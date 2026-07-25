use serde::Serialize;
use sqlx::Row;
use std::collections::HashSet;
use std::fs;
use std::path::{Component, Path, PathBuf};
use tauri::State;
use uuid::Uuid;

use crate::db::{self, DbPool};
use crate::path_utils::{
    default_central_skills_dir, expand_home_path, legacy_central_skills_dir, path_to_string,
    universal_skills_dir,
};
use crate::AppState;

const MIGRATION_PENDING: &str = "pending";
const MIGRATION_DEFERRED: &str = "deferred";
const MIGRATION_COMPLETED: &str = "completed";

#[derive(Debug, Clone, Serialize)]
pub struct CentralVaultStatus {
    pub central_path: String,
    pub default_central_path: String,
    pub legacy_path: String,
    pub universal_path: String,
    pub migration_state: String,
    pub migration_required: bool,
    pub legacy_skill_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct StoragePreview {
    pub current_path: String,
    pub new_path: String,
    pub skill_count: usize,
    pub conflicts: Vec<String>,
    pub can_proceed: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageChangeResult {
    pub central_path: String,
    pub skill_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SkillLocation {
    id: String,
    relative_dir: PathBuf,
}

#[derive(Debug, Clone)]
struct ManagedLink {
    path: PathBuf,
    new_target: PathBuf,
}

#[derive(Debug)]
struct ReplacedLink {
    path: PathBuf,
    old_target: PathBuf,
}

struct AppliedStorageMove {
    source: PathBuf,
    destination: PathBuf,
    backup: PathBuf,
    destination_was_empty: bool,
    recreated_universal: Option<PathBuf>,
    replaced_links: Vec<ReplacedLink>,
}

impl AppliedStorageMove {
    fn rollback(mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        for link in self.replaced_links.drain(..).rev() {
            if let Err(error) = remove_path(&link.path) {
                errors.push(error);
            }
            if let Err(error) = create_directory_symlink(&link.old_target, &link.path) {
                errors.push(error);
            }
        }
        if let Some(universal) = &self.recreated_universal {
            if let Err(error) = remove_path(universal) {
                errors.push(error);
            }
        }
        if let Err(error) = remove_path(&self.destination) {
            errors.push(error);
        }
        if let Err(error) = fs::rename(&self.backup, &self.source) {
            errors.push(format!("Failed to restore source backup: {error}"));
        }
        if self.destination_was_empty {
            if let Err(error) = fs::create_dir_all(&self.destination) {
                errors.push(format!("Failed to restore empty destination: {error}"));
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }

    fn commit(self) {
        let _ = remove_path(&self.backup);
    }
}

fn error_with_rollback(error: String, rollback: Result<(), String>) -> String {
    match rollback {
        Ok(()) => error,
        Err(rollback_error) => format!("{error}\nRollback also failed:\n{rollback_error}"),
    }
}

fn normalized_absolute(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!("Path must be absolute: '{}'", path.display()));
    }

    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}

fn comparable_path(path: &Path) -> Result<PathBuf, String> {
    let normalized = normalized_absolute(path)?;
    let mut existing = normalized.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        let name = existing
            .file_name()
            .ok_or_else(|| format!("Cannot resolve path '{}'", path.display()))?;
        missing.push(name.to_os_string());
        existing = existing
            .parent()
            .ok_or_else(|| format!("Cannot resolve path '{}'", path.display()))?;
    }
    let mut resolved = fs::canonicalize(existing)
        .map_err(|e| format!("Failed to resolve '{}': {e}", existing.display()))?;
    for component in missing.into_iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn paths_overlap(left: &Path, right: &Path) -> Result<bool, String> {
    let left = comparable_path(left)?;
    let right = comparable_path(right)?;
    Ok(left.starts_with(&right) || right.starts_with(&left))
}

fn directory_is_empty(path: &Path) -> Result<bool, String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => return Err(format!("Failed to inspect '{}': {error}", path.display())),
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Ok(false);
    }
    Ok(fs::read_dir(path)
        .map_err(|e| format!("Failed to read '{}': {e}", path.display()))?
        .next()
        .is_none())
}

fn normalized_skill_id(path: &Path) -> Result<String, String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_lowercase().replace(' ', "-"))
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("Invalid skill directory name: '{}'", path.display()))
}

fn collect_skill_locations(root: &Path) -> Result<Vec<SkillLocation>, String> {
    fn visit(
        root: &Path,
        current: &Path,
        active_dirs: &mut HashSet<PathBuf>,
        skills: &mut Vec<SkillLocation>,
    ) -> Result<(), String> {
        let metadata = fs::metadata(current)
            .map_err(|e| format!("Failed to inspect '{}': {e}", current.display()))?;
        if !metadata.is_dir() {
            return Ok(());
        }

        let canonical = fs::canonicalize(current)
            .map_err(|e| format!("Failed to resolve '{}': {e}", current.display()))?;
        if !active_dirs.insert(canonical.clone()) {
            return Err(format!("Symlink cycle detected at '{}'", current.display()));
        }

        if current != root
            && crate::commands::scanner::parse_skill_md(&current.join("SKILL.md")).is_some()
        {
            skills.push(SkillLocation {
                id: normalized_skill_id(current)?,
                relative_dir: current
                    .strip_prefix(root)
                    .map_err(|e| e.to_string())?
                    .to_path_buf(),
            });
            active_dirs.remove(&canonical);
            return Ok(());
        }

        let mut entries = fs::read_dir(current)
            .map_err(|e| format!("Failed to read '{}': {e}", current.display()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("Failed to read '{}': {e}", current.display()))?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            match fs::metadata(&path) {
                Ok(metadata) if metadata.is_dir() => {
                    visit(root, &path, active_dirs, skills)?;
                }
                Ok(_) => {}
                Err(error) => {
                    return Err(format!("Failed to inspect '{}': {error}", path.display()));
                }
            }
        }

        active_dirs.remove(&canonical);
        Ok(())
    }

    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut skills = Vec::new();
    visit(root, root, &mut HashSet::new(), &mut skills)?;
    skills.sort_by(|left, right| left.relative_dir.cmp(&right.relative_dir));
    Ok(skills)
}

fn duplicate_skill_conflicts(skills: &[SkillLocation]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut duplicates = HashSet::new();
    for skill in skills {
        if !seen.insert(&skill.id) {
            duplicates.insert(&skill.id);
        }
    }
    let mut duplicates = duplicates.into_iter().collect::<Vec<_>>();
    duplicates.sort();
    duplicates
        .into_iter()
        .map(|id| format!("Duplicate skill ID '{id}' cannot be flattened into Universal"))
        .collect()
}

fn preview_storage_move_impl(
    source: &Path,
    destination: &Path,
    universal: &Path,
) -> Result<(StoragePreview, Vec<SkillLocation>), String> {
    let source = normalized_absolute(source)?;
    let destination = normalized_absolute(destination)?;
    let universal = normalized_absolute(universal)?;
    let (skills, mut conflicts) = match collect_skill_locations(&source) {
        Ok(skills) => {
            let conflicts = duplicate_skill_conflicts(&skills);
            (skills, conflicts)
        }
        Err(error) => (Vec::new(), vec![error]),
    };

    if !source.is_dir() {
        conflicts.push(format!(
            "Current vault does not exist: '{}'",
            source.display()
        ));
    }

    if paths_overlap(&source, &destination)? {
        conflicts.push("Current and new vault paths must not overlap".to_string());
    }
    if paths_overlap(&destination, &universal)? {
        conflicts.push("Vault and Universal paths must not overlap".to_string());
    }
    if !directory_is_empty(&destination)? {
        conflicts.push(format!(
            "Destination is not empty: '{}'",
            destination.display()
        ));
    }

    let preview = StoragePreview {
        current_path: path_to_string(&source),
        new_path: path_to_string(&destination),
        skill_count: skills.len(),
        can_proceed: conflicts.is_empty(),
        conflicts,
    };
    Ok((preview, skills))
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fn copy_entry(
        source: &Path,
        destination: &Path,
        active_dirs: &mut HashSet<PathBuf>,
    ) -> Result<(), String> {
        let link_metadata = fs::symlink_metadata(source)
            .map_err(|e| format!("Failed to inspect '{}': {e}", source.display()))?;
        if link_metadata.file_type().is_symlink() {
            let target = fs::canonicalize(source)
                .map_err(|e| format!("Failed to resolve symlink '{}': {e}", source.display()))?;
            return copy_entry(&target, destination, active_dirs);
        }
        if link_metadata.is_dir() {
            let canonical = fs::canonicalize(source)
                .map_err(|e| format!("Failed to resolve '{}': {e}", source.display()))?;
            if !active_dirs.insert(canonical.clone()) {
                return Err(format!("Symlink cycle detected at '{}'", source.display()));
            }
            fs::create_dir_all(destination)
                .map_err(|e| format!("Failed to create '{}': {e}", destination.display()))?;
            let mut entries = fs::read_dir(source)
                .map_err(|e| format!("Failed to read '{}': {e}", source.display()))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| format!("Failed to read '{}': {e}", source.display()))?;
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                copy_entry(
                    &entry.path(),
                    &destination.join(entry.file_name()),
                    active_dirs,
                )?;
            }
            active_dirs.remove(&canonical);
            return Ok(());
        }
        if link_metadata.is_file() {
            fs::copy(source, destination).map_err(|e| {
                format!(
                    "Failed to copy '{}' to '{}': {e}",
                    source.display(),
                    destination.display()
                )
            })?;
            return Ok(());
        }
        Err(format!("Unsupported file type: '{}'", source.display()))
    }

    copy_entry(source, destination, &mut HashSet::new())
}

#[cfg(unix)]
fn create_directory_symlink(target: &Path, link: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link).map_err(|e| {
        format!(
            "Failed to create symlink '{}' -> '{}': {e}",
            link.display(),
            target.display()
        )
    })
}

#[cfg(windows)]
fn create_directory_symlink(target: &Path, link: &Path) -> Result<(), String> {
    std::os::windows::fs::symlink_dir(target, link).map_err(|e| {
        format!(
            "Failed to create symlink '{}' -> '{}': {e}",
            link.display(),
            target.display()
        )
    })
}

fn remove_path(path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("Failed to inspect '{}': {error}", path.display())),
    };
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path).map_err(|e| format!("Failed to remove '{}': {e}", path.display()))
    } else {
        fs::remove_dir_all(path).map_err(|e| format!("Failed to remove '{}': {e}", path.display()))
    }
}

fn temporary_sibling(path: &Path, purpose: &str) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("Path has no parent: '{}'", path.display()))?;
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("skills");
    Ok(parent.join(format!(".{name}.skillsmanage-{purpose}-{}", Uuid::new_v4())))
}

fn apply_storage_move_impl(
    source: &Path,
    destination: &Path,
    universal: &Path,
    skills: &[SkillLocation],
    recreate_universal: bool,
    managed_links: &[ManagedLink],
) -> Result<AppliedStorageMove, String> {
    let stage = temporary_sibling(destination, "stage")?;
    let backup = temporary_sibling(source, "backup")?;
    let destination_was_empty = destination.exists();
    fs::create_dir_all(
        destination
            .parent()
            .ok_or_else(|| "Destination has no parent".to_string())?,
    )
    .map_err(|e| format!("Failed to create destination parent: {e}"))?;

    if let Err(error) = copy_tree(source, &stage) {
        let _ = remove_path(&stage);
        return Err(error);
    }
    let staged_skills = match collect_skill_locations(&stage) {
        Ok(skills) => skills,
        Err(error) => {
            let _ = remove_path(&stage);
            return Err(error);
        }
    };
    if staged_skills != skills {
        let _ = remove_path(&stage);
        return Err("Staged vault verification failed".to_string());
    }

    if destination_was_empty {
        fs::remove_dir(destination)
            .map_err(|e| format!("Failed to prepare empty destination: {e}"))?;
    }
    if let Err(error) = fs::rename(source, &backup) {
        let _ = remove_path(&stage);
        if destination_was_empty {
            let _ = fs::create_dir_all(destination);
        }
        return Err(format!("Failed to create source backup: {error}"));
    }
    if let Err(error) = fs::rename(&stage, destination) {
        let _ = fs::rename(&backup, source);
        if destination_was_empty {
            let _ = fs::create_dir_all(destination);
        }
        let _ = remove_path(&stage);
        return Err(format!("Failed to activate new vault: {error}"));
    }

    let mut applied = AppliedStorageMove {
        source: source.to_path_buf(),
        destination: destination.to_path_buf(),
        backup,
        destination_was_empty,
        recreated_universal: recreate_universal.then(|| universal.to_path_buf()),
        replaced_links: Vec::new(),
    };

    let link_result = (|| {
        if recreate_universal {
            fs::create_dir_all(universal)
                .map_err(|e| format!("Failed to recreate Universal directory: {e}"))?;
            for skill in skills {
                create_directory_symlink(
                    &destination.join(&skill.relative_dir),
                    &universal.join(&skill.id),
                )?;
            }
        }
        for managed in managed_links {
            let old_target = fs::read_link(&managed.path).map_err(|e| {
                format!(
                    "Failed to read managed symlink '{}': {e}",
                    managed.path.display()
                )
            })?;
            fs::remove_file(&managed.path).map_err(|e| {
                format!(
                    "Failed to replace managed symlink '{}': {e}",
                    managed.path.display()
                )
            })?;
            applied.replaced_links.push(ReplacedLink {
                path: managed.path.clone(),
                old_target,
            });
            create_directory_symlink(&managed.new_target, &managed.path)?;
        }
        Ok(())
    })();

    if let Err(error) = link_result {
        let rollback = applied.rollback();
        return Err(error_with_rollback(error, rollback));
    }
    Ok(applied)
}

fn replace_path_prefix(value: &str, old_root: &Path, new_root: &Path) -> Option<String> {
    let path = Path::new(value);
    path.strip_prefix(old_root)
        .ok()
        .map(|relative| path_to_string(&new_root.join(relative)))
}

async fn managed_links_for_move(
    pool: &DbPool,
    old_root: &Path,
    new_root: &Path,
) -> Result<(Vec<ManagedLink>, Vec<String>), String> {
    let rows = sqlx::query(
        "SELECT installed_path, symlink_target FROM skill_installations
         WHERE link_type = 'symlink'",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let mut links = Vec::new();
    let mut conflicts = Vec::new();
    let mut seen_links = HashSet::new();
    for row in rows {
        let installed_path = PathBuf::from(row.get::<String, _>("installed_path"));
        if installed_path.starts_with(old_root) {
            continue;
        }
        match fs::symlink_metadata(&installed_path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let raw_target = fs::read_link(&installed_path).map_err(|e| e.to_string())?;
                let resolved_target = if raw_target.is_absolute() {
                    raw_target
                } else {
                    installed_path
                        .parent()
                        .ok_or_else(|| "Managed symlink has no parent".to_string())?
                        .join(raw_target)
                };
                let old_root = comparable_path(old_root)?;
                let resolved_target = comparable_path(&resolved_target)?;
                if let Ok(relative) = resolved_target.strip_prefix(old_root) {
                    if !seen_links.insert(installed_path.clone()) {
                        continue;
                    }
                    links.push(ManagedLink {
                        path: installed_path,
                        new_target: new_root.join(relative),
                    });
                }
            }
            Ok(_) => {
                let recorded_target = row.get::<Option<String>, _>("symlink_target");
                if recorded_target
                    .as_deref()
                    .and_then(|target| replace_path_prefix(target, old_root, new_root))
                    .is_some()
                {
                    conflicts.push(format!(
                        "Managed install is no longer a symlink: '{}'",
                        installed_path.display()
                    ));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok((links, conflicts))
}

async fn update_database_paths(
    pool: &DbPool,
    old_root: &Path,
    new_root: &Path,
    universal_root: &Path,
    skill_locations: &[SkillLocation],
    recreate_universal: bool,
    managed_links: &[ManagedLink],
) -> Result<(), String> {
    let old_path = path_to_string(old_root);
    let new_path = path_to_string(new_root);
    let mut transaction = pool.begin().await.map_err(|e| e.to_string())?;

    let skills =
        sqlx::query("SELECT id, file_path, canonical_path FROM skills WHERE is_central = 1")
            .fetch_all(&mut *transaction)
            .await
            .map_err(|e| e.to_string())?;
    for row in skills {
        let id = row.get::<String, _>("id");
        let file_path = row.get::<String, _>("file_path");
        let canonical_path = row.get::<Option<String>, _>("canonical_path");
        let new_file_path =
            replace_path_prefix(&file_path, old_root, new_root).unwrap_or(file_path);
        let new_canonical_path = canonical_path
            .map(|path| replace_path_prefix(&path, old_root, new_root).unwrap_or(path));
        sqlx::query("UPDATE skills SET file_path = ?, canonical_path = ? WHERE id = ?")
            .bind(new_file_path)
            .bind(new_canonical_path)
            .bind(id)
            .execute(&mut *transaction)
            .await
            .map_err(|e| e.to_string())?;
    }

    let installations = sqlx::query(
        "SELECT skill_id, agent_id, installed_path, symlink_target
         FROM skill_installations",
    )
    .fetch_all(&mut *transaction)
    .await
    .map_err(|e| e.to_string())?;
    for row in installations {
        let skill_id = row.get::<String, _>("skill_id");
        let agent_id = row.get::<String, _>("agent_id");
        let installed_path = row.get::<String, _>("installed_path");
        if agent_id == "central" || Path::new(&installed_path).starts_with(old_root) {
            sqlx::query("DELETE FROM skill_installations WHERE skill_id = ? AND agent_id = ?")
                .bind(skill_id)
                .bind(agent_id)
                .execute(&mut *transaction)
                .await
                .map_err(|e| e.to_string())?;
            continue;
        }
        let Some(target) = row.get::<Option<String>, _>("symlink_target") else {
            continue;
        };
        let Some(new_target) = replace_path_prefix(&target, old_root, new_root) else {
            continue;
        };
        sqlx::query(
            "UPDATE skill_installations SET symlink_target = ?
             WHERE skill_id = ? AND agent_id = ?",
        )
        .bind(new_target)
        .bind(skill_id)
        .bind(agent_id)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    }
    for managed in managed_links {
        sqlx::query(
            "UPDATE skill_installations SET symlink_target = ?
             WHERE installed_path = ? AND link_type = 'symlink'",
        )
        .bind(path_to_string(&managed.new_target))
        .bind(path_to_string(&managed.path))
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    }

    let now = chrono::Utc::now().to_rfc3339();
    for skill in skill_locations {
        let canonical_path = new_root.join(&skill.relative_dir);
        let skill_md_path = canonical_path.join("SKILL.md");
        let info = crate::commands::scanner::parse_skill_md(&skill_md_path).ok_or_else(|| {
            format!(
                "Migrated skill metadata is invalid: '{}'",
                skill_md_path.display()
            )
        })?;
        sqlx::query(
            "INSERT INTO skills
             (id, name, description, file_path, canonical_path, is_central, source, content, scanned_at)
             VALUES (?, ?, ?, ?, ?, 1, 'native', NULL, ?)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name,
               description = excluded.description,
               file_path = excluded.file_path,
               canonical_path = excluded.canonical_path,
               is_central = 1,
               source = 'native',
               scanned_at = excluded.scanned_at",
        )
        .bind(&skill.id)
        .bind(info.name)
        .bind(info.description)
        .bind(path_to_string(&skill_md_path))
        .bind(path_to_string(&canonical_path))
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
        sqlx::query(
            "INSERT OR REPLACE INTO skill_installations
             (skill_id, agent_id, installed_path, link_type, symlink_target, created_at)
             VALUES (?, 'central', ?, 'native', NULL, ?)",
        )
        .bind(&skill.id)
        .bind(path_to_string(&canonical_path))
        .bind(&now)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;

        if recreate_universal {
            sqlx::query(
                "INSERT OR REPLACE INTO skill_installations
                 (skill_id, agent_id, installed_path, link_type, symlink_target, created_at)
                 VALUES (?, 'universal', ?, 'symlink', ?, ?)",
            )
            .bind(&skill.id)
            .bind(path_to_string(&universal_root.join(&skill.id)))
            .bind(path_to_string(&canonical_path))
            .bind(&now)
            .execute(&mut *transaction)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
        .bind(db::CENTRAL_SKILLS_PATH_SETTING)
        .bind(&new_path)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, ?)")
        .bind(db::CENTRAL_MIGRATION_STATE_SETTING)
        .bind(MIGRATION_COMPLETED)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'central'")
        .bind(&new_path)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query(
        "INSERT INTO scan_directories
         (path, label, is_active, is_builtin, added_at)
         VALUES (?, 'Skill Vault', 1, 1, ?)
         ON CONFLICT(path) DO UPDATE SET
           label = 'Skill Vault', is_active = 1, is_builtin = 1",
    )
    .bind(&new_path)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(&mut *transaction)
    .await
    .map_err(|e| e.to_string())?;
    if old_root != universal_root {
        sqlx::query(
            "DELETE FROM scan_directories WHERE path = ? AND is_builtin = 1
             AND NOT EXISTS (
               SELECT 1 FROM agents WHERE id != 'central' AND global_skills_dir = ?
             )",
        )
        .bind(&old_path)
        .bind(&old_path)
        .execute(&mut *transaction)
        .await
        .map_err(|e| e.to_string())?;
    }

    transaction.commit().await.map_err(|e| e.to_string())
}

pub async fn initialize_storage_impl(
    pool: &DbPool,
    default_path: &Path,
    legacy_path: &Path,
) -> Result<(), String> {
    if db::get_setting(pool, db::CENTRAL_MIGRATION_STATE_SETTING)
        .await?
        .is_some()
    {
        fs::create_dir_all(db::get_central_skills_dir(pool).await?)
            .map_err(|e| format!("Failed to create Central vault: {e}"))?;
        return Ok(());
    }

    let current = db::get_central_skills_dir(pool).await?;
    let legacy_scan = collect_skill_locations(legacy_path);
    let migration_required = legacy_scan
        .as_ref()
        .map(|skills| !skills.is_empty())
        .unwrap_or(true);
    if migration_required && (current == default_path || current == legacy_path) {
        db::set_setting(
            pool,
            db::CENTRAL_SKILLS_PATH_SETTING,
            &path_to_string(legacy_path),
        )
        .await?;
        db::set_setting(pool, db::CENTRAL_MIGRATION_STATE_SETTING, MIGRATION_PENDING).await?;
    } else {
        if current == legacy_path {
            db::set_setting(
                pool,
                db::CENTRAL_SKILLS_PATH_SETTING,
                &path_to_string(default_path),
            )
            .await?;
        }
        db::set_setting(
            pool,
            db::CENTRAL_MIGRATION_STATE_SETTING,
            MIGRATION_COMPLETED,
        )
        .await?;
        fs::create_dir_all(db::get_central_skills_dir(pool).await?)
            .map_err(|e| format!("Failed to create Central vault: {e}"))?;
    }
    Ok(())
}

pub async fn initialize_storage(pool: &DbPool) -> Result<(), String> {
    initialize_storage_impl(
        pool,
        &default_central_skills_dir(),
        &legacy_central_skills_dir(),
    )
    .await
}

async fn get_central_vault_status_impl(
    pool: &DbPool,
    default_path: &Path,
    legacy_path: &Path,
    universal_path: &Path,
) -> Result<CentralVaultStatus, String> {
    let migration_state = db::get_setting(pool, db::CENTRAL_MIGRATION_STATE_SETTING)
        .await?
        .unwrap_or_else(|| MIGRATION_COMPLETED.to_string());
    Ok(CentralVaultStatus {
        central_path: path_to_string(&db::get_central_skills_dir(pool).await?),
        default_central_path: path_to_string(default_path),
        legacy_path: path_to_string(legacy_path),
        universal_path: path_to_string(universal_path),
        migration_required: migration_state != MIGRATION_COMPLETED,
        migration_state,
        legacy_skill_count: collect_skill_locations(legacy_path)
            .map(|skills| skills.len())
            .unwrap_or(0),
    })
}

async fn preview_central_path_change_impl(
    pool: &DbPool,
    destination: &Path,
    legacy_path: &Path,
    universal_path: &Path,
) -> Result<StoragePreview, String> {
    let current = db::get_central_skills_dir(pool).await?;
    let (mut preview, _) = preview_storage_move_impl(&current, destination, universal_path)?;
    let (_, link_conflicts) = managed_links_for_move(pool, &current, destination).await?;
    preview.conflicts.extend(link_conflicts);
    let migration_state = db::get_setting(pool, db::CENTRAL_MIGRATION_STATE_SETTING)
        .await?
        .unwrap_or_else(|| MIGRATION_COMPLETED.to_string());
    if migration_state != MIGRATION_COMPLETED && current != legacy_path {
        preview
            .conflicts
            .push("Pending migration source is not the legacy Central path".to_string());
    }
    if current == legacy_path && current != universal_path {
        preview
            .conflicts
            .push("Legacy and Universal paths are inconsistent".to_string());
    }
    preview.can_proceed = preview.conflicts.is_empty();
    Ok(preview)
}

async fn change_central_path_impl(
    pool: &DbPool,
    destination: &Path,
    legacy_path: &Path,
    universal_path: &Path,
) -> Result<StorageChangeResult, String> {
    let current = db::get_central_skills_dir(pool).await?;
    let (mut preview, skills) = preview_storage_move_impl(&current, destination, universal_path)?;
    let (managed_links, link_conflicts) =
        managed_links_for_move(pool, &current, destination).await?;
    preview.conflicts.extend(link_conflicts);
    if !preview.conflicts.is_empty() {
        return Err(preview.conflicts.join("\n"));
    }

    let migration_state = db::get_setting(pool, db::CENTRAL_MIGRATION_STATE_SETTING)
        .await?
        .unwrap_or_else(|| MIGRATION_COMPLETED.to_string());
    if migration_state != MIGRATION_COMPLETED && current != legacy_path {
        return Err("Pending migration source is not the legacy Central path".to_string());
    }
    let recreate_universal = migration_state != MIGRATION_COMPLETED && current == legacy_path;
    let applied = apply_storage_move_impl(
        &current,
        destination,
        universal_path,
        &skills,
        recreate_universal,
        &managed_links,
    )?;
    if let Err(error) = update_database_paths(
        pool,
        &current,
        destination,
        universal_path,
        &skills,
        recreate_universal,
        &managed_links,
    )
    .await
    {
        let rollback = applied.rollback();
        return Err(error_with_rollback(error, rollback));
    }
    applied.commit();
    Ok(StorageChangeResult {
        central_path: path_to_string(destination),
        skill_count: skills.len(),
    })
}

#[tauri::command]
pub async fn get_central_vault_status(
    state: State<'_, AppState>,
) -> Result<CentralVaultStatus, String> {
    get_central_vault_status_impl(
        &state.db,
        &default_central_skills_dir(),
        &legacy_central_skills_dir(),
        &universal_skills_dir(),
    )
    .await
}

#[tauri::command]
pub async fn preview_legacy_migration(
    state: State<'_, AppState>,
) -> Result<StoragePreview, String> {
    if db::get_setting(&state.db, db::CENTRAL_MIGRATION_STATE_SETTING)
        .await?
        .as_deref()
        == Some(MIGRATION_COMPLETED)
    {
        return Err("Legacy migration is not required".to_string());
    }
    preview_central_path_change_impl(
        &state.db,
        &default_central_skills_dir(),
        &legacy_central_skills_dir(),
        &universal_skills_dir(),
    )
    .await
}

#[tauri::command]
pub async fn defer_legacy_migration(state: State<'_, AppState>) -> Result<(), String> {
    if db::get_setting(&state.db, db::CENTRAL_MIGRATION_STATE_SETTING)
        .await?
        .as_deref()
        == Some(MIGRATION_COMPLETED)
    {
        return Ok(());
    }
    db::set_setting(
        &state.db,
        db::CENTRAL_MIGRATION_STATE_SETTING,
        MIGRATION_DEFERRED,
    )
    .await
}

#[tauri::command]
pub async fn migrate_legacy_central(
    state: State<'_, AppState>,
) -> Result<StorageChangeResult, String> {
    if db::get_setting(&state.db, db::CENTRAL_MIGRATION_STATE_SETTING)
        .await?
        .as_deref()
        == Some(MIGRATION_COMPLETED)
    {
        return Err("Legacy migration is not required".to_string());
    }
    change_central_path_impl(
        &state.db,
        &default_central_skills_dir(),
        &legacy_central_skills_dir(),
        &universal_skills_dir(),
    )
    .await
}

#[tauri::command]
pub async fn preview_central_path_change(
    state: State<'_, AppState>,
    new_path: String,
) -> Result<StoragePreview, String> {
    let destination = expand_home_path(new_path.trim());
    preview_central_path_change_impl(
        &state.db,
        &destination,
        &legacy_central_skills_dir(),
        &universal_skills_dir(),
    )
    .await
}

#[tauri::command]
pub async fn change_central_path(
    state: State<'_, AppState>,
    new_path: String,
) -> Result<StorageChangeResult, String> {
    let destination = expand_home_path(new_path.trim());
    change_central_path_impl(
        &state.db,
        &destination,
        &legacy_central_skills_dir(),
        &universal_skills_dir(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_skill(path: &Path, name: &str) {
        fs::create_dir_all(path).unwrap();
        fs::write(
            path.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: test\n---\n"),
        )
        .unwrap();
    }

    #[test]
    fn conflict_preview_does_not_change_files() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("legacy");
        let destination = temp.path().join("vault");
        let universal = temp.path().join("universal");
        write_skill(&source.join("group/my-skill"), "My Skill");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("occupied"), "keep").unwrap();

        let (preview, _) = preview_storage_move_impl(&source, &destination, &universal).unwrap();

        assert!(!preview.can_proceed);
        assert!(source.join("group/my-skill/SKILL.md").exists());
        assert_eq!(
            fs::read_to_string(destination.join("occupied")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn skill_assets_are_not_mistaken_for_nested_skills() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("vault");
        write_skill(&source.join("parent"), "Parent");
        write_skill(&source.join("parent/examples/child"), "Example Child");

        let skills = collect_skill_locations(&source).unwrap();

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].id, "parent");
    }

    #[cfg(unix)]
    #[test]
    fn migration_preserves_nesting_dereferences_source_links_and_rolls_back() {
        let temp = tempdir().unwrap();
        let legacy = temp.path().join(".agents/skills");
        let destination = temp.path().join(".skillsmanage/skills");
        let external = temp.path().join("external/my-skill");
        write_skill(&external, "My Skill");
        fs::create_dir_all(legacy.join("bundle")).unwrap();
        std::os::unix::fs::symlink(&external, legacy.join("bundle/my-skill")).unwrap();

        let (preview, skills) = preview_storage_move_impl(&legacy, &destination, &legacy).unwrap();
        assert!(preview.can_proceed);
        let applied =
            apply_storage_move_impl(&legacy, &destination, &legacy, &skills, true, &[]).unwrap();

        assert!(destination.join("bundle/my-skill/SKILL.md").is_file());
        assert!(!fs::symlink_metadata(destination.join("bundle/my-skill"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::canonicalize(legacy.join("my-skill")).unwrap(),
            fs::canonicalize(destination.join("bundle/my-skill")).unwrap()
        );
        assert!(external.join("SKILL.md").exists());

        applied.rollback().unwrap();
        assert!(fs::symlink_metadata(legacy.join("bundle/my-skill"))
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(!destination.exists());
        assert!(external.join("SKILL.md").exists());
    }

    #[tokio::test]
    async fn initialize_preserves_legacy_until_migration() {
        let temp = tempdir().unwrap();
        let default_path = temp.path().join(".skillsmanage/skills");
        let legacy = temp.path().join(".agents/skills");
        write_skill(&legacy.join("old-skill"), "Old Skill");
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();
        db::set_setting(
            &pool,
            db::CENTRAL_SKILLS_PATH_SETTING,
            &path_to_string(&default_path),
        )
        .await
        .unwrap();

        initialize_storage_impl(&pool, &default_path, &legacy)
            .await
            .unwrap();

        assert_eq!(db::get_central_skills_dir(&pool).await.unwrap(), legacy);
        assert_eq!(
            db::get_setting(&pool, db::CENTRAL_MIGRATION_STATE_SETTING)
                .await
                .unwrap()
                .as_deref(),
            Some(MIGRATION_PENDING)
        );
        assert!(!default_path.exists());
    }

    #[tokio::test]
    async fn path_change_updates_setting_agent_and_canonical_paths() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("vault-a");
        let destination = temp.path().join("vault-b");
        let legacy = temp.path().join("legacy");
        let universal = temp.path().join("universal");
        write_skill(&source.join("nested/demo"), "Demo");
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();
        db::set_setting(
            &pool,
            db::CENTRAL_SKILLS_PATH_SETTING,
            &path_to_string(&source),
        )
        .await
        .unwrap();
        db::set_setting(
            &pool,
            db::CENTRAL_MIGRATION_STATE_SETTING,
            MIGRATION_COMPLETED,
        )
        .await
        .unwrap();
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "demo".to_string(),
                name: "Demo".to_string(),
                description: None,
                file_path: path_to_string(&source.join("nested/demo/SKILL.md")),
                canonical_path: Some(path_to_string(&source.join("nested/demo"))),
                is_central: true,
                source: None,
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let result = change_central_path_impl(&pool, &destination, &legacy, &universal)
            .await
            .unwrap();

        assert_eq!(result.skill_count, 1);
        assert_eq!(
            db::get_central_skills_dir(&pool).await.unwrap(),
            destination
        );
        let skill = db::get_skill_by_id(&pool, "demo").await.unwrap().unwrap();
        assert_eq!(
            skill.canonical_path.as_deref(),
            Some(path_to_string(&destination.join("nested/demo")).as_str())
        );
        assert!(destination.join("nested/demo/SKILL.md").exists());
        assert!(!source.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn path_change_retargets_relative_managed_symlinks() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("vault-a");
        let destination = temp.path().join("vault-b");
        let legacy = temp.path().join("legacy");
        let universal = temp.path().join("universal");
        write_skill(&source.join("demo"), "Demo");
        fs::create_dir_all(&universal).unwrap();
        std::os::unix::fs::symlink("../vault-a/demo", universal.join("demo")).unwrap();
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();
        db::set_setting(
            &pool,
            db::CENTRAL_SKILLS_PATH_SETTING,
            &path_to_string(&source),
        )
        .await
        .unwrap();
        db::set_setting(
            &pool,
            db::CENTRAL_MIGRATION_STATE_SETTING,
            MIGRATION_COMPLETED,
        )
        .await
        .unwrap();
        db::upsert_skill_installation(
            &pool,
            &db::SkillInstallation {
                skill_id: "demo".to_string(),
                agent_id: "universal".to_string(),
                installed_path: path_to_string(&universal.join("demo")),
                link_type: "symlink".to_string(),
                symlink_target: Some("../vault-a/demo".to_string()),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        change_central_path_impl(&pool, &destination, &legacy, &universal)
            .await
            .unwrap();

        assert_eq!(
            fs::canonicalize(universal.join("demo")).unwrap(),
            fs::canonicalize(destination.join("demo")).unwrap()
        );
        let installation = db::get_skill_installations(&pool, "demo")
            .await
            .unwrap()
            .into_iter()
            .find(|installation| installation.agent_id == "universal")
            .unwrap();
        assert_eq!(
            installation.symlink_target.as_deref(),
            Some(path_to_string(&destination.join("demo")).as_str())
        );
    }

    #[tokio::test]
    async fn legacy_migration_rebuilds_central_and_universal_installations() {
        let temp = tempdir().unwrap();
        let legacy = temp.path().join(".agents/skills");
        let destination = temp.path().join(".skillsmanage/skills");
        write_skill(&legacy.join("nested/demo"), "Demo");
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();
        db::set_setting(
            &pool,
            db::CENTRAL_SKILLS_PATH_SETTING,
            &path_to_string(&legacy),
        )
        .await
        .unwrap();
        db::set_setting(
            &pool,
            db::CENTRAL_MIGRATION_STATE_SETTING,
            MIGRATION_PENDING,
        )
        .await
        .unwrap();
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "demo".to_string(),
                name: "Demo".to_string(),
                description: None,
                file_path: path_to_string(&legacy.join("nested/demo/SKILL.md")),
                canonical_path: Some(path_to_string(&legacy.join("nested/demo"))),
                is_central: true,
                source: None,
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        db::upsert_skill_installation(
            &pool,
            &db::SkillInstallation {
                skill_id: "demo".to_string(),
                agent_id: "central".to_string(),
                installed_path: path_to_string(&legacy.join("nested/demo")),
                link_type: "native".to_string(),
                symlink_target: None,
                created_at: now.clone(),
            },
        )
        .await
        .unwrap();
        db::upsert_skill_installation(
            &pool,
            &db::SkillInstallation {
                skill_id: "demo".to_string(),
                agent_id: "codex".to_string(),
                installed_path: path_to_string(&legacy.join("nested/demo")),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: now,
            },
        )
        .await
        .unwrap();

        change_central_path_impl(&pool, &destination, &legacy, &legacy)
            .await
            .unwrap();

        let installations = db::get_skill_installations(&pool, "demo").await.unwrap();
        assert_eq!(installations.len(), 2);
        let central = installations
            .iter()
            .find(|installation| installation.agent_id == "central")
            .unwrap();
        assert_eq!(
            central.installed_path,
            path_to_string(&destination.join("nested/demo"))
        );
        assert_eq!(central.link_type, "native");
        let universal = installations
            .iter()
            .find(|installation| installation.agent_id == "universal")
            .unwrap();
        assert_eq!(
            universal.installed_path,
            path_to_string(&legacy.join("demo"))
        );
        assert_eq!(
            universal.symlink_target.as_deref(),
            Some(path_to_string(&destination.join("nested/demo")).as_str())
        );
        assert_eq!(
            fs::canonicalize(&universal.installed_path).unwrap(),
            fs::canonicalize(destination.join("nested/demo")).unwrap()
        );
    }

    #[tokio::test]
    async fn database_failure_rolls_back_files_and_rows() {
        let temp = tempdir().unwrap();
        let source = temp.path().join("vault-a");
        let destination = temp.path().join("vault-b");
        let legacy = temp.path().join("legacy");
        let universal = temp.path().join("universal");
        write_skill(&source.join("demo"), "Demo");
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();
        db::set_setting(
            &pool,
            db::CENTRAL_SKILLS_PATH_SETTING,
            &path_to_string(&source),
        )
        .await
        .unwrap();
        db::set_setting(
            &pool,
            db::CENTRAL_MIGRATION_STATE_SETTING,
            MIGRATION_COMPLETED,
        )
        .await
        .unwrap();
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "demo".to_string(),
                name: "Demo".to_string(),
                description: None,
                file_path: path_to_string(&source.join("demo/SKILL.md")),
                canonical_path: Some(path_to_string(&source.join("demo"))),
                is_central: true,
                source: None,
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        db::upsert_skill_installation(
            &pool,
            &db::SkillInstallation {
                skill_id: "demo".to_string(),
                agent_id: "central".to_string(),
                installed_path: path_to_string(&source.join("demo")),
                link_type: "native".to_string(),
                symlink_target: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER fail_central_agent_update
             BEFORE UPDATE ON agents WHEN NEW.id = 'central'
             BEGIN SELECT RAISE(ABORT, 'forced failure'); END",
        )
        .execute(&pool)
        .await
        .unwrap();

        let result = change_central_path_impl(&pool, &destination, &legacy, &universal).await;

        assert!(result.is_err());
        assert!(source.join("demo/SKILL.md").exists());
        assert!(!destination.exists());
        assert_eq!(db::get_central_skills_dir(&pool).await.unwrap(), source);
        let installations = db::get_skill_installations(&pool, "demo").await.unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(
            installations[0].installed_path,
            path_to_string(&source.join("demo"))
        );
    }
}

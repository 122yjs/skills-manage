use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::db::{self, DbPool, SkillInstallation};
use crate::AppState;

// ─── Types ────────────────────────────────────────────────────────────────────

/// Result of a single skill install operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallResult {
    pub symlink_path: String,
}

/// Result of a batch install across multiple agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchInstallResult {
    pub succeeded: Vec<String>,
    pub failed: Vec<FailedInstall>,
}

/// Describes a single failed install within a batch operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailedInstall {
    pub agent_id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillBundleInstallResult {
    pub imported: Vec<String>,
    pub skipped: Vec<String>,
    pub succeeded: Vec<String>,
    pub failed: Vec<FailedInstall>,
}

// ─── Path Utilities ───────────────────────────────────────────────────────────

/// Compute a relative path from `from_dir` to `to_path`.
///
/// Both paths must be absolute. The resulting path can be used as a symlink
/// target placed inside `from_dir`.
///
/// Examples:
/// - `make_relative_path("/a/b/c", "/a/d/e/f")` -> `"../../d/e/f"`
/// - `make_relative_path("/home/user/.claude/skills", "/home/user/.agents/skills/my-skill")`
///   -> `"../../.agents/skills/my-skill"`
pub fn make_relative_path(from_dir: &Path, to_path: &Path) -> PathBuf {
    let from_components: Vec<_> = from_dir.components().collect();
    let to_components: Vec<_> = to_path.components().collect();

    // Find the length of the common path prefix.
    let common_len = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    // Number of ".." hops needed to climb out of `from_dir`.
    let up_count = from_components.len() - common_len;

    let mut result = PathBuf::new();
    for _ in 0..up_count {
        result.push("..");
    }
    for component in &to_components[common_len..] {
        result.push(component.as_os_str());
    }

    if result.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        result
    }
}

// ─── Platform-specific symlink creation ──────────────────────────────────────

#[cfg(unix)]
pub fn create_symlink(target: &Path, link: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target, link).map_err(|e| format!("Failed to create symlink: {}", e))
}

#[cfg(windows)]
pub fn create_symlink(target: &Path, link: &Path) -> Result<(), String> {
    std::os::windows::fs::symlink_dir(target, link)
        .map_err(|e| format!("Failed to create symlink: {}", e))
}

#[cfg(not(any(unix, windows)))]
pub fn create_symlink(_target: &Path, _link: &Path) -> Result<(), String> {
    Err("Symlink creation is only supported on Unix systems".to_string())
}

pub fn symlink_target_path(from_dir: &Path, to_path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let from_prefix = from_dir.components().next();
        let to_prefix = to_path.components().next();
        if from_prefix != to_prefix {
            return to_path.to_path_buf();
        }
    }

    make_relative_path(from_dir, to_path)
}

// ─── Recursive Directory Copy ─────────────────────────────────────────────────

/// Recursively copy a directory tree from `src` to `dst`.
///
/// `dst` must not exist prior to the call (or may be an empty dir).
/// The behaviour mirrors `cp -r src dst` on Unix.
pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| {
        format!(
            "Failed to create destination directory '{}': {}",
            dst.display(),
            e
        )
    })?;

    for entry in std::fs::read_dir(src)
        .map_err(|e| format!("Failed to read source directory '{}': {}", src.display(), e))?
    {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {}", e))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        let file_type = entry
            .file_type()
            .map_err(|e| format!("Failed to determine file type: {}", e))?;

        if file_type.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).map_err(|e| {
                format!(
                    "Failed to copy '{}' -> '{}': {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                )
            })?;
        }
    }

    Ok(())
}

// ─── Auto-centralize ─────────────────────────────────────────────────────────

/// Ensure the skill exists in the central directory. If it doesn't, copy it
/// from its actual location (looked up in the database) and update the DB
/// record to mark it as central.
///
/// This enables installing platform-specific skills to other platforms:
/// the skill is first adopted into the central directory, then distributed
/// via symlink/copy as usual.
async fn ensure_centralized(
    pool: &DbPool,
    skill_id: &str,
    canonical_dir: &Path,
) -> Result<(), String> {
    if canonical_dir.join("SKILL.md").exists() {
        return Ok(());
    }

    // Look up the skill's actual file location from the database.
    let skill = db::get_skill_by_id(pool, skill_id)
        .await?
        .ok_or_else(|| format!("Skill '{}' not found in database", skill_id))?;

    // Derive the source directory (parent of file_path).
    let source_file = PathBuf::from(&skill.file_path);
    let source_dir = source_file
        .parent()
        .ok_or_else(|| format!("Invalid file_path for skill '{}'", skill_id))?;

    if !source_file.exists() {
        return Err(format!(
            "Skill source not found at '{}'",
            source_file.display()
        ));
    }

    // Copy to central directory.
    copy_dir_all(source_dir, canonical_dir)?;

    // Update the DB record to reflect centralization.
    let mut updated = skill;
    updated.canonical_path = Some(canonical_dir.to_string_lossy().into_owned());
    updated.is_central = true;
    updated.file_path = canonical_dir
        .join("SKILL.md")
        .to_string_lossy()
        .into_owned();
    db::upsert_skill(pool, &updated).await?;

    Ok(())
}

async fn canonical_dir_for_skill(
    pool: &DbPool,
    skill_id: &str,
    central_root: &Path,
) -> Result<PathBuf, String> {
    if let Some(skill) = db::get_skill_by_id(pool, skill_id).await? {
        if let Some(canonical_path) = skill.canonical_path {
            let canonical_dir = PathBuf::from(canonical_path);
            if canonical_dir.join("SKILL.md").exists() {
                return Ok(canonical_dir);
            }
        }
    }

    Ok(central_root.join(skill_id))
}

async fn existing_installation_for_agent(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
) -> Result<Option<SkillInstallation>, String> {
    Ok(db::get_skill_installations(pool, skill_id)
        .await?
        .into_iter()
        .find(|installation| installation.agent_id == agent_id))
}

fn paths_refer_to_same_location(left: &Path, right: &Path) -> bool {
    left == right
        || left
            .canonicalize()
            .ok()
            .zip(right.canonicalize().ok())
            .is_some_and(|(left, right)| left == right)
}

fn agent_observes_universal(agent: &db::Agent, universal_root: &Path) -> bool {
    agent.id == "factory-droid"
        || db::agent_supports_universal_agents_skills(&agent.id)
        || paths_refer_to_same_location(Path::new(&agent.global_skills_dir), universal_root)
}

#[derive(Debug)]
struct DuplicateUniversalInstallation {
    installation: SkillInstallation,
    remove_path: bool,
    original_symlink_target: Option<PathBuf>,
}

fn resolved_symlink_target(link_path: &Path) -> Result<PathBuf, String> {
    let target = std::fs::read_link(link_path).map_err(|e| {
        format!(
            "Failed to read tracked symlink '{}': {}",
            link_path.display(),
            e
        )
    })?;
    if target.is_absolute() {
        Ok(target)
    } else {
        Ok(link_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(target))
    }
}

async fn preflight_universal_duplicates(
    pool: &DbPool,
    skill_id: &str,
    universal_root: &Path,
    universal_target: &Path,
    canonical_dir: &Path,
) -> Result<Vec<DuplicateUniversalInstallation>, String> {
    let agents = db::get_all_agents(pool).await?;
    let mut duplicates = Vec::new();

    for installation in db::get_skill_installations(pool, skill_id).await? {
        if installation.agent_id == "universal" {
            continue;
        }

        let agent = agents
            .iter()
            .find(|agent| agent.id == installation.agent_id);
        let is_compatible = agent
            .is_some_and(|agent| agent_observes_universal(agent, universal_root))
            || db::agent_supports_universal_agents_skills(&installation.agent_id);
        if !is_compatible {
            continue;
        }

        if installation.link_type != "symlink" {
            return Err(format!(
                "Skill '{}' already has a non-symlink installation for '{}'. Remove it before installing to Universal.",
                skill_id, installation.agent_id
            ));
        }

        let installed_path = PathBuf::from(&installation.installed_path);
        let original_symlink_target = match std::fs::symlink_metadata(&installed_path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let original_target = std::fs::read_link(&installed_path).map_err(|e| {
                    format!(
                        "Failed to read tracked symlink '{}': {}",
                        installed_path.display(),
                        e
                    )
                })?;
                let actual_target = resolved_symlink_target(&installed_path)?;
                if !paths_refer_to_same_location(&actual_target, canonical_dir) {
                    return Err(format!(
                        "Tracked symlink '{}' no longer points to the Central skill. Refusing to remove it.",
                        installed_path.display()
                    ));
                }
                Some(original_target)
            }
            Ok(_) => {
                return Err(format!(
                    "Path '{}' is not a symlink. Refusing to remove it while installing to Universal.",
                    installed_path.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => {
                return Err(format!(
                    "Failed to inspect existing installation '{}': {}",
                    installed_path.display(),
                    error
                ));
            }
        };

        duplicates.push(DuplicateUniversalInstallation {
            remove_path: !paths_refer_to_same_location(&installed_path, universal_target),
            installation,
            original_symlink_target,
        });
    }

    Ok(duplicates)
}

/// 공용 설치와 같은 물리 경로 또는 공용 호환 대상을 한 배치에서 중복 선택하지 못하게 한다.
/// 반환값은 선택 대상 중 실제 공용 설치로 처리될 대상이 있는지 뜻한다.
pub(crate) async fn validate_batch_install_targets(
    pool: &DbPool,
    agent_ids: &[String],
) -> Result<bool, String> {
    let agents = db::get_all_agents(pool).await?;
    let selected = agent_ids
        .iter()
        .filter_map(|id| agents.iter().find(|agent| agent.id == *id))
        .collect::<Vec<_>>();

    for (index, left) in selected.iter().enumerate() {
        for right in selected.iter().skip(index + 1) {
            if paths_refer_to_same_location(
                Path::new(&left.global_skills_dir),
                Path::new(&right.global_skills_dir),
            ) {
                return Err(format!(
                    "Agents '{}' and '{}' use the same install directory and cannot be selected together.",
                    left.id, right.id
                ));
            }
        }
    }

    let Some(universal) = agents.iter().find(|agent| agent.id == "universal") else {
        return Ok(false);
    };
    let universal_root = Path::new(&universal.global_skills_dir);
    let uses_universal = selected.iter().any(|agent| {
        agent.id == "universal"
            || paths_refer_to_same_location(Path::new(&agent.global_skills_dir), universal_root)
    });
    if !uses_universal {
        return Ok(false);
    }

    let conflicts = selected
        .iter()
        .filter(|agent| {
            agent.id != "universal"
                && !paths_refer_to_same_location(
                    Path::new(&agent.global_skills_dir),
                    universal_root,
                )
                && agent_observes_universal(agent, universal_root)
        })
        .map(|agent| agent.id.as_str())
        .collect::<Vec<_>>();
    if !conflicts.is_empty() {
        return Err(format!(
            "Universal and compatible platform targets cannot be selected together: {}",
            conflicts.join(", ")
        ));
    }

    Ok(true)
}

/// 배치 쓰기 전에 공용 대상의 관리되지 않은 파일과 기존 복사 설치 충돌을 확인한다.
pub(crate) async fn preflight_universal_install(
    pool: &DbPool,
    skill_id: &str,
) -> Result<(), String> {
    let central = db::get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central agent not found in database".to_string())?;
    let Some(universal) = db::get_agent_by_id(pool, "universal").await? else {
        return Ok(());
    };
    let central_root = Path::new(&central.global_skills_dir);
    let universal_root = Path::new(&universal.global_skills_dir);
    if paths_refer_to_same_location(central_root, universal_root) {
        return Ok(());
    }

    let canonical_dir = canonical_dir_for_skill(pool, skill_id, central_root).await?;
    let target_path = universal_root.join(skill_id);
    let duplicates = preflight_universal_duplicates(
        pool,
        skill_id,
        universal_root,
        &target_path,
        &canonical_dir,
    )
    .await?;
    let previous_installation =
        existing_installation_for_agent(pool, skill_id, "universal").await?;

    match std::fs::symlink_metadata(&target_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let tracked_alias = duplicates.iter().any(|duplicate| !duplicate.remove_path);
            if previous_installation.is_none() && !tracked_alias {
                return Err(format!(
                    "An unmanaged symlink already exists at '{}'. Refusing to replace it.",
                    target_path.display()
                ));
            }
        }
        Ok(metadata) if metadata.is_dir() => {
            return Err(format!(
                "A real directory already exists at '{}'. Refusing to overwrite.",
                target_path.display()
            ));
        }
        Ok(_) => {
            return Err(format!(
                "A file already exists at '{}'. Refusing to overwrite.",
                target_path.display()
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "Failed to inspect install path '{}': {}",
                target_path.display(),
                error
            ));
        }
    }

    Ok(())
}

async fn remove_universal_duplicates(
    pool: &DbPool,
    duplicates: &[DuplicateUniversalInstallation],
) -> Result<(), String> {
    let mut changed = Vec::new();

    for duplicate in duplicates {
        let installed_path = Path::new(&duplicate.installation.installed_path);
        let remove_result = if duplicate.remove_path {
            match std::fs::symlink_metadata(installed_path) {
                Ok(metadata) if metadata.file_type().is_symlink() => {
                    std::fs::remove_file(installed_path).map_err(|e| {
                        format!(
                            "Failed to remove duplicate symlink '{}': {}",
                            installed_path.display(),
                            e
                        )
                    })
                }
                Ok(_) => Err(format!(
                    "Path '{}' changed during installation. Refusing to remove it.",
                    installed_path.display()
                )),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(format!(
                    "Failed to inspect duplicate installation '{}': {}",
                    installed_path.display(),
                    error
                )),
            }
        } else {
            Ok(())
        };
        if let Err(error) = remove_result {
            let rollback_error = restore_universal_duplicates(pool, &changed).await;
            return Err(match rollback_error {
                Ok(()) => error,
                Err(rollback_error) => format!("{}; rollback failed: {}", error, rollback_error),
            });
        }
        changed.push(duplicate);
        if let Err(error) = db::delete_skill_installation(
            pool,
            &duplicate.installation.skill_id,
            &duplicate.installation.agent_id,
        )
        .await
        {
            let rollback_error = restore_universal_duplicates(pool, &changed).await;
            return Err(match rollback_error {
                Ok(()) => error,
                Err(rollback_error) => format!("{}; rollback failed: {}", error, rollback_error),
            });
        }
    }

    Ok(())
}

async fn restore_universal_duplicates(
    pool: &DbPool,
    duplicates: &[&DuplicateUniversalInstallation],
) -> Result<(), String> {
    let mut errors = Vec::new();

    for duplicate in duplicates.iter().rev() {
        let installed_path = Path::new(&duplicate.installation.installed_path);
        if duplicate.remove_path {
            if let Some(target) = duplicate.original_symlink_target.as_ref() {
                if let Some(parent) = installed_path.parent() {
                    if let Err(error) = std::fs::create_dir_all(parent) {
                        errors.push(format!(
                            "failed to restore directory '{}': {}",
                            parent.display(),
                            error
                        ));
                    }
                }
                if std::fs::symlink_metadata(installed_path).is_err() {
                    if let Err(error) = create_symlink(target, installed_path) {
                        errors.push(format!(
                            "failed to restore symlink '{}': {}",
                            installed_path.display(),
                            error
                        ));
                    }
                }
            }
        }
        if let Err(error) = db::upsert_skill_installation(pool, &duplicate.installation).await {
            errors.push(format!(
                "failed to restore installation '{}': {}",
                duplicate.installation.agent_id, error
            ));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

async fn rollback_install_target(
    pool: &DbPool,
    skill_id: &str,
    target_agent_id: &str,
    target_path: &Path,
    created_link_type: &str,
    previous_symlink_target: Option<&Path>,
    previous_installation: Option<&SkillInstallation>,
) -> Result<(), String> {
    let mut errors = Vec::new();

    match std::fs::symlink_metadata(target_path) {
        Ok(metadata) if created_link_type == "symlink" && metadata.file_type().is_symlink() => {
            if let Err(error) = std::fs::remove_file(target_path) {
                errors.push(format!(
                    "failed to remove new symlink '{}': {}",
                    target_path.display(),
                    error
                ));
            }
        }
        Ok(metadata) if created_link_type == "copy" && metadata.is_dir() => {
            if let Err(error) = std::fs::remove_dir_all(target_path) {
                errors.push(format!(
                    "failed to remove new copy '{}': {}",
                    target_path.display(),
                    error
                ));
            }
        }
        Ok(_) => errors.push(format!(
            "new install path '{}' changed before rollback",
            target_path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => errors.push(format!(
            "failed to inspect new install path '{}': {}",
            target_path.display(),
            error
        )),
    }

    if let Some(previous_target) = previous_symlink_target {
        if std::fs::symlink_metadata(target_path).is_err() {
            if let Err(error) = create_symlink(previous_target, target_path) {
                errors.push(format!(
                    "failed to restore previous symlink '{}': {}",
                    target_path.display(),
                    error
                ));
            }
        }
    }

    let db_result = if let Some(previous_installation) = previous_installation {
        db::upsert_skill_installation(pool, previous_installation).await
    } else {
        db::delete_skill_installation(pool, skill_id, target_agent_id).await
    };
    if let Err(error) = db_result {
        errors.push(format!("failed to restore installation record: {}", error));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[derive(Default)]
struct InstallRollback {
    previous_symlink_target: Option<PathBuf>,
    previous_installation: Option<SkillInstallation>,
}

fn replaceable_target(
    target_path: &Path,
    is_universal: bool,
    has_previous_installation: bool,
    universal_duplicates: &[DuplicateUniversalInstallation],
) -> Result<Option<PathBuf>, String> {
    match std::fs::symlink_metadata(target_path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            let tracked_alias = universal_duplicates
                .iter()
                .any(|duplicate| !duplicate.remove_path);
            if is_universal && !has_previous_installation && !tracked_alias {
                return Err(format!(
                    "An unmanaged symlink already exists at '{}'. Refusing to replace it.",
                    target_path.display()
                ));
            }
            let previous_target = std::fs::read_link(target_path).map_err(|e| {
                format!(
                    "Failed to read existing symlink '{}': {}",
                    target_path.display(),
                    e
                )
            })?;
            std::fs::remove_file(target_path)
                .map_err(|e| format!("Failed to remove existing symlink: {}", e))?;
            Ok(Some(previous_target))
        }
        Ok(metadata) if metadata.is_dir() => Err(format!(
            "A real directory already exists at '{}'. Refusing to overwrite.",
            target_path.display()
        )),
        Ok(_) => Err(format!(
            "A file already exists at '{}'. Refusing to overwrite.",
            target_path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "Failed to inspect install path '{}': {}",
            target_path.display(),
            error
        )),
    }
}

async fn rollback_error(
    pool: &DbPool,
    installation: &SkillInstallation,
    rollback: &InstallRollback,
    error: String,
) -> String {
    match rollback_install_target(
        pool,
        &installation.skill_id,
        &installation.agent_id,
        Path::new(&installation.installed_path),
        &installation.link_type,
        rollback.previous_symlink_target.as_deref(),
        rollback.previous_installation.as_ref(),
    )
    .await
    {
        Ok(()) => error,
        Err(rollback_error) => format!("{}; rollback failed: {}", error, rollback_error),
    }
}

async fn persist_install(
    pool: &DbPool,
    installation: &SkillInstallation,
    duplicates: &[DuplicateUniversalInstallation],
    rollback: &InstallRollback,
) -> Result<(), String> {
    if let Err(error) = db::upsert_skill_installation(pool, installation).await {
        return Err(rollback_error(pool, installation, rollback, error).await);
    }
    if let Err(error) = remove_universal_duplicates(pool, duplicates).await {
        return Err(rollback_error(pool, installation, rollback, error).await);
    }
    Ok(())
}

struct InstallPlan {
    canonical_dir: PathBuf,
    target_agent: db::Agent,
    target_path: PathBuf,
    legacy_noop: bool,
    universal_duplicates: Vec<DuplicateUniversalInstallation>,
}

async fn prepare_install(
    pool: &DbPool,
    skill_id: &str,
    requested_agent_id: &str,
) -> Result<InstallPlan, String> {
    if requested_agent_id == "central" {
        return Err("Cannot install a skill to the central agent itself".to_string());
    }

    let requested_agent = db::get_agent_by_id(pool, requested_agent_id)
        .await?
        .ok_or_else(|| format!("Agent '{}' not found", requested_agent_id))?;
    let central = db::get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central agent not found in database".to_string())?;
    let central_root = PathBuf::from(&central.global_skills_dir);
    let canonical_dir = canonical_dir_for_skill(pool, skill_id, &central_root).await?;
    let universal = db::get_agent_by_id(pool, "universal").await?;

    // 이전 DB는 ~/.agents/skills를 보관함과 플랫폼 로딩 경로로 함께 사용했다.
    // 마이그레이션 전까지는 기존처럼 보관만 해도 사용할 수 있는 동작을 유지한다.
    if universal.is_none() && db::agent_supports_universal_agents_skills(requested_agent_id) {
        ensure_centralized(pool, skill_id, &canonical_dir).await?;
        return Ok(InstallPlan {
            canonical_dir: canonical_dir.clone(),
            target_agent: requested_agent,
            target_path: canonical_dir,
            legacy_noop: true,
            universal_duplicates: Vec::new(),
        });
    }

    let Some(universal) = universal else {
        ensure_centralized(pool, skill_id, &canonical_dir).await?;
        let target_path = PathBuf::from(&requested_agent.global_skills_dir).join(skill_id);
        return Ok(InstallPlan {
            canonical_dir,
            target_agent: requested_agent,
            target_path,
            legacy_noop: false,
            universal_duplicates: Vec::new(),
        });
    };
    let universal_root = PathBuf::from(&universal.global_skills_dir);
    let requested_is_universal = requested_agent.id == "universal"
        || paths_refer_to_same_location(
            Path::new(&requested_agent.global_skills_dir),
            &universal_root,
        );
    let legacy_noop = paths_refer_to_same_location(&central_root, &universal_root)
        && agent_observes_universal(&requested_agent, &universal_root);

    if legacy_noop {
        ensure_centralized(pool, skill_id, &canonical_dir).await?;
        return Ok(InstallPlan {
            canonical_dir: canonical_dir.clone(),
            target_agent: universal,
            target_path: canonical_dir,
            legacy_noop: true,
            universal_duplicates: Vec::new(),
        });
    }

    let target_agent = if requested_is_universal {
        universal.clone()
    } else {
        requested_agent.clone()
    };
    let target_path = PathBuf::from(&target_agent.global_skills_dir).join(skill_id);

    if !requested_is_universal && agent_observes_universal(&requested_agent, &universal_root) {
        let has_universal_record = existing_installation_for_agent(pool, skill_id, "universal")
            .await?
            .is_some();
        if has_universal_record || std::fs::symlink_metadata(universal_root.join(skill_id)).is_ok()
        {
            return Err(format!(
                "Skill '{}' is already available through Universal. Remove the Universal installation before installing it specifically for '{}'.",
                skill_id, requested_agent_id
            ));
        }
    }

    ensure_centralized(pool, skill_id, &canonical_dir).await?;
    let universal_duplicates = if requested_is_universal {
        preflight_universal_duplicates(
            pool,
            skill_id,
            &universal_root,
            &target_path,
            &canonical_dir,
        )
        .await?
    } else {
        Vec::new()
    };

    Ok(InstallPlan {
        canonical_dir,
        target_agent,
        target_path,
        legacy_noop: false,
        universal_duplicates,
    })
}

// ─── Core Logic ───────────────────────────────────────────────────────────────

/// Tauri 계층과 분리된 실제 설치 로직이다.
/// 대상 플랫폼 경로에 보관함 원본을 가리키는 상대 심볼릭 링크를 만든다.
/// 대상 경로 충돌, 원본 누락, 잘못된 플랫폼 요청은 오류로 중단한다.
pub async fn install_skill_to_agent_impl(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
) -> Result<InstallResult, String> {
    let InstallPlan {
        canonical_dir,
        target_agent,
        target_path: symlink_path,
        legacy_noop,
        universal_duplicates,
    } = prepare_install(pool, skill_id, agent_id).await?;
    if legacy_noop {
        return Ok(InstallResult {
            symlink_path: canonical_dir.to_string_lossy().into_owned(),
        });
    }

    let agent_dir = PathBuf::from(&target_agent.global_skills_dir);
    std::fs::create_dir_all(&agent_dir)
        .map_err(|e| format!("Failed to create agent skills directory: {}", e))?;
    let previous_installation =
        existing_installation_for_agent(pool, skill_id, &target_agent.id).await?;
    let rollback = InstallRollback {
        previous_symlink_target: replaceable_target(
            &symlink_path,
            target_agent.id == "universal",
            previous_installation.is_some(),
            &universal_duplicates,
        )?,
        previous_installation,
    };
    let installation = SkillInstallation {
        skill_id: skill_id.to_string(),
        agent_id: target_agent.id,
        installed_path: symlink_path.to_string_lossy().into_owned(),
        link_type: "symlink".to_string(),
        symlink_target: Some(canonical_dir.to_string_lossy().into_owned()),
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let relative_target = symlink_target_path(&agent_dir, &canonical_dir);
    if let Err(error) = create_symlink(&relative_target, &symlink_path) {
        return Err(rollback_error(pool, &installation, &rollback, error).await);
    }
    persist_install(pool, &installation, &universal_duplicates, &rollback).await?;

    Ok(InstallResult {
        symlink_path: symlink_path.to_string_lossy().into_owned(),
    })
}

pub async fn install_skill_to_agent_auto_impl(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
) -> Result<InstallResult, String> {
    match install_skill_to_agent_impl(pool, skill_id, agent_id).await {
        Ok(result) => Ok(result),
        Err(error) if should_fallback_to_copy(&error) => {
            install_skill_to_agent_copy_impl(pool, skill_id, agent_id).await
        }
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn should_fallback_to_copy(error: &str) -> bool {
    error.contains("Failed to create symlink")
}

#[cfg(not(windows))]
fn should_fallback_to_copy(_error: &str) -> bool {
    false
}

/// Core copy-install logic — copies the skill directory instead of symlinking.
///
/// Copies `central.global_skills_dir/<skill_id>` recursively into
/// `agent.global_skills_dir/<skill_id>`. Existing symlinks at the target are
/// replaced; existing real directories cause an error.
pub async fn install_skill_to_agent_copy_impl(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
) -> Result<InstallResult, String> {
    let InstallPlan {
        canonical_dir,
        target_agent,
        target_path,
        legacy_noop,
        universal_duplicates,
    } = prepare_install(pool, skill_id, agent_id).await?;
    if legacy_noop {
        return Ok(InstallResult {
            symlink_path: canonical_dir.to_string_lossy().into_owned(),
        });
    }

    let agent_dir = PathBuf::from(&target_agent.global_skills_dir);
    std::fs::create_dir_all(&agent_dir)
        .map_err(|e| format!("Failed to create agent skills directory: {}", e))?;
    let previous_installation =
        existing_installation_for_agent(pool, skill_id, &target_agent.id).await?;
    let rollback = InstallRollback {
        previous_symlink_target: replaceable_target(
            &target_path,
            target_agent.id == "universal",
            previous_installation.is_some(),
            &universal_duplicates,
        )?,
        previous_installation,
    };
    let installation = SkillInstallation {
        skill_id: skill_id.to_string(),
        agent_id: target_agent.id,
        installed_path: target_path.to_string_lossy().into_owned(),
        link_type: "copy".to_string(),
        symlink_target: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    if let Err(error) = copy_dir_all(&canonical_dir, &target_path) {
        return Err(rollback_error(pool, &installation, &rollback, error).await);
    }
    persist_install(pool, &installation, &universal_duplicates, &rollback).await?;

    Ok(InstallResult {
        symlink_path: target_path.to_string_lossy().into_owned(),
    })
}

/// Core uninstall logic, separated from the Tauri layer for testability.
///
/// Removes the symlink at `agent.global_skills_dir/<skill_id>` and deletes the
/// corresponding `skill_installations` record.
///
/// For symlinked skills: removes the symlink.
/// For copied skills: removes the copied directory (tracked in the DB as link_type='copy').
/// Refuses to delete real directories not tracked as copies in the DB.
pub async fn uninstall_skill_from_agent_impl(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
) -> Result<(), String> {
    if agent_id == "central" {
        return Err("Cannot uninstall a Central skill through the platform installer".to_string());
    }

    let requested_agent = db::get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| format!("Agent '{}' not found", agent_id))?;
    let universal = db::get_agent_by_id(pool, "universal").await?;
    let central = db::get_agent_by_id(pool, "central").await?;

    let agent = if let Some(universal) = universal.as_ref() {
        if requested_agent.id == "universal"
            || paths_refer_to_same_location(
                Path::new(&requested_agent.global_skills_dir),
                Path::new(&universal.global_skills_dir),
            )
        {
            universal.clone()
        } else {
            requested_agent.clone()
        }
    } else {
        requested_agent.clone()
    };

    let installations = db::get_skill_installations(pool, skill_id).await?;
    let record = installations
        .iter()
        .find(|record| record.agent_id == agent.id);
    let legacy_shared_availability =
        universal
            .as_ref()
            .zip(central.as_ref())
            .is_some_and(|(universal, central)| {
                paths_refer_to_same_location(
                    Path::new(&universal.global_skills_dir),
                    Path::new(&central.global_skills_dir),
                ) && agent_observes_universal(
                    &requested_agent,
                    Path::new(&universal.global_skills_dir),
                )
            });
    if (legacy_shared_availability && agent.id == "universal")
        || (record.is_none()
            && (agent.id == "universal"
                || legacy_shared_availability
                || db::agent_supports_universal_agents_skills(agent_id)))
    {
        return Ok(());
    }
    let install_path = record
        .map(|r| PathBuf::from(&r.installed_path))
        .unwrap_or_else(|| PathBuf::from(&agent.global_skills_dir).join(skill_id));
    let link_type = record.map(|r| r.link_type.as_str()).unwrap_or("symlink");

    // 3. Inspect the entry at that path and remove it appropriately.
    match std::fs::symlink_metadata(&install_path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            // Always safe to remove symlinks.
            std::fs::remove_file(&install_path)
                .map_err(|e| format!("Failed to remove symlink: {}", e))?;
        }
        Ok(meta) if meta.is_dir() => {
            // Only remove real directories that were explicitly installed as copies.
            if link_type == "copy" {
                std::fs::remove_dir_all(&install_path)
                    .map_err(|e| format!("Failed to remove copied skill directory: {}", e))?;
            } else {
                return Err(format!(
                    "Path '{}' exists but is not a symlink. Refusing to delete.",
                    install_path.display()
                ));
            }
        }
        Ok(_) => {
            return Err(format!(
                "Path '{}' exists but is not a symlink. Refusing to delete.",
                install_path.display()
            ));
        }
        Err(_) => {
            // Path doesn't exist — still clean up the DB record.
        }
    }

    // 4. Remove the installation record from the database.
    db::delete_skill_installation(pool, skill_id, &agent.id).await?;

    Ok(())
}

// ─── Tauri Commands ───────────────────────────────────────────────────────────

/// Tauri command: install a skill to a single agent via relative symlink.
#[tauri::command]
pub async fn install_skill_to_agent(
    state: State<'_, AppState>,
    skill_id: String,
    agent_id: String,
    method: Option<String>,
) -> Result<InstallResult, String> {
    match method.as_deref().unwrap_or("auto") {
        "copy" => install_skill_to_agent_copy_impl(&state.db, &skill_id, &agent_id).await,
        "symlink" => install_skill_to_agent_impl(&state.db, &skill_id, &agent_id).await,
        _ => install_skill_to_agent_auto_impl(&state.db, &skill_id, &agent_id).await,
    }
}

/// Tauri command: remove a skill's symlink from an agent.
#[tauri::command]
pub async fn uninstall_skill_from_agent(
    state: State<'_, AppState>,
    skill_id: String,
    agent_id: String,
) -> Result<(), String> {
    uninstall_skill_from_agent_impl(&state.db, &skill_id, &agent_id).await
}

pub async fn batch_install_to_agents_impl(
    pool: &DbPool,
    skill_id: &str,
    agent_ids: &[String],
    method: &str,
) -> Result<BatchInstallResult, String> {
    let uses_universal = validate_batch_install_targets(pool, agent_ids).await?;
    if uses_universal {
        preflight_universal_install(pool, skill_id).await?;
    }

    let mut succeeded = Vec::new();
    let mut failed = Vec::new();

    for agent_id in agent_ids {
        let install_result = match method {
            "copy" => install_skill_to_agent_copy_impl(pool, skill_id, agent_id).await,
            "symlink" => install_skill_to_agent_impl(pool, skill_id, agent_id).await,
            _ => install_skill_to_agent_auto_impl(pool, skill_id, agent_id).await,
        };
        match install_result {
            Ok(_) => succeeded.push(agent_id.clone()),
            Err(e) => failed.push(FailedInstall {
                agent_id: agent_id.clone(),
                error: e,
            }),
        }
    }

    Ok(BatchInstallResult { succeeded, failed })
}

pub(crate) async fn batch_install_skills_to_agents_impl(
    pool: &DbPool,
    skill_ids: &[String],
    agent_ids: &[String],
) -> Result<BatchInstallResult, String> {
    if agent_ids.is_empty() {
        return Err("Select at least one install target".to_string());
    }

    let uses_universal = validate_batch_install_targets(pool, agent_ids).await?;
    if uses_universal {
        for skill_id in skill_ids {
            preflight_universal_install(pool, skill_id).await?;
        }
    }

    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    for skill_id in skill_ids {
        for agent_id in agent_ids {
            match install_skill_to_agent_impl(pool, skill_id, agent_id).await {
                Ok(_) => succeeded.push(format!("{skill_id}:{agent_id}")),
                Err(error) => failed.push(FailedInstall {
                    agent_id: format!("{skill_id}:{agent_id}"),
                    error,
                }),
            }
        }
    }

    Ok(BatchInstallResult { succeeded, failed })
}

fn plugin_bundle_directory_name(source_label: &str) -> Result<String, String> {
    let mut name = String::new();
    let mut last_was_dash = false;
    for ch in source_label.trim().to_lowercase().chars() {
        if ch.is_ascii_alphanumeric() {
            name.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            name.push('-');
            last_was_dash = true;
        }
    }
    let name = name.trim_matches('-').to_string();
    if name.is_empty() {
        return Err("Plugin source label is invalid".to_string());
    }
    Ok(name)
}

async fn rollback_plugin_bundle_import(
    pool: &DbPool,
    skill_ids: &[String],
    skill_paths: &[PathBuf],
    bundle_dir: &Path,
    remove_bundle_dir: bool,
) {
    for skill_id in skill_ids.iter().rev() {
        let _ = db::delete_skill(pool, skill_id).await;
    }
    for skill_path in skill_paths.iter().rev() {
        let _ = std::fs::remove_dir_all(skill_path);
    }
    if remove_bundle_dir {
        let _ = std::fs::remove_dir(bundle_dir);
    }
}

async fn centralize_plugin_bundle_impl(
    pool: &DbPool,
    source_agent_id: &str,
    source_label: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    let observations = db::get_agent_skill_observations(pool, source_agent_id)
        .await?
        .into_iter()
        .filter(|observation| {
            observation.source_kind == "plugin"
                && observation.source_label.as_deref() == Some(source_label)
        })
        .collect::<Vec<_>>();
    if observations.is_empty() {
        return Err(format!("Plugin bundle '{}' was not found", source_label));
    }

    let central = db::get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central agent not found in database".to_string())?;
    let central_root = PathBuf::from(central.global_skills_dir);
    std::fs::create_dir_all(&central_root)
        .map_err(|error| format!("Failed to create Central Skills root: {error}"))?;
    let bundle_dir = central_root.join(plugin_bundle_directory_name(source_label)?);
    let bundle_dir_existed = match std::fs::symlink_metadata(&bundle_dir) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => true,
        Ok(_) => {
            return Err(format!(
                "Central bundle path '{}' is not a writable directory",
                bundle_dir.display()
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(format!("Failed to inspect Central bundle path: {error}")),
    };
    if bundle_dir.join("SKILL.md").exists() {
        return Err(format!(
            "Central path '{}' is already a skill, not a bundle",
            bundle_dir.display()
        ));
    }

    let mut seen_ids = HashSet::new();
    let mut skipped = Vec::new();
    let mut candidates = Vec::new();
    for observation in observations {
        if !seen_ids.insert(observation.skill_id.clone()) {
            continue;
        }
        let target_dir = bundle_dir.join(&observation.skill_id);
        if db::get_skill_by_id(pool, &observation.skill_id)
            .await?
            .is_some()
            || std::fs::symlink_metadata(&target_dir).is_ok()
        {
            skipped.push(observation.skill_id);
            continue;
        }
        candidates.push((observation, target_dir));
    }

    let mut imported = Vec::new();
    let mut created_paths = Vec::new();
    for (observation, target_dir) in candidates {
        let skill = db::Skill {
            id: observation.skill_id.clone(),
            name: observation.name,
            description: observation.description,
            file_path: observation.file_path,
            canonical_path: None,
            is_central: false,
            source: Some(format!("plugin:{source_label}")),
            content: None,
            scanned_at: chrono::Utc::now().to_rfc3339(),
        };
        created_paths.push(target_dir.clone());
        imported.push(skill.id.clone());

        if let Err(error) = db::upsert_skill(pool, &skill).await {
            rollback_plugin_bundle_import(
                pool,
                &imported,
                &created_paths,
                &bundle_dir,
                !bundle_dir_existed,
            )
            .await;
            return Err(error);
        }
        if let Err(error) = ensure_centralized(pool, &skill.id, &target_dir).await {
            rollback_plugin_bundle_import(
                pool,
                &imported,
                &created_paths,
                &bundle_dir,
                !bundle_dir_existed,
            )
            .await;
            return Err(error);
        }
    }

    imported.sort();
    skipped.sort();
    Ok((imported, skipped))
}

pub async fn install_plugin_skill_bundle_to_agents_impl(
    pool: &DbPool,
    source_agent_id: &str,
    source_label: &str,
    agent_ids: &[String],
) -> Result<SkillBundleInstallResult, String> {
    if agent_ids.is_empty() {
        return Err("Select at least one install target".to_string());
    }
    validate_batch_install_targets(pool, agent_ids).await?;

    let (imported, skipped) =
        centralize_plugin_bundle_impl(pool, source_agent_id, source_label).await?;
    let installs = batch_install_skills_to_agents_impl(pool, &imported, agent_ids).await?;
    Ok(SkillBundleInstallResult {
        imported,
        skipped,
        succeeded: installs.succeeded,
        failed: installs.failed,
    })
}

/// 여러 설치 대상에 같은 스킬을 설치한다.
/// 공용 설치 관련 충돌은 쓰기 전에 중단하고, 그 밖의 독립적인 실패는 결과에 모은다.
#[tauri::command]
pub async fn batch_install_to_agents(
    state: State<'_, AppState>,
    skill_id: String,
    agent_ids: Vec<String>,
    method: Option<String>,
) -> Result<BatchInstallResult, String> {
    batch_install_to_agents_impl(
        &state.db,
        &skill_id,
        &agent_ids,
        method.as_deref().unwrap_or("auto"),
    )
    .await
}

#[tauri::command]
pub async fn install_plugin_skill_bundle_to_agents(
    state: State<'_, AppState>,
    source_agent_id: String,
    source_label: String,
    agent_ids: Vec<String>,
) -> Result<SkillBundleInstallResult, String> {
    install_plugin_skill_bundle_to_agents_impl(
        &state.db,
        &source_agent_id,
        &source_label,
        &agent_ids,
    )
    .await
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use sqlx::SqlitePool;
    use std::fs;
    use tempfile::TempDir;

    // ── Test helpers ──────────────────────────────────────────────────────────

    /// Create an in-memory SQLite pool with the full schema initialised and
    /// the central/claude-code agent directories redirected to `central_dir`
    /// and `agent_dir` respectively.
    async fn setup_db(central_dir: &Path, agent_dir: &Path) -> DbPool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();

        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'central'")
            .bind(central_dir.to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();

        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'claude-code'")
            .bind(agent_dir.to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();

        pool
    }

    async fn set_agent_dir(pool: &DbPool, agent_id: &str, path: &Path) {
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = ?")
            .bind(path.to_string_lossy().to_string())
            .bind(agent_id)
            .execute(pool)
            .await
            .unwrap();
    }

    /// Create a minimal skill directory containing a valid `SKILL.md`.
    fn create_central_skill(central_dir: &Path, skill_id: &str) -> PathBuf {
        let skill_dir = central_dir.join(skill_id);
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            format!(
                "---\nname: {}\ndescription: Test skill\n---\n\n# {}\n",
                skill_id, skill_id
            ),
        )
        .unwrap();
        skill_dir
    }

    async fn create_plugin_observation(
        pool: &DbPool,
        plugin_root: &Path,
        source_label: &str,
        skill_id: &str,
        skill_name: &str,
        create_source: bool,
    ) {
        let skill_dir = plugin_root.join("skills").join(skill_id);
        if create_source {
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(
                skill_dir.join("SKILL.md"),
                format!("---\nname: {skill_name}\ndescription: Plugin skill\n---\n"),
            )
            .unwrap();
        }

        db::upsert_agent_skill_observation(
            pool,
            &db::AgentSkillObservation {
                row_id: format!("claude-code::plugin::{source_label}::{skill_id}"),
                agent_id: "claude-code".to_string(),
                skill_id: skill_id.to_string(),
                name: skill_name.to_string(),
                description: Some("Plugin skill".to_string()),
                file_path: skill_dir.join("SKILL.md").to_string_lossy().into_owned(),
                dir_path: skill_dir.to_string_lossy().into_owned(),
                source_kind: "plugin".to_string(),
                source_root: plugin_root.to_string_lossy().into_owned(),
                source_label: Some(source_label.to_string()),
                link_type: "copy".to_string(),
                symlink_target: None,
                is_read_only: true,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
    }

    // ── make_relative_path ────────────────────────────────────────────────────

    #[test]
    fn test_make_relative_path_sibling_dirs() {
        let from = Path::new("/home/user/claude/skills");
        let to = Path::new("/home/user/.agents/skills/my-skill");
        let rel = make_relative_path(from, to);
        assert_eq!(rel, PathBuf::from("../../.agents/skills/my-skill"));
    }

    #[test]
    fn test_make_relative_path_same_parent() {
        let from = Path::new("/tmp/test/agent");
        let to = Path::new("/tmp/test/central/skill-x");
        let rel = make_relative_path(from, to);
        assert_eq!(rel, PathBuf::from("../central/skill-x"));
    }

    #[test]
    fn test_make_relative_path_deep_nesting() {
        let from = Path::new("/a/b/c/d");
        let to = Path::new("/a/x/y");
        let rel = make_relative_path(from, to);
        assert_eq!(rel, PathBuf::from("../../../x/y"));
    }

    // ── install_skill_to_agent_impl ───────────────────────────────────────────

    #[tokio::test]
    async fn test_install_creates_symlink() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;

        create_central_skill(&central_dir, "my-skill");

        let result = install_skill_to_agent_impl(&pool, "my-skill", "claude-code").await;
        assert!(result.is_ok(), "install should succeed: {:?}", result);

        let symlink_path = agent_dir.join("my-skill");
        let meta = fs::symlink_metadata(&symlink_path).unwrap();
        assert!(meta.file_type().is_symlink(), "entry should be a symlink");
    }

    #[tokio::test]
    async fn test_install_to_universal_agent_returns_central_availability_without_link() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join(".agents/skills");
        let cursor_dir = tmp.path().join(".cursor/skills");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        set_agent_dir(&pool, "universal", &central_dir).await;
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'cursor'")
            .bind(cursor_dir.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        create_central_skill(&central_dir, "universal-skill");

        let result = install_skill_to_agent_impl(&pool, "universal-skill", "cursor")
            .await
            .unwrap();

        assert_eq!(
            result.symlink_path,
            central_dir
                .join("universal-skill")
                .to_string_lossy()
                .into_owned()
        );
        assert!(
            !cursor_dir.join("universal-skill").exists(),
            "universal agents must not receive redundant links for central skills"
        );
        assert!(
            db::get_skill_installations(&pool, "universal-skill")
                .await
                .unwrap()
                .into_iter()
                .all(|installation| installation.agent_id != "cursor"),
            "universal availability must not create removable installation rows"
        );
    }

    #[tokio::test]
    async fn test_install_to_universal_creates_real_link_and_universal_record() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("vault");
        let universal_dir = tmp.path().join(".agents/skills");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        set_agent_dir(&pool, "universal", &universal_dir).await;
        let canonical = create_central_skill(&central_dir, "shared-skill");

        let result = install_skill_to_agent_impl(&pool, "shared-skill", "universal")
            .await
            .unwrap();

        let installed = universal_dir.join("shared-skill");
        assert_eq!(result.symlink_path, installed.to_string_lossy());
        assert!(fs::symlink_metadata(&installed)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            installed.canonicalize().unwrap(),
            canonical.canonicalize().unwrap()
        );
        let installations = db::get_skill_installations(&pool, "shared-skill")
            .await
            .unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "universal");
    }

    #[tokio::test]
    async fn test_copy_install_and_uninstall_use_universal_target_record() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("vault");
        let universal_dir = tmp.path().join(".agents/skills");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        set_agent_dir(&pool, "universal", &universal_dir).await;
        create_central_skill(&central_dir, "shared-copy");

        install_skill_to_agent_copy_impl(&pool, "shared-copy", "universal")
            .await
            .unwrap();

        let installed = universal_dir.join("shared-copy");
        assert!(installed.is_dir());
        assert!(!fs::symlink_metadata(&installed)
            .unwrap()
            .file_type()
            .is_symlink());
        let installations = db::get_skill_installations(&pool, "shared-copy")
            .await
            .unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "universal");
        assert_eq!(installations[0].link_type, "copy");

        uninstall_skill_from_agent_impl(&pool, "shared-copy", "universal")
            .await
            .unwrap();
        assert!(!installed.exists());
        assert!(central_dir.join("shared-copy/SKILL.md").exists());
    }

    #[tokio::test]
    async fn test_direct_shared_path_agent_is_recorded_as_universal() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("vault");
        let universal_dir = tmp.path().join(".agents/skills");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        set_agent_dir(&pool, "universal", &universal_dir).await;
        set_agent_dir(&pool, "codex", &universal_dir).await;
        create_central_skill(&central_dir, "codex-shared");

        install_skill_to_agent_impl(&pool, "codex-shared", "codex")
            .await
            .unwrap();

        let installations = db::get_skill_installations(&pool, "codex-shared")
            .await
            .unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "universal");
        assert!(fs::symlink_metadata(universal_dir.join("codex-shared")).is_ok());
    }

    #[tokio::test]
    async fn test_universal_install_replaces_tracked_compatible_symlink() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("vault");
        let universal_dir = tmp.path().join(".agents/skills");
        let cursor_dir = tmp.path().join(".cursor/skills");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        set_agent_dir(&pool, "universal", &universal_dir).await;
        set_agent_dir(&pool, "cursor", &cursor_dir).await;
        create_central_skill(&central_dir, "move-to-shared");
        install_skill_to_agent_impl(&pool, "move-to-shared", "cursor")
            .await
            .unwrap();

        install_skill_to_agent_impl(&pool, "move-to-shared", "universal")
            .await
            .unwrap();

        assert!(fs::symlink_metadata(cursor_dir.join("move-to-shared")).is_err());
        assert!(fs::symlink_metadata(universal_dir.join("move-to-shared")).is_ok());
        let installations = db::get_skill_installations(&pool, "move-to-shared")
            .await
            .unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "universal");
    }

    #[tokio::test]
    async fn test_universal_install_refuses_compatible_copy_without_changes() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("vault");
        let universal_dir = tmp.path().join(".agents/skills");
        let cursor_dir = tmp.path().join(".cursor/skills");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        set_agent_dir(&pool, "universal", &universal_dir).await;
        set_agent_dir(&pool, "cursor", &cursor_dir).await;
        create_central_skill(&central_dir, "copied-skill");
        install_skill_to_agent_copy_impl(&pool, "copied-skill", "cursor")
            .await
            .unwrap();

        let result = install_skill_to_agent_impl(&pool, "copied-skill", "universal").await;

        assert!(result.is_err());
        assert!(cursor_dir.join("copied-skill/SKILL.md").exists());
        assert!(fs::symlink_metadata(universal_dir.join("copied-skill")).is_err());
        let installations = db::get_skill_installations(&pool, "copied-skill")
            .await
            .unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "cursor");
        assert_eq!(installations[0].link_type, "copy");
    }

    #[tokio::test]
    async fn test_native_install_is_blocked_while_universal_exists() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("vault");
        let universal_dir = tmp.path().join(".agents/skills");
        let cursor_dir = tmp.path().join(".cursor/skills");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        set_agent_dir(&pool, "universal", &universal_dir).await;
        set_agent_dir(&pool, "cursor", &cursor_dir).await;
        create_central_skill(&central_dir, "shared-first");
        install_skill_to_agent_impl(&pool, "shared-first", "universal")
            .await
            .unwrap();

        let result = install_skill_to_agent_impl(&pool, "shared-first", "cursor").await;

        assert!(result.is_err());
        assert!(fs::symlink_metadata(cursor_dir.join("shared-first")).is_err());
    }

    #[tokio::test]
    async fn test_universal_cleanup_failure_rolls_back_new_install_and_duplicate() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("vault");
        let universal_dir = tmp.path().join(".agents/skills");
        let cursor_dir = tmp.path().join(".cursor/skills");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        set_agent_dir(&pool, "universal", &universal_dir).await;
        set_agent_dir(&pool, "cursor", &cursor_dir).await;
        let canonical = create_central_skill(&central_dir, "rollback-skill");
        install_skill_to_agent_impl(&pool, "rollback-skill", "cursor")
            .await
            .unwrap();
        sqlx::query(
            "CREATE TRIGGER fail_cursor_delete BEFORE DELETE ON skill_installations
             WHEN OLD.agent_id = 'cursor'
             BEGIN SELECT RAISE(ABORT, 'forced cleanup failure'); END",
        )
        .execute(&pool)
        .await
        .unwrap();

        let result = install_skill_to_agent_impl(&pool, "rollback-skill", "universal").await;

        assert!(result.is_err());
        assert!(fs::symlink_metadata(universal_dir.join("rollback-skill")).is_err());
        let cursor_link = cursor_dir.join("rollback-skill");
        assert!(fs::symlink_metadata(&cursor_link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            cursor_link.canonicalize().unwrap(),
            canonical.canonicalize().unwrap()
        );
        let installations = db::get_skill_installations(&pool, "rollback-skill")
            .await
            .unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "cursor");
    }

    #[tokio::test]
    async fn test_db_failure_restores_replaced_universal_symlink() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("vault");
        let universal_dir = tmp.path().join(".agents/skills");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&universal_dir).unwrap();
        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        set_agent_dir(&pool, "universal", &universal_dir).await;
        let canonical = create_central_skill(&central_dir, "restore-link");
        let target = universal_dir.join("restore-link");
        create_symlink(&canonical, &target).unwrap();
        let original_target = fs::read_link(&target).unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "restore-link".to_string(),
                agent_id: "universal".to_string(),
                installed_path: target.to_string_lossy().into_owned(),
                link_type: "symlink".to_string(),
                symlink_target: Some(canonical.to_string_lossy().into_owned()),
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        sqlx::query(
            "CREATE TRIGGER fail_universal_insert BEFORE INSERT ON skill_installations
             WHEN NEW.agent_id = 'universal'
             BEGIN SELECT RAISE(ABORT, 'forced insert failure'); END",
        )
        .execute(&pool)
        .await
        .unwrap();

        let result = install_skill_to_agent_impl(&pool, "restore-link", "universal").await;

        assert!(result.is_err());
        assert_eq!(fs::read_link(&target).unwrap(), original_target);
        let installations = db::get_skill_installations(&pool, "restore-link")
            .await
            .unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "universal");
    }

    #[tokio::test]
    async fn test_universal_install_refuses_unmanaged_symlink() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("vault");
        let universal_dir = tmp.path().join(".agents/skills");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&universal_dir).unwrap();
        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        set_agent_dir(&pool, "universal", &universal_dir).await;
        let canonical = create_central_skill(&central_dir, "manual-link");
        let target = universal_dir.join("manual-link");
        create_symlink(&canonical, &target).unwrap();

        let result = install_skill_to_agent_impl(&pool, "manual-link", "universal").await;

        assert!(result.is_err());
        assert_eq!(
            target.canonicalize().unwrap(),
            canonical.canonicalize().unwrap()
        );
        assert!(db::get_skill_installations(&pool, "manual-link")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_install_symlink_is_relative() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "rel-skill");

        install_skill_to_agent_impl(&pool, "rel-skill", "claude-code")
            .await
            .unwrap();

        let symlink_path = agent_dir.join("rel-skill");
        let link_target = fs::read_link(&symlink_path).unwrap();
        assert!(
            link_target.is_relative(),
            "symlink target should be relative, got {:?}",
            link_target
        );
    }

    #[tokio::test]
    async fn test_install_symlink_resolves_correctly() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "resolve-skill");

        install_skill_to_agent_impl(&pool, "resolve-skill", "claude-code")
            .await
            .unwrap();

        let symlink_path = agent_dir.join("resolve-skill");
        // Following the symlink should give access to SKILL.md in the central dir.
        let skill_md = symlink_path.join("SKILL.md");
        assert!(
            skill_md.exists(),
            "SKILL.md should be accessible via symlink"
        );
    }

    #[tokio::test]
    async fn test_install_creates_agent_dir_if_missing() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        // Do NOT pre-create agent_dir — install should create it.
        let agent_dir = tmp.path().join("new-agent-dir");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "dir-skill");

        let result = install_skill_to_agent_impl(&pool, "dir-skill", "claude-code").await;
        assert!(result.is_ok(), "install should create missing agent dir");
        assert!(agent_dir.exists(), "agent dir should have been created");
    }

    #[tokio::test]
    async fn test_install_updates_db_record() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "db-skill");

        install_skill_to_agent_impl(&pool, "db-skill", "claude-code")
            .await
            .unwrap();

        let installations = db::get_skill_installations(&pool, "db-skill")
            .await
            .unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "claude-code");
        assert_eq!(installations[0].link_type, "symlink");
    }

    #[tokio::test]
    async fn test_install_uses_nested_canonical_path_from_db() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        let nested_skill_dir = central_dir.join("superpowers").join("using-superpowers");
        fs::create_dir_all(&nested_skill_dir).unwrap();
        fs::write(
            nested_skill_dir.join("SKILL.md"),
            "---\nname: using-superpowers\ndescription: Nested central\n---\n",
        )
        .unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "using-superpowers".to_string(),
                name: "using-superpowers".to_string(),
                description: Some("Nested central".to_string()),
                file_path: nested_skill_dir
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(nested_skill_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("native".to_string()),
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        install_skill_to_agent_impl(&pool, "using-superpowers", "claude-code")
            .await
            .unwrap();

        let symlink_path = agent_dir.join("using-superpowers");
        assert!(symlink_path.join("SKILL.md").exists());
        assert!(
            !central_dir.join("using-superpowers").exists(),
            "nested canonical skill must not be copied to the central root"
        );
        let link_target = fs::read_link(&symlink_path).unwrap();
        assert!(
            link_target
                .to_string_lossy()
                .contains("superpowers/using-superpowers"),
            "symlink should point at nested canonical path, got {:?}",
            link_target
        );
    }

    #[tokio::test]
    async fn test_install_fails_when_canonical_missing() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        // Do NOT create the skill in central_dir.

        let result = install_skill_to_agent_impl(&pool, "nonexistent-skill", "claude-code").await;
        assert!(
            result.is_err(),
            "install should fail if canonical skill missing"
        );
    }

    #[tokio::test]
    async fn test_install_fails_for_unknown_agent() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "some-skill");

        let result = install_skill_to_agent_impl(&pool, "some-skill", "nonexistent-agent").await;
        assert!(result.is_err(), "install should fail for unknown agent");
    }

    #[tokio::test]
    async fn test_install_to_central_agent_is_rejected() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        create_central_skill(&central_dir, "self-skill");

        let result = install_skill_to_agent_impl(&pool, "self-skill", "central").await;
        assert!(
            result.is_err(),
            "installing to 'central' should be rejected"
        );
    }

    #[tokio::test]
    async fn test_install_replaces_existing_symlink() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "re-link-skill");

        // Install once.
        install_skill_to_agent_impl(&pool, "re-link-skill", "claude-code")
            .await
            .unwrap();

        // Install again — should replace the existing symlink without error.
        let result = install_skill_to_agent_impl(&pool, "re-link-skill", "claude-code").await;
        assert!(result.is_ok(), "re-install should succeed: {:?}", result);
    }

    #[tokio::test]
    async fn test_install_refuses_to_overwrite_real_dir() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "real-dir-skill");

        // Create a real (non-symlink) directory at the install location.
        fs::create_dir_all(agent_dir.join("real-dir-skill")).unwrap();

        let result = install_skill_to_agent_impl(&pool, "real-dir-skill", "claude-code").await;
        assert!(
            result.is_err(),
            "install should refuse to overwrite a real directory"
        );
    }

    // ── uninstall_skill_from_agent_impl ───────────────────────────────────────

    #[tokio::test]
    async fn test_uninstall_removes_symlink() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "uninstall-skill");

        install_skill_to_agent_impl(&pool, "uninstall-skill", "claude-code")
            .await
            .unwrap();

        let symlink_path = agent_dir.join("uninstall-skill");
        assert!(symlink_path.exists() || fs::symlink_metadata(&symlink_path).is_ok());

        uninstall_skill_from_agent_impl(&pool, "uninstall-skill", "claude-code")
            .await
            .unwrap();

        assert!(
            fs::symlink_metadata(&symlink_path).is_err(),
            "symlink should have been removed"
        );
    }

    #[tokio::test]
    async fn test_uninstall_removes_db_record() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "db-uninstall-skill");

        install_skill_to_agent_impl(&pool, "db-uninstall-skill", "claude-code")
            .await
            .unwrap();

        uninstall_skill_from_agent_impl(&pool, "db-uninstall-skill", "claude-code")
            .await
            .unwrap();

        let installations = db::get_skill_installations(&pool, "db-uninstall-skill")
            .await
            .unwrap();
        assert!(installations.is_empty(), "DB record should be removed");
    }

    #[tokio::test]
    async fn test_uninstall_refuses_real_dir() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;

        // Place a real directory where the symlink would be.
        fs::create_dir_all(agent_dir.join("protected-skill")).unwrap();

        let result = uninstall_skill_from_agent_impl(&pool, "protected-skill", "claude-code").await;
        assert!(
            result.is_err(),
            "uninstall should refuse to delete a real directory"
        );

        // Ensure the directory still exists.
        assert!(
            agent_dir.join("protected-skill").is_dir(),
            "real directory should NOT have been deleted"
        );
    }

    #[tokio::test]
    async fn test_uninstall_nonexistent_path_still_cleans_db() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;

        // Manually insert an installation record without creating the symlink.
        let installation = SkillInstallation {
            skill_id: "ghost-skill".to_string(),
            agent_id: "claude-code".to_string(),
            installed_path: agent_dir.join("ghost-skill").to_string_lossy().into_owned(),
            link_type: "symlink".to_string(),
            symlink_target: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        db::upsert_skill_installation(&pool, &installation)
            .await
            .unwrap();

        let result = uninstall_skill_from_agent_impl(&pool, "ghost-skill", "claude-code").await;
        assert!(result.is_ok(), "uninstall of missing path should succeed");

        let installations = db::get_skill_installations(&pool, "ghost-skill")
            .await
            .unwrap();
        assert!(installations.is_empty(), "DB record should be cleaned up");
    }

    #[tokio::test]
    async fn test_uninstall_universal_availability_without_record_is_noop() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join(".agents/skills");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        create_central_skill(&central_dir, "universal-skill");

        uninstall_skill_from_agent_impl(&pool, "universal-skill", "cursor")
            .await
            .unwrap();

        assert!(
            central_dir.join("universal-skill/SKILL.md").exists(),
            "uninstalling read-only universal availability must not delete the central skill"
        );
    }

    #[tokio::test]
    async fn test_uninstall_universal_removes_only_tracked_target() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("vault");
        let universal_dir = tmp.path().join(".agents/skills");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        set_agent_dir(&pool, "universal", &universal_dir).await;
        create_central_skill(&central_dir, "remove-shared");
        install_skill_to_agent_impl(&pool, "remove-shared", "universal")
            .await
            .unwrap();

        uninstall_skill_from_agent_impl(&pool, "remove-shared", "universal")
            .await
            .unwrap();

        assert!(fs::symlink_metadata(universal_dir.join("remove-shared")).is_err());
        assert!(central_dir.join("remove-shared/SKILL.md").exists());
        assert!(db::get_skill_installations(&pool, "remove-shared")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_uninstall_universal_preserves_untracked_real_directory() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("vault");
        let universal_dir = tmp.path().join(".agents/skills");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        set_agent_dir(&pool, "universal", &universal_dir).await;
        let manual_dir = create_central_skill(&universal_dir, "manual-skill");

        uninstall_skill_from_agent_impl(&pool, "manual-skill", "universal")
            .await
            .unwrap();

        assert!(manual_dir.join("SKILL.md").exists());
        assert!(db::get_skill_installations(&pool, "manual-skill")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_uninstall_uses_recorded_installed_path_for_nested_skill() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("hermes");
        let nested_installed = agent_dir.join("apple").join("apple-reminders");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&nested_installed).unwrap();
        fs::write(
            nested_installed.join("SKILL.md"),
            "---\nname: apple-reminders\ndescription: Nested platform\n---\n",
        )
        .unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        let installation = SkillInstallation {
            skill_id: "apple-reminders".to_string(),
            agent_id: "claude-code".to_string(),
            installed_path: nested_installed.to_string_lossy().into_owned(),
            link_type: "copy".to_string(),
            symlink_target: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        db::upsert_skill_installation(&pool, &installation)
            .await
            .unwrap();

        uninstall_skill_from_agent_impl(&pool, "apple-reminders", "claude-code")
            .await
            .unwrap();

        assert!(
            !nested_installed.exists(),
            "uninstall should remove the actual nested installed path"
        );
    }

    // ── batch install ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_batch_install_multiple_agents() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let claude_dir = tmp.path().join("claude");
        let aider_dir = tmp.path().join("aider");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &claude_dir).await;

        // Override a second non-universal agent's dir too.
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'aider'")
            .bind(aider_dir.to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();

        create_central_skill(&central_dir, "batch-skill");

        let result = batch_install_impl(
            &pool,
            "batch-skill",
            &["claude-code".to_string(), "aider".to_string()],
        )
        .await;

        assert_eq!(result.succeeded.len(), 2);
        assert!(result.failed.is_empty());

        assert!(fs::symlink_metadata(claude_dir.join("batch-skill")).is_ok());
        assert!(fs::symlink_metadata(aider_dir.join("batch-skill")).is_ok());
    }

    #[tokio::test]
    async fn test_batch_install_partial_failure() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let claude_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &claude_dir).await;
        create_central_skill(&central_dir, "partial-skill");

        let result = batch_install_impl(
            &pool,
            "partial-skill",
            &[
                "claude-code".to_string(),
                "nonexistent-agent".to_string(), // will fail
            ],
        )
        .await;

        assert_eq!(result.succeeded.len(), 1);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].agent_id, "nonexistent-agent");
    }

    #[tokio::test]
    async fn batch_rejects_universal_and_compatible_targets_before_writing() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let claude_dir = tmp.path().join("claude");
        let universal_dir = tmp.path().join("universal");
        let cursor_dir = tmp.path().join("cursor");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &claude_dir).await;
        set_agent_dir(&pool, "universal", &universal_dir).await;
        set_agent_dir(&pool, "cursor", &cursor_dir).await;
        create_central_skill(&central_dir, "conflicting-batch");

        for (agent_ids, method) in [
            (
                vec!["cursor".to_string(), "universal".to_string()],
                "symlink",
            ),
            (vec!["universal".to_string(), "cursor".to_string()], "copy"),
        ] {
            let result =
                batch_install_to_agents_impl(&pool, "conflicting-batch", &agent_ids, method).await;

            assert!(result.is_err());
            assert!(!cursor_dir.join("conflicting-batch").exists());
            assert!(!universal_dir.join("conflicting-batch").exists());
            assert!(db::get_skill_installations(&pool, "conflicting-batch")
                .await
                .unwrap()
                .is_empty());
        }
    }

    async fn batch_install_impl(
        pool: &DbPool,
        skill_id: &str,
        agent_ids: &[String],
    ) -> BatchInstallResult {
        batch_install_to_agents_impl(pool, skill_id, agent_ids, "symlink")
            .await
            .unwrap()
    }

    // ── install_skill_to_agent_copy_impl ──────────────────────────────────────

    #[tokio::test]
    async fn test_copy_install_creates_real_directory() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "copy-skill");

        let result = install_skill_to_agent_copy_impl(&pool, "copy-skill", "claude-code").await;
        assert!(result.is_ok(), "copy install should succeed: {:?}", result);

        let target = agent_dir.join("copy-skill");
        let meta = fs::symlink_metadata(&target).unwrap();
        // Must be a real directory — NOT a symlink.
        assert!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "installed path should be a real directory, not a symlink"
        );
    }

    #[tokio::test]
    async fn test_copy_install_files_are_copied() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;

        // Create skill with multiple files to verify all are copied.
        let skill_dir = central_dir.join("multi-file-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: multi-file-skill\ndescription: Test\n---\n",
        )
        .unwrap();
        fs::write(skill_dir.join("extra.txt"), "extra content").unwrap();

        install_skill_to_agent_copy_impl(&pool, "multi-file-skill", "claude-code")
            .await
            .unwrap();

        let installed_skill_dir = agent_dir.join("multi-file-skill");

        // Verify SKILL.md was copied.
        let skill_md = installed_skill_dir.join("SKILL.md");
        assert!(skill_md.exists(), "SKILL.md should be copied to agent dir");

        // Verify extra file was copied.
        let extra = installed_skill_dir.join("extra.txt");
        assert!(extra.exists(), "extra.txt should be copied to agent dir");
        assert_eq!(
            fs::read_to_string(&extra).unwrap(),
            "extra content",
            "copied file contents should match"
        );

        // Confirm that the installed path is NOT a symlink.
        let meta = fs::symlink_metadata(&installed_skill_dir).unwrap();
        assert!(
            !meta.file_type().is_symlink(),
            "installed directory must NOT be a symlink"
        );
    }

    #[tokio::test]
    async fn test_copy_install_updates_db_with_copy_type() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "db-copy-skill");

        install_skill_to_agent_copy_impl(&pool, "db-copy-skill", "claude-code")
            .await
            .unwrap();

        let installations = db::get_skill_installations(&pool, "db-copy-skill")
            .await
            .unwrap();
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "claude-code");
        assert_eq!(
            installations[0].link_type, "copy",
            "DB should record link_type as 'copy'"
        );
    }

    #[tokio::test]
    async fn test_copy_install_to_central_is_rejected() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        create_central_skill(&central_dir, "self-copy-skill");

        let result = install_skill_to_agent_copy_impl(&pool, "self-copy-skill", "central").await;
        assert!(
            result.is_err(),
            "copy install to 'central' should be rejected"
        );
    }

    #[tokio::test]
    async fn test_copy_install_fails_when_canonical_missing() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        // Deliberately do NOT create the skill in central_dir.

        let result = install_skill_to_agent_copy_impl(&pool, "missing-skill", "claude-code").await;
        assert!(
            result.is_err(),
            "copy install should fail when canonical skill is missing"
        );
    }

    #[tokio::test]
    async fn test_copy_install_refuses_to_overwrite_real_dir() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&agent_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "existing-dir-skill");

        // Create a real directory at the target location.
        fs::create_dir_all(agent_dir.join("existing-dir-skill")).unwrap();

        let result =
            install_skill_to_agent_copy_impl(&pool, "existing-dir-skill", "claude-code").await;
        assert!(
            result.is_err(),
            "copy install should refuse to overwrite an existing real directory"
        );
    }

    // ── uninstall (copy) ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_uninstall_removes_copied_directory() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "uninstall-copy-skill");

        // First, install via copy.
        install_skill_to_agent_copy_impl(&pool, "uninstall-copy-skill", "claude-code")
            .await
            .unwrap();

        let target = agent_dir.join("uninstall-copy-skill");
        assert!(
            target.is_dir(),
            "copied directory should exist before uninstall"
        );

        // Now uninstall.
        uninstall_skill_from_agent_impl(&pool, "uninstall-copy-skill", "claude-code")
            .await
            .unwrap();

        assert!(
            fs::symlink_metadata(&target).is_err(),
            "copied directory should have been removed after uninstall"
        );
    }

    #[tokio::test]
    async fn test_uninstall_copy_removes_db_record() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "db-copy-uninstall-skill");

        install_skill_to_agent_copy_impl(&pool, "db-copy-uninstall-skill", "claude-code")
            .await
            .unwrap();

        uninstall_skill_from_agent_impl(&pool, "db-copy-uninstall-skill", "claude-code")
            .await
            .unwrap();

        let installations = db::get_skill_installations(&pool, "db-copy-uninstall-skill")
            .await
            .unwrap();
        assert!(
            installations.is_empty(),
            "DB record should be removed after uninstall"
        );
    }

    #[tokio::test]
    async fn test_uninstall_refuses_real_dir_without_copy_record() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;

        // Place a real directory with NO DB record as 'copy' type.
        fs::create_dir_all(agent_dir.join("protected-skill")).unwrap();

        let result = uninstall_skill_from_agent_impl(&pool, "protected-skill", "claude-code").await;
        assert!(
            result.is_err(),
            "uninstall should refuse to delete a real directory without a copy record"
        );

        // Ensure the directory still exists.
        assert!(
            agent_dir.join("protected-skill").is_dir(),
            "real directory should NOT have been deleted"
        );
    }

    #[tokio::test]
    async fn test_batch_install_uses_copy_method() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "batch-copy-skill");

        let mut succeeded = Vec::new();
        let mut failed = Vec::new();
        for agent_id in &["claude-code".to_string()] {
            match install_skill_to_agent_copy_impl(&pool, "batch-copy-skill", agent_id).await {
                Ok(_) => succeeded.push(agent_id.clone()),
                Err(e) => failed.push(FailedInstall {
                    agent_id: agent_id.clone(),
                    error: e,
                }),
            }
        }

        assert_eq!(succeeded.len(), 1);
        assert!(failed.is_empty());

        // The installed directory must NOT be a symlink.
        let target = agent_dir.join("batch-copy-skill");
        let meta = fs::symlink_metadata(&target).unwrap();
        assert!(
            !meta.file_type().is_symlink(),
            "batch copy install should create a real directory"
        );
    }

    #[tokio::test]
    async fn plugin_bundle_copies_all_skills_and_installs_to_two_agents() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let claude_dir = tmp.path().join("claude");
        let cursor_dir = tmp.path().join("cursor");
        let plugin_root = tmp.path().join("plugins/ponytail/1.0.0");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &claude_dir).await;
        set_agent_dir(&pool, "cursor", &cursor_dir).await;
        create_plugin_observation(
            &pool,
            &plugin_root,
            "ponytail@official",
            "ponytail-audit",
            "Ponytail Audit",
            true,
        )
        .await;
        create_plugin_observation(
            &pool,
            &plugin_root,
            "ponytail@official",
            "ponytail-review",
            "Ponytail Review",
            true,
        )
        .await;

        let result = install_plugin_skill_bundle_to_agents_impl(
            &pool,
            "claude-code",
            "ponytail@official",
            &["claude-code".to_string(), "cursor".to_string()],
        )
        .await
        .unwrap();

        assert_eq!(result.imported, vec!["ponytail-audit", "ponytail-review"]);
        assert!(result.skipped.is_empty());
        assert_eq!(result.succeeded.len(), 4);
        assert!(result.failed.is_empty());
        for skill_id in ["ponytail-audit", "ponytail-review"] {
            let canonical = central_dir.join("ponytail-official").join(skill_id);
            assert!(canonical.join("SKILL.md").is_file());
            assert!(fs::symlink_metadata(claude_dir.join(skill_id))
                .unwrap()
                .file_type()
                .is_symlink());
            assert!(fs::symlink_metadata(cursor_dir.join(skill_id))
                .unwrap()
                .file_type()
                .is_symlink());
        }
    }

    #[tokio::test]
    async fn plugin_bundle_skips_existing_skill_without_overwriting_it() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let claude_dir = tmp.path().join("claude");
        let plugin_root = tmp.path().join("plugins/ponytail/1.0.0");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &claude_dir).await;
        let existing_dir = create_central_skill(&central_dir, "shared-skill");
        fs::write(existing_dir.join("SKILL.md"), "user-owned content").unwrap();
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "shared-skill".to_string(),
                name: "Shared Skill".to_string(),
                description: None,
                file_path: existing_dir.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: Some(existing_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("native".to_string()),
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        create_plugin_observation(
            &pool,
            &plugin_root,
            "ponytail@official",
            "shared-skill",
            "Plugin Shared Skill",
            true,
        )
        .await;

        let result = install_plugin_skill_bundle_to_agents_impl(
            &pool,
            "claude-code",
            "ponytail@official",
            &["claude-code".to_string()],
        )
        .await
        .unwrap();

        assert!(result.imported.is_empty());
        assert_eq!(result.skipped, vec!["shared-skill"]);
        assert!(result.succeeded.is_empty());
        assert_eq!(
            fs::read_to_string(existing_dir.join("SKILL.md")).unwrap(),
            "user-owned content"
        );
    }

    #[tokio::test]
    async fn plugin_bundle_copy_failure_removes_every_created_skill() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let claude_dir = tmp.path().join("claude");
        let plugin_root = tmp.path().join("plugins/ponytail/1.0.0");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &claude_dir).await;
        create_plugin_observation(
            &pool,
            &plugin_root,
            "ponytail@official",
            "first-good",
            "A Good Skill",
            true,
        )
        .await;
        create_plugin_observation(
            &pool,
            &plugin_root,
            "ponytail@official",
            "second-missing",
            "B Missing Skill",
            false,
        )
        .await;

        let result = install_plugin_skill_bundle_to_agents_impl(
            &pool,
            "claude-code",
            "ponytail@official",
            &["claude-code".to_string()],
        )
        .await;

        assert!(result.is_err());
        assert!(!central_dir.join("ponytail-official").exists());
        assert!(db::get_skill_by_id(&pool, "first-good")
            .await
            .unwrap()
            .is_none());
        assert!(!claude_dir.join("first-good").exists());
    }
}

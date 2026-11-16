use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::agents::is_agent_detected;
use crate::commands::recovery;
use crate::commands::skill_origin;
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

/// `source`를 가리키는 설치 링크를 `agent_dir` 안의 `link` 자리에 만든다.
///
/// 플랫폼 폴더 자체가 링크일 수 있으므로, 링크가 실제로 놓이는 위치(`agent_dir`의 실제 경로)
/// 기준의 상대 경로로 만든다. 기록 전에 만든 링크로 SKILL.md를 읽을 수 있는지 확인하고,
/// 읽지 못하면 링크를 지우고 이유를 돌려준다.
pub fn create_verified_symlink(agent_dir: &Path, source: &Path, link: &Path) -> Result<(), String> {
    let link_parent = std::fs::canonicalize(agent_dir)
        .map_err(|e| format!("Failed to resolve agent skills directory: {}", e))?;
    let source = std::fs::canonicalize(source).map_err(|e| {
        format!(
            "Failed to resolve skill directory '{}': {}",
            source.display(),
            e
        )
    })?;
    create_symlink(&symlink_target_path(&link_parent, &source), link)?;
    if let Err(error) = std::fs::read(link.join("SKILL.md")) {
        let _ = std::fs::remove_file(link);
        return Err(format!(
            "Installed link '{}' does not resolve to the skill: {}",
            link.display(),
            error
        ));
    }
    Ok(())
}

// ─── Atomic Directory Copy ────────────────────────────────────────────────────

/// 복사 임시 폴더 이름에 들어가는 표식. 스캐너가 이 폴더를 스킬로 읽지 않도록 같은 값을 쓴다.
pub(crate) const COPY_STAGE_MARKER: &str = ".skillsmanage-copy-";

/// 스킬 폴더를 `dst`로 "전부 되거나 아무것도 안 되게" 복사한다.
///
/// `dst` 옆의 임시 폴더에 전부 복사하고 원본과 비교해 검증한 뒤, 이름 바꾸기 한 번으로
/// 확정한다. 중간에 실패하면 임시 폴더를 지우고 이유를 돌려주므로 `dst`에는 아무것도
/// 남지 않는다. `dst`가 이미 있으면 기존 폴더와 섞어 쓰지 않고 중단한다.
///
/// 링크 규칙: 스킬 안을 가리키는 스킬 안의 링크는 링크로 보존하고, 스킬 밖을 가리키는
/// 링크는 따라가지 않고 이유와 함께 중단한다. `src` 자체가 링크이면 가리키는 폴더를 스킬로 본다.
pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    if std::fs::symlink_metadata(dst).is_ok() {
        return Err(format!(
            "대상 폴더가 이미 있어 복사를 중단했습니다. 기존 폴더와 섞어 쓰지 않습니다: '{}'",
            dst.display()
        ));
    }
    let parent = dst
        .parent()
        .ok_or_else(|| format!("대상 경로에 상위 폴더가 없습니다: '{}'", dst.display()))?;
    std::fs::create_dir_all(parent).map_err(|e| {
        format!(
            "대상 상위 폴더를 만들 수 없습니다 '{}': {}",
            parent.display(),
            e
        )
    })?;
    let name = dst.file_name().unwrap_or_default().to_string_lossy();
    let stage = parent.join(format!(
        ".{name}{COPY_STAGE_MARKER}{}",
        uuid::Uuid::new_v4()
    ));

    let result = copy_into_stage(src, &stage).and_then(|()| {
        std::fs::rename(&stage, dst).map_err(|e| {
            format!(
                "복사본을 '{}'(으)로 확정하지 못했습니다: {}",
                dst.display(),
                e
            )
        })
    });
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&stage);
    }
    result
}

/// `stage`에 `src`의 복사본을 만들고 원본과 같은지 확인한다.
fn copy_into_stage(src: &Path, stage: &Path) -> Result<(), String> {
    let root = std::fs::canonicalize(src)
        .map_err(|e| format!("원본 폴더를 열 수 없습니다 '{}': {}", src.display(), e))?;
    copy_tree_into(&root, &root, stage)?;
    if tree_snapshot(&root)? != tree_snapshot(stage)? {
        return Err(format!(
            "복사하는 동안 원본이 바뀌었거나 복사본이 원본과 달라 중단했습니다: '{}'",
            src.display()
        ));
    }
    Ok(())
}

fn copy_tree_into(root: &Path, src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir(dst).map_err(|e| {
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

        if file_type.is_symlink() {
            copy_inner_link(root, &src_path, &dst_path)?;
        } else if file_type.is_dir() {
            copy_tree_into(root, &src_path, &dst_path)?;
        } else if file_type.is_file() {
            std::fs::copy(&src_path, &dst_path).map_err(|e| {
                format!(
                    "Failed to copy '{}' -> '{}': {}",
                    src_path.display(),
                    dst_path.display(),
                    e
                )
            })?;
        } else {
            return Err(format!(
                "복사할 수 없는 파일 종류입니다: '{}'",
                src_path.display()
            ));
        }
    }

    Ok(())
}

/// 스킬 안을 가리키는 링크를 링크로 만든다. 스킬 밖을 가리키거나 대상을 확인할 수 없으면 중단한다.
fn copy_inner_link(root: &Path, src: &Path, dst: &Path) -> Result<(), String> {
    let target = std::fs::read_link(src)
        .map_err(|e| format!("링크를 읽을 수 없습니다 '{}': {}", src.display(), e))?;
    let resolved = std::fs::canonicalize(src).map_err(|e| {
        format!(
            "링크 '{}'의 대상을 확인할 수 없어 복사를 중단했습니다: {}",
            src.display(),
            e
        )
    })?;
    if !resolved.starts_with(root) {
        return Err(format!(
            "스킬 밖을 가리키는 링크 '{}' -> '{}'가 있어 복사를 중단했습니다. 링크는 따라가지 않습니다.",
            src.display(),
            target.display()
        ));
    }
    // 절대 경로 링크는 복사본 안에서도 스킬 안을 가리키도록 상대 경로로 바꾼다.
    let target = if target.is_absolute() {
        let link_dir = std::fs::canonicalize(src.parent().unwrap_or(root))
            .map_err(|e| format!("링크 위치를 확인할 수 없습니다 '{}': {}", src.display(), e))?;
        make_relative_path(&link_dir, &resolved)
    } else {
        target
    };
    create_symlink(&target, dst)
}

/// 폴더 안 항목의 (상대 경로, 종류, 파일 크기) 목록. 복사본 검증에 쓴다.
fn tree_snapshot(root: &Path) -> Result<Vec<(PathBuf, char, u64)>, String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<(PathBuf, char, u64)>) -> Result<(), String> {
        for entry in std::fs::read_dir(dir)
            .map_err(|e| format!("Failed to read directory '{}': {}", dir.display(), e))?
        {
            let path = entry
                .map_err(|e| format!("Failed to read directory entry: {}", e))?
                .path();
            let meta = std::fs::symlink_metadata(&path)
                .map_err(|e| format!("Failed to inspect '{}': {}", path.display(), e))?;
            let (kind, len) = if meta.is_dir() {
                ('d', 0)
            } else if meta.file_type().is_symlink() {
                ('l', 0)
            } else {
                ('f', meta.len())
            };
            out.push((
                path.strip_prefix(root).unwrap_or(&path).to_path_buf(),
                kind,
                len,
            ));
            if kind == 'd' {
                walk(root, &path, out)?;
            }
        }
        Ok(())
    }

    let mut entries = Vec::new();
    walk(root, root, &mut entries)?;
    entries.sort();
    Ok(entries)
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
    // Look up the skill's actual file location from the database.
    let skill = db::get_skill_by_id(pool, skill_id).await?;

    // SKILL.md가 있다는 것만으로 완성된 원본으로 보지 않는다. 보관함 스킬로 기록된 폴더이거나
    // 기록 없이 보관함에 놓인 폴더만 확정된 원본이다. 그 밖의 폴더는 아래 복사가 섞어 쓰지 않고 중단한다.
    if canonical_dir.join("SKILL.md").exists() && skill.as_ref().is_none_or(|s| s.is_central) {
        return Ok(());
    }

    let skill = skill.ok_or_else(|| format!("Skill '{}' not found in database", skill_id))?;

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

    centralize_from(pool, skill, source_dir, canonical_dir).await
}

/// `source_dir`을 보관함 원본 `canonical_dir`로 원자적으로 복사하고, 스킬 기록을 보관함 스킬로 바꾼다.
pub(crate) async fn centralize_from(
    pool: &DbPool,
    skill: db::Skill,
    source_dir: &Path,
    canonical_dir: &Path,
) -> Result<(), String> {
    // Copy to central directory.
    copy_dir_all(source_dir, canonical_dir)?;

    // Update the DB record to reflect centralization.
    let skill_id = skill.id.clone();
    let mut updated = skill;
    updated.canonical_path = Some(canonical_dir.to_string_lossy().into_owned());
    updated.is_central = true;
    updated.file_path = canonical_dir
        .join("SKILL.md")
        .to_string_lossy()
        .into_owned();
    db::upsert_skill(pool, &updated).await?;

    skill_origin::inherit_copied_origin(pool, source_dir, canonical_dir, &skill_id, None).await?;
    super::skill_groups::inherit_group_source(pool, source_dir, canonical_dir).await?;

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
        .map(|id| {
            agents
                .iter()
                .find(|agent| agent.id == *id)
                .ok_or_else(|| format!("Agent '{}' not found", id))
        })
        .collect::<Result<Vec<_>, _>>()?;

    for agent in &selected {
        if agent.id != "universal" && !is_agent_detected(agent) {
            return Err(format!(
                "Agent '{}' is not installed on this machine",
                agent.id
            ));
        }
    }

    let universal_root = agents
        .iter()
        .find(|a| a.id == "universal")
        .map(|a| Path::new(&a.global_skills_dir));
    let central_root = agents
        .iter()
        .find(|a| a.id == "central")
        .map(|a| Path::new(&a.global_skills_dir));
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
            // Cursor처럼 다른 플랫폼 폴더도 읽는 대상은 설치 순서와 무관하게 중복을 막는다.
            // 공용 설치는 아래의 기존 전환 규칙에서 별도로 처리한다.
            if left.id != "universal" && right.id != "universal" {
                for (reader, source) in [(left, right), (right, left)] {
                    if super::scanner::loading_paths_for_agent(reader, universal_root, central_root)
                        .iter()
                        .any(|root| {
                            paths_refer_to_same_location(root, Path::new(&source.global_skills_dir))
                        })
                    {
                        return Err(format!("{}는 {}의 스킬 폴더도 읽습니다. 같은 스킬이 중복되지 않도록 {}만 선택하세요.", reader.display_name, source.display_name, source.display_name));
                    }
                }
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
    if requested_agent.id != "universal" && !is_agent_detected(&requested_agent) {
        return Err(format!(
            "Agent '{}' is not installed on this machine",
            requested_agent_id
        ));
    }
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
        let target_path = PathBuf::from(&requested_agent.global_skills_dir).join(skill_id);
        super::skill_duplicates::check_install(
            pool,
            skill_id,
            &requested_agent,
            &target_path,
            &canonical_dir,
        )
        .await?;
        ensure_centralized(pool, skill_id, &canonical_dir).await?;
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

    super::skill_duplicates::check_install(
        pool,
        skill_id,
        &target_agent,
        &target_path,
        &canonical_dir,
    )
    .await?;

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
    if db::get_paused_installation(pool, skill_id, &target_agent.id)
        .await?
        .is_some()
    {
        return Err("비활성 설치가 있습니다. 활성 토글로 먼저 복원하세요.".to_string());
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
    if let Err(error) = create_verified_symlink(&agent_dir, &canonical_dir, &symlink_path) {
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
pub(crate) fn should_fallback_to_copy(error: &str) -> bool {
    error.contains("Failed to create symlink")
}

#[cfg(not(windows))]
pub(crate) fn should_fallback_to_copy(_error: &str) -> bool {
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
    if db::get_paused_installation(pool, skill_id, &target_agent.id)
        .await?
        .is_some()
    {
        return Err("비활성 설치가 있습니다. 활성 토글로 먼저 복원하세요.".to_string());
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
    if let Err(error) =
        super::skill_groups::inherit_group_source(pool, &canonical_dir, &target_path).await
    {
        return Err(rollback_error(pool, &installation, &rollback, error).await);
    }
    persist_install(pool, &installation, &universal_duplicates, &rollback).await?;

    if let Err(error) = skill_origin::inherit_copied_origin(
        pool,
        &canonical_dir,
        &target_path,
        skill_id,
        Some(&installation.agent_id),
    )
    .await
    {
        eprintln!("Could not retain GitHub origin for copied installation: {error}");
    }

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
    db::ensure_installation_managed(pool, skill_id, agent_id).await?;

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

    if db::get_paused_installation(pool, skill_id, &agent.id)
        .await?
        .is_some()
    {
        return Err(
            "비활성 설치는 제거할 수 없습니다. 활성 토글로 복원하거나 비활성 상태를 유지하세요."
                .to_string(),
        );
    }

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
    if agent.id == "universal"
        && !super::shared_delete::find_links(pool, &install_path)
            .await?
            .is_empty()
    {
        return Err("다른 플랫폼의 바로가기가 연결되어 있습니다. 공용 설치 관리에서 연결 목록을 확인한 뒤 함께 삭제하세요".into());
    }
    // 복사 설치는 백업 생성부터 DB 기록 정리까지 하나의 복구 잠금으로 처리합니다.
    let _recovery_guard = if link_type == "copy" {
        Some(recovery::recovery_lock().await)
    } else {
        None
    };

    // 3. 검사한 항목만 제거합니다. 복사 설치는 먼저 전체를 백업합니다.
    match std::fs::symlink_metadata(&install_path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            // Always safe to remove symlinks.
            std::fs::remove_file(&install_path)
                .map_err(|e| format!("Failed to remove symlink: {}", e))?;
        }
        Ok(meta) if meta.is_dir() => {
            // Only remove real directories that were explicitly installed as copies.
            if link_type == "copy" {
                let installation = record.ok_or_else(|| {
                    "복사 설치 기록이 없어 안전하게 제거할 수 없습니다".to_string()
                })?;
                recovery::backup_copy_installation_locked(pool, installation).await?;
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
    if link_type == "copy" {
        skill_origin::discard_origin_bindings(
            pool,
            &[skill_origin::origin_target_key(&install_path)],
        )
        .await?;
    }

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
            match install_skill_to_agent_auto_impl(pool, skill_id, agent_id).await {
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

/// 원본 플러그인 라벨을 서로 겹치지 않는 안전한 경로 이름으로 바꾼다.
/// 소문자 영숫자와 `-`, `_`만 유지하고 나머지 UTF-8 바이트는 `~xx`로 인코딩한다.
pub(crate) fn plugin_bundle_directory_name(source_label: &str) -> Result<String, String> {
    if source_label.trim().is_empty() {
        return Err("Plugin source label is invalid".to_string());
    }

    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut name = String::new();
    for byte in source_label.bytes() {
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_') {
            name.push(char::from(byte));
        } else {
            name.push('~');
            name.push(char::from(HEX[usize::from(byte >> 4)]));
            name.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
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
    row_id: Option<&str>,
) -> Result<(Vec<String>, Vec<String>), String> {
    let observations = db::get_agent_skill_observations(pool, source_agent_id)
        .await?
        .into_iter()
        .filter(|observation| {
            observation.source_kind == "plugin"
                && observation.source_label.as_deref() == Some(source_label)
                && row_id.is_none_or(|row_id| observation.row_id == row_id)
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
            source: Some(row_id.map_or_else(
                || format!("plugin:{source_label}"),
                |row_id| format!("plugin-row:{source_agent_id}:{row_id}"),
            )),
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
        centralize_plugin_bundle_impl(pool, source_agent_id, source_label, None).await?;
    let installs = batch_install_skills_to_agents_impl(pool, &imported, agent_ids).await?;
    Ok(SkillBundleInstallResult {
        imported,
        skipped,
        succeeded: installs.succeeded,
        failed: installs.failed,
    })
}

#[derive(Debug, Clone, Deserialize)]
pub struct SkillTransferSource {
    pub skill_id: String,
    pub source_agent_id: Option<String>,
    pub row_id: Option<String>,
}

/// 선택한 원본만 보관함으로 복사한 뒤 대상 하네스에 추가 설치한다.
pub async fn transfer_skills_to_agents_impl(
    pool: &DbPool,
    sources: &[SkillTransferSource],
    agent_ids: &[String],
) -> Result<BatchInstallResult, String> {
    if sources.is_empty() || agent_ids.is_empty() {
        return Err("스킬과 대상 하네스를 선택해 주세요.".to_string());
    }
    // 공용 설치로의 전환은 기존 설치를 정리하므로 원본 보존 이식에서 제외한다.
    if validate_batch_install_targets(pool, agent_ids).await? {
        return Err("이식할 개별 하네스를 선택해 주세요.".to_string());
    }
    let mut result = BatchInstallResult {
        succeeded: Vec::new(),
        failed: Vec::new(),
    };
    for source in sources {
        if agent_ids
            .iter()
            .all(|id| Some(id.as_str()) == source.source_agent_id.as_deref())
        {
            result.failed.push(FailedInstall {
                agent_id: source.skill_id.clone(),
                error: "원본과 다른 하네스를 선택해 주세요.".to_string(),
            });
            continue;
        }
        let prepared = async {
            if let Some(row_id) = source.row_id.as_deref() {
                let agent_id = source
                    .source_agent_id
                    .as_deref()
                    .ok_or_else(|| "스킬의 원본 하네스가 없습니다.".to_string())?;
                let observation = db::get_agent_skill_observations(pool, agent_id)
                    .await?
                    .into_iter()
                    .find(|row| row.row_id == row_id && row.skill_id == source.skill_id)
                    .ok_or_else(|| {
                        "선택한 원본을 찾을 수 없습니다. 목록을 새로고침해 주세요.".to_string()
                    })?;
                if observation.source_kind != "plugin" {
                    return Err("이 출처의 스킬은 직접 이식할 수 없습니다.".to_string());
                }
                let label = observation
                    .source_label
                    .as_deref()
                    .ok_or_else(|| "플러그인 출처가 없습니다.".to_string())?;
                let (_, skipped) =
                    centralize_plugin_bundle_impl(pool, agent_id, label, Some(row_id)).await?;
                if !skipped.is_empty() {
                    // 같은 원본으로 만든 보관본만 재사용한다. 다른 출처는 덮어쓰지 않는다.
                    let existing = db::get_skill_by_id(pool, &source.skill_id).await?;
                    let expected_source = format!("plugin-row:{agent_id}:{row_id}");
                    if existing.is_some_and(|skill| {
                        skill.source.as_deref() == Some(expected_source.as_str())
                            && skill.is_central
                            && Path::new(&skill.file_path).is_file()
                    }) {
                        return Ok(());
                    }
                    return Err(
                        "같은 이름의 스킬이 보관함에 있습니다. 기존 스킬을 확인해 주세요."
                            .to_string(),
                    );
                }
            }
            Ok(())
        }
        .await;
        if let Err(error) = prepared {
            for agent_id in agent_ids {
                result.failed.push(FailedInstall {
                    agent_id: format!("{}:{agent_id}", source.skill_id),
                    error: error.clone(),
                });
            }
            continue;
        }
        let targets = agent_ids
            .iter()
            .filter(|id| Some(id.as_str()) != source.source_agent_id.as_deref())
            .cloned()
            .collect::<Vec<_>>();
        let installed = batch_install_skills_to_agents_impl(
            pool,
            std::slice::from_ref(&source.skill_id),
            &targets,
        )
        .await?;
        result.succeeded.extend(installed.succeeded);
        result.failed.extend(installed.failed);
    }
    Ok(result)
}

#[tauri::command]
pub async fn transfer_skills_to_agents(
    state: State<'_, AppState>,
    sources: Vec<SkillTransferSource>,
    agent_ids: Vec<String>,
) -> Result<BatchInstallResult, String> {
    transfer_skills_to_agents_impl(&state.db, &sources, &agent_ids).await
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

    #[cfg(windows)]
    #[test]
    fn windows_symlink_errors_use_copy_fallback() {
        assert!(should_fallback_to_copy(
            "Failed to create symlink: privilege not held"
        ));
        assert!(!should_fallback_to_copy("Failed to copy directory"));
    }

    // ── Test helpers ──────────────────────────────────────────────────────────

    /// Create an in-memory SQLite pool with the full schema initialised and
    /// the central/claude-code agent directories redirected to `central_dir`
    /// and `agent_dir` respectively.
    async fn setup_db(central_dir: &Path, agent_dir: &Path) -> DbPool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();
        mark_builtin_agent_installed(agent_dir);

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

    fn mark_builtin_agent_installed(global_skills_dir: &Path) {
        let platform_root = global_skills_dir.parent().unwrap();
        fs::create_dir_all(platform_root).unwrap();
        fs::write(platform_root.join("settings.json"), "{}").unwrap();
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

    #[test]
    fn plugin_bundle_directory_name_keeps_distinct_labels_separate() {
        let at_label = plugin_bundle_directory_name("foo@bar").unwrap();
        let dash_label = plugin_bundle_directory_name("foo-bar").unwrap();
        let uppercase_label = plugin_bundle_directory_name("Foo@bar").unwrap();
        let escaped_text_label = plugin_bundle_directory_name("foo~40bar").unwrap();

        assert_eq!(at_label, "foo~40bar");
        assert_eq!(dash_label, "foo-bar");
        assert_ne!(at_label, dash_label);
        assert_ne!(at_label, uppercase_label);
        assert_ne!(at_label, escaped_text_label);
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
        mark_builtin_agent_installed(&cursor_dir);
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
        fs::create_dir_all(tmp.path().join(".codex")).unwrap();
        fs::write(tmp.path().join(".codex/settings.json"), "{}").unwrap();
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
        mark_builtin_agent_installed(&cursor_dir);
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
        mark_builtin_agent_installed(&cursor_dir);
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
        mark_builtin_agent_installed(&cursor_dir);
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
        mark_builtin_agent_installed(&cursor_dir);
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

    #[cfg(unix)]
    #[tokio::test]
    async fn test_install_through_symlinked_platform_dir_points_to_central() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        // 플랫폼 폴더가 깊이가 다른 위치를 가리키는 링크다.
        let real_dir = tmp.path().join("deep/er/real-skills");
        let agent_dir = tmp.path().join("claude/skills");
        fs::create_dir_all(&central_dir).unwrap();
        fs::create_dir_all(&real_dir).unwrap();
        fs::create_dir_all(agent_dir.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&real_dir, &agent_dir).unwrap();

        let pool = setup_db(&central_dir, &agent_dir).await;
        create_central_skill(&central_dir, "linked-dir-skill");

        install_skill_to_agent_impl(&pool, "linked-dir-skill", "claude-code")
            .await
            .unwrap();

        let link = agent_dir.join("linked-dir-skill");
        assert_eq!(
            fs::canonicalize(&link).unwrap(),
            fs::canonicalize(central_dir.join("linked-dir-skill")).unwrap()
        );
        assert!(link.join("SKILL.md").exists());
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

        let result = batch_install_to_agents_impl(
            &pool,
            "partial-skill",
            &[
                "claude-code".to_string(),
                "nonexistent-agent".to_string(), // will fail
            ],
            "symlink",
        )
        .await;

        assert!(result.is_err());
        assert!(!claude_dir.join("partial-skill").exists());
        assert!(db::get_skill_installations(&pool, "partial-skill")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn test_batch_install_rejects_undetected_builtin_before_creating_directory() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let augment_dir = tmp.path().join(".augment/skills");
        fs::create_dir_all(&central_dir).unwrap();

        let pool = setup_db(&central_dir, &tmp.path().join("claude")).await;
        set_agent_dir(&pool, "augment", &augment_dir).await;
        create_central_skill(&central_dir, "guarded-skill");

        let result = batch_install_to_agents_impl(
            &pool,
            "guarded-skill",
            &["augment".to_string()],
            "symlink",
        )
        .await;

        assert!(result.is_err());
        assert!(!augment_dir.exists());
        assert!(db::get_skill_installations(&pool, "guarded-skill")
            .await
            .unwrap()
            .is_empty());
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
        mark_builtin_agent_installed(&cursor_dir);
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
    async fn transfer_selected_plugin_row_preserves_originals_and_omits_other_skills() {
        let tmp = TempDir::new().unwrap();
        let central = tmp.path().join("central");
        let claude = tmp.path().join("claude");
        let cursor = tmp.path().join("cursor");
        let plugin = tmp.path().join("plugin");
        let other_plugin = tmp.path().join("other-plugin");
        let pool = setup_db(&central, &claude).await;
        set_agent_dir(&pool, "cursor", &cursor).await;
        mark_builtin_agent_installed(&cursor);
        create_plugin_observation(&pool, &plugin, "first", "chosen", "Chosen original", true).await;
        create_plugin_observation(&pool, &plugin, "first", "unselected", "Leave here", true).await;
        create_plugin_observation(
            &pool,
            &other_plugin,
            "second",
            "chosen",
            "Different original",
            true,
        )
        .await;
        let source_path = plugin.join("skills/chosen/SKILL.md");
        let original = fs::read_to_string(&source_path).unwrap();
        let source = SkillTransferSource {
            skill_id: "chosen".into(),
            source_agent_id: Some("claude-code".into()),
            row_id: Some("claude-code::plugin::first::chosen".into()),
        };
        let result = transfer_skills_to_agents_impl(
            &pool,
            std::slice::from_ref(&source),
            &["cursor".into()],
        )
        .await
        .unwrap();
        assert_eq!(result.succeeded, vec!["chosen:cursor"]);
        assert!(result.failed.is_empty());
        let source_label: String = sqlx::query_scalar(
            "SELECT source_label FROM skill_group_sources WHERE target_path = ?",
        )
        .bind(
            central
                .join("first/chosen")
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .as_ref(),
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(source_label, "first");

        assert_eq!(
            fs::read_to_string(cursor.join("chosen/SKILL.md")).unwrap(),
            original
        );
        assert_eq!(fs::read_to_string(&source_path).unwrap(), original);
        assert!(!cursor.join("unselected").exists());
        assert!(!central.join("first/unselected").exists());
        assert!(db::get_skill_by_id(&pool, "unselected")
            .await
            .unwrap()
            .is_none());
        assert!(plugin.join("skills/unselected/SKILL.md").is_file());
        let retry = transfer_skills_to_agents_impl(
            &pool,
            std::slice::from_ref(&source),
            &["cursor".into()],
        )
        .await
        .unwrap();
        assert!(retry.failed.is_empty());
        assert_eq!(retry.succeeded, vec!["chosen:cursor"]);
        // 같은 이름의 다른 원본으로 바꾸려 해도 이미 이식한 파일을 덮어쓰지 않는다.
        let conflicting = SkillTransferSource {
            row_id: Some("claude-code::plugin::second::chosen".into()),
            ..source
        };
        let result = transfer_skills_to_agents_impl(&pool, &[conflicting], &["cursor".into()])
            .await
            .unwrap();
        assert_eq!(result.failed.len(), 1);
        assert!(result.succeeded.is_empty());
        assert_eq!(
            fs::read_to_string(cursor.join("chosen/SKILL.md")).unwrap(),
            original
        );
    }

    #[tokio::test]
    async fn transfer_reports_missing_skill_and_keeps_successful_installation() {
        let tmp = TempDir::new().unwrap();
        let central = tmp.path().join("central");
        let claude = tmp.path().join("claude");
        let pool = setup_db(&central, &claude).await;
        let original = create_central_skill(&central, "existing");
        let sources = ["missing", "existing"].map(|id| SkillTransferSource {
            skill_id: id.into(),
            source_agent_id: None,
            row_id: None,
        });
        let result = transfer_skills_to_agents_impl(&pool, &sources, &["claude-code".into()])
            .await
            .unwrap();
        assert_eq!(result.succeeded, vec!["existing:claude-code"]);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].agent_id, "missing:claude-code");
        assert!(original.join("SKILL.md").is_file());
        assert!(claude.join("existing/SKILL.md").is_file());
    }

    #[tokio::test]
    async fn transfer_rejects_same_source_before_importing() {
        let tmp = TempDir::new().unwrap();
        let central = tmp.path().join("central");
        let claude = tmp.path().join("claude");
        let pool = setup_db(&central, &claude).await;
        let source = SkillTransferSource {
            skill_id: "chosen".into(),
            source_agent_id: Some("claude-code".into()),
            row_id: None,
        };
        let result = transfer_skills_to_agents_impl(&pool, &[source], &["claude-code".into()])
            .await
            .unwrap();
        assert!(result.succeeded.is_empty());
        assert_eq!(result.failed.len(), 1);
        assert!(!central.exists());
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
        mark_builtin_agent_installed(&cursor_dir);
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
            let canonical = central_dir.join("ponytail~40official").join(skill_id);
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
        mark_builtin_agent_installed(&claude_dir);
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
        mark_builtin_agent_installed(&claude_dir);
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
        assert!(!central_dir.join("ponytail~40official").exists());
        assert!(db::get_skill_by_id(&pool, "first-good")
            .await
            .unwrap()
            .is_none());
        assert!(!claude_dir.join("first-good").exists());
    }

    // ── 원자적 복사와 링크 규칙 ───────────────────────────────────────────────

    #[cfg(unix)]
    fn lock_file(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
    }

    #[cfg(unix)]
    fn unlock_file(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
    }

    fn write_skill_md(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            "---\nname: foo\ndescription: Test skill\n---\n",
        )
        .unwrap();
    }

    fn dir_entry_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// 다른 플랫폼에만 있는 스킬(보관함 밖, 보관함 스킬이 아님)을 DB에 기록한다.
    async fn seed_platform_only_skill(pool: &DbPool, source_dir: &Path) {
        db::upsert_skill(
            pool,
            &db::Skill {
                id: "foo".to_string(),
                name: "foo".to_string(),
                description: Some("Test skill".to_string()),
                file_path: source_dir.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: None,
                is_central: false,
                source: Some("copy".to_string()),
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn copy_dir_all_leaves_nothing_and_names_the_cause_when_a_file_cannot_be_read() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        write_skill_md(&src);
        fs::write(src.join("locked.txt"), "x").unwrap();
        lock_file(&src.join("locked.txt"));
        let out = tmp.path().join("out");
        fs::create_dir_all(&out).unwrap();

        let error = copy_dir_all(&src, &out.join("foo")).unwrap_err();

        assert!(
            error.contains("locked.txt"),
            "reason should name the file: {error}"
        );
        assert!(
            dir_entry_names(&out).is_empty(),
            "nothing may remain in the destination parent"
        );
    }

    #[cfg(unix)]
    #[test]
    fn copy_dir_all_stops_at_a_link_pointing_outside_the_skill() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        write_skill_md(&src);
        let secret = tmp.path().join("secret.txt");
        fs::write(&secret, "outside").unwrap();
        std::os::unix::fs::symlink(&secret, src.join("notes.md")).unwrap();
        let out = tmp.path().join("out");
        fs::create_dir_all(&out).unwrap();

        let error = copy_dir_all(&src, &out.join("foo")).unwrap_err();

        assert!(
            error.contains("notes.md"),
            "reason should name the link: {error}"
        );
        assert!(
            dir_entry_names(&out).is_empty(),
            "outside content must not be copied"
        );
        assert_eq!(fs::read_to_string(&secret).unwrap(), "outside");
    }

    #[cfg(unix)]
    #[test]
    fn copy_dir_all_keeps_links_that_stay_inside_the_skill() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        write_skill_md(&src);
        fs::create_dir_all(src.join("docs")).unwrap();
        fs::write(src.join("docs/guide.md"), "guide").unwrap();
        std::os::unix::fs::symlink("docs/guide.md", src.join("latest.md")).unwrap();
        std::os::unix::fs::symlink(src.join("docs"), src.join("all-docs")).unwrap();
        let dst = tmp.path().join("out/foo");

        copy_dir_all(&src, &dst).unwrap();

        let relative = fs::symlink_metadata(dst.join("latest.md")).unwrap();
        assert!(relative.file_type().is_symlink());
        assert_eq!(fs::read_to_string(dst.join("latest.md")).unwrap(), "guide");
        // 절대 경로 링크도 복사본 안을 가리켜야 한다(원본 위치에 기대지 않는다).
        let absolute = fs::symlink_metadata(dst.join("all-docs")).unwrap();
        assert!(absolute.file_type().is_symlink());
        assert!(fs::canonicalize(dst.join("all-docs"))
            .unwrap()
            .starts_with(fs::canonicalize(&dst).unwrap()));
    }

    #[cfg(unix)]
    #[test]
    fn copy_dir_all_does_not_mix_into_an_existing_destination() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        write_skill_md(&src);
        let outside = tmp.path().join("outside.txt");
        fs::write(&outside, "outside").unwrap();
        let dst = tmp.path().join("foo");
        fs::create_dir_all(&dst).unwrap();
        fs::write(dst.join("keep.txt"), "keep").unwrap();
        std::os::unix::fs::symlink(&outside, dst.join("SKILL.md")).unwrap();

        let error = copy_dir_all(&src, &dst).unwrap_err();

        assert!(
            error.contains("이미 있"),
            "reason should say the destination exists: {error}"
        );
        assert_eq!(fs::read_to_string(&outside).unwrap(), "outside");
        assert_eq!(dir_entry_names(&dst), ["SKILL.md", "keep.txt"]);
        assert_eq!(dir_entry_names(tmp.path()), ["foo", "outside.txt", "src"]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_centralization_leaves_no_original_and_a_retry_completes_it() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &agent_dir).await;
        let source = tmp.path().join("other-platform/foo");
        write_skill_md(&source);
        fs::write(source.join("reference.md"), "reference").unwrap();
        lock_file(&source.join("reference.md"));
        seed_platform_only_skill(&pool, &source).await;

        let error = install_skill_to_agent_impl(&pool, "foo", "claude-code")
            .await
            .unwrap_err();

        assert!(
            error.contains("reference.md"),
            "reason should name the file: {error}"
        );
        assert!(
            !central_dir.join("foo").exists(),
            "no partial original may remain"
        );
        assert!(db::get_skill_installations(&pool, "foo")
            .await
            .unwrap()
            .is_empty());

        unlock_file(&source.join("reference.md"));
        install_skill_to_agent_impl(&pool, "foo", "claude-code")
            .await
            .unwrap();

        assert_eq!(
            fs::read_to_string(central_dir.join("foo/reference.md")).unwrap(),
            "reference"
        );
        assert!(
            db::get_skill_by_id(&pool, "foo")
                .await
                .unwrap()
                .unwrap()
                .is_central
        );
    }

    #[tokio::test]
    async fn central_folder_with_only_skill_md_is_not_used_as_the_original() {
        let tmp = TempDir::new().unwrap();
        let central_dir = tmp.path().join("central");
        let agent_dir = tmp.path().join("claude");
        fs::create_dir_all(&central_dir).unwrap();
        let pool = setup_db(&central_dir, &agent_dir).await;
        let source = tmp.path().join("other-platform/foo");
        write_skill_md(&source);
        fs::write(source.join("reference.md"), "reference").unwrap();
        seed_platform_only_skill(&pool, &source).await;
        // 예전 실패한 복사가 남긴 것처럼 SKILL.md만 있는 보관함 폴더
        write_skill_md(&central_dir.join("foo"));

        let result = install_skill_to_agent_impl(&pool, "foo", "claude-code").await;

        assert!(
            result.is_err(),
            "an unconfirmed central folder must not be installed from"
        );
        assert!(fs::symlink_metadata(agent_dir.join("foo")).is_err());
        assert_eq!(dir_entry_names(&central_dir.join("foo")), ["SKILL.md"]);
    }
}

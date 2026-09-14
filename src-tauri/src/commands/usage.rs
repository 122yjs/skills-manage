use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::State;
use tokio::sync::{Mutex, MutexGuard};
use uuid::Uuid;

use crate::commands::linker::uninstall_skill_from_agent_impl;
use crate::commands::recovery;
use crate::db::{self, DbPool, PausedInstallation, SkillInstallation};
use crate::AppState;

const PAUSED_INSTALLATIONS_DIR: &str = "paused-installations";

static USAGE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// 플랫폼별 관리 설치의 실제 사용 상태입니다.
#[derive(Debug, Clone, Serialize)]
pub struct UsageStatus {
    pub agent_id: String,
    pub active_count: usize,
    pub paused_count: usize,
    pub external_count: usize,
    pub skills: Vec<UsageSkill>,
}

/// 한 스킬의 실제 사용 상태입니다.
#[derive(Debug, Clone, Serialize)]
pub struct UsageSkill {
    pub skill_id: String,
    pub name: String,
    pub enabled: bool,
    pub paused_by_bulk: bool,
}

/// 플랫폼 전체 삭제에서 실제로 제거한 관리 설치와 실패 항목입니다.
#[derive(Debug, Clone, Serialize)]
pub struct DeletePlatformInstallationsResult {
    pub deleted: Vec<String>,
    pub failed: Vec<DeleteInstallationFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeleteInstallationFailure {
    pub skill_id: String,
    pub error: String,
}

async fn usage_lock() -> MutexGuard<'static, ()> {
    USAGE_LOCK.get_or_init(|| Mutex::new(())).lock().await
}

async fn database_parent_dir(pool: &DbPool) -> Result<PathBuf, String> {
    let rows = sqlx::query("PRAGMA database_list")
        .fetch_all(pool)
        .await
        .map_err(|error| format!("데이터베이스 위치를 확인할 수 없습니다: {error}"))?;
    let file = rows
        .iter()
        .find(|row| row.get::<String, _>("name") == "main")
        .map(|row| row.get::<String, _>("file"))
        .unwrap_or_default();

    if file.is_empty() || file == ":memory:" {
        return Err("비활성 설치는 파일 기반 데이터베이스에서만 사용할 수 있습니다".to_string());
    }

    let database_path = PathBuf::from(file);
    if !database_path.is_absolute() {
        return Err(format!(
            "데이터베이스 파일 위치가 절대 경로가 아닙니다: {}",
            database_path.display()
        ));
    }
    database_path
        .parent()
        .ok_or_else(|| "데이터베이스 상위 폴더를 확인할 수 없습니다".to_string())?
        .canonicalize()
        .map_err(|error| format!("데이터베이스 상위 폴더를 확인할 수 없습니다: {error}"))
}

fn ensure_real_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "심볼릭 링크를 비활성 설치 보관소로 사용할 수 없습니다: {}",
            path.display()
        )),
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(format!(
            "비활성 설치 보관소가 폴더가 아닙니다: {}",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or_else(|| {
                format!(
                    "비활성 설치 보관소의 상위 폴더가 없습니다: {}",
                    path.display()
                )
            })?;
            let parent_metadata = fs::symlink_metadata(parent).map_err(|parent_error| {
                format!(
                    "비활성 설치 보관소의 상위 폴더를 확인할 수 없습니다 '{}': {parent_error}",
                    parent.display()
                )
            })?;
            if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
                return Err(format!(
                    "안전하지 않은 비활성 설치 보관소 상위 경로입니다: {}",
                    parent.display()
                ));
            }
            fs::create_dir(path).map_err(|create_error| {
                format!(
                    "비활성 설치 보관소를 만들 수 없습니다 '{}': {create_error}",
                    path.display()
                )
            })
        }
        Err(error) => Err(format!(
            "비활성 설치 보관소를 확인할 수 없습니다 '{}': {error}",
            path.display()
        )),
    }
}

async fn paused_root(pool: &DbPool) -> Result<PathBuf, String> {
    let root = database_parent_dir(pool)
        .await?
        .join(PAUSED_INSTALLATIONS_DIR);
    ensure_real_directory(&root)?;
    Ok(root)
}

fn metadata_without_following_links(path: &Path) -> Result<fs::Metadata, String> {
    fs::symlink_metadata(path).map_err(|error| {
        format!(
            "설치 경로를 확인할 수 없습니다 '{}': {error}",
            path.display()
        )
    })
}

fn validate_installation_path(agent_root: &Path, installed_path: &Path) -> Result<(), String> {
    if !installed_path.is_absolute() {
        return Err(format!(
            "설치 경로가 절대 경로가 아닙니다: {}",
            installed_path.display()
        ));
    }
    let root = agent_root.canonicalize().map_err(|error| {
        format!(
            "플랫폼 스킬 폴더를 확인할 수 없습니다 '{}': {error}",
            agent_root.display()
        )
    })?;
    let parent = installed_path.parent().ok_or_else(|| {
        format!(
            "설치 경로의 상위 폴더가 없습니다: {}",
            installed_path.display()
        )
    })?;
    let canonical_parent = parent.canonicalize().map_err(|error| {
        format!(
            "설치 경로의 상위 폴더를 확인할 수 없습니다 '{}': {error}",
            parent.display()
        )
    })?;
    if !canonical_parent.starts_with(&root) || installed_path == root {
        return Err(format!(
            "플랫폼 스킬 폴더 밖의 설치는 비활성화할 수 없습니다: {}",
            installed_path.display()
        ));
    }
    Ok(())
}

fn validate_paused_path(root: &Path, paused_path: &Path) -> Result<(), String> {
    let parent = paused_path.parent().ok_or_else(|| {
        format!(
            "비활성 설치 경로의 상위 폴더가 없습니다: {}",
            paused_path.display()
        )
    })?;
    if parent != root || paused_path.file_name().is_none() {
        return Err(format!(
            "비활성 설치 보관소 밖의 경로는 복원할 수 없습니다: {}",
            paused_path.display()
        ));
    }
    Ok(())
}

fn move_path(source: &Path, destination: &Path) -> Result<(), String> {
    if fs::symlink_metadata(destination).is_ok() {
        return Err(format!(
            "대상 경로가 이미 있어 파일을 덮어쓰지 않습니다: {}",
            destination.display()
        ));
    }
    fs::rename(source, destination).map_err(|error| {
        format!(
            "설치 파일을 안전한 보관소로 옮길 수 없습니다 '{}': {error}",
            source.display()
        )
    })
}

fn rollback_move(source: &Path, destination: &Path) -> Result<(), String> {
    if fs::symlink_metadata(destination).is_ok() {
        return Err(format!(
            "되돌릴 원래 경로가 이미 있어 파일을 덮어쓰지 않습니다: {}",
            destination.display()
        ));
    }
    fs::rename(source, destination).map_err(|error| {
        format!(
            "비활성 설치 파일을 원래 위치로 되돌릴 수 없습니다 '{}': {error}",
            source.display()
        )
    })
}

fn active_skill_name(pool_skill: Option<db::Skill>, skill_id: &str) -> String {
    pool_skill
        .map(|skill| skill.name)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| skill_id.to_string())
}

fn validate_active_installation(
    agent_root: &Path,
    installation: &SkillInstallation,
) -> Result<Option<String>, String> {
    let installed_path = Path::new(&installation.installed_path);
    validate_installation_path(agent_root, installed_path)?;
    let metadata = metadata_without_following_links(installed_path)?;

    if metadata.file_type().is_symlink() {
        if installation.link_type != "symlink" {
            return Err(format!(
                "설치 방식 기록이 심볼릭 링크와 맞지 않습니다: {}",
                installed_path.display()
            ));
        }
        return fs::read_link(installed_path)
            .map(|target| Some(target.to_string_lossy().into_owned()))
            .map_err(|error| {
                format!(
                    "심볼릭 링크 대상을 읽을 수 없습니다 '{}': {error}",
                    installed_path.display()
                )
            });
    }

    if metadata.is_dir() && matches!(installation.link_type.as_str(), "copy" | "native") {
        return Ok(None);
    }

    Err(format!(
        "관리 설치 파일 형식을 확인할 수 없습니다: {}",
        installed_path.display()
    ))
}

fn same_install_entry(left: &Path, right: &Path) -> bool {
    if left == right {
        return true;
    }
    let Some(left_name) = left.file_name() else {
        return false;
    };
    let Some(right_name) = right.file_name() else {
        return false;
    };
    left_name == right_name
        && left
            .parent()
            .and_then(|parent| parent.canonicalize().ok())
            .zip(right.parent().and_then(|parent| parent.canonicalize().ok()))
            .is_some_and(|(left_parent, right_parent)| left_parent == right_parent)
}

fn resolved_link_target(link_path: &Path) -> Option<PathBuf> {
    let target = fs::read_link(link_path).ok()?;
    let target = if target.is_absolute() {
        target
    } else {
        link_path.parent()?.join(target)
    };
    target.canonicalize().ok()
}

async fn ensure_pause_does_not_move_shared_source(
    pool: &DbPool,
    installation: &SkillInstallation,
) -> Result<(), String> {
    let installed_path = Path::new(&installation.installed_path);
    let installed_target = installed_path.canonicalize().ok();
    for other in db::get_skill_installations(pool, &installation.skill_id).await? {
        if other.agent_id == installation.agent_id {
            continue;
        }
        if same_install_entry(installed_path, Path::new(&other.installed_path)) {
            return Err(format!(
                "다른 플랫폼과 같은 설치 경로를 공유하므로 비활성화할 수 없습니다: {}",
                installed_path.display()
            ));
        }
        if installation.link_type != "symlink"
            && other.link_type == "symlink"
            && installed_target.as_ref().is_some_and(|target| {
                resolved_link_target(Path::new(&other.installed_path))
                    .is_some_and(|other_target| other_target == *target)
            })
        {
            return Err(format!(
                "다른 플랫폼의 심볼릭 링크 원본이므로 비활성화할 수 없습니다: {}",
                installed_path.display()
            ));
        }
    }
    Ok(())
}

fn paths_refer_to_same_location(left: &Path, right: &Path) -> bool {
    left == right
        || left
            .canonicalize()
            .ok()
            .zip(right.canonicalize().ok())
            .is_some_and(|(left, right)| left == right)
}

async fn ensure_not_shared_universal_root(pool: &DbPool, agent: &db::Agent) -> Result<(), String> {
    if agent.id == "universal" {
        return Ok(());
    }
    if let Some(universal) = db::get_agent_by_id(pool, "universal").await? {
        if paths_refer_to_same_location(
            Path::new(&agent.global_skills_dir),
            Path::new(&universal.global_skills_dir),
        ) {
            return Err(
                "공용 설치 경로를 공유하는 플랫폼의 관리 설치는 여기서 삭제할 수 없습니다"
                    .to_string(),
            );
        }
    }
    Ok(())
}

fn resolved_paused_link_target(paused: &PausedInstallation) -> Option<PathBuf> {
    let target = PathBuf::from(paused.symlink_target.as_ref()?);
    let target = if target.is_absolute() {
        target
    } else {
        Path::new(&paused.installed_path).parent()?.join(target)
    };
    target.canonicalize().ok()
}

async fn ensure_delete_does_not_remove_shared_source(
    pool: &DbPool,
    installation: &SkillInstallation,
) -> Result<(), String> {
    let installed_path = Path::new(&installation.installed_path);
    let installed_target = installed_path.canonicalize().map_err(|error| {
        format!(
            "관리 설치 원본을 확인할 수 없습니다 '{}': {error}",
            installed_path.display()
        )
    })?;

    for other in db::get_skill_installations(pool, &installation.skill_id).await? {
        if other.agent_id == installation.agent_id {
            continue;
        }
        if same_install_entry(installed_path, Path::new(&other.installed_path))
            || (other.link_type == "symlink"
                && resolved_link_target(Path::new(&other.installed_path))
                    .is_some_and(|target| target == installed_target))
        {
            return Err(format!(
                "다른 플랫폼이 이 설치 원본을 사용하므로 삭제할 수 없습니다: {}",
                installed_path.display()
            ));
        }
    }
    for paused in db::get_paused_installations(pool, &installation.skill_id).await? {
        if paused.agent_id == installation.agent_id {
            continue;
        }
        if same_install_entry(installed_path, Path::new(&paused.installed_path))
            || (paused.link_type == "symlink"
                && resolved_paused_link_target(&paused)
                    .is_some_and(|target| target == installed_target))
        {
            return Err(format!(
                "다른 플랫폼의 비활성 설치가 이 원본을 사용하므로 삭제할 수 없습니다: {}",
                installed_path.display()
            ));
        }
    }
    Ok(())
}

async fn pause_active_installation_locked(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
    paused_by_bulk: bool,
) -> Result<(), String> {
    let agent = db::get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| format!("플랫폼 '{}'을(를) 찾을 수 없습니다", agent_id))?;
    if agent.id == "central" {
        return Err("중앙 보관함 스킬은 비활성화할 수 없습니다".to_string());
    }
    let installation = db::get_skill_installation(pool, skill_id, agent_id)
        .await?
        .ok_or_else(|| format!("관리 중인 설치를 찾을 수 없습니다: {}", skill_id))?;
    if db::get_paused_installation(pool, skill_id, agent_id)
        .await?
        .is_some()
    {
        return Err("활성 설치와 비활성 설치 기록이 함께 있어 파일을 옮기지 않습니다".to_string());
    }
    let raw_symlink_target =
        validate_active_installation(Path::new(&agent.global_skills_dir), &installation)?;
    ensure_pause_does_not_move_shared_source(pool, &installation).await?;
    let root = paused_root(pool).await?;
    let paused_path = root.join(Uuid::new_v4().to_string());
    let paused = PausedInstallation {
        skill_id: skill_id.to_string(),
        agent_id: agent_id.to_string(),
        skill_name: active_skill_name(db::get_skill_by_id(pool, skill_id).await?, skill_id),
        installed_path: installation.installed_path.clone(),
        paused_path: paused_path.to_string_lossy().into_owned(),
        link_type: installation.link_type.clone(),
        symlink_target: raw_symlink_target,
        paused_by_bulk,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    let original_path = Path::new(&installation.installed_path);

    // 파일을 옮기기 전에 중지 위치를 먼저 기록합니다. 파일 이동 뒤 DB 정리가
    // 실패해도 보관 파일의 위치를 잃지 않기 위한 순서입니다.
    db::upsert_paused_installation(pool, &paused).await?;
    if let Err(error) = move_path(original_path, &paused_path) {
        let cleanup = db::delete_paused_installation(pool, skill_id, agent_id).await;
        return Err(rollback_errors(error, cleanup.err(), None));
    }
    if let Err(error) = db::delete_skill_installation(pool, skill_id, agent_id).await {
        let rollback = rollback_move(&paused_path, original_path);
        if rollback.is_ok() {
            let cleanup = db::delete_paused_installation(pool, skill_id, agent_id).await;
            return Err(rollback_errors(error, cleanup.err(), None));
        }
        // 되돌리기가 실패하면 중지 기록을 남겨 보관 파일을 다시 찾을 수 있게 합니다.
        return Err(rollback_errors(error, None, rollback.err()));
    }
    Ok(())
}

async fn restore_paused_installation_locked(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
) -> Result<(), String> {
    let agent = db::get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| format!("플랫폼 '{}'을(를) 찾을 수 없습니다", agent_id))?;
    if agent.id == "central" {
        return Err("중앙 보관함 스킬은 활성/비활성 전환 대상이 아닙니다".to_string());
    }
    if db::get_skill_installation(pool, skill_id, agent_id)
        .await?
        .is_some()
    {
        return Err(format!("이미 활성 설치 기록이 있습니다: {}", skill_id));
    }
    let paused = db::get_paused_installation(pool, skill_id, agent_id)
        .await?
        .ok_or_else(|| format!("비활성 관리 설치를 찾을 수 없습니다: {}", skill_id))?;
    let root = paused_root(pool).await?;
    let paused_path = Path::new(&paused.paused_path);
    validate_paused_path(&root, paused_path)?;
    let metadata = metadata_without_following_links(paused_path)?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(paused_path).map_err(|error| {
            format!(
                "비활성 심볼릭 링크를 읽을 수 없습니다 '{}': {error}",
                paused_path.display()
            )
        })?;
        let target_text = target.to_string_lossy().into_owned();
        if paused.symlink_target.as_deref() != Some(target_text.as_str()) {
            return Err("비활성 심볼릭 링크 대상이 기록과 달라 복원을 중단했습니다".to_string());
        }
    } else if !metadata.is_dir() || !matches!(paused.link_type.as_str(), "copy" | "native") {
        return Err(format!(
            "비활성 관리 설치 파일 형식을 확인할 수 없습니다: {}",
            paused_path.display()
        ));
    }

    let installed_path = PathBuf::from(&paused.installed_path);
    let agent_root = PathBuf::from(&agent.global_skills_dir);
    if !agent_root.exists() {
        fs::create_dir_all(&agent_root).map_err(|error| {
            format!(
                "플랫폼 스킬 폴더를 만들 수 없습니다 '{}': {error}",
                agent_root.display()
            )
        })?;
    }
    validate_installation_path(&agent_root, &installed_path)?;
    if fs::symlink_metadata(&installed_path).is_ok() {
        return Err(format!(
            "원래 설치 경로가 이미 있어 파일을 덮어쓰지 않습니다: {}",
            installed_path.display()
        ));
    }

    move_path(paused_path, &installed_path)?;
    let active = SkillInstallation {
        skill_id: paused.skill_id.clone(),
        agent_id: paused.agent_id.clone(),
        installed_path: paused.installed_path.clone(),
        link_type: paused.link_type.clone(),
        symlink_target: paused.symlink_target.clone(),
        created_at: paused.created_at.clone(),
    };
    if let Err(error) = db::upsert_skill_installation(pool, &active).await {
        let rollback = rollback_move(&installed_path, paused_path);
        return Err(rollback_error(error, rollback));
    }
    if let Err(error) = db::delete_paused_installation(pool, skill_id, agent_id).await {
        let delete_active = db::delete_skill_installation(pool, skill_id, agent_id).await;
        let rollback = rollback_move(&installed_path, paused_path);
        return Err(rollback_errors(error, delete_active.err(), rollback.err()));
    }
    Ok(())
}

fn rollback_error(error: String, rollback: Result<(), String>) -> String {
    match rollback {
        Ok(()) => error,
        Err(rollback_error) => format!("{}; 되돌리기에 실패했습니다: {}", error, rollback_error),
    }
}

fn rollback_errors(error: String, db_error: Option<String>, move_error: Option<String>) -> String {
    let mut errors = vec![error];
    if let Some(db_error) = db_error {
        errors.push(format!("비활성 기록 되돌리기 실패: {db_error}"));
    }
    if let Some(move_error) = move_error {
        errors.push(move_error);
    }
    errors.join("; ")
}

async fn delete_active_installation_locked(
    pool: &DbPool,
    agent: &db::Agent,
    installation: SkillInstallation,
) -> Result<(), String> {
    let installed_path = Path::new(&installation.installed_path);
    let metadata = match fs::symlink_metadata(installed_path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "관리 설치 경로를 확인할 수 없습니다 '{}': {error}",
                installed_path.display()
            ))
        }
    };

    if metadata.is_none() {
        return db::delete_skill_installation(pool, &installation.skill_id, &installation.agent_id)
            .await;
    }

    let symlink_target =
        validate_active_installation(Path::new(&agent.global_skills_dir), &installation)?;
    if symlink_target.is_some() {
        uninstall_skill_from_agent_impl(pool, &installation.skill_id, &installation.agent_id)
            .await?;
        if db::get_skill_installation(pool, &installation.skill_id, &installation.agent_id)
            .await?
            .is_some()
        {
            return Err("공용 설치 경로의 관리 기록은 여기서 삭제할 수 없습니다".to_string());
        }
        return Ok(());
    }

    ensure_delete_does_not_remove_shared_source(pool, &installation).await?;
    let _recovery_guard = recovery::recovery_lock().await;
    recovery::backup_copy_installation_locked(pool, &installation).await?;
    recovery::remove_path_without_following_links(installed_path)?;
    db::delete_skill_installation(pool, &installation.skill_id, &installation.agent_id).await
}

async fn delete_paused_installation_locked(
    pool: &DbPool,
    paused: PausedInstallation,
) -> Result<(), String> {
    let root = paused_root(pool).await?;
    let paused_path = Path::new(&paused.paused_path);
    validate_paused_path(&root, paused_path)?;
    let metadata = match fs::symlink_metadata(paused_path) {
        Ok(metadata) => Some(metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "비활성 설치 경로를 확인할 수 없습니다 '{}': {error}",
                paused_path.display()
            ))
        }
    };

    if metadata.is_none() {
        return db::delete_paused_installation(pool, &paused.skill_id, &paused.agent_id).await;
    }

    let metadata = metadata.expect("missing paused installation is handled above");
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(paused_path).map_err(|error| {
            format!(
                "비활성 심볼릭 링크를 읽을 수 없습니다 '{}': {error}",
                paused_path.display()
            )
        })?;
        if paused.link_type != "symlink"
            || paused.symlink_target.as_deref() != Some(target.to_string_lossy().as_ref())
        {
            return Err("비활성 심볼릭 링크가 관리 기록과 달라 삭제하지 않습니다".to_string());
        }
        fs::remove_file(paused_path).map_err(|error| {
            format!(
                "비활성 심볼릭 링크를 지울 수 없습니다 '{}': {error}",
                paused_path.display()
            )
        })?;
        return db::delete_paused_installation(pool, &paused.skill_id, &paused.agent_id).await;
    }

    if !metadata.is_dir() || !matches!(paused.link_type.as_str(), "copy" | "native") {
        return Err(format!(
            "비활성 관리 설치 파일 형식을 확인할 수 없습니다: {}",
            paused_path.display()
        ));
    }

    let _recovery_guard = recovery::recovery_lock().await;
    recovery::backup_paused_installation_locked(pool, &paused).await?;
    recovery::remove_path_without_following_links(paused_path)?;
    db::delete_paused_installation(pool, &paused.skill_id, &paused.agent_id).await
}

async fn delete_managed_installation_locked(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
) -> Result<(), String> {
    let agent = db::get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| format!("플랫폼 '{}'을(를) 찾을 수 없습니다", agent_id))?;
    if agent.id == "central" {
        return Err("중앙 보관함 설치는 삭제할 수 없습니다".to_string());
    }
    ensure_not_shared_universal_root(pool, &agent).await?;

    let active = db::get_skill_installation(pool, skill_id, agent_id).await?;
    let paused = db::get_paused_installation(pool, skill_id, agent_id).await?;
    if active.is_some() && paused.is_some() {
        return Err("활성 설치와 비활성 설치 기록이 함께 있어 삭제하지 않습니다".to_string());
    }
    if let Some(installation) = active {
        return delete_active_installation_locked(pool, &agent, installation).await;
    }
    if let Some(installation) = paused {
        return delete_paused_installation_locked(pool, installation).await;
    }
    Ok(())
}

pub async fn delete_skill_from_agent_impl(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
) -> Result<(), String> {
    let _guard = usage_lock().await;
    delete_managed_installation_locked(pool, skill_id, agent_id).await
}

pub async fn delete_platform_installations_impl(
    pool: &DbPool,
    agent_id: &str,
) -> Result<DeletePlatformInstallationsResult, String> {
    let _guard = usage_lock().await;
    let agent = db::get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| format!("플랫폼 '{}'을(를) 찾을 수 없습니다", agent_id))?;
    if agent.id == "central" {
        return Err("중앙 보관함 설치는 삭제할 수 없습니다".to_string());
    }
    ensure_not_shared_universal_root(pool, &agent).await?;

    let mut targets = db::get_skill_installations_by_agent(pool, agent_id)
        .await?
        .into_iter()
        .map(|installation| installation.skill_id)
        .collect::<Vec<_>>();
    targets.extend(
        db::get_paused_installations_by_agent(pool, agent_id)
            .await?
            .into_iter()
            .map(|installation| installation.skill_id),
    );
    targets.sort();
    targets.dedup();

    let mut result = DeletePlatformInstallationsResult {
        deleted: Vec::new(),
        failed: Vec::new(),
    };
    for skill_id in targets {
        match delete_managed_installation_locked(pool, &skill_id, agent_id).await {
            Ok(()) => result.deleted.push(skill_id),
            Err(error) => result
                .failed
                .push(DeleteInstallationFailure { skill_id, error }),
        }
    }
    Ok(result)
}

async fn set_skill_usage_impl_locked(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
    enabled: bool,
    paused_by_bulk: bool,
) -> Result<(), String> {
    if enabled {
        if db::get_skill_installation(pool, skill_id, agent_id)
            .await?
            .is_some()
        {
            return Ok(());
        }
        restore_paused_installation_locked(pool, skill_id, agent_id).await
    } else {
        if db::get_paused_installation(pool, skill_id, agent_id)
            .await?
            .is_some()
        {
            return Ok(());
        }
        pause_active_installation_locked(pool, skill_id, agent_id, paused_by_bulk).await
    }
}

pub async fn get_skill_usage_status_impl(pool: &DbPool) -> Result<Vec<UsageStatus>, String> {
    let agents = db::get_all_agents(pool).await?;
    let mut statuses = Vec::new();

    for agent in agents.into_iter().filter(|agent| agent.id != "central") {
        let active = db::get_skill_installations_by_agent(pool, &agent.id).await?;
        let paused = db::get_paused_installations_by_agent(pool, &agent.id).await?;
        let active_count = active.len();
        let paused_count = paused.len();
        let mut external_skill_ids = db::get_agent_skill_observations(pool, &agent.id)
            .await?
            .into_iter()
            // 같은 설치의 스캔 결과를 삭제 후에도 남는 외부 스킬로 세지 않는다.
            // 같은 이름이라도 다른 경로의 플러그인이나 공용 설치는 별개다.
            .filter(|observation| {
                !active.iter().any(|installation| {
                    Path::new(&installation.installed_path) == Path::new(&observation.dir_path)
                }) && !paused.iter().any(|installation| {
                    Path::new(&installation.installed_path) == Path::new(&observation.dir_path)
                })
            })
            .map(|observation| observation.skill_id)
            .collect::<std::collections::BTreeSet<_>>();
        if agent.id != "universal" {
            if let Some(universal) = db::get_agent_by_id(pool, "universal").await? {
                let same_universal_root = Path::new(&agent.global_skills_dir)
                    == Path::new(&universal.global_skills_dir)
                    || Path::new(&agent.global_skills_dir)
                        .canonicalize()
                        .ok()
                        .zip(Path::new(&universal.global_skills_dir).canonicalize().ok())
                        .is_some_and(|(agent_root, universal_root)| agent_root == universal_root);
                if agent.id == "factory-droid"
                    || db::agent_supports_universal_agents_skills(&agent.id)
                    || same_universal_root
                {
                    for installation in
                        db::get_skill_installations_by_agent(pool, "universal").await?
                    {
                        external_skill_ids.insert(installation.skill_id);
                    }
                }
            }
        }
        let external_count = external_skill_ids.len();
        let mut skills = Vec::with_capacity(active.len() + paused.len());

        for installation in active {
            let name = active_skill_name(
                db::get_skill_by_id(pool, &installation.skill_id).await?,
                &installation.skill_id,
            );
            skills.push(UsageSkill {
                skill_id: installation.skill_id,
                name,
                enabled: true,
                paused_by_bulk: false,
            });
        }
        for installation in paused {
            skills.push(UsageSkill {
                skill_id: installation.skill_id,
                name: installation.skill_name,
                enabled: false,
                paused_by_bulk: installation.paused_by_bulk,
            });
        }
        skills.sort_by(|left, right| {
            left.name
                .to_lowercase()
                .cmp(&right.name.to_lowercase())
                .then_with(|| left.skill_id.cmp(&right.skill_id))
        });

        statuses.push(UsageStatus {
            agent_id: agent.id,
            active_count,
            paused_count,
            external_count,
            skills,
        });
    }
    Ok(statuses)
}

pub async fn set_skill_usage_impl(
    pool: &DbPool,
    skill_id: &str,
    agent_id: &str,
    enabled: bool,
) -> Result<(), String> {
    let _guard = usage_lock().await;
    set_skill_usage_impl_locked(pool, skill_id, agent_id, enabled, false).await
}

pub async fn set_platform_usage_impl(
    pool: &DbPool,
    agent_id: &str,
    enabled: bool,
) -> Result<(), String> {
    let _guard = usage_lock().await;
    let agent = db::get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| format!("플랫폼 '{}'을(를) 찾을 수 없습니다", agent_id))?;
    if agent.id == "central" {
        return Err("중앙 보관함은 활성/비활성 전환 대상이 아닙니다".to_string());
    }

    // Non-universal bulk never auto-includes shared installs: those move only
    // through the explicit verified shared path.
    let targets = if enabled {
        let paused = db::get_paused_installations_by_agent(pool, agent_id).await?;
        let mut out = Vec::new();
        for installation in paused.into_iter().filter(|i| i.paused_by_bulk) {
            if agent.id != "universal"
                && entry_is_shared_with_others(pool, &installation.installed_path, agent_id).await?
            {
                continue;
            }
            out.push(installation.skill_id);
        }
        out
    } else {
        let active = db::get_skill_installations_by_agent(pool, agent_id).await?;
        let mut out = Vec::new();
        for installation in active {
            if agent.id != "universal"
                && entry_is_shared_with_others(pool, &installation.installed_path, agent_id).await?
            {
                continue;
            }
            out.push(installation.skill_id);
        }
        out
    };

    let mut failures = Vec::new();
    for skill_id in targets {
        let result =
            set_skill_usage_impl_locked(pool, &skill_id, agent_id, enabled, !enabled).await;
        if let Err(error) = result {
            failures.push(format!("{}: {}", skill_id, error));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "일부 스킬의 활성 상태를 바꾸지 못했습니다. 현재 상태를 다시 확인하세요: {}",
            failures.join(" | ")
        ))
    }
}

// ─── Shared install identity & impact ───────────────────────────────────────
// Shared ID is the normalized INSTALL ENTRY path: the parent is canonicalized
// but the final component is NOT followed. Distinct links (or copies) pointing
// at the same target keep distinct IDs. Never group by name, skill_id alone,
// or compatibility lists; confirmed readers need real managed/observed evidence.

/// Normalized install-entry identity: canonical parent + file name.
///
/// Does NOT follow the final symlink, so independent links stay separate.
/// Stable while inactive because the agent parent dir still exists after pause.
pub fn shared_entry_key(path_str: &str) -> String {
    let path = Path::new(path_str);
    let Some(name) = path.file_name() else {
        return path_str.to_string();
    };
    let Some(parent) = path.parent() else {
        return path_str.to_string();
    };
    match parent.canonicalize() {
        Ok(canon) => canon.join(name).to_string_lossy().into_owned(),
        Err(_) => path_str.to_string(),
    }
}

fn same_shared_entry(left: &str, right: &str) -> bool {
    shared_entry_key(left) == shared_entry_key(right)
}

fn shared_sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn is_counted_platform(agent_id: &str) -> bool {
    // Confirmed readers are real platforms only. Both pseudo agents (central
    // vault and universal shared root) never count; the backend excludes
    // them so the frontend never filters.
    agent_id != "central" && agent_id != "universal"
}

#[derive(Debug, Clone, Serialize)]
pub struct SharedPlatformRef {
    pub agent_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SharedSeparateInstall {
    pub agent_id: String,
    pub display_name: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SharedSkillImpact {
    pub shared_install_id: String,
    pub skill_id: String,
    pub skill_name: String,
    pub enabled: bool,
    pub confirmed_platforms: Vec<SharedPlatformRef>,
    pub separate_installs: Vec<SharedSeparateInstall>,
    pub reason: Option<String>,
    pub management_path: String,
    pub confirmation_token: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetSharedSkillUsageResult {
    pub applied: bool,
    pub impact: SharedSkillImpact,
}

#[derive(Debug, Clone, Serialize)]
pub struct SharedBulkFailure {
    pub skill_id: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SetSharedPlatformUsageResult {
    pub applied: bool,
    pub impacts: Vec<SharedSkillImpact>,
    pub failed: Vec<SharedBulkFailure>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SharedConfirmation {
    pub shared_install_id: String,
    pub confirmation_token: String,
}

fn live_entry_fingerprint(installed_path: &str) -> String {
    let path = Path::new(installed_path);
    let meta = fs::symlink_metadata(path);
    match meta {
        Ok(m) if m.file_type().is_symlink() => {
            let target = fs::read_link(path)
                .map(|t| t.to_string_lossy().into_owned())
                .unwrap_or_else(|e| format!("<read_link:{e}>"));
            format!("symlink:{target}")
        }
        Ok(m) if m.is_dir() => "dir".to_string(),
        Ok(_) => "other".to_string(),
        Err(e) => format!("<missing:{e}>"),
    }
}

/// Compute the shared impact for one normalized install-entry ID.
///
/// Confirmed readers come only from real managed rows (active/paused) and
/// non-compatibility observations. Compatibility lists never confirm.
/// Central vault ancestry and plugin ownership return a protection reason with
/// a management path instead of a generic unsupported-adapter message.
pub async fn compute_shared_impact(
    pool: &DbPool,
    shared_install_id: &str,
) -> Result<SharedSkillImpact, String> {
    // Tokens and bulk IDs always use the canonical entry key so a raw
    // source_path and its normalized spelling confirm the same install.
    let canonical_id = shared_entry_key(shared_install_id);
    let shared_install_id = canonical_id.as_str();
    let all_active = db::get_all_skill_installations(pool).await?;
    let all_paused = db::get_all_paused_installations(pool).await?;
    let all_observations = db::get_all_agent_skill_observations(pool).await?;
    let agents = db::get_all_agents(pool).await?;
    let names: BTreeMap<String, String> =
        agents.into_iter().map(|a| (a.id, a.display_name)).collect();

    let active: Vec<SkillInstallation> = all_active
        .into_iter()
        .filter(|i| same_shared_entry(&i.installed_path, shared_install_id))
        .collect();
    let paused: Vec<PausedInstallation> = all_paused
        .into_iter()
        .filter(|p| same_shared_entry(&p.installed_path, shared_install_id))
        .collect();
    let observed: Vec<db::AgentSkillObservation> = all_observations
        .into_iter()
        .filter(|o| same_shared_entry(&o.dir_path, shared_install_id))
        .collect();

    if active.is_empty() && paused.is_empty() && observed.is_empty() {
        return Err(format!("공유 설치를 찾을 수 없습니다: {shared_install_id}"));
    }

    let skill_id = active
        .first()
        .map(|i| i.skill_id.clone())
        .or_else(|| paused.first().map(|p| p.skill_id.clone()))
        .or_else(|| observed.first().map(|o| o.skill_id.clone()))
        .unwrap_or_else(|| shared_install_id.to_string());
    let skill_name = if let Some(first) = active.first() {
        active_skill_name(
            db::get_skill_by_id(pool, &first.skill_id).await?,
            &first.skill_id,
        )
    } else if let Some(first) = paused.first() {
        if first.skill_name.trim().is_empty() {
            first.skill_id.clone()
        } else {
            first.skill_name.clone()
        }
    } else if let Some(first) = observed.first() {
        first.name.clone()
    } else {
        skill_id.clone()
    };

    let enabled = !active.is_empty() || paused.is_empty();

    // Confirmed platforms: managed evidence + non-compatibility observations.
    // Pseudo central/universal never count toward the confirmed total.
    let mut confirmed_ids = BTreeSet::new();
    for i in &active {
        if is_counted_platform(&i.agent_id) {
            confirmed_ids.insert(i.agent_id.clone());
        }
    }
    for p in &paused {
        if is_counted_platform(&p.agent_id) {
            confirmed_ids.insert(p.agent_id.clone());
        }
    }
    for o in &observed {
        if o.source_kind == "compatibility" {
            continue;
        }
        if is_counted_platform(&o.agent_id) {
            confirmed_ids.insert(o.agent_id.clone());
        }
    }
    let mut confirmed_platforms: Vec<SharedPlatformRef> = confirmed_ids
        .into_iter()
        .map(|agent_id| SharedPlatformRef {
            display_name: names
                .get(&agent_id)
                .cloned()
                .unwrap_or_else(|| agent_id.clone()),
            agent_id,
        })
        .collect();
    confirmed_platforms.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
            .then_with(|| a.agent_id.cmp(&b.agent_id))
    });

    // Separate installs: same skill_id, different physical entry. These keep
    // working after the shared entry is paused.
    let all_active2 = db::get_all_skill_installations(pool).await?;
    let all_paused2 = db::get_all_paused_installations(pool).await?;
    let all_observations2 = db::get_all_agent_skill_observations(pool).await?;
    let mut separate_map: BTreeMap<(String, String), String> = BTreeMap::new();
    for i in all_active2 {
        if i.skill_id == skill_id && !same_shared_entry(&i.installed_path, shared_install_id) {
            if i.agent_id == "central" {
                continue;
            }
            separate_map
                .entry((i.agent_id, i.installed_path.clone()))
                .or_insert_with(|| i.installed_path.clone());
        }
    }
    for p in all_paused2 {
        if p.skill_id == skill_id && !same_shared_entry(&p.installed_path, shared_install_id) {
            if p.agent_id == "central" {
                continue;
            }
            separate_map
                .entry((p.agent_id, p.installed_path.clone()))
                .or_insert_with(|| p.installed_path.clone());
        }
    }
    for o in all_observations2 {
        if o.skill_id == skill_id
            && o.source_kind != "compatibility"
            && !same_shared_entry(&o.dir_path, shared_install_id)
        {
            if o.agent_id == "central" {
                continue;
            }
            separate_map
                .entry((o.agent_id.clone(), o.dir_path.clone()))
                .or_insert_with(|| o.dir_path.clone());
        }
    }
    let mut separate_installs: Vec<SharedSeparateInstall> = separate_map
        .into_iter()
        .map(|((agent_id, source_path), _)| SharedSeparateInstall {
            display_name: names
                .get(&agent_id)
                .cloned()
                .unwrap_or_else(|| agent_id.clone()),
            agent_id,
            source_path,
        })
        .collect();
    separate_installs.sort_by(|a, b| {
        a.display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase())
            .then_with(|| a.source_path.cmp(&b.source_path))
    });

    // Protection: vault ancestry and plugin ownership (not path equality only).
    // Symlink entries pointing AT the vault stay movable (only the link moves).
    let mut reason: Option<String> = None;
    let mut management_path = shared_install_id.to_string();
    let vault_canonical: Option<PathBuf> =
        db::get_central_skills_dir(pool).await?.canonicalize().ok();
    if let Some(vault) = vault_canonical.as_ref() {
        let entry_parent = Path::new(shared_install_id).parent().map(Path::to_path_buf);
        let inside = entry_parent.is_some_and(|parent| parent.starts_with(vault))
            || Path::new(shared_install_id) == vault.as_path();
        if inside {
            let vault_text = vault.to_string_lossy().into_owned();
            reason = Some(format!(
                "중앙 보관함 안의 원본이라 공용 제어로 옮기지 않습니다. 보관함에서 직접 관리하세요: {vault_text}"
            ));
            management_path = vault_text;
        }
    }
    if reason.is_none() {
        if let Some(plugin) = observed.iter().find(|o| o.source_kind == "plugin") {
            reason = Some(format!(
                "외부 플러그인이 소유한 경로라 공용 제어로 옮기지 않습니다. 플러그인 관리에서 변경하세요: {}",
                plugin.source_root
            ));
            management_path = plugin.source_root.clone();
        }
    }
    // Observed-only entries without managed rows cannot move through the
    // managed pause/restore flow; report honestly instead of toggling nothing.
    if reason.is_none() && active.is_empty() && paused.is_empty() {
        reason = Some(format!(
            "관리 설치가 아니라 공용 제어로 직접 옮기지 않습니다. 해당 경로에서 직접 관리하세요: {shared_install_id}"
        ));
        management_path = shared_install_id.to_string();
    }

    let confirmation_token = {
        let mut text = String::new();
        text.push_str(shared_install_id);
        text.push('|');
        text.push_str(if enabled { "1" } else { "0" });
        text.push('|');
        for platform in &confirmed_platforms {
            text.push_str(&platform.agent_id);
            text.push(',');
        }
        text.push('|');
        for install in &separate_installs {
            text.push_str(&install.agent_id);
            text.push(':');
            text.push_str(&install.source_path);
            text.push(',');
        }
        text.push('|');
        text.push_str(reason.as_deref().unwrap_or("-"));
        text.push('|');
        text.push_str(&management_path);
        text.push('|');
        let mut active_keys: Vec<_> = active
            .iter()
            .map(|i| {
                (
                    i.agent_id.clone(),
                    i.skill_id.clone(),
                    i.installed_path.clone(),
                    i.link_type.clone(),
                    i.symlink_target.clone(),
                )
            })
            .collect();
        active_keys.sort();
        for (agent, skill, path, link, target) in &active_keys {
            text.push_str(&format!(
                "A:{agent}:{skill}:{path}:{link}:{};",
                target.as_deref().unwrap_or("-")
            ));
        }
        text.push('|');
        let mut paused_keys: Vec<_> = paused
            .iter()
            .map(|p| {
                (
                    p.agent_id.clone(),
                    p.skill_id.clone(),
                    p.installed_path.clone(),
                    p.link_type.clone(),
                    p.symlink_target.clone(),
                )
            })
            .collect();
        paused_keys.sort();
        for (agent, skill, path, link, target) in &paused_keys {
            text.push_str(&format!(
                "P:{agent}:{skill}:{path}:{link}:{};",
                target.as_deref().unwrap_or("-")
            ));
        }
        text.push('|');
        let mut observed_keys: Vec<_> = observed
            .iter()
            .map(|o| {
                (
                    o.agent_id.clone(),
                    o.source_kind.clone(),
                    o.dir_path.clone(),
                    o.symlink_target.clone(),
                )
            })
            .collect();
        observed_keys.sort();
        for (agent, kind, path, target) in &observed_keys {
            text.push_str(&format!(
                "O:{agent}:{kind}:{path}:{};",
                target.as_deref().unwrap_or("-")
            ));
        }
        text.push('|');
        let mut live_paths: Vec<(String, String)> = Vec::new();
        let mut seen = BTreeSet::new();
        for i in &active {
            if seen.insert(i.installed_path.clone()) {
                live_paths.push((
                    i.installed_path.clone(),
                    live_entry_fingerprint(&i.installed_path),
                ));
            }
        }
        for p in &paused {
            if seen.insert(format!("paused:{}", p.paused_path)) {
                live_paths.push((
                    p.paused_path.clone(),
                    live_entry_fingerprint(&p.paused_path),
                ));
            }
        }
        live_paths.sort();
        for (path, fp) in &live_paths {
            text.push_str(&format!("L:{path}:{fp};"));
        }
        shared_sha256_hex(text.as_bytes())
    };

    Ok(SharedSkillImpact {
        shared_install_id: shared_install_id.to_string(),
        skill_id,
        skill_name,
        enabled,
        confirmed_platforms,
        separate_installs,
        reason,
        management_path,
        confirmation_token,
    })
}

pub async fn entry_is_shared_with_others(
    pool: &DbPool,
    installed_path: &str,
    agent_id: &str,
) -> Result<bool, String> {
    for other in db::get_all_skill_installations(pool).await? {
        if other.agent_id == agent_id {
            continue;
        }
        if same_shared_entry(installed_path, &other.installed_path) {
            return Ok(true);
        }
    }
    for other in db::get_all_paused_installations(pool).await? {
        if other.agent_id == agent_id {
            continue;
        }
        if same_shared_entry(installed_path, &other.installed_path) {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn pause_shared_installation_locked(
    pool: &DbPool,
    shared_install_id: &str,
    paused_by_bulk: bool,
) -> Result<(), String> {
    let matching: Vec<SkillInstallation> = db::get_all_skill_installations(pool)
        .await?
        .into_iter()
        .filter(|i| same_shared_entry(&i.installed_path, shared_install_id))
        .collect();
    if matching.is_empty() {
        return Ok(());
    }
    // Preflight every row before touching disk or DB.
    let mut validated: Vec<(SkillInstallation, db::Agent, Option<String>)> = Vec::new();
    for installation in &matching {
        let agent = db::get_agent_by_id(pool, &installation.agent_id)
            .await?
            .ok_or_else(|| format!("플랫폼 '{}'을(를) 찾을 수 없습니다", installation.agent_id))?;
        if agent.id == "central" {
            return Err("중앙 보관함 스킬은 비활성화할 수 없습니다".to_string());
        }
        let raw_target =
            validate_active_installation(Path::new(&agent.global_skills_dir), installation)?;
        validated.push((installation.clone(), agent, raw_target));
    }
    if db::get_all_paused_installations(pool)
        .await?
        .into_iter()
        .any(|p| same_shared_entry(&p.installed_path, shared_install_id))
    {
        return Err("활성 설치와 비활성 설치 기록이 함께 있어 파일을 옮기지 않습니다".to_string());
    }

    let root = paused_root(pool).await?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut paused_rows: Vec<PausedInstallation> = Vec::new();
    for (installation, _agent, raw_target) in &validated {
        let paused_path = root.join(Uuid::new_v4().to_string());
        paused_rows.push(PausedInstallation {
            skill_id: installation.skill_id.clone(),
            agent_id: installation.agent_id.clone(),
            skill_name: active_skill_name(
                db::get_skill_by_id(pool, &installation.skill_id).await?,
                &installation.skill_id,
            ),
            installed_path: installation.installed_path.clone(),
            paused_path: paused_path.to_string_lossy().into_owned(),
            link_type: installation.link_type.clone(),
            symlink_target: raw_target.clone(),
            paused_by_bulk,
            created_at: now.clone(),
        });
    }
    // Insert paused rows first so a later move failure never loses the entry.
    // ponytail: N paused rows share one physical entry; only the first
    // paused_path holds the moved file, the rest are logical siblings restored
    // together through the shared path only.
    let mut inserted: Vec<(String, String)> = Vec::new();
    for paused in &paused_rows {
        if let Err(error) = db::upsert_paused_installation(pool, paused).await {
            let mut cleanup_error: Option<String> = None;
            for (skill_id, agent_id) in inserted {
                if let Err(e) = db::delete_paused_installation(pool, &skill_id, &agent_id).await {
                    cleanup_error = Some(e);
                }
            }
            return Err(rollback_errors(error, cleanup_error, None));
        }
        inserted.push((paused.skill_id.clone(), paused.agent_id.clone()));
    }

    // One physical entry: move once, never twice.
    let source = PathBuf::from(&paused_rows[0].installed_path);
    let first_paused = PathBuf::from(&paused_rows[0].paused_path);
    if let Err(error) = move_path(&source, &first_paused) {
        let mut cleanup_error: Option<String> = None;
        for (skill_id, agent_id) in &inserted {
            if let Err(e) = db::delete_paused_installation(pool, skill_id, agent_id).await {
                cleanup_error = Some(e);
                break;
            }
        }
        return Err(rollback_errors(error, cleanup_error, None));
    }
    let mut delete_error: Option<String> = None;
    for (installation, _, _) in &validated {
        if let Err(e) =
            db::delete_skill_installation(pool, &installation.skill_id, &installation.agent_id)
                .await
        {
            delete_error = Some(e);
            break;
        }
    }
    if let Some(error) = delete_error {
        let rollback = rollback_move(&first_paused, &source);
        let mut cleanup_error: Option<String> = None;
        if rollback.is_ok() {
            for (skill_id, agent_id) in &inserted {
                if let Err(e) = db::delete_paused_installation(pool, skill_id, agent_id).await {
                    cleanup_error = Some(e);
                    break;
                }
            }
        }
        return Err(rollback_errors(error, cleanup_error, rollback.err()));
    }
    Ok(())
}

async fn restore_shared_installation_locked(
    pool: &DbPool,
    shared_install_id: &str,
) -> Result<(), String> {
    let matching: Vec<PausedInstallation> = db::get_all_paused_installations(pool)
        .await?
        .into_iter()
        .filter(|p| same_shared_entry(&p.installed_path, shared_install_id))
        .collect();
    if matching.is_empty() {
        return Ok(());
    }
    for m in &matching {
        if db::get_skill_installation(pool, &m.skill_id, &m.agent_id)
            .await?
            .is_some()
        {
            return Err(format!("이미 활성 설치 기록이 있습니다: {}", m.skill_id));
        }
    }
    // Representative file holds the single physical entry.
    let mut representative: Option<&PausedInstallation> = None;
    for m in &matching {
        if fs::symlink_metadata(&m.paused_path).is_ok() {
            representative = Some(m);
            break;
        }
    }
    let representative = representative
        .ok_or_else(|| "비활성 관리 설치 파일을 찾을 수 없습니다".to_string())?
        .clone();
    let root = paused_root(pool).await?;
    let paused_path = PathBuf::from(&representative.paused_path);
    validate_paused_path(&root, &paused_path)?;
    let metadata = metadata_without_following_links(&paused_path)?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(&paused_path).map_err(|error| {
            format!(
                "비활성 심볼릭 링크를 읽을 수 없습니다 '{}': {error}",
                paused_path.display()
            )
        })?;
        let target_text = target.to_string_lossy().into_owned();
        if representative.symlink_target.as_deref() != Some(target_text.as_str()) {
            return Err("비활성 심볼릭 링크 대상이 기록과 달라 복원을 중단했습니다".to_string());
        }
    } else if !metadata.is_dir() || !matches!(representative.link_type.as_str(), "copy" | "native")
    {
        return Err(format!(
            "비활성 관리 설치 파일 형식을 확인할 수 없습니다: {}",
            paused_path.display()
        ));
    }
    let agent = db::get_agent_by_id(pool, &representative.agent_id)
        .await?
        .ok_or_else(|| {
            format!(
                "플랫폼 '{}'을(를) 찾을 수 없습니다",
                representative.agent_id
            )
        })?;
    let installed_path = PathBuf::from(&representative.installed_path);
    let agent_root = PathBuf::from(&agent.global_skills_dir);
    if !agent_root.exists() {
        fs::create_dir_all(&agent_root).map_err(|error| {
            format!(
                "플랫폼 스킬 폴더를 만들 수 없습니다 '{}': {error}",
                agent_root.display()
            )
        })?;
    }
    validate_installation_path(&agent_root, &installed_path)?;
    // Restore never overwrites.
    if fs::symlink_metadata(&installed_path).is_ok() {
        return Err(format!(
            "원래 설치 경로가 이미 있어 파일을 덮어쓰지 않습니다: {}",
            installed_path.display()
        ));
    }

    move_path(&paused_path, &installed_path)?;
    let mut inserted: Vec<(String, String)> = Vec::new();
    let mut upsert_error: Option<String> = None;
    for m in &matching {
        let active = SkillInstallation {
            skill_id: m.skill_id.clone(),
            agent_id: m.agent_id.clone(),
            installed_path: m.installed_path.clone(),
            link_type: m.link_type.clone(),
            symlink_target: m.symlink_target.clone(),
            created_at: m.created_at.clone(),
        };
        if let Err(e) = db::upsert_skill_installation(pool, &active).await {
            upsert_error = Some(e);
            break;
        }
        inserted.push((m.skill_id.clone(), m.agent_id.clone()));
    }
    if let Some(error) = upsert_error {
        let rollback = rollback_move(&installed_path, &paused_path);
        let mut cleanup_error: Option<String> = None;
        for (skill_id, agent_id) in &inserted {
            if let Err(e) = db::delete_skill_installation(pool, skill_id, agent_id).await {
                cleanup_error = Some(e);
                break;
            }
        }
        return Err(rollback_errors(error, cleanup_error, rollback.err()));
    }
    let mut delete_error: Option<String> = None;
    for m in &matching {
        if let Err(e) = db::delete_paused_installation(pool, &m.skill_id, &m.agent_id).await {
            delete_error = Some(e);
            break;
        }
    }
    if let Some(error) = delete_error {
        let mut cleanup_error: Option<String> = None;
        for (skill_id, agent_id) in &inserted {
            if let Err(e) = db::delete_skill_installation(pool, skill_id, agent_id).await {
                cleanup_error = Some(e);
                break;
            }
        }
        let rollback = rollback_move(&installed_path, &paused_path);
        return Err(rollback_errors(error, cleanup_error, rollback.err()));
    }
    Ok(())
}

async fn set_shared_skill_usage_locked(
    pool: &DbPool,
    shared_install_id: &str,
    enabled: bool,
    confirmation_token: &str,
    paused_by_bulk: bool,
) -> Result<SetSharedSkillUsageResult, String> {
    let fresh = compute_shared_impact(pool, shared_install_id).await?;
    if fresh.confirmation_token != confirmation_token {
        return Ok(SetSharedSkillUsageResult {
            applied: false,
            impact: fresh,
        });
    }
    if let Some(reason) = fresh.reason.clone() {
        return Err(reason);
    }
    if fresh.enabled == enabled {
        return Ok(SetSharedSkillUsageResult {
            applied: true,
            impact: fresh,
        });
    }
    if enabled {
        restore_shared_installation_locked(pool, shared_install_id).await?;
    } else {
        pause_shared_installation_locked(pool, shared_install_id, paused_by_bulk).await?;
    }
    let refreshed = compute_shared_impact(pool, shared_install_id).await?;
    Ok(SetSharedSkillUsageResult {
        applied: true,
        impact: refreshed,
    })
}

pub async fn get_shared_skill_impact_impl(
    pool: &DbPool,
    shared_install_id: &str,
) -> Result<SharedSkillImpact, String> {
    compute_shared_impact(pool, shared_install_id).await
}

pub async fn set_shared_skill_usage_impl(
    pool: &DbPool,
    shared_install_id: &str,
    enabled: bool,
    confirmation_token: &str,
) -> Result<SetSharedSkillUsageResult, String> {
    let _guard = usage_lock().await;
    set_shared_skill_usage_locked(pool, shared_install_id, enabled, confirmation_token, false).await
}

fn bulk_shared_ids_for_universal(
    active: &[SkillInstallation],
    paused: &[PausedInstallation],
    enabled: bool,
) -> Vec<String> {
    let mut keys = BTreeSet::new();
    if enabled {
        for p in paused
            .iter()
            .filter(|p| p.agent_id == "universal" && p.paused_by_bulk)
        {
            keys.insert(shared_entry_key(&p.installed_path));
        }
    } else {
        for i in active.iter().filter(|i| i.agent_id == "universal") {
            keys.insert(shared_entry_key(&i.installed_path));
        }
    }
    keys.into_iter().collect()
}

pub async fn set_shared_platform_usage_impl(
    pool: &DbPool,
    enabled: bool,
    confirmations: &[SharedConfirmation],
) -> Result<SetSharedPlatformUsageResult, String> {
    let _guard = usage_lock().await;
    // Existing universal bulk semantics only: disable every active managed
    // universal entry, enable only paused_by_bulk. No new selection UI.
    let active = db::get_all_skill_installations(pool).await?;
    let paused = db::get_all_paused_installations(pool).await?;
    let current_ids = bulk_shared_ids_for_universal(&active, &paused, enabled);
    let mut current_impacts: Vec<SharedSkillImpact> = Vec::new();
    for id in &current_ids {
        current_impacts.push(compute_shared_impact(pool, id).await?);
    }
    current_impacts.sort_by(|a, b| a.shared_install_id.cmp(&b.shared_install_id));
    // Full-scope preflight: tokens must match the entire current set. Never
    // silently skip newly discovered items.
    let mut provided: BTreeMap<String, String> = BTreeMap::new();
    for c in confirmations {
        provided.insert(
            shared_entry_key(&c.shared_install_id),
            c.confirmation_token.clone(),
        );
    }
    let current_set: BTreeSet<String> = current_ids.iter().cloned().collect();
    let provided_set: BTreeSet<String> = provided.keys().cloned().collect();
    if current_set != provided_set
        || current_impacts.iter().any(|impact| {
            provided.get(&impact.shared_install_id) != Some(&impact.confirmation_token)
        })
    {
        return Ok(SetSharedPlatformUsageResult {
            applied: false,
            impacts: current_impacts,
            failed: Vec::new(),
        });
    }
    let mut failed: Vec<SharedBulkFailure> = Vec::new();
    for impact in &current_impacts {
        if impact.reason.is_some() {
            failed.push(SharedBulkFailure {
                skill_id: impact.skill_id.clone(),
                error: impact
                    .reason
                    .clone()
                    .unwrap_or_else(|| "공유 설치를 변경할 수 없습니다".to_string()),
            });
            continue;
        }
        match set_shared_skill_usage_locked(
            pool,
            &impact.shared_install_id,
            enabled,
            &impact.confirmation_token,
            true,
        )
        .await
        {
            Ok(result) => {
                if !result.applied {
                    failed.push(SharedBulkFailure {
                        skill_id: impact.skill_id.clone(),
                        error: "확인 토큰이 만료되어 bulk를 중단했습니다".to_string(),
                    });
                    break;
                }
            }
            Err(error) => failed.push(SharedBulkFailure {
                skill_id: impact.skill_id.clone(),
                error,
            }),
        }
    }
    let mut refreshed: Vec<SharedSkillImpact> = Vec::new();
    let active2 = db::get_all_skill_installations(pool).await?;
    let paused2 = db::get_all_paused_installations(pool).await?;
    // Refresh the preflight scope plus anything newly discovered mid-run.
    let mut refresh_ids: BTreeSet<String> = current_set;
    for i in &active2 {
        if !enabled && i.agent_id == "universal" {
            refresh_ids.insert(shared_entry_key(&i.installed_path));
        }
    }
    for p in &paused2 {
        if p.agent_id == "universal" {
            refresh_ids.insert(shared_entry_key(&p.installed_path));
        }
    }
    for id in refresh_ids {
        match compute_shared_impact(pool, &id).await {
            Ok(impact) => refreshed.push(impact),
            Err(error) => refreshed.push(SharedSkillImpact {
                shared_install_id: id.clone(),
                skill_id: id.clone(),
                skill_name: id,
                enabled,
                confirmed_platforms: Vec::new(),
                separate_installs: Vec::new(),
                reason: Some(error),
                management_path: String::new(),
                confirmation_token: String::new(),
            }),
        }
    }
    refreshed.sort_by(|a, b| a.shared_install_id.cmp(&b.shared_install_id));
    let applied = failed.is_empty();
    Ok(SetSharedPlatformUsageResult {
        applied,
        impacts: refreshed,
        failed,
    })
}

#[tauri::command]
pub async fn get_skill_usage_status(
    state: State<'_, AppState>,
) -> Result<Vec<UsageStatus>, String> {
    get_skill_usage_status_impl(&state.db).await
}

#[tauri::command]
pub async fn set_skill_usage(
    state: State<'_, AppState>,
    skill_id: String,
    agent_id: String,
    enabled: bool,
) -> Result<(), String> {
    set_skill_usage_impl(&state.db, &skill_id, &agent_id, enabled).await
}

#[tauri::command]
pub async fn set_platform_usage(
    state: State<'_, AppState>,
    agent_id: String,
    enabled: bool,
) -> Result<(), String> {
    set_platform_usage_impl(&state.db, &agent_id, enabled).await
}

#[tauri::command]
pub async fn get_shared_skill_impact(
    state: State<'_, AppState>,
    shared_install_id: String,
) -> Result<SharedSkillImpact, String> {
    get_shared_skill_impact_impl(&state.db, &shared_install_id).await
}

#[tauri::command]
pub async fn set_shared_skill_usage(
    state: State<'_, AppState>,
    shared_install_id: String,
    enabled: bool,
    confirmation_token: String,
) -> Result<SetSharedSkillUsageResult, String> {
    set_shared_skill_usage_impl(&state.db, &shared_install_id, enabled, &confirmation_token).await
}

#[tauri::command]
pub async fn set_shared_platform_usage(
    state: State<'_, AppState>,
    enabled: bool,
    confirmations: Vec<SharedConfirmation>,
) -> Result<SetSharedPlatformUsageResult, String> {
    set_shared_platform_usage_impl(&state.db, enabled, &confirmations).await
}

#[tauri::command]
pub async fn delete_skill_from_agent(
    state: State<'_, AppState>,
    skill_id: String,
    agent_id: String,
) -> Result<(), String> {
    delete_skill_from_agent_impl(&state.db, &skill_id, &agent_id).await
}

#[tauri::command]
pub async fn delete_platform_installations(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<DeletePlatformInstallationsResult, String> {
    delete_platform_installations_impl(&state.db, &agent_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{self, Skill};
    use tempfile::TempDir;

    async fn setup(pool_dir: &TempDir) -> (DbPool, PathBuf, PathBuf) {
        let db_path = pool_dir.path().join("db.sqlite");
        let pool = db::create_pool(&db_path.to_string_lossy()).await.unwrap();
        db::init_database(&pool).await.unwrap();
        let central = pool_dir.path().join("central");
        let agent = pool_dir.path().join("claude");
        fs::create_dir_all(&central).unwrap();
        fs::create_dir_all(&agent).unwrap();
        fs::write(agent.parent().unwrap().join("settings.json"), "{}").unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'central'")
            .bind(central.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'claude-code'")
            .bind(agent.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        (pool, central, agent)
    }

    async fn add_skill(pool: &DbPool, central: &Path, agent: &Path, id: &str, link_type: &str) {
        let central_skill = central.join(id);
        fs::create_dir_all(&central_skill).unwrap();
        fs::write(central_skill.join("SKILL.md"), "---\nname: Test\n---\n").unwrap();
        let installed = agent.join(id);
        match link_type {
            "symlink" => {
                #[cfg(unix)]
                std::os::unix::fs::symlink(
                    PathBuf::from("..").join("central").join(id),
                    &installed,
                )
                .unwrap();
                #[cfg(windows)]
                std::os::windows::fs::symlink_dir(&central_skill, &installed).unwrap();
            }
            "copy" | "native" => {
                fs::create_dir_all(&installed).unwrap();
                fs::write(installed.join("SKILL.md"), "---\nname: Test\n---\n").unwrap();
            }
            _ => unreachable!(),
        }
        db::upsert_skill(
            pool,
            &Skill {
                id: id.to_string(),
                name: format!("이름 {id}"),
                description: None,
                file_path: central_skill
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: Some(central_skill.to_string_lossy().into_owned()),
                is_central: true,
                source: None,
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        db::upsert_skill_installation(
            pool,
            &SkillInstallation {
                skill_id: id.to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: installed.to_string_lossy().into_owned(),
                link_type: link_type.to_string(),
                symlink_target: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn individual_pause_and_restore_preserves_changed_copy() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "copy-skill", "copy").await;
        let installed = agent.join("copy-skill");
        fs::write(installed.join("user-note.txt"), "keep this edit").unwrap();

        set_skill_usage_impl(&pool, "copy-skill", "claude-code", false)
            .await
            .unwrap();
        assert!(fs::symlink_metadata(&installed).is_err());
        assert!(
            db::get_skill_installation(&pool, "copy-skill", "claude-code")
                .await
                .unwrap()
                .is_none()
        );

        set_skill_usage_impl(&pool, "copy-skill", "claude-code", true)
            .await
            .unwrap();
        assert_eq!(
            fs::read_to_string(installed.join("user-note.txt")).unwrap(),
            "keep this edit"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlink_restore_keeps_original_relative_target() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "link-skill", "symlink").await;
        let installed = agent.join("link-skill");
        let before = fs::read_link(&installed).unwrap();

        set_skill_usage_impl(&pool, "link-skill", "claude-code", false)
            .await
            .unwrap();
        set_skill_usage_impl(&pool, "link-skill", "claude-code", true)
            .await
            .unwrap();
        assert_eq!(fs::read_link(installed).unwrap(), before);
    }

    #[tokio::test]
    async fn bulk_restore_keeps_previously_individual_pause_stopped() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "individual", "copy").await;
        add_skill(&pool, &central, &agent, "bulk", "copy").await;

        set_skill_usage_impl(&pool, "individual", "claude-code", false)
            .await
            .unwrap();
        set_platform_usage_impl(&pool, "claude-code", false)
            .await
            .unwrap();
        set_platform_usage_impl(&pool, "claude-code", true)
            .await
            .unwrap();

        assert!(agent.join("bulk").exists());
        assert!(fs::symlink_metadata(agent.join("individual")).is_err());
        let individual = db::get_paused_installation(&pool, "individual", "claude-code")
            .await
            .unwrap()
            .unwrap();
        assert!(!individual.paused_by_bulk);
    }

    #[tokio::test]
    async fn bulk_pause_partial_failure_keeps_only_successes_marked_for_restore() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "good", "copy").await;
        add_skill(&pool, &central, &agent, "missing", "copy").await;
        fs::remove_dir_all(agent.join("missing")).unwrap();

        assert!(set_platform_usage_impl(&pool, "claude-code", false)
            .await
            .is_err());
        let good = db::get_paused_installation(&pool, "good", "claude-code")
            .await
            .unwrap()
            .unwrap();
        assert!(good.paused_by_bulk);
        assert!(db::get_paused_installation(&pool, "missing", "claude-code")
            .await
            .unwrap()
            .is_none());

        set_platform_usage_impl(&pool, "claude-code", true)
            .await
            .unwrap();
        assert!(agent.join("good").is_dir());
        assert!(db::get_paused_installation(&pool, "good", "claude-code")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn paused_metadata_survives_skill_cleanup() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "stale", "copy").await;
        set_skill_usage_impl(&pool, "stale", "claude-code", false)
            .await
            .unwrap();
        db::delete_skills_not_in_scope(&pool, &[]).await.unwrap();
        assert!(db::get_skill_by_id(&pool, "stale").await.unwrap().is_some());

        let status = get_skill_usage_status_impl(&pool).await.unwrap();
        let platform = status
            .iter()
            .find(|status| status.agent_id == "claude-code")
            .unwrap();
        assert_eq!(platform.paused_count, 1);
        assert_eq!(platform.skills[0].name, "이름 stale");

        set_skill_usage_impl(&pool, "stale", "claude-code", true)
            .await
            .unwrap();
        let restored = db::get_skills_for_agent(&pool, "claude-code")
            .await
            .unwrap();
        assert!(restored.iter().any(|skill| skill.id == "stale"));
    }

    #[tokio::test]
    async fn paused_installation_survives_database_reopen() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "restart", "copy").await;
        set_skill_usage_impl(&pool, "restart", "claude-code", false)
            .await
            .unwrap();
        pool.close().await;

        let db_path = temp.path().join("db.sqlite");
        let reopened = db::create_pool(&db_path.to_string_lossy()).await.unwrap();
        let status = get_skill_usage_status_impl(&reopened).await.unwrap();
        let platform = status
            .iter()
            .find(|status| status.agent_id == "claude-code")
            .unwrap();
        assert_eq!(platform.paused_count, 1);
        assert_eq!(platform.skills[0].skill_id, "restart");
    }

    #[tokio::test]
    async fn pause_refuses_installation_path_shared_by_another_platform() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "shared-path", "copy").await;
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "shared-path".to_string(),
                agent_id: "cursor".to_string(),
                installed_path: agent.join("shared-path").to_string_lossy().into_owned(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        assert!(
            set_skill_usage_impl(&pool, "shared-path", "claude-code", false)
                .await
                .is_err()
        );
        assert!(agent.join("shared-path").is_dir());
    }

    #[tokio::test]
    async fn restore_refuses_path_collision_without_losing_pause() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "collision", "copy").await;
        set_skill_usage_impl(&pool, "collision", "claude-code", false)
            .await
            .unwrap();
        fs::create_dir_all(agent.join("collision")).unwrap();
        fs::write(agent.join("collision/user.txt"), "user file").unwrap();

        assert!(
            set_skill_usage_impl(&pool, "collision", "claude-code", true)
                .await
                .is_err()
        );
        assert_eq!(
            fs::read_to_string(agent.join("collision/user.txt")).unwrap(),
            "user file"
        );
        assert!(
            db::get_paused_installation(&pool, "collision", "claude-code")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn delete_active_symlink_removes_only_platform_link() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "active-link", "symlink").await;

        delete_skill_from_agent_impl(&pool, "active-link", "claude-code")
            .await
            .unwrap();

        assert!(fs::symlink_metadata(agent.join("active-link")).is_err());
        assert!(central.join("active-link/SKILL.md").is_file());
        assert!(
            db::get_skill_installation(&pool, "active-link", "claude-code")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn delete_paused_symlink_removes_only_preserved_link() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "paused-link", "symlink").await;
        set_skill_usage_impl(&pool, "paused-link", "claude-code", false)
            .await
            .unwrap();
        let paused = db::get_paused_installation(&pool, "paused-link", "claude-code")
            .await
            .unwrap()
            .unwrap();

        delete_skill_from_agent_impl(&pool, "paused-link", "claude-code")
            .await
            .unwrap();

        assert!(fs::symlink_metadata(paused.paused_path).is_err());
        assert!(central.join("paused-link/SKILL.md").is_file());
        assert!(
            db::get_paused_installation(&pool, "paused-link", "claude-code")
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn delete_paused_copy_keeps_recoverable_original_installation() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "paused-copy", "copy").await;
        fs::write(agent.join("paused-copy/user-note.txt"), "keep me").unwrap();
        set_skill_usage_impl(&pool, "paused-copy", "claude-code", false)
            .await
            .unwrap();
        let paused = db::get_paused_installation(&pool, "paused-copy", "claude-code")
            .await
            .unwrap()
            .unwrap();

        delete_skill_from_agent_impl(&pool, "paused-copy", "claude-code")
            .await
            .unwrap();

        assert!(fs::symlink_metadata(&paused.paused_path).is_err());
        assert!(central.join("paused-copy/SKILL.md").is_file());
        let backup = recovery::list_recovery_entries_impl(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|entry| entry.label == "비활성 설치 백업: paused-copy")
            .unwrap();
        assert_eq!(
            PathBuf::from(&backup.original_path),
            agent.canonicalize().unwrap().join("paused-copy")
        );

        recovery::restore_recovery_entry_impl(&pool, &backup.id)
            .await
            .unwrap();
        assert_eq!(
            fs::read_to_string(agent.join("paused-copy/user-note.txt")).unwrap(),
            "keep me"
        );
        assert!(
            db::get_skill_installation(&pool, "paused-copy", "claude-code")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn delete_paused_native_keeps_recoverable_original_installation() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "paused-native", "native").await;
        fs::write(agent.join("paused-native/user-note.txt"), "native pause").unwrap();
        set_skill_usage_impl(&pool, "paused-native", "claude-code", false)
            .await
            .unwrap();

        delete_skill_from_agent_impl(&pool, "paused-native", "claude-code")
            .await
            .unwrap();

        let backup = recovery::list_recovery_entries_impl(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|entry| entry.label == "비활성 설치 백업: paused-native")
            .unwrap();
        recovery::restore_recovery_entry_impl(&pool, &backup.id)
            .await
            .unwrap();
        assert_eq!(
            fs::read_to_string(agent.join("paused-native/user-note.txt")).unwrap(),
            "native pause"
        );
        assert_eq!(
            db::get_skill_installation(&pool, "paused-native", "claude-code")
                .await
                .unwrap()
                .unwrap()
                .link_type,
            "native"
        );
    }

    #[tokio::test]
    async fn delete_active_native_keeps_recovery_manifest() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "native-install", "native").await;
        fs::write(agent.join("native-install/user-note.txt"), "native edit").unwrap();

        delete_skill_from_agent_impl(&pool, "native-install", "claude-code")
            .await
            .unwrap();

        assert!(fs::symlink_metadata(agent.join("native-install")).is_err());
        let backup = recovery::list_recovery_entries_impl(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|entry| entry.label == "설치 백업: native-install")
            .unwrap();
        recovery::restore_recovery_entry_impl(&pool, &backup.id)
            .await
            .unwrap();
        assert_eq!(
            fs::read_to_string(agent.join("native-install/user-note.txt")).unwrap(),
            "native edit"
        );
        assert_eq!(
            db::get_skill_installation(&pool, "native-install", "claude-code")
                .await
                .unwrap()
                .unwrap()
                .link_type,
            "native"
        );
    }

    #[tokio::test]
    async fn delete_platform_removes_active_and_paused_but_preserves_failures_and_external() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "active", "copy").await;
        add_skill(&pool, &central, &agent, "paused", "copy").await;
        set_skill_usage_impl(&pool, "paused", "claude-code", false)
            .await
            .unwrap();
        add_skill(&pool, &central, &agent, "unsafe", "copy").await;
        let outside = temp.path().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("SKILL.md"), "do not remove").unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "unsafe".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: outside.to_string_lossy().into_owned(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        db::upsert_agent_skill_observation(
            &pool,
            &db::AgentSkillObservation {
                row_id: "plugin-row-delete".to_string(),
                agent_id: "claude-code".to_string(),
                skill_id: "plugin-skill".to_string(),
                name: "Plugin skill".to_string(),
                description: None,
                file_path: "/plugin/SKILL.md".to_string(),
                dir_path: "/plugin".to_string(),
                source_kind: "plugin".to_string(),
                source_root: "/plugin".to_string(),
                source_label: Some("Plugin".to_string()),
                link_type: "copy".to_string(),
                symlink_target: None,
                is_read_only: true,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let result = delete_platform_installations_impl(&pool, "claude-code")
            .await
            .unwrap();

        assert_eq!(result.deleted, vec!["active", "paused"]);
        assert_eq!(result.failed.len(), 1);
        assert_eq!(result.failed[0].skill_id, "unsafe");
        assert!(outside.join("SKILL.md").is_file());
        assert!(db::get_skill_installation(&pool, "unsafe", "claude-code")
            .await
            .unwrap()
            .is_some());
        assert_eq!(
            db::get_agent_skill_observations(&pool, "claude-code")
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn delete_platform_refuses_shared_universal_install_root() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "universal-only", "symlink").await;
        let installation = db::get_skill_installation(&pool, "universal-only", "claude-code")
            .await
            .unwrap()
            .unwrap();
        db::delete_skill_installation(&pool, "universal-only", "claude-code")
            .await
            .unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'universal'")
            .bind(agent.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                agent_id: "universal".to_string(),
                ..installation
            },
        )
        .await
        .unwrap();

        assert!(delete_platform_installations_impl(&pool, "claude-code")
            .await
            .is_err());
        assert!(fs::symlink_metadata(agent.join("universal-only")).is_ok());
        assert!(
            db::get_skill_installation(&pool, "universal-only", "universal")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn delete_platform_allows_managed_universal_installation() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "universal-delete", "symlink").await;
        let installation = db::get_skill_installation(&pool, "universal-delete", "claude-code")
            .await
            .unwrap()
            .unwrap();
        db::delete_skill_installation(&pool, "universal-delete", "claude-code")
            .await
            .unwrap();
        fs::remove_file(agent.join("universal-delete")).unwrap();
        let universal_root = temp.path().join("universal");
        fs::create_dir_all(&universal_root).unwrap();
        let universal_path = universal_root.join("universal-delete");
        std::os::unix::fs::symlink(central.join("universal-delete"), &universal_path).unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'universal'")
            .bind(universal_root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                agent_id: "universal".to_string(),
                installed_path: universal_path.to_string_lossy().into_owned(),
                ..installation
            },
        )
        .await
        .unwrap();

        let result = delete_platform_installations_impl(&pool, "universal")
            .await
            .unwrap();

        assert_eq!(result.deleted, vec!["universal-delete"]);
        assert!(result.failed.is_empty());
        assert!(fs::symlink_metadata(&universal_path).is_err());
        assert!(central.join("universal-delete/SKILL.md").is_file());
    }

    #[tokio::test]
    async fn delete_refuses_paused_path_outside_private_store() {
        let temp = TempDir::new().unwrap();
        let (pool, _central, agent) = setup(&temp).await;
        let outside = temp.path().join("outside-paused");
        fs::create_dir_all(&outside).unwrap();
        db::upsert_paused_installation(
            &pool,
            &PausedInstallation {
                skill_id: "traversal".to_string(),
                agent_id: "claude-code".to_string(),
                skill_name: "Traversal".to_string(),
                installed_path: agent.join("traversal").to_string_lossy().into_owned(),
                paused_path: outside.to_string_lossy().into_owned(),
                link_type: "copy".to_string(),
                symlink_target: None,
                paused_by_bulk: false,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        assert!(
            delete_skill_from_agent_impl(&pool, "traversal", "claude-code")
                .await
                .is_err()
        );
        assert!(outside.is_dir());
        assert!(
            db::get_paused_installation(&pool, "traversal", "claude-code")
                .await
                .unwrap()
                .is_some()
        );
    }

    #[tokio::test]
    async fn managed_observation_is_not_counted_as_external_but_other_sources_are() {
        let temp = TempDir::new().unwrap();
        let (pool, central, agent) = setup(&temp).await;
        add_skill(&pool, &central, &agent, "sample", "copy").await;
        let mut observation = db::AgentSkillObservation {
            row_id: "managed-row".into(),
            agent_id: "claude-code".into(),
            skill_id: "sample".into(),
            name: "sample".into(),
            description: None,
            file_path: agent.join("sample/SKILL.md").to_string_lossy().into_owned(),
            dir_path: agent.join("sample").to_string_lossy().into_owned(),
            source_kind: "user".into(),
            source_root: agent.to_string_lossy().into_owned(),
            source_label: None,
            link_type: "copy".into(),
            symlink_target: None,
            is_read_only: false,
            scanned_at: chrono::Utc::now().to_rfc3339(),
        };
        db::upsert_agent_skill_observation(&pool, &observation)
            .await
            .unwrap();
        let statuses = get_skill_usage_status_impl(&pool).await.unwrap();
        let status = statuses
            .iter()
            .find(|s| s.agent_id == "claude-code")
            .unwrap();
        assert_eq!(status.active_count, 1);
        assert_eq!(status.external_count, 0);

        // 이름이 같아도 다른 경로의 플러그인은 삭제 대상이 아니다.
        observation.row_id = "plugin-row".into();
        observation.dir_path = "/plugins/sample".into();
        observation.file_path = "/plugins/sample/SKILL.md".into();
        observation.source_kind = "plugin".into();
        observation.is_read_only = true;
        db::upsert_agent_skill_observation(&pool, &observation)
            .await
            .unwrap();
        let statuses = get_skill_usage_status_impl(&pool).await.unwrap();
        assert_eq!(
            statuses
                .iter()
                .find(|s| s.agent_id == "claude-code")
                .unwrap()
                .external_count,
            1
        );
    }

    #[tokio::test]
    async fn external_observations_are_reported_separately() {
        let temp = TempDir::new().unwrap();
        let (pool, _central, _agent) = setup(&temp).await;
        db::upsert_agent_skill_observation(
            &pool,
            &db::AgentSkillObservation {
                row_id: "plugin-row".to_string(),
                agent_id: "claude-code".to_string(),
                skill_id: "plugin-skill".to_string(),
                name: "Plugin skill".to_string(),
                description: None,
                file_path: "/plugin/SKILL.md".to_string(),
                dir_path: "/plugin".to_string(),
                source_kind: "plugin".to_string(),
                source_root: "/plugin".to_string(),
                source_label: Some("Plugin".to_string()),
                link_type: "copy".to_string(),
                symlink_target: None,
                is_read_only: true,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let status = get_skill_usage_status_impl(&pool).await.unwrap();
        let platform = status
            .iter()
            .find(|status| status.agent_id == "claude-code")
            .unwrap();
        assert_eq!(platform.external_count, 1);
        assert!(platform.skills.is_empty());
    }

    #[tokio::test]
    async fn shared_universal_installation_is_reported_as_external() {
        let temp = TempDir::new().unwrap();
        let (pool, central, _agent) = setup(&temp).await;
        let universal_root = temp.path().join("universal");
        let universal_skill = universal_root.join("shared-skill");
        fs::create_dir_all(&universal_skill).unwrap();
        fs::write(universal_skill.join("SKILL.md"), "---\nname: Shared\n---\n").unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'universal'")
            .bind(universal_root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        db::upsert_skill(
            &pool,
            &Skill {
                id: "shared-skill".to_string(),
                name: "Shared".to_string(),
                description: None,
                file_path: central
                    .join("shared-skill/SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                canonical_path: None,
                is_central: false,
                source: None,
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "shared-skill".to_string(),
                agent_id: "universal".to_string(),
                installed_path: universal_skill.to_string_lossy().into_owned(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        let status = get_skill_usage_status_impl(&pool).await.unwrap();
        let codex = status
            .iter()
            .find(|status| status.agent_id == "codex")
            .unwrap();
        assert_eq!(codex.external_count, 1);
        assert!(codex.skills.is_empty());
    }
    // ─── Shared install regressions (isolated TempDir/file-DB, no env) ───

    #[tokio::test]
    async fn shared_external_install_scan_pause_restore_and_delete_preserve_files() {
        for link_type in ["copy", "symlink"] {
            let tmp = TempDir::new().unwrap();
            let (pool, vault, universal, _) = shared_setup(&tmp).await;
            sqlx::query("DELETE FROM agents WHERE id NOT IN ('central', 'universal')")
                .execute(&pool)
                .await
                .unwrap();
            sqlx::query("DELETE FROM scan_directories")
                .execute(&pool)
                .await
                .unwrap();
            let installed = universal.join("external-skill");
            let source = if link_type == "symlink" {
                tmp.path().join("external-source")
            } else {
                installed.clone()
            };
            fs::create_dir_all(source.join("references")).unwrap();
            let content = "---\nname: external-skill\n---\n외부에서 설치한 스킬\n";
            fs::write(source.join("SKILL.md"), content).unwrap();
            fs::write(source.join("references/guide.md"), "함께 보존할 파일").unwrap();
            if link_type == "symlink" {
                #[cfg(unix)]
                std::os::unix::fs::symlink(&source, &installed).unwrap();
                #[cfg(windows)]
                std::os::windows::fs::symlink_dir(&source, &installed).unwrap();
            }

            // 설치 명령이나 사전 등록 없이 스캔만 해도 기존 제어를 사용할 수 있다.
            crate::commands::scanner::scan_all_skills_impl(&pool)
                .await
                .unwrap();
            let skills = db::get_skills_for_agent(&pool, "universal").await.unwrap();
            assert_eq!(skills.len(), 1);
            assert!(!skills[0].is_read_only);
            let id = shared_entry_key(&installed.to_string_lossy());
            for _ in 0..2 {
                let impact = compute_shared_impact(&pool, &id).await.unwrap();
                assert!(impact.reason.is_none());
                let result =
                    set_shared_skill_usage_impl(&pool, &id, false, &impact.confirmation_token)
                        .await
                        .unwrap();
                assert!(result.applied);
                assert!(fs::symlink_metadata(&installed).is_err());
                crate::commands::scanner::scan_all_skills_impl(&pool)
                    .await
                    .unwrap();
                let impact = compute_shared_impact(&pool, &id).await.unwrap();
                assert!(!impact.enabled);
                let result =
                    set_shared_skill_usage_impl(&pool, &id, true, &impact.confirmation_token)
                        .await
                        .unwrap();
                assert!(result.applied);
                crate::commands::scanner::scan_all_skills_impl(&pool)
                    .await
                    .unwrap();
                assert_eq!(
                    fs::read_to_string(installed.join("SKILL.md")).unwrap(),
                    content
                );
                assert_eq!(
                    fs::read_to_string(installed.join("references/guide.md")).unwrap(),
                    "함께 보존할 파일"
                );
            }

            delete_skill_from_agent_impl(&pool, "external-skill", "universal")
                .await
                .unwrap();
            assert!(fs::symlink_metadata(&installed).is_err());
            if link_type == "copy" {
                let backups = recovery::list_recovery_entries_impl(&pool).await.unwrap();
                assert_eq!(backups.len(), 1);
                recovery::restore_recovery_entry_impl(&pool, &backups[0].id)
                    .await
                    .unwrap();
            }
            assert_eq!(
                fs::read_to_string(source.join("SKILL.md")).unwrap(),
                content
            );
            assert!(vault.read_dir().unwrap().next().is_none());
        }
    }

    async fn shared_setup(tmp: &TempDir) -> (DbPool, PathBuf, PathBuf, PathBuf) {
        let db_path = tmp.path().join("db.sqlite");
        let pool = db::create_pool(&db_path.to_string_lossy()).await.unwrap();
        db::init_database(&pool).await.unwrap();
        let vault = tmp.path().join("vault");
        let universal = tmp.path().join("agents-skills");
        let codex = tmp.path().join("agents-skills");
        fs::create_dir_all(&vault).unwrap();
        fs::create_dir_all(&universal).unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'central'")
            .bind(vault.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'universal'")
            .bind(universal.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'codex'")
            .bind(codex.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        (pool, vault, universal, codex)
    }

    async fn put_skill(pool: &DbPool, id: &str, name: &str, dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("SKILL.md"), format!("---\nname: {name}\n---\n")).unwrap();
        db::upsert_skill(
            pool,
            &Skill {
                id: id.to_string(),
                name: name.to_string(),
                description: None,
                file_path: dir.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: None,
                is_central: false,
                source: None,
                content: None,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
    }

    async fn put_install(
        pool: &DbPool,
        skill_id: &str,
        agent_id: &str,
        path: &Path,
        link_type: &str,
        symlink_target: Option<String>,
    ) {
        db::upsert_skill_installation(
            pool,
            &SkillInstallation {
                skill_id: skill_id.to_string(),
                agent_id: agent_id.to_string(),
                installed_path: path.to_string_lossy().into_owned(),
                link_type: link_type.to_string(),
                symlink_target,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
    }

    #[test]
    fn shared_entry_key_keeps_independent_links_separate() {
        let tmp = TempDir::new().unwrap();
        let vault_skill = tmp.path().join("vault").join("s");
        fs::create_dir_all(&vault_skill).unwrap();
        let a_dir = tmp.path().join("a");
        let b_dir = tmp.path().join("b");
        fs::create_dir_all(&a_dir).unwrap();
        fs::create_dir_all(&b_dir).unwrap();
        let a_link = a_dir.join("s");
        let b_link = b_dir.join("s");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&vault_skill, &a_link).unwrap();
            std::os::unix::fs::symlink(&vault_skill, &b_link).unwrap();
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(&vault_skill, &a_link).unwrap();
            std::os::windows::fs::symlink_dir(&vault_skill, &b_link).unwrap();
        }
        // Same target, different entries: distinct IDs.
        assert_ne!(
            shared_entry_key(&a_link.to_string_lossy()),
            shared_entry_key(&b_link.to_string_lossy())
        );
        // Same entry, different spelling of an existing parent: same ID.
        assert_eq!(
            shared_entry_key(&a_link.to_string_lossy()),
            shared_entry_key(&a_link.to_string_lossy())
        );
    }

    #[tokio::test]
    async fn shared_same_entry_groups_universal_and_codex() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let dir = universal.join("shared-skill");
        put_skill(&pool, "shared-skill", "shared-skill", &dir).await;
        put_install(&pool, "shared-skill", "universal", &dir, "copy", None).await;
        put_install(&pool, "shared-skill", "codex", &dir, "copy", None).await;
        let id = shared_entry_key(&dir.to_string_lossy());
        let impact = compute_shared_impact(&pool, &id).await.unwrap();
        assert!(impact.reason.is_none());
        assert!(impact.enabled);
        let ids: Vec<_> = impact
            .confirmed_platforms
            .iter()
            .map(|c| c.agent_id.as_str())
            .collect();
        assert!(
            !ids.contains(&"universal"),
            "universal is a shared root, not a confirmed reader"
        );
        assert!(ids.contains(&"codex"));
        assert!(impact.separate_installs.is_empty());
        let paused = set_shared_skill_usage_impl(&pool, &id, false, &impact.confirmation_token)
            .await
            .unwrap();
        assert!(paused.applied);
        assert!(
            db::get_paused_installation(&pool, "shared-skill", "universal")
                .await
                .unwrap()
                .is_some()
        );
        assert!(db::get_paused_installation(&pool, "shared-skill", "codex")
            .await
            .unwrap()
            .is_some());
        assert!(fs::symlink_metadata(&dir).is_err());
    }

    #[tokio::test]
    async fn shared_independent_symlinks_to_vault_move_separately() {
        let tmp = TempDir::new().unwrap();
        let (pool, vault, _universal, _codex) = shared_setup(&tmp).await;
        let agent_a = tmp.path().join("agent-a");
        let agent_b = tmp.path().join("agent-b");
        fs::create_dir_all(&agent_a).unwrap();
        fs::create_dir_all(&agent_b).unwrap();
        for (id, dir) in [("codex", &agent_a), ("cline", &agent_b)] {
            sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = ?")
                .bind(dir.to_string_lossy().to_string())
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        let vault_skill = vault.join("vskill");
        put_skill(&pool, "vskill", "vskill", &vault_skill).await;
        let a_link = agent_a.join("vskill");
        let b_link = agent_b.join("vskill");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&vault_skill, &a_link).unwrap();
            std::os::unix::fs::symlink(&vault_skill, &b_link).unwrap();
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(&vault_skill, &a_link).unwrap();
            std::os::windows::fs::symlink_dir(&vault_skill, &b_link).unwrap();
        }
        put_install(
            &pool,
            "vskill",
            "codex",
            &a_link,
            "symlink",
            Some(vault_skill.to_string_lossy().into_owned()),
        )
        .await;
        put_install(
            &pool,
            "vskill",
            "cline",
            &b_link,
            "symlink",
            Some(vault_skill.to_string_lossy().into_owned()),
        )
        .await;
        let id_a = shared_entry_key(&a_link.to_string_lossy());
        let id_b = shared_entry_key(&b_link.to_string_lossy());
        assert_ne!(id_a, id_b);
        // Pause A only: B link and vault target stay untouched.
        let impact_a = compute_shared_impact(&pool, &id_a).await.unwrap();
        assert!(impact_a.reason.is_none());
        set_shared_skill_usage_impl(&pool, &id_a, false, &impact_a.confirmation_token)
            .await
            .unwrap();
        assert!(fs::symlink_metadata(&a_link).is_err());
        assert!(fs::symlink_metadata(&b_link).is_ok());
        assert!(vault_skill.join("SKILL.md").exists());
    }

    #[tokio::test]
    async fn shared_same_name_different_source_stays_separate() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let other_root = tmp.path().join("other");
        fs::create_dir_all(&other_root).unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'codex'")
            .bind(other_root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        let dir_a = universal.join("dup");
        let dir_b = other_root.join("dup");
        put_skill(&pool, "dup", "dup", &dir_a).await;
        put_install(&pool, "dup", "universal", &dir_a, "copy", None).await;
        put_install(&pool, "dup", "codex", &dir_b, "copy", None).await;
        let id_a = shared_entry_key(&dir_a.to_string_lossy());
        let impact = compute_shared_impact(&pool, &id_a).await.unwrap();
        assert_eq!(impact.separate_installs.len(), 1);
        assert_eq!(impact.separate_installs[0].agent_id, "codex");
        assert_eq!(
            impact.separate_installs[0].source_path,
            dir_b.to_string_lossy()
        );
    }

    #[tokio::test]
    async fn shared_vault_overlap_returns_protection() {
        let tmp = TempDir::new().unwrap();
        let (pool, vault, _universal, _codex) = shared_setup(&tmp).await;
        let inside = vault.join("kept");
        put_skill(&pool, "kept", "kept", &inside).await;
        put_install(&pool, "kept", "universal", &inside, "copy", None).await;
        let id = shared_entry_key(&inside.to_string_lossy());
        let impact = compute_shared_impact(&pool, &id).await.unwrap();
        assert!(impact.reason.is_some());
        assert!(
            impact.management_path.contains("vault")
                || Path::new(&impact.management_path).exists()
                || !impact.management_path.is_empty()
        );
        let err = set_shared_skill_usage_impl(&pool, &id, false, &impact.confirmation_token)
            .await
            .unwrap_err();
        assert!(err.contains("보관함"));
        assert!(inside.exists());
    }

    #[tokio::test]
    async fn shared_plugin_owned_returns_protection() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let dir = universal.join("pskill");
        put_skill(&pool, "pskill", "pskill", &dir).await;
        put_install(&pool, "pskill", "universal", &dir, "copy", None).await;
        db::upsert_agent_skill_observation(
            &pool,
            &db::AgentSkillObservation {
                row_id: format!("universal::{}", dir.to_string_lossy()),
                agent_id: "universal".to_string(),
                skill_id: "pskill".to_string(),
                name: "pskill".to_string(),
                description: None,
                file_path: dir.join("SKILL.md").to_string_lossy().into_owned(),
                dir_path: dir.to_string_lossy().into_owned(),
                source_kind: "plugin".to_string(),
                source_root: "/tmp/fake-plugin-root".to_string(),
                source_label: None,
                link_type: "copy".to_string(),
                symlink_target: None,
                is_read_only: true,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        let id = shared_entry_key(&dir.to_string_lossy());
        let impact = compute_shared_impact(&pool, &id).await.unwrap();
        assert!(impact.reason.is_some());
        assert!(impact.reason.as_ref().unwrap().contains("플러그인"));
        assert_eq!(impact.management_path, "/tmp/fake-plugin-root");
    }

    #[tokio::test]
    async fn shared_pause_restore_twice_preserves_platform_exclusions() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let dir = universal.join("exc");
        put_skill(&pool, "exc", "exc", &dir).await;
        put_install(&pool, "exc", "universal", &dir, "copy", None).await;
        put_install(&pool, "exc", "codex", &dir, "copy", None).await;
        db::upsert_platform_skill_control(
            &pool,
            &db::PlatformSkillControl {
                agent_id: "codex".to_string(),
                source_path: dir.to_string_lossy().into_owned(),
                skill_name: "exc".to_string(),
                state: "inactive".to_string(),
                original_value: Some("on".to_string()),
                applied_value: "off".to_string(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        let id = shared_entry_key(&dir.to_string_lossy());
        for _ in 0..2 {
            let off = compute_shared_impact(&pool, &id).await.unwrap();
            assert!(off.reason.is_none());
            let r = set_shared_skill_usage_impl(&pool, &id, false, &off.confirmation_token)
                .await
                .unwrap();
            assert!(r.applied);
            assert!(!r.impact.enabled);
            let on = compute_shared_impact(&pool, &id).await.unwrap();
            let r = set_shared_skill_usage_impl(&pool, &id, true, &on.confirmation_token)
                .await
                .unwrap();
            assert!(r.applied);
            assert!(r.impact.enabled);
        }
        let kept = db::get_platform_skill_control(&pool, "codex", &dir.to_string_lossy())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(kept.state, "inactive");
        assert_eq!(kept.applied_value, "off");
        assert_eq!(kept.original_value.as_deref(), Some("on"));
    }

    #[tokio::test]
    async fn shared_stale_token_does_not_mutate() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let dir = universal.join("stale");
        put_skill(&pool, "stale", "stale", &dir).await;
        put_install(&pool, "stale", "universal", &dir, "copy", None).await;
        let id = shared_entry_key(&dir.to_string_lossy());
        let first = compute_shared_impact(&pool, &id).await.unwrap();
        // Newly discovered sharer changes the token scope.
        put_install(&pool, "stale", "codex", &dir, "copy", None).await;
        let result = set_shared_skill_usage_impl(&pool, &id, false, &first.confirmation_token)
            .await
            .unwrap();
        assert!(!result.applied);
        assert!(dir.exists());
        assert!(db::get_skill_installation(&pool, "stale", "universal")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn shared_identity_stable_while_inactive() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let dir = universal.join("stable");
        put_skill(&pool, "stable", "stable", &dir).await;
        put_install(&pool, "stable", "universal", &dir, "copy", None).await;
        let id_before = shared_entry_key(&dir.to_string_lossy());
        let impact = compute_shared_impact(&pool, &id_before).await.unwrap();
        set_shared_skill_usage_impl(&pool, &id_before, false, &impact.confirmation_token)
            .await
            .unwrap();
        let id_after = shared_entry_key(&dir.to_string_lossy());
        assert_eq!(id_before, id_after);
        let paused_impact = compute_shared_impact(&pool, &id_before).await.unwrap();
        assert!(!paused_impact.enabled);
    }

    #[tokio::test]
    async fn shared_restore_conflict_never_overwrites() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let dir = universal.join("conflict");
        put_skill(&pool, "conflict", "conflict", &dir).await;
        put_install(&pool, "conflict", "universal", &dir, "copy", None).await;
        let id = shared_entry_key(&dir.to_string_lossy());
        let impact = compute_shared_impact(&pool, &id).await.unwrap();
        set_shared_skill_usage_impl(&pool, &id, false, &impact.confirmation_token)
            .await
            .unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("SKILL.md"), "---\nname: intruder\n---\n").unwrap();
        let retry = compute_shared_impact(&pool, &id).await.unwrap();
        let err = set_shared_skill_usage_impl(&pool, &id, true, &retry.confirmation_token)
            .await
            .unwrap_err();
        assert!(err.contains("덮어쓰지"));
        assert_eq!(
            fs::read_to_string(dir.join("SKILL.md")).unwrap(),
            "---\nname: intruder\n---\n"
        );
        assert!(db::get_paused_installation(&pool, "conflict", "universal")
            .await
            .unwrap()
            .is_some());
    }

    #[test]
    fn shared_rollback_error_detail_preserved_including_failed_recovery() {
        let moved = rollback_move(
            Path::new("/tmp/skillsmanage-test-missing-src"),
            Path::new("/tmp/skillsmanage-test-missing-dst"),
        );
        assert!(moved.is_err());
        let both = rollback_errors(
            "원본 실패".to_string(),
            Some("DB 정리 실패".to_string()),
            Some("복구 실패".to_string()),
        );
        assert!(both.contains("원본 실패"));
        assert!(both.contains("DB 정리 실패"));
        assert!(both.contains("복구 실패"));
        let single = rollback_error("원본 실패".to_string(), Err("복구 실패".to_string()));
        assert!(single.contains("되돌리기에 실패"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn shared_pause_file_failure_cleans_db_row() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let dir = universal.join("gone");
        put_skill(&pool, "gone", "gone", &dir).await;
        put_install(&pool, "gone", "universal", &dir, "copy", None).await;
        let gone_id = shared_entry_key(&dir.to_string_lossy());
        let gone_impact = compute_shared_impact(&pool, &gone_id).await.unwrap();
        assert!(gone_impact.reason.is_none());

        // Destination parent exists so paused-root preflight and the paused-row
        // insert succeed; a non-writable dest then makes move_path fail and
        // the inserted paused row must be cleaned up.
        let paused_dir = tmp
            .path()
            .canonicalize()
            .unwrap()
            .join("paused-installations");
        fs::create_dir(&paused_dir).unwrap();
        let writable = paused_dir.metadata().unwrap().permissions();
        let mut locked = writable.clone();
        locked.set_mode(0o555);
        fs::set_permissions(&paused_dir, locked).unwrap();
        let result =
            set_shared_skill_usage_impl(&pool, &gone_id, false, &gone_impact.confirmation_token)
                .await;
        fs::set_permissions(&paused_dir, writable).unwrap();
        let err = result.expect_err("pause must fail after the file move, not succeed");
        assert!(
            err.contains("옮길 수 없습니다"),
            "expected move failure, got: {err}"
        );
        assert!(
            dir.join("SKILL.md").exists(),
            "live file stays when move rolls back"
        );
        assert!(db::get_skill_installation(&pool, "gone", "universal")
            .await
            .unwrap()
            .is_some());
        assert!(
            db::get_paused_installation(&pool, "gone", "universal")
                .await
                .unwrap()
                .is_none(),
            "paused row inserted before the failed move must be deleted"
        );
    }

    #[tokio::test]
    async fn shared_db_failure_leaves_live_file_and_reports() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let dir = universal.join("dbfail");
        put_skill(&pool, "dbfail", "dbfail", &dir).await;
        put_install(&pool, "dbfail", "universal", &dir, "copy", None).await;
        sqlx::query("DROP TABLE paused_installations")
            .execute(&pool)
            .await
            .unwrap();
        let id = shared_entry_key(&dir.to_string_lossy());
        let err = compute_shared_impact(&pool, &id).await.unwrap_err();
        assert!(!err.is_empty());
        assert!(dir.exists());
    }

    #[tokio::test]
    async fn nonuniversal_bulk_skips_shared_installs() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        // codex shares universal's root, so its entry is shared; the solo
        // entry under the same root has no other sharer.
        let shared_dir = universal.join("bulk-shared");
        let solo_dir = universal.join("bulk-solo");
        put_skill(&pool, "bulk-shared", "bulk-shared", &shared_dir).await;
        put_install(&pool, "bulk-shared", "codex", &shared_dir, "copy", None).await;
        put_install(&pool, "bulk-shared", "universal", &shared_dir, "copy", None).await;
        put_skill(&pool, "bulk-solo", "bulk-solo", &solo_dir).await;
        put_install(&pool, "bulk-solo", "codex", &solo_dir, "copy", None).await;
        set_platform_usage_impl(&pool, "codex", false)
            .await
            .unwrap();
        // Solo moved; shared codex entry untouched by non-universal bulk.
        assert!(fs::symlink_metadata(&solo_dir).is_err());
        assert!(shared_dir.exists());
        assert!(db::get_skill_installation(&pool, "bulk-shared", "codex")
            .await
            .unwrap()
            .is_some());
        assert!(db::get_paused_installation(&pool, "bulk-solo", "codex")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn shared_bulk_preflight_rejects_newly_discovered() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let first = universal.join("b1");
        put_skill(&pool, "b1", "b1", &first).await;
        put_install(&pool, "b1", "universal", &first, "copy", None).await;
        let id1 = shared_entry_key(&first.to_string_lossy());
        let impact1 = compute_shared_impact(&pool, &id1).await.unwrap();
        // Discover a second universal entry after confirmation was captured.
        let second = universal.join("b2");
        put_skill(&pool, "b2", "b2", &second).await;
        put_install(&pool, "b2", "universal", &second, "copy", None).await;
        let stale = vec![SharedConfirmation {
            shared_install_id: id1.clone(),
            confirmation_token: impact1.confirmation_token.clone(),
        }];
        let result = set_shared_platform_usage_impl(&pool, false, &stale)
            .await
            .unwrap();
        assert!(!result.applied);
        assert_eq!(result.impacts.len(), 2);
        assert!(first.exists() && second.exists());
    }

    #[tokio::test]
    async fn shared_bulk_disables_universal_and_restores_bulk_only() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        for name in ["u1", "u2"] {
            let dir = universal.join(name);
            put_skill(&pool, name, name, &dir).await;
            put_install(&pool, name, "universal", &dir, "copy", None).await;
        }
        let active = db::get_all_skill_installations(&pool).await.unwrap();
        let paused = db::get_all_paused_installations(&pool).await.unwrap();
        let ids = bulk_shared_ids_for_universal(&active, &paused, false);
        assert_eq!(ids.len(), 2);
        let mut confirmations = Vec::new();
        for id in &ids {
            confirmations.push(SharedConfirmation {
                shared_install_id: id.clone(),
                confirmation_token: compute_shared_impact(&pool, id)
                    .await
                    .unwrap()
                    .confirmation_token,
            });
        }
        let off = set_shared_platform_usage_impl(&pool, false, &confirmations)
            .await
            .unwrap();
        assert!(off.applied);
        assert!(off.failed.is_empty());
        // Bulk restore targets only paused_by_bulk rows.
        let active = db::get_all_skill_installations(&pool).await.unwrap();
        let paused = db::get_all_paused_installations(&pool).await.unwrap();
        let restore_ids = bulk_shared_ids_for_universal(&active, &paused, true);
        assert_eq!(restore_ids.len(), 2);
    }

    #[tokio::test]
    async fn shared_independent_copies_stay_separate() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let other = tmp.path().join("other-root");
        fs::create_dir_all(&other).unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'cline'")
            .bind(other.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        let dir_a = universal.join("copied");
        let dir_b = other.join("copied");
        put_skill(&pool, "copied", "copied", &dir_a).await;
        fs::create_dir_all(&dir_b).unwrap();
        fs::write(dir_b.join("SKILL.md"), "---\nname: copied\n---\n").unwrap();
        put_install(&pool, "copied", "universal", &dir_a, "copy", None).await;
        put_install(&pool, "copied", "cline", &dir_b, "copy", None).await;
        let id_a = shared_entry_key(&dir_a.to_string_lossy());
        let id_b = shared_entry_key(&dir_b.to_string_lossy());
        assert_ne!(id_a, id_b);
        let impact = compute_shared_impact(&pool, &id_a).await.unwrap();
        assert_eq!(impact.separate_installs.len(), 1);
        assert_eq!(impact.separate_installs[0].agent_id, "cline");
        set_shared_skill_usage_impl(&pool, &id_a, false, &impact.confirmation_token)
            .await
            .unwrap();
        assert!(fs::symlink_metadata(&dir_a).is_err());
        assert!(dir_b.join("SKILL.md").exists());
        assert!(db::get_skill_installation(&pool, "copied", "cline")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn shared_compatibility_observation_never_confirms() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let dir = universal.join("compat");
        put_skill(&pool, "compat", "compat", &dir).await;
        put_install(&pool, "compat", "universal", &dir, "copy", None).await;
        db::upsert_agent_skill_observation(
            &pool,
            &db::AgentSkillObservation {
                row_id: format!("claude-code::{}", dir.to_string_lossy()),
                agent_id: "claude-code".to_string(),
                skill_id: "compat".to_string(),
                name: "compat".to_string(),
                description: None,
                file_path: dir.join("SKILL.md").to_string_lossy().into_owned(),
                dir_path: dir.to_string_lossy().into_owned(),
                source_kind: "compatibility".to_string(),
                source_root: "/tmp/compat-root".to_string(),
                source_label: None,
                link_type: "copy".to_string(),
                symlink_target: None,
                is_read_only: true,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        let id = shared_entry_key(&dir.to_string_lossy());
        let impact = compute_shared_impact(&pool, &id).await.unwrap();
        let ids: Vec<_> = impact
            .confirmed_platforms
            .iter()
            .map(|c| c.agent_id.as_str())
            .collect();
        assert!(!ids.contains(&"claude-code"));
    }

    #[tokio::test]
    async fn shared_raw_path_spelling_uses_canonical_id_and_token() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let dir = universal.join("canon");
        put_skill(&pool, "canon", "canon", &dir).await;
        put_install(&pool, "canon", "universal", &dir, "copy", None).await;
        let raw = format!("{}/./canon", universal.to_string_lossy());
        let from_raw = compute_shared_impact(&pool, &raw).await.unwrap();
        let from_key = compute_shared_impact(&pool, &shared_entry_key(&dir.to_string_lossy()))
            .await
            .unwrap();
        assert_eq!(from_raw.shared_install_id, from_key.shared_install_id);
        assert_eq!(from_raw.confirmation_token, from_key.confirmation_token);
        let result = set_shared_skill_usage_impl(&pool, &raw, false, &from_key.confirmation_token)
            .await
            .unwrap();
        assert!(result.applied);
        assert!(!result.impact.enabled);
    }

    #[tokio::test]
    async fn shared_vault_equals_universal_root_stays_protected() {
        let tmp = TempDir::new().unwrap();
        let (pool, vault, _universal, _codex) = shared_setup(&tmp).await;
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'universal'")
            .bind(vault.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        let inside = vault.join("origin");
        put_skill(&pool, "origin", "origin", &inside).await;
        put_install(&pool, "origin", "universal", &inside, "copy", None).await;
        let id = shared_entry_key(&inside.to_string_lossy());
        let impact = compute_shared_impact(&pool, &id).await.unwrap();
        assert!(impact.reason.as_ref().unwrap().contains("보관함"));
        let err = set_shared_skill_usage_impl(&pool, &id, false, &impact.confirmation_token)
            .await
            .unwrap_err();
        assert!(err.contains("보관함"));
        assert!(inside.exists());
    }

    #[tokio::test]
    async fn shared_restore_db_failure_rolls_file_back_and_reports() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let dir = universal.join("rb");
        put_skill(&pool, "rb", "rb", &dir).await;
        put_install(&pool, "rb", "universal", &dir, "copy", None).await;
        let id = shared_entry_key(&dir.to_string_lossy());
        let impact = compute_shared_impact(&pool, &id).await.unwrap();
        set_shared_skill_usage_impl(&pool, &id, false, &impact.confirmation_token)
            .await
            .unwrap();
        let paused = db::get_paused_installation(&pool, "rb", "universal")
            .await
            .unwrap()
            .unwrap();
        let paused_path = PathBuf::from(&paused.paused_path);
        assert!(fs::symlink_metadata(&paused_path).is_ok());
        sqlx::query(
            "CREATE TRIGGER fail_skill_install_upsert
             BEFORE INSERT ON skill_installations
             BEGIN
               SELECT RAISE(ABORT, 'forced upsert failure');
             END",
        )
        .execute(&pool)
        .await
        .unwrap();
        let retry = compute_shared_impact(&pool, &id).await.unwrap();
        let err = set_shared_skill_usage_impl(&pool, &id, true, &retry.confirmation_token)
            .await
            .unwrap_err();
        assert!(!err.is_empty());
        assert!(
            fs::symlink_metadata(&paused_path).is_ok(),
            "file must roll back to paused path"
        );
        assert!(
            fs::symlink_metadata(&dir).is_err(),
            "restore must not leave a partial live file"
        );
        assert!(db::get_paused_installation(&pool, "rb", "universal")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn shared_bulk_accepts_raw_ids_and_surfaces_restricted() {
        let tmp = TempDir::new().unwrap();
        let (pool, vault, universal, _codex) = shared_setup(&tmp).await;
        let movable = universal.join("bulk-ok");
        put_skill(&pool, "bulk-ok", "bulk-ok", &movable).await;
        put_install(&pool, "bulk-ok", "universal", &movable, "copy", None).await;
        let protected = vault.join("bulk-vault");
        put_skill(&pool, "bulk-vault", "bulk-vault", &protected).await;
        put_install(&pool, "bulk-vault", "universal", &protected, "copy", None).await;
        let raw_ok = format!("{}/./bulk-ok", universal.to_string_lossy());
        let raw_vault = protected.to_string_lossy().into_owned();
        let impact_ok = compute_shared_impact(&pool, &raw_ok).await.unwrap();
        let impact_vault = compute_shared_impact(&pool, &raw_vault).await.unwrap();
        assert!(impact_vault.reason.is_some());
        let result = set_shared_platform_usage_impl(
            &pool,
            false,
            &[
                SharedConfirmation {
                    shared_install_id: raw_ok,
                    confirmation_token: impact_ok.confirmation_token,
                },
                SharedConfirmation {
                    shared_install_id: raw_vault,
                    confirmation_token: impact_vault.confirmation_token,
                },
            ],
        )
        .await
        .unwrap();
        assert!(!result.applied);
        assert!(result.failed.iter().any(|f| f.skill_id == "bulk-vault"));
        assert!(fs::symlink_metadata(&movable).is_err());
        assert!(protected.exists());
    }

    #[tokio::test]
    async fn shared_bulk_preflight_compute_error_is_not_silent() {
        let tmp = TempDir::new().unwrap();
        let (pool, _vault, universal, _codex) = shared_setup(&tmp).await;
        let dir = universal.join("bulk-err");
        put_skill(&pool, "bulk-err", "bulk-err", &dir).await;
        put_install(&pool, "bulk-err", "universal", &dir, "copy", None).await;
        let id = shared_entry_key(&dir.to_string_lossy());
        let impact = compute_shared_impact(&pool, &id).await.unwrap();
        sqlx::query("DROP TABLE agent_skill_observations")
            .execute(&pool)
            .await
            .unwrap();
        let err = set_shared_platform_usage_impl(
            &pool,
            false,
            &[SharedConfirmation {
                shared_install_id: id,
                confirmation_token: impact.confirmation_token,
            }],
        )
        .await
        .unwrap_err();
        assert!(!err.is_empty());
        assert!(dir.exists());
    }
}

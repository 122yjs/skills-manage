use serde::Serialize;
use sqlx::Row;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::State;
use tokio::sync::{Mutex, MutexGuard};
use uuid::Uuid;

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
        return Err("중지 설치는 파일 기반 데이터베이스에서만 사용할 수 있습니다".to_string());
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
            "심볼릭 링크를 중지 설치 보관소로 사용할 수 없습니다: {}",
            path.display()
        )),
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(format!(
            "중지 설치 보관소가 폴더가 아닙니다: {}",
            path.display()
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path.parent().ok_or_else(|| {
                format!(
                    "중지 설치 보관소의 상위 폴더가 없습니다: {}",
                    path.display()
                )
            })?;
            let parent_metadata = fs::symlink_metadata(parent).map_err(|parent_error| {
                format!(
                    "중지 설치 보관소의 상위 폴더를 확인할 수 없습니다 '{}': {parent_error}",
                    parent.display()
                )
            })?;
            if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
                return Err(format!(
                    "안전하지 않은 중지 설치 보관소 상위 경로입니다: {}",
                    parent.display()
                ));
            }
            fs::create_dir(path).map_err(|create_error| {
                format!(
                    "중지 설치 보관소를 만들 수 없습니다 '{}': {create_error}",
                    path.display()
                )
            })
        }
        Err(error) => Err(format!(
            "중지 설치 보관소를 확인할 수 없습니다 '{}': {error}",
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
            "플랫폼 스킬 폴더 밖의 설치는 중지할 수 없습니다: {}",
            installed_path.display()
        ));
    }
    Ok(())
}

fn validate_paused_path(root: &Path, paused_path: &Path) -> Result<(), String> {
    let parent = paused_path.parent().ok_or_else(|| {
        format!(
            "중지 설치 경로의 상위 폴더가 없습니다: {}",
            paused_path.display()
        )
    })?;
    if parent != root || paused_path.file_name().is_none() {
        return Err(format!(
            "중지 설치 보관소 밖의 경로는 복원할 수 없습니다: {}",
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
            "중지 설치 파일을 원래 위치로 되돌릴 수 없습니다 '{}': {error}",
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
                "다른 플랫폼과 같은 설치 경로를 공유하므로 중지할 수 없습니다: {}",
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
                "다른 플랫폼의 심볼릭 링크 원본이므로 중지할 수 없습니다: {}",
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
        return Err("중앙 보관함 스킬은 사용 중지할 수 없습니다".to_string());
    }
    let installation = db::get_skill_installation(pool, skill_id, agent_id)
        .await?
        .ok_or_else(|| format!("관리 중인 설치를 찾을 수 없습니다: {}", skill_id))?;
    if db::get_paused_installation(pool, skill_id, agent_id)
        .await?
        .is_some()
    {
        return Err("활성 설치와 중지 설치 기록이 함께 있어 파일을 옮기지 않습니다".to_string());
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
        return Err("중앙 보관함 스킬은 사용 전환 대상이 아닙니다".to_string());
    }
    if db::get_skill_installation(pool, skill_id, agent_id)
        .await?
        .is_some()
    {
        return Err(format!("이미 활성 설치 기록이 있습니다: {}", skill_id));
    }
    let paused = db::get_paused_installation(pool, skill_id, agent_id)
        .await?
        .ok_or_else(|| format!("중지된 관리 설치를 찾을 수 없습니다: {}", skill_id))?;
    let root = paused_root(pool).await?;
    let paused_path = Path::new(&paused.paused_path);
    validate_paused_path(&root, paused_path)?;
    let metadata = metadata_without_following_links(paused_path)?;
    if metadata.file_type().is_symlink() {
        let target = fs::read_link(paused_path).map_err(|error| {
            format!(
                "중지된 심볼릭 링크를 읽을 수 없습니다 '{}': {error}",
                paused_path.display()
            )
        })?;
        let target_text = target.to_string_lossy().into_owned();
        if paused.symlink_target.as_deref() != Some(target_text.as_str()) {
            return Err("중지된 심볼릭 링크 대상이 기록과 달라 복원을 중단했습니다".to_string());
        }
    } else if !metadata.is_dir() || !matches!(paused.link_type.as_str(), "copy" | "native") {
        return Err(format!(
            "중지된 관리 설치 파일 형식을 확인할 수 없습니다: {}",
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
        errors.push(format!("중지 기록 되돌리기 실패: {db_error}"));
    }
    if let Some(move_error) = move_error {
        errors.push(move_error);
    }
    errors.join("; ")
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
        return Err("중앙 보관함은 사용 전환 대상이 아닙니다".to_string());
    }

    let targets = if enabled {
        db::get_paused_installations_by_agent(pool, agent_id)
            .await?
            .into_iter()
            .filter(|installation| installation.paused_by_bulk)
            .map(|installation| installation.skill_id)
            .collect::<Vec<_>>()
    } else {
        db::get_skill_installations_by_agent(pool, agent_id)
            .await?
            .into_iter()
            .map(|installation| installation.skill_id)
            .collect::<Vec<_>>()
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
            "일부 스킬의 사용 상태를 바꾸지 못했습니다. 현재 상태를 다시 확인하세요: {}",
            failures.join(" | ")
        ))
    }
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
}

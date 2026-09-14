//! 공용 설치 삭제는 활성 여부와 관계없이 영향 확인 후 같은 경로로 처리한다.
use super::usage::{self, DeleteInstallationFailure, DeletePlatformInstallationsResult};
use crate::{
    db::{self, DbPool, PausedInstallation, SkillInstallation},
    AppState,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tauri::State;

#[derive(Clone, Debug, Serialize)]
pub struct SharedDeleteLink {
    pub agent_id: String,
    pub display_name: String,
    pub path: String,
    pub installed_path: String,
    pub target: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct SharedDeletePreview {
    pub skill_id: String,
    pub skill_name: String,
    pub enabled: bool,
    pub source_path: String,
    pub links: Vec<SharedDeleteLink>,
    pub confirmation_token: String,
}

#[derive(Deserialize)]
pub struct SharedDeleteConfirmation {
    pub skill_id: String,
    pub confirmation_token: String,
}

/// 설치 기록이 스캔에서 사라진 끊어진 링크도 실제 플랫폼 폴더에서 찾는다.
/// 같은 보관함 원본을 가리키는 독립 링크는 포함하지 않고 공용 설치 자체를 가리키는 링크만 찾는다.
pub(crate) async fn find_links(
    pool: &DbPool,
    source: &Path,
) -> Result<Vec<SharedDeleteLink>, String> {
    let agents = db::get_all_agents(pool).await?;
    let vault = db::get_central_skills_dir(pool).await?.canonicalize().ok();
    let observations = db::get_all_agent_skill_observations(pool).await?;
    let mut candidates = BTreeMap::<String, (String, String)>::new();
    for agent in &agents {
        if agent.id == "central" || agent.id == "universal" {
            continue;
        }
        let root = Path::new(&agent.global_skills_dir);
        match fs::read_dir(root) {
            Ok(entries) => {
                for entry in entries {
                    let entry = entry.map_err(|e| {
                        format!("{}의 연결을 확인할 수 없습니다: {e}", agent.display_name)
                    })?;
                    let path = entry.path().to_string_lossy().into_owned();
                    candidates
                        .entry(path.clone())
                        .or_insert((agent.id.clone(), path));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(format!(
                    "{}의 연결을 확인할 수 없습니다: {e}",
                    agent.display_name
                ))
            }
        }
    }
    for row in db::get_all_skill_installations(pool).await? {
        if row.agent_id != "central" && row.agent_id != "universal" {
            candidates.insert(
                row.installed_path.clone(),
                (row.agent_id, row.installed_path),
            );
        }
    }
    for row in db::get_all_paused_installations(pool).await? {
        if row.agent_id != "central" && row.agent_id != "universal" {
            candidates.insert(row.paused_path, (row.agent_id, row.installed_path));
        }
    }
    let mut links = BTreeMap::new();
    for (physical, (agent_id, installed)) in candidates {
        if usage::shared_entry_key(&installed) == usage::shared_entry_key(&source.to_string_lossy())
        {
            continue;
        }
        let target = match fs::read_link(&physical) {
            Ok(target) => target,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::InvalidInput | std::io::ErrorKind::NotFound
                ) =>
            {
                continue
            }
            Err(error) => return Err(format!("연결을 확인할 수 없습니다: {physical}: {error}")),
        };
        if !usage::link_target_matches(Path::new(&installed), &target, source) {
            continue;
        }
        let entry = usage::shared_entry_key(&physical);
        if vault
            .as_ref()
            .is_some_and(|root| Path::new(&entry).starts_with(root))
        {
            return Err(format!(
                "보관함 안의 연결은 공용 삭제로 제거하지 않습니다: {installed}"
            ));
        }
        if observations.iter().any(|o| {
            o.source_kind == "plugin"
                && usage::shared_entry_key(&o.dir_path) == usage::shared_entry_key(&installed)
        }) {
            return Err(format!(
                "플러그인 소유 연결은 플러그인 관리에서 해제하세요: {installed}"
            ));
        }
        let agent = agents
            .iter()
            .find(|a| a.id == agent_id)
            .ok_or("연결 플랫폼을 찾을 수 없습니다")?;
        // 관리 기록만 믿고 플랫폼 폴더 밖의 파일을 제거하지 않는다.
        let root = fs::canonicalize(&agent.global_skills_dir).map_err(|e| e.to_string())?;
        let normalized = usage::shared_entry_key(&installed);
        if !Path::new(&normalized).starts_with(&root) || Path::new(&normalized) == root {
            return Err(format!(
                "플랫폼 폴더 밖의 연결은 직접 확인하세요: {installed}"
            ));
        }
        if physical != installed {
            let paused_root = super::usage::paused_root(pool).await?;
            if Path::new(&physical).parent() != Some(paused_root.as_path()) {
                return Err(format!(
                    "비활성 보관소 밖의 연결은 직접 확인하세요: {physical}"
                ));
            }
        }
        links.insert(
            usage::shared_entry_key(&physical),
            SharedDeleteLink {
                agent_id,
                display_name: agent.display_name.clone(),
                path: physical,
                installed_path: installed,
                target: target.to_string_lossy().into_owned(),
            },
        );
    }
    Ok(links.into_values().collect())
}

pub async fn preview(pool: &DbPool, skill_id: &str) -> Result<SharedDeletePreview, String> {
    let active = db::get_skill_installation(pool, skill_id, "universal").await?;
    let paused = db::get_paused_installation(pool, skill_id, "universal").await?;
    if active.is_some() && paused.is_some() {
        return Err("활성·비활성 기록이 겹칩니다. 목록을 새로고침하세요".into());
    }
    let (source, physical) = match (&active, &paused) {
        (Some(row), _) => (&row.installed_path, &row.installed_path),
        (_, Some(row)) => (&row.installed_path, &row.paused_path),
        _ => return Err("공용 설치가 없습니다. 목록을 새로고침하세요".into()),
    };
    let impact = usage::compute_shared_impact(pool, source).await?;
    if let Some(reason) = impact.reason {
        return Err(reason);
    }
    let links = find_links(pool, Path::new(source)).await?;
    // 실제 경로/링크 대상과 설치 기록을 함께 묶어 확인 후 대상이 달라지면 중단한다.
    let fingerprint = format!(
        "{:?}|{:?}|{}|{}|{}",
        active,
        paused,
        usage::live_entry_fingerprint(physical),
        impact.confirmation_token,
        serde_json::to_string(&links).map_err(|e| e.to_string())?
    );
    Ok(SharedDeletePreview {
        skill_id: skill_id.into(),
        skill_name: impact.skill_name,
        enabled: active.is_some(),
        source_path: source.clone(),
        links,
        confirmation_token: format!("{:x}", Sha256::digest(fingerprint.as_bytes())),
    })
}

#[cfg(unix)]
fn restore_link(link: &SharedDeleteLink) -> Result<(), String> {
    std::os::unix::fs::symlink(&link.target, &link.path).map_err(|e| e.to_string())
}
#[cfg(windows)]
fn restore_link(link: &SharedDeleteLink) -> Result<(), String> {
    std::os::windows::fs::symlink_dir(&link.target, &link.path).map_err(|e| e.to_string())
}

async fn delete_confirmed(pool: &DbPool, plan: &SharedDeletePreview) -> Result<(), String> {
    let keys: Vec<_> = plan
        .links
        .iter()
        .map(|l| usage::shared_entry_key(&l.installed_path))
        .collect();
    let active: Vec<SkillInstallation> = db::get_all_skill_installations(pool)
        .await?
        .into_iter()
        .filter(|r| keys.contains(&usage::shared_entry_key(&r.installed_path)))
        .collect();
    let paused: Vec<PausedInstallation> = db::get_all_paused_installations(pool)
        .await?
        .into_iter()
        .filter(|r| keys.contains(&usage::shared_entry_key(&r.installed_path)))
        .collect();
    let mut removed = Vec::new();
    let result = async {
        for link in &plan.links {
            if fs::read_link(&link.path).ok() != Some(PathBuf::from(&link.target)) {
                return Err("연결이 변경되었습니다. 삭제 대상을 다시 확인하세요".into());
            }
            fs::remove_file(&link.path).map_err(|e| {
                format!(
                    "{}의 바로가기를 삭제하지 못했습니다: {e}",
                    link.display_name
                )
            })?;
            removed.push(link);
        }
        for row in &active {
            db::delete_skill_installation(pool, &row.skill_id, &row.agent_id).await?;
        }
        for row in &paused {
            db::delete_paused_installation(pool, &row.skill_id, &row.agent_id).await?;
        }
        usage::delete_managed_installation_locked(pool, &plan.skill_id, "universal").await
    }
    .await;
    if let Err(error) = result {
        let mut failures = vec![error];
        for link in removed {
            if let Err(e) = restore_link(link) {
                failures.push(format!("바로가기 복원 실패: {}: {e}", link.path));
            }
        }
        for row in &active {
            if let Err(e) = db::upsert_skill_installation(pool, row).await {
                failures.push(e);
            }
        }
        for row in &paused {
            if let Err(e) = db::upsert_paused_installation(pool, row).await {
                failures.push(e);
            }
        }
        return Err(failures.join("; "));
    }
    Ok(())
}

pub async fn delete(
    pool: &DbPool,
    confirmations: Vec<SharedDeleteConfirmation>,
) -> Result<DeletePlatformInstallationsResult, String> {
    let _guard = usage::usage_lock().await;
    let mut result = DeletePlatformInstallationsResult {
        deleted: vec![],
        failed: vec![],
    };
    let mut seen = std::collections::BTreeSet::new();
    for confirmation in confirmations {
        if !seen.insert(confirmation.skill_id.clone()) {
            continue;
        }
        let outcome = match preview(pool, &confirmation.skill_id).await {
            Ok(plan) if plan.confirmation_token == confirmation.confirmation_token => {
                delete_confirmed(pool, &plan).await
            }
            Ok(_) => Err("설치 상태나 연결이 변경되었습니다. 삭제 대상을 다시 확인하세요".into()),
            Err(error) => Err(error),
        };
        match outcome {
            Ok(()) => result.deleted.push(confirmation.skill_id),
            Err(error) => result.failed.push(DeleteInstallationFailure {
                skill_id: confirmation.skill_id,
                error,
            }),
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn preview_shared_install_delete(
    state: State<'_, AppState>,
    skill_id: String,
) -> Result<SharedDeletePreview, String> {
    let _guard = usage::usage_lock().await;
    preview(&state.db, &skill_id).await
}

#[tauri::command]
pub async fn delete_shared_installs(
    state: State<'_, AppState>,
    confirmations: Vec<SharedDeleteConfirmation>,
) -> Result<DeletePlatformInstallationsResult, String> {
    delete(&state.db, confirmations).await
}

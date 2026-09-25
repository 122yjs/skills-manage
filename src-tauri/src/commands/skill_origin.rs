use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Row};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tauri::State;
use tokio::sync::{Mutex, MutexGuard};
use uuid::Uuid;

use crate::commands::{github_import, marketplace, recovery, scanner, skills};
use crate::db::{self, DbPool};
use crate::AppState;

mod installation_records;
use installation_records::RecordedOrigin;

// 한 번의 전체 확인에서 같은 저장소를 여러 스킬 때문에 반복 다운로드하지 않는다.
type RemoteCache = HashMap<(String, Option<String>), RemoteSnapshot>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillTargetRequest {
    pub skill_id: String,
    pub agent_id: Option<String>,
    pub row_id: Option<String>,
}

#[derive(Debug, Clone)]
struct ResolvedSkillTarget {
    skill_id: String,
    agent_id: Option<String>,
    row_id: Option<String>,
    target_path: PathBuf,
    target_key: String,
    is_read_only: bool,
}

static SKILL_MUTATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) async fn mutation_lock() -> MutexGuard<'static, ()> {
    SKILL_MUTATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .await
}

const MAX_SKILL_FILES: usize = 5_000;
const MAX_SKILL_BYTES: usize = 100 * 1024 * 1024;
const MAX_SKILL_FILE_BYTES: usize = 20 * 1024 * 1024;
/// 한 번의 출처 요약 조회에 담는 대상 경로 수. SQLite의 바인딩 변수 한도보다
/// 충분히 작게 잡아 긴 목록에서도 조회가 실패하지 않게 합니다.
const MAX_ORIGIN_QUERY_KEYS: usize = 500;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct SkillManifest {
    pub entries: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, FromRow)]
struct SkillOriginRow {
    binding_id: String,
    target_key: String,
    skill_id: String,
    agent_id: Option<String>,
    target_path: String,
    repository_id: Option<String>,
    owner: String,
    repo: String,
    source_path: String,
    ref_name: String,
    baseline_state: String,
    base_commit_oid: Option<String>,
    base_manifest_json: Option<String>,
    last_applied_commit_oid: Option<String>,
    last_applied_at: Option<String>,
    last_checked_at: Option<String>,
    last_remote_commit_oid: Option<String>,
    last_remote_manifest_json: Option<String>,
    last_error: Option<String>,
    binding_version: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillOriginInfo {
    pub binding_id: String,
    pub target_key: String,
    pub target_path: String,
    pub repository_id: Option<String>,
    pub owner: String,
    pub repo: String,
    pub source_path: String,
    pub ref_name: String,
    pub baseline_state: String,
    pub base_commit_oid: Option<String>,
    pub last_applied_commit_oid: Option<String>,
    pub last_applied_at: Option<String>,
    pub last_checked_at: Option<String>,
    pub last_remote_commit_oid: Option<String>,
    pub last_error: Option<String>,
    pub binding_version: i64,
    pub can_update: bool,
}

/// 카드와 목록에 붙일 최소 GitHub 출처 요약입니다.
///
/// 기준 manifest나 마지막 확인 시각 같은 상세 정보는 `get_skill_origin`이
/// 돌려주는 전체 출처가 담당합니다.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubSkillOriginSummary {
    pub owner: String,
    pub repo: String,
    pub source_path: String,
    pub ref_name: String,
    pub update_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OriginSyncState {
    UnknownBaseline,
    UpToDate,
    RemoteUpdate,
    LocalChanges,
    Diverged,
    LocalMatchesRemote,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ManifestChangeSummary {
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillOriginStatus {
    pub origin: SkillOriginInfo,
    pub state: OriginSyncState,
    pub local_vs_remote: ManifestChangeSummary,
    pub local_vs_base: ManifestChangeSummary,
    pub remote_vs_base: ManifestChangeSummary,
    pub remote_commit_oid: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSkillOriginRequest {
    pub target: SkillTargetRequest,
    pub repo_url: String,
    pub source_path: String,
    pub ref_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillOriginCandidate {
    pub repo_url: String,
    pub source_path: String,
    pub ref_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillOriginDiscovery {
    pub origin: Option<SkillOriginInfo>,
    pub candidates: Vec<SkillOriginCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareSkillUpdateRequest {
    pub target: SkillTargetRequest,
    pub allow_local_changes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUpdatePlan {
    pub operation_id: String,
    pub binding_id: String,
    pub target_path: String,
    pub remote_commit_oid: String,
    pub state: OriginSyncState,
    pub changes: ManifestChangeSummary,
    pub requires_local_change_confirmation: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillUpdateResult {
    pub operation_id: String,
    pub binding_id: String,
    pub applied_commit_oid: String,
    pub recovery_entry_id: Option<String>,
}

#[derive(Debug, Clone)]
struct RemoteSnapshot {
    repository_id: Option<String>,
    owner: String,
    repo: String,
    ref_name: String,
    commit_oid: String,
    source_path: String,
    snapshot: Arc<github_import::GitHubRepoSnapshot>,
    manifest: SkillManifest,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn safe_relative_path(path: &str) -> bool {
    let path = Path::new(path);
    !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

fn remote_files(
    snapshot: &github_import::GitHubRepoSnapshot,
    source_path: &str,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let source_path = source_path.trim_matches('/');
    let mut files = BTreeMap::new();
    let mut total_bytes = 0usize;
    for (repo_path, bytes) in &snapshot.files {
        let relative = if source_path.is_empty() || source_path == "." {
            repo_path.clone()
        } else {
            let prefix = format!("{source_path}/");
            match repo_path.strip_prefix(&prefix) {
                Some(relative) if !relative.is_empty() => relative.to_string(),
                _ => continue,
            }
        };
        if !safe_relative_path(&relative) {
            return Err(format!("Unsupported repository path: {relative}"));
        }
        if bytes.len() > MAX_SKILL_FILE_BYTES {
            return Err(format!(
                "Repository file is too large to update safely: {relative}"
            ));
        }
        total_bytes = total_bytes.saturating_add(bytes.len());
        if files.len() >= MAX_SKILL_FILES || total_bytes > MAX_SKILL_BYTES {
            return Err("GitHub skill payload exceeds the safe update limit".to_string());
        }
        files.insert(relative, bytes.clone());
    }
    if files.is_empty() || !files.contains_key("SKILL.md") {
        return Err(format!(
            "GitHub source path '{}' does not contain SKILL.md",
            if source_path.is_empty() {
                "."
            } else {
                source_path
            }
        ));
    }
    Ok(files)
}

fn manifest_from_remote_files(files: &BTreeMap<String, Vec<u8>>) -> SkillManifest {
    SkillManifest {
        entries: files
            .iter()
            .map(|(path, bytes)| ManifestEntry {
                path: path.clone(),
                size: bytes.len() as u64,
                sha256: sha256_hex(bytes),
            })
            .collect(),
    }
}

fn collect_local_files(
    root: &Path,
    current: &Path,
    out: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(current)
        .map_err(|error| format!("Failed to read '{}': {error}", current.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Failed to read '{}': {error}", current.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("Failed to inspect '{}': {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "Skill contains a symbolic link that cannot be updated safely: {}",
                path.display()
            ));
        }
        if metadata.is_dir() {
            collect_local_files(root, &path, out)?;
        } else if metadata.is_file() {
            if metadata.len() as usize > MAX_SKILL_FILE_BYTES {
                return Err(format!(
                    "Skill file is too large to update safely: {}",
                    path.display()
                ));
            }
            let relative = path
                .strip_prefix(root)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let bytes = fs::read(&path)
                .map_err(|error| format!("Failed to read '{}': {error}", path.display()))?;
            let current_size = out.values().map(Vec::len).sum::<usize>();
            if out.len() >= MAX_SKILL_FILES
                || current_size.saturating_add(bytes.len()) > MAX_SKILL_BYTES
            {
                return Err("Local skill payload exceeds the safe update limit".to_string());
            }
            out.insert(relative, bytes);
        } else {
            return Err(format!("Unsupported file type: {}", path.display()));
        }
    }
    Ok(())
}

fn manifest_from_local_directory(root: &Path) -> Result<SkillManifest, String> {
    let metadata = fs::symlink_metadata(root)
        .map_err(|error| format!("Skill target is unavailable '{}': {error}", root.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "Skill target is not a managed directory: {}",
            root.display()
        ));
    }
    let mut files = BTreeMap::new();
    collect_local_files(root, root, &mut files)?;
    if !files.contains_key("SKILL.md") {
        return Err(format!(
            "Skill target does not contain SKILL.md: {}",
            root.display()
        ));
    }
    Ok(manifest_from_remote_files(&files))
}

fn manifest_map(manifest: &SkillManifest) -> BTreeMap<&str, (&str, u64)> {
    manifest
        .entries
        .iter()
        .map(|entry| (entry.path.as_str(), (entry.sha256.as_str(), entry.size)))
        .collect()
}

fn summarize_changes(left: &SkillManifest, right: &SkillManifest) -> ManifestChangeSummary {
    let left = manifest_map(left);
    let right = manifest_map(right);
    let keys = left
        .keys()
        .chain(right.keys())
        .copied()
        .collect::<BTreeSet<_>>();
    let mut summary = ManifestChangeSummary::default();
    for key in keys {
        match (left.get(key), right.get(key)) {
            (None, Some(_)) => summary.added += 1,
            (Some(_), None) => summary.removed += 1,
            (Some(left), Some(right)) if left != right => summary.modified += 1,
            _ => {}
        }
    }
    summary
}

fn manifest_json(manifest: &SkillManifest) -> Result<String, String> {
    serde_json::to_string(manifest).map_err(|error| error.to_string())
}

fn parse_manifest(value: Option<&str>) -> Option<SkillManifest> {
    value.and_then(|value| serde_json::from_str(value).ok())
}

/// 읽기 전용 관찰 행도 동일한 관리 원본을 가리키면 GitHub 연결과 업데이트는 허용합니다.
pub(crate) async fn can_manage_observed_origin(
    pool: &DbPool,
    skill_id: &str,
    source_kind: &str,
    path: &Path,
) -> Result<bool, String> {
    if source_kind != "compatibility" {
        return Ok(false);
    }
    let Ok(target) = path.canonicalize() else {
        return Ok(false);
    };
    let central_root = db::get_central_skills_dir(pool).await?;
    if central_root
        .canonicalize()
        .is_ok_and(|root| target != root && target.starts_with(root))
    {
        return Ok(true);
    }
    Ok(recovery::managed_copy_for_target(pool, skill_id, &target)
        .await?
        .is_some())
}

async fn resolve_target(
    pool: &DbPool,
    request: &SkillTargetRequest,
) -> Result<ResolvedSkillTarget, String> {
    let detail = skills::get_skill_detail_with_row_impl(
        pool,
        &request.skill_id,
        request.agent_id.as_deref(),
        request.row_id.as_deref(),
    )
    .await?;
    let installations = db::get_skill_installations(pool, &request.skill_id).await?;
    let selected_installation = request.agent_id.as_deref().and_then(|agent_id| {
        installations
            .iter()
            .find(|installation| installation.agent_id == agent_id)
    });
    let target_path = if detail.is_read_only {
        let path = PathBuf::from(&detail.dir_path);
        if detail.can_manage_origin {
            path.canonicalize()
                .map_err(|error| format!("Failed to resolve shared skill: {error}"))?
        } else {
            path
        }
    } else if let Some(installation) = selected_installation {
        if installation.link_type == "symlink" {
            PathBuf::from(&installation.installed_path)
                .canonicalize()
                .map_err(|error| {
                    format!(
                        "Failed to resolve skill installation symlink '{}': {error}",
                        installation.installed_path
                    )
                })?
        } else {
            PathBuf::from(&installation.installed_path)
        }
    } else {
        detail
            .canonical_path
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&detail.dir_path))
    };
    // 플랫폼을 지정하지 않은 보관함/검색 상세도 바로가기의 실제 원본을 사용한다.
    let target_path = if detail.can_manage_origin {
        target_path.canonicalize().map_err(|error| format!("Failed to resolve skill target: {error}"))?
    } else {
        target_path
    };
    let target_key = origin_target_key(&target_path);
    Ok(ResolvedSkillTarget {
        skill_id: request.skill_id.clone(),
        agent_id: request.agent_id.clone(),
        row_id: request.row_id.clone(),
        target_key,
        target_path,
        is_read_only: !detail.can_manage_origin,
    })
}

async fn load_origin(pool: &DbPool, target_key: &str) -> Result<Option<SkillOriginRow>, String> {
    load_origin_with(pool, target_key).await
}

async fn load_origin_with<'e, E>(
    executor: E,
    target_key: &str,
) -> Result<Option<SkillOriginRow>, String>
where
    E: sqlx::Executor<'e, Database = sqlx::Sqlite>,
{
    sqlx::query_as::<_, SkillOriginRow>("SELECT * FROM skill_origins WHERE target_key = ?")
        .bind(target_key)
        .fetch_optional(executor)
        .await
        .map_err(|error| error.to_string())
}

fn origin_info(origin: &SkillOriginRow, can_update: bool) -> SkillOriginInfo {
    SkillOriginInfo {
        binding_id: origin.binding_id.clone(),
        target_key: origin.target_key.clone(),
        target_path: origin.target_path.clone(),
        repository_id: origin.repository_id.clone(),
        owner: origin.owner.clone(),
        repo: origin.repo.clone(),
        source_path: origin.source_path.clone(),
        ref_name: origin.ref_name.clone(),
        baseline_state: origin.baseline_state.clone(),
        base_commit_oid: origin.base_commit_oid.clone(),
        last_applied_commit_oid: origin.last_applied_commit_oid.clone(),
        last_applied_at: origin.last_applied_at.clone(),
        last_checked_at: origin.last_checked_at.clone(),
        last_remote_commit_oid: origin.last_remote_commit_oid.clone(),
        last_error: origin.last_error.clone(),
        binding_version: origin.binding_version,
        can_update,
    }
}

// ─── 가져온 스킬 출처 ─────────────────────────────────────────────────────────

/// 출처 바인딩 키. 실제 물리 대상 경로를 canonicalize한 문자열을 씁니다.
///
/// 가져오기와 조회가 같은 규칙을 쓰지 않으면 폴더 이름만 바꿔 가져온 뒤 방금
/// 기록한 출처를 같은 경로에서 찾지 못합니다.
pub(crate) fn origin_target_key(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// GitHub에서 방금 기록한 스킬과 출처 바인딩을 하나의 트랜잭션으로 저장합니다.
///
/// 저장소 내용은 다시 내려받지 않고 호출자가 실제로 기록한 대상 폴더에서 기준
/// manifest를 만듭니다. 확인한 커밋을 받았을 때만 그 커밋을 검증된 기준선으로
/// 기록하고, 그렇지 않으면 기준선을 `unknown`으로 남깁니다.
///
/// 같은 대상 경로에 이미 바인딩이 있으면 저장소와 기준선을 통째로 교체합니다.
/// 이전 저장소의 커밋 기록이 새 스킬에 남지 않습니다.
pub async fn persist_imported_skill(
    pool: &DbPool,
    skill: &db::Skill,
    repo: &github_import::GitHubRepoRef,
    source_path: &str,
    commit_oid: Option<&str>,
) -> Result<(), String> {
    let target_dir = skill
        .canonical_path
        .as_deref()
        .map(PathBuf::from)
        .or_else(|| Path::new(&skill.file_path).parent().map(Path::to_path_buf))
        .ok_or_else(|| format!("Imported skill '{}' has no target directory", skill.id))?;
    let target_key = origin_target_key(&target_dir);
    let manifest_json_text = manifest_json(&manifest_from_local_directory(&target_dir)?)?;
    // 로컬 설치 ID를 바꿔 가져와도 저장소 안 원본 경로는 그대로 남긴다.
    let source_path = if source_path.trim().is_empty() {
        ".".to_string()
    } else {
        source_path.trim_matches('/').to_string()
    };
    let commit_oid = commit_oid
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let now = Utc::now().to_rfc3339();
    let baseline_state = if commit_oid.is_some() {
        "verified"
    } else {
        "unknown"
    };
    let base_commit_oid = commit_oid.map(str::to_string);
    let base_manifest_json = commit_oid.map(|_| manifest_json_text.clone());
    let last_applied_commit_oid = commit_oid.map(str::to_string);
    let last_applied_at = commit_oid.map(|_| now.clone());
    let last_checked_at = commit_oid.map(|_| now.clone());
    let last_remote_commit_oid = commit_oid.map(str::to_string);
    let last_remote_manifest_json = commit_oid.map(|_| manifest_json_text.clone());

    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
    db::upsert_skill_with(&mut *transaction, skill).await?;
    let binding_id = load_origin_with(&mut *transaction, &target_key)
        .await?
        .map(|origin| origin.binding_id)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    sqlx::query(
        "INSERT INTO skill_origins
         (binding_id, target_key, skill_id, agent_id, row_id, target_path, provider, repository_id,
          owner, repo, source_path, ref_name, baseline_state, base_commit_oid, base_manifest_json,
          last_applied_commit_oid, last_applied_at, last_checked_at,
          last_remote_commit_oid, last_remote_manifest_json, last_error,
          binding_version, created_at, updated_at)
         VALUES (?, ?, ?, NULL, NULL, ?, 'github', NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, 1, ?, ?)
         ON CONFLICT(target_key) DO UPDATE SET
          skill_id=excluded.skill_id, agent_id=NULL, row_id=NULL,
          target_path=excluded.target_path, repository_id=NULL,
          owner=excluded.owner, repo=excluded.repo, source_path=excluded.source_path,
          ref_name=excluded.ref_name, baseline_state=excluded.baseline_state,
          base_commit_oid=excluded.base_commit_oid, base_manifest_json=excluded.base_manifest_json,
          last_applied_commit_oid=excluded.last_applied_commit_oid,
          last_applied_at=excluded.last_applied_at, last_checked_at=excluded.last_checked_at,
          last_remote_commit_oid=excluded.last_remote_commit_oid,
          last_remote_manifest_json=excluded.last_remote_manifest_json,
          last_error=NULL, binding_version=skill_origins.binding_version+1, updated_at=excluded.updated_at",
    )
    .bind(&binding_id)
    .bind(&target_key)
    .bind(&skill.id)
    .bind(target_dir.to_string_lossy().into_owned())
    .bind(&repo.owner)
    .bind(&repo.repo)
    .bind(&source_path)
    .bind(&repo.branch)
    .bind(baseline_state)
    .bind(&base_commit_oid)
    .bind(&base_manifest_json)
    .bind(&last_applied_commit_oid)
    .bind(&last_applied_at)
    .bind(&last_checked_at)
    .bind(&last_remote_commit_oid)
    .bind(&last_remote_manifest_json)
    .bind(&now)
    .bind(&now)
    .execute(&mut *transaction)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM skill_origin_ignores WHERE target_key = ?")
        .bind(&target_key).execute(&mut *transaction).await
        .map_err(|error| error.to_string())?;
    transaction.commit().await.map_err(|error| error.to_string())
}

/// 카드 목록용 출처 요약을 한 번의 조회로 가져옵니다.
///
/// 기준 manifest나 확인 기록은 읽지 않습니다. 실제 대상 경로가 정확히 일치하는
/// 바인딩만 돌려주므로 같은 이름의 다른 사본에는 출처가 붙지 않습니다.
pub(crate) async fn origin_summaries_for_targets(
    pool: &DbPool,
    target_keys: &[String],
) -> Result<HashMap<String, GitHubSkillOriginSummary>, String> {
    let mut summaries = HashMap::new();
    if target_keys.is_empty() {
        return Ok(summaries);
    }
    // 바인딩 변수 한도를 넘지 않게 나눠 조회하고 하나의 맵으로 합칩니다.
    // 보통 크기의 목록은 한 번의 조회로 끝납니다.
    for chunk in target_keys.chunks(MAX_ORIGIN_QUERY_KEYS) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT target_key, owner, repo, source_path, ref_name, base_manifest_json, last_remote_manifest_json
             FROM skill_origins WHERE target_key IN ({placeholders})"
        );
        let mut query = sqlx::query(&sql);
        for target_key in chunk {
            query = query.bind(target_key.as_str());
        }
        for row in query
            .fetch_all(pool)
            .await
            .map_err(|error| error.to_string())?
        {
            summaries.insert(
                row.get::<String, _>("target_key"),
                GitHubSkillOriginSummary {
                    owner: row.get("owner"),
                    repo: row.get("repo"),
                    source_path: row.get("source_path"),
                    ref_name: row.get("ref_name"),
                    update_available: row.get::<Option<String>, _>("base_manifest_json")
                        .zip(row.get::<Option<String>, _>("last_remote_manifest_json"))
                        .is_some_and(|(base, remote)| base != remote),
                },
            );
        }
    }
    Ok(summaries)
}

/// 사라진 물리 대상의 출처 바인딩을 제거합니다.
///
/// 보관함 스킬을 지우면 같은 경로에 다른 스킬이 들어올 수 있습니다. 대상이
/// 없어진 바인딩을 남겨 두면 그 스킬이 이전 저장소 출처를 물려받습니다.
/// 다른 경로의 출처와 컬렉션 기록은 건드리지 않습니다.
pub(crate) async fn discard_origin_bindings(
    pool: &DbPool,
    target_keys: &[String],
) -> Result<(), String> {
    if target_keys.is_empty() {
        return Ok(());
    }
    let placeholders = vec!["?"; target_keys.len()].join(",");
    let sql = format!("DELETE FROM skill_origins WHERE target_key IN ({placeholders})");
    let mut query = sqlx::query(&sql);
    for target_key in target_keys {
        query = query.bind(target_key.as_str());
    }
    query.execute(pool).await.map_err(|error| error.to_string())?;
    let sql = format!("DELETE FROM skill_group_sources WHERE target_path IN ({placeholders})");
    let mut query = sqlx::query(&sql);
    for target_key in target_keys { query = query.bind(target_key.as_str()); }
    query.execute(pool).await.map_err(|error| error.to_string())?;
    let sql = format!("DELETE FROM skill_origin_ignores WHERE target_key IN ({placeholders})");
    let mut query = sqlx::query(&sql);
    for target_key in target_keys { query = query.bind(target_key.as_str()); }
    query.execute(pool).await.map_err(|error| error.to_string())?;
    Ok(())
}

/// 관리 앱이 실제로 복사한 스킬에만 원본 정보를 이어 붙인다.
/// 대상에 이미 다른 연결이 있거나 복사 내용이 달라졌다면 그대로 둔다.
pub(crate) async fn inherit_copied_origin(
    pool: &DbPool,
    source_dir: &Path,
    target_dir: &Path,
    skill_id: &str,
    agent_id: Option<&str>,
) -> Result<(), String> {
    let source_key = origin_target_key(source_dir);
    let Some(source) = load_origin(pool, &source_key).await? else { return Ok(()); };
    let target_key = origin_target_key(target_dir);
    if target_key == source_key || load_origin(pool, &target_key).await?.is_some() { return Ok(()); }
    let ignored: Option<String> = sqlx::query_scalar("SELECT target_key FROM skill_origin_ignores WHERE target_key = ?")
        .bind(&target_key).fetch_optional(pool).await.map_err(|error| error.to_string())?;
    if ignored.is_some() { return Ok(()); }
    if manifest_from_local_directory(source_dir)? != manifest_from_local_directory(target_dir)? { return Ok(()); }
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT OR IGNORE INTO skill_origins
         (binding_id, target_key, skill_id, agent_id, row_id, target_path, provider, repository_id,
          owner, repo, source_path, ref_name, baseline_state, base_commit_oid, base_manifest_json,
          last_applied_commit_oid, last_applied_at, last_checked_at, last_remote_commit_oid,
          last_remote_manifest_json, last_error, binding_version, created_at, updated_at)
         VALUES (?, ?, ?, ?, NULL, ?, 'github', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, 1, ?, ?)"
    )
    .bind(Uuid::new_v4().to_string()).bind(&target_key).bind(skill_id).bind(agent_id)
    .bind(target_dir.to_string_lossy().into_owned()).bind(&source.repository_id)
    .bind(&source.owner).bind(&source.repo).bind(&source.source_path).bind(&source.ref_name)
    .bind(&source.baseline_state).bind(&source.base_commit_oid).bind(&source.base_manifest_json)
    .bind(&source.last_applied_commit_oid).bind(&source.last_applied_at).bind(&source.last_checked_at)
    .bind(&source.last_remote_commit_oid).bind(&source.last_remote_manifest_json)
    .bind(&now).bind(&now)
    .execute(pool).await.map_err(|error| error.to_string())?;
    Ok(())
}

async fn fetch_remote_snapshot(
    pool: &DbPool,
    repo_url: &str,
    source_path: &str,
    requested_ref: Option<&str>,
) -> Result<RemoteSnapshot, String> {
    let auth = github_import::github_direct_auth_from_settings(pool).await?;
    let mut repo = github_import::resolve_repo_ref(repo_url, auth.as_deref()).await?;
    let ref_name = requested_ref
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&repo.branch)
        .to_string();
    let client = reqwest::Client::builder()
        .user_agent("skills-manage/0.10.0")
        .build()
        .map_err(|error| error.to_string())?;

    let repo_api = format!("https://api.github.com/repos/{}/{}", repo.owner, repo.repo);
    let repo_response = github_import::send_with_auth_fallback(&client, &repo_api, auth.as_deref())
        .await
        .map_err(|error| error.to_string())?;
    if !repo_response.status().is_success() {
        return Err(format!(
            "GitHub repository lookup returned {}",
            repo_response.status()
        ));
    }
    let repo_value: serde_json::Value = repo_response
        .json()
        .await
        .map_err(|error| error.to_string())?;
    let repository_id = repo_value
        .get("id")
        .and_then(|value| value.as_u64())
        .map(|id| id.to_string());

    let mut commit_api = reqwest::Url::parse(&format!(
        "https://api.github.com/repos/{}/{}/commits",
        repo.owner, repo.repo
    ))
    .map_err(|error| error.to_string())?;
    commit_api
        .path_segments_mut()
        .map_err(|_| "Invalid GitHub commit URL")?
        .push(&ref_name);
    let commit_response =
        github_import::send_with_auth_fallback(&client, commit_api.as_str(), auth.as_deref())
            .await
            .map_err(|error| error.to_string())?;
    if !commit_response.status().is_success() {
        return Err(format!(
            "GitHub ref lookup returned {}",
            commit_response.status()
        ));
    }
    let commit_value: serde_json::Value = commit_response
        .json()
        .await
        .map_err(|error| error.to_string())?;
    let commit_oid = commit_value
        .get("sha")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "GitHub did not return a commit SHA".to_string())?
        .to_string();

    // 이후 비교와 적용은 사용자가 확인한 불변 커밋을 사용한다.
    repo.branch = commit_oid.clone();
    let snapshot = github_import::download_repo_snapshot(&client, &repo, auth.as_deref()).await?;
    if let Ok(skills) = github_import::build_repo_skill_candidates_from_snapshot(&repo, &snapshot) {
        sqlx::query("INSERT INTO skill_repository_catalog (owner, repo, ref_name, skill_count) VALUES (?, ?, ?, ?) ON CONFLICT(owner, repo, ref_name) DO UPDATE SET skill_count = excluded.skill_count")
            .bind(repo.owner.to_lowercase()).bind(repo.repo.to_lowercase()).bind(&ref_name)
            .bind(skills.len() as i64).execute(pool).await.map_err(|error| error.to_string())?;
    }
    let files = remote_files(&snapshot, source_path)?;
    let manifest = manifest_from_remote_files(&files);
    Ok(RemoteSnapshot {
        repository_id,
        owner: repo.owner,
        repo: repo.repo,
        ref_name,
        commit_oid,
        source_path: if source_path.trim().is_empty() {
            ".".into()
        } else {
            source_path.trim_matches('/').into()
        },
        snapshot: Arc::new(snapshot),
        manifest,
    })
}

async fn cached_remote_snapshot(
    pool: &DbPool,
    repo_url: &str,
    source_path: &str,
    ref_name: Option<&str>,
    cache: &mut RemoteCache,
) -> Result<RemoteSnapshot, String> {
    let key = (repo_url.to_string(), ref_name.map(str::to_string));
    if let Some(cached) = cache.get(&key) {
        let mut remote = cached.clone();
        remote.manifest = manifest_from_remote_files(&remote_files(&remote.snapshot, source_path)?);
        remote.source_path = source_path.to_string();
        return Ok(remote);
    }
    let remote = fetch_remote_snapshot(pool, repo_url, source_path, ref_name).await?;
    cache.insert(key, remote.clone());
    Ok(remote)
}

fn classify_state(
    base: Option<&SkillManifest>,
    local: &SkillManifest,
    remote: &SkillManifest,
) -> OriginSyncState {
    match base {
        None => {
            if local == remote {
                OriginSyncState::LocalMatchesRemote
            } else {
                OriginSyncState::UnknownBaseline
            }
        }
        Some(base) if local == base && remote == base => OriginSyncState::UpToDate,
        Some(base) if local == base && remote != base => OriginSyncState::RemoteUpdate,
        Some(base) if remote == base && local != base => OriginSyncState::LocalChanges,
        Some(_) if local == remote => OriginSyncState::LocalMatchesRemote,
        Some(_) => OriginSyncState::Diverged,
    }
}

async fn check_origin_impl(
    pool: &DbPool,
    target: &ResolvedSkillTarget,
) -> Result<SkillOriginStatus, String> {
    check_origin_with_cache(pool, target, &mut RemoteCache::new()).await
}

async fn check_origin_with_cache(
    pool: &DbPool,
    target: &ResolvedSkillTarget,
    cache: &mut RemoteCache,
) -> Result<SkillOriginStatus, String> {
    let origin = load_origin(pool, &target.target_key)
        .await?
        .ok_or_else(|| "This skill is not linked to a GitHub origin".to_string())?;
    let repo_url = format!("https://github.com/{}/{}", origin.owner, origin.repo);
    let remote = match cached_remote_snapshot(
        pool,
        &repo_url,
        &origin.source_path,
        Some(&origin.ref_name),
        cache,
    )
    .await
    {
        Ok(remote) => remote,
        Err(error) => {
            let now = Utc::now().to_rfc3339();
            sqlx::query(
                    "UPDATE skill_origins SET last_error = ?, last_checked_at = ?, updated_at = ? WHERE binding_id = ?",
                )
                .bind(&error)
                .bind(&now)
                .bind(&now)
                .bind(&origin.binding_id)
                .execute(pool)
                .await
                .map_err(|db_error| db_error.to_string())?;
            return Err(error);
        }
    };
    status_from_remote(pool, target, &origin, &remote).await
}

async fn status_from_remote(
    pool: &DbPool,
    target: &ResolvedSkillTarget,
    origin: &SkillOriginRow,
    remote: &RemoteSnapshot,
) -> Result<SkillOriginStatus, String> {
    let local = manifest_from_local_directory(&target.target_path)?;
    if let (Some(expected), Some(actual)) = (
        origin.repository_id.as_deref(),
        remote.repository_id.as_deref(),
    ) {
        if expected != actual {
            return Err(
                "The GitHub repository identity changed; relink the origin before updating"
                    .to_string(),
            );
        }
    }
    let base = parse_manifest(origin.base_manifest_json.as_deref());
    let state = classify_state(base.as_ref(), &local, &remote.manifest);
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE skill_origins SET repository_id = COALESCE(repository_id, ?), last_checked_at = ?,
         last_remote_commit_oid = ?, last_remote_manifest_json = ?, last_error = NULL, updated_at = ?
         WHERE binding_id = ?",
    )
    .bind(&remote.repository_id)
    .bind(&now)
    .bind(&remote.commit_oid)
    .bind(manifest_json(&remote.manifest)?)
    .bind(&now)
    .bind(&origin.binding_id)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    let refreshed = load_origin(pool, &target.target_key)
        .await?
        .ok_or_else(|| "The GitHub origin was removed while checking".to_string())?;
    let empty = SkillManifest::default();
    Ok(SkillOriginStatus {
        origin: origin_info(&refreshed, !target.is_read_only),
        state,
        local_vs_remote: summarize_changes(&local, &remote.manifest),
        local_vs_base: summarize_changes(base.as_ref().unwrap_or(&empty), &local),
        remote_vs_base: summarize_changes(base.as_ref().unwrap_or(&empty), &remote.manifest),
        remote_commit_oid: remote.commit_oid.clone(),
    })
}

#[tauri::command]
pub async fn get_skill_origin(
    state: State<'_, AppState>,
    target: SkillTargetRequest,
) -> Result<Option<SkillOriginInfo>, String> {
    let target = resolve_target(&state.db, &target).await?;
    Ok(load_origin(&state.db, &target.target_key)
        .await?
        .map(|origin| origin_info(&origin, !target.is_read_only)))
}

fn repo_url_from_record(source: &str) -> Option<String> {
    let path = source.strip_prefix("github:").or_else(|| source.strip_prefix("skills.sh:"))?;
    let mut parts = path.split('/');
    let (Some(owner), Some(repo), None) = (parts.next(), parts.next(), parts.next()) else { return None; };
    if owner.is_empty() || repo.is_empty() || repo == "." || repo == ".." || !owner.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || !repo.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.') {
        return None;
    }
    Some(format!("https://github.com/{owner}/{repo}"))
}

/// 다른 설치의 기록은 같은 이름만으로 빌리지 않고 전체 파일을 확인한다.
async fn recorded_origins_for_target(
    pool: &DbPool,
    target: &ResolvedSkillTarget,
    skill: Option<&db::Skill>,
) -> Result<Vec<RecordedOrigin>, String> {
    if let Some(record) = installation_records::read_for_target(&target.target_path)? {
        return Ok(vec![record]);
    }
    let mut paths = db::get_skill_installations(pool, &target.skill_id)
        .await?
        .into_iter()
        .map(|installation| PathBuf::from(installation.installed_path))
        .collect::<BTreeSet<_>>();
    if let Some(skill) = skill {
        if let Some(path) = &skill.canonical_path {
            paths.insert(PathBuf::from(path));
        }
        if let Some(path) = Path::new(&skill.file_path).parent() {
            paths.insert(path.to_path_buf());
        }
    }
    let mut local = None;
    let mut records = BTreeSet::new();
    for path in paths {
        let Ok(physical_path) = path.canonicalize() else {
            continue;
        };
        // 바로가기의 설치 기록은 링크가 놓인 경로에 있다. 실제 경로로
        // 바꾸기 전에 읽어야 보관함으로 옮긴 원본도 출처를 찾을 수 있다.
        let record = match installation_records::read_for_target(&path)? {
            Some(record) => Some(record),
            None if origin_target_key(&physical_path) == target.target_key => None,
            None => load_origin(pool, &origin_target_key(&physical_path))
                .await?
                .map(|origin| RecordedOrigin {
                    repo_url: format!("https://github.com/{}/{}", origin.owner, origin.repo),
                    source_path: Some(origin.source_path),
                    ref_name: Some(origin.ref_name),
                }),
        };
        let Some(record) = record else {
            continue;
        };
        let Ok(source_manifest) = manifest_from_local_directory(&physical_path) else {
            continue;
        };
        if local.is_none() {
            local = Some(manifest_from_local_directory(&target.target_path)?);
        }
        if local.as_ref() == Some(&source_manifest) {
            records.insert(record);
        }
    }
    // 예전 앱의 출처 문자열도 해당 물리 경로에만 적용한다.
    if records.is_empty() {
        if let Some(skill) = skill {
            let same_path = Path::new(&skill.file_path)
                .parent()
                .is_some_and(|path| origin_target_key(path) == target.target_key);
            if same_path {
                if let Some(repo_url) = skill.source.as_deref().and_then(repo_url_from_record) {
                    records.insert(RecordedOrigin {
                        repo_url,
                        source_path: None,
                        ref_name: None,
                    });
                }
            }
        }
    }
    Ok(records.into_iter().collect())
}

/// 공개 저장소 검색은 후보 제안에만 사용한다. 검색 결과만으로 출처를 확정하지 않는다.
async fn search_public_origin_candidates(
    pool: &DbPool,
    skill_id: &str,
    skill_name: &str,
    description: Option<&str>,
    local_skill_md: &Path,
) -> Vec<SkillOriginCandidate> {
    let auth = github_import::github_direct_auth_from_settings(pool).await.ok().flatten();
    let Ok(client) = reqwest::Client::builder()
        .user_agent("skills-manage/0.12.1")
        .timeout(std::time::Duration::from_secs(12))
        .build() else { return Vec::new(); };
    let Ok(mut url) = reqwest::Url::parse("https://api.github.com/search/repositories") else { return Vec::new(); };
    url.query_pairs_mut()
        .append_pair("q", &format!("{skill_id} in:name,description"))
        .append_pair("per_page", "5");
    let Ok(response) = github_import::send_with_auth_fallback(&client, url.as_str(), auth.as_deref()).await else {
        return Vec::new();
    };
    if !response.status().is_success() { return Vec::new(); }
    let Ok(payload) = response.json::<serde_json::Value>().await else { return Vec::new(); };
    let local_content = fs::read(local_skill_md).ok();
    let mut suggestions = Vec::new();
    for item in payload.get("items").and_then(|items| items.as_array()).into_iter().flatten().take(3) {
        let (Some(full_name), Some(branch)) = (
            item.get("full_name").and_then(|value| value.as_str()),
            item.get("default_branch").and_then(|value| value.as_str()),
        ) else { continue; };
        let Some(repo_url) = repo_url_from_record(&format!("github:{full_name}")) else { continue; };
        let Some((owner, repo_name)) = full_name.split_once('/') else { continue; };
        let repo = github_import::GitHubRepoRef {
            owner: owner.to_string(), repo: repo_name.to_string(), branch: branch.to_string(),
            normalized_url: repo_url.clone(),
        };
        let Ok(snapshot) = github_import::download_repo_snapshot(&client, &repo, auth.as_deref()).await else { continue; };
        let Ok(skills) = github_import::build_repo_skill_candidates_from_snapshot(&repo, &snapshot) else { continue; };
        for skill in skills {
            if skill.skill_id.eq_ignore_ascii_case(skill_id) || skill.skill_name.eq_ignore_ascii_case(skill_name) {
                let manifest_path = if skill.source_path == "." {
                    "SKILL.md".to_string()
                } else {
                    format!("{}/SKILL.md", skill.source_path)
                };
                let reason = if local_content.as_ref()
                    .zip(snapshot.files.get(&manifest_path))
                    .is_some_and(|(local, remote)| local == remote) {
                    "content_match"
                } else if description.zip(skill.description.as_deref())
                    .is_some_and(|(left, right)| left.trim().eq_ignore_ascii_case(right.trim())) {
                    "name_description"
                } else {
                    "public_search"
                };
                suggestions.push(SkillOriginCandidate {
                    repo_url: repo_url.clone(), source_path: skill.source_path,
                    ref_name: branch.to_string(), reason: reason.to_string(),
                });
            }
        }
    }
    suggestions
}

/// 기존 설치에 저장된 출처와 마켓플레이스 목록에서 후보를 찾는다.
/// 설치 기록의 저장소 안에서 원본 경로가 하나로 정해지면 자동 연결한다.
/// 파일이 달라진 경우에는 기준선을 알 수 없는 상태로 두어 사용자 확인 없이 덮지 않는다.
#[tauri::command]
pub async fn discover_skill_origin(
    state: State<'_, AppState>,
    target: SkillTargetRequest,
) -> Result<SkillOriginDiscovery, String> {
    discover_skill_origin_impl(&state.db, &target, true, &mut RemoteCache::new()).await
}

async fn discover_skill_origin_impl(
    pool: &DbPool,
    request: &SkillTargetRequest,
    search_public: bool,
    cache: &mut RemoteCache,
) -> Result<SkillOriginDiscovery, String> {
    let target = resolve_target(pool, request).await?;
    if target.is_read_only { return Ok(SkillOriginDiscovery { origin: None, candidates: vec![] }); }
    if let Some(origin) = load_origin(pool, &target.target_key).await? {
        return Ok(SkillOriginDiscovery { origin: Some(origin_info(&origin, true)), candidates: vec![] });
    }
    let ignored: Option<String> = sqlx::query_scalar("SELECT target_key FROM skill_origin_ignores WHERE target_key = ?")
        .bind(&target.target_key).fetch_optional(pool).await.map_err(|error| error.to_string())?;
    if ignored.is_some() { return Ok(SkillOriginDiscovery { origin: None, candidates: vec![] }); }
    let skill = db::get_skill_by_id(pool, &target.skill_id).await?;
    let mut candidates = Vec::new();
    let mut source_candidates = Vec::new();

    let records = recorded_origins_for_target(pool, &target, skill.as_ref()).await?;
    for record in &records {
        if let Some(source_path) = &record.source_path {
            source_candidates.push(SkillOriginCandidate {
                repo_url: record.repo_url.clone(),
                source_path: source_path.clone(),
                ref_name: record.ref_name.clone().unwrap_or_default(),
                reason: "installation_record".into(),
            });
        } else {
            let auth = github_import::github_direct_auth_from_settings(pool).await?;
            let mut repo =
                github_import::resolve_repo_ref(&record.repo_url, auth.as_deref()).await?;
            if let Some(ref_name) = &record.ref_name {
                repo.branch = ref_name.clone();
            }
            for remote_skill in
                github_import::fetch_repo_skill_candidates(&repo, auth.as_deref()).await?
            {
                if remote_skill.skill_id.eq_ignore_ascii_case(&target.skill_id)
                    || skill.as_ref().is_some_and(|skill| {
                        remote_skill.skill_name.eq_ignore_ascii_case(&skill.name)
                    })
                {
                    source_candidates.push(SkillOriginCandidate {
                        repo_url: record.repo_url.clone(),
                        source_path: remote_skill.source_path,
                        ref_name: repo.branch.clone(),
                        reason: "installation_record".into(),
                    });
                }
            }
        }
    }
    // 출처가 하나로 확인될 때만 자동 연결한다. 설치본과 최신본이 다르면
    // 설치 버전은 unknown으로 남기며, 실제 파일은 바꾸지 않는다.
    if records.len() == 1 && source_candidates.len() == 1 {
        let candidate = &source_candidates[0];
        let remote = cached_remote_snapshot(
            pool,
            &candidate.repo_url,
            &candidate.source_path,
            (!candidate.ref_name.is_empty()).then_some(candidate.ref_name.as_str()),
            cache,
        )
        .await?;
        let status = link_downloaded_origin(pool, &target, &remote, false).await?;
        return Ok(SkillOriginDiscovery {
            origin: Some(status.origin),
            candidates: vec![],
        });
    }
    candidates.extend(source_candidates);

    if let Some(skill) = &skill {
        let rows = sqlx::query(
            "SELECT download_url, description FROM marketplace_skills WHERE lower(name)=lower(?) OR lower(name)=lower(?) LIMIT 30"
        ).bind(&skill.name).bind(&skill.id).fetch_all(pool).await.map_err(|error| error.to_string())?;
        for row in rows {
            let url: String = row.get("download_url");
            if let Some((repo, source_path)) = marketplace::github_origin_from_raw_url(&url) {
                let catalog_description: Option<String> = row.get("description");
                let same_description = skill.description.as_deref()
                    .zip(catalog_description.as_deref())
                    .is_some_and(|(left, right)| left.trim().eq_ignore_ascii_case(right.trim()));
                candidates.push(SkillOriginCandidate {
                    repo_url: repo.normalized_url, source_path, ref_name: repo.branch,
                    reason: if same_description { "name_description" } else { "catalog_name" }.to_string(),
                });
            }
        }
    }
    if search_public && candidates.is_empty() {
        if let Some(skill) = &skill {
            candidates.extend(search_public_origin_candidates(
                pool, &skill.id, &skill.name, skill.description.as_deref(),
                &target.target_path.join("SKILL.md"),
            ).await);
        }
    }
    let mut seen = BTreeSet::new();
    candidates.retain(|candidate| seen.insert((candidate.repo_url.clone(), candidate.source_path.clone(), candidate.ref_name.clone())));
    candidates.sort_by_key(|candidate| match candidate.reason.as_str() {
        "content_match" => 0,
        "installation_record" => 1,
        "name_description" => 2,
        "catalog_name" => 3,
        _ => 4,
    });
    candidates.truncate(10);
    Ok(SkillOriginDiscovery { origin: None, candidates })
}

#[tauri::command]
pub async fn link_skill_origin(
    state: State<'_, AppState>,
    request: LinkSkillOriginRequest,
) -> Result<SkillOriginStatus, String> {
    let target = resolve_target(&state.db, &request.target).await?;
    link_origin_impl(&state.db, &target, &request.repo_url, &request.source_path, request.ref_name.as_deref()).await
}

async fn link_origin_impl(
    pool: &DbPool,
    target: &ResolvedSkillTarget,
    repo_url: &str,
    source_path: &str,
    ref_name: Option<&str>,
) -> Result<SkillOriginStatus, String> {
    if target.is_read_only {
        return Err("Read-only observed skills cannot be linked for in-place updates".to_string());
    }
    let remote = fetch_remote_snapshot(
        pool,
        repo_url,
        source_path,
        ref_name,
    )
    .await?;
    link_downloaded_origin(pool, target, &remote, true).await
}

async fn link_downloaded_origin(
    pool: &DbPool,
    target: &ResolvedSkillTarget,
    remote: &RemoteSnapshot,
    explicit: bool,
) -> Result<SkillOriginStatus, String> {
    let _guard = mutation_lock().await;
    if !explicit {
        if let Some(origin) = load_origin(pool, &target.target_key).await? {
            return Err(format!(
                "Origin changed during discovery: {}",
                origin.binding_id
            ));
        }
        let ignored: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM skill_origin_ignores WHERE target_key = ?)",
        )
        .bind(&target.target_key)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?;
        if ignored {
            return Err("Origin was unlinked during discovery".into());
        }
    }
    let local = manifest_from_local_directory(&target.target_path)?;
    let (baseline_state, base_commit_oid, base_manifest_json) = if local == remote.manifest {
        (
            "verified".to_string(),
            Some(remote.commit_oid.clone()),
            Some(manifest_json(&remote.manifest)?),
        )
    } else {
        ("unknown".to_string(), None, None)
    };
    let now = Utc::now().to_rfc3339();
    let binding_id = load_origin(pool, &target.target_key)
        .await?
        .map(|origin| origin.binding_id)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    sqlx::query(
        "INSERT INTO skill_origins
         (binding_id, target_key, skill_id, agent_id, row_id, target_path, provider, repository_id,
          owner, repo, source_path, ref_name, baseline_state, base_commit_oid, base_manifest_json,
          last_checked_at, last_remote_commit_oid, last_remote_manifest_json, last_error,
          binding_version, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, 'github', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, 1, ?, ?)
         ON CONFLICT(target_key) DO UPDATE SET
          skill_id=excluded.skill_id, agent_id=excluded.agent_id, row_id=excluded.row_id,
          target_path=excluded.target_path, repository_id=excluded.repository_id,
          owner=excluded.owner, repo=excluded.repo, source_path=excluded.source_path,
          ref_name=excluded.ref_name, baseline_state=excluded.baseline_state,
          base_commit_oid=excluded.base_commit_oid, base_manifest_json=excluded.base_manifest_json,
          last_checked_at=excluded.last_checked_at,
          last_remote_commit_oid=excluded.last_remote_commit_oid,
          last_remote_manifest_json=excluded.last_remote_manifest_json,
          last_error=NULL, binding_version=skill_origins.binding_version+1, updated_at=excluded.updated_at",
    )
    .bind(&binding_id)
    .bind(&target.target_key)
    .bind(&target.skill_id)
    .bind(&target.agent_id)
    .bind(&target.row_id)
    .bind(target.target_path.to_string_lossy().into_owned())
    .bind(&remote.repository_id)
    .bind(&remote.owner)
    .bind(&remote.repo)
    .bind(&remote.source_path)
    .bind(&remote.ref_name)
    .bind(&baseline_state)
    .bind(&base_commit_oid)
    .bind(&base_manifest_json)
    .bind(&now)
    .bind(&remote.commit_oid)
    .bind(manifest_json(&remote.manifest)?)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await
    .map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM skill_origin_ignores WHERE target_key = ?")
        .bind(&target.target_key)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    let origin = load_origin(pool, &target.target_key)
        .await?
        .ok_or_else(|| "Origin disappeared while linking".to_string())?;
    status_from_remote(pool, target, &origin, remote).await
}

#[tauri::command]
pub async fn unlink_skill_origin(
    state: State<'_, AppState>,
    target: SkillTargetRequest,
) -> Result<(), String> {
    let target = resolve_target(&state.db, &target).await?;
    let _guard = mutation_lock().await;
    let mut transaction = state.db.begin().await.map_err(|error| error.to_string())?;
    sqlx::query("DELETE FROM skill_origins WHERE target_key = ?")
        .bind(&target.target_key)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
    sqlx::query("INSERT OR REPLACE INTO skill_origin_ignores (target_key, created_at) VALUES (?, ?)")
        .bind(&target.target_key).bind(Utc::now().to_rfc3339())
        .execute(&mut *transaction).await.map_err(|error| error.to_string())?;
    transaction.commit().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn check_skill_origin(
    state: State<'_, AppState>,
    target: SkillTargetRequest,
) -> Result<SkillOriginStatus, String> {
    let target = resolve_target(&state.db, &target).await?;
    check_origin_impl(&state.db, &target).await
}

/// 스캐너의 source 값(copy/symlink)과 무관하게 보관함과 모든 관리 설치를
/// 살핀다. 물리 경로가 같은 바로가기는 한 번만 처리하고 공개 검색은 하지 않는다.
async fn discover_installed_origins(pool: &DbPool, cache: &mut RemoteCache) -> Result<(), String> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as(
        "SELECT id, NULL FROM skills UNION ALL SELECT skill_id, agent_id FROM skill_installations ORDER BY 1, 2"
    ).fetch_all(pool).await.map_err(|error| error.to_string())?;
    let mut seen = BTreeSet::new();
    for (skill_id, agent_id) in rows {
        let request = SkillTargetRequest {
            skill_id,
            agent_id,
            row_id: None,
        };
        let Ok(target) = resolve_target(pool, &request).await else {
            continue;
        };
        if target.is_read_only || !seen.insert(target.target_key) {
            continue;
        }
        // 한 설치의 삭제·손상·원격 실패가 다른 설치의 연결을 막지 않는다.
        let _ = discover_skill_origin_impl(pool, &request, false, cache).await;
    }
    Ok(())
}

/// 앱을 사용하는 동안 오래된 원본 연결을 다시 확인한다. 개별 실패는 그 연결에
/// 기록하고 나머지 스킬의 확인은 계속한다.
#[tauri::command]
pub async fn check_linked_skill_origins(state: State<'_, AppState>) -> Result<usize, String> {
    let mut cache = RemoteCache::new();
    discover_installed_origins(&state.db, &mut cache).await?;
    let cutoff = (Utc::now() - chrono::Duration::hours(6)).to_rfc3339();
    let origins = sqlx::query_as::<_, SkillOriginRow>(
        "SELECT * FROM skill_origins WHERE last_checked_at IS NULL OR last_checked_at < ? OR NOT EXISTS (SELECT 1 FROM skill_repository_catalog c WHERE c.owner = lower(skill_origins.owner) AND c.repo = lower(skill_origins.repo) AND c.ref_name = skill_origins.ref_name) ORDER BY last_checked_at LIMIT 100"
    ).bind(cutoff).fetch_all(&state.db).await.map_err(|error| error.to_string())?;
    let mut checked = 0;
    for origin in origins {
        let target_path = PathBuf::from(&origin.target_path);
        if !target_path.join("SKILL.md").is_file() {
            sqlx::query("UPDATE skill_origins SET last_error = 'Local SKILL.md not found', last_checked_at = ? WHERE binding_id = ?")
                .bind(Utc::now().to_rfc3339()).bind(&origin.binding_id)
                .execute(&state.db).await.map_err(|error| error.to_string())?;
            continue;
        }
        let binding_id = origin.binding_id;
        let target = ResolvedSkillTarget {
            skill_id: origin.skill_id, agent_id: origin.agent_id, row_id: None,
            target_path, target_key: origin.target_key, is_read_only: false,
        };
        match check_origin_with_cache(&state.db, &target, &mut cache).await {
            Ok(_) => checked += 1,
            Err(error) => {
                sqlx::query("UPDATE skill_origins SET last_error = ?, last_checked_at = ? WHERE binding_id = ?")
                    .bind(error).bind(Utc::now().to_rfc3339()).bind(binding_id)
                    .execute(&state.db).await.map_err(|error| error.to_string())?;
            }
        }
    }
    Ok(checked)
}

#[tauri::command]
pub async fn prepare_skill_update(
    state: State<'_, AppState>,
    request: PrepareSkillUpdateRequest,
) -> Result<SkillUpdatePlan, String> {
    let target = resolve_target(&state.db, &request.target).await?;
    if target.is_read_only {
        return Err("Read-only observed skills cannot be updated in place".to_string());
    }
    let status = check_origin_impl(&state.db, &target).await?;
    let requires_local_change_confirmation = matches!(
        status.state,
        OriginSyncState::LocalChanges
            | OriginSyncState::Diverged
            | OriginSyncState::UnknownBaseline
    );
    if requires_local_change_confirmation && !request.allow_local_changes {
        return Err("LOCAL_CHANGES_REQUIRE_CONFIRMATION".to_string());
    }
    if matches!(
        status.state,
        OriginSyncState::UpToDate | OriginSyncState::LocalMatchesRemote
    ) {
        return Err("Skill already matches the selected GitHub origin".to_string());
    }
    let local = manifest_from_local_directory(&target.target_path)?;
    let operation_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO skill_update_operations
         (operation_id, binding_id, state, expected_target_key, expected_local_manifest_json,
          remote_commit_oid, remote_manifest_json, created_at, updated_at)
         VALUES (?, ?, 'prepared', ?, ?, ?, ?, ?, ?)",
    )
    .bind(&operation_id)
    .bind(&status.origin.binding_id)
    .bind(&target.target_key)
    .bind(manifest_json(&local)?)
    .bind(&status.remote_commit_oid)
    .bind(
        load_origin(&state.db, &target.target_key)
            .await?
            .and_then(|origin| origin.last_remote_manifest_json)
            .ok_or_else(|| "Remote manifest was not cached".to_string())?,
    )
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;
    Ok(SkillUpdatePlan {
        operation_id,
        binding_id: status.origin.binding_id,
        target_path: target.target_path.to_string_lossy().into_owned(),
        remote_commit_oid: status.remote_commit_oid,
        state: status.state,
        changes: status.local_vs_remote,
        requires_local_change_confirmation,
    })
}

fn write_stage(files: &BTreeMap<String, Vec<u8>>, stage: &Path) -> Result<(), String> {
    fs::create_dir(stage).map_err(|error| format!("Failed to create update stage: {error}"))?;
    for (relative, bytes) in files {
        let destination = stage.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Failed to create staged directory: {error}"))?;
        }
        fs::write(&destination, bytes).map_err(|error| {
            format!(
                "Failed to write staged file '{}': {error}",
                destination.display()
            )
        })?;
    }
    scanner::parse_skill_md(&stage.join("SKILL.md"))
        .ok_or_else(|| "Updated SKILL.md has invalid frontmatter".to_string())?;
    Ok(())
}

fn remove_tree(path: &Path) {
    let _ = recovery::remove_path_without_following_links(path);
}

async fn backup_update_target(
    pool: &DbPool,
    origin: &SkillOriginRow,
    target: &Path,
) -> Result<Option<recovery::RecoveryEntry>, String> {
    let central_root = db::get_central_skills_dir(pool).await?;
    let target_canonical = target.canonicalize().map_err(|error| error.to_string())?;
    if central_root
        .canonicalize()
        .is_ok_and(|root| target_canonical != root && target_canonical.starts_with(root))
    {
        return recovery::backup_vault_before_removal(
            pool,
            target,
            format!("GitHub 업데이트 전 백업: {}", origin.skill_id),
        )
        .await;
    }
    let installation = recovery::managed_copy_for_target(pool, &origin.skill_id, target)
        .await?
        .ok_or_else(|| "업데이트할 실제 원본의 관리 설치 기록을 찾을 수 없습니다".to_string())?;
    recovery::backup_copy_installation(pool, &installation).await
}

#[tauri::command]
pub async fn apply_skill_update(
    state: State<'_, AppState>,
    operation_id: String,
) -> Result<SkillUpdateResult, String> {
    let row = sqlx::query(
        "SELECT operation_id, binding_id, state, expected_target_key, expected_local_manifest_json,
                remote_commit_oid, remote_manifest_json, stage_path, quarantine_path, recovery_entry_id
         FROM skill_update_operations WHERE operation_id = ?",
    )
    .bind(&operation_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "Update operation not found".to_string())?;
    let state_value: String = row.get("state");
    if state_value == "applied" {
        return Ok(SkillUpdateResult {
            operation_id,
            binding_id: row.get("binding_id"),
            applied_commit_oid: row.get("remote_commit_oid"),
            recovery_entry_id: row.get("recovery_entry_id"),
        });
    }
    if state_value != "prepared" {
        return Err(format!("Update operation is not applicable: {state_value}"));
    }
    let binding_id: String = row.get("binding_id");
    let origin =
        sqlx::query_as::<_, SkillOriginRow>("SELECT * FROM skill_origins WHERE binding_id = ?")
            .bind(&binding_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "GitHub origin binding no longer exists".to_string())?;
    let target = PathBuf::from(&origin.target_path);
    let target_key: String = row.get("expected_target_key");
    let current_key = target
        .canonicalize()
        .unwrap_or_else(|_| target.clone())
        .to_string_lossy()
        .into_owned();
    if current_key != target_key {
        return Err("PLAN_STALE: skill target changed".to_string());
    }
    let current_manifest = manifest_from_local_directory(&target)?;
    let expected_manifest: SkillManifest =
        serde_json::from_str(&row.get::<String, _>("expected_local_manifest_json"))
            .map_err(|error| error.to_string())?;
    if current_manifest != expected_manifest {
        return Err("PLAN_STALE: local skill changed after preview".to_string());
    }
    let remote_commit_oid: String = row.get("remote_commit_oid");
    let expected_remote_manifest: SkillManifest =
        serde_json::from_str(&row.get::<String, _>("remote_manifest_json"))
            .map_err(|error| error.to_string())?;
    let repo_url = format!("https://github.com/{}/{}", origin.owner, origin.repo);
    let remote = fetch_remote_snapshot(
        &state.db,
        &repo_url,
        &origin.source_path,
        Some(&remote_commit_oid),
    )
    .await?;
    if remote.commit_oid != remote_commit_oid || remote.manifest != expected_remote_manifest {
        return Err("PLAN_STALE: remote snapshot no longer matches the preview".to_string());
    }
    apply_downloaded_update(&state.db, operation_id, origin, expected_manifest, remote).await
}

/// 원격 다운로드가 끝난 뒤 백업, 파일 교체, 기록 갱신을 한 경로에서 수행합니다.
async fn apply_downloaded_update(
    pool: &DbPool,
    operation_id: String,
    origin: SkillOriginRow,
    expected_manifest: SkillManifest,
    remote: RemoteSnapshot,
) -> Result<SkillUpdateResult, String> {
    let target = PathBuf::from(&origin.target_path);
    let binding_id = origin.binding_id.clone();
    let remote_commit_oid = remote.commit_oid;
    let expected_remote_manifest = remote.manifest;
    let files = remote_files(&remote.snapshot, &origin.source_path)?;
    let _mutation_guard = mutation_lock().await;
    // 네트워크 요청 동안은 잠그지 않고, 실제 파일 변경 직전에 로컬 상태를 다시 검증한다.
    let current_manifest = manifest_from_local_directory(&target)?;
    if current_manifest != expected_manifest {
        return Err("PLAN_STALE: local skill changed while fetching the update".to_string());
    }
    let parent = target
        .parent()
        .ok_or_else(|| "Skill target has no parent directory".to_string())?;
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("skill");
    let stage = parent.join(format!(".{name}.skillsmanage-stage-{}", Uuid::new_v4()));
    let quarantine = parent.join(format!(".{name}.skillsmanage-old-{}", Uuid::new_v4()));
    if let Err(error) = write_stage(&files, &stage) {
        remove_tree(&stage);
        return Err(error);
    }
    if manifest_from_local_directory(&stage)? != expected_remote_manifest {
        remove_tree(&stage);
        return Err("Staged update verification failed".to_string());
    }

    let recovery_result = backup_update_target(pool, &origin, &target).await;
    let recovery_entry = match recovery_result {
        Ok(entry) => entry,
        Err(error) => {
            remove_tree(&stage);
            return Err(error);
        }
    };

    let now = Utc::now().to_rfc3339();
    sqlx::query("UPDATE skill_update_operations SET state='applying', stage_path=?, quarantine_path=?, recovery_entry_id=?, updated_at=? WHERE operation_id=?")
        .bind(stage.to_string_lossy().into_owned())
        .bind(quarantine.to_string_lossy().into_owned())
        .bind(recovery_entry.as_ref().map(|entry| entry.id.clone()))
        .bind(&now)
        .bind(&operation_id)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;

    if let Err(error) = fs::rename(&target, &quarantine) {
        remove_tree(&stage);
        let message = format!("Failed to quarantine current skill: {error}");
        let _ = sqlx::query("UPDATE skill_update_operations SET state='failed', error=?, updated_at=? WHERE operation_id=?")
            .bind(&message).bind(Utc::now().to_rfc3339()).bind(&operation_id).execute(pool).await;
        return Err(message);
    }
    if let Err(error) = fs::rename(&stage, &target) {
        let _ = fs::rename(&quarantine, &target);
        remove_tree(&stage);
        let message = format!("Failed to activate updated skill: {error}");
        let _ = sqlx::query("UPDATE skill_update_operations SET state='failed', error=?, updated_at=? WHERE operation_id=?")
            .bind(&message).bind(Utc::now().to_rfc3339()).bind(&operation_id).execute(pool).await;
        return Err(message);
    }

    let db_result = async {
        let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
        let applied_at = Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE skill_origins SET baseline_state='verified', base_commit_oid=?, base_manifest_json=?,
             last_applied_commit_oid=?, last_applied_at=?, last_checked_at=?, last_remote_commit_oid=?,
             last_remote_manifest_json=?, last_error=NULL, updated_at=? WHERE binding_id=?",
        )
        .bind(&remote_commit_oid)
        .bind(manifest_json(&expected_remote_manifest)?)
        .bind(&remote_commit_oid)
        .bind(&applied_at)
        .bind(&applied_at)
        .bind(&remote_commit_oid)
        .bind(manifest_json(&expected_remote_manifest)?)
        .bind(&applied_at)
        .bind(&binding_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;
        sqlx::query("UPDATE skill_update_operations SET state='applied', updated_at=? WHERE operation_id=?")
            .bind(&applied_at)
            .bind(&operation_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| error.to_string())?;
        transaction.commit().await.map_err(|error| error.to_string())
    }
    .await;
    if let Err(error) = db_result {
        remove_tree(&target);
        let rollback = fs::rename(&quarantine, &target);
        let message = match rollback {
            Ok(()) => error,
            Err(rollback_error) => format!("{error}; rollback failed: {rollback_error}"),
        };
        let _ = sqlx::query("UPDATE skill_update_operations SET state='failed', error=?, updated_at=? WHERE operation_id=?")
            .bind(&message).bind(Utc::now().to_rfc3339()).bind(&operation_id).execute(pool).await;
        return Err(message);
    }
    remove_tree(&quarantine);
    Ok(SkillUpdateResult {
        operation_id,
        binding_id,
        applied_commit_oid: remote_commit_oid,
        recovery_entry_id: recovery_entry.map(|entry| entry.id),
    })
}

pub async fn reconcile_incomplete_updates(pool: &DbPool) -> Result<(), String> {
    let rows = sqlx::query(
        "SELECT operation_id, binding_id, state, expected_target_key, remote_commit_oid,
                remote_manifest_json, stage_path, quarantine_path
         FROM skill_update_operations WHERE state = 'applying'",
    )
    .fetch_all(pool)
    .await
    .map_err(|error| error.to_string())?;

    for row in rows {
        let operation_id: String = row.get("operation_id");
        let binding_id: String = row.get("binding_id");
        let target = PathBuf::from(row.get::<String, _>("expected_target_key"));
        let remote_commit_oid: String = row.get("remote_commit_oid");
        let remote_manifest: SkillManifest =
            serde_json::from_str(&row.get::<String, _>("remote_manifest_json"))
                .map_err(|error| error.to_string())?;
        let stage_path = row
            .get::<Option<String>, _>("stage_path")
            .map(PathBuf::from);
        let quarantine_path = row
            .get::<Option<String>, _>("quarantine_path")
            .map(PathBuf::from);

        let target_is_remote = target.exists()
            && manifest_from_local_directory(&target)
                .map(|manifest| manifest == remote_manifest)
                .unwrap_or(false);
        if target_is_remote {
            let now = Utc::now().to_rfc3339();
            let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;
            sqlx::query(
                "UPDATE skill_origins SET baseline_state='verified', base_commit_oid=?, base_manifest_json=?,
                 last_applied_commit_oid=?, last_applied_at=?, last_checked_at=?, last_remote_commit_oid=?,
                 last_remote_manifest_json=?, last_error=NULL, updated_at=? WHERE binding_id=?",
            )
            .bind(&remote_commit_oid)
            .bind(manifest_json(&remote_manifest)?)
            .bind(&remote_commit_oid)
            .bind(&now)
            .bind(&now)
            .bind(&remote_commit_oid)
            .bind(manifest_json(&remote_manifest)?)
            .bind(&now)
            .bind(&binding_id)
            .execute(&mut *transaction)
            .await
            .map_err(|error| error.to_string())?;
            sqlx::query("UPDATE skill_update_operations SET state='applied', updated_at=? WHERE operation_id=?")
                .bind(&now)
                .bind(&operation_id)
                .execute(&mut *transaction)
                .await
                .map_err(|error| error.to_string())?;
            transaction
                .commit()
                .await
                .map_err(|error| error.to_string())?;
            if let Some(path) = quarantine_path.as_deref() {
                remove_tree(path);
            }
            if let Some(path) = stage_path.as_deref() {
                remove_tree(path);
            }
            continue;
        }

        if !target.exists() {
            if let Some(quarantine) = quarantine_path.as_deref().filter(|path| path.exists()) {
                fs::rename(quarantine, &target).map_err(|error| {
                    format!(
                        "Failed to restore interrupted GitHub update '{}': {error}",
                        target.display()
                    )
                })?;
                if let Some(path) = stage_path.as_deref() {
                    remove_tree(path);
                }
                sqlx::query("UPDATE skill_update_operations SET state='failed', error='Interrupted update was rolled back during startup', updated_at=? WHERE operation_id=?")
                    .bind(Utc::now().to_rfc3339())
                    .bind(&operation_id)
                    .execute(pool)
                    .await
                    .map_err(|error| error.to_string())?;
                continue;
            }
        }

        sqlx::query("UPDATE skill_update_operations SET state='recovery_required', error='Interrupted update needs manual recovery', updated_at=? WHERE operation_id=?")
            .bind(Utc::now().to_rfc3339())
            .bind(&operation_id)
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_file(root: &Path, relative: &str, content: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn local_manifest_detects_nested_changes() {
        let temp = TempDir::new().unwrap();
        write_file(temp.path(), "SKILL.md", "---\nname: demo\n---\n");
        write_file(temp.path(), "scripts/run.sh", "echo one\n");
        let before = manifest_from_local_directory(temp.path()).unwrap();
        write_file(temp.path(), "scripts/run.sh", "echo two\n");
        write_file(temp.path(), "references/new.md", "new\n");
        let after = manifest_from_local_directory(temp.path()).unwrap();
        let summary = summarize_changes(&before, &after);
        assert_eq!(summary.modified, 1);
        assert_eq!(summary.added, 1);
        assert_eq!(summary.removed, 0);
    }

    #[test]
    fn three_way_state_distinguishes_remote_local_and_diverged() {
        let base = SkillManifest {
            entries: vec![ManifestEntry {
                path: "SKILL.md".into(),
                size: 1,
                sha256: "a".into(),
            }],
        };
        let local = SkillManifest {
            entries: vec![ManifestEntry {
                path: "SKILL.md".into(),
                size: 1,
                sha256: "b".into(),
            }],
        };
        let remote = SkillManifest {
            entries: vec![ManifestEntry {
                path: "SKILL.md".into(),
                size: 1,
                sha256: "c".into(),
            }],
        };
        assert_eq!(
            classify_state(Some(&base), &base, &remote),
            OriginSyncState::RemoteUpdate
        );
        assert_eq!(
            classify_state(Some(&base), &local, &base),
            OriginSyncState::LocalChanges
        );
        assert_eq!(
            classify_state(Some(&base), &local, &remote),
            OriginSyncState::Diverged
        );
        assert_eq!(
            classify_state(None, &remote, &remote),
            OriginSyncState::LocalMatchesRemote
        );
    }

    // ── 가져온 스킬 출처 ──────────────────────────────────────────────────────

    use crate::commands::github_import::GitHubRepoRef;
    use sqlx::SqlitePool;

    async fn setup_origin_db() -> crate::db::DbPool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        crate::db::init_database(&pool).await.unwrap();
        pool
    }

    fn imported_repo() -> GitHubRepoRef {
        GitHubRepoRef {
            owner: "acme".to_string(),
            repo: "skills".to_string(),
            branch: "main".to_string(),
            normalized_url: "https://github.com/acme/skills".to_string(),
        }
    }

    async fn shared_origin_fixture(temp: &TempDir) -> (DbPool, PathBuf) {
        let pool = db::create_pool(temp.path().join("db.sqlite").to_str().unwrap())
            .await
            .unwrap();
        db::init_database(&pool).await.unwrap();
        let shared = temp.path().join("shared");
        let mut skill = write_imported_skill(&shared, "demo", "demo");
        skill.is_central = false;
        skill.canonical_path = None;
        let target = shared.join("demo").canonicalize().unwrap();
        persist_imported_skill(&pool, &skill, &imported_repo(), "skills/demo", Some("abc"))
            .await
            .unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'universal'")
            .bind(shared.to_string_lossy().as_ref())
            .execute(&pool)
            .await
            .unwrap();
        db::upsert_skill_installation(
            &pool,
            &db::SkillInstallation {
                skill_id: "demo".into(),
                agent_id: "universal".into(),
                installed_path: target.to_string_lossy().into_owned(),
                link_type: "copy".into(),
                symlink_target: None,
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        (pool, target)
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn shared_origin_backup_uses_physical_owner_from_every_platform() {
        let temp = TempDir::new().unwrap();
        let (pool, target) = shared_origin_fixture(&temp).await;
        let mut origin = load_origin(&pool, &origin_target_key(&target))
            .await
            .unwrap()
            .unwrap();
        let before = manifest_from_local_directory(&target).unwrap();
        for agent_id in ["omp", "claude-code", "pi", "universal"] {
            if agent_id != "universal" {
                let root = temp.path().join(agent_id);
                fs::create_dir_all(&root).unwrap();
                let link = root.join("demo");
                std::os::unix::fs::symlink(&target, &link).unwrap();
                sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = ?")
                    .bind(root.to_string_lossy().as_ref())
                    .bind(agent_id)
                    .execute(&pool)
                    .await
                    .unwrap();
                db::upsert_skill_installation(
                    &pool,
                    &db::SkillInstallation {
                        skill_id: "demo".into(),
                        agent_id: agent_id.into(),
                        installed_path: link.to_string_lossy().into_owned(),
                        link_type: "symlink".into(),
                        symlink_target: Some(target.to_string_lossy().into_owned()),
                        created_at: Utc::now().to_rfc3339(),
                    },
                )
                .await
                .unwrap();
            }
            origin.agent_id = Some(agent_id.into());
            let resolved = resolve_target(
                &pool,
                &SkillTargetRequest {
                    skill_id: "demo".into(),
                    agent_id: Some(agent_id.into()),
                    row_id: Some("demo".into()),
                },
            )
            .await
            .unwrap();
            assert_eq!(resolved.target_path, target);
            let entry = backup_update_target(&pool, &origin, &target)
                .await
                .unwrap_or_else(|error| panic!("{agent_id}: {error}"))
                .unwrap();
            assert_eq!(entry.original_path, target.to_string_lossy());
            assert_eq!(
                manifest_from_local_directory(Path::new(&entry.backup_path)).unwrap(),
                before
            );
            // 백업이 실제 원본으로 복원되며 플랫폼의 바로가기가 보존되는지도 검증한다.
            fs::remove_dir_all(&target).unwrap();
            recovery::restore_recovery_entry_impl(&pool, &entry.id)
                .await
                .unwrap();
            assert_eq!(manifest_from_local_directory(&target).unwrap(), before);
            if agent_id != "universal" {
                assert!(
                    fs::symlink_metadata(temp.path().join(agent_id).join("demo"))
                        .unwrap()
                        .file_type()
                        .is_symlink()
                );
            }
        }
        origin.agent_id = None;
        assert!(backup_update_target(&pool, &origin, &target)
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn shared_origin_update_replaces_files_preserves_links_and_keeps_restorable_backup() {
        let temp = TempDir::new().unwrap();
        let (pool, target) = shared_origin_fixture(&temp).await;
        let mut origin = load_origin(&pool, &origin_target_key(&target))
            .await
            .unwrap()
            .unwrap();
        origin.agent_id = Some("omp".into());
        let link = temp.path().join("omp-demo");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let before = manifest_from_local_directory(&target).unwrap();
        let files = BTreeMap::from([
            (
                "SKILL.md".to_string(),
                b"---\nname: demo\ndescription: Updated\n---\nUpdated content\n".to_vec(),
            ),
            ("scripts/new.sh".to_string(), b"echo updated\n".to_vec()),
        ]);
        let remote_manifest = manifest_from_remote_files(&files);
        let operation_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO skill_update_operations (operation_id, binding_id, state, expected_target_key, expected_local_manifest_json, remote_commit_oid, remote_manifest_json, created_at, updated_at) VALUES (?, ?, 'prepared', ?, ?, 'new-commit', ?, 'now', 'now')")
            .bind(&operation_id).bind(&origin.binding_id).bind(&origin.target_key)
            .bind(manifest_json(&before).unwrap()).bind(manifest_json(&remote_manifest).unwrap())
            .execute(&pool).await.unwrap();
        let remote = RemoteSnapshot {
            repository_id: None,
            owner: "acme".into(),
            repo: "skills".into(),
            ref_name: "main".into(),
            commit_oid: "new-commit".into(),
            source_path: "skills/demo".into(),
            snapshot: Arc::new(github_import::GitHubRepoSnapshot {
                files: files
                    .into_iter()
                    .map(|(path, bytes)| (format!("skills/demo/{path}"), bytes))
                    .collect(),
            }),
            manifest: remote_manifest.clone(),
        };
        let result =
            apply_downloaded_update(&pool, operation_id.clone(), origin, before.clone(), remote)
                .await
                .unwrap();
        assert_eq!(
            manifest_from_local_directory(&target).unwrap(),
            remote_manifest
        );
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read_to_string(link.join("scripts/new.sh")).unwrap(),
            "echo updated\n"
        );
        let state: String =
            sqlx::query_scalar("SELECT state FROM skill_update_operations WHERE operation_id = ?")
                .bind(&operation_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(state, "applied");
        let updated = load_origin(&pool, &origin_target_key(&target))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            updated.last_applied_commit_oid.as_deref(),
            Some("new-commit")
        );
        assert_eq!(
            parse_manifest(updated.base_manifest_json.as_deref()),
            Some(remote_manifest)
        );
        fs::remove_dir_all(&target).unwrap();
        recovery::restore_recovery_entry_impl(&pool, &result.recovery_entry_id.unwrap())
            .await
            .unwrap();
        assert_eq!(manifest_from_local_directory(&target).unwrap(), before);
        assert!(link.join("SKILL.md").is_file());
        assert!(fs::read_dir(target.parent().unwrap())
            .unwrap()
            .all(|entry| {
                !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains("skillsmanage-")
            }));
    }

    #[tokio::test]
    async fn shared_origin_is_manageable_from_compatibility_platforms_only_for_same_path() {
        let temp = TempDir::new().unwrap();
        let (pool, target) = shared_origin_fixture(&temp).await;
        for agent_id in ["antigravity", "gemini-cli", "factory-droid", "cursor"] {
            let row_id = format!("{agent_id}::{}", target.display());
            let mut observation = db::AgentSkillObservation {
                row_id: row_id.clone(),
                agent_id: agent_id.into(),
                skill_id: "demo".into(),
                name: "demo".into(),
                description: None,
                file_path: target.join("SKILL.md").to_string_lossy().into_owned(),
                dir_path: target.to_string_lossy().into_owned(),
                source_kind: "compatibility".into(),
                source_root: target.parent().unwrap().to_string_lossy().into_owned(),
                source_label: None,
                link_type: "copy".into(),
                symlink_target: None,
                is_read_only: true,
                scanned_at: Utc::now().to_rfc3339(),
            };
            db::upsert_agent_skill_observation(&pool, &observation)
                .await
                .unwrap();
            let request = SkillTargetRequest {
                skill_id: "demo".into(),
                agent_id: Some(agent_id.into()),
                row_id: Some(row_id.clone()),
            };
            let resolved = resolve_target(&pool, &request).await.unwrap();
            assert!(
                !resolved.is_read_only,
                "{agent_id}: shared GitHub origin must be manageable"
            );
            assert_eq!(resolved.target_path, target);
            let detail = skills::get_skill_detail_with_row_impl(
                &pool,
                "demo",
                Some(agent_id),
                Some(&row_id),
            )
            .await
            .unwrap();
            assert!(
                detail.is_read_only,
                "platform install/delete remains separate from shared origin"
            );
            assert!(detail.can_manage_origin);

            observation.source_kind = "plugin".into();
            db::upsert_agent_skill_observation(&pool, &observation)
                .await
                .unwrap();
            assert!(resolve_target(&pool, &request).await.unwrap().is_read_only);

            let other = temp.path().join(agent_id).join("demo");
            fs::create_dir_all(&other).unwrap();
            fs::write(other.join("SKILL.md"), "other").unwrap();
            observation.source_kind = "compatibility".into();
            observation.dir_path = other.to_string_lossy().into_owned();
            observation.file_path = other.join("SKILL.md").to_string_lossy().into_owned();
            db::upsert_agent_skill_observation(&pool, &observation)
                .await
                .unwrap();
            assert!(resolve_target(&pool, &request).await.unwrap().is_read_only);
        }
    }

    #[test]
    fn existing_source_record_only_accepts_github_repository_paths() {
        assert_eq!(repo_url_from_record("skills.sh:acme/skills"), Some("https://github.com/acme/skills".to_string()));
        assert_eq!(repo_url_from_record("github:acme/skills"), Some("https://github.com/acme/skills".to_string()));
        assert_eq!(repo_url_from_record("github:acme/skills/other"), None);
        assert_eq!(repo_url_from_record("skills.sh:acme/../other"), None);
    }

    #[tokio::test]
    async fn same_named_catalog_entry_is_only_a_candidate() {
        let temp = TempDir::new().unwrap();
        let pool = setup_origin_db().await;
        let mut skill = write_imported_skill(temp.path(), "demo", "demo");
        skill.source = None;
        db::upsert_skill(&pool, &skill).await.unwrap();
        sqlx::query("INSERT INTO marketplace_skills (id, registry_id, name, download_url, synced_at) VALUES ('demo-catalog', 'anthropic', 'demo', 'https://raw.githubusercontent.com/acme/skills/main/skills/demo/SKILL.md', 'now')")
            .execute(&pool).await.unwrap();

        let discovery = discover_skill_origin_impl(
            &pool,
            &SkillTargetRequest {
                skill_id: "demo".to_string(),
                agent_id: None,
                row_id: None,
            },
            false,
            &mut RemoteCache::new(),
        )
        .await
        .unwrap();
        assert!(discovery.origin.is_none());
        assert_eq!(discovery.candidates.len(), 1);
        assert_eq!(discovery.candidates[0].source_path, "skills/demo");
        assert_eq!(discovery.candidates[0].reason, "catalog_name");
        assert!(load_origin(&pool, &origin_target_key(Path::new(skill.canonical_path.as_deref().unwrap())))
            .await.unwrap().is_none());
        sqlx::query("INSERT INTO skill_origin_ignores (target_key, created_at) VALUES (?, 'now')")
            .bind(origin_target_key(Path::new(
                skill.canonical_path.as_deref().unwrap(),
            )))
            .execute(&pool)
            .await
            .unwrap();
        let ignored = discover_skill_origin_impl(
            &pool,
            &SkillTargetRequest {
                skill_id: "demo".to_string(),
                agent_id: None,
                row_id: None,
            },
            false,
            &mut RemoteCache::new(),
        )
        .await
        .unwrap();
        assert!(ignored.candidates.is_empty());
    }

    #[tokio::test]
    async fn copied_skill_inherits_origin_only_when_files_match() {
        let temp = TempDir::new().unwrap();
        let pool = setup_origin_db().await;
        let skill = write_imported_skill(temp.path(), "demo", "demo");
        persist_imported_skill(&pool, &skill, &imported_repo(), "skills/demo", Some("abc123"))
            .await.unwrap();
        let source = Path::new(skill.canonical_path.as_deref().unwrap());
        let copy = temp.path().join("copy");
        crate::commands::linker::copy_dir_all(source, &copy).unwrap();
        inherit_copied_origin(&pool, source, &copy, "demo", Some("cursor")).await.unwrap();
        let inherited = load_origin(&pool, &origin_target_key(&copy)).await.unwrap().unwrap();
        assert_eq!(inherited.source_path, "skills/demo");
        assert_eq!(inherited.agent_id.as_deref(), Some("cursor"));

        let changed = temp.path().join("changed");
        crate::commands::linker::copy_dir_all(source, &changed).unwrap();
        fs::write(changed.join("SKILL.md"), "different").unwrap();
        inherit_copied_origin(&pool, source, &changed, "demo", None).await.unwrap();
        assert!(load_origin(&pool, &origin_target_key(&changed)).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn recorded_installs_connect_nested_sources_and_matching_platform_copies() {
        let temp = TempDir::new().unwrap();
        let pool = setup_origin_db().await;
        let shared = temp.path().join(".agents/skills");
        let mut entries = serde_json::Map::new();
        let mut files = HashMap::new();
        for name in ["demo", "second"] {
            let mut skill = write_imported_skill(&shared, name, name);
            skill.source = Some("copy".into());
            skill.is_central = false;
            skill.canonical_path = None;
            db::upsert_skill(&pool, &skill).await.unwrap();
            entries.insert(
                name.into(),
                serde_json::json!({
                    "sourceType":"github", "source":"acme/skills",
                    "skillPath":format!("skills/engineering/{name}/SKILL.md"),
                    "ref":"release/v2", "skillFolderHash":"never-a-commit"
                }),
            );
            let mut local = BTreeMap::new();
            collect_local_files(&shared.join(name), &shared.join(name), &mut local).unwrap();
            for (path, bytes) in local {
                files.insert(format!("skills/engineering/{name}/{path}"), bytes);
            }
            for agent in ["universal", "pi", "cursor"] {
                let target = if agent == "universal" {
                    shared.join(name)
                } else {
                    temp.path().join(agent).join(name)
                };
                if agent != "universal" {
                    crate::commands::linker::copy_dir_all(&shared.join(name), &target).unwrap();
                }
                db::upsert_skill_installation(
                    &pool,
                    &db::SkillInstallation {
                        skill_id: name.into(),
                        agent_id: agent.into(),
                        installed_path: target.to_string_lossy().into_owned(),
                        link_type: "copy".into(),
                        symlink_target: None,
                        created_at: "now".into(),
                    },
                )
                .await
                .unwrap();
            }
        }
        fs::write(
            temp.path().join("skills-lock.json"),
            serde_json::json!({"version":1,"skills":entries}).to_string(),
        )
        .unwrap();
        // 독립 복사본의 보조 파일 하나만 달라도 출처를 자동으로 빌리지 않는다.
        fs::write(
            temp.path().join("cursor/demo/references/guide.md"),
            "local edit",
        )
        .unwrap();
        #[cfg(unix)]
        {
            let vault = temp.path().join("vault/renamed-demo");
            fs::create_dir_all(vault.parent().unwrap()).unwrap();
            fs::rename(shared.join("demo"), &vault).unwrap();
            std::os::unix::fs::symlink(&vault, shared.join("demo")).unwrap();
            // 스캐너가 기록한 링크 형식도 실제 설치와 맞춘다.
            sqlx::query("UPDATE skill_installations SET link_type = 'symlink' WHERE skill_id = 'demo' AND agent_id = 'universal'")
                .execute(&pool).await.unwrap();
        }
        // 최신 내용이 다른 설치는 원본 연결만 확정하고 설치 버전을 추측하지 않는다.
        files.insert(
            "skills/engineering/second/SKILL.md".into(),
            b"new upstream".to_vec(),
        );
        let snapshot = Arc::new(github_import::GitHubRepoSnapshot { files });
        let remote = RemoteSnapshot {
            repository_id: Some("123".into()),
            owner: "acme".into(),
            repo: "skills".into(),
            ref_name: "release/v2".into(),
            commit_oid: "latest-commit".into(),
            source_path: "skills/engineering/demo".into(),
            manifest: manifest_from_remote_files(
                &remote_files(&snapshot, "skills/engineering/demo").unwrap(),
            ),
            snapshot,
        };
        let mut cache = RemoteCache::from([(
            (
                "https://github.com/acme/skills".into(),
                Some("release/v2".into()),
            ),
            remote,
        )]);
        discover_installed_origins(&pool, &mut cache).await.unwrap();
        for name in ["demo", "second"] {
            for agent in ["universal", "pi", "cursor"] {
                let request = SkillTargetRequest {
                    skill_id: name.into(),
                    agent_id: Some(agent.into()),
                    row_id: None,
                };
                let discovery = discover_skill_origin_impl(&pool, &request, false, &mut cache)
                    .await
                    .unwrap();
                if name == "demo" && agent == "cursor" {
                    assert!(discovery.origin.is_none());
                } else {
                    let origin = discovery.origin.unwrap();
                    assert_eq!(origin.owner, "acme");
                    assert_eq!(origin.source_path, format!("skills/engineering/{name}"));
                    assert_eq!(origin.ref_name, "release/v2");
                    assert_eq!(
                        origin.last_remote_commit_oid.as_deref(),
                        Some("latest-commit")
                    );
                    assert_eq!(
                        origin.base_commit_oid.as_deref(),
                        if name == "demo" {
                            Some("latest-commit")
                        } else {
                            None
                        }
                    );
                    assert_eq!(
                        origin.baseline_state,
                        if name == "demo" {
                            "verified"
                        } else {
                            "unknown"
                        }
                    );
                }
            }
        }
        assert_eq!(
            cache.len(),
            1,
            "one repository snapshot serves every skill and copy"
        );
        assert_eq!(
            fs::read_to_string(temp.path().join("cursor/demo/references/guide.md")).unwrap(),
            "local edit"
        );
        assert!(fs::read_to_string(shared.join("second/SKILL.md"))
            .unwrap()
            .contains("# body"));
    }

    #[tokio::test]
    async fn discovery_preserves_explicit_unlinks_and_conflicting_installation_records() {
        let temp = TempDir::new().unwrap();
        let pool = setup_origin_db().await;
        let mut skill = write_imported_skill(temp.path(), "demo", "demo");
        skill.source = Some("copy".into());
        db::upsert_skill(&pool, &skill).await.unwrap();
        for (agent, owner) in [("pi", "first"), ("cursor", "second")] {
            let project = temp.path().join(agent);
            let shared = project.join(".agents/skills");
            write_imported_skill(&shared, "demo", "demo");
            fs::write(project.join("skills-lock.json"), serde_json::json!({"version":1,"skills":{"demo":{
                "sourceType":"github", "source":format!("{owner}/skills"), "skillPath":"skills/demo/SKILL.md"
            }}}).to_string()).unwrap();
            db::upsert_skill_installation(
                &pool,
                &db::SkillInstallation {
                    skill_id: "demo".into(),
                    agent_id: agent.into(),
                    installed_path: shared.join("demo").to_string_lossy().into_owned(),
                    link_type: "copy".into(),
                    symlink_target: None,
                    created_at: "now".into(),
                },
            )
            .await
            .unwrap();
        }
        let request = SkillTargetRequest {
            skill_id: "demo".into(),
            agent_id: None,
            row_id: None,
        };
        let discovery = discover_skill_origin_impl(&pool, &request, false, &mut RemoteCache::new())
            .await
            .unwrap();
        assert!(discovery.origin.is_none());
        assert_eq!(discovery.candidates.len(), 2);
        let target = resolve_target(&pool, &request).await.unwrap();
        sqlx::query("INSERT INTO skill_origin_ignores (target_key, created_at) VALUES (?, 'now')")
            .bind(&target.target_key)
            .execute(&pool)
            .await
            .unwrap();
        let discovery = discover_skill_origin_impl(&pool, &request, false, &mut RemoteCache::new())
            .await
            .unwrap();
        assert!(discovery.origin.is_none());
        assert!(discovery.candidates.is_empty());
    }

    /// 실제 설치 파일은 읽기만 하고, 연결 정보는 복제한 DB에서 검증한다.
    #[tokio::test]
    #[ignore = "GitHub 네트워크와 SKILLS_MANAGE_VERIFY_DB, SKILLS_MANAGE_VERIFY_IDS가 필요함"]
    async fn live_recorded_origins_verify_against_github_in_database_copy() {
        let path = PathBuf::from(std::env::var("SKILLS_MANAGE_VERIFY_DB").unwrap());
        assert_ne!(
            path.canonicalize().unwrap(),
            crate::path_utils::app_data_dir()
                .join("db.sqlite")
                .canonicalize()
                .unwrap()
        );
        let pool = db::create_pool(path.to_str().unwrap()).await.unwrap();
        let mut cache = RemoteCache::new();
        let ids = std::env::var("SKILLS_MANAGE_VERIFY_IDS").unwrap();
        let mut checked = 0;
        for id in ids.split(',') {
            let request = SkillTargetRequest {
                skill_id: id.into(),
                agent_id: Some("universal".into()),
                row_id: None,
            };
            let target = resolve_target(&pool, &request).await.unwrap();
            let installation = db::get_skill_installations(&pool, id).await.unwrap()
                .into_iter().find(|installation| installation.agent_id == "universal").unwrap();
            let record = installation_records::read_for_target(Path::new(&installation.installed_path))
                .unwrap().unwrap();
            let before = manifest_from_local_directory(&target.target_path).unwrap();
            let discovery = discover_skill_origin_impl(&pool, &request, false, &mut cache)
                .await
                .unwrap();
            let origin = discovery
                .origin
                .expect("recorded installation must connect");
            assert_eq!(
                format!("https://github.com/{}/{}", origin.owner, origin.repo),
                record.repo_url
            );
            assert_eq!(Some(origin.source_path), record.source_path);
            assert!(origin.last_remote_commit_oid.is_some());
            assert_eq!(
                manifest_from_local_directory(&target.target_path).unwrap(),
                before
            );
            checked += 1;
        }
        eprintln!(
            "Verified {checked} installed origins; {} repository snapshots; skill files unchanged",
            cache.len()
        );
    }

    /// 가져오기 직후처럼 SKILL.md와 동봉 파일을 가진 대상 폴더를 만든다.
    fn write_imported_skill(root: &Path, local_id: &str, frontmatter_name: &str) -> db::Skill {
        let dir = root.join(local_id);
        fs::create_dir_all(dir.join("references")).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {frontmatter_name}\ndescription: imported\n---\n\n# body\n"),
        )
        .unwrap();
        fs::write(dir.join("references/guide.md"), "guide\n").unwrap();
        db::Skill {
            id: local_id.to_string(),
            name: frontmatter_name.to_string(),
            description: Some("imported".to_string()),
            file_path: dir.join("SKILL.md").to_string_lossy().into_owned(),
            canonical_path: Some(dir.to_string_lossy().into_owned()),
            is_central: true,
            source: Some("github:acme/skills".to_string()),
            content: None,
            scanned_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[tokio::test]
    async fn imported_skill_binds_renamed_directory_to_upstream_path() {
        let temp = TempDir::new().unwrap();
        let pool = setup_origin_db().await;
        let central = temp.path().join("central");
        let skill = write_imported_skill(&central, "code-review-hermes", "code-review");

        persist_imported_skill(
            &pool,
            &skill,
            &imported_repo(),
            "skills/code-review",
            Some("abc123"),
        )
        .await
        .unwrap();

        let target_key = origin_target_key(&central.join("code-review-hermes"));
        let origin = load_origin(&pool, &target_key)
            .await
            .unwrap()
            .expect("binding for the just-written target");
        assert_eq!(origin.skill_id, "code-review-hermes");
        assert_eq!(origin.source_path, "skills/code-review");
        assert_eq!(origin.ref_name, "main");
        assert_eq!(
            (origin.owner.as_str(), origin.repo.as_str()),
            ("acme", "skills")
        );
        assert_eq!(origin.baseline_state, "verified");
        assert_eq!(origin.base_commit_oid.as_deref(), Some("abc123"));
        assert_eq!(origin.last_applied_commit_oid.as_deref(), Some("abc123"));
        assert_eq!(origin.last_remote_commit_oid.as_deref(), Some("abc123"));
        assert_eq!(
            origin.target_path,
            central
                .join("code-review-hermes")
                .to_string_lossy()
                .into_owned()
        );
        let manifest: SkillManifest =
            serde_json::from_str(origin.base_manifest_json.as_deref().unwrap()).unwrap();
        assert!(manifest
            .entries
            .iter()
            .any(|entry| entry.path == "references/guide.md"));
        assert!(db::get_skill_by_id(&pool, "code-review-hermes")
            .await
            .unwrap()
            .is_some());
    }

    #[tokio::test]
    async fn reimport_without_commit_keeps_unknown_baseline_without_stale_fields() {
        let temp = TempDir::new().unwrap();
        let pool = setup_origin_db().await;
        let central = temp.path().join("central");
        let skill = write_imported_skill(&central, "code-review-hermes", "code-review");

        persist_imported_skill(
            &pool,
            &skill,
            &imported_repo(),
            "skills/code-review",
            Some("abc123"),
        )
        .await
        .unwrap();
        let target_key = origin_target_key(&central.join("code-review-hermes"));
        // 이전 확인이 저장소 ID를 채워 둔 상태를 만든다.
        sqlx::query("UPDATE skill_origins SET repository_id = '4242' WHERE target_key = ?")
            .bind(&target_key)
            .execute(&pool)
            .await
            .unwrap();

        let other_repo = GitHubRepoRef {
            owner: "other".to_string(),
            repo: "vault".to_string(),
            branch: "release".to_string(),
            normalized_url: "https://github.com/other/vault".to_string(),
        };
        persist_imported_skill(&pool, &skill, &other_repo, "skills/renamed", None)
            .await
            .unwrap();

        let origin = load_origin(&pool, &target_key).await.unwrap().unwrap();
        assert_eq!(
            (
                origin.owner.as_str(),
                origin.repo.as_str(),
                origin.ref_name.as_str()
            ),
            ("other", "vault", "release")
        );
        assert_eq!(origin.source_path, "skills/renamed");
        assert_eq!(origin.baseline_state, "unknown");
        assert!(origin.base_commit_oid.is_none());
        assert!(origin.base_manifest_json.is_none());
        assert!(origin.last_applied_commit_oid.is_none());
        assert!(origin.last_applied_at.is_none());
        assert!(origin.last_checked_at.is_none());
        assert!(origin.last_remote_commit_oid.is_none());
        assert!(origin.repository_id.is_none());
    }

    #[tokio::test]
    async fn persist_refuses_target_without_skill_md_and_leaves_no_records() {
        let temp = TempDir::new().unwrap();
        let pool = setup_origin_db().await;
        let central = temp.path().join("central");
        let dir = central.join("not-a-skill");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("README.md"), "no frontmatter\n").unwrap();
        let skill = db::Skill {
            id: "not-a-skill".to_string(),
            name: "not-a-skill".to_string(),
            description: None,
            file_path: dir.join("README.md").to_string_lossy().into_owned(),
            canonical_path: Some(dir.to_string_lossy().into_owned()),
            is_central: true,
            source: Some("github:acme/skills".to_string()),
            content: None,
            scanned_at: chrono::Utc::now().to_rfc3339(),
        };

        let error = persist_imported_skill(&pool, &skill, &imported_repo(), "skills/x", Some("abc"))
            .await
            .unwrap_err();

        assert!(error.contains("SKILL.md"), "unexpected error: {error}");
        assert!(db::get_skill_by_id(&pool, "not-a-skill")
            .await
            .unwrap()
            .is_none());
        assert!(load_origin(&pool, &origin_target_key(&dir))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn detail_lookup_sees_the_imported_origin() {
        let temp = TempDir::new().unwrap();
        let pool = setup_origin_db().await;
        let central = temp.path().join("central");
        let skill = write_imported_skill(&central, "code-review-hermes", "code-review");
        persist_imported_skill(
            &pool,
            &skill,
            &imported_repo(),
            "skills/code-review",
            Some("abc123"),
        )
        .await
        .unwrap();

        let target = resolve_target(
            &pool,
            &SkillTargetRequest {
                skill_id: "code-review-hermes".to_string(),
                agent_id: None,
                row_id: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(
            target.target_key,
            origin_target_key(&central.join("code-review-hermes"))
        );
        let origin = load_origin(&pool, &target.target_key)
            .await
            .unwrap()
            .expect("skill detail resolves the imported origin");
        assert_eq!(origin.source_path, "skills/code-review");
    }

    #[tokio::test]
    async fn central_rescan_keeps_the_imported_origin() {
        let temp = TempDir::new().unwrap();
        let pool = setup_origin_db().await;
        sqlx::query("DELETE FROM agents WHERE id <> 'central'")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("DELETE FROM scan_directories")
            .execute(&pool)
            .await
            .unwrap();
        let central = temp.path().join("central");
        let skill = write_imported_skill(&central, "code-review-hermes", "code-review");
        persist_imported_skill(
            &pool,
            &skill,
            &imported_repo(),
            "skills/code-review",
            Some("abc123"),
        )
        .await
        .unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'central'")
            .bind(central.to_string_lossy().into_owned())
            .execute(&pool)
            .await
            .unwrap();

        crate::commands::scanner::scan_all_skills_impl(&pool)
            .await
            .unwrap();

        let scanned = db::get_skill_by_id(&pool, "code-review-hermes")
            .await
            .unwrap()
            .expect("rescan keeps the central skill");
        let target_key = origin_target_key(Path::new(
            scanned
                .canonical_path
                .as_deref()
                .expect("central rescan records the canonical path"),
        ));
        let summaries = origin_summaries_for_targets(&pool, &[target_key.clone()])
            .await
            .unwrap();
        assert_eq!(
            summaries
                .get(&target_key)
                .map(|summary| summary.source_path.as_str()),
            Some("skills/code-review")
        );
        assert!(!summaries.get(&target_key).unwrap().update_available);
        sqlx::query("UPDATE skill_origins SET last_remote_manifest_json = '{\"entries\":[]}' WHERE target_key = ?")
            .bind(&target_key).execute(&pool).await.unwrap();
        let changed = origin_summaries_for_targets(&pool, &[target_key.clone()]).await.unwrap();
        assert!(changed.get(&target_key).unwrap().update_available);
        assert_eq!(
            load_origin(&pool, &target_key).await.unwrap().unwrap().owner,
            "acme"
        );
    }

    #[tokio::test]
    async fn collection_rename_and_delete_keep_the_origin_reference() {
        let temp = TempDir::new().unwrap();
        let pool = setup_origin_db().await;
        let central = temp.path().join("central");
        let skill = write_imported_skill(&central, "code-review-hermes", "code-review");
        persist_imported_skill(
            &pool,
            &skill,
            &imported_repo(),
            "skills/code-review",
            Some("abc123"),
        )
        .await
        .unwrap();
        let target_key = origin_target_key(&central.join("code-review-hermes"));

        let collection = db::create_collection(&pool, "acme/skills", Some("bundle"))
            .await
            .unwrap();
        db::add_skill_to_collection(&pool, &collection.id, &skill.id)
            .await
            .unwrap();
        db::update_collection(&pool, &collection.id, "acme/skills (renamed)", None)
            .await
            .unwrap();
        assert!(load_origin(&pool, &target_key).await.unwrap().is_some());

        db::delete_collection(&pool, &collection.id).await.unwrap();

        let origin = load_origin(&pool, &target_key)
            .await
            .unwrap()
            .expect("collection deletion keeps the repository origin");
        assert_eq!(origin.source_path, "skills/code-review");
        assert_eq!(origin.owner, "acme");
    }

    #[tokio::test]
    async fn origin_summaries_resolve_targets_beyond_the_first_query_chunk() {
        let temp = TempDir::new().unwrap();
        let pool = setup_origin_db().await;
        let central = temp.path().join("central");
        let skill = write_imported_skill(&central, "code-review-hermes", "code-review");
        persist_imported_skill(
            &pool,
            &skill,
            &imported_repo(),
            "skills/code-review",
            Some("abc123"),
        )
        .await
        .unwrap();

        // 첫 조회 묶음을 넘겨도 실제 바인딩이 있는 경로는 같은 맵에 담긴다.
        let mut target_keys: Vec<String> = (0..MAX_ORIGIN_QUERY_KEYS)
            .map(|index| format!("/nowhere/missing-{index}"))
            .collect();
        target_keys.push(origin_target_key(&central.join("code-review-hermes")));

        let summaries = origin_summaries_for_targets(&pool, &target_keys)
            .await
            .unwrap();

        assert_eq!(summaries.len(), 1);
        let origin = summaries
            .get(&target_keys[MAX_ORIGIN_QUERY_KEYS])
            .expect("a named target in the second chunk still resolves");
        assert_eq!(origin.repo, "skills");
        assert_eq!(origin.source_path, "skills/code-review");
        assert_eq!(origin.ref_name, "main");
    }
}

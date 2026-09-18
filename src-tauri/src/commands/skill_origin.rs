use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Row};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use tauri::State;
use tokio::sync::{Mutex, MutexGuard};
use uuid::Uuid;

use crate::commands::{github_import, recovery, scanner, skills};
use crate::db::{self, DbPool};
use crate::AppState;

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
    snapshot: github_import::GitHubRepoSnapshot,
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
        PathBuf::from(&detail.dir_path)
    } else if let Some(installation) = selected_installation {
        if installation.link_type == "symlink" {
            detail
                .canonical_path
                .as_deref()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(&detail.dir_path))
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
    let target_key = origin_target_key(&target_path);
    Ok(ResolvedSkillTarget {
        skill_id: request.skill_id.clone(),
        agent_id: request.agent_id.clone(),
        row_id: request.row_id.clone(),
        target_key,
        target_path,
        is_read_only: detail.is_read_only,
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
            "SELECT target_key, owner, repo, source_path, ref_name
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
    query
        .execute(pool)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
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

    let commit_api = format!(
        "https://api.github.com/repos/{}/{}/commits/{}",
        repo.owner, repo.repo, ref_name
    );
    let commit_response =
        github_import::send_with_auth_fallback(&client, &commit_api, auth.as_deref())
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
        snapshot,
        manifest,
    })
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
    let origin = load_origin(pool, &target.target_key)
        .await?
        .ok_or_else(|| "This skill is not linked to a GitHub origin".to_string())?;
    let local = manifest_from_local_directory(&target.target_path)?;
    let repo_url = format!("https://github.com/{}/{}", origin.owner, origin.repo);
    let remote =
        match fetch_remote_snapshot(pool, &repo_url, &origin.source_path, Some(&origin.ref_name))
            .await
        {
            Ok(remote) => remote,
            Err(error) => {
                let now = Utc::now().to_rfc3339();
                sqlx::query(
                    "UPDATE skill_origins SET last_error = ?, updated_at = ? WHERE binding_id = ?",
                )
                .bind(&error)
                .bind(&now)
                .bind(&origin.binding_id)
                .execute(pool)
                .await
                .map_err(|db_error| db_error.to_string())?;
                return Err(error);
            }
        };
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
        .expect("origin exists");
    let empty = SkillManifest::default();
    Ok(SkillOriginStatus {
        origin: origin_info(&refreshed, !target.is_read_only),
        state,
        local_vs_remote: summarize_changes(&local, &remote.manifest),
        local_vs_base: summarize_changes(base.as_ref().unwrap_or(&empty), &local),
        remote_vs_base: summarize_changes(base.as_ref().unwrap_or(&empty), &remote.manifest),
        remote_commit_oid: remote.commit_oid,
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

#[tauri::command]
pub async fn link_skill_origin(
    state: State<'_, AppState>,
    request: LinkSkillOriginRequest,
) -> Result<SkillOriginStatus, String> {
    let target = resolve_target(&state.db, &request.target).await?;
    if target.is_read_only {
        return Err("Read-only observed skills cannot be linked for in-place updates".to_string());
    }
    let remote = fetch_remote_snapshot(
        &state.db,
        &request.repo_url,
        &request.source_path,
        request.ref_name.as_deref(),
    )
    .await?;
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
    let binding_id = load_origin(&state.db, &target.target_key)
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
    .execute(&state.db)
    .await
    .map_err(|error| error.to_string())?;
    check_origin_impl(&state.db, &target).await
}

#[tauri::command]
pub async fn unlink_skill_origin(
    state: State<'_, AppState>,
    target: SkillTargetRequest,
) -> Result<(), String> {
    let target = resolve_target(&state.db, &target).await?;
    sqlx::query("DELETE FROM skill_origins WHERE target_key = ?")
        .bind(target.target_key)
        .execute(&state.db)
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn check_skill_origin(
    state: State<'_, AppState>,
    target: SkillTargetRequest,
) -> Result<SkillOriginStatus, String> {
    let target = resolve_target(&state.db, &target).await?;
    check_origin_impl(&state.db, &target).await
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

    let central_root = db::get_central_skills_dir(&state.db).await?;
    let target_canonical = target.canonicalize().unwrap_or_else(|_| target.clone());
    let central_canonical = central_root.canonicalize().unwrap_or(central_root);
    let recovery_result = if target_canonical.starts_with(&central_canonical) {
        recovery::backup_vault_before_removal(
            &state.db,
            &target,
            format!("GitHub 업데이트 전 백업: {}", origin.skill_id),
        )
        .await
    } else if let Some(agent_id) = origin.agent_id.as_deref() {
        let installation = db::get_skill_installation(&state.db, &origin.skill_id, agent_id)
            .await?
            .ok_or_else(|| "Managed installation record is missing".to_string())?;
        recovery::backup_copy_installation(&state.db, &installation).await
    } else {
        remove_tree(&stage);
        return Err(
            "This target is outside the managed vault and has no managed installation backup path"
                .to_string(),
        );
    };
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
        .execute(&state.db)
        .await
        .map_err(|error| error.to_string())?;

    if let Err(error) = fs::rename(&target, &quarantine) {
        remove_tree(&stage);
        let message = format!("Failed to quarantine current skill: {error}");
        let _ = sqlx::query("UPDATE skill_update_operations SET state='failed', error=?, updated_at=? WHERE operation_id=?")
            .bind(&message).bind(Utc::now().to_rfc3339()).bind(&operation_id).execute(&state.db).await;
        return Err(message);
    }
    if let Err(error) = fs::rename(&stage, &target) {
        let _ = fs::rename(&quarantine, &target);
        remove_tree(&stage);
        let message = format!("Failed to activate updated skill: {error}");
        let _ = sqlx::query("UPDATE skill_update_operations SET state='failed', error=?, updated_at=? WHERE operation_id=?")
            .bind(&message).bind(Utc::now().to_rfc3339()).bind(&operation_id).execute(&state.db).await;
        return Err(message);
    }

    let db_result = async {
        let mut transaction = state.db.begin().await.map_err(|error| error.to_string())?;
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
            .bind(&message).bind(Utc::now().to_rfc3339()).bind(&operation_id).execute(&state.db).await;
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

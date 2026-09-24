use chrono::Utc;
use flate2::read::GzDecoder;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};
use tauri::{AppHandle, Emitter, State};

use crate::{
    commands::skill_origin,
    db::{self, DbPool, Skill},
    AppState,
};

pub(crate) fn github_request(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
) -> reqwest::RequestBuilder {
    let req = client.get(url);
    match token {
        Some(t) => req.bearer_auth(t),
        None => req,
    }
}

/// Send a GET request; if the token is invalid (401), retry without it.
/// 403 (rate limit, permissions) still propagates so the caller can act.
pub(crate) async fn send_with_auth_fallback(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
) -> Result<reqwest::Response, reqwest::Error> {
    if let Some(t) = token {
        let resp = github_request(client, url, Some(t)).send().await?;
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return github_request(client, url, None).send().await;
        }
        return Ok(resp);
    }
    github_request(client, url, None).send().await
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubRepoRef {
    pub owner: String,
    pub repo: String,
    pub branch: String,
    pub normalized_url: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateResolution {
    Overwrite,
    Skip,
    Rename,
}

/// 설치 대상 자리에 이미 있는 것이 무엇인지 구분한다.
///
/// `central`은 레코드가 대상 폴더를 소유할 때만 명시적으로 덮어쓸 수 있고,
/// `non_central`과 `unmanaged_path`는 덮어쓸 수 없다.
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitHubSkillConflictKind {
    Central,
    NonCentral,
    UnmanagedPath,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubSkillConflict {
    pub existing_skill_id: String,
    pub existing_name: String,
    pub existing_canonical_path: Option<String>,
    pub proposed_skill_id: String,
    pub proposed_name: String,
    pub conflict_kind: GitHubSkillConflictKind,
    pub existing_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubSkillPreview {
    pub source_path: String,
    pub skill_id: String,
    pub skill_name: String,
    pub description: Option<String>,
    pub root_directory: String,
    pub skill_directory_name: String,
    pub download_url: String,
    pub conflict: Option<GitHubSkillConflict>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubRepoPreview {
    pub repo: GitHubRepoRef,
    pub skills: Vec<GitHubSkillPreview>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubSkillImportSelection {
    pub source_path: String,
    pub resolution: DuplicateResolution,
    pub renamed_skill_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ImportedGitHubSkillSummary {
    pub source_path: String,
    pub original_skill_id: String,
    pub imported_skill_id: String,
    pub skill_name: String,
    pub target_directory: String,
    pub resolution: DuplicateResolution,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubRepoImportResult {
    pub repo: GitHubRepoRef,
    pub imported_skills: Vec<ImportedGitHubSkillSummary>,
    pub skipped_skills: Vec<String>,
}

/// `blocked`는 쓰기 전 차단, `failed`는 실행 중 실패를 뜻한다.
/// 실행 중 실패하면 같은 요청의 앞선 스킬이 이미 저장되어 있을 수 있다.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubImportFailure {
    pub code: GitHubImportFailureCode,
    pub message: String,
    pub source_path: Option<String>,
    pub skill_id: Option<String>,
    pub existing_path: Option<String>,
    pub imported_skills: Vec<ImportedGitHubSkillSummary>,
    pub skipped_skills: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitHubImportFailureCode {
    Blocked,
    Failed,
}

/// 가져오기 오류는 기존 문자열이거나 구조화된 실패일 수 있다.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum GitHubImportError {
    Message(String),
    Failure(Box<GitHubImportFailure>),
}

impl GitHubImportError {
    fn plain(message: impl Into<String>) -> Self {
        Self::Message(message.into())
    }
}


#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitHubImportProgressPhase {
    Preparing,
    Writing,
    Finalizing,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct GitHubImportProgressPayload {
    pub phase: GitHubImportProgressPhase,
    pub current_skill: Option<String>,
    pub current_path: Option<String>,
    pub completed_files: usize,
    pub total_files: usize,
    pub completed_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct SkillFrontmatter {
    pub(crate) name: String,
    pub(crate) description: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct RemoteSkillCandidate {
    pub(crate) source_path: String,
    pub(crate) skill_id: String,
    pub(crate) skill_name: String,
    pub(crate) description: Option<String>,
    pub(crate) root_directory: String,
    pub(crate) skill_directory_name: String,
    pub(crate) download_url: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct GitHubRepoSnapshot {
    pub(crate) files: HashMap<String, Vec<u8>>,
}

const GITHUB_PAT_SETTING_KEY: &str = "github_pat";

#[derive(Debug, Clone, PartialEq, Eq)]
enum GitHubAccessDenialKind {
    RateLimited {
        reset_at: Option<String>,
        remaining: Option<String>,
    },
    AuthenticationOrPermission,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GitHubAccessDenial {
    kind: GitHubAccessDenialKind,
    operation: &'static str,
    status: reqwest::StatusCode,
    github_message: Option<String>,
}

impl fmt::Display for GitHubAccessDenial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = self.status.as_u16();
        match &self.kind {
            GitHubAccessDenialKind::RateLimited {
                reset_at,
                remaining,
            } => {
                write!(
                    f,
                    "GitHub API access was denied while {} because the rate limit was exceeded (HTTP {}). Retry later",
                    self.operation, status
                )?;
                if let Some(reset_at) = reset_at {
                    write!(f, " after {} UTC", reset_at)?;
                }
                write!(f, " or use authenticated GitHub requests")?;
                if let Some(remaining) = remaining {
                    write!(f, " (remaining quota: {})", remaining)?;
                }
                if let Some(message) = &self.github_message {
                    write!(f, ". GitHub said: {}", message)?;
                } else {
                    write!(f, ".")?;
                }
                Ok(())
            }
            GitHubAccessDenialKind::AuthenticationOrPermission => {
                write!(
                    f,
                    "GitHub denied access while {} (HTTP {}). The repository may require authentication, your API quota may need authenticated requests, or the token/permissions are insufficient. Verify repository access, sign in with a GitHub token that can read the repo, or retry later",
                    self.operation, status
                )?;
                if let Some(message) = &self.github_message {
                    write!(f, ". GitHub said: {}", message)?;
                } else {
                    write!(f, ".")?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct GitHubErrorResponse {
    message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitHubFetchSurface {
    Api,
    Raw,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MirrorAttemptOutcome {
    status: Option<reqwest::StatusCode>,
    error_message: String,
}

#[derive(Debug, Clone, Copy)]
struct GitHubMirrorEndpoint {
    label: &'static str,
    api_base: &'static str,
    raw_base: &'static str,
}

const GITHUB_MIRROR_ENDPOINTS: &[GitHubMirrorEndpoint] = &[
    GitHubMirrorEndpoint {
        label: "github",
        api_base: "https://api.github.com",
        raw_base: "https://raw.githubusercontent.com",
    },
    GitHubMirrorEndpoint {
        label: "ghfast",
        api_base: "https://ghfast.top/https://api.github.com",
        raw_base: "https://ghfast.top/https://raw.githubusercontent.com",
    },
    GitHubMirrorEndpoint {
        label: "ghproxy",
        api_base: "https://ghproxy.net/https://api.github.com",
        raw_base: "https://ghproxy.net/https://raw.githubusercontent.com",
    },
    GitHubMirrorEndpoint {
        label: "gitproxy",
        api_base: "https://mirror.ghproxy.com/https://api.github.com",
        raw_base: "https://mirror.ghproxy.com/https://raw.githubusercontent.com",
    },
];

#[tauri::command]
pub async fn preview_github_repo_import(
    state: State<'_, AppState>,
    repo_url: String,
) -> Result<GitHubRepoPreview, String> {
    preview_github_repo_import_impl(&state.db, &repo_url).await
}

#[tauri::command]
pub async fn import_github_repo_skills(
    app: AppHandle,
    state: State<'_, AppState>,
    repo_url: String,
    selections: Vec<GitHubSkillImportSelection>,
) -> Result<GitHubRepoImportResult, GitHubImportError> {
    import_github_repo_skills_impl(&state.db, &repo_url, selections, Some(&app)).await
}

#[tauri::command]
pub async fn fetch_github_skill_markdown(
    state: State<'_, AppState>,
    download_url: String,
) -> Result<String, String> {
    let client = github_client()?;
    let auth = github_direct_auth_from_settings(&state.db).await?;
    fetch_raw_text(&client, &download_url, auth.as_deref()).await
}

async fn preview_github_repo_import_impl(
    pool: &DbPool,
    repo_url: &str,
) -> Result<GitHubRepoPreview, String> {
    let auth = github_direct_auth_from_settings(pool).await?;
    let repo = resolve_repo_ref(repo_url, auth.as_deref()).await?;
    let candidates = fetch_repo_skill_candidates(&repo, auth.as_deref()).await?;
    let central_root = central_skills_root(pool).await?;
    let skills = build_preview_skills(pool, &central_root, &candidates).await?;

    if skills.is_empty() {
        return Err(
            "No importable skills found in this repository. Supported layouts are repo-root skill directories or a top-level skills/ directory."
                .to_string(),
        );
    }

    Ok(GitHubRepoPreview { repo, skills })
}

async fn import_github_repo_skills_impl(
    pool: &DbPool,
    repo_url: &str,
    selections: Vec<GitHubSkillImportSelection>,
    app: Option<&AppHandle>,
) -> Result<GitHubRepoImportResult, GitHubImportError> {
    emit_github_import_progress(
        app,
        GitHubImportProgressPayload {
            phase: GitHubImportProgressPhase::Preparing,
            current_skill: None,
            current_path: None,
            completed_files: 0,
            total_files: 0,
            completed_bytes: 0,
            total_bytes: 0,
        },
    );

    let auth = github_direct_auth_from_settings(pool)
        .await
        .map_err(GitHubImportError::plain)?;
    let repo = resolve_repo_ref(repo_url, auth.as_deref())
        .await
        .map_err(GitHubImportError::plain)?;
    let client = github_client().map_err(GitHubImportError::plain)?;

    // 확인한 커밋이 있으면 그 커밋으로 고정해 내려받는다. 조회에 실패하면 기본
    // 브랜치로 진행하되, 검증된 기준으로 기록하지 않는다.
    let commit_oid = resolve_repo_commit_oid(&repo, auth.as_deref()).await.ok();
    let download_ref = match &commit_oid {
        Some(commit) => GitHubRepoRef {
            branch: commit.clone(),
            ..repo.clone()
        },
        None => repo.clone(),
    };
    let snapshot = download_repo_snapshot(&client, &download_ref, auth.as_deref())
        .await
        .map_err(GitHubImportError::plain)?;
    let candidates = build_repo_skill_candidates_from_snapshot(&download_ref, &snapshot)
        .map_err(GitHubImportError::plain)?;
    if candidates.is_empty() {
        return Err(GitHubImportError::plain(
            "No importable skills found in this repository. Supported layouts are repo-root skill directories or a top-level skills/ directory.",
        ));
    }

    install_repo_skills_from_snapshot(
        pool,
        RepoSnapshotImportRequest {
            repo,
            commit_oid,
            snapshot,
            candidates,
            selections,
        },
        app,
    )
    .await
}

/// 이미 내려받은 저장소 내용으로 설치만 수행하는 요청. 테스트가 네트워크 없이
/// 실제 설치 경로를 실행할 수 있도록 분리했다.
#[derive(Debug, Clone)]
struct RepoSnapshotImportRequest {
    repo: GitHubRepoRef,
    commit_oid: Option<String>,
    snapshot: GitHubRepoSnapshot,
    candidates: Vec<RemoteSkillCandidate>,
    selections: Vec<GitHubSkillImportSelection>,
}

#[derive(Debug, Clone)]
struct StagedImport {
    candidate: RemoteSkillCandidate,
    final_skill_id: String,
    resolution: DuplicateResolution,
    target_dir: PathBuf,
    /// 기존 중앙 레코드가 대상을 소유해 교체할 수 있는지 여부.
    owned_target: bool,
    source_files: Vec<SnapshotSourceFile>,
}

/// 실제로 완료된 결과. 실패 시 이미 저장된 스킬과 건너뛴 스킬을 정확히 보고한다.
#[derive(Debug, Clone, Default)]
struct ImportBatchProgress {
    imported_skills: Vec<ImportedGitHubSkillSummary>,
    skipped_skills: Vec<String>,
}

impl ImportBatchProgress {
    fn blocked(
        &self,
        message: String,
        source_path: Option<&str>,
        skill_id: Option<&str>,
        existing_path: Option<PathBuf>,
    ) -> GitHubImportError {
        GitHubImportError::Failure(Box::new(GitHubImportFailure {
            code: GitHubImportFailureCode::Blocked,
            message,
            source_path: source_path.map(str::to_string),
            skill_id: skill_id.map(str::to_string),
            existing_path: existing_path.map(|path| path.to_string_lossy().into_owned()),
            imported_skills: self.imported_skills.clone(),
            skipped_skills: self.skipped_skills.clone(),
        }))
    }

    fn failed(
        &self,
        message: String,
        candidate: &RemoteSkillCandidate,
        skill_id: &str,
        existing_path: Option<PathBuf>,
    ) -> GitHubImportError {
        GitHubImportError::Failure(Box::new(GitHubImportFailure {
            code: GitHubImportFailureCode::Failed,
            message,
            source_path: Some(candidate.source_path.clone()),
            skill_id: Some(skill_id.to_string()),
            existing_path: existing_path.map(|path| path.to_string_lossy().into_owned()),
            imported_skills: self.imported_skills.clone(),
            skipped_skills: self.skipped_skills.clone(),
        }))
    }
}

/// 내려받은 스냅샷에서 선택한 스킬만 설치한다.
///
/// 첫 쓰기 전에 모든 대상 경로를 검증한다. 각 스킬은 대상 옆의 스테이징
/// 디렉터리에 먼저 쓰고 나서 교체하므로, 실패해도 반쯤 쓰인 스킬이 남지 않는다.
/// 뒤 스킬이 실패해도 앞서 완료된 스킬은 되돌리지 않고 그대로 보고한다.
async fn install_repo_skills_from_snapshot(
    pool: &DbPool,
    request: RepoSnapshotImportRequest,
    app: Option<&AppHandle>,
) -> Result<GitHubRepoImportResult, GitHubImportError> {
    if request.selections.is_empty() {
        return Err(GitHubImportError::plain(
            "Select at least one skill to import.",
        ));
    }

    let central_root = central_skills_root(pool)
        .await
        .map_err(GitHubImportError::plain)?;
    std::fs::create_dir_all(&central_root).map_err(|error| {
        GitHubImportError::plain(format!(
            "Failed to create central skills directory: {}",
            error
        ))
    })?;

    let mut progress = ImportBatchProgress::default();
    let selected = resolve_selected_candidates(&request.candidates, request.selections, &progress)?;

    let mut reserved_ids = HashSet::new();
    let mut staging_ops = Vec::with_capacity(selected.len());
    for (candidate, selection) in &selected {
        if selection.resolution == DuplicateResolution::Skip {
            progress.skipped_skills.push(candidate.source_path.clone());
            continue;
        }
        staging_ops.push(
            stage_import_target(
                pool,
                &central_root,
                candidate,
                selection,
                &mut reserved_ids,
                &progress,
            )
            .await?,
        );
    }

    for op in &mut staging_ops {
        op.source_files = collect_snapshot_source_files(&request.snapshot, &op.candidate.source_path)
            .map_err(|message| {
                progress.blocked(
                    message,
                    Some(op.candidate.source_path.as_str()),
                    Some(op.final_skill_id.as_str()),
                    Some(op.target_dir.clone()),
                )
            })?;
    }

    let total_files = staging_ops
        .iter()
        .map(|op| op.source_files.len())
        .sum::<usize>();
    let total_bytes = staging_ops
        .iter()
        .flat_map(|op| op.source_files.iter())
        .map(|file| file.byte_len as u64)
        .sum::<u64>();
    let mut progress_state = GitHubImportProgressState {
        completed_files: 0,
        total_files,
        completed_bytes: 0,
        total_bytes,
    };

    emit_github_import_progress(
        app,
        GitHubImportProgressPayload {
            phase: GitHubImportProgressPhase::Writing,
            current_skill: None,
            current_path: None,
            completed_files: 0,
            total_files,
            completed_bytes: 0,
            total_bytes,
        },
    );

    for op in &staging_ops {
        let staging_container =
            unique_staging_container(&central_root, &op.final_skill_id, "stage");
        let staging_dir = staging_container.join(&op.final_skill_id);
        if let Err(message) = write_snapshot_source_to_target(
            &request.snapshot,
            &op.source_files,
            &staging_dir,
            &op.candidate.source_path,
            &mut progress_state,
            app,
        ) {
            remove_directory_if_present(&staging_container);
            return Err(progress.failed(
                message,
                &op.candidate,
                &op.final_skill_id,
                Some(op.target_dir.clone()),
            ));
        }

        let staged_skill_md = staging_dir.join("SKILL.md");
        let frontmatter = match std::fs::read_to_string(&staged_skill_md)
            .map_err(|error| format!("Failed to read imported SKILL.md: {}", error))
            .and_then(|raw| {
                parse_frontmatter(&raw).ok_or_else(|| {
                    format!(
                        "Imported skill '{}' is missing valid frontmatter.",
                        op.candidate.source_path
                    )
                })
            }) {
            Ok(frontmatter) => frontmatter,
            Err(message) => {
                remove_directory_if_present(&staging_container);
                return Err(progress.failed(
                    message,
                    &op.candidate,
                    &op.final_skill_id,
                    Some(op.target_dir.clone()),
                ));
            }
        };

        // 기존 대상은 새 파일이 준비된 뒤에만 옮긴다. 실패하면 원래 대상을 되돌린다.
        let backup_container = if op.owned_target {
            let backup_container =
                unique_staging_container(&central_root, &op.final_skill_id, "old");
            let moved_aside = std::fs::create_dir_all(&backup_container).and_then(|_| {
                std::fs::rename(&op.target_dir, backup_container.join(&op.final_skill_id))
            });
            if let Err(error) = moved_aside {
                remove_directory_if_present(&staging_container);
                remove_directory_if_present(&backup_container);
                return Err(progress.failed(
                    format!(
                        "Failed to move the existing skill '{}' aside: {}",
                        op.final_skill_id, error
                    ),
                    &op.candidate,
                    &op.final_skill_id,
                    Some(op.target_dir.clone()),
                ));
            }
            Some(backup_container)
        } else {
            None
        };

        if let Err(error) = std::fs::rename(&staging_dir, &op.target_dir) {
            remove_directory_if_present(&staging_container);
            let mut message =
                format!("Failed to install skill '{}': {}", op.final_skill_id, error);
            if let Some(note) =
                restore_previous_target(&backup_container, &op.final_skill_id, &op.target_dir)
            {
                message.push(' ');
                message.push_str(&note);
            }
            return Err(progress.failed(
                message,
                &op.candidate,
                &op.final_skill_id,
                Some(op.target_dir.clone()),
            ));
        }
        remove_directory_if_present(&staging_container);

        let skill_md_path = op.target_dir.join("SKILL.md");
        let skill_name = frontmatter.name;
        let skill_description = frontmatter.description;
        let db_skill = Skill {
            id: op.final_skill_id.clone(),
            name: skill_name.clone(),
            description: skill_description.clone(),
            file_path: skill_md_path.to_string_lossy().into_owned(),
            canonical_path: Some(op.target_dir.to_string_lossy().into_owned()),
            is_central: true,
            source: Some(format!("github:{}/{}", request.repo.owner, request.repo.repo)),
            content: None,
            scanned_at: Utc::now().to_rfc3339(),
        };
        if let Err(error) = skill_origin::persist_imported_skill(
            pool,
            &db_skill,
            &request.repo,
            &op.candidate.source_path,
            request.commit_oid.as_deref(),
        )
        .await
        {
            // 이 스킬의 파일만 되돌린다. 앞서 완료된 가져오기는 그대로 남긴다.
            let mut message = format!(
                "Failed to record the imported skill '{}': {}",
                op.final_skill_id, error
            );
            if let Some(note) = remove_imported_target(&op.target_dir) {
                message.push(' ');
                message.push_str(&note);
            }
            if let Some(note) =
                restore_previous_target(&backup_container, &op.final_skill_id, &op.target_dir)
            {
                message.push(' ');
                message.push_str(&note);
            }
            return Err(progress.failed(
                message,
                &op.candidate,
                &op.final_skill_id,
                Some(op.target_dir.clone()),
            ));
        }
        if let Some(backup_container) = &backup_container {
            remove_directory_if_present(backup_container);
        }

        progress.imported_skills.push(ImportedGitHubSkillSummary {
            source_path: op.candidate.source_path.clone(),
            original_skill_id: op.candidate.skill_id.clone(),
            imported_skill_id: op.final_skill_id.clone(),
            skill_name,
            target_directory: op.target_dir.to_string_lossy().into_owned(),
            resolution: op.resolution.clone(),
        });
    }

    emit_github_import_progress(
        app,
        GitHubImportProgressPayload {
            phase: GitHubImportProgressPhase::Finalizing,
            current_skill: None,
            current_path: None,
            completed_files: progress_state.completed_files,
            total_files: progress_state.total_files,
            completed_bytes: progress_state.completed_bytes,
            total_bytes: progress_state.total_bytes,
        },
    );

    Ok(GitHubRepoImportResult {
        repo: request.repo,
        imported_skills: progress.imported_skills,
        skipped_skills: progress.skipped_skills,
    })
}

fn resolve_selected_candidates<'a>(
    candidates: &'a [RemoteSkillCandidate],
    selections: Vec<GitHubSkillImportSelection>,
    progress: &ImportBatchProgress,
) -> Result<Vec<(&'a RemoteSkillCandidate, GitHubSkillImportSelection)>, GitHubImportError> {
    let mut selected_paths = HashSet::new();
    let mut selected = Vec::with_capacity(selections.len());
    for selection in selections {
        if !selected_paths.insert(selection.source_path.clone()) {
            return Err(progress.blocked(
                format!(
                    "Skill '{}' was selected more than once.",
                    selection.source_path
                ),
                Some(selection.source_path.as_str()),
                None,
                None,
            ));
        }
        let candidate = candidates
            .iter()
            .find(|candidate| candidate.source_path == selection.source_path)
            .ok_or_else(|| {
                progress.blocked(
                    format!(
                        "Selected skill '{}' is no longer available in the preview.",
                        selection.source_path
                    ),
                    Some(selection.source_path.as_str()),
                    None,
                    None,
                )
            })?;
        selected.push((candidate, selection));
    }
    Ok(selected)
}

/// 선택 하나를 쓰기 전에 검증하고 대상 경로를 확정한다.
async fn stage_import_target(
    pool: &DbPool,
    central_root: &Path,
    candidate: &RemoteSkillCandidate,
    selection: &GitHubSkillImportSelection,
    reserved_ids: &mut HashSet<String>,
    progress: &ImportBatchProgress,
) -> Result<StagedImport, GitHubImportError> {
    let final_skill_id = match selection.resolution {
        DuplicateResolution::Rename => sanitize_skill_id(
            selection.renamed_skill_id.as_deref().ok_or_else(|| {
                progress.blocked(
                    format!(
                        "Skill '{}' requires a renamed skill id for rename resolution.",
                        candidate.source_path
                    ),
                    Some(candidate.source_path.as_str()),
                    Some(candidate.skill_id.as_str()),
                    None,
                )
            })?,
        )
        .map_err(|message| {
            progress.blocked(
                message,
                Some(candidate.source_path.as_str()),
                Some(candidate.skill_id.as_str()),
                None,
            )
        })?,
        _ => candidate.skill_id.clone(),
    };
    let target_dir = central_root.join(&final_skill_id);
    let existing_record = db::get_skill_by_id(pool, &final_skill_id)
        .await
        .map_err(GitHubImportError::plain)?;

    match (&selection.resolution, &existing_record) {
        (DuplicateResolution::Rename, Some(existing)) => {
            return Err(progress.blocked(
                format!("Renamed skill id '{}' is already in use.", final_skill_id),
                Some(candidate.source_path.as_str()),
                Some(final_skill_id.as_str()),
                Some(existing_instance_path(existing)),
            ));
        }
        (_, Some(existing)) if !existing.is_central => {
            return Err(progress.blocked(
                format!(
                    "Skill '{}' conflicts with a non-central record and cannot be overwritten safely.",
                    final_skill_id
                ),
                Some(candidate.source_path.as_str()),
                Some(final_skill_id.as_str()),
                Some(existing_instance_path(existing)),
            ));
        }
        _ => {}
    }

    // 같은 요청 안에서 두 스킬이 같은 대상 id를 차지하지 못하게 한다.
    if !reserved_ids.insert(final_skill_id.clone()) {
        return Err(progress.blocked(
            format!(
                "Target id '{}' is requested by more than one skill in this import; rename one of them.",
                final_skill_id
            ),
            Some(candidate.source_path.as_str()),
            Some(final_skill_id.as_str()),
            Some(target_dir),
        ));
    }

    let owned_target =
        match inspect_import_target(&target_dir, &existing_record, &selection.resolution) {
            Ok(owned_target) => owned_target,
            Err(message) => {
                return Err(progress.blocked(
                    message,
                    Some(candidate.source_path.as_str()),
                    Some(final_skill_id.as_str()),
                    Some(target_dir),
                ));
            }
        };

    Ok(StagedImport {
        candidate: candidate.clone(),
        final_skill_id,
        resolution: selection.resolution.clone(),
        target_dir,
        owned_target,
        source_files: Vec::new(),
    })
}

/// 대상 자리에 이미 있는 것이 중앙 레코드 소유인지 확인한다. 레코드가 소유하지
/// 않은 폴더와 심볼릭 링크(끊긴 링크 포함)는 절대 교체하지 않는다.
fn inspect_import_target(
    target_dir: &Path,
    existing_record: &Option<Skill>,
    resolution: &DuplicateResolution,
) -> Result<bool, String> {
    let metadata = match std::fs::symlink_metadata(target_dir) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(format!(
                "Failed to inspect import target '{}': {}",
                target_dir.display(),
                error
            ));
        }
    };

    if metadata.file_type().is_symlink() {
        return Err(format!(
            "Import target '{}' is a symbolic link that no skill record owns; rename the imported skill id.",
            target_dir.display()
        ));
    }
    if !metadata.is_dir() {
        return Err(format!(
            "Import target '{}' already exists but is not a directory; rename the imported skill id.",
            target_dir.display()
        ));
    }

    let owned = resolution == &DuplicateResolution::Overwrite
        && existing_record
            .as_ref()
            .is_some_and(|record| target_is_owned_by_record(record, target_dir));
    if !owned {
        return Err(format!(
            "Import target '{}' already exists but is not owned by a central skill record; rename the imported skill id.",
            target_dir.display()
        ));
    }
    Ok(true)
}

fn paths_equivalent(left: &str, right: &Path) -> bool {
    let left = Path::new(left.trim());
    let left = left.canonicalize().unwrap_or_else(|_| left.to_path_buf());
    let right = right.canonicalize().unwrap_or_else(|_| right.to_path_buf());
    left == right
}

fn existing_instance_path(skill: &Skill) -> PathBuf {
    skill
        .canonical_path
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(&skill.file_path))
}

/// 스테이징과 백업은 대상과 같은 파일 시스템의 숨김 컨테이너 안에 둔다. 컨테이너
/// 이름은 스캐너가 무시하는 기존 규칙(`.skillsmanage-stage-`, `.skillsmanage-old-`)을
/// 따르고, 컨테이너 안에 SKILL.md를 두지 않으므로 일반 스캔에도 스킬로 잡히지 않는다.
fn unique_staging_container(central_root: &Path, final_skill_id: &str, kind: &str) -> PathBuf {
    central_root.join(format!(
        ".{}.skillsmanage-{}-{}",
        final_skill_id,
        kind,
        uuid::Uuid::new_v4()
    ))
}

/// 백업해 둔 이전 대상을 제자리로 되돌린다. 되돌리지 못하면 백업을 지우지 않고
/// 원본이 남아 있는 경로를 알려 준다.
fn restore_previous_target(
    backup_container: &Option<PathBuf>,
    final_skill_id: &str,
    target_dir: &Path,
) -> Option<String> {
    let container = backup_container.as_ref()?;
    let preserved = container.join(final_skill_id);
    if std::fs::rename(&preserved, target_dir).is_ok() {
        remove_directory_if_present(container);
        return None;
    }
    Some(format!(
        "The previous skill '{}' is preserved at '{}'.",
        final_skill_id,
        preserved.display()
    ))
}

/// 새로 설치한 대상을 지운다. 지우지 못했으면 어디에 남아 있는지 알려 준다.
fn remove_imported_target(target_dir: &Path) -> Option<String> {
    if remove_directory_if_present(target_dir) {
        return None;
    }
    Some(format!(
        "The new files could not be removed from '{}'.",
        target_dir.display()
    ))
}

/// 폴더나 심볼릭 링크가 남아 있으면 지운다. 지우지 못했으면 false를 돌려준다.
fn remove_directory_if_present(path: &Path) -> bool {
    if std::fs::symlink_metadata(path).is_err() {
        return true;
    }
    std::fs::remove_dir_all(path).is_ok()
}

async fn central_skills_root(pool: &DbPool) -> Result<PathBuf, String> {
    let central = db::get_agent_by_id(pool, "central")
        .await?
        .ok_or_else(|| "Central agent not found in database".to_string())?;
    Ok(PathBuf::from(central.global_skills_dir))
}

async fn build_preview_skills(
    pool: &DbPool,
    central_root: &Path,
    candidates: &[RemoteSkillCandidate],
) -> Result<Vec<GitHubSkillPreview>, String> {
    let mut skills = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let target_dir = central_root.join(&candidate.skill_id);
        let existing = db::get_skill_by_id(pool, &candidate.skill_id).await?;
        let conflict = match existing {
            // 비중앙 레코드는 설치 전에 항상 막히므로 그대로 보고한다.
            Some(existing) if !existing.is_central => {
                Some(conflict_from_record(candidate, &existing))
            }
            Some(existing) => Some(central_target_conflict(candidate, &existing, &target_dir)),
            None => unmanaged_target_conflict(candidate, &target_dir),
        };

        skills.push(GitHubSkillPreview {
            source_path: candidate.source_path.clone(),
            skill_id: candidate.skill_id.clone(),
            skill_name: candidate.skill_name.clone(),
            description: candidate.description.clone(),
            root_directory: candidate.root_directory.clone(),
            skill_directory_name: candidate.skill_directory_name.clone(),
            download_url: candidate.download_url.clone(),
            conflict,
        });
    }
    Ok(skills)
}

/// 중앙 레코드가 있어도 실제 대상 자리까지 확인한다. 레코드가 소유하지 않은
/// 링크나 폴더가 자리를 차지하면 설치가 막히므로 미리보기도 같은 이유를 보고한다.
fn central_target_conflict(
    candidate: &RemoteSkillCandidate,
    existing: &Skill,
    target_dir: &Path,
) -> GitHubSkillConflict {
    if !existing_target_is_unsafe(target_dir, Some(existing)) {
        return conflict_from_record(candidate, existing);
    }
    GitHubSkillConflict {
        existing_skill_id: existing.id.clone(),
        existing_name: existing.name.clone(),
        existing_canonical_path: existing.canonical_path.clone(),
        proposed_skill_id: candidate.skill_id.clone(),
        proposed_name: candidate.skill_name.clone(),
        conflict_kind: GitHubSkillConflictKind::UnmanagedPath,
        existing_path: target_dir.to_string_lossy().into_owned(),
    }
}

/// 기존 레코드가 있으면 중앙/비중앙을 구분해 보고한다.
fn conflict_from_record(candidate: &RemoteSkillCandidate, existing: &Skill) -> GitHubSkillConflict {
    GitHubSkillConflict {
        existing_skill_id: existing.id.clone(),
        existing_name: existing.name.clone(),
        existing_canonical_path: existing.canonical_path.clone(),
        proposed_skill_id: candidate.skill_id.clone(),
        proposed_name: candidate.skill_name.clone(),
        conflict_kind: if existing.is_central {
            GitHubSkillConflictKind::Central
        } else {
            GitHubSkillConflictKind::NonCentral
        },
        existing_path: existing_instance_path(existing)
            .to_string_lossy()
            .into_owned(),
    }
}

/// 레코드가 없는 폴더나 심볼릭 링크가 대상 자리를 차지한 경우.
fn unmanaged_target_conflict(
    candidate: &RemoteSkillCandidate,
    target_dir: &Path,
) -> Option<GitHubSkillConflict> {
    std::fs::symlink_metadata(target_dir).ok()?;

    Some(GitHubSkillConflict {
        existing_skill_id: candidate.skill_id.clone(),
        existing_name: candidate.skill_directory_name.clone(),
        existing_canonical_path: None,
        proposed_skill_id: candidate.skill_id.clone(),
        proposed_name: candidate.skill_name.clone(),
        conflict_kind: GitHubSkillConflictKind::UnmanagedPath,
        existing_path: target_dir.to_string_lossy().into_owned(),
    })
}

/// 대상 자리에 있는 것이 교체 불가능한지 판단한다. 심볼릭 링크(끊긴 링크 포함)와
/// 파일, 그리고 어떤 중앙 레코드도 소유하지 않은 폴더는 교체 대상이 아니다.
/// 설치 전 검사와 미리보기가 같은 기준을 쓰도록 여기서만 판단한다.
fn existing_target_is_unsafe(target_dir: &Path, record: Option<&Skill>) -> bool {
    let metadata = match std::fs::symlink_metadata(target_dir) {
        Ok(metadata) => metadata,
        Err(_) => return false,
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return true;
    }
    !record.is_some_and(|record| target_is_owned_by_record(record, target_dir))
}

/// 중앙 레코드가 이 대상 폴더를 소유하는지 판단한다.
fn target_is_owned_by_record(record: &Skill, target_dir: &Path) -> bool {
    record.is_central
        && record
            .canonical_path
            .as_deref()
            .is_some_and(|path| paths_equivalent(path, target_dir))
}

pub(crate) async fn resolve_repo_ref(
    repo_url: &str,
    auth_token: Option<&str>,
) -> Result<GitHubRepoRef, String> {
    let (owner, repo) = parse_github_url(repo_url)?;
    let client = github_client()?;
    let response = send_github_request_with_fallback(
        &client,
        GitHubFetchSurface::Api,
        |endpoint| {
            github_endpoint_url(
                endpoint,
                GitHubFetchSurface::Api,
                &format!("/repos/{owner}/{repo}"),
            )
        },
        "Failed to inspect GitHub repository",
        auth_token,
    )
    .await?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("GitHub repository not found.".to_string());
    }
    if !response.status().is_success() {
        let status = response.status();
        return Err(
            classify_github_denial_response(response, "inspecting the repository")
                .await
                .unwrap_or_else(|| format!("Failed to inspect GitHub repository: HTTP {}", status)),
        );
    }

    let payload: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    let branch = payload
        .get("default_branch")
        .and_then(|v| v.as_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("main")
        .to_string();

    Ok(GitHubRepoRef {
        owner: owner.clone(),
        repo: repo.clone(),
        branch,
        normalized_url: format!("https://github.com/{owner}/{repo}"),
    })
}

/// 브랜치 끝을 불변 커밋으로 확정한다. 내려받기와 출처 기록에 함께 쓴다.
pub(crate) async fn resolve_repo_commit_oid(
    repo: &GitHubRepoRef,
    auth_token: Option<&str>,
) -> Result<String, String> {
    let client = github_client()?;
    let response = send_github_request_with_fallback(
        &client,
        GitHubFetchSurface::Api,
        |endpoint| {
            github_endpoint_url(
                endpoint,
                GitHubFetchSurface::Api,
                &format!("/repos/{}/{}/commits/{}", repo.owner, repo.repo, repo.branch),
            )
        },
        "Failed to resolve the repository commit",
        auth_token,
    )
    .await?;

    if !response.status().is_success() {
        return Err(format!(
            "GitHub ref lookup returned HTTP {}",
            response.status()
        ));
    }

    let payload: serde_json::Value = response.json().await.map_err(|e| e.to_string())?;
    payload
        .get("sha")
        .and_then(|value| value.as_str())
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "GitHub did not return a commit SHA".to_string())
}

// 토큰은 요청에만 사용하며 CLI에서 읽은 값은 저장하거나 프런트엔드로 보내지 않습니다.
struct GitHubAuth {
    token: Option<String>,
    source: &'static str,
}

fn github_cli_path() -> Option<PathBuf> {
    let name = if cfg!(windows) { "gh.exe" } else { "gh" };
    super::agents::executable_search_paths()
        .into_iter()
        .filter(|path| path.is_absolute())
        .map(|path| path.join(name))
        .find(|path| path.is_file())
}

async fn resolve_github_auth(pool: &DbPool, cli: Option<&Path>) -> Result<GitHubAuth, String> {
    let saved = db::get_setting(pool, GITHUB_PAT_SETTING_KEY)
        .await?
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty());
    if saved.is_some() {
        return Ok(GitHubAuth {
            token: saved,
            source: "saved_token",
        });
    }
    let Some(cli) = cli else {
        return Ok(GitHubAuth {
            token: None,
            source: "cli_missing",
        });
    };
    let mut command = tokio::process::Command::new(cli);
    command
        .args(["auth", "token", "--hostname", "github.com"])
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let token =
        match tokio::time::timeout(std::time::Duration::from_secs(5), command.output()).await {
            Ok(Ok(output)) if output.status.success() => String::from_utf8(output.stdout)
                .ok()
                .map(|token| token.trim().to_string())
                .filter(|token| !token.is_empty() && !token.contains(['\r', '\n'])),
            _ => None,
        };
    let source = if token.is_some() {
        "github_cli"
    } else {
        "not_signed_in"
    };
    Ok(GitHubAuth { token, source })
}

pub(crate) async fn github_direct_auth_from_settings(
    pool: &DbPool,
) -> Result<Option<String>, String> {
    Ok(resolve_github_auth(pool, github_cli_path().as_deref())
        .await?
        .token)
}

#[tauri::command]
pub async fn get_github_auth_status(state: State<'_, AppState>) -> Result<String, String> {
    Ok(resolve_github_auth(&state.db, github_cli_path().as_deref())
        .await?
        .source
        .to_string())
}

fn github_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent("skills-manage/0.9.1")
        .build()
        .map_err(|e| e.to_string())
}

fn parse_github_url(url: &str) -> Result<(String, String), String> {
    let trimmed = url.trim();
    let parsed =
        reqwest::Url::parse(trimmed).map_err(|_| "Invalid GitHub repository URL.".to_string())?;

    if parsed.scheme() != "https" {
        return Err("Only https:// GitHub repository URLs are supported.".to_string());
    }
    if parsed.host_str() != Some("github.com") {
        return Err("Only github.com repository URLs are supported.".to_string());
    }

    let mut segments = parsed
        .path_segments()
        .ok_or_else(|| "Invalid GitHub repository URL.".to_string())?;
    let owner = segments
        .next()
        .filter(|segment| !segment.is_empty())
        .ok_or_else(|| "GitHub repository URL must include an owner.".to_string())?;
    let repo = segments
        .next()
        .filter(|segment| !segment.is_empty())
        .ok_or_else(|| "GitHub repository URL must include a repository name.".to_string())?;

    let repo = repo.strip_suffix(".git").unwrap_or(repo);
    if owner.is_empty() || repo.is_empty() {
        return Err("GitHub repository URL is missing owner or repository.".to_string());
    }

    Ok((owner.to_lowercase(), repo.to_lowercase()))
}

pub(crate) async fn fetch_repo_skill_candidates(
    repo: &GitHubRepoRef,
    auth_token: Option<&str>,
) -> Result<Vec<RemoteSkillCandidate>, String> {
    let client = github_client()?;
    let snapshot = download_repo_snapshot(&client, repo, auth_token).await?;
    build_repo_skill_candidates_from_snapshot(repo, &snapshot)
}

pub(crate) fn build_repo_skill_candidates_from_snapshot(
    repo: &GitHubRepoRef,
    snapshot: &GitHubRepoSnapshot,
) -> Result<Vec<RemoteSkillCandidate>, String> {
    let direct_endpoint = GITHUB_MIRROR_ENDPOINTS.first().expect("github endpoint");
    let mut manifests = snapshot
        .files
        .keys()
        .filter_map(|path| classify_skill_manifest_path(path))
        .collect::<Vec<_>>();
    manifests.sort_by(|left, right| left.source_path.cmp(&right.source_path));

    let mut candidates = Vec::with_capacity(manifests.len());
    for manifest in manifests {
        let raw = snapshot
            .files
            .get(&manifest.skill_md_path)
            .ok_or_else(|| format!("Missing snapshot file '{}'.", manifest.skill_md_path))?;
        let content = String::from_utf8(raw.clone())
            .map_err(|_| format!("Skill '{}' is not valid UTF-8.", manifest.source_path))?;
        let frontmatter = parse_frontmatter(&content).ok_or_else(|| {
            if manifest.source_path == "." {
                "Repository root SKILL.md is missing valid frontmatter.".to_string()
            } else {
                format!(
                    "Skill '{}' is missing valid frontmatter.",
                    manifest.source_path
                )
            }
        })?;

        let skill_id = if manifest.source_path == "." {
            let repo_skill_id = sanitize_skill_id(&repo.repo)?;
            repo_skill_id
                .strip_suffix("-skill")
                .unwrap_or(&repo_skill_id)
                .to_string()
        } else {
            sanitize_skill_id(&manifest.skill_directory_name)?
        };

        candidates.push(RemoteSkillCandidate {
            source_path: manifest.source_path.clone(),
            skill_id,
            skill_name: frontmatter.name,
            description: frontmatter.description,
            root_directory: manifest.root_directory,
            skill_directory_name: if manifest.source_path == "." {
                repo.repo.clone()
            } else {
                manifest.skill_directory_name
            },
            download_url: raw_file_url(direct_endpoint, repo, &manifest.skill_md_path),
        });
    }

    Ok(candidates)
}

#[derive(Debug, Clone)]
struct SnapshotSkillManifest {
    source_path: String,
    root_directory: String,
    skill_directory_name: String,
    skill_md_path: String,
}

fn classify_skill_manifest_path(path: &str) -> Option<SnapshotSkillManifest> {
    let normalized = path.trim_matches('/');
    if normalized.is_empty() {
        return None;
    }

    if normalized.eq_ignore_ascii_case("SKILL.md") {
        return Some(SnapshotSkillManifest {
            source_path: ".".to_string(),
            root_directory: "/".to_string(),
            skill_directory_name: String::new(),
            skill_md_path: "SKILL.md".to_string(),
        });
    }

    let parts = normalized.split('/').collect::<Vec<_>>();
    let (skill_md, source_parts) = parts.split_last()?;
    if !skill_md.eq_ignore_ascii_case("SKILL.md") {
        return None;
    }

    match source_parts {
        [skill_dir] if *skill_dir != ".github" && *skill_dir != "skills" => {
            Some(SnapshotSkillManifest {
                source_path: (*skill_dir).to_string(),
                root_directory: "/".to_string(),
                skill_directory_name: (*skill_dir).to_string(),
                skill_md_path: normalized.to_string(),
            })
        }
        _ if source_parts.first() == Some(&"skills") && source_parts.len() >= 2 => {
            Some(SnapshotSkillManifest {
                source_path: source_parts.join("/"),
                root_directory: source_parts[..source_parts.len() - 1].join("/"),
                skill_directory_name: source_parts.last()?.to_string(),
                skill_md_path: normalized.to_string(),
            })
        }
        _ => None,
    }
}

pub(crate) async fn download_repo_snapshot(
    client: &reqwest::Client,
    repo: &GitHubRepoRef,
    auth_token: Option<&str>,
) -> Result<GitHubRepoSnapshot, String> {
    let archive = download_repository_archive(client, repo, auth_token).await?;
    snapshot_from_repository_archive(&archive)
}

async fn download_repository_archive(
    client: &reqwest::Client,
    repo: &GitHubRepoRef,
    auth_token: Option<&str>,
) -> Result<Vec<u8>, String> {
    let response = send_github_request_with_fallback(
        client,
        GitHubFetchSurface::Api,
        |endpoint| {
            github_endpoint_url(
                endpoint,
                GitHubFetchSurface::Api,
                &format!(
                    "/repos/{}/{}/tarball/{}",
                    repo.owner, repo.repo, repo.branch
                ),
            )
        },
        "Failed to download GitHub repository archive",
        auth_token,
    )
    .await?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err("GitHub repository archive is unavailable.".to_string());
    }
    if !response.status().is_success() {
        let status = response.status();
        return Err(classify_github_denial_response(
            response,
            "downloading the repository archive",
        )
        .await
        .unwrap_or_else(|| {
            format!(
                "Failed to download GitHub repository archive: HTTP {}",
                status
            )
        }));
    }

    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|e| format!("Failed to read GitHub repository archive: {}", e))
}

fn snapshot_from_repository_archive(archive_bytes: &[u8]) -> Result<GitHubRepoSnapshot, String> {
    let cursor = Cursor::new(archive_bytes);
    let decoder = GzDecoder::new(cursor);
    let mut archive = tar::Archive::new(decoder);
    let mut files = HashMap::new();

    for entry_result in archive
        .entries()
        .map_err(|e| format!("Failed to inspect GitHub repository archive: {}", e))?
    {
        let mut entry = entry_result
            .map_err(|e| format!("Failed to inspect GitHub repository archive: {}", e))?;

        if !entry.header().entry_type().is_file() {
            continue;
        }

        let relative_path = relative_archive_path(&entry)?;
        let mut content = Vec::new();
        entry.read_to_end(&mut content).map_err(|e| {
            format!(
                "Failed to read GitHub repository archive entry '{}': {}",
                relative_path, e
            )
        })?;
        files.insert(relative_path, content);
    }

    Ok(GitHubRepoSnapshot { files })
}

fn relative_archive_path<R: Read>(entry: &tar::Entry<'_, R>) -> Result<String, String> {
    let archive_path = entry
        .path()
        .map_err(|e| format!("Failed to inspect GitHub repository archive: {}", e))?;
    let relative = archive_path
        .components()
        .skip(1)
        .map(|component| match component {
            Component::Normal(value) => Ok(value.to_string_lossy().into_owned()),
            _ => Err("GitHub repository archive contains an unsupported path.".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()?;

    if relative.is_empty() {
        return Err("GitHub repository archive contains an unsupported path.".to_string());
    }

    let joined = relative.join("/");
    if !is_safe_repo_relative_path(&joined) {
        return Err(format!(
            "GitHub repository archive contains an unsupported path '{}'.",
            joined
        ));
    }

    Ok(joined)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SnapshotSourceFile {
    repo_path: String,
    relative_path: String,
    byte_len: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct GitHubImportProgressState {
    completed_files: usize,
    total_files: usize,
    completed_bytes: u64,
    total_bytes: u64,
}

pub(crate) fn collect_snapshot_source_files(
    snapshot: &GitHubRepoSnapshot,
    source_path: &str,
) -> Result<Vec<SnapshotSourceFile>, String> {
    let mut files = snapshot
        .files
        .iter()
        .filter_map(|(path, bytes)| {
            let relative_path = if source_path == "." {
                if path.contains('/') {
                    return None;
                }
                path.clone()
            } else {
                let prefix = format!("{}/", source_path.trim_matches('/'));
                let relative = path.strip_prefix(&prefix)?;
                if relative.is_empty() {
                    return None;
                }
                relative.to_string()
            };

            Some(SnapshotSourceFile {
                repo_path: path.clone(),
                relative_path,
                byte_len: bytes.len(),
            })
        })
        .collect::<Vec<_>>();

    files.sort_by(|left, right| left.repo_path.cmp(&right.repo_path));

    if files.is_empty() {
        return Err(format!(
            "Repository path '{}' is no longer available in the archive.",
            source_path
        ));
    }

    Ok(files)
}

pub(crate) fn write_snapshot_source_to_target(
    snapshot: &GitHubRepoSnapshot,
    files: &[SnapshotSourceFile],
    target_dir: &Path,
    source_path: &str,
    progress_state: &mut GitHubImportProgressState,
    app: Option<&AppHandle>,
) -> Result<(), String> {
    std::fs::create_dir_all(target_dir)
        .map_err(|e| format!("Failed to create import target directory: {}", e))?;

    for file in files {
        if !is_safe_repo_relative_path(&file.relative_path) {
            return Err(format!(
                "Repository contains an unsupported path '{}'.",
                file.repo_path
            ));
        }

        let bytes = snapshot.files.get(&file.repo_path).ok_or_else(|| {
            format!(
                "Repository file '{}' is no longer available in the archive.",
                file.repo_path
            )
        })?;

        let destination = target_dir.join(&file.relative_path);
        let parent = destination
            .parent()
            .ok_or_else(|| "Failed to determine imported file parent directory.".to_string())?;
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create imported file parent directory: {}", e))?;
        std::fs::write(&destination, bytes).map_err(|e| {
            format!(
                "Failed to write imported file '{}': {}",
                destination.display(),
                e
            )
        })?;

        progress_state.completed_files += 1;
        progress_state.completed_bytes += file.byte_len as u64;
        emit_github_import_progress(
            app,
            GitHubImportProgressPayload {
                phase: GitHubImportProgressPhase::Writing,
                current_skill: Some(source_path.to_string()),
                current_path: Some(file.relative_path.clone()),
                completed_files: progress_state.completed_files,
                total_files: progress_state.total_files,
                completed_bytes: progress_state.completed_bytes,
                total_bytes: progress_state.total_bytes,
            },
        );
    }

    Ok(())
}

fn emit_github_import_progress(app: Option<&AppHandle>, payload: GitHubImportProgressPayload) {
    if let Some(app) = app {
        let _ = app.emit("github-import:progress", payload);
    }
}

fn is_safe_repo_relative_path(path: &str) -> bool {
    let relative = Path::new(path);
    !relative.is_absolute()
        && relative
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

async fn fetch_raw_text(
    client: &reqwest::Client,
    url: &str,
    auth_token: Option<&str>,
) -> Result<String, String> {
    let response = send_github_request_with_fallback(
        client,
        GitHubFetchSurface::Raw,
        |endpoint| {
            if let Some(path) = raw_url_to_repo_path(url) {
                raw_file_url(endpoint, &path.repo, &path.file_path)
            } else {
                url.to_string()
            }
        },
        "Failed to download skill metadata",
        auth_token,
    )
    .await?;

    if !response.status().is_success() {
        return Err(
            classify_github_denial_response(response, "downloading skill metadata")
                .await
                .unwrap_or_else(|| "Failed to download skill metadata.".to_string()),
        );
    }

    response
        .text()
        .await
        .map_err(|e| format!("Failed to read skill metadata: {}", e))
}

#[derive(Debug, Clone)]
struct RawRepoPath {
    repo: GitHubRepoRef,
    file_path: String,
}

fn raw_url_to_repo_path(url: &str) -> Option<RawRepoPath> {
    let parsed = reqwest::Url::parse(url).ok()?;
    let host = parsed.host_str()?;
    if host != "raw.githubusercontent.com" {
        return None;
    }

    let segments = parsed.path_segments()?;
    let parts = segments.collect::<Vec<_>>();
    if parts.len() < 4 {
        return None;
    }

    Some(RawRepoPath {
        repo: GitHubRepoRef {
            owner: parts[0].to_string(),
            repo: parts[1].to_string(),
            branch: parts[2].to_string(),
            normalized_url: format!("https://github.com/{}/{}", parts[0], parts[1]),
        },
        file_path: parts[3..].join("/"),
    })
}

fn github_endpoint_url(
    endpoint: &GitHubMirrorEndpoint,
    surface: GitHubFetchSurface,
    path: &str,
) -> String {
    let base = match surface {
        GitHubFetchSurface::Api => endpoint.api_base,
        GitHubFetchSurface::Raw => endpoint.raw_base,
    };
    format!("{}{}", base.trim_end_matches('/'), path)
}

fn raw_file_url(endpoint: &GitHubMirrorEndpoint, repo: &GitHubRepoRef, file_path: &str) -> String {
    github_endpoint_url(
        endpoint,
        GitHubFetchSurface::Raw,
        &format!(
            "/{}/{}/{}/{}",
            repo.owner,
            repo.repo,
            repo.branch,
            file_path.trim_start_matches('/')
        ),
    )
}

async fn send_github_request_with_fallback<F>(
    client: &reqwest::Client,
    surface: GitHubFetchSurface,
    build_url: F,
    failure_prefix: &str,
    auth_token: Option<&str>,
) -> Result<reqwest::Response, String>
where
    F: Fn(&GitHubMirrorEndpoint) -> String,
{
    let mut attempts = Vec::new();
    let mut last_retryable_denial = None;

    for endpoint in GITHUB_MIRROR_ENDPOINTS {
        let url = build_url(endpoint);
        let mut request = client.get(url);
        if endpoint.label == "github" {
            if let Some(token) = auth_token {
                request = request.bearer_auth(token);
            }
        }
        match request.send().await {
            Ok(response) => {
                let status = response.status();
                if matches!(
                    status,
                    reqwest::StatusCode::UNAUTHORIZED
                        | reqwest::StatusCode::FORBIDDEN
                        | reqwest::StatusCode::TOO_MANY_REQUESTS
                ) {
                    let denial = parse_github_denial_response(response, "contacting GitHub").await;
                    let can_retry_public_mirror = auth_token.is_none()
                        && denial.as_ref().is_some_and(|denial| {
                            matches!(denial.kind, GitHubAccessDenialKind::RateLimited { .. })
                        });
                    if can_retry_public_mirror {
                        last_retryable_denial = denial;
                        attempts.push(MirrorAttemptOutcome {
                            status: Some(status),
                            error_message: format!(
                                "{} mirror '{}' returned HTTP {} due to rate limiting",
                                surface_label(surface),
                                endpoint.label,
                                status
                            ),
                        });
                        continue;
                    }

                    return Err(denial
                        .map(|denial| denial.to_string())
                        .unwrap_or_else(|| format!("{}: HTTP {}", failure_prefix, status)));
                }

                if status.is_success() {
                    return Ok(response);
                }

                if status == reqwest::StatusCode::NOT_FOUND {
                    if last_retryable_denial.is_some() && auth_token.is_none() {
                        attempts.push(MirrorAttemptOutcome {
                            status: Some(status),
                            error_message: format!(
                                "{} mirror '{}' returned HTTP 404 after a prior rate-limit denial",
                                surface_label(surface),
                                endpoint.label
                            ),
                        });
                        continue;
                    }
                    return Ok(response);
                }

                if should_retry_via_mirror_status(surface, status) {
                    attempts.push(MirrorAttemptOutcome {
                        status: Some(status),
                        error_message: format!(
                            "{} mirror '{}' returned HTTP {}",
                            surface_label(surface),
                            endpoint.label,
                            status
                        ),
                    });
                    continue;
                }

                return Err(format!("{}: HTTP {}", failure_prefix, status));
            }
            Err(error) => {
                if is_retryable_github_transport_error(&error) {
                    attempts.push(MirrorAttemptOutcome {
                        status: error.status(),
                        error_message: format!(
                            "{} mirror '{}' failed: {}",
                            surface_label(surface),
                            endpoint.label,
                            error
                        ),
                    });
                    continue;
                }

                return Err(format!("{}: {}", failure_prefix, error));
            }
        }
    }

    if let Some(denial) = last_retryable_denial {
        return Err(denial.to_string());
    }

    Err(format!(
        "{}. Direct GitHub access and built-in mirrors were unreachable. Retry later or try a different network path. Last errors: {}",
        failure_prefix,
        summarize_mirror_attempts(&attempts)
    ))
}

fn should_retry_via_mirror_status(
    surface: GitHubFetchSurface,
    status: reqwest::StatusCode,
) -> bool {
    match surface {
        GitHubFetchSurface::Api | GitHubFetchSurface::Raw => {
            status.is_server_error()
                || status == reqwest::StatusCode::BAD_GATEWAY
                || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
                || status == reqwest::StatusCode::GATEWAY_TIMEOUT
        }
    }
}

fn is_retryable_github_transport_error(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request() || error.is_body()
}

fn summarize_mirror_attempts(attempts: &[MirrorAttemptOutcome]) -> String {
    attempts
        .iter()
        .map(|attempt| attempt.error_message.clone())
        .collect::<Vec<_>>()
        .join("; ")
}

fn surface_label(surface: GitHubFetchSurface) -> &'static str {
    match surface {
        GitHubFetchSurface::Api => "API",
        GitHubFetchSurface::Raw => "raw",
    }
}

async fn classify_github_denial_response(
    response: reqwest::Response,
    operation: &'static str,
) -> Option<String> {
    parse_github_denial_response(response, operation)
        .await
        .map(|denial| denial.to_string())
}

async fn parse_github_denial_response(
    response: reqwest::Response,
    operation: &'static str,
) -> Option<GitHubAccessDenial> {
    let status = response.status();
    if status != reqwest::StatusCode::UNAUTHORIZED
        && status != reqwest::StatusCode::FORBIDDEN
        && status != reqwest::StatusCode::TOO_MANY_REQUESTS
    {
        return None;
    }

    let headers = response.headers().clone();
    let body = response.text().await.ok();
    let github_message = body.as_deref().and_then(parse_github_error_message);

    let remaining = header_value(&headers, "x-ratelimit-remaining");
    let reset_at = header_value(&headers, "x-ratelimit-reset")
        .as_deref()
        .and_then(parse_rate_limit_reset_epoch);

    let message_lower = github_message
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let remaining_is_zero = remaining.as_deref() == Some("0");
    let kind = if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || remaining_is_zero
        || message_lower.contains("rate limit")
        || message_lower.contains("api rate limit exceeded")
        || header_value(&headers, "x-ratelimit-resource").is_some()
    {
        GitHubAccessDenialKind::RateLimited {
            reset_at,
            remaining,
        }
    } else {
        GitHubAccessDenialKind::AuthenticationOrPermission
    };

    Some(GitHubAccessDenial {
        kind,
        operation,
        status,
        github_message,
    })
}

fn parse_github_error_message(body: &str) -> Option<String> {
    serde_json::from_str::<GitHubErrorResponse>(body)
        .ok()
        .and_then(|payload| payload.message)
}

fn header_value(headers: &reqwest::header::HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn parse_rate_limit_reset_epoch(raw: &str) -> Option<String> {
    let epoch = raw.parse::<i64>().ok()?;
    chrono::DateTime::<Utc>::from_timestamp(epoch, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
}

pub(crate) fn parse_frontmatter(content: &str) -> Option<SkillFrontmatter> {
    let trimmed = content.trim();
    if !trimmed.starts_with("---") {
        return None;
    }
    let rest = &trimmed[3..];
    let end = rest.find("---")?;
    serde_yaml::from_str::<SkillFrontmatter>(&rest[..end]).ok()
}

fn sanitize_skill_id(raw: &str) -> Result<String, String> {
    let lowered = raw.trim().to_lowercase();
    let mut sanitized = String::new();
    let mut last_was_dash = false;
    for ch in lowered.chars() {
        if ch.is_ascii_alphanumeric() {
            sanitized.push(ch);
            last_was_dash = false;
        } else if !last_was_dash {
            sanitized.push('-');
            last_was_dash = true;
        }
    }
    let sanitized = sanitized.trim_matches('-').to_string();
    if sanitized.is_empty() {
        return Err(format!("Skill identifier '{}' is not supported.", raw));
    }
    Ok(sanitized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::{write::GzEncoder, Compression};
    use std::collections::HashMap;
    use sqlx::Row;
    use tempfile::tempdir;

    fn import_repo() -> GitHubRepoRef {
        GitHubRepoRef {
            owner: "anthropics".to_string(),
            repo: "skills".to_string(),
            branch: "main".to_string(),
            normalized_url: "https://github.com/anthropics/skills".to_string(),
        }
    }

    /// 중앙 스킬 폴더를 임시 디렉터리로 돌린다.
    async fn setup_central_dir(pool: &DbPool) -> tempfile::TempDir {
        let dir = tempdir().expect("central");
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'central'")
            .bind(dir.path().to_string_lossy().into_owned())
            .execute(pool)
            .await
            .expect("update central");
        dir
    }

    fn import_selection(
        source_path: &str,
        resolution: DuplicateResolution,
    ) -> GitHubSkillImportSelection {
        GitHubSkillImportSelection {
            source_path: source_path.to_string(),
            resolution,
            renamed_skill_id: None,
        }
    }

    fn rename_import_selection(
        source_path: &str,
        renamed_skill_id: &str,
    ) -> GitHubSkillImportSelection {
        GitHubSkillImportSelection {
            source_path: source_path.to_string(),
            resolution: DuplicateResolution::Rename,
            renamed_skill_id: Some(renamed_skill_id.to_string()),
        }
    }

    /// 레코드와 실제 폴더를 함께 만든다.
    async fn seed_skill_record(pool: &DbPool, id: &str, dir: &Path, is_central: bool, body: &str) {
        std::fs::create_dir_all(dir).expect("mkdir");
        std::fs::write(dir.join("SKILL.md"), body).expect("write skill");
        db::upsert_skill(
            pool,
            &Skill {
                id: id.to_string(),
                name: id.to_string(),
                description: Some("existing".to_string()),
                file_path: dir.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: Some(dir.to_string_lossy().into_owned()),
                is_central,
                source: Some("local".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .expect("upsert skill");
    }

    /// 네트워크 없이 실제 설치 경로를 실행한다.
    async fn install_snapshot(
        pool: &DbPool,
        snapshot: GitHubRepoSnapshot,
        selections: Vec<GitHubSkillImportSelection>,
    ) -> Result<GitHubRepoImportResult, GitHubImportError> {
        let repo = import_repo();
        let candidates = build_repo_skill_candidates_from_snapshot(&repo, &snapshot)
            .expect("build candidates");
        install_repo_skills_from_snapshot(
            pool,
            RepoSnapshotImportRequest {
                repo,
                commit_oid: None,
                snapshot,
                candidates,
                selections,
            },
            None,
        )
        .await
    }

    fn import_failure(error: GitHubImportError) -> GitHubImportFailure {
        match error {
            GitHubImportError::Failure(failure) => *failure,
            GitHubImportError::Message(message) => {
                panic!("expected structured failure, got message: {message}")
            }
        }
    }

    fn import_error_message(error: &GitHubImportError) -> String {
        match error {
            GitHubImportError::Message(message) => message.clone(),
            GitHubImportError::Failure(failure) => failure.message.clone(),
        }
    }

    fn central_entries(root: &Path) -> Vec<String> {
        let mut entries = std::fs::read_dir(root)
            .expect("read central")
            .map(|entry| entry.expect("entry").file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        entries.sort();
        entries
    }

    async fn setup_test_db() -> DbPool {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("github-import.sqlite");
        let pool = db::create_pool(db_path.to_str().unwrap())
            .await
            .expect("create db");
        db::init_database(&pool).await.expect("init db");
        std::mem::forget(dir);
        pool
    }

    fn sample_frontmatter(name: &str, description: &str) -> String {
        format!("---\nname: {name}\ndescription: {description}\n---\n\n# {name}\n")
    }

    fn repo_snapshot(files: &[(&str, String)]) -> GitHubRepoSnapshot {
        GitHubRepoSnapshot {
            files: files
                .iter()
                .map(|(path, content)| (path.to_string(), content.as_bytes().to_vec()))
                .collect::<HashMap<_, _>>(),
        }
    }

    fn root_repo_snapshot() -> GitHubRepoSnapshot {
        repo_snapshot(&[
            (
                "SKILL.md",
                sample_frontmatter("twitterapi-io", "root skill"),
            ),
            ("README.md", "# repo\n".to_string()),
        ])
    }

    fn multi_skill_snapshot() -> GitHubRepoSnapshot {
        repo_snapshot(&[
            (
                "skills/agent-planner/SKILL.md",
                sample_frontmatter("Agent Planner", "Agent Planner description"),
            ),
            (
                "skills/commit/SKILL.md",
                sample_frontmatter("Commit", "Commit description"),
            ),
            (
                "skills/code-review/SKILL.md",
                sample_frontmatter("Code Review", "Code Review description"),
            ),
            ("skills/commit/README.md", "# commit\n".to_string()),
        ])
    }

    fn namespaced_skill_snapshot() -> GitHubRepoSnapshot {
        repo_snapshot(&[
            (
                "skills/.curated/openai-docs/SKILL.md",
                sample_frontmatter("openai-docs", "OpenAI docs skill"),
            ),
            (
                "skills/.curated/openai-docs/references/api.md",
                "# api\n".to_string(),
            ),
            (
                "skills/.system/skill-creator/SKILL.md",
                sample_frontmatter("skill-creator", "Create skills"),
            ),
            (
                "skills/.system/skill-creator/scripts/init_skill.py",
                "print('hi')\n".to_string(),
            ),
        ])
    }

    fn repository_archive(files: &[(&str, &[u8])]) -> Vec<u8> {
        let encoder = GzEncoder::new(Vec::new(), Compression::default());
        let mut builder = tar::Builder::new(encoder);
        for (path, content) in files {
            let archive_path = format!("repo-snapshot/{}", path);
            let mut header = tar::Header::new_gnu();
            header.set_size(content.len() as u64);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Regular);
            header.set_cksum();
            builder
                .append_data(&mut header, archive_path, *content)
                .expect("append archive entry");
        }
        let encoder = builder.into_inner().expect("finalize tar");
        encoder.finish().expect("finalize gzip")
    }

    #[test]
    fn parse_github_url_normalizes_owner_and_repo() {
        let (owner, repo) =
            parse_github_url("https://github.com/Anthropics/Skills/").expect("parse");
        assert_eq!(owner, "anthropics");
        assert_eq!(repo, "skills");
    }

    #[test]
    fn parse_github_url_rejects_non_github_hosts() {
        let error = parse_github_url("https://gitlab.com/example/repo").unwrap_err();
        assert!(error.contains("github.com"));
    }

    #[test]
    fn sanitize_skill_id_collapses_symbols() {
        let skill_id = sanitize_skill_id("My Cool_Skill!").expect("sanitize");
        assert_eq!(skill_id, "my-cool-skill");
    }

    #[test]
    fn parse_frontmatter_requires_yaml_block() {
        assert!(parse_frontmatter("# nope").is_none());
        let parsed = parse_frontmatter(&sample_frontmatter("alpha", "desc")).expect("fm");
        assert_eq!(parsed.name, "alpha");
        assert_eq!(parsed.description.as_deref(), Some("desc"));
    }

    #[test]
    fn classify_github_rate_limit_denial_returns_actionable_message() {
        let denial = GitHubAccessDenial {
            kind: GitHubAccessDenialKind::RateLimited {
                reset_at: Some("2026-04-17 12:34:56".to_string()),
                remaining: Some("0".to_string()),
            },
            operation: "inspecting the repository",
            status: reqwest::StatusCode::FORBIDDEN,
            github_message: Some("API rate limit exceeded for 1.2.3.4.".to_string()),
        };

        let message = denial.to_string();

        assert!(message.contains("rate limit was exceeded"));
        assert!(message.contains("Retry later after 2026-04-17 12:34:56 UTC"));
        assert!(message.contains("authenticated GitHub requests"));
        assert!(message.contains("API rate limit exceeded"));
    }

    #[test]
    fn classify_github_permission_denial_returns_actionable_message() {
        let denial = GitHubAccessDenial {
            kind: GitHubAccessDenialKind::AuthenticationOrPermission,
            operation: "reading repository contents",
            status: reqwest::StatusCode::UNAUTHORIZED,
            github_message: Some("Requires authentication".to_string()),
        };

        let message = denial.to_string();

        assert!(message.contains("denied access"));
        assert!(message.contains("require authentication"));
        assert!(message.contains("token/permissions are insufficient"));
        assert!(message.contains("Requires authentication"));
    }

    #[test]
    fn raw_url_to_repo_path_parses_github_raw_urls() {
        let parsed = raw_url_to_repo_path(
            "https://raw.githubusercontent.com/owner/repo/main/skills/demo/SKILL.md",
        )
        .expect("parsed");

        assert_eq!(parsed.repo.owner, "owner");
        assert_eq!(parsed.repo.repo, "repo");
        assert_eq!(parsed.repo.branch, "main");
        assert_eq!(parsed.file_path, "skills/demo/SKILL.md");
    }

    #[test]
    fn raw_url_to_repo_path_ignores_non_github_raw_hosts() {
        assert!(raw_url_to_repo_path("https://example.com/file.txt").is_none());
    }

    #[test]
    fn mirror_status_retry_excludes_auth_denials() {
        assert!(should_retry_via_mirror_status(
            GitHubFetchSurface::Api,
            reqwest::StatusCode::BAD_GATEWAY
        ));
        assert!(!should_retry_via_mirror_status(
            GitHubFetchSurface::Api,
            reqwest::StatusCode::FORBIDDEN
        ));
        assert!(!should_retry_via_mirror_status(
            GitHubFetchSurface::Raw,
            reqwest::StatusCode::TOO_MANY_REQUESTS
        ));
    }

    #[test]
    fn summarize_mirror_attempts_reports_all_failures() {
        let message = summarize_mirror_attempts(&[
            MirrorAttemptOutcome {
                status: None,
                error_message: "API mirror 'github' failed: timeout".to_string(),
            },
            MirrorAttemptOutcome {
                status: Some(reqwest::StatusCode::BAD_GATEWAY),
                error_message: "API mirror 'ghfast' returned HTTP 502".to_string(),
            },
        ]);

        assert!(message.contains("timeout"));
        assert!(message.contains("HTTP 502"));
    }

    #[test]
    fn snapshot_from_repository_archive_strips_archive_root_directory() {
        let archive = repository_archive(&[
            (
                "skills/demo/SKILL.md",
                sample_frontmatter("Demo", "Archive demo").as_bytes(),
            ),
            ("README.md", b"# readme\n"),
        ]);

        let snapshot = snapshot_from_repository_archive(&archive).expect("snapshot");

        assert!(snapshot.files.contains_key("skills/demo/SKILL.md"));
        assert!(snapshot.files.contains_key("README.md"));
    }

    #[tokio::test]
    async fn preview_marks_canonical_conflicts_without_writing() {
        let pool = setup_test_db().await;
        let central_root = tempdir().expect("central");
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'central'")
            .bind(central_root.path().to_string_lossy().into_owned())
            .execute(&pool)
            .await
            .expect("update central");

        let existing_dir = central_root.path().join("twitterapi-io");
        std::fs::create_dir_all(&existing_dir).expect("mkdir");
        std::fs::write(
            existing_dir.join("SKILL.md"),
            sample_frontmatter("twitterapi-io", "existing"),
        )
        .expect("write skill");

        db::upsert_skill(
            &pool,
            &Skill {
                id: "twitterapi-io".to_string(),
                name: "twitterapi-io".to_string(),
                description: Some("existing".to_string()),
                file_path: existing_dir.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: Some(existing_dir.to_string_lossy().into_owned()),
                is_central: true,
                source: Some("local".to_string()),
                content: None,
                scanned_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .expect("upsert skill");

        let repo = GitHubRepoRef {
            owner: "dorukardahan".to_string(),
            repo: "twitterapi-io-skill".to_string(),
            branch: "main".to_string(),
            normalized_url: "https://github.com/dorukardahan/twitterapi-io-skill".to_string(),
        };
        let candidates = build_repo_skill_candidates_from_snapshot(&repo, &root_repo_snapshot())
            .expect("candidates");
        let preview = GitHubRepoPreview {
            repo,
            skills: build_preview_skills(&pool, central_root.path(), &candidates)
                .await
                .expect("preview skills"),
        };

        assert!(!preview.skills.is_empty());
        let conflict = preview
            .skills
            .iter()
            .find(|skill| skill.skill_id == "twitterapi-io")
            .and_then(|skill| skill.conflict.clone())
            .expect("conflict");
        assert_eq!(conflict.existing_skill_id, "twitterapi-io");
        assert_eq!(conflict.conflict_kind, GitHubSkillConflictKind::Central);
        assert_eq!(
            conflict.existing_path,
            existing_dir.to_string_lossy().into_owned()
        );
        assert_eq!(
            conflict.existing_canonical_path.as_deref(),
            Some(existing_dir.to_string_lossy().into_owned().as_str())
        );

        let central_entries = std::fs::read_dir(central_root.path())
            .expect("read dir")
            .count();
        assert_eq!(central_entries, 1, "preview should not write to central");
    }

    #[tokio::test]
    async fn import_repo_skills_honors_skip_rename_and_overwrite() {
        let pool = setup_test_db().await;
        let central_root = setup_central_dir(&pool).await;

        let planner_dir = central_root.path().join("agent-planner");
        let planner_original = sample_frontmatter("Agent Planner", "original planner");
        seed_skill_record(&pool, "agent-planner", &planner_dir, true, &planner_original).await;

        let commit_dir = central_root.path().join("commit");
        let commit_original = sample_frontmatter("Commit", "original commit");
        seed_skill_record(&pool, "commit", &commit_dir, true, &commit_original).await;

        let review_dir = central_root.path().join("code-review");
        let review_original = sample_frontmatter("Code Review", "original review");
        seed_skill_record(&pool, "code-review", &review_dir, true, &review_original).await;

        let snapshot = multi_skill_snapshot();
        let result = install_snapshot(
            &pool,
            snapshot.clone(),
            vec![
                rename_import_selection("skills/agent-planner", "agent-planner-imported"),
                import_selection("skills/commit", DuplicateResolution::Skip),
                import_selection("skills/code-review", DuplicateResolution::Overwrite),
            ],
        )
        .await
        .expect("import from snapshot");

        let imported_ids = result
            .imported_skills
            .iter()
            .map(|summary| summary.imported_skill_id.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            imported_ids,
            vec![
                "agent-planner-imported".to_string(),
                "code-review".to_string()
            ]
        );
        assert_eq!(result.skipped_skills, vec!["skills/commit".to_string()]);

        // 건너뛴 스킬과 이름을 바꾼 원본은 그대로 남는다.
        assert_eq!(
            std::fs::read_to_string(commit_dir.join("SKILL.md")).expect("read skipped"),
            commit_original
        );
        assert_eq!(
            std::fs::read_to_string(planner_dir.join("SKILL.md")).expect("read renamed source"),
            planner_original
        );
        // 덮어쓴 중앙 스킬은 저장소 내용으로 교체된다.
        let review_bytes = snapshot
            .files
            .get("skills/code-review/SKILL.md")
            .expect("review bytes");
        assert_eq!(
            std::fs::read(review_dir.join("SKILL.md"))
                .expect("read overwritten")
                .as_slice(),
            review_bytes.as_slice()
        );
        assert_eq!(
            central_entries(central_root.path()),
            vec![
                "agent-planner".to_string(),
                "agent-planner-imported".to_string(),
                "code-review".to_string(),
                "commit".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn blocked_import_leaves_central_storage_unchanged() {
        let pool = setup_test_db().await;
        let central_root = setup_central_dir(&pool).await;

        let error = install_snapshot(
            &pool,
            multi_skill_snapshot(),
            vec![import_selection(
                "skills/missing-skill",
                DuplicateResolution::Overwrite,
            )],
        )
        .await
        .expect_err("unknown selection must be blocked");
        let failure = import_failure(error);
        assert_eq!(failure.code, GitHubImportFailureCode::Blocked);
        assert_eq!(failure.source_path.as_deref(), Some("skills/missing-skill"));

        assert!(central_entries(central_root.path()).is_empty());
        let central_skills = db::get_central_skills(&pool).await.expect("central skills");
        assert!(central_skills.is_empty());
    }

    #[tokio::test]
    async fn denied_import_selection_performs_no_writes_or_db_mutations() {
        let pool = setup_test_db().await;
        let central_root = tempdir().expect("central");
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'central'")
            .bind(central_root.path().to_string_lossy().into_owned())
            .execute(&pool)
            .await
            .expect("update central");

        let before_skills = db::get_central_skills(&pool).await.expect("before skills");
        let before_entries = std::fs::read_dir(central_root.path())
            .expect("read central before")
            .count();

        let result = import_github_repo_skills_impl(
            &pool,
            "https://github.com/example/restricted-repo",
            vec![GitHubSkillImportSelection {
                source_path: "skills/private-skill".to_string(),
                resolution: DuplicateResolution::Overwrite,
                renamed_skill_id: None,
            }],
            None,
        )
        .await;

        let error = result.expect_err("denied import should fail");
        assert!(
            !import_error_message(&error).is_empty(),
            "failure should return an error message"
        );

        let after_skills = db::get_central_skills(&pool).await.expect("after skills");
        let after_entries = std::fs::read_dir(central_root.path())
            .expect("read central after")
            .count();
        assert_eq!(
            before_entries, after_entries,
            "denied import should not write files"
        );
        assert_eq!(
            before_skills.len(),
            after_skills.len(),
            "denied import should not mutate DB"
        );
    }

    #[tokio::test]
    async fn preview_flags_non_central_and_unmanaged_targets() {
        let pool = setup_test_db().await;
        let central_root = setup_central_dir(&pool).await;

        let external_dir = tempdir().expect("external");
        seed_skill_record(
            &pool,
            "commit",
            external_dir.path(),
            false,
            &sample_frontmatter("Commit", "existing"),
        )
        .await;

        let unmanaged_dir = central_root.path().join("agent-planner");
        std::fs::create_dir_all(&unmanaged_dir).expect("mkdir");
        std::fs::write(unmanaged_dir.join("SKILL.md"), "unmanaged").expect("write");

        let repo = import_repo();
        let candidates = build_repo_skill_candidates_from_snapshot(&repo, &multi_skill_snapshot())
            .expect("candidates");
        let preview = build_preview_skills(&pool, central_root.path(), &candidates)
            .await
            .expect("preview");

        let commit = preview
            .iter()
            .find(|skill| skill.skill_id == "commit")
            .expect("commit preview");
        let conflict = commit.conflict.as_ref().expect("non-central conflict");
        assert_eq!(conflict.conflict_kind, GitHubSkillConflictKind::NonCentral);
        assert_eq!(conflict.existing_skill_id, "commit");
        assert_eq!(
            conflict.existing_path,
            external_dir.path().to_string_lossy().into_owned()
        );

        let planner = preview
            .iter()
            .find(|skill| skill.skill_id == "agent-planner")
            .expect("planner preview");
        let conflict = planner.conflict.as_ref().expect("unmanaged conflict");
        assert_eq!(
            conflict.conflict_kind,
            GitHubSkillConflictKind::UnmanagedPath
        );
        assert_eq!(
            conflict.existing_path,
            unmanaged_dir.to_string_lossy().into_owned()
        );
    }

    #[tokio::test]
    async fn preview_marks_unsafe_central_target_as_unmanaged_path() {
        let pool = setup_test_db().await;
        let central_root = setup_central_dir(&pool).await;
        let repo = import_repo();
        let candidates = build_repo_skill_candidates_from_snapshot(&repo, &multi_skill_snapshot())
            .expect("candidates");

        // 레코드가 소유한 폴더는 중앙 충돌로 보고한다.
        let owned_dir = central_root.path().join("agent-planner");
        seed_skill_record(
            &pool,
            "agent-planner",
            &owned_dir,
            true,
            &sample_frontmatter("Agent Planner", "existing"),
        )
        .await;
        let preview = build_preview_skills(&pool, central_root.path(), &candidates)
            .await
            .expect("preview");
        let conflict = preview
            .iter()
            .find(|skill| skill.skill_id == "agent-planner")
            .and_then(|skill| skill.conflict.clone())
            .expect("owned conflict");
        assert_eq!(conflict.conflict_kind, GitHubSkillConflictKind::Central);
        assert_eq!(
            conflict.existing_path,
            owned_dir.to_string_lossy().into_owned()
        );

        // 중앙 레코드가 있어도 실제 대상이 그 레코드 소유가 아니면 교체 불가로 보고한다.
        let external_dir = tempdir().expect("external");
        seed_skill_record(
            &pool,
            "commit",
            external_dir.path(),
            true,
            &sample_frontmatter("Commit", "existing"),
        )
        .await;
        let mismatched_dir = central_root.path().join("commit");
        std::fs::create_dir_all(&mismatched_dir).expect("mkdir");
        std::fs::write(mismatched_dir.join("SKILL.md"), "mismatched").expect("write");

        let preview = build_preview_skills(&pool, central_root.path(), &candidates)
            .await
            .expect("preview");
        let conflict = preview
            .iter()
            .find(|skill| skill.skill_id == "commit")
            .and_then(|skill| skill.conflict.clone())
            .expect("mismatched conflict");
        assert_eq!(
            conflict.conflict_kind,
            GitHubSkillConflictKind::UnmanagedPath
        );
        assert_eq!(conflict.existing_skill_id, "commit");
        assert_eq!(
            conflict.existing_path,
            mismatched_dir.to_string_lossy().into_owned()
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn preview_marks_symlinked_central_target_as_unmanaged_path() {
        let pool = setup_test_db().await;
        let central_root = setup_central_dir(&pool).await;
        let external_dir = tempdir().expect("external");
        seed_skill_record(
            &pool,
            "agent-planner",
            external_dir.path(),
            true,
            &sample_frontmatter("Agent Planner", "existing"),
        )
        .await;
        let linked_dir = central_root.path().join("agent-planner");
        std::os::unix::fs::symlink(external_dir.path(), &linked_dir).expect("symlink");

        let repo = import_repo();
        let candidates = build_repo_skill_candidates_from_snapshot(&repo, &multi_skill_snapshot())
            .expect("candidates");
        let preview = build_preview_skills(&pool, central_root.path(), &candidates)
            .await
            .expect("preview");
        let conflict = preview
            .iter()
            .find(|skill| skill.skill_id == "agent-planner")
            .and_then(|skill| skill.conflict.clone())
            .expect("symlink conflict");
        assert_eq!(
            conflict.conflict_kind,
            GitHubSkillConflictKind::UnmanagedPath
        );
        assert_eq!(
            conflict.existing_path,
            linked_dir.to_string_lossy().into_owned()
        );
    }

    #[test]
    fn restore_previous_target_reports_preserved_backup_when_target_is_occupied() {
        let root = tempdir().expect("root");
        let container = root.path().join(".planner.skillsmanage-old-test");
        let backup_dir = container.join("planner");
        std::fs::create_dir_all(&backup_dir).expect("mkdir backup");
        std::fs::write(backup_dir.join("SKILL.md"), "original").expect("write backup");

        // 대상 자리가 막혀 있으면 되돌리지 못하고 백업이 남아 있는 경로를 알려 준다.
        let target_dir = root.path().join("planner");
        std::fs::write(&target_dir, "blocking file").expect("write blocker");
        let note = restore_previous_target(&Some(container.clone()), "planner", &target_dir)
            .expect("recovery note");
        assert!(note.contains("preserved at"));
        assert!(note.contains(&backup_dir.to_string_lossy().into_owned()));
        assert_eq!(
            std::fs::read_to_string(backup_dir.join("SKILL.md")).expect("backup kept"),
            "original"
        );

        // 대상 자리가 비어 있으면 제자리로 되돌리고 백업 컨테이너를 지운다.
        std::fs::remove_file(&target_dir).expect("remove blocker");
        assert!(
            restore_previous_target(&Some(container.clone()), "planner", &target_dir).is_none()
        );
        assert_eq!(
            std::fs::read_to_string(target_dir.join("SKILL.md")).expect("restored"),
            "original"
        );
        assert!(std::fs::symlink_metadata(&container).is_err());
        assert!(restore_previous_target(&None, "planner", &target_dir).is_none());
    }

    #[test]
    fn remove_imported_target_reports_when_the_new_files_stay() {
        let root = tempdir().expect("root");
        let missing_dir = root.path().join("planner");
        assert!(remove_imported_target(&missing_dir).is_none());

        std::fs::write(&missing_dir, "undeletable").expect("write file");
        let note = remove_imported_target(&missing_dir).expect("cleanup note");
        assert!(note.contains("could not be removed"));
        assert!(note.contains(&missing_dir.to_string_lossy().into_owned()));
    }

    #[tokio::test]
    async fn import_blocks_non_central_overwrite_before_writing() {
        let pool = setup_test_db().await;
        let central_root = setup_central_dir(&pool).await;
        let external_dir = tempdir().expect("external");
        let original = sample_frontmatter("Commit", "original");
        seed_skill_record(&pool, "commit", external_dir.path(), false, &original).await;

        let error = install_snapshot(
            &pool,
            multi_skill_snapshot(),
            vec![import_selection(
                "skills/commit",
                DuplicateResolution::Overwrite,
            )],
        )
        .await
        .expect_err("non-central overwrite must be blocked");
        let failure = import_failure(error);
        assert_eq!(failure.code, GitHubImportFailureCode::Blocked);
        assert_eq!(failure.source_path.as_deref(), Some("skills/commit"));
        assert_eq!(failure.skill_id.as_deref(), Some("commit"));
        assert_eq!(
            failure.existing_path.as_deref(),
            external_dir.path().to_str()
        );
        assert!(failure.imported_skills.is_empty());
        assert!(failure.skipped_skills.is_empty());
        assert!(failure.message.contains("non-central"));

        assert_eq!(
            std::fs::read_to_string(external_dir.path().join("SKILL.md")).expect("read original"),
            original
        );
        assert!(central_entries(central_root.path()).is_empty());
        let record = db::get_skill_by_id(&pool, "commit")
            .await
            .expect("load record")
            .expect("commit record");
        assert!(!record.is_central);
    }

    #[tokio::test]
    async fn import_blocks_rename_onto_non_central_id() {
        let pool = setup_test_db().await;
        let central_root = setup_central_dir(&pool).await;
        let external_dir = tempdir().expect("external");
        seed_skill_record(
            &pool,
            "commit",
            external_dir.path(),
            false,
            &sample_frontmatter("Commit", "original"),
        )
        .await;

        let error = install_snapshot(
            &pool,
            multi_skill_snapshot(),
            vec![rename_import_selection("skills/agent-planner", "commit")],
        )
        .await
        .expect_err("rename onto a non-central id must be blocked");
        let failure = import_failure(error);
        assert_eq!(failure.code, GitHubImportFailureCode::Blocked);
        assert_eq!(failure.skill_id.as_deref(), Some("commit"));
        assert!(failure.message.contains("already in use"));

        assert!(central_entries(central_root.path()).is_empty());
        assert!(db::get_skill_by_id(&pool, "agent-planner")
            .await
            .expect("load record")
            .is_none());
        assert!(db::get_skill_by_id(&pool, "commit")
            .await
            .expect("load record")
            .is_some());
    }

    #[tokio::test]
    async fn import_unique_rename_keeps_existing_files_records_and_installations() {
        let pool = setup_test_db().await;
        let central_root = setup_central_dir(&pool).await;
        let existing_dir = central_root.path().join("agent-planner");
        let original = sample_frontmatter("Agent Planner", "original");
        seed_skill_record(&pool, "agent-planner", &existing_dir, true, &original).await;
        db::upsert_skill_installation(
            &pool,
            &db::SkillInstallation {
                skill_id: "agent-planner".to_string(),
                agent_id: "claude".to_string(),
                installed_path: "/tmp/claude/agent-planner".to_string(),
                link_type: "symlink".to_string(),
                symlink_target: Some(existing_dir.to_string_lossy().into_owned()),
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .expect("seed installation");

        let snapshot = multi_skill_snapshot();
        let result = install_snapshot(
            &pool,
            snapshot.clone(),
            vec![rename_import_selection(
                "skills/agent-planner",
                "Agent Planner Imported",
            )],
        )
        .await
        .expect("unique rename must import");

        assert_eq!(result.imported_skills.len(), 1);
        let summary = &result.imported_skills[0];
        assert_eq!(summary.imported_skill_id, "agent-planner-imported");
        assert_eq!(summary.original_skill_id, "agent-planner");
        assert_eq!(summary.resolution, DuplicateResolution::Rename);
        assert!(result.skipped_skills.is_empty());

        // 기존 파일, 레코드, 설치 연결은 그대로 남는다.
        assert_eq!(
            std::fs::read_to_string(existing_dir.join("SKILL.md")).expect("read original"),
            original
        );
        let existing_record = db::get_skill_by_id(&pool, "agent-planner")
            .await
            .expect("load record")
            .expect("existing record");
        assert!(existing_record.is_central);
        assert_eq!(
            existing_record.canonical_path.as_deref(),
            Some(existing_dir.to_string_lossy().into_owned().as_str())
        );
        let installations = db::get_skill_installations(&pool, "agent-planner")
            .await
            .expect("load installations");
        assert_eq!(installations.len(), 1);
        assert_eq!(installations[0].agent_id, "claude");

        // 새 스킬은 저장소 원본 바이트를 그대로 쓴다.
        let new_dir = central_root.path().join("agent-planner-imported");
        let repo_bytes = snapshot
            .files
            .get("skills/agent-planner/SKILL.md")
            .expect("repo skill bytes");
        assert_eq!(
            std::fs::read(new_dir.join("SKILL.md"))
                .expect("read new skill")
                .as_slice(),
            repo_bytes.as_slice()
        );
        let new_record = db::get_skill_by_id(&pool, "agent-planner-imported")
            .await
            .expect("load record")
            .expect("new record");
        assert!(new_record.is_central);
        assert_eq!(new_record.source.as_deref(), Some("github:anthropics/skills"));
        assert_eq!(
            new_record.canonical_path.as_deref(),
            Some(new_dir.to_string_lossy().into_owned().as_str())
        );

        // 스테이징과 백업 디렉터리가 남지 않는다.
        assert_eq!(
            central_entries(central_root.path()),
            vec![
                "agent-planner".to_string(),
                "agent-planner-imported".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn import_records_repository_origin_for_imported_skill() {
        let pool = setup_test_db().await;
        let central_root = setup_central_dir(&pool).await;

        let result = install_snapshot(
            &pool,
            multi_skill_snapshot(),
            vec![import_selection(
                "skills/commit",
                DuplicateResolution::Overwrite,
            )],
        )
        .await
        .expect("import from snapshot");
        assert_eq!(result.imported_skills.len(), 1);

        let target_dir = central_root.path().join("commit");
        let row = sqlx::query(
            "SELECT owner, repo, source_path, ref_name, baseline_state, base_commit_oid, target_path
             FROM skill_origins WHERE skill_id = ?",
        )
        .bind("commit")
        .fetch_one(&pool)
        .await
        .expect("origin row");

        assert_eq!(row.get::<String, _>("owner"), "anthropics");
        assert_eq!(row.get::<String, _>("repo"), "skills");
        assert_eq!(row.get::<String, _>("source_path"), "skills/commit");
        assert_eq!(row.get::<String, _>("ref_name"), "main");
        assert_eq!(row.get::<String, _>("baseline_state"), "unknown");
        assert!(row.get::<Option<String>, _>("base_commit_oid").is_none());
        assert_eq!(
            Path::new(&row.get::<String, _>("target_path")),
            target_dir.as_path()
        );
    }

    #[tokio::test]
    async fn import_blocks_intra_batch_target_collisions_in_both_orders() {
        let pool = setup_test_db().await;
        let central_root = setup_central_dir(&pool).await;

        let duplicate_renames = vec![
            vec![
                rename_import_selection("skills/agent-planner", "commit-imported"),
                rename_import_selection("skills/commit", "commit-imported"),
            ],
            vec![
                rename_import_selection("skills/commit", "commit-imported"),
                rename_import_selection("skills/agent-planner", "commit-imported"),
            ],
        ];
        for selections in duplicate_renames {
            let error = install_snapshot(&pool, multi_skill_snapshot(), selections)
                .await
                .expect_err("duplicate targets must be blocked");
            let failure = import_failure(error);
            assert_eq!(failure.code, GitHubImportFailureCode::Blocked);
            assert!(failure.message.contains("more than one skill"));
            assert!(failure.imported_skills.is_empty());
            assert!(central_entries(central_root.path()).is_empty());
        }

        // 앞선 rename이 예약한 대상 id를 overwrite가 차지하지 못한다.
        let rename_then_overwrite = vec![
            rename_import_selection("skills/agent-planner", "commit"),
            import_selection("skills/commit", DuplicateResolution::Overwrite),
        ];
        let overwrite_then_rename = vec![
            import_selection("skills/commit", DuplicateResolution::Overwrite),
            rename_import_selection("skills/agent-planner", "commit"),
        ];
        for selections in [rename_then_overwrite, overwrite_then_rename] {
            let error = install_snapshot(&pool, multi_skill_snapshot(), selections)
                .await
                .expect_err("reserved target must not be overwritten");
            let failure = import_failure(error);
            assert_eq!(failure.code, GitHubImportFailureCode::Blocked);
            assert!(failure.imported_skills.is_empty());
            assert!(central_entries(central_root.path()).is_empty());
        }
    }

    #[tokio::test]
    async fn import_leaves_unmanaged_directory_untouched() {
        let pool = setup_test_db().await;
        let central_root = setup_central_dir(&pool).await;

        let unmanaged_dir = central_root.path().join("agent-planner");
        std::fs::create_dir_all(&unmanaged_dir).expect("mkdir");
        let sentinel = unmanaged_dir.join("SKILL.md");
        std::fs::write(&sentinel, "unmanaged content").expect("write");

        let error = install_snapshot(
            &pool,
            multi_skill_snapshot(),
            vec![import_selection(
                "skills/agent-planner",
                DuplicateResolution::Overwrite,
            )],
        )
        .await
        .expect_err("unmanaged directory must be blocked");
        let failure = import_failure(error);
        assert_eq!(failure.code, GitHubImportFailureCode::Blocked);
        assert_eq!(failure.skill_id.as_deref(), Some("agent-planner"));
        assert_eq!(
            failure.existing_path.as_deref(),
            unmanaged_dir.to_str()
        );
        assert!(failure.message.contains("rename the imported skill id"));

        assert_eq!(
            std::fs::read_to_string(&sentinel).expect("read sentinel"),
            "unmanaged content"
        );
        assert_eq!(
            central_entries(central_root.path()),
            vec!["agent-planner".to_string()]
        );
        assert!(db::get_skill_by_id(&pool, "agent-planner")
            .await
            .expect("load record")
            .is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn import_leaves_broken_symlink_target_untouched() {
        let pool = setup_test_db().await;
        let central_root = setup_central_dir(&pool).await;

        let missing_target = central_root.path().join("missing-target");
        let broken_link = central_root.path().join("commit");
        std::os::unix::fs::symlink(&missing_target, &broken_link).expect("symlink");

        let error = install_snapshot(
            &pool,
            multi_skill_snapshot(),
            vec![import_selection(
                "skills/commit",
                DuplicateResolution::Overwrite,
            )],
        )
        .await
        .expect_err("broken symlink target must be blocked");
        let failure = import_failure(error);
        assert_eq!(failure.code, GitHubImportFailureCode::Blocked);
        assert_eq!(failure.existing_path.as_deref(), broken_link.to_str());
        assert!(failure.message.contains("symbolic link"));

        let metadata = std::fs::symlink_metadata(&broken_link).expect("symlink metadata");
        assert!(metadata.file_type().is_symlink());
        assert_eq!(
            std::fs::read_link(&broken_link).expect("read link"),
            missing_target
        );
    }

    #[tokio::test]
    async fn import_late_failure_keeps_earlier_imports_and_restores_failed_target() {
        let pool = setup_test_db().await;
        let central_root = setup_central_dir(&pool).await;

        let planner_dir = central_root.path().join("agent-planner");
        let planner_original = sample_frontmatter("Agent Planner", "original");
        seed_skill_record(&pool, "agent-planner", &planner_dir, true, &planner_original).await;

        let commit_dir = central_root.path().join("commit");
        let commit_original = sample_frontmatter("Commit", "original");
        seed_skill_record(&pool, "commit", &commit_dir, true, &commit_original).await;

        // 두 번째 스킬의 저장 단계만 실패시킨다.
        sqlx::query(
            "CREATE TRIGGER github_import_fail_commit BEFORE INSERT ON skills
             WHEN NEW.id = 'commit'
             BEGIN SELECT RAISE(ABORT, 'simulated persistence failure'); END",
        )
        .execute(&pool)
        .await
        .expect("create trigger");

        let snapshot = multi_skill_snapshot();
        let error = install_snapshot(
            &pool,
            snapshot.clone(),
            vec![
                import_selection("skills/agent-planner", DuplicateResolution::Overwrite),
                import_selection("skills/commit", DuplicateResolution::Overwrite),
            ],
        )
        .await
        .expect_err("second skill must fail");
        let failure = import_failure(error);
        assert_eq!(failure.code, GitHubImportFailureCode::Failed);
        assert_eq!(failure.skill_id.as_deref(), Some("commit"));
        assert_eq!(failure.source_path.as_deref(), Some("skills/commit"));
        assert_eq!(failure.imported_skills.len(), 1);
        assert_eq!(failure.imported_skills[0].imported_skill_id, "agent-planner");
        assert_eq!(
            failure.imported_skills[0].resolution,
            DuplicateResolution::Overwrite
        );
        assert!(failure.skipped_skills.is_empty());

        // 되돌리기가 성공했으면 보존 안내를 남기지 않는다.
        assert!(!failure.message.contains("preserved at"));
        assert!(!failure.message.contains("could not be removed"));

        // 앞서 저장된 스킬은 새 내용, 실패한 스킬은 원래 내용을 유지한다.
        let planner_bytes = snapshot
            .files
            .get("skills/agent-planner/SKILL.md")
            .expect("planner bytes");
        assert_eq!(
            std::fs::read(planner_dir.join("SKILL.md"))
                .expect("read planner")
                .as_slice(),
            planner_bytes.as_slice()
        );
        assert_eq!(
            std::fs::read_to_string(commit_dir.join("SKILL.md")).expect("read commit"),
            commit_original
        );
        assert_eq!(
            central_entries(central_root.path()),
            vec!["agent-planner".to_string(), "commit".to_string()]
        );

        // DB도 실제 완료 상태와 일치한다.
        let planner_record = db::get_skill_by_id(&pool, "agent-planner")
            .await
            .expect("load record")
            .expect("planner record");
        assert_eq!(planner_record.source.as_deref(), Some("github:anthropics/skills"));
        let commit_record = db::get_skill_by_id(&pool, "commit")
            .await
            .expect("load record")
            .expect("commit record");
        assert_eq!(commit_record.source.as_deref(), Some("local"));
    }

    #[tokio::test]
    async fn preview_top_level_skills_directory_discovers_candidates() {
        let pool = setup_test_db().await;
        let repo = GitHubRepoRef {
            owner: "anthropics".to_string(),
            repo: "skills".to_string(),
            branch: "main".to_string(),
            normalized_url: "https://github.com/anthropics/skills".to_string(),
        };
        let candidates = build_repo_skill_candidates_from_snapshot(&repo, &multi_skill_snapshot())
            .expect("candidates");
        let central_root = tempdir().expect("central");
        let preview = GitHubRepoPreview {
            repo,
            skills: build_preview_skills(&pool, central_root.path(), &candidates)
                .await
                .expect("skills"),
        };

        assert!(preview
            .skills
            .iter()
            .any(|skill| skill.source_path.starts_with("skills/")));
    }

    #[tokio::test]
    async fn preview_namespaced_skills_directory_discovers_candidates() {
        let pool = setup_test_db().await;
        let repo = GitHubRepoRef {
            owner: "openai".to_string(),
            repo: "skills".to_string(),
            branch: "main".to_string(),
            normalized_url: "https://github.com/openai/skills".to_string(),
        };

        let candidates =
            build_repo_skill_candidates_from_snapshot(&repo, &namespaced_skill_snapshot())
                .expect("candidates");

        assert_eq!(
            candidates.len(),
            2,
            "expected two namespaced skill candidates"
        );

        let curated = candidates
            .iter()
            .find(|candidate| candidate.source_path == "skills/.curated/openai-docs")
            .expect("curated skill");
        assert_eq!(curated.root_directory, "skills/.curated");
        assert_eq!(curated.skill_directory_name, "openai-docs");
        assert_eq!(curated.skill_id, "openai-docs");

        let system = candidates
            .iter()
            .find(|candidate| candidate.source_path == "skills/.system/skill-creator")
            .expect("system skill");
        assert_eq!(system.root_directory, "skills/.system");
        assert_eq!(system.skill_directory_name, "skill-creator");
        assert_eq!(system.skill_id, "skill-creator");

        let central_root = tempdir().expect("central");
        let preview = GitHubRepoPreview {
            repo,
            skills: build_preview_skills(&pool, central_root.path(), &candidates)
                .await
                .expect("preview skills"),
        };

        assert!(preview
            .skills
            .iter()
            .any(|skill| skill.source_path == "skills/.curated/openai-docs"));
        assert!(preview
            .skills
            .iter()
            .any(|skill| skill.source_path == "skills/.system/skill-creator"));
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn github_cli_auth_respects_saved_token_and_handles_logged_out_cli() {
        use std::os::unix::fs::PermissionsExt;
        let pool = setup_test_db().await;
        let temp = tempfile::TempDir::new().unwrap();
        let cli = temp.path().join("gh");
        std::fs::write(&cli, "#!/bin/sh\n[ \"$*\" = \"auth token --hostname github.com\" ] || exit 2\nprintf 'fixture-token\\n'\n").unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o700)).unwrap();
        let auth = resolve_github_auth(&pool, Some(&cli)).await.unwrap();
        assert_eq!(auth.source, "github_cli");
        assert_eq!(auth.token.as_deref(), Some("fixture-token"));
        assert!(db::get_setting(&pool, GITHUB_PAT_SETTING_KEY)
            .await
            .unwrap()
            .is_none());
        db::set_setting(&pool, GITHUB_PAT_SETTING_KEY, " app-token ")
            .await
            .unwrap();
        let auth = resolve_github_auth(&pool, Some(&cli)).await.unwrap();
        assert_eq!(auth.source, "saved_token");
        assert_eq!(auth.token.as_deref(), Some("app-token"));
        db::set_setting(&pool, GITHUB_PAT_SETTING_KEY, " ")
            .await
            .unwrap();
        std::fs::write(&cli, "#!/bin/sh\nprintf 'must-not-use'\nexit 1\n").unwrap();
        let auth = resolve_github_auth(&pool, Some(&cli)).await.unwrap();
        assert_eq!(auth.source, "not_signed_in");
        assert!(auth.token.is_none());
        assert_eq!(
            resolve_github_auth(&pool, None).await.unwrap().source,
            "cli_missing"
        );
    }

    #[tokio::test]
    async fn github_pat_setting_is_trimmed_and_empty_values_are_ignored() {
        let pool = setup_test_db().await;

        db::set_setting(&pool, GITHUB_PAT_SETTING_KEY, "  test-token  ")
            .await
            .expect("set token");
        assert_eq!(
            resolve_github_auth(&pool, None)
                .await
                .expect("read token")
                .token,
            Some("test-token".to_string())
        );

        db::set_setting(&pool, GITHUB_PAT_SETTING_KEY, "   ")
            .await
            .expect("clear token");
        assert_eq!(
            resolve_github_auth(&pool, None)
                .await
                .expect("read empty")
                .token,
            None
        );
    }

    #[tokio::test]
    async fn authenticated_api_fallback_does_not_forward_bearer_auth_to_mirror() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        };

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let accepted = Arc::new(AtomicUsize::new(0));
        let requests_clone = Arc::clone(&requests);
        let accepted_clone = Arc::clone(&accepted);

        let server = std::thread::spawn(move || {
            while accepted_clone.load(Ordering::SeqCst) < 2 {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buffer = [0_u8; 2048];
                let bytes_read = stream.read(&mut buffer).expect("read");
                let request_text = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                requests_clone
                    .lock()
                    .expect("lock")
                    .push(request_text.clone());
                accepted_clone.fetch_add(1, Ordering::SeqCst);

                if request_text.contains("GET /direct") {
                    let response =
                        "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 11\r\n\r\nbad gateway";
                    stream.write_all(response.as_bytes()).expect("write direct");
                } else {
                    let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
                    stream.write_all(response.as_bytes()).expect("write mirror");
                }
            }
        });

        let client = github_client().expect("client");
        let direct_url = format!("http://{}/direct", address);
        let mirror_url = format!("http://{}/mirror", address);

        let response = send_github_request_with_fallback(
            &client,
            GitHubFetchSurface::Api,
            |endpoint| {
                if endpoint.label == "github" {
                    direct_url.clone()
                } else {
                    mirror_url.clone()
                }
            },
            "direct request failed",
            Some("direct-token"),
        )
        .await
        .expect("fallback response");
        assert!(response.status().is_success());

        server.join().expect("server join");
        let captured = requests.lock().expect("captured");
        let direct_request = captured
            .iter()
            .find(|request| request.contains("GET /direct"))
            .expect("captured direct request");
        let mirror_request = captured
            .iter()
            .find(|request| request.contains("GET /mirror"))
            .expect("captured mirror request");
        assert!(
            direct_request.contains("authorization: Bearer direct-token")
                || direct_request.contains("Authorization: Bearer direct-token"),
            "direct github request should include bearer auth"
        );
        assert!(
            !mirror_request.contains("authorization: Bearer direct-token")
                && !mirror_request.contains("Authorization: Bearer direct-token"),
            "mirror request should not include bearer auth"
        );
    }

    #[tokio::test]
    async fn authenticated_raw_fallback_does_not_forward_bearer_auth_to_mirror() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        };

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let accepted = Arc::new(AtomicUsize::new(0));
        let requests_clone = Arc::clone(&requests);
        let accepted_clone = Arc::clone(&accepted);

        let server = std::thread::spawn(move || {
            while accepted_clone.load(Ordering::SeqCst) < 2 {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buffer = [0_u8; 2048];
                let bytes_read = stream.read(&mut buffer).expect("read");
                let request_text = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                requests_clone
                    .lock()
                    .expect("lock")
                    .push(request_text.clone());
                accepted_clone.fetch_add(1, Ordering::SeqCst);

                if request_text.contains("GET /raw-direct") {
                    let response = "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 19\r\n\r\nservice unavailable";
                    stream.write_all(response.as_bytes()).expect("write direct");
                } else {
                    let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
                    stream.write_all(response.as_bytes()).expect("write mirror");
                }
            }
        });

        let client = github_client().expect("client");
        let direct_url = format!("http://{}/raw-direct", address);
        let mirror_url = format!("http://{}/raw-mirror", address);

        let response = send_github_request_with_fallback(
            &client,
            GitHubFetchSurface::Raw,
            |endpoint| {
                if endpoint.label == "github" {
                    direct_url.clone()
                } else {
                    mirror_url.clone()
                }
            },
            "raw request failed",
            Some("direct-token"),
        )
        .await
        .expect("fallback response");
        assert!(response.status().is_success());

        server.join().expect("server join");
        let captured = requests.lock().expect("captured");
        let direct_request = captured
            .iter()
            .find(|request| request.contains("GET /raw-direct"))
            .expect("captured direct request");
        let mirror_request = captured
            .iter()
            .find(|request| request.contains("GET /raw-mirror"))
            .expect("captured mirror request");
        assert!(
            direct_request.contains("authorization: Bearer direct-token")
                || direct_request.contains("Authorization: Bearer direct-token"),
            "direct raw request should include bearer auth"
        );
        assert!(
            !mirror_request.contains("authorization: Bearer direct-token")
                && !mirror_request.contains("Authorization: Bearer direct-token"),
            "mirror raw request should not include bearer auth"
        );
    }

    #[tokio::test]
    async fn unauthenticated_rate_limit_retries_public_mirror_before_failing() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        use std::sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        };

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("addr");
        let requests = Arc::new(Mutex::new(Vec::<String>::new()));
        let accepted = Arc::new(AtomicUsize::new(0));
        let requests_clone = Arc::clone(&requests);
        let accepted_clone = Arc::clone(&accepted);

        let server = std::thread::spawn(move || {
            while accepted_clone.load(Ordering::SeqCst) < 2 {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buffer = [0_u8; 2048];
                let bytes_read = stream.read(&mut buffer).expect("read");
                let request_text = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                let is_direct = request_text.contains("GET /direct");
                requests_clone.lock().expect("lock").push(request_text);
                accepted_clone.fetch_add(1, Ordering::SeqCst);

                if is_direct {
                    let response = concat!(
                        "HTTP/1.1 403 Forbidden\r\n",
                        "Content-Type: application/json\r\n",
                        "X-RateLimit-Remaining: 0\r\n",
                        "X-RateLimit-Reset: 1786576453\r\n",
                        "Content-Length: 48\r\n\r\n",
                        "{\"message\":\"API rate limit exceeded for 1.2.3.4\"}"
                    );
                    stream.write_all(response.as_bytes()).expect("write direct");
                } else {
                    let response = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok";
                    stream.write_all(response.as_bytes()).expect("write mirror");
                }
            }
        });

        let client = github_client().expect("client");
        let direct_url = format!("http://{}/direct", address);
        let mirror_url = format!("http://{}/mirror", address);

        let response = send_github_request_with_fallback(
            &client,
            GitHubFetchSurface::Api,
            |endpoint| {
                if endpoint.label == "github" {
                    direct_url.clone()
                } else {
                    mirror_url.clone()
                }
            },
            "request failed",
            None,
        )
        .await
        .expect("mirror retry response");
        assert!(response.status().is_success());

        server.join().expect("server join");
        let captured = requests.lock().expect("captured");
        assert!(captured
            .iter()
            .any(|request| request.contains("GET /direct")));
        assert!(captured
            .iter()
            .any(|request| request.contains("GET /mirror")));
    }
}

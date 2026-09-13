//! 플랫폼별 외부 스킬 사용 제어.
//!
//! 관리 설치는 `usage` 모듈의 이동·복원 로직을 그대로 사용한다. 이 모듈은
//! 공용 폴더나 호환 경로처럼 앱이 파일을 소유하지 않는 관측을 플랫폼의
//! 공식 설정으로 제어할 때만 사용한다. 공식 설정을 확인하지 못한 플랫폼은
//! 파일을 옮기거나 DB에만 성공을 기록하지 않고 제한 사유를 반환한다.

use serde::Serialize;
use serde_json::{Map as JsonMap, Value as JsonValue};
use std::collections::{HashMap, HashSet};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tauri::State;
use tokio::sync::{Mutex, MutexGuard};
use toml_edit::{value, ArrayOfTables, DocumentMut, Item, Table};
use uuid::Uuid;

use crate::commands::{skills, usage};
use crate::db::{self, Agent, DbPool, PlatformSkillControl as StoredControl};
use crate::path_utils::resolve_home_dir;
use crate::AppState;

const STATE_ACTIVE: &str = "active";
const STATE_INACTIVE: &str = "inactive";
const STATE_DELETED: &str = "deleted";
const STATE_UNSUPPORTED: &str = "unsupported";
// `user-invocable-only`는 수동 호출을 허용하므로 비활성 상태가 아니다.
// Claude 공식 설정에서 모델과 메뉴 모두 숨기는 값은 `off`다. 비활성과
// 적용 삭제는 같은 실제 설정값을 쓰고, DB의 state로 두 동작을 구분한다.
const CLAUDE_DISABLED_VALUE: &str = "off";

static CONTROL_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

async fn control_lock() -> MutexGuard<'static, ()> {
    CONTROL_LOCK.get_or_init(|| Mutex::new(())).lock().await
}

/// 한 플랫폼 화면의 스킬 한 행에 적용할 수 있는 실제 제어 상태다.
#[derive(Debug, Clone, Serialize)]
pub struct PlatformSkillControlStatus {
    pub agent_id: String,
    pub skill_id: String,
    pub row_id: String,
    pub skill_name: String,
    /// 출처 스킬 폴더의 안정적인 절대 경로다. 행 번호를 키로 쓰지 않는다.
    pub source_path: String,
    pub source_kind: Option<String>,
    pub state: String,
    pub supported: bool,
    pub can_toggle: bool,
    pub can_delete: bool,
    pub can_reapply: bool,
    pub reason: Option<String>,
    pub requires_reload: bool,
    /// `path`는 한 출처, `name`은 같은 이름의 여러 출처에 적용될 수 있다.
    pub scope: String,
    pub affected_source_count: usize,
    pub adapter: String,
    pub config_path: Option<String>,
    pub shared_install: Option<usage::SharedSkillImpact>,
    /// 실제 플랫폼 개별 제외 상태. 관리 설치는 항상 false다.
    pub excluded_here: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Adapter {
    ClaudeSkillOverrides,
    CodexSkillsConfig,
    ManagedInstallation,
    Unsupported,
}

impl Adapter {
    fn name(self) -> &'static str {
        match self {
            Self::ClaudeSkillOverrides => "claude-skill-overrides",
            Self::CodexSkillsConfig => "codex-skills-config",
            Self::ManagedInstallation => "managed-installation",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExternalAction {
    Disable,
    Enable,
    Delete,
    Reapply,
}

fn source_path_matches(left: &str, right: &str) -> bool {
    let left_path = Path::new(left);
    let right_path = Path::new(right);
    left == right
        || left_path
            .canonicalize()
            .ok()
            .zip(right_path.canonicalize().ok())
            .is_some_and(|(left, right)| left == right)
}

fn codex_paths_match(left: &Path, right: &Path) -> bool {
    let left = canonical_codex_skill_path(left);
    let right = canonical_codex_skill_path(right);
    source_path_matches(&left.to_string_lossy(), &right.to_string_lossy()) || left == right
}

fn source_path_for_skill(skill: &db::SkillForAgent) -> String {
    skill.dir_path.clone()
}

fn is_managed_installation(
    installations: &[db::SkillInstallation],
    paused: &[db::PausedInstallation],
    skill: &db::SkillForAgent,
) -> Option<bool> {
    if installations.iter().any(|installation| {
        installation.skill_id == skill.id
            && source_path_matches(&installation.installed_path, &skill.dir_path)
    }) {
        return Some(true);
    }
    if paused.iter().any(|installation| {
        installation.skill_id == skill.id
            && source_path_matches(&installation.installed_path, &skill.dir_path)
    }) {
        return Some(false);
    }
    None
}

fn unsupported_reason(agent: &Agent, source_kind: Option<&str>) -> String {
    if agent.id == "claude-code" && source_kind == Some("plugin") {
        return "Claude Code의 skillOverrides는 플러그인 스킬에 적용되지 않습니다. 플러그인 관리에서 끄거나 삭제해야 합니다.".to_string();
    }
    format!(
        "{}에 이 출처를 독립적으로 끄는 공식 설정을 확인하지 못했습니다. 공용 파일이나 링크를 옮겨 대신 처리하지 않습니다.",
        agent.display_name
    )
}

fn adapter_for(agent: &Agent, skill: &db::SkillForAgent, managed: bool) -> Adapter {
    if managed {
        return Adapter::ManagedInstallation;
    }
    match agent.id.as_str() {
        "claude-code" if skill.source_kind.as_deref() != Some("plugin") => {
            Adapter::ClaudeSkillOverrides
        }
        "codex" => Adapter::CodexSkillsConfig,
        _ => Adapter::Unsupported,
    }
}

fn config_path_for_claude(agent: &Agent) -> Result<PathBuf, String> {
    let root = Path::new(&agent.global_skills_dir)
        .parent()
        .ok_or_else(|| "Claude Code 스킬 설정 폴더를 확인할 수 없습니다".to_string())?;
    // settings.local.json은 프로젝트·로컬 우선순위 설정이라 전역 스킬 제어
    // 대상이라고 확인하지 않았다. 공식 사용자 설정 파일만 사용한다.
    Ok(root.join("settings.json"))
}

fn config_path_for_codex(agent: &Agent) -> Result<PathBuf, String> {
    if let Some(codex_home) = std::env::var_os("CODEX_HOME") {
        if !codex_home.is_empty() {
            return Ok(PathBuf::from(codex_home).join("config.toml"));
        }
    }

    let skills_dir = Path::new(&agent.global_skills_dir);
    let root = skills_dir
        .parent()
        .ok_or_else(|| "Codex 스킬 설정 폴더를 확인할 수 없습니다".to_string())?;
    if root.file_name().and_then(|name| name.to_str()) == Some(".codex") {
        return Ok(root.join("config.toml"));
    }
    if root.file_name().and_then(|name| name.to_str()) == Some(".agents") {
        return Ok(root
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(".codex/config.toml"));
    }
    Ok(resolve_home_dir().join(".codex/config.toml"))
}

fn config_path_for_codex_override(
    agent: &Agent,
    override_path: Option<&Path>,
) -> Result<PathBuf, String> {
    override_path
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(|| config_path_for_codex(agent))
}

fn read_file_if_exists(path: &Path) -> Result<Option<String>, String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(format!(
            "설정 파일을 읽을 수 없습니다 '{}': {error}",
            path.display()
        )),
    }
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    if let Ok(metadata) = fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "설정 파일이 심볼릭 링크라 안전하게 교체할 수 없습니다 '{}': 링크 대상의 설정을 직접 확인하세요",
                path.display()
            ));
        }
    }
    let parent = path
        .parent()
        .ok_or_else(|| format!("설정 파일의 상위 폴더가 없습니다: {}", path.display()))?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "설정 폴더를 만들 수 없습니다 '{}': {error}",
            parent.display()
        )
    })?;
    let temporary = parent.join(format!(".skillsmanage-control-{}", Uuid::new_v4()));
    let original_permissions = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());
    let result = (|| {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temporary).map_err(|error| {
            format!(
                "임시 설정 파일을 저장할 수 없습니다 '{}': {error}",
                temporary.display()
            )
        })?;
        std::io::Write::write_all(&mut file, text.as_bytes()).map_err(|error| {
            format!(
                "임시 설정 파일을 기록할 수 없습니다 '{}': {error}",
                temporary.display()
            )
        })?;
        file.sync_all().map_err(|error| {
            format!(
                "임시 설정 파일을 동기화할 수 없습니다 '{}': {error}",
                temporary.display()
            )
        })?;
        drop(file);
        if let Some(permissions) = original_permissions {
            fs::set_permissions(&temporary, permissions).map_err(|error| {
                format!(
                    "설정 파일 권한을 보존할 수 없습니다 '{}': {error}",
                    path.display()
                )
            })?;
        }
        fs::rename(&temporary, path).map_err(|error| {
            format!(
                "설정 파일을 원자적으로 교체할 수 없습니다 '{}': {error}",
                path.display()
            )
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn restore_file(path: &Path, original: Option<&str>) -> Result<(), String> {
    match original {
        Some(text) => write_atomic(path, text),
        None => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!(
                "설정 변경을 되돌릴 수 없습니다 '{}': {error}",
                path.display()
            )),
        },
    }
}

fn json_document(text: Option<&str>) -> Result<JsonValue, String> {
    match text {
        None => Ok(JsonValue::Object(JsonMap::new())),
        Some(text) => serde_json::from_str(text)
            .map_err(|error| format!("Claude 설정 JSON을 읽을 수 없습니다: {error}")),
    }
}

fn json_override(document: &JsonValue, skill_name: &str) -> Result<Option<String>, String> {
    let Some(overrides) = document.get("skillOverrides") else {
        return Ok(None);
    };
    let Some(overrides) = overrides.as_object() else {
        return Err("Claude 설정의 skillOverrides가 객체가 아닙니다".to_string());
    };
    let Some(value) = overrides.get(skill_name) else {
        return Ok(None);
    };
    value
        .as_str()
        .map(|value| Some(value.to_string()))
        .ok_or_else(|| {
            format!("Claude 설정의 skillOverrides['{skill_name}'] 값이 문자열이 아닙니다")
        })
}

fn set_json_override(
    document: &mut JsonValue,
    skill_name: &str,
    value: Option<&str>,
) -> Result<(), String> {
    let object = document
        .as_object_mut()
        .ok_or_else(|| "Claude 설정의 최상위 값이 객체가 아닙니다".to_string())?;
    if let Some(value) = value {
        let overrides = object
            .entry("skillOverrides".to_string())
            .or_insert_with(|| JsonValue::Object(JsonMap::new()));
        let overrides = overrides
            .as_object_mut()
            .ok_or_else(|| "Claude 설정의 skillOverrides가 객체가 아닙니다".to_string())?;
        overrides.insert(skill_name.to_string(), JsonValue::String(value.to_string()));
    } else if let Some(overrides) = object.get_mut("skillOverrides") {
        let overrides = overrides
            .as_object_mut()
            .ok_or_else(|| "Claude 설정의 skillOverrides가 객체가 아닙니다".to_string())?;
        overrides.remove(skill_name);
    }
    Ok(())
}

fn json_state(value: Option<&str>) -> Result<&'static str, String> {
    match value {
        None | Some("on") | Some("name-only") | Some("user-invocable-only") => Ok(STATE_ACTIVE),
        Some(CLAUDE_DISABLED_VALUE) => Ok(STATE_INACTIVE),
        Some(value) => Err(format!(
            "Claude skillOverrides의 확인되지 않은 값입니다: {value}"
        )),
    }
}

fn canonical_codex_skill_path(path: &Path) -> PathBuf {
    if path.file_name().and_then(|name| name.to_str()) == Some("SKILL.md") {
        path.to_path_buf()
    } else {
        path.join("SKILL.md")
    }
}

fn codex_source_path(skill: &db::SkillForAgent) -> String {
    canonical_codex_skill_path(Path::new(&skill.file_path))
        .to_string_lossy()
        .into_owned()
}

fn normalize_configured_path(raw: &str, config_path: &Path) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        canonical_codex_skill_path(&path)
    } else {
        canonical_codex_skill_path(
            &config_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(path),
        )
    }
}

fn codex_config_entries_mut(document: &mut DocumentMut) -> Result<&mut ArrayOfTables, String> {
    let skills = document["skills"].or_insert(Item::Table(Table::new()));
    let skills = skills
        .as_table_mut()
        .ok_or_else(|| "Codex 설정의 skills가 테이블이 아닙니다".to_string())?;
    let config = skills["config"].or_insert(Item::ArrayOfTables(ArrayOfTables::new()));
    config
        .as_array_of_tables_mut()
        .ok_or_else(|| "Codex 설정의 skills.config가 배열 테이블이 아닙니다".to_string())
}

fn codex_config_entries(
    document: &DocumentMut,
) -> Result<Option<&toml_edit::ArrayOfTables>, String> {
    let Some(skills) = document.get("skills") else {
        return Ok(None);
    };
    let Some(skills) = skills.as_table() else {
        return Err("Codex 설정의 skills가 테이블이 아닙니다".to_string());
    };
    let Some(config) = skills.get("config") else {
        return Ok(None);
    };
    config
        .as_array_of_tables()
        .map(Some)
        .ok_or_else(|| "Codex 설정의 skills.config가 배열 테이블이 아닙니다".to_string())
}

fn codex_entry_path(table: &Table) -> Result<&str, String> {
    table
        .get("path")
        .and_then(Item::as_value)
        .and_then(|value| value.as_str())
        .ok_or_else(|| "Codex skills.config 항목에 path가 없습니다".to_string())
}

fn codex_entry_enabled(table: &Table) -> Result<bool, String> {
    table
        .get("enabled")
        .and_then(Item::as_value)
        .and_then(|value| value.as_bool())
        .ok_or_else(|| "Codex skills.config 항목에 enabled가 없습니다".to_string())
}

fn codex_matching_indices(
    document: &DocumentMut,
    source_path: &str,
    config_path: &Path,
) -> Result<Vec<usize>, String> {
    let canonical_source = canonical_codex_skill_path(Path::new(source_path));
    let Some(entries) = codex_config_entries(document)? else {
        return Ok(Vec::new());
    };
    Ok(entries
        .iter()
        .enumerate()
        .filter_map(|(index, table)| {
            let raw = codex_entry_path(table).ok()?;
            codex_paths_match(
                &normalize_configured_path(raw, config_path),
                &canonical_source,
            )
            .then_some(index)
        })
        .collect())
}

fn codex_current_value(
    document: &DocumentMut,
    source_path: &str,
    config_path: &Path,
) -> Result<Option<bool>, String> {
    let indices = codex_matching_indices(document, source_path, config_path)?;
    if indices.len() > 1 {
        return Err(
            "Codex skills.config에 같은 스킬 경로가 여러 번 있어 변경하지 않습니다".to_string(),
        );
    }
    let Some(index) = indices.first().copied() else {
        return Ok(None);
    };
    let entries = codex_config_entries(document)?.expect("matching entry implies config");
    Ok(Some(codex_entry_enabled(
        entries.get(index).expect("matching index is in bounds"),
    )?))
}

fn set_codex_value(
    document: &mut DocumentMut,
    source_path: &str,
    config_path: &Path,
    enabled: bool,
) -> Result<Option<bool>, String> {
    let indices = codex_matching_indices(document, source_path, config_path)?;
    if indices.len() > 1 {
        return Err(
            "Codex skills.config에 같은 스킬 경로가 여러 번 있어 변경하지 않습니다".to_string(),
        );
    }
    if let Some(index) = indices.first().copied() {
        let entries = codex_config_entries_mut(document)?;
        let table = entries
            .get_mut(index)
            .ok_or_else(|| "Codex skills.config 항목을 다시 찾을 수 없습니다".to_string())?;
        let previous = codex_entry_enabled(table)?;
        table["enabled"] = value(enabled);
        return Ok(Some(previous));
    }
    let entries = codex_config_entries_mut(document)?;
    let mut table = Table::new();
    table["path"] = value(
        canonical_codex_skill_path(Path::new(source_path))
            .to_string_lossy()
            .to_string(),
    );
    table["enabled"] = value(enabled);
    entries.push(table);
    Ok(None)
}

fn restore_codex_value(
    document: &mut DocumentMut,
    source_path: &str,
    config_path: &Path,
    original: Option<bool>,
) -> Result<(), String> {
    let indices = codex_matching_indices(document, source_path, config_path)?;
    if indices.len() > 1 {
        return Err(
            "Codex skills.config에 같은 스킬 경로가 여러 번 있어 복원하지 않습니다".to_string(),
        );
    }
    match (indices.first().copied(), original) {
        (Some(index), Some(enabled)) => {
            let entries = codex_config_entries_mut(document)?;
            entries
                .get_mut(index)
                .ok_or_else(|| "Codex skills.config 항목을 다시 찾을 수 없습니다".to_string())?
                ["enabled"] = value(enabled);
        }
        (Some(index), None) => {
            codex_config_entries_mut(document)?.remove(index);
        }
        (None, Some(enabled)) => {
            let entries = codex_config_entries_mut(document)?;
            let mut table = Table::new();
            table["path"] = value(
                canonical_codex_skill_path(Path::new(source_path))
                    .to_string_lossy()
                    .to_string(),
            );
            table["enabled"] = value(enabled);
            entries.push(table);
        }
        (None, None) => {}
    }
    Ok(())
}

fn stored_original_bool(control: &StoredControl) -> Result<Option<bool>, String> {
    control
        .original_value
        .as_deref()
        .map(|value| match value {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err("Codex의 원래 enabled 값 기록이 손상되었습니다".to_string()),
        })
        .transpose()
}

fn control_state_from_stored(
    stored: Option<&StoredControl>,
    actual_value: &str,
    actual_state: &'static str,
) -> Result<&'static str, String> {
    let Some(stored) = stored else {
        return Ok(actual_state);
    };
    if stored.applied_value != actual_value {
        return Err(
            "설정이 다른 프로그램에 의해 바뀌어 충돌했습니다. 덮어쓰지 않습니다".to_string(),
        );
    }
    match stored.state.as_str() {
        STATE_INACTIVE => Ok(STATE_INACTIVE),
        STATE_DELETED => Ok(STATE_DELETED),
        _ => Ok(actual_state),
    }
}

#[allow(clippy::too_many_arguments)]
fn make_status(
    agent: &Agent,
    skill: db::SkillForAgent,
    adapter: Adapter,
    stored: Option<&StoredControl>,
    actual_state: Result<(&'static str, String), String>,
    config_path: Option<PathBuf>,
    affected_source_count: usize,
    scope: &str,
) -> PlatformSkillControlStatus {
    let state_result = match adapter {
        Adapter::Unsupported => Ok(STATE_UNSUPPORTED),
        _ => {
            actual_state.and_then(|(state, value)| control_state_from_stored(stored, &value, state))
        }
    };
    let base_reason = match (&adapter, &state_result) {
        (Adapter::Unsupported, _) => Some(unsupported_reason(agent, skill.source_kind.as_deref())),
        (_, Err(error)) => Some(error.clone()),
        _ => None,
    };
    let state = state_result.unwrap_or(STATE_UNSUPPORTED);
    let supported = adapter != Adapter::Unsupported && base_reason.is_none();
    let deleted = state == STATE_DELETED;
    let source_path = source_path_for_skill(&skill);
    PlatformSkillControlStatus {
        agent_id: agent.id.clone(),
        skill_id: skill.id,
        row_id: skill.row_id,
        skill_name: skill.name,
        source_path,
        source_kind: skill.source_kind,
        state: state.to_string(),
        supported,
        can_toggle: supported && !deleted,
        can_delete: supported && !deleted,
        can_reapply: supported && deleted,
        reason: if supported {
            if scope == "name" && affected_source_count > 1 {
                Some(format!(
                    "같은 이름의 {}개 출처에 함께 적용됩니다.",
                    affected_source_count
                ))
            } else {
                None
            }
        } else {
            base_reason
        },
        requires_reload: matches!(
            adapter,
            Adapter::ClaudeSkillOverrides | Adapter::CodexSkillsConfig
        ),
        scope: scope.to_string(),
        affected_source_count,
        adapter: adapter.name().to_string(),
        config_path: config_path.map(|path| path.to_string_lossy().into_owned()),
        shared_install: None,
        excluded_here: matches!(state, STATE_INACTIVE | STATE_DELETED),
    }
}

async fn actual_external_status_at_path(
    agent: &Agent,
    skill: &db::SkillForAgent,
    stored: Option<&StoredControl>,
    affected_source_count: usize,
    codex_config_override: Option<&Path>,
) -> PlatformSkillControlStatus {
    let adapter = adapter_for(agent, skill, false);
    match adapter {
        Adapter::ClaudeSkillOverrides => {
            let config_path = config_path_for_claude(agent);
            let actual_result = match config_path.as_ref() {
                Ok(path) => match read_file_if_exists(path)
                    .and_then(|text| json_document(text.as_deref()))
                    .and_then(|document| json_override(&document, &skill.name))
                    .and_then(|value| {
                        let raw = value.clone().unwrap_or_else(|| "on".to_string());
                        json_state(value.as_deref()).map(|state| (state, raw))
                    }) {
                    Ok(state) => Ok(state),
                    Err(error) => Err(error),
                },
                Err(error) => Err(error.clone()),
            };
            make_status(
                agent,
                skill.clone(),
                adapter,
                stored,
                actual_result,
                config_path.as_ref().ok().cloned(),
                affected_source_count,
                "name",
            )
        }
        Adapter::CodexSkillsConfig => {
            let config_path = config_path_for_codex_override(agent, codex_config_override);
            let actual_result = match config_path.as_ref() {
                Ok(path) => match read_file_if_exists(path).and_then(|text| {
                    text.as_deref()
                        .unwrap_or("")
                        .parse::<DocumentMut>()
                        .map_err(|error| format!("Codex config.toml을 읽을 수 없습니다: {error}"))
                }) {
                    Ok(document) => {
                        match codex_current_value(&document, &codex_source_path(skill), path) {
                            Ok(Some(enabled)) => Ok((
                                if enabled {
                                    STATE_ACTIVE
                                } else {
                                    STATE_INACTIVE
                                },
                                enabled.to_string(),
                            )),
                            Ok(None) => Ok((STATE_ACTIVE, "true".to_string())),
                            Err(error) => Err(error),
                        }
                    }
                    Err(error) => Err(error),
                },
                Err(error) => Err(error.clone()),
            };
            make_status(
                agent,
                skill.clone(),
                adapter,
                stored,
                actual_result,
                config_path.as_ref().ok().cloned(),
                1,
                "path",
            )
        }
        Adapter::Unsupported => make_status(
            agent,
            skill.clone(),
            adapter,
            stored,
            Ok((STATE_UNSUPPORTED, String::new())),
            None,
            1,
            "path",
        ),
        Adapter::ManagedInstallation => unreachable!("managed skills use usage.rs"),
    }
}

/// 플랫폼의 현재 스킬 행마다 실제 Adapter 상태를 조회한다.
async fn get_platform_skill_controls_impl_at_path(
    pool: &DbPool,
    agent_id: &str,
    codex_config_override: Option<&Path>,
) -> Result<Vec<PlatformSkillControlStatus>, String> {
    let agent = db::get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| format!("플랫폼 '{}'을(를) 찾을 수 없습니다", agent_id))?;
    let mut skills = skills::get_skills_by_agent_impl(pool, agent_id).await?;
    let installations = db::get_skill_installations_by_agent(pool, agent_id).await?;
    let paused = db::get_paused_installations_by_agent(pool, agent_id).await?;
    // Rescan retention: paused entries lose their on-disk path by design, so
    // the scanner would prune their observations. Keep cards/restore by
    // merging paused rows missing from the skill list (observation identity
    // uses entry keys, never the last-link target).
    {
        let mut known_keys: HashSet<String> = skills
            .iter()
            .map(|s| usage::shared_entry_key(&s.dir_path))
            .collect();
        for p in &paused {
            let key = usage::shared_entry_key(&p.installed_path);
            if known_keys.contains(&key) {
                continue;
            }
            known_keys.insert(key);
            let db_skill = db::get_skill_by_id(pool, &p.skill_id).await?;
            let name = if p.skill_name.trim().is_empty() {
                db_skill
                    .as_ref()
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| p.skill_id.clone())
            } else {
                p.skill_name.clone()
            };
            let description = db_skill.and_then(|s| s.description);
            skills.push(db::SkillForAgent {
                id: p.skill_id.clone(),
                row_id: p.skill_id.clone(),
                name,
                description,
                file_path: Path::new(&p.installed_path)
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                dir_path: p.installed_path.clone(),
                link_type: p.link_type.clone(),
                symlink_target: p.symlink_target.clone(),
                is_central: false,
                source_kind: None,
                source_root: None,
                source_label: None,
                is_read_only: false,
                conflict_group: None,
                conflict_count: 0,
            });
        }
    }
    let stored = db::get_platform_skill_controls(pool, agent_id).await?;
    let mut result = Vec::with_capacity(skills.len());

    for skill in skills {
        let source_path = source_path_for_skill(&skill);
        if let Some(enabled) = is_managed_installation(&installations, &paused, &skill) {
            result.push(PlatformSkillControlStatus {
                agent_id: agent.id.clone(),
                skill_id: skill.id,
                row_id: skill.row_id,
                skill_name: skill.name,
                source_path,
                source_kind: skill.source_kind,
                state: if enabled {
                    STATE_ACTIVE
                } else {
                    STATE_INACTIVE
                }
                .to_string(),
                supported: true,
                can_toggle: true,
                can_delete: true,
                can_reapply: false,
                reason: None,
                requires_reload: false,
                scope: "path".to_string(),
                affected_source_count: 1,
                adapter: Adapter::ManagedInstallation.name().to_string(),
                config_path: None,
                shared_install: None,
                excluded_here: false,
            });
            continue;
        }

        let matching_count =
            if agent.id == "claude-code" && skill.source_kind.as_deref() != Some("plugin") {
                skills::get_skills_by_agent_impl(pool, agent_id)
                    .await?
                    .into_iter()
                    .filter(|candidate| candidate.name == skill.name)
                    .count()
            } else {
                1
            };
        let stored_control = stored
            .iter()
            .find(|control| {
                source_path_matches(&control.source_path, &source_path)
                    || (agent.id == "codex"
                        && codex_paths_match(
                            Path::new(&control.source_path),
                            Path::new(&skill.file_path),
                        ))
            })
            .or_else(|| {
                (agent.id == "claude-code" && skill.source_kind.as_deref() != Some("plugin"))
                    .then(|| {
                        stored
                            .iter()
                            .find(|control| control.skill_name == skill.name)
                    })
                    .flatten()
            });
        result.push(
            actual_external_status_at_path(
                &agent,
                &skill,
                stored_control,
                matching_count.max(1),
                codex_config_override,
            )
            .await,
        );
    }

    // Shared enrichment (including Universal): one impact per distinct entry
    // key. Existing source_path_matches follows the last link and is never
    // reused for this identity. Compute errors stay visible as a restricted
    // impact instead of looking like "not shared".
    {
        let mut cache: HashMap<String, Result<usage::SharedSkillImpact, String>> = HashMap::new();
        for status in result.iter_mut() {
            let key = usage::shared_entry_key(&status.source_path);
            let computed = match cache.get(&key) {
                Some(cached) => cached.clone(),
                None => {
                    let computed = usage::compute_shared_impact(pool, &key).await;
                    cache.insert(key.clone(), computed.clone());
                    computed
                }
            };
            status.shared_install = match computed {
                Ok(impact) => {
                    let shares_other =
                        usage::entry_is_shared_with_others(pool, &key, &status.agent_id)
                            .await
                            .unwrap_or(true)
                            || impact
                                .confirmed_platforms
                                .iter()
                                .any(|c| c.agent_id != status.agent_id);
                    if impact.reason.is_some()
                        || !impact.separate_installs.is_empty()
                        || shares_other
                        || status.agent_id == "universal"
                    {
                        Some(impact)
                    } else {
                        None
                    }
                }
                Err(error) => {
                    if status.reason.is_none() {
                        status.reason = Some(error.clone());
                    }
                    Some(usage::SharedSkillImpact {
                        shared_install_id: key,
                        skill_id: status.skill_id.clone(),
                        skill_name: status.skill_name.clone(),
                        enabled: status.state == STATE_ACTIVE,
                        confirmed_platforms: Vec::new(),
                        separate_installs: Vec::new(),
                        reason: Some(error),
                        management_path: status.source_path.clone(),
                        confirmation_token: String::new(),
                    })
                }
            };
        }
    }

    Ok(result)
}

pub async fn get_platform_skill_controls_impl(
    pool: &DbPool,
    agent_id: &str,
) -> Result<Vec<PlatformSkillControlStatus>, String> {
    get_platform_skill_controls_impl_at_path(pool, agent_id, None).await
}

fn find_target_skill(
    skills: &[db::SkillForAgent],
    skill_id: &str,
    skill_name: &str,
    source_path: &str,
) -> Result<db::SkillForAgent, String> {
    skills
        .iter()
        .find(|skill| {
            skill.id == skill_id
                && skill.name == skill_name
                && source_path_matches(&skill.dir_path, source_path)
        })
        .cloned()
        .ok_or_else(|| "스킬 출처가 현재 플랫폼 목록과 달라 변경을 중단했습니다".to_string())
}

async fn execute_external_action(
    pool: &DbPool,
    agent: &Agent,
    skill: &db::SkillForAgent,
    scope_skills: &[db::SkillForAgent],
    action: ExternalAction,
) -> Result<(), String> {
    let adapter = adapter_for(agent, skill, false);
    if adapter == Adapter::Unsupported {
        return Err(unsupported_reason(agent, skill.source_kind.as_deref()));
    }
    let source_path = if adapter == Adapter::CodexSkillsConfig {
        codex_source_path(skill)
    } else {
        source_path_for_skill(skill)
    };
    let previous_controls = db::get_platform_skill_controls(pool, &agent.id).await?;
    let existing = match previous_controls
        .iter()
        .find(|control| source_path_matches(&control.source_path, &source_path))
        .cloned()
    {
        Some(control) => Some(control),
        None if adapter == Adapter::ClaudeSkillOverrides => {
            let sibling_path = scope_skills.iter().find_map(|candidate| {
                (candidate.name == skill.name && candidate.dir_path != source_path)
                    .then(|| candidate.dir_path.clone())
            });
            sibling_path.and_then(|path| {
                previous_controls
                    .iter()
                    .find(|control| source_path_matches(&control.source_path, &path))
                    .cloned()
            })
        }
        None => None,
    };
    match adapter {
        Adapter::ClaudeSkillOverrides => {
            execute_claude_action(
                pool,
                agent,
                skill,
                scope_skills,
                &previous_controls,
                source_path,
                existing,
                action,
            )
            .await
        }
        Adapter::CodexSkillsConfig => {
            execute_codex_action(pool, agent, skill, source_path, existing, action).await
        }
        Adapter::ManagedInstallation | Adapter::Unsupported => {
            Err("외부 스킬 Adapter 대상이 아닙니다".to_string())
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_claude_action(
    pool: &DbPool,
    agent: &Agent,
    skill: &db::SkillForAgent,
    scope_skills: &[db::SkillForAgent],
    previous_controls: &[StoredControl],
    source_path: String,
    existing: Option<StoredControl>,
    action: ExternalAction,
) -> Result<(), String> {
    let config_path = config_path_for_claude(agent)?;
    let before = read_file_if_exists(&config_path)?;
    let mut document = json_document(before.as_deref())?;
    let current = json_override(&document, &skill.name)?;
    let expected_applied = existing
        .as_ref()
        .map(|control| control.applied_value.as_str());
    if let Some(expected) = expected_applied {
        if current.as_deref() != Some(expected) {
            return Err(
                "설정이 다른 프로그램에 의해 바뀌어 충돌했습니다. 덮어쓰지 않습니다".to_string(),
            );
        }
    }
    // 기존 제어 기록이 있으면 원래 값이 `None`인 것도 의미 있는 기록이다.
    // 현재 값을 다시 섞으면 비활성→삭제→재적용 때 앱이 쓴 `off`가 원본으로
    // 저장되어 영구적으로 비활성 상태가 된다.
    let original = match (action, existing.as_ref()) {
        // 기록이 없는 수동 `off`를 활성화할 때는 기본 활성 상태로 되돌린다.
        // 앱이 기록한 비활성/삭제는 아래 existing 기록의 원본(None 포함)을 쓴다.
        (ExternalAction::Enable | ExternalAction::Reapply, None) => None,
        (_, Some(control)) => control.original_value.clone(),
        (_, None) => current.clone(),
    };
    let (state, applied, restore) = match action {
        ExternalAction::Disable => (STATE_INACTIVE, CLAUDE_DISABLED_VALUE, false),
        ExternalAction::Delete => (STATE_DELETED, CLAUDE_DISABLED_VALUE, false),
        ExternalAction::Enable | ExternalAction::Reapply => {
            if action == ExternalAction::Enable
                && existing
                    .as_ref()
                    .is_some_and(|control| control.state == STATE_DELETED)
            {
                return Err("적용 삭제된 스킬은 재적용 버튼으로 다시 활성화하세요".to_string());
            }
            (STATE_ACTIVE, "", true)
        }
    };
    if restore {
        set_json_override(&mut document, &skill.name, original.as_deref())?;
    } else {
        set_json_override(&mut document, &skill.name, Some(applied))?;
    }
    let text = serde_json::to_string_pretty(&document)
        .map_err(|error| format!("Claude 설정을 저장할 수 없습니다: {error}"))?;
    write_atomic(&config_path, &text)?;

    let verification = (|| -> Result<(), String> {
        let verify = read_file_if_exists(&config_path)?;
        let verify_document = json_document(verify.as_deref())?;
        let verify_value = json_override(&verify_document, &skill.name)?;
        let expected_value = if restore {
            original.clone()
        } else {
            Some(applied.to_string())
        };
        if verify_value != expected_value {
            return Err("Claude 설정을 다시 읽었을 때 요청한 상태가 아닙니다".to_string());
        }
        Ok(())
    })();
    if let Err(error) = verification {
        let rollback = restore_file(&config_path, before.as_deref());
        return Err(match rollback {
            Ok(()) => format!("Claude 설정 검증에 실패해 변경을 되돌렸습니다: {error}"),
            Err(rollback_error) => format!(
                "Claude 설정 검증에 실패했고 되돌리기도 실패했습니다: {error}; {rollback_error}"
            ),
        });
    }

    let db_result = if restore {
        db::delete_platform_skill_controls_by_name(pool, &agent.id, &skill.name).await
    } else {
        let paths = scope_skills
            .iter()
            .filter(|candidate| candidate.name == skill.name)
            .map(|candidate| candidate.dir_path.clone())
            .chain(std::iter::once(source_path.clone()))
            .collect::<std::collections::BTreeSet<_>>();
        let updated_at = chrono::Utc::now().to_rfc3339();
        let mut result = Ok(());
        for path in paths {
            result = db::upsert_platform_skill_control(
                pool,
                &StoredControl {
                    agent_id: agent.id.clone(),
                    source_path: path,
                    skill_name: skill.name.clone(),
                    state: state.to_string(),
                    original_value: original.clone(),
                    applied_value: applied.to_string(),
                    updated_at: updated_at.clone(),
                },
            )
            .await;
            if result.is_err() {
                break;
            }
        }
        result
    };
    if let Err(error) = db_result {
        let records_rollback = if restore {
            Ok(())
        } else {
            let rollback_delete =
                db::delete_platform_skill_controls_by_name(pool, &agent.id, &skill.name).await;
            match rollback_delete {
                Ok(()) => {
                    let mut rollback_result = Ok(());
                    for control in previous_controls
                        .iter()
                        .filter(|control| control.skill_name == skill.name)
                    {
                        rollback_result = db::upsert_platform_skill_control(pool, control).await;
                        if rollback_result.is_err() {
                            break;
                        }
                    }
                    rollback_result
                }
                Err(rollback_error) => Err(rollback_error),
            }
        };
        let rollback = restore_file(&config_path, before.as_deref());
        return Err(match (rollback, records_rollback) {
            (Ok(()), Ok(())) => format!("플랫폼 설정과 제어 기록을 되돌렸지만 저장에 실패했습니다: {error}"),
            (Err(file_error), Ok(())) => format!("제어 기록은 되돌렸지만 플랫폼 설정 복원에 실패했습니다: {error}; {file_error}"),
            (Ok(()), Err(records_error)) => format!("플랫폼 설정은 되돌렸지만 제어 기록 복원에 실패했습니다: {error}; {records_error}"),
            (Err(file_error), Err(records_error)) => format!("플랫폼 설정과 제어 기록 복원이 모두 실패했습니다: {error}; {file_error}; {records_error}"),
        });
    }
    Ok(())
}

async fn execute_codex_action(
    pool: &DbPool,
    agent: &Agent,
    skill: &db::SkillForAgent,
    source_path: String,
    existing: Option<StoredControl>,
    action: ExternalAction,
) -> Result<(), String> {
    let config_path = config_path_for_codex(agent)?;
    execute_codex_action_at_path(
        pool,
        agent,
        skill,
        source_path,
        config_path,
        existing,
        action,
    )
    .await
}

async fn execute_codex_action_at_path(
    pool: &DbPool,
    agent: &Agent,
    skill: &db::SkillForAgent,
    source_path: String,
    config_path: PathBuf,
    existing: Option<StoredControl>,
    action: ExternalAction,
) -> Result<(), String> {
    let before = read_file_if_exists(&config_path)?;
    let mut document = before
        .as_deref()
        .unwrap_or("")
        .parse::<DocumentMut>()
        .map_err(|error| format!("Codex config.toml을 읽을 수 없습니다: {error}"))?;
    let current = codex_current_value(&document, &source_path, &config_path)?;
    let current_text = current.map(|value| value.to_string());
    if let Some(control) = existing.as_ref() {
        if current_text.as_deref() != Some(control.applied_value.as_str()) {
            return Err(
                "설정이 다른 프로그램에 의해 바뀌어 충돌했습니다. 덮어쓰지 않습니다".to_string(),
            );
        }
    }
    let original = match (action, existing.as_ref()) {
        // 기록이 없는 수동 `enabled = false`를 활성화할 때는 설정 항목을
        // 제거해 Codex 기본 활성 상태로 되돌린다.
        (ExternalAction::Enable | ExternalAction::Reapply, None) => None,
        (_, Some(control)) => control.original_value.clone(),
        (_, None) => current_text,
    };
    let (state, applied, restore) = match action {
        ExternalAction::Disable => (STATE_INACTIVE, "false".to_string(), false),
        ExternalAction::Delete => (STATE_DELETED, "false".to_string(), false),
        ExternalAction::Enable | ExternalAction::Reapply => {
            if action == ExternalAction::Enable
                && existing
                    .as_ref()
                    .is_some_and(|control| control.state == STATE_DELETED)
            {
                return Err("적용 삭제된 스킬은 재적용 버튼으로 다시 활성화하세요".to_string());
            }
            (STATE_ACTIVE, "true".to_string(), true)
        }
    };
    if restore {
        let original_bool = existing
            .as_ref()
            .map(stored_original_bool)
            .transpose()?
            .flatten();
        restore_codex_value(&mut document, &source_path, &config_path, original_bool)?;
    } else {
        set_codex_value(&mut document, &source_path, &config_path, false)?;
    }
    let text = document.to_string();
    write_atomic(&config_path, &text)?;

    let verification = (|| -> Result<(), String> {
        let verify = read_file_if_exists(&config_path)?;
        let verify_document = verify
            .as_deref()
            .unwrap_or("")
            .parse::<DocumentMut>()
            .map_err(|error| format!("Codex config.toml을 다시 읽을 수 없습니다: {error}"))?;
        let verify_value = codex_current_value(&verify_document, &source_path, &config_path)?;
        let expected_value = if restore {
            existing
                .as_ref()
                .map(stored_original_bool)
                .transpose()?
                .flatten()
        } else {
            Some(false)
        };
        if verify_value != expected_value {
            return Err("Codex 설정을 다시 읽었을 때 요청한 상태가 아닙니다".to_string());
        }
        Ok(())
    })();
    if let Err(error) = verification {
        let rollback = restore_file(&config_path, before.as_deref());
        return Err(match rollback {
            Ok(()) => format!("Codex 설정 검증에 실패해 변경을 되돌렸습니다: {error}"),
            Err(rollback_error) => format!(
                "Codex 설정 검증에 실패했고 되돌리기도 실패했습니다: {error}; {rollback_error}"
            ),
        });
    }

    let db_result = if restore {
        db::delete_platform_skill_control(pool, &agent.id, &source_path).await
    } else {
        db::upsert_platform_skill_control(
            pool,
            &StoredControl {
                agent_id: agent.id.clone(),
                source_path: source_path.clone(),
                skill_name: skill.name.clone(),
                state: state.to_string(),
                original_value: original,
                applied_value: applied,
                updated_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
    };
    if let Err(error) = db_result {
        let rollback = restore_file(&config_path, before.as_deref());
        return Err(match rollback {
            Ok(()) => format!("플랫폼 설정은 되돌렸지만 제어 기록을 저장하지 못했습니다: {error}"),
            Err(rollback_error) => format!(
                "플랫폼 설정과 제어 기록 저장이 실패했고 되돌리기도 실패했습니다: {error}; {rollback_error}"
            ),
        });
    }
    Ok(())
}

async fn execute_platform_skill_control_impl(
    pool: &DbPool,
    agent_id: &str,
    skill_id: &str,
    skill_name: &str,
    source_path: &str,
    action: ExternalAction,
) -> Result<(), String> {
    let agent = db::get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| format!("플랫폼 '{}'을(를) 찾을 수 없습니다", agent_id))?;
    let skills = skills::get_skills_by_agent_impl(pool, agent_id).await?;
    let skill = find_target_skill(&skills, skill_id, skill_name, source_path)?;
    let installations = db::get_skill_installations_by_agent(pool, agent_id).await?;
    let paused = db::get_paused_installations_by_agent(pool, agent_id).await?;
    if is_managed_installation(&installations, &paused, &skill).is_some() {
        match action {
            ExternalAction::Disable => {
                usage::set_skill_usage_impl(pool, skill_id, agent_id, false).await
            }
            ExternalAction::Enable => {
                usage::set_skill_usage_impl(pool, skill_id, agent_id, true).await
            }
            ExternalAction::Delete | ExternalAction::Reapply => {
                if action == ExternalAction::Reapply {
                    usage::set_skill_usage_impl(pool, skill_id, agent_id, true).await
                } else {
                    usage::delete_skill_from_agent_impl(pool, skill_id, agent_id).await
                }
            }
        }
    } else {
        execute_external_action(pool, &agent, &skill, &skills, action).await
    }
}

#[tauri::command]
pub async fn get_platform_skill_controls(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<Vec<PlatformSkillControlStatus>, String> {
    get_platform_skill_controls_impl(&state.db, &agent_id).await
}

#[tauri::command]
pub async fn set_platform_skill_control(
    state: State<'_, AppState>,
    agent_id: String,
    skill_id: String,
    skill_name: String,
    source_path: String,
    enabled: bool,
) -> Result<(), String> {
    let _guard = control_lock().await;
    execute_platform_skill_control_impl(
        &state.db,
        &agent_id,
        &skill_id,
        &skill_name,
        &source_path,
        if enabled {
            ExternalAction::Enable
        } else {
            ExternalAction::Disable
        },
    )
    .await
}

#[tauri::command]
pub async fn delete_platform_skill_control(
    state: State<'_, AppState>,
    agent_id: String,
    skill_id: String,
    skill_name: String,
    source_path: String,
) -> Result<(), String> {
    let _guard = control_lock().await;
    execute_platform_skill_control_impl(
        &state.db,
        &agent_id,
        &skill_id,
        &skill_name,
        &source_path,
        ExternalAction::Delete,
    )
    .await
}

#[tauri::command]
pub async fn reapply_platform_skill_control(
    state: State<'_, AppState>,
    agent_id: String,
    skill_id: String,
    skill_name: String,
    source_path: String,
) -> Result<(), String> {
    let _guard = control_lock().await;
    execute_platform_skill_control_impl(
        &state.db,
        &agent_id,
        &skill_id,
        &skill_name,
        &source_path,
        ExternalAction::Reapply,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_agent(id: &str, display_name: &str, global_skills_dir: &Path) -> Agent {
        Agent {
            id: id.to_string(),
            display_name: display_name.to_string(),
            category: "coding".to_string(),
            global_skills_dir: global_skills_dir.to_string_lossy().into_owned(),
            project_skills_dir: None,
            icon_name: None,
            is_detected: true,
            is_builtin: true,
            is_enabled: true,
        }
    }

    fn test_skill(id: &str, name: &str, skill_dir: &Path, source_kind: &str) -> db::SkillForAgent {
        let file_path = skill_dir.join("SKILL.md");
        fs::create_dir_all(skill_dir).unwrap();
        fs::write(&file_path, format!("---\nname: {name}\n---\n")).unwrap();
        db::SkillForAgent {
            id: id.to_string(),
            row_id: format!("row-{id}"),
            name: name.to_string(),
            description: Some("임시 테스트 스킬".to_string()),
            file_path: file_path.to_string_lossy().into_owned(),
            dir_path: skill_dir.to_string_lossy().into_owned(),
            link_type: "native".to_string(),
            symlink_target: None,
            is_central: false,
            source_kind: Some(source_kind.to_string()),
            source_root: Some(skill_dir.to_string_lossy().into_owned()),
            source_label: None,
            is_read_only: true,
            conflict_group: None,
            conflict_count: 0,
        }
    }

    async fn test_pool(directory: &TempDir) -> db::DbPool {
        let path = directory.path().join("controls.sqlite");
        let pool = db::create_pool(path.to_str().unwrap()).await.unwrap();
        db::init_database(&pool).await.unwrap();
        pool
    }

    async fn register_test_observation(
        pool: &db::DbPool,
        agent: &Agent,
        skill: &db::SkillForAgent,
    ) {
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = ?")
            .bind(&agent.global_skills_dir)
            .bind(&agent.id)
            .execute(pool)
            .await
            .unwrap();
        db::upsert_agent_skill_observation(
            pool,
            &db::AgentSkillObservation {
                row_id: skill.row_id.clone(),
                agent_id: agent.id.clone(),
                skill_id: skill.id.clone(),
                name: skill.name.clone(),
                description: skill.description.clone(),
                file_path: skill.file_path.clone(),
                dir_path: skill.dir_path.clone(),
                source_kind: skill
                    .source_kind
                    .clone()
                    .unwrap_or_else(|| "compatibility".to_string()),
                source_root: skill.source_root.clone().unwrap_or_default(),
                source_label: skill.source_label.clone(),
                link_type: skill.link_type.clone(),
                symlink_target: skill.symlink_target.clone(),
                is_read_only: skill.is_read_only,
                scanned_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
    }

    #[test]
    fn claude_override_can_be_disabled_and_restored_without_touching_other_settings() {
        let mut document: JsonValue = serde_json::json!({
            "theme": "dark",
            "skillOverrides": {"other-skill": "on"}
        });

        let original = json_override(&document, "shared-skill").unwrap();
        set_json_override(&mut document, "shared-skill", Some(CLAUDE_DISABLED_VALUE)).unwrap();
        assert_eq!(
            json_state(json_override(&document, "shared-skill").unwrap().as_deref()).unwrap(),
            STATE_INACTIVE
        );
        assert_eq!(document["theme"], "dark");
        assert_eq!(
            json_override(&document, "other-skill").unwrap().as_deref(),
            Some("on")
        );

        set_json_override(&mut document, "shared-skill", original.as_deref()).unwrap();
        assert_eq!(
            json_state(json_override(&document, "shared-skill").unwrap().as_deref()).unwrap(),
            STATE_ACTIVE
        );
    }

    #[test]
    fn codex_config_restores_a_new_entry_without_removing_unrelated_entries() {
        let config_path = Path::new("/tmp/skillsmanage-test-config.toml");
        let source_path = "/tmp/skills/shared-skill";
        let mut parsed = "# keep this comment\n[other]\nvalue = \"kept\"\n"
            .parse::<DocumentMut>()
            .unwrap();

        assert_eq!(
            codex_current_value(&parsed, source_path, config_path).unwrap(),
            None
        );
        set_codex_value(&mut parsed, source_path, config_path, false).unwrap();
        assert_eq!(
            codex_current_value(&parsed, source_path, config_path).unwrap(),
            Some(false)
        );
        let rendered = parsed.to_string();
        assert!(rendered.contains("value = \"kept\""));
        assert!(rendered.contains("# keep this comment"));

        restore_codex_value(&mut parsed, source_path, config_path, None).unwrap();
        assert_eq!(
            codex_current_value(&parsed, source_path, config_path).unwrap(),
            None
        );
        assert!(parsed.to_string().contains("value = \"kept\""));
    }

    #[cfg(unix)]
    #[test]
    fn codex_matching_accepts_a_symlinked_skill_directory_and_file_config_path() {
        use std::os::unix::fs::symlink;

        let directory = TempDir::new().unwrap();
        let actual = directory.path().join("actual-skill");
        let alias = directory.path().join("linked-skill");
        fs::create_dir_all(&actual).unwrap();
        fs::write(actual.join("SKILL.md"), "---\nname: linked\n---\n").unwrap();
        symlink(&actual, &alias).unwrap();
        let config_path = directory.path().join(".codex/config.toml");
        let config = format!(
            "[[skills.config]]\npath = \"{}\"\nenabled = false\n",
            actual.join("SKILL.md").display()
        );
        let parsed = config.parse::<DocumentMut>().unwrap();

        assert_eq!(
            codex_current_value(&parsed, &alias.to_string_lossy(), &config_path).unwrap(),
            Some(false)
        );
    }

    #[test]
    fn atomic_setting_write_and_rollback_stay_in_a_temporary_directory() {
        let directory =
            std::env::temp_dir().join(format!("skillsmanage-control-test-{}", Uuid::new_v4()));
        let path = directory.join("settings.json");
        write_atomic(&path, "{\"value\":1}").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"value\":1}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        restore_file(&path, None).unwrap();
        assert!(!path.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn atomic_setting_write_does_not_replace_a_symlinked_config() {
        use std::os::unix::fs::symlink;

        let directory = TempDir::new().unwrap();
        let target = directory.path().join("real-settings.json");
        let link = directory.path().join("settings.json");
        fs::write(&target, "{\"value\":1}").unwrap();
        symlink(&target, &link).unwrap();

        assert!(write_atomic(&link, "{\"value\":2}").is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "{\"value\":1}");
    }

    #[tokio::test]
    async fn claude_adapter_round_trips_disable_delete_reapply_and_preserves_original_value() {
        let directory = TempDir::new().unwrap();
        let claude_root = directory.path().join(".claude");
        let skills_root = claude_root.join("skills");
        let skill = test_skill(
            "shared-skill",
            "shared-skill",
            &skills_root.join("shared-skill"),
            "compatibility",
        );
        let agent = test_agent("claude-code", "Claude Code", &skills_root);
        let settings_path = claude_root.join("settings.json");
        fs::create_dir_all(&claude_root).unwrap();
        fs::write(&settings_path, r#"{"theme":"dark"}"#).unwrap();
        // Claude 설정을 바꿀 때 같은 임시 홈의 다른 플랫폼 설정은 건드리지 않아야 한다.
        let other_platform_settings = directory.path().join(".codex/config.toml");
        fs::create_dir_all(other_platform_settings.parent().unwrap()).unwrap();
        let other_platform_original = "[other]\nvalue = \"kept\"\n";
        fs::write(&other_platform_settings, other_platform_original).unwrap();
        let pool = test_pool(&directory).await;
        register_test_observation(&pool, &agent, &skill).await;

        execute_claude_action(
            &pool,
            &agent,
            &skill,
            std::slice::from_ref(&skill),
            &[],
            skill.dir_path.clone(),
            None,
            ExternalAction::Disable,
        )
        .await
        .unwrap();
        let disabled: JsonValue =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(disabled["skillOverrides"]["shared-skill"], "off");
        assert_eq!(
            fs::read_to_string(&other_platform_settings).unwrap(),
            other_platform_original
        );
        let controls = db::get_platform_skill_controls(&pool, "claude-code")
            .await
            .unwrap();
        assert_eq!(controls.len(), 1);
        assert_eq!(controls[0].state, STATE_INACTIVE);
        assert_eq!(controls[0].original_value, None);
        assert_eq!(controls[0].applied_value, "off");
        assert_eq!(
            get_platform_skill_controls_impl(&pool, "claude-code")
                .await
                .unwrap()[0]
                .state,
            STATE_INACTIVE
        );
        let status =
            actual_external_status_at_path(&agent, &skill, controls.first(), 1, None).await;
        assert_eq!(status.state, STATE_INACTIVE);

        let existing = controls[0].clone();
        execute_claude_action(
            &pool,
            &agent,
            &skill,
            std::slice::from_ref(&skill),
            &controls,
            skill.dir_path.clone(),
            Some(existing.clone()),
            ExternalAction::Delete,
        )
        .await
        .unwrap();
        let deleted_controls = db::get_platform_skill_controls(&pool, "claude-code")
            .await
            .unwrap();
        assert_eq!(deleted_controls[0].state, STATE_DELETED);
        assert_eq!(deleted_controls[0].original_value, None);

        execute_claude_action(
            &pool,
            &agent,
            &skill,
            std::slice::from_ref(&skill),
            &deleted_controls,
            skill.dir_path.clone(),
            Some(deleted_controls[0].clone()),
            ExternalAction::Reapply,
        )
        .await
        .unwrap();
        let reapplied: JsonValue =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert!(reapplied
            .get("skillOverrides")
            .and_then(|v| v.get("shared-skill"))
            .is_none());
        assert_eq!(
            fs::read_to_string(&other_platform_settings).unwrap(),
            other_platform_original
        );
        assert!(db::get_platform_skill_controls(&pool, "claude-code")
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            get_platform_skill_controls_impl(&pool, "claude-code")
                .await
                .unwrap()[0]
                .state,
            STATE_ACTIVE
        );
        let status = actual_external_status_at_path(&agent, &skill, None, 1, None).await;
        assert_eq!(status.state, STATE_ACTIVE);

        // 이미 있던 값은 비활성화 후 그대로 복원한다.
        fs::write(
            &settings_path,
            r#"{"theme":"dark","skillOverrides":{"shared-skill":"name-only"}}"#,
        )
        .unwrap();
        execute_claude_action(
            &pool,
            &agent,
            &skill,
            std::slice::from_ref(&skill),
            &[],
            skill.dir_path.clone(),
            None,
            ExternalAction::Disable,
        )
        .await
        .unwrap();
        let controls = db::get_platform_skill_controls(&pool, "claude-code")
            .await
            .unwrap();
        assert_eq!(controls[0].original_value.as_deref(), Some("name-only"));
        execute_claude_action(
            &pool,
            &agent,
            &skill,
            std::slice::from_ref(&skill),
            &controls,
            skill.dir_path.clone(),
            Some(controls[0].clone()),
            ExternalAction::Enable,
        )
        .await
        .unwrap();
        let restored: JsonValue =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(restored["skillOverrides"]["shared-skill"], "name-only");
    }

    #[tokio::test]
    async fn claude_adapter_rolls_back_setting_when_database_record_fails() {
        let directory = TempDir::new().unwrap();
        let claude_root = directory.path().join(".claude");
        let skills_root = claude_root.join("skills");
        let skill = test_skill(
            "rollback-skill",
            "rollback-skill",
            &skills_root.join("rollback-skill"),
            "compatibility",
        );
        let agent = test_agent("claude-code", "Claude Code", &skills_root);
        let settings_path = claude_root.join("settings.json");
        fs::create_dir_all(&claude_root).unwrap();
        let original = r#"{"theme":"dark"}"#;
        fs::write(&settings_path, original).unwrap();
        let pool = test_pool(&directory).await;
        sqlx::query(
            "CREATE TRIGGER fail_platform_control_insert
             BEFORE INSERT ON platform_skill_controls
             BEGIN SELECT RAISE(ABORT, 'intentional test failure'); END",
        )
        .execute(&pool)
        .await
        .unwrap();

        let result = execute_claude_action(
            &pool,
            &agent,
            &skill,
            std::slice::from_ref(&skill),
            &[],
            skill.dir_path.clone(),
            None,
            ExternalAction::Disable,
        )
        .await;
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&settings_path).unwrap(), original);
        assert!(db::get_platform_skill_controls(&pool, "claude-code")
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn codex_adapter_uses_codex_home_layout_and_canonical_skill_file_path() {
        let directory = TempDir::new().unwrap();
        let agents_root = directory.path().join(".agents");
        let skills_root = agents_root.join("skills");
        let skill = test_skill(
            "codex-skill",
            "codex-skill",
            &skills_root.join("codex-skill"),
            "compatibility",
        );
        let agent = test_agent("codex", "Codex", &skills_root);
        let config_path = directory.path().join(".codex/config.toml");
        fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        fs::write(&config_path, "# keep\n[other]\nvalue = \"kept\"\n").unwrap();
        let pool = test_pool(&directory).await;
        register_test_observation(&pool, &agent, &skill).await;

        execute_codex_action_at_path(
            &pool,
            &agent,
            &skill,
            codex_source_path(&skill),
            config_path.clone(),
            None,
            ExternalAction::Disable,
        )
        .await
        .unwrap();
        let disabled = fs::read_to_string(&config_path).unwrap();
        assert!(disabled.contains("enabled = false"));
        assert!(disabled.contains("SKILL.md"));
        assert!(disabled.contains("value = \"kept\""));
        let controls = db::get_platform_skill_controls(&pool, "codex")
            .await
            .unwrap();
        assert_eq!(controls[0].state, STATE_INACTIVE);
        assert_eq!(
            get_platform_skill_controls_impl_at_path(&pool, "codex", Some(&config_path))
                .await
                .unwrap()[0]
                .state,
            STATE_INACTIVE
        );
        let status =
            actual_external_status_at_path(&agent, &skill, controls.first(), 1, Some(&config_path))
                .await;
        assert_eq!(status.state, STATE_INACTIVE);

        execute_codex_action_at_path(
            &pool,
            &agent,
            &skill,
            codex_source_path(&skill),
            config_path.clone(),
            Some(controls[0].clone()),
            ExternalAction::Delete,
        )
        .await
        .unwrap();
        let deleted = db::get_platform_skill_controls(&pool, "codex")
            .await
            .unwrap();
        assert_eq!(deleted[0].state, STATE_DELETED);
        let deleted_status =
            get_platform_skill_controls_impl_at_path(&pool, "codex", Some(&config_path))
                .await
                .unwrap();
        assert_eq!(deleted_status[0].state, STATE_DELETED);
        assert!(deleted_status[0].can_reapply);
        execute_codex_action_at_path(
            &pool,
            &agent,
            &skill,
            codex_source_path(&skill),
            config_path.clone(),
            Some(deleted[0].clone()),
            ExternalAction::Reapply,
        )
        .await
        .unwrap();
        assert!(!fs::read_to_string(&config_path)
            .unwrap()
            .contains("codex-skill"));
        assert!(db::get_platform_skill_controls(&pool, "codex")
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            get_platform_skill_controls_impl_at_path(&pool, "codex", Some(&config_path))
                .await
                .unwrap()[0]
                .state,
            STATE_ACTIVE
        );
    }
    #[tokio::test]
    async fn shared_install_shown_for_shared_entry_and_hidden_for_singleton() {
        let dir = TempDir::new().unwrap();
        let pool = test_pool(&dir).await;
        let shared_root = dir.path().join("agents-skills");
        fs::create_dir_all(&shared_root).unwrap();
        for id in ["universal", "codex"] {
            sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = ?")
                .bind(shared_root.to_string_lossy().to_string())
                .bind(id)
                .execute(&pool)
                .await
                .unwrap();
        }
        let shared_dir = shared_root.join("shared-card");
        fs::create_dir_all(&shared_dir).unwrap();
        fs::write(shared_dir.join("SKILL.md"), "---\nname: shared-card\n---\n").unwrap();
        let solo_dir = shared_root.join("solo-card");
        fs::create_dir_all(&solo_dir).unwrap();
        fs::write(solo_dir.join("SKILL.md"), "---\nname: solo-card\n---\n").unwrap();
        for (skill_id, d) in [("shared-card", &shared_dir), ("solo-card", &solo_dir)] {
            db::upsert_skill(
                &pool,
                &db::Skill {
                    id: skill_id.to_string(),
                    name: skill_id.to_string(),
                    description: None,
                    file_path: d.join("SKILL.md").to_string_lossy().into_owned(),
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
        for (skill_id, agent) in [
            ("shared-card", "universal"),
            ("shared-card", "codex"),
            ("solo-card", "codex"),
        ] {
            let d = shared_root.join(skill_id);
            db::upsert_skill_installation(
                &pool,
                &db::SkillInstallation {
                    skill_id: skill_id.to_string(),
                    agent_id: agent.to_string(),
                    installed_path: d.to_string_lossy().into_owned(),
                    link_type: "copy".to_string(),
                    symlink_target: None,
                    created_at: chrono::Utc::now().to_rfc3339(),
                },
            )
            .await
            .unwrap();
        }
        let statuses = get_platform_skill_controls_impl(&pool, "codex")
            .await
            .unwrap();
        let shared = statuses
            .iter()
            .find(|s| s.skill_id == "shared-card")
            .unwrap();
        assert!(shared.shared_install.is_some());
        assert!(!shared.excluded_here);
        let solo = statuses.iter().find(|s| s.skill_id == "solo-card").unwrap();
        assert!(solo.shared_install.is_none());
        assert!(!solo.excluded_here);
    }

    #[tokio::test]
    async fn paused_shared_card_survives_observation_prune() {
        let dir = TempDir::new().unwrap();
        let pool = test_pool(&dir).await;
        let root = dir.path().join("u");
        fs::create_dir_all(&root).unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'universal'")
            .bind(root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        let skill_dir = root.join("kept-card");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "---\nname: kept-card\n---\n").unwrap();
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "kept-card".to_string(),
                name: "kept-card".to_string(),
                description: None,
                file_path: skill_dir.join("SKILL.md").to_string_lossy().into_owned(),
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
            &db::SkillInstallation {
                skill_id: "kept-card".to_string(),
                agent_id: "universal".to_string(),
                installed_path: skill_dir.to_string_lossy().into_owned(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        // Pause through the shared path, then simulate a rescan that pruned
        // the observation row entirely.
        let key = usage::shared_entry_key(&skill_dir.to_string_lossy());
        // Universal-only entries are observed-only-manageable in some setups;
        // force a managed pause here by adding a counted sharer.
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'codex'")
            .bind(root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        db::upsert_skill_installation(
            &pool,
            &db::SkillInstallation {
                skill_id: "kept-card".to_string(),
                agent_id: "codex".to_string(),
                installed_path: skill_dir.to_string_lossy().into_owned(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        let impact = usage::compute_shared_impact(&pool, &key).await.unwrap();
        assert!(impact.reason.is_none());
        usage::set_shared_skill_usage_impl(&pool, &key, false, &impact.confirmation_token)
            .await
            .unwrap();
        db::delete_stale_agent_skill_observations(&pool, "codex", &[])
            .await
            .unwrap();
        let statuses = get_platform_skill_controls_impl(&pool, "codex")
            .await
            .unwrap();
        let card = statuses.iter().find(|s| s.skill_id == "kept-card").unwrap();
        assert_eq!(card.state, "inactive");
        assert!(!card.excluded_here);
        assert!(card.shared_install.is_some());
    }

    #[tokio::test]
    async fn universal_only_entry_exposes_shared_impact_for_bulk_scope() {
        let dir = TempDir::new().unwrap();
        let pool = test_pool(&dir).await;
        let root = dir.path().join("u-only");
        fs::create_dir_all(&root).unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'universal'")
            .bind(root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        let skill_dir = root.join("solo-universal");
        fs::create_dir_all(&skill_dir).unwrap();
        fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: solo-universal\n---\n",
        )
        .unwrap();
        db::upsert_skill(
            &pool,
            &db::Skill {
                id: "solo-universal".to_string(),
                name: "solo-universal".to_string(),
                description: None,
                file_path: skill_dir.join("SKILL.md").to_string_lossy().into_owned(),
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
            &db::SkillInstallation {
                skill_id: "solo-universal".to_string(),
                agent_id: "universal".to_string(),
                installed_path: skill_dir.to_string_lossy().into_owned(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: chrono::Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();
        let statuses = get_platform_skill_controls_impl(&pool, "universal")
            .await
            .unwrap();
        let card = statuses
            .iter()
            .find(|s| s.skill_id == "solo-universal")
            .unwrap();
        assert!(card.shared_install.is_some());
        assert_eq!(
            card.shared_install.as_ref().unwrap().shared_install_id,
            usage::shared_entry_key(&skill_dir.to_string_lossy())
        );
    }
}

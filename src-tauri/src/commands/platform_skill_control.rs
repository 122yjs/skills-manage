//! 플랫폼별 외부 스킬 사용 제어.
//!
//! 관리 설치는 `usage` 모듈의 이동·복원 로직을 그대로 사용한다. 이 모듈은
//! 공용 폴더나 호환 경로처럼 앱이 파일을 소유하지 않는 관측을 플랫폼의
//! 공식 설정으로 제어할 때만 사용한다. 공식 설정을 확인하지 못한 플랫폼은
//! 파일을 옮기거나 DB에만 성공을 기록하지 않고 제한 사유를 반환한다.

use glob::Pattern;
use regex::Regex;
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
use toml_edit::{value, Array, ArrayOfTables, DocumentMut, Item, Table};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Adapter {
    ClaudeSkillOverrides,
    CodeBuddySkillOverrides,
    CodexSkillsConfig,
    FactoryDisabledSkills,
    CommandCodeDisabledSkills,
    MistralDisabledSkills,
    OpenCodeSkillPermissions,
    OmpIgnoredSkills,
    HermesDisabledSkills,
    OpenClawEntries,
    ManagedInstallation,
    Unsupported,
}

impl Adapter {
    fn name(self) -> &'static str {
        match self {
            Self::ClaudeSkillOverrides => "claude-skill-overrides",
            Self::CodeBuddySkillOverrides => "codebuddy-skill-overrides",
            Self::CodexSkillsConfig => "codex-skills-config",
            Self::FactoryDisabledSkills => "factory-disabled-skills",
            Self::CommandCodeDisabledSkills => "command-code-disabled-skills",
            Self::MistralDisabledSkills => "mistral-disabled-skills",
            Self::OpenCodeSkillPermissions => "opencode-skill-permissions",
            Self::OmpIgnoredSkills => "omp-ignored-skills",
            Self::HermesDisabledSkills => "hermes-disabled-skills",
            Self::OpenClawEntries => "openclaw-skill-entries",
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
    if matches!(agent.id.as_str(), "claude-code" | "codebuddy") && source_kind == Some("plugin") {
        return format!(
            "{}의 skillOverrides는 플러그인 스킬에 적용되지 않습니다. 플러그인 관리에서 끄거나 삭제해야 합니다.",
            agent.display_name
        );
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
        "codebuddy" if skill.source_kind.as_deref() != Some("plugin") => {
            Adapter::CodeBuddySkillOverrides
        }
        "codex" => Adapter::CodexSkillsConfig,
        "factory-droid" => Adapter::FactoryDisabledSkills,
        "command-code" => Adapter::CommandCodeDisabledSkills,
        "mistral-vibe" => Adapter::MistralDisabledSkills,
        "opencode" => Adapter::OpenCodeSkillPermissions,
        "omp" => Adapter::OmpIgnoredSkills,
        "hermes" => Adapter::HermesDisabledSkills,
        "openclaw" => Adapter::OpenClawEntries,
        _ => Adapter::Unsupported,
    }
}

fn is_name_scoped_adapter(adapter: Adapter) -> bool {
    matches!(
        adapter,
        Adapter::ClaudeSkillOverrides
            | Adapter::CodeBuddySkillOverrides
            | Adapter::FactoryDisabledSkills
            | Adapter::CommandCodeDisabledSkills
            | Adapter::MistralDisabledSkills
            | Adapter::OpenCodeSkillPermissions
            | Adapter::OmpIgnoredSkills
            | Adapter::HermesDisabledSkills
            | Adapter::OpenClawEntries
    )
}

fn config_path_for_claude(agent: &Agent) -> Result<PathBuf, String> {
    let root = Path::new(&agent.global_skills_dir)
        .parent()
        .ok_or_else(|| "Claude Code 스킬 설정 폴더를 확인할 수 없습니다".to_string())?;
    // settings.local.json은 프로젝트·로컬 우선순위 설정이라 전역 스킬 제어
    // 대상이라고 확인하지 않았다. 공식 사용자 설정 파일만 사용한다.
    Ok(root.join("settings.json"))
}

fn config_path_for_codebuddy(agent: &Agent) -> Result<PathBuf, String> {
    let root = Path::new(&agent.global_skills_dir)
        .parent()
        .ok_or_else(|| "CodeBuddy 스킬 설정 폴더를 확인할 수 없습니다".to_string())?;
    Ok(root.join("settings.json"))
}

fn config_path_in_skills_parent(
    agent: &Agent,
    platform_name: &str,
    file_name: &str,
) -> Result<PathBuf, String> {
    Path::new(&agent.global_skills_dir)
        .parent()
        .map(|root| root.join(file_name))
        .ok_or_else(|| format!("{platform_name} 스킬 설정 폴더를 확인할 수 없습니다"))
}

fn config_path_for_disabled_skills_json(
    agent: &Agent,
    adapter: Adapter,
) -> Result<PathBuf, String> {
    match adapter {
        Adapter::FactoryDisabledSkills => {
            config_path_in_skills_parent(agent, "Factory Droid", "settings.json")
        }
        Adapter::CommandCodeDisabledSkills => {
            config_path_in_skills_parent(agent, "Command Code", "settings.json")
        }
        _ => Err("이 Adapter는 disabledSkills JSON 설정을 사용하지 않습니다".to_string()),
    }
}

fn config_path_for_omp(agent: &Agent) -> Result<PathBuf, String> {
    let root = Path::new(&agent.global_skills_dir)
        .parent()
        .ok_or_else(|| "OMP 스킬 설정 폴더를 확인할 수 없습니다".to_string())?;
    let yml = root.join("config.yml");
    let yaml = root.join("config.yaml");
    match (yml.exists(), yaml.exists()) {
        (true, true) => Err(
            "OMP config.yml과 config.yaml이 모두 있어 적용 대상을 결정할 수 없습니다".to_string(),
        ),
        (false, true) => Ok(yaml),
        _ => Ok(yml),
    }
}

fn config_path_for_hermes(agent: &Agent) -> Result<PathBuf, String> {
    config_path_in_skills_parent(agent, "Hermes", "config.yaml")
}

fn config_path_for_mistral(agent: &Agent) -> Result<PathBuf, String> {
    if Path::new(&agent.global_skills_dir).starts_with(resolve_home_dir()) {
        if let Some(vibe_home) = std::env::var_os("VIBE_HOME") {
            if !vibe_home.is_empty() {
                return Ok(PathBuf::from(vibe_home).join("config.toml"));
            }
        }
    }
    config_path_in_skills_parent(agent, "Mistral Vibe", "config.toml")
}

fn config_path_for_opencode(agent: &Agent) -> Result<PathBuf, String> {
    if Path::new(&agent.global_skills_dir).starts_with(resolve_home_dir()) {
        if let Some(config) = std::env::var_os("OPENCODE_CONFIG") {
            if !config.is_empty() {
                return Ok(PathBuf::from(config));
            }
        }
    }
    let skills_parent = Path::new(&agent.global_skills_dir)
        .parent()
        .ok_or_else(|| "OpenCode 스킬 설정 폴더를 확인할 수 없습니다".to_string())?;
    let root = if skills_parent.file_name().and_then(|name| name.to_str()) == Some("opencode")
        && skills_parent
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            == Some(".config")
    {
        skills_parent.to_path_buf()
    } else {
        skills_parent
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(".config/opencode")
    };
    let json = root.join("opencode.json");
    let jsonc = root.join("opencode.jsonc");
    match (json.exists(), jsonc.exists()) {
        (true, true) => Err(
            "OpenCode opencode.json과 opencode.jsonc가 모두 있어 적용 대상을 결정할 수 없습니다"
                .to_string(),
        ),
        (false, true) => Ok(jsonc),
        _ => Ok(json),
    }
}

fn config_path_for_openclaw(agent: &Agent) -> Result<PathBuf, String> {
    let root = Path::new(&agent.global_skills_dir)
        .parent()
        .ok_or_else(|| "OpenClaw 스킬 설정 폴더를 확인할 수 없습니다".to_string())?;
    Ok(root.join("openclaw.json"))
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

fn strict_json_document(text: Option<&str>, label: &str) -> Result<JsonValue, String> {
    match text {
        None => Ok(JsonValue::Object(JsonMap::new())),
        Some(text) => serde_json::from_str(text)
            .map_err(|error| format!("{label} 설정 JSON을 읽을 수 없습니다: {error}")),
    }
}

fn json_document(text: Option<&str>) -> Result<JsonValue, String> {
    strict_json_document(text, "skillOverrides")
}

fn json_override(document: &JsonValue, skill_name: &str) -> Result<Option<String>, String> {
    let Some(overrides) = document.get("skillOverrides") else {
        return Ok(None);
    };
    let Some(overrides) = overrides.as_object() else {
        return Err("skillOverrides 설정이 객체가 아닙니다".to_string());
    };
    let Some(value) = overrides.get(skill_name) else {
        return Ok(None);
    };
    value
        .as_str()
        .map(|value| Some(value.to_string()))
        .ok_or_else(|| format!("skillOverrides['{skill_name}'] 값이 문자열이 아닙니다"))
}

fn set_json_override(
    document: &mut JsonValue,
    skill_name: &str,
    value: Option<&str>,
) -> Result<(), String> {
    let object = document
        .as_object_mut()
        .ok_or_else(|| "skillOverrides 설정의 최상위 값이 객체가 아닙니다".to_string())?;
    if let Some(value) = value {
        let overrides = object
            .entry("skillOverrides".to_string())
            .or_insert_with(|| JsonValue::Object(JsonMap::new()));
        let overrides = overrides
            .as_object_mut()
            .ok_or_else(|| "skillOverrides 설정이 객체가 아닙니다".to_string())?;
        overrides.insert(skill_name.to_string(), JsonValue::String(value.to_string()));
    } else if let Some(overrides) = object.get_mut("skillOverrides") {
        let overrides = overrides
            .as_object_mut()
            .ok_or_else(|| "skillOverrides 설정이 객체가 아닙니다".to_string())?;
        overrides.remove(skill_name);
    }
    Ok(())
}

fn json_string_list(document: &JsonValue, key: &str, label: &str) -> Result<Vec<String>, String> {
    let Some(values) = document.get(key) else {
        return Ok(Vec::new());
    };
    let Some(values) = values.as_array() else {
        return Err(format!("{label} 설정이 문자열 목록이 아닙니다"));
    };
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("{label} 설정에 문자열이 아닌 값이 있습니다"))
        })
        .collect()
}

fn set_json_string_list_item(
    document: &mut JsonValue,
    key: &str,
    skill_name: &str,
    present: bool,
    label: &str,
) -> Result<(), String> {
    let object = document
        .as_object_mut()
        .ok_or_else(|| format!("{label} 설정의 최상위 값이 객체가 아닙니다"))?;
    let values = object
        .entry(key.to_string())
        .or_insert_with(|| JsonValue::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| format!("{label} 설정이 문자열 목록이 아닙니다"))?;
    if values.iter().any(|value| !value.is_string()) {
        return Err(format!("{label} 설정에 문자열이 아닌 값이 있습니다"));
    }
    let has_exact = values
        .iter()
        .any(|value| value.as_str() == Some(skill_name));
    if present && !has_exact {
        values.push(JsonValue::String(skill_name.to_string()));
    } else if !present && has_exact {
        values.retain(|value| value.as_str() != Some(skill_name));
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

fn name_override_config_path(agent: &Agent, adapter: Adapter) -> Result<PathBuf, String> {
    match adapter {
        Adapter::ClaudeSkillOverrides => config_path_for_claude(agent),
        Adapter::CodeBuddySkillOverrides => config_path_for_codebuddy(agent),
        _ => Err("이 Adapter는 skillOverrides 설정을 사용하지 않습니다".to_string()),
    }
}

fn name_override_state(adapter: Adapter, value: Option<&str>) -> Result<&'static str, String> {
    match adapter {
        Adapter::ClaudeSkillOverrides => json_state(value),
        Adapter::CodeBuddySkillOverrides => match value {
            None | Some("on") => Ok(STATE_ACTIVE),
            Some(CLAUDE_DISABLED_VALUE) => Ok(STATE_INACTIVE),
            Some(value) => Err(format!(
                "CodeBuddy skillOverrides의 확인되지 않은 값입니다: {value}"
            )),
        },
        _ => Err("이 Adapter는 skillOverrides 설정을 사용하지 않습니다".to_string()),
    }
}

fn yaml_string_list(
    text: Option<&str>,
    section_key: &str,
    list_key: &str,
    label: &str,
) -> Result<Vec<String>, String> {
    let Some(text) = text else {
        return Ok(Vec::new());
    };
    let document: serde_yaml::Value = serde_yaml::from_str(text)
        .map_err(|error| format!("{label} 설정 YAML을 읽을 수 없습니다: {error}"))?;
    let Some(section) = document.get(section_key) else {
        return Ok(Vec::new());
    };
    let Some(section) = section.as_mapping() else {
        return Err(format!("{label} 설정의 {section_key}가 객체가 아닙니다"));
    };
    let Some(values) = section.get(serde_yaml::Value::String(list_key.to_string())) else {
        return Ok(Vec::new());
    };
    let Some(values) = values.as_sequence() else {
        return Err(format!(
            "{label} 설정의 {section_key}.{list_key}가 문자열 목록이 아닙니다"
        ));
    };
    values
        .iter()
        .map(|value| {
            value.as_str().map(str::to_string).ok_or_else(|| {
                format!("{label} 설정의 {section_key}.{list_key}에 문자열이 아닌 값이 있습니다")
            })
        })
        .collect()
}

fn omp_ignored_skills(text: Option<&str>) -> Result<Vec<String>, String> {
    yaml_string_list(text, "skills", "ignoredSkills", "OMP")
}

fn hermes_disabled_skills(text: Option<&str>) -> Result<Vec<String>, String> {
    yaml_string_list(text, "skills", "disabled", "Hermes")
}

fn omp_ignore_state(patterns: &[String], skill_name: &str) -> Result<&'static str, String> {
    pattern_disabled_state(patterns, skill_name, "OMP ignoredSkills")
}

fn yaml_indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn yaml_key_line(line: &str, key: &str) -> bool {
    let trimmed = line.trim_start_matches(' ');
    trimmed
        .strip_prefix(key)
        .is_some_and(|rest| rest.starts_with(':'))
}

fn yaml_list_item_value(line: &str, label: &str) -> Option<Result<String, String>> {
    let trimmed = line.trim_start_matches(' ');
    let value = trimmed.strip_prefix("- ")?;
    Some(
        serde_yaml::from_str::<String>(value)
            .map_err(|error| format!("{label} 목록 항목을 읽을 수 없습니다: {error}")),
    )
}

/// YAML 설정 전체를 다시 직렬화하지 않고 중첩 문자열 목록의 정확한 이름 한
/// 항목만 추가하거나 제거한다. 다른 키, 순서, 빈 줄과 주석은 그대로 보존한다.
fn mutate_yaml_string_list_item(
    text: Option<&str>,
    section_key: &str,
    list_key: &str,
    skill_name: &str,
    present: bool,
    label: &str,
) -> Result<String, String> {
    let original = text.unwrap_or("");
    if original.contains('\r') {
        return Err(format!(
            "{label} 설정의 줄바꿈 형식을 안전하게 보존할 수 없습니다"
        ));
    }
    let current = yaml_string_list(text, section_key, list_key, label)?;
    let has_exact = current.iter().any(|value| value == skill_name);
    if has_exact == present {
        return Ok(original.to_string());
    }

    let had_final_newline = original.ends_with('\n');
    let mut lines: Vec<String> = original.lines().map(str::to_string).collect();
    let section_index = lines
        .iter()
        .position(|line| yaml_indent(line) == 0 && yaml_key_line(line, section_key));
    let quoted = serde_json::to_string(skill_name)
        .map_err(|error| format!("{label} 스킬 이름을 저장할 수 없습니다: {error}"))?;

    let Some(section_index) = section_index else {
        if !present {
            return Ok(original.to_string());
        }
        if !lines.is_empty() && !lines.last().is_some_and(|line| line.is_empty()) {
            lines.push(String::new());
        }
        lines.extend([
            format!("{section_key}:"),
            format!("  {list_key}:"),
            format!("    - {quoted}"),
        ]);
        let mut result = lines.join("\n");
        result.push('\n');
        return Ok(result);
    };

    let section_indent = yaml_indent(&lines[section_index]);
    let section_value = lines[section_index]
        .trim_start_matches(' ')
        .strip_prefix(&format!("{section_key}:"))
        .expect("matched key")
        .trim();
    if !section_value.is_empty() && !section_value.starts_with('#') {
        return Err(format!(
            "{label}의 {section_key}가 한 줄 객체라 주석과 형식을 안전하게 보존할 수 없습니다"
        ));
    }
    let section_end = ((section_index + 1)..lines.len())
        .find(|index| {
            let line = &lines[*index];
            !line.trim().is_empty()
                && !line.trim_start().starts_with('#')
                && yaml_indent(line) <= section_indent
        })
        .unwrap_or(lines.len());
    let list_index =
        ((section_index + 1)..section_end).find(|index| yaml_key_line(&lines[*index], list_key));

    let Some(list_index) = list_index else {
        if !present {
            return Ok(original.to_string());
        }
        lines.splice(
            section_end..section_end,
            [
                format!("{}{list_key}:", " ".repeat(section_indent + 2)),
                format!("{}- {quoted}", " ".repeat(section_indent + 4)),
            ],
        );
        let mut result = lines.join("\n");
        if had_final_newline {
            result.push('\n');
        }
        return Ok(result);
    };

    let key_indent = yaml_indent(&lines[list_index]);
    let key_line = lines[list_index].clone();
    let value_text = key_line
        .trim_start_matches(' ')
        .strip_prefix(&format!("{list_key}:"))
        .expect("matched key")
        .trim();
    if !value_text.is_empty() && !value_text.starts_with('#') {
        if value_text.contains('#') {
            return Err(format!(
                "{label} {list_key}의 같은 줄 주석을 안전하게 보존할 수 없습니다"
            ));
        }
        let mut values: Vec<String> = serde_yaml::from_str(value_text)
            .map_err(|error| format!("{label} {list_key} 목록을 읽을 수 없습니다: {error}"))?;
        if present {
            values.push(skill_name.to_string());
        } else {
            values.retain(|value| value != skill_name);
        }
        let serialized = serde_json::to_string(&values)
            .map_err(|error| format!("{label} {list_key} 목록을 저장할 수 없습니다: {error}"))?;
        lines[list_index] = format!("{}{list_key}: {serialized}", " ".repeat(key_indent));
    } else {
        let list_end = ((list_index + 1)..section_end)
            .find(|index| {
                let line = &lines[*index];
                !line.trim().is_empty()
                    && !line.trim_start().starts_with('#')
                    && yaml_indent(line) <= key_indent
            })
            .unwrap_or(section_end);
        if present {
            lines.insert(
                list_end,
                format!("{}- {quoted}", " ".repeat(key_indent + 2)),
            );
        } else {
            let mut remove_index = None;
            for (index, line) in lines.iter().enumerate().take(list_end).skip(list_index + 1) {
                if yaml_indent(line) <= key_indent {
                    continue;
                }
                if let Some(value) = yaml_list_item_value(line, label) {
                    if value? == skill_name {
                        remove_index = Some(index);
                        break;
                    }
                }
            }
            if let Some(index) = remove_index {
                lines.remove(index);
            }
        }
    }

    let mut result = lines.join("\n");
    if had_final_newline {
        result.push('\n');
    }
    yaml_string_list(Some(&result), section_key, list_key, label)?;
    Ok(result)
}

fn mutate_omp_ignored_skill(
    text: Option<&str>,
    skill_name: &str,
    ignored: bool,
) -> Result<String, String> {
    mutate_yaml_string_list_item(text, "skills", "ignoredSkills", skill_name, ignored, "OMP")
}

fn mutate_hermes_disabled_skill(
    text: Option<&str>,
    skill_name: &str,
    disabled: bool,
) -> Result<String, String> {
    mutate_yaml_string_list_item(text, "skills", "disabled", skill_name, disabled, "Hermes")
}

fn toml_string_array(
    document: &DocumentMut,
    key: &str,
    label: &str,
) -> Result<Vec<String>, String> {
    let Some(item) = document.get(key) else {
        return Ok(Vec::new());
    };
    let Some(values) = item.as_array() else {
        return Err(format!("{label} 설정이 문자열 목록이 아닙니다"));
    };
    values
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("{label} 설정에 문자열이 아닌 값이 있습니다"))
        })
        .collect()
}

fn set_toml_string_array_item(
    document: &mut DocumentMut,
    key: &str,
    skill_name: &str,
    present: bool,
    label: &str,
) -> Result<(), String> {
    if document.get(key).is_none() {
        document[key] = Item::Value(toml_edit::Value::Array(Array::new()));
    }
    let values = document[key]
        .as_array_mut()
        .ok_or_else(|| format!("{label} 설정이 문자열 목록이 아닙니다"))?;
    if values.iter().any(|value| !value.is_str()) {
        return Err(format!("{label} 설정에 문자열이 아닌 값이 있습니다"));
    }
    let has_exact = values
        .iter()
        .any(|value| value.as_str() == Some(skill_name));
    if present && !has_exact {
        values.push(skill_name);
    } else if !present && has_exact {
        values.retain(|value| value.as_str() != Some(skill_name));
    }
    Ok(())
}

fn mistral_disabled_skills(text: Option<&str>) -> Result<Vec<String>, String> {
    let document = text
        .unwrap_or("")
        .parse::<DocumentMut>()
        .map_err(|error| format!("Mistral Vibe config.toml을 읽을 수 없습니다: {error}"))?;
    let enabled = toml_string_array(&document, "enabled_skills", "Mistral enabled_skills")?;
    if !enabled.is_empty() {
        return Err(
            "Mistral enabled_skills allowlist가 있어 disabled_skills만 바꾸면 실제 상태를 보장할 수 없습니다. allowlist를 직접 확인하세요."
                .to_string(),
        );
    }
    toml_string_array(&document, "disabled_skills", "Mistral disabled_skills")
}

fn mutate_mistral_disabled_skill(
    text: Option<&str>,
    skill_name: &str,
    disabled: bool,
) -> Result<String, String> {
    let mut document = text
        .unwrap_or("")
        .parse::<DocumentMut>()
        .map_err(|error| format!("Mistral Vibe config.toml을 읽을 수 없습니다: {error}"))?;
    let enabled = toml_string_array(&document, "enabled_skills", "Mistral enabled_skills")?;
    if !enabled.is_empty() {
        return Err(
            "Mistral enabled_skills allowlist가 있어 자동으로 변경하지 않습니다".to_string(),
        );
    }
    set_toml_string_array_item(
        &mut document,
        "disabled_skills",
        skill_name,
        disabled,
        "Mistral disabled_skills",
    )?;
    Ok(document.to_string())
}

fn pattern_matches(pattern: &str, skill_name: &str, label: &str) -> Result<bool, String> {
    if let Some(expression) = pattern.strip_prefix("re:") {
        return Regex::new(expression)
            .map(|regex| regex.is_match(skill_name))
            .map_err(|error| format!("{label} 정규식 '{pattern}'을 읽을 수 없습니다: {error}"));
    }
    Pattern::new(pattern)
        .map(|pattern| pattern.matches(skill_name))
        .map_err(|error| format!("{label} 패턴을 읽을 수 없습니다: {error}"))
}

fn pattern_disabled_state(
    patterns: &[String],
    skill_name: &str,
    label: &str,
) -> Result<&'static str, String> {
    let mut exact = false;
    for pattern in patterns {
        if pattern == skill_name {
            exact = true;
        } else if pattern_matches(pattern, skill_name, label)? {
            return Err(format!(
                "{label} 패턴 '{pattern}'이 이 스킬에도 적용됩니다. 다른 스킬에 영향을 줄 수 있어 자동으로 바꾸지 않습니다."
            ));
        }
    }
    Ok(if exact { STATE_INACTIVE } else { STATE_ACTIVE })
}

fn list_adapter_label(adapter: Adapter) -> Result<&'static str, String> {
    match adapter {
        Adapter::FactoryDisabledSkills => Ok("Factory Droid disabledSkills"),
        Adapter::CommandCodeDisabledSkills => Ok("Command Code disabledSkills"),
        Adapter::MistralDisabledSkills => Ok("Mistral disabled_skills"),
        Adapter::OmpIgnoredSkills => Ok("OMP ignoredSkills"),
        Adapter::HermesDisabledSkills => Ok("Hermes skills.disabled"),
        _ => Err("이 Adapter는 이름 목록 설정을 사용하지 않습니다".to_string()),
    }
}

fn list_adapter_config_path(agent: &Agent, adapter: Adapter) -> Result<PathBuf, String> {
    match adapter {
        Adapter::FactoryDisabledSkills | Adapter::CommandCodeDisabledSkills => {
            config_path_for_disabled_skills_json(agent, adapter)
        }
        Adapter::OmpIgnoredSkills => config_path_for_omp(agent),
        Adapter::MistralDisabledSkills => config_path_for_mistral(agent),
        Adapter::HermesDisabledSkills => config_path_for_hermes(agent),
        _ => Err("이 Adapter는 이름 목록 설정을 사용하지 않습니다".to_string()),
    }
}

fn list_adapter_values(adapter: Adapter, text: Option<&str>) -> Result<Vec<String>, String> {
    match adapter {
        Adapter::FactoryDisabledSkills | Adapter::CommandCodeDisabledSkills => {
            let label = list_adapter_label(adapter)?;
            let document = strict_json_document(text, label)?;
            json_string_list(&document, "disabledSkills", label)
        }
        Adapter::OmpIgnoredSkills => omp_ignored_skills(text),
        Adapter::MistralDisabledSkills => mistral_disabled_skills(text),
        Adapter::HermesDisabledSkills => hermes_disabled_skills(text),
        _ => Err("이 Adapter는 이름 목록 설정을 사용하지 않습니다".to_string()),
    }
}

fn mutate_list_adapter_item(
    adapter: Adapter,
    text: Option<&str>,
    skill_name: &str,
    present: bool,
) -> Result<String, String> {
    match adapter {
        Adapter::FactoryDisabledSkills | Adapter::CommandCodeDisabledSkills => {
            let label = list_adapter_label(adapter)?;
            let mut document = strict_json_document(text, label)?;
            set_json_string_list_item(&mut document, "disabledSkills", skill_name, present, label)?;
            serde_json::to_string_pretty(&document)
                .map_err(|error| format!("{label} 설정을 저장할 수 없습니다: {error}"))
        }
        Adapter::OmpIgnoredSkills => mutate_omp_ignored_skill(text, skill_name, present),
        Adapter::MistralDisabledSkills => mutate_mistral_disabled_skill(text, skill_name, present),
        Adapter::HermesDisabledSkills => mutate_hermes_disabled_skill(text, skill_name, present),
        _ => Err("이 Adapter는 이름 목록 설정을 사용하지 않습니다".to_string()),
    }
}

fn list_adapter_state(
    adapter: Adapter,
    values: &[String],
    skill_name: &str,
) -> Result<&'static str, String> {
    if adapter == Adapter::OmpIgnoredSkills {
        omp_ignore_state(values, skill_name)
    } else if adapter == Adapter::MistralDisabledSkills {
        pattern_disabled_state(values, skill_name, "Mistral disabled_skills")
    } else if values.iter().any(|value| value == skill_name) {
        Ok(STATE_INACTIVE)
    } else {
        Ok(STATE_ACTIVE)
    }
}

fn has_json_comments(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    let mut quote = None;
    let mut escaped = false;
    while let Some(ch) = chars.next() {
        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == active_quote {
                quote = None;
            }
            continue;
        }
        if matches!(ch, '\'' | '"') {
            quote = Some(ch);
            continue;
        }
        if ch == '/' && matches!(chars.peek().copied(), Some('/') | Some('*')) {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpenCodePermissionShape {
    V1,
    V2,
}

fn opencode_document(text: Option<&str>) -> Result<(JsonValue, OpenCodePermissionShape), String> {
    let Some(text) = text else {
        return Err(
            "OpenCode 설정이 없어 v1 permission.skill과 v2 permissions 형식 중 어느 것을 쓸지 확인할 수 없습니다"
                .to_string(),
        );
    };
    if has_json_comments(text) {
        return Err(
            "OpenCode JSONC 설정에 주석이 있어 자동 저장하면 주석이 사라집니다. 주석을 보존하기 위해 변경하지 않습니다."
                .to_string(),
        );
    }
    let document: JsonValue = json5::from_str(text)
        .map_err(|error| format!("OpenCode 설정 JSON/JSONC를 읽을 수 없습니다: {error}"))?;
    let has_v1 = document.get("permission").is_some();
    let has_v2 = document.get("permissions").is_some();
    match (has_v1, has_v2) {
        (true, false) => Ok((document, OpenCodePermissionShape::V1)),
        (false, true) => Ok((document, OpenCodePermissionShape::V2)),
        (true, true) => Err(
            "OpenCode v1 permission과 v2 permissions가 함께 있어 자동으로 변경하지 않습니다"
                .to_string(),
        ),
        (false, false) => Err(
            "OpenCode 설정에서 v1 permission.skill 또는 v2 permissions 형식을 확인할 수 없습니다"
                .to_string(),
        ),
    }
}

fn opencode_skill_key(skill: &db::SkillForAgent) -> Result<String, String> {
    Path::new(&skill.dir_path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "OpenCode 스킬의 경로 기반 ID를 확인할 수 없습니다".to_string())
}

fn opencode_action_state(action: Option<&str>) -> Result<&'static str, String> {
    match action {
        Some("deny") => Ok(STATE_INACTIVE),
        None | Some("allow") | Some("ask") => Ok(STATE_ACTIVE),
        Some(action) => Err(format!(
            "OpenCode skill permission 값 '{action}'을 알 수 없습니다"
        )),
    }
}

fn opencode_v1_skill_permissions(
    document: &JsonValue,
) -> Result<Option<&JsonMap<String, JsonValue>>, String> {
    let permission = document
        .get("permission")
        .and_then(JsonValue::as_object)
        .ok_or_else(|| "OpenCode v1 permission이 객체가 아닙니다".to_string())?;
    match permission.get("skill") {
        None => Ok(None),
        Some(skill) => skill
            .as_object()
            .map(Some)
            .ok_or_else(|| "OpenCode v1 permission.skill이 객체가 아닙니다".to_string()),
    }
}

fn opencode_v1_skill_permissions_mut(
    document: &mut JsonValue,
) -> Result<&mut JsonMap<String, JsonValue>, String> {
    let permission = document
        .get_mut("permission")
        .and_then(JsonValue::as_object_mut)
        .ok_or_else(|| "OpenCode v1 permission이 객체가 아닙니다".to_string())?;
    permission
        .entry("skill".to_string())
        .or_insert_with(|| JsonValue::Object(JsonMap::new()))
        .as_object_mut()
        .ok_or_else(|| "OpenCode v1 permission.skill이 객체가 아닙니다".to_string())
}

fn opencode_v1_effect(document: &JsonValue, skill_key: &str) -> Result<Option<String>, String> {
    let mut effect = None;
    let Some(permissions) = opencode_v1_skill_permissions(document)? else {
        return Ok(None);
    };
    for (pattern, value) in permissions {
        let action = value.as_str().ok_or_else(|| {
            format!("OpenCode v1 permission.skill['{pattern}']이 문자열이 아닙니다")
        })?;
        if pattern_matches(pattern, skill_key, "OpenCode skill permission")? {
            effect = Some(action.to_string());
        }
    }
    Ok(effect)
}

fn opencode_v2_rules(document: &JsonValue) -> Result<&Vec<JsonValue>, String> {
    document
        .get("permissions")
        .and_then(JsonValue::as_array)
        .ok_or_else(|| "OpenCode v2 permissions가 배열이 아닙니다".to_string())
}

fn opencode_v2_rules_mut(document: &mut JsonValue) -> Result<&mut Vec<JsonValue>, String> {
    document
        .get_mut("permissions")
        .and_then(JsonValue::as_array_mut)
        .ok_or_else(|| "OpenCode v2 permissions가 배열이 아닙니다".to_string())
}

fn opencode_v2_effect(document: &JsonValue, skill_key: &str) -> Result<Option<String>, String> {
    let mut effect = None;
    for rule in opencode_v2_rules(document)? {
        let Some(rule) = rule.as_object() else {
            return Err("OpenCode v2 permissions 항목이 객체가 아닙니다".to_string());
        };
        if rule.get("action").and_then(JsonValue::as_str) != Some("skill") {
            continue;
        }
        let resource = rule
            .get("resource")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| "OpenCode v2 skill permission에 resource가 없습니다".to_string())?;
        let action = rule
            .get("effect")
            .and_then(JsonValue::as_str)
            .ok_or_else(|| "OpenCode v2 skill permission에 effect가 없습니다".to_string())?;
        if pattern_matches(resource, skill_key, "OpenCode v2 skill permission")? {
            effect = Some(action.to_string());
        }
    }
    Ok(effect)
}

fn opencode_effect(
    document: &JsonValue,
    shape: OpenCodePermissionShape,
    skill_key: &str,
) -> Result<Option<String>, String> {
    match shape {
        OpenCodePermissionShape::V1 => opencode_v1_effect(document, skill_key),
        OpenCodePermissionShape::V2 => opencode_v2_effect(document, skill_key),
    }
}

fn opencode_set_v1_exact(
    document: &mut JsonValue,
    skill_key: &str,
    effect: Option<&str>,
) -> Result<(), String> {
    let permissions = opencode_v1_skill_permissions_mut(document)?;
    permissions.remove(skill_key);
    if let Some(effect) = effect {
        permissions.insert(skill_key.to_string(), JsonValue::String(effect.to_string()));
    }
    Ok(())
}

fn opencode_append_v2_rule(
    document: &mut JsonValue,
    skill_key: &str,
    effect: &str,
) -> Result<(), String> {
    let mut rule = JsonMap::new();
    rule.insert("action".to_string(), JsonValue::String("skill".to_string()));
    rule.insert(
        "resource".to_string(),
        JsonValue::String(skill_key.to_string()),
    );
    rule.insert("effect".to_string(), JsonValue::String(effect.to_string()));
    opencode_v2_rules_mut(document)?.push(JsonValue::Object(rule));
    Ok(())
}

fn opencode_remove_last_v2_rule(
    document: &mut JsonValue,
    skill_key: &str,
    effect: &str,
) -> Result<(), String> {
    let rules = opencode_v2_rules_mut(document)?;
    let index = rules.iter().rposition(|rule| {
        rule.as_object().is_some_and(|rule| {
            rule.get("action").and_then(JsonValue::as_str) == Some("skill")
                && rule.get("resource").and_then(JsonValue::as_str) == Some(skill_key)
                && rule.get("effect").and_then(JsonValue::as_str) == Some(effect)
        })
    });
    let Some(index) = index else {
        return Err("OpenCode에 앱이 추가한 skill permission을 찾을 수 없습니다".to_string());
    };
    rules.remove(index);
    Ok(())
}

fn openclaw_document(text: Option<&str>) -> Result<JsonValue, String> {
    let Some(text) = text else {
        return Ok(JsonValue::Object(JsonMap::new()));
    };
    // OpenClaw 자체 writer도 JSON5 주석을 제거한다. 앱에서는 그 손실을
    // 허용하지 않고 사용자가 주석을 정리하거나 직접 설정하도록 안내한다.
    if has_json_comments(text) {
        return Err(
            "OpenClaw 설정에 JSON5 주석이 있어 자동 저장하면 주석이 사라집니다. 주석을 보존하기 위해 변경하지 않습니다."
                .to_string(),
        );
    }
    json5::from_str(text)
        .map_err(|error| format!("OpenClaw 설정 JSON5를 읽을 수 없습니다: {error}"))
}

fn openclaw_skill_key(skill: &db::SkillForAgent) -> Result<String, String> {
    let text = fs::read_to_string(&skill.file_path).map_err(|error| {
        format!(
            "OpenClaw 스킬 설정 키를 확인할 수 없습니다 '{}': {error}",
            skill.file_path
        )
    })?;
    let mut lines = text.lines();
    if lines.next() != Some("---") {
        return Ok(skill.name.clone());
    }
    let mut frontmatter = String::new();
    for line in lines {
        if line == "---" {
            break;
        }
        frontmatter.push_str(line);
        frontmatter.push('\n');
    }
    let yaml: serde_yaml::Value = serde_yaml::from_str(&frontmatter)
        .map_err(|error| format!("OpenClaw 스킬 frontmatter를 읽을 수 없습니다: {error}"))?;
    let Some(metadata) = yaml.get("metadata") else {
        return Ok(skill.name.clone());
    };
    let metadata = if let Some(text) = metadata.as_str() {
        json5::from_str::<JsonValue>(text)
            .map_err(|error| format!("OpenClaw 스킬 metadata를 읽을 수 없습니다: {error}"))?
    } else {
        serde_json::to_value(metadata)
            .map_err(|error| format!("OpenClaw 스킬 metadata를 읽을 수 없습니다: {error}"))?
    };
    Ok(metadata
        .pointer("/openclaw/skillKey")
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&skill.name)
        .to_string())
}

fn name_scope_key(adapter: Adapter, skill: &db::SkillForAgent) -> Result<String, String> {
    match adapter {
        Adapter::OpenClawEntries => openclaw_skill_key(skill),
        Adapter::OpenCodeSkillPermissions => opencode_skill_key(skill),
        _ if is_name_scoped_adapter(adapter) => Ok(skill.name.clone()),
        _ => Err("이 Adapter는 이름 단위 제어가 아닙니다".to_string()),
    }
}

fn openclaw_enabled(document: &JsonValue, skill_key: &str) -> Result<Option<bool>, String> {
    let Some(skills) = document.get("skills") else {
        return Ok(None);
    };
    let Some(skills) = skills.as_object() else {
        return Err("OpenClaw 설정의 skills가 객체가 아닙니다".to_string());
    };
    let Some(entries) = skills.get("entries") else {
        return Ok(None);
    };
    let Some(entries) = entries.as_object() else {
        return Err("OpenClaw 설정의 skills.entries가 객체가 아닙니다".to_string());
    };
    let Some(entry) = entries.get(skill_key) else {
        return Ok(None);
    };
    let Some(entry) = entry.as_object() else {
        return Err(format!(
            "OpenClaw 설정의 skills.entries['{skill_key}']가 객체가 아닙니다"
        ));
    };
    match entry.get("enabled") {
        None => Ok(None),
        Some(value) => value.as_bool().map(Some).ok_or_else(|| {
            format!("OpenClaw 설정의 skills.entries['{skill_key}'].enabled가 boolean이 아닙니다")
        }),
    }
}

fn set_openclaw_enabled(
    document: &mut JsonValue,
    skill_key: &str,
    enabled: Option<bool>,
) -> Result<(), String> {
    let root = document
        .as_object_mut()
        .ok_or_else(|| "OpenClaw 설정의 최상위 값이 객체가 아닙니다".to_string())?;
    if enabled.is_none() {
        let Some(skills) = root.get_mut("skills") else {
            return Ok(());
        };
        let Some(skills) = skills.as_object_mut() else {
            return Err("OpenClaw 설정의 skills가 객체가 아닙니다".to_string());
        };
        let Some(entries) = skills.get_mut("entries") else {
            return Ok(());
        };
        let Some(entries) = entries.as_object_mut() else {
            return Err("OpenClaw 설정의 skills.entries가 객체가 아닙니다".to_string());
        };
        if let Some(entry) = entries.get_mut(skill_key) {
            let Some(entry) = entry.as_object_mut() else {
                return Err(format!(
                    "OpenClaw 설정의 skills.entries['{skill_key}']가 객체가 아닙니다"
                ));
            };
            entry.remove("enabled");
            if entry.is_empty() {
                entries.remove(skill_key);
            }
        }
        if entries.is_empty() {
            skills.remove("entries");
        }
        if skills.is_empty() {
            root.remove("skills");
        }
        return Ok(());
    }
    let skills = root
        .entry("skills".to_string())
        .or_insert_with(|| JsonValue::Object(JsonMap::new()))
        .as_object_mut()
        .ok_or_else(|| "OpenClaw 설정의 skills가 객체가 아닙니다".to_string())?;
    let entries = skills
        .entry("entries".to_string())
        .or_insert_with(|| JsonValue::Object(JsonMap::new()))
        .as_object_mut()
        .ok_or_else(|| "OpenClaw 설정의 skills.entries가 객체가 아닙니다".to_string())?;
    let entry = entries
        .entry(skill_key.to_string())
        .or_insert_with(|| JsonValue::Object(JsonMap::new()))
        .as_object_mut()
        .ok_or_else(|| {
            format!("OpenClaw 설정의 skills.entries['{skill_key}']가 객체가 아닙니다")
        })?;
    entry.insert(
        "enabled".to_string(),
        JsonValue::Bool(enabled.expect("none handled above")),
    );
    Ok(())
}

fn bool_setting_text(value: Option<bool>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "default".to_string())
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
            Adapter::ClaudeSkillOverrides
                | Adapter::CodeBuddySkillOverrides
                | Adapter::CodexSkillsConfig
                | Adapter::FactoryDisabledSkills
                | Adapter::CommandCodeDisabledSkills
                | Adapter::MistralDisabledSkills
                | Adapter::OpenCodeSkillPermissions
                | Adapter::OmpIgnoredSkills
                | Adapter::HermesDisabledSkills
                | Adapter::OpenClawEntries
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
        Adapter::ClaudeSkillOverrides | Adapter::CodeBuddySkillOverrides => {
            let config_path = name_override_config_path(agent, adapter);
            let actual_result = match config_path.as_ref() {
                Ok(path) => match read_file_if_exists(path)
                    .and_then(|text| json_document(text.as_deref()))
                    .and_then(|document| json_override(&document, &skill.name))
                    .and_then(|value| {
                        let raw = value.clone().unwrap_or_else(|| "on".to_string());
                        name_override_state(adapter, value.as_deref()).map(|state| (state, raw))
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
        Adapter::FactoryDisabledSkills
        | Adapter::CommandCodeDisabledSkills
        | Adapter::MistralDisabledSkills
        | Adapter::OmpIgnoredSkills
        | Adapter::HermesDisabledSkills => {
            let config_path = list_adapter_config_path(agent, adapter);
            let actual_result = match config_path.as_ref() {
                Ok(path) => read_file_if_exists(path)
                    .and_then(|text| list_adapter_values(adapter, text.as_deref()))
                    .and_then(|values| {
                        let present = values.iter().any(|value| value == &skill.name);
                        list_adapter_state(adapter, &values, &skill.name)
                            .map(|state| (state, present.to_string()))
                    }),
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
        Adapter::OpenCodeSkillPermissions => {
            let config_path = config_path_for_opencode(agent);
            let actual_result = match config_path.as_ref() {
                Ok(path) => read_file_if_exists(path)
                    .and_then(|text| opencode_document(text.as_deref()))
                    .and_then(|(document, shape)| {
                        let skill_key = opencode_skill_key(skill)?;
                        let effect = opencode_effect(&document, shape, &skill_key)?;
                        let state = opencode_action_state(effect.as_deref())?;
                        Ok((state, state.to_string()))
                    }),
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
        Adapter::OpenClawEntries => {
            let config_path = config_path_for_openclaw(agent);
            let actual_result = match config_path.as_ref() {
                Ok(path) => read_file_if_exists(path)
                    .and_then(|text| openclaw_document(text.as_deref()))
                    .and_then(|document| {
                        let skill_key = openclaw_skill_key(skill)?;
                        let enabled = openclaw_enabled(&document, &skill_key)?;
                        Ok((
                            if enabled == Some(false) {
                                STATE_INACTIVE
                            } else {
                                STATE_ACTIVE
                            },
                            bool_setting_text(enabled),
                        ))
                    }),
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
    let mut name_scope_counts: HashMap<(Adapter, String), usize> = HashMap::new();
    for skill in &skills {
        if is_managed_installation(&installations, &paused, skill).is_some() {
            continue;
        }
        let adapter = adapter_for(&agent, skill, false);
        if !is_name_scoped_adapter(adapter) {
            continue;
        }
        if let Ok(scope_key) = name_scope_key(adapter, skill) {
            *name_scope_counts.entry((adapter, scope_key)).or_default() += 1;
        }
    }
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

        let adapter = adapter_for(&agent, &skill, false);
        let scope_key = name_scope_key(adapter, &skill).ok();
        let matching_count = scope_key
            .as_ref()
            .and_then(|key| name_scope_counts.get(&(adapter, key.clone())).copied())
            .unwrap_or(1);
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
                is_name_scoped_adapter(adapter)
                    .then(|| {
                        scope_key.as_ref().and_then(|key| {
                            stored
                                .iter()
                                .find(|control| control.skill_name == key.as_str())
                        })
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
    let scope_key = name_scope_key(adapter, skill).ok();
    let existing = match previous_controls
        .iter()
        .find(|control| source_path_matches(&control.source_path, &source_path))
        .cloned()
    {
        Some(control) => Some(control),
        None if is_name_scoped_adapter(adapter) => {
            let sibling_path = scope_skills.iter().find_map(|candidate| {
                (candidate.dir_path != source_path
                    && scope_key.as_ref().is_some_and(|key| {
                        name_scope_key(adapter, candidate)
                            .ok()
                            .as_ref()
                            .is_some_and(|candidate_key| candidate_key == key)
                    }))
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
        Adapter::ClaudeSkillOverrides | Adapter::CodeBuddySkillOverrides => {
            execute_name_override_action(
                pool,
                agent,
                skill,
                scope_skills,
                &previous_controls,
                source_path,
                existing,
                action,
                adapter,
            )
            .await
        }
        Adapter::CodexSkillsConfig => {
            execute_codex_action(pool, agent, skill, source_path, existing, action).await
        }
        Adapter::FactoryDisabledSkills
        | Adapter::CommandCodeDisabledSkills
        | Adapter::MistralDisabledSkills
        | Adapter::OmpIgnoredSkills
        | Adapter::HermesDisabledSkills => {
            execute_list_adapter_action(
                pool,
                agent,
                skill,
                scope_skills,
                &previous_controls,
                source_path,
                existing,
                action,
                adapter,
            )
            .await
        }
        Adapter::OpenCodeSkillPermissions => {
            execute_opencode_action(
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
        Adapter::OpenClawEntries => {
            execute_openclaw_action(
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
        Adapter::ManagedInstallation | Adapter::Unsupported => {
            Err("외부 스킬 Adapter 대상이 아닙니다".to_string())
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_name_override_action(
    pool: &DbPool,
    agent: &Agent,
    skill: &db::SkillForAgent,
    scope_skills: &[db::SkillForAgent],
    previous_controls: &[StoredControl],
    source_path: String,
    existing: Option<StoredControl>,
    action: ExternalAction,
    adapter: Adapter,
) -> Result<(), String> {
    let config_path = name_override_config_path(agent, adapter)?;
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
        .map_err(|error| format!("{} 설정을 저장할 수 없습니다: {error}", agent.display_name))?;
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
            return Err(format!(
                "{} 설정을 다시 읽었을 때 요청한 상태가 아닙니다",
                agent.display_name
            ));
        }
        Ok(())
    })();
    if let Err(error) = verification {
        let rollback = restore_file(&config_path, before.as_deref());
        return Err(match rollback {
            Ok(()) => format!(
                "{} 설정 검증에 실패해 변경을 되돌렸습니다: {error}",
                agent.display_name
            ),
            Err(rollback_error) => format!(
                "{} 설정 검증에 실패했고 되돌리기도 실패했습니다: {error}; {rollback_error}",
                agent.display_name
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

#[cfg(test)]
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
    execute_name_override_action(
        pool,
        agent,
        skill,
        scope_skills,
        previous_controls,
        source_path,
        existing,
        action,
        Adapter::ClaudeSkillOverrides,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn execute_list_adapter_action(
    pool: &DbPool,
    agent: &Agent,
    skill: &db::SkillForAgent,
    scope_skills: &[db::SkillForAgent],
    previous_controls: &[StoredControl],
    source_path: String,
    existing: Option<StoredControl>,
    action: ExternalAction,
    adapter: Adapter,
) -> Result<(), String> {
    let label = list_adapter_label(adapter)?;
    let config_path = list_adapter_config_path(agent, adapter)?;
    let before = read_file_if_exists(&config_path)?;
    let current = list_adapter_values(adapter, before.as_deref())?;
    list_adapter_state(adapter, &current, &skill.name)?;
    let current_present = current.iter().any(|value| value == &skill.name);
    if let Some(control) = existing.as_ref() {
        let expected_present = control
            .applied_value
            .parse::<bool>()
            .map_err(|_| format!("{label} 제어 기록이 손상되었습니다"))?;
        if current_present != expected_present {
            return Err(
                "설정이 다른 프로그램에 의해 바뀌어 충돌했습니다. 덮어쓰지 않습니다".to_string(),
            );
        }
    }

    if action == ExternalAction::Enable
        && existing
            .as_ref()
            .is_some_and(|control| control.state == STATE_DELETED)
    {
        return Err("적용 삭제된 스킬은 재적용 버튼으로 다시 활성화하세요".to_string());
    }
    let restore = matches!(action, ExternalAction::Enable | ExternalAction::Reapply);
    let original_present = match existing.as_ref() {
        Some(control) => control
            .original_value
            .as_deref()
            .unwrap_or("false")
            .parse::<bool>()
            .map_err(|_| format!("{label}의 원래 값 기록이 손상되었습니다"))?,
        None if restore => false,
        None => current_present,
    };
    let target_present = if restore { original_present } else { true };
    let state = match action {
        ExternalAction::Delete => STATE_DELETED,
        ExternalAction::Disable => STATE_INACTIVE,
        ExternalAction::Enable | ExternalAction::Reapply => STATE_ACTIVE,
    };
    let text = mutate_list_adapter_item(adapter, before.as_deref(), &skill.name, target_present)?;
    let expected = list_adapter_values(adapter, Some(&text))?;
    list_adapter_state(adapter, &expected, &skill.name).and_then(|actual| {
        let expected_state = if target_present {
            STATE_INACTIVE
        } else {
            STATE_ACTIVE
        };
        if actual == expected_state {
            Ok(())
        } else {
            Err(format!("{label}를 바꿔도 요청한 상태가 되지 않습니다"))
        }
    })?;
    let applied = target_present.to_string();
    write_atomic(&config_path, &text)?;

    let verification = read_file_if_exists(&config_path)
        .and_then(|text| list_adapter_values(adapter, text.as_deref()))
        .and_then(|values| {
            let present = values.iter().any(|value| value == &skill.name);
            if present != target_present {
                return Err(format!("{label}를 다시 읽었을 때 값이 달라졌습니다"));
            }
            list_adapter_state(adapter, &values, &skill.name).map(|_| ())
        });
    if let Err(error) = verification {
        let rollback = restore_file(&config_path, before.as_deref());
        return Err(match rollback {
            Ok(()) => format!("{label} 설정 검증에 실패해 변경을 되돌렸습니다: {error}"),
            Err(rollback_error) => format!(
                "{label} 설정 검증에 실패했고 되돌리기도 실패했습니다: {error}; {rollback_error}"
            ),
        });
    }

    let db_result = if restore {
        db::delete_platform_skill_controls_by_name(pool, &agent.id, &skill.name).await
    } else {
        let original = existing
            .as_ref()
            .and_then(|control| control.original_value.clone())
            .unwrap_or_else(|| current_present.to_string());
        let paths = scope_skills
            .iter()
            .filter(|candidate| {
                adapter_for(agent, candidate, false) == adapter && candidate.name == skill.name
            })
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
                    original_value: Some(original.clone()),
                    applied_value: applied.clone(),
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
            (Ok(()), Ok(())) => {
                format!("{label} 설정과 제어 기록을 되돌렸지만 저장에 실패했습니다: {error}")
            }
            (Err(file_error), Ok(())) => format!(
                "제어 기록은 되돌렸지만 {label} 설정 복원에 실패했습니다: {error}; {file_error}"
            ),
            (Ok(()), Err(records_error)) => format!(
                "{label} 설정은 되돌렸지만 제어 기록 복원에 실패했습니다: {error}; {records_error}"
            ),
            (Err(file_error), Err(records_error)) => format!(
                "{label} 설정과 제어 기록 복원이 모두 실패했습니다: {error}; {file_error}; {records_error}"
            ),
        });
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn execute_opencode_action(
    pool: &DbPool,
    agent: &Agent,
    skill: &db::SkillForAgent,
    scope_skills: &[db::SkillForAgent],
    previous_controls: &[StoredControl],
    source_path: String,
    existing: Option<StoredControl>,
    action: ExternalAction,
) -> Result<(), String> {
    let config_path = config_path_for_opencode(agent)?;
    let before = read_file_if_exists(&config_path)?;
    let (mut document, shape) = opencode_document(before.as_deref())?;
    let skill_key = opencode_skill_key(skill)?;
    let current_effect = opencode_effect(&document, shape, &skill_key)?;
    let current_state = opencode_action_state(current_effect.as_deref())?;
    if let Some(control) = existing.as_ref() {
        if current_state != control.applied_value {
            return Err(
                "설정이 다른 프로그램에 의해 바뀌어 충돌했습니다. 덮어쓰지 않습니다".to_string(),
            );
        }
    }
    if action == ExternalAction::Enable
        && existing
            .as_ref()
            .is_some_and(|control| control.state == STATE_DELETED)
    {
        return Err("적용 삭제된 스킬은 재적용 버튼으로 다시 활성화하세요".to_string());
    }

    let restore = matches!(action, ExternalAction::Enable | ExternalAction::Reapply);
    let original = if let Some(control) = existing.as_ref() {
        control
            .original_value
            .clone()
            .ok_or_else(|| "OpenCode의 원래 permission 기록이 없습니다".to_string())?
    } else {
        match shape {
            OpenCodePermissionShape::V1 => {
                let exact = opencode_v1_skill_permissions(&document)?
                    .and_then(|permissions| permissions.get(&skill_key))
                    .map(|value| {
                        value.as_str().map(str::to_string).ok_or_else(|| {
                            format!(
                                "OpenCode v1 permission.skill['{skill_key}']이 문자열이 아닙니다"
                            )
                        })
                    })
                    .transpose()?;
                exact
                    .map(|effect| format!("v1:{effect}"))
                    .unwrap_or_else(|| "v1:none".to_string())
            }
            OpenCodePermissionShape::V2 => "v2:appended-deny".to_string(),
        }
    };

    if restore {
        match (shape, existing.as_ref()) {
            (OpenCodePermissionShape::V1, Some(_)) => {
                let effect = original
                    .strip_prefix("v1:")
                    .ok_or_else(|| "OpenCode v1 원래 값 기록이 손상되었습니다".to_string())?;
                opencode_set_v1_exact(
                    &mut document,
                    &skill_key,
                    (effect != "none").then_some(effect),
                )?;
            }
            (OpenCodePermissionShape::V2, Some(_)) => {
                if original != "v2:appended-deny" {
                    return Err("OpenCode v2 원래 값 기록이 손상되었습니다".to_string());
                }
                opencode_remove_last_v2_rule(&mut document, &skill_key, "deny")?;
            }
            (OpenCodePermissionShape::V1, None) => {
                opencode_set_v1_exact(&mut document, &skill_key, Some("allow"))?;
            }
            (OpenCodePermissionShape::V2, None) => {
                opencode_append_v2_rule(&mut document, &skill_key, "allow")?;
            }
        }
    } else {
        match shape {
            OpenCodePermissionShape::V1 => {
                opencode_set_v1_exact(&mut document, &skill_key, Some("deny"))?;
            }
            OpenCodePermissionShape::V2 => {
                opencode_append_v2_rule(&mut document, &skill_key, "deny")?;
            }
        }
    }

    let text = serde_json::to_string_pretty(&document)
        .map_err(|error| format!("OpenCode 설정을 저장할 수 없습니다: {error}"))?;
    write_atomic(&config_path, &text)?;
    let expected_state = if restore {
        STATE_ACTIVE
    } else {
        STATE_INACTIVE
    };
    let verification = read_file_if_exists(&config_path)
        .and_then(|text| opencode_document(text.as_deref()))
        .and_then(|(document, verify_shape)| {
            if verify_shape != shape {
                return Err("OpenCode 설정 형식이 저장 뒤 달라졌습니다".to_string());
            }
            let effect = opencode_effect(&document, shape, &skill_key)?;
            let state = opencode_action_state(effect.as_deref())?;
            if state == expected_state {
                Ok(())
            } else {
                Err("OpenCode 설정을 다시 읽었을 때 요청한 상태가 아닙니다".to_string())
            }
        });
    if let Err(error) = verification {
        let rollback = restore_file(&config_path, before.as_deref());
        return Err(match rollback {
            Ok(()) => format!("OpenCode 설정 검증에 실패해 변경을 되돌렸습니다: {error}"),
            Err(rollback_error) => format!(
                "OpenCode 설정 검증에 실패했고 되돌리기도 실패했습니다: {error}; {rollback_error}"
            ),
        });
    }

    let db_result = if restore {
        db::delete_platform_skill_controls_by_name(pool, &agent.id, &skill_key).await
    } else {
        let paths = scope_skills
            .iter()
            .filter(|candidate| {
                adapter_for(agent, candidate, false) == Adapter::OpenCodeSkillPermissions
                    && opencode_skill_key(candidate)
                        .ok()
                        .is_some_and(|key| key == skill_key)
            })
            .map(|candidate| candidate.dir_path.clone())
            .chain(std::iter::once(source_path.clone()))
            .collect::<std::collections::BTreeSet<_>>();
        let state = if action == ExternalAction::Delete {
            STATE_DELETED
        } else {
            STATE_INACTIVE
        };
        let updated_at = chrono::Utc::now().to_rfc3339();
        let mut result = Ok(());
        for path in paths {
            result = db::upsert_platform_skill_control(
                pool,
                &StoredControl {
                    agent_id: agent.id.clone(),
                    source_path: path,
                    skill_name: skill_key.clone(),
                    state: state.to_string(),
                    original_value: Some(original.clone()),
                    applied_value: STATE_INACTIVE.to_string(),
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
                db::delete_platform_skill_controls_by_name(pool, &agent.id, &skill_key).await;
            match rollback_delete {
                Ok(()) => {
                    let mut rollback_result = Ok(());
                    for control in previous_controls
                        .iter()
                        .filter(|control| control.skill_name == skill_key)
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
            (Ok(()), Ok(())) => format!(
                "OpenCode 설정과 제어 기록을 되돌렸지만 저장에 실패했습니다: {error}"
            ),
            (Err(file_error), Ok(())) => format!(
                "제어 기록은 되돌렸지만 OpenCode 설정 복원에 실패했습니다: {error}; {file_error}"
            ),
            (Ok(()), Err(records_error)) => format!(
                "OpenCode 설정은 되돌렸지만 제어 기록 복원에 실패했습니다: {error}; {records_error}"
            ),
            (Err(file_error), Err(records_error)) => format!(
                "OpenCode 설정과 제어 기록 복원이 모두 실패했습니다: {error}; {file_error}; {records_error}"
            ),
        });
    }
    Ok(())
}

fn stored_openclaw_original(control: &StoredControl) -> Result<Option<bool>, String> {
    match control.original_value.as_deref() {
        None | Some("default") => Ok(None),
        Some("true") => Ok(Some(true)),
        Some("false") => Ok(Some(false)),
        Some(_) => Err("OpenClaw의 원래 enabled 값 기록이 손상되었습니다".to_string()),
    }
}

#[allow(clippy::too_many_arguments)]
async fn execute_openclaw_action(
    pool: &DbPool,
    agent: &Agent,
    skill: &db::SkillForAgent,
    scope_skills: &[db::SkillForAgent],
    previous_controls: &[StoredControl],
    source_path: String,
    existing: Option<StoredControl>,
    action: ExternalAction,
) -> Result<(), String> {
    let config_path = config_path_for_openclaw(agent)?;
    let before = read_file_if_exists(&config_path)?;
    let mut document = openclaw_document(before.as_deref())?;
    let skill_key = openclaw_skill_key(skill)?;
    let current = openclaw_enabled(&document, &skill_key)?;
    let current_text = bool_setting_text(current);
    if let Some(control) = existing.as_ref() {
        if current_text != control.applied_value {
            return Err(
                "설정이 다른 프로그램에 의해 바뀌어 충돌했습니다. 덮어쓰지 않습니다".to_string(),
            );
        }
    }
    if action == ExternalAction::Enable
        && existing
            .as_ref()
            .is_some_and(|control| control.state == STATE_DELETED)
    {
        return Err("적용 삭제된 스킬은 재적용 버튼으로 다시 활성화하세요".to_string());
    }

    let restore = matches!(action, ExternalAction::Enable | ExternalAction::Reapply);
    let target = if restore {
        existing
            .as_ref()
            .map(stored_openclaw_original)
            .transpose()?
            .flatten()
    } else {
        Some(false)
    };
    set_openclaw_enabled(&mut document, &skill_key, target)?;
    let text = serde_json::to_string_pretty(&document)
        .map_err(|error| format!("OpenClaw 설정을 저장할 수 없습니다: {error}"))?;
    write_atomic(&config_path, &text)?;

    let expected_text = bool_setting_text(target);
    let verification = read_file_if_exists(&config_path)
        .and_then(|text| openclaw_document(text.as_deref()))
        .and_then(|document| openclaw_enabled(&document, &skill_key))
        .and_then(|enabled| {
            if enabled == target {
                Ok(())
            } else {
                Err("OpenClaw 설정을 다시 읽었을 때 요청한 상태가 아닙니다".to_string())
            }
        });
    if let Err(error) = verification {
        let rollback = restore_file(&config_path, before.as_deref());
        return Err(match rollback {
            Ok(()) => format!("OpenClaw 설정 검증에 실패해 변경을 되돌렸습니다: {error}"),
            Err(rollback_error) => format!(
                "OpenClaw 설정 검증에 실패했고 되돌리기도 실패했습니다: {error}; {rollback_error}"
            ),
        });
    }

    let db_result = if restore {
        db::delete_platform_skill_controls_by_name(pool, &agent.id, &skill_key).await
    } else {
        let original = existing
            .as_ref()
            .and_then(|control| control.original_value.clone())
            .unwrap_or(current_text);
        let paths = scope_skills
            .iter()
            .filter(|candidate| {
                openclaw_skill_key(candidate)
                    .ok()
                    .is_some_and(|candidate_key| candidate_key == skill_key)
            })
            .map(|candidate| candidate.dir_path.clone())
            .chain(std::iter::once(source_path.clone()))
            .collect::<std::collections::BTreeSet<_>>();
        let state = if action == ExternalAction::Delete {
            STATE_DELETED
        } else {
            STATE_INACTIVE
        };
        let updated_at = chrono::Utc::now().to_rfc3339();
        let mut result = Ok(());
        for path in paths {
            result = db::upsert_platform_skill_control(
                pool,
                &StoredControl {
                    agent_id: agent.id.clone(),
                    source_path: path,
                    skill_name: skill_key.clone(),
                    state: state.to_string(),
                    original_value: Some(original.clone()),
                    applied_value: expected_text.clone(),
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
                db::delete_platform_skill_controls_by_name(pool, &agent.id, &skill_key).await;
            match rollback_delete {
                Ok(()) => {
                    let mut rollback_result = Ok(());
                    for control in previous_controls
                        .iter()
                        .filter(|control| control.skill_name == skill_key)
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
            (Ok(()), Ok(())) => format!(
                "OpenClaw 설정과 제어 기록을 되돌렸지만 저장에 실패했습니다: {error}"
            ),
            (Err(file_error), Ok(())) => format!(
                "제어 기록은 되돌렸지만 OpenClaw 설정 복원에 실패했습니다: {error}; {file_error}"
            ),
            (Ok(()), Err(records_error)) => format!(
                "OpenClaw 설정은 되돌렸지만 제어 기록 복원에 실패했습니다: {error}; {records_error}"
            ),
            (Err(file_error), Err(records_error)) => format!(
                "OpenClaw 설정과 제어 기록 복원이 모두 실패했습니다: {error}; {file_error}; {records_error}"
            ),
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
    use serde_json::json;
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
    fn omp_ignored_skills_edit_preserves_unrelated_yaml_and_comments() {
        let original = "# root comment\nskills:\n  # keep skill comment\n  ignoredSkills:\n    - \"kept-*\"\n  includeSkills: []\nother:\n  value: kept\n";
        let disabled = mutate_omp_ignored_skill(Some(original), "airbnb-full", true).unwrap();
        assert!(disabled.contains("# root comment"));
        assert!(disabled.contains("# keep skill comment"));
        assert!(disabled.contains("other:\n  value: kept"));
        assert_eq!(
            omp_ignored_skills(Some(&disabled)).unwrap(),
            vec!["kept-*", "airbnb-full"]
        );

        let restored = mutate_omp_ignored_skill(Some(&disabled), "airbnb-full", false).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn omp_ignored_skills_supports_inline_list_and_rejects_broad_match() {
        let disabled = mutate_omp_ignored_skill(
            Some("skills:\n  ignoredSkills: [\"kept\"]\n"),
            "airbnb-full",
            true,
        )
        .unwrap();
        assert_eq!(
            omp_ignored_skills(Some(&disabled)).unwrap(),
            vec!["kept", "airbnb-full"]
        );
        assert!(omp_ignore_state(&["airbnb-*".to_string()], "airbnb-full").is_err());
        assert!(omp_ignore_state(
            &["airbnb-full".to_string(), "airbnb-*".to_string()],
            "airbnb-full"
        )
        .is_err());
    }

    #[tokio::test]
    async fn codebuddy_name_override_round_trips_without_touching_other_settings() {
        let directory = TempDir::new().unwrap();
        let root = directory.path().join(".codebuddy");
        let skills_root = root.join("skills");
        let skill = test_skill(
            "shared-skill",
            "shared-skill",
            &skills_root.join("shared-skill"),
            "compatibility",
        );
        let agent = test_agent("codebuddy", "CodeBuddy", &skills_root);
        let settings_path = root.join("settings.json");
        fs::write(&settings_path, r#"{"theme":"dark"}"#).unwrap();
        let pool = test_pool(&directory).await;
        register_test_observation(&pool, &agent, &skill).await;

        execute_name_override_action(
            &pool,
            &agent,
            &skill,
            std::slice::from_ref(&skill),
            &[],
            skill.dir_path.clone(),
            None,
            ExternalAction::Disable,
            Adapter::CodeBuddySkillOverrides,
        )
        .await
        .unwrap();
        let disabled: JsonValue =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(disabled["theme"], "dark");
        assert_eq!(disabled["skillOverrides"]["shared-skill"], "off");

        let controls = db::get_platform_skill_controls(&pool, "codebuddy")
            .await
            .unwrap();
        let status = get_platform_skill_controls_impl(&pool, "codebuddy")
            .await
            .unwrap();
        assert_eq!(status[0].state, STATE_INACTIVE);
        assert_eq!(status[0].adapter, "codebuddy-skill-overrides");
        execute_name_override_action(
            &pool,
            &agent,
            &skill,
            std::slice::from_ref(&skill),
            &controls,
            skill.dir_path.clone(),
            Some(controls[0].clone()),
            ExternalAction::Enable,
            Adapter::CodeBuddySkillOverrides,
        )
        .await
        .unwrap();
        let restored: JsonValue =
            serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
        assert_eq!(restored["theme"], "dark");
        assert!(restored
            .get("skillOverrides")
            .and_then(|value| value.get("shared-skill"))
            .is_none());
    }

    #[tokio::test]
    async fn codebuddy_name_scope_does_not_count_plugin_sources() {
        let directory = TempDir::new().unwrap();
        let skills_root = directory.path().join(".codebuddy/skills");
        let regular = test_skill(
            "regular-shared-skill",
            "shared-skill",
            &skills_root.join("shared-skill"),
            "compatibility",
        );
        let plugin = test_skill(
            "plugin-shared-skill",
            "shared-skill",
            &directory.path().join(".codebuddy/plugins/shared-skill"),
            "plugin",
        );
        let agent = test_agent("codebuddy", "CodeBuddy", &skills_root);
        let pool = test_pool(&directory).await;
        register_test_observation(&pool, &agent, &regular).await;
        register_test_observation(&pool, &agent, &plugin).await;

        let statuses = get_platform_skill_controls_impl(&pool, "codebuddy")
            .await
            .unwrap();
        let regular_status = statuses
            .iter()
            .find(|status| status.source_kind.as_deref() == Some("compatibility"))
            .unwrap();
        assert_eq!(regular_status.affected_source_count, 1);
        assert_eq!(regular_status.scope, "name");
        assert!(regular_status.reason.is_none());

        let plugin_status = statuses
            .iter()
            .find(|status| status.source_kind.as_deref() == Some("plugin"))
            .unwrap();
        assert!(!plugin_status.supported);
        assert_eq!(plugin_status.adapter, "unsupported");
    }

    #[tokio::test]
    async fn omp_adapter_round_trips_and_preserves_comments() {
        let directory = TempDir::new().unwrap();
        let root = directory.path().join(".omp/agent");
        let skills_root = root.join("skills");
        let skill = test_skill(
            "airbnb-full",
            "airbnb-full",
            &skills_root.join("airbnb-full"),
            "compatibility",
        );
        let agent = test_agent("omp", "Oh My Pi", &skills_root);
        let config_path = root.join("config.yml");
        let original =
            "# keep\nskills:\n  ignoredSkills:\n    - \"other\"\nother:\n  value: kept\n";
        fs::write(&config_path, original).unwrap();
        let pool = test_pool(&directory).await;
        register_test_observation(&pool, &agent, &skill).await;

        execute_list_adapter_action(
            &pool,
            &agent,
            &skill,
            std::slice::from_ref(&skill),
            &[],
            skill.dir_path.clone(),
            None,
            ExternalAction::Disable,
            Adapter::OmpIgnoredSkills,
        )
        .await
        .unwrap();
        let disabled = fs::read_to_string(&config_path).unwrap();
        assert!(disabled.contains("# keep"));
        assert!(disabled.contains("value: kept"));
        assert_eq!(
            omp_ignored_skills(Some(&disabled)).unwrap(),
            vec!["other", "airbnb-full"]
        );

        let controls = db::get_platform_skill_controls(&pool, "omp").await.unwrap();
        let status = get_platform_skill_controls_impl(&pool, "omp")
            .await
            .unwrap();
        assert_eq!(status[0].state, STATE_INACTIVE);
        assert_eq!(status[0].adapter, "omp-ignored-skills");
        execute_list_adapter_action(
            &pool,
            &agent,
            &skill,
            std::slice::from_ref(&skill),
            &controls,
            skill.dir_path.clone(),
            Some(controls[0].clone()),
            ExternalAction::Enable,
            Adapter::OmpIgnoredSkills,
        )
        .await
        .unwrap();
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
    }

    #[tokio::test]
    async fn omp_adapter_allows_multiple_independent_disabled_skills() {
        let directory = TempDir::new().unwrap();
        let root = directory.path().join(".omp/agent");
        let skills_root = root.join("skills");
        let first = test_skill(
            "first-skill",
            "first-skill",
            &skills_root.join("first-skill"),
            "compatibility",
        );
        let second = test_skill(
            "second-skill",
            "second-skill",
            &skills_root.join("second-skill"),
            "compatibility",
        );
        let agent = test_agent("omp", "Oh My Pi", &skills_root);
        let config_path = root.join("config.yml");
        fs::create_dir_all(&root).unwrap();
        fs::write(&config_path, "skills:\n  ignoredSkills: []\n").unwrap();
        let pool = test_pool(&directory).await;
        register_test_observation(&pool, &agent, &first).await;
        register_test_observation(&pool, &agent, &second).await;
        let skills = vec![first.clone(), second.clone()];

        execute_external_action(&pool, &agent, &first, &skills, ExternalAction::Disable)
            .await
            .unwrap();
        execute_external_action(&pool, &agent, &second, &skills, ExternalAction::Disable)
            .await
            .unwrap();
        let statuses = get_platform_skill_controls_impl(&pool, "omp")
            .await
            .unwrap();
        assert!(statuses.iter().all(|status| status.state == STATE_INACTIVE));

        execute_external_action(&pool, &agent, &first, &skills, ExternalAction::Enable)
            .await
            .unwrap();
        let values = omp_ignored_skills(Some(&fs::read_to_string(&config_path).unwrap())).unwrap();
        assert_eq!(values, vec!["second-skill"]);
        execute_external_action(&pool, &agent, &second, &skills, ExternalAction::Enable)
            .await
            .unwrap();
        assert!(
            omp_ignored_skills(Some(&fs::read_to_string(&config_path).unwrap()))
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn json_disabled_skills_adapters_round_trip_unrelated_settings() {
        for (agent_id, display_name, directory_name, adapter_name) in [
            (
                "factory-droid",
                "Factory Droid",
                ".factory",
                "factory-disabled-skills",
            ),
            (
                "command-code",
                "Command Code",
                ".commandcode",
                "command-code-disabled-skills",
            ),
        ] {
            let directory = TempDir::new().unwrap();
            let root = directory.path().join(directory_name);
            let skills_root = root.join("skills");
            let skill = test_skill(
                "shared-skill",
                "shared-skill",
                &skills_root.join("shared-skill"),
                "native",
            );
            let agent = test_agent(agent_id, display_name, &skills_root);
            let settings_path = root.join("settings.json");
            fs::create_dir_all(&root).unwrap();
            fs::write(
                &settings_path,
                r#"{"theme":"dark","disabledSkills":["other"]}"#,
            )
            .unwrap();
            let pool = test_pool(&directory).await;
            register_test_observation(&pool, &agent, &skill).await;

            execute_external_action(
                &pool,
                &agent,
                &skill,
                std::slice::from_ref(&skill),
                ExternalAction::Disable,
            )
            .await
            .unwrap();
            let disabled: JsonValue =
                serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
            assert_eq!(disabled["theme"], "dark");
            assert_eq!(disabled["disabledSkills"], json!(["other", "shared-skill"]));
            let status = get_platform_skill_controls_impl(&pool, agent_id)
                .await
                .unwrap();
            assert_eq!(status[0].adapter, adapter_name);
            assert_eq!(status[0].state, STATE_INACTIVE);

            execute_external_action(
                &pool,
                &agent,
                &skill,
                std::slice::from_ref(&skill),
                ExternalAction::Enable,
            )
            .await
            .unwrap();
            let restored: JsonValue =
                serde_json::from_str(&fs::read_to_string(&settings_path).unwrap()).unwrap();
            assert_eq!(restored["theme"], "dark");
            assert_eq!(restored["disabledSkills"], json!(["other"]));
        }
    }

    #[tokio::test]
    async fn hermes_disabled_skills_round_trip_preserves_yaml_comments() {
        let directory = TempDir::new().unwrap();
        let root = directory.path().join(".hermes");
        let skills_root = root.join("skills");
        let skill = test_skill(
            "shared-skill",
            "shared-skill",
            &skills_root.join("shared-skill"),
            "native",
        );
        let agent = test_agent("hermes", "Hermes", &skills_root);
        let config_path = root.join("config.yaml");
        let original =
            "# keep\nskills:\n  # keep list comment\n  disabled:\n    - \"other\"\nmodel:\n  default: kept\n";
        fs::create_dir_all(&root).unwrap();
        fs::write(&config_path, original).unwrap();
        let pool = test_pool(&directory).await;
        register_test_observation(&pool, &agent, &skill).await;

        execute_external_action(
            &pool,
            &agent,
            &skill,
            std::slice::from_ref(&skill),
            ExternalAction::Disable,
        )
        .await
        .unwrap();
        let disabled = fs::read_to_string(&config_path).unwrap();
        assert!(disabled.contains("# keep list comment"));
        assert!(disabled.contains("default: kept"));
        assert_eq!(
            hermes_disabled_skills(Some(&disabled)).unwrap(),
            vec!["other", "shared-skill"]
        );

        execute_external_action(
            &pool,
            &agent,
            &skill,
            std::slice::from_ref(&skill),
            ExternalAction::Enable,
        )
        .await
        .unwrap();
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
    }

    #[tokio::test]
    async fn mistral_disabled_skills_round_trip_preserves_toml_comments() {
        let directory = TempDir::new().unwrap();
        let root = directory.path().join(".vibe");
        let skills_root = root.join("skills");
        let skill = test_skill(
            "shared-skill",
            "shared-skill",
            &skills_root.join("shared-skill"),
            "native",
        );
        let agent = test_agent("mistral-vibe", "Mistral Vibe", &skills_root);
        let config_path = root.join("config.toml");
        let original = "# keep\ndisabled_skills = [\"other\"]\nactive_model = \"kept\"\n";
        fs::create_dir_all(&root).unwrap();
        fs::write(&config_path, original).unwrap();
        let pool = test_pool(&directory).await;
        register_test_observation(&pool, &agent, &skill).await;

        execute_external_action(
            &pool,
            &agent,
            &skill,
            std::slice::from_ref(&skill),
            ExternalAction::Disable,
        )
        .await
        .unwrap();
        let disabled = fs::read_to_string(&config_path).unwrap();
        assert!(disabled.contains("# keep"));
        assert!(disabled.contains("active_model = \"kept\""));
        assert_eq!(
            mistral_disabled_skills(Some(&disabled)).unwrap(),
            vec!["other", "shared-skill"]
        );
        let status = get_platform_skill_controls_impl(&pool, "mistral-vibe")
            .await
            .unwrap();
        assert_eq!(status[0].adapter, "mistral-disabled-skills");
        assert_eq!(status[0].state, STATE_INACTIVE);

        execute_external_action(
            &pool,
            &agent,
            &skill,
            std::slice::from_ref(&skill),
            ExternalAction::Enable,
        )
        .await
        .unwrap();
        assert_eq!(fs::read_to_string(&config_path).unwrap(), original);
    }

    #[test]
    fn mistral_refuses_allowlist_and_broad_deny_patterns() {
        assert!(
            mistral_disabled_skills(Some("enabled_skills = [\"shared-skill\"]\n"))
                .unwrap_err()
                .contains("allowlist")
        );
        assert!(pattern_disabled_state(
            &["shared-*".to_string()],
            "shared-skill",
            "Mistral disabled_skills",
        )
        .is_err());
        assert!(pattern_disabled_state(
            &["re:^shared-".to_string()],
            "shared-skill",
            "Mistral disabled_skills",
        )
        .is_err());
    }

    #[tokio::test]
    async fn opencode_v1_and_v2_permissions_round_trip() {
        for (shape_name, original) in [
            (
                "v1",
                r#"{"theme":"kept","permission":{"skill":{"*":"allow"}}}"#,
            ),
            (
                "v2",
                r#"{"theme":"kept","permissions":[{"action":"skill","resource":"*","effect":"allow"}]}"#,
            ),
        ] {
            let directory = TempDir::new().unwrap();
            let skills_root = directory.path().join(".opencode/skills");
            let skill = test_skill(
                "shared-skill",
                "shared-skill",
                &skills_root.join("shared-skill"),
                "compatibility",
            );
            let agent = test_agent("opencode", "OpenCode", &skills_root);
            let config_path = directory.path().join(".config/opencode/opencode.json");
            fs::create_dir_all(config_path.parent().unwrap()).unwrap();
            fs::write(&config_path, original).unwrap();
            let pool = test_pool(&directory).await;
            register_test_observation(&pool, &agent, &skill).await;
            sqlx::query("UPDATE agents SET is_builtin = 0 WHERE id = 'opencode'")
                .execute(&pool)
                .await
                .unwrap();

            execute_external_action(
                &pool,
                &agent,
                &skill,
                std::slice::from_ref(&skill),
                ExternalAction::Disable,
            )
            .await
            .unwrap_or_else(|error| panic!("{shape_name} disable failed: {error}"));
            let disabled_text = fs::read_to_string(&config_path).unwrap();
            let (disabled, shape) = opencode_document(Some(&disabled_text)).unwrap();
            assert_eq!(disabled["theme"], "kept");
            assert_eq!(
                opencode_action_state(
                    opencode_effect(&disabled, shape, "shared-skill")
                        .unwrap()
                        .as_deref(),
                )
                .unwrap(),
                STATE_INACTIVE
            );
            let status = get_platform_skill_controls_impl(&pool, "opencode")
                .await
                .unwrap();
            assert_eq!(status[0].adapter, "opencode-skill-permissions");
            assert_eq!(status[0].state, STATE_INACTIVE);

            execute_external_action(
                &pool,
                &agent,
                &skill,
                std::slice::from_ref(&skill),
                ExternalAction::Enable,
            )
            .await
            .unwrap_or_else(|error| panic!("{shape_name} enable failed: {error}"));
            let restored_text = fs::read_to_string(&config_path).unwrap();
            let (restored, shape) = opencode_document(Some(&restored_text)).unwrap();
            assert_eq!(restored["theme"], "kept");
            assert_eq!(
                opencode_action_state(
                    opencode_effect(&restored, shape, "shared-skill")
                        .unwrap()
                        .as_deref(),
                )
                .unwrap(),
                STATE_ACTIVE
            );
        }
    }

    #[test]
    fn opencode_refuses_ambiguous_or_commented_config() {
        assert!(opencode_document(None).is_err());
        assert!(opencode_document(Some(
            "{ // keep\n permission: { skill: { '*': 'allow' } }\n}"
        ))
        .unwrap_err()
        .contains("주석"));
        assert!(
            opencode_document(Some(r#"{"permission":{"skill":{}},"permissions":[]}"#)).is_err()
        );

        let (mut document, shape) =
            opencode_document(Some(r#"{"permission":{"bash":"ask"},"theme":"kept"}"#)).unwrap();
        assert_eq!(shape, OpenCodePermissionShape::V1);
        assert_eq!(
            opencode_effect(&document, shape, "shared-skill").unwrap(),
            None
        );
        opencode_set_v1_exact(&mut document, "shared-skill", Some("deny")).unwrap();
        assert_eq!(document["permission"]["bash"], "ask");
        assert_eq!(document["permission"]["skill"]["shared-skill"], "deny");
    }

    #[test]
    fn openclaw_json5_comment_guard_ignores_comment_markers_inside_strings() {
        assert!(!has_json_comments(r#"{"url":"https://example.com/a/*/b"}"#));
        assert!(has_json_comments("{ // keep\n value: true\n}"));
        assert!(openclaw_document(Some("{ // keep\n value: true\n}")).is_err());
    }

    #[tokio::test]
    async fn openclaw_adapter_uses_metadata_key_and_round_trips_enabled_value() {
        let directory = TempDir::new().unwrap();
        let root = directory.path().join(".openclaw");
        let skills_root = root.join("skills");
        let skill = test_skill(
            "display-name",
            "display-name",
            &skills_root.join("display-name"),
            "compatibility",
        );
        fs::write(
            &skill.file_path,
            "---\nname: display-name\nmetadata:\n  openclaw:\n    skillKey: stable-key\n---\n",
        )
        .unwrap();
        let agent = test_agent("openclaw", "OpenClaw", &skills_root);
        let config_path = root.join("openclaw.json");
        fs::write(&config_path, r#"{"theme":"dark"}"#).unwrap();
        let pool = test_pool(&directory).await;
        register_test_observation(&pool, &agent, &skill).await;

        execute_openclaw_action(
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
        let disabled = openclaw_document(fs::read_to_string(&config_path).ok().as_deref()).unwrap();
        assert_eq!(disabled["theme"], "dark");
        assert_eq!(
            disabled["skills"]["entries"]["stable-key"]["enabled"],
            false
        );
        let controls = db::get_platform_skill_controls(&pool, "openclaw")
            .await
            .unwrap();
        assert_eq!(controls[0].skill_name, "stable-key");
        let status = get_platform_skill_controls_impl(&pool, "openclaw")
            .await
            .unwrap();
        assert_eq!(status[0].state, STATE_INACTIVE);
        assert_eq!(status[0].adapter, "openclaw-skill-entries");

        execute_openclaw_action(
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
        let restored = openclaw_document(fs::read_to_string(&config_path).ok().as_deref()).unwrap();
        assert_eq!(restored["theme"], "dark");
        assert_eq!(openclaw_enabled(&restored, "stable-key").unwrap(), None);
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

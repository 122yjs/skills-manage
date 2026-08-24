use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::db::{self, Agent, DbPool};
use crate::path_utils::{expand_home_path, path_to_string, resolve_home_dir};
use crate::AppState;

// ─── Types ────────────────────────────────────────────────────────────────────

/// An agent enriched with a live `is_detected` flag derived from the file
/// system at query time, rather than from the last scan run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentWithStatus {
    pub id: String,
    pub display_name: String,
    pub category: String,
    pub global_skills_dir: String,
    pub project_skills_dir: Option<String>,
    pub icon_name: Option<String>,
    /// `true` if the agent is considered "installed" on this machine.
    pub is_detected: bool,
    pub is_builtin: bool,
    pub is_enabled: bool,
}

/// Payload for registering a new user-defined agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomAgentConfig {
    /// Optional explicit ID. If omitted or empty, one is derived from
    /// `display_name`.
    pub id: Option<String>,
    /// Human-readable name shown in the UI.
    pub display_name: String,
    /// Agent category — "coding", "lobster", or "other".
    pub category: Option<String>,
    /// Absolute path to the agent's global skills directory.
    pub global_skills_dir: String,
}

/// Payload for updating an existing user-defined agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCustomAgentConfig {
    /// Human-readable name shown in the UI.
    pub display_name: String,
    /// Agent category — "coding", "lobster", or "other".
    pub category: Option<String>,
    /// Absolute path to the agent's global skills directory.
    pub global_skills_dir: String,
}

// ─── Detection Helpers ────────────────────────────────────────────────────────

/// `skills` 디렉터리를 제외한 플랫폼 고유 파일이 있는지 확인한다.
/// 스킬 관리 앱이 만든 디렉터리나 링크만으로는 도구 설치를 판단하지 않는다.
fn has_platform_owned_entry(global_skills_dir: &Path) -> bool {
    let Some(platform_root) = global_skills_dir.parent() else {
        return false;
    };
    let Ok(entries) = std::fs::read_dir(platform_root) else {
        return false;
    };

    entries
        .flatten()
        .any(|entry| entry.path() != global_skills_dir)
}

fn is_shared_agents_skills_dir(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == "skills")
        && path
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == ".agents")
}

fn directory_has_entry(path: &Path) -> bool {
    std::fs::read_dir(path)
        .ok()
        .and_then(|mut entries| entries.next())
        .is_some()
}

fn shared_agent_marker_dir(agent: &Agent, shared_skills_dir: &Path) -> Option<std::path::PathBuf> {
    let home = shared_skills_dir.parent()?.parent()?;
    let marker_name = match agent.id.as_str() {
        "codex" => ".codex",
        "kimi-code-cli" => ".kimi",
        id => return Some(home.join(format!(".{id}"))),
    };
    Some(home.join(marker_name))
}

fn executable_names(agent: &Agent) -> Vec<String> {
    let mut names = vec![agent.id.clone()];
    if let Some(icon_name) = &agent.icon_name {
        if !names.contains(icon_name) {
            names.push(icon_name.clone());
        }
    }
    match agent.id.as_str() {
        "codex" => names.push("codex".to_string()),
        "kimi-code-cli" => names.push("kimi".to_string()),
        _ => {}
    }
    names
}

/// macOS 앱은 셸보다 짧은 PATH로 실행될 수 있다.
/// Homebrew로 설치한 CLI도 실제 설치 여부 판단에 포함한다.
fn executable_search_paths() -> Vec<std::path::PathBuf> {
    let mut directories: Vec<std::path::PathBuf> = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect())
        .unwrap_or_default();

    #[cfg(target_os = "macos")]
    directories.extend([
        std::path::PathBuf::from("/opt/homebrew/bin"),
        std::path::PathBuf::from("/usr/local/bin"),
    ]);

    directories
}

fn executable_exists(agent: &Agent) -> bool {
    let extensions: Vec<String> = std::env::var_os("PATHEXT")
        .map(|value| {
            value
                .to_string_lossy()
                .split(';')
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_else(|| vec![String::new()]);

    executable_search_paths().iter().any(|directory| {
        executable_names(agent).iter().any(|name| {
            extensions
                .iter()
                .any(|extension| directory.join(format!("{name}{extension}")).is_file())
        })
    })
}

fn macos_app_exists(agent: &Agent) -> bool {
    let app_name = format!("{}.app", agent.display_name);
    Path::new("/Applications").join(&app_name).is_dir()
        || resolve_home_dir()
            .join("Applications")
            .join(app_name)
            .is_dir()
}

fn has_system_install_evidence(agent: &Agent, skills_dir: &Path) -> bool {
    skills_dir.starts_with(resolve_home_dir())
        && (executable_exists(agent) || macos_app_exists(agent))
}

/// 첫 실행 추천에는 설정 파일이나 빈 디렉터리를 설치 근거로 사용하지 않는다.
pub(crate) fn is_agent_strongly_detected(agent: &Agent) -> bool {
    has_system_install_evidence(agent, Path::new(&agent.global_skills_dir))
}

/// Returns `true` if the agent appears to be installed on the current machine.
pub fn is_agent_detected(agent: &Agent) -> bool {
    if matches!(agent.id.as_str(), "central" | "universal" | "obsidian") {
        return Path::new(&agent.global_skills_dir).exists();
    }

    if !agent.is_builtin {
        let dir = Path::new(&agent.global_skills_dir);
        return dir.exists() || dir.parent().is_some_and(Path::exists);
    }

    let skills_dir = Path::new(&agent.global_skills_dir);
    if is_shared_agents_skills_dir(skills_dir) {
        return shared_agent_marker_dir(agent, skills_dir)
            .is_some_and(|path| directory_has_entry(&path))
            || has_system_install_evidence(agent, skills_dir);
    }

    has_platform_owned_entry(skills_dir) || has_system_install_evidence(agent, skills_dir)
}

/// Convert a `db::Agent` into `AgentWithStatus` using a live filesystem check.
fn agent_to_with_status(agent: Agent) -> AgentWithStatus {
    let is_detected = is_agent_detected(&agent);
    AgentWithStatus {
        id: agent.id,
        display_name: agent.display_name,
        category: agent.category,
        global_skills_dir: agent.global_skills_dir,
        project_skills_dir: agent.project_skills_dir,
        icon_name: agent.icon_name,
        is_detected,
        is_builtin: agent.is_builtin,
        is_enabled: agent.is_enabled,
    }
}

// ─── Core Logic ───────────────────────────────────────────────────────────────

/// Return all agents from the DB with live detection status.
pub async fn get_agents_impl(pool: &DbPool) -> Result<Vec<AgentWithStatus>, String> {
    let agents = db::get_all_agents(pool).await?;
    Ok(agents.into_iter().map(agent_to_with_status).collect())
}

/// Scan the filesystem to update each agent's `is_detected` flag, then return
/// all agents with their refreshed status.
pub async fn detect_agents_impl(pool: &DbPool) -> Result<Vec<AgentWithStatus>, String> {
    let agents = db::get_all_agents(pool).await?;
    let mut result = Vec::with_capacity(agents.len());

    for agent in agents {
        let is_detected = is_agent_detected(&agent);
        // Best-effort update; ignore errors (e.g., read-only DB in tests).
        let _ = db::update_agent_detected(pool, &agent.id, is_detected).await;

        result.push(AgentWithStatus {
            id: agent.id,
            display_name: agent.display_name,
            category: agent.category,
            global_skills_dir: agent.global_skills_dir,
            project_skills_dir: agent.project_skills_dir,
            icon_name: agent.icon_name,
            is_detected,
            is_builtin: agent.is_builtin,
            is_enabled: agent.is_enabled,
        });
    }

    Ok(result)
}

/// 개별 플랫폼의 스캔·설치 대상 포함 여부를 바꾼다.
pub async fn set_agent_enabled_impl(
    pool: &DbPool,
    agent_id: &str,
    enabled: bool,
) -> Result<AgentWithStatus, String> {
    let agent = db::get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| format!("플랫폼을 찾을 수 없습니다: {agent_id}"))?;

    if matches!(agent.id.as_str(), "central" | "universal" | "obsidian")
        || matches!(agent.category.as_str(), "central" | "shared")
    {
        return Err(format!("활성 상태를 바꿀 수 없는 플랫폼입니다: {agent_id}"));
    }

    db::update_agent_enabled(pool, agent_id, enabled).await?;
    let updated = db::get_agent_by_id(pool, agent_id)
        .await?
        .ok_or_else(|| format!("플랫폼을 다시 읽을 수 없습니다: {agent_id}"))?;

    Ok(agent_to_with_status(updated))
}

/// Insert a new user-defined agent and return its representation.
pub async fn add_custom_agent_impl(
    pool: &DbPool,
    config: CustomAgentConfig,
) -> Result<AgentWithStatus, String> {
    // Derive an ID from the provided value or the display_name.
    let id = match config.id.as_deref() {
        Some(s) if !s.trim().is_empty() => s.trim().to_lowercase().replace(' ', "-"),
        _ => {
            // Auto-generate a slug from the display name, falling back to a UUID.
            let slug = config
                .display_name
                .trim()
                .to_lowercase()
                .replace(' ', "-")
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-')
                .collect::<String>();
            if slug.is_empty() {
                format!("custom-{}", Uuid::new_v4())
            } else {
                format!("custom-{}", slug)
            }
        }
    };

    if id.is_empty() {
        return Err("Agent ID cannot be empty".to_string());
    }

    let category = config.category.unwrap_or_else(|| "other".to_string());
    let global_skills_dir = path_to_string(&expand_home_path(&config.global_skills_dir));

    let agent = Agent {
        id: id.clone(),
        display_name: config.display_name,
        category,
        global_skills_dir,
        project_skills_dir: None,
        icon_name: None,
        is_detected: false, // will be computed live below
        is_builtin: false,
        is_enabled: true,
    };

    db::insert_custom_agent(pool, &agent).await?;

    // Re-fetch so we have the persisted record.
    let persisted = db::get_agent_by_id(pool, &id)
        .await?
        .ok_or_else(|| "Failed to retrieve newly created agent".to_string())?;

    Ok(agent_to_with_status(persisted))
}

/// Update an existing user-defined (non-builtin) agent and return its updated representation.
pub async fn update_custom_agent_impl(
    pool: &DbPool,
    agent_id: &str,
    config: UpdateCustomAgentConfig,
) -> Result<AgentWithStatus, String> {
    if config.display_name.trim().is_empty() {
        return Err("Agent display name cannot be empty".to_string());
    }
    if config.global_skills_dir.trim().is_empty() {
        return Err("Agent global skills directory cannot be empty".to_string());
    }

    let category = config.category.unwrap_or_else(|| "other".to_string());
    let global_skills_dir = path_to_string(&expand_home_path(config.global_skills_dir.trim()));

    let updated = db::update_custom_agent(
        pool,
        agent_id,
        config.display_name.trim(),
        &category,
        &global_skills_dir,
    )
    .await?;

    Ok(agent_to_with_status(updated))
}

/// Remove a user-defined (non-builtin) agent by ID.
pub async fn remove_custom_agent_impl(pool: &DbPool, agent_id: &str) -> Result<(), String> {
    db::delete_custom_agent(pool, agent_id).await
}

// ─── Tauri Commands ───────────────────────────────────────────────────────────

/// Tauri command: return all registered agents with live detection status.
#[tauri::command]
pub async fn get_agents(state: State<'_, AppState>) -> Result<Vec<AgentWithStatus>, String> {
    get_agents_impl(&state.db).await
}

#[tauri::command]
pub async fn set_agent_enabled(
    state: State<'_, AppState>,
    agent_id: String,
    enabled: bool,
) -> Result<AgentWithStatus, String> {
    set_agent_enabled_impl(&state.db, &agent_id, enabled).await
}

/// Tauri command: refresh detection status for all agents and return them.
#[tauri::command]
pub async fn detect_agents(state: State<'_, AppState>) -> Result<Vec<AgentWithStatus>, String> {
    detect_agents_impl(&state.db).await
}

/// Tauri command: register a new user-defined agent.
#[tauri::command]
pub async fn add_custom_agent(
    state: State<'_, AppState>,
    config: CustomAgentConfig,
) -> Result<AgentWithStatus, String> {
    add_custom_agent_impl(&state.db, config).await
}

/// Tauri command: update an existing user-defined agent.
#[tauri::command]
pub async fn update_custom_agent(
    state: State<'_, AppState>,
    agent_id: String,
    config: UpdateCustomAgentConfig,
) -> Result<AgentWithStatus, String> {
    update_custom_agent_impl(&state.db, &agent_id, config).await
}

/// Tauri command: remove a user-defined (non-builtin) agent by ID.
#[tauri::command]
pub async fn remove_custom_agent(
    state: State<'_, AppState>,
    agent_id: String,
) -> Result<(), String> {
    remove_custom_agent_impl(&state.db, &agent_id).await
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use sqlx::SqlitePool;
    use std::fs;
    use tempfile::TempDir;

    async fn setup_test_db() -> DbPool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();
        pool
    }

    // ── is_agent_detected ─────────────────────────────────────────────────────

    fn detection_agent(id: &str, global_skills_dir: &Path, is_builtin: bool) -> Agent {
        Agent {
            id: id.to_string(),
            display_name: id.to_string(),
            category: "coding".to_string(),
            global_skills_dir: global_skills_dir.to_string_lossy().into_owned(),
            project_skills_dir: None,
            icon_name: None,
            is_detected: false,
            is_builtin,
            is_enabled: true,
        }
    }

    #[test]
    fn test_builtin_is_not_detected_from_managed_skills_dir_alone() {
        let tmp = TempDir::new().unwrap();
        let skills_dir = tmp.path().join(".augment/skills");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::write(skills_dir.join("managed-link"), "test").unwrap();

        assert!(
            !is_agent_detected(&detection_agent("augment", &skills_dir, true)),
            "skills-manage가 만든 skills 디렉터리만으로 내장 도구를 감지하면 안 된다"
        );
    }

    #[test]
    fn test_builtin_is_detected_from_platform_owned_entry() {
        let tmp = TempDir::new().unwrap();
        let platform_root = tmp.path().join(".augment");
        let skills_dir = platform_root.join("skills");
        fs::create_dir_all(&skills_dir).unwrap();
        fs::write(platform_root.join("settings.json"), "{}").unwrap();

        assert!(
            is_agent_detected(&detection_agent("augment", &skills_dir, true)),
            "플랫폼이 소유한 설정 파일은 설치 증거로 사용해야 한다"
        );
    }

    #[test]
    fn test_custom_agent_keeps_path_based_detection() {
        let tmp = TempDir::new().unwrap();
        let skills_dir = tmp.path().join("skills");

        assert!(
            is_agent_detected(&detection_agent("custom-test", &skills_dir, false)),
            "사용자가 직접 등록한 플랫폼은 설정한 부모 경로를 신뢰한다"
        );
    }

    // ── get_agents_impl ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_agents_returns_all_builtin() {
        let pool = setup_test_db().await;
        let agents = get_agents_impl(&pool).await.unwrap();
        assert_eq!(
            agents.len(),
            db::builtin_agents().len(),
            "should return all built-in agents"
        );
    }

    #[tokio::test]
    async fn test_get_agents_detected_flag_reflects_fs() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        let skills_dir = tmp.path().join(".claude/skills");
        fs::create_dir_all(skills_dir.parent().unwrap()).unwrap();
        fs::write(skills_dir.parent().unwrap().join("settings.json"), "{}").unwrap();

        // Point claude-code's skills dir at a platform root with its own marker.
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'claude-code'")
            .bind(skills_dir.to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();

        let agents = get_agents_impl(&pool).await.unwrap();
        let claude = agents.iter().find(|a| a.id == "claude-code").unwrap();
        assert!(
            claude.is_detected,
            "claude-code should be detected when its dir exists"
        );
    }

    #[tokio::test]
    async fn test_get_agents_not_detected_when_dir_missing() {
        let pool = setup_test_db().await;

        // Point claude-code at a path whose parent also doesn't exist.
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'claude-code'")
            .bind("/nonexistent/deep/path/skills")
            .execute(&pool)
            .await
            .unwrap();

        let agents = get_agents_impl(&pool).await.unwrap();
        let claude = agents.iter().find(|a| a.id == "claude-code").unwrap();
        assert!(
            !claude.is_detected,
            "claude-code should not be detected when dir and parent both missing"
        );
    }

    // ── detect_agents_impl ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_detect_agents_updates_db() {
        let tmp = TempDir::new().unwrap();
        let pool = setup_test_db().await;
        let skills_dir = tmp.path().join(".claude/skills");
        fs::create_dir_all(skills_dir.parent().unwrap()).unwrap();
        fs::write(skills_dir.parent().unwrap().join("settings.json"), "{}").unwrap();

        // Point claude-code at a platform root with its own marker.
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'claude-code'")
            .bind(skills_dir.to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();

        // Initially not detected in DB.
        let before = db::get_agent_by_id(&pool, "claude-code")
            .await
            .unwrap()
            .unwrap();
        assert!(!before.is_detected);

        detect_agents_impl(&pool).await.unwrap();

        let after = db::get_agent_by_id(&pool, "claude-code")
            .await
            .unwrap()
            .unwrap();
        assert!(
            after.is_detected,
            "DB should reflect detected status after detect_agents"
        );
    }

    #[tokio::test]
    async fn test_detect_agents_returns_all_agents() {
        let pool = setup_test_db().await;
        let agents = detect_agents_impl(&pool).await.unwrap();
        assert_eq!(agents.len(), db::builtin_agents().len());
    }

    #[tokio::test]
    async fn test_set_agent_enabled_updates_single_platform() {
        let pool = setup_test_db().await;

        let disabled = set_agent_enabled_impl(&pool, "claude-code", false)
            .await
            .unwrap();
        assert!(!disabled.is_enabled);
        assert!(
            !db::get_agent_by_id(&pool, "claude-code")
                .await
                .unwrap()
                .unwrap()
                .is_enabled
        );

        let enabled = set_agent_enabled_impl(&pool, "claude-code", true)
            .await
            .unwrap();
        assert!(enabled.is_enabled);
    }

    #[tokio::test]
    async fn test_set_agent_enabled_rejects_managed_roots() {
        let pool = setup_test_db().await;

        assert!(set_agent_enabled_impl(&pool, "central", false)
            .await
            .is_err());
        assert!(set_agent_enabled_impl(&pool, "universal", false)
            .await
            .is_err());
    }

    // ── add_custom_agent_impl ─────────────────────────────────────────────────

    #[tokio::test]
    async fn test_add_custom_agent_appears_in_list() {
        let pool = setup_test_db().await;

        let config = CustomAgentConfig {
            id: Some("my-custom".to_string()),
            display_name: "My Custom Agent".to_string(),
            category: Some("coding".to_string()),
            global_skills_dir: "/tmp/my-custom/skills".to_string(),
        };

        add_custom_agent_impl(&pool, config).await.unwrap();

        let agents = get_agents_impl(&pool).await.unwrap();
        assert_eq!(
            agents.len(),
            db::builtin_agents().len() + 1,
            "should have built-ins + 1 custom"
        );

        let custom = agents.iter().find(|a| a.id == "my-custom").unwrap();
        assert_eq!(custom.display_name, "My Custom Agent");
        assert!(!custom.is_builtin);
    }

    #[tokio::test]
    async fn test_add_custom_agent_auto_generates_id() {
        let pool = setup_test_db().await;

        let config = CustomAgentConfig {
            id: None,
            display_name: "Auto Named".to_string(),
            category: None,
            global_skills_dir: "/tmp/auto/skills".to_string(),
        };

        let agent = add_custom_agent_impl(&pool, config).await.unwrap();
        assert!(
            !agent.id.is_empty(),
            "auto-generated ID should not be empty"
        );
        assert!(
            agent.id.starts_with("custom-"),
            "auto-generated ID should start with 'custom-'"
        );
    }

    #[tokio::test]
    async fn test_add_custom_agent_with_detected_dir() {
        let tmp = TempDir::new().unwrap();
        let skills_dir = tmp.path().join("skills");
        fs::create_dir_all(&skills_dir).unwrap();

        let pool = setup_test_db().await;

        let config = CustomAgentConfig {
            id: Some("detected-agent".to_string()),
            display_name: "Detected Agent".to_string(),
            category: None,
            global_skills_dir: skills_dir.to_string_lossy().into_owned(),
        };

        let agent = add_custom_agent_impl(&pool, config).await.unwrap();
        assert!(
            agent.is_detected,
            "agent should be detected when skills dir exists"
        );
    }

    #[tokio::test]
    async fn test_add_custom_agent_duplicate_id_fails() {
        let pool = setup_test_db().await;

        let config = CustomAgentConfig {
            id: Some("unique-id".to_string()),
            display_name: "First".to_string(),
            category: None,
            global_skills_dir: "/tmp/first/skills".to_string(),
        };
        add_custom_agent_impl(&pool, config).await.unwrap();

        let config2 = CustomAgentConfig {
            id: Some("unique-id".to_string()),
            display_name: "Second".to_string(),
            category: None,
            global_skills_dir: "/tmp/second/skills".to_string(),
        };
        let result = add_custom_agent_impl(&pool, config2).await;
        assert!(result.is_err(), "duplicate agent ID should fail");
    }

    #[tokio::test]
    async fn test_add_custom_agent_default_category() {
        let pool = setup_test_db().await;

        let config = CustomAgentConfig {
            id: Some("no-category".to_string()),
            display_name: "No Category".to_string(),
            category: None, // omitted
            global_skills_dir: "/tmp/nc/skills".to_string(),
        };

        let agent = add_custom_agent_impl(&pool, config).await.unwrap();
        assert_eq!(
            agent.category, "other",
            "default category should be 'other'"
        );
    }

    #[tokio::test]
    async fn test_add_custom_agent_expands_tilde_path() {
        let pool = setup_test_db().await;

        let config = CustomAgentConfig {
            id: Some("tilde-agent".to_string()),
            display_name: "Tilde Agent".to_string(),
            category: Some("coding".to_string()),
            global_skills_dir: "~/.tilde-agent/skills".to_string(),
        };

        let agent = add_custom_agent_impl(&pool, config).await.unwrap();
        assert!(
            !agent.global_skills_dir.starts_with('~'),
            "tilde paths must be expanded before persistence"
        );
        assert!(agent.global_skills_dir.contains(".tilde-agent"));
    }

    // ── update_custom_agent_impl ──────────────────────────────────────────────

    async fn add_test_custom_agent(pool: &DbPool, id: &str) {
        let config = CustomAgentConfig {
            id: Some(id.to_string()),
            display_name: format!("Agent {}", id),
            category: Some("other".to_string()),
            global_skills_dir: format!("/tmp/{}/skills", id),
        };
        add_custom_agent_impl(pool, config).await.unwrap();
    }

    #[tokio::test]
    async fn test_update_custom_agent_changes_fields() {
        let pool = setup_test_db().await;
        add_test_custom_agent(&pool, "update-me").await;

        let config = UpdateCustomAgentConfig {
            display_name: "Updated Name".to_string(),
            category: Some("coding".to_string()),
            global_skills_dir: "/tmp/updated/skills".to_string(),
        };

        let updated = update_custom_agent_impl(&pool, "update-me", config)
            .await
            .unwrap();
        assert_eq!(updated.display_name, "Updated Name");
        assert_eq!(updated.category, "coding");
        assert_eq!(updated.global_skills_dir, "/tmp/updated/skills");
        assert!(!updated.is_builtin);
    }

    #[tokio::test]
    async fn test_update_custom_agent_default_category() {
        let pool = setup_test_db().await;
        add_test_custom_agent(&pool, "cat-default").await;

        let config = UpdateCustomAgentConfig {
            display_name: "Cat Default".to_string(),
            category: None,
            global_skills_dir: "/tmp/cat-default/skills".to_string(),
        };

        let updated = update_custom_agent_impl(&pool, "cat-default", config)
            .await
            .unwrap();
        assert_eq!(
            updated.category, "other",
            "default category should be 'other'"
        );
    }

    #[tokio::test]
    async fn test_update_custom_agent_expands_tilde_path() {
        let pool = setup_test_db().await;
        add_test_custom_agent(&pool, "tilde-update").await;

        let config = UpdateCustomAgentConfig {
            display_name: "Tilde Update".to_string(),
            category: Some("coding".to_string()),
            global_skills_dir: "~/.tilde-update/skills".to_string(),
        };

        let updated = update_custom_agent_impl(&pool, "tilde-update", config)
            .await
            .unwrap();
        assert!(
            !updated.global_skills_dir.starts_with('~'),
            "tilde paths must be expanded before persistence"
        );
        assert!(updated.global_skills_dir.contains(".tilde-update"));
    }

    #[tokio::test]
    async fn test_update_custom_agent_not_found_fails() {
        let pool = setup_test_db().await;

        let config = UpdateCustomAgentConfig {
            display_name: "Ghost".to_string(),
            category: None,
            global_skills_dir: "/tmp/ghost/skills".to_string(),
        };

        let result = update_custom_agent_impl(&pool, "nonexistent-agent", config).await;
        assert!(result.is_err(), "Updating a nonexistent agent should fail");
    }

    #[tokio::test]
    async fn test_update_builtin_agent_fails() {
        let pool = setup_test_db().await;

        let config = UpdateCustomAgentConfig {
            display_name: "Hacked Name".to_string(),
            category: None,
            global_skills_dir: "/tmp/hacked/skills".to_string(),
        };

        let result = update_custom_agent_impl(&pool, "claude-code", config).await;
        assert!(result.is_err(), "Updating a built-in agent should fail");
    }

    #[tokio::test]
    async fn test_update_custom_agent_empty_display_name_fails() {
        let pool = setup_test_db().await;
        add_test_custom_agent(&pool, "empty-name").await;

        let config = UpdateCustomAgentConfig {
            display_name: "   ".to_string(),
            category: None,
            global_skills_dir: "/tmp/empty-name/skills".to_string(),
        };

        let result = update_custom_agent_impl(&pool, "empty-name", config).await;
        assert!(result.is_err(), "Empty display name should fail validation");
    }

    // ── remove_custom_agent_impl ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_remove_custom_agent_success() {
        let pool = setup_test_db().await;
        add_test_custom_agent(&pool, "removable").await;

        remove_custom_agent_impl(&pool, "removable").await.unwrap();

        let agents = get_agents_impl(&pool).await.unwrap();
        assert!(
            agents.iter().all(|a| a.id != "removable"),
            "Removed agent should no longer appear in agent list"
        );
    }

    #[tokio::test]
    async fn test_remove_custom_agent_not_found_fails() {
        let pool = setup_test_db().await;
        let result = remove_custom_agent_impl(&pool, "ghost-agent").await;
        assert!(result.is_err(), "Removing a nonexistent agent should fail");
    }

    #[tokio::test]
    async fn test_remove_builtin_agent_fails() {
        let pool = setup_test_db().await;
        let result = remove_custom_agent_impl(&pool, "cursor").await;
        assert!(result.is_err(), "Removing a built-in agent should fail");
    }
}

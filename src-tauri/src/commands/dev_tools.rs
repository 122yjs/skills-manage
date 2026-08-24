use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::commands::agents::{is_agent_strongly_detected, AgentWithStatus};
use crate::db::{self, DbPool};
use crate::AppState;

const DEV_TOOL_SETUP_SETTING: &str = "dev_tool_setup_completed_v1";

/// 포크 최초 기본 도구와 Paseo 0.2.5 provider의 합집합이다.
/// 첫 화면에서 이 순서로 먼저 배치하되, 나머지 내장 도구도 선택지로 유지한다.
pub const PRIORITIZED_DEV_TOOL_IDS: &[&str] = &[
    "claude-code",
    "codex",
    "cursor",
    "gemini-cli",
    "trae",
    "factory-droid",
    "copilot",
    "opencode",
    "pi",
    "omp",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DevToolSetupState {
    pub completed: bool,
    pub tools: Vec<AgentWithStatus>,
}

pub async fn get_dev_tool_setup_state_impl(pool: &DbPool) -> Result<DevToolSetupState, String> {
    let completed = db::get_setting(pool, DEV_TOOL_SETUP_SETTING)
        .await?
        .is_some_and(|value| value == "true");
    let mut tools = db::get_all_agents(pool)
        .await?
        .into_iter()
        .filter(|agent| agent.is_builtin && agent.category == "coding")
        .map(|agent| AgentWithStatus {
            is_detected: is_agent_strongly_detected(&agent),
            id: agent.id,
            display_name: agent.display_name,
            category: agent.category,
            global_skills_dir: agent.global_skills_dir,
            project_skills_dir: agent.project_skills_dir,
            icon_name: agent.icon_name,
            is_builtin: agent.is_builtin,
            is_enabled: agent.is_enabled,
        })
        .collect::<Vec<_>>();

    tools.sort_by(|left, right| {
        let left_position = PRIORITIZED_DEV_TOOL_IDS
            .iter()
            .position(|id| *id == left.id);
        let right_position = PRIORITIZED_DEV_TOOL_IDS
            .iter()
            .position(|id| *id == right.id);
        match (left_position, right_position) {
            (Some(left), Some(right)) => left.cmp(&right),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left.display_name.cmp(&right.display_name),
        }
    });

    Ok(DevToolSetupState { completed, tools })
}

pub async fn save_dev_tool_selection_impl(
    pool: &DbPool,
    agent_ids: &[String],
) -> Result<DevToolSetupState, String> {
    let allowed = db::get_all_agents(pool)
        .await?
        .into_iter()
        .filter(|agent| agent.is_builtin && agent.category == "coding")
        .map(|agent| agent.id)
        .collect::<HashSet<_>>();
    let selected: HashSet<&str> = agent_ids.iter().map(String::as_str).collect();

    if let Some(invalid) = selected.iter().find(|id| !allowed.contains(**id)) {
        return Err(format!("선택할 수 없는 개발 도구입니다: {invalid}"));
    }

    let mut transaction = pool.begin().await.map_err(|error| error.to_string())?;

    // 기존 대형 내장 카탈로그는 데이터 보존을 위해 삭제하지 않고 비활성화한다.
    sqlx::query("UPDATE agents SET is_enabled = 0 WHERE is_builtin = 1 AND category = 'coding'")
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;

    for agent_id in &selected {
        let result = sqlx::query(
            "UPDATE agents SET is_enabled = 1
             WHERE id = ? AND is_builtin = 1 AND category = 'coding'",
        )
        .bind(agent_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;

        if result.rows_affected() != 1 {
            return Err(format!("개발 도구를 찾을 수 없습니다: {agent_id}"));
        }
    }

    sqlx::query("INSERT OR REPLACE INTO settings (key, value) VALUES (?, 'true')")
        .bind(DEV_TOOL_SETUP_SETTING)
        .execute(&mut *transaction)
        .await
        .map_err(|error| error.to_string())?;

    transaction
        .commit()
        .await
        .map_err(|error| error.to_string())?;

    get_dev_tool_setup_state_impl(pool).await
}

#[tauri::command]
pub async fn get_dev_tool_setup_state(
    state: State<'_, AppState>,
) -> Result<DevToolSetupState, String> {
    get_dev_tool_setup_state_impl(&state.db).await
}

#[tauri::command]
pub async fn save_dev_tool_selection(
    state: State<'_, AppState>,
    agent_ids: Vec<String>,
) -> Result<DevToolSetupState, String> {
    save_dev_tool_selection_impl(&state.db, &agent_ids).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{Row, SqlitePool};

    async fn setup_test_db() -> DbPool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn first_run_orders_common_tools_first_and_keeps_other_options() {
        let pool = setup_test_db().await;
        let state = get_dev_tool_setup_state_impl(&pool).await.unwrap();

        assert!(!state.completed);
        let first_ids = state
            .tools
            .iter()
            .take(PRIORITIZED_DEV_TOOL_IDS.len())
            .map(|tool| tool.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(first_ids, PRIORITIZED_DEV_TOOL_IDS);
        assert!(state.tools.iter().any(|tool| tool.id == "augment"));
        assert!(state.tools.iter().any(|tool| tool.id == "continue"));
        assert!(state.tools.iter().any(|tool| tool.id == "dexto"));
    }

    #[tokio::test]
    async fn saving_selection_disables_unselected_coding_tools() {
        let pool = setup_test_db().await;
        let selected = vec!["codex".to_string(), "augment".to_string()];

        let state = save_dev_tool_selection_impl(&pool, &selected)
            .await
            .unwrap();

        assert!(state.completed);
        assert!(
            state
                .tools
                .iter()
                .find(|tool| tool.id == "codex")
                .unwrap()
                .is_enabled
        );
        assert!(
            state
                .tools
                .iter()
                .find(|tool| tool.id == "augment")
                .unwrap()
                .is_enabled
        );

        let continue_enabled: bool =
            sqlx::query("SELECT is_enabled FROM agents WHERE id = 'continue'")
                .fetch_one(&pool)
                .await
                .unwrap()
                .get("is_enabled");
        assert!(!continue_enabled);
    }

    #[tokio::test]
    async fn rejects_tools_outside_the_curated_catalog() {
        let pool = setup_test_db().await;
        let result = save_dev_tool_selection_impl(&pool, &["not-a-real-tool".to_string()]).await;

        assert!(result.is_err());
        assert!(db::get_setting(&pool, DEV_TOOL_SETUP_SETTING)
            .await
            .unwrap()
            .is_none());
    }
}

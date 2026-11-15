//! 앱 시작 시 데이터 폴더·DB·보관함 초기화와 실패 복구.
//!
//! 초기화가 실패해도 앱을 끄지 않고 실패 원인을 `StartupState`에 보관한다.
//! 화면은 `get_startup_status`로 원인을 읽고, `retry_startup`으로 다시 시도하거나
//! 보관함 경로를 바꿔 다시 시도한다. 성공하면 그때 `AppState`를 등록한다.

use serde::Serialize;
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, State};

use crate::commands::{recovery, settings, skill_origin, storage};
use crate::db::{self, DbPool};
use crate::path_utils::{
    app_data_dir, default_central_skills_dir, expand_home_path, legacy_central_skills_dir,
    path_to_string,
};
use crate::secrets;
use crate::AppState;

/// 마지막 시작 실패 원인. 정상이면 `None`이다.
#[derive(Default)]
pub struct StartupState {
    failure: Mutex<Option<String>>,
}

impl StartupState {
    pub fn set_failure(&self, failure: Option<String>) {
        *self.failure.lock().unwrap() = failure;
    }
}

#[derive(Debug, Serialize)]
pub struct StartupStatus {
    pub ready: bool,
    pub error: Option<String>,
}

/// 데이터 폴더를 만들고 DB·보관함을 초기화한다. `central_override`가 있으면 보관함 경로를 그 폴더로 바꾼다.
pub async fn initialize_app(
    data_dir: &Path,
    default_central: &Path,
    legacy_central: &Path,
    central_override: Option<&Path>,
) -> Result<DbPool, String> {
    fs::create_dir_all(data_dir).map_err(|e| {
        format!(
            "데이터 폴더를 만들 수 없습니다 ({}): {e}",
            data_dir.display()
        )
    })?;
    let db_path = path_to_string(&data_dir.join("db.sqlite"));
    let pool = db::create_pool(&db_path)
        .await
        .map_err(|e| format!("데이터베이스를 열 수 없습니다 ({db_path}): {e}"))?;
    if let Err(error) =
        initialize_pool(&pool, default_central, legacy_central, central_override).await
    {
        pool.close().await;
        return Err(error);
    }
    Ok(pool)
}

async fn initialize_pool(
    pool: &DbPool,
    default_central: &Path,
    legacy_central: &Path,
    central_override: Option<&Path>,
) -> Result<(), String> {
    recovery::snapshot_before_schema_change(pool)
        .await
        .map_err(|e| format!("스키마 변경 전 데이터베이스 백업에 실패했습니다: {e}"))?;
    db::init_database(pool)
        .await
        .map_err(|e| format!("데이터베이스 스키마를 초기화할 수 없습니다: {e}"))?;
    // 이전 실패는 다음 실행에서 다시 시도하므로 시작을 막지 않는다.
    if let Err(error) = settings::migrate_legacy_secrets_impl(pool, secrets::shared()).await {
        eprintln!("Could not migrate legacy secrets: {error}");
    }
    skill_origin::reconcile_incomplete_updates(pool)
        .await
        .map_err(|e| format!("중단된 스킬 업데이트를 정리할 수 없습니다: {e}"))?;
    if let Some(path) = central_override {
        // 쓸 수 없는 폴더는 설정에 저장하지 않는다.
        fs::create_dir_all(path)
            .map_err(|e| format!("보관함 폴더를 사용할 수 없습니다 ({}): {e}", path.display()))?;
        db::set_setting(pool, db::CENTRAL_SKILLS_PATH_SETTING, &path_to_string(path)).await?;
    }
    let current = db::get_central_skills_dir(pool).await?;
    storage::initialize_storage_impl(pool, default_central, legacy_central)
        .await
        .map_err(|e| {
            format!(
                "보관함 폴더를 사용할 수 없습니다 ({}): {e}",
                current.display()
            )
        })
}

/// 실제 사용자 폴더 기준으로 초기화한다.
pub async fn initialize_default(central_override: Option<&Path>) -> Result<DbPool, String> {
    initialize_app(
        &app_data_dir(),
        &default_central_skills_dir(),
        &legacy_central_skills_dir(),
        central_override,
    )
    .await
}

#[tauri::command]
pub fn get_startup_status(startup: State<'_, StartupState>) -> StartupStatus {
    let error = startup.failure.lock().unwrap().clone();
    StartupStatus {
        ready: error.is_none(),
        error,
    }
}

/// 시작 초기화를 다시 시도한다. `central_path`가 있으면 보관함 경로를 바꾼 뒤 시도한다.
#[tauri::command]
pub async fn retry_startup(
    app: AppHandle,
    startup: State<'_, StartupState>,
    central_path: Option<String>,
) -> Result<(), String> {
    if app.try_state::<AppState>().is_some() {
        return Ok(());
    }
    let central_override = central_path.map(|path| expand_home_path(path.trim()));
    match initialize_default(central_override.as_deref()).await {
        Ok(db) => {
            app.manage(AppState { db });
            startup.set_failure(None);
            Ok(())
        }
        Err(error) => {
            startup.set_failure(Some(error.clone()));
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn unusable_vault_path_fails_then_new_path_recovers() {
        let temp = tempdir().unwrap();
        let data_dir = temp.path().join("data");
        let default_central = data_dir.join("skills");
        let legacy = temp.path().join("legacy");
        // 보관함 경로의 부모가 일반 파일이라 폴더를 만들 수 없는 상황
        let blocker = temp.path().join("blocker");
        fs::write(&blocker, "file").unwrap();
        let broken_vault = blocker.join("vault");
        let pool = initialize_app(&data_dir, &default_central, &legacy, None)
            .await
            .unwrap();
        db::set_setting(
            &pool,
            db::CENTRAL_SKILLS_PATH_SETTING,
            &path_to_string(&broken_vault),
        )
        .await
        .unwrap();
        pool.close().await;

        let error = initialize_app(&data_dir, &default_central, &legacy, None)
            .await
            .unwrap_err();
        assert!(
            error.contains(&broken_vault.display().to_string()),
            "{error}"
        );

        let new_vault = temp.path().join("new-vault");
        let pool = initialize_app(&data_dir, &default_central, &legacy, Some(&new_vault))
            .await
            .unwrap();
        assert_eq!(db::get_central_skills_dir(&pool).await.unwrap(), new_vault);
        assert!(new_vault.is_dir());
    }

    #[tokio::test]
    async fn startup_moves_legacy_secrets_to_the_keychain_and_repeats_safely() {
        let temp = tempdir().unwrap();
        let data_dir = temp.path().join("data");
        let default_central = data_dir.join("skills");
        let legacy = temp.path().join("legacy");
        let key = "ai_api_key__startup-test";
        let pool = initialize_app(&data_dir, &default_central, &legacy, None)
            .await
            .unwrap();
        db::set_setting(&pool, key, "sk-old").await.unwrap();
        pool.close().await;

        for _ in 0..2 {
            let pool = initialize_app(&data_dir, &default_central, &legacy, None)
                .await
                .unwrap();
            assert_eq!(
                db::get_setting(&pool, key).await.unwrap().as_deref(),
                Some(secrets::CONFIGURED_MARKER)
            );
            assert_eq!(secrets::shared().get(key).as_deref(), Some("sk-old"));
            pool.close().await;
        }
    }

    #[tokio::test]
    async fn unusable_data_dir_returns_error_instead_of_panicking() {
        let temp = tempdir().unwrap();
        let blocker = temp.path().join("blocker");
        fs::write(&blocker, "file").unwrap();

        let error = initialize_app(
            &blocker.join("data"),
            &temp.path().join("skills"),
            &temp.path().join("legacy"),
            None,
        )
        .await
        .unwrap_err();
        assert!(error.contains("데이터 폴더"), "{error}");
    }
}

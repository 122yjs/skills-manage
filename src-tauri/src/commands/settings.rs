use serde::Serialize;
use tauri::State;

use crate::commands::recovery;
use crate::db::{self, DbPool, ScanDirectory};
use crate::path_utils::{expand_home_path, path_to_string};
use crate::secrets::{self, Secrets, CONFIGURED_MARKER};
use crate::AppState;

// ─── Core Implementations (testable without Tauri State) ──────────────────────

/// Return all scan directories, built-in first then custom ordered by added_at.
pub async fn get_scan_directories_impl(pool: &DbPool) -> Result<Vec<ScanDirectory>, String> {
    // 도구 표시 상태와 스캔 디렉터리 목록은 독립적이다. 숨긴 도구의
    // 기본 디렉터리도 설정에서 계속 보여야 스캔 대상을 확인할 수 있다.
    let builtin_paths = db::get_all_agents(pool)
        .await?
        .into_iter()
        .filter(|agent| agent.is_builtin)
        .map(|agent| agent.global_skills_dir)
        .collect::<std::collections::HashSet<_>>();
    let directories = db::get_scan_directories(pool).await?;

    Ok(directories
        .into_iter()
        .filter(|directory| !directory.is_builtin || builtin_paths.contains(&directory.path))
        .collect())
}

/// Add a new custom (non-builtin) scan directory.
/// Returns the newly created record.
pub async fn add_scan_directory_impl(
    pool: &DbPool,
    path: &str,
    label: Option<&str>,
) -> Result<ScanDirectory, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("Scan directory path cannot be empty".to_string());
    }
    let expanded_path = path_to_string(&expand_home_path(path));
    db::add_scan_directory(pool, &expanded_path, label).await
}

/// Remove a custom (non-builtin) scan directory by path.
/// Returns an error if the directory is built-in or not found.
pub async fn remove_scan_directory_impl(pool: &DbPool, path: &str) -> Result<(), String> {
    db::remove_scan_directory(pool, path).await
}

/// Toggle the `is_active` flag on a scan directory by path.
pub async fn set_scan_directory_active_impl(
    pool: &DbPool,
    path: &str,
    is_active: bool,
) -> Result<(), String> {
    db::toggle_scan_directory(pool, path, is_active).await
}

/// Get a settings value by key. Returns `None` if the key is not set.
/// 비밀값은 화면으로 보내지 않으므로 `get_secret_status`로 설정 여부만 조회한다.
pub async fn get_setting_impl(pool: &DbPool, key: &str) -> Result<Option<String>, String> {
    if secrets::is_secret_key(key) {
        return Err(SECRET_KEY_ERROR.to_string());
    }
    db::get_setting(pool, key).await
}

/// Set (upsert) a settings value.
pub async fn set_setting_impl(pool: &DbPool, key: &str, value: &str) -> Result<(), String> {
    if key.trim().is_empty() {
        return Err("Settings key cannot be empty".to_string());
    }
    if secrets::is_secret_key(key) {
        return Err(SECRET_KEY_ERROR.to_string());
    }
    if key == db::CENTRAL_SKILLS_PATH_SETTING {
        return Err("Use change_central_path to move the Central vault safely".to_string());
    }
    db::set_setting(pool, key, value).await
}

const SECRET_KEY_ERROR: &str = "Secrets are stored in the OS keychain; use set_secret";

/// 화면에는 비밀값 자체가 아니라 설정 여부와 키체인 사용 가능 여부만 전달한다.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretStatus {
    pub configured: bool,
    pub keychain_available: bool,
}

fn ensure_secret_key(key: &str) -> Result<(), String> {
    if secrets::is_secret_key(key) {
        Ok(())
    } else {
        Err(format!("'{key}' is not a secret setting"))
    }
}

fn secret_status(secrets: &Secrets, key: &str) -> SecretStatus {
    SecretStatus {
        configured: secrets.get(key).is_some(),
        keychain_available: secrets.keychain_available(),
    }
}

pub fn get_secret_status_impl(secrets: &Secrets, key: &str) -> Result<SecretStatus, String> {
    ensure_secret_key(key)?;
    Ok(secret_status(secrets, key))
}

/// 비밀값을 키체인에 저장하고 DB에는 설정 여부 표시만 남긴다. 빈 값이면 지운다.
/// 키체인을 쓸 수 없으면 메모리에만 두고 DB에는 아무것도 남기지 않는다.
pub async fn set_secret_impl(
    pool: &DbPool,
    secrets: &Secrets,
    key: &str,
    value: &str,
) -> Result<SecretStatus, String> {
    ensure_secret_key(key)?;
    let value = value.trim();
    let marker = if value.is_empty() {
        secrets.delete(key)?;
        ""
    } else if secrets.set(key, value) {
        CONFIGURED_MARKER
    } else {
        ""
    };
    db::set_setting(pool, key, marker).await?;
    Ok(secret_status(secrets, key))
}

// ─── 이전 버전이 DB에 남긴 평문 비밀값 (ADR-0003) ──────────────────────────────

/// DB 설정 칸의 비밀값을 `value`로 바꾼다. 바꾼 뒤 이전 값이 DB·WAL 파일에 남지 않게 덮어쓴다.
async fn overwrite_secret_settings(
    pool: &DbPool,
    keys: &[String],
    value: &str,
) -> Result<(), String> {
    if keys.is_empty() {
        return Ok(());
    }
    let mut connection = pool.acquire().await.map_err(|error| error.to_string())?;
    sqlx::query("PRAGMA secure_delete = ON")
        .execute(&mut *connection)
        .await
        .map_err(|error| error.to_string())?;
    for key in keys {
        sqlx::query("UPDATE settings SET value = ? WHERE key = ?")
            .bind(value)
            .bind(key)
            .execute(&mut *connection)
            .await
            .map_err(|error| error.to_string())?;
    }
    for statement in [
        "PRAGMA secure_delete = OFF",
        "PRAGMA wal_checkpoint(TRUNCATE)",
    ] {
        sqlx::query(statement)
            .execute(&mut *connection)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// 시작할 때 실행한다. 이전 버전이 DB에 평문으로 남긴 비밀값을 키체인으로 옮기고 DB 값을 지운다.
/// 키체인에 저장하고 다시 읽어 확인한 값만 지우므로, 중간에 실패해도 값을 잃지 않고 다음 실행에서 이어진다.
/// 키체인을 쓸 수 없으면 DB와 백업의 값을 그대로 두고(이번 실행에서는 메모리에서 쓴다) 화면에서 지울지 묻는다.
pub async fn migrate_legacy_secrets_impl(pool: &DbPool, secrets: &Secrets) -> Result<(), String> {
    let mut moved = Vec::new();
    for (key, value) in db::plaintext_secret_settings(pool).await? {
        if secrets.set(&key, value.trim()) {
            moved.push(key);
        }
    }
    overwrite_secret_settings(pool, &moved, CONFIGURED_MARKER).await?;
    if secrets.keychain_available() {
        recovery::blank_secrets_in_database_backups(pool).await?;
    }
    Ok(())
}

/// DB에 평문 비밀값이 남아 있는지 확인한다. 값 자체는 돌려주지 않는다.
pub async fn has_legacy_secrets_impl(pool: &DbPool) -> Result<bool, String> {
    Ok(!db::plaintext_secret_settings(pool).await?.is_empty())
}

/// 사용자가 동의하면 DB와 DB 백업에 남은 이전 평문 비밀값을 모두 지운다.
pub async fn clear_legacy_secrets_impl(pool: &DbPool, secrets: &Secrets) -> Result<(), String> {
    let keys: Vec<String> = db::plaintext_secret_settings(pool)
        .await?
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    for key in &keys {
        secrets.delete(key)?;
    }
    overwrite_secret_settings(pool, &keys, "").await?;
    recovery::blank_secrets_in_database_backups(pool).await
}

// ─── Tauri Commands ───────────────────────────────────────────────────────────

/// Tauri command: return all scan directories.
#[tauri::command]
pub async fn get_scan_directories(
    state: State<'_, AppState>,
) -> Result<Vec<ScanDirectory>, String> {
    get_scan_directories_impl(&state.db).await
}

/// Tauri command: add a new custom scan directory.
#[tauri::command]
pub async fn add_scan_directory(
    state: State<'_, AppState>,
    path: String,
    label: Option<String>,
) -> Result<ScanDirectory, String> {
    add_scan_directory_impl(&state.db, &path, label.as_deref()).await
}

/// Tauri command: remove a custom scan directory by path.
#[tauri::command]
pub async fn remove_scan_directory(state: State<'_, AppState>, path: String) -> Result<(), String> {
    remove_scan_directory_impl(&state.db, &path).await
}

/// Tauri command: set the is_active flag on a scan directory.
#[tauri::command]
pub async fn set_scan_directory_active(
    state: State<'_, AppState>,
    path: String,
    is_active: bool,
) -> Result<(), String> {
    set_scan_directory_active_impl(&state.db, &path, is_active).await
}

/// Tauri command: get a settings value by key.
#[tauri::command]
pub async fn get_setting(
    state: State<'_, AppState>,
    key: String,
) -> Result<Option<String>, String> {
    get_setting_impl(&state.db, &key).await
}

/// Tauri command: set (upsert) a settings value.
#[tauri::command]
pub async fn set_setting(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    set_setting_impl(&state.db, &key, &value).await
}

/// Tauri command: 비밀값의 설정 여부만 조회한다. 값은 반환하지 않는다.
#[tauri::command]
pub async fn get_secret_status(key: String) -> Result<SecretStatus, String> {
    get_secret_status_impl(secrets::shared(), &key)
}

/// Tauri command: 비밀값을 OS 키체인에 저장한다. 빈 값이면 지운다.
#[tauri::command]
pub async fn set_secret(
    state: State<'_, AppState>,
    key: String,
    value: String,
) -> Result<SecretStatus, String> {
    set_secret_impl(&state.db, secrets::shared(), &key, &value).await
}

/// Tauri command: 키체인으로 옮기지 못한 이전 평문 비밀값이 DB에 남아 있는지 조회한다.
#[tauri::command]
pub async fn has_legacy_secrets(state: State<'_, AppState>) -> Result<bool, String> {
    has_legacy_secrets_impl(&state.db).await
}

/// Tauri command: 사용자가 동의하면 남은 이전 평문 비밀값을 DB와 DB 백업에서 지운다.
#[tauri::command]
pub async fn clear_legacy_secrets(state: State<'_, AppState>) -> Result<(), String> {
    clear_legacy_secrets_impl(&state.db, secrets::shared()).await
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::recovery;
    use crate::db;
    use crate::secrets::{MemoryStore, SecretStore, UnavailableStore};
    use sqlx::SqlitePool;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn setup_test_db() -> DbPool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();
        pool
    }

    // ── get_scan_directories_impl ─────────────────────────────────────────────

    /// Counts unique global_skills_dir paths across all built-in agents — the
    /// same number that seed_builtin_scan_directories inserts.
    fn expected_builtin_count() -> usize {
        let mut paths = std::collections::HashSet::new();
        for agent in db::builtin_agents() {
            paths.insert(agent.global_skills_dir);
        }
        paths.len()
    }

    #[tokio::test]
    async fn test_get_scan_directories_has_builtin_dirs_initially() {
        let pool = setup_test_db().await;
        let dirs = get_scan_directories_impl(&pool).await.unwrap();
        let builtin_count = expected_builtin_count();
        // After init, built-in scan directories are seeded automatically.
        assert_eq!(
            dirs.len(),
            builtin_count,
            "Fresh database should have {} built-in scan directories, got {}",
            builtin_count,
            dirs.len()
        );
        // All seeded rows must be marked built-in.
        for dir in &dirs {
            assert!(
                dir.is_builtin,
                "Scan directory '{}' seeded during init must have is_builtin=true",
                dir.path
            );
        }
    }

    #[tokio::test]
    async fn hidden_builtin_agent_keeps_its_scan_directory_in_settings() {
        let pool = setup_test_db().await;
        let hidden_agent = db::builtin_agents()
            .into_iter()
            .find(|agent| agent.id == "claude-code")
            .unwrap();
        db::update_agent_enabled(&pool, &hidden_agent.id, false)
            .await
            .unwrap();

        let directories = get_scan_directories_impl(&pool).await.unwrap();

        assert!(directories.iter().any(|directory| {
            directory.is_builtin && directory.path == hidden_agent.global_skills_dir
        }));
    }

    #[tokio::test]
    async fn test_get_scan_directories_returns_added() {
        let pool = setup_test_db().await;
        add_scan_directory_impl(&pool, "/tmp/proj-a", Some("Project A"))
            .await
            .unwrap();
        add_scan_directory_impl(&pool, "/tmp/proj-b", None)
            .await
            .unwrap();

        let dirs = get_scan_directories_impl(&pool).await.unwrap();
        // N built-in dirs are already there; we added 2 custom ones.
        let builtin_count = expected_builtin_count();
        assert_eq!(dirs.len(), builtin_count + 2);
        let paths: Vec<&str> = dirs.iter().map(|d| d.path.as_str()).collect();
        assert!(paths.contains(&"/tmp/proj-a"));
        assert!(paths.contains(&"/tmp/proj-b"));
    }

    // ── add_scan_directory_impl ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_add_scan_directory_creates_non_builtin() {
        let pool = setup_test_db().await;
        let dir = add_scan_directory_impl(&pool, "/tmp/my-project", Some("My Project"))
            .await
            .unwrap();

        assert_eq!(dir.path, "/tmp/my-project");
        assert_eq!(dir.label.as_deref(), Some("My Project"));
        assert!(dir.is_active);
        assert!(
            !dir.is_builtin,
            "Newly added directory should not be built-in"
        );
    }

    #[tokio::test]
    async fn test_add_scan_directory_without_label() {
        let pool = setup_test_db().await;
        let dir = add_scan_directory_impl(&pool, "/tmp/no-label", None)
            .await
            .unwrap();
        assert!(dir.label.is_none());
    }

    #[tokio::test]
    async fn test_add_scan_directory_expands_tilde() {
        let pool = setup_test_db().await;
        let dir = add_scan_directory_impl(&pool, "~/.skillsmanage/custom-scan", None)
            .await
            .unwrap();
        assert!(
            !dir.path.starts_with('~'),
            "tilde paths must be expanded before persistence"
        );
        assert!(dir.path.contains(".skillsmanage"));
    }

    #[tokio::test]
    async fn test_add_scan_directory_empty_path_fails() {
        let pool = setup_test_db().await;
        let result = add_scan_directory_impl(&pool, "   ", None).await;
        assert!(result.is_err(), "Empty path should fail validation");
    }

    #[tokio::test]
    async fn test_add_scan_directory_duplicate_path_fails() {
        let pool = setup_test_db().await;
        add_scan_directory_impl(&pool, "/tmp/same-path", None)
            .await
            .unwrap();
        let result = add_scan_directory_impl(&pool, "/tmp/same-path", None).await;
        assert!(
            result.is_err(),
            "Duplicate path should fail (UNIQUE constraint)"
        );
    }

    // ── remove_scan_directory_impl ────────────────────────────────────────────

    #[tokio::test]
    async fn test_remove_scan_directory_success() {
        let pool = setup_test_db().await;
        add_scan_directory_impl(&pool, "/tmp/removable", None)
            .await
            .unwrap();

        remove_scan_directory_impl(&pool, "/tmp/removable")
            .await
            .unwrap();

        let dirs = get_scan_directories_impl(&pool).await.unwrap();
        // Built-in dirs remain; only the custom /tmp/removable should be gone.
        let builtin_count = expected_builtin_count();
        assert_eq!(
            dirs.len(),
            builtin_count,
            "Only the custom directory should be removed"
        );
        assert!(
            !dirs.iter().any(|d| d.path == "/tmp/removable"),
            "Removed directory must not appear in the list"
        );
    }

    #[tokio::test]
    async fn test_remove_nonexistent_scan_directory_fails() {
        let pool = setup_test_db().await;
        let result = remove_scan_directory_impl(&pool, "/nonexistent/path").await;
        assert!(
            result.is_err(),
            "Removing a nonexistent directory should fail"
        );
    }

    #[tokio::test]
    async fn test_remove_builtin_scan_directory_fails() {
        let pool = setup_test_db().await;
        // Manually insert a builtin directory
        sqlx::query(
            "INSERT INTO scan_directories (path, is_active, is_builtin, added_at)
             VALUES ('/builtin/path', 1, 1, datetime('now'))",
        )
        .execute(&pool)
        .await
        .unwrap();

        let result = remove_scan_directory_impl(&pool, "/builtin/path").await;
        assert!(result.is_err(), "Removing a built-in directory should fail");
    }

    // ── set_scan_directory_active_impl ────────────────────────────────────────

    #[tokio::test]
    async fn test_set_scan_directory_active_disables() {
        let pool = setup_test_db().await;
        add_scan_directory_impl(&pool, "/tmp/toggle-me", None)
            .await
            .unwrap();
        set_scan_directory_active_impl(&pool, "/tmp/toggle-me", false)
            .await
            .unwrap();
        let dirs = get_scan_directories_impl(&pool).await.unwrap();
        let dir = dirs.iter().find(|d| d.path == "/tmp/toggle-me").unwrap();
        assert!(!dir.is_active, "Directory should be inactive");
    }

    #[tokio::test]
    async fn test_set_scan_directory_active_enables() {
        let pool = setup_test_db().await;
        add_scan_directory_impl(&pool, "/tmp/re-enable-me", None)
            .await
            .unwrap();
        // First disable
        set_scan_directory_active_impl(&pool, "/tmp/re-enable-me", false)
            .await
            .unwrap();
        // Then re-enable
        set_scan_directory_active_impl(&pool, "/tmp/re-enable-me", true)
            .await
            .unwrap();
        let dirs = get_scan_directories_impl(&pool).await.unwrap();
        let dir = dirs.iter().find(|d| d.path == "/tmp/re-enable-me").unwrap();
        assert!(dir.is_active, "Directory should be active again");
    }

    // ── get_setting_impl ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_setting_not_set_returns_none() {
        let pool = setup_test_db().await;
        let value = get_setting_impl(&pool, "unset_key").await.unwrap();
        assert!(value.is_none(), "Unset key should return None");
    }

    #[tokio::test]
    async fn test_get_setting_after_set() {
        let pool = setup_test_db().await;
        set_setting_impl(&pool, "theme", "dark").await.unwrap();

        let value = get_setting_impl(&pool, "theme").await.unwrap();
        assert_eq!(value.as_deref(), Some("dark"));
    }

    // ── set_setting_impl ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_set_setting_upserts() {
        let pool = setup_test_db().await;
        set_setting_impl(&pool, "lang", "en").await.unwrap();
        set_setting_impl(&pool, "lang", "zh").await.unwrap();

        let value = get_setting_impl(&pool, "lang").await.unwrap();
        assert_eq!(
            value.as_deref(),
            Some("zh"),
            "Second set should overwrite first"
        );
    }

    #[tokio::test]
    async fn test_set_setting_empty_key_fails() {
        let pool = setup_test_db().await;
        let result = set_setting_impl(&pool, "  ", "some-value").await;
        assert!(result.is_err(), "Empty key should fail validation");
    }

    #[tokio::test]
    async fn test_central_path_requires_safe_move_command() {
        let pool = setup_test_db().await;
        let result = set_setting_impl(
            &pool,
            db::CENTRAL_SKILLS_PATH_SETTING,
            "/tmp/unsafe-direct-change",
        )
        .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_set_and_get_multiple_settings() {
        let pool = setup_test_db().await;
        set_setting_impl(&pool, "a", "1").await.unwrap();
        set_setting_impl(&pool, "b", "2").await.unwrap();
        set_setting_impl(&pool, "c", "3").await.unwrap();

        assert_eq!(
            get_setting_impl(&pool, "a").await.unwrap().as_deref(),
            Some("1")
        );
        assert_eq!(
            get_setting_impl(&pool, "b").await.unwrap().as_deref(),
            Some("2")
        );
        assert_eq!(
            get_setting_impl(&pool, "c").await.unwrap().as_deref(),
            Some("3")
        );
    }

    #[tokio::test]
    async fn test_set_setting_empty_value_is_allowed() {
        let pool = setup_test_db().await;
        // Empty value is valid — it means the key is explicitly set to empty string.
        let result = set_setting_impl(&pool, "empty-val", "").await;
        assert!(result.is_ok(), "Setting an empty value should succeed");
        let value = get_setting_impl(&pool, "empty-val").await.unwrap();
        assert_eq!(value.as_deref(), Some(""));
    }

    // ── secrets ───────────────────────────────────────────────────────────────

    fn memory_secrets() -> (Arc<MemoryStore>, Secrets) {
        let keychain = Arc::new(MemoryStore::default());
        (keychain.clone(), Secrets::new(keychain))
    }

    #[tokio::test]
    async fn test_set_secret_keeps_the_value_in_the_keychain_and_only_a_marker_in_the_db() {
        let pool = setup_test_db().await;
        let (keychain, secrets) = memory_secrets();

        let status = set_secret_impl(&pool, &secrets, "github_pat", "  ghp_secret  ")
            .await
            .unwrap();

        assert!(status.configured && status.keychain_available);
        assert_eq!(
            keychain.get("github_pat").unwrap().as_deref(),
            Some("ghp_secret")
        );
        assert_eq!(
            db::get_setting(&pool, "github_pat")
                .await
                .unwrap()
                .as_deref(),
            Some(CONFIGURED_MARKER)
        );

        let status = set_secret_impl(&pool, &secrets, "github_pat", "")
            .await
            .unwrap();
        assert!(!status.configured);
        assert_eq!(keychain.get("github_pat").unwrap(), None);
        assert_eq!(
            db::get_setting(&pool, "github_pat")
                .await
                .unwrap()
                .as_deref(),
            Some("")
        );
    }

    #[tokio::test]
    async fn test_set_secret_never_writes_plaintext_to_the_db_without_a_keychain() {
        let pool = setup_test_db().await;
        let secrets = Secrets::new(Arc::new(UnavailableStore));

        let status = set_secret_impl(&pool, &secrets, "ai_api_key__claude", "sk-secret")
            .await
            .unwrap();

        assert!(status.configured && !status.keychain_available);
        assert_eq!(
            secrets.get("ai_api_key__claude").as_deref(),
            Some("sk-secret")
        );
        assert_eq!(
            db::get_setting(&pool, "ai_api_key__claude")
                .await
                .unwrap()
                .as_deref(),
            Some("")
        );
    }

    #[tokio::test]
    async fn test_generic_settings_commands_refuse_secret_keys() {
        let pool = setup_test_db().await;

        assert!(set_setting_impl(&pool, "github_pat", "ghp_secret")
            .await
            .is_err());
        assert!(set_setting_impl(&pool, "ai_api_key__claude", "sk-secret")
            .await
            .is_err());
        assert!(get_setting_impl(&pool, "github_pat").await.is_err());
        assert!(set_secret_impl(&pool, &memory_secrets().1, "theme", "dark")
            .await
            .is_err());
    }

    // ── 이전 버전의 평문 비밀값 이전 ──────────────────────────────────────────

    /// 저장은 성공했다고 답하지만 다시 읽으면 값이 없는 키체인을 흉내 낸다.
    struct ForgetfulStore;

    impl SecretStore for ForgetfulStore {
        fn get(&self, _key: &str) -> Result<Option<String>, String> {
            Ok(None)
        }
        fn set(&self, _key: &str, _value: &str) -> Result<(), String> {
            Ok(())
        }
        fn delete(&self, _key: &str) -> Result<(), String> {
            Ok(())
        }
    }

    const GITHUB_OLD: &str = "ghp_0123456789abcdefghijklmnopqrstuvwxyz";
    const AI_OLD: &str = "sk-ant-0123456789abcdefghijklmnopqrstuvwxyz";

    /// 값의 끝 16바이트가 DB 파일이나 WAL에 남아 있으면 true. 지운 값이 빈 공간에 남는 경우를 잡는다.
    fn file_leaks(path: &std::path::Path, secret: &str) -> bool {
        let tail = &secret.as_bytes()[secret.len() - 16..];
        let wal = std::path::PathBuf::from(format!("{}-wal", path.display()));
        [path.to_path_buf(), wal]
            .iter()
            .filter_map(|file| std::fs::read(file).ok())
            .any(|bytes| bytes.windows(tail.len()).any(|window| window == tail))
    }

    async fn setup_file_db(temp: &TempDir) -> DbPool {
        let path = temp.path().join("db.sqlite");
        let pool = db::create_pool(path.to_str().unwrap()).await.unwrap();
        db::init_database(&pool).await.unwrap();
        pool
    }

    /// 이전 버전처럼 비밀값이 평문으로 든 DB를 만든다.
    async fn setup_legacy_db(temp: &TempDir) -> DbPool {
        let pool = setup_file_db(temp).await;
        db::set_setting(&pool, "github_pat", GITHUB_OLD)
            .await
            .unwrap();
        db::set_setting(&pool, "ai_api_key__claude", AI_OLD)
            .await
            .unwrap();
        db::set_setting(&pool, "ai_provider", "claude")
            .await
            .unwrap();
        pool
    }

    async fn open_backup(path: &str) -> SqlitePool {
        SqlitePool::connect(&format!("sqlite://{path}?mode=ro"))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn test_migrate_moves_db_plaintext_to_the_keychain_and_is_repeatable() {
        let temp = TempDir::new().unwrap();
        let pool = setup_legacy_db(&temp).await;
        let (keychain, secrets) = memory_secrets();

        migrate_legacy_secrets_impl(&pool, &secrets).await.unwrap();
        migrate_legacy_secrets_impl(&pool, &secrets).await.unwrap();

        assert_eq!(
            keychain.get("github_pat").unwrap().as_deref(),
            Some(GITHUB_OLD)
        );
        assert_eq!(
            keychain.get("ai_api_key__claude").unwrap().as_deref(),
            Some(AI_OLD)
        );
        for key in ["github_pat", "ai_api_key__claude"] {
            assert_eq!(
                db::get_setting(&pool, key).await.unwrap().as_deref(),
                Some(CONFIGURED_MARKER)
            );
        }
        assert_eq!(
            db::get_setting(&pool, "ai_provider")
                .await
                .unwrap()
                .as_deref(),
            Some("claude")
        );

        // 앱이 DB를 연 채로도, 지운 값은 DB 파일과 WAL의 빈 공간에 남지 않는다.
        for old in [GITHUB_OLD, AI_OLD] {
            assert!(!file_leaks(&temp.path().join("db.sqlite"), old), "{old}");
        }
    }

    #[tokio::test]
    async fn test_migrate_keeps_the_db_value_when_the_keychain_save_is_not_confirmed() {
        let temp = TempDir::new().unwrap();
        let pool = setup_legacy_db(&temp).await;
        let secrets = Secrets::new(Arc::new(ForgetfulStore));

        migrate_legacy_secrets_impl(&pool, &secrets).await.unwrap();

        assert_eq!(
            db::get_setting(&pool, "github_pat")
                .await
                .unwrap()
                .as_deref(),
            Some(GITHUB_OLD)
        );
        assert!(has_legacy_secrets_impl(&pool).await.unwrap());
    }

    #[tokio::test]
    async fn test_migrate_blanks_secret_columns_in_database_backups_and_keeps_the_files() {
        let temp = TempDir::new().unwrap();
        let pool = setup_legacy_db(&temp).await;
        let entry = recovery::create_database_backup_impl(&pool).await.unwrap();
        // 설정에서 이미 지운 키도 백업에는 남아 있을 수 있다.
        db::set_setting(&pool, "github_pat", "").await.unwrap();
        let (_keychain, secrets) = memory_secrets();

        migrate_legacy_secrets_impl(&pool, &secrets).await.unwrap();

        let backup = open_backup(&entry.backup_path).await;
        for key in ["github_pat", "ai_api_key__claude"] {
            assert_eq!(
                db::get_setting(&backup, key).await.unwrap().as_deref(),
                Some(""),
                "{key}"
            );
        }
        assert_eq!(
            db::get_setting(&backup, "ai_provider")
                .await
                .unwrap()
                .as_deref(),
            Some("claude")
        );
        backup.close().await;
        for old in [GITHUB_OLD, AI_OLD] {
            assert!(
                !file_leaks(std::path::Path::new(&entry.backup_path), old),
                "{old}"
            );
        }
        let listed = recovery::list_recovery_entries_impl(&pool).await.unwrap();
        assert!(listed.iter().any(|item| item.id == entry.id));
    }

    #[tokio::test]
    async fn test_migrate_never_deletes_old_values_when_the_keychain_is_unavailable() {
        let temp = TempDir::new().unwrap();
        let pool = setup_legacy_db(&temp).await;
        let entry = recovery::create_database_backup_impl(&pool).await.unwrap();
        let secrets = Secrets::new(Arc::new(UnavailableStore));

        migrate_legacy_secrets_impl(&pool, &secrets).await.unwrap();

        assert_eq!(
            db::get_setting(&pool, "github_pat")
                .await
                .unwrap()
                .as_deref(),
            Some(GITHUB_OLD)
        );
        let backup = open_backup(&entry.backup_path).await;
        assert_eq!(
            db::get_setting(&backup, "github_pat")
                .await
                .unwrap()
                .as_deref(),
            Some(GITHUB_OLD)
        );
        assert!(has_legacy_secrets_impl(&pool).await.unwrap());
        // 이번 실행 동안은 이전 값을 메모리에서 계속 쓸 수 있다.
        assert_eq!(secrets.get("github_pat").as_deref(), Some(GITHUB_OLD));
    }

    #[tokio::test]
    async fn test_clear_legacy_secrets_wipes_db_and_backups_once_the_user_agrees() {
        let temp = TempDir::new().unwrap();
        let pool = setup_legacy_db(&temp).await;
        let entry = recovery::create_database_backup_impl(&pool).await.unwrap();
        let secrets = Secrets::new(Arc::new(UnavailableStore));
        migrate_legacy_secrets_impl(&pool, &secrets).await.unwrap();

        clear_legacy_secrets_impl(&pool, &secrets).await.unwrap();

        assert!(!has_legacy_secrets_impl(&pool).await.unwrap());
        assert_eq!(secrets.get("github_pat"), None);
        let backup = open_backup(&entry.backup_path).await;
        assert_eq!(
            db::get_setting(&backup, "github_pat")
                .await
                .unwrap()
                .as_deref(),
            Some("")
        );
        assert_eq!(
            db::get_setting(&pool, "ai_provider")
                .await
                .unwrap()
                .as_deref(),
            Some("claude")
        );
    }
}

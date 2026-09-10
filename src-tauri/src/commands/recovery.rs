use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::OnceLock;
use tauri::State;
use tokio::sync::{Mutex, MutexGuard};
use uuid::Uuid;

use crate::db::{self, DbPool, SkillInstallation};
use crate::AppState;

const COPY_BACKUP: &str = "copy_backup";
const VAULT_TRASH: &str = "vault_trash";
const DATABASE_BACKUP: &str = "database";
const RECOVERY_RETENTION_DAYS: i64 = 30;

static RECOVERY_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// 설정 화면과 IPC에서 사용하는 복구 항목입니다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryEntry {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub original_path: String,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub backup_path: String,
}

/// 파일 내용과 함께 보존해야 하는 복사 설치 기록입니다.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecoveryManifest {
    #[serde(flatten)]
    entry: RecoveryEntry,
    copy_installation: Option<SkillInstallation>,
}

struct RecoveryLayout {
    root: PathBuf,
    manifests: PathBuf,
    copy_backups: PathBuf,
    vault_trash: PathBuf,
    database: PathBuf,
}

/// 동일 프로세스 안에서 복원과 영구 삭제가 겹치지 않도록 사용하는 잠금입니다.
pub async fn recovery_lock() -> MutexGuard<'static, ()> {
    RECOVERY_LOCK.get_or_init(|| Mutex::new(())).lock().await
}

fn valid_entry_id(id: &str) -> bool {
    Uuid::parse_str(id).is_ok()
}

fn valid_kind(kind: &str) -> bool {
    matches!(kind, COPY_BACKUP | VAULT_TRASH | DATABASE_BACKUP)
}

fn safe_child_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\'])
        && Path::new(name).components().count() == 1
}

fn canonical_parent_path(path: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err(format!(
            "복구 경로는 절대 경로여야 합니다: {}",
            path.display()
        ));
    }
    let file_name = path
        .file_name()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| format!("복구 경로에 파일 이름이 없습니다: {}", path.display()))?;
    let parent = path
        .parent()
        .ok_or_else(|| format!("복구 경로에 상위 폴더가 없습니다: {}", path.display()))?
        .canonicalize()
        .map_err(|error| format!("복구 경로의 상위 폴더를 확인할 수 없습니다: {error}"))?;
    Ok(parent.join(file_name))
}

fn ensure_real_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "심볼릭 링크를 복구 저장소로 사용할 수 없습니다: {}",
            path.display()
        )),
        Ok(metadata) if metadata.is_dir() => {
            restrict_recovery_directory(path)?;
            Ok(())
        }
        Ok(_) => Err(format!("폴더가 아닌 경로입니다: {}", path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let parent = path
                .parent()
                .ok_or_else(|| format!("복구 저장소의 상위 폴더가 없습니다: {}", path.display()))?;
            match fs::symlink_metadata(parent) {
                Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
                    "심볼릭 링크 아래에 복구 저장소를 만들 수 없습니다: {}",
                    parent.display()
                )),
                Ok(metadata) if metadata.is_dir() => fs::create_dir(path)
                    .map_err(|create_error| {
                        format!("복구 저장소 폴더를 만들 수 없습니다: {create_error}")
                    })
                    .and_then(|_| restrict_recovery_directory(path)),
                Ok(_) => Err(format!("상위 경로가 폴더가 아닙니다: {}", parent.display())),
                Err(parent_error) => Err(format!(
                    "복구 저장소의 상위 폴더를 확인할 수 없습니다: {parent_error}"
                )),
            }
        }
        Err(error) => Err(format!("복구 저장소를 확인할 수 없습니다: {error}")),
    }
}

#[cfg(unix)]
fn restrict_recovery_directory(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        format!(
            "복구 저장소의 권한을 제한할 수 없습니다 '{}': {error}",
            path.display()
        )
    })
}

#[cfg(not(unix))]
fn restrict_recovery_directory(_path: &Path) -> Result<(), String> {
    Ok(())
}

async fn database_file_path(pool: &DbPool) -> Result<Option<PathBuf>, String> {
    let rows = sqlx::query("PRAGMA database_list")
        .fetch_all(pool)
        .await
        .map_err(|error| format!("데이터베이스 위치를 확인할 수 없습니다: {error}"))?;
    let file = rows
        .iter()
        .find(|row| row.get::<String, _>("name") == "main")
        .map(|row| row.get::<String, _>("file"))
        .unwrap_or_default();

    if file.is_empty() || file == ":memory:" {
        return Ok(None);
    }

    let path = PathBuf::from(file);
    if !path.is_absolute() {
        return Err(format!(
            "데이터베이스 파일 위치가 절대 경로가 아닙니다: {}",
            path.display()
        ));
    }
    canonical_parent_path(&path).map(Some)
}

async fn recovery_layout(pool: &DbPool) -> Result<Option<RecoveryLayout>, String> {
    let Some(database_path) = database_file_path(pool).await? else {
        // 메모리 DB에는 안전하게 연결할 영속 경로가 없으므로 사용자 폴더를 추정하지 않습니다.
        return Ok(None);
    };
    let database_parent = database_path.parent().ok_or_else(|| {
        format!(
            "데이터베이스 상위 폴더를 확인할 수 없습니다: {}",
            database_path.display()
        )
    })?;
    let root = database_parent.join("recovery");
    ensure_real_directory(&root)?;

    let manifests = root.join("manifests");
    let copy_backups = root.join("copy-backups");
    let vault_trash = root.join("vault-trash");
    let database = root.join("database");
    for directory in [&manifests, &copy_backups, &vault_trash, &database] {
        ensure_real_directory(directory)?;
    }

    Ok(Some(RecoveryLayout {
        root,
        manifests,
        copy_backups,
        vault_trash,
        database,
    }))
}

fn entry_data_path(layout: &RecoveryLayout, kind: &str, id: &str) -> Result<PathBuf, String> {
    if !valid_entry_id(id) || !valid_kind(kind) {
        return Err("유효하지 않은 복구 항목입니다".to_string());
    }
    match kind {
        COPY_BACKUP => Ok(layout.copy_backups.join(id)),
        VAULT_TRASH => Ok(layout.vault_trash.join(id)),
        DATABASE_BACKUP => Ok(layout.database.join(format!("{id}.sqlite"))),
        _ => unreachable!(),
    }
}

fn manifest_path(layout: &RecoveryLayout, id: &str) -> Result<PathBuf, String> {
    if !valid_entry_id(id) {
        return Err("유효하지 않은 복구 항목 ID입니다".to_string());
    }
    Ok(layout.manifests.join(format!("{id}.json")))
}

fn is_path_inside(path: &Path, root: &Path) -> bool {
    path.starts_with(root) && path != root
}

fn remove_path_without_following_links(path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "경로를 확인할 수 없습니다 '{}': {error}",
                path.display()
            ))
        }
    };

    if metadata.file_type().is_symlink() || metadata.is_file() {
        return fs::remove_file(path)
            .map_err(|error| format!("파일을 지울 수 없습니다 '{}': {error}", path.display()));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)
            .map_err(|error| format!("폴더를 읽을 수 없습니다 '{}': {error}", path.display()))?
        {
            let entry = entry.map_err(|error| format!("폴더 항목을 읽을 수 없습니다: {error}"))?;
            remove_path_without_following_links(&entry.path())?;
        }
        return fs::remove_dir(path)
            .map_err(|error| format!("폴더를 지울 수 없습니다 '{}': {error}", path.display()));
    }
    Err(format!("지원하지 않는 파일 형식입니다: {}", path.display()))
}

fn sync_backup_path(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        format!(
            "백업 파일을 확인할 수 없습니다 '{}': {error}",
            path.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_file() {
        fs::File::open(path)
            .and_then(|file| file.sync_all())
            .map_err(|error| {
                format!(
                    "백업 파일을 기록할 수 없습니다 '{}': {error}",
                    path.display()
                )
            })?;
        return Ok(());
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|error| {
            format!("백업 폴더를 읽을 수 없습니다 '{}': {error}", path.display())
        })? {
            let entry =
                entry.map_err(|error| format!("백업 폴더 항목을 읽을 수 없습니다: {error}"))?;
            sync_backup_path(&entry.path())?;
        }
        #[cfg(unix)]
        {
            fs::File::open(path)
                .and_then(|directory| directory.sync_all())
                .map_err(|error| {
                    format!(
                        "백업 폴더를 기록할 수 없습니다 '{}': {error}",
                        path.display()
                    )
                })?;
        }
        return Ok(());
    }
    Err(format!(
        "지원하지 않는 백업 파일 형식입니다: {}",
        path.display()
    ))
}

#[cfg(unix)]
fn copy_symlink(source: &Path, destination: &Path) -> Result<(), String> {
    let target = fs::read_link(source).map_err(|error| {
        format!(
            "심볼릭 링크를 읽을 수 없습니다 '{}': {error}",
            source.display()
        )
    })?;
    std::os::unix::fs::symlink(&target, destination).map_err(|error| {
        format!(
            "심볼릭 링크를 보존할 수 없습니다 '{}': {error}",
            source.display()
        )
    })
}

#[cfg(windows)]
fn copy_symlink(source: &Path, destination: &Path) -> Result<(), String> {
    let target = fs::read_link(source).map_err(|error| {
        format!(
            "심볼릭 링크를 읽을 수 없습니다 '{}': {error}",
            source.display()
        )
    })?;
    if fs::metadata(source)
        .map(|metadata| metadata.is_dir())
        .unwrap_or(false)
    {
        std::os::windows::fs::symlink_dir(&target, destination)
    } else {
        std::os::windows::fs::symlink_file(&target, destination)
    }
    .map_err(|error| {
        format!(
            "심볼릭 링크를 보존할 수 없습니다 '{}': {error}",
            source.display()
        )
    })
}

#[cfg(not(any(unix, windows)))]
fn copy_symlink(_source: &Path, _destination: &Path) -> Result<(), String> {
    Err("이 운영체제에서는 심볼릭 링크 백업을 지원하지 않습니다".to_string())
}

/// 심볼릭 링크 대상은 따라가지 않고 링크 자체만 보존합니다.
fn copy_entry_without_following_links(source: &Path, destination: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        format!(
            "백업 원본을 확인할 수 없습니다 '{}': {error}",
            source.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return copy_symlink(source, destination);
    }
    if metadata.is_file() {
        return fs::copy(source, destination)
            .map(|_| ())
            .map_err(|error| format!("파일을 백업할 수 없습니다 '{}': {error}", source.display()));
    }
    if metadata.is_dir() {
        fs::create_dir(destination).map_err(|error| {
            format!(
                "백업 폴더를 만들 수 없습니다 '{}': {error}",
                destination.display()
            )
        })?;
        let mut entries = fs::read_dir(source)
            .map_err(|error| {
                format!(
                    "백업 원본을 읽을 수 없습니다 '{}': {error}",
                    source.display()
                )
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("백업 원본 항목을 읽을 수 없습니다: {error}"))?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            copy_entry_without_following_links(
                &entry.path(),
                &destination.join(entry.file_name()),
            )?;
        }
        fs::set_permissions(destination, metadata.permissions()).map_err(|error| {
            format!(
                "백업 폴더의 권한을 보존할 수 없습니다 '{}': {error}",
                destination.display()
            )
        })?;
        return Ok(());
    }
    Err(format!(
        "지원하지 않는 백업 원본 형식입니다: {}",
        source.display()
    ))
}

/// 원래 위치을 먼저 원자적으로 예약해 기존 항목을 절대 덮어쓰지 않습니다.
fn restore_entry_without_overwriting(source: &Path, target: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(source).map_err(|error| {
        format!(
            "복구 파일을 확인할 수 없습니다 '{}': {error}",
            source.display()
        )
    })?;
    if metadata.file_type().is_symlink() {
        return copy_symlink(source, target).map_err(|error| {
            format!(
                "원래 위치에 복원할 수 없습니다 '{}': {error}",
                target.display()
            )
        });
    }
    if metadata.is_file() {
        use std::io::copy;

        let mut input = fs::File::open(source).map_err(|error| {
            format!("복구 파일을 열 수 없습니다 '{}': {error}", source.display())
        })?;
        // create_new가 성공한 뒤에만 아래 오류에서 새 파일을 정리합니다.
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)
            .map_err(|error| {
                format!(
                    "원래 위치가 비어 있지 않아 복원할 수 없습니다 '{}': {error}",
                    target.display()
                )
            })?;
        let result = (|| {
            copy(&mut input, &mut output)?;
            output.sync_all()?;
            fs::set_permissions(target, metadata.permissions())?;
            Ok::<(), std::io::Error>(())
        })();
        if let Err(error) = result {
            let _ = remove_path_without_following_links(target);
            return Err(format!(
                "파일을 복원할 수 없습니다 '{}': {error}",
                target.display()
            ));
        }
        return Ok(());
    }
    if metadata.is_dir() {
        fs::create_dir(target).map_err(|error| {
            format!(
                "원래 위치가 비어 있지 않아 복원할 수 없습니다 '{}': {error}",
                target.display()
            )
        })?;
        let result = (|| {
            let mut entries = fs::read_dir(source)
                .map_err(|error| {
                    format!(
                        "복구 폴더를 읽을 수 없습니다 '{}': {error}",
                        source.display()
                    )
                })?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("복구 폴더 항목을 읽을 수 없습니다: {error}"))?;
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                copy_entry_without_following_links(&entry.path(), &target.join(entry.file_name()))?;
            }
            fs::set_permissions(target, metadata.permissions()).map_err(|error| {
                format!(
                    "폴더 권한을 복원할 수 없습니다 '{}': {error}",
                    target.display()
                )
            })?;
            sync_backup_path(target)
        })();
        if let Err(error) = result {
            let _ = remove_path_without_following_links(target);
            return Err(error);
        }
        return Ok(());
    }
    Err(format!(
        "지원하지 않는 복구 파일 형식입니다: {}",
        source.display()
    ))
}

fn write_manifest(layout: &RecoveryLayout, manifest: &RecoveryManifest) -> Result<(), String> {
    let manifest_path = manifest_path(layout, &manifest.entry.id)?;
    let temp_path = layout
        .manifests
        .join(format!(".{}.partial", manifest.entry.id));
    if fs::symlink_metadata(&manifest_path).is_ok() || fs::symlink_metadata(&temp_path).is_ok() {
        return Err("같은 복구 항목 ID가 이미 있습니다".to_string());
    }
    let contents = serde_json::to_vec_pretty(manifest)
        .map_err(|error| format!("복구 정보를 저장할 수 없습니다: {error}"))?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|error| format!("복구 정보를 임시 저장할 수 없습니다: {error}"))?;
    use std::io::Write;
    file.write_all(&contents)
        .and_then(|_| file.sync_all())
        .map_err(|error| format!("복구 정보를 기록할 수 없습니다: {error}"))?;
    fs::rename(&temp_path, &manifest_path)
        .map_err(|error| format!("복구 정보를 공개할 수 없습니다: {error}"))
}

fn validate_manifest(
    layout: &RecoveryLayout,
    manifest: RecoveryManifest,
) -> Result<RecoveryManifest, String> {
    let entry = &manifest.entry;
    if !valid_entry_id(&entry.id) || !valid_kind(&entry.kind) || entry.label.trim().is_empty() {
        return Err("복구 정보가 유효하지 않습니다".to_string());
    }
    let expected_backup = entry_data_path(layout, &entry.kind, &entry.id)?;
    let saved_backup = PathBuf::from(&entry.backup_path);
    if saved_backup != expected_backup || !is_path_inside(&expected_backup, &layout.root) {
        return Err("복구 정보의 백업 경로가 유효하지 않습니다".to_string());
    }
    let original = Path::new(&entry.original_path);
    if !original.is_absolute()
        || original
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err("복구 정보의 원래 경로가 유효하지 않습니다".to_string());
    }
    if entry.kind == DATABASE_BACKUP {
        if entry.expires_at.is_some() || manifest.copy_installation.is_some() {
            return Err("데이터베이스 복구 정보가 유효하지 않습니다".to_string());
        }
    } else {
        let expires_at = entry
            .expires_at
            .as_deref()
            .ok_or_else(|| "파일 복구 정보에 만료 시각이 없습니다".to_string())?;
        DateTime::parse_from_rfc3339(expires_at)
            .map_err(|_| "파일 복구 정보의 만료 시각이 유효하지 않습니다".to_string())?;
    }
    if entry.kind == COPY_BACKUP {
        let installation = manifest
            .copy_installation
            .as_ref()
            .ok_or_else(|| "복사 설치 정보가 없습니다".to_string())?;
        if installation.link_type != "copy" || installation.installed_path != entry.original_path {
            return Err("복사 설치 정보가 유효하지 않습니다".to_string());
        }
    } else if manifest.copy_installation.is_some() {
        return Err("복구 정보의 설치 메타데이터가 유효하지 않습니다".to_string());
    }
    Ok(manifest)
}

fn read_manifest(layout: &RecoveryLayout, id: &str) -> Result<RecoveryManifest, String> {
    let path = manifest_path(layout, id)?;
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| format!("복구 정보를 찾을 수 없습니다: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("복구 정보 파일이 안전하지 않습니다".to_string());
    }
    let contents =
        fs::read(&path).map_err(|error| format!("복구 정보를 읽을 수 없습니다: {error}"))?;
    let manifest = serde_json::from_slice(&contents)
        .map_err(|error| format!("복구 정보를 해석할 수 없습니다: {error}"))?;
    let manifest = validate_manifest(layout, manifest)?;
    if manifest.entry.id != id {
        return Err("복구 항목 ID가 일치하지 않습니다".to_string());
    }
    Ok(manifest)
}

fn backup_data_exists(layout: &RecoveryLayout, manifest: &RecoveryManifest) -> bool {
    entry_data_path(layout, &manifest.entry.kind, &manifest.entry.id)
        .ok()
        .and_then(|path| fs::symlink_metadata(path).ok())
        .is_some()
}

fn is_expired(entry: &RecoveryEntry, now: DateTime<Utc>) -> bool {
    if entry.kind == DATABASE_BACKUP {
        return false;
    }
    entry
        .expires_at
        .as_deref()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .is_some_and(|value| value.with_timezone(&Utc) <= now)
}

fn delete_manifest_entry(
    layout: &RecoveryLayout,
    manifest: &RecoveryManifest,
) -> Result<(), String> {
    let backup_path = entry_data_path(layout, &manifest.entry.kind, &manifest.entry.id)?;
    let manifest_path = manifest_path(layout, &manifest.entry.id)?;
    remove_path_without_following_links(&backup_path)?;
    fs::remove_file(&manifest_path)
        .map_err(|error| format!("복구 정보를 지울 수 없습니다: {error}"))
}

fn cleanup_expired_entries(layout: &RecoveryLayout) -> Result<(), String> {
    let now = Utc::now();
    for entry in fs::read_dir(&layout.manifests)
        .map_err(|error| format!("복구 목록을 읽을 수 없습니다: {error}"))?
    {
        let entry = entry.map_err(|error| format!("복구 목록 항목을 읽을 수 없습니다: {error}"))?;
        let path = entry.path();
        let Some(id) = path.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };
        if path.extension().and_then(|extension| extension.to_str()) != Some("json")
            || !valid_entry_id(id)
        {
            continue;
        }
        let Ok(manifest) = read_manifest(layout, id) else {
            continue;
        };
        if is_expired(&manifest.entry, now) {
            delete_manifest_entry(layout, &manifest)?;
        }
    }
    Ok(())
}

fn create_file_backup(
    layout: &RecoveryLayout,
    kind: &str,
    label: String,
    source: &Path,
    copy_installation: Option<SkillInstallation>,
) -> Result<RecoveryEntry, String> {
    let original_path = canonical_parent_path(source)?;
    if original_path.starts_with(&layout.root) || layout.root.starts_with(&original_path) {
        return Err("복구 저장소와 겹치는 경로는 백업할 수 없습니다".to_string());
    }
    fs::symlink_metadata(&original_path).map_err(|error| {
        format!(
            "백업 원본을 찾을 수 없습니다 '{}': {error}",
            original_path.display()
        )
    })?;

    let id = Uuid::new_v4().to_string();
    let data_path = entry_data_path(layout, kind, &id)?;
    let data_parent = data_path
        .parent()
        .ok_or_else(|| "복구 저장소를 확인할 수 없습니다".to_string())?;
    let stage = data_parent.join(format!(".{id}.partial"));
    if fs::symlink_metadata(&data_path).is_ok() || fs::symlink_metadata(&stage).is_ok() {
        return Err("같은 복구 항목 ID가 이미 있습니다".to_string());
    }

    if let Err(error) = copy_entry_without_following_links(&original_path, &stage) {
        let _ = remove_path_without_following_links(&stage);
        return Err(error);
    }
    if let Err(error) = sync_backup_path(&stage) {
        let _ = remove_path_without_following_links(&stage);
        return Err(error);
    }
    if let Err(error) = fs::rename(&stage, &data_path) {
        let _ = remove_path_without_following_links(&stage);
        return Err(format!("백업을 완성할 수 없습니다: {error}"));
    }

    let created_at = Utc::now();
    let entry = RecoveryEntry {
        id,
        kind: kind.to_string(),
        label,
        original_path: original_path.to_string_lossy().into_owned(),
        created_at: created_at.to_rfc3339(),
        expires_at: Some((created_at + Duration::days(RECOVERY_RETENTION_DAYS)).to_rfc3339()),
        backup_path: data_path.to_string_lossy().into_owned(),
    };
    let copy_installation = copy_installation.map(|mut installation| {
        installation.installed_path = original_path.to_string_lossy().into_owned();
        installation
    });
    let manifest = RecoveryManifest {
        entry: entry.clone(),
        copy_installation,
    };
    if let Err(error) = write_manifest(layout, &manifest) {
        let _ = remove_path_without_following_links(&data_path);
        return Err(error);
    }
    Ok(entry)
}

async fn expected_copy_install_path(
    pool: &DbPool,
    installation: &SkillInstallation,
) -> Result<PathBuf, String> {
    if installation.link_type != "copy" || !safe_child_name(&installation.skill_id) {
        return Err("복사 설치 정보가 유효하지 않습니다".to_string());
    }
    let agent = db::get_agent_by_id(pool, &installation.agent_id)
        .await?
        .ok_or_else(|| format!("설치 플랫폼을 찾을 수 없습니다: {}", installation.agent_id))?;
    let agent_root = PathBuf::from(&agent.global_skills_dir)
        .canonicalize()
        .map_err(|error| format!("설치 폴더를 확인할 수 없습니다: {error}"))?;
    let expected = agent_root.join(&installation.skill_id);
    let recorded = canonical_parent_path(Path::new(&installation.installed_path))?;
    if recorded != expected {
        return Err("복사 설치 경로가 플랫폼 폴더와 일치하지 않습니다".to_string());
    }
    Ok(expected)
}

/// 복사 설치를 지우기 직전에 전체 내용을 보존합니다.
pub async fn backup_copy_installation(
    pool: &DbPool,
    installation: &SkillInstallation,
) -> Result<Option<RecoveryEntry>, String> {
    let _guard = recovery_lock().await;
    backup_copy_installation_locked(pool, installation).await
}

/// 호출자는 복사 설치 삭제가 끝날 때까지 `recovery_lock`을 유지해야 합니다.
pub async fn backup_copy_installation_locked(
    pool: &DbPool,
    installation: &SkillInstallation,
) -> Result<Option<RecoveryEntry>, String> {
    let Some(layout) = recovery_layout(pool).await? else {
        return Ok(None);
    };
    let source = expected_copy_install_path(pool, installation).await?;
    create_file_backup(
        &layout,
        COPY_BACKUP,
        format!("복사 설치 백업: {}", installation.skill_id),
        &source,
        Some(installation.clone()),
    )
    .map(Some)
}

/// 보관함 원본을 삭제하기 전에 파일 휴지통과 DB 스냅샷을 함께 만듭니다.
pub async fn backup_vault_before_removal(
    pool: &DbPool,
    source: &Path,
    label: String,
) -> Result<Option<RecoveryEntry>, String> {
    let _guard = recovery_lock().await;
    backup_vault_before_removal_locked(pool, source, label).await
}

async fn backup_vault_before_removal_locked(
    pool: &DbPool,
    source: &Path,
    label: String,
) -> Result<Option<RecoveryEntry>, String> {
    let Some(layout) = recovery_layout(pool).await? else {
        return Ok(None);
    };
    let _ = snapshot_database_locked(pool, &layout, "보관함 변경 전 데이터베이스 백업").await?;
    create_file_backup(&layout, VAULT_TRASH, label, source, None).map(Some)
}

async fn snapshot_database_locked(
    pool: &DbPool,
    layout: &RecoveryLayout,
    label: &str,
) -> Result<RecoveryEntry, String> {
    let database_path = database_file_path(pool)
        .await?
        .ok_or_else(|| "메모리 데이터베이스는 파일 백업을 만들 수 없습니다".to_string())?;
    let id = Uuid::new_v4().to_string();
    let data_path = entry_data_path(layout, DATABASE_BACKUP, &id)?;
    let stage = layout.database.join(format!(".{id}.partial.sqlite"));
    if fs::symlink_metadata(&data_path).is_ok() || fs::symlink_metadata(&stage).is_ok() {
        return Err("같은 복구 항목 ID가 이미 있습니다".to_string());
    }

    if let Err(error) = sqlx::query("VACUUM INTO ?")
        .bind(stage.to_string_lossy().into_owned())
        .execute(pool)
        .await
    {
        let _ = remove_path_without_following_links(&stage);
        return Err(format!("데이터베이스 스냅샷을 만들 수 없습니다: {error}"));
    }
    let stage_metadata = fs::symlink_metadata(&stage)
        .map_err(|error| format!("데이터베이스 스냅샷을 확인할 수 없습니다: {error}"))?;
    if stage_metadata.file_type().is_symlink() || !stage_metadata.is_file() {
        let _ = remove_path_without_following_links(&stage);
        return Err("데이터베이스 스냅샷 파일이 안전하지 않습니다".to_string());
    }
    if let Err(error) = fs::File::open(&stage).and_then(|file| file.sync_all()) {
        let _ = remove_path_without_following_links(&stage);
        return Err(format!("데이터베이스 스냅샷을 기록할 수 없습니다: {error}"));
    }
    if let Err(error) = fs::rename(&stage, &data_path) {
        let _ = remove_path_without_following_links(&stage);
        return Err(format!("데이터베이스 스냅샷을 완성할 수 없습니다: {error}"));
    }

    let entry = RecoveryEntry {
        id,
        kind: DATABASE_BACKUP.to_string(),
        label: label.to_string(),
        original_path: database_path.to_string_lossy().into_owned(),
        created_at: Utc::now().to_rfc3339(),
        expires_at: None,
        backup_path: data_path.to_string_lossy().into_owned(),
    };
    let manifest = RecoveryManifest {
        entry: entry.clone(),
        copy_installation: None,
    };
    if let Err(error) = write_manifest(layout, &manifest) {
        let _ = remove_path_without_following_links(&data_path);
        return Err(error);
    }
    Ok(entry)
}

/// 수동·자동 DB 백업에서 공통으로 쓰는 SQLite 일관성 스냅샷입니다.
pub async fn snapshot_database(
    pool: &DbPool,
    label: &str,
) -> Result<Option<RecoveryEntry>, String> {
    let _guard = recovery_lock().await;
    let Some(layout) = recovery_layout(pool).await? else {
        return Ok(None);
    };
    snapshot_database_locked(pool, &layout, label)
        .await
        .map(Some)
}

/// 기존 DB가 있을 때만 앱 초기화 전에 스냅샷을 남깁니다.
pub async fn snapshot_before_startup(pool: &DbPool) -> Result<(), String> {
    let _guard = recovery_lock().await;
    let Some(layout) = recovery_layout(pool).await? else {
        return Ok(());
    };
    let has_user_table = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master
            WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
        )",
    )
    .fetch_one(pool)
    .await
    .map_err(|error| format!("기존 데이터베이스 상태를 확인할 수 없습니다: {error}"))?
        != 0;
    if has_user_table {
        snapshot_database_locked(pool, &layout, "앱 시작 전 데이터베이스 백업").await?;
    }
    Ok(())
}

pub async fn list_recovery_entries_impl(pool: &DbPool) -> Result<Vec<RecoveryEntry>, String> {
    let _guard = recovery_lock().await;
    let Some(layout) = recovery_layout(pool).await? else {
        return Ok(Vec::new());
    };
    cleanup_expired_entries(&layout)?;

    let mut entries = Vec::new();
    for directory_entry in fs::read_dir(&layout.manifests)
        .map_err(|error| format!("복구 목록을 읽을 수 없습니다: {error}"))?
    {
        let directory_entry = directory_entry
            .map_err(|error| format!("복구 목록 항목을 읽을 수 없습니다: {error}"))?;
        let path = directory_entry.path();
        let Some(id) = path.file_stem().and_then(|name| name.to_str()) else {
            continue;
        };
        if path.extension().and_then(|extension| extension.to_str()) != Some("json")
            || !valid_entry_id(id)
        {
            continue;
        }
        if let Ok(manifest) = read_manifest(&layout, id) {
            if !backup_data_exists(&layout, &manifest) {
                continue;
            }
            if !manifest_target_is_allowed(pool, &manifest).await {
                continue;
            }
            entries.push(manifest.entry);
        }
    }
    entries.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(entries)
}

#[tauri::command]
pub async fn list_recovery_entries(
    state: State<'_, AppState>,
) -> Result<Vec<RecoveryEntry>, String> {
    list_recovery_entries_impl(&state.db).await
}

fn validate_empty_restore_target(target: &Path) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or_else(|| format!("복원할 경로에 상위 폴더가 없습니다: {}", target.display()))?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|error| format!("복원할 상위 폴더를 확인할 수 없습니다: {error}"))?;
    if canonical_parent != parent {
        return Err("심볼릭 링크를 거치는 경로에는 복원할 수 없습니다".to_string());
    }
    for ancestor in parent.ancestors() {
        let metadata = fs::symlink_metadata(ancestor)
            .map_err(|error| format!("복원 경로를 확인할 수 없습니다: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err("심볼릭 링크를 거치는 경로에는 복원할 수 없습니다".to_string());
        }
    }
    match fs::symlink_metadata(target) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err(format!(
            "원래 위치에 이미 항목이 있어 복원할 수 없습니다: {}",
            target.display()
        )),
        Err(error) => Err(format!("복원 위치를 확인할 수 없습니다: {error}")),
    }
}

async fn restore_copy_target(
    pool: &DbPool,
    manifest: &RecoveryManifest,
) -> Result<(PathBuf, SkillInstallation), String> {
    let installation = manifest
        .copy_installation
        .as_ref()
        .ok_or_else(|| "복사 설치 정보가 없습니다".to_string())?
        .clone();
    let expected = expected_copy_install_path(pool, &installation).await?;
    if expected.to_string_lossy() != manifest.entry.original_path {
        return Err("복사 설치 복원 경로가 유효하지 않습니다".to_string());
    }
    Ok((expected, installation))
}

async fn restore_vault_target(
    pool: &DbPool,
    manifest: &RecoveryManifest,
) -> Result<PathBuf, String> {
    let central_root = PathBuf::from(db::get_central_skills_dir(pool).await?)
        .canonicalize()
        .map_err(|error| format!("현재 보관함 위치를 확인할 수 없습니다: {error}"))?;
    let target = PathBuf::from(&manifest.entry.original_path);
    if !is_path_inside(&target, &central_root) {
        return Err("보관함 밖의 경로로는 복원할 수 없습니다".to_string());
    }
    Ok(target)
}

async fn manifest_target_is_allowed(pool: &DbPool, manifest: &RecoveryManifest) -> bool {
    match manifest.entry.kind.as_str() {
        COPY_BACKUP => restore_copy_target(pool, manifest).await.is_ok(),
        VAULT_TRASH => restore_vault_target(pool, manifest).await.is_ok(),
        DATABASE_BACKUP => true,
        _ => false,
    }
}

/// 파일 항목을 원래 위치에 복원합니다. DB 백업은 앱을 종료한 뒤 수동으로 사용합니다.
pub async fn restore_recovery_entry_impl(pool: &DbPool, id: &str) -> Result<(), String> {
    let _guard = recovery_lock().await;
    let layout = recovery_layout(pool)
        .await?
        .ok_or_else(|| "메모리 데이터베이스에는 복구 항목이 없습니다".to_string())?;
    let manifest = read_manifest(&layout, &id)?;
    if manifest.entry.kind == DATABASE_BACKUP {
        return Err("데이터베이스 백업은 앱을 종료한 뒤 수동으로 복원해야 합니다".to_string());
    }
    if is_expired(&manifest.entry, Utc::now()) {
        delete_manifest_entry(&layout, &manifest)?;
        return Err("복구 보관 기간이 지났습니다".to_string());
    }
    let data_path = entry_data_path(&layout, &manifest.entry.kind, &manifest.entry.id)?;
    fs::symlink_metadata(&data_path)
        .map_err(|error| format!("복구 파일을 찾을 수 없습니다: {error}"))?;

    let (target, installation) = match manifest.entry.kind.as_str() {
        COPY_BACKUP => {
            let (target, installation) = restore_copy_target(pool, &manifest).await?;
            (target, Some(installation))
        }
        VAULT_TRASH => (restore_vault_target(pool, &manifest).await?, None),
        _ => unreachable!(),
    };
    validate_empty_restore_target(&target)?;
    restore_entry_without_overwriting(&data_path, &target)?;

    if let Some(installation) = installation {
        if let Err(error) = db::upsert_skill_installation(pool, &installation).await {
            let rollback = remove_path_without_following_links(&target);
            return match rollback {
                Ok(()) => Err(error),
                Err(rollback_error) => Err(format!(
                    "설치 기록을 복원할 수 없습니다: {error}\n복원한 파일 정리도 실패했습니다: {rollback_error}"
                )),
            };
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn restore_recovery_entry(state: State<'_, AppState>, id: String) -> Result<(), String> {
    restore_recovery_entry_impl(&state.db, &id).await
}

pub async fn delete_recovery_entry_impl(pool: &DbPool, id: &str) -> Result<(), String> {
    let _guard = recovery_lock().await;
    let layout = recovery_layout(pool)
        .await?
        .ok_or_else(|| "메모리 데이터베이스에는 복구 항목이 없습니다".to_string())?;
    let manifest = read_manifest(&layout, &id)?;
    delete_manifest_entry(&layout, &manifest)
}

#[tauri::command]
pub async fn delete_recovery_entry(state: State<'_, AppState>, id: String) -> Result<(), String> {
    delete_recovery_entry_impl(&state.db, &id).await
}

pub async fn create_database_backup_impl(pool: &DbPool) -> Result<RecoveryEntry, String> {
    snapshot_database(pool, "데이터베이스 수동 백업")
        .await?
        .ok_or_else(|| "메모리 데이터베이스는 파일 백업을 만들 수 없습니다".to_string())
}

#[tauri::command]
pub async fn create_database_backup(state: State<'_, AppState>) -> Result<RecoveryEntry, String> {
    create_database_backup_impl(&state.db).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;
    use tempfile::TempDir;

    async fn setup_file_database(temp: &TempDir) -> DbPool {
        let path = temp.path().join("db.sqlite");
        let pool = db::create_pool(path.to_str().unwrap()).await.unwrap();
        db::init_database(&pool).await.unwrap();
        pool
    }

    fn recovery_root(temp: &TempDir) -> PathBuf {
        temp.path().join("recovery")
    }

    #[tokio::test]
    async fn database_snapshot_contains_committed_data() {
        let temp = TempDir::new().unwrap();
        let pool = setup_file_database(&temp).await;
        db::set_setting(&pool, "recovery_test", "saved")
            .await
            .unwrap();

        let entry = snapshot_database(&pool, "테스트 DB 백업")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(entry.kind, DATABASE_BACKUP);
        assert!(entry.expires_at.is_none());

        let backup = SqlitePool::connect(&format!("sqlite://{}?mode=ro", entry.backup_path))
            .await
            .unwrap();
        assert_eq!(
            db::get_setting(&backup, "recovery_test").await.unwrap(),
            Some("saved".to_string())
        );
        assert!(recovery_root(&temp).join("manifests").exists());
    }

    #[tokio::test]
    async fn copy_backup_restores_content_and_installation_record() {
        let temp = TempDir::new().unwrap();
        let pool = setup_file_database(&temp).await;
        let agent_root = temp.path().join("agent");
        fs::create_dir_all(&agent_root).unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'claude-code'")
            .bind(agent_root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        let original = agent_root.join("saved-skill");
        fs::create_dir_all(&original).unwrap();
        fs::write(original.join("SKILL.md"), "saved").unwrap();
        fs::write(original.join("user-note.txt"), "keep me").unwrap();
        let installation = SkillInstallation {
            skill_id: "saved-skill".to_string(),
            agent_id: "claude-code".to_string(),
            installed_path: original.to_string_lossy().to_string(),
            link_type: "copy".to_string(),
            symlink_target: None,
            created_at: Utc::now().to_rfc3339(),
        };
        db::upsert_skill_installation(&pool, &installation)
            .await
            .unwrap();

        let _guard = recovery_lock().await;
        let entry = backup_copy_installation_locked(&pool, &installation)
            .await
            .unwrap()
            .unwrap();
        remove_path_without_following_links(&original).unwrap();
        db::delete_skill_installation(&pool, "saved-skill", "claude-code")
            .await
            .unwrap();
        drop(_guard);

        restore_recovery_entry_impl(&pool, &entry.id).await.unwrap();

        assert_eq!(
            fs::read_to_string(original.join("user-note.txt")).unwrap(),
            "keep me"
        );
        assert_eq!(
            db::get_skill_installations(&pool, "saved-skill")
                .await
                .unwrap()
                .len(),
            1
        );
    }

    #[tokio::test]
    async fn restore_refuses_existing_target_and_keeps_backup() {
        let temp = TempDir::new().unwrap();
        let pool = setup_file_database(&temp).await;
        let vault = temp.path().join("vault");
        let removed = vault.join("skill");
        fs::create_dir_all(&removed).unwrap();
        fs::write(removed.join("SKILL.md"), "backup").unwrap();
        db::set_setting(
            &pool,
            db::CENTRAL_SKILLS_PATH_SETTING,
            vault.to_string_lossy().as_ref(),
        )
        .await
        .unwrap();

        let entry = backup_vault_before_removal(&pool, &removed, "휴지통 테스트".to_string())
            .await
            .unwrap()
            .unwrap();
        remove_path_without_following_links(&removed).unwrap();
        restore_recovery_entry_impl(&pool, &entry.id).await.unwrap();
        assert_eq!(
            fs::read_to_string(removed.join("SKILL.md")).unwrap(),
            "backup"
        );
        remove_path_without_following_links(&removed).unwrap();
        fs::create_dir_all(&removed).unwrap();

        let layout = recovery_layout(&pool).await.unwrap().unwrap();
        let manifest = read_manifest(&layout, &entry.id).unwrap();
        let target = restore_vault_target(&pool, &manifest).await.unwrap();
        assert!(validate_empty_restore_target(&target).is_err());
        assert!(
            fs::symlink_metadata(entry_data_path(&layout, VAULT_TRASH, &entry.id).unwrap()).is_ok()
        );
    }

    #[test]
    fn restore_never_overwrites_existing_file_or_directory() {
        let temp = TempDir::new().unwrap();
        let source_file = temp.path().join("backup.txt");
        fs::write(&source_file, "backup").unwrap();

        let existing_file = temp.path().join("existing.txt");
        fs::write(&existing_file, "keep").unwrap();
        assert!(restore_entry_without_overwriting(&source_file, &existing_file).is_err());
        assert_eq!(fs::read_to_string(&existing_file).unwrap(), "keep");

        let existing_dir = temp.path().join("existing-dir");
        fs::create_dir(&existing_dir).unwrap();
        assert!(restore_entry_without_overwriting(&source_file, &existing_dir).is_err());
        assert!(existing_dir.is_dir());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn backup_failure_keeps_original_file_tree() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let pool = setup_file_database(&temp).await;
        let source = temp.path().join("source");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "keep").unwrap();
        let layout = recovery_layout(&pool).await.unwrap().unwrap();
        fs::set_permissions(&layout.vault_trash, fs::Permissions::from_mode(0o500)).unwrap();
        let result = create_file_backup(
            &layout,
            VAULT_TRASH,
            "실패 테스트".to_string(),
            &source,
            None,
        );
        fs::set_permissions(&layout.vault_trash, fs::Permissions::from_mode(0o700)).unwrap();

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(source.join("SKILL.md")).unwrap(), "keep");
    }

    #[tokio::test]
    async fn uninstall_copy_keeps_user_files_in_file_backed_recovery() {
        let temp = TempDir::new().unwrap();
        let pool = setup_file_database(&temp).await;
        let agent_root = temp.path().join("agent");
        fs::create_dir_all(&agent_root).unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir = ? WHERE id = 'claude-code'")
            .bind(agent_root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .unwrap();
        let target = agent_root.join("copied");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("SKILL.md"), "managed").unwrap();
        fs::write(target.join("user-note.txt"), "preserve").unwrap();
        db::upsert_skill_installation(
            &pool,
            &SkillInstallation {
                skill_id: "copied".to_string(),
                agent_id: "claude-code".to_string(),
                installed_path: target.to_string_lossy().into_owned(),
                link_type: "copy".to_string(),
                symlink_target: None,
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await
        .unwrap();

        crate::commands::linker::uninstall_skill_from_agent_impl(&pool, "copied", "claude-code")
            .await
            .unwrap();
        assert!(!target.exists());
        let entry = list_recovery_entries_impl(&pool)
            .await
            .unwrap()
            .into_iter()
            .find(|entry| entry.kind == COPY_BACKUP)
            .unwrap();
        assert_eq!(
            fs::read_to_string(Path::new(&entry.backup_path).join("user-note.txt")).unwrap(),
            "preserve"
        );
    }

    #[tokio::test]
    async fn expired_file_entries_are_removed_but_database_backups_remain() {
        let temp = TempDir::new().unwrap();
        let pool = setup_file_database(&temp).await;
        let vault = temp.path().join("vault");
        let source = vault.join("expired");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "expired").unwrap();
        db::set_setting(
            &pool,
            db::CENTRAL_SKILLS_PATH_SETTING,
            vault.to_string_lossy().as_ref(),
        )
        .await
        .unwrap();
        let file_entry = backup_vault_before_removal(&pool, &source, "만료 테스트".to_string())
            .await
            .unwrap()
            .unwrap();
        let database_entry = create_database_backup_impl(&pool).await.unwrap();
        let layout = recovery_layout(&pool).await.unwrap().unwrap();
        let manifest_path = manifest_path(&layout, &file_entry.id).unwrap();
        let mut manifest: RecoveryManifest =
            serde_json::from_slice(&fs::read(&manifest_path).unwrap()).unwrap();
        manifest.entry.expires_at = Some((Utc::now() - Duration::days(1)).to_rfc3339());
        write_manifest(&layout, &manifest).unwrap_err();
        fs::remove_file(&manifest_path).unwrap();
        write_manifest(&layout, &manifest).unwrap();

        let entries = list_recovery_entries_impl(&pool).await.unwrap();
        assert!(entries.iter().all(|entry| entry.id != file_entry.id));
        assert!(entries.iter().any(|entry| entry.id == database_entry.id));
        assert!(fs::symlink_metadata(
            entry_data_path(&layout, VAULT_TRASH, &file_entry.id).unwrap()
        )
        .is_err());
    }

    #[tokio::test]
    async fn tampered_manifest_path_is_not_listed_or_restored() {
        let temp = TempDir::new().unwrap();
        let pool = setup_file_database(&temp).await;
        let vault = temp.path().join("vault");
        let source = vault.join("skill");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "safe").unwrap();
        db::set_setting(
            &pool,
            db::CENTRAL_SKILLS_PATH_SETTING,
            vault.to_string_lossy().as_ref(),
        )
        .await
        .unwrap();
        let entry = backup_vault_before_removal(&pool, &source, "변조 테스트".to_string())
            .await
            .unwrap()
            .unwrap();
        remove_path_without_following_links(&source).unwrap();
        let layout = recovery_layout(&pool).await.unwrap().unwrap();
        let path = manifest_path(&layout, &entry.id).unwrap();
        let mut manifest: RecoveryManifest =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        manifest.entry.original_path = temp.path().join("outside").to_string_lossy().into_owned();
        fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();

        assert!(list_recovery_entries_impl(&pool)
            .await
            .unwrap()
            .iter()
            .all(|listed| listed.id != entry.id));
        assert!(restore_recovery_entry_impl(&pool, &entry.id).await.is_err());
        assert!(!temp.path().join("outside").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn backup_preserves_dangling_symlink_without_following_it() {
        let temp = TempDir::new().unwrap();
        let pool = setup_file_database(&temp).await;
        let vault = temp.path().join("vault");
        let source = vault.join("linked-skill");
        fs::create_dir_all(&vault).unwrap();
        std::os::unix::fs::symlink("missing-target", &source).unwrap();
        db::set_setting(
            &pool,
            db::CENTRAL_SKILLS_PATH_SETTING,
            vault.to_string_lossy().as_ref(),
        )
        .await
        .unwrap();

        let entry = backup_vault_before_removal(&pool, &source, "링크 휴지통".to_string())
            .await
            .unwrap()
            .unwrap();
        let layout = recovery_layout(&pool).await.unwrap().unwrap();
        let backup = entry_data_path(&layout, VAULT_TRASH, &entry.id).unwrap();
        assert!(fs::symlink_metadata(&backup)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read_link(backup).unwrap(),
            PathBuf::from("missing-target")
        );
    }
}

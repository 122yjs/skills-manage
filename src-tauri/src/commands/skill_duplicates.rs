//! 같은 이름은 비교 대상으로만 취급한다. 비교 결과로 파일을 자동 삭제하지 않는다.
use super::scanner::{self, ScanDirectoryOptions};
use crate::{
    db::{self, DbPool},
    AppState,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, io::Read, path::Path};
use tauri::State;

#[derive(Debug, Serialize)]
pub struct DuplicateComparison {
    pub relation: String,
    pub paths: Vec<String>,
}

/// 보조 파일까지 비교하되 큰 패키지와 내부 링크는 임의로 같다고 판정하지 않는다.
fn fingerprint(root: &Path) -> Result<Vec<u8>, String> {
    fn walk(
        root: &Path,
        dir: &Path,
        hash: &mut Sha256,
        budget: &mut u64,
        count: &mut usize,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 32 {
            return Err("비교 깊이 초과".into());
        }
        let mut entries = fs::read_dir(dir)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            *count += 1;
            if *count > 2000 {
                return Err("비교 파일 수 초과".into());
            }
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            let relative = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy();
            hash.update((relative.len() as u64).to_le_bytes());
            hash.update(relative.as_bytes());
            if metadata.is_dir() {
                hash.update(b"dir");
                walk(root, &path, hash, budget, count, depth + 1)?;
            } else if metadata.is_file() {
                let mut bytes = Vec::new();
                fs::File::open(&path)
                    .map_err(|e| e.to_string())?
                    .take(*budget + 1)
                    .read_to_end(&mut bytes)
                    .map_err(|e| e.to_string())?;
                *budget = budget
                    .checked_sub(bytes.len() as u64)
                    .ok_or("비교 크기 초과")?;
                hash.update(b"file");
                hash.update((bytes.len() as u64).to_le_bytes());
                hash.update(bytes);
                // 스크립트의 실행 가능 여부도 스킬 동작에 영향을 준다.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    hash.update((metadata.permissions().mode() & 0o111).to_le_bytes());
                }
            } else {
                return Err("내부 링크 또는 특수 파일은 직접 확인하세요".into());
            }
        }
        Ok(())
    }
    let mut hash = Sha256::new();
    walk(root, root, &mut hash, &mut (32 * 1024 * 1024), &mut 0, 0)?;
    Ok(hash.finalize().to_vec())
}

pub fn compare_paths(paths: Vec<String>) -> DuplicateComparison {
    let resolved = paths
        .iter()
        .map(fs::canonicalize)
        .collect::<Result<BTreeSet<_>, _>>();
    let relation = match resolved {
        Ok(roots) if paths.len() > 1 && roots.len() == 1 => "same_origin",
        Ok(roots) if roots.len() > 1 => {
            match roots
                .iter()
                .map(|p| fingerprint(p))
                .collect::<Result<BTreeSet<_>, _>>()
            {
                Ok(hashes) if hashes.len() == 1 => "identical",
                Ok(_) => "different",
                Err(_) => "unknown",
            }
        }
        _ => "unknown",
    };
    DuplicateComparison {
        relation: relation.into(),
        paths,
    }
}

#[tauri::command]
pub async fn compare_skill_locations(
    state: State<'_, AppState>,
    agent_id: String,
    name: String,
) -> Result<DuplicateComparison, String> {
    let paths = db::get_skills_for_agent(&state.db, &agent_id)
        .await?
        .into_iter()
        .filter(|s| s.name.trim() == name.trim())
        .map(|s| s.dir_path)
        .collect();
    tauri::async_runtime::spawn_blocking(move || compare_paths(paths))
        .await
        .map_err(|e| e.to_string())
}

/// DB가 오래됐어도 실제 로딩 경로의 같은 이름을 발견하면 새 복사본을 만들지 않는다.
pub(crate) async fn check_install(
    pool: &DbPool,
    skill_id: &str,
    agent: &db::Agent,
    target: &Path,
    source: &Path,
) -> Result<(), String> {
    let name = scanner::parse_skill_md(&source.join("SKILL.md")).map(|s| s.name);
    let name = match name {
        Some(name) => name,
        None => db::get_skill_by_id(pool, skill_id)
            .await?
            .map(|s| s.name)
            .unwrap_or_else(|| skill_id.into()),
    };
    let universal = db::get_agent_by_id(pool, "universal").await?;
    let central = db::get_agent_by_id(pool, "central").await?;
    let mut paths = BTreeSet::new();
    for root in scanner::loading_paths_for_agent(
        agent,
        universal.as_ref().map(|a| Path::new(&a.global_skills_dir)),
        central.as_ref().map(|a| Path::new(&a.global_skills_dir)),
    ) {
        for existing in scanner::scan_skill_root(&root, false, ScanDirectoryOptions::nested()) {
            // 같은 위치 재연결은 허용한다. 서로 다른 링크 경로는 중복일 수 있으므로 합치지 않는다.
            if Path::new(&existing.dir_path) != target && existing.name.trim() == name.trim() {
                paths.insert(existing.dir_path);
            }
        }
    }
    if paths.is_empty() {
        return Ok(());
    }
    Err(format!("{}에서 같은 이름의 스킬을 이미 읽을 수 있습니다. 추가 설치 대신 기존 출처를 확인하세요: {}", agent.display_name, paths.into_iter().collect::<Vec<_>>().join(", ")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_support_files_and_preserves_different_copies() {
        let tmp = tempfile::tempdir().unwrap();
        let left = tmp.path().join("left");
        let right = tmp.path().join("right");
        for path in [&left, &right] {
            fs::create_dir_all(path.join("scripts")).unwrap();
            fs::write(path.join("SKILL.md"), "같은 본문").unwrap();
            fs::write(path.join("scripts/run.py"), "print(1)").unwrap();
        }
        let paths = vec![
            left.to_string_lossy().into_owned(),
            right.to_string_lossy().into_owned(),
        ];
        assert_eq!(compare_paths(paths.clone()).relation, "identical");
        fs::write(right.join("scripts/run.py"), "print(2)").unwrap();
        assert_eq!(compare_paths(paths.clone()).relation, "different");
        assert_eq!(
            fs::read_to_string(right.join("scripts/run.py")).unwrap(),
            "print(2)"
        );
        fs::remove_dir_all(&right).unwrap();
        assert_eq!(compare_paths(paths).relation, "unknown");
    }

    #[cfg(unix)]
    #[test]
    fn shared_link_is_one_origin_but_internal_link_is_not_assumed_identical() {
        use std::os::unix::fs::symlink;
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("source");
        let link = tmp.path().join("link");
        fs::create_dir_all(&source).unwrap();
        fs::write(source.join("SKILL.md"), "본문").unwrap();
        symlink(&source, &link).unwrap();
        assert_eq!(
            compare_paths(vec![
                source.to_string_lossy().into(),
                link.to_string_lossy().into()
            ])
            .relation,
            "same_origin"
        );
        let other = tmp.path().join("other");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("SKILL.md"), "본문").unwrap();
        symlink(&source, other.join("loop")).unwrap();
        assert_eq!(
            compare_paths(vec![
                source.to_string_lossy().into(),
                other.to_string_lossy().into()
            ])
            .relation,
            "unknown"
        );
    }

    #[tokio::test]
    async fn install_checks_live_compatibility_paths_even_without_scan_records() {
        use super::super::linker;
        let tmp = tempfile::tempdir().unwrap();
        let pool = db::create_pool(tmp.path().join("db.sqlite").to_str().unwrap())
            .await
            .unwrap();
        db::init_database(&pool).await.unwrap();
        for agent in db::get_all_agents(&pool).await.unwrap() {
            let root = match agent.id.as_str() {
                "universal" => tmp.path().join(".agents/skills"),
                "claude-code" => tmp.path().join(".claude/skills"),
                "codex" => tmp.path().join(".codex/skills"),
                "cursor" => tmp.path().join(".cursor/skills"),
                _ => tmp.path().join(&agent.id),
            };
            fs::create_dir_all(&root).unwrap();
            sqlx::query(
                "UPDATE agents SET global_skills_dir=?, is_builtin=0, is_detected=1 WHERE id=?",
            )
            .bind(root.to_str().unwrap())
            .bind(agent.id)
            .execute(&pool)
            .await
            .unwrap();
        }
        // 두 대상을 동시에 선택해도 설치 순서 때문에 중복을 만들지 않는다.
        for targets in [["cursor", "claude-code"], ["claude-code", "cursor"]] {
            let targets = targets.into_iter().map(str::to_owned).collect::<Vec<_>>();
            assert!(linker::validate_batch_install_targets(&pool, &targets)
                .await
                .unwrap_err()
                .contains("스킬 폴더도 읽습니다"));
        }
        let source = tmp.path().join("central/my-skill");
        let existing = tmp.path().join(".claude/skills/another-folder");
        for dir in [&source, &existing] {
            fs::create_dir_all(dir).unwrap();
            fs::write(dir.join("SKILL.md"), "---\nname: matching-name\n---\n본문").unwrap();
        }
        // 이름이 같고 ID가 달라도 모든 설치 방식에서 중복을 차단한다.
        for method in ["symlink", "copy", "auto"] {
            let result = match method {
                "symlink" => linker::install_skill_to_agent_impl(&pool, "my-skill", "cursor").await,
                "copy" => {
                    linker::install_skill_to_agent_copy_impl(&pool, "my-skill", "cursor").await
                }
                _ => linker::install_skill_to_agent_auto_impl(&pool, "my-skill", "cursor").await,
            };
            let error = result.unwrap_err();
            assert!(error.contains("another-folder"), "{error}");
            assert!(!tmp.path().join(".cursor/skills/my-skill").exists());
        }
        assert!(existing.join("SKILL.md").is_file());
        // 외부에서 옮긴 뒤에는 오래된 DB 관측값에 막히지 않는다.
        fs::rename(&existing, tmp.path().join("backup")).unwrap();
        linker::install_skill_to_agent_impl(&pool, "my-skill", "cursor")
            .await
            .unwrap();
        assert!(tmp
            .path()
            .join(".cursor/skills/my-skill/SKILL.md")
            .is_file());
    }
}

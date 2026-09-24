use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::Serialize;
use tauri::State;

use crate::{
    db::{self, DbPool},
    AppState,
};

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMember {
    skill_id: String,
    source_key: String,
    name: String,
    description: Option<String>,
    file_path: String,
    agent_id: Option<String>,
    row_id: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillGroup {
    id: String,
    name: String,
    kind: String,
    folder_path: Option<String>,
    repository_url: Option<String>,
    members: Vec<GroupMember>,
    source_skill_count: Option<usize>,
}

fn physical(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// 실제 복사 동작에서만 호출한다. 파일명이 같다는 이유로 출처를 추측하지 않는다.
pub async fn inherit_group_source(
    pool: &DbPool,
    source: &Path,
    target: &Path,
) -> Result<(), String> {
    let source_path = physical(source);
    let target_path = physical(target);
    if source_path == target_path {
        return Ok(());
    }
    let saved = sqlx::query_as::<_, (String, String)>(
        "SELECT source_root, source_label FROM skill_group_sources WHERE target_path = ?",
    )
    .bind(&source_path)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    let origin = if saved.is_some() {
        saved
    } else {
        db::get_all_agent_skill_observations(pool)
            .await?
            .into_iter()
            .find(|o| o.source_kind == "plugin" && physical(Path::new(&o.dir_path)) == source_path)
            .and_then(|o| {
                o.source_label
                    .map(|label| (physical(Path::new(&o.source_root)), label))
            })
    };
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    sqlx::query("DELETE FROM skill_group_sources WHERE target_path = ?")
        .bind(&target_path)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    if let Some((root, label)) = origin {
        sqlx::query("INSERT INTO skill_group_sources (target_path, source_root, source_label) VALUES (?, ?, ?)")
            .bind(target_path).bind(root).bind(label).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())
}

fn add_member(
    groups: &mut BTreeMap<String, SkillGroup>,
    mut group: SkillGroup,
    member: &GroupMember,
    source_key: &str,
) {
    let entry = groups.entry(group.id.clone()).or_insert_with(|| {
        group.members = Vec::new();
        group
    });
    // 여러 플랫폼이 같은 파일을 읽어도 모음집에는 한 번만 표시한다.
    if !entry
        .members
        .iter()
        .any(|m| m.file_path == member.file_path)
    {
        let mut member = member.clone();
        member.source_key = source_key.to_string();
        entry.members.push(member);
    }
}

/// 앱이 기록한 배포 표식을 읽는다. 이름 접두어나 설치 경로 개수로 묶지 않는다.
fn is_paseo_managed_skill(dir: &Path) -> bool {
    #[derive(serde::Deserialize)]
    struct ManagedFiles {
        version: u32,
        files: BTreeMap<String, String>,
    }
    let path = dir.join(".paseo-managed-files.json");
    let Ok(metadata) = std::fs::metadata(&path) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() > 256 * 1024 {
        return false;
    }
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let Ok(manifest) = serde_json::from_slice::<ManagedFiles>(&bytes) else {
        return false;
    };
    // 해시는 배포 당시 내용이다. 사용자 편집으로 현재 해시가 달라도 소속은 유지한다.
    manifest.version == 1
        && manifest.files.get("SKILL.md").is_some_and(|hash| {
            hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

pub async fn get_skill_groups_impl(pool: &DbPool) -> Result<Vec<SkillGroup>, String> {
    let skills = sqlx::query_as::<_, db::Skill>("SELECT * FROM skills ORDER BY name, file_path")
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?;
    let observations = db::get_all_agent_skill_observations(pool).await?;
    let mut members = Vec::new();
    for skill in &skills {
        members.push(GroupMember {
            source_key: String::new(),
            skill_id: skill.id.clone(),
            name: skill.name.clone(),
            description: skill.description.clone(),
            file_path: skill.file_path.clone(),
            agent_id: None,
            row_id: None,
        });
    }
    for install in db::get_all_skill_installations(pool).await? {
        if let Some(skill) = skills.iter().find(|s| s.id == install.skill_id) {
            members.push(GroupMember {
                source_key: String::new(),
                skill_id: skill.id.clone(),
                name: skill.name.clone(),
                description: skill.description.clone(),
                file_path: Path::new(&install.installed_path)
                    .join("SKILL.md")
                    .to_string_lossy()
                    .into_owned(),
                agent_id: Some(install.agent_id),
                row_id: None,
            });
        }
    }
    let mut plugin_sources: BTreeMap<String, (String, String)> =
        sqlx::query_as::<_, (String, String, String)>(
            "SELECT target_path, source_root, source_label FROM skill_group_sources",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(path, root, label)| (path, (root, label)))
        .collect();
    // 이전 버전이 만든 모음 폴더는 복사 경로와 전체 파일이 모두 맞는 경우만 복구한다.
    let central_root = db::get_central_skills_dir(pool).await?;
    for skill in skills.iter().filter(|skill| skill.is_central) {
        let Some(target) = Path::new(&skill.file_path).parent() else {
            continue;
        };
        let target_path = physical(target);
        if plugin_sources.contains_key(&target_path) {
            continue;
        }
        let mut matches = BTreeMap::new();
        for observation in observations
            .iter()
            .filter(|o| o.source_kind == "plugin" && o.skill_id == skill.id)
        {
            let Some(label) = observation.source_label.as_deref() else {
                continue;
            };
            let Ok(bundle) = super::linker::plugin_bundle_directory_name(label) else {
                continue;
            };
            let expected = central_root.join(bundle).join(&skill.id);
            if physical(&expected) != target_path {
                continue;
            }
            let comparison = super::skill_duplicates::compare_paths(vec![
                target_path.clone(),
                observation.dir_path.clone(),
            ]);
            if matches!(comparison.relation.as_str(), "identical" | "same_origin") {
                matches.insert(physical(Path::new(&observation.source_root)), observation);
            }
        }
        if matches.len() == 1 {
            let (root, observation) = matches.into_iter().next().unwrap();
            let label = observation.source_label.clone().unwrap();
            sqlx::query("INSERT OR IGNORE INTO skill_group_sources (target_path, source_root, source_label) VALUES (?, ?, ?)")
                .bind(&target_path).bind(&root).bind(&label).execute(pool).await.map_err(|e| e.to_string())?;
            plugin_sources.insert(target_path, (root, label));
        }
    }
    for observation in observations {
        if observation.source_kind == "plugin" {
            if let Some(label) = &observation.source_label {
                plugin_sources.insert(
                    physical(Path::new(&observation.dir_path)),
                    (physical(Path::new(&observation.source_root)), label.clone()),
                );
            }
        }
        members.push(GroupMember {
            source_key: String::new(),
            skill_id: observation.skill_id,
            name: observation.name,
            description: observation.description,
            file_path: observation.file_path,
            agent_id: Some(observation.agent_id),
            row_id: Some(observation.row_id),
        });
    }
    let repos: BTreeMap<String, (String, String, String)> =
        sqlx::query_as::<_, (String, String, String, String)>(
            "SELECT target_path, owner, repo, source_path FROM skill_origins",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(path, owner, repo, source)| (physical(Path::new(&path)), (owner, repo, source)))
        .collect();
    let mut groups = BTreeMap::new();
    let mut app_sources = BTreeMap::new();
    for member in members {
        let file = Path::new(&member.file_path);
        if !file.is_file() {
            continue;
        }
        let Some(dir) = file.parent() else {
            continue;
        };
        let key = physical(dir);
        if *app_sources
            .entry(key.clone())
            .or_insert_with(|| is_paseo_managed_skill(dir))
        {
            add_member(
                &mut groups,
                SkillGroup {
                    id: "bundle:paseo".into(),
                    name: "Paseo".into(),
                    kind: "bundle".into(),
                    folder_path: None,
                    repository_url: None,
                    members: Vec::new(),
                    source_skill_count: None,
                },
                &member,
                &member.skill_id,
            );
        }
        if let Some((root, label)) = plugin_sources.get(&key) {
            add_member(
                &mut groups,
                SkillGroup {
                    id: format!("plugin:{root}"),
                    name: label.clone(),
                    kind: "plugin".into(),
                    folder_path: Path::new(root).is_dir().then(|| root.clone()),
                    repository_url: None,
                    members: Vec::new(),
                    source_skill_count: None,
                },
                &member,
                &member.skill_id,
            );
        }
        if let Some((owner, repo, source_path)) = repos.get(&key) {
            add_member(
                &mut groups,
                SkillGroup {
                    id: format!(
                        "repository:{}/{}",
                        owner.to_lowercase(),
                        repo.to_lowercase()
                    ),
                    name: format!("{owner}/{repo}"),
                    kind: "repository".into(),
                    folder_path: None,
                    repository_url: Some(format!("https://github.com/{owner}/{repo}")),
                    members: Vec::new(),
                    source_skill_count: None,
                },
                &member,
                source_path,
            );
        }
    }
    let published: BTreeMap<String, i64> = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT owner, repo, MAX(skill_count) FROM skill_repository_catalog GROUP BY owner, repo",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?
    .into_iter()
    .map(|(owner, repo, count)| (format!("repository:{owner}/{repo}"), count))
    .collect();
    // 복사본의 수가 아니라 원본의 서로 다른 스킬 경로로 스킬셋 여부를 판단한다.
    // 원본에 여러 스킬이 확인됐다면 하나만 설치되어 있어도 스킬셋이다.
    let mut groups: Vec<_> = groups
        .into_values()
        .filter(|group| {
            matches!(group.kind.as_str(), "plugin" | "bundle")
                || published.get(&group.id).map_or_else(
                    || {
                        group
                            .members
                            .iter()
                            .map(|m| &m.source_key)
                            .collect::<BTreeSet<_>>()
                            .len()
                            > 1
                    },
                    |count| *count > 1,
                )
        })
        .collect();
    for group in &mut groups {
        group.source_skill_count = Some(
            published
                .get(&group.id)
                .map(|count| *count as usize)
                .unwrap_or_else(|| {
                    group
                        .members
                        .iter()
                        .map(|m| &m.source_key)
                        .collect::<BTreeSet<_>>()
                        .len()
                }),
        );
        group
            .members
            .sort_by(|a, b| a.name.cmp(&b.name).then(a.file_path.cmp(&b.file_path)));
    }
    groups.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    Ok(groups)
}

#[tauri::command]
pub async fn get_skill_groups(state: State<'_, AppState>) -> Result<Vec<SkillGroup>, String> {
    get_skill_groups_impl(&state.db).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn setup() -> DbPool {
        let pool = sqlx::SqlitePool::connect(":memory:").await.unwrap();
        db::init_database(&pool).await.unwrap();
        pool
    }

    async fn skill(pool: &DbPool, dir: &Path, id: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "---\nname: same\n---\nBody").unwrap();
        db::upsert_skill(
            pool,
            &db::Skill {
                id: id.into(),
                name: "same".into(),
                description: None,
                file_path: dir.join("SKILL.md").to_string_lossy().into_owned(),
                canonical_path: None,
                is_central: true,
                source: Some("native".into()),
                content: None,
                scanned_at: "now".into(),
            },
        )
        .await
        .unwrap();
    }

    async fn plugin(pool: &DbPool, root: &Path, dir: &Path, row: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("SKILL.md"), "---\nname: same\n---\nBody").unwrap();
        db::upsert_agent_skill_observation(
            pool,
            &db::AgentSkillObservation {
                row_id: row.into(),
                agent_id: "codex".into(),
                skill_id: "same".into(),
                name: "same".into(),
                description: None,
                file_path: dir.join("SKILL.md").to_string_lossy().into_owned(),
                dir_path: dir.to_string_lossy().into_owned(),
                source_kind: "plugin".into(),
                source_root: root.to_string_lossy().into_owned(),
                source_label: Some("same-plugin".into()),
                link_type: "native".into(),
                symlink_target: None,
                is_read_only: true,
                scanned_at: "now".into(),
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn copied_plugin_keeps_source_after_scan_and_original_removal() {
        let tmp = TempDir::new().unwrap();
        let pool = setup().await;
        let root = tmp.path().join("plugin");
        let source = root.join("skills/same");
        let target = tmp.path().join("vault/renamed");
        plugin(&pool, &root, &source, "p1").await;
        skill(&pool, &target, "renamed").await;
        inherit_group_source(&pool, &source, &target).await.unwrap();
        // 재스캔은 skills.source를 native로 바꿔도 모음 출처를 보존해야 한다.
        skill(&pool, &target, "renamed").await;
        sqlx::query("DELETE FROM agent_skill_observations")
            .execute(&pool)
            .await
            .unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        let groups = get_skill_groups_impl(&pool).await.unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "same-plugin");
        assert!(groups[0].folder_path.is_none());
        assert_eq!(groups[0].members[0].skill_id, "renamed");
        // 다른 출처로 같은 목적지를 다시 복사하면 예전 출처를 지운다.
        let unrelated = tmp.path().join("unrelated");
        skill(&pool, &unrelated, "other").await;
        inherit_group_source(&pool, &unrelated, &target)
            .await
            .unwrap();
        assert!(get_skill_groups_impl(&pool).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn same_names_from_distinct_plugin_roots_stay_separate() {
        let tmp = TempDir::new().unwrap();
        let pool = setup().await;
        for name in ["a", "b"] {
            let root = tmp.path().join(name);
            plugin(&pool, &root, &root.join("skills/same"), name).await;
        }
        let groups = get_skill_groups_impl(&pool).await.unwrap();
        assert_eq!(groups.len(), 2);
        assert_ne!(groups[0].id, groups[1].id);
        assert_eq!(groups[0].members.len(), 1);
        assert_eq!(groups[1].members.len(), 1);
    }

    #[tokio::test]
    async fn repository_membership_uses_actual_paths_not_names() {
        let tmp = TempDir::new().unwrap();
        let pool = setup().await;
        for (id, owner) in [("one", "mattpocock"), ("two", "someone")] {
            let dir = tmp.path().join(id);
            skill(&pool, &dir, id).await;
            sqlx::query("INSERT INTO skill_origins (binding_id,target_key,skill_id,target_path,provider,owner,repo,source_path,ref_name,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?,?)")
                .bind(id).bind(id).bind(id).bind(physical(&dir)).bind("github").bind(owner).bind("skills").bind("skills/same").bind("main").bind("now").bind("now")
                .execute(&pool).await.unwrap();
        }
        for owner in ["mattpocock", "someone"] {
            sqlx::query("INSERT INTO skill_repository_catalog VALUES (?, 'skills', 'main', 2)")
                .bind(owner)
                .execute(&pool)
                .await
                .unwrap();
        }
        skill(&pool, &tmp.path().join("unknown"), "unknown").await;
        let groups = get_skill_groups_impl(&pool).await.unwrap();
        assert_eq!(groups.len(), 2);
        assert!(groups
            .iter()
            .all(|g| g.members.len() == 1 && g.folder_path.is_none()));
        assert!(groups
            .iter()
            .any(|g| g.id == "repository:mattpocock/skills" && g.members[0].skill_id == "one"));
    }
    #[tokio::test]
    async fn legacy_plugin_copy_requires_bundle_path_and_all_files_to_match() {
        let tmp = TempDir::new().unwrap();
        let pool = setup().await;
        let vault = tmp.path().join("vault");
        db::set_setting(
            &pool,
            db::CENTRAL_SKILLS_PATH_SETTING,
            &vault.to_string_lossy(),
        )
        .await
        .unwrap();
        let root = tmp.path().join("plugin");
        let source = root.join("skills/same");
        plugin(&pool, &root, &source, "p").await;
        let target = vault.join("same-plugin/same");
        skill(&pool, &target, "same").await;
        std::fs::write(target.join("helper.txt"), "local change").unwrap();
        let groups = get_skill_groups_impl(&pool).await.unwrap();
        assert_eq!(groups[0].members.len(), 1);
        std::fs::remove_file(target.join("helper.txt")).unwrap();
        let groups = get_skill_groups_impl(&pool).await.unwrap();
        assert_eq!(groups[0].members.len(), 2);
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM skill_group_sources")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);
    }
    #[tokio::test]
    async fn repeated_installations_do_not_make_a_skill_set() {
        let tmp = TempDir::new().unwrap();
        let pool = setup().await;
        for id in ["platform-one", "platform-two"] {
            let dir = tmp.path().join(id);
            skill(&pool, &dir, id).await;
            sqlx::query("INSERT INTO skill_origins (binding_id,target_key,skill_id,target_path,provider,owner,repo,source_path,ref_name,created_at,updated_at) VALUES (?,?,?,?, 'github','owner','single','skills/demo','main','now','now')")
                .bind(id).bind(id).bind(id).bind(physical(&dir)).execute(&pool).await.unwrap();
        }
        assert!(get_skill_groups_impl(&pool).await.unwrap().is_empty());
        // 같은 저장소 안의 다른 스킬 경로가 확인되어야 스킬셋으로 분류한다.
        sqlx::query(
            "UPDATE skill_origins SET source_path='skills/other' WHERE binding_id='platform-two'",
        )
        .execute(&pool)
        .await
        .unwrap();
        let groups = get_skill_groups_impl(&pool).await.unwrap();
        assert_eq!(groups.len(), 1);
        assert_ne!(
            groups[0].members[0].source_key,
            groups[0].members[1].source_key
        );
        // 원본의 최신 구성이 단일 스킬이면 과거 설치 경로가 둘이어도 스킬셋이 아니다.
        sqlx::query("INSERT INTO skill_repository_catalog VALUES ('owner', 'single', 'main', 1)")
            .execute(&pool)
            .await
            .unwrap();
        assert!(get_skill_groups_impl(&pool).await.unwrap().is_empty());
    }
    #[tokio::test]
    async fn app_bundle_records_form_a_set_without_github_or_plugin_metadata() {
        let tmp = TempDir::new().unwrap();
        let pool = setup().await;
        for id in ["paseo", "paseo-handoff", "paseo-committee", "paseo-custom"] {
            let dir = tmp.path().join(id);
            skill(&pool, &dir, id).await;
            if id != "paseo-custom" {
                std::fs::write(
                    dir.join(".paseo-managed-files.json"),
                    serde_json::json!({
                        "version": 1, "files": {"SKILL.md": "a".repeat(64)}
                    })
                    .to_string(),
                )
                .unwrap();
            }
        }
        let groups = get_skill_groups_impl(&pool).await.unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].id, "bundle:paseo");
        assert_eq!(groups[0].source_skill_count, Some(3));
        assert_eq!(groups[0].members.len(), 3);
        assert!(!groups[0]
            .members
            .iter()
            .any(|m| m.skill_id == "paseo-custom"));
        // 표식 이름만 같거나 다른 버전의 형식이면 출처라고 단정하지 않는다.
        let unknown = tmp.path().join("paseo-custom");
        for contents in [
            "{}",
            r#"{"version":1,"files":{"SKILL.md":"invalid"}}"#,
            r#"{"version":2,"files":{}}"#,
        ] {
            std::fs::write(unknown.join(".paseo-managed-files.json"), contents).unwrap();
            assert!(!is_paseo_managed_skill(&unknown));
        }
    }
}

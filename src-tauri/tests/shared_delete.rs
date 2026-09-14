#![cfg(unix)]
use skills_manage_lib::{
    commands::{shared_delete, usage},
    db,
};
use std::{fs, path::Path};

async fn fixture() -> (tempfile::TempDir, db::DbPool) {
    let tmp = tempfile::tempdir().unwrap();
    let pool = db::create_pool(tmp.path().join("db.sqlite").to_str().unwrap())
        .await
        .unwrap();
    db::init_database(&pool).await.unwrap();
    // 사용자 폴더를 읽지 않도록 모든 플랫폼 경로를 임시 폴더로 한정한다.
    for agent in db::get_all_agents(&pool).await.unwrap() {
        let root = tmp.path().join(&agent.id);
        fs::create_dir_all(&root).unwrap();
        sqlx::query("UPDATE agents SET global_skills_dir=? WHERE id=?")
            .bind(root.to_str().unwrap())
            .bind(agent.id)
            .execute(&pool)
            .await
            .unwrap();
    }
    let source = tmp.path().join("universal/bailian-gen");
    fs::create_dir_all(&source).unwrap();
    fs::write(
        source.join("SKILL.md"),
        "---\nname: bailian-gen\n---\n사용자 수정",
    )
    .unwrap();
    record(&pool, "universal", &source, "copy", None).await;
    let link = tmp.path().join("claude-code/bailian-gen");
    std::os::unix::fs::symlink("../universal/bailian-gen", &link).unwrap();
    record(
        &pool,
        "claude-code",
        &link,
        "symlink",
        Some("../universal/bailian-gen".into()),
    )
    .await;
    (tmp, pool)
}
async fn record(pool: &db::DbPool, agent: &str, path: &Path, kind: &str, target: Option<String>) {
    db::upsert_skill_installation(
        pool,
        &db::SkillInstallation {
            skill_id: "bailian-gen".into(),
            agent_id: agent.into(),
            installed_path: path.to_string_lossy().into(),
            link_type: kind.into(),
            symlink_target: target,
            created_at: "2026-09-14T00:00:00Z".into(),
        },
    )
    .await
    .unwrap();
}
async fn pause(pool: &db::DbPool, path: &Path) {
    let impact = usage::compute_shared_impact(pool, path.to_str().unwrap())
        .await
        .unwrap();
    usage::set_shared_skill_usage_impl(
        pool,
        path.to_str().unwrap(),
        false,
        &impact.confirmation_token,
    )
    .await
    .unwrap();
    // 실제 재스캔처럼 끊어진 링크의 설치 기록을 제거해도 파일은 찾아야 한다.
    db::delete_skill_installation(pool, "bailian-gen", "claude-code")
        .await
        .unwrap();
}
#[tokio::test]
async fn active_and_paused_delete_links_and_keep_independent_copies() {
    for inactive in [false, true] {
        let (tmp, pool) = fixture().await;
        let source = tmp.path().join("universal/bailian-gen");
        let independent = tmp.path().join("cursor/bailian-gen");
        fs::create_dir_all(&independent).unwrap();
        fs::write(independent.join("SKILL.md"), "독립 복사본").unwrap();
        if inactive {
            pause(&pool, &source).await;
        }
        let plan = shared_delete::preview(&pool, "bailian-gen").await.unwrap();
        assert_eq!(plan.links.len(), 1);
        assert_eq!(plan.links[0].agent_id, "claude-code");
        let result = shared_delete::delete(
            &pool,
            vec![shared_delete::SharedDeleteConfirmation {
                skill_id: plan.skill_id,
                confirmation_token: plan.confirmation_token,
            }],
        )
        .await
        .unwrap();
        assert!(result.failed.is_empty(), "{:?}", result.failed);
        assert_eq!(result.deleted, vec!["bailian-gen"]);
        assert!(fs::symlink_metadata(source).is_err());
        assert!(fs::symlink_metadata(tmp.path().join("claude-code/bailian-gen")).is_err());
        assert!(independent.join("SKILL.md").is_file());
        let backups = skills_manage_lib::commands::recovery::list_recovery_entries_impl(&pool)
            .await
            .unwrap();
        assert_eq!(backups.len(), 1);
        assert!(Path::new(&backups[0].backup_path)
            .join("SKILL.md")
            .is_file());
    }
}
#[tokio::test]
async fn legacy_delete_cannot_bypass_confirmation_by_pausing() {
    let (tmp, pool) = fixture().await;
    assert!(
        skills_manage_lib::commands::linker::uninstall_skill_from_agent_impl(
            &pool,
            "bailian-gen",
            "universal"
        )
        .await
        .is_err()
    );
    assert!(tmp.path().join("universal/bailian-gen/SKILL.md").is_file());
    pause(&pool, &tmp.path().join("universal/bailian-gen")).await;
    assert!(
        usage::delete_skill_from_agent_impl(&pool, "bailian-gen", "universal")
            .await
            .is_err()
    );
    assert!(
        db::get_paused_installation(&pool, "bailian-gen", "universal")
            .await
            .unwrap()
            .is_some()
    );
}
#[tokio::test]
async fn changed_link_requires_new_confirmation() {
    let (tmp, pool) = fixture().await;
    let plan = shared_delete::preview(&pool, "bailian-gen").await.unwrap();
    let link = tmp.path().join("claude-code/bailian-gen");
    fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink("../central/other", &link).unwrap();
    let result = shared_delete::delete(
        &pool,
        vec![shared_delete::SharedDeleteConfirmation {
            skill_id: plan.skill_id,
            confirmation_token: plan.confirmation_token,
        }],
    )
    .await
    .unwrap();
    assert_eq!(result.failed.len(), 1);
    assert!(tmp.path().join("universal/bailian-gen/SKILL.md").is_file());
    assert_eq!(fs::read_link(link).unwrap(), Path::new("../central/other"));
}
#[tokio::test]
async fn backup_failure_restores_removed_links_and_records() {
    let (tmp, pool) = fixture().await;
    fs::write(tmp.path().join("recovery"), "폴더 생성 차단").unwrap();
    let plan = shared_delete::preview(&pool, "bailian-gen").await.unwrap();
    let result = shared_delete::delete(
        &pool,
        vec![shared_delete::SharedDeleteConfirmation {
            skill_id: plan.skill_id,
            confirmation_token: plan.confirmation_token,
        }],
    )
    .await
    .unwrap();
    assert_eq!(result.failed.len(), 1);
    assert!(tmp
        .path()
        .join("claude-code/bailian-gen/SKILL.md")
        .is_file());
    assert!(
        db::get_skill_installation(&pool, "bailian-gen", "claude-code")
            .await
            .unwrap()
            .is_some()
    );
}

#[tokio::test]
async fn independent_vault_link_is_kept_when_shared_install_is_a_link() {
    let (tmp, pool) = fixture().await;
    let source = tmp.path().join("universal/bailian-gen");
    let vault = tmp.path().join("central/bailian-gen");
    fs::rename(&source, &vault).unwrap();
    std::os::unix::fs::symlink("../central/bailian-gen", &source).unwrap();
    record(
        &pool,
        "universal",
        &source,
        "symlink",
        Some("../central/bailian-gen".into()),
    )
    .await;
    let independent = tmp.path().join("cursor/bailian-gen");
    std::os::unix::fs::symlink("../central/bailian-gen", &independent).unwrap();
    record(
        &pool,
        "cursor",
        &independent,
        "symlink",
        Some("../central/bailian-gen".into()),
    )
    .await;
    let plan = shared_delete::preview(&pool, "bailian-gen").await.unwrap();
    assert_eq!(plan.links.len(), 1);
    let result = shared_delete::delete(
        &pool,
        vec![shared_delete::SharedDeleteConfirmation {
            skill_id: plan.skill_id,
            confirmation_token: plan.confirmation_token,
        }],
    )
    .await
    .unwrap();
    assert!(result.failed.is_empty(), "{:?}", result.failed);
    assert!(vault.join("SKILL.md").is_file());
    assert!(independent.join("SKILL.md").is_file());
}

#[tokio::test]
async fn paused_relative_shortcut_is_included_and_removed() {
    let (tmp, pool) = fixture().await;
    usage::set_skill_usage_impl(&pool, "bailian-gen", "claude-code", false)
        .await
        .unwrap();
    let paused = db::get_paused_installation(&pool, "bailian-gen", "claude-code")
        .await
        .unwrap()
        .unwrap();
    let plan = shared_delete::preview(&pool, "bailian-gen").await.unwrap();
    assert_eq!(plan.links.len(), 1);
    assert_eq!(plan.links[0].path, paused.paused_path);
    let result = shared_delete::delete(
        &pool,
        vec![shared_delete::SharedDeleteConfirmation {
            skill_id: plan.skill_id,
            confirmation_token: plan.confirmation_token,
        }],
    )
    .await
    .unwrap();
    assert!(result.failed.is_empty(), "{:?}", result.failed);
    assert!(fs::symlink_metadata(paused.paused_path).is_err());
    assert!(
        db::get_paused_installation(&pool, "bailian-gen", "claude-code")
            .await
            .unwrap()
            .is_none()
    );
    assert!(!tmp.path().join("universal/bailian-gen").exists());
}

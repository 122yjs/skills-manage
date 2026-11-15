//! 관찰 항목(사용자가 플랫폼 폴더에 직접 둔 스킬 폴더)은 앱이 지우거나 옮기지 않는다 (ADR-0002).
use skills_manage_lib::{
    commands::{
        linker, scanner, skill_duplicates,
        skills::{self, DeleteCentralSkillBundleOptions, DeleteCentralSkillOptions},
        usage,
    },
    db,
};
use std::{fs, path::Path};

fn write_skill(dir: &Path, name: &str, body: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("SKILL.md"),
        format!("---\nname: {name}\n---\n{body}"),
    )
    .unwrap();
}

/// 모든 플랫폼 경로를 임시 폴더로 한정하고, 사용자 정의 플랫폼 `mine`을 추가한다.
async fn fixture() -> (tempfile::TempDir, db::DbPool) {
    let tmp = tempfile::tempdir().unwrap();
    let pool = db::create_pool(tmp.path().join("db.sqlite").to_str().unwrap())
        .await
        .unwrap();
    db::init_database(&pool).await.unwrap();
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
    db::insert_custom_agent(
        &pool,
        &db::Agent {
            id: "mine".into(),
            display_name: "Mine".into(),
            category: "coding".into(),
            global_skills_dir: tmp.path().join("mine").to_string_lossy().into_owned(),
            project_skills_dir: None,
            icon_name: None,
            is_detected: false,
            is_builtin: false,
            is_enabled: true,
        },
    )
    .await
    .unwrap();
    fs::create_dir_all(tmp.path().join("mine")).unwrap();
    (tmp, pool)
}

fn assert_user_folder_untouched(dir: &Path) {
    assert!(dir.join("SKILL.md").is_file(), "{} 사라짐", dir.display());
    assert_eq!(
        fs::read_to_string(dir.join("notes.txt")).unwrap(),
        "내 메모"
    );
}

#[tokio::test]
async fn scan_records_plain_folders_as_observed_and_keeps_app_copies_managed() {
    let (tmp, pool) = fixture().await;
    write_skill(&tmp.path().join("central/linked"), "linked", "원본");
    write_skill(&tmp.path().join("mine/user-made"), "user-made", "내 스킬");
    scanner::scan_all_skills_impl(&pool).await.unwrap();
    linker::install_skill_to_agent_copy_impl(&pool, "linked", "mine")
        .await
        .unwrap();

    // 다시 스캔해도 앱이 만든 복사 설치는 관리 설치로 남는다.
    scanner::scan_all_skills_impl(&pool).await.unwrap();

    assert_eq!(
        db::get_skill_installations(&pool, "linked")
            .await
            .unwrap()
            .iter()
            .filter(|i| i.agent_id == "mine")
            .count(),
        1
    );
    assert!(db::get_skill_installations(&pool, "user-made")
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        db::get_unmanaged_installations(&pool, "user-made")
            .await
            .unwrap()
            .len(),
        1
    );
    let listed = skills::get_skills_by_agent_impl(&pool, "mine")
        .await
        .unwrap();
    let observed = listed.iter().find(|s| s.id == "user-made").unwrap();
    assert!(!observed.is_managed && observed.is_read_only);
    assert!(listed.iter().find(|s| s.id == "linked").unwrap().is_managed);
}

#[tokio::test]
async fn observed_folder_survives_remove_pause_and_platform_delete() {
    let (tmp, pool) = fixture().await;
    let folder = tmp.path().join("mine/only-mine");
    write_skill(&folder, "only-mine", "내 스킬");
    fs::write(folder.join("notes.txt"), "내 메모").unwrap();
    scanner::scan_all_skills_impl(&pool).await.unwrap();

    assert!(
        linker::uninstall_skill_from_agent_impl(&pool, "only-mine", "mine")
            .await
            .is_err()
    );
    assert!(
        usage::set_skill_usage_impl(&pool, "only-mine", "mine", false)
            .await
            .is_err()
    );
    usage::set_platform_usage_impl(&pool, "mine", false)
        .await
        .unwrap();
    let deleted = usage::delete_platform_installations_impl(&pool, "mine")
        .await
        .unwrap();

    assert!(deleted.deleted.is_empty() && deleted.failed.is_empty());
    assert_user_folder_untouched(&folder);
}

#[tokio::test]
async fn observed_folder_survives_central_cascade_delete() {
    let (tmp, pool) = fixture().await;
    write_skill(
        &tmp.path().join("central/my-skill"),
        "my-skill",
        "보관함 원본",
    );
    let folder = tmp.path().join("mine/my-skill");
    write_skill(&folder, "my-skill", "내가 고친 내용");
    fs::write(folder.join("notes.txt"), "내 메모").unwrap();
    scanner::scan_all_skills_impl(&pool).await.unwrap();

    let result = skills::delete_central_skill_impl(
        &pool,
        "my-skill",
        DeleteCentralSkillOptions {
            cascade_uninstall: true,
        },
    )
    .await
    .unwrap();

    assert!(result.uninstalled_agents.is_empty());
    assert!(!tmp.path().join("central/my-skill").exists());
    assert_user_folder_untouched(&folder);
}

#[tokio::test]
async fn observed_folder_survives_bundle_cascade_delete_and_preview_lists_it() {
    let (tmp, pool) = fixture().await;
    write_skill(
        &tmp.path().join("central/pack/my-skill"),
        "my-skill",
        "원본",
    );
    let folder = tmp.path().join("mine/my-skill");
    write_skill(&folder, "my-skill", "내가 고친 내용");
    fs::write(folder.join("notes.txt"), "내 메모").unwrap();
    scanner::scan_all_skills_impl(&pool).await.unwrap();

    let preview = skills::preview_delete_central_skill_bundle_impl(&pool, "pack")
        .await
        .unwrap();
    assert!(!preview.affected_agents.contains(&"mine".to_string()));
    assert_eq!(preview.skills[0].observed_agents, vec!["mine"]);

    skills::delete_central_skill_bundle_impl(
        &pool,
        "pack",
        DeleteCentralSkillBundleOptions {
            cascade_uninstall: true,
        },
    )
    .await
    .unwrap();

    assert!(!tmp.path().join("central/pack").exists());
    assert_user_folder_untouched(&folder);
}

#[tokio::test]
async fn first_run_keeps_only_identical_copies_managed() {
    let (tmp, pool) = fixture().await;
    for name in ["same", "edited"] {
        write_skill(&tmp.path().join("central").join(name), name, "원본");
    }
    write_skill(&tmp.path().join("mine/same"), "same", "원본");
    write_skill(&tmp.path().join("mine/edited"), "edited", "내가 고침");
    write_skill(&tmp.path().join("mine/orphan"), "orphan", "보관함에 없음");
    scanner::scan_all_skills_impl(&pool).await.unwrap();
    // 업데이트 전 버전은 플랫폼 폴더의 모든 일반 폴더를 복사 설치로 기록했다.
    for id in ["same", "edited", "orphan"] {
        db::upsert_skill_installation(
            &pool,
            &db::SkillInstallation {
                skill_id: id.into(),
                agent_id: "mine".into(),
                installed_path: tmp.path().join("mine").join(id).to_string_lossy().into(),
                link_type: "copy".into(),
                symlink_target: None,
                created_at: "2026-09-14T00:00:00Z".into(),
            },
        )
        .await
        .unwrap();
    }

    skill_duplicates::reclassify_copy_installations(&pool)
        .await
        .unwrap();

    assert_eq!(
        db::get_skill_installations_by_agent(&pool, "mine")
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(db::get_skill_installation(&pool, "same", "mine")
        .await
        .unwrap()
        .is_some());
    for id in ["edited", "orphan"] {
        assert_eq!(
            db::get_unmanaged_installations(&pool, id)
                .await
                .unwrap()
                .len(),
            1,
            "{id}"
        );
    }

    // 처음 한 번만 비교한다. 이후에 고친 관리 설치를 다시 낮추지 않는다.
    write_skill(&tmp.path().join("mine/same"), "same", "나중에 고침");
    skill_duplicates::reclassify_copy_installations(&pool)
        .await
        .unwrap();
    assert!(db::get_skill_installation(&pool, "same", "mine")
        .await
        .unwrap()
        .is_some());
}

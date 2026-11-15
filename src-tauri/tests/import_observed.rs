//! 관찰 항목 가져오기: 내용을 보관함 원본으로 복사하고 그 폴더를 관리 설치로 기록한다 (ADR-0002).
use skills_manage_lib::{
    commands::{
        linker,
        observed_import::import_observed_skill_impl,
        scanner,
        skills::{self},
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

/// 모든 플랫폼 경로를 임시 폴더로 한정한다.
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
    (tmp, pool)
}

#[tokio::test]
async fn import_copies_to_central_keeps_platform_folder_and_becomes_managed() {
    let (tmp, pool) = fixture().await;
    let folder = tmp.path().join("claude-code/user-made");
    write_skill(&folder, "user-made", "내 스킬");
    fs::write(folder.join("notes.txt"), "내 메모").unwrap();
    scanner::scan_all_skills_impl(&pool).await.unwrap();
    assert_eq!(
        db::get_unmanaged_installations(&pool, "user-made")
            .await
            .unwrap()
            .len(),
        1
    );

    let conflict = import_observed_skill_impl(&pool, "user-made", "claude-code")
        .await
        .unwrap();

    assert!(conflict.is_none());
    // 보관함 원본이 생기고, 플랫폼 폴더의 파일은 그대로다.
    let central = tmp.path().join("central/user-made");
    assert_eq!(
        fs::read_to_string(central.join("notes.txt")).unwrap(),
        "내 메모"
    );
    assert_eq!(
        fs::read_to_string(central.join("SKILL.md")).unwrap(),
        fs::read_to_string(folder.join("SKILL.md")).unwrap()
    );
    assert_eq!(
        fs::read_to_string(folder.join("notes.txt")).unwrap(),
        "내 메모"
    );
    // 관리 설치로 표시된다.
    assert!(db::get_unmanaged_installations(&pool, "user-made")
        .await
        .unwrap()
        .is_empty());
    let listed = skills::get_skills_by_agent_impl(&pool, "claude-code")
        .await
        .unwrap();
    assert!(
        listed
            .iter()
            .find(|s| s.id == "user-made")
            .unwrap()
            .is_managed
    );
    // 다른 플랫폼에 설치할 수 있고, 가져온 플랫폼에서 제거해도 원본은 보관함에 남는다.
    linker::install_skill_to_agent_auto_impl(&pool, "user-made", "cursor")
        .await
        .unwrap();
    linker::uninstall_skill_from_agent_impl(&pool, "user-made", "claude-code")
        .await
        .unwrap();
    assert!(central.join("SKILL.md").is_file());
}

#[tokio::test]
async fn import_with_same_named_central_skill_stops_and_returns_comparison() {
    let (tmp, pool) = fixture().await;
    write_skill(
        &tmp.path().join("central/my-skill"),
        "my-skill",
        "보관함 원본",
    );
    let folder = tmp.path().join("claude-code/my-skill");
    write_skill(&folder, "my-skill", "내가 고친 내용");
    scanner::scan_all_skills_impl(&pool).await.unwrap();

    let comparison = import_observed_skill_impl(&pool, "my-skill", "claude-code")
        .await
        .unwrap()
        .expect("같은 이름의 보관함 스킬이 있으면 비교를 돌려준다");

    assert_eq!(comparison.relation, "different");
    assert_eq!(comparison.paths.len(), 2);
    assert!(
        fs::read_to_string(tmp.path().join("central/my-skill/SKILL.md"))
            .unwrap()
            .contains("보관함 원본")
    );
    assert_eq!(
        db::get_unmanaged_installations(&pool, "my-skill")
            .await
            .unwrap()
            .len(),
        1
    );
}

#[cfg(unix)]
#[tokio::test]
async fn failed_import_leaves_no_partial_central_original() {
    let (tmp, pool) = fixture().await;
    let folder = tmp.path().join("claude-code/leaky");
    write_skill(&folder, "leaky", "밖을 가리키는 링크가 있음");
    let outside = tmp.path().join("outside.txt");
    fs::write(&outside, "스킬 밖 파일").unwrap();
    std::os::unix::fs::symlink(&outside, folder.join("outside-link")).unwrap();
    scanner::scan_all_skills_impl(&pool).await.unwrap();

    let error = import_observed_skill_impl(&pool, "leaky", "claude-code")
        .await
        .unwrap_err();

    assert!(error.contains("스킬 밖"), "{error}");
    assert_eq!(fs::read_dir(tmp.path().join("central")).unwrap().count(), 0);
    assert_eq!(
        db::get_unmanaged_installations(&pool, "leaky")
            .await
            .unwrap()
            .len(),
        1
    );
}

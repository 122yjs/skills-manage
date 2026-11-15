#![cfg(unix)]
//! 통합 테스트에서도 개발자 PC의 실제 GitHub CLI(`gh`)를 실행하지 않는다.
use skills_manage_lib::commands::github_import::get_github_auth_status;
use std::{fs, os::unix::fs::PermissionsExt};

#[tokio::test]
async fn github_auth_path_does_not_run_gh() {
    let tmp = tempfile::tempdir().unwrap();
    let marker = tmp.path().join("gh-called");
    let gh = tmp.path().join("gh");
    fs::write(
        &gh,
        format!(
            "#!/bin/sh\n/usr/bin/touch '{}'\necho fake-token\n",
            marker.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
    // 이 테스트 파일에는 테스트가 하나뿐이라 환경변수를 바꿔도 다른 테스트에 영향이 없다.
    std::env::set_var("PATH", tmp.path());

    let source = get_github_auth_status().await.unwrap();

    assert!(!marker.exists(), "실제 gh가 실행되었다");
    assert_eq!(source, "cli_missing");
}

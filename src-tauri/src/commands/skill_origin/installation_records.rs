use super::*;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct RecordedOrigin {
    pub repo_url: String,
    pub source_path: Option<String>,
    pub ref_name: Option<String>,
}

/// 설치 기록은 공용 설치의 실제 경로에만 적용한다. 다른 플랫폼의 독립
/// 복사본은 호출자가 전체 파일을 비교한 뒤 이 기록을 사용할 수 있다.
pub(super) fn read_for_target(target: &Path) -> Result<Option<RecordedOrigin>, String> {
    read_for_target_with_home(
        target,
        &crate::path_utils::resolve_home_dir(),
        std::env::var_os("XDG_STATE_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .as_deref(),
    )
}

fn read_for_target_with_home(
    target: &Path,
    home: &Path,
    state_home: Option<&Path>,
) -> Result<Option<RecordedOrigin>, String> {
    target.canonicalize().map_err(|error| error.to_string())?;
    let Some(name) = target.file_name().and_then(|name| name.to_str()) else {
        return Ok(None);
    };
    let Some(skills_root) = target.parent() else {
        return Ok(None);
    };
    let global_root = home.join(".agents/skills").canonicalize().ok();
    let (lock_path, version) =
        if global_root.is_some() && global_root == skills_root.canonicalize().ok() {
            (
                state_home
                    .map(|root| root.join("skills/.skill-lock.json"))
                    .unwrap_or_else(|| home.join(".agents/.skill-lock.json")),
                3,
            )
        } else if skills_root.file_name().is_some_and(|name| name == "skills")
            && skills_root
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|name| name == ".agents")
        {
            let Some(project) = skills_root.parent().and_then(Path::parent) else {
                return Ok(None);
            };
            (project.join("skills-lock.json"), 1)
        } else {
            return Ok(None);
        };
    let metadata = match fs::metadata(&lock_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("Cannot read skill installation record: {error}")),
    };
    if metadata.len() > 4 * 1024 * 1024 {
        return Err("Skill installation record exceeds the safe size limit".into());
    }
    let bytes = fs::read(&lock_path).map_err(|error| error.to_string())?;
    let lock: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "Invalid skill installation record '{}': {error}",
            lock_path.display()
        )
    })?;
    if lock.get("version").and_then(|value| value.as_u64()) != Some(version) {
        return Ok(None);
    }
    let Some(entry) = lock.get("skills").and_then(|skills| skills.get(name)) else {
        return Ok(None);
    };
    parse_entry(entry)
}

fn parse_entry(entry: &serde_json::Value) -> Result<Option<RecordedOrigin>, String> {
    if entry.get("sourceType").and_then(|value| value.as_str()) != Some("github") {
        return Ok(None);
    }
    let source = entry
        .get("source")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let repo_url = repo_url_from_record(&format!("github:{source}"))
        .ok_or_else(|| "Invalid GitHub repository in skill installation record".to_string())?;
    let source_path = entry
        .get("skillPath")
        .map(|value| {
            let path = value
                .as_str()
                .ok_or("Invalid skillPath in installation record")?;
            // Windows의 드라이브 경로와 역슬래시도 모든 운영체제에서 동일하게 거부한다.
            if path.is_empty()
                || path.contains(['\\', ':'])
                || (path != "." && !safe_relative_path(path))
            {
                return Err("Unsafe skillPath in installation record".to_string());
            }
            Ok(if path == "SKILL.md" {
                ".".to_string()
            } else {
                path.strip_suffix("/SKILL.md").unwrap_or(path).to_string()
            })
        })
        .transpose()?;
    let ref_name = entry
        .get("ref")
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.trim().is_empty() && !value.chars().any(char::is_control))
                .map(str::to_string)
                .ok_or_else(|| "Invalid ref in installation record".to_string())
        })
        .transpose()?;
    // skillFolderHash는 폴더의 해시이며 커밋 ID가 아니다. 설치 버전으로 사용하지 않는다.
    Ok(Some(RecordedOrigin {
        repo_url,
        source_path,
        ref_name,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn installation_records_follow_scope_and_preserve_repository_paths() {
        let temp = TempDir::new().unwrap();
        let home = temp.path().join("home");
        let global = home.join(".agents/skills/demo");
        let project = home.join("project");
        let local = project.join(".agents/skills/demo");
        let other = home.join(".cursor/skills/demo");
        for path in [&global, &local, &other] {
            fs::create_dir_all(path).unwrap();
        }
        let entry = serde_json::json!({"sourceType":"github", "source":"acme/skills", "skillPath":"skills/engineering/demo/SKILL.md", "ref":"release/v2", "skillFolderHash":"not-a-commit"});
        fs::write(
            home.join(".agents/.skill-lock.json"),
            serde_json::json!({"version":3,"skills":{"demo":entry}}).to_string(),
        )
        .unwrap();
        let record = read_for_target_with_home(&global, &home, None)
            .unwrap()
            .unwrap();
        assert_eq!(record.repo_url, "https://github.com/acme/skills");
        assert_eq!(
            record.source_path.as_deref(),
            Some("skills/engineering/demo")
        );
        assert_eq!(record.ref_name.as_deref(), Some("release/v2"));
        assert!(read_for_target_with_home(&other, &home, None)
            .unwrap()
            .is_none());
        assert!(read_for_target_with_home(&local, &home, None)
            .unwrap()
            .is_none());
        fs::write(project.join("skills-lock.json"), serde_json::json!({"version":1,"skills":{"demo":{"sourceType":"github","source":"other/skills"}}}).to_string()).unwrap();
        let record = read_for_target_with_home(&local, &home, None)
            .unwrap()
            .unwrap();
        assert_eq!(record.repo_url, "https://github.com/other/skills");
        assert_eq!(record.source_path, None);
        let state = temp.path().join("state");
        fs::create_dir_all(state.join("skills")).unwrap();
        assert!(read_for_target_with_home(&global, &home, Some(&state))
            .unwrap()
            .is_none());
        fs::copy(
            home.join(".agents/.skill-lock.json"),
            state.join("skills/.skill-lock.json"),
        )
        .unwrap();
        assert!(read_for_target_with_home(&global, &home, Some(&state))
            .unwrap()
            .is_some());
        fs::write(project.join("skills-lock.json"), "{invalid").unwrap();
        assert!(read_for_target_with_home(&local, &home, None).is_err());
    }

    #[test]
    fn installation_records_reject_unsafe_paths_and_non_github_sources() {
        for path in [
            "../demo/SKILL.md",
            "/tmp/SKILL.md",
            "C:/demo/SKILL.md",
            "skills\\demo",
            "",
        ] {
            assert!(parse_entry(&serde_json::json!({"sourceType":"github","source":"acme/skills","skillPath":path})).is_err(), "{path}");
        }
        assert!(
            parse_entry(&serde_json::json!({"sourceType":"github","source":"acme/../skills"}))
                .is_err()
        );
        assert!(
            parse_entry(&serde_json::json!({"sourceType":"local","source":"acme/skills"}))
                .unwrap()
                .is_none()
        );
        assert_eq!(parse_entry(&serde_json::json!({"sourceType":"github","source":"acme/skills","skillPath":"SKILL.md"})).unwrap().unwrap().source_path.as_deref(), Some("."));
    }
}

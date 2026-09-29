use super::*;
use regex::Regex;

/// 문서 링크는 후보만 만든다. 실제 연결은 각 스킬의 파일 전체를 따로 비교한다.
pub(super) fn github_repositories_in_document(content: &str) -> BTreeSet<String> {
    static LINKS: OnceLock<Regex> = OnceLock::new();
    let links = LINKS
        .get_or_init(|| Regex::new(r"https://github\.com/[A-Za-z0-9-]+/[A-Za-z0-9_.-]+").unwrap());
    links
        .find_iter(content)
        .filter_map(|found| {
            if found.start() > 0 && content.as_bytes()[found.start() - 1].is_ascii_alphanumeric() {
                return None;
            }
            let path = found
                .as_str()
                .trim_start_matches("https://github.com/")
                .trim_end_matches('.');
            repo_url_from_record(&format!(
                "github:{}",
                path.trim_end_matches(".git").to_lowercase()
            ))
        })
        .collect()
}

/// 일반 링크는 후보로만 쓰고, 설치 명령이나 명시된 출처는 수정본 판단 근거로 남긴다.
pub(super) fn repository_hints(path: &Path) -> BTreeMap<String, Option<String>> {
    let mut hints = local_document_repositories(path)
        .into_iter()
        .map(|url| (url, None))
        .collect::<BTreeMap<_, _>>();
    if hints.is_empty() {
        return hints;
    }
    let content = fs::read_to_string(path.join("SKILL.md")).unwrap_or_default();
    for line in content.lines() {
        let lower = line.trim().to_lowercase();
        if lower.contains("git clone ")
            || lower.contains("skills add ")
            || ["source:", "repository:", "origin:"]
                .iter()
                .any(|prefix| lower.starts_with(prefix))
        {
            for url in github_repositories_in_document(line) {
                hints.insert(
                    url,
                    Some(path.join("SKILL.md").to_string_lossy().into_owned()),
                );
            }
        }
    }
    hints
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginEvidence {
    pub repository_document: String,
    pub matched_files: Vec<String>,
    pub matched_paragraphs: usize,
    pub matched_characters: usize,
}

/// 본문 참조는 내려받은 동일 스킬 폴더 안에서만 읽는다. 문서의 명령은 실행하지 않는다.
fn content_documents(files: &BTreeMap<String, SnapshotFile>) -> Vec<(String, String)> {
    let skill = files
        .get("SKILL.md")
        .and_then(|f| std::str::from_utf8(&f.bytes).ok())
        .unwrap_or("");
    files
        .iter()
        .filter(|(path, _)| {
            if ["license", "licence", "copying"]
                .iter()
                .any(|word| path.to_lowercase().contains(word))
            {
                return false;
            }
            path.as_str() == "SKILL.md"
                || path.starts_with("scripts/")
                || path.starts_with("references/")
                || skill.contains(&format!("`{path}`"))
                || skill.contains(&format!("]({path})"))
                || skill.contains(&format!("/{path}"))
        })
        .filter_map(|(path, file)| {
            std::str::from_utf8(&file.bytes)
                .ok()
                .map(|text| (path.clone(), text.to_string()))
        })
        .collect()
}

fn descriptions_match(left: &str, right: &str) -> bool {
    let (left, right) = (left.trim(), right.trim());
    left == right
        || [(left, right), (right, left)].iter().any(|(short, long)| {
            // ponytail: 설명은 80자 이상 원문의 끝부분 추가만 허용한다. 전면 재작성은 추가 출처 근거가 있을 때 지원한다.
            short.chars().count() >= 80
                && long
                    .strip_prefix(short)
                    .is_some_and(|suffix| suffix.starts_with(char::is_whitespace))
        })
}

fn body_paragraphs(text: &str, description: &str) -> BTreeSet<String> {
    let normalized = text.replace("\r\n", "\n");
    let body = normalized
        .strip_prefix("---\n")
        .and_then(|rest| rest.split_once("\n---").map(|(_, body)| body))
        .unwrap_or(&normalized);
    let mut filtered = String::new();
    let mut excluded_level = None;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let level = trimmed.chars().take_while(|c| *c == '#').count();
            if excluded_level.is_some_and(|previous| level <= previous) {
                excluded_level = None;
            }
            let heading = trimmed.to_lowercase();
            if [
                "license",
                "install",
                "setup",
                "safety",
                "hard rules",
                "설치",
                "안전",
                "저작권",
            ]
            .iter()
            .any(|word| heading.contains(word))
            {
                excluded_level = Some(level);
            }
            filtered.push_str("\n\n");
            continue;
        }
        if excluded_level.is_some() || trimmed.starts_with("<!--") {
            continue;
        }
        // 목록에 항목이 추가되어도 기존 항목의 일치 근거가 사라지지 않게 나눈다.
        if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
            filtered.push('\n');
        }
        if !trimmed.starts_with("```") {
            filtered.push_str(line);
        }
        filtered.push('\n');
    }
    filtered
        .split("\n\n")
        .map(|block| block.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|block| {
            block.chars().count() >= 32
                && block != description
                && !block.starts_with("https://")
                && !block.starts_with("http://")
        })
        .collect()
}

/// 이름·설명과 독립적인 내용 근거가 함께 있어야 한다. 공통 템플릿은 근거에서 뺀다.
pub(super) fn modified_skill_paths(
    snapshot: &github_import::GitHubRepoSnapshot,
    local_files: &BTreeMap<String, SnapshotFile>,
    repository_document: &str,
) -> Vec<(String, OriginEvidence)> {
    let Some(local_text) = local_files
        .get("SKILL.md")
        .and_then(|f| std::str::from_utf8(&f.bytes).ok())
    else {
        return vec![];
    };
    let Some(identity) = github_import::parse_frontmatter(local_text) else {
        return vec![];
    };
    let Some(description) = identity
        .description
        .as_deref()
        .filter(|d| !d.trim().is_empty())
    else {
        return vec![];
    };
    let skill_paths = snapshot
        .files
        .keys()
        .filter_map(|path| {
            path.strip_suffix("/SKILL.md")
                .or_else(|| (path == "SKILL.md").then_some("."))
        })
        .collect::<Vec<_>>();
    let matching = skill_paths
        .iter()
        .filter(|path| {
            let key = if **path == "." {
                "SKILL.md".into()
            } else {
                format!("{path}/SKILL.md")
            };
            snapshot
                .files
                .get(&key)
                .and_then(|f| std::str::from_utf8(&f.bytes).ok())
                .and_then(github_import::parse_frontmatter)
                .is_some_and(|remote| {
                    remote
                        .name
                        .trim()
                        .eq_ignore_ascii_case(identity.name.trim())
                        && remote
                            .description
                            .as_deref()
                            .is_some_and(|d| descriptions_match(d, description))
                })
        })
        .copied()
        .collect::<Vec<_>>();
    if matching.is_empty() {
        return vec![];
    }
    let mut paragraphs_by_skill = BTreeMap::new();
    let mut paragraph_counts = HashMap::<String, usize>::new();
    let mut file_counts = HashMap::<String, usize>::new();
    // ponytail: 저장소 내 본문 인덱스는 대상별로 만든다. 실제 지연이 커지면 RemoteCache에 보관한다.
    for path in &skill_paths {
        let Ok(files) = remote_files(snapshot, path) else {
            continue;
        };
        let docs = content_documents(&files);
        let paragraphs = docs
            .iter()
            .flat_map(|(_, text)| body_paragraphs(text, description))
            .collect::<BTreeSet<_>>();
        for paragraph in &paragraphs {
            *paragraph_counts.entry(paragraph.clone()).or_default() += 1;
        }
        let hashes = docs
            .iter()
            .filter(|(name, text)| name != "SKILL.md" && text.len() >= 256)
            .map(|(_, text)| sha256_hex(text.as_bytes()))
            .collect::<BTreeSet<_>>();
        for hash in hashes {
            *file_counts.entry(hash).or_default() += 1;
        }
        paragraphs_by_skill.insert(*path, docs);
    }
    let local_docs = content_documents(local_files);
    let mut matches = Vec::new();
    for path in matching {
        let Some(remote_docs) = paragraphs_by_skill.get(path) else {
            continue;
        };
        let mut evidence = OriginEvidence {
            repository_document: repository_document.into(),
            matched_files: vec![],
            matched_paragraphs: 0,
            matched_characters: 0,
        };
        for (remote_name, remote_text) in remote_docs {
            if remote_name != "SKILL.md"
                && remote_text.len() >= 256
                && file_counts.get(&sha256_hex(remote_text.as_bytes())) == Some(&1)
                && local_docs
                    .iter()
                    .any(|(name, text)| name != "SKILL.md" && text == remote_text)
            {
                evidence.matched_files.push(remote_name.clone());
            }
            let remote = body_paragraphs(remote_text, description)
                .into_iter()
                .filter(|p| paragraph_counts.get(p) == Some(&1))
                .collect::<BTreeSet<_>>();
            let remote_len: usize = remote.iter().map(|p| p.chars().count()).sum();
            for (_, text) in &local_docs {
                let local = body_paragraphs(text, description)
                    .into_iter()
                    .filter(|p| paragraph_counts.get(p).is_none_or(|n| *n == 1))
                    .collect::<BTreeSet<_>>();
                let local_len: usize = local.iter().map(|p| p.chars().count()).sum();
                let shared = local.intersection(&remote).collect::<Vec<_>>();
                let chars: usize = shared.iter().map(|p| p.chars().count()).sum();
                if shared.len() >= 2
                    && chars >= 400
                    && chars * 2 >= local_len
                    && chars * 2 >= remote_len
                    && chars > evidence.matched_characters
                {
                    evidence.matched_paragraphs = shared.len();
                    evidence.matched_characters = chars;
                }
            }
        }
        if !evidence.matched_files.is_empty() || evidence.matched_paragraphs > 0 {
            matches.push((path.into(), evidence));
        }
    }
    matches
}

pub(super) fn local_document_repositories(path: &Path) -> BTreeSet<String> {
    let skill_md = path.join("SKILL.md");
    if !fs::metadata(&skill_md)
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= 1024 * 1024)
    {
        return BTreeSet::new();
    }
    fs::read_to_string(skill_md)
        .map(|content| github_repositories_in_document(&content))
        .unwrap_or_default()
}

pub(super) fn exact_skill_paths(
    snapshot: &github_import::GitHubRepoSnapshot,
    local: &SkillManifest,
) -> Vec<String> {
    let Some(local_skill_md) = local.entries.iter().find(|entry| entry.path == "SKILL.md") else {
        return vec![];
    };
    let mut paths = snapshot
        .files
        .iter()
        .filter_map(|(path, file)| {
            let parent = path
                .strip_suffix("/SKILL.md")
                .or_else(|| (path == "SKILL.md").then_some("."))?;
            // 필수 문서부터 비교해 무관한 저장소의 모든 파일을 반복 해싱하지 않는다.
            if file.bytes.len() as u64 != local_skill_md.size
                || sha256_hex(&file.bytes) != local_skill_md.sha256
                || (cfg!(unix) && file.executable) != local_skill_md.executable
            {
                return None;
            }
            Some(parent.to_string())
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .filter(|path| {
            remote_files(snapshot, path).is_ok_and(|files| manifest_from_files(&files) == *local)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn document_links_allow_only_github_repositories() {
        let found = github_repositories_in_document("https://github.com/acme/skills and https://github.com.evil.test/acme/skills and https://gitlab.com/acme/skills");
        assert_eq!(
            found,
            BTreeSet::from(["https://github.com/acme/skills".to_string()])
        );
    }

    fn sample_body() -> String {
        format!("{}\n\n{}", "지역별 관측 시각과 측정 단위를 확인하고 각 관측소의 최근 값을 구분하여 사용한다. ".repeat(7),
            "누락된 측정값은 임의의 숫자로 채우지 않고 자료가 없는 지역과 다음 관측 예정 시각을 함께 기록한다. ".repeat(7))
    }

    fn file(text: &str) -> SnapshotFile {
        SnapshotFile {
            bytes: text.as_bytes().to_vec(),
            executable: false,
        }
    }

    #[test]
    fn moved_body_requires_identity_unique_content_and_sufficient_overlap() {
        let header = "---\nname: forecast\ndescription: 지역 관측 정보를 확인한다\n---\n\n";
        let body = sample_body();
        let local = BTreeMap::from([(
            "SKILL.md".into(),
            file(&format!("{header}{body}\n\n로컬 설정 한 줄")),
        )]);
        let mut snapshot = github_import::GitHubRepoSnapshot { files: HashMap::from([
            ("forecast/SKILL.md".into(), file(&format!("{header}설명: https://github.com/acme/skills/blob/main/forecast/instruction.md"))),
            ("forecast/instruction.md".into(), file(&body)),
        ]) };
        let matches = modified_skill_paths(&snapshot, &local, "/installer/SKILL.md");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].0, "forecast");
        assert!(matches[0].1.matched_characters >= 400);
        // 다른 스킬에도 있는 템플릿 본문이면 연결 근거로 사용할 수 없다.
        snapshot.files.insert(
            "unrelated/SKILL.md".into(),
            file(&format!(
                "---\nname: other\ndescription: 다른 스킬\n---\n\n{body}"
            )),
        );
        assert!(modified_skill_paths(&snapshot, &local, "installer").is_empty());
        snapshot.files.remove("unrelated/SKILL.md");
        snapshot.files.insert(
            "forecast/instruction.md".into(),
            file(&format!(
                "{body}\n\n{}",
                "새로운 동작으로 본문이 대부분 바뀜 ".repeat(200)
            )),
        );
        assert!(modified_skill_paths(&snapshot, &local, "installer").is_empty());
        snapshot
            .files
            .insert("forecast/instruction.md".into(), file(&body));
        snapshot.files.insert(
            "forecast/SKILL.md".into(),
            file(
                "---\nname: forecast\ndescription: 이름만 같은 독립 스킬\n---\n\n`instruction.md`",
            ),
        );
        assert!(modified_skill_paths(&snapshot, &local, "installer").is_empty());
    }

    #[test]
    fn common_license_or_tiny_file_does_not_prove_origin() {
        let text = "---\nname: example\ndescription: 같은 설명\n---\n\n# 설치\nhttps://github.com/acme/skills\n\n# License\n";
        let local = BTreeMap::from([
            (
                "SKILL.md".into(),
                file(&format!("{text}{}", "license terms ".repeat(100))),
            ),
            (
                "references/LICENSE.md".into(),
                file(&"license terms ".repeat(100)),
            ),
            ("scripts/run.sh".into(), file("echo hello")),
        ]);
        let snapshot = github_import::GitHubRepoSnapshot {
            files: local
                .iter()
                .map(|(p, f)| (format!("example/{p}"), f.clone()))
                .collect(),
        };
        assert!(modified_skill_paths(&snapshot, &local, "installer").is_empty());
    }

    #[test]
    fn description_suffix_and_list_additions_preserve_origin_evidence() {
        assert!(!descriptions_match("Regional observations", "Regional observations and more"));
        let description = "Look up current regional observations, keeping the station, measurement time and unit together when reporting each result.";
        let extended = format!("{description} 추가 작업도 지원한다.");
        let first = "- 지역 관측소의 고유 식별자와 관측 시각을 함께 기록하며, 서로 다른 관측소의 측정값을 하나로 섞지 않는다. ".repeat(4);
        let second = "- 온도와 강수량의 단위를 확인하고 원본 자료가 제공하는 값을 그대로 사용하며, 없는 측정값은 임의로 채우지 않는다. ".repeat(4);
        let third = "- 관측 결과의 유효 시각을 표시하고 자료가 갱신되지 않은 관측소는 별도로 알려준다. ".repeat(4);
        let body = format!("{first}\n{second}\n{third}");
        let expanded_body = format!("{first}\n- 새로 추가한 결과 표시 옵션을 사용할 때도 원본 관측값은 바꾸지 않고 보존한다.\n{second}\n{third}");
        for (local_description, remote_description) in [
            (description, extended.as_str()),
            (extended.as_str(), description),
        ] {
            let local = BTreeMap::from([("SKILL.md".into(), file(&format!(
                "---\nname: forecast\ndescription: {local_description}\n---\n\n{body}"
            )))]);
            let mut snapshot = github_import::GitHubRepoSnapshot { files: HashMap::from([
                ("forecast/SKILL.md".into(), file(&format!("---\nname: forecast\ndescription: {remote_description}\n---\n\n`instruction.md`"))),
                ("forecast/instruction.md".into(), file(&expanded_body)),
            ]) };
            let matches = modified_skill_paths(&snapshot, &local, "/installer/SKILL.md");
            assert_eq!(matches.len(), 1);
            assert_eq!(matches[0].1.matched_paragraphs, 3);
            assert!(matches[0].1.matched_characters >= 400);

            // 이름과 설명이 맞아도 본문이 다른 스킬은 연결하지 않는다.
            snapshot.files.insert("forecast/instruction.md".into(), file(&"독립적으로 작성한 다른 스킬의 내용이다. ".repeat(100)));
            assert!(modified_skill_paths(&snapshot, &local, "installer").is_empty());
        }
    }
}

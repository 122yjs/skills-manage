use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;
use serde_yaml::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositorySkillDescriptions {
    pub localized_descriptions: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_locale: Option<String>,
}

#[derive(Debug)]
struct LocalizedReadme {
    locale: String,
    path: PathBuf,
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn normalize_locale(locale: &str) -> Option<String> {
    let normalized = locale.trim().replace('_', "-").to_lowercase();
    if normalized.is_empty()
        || normalized.split('-').any(|part| {
            part.is_empty()
                || !part
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric())
        })
    {
        return None;
    }

    Some(normalized)
}

fn locale_base(locale: &str) -> &str {
    locale.split('-').next().unwrap_or(locale)
}

fn matches_locale_family(candidate: &str, requested: &str) -> bool {
    locale_base(candidate) == locale_base(requested)
}

fn find_matching_locale(
    descriptions: &BTreeMap<String, String>,
    requested: &str,
) -> Option<String> {
    if descriptions.contains_key(requested) {
        return Some(requested.to_string());
    }

    let base = locale_base(requested);
    if descriptions.contains_key(base) {
        return Some(base.to_string());
    }

    descriptions
        .keys()
        .find(|locale| matches_locale_family(locale, requested))
        .cloned()
}

fn extract_frontmatter(content: &str) -> Option<Value> {
    let content = content.strip_prefix('\u{feff}').unwrap_or(content);
    let mut lines = content.lines();
    if lines.next()?.trim() != "---" {
        return None;
    }

    let mut yaml = String::new();
    for line in lines {
        if matches!(line.trim(), "---" | "...") {
            return serde_yaml::from_str(&yaml).ok();
        }
        yaml.push_str(line);
        yaml.push('\n');
    }

    None
}

fn yaml_text(value: &Value) -> Option<String> {
    non_empty(value.as_str().map(ToOwned::to_owned))
}

fn insert_description_map(value: Option<&Value>, descriptions: &mut BTreeMap<String, String>) {
    let Some(mapping) = value.and_then(Value::as_mapping) else {
        return;
    };

    for (locale, description) in mapping {
        let Some(locale) = locale.as_str().and_then(normalize_locale) else {
            continue;
        };
        let Some(description) = yaml_text(description) else {
            continue;
        };
        descriptions.insert(locale, description);
    }
}

fn parse_frontmatter_descriptions(
    content: &str,
    fallback_description: Option<String>,
) -> (BTreeMap<String, String>, Option<String>) {
    let fallback_description = non_empty(fallback_description);
    let Some(frontmatter) = extract_frontmatter(content) else {
        return (BTreeMap::new(), fallback_description);
    };
    let Some(mapping) = frontmatter.as_mapping() else {
        return (BTreeMap::new(), fallback_description);
    };

    let description_value = mapping.get(Value::String("description".to_string()));
    let scalar_description = description_value.and_then(yaml_text);
    let legacy_description = scalar_description.clone().or(fallback_description);
    let mut localized = BTreeMap::new();

    insert_description_map(description_value, &mut localized);
    insert_description_map(
        mapping.get(Value::String("descriptions".to_string())),
        &mut localized,
    );

    // description_ko 같은 명시적 필드를 가장 구체적인 frontmatter 표현으로 본다.
    for (key, value) in mapping {
        let Some(key) = key.as_str() else {
            continue;
        };
        let Some(locale) = key
            .to_ascii_lowercase()
            .strip_prefix("description_")
            .and_then(normalize_locale)
        else {
            continue;
        };
        let Some(description) = yaml_text(value) else {
            continue;
        };
        localized.insert(locale, description);
    }

    (localized, legacy_description)
}

fn localized_readme_locale(file_name: &str) -> Option<String> {
    let lower = file_name.to_ascii_lowercase();
    let locale = lower.strip_prefix("readme")?.strip_suffix(".md")?;
    let locale = locale.strip_prefix(['.', '_', '-'])?;
    let locale = normalize_locale(locale)?;

    matches!(locale_base(&locale), "en" | "ko" | "zh").then_some(locale)
}

fn list_localized_readmes(directory: &Path) -> Vec<LocalizedReadme> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };

    let mut readmes = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let file_name = file_name.to_str()?;
            let locale = localized_readme_locale(file_name)?;
            Some(LocalizedReadme {
                locale,
                path: entry.path(),
            })
        })
        .collect::<Vec<_>>();

    readmes.sort_by(|left, right| left.path.cmp(&right.path));
    readmes
}

fn is_code_fence(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("```") || trimmed.starts_with("~~~")
}

fn is_structural_markdown(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    let is_list = trimmed.starts_with("- ")
        || trimmed.starts_with("* ")
        || trimmed.starts_with("+ ")
        || trimmed
            .split_once(". ")
            .is_some_and(|(prefix, _)| prefix.chars().all(|character| character.is_ascii_digit()));
    let is_rule = trimmed.len() >= 3
        && (trimmed.chars().all(|character| character == '-')
            || trimmed.chars().all(|character| character == '=')
            || trimmed.chars().all(|character| character == '*'));

    trimmed.starts_with('#')
        || trimmed.starts_with('>')
        || trimmed.starts_with('|')
        || trimmed.starts_with('<')
        || trimmed.starts_with("![")
        || trimmed.starts_with("[![")
        || trimmed.contains("![")
        || is_list
        || is_rule
        || line.starts_with("    ")
        || line.starts_with('\t')
}

fn ordinary_paragraph(block: &[String]) -> Option<String> {
    if block.is_empty() || block.iter().any(|line| is_structural_markdown(line)) {
        return None;
    }

    non_empty(Some(
        block
            .iter()
            .map(|line| line.trim())
            .collect::<Vec<_>>()
            .join(" "),
    ))
}

fn extract_first_ordinary_paragraph(content: &str) -> Option<String> {
    let mut block = Vec::new();
    let mut in_code_fence = false;
    let mut in_frontmatter = false;

    for (index, line) in content.lines().enumerate() {
        let trimmed = line.trim();

        if index == 0 && trimmed == "---" {
            in_frontmatter = true;
            continue;
        }
        if in_frontmatter {
            if matches!(trimmed, "---" | "...") {
                in_frontmatter = false;
            }
            continue;
        }
        if is_code_fence(line) {
            if !block.is_empty() {
                if let Some(paragraph) = ordinary_paragraph(&block) {
                    return Some(paragraph);
                }
                block.clear();
            }
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence {
            continue;
        }
        if trimmed.is_empty() {
            if let Some(paragraph) = ordinary_paragraph(&block) {
                return Some(paragraph);
            }
            block.clear();
            continue;
        }

        block.push(line.to_string());
    }

    ordinary_paragraph(&block)
}

fn read_readme_for_locale(
    readmes: &[LocalizedReadme],
    requested_locale: &str,
) -> Option<(String, String)> {
    let mut matching = readmes
        .iter()
        .filter(|readme| matches_locale_family(&readme.locale, requested_locale))
        .collect::<Vec<_>>();

    matching.sort_by_key(|readme| {
        if readme.locale == requested_locale {
            0
        } else if readme.locale == locale_base(requested_locale) {
            1
        } else {
            2
        }
    });

    matching.into_iter().find_map(|readme| {
        let content = fs::read_to_string(&readme.path).ok()?;
        let description = extract_first_ordinary_paragraph(&content)?;
        Some((readme.locale.clone(), description))
    })
}

fn fallback_only(fallback_description: Option<String>) -> RepositorySkillDescriptions {
    let legacy_description = non_empty(fallback_description);
    RepositorySkillDescriptions {
        localized_descriptions: BTreeMap::new(),
        source_locale: None,
        legacy_description,
    }
}

fn repository_skill_descriptions(
    file_path: &Path,
    fallback_description: Option<String>,
    target_locale: &str,
) -> RepositorySkillDescriptions {
    let skill_path = if file_path.is_dir() {
        file_path.join("SKILL.md")
    } else {
        file_path.to_path_buf()
    };
    let Ok(content) = fs::read_to_string(&skill_path) else {
        return fallback_only(fallback_description);
    };

    let (mut localized_descriptions, legacy_description) =
        parse_frontmatter_descriptions(&content, fallback_description);
    let target_locale = normalize_locale(target_locale).unwrap_or_else(|| "en".to_string());
    let readmes = skill_path
        .parent()
        .map(list_localized_readmes)
        .unwrap_or_default();

    // 화면 언어를 먼저 확인하고, 그 다음 영어 README를 확인한다. 같은 언어 계열의
    // frontmatter가 하나라도 있으면 README보다 frontmatter를 우선한다.
    let mut requested_locales = vec![target_locale.clone()];
    if locale_base(&target_locale) != "en" {
        requested_locales.push("en".to_string());
    }
    for requested in requested_locales {
        if find_matching_locale(&localized_descriptions, &requested).is_some() {
            continue;
        }
        if let Some((locale, description)) = read_readme_for_locale(&readmes, &requested) {
            localized_descriptions.insert(locale, description);
        }
    }

    let source_locale = find_matching_locale(&localized_descriptions, &target_locale)
        .or_else(|| find_matching_locale(&localized_descriptions, "en"));

    RepositorySkillDescriptions {
        localized_descriptions,
        legacy_description,
        source_locale,
    }
}

/// SKILL.md와 같은 폴더의 README가 제공하는 짧은 언어별 설명을 읽는다.
/// 파일을 읽을 수 없을 때도 화면이 깨지지 않도록 기존 설명만 반환한다.
#[tauri::command]
pub fn get_repository_skill_descriptions(
    file_path: String,
    fallback_description: Option<String>,
    target_locale: String,
) -> RepositorySkillDescriptions {
    repository_skill_descriptions(Path::new(&file_path), fallback_description, &target_locale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_skill(directory: &Path, frontmatter: &str) -> PathBuf {
        let path = directory.join("SKILL.md");
        fs::write(&path, format!("---\n{frontmatter}\n---\n\n# Skill\n")).unwrap();
        path
    }

    #[test]
    fn parses_all_supported_frontmatter_shapes_with_explicit_fields_first() {
        let directory = tempdir().unwrap();
        let skill = write_skill(
            directory.path(),
            r#"name: demo
description:
  en: English from description map
  ko: 한국어 설명 객체
descriptions:
  zh_CN: 简体中文说明
  ko: 한국어 descriptions
description_ko: 한국어 명시 필드
description_en: Explicit English"#,
        );

        let result = repository_skill_descriptions(&skill, None, "ko-KR");

        assert_eq!(
            result.localized_descriptions.get("ko"),
            Some(&"한국어 명시 필드".to_string())
        );
        assert_eq!(
            result.localized_descriptions.get("en"),
            Some(&"Explicit English".to_string())
        );
        assert_eq!(
            result.localized_descriptions.get("zh-cn"),
            Some(&"简体中文说明".to_string())
        );
        assert_eq!(result.legacy_description, None);
        assert_eq!(result.source_locale.as_deref(), Some("ko"));
    }

    #[test]
    fn scalar_description_remains_the_language_unknown_legacy_fallback() {
        let directory = tempdir().unwrap();
        let skill = write_skill(
            directory.path(),
            "name: demo\ndescription: A concise skill description",
        );
        fs::write(
            directory.path().join("README.en.md"),
            "# Demo\n\nA longer README description.",
        )
        .unwrap();

        let result =
            repository_skill_descriptions(&skill, Some("stale database value".to_string()), "ko");

        assert_eq!(
            result.localized_descriptions.get("en"),
            Some(&"A longer README description.".to_string())
        );
        assert_eq!(
            result.legacy_description.as_deref(),
            Some("A concise skill description")
        );
        assert_eq!(result.source_locale.as_deref(), Some("en"));
    }

    #[test]
    fn reads_target_readme_first_and_also_collects_english_fallback() {
        let directory = tempdir().unwrap();
        let skill = write_skill(directory.path(), "name: demo");
        fs::write(
            directory.path().join("README_ko_KR.md"),
            "# 데모\n\n한국어 저장소 설명입니다.",
        )
        .unwrap();
        fs::write(
            directory.path().join("README-en.md"),
            "# Demo\n\nEnglish repository description.",
        )
        .unwrap();
        fs::write(
            directory.path().join("README.zh.md"),
            "# 演示\n\n不应读取的说明。",
        )
        .unwrap();

        let result = repository_skill_descriptions(&skill, None, "ko-KR");

        assert_eq!(
            result.localized_descriptions,
            BTreeMap::from([
                (
                    "en".to_string(),
                    "English repository description.".to_string()
                ),
                ("ko-kr".to_string(), "한국어 저장소 설명입니다.".to_string()),
            ])
        );
        assert_eq!(result.source_locale.as_deref(), Some("ko-kr"));
    }

    #[test]
    fn frontmatter_locale_wins_over_a_more_specific_readme_variant() {
        let directory = tempdir().unwrap();
        let skill = write_skill(
            directory.path(),
            "name: demo\ndescription_ko: frontmatter 설명",
        );
        fs::write(
            directory.path().join("README.ko-KR.md"),
            "# 데모\n\nREADME 설명",
        )
        .unwrap();

        let result = repository_skill_descriptions(&skill, None, "ko-KR");

        assert_eq!(
            result.localized_descriptions,
            BTreeMap::from([("ko".to_string(), "frontmatter 설명".to_string())])
        );
    }

    #[test]
    fn readme_uses_first_plain_paragraph_and_skips_non_prose_blocks() {
        let directory = tempdir().unwrap();
        let skill = write_skill(directory.path(), "name: demo");
        fs::write(
            directory.path().join("README.zh-CN.md"),
            r#"---
title: 演示
---
# 演示

[![build](https://img.shields.io/badge/build-passing.svg)](https://example.com)

![screenshot](screenshot.png)

```shell
echo not-a-description
```

- 功能一
- 功能二

这是第一段普通说明，
它换行后仍属于同一段。

这是第二段。"#,
        )
        .unwrap();

        let result = repository_skill_descriptions(&skill, None, "zh_CN");

        assert_eq!(
            result.localized_descriptions.get("zh-cn"),
            Some(&"这是第一段普通说明， 它换行后仍属于同一段。".to_string())
        );
    }

    #[test]
    fn missing_skill_file_returns_only_trimmed_fallback_without_panicking() {
        let directory = tempdir().unwrap();

        let result = repository_skill_descriptions(
            &directory.path().join("missing.md"),
            Some("  existing description  ".to_string()),
            "ko",
        );

        assert!(result.localized_descriptions.is_empty());
        assert_eq!(
            result.legacy_description.as_deref(),
            Some("existing description")
        );
        assert_eq!(result.source_locale, None);
    }

    #[test]
    fn malformed_frontmatter_can_still_use_localized_readmes_and_fallback() {
        let directory = tempdir().unwrap();
        let skill = directory.path().join("SKILL.md");
        fs::write(&skill, "---\ndescription: [invalid\n---\n").unwrap();
        fs::write(
            directory.path().join("README.ko.md"),
            "# 데모\n\nREADME 한국어 설명",
        )
        .unwrap();

        let result =
            repository_skill_descriptions(&skill, Some("English fallback".to_string()), "ko");

        assert_eq!(
            result.localized_descriptions.get("ko"),
            Some(&"README 한국어 설명".to_string())
        );
        assert_eq!(
            result.legacy_description.as_deref(),
            Some("English fallback")
        );
    }

    #[test]
    fn accepts_a_skill_directory_as_the_file_path() {
        let directory = tempdir().unwrap();
        write_skill(directory.path(), "name: demo\ndescription_zh: 中文说明");

        let result = repository_skill_descriptions(directory.path(), None, "zh-CN");

        assert_eq!(
            result.localized_descriptions.get("zh"),
            Some(&"中文说明".to_string())
        );
    }

    #[test]
    fn localized_readme_name_parser_accepts_documented_separator_variants() {
        for (file_name, expected) in [
            ("README.ko.md", "ko"),
            ("README.ko-KR.md", "ko-kr"),
            ("README_zh_CN.md", "zh-cn"),
            ("README-en-US.md", "en-us"),
        ] {
            assert_eq!(
                localized_readme_locale(file_name).as_deref(),
                Some(expected)
            );
        }

        assert_eq!(localized_readme_locale("README.md"), None);
        assert_eq!(localized_readme_locale("README.fr.md"), None);
    }
}

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct SkillDescriptionTranslation {
    pub resource_id: String,
    pub source_hash: String,
    pub source_text: String,
    pub source_locale: Option<String>,
    pub target_locale: String,
    pub engine: String,
    pub translated_text: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 언어 태그를 DB 키로 쓸 수 있도록 `ko_KR`과 `ko-KR`을 같은 형태로 맞춘다.
pub fn normalize_description_locale(locale: &str) -> String {
    locale.trim().replace('_', "-").to_ascii_lowercase()
}

/// 설명 원문이 바뀌면 기존 번역을 재사용하지 않도록 만드는 안정적인 해시다.
///
/// FNV-1a는 작은 문자열 키에 충분히 빠르고 플랫폼과 실행 시점에 관계없이 같은
/// 값을 만든다. DB 조회 시 원문도 함께 비교하므로 해시 충돌이 잘못된 번역을
/// 반환하지는 않는다.
pub fn description_source_hash(source_text: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in source_text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_normalization_is_stable() {
        assert_eq!(normalize_description_locale(" KO_kr "), "ko-kr");
    }

    #[test]
    fn source_hash_changes_with_source_text() {
        assert_eq!(
            description_source_hash("English description"),
            description_source_hash("English description")
        );
        assert_ne!(
            description_source_hash("English description"),
            description_source_hash("Updated English description")
        );
    }
}

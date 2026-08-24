use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::{
    db::{
        get_skill_description_translation, normalize_description_locale,
        upsert_skill_description_translation,
    },
    AppState,
};

use super::{
    marketplace::translate_skill_description_impl,
    on_device_translation::{translate_on_device_impl, OnDeviceTranslationRequest},
};

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DescriptionTranslationRequest {
    pub resource_id: String,
    pub source_text: String,
    pub source_locale: Option<String>,
    pub target_locale: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DescriptionTranslationResult {
    pub translated_text: String,
    pub engine: String,
    pub target_locale: String,
    pub cached: bool,
}

fn validate_request(request: &DescriptionTranslationRequest) -> Result<(), String> {
    if request.resource_id.trim().is_empty() {
        return Err("번역을 저장할 스킬 식별자가 없습니다.".to_string());
    }
    if request.source_text.trim().is_empty() {
        return Err("번역할 설명이 비어 있습니다.".to_string());
    }
    if request.target_locale.trim().is_empty() {
        return Err("번역할 대상 언어가 없습니다.".to_string());
    }
    let target = normalize_description_locale(&request.target_locale);
    if !matches!(target.split('-').next(), Some("en" | "ko" | "zh")) {
        return Err("지원하지 않는 번역 대상 언어입니다.".to_string());
    }
    Ok(())
}

async fn load_cached(
    state: &AppState,
    request: &DescriptionTranslationRequest,
) -> Result<Option<DescriptionTranslationResult>, String> {
    validate_request(request)?;

    // 사용자가 명시적으로 요청한 API 번역이 있으면 그 선택을 보존한다.
    // API 번역이 없을 때만 무료 기기 번역 캐시를 사용한다.
    for engine in ["api", "apple"] {
        if let Some(cached) = get_skill_description_translation(
            &state.db,
            &request.resource_id,
            &request.source_text,
            &request.target_locale,
            engine,
        )
        .await?
        {
            return Ok(Some(DescriptionTranslationResult {
                translated_text: cached.translated_text,
                engine: cached.engine,
                target_locale: cached.target_locale,
                cached: true,
            }));
        }
    }

    Ok(None)
}

async fn save_translation(
    state: &AppState,
    request: &DescriptionTranslationRequest,
    engine: &str,
    translated_text: &str,
) -> Result<DescriptionTranslationResult, String> {
    let saved = upsert_skill_description_translation(
        &state.db,
        &request.resource_id,
        &request.source_text,
        request.source_locale.as_deref(),
        &request.target_locale,
        engine,
        translated_text,
    )
    .await?;

    Ok(DescriptionTranslationResult {
        translated_text: saved.translated_text,
        engine: saved.engine,
        target_locale: saved.target_locale,
        cached: false,
    })
}

/// 언어를 바꿀 때는 네트워크 요청 없이 저장된 번역만 읽는다.
#[tauri::command]
pub async fn get_cached_skill_description_translation(
    state: State<'_, AppState>,
    request: DescriptionTranslationRequest,
) -> Result<Option<DescriptionTranslationResult>, String> {
    load_cached(&state, &request).await
}

/// macOS가 제공하는 기기 내 번역을 실행하고 결과를 로컬 DB에 저장한다.
#[tauri::command]
pub async fn translate_skill_description_on_device(
    app: AppHandle,
    state: State<'_, AppState>,
    request: DescriptionTranslationRequest,
) -> Result<DescriptionTranslationResult, String> {
    validate_request(&request)?;

    if let Some(cached) = get_skill_description_translation(
        &state.db,
        &request.resource_id,
        &request.source_text,
        &request.target_locale,
        "apple",
    )
    .await?
    {
        return Ok(DescriptionTranslationResult {
            translated_text: cached.translated_text,
            engine: cached.engine,
            target_locale: cached.target_locale,
            cached: true,
        });
    }

    let translated = translate_on_device_impl(
        app,
        OnDeviceTranslationRequest {
            source_text: request.source_text.clone(),
            source_language: request.source_locale.clone(),
            target_language: normalize_description_locale(&request.target_locale),
        },
    )
    .await
    .map_err(|error| error.message)?;

    save_translation(
        &state,
        &request,
        translated.engine,
        &translated.translated_text,
    )
    .await
}

/// 사용자가 개별 스킬의 버튼을 눌렀을 때만 설정된 AI API로 번역한다.
#[tauri::command]
pub async fn translate_skill_description_with_api(
    state: State<'_, AppState>,
    request: DescriptionTranslationRequest,
) -> Result<DescriptionTranslationResult, String> {
    validate_request(&request)?;

    let translated =
        translate_skill_description_impl(&state.db, &request.source_text, &request.target_locale)
            .await?;

    save_translation(&state, &request, "api", &translated).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(
        resource_id: &str,
        source_text: &str,
        target_locale: &str,
    ) -> DescriptionTranslationRequest {
        DescriptionTranslationRequest {
            resource_id: resource_id.to_string(),
            source_text: source_text.to_string(),
            source_locale: Some("en".to_string()),
            target_locale: target_locale.to_string(),
        }
    }

    #[test]
    fn rejects_missing_translation_identity_text_and_locale() {
        assert!(validate_request(&request("", "Source", "ko")).is_err());
        assert!(validate_request(&request("skill-a", "  ", "ko")).is_err());
        assert!(validate_request(&request("skill-a", "Source", "  ")).is_err());
        assert!(validate_request(&request("skill-a", "Source", "fr")).is_err());
    }

    #[test]
    fn accepts_complete_per_skill_translation_request() {
        assert!(validate_request(&request("local:/skill/SKILL.md", "Source", "ko-KR")).is_ok());
    }
}

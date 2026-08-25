use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnDeviceTranslation {
    pub translated_text: String,
    pub engine: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnDeviceTranslationError {
    pub code: &'static str,
    pub message: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnDeviceTranslationRequest {
    pub source_text: String,
    pub source_language: Option<String>,
    pub target_language: String,
}

#[cfg(target_os = "macos")]
pub(crate) async fn translate_on_device_impl(
    app: tauri::AppHandle,
    request: OnDeviceTranslationRequest,
) -> Result<OnDeviceTranslation, OnDeviceTranslationError> {
    macos::translate(app, request).await
}

#[cfg(not(target_os = "macos"))]
pub(crate) async fn translate_on_device_impl(
    _app: tauri::AppHandle,
    _request: OnDeviceTranslationRequest,
) -> Result<OnDeviceTranslation, OnDeviceTranslationError> {
    Err(error(
        "unsupported_platform",
        "기기 내 무료 번역은 macOS에서만 사용할 수 있습니다.",
    ))
}

fn error(code: &'static str, message: impl Into<String>) -> OnDeviceTranslationError {
    OnDeviceTranslationError {
        code,
        message: message.into(),
    }
}

fn message_for_code(code: &'static str) -> &'static str {
    match code {
        "unsupported_os" => "기기 내 무료 번역은 macOS 15 이상에서 사용할 수 있습니다.",
        "unsupported_source_language" => "원문 언어를 Apple 번역이 지원하지 않습니다.",
        "unsupported_target_language" => "대상 언어를 Apple 번역이 지원하지 않습니다.",
        "unsupported_language_pairing" => "이 언어 조합은 Apple 번역에서 지원하지 않습니다.",
        "language_not_downloaded" => {
            "이 언어 조합의 번역 언어 팩이 아직 내려받아지지 않았습니다."
        }
        "unable_to_identify_language" => "원문의 언어를 확인하지 못했습니다.",
        "source_matches_target" => "원문과 대상 언어가 같아 번역하지 않았습니다.",
        "empty_text" => "번역할 설명이 없습니다.",
        "cancelled" => "기기 내 번역이 취소되었습니다.",
        "translation_timeout" => "기기 내 번역 대기 시간이 초과되었습니다.",
        _ => "기기 내 번역을 완료하지 못했습니다.",
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::{error, message_for_code, OnDeviceTranslation, OnDeviceTranslationRequest};
    use std::{
        ffi::{c_char, c_void, CStr, CString},
        ptr,
    };
    use tauri::Manager;
    use tokio::sync::oneshot;

    type Callback = unsafe extern "C" fn(
        translated_text: *const c_char,
        error_code: *const c_char,
        context: *mut c_void,
    );

    extern "C" {
        fn skills_manage_translate_on_device(
            source_text: *const c_char,
            source_language: *const c_char,
            target_language: *const c_char,
            parent_view: *mut c_void,
            callback: Callback,
            context: *mut c_void,
        );
        fn skills_manage_cancel_translation(context: *mut c_void);
    }

    struct CallbackState {
        sender: Option<oneshot::Sender<Result<String, &'static str>>>,
    }

    struct NativeRequestGuard {
        window: tauri::WebviewWindow,
        context: usize,
        armed: bool,
    }

    impl Drop for NativeRequestGuard {
        fn drop(&mut self) {
            if !self.armed {
                return;
            }

            let context = self.context;
            let _ = self.window.run_on_main_thread(move || unsafe {
                skills_manage_cancel_translation(context as *mut c_void);
            });
        }
    }

    pub(super) async fn translate(
        app: tauri::AppHandle,
        request: OnDeviceTranslationRequest,
    ) -> Result<OnDeviceTranslation, super::OnDeviceTranslationError> {
        if request.source_text.trim().is_empty() {
            return Err(error("empty_text", message_for_code("empty_text")));
        }
        if request.target_language.trim().is_empty() {
            return Err(error(
                "unsupported_target_language",
                message_for_code("unsupported_target_language"),
            ));
        }

        let source_text = c_string(&request.source_text, "invalid_source_text")?;
        let source_language = request
            .source_language
            .as_deref()
            .filter(|language| !language.trim().is_empty())
            .map(|language| c_string(language, "invalid_source_language"))
            .transpose()?;
        let target_language = c_string(&request.target_language, "invalid_target_language")?;

        let window = app.get_webview_window("main").ok_or_else(|| {
            error(
                "window_unavailable",
                "번역을 실행할 앱 창을 찾지 못했습니다.",
            )
        })?;
        let (sender, receiver) = oneshot::channel();
        let state = Box::new(CallbackState {
            sender: Some(sender),
        });
        let context = Box::into_raw(state) as usize;

        let window_for_call = window.clone();
        if let Err(run_error) = window.run_on_main_thread(move || {
            let context = context as *mut c_void;
            match window_for_call.ns_view() {
                Ok(parent_view) => unsafe {
                    skills_manage_translate_on_device(
                        source_text.as_ptr(),
                        source_language
                            .as_ref()
                            .map_or(ptr::null(), |language| language.as_ptr()),
                        target_language.as_ptr(),
                        parent_view,
                        translation_completed,
                        context,
                    );
                },
                Err(_) => unsafe {
                    translation_completed(ptr::null(), ptr::null(), context);
                },
            }
        }) {
            let mut state = unsafe { Box::from_raw(context as *mut CallbackState) };
            if let Some(sender) = state.sender.take() {
                let _ = sender.send(Err("translation_failed"));
            }
            return Err(error("window_unavailable", run_error.to_string()));
        }

        let mut guard = NativeRequestGuard {
            window,
            context,
            armed: true,
        };
        let result = receiver.await;
        guard.armed = false;

        match result {
            Ok(Ok(translated_text)) => Ok(OnDeviceTranslation {
                translated_text,
                engine: "apple",
            }),
            Ok(Err(code)) => Err(error(code, message_for_code(code))),
            Err(_) => Err(error(
                "translation_failed",
                message_for_code("translation_failed"),
            )),
        }
    }

    fn c_string(
        value: &str,
        code: &'static str,
    ) -> Result<CString, super::OnDeviceTranslationError> {
        CString::new(value).map_err(|_| error(code, "번역 입력에 지원하지 않는 문자가 있습니다."))
    }

    unsafe extern "C" fn translation_completed(
        translated_text: *const c_char,
        error_code: *const c_char,
        context: *mut c_void,
    ) {
        if context.is_null() {
            return;
        }

        let mut state = Box::from_raw(context as *mut CallbackState);
        let result = if !translated_text.is_null() {
            Ok(CStr::from_ptr(translated_text)
                .to_string_lossy()
                .into_owned())
        } else {
            let code = if error_code.is_null() {
                "translation_failed"
            } else {
                match CStr::from_ptr(error_code)
                    .to_str()
                    .unwrap_or("translation_failed")
                {
                    "unsupported_os" => "unsupported_os",
                    "unsupported_source_language" => "unsupported_source_language",
                    "unsupported_target_language" => "unsupported_target_language",
                    "unsupported_language_pairing" => "unsupported_language_pairing",
                    "language_not_downloaded" => "language_not_downloaded",
                    "unable_to_identify_language" => "unable_to_identify_language",
                    "source_matches_target" => "source_matches_target",
                    "empty_text" => "empty_text",
                    "cancelled" => "cancelled",
                    "translation_timeout" => "translation_timeout",
                    _ => "translation_failed",
                }
            };
            Err(code)
        };

        if let Some(sender) = state.sender.take() {
            let _ = sender.send(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::message_for_code;

    #[test]
    fn known_native_error_codes_have_user_messages() {
        for code in [
            "unsupported_os",
            "unsupported_source_language",
            "unsupported_target_language",
            "unsupported_language_pairing",
            "language_not_downloaded",
            "unable_to_identify_language",
            "source_matches_target",
            "empty_text",
            "cancelled",
            "translation_timeout",
            "translation_failed",
        ] {
            assert!(!message_for_code(code).is_empty());
        }
    }
}

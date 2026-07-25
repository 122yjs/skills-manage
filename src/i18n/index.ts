import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import LanguageDetector from "i18next-browser-languagedetector";

import zh from "./locales/zh.json";
import en from "./locales/en.json";
import ko from "./locales/ko.json";

i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      zh: { translation: zh },
      en: { translation: en },
      ko: { translation: ko },
    },
    // 저장된 선택이 없으면 WebView의 navigator 언어(OS 언어)를 사용한다.
    // 지원하지 않는 OS 언어는 영어로 표시한다.
    supportedLngs: ["zh", "en", "ko"],
    nonExplicitSupportedLngs: true,
    load: "languageOnly",
    fallbackLng: "en",
    // Use localStorage key 'i18nextLng' (i18next default) to persist choice.
    detection: {
      order: ["localStorage", "navigator"],
      lookupLocalStorage: "i18nextLng",
      caches: ["localStorage"],
    },
    interpolation: {
      escapeValue: false, // React already handles XSS escaping
    },
  });

export default i18n;

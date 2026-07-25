import { beforeEach, describe, expect, it, vi } from "vitest";

function setOsLanguage(language: string) {
  Object.defineProperty(window.navigator, "language", {
    configurable: true,
    value: language,
  });
  Object.defineProperty(window.navigator, "languages", {
    configurable: true,
    value: [language],
  });
}

async function loadI18n() {
  vi.resetModules();
  return (await import("../i18n")).default;
}

describe("i18n 기본 언어", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("저장된 선택이 없으면 한국어 OS 언어를 감지한다", async () => {
    setOsLanguage("ko-KR");

    const i18n = await loadI18n();

    expect(i18n.resolvedLanguage).toBe("ko");
    expect(i18n.t("settings.title")).toBe("설정");
  });

  it("지원하지 않는 OS 언어는 영어로 표시한다", async () => {
    setOsLanguage("fr-FR");

    const i18n = await loadI18n();

    expect(i18n.resolvedLanguage).toBe("en");
    expect(i18n.t("settings.title")).toBe("Settings");
  });

  it("저장된 사용자 언어 선택을 OS 언어보다 우선한다", async () => {
    setOsLanguage("ko-KR");
    localStorage.setItem("i18nextLng", "zh");

    const i18n = await loadI18n();

    expect(i18n.resolvedLanguage).toBe("zh");
    expect(i18n.t("settings.title")).toBe("设置");
  });
});

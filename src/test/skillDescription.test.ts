import { describe, expect, it } from "vitest";
import { resolveSkillDescription } from "@/lib/skillDescription";

describe("resolveSkillDescription", () => {
  it("현재 언어로 저장소가 제공한 설명을 가장 먼저 고른다", () => {
    expect(
      resolveSkillDescription({
        locale: "ko-KR",
        localizedDescriptions: {
          ko: "한국어 설명",
          en: "English description",
        },
        legacyDescription: "원문",
      })
    ).toEqual({
      text: "한국어 설명",
      locale: "ko",
      source: "requested-locale",
    });
  });

  it("현재 언어가 없으면 영어 설명으로 fallback한다", () => {
    expect(
      resolveSkillDescription({
        locale: "zh-CN",
        localizedDescriptions: { en_US: "English from repository" },
        legacyDescription: "Legacy description",
      })
    ).toEqual({
      text: "English from repository",
      locale: "en-us",
      source: "english-fallback",
    });
  });

  it("현재 언어와 영어가 없으면 기존 원문으로 fallback한다", () => {
    expect(
      resolveSkillDescription({
        locale: "ko",
        localizedDescriptions: { zh: "中文说明" },
        legacyDescription: "  Original description  ",
      })
    ).toEqual({
      text: "Original description",
      source: "original-fallback",
    });
  });

  it("빈 설명은 건너뛰고 사용할 값이 없으면 undefined를 반환한다", () => {
    expect(
      resolveSkillDescription({
        locale: "ko",
        localizedDescriptions: { ko: " ", en: "" },
        legacyDescription: "  ",
      })
    ).toBeUndefined();
  });

  it("정확한 지역 언어가 기본 언어보다 우선한다", () => {
    expect(
      resolveSkillDescription({
        locale: "zh-TW",
        localizedDescriptions: {
          zh: "简体说明",
          "zh-TW": "繁體說明",
        },
      })
    ).toMatchObject({ text: "繁體說明", locale: "zh-tw" });
  });
});

import { describe, expect, it } from "vitest";
import { AI_PROVIDERS } from "../data/aiProviders";

describe("AI 제공자 기본 모델 목록", () => {
  it("모든 기본 제공자가 선택 가능한 모델을 가진다", () => {
    for (const provider of AI_PROVIDERS.filter((item) => item.id !== "custom")) {
      expect(provider.models, provider.id).toBeDefined();
      expect(provider.models, provider.id).toContain(provider.defaultModel);
    }
  });
});

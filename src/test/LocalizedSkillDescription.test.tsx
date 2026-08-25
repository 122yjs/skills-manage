import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LocalizedSkillDescription } from "@/components/skill/LocalizedSkillDescription";
import { resetUnavailableOnDeviceTranslations } from "@/hooks/useSkillDescriptionTranslation";

const { mockInvoke, mockIsTauriRuntime } = vi.hoisted(() => ({
  mockInvoke: vi.fn(),
  mockIsTauriRuntime: vi.fn(() => true),
}));

vi.mock("@/lib/tauri", () => ({
  invoke: mockInvoke,
  isTauriRuntime: mockIsTauriRuntime,
}));

type ObserverCallback = (entries: Array<{ isIntersecting: boolean }>) => void;
let observerCallback: ObserverCallback | undefined;
const observe = vi.fn();
const disconnect = vi.fn();

beforeEach(() => {
  mockInvoke.mockReset();
  mockIsTauriRuntime.mockReturnValue(true);
  resetUnavailableOnDeviceTranslations();
  observerCallback = undefined;
  observe.mockReset();
  disconnect.mockReset();

  class MockIntersectionObserver {
    constructor(callback: ObserverCallback) {
      observerCallback = callback;
    }

    observe = observe;
    disconnect = disconnect;
  }

  vi.stubGlobal("IntersectionObserver", MockIntersectionObserver);
});

function renderDescription(overrides: Partial<React.ComponentProps<typeof LocalizedSkillDescription>> = {}) {
  return render(
    <LocalizedSkillDescription
      resourceId="skill:demo"
      description="English legacy description"
      {...overrides}
    />
  );
}

describe("LocalizedSkillDescription", () => {
  it("보이기 전에는 캐시나 기기 번역을 호출하지 않는다", () => {
    renderDescription();

    expect(screen.getByText("English legacy description")).toBeInTheDocument();
    expect(observe).toHaveBeenCalledTimes(1);
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("한 번 본 카드도 화면 밖으로 나가면 다음 언어 작업 대상에서 제외한다", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_cached_skill_description_translation") return Promise.resolve(null);
      if (command === "translate_skill_description_on_device") return Promise.reject("unsupported");
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });

    renderDescription({ sourceLocale: "en" });
    await act(async () => observerCallback?.([{ isIntersecting: true }]));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalled());

    await act(async () => observerCallback?.([{ isIntersecting: false }]));
    mockInvoke.mockClear();
    await act(async () => Promise.resolve());

    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("저장소가 현재 언어 설명을 제공하면 번역 호출 없이 바로 사용한다", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_repository_skill_descriptions") {
        return Promise.resolve({
          localizedDescriptions: { zh: "仓库提供的中文说明" },
          legacyDescription: "English legacy description",
          sourceLocale: "en",
        });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });

    renderDescription({ filePath: "/skills/demo/SKILL.md" });
    await act(async () => observerCallback?.([{ isIntersecting: true }]));

    expect(await screen.findByText("仓库提供的中文说明")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("get_repository_skill_descriptions", {
      filePath: "/skills/demo/SKILL.md",
      fallbackDescription: "English legacy description",
      targetLocale: "zh",
    });
    expect(mockInvoke).not.toHaveBeenCalledWith(
      "get_cached_skill_description_translation",
      expect.anything()
    );
  });

  it("보이는 카드만 캐시를 조회하고 캐시가 없을 때 기기 번역한다", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_cached_skill_description_translation") return Promise.resolve(null);
      if (command === "translate_skill_description_on_device") {
        return Promise.resolve({
          translatedText: "设备翻译说明",
          engine: "apple",
          targetLocale: "zh",
          cached: false,
        });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });

    renderDescription({ sourceLocale: "en" });
    await act(async () => observerCallback?.([{ isIntersecting: true }]));

    expect(await screen.findByText("设备翻译说明")).toBeInTheDocument();
    expect(screen.getByText("已在此 Mac 上翻译")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenNthCalledWith(1, "get_cached_skill_description_translation", {
      request: {
        resourceId: "skill:demo",
        sourceText: "English legacy description",
        sourceLocale: "en",
        targetLocale: "zh",
      },
    });
    expect(mockInvoke).toHaveBeenNthCalledWith(2, "translate_skill_description_on_device", {
      request: {
        resourceId: "skill:demo",
        sourceText: "English legacy description",
        sourceLocale: "en",
        targetLocale: "zh",
      },
    });

    fireEvent.click(screen.getByRole("button", { name: "查看原文" }));
    expect(screen.getByText("English legacy description")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查看译文" })).toBeInTheDocument();
  });

  it("API 번역은 개별 카드에서 비용 안내를 다시 확인한 뒤에만 호출한다", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_cached_skill_description_translation") return Promise.resolve(null);
      if (command === "translate_skill_description_on_device") return Promise.reject("unsupported");
      if (command === "translate_skill_description_with_api") {
        return Promise.resolve({
          translatedText: "API 翻译说明",
          engine: "api",
          targetLocale: "zh",
          cached: false,
        });
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });

    renderDescription({ immediate: true, sourceLocale: "en" });
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        "translate_skill_description_on_device",
        expect.anything()
      );
    });

    fireEvent.click(screen.getByRole("button", { name: "使用 API 翻译" }));
    expect(screen.getByText("使用已配置的 API？可能产生费用")).toBeInTheDocument();
    expect(mockInvoke).not.toHaveBeenCalledWith(
      "translate_skill_description_with_api",
      expect.anything()
    );

    fireEvent.click(screen.getByRole("button", { name: "确认" }));
    expect(await screen.findByText("API 翻译说明")).toBeInTheDocument();
    expect(screen.getByText("已使用 API 翻译")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledWith("translate_skill_description_with_api", {
      request: {
        resourceId: "skill:demo",
        sourceText: "English legacy description",
        sourceLocale: "en",
        targetLocale: "zh",
      },
    });
  });

  it("Tauri가 아닌 환경에서는 원문만 표시하고 번역 동작을 숨긴다", () => {
    mockIsTauriRuntime.mockReturnValue(false);

    renderDescription({ immediate: true, sourceLocale: "en" });

    expect(screen.getByText("English legacy description")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "使用 API 翻译" })).not.toBeInTheDocument();
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("기기 번역이 실패한 언어 조합은 다른 카드에서 다시 호출하지 않는다", async () => {
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_cached_skill_description_translation") return Promise.resolve(null);
      if (command === "translate_skill_description_on_device") {
        return Promise.reject("language_not_downloaded");
      }
      return Promise.reject(new Error(`unexpected command: ${command}`));
    });

    renderDescription({ immediate: true, sourceLocale: "en" });
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        "translate_skill_description_on_device",
        expect.anything()
      );
    });

    mockInvoke.mockClear();
    renderDescription({
      immediate: true,
      sourceLocale: "en",
      resourceId: "skill:another",
    });
    await waitFor(() => {
      expect(mockInvoke).toHaveBeenCalledWith(
        "get_cached_skill_description_translation",
        expect.anything()
      );
    });

    expect(mockInvoke).not.toHaveBeenCalledWith(
      "translate_skill_description_on_device",
      expect.anything()
    );
  });
});

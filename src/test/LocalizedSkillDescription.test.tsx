import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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

  it("Tauri가 아닌 환경에서도 두 메뉴와 사용 불가 이유를 표시한다", () => {
    mockIsTauriRuntime.mockReturnValue(false);

    renderDescription({ immediate: true, sourceLocale: "en" });

    expect(screen.getByText("English legacy description")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "使用 API 翻译" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "在设备上翻译" })).toBeDisabled();
    expect(screen.getByText("请在桌面应用中使用翻译")).toBeInTheDocument();
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
  it("현재 언어 설명이 있어도 두 메뉴를 비활성 상태로 유지한다", () => {
    renderDescription({ immediate: true, localizedDescriptions: { zh: "中文说明" } });
    expect(screen.getByRole("button", { name: "使用 API 翻译" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "在设备上翻译" })).toBeDisabled();
    expect(screen.getByText("说明已使用当前语言")).toBeInTheDocument();
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("실패 이유를 표시하고 선택한 카드만 기기 번역을 재시도한다", async () => {
    let attempts = 0;
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_cached_skill_description_translation") return Promise.resolve(null);
      if (command === "translate_skill_description_on_device") {
        attempts += 1;
        return attempts === 1 ? Promise.reject("language_not_downloaded") : Promise.resolve({
          translatedText: "重试成功", engine: "apple", targetLocale: "zh", cached: false,
        });
      }
      return Promise.reject(new Error(command));
    });
    const first = renderDescription({ immediate: true, sourceLocale: "en" });
    await within(first.container).findByText(/language_not_downloaded/);
    const second = renderDescription({ immediate: true, sourceLocale: "en", resourceId: "skill:second" });
    await within(second.container).findByText(/language_not_downloaded/);
    mockInvoke.mockClear();
    fireEvent.click(within(second.container).getByRole("button", { name: /在设备上翻译/ }));
    expect(await within(second.container).findByText("重试成功")).toBeInTheDocument();
    expect(mockInvoke).toHaveBeenCalledTimes(1);
    expect(mockInvoke).toHaveBeenCalledWith("translate_skill_description_on_device", {
      request: expect.objectContaining({ resourceId: "skill:second" }),
    });
    expect(within(first.container).getByText("English legacy description")).toBeInTheDocument();
    expect(within(second.container).getByRole("button", { name: "使用 API 翻译" })).toBeEnabled();
  });

  it("API 요청 중에도 두 메뉴를 유지하고 선택한 카드에만 API 진행 상태를 표시한다", async () => {
    let finish: (value: unknown) => void = () => {};
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_cached_skill_description_translation") return Promise.resolve(null);
      if (command === "translate_skill_description_on_device") return Promise.reject("unsupported");
      if (command === "translate_skill_description_with_api") return new Promise(resolve => { finish = resolve; });
      return Promise.reject(new Error(command));
    });
    const first = renderDescription({ immediate: true, sourceLocale: "en" });
    await waitFor(() => expect(within(first.container).getByRole("button", { name: "使用 API 翻译" })).toBeEnabled());
    const second = renderDescription({ immediate: true, sourceLocale: "en", resourceId: "skill:second" });
    await waitFor(() => expect(within(second.container).getByRole("button", { name: "使用 API 翻译" })).toBeEnabled());
    mockInvoke.mockClear();
    fireEvent.click(within(first.container).getByRole("button", { name: "使用 API 翻译" }));
    expect(within(first.container).getByRole("button", { name: "使用 API 翻译" })).toBeInTheDocument();
    fireEvent.click(within(first.container).getByRole("button", { name: "确认" }));
    expect(within(first.container).getByRole("button", { name: /使用 API 翻译.*正在翻译/ })).toBeDisabled();
    expect(within(second.container).getByRole("button", { name: "使用 API 翻译" })).toBeEnabled();
    expect(mockInvoke).toHaveBeenCalledTimes(1);
    expect(mockInvoke).toHaveBeenCalledWith("translate_skill_description_with_api", {
      request: expect.objectContaining({ resourceId: "skill:demo" }),
    });
    await act(async () => finish({ translatedText: "API 结果", engine: "api", targetLocale: "zh", cached: false }));
    expect(within(first.container).getByRole("button", { name: /在设备上翻译/ })).toBeInTheDocument();
    expect(within(first.container).getByRole("button", { name: "使用 API 翻译" })).toBeEnabled();
  });

  it("기기 번역 중에는 API 메뉴에 번역 중 표시를 붙이지 않는다", async () => {
    let finish: (value: unknown) => void = () => {};
    mockInvoke.mockImplementation((command: string) => {
      if (command === "get_cached_skill_description_translation") return Promise.resolve(null);
      if (command === "translate_skill_description_on_device") return new Promise(resolve => { finish = resolve; });
      return Promise.reject(new Error(command));
    });
    renderDescription({ immediate: true, sourceLocale: "en" });
    await screen.findByRole("button", { name: /在设备上翻译.*正在翻译/ });
    expect(screen.getByRole("button", { name: "使用 API 翻译" })).toBeDisabled();
    await act(async () => finish({ translatedText: "设备结果", engine: "apple", targetLocale: "zh", cached: false }));
    expect(screen.getByRole("button", { name: "在设备上翻译" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "使用 API 翻译" })).toBeEnabled();
  });

});

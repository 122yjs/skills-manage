import { StrictMode } from "react";
import { act, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Route, Routes, useNavigate } from "react-router-dom";
import { AppShell } from "@/components/layout/AppShell";
import { usePlatformStore } from "@/stores/platformStore";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { useDiscoverStore } from "@/stores/discoverStore";
import { useDevToolSetupStore } from "@/stores/devToolSetupStore";
import { useSkillUsageStore } from "@/stores/skillUsageStore";
import { invoke, isTauriRuntime } from "@/lib/tauri";

vi.mock("@/lib/tauri", () => ({
  invoke: vi.fn().mockResolvedValue(0),
  isTauriRuntime: vi.fn(() => false),
}));

let triggerRescanInMock = false;

vi.mock("@/stores/platformStore", () => ({
  usePlatformStore: Object.assign(vi.fn(), { setState: vi.fn() }),
}));

vi.mock("@/stores/centralSkillsStore", () => ({
  useCentralSkillsStore: vi.fn(),
}));

vi.mock("@/stores/discoverStore", () => ({
  useDiscoverStore: vi.fn(),
}));

vi.mock("@/stores/devToolSetupStore", () => ({
  useDevToolSetupStore: vi.fn(),
}));

vi.mock("@/stores/skillUsageStore", () => ({
  useSkillUsageStore: vi.fn(),
}));

vi.mock("@/components/settings/DevToolSetupDialog", () => ({
  DevToolSetupDialog: () => <div data-testid="dev-tool-setup-dialog" />,
}));

vi.mock("@/components/layout/Sidebar", () => ({
  Sidebar: () => <div data-testid="sidebar" />,
}));

vi.mock("@/components/layout/TopBar", () => ({
  TopBar: ({ onSearchClick }: { onSearchClick: () => void }) => (
    <button type="button" onClick={onSearchClick}>
      open-search
    </button>
  ),
}));

vi.mock("@/components/layout/GlobalSearchDialog", () => ({
  GlobalSearchDialog: ({
    open,
    onAction,
  }: {
    open: boolean;
    onAction: (action: string) => void;
  }) =>
    open ? (
      triggerRescanInMock ? (
        <button type="button" onClick={() => onAction("rescan")}>
          trigger-rescan
        </button>
      ) : (
        <div data-testid="global-search-dialog" />
      )
    ) : null,
}));

const mockUsePlatformStore = vi.mocked(usePlatformStore);
const mockUseCentralSkillsStore = vi.mocked(useCentralSkillsStore);
const mockUseDiscoverStore = vi.mocked(useDiscoverStore);
const mockUseDevToolSetupStore = vi.mocked(useDevToolSetupStore);
const mockUseSkillUsageStore = vi.mocked(useSkillUsageStore);
const mockInvoke = vi.mocked(invoke);
const mockIsTauriRuntime = vi.mocked(isTauriRuntime);

let testNavigate: ReturnType<typeof useNavigate> | null = null;

function NavigationHarness() {
  testNavigate = useNavigate();
  return null;
}

function DummyPage({ label }: { label: string }) {
  return (
    <div className="flex h-full flex-col">
      <div>{label}</div>
      <div className="flex-1 overflow-auto p-4">
        <div style={{ height: 1600 }}>content</div>
      </div>
    </div>
  );
}

describe("AppShell", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockIsTauriRuntime.mockReturnValue(false);
    mockInvoke.mockResolvedValue(0);
    testNavigate = null;
    triggerRescanInMock = false;

    mockUseDevToolSetupStore.mockImplementation((selector?: unknown) => {
      const state = {
        status: "ready",
        completed: true,
        load: vi.fn().mockResolvedValue(undefined),
      };
      if (typeof selector === "function") return selector(state);
      return state;
    });

    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      const state = {
        initialize: vi.fn().mockResolvedValue(undefined),
        rescan: vi.fn().mockResolvedValue(undefined),
      };
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseSkillUsageStore.mockImplementation((selector?: unknown) => {
      const state = {
        loadUsageStatus: vi.fn().mockResolvedValue(undefined),
      };
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseCentralSkillsStore.mockImplementation((selector?: unknown) => {
      const state = {
        loadCentralSkills: vi.fn(),
      };
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseDiscoverStore.mockImplementation((selector?: unknown) => {
      const state = {
        refreshCounts: vi.fn(),
        rescanFromDisk: vi.fn(),
      };
      if (typeof selector === "function") return selector(state);
      return state;
    });
  });

  it.each(["idle", "loading", "ready", "error"])(
    "도구 설정이 %s 상태여도 실행당 한 번 스캔하고 완료 후 목록·디스크 스캔을 갱신한다",
    async (status) => {
      let finishScan!: () => void;
      const initialize = vi.fn(() => new Promise<void>((resolve) => {
        finishScan = resolve;
      }));
      const loadCentralSkills = vi.fn().mockResolvedValue(undefined);
      const loadUsageStatus = vi.fn().mockResolvedValue(undefined);
      const rescanFromDisk = vi.fn().mockResolvedValue(undefined);
      const setupState = { status, completed: false, load: vi.fn() };
      mockUseDevToolSetupStore.mockImplementation((selector?: unknown) => typeof selector === "function" ? selector(setupState) : setupState);
      mockUsePlatformStore.mockImplementation((selector?: unknown) => typeof selector === "function" ? selector({ initialize, rescan: vi.fn() }) : { initialize, rescan: vi.fn() });
      mockUseCentralSkillsStore.mockImplementation((selector?: unknown) => typeof selector === "function" ? selector({ loadCentralSkills }) : { loadCentralSkills });
      mockUseDiscoverStore.mockImplementation((selector?: unknown) => typeof selector === "function" ? selector({ rescanFromDisk }) : { rescanFromDisk });
      mockUseSkillUsageStore.mockImplementation((selector?: unknown) => typeof selector === "function" ? selector({ loadUsageStatus }) : { loadUsageStatus });

      const shell = () => (
        <StrictMode>
          <MemoryRouter>
            <NavigationHarness />
            <AppShell />
          </MemoryRouter>
        </StrictMode>
      );
      const { rerender, unmount } = render(shell());
      expect(initialize).toHaveBeenCalledTimes(1);
      // 플랫폼 스캔이 끝나기 전에는 어떤 후속 갱신도 시작하지 않는다.
      expect(loadCentralSkills).not.toHaveBeenCalled();
      expect(rescanFromDisk).not.toHaveBeenCalled();
      expect(loadUsageStatus).not.toHaveBeenCalled();

      await act(async () => finishScan());
      expect(loadCentralSkills).toHaveBeenCalledTimes(1);
      expect(rescanFromDisk).toHaveBeenCalledTimes(1);
      expect(loadUsageStatus).toHaveBeenCalledTimes(1);

      // StrictMode 재실행과 리렌더·라우트 변경에도 디스크 스캔은 중복되지 않는다.
      setupState.completed = true;
      rerender(shell());
      await act(async () => testNavigate?.("/central"));
      expect(initialize).toHaveBeenCalledTimes(1);
      expect(rescanFromDisk).toHaveBeenCalledTimes(1);

      // 다음 실행에서는 다시 스캔한다.
      unmount();
      render(shell());
      expect(initialize).toHaveBeenCalledTimes(2);
      await act(async () => finishScan());
      expect(rescanFromDisk).toHaveBeenCalledTimes(2);
    }
  );

  it("데스크톱 시작 시 연결된 원본을 확인하고 중앙 목록을 새로 읽는다", async () => {
    mockIsTauriRuntime.mockReturnValue(true);
    const loadCentralSkills = vi.fn().mockResolvedValue(undefined);
    mockUseCentralSkillsStore.mockImplementation((selector?: unknown) => {
      const state = { loadCentralSkills };
      return typeof selector === "function" ? selector(state) : state;
    });

    render(<MemoryRouter><AppShell /></MemoryRouter>);

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("check_linked_skill_origins"));
    await waitFor(() => expect(loadCentralSkills).toHaveBeenCalledTimes(2));
    expect(mockUsePlatformStore.setState).toHaveBeenCalled();
  });

  it("수동 재스캔 뒤 원본을 확인하고 플랫폼 링크를 갱신한다", async () => {
    mockIsTauriRuntime.mockReturnValue(true);
    triggerRescanInMock = true;
    render(<MemoryRouter><AppShell /></MemoryRouter>);
    await waitFor(() => expect(mockUsePlatformStore.setState).toHaveBeenCalledTimes(1));
    mockInvoke.mockClear();
    vi.mocked(mockUsePlatformStore.setState).mockClear();

    await act(async () => screen.getByRole("button", { name: /open-search/i }).click());
    await act(async () => screen.getByRole("button", { name: /trigger-rescan/i }).click());

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("check_linked_skill_origins"));
    await waitFor(() => expect(mockUsePlatformStore.setState).toHaveBeenCalledTimes(1));
  });

  it("resets shell scroll and keeps main non-scrollable when the route changes", async () => {
    render(
      <MemoryRouter initialEntries={["/a"]}>
        <NavigationHarness />
        <Routes>
          <Route path="/" element={<AppShell />}>
            <Route path="a" element={<DummyPage label="page-a" />} />
            <Route path="b" element={<DummyPage label="page-b" />} />
          </Route>
        </Routes>
      </MemoryRouter>
    );

    const main = document.querySelector("main");
    expect(main).not.toBeNull();
    if (!main) return;

    expect(main.className).toContain("overflow-hidden");
    expect(main.className).not.toContain("overflow-auto");

    (main as HTMLElement).scrollTop = 240;

    await act(async () => {
      testNavigate?.("/b");
    });

    await waitFor(() => {
      expect(screen.getByText("page-b")).toBeInTheDocument();
    });

    expect((main as HTMLElement).scrollTop).toBe(0);
  });

  it("routes the global rescan action to the platform, central, and discover disk scan stores", async () => {
    const mockRescan = vi.fn().mockResolvedValue(undefined);
    const mockLoadCentralSkills = vi.fn().mockResolvedValue(undefined);
    const mockRefreshDiscoverCounts = vi.fn().mockResolvedValue(undefined);
    const mockRescanDiscoverFromDisk = vi.fn().mockResolvedValue(undefined);
    triggerRescanInMock = true;

    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      const state = {
        initialize: vi.fn().mockResolvedValue(undefined),
        rescan: mockRescan,
      };
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseCentralSkillsStore.mockImplementation((selector?: unknown) => {
      const state = {
        loadCentralSkills: mockLoadCentralSkills,
      };
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseDiscoverStore.mockImplementation((selector?: unknown) => {
      const state = {
        refreshCounts: mockRefreshDiscoverCounts,
        rescanFromDisk: mockRescanDiscoverFromDisk,
      };
      if (typeof selector === "function") return selector(state);
      return state;
    });
    const mockLoadUsageStatus = vi.fn().mockResolvedValue(undefined);
    mockUseSkillUsageStore.mockImplementation((selector?: unknown) => {
      const state = { loadUsageStatus: mockLoadUsageStatus };
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseDevToolSetupStore.mockImplementation((selector?: unknown) => {
      const state = {
        status: "ready",
        completed: false,
        load: vi.fn().mockResolvedValue(undefined),
      };
      if (typeof selector === "function") return selector(state);
      return state;
    });

    render(
      <MemoryRouter initialEntries={["/a"]}>
        <Routes>
          <Route path="/" element={<AppShell />}>
            <Route path="a" element={<DummyPage label="page-a" />} />
          </Route>
        </Routes>
      </MemoryRouter>
    );
    await waitFor(() => expect(mockLoadCentralSkills).toHaveBeenCalledTimes(1));
    // 시작 시 디스크 스캔이 이미 1회 돌았으므로 수동 재스캔만 검증하도록 초기화한다.
    expect(mockRescanDiscoverFromDisk).toHaveBeenCalledTimes(1);
    mockLoadCentralSkills.mockClear();
    mockRescanDiscoverFromDisk.mockClear();
    mockLoadUsageStatus.mockClear();

    await act(async () => {
      screen.getByRole("button", { name: /open-search/i }).click();
    });

    await act(async () => {
      screen.getByRole("button", { name: /trigger-rescan/i }).click();
    });

    expect(mockRescan).toHaveBeenCalledTimes(1);
    expect(mockLoadCentralSkills).toHaveBeenCalledTimes(1);
    expect(mockRescanDiscoverFromDisk).toHaveBeenCalledTimes(1);
    expect(mockRefreshDiscoverCounts).not.toHaveBeenCalled();
    expect(mockLoadUsageStatus).toHaveBeenCalled();
  });

  it("waits for the platform rescan before refreshing central and rerunning discover from disk", async () => {
    let resolveRescan!: () => void;
    const rescanPromise = new Promise<void>((resolve) => {
      resolveRescan = () => resolve();
    });
    const mockRescan = vi.fn().mockReturnValue(rescanPromise);
    const mockLoadCentralSkills = vi.fn().mockResolvedValue(undefined);
    const mockRefreshDiscoverCounts = vi.fn().mockResolvedValue(undefined);
    const mockRescanDiscoverFromDisk = vi.fn().mockResolvedValue(undefined);
    triggerRescanInMock = true;

    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      const state = {
        initialize: vi.fn().mockResolvedValue(undefined),
        rescan: mockRescan,
      };
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseCentralSkillsStore.mockImplementation((selector?: unknown) => {
      const state = {
        loadCentralSkills: mockLoadCentralSkills,
      };
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseDiscoverStore.mockImplementation((selector?: unknown) => {
      const state = {
        refreshCounts: mockRefreshDiscoverCounts,
        rescanFromDisk: mockRescanDiscoverFromDisk,
      };
      if (typeof selector === "function") return selector(state);
      return state;
    });
    const mockLoadUsageStatus = vi.fn().mockResolvedValue(undefined);
    mockUseSkillUsageStore.mockImplementation((selector?: unknown) => {
      const state = { loadUsageStatus: mockLoadUsageStatus };
      if (typeof selector === "function") return selector(state);
      return state;
    });

    mockUseDevToolSetupStore.mockImplementation((selector?: unknown) => {
      const state = {
        status: "ready",
        completed: false,
        load: vi.fn().mockResolvedValue(undefined),
      };
      if (typeof selector === "function") return selector(state);
      return state;
    });

    render(
      <MemoryRouter initialEntries={["/a"]}>
        <Routes>
          <Route path="/" element={<AppShell />}>
            <Route path="a" element={<DummyPage label="page-a" />} />
          </Route>
        </Routes>
      </MemoryRouter>
    );
    await waitFor(() => expect(mockLoadCentralSkills).toHaveBeenCalledTimes(1));
    // 시작 시 디스크 스캔이 이미 1회 돌았으므로 수동 재스캔만 검증하도록 초기화한다.
    expect(mockRescanDiscoverFromDisk).toHaveBeenCalledTimes(1);
    mockLoadCentralSkills.mockClear();
    mockRescanDiscoverFromDisk.mockClear();
    mockLoadUsageStatus.mockClear();

    await act(async () => {
      screen.getByRole("button", { name: /open-search/i }).click();
    });

    await act(async () => {
      screen.getByRole("button", { name: /trigger-rescan/i }).click();
    });

    expect(mockRescan).toHaveBeenCalledTimes(1);
    expect(mockLoadCentralSkills).not.toHaveBeenCalled();
    expect(mockRescanDiscoverFromDisk).not.toHaveBeenCalled();
    expect(mockRefreshDiscoverCounts).not.toHaveBeenCalled();
    expect(mockLoadUsageStatus).not.toHaveBeenCalled();

    resolveRescan();

    await waitFor(() => {
      expect(mockLoadCentralSkills).toHaveBeenCalledTimes(1);
      expect(mockRescanDiscoverFromDisk).toHaveBeenCalledTimes(1);
      expect(mockLoadUsageStatus).toHaveBeenCalledTimes(1);
    });

    expect(mockRefreshDiscoverCounts).not.toHaveBeenCalled();
  });
});

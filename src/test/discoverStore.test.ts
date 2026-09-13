import { describe, it, expect, vi, beforeEach } from "vitest";
import * as tauriBridge from "@/lib/tauri";
import { ScanRoot, DiscoveredProject, DiscoverResult, DiscoverImportResult } from "../types";
import {
  OBSIDIAN_CROSS_AREA_FIXTURE,
  obsidianCrossAreaProjects,
} from "./fixtures/obsidianCrossAreaFixture";

// Mock Tauri core before importing the store
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

// Mock Tauri event (used for streaming scan progress)
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(vi.fn()),
}));

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useDiscoverStore } from "../stores/discoverStore";

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const mockScanRoots: ScanRoot[] = [
  { path: "/home/user/Documents", label: "Documents", exists: true, enabled: true },
  { path: "/home/user/projects", label: "projects", exists: true, enabled: true },
  { path: "/home/user/nonexistent", label: "nonexistent", exists: false, enabled: false },
];

const mockDiscoveredProjects: DiscoveredProject[] = [
  {
    project_path: "/home/user/projects/my-app",
    project_name: "my-app",
    skills: [
      {
        id: "claude-code__my-app__deploy",
        name: "deploy",
        description: "Deploy the app",
        file_path: "/home/user/projects/my-app/.claude/skills/deploy/SKILL.md",
        dir_path: "/home/user/projects/my-app/.claude/skills/deploy",
        platform_id: "claude-code",
        platform_name: "Claude Code",
        project_path: "/home/user/projects/my-app",
        project_name: "my-app",
        is_already_central: false,
      },
      {
        id: "cursor__my-app__review",
        name: "review",
        description: "Review code",
        file_path: "/home/user/projects/my-app/.cursor/skills/review/SKILL.md",
        dir_path: "/home/user/projects/my-app/.cursor/skills/review",
        platform_id: "cursor",
        platform_name: "Cursor",
        project_path: "/home/user/projects/my-app",
        project_name: "my-app",
        is_already_central: true,
      },
    ],
  },
];

const mockObsidianProjects: DiscoveredProject[] = obsidianCrossAreaProjects;

const mockImportResult: DiscoverImportResult = {
  skill_id: "deploy",
  target: "central",
};

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("discoverStore", () => {
  beforeEach(() => {
    useDiscoverStore.setState({
      scanRoots: [],
      isLoadingRoots: false,
      isScanning: false,
      scanProgress: 0,
      currentPath: "",
      skillsFoundSoFar: 0,
      projectsFoundSoFar: 0,
      discoveredProjects: [],
      totalSkillsFound: 0,
      lastScanAt: null,
      groupBy: "project",
      platformFilter: null,
      searchQuery: "",
      selectedSkillIds: new Set<string>(),
      error: null,
    });
    vi.clearAllMocks();
    // 동시성 가드 테스트가 mockImplementation을 남겨도 다음 테스트로 새지 않게 한다.
    vi.mocked(invoke).mockReset();
  });

  // ── Initial State ─────────────────────────────────────────────────────────

  it("has correct initial state", () => {
    const state = useDiscoverStore.getState();
    expect(state.scanRoots).toEqual([]);
    expect(state.isLoadingRoots).toBe(false);
    expect(state.isScanning).toBe(false);
    expect(state.scanProgress).toBe(0);
    expect(state.currentPath).toBe("");
    expect(state.skillsFoundSoFar).toBe(0);
    expect(state.projectsFoundSoFar).toBe(0);
    expect(state.discoveredProjects).toEqual([]);
    expect(state.totalSkillsFound).toBe(0);
    expect(state.lastScanAt).toBeNull();
    expect(state.groupBy).toBe("project");
    expect(state.platformFilter).toBeNull();
    expect(state.searchQuery).toBe("");
    expect(state.selectedSkillIds).toEqual(new Set());
    expect(state.error).toBeNull();
  });

  // ── loadScanRoots ─────────────────────────────────────────────────────────

  it("calls get_scan_roots (persisted) on loadScanRoots", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(mockScanRoots);

    await useDiscoverStore.getState().loadScanRoots();

    expect(invoke).toHaveBeenCalledWith("get_scan_roots");
  });

  it("populates scanRoots after successful loadScanRoots", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(mockScanRoots);

    await useDiscoverStore.getState().loadScanRoots();

    const state = useDiscoverStore.getState();
    expect(state.scanRoots).toEqual(mockScanRoots);
    expect(state.isLoadingRoots).toBe(false);
    expect(state.error).toBeNull();
  });

  it("sets error when loadScanRoots fails", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("DB error"));

    await useDiscoverStore.getState().loadScanRoots();

    const state = useDiscoverStore.getState();
    expect(state.error).toContain("DB error");
    expect(state.isLoadingRoots).toBe(false);
  });

  // ── setScanRootEnabled ───────────────────────────────────────────────────

  it("optimistically updates local state and persists to backend", async () => {
    useDiscoverStore.setState({ scanRoots: mockScanRoots });
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await useDiscoverStore.getState().setScanRootEnabled("/home/user/Documents", false);

    // Local state updated optimistically
    const state = useDiscoverStore.getState();
    const changed = state.scanRoots.find((r) => r.path === "/home/user/Documents");
    expect(changed?.enabled).toBe(false);

    // Backend called
    expect(invoke).toHaveBeenCalledWith("set_scan_root_enabled", {
      path: "/home/user/Documents",
      enabled: false,
    });
  });

  it("reverts local state when persist fails", async () => {
    useDiscoverStore.setState({ scanRoots: mockScanRoots });
    vi.mocked(invoke).mockRejectedValueOnce(new Error("persist failed"));

    await useDiscoverStore.getState().setScanRootEnabled("/home/user/Documents", false);

    const state = useDiscoverStore.getState();
    const changed = state.scanRoots.find((r) => r.path === "/home/user/Documents");
    expect(changed?.enabled).toBe(true); // reverted back to original
    expect(state.error).toContain("persist failed");
  });

  // ── startScan ─────────────────────────────────────────────────────────────

  it("calls start_project_scan and updates state", async () => {
    const result: DiscoverResult = {
      total_projects: 1,
      total_skills: 2,
      projects: mockDiscoveredProjects,
    };
    vi.mocked(invoke).mockResolvedValueOnce(result);

    useDiscoverStore.setState({ scanRoots: mockScanRoots });

    await useDiscoverStore.getState().startScan();

    expect(invoke).toHaveBeenCalledWith("start_project_scan", {
      roots: mockScanRoots,
    });

    const state = useDiscoverStore.getState();
    expect(state.isScanning).toBe(false);
    expect(state.scanProgress).toBe(100);
    expect(state.discoveredProjects).toEqual(mockDiscoveredProjects);
    expect(state.totalSkillsFound).toBe(2);
    expect(state.lastScanAt).not.toBeNull();
  });

  it("resets state when starting a scan", async () => {
    useDiscoverStore.setState({
      discoveredProjects: mockDiscoveredProjects,
      totalSkillsFound: 5,
      selectedSkillIds: new Set(["some-id"]),
    });

    const result: DiscoverResult = {
      total_projects: 0,
      total_skills: 0,
      projects: [],
    };
    vi.mocked(invoke).mockResolvedValueOnce(result);
    useDiscoverStore.setState({ scanRoots: mockScanRoots });

    await useDiscoverStore.getState().startScan();

    // Verify that state was reset before the scan result was set
    // (We check the final state after scan completes)
    const state = useDiscoverStore.getState();
    expect(state.selectedSkillIds).toEqual(new Set());
  });

  it("sets error when startScan fails", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("scan failed"));
    useDiscoverStore.setState({ scanRoots: mockScanRoots });

    await useDiscoverStore.getState().startScan();

    const state = useDiscoverStore.getState();
    expect(state.isScanning).toBe(false);
    expect(state.error).toContain("scan failed");
  });

  it("rescanFromDisk reloads persisted scan roots and reruns the real project scan", async () => {
    const result: DiscoverResult = {
      total_projects: 1,
      total_skills: 2,
      projects: mockDiscoveredProjects,
    };

    vi.mocked(invoke)
      .mockResolvedValueOnce(mockScanRoots)
      .mockResolvedValueOnce(result);

    await useDiscoverStore.getState().rescanFromDisk();

    expect(invoke).toHaveBeenNthCalledWith(1, "get_scan_roots");
    expect(invoke).toHaveBeenNthCalledWith(2, "start_project_scan", {
      roots: mockScanRoots,
    });
    expect(invoke).not.toHaveBeenCalledWith("get_discovered_skills");

    const state = useDiscoverStore.getState();
    expect(state.scanRoots).toEqual(mockScanRoots);
    expect(state.discoveredProjects).toEqual(mockDiscoveredProjects);
    expect(state.totalSkillsFound).toBe(2);
    expect(state.lastScanAt).not.toBeNull();
  });

  it("rescanFromDisk surfaces root-loading failures without starting a stale cached refresh", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("root load failed"));

    await useDiscoverStore.getState().rescanFromDisk();

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("get_scan_roots");

    const state = useDiscoverStore.getState();
    expect(state.isLoadingRoots).toBe(false);
    expect(state.isScanning).toBe(false);
    expect(state.error).toContain("root load failed");
  });

  // ── stopScan ──────────────────────────────────────────────────────────────

  it("calls stop_project_scan on stopScan", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await useDiscoverStore.getState().stopScan();

    expect(invoke).toHaveBeenCalledWith("stop_project_scan");
  });

  it("sets isScanning to false on stopScan", async () => {
    useDiscoverStore.setState({ isScanning: true });
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await useDiscoverStore.getState().stopScan();

    expect(useDiscoverStore.getState().isScanning).toBe(false);
  });

  // ── loadDiscoveredSkills ──────────────────────────────────────────────────

  it("calls get_discovered_skills on loadDiscoveredSkills", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(mockDiscoveredProjects);

    await useDiscoverStore.getState().loadDiscoveredSkills();

    expect(invoke).toHaveBeenCalledWith("get_discovered_skills");
  });

  it("populates discoveredProjects after successful loadDiscoveredSkills", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(mockDiscoveredProjects);

    await useDiscoverStore.getState().loadDiscoveredSkills();

    const state = useDiscoverStore.getState();
    expect(state.discoveredProjects).toEqual(mockDiscoveredProjects);
    expect(state.totalSkillsFound).toBe(2);
  });

  it("sets error when loadDiscoveredSkills fails", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("load failed"));

    await useDiscoverStore.getState().loadDiscoveredSkills();

    const state = useDiscoverStore.getState();
    expect(state.error).toContain("load failed");
  });

  it("returns deterministic browser fixture discover results when Tauri runtime is unavailable", async () => {
    const isTauriSpy = vi.spyOn(tauriBridge, "isTauriRuntime").mockReturnValue(false);

    await useDiscoverStore.getState().loadDiscoveredSkills();

    expect(invoke).not.toHaveBeenCalled();
    expect(useDiscoverStore.getState().discoveredProjects).toEqual([
      expect.objectContaining({
        project_name: "Fixture Project",
        project_path: "/Users/fixture/project",
        skills: [expect.objectContaining({ id: "fixture-central-skill", is_already_central: true })],
      }),
    ]);
    expect(useDiscoverStore.getState().totalSkillsFound).toBe(1);

    isTauriSpy.mockRestore();
  });

  // ── importToCentral ──────────────────────────────────────────────────────

  it("calls import_discovered_skill_to_central with correct args", async () => {
    useDiscoverStore.setState({ discoveredProjects: mockDiscoveredProjects });
    vi.mocked(invoke).mockResolvedValueOnce(mockImportResult);

    await useDiscoverStore.getState().importToCentral("claude-code__my-app__deploy");

    expect(invoke).toHaveBeenCalledWith("import_discovered_skill_to_central", {
      discoveredSkillId: "claude-code__my-app__deploy",
    });
  });

  it("removes imported skill from discoveredProjects", async () => {
    useDiscoverStore.setState({ discoveredProjects: mockDiscoveredProjects });
    vi.mocked(invoke).mockResolvedValueOnce(mockImportResult);

    await useDiscoverStore.getState().importToCentral("claude-code__my-app__deploy");

    const state = useDiscoverStore.getState();
    expect(state.discoveredProjects[0].skills).toHaveLength(1);
    expect(state.discoveredProjects[0].skills[0].id).toBe("cursor__my-app__review");
    expect(state.totalSkillsFound).toBe(1);
  });

  it("removes imported skill from selectedSkillIds", async () => {
    useDiscoverStore.setState({
      discoveredProjects: mockDiscoveredProjects,
      selectedSkillIds: new Set(["claude-code__my-app__deploy"]),
    });
    vi.mocked(invoke).mockResolvedValueOnce(mockImportResult);

    await useDiscoverStore.getState().importToCentral("claude-code__my-app__deploy");

    const state = useDiscoverStore.getState();
    expect(state.selectedSkillIds.has("claude-code__my-app__deploy")).toBe(false);
  });

  it("updates Obsidian vault counts by removing an imported central skill and empty vault rows", async () => {
    useDiscoverStore.setState({
      discoveredProjects: mockObsidianProjects,
      totalSkillsFound: 1,
      selectedSkillIds: new Set([OBSIDIAN_CROSS_AREA_FIXTURE.skillId]),
    });
    vi.mocked(invoke).mockResolvedValueOnce({
      skill_id: OBSIDIAN_CROSS_AREA_FIXTURE.skillDirName,
      target: "central",
    });

    await useDiscoverStore.getState().importToCentral(OBSIDIAN_CROSS_AREA_FIXTURE.skillId);

    expect(invoke).toHaveBeenCalledWith("import_discovered_skill_to_central", {
      discoveredSkillId: OBSIDIAN_CROSS_AREA_FIXTURE.skillId,
    });
    expect(useDiscoverStore.getState().discoveredProjects).toEqual([]);
    expect(useDiscoverStore.getState().totalSkillsFound).toBe(0);
    expect(useDiscoverStore.getState().selectedSkillIds).toEqual(new Set());
  });

  it("sets error when importToCentral fails", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("import failed"));

    await expect(
      useDiscoverStore.getState().importToCentral("nonexistent-id")
    ).rejects.toThrow();

    const state = useDiscoverStore.getState();
    expect(state.error).toContain("import failed");
  });

  // ── importToPlatform ──────────────────────────────────────────────────────

  it("calls import_discovered_skill_to_platform with correct args", async () => {
    useDiscoverStore.setState({ discoveredProjects: mockDiscoveredProjects });
    const platformResult: DiscoverImportResult = { skill_id: "deploy", target: "claude-code" };
    vi.mocked(invoke).mockResolvedValueOnce(platformResult);

    await useDiscoverStore.getState().importToPlatform("claude-code__my-app__deploy", "claude-code");

    expect(invoke).toHaveBeenCalledWith("import_discovered_skill_to_platform", {
      discoveredSkillId: "claude-code__my-app__deploy",
      agentId: "claude-code",
    });
  });

  it("keeps Obsidian discovered rows after installing to a real platform", async () => {
    useDiscoverStore.setState({
      discoveredProjects: mockObsidianProjects,
      totalSkillsFound: 1,
    });
    const platformResult: DiscoverImportResult = {
      skill_id: OBSIDIAN_CROSS_AREA_FIXTURE.skillDirName,
      target: OBSIDIAN_CROSS_AREA_FIXTURE.installAgentId,
    };
    vi.mocked(invoke).mockResolvedValueOnce(platformResult);

    await useDiscoverStore.getState().importToPlatform(
      OBSIDIAN_CROSS_AREA_FIXTURE.skillId,
      OBSIDIAN_CROSS_AREA_FIXTURE.installAgentId
    );

    expect(useDiscoverStore.getState().discoveredProjects).toEqual(mockObsidianProjects);
    expect(useDiscoverStore.getState().totalSkillsFound).toBe(1);
  });

  it("forwards the selected install method for Obsidian platform installs", async () => {
    useDiscoverStore.setState({
      discoveredProjects: mockObsidianProjects,
      totalSkillsFound: 1,
    });
    const platformResult: DiscoverImportResult = {
      skill_id: OBSIDIAN_CROSS_AREA_FIXTURE.skillDirName,
      target: "cursor",
    };
    vi.mocked(invoke).mockResolvedValueOnce(platformResult);

    await useDiscoverStore.getState().importToPlatform(
      OBSIDIAN_CROSS_AREA_FIXTURE.skillId,
      "cursor",
      OBSIDIAN_CROSS_AREA_FIXTURE.copyInstallMethod
    );

    expect(invoke).toHaveBeenCalledWith("import_discovered_skill_to_platform", {
      discoveredSkillId: OBSIDIAN_CROSS_AREA_FIXTURE.skillId,
      agentId: "cursor",
      method: OBSIDIAN_CROSS_AREA_FIXTURE.copyInstallMethod,
    });
    expect(useDiscoverStore.getState().discoveredProjects).toEqual(mockObsidianProjects);
    expect(useDiscoverStore.getState().totalSkillsFound).toBe(1);
  });

  it("rejects Obsidian as a platform install target without invoking the backend", async () => {
    useDiscoverStore.setState({ discoveredProjects: mockObsidianProjects });

    await expect(
      useDiscoverStore.getState().importToPlatform(
        OBSIDIAN_CROSS_AREA_FIXTURE.skillId,
        OBSIDIAN_CROSS_AREA_FIXTURE.platformId
      )
    ).rejects.toThrow(/Obsidian/);

    expect(invoke).not.toHaveBeenCalled();
    expect(useDiscoverStore.getState().error).toContain("Obsidian");
  });

  it("refreshCounts replaces Obsidian vault counts with reconciled cached rows", async () => {
    useDiscoverStore.setState({
      discoveredProjects: mockObsidianProjects,
      totalSkillsFound: 1,
    });
    const reconciledProjects: DiscoveredProject[] = [];
    vi.mocked(invoke).mockResolvedValueOnce(reconciledProjects);

    await useDiscoverStore.getState().refreshCounts();

    expect(useDiscoverStore.getState().discoveredProjects).toEqual(reconciledProjects);
    expect(useDiscoverStore.getState().totalSkillsFound).toBe(0);
  });

  // ── clearResults ──────────────────────────────────────────────────────────

  it("calls clear_discovered_skills and resets state", async () => {
    useDiscoverStore.setState({
      discoveredProjects: mockDiscoveredProjects,
      totalSkillsFound: 2,
      selectedSkillIds: new Set(["some-id"]),
    });
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await useDiscoverStore.getState().clearResults();

    expect(invoke).toHaveBeenCalledWith("clear_discovered_skills");

    const state = useDiscoverStore.getState();
    expect(state.discoveredProjects).toEqual([]);
    expect(state.totalSkillsFound).toBe(0);
    expect(state.lastScanAt).toBeNull();
    expect(state.selectedSkillIds).toEqual(new Set());
  });

  // ── Grouping / Filtering ──────────────────────────────────────────────────

  it("setGroupBy updates groupBy state", () => {
    useDiscoverStore.getState().setGroupBy("platform");
    expect(useDiscoverStore.getState().groupBy).toBe("platform");

    useDiscoverStore.getState().setGroupBy("skill");
    expect(useDiscoverStore.getState().groupBy).toBe("skill");
  });

  it("setPlatformFilter updates platformFilter state", () => {
    useDiscoverStore.getState().setPlatformFilter("claude-code");
    expect(useDiscoverStore.getState().platformFilter).toBe("claude-code");

    useDiscoverStore.getState().setPlatformFilter(null);
    expect(useDiscoverStore.getState().platformFilter).toBeNull();
  });

  it("setSearchQuery updates searchQuery state", () => {
    useDiscoverStore.getState().setSearchQuery("deploy");
    expect(useDiscoverStore.getState().searchQuery).toBe("deploy");
  });

  // ── Selection ──────────────────────────────────────────────────────────────

  it("toggleSkillSelection adds and removes skill IDs", () => {
    useDiscoverStore.getState().toggleSkillSelection("skill-1");
    expect(useDiscoverStore.getState().selectedSkillIds.has("skill-1")).toBe(true);

    useDiscoverStore.getState().toggleSkillSelection("skill-1");
    expect(useDiscoverStore.getState().selectedSkillIds.has("skill-1")).toBe(false);
  });

  it("selectAllVisible adds all given IDs", () => {
    useDiscoverStore.getState().selectAllVisible(["skill-1", "skill-2", "skill-3"]);
    const state = useDiscoverStore.getState();
    expect(state.selectedSkillIds.has("skill-1")).toBe(true);
    expect(state.selectedSkillIds.has("skill-2")).toBe(true);
    expect(state.selectedSkillIds.has("skill-3")).toBe(true);
  });

  it("clearSelection removes all selected IDs", () => {
    useDiscoverStore.setState({ selectedSkillIds: new Set(["skill-1", "skill-2"]) });
    useDiscoverStore.getState().clearSelection();
    expect(useDiscoverStore.getState().selectedSkillIds).toEqual(new Set());
  });

  // ── Error ──────────────────────────────────────────────────────────────────

  it("clearError resets error to null", () => {
    useDiscoverStore.setState({ error: "something went wrong" });
    useDiscoverStore.getState().clearError();
    expect(useDiscoverStore.getState().error).toBeNull();
  });

  // ── 동시성 가드 ────────────────────────────────────────────────────────────

  it("늦게 도착한 get_discovered_skills 스냅샷이 더 새로운 디스크 스캔 결과를 덮지 않는다", async () => {
    const staleProjects: DiscoveredProject[] = [
      {
        project_path: "/stale/project",
        project_name: "stale",
        skills: [
          {
            id: "cursor__stale__old",
            name: "old",
            description: "stale row",
            file_path: "/stale/project/.cursor/skills/old/SKILL.md",
            dir_path: "/stale/project/.cursor/skills/old",
            platform_id: "cursor",
            platform_name: "Cursor",
            project_path: "/stale/project",
            project_name: "stale",
            is_already_central: false,
          },
        ],
      },
    ];
    const scanResult: DiscoverResult = {
      total_projects: 1,
      total_skills: 2,
      projects: mockDiscoveredProjects,
    };

    let resolveLoad!: (projects: DiscoveredProject[]) => void;
    const pendingLoad = new Promise<DiscoveredProject[]>((resolve) => {
      resolveLoad = resolve;
    });

    vi.mocked(invoke).mockImplementation((command: string) => {
      if (command === "get_discovered_skills") return pendingLoad;
      if (command === "get_scan_roots") return Promise.resolve(mockScanRoots);
      if (command === "start_project_scan") return Promise.resolve(scanResult);
      return Promise.resolve(undefined);
    });

    // Sidebar/DiscoverView가 마운트에서 시작한 DB 조회가 아직 진행 중이다.
    const load = useDiscoverStore.getState().loadDiscoveredSkills();
    // 그 사이 AppShell의 디스크 재스캔이 먼저 끝나 더 새로운 결과를 반영한다.
    await useDiscoverStore.getState().rescanFromDisk();
    expect(useDiscoverStore.getState().discoveredProjects).toEqual(mockDiscoveredProjects);

    // 뒤늦게 도착한 낡은 DB 스냅샷은 최신 디스크 결과를 덮지 않는다.
    resolveLoad(staleProjects);
    await load;

    const state = useDiscoverStore.getState();
    expect(state.discoveredProjects).toEqual(mockDiscoveredProjects);
    expect(state.totalSkillsFound).toBe(2);
  });

  it("자동 rescanFromDisk의 루트 로딩 중 수동 startScan은 중복 스캔을 시작하지 않는다", async () => {
    const scanResult: DiscoverResult = {
      total_projects: 1,
      total_skills: 2,
      projects: mockDiscoveredProjects,
    };

    let resolveRoots!: (roots: ScanRoot[]) => void;
    const pendingRoots = new Promise<ScanRoot[]>((resolve) => {
      resolveRoots = resolve;
    });

    vi.mocked(invoke).mockImplementation((command: string) => {
      if (command === "get_scan_roots") return pendingRoots;
      if (command === "start_project_scan") return Promise.resolve(scanResult);
      return Promise.resolve(undefined);
    });
    useDiscoverStore.setState({ scanRoots: mockScanRoots });

    const scanCalls = () =>
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "start_project_scan").length;

    const autoRescan = useDiscoverStore.getState().rescanFromDisk();
    await useDiscoverStore.getState().startScan();

    // 루트를 불러오는 중 들어온 수동 스캔은 백엔드 스캔을 새로 시작하지 않는다.
    expect(scanCalls()).toBe(0);

    resolveRoots(mockScanRoots);
    await autoRescan;

    expect(scanCalls()).toBe(1);
    expect(useDiscoverStore.getState().discoveredProjects).toEqual(mockDiscoveredProjects);
  });

  it("디스크 스캔이 진행 중일 때 수동 rescanFromDisk는 중복 시작되지 않는다", async () => {
    const scanResult: DiscoverResult = {
      total_projects: 1,
      total_skills: 2,
      projects: mockDiscoveredProjects,
    };

    let resolveScan!: (result: DiscoverResult) => void;
    const pendingScan = new Promise<DiscoverResult>((resolve) => {
      resolveScan = resolve;
    });

    vi.mocked(invoke).mockImplementation((command: string) => {
      if (command === "get_scan_roots") return Promise.resolve(mockScanRoots);
      if (command === "start_project_scan") return pendingScan;
      return Promise.resolve(undefined);
    });

    const scanCalls = () =>
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "start_project_scan").length;

    const autoRescan = useDiscoverStore.getState().rescanFromDisk();
    await vi.waitFor(() => expect(scanCalls()).toBe(1));

    // 스캔이 끝나기 전에 들어온 수동 재스캔은 아무것도 시작하지 않는다.
    await useDiscoverStore.getState().rescanFromDisk();
    expect(scanCalls()).toBe(1);

    resolveScan(scanResult);
    await autoRescan;

    expect(useDiscoverStore.getState().discoveredProjects).toEqual(mockDiscoveredProjects);
    expect(useDiscoverStore.getState().isScanning).toBe(false);
  });

  it("중지 후에는 이전 스캔이 끝날 때까지 새 스캔을 시작하지 않고, 끝난 뒤 재시도할 수 있다", async () => {
    const stoppedResult: DiscoverResult = {
      total_projects: 1,
      total_skills: 1,
      projects: mockDiscoveredProjects,
    };
    const retryResult: DiscoverResult = {
      total_projects: 1,
      total_skills: 2,
      projects: mockDiscoveredProjects,
    };

    const pendingScans: Array<(result: DiscoverResult) => void> = [];
    vi.mocked(invoke).mockImplementation((command: string) => {
      if (command === "start_project_scan") {
        return new Promise<DiscoverResult>((resolve) => {
          pendingScans.push(resolve);
        });
      }
      return Promise.resolve(undefined);
    });
    useDiscoverStore.setState({ scanRoots: mockScanRoots });

    const stoppedScan = useDiscoverStore.getState().startScan();
    await vi.waitFor(() => expect(pendingScans).toHaveLength(1));

    await useDiscoverStore.getState().stopScan();
    expect(useDiscoverStore.getState().isScanning).toBe(false);

    // 중지 요청만으로는 스캔이 끝나지 않았으므로 새 IPC를 시작하지 않는다.
    // (백엔드 start_project_scan이 SCAN_CANCEL을 리셋해 이전 스캔과 겹치는 것을 막는다.)
    void useDiscoverStore.getState().startScan();
    await vi.waitFor(() => expect(pendingScans).toHaveLength(1));
    await vi.waitFor(() => expect(useDiscoverStore.getState().isScanning).toBe(false));

    // 실제 스캔이 끝나면 중지된 스캔의 부분 결과는 그대로 반영되고 가드가 풀린다.
    pendingScans[0](stoppedResult);
    await stoppedScan;
    const stoppedState = useDiscoverStore.getState();
    expect(stoppedState.isScanning).toBe(false);
    expect(stoppedState.discoveredProjects).toEqual(mockDiscoveredProjects);

    // 종료 후 재시도는 새 스캔을 시작한다.
    const retryScan = useDiscoverStore.getState().startScan();
    await vi.waitFor(() => expect(pendingScans).toHaveLength(2));
    pendingScans[1](retryResult);
    await retryScan;

    expect(useDiscoverStore.getState().discoveredProjects).toEqual(mockDiscoveredProjects);
    expect(useDiscoverStore.getState().totalSkillsFound).toBe(2);
  });

  it("루트 로딩 중 중지되면 실제 스캔을 시작하지 않고 상태와 가드를 정리한다", async () => {
    let resolveStop!: () => void;
    const pendingStop = new Promise<void>((resolve) => {
      resolveStop = resolve;
    });
    let resolveRoots!: (roots: ScanRoot[]) => void;
    const pendingRoots = new Promise<ScanRoot[]>((resolve) => {
      resolveRoots = resolve;
    });
    const scanResult: DiscoverResult = {
      total_projects: 1,
      total_skills: 2,
      projects: mockDiscoveredProjects,
    };

    vi.mocked(invoke).mockImplementation((command: string) => {
      if (command === "get_scan_roots") return pendingRoots;
      if (command === "stop_project_scan") return pendingStop;
      if (command === "start_project_scan") return Promise.resolve(scanResult);
      return Promise.resolve(undefined);
    });

    const rescan = useDiscoverStore.getState().rescanFromDisk();
    await vi.waitFor(() => expect(useDiscoverStore.getState().isLoadingRoots).toBe(true));

    const stop = useDiscoverStore.getState().stopScan();
    resolveRoots(mockScanRoots);
    await rescan;

    // 중지된 스캔은 백엔드 스캔을 시작하지 않고 로딩 표시만 정리한다.
    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "start_project_scan")
    ).toHaveLength(0);
    expect(useDiscoverStore.getState().isLoadingRoots).toBe(false);
    expect(useDiscoverStore.getState().isScanning).toBe(false);

    // 중지 응답이 루트 로딩보다 늦게 도착해도 스캔은 시작되지 않는다.
    resolveStop();
    await stop;

    // 가드가 풀렸으므로 재시도는 실제 스캔을 시작한다.
    useDiscoverStore.setState({ scanRoots: mockScanRoots });
    await useDiscoverStore.getState().startScan();

    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "start_project_scan")
    ).toHaveLength(1);
    expect(useDiscoverStore.getState().discoveredProjects).toEqual(mockDiscoveredProjects);
  });

  it("리스너 준비 중 중지되면 실제 스캔을 시작하지 않고 재시도할 수 있다", async () => {
    let releaseListen!: () => void;
    const listenGate = new Promise<void>((resolve) => {
      releaseListen = resolve;
    });
    const gatedListen = () => listenGate.then(() => vi.fn());
    vi.mocked(listen)
      .mockImplementationOnce(gatedListen)
      .mockImplementationOnce(gatedListen)
      .mockImplementationOnce(gatedListen);

    const scanResult: DiscoverResult = {
      total_projects: 1,
      total_skills: 2,
      projects: mockDiscoveredProjects,
    };
    vi.mocked(invoke).mockImplementation((command: string) => {
      if (command === "start_project_scan") return Promise.resolve(scanResult);
      return Promise.resolve(undefined);
    });
    useDiscoverStore.setState({ scanRoots: mockScanRoots });

    const scan = useDiscoverStore.getState().startScan();
    await vi.waitFor(() => expect(useDiscoverStore.getState().isScanning).toBe(true));

    // 리스너가 준비되기 전에 중지한다.
    await useDiscoverStore.getState().stopScan();
    releaseListen();
    await scan;

    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "start_project_scan")
    ).toHaveLength(0);
    expect(useDiscoverStore.getState().isScanning).toBe(false);

    // 가드가 풀렸으므로 재시도는 실제 스캔을 시작한다.
    await useDiscoverStore.getState().startScan();

    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "start_project_scan")
    ).toHaveLength(1);
    expect(useDiscoverStore.getState().discoveredProjects).toEqual(mockDiscoveredProjects);
  });

  it("스캔이 시작되면 진행 중이던 get_discovered_skills 응답과 오류는 버려진다", async () => {
    const staleProjects: DiscoveredProject[] = [
      {
        project_path: "/stale/project",
        project_name: "stale",
        skills: [
          {
            id: "cursor__stale__old",
            name: "old",
            description: "stale row",
            file_path: "/stale/project/.cursor/skills/old/SKILL.md",
            dir_path: "/stale/project/.cursor/skills/old",
            platform_id: "cursor",
            platform_name: "Cursor",
            project_path: "/stale/project",
            project_name: "stale",
            is_already_central: false,
          },
        ],
      },
    ];
    const scanResult: DiscoverResult = {
      total_projects: 1,
      total_skills: 2,
      projects: mockDiscoveredProjects,
    };

    let resolveLoad!: (projects: DiscoveredProject[]) => void;
    const pendingLoad = new Promise<DiscoveredProject[]>((resolve) => {
      resolveLoad = resolve;
    });
    let rejectLoad!: (err: Error) => void;
    const pendingError = new Promise<DiscoveredProject[]>((_, reject) => {
      rejectLoad = reject;
    });
    let resolveScan!: (result: DiscoverResult) => void;
    const pendingScan = new Promise<DiscoverResult>((resolve) => {
      resolveScan = resolve;
    });

    let loadCalls = 0;
    vi.mocked(invoke).mockImplementation((command: string) => {
      if (command === "get_discovered_skills") {
        loadCalls += 1;
        return loadCalls === 1 ? pendingLoad : pendingError;
      }
      if (command === "start_project_scan") return pendingScan;
      return Promise.resolve(undefined);
    });
    useDiscoverStore.setState({ scanRoots: mockScanRoots });

    // 스캔 시작 전에 시작된 DB 조회 두 건이 아직 진행 중이다.
    const load = useDiscoverStore.getState().loadDiscoveredSkills();
    const refresh = useDiscoverStore.getState().refreshCounts();

    const scan = useDiscoverStore.getState().startScan();
    await vi.waitFor(() =>
      expect(
        vi.mocked(invoke).mock.calls.filter(([command]) => command === "start_project_scan")
      ).toHaveLength(1)
    );

    // 스캔 도중 늦게 도착한 DB 스냅샷과 조회 오류는 스트리밍 결과를 덮지 않는다.
    resolveLoad(staleProjects);
    await load;
    rejectLoad(new Error("stale load failed"));
    await refresh;

    const midScanState = useDiscoverStore.getState();
    expect(midScanState.discoveredProjects).toEqual([]);
    expect(midScanState.totalSkillsFound).toBe(0);
    expect(midScanState.error).toBeNull();

    resolveScan(scanResult);
    await scan;

    const state = useDiscoverStore.getState();
    expect(state.discoveredProjects).toEqual(mockDiscoveredProjects);
    expect(state.totalSkillsFound).toBe(2);
    expect(state.error).toBeNull();
  });

  it("스캔 진행 중에는 DB 조회를 시작하지 않고, 스캔이 끝나면 다시 조회한다", async () => {
    const scanResult: DiscoverResult = {
      total_projects: 1,
      total_skills: 2,
      projects: mockDiscoveredProjects,
    };

    let resolveScan!: (result: DiscoverResult) => void;
    const pendingScan = new Promise<DiscoverResult>((resolve) => {
      resolveScan = resolve;
    });

    vi.mocked(invoke).mockImplementation((command: string) => {
      if (command === "start_project_scan") return pendingScan;
      return Promise.resolve(undefined);
    });
    useDiscoverStore.setState({ scanRoots: mockScanRoots });

    const scan = useDiscoverStore.getState().startScan();
    await vi.waitFor(() =>
      expect(
        vi.mocked(invoke).mock.calls.filter(([command]) => command === "start_project_scan")
      ).toHaveLength(1)
    );

    // 스캔 진행 중에는 스트리밍 목록을 DB 스냅샷으로 덮지 않기 위해 조회 자체를 생략한다.
    await useDiscoverStore.getState().loadDiscoveredSkills();
    await useDiscoverStore.getState().refreshCounts();
    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "get_discovered_skills")
    ).toHaveLength(0);

    resolveScan(scanResult);
    await scan;
    expect(useDiscoverStore.getState().discoveredProjects).toEqual(mockDiscoveredProjects);

    // 스캔이 끝난 뒤의 조회는 정상 반영된다.
    vi.mocked(invoke).mockImplementation((command: string) => {
      if (command === "get_discovered_skills") return Promise.resolve(mockDiscoveredProjects);
      return Promise.resolve(undefined);
    });
    await useDiscoverStore.getState().loadDiscoveredSkills();

    expect(
      vi.mocked(invoke).mock.calls.filter(([command]) => command === "get_discovered_skills")
    ).toHaveLength(1);
    expect(useDiscoverStore.getState().discoveredProjects).toEqual(mockDiscoveredProjects);
    expect(useDiscoverStore.getState().totalSkillsFound).toBe(2);
  });

  it("이벤트 리스너 초기화가 실패하면 스캔 중 상태가 고착되지 않고 재시도할 수 있다", async () => {
    vi.mocked(listen).mockRejectedValueOnce(new Error("listen failed"));
    useDiscoverStore.setState({ scanRoots: mockScanRoots });

    await useDiscoverStore.getState().startScan();

    let state = useDiscoverStore.getState();
    expect(state.isScanning).toBe(false);
    expect(state.error).toContain("listen failed");

    const result: DiscoverResult = {
      total_projects: 1,
      total_skills: 2,
      projects: mockDiscoveredProjects,
    };
    vi.mocked(invoke).mockResolvedValueOnce(result);

    await useDiscoverStore.getState().startScan();

    state = useDiscoverStore.getState();
    expect(state.discoveredProjects).toEqual(mockDiscoveredProjects);
    expect(state.isScanning).toBe(false);
    expect(state.error).toBeNull();
  });
});

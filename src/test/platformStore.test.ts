import { describe, it, expect, vi, beforeEach } from "vitest";
import { AgentWithStatus, ScanResult } from "../types";

// Mock Tauri core before importing the store
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { usePlatformStore } from "../stores/platformStore";

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const mockAgents: AgentWithStatus[] = [
  {
    id: "claude-code",
    display_name: "Claude Code",
    category: "coding",
    global_skills_dir: "~/.claude/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "central",
    display_name: "Central Skills",
    category: "central",
    global_skills_dir: "~/.agents/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
];

const mockScanResult: ScanResult = {
  total_skills: 8,
  agents_scanned: 2,
  skills_by_agent: {
    "claude-code": 5,
    central: 3,
  },
};

const bulkAgents: AgentWithStatus[] = [
  ...mockAgents,
  ...[
    { id: "cursor", category: "coding", is_enabled: false, is_detected: false },
    { id: "custom-tool", category: "other", is_enabled: true, is_builtin: false },
    { id: "lobster-tool", category: "lobster", is_enabled: false },
    { id: "universal", category: "shared" },
    { id: "obsidian", category: "other" },
    { id: "custom-shared", category: "shared", is_enabled: false },
    { id: "custom-central", category: "central" },
  ].map((overrides) => ({ ...mockAgents[0], ...overrides })),
];
const toggleableIds = ["claude-code", "cursor", "custom-tool", "lobster-tool"];

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("platformStore", () => {
  beforeEach(() => {
    // Reset store to initial state before each test
    usePlatformStore.setState({
      agents: [],
      skillsByAgent: {},
      isLoading: false,
      isRefreshing: false,
      updatingAgentIds: {},
      scanGeneration: 0,
      error: null,
    });
    vi.clearAllMocks();
  });

  // ── Initial State ─────────────────────────────────────────────────────────

  it("has correct initial state", () => {
    const state = usePlatformStore.getState();
    expect(state.agents).toEqual([]);
    expect(state.skillsByAgent).toEqual({});
    expect(state.isLoading).toBe(false);
    expect(state.isRefreshing).toBe(false);
    expect(state.updatingAgentIds).toEqual({});
    expect(state.scanGeneration).toBe(0);
    expect(state.error).toBeNull();
  });

  // ── initialize ────────────────────────────────────────────────────────────

  it("sets isLoading to true while initializing", async () => {
    let resolveAgents!: (value: AgentWithStatus[]) => void;
    let resolveScan!: (value: ScanResult) => void;

    vi.mocked(invoke)
      .mockReturnValueOnce(
        new Promise<AgentWithStatus[]>((r) => (resolveAgents = r))
      )
      .mockReturnValueOnce(new Promise<ScanResult>((r) => (resolveScan = r)));

    const initPromise = usePlatformStore.getState().initialize();

    // isLoading should be true while the calls are pending
    expect(usePlatformStore.getState().isLoading).toBe(true);

    resolveAgents(mockAgents);
    resolveScan(mockScanResult);
    await initPromise;
  });

  it("populates agents and skillsByAgent after initialize", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(mockAgents)
      .mockResolvedValueOnce(mockScanResult);

    await usePlatformStore.getState().initialize();

    const state = usePlatformStore.getState();
    expect(state.agents).toEqual(mockAgents);
    expect(state.skillsByAgent).toEqual(mockScanResult.skills_by_agent);
    expect(state.isLoading).toBe(false);
    expect(state.scanGeneration).toBe(1);
    expect(state.error).toBeNull();
  });

  it("calls get_agents and scan_all_skills during initialize", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(mockAgents)
      .mockResolvedValueOnce(mockScanResult);

    await usePlatformStore.getState().initialize();

    expect(invoke).toHaveBeenCalledWith("get_agents");
    expect(invoke).toHaveBeenCalledWith("scan_all_skills");
    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it("sets error and clears isLoading when initialize fails", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("Scan failed"));

    await usePlatformStore.getState().initialize();

    const state = usePlatformStore.getState();
    expect(state.error).toContain("Scan failed");
    expect(state.isLoading).toBe(false);
    expect(state.agents).toEqual([]);
  });

  // ── rescan ────────────────────────────────────────────────────────────────

  it("rescan refreshes agents and skill counts", async () => {
    // Start with some existing state
    usePlatformStore.setState({
      agents: mockAgents,
      skillsByAgent: { "claude-code": 2 },
      isLoading: false,
      isRefreshing: false,
      scanGeneration: 1,
      error: null,
    });

    const updatedScanResult: ScanResult = {
      total_skills: 10,
      agents_scanned: 2,
      skills_by_agent: { "claude-code": 7, central: 3 },
    };

    vi.mocked(invoke)
      .mockResolvedValueOnce(mockAgents)
      .mockResolvedValueOnce(updatedScanResult);

    await usePlatformStore.getState().rescan();

    const state = usePlatformStore.getState();
    expect(state.skillsByAgent["claude-code"]).toBe(7);
    expect(state.isLoading).toBe(false);
    expect(state.scanGeneration).toBe(2);
    expect(state.error).toBeNull();
  });

  it("rescan sets error on failure", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("Network error"));

    await usePlatformStore.getState().rescan();

    const state = usePlatformStore.getState();
    expect(state.error).toContain("Network error");
    expect(state.isLoading).toBe(false);
  });

  it("refreshCounts updates counts without entering the full loading state", async () => {
    usePlatformStore.setState({
      agents: mockAgents,
      skillsByAgent: { "claude-code": 2, central: 3 },
      isLoading: false,
      isRefreshing: false,
      scanGeneration: 1,
      error: null,
    });

    const updatedScanResult: ScanResult = {
      total_skills: 11,
      agents_scanned: 2,
      skills_by_agent: { "claude-code": 8, central: 3 },
    };

    vi.mocked(invoke)
      .mockResolvedValueOnce(mockAgents)
      .mockResolvedValueOnce(updatedScanResult);

    const refreshPromise = usePlatformStore.getState().refreshCounts();
    expect(usePlatformStore.getState().isLoading).toBe(false);
    expect(usePlatformStore.getState().isRefreshing).toBe(true);

    await refreshPromise;

    const state = usePlatformStore.getState();
    expect(state.skillsByAgent).toEqual(updatedScanResult.skills_by_agent);
    expect(state.isLoading).toBe(false);
    expect(state.isRefreshing).toBe(false);
    expect(state.scanGeneration).toBe(2);
  });

  it("setAgentVisibility stores only the list visibility without scanning", async () => {
    usePlatformStore.setState({
      agents: mockAgents,
      skillsByAgent: mockScanResult.skills_by_agent,
      isLoading: false,
      isRefreshing: false,
      updatingAgentIds: {},
      scanGeneration: 1,
      error: null,
    });
    const disabledAgents = mockAgents.map((agent) =>
      agent.id === "claude-code" ? { ...agent, is_enabled: false } : agent
    );
    vi.mocked(invoke).mockResolvedValueOnce(disabledAgents[0]);

    await usePlatformStore.getState().setAgentVisibility("claude-code", false);

    expect(invoke).toHaveBeenNthCalledWith(1, "set_agent_enabled", {
      agentId: "claude-code",
      enabled: false,
    });
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(
      usePlatformStore.getState().agents.find((agent) => agent.id === "claude-code")
        ?.is_enabled
    ).toBe(false);
    expect(usePlatformStore.getState().skillsByAgent).toEqual(mockScanResult.skills_by_agent);
    expect(usePlatformStore.getState().scanGeneration).toBe(1);
    expect(usePlatformStore.getState().updatingAgentIds).toEqual({});
  });

  it.each([false, true])("전체 표시 상태를 %s로 저장하고 스캔하지 않는다", async (visible) => {
    usePlatformStore.setState({ agents: bulkAgents });
    const updated = bulkAgents.map((agent) =>
      toggleableIds.includes(agent.id) ? { ...agent, is_enabled: visible } : agent
    );
    let resolveUpdate!: (agents: AgentWithStatus[]) => void;
    vi.mocked(invoke).mockReturnValueOnce(
      new Promise<AgentWithStatus[]>((resolve) => {
        resolveUpdate = resolve;
      })
    );

    const pending = usePlatformStore.getState().setAllAgentsVisibility(visible);
    expect(Object.keys(usePlatformStore.getState().updatingAgentIds)).toEqual(toggleableIds);
    // 저장이 끝나기 전에 반대 방향의 일괄 변경이나 개별 변경을 보내지 않는다.
    await usePlatformStore.getState().setAllAgentsVisibility(!visible);
    await usePlatformStore.getState().setAgentVisibility("cursor", !visible);
    resolveUpdate(updated);
    await pending;

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("set_all_agents_enabled", { enabled: visible });
    expect(usePlatformStore.getState().agents).toEqual(updated);
    expect(usePlatformStore.getState().skillsByAgent).toEqual({});
    expect(usePlatformStore.getState().scanGeneration).toBe(0);
    expect(usePlatformStore.getState().updatingAgentIds).toEqual({});
  });

  it("일괄 저장 실패 시 기존 상태를 유지하고 다시 시도할 수 있다", async () => {
    usePlatformStore.setState({ agents: bulkAgents });
    vi.mocked(invoke).mockRejectedValueOnce(new Error("저장 실패"));

    await expect(usePlatformStore.getState().setAllAgentsVisibility(false)).rejects.toThrow("저장 실패");

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(usePlatformStore.getState().agents).toEqual(bulkAgents);
    expect(usePlatformStore.getState().updatingAgentIds).toEqual({});
    expect(usePlatformStore.getState().error).toContain("저장 실패");
  });

  it("변경할 플랫폼이 없거나 스캔 중이면 일괄 표시 저장을 생략한다", async () => {
    await usePlatformStore.getState().setAllAgentsVisibility(false);
    usePlatformStore.setState({ agents: mockAgents });
    await usePlatformStore.getState().setAllAgentsVisibility(true);
    usePlatformStore.setState({ isRefreshing: true });
    await usePlatformStore.getState().setAllAgentsVisibility(false);

    expect(invoke).not.toHaveBeenCalled();
    expect(usePlatformStore.getState().updatingAgentIds).toEqual({});
  });
});

import { create } from "zustand";
import { invoke, isTauriRuntime } from "@/lib/tauri";
import { AgentWithStatus, ScanResult } from "@/types";

const BROWSER_FIXTURE_AGENTS: AgentWithStatus[] = [
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
    id: "cursor",
    display_name: "Cursor",
    category: "coding",
    global_skills_dir: "~/.cursor/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "central",
    display_name: "Skill Library",
    category: "central",
    global_skills_dir: "~/.skillsmanage/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "universal",
    display_name: "Shared Install (.agents)",
    category: "shared",
    global_skills_dir: "~/.agents/skills/",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
];

const BROWSER_FIXTURE_COUNTS: ScanResult = {
  total_skills: 1,
  agents_scanned: 4,
  skills_by_agent: {
    "claude-code": 1,
    cursor: 1,
    central: 1,
    universal: 1,
  },
};

// ─── State ────────────────────────────────────────────────────────────────────

interface PlatformState {
  agents: AgentWithStatus[];
  skillsByAgent: Record<string, number>;
  isLoading: boolean;
  isRefreshing: boolean;
  updatingAgentIds: Record<string, boolean>;
  scanGeneration?: number;
  error: string | null;

  // Actions
  initialize: () => Promise<void>;
  rescan: () => Promise<void>;
  refreshCounts: () => Promise<void>;
  setAgentEnabled: (agentId: string, enabled: boolean) => Promise<void>;
}

// ─── Store ────────────────────────────────────────────────────────────────────

export const usePlatformStore = create<PlatformState>((set) => ({
  agents: [],
  skillsByAgent: {},
  isLoading: false,
  isRefreshing: false,
  updatingAgentIds: {},
  scanGeneration: 0,
  error: null,

  /**
   * Initialize the store on app mount: load agents then trigger a full scan.
   * Called once from AppShell's useEffect.
   */
  initialize: async () => {
    set({ isLoading: true, error: null });
    if (!isTauriRuntime()) {
      set((state) => ({
        agents: BROWSER_FIXTURE_AGENTS,
        skillsByAgent: BROWSER_FIXTURE_COUNTS.skills_by_agent,
        isLoading: false,
        scanGeneration: (state.scanGeneration ?? 0) + 1,
      }));
      return;
    }
    try {
      const [agents, scanResult] = await Promise.all([
        invoke<AgentWithStatus[]>("get_agents"),
        invoke<ScanResult>("scan_all_skills"),
      ]);
      set((state) => ({
        agents,
        skillsByAgent: scanResult.skills_by_agent,
        isLoading: false,
        scanGeneration: (state.scanGeneration ?? 0) + 1,
      }));
    } catch (err) {
      set({ error: String(err), isLoading: false });
    }
  },

  /**
   * Re-trigger a full scan and refresh agent list.
   * Called from manual refresh button.
   */
  rescan: async () => {
    set({ isLoading: true, error: null });
    if (!isTauriRuntime()) {
      set((state) => ({
        agents: BROWSER_FIXTURE_AGENTS,
        skillsByAgent: BROWSER_FIXTURE_COUNTS.skills_by_agent,
        isLoading: false,
        scanGeneration: (state.scanGeneration ?? 0) + 1,
      }));
      return;
    }
    try {
      const [agents, scanResult] = await Promise.all([
        invoke<AgentWithStatus[]>("get_agents"),
        invoke<ScanResult>("scan_all_skills"),
      ]);
      set((state) => ({
        agents,
        skillsByAgent: scanResult.skills_by_agent,
        isLoading: false,
        scanGeneration: (state.scanGeneration ?? 0) + 1,
      }));
    } catch (err) {
      set({ error: String(err), isLoading: false });
    }
  },

  refreshCounts: async () => {
    set({ isRefreshing: true, error: null });
    if (!isTauriRuntime()) {
      set((state) => ({
        agents: BROWSER_FIXTURE_AGENTS,
        skillsByAgent: BROWSER_FIXTURE_COUNTS.skills_by_agent,
        isRefreshing: false,
        isLoading: state.isLoading,
        scanGeneration: (state.scanGeneration ?? 0) + 1,
      }));
      return;
    }
    try {
      const [agents, scanResult] = await Promise.all([
        invoke<AgentWithStatus[]>("get_agents"),
        invoke<ScanResult>("scan_all_skills"),
      ]);
      set((state) => ({
        agents,
        skillsByAgent: scanResult.skills_by_agent,
        isRefreshing: false,
        isLoading: state.isLoading,
        scanGeneration: (state.scanGeneration ?? 0) + 1,
      }));
    } catch (err) {
      set({ error: String(err), isRefreshing: false });
    }
  },

  setAgentEnabled: async (agentId, enabled) => {
    set((state) => ({
      updatingAgentIds: { ...state.updatingAgentIds, [agentId]: true },
      error: null,
    }));

    if (!isTauriRuntime()) {
      set((state) => {
        const updatingAgentIds = { ...state.updatingAgentIds };
        delete updatingAgentIds[agentId];
        return {
          agents: state.agents.map((agent) =>
            agent.id === agentId ? { ...agent, is_enabled: enabled } : agent
          ),
          updatingAgentIds,
        };
      });
      return;
    }

    try {
      await invoke<AgentWithStatus>("set_agent_enabled", { agentId, enabled });
      const [agents, scanResult] = await Promise.all([
        invoke<AgentWithStatus[]>("get_agents"),
        invoke<ScanResult>("scan_all_skills"),
      ]);
      set((state) => {
        const updatingAgentIds = { ...state.updatingAgentIds };
        delete updatingAgentIds[agentId];
        return {
          agents,
          skillsByAgent: scanResult.skills_by_agent,
          updatingAgentIds,
          scanGeneration: (state.scanGeneration ?? 0) + 1,
          error: null,
        };
      });
    } catch (error) {
      set((state) => {
        const updatingAgentIds = { ...state.updatingAgentIds };
        delete updatingAgentIds[agentId];
        return { updatingAgentIds, error: String(error) };
      });
      throw error;
    }
  },
}));

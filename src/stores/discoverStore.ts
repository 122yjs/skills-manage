import { create } from "zustand";
import { UnlistenFn } from "@tauri-apps/api/event";
import {
  ScanRoot,
  DiscoveredProject,
  DiscoverResult,
  DiscoverProgressPayload,
  DiscoverFoundPayload,
  DiscoverCompletePayload,
  DiscoverImportResult,
} from "@/types";
import { invoke, listen, isTauriRuntime } from "@/lib/tauri";
import { OBSIDIAN_AGENT_ID } from "@/lib/agents";

const BROWSER_FIXTURE_DISCOVERED_PROJECTS: DiscoveredProject[] = [
  {
    project_path: "/Users/fixture/project",
    project_name: "Fixture Project",
    skills: [
      {
        id: "fixture-central-skill",
        name: "fixture-central-skill",
        description: "Browser validation fixture for Discover drawer entry.",
        file_path: "/Users/fixture/project/.skills/fixture-central-skill/SKILL.md",
        dir_path: "/Users/fixture/project/.skills/fixture-central-skill",
        platform_id: "claude-code",
        platform_name: "Claude Code",
        project_path: "/Users/fixture/project",
        project_name: "Fixture Project",
        is_already_central: true,
      },
    ],
  },
];

const BROWSER_FIXTURE_TOTAL_SKILLS = BROWSER_FIXTURE_DISCOVERED_PROJECTS.reduce(
  (sum, project) => sum + project.skills.length,
  0
);

// ─── State ────────────────────────────────────────────────────────────────────

interface DiscoverState {
  // Scan configuration
  scanRoots: ScanRoot[];
  isLoadingRoots: boolean;

  // Scan progress
  isScanning: boolean;
  scanProgress: number;
  currentPath: string;
  skillsFoundSoFar: number;
  projectsFoundSoFar: number;

  // Results
  discoveredProjects: DiscoveredProject[];
  totalSkillsFound: number;
  lastScanAt: string | null;

  // Grouping / filtering
  groupBy: "project" | "platform" | "skill";
  platformFilter: string | null;
  searchQuery: string;

  // Selection for batch ops
  selectedSkillIds: Set<string>;

  // Error
  error: string | null;

  // Actions
  loadScanRoots: () => Promise<void>;
  setScanRootEnabled: (path: string, enabled: boolean) => Promise<void>;
  startScan: () => Promise<void>;
  stopScan: () => Promise<void>;
  loadDiscoveredSkills: () => Promise<void>;
  refreshCounts: () => Promise<void>;
  rescanFromDisk: () => Promise<void>;
  importToCentral: (skillId: string) => Promise<DiscoverImportResult>;
  importToPlatform: (
    skillId: string,
    agentId: string,
    method?: "auto" | "copy"
  ) => Promise<DiscoverImportResult>;
  clearResults: () => Promise<void>;
  setGroupBy: (groupBy: "project" | "platform" | "skill") => void;
  setPlatformFilter: (platformId: string | null) => void;
  setSearchQuery: (query: string) => void;
  toggleSkillSelection: (skillId: string) => void;
  selectAllVisible: (skillIds: string[]) => void;
  clearSelection: () => void;
  clearError: () => void;
}

// ─── Event listeners (managed outside store) ──────────────────────────────────

let unlistenProgress: UnlistenFn | null = null;
let unlistenFound: UnlistenFn | null = null;
let unlistenComplete: UnlistenFn | null = null;

async function setupEventListeners(set: (fn: Partial<DiscoverState> | ((s: DiscoverState) => Partial<DiscoverState>)) => void) {
  // Clean up any existing listeners.
  if (unlistenProgress) { unlistenProgress(); unlistenProgress = null; }
  if (unlistenFound) { unlistenFound(); unlistenFound = null; }
  if (unlistenComplete) { unlistenComplete(); unlistenComplete = null; }

  unlistenProgress = await listen<DiscoverProgressPayload>("discover:progress", (event) => {
    set({
      scanProgress: event.payload.percent,
      currentPath: event.payload.current_path,
      skillsFoundSoFar: event.payload.skills_found,
      projectsFoundSoFar: event.payload.projects_found,
    });
  });

  unlistenFound = await listen<DiscoverFoundPayload>("discover:found", (event) => {
    set((state) => {
      const newProject = event.payload.project;
      // Check if we already have this project (from a different root).
      const existingIdx = state.discoveredProjects.findIndex(
        (p) => p.project_path === newProject.project_path
      );
      let updatedProjects: DiscoveredProject[];
      if (existingIdx >= 0) {
        // Merge skills into existing project.
        updatedProjects = [...state.discoveredProjects];
        updatedProjects[existingIdx] = {
          ...updatedProjects[existingIdx],
          skills: [...updatedProjects[existingIdx].skills, ...newProject.skills],
        };
      } else {
        updatedProjects = [...state.discoveredProjects, newProject];
      }
      const totalSkills = updatedProjects.reduce((sum, p) => sum + p.skills.length, 0);
      return {
        discoveredProjects: updatedProjects,
        totalSkillsFound: totalSkills,
      };
    });
  });

  unlistenComplete = await listen<DiscoverCompletePayload>("discover:complete", () => {
    // 백엔드 완료 이벤트는 진행 표시만 끝낸다. 실제 start_project_scan Promise가 끝날
    // 때까지 동시성 가드(activeScanToken)는 유지되므로 여기서 해제하지 않는다.
    set({
      isScanning: false,
      scanProgress: 100,
      lastScanAt: new Date().toISOString(),
    });
  });
}

// ─── 디스크 스캔 동시성 가드 ──────────────────────────────────────────────────

// 디스크 스캔 세대. 새 디스크 스캔이 시작될 때마다 증가한다. get_discovered_skills 조회는
// 시작 시점의 세대를 기억했다가 응답이 도착했을 때 값이 달라졌으면(그 사이 새 스캔이
// 시작됐으면) 낡은 DB 스냅샷과 그 오류를 버린다.
let diskResultsGeneration = 0;

// 진행 중인 디스크 스캔 토큰. null이면 진행 중인 스캔이 없다.
// 중지 요청은 스캔 종료가 아니므로 stopScan도 토큰을 해제하지 않는다. 실제
// start_project_scan Promise가 끝나야 해제되며, 그 전까지 새 스캔은 시작되지 않는다
// (백엔드 start_project_scan이 SCAN_CANCEL을 false로 리셋해 이전 스캔과 겹치는 것을 막는다).
let activeScanToken: number | null = null;
let scanTokenSeq = 0;

// 중지가 요청된 스캔 토큰. 루트 로딩이나 리스너 준비 중 중지되면 실제
// start_project_scan을 시작하지 않고 상태와 가드만 정리한다.
let stoppedScanToken: number | null = null;

// ─── Store ────────────────────────────────────────────────────────────────────

export const useDiscoverStore = create<DiscoverState>((set, get) => ({
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

  // ── Scan Roots ─────────────────────────────────────────────────────────────

  loadScanRoots: async () => {
    set({ isLoadingRoots: true, error: null });
    try {
      // Use get_scan_roots which overlays persisted enabled/disabled states
      // from the DB, rather than discover_scan_roots which only auto-detects.
      const roots = await invoke<ScanRoot[]>("get_scan_roots");
      set({ scanRoots: roots, isLoadingRoots: false });
    } catch (err) {
      set({ error: String(err), isLoadingRoots: false });
    }
  },

  setScanRootEnabled: async (path: string, enabled: boolean) => {
    // Optimistically update local state.
    set((state) => ({
      scanRoots: state.scanRoots.map((r) =>
        r.path === path ? { ...r, enabled } : r
      ),
    }));
    // Persist the change to the backend.
    try {
      await invoke("set_scan_root_enabled", { path, enabled });
    } catch (err) {
      // Revert on failure.
      set((state) => ({
        scanRoots: state.scanRoots.map((r) =>
          r.path === path ? { ...r, enabled: !enabled } : r
        ),
        error: String(err),
      }));
    }
  },

  // ── Scan ───────────────────────────────────────────────────────────────────

  startScan: async () => {
    // 자동 rescanFromDisk가 루트를 불러오는 중이거나 스캔 중이면 중복 시작하지 않는다.
    if (activeScanToken !== null) return;
    const scanToken = ++scanTokenSeq;
    activeScanToken = scanToken;
    stoppedScanToken = null;
    // 이 시점부터 이미 시작된 get_discovered_skills 조회는 낡은 스냅샷이 된다.
    diskResultsGeneration++;

    set({
      isScanning: true,
      scanProgress: 0,
      currentPath: "",
      skillsFoundSoFar: 0,
      projectsFoundSoFar: 0,
      discoveredProjects: [],
      totalSkillsFound: 0,
      error: null,
      selectedSkillIds: new Set<string>(),
    });

    try {
      // Set up event listeners for streaming updates.
      await setupEventListeners(set);
      // 리스너를 준비하는 사이 중지됐으면 실제 스캔을 새로 시작하지 않는다.
      if (activeScanToken !== scanToken) return;
      if (stoppedScanToken === scanToken) {
        set({ isScanning: false });
        return;
      }

      const { scanRoots } = get();
      const result = await invoke<DiscoverResult>("start_project_scan", {
        roots: scanRoots,
      });
      // 더 새로운 스캔이 이어받았으면 낡은 결과를 반영하지 않는다.
      // (중지된 스캔의 부분 결과는 토큰이 유지되므로 그대로 반영된다.)
      if (activeScanToken !== scanToken) return;
      set({
        isScanning: false,
        scanProgress: 100,
        discoveredProjects: result.projects,
        totalSkillsFound: result.total_skills,
        lastScanAt: new Date().toISOString(),
      });
    } catch (err) {
      // 리스너 초기화 실패 등으로 스캔이 중단돼도 진행 표시가 고착되지 않게 정리한다.
      if (activeScanToken !== scanToken) return;
      set({
        isScanning: false,
        error: String(err),
      });
    } finally {
      // 실제 시작한 스캔(또는 시작되지 못한 스캔)이 끝나야 가드를 해제한다.
      if (activeScanToken === scanToken) {
        activeScanToken = null;
        stoppedScanToken = null;
      }
    }
  },

  stopScan: async () => {
    // 중지 응답보다 루트·리스너 준비가 먼저 끝나도 새 스캔을 시작하지 않는다.
    const scanToken = activeScanToken;
    stoppedScanToken = scanToken;
    try {
      await invoke("stop_project_scan");
      // 중지 요청만으로 스캔이 끝난 것은 아니다. 실제 start_project_scan Promise가 끝날
      // 때까지 가드를 유지해야 다음 스캔이 SCAN_CANCEL을 리셋해 이전 스캔과 겹치지 않는다.
      if (activeScanToken !== scanToken) return;
      set({ isScanning: false, lastScanAt: new Date().toISOString() });
    } catch (err) {
      if (activeScanToken !== scanToken) return;
      stoppedScanToken = null;
      set({ error: String(err) });
    }
  },

  // ── Load persisted results ─────────────────────────────────────────────────

  loadDiscoveredSkills: async () => {
    set({ error: null });
    if (!isTauriRuntime()) {
      set({
        discoveredProjects: BROWSER_FIXTURE_DISCOVERED_PROJECTS,
        totalSkillsFound: 1,
      });
      return;
    }
    // 스캔이 진행 중이면 DB 스냅샷이 스트리밍 목록을 덮을 수 있으므로 조회를 생략한다.
    if (activeScanToken !== null) return;
    const generation = diskResultsGeneration;
    try {
      const projects = await invoke<DiscoveredProject[]>("get_discovered_skills");
      // 조회 중 새 디스크 스캔이 시작됐다면 낡은 DB 스냅샷이나 오류를 반영하지 않는다.
      if (generation !== diskResultsGeneration || activeScanToken !== null) return;
      const totalSkills = projects.reduce((sum, p) => sum + p.skills.length, 0);
      set({
        discoveredProjects: projects,
        totalSkillsFound: totalSkills,
      });
    } catch (err) {
      if (generation !== diskResultsGeneration || activeScanToken !== null) return;
      set({ error: String(err) });
    }
  },

  refreshCounts: async () => {
    if (!isTauriRuntime()) {
      set({
        discoveredProjects: BROWSER_FIXTURE_DISCOVERED_PROJECTS,
        totalSkillsFound: BROWSER_FIXTURE_TOTAL_SKILLS,
      });
      return;
    }
    // 스캔이 진행 중이면 DB 스냅샷이 스트리밍 목록을 덮을 수 있으므로 조회를 생략한다.
    if (activeScanToken !== null) return;
    const generation = diskResultsGeneration;
    try {
      const projects = await invoke<DiscoveredProject[]>("get_discovered_skills");
      // 조회 중 새 디스크 스캔이 시작됐다면 낡은 DB 스냅샷이나 오류를 반영하지 않는다.
      if (generation !== diskResultsGeneration || activeScanToken !== null) return;
      const totalSkills = projects.reduce((sum, p) => sum + p.skills.length, 0);
      set({
        discoveredProjects: projects,
        totalSkillsFound: totalSkills,
      });
    } catch (err) {
      if (generation !== diskResultsGeneration || activeScanToken !== null) return;
      set({ error: String(err) });
      throw err;
    }
  },

  rescanFromDisk: async () => {
    if (!isTauriRuntime()) {
      set({
        scanRoots: [],
        isLoadingRoots: false,
        isScanning: false,
        scanProgress: 100,
        currentPath: "",
        skillsFoundSoFar: BROWSER_FIXTURE_TOTAL_SKILLS,
        projectsFoundSoFar: BROWSER_FIXTURE_DISCOVERED_PROJECTS.length,
        discoveredProjects: BROWSER_FIXTURE_DISCOVERED_PROJECTS,
        totalSkillsFound: BROWSER_FIXTURE_TOTAL_SKILLS,
        lastScanAt: new Date().toISOString(),
        selectedSkillIds: new Set<string>(),
        error: null,
      });
      return;
    }

    // 루트 로딩 중에 수동 startScan/rescanFromDisk가 겹쳐 들어와도 중복 시작하지 않는다.
    if (activeScanToken !== null) return;
    const scanToken = ++scanTokenSeq;
    activeScanToken = scanToken;
    stoppedScanToken = null;
    // 이 시점부터 이미 시작된 get_discovered_skills 조회는 낡은 스냅샷이 된다.
    diskResultsGeneration++;

    set({ isLoadingRoots: true, error: null });
    try {
      const roots = await invoke<ScanRoot[]>("get_scan_roots");
      // 루트를 불러오는 사이 더 새로운 스캔이 이어받았으면 진행하지 않는다.
      if (activeScanToken !== scanToken) return;
      // 루트를 불러오는 사이 중지됐으면 실제 스캔을 시작하지 않고 상태만 정리한다.
      if (stoppedScanToken === scanToken) {
        set({ isLoadingRoots: false, isScanning: false });
        return;
      }
      set({ scanRoots: roots, isLoadingRoots: false });

      set({
        isScanning: true,
        scanProgress: 0,
        currentPath: "",
        skillsFoundSoFar: 0,
        projectsFoundSoFar: 0,
        discoveredProjects: [],
        totalSkillsFound: 0,
        error: null,
        selectedSkillIds: new Set<string>(),
      });

      await setupEventListeners(set);
      // 리스너를 준비하는 사이 중지됐으면 실제 스캔을 새로 시작하지 않는다.
      if (activeScanToken !== scanToken) return;
      if (stoppedScanToken === scanToken) {
        set({ isScanning: false });
        return;
      }

      const result = await invoke<DiscoverResult>("start_project_scan", {
        roots,
      });
      // 더 새로운 스캔이 이어받았으면 낡은 결과를 반영하지 않는다.
      // (중지된 스캔의 부분 결과는 토큰이 유지되므로 그대로 반영된다.)
      if (activeScanToken !== scanToken) return;
      set({
        isScanning: false,
        scanProgress: 100,
        discoveredProjects: result.projects,
        totalSkillsFound: result.total_skills,
        lastScanAt: new Date().toISOString(),
      });
    } catch (err) {
      // 리스너 초기화 실패 등으로 스캔이 중단돼도 진행 표시가 고착되지 않게 정리한다.
      if (activeScanToken !== scanToken) return;
      set({
        error: String(err),
        isLoadingRoots: false,
        isScanning: false,
      });
    } finally {
      // 실제 시작한 스캔(또는 시작되지 못한 스캔)이 끝나야 가드를 해제한다.
      if (activeScanToken === scanToken) {
        activeScanToken = null;
        stoppedScanToken = null;
      }
    }
  },

  // ── Import ─────────────────────────────────────────────────────────────────

  importToCentral: async (skillId: string) => {
    set({ error: null });
    try {
      const result = await invoke<DiscoverImportResult>(
        "import_discovered_skill_to_central",
        { discoveredSkillId: skillId }
      );
      // Remove the skill from discovered results.
      set((state) => {
        const updatedProjects = state.discoveredProjects
          .map((p) => ({
            ...p,
            skills: p.skills.filter((s) => s.id !== skillId),
          }))
          .filter((p) => p.skills.length > 0);
        const totalSkills = updatedProjects.reduce((sum, p) => sum + p.skills.length, 0);
        const newSelection = new Set(state.selectedSkillIds);
        newSelection.delete(skillId);
        return {
          discoveredProjects: updatedProjects,
          totalSkillsFound: totalSkills,
          selectedSkillIds: newSelection,
        };
      });
      return result;
    } catch (err) {
      set({ error: String(err) });
      throw err;
    }
  },

  importToPlatform: async (
    skillId: string,
    agentId: string,
    method?: "auto" | "copy"
  ) => {
    set({ error: null });
    if (agentId === OBSIDIAN_AGENT_ID) {
      const error = "Obsidian is a source-only Discover category and cannot be used as an install target.";
      set({ error });
      throw new Error(error);
    }
    try {
      const result = await invoke<DiscoverImportResult>(
        "import_discovered_skill_to_platform",
        {
          discoveredSkillId: skillId,
          agentId,
          ...(method ? { method } : {}),
        }
      );
      // NOTE: We do NOT remove the skill from discovered results here because
      // the Rust backend no longer deletes the discovered record on platform
      // install (to support multi-platform install). The skill stays in the
      // list and will be shown with updated status after the next reload.
      return result;
    } catch (err) {
      set({ error: String(err) });
      throw err;
    }
  },

  // ── Clear ──────────────────────────────────────────────────────────────────

  clearResults: async () => {
    try {
      await invoke("clear_discovered_skills");
      set({
        discoveredProjects: [],
        totalSkillsFound: 0,
        lastScanAt: null,
        selectedSkillIds: new Set<string>(),
      });
    } catch (err) {
      set({ error: String(err) });
    }
  },

  // ── Grouping / Filtering ───────────────────────────────────────────────────

  setGroupBy: (groupBy) => set({ groupBy }),
  setPlatformFilter: (platformFilter) => set({ platformFilter }),
  setSearchQuery: (searchQuery) => set({ searchQuery }),

  // ── Selection ──────────────────────────────────────────────────────────────

  toggleSkillSelection: (skillId) => {
    set((state) => {
      const newSelection = new Set(state.selectedSkillIds);
      if (newSelection.has(skillId)) {
        newSelection.delete(skillId);
      } else {
        newSelection.add(skillId);
      }
      return { selectedSkillIds: newSelection };
    });
  },

  selectAllVisible: (skillIds) => {
    set((state) => {
      const newSelection = new Set(state.selectedSkillIds);
      for (const id of skillIds) {
        newSelection.add(id);
      }
      return { selectedSkillIds: newSelection };
    });
  },

  clearSelection: () => set({ selectedSkillIds: new Set<string>() }),

  // ── Error ──────────────────────────────────────────────────────────────────

  clearError: () => set({ error: null }),
}));

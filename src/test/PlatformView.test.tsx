import { describe, it, expect, vi, beforeEach } from "vitest";
import { act, render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import {
  MemoryRouter,
  Route,
  Routes,
  useNavigate,
} from "react-router-dom";
import { PlatformView } from "../pages/PlatformView";
import { AgentWithStatus, ScannedSkill } from "../types";

// Mock stores
vi.mock("../stores/platformStore", () => ({
  usePlatformStore: vi.fn(),
}));

vi.mock("../stores/skillStore", () => ({
  useSkillStore: vi.fn(),
}));

vi.mock("../stores/centralSkillsStore", () => ({
  useCentralSkillsStore: vi.fn(),
}));

vi.mock("../components/skill/SkillDetailDrawer", () => ({
  SkillDetailDrawer: ({
    open,
    skillId,
    agentId,
    rowId,
    onOpenChange,
    returnFocusRef,
  }: {
    open: boolean;
    skillId: string | null;
    agentId?: string | null;
    rowId?: string | null;
    onOpenChange: (open: boolean) => void;
    returnFocusRef?: { current: HTMLElement | null };
  }) =>
    open ? (
      <div data-testid="skill-detail-drawer">
        <div>drawer-skill:{skillId}</div>
        <div>drawer-agent:{agentId ?? "none"}</div>
        <div>drawer-row:{rowId ?? "none"}</div>
        <button
          onClick={() => {
            onOpenChange(false);
            returnFocusRef?.current?.focus();
          }}
        >
          Close drawer
        </button>
      </div>
    ) : null,
}));

vi.mock("../components/skill/SkillFolderDrawer", () => ({
  SkillFolderDrawer: ({
    open,
    title,
    skills,
    onInstallAll,
  }: {
    open: boolean;
    title: string;
    skills: Array<{ name: string }>;
    onInstallAll?: () => void;
  }) =>
    open ? (
      <div data-testid="skill-folder-drawer">
        <div>folder-title:{title}</div>
        {onInstallAll && <button onClick={onInstallAll}>install-all</button>}
        {skills.map((skill) => (
          <div key={skill.name}>folder-skill:{skill.name}</div>
        ))}
      </div>
    ) : null,
}));

import { usePlatformStore } from "../stores/platformStore";
import { useSkillStore } from "../stores/skillStore";
import { useCentralSkillsStore } from "../stores/centralSkillsStore";
import { SkillUsageBusyError, useSkillUsageStore } from "../stores/skillUsageStore";
import * as tauriBridge from "@/lib/tauri";

const userSourceText = /用户来源|User source/i;
const pluginSourceText = /插件来源|Plugin source/i;
const readOnlyText = /只读|Read-only/i;
const badgeQueryOptions = { selector: "span" } as const;
const claudeTabName = (label: string, count?: number) =>
  count == null
    ? new RegExp(`^${label}(?:\\s*\\(\\d+\\))?$`)
    : new RegExp(`^${label}\\s*\\(${count}\\)$`);
const getCardBadgeMatches = (matcher: RegExp) =>
  screen
    .queryAllByText(matcher, badgeQueryOptions)
    .filter((element) => element.closest(".rounded-xl"));

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const mockAgent: AgentWithStatus = {
  id: "claude-code",
  display_name: "Claude Code",
  category: "coding",
  global_skills_dir: "/Users/test/.claude/skills/",
  is_detected: true,
  is_builtin: true,
  is_enabled: true,
};

const mockCursorAgent: AgentWithStatus = {
  id: "cursor",
  display_name: "Cursor",
  category: "coding",
  global_skills_dir: "/Users/test/.cursor/skills/",
  is_detected: true,
  is_builtin: true,
  is_enabled: true,
};

const mockSkills: ScannedSkill[] = [
  {
    id: "frontend-design",
    name: "frontend-design",
    description: "Build distinctive, production-grade frontend interfaces",
    file_path: "~/.claude/skills/frontend-design/SKILL.md",
    dir_path: "~/.claude/skills/frontend-design",
    link_type: "symlink",
    symlink_target: "~/.agents/skills/frontend-design",
    is_central: true,
  },
  {
    id: "code-reviewer",
    name: "code-reviewer",
    description: "Review code changes and identify high-confidence actionable bugs",
    file_path: "~/.claude/skills/code-reviewer/SKILL.md",
    dir_path: "~/.claude/skills/code-reviewer",
    link_type: "copy",
    is_central: false,
  },
];

const mockCursorSkills: ScannedSkill[] = [
  {
    id: "cursor-helper",
    name: "cursor-helper",
    description: "Cursor-specific helper skill",
    file_path: "~/.cursor/skills/cursor-helper/SKILL.md",
    dir_path: "~/.cursor/skills/cursor-helper",
    link_type: "symlink",
    symlink_target: "~/.agents/skills/cursor-helper",
    is_central: true,
  },
];

const mockNestedPlatformSkills: ScannedSkill[] = [
  {
    id: "root-helper",
    name: "root-helper",
    description: "Top-level helper",
    file_path: "/Users/test/.claude/skills/root-helper/SKILL.md",
    dir_path: "/Users/test/.claude/skills/root-helper",
    link_type: "copy",
    is_central: false,
  },
  {
    id: "nested-helper",
    name: "nested-helper",
    description: "Nested helper",
    file_path: "/Users/test/.claude/skills/toolkit/nested-helper/SKILL.md",
    dir_path: "/Users/test/.claude/skills/toolkit/nested-helper",
    link_type: "copy",
    is_central: false,
  },
];

const mockPluginBundleSkills: ScannedSkill[] = [
  {
    id: "ponytail-audit",
    row_id: "claude-code::plugin::ponytail-audit",
    name: "ponytail-audit",
    description: "Audit over-engineering",
    file_path: "/Users/test/.claude/plugins/ponytail/1.0.0/skills/ponytail-audit/SKILL.md",
    dir_path: "/Users/test/.claude/plugins/ponytail/1.0.0/skills/ponytail-audit",
    link_type: "copy",
    is_central: false,
    source_kind: "plugin",
    source_root: "/Users/test/.claude/plugins/ponytail/1.0.0",
    source_label: "ponytail@official",
    is_read_only: true,
  },
  {
    id: "ponytail-review",
    row_id: "claude-code::plugin::ponytail-review",
    name: "ponytail-review",
    description: "Review over-engineering",
    file_path: "/Users/test/.claude/plugins/ponytail/1.0.0/skills/ponytail-review/SKILL.md",
    dir_path: "/Users/test/.claude/plugins/ponytail/1.0.0/skills/ponytail-review",
    link_type: "copy",
    is_central: false,
    source_kind: "plugin",
    source_root: "/Users/test/.claude/plugins/ponytail/1.0.0",
    source_label: "ponytail@official",
    is_read_only: true,
  },
];

const mockDuplicateClaudeSkills: ScannedSkill[] = [
  {
    id: "shared-skill",
    row_id: "claude-code::user::shared-skill",
    name: "shared-skill",
    description: "User-source copy",
    file_path: "~/.claude/skills/shared-skill/SKILL.md",
    dir_path: "~/.claude/skills/shared-skill",
    link_type: "native",
    is_central: false,
    source_kind: "user",
    source_root: "~/.claude/skills",
    is_read_only: false,
    conflict_count: 2,
  },
  {
    id: "shared-skill",
    row_id: "claude-code::plugin::shared-skill",
    name: "shared-skill",
    description: "Plugin copy",
    file_path: "~/.claude/plugins/cache/publisher/plugin-a/1.0.0/skills/shared-skill/SKILL.md",
    dir_path: "~/.claude/plugins/cache/publisher/plugin-a/1.0.0/skills/shared-skill",
    link_type: "native",
    is_central: false,
    source_kind: "plugin",
    source_root: "~/.claude/plugins/cache/publisher/plugin-a/1.0.0",
    is_read_only: true,
    conflict_count: 2,
  },
];

const mockDuplicateClaudeSkillsWithDistinctIds: ScannedSkill[] = [
  {
    id: "shared-skill-id",
    row_id: "claude-code::user::shared-skill-id",
    name: "Shared skill",
    description: "User-source copy",
    file_path: "~/.claude/skills/shared-skill/SKILL.md",
    dir_path: "~/.claude/skills/shared-skill",
    link_type: "native",
    is_central: false,
    source_kind: "user",
    source_root: "~/.claude/skills",
    is_read_only: false,
    conflict_count: 2,
  },
  {
    id: "shared-skill-id",
    row_id: "claude-code::plugin::shared-skill-id",
    name: "Shared skill",
    description: "Plugin copy",
    file_path: "~/.claude/plugins/cache/publisher/plugin-a/1.0.0/skills/shared-skill/SKILL.md",
    dir_path: "~/.claude/plugins/cache/publisher/plugin-a/1.0.0/skills/shared-skill",
    link_type: "native",
    is_central: false,
    source_kind: "plugin",
    source_root: "~/.claude/plugins/cache/publisher/plugin-a/1.0.0",
    is_read_only: true,
    conflict_count: 2,
  },
];

const mockClaudePluginSliceDuplicates: ScannedSkill[] = [
  {
    id: "shared-skill",
    row_id: "claude-code::user::shared-skill",
    name: "shared-skill",
    description: "User-source copy",
    file_path: "~/.claude/skills/shared-skill/SKILL.md",
    dir_path: "~/.claude/skills/shared-skill",
    link_type: "native",
    is_central: false,
    source_kind: "user",
    source_root: "~/.claude/skills",
    is_read_only: false,
    conflict_count: 3,
  },
  {
    id: "shared-skill",
    row_id: "claude-code::plugin::publisher-a::shared-skill",
    name: "shared-skill",
    description: "Plugin A copy",
    file_path: "~/.claude/plugins/cache/publisher-a/plugin-a/1.0.0/skills/shared-skill/SKILL.md",
    dir_path: "~/.claude/plugins/cache/publisher-a/plugin-a/1.0.0/skills/shared-skill",
    link_type: "native",
    is_central: false,
    source_kind: "plugin",
    source_root: "~/.claude/plugins/cache/publisher-a/plugin-a/1.0.0",
    is_read_only: true,
    conflict_count: 3,
  },
  {
    id: "shared-skill",
    row_id: "claude-code::plugin::publisher-b::shared-skill",
    name: "shared-skill",
    description: "Plugin B copy",
    file_path: "~/.claude/plugins/cache/publisher-b/plugin-b/2.0.0/.claude/skills/shared-skill/SKILL.md",
    dir_path: "~/.claude/plugins/cache/publisher-b/plugin-b/2.0.0/.claude/skills/shared-skill",
    link_type: "native",
    is_central: false,
    source_kind: "plugin",
    source_root: "~/.claude/plugins/cache/publisher-b/plugin-b/2.0.0",
    is_read_only: true,
    conflict_count: 3,
  },
];

const mockGetSkillsByAgent = vi.fn();
const mockLoadCentralSkills = vi.fn();
const mockInstallSkill = vi.fn();
const mockRefreshCounts = vi.fn();
const mockLoadUsageStatus = vi.fn();
const mockSetSkillUsage = vi.fn();
const mockSetPlatformUsage = vi.fn();
const mockDeleteSkillFromAgent = vi.fn();
const mockDeletePlatformInstallations = vi.fn();
const mockLoadPlatformSkillControls = vi.fn();
const mockSetPlatformSkillControl = vi.fn();
const mockDeletePlatformSkillControl = vi.fn();
const mockReapplyPlatformSkillControl = vi.fn();
const mockLoadSharedSkillImpact = vi.fn();
const mockSetSharedSkillUsage = vi.fn();
const mockUsePlatformStore = vi.mocked(usePlatformStore);
const mockUseSkillStore = vi.mocked(useSkillStore);
const mockUseCentralSkillsStore = vi.mocked(useCentralSkillsStore);

function buildPlatformStoreState(overrides = {}) {
  return {
    agents: [mockAgent],
    skillsByAgent: { "claude-code": 2 },
    isLoading: false,
    isRefreshing: false,
    scanGeneration: 1,
    error: null,
    initialize: vi.fn(),
    rescan: vi.fn(),
    refreshCounts: mockRefreshCounts,
    ...overrides,
  };
}

function buildSkillStoreState(overrides = {}) {
  return {
    skillsByAgent: { "claude-code": mockSkills },
    loadingByAgent: { "claude-code": false },
    pendingSkillActionKeys: {},
    error: null,
    getSkillsByAgent: mockGetSkillsByAgent,
    ...overrides,
  };
}

function buildCentralSkillsStoreState(overrides = {}) {
  return {
    skills: [],
    agents: [mockAgent],
    loadCentralSkills: mockLoadCentralSkills,
    loadInstallTarget: vi.fn().mockResolvedValue({
      id: "frontend-design", name: "frontend-design", linked_agents: ["claude-code"],
      read_only_agents: [], is_central: false,
    }),
    installSkill: mockInstallSkill,
    ...overrides,
  };
}

function installDefaultStoreMocks() {
  mockUsePlatformStore.mockImplementation((selector?: unknown) => {
    const state = buildPlatformStoreState();
    if (typeof selector === "function") return selector(state);
    return state;
  });
  mockUseSkillStore.mockImplementation((selector?: unknown) => {
    const state = buildSkillStoreState();
    if (typeof selector === "function") return selector(state);
    return state;
  });
  mockUseCentralSkillsStore.mockImplementation((selector?: unknown) => {
    const state = buildCentralSkillsStoreState();
    if (typeof selector === "function") return selector(state);
    return state;
  });
}

function renderPlatformView(agentId = "claude-code") {
  return render(
    <MemoryRouter initialEntries={[`/platform/${agentId}`]}>
      <Routes>
        <Route path="/platform/:agentId" element={<PlatformView />} />
      </Routes>
    </MemoryRouter>
  );
}

let testNavigate: ReturnType<typeof useNavigate> | null = null;

function NavigationHarness() {
  testNavigate = useNavigate();
  return null;
}

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("PlatformView", () => {
  it("보관함에 없는 플랫폼 스킬의 설치 창을 연다", async () => {
    renderPlatformView();
    fireEvent.click(screen.getByRole("button", { name: /将 frontend-design 安装到平台/i }));
    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
    expect(screen.getByRole("dialog")).toHaveTextContent("frontend-design");
  });

  beforeEach(() => {
    vi.clearAllMocks();
    window.localStorage.clear();
    testNavigate = null;
    mockRefreshCounts.mockReset();
    mockLoadUsageStatus.mockReset().mockResolvedValue(undefined);
    mockSetSkillUsage.mockReset().mockResolvedValue(undefined);
    mockSetPlatformUsage.mockReset().mockResolvedValue(undefined);
    mockDeleteSkillFromAgent.mockReset().mockResolvedValue(undefined);
    mockDeletePlatformInstallations.mockReset().mockResolvedValue({ deleted: [], failed: [] });
    mockLoadPlatformSkillControls.mockReset().mockResolvedValue(undefined);
    mockSetPlatformSkillControl.mockReset().mockResolvedValue(undefined);
    mockDeletePlatformSkillControl.mockReset().mockResolvedValue(undefined);
    mockReapplyPlatformSkillControl.mockReset().mockResolvedValue(undefined);
    useSkillUsageStore.setState({
      statuses: [
        {
          agent_id: "claude-code",
          active_count: 2,
          paused_count: 0,
          external_count: 0,
          skills: [
            { skill_id: "frontend-design", name: "frontend-design", enabled: true, paused_by_bulk: false },
            { skill_id: "code-reviewer", name: "code-reviewer", enabled: true, paused_by_bulk: false },
          ],
        },
      ],
      isLoading: false,
      updatingSkillKeys: {},
      updatingAgentIds: {},
      error: null,
      loadUsageStatus: mockLoadUsageStatus,
      setSkillUsage: mockSetSkillUsage,
      setPlatformUsage: mockSetPlatformUsage,
      deleteSkillFromAgent: mockDeleteSkillFromAgent,
      deletePlatformInstallations: mockDeletePlatformInstallations,
      platformControlsByAgent: {},
      updatingPlatformControlKeys: {},
      loadPlatformSkillControls: mockLoadPlatformSkillControls,
      setPlatformSkillControl: mockSetPlatformSkillControl,
      deletePlatformSkillControl: mockDeletePlatformSkillControl,
      reapplyPlatformSkillControl: mockReapplyPlatformSkillControl,
      sharedImpactsById: {},
      updatingSharedKeys: {},
      updatingSharedBulk: false,
      loadSharedSkillImpact: mockLoadSharedSkillImpact,
      setSharedSkillUsage: mockSetSharedSkillUsage,
    });
    installDefaultStoreMocks();
  });

  // ── Header ────────────────────────────────────────────────────────────────

  it("shows platform name in header", () => {
    renderPlatformView();
    expect(screen.getByText("Claude Code")).toBeInTheDocument();
  });

  it("shows platform directory path in header", () => {
    renderPlatformView();
    expect(screen.getByText("/Users/test/.claude/skills/")).toBeInTheDocument();
  });

  // ── Skill List ────────────────────────────────────────────────────────────

  it("renders skill cards for all skills", () => {
    renderPlatformView();
    expect(screen.getByText("frontend-design")).toBeInTheDocument();
    expect(screen.getByText("code-reviewer")).toBeInTheDocument();
  });

  it("defaults to all-skills mode for nested platform skills", () => {
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockNestedPlatformSkills },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView();

    expect(screen.getByText("nested-helper")).toBeInTheDocument();
    expect(screen.queryByText("toolkit")).not.toBeInTheDocument();
  });

  it("shows platform folders and only top-level skills in folders mode", () => {
    window.localStorage.setItem("skills-manage.skillListViewMode.platform", "folders");
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockNestedPlatformSkills },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView();

    expect(screen.getByText("toolkit")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /查看 root-helper 的详情/i })
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /查看 nested-helper 的详情/i })
    ).not.toBeInTheDocument();
  });

  it("opens a platform folder drawer for nested skills", () => {
    window.localStorage.setItem("skills-manage.skillListViewMode.platform", "folders");
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockNestedPlatformSkills },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView();

    fireEvent.click(screen.getByRole("button", { name: /打开目录 toolkit|Open folder toolkit/i }));

    expect(screen.getByTestId("skill-folder-drawer")).toBeInTheDocument();
    expect(screen.getByText("folder-title:toolkit")).toBeInTheDocument();
    expect(screen.getByText("folder-skill:nested-helper")).toBeInTheDocument();
  });

  it("groups plugin skills by their plugin label in folders mode", () => {
    window.localStorage.setItem("skills-manage.skillListViewMode.platform", "folders");
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockPluginBundleSkills },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView();

    expect(screen.getByText("ponytail@official")).toBeInTheDocument();
    expect(screen.getByText(/2 个技能|2 skills/i)).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /查看 ponytail-audit 的详情/i })
    ).not.toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", {
        name: /打开目录 ponytail@official|Open folder ponytail@official/i,
      })
    );
    fireEvent.click(screen.getByRole("button", { name: "install-all" }));

    expect(
      screen.getByRole("dialog", { name: /批量安装.*ponytail@official|Batch Install.*ponytail@official/i })
    ).toBeInTheDocument();
  });

  it("shows source indicator on skill cards", () => {
    renderPlatformView();
    expect(
      screen.getAllByText((_, element) => element?.textContent?.replace(/\s+/g, " ").trim() === "技能仓库 - 符号链接")
        .length
    ).toBeGreaterThan(0);
    expect(
      screen.getAllByText((_, element) => element?.textContent?.replace(/\s+/g, " ").trim() === "独立安装 - 复制安装")
        .length
    ).toBeGreaterThan(0);
  });

  it("renders browser fixture installed card on the localhost validation surface without Tauri", async () => {
    const isTauriSpy = vi.spyOn(tauriBridge, "isTauriRuntime").mockReturnValue(false);

    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: {
          "claude-code": [
            {
              id: "fixture-central-skill",
              name: "fixture-central-skill",
              description: "Browser fixture skill sourced from the central library",
              file_path: "~/.claude/skills/fixture-central-skill/SKILL.md",
              dir_path: "~/.claude/skills/fixture-central-skill",
              link_type: "symlink",
              symlink_target: "~/.agents/skills/fixture-central-skill",
              is_central: true,
            },
          ],
        },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    render(
      <MemoryRouter initialEntries={["/platform/claude-code"]}>
        <Routes>
          <Route path="/platform/:agentId" element={<PlatformView />} />
        </Routes>
      </MemoryRouter>
    );

    expect(await screen.findByRole("button", { name: /查看 fixture-central-skill 的详情/i })).toBeInTheDocument();
    expect(
      screen.getAllByText((_, element) => element?.textContent?.replace(/\s+/g, " ").trim() === "技能仓库 - 符号链接")
        .length
    ).toBeGreaterThan(0);

    isTauriSpy.mockRestore();
  });

  // ── Empty State ───────────────────────────────────────────────────────────

  it("shows empty state when platform has no skills", () => {
    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      const state = buildPlatformStoreState({
        skillsByAgent: { "claude-code": 0 },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": [] },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    render(
      <MemoryRouter initialEntries={["/platform/claude-code"]}>
        <Routes>
          <Route path="/platform/:agentId" element={<PlatformView />} />
        </Routes>
      </MemoryRouter>
    );

    expect(
      screen.getByText(/该平台暂无技能/)
    ).toBeInTheDocument();
  });

  // ── Platform Not Found ────────────────────────────────────────────────────

  it("shows not found when agent doesn't exist", () => {
    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      const state = buildPlatformStoreState({ agents: [] });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({ skillsByAgent: {} });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    render(
      <MemoryRouter initialEntries={["/platform/unknown"]}>
        <Routes>
          <Route path="/platform/:agentId" element={<PlatformView />} />
        </Routes>
      </MemoryRouter>
    );

    expect(screen.getByText("未找到平台")).toBeInTheDocument();
  });

  // ── Search / Filter ───────────────────────────────────────────────────────

  it("renders search input", () => {
    renderPlatformView();
    expect(
      screen.getByPlaceholderText(/搜索技能/)
    ).toBeInTheDocument();
  });

  it("filters skills by name when searching", async () => {
    renderPlatformView();
    const searchInput = screen.getByPlaceholderText(/搜索技能/);
    fireEvent.change(searchInput, { target: { value: "frontend" } });

    await waitFor(() => {
      expect(screen.getByText("frontend-design")).toBeInTheDocument();
      expect(screen.queryByText("code-reviewer")).not.toBeInTheDocument();
    });
  });

  it("filters skills by description when searching", async () => {
    renderPlatformView();
    const searchInput = screen.getByPlaceholderText(/搜索技能/);
    fireEvent.change(searchInput, { target: { value: "actionable" } });

    await waitFor(() => {
      expect(screen.getByText("code-reviewer")).toBeInTheDocument();
      expect(screen.queryByText("frontend-design")).not.toBeInTheDocument();
    });
  });

  it("shows all skills when search is cleared", async () => {
    renderPlatformView();
    const searchInput = screen.getByPlaceholderText(/搜索技能/);
    fireEvent.change(searchInput, { target: { value: "frontend" } });
    fireEvent.change(searchInput, { target: { value: "" } });

    await waitFor(() => {
      expect(screen.getByText("frontend-design")).toBeInTheDocument();
      expect(screen.getByText("code-reviewer")).toBeInTheDocument();
    });
  });

  it("shows empty state message when search has no results", async () => {
    renderPlatformView();
    const searchInput = screen.getByPlaceholderText(/搜索技能/);
    fireEvent.change(searchInput, { target: { value: "nonexistent-skill-xyz" } });

    await waitFor(() => {
      expect(screen.getByText(/没有匹配的技能/)).toBeInTheDocument();
    });
  });

  // ── Data Loading ──────────────────────────────────────────────────────────

  it("calls getSkillsByAgent on mount", () => {
    renderPlatformView();
    expect(mockGetSkillsByAgent).toHaveBeenCalledWith("claude-code");
  });

  it("opens the skill detail drawer without navigating away", async () => {
    renderPlatformView();

    fireEvent.click(screen.getByRole("button", { name: /查看 frontend-design 的详情/i }));

    await waitFor(() => {
      expect(screen.getByTestId("skill-detail-drawer")).toBeInTheDocument();
    });
    expect(screen.getByText("drawer-skill:frontend-design")).toBeInTheDocument();
  });

  it("passes Claude row identity into the drawer when duplicate platform rows share a skill id", async () => {
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockDuplicateClaudeSkills },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView();
    fireEvent.click(screen.getByRole("button", { name: /查看 .+ 的 \d+ 个来源位置/ }));

    const detailButtons = screen.getAllByRole("button", { name: /查看 shared-skill 的详情/i });
    expect(detailButtons).toHaveLength(2);

    fireEvent.click(detailButtons[1]);

    await waitFor(() => {
      expect(screen.getByTestId("skill-detail-drawer")).toBeInTheDocument();
    });

    expect(screen.getByText("drawer-skill:shared-skill")).toBeInTheDocument();
    expect(screen.getByText("drawer-agent:claude-code")).toBeInTheDocument();
    expect(
      screen.getByText("drawer-row:claude-code::plugin::shared-skill")
    ).toBeInTheDocument();
  });

  it("shows duplicate Claude rows with explicit source markers and read-only list treatment", () => {
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockDuplicateClaudeSkills },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    useSkillUsageStore.setState({
      statuses: [{
        agent_id: "claude-code",
        active_count: 1,
        paused_count: 0,
        external_count: 1,
        skills: [{
          skill_id: "shared-skill",
          name: "shared-skill",
          enabled: true,
          paused_by_bulk: false,
        }],
      }],
    });

    renderPlatformView();
    fireEvent.click(screen.getByRole("button", { name: /查看 .+ 的 \d+ 个来源位置/ }));

    expect(screen.getAllByRole("button", { name: /查看 shared-skill 的详情/i })).toHaveLength(2);

    const [userBadge] = getCardBadgeMatches(userSourceText);
    const [pluginBadge] = getCardBadgeMatches(pluginSourceText);
    const [readOnlyBadge] = getCardBadgeMatches(readOnlyText);

    expect(userBadge).toBeDefined();
    expect(pluginBadge).toBeDefined();
    expect(readOnlyBadge).toBeDefined();
    const userCard = userBadge.closest(".rounded-xl");
    const pluginCard = pluginBadge.closest(".rounded-xl");

    expect(userCard).not.toBeNull();
    expect(pluginCard).not.toBeNull();
    expect(readOnlyBadge.closest(".rounded-xl")).toBe(pluginCard);

    if (!userCard || !pluginCard) {
      return;
    }

    expect(
      within(userCard as HTMLElement).getByRole("button", {
        name: /将 shared-skill 安装到平台/i,
      })
    ).toBeInTheDocument();
    expect(
      within(userCard as HTMLElement).getByRole("switch", {
        name: /切换 shared-skill 的激活状态/i,
      })
    ).toBeInTheDocument();
    expect(
      within(pluginCard as HTMLElement).queryByRole("button", {
        name: /将 shared-skill 安装到平台/i,
      })
    ).not.toBeInTheDocument();
    expect(
      within(pluginCard as HTMLElement).queryByRole("switch", {
        name: /切换 shared-skill 的激活状态/i,
      })
    ).not.toBeInTheDocument();
  });

  it("shows the shared source toggle and sends a platform-scoped change", async () => {
    const sharedSkill: ScannedSkill = {
      id: "shared-public",
      row_id: "claude-code::compatibility::shared-public",
      name: "shared-public",
      description: "Shared compatibility source",
      file_path: "/Users/test/.agents/skills/shared-public/SKILL.md",
      dir_path: "/Users/test/.agents/skills/shared-public",
      link_type: "native",
      is_central: false,
      source_kind: "compatibility",
      source_root: "/Users/test/.agents/skills",
      is_read_only: true,
    };
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({ skillsByAgent: { "claude-code": [sharedSkill] } });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    useSkillUsageStore.setState({
      platformControlsByAgent: {
        "claude-code": [{
          agent_id: "claude-code",
          skill_id: sharedSkill.id,
          row_id: sharedSkill.row_id!,
          skill_name: sharedSkill.name,
          source_path: sharedSkill.dir_path,
          source_kind: "compatibility",
          state: "active",
          supported: true,
          can_toggle: true,
          can_delete: true,
          can_reapply: false,
          reason: null,
          requires_reload: true,
          scope: "name",
          affected_source_count: 1,
          adapter: "claude-skill-overrides",
          config_path: "/Users/test/.claude/settings.json",
        }],
      },
    });

    renderPlatformView();

    const toggle = screen.getByRole("switch", { name: /shared-public.*활성 상태|shared-public.*激活状态/i });
    expect(toggle).toBeChecked();
    expect(screen.getAllByText(/새 세션|新会话|reload/i).length).toBeGreaterThan(0);
    fireEvent.click(toggle);

    await waitFor(() => {
      expect(mockSetPlatformSkillControl).toHaveBeenCalledWith(
        "claude-code",
        {
          skillId: sharedSkill.id,
          skillName: sharedSkill.name,
          sourcePath: sharedSkill.dir_path,
        },
        false
      );
    });
  });

  it("opens an impact dialog for shared installs and confirms with a fresh token", async () => {
    const impact = {
      shared_install_id: "~/.agents/skills/frontend-design",
      skill_id: "frontend-design",
      skill_name: "frontend-design",
      enabled: true,
      confirmed_platforms: [{ agent_id: "claude-code", display_name: "Claude Code" }],
      separate_installs: [],
      reason: null,
      management_path: "~/.agents/skills/frontend-design",
      confirmation_token: "token-1",
    };
    mockLoadSharedSkillImpact.mockResolvedValue(impact);
    mockSetSharedSkillUsage.mockResolvedValue({
      applied: true,
      impact: { ...impact, enabled: false, confirmation_token: "token-2" },
    });
    useSkillUsageStore.setState({
      loadSharedSkillImpact: mockLoadSharedSkillImpact,
      setSharedSkillUsage: mockSetSharedSkillUsage,
      platformControlsByAgent: {
        "claude-code": [{
          agent_id: "claude-code",
          skill_id: "frontend-design",
          row_id: "claude-code::frontend-design",
          skill_name: "frontend-design",
          source_path: "~/.claude/skills/frontend-design",
          source_kind: "compatibility",
          state: "active",
          supported: true,
          can_toggle: true,
          can_delete: false,
          can_reapply: false,
          reason: null,
          requires_reload: false,
          scope: "path",
          affected_source_count: 1,
          adapter: "claude-skill-overrides",
          config_path: "/Users/test/.claude/settings.json",
          shared_install: impact,
          excluded_here: true,
        }],
      },
    });

    renderPlatformView();

    const toggle = screen.getByRole("switch", { name: "切换 frontend-design 的公用状态" });
    expect(toggle).toBeChecked();
    expect(screen.getByText("已在此平台排除")).toBeInTheDocument();

    fireEvent.click(toggle);

    await waitFor(() => {
      expect(mockLoadSharedSkillImpact).toHaveBeenCalledWith("~/.agents/skills/frontend-design");
    });
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "停用公用" })).toBeEnabled();
    expect(screen.getByText("已确认 1 个")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "停用公用" }));

    await waitFor(() => {
      expect(mockSetSharedSkillUsage).toHaveBeenCalledWith(
        "~/.agents/skills/frontend-design",
        false,
        "token-1"
      );
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(mockSetPlatformSkillControl).not.toHaveBeenCalled();
    expect(mockSetSkillUsage).not.toHaveBeenCalled();
    expect(screen.getByText("已在此平台排除")).toBeInTheDocument();
  });

  it("shows unsupported controls as unavailable and keeps the switch disabled", () => {
    const unsupportedSkill: ScannedSkill = {
      id: "cursor-public",
      name: "cursor-public",
      description: "Cursor compatibility source",
      file_path: "/Users/test/.agents/skills/cursor-public/SKILL.md",
      dir_path: "/Users/test/.agents/skills/cursor-public",
      link_type: "native",
      is_central: false,
      source_kind: "compatibility",
      source_root: "/Users/test/.agents/skills",
      is_read_only: true,
    };
    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      const state = buildPlatformStoreState({ agents: [mockCursorAgent], skillsByAgent: { cursor: 1 } });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({ skillsByAgent: { cursor: [unsupportedSkill] } });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    useSkillUsageStore.setState({
      platformControlsByAgent: {
        cursor: [{
          agent_id: "cursor",
          skill_id: unsupportedSkill.id,
          row_id: unsupportedSkill.row_id ?? "cursor-public",
          skill_name: unsupportedSkill.name,
          source_path: unsupportedSkill.dir_path,
          source_kind: "compatibility",
          state: "unsupported",
          supported: false,
          can_toggle: false,
          can_delete: false,
          can_reapply: false,
          reason: "Cursor의 공용 출처 독립 제어를 확인하지 못했습니다.",
          requires_reload: false,
          scope: "path",
          affected_source_count: 1,
          adapter: "unsupported",
          config_path: null,
        }],
      },
    });

    renderPlatformView("cursor");

    expect(screen.getByText(/无法确认|Unavailable|확인 불가/)).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: /cursor-public/i })).toHaveAttribute("aria-disabled", "true");
  });

  it("shows a deleted shared application as reapply instead of an active toggle", async () => {
    const deletedSkill: ScannedSkill = {
      id: "deleted-public",
      row_id: "claude-code::compatibility::deleted-public",
      name: "deleted-public",
      description: "Deleted shared application",
      file_path: "/Users/test/.agents/skills/deleted-public/SKILL.md",
      dir_path: "/Users/test/.agents/skills/deleted-public",
      link_type: "native",
      is_central: false,
      source_kind: "compatibility",
      source_root: "/Users/test/.agents/skills",
      is_read_only: true,
    };
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({ skillsByAgent: { "claude-code": [deletedSkill] } });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    useSkillUsageStore.setState({
      platformControlsByAgent: {
        "claude-code": [{
          agent_id: "claude-code",
          skill_id: deletedSkill.id,
          row_id: deletedSkill.row_id!,
          skill_name: deletedSkill.name,
          source_path: deletedSkill.dir_path,
          source_kind: "compatibility",
          state: "deleted",
          supported: true,
          can_toggle: false,
          can_delete: false,
          can_reapply: true,
          reason: null,
          requires_reload: true,
          scope: "name",
          affected_source_count: 1,
          adapter: "claude-skill-overrides",
          config_path: "/Users/test/.claude/settings.json",
        }],
      },
    });

    renderPlatformView();

    expect(screen.queryByRole("switch", { name: /deleted-public/i })).not.toBeInTheDocument();
    const reapply = screen.getByRole("button", { name: /重新应用|Reapply|再次应用/i });
    fireEvent.click(reapply);
    await waitFor(() => {
      expect(mockReapplyPlatformSkillControl).toHaveBeenCalledWith("claude-code", {
        skillId: deletedSkill.id,
        skillName: deletedSkill.name,
        sourcePath: deletedSkill.dir_path,
      });
    });
  });

  it("shows nested folder matches as individual cards while searching", async () => {
    window.localStorage.setItem("skills-manage.skillListViewMode.platform", "folders");
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({ skillsByAgent: { "claude-code": mockNestedPlatformSkills } });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView();
    fireEvent.change(screen.getByPlaceholderText(/검색 기술|搜索技能/), {
      target: { value: "nested-helper" },
    });

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /nested-helper.*详情|nested-helper.*detail/i })).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: /打开目录 toolkit|Open folder toolkit/i })).not.toBeInTheDocument();
    });
  });

  it("renders usage switches for managed platform skills", () => {
    renderPlatformView();

    expect(
      screen.getByRole("switch", { name: /切换 frontend-design 的激活状态/i })
    ).toBeInTheDocument();
    expect(
      screen.getByRole("switch", { name: /切换 code-reviewer 的激活状态/i })
    ).toBeInTheDocument();
  });

  it("changes usage without deleting the installed skill and refreshes counts", async () => {
    renderPlatformView();

    fireEvent.click(
      screen.getByRole("switch", { name: /切换 frontend-design 的激活状态/i })
    );

    await waitFor(() => {
      expect(mockSetSkillUsage).toHaveBeenCalledWith(
        "frontend-design",
        "claude-code",
        false
      );
    });
    expect(mockRefreshCounts).toHaveBeenCalledTimes(1);
  });

  it("pauses only active managed skills from a partially active platform", async () => {
    useSkillUsageStore.setState({
      statuses: [{
        agent_id: "claude-code",
        active_count: 1,
        paused_count: 1,
        external_count: 0,
        skills: [
          { skill_id: "frontend-design", name: "frontend-design", enabled: true, paused_by_bulk: false },
          { skill_id: "code-reviewer", name: "code-reviewer", enabled: false, paused_by_bulk: false },
        ],
      }],
    });

    renderPlatformView();

    const platformSwitch = screen.getByRole("switch", {
      name: /切换 Claude Code 中全部受管理技能的激活状态/i,
    });
    expect(platformSwitch).toBeChecked();
    expect(screen.getByText(/受管理技能: 部分激活/)).toBeInTheDocument();

    fireEvent.click(platformSwitch);

    await waitFor(() => {
      expect(mockSetPlatformUsage).toHaveBeenCalledWith("claude-code", false);
    });
  });

  it("restores only bulk-paused skills when no managed skill is active", async () => {
    useSkillUsageStore.setState({
      statuses: [{
        agent_id: "claude-code",
        active_count: 0,
        paused_count: 2,
        external_count: 0,
        skills: [
          { skill_id: "frontend-design", name: "frontend-design", enabled: false, paused_by_bulk: true },
          { skill_id: "code-reviewer", name: "code-reviewer", enabled: false, paused_by_bulk: false },
        ],
      }],
    });

    renderPlatformView();

    const platformSwitch = screen.getByRole("switch", {
      name: /切换 Claude Code 中全部受管理技能的激活状态/i,
    });
    expect(platformSwitch).not.toBeChecked();
    expect(platformSwitch).not.toBeDisabled();
    expect(screen.getByText("恢复到全部设为未激活前的状态。")).toBeInTheDocument();

    fireEvent.click(platformSwitch);

    await waitFor(() => {
      expect(mockSetPlatformUsage).toHaveBeenCalledWith("claude-code", true);
    });
  });

  it("does not send shared installs through ordinary platform bulk", async () => {
    const impact = {
      shared_install_id: "~/.agents/skills/frontend-design",
      skill_id: "frontend-design",
      skill_name: "frontend-design",
      enabled: true,
      confirmed_platforms: [{ agent_id: "claude-code", display_name: "Claude Code" }],
      separate_installs: [],
      reason: null,
      management_path: "~/.agents/skills/frontend-design",
      confirmation_token: "token-1",
    };
    const setSharedPlatformUsage = vi.fn();
    useSkillUsageStore.setState({
      setSharedPlatformUsage,
      loadSharedSkillImpact: mockLoadSharedSkillImpact,
      platformControlsByAgent: {
        "claude-code": [{
          agent_id: "claude-code",
          skill_id: "frontend-design",
          row_id: "claude-code::frontend-design",
          skill_name: "frontend-design",
          source_path: "~/.claude/skills/frontend-design",
          source_kind: "compatibility",
          state: "active",
          supported: true,
          can_toggle: true,
          can_delete: false,
          can_reapply: false,
          reason: null,
          requires_reload: false,
          scope: "path",
          affected_source_count: 1,
          adapter: "claude-skill-overrides",
          config_path: "/Users/test/.claude/settings.json",
          shared_install: impact,
          excluded_here: false,
        }],
      },
    });

    renderPlatformView();

    fireEvent.click(
      screen.getByRole("switch", {
        name: /切换 Claude Code 中全部受管理技能的激活状态/i,
      })
    );

    await waitFor(() => {
      expect(mockSetPlatformUsage).toHaveBeenCalledWith("claude-code", false);
    });
    expect(setSharedPlatformUsage).not.toHaveBeenCalled();
    expect(mockLoadSharedSkillImpact).not.toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "停用公用" })).not.toBeInTheDocument();
  });

  it("swallows a busy rejection from platform-wide usage without a false failure path", async () => {
    mockSetPlatformUsage.mockRejectedValue(new SkillUsageBusyError());
    renderPlatformView();
    fireEvent.click(
      screen.getByRole("switch", {
        name: /切换 Claude Code 中全部受管理技能的激活状态/i,
      })
    );
    await waitFor(() => {
      expect(mockSetPlatformUsage).toHaveBeenCalledWith("claude-code", false);
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("disables platform-wide restore when every managed skill was paused individually", () => {
    useSkillUsageStore.setState({
      statuses: [{
        agent_id: "claude-code",
        active_count: 0,
        paused_count: 2,
        external_count: 0,
        skills: [
          { skill_id: "frontend-design", name: "frontend-design", enabled: false, paused_by_bulk: false },
          { skill_id: "code-reviewer", name: "code-reviewer", enabled: false, paused_by_bulk: false },
        ],
      }],
    });

    renderPlatformView();

    const platformSwitch = screen.getByRole("switch", {
      name: /切换 Claude Code 中全部受管理技能的激活状态/i,
    });
    expect(platformSwitch).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByText("所有受管理技能均已单独设为未激活。")).toBeInTheDocument();

    fireEvent.click(platformSwitch);
    expect(mockSetPlatformUsage).not.toHaveBeenCalled();
  });

  it("keeps a paused managed row operable when a read-only plugin copy remains", async () => {
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockPluginBundleSkills },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    useSkillUsageStore.setState({
      statuses: [{
        agent_id: "claude-code",
        active_count: 0,
        paused_count: 1,
        external_count: 1,
        skills: [{
          skill_id: "ponytail-audit",
          name: "ponytail-audit",
          enabled: false,
          paused_by_bulk: false,
        }],
      }],
    });

    renderPlatformView();
    fireEvent.click(screen.getByRole("button", { name: /查看 .+ 的 \d+ 个来源位置/ }));

    const usageSwitch = screen.getByRole("switch", {
      name: /切换 ponytail-audit 的激活状态/i,
    });
    expect(usageSwitch).not.toBeChecked();
    expect(screen.getByText(/仍有 1 个外部提供的技能可用/)).toBeInTheDocument();

    fireEvent.click(usageSwitch);

    await waitFor(() => {
      expect(mockSetSkillUsage).toHaveBeenCalledWith(
        "ponytail-audit",
        "claude-code",
        true
      );
    });
  });

  it("deletes a paused managed row even when only a read-only plugin copy remains", async () => {
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockPluginBundleSkills },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    useSkillUsageStore.setState({
      statuses: [{
        agent_id: "claude-code",
        active_count: 0,
        paused_count: 1,
        external_count: 1,
        skills: [{
          skill_id: "ponytail-audit",
          name: "ponytail-audit",
          enabled: false,
          paused_by_bulk: false,
        }],
      }],
    });

    renderPlatformView();
    fireEvent.click(screen.getByRole("button", { name: /查看 .+ 的 \d+ 个来源位置/ }));

    fireEvent.click(
      screen.getByRole("button", { name: "从 Claude Code 删除 ponytail-audit 安装" })
    );
    fireEvent.click(screen.getByRole("button", { name: "确认删除" }));

    await waitFor(() => {
      expect(mockDeleteSkillFromAgent).toHaveBeenCalledWith("ponytail-audit", "claude-code");
    });
  });

  it("confirms whole-platform deletion and explains that external skills remain", async () => {
    mockDeletePlatformInstallations.mockResolvedValue({
      deleted: ["frontend-design"],
      failed: [{ skill_id: "code-reviewer", error: "preserve failed" }],
    });
    useSkillUsageStore.setState({
      statuses: [{
        agent_id: "claude-code",
        active_count: 1,
        paused_count: 1,
        external_count: 1,
        skills: [
          { skill_id: "frontend-design", name: "frontend-design", enabled: true, paused_by_bulk: false },
          { skill_id: "code-reviewer", name: "code-reviewer", enabled: false, paused_by_bulk: false },
        ],
      }],
    });

    renderPlatformView();

    fireEvent.click(
      screen.getByRole("button", { name: "删除 Claude Code 的全部受管理安装" })
    );

    expect(screen.getByRole("dialog", { name: "删除 Claude Code 的受管理安装？" })).toBeInTheDocument();
    expect(screen.getByText("技能仓库中的原件会保留。")).toBeInTheDocument();
    expect(screen.getByText("不会删除其他来源的 1 个技能。这些技能不一定是公用安装。")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "删除受管理安装" }));

    await waitFor(() => {
      expect(mockDeletePlatformInstallations).toHaveBeenCalledWith("claude-code");
    });
  });

  it("disables whole-platform deletion when there are no managed installs", () => {
    useSkillUsageStore.setState({
      statuses: [{
        agent_id: "claude-code",
        active_count: 0,
        paused_count: 0,
        external_count: 1,
        skills: [],
      }],
    });

    renderPlatformView();

    expect(
      screen.getByRole("button", { name: "删除 Claude Code 的全部受管理安装" })
    ).toBeDisabled();
  });
  it("shows Claude-only source tabs with 全部 selected by default", () => {
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockClaudePluginSliceDuplicates },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView();
    fireEvent.click(screen.getByRole("button", { name: /查看 .+ 的 \d+ 个来源位置/ }));

    expect(screen.getByRole("tab", { name: claudeTabName("全部", 3) })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tab", { name: claudeTabName("用户来源", 1) })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: claudeTabName("插件来源", 2) })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /查看 shared-skill 的详情/i })).toHaveLength(3);
  });

  it("filters Claude rows by the active source tab and keeps duplicate rows visible inside the selected slice", async () => {
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockClaudePluginSliceDuplicates },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView();

    fireEvent.click(screen.getByRole("tab", { name: claudeTabName("插件来源", 2) }));

    fireEvent.click(screen.getByRole("button", { name: /查看 .+ 的 \d+ 个来源位置/ }));

    await waitFor(() => {
      expect(screen.getAllByRole("button", { name: /查看 shared-skill 的详情/i })).toHaveLength(2);
    });

    expect(getCardBadgeMatches(userSourceText)).toHaveLength(0);
    expect(getCardBadgeMatches(pluginSourceText)).toHaveLength(2);
    expect(getCardBadgeMatches(readOnlyText)).toHaveLength(2);

    fireEvent.click(screen.getByRole("tab", { name: claudeTabName("用户来源", 1) }));

    await waitFor(() => {
      expect(screen.getAllByRole("button", { name: /查看 shared-skill 的详情/i })).toHaveLength(1);
    });

    expect(getCardBadgeMatches(userSourceText)).toHaveLength(1);
    expect(getCardBadgeMatches(pluginSourceText)).toHaveLength(0);
    expect(getCardBadgeMatches(readOnlyText)).toHaveLength(0);
  });

  it("searches only inside the active Claude source tab", async () => {
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockClaudePluginSliceDuplicates },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView();

    fireEvent.click(screen.getByRole("tab", { name: claudeTabName("用户来源", 1) }));
    fireEvent.change(screen.getByPlaceholderText(/搜索技能/), {
      target: { value: "shared-skill" },
    });

    await waitFor(() => {
      expect(screen.getAllByRole("button", { name: /查看 shared-skill 的详情/i })).toHaveLength(1);
    });

    expect(getCardBadgeMatches(userSourceText)).toHaveLength(1);
    expect(getCardBadgeMatches(pluginSourceText)).toHaveLength(0);
    expect(getCardBadgeMatches(readOnlyText)).toHaveLength(0);
  });

  it("searching by duplicated Claude skill id keeps both source rows and badges visible", async () => {
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockDuplicateClaudeSkillsWithDistinctIds },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView();

    fireEvent.change(screen.getByPlaceholderText(/搜索技能/), {
      target: { value: "shared-skill-id" },
    });

    fireEvent.click(screen.getByRole("button", { name: /查看 .+ 的 \d+ 个来源位置/ }));

    await waitFor(() => {
      expect(
        screen.getAllByRole("button", { name: /查看 Shared skill 的详情/i })
      ).toHaveLength(2);
    });

    expect(getCardBadgeMatches(userSourceText)).toHaveLength(1);
    expect(getCardBadgeMatches(pluginSourceText)).toHaveLength(1);
    expect(getCardBadgeMatches(readOnlyText)).toHaveLength(1);
  });

  it("does not render Claude source tabs on non-Claude platform pages", () => {
    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      const state = buildPlatformStoreState({
        agents: [mockAgent, mockCursorAgent],
        skillsByAgent: {
          "claude-code": mockSkills.length,
          cursor: mockCursorSkills.length,
        },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: {
          "claude-code": mockSkills,
          cursor: mockCursorSkills,
        },
        loadingByAgent: {
          "claude-code": false,
          cursor: false,
        },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView("cursor");

    expect(screen.queryByRole("tab", { name: claudeTabName("全部") })).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: claudeTabName("用户来源") })).not.toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: claudeTabName("插件来源") })).not.toBeInTheDocument();
  });

  it("preserves platform search and scroll state when closing the drawer and restores focus", async () => {
    renderPlatformView();

    const searchInput = screen.getByPlaceholderText(/搜索技能/);
    fireEvent.change(searchInput, { target: { value: "frontend" } });

    const scroller = searchInput.closest(".flex.flex-col.h-full")?.querySelector(".flex-1.overflow-auto.p-6");
    expect(scroller).not.toBeNull();
    if (!scroller) return;
    (scroller as HTMLDivElement).scrollTop = 180;

    const trigger = screen.getByRole("button", { name: /查看 frontend-design 的详情/i });
    fireEvent.click(trigger);

    await waitFor(() => {
      expect(screen.getByTestId("skill-detail-drawer")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: /close drawer/i }));

    await waitFor(() => {
      expect(screen.queryByTestId("skill-detail-drawer")).not.toBeInTheDocument();
    });

    expect(searchInput).toHaveValue("frontend");
    expect((scroller as HTMLDivElement).scrollTop).toBe(180);
    expect(trigger).toHaveFocus();
  });

  it("restores focus to the originating duplicate Claude row trigger", async () => {
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": mockDuplicateClaudeSkills },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView();
    fireEvent.click(screen.getByRole("button", { name: /查看 .+ 的 \d+ 个来源位置/ }));

    const [userTrigger] = screen.getAllByRole("button", {
      name: /查看 shared-skill 的详情/i,
    });
    fireEvent.click(userTrigger);

    await waitFor(() => {
      expect(screen.getByTestId("skill-detail-drawer")).toBeInTheDocument();
    });

    fireEvent.click(screen.getByRole("button", { name: /close drawer/i }));

    await waitFor(() => {
      expect(screen.queryByTestId("skill-detail-drawer")).not.toBeInTheDocument();
    });

    expect(userTrigger).toHaveFocus();
  });

  it("re-fetches the live Claude list after a scan generation change and removes stale duplicate rows without clearing the search query", async () => {
    let platformState = buildPlatformStoreState({
      scanGeneration: 1,
      skillsByAgent: { "claude-code": 2 },
    });
    let skillState = buildSkillStoreState({
      skillsByAgent: { "claude-code": mockDuplicateClaudeSkillsWithDistinctIds },
    });

    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      if (typeof selector === "function") return selector(platformState);
      return platformState;
    });
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      if (typeof selector === "function") return selector(skillState);
      return skillState;
    });

    const view = renderPlatformView();

    const searchInput = screen.getByPlaceholderText(/搜索技能/);
    fireEvent.change(searchInput, { target: { value: "shared-skill-id" } });

    fireEvent.click(screen.getByRole("button", { name: /查看 .+ 的 \d+ 个来源位置/ }));

    await waitFor(() => {
      expect(
        screen.getAllByRole("button", { name: /查看 Shared skill 的详情/i })
      ).toHaveLength(2);
    });

    mockGetSkillsByAgent.mockClear();

    platformState = buildPlatformStoreState({
      scanGeneration: 2,
      skillsByAgent: { "claude-code": 2 },
    });
    skillState = buildSkillStoreState({
      skillsByAgent: {
        "claude-code": [
          mockDuplicateClaudeSkillsWithDistinctIds[1],
          {
            id: "other-skill",
            name: "Other skill",
            description: "Non-matching survivor",
            file_path: "~/.claude/skills/other-skill/SKILL.md",
            dir_path: "~/.claude/skills/other-skill",
            link_type: "native",
            is_central: false,
            source_kind: "user",
            source_root: "~/.claude/skills",
            is_read_only: false,
          },
        ],
      },
    });

    view.rerender(
      <MemoryRouter initialEntries={["/platform/claude-code"]}>
        <Routes>
          <Route path="/platform/:agentId" element={<PlatformView />} />
        </Routes>
      </MemoryRouter>
    );

    await waitFor(() => {
      expect(mockGetSkillsByAgent).toHaveBeenCalledWith("claude-code");
    });

    expect(searchInput).toHaveValue("shared-skill-id");
    expect(
      screen.getAllByRole("button", { name: /查看 Shared skill 的详情/i })
    ).toHaveLength(1);
    expect(getCardBadgeMatches(userSourceText)).toHaveLength(0);
    expect(getCardBadgeMatches(pluginSourceText)).toHaveLength(1);
    expect(getCardBadgeMatches(readOnlyText)).toHaveLength(1);
    expect(screen.queryByText("Other skill")).not.toBeInTheDocument();
  });

  it("resets the platform content scroll when navigating to another platform", async () => {
    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      const state = buildPlatformStoreState({
        agents: [mockAgent, mockCursorAgent],
        skillsByAgent: {
          "claude-code": mockSkills.length,
          cursor: mockCursorSkills.length,
        },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: {
          "claude-code": mockSkills,
          cursor: mockCursorSkills,
        },
        loadingByAgent: {
          "claude-code": false,
          cursor: false,
        },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    render(
      <MemoryRouter initialEntries={["/platform/claude-code"]}>
        <NavigationHarness />
        <Routes>
          <Route path="/platform/:agentId" element={<PlatformView />} />
        </Routes>
      </MemoryRouter>
    );

    expect(screen.getByText("Claude Code")).toBeInTheDocument();

    const searchInput = screen.getByPlaceholderText(/搜索技能/);
    const scroller = searchInput
      .closest(".flex.flex-col.h-full")
      ?.querySelector(".flex-1.overflow-auto.p-6");
    expect(scroller).not.toBeNull();
    if (!scroller) return;

    (scroller as HTMLDivElement).scrollTop = 180;

    await act(async () => {
      testNavigate?.("/platform/cursor");
    });

    await waitFor(() => {
      expect(screen.getByText("Cursor")).toBeInTheDocument();
    });

    await waitFor(() => {
      expect((scroller as HTMLDivElement).scrollTop).toBe(0);
    });
  });
});

// ─── 공용 설치 판정 ────────────────────────────────────────────────────────────
// compatibility(다른 경로에서 읽음)와 공용 설치는 다른 개념이다.
// 출처 경로가 실제 공용 설치 경로와 같을 때만 공용으로 분류되어야 한다.

describe("PlatformView 공용 설치 판정", () => {
  const universalRoot = "/Users/test/.agents/skills";
  const universalBadgeText = "共享安装";
  const manageUniversalText = "管理共享安装";
  const compatibilityBadgeText = /兼容来源/;
  const installSourceTablist = "安装来源筛选";

  const universalAgent: AgentWithStatus = {
    id: "universal",
    display_name: "Universal (.agents)",
    category: "shared",
    global_skills_dir: universalRoot,
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  };

  const platforms = [
    { id: "codex", dir: "/Users/test/.agents/skills" },
    { id: "claude-code", dir: "/Users/test/.claude/skills" },
    { id: "cursor", dir: "/Users/test/.cursor/skills" },
    { id: "gemini-cli", dir: "/Users/test/.gemini/config/skills" },
  ];

  // 공용 설치 경로가 아닌 전용 경로들. 플랫폼 기본 경로와 임의 경로를 함께 넣는다.
  const dedicatedRoots = [
    "/Users/test/.codex/skills",
    "/Users/test/.claude/skills",
    "/Users/test/.cursor/skills",
    "/Users/test/.gemini/config/skills",
    "/Users/test/.custom-tool/skills",
  ];

  function platformAgent(id: string, dir: string): AgentWithStatus {
    return {
      id,
      display_name: id,
      category: "coding",
      global_skills_dir: dir,
      is_detected: true,
      is_builtin: true,
      is_enabled: true,
    };
  }

  function compatibilitySkill(overrides: Partial<ScannedSkill> = {}): ScannedSkill {
    return {
      id: "tdd",
      name: "tdd",
      description: "Test-driven development workflow",
      file_path: "/Users/test/.codex/skills/tdd/SKILL.md",
      dir_path: "/Users/test/.codex/skills/tdd",
      link_type: "copy",
      is_central: false,
      source_kind: "compatibility",
      source_root: "/Users/test/.codex/skills",
      is_read_only: true,
      ...overrides,
    };
  }

  function renderPlatformSkills(
    agent: AgentWithStatus,
    skills: ScannedSkill[],
    universalDir: string = universalRoot
  ) {
    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      const state = buildPlatformStoreState({
        agents: [agent, { ...universalAgent, global_skills_dir: universalDir }],
        skillsByAgent: { [agent.id]: skills.length },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { [agent.id]: skills },
        loadingByAgent: { [agent.id]: false },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    return renderPlatformView(agent.id);
  }

  function expectNoUniversalMarkers() {
    expect(
      screen.queryByRole("button", { name: manageUniversalText })
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: universalBadgeText })
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("tablist", { name: installSourceTablist })
    ).not.toBeInTheDocument();
  }

  beforeEach(() => {
    vi.clearAllMocks();
    window.localStorage.clear();
    testNavigate = null;
    mockLoadUsageStatus.mockReset().mockResolvedValue(undefined);
    mockSetSkillUsage.mockReset().mockResolvedValue(undefined);
    mockSetPlatformUsage.mockReset().mockResolvedValue(undefined);
    mockDeleteSkillFromAgent.mockReset().mockResolvedValue(undefined);
    mockDeletePlatformInstallations.mockReset().mockResolvedValue({ deleted: [], failed: [] });
    mockLoadPlatformSkillControls.mockReset().mockResolvedValue(undefined);
    mockSetPlatformSkillControl.mockReset().mockResolvedValue(undefined);
    mockDeletePlatformSkillControl.mockReset().mockResolvedValue(undefined);
    mockReapplyPlatformSkillControl.mockReset().mockResolvedValue(undefined);
    useSkillUsageStore.setState({
      statuses: [],
      isLoading: false,
      updatingSkillKeys: {},
      updatingAgentIds: {},
      error: null,
      loadUsageStatus: mockLoadUsageStatus,
      setSkillUsage: mockSetSkillUsage,
      setPlatformUsage: mockSetPlatformUsage,
      deleteSkillFromAgent: mockDeleteSkillFromAgent,
      deletePlatformInstallations: mockDeletePlatformInstallations,
      platformControlsByAgent: {},
      updatingPlatformControlKeys: {},
      loadPlatformSkillControls: mockLoadPlatformSkillControls,
      setPlatformSkillControl: mockSetPlatformSkillControl,
      deletePlatformSkillControl: mockDeletePlatformSkillControl,
      reapplyPlatformSkillControl: mockReapplyPlatformSkillControl,
      sharedImpactsById: {},
      updatingSharedKeys: {},
      updatingSharedBulk: false,
      loadSharedSkillImpact: mockLoadSharedSkillImpact,
      setSharedSkillUsage: mockSetSharedSkillUsage,
    });
    installDefaultStoreMocks();
  });

  const dedicatedCases: Array<[string, string, string]> = platforms.flatMap((platform) =>
    dedicatedRoots.map(
      (root): [string, string, string] => [platform.id, platform.dir, root]
    )
  );

  it.each(dedicatedCases)(
    "%s 화면에서 전용 경로(%s) 출처는 공용으로 분류하지 않는다 [출처 %s]",
    (platformId, platformDir, sourceRoot) => {
      renderPlatformSkills(platformAgent(platformId, platformDir), [
        compatibilitySkill({ source_root: sourceRoot }),
      ]);

      expect(getCardBadgeMatches(compatibilityBadgeText)).toHaveLength(1);
      expect(getCardBadgeMatches(readOnlyText)).toHaveLength(1);
      expectNoUniversalMarkers();
    }
  );

  it.each(platforms.map((platform) => [platform.id, platform.dir]))(
    "%s 화면에서 공용 경로 출처만 공용으로 분류한다",
    (platformId, platformDir) => {
      renderPlatformSkills(platformAgent(platformId, platformDir), [
        compatibilitySkill({ source_root: universalRoot }),
        compatibilitySkill({
          id: "custom",
          name: "custom",
          source_root: "/Users/test/.custom-tool/skills",
        }),
        compatibilitySkill({ id: "unknown", name: "unknown", source_root: null }),
      ]);

      expect(getCardBadgeMatches(compatibilityBadgeText)).toHaveLength(3);
      expect(getCardBadgeMatches(readOnlyText)).toHaveLength(2);
      expect(screen.getAllByRole("button", { name: universalBadgeText })).toHaveLength(1);
      expect(screen.getAllByRole("button", { name: manageUniversalText })).toHaveLength(1);
      expect(screen.getByRole("tab", { name: claudeTabName(universalBadgeText, 1) })).toBeInTheDocument();
    }
  );

  it("경로 표기 차이(끝 슬래시·구분자)를 정규화해 같은 공용 경로로 본다", () => {
    renderPlatformSkills(
      platformAgent("codex", "C:\\Users\\test\\.codex\\skills"),
      [compatibilitySkill({ source_root: "C:/Users/test/.agents/skills" })],
      "C:\\Users\\test\\.agents\\skills\\"
    );

    expect(screen.getAllByRole("button", { name: universalBadgeText })).toHaveLength(1);
    expect(screen.getAllByRole("button", { name: manageUniversalText })).toHaveLength(1);
  });

  it("사용자 지정 공용 경로를 기준으로 삼고 기본 경로는 공용으로 보지 않는다", () => {
    renderPlatformSkills(
      platformAgent("cursor", "/Users/test/.cursor/skills"),
      [
        compatibilitySkill({ source_root: "/Users/test/custom/shared-skills" }),
        compatibilitySkill({
          id: "legacy",
          name: "legacy",
          source_root: "/Users/test/.agents/skills",
        }),
      ],
      "/Users/test/custom/shared-skills"
    );

    expect(screen.getAllByRole("button", { name: universalBadgeText })).toHaveLength(1);
    expect(getCardBadgeMatches(readOnlyText)).toHaveLength(1);
    expect(screen.getByRole("tab", { name: claudeTabName(universalBadgeText, 1) })).toBeInTheDocument();
  });

  it("공용 설치 경로를 알 수 없으면 공용으로 추정하지 않는다", () => {
    mockUsePlatformStore.mockImplementation((selector?: unknown) => {
      const state = buildPlatformStoreState({
        agents: [platformAgent("claude-code", "/Users/test/.claude/skills")],
        skillsByAgent: { "claude-code": 1 },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });
    mockUseSkillStore.mockImplementation((selector?: unknown) => {
      const state = buildSkillStoreState({
        skillsByAgent: { "claude-code": [compatibilitySkill({ source_root: universalRoot })] },
        loadingByAgent: { "claude-code": false },
      });
      if (typeof selector === "function") return selector(state);
      return state;
    });

    renderPlatformView("claude-code");

    expect(getCardBadgeMatches(readOnlyText)).toHaveLength(1);
    expectNoUniversalMarkers();
  });
});

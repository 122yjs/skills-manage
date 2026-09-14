import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/stores/platformStore", () => ({ usePlatformStore: vi.fn() }));
vi.mock("@/stores/skillStore", () => ({ useSkillStore: vi.fn() }));
vi.mock("@/stores/storageStore", () => ({ useStorageStore: vi.fn() }));
vi.mock("@/components/skill/SkillDetailDrawer", () => ({
  SkillDetailDrawer: () => null,
}));

import { UniversalInstallView } from "@/pages/UniversalInstallView";
import { usePlatformStore } from "@/stores/platformStore";
import { useSkillStore } from "@/stores/skillStore";
import { useStorageStore } from "@/stores/storageStore";
import { useSkillUsageStore } from "@/stores/skillUsageStore";
import type {
  AgentWithStatus,
  PlatformSkillControlStatus,
  ScannedSkill,
  SharedSkillImpact,
} from "@/types";

const universalAgent: AgentWithStatus = {
  id: "universal",
  display_name: "Universal (.agents)",
  category: "shared",
  global_skills_dir: "/Users/test/.agents/skills",
  is_detected: true,
  is_builtin: true,
  is_enabled: true,
};

const managedSkill: ScannedSkill = {
  id: "managed",
  name: "Managed skill",
  file_path: "/Users/test/.agents/skills/managed/SKILL.md",
  dir_path: "/Users/test/.agents/skills/managed",
  link_type: "symlink",
  is_central: true,
  is_read_only: false,
};

const unmanagedSkill: ScannedSkill = {
  id: "manual",
  name: "Manual skill",
  file_path: "/Users/test/.agents/skills/manual/SKILL.md",
  dir_path: "/Users/test/.agents/skills/manual",
  link_type: "native",
  is_central: false,
  is_read_only: true,
  source_kind: "unmanaged",
};
const universalSkills = [managedSkill, unmanagedSkill];

const getSkillsByAgent = vi.fn().mockResolvedValue(undefined);
const uninstallSkillFromAgent = vi.fn().mockResolvedValue(undefined);
const refreshCounts = vi.fn().mockResolvedValue(undefined);
const loadUsageStatus = vi.fn().mockResolvedValue(undefined);
const loadPlatformSkillControls = vi.fn().mockResolvedValue(undefined);
const deleteSkillFromAgent = vi.fn().mockResolvedValue(undefined);
const deletePlatformInstallations = vi.fn().mockResolvedValue({
  deleted: ["managed", "paused"],
  failed: [],
});

describe("UniversalInstallView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useSkillUsageStore.setState({
      statuses: [{
        agent_id: "universal",
        active_count: 1,
        paused_count: 1,
        external_count: 1,
        skills: [
          { skill_id: "managed", name: "Managed skill", enabled: true, paused_by_bulk: false },
          { skill_id: "paused", name: "Paused skill", enabled: false, paused_by_bulk: false },
        ],
      }],
      isLoading: false,
      updatingSkillKeys: {},
      updatingAgentIds: {},
      error: null,
      platformControlsByAgent: {},
      sharedImpactsById: {},
      updatingSharedKeys: {},
      updatingSharedBulk: false,
      loadUsageStatus,
      loadPlatformSkillControls,
      deleteSkillFromAgent,
      deletePlatformInstallations,
    });
    vi.mocked(usePlatformStore).mockImplementation((selector) =>
      selector({
        agents: [universalAgent],
        skillsByAgent: { universal: 2 },
        isLoading: false,
        isRefreshing: false,
        updatingAgentIds: {},
        scanGeneration: 1,
        error: null,
        initialize: vi.fn(),
        rescan: vi.fn(),
        refreshCounts,
        setAgentVisibility: vi.fn(),
        setAllAgentsVisibility: vi.fn(),
      })
    );
    vi.mocked(useSkillStore).mockImplementation((selector) =>
      selector({
        skillsByAgent: { universal: universalSkills },
        loadingByAgent: { universal: false },
        pendingSkillActionKeys: {},
        error: null,
        getSkillsByAgent,
        uninstallSkillFromAgent,
      })
    );
    vi.mocked(useStorageStore).mockImplementation((selector) =>
      selector({
        status: {
          central_path: "/Volumes/My Skill Library",
          default_central_path: "/Users/test/.skillsmanage/skills",
          legacy_path: "/Users/test/.agents/skills",
          universal_path: "/Users/test/.agents/skills",
          migration_state: "completed",
          migration_required: false,
          legacy_skill_count: 0,
        },
      } as never)
    );
  });

  it("shows the shared target and marks unmanaged filesystem entries", () => {
    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    expect(screen.getByRole("heading", { name: "共享安装 (.agents)" })).toBeInTheDocument();
    expect(screen.getByText("手动管理")).toBeInTheDocument();
    expect(screen.getAllByRole("checkbox")).toHaveLength(3);
    expect(screen.queryByRole("button", { name: /删除 Manual skill 的共享安装/ })).not.toBeInTheDocument();
  });

  it("외부 설치도 스캔 후 공용 상태를 바꾸고 삭제할 수 있다", async () => {
    const skill = { ...unmanagedSkill, is_read_only: false, source_kind: undefined };
    vi.mocked(useSkillStore).mockImplementation((selector) => selector({
      skillsByAgent: { universal: [skill] },
      loadingByAgent: { universal: false },
      pendingSkillActionKeys: {},
      getSkillsByAgent,
    } as never));
    const impact = {
      ...bulkImpactA,
      shared_install_id: skill.dir_path,
      skill_id: skill.id,
      skill_name: skill.name,
      management_path: skill.dir_path,
    };
    const setSharedSkillUsage = vi.fn().mockResolvedValue({
      applied: true,
      impact: { ...impact, enabled: false },
    });
    useSkillUsageStore.setState({
      statuses: [{
        agent_id: "universal", active_count: 1, paused_count: 0, external_count: 0,
        skills: [{ skill_id: skill.id, name: skill.name, enabled: true, paused_by_bulk: false }],
      }],
      platformControlsByAgent: { universal: [{
        ...universalSharedControls().universal[0],
        row_id: skill.id,
        skill_id: skill.id,
        skill_name: skill.name,
        source_path: skill.dir_path,
        shared_install: impact,
      }] },
      loadSharedSkillImpact: vi.fn().mockResolvedValue(impact),
      setSharedSkillUsage,
    });

    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    expect(screen.queryByText("手动管理")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "删除 Manual skill 的共享安装" })).toBeEnabled();
    const toggle = screen.getByRole("switch", { name: "切换 Manual skill 的公用状态" });
    expect(toggle).toBeEnabled();
    fireEvent.click(toggle);
    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
    expect(setSharedSkillUsage).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "停用公用" }));
    await waitFor(() => expect(setSharedSkillUsage).toHaveBeenCalledWith(
      skill.dir_path, false, impact.confirmation_token
    ));
  });

  it("selects and deletes active and inactive managed entries while keeping the custom library path", async () => {
    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    fireEvent.click(screen.getByRole("checkbox", { name: "选择当前列表中的技能" }));
    fireEvent.click(screen.getByRole("button", { name: "删除选中的 2 项安装" }));

    expect(screen.getByText("/Volumes/My Skill Library")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "删除共享安装" }));

    await waitFor(() => {
      expect(deleteSkillFromAgent).toHaveBeenCalledWith("managed", "universal");
    });
    expect(deleteSkillFromAgent).toHaveBeenCalledWith("paused", "universal");
    expect(deleteSkillFromAgent).not.toHaveBeenCalledWith("manual", "universal");
  });

  it("deletes selected managed entries one at a time", async () => {
    let finishFirstDelete: (() => void) | undefined;
    deleteSkillFromAgent.mockImplementation((skillId: string) => {
      if (skillId === "managed") {
        return new Promise<void>((resolve) => {
          finishFirstDelete = resolve;
        });
      }
      return Promise.resolve();
    });

    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    fireEvent.click(screen.getByRole("checkbox", { name: "选择当前列表中的技能" }));
    fireEvent.click(screen.getByRole("button", { name: "删除选中的 2 项安装" }));
    fireEvent.click(screen.getByRole("button", { name: "删除共享安装" }));

    await waitFor(() => {
      expect(deleteSkillFromAgent).toHaveBeenCalledWith("managed", "universal");
    });
    expect(deleteSkillFromAgent).not.toHaveBeenCalledWith("paused", "universal");

    finishFirstDelete?.();

    await waitFor(() => {
      expect(deleteSkillFromAgent).toHaveBeenCalledWith("paused", "universal");
    });
  });

  it("confirms whole-platform deletion, keeps external skills visible, and calls the shared action", async () => {
    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    fireEvent.click(
      screen.getByRole("button", { name: "删除 共享安装 (.agents) 的全部受管理安装" })
    );

    expect(screen.getByRole("dialog", { name: "删除 共享安装 (.agents) 的受管理安装？" })).toBeInTheDocument();
    expect(screen.getByText("技能仓库中的原件会保留。")).toBeInTheDocument();
    expect(screen.getByText("不会删除其他来源的 1 个技能。这些技能不一定是公用安装。")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "删除受管理安装" }));

    await waitFor(() => {
      expect(deletePlatformInstallations).toHaveBeenCalledWith("universal");
    });
  });

  const bulkImpactA: SharedSkillImpact = {
    shared_install_id: "/Users/test/.agents/skills/managed",
    skill_id: "managed",
    skill_name: "Managed skill",
    enabled: true,
    confirmed_platforms: [{ agent_id: "claude-code", display_name: "Claude Code" }],
    separate_installs: [],
    reason: null,
    management_path: "/Users/test/.agents/skills/managed",
    confirmation_token: "tok-a",
  };
  const bulkImpactPaused: SharedSkillImpact = {
    shared_install_id: "/Users/test/.agents/skills/paused",
    skill_id: "paused",
    skill_name: "Paused skill",
    enabled: false,
    confirmed_platforms: [],
    separate_installs: [],
    reason: null,
    management_path: "/Users/test/.agents/skills/paused",
    confirmation_token: "tok-b",
  };
  function universalSharedControls(): { universal: PlatformSkillControlStatus[] } {
    const base = {
      agent_id: "universal",
      row_id: "universal::managed",
      state: "active",
      supported: true,
      can_toggle: true,
      can_delete: false,
      can_reapply: false,
      reason: null,
      requires_reload: false,
      scope: "path",
      affected_source_count: 1,
      adapter: "managed-installation",
      config_path: null,
    };
    return {
      universal: [
        {
          ...base,
          skill_id: "managed",
          skill_name: "Managed skill",
          source_path: "/Users/test/.agents/skills/managed",
          shared_install: bulkImpactA,
          excluded_here: false,
        },
        {
          ...base,
          row_id: "universal::paused",
          skill_id: "paused",
          skill_name: "Paused skill",
          source_path: "/Users/test/.agents/skills/paused",
          shared_install: bulkImpactPaused,
          excluded_here: false,
        },
      ],
    };
  }

  it("confirms the universal bulk change with fresh tokens instead of toggling directly", async () => {
    const loadSharedSkillImpact = vi.fn(async (id: string) =>
      id === bulkImpactA.shared_install_id ? bulkImpactA : bulkImpactPaused
    );
    const setSharedPlatformUsage = vi.fn().mockResolvedValue({
      applied: true,
      impacts: [{ ...bulkImpactA, enabled: false }],
      failed: [],
    });
    useSkillUsageStore.setState({
      platformControlsByAgent: universalSharedControls(),
      loadSharedSkillImpact,
      setSharedPlatformUsage,
    });

    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    fireEvent.click(
      screen.getByRole("switch", { name: "切换 共享安装 (.agents) 中全部受管理技能的激活状态" })
    );

    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
    expect(loadSharedSkillImpact).toHaveBeenCalledWith(bulkImpactA.shared_install_id);
    expect(screen.getByRole("button", { name: "停用公用" })).toBeEnabled();
    expect(
      screen.getByText("即使不在列表中，读取此路径的其他工具也可能受到影响。")
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "停用公用" }));

    await waitFor(() => {
      expect(setSharedPlatformUsage).toHaveBeenCalledWith(false, [
        {

          shared_install_id: bulkImpactA.shared_install_id,
          confirmation_token: "tok-a",
        },
      ]);
    });
  });

  it("preflights only active installs when pausing and skips individually paused rows", async () => {
    const loadSharedSkillImpact = vi.fn(async (id: string) =>
      id === bulkImpactA.shared_install_id ? bulkImpactA : bulkImpactPaused
    );
    const setSharedPlatformUsage = vi.fn().mockResolvedValue({
      applied: true,
      impacts: [{ ...bulkImpactA, enabled: false }],
      failed: [],
    });
    useSkillUsageStore.setState({
      platformControlsByAgent: universalSharedControls(),
      loadSharedSkillImpact,
      setSharedPlatformUsage,
    });

    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    fireEvent.click(
      screen.getByRole("switch", { name: "切换 共享安装 (.agents) 中全部受管理技能的激活状态" })
    );

    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
    expect(loadSharedSkillImpact).toHaveBeenCalledWith(bulkImpactA.shared_install_id);
    expect(loadSharedSkillImpact).not.toHaveBeenCalledWith(bulkImpactPaused.shared_install_id);
    expect(screen.getByRole("button", { name: "停用公用" })).toBeEnabled();
  });

  it("restores only bulk-paused installs", async () => {
    useSkillUsageStore.setState({
      statuses: [{
        agent_id: "universal",
        active_count: 0,
        paused_count: 2,
        external_count: 0,
        skills: [
          { skill_id: "managed", name: "Managed skill", enabled: false, paused_by_bulk: true },
          { skill_id: "paused", name: "Paused skill", enabled: false, paused_by_bulk: false },
        ],
      }],
    });
    const loadSharedSkillImpact = vi.fn(async (id: string) =>
      id === bulkImpactA.shared_install_id ? bulkImpactA : bulkImpactPaused
    );
    const setSharedPlatformUsage = vi.fn().mockResolvedValue({
      applied: true,
      impacts: [{ ...bulkImpactA, enabled: true }],
      failed: [],
    });
    useSkillUsageStore.setState({
      platformControlsByAgent: universalSharedControls(),
      loadSharedSkillImpact,
      setSharedPlatformUsage,
    });

    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    fireEvent.click(
      screen.getByRole("switch", { name: "切换 共享安装 (.agents) 中全部受管理技能的激活状态" })
    );

    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
    expect(loadSharedSkillImpact).toHaveBeenCalledWith(bulkImpactA.shared_install_id);
    expect(loadSharedSkillImpact).not.toHaveBeenCalledWith(bulkImpactPaused.shared_install_id);
    expect(screen.getByRole("button", { name: "启用公用" })).toBeEnabled();

    fireEvent.click(screen.getByRole("button", { name: "启用公用" }));

    await waitFor(() => {
      expect(setSharedPlatformUsage).toHaveBeenCalledWith(true, [
        {
          shared_install_id: bulkImpactA.shared_install_id,
          confirmation_token: "tok-a",
        },
      ]);
    });
  });

  it("preflights a source_path when shared_install is missing and confirms the returned canonical id", async () => {
    const soloImpact: SharedSkillImpact = {
      ...bulkImpactA,
      shared_install_id: "/Users/test/.agents/skills/solo",
      skill_id: "solo",
      skill_name: "Solo skill",
      confirmation_token: "tok-solo",
    };
    useSkillUsageStore.setState({
      statuses: [{
        agent_id: "universal",
        active_count: 2,
        paused_count: 1,
        external_count: 0,
        skills: [
          { skill_id: "managed", name: "Managed skill", enabled: true, paused_by_bulk: false },
          { skill_id: "solo", name: "Solo skill", enabled: true, paused_by_bulk: false },
          { skill_id: "paused", name: "Paused skill", enabled: false, paused_by_bulk: false },
        ],
      }],
    });
    const loadSharedSkillImpact = vi.fn(async (id: string) => {
      if (id === soloImpact.shared_install_id) return soloImpact;
      if (id === bulkImpactA.shared_install_id) return bulkImpactA;
      return bulkImpactPaused;
    });
    const setSharedPlatformUsage = vi.fn().mockResolvedValue({
      applied: true,
      impacts: [bulkImpactA, soloImpact],
      failed: [],
    });
    const controls = universalSharedControls();
    const soloControl: PlatformSkillControlStatus = {
      ...controls.universal[0],
      row_id: "universal::solo",
      skill_id: "solo",
      skill_name: "Solo skill",
      source_path: "/Users/test/.agents/skills/solo",
      shared_install: null,
      excluded_here: false,
    };
    controls.universal.push(soloControl);
    useSkillUsageStore.setState({
      platformControlsByAgent: controls,
      loadSharedSkillImpact,
      setSharedPlatformUsage,
    });

    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    fireEvent.click(
      screen.getByRole("switch", { name: "切换 共享安装 (.agents) 中全部受管理技能的激活状态" })
    );

    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
    expect(loadSharedSkillImpact).toHaveBeenCalledWith(bulkImpactA.shared_install_id);
    expect(loadSharedSkillImpact).toHaveBeenCalledWith(soloImpact.shared_install_id);
    expect(screen.getByRole("button", { name: "停用公用" })).toBeEnabled();

    fireEvent.click(screen.getByRole("button", { name: "停用公用" }));

    await waitFor(() => {
      expect(setSharedPlatformUsage).toHaveBeenCalledWith(false, [
        {
          shared_install_id: bulkImpactA.shared_install_id,
          confirmation_token: "tok-a",
        },
        {
          shared_install_id: soloImpact.shared_install_id,
          confirmation_token: "tok-solo",
        },
      ]);
    });
  });

  it("blocks bulk confirmation when a target has neither shared_install nor source_path", async () => {
    const loadSharedSkillImpact = vi.fn();
    const setSharedPlatformUsage = vi.fn();
    useSkillUsageStore.setState({
      statuses: [{
        agent_id: "universal",
        active_count: 2,
        paused_count: 0,
        external_count: 0,
        skills: [
          { skill_id: "managed", name: "Managed skill", enabled: true, paused_by_bulk: false },
          { skill_id: "ghost", name: "Ghost skill", enabled: true, paused_by_bulk: false },
        ],
      }],
      platformControlsByAgent: universalSharedControls(),
      loadSharedSkillImpact,
      setSharedPlatformUsage,
    });

    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    fireEvent.click(
      screen.getByRole("switch", { name: "切换 共享安装 (.agents) 中全部受管理技能的激活状态" })
    );

    await waitFor(() => {
      expect(loadSharedSkillImpact).not.toHaveBeenCalled();
      expect(setSharedPlatformUsage).not.toHaveBeenCalled();
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    });
  });

  it("prefers the backend shared_install id over a different source_path", async () => {
    const canonical: SharedSkillImpact = {
      ...bulkImpactA,
      shared_install_id: "/canonical/managed",
    };
    const loadSharedSkillImpact = vi.fn(async (id: string) =>
      id === canonical.shared_install_id ? canonical : bulkImpactPaused
    );
    const setSharedPlatformUsage = vi.fn().mockResolvedValue({
      applied: true,
      impacts: [{ ...canonical, enabled: false }],
      failed: [],
    });
    const controls = universalSharedControls();
    const managed: PlatformSkillControlStatus = {
      ...controls.universal[0],
      source_path: "/raw/managed",
      shared_install: canonical,
    };
    controls.universal[0] = managed;
    useSkillUsageStore.setState({
      platformControlsByAgent: controls,
      loadSharedSkillImpact,
      setSharedPlatformUsage,
    });

    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    fireEvent.click(
      screen.getByRole("switch", { name: "切换 共享安装 (.agents) 中全部受管理技能的激活状态" })
    );

    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
    expect(loadSharedSkillImpact).toHaveBeenCalledWith(canonical.shared_install_id);
    expect(loadSharedSkillImpact).not.toHaveBeenCalledWith("/raw/managed");

    fireEvent.click(screen.getByRole("button", { name: "停用公用" }));

    await waitFor(() => {
      expect(setSharedPlatformUsage).toHaveBeenCalledWith(false, [
        {
          shared_install_id: canonical.shared_install_id,
          confirmation_token: "tok-a",
        },
      ]);
    });
  });

  it("surfaces a restricted bulk member and blocks confirm instead of skipping it", async () => {
    const restricted: SharedSkillImpact = {
      ...bulkImpactA,
      reason: "vault overlaps the install entry",
    };
    const loadSharedSkillImpact = vi.fn(async () => restricted);
    const setSharedPlatformUsage = vi.fn();
    useSkillUsageStore.setState({
      platformControlsByAgent: universalSharedControls(),
      loadSharedSkillImpact,
      setSharedPlatformUsage,
    });

    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    fireEvent.click(
      screen.getByRole("switch", { name: "切换 共享安装 (.agents) 中全部受管理技能的激活状态" })
    );

    await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "停用公用" })).toBeDisabled();
    expect(setSharedPlatformUsage).not.toHaveBeenCalled();
  });

  it("surfaces bulk lookup failure without confirming from stale cache", async () => {
    const loadSharedSkillImpact = vi.fn().mockRejectedValue(new Error("impact unavailable"));
    const setSharedPlatformUsage = vi.fn();
    useSkillUsageStore.setState({
      platformControlsByAgent: universalSharedControls(),
      loadSharedSkillImpact,
      setSharedPlatformUsage,
    });

    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    fireEvent.click(
      screen.getByRole("switch", { name: "切换 共享安装 (.agents) 中全部受管理技能的激活状态" })
    );

    await waitFor(() => expect(loadSharedSkillImpact).toHaveBeenCalled());
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(setSharedPlatformUsage).not.toHaveBeenCalled();
  });
});

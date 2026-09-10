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
import type { AgentWithStatus, ScannedSkill } from "@/types";

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
      loadUsageStatus,
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

  it("selects and deletes active and inactive managed entries while keeping the custom library path", async () => {
    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    fireEvent.click(screen.getByRole("checkbox", { name: "全选" }));
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

    fireEvent.click(screen.getByRole("checkbox", { name: "全选" }));
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
    expect(screen.getByText("仍有 1 个外部提供的技能可用。")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "删除受管理安装" }));

    await waitFor(() => {
      expect(deletePlatformInstallations).toHaveBeenCalledWith("universal");
    });
  });
});

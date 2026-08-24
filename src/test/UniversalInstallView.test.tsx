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

describe("UniversalInstallView", () => {
  beforeEach(() => {
    vi.clearAllMocks();
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
        setAgentEnabled: vi.fn(),
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
    expect(screen.getAllByRole("checkbox")).toHaveLength(2);
    expect(screen.queryByRole("button", { name: /卸载 Manual skill 的共享安装/ })).not.toBeInTheDocument();
  });

  it("selects and removes only app-managed entries while keeping the custom library path", async () => {
    render(<MemoryRouter><UniversalInstallView /></MemoryRouter>);

    fireEvent.click(screen.getByRole("checkbox", { name: "全选" }));
    fireEvent.click(screen.getByRole("button", { name: "卸载选中的 1 项" }));

    expect(screen.getByText("/Volumes/My Skill Library")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "卸载共享安装" }));

    await waitFor(() => {
      expect(uninstallSkillFromAgent).toHaveBeenCalledWith("managed", "universal");
    });
    expect(uninstallSkillFromAgent).not.toHaveBeenCalledWith("manual", "universal");
  });
});

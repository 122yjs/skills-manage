import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/stores/storageStore", () => ({ useStorageStore: vi.fn() }));
vi.mock("@/stores/platformStore", () => ({ usePlatformStore: vi.fn() }));
vi.mock("@/stores/centralSkillsStore", () => ({ useCentralSkillsStore: vi.fn() }));

import { CentralVaultSettings } from "@/components/settings/CentralVaultSettings";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { usePlatformStore } from "@/stores/platformStore";
import { useStorageStore } from "@/stores/storageStore";

const previewPathChange = vi.fn().mockResolvedValue(undefined);
const changePath = vi.fn().mockResolvedValue({
  central_path: "/Volumes/Skills",
  skill_count: 4,
});
const rescan = vi.fn().mockResolvedValue(undefined);
const loadCentralSkills = vi.fn().mockResolvedValue(undefined);

describe("CentralVaultSettings", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(useStorageStore).mockImplementation((selector) =>
      selector({
        status: {
          central_path: "/Users/test/.skillsmanage/skills",
          default_central_path: "/Users/test/.skillsmanage/skills",
          legacy_path: "/Users/test/.agents/skills",
          universal_path: "/Users/test/.agents/skills",
          migration_state: "completed",
          migration_required: false,
          legacy_skill_count: 0,
        },
        preview: {
          current_path: "/Users/test/.skillsmanage/skills",
          new_path: "/Volumes/Skills",
          skill_count: 4,
          conflicts: [],
          can_proceed: true,
        },
        isLoading: false,
        isApplying: false,
        error: null,
        loadStatus: vi.fn(),
        previewPathChange,
        changePath,
        clearPreview: vi.fn(),
      } as never)
    );
    vi.mocked(usePlatformStore).mockImplementation((selector) => selector({ rescan } as never));
    vi.mocked(useCentralSkillsStore).mockImplementation((selector) =>
      selector({ loadCentralSkills } as never)
    );
  });

  it("previews and executes a safe library move", async () => {
    render(<CentralVaultSettings />);

    const input = screen.getByLabelText("当前仓库路径");
    fireEvent.change(input, { target: { value: "/Volumes/Skills" } });
    fireEvent.click(screen.getByRole("button", { name: "预览移动内容" }));

    await waitFor(() => {
      expect(previewPathChange).toHaveBeenCalledWith("/Volumes/Skills");
    });
    expect(screen.getByRole("dialog", { name: "移动技能仓库？" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "安全移动" }));

    await waitFor(() => expect(changePath).toHaveBeenCalledWith("/Volumes/Skills"));
    expect(rescan).toHaveBeenCalled();
    expect(loadCentralSkills).toHaveBeenCalled();
  });
});

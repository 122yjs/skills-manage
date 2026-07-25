import { render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/stores/storageStore", () => ({ useStorageStore: vi.fn() }));

import { LegacyMigrationNotice } from "@/components/layout/LegacyMigrationNotice";
import { useStorageStore } from "@/stores/storageStore";

const previewMigration = vi.fn().mockResolvedValue(undefined);

describe("LegacyMigrationNotice", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(useStorageStore).mockImplementation((selector) =>
      selector({
        status: {
          central_path: "/Users/test/.agents/skills",
          default_central_path: "/Users/test/.skillsmanage/skills",
          legacy_path: "/Users/test/.agents/skills",
          universal_path: "/Users/test/.agents/skills",
          migration_state: "pending",
          migration_required: true,
          legacy_skill_count: 2,
        },
        preview: null,
        isLoading: false,
        isApplying: false,
        error: null,
        previewMigration,
        deferMigration: vi.fn(),
        migrateLegacy: vi.fn(),
        clearPreview: vi.fn(),
      } as never)
    );
  });

  it("opens the first-run prompt and shows legacy to default paths before preview arrives", async () => {
    render(<LegacyMigrationNotice onMigrated={vi.fn()} />);

    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    expect(screen.getAllByText("/Users/test/.agents/skills")).toHaveLength(1);
    expect(screen.getByText("/Users/test/.skillsmanage/skills")).toBeInTheDocument();
    await waitFor(() => expect(previewMigration).toHaveBeenCalled());
  });
});

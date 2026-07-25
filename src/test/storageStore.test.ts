import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { useStorageStore } from "@/stores/storageStore";
import type { CentralVaultStatus, StoragePreview } from "@/types";

const status: CentralVaultStatus = {
  central_path: "/Users/test/.skillsmanage/skills",
  default_central_path: "/Users/test/.skillsmanage/skills",
  legacy_path: "/Users/test/.agents/skills",
  universal_path: "/Users/test/.agents/skills",
  migration_state: "pending",
  migration_required: true,
  legacy_skill_count: 3,
};

const preview: StoragePreview = {
  current_path: status.legacy_path,
  new_path: status.default_central_path,
  skill_count: 3,
  conflicts: [],
  can_proceed: true,
};

describe("storageStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useStorageStore.setState({
      status: null,
      preview: null,
      isLoading: false,
      isApplying: false,
      error: null,
    });
  });

  it("loads the current library and migration status", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(status);

    await useStorageStore.getState().loadStatus();

    expect(invoke).toHaveBeenCalledWith("get_central_vault_status");
    expect(useStorageStore.getState().status).toEqual(status);
  });

  it("previews a path change with the Tauri camelCase payload", async () => {
    vi.mocked(invoke).mockResolvedValueOnce({
      ...preview,
      current_path: status.central_path,
      new_path: "/Volumes/Skills",
    });

    await useStorageStore.getState().previewPathChange("/Volumes/Skills");

    expect(invoke).toHaveBeenCalledWith("preview_central_path_change", {
      newPath: "/Volumes/Skills",
    });
  });

  it("moves the library and refreshes authoritative status", async () => {
    const movedStatus = { ...status, central_path: "/Volumes/Skills" };
    vi.mocked(invoke)
      .mockResolvedValueOnce({ central_path: "/Volumes/Skills", skill_count: 3 })
      .mockResolvedValueOnce(movedStatus);

    await useStorageStore.getState().changePath("/Volumes/Skills");

    expect(invoke).toHaveBeenNthCalledWith(1, "change_central_path", {
      newPath: "/Volumes/Skills",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "get_central_vault_status");
    expect(useStorageStore.getState().status).toEqual(movedStatus);
  });

  it("keeps migration required but marks it deferred", async () => {
    useStorageStore.setState({ status });
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await useStorageStore.getState().deferMigration();

    expect(invoke).toHaveBeenCalledWith("defer_legacy_migration");
    expect(useStorageStore.getState().status).toMatchObject({
      migration_required: true,
      migration_state: "deferred",
    });
  });

  it("migrates legacy skills and reloads status", async () => {
    const completed = {
      ...status,
      migration_required: false,
      migration_state: "completed" as const,
    };
    vi.mocked(invoke)
      .mockResolvedValueOnce({ central_path: status.central_path, skill_count: 3 })
      .mockResolvedValueOnce(completed);

    await useStorageStore.getState().migrateLegacy();

    expect(invoke).toHaveBeenNthCalledWith(1, "migrate_legacy_central");
    expect(useStorageStore.getState().status).toEqual(completed);
  });
});

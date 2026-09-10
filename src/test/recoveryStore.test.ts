import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { useRecoveryStore } from "@/stores/recoveryStore";
import type { RecoveryEntry } from "@/types/recovery";

const fileEntry: RecoveryEntry = {
  id: "copy-1",
  kind: "copy_backup",
  label: "review skill",
  original_path: "/Users/test/.cursor/skills/review",
  created_at: "2026-09-11T02:00:00Z",
  expires_at: "2026-10-11T02:00:00Z",
  backup_path: "/Users/test/.skillsmanage/recovery/copy-1",
};

const databaseEntry: RecoveryEntry = {
  id: "database-1",
  kind: "database",
  label: "Database backup",
  original_path: "/Users/test/.skillsmanage/db.sqlite",
  created_at: "2026-09-11T02:30:00Z",
  expires_at: null,
  backup_path: "/Users/test/.skillsmanage/recovery/database-1.sqlite",
};

describe("recoveryStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useRecoveryStore.setState({
      entries: [],
      isLoading: false,
      isCreatingDatabaseBackup: false,
      restoringEntryId: null,
      deletingEntryId: null,
      error: null,
    });
  });

  it("loads recovery entries from the backend", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([fileEntry, databaseEntry]);

    await useRecoveryStore.getState().loadEntries();

    expect(invoke).toHaveBeenCalledWith("list_recovery_entries");
    expect(useRecoveryStore.getState().entries).toEqual([fileEntry, databaseEntry]);
  });

  it("restores an entry and reloads the authoritative list", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce([databaseEntry]);

    await useRecoveryStore.getState().restoreEntry(fileEntry.id);

    expect(invoke).toHaveBeenNthCalledWith(1, "restore_recovery_entry", { id: fileEntry.id });
    expect(invoke).toHaveBeenNthCalledWith(2, "list_recovery_entries");
    expect(useRecoveryStore.getState().entries).toEqual([databaseEntry]);
  });

  it("removes an entry locally after permanent deletion", async () => {
    useRecoveryStore.setState({ entries: [fileEntry, databaseEntry] });
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await useRecoveryStore.getState().deleteEntry(fileEntry.id);

    expect(invoke).toHaveBeenCalledWith("delete_recovery_entry", { id: fileEntry.id });
    expect(useRecoveryStore.getState().entries).toEqual([databaseEntry]);
  });

  it("creates a database backup then reloads the list", async () => {
    vi.mocked(invoke)
      .mockResolvedValueOnce(databaseEntry)
      .mockResolvedValueOnce([databaseEntry]);

    await expect(useRecoveryStore.getState().createDatabaseBackup()).resolves.toEqual(databaseEntry);

    expect(invoke).toHaveBeenNthCalledWith(1, "create_database_backup");
    expect(invoke).toHaveBeenNthCalledWith(2, "list_recovery_entries");
  });

  it("opens the parent directory for a backup file", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);

    await useRecoveryStore.getState().openBackupLocation(databaseEntry.backup_path);

    expect(invoke).toHaveBeenCalledWith("open_in_file_manager", {
      path: "/Users/test/.skillsmanage/recovery",
    });
  });

  it("keeps a backend error in state for the screen to show", async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error("restore conflict"));

    await expect(useRecoveryStore.getState().restoreEntry(fileEntry.id)).rejects.toThrow("restore conflict");

    expect(useRecoveryStore.getState().error).toContain("restore conflict");
    expect(useRecoveryStore.getState().restoringEntryId).toBeNull();
  });
});

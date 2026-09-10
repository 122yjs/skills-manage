import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/stores/recoveryStore", () => ({ useRecoveryStore: vi.fn() }));
vi.mock("@/stores/platformStore", () => ({
  usePlatformStore: Object.assign(vi.fn(), { getState: () => ({ error: null }) }),
}));
vi.mock("@/stores/centralSkillsStore", () => ({
  useCentralSkillsStore: Object.assign(vi.fn(), { getState: () => ({ error: null }) }),
}));

import { RecoverySettings } from "@/components/settings/RecoverySettings";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { usePlatformStore } from "@/stores/platformStore";
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

function setup({
  entries = [fileEntry, databaseEntry],
  loadEntries = vi.fn().mockResolvedValue(entries),
  restoreEntry = vi.fn().mockResolvedValue(undefined),
  deleteEntry = vi.fn().mockResolvedValue(undefined),
  createDatabaseBackup = vi.fn().mockResolvedValue(databaseEntry),
  openBackupLocation = vi.fn().mockResolvedValue(undefined),
  rescan = vi.fn().mockResolvedValue(undefined),
  loadCentralSkills = vi.fn().mockResolvedValue(undefined),
  error = null as string | null,
} = {}) {
  vi.mocked(useRecoveryStore).mockImplementation((selector) => selector({
    entries,
    isLoading: false,
    isCreatingDatabaseBackup: false,
    restoringEntryId: null,
    deletingEntryId: null,
    error,
    loadEntries,
    restoreEntry,
    deleteEntry,
    createDatabaseBackup,
    openBackupLocation,
  }));
  vi.mocked(usePlatformStore).mockImplementation((selector) => selector({ rescan } as never));
  vi.mocked(useCentralSkillsStore).mockImplementation((selector) => selector({ loadCentralSkills } as never));
  return { loadEntries, restoreEntry, deleteEntry, createDatabaseBackup, openBackupLocation, rescan, loadCentralSkills };
}

describe("RecoverySettings", () => {
  beforeEach(() => vi.clearAllMocks());

  it("loads entries when displayed and renders file metadata", () => {
    const { loadEntries } = setup();
    render(<RecoverySettings />);

    expect(loadEntries).toHaveBeenCalled();
    expect(screen.getByText("review skill")).toBeTruthy();
    expect(screen.getByText("/Users/test/.skillsmanage/recovery/copy-1")).toBeTruthy();
  });

  it("restores a file before scanning and reloading the library", async () => {
    const calls: string[] = [];
    const restoreEntry = vi.fn().mockImplementation(async () => { calls.push("restore"); });
    const rescan = vi.fn().mockImplementation(async () => { calls.push("rescan"); });
    const loadCentralSkills = vi.fn().mockImplementation(async () => { calls.push("central"); });
    setup({ restoreEntry, rescan, loadCentralSkills });
    render(<RecoverySettings />);

    fireEvent.click(screen.getByRole("button", { name: "恢复文件" }));

    await waitFor(() => expect(calls).toEqual(["restore", "rescan", "central"]));
  });

  it("does not offer automatic restoration for a database backup", () => {
    setup();
    render(<RecoverySettings />);

    expect(screen.getAllByRole("button", { name: "恢复文件" })).toHaveLength(1);
    expect(screen.getByText(/请在退出应用后手动恢复数据库备份/)).toBeTruthy();
  });

  it("requires a second click before permanent deletion", async () => {
    const { deleteEntry } = setup();
    render(<RecoverySettings />);

    fireEvent.click(screen.getAllByRole("button", { name: "永久删除" })[0]);
    expect(deleteEntry).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "立即删除" }));

    await waitFor(() => expect(deleteEntry).toHaveBeenCalledWith(fileEntry.id));
  });

  it("creates a manual database backup", async () => {
    const { createDatabaseBackup } = setup();
    render(<RecoverySettings />);

    fireEvent.click(screen.getByRole("button", { name: "创建数据库备份" }));

    await waitFor(() => expect(createDatabaseBackup).toHaveBeenCalled());
  });

  it("opens the backup location", async () => {
    const { openBackupLocation } = setup();
    render(<RecoverySettings />);

    fireEvent.click(screen.getAllByRole("button", { name: "打开位置" })[0]);

    await waitFor(() => expect(openBackupLocation).toHaveBeenCalledWith(fileEntry.backup_path));
  });

  it("shows the stored load error as an alert", () => {
    const loadEntries = vi.fn().mockRejectedValue(new Error("recovery list unavailable"));
    setup({ entries: [], loadEntries, error: "recovery list unavailable" });
    render(<RecoverySettings />);

    expect(screen.getByRole("alert")).toHaveTextContent("recovery list unavailable");
  });
});

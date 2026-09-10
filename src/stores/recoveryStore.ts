import { create } from "zustand";

import { invoke, isTauriRuntime } from "@/lib/tauri";
import type { RecoveryEntry } from "@/types/recovery";

interface RecoveryState {
  entries: RecoveryEntry[];
  isLoading: boolean;
  isCreatingDatabaseBackup: boolean;
  restoringEntryId: string | null;
  deletingEntryId: string | null;
  error: string | null;
  loadEntries: () => Promise<RecoveryEntry[]>;
  restoreEntry: (id: string) => Promise<void>;
  deleteEntry: (id: string) => Promise<void>;
  createDatabaseBackup: () => Promise<RecoveryEntry>;
  openBackupLocation: (backupPath: string) => Promise<void>;
}

function getParentPath(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  const separator = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  if (separator === 0) return "/";
  return separator > 0 ? trimmed.slice(0, separator) : path;
}

export const useRecoveryStore = create<RecoveryState>((set, get) => ({
  entries: [],
  isLoading: false,
  isCreatingDatabaseBackup: false,
  restoringEntryId: null,
  deletingEntryId: null,
  error: null,

  loadEntries: async () => {
    set({ isLoading: true, error: null });
    if (!isTauriRuntime()) {
      set({ entries: [], isLoading: false });
      return [];
    }
    try {
      const entries = await invoke<RecoveryEntry[]>("list_recovery_entries");
      const nextEntries = entries ?? [];
      set({ entries: nextEntries, isLoading: false });
      return nextEntries;
    } catch (error) {
      set({ error: String(error), isLoading: false });
      throw error;
    }
  },

  restoreEntry: async (id) => {
    set({ restoringEntryId: id, error: null });
    try {
      await invoke("restore_recovery_entry", { id });
      await get().loadEntries();
      set({ restoringEntryId: null });
    } catch (error) {
      set({ error: String(error), restoringEntryId: null });
      throw error;
    }
  },

  deleteEntry: async (id) => {
    set({ deletingEntryId: id, error: null });
    try {
      await invoke("delete_recovery_entry", { id });
      set((state) => ({
        entries: state.entries.filter((entry) => entry.id !== id),
        deletingEntryId: null,
      }));
    } catch (error) {
      set({ error: String(error), deletingEntryId: null });
      throw error;
    }
  },

  createDatabaseBackup: async () => {
    set({ isCreatingDatabaseBackup: true, error: null });
    try {
      const entry = await invoke<RecoveryEntry>("create_database_backup");
      await get().loadEntries();
      set({ isCreatingDatabaseBackup: false });
      return entry;
    } catch (error) {
      set({ error: String(error), isCreatingDatabaseBackup: false });
      throw error;
    }
  },

  openBackupLocation: async (backupPath) => {
    try {
      await invoke("open_in_file_manager", { path: getParentPath(backupPath) });
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },
}));

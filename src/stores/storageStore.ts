import { create } from "zustand";

import { invoke, isTauriRuntime } from "@/lib/tauri";
import type {
  CentralVaultStatus,
  StorageChangeResult,
  StoragePreview,
} from "@/types";

const BROWSER_STATUS: CentralVaultStatus = {
  central_path: "~/.skillsmanage/skills/",
  default_central_path: "~/.skillsmanage/skills/",
  legacy_path: "~/.agents/skills/",
  universal_path: "~/.agents/skills/",
  migration_state: "completed",
  migration_required: false,
  legacy_skill_count: 0,
};

interface StorageState {
  status: CentralVaultStatus | null;
  preview: StoragePreview | null;
  isLoading: boolean;
  isApplying: boolean;
  error: string | null;
  loadStatus: () => Promise<CentralVaultStatus>;
  previewMigration: () => Promise<StoragePreview>;
  deferMigration: () => Promise<void>;
  migrateLegacy: () => Promise<StorageChangeResult>;
  previewPathChange: (newPath: string) => Promise<StoragePreview>;
  changePath: (newPath: string) => Promise<StorageChangeResult>;
  clearPreview: () => void;
  clearError: () => void;
}

export const useStorageStore = create<StorageState>((set, get) => ({
  status: null,
  preview: null,
  isLoading: false,
  isApplying: false,
  error: null,

  loadStatus: async () => {
    set({ isLoading: true, error: null });
    if (!isTauriRuntime()) {
      set({ status: BROWSER_STATUS, isLoading: false });
      return BROWSER_STATUS;
    }
    try {
      const status = await invoke<CentralVaultStatus>("get_central_vault_status");
      set({ status, isLoading: false });
      return status;
    } catch (error) {
      set({ error: String(error), isLoading: false });
      throw error;
    }
  },

  previewMigration: async () => {
    set({ isLoading: true, error: null });
    try {
      const preview = await invoke<StoragePreview>("preview_legacy_migration");
      set({ preview, isLoading: false });
      return preview;
    } catch (error) {
      set({ error: String(error), isLoading: false });
      throw error;
    }
  },

  deferMigration: async () => {
    await invoke("defer_legacy_migration");
    const status = get().status;
    set({
      status: status ? { ...status, migration_state: "deferred" } : status,
      error: null,
    });
  },

  migrateLegacy: async () => {
    set({ isApplying: true, error: null });
    try {
      const result = await invoke<StorageChangeResult>("migrate_legacy_central");
      const status = await invoke<CentralVaultStatus>("get_central_vault_status");
      set({ status, preview: null, isApplying: false });
      return result;
    } catch (error) {
      set({ error: String(error), isApplying: false });
      throw error;
    }
  },

  previewPathChange: async (newPath) => {
    set({ isLoading: true, error: null });
    try {
      const preview = await invoke<StoragePreview>("preview_central_path_change", {
        newPath,
      });
      set({ preview, isLoading: false });
      return preview;
    } catch (error) {
      set({ error: String(error), isLoading: false });
      throw error;
    }
  },

  changePath: async (newPath) => {
    set({ isApplying: true, error: null });
    try {
      const result = await invoke<StorageChangeResult>("change_central_path", {
        newPath,
      });
      const status = await invoke<CentralVaultStatus>("get_central_vault_status");
      set({ status, preview: null, isApplying: false });
      return result;
    } catch (error) {
      set({ error: String(error), isApplying: false });
      throw error;
    }
  },

  clearPreview: () => set({ preview: null }),
  clearError: () => set({ error: null }),
}));

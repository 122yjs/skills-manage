import { create } from "zustand";

import { invoke, isTauriRuntime } from "@/lib/tauri";
import type { StartupStatus } from "@/types";

interface StartupState {
  phase: "checking" | "ready" | "failed";
  error: string | null;
  isRetrying: boolean;
  load: () => Promise<void>;
  retry: (centralPath?: string) => Promise<void>;
}

export const useStartupStore = create<StartupState>((set) => ({
  phase: isTauriRuntime() ? "checking" : "ready",
  error: null,
  isRetrying: false,

  load: async () => {
    if (!isTauriRuntime()) return;
    try {
      const status = await invoke<StartupStatus>("get_startup_status");
      set(
        status?.ready === false
          ? { phase: "failed", error: status.error }
          : { phase: "ready", error: null },
      );
    } catch (error) {
      set({ phase: "failed", error: String(error) });
    }
  },

  retry: async (centralPath) => {
    set({ isRetrying: true });
    try {
      await invoke("retry_startup", { centralPath: centralPath?.trim() || null });
      set({ phase: "ready", error: null, isRetrying: false });
    } catch (error) {
      set({ phase: "failed", error: String(error), isRetrying: false });
    }
  },
}));

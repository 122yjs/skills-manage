import { create } from "zustand";

import { invoke, isTauriRuntime } from "@/lib/tauri";
import type { AgentWithStatus, DevToolSetupState } from "@/types";

type LoadStatus = "idle" | "loading" | "ready" | "error";

interface DevToolSetupStore {
  status: LoadStatus;
  completed: boolean;
  tools: AgentWithStatus[];
  isEditorOpen: boolean;
  isSaving: boolean;
  error: string | null;
  load: () => Promise<void>;
  openEditor: () => void;
  closeEditor: () => void;
  save: (agentIds: string[]) => Promise<void>;
}

export const useDevToolSetupStore = create<DevToolSetupStore>((set) => ({
  status: "idle",
  completed: false,
  tools: [],
  isEditorOpen: false,
  isSaving: false,
  error: null,

  load: async () => {
    if (!isTauriRuntime()) {
      set({ status: "ready", completed: true, tools: [], error: null });
      return;
    }

    set({ status: "loading", error: null });
    try {
      const state = await invoke<DevToolSetupState>("get_dev_tool_setup_state");
      set({
        status: "ready",
        completed: state.completed,
        tools: state.tools,
        error: null,
      });
    } catch (error) {
      set({ status: "error", error: String(error) });
    }
  },

  openEditor: () => {
    set({ isEditorOpen: true, error: null });
    if (!isTauriRuntime()) return;

    void invoke<DevToolSetupState>("get_dev_tool_setup_state")
      .then((state) =>
        set({ completed: state.completed, tools: state.tools, error: null })
      )
      .catch((error) => set({ error: String(error) }));
  },
  closeEditor: () => set({ isEditorOpen: false, error: null }),

  save: async (agentIds) => {
    set({ isSaving: true, error: null });
    try {
      const state = await invoke<DevToolSetupState>("save_dev_tool_selection", {
        agentIds,
      });
      set({
        status: "ready",
        completed: state.completed,
        tools: state.tools,
        isEditorOpen: false,
        isSaving: false,
        error: null,
      });
    } catch (error) {
      set({ isSaving: false, error: String(error) });
      throw error;
    }
  },
}));

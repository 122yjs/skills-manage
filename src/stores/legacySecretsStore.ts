import { create } from "zustand";

import { invoke } from "@/lib/tauri";

interface LegacySecretsState {
  /** 키체인으로 옮기지 못해 DB에 평문으로 남은 이전 비밀값이 있는지. */
  present: boolean;
  load: () => Promise<void>;
  clear: () => Promise<void>;
}

export const useLegacySecretsStore = create<LegacySecretsState>((set) => ({
  present: false,

  load: async () => {
    set({ present: await invoke<boolean>("has_legacy_secrets") });
  },

  clear: async () => {
    await invoke<void>("clear_legacy_secrets");
    set({ present: false });
  },
}));

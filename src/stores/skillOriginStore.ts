import { useSkillGroupStore } from "./skillGroupStore";
import { create } from "zustand";
import { invoke, isTauriRuntime } from "@/lib/tauri";
import type {
  SkillDetailRequest,
  SkillOriginCandidate,
  SkillOriginDiscovery,
  SkillOriginInfo,
  SkillOriginStatus,
  SkillUpdatePlan,
  SkillUpdateResult,
} from "@/types";

interface LinkOriginInput {
  repoUrl: string;
  sourcePath: string;
  refName?: string;
}

interface SkillOriginState {
  origin: SkillOriginInfo | null;
  status: SkillOriginStatus | null;
  candidates: SkillOriginCandidate[];
  isDiscovering: boolean;
  isLoading: boolean;
  isChecking: boolean;
  isUpdating: boolean;
  error: string | null;
  loadOrigin: (target: SkillDetailRequest) => Promise<void>;
  linkOrigin: (target: SkillDetailRequest, input: LinkOriginInput) => Promise<SkillOriginStatus>;
  unlinkOrigin: (target: SkillDetailRequest) => Promise<void>;
  checkOrigin: (target: SkillDetailRequest) => Promise<SkillOriginStatus>;
  prepareUpdate: (target: SkillDetailRequest, allowLocalChanges: boolean) => Promise<SkillUpdatePlan>;
  applyUpdate: (operationId: string) => Promise<SkillUpdateResult>;
  reset: () => void;
}

function backendTarget(target: SkillDetailRequest) {
  return {
    skillId: target.skillId,
    agentId: target.agentId ?? null,
    rowId: target.rowId ?? null,
  };
}

let originLoadToken = 0;

export const useSkillOriginStore = create<SkillOriginState>((set) => ({
  origin: null,
  status: null,
  candidates: [],
  isDiscovering: false,
  isLoading: false,
  isChecking: false,
  isUpdating: false,
  error: null,

  loadOrigin: async (target) => {
    const token = ++originLoadToken;
    if (!isTauriRuntime()) {
      set({ origin: null, status: null, candidates: [], isLoading: false, error: null });
      return;
    }
    set({ isLoading: true, error: null });
    try {
      const origin = await invoke<SkillOriginInfo | null>("get_skill_origin", {
        target: backendTarget(target),
      });
      if (token !== originLoadToken) return;
      set({ origin, status: null, candidates: [], isLoading: false, isDiscovering: !origin });
      if (!origin) {
        try {
          const discovery = await invoke<SkillOriginDiscovery>("discover_skill_origin", {
            target: backendTarget(target),
          });
          if (token !== originLoadToken) return;
          set({ origin: discovery.origin, candidates: discovery.candidates, isDiscovering: false });
          if (discovery.origin) void useSkillGroupStore.getState().load();
        } catch (error) {
          if (token === originLoadToken) set({ error: String(error), isDiscovering: false });
        }
      }
    } catch (error) {
      if (token === originLoadToken) set({ error: String(error), isLoading: false });
    }
  },

  linkOrigin: async (target, input) => {
    originLoadToken++;
    set({ isChecking: true, isDiscovering: false, error: null });
    try {
      const status = await invoke<SkillOriginStatus>("link_skill_origin", {
        request: {
          target: backendTarget(target),
          repoUrl: input.repoUrl,
          sourcePath: input.sourcePath || ".",
          refName: input.refName || null,
        },
      });
      set({ origin: status.origin, status, candidates: [], isChecking: false });
      void useSkillGroupStore.getState().load();
      return status;
    } catch (error) {
      set({ error: String(error), isChecking: false });
      throw error;
    }
  },

  unlinkOrigin: async (target) => {
    originLoadToken++;
    set({ isChecking: true, isDiscovering: false, error: null });
    try {
      await invoke("unlink_skill_origin", { target: backendTarget(target) });
      set({ origin: null, status: null, candidates: [], isChecking: false });
      void useSkillGroupStore.getState().load();
    } catch (error) {
      set({ error: String(error), isChecking: false });
      throw error;
    }
  },

  checkOrigin: async (target) => {
    set({ isChecking: true, error: null });
    try {
      const status = await invoke<SkillOriginStatus>("check_skill_origin", {
        target: backendTarget(target),
      });
      set((current) => current.origin?.bindingId === status.origin.bindingId
        ? { origin: status.origin, status, isChecking: false }
        : { isChecking: false });
      return status;
    } catch (error) {
      set({ error: String(error), isChecking: false });
      throw error;
    }
  },

  prepareUpdate: async (target, allowLocalChanges) => {
    set({ isUpdating: true, error: null });
    try {
      const plan = await invoke<SkillUpdatePlan>("prepare_skill_update", {
        request: { target: backendTarget(target), allowLocalChanges },
      });
      set({ isUpdating: false });
      return plan;
    } catch (error) {
      set({ error: String(error), isUpdating: false });
      throw error;
    }
  },

  applyUpdate: async (operationId) => {
    set({ isUpdating: true, error: null });
    try {
      const result = await invoke<SkillUpdateResult>("apply_skill_update", { operationId });
      set({ isUpdating: false });
      return result;
    } catch (error) {
      set({ error: String(error), isUpdating: false });
      throw error;
    }
  },

  reset: () => {
    originLoadToken++;
    set({
    origin: null,
    status: null,
    candidates: [],
    isDiscovering: false,
    isLoading: false,
    isChecking: false,
    isUpdating: false,
    error: null,
    });
  },
}));

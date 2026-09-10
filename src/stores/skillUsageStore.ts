import { create } from "zustand";

import { invoke, isTauriRuntime } from "@/lib/tauri";
import type { UsageSkillStatus, UsageStatus } from "@/types";

function skillActionKey(agentId: string, skillId: string) {
  return `${agentId}::${skillId}`;
}

export const SKILL_USAGE_BUSY_ERROR_CODE = "SKILL_USAGE_BUSY";

export class SkillUsageBusyError extends Error {
  readonly code = SKILL_USAGE_BUSY_ERROR_CODE;

  constructor() {
    super("A skill action is already in progress for this platform.");
    this.name = "SkillUsageBusyError";
  }
}

export function isSkillUsageBusyError(error: unknown): boolean {
  return (
    error instanceof SkillUsageBusyError ||
    (typeof error === "object" &&
      error !== null &&
      "code" in error &&
      error.code === SKILL_USAGE_BUSY_ERROR_CODE)
  );
}

function replaceUsageSkill(
  statuses: UsageStatus[],
  agentId: string,
  skillId: string,
  enabled: boolean
): UsageStatus[] {
  return statuses.map((status) => {
    if (status.agent_id !== agentId) return status;

    const skills = status.skills.map((skill) =>
      skill.skill_id === skillId
        ? { ...skill, enabled, paused_by_bulk: enabled ? false : skill.paused_by_bulk }
        : skill
    );
    return {
      ...status,
      skills,
      active_count: skills.filter((skill) => skill.enabled).length,
      paused_count: skills.filter((skill) => !skill.enabled).length,
    };
  });
}

function removeUsageSkill(
  statuses: UsageStatus[],
  agentId: string,
  skillId: string
): UsageStatus[] {
  return statuses.map((status) => {
    if (status.agent_id !== agentId) return status;

    const skills = status.skills.filter((skill) => skill.skill_id !== skillId);
    return {
      ...status,
      skills,
      active_count: skills.filter((skill) => skill.enabled).length,
      paused_count: skills.filter((skill) => !skill.enabled).length,
    };
  });
}

export interface DeletePlatformInstallationsResult {
  deleted: string[];
  failed: Array<{ skill_id: string; error: string }>;
}

interface SkillUsageState {
  statuses: UsageStatus[];
  isLoading: boolean;
  updatingSkillKeys: Record<string, boolean>;
  updatingAgentIds: Record<string, boolean>;
  error: string | null;

  loadUsageStatus: () => Promise<void>;
  setSkillUsage: (skillId: string, agentId: string, enabled: boolean) => Promise<void>;
  setPlatformUsage: (agentId: string, enabled: boolean) => Promise<void>;
  deleteSkillFromAgent: (skillId: string, agentId: string) => Promise<void>;
  deletePlatformInstallations: (agentId: string) => Promise<DeletePlatformInstallationsResult>;
  getUsageStatus: (agentId: string) => UsageStatus | undefined;
  getSkillUsage: (agentId: string, skillId: string) => UsageSkillStatus | undefined;
}

/**
 * 설치 파일의 실제 활성 상태만 관리한다. 사이드바에 플랫폼을 표시할지 여부와는
 * 별개라서, 표시 설정을 바꿔도 이 상태를 다시 쓰지 않는다.
 */
export const useSkillUsageStore = create<SkillUsageState>((set, get) => ({
  statuses: [],
  isLoading: false,
  updatingSkillKeys: {},
  updatingAgentIds: {},
  error: null,

  loadUsageStatus: async () => {
    set({ isLoading: true, error: null });
    if (!isTauriRuntime()) {
      set({ isLoading: false });
      return;
    }

    try {
      const statuses = await invoke<UsageStatus[]>("get_skill_usage_status");
      set({ statuses: statuses ?? [], isLoading: false });
    } catch (error) {
      set({ error: String(error), isLoading: false });
      throw error;
    }
  },

  setSkillUsage: async (skillId, agentId, enabled) => {
    const actionKey = skillActionKey(agentId, skillId);
    if (get().updatingSkillKeys[actionKey] || get().updatingAgentIds[agentId]) return;

    set((state) => ({
      updatingSkillKeys: { ...state.updatingSkillKeys, [actionKey]: true },
      error: null,
    }));

    try {
      if (!isTauriRuntime()) {
        set((state) => ({
          statuses: replaceUsageSkill(state.statuses, agentId, skillId, enabled),
        }));
        return;
      }

      await invoke("set_skill_usage", { skillId, agentId, enabled });
      await get().loadUsageStatus();
    } catch (error) {
      // 실패한 작업을 성공처럼 보이지 않게 서버가 가진 상태를 다시 읽는다.
      try {
        await get().loadUsageStatus();
      } catch {
        // 원래 활성 상태 전환 실패를 호출자에게 유지한다.
      }
      set({ error: String(error) });
      throw error;
    } finally {
      set((state) => {
        const updatingSkillKeys = { ...state.updatingSkillKeys };
        delete updatingSkillKeys[actionKey];
        return { updatingSkillKeys };
      });
    }
  },

  setPlatformUsage: async (agentId, enabled) => {
    if (
      get().updatingAgentIds[agentId] ||
      Object.keys(get().updatingSkillKeys).some((key) => key.startsWith(`${agentId}::`))
    ) return;

    set((state) => ({
      updatingAgentIds: { ...state.updatingAgentIds, [agentId]: true },
      error: null,
    }));

    try {
      if (!isTauriRuntime()) {
        set((state) => ({
          statuses: state.statuses.map((status) => {
            if (status.agent_id !== agentId) return status;
            const skills = status.skills.map((skill) =>
              enabled
                ? { ...skill, enabled: skill.paused_by_bulk ? true : skill.enabled, paused_by_bulk: false }
                : { ...skill, enabled: false, paused_by_bulk: skill.enabled }
            );
            return {
              ...status,
              skills,
              active_count: skills.filter((skill) => skill.enabled).length,
              paused_count: skills.filter((skill) => !skill.enabled).length,
            };
          }),
        }));
        return;
      }

      await invoke("set_platform_usage", { agentId, enabled });
      await get().loadUsageStatus();
    } catch (error) {
      try {
        await get().loadUsageStatus();
      } catch {
        // 원래 활성 상태 전환 실패를 호출자에게 유지한다.
      }
      set({ error: String(error) });
      throw error;
    } finally {
      set((state) => {
        const updatingAgentIds = { ...state.updatingAgentIds };
        delete updatingAgentIds[agentId];
        return { updatingAgentIds };
      });
    }
  },

  deleteSkillFromAgent: async (skillId, agentId) => {
    const actionKey = skillActionKey(agentId, skillId);
    if (get().updatingSkillKeys[actionKey] || get().updatingAgentIds[agentId]) {
      throw new SkillUsageBusyError();
    }

    set((state) => ({
      updatingSkillKeys: { ...state.updatingSkillKeys, [actionKey]: true },
      error: null,
    }));

    try {
      if (!isTauriRuntime()) {
        set((state) => ({
          statuses: removeUsageSkill(state.statuses, agentId, skillId),
        }));
        return;
      }

      await invoke("delete_skill_from_agent", { skillId, agentId });
      await get().loadUsageStatus();
    } catch (error) {
      // 삭제가 일부라도 실패한 경우 서버 상태를 다시 읽어 실제 결과를 보여 준다.
      try {
        await get().loadUsageStatus();
      } catch {
        // 원래 삭제 실패를 호출자에게 유지한다.
      }
      set({ error: String(error) });
      throw error;
    } finally {
      set((state) => {
        const updatingSkillKeys = { ...state.updatingSkillKeys };
        delete updatingSkillKeys[actionKey];
        return { updatingSkillKeys };
      });
    }
  },

  deletePlatformInstallations: async (agentId) => {
    if (
      get().updatingAgentIds[agentId] ||
      Object.keys(get().updatingSkillKeys).some((key) => key.startsWith(`${agentId}::`))
    ) {
      throw new SkillUsageBusyError();
    }

    set((state) => ({
      updatingAgentIds: { ...state.updatingAgentIds, [agentId]: true },
      error: null,
    }));

    try {
      if (!isTauriRuntime()) {
        const status = get().statuses.find((candidate) => candidate.agent_id === agentId);
        const deleted = status?.skills.map((skill) => skill.skill_id) ?? [];
        set((state) => ({
          statuses: state.statuses.map((candidate) =>
            candidate.agent_id === agentId
              ? { ...candidate, skills: [], active_count: 0, paused_count: 0 }
              : candidate
          ),
        }));
        return { deleted, failed: [] };
      }

      const result = await invoke<DeletePlatformInstallationsResult>(
        "delete_platform_installations",
        { agentId }
      );
      await get().loadUsageStatus();
      return result;
    } catch (error) {
      try {
        await get().loadUsageStatus();
      } catch {
        // 원래 삭제 실패를 호출자에게 유지한다.
      }
      set({ error: String(error) });
      throw error;
    } finally {
      set((state) => {
        const updatingAgentIds = { ...state.updatingAgentIds };
        delete updatingAgentIds[agentId];
        return { updatingAgentIds };
      });
    }
  },

  getUsageStatus: (agentId) => get().statuses.find((status) => status.agent_id === agentId),
  getSkillUsage: (agentId, skillId) =>
    get().statuses
      .find((status) => status.agent_id === agentId)
      ?.skills.find((skill) => skill.skill_id === skillId),
}));

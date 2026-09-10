import { create } from "zustand";

import { invoke, isTauriRuntime } from "@/lib/tauri";
import type { UsageSkillStatus, UsageStatus } from "@/types";

function skillActionKey(agentId: string, skillId: string) {
  return `${agentId}::${skillId}`;
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

interface SkillUsageState {
  statuses: UsageStatus[];
  isLoading: boolean;
  updatingSkillKeys: Record<string, boolean>;
  updatingAgentIds: Record<string, boolean>;
  error: string | null;

  loadUsageStatus: () => Promise<void>;
  setSkillUsage: (skillId: string, agentId: string, enabled: boolean) => Promise<void>;
  setPlatformUsage: (agentId: string, enabled: boolean) => Promise<void>;
  getUsageStatus: (agentId: string) => UsageStatus | undefined;
  getSkillUsage: (agentId: string, skillId: string) => UsageSkillStatus | undefined;
}

/**
 * 설치 파일의 실제 사용 여부만 관리한다. 사이드바에 플랫폼을 표시할지 여부와는
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
        // 원래 사용 전환 실패를 호출자에게 유지한다.
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
        // 원래 사용 전환 실패를 호출자에게 유지한다.
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

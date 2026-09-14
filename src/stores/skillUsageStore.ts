import { create } from "zustand";

import { invoke, isTauriRuntime } from "@/lib/tauri";
import i18n from "@/i18n";
import type {
  PlatformSkillControlStatus,
  SharedPlatformUsageResult,
  SharedSkillConfirmation,
  SharedSkillImpact,
  SharedSkillUsageResult,
  UsageSkillStatus,
  UsageStatus,
} from "@/types";

function skillActionKey(agentId: string, skillId: string) {
  return `${agentId}::${skillId}`;
}

function platformControlKey(agentId: string, sourcePath: string) {
  return `${agentId}::${sourcePath}`;
}

function sharedActionKey(sharedInstallId: string) {
  return `shared::${sharedInstallId}`;
}

function hasAnySkillUsageMutation(state: {
  updatingSkillKeys: Record<string, boolean>;
  updatingAgentIds: Record<string, boolean>;
  updatingPlatformControlKeys: Record<string, boolean>;
  updatingSharedKeys: Record<string, boolean>;
  updatingSharedBulk: boolean;
}): boolean {
  return (
    Object.keys(state.updatingSkillKeys).length > 0 ||
    Object.keys(state.updatingAgentIds).length > 0 ||
    Object.keys(state.updatingPlatformControlKeys).length > 0 ||
    Object.keys(state.updatingSharedKeys).length > 0 ||
    state.updatingSharedBulk
  );
}

export interface PlatformSkillControlTarget {
  skillId: string;
  skillName: string;
  sourcePath: string;
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

export interface SharedDeletePreview {
  skill_id: string;
  skill_name: string;
  enabled: boolean;
  source_path: string;
  links: Array<{ agent_id: string; display_name: string; path: string; installed_path: string; target: string }>;
  confirmation_token: string;
}

interface SkillUsageState {
  statuses: UsageStatus[];
  isLoading: boolean;
  updatingSkillKeys: Record<string, boolean>;
  updatingAgentIds: Record<string, boolean>;
  error: string | null;
  platformControlsByAgent: Record<string, PlatformSkillControlStatus[]>;
  updatingPlatformControlKeys: Record<string, boolean>;
  sharedImpactsById: Record<string, SharedSkillImpact>;
  updatingSharedKeys: Record<string, boolean>;
  updatingSharedBulk: boolean;

  previewSharedDelete: (skillId: string) => Promise<SharedDeletePreview>;
  deleteSharedInstalls: (plans: SharedDeletePreview[]) => Promise<DeletePlatformInstallationsResult>;
  loadUsageStatus: () => Promise<void>;
  setSkillUsage: (skillId: string, agentId: string, enabled: boolean) => Promise<void>;
  setPlatformUsage: (agentId: string, enabled: boolean) => Promise<void>;
  deleteSkillFromAgent: (skillId: string, agentId: string) => Promise<void>;
  deletePlatformInstallations: (agentId: string) => Promise<DeletePlatformInstallationsResult>;
  getUsageStatus: (agentId: string) => UsageStatus | undefined;
  getSkillUsage: (agentId: string, skillId: string) => UsageSkillStatus | undefined;
  loadPlatformSkillControls: (agentId: string) => Promise<void>;
  setPlatformSkillControl: (
    agentId: string,
    target: PlatformSkillControlTarget,
    enabled: boolean
  ) => Promise<void>;
  deletePlatformSkillControl: (
    agentId: string,
    target: PlatformSkillControlTarget
  ) => Promise<void>;
  reapplyPlatformSkillControl: (
    agentId: string,
    target: PlatformSkillControlTarget
  ) => Promise<void>;
  getSharedImpact: (sharedInstallId: string) => SharedSkillImpact | undefined;
  loadSharedSkillImpact: (sharedInstallId: string) => Promise<SharedSkillImpact>;
  setSharedSkillUsage: (
    sharedInstallId: string,
    enabled: boolean,
    confirmationToken: string
  ) => Promise<SharedSkillUsageResult>;
  setSharedPlatformUsage: (
    enabled: boolean,
    confirmations: SharedSkillConfirmation[]
  ) => Promise<SharedPlatformUsageResult>;
  reloadSharedRelatedState: (impacts: SharedSkillImpact[]) => Promise<void>;
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
  platformControlsByAgent: {},
  updatingPlatformControlKeys: {},
  sharedImpactsById: {},
  updatingSharedKeys: {},
  updatingSharedBulk: false,

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
    if (hasAnySkillUsageMutation(get())) {
      throw new SkillUsageBusyError();
    }

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
    if (hasAnySkillUsageMutation(get())) {
      throw new SkillUsageBusyError();
    }

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
    if (hasAnySkillUsageMutation(get())) {
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
    if (hasAnySkillUsageMutation(get())) {
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

  loadPlatformSkillControls: async (agentId) => {
    if (!isTauriRuntime()) {
      set((state) => ({
        platformControlsByAgent: {
          ...state.platformControlsByAgent,
          [agentId]: [],
        },
      }));
      return;
    }
    try {
      const controls = await invoke<PlatformSkillControlStatus[]>(
        "get_platform_skill_controls",
        { agentId }
      );
      set((state) => ({
        platformControlsByAgent: {
          ...state.platformControlsByAgent,
          [agentId]: controls ?? [],
        },
      }));
    } catch (error) {
      set({ error: String(error) });
      throw error;
    }
  },

  setPlatformSkillControl: async (agentId, target, enabled) => {
    const actionKey = platformControlKey(agentId, target.sourcePath);
    if (hasAnySkillUsageMutation(get())) {
      throw new SkillUsageBusyError();
    }
    set((state) => ({
      updatingPlatformControlKeys: {
        ...state.updatingPlatformControlKeys,
        [actionKey]: true,
      },
      error: null,
    }));
    try {
      if (!isTauriRuntime()) {
        throw new Error("플랫폼 스킬 제어는 데스크톱 앱에서만 사용할 수 있습니다.");
      }
      await invoke("set_platform_skill_control", {
        agentId,
        skillId: target.skillId,
        skillName: target.skillName,
        sourcePath: target.sourcePath,
        enabled,
      });
      await get().loadPlatformSkillControls(agentId);
    } catch (error) {
      try {
        await get().loadPlatformSkillControls(agentId);
      } catch {
        // 원래 제어 오류를 호출자에게 유지한다.
      }
      set({ error: String(error) });
      throw error;
    } finally {
      set((state) => {
        const updatingPlatformControlKeys = { ...state.updatingPlatformControlKeys };
        delete updatingPlatformControlKeys[actionKey];
        return { updatingPlatformControlKeys };
      });
    }
  },

  deletePlatformSkillControl: async (agentId, target) => {
    const actionKey = platformControlKey(agentId, target.sourcePath);
    if (hasAnySkillUsageMutation(get())) {
      throw new SkillUsageBusyError();
    }
    set((state) => ({
      updatingPlatformControlKeys: {
        ...state.updatingPlatformControlKeys,
        [actionKey]: true,
      },
      error: null,
    }));
    try {
      if (!isTauriRuntime()) {
        throw new Error("플랫폼 스킬 제어는 데스크톱 앱에서만 사용할 수 있습니다.");
      }
      await invoke("delete_platform_skill_control", {
        agentId,
        skillId: target.skillId,
        skillName: target.skillName,
        sourcePath: target.sourcePath,
      });
      await get().loadPlatformSkillControls(agentId);
    } catch (error) {
      try {
        await get().loadPlatformSkillControls(agentId);
      } catch {
        // 원래 제어 오류를 호출자에게 유지한다.
      }
      set({ error: String(error) });
      throw error;
    } finally {
      set((state) => {
        const updatingPlatformControlKeys = { ...state.updatingPlatformControlKeys };
        delete updatingPlatformControlKeys[actionKey];
        return { updatingPlatformControlKeys };
      });
    }
  },

  reapplyPlatformSkillControl: async (agentId, target) => {
    const actionKey = platformControlKey(agentId, target.sourcePath);
    if (hasAnySkillUsageMutation(get())) {
      throw new SkillUsageBusyError();
    }
    set((state) => ({
      updatingPlatformControlKeys: {
        ...state.updatingPlatformControlKeys,
        [actionKey]: true,
      },
      error: null,
    }));
    try {
      if (!isTauriRuntime()) {
        throw new Error("플랫폼 스킬 제어는 데스크톱 앱에서만 사용할 수 있습니다.");
      }
      await invoke("reapply_platform_skill_control", {
        agentId,
        skillId: target.skillId,
        skillName: target.skillName,
        sourcePath: target.sourcePath,
      });
      await get().loadPlatformSkillControls(agentId);
    } catch (error) {
      try {
        await get().loadPlatformSkillControls(agentId);
      } catch {
        // 원래 제어 오류를 호출자에게 유지한다.
      }
      set({ error: String(error) });
      throw error;
    } finally {
      set((state) => {
        const updatingPlatformControlKeys = { ...state.updatingPlatformControlKeys };
        delete updatingPlatformControlKeys[actionKey];
        return { updatingPlatformControlKeys };
      });
    }
  },

  previewSharedDelete: async (skillId) => {
    if (!isTauriRuntime()) throw new Error(i18n.t("skillUsage.sharedDesktopRequired"));
    return invoke<SharedDeletePreview>("preview_shared_install_delete", { skillId });
  },

  deleteSharedInstalls: async (plans) => {
    if (hasAnySkillUsageMutation(get())) throw new SkillUsageBusyError();
    set({ updatingSharedBulk: true, error: null });
    try {
      if (!isTauriRuntime()) throw new Error(i18n.t("skillUsage.sharedDesktopRequired"));
      const result = await invoke<DeletePlatformInstallationsResult>("delete_shared_installs", {
        confirmations: plans.map(({ skill_id, confirmation_token }) => ({ skill_id, confirmation_token })),
      });
      // 삭제 성공과 목록 갱신 실패를 섞지 않는다. 화면에서 갱신 결과를 별도로 알린다.
      set({ platformControlsByAgent: {}, sharedImpactsById: {} });
      return result;
    } finally {
      set({ updatingSharedBulk: false });
    }
  },

  getSharedImpact: (sharedInstallId) => get().sharedImpactsById[sharedInstallId],

  loadSharedSkillImpact: async (sharedInstallId) => {
    if (!isTauriRuntime()) {
      const cached = get().sharedImpactsById[sharedInstallId];
      if (cached) return cached;
      throw new Error(i18n.t("skillUsage.sharedDesktopRequired"));
    }
    const impact = await invoke<SharedSkillImpact>("get_shared_skill_impact", {
      sharedInstallId,
    });
    set((state) => ({
      sharedImpactsById: { ...state.sharedImpactsById, [sharedInstallId]: impact },
    }));
    return impact;
  },

  setSharedSkillUsage: async (sharedInstallId, enabled, confirmationToken) => {
    const actionKey = sharedActionKey(sharedInstallId);
    if (hasAnySkillUsageMutation(get())) {
      throw new SkillUsageBusyError();
    }
    set((state) => ({
      updatingSharedKeys: { ...state.updatingSharedKeys, [actionKey]: true },
      error: null,
    }));
    try {
      if (!isTauriRuntime()) {
        throw new Error(i18n.t("skillUsage.sharedDesktopRequired"));
      }
      const result = await invoke<SharedSkillUsageResult>("set_shared_skill_usage", {
        sharedInstallId,
        enabled,
        confirmationToken,
      });
      set((state) => ({
        sharedImpactsById: {
          ...state.sharedImpactsById,
          [result.impact.shared_install_id]: result.impact,
        },
      }));
      await get().reloadSharedRelatedState([result.impact]);
      return result;
    } catch (error) {
      try {
        await get().loadUsageStatus();
      } catch {
        // 원래 공용 제어 오류를 호출자에게 유지한다.
      }
      set({ error: String(error) });
      throw error;
    } finally {
      set((state) => {
        const updatingSharedKeys = { ...state.updatingSharedKeys };
        delete updatingSharedKeys[actionKey];
        return { updatingSharedKeys };
      });
    }
  },

  setSharedPlatformUsage: async (enabled, confirmations) => {
    if (hasAnySkillUsageMutation(get())) {
      throw new SkillUsageBusyError();
    }
    set({ updatingSharedBulk: true, error: null });
    try {
      if (!isTauriRuntime()) {
        throw new Error(i18n.t("skillUsage.sharedDesktopRequired"));
      }
      const result = await invoke<SharedPlatformUsageResult>("set_shared_platform_usage", {
        enabled,
        confirmations,
      });
      set((state) => {
        const sharedImpactsById = { ...state.sharedImpactsById };
        for (const impact of result.impacts) {
          sharedImpactsById[impact.shared_install_id] = impact;
        }
        return { sharedImpactsById };
      });
      await get().reloadSharedRelatedState(result.impacts);
      return result;
    } catch (error) {
      try {
        await get().loadUsageStatus();
      } catch {
        // 원래 일괄 제어 오류를 호출자에게 유지한다.
      }
      set({ error: String(error) });
      throw error;
    } finally {
      set({ updatingSharedBulk: false });
    }
  },

  reloadSharedRelatedState: async (impacts) => {
    const failures: unknown[] = [];
    try {
      await get().loadUsageStatus();
    } catch (error) {
      failures.push(error);
    }
    const agentIds = new Set<string>();
    for (const impact of impacts) {
      for (const platform of impact.confirmed_platforms ?? []) {
        if (platform.agent_id) agentIds.add(platform.agent_id);
      }
      for (const separate of impact.separate_installs ?? []) {
        if (separate.agent_id) agentIds.add(separate.agent_id);
      }
    }
    agentIds.add("universal");
    try {
      const { usePlatformStore } = await import("@/stores/platformStore");
      await usePlatformStore.getState().refreshCounts();
    } catch (error) {
      failures.push(error);
    }
    try {
      const { useSkillStore } = await import("@/stores/skillStore");
      for (const agentId of agentIds) {
        try {
          await useSkillStore.getState().getSkillsByAgent(agentId);
        } catch (error) {
          failures.push(error);
        }
      }
    } catch (error) {
      failures.push(error);
    }
    for (const agentId of agentIds) {
      if (!(agentId in get().platformControlsByAgent)) continue;
      try {
        await get().loadPlatformSkillControls(agentId);
      } catch (error) {
        failures.push(error);
      }
    }
    if (failures.length > 0) {
      const first = failures[0];
      set({ error: String(first) });
      throw first;
    }
  },
}));

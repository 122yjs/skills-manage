import { create } from "zustand";
import { invoke, isTauriRuntime } from "@/lib/tauri";

export interface SkillGroupMember {
  skillId: string;
  sourceKey: string;
  name: string;
  description?: string | null;
  filePath: string;
  agentId?: string | null;
  rowId?: string | null;
}

export interface SkillGroup {
  id: string;
  name: string;
  kind: "plugin" | "repository" | "bundle";
  folderPath?: string | null;
  repositoryUrl?: string | null;
  members: SkillGroupMember[];
  sourceSkillCount?: number;
}

let request = 0;
export const useSkillGroupStore = create<{
  groups: SkillGroup[];
  loading: boolean;
  error: string | null;
  load: () => Promise<void>;
}>((set) => ({
  groups: [], loading: false, error: null,
  load: async () => {
    if (!isTauriRuntime()) return;
    const token = ++request;
    set({ loading: true, error: null });
    try {
      const groups = await invoke<SkillGroup[]>("get_skill_groups");
      if (token === request) set({ groups: groups ?? [], loading: false });
    } catch (error) {
      if (token === request) set({ error: String(error), loading: false });
    }
  },
}));

export function skillGroupUrl(id: string) {
  return `/collections?group=${encodeURIComponent(id)}`;
}

// 같은 원본 스킬의 여러 설치본을 카드 하나 아래에 모은다.
export function groupSkillMembers(members: SkillGroupMember[]) {
  const skills = new Map<string, SkillGroupMember[]>();
  for (const member of members) {
    const locations = skills.get(member.sourceKey) ?? [];
    locations.push(member);
    skills.set(member.sourceKey, locations);
  }
  return [...skills.entries()].map(([sourceKey, locations]) => ({ sourceKey, locations }));
}

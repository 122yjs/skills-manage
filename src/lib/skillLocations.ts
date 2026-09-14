import type { ScannedSkill } from "@/types";

/** 이름은 화면에서만 묶는다. 개별 출처의 ID와 관리 대상을 합치지 않는다. */
export function groupSkillLocations(skills: ScannedSkill[]): ScannedSkill[][] {
  const groups = new Map<string, ScannedSkill[]>();
  for (const skill of skills) {
    const key = skill.name.trim() || skill.id;
    const group = groups.get(key) ?? [];
    group.push(skill);
    groups.set(key, group);
  }
  return [...groups.values()];
}


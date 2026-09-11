import { useEffect, useState } from "react";

export interface SelectableSkill {
  id: string;
  name: string;
  row_id?: string;
  source_kind?: string | null;
  is_read_only?: boolean;
}

export function skillSelectionKey(skill: SelectableSkill) {
  return skill.row_id ?? skill.id;
}

export function canTransferSkill(skill: SelectableSkill) {
  if (skill.source_kind === "plugin") return Boolean(skill.row_id);
  return !skill.is_read_only || skill.source_kind === "compatibility";
}

export function useSkillSelection(skills: SelectableSkill[], scope = "") {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const selectable = skills.filter(canTransferSkill);
  const visibleKeys = JSON.stringify(selectable.map(skillSelectionKey));
  useEffect(() => {
    const keys = new Set<string>(JSON.parse(visibleKeys));
    setSelected((current) => new Set([...current].filter((key) => keys.has(key))));
  }, [visibleKeys]);
  useEffect(() => setSelected(new Set()), [scope]);
  const selectedSkills = selectable.filter((skill) => selected.has(skillSelectionKey(skill)));
  const allSelected = selectable.length > 0 && selectedSkills.length === selectable.length;
  return {
    selected,
    selectedSkills,
    allSelected,
    selectableCount: selectable.length,
    toggle(key: string) {
      setSelected((current) => {
        const next = new Set(current);
        if (next.has(key)) next.delete(key);
        else next.add(key);
        return next;
      });
    },
    toggleAll() {
      setSelected(allSelected ? new Set() : new Set(selectable.map(skillSelectionKey)));
    },
    clear() { setSelected(new Set()); },
  };
}

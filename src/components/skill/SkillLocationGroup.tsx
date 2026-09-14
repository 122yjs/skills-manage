import { useId, useState, type ReactNode } from "react";
import { ChevronDown, ChevronRight, Layers } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ScannedSkill } from "@/types";

export function SkillLocationGroup({ skills, children, selectedCount = 0 }: { skills: ScannedSkill[]; children: ReactNode; selectedCount?: number }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const id = useId();
  if (skills.length === 1) return <>{children}</>;
  return <div className="self-start overflow-hidden rounded-xl bg-card shadow-sm ring-1 ring-border">
    <button type="button" className="flex w-full items-center gap-3 p-4 text-left hover:bg-muted/40"
      aria-expanded={open} aria-controls={id} onClick={() => setOpen(!open)}
      aria-label={t("skillLocations.toggle", { name: skills[0].name, count: skills.length })}>
      <Layers className="size-5 shrink-0 text-muted-foreground" />
      <span className="min-w-0 flex-1"><span className="block break-words font-medium">{skills[0].name}</span>
        <span className="mt-1 block text-xs text-muted-foreground">{t("skillLocations.count", { count: skills.length })}</span>
      </span>
      {selectedCount > 0 && <span className="text-xs text-primary">{t("skillLocations.selected", { count: selectedCount })}</span>}
      {open ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
    </button>
    <div id={id} hidden={!open}>
      {open && <div className="space-y-3 border-t border-border p-3">
        <p className="text-xs text-muted-foreground">{t("skillLocations.help")}</p>
        {children}
      </div>}
    </div>
  </div>;
}

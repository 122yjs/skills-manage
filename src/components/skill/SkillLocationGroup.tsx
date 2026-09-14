import { useId, useState, type ReactNode } from "react";
import { ChevronDown, ChevronRight, Layers } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ScannedSkill } from "@/types";
import { Button } from "@/components/ui/button";
import { useSkillStore, type DuplicateComparison } from "@/stores/skillStore";

export function SkillLocationGroup({ skills, children, selectedCount = 0, agentId }: { skills: ScannedSkill[]; children: ReactNode; selectedCount?: number; agentId?: string }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const [comparison, setComparison] = useState<DuplicateComparison | null>(null);
  const [comparing, setComparing] = useState(false);
  async function compare() {
    if (!agentId) return;
    setComparing(true);
    setComparison(null);
    try {
      setComparison(await useSkillStore.getState().compareLocations(agentId, skills[0].name));
    } catch {
      setComparison({ relation: "unknown", paths: [] });
    } finally {
      setComparing(false);
    }
  }
  const id = useId();
  if (skills.length === 1) return <>{children}</>;
  return <div className="self-start overflow-hidden rounded-xl bg-card shadow-sm ring-1 ring-border">
    <button type="button" className="flex w-full items-center gap-3 p-4 text-left hover:bg-muted/40"
      aria-expanded={open} aria-controls={id} onClick={() => setOpen(!open)}
      aria-label={t("skillLocations.toggle", { name: skills[0].name, count: skills.length })}>
      <Layers className="size-5 shrink-0 text-muted-foreground" />
      <span className="min-w-0 flex-1"><span className="block break-words font-medium">{skills[0].name}</span>
        <span className="mt-1 block text-xs text-muted-foreground">{t("skillLocations.count", { count: skills.length })}</span>
        <span className="mt-1 block text-xs text-muted-foreground">{t(comparison ? `skillLocations.result.${comparison.relation}` : "skillLocations.checkDuplicates")}</span>
      </span>
      {selectedCount > 0 && <span className="text-xs text-primary">{t("skillLocations.selected", { count: selectedCount })}</span>}
      {open ? <ChevronDown className="size-4" /> : <ChevronRight className="size-4" />}
    </button>
    <div id={id} hidden={!open}>
      {open && <div className="space-y-3 border-t border-border p-3">
        <p className="text-xs text-muted-foreground">{t("skillLocations.help")}</p>
        {agentId && <Button size="sm" variant="outline" disabled={comparing} onClick={() => void compare()}>
          {t(comparing ? "skillLocations.comparing" : "skillLocations.compare")}
        </Button>}
        {comparison && <p role="status" className="text-xs text-muted-foreground">{t(`skillLocations.${comparison.relation}`)}</p>}
        {skills.some((skill) => skill.name === "paseo" || skill.name.startsWith("paseo-")) &&
          <p className="text-xs text-muted-foreground">{t("skillLocations.paseoHelp")}</p>}
        {children}
      </div>}
    </div>
  </div>;
}

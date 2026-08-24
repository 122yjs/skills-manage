import { useEffect, useMemo, useState } from "react";
import { Blocks, Loader2, Search, Share2, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { SkillDetailDrawer } from "@/components/skill/SkillDetailDrawer";
import { UnifiedSkillCard } from "@/components/skill/UnifiedSkillCard";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { formatPathForDisplay } from "@/lib/path";
import { UNIVERSAL_AGENT_ID } from "@/lib/agents";
import { usePlatformStore } from "@/stores/platformStore";
import { useSkillStore } from "@/stores/skillStore";
import { useStorageStore } from "@/stores/storageStore";
import type { ScannedSkill } from "@/types";

const EMPTY_SKILLS: ScannedSkill[] = [];

export function UniversalInstallView() {
  const { t } = useTranslation();
  const agents = usePlatformStore((state) => state.agents);
  const scanGeneration = usePlatformStore((state) => state.scanGeneration ?? 0);
  const refreshCounts = usePlatformStore((state) => state.refreshCounts);
  const skills = useSkillStore(
    (state) => state.skillsByAgent[UNIVERSAL_AGENT_ID] ?? EMPTY_SKILLS
  );
  const isLoading = useSkillStore((state) => state.loadingByAgent[UNIVERSAL_AGENT_ID] ?? false);
  const pendingActions = useSkillStore((state) => state.pendingSkillActionKeys);
  const getSkillsByAgent = useSkillStore((state) => state.getSkillsByAgent);
  const uninstallSkill = useSkillStore((state) => state.uninstallSkillFromAgent);
  const centralPath = useStorageStore((state) => state.status?.central_path);
  const [query, setQuery] = useState("");
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [isRemoving, setIsRemoving] = useState(false);
  const [drawerSkill, setDrawerSkill] = useState<ScannedSkill | null>(null);
  const agent = agents.find((candidate) => candidate.id === UNIVERSAL_AGENT_ID);

  useEffect(() => {
    void getSkillsByAgent(UNIVERSAL_AGENT_ID);
  }, [getSkillsByAgent, scanGeneration]);

  useEffect(() => {
    const installedIds = new Set(skills.map((skill) => skill.id));
    setSelectedIds((current) => new Set([...current].filter((id) => installedIds.has(id))));
  }, [skills]);

  const filteredSkills = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return skills;
    return skills.filter(
      (skill) =>
        skill.name.toLowerCase().includes(normalized) ||
        skill.id.toLowerCase().includes(normalized) ||
        skill.description?.toLowerCase().includes(normalized)
    );
  }, [query, skills]);

  const removableSkills = useMemo(
    () => skills.filter((skill) => !skill.is_read_only),
    [skills]
  );
  const allSelected = removableSkills.length > 0 && removableSkills.every((skill) => selectedIds.has(skill.id));

  function toggleSkill(skillId: string) {
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(skillId)) next.delete(skillId);
      else next.add(skillId);
      return next;
    });
  }

  function toggleAll() {
    setSelectedIds(allSelected ? new Set() : new Set(removableSkills.map((skill) => skill.id)));
  }

  async function removeSkills(skillIds: string[]) {
    setIsRemoving(true);
    const results = await Promise.allSettled(
      skillIds.map((skillId) => uninstallSkill(skillId, UNIVERSAL_AGENT_ID))
    );
    await Promise.all([getSkillsByAgent(UNIVERSAL_AGENT_ID), refreshCounts()]);
    setIsRemoving(false);
    setConfirmOpen(false);
    setSelectedIds(new Set());

    const failed = results.filter((result) => result.status === "rejected").length;
    if (failed > 0) {
      toast.error(t("universal.removePartial", { failed }));
    } else {
      toast.success(t("universal.removeSuccess", { count: skillIds.length }));
    }
  }

  return (
    <div className="flex h-full flex-col">
      <div className="border-b border-border px-6 py-4">
        <div className="flex items-center gap-2.5">
          <Share2 className="size-6 text-primary/70" />
          <h1 className="text-xl font-semibold">{t("universal.title")}</h1>
        </div>
        <p className="mt-0.5 text-sm text-muted-foreground">
          {formatPathForDisplay(agent?.global_skills_dir ?? "~/.agents/skills/")}
        </p>
        <p className="mt-1 text-xs text-muted-foreground">{t("universal.description")}</p>
        {skills.some((skill) => skill.source_kind === "unmanaged") ? (
          <p className="mt-1 text-xs text-amber-700 dark:text-amber-300">
            {t("universal.externalHint")}
          </p>
        ) : null}
      </div>

      <div className="flex flex-col gap-3 border-b border-border px-6 py-3 lg:flex-row lg:items-center">
        <div className="relative flex-1">
          <Search className="pointer-events-none absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("universal.searchPlaceholder")}
            aria-label={t("universal.searchPlaceholder")}
            className="bg-muted/40 pl-8"
          />
        </div>
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Checkbox
            id="universal-select-all"
            checked={allSelected}
            disabled={removableSkills.length === 0}
            onCheckedChange={toggleAll}
            aria-label={t("universal.selectAll")}
          />
          <label htmlFor="universal-select-all" className="cursor-pointer select-none">
            {t("universal.selectAll")}
          </label>
        </div>
        <Button
          type="button"
          variant="destructive"
          disabled={selectedIds.size === 0 || isRemoving}
          onClick={() => setConfirmOpen(true)}
        >
          {isRemoving ? <Loader2 className="size-4 animate-spin" /> : <Trash2 className="size-4" />}
          {t("universal.removeSelected", { count: selectedIds.size })}
        </Button>
      </div>

      <div className="flex-1 overflow-auto p-6">
        {isLoading ? (
          <div className="flex h-full items-center justify-center gap-2 text-sm text-muted-foreground">
            <Loader2 className="size-4 animate-spin" />
            {t("universal.loading")}
          </div>
        ) : skills.length === 0 ? (
          <div className="flex h-full flex-col items-center justify-center gap-4 py-20 text-center">
            <div className="rounded-full bg-muted/60 p-4">
              <Blocks className="size-12 text-muted-foreground opacity-60" />
            </div>
            <p className="text-sm font-medium text-muted-foreground">{t("universal.empty")}</p>
            <p className="max-w-md text-xs text-muted-foreground">{t("universal.emptyHint")}</p>
          </div>
        ) : filteredSkills.length === 0 ? (
          <p className="py-20 text-center text-sm text-muted-foreground">
            {t("universal.noMatch", { query })}
          </p>
        ) : (
          <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
            {filteredSkills.map((skill) => (
              <UnifiedSkillCard
                key={skill.row_id ?? skill.id}
                name={skill.name}
                description={skill.description}
                translation={{
                  resourceId: `local:${skill.file_path}`,
                  filePath: skill.file_path,
                }}
                checkbox={skill.is_read_only
                  ? undefined
                  : { checked: selectedIds.has(skill.id), onChange: () => toggleSkill(skill.id) }}
                sourceType={skill.link_type as "symlink" | "copy" | "native"}
                isReadOnly={skill.is_read_only ?? false}
                isExternallyManaged={skill.source_kind === "unmanaged"}
                onDetail={() => setDrawerSkill(skill)}
                onUninstallFromPlatform={skill.is_read_only ? undefined : () => void removeSkills([skill.id])}
                uninstallFromLabel={t("universal.removeOne", { skill: skill.name })}
                isLoading={pendingActions[`${UNIVERSAL_AGENT_ID}::${skill.id}`] ?? false}
              />
            ))}
          </div>
        )}
      </div>

      <Dialog open={confirmOpen} onOpenChange={setConfirmOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("universal.confirmTitle", { count: selectedIds.size })}</DialogTitle>
            <DialogDescription>{t("universal.confirmDescription")}</DialogDescription>
          </DialogHeader>
          <DialogBody>
            <div className="rounded-lg border border-border bg-muted/20 p-3 text-sm">
              <p>{t("universal.originalKept")}</p>
              <code className="mt-2 block text-xs text-muted-foreground">
                {formatPathForDisplay(centralPath ?? "~/.skillsmanage/skills/")}
              </code>
            </div>
          </DialogBody>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setConfirmOpen(false)} disabled={isRemoving}>
              {t("common.cancel")}
            </Button>
            <Button type="button" variant="destructive" onClick={() => void removeSkills([...selectedIds])} disabled={isRemoving}>
              {isRemoving ? <Loader2 className="size-4 animate-spin" /> : null}
              {t("universal.confirmRemove")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <SkillDetailDrawer
        open={drawerSkill !== null}
        skillId={drawerSkill?.id ?? null}
        agentId={UNIVERSAL_AGENT_ID}
        rowId={drawerSkill?.row_id ?? null}
        onOpenChange={(open) => {
          if (!open) setDrawerSkill(null);
        }}
      />
    </div>
  );
}

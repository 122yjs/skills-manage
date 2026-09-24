import { useEffect, useState } from "react";
import { FolderOpen, Layers, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { UnifiedSkillCard } from "@/components/skill/UnifiedSkillCard";
import { SkillDetailDrawer } from "@/components/skill/SkillDetailDrawer";
import { GitHubSourceLink } from "@/components/skill/GitHubSourceLink";
import { invoke } from "@/lib/tauri";
import { cn } from "@/lib/utils";
import type { Collection } from "@/types";
import { groupSkillMembers, type SkillGroup, type SkillGroupMember } from "@/stores/skillGroupStore";

export function SkillGroupMenu({ groups, collections, selectedId, onSelect }: {
  groups: SkillGroup[]; collections: Collection[]; selectedId: string | null; onSelect: (id: string) => void;
}) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState("all");
  const entries = [
    ...groups,
    ...collections.map((c) => ({ id: `collection:${c.id}`, name: c.name, kind: "collection", folderPath: null })),
  ].filter((g) => (kind === "all" || kind === g.kind || (kind === "skillset" && (g.kind === "repository" || g.kind === "bundle"))) && g.name.toLowerCase().includes(query.toLowerCase()));
  return (
    <section className="space-y-3 border-b border-border px-6 py-4" aria-label={t("sidebar.collections")}>
      <p className="text-sm text-muted-foreground">{t("skillGroups.description")}</p>
      <div className="flex flex-wrap items-center gap-2">
        <div className="flex flex-wrap gap-1" aria-label={t("skillGroups.filter")}>
          {["all", "plugin", "skillset", "collection"].map((value) => (
            <Button key={value} size="sm" variant={kind === value ? "secondary" : "ghost"} aria-pressed={kind === value} onClick={() => setKind(value)}>
              {t(`skillGroups.${value}`)}
            </Button>
          ))}
        </div>
        <label className="ml-auto flex items-center gap-2 rounded-md border border-border px-3 py-2">
          <Search className="size-4 text-muted-foreground" />
          <input aria-label={t("skillGroups.search")} placeholder={t("skillGroups.search")} value={query} onChange={(e) => setQuery(e.target.value)} className="w-44 bg-transparent text-sm outline-none" />
        </label>
      </div>
      <div className="grid max-h-64 grid-cols-[repeat(auto-fill,minmax(min(100%,15rem),1fr))] gap-2 overflow-y-auto">
        {entries.map((g) => (
          <button key={g.id} type="button" aria-label={g.name} aria-pressed={selectedId === g.id} onClick={() => onSelect(g.id)}
            className={cn("flex min-w-0 items-center gap-2 rounded-md border px-3 py-2 text-left hover:bg-muted", selectedId === g.id ? "border-primary bg-primary/10" : "border-border")}>
            <Layers className="size-4 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1"><span className="block truncate text-sm font-medium" title={g.name}>{g.name}</span><span className="text-xs text-muted-foreground">{t(`skillGroups.${g.kind}`)}</span>{g.folderPath && <span className="block truncate text-xs text-muted-foreground" title={g.folderPath}>{g.folderPath}</span>}</span>
          </button>
        ))}
        {!entries.length && <p className="py-4 text-sm text-muted-foreground">{t("skillGroups.noMatch")}</p>}
      </div>
    </section>
  );
}

export function SkillGroupDetail({ group }: { group: SkillGroup }) {
  const { t } = useTranslation();
  const skills = groupSkillMembers(group.members);
  const [selected, setSelected] = useState<SkillGroupMember | null>(null);
  useEffect(() => { setSelected(null); }, [group.id]);
  return (
    <section className="px-6 py-4">
      <div className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <div><h2 className="font-semibold">{group.name}</h2><p className="text-sm text-muted-foreground">{t(`skillGroups.${group.kind}`)} · {t("skillGroups.sourceSkills", { count: group.sourceSkillCount ?? skills.length })} · {t("skillGroups.installedSkills", { count: skills.length })} · {t("skillGroups.locations", { count: group.members.length })}</p></div>
        <div className="flex gap-2">
          {group.repositoryUrl && <GitHubSourceLink href={group.repositoryUrl} className="text-sm underline">{t("skillGroups.repositoryFolder")}</GitHubSourceLink>}
          {group.folderPath && <Button variant="outline" size="sm" onClick={() => {
            void invoke("open_in_file_manager", { path: group.folderPath }).catch((error) => toast.error(String(error)));
          }}><FolderOpen className="size-4" />{t("skillGroups.folder")}</Button>}
        </div>
      </div>
      {!group.folderPath && <p className="mb-4 text-xs text-muted-foreground">{t("skillGroups.virtualGroup")}</p>}
      <div className="grid grid-cols-[repeat(auto-fill,minmax(min(100%,19rem),1fr))] gap-3">
        {skills.map(({ sourceKey, locations }) => {
          const member = locations[0];
          return (
            <div key={sourceKey} className="min-w-0 space-y-2">
              <UnifiedSkillCard
                name={member.name}
                description={member.description ?? undefined}
                translation={{ resourceId: `local:${member.filePath}`, filePath: member.filePath }}
                onDetail={() => setSelected(member)}
              />
              <details className="rounded-md border border-border px-3 py-2 text-xs">
                <summary className="cursor-pointer font-medium">{t("skillGroups.locations", { count: locations.length })}</summary>
                <ul className="mt-2 space-y-2">
                  {locations.map((location) => (
                    <li key={location.filePath} className="flex min-w-0 items-center gap-2">
                      <button className="min-w-0 flex-1 truncate text-left text-muted-foreground hover:underline"
                        title={location.filePath} onClick={() => setSelected(location)}>{location.filePath}</button>
                      <button className="shrink-0 rounded p-1 hover:bg-muted"
                        aria-label={t("skillGroups.locationFolder", { path: location.filePath })}
                        onClick={() => {
                          void invoke("open_in_file_manager", { path: location.filePath.replace(/[/\\]SKILL\.md$/, "") }).catch((error) => toast.error(String(error)));
                        }}><FolderOpen className="size-3.5" /></button>
                    </li>
                  ))}
                </ul>
              </details>
            </div>
          );
        })}
      </div>
      <SkillDetailDrawer open={selected !== null} skillId={selected?.skillId ?? null} agentId={selected?.agentId} rowId={selected?.rowId} onOpenChange={(open) => { if (!open) setSelected(null); }} />
    </section>
  );
}

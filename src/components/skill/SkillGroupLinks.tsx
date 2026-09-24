import { GitHubSourceLink } from "./GitHubSourceLink";
import { FolderOpen, Layers, ExternalLink } from "lucide-react";
import { Link } from "react-router-dom";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { invoke } from "@/lib/tauri";
import { skillGroupUrl, useSkillGroupStore } from "@/stores/skillGroupStore";

export function SkillGroupLinks({ filePath, onNavigate }: { filePath?: string; onNavigate?: () => void }) {
  const { t } = useTranslation();
  const groups = useSkillGroupStore((s) => s.groups);
  const memberships = filePath ? groups.filter((group) => group.members.some((m) => m.filePath === filePath)) : [];
  if (!memberships.length) return null;
  return (
    <div className="flex flex-wrap items-center gap-2" aria-label={t("skillGroups.sourceGroups")}>
      {memberships.map((group) => (
        <span key={group.id} className="inline-flex min-w-0 items-center gap-1 rounded-md border border-border px-2 py-1 text-xs">
          <Link to={skillGroupUrl(group.id)} className="inline-flex min-w-0 items-center gap-1 hover:underline" title={t("skillGroups.goTo", { name: group.name })} onClick={(event) => { event.stopPropagation(); onNavigate?.(); }}>
            <Layers className="size-3 shrink-0" />
            <span className="truncate">{group.name}</span>
          </Link>
          {group.repositoryUrl && <GitHubSourceLink href={group.repositoryUrl} className="rounded p-1 hover:bg-muted" aria-label={t("skillGroups.repositoryFolder")}><ExternalLink className="size-3.5" /></GitHubSourceLink>}
          {group.folderPath && (
            <button type="button" className="rounded p-1 hover:bg-muted" aria-label={t("skillGroups.openFolder", { name: group.name })}
              onClick={(event) => {
                event.stopPropagation();
                void invoke("open_in_file_manager", { path: group.folderPath }).catch((error) => toast.error(String(error)));
              }}>
              <FolderOpen className="size-3.5" />
            </button>
          )}
        </span>
      ))}
    </div>
  );
}

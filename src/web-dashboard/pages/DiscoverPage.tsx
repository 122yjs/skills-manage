import { useTranslation } from "react-i18next";
import { FolderKanbanIcon } from "lucide-react";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

import { EmptyPanel, SkillRow } from "../SkillRow";
import type { DashboardSnapshot } from "../types";

export function DiscoverPage({ snapshot }: { snapshot: DashboardSnapshot }) {
  const { t } = useTranslation();

  if (snapshot.discoveredProjects.length === 0) {
    return <EmptyPanel>{t("webDashboard.empty.discovered")}</EmptyPanel>;
  }

  return (
    <div className="space-y-4">
      {snapshot.discoveredProjects.map((project) => (
        <Card key={project.projectPath}>
          <CardHeader className="pb-2">
            <CardTitle className="flex items-center justify-between gap-3 text-base">
              <span className="flex min-w-0 items-center gap-2">
                <FolderKanbanIcon
                  className="size-4 shrink-0 text-primary"
                  aria-hidden="true"
                />
                <span className="truncate">{project.projectName}</span>
              </span>
              <span className="shrink-0 text-xs font-normal text-muted-foreground">
                {t("webDashboard.skillCount", { count: project.skills.length })}
              </span>
            </CardTitle>
            <div className="flex flex-wrap items-center gap-1.5">
              <p className="truncate text-xs text-muted-foreground">
                {project.projectPath}
              </p>
              {project.platforms.map((platform) => (
                <span
                  key={platform}
                  className="rounded-md bg-muted px-2 py-0.5 text-[11px] text-muted-foreground"
                >
                  {platform}
                </span>
              ))}
            </div>
          </CardHeader>
          <CardContent>
            <div className="grid gap-2 xl:grid-cols-2">
              {project.skills.map((skill) => (
                <SkillRow key={skill.id} skill={skill} />
              ))}
            </div>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

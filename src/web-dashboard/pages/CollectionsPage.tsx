import { useTranslation } from "react-i18next";
import { LayersIcon } from "lucide-react";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

import { EmptyPanel, SkillRow } from "../SkillRow";
import type { DashboardSnapshot } from "../types";

export function CollectionsPage({ snapshot }: { snapshot: DashboardSnapshot }) {
  const { t } = useTranslation();

  if (snapshot.collections.length === 0) {
    return <EmptyPanel>{t("webDashboard.empty.collections")}</EmptyPanel>;
  }

  return (
    <div className="grid gap-4 xl:grid-cols-2">
      {snapshot.collections.map((collection) => (
        <Card key={collection.id} className="min-w-0">
          <CardHeader className="pb-2">
            <CardTitle className="flex items-center justify-between gap-3 text-base">
              <span className="flex min-w-0 items-center gap-2">
                <LayersIcon className="size-4 shrink-0 text-primary" aria-hidden="true" />
                <span className="truncate">{collection.name}</span>
              </span>
              <span className="shrink-0 text-xs font-normal text-muted-foreground">
                {t("webDashboard.skillCount", { count: collection.skills.length })}
              </span>
            </CardTitle>
            {collection.description && (
              <p className="text-xs text-muted-foreground">{collection.description}</p>
            )}
          </CardHeader>
          <CardContent>
            {collection.skills.length === 0 ? (
              <p className="text-xs text-muted-foreground">
                {t("webDashboard.empty.collectionSkills")}
              </p>
            ) : (
              <div className="space-y-2">
                {collection.skills.map((skill) => (
                  <SkillRow key={skill.id} skill={skill} />
                ))}
              </div>
            )}
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

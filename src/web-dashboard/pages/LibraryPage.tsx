import { useTranslation } from "react-i18next";

import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

import { EmptyPanel, SkillRow } from "../SkillRow";
import type { DashboardSnapshot } from "../types";

/**
 * 스킬 라이브러리 페이지: 중앙 보관함 원본 + 플러그인 등 소스별 스킬 묶음.
 * 폴터 경로가 다른 묶음은 이름이 같아도 따로 보여주므로, 서로 다른 플러그인이
 * 같은 이름으로 정규화돼도 섞이지 않는다.
 */
export function LibraryPage({ snapshot }: { snapshot: DashboardSnapshot }) {
  const { t } = useTranslation();

  if (snapshot.libraryGroups.length === 0) {
    return <EmptyPanel>{t("webDashboard.empty.central")}</EmptyPanel>;
  }

  return (
    <div className="grid gap-4 xl:grid-cols-2">
      {snapshot.libraryGroups.map((group) => (
        <Card key={group.id} className="min-w-0">
          <CardHeader className="pb-2">
            <CardTitle className="flex items-center justify-between gap-3 text-base">
              <span className="truncate">{group.label}</span>
              <span className="shrink-0 text-xs font-normal text-muted-foreground">
                {t("webDashboard.skillCount", { count: group.skills.length })}
              </span>
            </CardTitle>
            <p className="truncate text-xs text-muted-foreground" title={group.path}>
              {group.path}
            </p>
          </CardHeader>
          <CardContent>
            <div className="max-h-96 space-y-2 overflow-y-auto pr-1">
              {group.skills.map((skill) => (
                <SkillRow key={`${skill.id}:${skill.path ?? ""}`} skill={skill} />
              ))}
            </div>
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

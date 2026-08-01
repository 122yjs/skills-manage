import { LockIcon } from "lucide-react";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/utils";

import type { DashboardSkillEntry } from "./types";

interface SkillRowProps {
  skill: DashboardSkillEntry;
  /** 행 오른쪽에 붙일 작은 배지 (예: 링크된 플랫폼 수) */
  trailing?: React.ReactNode;
}

/**
 * 스킬 한 줄을 보여주는 공용 행. 데스크톱 앱의 UnifiedSkillCard 처럼
 * 웹 대시보드의 모든 목록에서 이 컴포넌트 하나만 쓴다.
 */
export function SkillRow({ skill, trailing }: SkillRowProps) {
  const { t } = useTranslation();
  const isReadOnly = skill.linkType === "read-only";

  return (
    <article className="rounded-lg border border-border/70 bg-background/50 p-3 transition-colors hover:bg-muted/30">
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="flex items-center gap-1.5">
            <h3 className="truncate text-sm font-medium">{skill.name}</h3>
            {isReadOnly && (
              <span
                className="inline-flex shrink-0 items-center gap-1 rounded-md bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground"
                title={t("webDashboard.readOnlySource")}
              >
                <LockIcon className="size-2.5" aria-hidden="true" />
                {t("webDashboard.readOnlyBadge")}
              </span>
            )}
          </div>
          <p className="mt-1 line-clamp-2 text-xs leading-5 text-muted-foreground">
            {skill.description ?? t("webDashboard.noDescription")}
          </p>
        </div>
        {trailing && (
          <span
            className={cn(
              "shrink-0 rounded-md bg-secondary px-2 py-1 text-[11px]",
              "text-secondary-foreground",
            )}
          >
            {trailing}
          </span>
        )}
      </div>
      {skill.sourceLabel && (
        <p className="mt-2 truncate text-[11px] text-muted-foreground/80">
          {skill.sourceLabel}
        </p>
      )}
    </article>
  );
}

export function EmptyPanel({ children }: { children: string }) {
  return (
    <div className="rounded-lg border border-dashed p-8 text-center text-sm text-muted-foreground">
      {children}
    </div>
  );
}

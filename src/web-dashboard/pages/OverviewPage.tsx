import { useTranslation } from "react-i18next";
import {
  BotIcon,
  BoxesIcon,
  DatabaseIcon,
  RadarIcon,
  StoreIcon,
} from "lucide-react";

import { Card, CardContent } from "@/components/ui/card";
import { RECOMMENDED_SKILLS } from "@/data/officialSources";

import type { DashboardSnapshot } from "../types";

interface StatCardProps {
  label: string;
  value: number;
  detail: string;
  icon: typeof DatabaseIcon;
}

function StatCard({ label, value, detail, icon: Icon }: StatCardProps) {
  return (
    <Card size="sm" className="min-h-28 bg-card/85">
      <CardContent className="flex h-full items-start justify-between gap-4 pt-1">
        <div className="space-y-1.5">
          <p className="text-xs font-medium tracking-wide text-muted-foreground uppercase">
            {label}
          </p>
          <p className="font-heading text-3xl font-semibold tabular-nums">{value}</p>
          <p className="text-xs text-muted-foreground">{detail}</p>
        </div>
        <span className="rounded-xl bg-primary/12 p-2.5 text-primary ring-1 ring-primary/20">
          <Icon className="size-5" aria-hidden="true" />
        </span>
      </CardContent>
    </Card>
  );
}

export function OverviewPage({ snapshot }: { snapshot: DashboardSnapshot }) {
  const { t } = useTranslation();

  return (
    <div className="space-y-6">
      <section
        className="grid gap-4 sm:grid-cols-2 xl:grid-cols-5"
        aria-label={t("webDashboard.summaryLabel")}
      >
        <StatCard
          label={t("webDashboard.stats.central")}
          value={snapshot.summary.centralSkillCount}
          detail={t("webDashboard.stats.centralDetail")}
          icon={DatabaseIcon}
        />
        <StatCard
          label={t("webDashboard.stats.platforms")}
          value={snapshot.summary.detectedPlatformCount}
          detail={t("webDashboard.stats.platformsDetail")}
          icon={BotIcon}
        />
        <StatCard
          label={t("webDashboard.stats.collections")}
          value={snapshot.summary.collectionCount}
          detail={t("webDashboard.stats.collectionsDetail")}
          icon={BoxesIcon}
        />
        <StatCard
          label={t("webDashboard.stats.discovered")}
          value={snapshot.summary.discoveredProjectCount}
          detail={t("webDashboard.stats.discoveredDetail", {
            count: snapshot.summary.discoveredSkillCount,
          })}
          icon={RadarIcon}
        />
        <StatCard
          label={t("webDashboard.stats.marketplace")}
          value={Math.max(
            snapshot.summary.marketplaceSkillCount,
            RECOMMENDED_SKILLS.length,
          )}
          detail={
            snapshot.summary.marketplaceSkillCount > 0
              ? t("webDashboard.stats.marketplaceDetail")
              : t("webDashboard.stats.marketplaceDefaultDetail")
          }
          icon={StoreIcon}
        />
      </section>

      <Card>
        <CardContent className="pt-1">
          <h2 className="mb-3 text-sm font-semibold">
            {t("webDashboard.sections.platforms")}
          </h2>
          <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
            {snapshot.platforms
              .filter((platform) => platform.isEnabled && platform.isDetected)
              .slice(0, 9)
              .map((platform) => (
                <div
                  key={platform.id}
                  className="flex items-center justify-between rounded-lg bg-muted/25 px-3 py-2 ring-1 ring-border"
                >
                  <span className="truncate text-sm">{platform.displayName}</span>
                  <span className="shrink-0 text-xs text-muted-foreground">
                    {t("webDashboard.skillCount", { count: platform.skills.length })}
                  </span>
                </div>
              ))}
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

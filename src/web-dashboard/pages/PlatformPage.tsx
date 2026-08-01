import { useParams } from "react-router-dom";
import { useTranslation } from "react-i18next";

import { cn } from "@/lib/utils";
import { PlatformIcon } from "@/components/platform/PlatformIcon";

import { EmptyPanel, SkillRow } from "../SkillRow";
import type { DashboardSnapshot } from "../types";

export function PlatformPage({ snapshot }: { snapshot: DashboardSnapshot }) {
  const { t } = useTranslation();
  const { agentId } = useParams<{ agentId: string }>();
  const platform = snapshot.platforms.find((entry) => entry.id === agentId);

  if (!platform) {
    return <EmptyPanel>{t("webDashboard.empty.platform")}</EmptyPanel>;
  }

  const active = platform.isEnabled && platform.isDetected;

  return (
    <div className="space-y-4">
      <header className="flex flex-wrap items-center gap-3">
        <span className="rounded-xl bg-muted/40 p-2 ring-1 ring-border">
          <PlatformIcon agentId={platform.id} className="size-5" />
        </span>
        <div className="min-w-0 flex-1">
          <h2 className="truncate text-lg font-semibold">{platform.displayName}</h2>
          <p className="truncate text-xs text-muted-foreground">
            {platform.globalSkillsDir}
          </p>
        </div>
        <span
          className={cn(
            "inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-xs ring-1",
            active
              ? "bg-[color-mix(in_srgb,var(--ctp-green)_12%,transparent)] text-[var(--ctp-green)] ring-[color-mix(in_srgb,var(--ctp-green)_30%,transparent)]"
              : "bg-muted text-muted-foreground ring-border",
          )}
        >
          <span
            className={cn(
              "size-1.5 rounded-full",
              active ? "bg-[var(--ctp-green)]" : "bg-muted-foreground/50",
            )}
            aria-hidden="true"
          />
          {active
            ? t("webDashboard.status.detected")
            : t("webDashboard.status.notDetected")}
        </span>
      </header>

      {platform.skills.length === 0 ? (
        <EmptyPanel>{t("webDashboard.empty.platformSkills")}</EmptyPanel>
      ) : (
        <div className="grid gap-2 xl:grid-cols-2">
          {platform.skills.map((skill) => (
            <SkillRow key={`${skill.id}:${skill.path ?? ""}`} skill={skill} />
          ))}
        </div>
      )}
    </div>
  );
}

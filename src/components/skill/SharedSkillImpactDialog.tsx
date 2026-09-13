import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type {
  SharedSkillConfirmedPlatform,
  SharedSkillImpact,
} from "@/types";
import { formatPathForDisplay } from "@/lib/path";

/** 현재 플랫폼을 먼저, 나머지는 표시 이름 순으로 정렬한다. */
function sortSharedPlatforms<T extends SharedSkillConfirmedPlatform>(
  platforms: T[],
  currentAgentId?: string | null,
): T[] {
  return [...platforms].sort((left, right) => {
    if (currentAgentId) {
      if (left.agent_id === currentAgentId && right.agent_id !== currentAgentId) return -1;
      if (right.agent_id === currentAgentId && left.agent_id !== currentAgentId) return 1;
    }
    return left.display_name.localeCompare(right.display_name);
  });
}

export interface SharedSkillImpactDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** 단일 토글은 1건, 공용 일괄은 여러 건을 전달한다. */
  impacts: SharedSkillImpact[];
  currentAgentId?: string | null;
  isConfirming: boolean;
  onConfirm: () => void;
  title: string;
  description?: string;
  confirmLabel: string;
}

function ImpactSection({
  impact,
  currentAgentId,
  sectionId,
}: {
  impact: SharedSkillImpact;
  currentAgentId?: string | null;
  sectionId: string;
}) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  useEffect(() => {
    setExpanded(false);
  }, [impact.shared_install_id, impact.confirmation_token]);
  const confirmed = sortSharedPlatforms(impact.confirmed_platforms ?? [], currentAgentId);
  const separate = [...(impact.separate_installs ?? [])].sort((left, right) =>
    left.display_name.localeCompare(right.display_name),
  );
  const restricted = impact.reason != null && impact.reason !== "";

  return (
    <section
      aria-label={impact.skill_name}
      className="rounded-lg border border-border p-3"
    >
      <div className="flex min-w-0 items-start justify-between gap-2">
        <p className="min-w-0 flex-1 break-words text-sm font-medium">
          <span title={impact.skill_name}>{impact.skill_name}</span>
        </p>
        <span className="shrink-0 rounded-full bg-muted px-2 py-0.5 text-[11px] text-muted-foreground">
          {t("sharedImpact.confirmedCount", { count: confirmed.length })}
        </span>
      </div>
      <p className="mt-1 min-w-0 truncate text-xs text-muted-foreground" title={impact.management_path}>
        {t("sharedImpact.managementPath", { path: formatPathForDisplay(impact.management_path) })}
      </p>
      {restricted && (
        <p role="alert" className="mt-2 break-words text-xs text-amber-700 dark:text-amber-300">
          {t("sharedImpact.restricted", { reason: impact.reason })}
        </p>
      )}
      <p className="mt-2 break-words text-xs text-muted-foreground">
        {t("sharedImpact.unconfirmedWarning")}
      </p>
      {separate.length > 0 && (
        <p className="mt-1 break-words text-xs text-amber-700 dark:text-amber-300">
          {t("sharedImpact.separateWarning", { count: separate.length })}
        </p>
      )}
      <div className="mt-2">
        <button
          type="button"
          aria-expanded={expanded}
          aria-controls={sectionId}
          onClick={() => setExpanded((value) => !value)}
          className="rounded-md px-1 py-0.5 text-xs font-medium text-primary hover:bg-primary/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
        >
          {expanded ? t("sharedImpact.hideDetails") : t("sharedImpact.showDetails")}
        </button>
        {expanded && (
          <div id={sectionId} className="mt-1 max-h-[240px] space-y-2 overflow-y-auto pr-1">
            <div>
              <p className="text-xs font-medium">
                {t("sharedImpact.confirmedPlatforms", { count: confirmed.length })}
              </p>
              {confirmed.length === 0 ? (
                <p className="mt-0.5 text-xs text-muted-foreground">
                  {t("sharedImpact.confirmedEmpty")}
                </p>
              ) : (
                <ul className="mt-0.5 space-y-0.5">
                  {confirmed.map((platform) => (
                    <li
                      key={platform.agent_id}
                      className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground"
                    >
                      <span className="min-w-0 flex-1 truncate" title={platform.display_name}>
                        {platform.display_name}
                      </span>
                      {currentAgentId && platform.agent_id === currentAgentId && (
                        <span className="shrink-0 rounded bg-muted px-1 text-[10px]">
                          {t("sharedImpact.currentPlatform")}
                        </span>
                      )}
                    </li>
                  ))}
                </ul>
              )}
            </div>
            <div>
              <p className="text-xs font-medium">
                {t("sharedImpact.separateInstalls", { count: separate.length })}
              </p>
              {separate.length === 0 ? (
                <p className="mt-0.5 text-xs text-muted-foreground">
                  {t("sharedImpact.separateEmpty")}
                </p>
              ) : (
                <>
                  <ul className="mt-0.5 space-y-0.5">
                    {separate.map((item) => (
                      <li key={`${item.agent_id}::${item.source_path}`} className="min-w-0 text-xs text-muted-foreground">
                        <span className="block truncate" title={item.display_name}>
                          {item.display_name}
                        </span>
                        <span className="block truncate text-[11px] opacity-80" title={item.source_path}>
                          {formatPathForDisplay(item.source_path)}
                        </span>
                      </li>
                    ))}
                  </ul>
                  <p className="mt-0.5 break-words text-[11px] text-muted-foreground">
                    {t("sharedImpact.separateNote")}
                  </p>
                </>
              )}
            </div>
          </div>
        )}
      </div>
    </section>
  );
}

export function SharedSkillImpactDialog({
  open,
  onOpenChange,
  impacts,
  currentAgentId,
  isConfirming,
  onConfirm,
  title,
  description,
  confirmLabel,
}: SharedSkillImpactDialogProps) {
  const { t } = useTranslation();
  const restricted = impacts.some((impact) => impact.reason != null && impact.reason !== "");

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[80vh] flex-col sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle className="break-words">{title}</DialogTitle>
          {description && <DialogDescription className="break-words">{description}</DialogDescription>}
        </DialogHeader>
        <DialogBody className="min-h-0 flex-1 space-y-2 overflow-y-auto">
          {impacts.length === 0 ? (
            <p className="text-sm text-muted-foreground">{t("sharedImpact.confirmedEmpty")}</p>
          ) : (
            impacts.map((impact) => (
              <ImpactSection
                key={impact.shared_install_id}
                impact={impact}
                currentAgentId={currentAgentId}
                sectionId={`shared-impact-${impact.shared_install_id}`}
              />
            ))
          )}
        </DialogBody>
        <DialogFooter className="shrink-0">
          <Button type="button" variant="outline" onClick={() => onOpenChange(false)} disabled={isConfirming}>
            {t("common.cancel")}
          </Button>
          <Button
            type="button"
            variant={impacts[0] && !impacts[0].enabled ? "default" : "destructive"}
            onClick={onConfirm}
            disabled={isConfirming || restricted || impacts.length === 0}
          >
            {confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

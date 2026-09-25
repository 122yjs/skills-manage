import { SkillGroupLinks } from "./SkillGroupLinks";
import { GitHubSourceLink } from "@/components/skill/GitHubSourceLink";
import {
  PackagePlus,
  Check,
  Link2,
  FolderOpen,
  Folder,
  Globe,
  ArrowUpRight,
  Plus,
  ChevronRight,
  X,
  Loader2,
  Lock,
  Trash2,
  RotateCcw,
  MoreHorizontal,
} from "lucide-react";
import { useRef, useState, type MouseEventHandler, type Ref } from "react";
import { useTranslation } from "react-i18next";
import { Checkbox } from "@/components/ui/checkbox";
import { Switch } from "@/components/ui/switch";
import { InlineConfirmAction } from "@/components/ui/inline-confirm-action";
import { PlatformIcon } from "@/components/platform/PlatformIcon";
import type {
  AgentWithStatus,
  ClaudeSourceKind,
  GitHubSkillOriginSummary,
  SharedSkillImpact,
  SkillDescriptionTranslationMeta,
} from "@/types";
import { cn } from "@/lib/utils";
import { getAgentDisplayName, getDistinctInstallTargetAgents } from "@/lib/agents";
import { LocalizedSkillDescription } from "@/components/skill/LocalizedSkillDescription";
import { formatPathForDisplay } from "@/lib/path";
import { githubSkillSourceUrl } from "@/lib/skillOrigin";

const FEATURED_CODING_AGENT_IDS = [
  "cursor",
  "trae",
  "claude-code",
  "windsurf",
  "codex",
  "qwen",
];

// ─── Platform Toggle Icon (internal) ──────────────────────────────────────────

function PlatformToggleIcon({
  agent,
  skillName,
  isLinked,
  isReadOnly,
  isToggling,
  onToggle,
}: {
  agent: AgentWithStatus;
  skillName: string;
  isLinked: boolean;
  isReadOnly: boolean;
  isToggling: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const displayName = getAgentDisplayName(agent, t("sidebar.universal"));
  return (
    <button
      type="button"
      className={cn(
        "inline-flex h-7 w-7 items-center justify-center rounded-md transition-colors cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        isLinked && !isReadOnly
          ? "text-primary hover:bg-primary/10"
          : "text-muted-foreground/40 hover:bg-muted/60 hover:text-muted-foreground",
        isReadOnly && "cursor-default hover:bg-transparent",
        isToggling && "animate-pulse pointer-events-none"
      )}
      title={displayName}
      aria-label={t("skillUsage.toggleSkill", { name: `${skillName} (${displayName})` })}
      aria-pressed={isLinked && !isReadOnly}
      disabled={isToggling || isReadOnly}
      onClick={onToggle}
    >
      <PlatformIcon
        agentId={agent.id}
        className={cn(
          "size-4 shrink-0 transition-all",
          isLinked && !isReadOnly ? "opacity-100 grayscale-0" : "opacity-40 grayscale"
        )}
        size={16}
      />
    </button>
  );
}

// ─── Types ────────────────────────────────────────────────────────────────────

export interface UnifiedSkillCardProps {
  /** Core data — always required. */
  name: string;
  description?: string;
  className?: string;
  /** 목록 보기에서도 같은 카드와 설치 동작을 재사용한다. */
  layout?: "grid" | "list";
  /** 설명 다국어 선택 및 번역에 필요한 스킬별 식별 정보. */
  translation?: SkillDescriptionTranslationMeta;

  /** Click the card itself (platform variant navigates to detail). */
  onClick?: () => void;

  // ── discover variant ──
  checkbox?: { checked: boolean; onChange: () => void; disabled?: boolean };
  isCentral?: boolean;
  platformBadge?: { id: string; name: string };
  projectBadge?: string;

  // ── central variant ──
  platformIcons?: {
    agents: AgentWithStatus[];
    linkedAgents: string[];
    readOnlyAgents?: string[];
    /** 비활성 관리 설치도 카드에서 다시 활성으로 바꿀 수 있도록 별도로 전달한다. */
    usageByAgent?: Record<string, { enabled: boolean; paused_by_bulk: boolean }>;
    skillId: string;
    onToggle: (skillId: string, agentId: string) => void;
    onManage?: () => void;
    togglingAgentId: string | null;
  };

  // ── platform variant ──
  sourceType?: "symlink" | "copy" | "native";
  originKind?: ClaudeSourceKind | null;
  isReadOnly?: boolean;
  isUniversalSource?: boolean;
  isExternallyManaged?: boolean;
  /** 앱이 관리하는 설치의 실제 활성 상태. 설치 파일 삭제와는 별개다. */
  usageControl?: {
    enabled: boolean;
    pausedByBulk?: boolean;
    onCheckedChange: (enabled: boolean) => void;
    isLoading?: boolean;
    disabledReason?: string;
    state?: "active" | "inactive" | "deleted" | "unsupported" | string;
  };
  /** 공용 설치의 공통 on/off와 이 플랫폼 제외 상태를 구분해서 보여 준다. */
  sharedControl?: {
    impact: SharedSkillImpact;
    excludedHere: boolean;
    onToggleShared: () => void;
    isLoading?: boolean;
    individual?: {
      enabled: boolean;
      canToggle: boolean;
      disabledReason?: string;
      onToggle: (enabled: boolean) => void;
      isLoading?: boolean;
      platformDisplayName: string;
    } | null;
  };
  /** 적용 삭제 상태에서만 표시하는 명시적 재적용 동작. */
  onReapplyPlatform?: () => void;
  reapplyPlatformLabel?: string;
  /** 지원 범위가 이름 단위처럼 넓어질 때 보여 주는 안내. */
  platformControlNotice?: string;
  /** 비활성으로 바꿔도 남아 있는 공용/플러그인 제공 항목 수다. */
  externalUsageCount?: number;

  // ── origin (persisted github provenance) ──
  /** Owner/repo + original repo path recorded when this skill was imported. */
  origin?: GitHubSkillOriginSummary | null;
  /** Local installation ID; defaults to the central variant's platformIcons.skillId. */
  installId?: string;

  // ── marketplace variant ──
  isInstalled?: boolean;
  tags?: { key: string; label: string }[];
  publisher?: string;

  // ── actions (pass only the ones relevant to the context) ──
  onDetail?: MouseEventHandler<HTMLButtonElement>;
  onInstallTo?: () => void;
  onInstallToCentral?: () => void;
  onInstallToPlatform?: () => void;
  onUninstallFromPlatform?: () => void;
  onManageUniversal?: () => void;
  platformDisplayName?: string;
  uninstallFromLabel?: string;
  uninstallConfirmLabel?: string;
  uninstallRequiresDialog?: boolean;
  onDeleteFromCentral?: () => void;
  deleteFromCentralLabel?: string;
  deleteFromCentralRequiresDialog?: boolean;
  onInstall?: () => void;
  onRemove?: () => void;
  isLoading?: boolean;
  detailButtonRef?: Ref<HTMLButtonElement>;
}

// ─── UnifiedSkillCard ─────────────────────────────────────────────────────────

export function UnifiedSkillCard(props: UnifiedSkillCardProps) {
  const { t } = useTranslation();
  const [confirmPlatformRemoval, setConfirmPlatformRemoval] = useState(false);
  const platformActionsRef = useRef<HTMLDetailsElement>(null);
  const {
    name,
    description,
    className,
    layout = "grid",
    translation,
    onClick,
    checkbox,
    isCentral,
    platformBadge,
    projectBadge,
    platformIcons,
    sourceType,
    originKind,
    isReadOnly,
    isUniversalSource,
    isExternallyManaged,
    usageControl,
    sharedControl,
    onReapplyPlatform,
    reapplyPlatformLabel,
    platformControlNotice,
    externalUsageCount = 0,
    isInstalled,
    origin,
    installId,
    tags,
    publisher,
    onDetail,
    onInstallTo,
    onInstallToCentral,
    onInstallToPlatform,
    onUninstallFromPlatform,
    onManageUniversal,
    platformDisplayName,
    uninstallFromLabel,
    uninstallConfirmLabel,
    uninstallRequiresDialog,
    onDeleteFromCentral,
    deleteFromCentralLabel,
    deleteFromCentralRequiresDialog,
    onInstall,
    onRemove,
    isLoading,
    detailButtonRef,
  } = props;

  const isPlatformCard = Boolean(platformDisplayName || sharedControl || usageControl || sourceType);

  // Determine variant features
  const hasCheckbox = !!checkbox;
  const hasPlatformIcons = !!platformIcons;
  const hasActions = !!(
    onDetail ||
    onInstallTo ||
    onInstallToCentral ||
    onInstallToPlatform ||
    onUninstallFromPlatform ||
    onReapplyPlatform ||
    (onManageUniversal && !isPlatformCard) ||
    onDeleteFromCentral ||
    onInstall ||
    onRemove
  );

  // Show all Lobster platforms, but only the highest-frequency Coding platforms.
  const targetPlatformAgents = platformIcons
    ? getDistinctInstallTargetAgents(platformIcons.agents)
    : [];
  const lobsterAgents = targetPlatformAgents.filter((agent) => agent.category === "lobster");
  const codingAgents = targetPlatformAgents.filter((agent) => agent.category !== "lobster");
  const linkedAgentIds = new Set(platformIcons?.linkedAgents ?? []);
  const readOnlyAgentIds = new Set(platformIcons?.readOnlyAgents ?? []);
  /** Local installation ID: collection callers pass it; the central variant carries it as platformIcons.skillId. */
  const localSkillId = installId ?? platformIcons?.skillId;
  const usageByAgent = platformIcons?.usageByAgent ?? {};
  const featuredCodingAgents = FEATURED_CODING_AGENT_IDS
    .map((agentId) => codingAgents.find((agent) => agent.id === agentId))
    .filter((agent): agent is AgentWithStatus => !!agent);
  const featuredCodingAgentIds = new Set(featuredCodingAgents.map((agent) => agent.id));
  const hiddenCodingCount = codingAgents.filter((agent) => !featuredCodingAgentIds.has(agent.id)).length;

  // ── Platform variant: clickable card style ──
  if (onClick && !hasActions && !hasCheckbox && !hasPlatformIcons) {
    if (translation) {
      return (
        <div
          className={cn(
            "w-full h-full rounded-xl bg-card ring-1 ring-border shadow-sm p-3 flex flex-col gap-2 transition-all hover:ring-primary/25 hover:bg-accent/30",
            className
          )}
        >
          <button
            type="button"
            onClick={onClick}
            className="flex flex-1 items-start justify-between gap-3 text-left focus:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-md"
            aria-label={t("platform.searchSkillLabel", { name })}
          >
            <div className="min-w-0 flex-1 space-y-1">
              <div className="font-medium text-sm text-foreground truncate">{name}</div>
              {sourceType && <SourceIndicator sourceType={sourceType} />}
            </div>
            <ChevronRight className="size-4 text-muted-foreground shrink-0 mt-0.5" />
          </button>
          <LocalizedSkillDescription {...translation} description={description} />
        </div>
      );
    }

    return (
      <button
        role="button"
        onClick={onClick}
        className={cn(
          "w-full h-full text-left focus:outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-xl",
          className
        )}
        aria-label={t("platform.searchSkillLabel", { name })}
      >
        <div className="h-full flex flex-col rounded-xl bg-card ring-1 ring-border shadow-sm p-3 gap-3 transition-all hover:ring-primary/25 hover:bg-accent/30 cursor-pointer">
          <div className="flex flex-1 items-start justify-between gap-3">
            <div className="min-w-0 flex-1 space-y-1">
              <div className="font-medium text-sm text-foreground truncate">{name}</div>
              {description && (
                <p className="text-xs text-muted-foreground line-clamp-2 leading-relaxed">{description}</p>
              )}
              {sourceType && <SourceIndicator sourceType={sourceType} />}
            </div>
            <ChevronRight className="size-4 text-muted-foreground shrink-0 mt-0.5" />
          </div>
        </div>
      </button>
    );
  }

  // ── Default card style (central, discover, marketplace) ──
  return (
    <div
      className={cn(
        "rounded-xl bg-card ring-1 ring-border shadow-sm p-3 flex flex-col transition-colors hover:ring-primary/30 focus-within:ring-primary/50",
        layout === "list" && "skill-card--list",
        checkbox?.checked && "ring-primary/40 bg-primary/5",
        isLoading && "opacity-50",
        className
      )}
    >
      <div className="flex items-start gap-2.5">
        {/* Optional checkbox (discover) */}
        {hasCheckbox && (
          <div className="pt-0.5">
            <Checkbox
              checked={checkbox.checked}
              onCheckedChange={checkbox.onChange}
              disabled={checkbox.disabled}
              aria-label={t("discover.selectSkill")}
            />
          </div>
        )}

        {/* Main content */}
        <div className="skill-card-content flex-1 min-w-0 space-y-1.5">
          {/* Row 1: Name + icon actions */}
          <div className="flex items-center justify-between gap-2">
            {/* Skill name — clickable if onDetail provided */}
            {onDetail ? (
              <button
                ref={detailButtonRef}
                className="rounded-sm font-medium text-sm text-foreground truncate hover:text-primary hover:underline text-left min-w-0 flex-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                onClick={onDetail}
                aria-label={t("central.viewDetailsLabel", { name })}
              >
                {name}
              </button>
            ) : (
              <h3 className="text-sm font-medium truncate min-w-0 flex-1">{name}</h3>
            )}

            {/* Icon action buttons */}
            {hasActions && (
              <div className="flex items-center gap-0.5 shrink-0">
                {/* Install To... (central / platform / collection / marketplace) */}
                {onInstallTo && (
                  <button
                    onClick={onInstallTo}
                    disabled={isLoading}
                    title={t("central.installTo")}
                    aria-label={t("central.installLabel", { name })}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-md transition-colors text-muted-foreground hover:bg-primary/10 hover:text-primary disabled:opacity-50 disabled:cursor-default"
                  >
                    <PackagePlus className="size-4" />
                  </button>
                )}

                {/* Install to Central (discover) */}
                {onInstallToCentral && !isCentral && (
                  <button
                    onClick={onInstallToCentral}
                    disabled={isLoading}
                    title={t("discover.installToCentral")}
                    aria-label={t("discover.installToCentral")}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-md transition-colors text-muted-foreground hover:bg-primary/10 hover:text-primary disabled:opacity-50 disabled:cursor-default"
                  >
                    {isLoading ? <Loader2 className="size-4 animate-spin" /> : <ArrowUpRight className="size-4" />}
                  </button>
                )}

                {/* Install to Platform (discover) */}
                {onInstallToPlatform && (
                  <button
                    onClick={onInstallToPlatform}
                    disabled={isLoading}
                    title={t("discover.installToPlatform")}
                    aria-label={t("discover.installToPlatform")}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-md transition-colors text-muted-foreground hover:bg-primary/10 hover:text-primary disabled:opacity-50 disabled:cursor-default"
                  >
                    {isLoading ? <Loader2 className="size-4 animate-spin" /> : <Plus className="size-4" />}
                  </button>
                )}

                {!isPlatformCard && onUninstallFromPlatform && (uninstallRequiresDialog ? (
                  <button type="button" onClick={onUninstallFromPlatform} disabled={isLoading}
                    aria-label={uninstallFromLabel ?? t("common.uninstall")}
                    title={uninstallFromLabel ?? t("common.uninstall")}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/10 hover:text-destructive disabled:opacity-50">
                    <Trash2 className="size-4" />
                  </button>
                ) : (
                  <InlineConfirmAction
                    onConfirm={onUninstallFromPlatform}
                    isLoading={isLoading}
                    idleTitle={uninstallFromLabel ?? t("common.uninstall")}
                    idleAriaLabel={uninstallFromLabel ?? t("common.uninstall")}
                    confirmLabel={uninstallConfirmLabel ?? t("common.confirmDelete")}
                    icon={<X className="size-4" />}
                  />
                ))}

                {!isPlatformCard && onReapplyPlatform && (
                  <button
                    type="button"
                    onClick={onReapplyPlatform}
                    disabled={isLoading}
                    title={reapplyPlatformLabel ?? t("common.reapply")}
                    aria-label={reapplyPlatformLabel ?? t("common.reapply")}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-md text-muted-foreground transition-colors hover:bg-primary/10 hover:text-primary disabled:cursor-default disabled:opacity-50"
                  >
                    {isLoading ? <Loader2 className="size-4 animate-spin" /> : <RotateCcw className="size-4" />}
                  </button>
                )}

                {!isPlatformCard && onManageUniversal && (
                  <button
                    type="button"
                    onClick={onManageUniversal}
                    className="rounded-md px-2 py-1 text-xs font-medium text-primary hover:bg-primary/10"
                    aria-label={t("platform.manageUniversal")}
                  >
                    {t("platform.manageUniversal")}
                  </button>
                )}

                {isPlatformCard && (onUninstallFromPlatform || onReapplyPlatform) && (
                  <details ref={platformActionsRef} className="relative" onClick={(event) => event.stopPropagation()}>
                    <summary className="flex size-8 cursor-pointer list-none items-center justify-center rounded-md text-muted-foreground hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring" aria-label={t("platform.cardActions", { name })} onClick={() => setConfirmPlatformRemoval(false)}>
                      <MoreHorizontal className="size-4" aria-hidden="true" />
                    </summary>
                    <div className="absolute right-0 z-20 mt-1 flex w-60 max-w-[calc(100vw-2rem)] flex-col gap-1 rounded-lg border border-border bg-popover p-1.5 shadow-md">
                      {onReapplyPlatform && (
                        <button type="button" onClick={() => { onReapplyPlatform(); if (platformActionsRef.current) platformActionsRef.current.open = false; }} disabled={isLoading}
                          className="rounded-md px-2 py-1.5 text-left text-xs hover:bg-muted disabled:opacity-50">
                          {reapplyPlatformLabel ?? t("common.reapply")}
                        </button>
                      )}
                      {onUninstallFromPlatform && (
                        <button type="button" disabled={isLoading}
                          onClick={() => {
                            if (uninstallRequiresDialog || confirmPlatformRemoval) {
                              onUninstallFromPlatform();
                              setConfirmPlatformRemoval(false);
                              if (platformActionsRef.current) platformActionsRef.current.open = false;
                            } else {
                              setConfirmPlatformRemoval(true);
                            }
                          }}
                          className="rounded-md px-2 py-1.5 text-left text-xs text-destructive hover:bg-destructive/10 disabled:opacity-50">
                          {confirmPlatformRemoval ? (uninstallConfirmLabel ?? t("platform.confirmRemoveFromPlatform", { platform: platformDisplayName, name })) : (uninstallFromLabel ?? t("common.uninstall"))}
                        </button>
                      )}
                      {confirmPlatformRemoval && (
                        <button type="button" onClick={() => setConfirmPlatformRemoval(false)}
                          className="rounded-md px-2 py-1.5 text-left text-xs hover:bg-muted">{t("common.cancel")}</button>
                      )}
                    </div>
                  </details>
                )}

                {onDeleteFromCentral &&
                  (deleteFromCentralRequiresDialog ? (
                    <button
                      onClick={onDeleteFromCentral}
                      disabled={isLoading}
                      title={deleteFromCentralLabel ?? t("common.delete")}
                      aria-label={deleteFromCentralLabel ?? t("common.delete")}
                      className="inline-flex h-8 w-8 items-center justify-center rounded-md transition-colors text-muted-foreground hover:text-destructive hover:bg-destructive/10 disabled:opacity-50 disabled:cursor-default"
                    >
                      {isLoading ? <Loader2 className="size-4 animate-spin" /> : <Trash2 className="size-4" />}
                    </button>
                  ) : (
                    <InlineConfirmAction
                      onConfirm={onDeleteFromCentral}
                      isLoading={isLoading}
                      idleTitle={deleteFromCentralLabel ?? t("common.delete")}
                      idleAriaLabel={deleteFromCentralLabel ?? t("common.delete")}
                      confirmLabel={t("common.confirmDelete")}
                      icon={<Trash2 className="size-4" />}
                    />
                  ))}

                {/* Marketplace installed indicator (disabled Check icon) */}
                {onInstall && isInstalled && (
                  <button
                    disabled
                    title={t("marketplace.installed")}
                    aria-label={t("marketplace.installed")}
                    className="inline-flex h-8 w-8 items-center justify-center rounded-md text-primary cursor-default"
                  >
                    <Check className="size-4" />
                  </button>
                )}

                {/* Remove (collection) */}
                {onRemove && (
                  <InlineConfirmAction
                    onConfirm={onRemove}
                    isLoading={isLoading}
                    idleTitle={t("collection.removeSkillLabel", { name })}
                    idleAriaLabel={t("collection.removeSkillLabel", { name })}
                    confirmLabel={t("common.confirmDelete")}
                    icon={<X className="size-4" />}
                  />
                )}
              </div>
            )}
          </div>

          {/* Row 2: Description — full width, not compressed by actions */}
          {translation ? (
            <LocalizedSkillDescription {...translation} description={description} />
          ) : description ? (
            <p className="text-xs text-muted-foreground line-clamp-2 leading-relaxed">{description}</p>
          ) : null}

          {!isPlatformCard && <SkillGroupLinks filePath={translation?.filePath} />}

          {isPlatformCard && (
            <div className="space-y-2 pt-1">
              {(sharedControl || usageControl) && (
                <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg bg-muted/50 px-2.5 py-2 text-xs">
                  <div className="min-w-0">
                    <div className="font-medium text-foreground">
                      {sharedControl?.individual ? t("platform.allowOnPlatform", { platform: sharedControl.individual.platformDisplayName }) : t("platform.useOnPlatform", { platform: platformDisplayName ?? t("platform.thisPlatform") })}
                    </div>
                    <div className="text-muted-foreground">
                      {sharedControl?.individual
                        ? sharedControl.individual.enabled ? t("platform.allowedHere") : t("platform.blockedHere")
                        : sharedControl ? t("platform.individualUnavailable")
                        : usageControl?.state === "unsupported" || (usageControl?.state && !["active", "inactive", "deleted"].includes(usageControl.state))
                          ? t("skillUsage.unavailable")
                          : usageControl?.enabled ? t("skillUsage.active") : t("skillUsage.paused")}
                    </div>
                  </div>
                  {sharedControl?.individual ? (
                    <Switch checked={sharedControl.individual.enabled}
                      disabled={sharedControl.individual.isLoading || !sharedControl.individual.canToggle || Boolean(sharedControl.individual.disabledReason)}
                      onCheckedChange={sharedControl.individual.onToggle}
                      aria-label={t("skillUsage.toggleIndividualSkill", { name, platform: sharedControl.individual.platformDisplayName })} />
                  ) : usageControl && (
                    <Switch checked={usageControl.enabled}
                      disabled={usageControl.isLoading || Boolean(usageControl.disabledReason) || usageControl.state === "deleted"}
                      onCheckedChange={usageControl.onCheckedChange}
                      aria-label={t("skillUsage.toggleSkill", { name })} />
                  )}
                  {(sharedControl?.individual?.disabledReason || usageControl?.disabledReason) && (
                    <p className="basis-full text-amber-700 dark:text-amber-300">{sharedControl?.individual?.disabledReason || usageControl?.disabledReason}</p>
                  )}
                </div>
              )}
              {sharedControl && (
                <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border/60 pt-2 text-xs">
                  <div className="min-w-0">
                    <div className="font-medium text-foreground">{t("platform.sharedAcrossPlatforms")}</div>
                    <div className="text-muted-foreground">
                      {sharedControl.impact.enabled ? t("platform.sharedOn") : t("platform.sharedOff")}
                      {sharedControl.individual?.enabled && !sharedControl.impact.enabled && ` · ${t("platform.sharedOffHere")}`}
                    </div>
                  </div>
                  <div className="flex items-center gap-2">
                    {onManageUniversal && (
                      <button type="button" onClick={onManageUniversal}
                        className="rounded-sm text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                        aria-label={t("platform.manageUniversal")}>{t("platform.manageSharedShort")}</button>
                    )}
                    <Switch checked={sharedControl.impact.enabled}
                      disabled={sharedControl.isLoading || Boolean(sharedControl.impact.reason)}
                      onCheckedChange={sharedControl.onToggleShared}
                      aria-label={t("skillUsage.toggleSharedSkill", { name })} />
                  </div>
                  {sharedControl.impact.reason && (
                    <p className="basis-full text-amber-700 dark:text-amber-300">{sharedControl.impact.reason}</p>
                  )}
                </div>
              )}
              {!sharedControl && onManageUniversal && (
                <button type="button" onClick={onManageUniversal}
                  className="text-xs text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  aria-label={t("platform.manageUniversal")}>{t("platform.manageUniversal")}</button>
              )}
              {platformControlNotice && !usageControl?.disabledReason && (
                <p className="text-xs text-muted-foreground" title={platformControlNotice}>{platformControlNotice}</p>
              )}
              {!usageControl?.enabled && externalUsageCount > 0 && (
                <p className="text-xs text-amber-700 dark:text-amber-300">{t("skillUsage.externalStillAvailable", { count: externalUsageCount })}</p>
              )}
              {(origin || originKind || isExternallyManaged || isUniversalSource || isReadOnly || sourceType || translation?.filePath || sharedControl) && (
                <details className="text-xs text-muted-foreground">
                  <summary className="w-fit cursor-pointer rounded-sm hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">{t("platform.cardDetails")}</summary>
                  <div className="mt-2 flex flex-wrap items-center gap-2">
                    {origin && <GitHubOriginBadge origin={origin} installId={localSkillId} originalName={name} />}
                    {originKind && <SourceOriginBadge originKind={originKind} />}
                    {isExternallyManaged ? <ExternalManagedBadge /> : isUniversalSource ? <UniversalSourceBadge /> : isReadOnly ? <ReadOnlyBadge /> : null}
                    {sourceType && <SourceIndicator sourceType={sourceType} />}
                    {sharedControl && <span>{t("platform.confirmedPlatforms", { count: sharedControl.impact.confirmed_platforms.length })}</span>}
                    {sharedControl && sharedControl.impact.management_path && <span className="break-all">{formatPathForDisplay(sharedControl.impact.management_path)}</span>}
                    <SkillGroupLinks filePath={translation?.filePath} />
                  </div>
                </details>
              )}
            </div>
          )}
          {!isPlatformCard && (
          <div className="flex flex-wrap items-center gap-1.5 empty:hidden">
            {origin && (
              <GitHubOriginBadge origin={origin} installId={localSkillId} originalName={name} />
            )}
            {originKind && <SourceOriginBadge originKind={originKind} />}
            {isExternallyManaged
              ? <ExternalManagedBadge />
              : isUniversalSource
                ? <UniversalSourceBadge onClick={onManageUniversal} />
                : isReadOnly
                  ? <ReadOnlyBadge />
                  : null}

            {usageControl && (
              <span className="inline-flex items-center gap-1.5 rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground ring-1 ring-border/70">
                <span>
                  {usageControl.state === "unsupported" ||
                  (usageControl.state &&
                    !["active", "inactive", "deleted"].includes(usageControl.state))
                    ? t("skillUsage.unavailable")
                    : usageControl.enabled
                    ? t("skillUsage.active")
                    : t("skillUsage.paused")}
                </span>
                <Switch
                  checked={usageControl.enabled}
                  disabled={usageControl.isLoading || Boolean(usageControl.disabledReason) || usageControl.state === "deleted"}
                  onCheckedChange={usageControl.onCheckedChange}
                  aria-label={t("skillUsage.toggleSkill", { name })}
                  className="h-4 w-7 [&_[data-slot=switch-thumb]]:size-3 [&_[data-slot=switch-thumb]]:group-data-[checked]/switch:translate-x-3"
                />
              </span>
            )}

            {usageControl?.disabledReason && (
              <span className="text-[10px] text-amber-700 dark:text-amber-300" title={usageControl.disabledReason}>
                {usageControl.disabledReason}
              </span>
            )}

            {sharedControl && (
              <span
                className="inline-flex min-w-0 max-w-full items-center gap-1.5 rounded-full bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground ring-1 ring-border/70"
                onClick={(event) => event.stopPropagation()}
              >
                <span className="min-w-0 truncate" title={sharedControl.impact.skill_name}>
                  {sharedControl.impact.enabled
                    ? t("skillUsage.sharedActive")
                    : t("skillUsage.sharedPaused")}
                  {" · "}
                  {t("skillUsage.sharedConfirmed", {
                    count: sharedControl.impact.confirmed_platforms.length,
                  })}
                </span>
                <Switch
                  checked={sharedControl.impact.enabled}
                  disabled={sharedControl.isLoading || Boolean(sharedControl.impact.reason)}
                  onCheckedChange={() => sharedControl.onToggleShared()}
                  aria-label={t("skillUsage.toggleSharedSkill", { name })}
                  className="h-4 w-7 shrink-0 [&_[data-slot=switch-thumb]]:size-3 [&_[data-slot=switch-thumb]]:group-data-[checked]/switch:translate-x-3"
                />
              </span>
            )}

            {sharedControl?.impact.reason && (
              <span
                className="min-w-0 break-words text-[10px] text-amber-700 dark:text-amber-300"
                title={`${sharedControl.impact.management_path} · ${sharedControl.impact.reason}`}
                onClick={(event) => event.stopPropagation()}
              >
                {t("sharedImpact.managementPath", {
                  path: formatPathForDisplay(sharedControl.impact.management_path),
                })}{" · "}
                {t("sharedImpact.restricted", { reason: sharedControl.impact.reason })}
              </span>
            )}

            {sharedControl && sharedControl.excludedHere && (
              <span className="inline-flex shrink-0 items-center gap-1 rounded-full bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium text-amber-700 ring-1 ring-amber-500/40 dark:text-amber-300">
                {t("skillUsage.excludedHere")}
              </span>
            )}

            {sharedControl?.individual && (
              <details
                className="min-w-0 text-[10px] text-muted-foreground"
                onClick={(event) => event.stopPropagation()}
              >
                <summary className="cursor-pointer rounded px-1 py-0.5 hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring">
                  {t("skillUsage.individualMenu")}
                </summary>
                <div className="mt-1 flex min-w-0 items-center gap-1.5">
                  <Switch
                    checked={sharedControl.individual.enabled}
                    disabled={
                      sharedControl.individual.isLoading ||
                      !sharedControl.individual.canToggle ||
                      Boolean(sharedControl.individual.disabledReason)
                    }
                    onCheckedChange={sharedControl.individual.onToggle}
                    aria-label={t("skillUsage.toggleIndividualSkill", {
                      name,
                      platform: sharedControl.individual.platformDisplayName,
                    })}
                    className="h-4 w-7 shrink-0 [&_[data-slot=switch-thumb]]:size-3 [&_[data-slot=switch-thumb]]:group-data-[checked]/switch:translate-x-3"
                  />
                  <span className="min-w-0 truncate">
                    {sharedControl.individual.enabled
                      ? t("skillUsage.active")
                      : t("skillUsage.paused")}
                  </span>
                </div>
                {sharedControl.individual.disabledReason && (
                  <span
                    className="mt-0.5 block min-w-0 break-words text-amber-700 dark:text-amber-300"
                    title={sharedControl.individual.disabledReason}
                  >
                    {sharedControl.individual.disabledReason}
                  </span>
                )}
              </details>
            )}

            {platformControlNotice && !usageControl?.disabledReason && (
              <span className="text-[10px] text-muted-foreground" title={platformControlNotice}>
                {platformControlNotice}
              </span>
            )}

            {!usageControl?.enabled && externalUsageCount > 0 && (
              <span className="text-[10px] text-amber-700 dark:text-amber-300">
                {t("skillUsage.externalStillAvailable", { count: externalUsageCount })}
              </span>
            )}

            {/* Source indicator (platform) */}
            {sourceType && <SourceIndicator sourceType={sourceType} />}

            {/* "Already in Central" badge */}
            {isCentral && (
              <span className="inline-flex items-center gap-1 text-xs text-muted-foreground bg-muted/50 px-1.5 py-0.5 rounded">
                <Globe className="size-3" />
                {t("discover.alreadyCentral")}
              </span>
            )}

            {/* Platform badge (discover) */}
            {platformBadge && (
              <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
                <PlatformIcon agentId={platformBadge.id} className="size-3" />
                {platformBadge.name}
              </span>
            )}

            {/* Project badge (discover) */}
            {projectBadge && (
              <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
                <Folder className="size-3" />
                {projectBadge}
              </span>
            )}

            {/* Publisher (marketplace recommended) */}
            {publisher && (
              <span className="text-[10px] text-muted-foreground truncate">{publisher}</span>
            )}

            {/* Tags (marketplace recommended) */}
            {tags && tags.length > 0 && (
              <div className="flex items-center gap-1">
                {tags.slice(0, 2).map((tag) => (
                  <span key={tag.key} className="text-[10px] bg-muted/60 text-muted-foreground px-1.5 py-0.5 rounded">
                    {tag.label}
                  </span>
                ))}
              </div>
            )}
          </div>

          )}

          {/* Row 3: Platform toggles (central) */}
          {hasPlatformIcons && (lobsterAgents.length > 0 || codingAgents.length > 0) && (
            <div className="skill-card-platforms mt-auto space-y-1 pt-1">
              {lobsterAgents.length > 0 && (
                <div className="flex items-center gap-1.5">
                  <span className="w-14 shrink-0 text-[10px] font-medium uppercase tracking-wider text-muted-foreground/70">
                    {t("sidebar.categoryLobster")}
                  </span>
                  <div className="flex min-w-0 flex-1 items-center gap-0.5 overflow-hidden">
                    {lobsterAgents.map((agent) => {
                      const usage = usageByAgent[agent.id];
                      const isManaged = Boolean(usage) || linkedAgentIds.has(agent.id);
                      const isReadOnlyAgent = readOnlyAgentIds.has(agent.id) && !isManaged;
                      return (
                        <PlatformToggleIcon
                          key={agent.id}
                          agent={agent}
                          skillName={name}
                          isLinked={(usage?.enabled ?? linkedAgentIds.has(agent.id)) || isReadOnlyAgent}
                          isReadOnly={isReadOnlyAgent}
                          isToggling={platformIcons.togglingAgentId === agent.id}
                          onToggle={() => platformIcons.onToggle(platformIcons.skillId, agent.id)}
                        />
                      );
                    })}
                  </div>
                </div>
              )}
              {codingAgents.length > 0 && (
                <div className="flex items-center gap-1.5">
                  <span className="w-14 shrink-0 text-[10px] font-medium uppercase tracking-wider text-muted-foreground/70">
                    {t("sidebar.categoryCoding")}
                  </span>
                  <div className="flex min-w-0 flex-1 items-center gap-0.5 overflow-hidden">
                    {featuredCodingAgents.map((agent) => {
                      const usage = usageByAgent[agent.id];
                      const isManaged = Boolean(usage) || linkedAgentIds.has(agent.id);
                      const isReadOnlyAgent = readOnlyAgentIds.has(agent.id) && !isManaged;
                      return (
                        <PlatformToggleIcon
                          key={agent.id}
                          agent={agent}
                          skillName={name}
                          isLinked={(usage?.enabled ?? linkedAgentIds.has(agent.id)) || isReadOnlyAgent}
                          isReadOnly={isReadOnlyAgent}
                          isToggling={platformIcons.togglingAgentId === agent.id}
                          onToggle={() => platformIcons.onToggle(platformIcons.skillId, agent.id)}
                        />
                      );
                    })}
                    {hiddenCodingCount > 0 && (
                      <span className="ml-0.5 rounded-md bg-muted/60 px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                        +{hiddenCodingCount}
                      </span>
                    )}
                  </div>
                  {platformIcons.onManage && (
                    <button
                      type="button"
                      onClick={platformIcons.onManage}
                      className="shrink-0 rounded-md px-2 py-1 text-xs font-medium text-primary transition-colors hover:bg-primary/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                      aria-label={t("central.managePlatformsLabel", { skill: name })}
                    >
                      {t("central.managePlatforms")}
                    </button>
                  )}
                </div>
              )}
              {codingAgents.length === 0 && platformIcons.onManage && (
                <div className="flex justify-end">
                  <button
                    type="button"
                    onClick={platformIcons.onManage}
                    className="shrink-0 rounded-md px-2 py-1 text-xs font-medium text-primary transition-colors hover:bg-primary/10 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                    aria-label={t("central.managePlatformsLabel", { skill: name })}
                  >
                    {t("central.managePlatforms")}
                  </button>
                </div>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ─── Source Indicator (internal) ──────────────────────────────────────────────

function SourceIndicator({ sourceType }: { sourceType: string }) {
  const { t, i18n } = useTranslation();
  const isSymlink = sourceType === "symlink";
  const isNative = sourceType === "native";
  const primaryLabel = isSymlink ? t("platform.sourceCentral") : t("platform.sourceStandalone");
  const secondaryLabel = isSymlink
    ? t("platform.sourceSymlinkLabel")
    : isNative
      ? t("platform.sourceNativeLabel", {
          defaultValue: i18n.language.startsWith("zh") ? "原生" : "native",
        })
      : t("platform.sourceCopyLabel");

  return (
    <div
      className={cn(
        "inline-flex items-center gap-1 text-xs font-medium",
        isSymlink ? "text-primary/80" : "text-muted-foreground"
      )}
    >
      {isSymlink ? <Link2 className="size-3 shrink-0" /> : <FolderOpen className="size-3 shrink-0" />}
      <div className="inline-flex items-center gap-1">
        <span>{primaryLabel}</span>
        <span aria-hidden="true" className="h-px w-3 shrink-0 rounded-full bg-current opacity-40" />
        <span className="sr-only"> - </span>
        <span>{secondaryLabel}</span>
      </div>
    </div>
  );
}

function SourceOriginBadge({ originKind }: { originKind: ClaudeSourceKind }) {
  const { t, i18n } = useTranslation();
  const isPlugin = originKind === "plugin";
  const isCompatibility = originKind === "compatibility";

  return (
    <span
      className={cn(
        "inline-flex items-center rounded-full px-2 py-0.5 text-[10px] font-medium ring-1",
        isPlugin
          ? "bg-amber-500/10 text-amber-700 ring-amber-500/20 dark:text-amber-300"
          : isCompatibility
            ? "bg-violet-500/10 text-violet-700 ring-violet-500/20 dark:text-violet-300"
          : "bg-sky-500/10 text-sky-700 ring-sky-500/20 dark:text-sky-300"
      )}
    >
      {isPlugin
        ? t("platform.originPlugin", {
            defaultValue: i18n.language.startsWith("zh") ? "插件来源" : "Plugin source",
          })
        : isCompatibility
          ? t("platform.originCompatibility", {
              defaultValue: i18n.language.startsWith("zh") ? "兼容来源" : "Compatibility source",
            })
        : t("platform.originUser", {
            defaultValue: i18n.language.startsWith("zh") ? "用户来源" : "User source",
          })}
    </span>
  );
}

function ReadOnlyBadge() {
  const { t, i18n } = useTranslation();

  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium text-muted-foreground ring-1 ring-border/70">
      <Lock className="size-3 shrink-0" />
      {t("platform.readOnly", {
        defaultValue: i18n.language.startsWith("zh") ? "只读" : "Read-only",
      })}
    </span>
  );
}

function UniversalSourceBadge({ onClick }: { onClick?: () => void }) {
  const { t } = useTranslation();
  const className = "inline-flex items-center gap-1 rounded-full bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary ring-1 ring-primary/20";

  if (onClick) {
    return (
      <button type="button" className={className} onClick={onClick}>
        <Globe className="size-3 shrink-0" />
        {t("platform.universalSource")}
      </button>
    );
  }

  return (
    <span className={className}>
      <Globe className="size-3 shrink-0" />
      {t("platform.universalSource")}
    </span>
  );
}

function ExternalManagedBadge() {
  const { t } = useTranslation();
  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium text-muted-foreground ring-1 ring-border/70">
      <Lock className="size-3 shrink-0" />
      {t("universal.externallyManaged")}
    </span>
  );
}

// ─── GitHub Origin Badge (internal) ───────────────────────────────────────────

function GitHubOriginBadge({
  origin,
  installId,
  originalName,
}: {
  origin: GitHubSkillOriginSummary;
  installId?: string;
  originalName: string;
}) {
  const { t } = useTranslation();
  const url = githubSkillSourceUrl(origin);
  return (
    <>
      <GitHubSourceLink
        href={url}
        target="_blank"
        rel="noreferrer"
        title={url}
        aria-label={t("skillOrigin.viewSource", { repo: `${origin.owner}/${origin.repo}` })}
        className="inline-flex max-w-full items-center gap-1 rounded-full bg-sky-500/10 px-2 py-0.5 text-[10px] font-medium text-sky-700 ring-1 ring-sky-500/20 dark:text-sky-300 hover:underline"
      >
        <Link2 className="size-3 shrink-0" />
        <span className="min-w-0 truncate">{origin.owner}/{origin.repo}</span>
      </GitHubSourceLink>
      {origin.updateAvailable && (
        <span className="inline-flex items-center gap-1 rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] font-medium text-amber-700 ring-1 ring-amber-500/20 dark:text-amber-300">
          <RotateCcw className="size-3" />
          {t("skillOrigin.updateAvailable")}
        </span>
      )}
      {installId !== undefined && installId !== originalName && (
        <span
          className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-[10px] font-medium text-muted-foreground ring-1 ring-border/70"
          title={t("skillOrigin.installedAsTitle", { id: installId })}
        >
          {t("skillOrigin.installedAs", { id: installId })}
        </span>
      )}
    </>
  );
}

import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Search, Blocks, FolderOpen, Loader2, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import { usePlatformStore } from "@/stores/platformStore";
import { useSkillStore } from "@/stores/skillStore";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { isSkillUsageBusyError, useSkillUsageStore } from "@/stores/skillUsageStore";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
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
import { SkillTransferToolbar } from "@/components/skill/SkillTransferToolbar";
import { canTransferSkill, skillSelectionKey, useSkillSelection } from "@/hooks/useSkillSelection";
import { UnifiedSkillCard } from "@/components/skill/UnifiedSkillCard";
import { SkillDetailDrawer } from "@/components/skill/SkillDetailDrawer";
import {
  SkillFolderDrawer,
  type SkillFolderDrawerSkill,
} from "@/components/skill/SkillFolderDrawer";
import { SkillFolderCard } from "@/components/skill/SkillFolderCard";
import { SkillListModeToggle } from "@/components/skill/SkillListModeToggle";
import { PlatformIcon } from "@/components/platform/PlatformIcon";
import { InstallDialog } from "@/components/central/InstallDialog";
import { CollectionInstallDialog } from "@/components/collection/CollectionInstallDialog";
import { useSkillListViewMode } from "@/hooks/useSkillListViewMode";
import { formatPathForDisplay } from "@/lib/path";
import { splitSkillsByTopLevel } from "@/lib/skillFolders";
import { cn } from "@/lib/utils";
import { ScannedSkill, SkillWithLinks } from "@/types";

// ─── Empty State ──────────────────────────────────────────────────────────────

function EmptyState({ message }: { message: string }) {
  return (
    <div className="flex flex-col items-center justify-center h-full gap-4 py-20">
      <div className="p-4 rounded-full bg-muted/60">
        <Blocks className="size-12 text-muted-foreground opacity-60" />
      </div>
      <p className="text-sm text-muted-foreground font-medium">{message}</p>
    </div>
  );
}

type ClaudeSourceFilter = "all" | "user" | "plugin";
type InstallSourceFilter = "all" | "platform" | "universal";

interface PluginBundleTarget {
  sourceAgentId: string;
  sourceLabel: string;
  name: string;
  skillCount: number;
}

function isUniversalSource(skill: ScannedSkill): boolean {
  return Boolean(skill.is_read_only && skill.source_kind === "compatibility");
}

// ─── PlatformView ─────────────────────────────────────────────────────────────

export function PlatformView() {
  const { agentId } = useParams<{ agentId: string }>();
  const navigate = useNavigate();
  const { t, i18n } = useTranslation();
  const agents = usePlatformStore((state) => state.agents);
  const scanGeneration = usePlatformStore((state) => state.scanGeneration ?? 0);

  const skillsByAgent = useSkillStore((state) => state.skillsByAgent);
  const loadingByAgent = useSkillStore((state) => state.loadingByAgent);
  const pendingSkillActionKeys = useSkillStore((state) => state.pendingSkillActionKeys);
  const getSkillsByAgent = useSkillStore((state) => state.getSkillsByAgent);

  const centralSkills = useCentralSkillsStore((state) => state.skills);
  const centralAgents = useCentralSkillsStore((state) => state.agents);
  const loadInstallTarget = useCentralSkillsStore((state) => state.loadInstallTarget);
  const loadCentralSkills = useCentralSkillsStore((state) => state.loadCentralSkills);
  const installSkill = useCentralSkillsStore((state) => state.installSkill);
  const installPluginBundle = useCentralSkillsStore(
    (state) => state.installPluginBundle
  );
  const refreshCounts = usePlatformStore((state) => state.refreshCounts);
  const usageStatuses = useSkillUsageStore((state) => state.statuses);
  const usageUpdatingSkillKeys = useSkillUsageStore((state) => state.updatingSkillKeys);
  const usageUpdatingAgentIds = useSkillUsageStore((state) => state.updatingAgentIds);
  const setSkillUsage = useSkillUsageStore((state) => state.setSkillUsage);
  const setPlatformUsage = useSkillUsageStore((state) => state.setPlatformUsage);
  const deleteSkillFromAgent = useSkillUsageStore((state) => state.deleteSkillFromAgent);
  const deletePlatformInstallations = useSkillUsageStore(
    (state) => state.deletePlatformInstallations
  );
  const loadUsageStatus = useSkillUsageStore((state) => state.loadUsageStatus);

  const [searchQuery, setSearchQuery] = useState("");
  const [sourceFilter, setSourceFilter] = useState<ClaudeSourceFilter>("all");
  const [installSourceFilter, setInstallSourceFilter] = useState<InstallSourceFilter>("all");
  const [viewMode, setViewMode] = useSkillListViewMode("platform");
  const [installTargetSkill, setInstallTargetSkill] = useState<SkillWithLinks | null>(null);
  const [isDialogOpen, setIsDialogOpen] = useState(false);
  const [drawerSkill, setDrawerSkill] = useState<ScannedSkill | null>(null);
  const [isDrawerOpen, setIsDrawerOpen] = useState(false);
  const [folderDrawerGroupPath, setFolderDrawerGroupPath] = useState<string | null>(null);
  const [isFolderDrawerOpen, setIsFolderDrawerOpen] = useState(false);
  const [pluginBundleTarget, setPluginBundleTarget] =
    useState<PluginBundleTarget | null>(null);
  const [isPluginBundleDialogOpen, setIsPluginBundleDialogOpen] = useState(false);
  const [returnFocusRowKey, setReturnFocusRowKey] = useState<string | null>(null);
  const [isPlatformDeleteDialogOpen, setIsPlatformDeleteDialogOpen] = useState(false);
  const contentRef = useRef<HTMLDivElement | null>(null);
  const detailButtonRefs = useRef<Record<string, HTMLButtonElement | null>>({});

  function getSkillRowKey(skill: ScannedSkill) {
    return skill.row_id ?? skill.id;
  }

  const agent = agents.find((a) => a.id === agentId);
  const isClaudePage = agent?.id === "claude-code";

  // Load skills for this agent when the route changes or a fresh scan completes.
  useEffect(() => {
    if (agentId) {
      getSkillsByAgent(agentId);
      void loadUsageStatus().catch(() => undefined);
    }
  }, [agentId, getSkillsByAgent, loadUsageStatus, scanGeneration]);

  useEffect(() => {
    if (!contentRef.current) return;
    contentRef.current.scrollTop = 0;
  }, [agentId]);

  useEffect(() => {
    setSourceFilter("all");
    setInstallSourceFilter("all");
  }, [agentId]);

  // Ensure central skills are loaded so we can resolve SkillWithLinks for InstallDialog.
  useEffect(() => {
    if (centralSkills.length === 0) {
      loadCentralSkills();
    }
  }, [centralSkills.length, loadCentralSkills]);

  async function handleInstallClick(skillId: string) {
    try {
      const target = centralSkills.find((skill) => skill.id === skillId)
        ?? await loadInstallTarget(skillId);
      setInstallTargetSkill(target);
      setIsDialogOpen(true);
    } catch (err) {
      toast.error(t("central.installError", { error: String(err) }));
    }
  }

  async function handleInstall(skillId: string, agentIds: string[], method: string) {
    try {
      const result = await installSkill(skillId, agentIds, method);
      await refreshCounts();
      await loadCentralSkills();
      if (agentId) {
        await getSkillsByAgent(agentId);
      }
      if (result.failed.length > 0) {
        const failedNames = result.failed.map((f) => f.agent_id).join(", ");
        toast.error(t("central.installPartialFail", { platforms: failedNames }));
      }
    } catch (err) {
      toast.error(t("central.installError", { error: String(err) }));
    }
  }

  async function handleUsageChange(skillId: string, enabled: boolean) {
    if (!agentId) return;
    try {
      await setSkillUsage(skillId, agentId, enabled);
      await Promise.all([refreshCounts(), getSkillsByAgent(agentId)]);
    } catch (err) {
      toast.error(t("skillUsage.updateError", { error: String(err) }));
    }
  }

  async function handleDeleteSkill(skillId: string) {
    if (!agentId) return;
    try {
      await deleteSkillFromAgent(skillId, agentId);
      await Promise.all([refreshCounts(), getSkillsByAgent(agentId)]);
      toast.success(
        t("skillUsage.deleteSkillSuccess", {
          name: skillId,
          platform: agent?.display_name ?? agentId,
        })
      );
    } catch (error) {
      if (isSkillUsageBusyError(error)) return;
      toast.error(t("skillUsage.deleteError", { error: String(error) }));
    }
  }

  const isLoading = agentId ? (loadingByAgent[agentId] ?? false) : false;

  // Memoize skills to avoid changing dependency reference on every render
  const skills = useMemo(
    () => (agentId ? (skillsByAgent[agentId] ?? []) : []),
    [agentId, skillsByAgent]
  );
  const usageStatus = usageStatuses.find((status) => status.agent_id === agentId);
  const usageBySkillId = useMemo(
    () => new Map((usageStatus?.skills ?? []).map((usage) => [usage.skill_id, usage])),
    [usageStatus?.skills]
  );
  const hasBulkPausedSkills = (usageStatus?.skills ?? []).some((usage) => usage.paused_by_bulk);
  const canPausePlatform = (usageStatus?.active_count ?? 0) > 0;
  const canRestorePlatform = !canPausePlatform && hasBulkPausedSkills;
  const isPlatformUsageUpdating = agentId
    ? (usageUpdatingAgentIds[agentId] ?? false) ||
      Object.keys(usageUpdatingSkillKeys).some((key) => key.startsWith(`${agentId}::`))
    : false;
  const platformUsageState = canPausePlatform
    ? (usageStatus?.paused_count ?? 0) > 0
      ? t("skillUsage.platformMixed")
      : t("skillUsage.platformActive")
    : t("skillUsage.platformPaused");

  async function handlePlatformUsageChange() {
    if (!agentId || (!canPausePlatform && !canRestorePlatform)) return;
    try {
      await setPlatformUsage(agentId, !canPausePlatform);
      await Promise.all([refreshCounts(), getSkillsByAgent(agentId)]);
    } catch (error) {
      toast.error(t("skillUsage.updateError", { error: String(error) }));
    }
  }

  const managedInstallCount = usageStatus?.skills.length ?? 0;
  const canDeletePlatform = managedInstallCount > 0;

  async function handlePlatformDelete() {
    if (!agentId || !canDeletePlatform) return;
    try {
      const result = await deletePlatformInstallations(agentId);
      await Promise.all([refreshCounts(), getSkillsByAgent(agentId)]);
      setIsPlatformDeleteDialogOpen(false);

      const externalCount = usageStatus?.external_count ?? 0;
      const failed = result.failed.length;
      const messageKey = failed > 0
        ? externalCount > 0
          ? "skillUsage.deletePlatformPartialExternal"
          : "skillUsage.deletePlatformPartial"
        : externalCount > 0
          ? "skillUsage.deletePlatformSuccessExternal"
          : "skillUsage.deletePlatformSuccess";
      toast[failed > 0 ? "error" : "success"](
        t(messageKey, {
          name: agent?.display_name ?? agentId,
          count: result.deleted.length,
          deleted: result.deleted.length,
          failed,
          external: externalCount,
        })
      );
    } catch (error) {
      if (isSkillUsageBusyError(error)) return;
      toast.error(t("skillUsage.deleteError", { error: String(error) }));
    }
  }
  const managedSkills = useMemo(() => {
    const activeSkillIds = new Set(
      skills.filter((skill) => !skill.is_read_only).map((skill) => skill.id)
    );
    const pausedSkills: ScannedSkill[] = (usageStatus?.skills ?? [])
      .filter((usage) => !usage.enabled && !activeSkillIds.has(usage.skill_id))
      .map((usage) => ({
        id: usage.skill_id,
        row_id: `${agentId}::paused::${usage.skill_id}`,
        name: usage.name,
        file_path: "",
        dir_path: agent?.global_skills_dir ?? "",
        link_type: "symlink",
        is_central: true,
      }));
    return [...skills, ...pausedSkills];
  }, [agent?.global_skills_dir, agentId, skills, usageStatus?.skills]);

  const sourceFilteredSkills = useMemo(() => {
    const claudeFiltered = !isClaudePage || sourceFilter === "all"
      ? managedSkills
      : managedSkills.filter((skill) => skill.source_kind === sourceFilter);

    if (installSourceFilter === "universal") {
      return claudeFiltered.filter(isUniversalSource);
    }
    if (installSourceFilter === "platform") {
      return claudeFiltered.filter((skill) => !isUniversalSource(skill));
    }
    return claudeFiltered;
  }, [installSourceFilter, isClaudePage, managedSkills, sourceFilter]);

  const universalCount = useMemo(
    () => skills.filter(isUniversalSource).length,
    [skills]
  );

  const platformFolderSplit = useMemo(
    () =>
      splitSkillsByTopLevel({
        skills: sourceFilteredSkills,
        rootPath: agent?.global_skills_dir ?? "",
        getDirPaths: (skill) => skill.dir_path,
        getTopLevelGroup: (skill) =>
          skill.source_kind === "plugin" && skill.source_label && skill.source_root
            ? {
                name: skill.source_label,
                relativePath: `plugin:${skill.source_label}`,
                path: skill.source_root,
              }
            : null,
      }),
    [agent?.global_skills_dir, sourceFilteredSkills]
  );
  const platformFolderGroupsByPath = useMemo(
    () =>
      new Map(
        platformFolderSplit.groups.map((group) => [
          group.relativePath,
          group,
        ])
      ),
    [platformFolderSplit.groups]
  );
  const visibleSkills =
    viewMode === "folders" ? platformFolderSplit.rootSkills : sourceFilteredSkills;

  const sourceCounts = useMemo(() => {
    const counts: Record<ClaudeSourceFilter, number> = {
      all: skills.length,
      user: 0,
      plugin: 0,
    };

    for (const skill of skills) {
      if (skill.source_kind === "user") {
        counts.user += 1;
      } else if (skill.source_kind === "plugin") {
        counts.plugin += 1;
      }
    }

    return counts;
  }, [skills]);

  // Filter skills by search query using useMemo
  const filteredSkills = useMemo(() => {
    if (!searchQuery.trim()) return visibleSkills;
    const q = searchQuery.toLowerCase();
    return visibleSkills.filter(
      (skill) =>
        skill.id.toLowerCase().includes(q) ||
        skill.name.toLowerCase().includes(q) ||
        skill.description?.toLowerCase().includes(q)
    );
  }, [visibleSkills, searchQuery]);

  const transferSelection = useSkillSelection(filteredSkills, agentId);

  const filteredFolderGroups = useMemo(() => {
    if (viewMode !== "folders") return [];
    if (!searchQuery.trim()) return platformFolderSplit.groups;
    const q = searchQuery.toLowerCase();
    return platformFolderSplit.groups.filter(
      (group) =>
        group.name.toLowerCase().includes(q) ||
        group.path.toLowerCase().includes(q) ||
        group.skills.some(
          (skill) =>
            skill.id.toLowerCase().includes(q) ||
            skill.name.toLowerCase().includes(q) ||
            skill.description?.toLowerCase().includes(q)
        )
    );
  }, [platformFolderSplit.groups, searchQuery, viewMode]);

  useEffect(() => {
    if (!drawerSkill) return;

    const rowKey = getSkillRowKey(drawerSkill);
    const refreshedSkill = skills.find((skill) => getSkillRowKey(skill) === rowKey);

    if (!refreshedSkill) {
      setIsDrawerOpen(false);
      setDrawerSkill(null);
      return;
    }

    if (refreshedSkill !== drawerSkill) {
      setDrawerSkill(refreshedSkill);
    }
  }, [drawerSkill, skills]);

  function setDetailButtonRef(rowKey: string, node: HTMLButtonElement | null) {
    if (node) {
      detailButtonRefs.current[rowKey] = node;
      return;
    }
    delete detailButtonRefs.current[rowKey];
  }

  function handleOpenDrawer(skill: ScannedSkill) {
    setReturnFocusRowKey(getSkillRowKey(skill));
    setDrawerSkill(skill);
    setIsDrawerOpen(true);
  }

  function handleOpenFolderDrawer(relativePath: string) {
    setFolderDrawerGroupPath(relativePath);
    setIsFolderDrawerOpen(true);
  }

  const folderDrawerGroup = folderDrawerGroupPath
    ? platformFolderGroupsByPath.get(folderDrawerGroupPath)
    : null;
  const folderDrawerPluginLabel =
    folderDrawerGroup?.skills.length &&
    folderDrawerGroup.skills.every(
      (skill) =>
        skill.source_kind === "plugin" &&
        skill.source_label === folderDrawerGroup.skills[0].source_label
    )
      ? folderDrawerGroup.skills[0].source_label
      : null;
  const folderDrawerSkills = useMemo<SkillFolderDrawerSkill[]>(
    () =>
      (folderDrawerGroup?.skills ?? []).map((skill) => ({
        key: getSkillRowKey(skill),
        id: skill.id,
        name: skill.name,
        description: skill.description,
        path: skill.dir_path,
        relativePath: skill.dir_path.replace(`${folderDrawerGroup?.path ?? ""}/`, ""),
        agentId,
        rowId: skill.row_id ?? null,
        sourceLabel:
          skill.source_kind === "user"
            ? t("platform.originUser")
            : skill.source_kind === "plugin"
              ? t("platform.originPlugin")
              : isUniversalSource(skill)
                ? t("platform.universalSource")
                : skill.link_type,
        isReadOnly: skill.is_read_only ?? false,
        sourceKind: skill.source_kind,
      })),
    [agentId, folderDrawerGroup, t]
  );

  function handleInstallPluginBundleClick() {
    if (!agentId || !folderDrawerGroup || !folderDrawerPluginLabel) return;
    setPluginBundleTarget({
      sourceAgentId: agentId,
      sourceLabel: folderDrawerPluginLabel,
      name: folderDrawerGroup.name,
      skillCount: folderDrawerGroup.skillCount,
    });
    setIsFolderDrawerOpen(false);
    setFolderDrawerGroupPath(null);
    setIsPluginBundleDialogOpen(true);
  }

  async function handleInstallPluginBundle(agentIds: string[]) {
    if (!pluginBundleTarget) {
      throw new Error(t("platform.notFound"));
    }
    const result = await installPluginBundle(
      pluginBundleTarget.sourceAgentId,
      pluginBundleTarget.sourceLabel,
      agentIds
    );
    await Promise.all([
      refreshCounts(),
      agentId ? getSkillsByAgent(agentId) : Promise.resolve(),
    ]);
    return result;
  }

  if (!agent) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        {t("platform.notFound")}
      </div>
    );
  }

  const sourceTabs: { id: ClaudeSourceFilter; label: string; count: number }[] = [
    {
      id: "all",
      label: t("platform.sourceFilter.all", {
        defaultValue: i18n.language.startsWith("zh") ? "全部" : "All",
      }),
      count: sourceCounts.all,
    },
    {
      id: "user",
      label: t("platform.sourceFilter.user", {
        defaultValue: i18n.language.startsWith("zh") ? "用户来源" : "User source",
      }),
      count: sourceCounts.user,
    },
    {
      id: "plugin",
      label: t("platform.sourceFilter.plugin", {
        defaultValue: i18n.language.startsWith("zh") ? "插件来源" : "Plugin source",
      }),
      count: sourceCounts.plugin,
    },
  ];
  const activeSourceLabel = sourceTabs.find((tab) => tab.id === sourceFilter)?.label ?? sourceTabs[0].label;
  const activeInstallSourceLabel = t(`platform.installSourceFilter.${installSourceFilter}`);
  const platformDeleteButton = (
    <Button
      type="button"
      variant="destructive"
      size="sm"
      disabled={!canDeletePlatform || isPlatformUsageUpdating}
      onClick={() => setIsPlatformDeleteDialogOpen(true)}
      aria-label={t("skillUsage.deletePlatformAria", { name: agent.display_name })}
    >
      <Trash2 className="size-3.5" />
      {t("skillUsage.deletePlatform")}
    </Button>
  );

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="border-b border-border px-6 py-4">
        <div className="flex items-center gap-2.5">
          <PlatformIcon agentId={agent.id} className="size-6 text-primary/70" size={24} />
          <h1 className="text-xl font-semibold">{agent.display_name}</h1>
        </div>
        <p className="text-sm text-muted-foreground mt-0.5">
          {formatPathForDisplay(agent.global_skills_dir)}
        </p>
        {usageStatus ? (
          <div className="mt-3 flex flex-wrap items-center gap-2">
            {usageStatus.skills.length > 0 ? (
              <>
                <p className="text-xs text-muted-foreground">
                  {t("skillUsage.managedSummary", {
                    active: usageStatus.active_count,
                    paused: usageStatus.paused_count,
                  })}
                </p>
                <div className="flex items-center gap-2">
                  <Switch
                    checked={canPausePlatform}
                    disabled={
                      isPlatformUsageUpdating ||
                      (!canPausePlatform && !canRestorePlatform)
                    }
                    onCheckedChange={() => void handlePlatformUsageChange()}
                    aria-label={t("skillUsage.togglePlatform", { name: agent.display_name })}
                  />
                  <span className="text-xs text-muted-foreground">
                    {t("skillUsage.platformSwitchLabel")}: {platformUsageState}
                  </span>
                  {platformDeleteButton}
                </div>
                {!canPausePlatform && (
                  <p className="basis-full text-xs text-muted-foreground">
                    {canRestorePlatform
                      ? t("skillUsage.restoreBulkHint")
                      : t("skillUsage.individualPaused")}
                  </p>
                )}
              </>
            ) : (
              <>
                <p className="text-xs text-muted-foreground">{t("skillUsage.noManaged")}</p>
                {platformDeleteButton}
              </>
            )}
            {usageStatus.external_count > 0 && (
              <p className="text-xs text-amber-700 dark:text-amber-300">
                {t("skillUsage.externalHint", { count: usageStatus.external_count })}
              </p>
            )}
            {usageStatus.skills.length > 0 && (
              <p className="basis-full text-xs text-muted-foreground">
                {t("skillUsage.reloadHint")}
              </p>
            )}
          </div>
        ) : null}
      </div>

      {isClaudePage && (
        <div
          role="tablist"
          aria-label={t("platform.sourceFilterTabsLabel", {
            defaultValue: i18n.language.startsWith("zh") ? "Claude 来源筛选" : "Claude source filters",
          })}
          className="flex items-center gap-1 px-6 py-3 border-b border-border"
        >
          {sourceTabs.map((tab) => (
            <button
              key={tab.id}
              type="button"
              role="tab"
              aria-selected={sourceFilter === tab.id}
              onClick={() => setSourceFilter(tab.id)}
              className={cn(
                "inline-flex items-center gap-1.5 px-4 py-1.5 rounded-md text-sm transition-colors cursor-pointer",
                sourceFilter === tab.id
                  ? "bg-primary/15 text-foreground font-medium"
                  : "text-muted-foreground hover:bg-muted/40"
              )}
            >
              <span>{tab.label}</span>
              <span className="text-xs opacity-75">({tab.count})</span>
            </button>
          ))}
        </div>
      )}

      {universalCount > 0 && (
        <div
          role="tablist"
          aria-label={t("platform.installSourceFilterLabel")}
          className="flex items-center gap-1 border-b border-border px-6 py-3"
        >
          {(["all", "platform", "universal"] as const).map((filter) => {
            const count = filter === "all"
              ? skills.length
              : filter === "universal"
                ? universalCount
                : skills.length - universalCount;
            return (
              <button
                key={filter}
                type="button"
                role="tab"
                aria-selected={installSourceFilter === filter}
                onClick={() => setInstallSourceFilter(filter)}
                className={cn(
                  "inline-flex items-center gap-1.5 rounded-md px-4 py-1.5 text-sm transition-colors",
                  installSourceFilter === filter
                    ? "bg-primary/15 font-medium text-foreground"
                    : "text-muted-foreground hover:bg-muted/40"
                )}
              >
                {t(`platform.installSourceFilter.${filter}`)}
                <span className="text-xs opacity-75">({count})</span>
              </button>
            );
          })}
        </div>
      )}

      {/* Search bar */}
      <div className="px-6 py-3 border-b border-border">
        <div className="flex flex-col gap-3 lg:flex-row lg:items-center">
          <div className="relative flex-1">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 size-4 text-muted-foreground pointer-events-none" />
            <Input
              placeholder={t("platform.searchPlaceholder")}
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              className="pl-8 bg-muted/40"
            />
          </div>
          <SkillListModeToggle value={viewMode} onChange={setViewMode} />
        </div>
      </div>

      <SkillTransferToolbar selection={transferSelection} agents={agents} sourceAgentId={agentId} disabled={isLoading} />

      {/* Content */}
      <div ref={contentRef} className="flex-1 overflow-auto p-6">
        {isLoading ? (
          <EmptyState message={t("platform.loading")} />
        ) : managedSkills.length === 0 ? (
          <EmptyState
            message={t("platform.noSkills", { name: agent.display_name })}
          />
        ) : sourceFilteredSkills.length === 0 ? (
          <EmptyState
            message={t("platform.noSourceSkills", {
              name: agent.display_name,
              source: installSourceFilter === "all" ? activeSourceLabel : activeInstallSourceLabel,
              defaultValue: i18n.language.startsWith("zh")
                ? `${agent.display_name} 下暂无${activeSourceLabel}技能`
                : `No ${activeSourceLabel} skills installed for ${agent.display_name}`,
            })}
          />
        ) : filteredSkills.length === 0 && filteredFolderGroups.length === 0 ? (
          <EmptyState
            message={t("platform.noMatch", { query: searchQuery })}
          />
        ) : (
          <div className="space-y-6">
            {viewMode === "folders" && filteredFolderGroups.length > 0 && (
              <section className="space-y-3" aria-label={t("skillFolder.foldersTitle")}>
                <div className="flex items-center gap-2">
                  <FolderOpen className="size-4 text-primary" />
                  <h2 className="text-sm font-semibold">{t("skillFolder.foldersTitle")}</h2>
                </div>
                <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
                  {filteredFolderGroups.map((group) => (
                    <SkillFolderCard
                      key={group.relativePath}
                      name={group.name}
                      path={group.path}
                      skillCount={group.skillCount}
                      previewNames={group.skills.map((skill) => skill.name)}
                      onOpen={() => handleOpenFolderDrawer(group.relativePath)}
                    />
                  ))}
                </div>
              </section>
            )}

            {filteredSkills.length > 0 && (
              <section className="space-y-3">
                {viewMode === "folders" && (
                  <div className="flex items-center gap-2">
                    <Blocks className="size-4 text-primary" />
                    <h2 className="text-sm font-semibold">{t("skillFolder.topLevelSkills")}</h2>
                  </div>
                )}
                <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
                  {filteredSkills.map((skill) => (
                    (() => {
                      const usage = usageBySkillId.get(skill.id);
                      const hasExternalCounterpart = skills.some(
                        (candidate) =>
                          candidate.id === skill.id &&
                          candidate.is_read_only &&
                          candidate.row_id !== skill.row_id
                      );
                      return (
                        <UnifiedSkillCard
                          key={getSkillRowKey(skill)}
                          checkbox={canTransferSkill(skill) ? { checked: transferSelection.selected.has(skillSelectionKey(skill)), onChange: () => transferSelection.toggle(skillSelectionKey(skill)) } : undefined}
                          name={skill.name}
                          description={skill.description}
                          translation={skill.file_path
                            ? {
                                resourceId: `local:${skill.file_path}`,
                                filePath: skill.file_path,
                              }
                            : undefined}
                          sourceType={skill.file_path
                            ? skill.link_type as "symlink" | "copy" | "native"
                            : undefined}
                          originKind={skill.source_kind ?? null}
                          isReadOnly={skill.is_read_only ?? false}
                          isUniversalSource={isUniversalSource(skill)}
                          usageControl={usage && !skill.is_read_only
                            ? {
                                enabled: usage.enabled,
                                pausedByBulk: usage.paused_by_bulk,
                                onCheckedChange: (enabled) => void handleUsageChange(skill.id, enabled),
                                isLoading: agentId
                                  ? (usageUpdatingSkillKeys[`${agentId}::${skill.id}`] ?? false) ||
                                    (usageUpdatingAgentIds[agentId] ?? false)
                                  : false,
                              }
                            : undefined}
                          externalUsageCount={hasExternalCounterpart && usage && !usage.enabled ? 1 : 0}
                          isLoading={
                            agentId
                              ? (pendingSkillActionKeys[`${agentId}::${skill.id}`] ?? false) ||
                                (usageUpdatingSkillKeys[`${agentId}::${skill.id}`] ?? false) ||
                                (usageUpdatingAgentIds[agentId] ?? false)
                              : false
                          }
                          onDetail={() => handleOpenDrawer(skill)}
                          onInstallTo={
                            skill.is_read_only
                              ? undefined
                              : () => handleInstallClick(skill.id)
                          }
                          onManageUniversal={
                            isUniversalSource(skill) ? () => navigate("/universal") : undefined
                          }
                          onUninstallFromPlatform={
                            usage && !skill.is_read_only
                              ? () => void handleDeleteSkill(skill.id)
                              : undefined
                          }
                          uninstallFromLabel={t("skillUsage.deleteSkill", {
                            name: skill.name,
                            platform: agent.display_name,
                          })}
                          detailButtonRef={(node) => setDetailButtonRef(getSkillRowKey(skill), node)}
                        />
                      );
                    })()
                  ))}
                </div>
              </section>
            )}
          </div>
        )}
      </div>

      {/* Install Dialog */}
      <InstallDialog
        open={isDialogOpen}
        onOpenChange={setIsDialogOpen}
        skill={installTargetSkill}
        agents={centralAgents}
        onInstall={handleInstall}
      />

      <Dialog
        open={isPlatformDeleteDialogOpen}
        onOpenChange={(open) => {
          if (!isPlatformUsageUpdating) setIsPlatformDeleteDialogOpen(open);
        }}
      >
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle>{t("skillUsage.deletePlatformTitle", { name: agent.display_name })}</DialogTitle>
            <DialogDescription>
              {t("skillUsage.deletePlatformDescription", {
                active: usageStatus?.active_count ?? 0,
                paused: usageStatus?.paused_count ?? 0,
              })}
            </DialogDescription>
          </DialogHeader>
          <DialogBody className="space-y-2 text-sm text-muted-foreground">
            <p>{t("skillUsage.deletePlatformOriginalKept")}</p>
            {(usageStatus?.external_count ?? 0) > 0 && (
              <p className="text-amber-700 dark:text-amber-300">
                {t("skillUsage.deletePlatformExternal", {
                  count: usageStatus?.external_count ?? 0,
                })}
              </p>
            )}
          </DialogBody>
          <DialogFooter>
            <Button
              type="button"
              variant="outline"
              onClick={() => setIsPlatformDeleteDialogOpen(false)}
              disabled={isPlatformUsageUpdating}
            >
              {t("common.cancel")}
            </Button>
            <Button
              type="button"
              variant="destructive"
              onClick={() => void handlePlatformDelete()}
              disabled={!canDeletePlatform || isPlatformUsageUpdating}
            >
              {isPlatformUsageUpdating && <Loader2 className="size-4 animate-spin" />}
              {t("skillUsage.confirmDeletePlatform")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <SkillDetailDrawer
        open={isDrawerOpen}
        skillId={drawerSkill?.id ?? null}
        agentId={agentId ?? null}
        rowId={drawerSkill?.row_id ?? null}
        onOpenChange={(open) => {
          setIsDrawerOpen(open);
          if (!open) {
            setDrawerSkill(null);
          }
        }}
        returnFocusRef={
          returnFocusRowKey
            ? {
                current: detailButtonRefs.current[returnFocusRowKey] ?? null,
              }
            : undefined
        }
      />

      <SkillFolderDrawer
        open={isFolderDrawerOpen}
        title={folderDrawerGroup?.name ?? folderDrawerGroupPath ?? t("skillFolder.foldersTitle")}
        path={folderDrawerGroup?.path}
        skills={folderDrawerSkills}
        loading={false}
        onInstallAll={
          folderDrawerPluginLabel ? handleInstallPluginBundleClick : undefined
        }
        onOpenChange={(open) => {
          setIsFolderDrawerOpen(open);
          if (!open) {
            setFolderDrawerGroupPath(null);
          }
        }}
        onInstallationsChange={async () => {
          await Promise.all([
            refreshCounts(),
            agentId ? getSkillsByAgent(agentId) : Promise.resolve(),
          ]);
        }}
      />

      <CollectionInstallDialog
        open={isPluginBundleDialogOpen}
        onOpenChange={(open) => {
          setIsPluginBundleDialogOpen(open);
          if (!open) setPluginBundleTarget(null);
        }}
        collectionName={pluginBundleTarget?.name ?? ""}
        skillCount={pluginBundleTarget?.skillCount ?? 0}
        agents={centralAgents}
        description={t("skillFolder.installBundleDesc", {
          count: pluginBundleTarget?.skillCount ?? 0,
        })}
        onInstall={handleInstallPluginBundle}
      />
    </div>
  );
}

import { useEffect, useState } from "react";
import { useNavigate, useLocation } from "react-router-dom";
import {
  Loader2,
  LibraryBig,
  Layers,
  Radar,
  Store,
  Share2,
  Settings,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { PlatformIcon } from "@/components/platform/PlatformIcon";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { usePlatformStore } from "@/stores/platformStore";
import { useCollectionStore } from "@/stores/collectionStore";
import { useDiscoverStore } from "@/stores/discoverStore";
import { useObsidianStore } from "@/stores/obsidianStore";
import { cn } from "@/lib/utils";
import { isToggleableAgent, UNIVERSAL_AGENT_ID } from "@/lib/agents";
import type { AgentWithStatus } from "@/types";
import {
  DashboardNavItem,
  DashboardPlatformToggle,
  DashboardSectionLabel,
  DashboardSidebarFrame,
  SIDEBAR_SHOW_ALL_KEY,
} from "./DashboardShell";

const OBSIDIAN_PLATFORM_ID = "obsidian";

function ManagedPlatformNavItem({
  agent,
  expanded,
  count,
  isActive,
  isUpdating,
  visibilityLabel,
  onOpen,
  onVisibilityChange,
}: {
  agent: AgentWithStatus;
  expanded: boolean;
  count?: number;
  isActive: boolean;
  isUpdating: boolean;
  visibilityLabel: string;
  onOpen: () => void;
  onVisibilityChange: (visible: boolean) => void;
}) {
  return (
    <div
      className={cn(
        "relative flex w-full items-center rounded-md transition-colors",
        !isActive && "text-muted-foreground hover:bg-primary/10 hover:text-primary",
        isActive && "bg-hover-bg font-medium text-white",
        !agent.is_enabled && "opacity-60"
      )}
    >
      <button
        type="button"
        onClick={onOpen}
        disabled={isUpdating}
        title={agent.display_name}
        aria-label={agent.display_name}
        aria-current={isActive ? "page" : undefined}
        className={cn(
          "flex min-w-0 flex-1 items-center disabled:cursor-wait",
          expanded ? "gap-2.5 px-2.5 py-1.5 text-sm" : "justify-center px-1.5 py-2"
        )}
      >
        <span className="shrink-0">
          {isUpdating ? (
            <Loader2 className="size-4 animate-spin" />
          ) : (
            <PlatformIcon agentId={agent.id} className="size-4" />
          )}
        </span>
        {expanded && (
          <>
            <span className="flex-1 truncate text-left">{agent.display_name}</span>
            {count !== undefined && count > 0 && (
              <span className="shrink-0 rounded-full bg-muted/60 px-1.5 py-0.5 font-mono text-[10px] tabular-nums text-muted-foreground">
                {count}
              </span>
            )}
          </>
        )}
      </button>
      {expanded && (
        <Switch
          checked={agent.is_enabled}
          disabled={isUpdating}
          onCheckedChange={onVisibilityChange}
          aria-label={visibilityLabel}
          title={visibilityLabel}
          className="mr-2 scale-75"
        />
      )}
      {isActive && (
        <span
          className="absolute top-1.5 bottom-1.5 left-0 w-0.5 rounded-r bg-white"
          aria-hidden="true"
        />
      )}
    </div>
  );
}

function getActiveObsidianVaultId(pathname: string): string | null {
  const obsidianPrefix = "/obsidian/";
  if (!pathname.startsWith(obsidianPrefix)) {
    return null;
  }

  const encodedVaultId = pathname.slice(obsidianPrefix.length);
  if (!encodedVaultId) {
    return null;
  }

  try {
    return decodeURIComponent(encodedVaultId);
  } catch {
    return encodedVaultId;
  }
}

// ─── Sidebar ────────────────────────────────────────────────────────────────

export function Sidebar() {
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const { t } = useTranslation();
  const {
    agents, skillsByAgent, isLoading, isRefreshing, updatingAgentIds,
    setAgentVisibility, setAllAgentsVisibility,
  } = usePlatformStore();

  const collections = useCollectionStore((s) => s.collections);
  const loadCollections = useCollectionStore((s) => s.loadCollections);

  const totalDiscovered = useDiscoverStore((s) => s.totalSkillsFound);
  const loadDiscoveredSkills = useDiscoverStore((s) => s.loadDiscoveredSkills);
  const obsidianVaults = useObsidianStore((s) => s.vaults);
  const loadObsidianVaults = useObsidianStore((s) => s.loadVaults);

  const [expanded, setExpanded] = useState(true);
  const [showAllPlatforms, setShowAllPlatforms] = useState(() => {
    try {
      return window.localStorage.getItem(SIDEBAR_SHOW_ALL_KEY) === "true";
    } catch {
      return false;
    }
  });

  useEffect(() => {
    loadCollections();
    loadDiscoveredSkills();
    loadObsidianVaults();
  }, [loadCollections, loadDiscoveredSkills, loadObsidianVaults]);

  function toggleShowAllPlatforms() {
    setShowAllPlatforms((previous) => {
      const next = !previous;
      try {
        window.localStorage.setItem(SIDEBAR_SHOW_ALL_KEY, String(next));
      } catch {
        // Ignore storage failures and keep the in-memory preference.
      }
      return next;
    });
  }

  const catalogAgents = agents.filter(isToggleableAgent);
  const isUpdatingPlatforms = Object.values(updatingAgentIds).some(Boolean);
  const platformAgents = catalogAgents.filter(
    (agent) =>
      showAllPlatforms ||
      (agent.is_enabled && agent.is_detected && (skillsByAgent[agent.id] ?? 0) > 0)
  );
  const lobsterAgents = platformAgents.filter((a) => a.category === "lobster");
  const codingAgents = platformAgents.filter((a) => a.category !== "lobster");
  const populatedObsidianVaults = obsidianVaults.filter((vault) => vault.skill_count > 0);
  const activeObsidianVaultId = getActiveObsidianVaultId(pathname);

  const isCollectionActive = pathname === "/collections";

  function handleCollectionClick() {
    navigate("/collections");
  }

  async function handleAgentVisibilityChange(agent: AgentWithStatus, visible: boolean) {
    try {
      await setAgentVisibility(agent.id, visible);
    } catch {
      toast.error(
        t("sidebar.platformToggleError", {
          name: agent.display_name,
        })
      );
    }
  }

  async function handleAllAgentsVisibilityChange(visible: boolean) {
    try {
      await setAllAgentsVisibility(visible);
    } catch {
      toast.error(t("sidebar.allPlatformsToggleError"));
    }
  }

  function renderPlatformAgent(agent: AgentWithStatus) {
    const isActive = pathname === `/platform/${agent.id}`;
    if (!showAllPlatforms) {
      return (
        <DashboardNavItem
          key={agent.id}
          label={agent.display_name}
          isActive={isActive}
          onClick={() => navigate(`/platform/${agent.id}`)}
          icon={<PlatformIcon agentId={agent.id} className="size-4" />}
          expanded={expanded}
          count={skillsByAgent[agent.id]}
        />
      );
    }

    const visibilityLabel = t(
      agent.is_enabled ? "sidebar.hidePlatform" : "sidebar.showPlatform",
      { name: agent.display_name }
    );
    return (
      <ManagedPlatformNavItem
        key={agent.id}
        agent={agent}
        expanded={expanded}
        count={skillsByAgent[agent.id]}
        isActive={isActive}
        isUpdating={isUpdatingPlatforms || isRefreshing}
        visibilityLabel={visibilityLabel}
        onOpen={() => navigate(`/platform/${agent.id}`)}
        onVisibilityChange={(visible) => void handleAgentVisibilityChange(agent, visible)}
      />
    );
  }

  return (
    <DashboardSidebarFrame
      expanded={expanded}
      onExpandedChange={setExpanded}
      title={t("webDashboard.title")}
      subtitle={t("app.name")}
      navLabel={t("webDashboard.navLabel")}
      collapseLabel={t("sidebar.collapseSidebar")}
      expandLabel={t("sidebar.expandSidebar")}
      footer={
        <DashboardNavItem
          to="/settings"
          label={t("sidebar.settings")}
          icon={<Settings className="size-4" />}
          expanded={expanded}
        />
      }
    >
        <DashboardSectionLabel expanded={expanded} first>
          {t("webDashboard.sections.libraryNav")}
        </DashboardSectionLabel>
        {/* Central Skills */}
        <DashboardNavItem
          label={t("sidebar.centralSkills")}
          isActive={pathname === "/central" || pathname === "/"}
          onClick={() => navigate("/central")}
          icon={<LibraryBig className="size-4" />}
          expanded={expanded}
          count={skillsByAgent["central"]}
        />

        {/* Discover */}
        <DashboardNavItem
          label={t("sidebar.discovered")}
          isActive={pathname.startsWith("/discover")}
          onClick={() => navigate("/discover")}
          icon={<Radar className="size-4" />}
          expanded={expanded}
          count={totalDiscovered}
        />

        {/* Marketplace */}
        <DashboardNavItem
          label={t("marketplace.title")}
          isActive={pathname === "/marketplace"}
          onClick={() => navigate("/marketplace")}
          icon={<Store className="size-4" />}
          expanded={expanded}
        />

        {/* Collections */}
        <DashboardNavItem
          label={t("sidebar.collections")}
          isActive={isCollectionActive}
          onClick={handleCollectionClick}
          icon={<Layers className="size-4" />}
          expanded={expanded}
          count={collections.length}
        />

        <DashboardSectionLabel expanded={expanded}>
          {t("sidebar.installTargetsSection")}
        </DashboardSectionLabel>

        <DashboardNavItem
          label={t("sidebar.universal")}
          isActive={pathname === "/universal"}
          onClick={() => navigate("/universal")}
          icon={<Share2 className="size-4" />}
          expanded={expanded}
          count={skillsByAgent[UNIVERSAL_AGENT_ID]}
        />

        {/* Platform icons */}
        {isLoading ? (
          <div className={cn(
            "flex items-center py-2 text-muted-foreground text-sm",
            expanded ? "gap-2 px-2.5" : "justify-center"
          )}>
            <Loader2 className="size-4 animate-spin shrink-0" />
            {expanded && <span>{t("sidebar.scanning")}</span>}
          </div>
        ) : (
          <>
            {/* Obsidian vaults */}
            {populatedObsidianVaults.length > 0 && (
              <>
                <DashboardSectionLabel expanded={expanded}>
                  {t("sidebar.categoryObsidian")}
                </DashboardSectionLabel>
                {populatedObsidianVaults.map((vault) => {
                  const vaultAccessibleLabel = t("sidebar.obsidianVaultLabel", {
                    name: vault.name,
                    count: vault.skill_count,
                    path: vault.path,
                  });
                  return (
                    <DashboardNavItem
                      key={vault.id}
                      label={vault.name}
                      ariaLabel={vaultAccessibleLabel}
                      title={vaultAccessibleLabel}
                      isActive={activeObsidianVaultId === vault.id}
                      onClick={() => navigate(`/obsidian/${encodeURIComponent(vault.id)}`)}
                      icon={<PlatformIcon agentId={OBSIDIAN_PLATFORM_ID} className="size-4" />}
                      expanded={expanded}
                      count={vault.skill_count}
                    />
                  );
                })}
              </>
            )}

            {expanded && showAllPlatforms && catalogAgents.length > 0 && (
              <div className="flex gap-1 px-1 pt-2" aria-busy={isUpdatingPlatforms}>
                <Button
                  variant="outline"
                  size="xs"
                  className="min-w-0 flex-1"
                  disabled={isUpdatingPlatforms || isRefreshing || catalogAgents.every((agent) => agent.is_enabled)}
                  onClick={() => void handleAllAgentsVisibilityChange(true)}
                >
                  {t("sidebar.showAllPlatformEntries")}
                </Button>
                <Button
                  variant="outline"
                  size="xs"
                  className="min-w-0 flex-1"
                  disabled={isUpdatingPlatforms || isRefreshing || catalogAgents.every((agent) => !agent.is_enabled)}
                  onClick={() => void handleAllAgentsVisibilityChange(false)}
                >
                  {t("sidebar.hideAllPlatformEntries")}
                </Button>
              </div>
            )}

            {/* Lobster agents */}
            {lobsterAgents.length > 0 && (
              <>
                <DashboardSectionLabel expanded={expanded}>
                  {t("sidebar.categoryLobster")}
                </DashboardSectionLabel>
                {lobsterAgents.map(renderPlatformAgent)}
              </>
            )}

            {/* Coding agents */}
            {codingAgents.length > 0 && (
              <>
                <DashboardSectionLabel expanded={expanded}>
                  {t("sidebar.categoryCoding")}
                </DashboardSectionLabel>
                {codingAgents.map(renderPlatformAgent)}
              </>
            )}
          </>
        )}

        {!isLoading && (
          <DashboardPlatformToggle
            expanded={expanded}
            showAll={showAllPlatforms}
            onClick={toggleShowAllPlatforms}
            showLabel={t("sidebar.showAllPlatforms")}
            hideLabel={t("sidebar.showActivePlatforms")}
          />
        )}
    </DashboardSidebarFrame>
  );
}

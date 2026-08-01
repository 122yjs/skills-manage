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
import { PlatformIcon } from "@/components/platform/PlatformIcon";
import { usePlatformStore } from "@/stores/platformStore";
import { useCollectionStore } from "@/stores/collectionStore";
import { useDiscoverStore } from "@/stores/discoverStore";
import { useObsidianStore } from "@/stores/obsidianStore";
import { cn } from "@/lib/utils";
import { isEnabledInstallTargetAgent, UNIVERSAL_AGENT_ID } from "@/lib/agents";
import {
  DashboardNavItem,
  DashboardPlatformToggle,
  DashboardSectionLabel,
  DashboardSidebarFrame,
  SIDEBAR_SHOW_ALL_KEY,
} from "./DashboardShell";

const OBSIDIAN_PLATFORM_ID = "obsidian";

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
  const { agents, skillsByAgent, isLoading } = usePlatformStore();

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

  const platformAgents = agents.filter(
    (a) =>
      isEnabledInstallTargetAgent(a) &&
      a.id !== UNIVERSAL_AGENT_ID &&
      a.category !== "shared" &&
      (showAllPlatforms || (skillsByAgent[a.id] ?? 0) > 0)
  );
  const lobsterAgents = platformAgents.filter((a) => a.category === "lobster");
  const codingAgents = platformAgents.filter((a) => a.category !== "lobster");
  const populatedObsidianVaults = obsidianVaults.filter((vault) => vault.skill_count > 0);
  const activeObsidianVaultId = getActiveObsidianVaultId(pathname);

  const isCollectionActive = pathname === "/collections";

  function handleCollectionClick() {
    navigate("/collections");
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

            {/* Lobster agents */}
            {lobsterAgents.length > 0 && (
              <>
                <DashboardSectionLabel expanded={expanded}>
                  {t("sidebar.categoryLobster")}
                </DashboardSectionLabel>
                {lobsterAgents.map((agent) => (
                  <DashboardNavItem
                    key={agent.id}
                    label={agent.display_name}
                    isActive={pathname === `/platform/${agent.id}`}
                    onClick={() => navigate(`/platform/${agent.id}`)}
                    icon={<PlatformIcon agentId={agent.id} className="size-4" />}
                    expanded={expanded}
                    count={skillsByAgent[agent.id]}
                  />
                ))}
              </>
            )}

            {/* Coding agents */}
            {codingAgents.length > 0 && (
              <>
                <DashboardSectionLabel expanded={expanded}>
                  {t("sidebar.categoryCoding")}
                </DashboardSectionLabel>
                {codingAgents.map((agent) => (
                  <DashboardNavItem
                    key={agent.id}
                    label={agent.display_name}
                    isActive={pathname === `/platform/${agent.id}`}
                    onClick={() => navigate(`/platform/${agent.id}`)}
                    icon={<PlatformIcon agentId={agent.id} className="size-4" />}
                    expanded={expanded}
                    count={skillsByAgent[agent.id]}
                  />
                ))}
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
            hideLabel={t("sidebar.hideEmptyPlatforms")}
          />
        )}
    </DashboardSidebarFrame>
  );
}

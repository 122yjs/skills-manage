import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  LayersIcon,
  LibraryBigIcon,
  RadarIcon,
  SettingsIcon,
  StoreIcon,
} from "lucide-react";

import { PlatformIcon } from "@/components/platform/PlatformIcon";
import {
  DashboardNavItem,
  DashboardPlatformToggle,
  DashboardSectionLabel,
  DashboardSidebarFrame,
  SIDEBAR_SHOW_ALL_KEY,
} from "@/components/layout/DashboardShell";
import { RECOMMENDED_SKILLS } from "@/data/officialSources";

import type { DashboardSnapshot } from "./types";

interface DashboardSidebarProps {
  snapshot: DashboardSnapshot;
}

export function DashboardSidebar({ snapshot }: DashboardSidebarProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(true);
  const [showAllPlatforms, setShowAllPlatforms] = useState(() => {
    try {
      return window.localStorage.getItem(SIDEBAR_SHOW_ALL_KEY) === "true";
    } catch {
      return false;
    }
  });

  const discoveredSkillCount = useMemo(
    () =>
      snapshot.discoveredProjects.reduce(
        (total, project) => total + project.skills.length,
        0,
      ),
    [snapshot.discoveredProjects],
  );

  // 스킬이 하나라도 보이는 플랫폼만 기본 노출하고, 나머지는 토글로 펼친다.
  const installTargets = snapshot.platforms.filter(
    (platform) => platform.isEnabled && platform.category !== "shared",
  );
  const visibleTargets = installTargets.filter(
    (platform) => showAllPlatforms || platform.skills.length > 0,
  );
  const hasEmptyTargets = installTargets.some(
    (platform) => platform.skills.length === 0,
  );
  const codingTargets = visibleTargets.filter((platform) => platform.category === "coding");
  const lobsterTargets = visibleTargets.filter((platform) => platform.category === "lobster");
  const otherTargets = visibleTargets.filter(
    (platform) => platform.category !== "coding" && platform.category !== "lobster",
  );
  const sharedTargets = snapshot.platforms.filter(
    (platform) => platform.category === "shared",
  );

  function toggleShowAll() {
    setShowAllPlatforms((previous) => {
      const next = !previous;
      try {
        window.localStorage.setItem(SIDEBAR_SHOW_ALL_KEY, String(next));
      } catch {
        // 저장에 실패필 경우 메모리 상태만 유지한다.
      }
      return next;
    });
  }

  const renderPlatformItems = (targets: typeof installTargets) =>
    targets.map((platform) => (
      <DashboardNavItem
        key={platform.id}
        to={`/platform/${encodeURIComponent(platform.id)}`}
        label={platform.displayName}
        icon={<PlatformIcon agentId={platform.id} className="size-4" />}
        expanded={expanded}
        count={platform.skills.length}
        title={`${platform.displayName} · ${platform.globalSkillsDir}`}
      />
    ));

  return (
    <DashboardSidebarFrame
      expanded={expanded}
      onExpandedChange={setExpanded}
      title={t("webDashboard.title")}
      subtitle={snapshot.appName}
      navLabel={t("webDashboard.navLabel")}
      collapseLabel={t("sidebar.collapseSidebar")}
      expandLabel={t("sidebar.expandSidebar")}
      footer={
        <DashboardNavItem
          to="/settings"
          label={t("sidebar.settings")}
          icon={<SettingsIcon className="size-4" />}
          expanded={expanded}
        />
      }
    >
        <DashboardSectionLabel expanded={expanded} first>
          {t("webDashboard.sections.libraryNav")}
        </DashboardSectionLabel>

        <DashboardNavItem
          to="/central"
          label={t("sidebar.centralSkills")}
          icon={<LibraryBigIcon className="size-4" />}
          expanded={expanded}
          count={snapshot.summary.centralSkillCount}
        />
        <DashboardNavItem
          to="/discover"
          label={t("sidebar.discovered")}
          icon={<RadarIcon className="size-4" />}
          expanded={expanded}
          count={discoveredSkillCount}
        />
        <DashboardNavItem
          to="/marketplace"
          label={t("marketplace.title")}
          icon={<StoreIcon className="size-4" />}
          expanded={expanded}
          count={Math.max(
            snapshot.summary.marketplaceSkillCount,
            RECOMMENDED_SKILLS.length,
          )}
        />
        <DashboardNavItem
          to="/collections"
          label={t("sidebar.collections")}
          icon={<LayersIcon className="size-4" />}
          expanded={expanded}
          count={snapshot.summary.collectionCount}
        />

        <DashboardSectionLabel expanded={expanded}>
          {t("sidebar.installTargetsSection")}
        </DashboardSectionLabel>

        {sharedTargets.map((platform) => (
          <DashboardNavItem
            key={platform.id}
            to={`/platform/${encodeURIComponent(platform.id)}`}
            label={t("sidebar.universal")}
            icon={<PlatformIcon agentId={platform.id} className="size-4" />}
            expanded={expanded}
            count={platform.skills.length}
          />
        ))}

        {lobsterTargets.length > 0 && (
          <>
            <DashboardSectionLabel expanded={expanded}>
              {t("sidebar.categoryLobster")}
            </DashboardSectionLabel>
            {renderPlatformItems(lobsterTargets)}
          </>
        )}

        {codingTargets.length > 0 && (
          <>
            <DashboardSectionLabel expanded={expanded}>
              {t("sidebar.categoryCoding")}
            </DashboardSectionLabel>
            {renderPlatformItems(codingTargets)}
          </>
        )}

        {otherTargets.length > 0 && renderPlatformItems(otherTargets)}

        {hasEmptyTargets && (
          <DashboardPlatformToggle
            expanded={expanded}
            showAll={showAllPlatforms}
            onClick={toggleShowAll}
            showLabel={t("sidebar.showAllPlatforms")}
            hideLabel={t("sidebar.hideEmptyPlatforms")}
          />
        )}
    </DashboardSidebarFrame>
  );
}

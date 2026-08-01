import { useTranslation } from "react-i18next";
import { useLocation } from "react-router-dom";
import { Search } from "lucide-react";

import { usePlatformStore } from "@/stores/platformStore";
import { useDiscoverStore } from "@/stores/discoverStore";
import { cn } from "@/lib/utils";
import { DashboardHeader } from "./DashboardShell";

interface TopBarProps {
  onSearchClick: () => void;
}

export function TopBar({ onSearchClick }: TopBarProps) {
  const { t } = useTranslation();
  const { pathname } = useLocation();

  const agents = usePlatformStore((s) => s.agents);
  const isScanning = useDiscoverStore((s) => s.isScanning);

  const pageTitle = (() => {
    if (pathname === "/central" || pathname === "/") {
      return t("sidebar.centralSkills");
    }
    if (pathname.startsWith("/platform/")) {
      const agentId = pathname.split("/platform/")[1];
      const agent = agents.find((a) => a.id === agentId);
      return agent?.display_name ?? agentId;
    }
    if (pathname === "/universal") {
      return t("sidebar.universal");
    }
    if (pathname.startsWith("/discover")) {
      return t("sidebar.discovered");
    }
    if (pathname === "/marketplace") {
      return t("marketplace.title");
    }
    if (pathname === "/collections") {
      return t("sidebar.collections");
    }
    if (pathname === "/settings") {
      return t("sidebar.settings");
    }
    if (pathname.startsWith("/skill/")) {
      return t("globalSearch.skillDetail");
    }
    return t("webDashboard.title");
  })();

  const isMac =
    typeof navigator !== "undefined" &&
    navigator.platform.toUpperCase().includes("MAC");

  return (
    <DashboardHeader title={pageTitle}>
      <button
        type="button"
        onClick={onSearchClick}
        className={cn(
          "flex h-8 min-w-0 items-center gap-2 rounded-md border border-border/50 bg-muted/40 px-3 text-sm text-muted-foreground",
          "w-[min(26rem,42vw)] hover:border-border hover:bg-muted/60 transition-colors cursor-pointer",
        )}
      >
        <Search className="size-3.5 shrink-0" />
        <span className="flex-1 truncate text-left">{t("globalSearch.trigger")}</span>
        <kbd className="hidden items-center gap-0.5 rounded border border-border/50 px-1 py-0.5 font-mono text-[10px] text-muted-foreground/60 sm:inline-flex">
          {isMac ? "⌘" : "Ctrl"}K
        </kbd>
      </button>

      {/* Scan indicator */}
      {isScanning && (
        <div className="mr-2 flex items-center gap-1.5 text-xs text-primary shrink-0">
          <span className="relative flex size-2">
            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-primary opacity-75" />
            <span className="relative inline-flex rounded-full size-2 bg-primary" />
          </span>
          <span className="text-primary/70">{t("discover.scanning")}</span>
        </div>
      )}

    </DashboardHeader>
  );
}

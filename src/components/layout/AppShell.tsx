import { useEffect, useRef, useState } from "react";
import { Outlet, useLocation } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";
import { GlobalSearchDialog } from "./GlobalSearchDialog";
import { usePlatformStore } from "@/stores/platformStore";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { useDiscoverStore } from "@/stores/discoverStore";
import { useStorageStore } from "@/stores/storageStore";
import { LegacyMigrationNotice } from "./LegacyMigrationNotice";
import { DevToolSetupDialog } from "@/components/settings/DevToolSetupDialog";
import { useDevToolSetupStore } from "@/stores/devToolSetupStore";
import { useSkillUsageStore } from "@/stores/skillUsageStore";

/**
 * Top-level app shell shared visually with the read-only web dashboard.
 * Triggers the initial platform scan on mount.
 */
export function AppShell() {
  const [isSearchOpen, setIsSearchOpen] = useState(false);
  const mainRef = useRef<HTMLElement | null>(null);
  const didInitializeRef = useRef(false);
  const { pathname } = useLocation();

  const initialize = usePlatformStore((s) => s.initialize);
  const rescan = usePlatformStore((s) => s.rescan);
  const loadCentralSkills = useCentralSkillsStore((s) => s.loadCentralSkills);
  const rescanDiscoverFromDisk = useDiscoverStore((s) => s.rescanFromDisk);
  const loadStorageStatus = useStorageStore((s) => s.loadStatus);
  const loadDevToolSetup = useDevToolSetupStore((s) => s.load);
  const loadUsageStatus = useSkillUsageStore((s) => s.loadUsageStatus);

  useEffect(() => {
    void loadDevToolSetup();
    void loadStorageStatus().catch(() => undefined);
  }, [loadDevToolSetup, loadStorageStatus]);

  useEffect(() => {
    // 도구 선택은 표시 설정이므로 기다리지 않고 실행마다 한 번 스캔한다.
    if (didInitializeRef.current) return;
    didInitializeRef.current = true;
    void initialize().finally(() => {
      void Promise.allSettled([loadCentralSkills(), loadUsageStatus()]);
    });
  }, [initialize, loadCentralSkills, loadUsageStatus]);

  useEffect(() => {
    if (!mainRef.current) return;
    mainRef.current.scrollTop = 0;
  }, [pathname]);

  async function handleGlobalRescan() {
    await rescan();
    await Promise.allSettled([
      loadCentralSkills(),
      rescanDiscoverFromDisk(),
      loadUsageStatus(),
    ]);
  }

  function handleAction(action: string) {
    switch (action) {
      case "rescan":
        void handleGlobalRescan();
        break;
    }
  }

  return (
    <div className="flex h-screen overflow-hidden bg-background text-foreground">
      <Sidebar />
      <div className="flex min-w-0 flex-1 flex-col">
        <TopBar onSearchClick={() => setIsSearchOpen(true)} />
        <LegacyMigrationNotice onMigrated={handleGlobalRescan} />
        <main ref={mainRef} className="flex-1 min-h-0 min-w-0 overflow-hidden">
          <Outlet />
        </main>
      </div>
      <GlobalSearchDialog
        open={isSearchOpen}
        onOpenChange={setIsSearchOpen}
        onAction={handleAction}
      />
      <DevToolSetupDialog />
    </div>
  );
}

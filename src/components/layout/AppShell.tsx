import { useCallback, useEffect, useRef, useState } from "react";
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
import { invoke, isTauriRuntime } from "@/lib/tauri";

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

  const refreshOrigins = useCallback(async () => {
    if (!isTauriRuntime()) return;
    await invoke("check_linked_skill_origins");
    // 원본 연결이 끝나면 열린 플랫폼 목록도 DB의 새 링크를 읽는다.
    usePlatformStore.setState((state) => ({
      scanGeneration: (state.scanGeneration ?? 0) + 1,
    }));
    await loadCentralSkills();
  }, [loadCentralSkills]);

  useEffect(() => {
    void loadDevToolSetup();
    void loadStorageStatus().catch(() => undefined);
  }, [loadDevToolSetup, loadStorageStatus]);

  useEffect(() => {
    // 도구 선택은 표시 설정이므로 기다리지 않고 실행마다 한 번 스캔한다.
    if (didInitializeRef.current) return;
    didInitializeRef.current = true;
    void initialize().finally(() => {
      // 플랫폼 스캔이 끝난 뒤 중앙 목록·디스크 발견 스킬·사용 현황을 한 번에 갱신한다.
      // rescanFromDisk는 저장된 검색 루트(get_scan_roots)로 start_project_scan을 실행한다.
      void Promise.allSettled([
        loadCentralSkills(),
        rescanDiscoverFromDisk(),
        loadUsageStatus(),
      ]);
      void refreshOrigins().catch(() => undefined);
    });
  }, [initialize, loadCentralSkills, rescanDiscoverFromDisk, loadUsageStatus, refreshOrigins]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    const timer = window.setInterval(() => {
      void refreshOrigins().catch(() => undefined);
    }, 6 * 60 * 60 * 1000);
    return () => window.clearInterval(timer);
  }, [refreshOrigins]);

  useEffect(() => {
    if (!mainRef.current) return;
    mainRef.current.scrollTop = 0;
  }, [pathname]);

  async function handleGlobalRescan() {
    await rescan();
    void refreshOrigins().catch(() => undefined);
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

import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Navigate,
  Route,
  Routes,
  useLocation,
  useParams,
} from "react-router-dom";
import { RefreshCwIcon, ShieldCheckIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import { DashboardHeader } from "@/components/layout/DashboardShell";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { cn } from "@/lib/utils";

import { DashboardSidebar } from "./DashboardSidebar";
import { requestDashboardSnapshot } from "./dashboardData";
import { CollectionsPage } from "./pages/CollectionsPage";
import { DiscoverPage } from "./pages/DiscoverPage";
import { LibraryPage } from "./pages/LibraryPage";
import { MarketplacePage } from "./pages/MarketplacePage";
import { OverviewPage } from "./pages/OverviewPage";
import { PlatformPage } from "./pages/PlatformPage";
import { SettingsPage } from "./pages/SettingsPage";
import type { DashboardSnapshot } from "./types";

function LoadingShell() {
  return (
    <div className="flex h-screen animate-pulse bg-background" aria-label="loading">
      <div className="w-56 border-r border-border bg-sidebar" />
      <div className="flex-1 space-y-4 p-6">
        <div className="h-10 w-64 rounded-lg bg-muted/60" />
        <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-5">
          {Array.from({ length: 5 }, (_, index) => (
            <div key={index} className="h-28 rounded-xl bg-muted/45" />
          ))}
        </div>
        <div className="h-80 rounded-xl bg-muted/45" />
      </div>
    </div>
  );
}

function ErrorScreen({
  error,
  isRefreshing,
  onRetry,
}: {
  error: string;
  isRefreshing: boolean;
  onRetry: () => void;
}) {
  const { t } = useTranslation();
  return (
    <main className="grid min-h-screen place-items-center bg-background p-6 text-foreground">
      <Card className="w-full max-w-xl">
        <CardHeader>
          <CardTitle>{t("webDashboard.errorTitle")}</CardTitle>
          <CardDescription>{t("webDashboard.errorHint")}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <pre className="overflow-auto rounded-lg bg-destructive/10 p-3 text-xs text-destructive ring-1 ring-destructive/20">
            {error}
          </pre>
          <Button onClick={onRetry} disabled={isRefreshing}>
            <RefreshCwIcon className={cn("size-4", isRefreshing && "animate-spin")} />
            {t("common.retry")}
          </Button>
        </CardContent>
      </Card>
    </main>
  );
}

/** 현재 라우트에 맞는 페이지 제목을 계산한다. */
function usePageTitle(snapshot: DashboardSnapshot | null): string {
  const { t } = useTranslation();
  const { pathname } = useLocation();

  return useMemo(() => {
    if (pathname.startsWith("/platform/") && snapshot) {
      const agentId = decodeURIComponent(pathname.slice("/platform/".length));
      return (
        snapshot.platforms.find((platform) => platform.id === agentId)?.displayName ??
        agentId
      );
    }
    if (pathname.startsWith("/central")) return t("sidebar.centralSkills");
    if (pathname.startsWith("/discover")) return t("sidebar.discovered");
    if (pathname.startsWith("/marketplace")) return t("marketplace.title");
    if (pathname.startsWith("/collections")) return t("sidebar.collections");
    if (pathname.startsWith("/settings")) return t("sidebar.settings");
    return t("webDashboard.overview");
  }, [pathname, snapshot, t]);
}

function PlatformRouteWrapper({ snapshot }: { snapshot: DashboardSnapshot }) {
  // useParams 로 감싸서 플랫폼 전환 시 페이지가 확실히 다시 그려지게 한다.
  const { agentId } = useParams();
  return <PlatformPage key={agentId} snapshot={snapshot} />;
}

export function WebDashboardApp() {
  const { t, i18n } = useTranslation();
  const [snapshot, setSnapshot] = useState<DashboardSnapshot | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);

  // 데스크톱 앱 설정에 저장된 언어가 있으면 웹 대시보드에도 적용한다.
  useEffect(() => {
    if (snapshot?.language && i18n.language !== snapshot.language) {
      const stored = window.localStorage.getItem("i18nextLng");
      if (!stored) void i18n.changeLanguage(snapshot.language);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [snapshot?.language]);

  const loadSnapshot = useCallback(async (signal?: AbortSignal) => {
    setIsRefreshing(true);
    setError(null);
    try {
      setSnapshot(await requestDashboardSnapshot(signal));
    } catch (loadError) {
      if (loadError instanceof DOMException && loadError.name === "AbortError") return;
      setError(loadError instanceof Error ? loadError.message : String(loadError));
    } finally {
      if (!signal?.aborted) setIsRefreshing(false);
    }
  }, []);

  useEffect(() => {
    const controller = new AbortController();
    void loadSnapshot(controller.signal);
    return () => controller.abort();
  }, [loadSnapshot]);

  const pageTitle = usePageTitle(snapshot);
  useEffect(() => {
    document.title = `${pageTitle} · ${t("webDashboard.title")}`;
  }, [pageTitle, t]);

  if (!snapshot && !error) return <LoadingShell />;
  if (!snapshot && error) {
    return (
      <ErrorScreen
        error={error}
        isRefreshing={isRefreshing}
        onRetry={() => void loadSnapshot()}
      />
    );
  }
  if (!snapshot) return null;

  const locale = i18n.resolvedLanguage ?? i18n.language;
  const updatedAt = new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "medium",
  }).format(new Date(snapshot.generatedAt));

  return (
    <div className="flex h-screen bg-background text-foreground">
      <DashboardSidebar snapshot={snapshot} />

      <div className="flex min-w-0 flex-1 flex-col">
        <DashboardHeader title={pageTitle}>
          <span
            className="hidden items-center gap-1.5 rounded-full bg-primary/12 px-2.5 py-1 text-xs font-medium text-primary ring-1 ring-primary/20 sm:inline-flex"
            title={t("webDashboard.readOnlyHint")}
          >
            <ShieldCheckIcon className="size-3.5" aria-hidden="true" />
            {t("webDashboard.readOnly")}
          </span>
          <span className="hidden text-xs text-muted-foreground md:inline">
            {t("webDashboard.lastUpdated", { time: updatedAt })}
          </span>
          <Button
            variant="outline"
            size="sm"
            onClick={() => void loadSnapshot()}
            disabled={isRefreshing}
            aria-label={t("webDashboard.refresh")}
          >
            <RefreshCwIcon
              className={cn("size-4", isRefreshing && "animate-spin")}
            />
            <span className="hidden sm:inline">{t("webDashboard.refresh")}</span>
          </Button>
        </DashboardHeader>

        <main className="min-h-0 flex-1 overflow-y-auto p-4 sm:p-6">
          {error && (
            <div className="mb-4 rounded-lg bg-destructive/10 px-3 py-2 text-xs text-destructive ring-1 ring-destructive/20">
              {error}
            </div>
          )}
          <Routes>
            <Route index element={<Navigate to="/central" replace />} />
            <Route path="/overview" element={<OverviewPage snapshot={snapshot} />} />
            <Route path="/central" element={<LibraryPage snapshot={snapshot} />} />
            <Route
              path="/platform/:agentId"
              element={<PlatformRouteWrapper snapshot={snapshot} />}
            />
            <Route path="/discover" element={<DiscoverPage snapshot={snapshot} />} />
            <Route
              path="/marketplace"
              element={<MarketplacePage snapshot={snapshot} />}
            />
            <Route
              path="/collections"
              element={<CollectionsPage snapshot={snapshot} />}
            />
            <Route path="/settings" element={<SettingsPage snapshot={snapshot} />} />
            <Route path="*" element={<Navigate to="/central" replace />} />
          </Routes>
        </main>
      </div>
    </div>
  );
}

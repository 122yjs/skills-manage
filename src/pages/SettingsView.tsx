import { useEffect, useMemo, useRef, useState } from "react";
import { Plus, Trash2, Pencil, Loader2, FolderOpen, Cpu, Info, Database, Globe, Palette, Droplets, Bot, ChevronDown, ChevronRight, KeyRound, Eye, EyeOff, Check, RefreshCw, Wrench } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { useTranslation } from "react-i18next";
import i18n from "@/i18n";

import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  CardDescription,
} from "@/components/ui/card";
import { InlineConfirmAction } from "@/components/ui/inline-confirm-action";
import { Switch } from "@/components/ui/switch";
import { useSettingsStore } from "@/stores/settingsStore";
import { useThemeStore, CatppuccinFlavor, CatppuccinAccent, ACCENT_NAMES } from "@/stores/themeStore";
import { usePlatformStore } from "@/stores/platformStore";
import { AddDirectoryDialog } from "@/components/settings/AddDirectoryDialog";
import { PlatformDialog } from "@/components/settings/PlatformDialog";
import { Input } from "@/components/ui/input";
import { AgentWithStatus, ScanDirectory } from "@/types";
import { AI_PROVIDERS, PROVIDER_GROUPS, RegionId, ApiProtocol, API_PROTOCOLS } from "@/data/aiProviders";
import { deriveHomeDir, formatPathForDisplay, joinPathForDisplay } from "@/lib/path";
import { CentralVaultSettings } from "@/components/settings/CentralVaultSettings";
import { useDevToolSetupStore } from "@/stores/devToolSetupStore";

// ─── App constants ────────────────────────────────────────────────────────────

const APP_VERSION = "0.9.1";
const DB_PATH_FALLBACK = "~/.skillsmanage/db.sqlite";
const CUSTOM_MODEL_VALUE = "__custom_model__";

/** Catppuccin Lavender hex per flavor — used for visual preview dots on flavor buttons (default accent). */
const FLAVOR_COLORS: Record<CatppuccinFlavor, string> = {
  mocha: "#b4befe",
  macchiato: "#b7bdf8",
  frappe: "#babbf1",
  latte: "#7287fd",
};

/**
 * Mapping of accent name → CSS custom property name.
 * These are resolved at runtime via getComputedStyle to show the
 * actual color for the current flavor.
 */
const CTP_VAR_MAP: Record<CatppuccinAccent, string> = {
  rosewater: "--ctp-rosewater",
  flamingo: "--ctp-flamingo",
  pink: "--ctp-pink",
  mauve: "--ctp-mauve",
  red: "--ctp-red",
  maroon: "--ctp-maroon",
  peach: "--ctp-peach",
  yellow: "--ctp-yellow",
  green: "--ctp-green",
  teal: "--ctp-teal",
  sky: "--ctp-sky",
  sapphire: "--ctp-sapphire",
  blue: "--ctp-blue",
  lavender: "--ctp-lavender",
};

const FLAVOR_ORDER: CatppuccinFlavor[] = ["mocha", "macchiato", "frappe", "latte"];

/** Expand a raw custom base URL to its full endpoint based on protocol. */
function resolveCustomUrl(rawUrl: string, protocol: ApiProtocol | ""): string {
  const trimmed = rawUrl.trim();
  if (!trimmed || !trimmed.endsWith("/v1")) return trimmed;
  switch (protocol) {
    case "openai":
      return trimmed + "/chat/completions";
    case "anthropic":
      return trimmed + "/messages";
    default:
      return trimmed;
  }
}

// ─── ScanDirectoryRow ─────────────────────────────────────────────────────────

interface ScanDirectoryRowProps {
  dir: ScanDirectory;
  onRemove: () => void;
  onToggle: (active: boolean) => void;
  isRemoving: boolean;
}

function ScanDirectoryRow({ dir, onRemove, onToggle, isRemoving }: ScanDirectoryRowProps) {
  const { t } = useTranslation();
  const action = dir.is_active ? t("settings.enabled") : t("settings.disabled");
  return (
    <div className="flex items-center gap-3 py-2.5 px-4 border-b border-border/50 last:border-0">
      <FolderOpen className="size-4 text-muted-foreground shrink-0" />
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium truncate">{formatPathForDisplay(dir.path)}</div>
        {dir.label && (
          <div className="text-xs text-muted-foreground mt-0.5">{dir.label}</div>
        )}
        {dir.is_builtin && (
          <div className="text-xs text-muted-foreground mt-0.5">{t("settings.builtinDir")}</div>
        )}
      </div>
      <div className="flex items-center gap-2 shrink-0">
        {/* Toggle for non-builtin dirs (built-in dirs are always active) */}
        {!dir.is_builtin && (
          <div className="flex items-center gap-1.5">
            <span className="text-xs text-muted-foreground">
              {action}
            </span>
            <Switch
              checked={dir.is_active}
              onCheckedChange={onToggle}
              aria-label={t("settings.enableDirLabel", { action, path: dir.path })}
            />
          </div>
        )}
        {/* Remove button for non-builtin dirs */}
        {!dir.is_builtin && (
          <InlineConfirmAction
            onConfirm={onRemove}
            isLoading={isRemoving}
            idleAriaLabel={t("settings.removeDirLabel", { path: dir.path })}
            idleTitle={t("settings.removeDirLabel", { path: dir.path })}
            confirmLabel={t("common.confirmDelete")}
            icon={<Trash2 className="size-3.5" />}
          />
        )}
      </div>
    </div>
  );
}

// ─── CustomPlatformRow ────────────────────────────────────────────────────────

interface CustomPlatformRowProps {
  agent: AgentWithStatus;
  onEdit: () => void;
  onRemove: () => void;
  isRemoving: boolean;
}

function CustomPlatformRow({ agent, onEdit, onRemove, isRemoving }: CustomPlatformRowProps) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center gap-3 py-2.5 px-4 border-b border-border/50 last:border-0">
      <Cpu className="size-4 text-muted-foreground shrink-0" />
      <div className="flex-1 min-w-0">
        <div className="text-sm font-medium truncate">{agent.display_name}</div>
        <div className="text-xs text-muted-foreground truncate mt-0.5">
          {formatPathForDisplay(agent.global_skills_dir)}
        </div>
      </div>
      <div className="flex items-center gap-2 shrink-0">
        <Button
          variant="outline"
          size="sm"
          onClick={onEdit}
          aria-label={t("settings.editPlatformLabel", { name: agent.display_name })}
        >
          <Pencil className="size-3.5" />
          <span>{t("common.edit")}</span>
        </Button>
        <InlineConfirmAction
          onConfirm={onRemove}
          isLoading={isRemoving}
          idleAriaLabel={t("settings.removePlatformLabel", { name: agent.display_name })}
          idleTitle={t("settings.removePlatformLabel", { name: agent.display_name })}
          confirmLabel={t("common.confirmDelete")}
          icon={<Trash2 className="size-3.5" />}
        />
      </div>
    </div>
  );
}

// ─── SettingsView ─────────────────────────────────────────────────────────────

export function SettingsView() {
  const { t } = useTranslation();
  const openDevToolEditor = useDevToolSetupStore((state) => state.openEditor);
  const activeLanguage = i18n.resolvedLanguage ?? i18n.language.split("-")[0];

  // ── Store State ────────────────────────────────────────────────────────────

  const scanDirectories = useSettingsStore((s) => s.scanDirectories);
  const isLoadingScanDirs = useSettingsStore((s) => s.isLoadingScanDirs);
  const loadScanDirectories = useSettingsStore((s) => s.loadScanDirectories);
  const addScanDirectory = useSettingsStore((s) => s.addScanDirectory);
  const removeScanDirectory = useSettingsStore((s) => s.removeScanDirectory);
  const toggleScanDirectory = useSettingsStore((s) => s.toggleScanDirectory);
  const addCustomAgent = useSettingsStore((s) => s.addCustomAgent);
  const updateCustomAgent = useSettingsStore((s) => s.updateCustomAgent);
  const removeCustomAgent = useSettingsStore((s) => s.removeCustomAgent);
  const githubPat = useSettingsStore((s) => s.githubPat);
  const isLoadingGitHubPat = useSettingsStore((s) => s.isLoadingGitHubPat);
  const isSavingGitHubPat = useSettingsStore((s) => s.isSavingGitHubPat);
  const loadGitHubPat = useSettingsStore((s) => s.loadGitHubPat);
  const saveGitHubPat = useSettingsStore((s) => s.saveGitHubPat);
  const clearGitHubPat = useSettingsStore((s) => s.clearGitHubPat);

  const agents = usePlatformStore((s) => s.agents);

  const flavor = useThemeStore((s) => s.flavor);
  const setFlavor = useThemeStore((s) => s.setFlavor);
  const accent = useThemeStore((s) => s.accent);
  const setAccent = useThemeStore((s) => s.setAccent);
  const rescan = usePlatformStore((s) => s.rescan);
  const refreshCounts = usePlatformStore((s) => s.refreshCounts);

  // Custom agents are those that are not built-in.
  const customAgents = agents.filter((a) => !a.is_builtin);
  const homeDir = useMemo(() => {
    const candidates = [
      agents.find((agent) => agent.id === "central")?.global_skills_dir,
      ...scanDirectories.map((dir) => dir.path),
      ...agents.map((agent) => agent.global_skills_dir),
    ].filter((candidate): candidate is string => Boolean(candidate));

    return candidates
      .map((candidate) => deriveHomeDir(candidate))
      .find((candidate): candidate is string => Boolean(candidate));
  }, [agents, scanDirectories]);
  const dbPathDisplay = useMemo(
    () => (homeDir ? joinPathForDisplay(homeDir, ".skillsmanage/db.sqlite") : DB_PATH_FALLBACK),
    [homeDir]
  );

  // ── Local State ────────────────────────────────────────────────────────────

  // AI Provider state
  const [aiProvider, setAiProvider] = useState("claude");
  const [aiRegion, setAiRegion] = useState<RegionId>("intl");
  const [aiApiKey, setAiApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [aiModel, setAiModel] = useState("");
  const [aiCustomUrl, setAiCustomUrl] = useState("");
  const [aiProtocol, setAiProtocol] = useState<ApiProtocol | "">("");
  const [aiLoaded, setAiLoaded] = useState(false);
  const [hasUserInteracted, setHasUserInteracted] = useState(false);
  const [providerLoading, setProviderLoading] = useState(false);
  const [modelOptions, setModelOptions] = useState<string[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [modelsSource, setModelsSource] = useState<"live" | "cache" | "catalog" | "fallback" | null>(null);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [aiTesting, setAiTesting] = useState(false);
  const [aiTestResult, setAiTestResult] = useState<{ ok: boolean; msg: string; details?: string } | null>(null);
  const [showAiTestDetails, setShowAiTestDetails] = useState(false);
  // 진행 중인 모델 목록 요청 세대 (오래된 응답 무시)
  const modelsRequestIdRef = useRef(0);

  /**
   * 프리셋 URL/프로토콜 초깃값 결정.
   * - 저장된 URL이 있으면 유지 (사용자가 수정한 값)
   * - 없거나 OpenRouter 구형 엔드포인트면 레지스트리 기본 URL로 채움
   */
  async function syncPresetEndpoint(
    providerId: string,
    region: RegionId,
    savedProtocol: string | null
  ): Promise<{ protocol: ApiProtocol | ""; url: string }> {
    const p = AI_PROVIDERS.find((x) => x.id === providerId);
    if (!p || providerId === "custom") {
      return {
        protocol: (savedProtocol as ApiProtocol | "") || "",
        url: "",
      };
    }
    const useRegion = p.regions.includes(region) ? region : p.regions[0];
    const registryUrl = p.endpoints[useRegion] ?? "";
    const savedUrl = await invoke<string | null>("get_setting", { key: `ai_api_url__${providerId}` });

    // OpenRouter: 예전 Anthropic 경로 → 새 OpenAI chat/completions 로 마이그레이션
    const needsOpenRouterMigrate =
      providerId === "openrouter" &&
      !!savedUrl &&
      savedUrl.includes("openrouter.ai") &&
      !savedUrl.includes("/chat/completions");

    const nextUrl =
      !savedUrl || needsOpenRouterMigrate ? registryUrl : savedUrl;

    const nextProtocol: ApiProtocol | "" = needsOpenRouterMigrate
      ? p.protocol
      : savedProtocol
        ? (savedProtocol as ApiProtocol | "")
        : p.protocol;

    if (nextUrl && nextUrl !== savedUrl) {
      await invoke("set_setting", { key: `ai_api_url__${providerId}`, value: nextUrl });
    }
    if (needsOpenRouterMigrate || !savedProtocol) {
      await invoke("set_setting", { key: `ai_protocol__${providerId}`, value: nextProtocol });
    }
    return { protocol: nextProtocol, url: nextUrl || registryUrl };
  }

  // Load AI settings on mount
  useEffect(() => {
    (async () => {
      try {
        const provider = await invoke<string | null>("get_setting", { key: "ai_provider" });
        const region = await invoke<string | null>("get_setting", { key: "ai_region" });
        const nextRegion = (region as RegionId) || "intl";
        if (provider) setAiProvider(provider);
        if (region) setAiRegion(region as RegionId);
        if (provider) {
          const key = await invoke<string | null>("get_setting", { key: `ai_api_key__${provider}` });
          const model = await invoke<string | null>("get_setting", { key: `ai_model__${provider}` });
          const baseUrl = await invoke<string | null>("get_setting", { key: `ai_custom_base_url__${provider}` });
          const protocol = await invoke<string | null>("get_setting", { key: `ai_protocol__${provider}` });
          if (key) setAiApiKey(key);
          if (model) setAiModel(model);
          else {
            const p = AI_PROVIDERS.find((x) => x.id === provider);
            if (p) setAiModel(p.defaultModel);
          }
          if (provider === "custom") {
            setAiProtocol(protocol ? (protocol as ApiProtocol | "") : "");
            setAiCustomUrl(baseUrl ?? "");
          } else {
            const synced = await syncPresetEndpoint(provider, nextRegion, protocol);
            setAiProtocol(synced.protocol);
            // 입력칸에 기본 URL이 미리 보이도록 채움
            setAiCustomUrl(synced.url || baseUrl || "");
          }
        }
      } catch { /* first run, no settings yet */ }
      setAiLoaded(true);
    })();
  }, []);

  // Save AI settings when changed (only after user interaction)
  useEffect(() => {
    if (!aiLoaded || !hasUserInteracted) return;
    const save = async () => {
      try {
        await invoke("set_setting", { key: "ai_provider", value: aiProvider });
        await invoke("set_setting", { key: "ai_region", value: aiRegion });
        await invoke("set_setting", { key: `ai_api_key__${aiProvider}`, value: aiApiKey });
        await invoke("set_setting", { key: `ai_model__${aiProvider}`, value: aiModel });
        const url = resolveCustomUrl(aiCustomUrl, aiProtocol);
        await invoke("set_setting", { key: `ai_api_url__${aiProvider}`, value: url });
        await invoke("set_setting", { key: `ai_custom_base_url__${aiProvider}`, value: aiCustomUrl });
        await invoke("set_setting", { key: `ai_protocol__${aiProvider}`, value: aiProtocol });
      } catch { /* ignore */ }
    };
    save();
  }, [aiProvider, aiRegion, aiApiKey, aiModel, aiCustomUrl, aiProtocol, aiLoaded, hasUserInteracted]);

  // When provider or region changes, update model to default
  async function handleProviderChange(id: string) {
    if (providerLoading) return;
    setProviderLoading(true);
    setAiLoaded(false);           // block save until new config is loaded
    setAiProvider(id);
    setAiTestResult(null);
    const p = AI_PROVIDERS.find((x) => x.id === id);
    let nextRegion = aiRegion;
    if (p) {
      if (!p.regions.includes(aiRegion)) {
        nextRegion = p.regions[0];
        setAiRegion(nextRegion);
      }
    }
    try {
      const key = await invoke<string | null>("get_setting", { key: `ai_api_key__${id}` });
      const model = await invoke<string | null>("get_setting", { key: `ai_model__${id}` });
      const protocol = await invoke<string | null>("get_setting", { key: `ai_protocol__${id}` });
      const baseUrl = await invoke<string | null>("get_setting", { key: `ai_custom_base_url__${id}` });
      setAiApiKey(key ?? "");
      setAiModel(model ?? p?.defaultModel ?? "");
      if (id === "custom") {
        setAiProtocol(protocol ? (protocol as ApiProtocol | "") : "");
        setAiCustomUrl(baseUrl ?? "");
      } else {
        const synced = await syncPresetEndpoint(id, nextRegion, protocol);
        setAiProtocol(synced.protocol);
        setAiCustomUrl(synced.url || baseUrl || "");
      }
      setModelOptions([]);
      setModelsSource(null);
      setModelsError(null);
    } catch {
      setAiApiKey("");
      setAiModel(p?.defaultModel ?? "");
      setAiCustomUrl(p?.endpoints[nextRegion] ?? "");
      setAiProtocol(p?.protocol ?? "");
    } finally {
      setAiLoaded(true);
      setProviderLoading(false);
    }
  }

  /** 지역 변경 시: URL이 비었거나 기존 지역 기본값과 같으면 새 지역 URL로 교체 */
  function handleRegionChange(r: RegionId) {
    setHasUserInteracted(true);
    const provider = AI_PROVIDERS.find((x) => x.id === aiProvider);
    const prevEndpoints = provider
      ? Object.values(provider.endpoints).filter(Boolean)
      : [];
    const shouldReplaceUrl =
      !aiCustomUrl.trim() || prevEndpoints.includes(aiCustomUrl.trim());
    setAiRegion(r);
    if (shouldReplaceUrl && provider?.endpoints[r]) {
      setAiCustomUrl(provider.endpoints[r] ?? "");
    }
  }

  const currentProvider = AI_PROVIDERS.find((p) => p.id === aiProvider);
  const registryUrl = currentProvider?.endpoints[aiRegion] ?? "";
  // 입력칸 값 우선, 비어 있으면 레지스트리 기본 URL
  const resolvedUrl = resolveCustomUrl(aiCustomUrl.trim() || registryUrl, aiProtocol);
  // 화면에서 고른 프로토콜을 Test/모델목록에도 그대로 사용
  const resolvedProtocol: ApiProtocol | "" = aiProtocol;
  // URL을 레지스트리 기본값으로 쓰는 경우에만 전용 models URL 사용 (아니면 백엔드가 chat URL에서 유추)
  const urlMatchesRegistry =
    aiProvider !== "custom" &&
    (!aiCustomUrl.trim() ||
      aiCustomUrl.trim() === registryUrl ||
      resolveCustomUrl(aiCustomUrl, aiProtocol) === registryUrl);
  const resolvedModelsUrl = urlMatchesRegistry
    ? (currentProvider?.modelsUrls?.[aiRegion] ?? currentProvider?.modelsUrls?.intl ?? undefined)
    : undefined;
  const modelsRequireApiKey = currentProvider?.modelsRequireApiKey ?? true;
  const catalogProviderId =
    aiProvider === "custom"
      ? undefined
      : (currentProvider?.catalogIds?.[aiRegion] ?? currentProvider?.catalogIds?.intl ?? undefined);
  // 공개 models.dev 카탈로그가 있으면 API 키 없이도 대표 모델 목록을 조회할 수 있다.
  const canUsePublicCatalog = !!catalogProviderId;
  const providerModelOptions =
    currentProvider?.models ?? (currentProvider?.defaultModel ? [currentProvider.defaultModel] : []);
  const selectableModelOptions = modelOptions.length > 0 ? modelOptions : providerModelOptions;
  const modelSelectValue = selectableModelOptions.includes(aiModel) ? aiModel : CUSTOM_MODEL_VALUE;
  const isCustomModelSelected = modelSelectValue === CUSTOM_MODEL_VALUE;

  // Clear stale test result when config changes
  useEffect(() => {
    setAiTestResult(null);
  }, [aiApiKey, aiCustomUrl, aiProtocol, aiRegion, aiModel]);

  async function loadAiModels(showToast = false, forceRefresh = false) {
    // OpenRouter: 카탈로그가 너무 커서 라이브 목록을 가져오지 않음
    if (aiProvider === "openrouter") {
      setModelOptions(providerModelOptions);
      setModelsSource("fallback");
      setModelsError(t("settings.aiModelOpenRouterSkip"));
      if (showToast) toast.message(t("settings.aiModelOpenRouterSkip"));
      return;
    }
    if (modelsRequireApiKey && !aiApiKey.trim()) {
      if (!canUsePublicCatalog) {
        setModelsError(t("settings.aiModelNeedKey"));
        if (showToast) toast.error(t("settings.aiModelNeedKey"));
        return;
      }
    }
    if (!resolvedUrl.trim()) {
      setModelsError(t("settings.aiTestEnterUrl"));
      if (showToast) toast.error(t("settings.aiTestEnterUrl"));
      return;
    }

    const requestId = ++modelsRequestIdRef.current;
    setModelsLoading(true);
    setModelsError(null);
    if (showToast) toast.message(t("settings.aiModelLoading"));

    try {
      // IPC/네트워크가 멈추면 버튼이 영구 잠기지 않도록 프론트 타임아웃
      const result = await Promise.race([
        invoke<{
          models: string[];
          source: string;
          error?: string | null;
        }>("list_ai_models", {
          request: {
            apiKey: aiApiKey,
            apiUrl: resolvedUrl,
            protocol: resolvedProtocol || null,
            modelsUrl: resolvedModelsUrl ?? null,
            fallbackModel: aiModel || currentProvider?.defaultModel || null,
            forceRefresh,
            catalogProviderId: catalogProviderId ?? null,
          },
        }),
        new Promise<never>((_, reject) => {
          window.setTimeout(() => reject(new Error(t("settings.aiModelTimeout"))), 25000);
        }),
      ]);

      if (requestId !== modelsRequestIdRef.current) return;

      setModelOptions([...new Set([...result.models, ...providerModelOptions])]);
      setModelsSource(
        result.source === "live" || result.source === "cache" || result.source === "catalog" || result.source === "fallback"
          ? result.source
          : "fallback"
      );
      if (result.error) {
        setModelsError(result.error);
        if (showToast) toast.error(result.error);
      } else if (showToast) {
        toast.success(t("settings.aiModelLive", { count: result.models.length }));
      }
    } catch (err) {
      if (requestId !== modelsRequestIdRef.current) return;
      const msg = String(err);
      setModelsError(msg);
      setModelsSource("fallback");
      setModelOptions(providerModelOptions);
      if (showToast) toast.error(msg);
    } finally {
      if (requestId === modelsRequestIdRef.current) {
        setModelsLoading(false);
      }
    }
  }

  // 제공자/지역/URL/프로토콜이 준비되면 한 번 자동 로드 (API 키 타이핑마다 재호출하지 않음)
  useEffect(() => {
    if (!aiLoaded || providerLoading) return;
    if (aiProvider === "openrouter") {
      setModelOptions(providerModelOptions);
      setModelsSource("fallback");
      setModelsError(t("settings.aiModelOpenRouterSkip"));
      return;
    }
    if (!resolvedUrl.trim()) {
      setModelOptions(providerModelOptions);
      setModelsSource(providerModelOptions.length > 0 ? "fallback" : null);
      return;
    }
    if (modelsRequireApiKey && !aiApiKey.trim() && !canUsePublicCatalog) {
      setModelOptions(providerModelOptions);
      setModelsSource(providerModelOptions.length > 0 ? "fallback" : null);
      return;
    }
    const timer = window.setTimeout(() => {
      void loadAiModels(false, false);
    }, 400);
    return () => window.clearTimeout(timer);
    // apiKey는 deps에서 제외 — 입력 중 연속 요청으로 버튼이 잠기는 것 방지
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [aiLoaded, providerLoading, aiProvider, aiRegion, resolvedUrl, resolvedProtocol, resolvedModelsUrl]);

  const canRefreshModels =
    (!modelsRequireApiKey || !!aiApiKey.trim() || canUsePublicCatalog) &&
    !!resolvedUrl.trim() &&
    aiProvider !== "openrouter";

  const [isAddDirOpen, setIsAddDirOpen] = useState(false);
  const [showBuiltinDirs, setShowBuiltinDirs] = useState(false);
  const [isPlatformDialogOpen, setIsPlatformDialogOpen] = useState(false);
  const [editingPlatform, setEditingPlatform] = useState<AgentWithStatus | null>(null);
  const [removingDir, setRemovingDir] = useState<string | null>(null);
  const [removingAgent, setRemovingAgent] = useState<string | null>(null);
  const [scanDirError, setScanDirError] = useState<string | null>(null);
  const [platformError, setPlatformError] = useState<string | null>(null);
  const [githubPatInput, setGitHubPatInput] = useState("");
  const [githubPatMessage, setGitHubPatMessage] = useState<{ type: "success" | "error"; text: string } | null>(null);

  // ── Load on mount ──────────────────────────────────────────────────────────

  useEffect(() => {
    loadScanDirectories();
    loadGitHubPat();
  }, [loadScanDirectories, loadGitHubPat]);

  useEffect(() => {
    setGitHubPatInput(githubPat);
  }, [githubPat]);

  const isGitHubPatDirty = useMemo(() => githubPatInput.trim() !== githubPat, [githubPatInput, githubPat]);

  // ── Scan Directories Handlers ──────────────────────────────────────────────

  async function handleAddDirectory(path: string) {
    setScanDirError(null);
    try {
      await addScanDirectory(path);
      // Trigger rescan after adding a directory.
      await refreshCounts();
      toast.success(t("addDir.add") + " ✓");
    } catch (err) {
      setScanDirError(String(err));
      toast.error(String(err));
      throw err; // Re-throw so the dialog knows it failed
    }
  }

  async function handleRemoveDirectory(path: string) {
    setRemovingDir(path);
    setScanDirError(null);
    try {
      await removeScanDirectory(path);
      // Trigger rescan after removing a directory.
      await refreshCounts();
      toast.success(t("common.delete") + " ✓");
    } catch (err) {
      setScanDirError(String(err));
      toast.error(String(err));
    } finally {
      setRemovingDir(null);
    }
  }

  /**
   * Toggle the active state of a custom scan directory.
   * Persists the change to the backend via set_scan_directory_active command.
   */
  async function handleToggleDirectory(path: string, active: boolean) {
    setScanDirError(null);
    try {
      await toggleScanDirectory(path, active);
    } catch (err) {
      setScanDirError(String(err));
      toast.error(String(err));
    }
  }

  // ── Custom Platform Handlers ───────────────────────────────────────────────

  function handleOpenAddPlatform() {
    setEditingPlatform(null);
    setPlatformError(null);
    setIsPlatformDialogOpen(true);
  }

  function handleOpenEditPlatform(agent: AgentWithStatus) {
    setEditingPlatform(agent);
    setPlatformError(null);
    setIsPlatformDialogOpen(true);
  }

  async function handleAddPlatform(displayName: string, globalSkillsDir: string, category?: string) {
    setPlatformError(null);
    try {
      await addCustomAgent({
        display_name: displayName,
        global_skills_dir: globalSkillsDir,
        category: category || "coding",
      });
      // Refresh agents + rescan to show new platform in sidebar.
      await rescan();
      toast.success(t("platformDialog.add") + " ✓");
    } catch (err) {
      setPlatformError(String(err));
      toast.error(String(err));
      throw err;
    }
  }

  async function handleEditPlatform(displayName: string, globalSkillsDir: string, category?: string) {
    if (!editingPlatform) return;
    setPlatformError(null);
    try {
      await updateCustomAgent(editingPlatform.id, {
        display_name: displayName,
        global_skills_dir: globalSkillsDir,
        category: category || "coding",
      });
      // Refresh agents + rescan.
      await rescan();
      toast.success(t("platformDialog.save") + " ✓");
    } catch (err) {
      setPlatformError(String(err));
      toast.error(String(err));
      throw err;
    }
  }

  async function handleRemovePlatform(agentId: string) {
    setRemovingAgent(agentId);
    setPlatformError(null);
    try {
      await removeCustomAgent(agentId);
      // Refresh agents.
      await rescan();
      toast.success(t("common.delete") + " ✓");
    } catch (err) {
      setPlatformError(String(err));
      toast.error(String(err));
    } finally {
      setRemovingAgent(null);
    }
  }

  async function handleSaveGitHubPat() {
    setGitHubPatMessage(null);
    try {
      await saveGitHubPat(githubPatInput);
      setGitHubPatMessage({
        type: "success",
        text: t("settings.githubPatSaved"),
      });
      toast.success(t("settings.githubPatSaved"));
    } catch (err) {
      const text = String(err);
      setGitHubPatMessage({ type: "error", text });
      toast.error(text);
    }
  }

  async function handleClearGitHubPat() {
    setGitHubPatMessage(null);
    try {
      await clearGitHubPat();
      setGitHubPatInput("");
      setGitHubPatMessage({
        type: "success",
        text: t("settings.githubPatCleared"),
      });
      toast.success(t("settings.githubPatCleared"));
    } catch (err) {
      const text = String(err);
      setGitHubPatMessage({ type: "error", text });
      toast.error(text);
    }
  }

  // ── Render ─────────────────────────────────────────────────────────────────

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="border-b border-border px-6 py-4">
        <h1 className="text-xl font-semibold">{t("settings.title")}</h1>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-auto p-6 space-y-6">

        <CentralVaultSettings />

        {/* ── Section 1: Development tools ─────────────────────────────────── */}
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between gap-4">
              <div>
                <CardTitle className="flex items-center gap-2">
                  <Wrench className="size-4 text-muted-foreground" />
                  {t("devToolSetup.settingsTitle")}
                </CardTitle>
                <CardDescription className="mt-1">
                  {t("devToolSetup.settingsDescription")}
                </CardDescription>
              </div>
              <Button variant="outline" size="sm" onClick={openDevToolEditor}>
                {t("devToolSetup.changeSelection")}
              </Button>
            </div>
          </CardHeader>
        </Card>

        {/* ── Section 2: Custom Platforms ───────────────────────────────────── */}
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle>{t("settings.customPlatforms")}</CardTitle>
                <CardDescription className="mt-1">
                  {t("settings.customPlatformsDesc")}
                </CardDescription>
              </div>
              <Button
                variant="outline"
                size="sm"
                onClick={handleOpenAddPlatform}
                aria-label={t("settings.addPlatformAriaLabel")}
              >
                <Plus className="size-3.5" />
                <span>{t("settings.addPlatform")}</span>
              </Button>
            </div>
          </CardHeader>

          <CardContent>
            {platformError && (
              <p className="text-xs text-destructive mb-3" role="alert">
                {platformError}
              </p>
            )}

            {customAgents.length === 0 ? (
              <p className="text-sm text-muted-foreground py-4 text-center">
                {t("settings.noPlatforms")}
              </p>
            ) : (
              <div className="rounded-lg border border-border overflow-hidden">
                {customAgents.map((agent) => (
                  <CustomPlatformRow
                    key={agent.id}
                    agent={agent}
                    onEdit={() => handleOpenEditPlatform(agent)}
                    onRemove={() => handleRemovePlatform(agent.id)}
                    isRemoving={removingAgent === agent.id}
                  />
                ))}
              </div>
            )}
          </CardContent>
        </Card>

        {/* ── Section 2: GitHub Import Auth ─────────────────────────────── */}
        <Card>
          <CardHeader>
            <div className="flex items-center gap-2">
              <KeyRound className="size-5 text-muted-foreground" />
              <div>
                <CardTitle>{t("settings.githubPatTitle")}</CardTitle>
                <CardDescription className="mt-1">
                  {t("settings.githubPatDesc")}
                </CardDescription>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              <div>
                <label htmlFor="github-pat" className="mb-1 block text-xs text-muted-foreground">
                  {t("settings.githubPatLabel")}
                </label>
                <Input
                  id="github-pat"
                  type="password"
                  placeholder="github_pat_..."
                  value={githubPatInput}
                  onChange={(event) => setGitHubPatInput(event.target.value)}
                  disabled={isLoadingGitHubPat || isSavingGitHubPat}
                />
              </div>

              <div className="rounded-lg border border-border/70 bg-muted/20 p-3 text-sm text-muted-foreground">
                <p>{t("settings.githubPatDirectOnly")}</p>
                <p className="mt-2">{t("settings.githubPatRateLimitHint")}</p>
              </div>

              {githubPatMessage ? (
                <p
                  className={githubPatMessage.type === "error" ? "text-sm text-destructive" : "text-sm text-emerald-600 dark:text-emerald-400"}
                  role="status"
                >
                  {githubPatMessage.text}
                </p>
              ) : null}

              <div className="flex flex-wrap items-center gap-2">
                <Button
                  onClick={handleSaveGitHubPat}
                  disabled={isLoadingGitHubPat || isSavingGitHubPat || !isGitHubPatDirty}
                >
                  {isSavingGitHubPat ? <Loader2 className="size-4 animate-spin" /> : null}
                  <span>{t("common.save")}</span>
                </Button>
                <Button
                  variant="outline"
                  onClick={handleClearGitHubPat}
                  disabled={isLoadingGitHubPat || isSavingGitHubPat || !githubPat}
                >
                  <span>{t("settings.githubPatClear")}</span>
                </Button>
                {isLoadingGitHubPat ? (
                  <span className="text-xs text-muted-foreground">{t("settings.loading")}</span>
                ) : null}
              </div>
            </div>
          </CardContent>
        </Card>

        {/* ── Section 3: AI Provider ─────────────────────────────────────── */}
        <Card>
          <CardHeader>
            <div className="flex items-center gap-2">
              <Bot className="size-5 text-muted-foreground" />
              <div>
                <CardTitle>{t("settings.aiProviderTitle")}</CardTitle>
                <CardDescription className="mt-1">
                  {t("settings.aiProviderDesc")}
                </CardDescription>
              </div>
            </div>
          </CardHeader>
          <CardContent>
            <div className="space-y-4">
              <div className="space-y-3">
                <label className="text-xs text-muted-foreground mb-2 block">{t("settings.aiProviderLabel")}</label>
                {PROVIDER_GROUPS.map((group) => {
                  const providers = AI_PROVIDERS.filter((p) => p.group === group);
                  if (providers.length === 0) return null;
                  return (
                    <div key={group} className="space-y-1.5">
                      <p className="text-[11px] uppercase tracking-wide text-muted-foreground/80">
                        {t(`settings.aiProviderGroup.${group}`)}
                      </p>
                      <div className="flex flex-wrap gap-1.5">
                        {providers.map((p) => (
                          <button
                            key={p.id}
                            disabled={providerLoading}
                            onClick={() => { setHasUserInteracted(true); handleProviderChange(p.id); }}
                            className={`px-3 py-1.5 rounded-md text-xs transition-colors cursor-pointer border ${aiProvider === p.id ? "bg-primary/15 border-primary text-foreground font-medium" : "border-border bg-background text-muted-foreground hover:border-primary/40 hover:bg-hover-bg/10"} ${providerLoading ? "opacity-50 cursor-not-allowed" : ""}`}
                          >
                            {providerLoading && aiProvider === p.id ? <Loader2 className="size-3 animate-spin inline mr-1" /> : null}
                            {t(`settings.aiProvider.${p.id}`)}
                          </button>
                        ))}
                      </div>
                    </div>
                  );
                })}
              </div>
              {currentProvider && currentProvider.regions.length > 1 && (
                <div>
                  <label className="text-xs text-muted-foreground mb-2 block">{t("settings.aiRegionLabel")}</label>
                  <div className="flex gap-1.5">
                    {currentProvider.regions.map((r) => (
                      <button key={r} onClick={() => handleRegionChange(r)} className={`px-3 py-1.5 rounded-md text-xs transition-colors cursor-pointer border ${aiRegion === r ? "bg-primary/15 border-primary text-foreground font-medium" : "border-border bg-background text-muted-foreground hover:border-primary/40"}`}>
                        {t(`settings.aiRegion.${r}`)}
                      </button>
                    ))}
                  </div>
                </div>
              )}
              <div>
                <label className="text-xs text-muted-foreground mb-1 block">{t("settings.aiApiKeyLabel")}</label>
                <div className="relative">
                  <Input
                    type={showKey ? "text" : "password"}
                    placeholder="sk-..."
                    value={aiApiKey}
                    onChange={(e) => { setHasUserInteracted(true); setAiApiKey(e.target.value); }}
                    onBlur={() => {
                      // 키 입력이 끝나면 그때 모델 목록 로드
                      if (
                        (!modelsRequireApiKey || aiApiKey.trim() || canUsePublicCatalog) &&
                        resolvedUrl.trim() &&
                        aiProvider !== "openrouter"
                      ) {
                        void loadAiModels(false, false);
                      }
                    }}
                    className="pr-9"
                  />
                  <button type="button" onClick={() => setShowKey(!showKey)} className="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground">
                    {showKey ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
                  </button>
                </div>
              </div>
              <div>
                <div className="flex items-center justify-between gap-2 mb-1">
                  <label htmlFor="ai-model-select" className="text-xs text-muted-foreground">{t("settings.aiModelLabel")}</label>
                  {/* Base UI Button 대신 native — loading 중 pointer-events-none으로 클릭이 먹통 되는 문제 방지 */}
                  <button
                    type="button"
                    className="inline-flex h-7 items-center gap-1 rounded-md px-2 text-xs text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50"
                    disabled={!canRefreshModels}
                    title={
                      aiProvider === "openrouter"
                        ? t("settings.aiModelOpenRouterSkip")
                        : modelsRequireApiKey && !aiApiKey.trim() && !canUsePublicCatalog
                          ? t("settings.aiModelNeedKey")
                          : !resolvedUrl.trim()
                            ? t("settings.aiTestEnterUrl")
                            : t("settings.aiModelRefresh")
                    }
                    onClick={() => {
                      void loadAiModels(true, true);
                    }}
                  >
                    {modelsLoading ? <Loader2 className="size-3.5 animate-spin" /> : <RefreshCw className="size-3.5" />}
                    <span>{modelsLoading ? t("settings.aiModelLoading") : t("settings.aiModelRefresh")}</span>
                  </button>
                </div>
                <select
                  id="ai-model-select"
                  value={modelSelectValue}
                  onChange={(e) => {
                    setHasUserInteracted(true);
                    setAiModel(e.target.value === CUSTOM_MODEL_VALUE ? "" : e.target.value);
                  }}
                  className="flex h-9 w-full rounded-md border border-input bg-background px-3 py-1 text-sm text-foreground shadow-sm outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50"
                >
                  {selectableModelOptions.map((model) => (
                    <option key={model} value={model}>{model}</option>
                  ))}
                  <option value={CUSTOM_MODEL_VALUE}>{t("settings.aiModelCustom")}</option>
                </select>
                {isCustomModelSelected ? (
                  <Input
                    id="ai-custom-model-input"
                    className="mt-2"
                    aria-label={t("settings.aiModelCustomLabel")}
                    placeholder={t("settings.aiModelCustomPlaceholder")}
                    value={aiModel}
                    onChange={(e) => { setHasUserInteracted(true); setAiModel(e.target.value); }}
                  />
                ) : null}
                {modelsSource === "live" || modelsSource === "cache" || modelsSource === "catalog" ? (
                  <p className="mt-1 text-[11px] text-muted-foreground">
                    {modelsSource === "catalog"
                      ? t("settings.aiModelCatalog", { count: modelOptions.length })
                      : t("settings.aiModelLive", { count: modelOptions.length })}
                  </p>
                ) : null}
                {modelsError ? (
                  <p className="mt-1 text-[11px] text-destructive/90">
                    {modelsError}
                  </p>
                ) : null}
                {!canRefreshModels && aiProvider !== "openrouter" ? (
                  <p className="mt-1 text-[11px] text-muted-foreground">
                    {modelsRequireApiKey && !aiApiKey.trim()
                      ? t("settings.aiModelNeedKey")
                      : t("settings.aiTestEnterUrl")}
                  </p>
                ) : null}
              </div>
              {/* Global/Regional도 기본 프로토콜을 보여주고, 이후 수정 가능 */}
              <div>
                <label className="text-xs text-muted-foreground mb-2 block">{t("settings.aiApiFormatLabel")}</label>
                <div className="flex flex-wrap gap-1.5">
                  {API_PROTOCOLS.map((proto) => (
                    <button
                      key={proto.id || "auto"}
                      type="button"
                      onClick={() => { setHasUserInteracted(true); setAiProtocol(proto.id as ApiProtocol | ""); }}
                      className={`px-3 py-1.5 rounded-md text-xs transition-colors cursor-pointer border ${aiProtocol === proto.id ? "bg-primary/15 border-primary text-foreground font-medium" : "border-border bg-background text-muted-foreground hover:border-primary/40 hover:bg-hover-bg/10"}`}
                    >
                      {t(`settings.aiProtocol.${proto.id || "auto"}`)}
                    </button>
                  ))}
                </div>
                {aiProvider !== "custom" && currentProvider ? (
                  <p className="mt-1.5 text-[11px] text-muted-foreground">
                    {t("settings.aiApiFormatDefaultHint", {
                      format: t(`settings.aiProtocol.${currentProvider.protocol}`),
                    })}
                  </p>
                ) : null}
              </div>
              <div>
                <label className="text-xs text-muted-foreground mb-1 block">{t("settings.aiApiUrlLabel")}</label>
                <Input
                  placeholder={registryUrl || "https://..."}
                  value={aiCustomUrl}
                  onChange={(e) => { setHasUserInteracted(true); setAiCustomUrl(e.target.value); }}
                />
                {aiProvider !== "custom" && registryUrl ? (
                  <p className="mt-1.5 text-[11px] text-muted-foreground">
                    {t("settings.aiApiUrlDefaultHint")}
                  </p>
                ) : null}
              </div>
              <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-2">
                <div className="min-w-0">
                  {aiTestResult?.ok ? (
                    <span className="inline-flex items-center gap-1.5 text-xs text-green-700 dark:text-green-400">
                      <Check className="size-3.5" />
                      {t("settings.aiTestConnectionVerified")}
                    </span>
                  ) : !resolvedUrl ? (
                    <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground">
                      <Info className="size-3.5" />
                      {t("settings.aiTestEnterUrl")}
                    </span>
                  ) : null}
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  disabled={aiTesting || !aiApiKey || !resolvedUrl}
                  onClick={async () => {
                    setAiTesting(true); setAiTestResult(null); setShowAiTestDetails(false);
                    try {
                      // 화면의 현재 URL/키/프로토콜로 테스트 (DB 저장 전에도 일치)
                      await invoke<string>("test_ai_connection", {
                        request: {
                          apiKey: aiApiKey,
                          apiUrl: resolvedUrl,
                          protocol: resolvedProtocol || null,
                          model: aiModel || null,
                        },
                      });
                      setAiTestResult({ ok: true, msg: t("settings.aiTestSuccess") });
                    } catch (err) {
                      const raw = String(err);
                      let msg = raw;
                      let details: string | undefined;
                      const prefix = "API 请求失败: ";
                      if (raw.startsWith(prefix)) {
                        const after = raw.slice(prefix.length);
                        const nlIdx = after.indexOf("\n");
                        if (nlIdx > 0) {
                          msg = after.slice(nlIdx + 1);
                          details = after.slice(0, nlIdx);
                        } else {
                          msg = after;
                        }
                      }
                      setAiTestResult({ ok: false, msg, details });
                    } finally { setAiTesting(false); }
                  }}
                  className="shrink-0"
                >
                  {aiTesting ? <Loader2 className="size-3.5 animate-spin" /> : <Bot className="size-3.5" />}
                  <span>{aiTesting ? t("settings.aiTestTesting") : t("settings.aiTestButton")}</span>
                </Button>
              </div>
              {aiTestResult && !aiTestResult.ok && (
                <div className="rounded-md border border-destructive/20 bg-destructive/5 px-3 py-2 space-y-1.5 text-destructive text-xs">
                  <p>{aiTestResult.msg}</p>
                  {aiTestResult.details && (
                    <div>
                      <button
                        className="flex items-center gap-1 text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
                        onClick={() => setShowAiTestDetails((v) => !v)}
                      >
                        {showAiTestDetails ? <ChevronDown className="size-3" /> : <ChevronRight className="size-3" />}
                        {t("settings.aiTestDetails")}
                      </button>
                      {showAiTestDetails && (
                        <pre className="mt-1 text-[11px] leading-4 font-mono text-muted-foreground whitespace-pre-wrap break-all bg-muted/30 rounded-md p-2 max-h-32 overflow-auto">
                          {aiTestResult.details}
                        </pre>
                      )}
                    </div>
                  )}
                  {currentProvider && currentProvider.regions.length > 1 && (
                    <p className="text-muted-foreground">
                      {t("settings.aiTestRegionTip", { region: t(`settings.aiRegion.${aiRegion}`) })}
                    </p>
                  )}
                </div>
              )}
            </div>
          </CardContent>
        </Card>

        {/* ── Section 4: Scan Directories (compact) ─────────────────────── */}
        <Card>
          <CardHeader>
            <div className="flex items-center justify-between">
              <div>
                <CardTitle>{t("settings.scanDirs")}</CardTitle>
                <CardDescription className="mt-1">{t("settings.scanDirsDesc")}</CardDescription>
              </div>
              <Button variant="outline" size="sm" onClick={() => setIsAddDirOpen(true)} aria-label={t("settings.addDirAriaLabel")}>
                <Plus className="size-3.5" />
                <span>{t("settings.addDirectory")}</span>
              </Button>
            </div>
          </CardHeader>
          <CardContent>
            {scanDirError && <p className="text-xs text-destructive mb-3" role="alert">{scanDirError}</p>}
            {isLoadingScanDirs ? (
              <div className="flex items-center gap-2 py-4 text-muted-foreground text-sm justify-center">
                <Loader2 className="size-4 animate-spin" />
                <span>{t("settings.loading")}</span>
              </div>
            ) : (() => {
              const customDirs = scanDirectories.filter((d) => !d.is_builtin);
              const builtinDirs = scanDirectories.filter((d) => d.is_builtin);
              return (
                <div className="space-y-3">
                  {/* Custom dirs first */}
                  {customDirs.length > 0 && (
                    <div className="rounded-lg border border-border overflow-hidden">
                      {customDirs.map((dir) => (
                        <ScanDirectoryRow key={dir.id} dir={dir} onRemove={() => handleRemoveDirectory(dir.path)} onToggle={(active) => handleToggleDirectory(dir.path, active)} isRemoving={removingDir === dir.path} />
                      ))}
                    </div>
                  )}
                  {customDirs.length === 0 && (
                    <p className="text-xs text-muted-foreground text-center py-2">{t("settings.noDirs")}</p>
                  )}
                  {/* Built-in dirs — collapsible, two-column */}
                  {builtinDirs.length > 0 && (
                    <div>
                      <button
                        onClick={() => setShowBuiltinDirs((v) => !v)}
                        className="flex items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground transition-colors cursor-pointer"
                      >
                        <span>{showBuiltinDirs ? "▾" : "▸"}</span>
                        <span>{t("settings.builtinDir")} ({builtinDirs.length})</span>
                      </button>
                      {showBuiltinDirs && (
                        <div className="grid grid-cols-2 gap-1.5 mt-2">
                          {builtinDirs.map((dir) => (
                            <div key={dir.id} className="flex items-center gap-2 px-2.5 py-1.5 rounded-md bg-muted/30 text-xs text-muted-foreground truncate">
                              <FolderOpen className="size-3 shrink-0" />
                              <span className="truncate">{formatPathForDisplay(dir.path)}</span>
                              {dir.label && <span className="shrink-0 opacity-60">· {dir.label}</span>}
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                  )}
                </div>
              );
            })()}
          </CardContent>
        </Card>

        {/* ── Section 5: About ────────────────────────────────────────────── */}
        <Card>
          <CardHeader>
            <CardTitle>{t("settings.about")}</CardTitle>
          </CardHeader>
          <CardContent>
            <div className="space-y-3">
              <div className="flex items-center gap-3">
                <Info className="size-4 text-muted-foreground shrink-0" />
                <div>
                  <div className="text-xs text-muted-foreground">{t("settings.appVersion")}</div>
                  <div className="text-sm font-medium">skills-manage v{APP_VERSION}</div>
                </div>
              </div>
              <div className="flex items-center gap-3">
                <Database className="size-4 text-muted-foreground shrink-0" />
                <div>
                  <div className="text-xs text-muted-foreground">{t("settings.dbPath")}</div>
                  <div className="text-sm font-medium font-mono">{dbPathDisplay}</div>
                </div>
              </div>
              {/* ── Flavor Switcher ──────────────────────────────────────── */}
              <div className="flex items-center gap-3">
                <Palette className="size-4 text-muted-foreground shrink-0" />
                <div className="flex-1">
                  <div className="text-xs text-muted-foreground mb-1.5">{t("settings.flavor")}</div>
                  <div className="flex gap-2">
                    {FLAVOR_ORDER.map((f) => (
                      <Button
                        key={f}
                        variant={flavor === f ? "default" : "outline"}
                        size="sm"
                        onClick={() => setFlavor(f)}
                        aria-pressed={flavor === f}
                      >
                        <span
                          className="inline-block size-2 rounded-full mr-1.5 shrink-0"
                          style={{ backgroundColor: FLAVOR_COLORS[f] }}
                        />
                        {t(`settings.${f}`)}
                      </Button>
                    ))}
                  </div>
                </div>
              </div>
              {/* ── Accent Color Picker ─────────────────────────────────── */}
              <div className="flex items-center gap-3">
                <Droplets className="size-4 text-muted-foreground shrink-0" />
                <div className="flex-1">
                  <div className="text-xs text-muted-foreground mb-1.5">{t("settings.accentColor")}</div>
                  <div className="flex flex-wrap gap-1.5" role="radiogroup" aria-label={t("settings.accentColor")}>
                    {ACCENT_NAMES.map((name) => {
                      const ctpVar = CTP_VAR_MAP[name];
                      const isActive = accent === name;
                      return (
                        <button
                          key={name}
                          type="button"
                          role="radio"
                          aria-checked={isActive}
                          aria-label={t(`settings.accent.${name}`)}
                          title={t(`settings.accent.${name}`)}
                          onClick={() => setAccent(name)}
                          className={`relative size-6 rounded-full transition-all cursor-pointer
                            ${isActive
                              ? "ring-2 ring-ring ring-offset-2 ring-offset-background scale-110"
                              : "ring-1 ring-border hover:scale-105 hover:ring-2 hover:ring-ring/50"
                            }`}
                          style={{ backgroundColor: `var(${ctpVar})` }}
                        />
                      );
                    })}
                  </div>
                </div>
              </div>
              {/* ── Language Switcher ──────────────────────────────────────── */}
              <div className="flex items-center gap-3">
                <Globe className="size-4 text-muted-foreground shrink-0" />
                <div className="flex-1">
                  <div className="text-xs text-muted-foreground mb-1.5">{t("settings.language")}</div>
                  <div className="flex gap-2">
                    <Button
                      variant={activeLanguage === "ko" ? "default" : "outline"}
                      size="sm"
                      onClick={() => i18n.changeLanguage("ko")}
                      aria-pressed={activeLanguage === "ko"}
                    >
                      {t("settings.korean")}
                    </Button>
                    <Button
                      variant={activeLanguage === "zh" ? "default" : "outline"}
                      size="sm"
                      onClick={() => i18n.changeLanguage("zh")}
                      aria-pressed={activeLanguage === "zh"}
                    >
                      {t("settings.chinese")}
                    </Button>
                    <Button
                      variant={activeLanguage === "en" ? "default" : "outline"}
                      size="sm"
                      onClick={() => i18n.changeLanguage("en")}
                      aria-pressed={activeLanguage === "en"}
                    >
                      {t("settings.english")}
                    </Button>
                  </div>
                </div>
              </div>
            </div>
          </CardContent>
        </Card>
      </div>

      {/* ── Dialogs ────────────────────────────────────────────────────────── */}
      <AddDirectoryDialog
        open={isAddDirOpen}
        onOpenChange={setIsAddDirOpen}
        onAdd={handleAddDirectory}
      />

      <PlatformDialog
        open={isPlatformDialogOpen}
        onOpenChange={setIsPlatformDialogOpen}
        platform={editingPlatform}
        onAdd={handleAddPlatform}
        onEdit={handleEditPlatform}
      />
    </div>
  );
}

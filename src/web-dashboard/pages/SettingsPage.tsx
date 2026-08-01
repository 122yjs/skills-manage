import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";
import {
  BookOpenIcon,
  BotIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  DatabaseIcon,
  FolderCogIcon,
  FolderSearchIcon,
  InfoIcon,
  KeyRoundIcon,
  MonitorCogIcon,
  PaletteIcon,
  ShieldCheckIcon,
} from "lucide-react";

import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { cn } from "@/lib/utils";
import {
  ACCENT_NAMES,
  type CatppuccinAccent,
  type CatppuccinFlavor,
  useThemeStore,
} from "@/stores/themeStore";

import type { DashboardScanDirectory, DashboardSnapshot } from "../types";

const LANGUAGE_OPTIONS = [
  { code: "ko", labelKey: "settings.korean" },
  { code: "en", labelKey: "settings.english" },
  { code: "zh", labelKey: "settings.chinese" },
] as const;

const FLAVOR_ORDER: CatppuccinFlavor[] = [
  "mocha",
  "macchiato",
  "frappe",
  "latte",
];

const FLAVOR_COLORS: Record<CatppuccinFlavor, string> = {
  mocha: "#b4befe",
  macchiato: "#b7bdf8",
  frappe: "#babbf1",
  latte: "#7287fd",
};

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

function StatusBadge({ active, label }: { active: boolean; label: string }) {
  return (
    <span
      className={cn(
        "inline-flex shrink-0 items-center gap-1.5 rounded-full px-2.5 py-1 text-xs ring-1",
        active
          ? "bg-[color-mix(in_srgb,var(--ctp-green)_12%,transparent)] text-[var(--ctp-green)] ring-[color-mix(in_srgb,var(--ctp-green)_30%,transparent)]"
          : "bg-muted text-muted-foreground ring-border",
      )}
    >
      <span
        className={cn(
          "size-1.5 rounded-full",
          active ? "bg-[var(--ctp-green)]" : "bg-muted-foreground/50",
        )}
        aria-hidden="true"
      />
      {label}
    </span>
  );
}

function SettingValue({ label, value }: { label: string; value: string | null }) {
  return (
    <div className="min-w-0 rounded-lg bg-muted/25 px-3 py-2 ring-1 ring-border/70">
      <p className="text-[11px] text-muted-foreground">{label}</p>
      <p className="mt-1 truncate font-mono text-xs" title={value ?? undefined}>
        {value || "—"}
      </p>
    </div>
  );
}

function DirectoryRow({ directory }: { directory: DashboardScanDirectory }) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center justify-between gap-3 border-b border-border/60 px-3 py-2 last:border-b-0">
      <div className="min-w-0">
        <p className="truncate text-sm">{directory.label || directory.path}</p>
        {directory.label && (
          <p className="truncate font-mono text-[11px] text-muted-foreground">
            {directory.path}
          </p>
        )}
      </div>
      <StatusBadge
        active={directory.isActive}
        label={
          directory.isActive
            ? t("webDashboard.settingsView.active")
            : t("webDashboard.settingsView.inactive")
        }
      />
    </div>
  );
}

/** 데스크톱 앱의 설정 구조를 웹에서 안전하게 확인하는 읽기 전용 화면. */
export function SettingsPage({ snapshot }: { snapshot: DashboardSnapshot }) {
  const { t, i18n } = useTranslation();
  const navigate = useNavigate();
  const [showBuiltinDirectories, setShowBuiltinDirectories] = useState(false);
  const flavor = useThemeStore((state) => state.flavor);
  const setFlavor = useThemeStore((state) => state.setFlavor);
  const accent = useThemeStore((state) => state.accent);
  const setAccent = useThemeStore((state) => state.setAccent);
  const currentLanguage = i18n.resolvedLanguage ?? i18n.language;
  const { settings } = snapshot;

  const customDirectories = settings.scanDirectories.filter(
    (directory) => !directory.isBuiltin,
  );
  const builtinDirectories = settings.scanDirectories.filter(
    (directory) => directory.isBuiltin,
  );

  function handleLanguageChange(code: string) {
    void i18n.changeLanguage(code);
  }

  return (
    <div className="mx-auto max-w-4xl space-y-4 pb-8">
      <section className="flex items-start gap-3 rounded-xl bg-primary/8 p-4 ring-1 ring-primary/20">
        <ShieldCheckIcon className="mt-0.5 size-5 shrink-0 text-primary" aria-hidden="true" />
        <div>
          <h2 className="text-sm font-semibold">
            {t("webDashboard.settingsView.readOnlyTitle")}
          </h2>
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {t("webDashboard.settingsView.readOnlyDesc")}
          </p>
        </div>
      </section>

      <Card>
        <CardHeader className="pb-2">
          <div className="flex items-center gap-2">
            <FolderCogIcon className="size-5 text-primary" aria-hidden="true" />
            <div>
              <CardTitle className="text-base">{t("storage.title")}</CardTitle>
              <CardDescription className="mt-1">
                {t("storage.description")}
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="grid gap-3 sm:grid-cols-2">
          <SettingValue label={t("storage.pathLabel")} value={settings.centralSkillsPath} />
          <SettingValue
            label={t("webDashboard.settingsView.migrationState")}
            value={settings.migrationState}
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="pb-2">
          <div className="flex items-center gap-2">
            <MonitorCogIcon className="size-5 text-primary" aria-hidden="true" />
            <div>
              <CardTitle className="text-base">{t("settings.customPlatforms")}</CardTitle>
              <CardDescription className="mt-1">
                {t("settings.customPlatformsDesc")}
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent>
          {settings.customPlatforms.length === 0 ? (
            <p className="py-3 text-center text-sm text-muted-foreground">
              {t("webDashboard.settingsView.customPlatformsEmpty")}
            </p>
          ) : (
            <div className="overflow-hidden rounded-lg border border-border">
              {settings.customPlatforms.map((platform) => (
                <div
                  key={platform.id}
                  className="flex items-center justify-between gap-3 border-b border-border/60 px-3 py-2 last:border-b-0"
                >
                  <div className="min-w-0">
                    <p className="truncate text-sm font-medium">{platform.displayName}</p>
                    <p className="truncate font-mono text-[11px] text-muted-foreground">
                      {platform.globalSkillsDir}
                    </p>
                  </div>
                  <StatusBadge
                    active={platform.isEnabled}
                    label={
                      platform.isEnabled
                        ? t("webDashboard.settingsView.active")
                        : t("webDashboard.settingsView.inactive")
                    }
                  />
                </div>
              ))}
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="pb-2">
          <div className="flex items-center gap-2">
            <KeyRoundIcon className="size-5 text-primary" aria-hidden="true" />
            <div>
              <CardTitle className="text-base">{t("settings.githubPatTitle")}</CardTitle>
              <CardDescription className="mt-1">
                {t("settings.githubPatDesc")}
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p className="text-sm">{t("webDashboard.settingsView.githubToken")}</p>
            <p className="mt-1 text-xs text-muted-foreground">
              {t("webDashboard.settingsView.secretRedacted")}
            </p>
          </div>
          <StatusBadge
            active={settings.githubPatConfigured}
            label={
              settings.githubPatConfigured
                ? t("webDashboard.settingsView.configured")
                : t("webDashboard.settingsView.notConfigured")
            }
          />
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="pb-2">
          <div className="flex items-center gap-2">
            <BotIcon className="size-5 text-primary" aria-hidden="true" />
            <div>
              <CardTitle className="text-base">{t("settings.aiProviderTitle")}</CardTitle>
              <CardDescription className="mt-1">
                {t("settings.aiProviderDesc")}
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            <SettingValue
              label={t("settings.aiProviderLabel")}
              value={
                settings.aiProvider
                  ? t(`settings.aiProvider.${settings.aiProvider}`, {
                      defaultValue: settings.aiProvider,
                    })
                  : t("webDashboard.settingsView.providerNotSelected")
              }
            />
            <SettingValue label={t("settings.aiRegionLabel")} value={settings.aiRegion} />
            <SettingValue label={t("settings.aiModelLabel")} value={settings.aiModel} />
            <SettingValue label={t("settings.aiApiFormatLabel")} value={settings.aiProtocol} />
            <SettingValue
              label={t("webDashboard.settingsView.apiEndpoint")}
              value={settings.aiApiUrl}
            />
            <div className="flex items-center justify-between gap-3 rounded-lg bg-muted/25 px-3 py-2 ring-1 ring-border/70">
              <div>
                <p className="text-[11px] text-muted-foreground">
                  {t("webDashboard.settingsView.apiKey")}
                </p>
                <p className="mt-1 text-xs text-muted-foreground">
                  {t("webDashboard.settingsView.secretRedacted")}
                </p>
              </div>
              <StatusBadge
                active={settings.aiApiKeyConfigured}
                label={
                  settings.aiApiKeyConfigured
                    ? t("webDashboard.settingsView.configured")
                    : t("webDashboard.settingsView.notConfigured")
                }
              />
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="pb-2">
          <div className="flex items-center gap-2">
            <FolderSearchIcon className="size-5 text-primary" aria-hidden="true" />
            <div>
              <CardTitle className="text-base">{t("settings.scanDirs")}</CardTitle>
              <CardDescription className="mt-1">{t("settings.scanDirsDesc")}</CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-3">
          {customDirectories.length > 0 ? (
            <div>
              <p className="mb-2 text-xs font-medium text-muted-foreground">
                {t("webDashboard.settingsView.customDirectories")}
              </p>
              <div className="overflow-hidden rounded-lg border border-border">
                {customDirectories.map((directory) => (
                  <DirectoryRow key={directory.id} directory={directory} />
                ))}
              </div>
            </div>
          ) : (
            <p className="text-sm text-muted-foreground">
              {t("webDashboard.settingsView.customDirectoriesEmpty")}
            </p>
          )}

          {builtinDirectories.length > 0 && (
            <div>
              <button
                onClick={() => setShowBuiltinDirectories((value) => !value)}
                aria-expanded={showBuiltinDirectories}
                className="flex cursor-pointer items-center gap-1.5 text-xs text-muted-foreground transition-colors hover:text-foreground"
              >
                {showBuiltinDirectories ? (
                  <ChevronDownIcon className="size-3.5" />
                ) : (
                  <ChevronRightIcon className="size-3.5" />
                )}
                {showBuiltinDirectories
                  ? t("webDashboard.settingsView.hideBuiltin")
                  : t("webDashboard.settingsView.showBuiltin", {
                      count: builtinDirectories.length,
                    })}
              </button>
              {showBuiltinDirectories && (
                <div className="mt-2 grid gap-1.5 sm:grid-cols-2">
                  {builtinDirectories.map((directory) => (
                    <DirectoryRow key={directory.id} directory={directory} />
                  ))}
                </div>
              )}
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader className="pb-2">
          <div className="flex items-center gap-2">
            <PaletteIcon className="size-5 text-primary" aria-hidden="true" />
            <div>
              <CardTitle className="text-base">{t("settings.flavor")}</CardTitle>
              <CardDescription className="mt-1">
                {t("webDashboard.settingsView.browserAppearanceHint")}
              </CardDescription>
            </div>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <div>
            <p className="mb-2 text-xs text-muted-foreground">{t("settings.flavor")}</p>
            <div className="flex flex-wrap gap-2">
              {FLAVOR_ORDER.map((option) => (
                <Button
                  key={option}
                  variant={flavor === option ? "default" : "outline"}
                  size="sm"
                  onClick={() => setFlavor(option)}
                  aria-pressed={flavor === option}
                >
                  <span
                    className="size-2 rounded-full"
                    style={{ backgroundColor: FLAVOR_COLORS[option] }}
                    aria-hidden="true"
                  />
                  {t(`settings.${option}`)}
                </Button>
              ))}
            </div>
          </div>

          <div>
            <p className="mb-2 text-xs text-muted-foreground">
              {t("settings.accentColor")}
            </p>
            <div
              className="flex flex-wrap gap-2"
              role="radiogroup"
              aria-label={t("settings.accentColor")}
            >
              {ACCENT_NAMES.map((name) => (
                <button
                  key={name}
                  type="button"
                  role="radio"
                  aria-checked={accent === name}
                  aria-label={t(`settings.accent.${name}`)}
                  title={t(`settings.accent.${name}`)}
                  onClick={() => setAccent(name)}
                  className={cn(
                    "size-6 cursor-pointer rounded-full transition-transform",
                    accent === name
                      ? "scale-110 ring-2 ring-ring ring-offset-2 ring-offset-background"
                      : "ring-1 ring-border hover:scale-105",
                  )}
                  style={{ backgroundColor: `var(${CTP_VAR_MAP[name]})` }}
                />
              ))}
            </div>
          </div>

          <div>
            <p className="mb-2 text-xs text-muted-foreground">{t("settings.language")}</p>
            <div className="flex flex-wrap gap-2">
              {LANGUAGE_OPTIONS.map((option) => (
                <Button
                  key={option.code}
                  variant={currentLanguage === option.code ? "default" : "outline"}
                  size="sm"
                  onClick={() => handleLanguageChange(option.code)}
                  aria-pressed={currentLanguage === option.code}
                >
                  {t(option.labelKey)}
                </Button>
              ))}
            </div>
          </div>
        </CardContent>
      </Card>

      {snapshot.obsidianVaults.length > 0 && (
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="flex items-center gap-2 text-base">
              <BookOpenIcon className="size-5 text-primary" aria-hidden="true" />
              {t("webDashboard.sections.obsidianVaults")}
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-2">
            {snapshot.obsidianVaults.map((vault) => (
              <button
                key={vault.id}
                onClick={() =>
                  navigate(`/discover?project=${encodeURIComponent(vault.path)}`)
                }
                className="flex w-full cursor-pointer items-center justify-between gap-3 rounded-lg bg-muted/25 px-3 py-2 text-left ring-1 ring-border transition-colors hover:bg-muted/40"
              >
                <span className="min-w-0">
                  <span className="block truncate text-sm">{vault.name}</span>
                  <span className="block truncate text-[11px] text-muted-foreground">
                    {vault.path}
                  </span>
                </span>
                <span className="shrink-0 text-xs text-muted-foreground">
                  {t("webDashboard.skillCount", { count: vault.skillCount })}
                </span>
              </button>
            ))}
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="flex items-center gap-2 text-base">
            <InfoIcon className="size-5 text-primary" aria-hidden="true" />
            {t("settings.about")}
          </CardTitle>
        </CardHeader>
        <CardContent className="grid gap-3 sm:grid-cols-2">
          <SettingValue label={t("app.name")} value={snapshot.appName} />
          <SettingValue label={t("settings.dbPath")} value={settings.databasePath} />
          <div className="flex items-center gap-3 rounded-lg bg-muted/25 px-3 py-2 ring-1 ring-border/70 sm:col-span-2">
            <DatabaseIcon className="size-4 shrink-0 text-primary" aria-hidden="true" />
            <div>
              <p className="text-xs font-medium">
                {t("webDashboard.settingsView.readOnlyMode")}
              </p>
              <p className="mt-0.5 text-[11px] text-muted-foreground">
                {t("webDashboard.settingsView.readOnlyModeDesc")}
              </p>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

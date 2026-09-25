import { useEffect } from "react";
import { ArchiveRestore, DatabaseBackup, FolderOpen, Loader2, Trash2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { InlineConfirmAction } from "@/components/ui/inline-confirm-action";
import { formatPathForDisplay } from "@/lib/path";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { usePlatformStore } from "@/stores/platformStore";
import { useRecoveryStore } from "@/stores/recoveryStore";
import type { RecoveryEntry } from "@/types/recovery";

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function RecoveryRow({ entry }: { entry: RecoveryEntry }) {
  const { t } = useTranslation();
  const restoreEntry = useRecoveryStore((state) => state.restoreEntry);
  const deleteEntry = useRecoveryStore((state) => state.deleteEntry);
  const openBackupLocation = useRecoveryStore((state) => state.openBackupLocation);
  const restoringEntryId = useRecoveryStore((state) => state.restoringEntryId);
  const deletingEntryId = useRecoveryStore((state) => state.deletingEntryId);
  const isCreatingDatabaseBackup = useRecoveryStore((state) => state.isCreatingDatabaseBackup);
  const rescan = usePlatformStore((state) => state.rescan);
  const loadCentralSkills = useCentralSkillsStore((state) => state.loadCentralSkills);
  const isRestoring = restoringEntryId === entry.id;
  const isDeleting = deletingEntryId === entry.id;
  const isBusy = Boolean(restoringEntryId || deletingEntryId || isCreatingDatabaseBackup);
  const canRestore = entry.kind !== "database";

  async function handleRestore() {
    try {
      await restoreEntry(entry.id);
      await rescan();
      const scanError = usePlatformStore.getState().error;
      if (scanError) throw new Error(scanError);
      await loadCentralSkills();
      const libraryError = useCentralSkillsStore.getState().error;
      if (libraryError) throw new Error(libraryError);
      toast.success(t("recovery.restored"));
    } catch (error) {
      toast.error(String(error));
    }
  }

  async function handleDelete() {
    try {
      await deleteEntry(entry.id);
      toast.success(t("recovery.deleted"));
    } catch (error) {
      toast.error(String(error));
    }
  }

  async function handleOpenLocation() {
    try {
      await openBackupLocation(entry.backup_path);
    } catch (error) {
      toast.error(String(error));
    }
  }

  return (
    <div className="border-b border-border/50 px-4 py-3 last:border-0">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="text-sm font-medium break-words">{entry.label}</div>
          <div className="mt-0.5 text-xs text-muted-foreground"><span>{t(`recovery.kind.${entry.kind}`)}</span> · {formatDate(entry.created_at)}</div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Button
            type="button"
            variant="ghost"
            size="icon-sm"
            onClick={handleOpenLocation}
            disabled={isBusy}
            aria-label={t("recovery.openLocation")}
            title={t("recovery.openLocation")}
          >
            <FolderOpen className="size-3.5" />
          </Button>
          {canRestore ? (
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={handleRestore}
              disabled={isBusy}
            >
              {isRestoring ? <Loader2 className="size-3.5 animate-spin" /> : <ArchiveRestore className="size-3.5" />}
              {isRestoring ? t("recovery.restoring") : t("recovery.restore")}
            </Button>
          ) : null}
          <InlineConfirmAction
            idleAriaLabel={t("recovery.deletePermanently")}
            idleTitle={t("recovery.deletePermanently")}
            confirmLabel={t("recovery.deleteConfirm")}
            onConfirm={handleDelete}
            isLoading={isDeleting}
            disabled={isBusy}
            icon={<Trash2 className="size-3.5" />}
          />
        </div>
      </div>
      <details className="mt-2 text-xs text-muted-foreground">
        <summary className="cursor-pointer">{t("recovery.details")}</summary>
        <dl className="mt-3 grid gap-1 text-xs text-muted-foreground">
          <div className="grid grid-cols-[auto_1fr] gap-x-2">
            <dt>{t("recovery.originalPath")}</dt>
            <dd className="truncate font-mono" title={entry.original_path}>{formatPathForDisplay(entry.original_path)}</dd>
          </div>
          <div className="grid grid-cols-[auto_1fr] gap-x-2">
            <dt>{t("recovery.backupPath")}</dt>
            <dd className="truncate font-mono" title={entry.backup_path}>{formatPathForDisplay(entry.backup_path)}</dd>
          </div>
          <div className="grid grid-cols-[auto_1fr] gap-x-2">
            <dt>{t("recovery.createdAt")}</dt>
            <dd>{formatDate(entry.created_at)}</dd>
          </div>
          <div className="grid grid-cols-[auto_1fr] gap-x-2">
            <dt>{t("recovery.expiresAt")}</dt>
            <dd>{entry.expires_at ? formatDate(entry.expires_at) : t("recovery.neverExpires")}</dd>
          </div>
        </dl>
      </details>
    </div>
  );
}

export function RecoverySettings() {
  const { t } = useTranslation();
  const entries = useRecoveryStore((state) => state.entries);
  const isLoading = useRecoveryStore((state) => state.isLoading);
  const isCreatingDatabaseBackup = useRecoveryStore((state) => state.isCreatingDatabaseBackup);
  const restoringEntryId = useRecoveryStore((state) => state.restoringEntryId);
  const deletingEntryId = useRecoveryStore((state) => state.deletingEntryId);
  const error = useRecoveryStore((state) => state.error);
  const loadEntries = useRecoveryStore((state) => state.loadEntries);
  const createDatabaseBackup = useRecoveryStore((state) => state.createDatabaseBackup);

  const openDatabaseBackupFolder = useRecoveryStore((state) => state.openDatabaseBackupFolder);
  const databaseCount = entries.filter((entry) => entry.kind === "database").length;
  const fileEntries = entries.filter((entry) => entry.kind !== "database");
  const isBusy = isLoading || isCreatingDatabaseBackup || Boolean(restoringEntryId || deletingEntryId);

  async function handleOpenDatabaseFolder() {
    try {
      await openDatabaseBackupFolder();
    } catch (cause) {
      toast.error(String(cause));
    }
  }

  useEffect(() => {
    void loadEntries().catch(() => undefined);
  }, [loadEntries]);

  async function handleCreateDatabaseBackup() {
    try {
      await createDatabaseBackup();
      toast.success(t("recovery.backupCreated"));
    } catch (error) {
      toast.error(String(error));
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{t("recovery.title")}</CardTitle>
        <CardDescription>{t("recovery.description")}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-border px-3 py-2">
          <span className="text-sm" aria-live="polite">
            {isLoading ? t("recovery.loading") : t("recovery.databaseCount", { count: databaseCount })}
          </span>
          <div className="flex items-center gap-2">
            <Button type="button" variant="outline" size="sm" onClick={handleOpenDatabaseFolder} disabled={isBusy}>
              <FolderOpen className="size-3.5" />{t("recovery.openFolder")}
            </Button>
            <Button type="button" variant="outline" size="sm" onClick={handleCreateDatabaseBackup} disabled={isBusy}>
              {isCreatingDatabaseBackup ? <Loader2 className="size-3.5 animate-spin" /> : <DatabaseBackup className="size-3.5" />}
              {isCreatingDatabaseBackup ? t("recovery.creatingDatabaseBackup") : t("recovery.createDatabaseBackup")}
            </Button>
          </div>
        </div>
        <p className="text-xs text-muted-foreground">{t("recovery.databaseRestoreHint")}</p>
        {error ? <p className="text-xs text-destructive" role="alert">{error}</p> : null}
        <details className="rounded-lg border border-border">
          <summary className="cursor-pointer px-3 py-2 text-sm">{t("recovery.fileRecoveryCount", { count: fileEntries.length })}</summary>
          {fileEntries.length === 0 ? (
            <p className="px-3 pb-3 text-xs text-muted-foreground">{t("recovery.empty")}</p>
          ) : (
            <div className="max-h-96 overflow-y-auto border-t border-border">
              {fileEntries.map((entry) => <RecoveryRow key={entry.id} entry={entry} />)}
            </div>
          )}
        </details>
      </CardContent>
    </Card>
  );
}

import { useEffect, useState } from "react";
import { AlertTriangle, ArrowRight, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { formatPathForDisplay } from "@/lib/path";
import { useStorageStore } from "@/stores/storageStore";

interface LegacyMigrationNoticeProps {
  onMigrated: () => Promise<void>;
}

export function LegacyMigrationNotice({ onMigrated }: LegacyMigrationNoticeProps) {
  const { t } = useTranslation();
  const status = useStorageStore((state) => state.status);
  const preview = useStorageStore((state) => state.preview);
  const isLoading = useStorageStore((state) => state.isLoading);
  const isApplying = useStorageStore((state) => state.isApplying);
  const error = useStorageStore((state) => state.error);
  const previewMigration = useStorageStore((state) => state.previewMigration);
  const deferMigration = useStorageStore((state) => state.deferMigration);
  const migrateLegacy = useStorageStore((state) => state.migrateLegacy);
  const clearPreview = useStorageStore((state) => state.clearPreview);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (status?.migration_required && status.migration_state === "pending") {
      setOpen(true);
    }
  }, [status?.migration_required, status?.migration_state]);

  useEffect(() => {
    if (!open || !status?.migration_required) return;
    void previewMigration().catch(() => undefined);
  }, [open, previewMigration, status?.migration_required]);

  if (!status?.migration_required) return null;

  async function handleLater() {
    try {
      await deferMigration();
      setOpen(false);
      clearPreview();
    } catch {
      // The store exposes the error inside the dialog.
    }
  }

  async function handleMigrate() {
    try {
      await migrateLegacy();
      setOpen(false);
      await onMigrated();
    } catch {
      // The store exposes the error inside the dialog.
    }
  }

  return (
    <>
      <div
        className="flex min-h-10 shrink-0 items-center gap-3 border-b border-amber-500/30 bg-amber-500/10 px-4 py-2 text-sm"
        role="status"
      >
        <AlertTriangle className="size-4 shrink-0 text-amber-600 dark:text-amber-400" />
        <span className="min-w-0 flex-1 text-foreground">
          {t("migration.banner", { count: status.legacy_skill_count })}
        </span>
        <Button type="button" variant="outline" size="sm" onClick={() => setOpen(true)}>
          {t("migration.review")}
        </Button>
      </div>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="sm:max-w-lg" showCloseButton={false}>
          <DialogHeader>
            <DialogTitle>{t("migration.title")}</DialogTitle>
            <DialogDescription>
              {t("migration.description", { count: status.legacy_skill_count })}
            </DialogDescription>
          </DialogHeader>
          <DialogBody className="space-y-4">
            <div className="rounded-lg border border-border bg-muted/20 p-3">
              <div className="flex items-center gap-2 text-sm">
                <code className="min-w-0 flex-1 truncate">
                  {formatPathForDisplay(preview?.current_path ?? status.legacy_path)}
                </code>
                <ArrowRight className="size-4 shrink-0 text-muted-foreground" />
                <code className="min-w-0 flex-1 truncate text-right">
                  {formatPathForDisplay(preview?.new_path ?? status.default_central_path)}
                </code>
              </div>
              <p className="mt-2 text-xs text-muted-foreground">
                {t("migration.keepsInstalled", { path: formatPathForDisplay(status.universal_path) })}
              </p>
            </div>

            {isLoading ? (
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <Loader2 className="size-4 animate-spin" />
                {t("migration.checking")}
              </div>
            ) : null}

            {preview && preview.conflicts.length > 0 ? (
              <div className="rounded-lg border border-destructive/30 bg-destructive/5 p-3" role="alert">
                <p className="text-sm font-medium text-destructive">
                  {t("migration.conflicts", { count: preview.conflicts.length })}
                </p>
                <ul className="mt-2 max-h-28 list-disc overflow-auto pl-5 text-xs text-muted-foreground">
                  {preview.conflicts.map((conflict) => (
                    <li key={conflict}>{conflict}</li>
                  ))}
                </ul>
              </div>
            ) : null}

            {error ? <p className="text-sm text-destructive" role="alert">{error}</p> : null}
          </DialogBody>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={handleLater} disabled={isApplying}>
              {t("migration.later")}
            </Button>
            <Button
              type="button"
              onClick={handleMigrate}
              disabled={isApplying || isLoading || !preview?.can_proceed}
            >
              {isApplying ? <Loader2 className="size-4 animate-spin" /> : null}
              {isApplying ? t("migration.migrating") : t("migration.start")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

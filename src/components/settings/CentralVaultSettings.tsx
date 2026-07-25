import { useEffect, useState } from "react";
import { FolderOpen, Loader2, MoveRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { formatPathForDisplay } from "@/lib/path";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { usePlatformStore } from "@/stores/platformStore";
import { useStorageStore } from "@/stores/storageStore";

export function CentralVaultSettings() {
  const { t } = useTranslation();
  const status = useStorageStore((state) => state.status);
  const preview = useStorageStore((state) => state.preview);
  const isLoading = useStorageStore((state) => state.isLoading);
  const isApplying = useStorageStore((state) => state.isApplying);
  const error = useStorageStore((state) => state.error);
  const previewPathChange = useStorageStore((state) => state.previewPathChange);
  const changePath = useStorageStore((state) => state.changePath);
  const clearPreview = useStorageStore((state) => state.clearPreview);
  const rescan = usePlatformStore((state) => state.rescan);
  const loadCentralSkills = useCentralSkillsStore((state) => state.loadCentralSkills);
  const [path, setPath] = useState("");
  const [confirmOpen, setConfirmOpen] = useState(false);
  const pathError = error && !confirmOpen ? error : null;

  useEffect(() => {
    if (status?.central_path) setPath(status.central_path);
  }, [status?.central_path]);

  async function chooseFolder() {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      directory: true,
      multiple: false,
      defaultPath: path || status?.central_path,
      title: t("storage.chooseFolder"),
    });
    if (typeof selected === "string") setPath(selected);
  }

  async function handlePreview() {
    try {
      await previewPathChange(path.trim());
      setConfirmOpen(true);
    } catch {
      // The store renders the backend validation error below the field.
    }
  }

  async function handleChange() {
    try {
      const result = await changePath(path.trim());
      setConfirmOpen(false);
      await rescan();
      await loadCentralSkills();
      toast.success(t("storage.changed", { count: result.skill_count }));
    } catch {
      // The store renders the error in the confirmation dialog.
    }
  }

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>{t("storage.title")}</CardTitle>
          <CardDescription className="mt-1">{t("storage.description")}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <label htmlFor="central-vault-path" className="text-xs text-muted-foreground">
            {t("storage.pathLabel")}
          </label>
          <div className="flex gap-2">
            <Input
              id="central-vault-path"
              value={path}
              onChange={(event) => setPath(event.target.value)}
              placeholder="~/.skillsmanage/skills/"
              className="font-mono"
              aria-invalid={!!pathError}
              aria-describedby={
                pathError
                  ? "central-vault-path-hint central-vault-path-error"
                  : "central-vault-path-hint"
              }
            />
            <Button type="button" variant="outline" onClick={chooseFolder}>
              <FolderOpen className="size-4" />
              {t("storage.chooseFolder")}
            </Button>
          </div>
          <p id="central-vault-path-hint" className="text-xs text-muted-foreground">
            {t("storage.sharedPathHint", {
              path: formatPathForDisplay(status?.universal_path ?? "~/.agents/skills/"),
            })}
          </p>
          {pathError ? (
            <p id="central-vault-path-error" className="text-xs text-destructive" role="alert">
              {pathError}
            </p>
          ) : null}
          <Button
            type="button"
            onClick={handlePreview}
            disabled={
              isLoading ||
              !path.trim() ||
              path.trim() === status?.central_path
            }
          >
            {isLoading ? <Loader2 className="size-4 animate-spin" /> : <MoveRight className="size-4" />}
            {t("storage.preview")}
          </Button>
        </CardContent>
      </Card>

      <Dialog
        open={confirmOpen}
        onOpenChange={(open) => {
          setConfirmOpen(open);
          if (!open) clearPreview();
        }}
      >
        <DialogContent className="sm:max-w-lg">
          <DialogHeader>
            <DialogTitle>{t("storage.confirmTitle")}</DialogTitle>
            <DialogDescription>
              {t("storage.confirmDescription", { count: preview?.skill_count ?? 0 })}
            </DialogDescription>
          </DialogHeader>
          <DialogBody className="space-y-4">
            {preview ? (
              <div className="rounded-lg border border-border bg-muted/20 p-3 text-sm">
                <code className="block truncate">{formatPathForDisplay(preview.current_path)}</code>
                <MoveRight className="my-2 size-4 text-muted-foreground" />
                <code className="block truncate">{formatPathForDisplay(preview.new_path)}</code>
              </div>
            ) : null}
            {preview && preview.conflicts.length > 0 ? (
              <div className="rounded-lg border border-destructive/30 bg-destructive/5 p-3" role="alert">
                <p className="text-sm font-medium text-destructive">
                  {t("storage.conflicts", { count: preview.conflicts.length })}
                </p>
                <ul className="mt-2 max-h-28 list-disc overflow-auto pl-5 text-xs text-muted-foreground">
                  {preview.conflicts.map((conflict) => <li key={conflict}>{conflict}</li>)}
                </ul>
              </div>
            ) : null}
            {error ? <p className="text-sm text-destructive" role="alert">{error}</p> : null}
          </DialogBody>
          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => setConfirmOpen(false)} disabled={isApplying}>
              {t("common.cancel")}
            </Button>
            <Button type="button" onClick={handleChange} disabled={isApplying || !preview?.can_proceed}>
              {isApplying ? <Loader2 className="size-4 animate-spin" /> : null}
              {isApplying ? t("storage.moving") : t("storage.move")}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}

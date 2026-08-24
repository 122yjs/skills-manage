import { useEffect, useMemo, useState } from "react";
import { ChevronDown, ChevronRight, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import { PlatformIcon } from "@/components/platform/PlatformIcon";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Dialog,
  DialogBody,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { useDevToolSetupStore } from "@/stores/devToolSetupStore";

export function DevToolSetupDialog() {
  const { t } = useTranslation();
  const status = useDevToolSetupStore((state) => state.status);
  const completed = useDevToolSetupStore((state) => state.completed);
  const tools = useDevToolSetupStore((state) => state.tools);
  const isEditorOpen = useDevToolSetupStore((state) => state.isEditorOpen);
  const isSaving = useDevToolSetupStore((state) => state.isSaving);
  const error = useDevToolSetupStore((state) => state.error);
  const closeEditor = useDevToolSetupStore((state) => state.closeEditor);
  const save = useDevToolSetupStore((state) => state.save);
  const open = status === "ready" && (!completed || isEditorOpen);
  const canClose = completed;

  const defaultSelection = useMemo(
    () =>
      tools
        .filter((tool) => (completed ? tool.is_enabled : tool.is_detected))
        .map((tool) => tool.id),
    [completed, tools]
  );
  const [selectedIds, setSelectedIds] = useState<Set<string>>(new Set());
  const [showUnselectedTools, setShowUnselectedTools] = useState(false);
  const selectedTools = tools.filter((tool) => selectedIds.has(tool.id));
  const unselectedTools = tools.filter((tool) => !selectedIds.has(tool.id));

  useEffect(() => {
    if (open) {
      setSelectedIds(new Set(defaultSelection));
      setShowUnselectedTools(false);
    }
  }, [defaultSelection, open]);

  function toggleTool(toolId: string, checked: boolean) {
    setSelectedIds((current) => {
      const next = new Set(current);
      if (checked) next.add(toolId);
      else next.delete(toolId);
      return next;
    });
  }

  function renderToolOption(tool: (typeof tools)[number]) {
    const checkboxId = `dev-tool-${tool.id}`;
    return (
      <label
        key={tool.id}
        htmlFor={checkboxId}
        className="flex cursor-pointer items-center gap-3 rounded-lg border border-border px-3 py-2.5 hover:bg-muted/40"
      >
        <Checkbox
          id={checkboxId}
          checked={selectedIds.has(tool.id)}
          onCheckedChange={(checked) => toggleTool(tool.id, !!checked)}
          aria-label={tool.display_name}
        />
        <PlatformIcon agentId={tool.id} className="size-5" size={20} />
        <span className="min-w-0 flex-1 truncate text-sm font-medium">
          {tool.display_name}
        </span>
        {tool.is_detected && (
          <span className="shrink-0 rounded-full bg-primary/10 px-2 py-0.5 text-[11px] text-primary">
            {t("devToolSetup.detected")}
          </span>
        )}
      </label>
    );
  }

  return (
    <Dialog
      open={open}
      disablePointerDismissal={!canClose}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && canClose) closeEditor();
      }}
    >
      <DialogContent
        className="sm:max-w-2xl"
        showCloseButton={canClose}
      >
        <DialogHeader>
          <DialogTitle>{t("devToolSetup.title")}</DialogTitle>
          <DialogDescription>{t("devToolSetup.description")}</DialogDescription>
        </DialogHeader>

        <DialogBody className="space-y-4">
          <p className="text-xs text-muted-foreground">
            {t("devToolSetup.detectedHint")}
          </p>
          {selectedTools.length > 0 && (
            <div className="grid grid-cols-2 gap-2" role="group" aria-label={t("devToolSetup.listLabel")}>
              {selectedTools.map(renderToolOption)}
            </div>
          )}
          {unselectedTools.length > 0 && (
            <div className="space-y-2">
              <button
                type="button"
                className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground hover:text-foreground"
                onClick={() => setShowUnselectedTools((current) => !current)}
              >
                {showUnselectedTools ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
                {showUnselectedTools
                  ? t("devToolSetup.hideUnselectedTools")
                  : t("devToolSetup.showUnselectedTools", { count: unselectedTools.length })}
              </button>
              {showUnselectedTools && (
                <div className="grid grid-cols-2 gap-2" role="group" aria-label={t("devToolSetup.unselectedTools")}>
                  {unselectedTools.map(renderToolOption)}
                </div>
              )}
            </div>
          )}
          {error && <p className="text-sm text-destructive">{error}</p>}
        </DialogBody>

        <DialogFooter>
          {canClose && (
            <Button variant="outline" onClick={closeEditor} disabled={isSaving}>
              {t("common.cancel")}
            </Button>
          )}
          <Button
            onClick={() => void save(Array.from(selectedIds)).catch(() => undefined)}
            disabled={isSaving}
          >
            {isSaving && <Loader2 className="size-4 animate-spin" />}
            {completed ? t("devToolSetup.save") : t("devToolSetup.continue")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

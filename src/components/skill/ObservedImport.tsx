import { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
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
import { useSkillStore, type DuplicateComparison } from "@/stores/skillStore";

/**
 * 관찰 항목 가져오기 동작. 같은 이름의 보관함 스킬이 있으면 가져오지 않고 중복 출처 비교를 보여 준다.
 * `onImported`는 가져오기에 성공한 뒤 화면의 목록을 다시 읽는 일을 맡는다.
 */
export function useObservedImport(onImported: () => Promise<unknown>) {
  const { t } = useTranslation();
  const importObserved = useSkillStore((state) => state.importObserved);
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [conflict, setConflict] = useState<{ name: string; comparison: DuplicateComparison } | null>(null);

  async function run(skillId: string, agentId: string, name: string) {
    setBusyKey(`${agentId}::${skillId}`);
    try {
      const comparison = await importObserved(skillId, agentId);
      if (comparison) {
        setConflict({ name, comparison });
        return;
      }
      await onImported();
      toast.success(t("observedImport.success", { name }));
    } catch (error) {
      toast.error(t("observedImport.error", { error: String(error) }));
    } finally {
      setBusyKey(null);
    }
  }

  const dialog = (
    <Dialog open={conflict !== null} onOpenChange={(open) => !open && setConflict(null)}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>{t("observedImport.conflictTitle")}</DialogTitle>
          <DialogDescription>
            {t("observedImport.conflictDescription", { name: conflict?.name ?? "" })}
          </DialogDescription>
        </DialogHeader>
        {conflict && (
          <DialogBody className="space-y-3 text-sm">
            <p className="font-medium">{t(`skillLocations.result.${conflict.comparison.relation}`)}</p>
            <p className="text-muted-foreground">{t(`skillLocations.${conflict.comparison.relation}`)}</p>
            <div>
              <div className="text-xs font-medium text-muted-foreground">{t("observedImport.paths")}</div>
              <ul className="mt-1 space-y-1">
                {conflict.comparison.paths.map((path) => (
                  <li key={path} className="break-all text-xs text-muted-foreground">{path}</li>
                ))}
              </ul>
            </div>
          </DialogBody>
        )}
        <DialogFooter>
          <Button type="button" variant="outline" onClick={() => setConflict(null)}>
            {t("observedImport.close")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );

  return { run, busyKey, dialog };
}

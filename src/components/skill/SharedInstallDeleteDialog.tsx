import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Loader2 } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { Dialog, DialogBody, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { useSkillUsageStore, type SharedDeletePreview } from "@/stores/skillUsageStore";

/** 개별·선택·전체 삭제가 같은 영향 확인과 오류 표시를 사용한다. */
export function SharedInstallDeleteDialog({ skillIds, onClose, onDeleted }: {
  skillIds: string[];
  onClose: () => void;
  onDeleted: () => Promise<void>;
}) {
  const { t } = useTranslation();
  const preview = useSkillUsageStore((s) => s.previewSharedDelete);
  const remove = useSkillUsageStore((s) => s.deleteSharedInstalls);
  const [plans, setPlans] = useState<SharedDeletePreview[]>([]);
  const [errors, setErrors] = useState<Array<{ skill_id: string; error: string }>>([]);
  const [pending, setPending] = useState(skillIds);
  const [revision, setRevision] = useState(0);
  const [loading, setLoading] = useState(true);
  const [deleting, setDeleting] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setPlans([]);
    setErrors([]);
    void Promise.allSettled(pending.map((id) => preview(id))).then((results) => {
      if (cancelled) return;
      setPlans(results.flatMap((r) => r.status === "fulfilled" ? [r.value] : []));
      setErrors(results.flatMap((r, i) => r.status === "rejected" ? [{ skill_id: pending[i], error: String(r.reason) }] : []));
      setLoading(false);
    });
    return () => { cancelled = true; };
  }, [pending, preview, revision]);

  async function confirm() {
    setDeleting(true);
    try {
      const result = await remove(plans);
      if (result.deleted.length) toast.success(t("universal.removeSuccess", { count: result.deleted.length }));
      setPlans([]);
      // 재시도는 성공 항목을 제외하고 대상을 다시 읽은 후에만 가능하다.
      setErrors(result.failed);
      if (result.failed.length) {
        toast.error(t("universal.removePartial", { deleted: result.deleted.length, failed: result.failed.length }));
      }
      try { await onDeleted(); }
      catch (error) { toast.error(t("sharedDelete.refreshFailed", { error: String(error) })); }
      if (!result.failed.length) onClose();
    } catch (error) {
      setPlans([]);
      setErrors(pending.map((skill_id) => ({ skill_id, error: String(error) })));
    } finally { setDeleting(false); }
  }

  return <Dialog open onOpenChange={(open) => { if (!open && !deleting) onClose(); }}>
    <DialogContent className="sm:max-w-xl">
      <DialogHeader>
        <DialogTitle>{t("sharedDelete.title")}</DialogTitle>
        <DialogDescription>{t("sharedDelete.description")}</DialogDescription>
      </DialogHeader>
      <DialogBody className="space-y-3">
        <p className="rounded-lg bg-muted p-3 text-sm">{t("sharedDelete.kept")}</p>
        {loading && <p role="status" className="flex items-center gap-2 text-sm"><Loader2 className="size-4 animate-spin" />{t("sharedDelete.loading")}</p>}
        {plans.map((plan) => <section key={plan.skill_id} className="rounded-lg border p-3 text-sm">
          <div className="flex justify-between gap-3"><strong>{plan.skill_name}</strong><span className="text-muted-foreground">{t(plan.enabled ? "skillUsage.active" : "skillUsage.paused")}</span></div>
          <p className="mt-1 break-all text-xs text-muted-foreground">{plan.source_path}</p>
          <p className="mt-3 font-medium">{t("sharedDelete.links", { count: plan.links.length })}</p>
          {plan.links.length ? <ul className="mt-1 space-y-2">{plan.links.map((link) => <li key={link.path}>
            <span>{link.display_name}</span><span className="block break-all text-xs text-muted-foreground">{link.installed_path}</span>
          </li>)}</ul> : <p className="mt-1 text-muted-foreground">{t("sharedDelete.noLinks")}</p>}
        </section>)}
        {!!errors.length && <div role="alert" className="rounded-lg border border-destructive/50 bg-destructive/5 p-3 text-sm">
          <p className="font-medium">{t("sharedDelete.failed")}</p>
          <ul className="mt-2 space-y-2">{errors.map((failure) => <li key={failure.skill_id}><strong>{failure.skill_id}</strong><p className="break-words">{failure.error}</p></li>)}</ul>
        </div>}
      </DialogBody>
      <DialogFooter>
        <Button variant="outline" disabled={deleting} onClick={onClose}>{t("common.cancel")}</Button>
        {!!errors.length && <Button variant="outline" disabled={loading || deleting} onClick={() => {
          setPending(plans.length ? pending : errors.map((e) => e.skill_id));
          setRevision((n) => n + 1);
        }}>{t("sharedDelete.retry")}</Button>}
        <Button variant="destructive" disabled={loading || deleting || !!errors.length || !plans.length} onClick={() => void confirm()}>
          {deleting && <Loader2 className="size-4 animate-spin" />}{t("sharedDelete.confirm", { count: plans.length })}
        </Button>
      </DialogFooter>
    </DialogContent>
  </Dialog>;
}

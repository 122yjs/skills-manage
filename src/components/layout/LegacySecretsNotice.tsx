import { useEffect, useState } from "react";
import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { isTauriRuntime } from "@/lib/tauri";
import { useLegacySecretsStore } from "@/stores/legacySecretsStore";

/** 키체인을 쓸 수 없어 옮기지 못한 옛 비밀값을 지울지 사용자에게 묻는다(ADR-0003). */
export function LegacySecretsNotice() {
  const { t } = useTranslation();
  const present = useLegacySecretsStore((state) => state.present);
  const load = useLegacySecretsStore((state) => state.load);
  const clear = useLegacySecretsStore((state) => state.clear);
  const [dismissed, setDismissed] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    void load().catch(() => undefined);
  }, [load]);

  if (!present || dismissed) return null;

  async function handleClear() {
    try {
      await clear();
    } catch (err) {
      setError(String(err));
    }
  }

  return (
    <div
      className="flex min-h-10 shrink-0 items-center gap-3 border-b border-amber-500/30 bg-amber-500/10 px-4 py-2 text-sm"
      role="status"
    >
      <AlertTriangle className="size-4 shrink-0 text-amber-600 dark:text-amber-400" />
      <span className="min-w-0 flex-1 text-foreground">
        {t("legacySecrets.banner")}
        {error ? <span className="ml-2 text-destructive">{error}</span> : null}
      </span>
      <Button type="button" variant="outline" size="sm" onClick={() => setDismissed(true)}>
        {t("legacySecrets.later")}
      </Button>
      <Button type="button" size="sm" onClick={handleClear}>
        {t("legacySecrets.clear")}
      </Button>
    </div>
  );
}

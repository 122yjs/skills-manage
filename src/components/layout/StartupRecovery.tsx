import { useEffect, useState, type ReactNode } from "react";
import { FolderOpen, Loader2, RotateCw, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useStartupStore } from "@/stores/startupStore";

/** 시작 초기화가 실패했으면 복구 화면을, 아니면 앱 화면을 보여 준다. */
export function StartupGate({ children }: { children: ReactNode }) {
  const phase = useStartupStore((state) => state.phase);
  const load = useStartupStore((state) => state.load);

  useEffect(() => {
    void load();
  }, [load]);

  if (phase === "checking") return null;
  return phase === "failed" ? <StartupRecovery /> : <>{children}</>;
}

function StartupRecovery() {
  const { t } = useTranslation();
  const error = useStartupStore((state) => state.error);
  const isRetrying = useStartupStore((state) => state.isRetrying);
  const retry = useStartupStore((state) => state.retry);
  const [path, setPath] = useState("");

  async function chooseFolder() {
    const { open } = await import("@tauri-apps/plugin-dialog");
    const selected = await open({
      directory: true,
      multiple: false,
      title: t("storage.chooseFolder"),
    });
    if (typeof selected === "string") setPath(selected);
  }

  return (
    <main className="mx-auto flex min-h-screen max-w-xl flex-col justify-center gap-4 p-6">
      <div className="flex items-center gap-2 text-destructive">
        <TriangleAlert className="size-5" />
        <h1 className="text-lg font-semibold">{t("startup.title")}</h1>
      </div>
      <p className="text-sm text-muted-foreground">{t("startup.description")}</p>
      <pre
        role="alert"
        className="whitespace-pre-wrap break-all rounded-md border bg-muted p-3 text-xs"
      >
        {error}
      </pre>
      <label htmlFor="startup-vault-path" className="text-xs text-muted-foreground">
        {t("startup.pathLabel")}
      </label>
      <div className="flex gap-2">
        <Input
          id="startup-vault-path"
          value={path}
          onChange={(event) => setPath(event.target.value)}
          placeholder="~/.skillsmanage/skills/"
          className="font-mono"
        />
        <Button type="button" variant="outline" onClick={chooseFolder}>
          <FolderOpen className="size-4" />
          {t("storage.chooseFolder")}
        </Button>
      </div>
      <div className="flex gap-2">
        <Button type="button" disabled={isRetrying} onClick={() => void retry()}>
          {isRetrying ? <Loader2 className="size-4 animate-spin" /> : <RotateCw className="size-4" />}
          {t("startup.retry")}
        </Button>
        <Button
          type="button"
          variant="outline"
          disabled={isRetrying || !path.trim()}
          onClick={() => void retry(path)}
        >
          {t("startup.changePath")}
        </Button>
      </div>
    </main>
  );
}

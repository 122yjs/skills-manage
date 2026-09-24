import type { ComponentProps } from "react";
import { open } from "@tauri-apps/plugin-shell";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { isTauriRuntime } from "@/lib/tauri";

// 데스크톱에서는 시스템 브라우저로, 웹에서는 일반 새 탭으로 원본을 연다.
export function GitHubSourceLink({
  href,
  children,
  ...props
}: Omit<ComponentProps<"a">, "onClick" | "href"> & { href: string }) {
  const { t } = useTranslation();
  return (
    <a
      {...props}
      href={href}
      target="_blank"
      rel="noreferrer"
      onClick={(event) => {
        event.stopPropagation();
        if (!isTauriRuntime()) return;
        event.preventDefault();
        void open(href).catch(() => {
          toast.error(t("skillOrigin.openFailed"));
        });
      }}
    >
      {children}
    </a>
  );
}

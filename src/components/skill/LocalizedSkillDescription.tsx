import { Loader2, Languages } from "lucide-react";
import type { ReactNode } from "react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useSkillDescriptionTranslation } from "@/hooks/useSkillDescriptionTranslation";
import type { SkillDescriptionTranslationMeta } from "@/types";
import { cn } from "@/lib/utils";

export interface LocalizedSkillDescriptionProps
  extends SkillDescriptionTranslationMeta {
  description?: string | null;
  className?: string;
  /** 상세 화면처럼 이미 보이는 내용은 IntersectionObserver 없이 즉시 처리한다. */
  immediate?: boolean;
  renderText?: (text: string) => ReactNode;
}

export function LocalizedSkillDescription({
  className,
  immediate = false,
  renderText,
  ...translationOptions
}: LocalizedSkillDescriptionProps) {
  const { t } = useTranslation();
  const rootRef = useRef<HTMLDivElement | null>(null);
  const [isVisible, setIsVisible] = useState(immediate);

  useEffect(() => {
    if (immediate) {
      setIsVisible(true);
      return;
    }
    if (typeof IntersectionObserver === "undefined") {
      return;
    }

    const element = rootRef.current;
    if (!element) return;
    const observer = new IntersectionObserver(
      (entries) => {
        setIsVisible(entries.some((entry) => entry.isIntersecting));
      },
      { rootMargin: "0px" }
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [immediate]);

  const state = useSkillDescriptionTranslation({
    ...translationOptions,
    isVisible,
  });

  const engineLabel =
    state.engine === "apple"
      ? t("common.translatedOnDevice")
      : state.engine === "api"
        ? t("common.translatedWithApi")
        : undefined;

  return (
    <div ref={rootRef} className={cn("space-y-1", className)}>
      {state.displayText &&
        (renderText ? (
          renderText(state.displayText)
        ) : (
          <p className="text-xs text-muted-foreground line-clamp-2 leading-relaxed">
            {state.displayText}
          </p>
        ))}
      {state.displayText && (
        <div className="flex min-h-5 flex-wrap items-center gap-x-2 gap-y-1 text-[10px] text-muted-foreground">
          {engineLabel && <span>{engineLabel}</span>}
          {state.isEnglishFallback && <span>{t("common.englishFallback")}</span>}
          {state.translation && (
            <button
              type="button"
              className="rounded-sm text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
              onClick={(event) => {
                event.stopPropagation();
                state.toggleOriginal();
              }}
            >
              {state.isShowingOriginal ? t("common.showTranslation") : t("common.showOriginal")}
            </button>
          )}
          <button
            type="button"
            className="inline-flex items-center gap-1 rounded-sm hover:text-primary hover:underline disabled:cursor-not-allowed disabled:opacity-60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            disabled={!state.canTranslate || state.isLoading}
            title={state.unavailableReason ?? state.onDeviceError}
            onClick={(event) => {
              event.stopPropagation();
              void state.requestOnDeviceTranslation();
            }}
          >
            {state.loadingEngine === "apple" && <Loader2 className="size-3 animate-spin" />}
            {t("common.translateOnDevice")}
            {state.loadingEngine === "apple"
              ? ` · ${t("common.translationLoading")}`
              : state.onDeviceError
                ? ` · ${t("common.retry")}`
                : null}
          </button>
          <button
            type="button"
            className="inline-flex items-center gap-1 rounded-sm hover:text-primary hover:underline disabled:cursor-not-allowed disabled:opacity-60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            disabled={!state.canTranslate || state.isLoading || state.apiConfirming}
            title={state.unavailableReason}
            onClick={(event) => {
              event.stopPropagation();
              void state.requestApiTranslation();
            }}
          >
            {state.loadingEngine === "api" ? (
              <Loader2 className="size-3 animate-spin" />
            ) : (
              <Languages className="size-3" />
            )}
            {t("common.translateWithApi")}
            {state.loadingEngine === "api" && ` · ${t("common.translationLoading")}`}
          </button>
          {state.canTranslate && state.apiConfirming && (
            <span className="inline-flex items-center gap-1.5">
              <span>{t("common.translateWithApiConfirm")}</span>
              <button
                type="button"
                className="rounded-sm font-medium text-primary hover:underline"
                disabled={state.isLoading}
                onClick={(event) => {
                  event.stopPropagation();
                  void state.requestApiTranslation();
                }}
              >
                {t("common.confirm")}
              </button>
              <button
                type="button"
                className="rounded-sm hover:underline"
                disabled={state.isLoading}
                onClick={(event) => {
                  event.stopPropagation();
                  state.cancelApiConfirmation();
                }}
              >
                {t("common.cancel")}
              </button>
            </span>
          )}
          {state.unavailableReason && <span>{state.unavailableReason}</span>}
          {state.isLoading && !state.loadingEngine && <span>{t("common.translationChecking")}</span>}
          {state.onDeviceError && (
            <span className="basis-full" role="status">
              {t("common.onDeviceTranslationFailed", { error: state.onDeviceError })}
            </span>
          )}
          {state.error && (
            <span role="alert" className="text-destructive">
              {t("common.translationFailed", { error: state.error })}
            </span>
          )}
        </div>
      )}
    </div>
  );
}

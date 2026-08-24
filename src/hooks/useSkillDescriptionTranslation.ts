import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  LocalizedSkillDescriptions,
  SkillDescriptionTranslationEngine,
  SkillDescriptionTranslationMeta,
} from "@/types";
import { resolveSkillDescription } from "@/lib/skillDescription";
import { invoke, isTauriRuntime } from "@/lib/tauri";

export interface UseSkillDescriptionTranslationOptions
  extends SkillDescriptionTranslationMeta {
  description?: string | null;
  /** 카드에서는 false로 두고 IntersectionObserver가 보일 때만 true로 바꾼다. */
  isVisible?: boolean;
}

interface RepositoryDescriptionsResult {
  localizedDescriptions?: LocalizedSkillDescriptions;
  legacyDescription?: string | null;
  sourceLocale?: string | null;
}

interface TranslationResult {
  translatedText: string;
  engine: SkillDescriptionTranslationEngine | string;
  targetLocale: string;
  cached: boolean;
}

export interface SkillDescriptionTranslationState {
  displayText?: string;
  originalText?: string;
  translation?: string;
  engine?: SkillDescriptionTranslationEngine | string;
  isShowingOriginal: boolean;
  isLoading: boolean;
  error?: string;
  apiConfirming: boolean;
  canTranslate: boolean;
  isEnglishFallback: boolean;
  requestApiTranslation: () => Promise<void>;
  toggleOriginal: () => void;
  cancelApiConfirmation: () => void;
}

function normalizeLocale(locale: string): string {
  return locale.trim().replace(/_/g, "-").toLowerCase();
}

export function useSkillDescriptionTranslation({
  resourceId,
  filePath,
  localizedDescriptions = {},
  sourceLocale,
  description,
  isVisible = true,
}: UseSkillDescriptionTranslationOptions): SkillDescriptionTranslationState {
  const { i18n } = useTranslation();
  const targetLocale = normalizeLocale(i18n.resolvedLanguage ?? i18n.language ?? "en");
  const [repositoryDescriptions, setRepositoryDescriptions] = useState<RepositoryDescriptionsResult>();
  const [resolvedRepositoryKey, setResolvedRepositoryKey] = useState<string>();
  const [translation, setTranslation] = useState<TranslationResult>();
  const [isShowingOriginal, setIsShowingOriginal] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string>();
  const [apiConfirming, setApiConfirming] = useState(false);
  const repositoryRequestRef = useRef(0);
  const translationRequestRef = useRef(0);
  const apiTranslationInFlightRef = useRef(false);

  const availableDescriptions = useMemo(
    () => ({
      ...localizedDescriptions,
      ...repositoryDescriptions?.localizedDescriptions,
    }),
    [localizedDescriptions, repositoryDescriptions?.localizedDescriptions]
  );
  const legacyDescription = repositoryDescriptions?.legacyDescription ?? description;
  const resolved = useMemo(
    () =>
      resolveSkillDescription({
        locale: targetLocale,
        localizedDescriptions: availableDescriptions,
        legacyDescription,
      }),
    [availableDescriptions, legacyDescription, targetLocale]
  );
  const originalText = resolved?.text;
  const resolvedSourceLocale =
    resolved?.locale ?? repositoryDescriptions?.sourceLocale ?? sourceLocale;
  const alreadyLocalized = resolved?.source === "requested-locale";
  const repositoryKey = filePath
    ? `${filePath}\u0000${targetLocale}\u0000${description ?? ""}`
    : undefined;
  const isRepositoryResolved = !filePath || resolvedRepositoryKey === repositoryKey;
  const awaitingRepositoryDescriptions = Boolean(
    filePath && isVisible && isTauriRuntime() && !isRepositoryResolved
  );

  useEffect(() => {
    repositoryRequestRef.current += 1;
    setRepositoryDescriptions(undefined);
    setResolvedRepositoryKey(undefined);
  }, [description, filePath, resourceId, targetLocale]);

  useEffect(() => {
    translationRequestRef.current += 1;
    setTranslation(undefined);
    setIsShowingOriginal(false);
    setApiConfirming(false);
    setError(undefined);
  }, [resourceId, originalText, targetLocale]);

  useEffect(() => {
    const requestId = ++repositoryRequestRef.current;
    if (!isVisible || !filePath || !repositoryKey || !isTauriRuntime()) return;

    void invoke<RepositoryDescriptionsResult>("get_repository_skill_descriptions", {
      filePath,
      fallbackDescription: description ?? null,
      targetLocale,
    })
      .then((result) => {
        if (repositoryRequestRef.current === requestId) setRepositoryDescriptions(result);
      })
      .catch(() => {
        // 저장소 설명을 읽지 못해도 기존 카드 설명은 그대로 쓴다.
      })
      .finally(() => {
        if (repositoryRequestRef.current === requestId) setResolvedRepositoryKey(repositoryKey);
      });
  }, [description, filePath, isVisible, repositoryKey, targetLocale]);

  const sourceMatchesTarget = resolvedSourceLocale
    ? normalizeLocale(resolvedSourceLocale).split("-")[0] === targetLocale.split("-")[0]
    : false;
  const canTranslate = Boolean(
    originalText &&
      !awaitingRepositoryDescriptions &&
      !alreadyLocalized &&
      !sourceMatchesTarget &&
      isTauriRuntime()
  );
  const isEnglishFallback = Boolean(
    !translation &&
      resolved?.source === "english-fallback" &&
      targetLocale.split("-")[0] !== "en"
  );

  const translationRequest = useMemo(
    () =>
      originalText &&
        !awaitingRepositoryDescriptions &&
        !alreadyLocalized &&
        !sourceMatchesTarget
        ? {
            resourceId,
            sourceText: originalText,
            sourceLocale: resolvedSourceLocale ?? null,
            targetLocale,
          }
        : undefined,
    [
      alreadyLocalized,
      awaitingRepositoryDescriptions,
      originalText,
      resolvedSourceLocale,
      resourceId,
      sourceMatchesTarget,
      targetLocale,
    ]
  );

  useEffect(() => {
    const requestId = ++translationRequestRef.current;
    if (
      !isVisible ||
      !isRepositoryResolved ||
      !translationRequest ||
      alreadyLocalized ||
      !isTauriRuntime()
    ) {
      return;
    }

    setIsLoading(true);
    setError(undefined);

    void invoke<TranslationResult | null>("get_cached_skill_description_translation", {
      request: translationRequest,
    })
      .then(async (cached) => {
        if (translationRequestRef.current !== requestId) return;
        if (cached) {
          setTranslation(cached);
          return;
        }

        try {
          const onDevice = await invoke<TranslationResult>(
            "translate_skill_description_on_device",
            { request: translationRequest }
          );
          if (translationRequestRef.current === requestId) setTranslation(onDevice);
        } catch {
          // macOS 15가 아니거나 번역 언어 팩이 없으면 원문을 조용히 유지한다.
        }
      })
      .catch(() => {
        // 캐시 조회 실패 역시 원문 표시를 막지 않는다.
      })
      .finally(() => {
        if (translationRequestRef.current === requestId) setIsLoading(false);
      });
  }, [alreadyLocalized, isRepositoryResolved, isVisible, translationRequest]);

  const requestApiTranslation = useCallback(async () => {
    if (!translationRequest || !isTauriRuntime() || apiTranslationInFlightRef.current) return;

    if (!apiConfirming) {
      setApiConfirming(true);
      return;
    }

    apiTranslationInFlightRef.current = true;
    setIsLoading(true);
    setError(undefined);
    try {
      const result = await invoke<TranslationResult>(
        "translate_skill_description_with_api",
        { request: translationRequest }
      );
      setTranslation(result);
      setIsShowingOriginal(false);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      apiTranslationInFlightRef.current = false;
      setIsLoading(false);
      setApiConfirming(false);
    }
  }, [apiConfirming, translationRequest]);

  const toggleOriginal = useCallback(() => {
    setIsShowingOriginal((showing) => !showing);
  }, []);

  const displayText =
    translation && !isShowingOriginal ? translation.translatedText : originalText;

  return {
    displayText,
    originalText,
    translation: translation?.translatedText,
    engine: translation?.engine,
    isShowingOriginal,
    isLoading,
    error,
    apiConfirming,
    canTranslate,
    isEnglishFallback,
    requestApiTranslation,
    toggleOriginal,
    cancelApiConfirmation: () => setApiConfirming(false),
  };
}

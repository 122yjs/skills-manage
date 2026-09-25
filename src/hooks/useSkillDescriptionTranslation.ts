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
  onDeviceError?: string;
  loadingEngine?: "apple" | "api";
  unavailableReason?: string;
  requestOnDeviceTranslation: () => Promise<void>;
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

/**
 * 기기 번역이 언어 팩 부족 등으로 실패한 언어 조합을 기억한다.
 * 카드가 화면에 보일 때마다 같은 요청을 반복하면 macOS가 언어 다운로드
 * 창을 계속 띄우기 때문에, 한 번 실패한 조합은 다시 시도하지 않는다.
 */
const unavailableOnDevicePairs = new Map<string, string>();

/**
 * 같은 언어 조합의 첫 기기 번역 요청만 실제로 보내고, 동시에 보이는 다른
 * 카드들은 그 결과를 기다린다. 화면에 카드가 여러 개 있어도 macOS 언어
 * 다운로드 창이 한 번만 뜨게 한다.
 */
const inFlightOnDevicePairs = new Map<string, Promise<unknown>>();

/**
 * 표시 언어가 실제로 바뀌면 이전 언어 조합의 실패 기록은 의미가 없다.
 * 카드가 새로 생길 때가 아니라 언어가 바뀐 순간에만 기록을 비운다.
 */
let lastKnownTargetLocale: string | undefined;

function languagePairKey(sourceLocale: string | null | undefined, targetLocale: string): string {
  const source = sourceLocale ? normalizeLocale(sourceLocale).split("-")[0] : "auto";
  return `${source}->${targetLocale.split("-")[0]}`;
}

/** 테스트와 언어 팩 설치 후 재시도를 위해 차단 기록을 비운다. */
export function resetUnavailableOnDeviceTranslations(): void {
  unavailableOnDevicePairs.clear();
  inFlightOnDevicePairs.clear();
  lastKnownTargetLocale = undefined;
}

export function useSkillDescriptionTranslation({
  resourceId,
  filePath,
  localizedDescriptions = {},
  sourceLocale,
  description,
  isVisible = true,
}: UseSkillDescriptionTranslationOptions): SkillDescriptionTranslationState {
  const { i18n, t } = useTranslation();
  const targetLocale = normalizeLocale(i18n.resolvedLanguage ?? i18n.language ?? "en");

  // 사용자가 언어 팩을 설치한 뒤 표시 언어를 다시 고르면 자동 번역이 되살아난다.
  if (lastKnownTargetLocale !== targetLocale) {
    if (lastKnownTargetLocale !== undefined) {
      unavailableOnDevicePairs.clear();
      inFlightOnDevicePairs.clear();
    }
    lastKnownTargetLocale = targetLocale;
  }

  const [repositoryDescriptions, setRepositoryDescriptions] = useState<RepositoryDescriptionsResult>();
  const [resolvedRepositoryKey, setResolvedRepositoryKey] = useState<string>();
  const [translation, setTranslation] = useState<TranslationResult>();
  const [isShowingOriginal, setIsShowingOriginal] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [loadingEngine, setLoadingEngine] = useState<"apple" | "api">();
  const [onDeviceError, setOnDeviceError] = useState<string>();
  const [error, setError] = useState<string>();
  const [apiConfirming, setApiConfirming] = useState(false);
  const repositoryRequestRef = useRef(0);
  const translationRequestRef = useRef(0);
  const manualTranslationInFlightRef = useRef(false);
  const identityRef = useRef(0);

  useEffect(() => () => {
    identityRef.current += 1;
    translationRequestRef.current += 1;
    repositoryRequestRef.current += 1;
  }, []);

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
    identityRef.current += 1;
    manualTranslationInFlightRef.current = false;
    setOnDeviceError(undefined);
    setLoadingEngine(undefined);
    setIsLoading(false);
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
  const unavailableReason = !isTauriRuntime()
    ? t("common.translationDesktopOnly")
    : alreadyLocalized || sourceMatchesTarget
      ? t("common.translationAlreadyLocalized")
      : awaitingRepositoryDescriptions
        ? t("common.translationChecking")
        : undefined;
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
    if (manualTranslationInFlightRef.current) return;
    setIsLoading(false);
    setLoadingEngine(undefined);
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

        const pairKey = languagePairKey(
          translationRequest.sourceLocale,
          translationRequest.targetLocale
        );
        if (unavailableOnDevicePairs.has(pairKey)) {
          setOnDeviceError(unavailableOnDevicePairs.get(pairKey));
          return;
        }

        // 같은 언어 조합이 이미 진행 중이면 그 결과를 기다렸다가,
        // 실패한 조합이면 새 요청을 보내지 않는다.
        const pending = inFlightOnDevicePairs.get(pairKey);
        if (pending) {
          await pending.catch(() => undefined);
          if (translationRequestRef.current !== requestId) return;
          if (unavailableOnDevicePairs.has(pairKey)) {
            setOnDeviceError(unavailableOnDevicePairs.get(pairKey));
            return;
          }
        }

        try {
          setLoadingEngine("apple");
          const onDeviceRequest = invoke<TranslationResult>(
            "translate_skill_description_on_device",
            { request: translationRequest }
          );
          inFlightOnDevicePairs.set(pairKey, onDeviceRequest);
          const onDevice = await onDeviceRequest.finally(() => {
            if (inFlightOnDevicePairs.get(pairKey) === onDeviceRequest) {
              inFlightOnDevicePairs.delete(pairKey);
            }
          });
          if (translationRequestRef.current === requestId) setTranslation(onDevice);
        } catch (cause) {
          // macOS 15가 아니거나 번역 언어 팩이 없으면 원문을 유지하고 실패 이유를 표시한다.
          // 같은 언어 조합의 자동 재시도는 막되 사용자가 직접 재시도할 수 있다.
          const message = cause instanceof Error ? cause.message : String(cause);
          unavailableOnDevicePairs.set(pairKey, message);
          if (translationRequestRef.current === requestId) setOnDeviceError(message);
        }
      })
      .catch(() => {
        // 캐시 조회 실패 역시 원문 표시를 막지 않는다.
      })
      .finally(() => {
        if (translationRequestRef.current === requestId) {
          setIsLoading(false);
          setLoadingEngine(undefined);
        }
      });
  }, [alreadyLocalized, isRepositoryResolved, isVisible, translationRequest]);

  // 명시적인 재시도는 이 카드에만 적용하고 자동 재시도 차단은 유지한다.
  const requestTranslation = useCallback(async (engine: "apple" | "api") => {
    if (!translationRequest || !isTauriRuntime() || manualTranslationInFlightRef.current) return;
    manualTranslationInFlightRef.current = true;
    const identity = identityRef.current;
    // 늦게 도착한 자동 번역이 사용자의 선택을 덮어쓰지 않게 한다.
    translationRequestRef.current += 1;
    setIsLoading(true);
    setLoadingEngine(engine);
    setError(undefined);
    if (engine === "apple") setOnDeviceError(undefined);
    try {
      const result = await invoke<TranslationResult>(
        engine === "apple" ? "translate_skill_description_on_device" : "translate_skill_description_with_api",
        { request: translationRequest }
      );
      if (identityRef.current !== identity) return;
      setTranslation(result);
      setIsShowingOriginal(false);
      if (engine === "apple") {
        unavailableOnDevicePairs.delete(languagePairKey(translationRequest.sourceLocale, translationRequest.targetLocale));
      }
    } catch (cause) {
      if (identityRef.current !== identity) return;
      const message = cause instanceof Error ? cause.message : String(cause);
      if (engine === "apple") setOnDeviceError(message);
      else setError(message);
    } finally {
      if (identityRef.current === identity) {
        manualTranslationInFlightRef.current = false;
        setIsLoading(false);
        setLoadingEngine(undefined);
        setApiConfirming(false);
      }
    }
  }, [translationRequest]);

  const requestOnDeviceTranslation = useCallback(
    () => requestTranslation("apple"), [requestTranslation]
  );
  const requestApiTranslation = useCallback(async () => {
    if (!translationRequest || !isTauriRuntime() || manualTranslationInFlightRef.current) return;
    if (!apiConfirming) {
      setApiConfirming(true);
      return;
    }
    await requestTranslation("api");
  }, [apiConfirming, translationRequest, requestTranslation]);

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
    onDeviceError,
    loadingEngine,
    unavailableReason,
    requestOnDeviceTranslation,
    apiConfirming,
    canTranslate,
    isEnglishFallback,
    requestApiTranslation,
    toggleOriginal,
    cancelApiConfirmation: () => setApiConfirming(false),
  };
}

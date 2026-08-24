import type {
  LocalizedSkillDescriptions,
  ResolvedSkillDescription,
  SkillDescriptionResolutionSource,
} from "@/types";

function normalizeLocale(locale: string): string {
  return locale.trim().replace(/_/g, "-").toLowerCase();
}

function normalizedDescriptions(
  descriptions: LocalizedSkillDescriptions
): Map<string, { locale: string; text: string }> {
  const normalized = new Map<string, { locale: string; text: string }>();

  for (const [locale, description] of Object.entries(descriptions)) {
    const normalizedLocale = normalizeLocale(locale);
    const text = description.trim();
    if (normalizedLocale && text) {
      normalized.set(normalizedLocale, { locale: normalizedLocale, text });
    }
  }

  return normalized;
}

function findLocalizedDescription(
  descriptions: Map<string, { locale: string; text: string }>,
  locale: string,
  source: SkillDescriptionResolutionSource
): ResolvedSkillDescription | undefined {
  const normalizedLocale = normalizeLocale(locale);
  const exact = descriptions.get(normalizedLocale);
  if (exact) {
    return { ...exact, source };
  }

  const baseLocale = normalizedLocale.split("-")[0];
  const base = descriptions.get(baseLocale);
  if (base) {
    return { ...base, source };
  }

  const regional = [...descriptions.values()].find(
    (description) => description.locale.split("-")[0] === baseLocale
  );
  return regional ? { ...regional, source } : undefined;
}

/**
 * 짧은 스킬 설명을 `현재 언어 → 영어 → 기존 원문` 순서로 고른다.
 * 번역 API나 기기 번역은 호출하지 않는 순수 함수라 언어 변경 시 안전하게 쓸 수 있다.
 */
export function resolveSkillDescription({
  locale,
  localizedDescriptions = {},
  legacyDescription,
}: {
  locale: string;
  localizedDescriptions?: LocalizedSkillDescriptions;
  legacyDescription?: string | null;
}): ResolvedSkillDescription | undefined {
  const descriptions = normalizedDescriptions(localizedDescriptions);

  const requested = findLocalizedDescription(
    descriptions,
    locale,
    "requested-locale"
  );
  if (requested) {
    return requested;
  }

  const english = findLocalizedDescription(descriptions, "en", "english-fallback");
  if (english) {
    return english;
  }

  const original = legacyDescription?.trim();
  return original
    ? { text: original, source: "original-fallback" }
    : undefined;
}

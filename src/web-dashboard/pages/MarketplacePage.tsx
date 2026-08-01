import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ExternalLinkIcon, SearchIcon, StoreIcon } from "lucide-react";

import { Input } from "@/components/ui/input";
import {
  ALL_TAGS,
  OFFICIAL_PUBLISHERS,
  RECOMMENDED_SKILLS,
  TAG_LABELS,
  type SkillTag,
} from "@/data/officialSources";
import { cn } from "@/lib/utils";

import { EmptyPanel, SkillRow } from "../SkillRow";
import type { DashboardSnapshot } from "../types";

type MarketplaceTab = "recommended" | "official" | "cached";

function TabButton({
  active,
  count,
  label,
  onClick,
}: {
  active: boolean;
  count: number;
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      role="tab"
      aria-selected={active}
      onClick={onClick}
      className={cn(
        "cursor-pointer border-b-2 px-1 py-2 text-sm transition-colors",
        active
          ? "border-primary font-medium text-primary"
          : "border-transparent text-muted-foreground hover:text-foreground",
      )}
    >
      {label}
      <span className="ml-1.5 font-mono text-[10px] opacity-70">{count}</span>
    </button>
  );
}

/**
 * 읽기 전용 웹 마켓플레이스.
 * 데스크톱과 같은 추천/공식 목록은 번들에서 즉시 읽고, DB 동기화 결과는
 * 별도의 "내 소스" 탭에서 보여준다. 따라서 캐시가 비어도 기본 목록은 보인다.
 */
export function MarketplacePage({ snapshot }: { snapshot: DashboardSnapshot }) {
  const { t, i18n } = useTranslation();
  const [activeTab, setActiveTab] = useState<MarketplaceTab>("recommended");
  const [query, setQuery] = useState("");
  const [selectedTag, setSelectedTag] = useState<SkillTag | null>(null);
  const [selectedSourceId, setSelectedSourceId] = useState<string | null>(null);

  const normalizedQuery = query.trim().toLowerCase();
  const recommendedSkills = useMemo(
    () =>
      RECOMMENDED_SKILLS.filter(
        (skill) => !selectedTag || skill.tags.includes(selectedTag),
      ).filter(
        (skill) =>
          !normalizedQuery ||
          skill.name.toLowerCase().includes(normalizedQuery) ||
          skill.description.toLowerCase().includes(normalizedQuery) ||
          skill.publisher.toLowerCase().includes(normalizedQuery),
      ),
    [normalizedQuery, selectedTag],
  );

  const publishers = useMemo(
    () =>
      OFFICIAL_PUBLISHERS.filter(
        (publisher) =>
          !normalizedQuery ||
          publisher.name.toLowerCase().includes(normalizedQuery) ||
          publisher.slug.toLowerCase().includes(normalizedQuery) ||
          publisher.repos.some((repo) =>
            repo.fullName.toLowerCase().includes(normalizedQuery),
          ),
      ),
    [normalizedQuery],
  );

  const activeSourceId =
    selectedSourceId ??
    snapshot.marketplaceSources.find((source) => source.skillCount > 0)?.id ??
    snapshot.marketplaceSources[0]?.id ??
    null;
  const cachedSkills = snapshot.marketplaceSkills.filter(
    (skill) =>
      skill.registryId === activeSourceId &&
      (!normalizedQuery ||
        skill.name.toLowerCase().includes(normalizedQuery) ||
        skill.description?.toLowerCase().includes(normalizedQuery)),
  );

  const language = (i18n.resolvedLanguage ?? i18n.language).startsWith("zh")
    ? "zh"
    : "en";

  return (
    <div className="space-y-4">
      <div
        className="flex gap-5 border-b border-border"
        role="tablist"
        aria-label={t("webDashboard.marketplace.tabsLabel")}
      >
        <TabButton
          active={activeTab === "recommended"}
          count={RECOMMENDED_SKILLS.length}
          label={t("webDashboard.marketplace.recommended")}
          onClick={() => setActiveTab("recommended")}
        />
        <TabButton
          active={activeTab === "official"}
          count={OFFICIAL_PUBLISHERS.length}
          label={t("webDashboard.marketplace.official")}
          onClick={() => setActiveTab("official")}
        />
        <TabButton
          active={activeTab === "cached"}
          count={snapshot.marketplaceSkills.length}
          label={t("webDashboard.marketplace.cached")}
          onClick={() => setActiveTab("cached")}
        />
      </div>

      <div className="relative max-w-xl">
        <SearchIcon
          className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
          aria-hidden="true"
        />
        <Input
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t("marketplace.searchPlaceholder")}
          aria-label={t("marketplace.searchPlaceholder")}
          className="pl-9"
        />
      </div>

      {activeTab === "recommended" && (
        <>
          <div className="flex flex-wrap gap-1.5">
            <button
              onClick={() => setSelectedTag(null)}
              className={cn(
                "cursor-pointer rounded-full px-2.5 py-1 text-xs ring-1 transition-colors",
                selectedTag === null
                  ? "bg-primary/15 text-primary ring-primary/30"
                  : "bg-muted/30 text-muted-foreground ring-border hover:bg-muted/60",
              )}
            >
              {t("webDashboard.marketplace.all")}
            </button>
            {ALL_TAGS.map((tag) => (
              <button
                key={tag}
                onClick={() => setSelectedTag(tag)}
                className={cn(
                  "cursor-pointer rounded-full px-2.5 py-1 text-xs ring-1 transition-colors",
                  selectedTag === tag
                    ? "bg-primary/15 text-primary ring-primary/30"
                    : "bg-muted/30 text-muted-foreground ring-border hover:bg-muted/60",
                )}
              >
                {TAG_LABELS[tag][language]}
              </button>
            ))}
          </div>

          {recommendedSkills.length === 0 ? (
            <EmptyPanel>{t("webDashboard.marketplace.noResults")}</EmptyPanel>
          ) : (
            <div className="grid gap-2 xl:grid-cols-2">
              {recommendedSkills.map((skill) => (
                <SkillRow
                  key={`${skill.repoFullName}:${skill.name}`}
                  skill={{
                    id: `${skill.repoFullName}:${skill.name}`,
                    name: skill.name,
                    description: skill.description,
                    path: skill.downloadUrl,
                    sourceLabel: skill.publisher,
                    linkType: null,
                  }}
                />
              ))}
            </div>
          )}
        </>
      )}

      {activeTab === "official" && (
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {publishers.map((publisher) => (
            <article
              key={publisher.slug}
              className="rounded-xl border border-border bg-card/70 p-4"
            >
              <div className="flex items-start justify-between gap-3">
                <div className="min-w-0">
                  <h2 className="truncate text-sm font-semibold">{publisher.name}</h2>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {t("webDashboard.marketplace.publisherSummary", {
                      skills: publisher.totalSkills,
                      repos: publisher.repos.length,
                    })}
                  </p>
                </div>
                <StoreIcon className="size-4 shrink-0 text-primary" aria-hidden="true" />
              </div>
              <div className="mt-3 space-y-1.5">
                {publisher.repos.map((repo) => (
                  <a
                    key={repo.fullName}
                    href={repo.url}
                    target="_blank"
                    rel="noreferrer"
                    className="flex items-center justify-between gap-2 rounded-md bg-muted/25 px-2.5 py-2 text-xs text-muted-foreground transition-colors hover:bg-muted/50 hover:text-foreground"
                  >
                    <span className="truncate">{repo.fullName}</span>
                    <ExternalLinkIcon className="size-3 shrink-0" aria-hidden="true" />
                  </a>
                ))}
              </div>
            </article>
          ))}
        </div>
      )}

      {activeTab === "cached" && (
        <>
          {snapshot.marketplaceSources.length > 0 && (
            <div
              className="flex flex-wrap gap-2"
              role="tablist"
              aria-label={t("webDashboard.sections.sources")}
            >
              {snapshot.marketplaceSources.map((source) => (
                <button
                  key={source.id}
                  role="tab"
                  aria-selected={source.id === activeSourceId}
                  onClick={() => setSelectedSourceId(source.id)}
                  className={cn(
                    "cursor-pointer rounded-full px-3 py-1.5 text-xs ring-1 transition-colors",
                    source.id === activeSourceId
                      ? "bg-primary/15 text-primary ring-primary/30"
                      : "bg-muted/40 text-muted-foreground ring-border hover:bg-muted/70",
                  )}
                  title={source.url}
                >
                  {source.name}
                  <span className="ml-1.5 font-mono text-[10px] opacity-70">
                    {source.skillCount}
                  </span>
                </button>
              ))}
            </div>
          )}

          {cachedSkills.length === 0 ? (
            <EmptyPanel>{t("webDashboard.empty.marketplaceSource")}</EmptyPanel>
          ) : (
            <div className="grid gap-2 xl:grid-cols-2">
              {cachedSkills.map((skill) => (
                <SkillRow
                  key={skill.id}
                  skill={{
                    id: skill.id,
                    name: skill.name,
                    description: skill.description,
                    path: null,
                    sourceLabel: null,
                    linkType: null,
                  }}
                  trailing={
                    skill.isInstalled ? (
                      <span className="inline-flex items-center gap-1 text-[var(--ctp-green)]">
                        <StoreIcon className="size-3" aria-hidden="true" />
                        {t("webDashboard.installed")}
                      </span>
                    ) : undefined
                  }
                />
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}

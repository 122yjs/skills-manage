import { useState, type ReactNode } from "react";
import { Blocks, Folder, LibraryBig, Search } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { buildSearchText, normalizeSearchQuery } from "@/lib/search";
import { cn } from "@/lib/utils";

interface LibraryFolderNavProps {
  folders: Array<{ name: string; relativePath: string; skillCount: number }>;
  totalCount: number;
  rootCount: number;
  selectedPath: string | null;
  onSelect: (path: string | null) => void;
}

/** 폴더를 선택해도 목록과 검색 맥락을 같은 화면에 유지한다. */
export function LibraryFolderNav({
  folders,
  totalCount,
  rootCount,
  selectedPath,
  onSelect,
}: LibraryFolderNavProps) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const normalizedQuery = normalizeSearchQuery(query);
  const visibleFolders = folders.filter((folder) =>
    buildSearchText([folder.name, folder.relativePath]).includes(normalizedQuery)
  );

  function renderFolder(path: string | null, name: string, count: number, icon: ReactNode) {
    return (
      <button
        key={path ?? "all"}
        type="button"
        aria-label={t("libraryBrowser.openFolder", { name })}
        aria-pressed={selectedPath === path}
        title={name}
        onClick={() => onSelect(path)}
        className={cn(
          "flex min-h-9 shrink-0 items-center gap-2 rounded-lg px-2.5 text-sm transition-colors lg:w-full",
          "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
          selectedPath === path
            ? "bg-primary/10 font-medium text-primary ring-1 ring-primary/20"
            : "text-muted-foreground hover:bg-muted/50 hover:text-foreground"
        )}
      >
        {icon}
        <span className="max-w-40 flex-1 truncate text-left">{name}</span>
        <span className="text-xs tabular-nums opacity-80">{count}</span>
      </button>
    );
  }

  return (
    <aside
      aria-label={t("libraryBrowser.folders")}
      className="flex shrink-0 flex-col border-b border-border bg-card/40 lg:w-48 lg:min-h-0 lg:border-r lg:border-b-0"
    >
      <div className="flex items-center gap-3 px-3 pt-3 lg:block lg:space-y-3 lg:pt-4 lg:pb-3">
        <h2 className="shrink-0 px-1 text-xs font-semibold text-muted-foreground">{t("libraryBrowser.folders")}</h2>
        <div className="relative min-w-0 flex-1">
          <Search aria-hidden="true" className="pointer-events-none absolute left-2.5 top-2.5 size-3.5 text-muted-foreground" />
          <Input
            aria-label={t("libraryBrowser.searchFolders")}
            placeholder={t("libraryBrowser.searchFolders")}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            className="h-9 bg-background/60 pl-8 text-xs"
          />
        </div>
      </div>
      <div className="flex gap-1 overflow-auto p-2 lg:min-h-0 lg:flex-1 lg:flex-col lg:justify-start lg:gap-1 lg:px-3">
        {renderFolder(null, t("libraryBrowser.allSkills"), totalCount, <LibraryBig className="size-4 shrink-0" />)}
        {renderFolder("", t("libraryBrowser.rootSkills"), rootCount, <Blocks className="size-4 shrink-0" />)}
        <div className="hidden border-t border-border/70 my-2 lg:block" />
        {visibleFolders.map((folder) => renderFolder(folder.relativePath, folder.name, folder.skillCount, <Folder className="size-4 shrink-0" />))}
        {normalizedQuery && visibleFolders.length === 0 && (
          <p className="p-2 text-xs text-muted-foreground">{t("libraryBrowser.noFolders")}</p>
        )}
      </div>
    </aside>
  );
}

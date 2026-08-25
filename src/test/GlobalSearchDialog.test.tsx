import { render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { GlobalSearchDialog } from "@/components/layout/GlobalSearchDialog";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { useCollectionStore } from "@/stores/collectionStore";
import { useDiscoverStore } from "@/stores/discoverStore";
import { usePlatformStore } from "@/stores/platformStore";
import { useSkillStore } from "@/stores/skillStore";

vi.mock("@/stores/centralSkillsStore", () => ({ useCentralSkillsStore: vi.fn() }));
vi.mock("@/stores/collectionStore", () => ({ useCollectionStore: vi.fn() }));
vi.mock("@/stores/discoverStore", () => ({ useDiscoverStore: vi.fn() }));
vi.mock("@/stores/platformStore", () => ({ usePlatformStore: vi.fn() }));
vi.mock("@/stores/skillStore", () => ({ useSkillStore: vi.fn() }));

const getSkillsByAgent = vi.fn();

describe("GlobalSearchDialog", () => {
  beforeEach(() => {
    global.ResizeObserver = class ResizeObserver {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
    Element.prototype.scrollIntoView = vi.fn();
    getSkillsByAgent.mockReset();
    vi.mocked(useCentralSkillsStore).mockImplementation((selector) =>
      selector({ skills: [] } as never)
    );
    vi.mocked(useCollectionStore).mockImplementation((selector) =>
      selector({ collections: [] } as never)
    );
    vi.mocked(useDiscoverStore).mockImplementation((selector) =>
      selector({ discoveredProjects: [] } as never)
    );
    vi.mocked(usePlatformStore).mockImplementation((selector) =>
      selector({
        agents: [
          {
            id: "codex",
            display_name: "Codex CLI",
            category: "coding",
            global_skills_dir: "~/.agents/skills",
            is_detected: true,
            is_builtin: true,
            is_enabled: true,
          },
          {
            id: "dexto",
            display_name: "Dexto",
            category: "coding",
            global_skills_dir: "~/.agents/skills",
            is_detected: false,
            is_builtin: true,
            is_enabled: true,
          },
          {
            id: "cursor",
            display_name: "Cursor",
            category: "coding",
            global_skills_dir: "~/.cursor/skills",
            is_detected: true,
            is_builtin: true,
            is_enabled: true,
          },
        ],
      } as never)
    );
    vi.mocked(useSkillStore).mockImplementation((selector) =>
      selector({
        skillsByAgent: {
          codex: [
            {
              id: "shared-tool",
              row_id: "codex::shared-tool",
              name: "Shared Tool",
              description: "Codex plugin skill",
              file_path: "~/.codex/plugins/cache/ponytail/skills/shared-tool/SKILL.md",
              dir_path: "~/.codex/plugins/cache/ponytail/skills/shared-tool",
              link_type: "native",
              is_central: false,
              source_kind: "plugin",
              source_label: "ponytail@ponytail",
              is_read_only: true,
            },
          ],
          cursor: [
            {
              id: "shared-tool",
              row_id: "cursor::shared-tool",
              name: "Shared Tool",
              description: "Cursor compatibility skill",
              file_path: "~/.codex/skills/shared-tool/SKILL.md",
              dir_path: "~/.codex/skills/shared-tool",
              link_type: "native",
              is_central: false,
              source_kind: "compatibility",
              is_read_only: true,
            },
          ],
        },
        getSkillsByAgent,
      } as never)
    );
  });

  it("감지된 공용 호환 도구는 찾고 미설치 도구는 숨긴다", () => {
    render(
      <MemoryRouter>
        <GlobalSearchDialog open onOpenChange={vi.fn()} onAction={vi.fn()} />
      </MemoryRouter>
    );

    expect(screen.getAllByText("Codex CLI").length).toBeGreaterThan(0);
    expect(screen.queryByText("Dexto")).not.toBeInTheDocument();
  });

  it("플랫폼 스킬을 이름으로 합쳐 플랫폼과 출처 배지를 표시한다", () => {
    render(
      <MemoryRouter>
        <GlobalSearchDialog open onOpenChange={vi.fn()} onAction={vi.fn()} />
      </MemoryRouter>
    );

    const skillItems = screen.getAllByText("Shared Tool");
    expect(skillItems).toHaveLength(1);

    const item = skillItems[0].closest("[cmdk-item]");
    expect(item).not.toBeNull();
    expect(within(item as HTMLElement).getByText("Codex CLI")).toBeInTheDocument();
    expect(within(item as HTMLElement).getByText("Cursor")).toBeInTheDocument();
    expect(
      within(item as HTMLElement).getByText("ponytail@ponytail")
    ).toBeInTheDocument();
  });
});

import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { GlobalSearchDialog } from "@/components/layout/GlobalSearchDialog";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { useCollectionStore } from "@/stores/collectionStore";
import { useDiscoverStore } from "@/stores/discoverStore";
import { usePlatformStore } from "@/stores/platformStore";

vi.mock("@/stores/centralSkillsStore", () => ({ useCentralSkillsStore: vi.fn() }));
vi.mock("@/stores/collectionStore", () => ({ useCollectionStore: vi.fn() }));
vi.mock("@/stores/discoverStore", () => ({ useDiscoverStore: vi.fn() }));
vi.mock("@/stores/platformStore", () => ({ usePlatformStore: vi.fn() }));

describe("GlobalSearchDialog", () => {
  beforeEach(() => {
    global.ResizeObserver = class ResizeObserver {
      observe() {}
      unobserve() {}
      disconnect() {}
    };
    Element.prototype.scrollIntoView = vi.fn();
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
        ],
      } as never)
    );
  });

  it("감지된 공용 호환 도구는 찾고 미설치 도구는 숨긴다", () => {
    render(
      <MemoryRouter>
        <GlobalSearchDialog open onOpenChange={vi.fn()} onAction={vi.fn()} />
      </MemoryRouter>
    );

    expect(screen.getByText("Codex CLI")).toBeInTheDocument();
    expect(screen.queryByText("Dexto")).not.toBeInTheDocument();
  });
});

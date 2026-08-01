import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { GitHubRepoImportWizard } from "@/components/marketplace/GitHubRepoImportWizard";
import { useCollectionStore } from "@/stores/collectionStore";
import { useMarketplaceStore } from "@/stores/marketplaceStore";
import type { GitHubRepoImportResult, GitHubRepoPreview } from "@/types";

const preview: GitHubRepoPreview = {
  repo: {
    owner: "openai",
    repo: "skills",
    branch: "main",
    normalizedUrl: "https://github.com/openai/skills",
  },
  skills: [
    {
      sourcePath: "skills/docs",
      skillId: "docs",
      skillName: "Docs",
      description: "Documentation helper",
      rootDirectory: "skills",
      skillDirectoryName: "docs",
      downloadUrl: "https://example.com/docs/SKILL.md",
    },
  ],
};

const importResult: GitHubRepoImportResult = {
  repo: preview.repo,
  importedSkills: [
    {
      sourcePath: "skills/docs",
      originalSkillId: "docs",
      importedSkillId: "docs",
      skillName: "Docs",
      targetDirectory: "/tmp/skills/docs",
      resolution: "overwrite",
    },
  ],
  skippedSkills: [],
};

describe("GitHubRepoImportWizard repository collection", () => {
  const createCollectionFromSkills = vi.fn();
  const onImport = vi.fn();
  const onAfterImportSuccess = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    createCollectionFromSkills.mockResolvedValue({
      id: "collection-1",
      name: "openai/skills",
      created_at: "2026-08-01T00:00:00Z",
      updated_at: "2026-08-01T00:00:00Z",
    });
    onImport.mockResolvedValue(importResult);
    useCollectionStore.setState({ createCollectionFromSkills });
    useMarketplaceStore.setState((state) => ({
      githubImport: {
        ...state.githubImport,
        importProgress: null,
        importStartedAt: null,
        skillMarkdown: {
          "skills/docs": { status: "ready", content: "# Docs" },
        },
        aiSummaries: {},
      },
    }));
  });

  function renderWizard() {
    return render(
      <MemoryRouter>
        <GitHubRepoImportWizard
          open
          onOpenChange={vi.fn()}
          repoUrl="https://github.com/openai/skills"
          onRepoUrlChange={vi.fn()}
          preview={preview}
          previewError={null}
          isPreviewLoading={false}
          isImporting={false}
          importResult={null}
          onPreview={vi.fn()}
          onImport={onImport}
          onReset={vi.fn()}
          launcherLabel="Marketplace"
          onAfterImportSuccess={onAfterImportSuccess}
        />
      </MemoryRouter>,
    );
  }

  async function openConfirmStep() {
    renderWizard();
    fireEvent.click(screen.getByRole("button", { name: "检查导入内容" }));
    await screen.findByTestId("github-import-confirm-summary");
    expect(screen.getByText("创建“openai/skills”集合")).toBeInTheDocument();
    return screen.getByTestId("github-import-create-collection");
  }

  it("creates a checked repository collection from successfully imported skills", async () => {
    const checkbox = await openConfirmStep();
    expect(checkbox).toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "导入" }));

    await waitFor(() => {
      expect(createCollectionFromSkills).toHaveBeenCalledWith(
        "openai/skills",
        "从 GitHub 仓库 openai/skills 导入的技能",
        ["docs"],
      );
    });
    expect(onAfterImportSuccess).toHaveBeenCalledWith(importResult);
  });

  it("keeps the existing import behavior when collection creation is unchecked", async () => {
    const checkbox = await openConfirmStep();
    fireEvent.click(checkbox);
    expect(checkbox).not.toBeChecked();

    fireEvent.click(screen.getByRole("button", { name: "导入" }));

    await waitFor(() => expect(onAfterImportSuccess).toHaveBeenCalledWith(importResult));
    expect(createCollectionFromSkills).not.toHaveBeenCalled();
  });

  it("keeps import success when collection creation fails", async () => {
    createCollectionFromSkills.mockRejectedValueOnce(new Error("collection failed"));
    await openConfirmStep();

    fireEvent.click(screen.getByRole("button", { name: "导入" }));

    await waitFor(() => expect(onAfterImportSuccess).toHaveBeenCalledWith(importResult));
  });
});

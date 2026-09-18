import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { GitHubRepoImportWizard } from "@/components/marketplace/GitHubRepoImportWizard";
import { useCollectionStore } from "@/stores/collectionStore";
import { useMarketplaceStore } from "@/stores/marketplaceStore";
import type {
  GitHubImportFailure,
  GitHubRepoImportResult,
  GitHubRepoPreview,
  GitHubSkillPreview,
} from "@/types";

import zh from "@/i18n/locales/zh.json";

const renameActionLabel = zh.marketplace.githubImportStatusChangeToRename;

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

const conflictRepo: GitHubRepoPreview["repo"] = {
  owner: "anthropics",
  repo: "skills",
  branch: "main",
  normalizedUrl: "https://github.com/anthropics/skills",
};

const docsSkill: GitHubSkillPreview = {
  sourcePath: "skills/docs/SKILL.md",
  skillId: "docs",
  skillName: "OpenAI Docs",
  description: "Docs helper",
  rootDirectory: "skills",
  skillDirectoryName: "docs",
  downloadUrl: "https://example.com/docs/SKILL.md",
  conflict: null,
};

const extraSkill: GitHubSkillPreview = {
  sourcePath: "skills/extra/SKILL.md",
  skillId: "extra",
  skillName: "Extra Skill",
  description: null,
  rootDirectory: "skills",
  skillDirectoryName: "extra",
  downloadUrl: "https://example.com/extra/SKILL.md",
  conflict: null,
};

const thirdSkill: GitHubSkillPreview = {
  sourcePath: "skills/third/SKILL.md",
  skillId: "third",
  skillName: "Third Skill",
  description: null,
  rootDirectory: "skills",
  skillDirectoryName: "third",
  downloadUrl: "https://example.com/third/SKILL.md",
  conflict: null,
};

const legacySkill: GitHubSkillPreview = {
  sourcePath: "skills/legacy/SKILL.md",
  skillId: "legacy-skill",
  skillName: "Legacy Skill",
  description: null,
  rootDirectory: "skills",
  skillDirectoryName: "legacy-skill",
  downloadUrl: "https://example.com/legacy/SKILL.md",
  conflict: null,
};

type ConflictKind = NonNullable<GitHubSkillPreview["conflict"]>["conflictKind"];

/** Fixture for the local skill that already owns the incoming id. */
function conflictSkill(kind: ConflictKind): GitHubSkillPreview {
  return {
    sourcePath: "skills/.system/skill-creator/SKILL.md",
    skillId: "skill-creator",
    skillName: "Skill Creator",
    description: "Create skills safely",
    rootDirectory: "skills/.system",
    skillDirectoryName: "skill-creator",
    downloadUrl: "https://example.com/skill-creator/SKILL.md",
    conflict: {
      existingSkillId: "skill-creator",
      existingName: "Skill Creator",
      existingCanonicalPath: "/Users/test/.agents/skills/skill-creator",
      existingPath: "/Users/test/.claude/skills/skill-creator",
      conflictKind: kind,
      proposedSkillId: "skill-creator",
      proposedName: "Skill Creator",
    },
  };
}

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

describe("GitHubRepoImportWizard conflict and failure handling", () => {
  const onImport = vi.fn();
  const onAfterImportSuccess = vi.fn();
  const onClearImportFailure = vi.fn();

  type WizardOverrides = Partial<{
    importFailure: GitHubImportFailure | null;
    importResult: GitHubRepoImportResult | null;
  }>;

  beforeEach(() => {
    vi.clearAllMocks();
    useMarketplaceStore.setState((state) => ({
      githubImport: {
        ...state.githubImport,
        importProgress: null,
        importStartedAt: null,
        skillMarkdown: {},
        aiSummaries: {},
      },
    }));
  });

  function renderConflictWizard(options: {
    preview: GitHubRepoPreview;
    importResult?: GitHubRepoImportResult | null;
    importFailure?: GitHubImportFailure | null;
  }) {
    const element = (overrides: WizardOverrides = {}) => (
      <MemoryRouter>
        <GitHubRepoImportWizard
          open
          onOpenChange={vi.fn()}
          repoUrl="https://github.com/anthropics/skills"
          onRepoUrlChange={vi.fn()}
          preview={options.preview}
          previewError={null}
          isPreviewLoading={false}
          isImporting={false}
          importResult={overrides.importResult ?? options.importResult ?? null}
          importFailure={overrides.importFailure ?? options.importFailure ?? null}
          onPreview={vi.fn()}
          onImport={onImport}
          onReset={vi.fn()}
          onClearImportFailure={onClearImportFailure}
          launcherLabel="Marketplace"
          onAfterImportSuccess={onAfterImportSuccess}
        />
      </MemoryRouter>
    );
    const result = render(element());
    return {
      rerender: (overrides: WizardOverrides) => result.rerender(element(overrides)),
    };
  }

  it("defaults a non-central conflict to skip with its path, source and preservation reason", async () => {
    renderConflictWizard({
      preview: { repo: conflictRepo, skills: [conflictSkill("non_central")] },
    });

    const detail = await screen.findByTestId("github-import-detail-pane");
    expect(
      within(detail).getByText("本地已存在「Skill Creator」，默认跳过不写入"),
    ).toBeInTheDocument();
    expect(
      within(detail).getByTestId("github-import-conflict-existing-path"),
    ).toHaveTextContent("/Users/test/.claude/skills/skill-creator");
    expect(
      within(detail).getByTestId("github-import-conflict-incoming-path"),
    ).toHaveTextContent("skills/.system/skill-creator/SKILL.md");
    expect(
      within(detail).getByTestId("github-import-conflict-reason"),
    ).toHaveTextContent("中央技能库之外的平台技能已使用该 ID");
    expect(
      within(detail).queryByRole("button", { name: "改为覆盖" }),
    ).not.toBeInTheDocument();
    expect(
      within(detail).getByRole("button", { name: renameActionLabel }),
    ).toBeInTheDocument();
  });

  it("explains unmanaged target folders without offering overwrite", async () => {
    renderConflictWizard({
      preview: { repo: conflictRepo, skills: [conflictSkill("unmanaged_path")] },
    });

    const detail = await screen.findByTestId("github-import-detail-pane");
    expect(
      within(detail).getByTestId("github-import-conflict-reason"),
    ).toHaveTextContent("目标文件夹已存在于磁盘上且没有管理记录");
    expect(
      within(detail).queryByRole("button", { name: "改为覆盖" }),
    ).not.toBeInTheDocument();
  });

  it("still offers overwrite for a central conflict", async () => {
    renderConflictWizard({
      preview: { repo: conflictRepo, skills: [conflictSkill("central")] },
    });

    const detail = await screen.findByTestId("github-import-detail-pane");
    fireEvent.click(within(detail).getByRole("button", { name: "改为覆盖" }));

    await waitFor(() =>
      expect(within(detail).getByText("将覆盖本地「Skill Creator」")).toBeInTheDocument(),
    );
  });

  it("focuses and selects the current installation ID when rename starts", async () => {
    renderConflictWizard({
      preview: { repo: conflictRepo, skills: [conflictSkill("non_central")] },
    });
    const detail = await screen.findByTestId("github-import-detail-pane");
    fireEvent.click(within(detail).getByRole("button", { name: renameActionLabel }));
    const input = screen.getByPlaceholderText("新的安装 ID") as HTMLInputElement;
    expect(input).toHaveFocus();
    expect(input.selectionStart).toBe(0);
    expect(input.selectionEnd).toBe(input.value.length);
  });

  it("blocks review when a renamed id is not unique among the selected targets", async () => {
    renderConflictWizard({
      preview: {
        repo: conflictRepo,
        skills: [conflictSkill("non_central"), docsSkill],
      },
    });

    const detail = await screen.findByTestId("github-import-detail-pane");
    fireEvent.click(within(detail).getByRole("button", { name: renameActionLabel }));
    fireEvent.change(screen.getByPlaceholderText("新的安装 ID"), {
      target: { value: "Docs" },
    });
    fireEvent.click(screen.getByRole("button", { name: "确认" }));

    expect(screen.getByTestId("github-import-rename-issue")).toHaveTextContent(
      "该安装 ID 已被本次导入中的「OpenAI Docs」使用",
    );
    expect(screen.getByTestId("github-import-rename-notice")).toHaveTextContent(
      "源 SKILL.md 的名称与内容保持不变",
    );
    expect(screen.getByRole("button", { name: "检查导入内容" })).toBeDisabled();

    fireEvent.click(within(detail).getByRole("button", { name: renameActionLabel }));
    fireEvent.change(screen.getByPlaceholderText("新的安装 ID"), {
      target: { value: "docs-copy" },
    });
    fireEvent.click(screen.getByRole("button", { name: "确认" }));

    expect(screen.queryByTestId("github-import-rename-issue")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "检查导入内容" })).toBeEnabled();
  });

  it("keeps decisions and reports the blocked reason with the existing path", async () => {
    const blockedFailure: GitHubImportFailure = {
      code: "blocked",
      message: "Local platform skill already owns this id.",
      sourcePath: "skills/.system/skill-creator/SKILL.md",
      skillId: "skill-creator",
      existingPath: "/Users/test/.claude/skills/skill-creator/target",
      importedSkills: [],
      skippedSkills: [],
    };
    onImport.mockRejectedValueOnce(blockedFailure);

    const wizard = renderConflictWizard({
      preview: {
        repo: conflictRepo,
        skills: [conflictSkill("non_central"), docsSkill],
      },
    });

    const detail = await screen.findByTestId("github-import-detail-pane");
    fireEvent.click(within(detail).getByRole("button", { name: renameActionLabel }));
    fireEvent.change(screen.getByPlaceholderText("新的安装 ID"), {
      target: { value: "skill-creator-renamed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "确认" }));
    fireEvent.click(screen.getByRole("button", { name: "检查导入内容" }));
    fireEvent.click(await screen.findByRole("button", { name: "导入" }));

    await waitFor(() => expect(onImport).toHaveBeenCalled());
    wizard.rerender({ importFailure: blockedFailure });

    const banner = await screen.findByTestId("github-import-blocked-failure");
    expect(banner).toHaveTextContent("Local platform skill already owns this id.");
    expect(banner).toHaveTextContent(
      "/Users/test/.claude/skills/skill-creator/target",
    );
    expect(screen.getByTestId("github-import-confirm-summary")).toHaveTextContent(
      "skill-creator-renamed",
    );

    fireEvent.click(screen.getByRole("button", { name: "返回预览修改" }));
    const reopenedDetail = await screen.findByTestId("github-import-detail-pane");
    fireEvent.click(within(reopenedDetail).getByRole("button", { name: renameActionLabel }));
    expect(onClearImportFailure).toHaveBeenCalled();
  });

  it("reports exact imported, skipped, failed and not-attempted state after a partial failure", async () => {
    const failure: GitHubImportFailure = {
      code: "failed",
      message: "Could not write skills/extra/SKILL.md",
      sourcePath: "skills/extra/SKILL.md",
      skillId: "extra",
      existingPath: null,
      importedSkills: [
        {
          sourcePath: "skills/docs/SKILL.md",
          originalSkillId: "docs",
          importedSkillId: "docs",
          skillName: "OpenAI Docs",
          targetDirectory: "/Users/test/.agents/skills/docs",
          resolution: "overwrite",
        },
      ],
      skippedSkills: [],
    };
    onImport.mockRejectedValueOnce(failure);

    const wizard = renderConflictWizard({
      preview: {
        repo: conflictRepo,
        skills: [docsSkill, extraSkill, thirdSkill],
      },
    });

    fireEvent.click(screen.getByRole("button", { name: "检查导入内容" }));
    fireEvent.click(await screen.findByRole("button", { name: "导入" }));
    await waitFor(() => expect(onImport).toHaveBeenCalled());
    wizard.rerender({ importFailure: failure });

    const banner = await screen.findByTestId("github-import-partial-failure");
    expect(banner).toHaveTextContent("Could not write skills/extra/SKILL.md");
    expect(screen.getByTestId("github-import-result-imported-count")).toHaveTextContent("1");
    expect(screen.getByTestId("github-import-result-skipped-count")).toHaveTextContent("0");
    expect(screen.getByTestId("github-import-result-failed-count")).toHaveTextContent("1");
    expect(screen.getByTestId("github-import-result-not-attempted-count")).toHaveTextContent("1");
    expect(screen.getByTestId("github-import-not-attempted")).toHaveTextContent(
      "Third Skill",
    );
    expect(screen.queryByText("导入完成")).not.toBeInTheDocument();
  });

  it("does not claim success when every skill was skipped", async () => {
    renderConflictWizard({
      preview: { repo: conflictRepo, skills: [legacySkill] },
      importResult: {
        repo: conflictRepo,
        importedSkills: [],
        skippedSkills: ["skills/legacy/SKILL.md"],
      },
    });

    const header = await screen.findByTestId("github-import-success-header");
    expect(header).toHaveTextContent("没有导入任何技能");
    expect(screen.queryByText("导入完成")).not.toBeInTheDocument();
  });

  it("keeps legacy error text next to the user's choices", async () => {
    onImport.mockRejectedValueOnce(new Error("network down"));

    renderConflictWizard({
      preview: {
        repo: conflictRepo,
        skills: [conflictSkill("non_central"), docsSkill],
      },
    });
    fireEvent.click(screen.getByRole("button", { name: "检查导入内容" }));
    fireEvent.click(await screen.findByRole("button", { name: "导入" }));

    const banner = await screen.findByTestId("github-import-legacy-failure");
    expect(banner).toHaveTextContent("network down");
    expect(screen.getByTestId("github-import-confirm-summary")).toBeInTheDocument();
  });

  it("reports a post-import refresh failure without repeating the import", async () => {
    const result: GitHubRepoImportResult = {
      repo: conflictRepo,
      importedSkills: [
        {
          sourcePath: "skills/docs/SKILL.md",
          originalSkillId: "docs",
          importedSkillId: "docs",
          skillName: "OpenAI Docs",
          targetDirectory: "/Users/test/.agents/skills/docs",
          resolution: "overwrite",
        },
      ],
      skippedSkills: [],
    };
    onImport.mockResolvedValueOnce(result);
    onAfterImportSuccess
      .mockRejectedValueOnce(new Error("refresh failed"))
      .mockResolvedValueOnce(undefined);

    const wizard = renderConflictWizard({
      preview: { repo: conflictRepo, skills: [docsSkill] },
    });
    fireEvent.click(screen.getByRole("button", { name: "检查导入内容" }));
    fireEvent.click(await screen.findByRole("button", { name: "导入" }));

    await waitFor(() => expect(onAfterImportSuccess).toHaveBeenCalledTimes(1));
    wizard.rerender({ importResult: result });

    const banner = await screen.findByTestId("github-import-post-sync-error");
    expect(banner).toHaveTextContent("refresh failed");

    fireEvent.click(within(banner).getByRole("button", { name: "重试刷新" }));

    await waitFor(() => expect(onAfterImportSuccess).toHaveBeenCalledTimes(2));
    expect(onImport).toHaveBeenCalledTimes(1);
  });

  it("labels a conflict-free skill as a new install on the confirm and result steps", async () => {
    const result: GitHubRepoImportResult = {
      repo: conflictRepo,
      importedSkills: [
        {
          sourcePath: docsSkill.sourcePath,
          originalSkillId: "docs",
          importedSkillId: "docs",
          skillName: "OpenAI Docs",
          targetDirectory: "/Users/test/.agents/skills/docs",
          resolution: "overwrite",
        },
      ],
      skippedSkills: [],
    };

    const wizard = renderConflictWizard({
      preview: { repo: conflictRepo, skills: [docsSkill] },
    });
    expect(
      screen.getByRole("checkbox", {
        name: zh.marketplace.githubImportSelectSkill.replace(
          "{{name}}",
          "OpenAI Docs",
        ),
      }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "检查导入内容" }));
    await screen.findByTestId("github-import-confirm-summary");

    expect(screen.getAllByText("新安装").length).toBeGreaterThan(0);
    expect(screen.queryByText("覆盖")).not.toBeInTheDocument();

    wizard.rerender({ importResult: result });

    const hub = await screen.findByTestId("github-import-result-hub");
    expect(within(hub).getByText("新安装")).toBeInTheDocument();
    expect(within(hub).queryByText("覆盖")).not.toBeInTheDocument();
  });

  it("keeps an explicitly renamed conflict-free skill as an installation-ID change", async () => {
    renderConflictWizard({
      preview: { repo: conflictRepo, skills: [docsSkill] },
    });
    const detail = await screen.findByTestId("github-import-detail-pane");
    fireEvent.click(
      within(detail).getByRole("button", { name: renameActionLabel }),
    );
    fireEvent.change(screen.getByPlaceholderText("新的安装 ID"), {
      target: { value: "docs-renamed" },
    });
    fireEvent.click(screen.getByRole("button", { name: "检查导入内容" }));
    await screen.findByTestId("github-import-confirm-summary");

    expect(screen.getAllByText("更改安装 ID").length).toBeGreaterThan(0);
    expect(screen.queryByText("新安装")).not.toBeInTheDocument();
  });
});

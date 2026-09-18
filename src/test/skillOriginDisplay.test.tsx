import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { UnifiedSkillCard } from "../components/skill/UnifiedSkillCard";
import type { GitHubSkillOriginSummary } from "../types";

const origin: GitHubSkillOriginSummary = {
  owner: "mattpocock",
  repo: "skills",
  sourcePath: "skills/code-review",
  refName: "main",
};

function renderCard(
  originProp: GitHubSkillOriginSummary | null,
  installId?: string,
  name = "code-review"
) {
  return render(
    <UnifiedSkillCard
      name={name}
      description="Review code"
      origin={originProp}
      installId={installId}
    />
  );
}

describe("UnifiedSkillCard github origin badge", () => {
  it("links the imported skill manifest inside its repository", () => {
    renderCard(origin, "code-review");

    const link = screen.getByRole("link", { name: "在 GitHub 上打开来源" });
    expect(link).toHaveAttribute(
      "href",
      "https://github.com/mattpocock/skills/blob/main/skills/code-review/SKILL.md"
    );
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noreferrer");
  });

  it("encodes every source path and ref segment in the link", () => {
    renderCard(
      {
        owner: "acme co",
        repo: "skills",
        sourcePath: "skills/demo skill/café",
        refName: "feature/branch x",
      },
      "code-review"
    );

    expect(screen.getByRole("link", { name: "在 GitHub 上打开来源" })).toHaveAttribute(
      "href",
      "https://github.com/acme%20co/skills/blob/feature/branch%20x/skills/demo%20skill/caf%C3%A9/SKILL.md"
    );
  });

  it("normalizes repo-root imports to the manifest at the repository root", () => {
    renderCard(
      { owner: "acme", repo: "root-skills", sourcePath: ".", refName: "main" },
      "root-skills",
      "root-skills"
    );

    expect(screen.getByRole("link", { name: "在 GitHub 上打开来源" })).toHaveAttribute(
      "href",
      "https://github.com/acme/root-skills/blob/main/SKILL.md"
    );
    expect(screen.queryByText(/安装 ID/)).not.toBeInTheDocument();
  });

  it("labels renamed installs with the local installation ID", () => {
    renderCard(origin, "code-review-renamed");

    expect(screen.getByText("安装 ID code-review-renamed")).toBeInTheDocument();
  });

  it("keeps the card compact when the local ID matches the SKILL.md name", () => {
    renderCard(origin, "code-review");

    expect(screen.queryByText(/安装 ID/)).not.toBeInTheDocument();
  });

  it("compares the local ID against the SKILL.md name, not the repository folder", () => {
    renderCard(
      {
        owner: "mattpocock",
        repo: "skills",
        sourcePath: "skills/legacy-folder",
        refName: "main",
      },
      "code-review"
    );

    expect(screen.queryByText(/安装 ID/)).not.toBeInTheDocument();
  });

  it("keeps unaffected cards compact without any origin evidence", () => {
    renderCard(null, "code-review");

    expect(screen.queryByRole("link", { name: "在 GitHub 上打开来源" })).not.toBeInTheDocument();
    expect(screen.queryByText(/安装 ID/)).not.toBeInTheDocument();
  });

  it("does not invent a local ID chip when the caller has no local ID", () => {
    renderCard(origin);

    expect(screen.queryByText(/安装 ID/)).not.toBeInTheDocument();
  });
});

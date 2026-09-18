import type { GitHubSkillOriginSummary } from "@/types";

/**
 * GitHub URL of the imported skill manifest.
 *
 * `sourcePath` is the skill directory inside the repository ("." for a
 * repo-root skill), so the link always points at that directory's SKILL.md
 * with every path and ref segment encoded.
 */
export function githubSkillSourceUrl(origin: GitHubSkillOriginSummary): string {
  const ref = origin.refName
    .split("/")
    .map((part) => encodeURIComponent(part))
    .join("/");
  const dir = origin.sourcePath
    .split("/")
    .filter((segment) => segment && segment !== ".")
    .map((segment) => encodeURIComponent(segment))
    .join("/");
  return (
    `https://github.com/${encodeURIComponent(origin.owner)}` +
    `/${encodeURIComponent(origin.repo)}/blob/${ref}` +
    (dir ? `/${dir}` : "") +
    "/SKILL.md"
  );
}

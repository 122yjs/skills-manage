import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { CollectionInstallDialog } from "@/components/collection/CollectionInstallDialog";
import type { AgentWithStatus } from "@/types";

const agents: AgentWithStatus[] = [
  {
    id: "universal",
    display_name: "Universal",
    category: "shared",
    global_skills_dir: "~/.agents/skills",
    is_detected: true,
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
  {
    id: "claude-code",
    display_name: "Claude Code",
    category: "coding",
    global_skills_dir: "~/.claude/skills",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
];

describe("CollectionInstallDialog", () => {
  it("공용 설치와 호환 개별 대상을 동시에 선택하지 않는다", async () => {
    const onInstall = vi.fn().mockResolvedValue({ succeeded: [], failed: [] });
    render(
      <CollectionInstallDialog
        open
        onOpenChange={vi.fn()}
        collectionName="Test"
        skillCount={2}
        agents={agents}
        onInstall={onInstall}
      />
    );

    expect(screen.getByRole("checkbox", { name: "共享安装 (.agents)" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Cursor" })).not.toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Claude Code" })).toBeChecked();

    fireEvent.click(screen.getByRole("checkbox", { name: "Cursor" }));

    await waitFor(() => {
      expect(
        screen.getByRole("checkbox", { name: "共享安装 (.agents)" })
      ).not.toBeChecked();
      expect(screen.getByRole("checkbox", { name: "Cursor" })).toBeChecked();
    });

    fireEvent.click(screen.getByRole("button", { name: "安装到 2 个平台" }));

    await waitFor(() => {
      expect(onInstall).toHaveBeenCalledWith(["claude-code", "cursor"]);
    });
  });
});

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { DevToolSetupDialog } from "@/components/settings/DevToolSetupDialog";
import { useDevToolSetupStore } from "@/stores/devToolSetupStore";
import type { AgentWithStatus } from "@/types";

vi.mock("@/stores/devToolSetupStore", () => ({
  useDevToolSetupStore: vi.fn(),
}));

const tools: AgentWithStatus[] = [
  {
    id: "codex",
    display_name: "Codex CLI",
    category: "coding",
    global_skills_dir: "/Users/test/.agents/skills",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "cursor",
    display_name: "Cursor",
    category: "coding",
    global_skills_dir: "/Users/test/.cursor/skills",
    is_detected: false,
    is_builtin: true,
    is_enabled: true,
  },
];

describe("DevToolSetupDialog", () => {
  const save = vi.fn().mockResolvedValue(undefined);
  const closeEditor = vi.fn();

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(useDevToolSetupStore).mockImplementation((selector) =>
      selector({
        status: "ready",
        completed: false,
        tools,
        isEditorOpen: false,
        isSaving: false,
        error: null,
        load: vi.fn(),
        openEditor: vi.fn(),
        closeEditor,
        save,
      })
    );
  });

  it("강하게 감지된 도구만 미리 고르고, 사이드바 표시 선택을 저장한다", async () => {
    render(<DevToolSetupDialog />);

    expect(
      screen.getByRole("checkbox", { name: /Codex CLI/ })
    ).toBeChecked();
    expect(screen.queryByRole("checkbox", { name: /Cursor/ })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: /显示 1 个已隐藏的平台/ }));
    fireEvent.click(screen.getByRole("checkbox", { name: /Cursor/ }));
    fireEvent.click(screen.getByRole("button", { name: "保存显示设置" }));

    await waitFor(() => {
      expect(save).toHaveBeenCalledWith(["codex", "cursor"]);
    });
  });
});

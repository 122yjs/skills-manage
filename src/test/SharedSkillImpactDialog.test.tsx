import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { SharedSkillImpactDialog } from "../components/skill/SharedSkillImpactDialog";
import type { SharedSkillImpact } from "../types";

function impact(overrides: Partial<SharedSkillImpact> = {}): SharedSkillImpact {
  return {
    shared_install_id: "/Users/test/.agents/skills/managed",
    skill_id: "managed",
    skill_name: "Managed skill",
    enabled: true,
    confirmed_platforms: [{ agent_id: "claude-code", display_name: "Claude Code" }],
    separate_installs: [],
    reason: null,
    management_path: "/Users/test/.agents/skills/managed",
    confirmation_token: "token-1",
    ...overrides,
  };
}

function renderDialog(props: Partial<Parameters<typeof SharedSkillImpactDialog>[0]> = {}) {
  const onOpenChange = vi.fn();
  const onConfirm = vi.fn();
  const view = render(
    <SharedSkillImpactDialog
      open
      onOpenChange={onOpenChange}
      impacts={[impact()]}
      currentAgentId="claude-code"
      isConfirming={false}
      onConfirm={onConfirm}
      title="Disable?"
      description="Desc"
      confirmLabel="Confirm"
      {...props}
    />
  );
  return { onOpenChange, onConfirm, ...view };
}

describe("SharedSkillImpactDialog", () => {
  it("shows truthful zero counts and the unconfirmed-tools warning with no readers", () => {
    renderDialog({
      impacts: [impact({ confirmed_platforms: [], separate_installs: [] })],
    });

    expect(screen.getByText("已确认 0 个")).toBeInTheDocument();
    expect(
      screen.getByText("即使不在列表中，读取此路径的其他工具也可能受到影响。")
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Confirm" })).toBeEnabled();
  });

  it("starts collapsed and lists the current platform first on expand", () => {
    renderDialog({
      impacts: [
        impact({
          confirmed_platforms: [
            { agent_id: "zeta", display_name: "Zeta" },
            { agent_id: "alpha", display_name: "Alpha" },
            { agent_id: "cur", display_name: "Cur" },
          ],
        }),
      ],
      currentAgentId: "cur",
    });

    expect(screen.queryByText("Zeta")).not.toBeInTheDocument();
    const toggle = screen.getByRole("button", { name: "查看详情" });
    expect(toggle.tagName).toBe("BUTTON");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(toggle);

    expect(toggle).toHaveAttribute("aria-expanded", "true");
    const items = screen.getAllByRole("listitem");
    const names = items.map((item) => item.textContent ?? "");
    expect(names[0]).toContain("Cur");
    expect(names[1]).toContain("Alpha");
    expect(names[2]).toContain("Zeta");
    expect(screen.getByText("当前")).toBeInTheDocument();
  });

  it("warns that separate installs can keep working", () => {
    renderDialog({
      impacts: [
        impact({
          separate_installs: [
            {
              agent_id: "cursor",
              display_name: "Cursor",
              source_path: "/Users/test/.cursor/skills/managed",
            },
          ],
        }),
      ],
    });

    fireEvent.click(screen.getByRole("button", { name: "查看详情" }));
    expect(screen.getByText("独立安装 (1 个)")).toBeInTheDocument();
    expect(
      screen.getByText("独立安装指向相同内容但相互独立，此更改后仍可继续使用。")
    ).toBeInTheDocument();
  });

  it("shows a concise separate warning even while details stay collapsed", () => {
    renderDialog({
      impacts: [
        impact({
          separate_installs: [
            {
              agent_id: "cursor",
              display_name: "Cursor",
              source_path: "/Users/test/.cursor/skills/managed",
            },
          ],
        }),
      ],
    });

    expect(screen.getByText("1 个独立安装在此更改后仍可继续使用。")).toBeInTheDocument();
    expect(screen.queryByText("Cursor")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查看详情" })).toHaveAttribute("aria-expanded", "false");
  });

  it("disables confirmation and shows the real reason plus management path when restricted", () => {
    renderDialog({
      impacts: [impact({ reason: "vault overlaps the install entry" })],
    });

    expect(screen.getByRole("button", { name: "Confirm" })).toBeDisabled();
    expect(screen.getByText("无法更改：vault overlaps the install entry")).toBeInTheDocument();
    expect(screen.getByText(/管理路径/)).toBeInTheDocument();
    expect(
      screen.getByText("即使不在列表中，读取此路径的其他工具也可能受到影响。")
    ).toBeInTheDocument();
  });

  it("keeps long names readable without losing the confirm button", () => {
    const longSkill = "s".repeat(200);
    const longPlatform = "p".repeat(120);
    renderDialog({
      impacts: [
        impact({
          skill_name: longSkill,
          confirmed_platforms: [{ agent_id: "x", display_name: longPlatform }],
        }),
      ],
    });

    expect(screen.getByRole("button", { name: "Confirm" })).toBeInTheDocument();
    expect(screen.getByTitle(longSkill)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "查看详情" }));
    expect(screen.getByTitle(longPlatform)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Confirm" })).toBeInTheDocument();
  });

  it("cancels without confirming", () => {
    const { onOpenChange, onConfirm } = renderDialog();

    fireEvent.click(screen.getByRole("button", { name: "取消" }));

    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("ignores a second confirm click while the first is still running", () => {
    const { onConfirm, rerender } = renderDialog();

    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);

    rerender(
      <SharedSkillImpactDialog
        open
        onOpenChange={vi.fn()}
        impacts={[impact()]}
        currentAgentId="claude-code"
        isConfirming
        onConfirm={onConfirm}
        title="Disable?"
        confirmLabel="Confirm"
      />
    );
    fireEvent.click(screen.getByRole("button", { name: "Confirm" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("replaces the confirmation set when refreshed impacts arrive", () => {
    const { rerender } = renderDialog({
      impacts: [impact({ confirmed_platforms: [{ agent_id: "a", display_name: "A" }] })],
    });
    expect(screen.getByText("已确认 1 个")).toBeInTheDocument();

    const refreshed = impact({
      confirmation_token: "token-2",
      confirmed_platforms: [
        { agent_id: "a", display_name: "A" },
        { agent_id: "b", display_name: "B" },
      ],
    });
    rerender(
      <SharedSkillImpactDialog
        open
        onOpenChange={vi.fn()}
        impacts={[refreshed]}
        currentAgentId="claude-code"
        isConfirming={false}
        onConfirm={vi.fn()}
        title="Disable?"
        confirmLabel="Confirm"
      />
    );

    expect(screen.getByText("已确认 2 个")).toBeInTheDocument();
    expect(screen.queryByText("已确认 1 个")).not.toBeInTheDocument();
    expect(within(screen.getByRole("dialog")).getByRole("button", { name: "Confirm" })).toBeEnabled();
  });

  it("keeps confirm keyboard-reachable and cancels from the dialog", () => {
    const { onOpenChange, onConfirm } = renderDialog();
    const confirm = screen.getByRole("button", { name: "Confirm" });
    confirm.focus();
    expect(confirm).toHaveFocus();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(onOpenChange).toHaveBeenCalledWith(false);
    expect(onConfirm).not.toHaveBeenCalled();
  });

  it("keeps confirm visible when many platforms are listed in the 240px pane", () => {
    const platforms = Array.from({ length: 24 }, (_, index) => ({
      agent_id: index === 3 ? "cur" : `p${index}`,
      display_name: index === 3 ? "Current Long Platform" : `Platform ${String(index).padStart(2, "0")}`,
    }));
    renderDialog({
      impacts: [impact({ confirmed_platforms: platforms })],
      currentAgentId: "cur",
    });
    const confirm = screen.getByRole("button", { name: "Confirm" });
    expect(confirm).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "查看详情" }));
    const items = screen.getAllByRole("listitem");
    expect(items).toHaveLength(24);
    expect(items[0]).toHaveTextContent("Current Long Platform");
    const pane = document.getElementById("shared-impact-/Users/test/.agents/skills/managed");
    expect(pane).toHaveClass("max-h-[240px]");
    expect(confirm).toBeInTheDocument();
  });
});

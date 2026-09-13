import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { UnifiedSkillCard } from "../components/skill/UnifiedSkillCard";
import type { AgentWithStatus } from "../types";

const agents: AgentWithStatus[] = [
  {
    id: "claude-code",
    display_name: "Claude Code",
    category: "coding",
    global_skills_dir: "/Users/test/.claude/skills",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "cursor",
    display_name: "Cursor",
    category: "coding",
    global_skills_dir: "/Users/test/.cursor/skills",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "trae",
    display_name: "Trae",
    category: "coding",
    global_skills_dir: "/Users/test/.trae/skills",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "windsurf",
    display_name: "Windsurf",
    category: "coding",
    global_skills_dir: "/Users/test/.codeium/windsurf/memories",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "codex",
    display_name: "Codex CLI",
    category: "coding",
    global_skills_dir: "/Users/test/.codex/skills",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "qwen",
    display_name: "Qwen Code",
    category: "coding",
    global_skills_dir: "/Users/test/.qwen/skills",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "gemini-cli",
    display_name: "AGY CLI",
    category: "coding",
    global_skills_dir: "/Users/test/.gemini/config/skills",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "copilot",
    display_name: "GitHub Copilot",
    category: "coding",
    global_skills_dir: "/Users/test/.copilot/skills",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "opencode",
    display_name: "OpenCode",
    category: "coding",
    global_skills_dir: "/Users/test/.opencode/skills",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "openclaw",
    display_name: "OpenClaw",
    category: "lobster",
    global_skills_dir: "/Users/test/.openclaw/skills",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
  {
    id: "kiro",
    display_name: "Kiro",
    category: "lobster",
    global_skills_dir: "/Users/test/.kiro/skills",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
];

function renderCard(
  linkedAgents: string[],
  readOnlyAgents: string[] = [],
  usageByAgent: Record<string, { enabled: boolean; paused_by_bulk: boolean }> = {}
) {
  const onToggle = vi.fn();
  const onManagePlatforms = vi.fn();
  render(
    <UnifiedSkillCard
      name="demo-skill"
      description="Demo skill"
      platformIcons={{
        agents,
        linkedAgents,
        readOnlyAgents,
        usageByAgent,
        skillId: "demo-skill",
        onToggle,
        togglingAgentId: null,
        onManage: onManagePlatforms,
      }}
    />
  );
  return { onToggle, onManagePlatforms };
}

describe("UnifiedSkillCard platform toggles", () => {
  it("renders all lobster toggles and only featured coding toggles on the card", () => {
    renderCard(["cursor", "openclaw"]);

    expect(screen.getByText("龙虾类")).toBeInTheDocument();
    expect(screen.getByText("编程类")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "管理 demo-skill 的平台安装" })).toBeInTheDocument();

    expect(screen.getByRole("button", { name: "切换 demo-skill (OpenClaw) 的激活状态" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "切换 demo-skill (Kiro) 的激活状态" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "切换 demo-skill (Claude Code) 的激活状态" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "切换 demo-skill (Cursor) 的激活状态" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "切换 demo-skill (Trae) 的激活状态" })).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "切换 demo-skill (AGY CLI) 的激活状态" })
    ).not.toBeInTheDocument();
  });

  it("toggles featured coding platforms directly from the card", () => {
    const { onToggle } = renderCard([]);

    const button = screen.getByRole("button", {
      name: "切换 demo-skill (Cursor) 的激活状态",
    });

    expect(button).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(button);
    expect(onToggle).toHaveBeenCalledWith("demo-skill", "cursor");
  });

  it("keeps read-only direct toggles disabled without marking a direct install", () => {
    renderCard(["cursor"], ["claude-code"]);

    const button = screen.getByRole("button", {
      name: "切换 demo-skill (Claude Code) 的激活状态",
    });

    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("aria-pressed", "false");
    expect(button).toHaveClass("text-muted-foreground/40");
    expect(button.querySelector("svg")).toHaveClass("opacity-40", "grayscale");
  });

  it("keeps a paused managed install available for restoration", () => {
    const { onToggle } = renderCard([], [], {
      cursor: { enabled: false, paused_by_bulk: true },
    });

    const button = screen.getByRole("button", {
      name: "切换 demo-skill (Cursor) 的激活状态",
    });
    expect(button).toHaveAttribute("aria-pressed", "false");
    fireEvent.click(button);
    expect(onToggle).toHaveBeenCalledWith("demo-skill", "cursor");
  });

  it("opens the platform manager for hidden coding platforms", () => {
    const { onManagePlatforms } = renderCard(["cursor"]);

    fireEvent.click(screen.getByRole("button", { name: "管理 demo-skill 的平台安装" }));

    expect(onManagePlatforms).toHaveBeenCalledTimes(1);
    expect(screen.getByText("+3")).toBeInTheDocument();
  });
});

describe("UnifiedSkillCard shared control", () => {
  const sharedImpact = {
    shared_install_id: "/Users/test/.agents/skills/demo",
    skill_id: "demo-skill",
    skill_name: "demo-skill",
    enabled: true,
    confirmed_platforms: [
      { agent_id: "claude-code", display_name: "Claude Code" },
      { agent_id: "cursor", display_name: "Cursor" },
    ],
    separate_installs: [],
    reason: null,
    management_path: "/Users/test/.agents/skills/demo",
    confirmation_token: "token-1",
  };

  function renderShared(excludedHere: boolean, individual = true) {
    const onToggleShared = vi.fn();
    const onIndividualToggle = vi.fn();
    render(
      <UnifiedSkillCard
        name="demo-skill"
        description="Demo skill"
        sharedControl={{
          impact: sharedImpact,
          excludedHere,
          onToggleShared,
          individual: individual
            ? {
                enabled: !excludedHere,
                canToggle: true,
                onToggle: onIndividualToggle,
                platformDisplayName: "Claude Code",
              }
            : null,
        }}
      />
    );
    return { onToggleShared, onIndividualToggle };
  }

  it("shows shared state and exclusion distinctly", () => {
    renderShared(true);

    const sharedSwitch = screen.getByRole("switch", { name: "切换 demo-skill 的公用状态" });
    expect(sharedSwitch).toBeChecked();
    expect(screen.getByText("已在此平台排除")).toBeInTheDocument();
  });

  it("keeps exclusion independent of the shared toggle", () => {
    const { onToggleShared } = renderShared(true);

    const sharedSwitch = screen.getByRole("switch", { name: "切换 demo-skill 的公用状态" });
    fireEvent.click(sharedSwitch);
    expect(onToggleShared).toHaveBeenCalledTimes(1);
    expect(screen.getByText("已在此平台排除")).toBeInTheDocument();
    expect(sharedSwitch).toBeChecked();
  });

  it("keeps the supported individual toggle in a secondary menu", () => {
    const { onToggleShared, onIndividualToggle } = renderShared(false);

    const menu = screen.getByText("单个平台控制").closest("details");
    expect(menu).not.toBeNull();
    expect(menu).not.toHaveAttribute("open");
    fireEvent.click(screen.getByText("单个平台控制"));
    expect(menu).toHaveAttribute("open");

    const individual = screen.getByRole("switch", {
      name: "仅在 Claude Code 切换 demo-skill",
    });
    expect(individual).toBeChecked();
    fireEvent.click(individual);

    expect(onIndividualToggle).toHaveBeenCalledWith(false);
    expect(onToggleShared).not.toHaveBeenCalled();
  });

  it("disables the shared switch and shows the reason when restricted", () => {
    const onToggleShared = vi.fn();
    render(
      <UnifiedSkillCard
        name="demo-skill"
        description="Demo skill"
        sharedControl={{
          impact: { ...sharedImpact, reason: "vault overlaps the install entry" },
          excludedHere: false,
          onToggleShared,
        }}
      />
    );

    const sharedSwitch = screen.getByRole("switch", { name: "切换 demo-skill 的公用状态" });
    expect(
      sharedSwitch.getAttribute("data-disabled") !== null ||
        sharedSwitch.getAttribute("aria-disabled") === "true"
    ).toBe(true);
    expect(screen.getByText(/vault overlaps the install entry/)).toBeInTheDocument();
  });
});

describe("UnifiedSkillCard localized description", () => {
  it("translation 메타가 있는 클릭형 카드에 중첩 버튼을 만들지 않는다", () => {
    const { container } = render(
      <UnifiedSkillCard
        name="demo-skill"
        description="Demo skill"
        onClick={vi.fn()}
        translation={{ resourceId: "skill:demo", sourceLocale: "en" }}
      />
    );

    expect(container.querySelector("button button")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查看技能 demo-skill" })).toBeInTheDocument();
  });
});
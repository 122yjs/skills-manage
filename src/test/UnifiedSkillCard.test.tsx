import { afterEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { UnifiedSkillCard } from "../components/skill/UnifiedSkillCard";
import { useStorageStore } from "../stores/storageStore";
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

  it("shows the individual toggle before the shared control", () => {
    const { onToggleShared, onIndividualToggle } = renderShared(false);
    const individual = screen.getByRole("switch", {
      name: "仅在 Claude Code 切换 demo-skill",
    });
    const shared = screen.getByRole("switch", { name: "切换 demo-skill 的公用状态" });
    expect(screen.getByText("允许在 Claude Code 中使用")).toBeInTheDocument();
    expect(individual.compareDocumentPosition(shared) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    fireEvent.click(individual);
    expect(onIndividualToggle).toHaveBeenCalledWith(false);
    expect(onToggleShared).not.toHaveBeenCalled();
  });

  it("shared installation has one management action and keeps its source in details", () => {
    const onManageUniversal = vi.fn();
    render(<UnifiedSkillCard name="demo-skill" sourceType="symlink" isUniversalSource
      onManageUniversal={onManageUniversal} sharedControl={{ impact: sharedImpact, excludedHere: false, onToggleShared: vi.fn() }} />);
    expect(screen.getAllByRole("button", { name: "管理共享安装" })).toHaveLength(1);
    expect(screen.getByText("多个平台共用的安装")).toBeInTheDocument();
    const details = screen.getByText("来源与安装信息").closest("details");
    expect(details).not.toHaveAttribute("open");
    fireEvent.click(screen.getByText("来源与安装信息"));
    expect(details).toHaveAttribute("open");
    expect(screen.getByText("链接的原文件")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "管理共享安装" }));
    expect(onManageUniversal).toHaveBeenCalledOnce();
  });

  it("platform removal is named and requires a second click", () => {
    const onUninstall = vi.fn();
    render(<UnifiedSkillCard name="demo-skill" platformDisplayName="Claude Code"
      onUninstallFromPlatform={onUninstall} uninstallFromLabel="Claude Code에서 설치 삭제"
      uninstallConfirmLabel="Claude Code 설치 삭제 확인" />);
    const menu = screen.getByLabelText("demo-skill 的操作").closest("details");
    expect(menu).not.toHaveAttribute("open");
    fireEvent.click(screen.getByLabelText("demo-skill 的操作"));
    expect(menu).toHaveAttribute("open");
    fireEvent.click(screen.getByRole("button", { name: "Claude Code에서 설치 삭제" }));
    expect(onUninstall).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    expect(onUninstall).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Claude Code에서 설치 삭제" }));
    fireEvent.click(screen.getByRole("button", { name: "Claude Code 설치 삭제 확인" }));
    expect(onUninstall).toHaveBeenCalledOnce();
    expect(menu).not.toHaveAttribute("open");
  });

  it("marks an observed folder as not managed and keeps its switch disabled", () => {
    render(<UnifiedSkillCard name="my-notes" platformDisplayName="Claude Code" isReadOnly isUnmanaged
      usageControl={{ enabled: false, onCheckedChange: vi.fn(), state: "unsupported",
        disabledReason: "앱이 관리하지 않는 폴더입니다." }} />);
    expect(screen.getByText("应用未管理")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: /my-notes/ })).toHaveAttribute("data-disabled");
  });

  it("shows an import action on an observed folder only", () => {
    const onImport = vi.fn();
    const { rerender } = render(<UnifiedSkillCard name="my-notes" platformDisplayName="Claude Code" isReadOnly isUnmanaged
      onImportObserved={onImport} />);
    fireEvent.click(screen.getByRole("button", { name: "导入 my-notes" }));
    expect(onImport).toHaveBeenCalledOnce();

    rerender(<UnifiedSkillCard name="my-notes" platformDisplayName="Claude Code" onImportObserved={onImport} />);
    expect(screen.queryByRole("button", { name: "导入 my-notes" })).not.toBeInTheDocument();
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
    expect(screen.getByText("无法单独切换此平台")).toBeInTheDocument();
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

describe("UnifiedSkillCard symlink source", () => {
  function renderLinkCard(symlinkTarget: string) {
    useStorageStore.setState({
      status: {
        central_path: "/Users/test/.skillsmanage/skills",
        default_central_path: "/Users/test/.skillsmanage/skills",
        legacy_path: "/Users/test/.agents/skills",
        universal_path: "/Users/test/.agents/skills",
        migration_state: "completed",
        migration_required: false,
        legacy_skill_count: 0,
      },
    });
    render(<UnifiedSkillCard name="samcheonpo" platformDisplayName="Antigravity" sourceType="symlink"
      symlinkTarget={symlinkTarget} installedPath="/Users/test/.gemini/config/skills/anti-samcheonpo" />);
    fireEvent.click(screen.getByText("来源与安装信息"));
  }

  afterEach(() => useStorageStore.setState({ status: null }));

  it("보관함 안을 가리키는 링크는 보관함 이름표와 원본 경로를 보여 준다", () => {
    renderLinkCard("/Users/test/.skillsmanage/skills/samcheonpo");
    expect(screen.getByText("技能仓库")).toBeInTheDocument();
    expect(screen.getByText("指向的原文件：/Users/test/.skillsmanage/skills/samcheonpo")).toBeInTheDocument();
    expect(screen.getByText("链接位置：/Users/test/.gemini/config/skills/anti-samcheonpo")).toBeInTheDocument();
  });

  it("보관함 밖을 가리키는 링크는 외부 원본 이름표와 풀어 쓴 원본 경로를 보여 준다", () => {
    renderLinkCard("../../../.agents/skills/samcheonpo");
    expect(screen.queryByText("技能仓库")).not.toBeInTheDocument();
    expect(screen.getByText("外部原文件")).toBeInTheDocument();
    expect(screen.getByText("指向的原文件：/Users/test/.agents/skills/samcheonpo")).toBeInTheDocument();
  });
});

import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import { useDevToolSetupStore } from "@/stores/devToolSetupStore";
import { usePlatformStore } from "@/stores/platformStore";
import type { AgentWithStatus, DevToolSetupState } from "@/types";

const platformAgents: AgentWithStatus[] = [
  {
    id: "codex",
    display_name: "Codex CLI",
    category: "coding",
    global_skills_dir: "~/.codex/skills",
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
    id: "central",
    display_name: "Skill Library",
    category: "central",
    global_skills_dir: "~/.skillsmanage/skills",
    is_detected: true,
    is_builtin: true,
    is_enabled: true,
  },
];

describe("devToolSetupStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePlatformStore.setState({ agents: platformAgents, skillsByAgent: { codex: 2, cursor: 1 } });
    useDevToolSetupStore.setState({
      status: "ready",
      completed: true,
      tools: platformAgents.filter((agent) => agent.category === "coding"),
      isEditorOpen: true,
      isSaving: false,
      error: null,
    });
  });

  it("updates only platform visibility metadata after saving without a rescan", async () => {
    const saved: DevToolSetupState = {
      completed: true,
      tools: platformAgents
        .filter((agent) => agent.category === "coding")
        .map((agent) =>
          agent.id === "cursor" ? { ...agent, is_enabled: false } : agent
        ),
    };
    vi.mocked(invoke).mockResolvedValueOnce(saved);

    await useDevToolSetupStore.getState().save(["codex"]);

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke).toHaveBeenCalledWith("save_dev_tool_selection", {
      agentIds: ["codex"],
    });
    expect(usePlatformStore.getState().agents).toEqual([
      platformAgents[0],
      { ...platformAgents[1], is_enabled: false },
      platformAgents[2],
    ]);
    expect(usePlatformStore.getState().skillsByAgent).toEqual({ codex: 2, cursor: 1 });
  });
});

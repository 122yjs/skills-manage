import { describe, expect, it } from "vitest";

import {
  getDistinctInstallTargetAgents,
  UNIVERSAL_AGENT_ID,
  updateInstallTargetSelection,
} from "@/lib/agents";
import type { AgentWithStatus } from "@/types";

describe("updateInstallTargetSelection", () => {
  it("prefers Universal over compatible native targets", () => {
    const result = updateInstallTargetSelection(
      new Set(["cursor", "claude-code"]),
      UNIVERSAL_AGENT_ID,
      true
    );

    expect(result).toEqual(new Set(["claude-code", UNIVERSAL_AGENT_ID]));
  });

  it("selecting a compatible native target removes Universal", () => {
    const result = updateInstallTargetSelection(
      new Set([UNIVERSAL_AGENT_ID, "claude-code"]),
      "gemini-cli",
      true
    );

    expect(result).toEqual(new Set(["claude-code", "gemini-cli"]));
  });
});

describe("getDistinctInstallTargetAgents", () => {
  it("공용 대상은 유지하고 감지되지 않은 내장 도구는 제외한다", () => {
    const agents: AgentWithStatus[] = [
      {
        id: UNIVERSAL_AGENT_ID,
        display_name: "Universal",
        category: "shared",
        global_skills_dir: "~/.agents/skills",
        is_detected: true,
        is_builtin: true,
        is_enabled: true,
      },
      {
        id: "augment",
        display_name: "Augment",
        category: "coding",
        global_skills_dir: "~/.augment/skills",
        is_detected: false,
        is_builtin: true,
        is_enabled: true,
      },
    ];

    expect(getDistinctInstallTargetAgents(agents).map((agent) => agent.id)).toEqual([
      UNIVERSAL_AGENT_ID,
    ]);
  });
});

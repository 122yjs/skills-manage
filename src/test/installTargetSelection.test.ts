import { describe, expect, it } from "vitest";

import {
  UNIVERSAL_AGENT_ID,
  updateInstallTargetSelection,
} from "@/lib/agents";

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

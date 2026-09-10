import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

import { invoke } from "@tauri-apps/api/core";
import {
  isSkillUsageBusyError,
  SkillUsageBusyError,
  useSkillUsageStore,
} from "../stores/skillUsageStore";
import type { UsageStatus } from "../types";

const usageStatus: UsageStatus = {
  agent_id: "claude-code",
  active_count: 1,
  paused_count: 1,
  external_count: 1,
  skills: [
    { skill_id: "frontend-design", name: "frontend-design", enabled: true, paused_by_bulk: false },
    { skill_id: "code-reviewer", name: "code-reviewer", enabled: false, paused_by_bulk: true },
  ],
};

describe("skillUsageStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useSkillUsageStore.setState({
      statuses: [],
      isLoading: false,
      updatingSkillKeys: {},
      updatingAgentIds: {},
      error: null,
    });
  });

  it("reads the usage DTO through the established snake-case command", async () => {
    vi.mocked(invoke).mockResolvedValueOnce([usageStatus]);

    await useSkillUsageStore.getState().loadUsageStatus();

    expect(invoke).toHaveBeenCalledWith("get_skill_usage_status");
    expect(useSkillUsageStore.getState().statuses).toEqual([usageStatus]);
  });

  it("changes one skill's usage with camel-case arguments and reloads the server state", async () => {
    const refreshed = {
      ...usageStatus,
      active_count: 0,
      paused_count: 2,
      skills: usageStatus.skills.map((skill) =>
        skill.skill_id === "frontend-design" ? { ...skill, enabled: false } : skill
      ),
    };
    vi.mocked(invoke)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce([refreshed]);

    await useSkillUsageStore.getState().setSkillUsage("frontend-design", "claude-code", false);

    expect(invoke).toHaveBeenNthCalledWith(1, "set_skill_usage", {
      skillId: "frontend-design",
      agentId: "claude-code",
      enabled: false,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "get_skill_usage_status");
    expect(useSkillUsageStore.getState().statuses).toEqual([refreshed]);
  });

  it("reloads state and exposes a failure instead of reporting a failed mutation as success", async () => {
    vi.mocked(invoke)
      .mockRejectedValueOnce(new Error("write failed"))
      .mockResolvedValueOnce([usageStatus]);

    await expect(
      useSkillUsageStore.getState().setSkillUsage("frontend-design", "claude-code", false)
    ).rejects.toThrow("write failed");

    expect(invoke).toHaveBeenNthCalledWith(1, "set_skill_usage", {
      skillId: "frontend-design",
      agentId: "claude-code",
      enabled: false,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "get_skill_usage_status");
    expect(useSkillUsageStore.getState().statuses).toEqual([usageStatus]);
    expect(useSkillUsageStore.getState().error).toContain("write failed");
  });

  it("does not start a platform-wide mutation while one of its skills is changing", async () => {
    useSkillUsageStore.setState({
      updatingSkillKeys: { "claude-code::frontend-design": true },
    });

    await useSkillUsageStore.getState().setPlatformUsage("claude-code", false);

    expect(invoke).not.toHaveBeenCalled();
    expect(useSkillUsageStore.getState().updatingAgentIds).toEqual({});
  });

  it("deletes one managed install with camel-case arguments and reloads usage state", async () => {
    const refreshed = {
      ...usageStatus,
      active_count: 0,
      paused_count: 1,
      skills: [usageStatus.skills[1]],
    };
    vi.mocked(invoke)
      .mockResolvedValueOnce(undefined)
      .mockResolvedValueOnce([refreshed]);

    await useSkillUsageStore.getState().deleteSkillFromAgent("frontend-design", "claude-code");

    expect(invoke).toHaveBeenNthCalledWith(1, "delete_skill_from_agent", {
      skillId: "frontend-design",
      agentId: "claude-code",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "get_skill_usage_status");
    expect(useSkillUsageStore.getState().statuses).toEqual([refreshed]);
  });

  it("returns partial platform deletion details and reloads usage state", async () => {
    const result = {
      deleted: ["frontend-design"],
      failed: [{ skill_id: "code-reviewer", error: "preserve failed" }],
    };
    const refreshed = {
      ...usageStatus,
      active_count: 0,
      paused_count: 1,
      skills: [usageStatus.skills[1]],
    };
    vi.mocked(invoke)
      .mockResolvedValueOnce(result)
      .mockResolvedValueOnce([refreshed]);

    await expect(
      useSkillUsageStore.getState().deletePlatformInstallations("claude-code")
    ).resolves.toEqual(result);

    expect(invoke).toHaveBeenNthCalledWith(1, "delete_platform_installations", {
      agentId: "claude-code",
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "get_skill_usage_status");
    expect(useSkillUsageStore.getState().statuses).toEqual([refreshed]);
  });

  it("rejects duplicate deletion requests instead of returning a false success", async () => {
    useSkillUsageStore.setState({
      updatingSkillKeys: { "claude-code::frontend-design": true },
    });

    await expect(
      useSkillUsageStore.getState().deleteSkillFromAgent("frontend-design", "claude-code")
    ).rejects.toThrow("already in progress");
    await expect(
      useSkillUsageStore.getState().deletePlatformInstallations("claude-code")
    ).rejects.toThrow("already in progress");

    expect(invoke).not.toHaveBeenCalled();
  });

  it("marks duplicate deletion errors so views can avoid showing a false failure toast", () => {
    expect(isSkillUsageBusyError(new SkillUsageBusyError())).toBe(true);
    expect(isSkillUsageBusyError(new Error("delete failed"))).toBe(false);
  });
});

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

const sharedImpact = {
  shared_install_id: "/Users/test/.agents/skills/managed",
  skill_id: "managed",
  skill_name: "Managed skill",
  enabled: true,
  confirmed_platforms: [{ agent_id: "claude-code", display_name: "Claude Code" }],
  separate_installs: [],
  reason: null,
  management_path: "/Users/test/.agents/skills/managed",
  confirmation_token: "token-1",
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
      platformControlsByAgent: {},
      updatingPlatformControlKeys: {},
      sharedImpactsById: {},
      updatingSharedKeys: {},
      updatingSharedBulk: false,
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

    await expect(
      useSkillUsageStore.getState().setPlatformUsage("claude-code", false)
    ).rejects.toBeInstanceOf(SkillUsageBusyError);

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

  it("loads platform controls with the stable source path", async () => {
    const control = {
      agent_id: "claude-code",
      skill_id: "shared-skill",
      row_id: "claude-code::shared-skill",
      skill_name: "shared-skill",
      source_path: "/tmp/skills/shared-skill",
      source_kind: "compatibility",
      state: "inactive",
      supported: true,
      can_toggle: true,
      can_delete: true,
      can_reapply: false,
      reason: null,
      requires_reload: true,
      scope: "name",
      affected_source_count: 2,
      adapter: "claude-skill-overrides",
      config_path: "/tmp/.claude/settings.json",
    };
    vi.mocked(invoke).mockResolvedValueOnce([control]);

    await useSkillUsageStore.getState().loadPlatformSkillControls("claude-code");

    expect(invoke).toHaveBeenCalledWith("get_platform_skill_controls", {
      agentId: "claude-code",
    });
    expect(useSkillUsageStore.getState().platformControlsByAgent["claude-code"]).toEqual([control]);
  });

  it("updates a platform control and reloads actual state after the command", async () => {
    const target = {
      skillId: "shared-skill",
      skillName: "shared-skill",
      sourcePath: "/tmp/skills/shared-skill",
    };
    const refreshed = { ...target, state: "inactive" };
    vi.mocked(invoke).mockResolvedValueOnce(undefined).mockResolvedValueOnce([refreshed]);

    await useSkillUsageStore.getState().setPlatformSkillControl("claude-code", target, false);

    expect(invoke).toHaveBeenNthCalledWith(1, "set_platform_skill_control", {
      agentId: "claude-code",
      skillId: target.skillId,
      skillName: target.skillName,
      sourcePath: target.sourcePath,
      enabled: false,
    });
    expect(invoke).toHaveBeenNthCalledWith(2, "get_platform_skill_controls", {
      agentId: "claude-code",
    });
    expect(useSkillUsageStore.getState().updatingPlatformControlKeys).toEqual({});
  });

  it("loads a shared impact with camel-case arguments and caches it", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(sharedImpact);

    const impact = await useSkillUsageStore.getState().loadSharedSkillImpact(sharedImpact.shared_install_id);

    expect(invoke).toHaveBeenCalledWith("get_shared_skill_impact", {
      sharedInstallId: sharedImpact.shared_install_id,
    });
    expect(impact).toEqual(sharedImpact);
    expect(
      useSkillUsageStore.getState().sharedImpactsById[sharedImpact.shared_install_id]
    ).toEqual(sharedImpact);
  });

  it("toggles one shared install with its token and reloads related controls", async () => {
    const updated = { ...sharedImpact, enabled: false, confirmation_token: "token-2" };
    useSkillUsageStore.setState({ platformControlsByAgent: { "claude-code": [] } });
    vi.mocked(invoke)
      .mockResolvedValueOnce({ applied: true, impact: updated })
      .mockResolvedValueOnce([usageStatus])
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ total_skills: 0, agents_scanned: 0, skills_by_agent: {} })
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([]);

    const result = await useSkillUsageStore.getState().setSharedSkillUsage(
      sharedImpact.shared_install_id,
      false,
      "token-1"
    );

    expect(invoke).toHaveBeenNthCalledWith(1, "set_shared_skill_usage", {
      sharedInstallId: sharedImpact.shared_install_id,
      enabled: false,
      confirmationToken: "token-1",
    });
    expect(result).toEqual({ applied: true, impact: updated });
    expect(
      useSkillUsageStore.getState().sharedImpactsById[sharedImpact.shared_install_id]
    ).toEqual(updated);
    expect(invoke).toHaveBeenNthCalledWith(2, "get_skill_usage_status");
    expect(invoke).toHaveBeenCalledWith("get_agents");
    expect(invoke).toHaveBeenCalledWith("scan_all_skills");
    expect(invoke).toHaveBeenCalledWith("get_skills_by_agent", { agentId: "claude-code" });
    expect(invoke).toHaveBeenCalledWith("get_skills_by_agent", { agentId: "universal" });
    expect(invoke).toHaveBeenLastCalledWith("get_platform_skill_controls", {
      agentId: "claude-code",
    });
    expect(useSkillUsageStore.getState().updatingSharedKeys).toEqual({});
  });

  it("returns the refreshed impact without a second mutation on token mismatch", async () => {
    const refreshed = {
      ...sharedImpact,
      confirmation_token: "token-2",
      confirmed_platforms: [
        { agent_id: "claude-code", display_name: "Claude Code" },
        { agent_id: "cursor", display_name: "Cursor" },
      ],
    };
    vi.mocked(invoke)
      .mockResolvedValueOnce({ applied: false, impact: refreshed })
      .mockResolvedValueOnce([usageStatus])
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ total_skills: 0, agents_scanned: 0, skills_by_agent: {} })
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([]);

    const result = await useSkillUsageStore.getState().setSharedSkillUsage(
      sharedImpact.shared_install_id,
      false,
      "stale-token"
    );

    expect(result.applied).toBe(false);
    expect(result.impact.confirmation_token).toBe("token-2");
    expect(result.impact.confirmed_platforms).toHaveLength(2);
    expect(invoke).toHaveBeenCalledWith("get_skill_usage_status");
    expect(invoke).toHaveBeenCalledWith("get_skills_by_agent", { agentId: "claude-code" });
    expect(invoke).toHaveBeenCalledWith("get_skills_by_agent", { agentId: "cursor" });
    expect(invoke).toHaveBeenCalledWith("get_skills_by_agent", { agentId: "universal" });
    expect(
      useSkillUsageStore.getState().sharedImpactsById[sharedImpact.shared_install_id]
    ).toEqual(refreshed);
    expect(useSkillUsageStore.getState().updatingSharedKeys).toEqual({});
  });

  it("blocks duplicate shared operations across single and bulk mutations", async () => {
    useSkillUsageStore.setState({
      updatingSharedKeys: { ["shared::" + sharedImpact.shared_install_id]: true },
    });

    await expect(
      useSkillUsageStore.getState().setSharedSkillUsage(sharedImpact.shared_install_id, false, "token-1")
    ).rejects.toThrow("already in progress");
    await expect(
      useSkillUsageStore.getState().setSharedPlatformUsage(false, [])
    ).rejects.toThrow("already in progress");
    expect(invoke).not.toHaveBeenCalled();

    useSkillUsageStore.setState({ updatingSharedKeys: {}, updatingSharedBulk: true });
    await expect(
      useSkillUsageStore.getState().setSharedSkillUsage(sharedImpact.shared_install_id, false, "token-1")
    ).rejects.toThrow("already in progress");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("runs the shared bulk change with snake-case confirmations and reports failures", async () => {
    const updated = { ...sharedImpact, enabled: false, confirmation_token: "token-2" };
    const payload = {
      applied: true,
      impacts: [updated],
      failed: [{ skill_id: "other", error: "locked" }],
    };
    vi.mocked(invoke)
      .mockResolvedValueOnce(payload)
      .mockResolvedValueOnce([usageStatus])
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ total_skills: 0, agents_scanned: 0, skills_by_agent: {} })
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([]);

    const result = await useSkillUsageStore.getState().setSharedPlatformUsage(false, [
      {
        shared_install_id: sharedImpact.shared_install_id,
        confirmation_token: "token-1",
      },
    ]);

    expect(invoke).toHaveBeenNthCalledWith(1, "set_shared_platform_usage", {
      enabled: false,
      confirmations: [
        {
          shared_install_id: sharedImpact.shared_install_id,
          confirmation_token: "token-1",
        },
      ],
    });
    expect(result).toEqual(payload);
    expect(useSkillUsageStore.getState().updatingSharedBulk).toBe(false);
    expect(invoke).toHaveBeenCalledWith("get_skill_usage_status");
    expect(invoke).toHaveBeenCalledWith("get_skills_by_agent", { agentId: "universal" });
  });

  it("resets shared busy state after a failure and reloads usage", async () => {
    vi.mocked(invoke)
      .mockRejectedValueOnce(new Error("move failed"))
      .mockResolvedValueOnce([usageStatus]);

    await expect(
      useSkillUsageStore.getState().setSharedSkillUsage(sharedImpact.shared_install_id, false, "token-1")
    ).rejects.toThrow("move failed");

    expect(invoke).toHaveBeenNthCalledWith(2, "get_skill_usage_status");
    expect(useSkillUsageStore.getState().updatingSharedKeys).toEqual({});
    expect(useSkillUsageStore.getState().updatingSharedBulk).toBe(false);
    expect(useSkillUsageStore.getState().error).toContain("move failed");
  });

  it("blocks individual platform controls while a shared bulk change runs", async () => {
    useSkillUsageStore.setState({ updatingSharedBulk: true });

    await expect(
      useSkillUsageStore.getState().setPlatformSkillControl(
        "claude-code",
        {
          skillId: "shared-skill",
          skillName: "shared-skill",
          sourcePath: "/tmp/skills/shared-skill",
        },
        false
      )
    ).rejects.toThrow("already in progress");
    expect(invoke).not.toHaveBeenCalled();
  });

  it("rejects shared single while an individual skill change is really in flight, then retries", async () => {
    let releaseFirst: ((value: unknown) => void) | undefined;
    vi.mocked(invoke).mockImplementationOnce(
      () => new Promise((resolve) => { releaseFirst = resolve; })
    );
    const first = useSkillUsageStore.getState().setSkillUsage("s1", "claude-code", false);
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(releaseFirst).toEqual(expect.any(Function));
    await expect(
      useSkillUsageStore.getState().setSharedSkillUsage(sharedImpact.shared_install_id, false, "token-1")
    ).rejects.toBeInstanceOf(SkillUsageBusyError);
    expect(isSkillUsageBusyError(
      await useSkillUsageStore.getState().setSharedSkillUsage(sharedImpact.shared_install_id, false, "token-1").catch((e) => e)
    )).toBe(true);
    releaseFirst?.(undefined);
    await first.catch(() => undefined);
    vi.mocked(invoke).mockReset();
    const updated = { ...sharedImpact, enabled: false, confirmation_token: "token-2" };
    vi.mocked(invoke)
      .mockResolvedValueOnce({ applied: true, impact: updated })
      .mockResolvedValueOnce([usageStatus])
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce({ total_skills: 0, agents_scanned: 0, skills_by_agent: {} })
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([]);
    useSkillUsageStore.setState({
      updatingSkillKeys: {},
      updatingAgentIds: {},
      updatingPlatformControlKeys: {},
      updatingSharedKeys: {},
      updatingSharedBulk: false,
      platformControlsByAgent: { "claude-code": [] },
    });
    const retry = await useSkillUsageStore.getState().setSharedSkillUsage(
      sharedImpact.shared_install_id, false, "token-1"
    );
    expect(retry.applied).toBe(true);
  });

  it("rejects individual usage while a shared single change is really in flight, then retries", async () => {
    let releaseShared: ((value: unknown) => void) | undefined;
    vi.mocked(invoke).mockImplementationOnce(
      () => new Promise((resolve) => { releaseShared = resolve; })
    );
    useSkillUsageStore.setState({ platformControlsByAgent: {} });
    const first = useSkillUsageStore.getState().setSharedSkillUsage(
      sharedImpact.shared_install_id, false, "token-1"
    );
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(releaseShared).toEqual(expect.any(Function));
    await expect(
      useSkillUsageStore.getState().setSkillUsage("other", "claude-code", true)
    ).rejects.toBeInstanceOf(SkillUsageBusyError);
    await expect(
      useSkillUsageStore.getState().deleteSkillFromAgent("other", "claude-code")
    ).rejects.toBeInstanceOf(SkillUsageBusyError);
    releaseShared?.({ applied: true, impact: { ...sharedImpact, enabled: false, confirmation_token: "token-2" } });
    await first.catch(() => undefined);
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockResolvedValueOnce(undefined).mockResolvedValueOnce([usageStatus]);
    useSkillUsageStore.setState({
      updatingSkillKeys: {},
      updatingAgentIds: {},
      updatingPlatformControlKeys: {},
      updatingSharedKeys: {},
      updatingSharedBulk: false,
    });
    await useSkillUsageStore.getState().setSkillUsage("other", "claude-code", true);
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("set_skill_usage", {
      skillId: "other",
      agentId: "claude-code",
      enabled: true,
    });
  });

  it("throws a localized desktop-required error for shared mutations without touching cache", async () => {
    const w = window as unknown as Record<string, unknown>;
    const savedTauri = w.__TAURI__;
    const savedInternals = w.__TAURI_INTERNALS__;
    delete w.__TAURI__;
    delete w.__TAURI_INTERNALS__;
    try {
      useSkillUsageStore.setState({
        sharedImpactsById: { [sharedImpact.shared_install_id]: { ...sharedImpact } },
        statuses: [usageStatus],
        updatingSharedKeys: {},
        updatingSharedBulk: false,
        updatingSkillKeys: {},
        updatingAgentIds: {},
        updatingPlatformControlKeys: {},
        error: null,
      });
      const beforeImpact = useSkillUsageStore.getState().sharedImpactsById[sharedImpact.shared_install_id];
      const beforeStatuses = useSkillUsageStore.getState().statuses;
      await expect(
        useSkillUsageStore.getState().setSharedSkillUsage(sharedImpact.shared_install_id, false, "token-1")
      ).rejects.toThrow();
      await expect(
        useSkillUsageStore.getState().setSharedPlatformUsage(false, [
          { shared_install_id: sharedImpact.shared_install_id, confirmation_token: "token-1" },
        ])
      ).rejects.toThrow();
      expect(vi.mocked(invoke)).not.toHaveBeenCalled();
      expect(useSkillUsageStore.getState().sharedImpactsById[sharedImpact.shared_install_id]).toEqual(beforeImpact);
      expect(useSkillUsageStore.getState().statuses).toEqual(beforeStatuses);
      expect(useSkillUsageStore.getState().updatingSharedKeys).toEqual({});
      expect(useSkillUsageStore.getState().updatingSharedBulk).toBe(false);
    } finally {
      w.__TAURI__ = savedTauri;
      w.__TAURI_INTERNALS__ = savedInternals;
    }
  });

  it("surfaces shared refresh failure instead of claiming fully updated", async () => {
    const updated = { ...sharedImpact, enabled: false, confirmation_token: "token-2" };
    useSkillUsageStore.setState({ platformControlsByAgent: { "claude-code": [] } });
    vi.mocked(invoke)
      .mockResolvedValueOnce({ applied: true, impact: updated })
      .mockRejectedValueOnce(new Error("usage reload gone"));
    await expect(
      useSkillUsageStore.getState().setSharedSkillUsage(sharedImpact.shared_install_id, false, "token-1")
    ).rejects.toThrow("usage reload gone");
    expect(useSkillUsageStore.getState().updatingSharedKeys).toEqual({});
  });
});

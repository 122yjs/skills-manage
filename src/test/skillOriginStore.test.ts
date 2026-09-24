import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/tauri", () => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

import { invoke, isTauriRuntime } from "@/lib/tauri";
import { useSkillOriginStore } from "@/stores/skillOriginStore";

const mockInvoke = vi.mocked(invoke);
const mockIsTauriRuntime = vi.mocked(isTauriRuntime);

const target = {
  skillId: "demo",
  agentId: "cursor",
  rowId: "demo",
};

const origin = {
  bindingId: "binding-1",
  targetKey: "/tmp/cursor/demo",
  targetPath: "/tmp/cursor/demo",
  repositoryId: "123",
  owner: "acme",
  repo: "skills",
  sourcePath: "skills/demo",
  refName: "main",
  baselineState: "verified",
  baseCommitOid: "abc123",
  lastAppliedCommitOid: null,
  lastAppliedAt: null,
  lastCheckedAt: "2026-09-13T00:00:00Z",
  lastRemoteCommitOid: "def456",
  lastError: null,
  bindingVersion: 1,
  canUpdate: true,
};

const status = {
  origin,
  state: "remote_update" as const,
  localVsRemote: { added: 1, modified: 2, removed: 0 },
  localVsBase: { added: 0, modified: 0, removed: 0 },
  remoteVsBase: { added: 1, modified: 2, removed: 0 },
  remoteCommitOid: "def456",
};

describe("skillOriginStore", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
    mockIsTauriRuntime.mockReset();
    mockIsTauriRuntime.mockReturnValue(true);
    useSkillOriginStore.getState().reset();
  });

  it("loads the origin for the exact physical detail target", async () => {
    mockInvoke.mockResolvedValue(origin);

    await useSkillOriginStore.getState().loadOrigin(target);

    expect(mockInvoke).toHaveBeenCalledWith("get_skill_origin", {
      target: { skillId: "demo", agentId: "cursor", rowId: "demo" },
    });
    expect(useSkillOriginStore.getState().origin).toEqual(origin);
  });

  it("shows name-only catalog matches without linking them", async () => {
    const candidate = {
      repoUrl: "https://github.com/acme/skills",
      sourcePath: "skills/demo",
      refName: "main",
      reason: "catalog_name",
    };
    mockInvoke.mockResolvedValueOnce(null).mockResolvedValueOnce({ origin: null, candidates: [candidate] });

    await useSkillOriginStore.getState().loadOrigin(target);

    expect(mockInvoke).toHaveBeenCalledWith("discover_skill_origin", {
      target: { skillId: "demo", agentId: "cursor", rowId: "demo" },
    });
    expect(useSkillOriginStore.getState().origin).toBeNull();
    expect(useSkillOriginStore.getState().candidates).toEqual([candidate]);
  });

  it("accepts an automatically verified existing installation", async () => {
    mockInvoke.mockResolvedValueOnce(null).mockResolvedValueOnce({ origin, candidates: [] });

    await useSkillOriginStore.getState().loadOrigin(target);

    expect(useSkillOriginStore.getState().origin).toEqual(origin);
    expect(useSkillOriginStore.getState().candidates).toEqual([]);
  });

  it("links a repository path without mutating the target contract", async () => {
    mockInvoke.mockResolvedValue(status);

    const result = await useSkillOriginStore.getState().linkOrigin(target, {
      repoUrl: "https://github.com/acme/skills",
      sourcePath: "skills/demo",
      refName: "main",
    });

    expect(mockInvoke).toHaveBeenCalledWith("link_skill_origin", {
      request: {
        target: { skillId: "demo", agentId: "cursor", rowId: "demo" },
        repoUrl: "https://github.com/acme/skills",
        sourcePath: "skills/demo",
        refName: "main",
      },
    });
    expect(result).toEqual(status);
    expect(useSkillOriginStore.getState().status).toEqual(status);
  });

  it("prepares an explicit local-change replacement plan before applying", async () => {
    const plan = {
      operationId: "operation-1",
      bindingId: "binding-1",
      targetPath: "/tmp/cursor/demo",
      remoteCommitOid: "def456",
      state: "diverged" as const,
      changes: { added: 1, modified: 2, removed: 1 },
      requiresLocalChangeConfirmation: true,
    };
    mockInvoke.mockResolvedValue(plan);

    await useSkillOriginStore.getState().prepareUpdate(target, true);

    expect(mockInvoke).toHaveBeenCalledWith("prepare_skill_update", {
      request: {
        target: { skillId: "demo", agentId: "cursor", rowId: "demo" },
        allowLocalChanges: true,
      },
    });
  });

  it("does not expose origin operations in the browser fixture runtime", async () => {
    mockIsTauriRuntime.mockReturnValue(false);

    await useSkillOriginStore.getState().loadOrigin(target);

    expect(mockInvoke).not.toHaveBeenCalled();
    expect(useSkillOriginStore.getState().origin).toBeNull();
  });
});

import type { AgentWithStatus } from "@/types";

export const CENTRAL_AGENT_ID = "central";
export const UNIVERSAL_AGENT_ID = "universal";
export const OBSIDIAN_AGENT_ID = "obsidian";

const UNIVERSAL_COMPATIBLE_AGENT_IDS = new Set([
  "amp",
  "antigravity",
  "cline",
  "codex",
  "cursor",
  "deep-agents",
  "dexto",
  "factory-droid",
  "firebender",
  "gemini-cli",
  "copilot",
  "kimi-code-cli",
  "opencode",
  "omp",
  "warp",
]);

const NON_INSTALL_TARGET_AGENT_IDS = new Set([
  CENTRAL_AGENT_ID,
  OBSIDIAN_AGENT_ID,
]);

export function isInstallTargetAgent(agent: Pick<AgentWithStatus, "id">): boolean {
  return !NON_INSTALL_TARGET_AGENT_IDS.has(agent.id);
}

export function isEnabledInstallTargetAgent(
  agent: Pick<AgentWithStatus, "id" | "is_enabled">
): boolean {
  return isInstallTargetAgent(agent) && agent.is_enabled;
}

function normalizeSkillsPath(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/, "");
}

/** 같은 공용 경로를 가리키는 플랫폼을 합쳐 실제 설치 대상만 반환한다. */
export function getDistinctInstallTargetAgents(
  agents: AgentWithStatus[]
): AgentWithStatus[] {
  const universalPath = normalizeSkillsPath(
    agents.find((agent) => agent.id === UNIVERSAL_AGENT_ID)?.global_skills_dir ??
      "~/.agents/skills"
  );

  return agents
    .filter(
      (agent) =>
        isInstallTargetAgent(agent) &&
        agent.is_enabled &&
        (agent.id === UNIVERSAL_AGENT_ID || agent.is_detected) &&
        (agent.id === UNIVERSAL_AGENT_ID ||
          normalizeSkillsPath(agent.global_skills_dir) !== universalPath)
    )
    .sort((left, right) => {
      if (left.id === UNIVERSAL_AGENT_ID) return -1;
      if (right.id === UNIVERSAL_AGENT_ID) return 1;
      return 0;
    });
}

export function getAgentDisplayName(
  agent: Pick<AgentWithStatus, "id" | "display_name">,
  universalLabel: string
): string {
  return agent.id === UNIVERSAL_AGENT_ID ? universalLabel : agent.display_name;
}

export function isUniversalCompatibleAgentId(agentId: string): boolean {
  return UNIVERSAL_COMPATIBLE_AGENT_IDS.has(agentId);
}

/** 공용 설치와 이를 읽는 개별 플랫폼은 같은 스킬을 중복 선택할 수 없다. */
export function updateInstallTargetSelection(
  current: ReadonlySet<string>,
  agentId: string,
  checked: boolean
): Set<string> {
  const next = new Set(current);

  if (!checked) {
    next.delete(agentId);
    return next;
  }

  next.add(agentId);
  if (agentId === UNIVERSAL_AGENT_ID) {
    for (const selectedId of next) {
      if (isUniversalCompatibleAgentId(selectedId)) {
        next.delete(selectedId);
      }
    }
  } else if (isUniversalCompatibleAgentId(agentId)) {
    next.delete(UNIVERSAL_AGENT_ID);
  }

  return next;
}

import type { AgentWithStatus, ScannedSkill } from "@/types";

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

/** 보관함과 공용 경로를 제외하고 목록에서 표시하거나 숨길 수 있는 플랫폼이다. */
export function isToggleableAgent(
  agent: Pick<AgentWithStatus, "id" | "category">
): boolean {
  return (
    isInstallTargetAgent(agent) && agent.id !== UNIVERSAL_AGENT_ID &&
    agent.category !== "central" && agent.category !== "shared"
  );
}

function normalizeSkillsPath(path: string): string {
  return path.replace(/\\/g, "/").replace(/\/+$/, "");
}

/** 경로 표기(구분자·끝 슬래시)만 다른 같은 위치인지 비교한다. */
function isSameSkillsPath(left?: string | null, right?: string | null): boolean {
  const normalizedLeft = normalizeSkillsPath(left ?? "");
  return normalizedLeft.length > 0 && normalizedLeft === normalizeSkillsPath(right ?? "");
}

/** 공용 설치로 볼 수 있는 항목인지 판정한다.
 *
 * `compatibility`는 "다른 경로에서 읽었다"는 뜻일 뿐이라 공용 설치와 같지 않다.
 * Cursor는 `.claude/skills`·`.codex/skills`도 함께 읽는데, 그런 전용 경로를
 * 공용 설치로 표시하면 안 된다. 그래서 출처 경로가 실제 공용 설치 경로와 같을
 * 때만 공용 설치로 인정하고, 출처가 없으면 공용으로 추정하지 않는다.
 */
export function isUniversalSource(
  skill: ScannedSkill,
  universalRoot?: string | null
): boolean {
  if (!skill.is_read_only || skill.source_kind !== "compatibility") {
    return false;
  }
  return isSameSkillsPath(skill.source_root, universalRoot);
}

/** 같은 공용 경로를 가리키는 플랫폼을 합쳐 실제 적용 대상만 반환한다.
 *
 * `is_enabled`는 목록 표시 상태이므로 적용 대상을 고를 때 사용하지 않는다.
 */
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

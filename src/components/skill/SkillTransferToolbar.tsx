import { useId, useState } from "react";
import { ArrowRightLeft } from "lucide-react";
import { useTranslation } from "react-i18next";
import { CollectionInstallDialog } from "@/components/collection/CollectionInstallDialog";
import { Button } from "@/components/ui/button";
import { Checkbox } from "@/components/ui/checkbox";
import { useCentralSkillsStore } from "@/stores/centralSkillsStore";
import { usePlatformStore } from "@/stores/platformStore";
import { useSkillStore } from "@/stores/skillStore";
import { getDistinctInstallTargetAgents, UNIVERSAL_AGENT_ID } from "@/lib/agents";
import type { SelectableSkill, useSkillSelection } from "@/hooks/useSkillSelection";
import type { AgentWithStatus } from "@/types";

interface Props {
  selection: ReturnType<typeof useSkillSelection>;
  agents: AgentWithStatus[];
  sourceAgentId?: string;
  disabled?: boolean;
  onTransferred?: () => void | Promise<void>;
}

export function SkillTransferToolbar({ selection, agents, sourceAgentId, disabled, onTransferred }: Props) {
  const { t } = useTranslation();
  const selectAllId = useId();
  const transferSkills = useCentralSkillsStore((state) => state.transferSkills);
  const refreshCounts = usePlatformStore((state) => state.refreshCounts);
  const getSkillsByAgent = useSkillStore((state) => state.getSkillsByAgent);
  const [pendingSkills, setPendingSkills] = useState<SelectableSkill[] | null>(null);
  // 공용 설치 전환은 기존 설치를 정리하므로 개별 하네스만 대상으로 삼는다.
  const targets = getDistinctInstallTargetAgents(agents).filter(
    (agent) => agent.id !== sourceAgentId && agent.id !== UNIVERSAL_AGENT_ID
  );
  return (
    <div className="flex flex-wrap items-center gap-3 border-b border-border px-6 py-2">
      <div className="flex items-center gap-2 text-xs text-muted-foreground">
        <Checkbox id={selectAllId} checked={selection.allSelected} onCheckedChange={selection.toggleAll}
          disabled={disabled || selection.selectableCount === 0}
          aria-label={t("skillTransfer.selectVisible")} />
        <label htmlFor={selectAllId} className="cursor-pointer">{t("skillTransfer.selectVisible")}</label>
      </div>
      <Button size="sm" variant="outline" disabled={disabled || selection.selectedSkills.length === 0}
        onClick={() => setPendingSkills([...selection.selectedSkills])}>
        <ArrowRightLeft className="size-3.5" />
        {t("skillTransfer.action", { count: selection.selectedSkills.length })}
      </Button>
      <CollectionInstallDialog
        open={pendingSkills !== null}
        onOpenChange={(open) => { if (!open) setPendingSkills(null); }}
        title={t("skillTransfer.title")}
        collectionName=""
        skillCount={pendingSkills?.length ?? 0}
        description={t("skillTransfer.description", { names: pendingSkills?.map((skill) => skill.name).join(", ") })}
        agents={targets}
        selectDetectedByDefault={false}
        onInstall={async (agentIds) => {
          const result = await transferSkills((pendingSkills ?? []).map((skill) => ({
            skill_id: skill.id,
            ...(sourceAgentId ? { source_agent_id: sourceAgentId } : {}),
            ...(skill.source_kind === "plugin" ? { row_id: skill.row_id } : {}),
          })), agentIds);
          await Promise.all([refreshCounts(), ...agentIds.map(getSkillsByAgent),
            ...(sourceAgentId ? [getSkillsByAgent(sourceAgentId)] : [])]);
          await onTransferred?.();
          if (result.failed.length === 0) selection.clear();
          return result;
        }}
      />
    </div>
  );
}

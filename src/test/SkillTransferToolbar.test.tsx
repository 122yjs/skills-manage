import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SkillTransferToolbar } from "@/components/skill/SkillTransferToolbar";
import { useSkillSelection, skillSelectionKey, type SelectableSkill } from "@/hooks/useSkillSelection";
import { useCentralSkillsStore, BROWSER_FIXTURE_AGENTS } from "@/stores/centralSkillsStore";
import { usePlatformStore } from "@/stores/platformStore";
import { useSkillStore } from "@/stores/skillStore";
import { SkillFolderDrawer } from "@/components/skill/SkillFolderDrawer";

vi.mock("@/components/skill/SkillDetailView", () => ({ SkillDetailView: () => null }));

const transfer = vi.fn();
const refresh = vi.fn();
const skills: SelectableSkill[] = [
  { id: "alpha", name: "Alpha" },
  { id: "beta", name: "Beta", row_id: "plugin::beta", source_kind: "plugin", is_read_only: true },
  { id: "manual", name: "Manual", is_read_only: true, source_kind: "unmanaged" },
];

function Harness({ visible = skills, scope = "claude-code" }: { visible?: SelectableSkill[]; scope?: string }) {
  const selection = useSkillSelection(visible, scope);
  return <>
    {visible.map((skill) => <button key={skillSelectionKey(skill)} onClick={() => selection.toggle(skillSelectionKey(skill))}>{skill.name}</button>)}
    <SkillTransferToolbar selection={selection} agents={BROWSER_FIXTURE_AGENTS} sourceAgentId={scope} />
  </>;
}

describe("선택 스킬 이식", () => {
  beforeEach(() => {
    transfer.mockReset().mockResolvedValue({ succeeded: ["beta:cursor"], failed: [] });
    refresh.mockReset().mockResolvedValue(undefined);
    useCentralSkillsStore.setState({ transferSkills: transfer });
    usePlatformStore.setState({ refreshCounts: refresh });
    useSkillStore.setState({ getSkillsByAgent: vi.fn().mockResolvedValue(undefined) });
  });

  it("선택한 플러그인의 출처와 대상만 전달하고 원본 하네스를 제외한다", async () => {
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "Beta" }));
    fireEvent.click(screen.getByRole("button", { name: "移植所选 1 个技能" }));
    expect(screen.getByRole("dialog")).toHaveTextContent("Beta");
    expect(screen.queryByRole("checkbox", { name: "Claude Code" })).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: "共享安装 (.agents)" })).not.toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Cursor" })).not.toBeChecked();
    fireEvent.click(screen.getByRole("checkbox", { name: "Cursor" }));
    fireEvent.click(screen.getByRole("button", { name: "安装到 1 个平台" }));
    await waitFor(() => expect(transfer).toHaveBeenCalledWith([
      { skill_id: "beta", source_agent_id: "claude-code", row_id: "plugin::beta" },
    ], ["cursor"]));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByRole("button", { name: "移植所选 0 个技能" })).toBeDisabled();
    expect(refresh).toHaveBeenCalled();
  });

  it("전체 선택은 이식 가능한 현재 목록만 포함하고 필터나 하네스를 바꾸면 숨은 선택을 없앤다", async () => {
    const view = render(<Harness />);
    fireEvent.click(screen.getByRole("checkbox", { name: "选择当前列表中的技能" }));
    expect(screen.getByRole("button", { name: "移植所选 2 个技能" })).toBeEnabled();
    view.rerender(<Harness visible={[skills[1]]} />);
    expect(screen.getByRole("button", { name: "移植所选 1 个技能" })).toBeEnabled();
    view.rerender(<Harness />);
    expect(screen.getByRole("button", { name: "移植所选 1 个技能" })).toBeEnabled();
    view.rerender(<Harness scope="cursor" />);
    await waitFor(() => expect(screen.getByRole("button", { name: "移植所选 0 个技能" })).toBeDisabled());
  });

  it("일부 실패는 대화상자에 남기고 선택을 유지한다", async () => {
    transfer.mockResolvedValue({ succeeded: [], failed: [{ agent_id: "alpha:cursor", error: "대상 경로 충돌" }] });
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: "Alpha" }));
    fireEvent.click(screen.getByRole("button", { name: "移植所选 1 个技能" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Cursor" }));
    fireEvent.click(screen.getByRole("button", { name: "安装到 1 个平台" }));
    await waitFor(() => expect(screen.getByRole("dialog")).toHaveTextContent("대상 경로 충돌"));
    expect(transfer).toHaveBeenCalledTimes(1);
  });

  it("폴더 안에서 고른 플러그인 스킬도 정확한 출처로 이식한다", async () => {
    render(<SkillFolderDrawer open title="Plugin folder" path="/plugins/example"
      agents={BROWSER_FIXTURE_AGENTS} onOpenChange={vi.fn()}
      skills={[
        { key: "row-one", id: "one", name: "One", agentId: "claude-code", rowId: "row-one", sourceKind: "plugin", isReadOnly: true },
        { key: "row-two", id: "two", name: "Two", agentId: "claude-code", rowId: "row-two", sourceKind: "plugin", isReadOnly: true },
      ]} />);
    fireEvent.click(screen.getByRole("checkbox", { name: "选择 Two" }));
    fireEvent.click(screen.getByRole("button", { name: "移植所选 1 个技能" }));
    fireEvent.click(screen.getByRole("checkbox", { name: "Cursor" }));
    fireEvent.click(screen.getByRole("button", { name: "安装到 1 个平台" }));
    await waitFor(() => expect(transfer).toHaveBeenCalledWith([
      { skill_id: "two", source_agent_id: "claude-code", row_id: "row-two" },
    ], ["cursor"]));
  });
});

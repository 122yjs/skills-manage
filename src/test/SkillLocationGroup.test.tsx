import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { SkillLocationGroup } from "@/components/skill/SkillLocationGroup";
import { groupSkillLocations } from "@/lib/skillLocations";
import type { ScannedSkill } from "@/types";

const skills: ScannedSkill[] = ["agents", "claude", "codex"].map((source) => ({
  id: "paseo",
  row_id: source,
  name: "paseo",
  description: source,
  file_path: `/home/.${source}/skills/paseo/SKILL.md`,
  dir_path: `/home/.${source}/skills/paseo`,
  link_type: "native",
  is_central: source === "agents",
}));

describe("설치 위치 묶음", () => {
  it("이름만 묶고 내용과 경로가 다른 원본 행을 모두 보존한다", () => {
    const other = { ...skills[0], id: "other", name: "other" };
    const groups = groupSkillLocations([skills[0], other, ...skills.slice(1)]);
    expect(groups).toEqual([skills, [other]]);
    expect(groups[0][1]).toBe(skills[1]);
  });

  it("접힌 목록은 한 항목으로 표시하고 펼친 뒤 선택한 위치만 관리한다", () => {
    const manage = vi.fn();
    render(<SkillLocationGroup skills={skills} selectedCount={2}>
      {skills.map((skill) => <button key={skill.row_id} onClick={() => manage(skill.row_id)}>{skill.dir_path}</button>)}
    </SkillLocationGroup>);
    const toggle = screen.getByRole("button", { name: /paseo/ });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("button", { name: skills[0].dir_path })).not.toBeInTheDocument();
    expect(screen.getByText("已选 2 个位置")).toBeInTheDocument();
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getAllByRole("button")).toHaveLength(4);
    fireEvent.click(screen.getByRole("button", { name: skills[1].dir_path }));
    expect(manage).toHaveBeenCalledExactlyOnceWith("claude");
    fireEvent.click(toggle);
    expect(screen.queryByRole("button", { name: skills[1].dir_path })).not.toBeInTheDocument();
  });

  it("한 위치만 있으면 바로 관리 버튼을 보여준다", () => {
    render(<SkillLocationGroup skills={[skills[0]]}><button>관리</button></SkillLocationGroup>);
    expect(screen.getAllByRole("button")).toHaveLength(1);
    expect(screen.getByRole("button", { name: "관리" })).toBeVisible();
  });
});

it("비교 전에는 같은 내용이라고 단정하지 않고 실제 비교 결과를 표시한다", async () => {
  const { useSkillStore } = await import("@/stores/skillStore");
  const compare = vi.fn().mockResolvedValue({ relation: "same_origin", paths: skills.map(s => s.dir_path) });
  useSkillStore.setState({ compareLocations: compare });
  render(<SkillLocationGroup skills={skills} agentId="cursor"><p>경로별 관리</p></SkillLocationGroup>);
  expect(screen.queryByRole("status")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: /paseo/ }));
  fireEvent.click(screen.getByRole("button", { name: "比较原文件与内容" }));
  expect(await screen.findByRole("status")).toHaveTextContent("这些路径指向同一份原文件");
  expect(screen.getByText("同一份原文件")).toBeVisible();
  expect(compare).toHaveBeenCalledWith("cursor", "paseo");
});

import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, useLocation } from "react-router-dom";
import { SkillGroupLinks } from "@/components/skill/SkillGroupLinks";
import { SkillGroupMenu, SkillGroupDetail } from "@/components/collection/SkillGroupBrowser";
import { skillGroupUrl, useSkillGroupStore, type SkillGroup } from "@/stores/skillGroupStore";
import { invoke } from "@/lib/tauri";

vi.mock("@/lib/tauri", () => ({ invoke: vi.fn(), isTauriRuntime: () => true }));

const groups: SkillGroup[] = [
  { id: "repository:mattpocock/skills", name: "mattpocock/skills", kind: "repository", repositoryUrl: "https://github.com/mattpocock/skills", members: [{ skillId: "ask-matt", sourceKey: "skills/ask-matt", name: "ask-matt", filePath: "/agents/ask-matt/SKILL.md" }] },
  { id: "plugin:/cache/tool/1.0", name: "tool", kind: "plugin", folderPath: "/cache/tool/1.0", members: [{ skillId: "ask-matt", sourceKey: "skills/ask-matt", name: "ask-matt", filePath: "/vault/extracted/SKILL.md" }] },
];
function Location() { const l = useLocation(); return <output>{l.pathname}{l.search}</output>; }

beforeEach(() => { useSkillGroupStore.setState({ groups, loading: false, error: null }); });

describe("스킬셋과 플러그인 출처 이동", () => {
  it("이름이 같아도 실제 파일 경로가 속한 모음만 연결하고 상세를 닫는다", () => {
    const close = vi.fn();
    render(<MemoryRouter><SkillGroupLinks filePath="/agents/ask-matt/SKILL.md" onNavigate={close} /><Location /></MemoryRouter>);
    expect(screen.queryByRole("link", { name: "tool" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("link", { name: "mattpocock/skills" }));
    expect(close).toHaveBeenCalledOnce();
    expect(screen.getByRole("status")).toHaveTextContent(skillGroupUrl(groups[0].id));
  });

  it("개별 추출본에서도 원래 플러그인 폴더를 연다", async () => {
    vi.mocked(invoke).mockResolvedValueOnce(undefined);
    render(<MemoryRouter><SkillGroupLinks filePath="/vault/extracted/SKILL.md" /></MemoryRouter>);
    fireEvent.click(screen.getByRole("button", { name: "打开 tool 源文件夹" }));
    await waitFor(() => expect(invoke).toHaveBeenCalledWith("open_in_file_manager", { path: "/cache/tool/1.0" }));
  });

  it("하나의 메뉴에 모든 종류를 표시하고 종류와 검색을 함께 적용한다", () => {
    const select = vi.fn();
    render(<SkillGroupMenu groups={groups} collections={[{ id: "mine", name: "My collection", created_at: "", updated_at: "" }]} selectedId={null} onSelect={select} />);
    expect(screen.getByRole("button", { name: "mattpocock/skills" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "tool" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "My collection" }));
    expect(select).toHaveBeenCalledWith("collection:mine");
    fireEvent.click(screen.getByRole("button", { name: "插件" }));
    expect(screen.queryByRole("button", { name: "My collection" })).not.toBeInTheDocument();
    fireEvent.change(screen.getByRole("textbox"), { target: { value: "missing" } });
    expect(screen.getByText("没有匹配的合集。")).toBeInTheDocument();
  });
  it("같은 원본 스킬의 설치본 두 개는 카드 하나와 설치 위치 두 곳으로 표시한다", () => {
    const member = groups[0].members[0];
    const group = { ...groups[0], sourceSkillCount: 20, members: [member, { ...member, skillId: "renamed", filePath: "/pi/renamed/SKILL.md", agentId: "pi" }] };
    render(<MemoryRouter><SkillGroupDetail group={group} /></MemoryRouter>);
    expect(screen.getAllByRole("button", { name: "查看 ask-matt 的详情" })).toHaveLength(1);
    expect(screen.getByText(/原始 20 个技能/)).toHaveTextContent("本地 1 个技能");
    expect(screen.getByText("2 个文件位置")).toBeInTheDocument();
    expect(screen.getByText("/agents/ask-matt/SKILL.md")).toBeInTheDocument();
    expect(screen.getByText("/pi/renamed/SKILL.md")).toBeInTheDocument();
  });

  it("스킬셋 필터에 GitHub 스킬셋과 앱 제공 스킬셋을 함께 표시한다", () => {
    const paseo: SkillGroup = { id: "bundle:paseo", name: "Paseo", kind: "bundle", members: [] };
    render(<SkillGroupMenu groups={[...groups, paseo]} collections={[]} selectedId={null} onSelect={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: "技能集" }));
    expect(screen.getByRole("button", { name: "Paseo" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "mattpocock/skills" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "tool" })).not.toBeInTheDocument();
  });

});

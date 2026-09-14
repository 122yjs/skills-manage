import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SharedInstallDeleteDialog } from "@/components/skill/SharedInstallDeleteDialog";
import { useSkillUsageStore } from "@/stores/skillUsageStore";

const preview = vi.fn();
const remove = vi.fn();
const onClose = vi.fn();
const onDeleted = vi.fn().mockResolvedValue(undefined);
const plan = (id: string) => ({
  skill_id: id, skill_name: id, enabled: true, source_path: `/shared/${id}`, confirmation_token: `${id}-token`,
  links: [{ agent_id: "claude-code", display_name: "Claude Code", path: `/claude/${id}`, installed_path: `/claude/${id}`, target: `../shared/${id}` }],
});
describe("SharedInstallDeleteDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    preview.mockImplementation(async (id) => plan(id));
    remove.mockResolvedValue({ deleted: ["one"], failed: [] });
    useSkillUsageStore.setState({ previewSharedDelete: preview, deleteSharedInstalls: remove });
  });
  it("플랫폼과 경로를 확인하기 전에는 삭제하지 않는다", async () => {
    render(<SharedInstallDeleteDialog skillIds={["one"]} onClose={onClose} onDeleted={onDeleted} />);
    await screen.findByText("Claude Code");
    expect(screen.getByText("/claude/one")).toBeInTheDocument();
    expect(remove).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "删除已确认的 1 项安装" }));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(remove).toHaveBeenCalledWith([plan("one")]);
  });
  it("실패 이유를 유지하고 실패한 항목만 새로 확인해 재시도한다", async () => {
    remove.mockResolvedValueOnce({ deleted: ["one"], failed: [{ skill_id: "two", error: "백업 공간이 부족합니다" }] });
    render(<SharedInstallDeleteDialog skillIds={["one", "two"]} onClose={onClose} onDeleted={onDeleted} />);
    fireEvent.click(await screen.findByRole("button", { name: "删除已确认的 2 项安装" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("백업 공간이 부족합니다");
    expect(onClose).not.toHaveBeenCalled();
    preview.mockClear();
    fireEvent.click(screen.getByRole("button", { name: "重新确认目标" }));
    await screen.findByRole("button", { name: "删除已确认的 1 项安装" });
    expect(preview).toHaveBeenCalledWith("two");
    expect(preview).not.toHaveBeenCalledWith("one");
  });
  it("영향을 읽지 못하면 오류를 표시하고 삭제를 막는다", async () => {
    preview.mockRejectedValueOnce(new Error("플랫폼 폴더 접근 실패"));
    render(<SharedInstallDeleteDialog skillIds={["one"]} onClose={onClose} onDeleted={onDeleted} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("플랫폼 폴더 접근 실패");
    expect(screen.getByRole("button", { name: "删除已确认的 0 项安装" })).toBeDisabled();
    expect(remove).not.toHaveBeenCalled();
  });
});

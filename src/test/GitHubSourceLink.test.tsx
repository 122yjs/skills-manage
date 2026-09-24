import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { open } from "@tauri-apps/plugin-shell";
import { toast } from "sonner";
import { isTauriRuntime } from "../lib/tauri";
import { GitHubSourceLink } from "../components/skill/GitHubSourceLink";

vi.mock("@tauri-apps/plugin-shell", () => ({ open: vi.fn() }));
vi.mock("../lib/tauri", () => ({ isTauriRuntime: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn() } }));

const url = "https://github.com/owner/repo/blob/main/skills/example/SKILL.md";

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(open).mockResolvedValue();
});

describe("GitHub 원본 링크", () => {
  it("데스크톱에서 기본 브라우저로 열고 카드 선택은 실행하지 않는다", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    const selectCard = vi.fn();
    render(<div onClick={selectCard}><GitHubSourceLink href={url}>원본</GitHubSourceLink></div>);
    expect(fireEvent.click(screen.getByRole("link"))).toBe(false);
    await waitFor(() => expect(open).toHaveBeenCalledWith(url));
    expect(selectCard).not.toHaveBeenCalled();
  });

  it("웹에서는 기본 새 탭 동작을 유지한다", () => {
    vi.mocked(isTauriRuntime).mockReturnValue(false);
    render(<GitHubSourceLink href={url}>원본</GitHubSourceLink>);
    const link = screen.getByRole("link");
    // 실제 탐색 없이 클릭의 기본 동작이 취소되는지만 검사한다.
    expect(fireEvent.click(link)).toBe(true);
    expect(link).toHaveAttribute("href", url);
    expect(link).toHaveAttribute("target", "_blank");
    expect(open).not.toHaveBeenCalled();
  });

  it("브라우저 열기 실패를 사용자에게 알린다", async () => {
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    vi.mocked(open).mockRejectedValue(new Error("실행 실패"));
    render(<GitHubSourceLink href={url}>원본</GitHubSourceLink>);
    fireEvent.click(screen.getByRole("link"));
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("无法打开 GitHub 来源，请重试。"));
  });
});

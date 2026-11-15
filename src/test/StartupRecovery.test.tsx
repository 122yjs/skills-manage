import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

import { invoke } from "@tauri-apps/api/core";
import { StartupGate } from "@/components/layout/StartupRecovery";
import { useStartupStore } from "@/stores/startupStore";

describe("StartupGate", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useStartupStore.setState({ phase: "checking", error: null, isRetrying: false });
  });

  it("shows the failure cause and returns to the app after a retry with a new vault path", async () => {
    const user = userEvent.setup();
    vi.mocked(invoke).mockImplementation(async (command, args) => {
      if (command === "get_startup_status") {
        return { ready: false, error: "보관함 폴더를 사용할 수 없습니다 (/Volumes/Gone)" };
      }
      if (!(args as { centralPath: string | null }).centralPath) {
        throw "디스크가 아직 연결되지 않았습니다";
      }
      return undefined;
    });

    render(
      <StartupGate>
        <div>정상 화면</div>
      </StartupGate>,
    );

    expect(await screen.findByText(/\/Volumes\/Gone/)).toBeInTheDocument();
    expect(screen.queryByText("정상 화면")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "重试" }));
    expect(await screen.findByText("디스크가 아직 연결되지 않았습니다")).toBeInTheDocument();

    await user.type(screen.getByLabelText("新的仓库路径"), "/Volumes/New");
    await user.click(screen.getByRole("button", { name: "使用此路径并重试" }));

    await waitFor(() => expect(screen.getByText("정상 화면")).toBeInTheDocument());
    expect(invoke).toHaveBeenCalledWith("retry_startup", { centralPath: "/Volumes/New" });
  });
});

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/tauri", () => ({
  invoke: vi.fn(),
  isTauriRuntime: vi.fn(() => true),
}));

import { LegacySecretsNotice } from "@/components/layout/LegacySecretsNotice";
import { invoke } from "@/lib/tauri";
import { useLegacySecretsStore } from "@/stores/legacySecretsStore";

describe("LegacySecretsNotice", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useLegacySecretsStore.setState({ present: false });
    vi.mocked(invoke).mockImplementation(async (command) =>
      command === "has_legacy_secrets" ? true : undefined
    );
  });

  it("asks before deleting the old secrets and deletes them only after the user agrees", async () => {
    render(<LegacySecretsNotice />);

    expect(await screen.findByText(/无法使用系统钥匙串/)).toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("clear_legacy_secrets");

    fireEvent.click(screen.getByRole("button", { name: "删除旧值" }));

    await waitFor(() => expect(invoke).toHaveBeenCalledWith("clear_legacy_secrets"));
    await waitFor(() => expect(screen.queryByRole("status")).not.toBeInTheDocument());
  });

  it("keeps the old secrets when the user chooses later", async () => {
    render(<LegacySecretsNotice />);

    fireEvent.click(await screen.findByRole("button", { name: "稍后处理" }));

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(invoke).not.toHaveBeenCalledWith("clear_legacy_secrets");
  });
});

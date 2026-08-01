import type { DashboardSnapshot } from "./types";

export async function requestDashboardSnapshot(
  signal?: AbortSignal,
): Promise<DashboardSnapshot> {
  const response = await fetch("/api/dashboard", {
    method: "GET",
    headers: { Accept: "application/json" },
    signal,
  });

  if (!response.ok) {
    const body = (await response.json().catch(() => null)) as {
      error?: string;
    } | null;
    throw new Error(body?.error ?? `Dashboard request failed (${response.status})`);
  }

  return (await response.json()) as DashboardSnapshot;
}

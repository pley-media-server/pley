import { act, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { plexImportKeys, type ImportProgress } from "@/api/plexImport";
import { mockApi, renderWithProviders, testUser } from "@/test/render";
import { ImportTabTitle } from "./ImportTabTitle";

const idle: ImportProgress = { state: "idle", step: null, done: 0, total: 0, error: null, report: null, first_pass_done: false };

const running: ImportProgress = {
  ...idle,
  state: "running",
  step: "fetching",
  done: 1,
  total: 2,
  started_at: 100,
  // Every step before fetching done (53 of 100) and fetching (35) half done: 70.5%.
  steps: (["profiles", "scanning", "labels", "edits", "collections", "watch_state", "history", "playlists", "fetching", "matching_again"] as const).map(
    (step) => ({ step, state: step === "fetching" ? "running" : step === "matching_again" ? "to_come" : "done", count: null }),
  ),
};

function hide(hidden: boolean) {
  vi.spyOn(document, "hidden", "get").mockReturnValue(hidden);
  document.dispatchEvent(new Event("visibilitychange"));
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("ImportTabTitle", () => {
  it("says how far the import is in the tab, then how it ended while the tab was out of sight, until it's seen", async () => {
    document.title = "Home · Pley";
    let progress = running;
    mockApi({ "GET /api/import/plex": () => progress });
    const { client } = renderWithProviders(<ImportTabTitle />);
    await waitFor(() => expect(document.title).toBe("70% · Importing from Plex · Home · Pley"));

    act(() => hide(true));
    progress = { ...idle, state: "done", report: null };
    await act(() => client.invalidateQueries({ queryKey: plexImportKeys.progress }));
    await waitFor(() => expect(document.title).toBe("Plex import done · Home · Pley"));

    act(() => hide(false));
    expect(document.title).toBe("Home · Pley");
  });

  it("says nothing of an import that ended in sight", async () => {
    document.title = "Home · Pley";
    let progress = running;
    mockApi({ "GET /api/import/plex": () => progress });
    const { client } = renderWithProviders(<ImportTabTitle />);
    await waitFor(() => expect(document.title).toMatch(/^70%/));
    progress = { ...idle, state: "failed", report: null };
    await act(() => client.invalidateQueries({ queryKey: plexImportKeys.progress }));
    await waitFor(() => expect(document.title).toBe("Home · Pley"));
  });

  it("asks nothing for others", async () => {
    const requests = mockApi({ "GET /api/import/plex": () => running });
    renderWithProviders(<ImportTabTitle />, { user: { ...testUser, is_admin: false } });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(requests.some((r) => r.path === "/api/import/plex")).toBe(false);
  });
});

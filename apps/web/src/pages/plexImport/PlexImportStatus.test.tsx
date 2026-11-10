import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { plexImportKeys, type ImportProgress, type ImportReport } from "@/api/plexImport";
import { mockApi, renderWithProviders } from "@/test/render";
import { PlexImportStatus } from "./PlexImportStatus";

const idle: ImportProgress = { state: "idle", step: null, done: 0, total: 0, error: null, report: null, first_pass_done: false };

const report: ImportReport = {
  // 8 November 2026, midday UTC.
  finished_at: 1_794_139_200,
  outcome: "done",
  error: null,
  libraries: [],
  profiles: [],
  labels_added: 0,
  dates_moved: 0,
  matches_fixed: 0,
  edits: {},
  collections_made: 0,
  collection_items: 0,
  smart_left: 0,
};

describe("PlexImportStatus", () => {
  it("shows the import's step and count while it runs, then when it ended and its report", async () => {
    const user = userEvent.setup();
    let progress: ImportProgress = { ...idle, state: "running", step: "fetching", done: 1204, total: 3500, first_pass_done: true };
    mockApi({ "GET /api/import/plex": () => progress });
    const onReport = vi.fn();
    const { client } = renderWithProviders(<PlexImportStatus onReport={onReport} />);
    expect(await screen.findByText("Importing from Plex: Fetching posters and details")).toBeInTheDocument();
    expect(screen.getByText("1,204 of 3,500")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "See what came over" })).not.toBeInTheDocument();

    progress = { ...idle, state: "done", report };
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    expect(await screen.findByText("Imported from Plex on Nov 8, 2026")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "See what came over" }));
    expect(onReport).toHaveBeenCalled();
    // Settings › Libraries keeps it: only Home puts it away.
    expect(screen.queryByRole("button", { name: "Close" })).not.toBeInTheDocument();
  });

  it("shows the steps open, hidden for that import once hidden, and open again for the next", async () => {
    const user = userEvent.setup();
    const steps: ImportProgress["steps"] = [
      { step: "profiles", state: "done", count: 2 },
      { step: "fetching", state: "running", count: null },
    ];
    let progress: ImportProgress = { ...idle, state: "running", step: "fetching", done: 1, total: 4, started_at: 100, steps };
    mockApi({ "GET /api/import/plex": () => progress });
    const first = renderWithProviders(<PlexImportStatus onReport={() => {}} />);
    await user.click(await screen.findByRole("button", { name: /Hide the steps/ }));
    expect(screen.queryByText("Making profiles")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Show the steps/ })).toHaveAttribute("aria-expanded", "false");
    first.unmount();

    // Hidden still, on coming back to it.
    const { client } = renderWithProviders(<PlexImportStatus onReport={() => {}} />);
    expect(await screen.findByRole("button", { name: /Show the steps/ })).toBeInTheDocument();
    progress = { ...progress, started_at: 200 };
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    expect(await screen.findByText("Making profiles")).toBeInTheDocument();
  });

  it("stops the import from its steps, once asked", async () => {
    const user = userEvent.setup();
    const steps: ImportProgress["steps"] = [{ step: "fetching", state: "running", count: null }];
    const requests = mockApi({
      "GET /api/import/plex": () => ({ ...idle, state: "running", step: "fetching", started_at: 100, steps }),
      "DELETE /api/import/plex": () => new Response(null, { status: 204 }),
    });
    renderWithProviders(<PlexImportStatus onReport={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "Stop the import" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Stop the import" }));
    await vi.waitFor(() => expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/import/plex")).toBe(true));
  });

  it("says which of the steps is running and how far the whole import has come", async () => {
    const steps: ImportProgress["steps"] = [
      { step: "profiles", state: "done", count: 3 },
      { step: "scanning", state: "running", count: null },
      { step: "labels", state: "to_come", count: null },
      { step: "fetching", state: "to_come", count: null },
    ];
    const progress: ImportProgress = { ...idle, state: "running", step: "scanning", done: 1, total: 2, steps };
    mockApi({ "GET /api/import/plex": () => progress });
    renderWithProviders(<PlexImportStatus onReport={() => {}} />);
    expect(await screen.findByText("Importing from Plex, step 2 of 4: Scanning the new libraries")).toBeInTheDocument();
    expect(screen.getByText("1 of 2")).toBeInTheDocument();
    // One step done and half of the second: 1.5 of 4.
    expect(screen.getByRole("progressbar", { name: "The whole import" })).toHaveAttribute("aria-valuenow", "38");
  });

  it("shows nothing before any import", async () => {
    const requests = mockApi({ "GET /api/import/plex": () => idle });
    const { container } = renderWithProviders(<PlexImportStatus onReport={() => {}} />);
    await vi.waitFor(() => expect(requests.some((r) => r.path === "/api/import/plex")).toBe(true));
    expect(container).toBeEmptyDOMElement();
  });
});

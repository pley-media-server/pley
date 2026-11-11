import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { plexImportKeys, type ImportProgress, type ImportReport } from "@/api/plexImport";
import { keys } from "@/api/hooks";
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
    let progress: ImportProgress = { ...idle, state: "running", step: "fetching", done: 1204, total: 3500, left_secs: 1500, first_pass_done: true };
    mockApi({ "GET /api/import/plex": () => progress });
    const onReport = vi.fn();
    const { client } = renderWithProviders(<PlexImportStatus onReport={onReport} onAgain={() => {}} />);
    expect(await screen.findByText("Importing from Plex: Fetching posters and details")).toBeInTheDocument();
    expect(screen.getByText("· 1,204 of 3,500")).toBeInTheDocument();
    expect(screen.getByText("· about 25 minutes left")).toBeInTheDocument();
    // Screen readers hear the step, not every count.
    expect(screen.getByText("Importing from Plex: Fetching posters and details")).toHaveAttribute("aria-live", "polite");
    expect(screen.getByText("· 1,204 of 3,500").closest("[aria-live]")).toBeNull();
    expect(screen.getByText("· about 25 minutes left").closest("[aria-live]")).toBeNull();
    expect(screen.queryByRole("button", { name: "See what came over" })).not.toBeInTheDocument();

    progress = { ...idle, state: "done", report };
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    expect(await screen.findByText("Imported from Plex on Nov 8, 2026")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "See what came over" }));
    expect(onReport).toHaveBeenCalled();
    // Settings › Libraries keeps it: only Home puts it away.
    expect(screen.queryByRole("button", { name: "Close" })).not.toBeInTheDocument();
  });

  it("fetches the account and profiles again as the import moves on, for the pictures it brings", async () => {
    let progress: ImportProgress = { ...idle, state: "running", step: "profiles", done: 0, total: 3, started_at: 4242 };
    mockApi({ "GET /api/import/plex": () => progress });
    const { client } = renderWithProviders(<PlexImportStatus onReport={() => {}} onAgain={() => {}} />);
    expect(await screen.findByText("Importing from Plex: Making profiles")).toBeInTheDocument();
    // Fetched before the import gave the administrator Plex's picture.
    client.setQueryData(keys.me, { id: 1, avatar_url: null });
    client.setQueryData(keys.profiles, []);
    progress = { ...progress, step: "scanning", done: 0, total: 2 };
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    await waitFor(() => expect(client.getQueryState(keys.me)?.isInvalidated).toBe(true));
    expect(client.getQueryState(keys.profiles)?.isInvalidated).toBe(true);
  });

  it("shows the steps open, hidden for that import once hidden, and open again for the next", async () => {
    const user = userEvent.setup();
    const steps: ImportProgress["steps"] = [
      { step: "profiles", state: "done", count: 2 },
      { step: "fetching", state: "running", count: null },
    ];
    let progress: ImportProgress = { ...idle, state: "running", step: "fetching", done: 1, total: 4, left_secs: 20, started_at: 100, steps };
    mockApi({ "GET /api/import/plex": () => progress });
    const first = renderWithProviders(<PlexImportStatus onReport={() => {}} onAgain={() => {}} />);
    expect(await screen.findByText("· under a minute left")).toBeInTheDocument();
    await user.click(await screen.findByRole("button", { name: /Hide the steps/ }));
    expect(screen.queryByText("Making profiles")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Show the steps/ })).toHaveAttribute("aria-expanded", "false");
    first.unmount();

    // Hidden still, on coming back to it.
    const { client } = renderWithProviders(<PlexImportStatus onReport={() => {}} onAgain={() => {}} />);
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
    renderWithProviders(<PlexImportStatus onReport={() => {}} onAgain={() => {}} />);
    await user.click(await screen.findByRole("button", { name: "Stop the import" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Stop the import" }));
    await vi.waitFor(() => expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/import/plex")).toBe(true));
  });

  it("says which of the steps is running, each library scanned, and how far the whole import has come", async () => {
    const user = userEvent.setup();
    const steps: ImportProgress["steps"] = [
      { step: "profiles", state: "done", count: 3 },
      { step: "scanning", state: "running", count: null },
      { step: "labels", state: "to_come", count: null },
      { step: "fetching", state: "to_come", count: null },
    ];
    const libraries: ImportProgress["libraries"] = [
      { library: "Movies", phase: "done", done: 812, total: 812, left_secs: null, found: 812 },
      { library: "TV Shows", phase: "probing", done: 504, total: 6509, left_secs: 1500, found: null },
    ];
    const progress: ImportProgress = { ...idle, state: "running", step: "scanning", done: 1, total: 2, started_at: 100, steps, libraries };
    mockApi({ "GET /api/import/plex": () => progress });
    renderWithProviders(<PlexImportStatus onReport={() => {}} onAgain={() => {}} />);
    // Open, the steps name the step: the line says which it is.
    expect(await screen.findByText("Importing from Plex, step 2 of 4")).toBeInTheDocument();
    expect(screen.queryByText("· 1 of 2")).not.toBeInTheDocument();
    expect(screen.getByText("· 812 titles found")).toBeInTheDocument();
    expect(screen.getByText("Reading files · about 25 minutes left")).toBeInTheDocument();
    expect(screen.getByText("504 / 6,509")).toBeInTheDocument();
    // Weighed by how long each step takes: profiles (1) done, and the scans
    // (40) a library and 504 of 6,509 files into two, of 78 in all.
    const overall = (1 + 40 * ((1 + 504 / 6509) / 2)) / 78;
    expect(screen.getByRole("progressbar", { name: "The whole import" })).toHaveAttribute("aria-valuenow", String(Math.round(overall * 100)));
    expect(screen.getByText("28%")).toBeInTheDocument();
    // The steps after the next wait behind a button.
    expect(screen.queryByText("Fetching posters and details")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "1 more step" }));
    expect(screen.getByText("Fetching posters and details")).toBeInTheDocument();

    // Hidden, the line names the step and counts it.
    await user.click(screen.getByRole("button", { name: /Hide the steps/ }));
    expect(screen.getByText("Importing from Plex, step 2 of 4: Scanning the new libraries")).toBeInTheDocument();
    expect(screen.getByText("· 1 of 2")).toBeInTheDocument();
    // The scans' time left is the library whose files are read.
    expect(screen.getByText("· about 25 minutes left")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Stop the import" })).not.toBeInTheDocument();
  });

  it("says when a restart cut the import short, or it failed, with the way to import again", async () => {
    const user = userEvent.setup();
    // Cut short on 8 November 2026: no report to see.
    let progress: ImportProgress = { ...idle, state: "interrupted", started_at: 1_794_139_200 };
    mockApi({ "GET /api/import/plex": () => progress });
    const onAgain = vi.fn();
    const { client } = renderWithProviders(<PlexImportStatus onReport={() => {}} onAgain={onAgain} />);
    expect(await screen.findByText("The Plex import was cut short when Pley restarted on Nov 8, 2026")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "See what came over" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Import again" }));
    expect(onAgain).toHaveBeenCalled();

    progress = { ...idle, state: "failed", report: { ...report, outcome: "failed", error: "Plex stopped answering" } };
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    expect(await screen.findByText("Plex stopped answering")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Import again" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "See what came over" })).toBeInTheDocument();

    // One that finished has nothing left to bring.
    progress = { ...idle, state: "done", report };
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    expect(await screen.findByText("Imported from Plex on Nov 8, 2026")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Import again" })).not.toBeInTheDocument();
  });

  it("shows nothing before any import", async () => {
    const requests = mockApi({ "GET /api/import/plex": () => idle });
    const { container } = renderWithProviders(<PlexImportStatus onReport={() => {}} onAgain={() => {}} />);
    await vi.waitFor(() => expect(requests.some((r) => r.path === "/api/import/plex")).toBe(true));
    expect(container).toBeEmptyDOMElement();
  });
});

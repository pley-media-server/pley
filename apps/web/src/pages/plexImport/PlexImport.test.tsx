import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { plexImportKeys, type ImportPlan, type ImportProgress, type ImportReport, type PlexServer } from "@/api/plexImport";
import type { Library } from "@/api/types";
import { apiError, mockApi, renderWithProviders } from "@/test/render";
import { PlexImport } from "./PlexImport";

const films: Library = { id: 7, name: "Films", kind: "movies", paths: ["/media/films"], item_count: 3, created_at: 0, scanned_at: null, scanning: false };

const plan: ImportPlan = {
  server: "Den",
  libraries: [
    {
      key: "1",
      title: "Movies",
      kind: "movies",
      locations: ["/data/movies"],
      pairing: { status: "matched", library_id: 7, folders: [{ plex: "/data/movies", pley: "/media/films" }], folders_here: [] },
    },
    {
      key: "2",
      title: "TV Shows",
      kind: "shows",
      locations: ["/data/tv"],
      pairing: { status: "not_in_pley", library_id: null, folders: [], folders_here: [] },
    },
  ],
  users: [
    { id: 1, title: "Alex", admin: true, restricted: false, protected: false, profile_id: null, limits: noLimits() },
    {
      id: 2,
      title: "Sam",
      admin: false,
      restricted: true,
      protected: true,
      profile_id: null,
      limits: { ...noLimits(), max_age: 7, max_age_system: "age", show_labels: ["Family"], not_kept: ["contentRating=X"] },
    },
    { id: 3, title: "Robin", admin: false, restricted: false, protected: false, profile_id: null, limits: noLimits() },
  ],
};

function noLimits() {
  return { max_age: null, max_age_system: null, show_labels: [], hide_labels: [], left_over: [], not_kept: [] };
}

const report: ImportReport = {
  finished_at: 0,
  outcome: "done",
  error: null,
  libraries: [{ title: "Movies", library_id: 7, created: false, matched: 41, missed: 2, missed_titles: ["A Film", "Another"] }],
  profiles: [
    {
      title: "Sam",
      user_id: 9,
      created: true,
      skipped: null,
      max_age: 7,
      show_labels: ["Family"],
      hide_labels: [],
      left_over: [],
      not_kept: [],
      watched: 12,
      resumed: 1,
      rated: 0,
      plays: 30,
      playlists: 2,
      incomplete: false,
    },
  ],
  labels_added: 5,
  dates_moved: 40,
  matches_fixed: 3,
  edits: { summary: 2, content_rating: 9 },
  collections_made: 1,
  collection_items: 4,
  smart_left: 0,
};

function serve({
  signedIn = true,
  progress = () => idle,
  servers = [{ id: "m1", name: "Den", owned: true, version: "1.41.0" }],
  connect = () => ({ name: "Den" }),
}: {
  signedIn?: boolean | null;
  progress?: () => ImportProgress;
  servers?: PlexServer[];
  connect?: () => unknown;
} = {}) {
  return mockApi({
    "GET /api/import/plex": () => progress(),
    "GET /api/import/plex/pin": () => (signedIn === null ? apiError(409, "plex_signed_out", "sign in with Plex first") : { signed_in: signedIn }),
    "POST /api/import/plex/pin": () => {
      signedIn = false;
      return { code: 1, auth_url: "https://app.plex.tv/auth#?code=abc" };
    },
    "GET /api/import/plex/servers": () => servers,
    "POST /api/import/plex/server": () => connect(),
    "GET /api/import/plex/plan": () => plan,
    "POST /api/import/plex/start": () => undefined,
    "GET /api/libraries": () => [films],
    "GET /api/ratings/systems": () => ({ region: "US", systems: [] }),
    "POST /api/fs": (req) => {
      const { path } = req.body as { path: string | null };
      return { path, parent: null, entries: path ? [] : [{ name: "/", path: "/" }] };
    },
  });
}

const idle: ImportProgress = { state: "idle", step: null, done: 0, total: 0, error: null, report: null };

describe("PlexImport", () => {
  it("brings the libraries and people chosen, then shows the report", async () => {
    const user = userEvent.setup();
    let progress = idle;
    const requests = serve({ progress: () => progress });
    const onFinish = vi.fn();
    const { client } = renderWithProviders(<PlexImport onLeave={() => {}} onFinish={onFinish} leaveLabel="Cancel" />);

    // Signed in already, with one server: straight to the libraries.
    expect(await screen.findByRole("heading", { name: "Libraries on Den" })).toBeInTheDocument();
    expect(requests.find((r) => r.method === "POST" && r.path === "/api/import/plex/server")?.body).toEqual({ id: "m1" });
    expect(screen.getByLabelText("Where Movies goes")).toHaveValue("7");
    // Pley can't open TV Shows' folder: left out until it's said where it is.
    const tv = screen.getByLabelText("Where TV Shows goes");
    expect(tv).toHaveValue("leave");
    await user.selectOptions(tv, "new");
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Say where at least one of its folders is for Pley.");
    await user.type(screen.getByLabelText("Where Pley finds /data/tv"), "/media/tv");
    await user.click(screen.getByRole("button", { name: "Continue" }));

    expect(await screen.findByRole("heading", { name: "Who comes over" })).toBeInTheDocument();
    expect(screen.getByText("Up to age 7")).toBeInTheDocument();
    expect(screen.getByText("Always: Family")).toBeInTheDocument();
    expect(screen.getByText("contentRating=X")).toBeInTheDocument();
    await user.click(screen.getByRole("switch", { name: "Bring Robin" }));
    await user.type(screen.getByLabelText("Plex PIN"), "12a34");
    await user.click(screen.getByRole("button", { name: "Continue" }));

    expect(await screen.findByRole("heading", { name: "What to bring" })).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Watch history" })).toBeChecked();
    await user.click(screen.getByRole("switch", { name: "Matches and edits" }));
    progress = { ...idle, state: "running", step: "matching", done: 10, total: 40 };
    await user.click(screen.getByRole("button", { name: "Start the import" }));

    await waitFor(() =>
      expect(requests.find((r) => r.path === "/api/import/plex/start")?.body).toEqual({
        libraries: [
          { key: "1", library_id: 7 },
          { key: "2", new: { name: "TV Shows", paths: ["/media/tv"] } },
        ],
        folders: [
          { plex: "/data/movies", pley: "/media/films" },
          { plex: "/data/tv", pley: "/media/tv" },
        ],
        users: [{ id: 2, pin: "1234" }],
        bring: { watch_history: true, collections: true, edits: false },
      }),
    );
    expect(await screen.findByText("Finding Plex's titles in Pley")).toBeInTheDocument();
    expect(screen.getByText("10 / 40")).toBeInTheDocument();

    // What an `import_changed` event does.
    progress = { ...idle, state: "done", report };
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    expect(await screen.findByRole("heading", { name: "Your Plex library is in Pley" })).toBeInTheDocument();
    expect(screen.getByText("41 titles found · 2 not found")).toBeInTheDocument();
    expect(screen.getByText("12 watched · 1 to resume · 0 rated · 30 plays · 2 playlists")).toBeInTheDocument();
    expect(
      screen.getByText("5 labels added · 40 dates added moved earlier · 3 titles matched as in Plex · 1 collection made · 4 titles put in collections"),
    ).toBeInTheDocument();
    // Most edited first.
    expect(screen.getAllByRole("listitem").map((li) => li.textContent).filter((s) => /^(Summary|Content rating)/.test(s ?? ""))).toEqual([
      "Content rating9",
      "Summary2",
    ]);
    await user.click(screen.getByRole("button", { name: "Done" }));
    expect(onFinish).toHaveBeenCalled();
  });

  it("asks for the owner's account when the servers are only shared with this one", async () => {
    const user = userEvent.setup();
    const requests = serve({ servers: [{ id: "f1", name: "A friend's", owned: false, version: null }] });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" />);

    expect(await screen.findByText(/belong to someone else \(A friend's\)/)).toBeInTheDocument();
    expect(screen.queryByLabelText("Or its address")).not.toBeInTheDocument();
    expect(requests.some((r) => r.method === "POST" && r.path === "/api/import/plex/server")).toBe(false);
    await user.click(screen.getByRole("button", { name: "Sign in with another account" }));
    expect(await screen.findByRole("button", { name: "Sign in with Plex" })).toBeInTheDocument();
  });

  it("signs in through Plex's page, opened in a window", async () => {
    const user = userEvent.setup();
    const popup = { closed: false, location: { href: "" }, close: vi.fn() };
    const open = vi.spyOn(window, "open").mockReturnValue(popup as unknown as Window);
    serve({ signedIn: null });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" />);

    await user.click(await screen.findByRole("button", { name: "Sign in with Plex" }));
    expect(open).toHaveBeenCalled();
    await waitFor(() => expect(popup.location.href).toBe("https://app.plex.tv/auth#?code=abc"));
    expect(screen.getByRole("link", { name: "Open Plex's sign-in page" })).toHaveAttribute("href", "https://app.plex.tv/auth#?code=abc");
    open.mockRestore();
  });

  it("shows an import already running, through to its report", async () => {
    let progress: ImportProgress = { ...idle, state: "running", step: "history", done: 1, total: 3 };
    serve({ progress: () => progress });
    const { client } = renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" />);
    const step = await screen.findByText("Bringing everyone's plays");
    expect(within(step.closest("section")!).getByRole("button", { name: "Stop the import" })).toBeInTheDocument();
    progress = { ...idle, state: "done", report };
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    expect(await screen.findByRole("heading", { name: "Your Plex library is in Pley" })).toBeInTheDocument();
  });

  it("leaves out what wasn't brought, and says when playlists were refused", async () => {
    const profile = { ...report.profiles[0]!, playlists_incomplete: true };
    const bring = { watch_history: false, collections: true, edits: true };
    let progress: ImportProgress = { ...idle, state: "running", step: "playlists", done: 1, total: 3 };
    serve({ progress: () => progress });
    const { client } = renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" />);
    expect(await screen.findByText("Bringing playlists")).toBeInTheDocument();
    progress = { ...idle, state: "done", report: { ...report, profiles: [profile], bring } };
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    expect(await screen.findByText("2 playlists")).toBeInTheDocument();
    expect(screen.queryByText(/watched/)).not.toBeInTheDocument();
    expect(screen.getByText("Plex refused some of their playlists, so not all of them came over.")).toBeInTheDocument();
  });

  it("picks where a Plex folder is with the folder picker", async () => {
    const user = userEvent.setup();
    serve();
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" />);
    await user.selectOptions(await screen.findByLabelText("Where TV Shows goes"), "new");
    await user.click(screen.getByRole("button", { name: "Browse for where Pley finds /data/tv" }));
    const picker = await screen.findByRole("dialog", { name: "Choose a folder" });
    await user.type(within(picker).getByLabelText("Folder path"), "/volume1/video/tv{Enter}");
    await user.click(within(picker).getByRole("button", { name: "Select this folder" }));
    expect(screen.getByLabelText("Where Pley finds /data/tv")).toHaveValue("/volume1/video/tv");
  });

  it("connects to the only server without showing the list of servers", async () => {
    let connected = () => {};
    serve({ connect: () => new Promise((resolve) => (connected = () => resolve({ name: "Den" }))) });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" />);
    expect(await screen.findByText("Connecting to Den…")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Which server?" })).not.toBeInTheDocument();
    connected();
    expect(await screen.findByRole("heading", { name: "Libraries on Den" })).toBeInTheDocument();
  });

  it("goes back to the only server without connecting again, and keeps the choices", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" />);
    await user.selectOptions(await screen.findByLabelText("Where Movies goes"), "leave");
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByRole("heading", { name: "Which server?" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByLabelText("Where Movies goes")).toHaveValue("leave");
    expect(requests.filter((r) => r.path === "/api/import/plex/server")).toHaveLength(2);
    expect(requests.filter((r) => r.path === "/api/import/plex/plan")).toHaveLength(1);
  });

  it("says an import lost to a restart was interrupted", async () => {
    let progress: ImportProgress = { ...idle, state: "running", step: "matching", done: 1, total: 3 };
    serve({ progress: () => progress });
    const { client } = renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" />);
    expect(await screen.findByText("Finding Plex's titles in Pley")).toBeInTheDocument();
    // The server restarted: idle, and no report.
    progress = idle;
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    expect(await screen.findByRole("heading", { name: "The import was interrupted" })).toBeInTheDocument();
  });
});

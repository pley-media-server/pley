import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { plexImportKeys, type FolderCheck, type ImportPlan, type ImportProgress, type ImportReport, type PlexServer } from "@/api/plexImport";
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
      items: 812,
      groups: null,
      pairing: { status: "matched", library_id: 7, folders: [{ plex: "/data/movies", pley: "/media/films" }], folders_here: [], folders_found: [] },
    },
    {
      key: "2",
      title: "TV Shows",
      kind: "shows",
      locations: ["/data/tv"],
      items: 6509,
      groups: 64,
      pairing: { status: "not_in_pley", library_id: null, folders: [], folders_here: [], folders_found: [] },
    },
  ],
  users: [
    { id: 1, title: "Alex", admin: true, restricted: false, protected: false, profile_id: null, limits: noLimits(), picture: true, color: null },
    {
      id: 2,
      title: "Sam",
      admin: false,
      restricted: true,
      protected: true,
      profile_id: null,
      limits: { ...noLimits(), max_age: 7, max_age_system: "age", show_labels: ["Family"], not_kept: ["contentRating=X"] },
      picture: false,
      color: "#eb9f9f",
    },
    { id: 3, title: "Robin", admin: false, restricted: false, protected: false, profile_id: null, limits: noLimits(), picture: false, color: null },
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
  thePlan = plan,
  pleyLibraries = [films],
  exists = () => true,
  check = () => "untold",
  pinAfter = 0,
}: {
  signedIn?: boolean | null;
  progress?: () => ImportProgress;
  servers?: PlexServer[];
  connect?: () => unknown;
  /** Null: Plex doesn't answer. */
  thePlan?: ImportPlan | null;
  pleyLibraries?: Library[];
  /** Whether a folder is there for Pley. */
  exists?: (path: string) => boolean;
  /** What Pley finds of a library's Plex files in a folder. */
  check?: (folder: { library: string; plex: string; pley: string }) => FolderCheck;
  /** How long the server takes to say whether it's signed in, in ms. */
  pinAfter?: number;
} = {}) {
  return mockApi({
    "GET /api/import/plex": () => progress(),
    "GET /api/import/plex/pin": async () => {
      await new Promise((resolve) => setTimeout(resolve, pinAfter));
      return signedIn === null ? apiError(409, "plex_signed_out", "sign in with Plex first") : { signed_in: signedIn };
    },
    "POST /api/import/plex/pin": () => {
      signedIn = false;
      return { code: 1, auth_url: "https://app.plex.tv/auth#?code=abc" };
    },
    "GET /api/import/plex/servers": () => servers,
    "POST /api/import/plex/server": () => connect(),
    "GET /api/import/plex/plan": () => thePlan ?? apiError(502, "plex_unreachable", "Plex didn't answer"),
    "POST /api/import/plex/start": () => undefined,
    "POST /api/import/plex/folder": (req) => ({ state: check(req.body as { library: string; plex: string; pley: string }) }),
    "GET /api/libraries": () => pleyLibraries,
    "GET /api/ratings/systems": () => ({ region: "US", systems: [] }),
    "POST /api/fs": (req) => {
      const { path } = req.body as { path: string | null };
      if (path && !exists(path)) return apiError(400, "bad_request", `cannot open ${path}`);
      return { path, parent: null, entries: path ? [] : [{ name: "/", path: "/" }] };
    },
  });
}

const idle: ImportProgress = { state: "idle", step: null, done: 0, total: 0, error: null, report: null, first_pass_done: false };

describe("PlexImport", () => {
  it("brings the libraries and people chosen, then shows the report", async () => {
    const user = userEvent.setup();
    let progress = idle;
    const requests = serve({ progress: () => progress });
    const onFinish = vi.fn();
    const { client } = renderWithProviders(<PlexImport onLeave={() => {}} onFinish={onFinish} leaveLabel="Cancel" goOnLabel="Start watching" />);

    // Signed in already, with one server: straight to the libraries.
    expect(await screen.findByRole("heading", { name: "Libraries on Den" })).toBeInTheDocument();
    expect(requests.find((r) => r.method === "POST" && r.path === "/api/import/plex/server")?.body).toEqual({ id: "m1" });
    expect(screen.getByRole("switch", { name: "Bring Movies" })).toBeChecked();
    expect(screen.getByText(/Goes into/)).toHaveTextContent("Goes into Films");
    expect(screen.getByText("812 movies")).toBeInTheDocument();
    expect(screen.getByText("64 shows · 6,509 episodes")).toBeInTheDocument();
    // Pley has no library of shows to put TV Shows in, and can't open its folder: left out, and its folder asked for.
    const tv = screen.getByRole("switch", { name: "Bring TV Shows" });
    expect(tv).not.toBeChecked();
    // Giving one Pley can open brings the library, under Plex's name.
    await user.type(screen.getByLabelText("Plex sees /data/tv. Where does Pley see it?"), "/media/tv");
    await waitFor(() => expect(tv).toBeChecked());
    expect(screen.getAllByText(/Goes into/)).toHaveLength(1);
    await user.clear(screen.getByLabelText("Plex sees /data/tv. Where does Pley see it?"));
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Say where at least one of its folders is for Pley.");
    await user.type(screen.getByLabelText("Plex sees /data/tv. Where does Pley see it?"), "/media/tv");
    await user.click(screen.getByRole("button", { name: "Continue" }));

    expect(await screen.findByRole("heading", { name: "Who comes over" })).toBeInTheDocument();
    // Alex's Plex picture; Sam's initial on Plex's colour for them.
    expect(document.querySelector('img[src="/api/import/plex/users/1/picture"]')).toBeInTheDocument();
    expect(screen.getByText("S", { exact: true })).toHaveStyle({ backgroundColor: "#eb9f9f" });
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

  it("goes where it's told once the import starts, instead of its progress", async () => {
    const user = userEvent.setup();
    let progress = idle;
    const requests = serve({ progress: () => progress });
    const onStarted = vi.fn();
    renderWithProviders(
      <PlexImport onLeave={() => {}} onStarted={onStarted} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />,
    );
    expect(await screen.findByRole("heading", { name: "Libraries on Den" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await user.click(await screen.findByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("heading", { name: "What to bring" })).toBeInTheDocument();
    progress = { ...idle, state: "running", step: "profiles" };
    await user.click(screen.getByRole("button", { name: "Start the import" }));
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(requests.some((r) => r.path === "/api/import/plex/start")).toBe(true);
    expect(screen.queryByRole("heading", { name: "Importing from Plex" })).not.toBeInTheDocument();
  });

  it("asks for the owner's account when the servers are only shared with this one", async () => {
    const user = userEvent.setup();
    const requests = serve({ servers: [{ id: "f1", name: "A friend's", owned: false, version: null }] });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);

    expect(await screen.findByText(/belong to someone else \(A friend's\)/)).toBeInTheDocument();
    expect(screen.queryByLabelText("Or its address")).not.toBeInTheDocument();
    expect(requests.some((r) => r.method === "POST" && r.path === "/api/import/plex/server")).toBe(false);
    await user.click(screen.getByRole("button", { name: "Sign in with another account" }));
    expect(await screen.findByRole("button", { name: "Sign in with Plex" })).toBeInTheDocument();
  });

  it("waits for the new sign-in when signing in again, not the one before", async () => {
    const user = userEvent.setup();
    const popup = { closed: false, location: { href: "" }, close: vi.fn() };
    const open = vi.spyOn(window, "open").mockReturnValue(popup as unknown as Window);
    serve({ servers: [{ id: "f1", name: "A friend's", owned: false, version: null }], pinAfter: 50 });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);

    await user.click(await screen.findByRole("button", { name: "Sign in with another account" }));
    await user.click(await screen.findByRole("button", { name: "Sign in with Plex" }));
    await waitFor(() => expect(popup.location.href).toBe("https://app.plex.tv/auth#?code=abc"));
    await new Promise((resolve) => setTimeout(resolve, 150));
    expect(popup.close).not.toHaveBeenCalled();
    expect(screen.getByRole("link", { name: "Open Plex's sign-in page" })).toBeInTheDocument();
    open.mockRestore();
  });

  it("signs in through Plex's page, opened in a window", async () => {
    const user = userEvent.setup();
    const popup = { closed: false, location: { href: "" }, close: vi.fn() };
    const open = vi.spyOn(window, "open").mockReturnValue(popup as unknown as Window);
    serve({ signedIn: null });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);

    await user.click(await screen.findByRole("button", { name: "Sign in with Plex" }));
    expect(open).toHaveBeenCalled();
    await waitFor(() => expect(popup.location.href).toBe("https://app.plex.tv/auth#?code=abc"));
    expect(screen.getByRole("link", { name: "Open Plex's sign-in page" })).toHaveAttribute("href", "https://app.plex.tv/auth#?code=abc");
    open.mockRestore();
  });

  it("shows an import already running, through to its report", async () => {
    let progress: ImportProgress = { ...idle, state: "running", step: "history", done: 1, total: 3 };
    serve({ progress: () => progress });
    const { client } = renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
    const step = await screen.findByText("Bringing everyone's plays");
    expect(within(step.closest("section")!).getByRole("button", { name: "Stop the import" })).toBeInTheDocument();
    progress = { ...idle, state: "done", report };
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    expect(await screen.findByRole("heading", { name: "Your Plex library is in Pley" })).toBeInTheDocument();
  });

  it("says the step to screen readers when it changes, not each count", async () => {
    const steps: ImportProgress["steps"] = [
      { step: "profiles", state: "done", count: 2 },
      { step: "history", state: "running", count: null },
    ];
    serve({ progress: () => ({ ...idle, state: "running", step: "history", done: 1, total: 3, steps }) });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
    const list = await screen.findByRole("list");
    expect(list.closest("[aria-live]")).toBeNull();
    const live = document.querySelector("[aria-live]");
    expect(live).toHaveTextContent("Bringing everyone's plays");
    expect(live).toHaveClass("sr-only");
  });

  it("says which library is being scanned, and its files read", async () => {
    let progress: ImportProgress = {
      ...idle,
      state: "running",
      step: "scanning",
      done: 0,
      total: 2,
      scan: { library: "Movies", phase: "walking", done: 0, total: 0 },
    };
    serve({ progress: () => progress });
    const { client } = renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
    expect(await screen.findByText("Movies: finding files")).toBeInTheDocument();
    expect(screen.getByText("0 / 2")).toBeInTheDocument();

    progress = { ...progress, done: 1, scan: { library: "TV Shows", phase: "probing", done: 2140, total: 6509 } };
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    expect(await screen.findByText("TV Shows: reading files")).toBeInTheDocument();
    expect(screen.getByText("2,140 / 6,509")).toBeInTheDocument();
    expect(screen.getByRole("progressbar", { name: "TV Shows: reading files" })).toBeInTheDocument();

    progress = { ...progress, scan: { library: "Music", phase: "queued", done: 0, total: 0 } };
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    expect(await screen.findByText("Music: waiting to start")).toBeInTheDocument();
    expect(screen.queryByRole("progressbar", { name: "Music: waiting to start" })).not.toBeInTheDocument();
  });

  it("leaves out what wasn't brought, and says when playlists were refused", async () => {
    const profile = { ...report.profiles[0]!, playlists_incomplete: true };
    const bring = { watch_history: false, collections: true, edits: true };
    let progress: ImportProgress = { ...idle, state: "running", step: "playlists", done: 1, total: 3 };
    serve({ progress: () => progress });
    const { client } = renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
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
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
    expect(await screen.findByRole("switch", { name: "Bring TV Shows" })).not.toBeChecked();
    await user.click(screen.getByRole("button", { name: "Browse for where Pley finds /data/tv" }));
    const picker = await screen.findByRole("dialog", { name: "Choose a folder" });
    await user.type(within(picker).getByLabelText("Folder path"), "/volume1/video/tv{Enter}");
    await user.click(within(picker).getByRole("button", { name: "Select this folder" }));
    expect(screen.getByLabelText("Plex sees /data/tv. Where does Pley see it?")).toHaveValue("/volume1/video/tv");
    await waitFor(() => expect(screen.getByRole("switch", { name: "Bring TV Shows" })).toBeChecked());
  });

  it("leaves a library out while Pley can't open the folder given for it", async () => {
    const user = userEvent.setup();
    serve({ check: ({ pley }) => (pley === "/nowhere" ? "missing" : "none_found") });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
    const tv = await screen.findByRole("switch", { name: "Bring TV Shows" });
    const folder = screen.getByLabelText("Plex sees /data/tv. Where does Pley see it?");
    await user.type(folder, "/nowhere");
    expect(await screen.findByText("Pley can't open this folder")).toBeInTheDocument();
    expect(tv).not.toBeChecked();
    // A folder it opens brings it, even with none of Plex's files in it, to be checked.
    await user.clear(folder);
    await user.type(folder, "/media/other");
    expect(await screen.findByText("None of Plex's files are here")).toBeInTheDocument();
    expect(tv).toBeChecked();
  });

  it("says whether Plex's files are in each folder, as the folder changes", async () => {
    const user = userEvent.setup();
    const states: Record<string, FolderCheck> = { "/media/tv": "found", "/media/films": "none_found", "/nowhere": "missing" };
    const requests = serve({ check: ({ pley }) => states[pley] ?? "untold" });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
    const folder = await screen.findByLabelText("Plex sees /data/tv. Where does Pley see it?");
    await user.type(folder, "/media/tv");
    expect(await screen.findByText("Plex's files are here")).toBeInTheDocument();
    expect(requests.find((r) => r.path === "/api/import/plex/folder")?.body).toEqual({ library: "2", plex: "/data/tv", pley: "/media/tv" });
    await user.clear(folder);
    await user.type(folder, "/media/films");
    expect(await screen.findByText("None of Plex's files are here")).toBeInTheDocument();
    await user.clear(folder);
    // Nothing's said of a folder until it's settled.
    expect(screen.queryByText("None of Plex's files are here")).not.toBeInTheDocument();
    await user.type(folder, "/nowhere");
    expect(await screen.findByText("Pley can't open this folder")).toBeInTheDocument();
    // Nor where Pley can't tell.
    await user.clear(folder);
    await user.type(folder, "/elsewhere");
    await waitFor(() => expect(requests.some((r) => (r.body as { pley?: string } | undefined)?.pley === "/elsewhere")).toBe(true));
    expect(screen.queryByText(/files are here|can't open/)).not.toBeInTheDocument();
  });

  it("goes back to the servers when Plex's can't be read", async () => {
    const user = userEvent.setup();
    serve({ thePlan: null });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
    expect(await screen.findByRole("heading", { name: "Pley couldn't read your Plex server" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByRole("heading", { name: "Which server?" })).toBeInTheDocument();
  });

  it("brings a library back into the Pley library with its files once turned on again", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
    const movies = await screen.findByRole("switch", { name: "Bring Movies" });
    await user.click(movies);
    expect(screen.queryByText(/Goes into/)).not.toBeInTheDocument();
    await user.click(movies);
    expect(screen.getByText(/Goes into/)).toHaveTextContent("Goes into Films");
    // There's nothing to name: a new library takes Plex's name.
    expect(screen.queryByRole("textbox", { name: /name/i })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Continue" }));
    await user.click(await screen.findByRole("button", { name: "Continue" }));
    await user.click(await screen.findByRole("button", { name: "Start the import" }));
    await waitFor(() =>
      expect((requests.find((r) => r.path === "/api/import/plex/start")?.body as { libraries: unknown[] }).libraries).toEqual([{ key: "1", library_id: 7 }]),
    );
  });

  it("connects to the only server without showing the list of servers", async () => {
    let connected = () => {};
    serve({ connect: () => new Promise((resolve) => (connected = () => resolve({ name: "Den" }))) });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
    expect(await screen.findByText("Connecting to Den…")).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Which server?" })).not.toBeInTheDocument();
    connected();
    expect(await screen.findByRole("heading", { name: "Libraries on Den" })).toBeInTheDocument();
  });

  it("goes back to the only server without connecting again, and keeps the choices", async () => {
    const user = userEvent.setup();
    const requests = serve();
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
    await user.click(await screen.findByRole("switch", { name: "Bring Movies" }));
    await user.click(screen.getByRole("button", { name: "Back" }));
    expect(await screen.findByRole("heading", { name: "Which server?" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Continue" }));
    expect(await screen.findByRole("switch", { name: "Bring Movies" })).not.toBeChecked();
    expect(requests.filter((r) => r.path === "/api/import/plex/server")).toHaveLength(2);
    expect(requests.filter((r) => r.path === "/api/import/plex/plan")).toHaveLength(1);
  });

  it("lets people go on while it runs, saying so once what's found by file has come over", async () => {
    const user = userEvent.setup();
    let progress: ImportProgress = { ...idle, state: "running", step: "watch_state", done: 1, total: 2 };
    serve({ progress: () => progress });
    const onFinish = vi.fn();
    const { client } = renderWithProviders(<PlexImport onLeave={() => {}} onFinish={onFinish} leaveLabel="Cancel" goOnLabel="Start watching" />);
    expect(await screen.findByText("Bringing what everyone watched")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Importing from Plex" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Start watching" })).toBeInTheDocument();

    progress = { ...progress, step: "fetching", done: 1204, total: 3500, first_pass_done: true };
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    expect(await screen.findByRole("heading", { name: "Your Plex data is in" })).toBeInTheDocument();
    expect(screen.getByText("Fetching posters and details")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Stop the import" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Start watching" }));
    expect(onFinish).toHaveBeenCalled();
  });

  it("lists every step: those done with what they brought, the one running, the rest to come", async () => {
    const progress: ImportProgress = {
      ...idle,
      state: "running",
      step: "scanning",
      done: 1,
      total: 2,
      scan: { library: "TV Shows", phase: "probing", done: 4510, total: 6509 },
      steps: [
        { step: "profiles", state: "done", count: 2 },
        { step: "scanning", state: "running", count: null },
        { step: "labels", state: "to_come", count: null },
        { step: "watch_state", state: "to_come", count: null },
        { step: "fetching", state: "to_come", count: null },
      ],
    };
    serve({ progress: () => progress });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
    const steps = within(await screen.findByRole("list"));
    expect(steps.getAllByRole("listitem").map((item) => item.textContent)).toEqual([
      "Making profiles· 2 profiles",
      "Scanning the new libraries1 / 2TV Shows: reading files4,510 / 6,509",
      "Bringing labels and dates added",
      "Bringing what everyone watched",
      "Fetching posters and details",
    ]);
    expect(steps.getByRole("progressbar", { name: "Scanning the new libraries" })).toBeInTheDocument();
  });

  it("shows the second round's steps under finding the rest", async () => {
    const progress: ImportProgress = {
      ...idle,
      state: "running",
      step: "watch_state",
      done: 0,
      total: 1,
      first_pass_done: true,
      steps: [
        { step: "watch_state", state: "done", count: 12 },
        { step: "fetching", state: "done", count: null },
        { step: "matching_again", state: "running", count: null },
      ],
    };
    serve({ progress: () => progress });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
    const steps = within(await screen.findByRole("list"));
    expect(steps.getByText("· 12 watched")).toBeInTheDocument();
    expect(steps.getByText("Finding the rest of Plex's titles")).toBeInTheDocument();
    // What it does for the titles found, under the step it's part of.
    expect(steps.getByText("Bringing what everyone watched", { selector: "p" })).toBeInTheDocument();
  });

  it("says which library still waits for Plex's labels when the import didn't finish, and which titles its labels miss", async () => {
    const hiding = { ...report.profiles[0]!, hide_labels: ["Scary"] };
    const pending = { ...report.libraries[0]!, labels_pending: true };
    serve({ progress: () => ({ ...idle, state: "cancelled", report: { ...report, outcome: "cancelled", libraries: [pending], profiles: [hiding] } }) });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Close" report />);
    expect(await screen.findByText(/^Still waiting for Plex's labels/)).toBeInTheDocument();
    expect(screen.getByText(/^Titles Pley couldn't find don't have Plex's labels/)).toBeInTheDocument();
  });

  it("says a profile found again kept its own age limit rather than hiding more", async () => {
    const found = { ...report.profiles[0]!, not_kept: ["genre=Kids"], kept_limit: true };
    serve({ progress: () => ({ ...idle, state: "done", report: { ...report, profiles: [found] } }) });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Close" report />);
    expect(await screen.findByText(/the profile keeps the age limit it has in Pley/)).toBeInTheDocument();
    expect(screen.queryByText(/so it hides more instead/)).not.toBeInTheDocument();
  });

  it("says nothing of waiting once the labels are on", async () => {
    serve({ progress: () => ({ ...idle, state: "done", report }) });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Close" report />);
    expect(await screen.findByText("Sam")).toBeInTheDocument();
    expect(screen.queryByText(/waiting for Plex's labels/)).not.toBeInTheDocument();
    expect(screen.queryByText(/^Titles Pley couldn't find/)).not.toBeInTheDocument();
  });

  it("signs in again to import again, since a run forgets the sign-in", async () => {
    const user = userEvent.setup();
    serve({ progress: () => ({ ...idle, state: "cancelled", report: { ...report, outcome: "cancelled" } }) });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Close" report />);
    await user.click(await screen.findByRole("button", { name: "Import again" }));
    expect(await screen.findByRole("heading", { name: "Sign in with Plex" })).toBeInTheDocument();
  });

  it("opens on the last import's report when asked", async () => {
    serve({ progress: () => ({ ...idle, state: "done", report }) });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Close" report />);
    expect(await screen.findByRole("heading", { name: "Your Plex library is in Pley" })).toBeInTheDocument();
    expect(screen.queryByText(/^Took/)).not.toBeInTheDocument();
  });

  it("says how long the import and each of its steps took", async () => {
    const steps: ImportReport["steps"] = [
      { step: "profiles", secs: 2 },
      { step: "scanning", secs: 2400 },
      { step: "fetching", secs: 1920 },
    ];
    serve({ progress: () => ({ ...idle, state: "done", report: { ...report, started_at: 1000, finished_at: 1000 + 72 * 60, steps } }) });
    renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Close" report />);
    await userEvent.setup().click(await screen.findByText("Took 1h 12m"));
    const times = screen.getAllByRole("listitem").filter((li) => li.closest("details"));
    expect(times.map((li) => li.textContent)).toEqual(["Making profiles2s", "Scanning the new libraries40m", "Fetching posters and details32m"]);
  });

  it("says an import lost to a restart was interrupted", async () => {
    let progress: ImportProgress = { ...idle, state: "running", step: "matching", done: 1, total: 3 };
    serve({ progress: () => progress });
    const { client } = renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
    expect(await screen.findByText("Finding Plex's titles in Pley")).toBeInTheDocument();
    // The server restarted.
    progress = { ...idle, state: "interrupted", started_at: 100 };
    await client.invalidateQueries({ queryKey: plexImportKeys.progress });
    expect(await screen.findByRole("heading", { name: "The import was interrupted" })).toBeInTheDocument();
  });

  describe("on a server with no libraries, Plex in Docker", () => {
    const notInPley = (key: string, title: string, kind: "movies" | "shows" | "music", location: string, found: string | null = null) => ({
      key,
      title,
      kind,
      locations: [location],
      pairing: {
        status: "not_in_pley" as const,
        library_id: null,
        folders: [],
        folders_here: [],
        folders_found: found ? [{ plex: location, pley: found }] : [],
      },
    });
    const docker: ImportPlan = {
      ...plan,
      libraries: [
        notInPley("1", "Movies", "movies", "/data/media/movies", "/media/Movies"),
        notInPley("2", "TV Shows", "shows", "/data/media/tv"),
        notInPley("3", "Music", "music", "/data/media/music"),
      ],
    };

    it("starts from the folders Pley found, says so, and shows no badges", async () => {
      serve({ thePlan: docker, pleyLibraries: [], check: ({ pley }) => (pley === "/media/Movies" ? "found" : "untold") });
      renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
      expect(await screen.findByRole("switch", { name: "Bring Movies" })).toBeChecked();
      expect(screen.getByLabelText("Plex sees /data/media/movies. Where does Pley see it?")).toHaveValue("/media/Movies");
      expect(screen.getByRole("switch", { name: "Bring TV Shows" })).not.toBeChecked();
      expect(screen.queryByText("Not in Pley")).not.toBeInTheDocument();
      expect(await screen.findByText("Plex's files are here")).toBeInTheDocument();
    });

    it("doesn't fill in a folder that isn't there", async () => {
      const user = userEvent.setup();
      // Pley sees Plex's /data/media as /mnt/user, with TV there and no music.
      serve({ thePlan: { ...docker, libraries: docker.libraries.slice(1) }, pleyLibraries: [], exists: (path) => path === "/mnt/user/tv" });
      renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
      const music = await screen.findByLabelText("Plex sees /data/media/music. Where does Pley see it?");
      await user.type(screen.getByLabelText("Plex sees /data/media/tv. Where does Pley see it?"), "/mnt/user/TV");
      await user.click(music);
      // /mnt/user/music isn't there: nothing's filled in for Music.
      await user.tab();
      expect(music).toHaveValue("");
      expect(screen.getByRole("switch", { name: "Bring Music" })).not.toBeChecked();
    });

    it("fills in a library from another's folder", async () => {
      const user = userEvent.setup();
      serve({ thePlan: { ...docker, libraries: docker.libraries.slice(1) }, pleyLibraries: [], exists: (path) => path === "/mnt/user/music" });
      renderWithProviders(<PlexImport onLeave={() => {}} onFinish={() => {}} leaveLabel="Cancel" goOnLabel="Start watching" />);
      await user.type(await screen.findByLabelText("Plex sees /data/media/tv. Where does Pley see it?"), "/mnt/user/tv");
      await user.tab();
      await waitFor(() => expect(screen.getByLabelText("Plex sees /data/media/music. Where does Pley see it?")).toHaveValue("/mnt/user/music"));
      expect(screen.getByRole("switch", { name: "Bring Music" })).toBeChecked();
    });
  });
});

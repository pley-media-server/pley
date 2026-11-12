import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18next from "i18next";
import { describe, expect, it } from "vitest";
import type { ImportProgress, ImportReport } from "@/api/plexImport";
import type { Activity, Hub, ItemDetail, Library, ScanStatus } from "@/api/types";
import { ListMenuProvider } from "@/components/lists/ListMenu";
import { makeItem } from "@/test/fixtures";
import { apiError, mockApi, renderWithProviders, testUser } from "@/test/render";
import { HomePage } from "./HomePage";
import { itemUrl, libraryUrl } from "@/lib/urls";

const movies: Library = {
  id: 1,
  name: "Movies",
  kind: "movies",
  paths: ["/media/movies"],
  item_count: 2,
  created_at: 0,
  scanned_at: 0,
  scanning: false,
};

const hubs: Hub[] = [
  {
    key: "continue_watching",
    title: "Continue Watching",
    library_id: null,
    items: [
      makeItem({ id: 6, title: "Alien", duration_ms: 2_640_000, user_state: { position_ms: 600_000, watched: false, play_count: 0 } }),
      makeItem({ id: 7, kind: "episode", title: "Pilot", grandparent_title: "Lost", parent_index: 1, index: 1 }),
    ],
  },
  { key: "recently_added_1", title: "Recently Added in Movies", library_id: 1, items: [makeItem({ id: 8, title: "Heat", year: 1995 })] },
  { key: "top_rated_1", title: "Top Rated", library_id: 1, items: [] },
];

const shows: Library = { ...movies, id: 2, name: "TV Shows", kind: "shows" };

/** Recently added with backdrops: a film and a show the banner features. */
const featuredHubs: Hub[] = [
  {
    key: "recently_added:1",
    title: "Recently Added in Movies",
    library_id: 1,
    items: [makeItem({ id: 8, title: "Heat", year: 1995, duration_ms: 10_200_000, backdrop_url: "/api/items/8/images/backdrop" })],
  },
  {
    key: "recently_added:2",
    title: "Recently Added in TV Shows",
    library_id: 2,
    items: [makeItem({ id: 9, library_id: 2, kind: "show", title: "Lost", year: 2004, backdrop_url: "/api/items/9/images/backdrop" })],
  },
];

function detail(item: Hub["items"][number], overrides: Partial<ItemDetail> = {}): ItemDetail {
  return {
    ...item,
    original_title: null,
    tagline: null,
    genres: [],
    studio: null,
    content_rating: null,
    tmdb_id: null,
    imdb_id: null,
    files: [],
    ...overrides,
  };
}

function serve({
  libraries = [movies],
  home = hubs,
  scans = [],
  progress,
}: { libraries?: Library[]; home?: Hub[] | Response; scans?: ScanStatus[]; progress?: ImportProgress } = {}) {
  return mockApi({
    "GET /api/libraries": () => libraries,
    "GET /api/hubs/home": () => (home instanceof Response ? home : { hubs: home }),
    "GET /api/activity": (): Activity => ({ scans, sessions: [] }),
    "GET /api/import/plex": () => progress ?? { ...importing, state: "idle", step: null },
  });
}

const importing: ImportProgress = {
  state: "running",
  step: "fetching",
  done: 40,
  total: 138,
  error: null,
  report: null,
  first_pass_done: true,
};

const emptyReport: ImportReport = {
  finished_at: 0,
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

const firstScan: ScanStatus = {
  library_id: 2,
  library_name: "TV Shows",
  phase: "probing",
  processed: 2140,
  total: 6509,
  started_at: 0,
  first: true,
  left_secs: 1500,
};

describe("HomePage", () => {
  it("shows the hubs that have something in them", async () => {
    serve();
    renderWithProviders(<HomePage />);

    const continueRow = (await screen.findByRole("heading", { name: "Continue Watching" })).closest("section") as HTMLElement;
    // Movies say how much is left; episodes which one is next.
    expect(within(continueRow).getByText("34m left")).toBeInTheDocument();
    expect(within(continueRow).getByText("S1 · E1 · Pilot")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Recently Added in Movies" })).toHaveAttribute(
      "href",
      libraryUrl(1, "tab=library&sort=added&order=desc"),
    );
    expect(screen.queryByRole("heading", { name: "Top Rated" })).not.toBeInTheDocument();
  });

  it("asks an administrator to add a first library", async () => {
    serve({ libraries: [] });
    renderWithProviders(<HomePage />);

    expect(await screen.findByText("No libraries yet")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Add a library" })).toHaveAttribute("href", "/settings/libraries?add=1");
  });

  it("tells everyone else to ask the owner", async () => {
    serve({ libraries: [] });
    renderWithProviders(<HomePage />, { user: { ...testUser, is_admin: false } });

    expect(await screen.findByText("Ask the server owner to add a library.")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Add a library" })).not.toBeInTheDocument();
  });

  it("says when the libraries are still being scanned", async () => {
    serve({ libraries: [{ ...movies, scanning: true }], home: [] });
    renderWithProviders(<HomePage />);
    expect(await screen.findByText("Scanning your libraries…")).toBeInTheDocument();
  });

  it("says how far a library's first scan is, on a server that has others", async () => {
    const kids: Library = { ...movies, id: 5, name: "Kids", scanned_at: 1_700_000_000 };
    serve({
      libraries: [movies, { ...movies, id: 2, name: "TV Shows", kind: "shows" }, kids],
      scans: [{ ...firstScan, library_id: 1, library_name: "Movies", phase: "queued", processed: 0, total: 0 }, firstScan],
    });
    renderWithProviders(<HomePage />);
    expect(await screen.findByText("Scanning TV Shows: reading files")).toBeInTheDocument();
    expect(screen.getByText("2,140 / 6,509")).toBeInTheDocument();
    expect(screen.getByText("· about 25 minutes left")).toBeInTheDocument();
    // Screen readers hear the phase, not every count.
    expect(screen.getByText("Scanning TV Shows: reading files")).toHaveAttribute("aria-live", "polite");
    expect(screen.getByText("2,140 / 6,509").closest("[aria-live]")).toBeNull();
    expect(screen.getByRole("progressbar", { name: "Scanning TV Shows: reading files" })).toBeInTheDocument();
    expect(screen.queryByText(/Movies: waiting/)).not.toBeInTheDocument();
  });

  it("ticks off a new server's libraries as their first scans finish", async () => {
    const user = userEvent.setup();
    const libraries: Library[] = [movies, { ...movies, id: 2, name: "TV Shows", kind: "shows" }, { ...movies, id: 3, name: "Music", kind: "music" }];
    // Movies is done; Music waits behind TV Shows, its name not known to its scan yet.
    const done: ScanStatus = { ...firstScan, library_id: 1, library_name: "Movies", phase: "done", processed: 812, total: 812 };
    const music: ScanStatus = { ...firstScan, library_id: 3, library_name: "", phase: "queued", processed: 0, total: 0, started_at: 5 };
    mockApi({
      "GET /api/libraries": () => libraries,
      "GET /api/hubs/home": () => ({ hubs }),
      "GET /api/activity": (): Activity => ({ scans: [firstScan, music], first_scans_done: [done], sessions: [] }),
      "GET /api/import/plex": () => ({ ...importing, state: "idle", step: null }),
    });
    renderWithProviders(<HomePage />);
    expect(await screen.findByText("Scanning your libraries, 2 of 3: TV Shows")).toHaveAttribute("aria-live", "polite");
    expect(screen.getByText("· about 25 minutes left")).toBeInTheDocument();
    expect(screen.getByRole("list").closest("[aria-live]")).toBeNull();
    // Movies done, TV Shows a third of the way through reading its files: (1 + 2140 / 6509 / 2) / 3.
    expect(screen.getByRole("progressbar", { name: "All your libraries" })).toHaveAttribute("aria-valuenow", "39");
    const list = screen.getByRole("list");
    expect(within(list).getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "Movies",
      "TV Shows2,140 / 6,509Reading files · about 25 minutes left",
      "Music",
    ]);
    await user.click(screen.getByRole("button", { name: /Hide the libraries/ }));
    expect(screen.queryByRole("list")).not.toBeInTheDocument();
    expect(localStorage.getItem("pley.home.firstScansHidden")).toBe("true");
  });

  it("says nothing more once a new server's libraries are all scanned", async () => {
    const done: ScanStatus = { ...firstScan, library_id: 1, library_name: "Movies", phase: "done" };
    const requests = mockApi({
      "GET /api/libraries": () => [movies],
      "GET /api/hubs/home": () => ({ hubs }),
      "GET /api/activity": (): Activity => ({ scans: [done], sessions: [] }),
      "GET /api/import/plex": () => ({ ...importing, state: "idle", step: null }),
    });
    renderWithProviders(<HomePage />);
    expect(await screen.findByRole("heading", { name: "Continue Watching" })).toBeInTheDocument();
    await waitFor(() => expect(requests.some((r) => r.path === "/api/activity")).toBe(true));
    expect(screen.queryByText(/Scanning your libraries/)).not.toBeInTheDocument();
  });

  it("says nothing of a library scanned again", async () => {
    const requests = serve({ scans: [{ ...firstScan, first: false }] });
    renderWithProviders(<HomePage />);
    expect(await screen.findByRole("heading", { name: "Continue Watching" })).toBeInTheDocument();
    await waitFor(() => expect(requests.some((r) => r.path === "/api/activity")).toBe(true));
    expect(screen.queryByText(/TV Shows/)).not.toBeInTheDocument();
  });

  it("shows the Plex import instead while it runs, with its steps open", async () => {
    // Home doesn't come with the settings' words the import's line uses, as
    // the tests' set-up gives every page: it loads them itself.
    i18next.removeResourceBundle("en", "settings");
    const steps: ImportProgress["steps"] = [
      { step: "profiles", state: "done", count: 2 },
      { step: "fetching", state: "running", count: null },
      { step: "matching_again", state: "to_come", count: null },
    ];
    serve({ scans: [firstScan], progress: { ...importing, started_at: 1_794_100_000, steps } });
    renderWithProviders(<HomePage />);
    expect(await screen.findByText("Importing from Plex, step 2 of 3")).toBeInTheDocument();
    expect(screen.getByText("Titles fill in with their details and posters as Pley fetches them.")).toBeInTheDocument();
    expect(screen.queryByText("Scanning TV Shows: reading files")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Hide the steps/ })).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("Finding the rest of Plex's titles")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Stop the import" })).toBeInTheDocument();
  });

  it("shows how the Plex import ended until that's put away", async () => {
    i18next.removeResourceBundle("en", "settings");
    const now = Math.floor(Date.now() / 1000);
    const report: ImportReport = { ...emptyReport, finished_at: now - 60, outcome: "failed", error: "Plex couldn't be reached" };
    serve({ progress: { ...importing, state: "failed", report, first_pass_done: false } });
    renderWithProviders(<HomePage />);
    expect(await screen.findByText(/^The Plex import failed on/)).toBeInTheDocument();
    expect(screen.getByText("Plex couldn't be reached")).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByText(/^The Plex import failed on/)).not.toBeInTheDocument();
    expect(localStorage.getItem("pley.plexImport.closed")).toBe(String(report.finished_at));
  });

  it("says when a restart cut the Plex import short, and imports again from there", async () => {
    i18next.removeResourceBundle("en", "settings");
    const startedAt = Math.floor(Date.now() / 1000) - 3600;
    serve({ progress: { ...importing, state: "interrupted", step: null, started_at: startedAt, first_pass_done: false } });
    renderWithProviders(<HomePage />);
    expect(await screen.findByText(/^The Plex import started on .* was cut short when Pley restarted$/)).toBeInTheDocument();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Import again" }));
    expect(await screen.findByRole("dialog")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByText(/cut short/)).not.toBeInTheDocument();
    expect(localStorage.getItem("pley.plexImport.closed")).toBe(String(startedAt));
  });

  it("says nothing of an import that ended over a week ago", async () => {
    i18next.removeResourceBundle("en", "settings");
    const report: ImportReport = { ...emptyReport, finished_at: Math.floor(Date.now() / 1000) - 8 * 24 * 60 * 60, outcome: "done" };
    const requests = serve({ progress: { ...importing, state: "done", report } });
    renderWithProviders(<HomePage />);
    expect(await screen.findByRole("heading", { name: "Continue Watching" })).toBeInTheDocument();
    await waitFor(() => expect(requests.some((r) => r.path === "/api/import/plex")).toBe(true));
    expect(screen.queryByText(/Imported from Plex/)).not.toBeInTheDocument();
  });

  it("says nothing of scans or imports to others", async () => {
    const requests = serve({ scans: [firstScan], progress: importing });
    renderWithProviders(<HomePage />, { user: { ...testUser, is_admin: false } });
    expect(await screen.findByRole("heading", { name: "Continue Watching" })).toBeInTheDocument();
    expect(screen.queryByText(/TV Shows: reading|Importing from Plex/)).not.toBeInTheDocument();
    expect(requests.some((r) => r.path === "/api/import/plex")).toBe(false);
  });

  it("has My List when the server sends it", async () => {
    serve({ home: [...hubs, { key: "my_list", title: "My List", library_id: null, items: [makeItem({ id: 9, title: "Ran" })] }] });
    renderWithProviders(<HomePage />);
    const row = (await screen.findByRole("heading", { name: "My List" })).closest("section") as HTMLElement;
    expect(within(row).getAllByRole("link", { name: "Ran" })).not.toHaveLength(0);
  });

  it("takes a card off Continue Watching at once", async () => {
    const user = userEvent.setup();
    let answer: (res: Response | undefined) => void = () => undefined;
    const requests = mockApi({
      "GET /api/libraries": () => [movies],
      "GET /api/hubs/home": () => ({ hubs }),
      "GET /api/watchlist": () => ({ items: [] }),
      "DELETE /api/continue-watching/6": () => new Promise<Response | undefined>((resolve) => (answer = resolve)),
    });
    renderWithProviders(
      <ListMenuProvider>
        <HomePage />
      </ListMenuProvider>,
    );
    const row = (await screen.findByRole("heading", { name: "Continue Watching" })).closest("section") as HTMLElement;
    fireEvent.contextMenu(within(row).getAllByRole("link", { name: "Alien" })[0] as HTMLElement);
    await user.click(screen.getByRole("menuitem", { name: "Remove from Continue Watching" }));

    // Gone before the server answers.
    expect(within(row).queryAllByRole("link", { name: "Alien" })).toHaveLength(0);
    expect(within(row).getByText("S1 · E1 · Pilot")).toBeInTheDocument();
    expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/continue-watching/6")).toBe(true);
    answer(undefined);
  });

  it("puts the card back when the server says no", async () => {
    const user = userEvent.setup();
    mockApi({
      "GET /api/libraries": () => [movies],
      "GET /api/hubs/home": () => ({ hubs }),
      "GET /api/watchlist": () => ({ items: [] }),
      "DELETE /api/continue-watching/6": () => apiError(500, "internal", "database is locked"),
    });
    renderWithProviders(
      <ListMenuProvider>
        <HomePage />
      </ListMenuProvider>,
    );
    const row = (await screen.findByRole("heading", { name: "Continue Watching" })).closest("section") as HTMLElement;
    fireEvent.contextMenu(within(row).getAllByRole("link", { name: "Alien" })[0] as HTMLElement);
    await user.click(screen.getByRole("menuitem", { name: "Remove from Continue Watching" }));

    expect(await screen.findByText("Database is locked")).toBeInTheDocument();
    expect(await within(row).findAllByRole("link", { name: "Alien" })).not.toHaveLength(0);
  });

  it("only offers that in Continue Watching", async () => {
    mockApi({
      "GET /api/libraries": () => [movies],
      "GET /api/hubs/home": () => ({ hubs }),
      "GET /api/watchlist": () => ({ items: [] }),
    });
    renderWithProviders(
      <ListMenuProvider>
        <HomePage />
      </ListMenuProvider>,
    );
    fireEvent.contextMenu((await screen.findAllByRole("link", { name: "Heat" }))[0] as HTMLElement);
    expect(await screen.findByRole("menuitem", { name: "Add to My List" })).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Remove from Continue Watching" })).not.toBeInTheDocument();
  });

  it("puts a card on My List, or takes it off, from its menu", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/libraries": () => [movies],
      "GET /api/hubs/home": () => ({ hubs }),
      "GET /api/watchlist": () => ({ items: [makeItem({ id: 8, title: "Heat" })] }),
      "DELETE /api/watchlist/8": () => undefined,
      "PUT /api/watchlist/6": () => undefined,
    });
    renderWithProviders(
      <ListMenuProvider>
        <HomePage />
      </ListMenuProvider>,
    );
    fireEvent.contextMenu((await screen.findAllByRole("link", { name: "Heat" }))[0] as HTMLElement);
    await user.click(await screen.findByRole("menuitem", { name: "Remove from My List" }));
    await waitFor(() => expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/watchlist/8")).toBe(true));

    fireEvent.contextMenu(screen.getAllByRole("link", { name: "Alien" })[0] as HTMLElement);
    await user.click(await screen.findByRole("menuitem", { name: "Add to My List" }));
    await waitFor(() => expect(requests.some((r) => r.method === "PUT" && r.path === "/api/watchlist/6")).toBe(true));
  });

  it("retries after an error", async () => {
    const user = userEvent.setup();
    let fail = true;
    mockApi({
      "GET /api/libraries": () => [movies],
      "GET /api/hubs/home": () => (fail ? apiError(500, "internal", "database is locked") : { hubs }),
    });
    renderWithProviders(<HomePage />);

    expect(await screen.findByText("Database is locked")).toBeInTheDocument();
    fail = false;
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("heading", { name: "Continue Watching" })).toBeInTheDocument();
  });

  describe("banner", () => {
    function serveFeatured() {
      const [heat, lost] = featuredHubs.map((h) => h.items[0]!);
      return mockApi({
        "GET /api/libraries": () => [movies, shows],
        "GET /api/hubs/home": () => ({ hubs: featuredHubs }),
        "GET /api/items/8": () =>
          detail(heat!, { summary: "A thief's last job.", genres: ["Crime", "Thriller"], content_rating: "R", in_watchlist: false }),
        "GET /api/items/9": () => detail(lost!, { child_count: 6, summary: "Survivors of a crash." }),
        "PUT /api/watchlist/8": () => undefined,
      });
    }

    it("comes after what's filling in, while that shows", async () => {
      const kids: Library = { ...movies, id: 5, name: "Kids", scanned_at: 1_700_000_000 };
      mockApi({
        "GET /api/libraries": () => [movies, shows, kids],
        "GET /api/hubs/home": () => ({ hubs: featuredHubs }),
        "GET /api/activity": (): Activity => ({ scans: [firstScan], sessions: [] }),
        "GET /api/import/plex": () => ({ ...importing, state: "idle", step: null }),
      });
      renderWithProviders(<HomePage />);
      const banner = await screen.findByRole("region", { name: "Featured" });
      const scan = await screen.findByText("Scanning TV Shows: reading files");
      expect(scan.compareDocumentPosition(banner) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      expect(banner.previousElementSibling).toHaveAttribute("data-home-progress");
      expect(banner.previousElementSibling).not.toBeEmptyDOMElement();
    });

    it("features the newest title, with what its page says", async () => {
      serveFeatured();
      renderWithProviders(<HomePage />);

      const banner = await screen.findByRole("region", { name: "Featured" });
      expect(within(banner).getByText("New in Movies")).toBeInTheDocument();
      expect(within(banner).getByRole("heading", { name: "Heat" })).toBeInTheDocument();
      expect(await within(banner).findByText("A thief's last job.")).toBeInTheDocument();
      expect(within(banner).getByText("2h 50m")).toBeInTheDocument();
      expect(within(banner).getByText("Crime")).toBeInTheDocument();
      expect(within(banner).getByRole("button", { name: "Play" })).toBeInTheDocument();
      expect(within(banner).getByRole("link", { name: "More info" })).toHaveAttribute("href", itemUrl(8));
      // The rows are still there under it.
      expect(screen.getByRole("link", { name: "Recently Added in Movies" })).toBeInTheDocument();
    });

    it("turns to another title when asked", async () => {
      const user = userEvent.setup();
      serveFeatured();
      renderWithProviders(<HomePage />);

      const banner = await screen.findByRole("region", { name: "Featured" });
      expect(within(banner).getByRole("button", { name: "Show Heat" })).toHaveAttribute("aria-current", "true");
      await user.click(within(banner).getByRole("button", { name: "Show Lost" }));

      expect(within(banner).getByRole("heading", { name: "Lost" })).toBeInTheDocument();
      expect(within(banner).getByText("New in TV Shows")).toBeInTheDocument();
      expect(await within(banner).findByText("6 seasons")).toBeInTheDocument();
      expect(within(banner).queryByRole("heading", { name: "Heat" })).not.toBeInTheDocument();
    });

    it("puts the title on My List", async () => {
      const user = userEvent.setup();
      const requests = serveFeatured();
      renderWithProviders(<HomePage />);

      const banner = await screen.findByRole("region", { name: "Featured" });
      await user.click(await within(banner).findByRole("button", { name: "Add to My List" }));
      await waitFor(() => expect(requests.some((r) => r.method === "PUT" && r.path === "/api/watchlist/8")).toBe(true));
    });

    it("features shows when there are no films", async () => {
      const lost = featuredHubs[1]!.items[0]!;
      mockApi({
        "GET /api/libraries": () => [shows],
        "GET /api/hubs/home": () => ({ hubs: [featuredHubs[1]] }),
        "GET /api/items/9": () => detail(lost, { child_count: 6 }),
      });
      renderWithProviders(<HomePage />);

      const banner = await screen.findByRole("region", { name: "Featured" });
      expect(within(banner).getByRole("heading", { name: "Lost" })).toBeInTheDocument();
      expect(within(banner).getByText("New in TV Shows")).toBeInTheDocument();
      // One title: nothing to turn to.
      expect(within(banner).queryByRole("button", { name: /^Show / })).not.toBeInTheDocument();
    });

    it("still features something when everything has been watched", async () => {
      const seen = { position_ms: 0, watched: true, play_count: 1 };
      const watched = featuredHubs.map((h) => ({ ...h, items: h.items.map((i) => ({ ...i, user_state: seen })) }));
      mockApi({
        "GET /api/libraries": () => [movies, shows],
        "GET /api/hubs/home": () => ({ hubs: watched }),
      });
      renderWithProviders(<HomePage />);

      const banner = await screen.findByRole("region", { name: "Featured" });
      expect(within(banner).getByRole("heading", { name: "Heat" })).toBeInTheDocument();
      expect(within(banner).getByRole("button", { name: "Show Lost" })).toBeInTheDocument();
    });

    it("isn't there without a backdrop to show", async () => {
      serve();
      renderWithProviders(<HomePage />);
      await screen.findByRole("heading", { name: "Continue Watching" });
      expect(screen.queryByRole("region", { name: "Featured" })).not.toBeInTheDocument();
    });
  });
});

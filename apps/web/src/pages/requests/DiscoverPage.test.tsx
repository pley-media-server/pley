import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { DiscoverPage as Page } from "@/api/requests";
import { apiError, LocationDisplay, mockApi, renderWithProviders, testUser } from "@/test/render";
import { makeDetail, makeRequest, makeRequestList, makeSummary, makeTitle } from "@/test/requests";
import { DiscoverPage } from "./DiscoverPage";

const ann = { ...testUser, id: 2, username: "ann", name: "Ann", is_admin: false };

function page(results = [makeTitle()], n = 1, total = 1): Page {
  return { page: n, total_pages: total, results };
}

/** Every list answers with one title of its own. */
function lists() {
  return {
    "GET /api/discover/trending": () => page([makeTitle({ tmdb_id: 1, title: "Trending One", overview: "First of the week." })]),
    "GET /api/discover/movies": () => page([makeTitle({ tmdb_id: 2, title: "Popular Movie", status: "library", item_id: 7 })]),
    "GET /api/discover/shows": () => page([makeTitle({ tmdb_id: 3, kind: "show", title: "Popular Show" })]),
    "GET /api/discover/upcoming": () => page([]),
    "GET /api/discover/anime": () => page([makeTitle({ tmdb_id: 5, kind: "show", title: "Some Anime", status: "requested" })]),
    "GET /api/discover/movie/1": () => makeDetail({ tmdb_id: 1, title: "Trending One" }),
  };
}

describe("DiscoverPage", () => {
  it("shows the first trending title large, with the one thing to do about it", async () => {
    mockApi({ "GET /api/requests/summary": () => makeSummary(), ...lists() });
    renderWithProviders(<DiscoverPage />, { route: "/discover", user: ann });

    const hero = await screen.findByRole("region", { name: "Trending now" });
    expect(within(hero).getByRole("link", { name: "Trending One" })).toHaveAttribute("href", "/discover/movie/1");
    expect(within(hero).getByText("First of the week.")).toBeInTheDocument();
    expect(await within(hero).findByRole("button", { name: "Request" })).toBeEnabled();
  });

  it("shows TMDB's lists as rows with where each title stands, each with a way to all of it", async () => {
    mockApi({ "GET /api/requests/summary": () => makeSummary(), ...lists() });
    renderWithProviders(<DiscoverPage />, { route: "/discover", user: ann });

    expect(await screen.findByRole("link", { name: "Popular Show, 1999, Show" })).toHaveAttribute("href", "/discover/show/3");
    // Where a title stands is in its name, and on its poster.
    expect(await screen.findByRole("link", { name: "Popular Movie, 1999, Movie, In library" })).toBeInTheDocument();
    const anime = (await screen.findByRole("heading", { name: "Popular anime" })).closest("section") as HTMLElement;
    expect(within(anime).getByText("Requested")).toBeInTheDocument();
    expect(within(anime).getByRole("link", { name: "See all" })).toHaveAttribute("href", "/discover?list=anime");
    // A list without titles leaves no row.
    expect(screen.queryByRole("heading", { name: "Coming to cinemas" })).not.toBeInTheDocument();
    expect(
      screen.getByText("Details and pictures from TMDB. This product uses the TMDB API but is not endorsed or certified by TMDB."),
    ).toBeInTheDocument();
  });

  it("ends with the account's own requests, for those who have any", async () => {
    const requests = mockApi({
      "GET /api/requests/summary": () => makeSummary({ own: 1, unseen: 1 }),
      ...lists(),
      "GET /api/requests": () => makeRequestList([makeRequest({ id: 4, title: "Heat", tmdb_id: 949, mine: true })]),
    });
    renderWithProviders(<DiscoverPage />, { route: "/discover", user: ann });

    const row = (await screen.findByRole("heading", { name: "Your requests" })).closest("section") as HTMLElement;
    expect(within(row).getByRole("link", { name: "Heat, Waiting for approval" })).toHaveAttribute("href", "/discover/movie/949");
    expect(requests.find((r) => r.path.startsWith("/api/requests?"))?.path).toBe("/api/requests?mine=1&limit=20");
    // And the way to all of them, with what's new there.
    expect(screen.getByRole("link", { name: "Your requests 1 waiting" })).toHaveAttribute("href", "/requests");
  });

  it("leaves the requests row out for those who never asked", async () => {
    const requests = mockApi({ "GET /api/requests/summary": () => makeSummary(), ...lists() });
    renderWithProviders(<DiscoverPage />, { route: "/discover", user: ann });
    expect(await screen.findByRole("heading", { name: "Popular anime" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Your requests" })).not.toBeInTheDocument();
    expect(requests.some((r) => r.path.startsWith("/api/requests?"))).toBe(false);
  });

  it("shows one list whole, a page at a time", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/requests/summary": () => makeSummary(),
      "GET /api/discover/movies": (req) =>
        req.path.includes("page=2")
          ? page([makeTitle({ tmdb_id: 12, title: "Second Page" })], 2, 2)
          : page(
              Array.from({ length: 24 }, (_, i) => makeTitle({ tmdb_id: 100 + i, title: `Movie ${i}` })),
              1,
              2,
            ),
    });
    renderWithProviders(<DiscoverPage />, { route: "/discover?list=movies" });

    expect(await screen.findByRole("heading", { name: "Popular movies" })).toBeInTheDocument();
    expect(await screen.findByRole("link", { name: "Movie 0, 1999, Movie" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Show more" }));
    expect(await screen.findByRole("link", { name: "Second Page, 1999, Movie" })).toBeInTheDocument();
    expect(requests.map((r) => r.path)).toContain("/api/discover/movies?page=2");
    expect(screen.queryByRole("button", { name: "Show more" })).not.toBeInTheDocument();
  });

  it("says when an age limit leaves a list empty", async () => {
    mockApi({ "GET /api/requests/summary": () => makeSummary(), "GET /api/discover/anime": () => page([]) });
    renderWithProviders(<DiscoverPage />, { route: "/discover?list=anime", user: { ...ann, max_age: 12 } });
    expect(await screen.findByRole("heading", { name: "Nothing here right now" })).toBeInTheDocument();
    expect(screen.getByText("What TMDB lists here is above this profile's age limit.")).toBeInTheDocument();
  });

  it("goes on past pages that came short (titles above an age limit left out), as they aren't the end", async () => {
    mockApi({
      "GET /api/requests/summary": () => makeSummary(),
      "GET /api/discover/movies": (req) =>
        req.path.includes("page=3")
          ? page([makeTitle({ tmdb_id: 13, title: "Third Page" })], 3, 3)
          : req.path.includes("page=2")
            ? page([], 2, 3)
            : page([], 1, 3),
    });
    renderWithProviders(<DiscoverPage />, { route: "/discover?list=movies" });
    expect(await screen.findByRole("link", { name: "Third Page, 1999, Movie" })).toBeInTheDocument();
  });

  it("has no search of its own: TMDB is searched from Search", async () => {
    const requests = mockApi({ "GET /api/requests/summary": () => makeSummary(), ...lists() });
    renderWithProviders(<DiscoverPage />, { route: "/discover", user: ann });
    expect(await screen.findByRole("region", { name: "Trending now" })).toBeInTheDocument();
    expect(screen.queryByRole("search")).not.toBeInTheDocument();
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
    expect(requests.some((r) => r.path.startsWith("/api/discover/search"))).toBe(false);
  });

  it("shows all TMDB finds for Search's words, with the way back to Search", async () => {
    const requests = mockApi({
      "GET /api/requests/summary": () => makeSummary(),
      "GET /api/discover/search": (req) => (req.path.includes("q=dune") ? page([makeTitle({ tmdb_id: 438631, title: "Dune", year: 2021 })]) : page([])),
    });
    renderWithProviders(<DiscoverPage />, { route: "/discover?q=dune" });

    expect(await screen.findByRole("heading", { name: "Results for “dune”" })).toBeInTheDocument();
    expect(await screen.findByRole("link", { name: "Dune, 2021, Movie" })).toHaveAttribute("href", "/discover/movie/438631");
    expect(screen.getByRole("link", { name: "Back to search" })).toHaveAttribute("href", "/search?q=dune");
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
    // Every title TMDB has, those in a library too (Search's row leaves them out).
    expect(requests.filter((r) => r.path.startsWith("/api/discover/search")).map((r) => r.path)).toEqual(["/api/discover/search?q=dune"]);
  });

  it("searches for as much of a long query as the server takes, whole characters", async () => {
    const requests = mockApi({ "GET /api/requests/summary": () => makeSummary(), "GET /api/discover/search": () => page([]) });
    // 199 letters, then a character of two UTF-16 units: the 200th.
    const kept = `${"x".repeat(199)}🎬`;
    renderWithProviders(<DiscoverPage />, { route: `/discover?q=${encodeURIComponent(`${kept}${"y".repeat(50)}`)}` });
    expect(await screen.findByText(`Nothing on TMDB for “${kept}”`)).toBeInTheDocument();
    const asked = new URL(requests.find((r) => r.path.startsWith("/api/discover/search"))?.path ?? "", "http://pley.test");
    expect(asked.searchParams.get("q")).toBe(kept);
  });

  it("says when TMDB finds nothing", async () => {
    mockApi({ "GET /api/requests/summary": () => makeSummary(), "GET /api/discover/search": () => page([]) });
    renderWithProviders(<DiscoverPage />, { route: "/discover?q=zzz" });
    expect(await screen.findByText("Nothing on TMDB for “zzz”")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Back to search" })).toHaveAttribute("href", "/search?q=zzz");
  });

  it("says when TMDB isn't answering, with Retry, and doesn't ask again by itself", async () => {
    const requests = mockApi({
      "GET /api/requests/summary": () => makeSummary(),
      "GET /api/discover/search": () => apiError(503, "tmdb_down", "TMDB didn't answer"),
    });
    renderWithProviders(<DiscoverPage />, { route: "/discover?q=heat" });
    expect(await screen.findByRole("heading", { name: "TMDB isn't answering" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(requests.filter((r) => r.path.startsWith("/api/discover/search"))).toHaveLength(1);
  });

  it("says once that TMDB isn't answering, in place of every row, keeping the account's requests", async () => {
    mockApi({
      "GET /api/requests/summary": () => makeSummary({ own: 1 }),
      ...lists(),
      "GET /api/discover/trending": () => apiError(503, "tmdb_down", "TMDB didn't answer"),
      "GET /api/requests": () => makeRequestList([makeRequest({ id: 4, title: "Heat", tmdb_id: 949, mine: true })]),
    });
    renderWithProviders(<DiscoverPage />, { route: "/discover", user: ann });
    expect(await screen.findByRole("heading", { name: "TMDB isn't answering" })).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Retry" })).toHaveLength(1);
    expect(await screen.findByRole("heading", { name: "Your requests" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { name: "Popular movies" })).not.toBeInTheDocument();
  });

  it("sends managers to the key TMDB refused, and tells others an administrator must check it", async () => {
    mockApi({
      "GET /api/requests/summary": () => makeSummary({ access: "auto", manage: true }),
      "GET /api/discover/search": () => apiError(503, "tmdb_key", "TMDB refused the key"),
    });
    const { unmount } = renderWithProviders(<DiscoverPage />, { route: "/discover?q=heat" });
    expect(await screen.findByRole("heading", { name: "TMDB refused this server's key" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Check the key" })).toHaveAttribute("href", "/settings/server#tmdb-key");
    expect(screen.queryByRole("button", { name: "Retry" })).not.toBeInTheDocument();
    unmount();

    mockApi({
      "GET /api/requests/summary": () => makeSummary(),
      "GET /api/discover/search": () => apiError(503, "tmdb_key", "TMDB refused the key"),
    });
    renderWithProviders(<DiscoverPage />, { route: "/discover?q=heat", user: ann });
    expect(await screen.findByText("An administrator needs to check it.")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Check the key" })).not.toBeInTheDocument();
  });

  it("sends administrators to set requests up while they're off", async () => {
    const requests = mockApi({ "GET /api/requests/summary": () => makeSummary({ enabled: false }) });
    renderWithProviders(<DiscoverPage />, { route: "/discover" });
    expect(await screen.findByRole("heading", { name: "Requests are off" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Set up requests" })).toHaveAttribute("href", "/settings/requests");
    expect(requests.some((r) => r.path.startsWith("/api/discover"))).toBe(false);
  });

  it("says requests are off to others, and on servers without them", async () => {
    mockApi({});
    renderWithProviders(<DiscoverPage />, { route: "/discover", user: ann });
    expect(await screen.findByRole("heading", { name: "Requests are off" })).toBeInTheDocument();
    expect(screen.getByText("This server doesn't take requests for movies and shows.")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Set up requests" })).not.toBeInTheDocument();
  });

  it("says when this account may not ask, keeping what it asked before in reach", async () => {
    mockApi({ "GET /api/requests/summary": () => makeSummary({ access: "off", own: 2 }) });
    renderWithProviders(<DiscoverPage />, { route: "/discover", user: ann });
    expect(await screen.findByRole("heading", { name: "Requests aren't open to this account" })).toBeInTheDocument();
    expect(screen.getByText("An administrator can let it ask for movies and shows.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "See your requests" })).toHaveAttribute("href", "/requests");
  });

  it("asks a sign-in without requests to sign in again, and nothing more", async () => {
    const requests = mockApi({
      "GET /api/requests/summary": () => makeSummary({ sign_in_again: "requests" }),
      "POST /api/auth/logout": () => undefined,
    });
    renderWithProviders(
      <>
        <DiscoverPage />
        <LocationDisplay />
      </>,
      { route: "/discover?list=anime", user: ann },
    );
    expect(await screen.findByRole("heading", { name: "Sign in again to ask for movies and shows" })).toBeInTheDocument();
    expect(requests.some((r) => r.path.startsWith("/api/discover"))).toBe(false);

    // Signing out first, so the sign-in comes back here.
    await userEvent.click(screen.getByRole("button", { name: "Sign in" }));
    await waitFor(() => expect(screen.getByTestId("location")).toHaveTextContent("/login?next=%2Fdiscover%3Flist%3Danime"));
    expect(requests.some((r) => r.method === "POST" && r.path === "/api/auth/logout")).toBe(true);
  });
});

import { Route, Routes } from "react-router";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import type { ArrChoices, MediaRequest } from "@/api/requests";
import type { User } from "@/api/types";
import { itemUrl } from "@/lib/urls";
import { apiError, LocationDisplay, mockApi, renderWithProviders, testUser } from "@/test/render";
import { makeDetail, makeRequest, makeSeason, makeShowDetail, makeSummary } from "@/test/requests";
import { TitlePage } from "./TitlePage";

const ann: User = { ...testUser, id: 2, username: "ann", name: "Ann", is_admin: false };
const now = () => Math.floor(Date.now() / 1000);
const DAY = 86_400;

function open(route: string, user: User = ann) {
  return renderWithProviders(
    <>
      <Routes>
        <Route path="/discover/:kind/:tmdbId" element={<TitlePage />} />
      </Routes>
      <LocationDisplay />
    </>,
    { route, user },
  );
}

/** A new request, as the server answers one (201; 200 means following one there already). */
function created(request: MediaRequest): Response {
  return new Response(JSON.stringify(request), { status: 201, headers: { "content-type": "application/json" } });
}

/** A show partly in a library: Season 1 whole, Season 2 in part, Season 3 asked for, Season 4 missing, Season 5 not aired. */
const dark = makeShowDetail({
  status: "partial",
  item_id: 40,
  seasons: [
    makeSeason(0, { episodes: 3, aired: 3 }),
    makeSeason(1, { in_library: 10, state: "library" }),
    makeSeason(2, { in_library: 3, state: "partial" }),
    makeSeason(3, { requested: true, state: "requested" }),
    makeSeason(4),
    makeSeason(5, { aired: 0, air_date: null, state: "unaired" }),
  ],
});

function choices(overrides: Partial<ArrChoices> = {}): ArrChoices {
  return {
    quality_profiles: [
      { id: 1, name: "HD-1080p" },
      { id: 4, name: "Ultra-HD" },
    ],
    root_folders: [{ path: "/movies", free_bytes: null }],
    quality_profile: 1,
    root_folder: "/movies",
    anime_quality_profile: null,
    anime_root_folder: null,
    error: null,
    ...overrides,
  };
}

describe("TitlePage", () => {
  it("shows what a movie is, and asks for it at once, saying an administrator decides", async () => {
    const user = userEvent.setup();
    let asked = false;
    const requests = mockApi({
      "GET /api/discover/movie/278": () =>
        asked
          ? makeDetail({ status: "requested", requests: [makeRequest({ id: 5, mine: true, actions: ["withdraw"] })] })
          : makeDetail({ cast: [{ name: "Tim Robbins", character: "Andy Dufresne", photo_url: null }] }),
      "GET /api/requests/summary": () => makeSummary(),
      "POST /api/requests": () => {
        asked = true;
        return created(makeRequest({ id: 5, mine: true }));
      },
    });
    open("/discover/movie/278");

    expect(await screen.findByRole("heading", { name: "The Shawshank Redemption", level: 1 })).toBeInTheDocument();
    expect(screen.getByText("2h 22m")).toBeInTheDocument();
    expect(screen.getByText("Andy Dufresne")).toBeInTheDocument();
    expect(screen.getByText("Details and pictures from TMDB.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Request" }));

    expect(await screen.findByText("Asked for “The Shawshank Redemption”: an administrator decides.")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "POST")?.body).toEqual({ kind: "movie", tmdb_id: 278 });
    // Asked, it says so, with where the request is.
    expect(await screen.findByText("Requested")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Request" })).not.toBeInTheDocument();
    expect(screen.getAllByText("Waiting for approval").length).toBeGreaterThan(0);
  });

  it("says when the account may ask again, past its limit", async () => {
    const user = userEvent.setup();
    mockApi({
      "GET /api/discover/movie/278": () => makeDetail(),
      "GET /api/requests/summary": () => makeSummary(),
      "POST /api/requests": () =>
        new Response(JSON.stringify({ code: "too_many_requests", message: "past the limit", retry_at: now() + 3 * DAY }), {
          status: 429,
          headers: { "content-type": "application/json" },
        }),
    });
    open("/discover/movie/278");
    await user.click(await screen.findByRole("button", { name: "Request" }));
    expect(await screen.findByText("You can ask for more in 3 days.")).toBeInTheDocument();
  });

  it("holds Request once the allowance is spent, saying when more may be asked for", async () => {
    mockApi({
      "GET /api/discover/movie/278": () => makeDetail({ movies_left: 0, movies_next_at: now() + 3 * DAY }),
      "GET /api/requests/summary": () => makeSummary({ movies_left: 0, movies_limit: 3 }),
    });
    open("/discover/movie/278");
    expect(await screen.findByRole("button", { name: "Request" })).toBeDisabled();
    expect(screen.getByText("You've asked for all the movies you can for now: you can ask for more in 3 days.")).toBeInTheDocument();
  });

  it("asks for the seasons a library lacks, those missing picked at first", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/discover/show/70523": () => dark,
      "GET /api/requests/summary": () => makeSummary({ access: "auto" }),
      "POST /api/requests": (req) =>
        created(makeRequest({ kind: "show", tmdb_id: 70523, title: "Dark", seasons: (req.body as { seasons: number[] }).seasons, status: "approved", mine: true })),
    });
    open("/discover/show/70523");

    expect(await screen.findByRole("link", { name: "Open" })).toHaveAttribute("href", itemUrl(40));
    await user.click(screen.getByRole("button", { name: "Request more seasons" }));
    const dialog = screen.getByRole("dialog", { name: "Which seasons?" });
    expect(within(dialog).getByText("Approved as you ask, and sent on to download.")).toBeInTheDocument();
    expect(screen.getByTestId("location")).toHaveTextContent("/discover/show/70523?pick=1");

    // Whole in a library, or asked for already: not to be picked.
    expect(within(dialog).getByRole("checkbox", { name: /Season 1/ })).toBeDisabled();
    expect(within(dialog).getByRole("checkbox", { name: /Season 3/ })).toBeDisabled();
    expect(within(dialog).getByRole("checkbox", { name: /Season 2/ })).toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: /Season 4/ })).toBeChecked();
    // Not aired yet, and specials: offered, not picked; specials come last.
    expect(within(dialog).getByRole("checkbox", { name: /Season 5/ })).not.toBeChecked();
    expect(within(dialog).getByRole("checkbox", { name: /Specials/ })).not.toBeChecked();
    expect(within(dialog).getAllByRole("checkbox").at(-1)).toHaveAccessibleName(/Specials/);

    await user.click(within(dialog).getByRole("checkbox", { name: /Specials/ }));
    await user.click(within(dialog).getByRole("button", { name: "Request 3 seasons" }));

    expect(await screen.findByText("“Dark” is approved and on its way.")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "POST")?.body).toEqual({ kind: "show", tmdb_id: 70523, seasons: [0, 2, 4] });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(screen.getByTestId("location")).toHaveTextContent(/^\/discover\/show\/70523$/);
  });

  it("asks for at least a season", async () => {
    const user = userEvent.setup();
    mockApi({ "GET /api/discover/show/70523": () => dark, "GET /api/requests/summary": () => makeSummary() });
    open("/discover/show/70523?pick=1");
    const dialog = await screen.findByRole("dialog", { name: "Which seasons?" });
    await user.click(within(dialog).getByRole("checkbox", { name: /Season 2/ }));
    await user.click(within(dialog).getByRole("checkbox", { name: /Season 4/ }));
    expect(within(dialog).getByText("Pick a season.")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Request" })).toBeDisabled();
  });

  it("won't ask for more seasons than the account has left", async () => {
    const user = userEvent.setup();
    mockApi({
      "GET /api/discover/show/70523": () => ({ ...dark, seasons_left: 1 }),
      "GET /api/requests/summary": () => makeSummary({ seasons_left: 1, seasons_limit: 5 }),
    });
    // From a library's page: the picker opens with the page.
    open("/discover/show/70523?pick=1");
    const dialog = await screen.findByRole("dialog", { name: "Which seasons?" });
    // As many as are left are picked to begin with.
    expect(within(dialog).getByRole("button", { name: "Request 1 season" })).toBeEnabled();
    await user.click(within(dialog).getByRole("checkbox", { name: /Season 4/ }));
    expect(within(dialog).getByRole("button", { name: "Request 2 seasons" })).toBeDisabled();
    expect(within(dialog).getByText("You can ask for 1 more season. Limits: 5 seasons per 7 days.")).toHaveClass("text-danger");
  });

  it("shows each season's state", async () => {
    mockApi({ "GET /api/discover/show/70523": () => dark, "GET /api/requests/summary": () => makeSummary() });
    open("/discover/show/70523");
    const seasons = (await screen.findByRole("heading", { name: "Seasons" })).closest("section") as HTMLElement;
    const row = (name: string) => within(seasons).getByText(name).closest("li") as HTMLElement;
    expect(within(row("Season 1")).getByText("In library")).toBeInTheDocument();
    expect(within(row("Season 2")).getByText("Partly in library")).toBeInTheDocument();
    expect(within(row("Season 3")).getByText("Requested")).toBeInTheDocument();
    expect(within(row("Season 5")).getByText("Not aired yet")).toBeInTheDocument();
    // Specials last.
    expect(within(seasons).getAllByRole("listitem").at(-1)).toHaveTextContent(/^Specials/);
  });

  it("says how the download goes for a season a request partly ready still lacks", async () => {
    const request = makeRequest({
      id: 7,
      kind: "show",
      tmdb_id: 70523,
      title: "Dark",
      seasons: [1, 2],
      available_seasons: [1],
      status: "approved",
      stage: "partial",
      mine: true,
      download: { progress: 0.3, state: "downloading", eta: now() + 5 * 3600, episodes: 10 },
    });
    mockApi({
      "GET /api/discover/show/70523": () =>
        makeShowDetail({
          status: "partial",
          item_id: 40,
          seasons: [makeSeason(1, { in_library: 10, state: "library", request_id: 7 }), makeSeason(2, { requested: true, state: "requested", request_id: 7 })],
          requests: [request],
        }),
      "GET /api/requests/summary": () => makeSummary(),
    });
    open("/discover/show/70523");
    const seasons = (await screen.findByRole("heading", { name: "Seasons" })).closest("section") as HTMLElement;
    const row = within(seasons).getByText("Season 2").closest("li") as HTMLElement;
    expect(within(row).getByText("Downloading 30%")).toBeInTheDocument();
    expect(within(row).getByRole("progressbar")).toHaveAttribute("aria-valuenow", "30");
    expect(within(seasons).queryByText("Season 1 ready")).not.toBeInTheDocument();
  });

  it("lets the account withdraw what it asked for", async () => {
    const user = userEvent.setup();
    let mine: MediaRequest[] = [makeRequest({ id: 5, mine: true, actions: ["withdraw"] })];
    const requests = mockApi({
      "GET /api/discover/movie/278": () => makeDetail({ status: mine.length ? "requested" : "none", requests: mine }),
      "GET /api/requests/summary": () => makeSummary(),
      "DELETE /api/requests/5": () => {
        mine = [];
        return undefined;
      },
    });
    open("/discover/movie/278");

    const list = await screen.findByRole("list");
    expect(within(list).getByText(/You asked/)).toBeInTheDocument();
    expect(within(list).queryByRole("button", { name: "Approve" })).not.toBeInTheDocument();
    await user.click(within(list).getByRole("button", { name: "Withdraw" }));
    const confirm = screen.getByRole("dialog", { name: "Withdraw your request for “The Shawshank Redemption”?" });
    await user.click(within(confirm).getByRole("button", { name: "Withdraw" }));
    expect(await screen.findByRole("button", { name: "Request" })).toBeInTheDocument();
    expect(requests.find((r) => r.method === "DELETE")?.path).toBe("/api/requests/5");
  });

  it("offers Tell me too where someone else asked, and says it follows", async () => {
    const user = userEvent.setup();
    const theirs = makeRequest({ id: 8 });
    const requests = mockApi({
      "GET /api/discover/movie/278": () => makeDetail({ status: "requested" }),
      "GET /api/requests/summary": () => makeSummary(),
      "POST /api/requests": () => ({ ...theirs, watching: true }),
    });
    open("/discover/movie/278");
    expect(await screen.findByText("Someone asked for it already.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Tell me too" }));
    expect(await screen.findByText("You'll hear when “The Shawshank Redemption” is ready.")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "POST")?.body).toEqual({ kind: "movie", tmdb_id: 278 });
  });

  it("stops following", async () => {
    const user = userEvent.setup();
    const followed = makeRequest({ id: 6, watching: true, actions: ["unwatch"], status: "approved", stage: "searching" });
    const requests = mockApi({
      "GET /api/discover/movie/278": () => makeDetail({ status: "requested", watching: true, requests: [followed] }),
      "GET /api/requests/summary": () => makeSummary(),
      "DELETE /api/requests/6/watch": () => undefined,
    });
    open("/discover/movie/278");
    expect(await screen.findByText("Following")).toBeInTheDocument();
    // Its stage, and Stop following, once: with the request, in the list.
    const list = await screen.findByRole("list");
    expect(within(list).getByText("Searching")).toBeInTheDocument();
    expect(screen.getAllByText("Searching")).toHaveLength(1);
    await user.click(within(list).getByRole("button", { name: "Stop following" }));
    await waitFor(() => expect(requests.some((r) => r.method === "DELETE" && r.path === "/api/requests/6/watch")).toBe(true));
  });

  it("says when a declined title may be asked for again, with no button", async () => {
    mockApi({
      "GET /api/discover/movie/278": () =>
        makeDetail({ status: "declined", declined: { reason: "We have it on disc", at: now() - 18 * DAY, again_at: now() + 12 * DAY } }),
      "GET /api/requests/summary": () => makeSummary(),
    });
    open("/discover/movie/278");
    expect(await screen.findByText("Declined: “We have it on disc”. You can ask again in 12 days.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Request" })).not.toBeInTheDocument();
  });

  it("says why a title above the account's age limit can't be asked for", async () => {
    mockApi({
      "GET /api/discover/movie/278": () => makeDetail({ status: "restricted", restriction: "Rated R: above this account's age limit" }),
      "GET /api/requests/summary": () => makeSummary(),
    });
    open("/discover/movie/278");
    expect(await screen.findByText("Rated R: above this account's age limit")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Request" })).not.toBeInTheDocument();
  });

  it("says requests aren't open to an account that may not ask", async () => {
    mockApi({
      "GET /api/discover/movie/278": () => makeDetail(),
      "GET /api/requests/summary": () => makeSummary({ access: "off" }),
    });
    open("/discover/movie/278");
    expect(await screen.findByText("Requests aren't open to this account.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Request" })).not.toBeInTheDocument();
  });

  it("lets a manager approve a request with the settings' choices", async () => {
    const user = userEvent.setup();
    let request = makeRequest({ id: 5, actions: ["approve", "decline", "handled", "delete"] });
    const requests = mockApi({
      "GET /api/discover/movie/278": () => makeDetail({ status: "requested", requests: [request] }),
      "GET /api/requests/summary": () => makeSummary({ access: "auto", manage: true, pending: 1 }),
      "POST /api/requests/5/approve": () => {
        request = { ...request, status: "approved", stage: "searching", actions: [] };
        return request;
      },
    });
    open("/discover/movie/278", testUser);

    const list = await screen.findByRole("list");
    expect(within(list).getByText("Ann")).toBeInTheDocument();
    // Seldom wanted, the rest wait in a menu.
    expect(within(list).getByRole("button", { name: "More for The Shawshank Redemption" })).toBeInTheDocument();
    await user.click(within(list).getByRole("button", { name: "Approve" }));
    expect(await screen.findByText("“The Shawshank Redemption” was approved.")).toBeInTheDocument();
    expect(requests.find((r) => r.path === "/api/requests/5/approve")?.body).toEqual({});
    expect(await within(list).findByText("Searching")).toBeInTheDocument();
  });

  it("lets a manager decline a request with a reason", async () => {
    const user = userEvent.setup();
    const pending = makeRequest({ id: 5, actions: ["approve", "decline"] });
    const requests = mockApi({
      "GET /api/discover/movie/278": () => makeDetail({ status: "requested", requests: [pending] }),
      "GET /api/requests/summary": () => makeSummary({ access: "auto", manage: true, pending: 1 }),
      "POST /api/requests/5/decline": () => ({ ...pending, status: "declined", stage: "declined", actions: [] }),
    });
    open("/discover/movie/278", testUser);

    const list = await screen.findByRole("list");
    await user.click(within(list).getByRole("button", { name: "Decline" }));
    const dialog = screen.getByRole("dialog", { name: "Decline “The Shawshank Redemption”?" });
    await user.type(within(dialog).getByLabelText("Reason"), "We have it on disc");
    await user.click(within(dialog).getByRole("button", { name: "Decline" }));
    expect(await screen.findByText("“The Shawshank Redemption” was declined.")).toBeInTheDocument();
    expect(requests.find((r) => r.path === "/api/requests/5/decline")?.body).toEqual({ reason: "We have it on disc" });
  });

  it("declines once when Enter is pressed again before the answer comes", async () => {
    const user = userEvent.setup();
    const pending = makeRequest({ id: 5, actions: ["approve", "decline"] });
    let declined = false;
    const requests = mockApi({
      "GET /api/discover/movie/278": () => makeDetail({ status: "requested", requests: [pending] }),
      "GET /api/requests/summary": () => makeSummary({ access: "auto", manage: true, pending: 1 }),
      "POST /api/requests/5/decline": () => {
        if (declined) return apiError(409, "conflict", "it was declined meanwhile");
        declined = true;
        return { ...pending, status: "declined", stage: "declined", actions: [] };
      },
    });
    open("/discover/movie/278", testUser);

    await user.click(within(await screen.findByRole("list")).getByRole("button", { name: "Decline" }));
    const form = within(screen.getByRole("dialog", { name: "Decline “The Shawshank Redemption”?" })).getByLabelText("Reason").closest("form");
    expect(form).not.toBeNull();
    fireEvent.submit(form as HTMLFormElement);
    fireEvent.submit(form as HTMLFormElement);
    expect(await screen.findByText("“The Shawshank Redemption” was declined.")).toBeInTheDocument();
    expect(requests.filter((r) => r.path === "/api/requests/5/decline")).toHaveLength(1);
    expect(screen.queryByText("It was declined meanwhile")).not.toBeInTheDocument();
  });

  it("approves with a profile of the manager's choosing", async () => {
    const user = userEvent.setup();
    const pending = makeRequest({ id: 5, actions: ["approve", "decline"] });
    const requests = mockApi({
      "GET /api/discover/movie/278": () => makeDetail({ status: "requested", requests: [pending] }),
      "GET /api/requests/summary": () => makeSummary({ access: "auto", manage: true, pending: 1 }),
      "GET /api/requests/options": () => ({ radarr: choices(), sonarr: null }),
      "POST /api/requests/5/approve": () => ({ ...pending, status: "approved", stage: "searching", actions: [] }),
    });
    open("/discover/movie/278", testUser);

    await user.click(await screen.findByRole("button", { name: "More for The Shawshank Redemption" }));
    await user.click(screen.getByRole("menuitem", { name: "Approve…" }));
    const dialog = screen.getByRole("dialog", { name: "Approve “The Shawshank Redemption”" });
    // The settings' choices to begin with.
    expect(await within(dialog).findByLabelText("Quality profile")).toHaveValue("1");
    await user.selectOptions(within(dialog).getByLabelText("Quality profile"), "Ultra-HD");
    await user.click(within(dialog).getByRole("button", { name: "Approve" }));
    expect(await screen.findByText("“The Shawshank Redemption” was approved.")).toBeInTheDocument();
    expect(requests.find((r) => r.path === "/api/requests/5/approve")?.body).toEqual({ quality_profile: 4 });
  });

  it("tries a failed request again with choices of its own", async () => {
    const user = userEvent.setup();
    const failed = makeRequest({ id: 5, status: "failed", stage: "failed", reason: "Radarr refused the folder", actions: ["retry", "decline", "handled", "delete"] });
    const requests = mockApi({
      "GET /api/discover/movie/278": () => makeDetail({ status: "requested", requests: [failed] }),
      "GET /api/requests/summary": () => makeSummary({ access: "auto", manage: true, failed: 1 }),
      "GET /api/requests/options": () => ({ radarr: choices({ root_folders: [{ path: "/movies", free_bytes: null }, { path: "/films", free_bytes: null }] }), sonarr: null }),
      "POST /api/requests/5/approve": () => ({ ...failed, status: "approved", stage: "searching", actions: [] }),
    });
    open("/discover/movie/278", testUser);

    await user.click(await screen.findByRole("button", { name: "More for The Shawshank Redemption" }));
    await user.click(screen.getByRole("menuitem", { name: "Try again…" }));
    const dialog = screen.getByRole("dialog", { name: "Try “The Shawshank Redemption” again" });
    await user.selectOptions(await within(dialog).findByLabelText("Folder"), "/films");
    await user.click(within(dialog).getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("“The Shawshank Redemption” was approved.")).toBeInTheDocument();
    expect(requests.find((r) => r.path === "/api/requests/5/approve")?.body).toEqual({ root_folder: "/films" });
  });

  it("shows the settings' profile and folder as gone where Radarr no longer has them, as approving sends them", async () => {
    const user = userEvent.setup();
    const pending = makeRequest({ id: 5, actions: ["approve", "decline"] });
    const requests = mockApi({
      "GET /api/discover/movie/278": () => makeDetail({ status: "requested", requests: [pending] }),
      "GET /api/requests/summary": () => makeSummary({ access: "auto", manage: true, pending: 1 }),
      "GET /api/requests/options": () => ({ radarr: choices({ quality_profile: 9, root_folder: "/old" }), sonarr: null }),
      "POST /api/requests/5/approve": () => ({ ...pending, status: "approved", stage: "searching", actions: [] }),
    });
    open("/discover/movie/278", testUser);

    await user.click(await screen.findByRole("button", { name: "More for The Shawshank Redemption" }));
    await user.click(screen.getByRole("menuitem", { name: "Approve…" }));
    const dialog = screen.getByRole("dialog", { name: "Approve “The Shawshank Redemption”" });
    const profile = await within(dialog).findByLabelText("Quality profile");
    expect(profile).toHaveValue("9");
    expect(within(profile).getByRole("option", { selected: true })).toHaveTextContent("Profile 9 (gone)");
    expect(within(dialog).getByLabelText("Folder")).toHaveValue("/old");
    expect(within(within(dialog).getByLabelText("Folder")).getByRole("option", { selected: true })).toHaveTextContent("/old (gone)");
    await user.click(within(dialog).getByRole("button", { name: "Approve" }));
    expect(await screen.findByText("“The Shawshank Redemption” was approved.")).toBeInTheDocument();
    expect(requests.find((r) => r.path === "/api/requests/5/approve")?.body).toEqual({});
  });

  it("approves with the settings' choices when Radarr didn't answer", async () => {
    const user = userEvent.setup();
    const pending = makeRequest({ id: 5, actions: ["approve", "decline"] });
    const requests = mockApi({
      "GET /api/discover/movie/278": () => makeDetail({ status: "requested", requests: [pending] }),
      "GET /api/requests/summary": () => makeSummary({ access: "auto", manage: true, pending: 1 }),
      "GET /api/requests/options": () => ({
        radarr: choices({ quality_profiles: [], root_folders: [], error: "Radarr isn't answering" }),
        sonarr: null,
      }),
      "POST /api/requests/5/approve": () => ({ ...pending, status: "approved", stage: "approved", actions: [] }),
    });
    open("/discover/movie/278", testUser);

    await user.click(await screen.findByRole("button", { name: "More for The Shawshank Redemption" }));
    await user.click(screen.getByRole("menuitem", { name: "Approve…" }));
    const dialog = screen.getByRole("dialog", { name: "Approve “The Shawshank Redemption”" });
    expect(await within(dialog).findByText("Radarr isn't answering")).toBeInTheDocument();
    await user.click(within(dialog).getByRole("button", { name: "Approve with the settings' choices" }));
    expect(await screen.findByText("“The Shawshank Redemption” was approved.")).toBeInTheDocument();
    expect(requests.find((r) => r.path === "/api/requests/5/approve")?.body).toEqual({});
  });

  it("shows a download's progress", async () => {
    mockApi({
      "GET /api/discover/movie/278": () =>
        makeDetail({
          status: "requested",
          requests: [
            makeRequest({ id: 5, mine: true, status: "approved", stage: "downloading", download: { progress: 0.5, state: "downloading", eta: null, episodes: 1 } }),
          ],
        }),
      "GET /api/requests/summary": () => makeSummary(),
    });
    open("/discover/movie/278");
    const bars = await screen.findAllByRole("progressbar", { name: "The Shawshank Redemption: Downloading 50%" });
    expect(bars[0]).toHaveAttribute("aria-valuenow", "50");
  });

  it("says requests are off when the server says so", async () => {
    mockApi({
      "GET /api/discover/movie/278": () => apiError(503, "unavailable", "requests are off"),
      "GET /api/requests/summary": () => makeSummary({ enabled: false }),
    });
    open("/discover/movie/278");
    expect(await screen.findByRole("heading", { name: "Requests are off" })).toBeInTheDocument();
  });

  it("says TMDB isn't answering, and tries again", async () => {
    const user = userEvent.setup();
    let down = true;
    mockApi({
      "GET /api/discover/movie/278": () => (down ? apiError(503, "tmdb_down", "TMDB didn't answer") : makeDetail()),
      "GET /api/requests/summary": () => makeSummary(),
    });
    open("/discover/movie/278");
    expect(await screen.findByRole("heading", { name: "TMDB isn't answering" })).toBeInTheDocument();
    down = false;
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("heading", { name: "The Shawshank Redemption", level: 1 })).toBeInTheDocument();
  });

  it("knows nothing of other kinds of titles, nor of titles TMDB doesn't have", async () => {
    mockApi({ "GET /api/requests/summary": () => makeSummary() });
    const { unmount } = open("/discover/person/5");
    expect(await screen.findByRole("heading", { name: "Page not found" })).toBeInTheDocument();
    unmount();

    mockApi({ "GET /api/requests/summary": () => makeSummary(), "GET /api/discover/movie/9": () => apiError(404, "not_found", "no such title") });
    open("/discover/movie/9");
    expect(await screen.findByRole("heading", { name: "Page not found" })).toBeInTheDocument();
  });
});

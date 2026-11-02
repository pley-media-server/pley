import type { ReactElement } from "react";
import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { requestKeys, type MediaRequest } from "@/api/requests";
import type { User } from "@/api/types";
import { itemUrl } from "@/lib/urls";
import { apiError, LocationDisplay, mockApi, renderWithProviders, testUser } from "@/test/render";
import { makeRequest, makeRequestList, makeSummary } from "@/test/requests";
import { RequestsPage } from "./RequestsPage";

const ann: User = { ...testUser, id: 2, username: "ann", name: "Ann", is_admin: false };
const manager = makeSummary({ access: "auto", manage: true, pending: 2, failed: 1 });
const people = [
  { id: 2, name: "Ann", avatar_url: null, avatar_color: null },
  { id: 3, name: "Bo", avatar_url: null, avatar_color: null },
];

const shawshank = makeRequest({ id: 1, title: "The Shawshank Redemption", actions: ["approve", "decline", "handled", "delete"] });
const jaws = makeRequest({ id: 2, tmdb_id: 578, title: "Jaws", year: 1975, actions: ["approve", "decline", "handled", "delete"] });
const failed = makeRequest({
  id: 3,
  kind: "show",
  tmdb_id: 70523,
  title: "Dark",
  seasons: [1, 2, 3],
  status: "failed",
  reason: "no root folder is chosen",
  actions: ["retry", "decline", "handled", "delete"],
});

function show(ui: ReactElement, route = "/requests", user: User = testUser) {
  return renderWithProviders(
    <>
      {ui}
      <LocationDisplay />
    </>,
    { route, user },
  );
}

const paths = (requests: { path: string }[]) => requests.map((r) => r.path);
const rows = async () => within(await screen.findByRole("list")).getAllByRole("listitem");

describe("RequestsPage for managers", () => {
  it("opens on what needs them, oldest first, saying why a request failed", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/requests/summary": () => manager,
      "GET /api/requests": () => makeRequestList([shawshank, jaws, failed], 3, people),
    });
    show(<RequestsPage />);

    expect(await screen.findByRole("heading", { name: "Requests" })).toBeInTheDocument();
    const tabs = screen.getByRole("tablist", { name: "Requests" });
    expect(within(tabs).getByRole("tab", { name: "Needs attention 3" })).toHaveAttribute("aria-selected", "true");
    const [first, , third] = await rows();
    expect(within(first as HTMLElement).getByRole("button", { name: "Approve" })).toBeInTheDocument();
    expect(third).toHaveTextContent("Seasons 1–3");
    expect(within(third as HTMLElement).getByText("Couldn't send")).toBeInTheDocument();
    expect(within(third as HTMLElement).getByText("no root folder is chosen")).toBeInTheDocument();
    expect(within(third as HTMLElement).getByRole("button", { name: "Try again" })).toBeInTheDocument();
    expect(paths(requests)).toContain("/api/requests?attention=1&order=oldest&limit=50");
    // Choices of its own, from its menu.
    await user.click(within(first as HTMLElement).getByRole("button", { name: "More for The Shawshank Redemption" }));
    expect(screen.getByRole("menuitem", { name: "Approve…" })).toBeInTheDocument();
  });

  it("shows everyone's requests, filtered by status as pressed buttons", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/requests/summary": () => manager,
      "GET /api/requests": (req) =>
        req.path.includes("status=pending")
          ? { ...makeRequestList([shawshank], 1, people), counts: makeRequestList([shawshank, jaws, failed]).counts }
          : makeRequestList([shawshank, jaws, failed], 3, people),
    });
    show(<RequestsPage />);

    await user.click(await screen.findByRole("tab", { name: "All requests" }));
    expect(screen.getByTestId("location")).toHaveTextContent("/requests?view=all");
    const status = await screen.findByRole("group", { name: "Status" });
    expect(within(status).getByRole("button", { name: "All" })).toHaveAttribute("aria-pressed", "true");
    await user.click(await within(status).findByRole("button", { name: "Pending 2" }));
    expect(within(status).getByRole("button", { name: /^Pending/ })).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("location")).toHaveTextContent("/requests?view=all&status=pending");
    await waitFor(() => expect(within(screen.getByRole("list")).getAllByRole("listitem")).toHaveLength(1));
    expect(paths(requests)).toContain("/api/requests?status=pending&limit=50");
  });

  it("filters by kind, by who asked and by title", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/requests/summary": () => manager,
      "GET /api/requests": () => makeRequestList([shawshank, jaws], 2, people),
    });
    show(<RequestsPage />, "/requests?view=all");

    await user.click(within(await screen.findByRole("group", { name: "Kind" })).getByRole("button", { name: "Movies" }));
    await user.selectOptions(await screen.findByRole("combobox", { name: "Who asked" }), "Bo");
    await user.type(screen.getByRole("searchbox", { name: "Requests for titles containing" }), "jaws");
    await waitFor(() => expect(paths(requests)).toContain("/api/requests?kind=movie&user_id=3&q=jaws&limit=50"));
    expect(screen.getByTestId("location")).toHaveTextContent("/requests?view=all&kind=movie&user=3&q=jaws");
  });

  it("finds titles by as much of a long query as the server takes", async () => {
    const requests = mockApi({
      "GET /api/requests/summary": () => manager,
      "GET /api/requests": () => makeRequestList([shawshank], 1, people),
    });
    show(<RequestsPage />, `/requests?view=all&q=${"x".repeat(250)}`);
    expect(await screen.findByText("The Shawshank Redemption")).toBeInTheDocument();
    expect(paths(requests)).toContain(`/api/requests?q=${"x".repeat(200)}&limit=50`);
    expect(screen.getByRole("searchbox", { name: "Requests for titles containing" })).toHaveAttribute("maxlength", "200");
  });

  it("approves several at once, saying how it went", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/requests/summary": () => manager,
      "GET /api/requests": () => makeRequestList([shawshank, jaws, failed], 3, people),
      "POST /api/requests/1/approve": () => ({ ...shawshank, status: "approved", stage: "searching", actions: [] }),
      "POST /api/requests/2/approve": () => apiError(409, "conflict", "it was declined meanwhile"),
    });
    show(<RequestsPage />);

    await user.click(await screen.findByRole("checkbox", { name: "Select The Shawshank Redemption" }));
    await user.click(screen.getByRole("checkbox", { name: "Select Jaws" }));
    const bar = screen.getByRole("region", { name: "Requests picked" });
    expect(bar).toHaveTextContent("2 requests picked");
    await user.click(within(bar).getByRole("button", { name: "Approve" }));
    expect(await screen.findByText("Approved 1 of 2. “Jaws”: It was declined meanwhile")).toBeInTheDocument();
    expect(paths(requests.filter((r) => r.method === "POST"))).toEqual(["/api/requests/1/approve", "/api/requests/2/approve"]);
    expect(screen.queryByRole("region", { name: "Requests picked" })).not.toBeInTheDocument();
  });

  it("declines several at once, with one reason", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/requests/summary": () => manager,
      "GET /api/requests": () => makeRequestList([shawshank, jaws], 2, people),
      "POST /api/requests/1/decline": () => ({ ...shawshank, status: "declined", stage: "declined", actions: [] }),
      "POST /api/requests/2/decline": () => ({ ...jaws, status: "declined", stage: "declined", actions: [] }),
    });
    show(<RequestsPage />);

    await user.click(await screen.findByRole("checkbox", { name: "Select The Shawshank Redemption" }));
    await user.click(screen.getByRole("checkbox", { name: "Select Jaws" }));
    await user.click(within(screen.getByRole("region", { name: "Requests picked" })).getByRole("button", { name: "Decline" }));
    const dialog = screen.getByRole("dialog", { name: "Decline 2 requests?" });
    await user.type(within(dialog).getByLabelText("Reason"), "Not this month");
    await user.click(within(dialog).getByRole("button", { name: "Decline" }));
    expect(await screen.findByText("Declined 2 requests.")).toBeInTheDocument();
    expect(requests.filter((r) => r.method === "POST").map((r) => r.body)).toEqual([{ reason: "Not this month" }, { reason: "Not this month" }]);
  });

  it("marks a request as added by hand, from its menu", async () => {
    const user = userEvent.setup();
    const requests = mockApi({
      "GET /api/requests/summary": () => manager,
      "GET /api/requests": () => makeRequestList([shawshank], 1, people),
      "POST /api/requests/1/handled": () => ({ ...shawshank, status: "approved", stage: "searching", actions: [] }),
    });
    show(<RequestsPage />);

    await user.click(await screen.findByRole("button", { name: "More for The Shawshank Redemption" }));
    await user.click(screen.getByRole("menuitem", { name: "Mark as added by hand" }));
    expect(await screen.findByText("“The Shawshank Redemption” is marked as added by hand.")).toBeInTheDocument();
    expect(requests.some((r) => r.method === "POST" && r.path === "/api/requests/1/handled")).toBe(true);
  });

  it("deletes a request after asking, saying Radarr and Sonarr keep what they have", async () => {
    const user = userEvent.setup();
    let list: MediaRequest[] = [makeRequest({ id: 7, status: "available", item_id: 70, actions: ["delete"] })];
    const requests = mockApi({
      "GET /api/requests/summary": () => manager,
      "GET /api/requests": () => makeRequestList(list),
      "DELETE /api/requests/7": () => {
        list = [];
        return undefined;
      },
    });
    show(<RequestsPage />, "/requests?view=all");

    const [row] = await rows();
    expect(within(row as HTMLElement).getByRole("link", { name: "Watch" })).toHaveAttribute("href", itemUrl(70));
    await user.click(within(row as HTMLElement).getByRole("button", { name: "More for The Shawshank Redemption" }));
    await user.click(screen.getByRole("menuitem", { name: "Delete" }));
    const dialog = screen.getByRole("dialog", { name: "Delete this request?" });
    expect(dialog).toHaveTextContent("It goes for everyone who asked for The Shawshank Redemption or follows it. Radarr and Sonarr keep what they have.");
    await user.click(within(dialog).getByRole("button", { name: "Delete" }));
    expect(await screen.findByText("No one has asked for anything yet.")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "DELETE")?.path).toBe("/api/requests/7");
  });

  it("says when nothing waits for them, with the way to everyone's", async () => {
    const user = userEvent.setup();
    mockApi({ "GET /api/requests/summary": () => makeSummary({ access: "auto", manage: true }), "GET /api/requests": () => makeRequestList([]) });
    show(<RequestsPage />);
    expect(await screen.findByText("Nothing waits for you.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "See all requests" }));
    expect(screen.getByTestId("location")).toHaveTextContent("/requests?view=all");
  });

  it("clears its filters when nothing matches them", async () => {
    const user = userEvent.setup();
    mockApi({
      "GET /api/requests/summary": () => manager,
      "GET /api/requests": (req) => (req.path.includes("q=zzz") ? makeRequestList([]) : makeRequestList([shawshank], 1, people)),
    });
    show(<RequestsPage />, "/requests?view=all&kind=movie&q=zzz");
    expect(await screen.findByText("No requests match.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Clear filters" }));
    expect(screen.getByTestId("location")).toHaveTextContent(/^\/requests\?view=all$/);
    expect(await screen.findByText("The Shawshank Redemption")).toBeInTheDocument();
  });

  it("asks an app that may not approve to sign in again", async () => {
    mockApi({
      "GET /api/requests/summary": () => makeSummary({ access: "auto", sign_in_again: "approvals" }),
      "GET /api/requests": () => makeRequestList([]),
    });
    show(<RequestsPage />);
    expect(await screen.findByText("Sign in again to approve and decline here.")).toBeInTheDocument();
  });
});

describe("RequestsPage for everyone else", () => {
  it("lists their own requests and those they follow, with what they may do, and marks the news seen", async () => {
    const user = userEvent.setup();
    const mine = makeRequest({ id: 1, mine: true, actions: ["withdraw"] });
    const declined = makeRequest({ id: 2, title: "Old Film", mine: true, status: "declined", reason: "We have it on disc", unseen: true, actions: ["withdraw"] });
    const followed = makeRequest({ id: 3, title: "Jaws", watching: true, status: "approved", stage: "searching", actions: ["unwatch"] });
    const requests = mockApi({
      "GET /api/requests/summary": () => makeSummary({ unseen: 1, own: 2, movies_left: 2, movies_limit: 3 }),
      "GET /api/requests": () => makeRequestList([mine, declined, followed]),
      "POST /api/requests/seen": () => undefined,
      "DELETE /api/requests/3/watch": () => undefined,
    });
    show(<RequestsPage />, "/requests", ann);

    expect(await screen.findByRole("heading", { name: "Your requests" })).toBeInTheDocument();
    expect(screen.queryByRole("tablist")).not.toBeInTheDocument();
    const [first, second, third] = await rows();
    expect(within(first as HTMLElement).getByRole("button", { name: "Withdraw" })).toBeInTheDocument();
    expect(within(first as HTMLElement).queryByRole("button", { name: "Approve" })).not.toBeInTheDocument();
    // Declined, it may be cleared away.
    expect(second).toHaveTextContent("Declined: “We have it on disc”");
    expect(within(second as HTMLElement).getByRole("button", { name: "Remove" })).toBeInTheDocument();
    expect(third).toHaveTextContent("You follow it");
    expect(screen.getByText("You can ask for 2 more movies. Limits: 3 movies per 7 days.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Discover" })).toHaveAttribute("href", "/discover");
    await waitFor(() => expect(requests.some((r) => r.method === "POST" && r.path === "/api/requests/seen")).toBe(true));

    await user.click(within(third as HTMLElement).getByRole("button", { name: "Stop following" }));
    expect(await screen.findByText("You won't hear about “Jaws” any more.")).toBeInTheDocument();
  });

  it("marks news seen only once it shows, and again as more comes while the page is open", async () => {
    const user = userEvent.setup();
    let list = [makeRequest({ id: 1, mine: true })];
    const requests = mockApi({
      "GET /api/requests/summary": () => makeSummary({ unseen: 1, own: 2 }),
      "GET /api/requests": (req) => makeRequestList(req.path.includes("status=pending") ? list : [...list, makeRequest({ id: 2, title: "Old Film", mine: true, status: "declined", unseen: true, updated_at: 1_700_000_100 })]),
      "POST /api/requests/seen": () => undefined,
    });
    const { client } = show(<RequestsPage />, "/requests?status=pending", ann);
    const seen = () => requests.filter((r) => r.method === "POST" && r.path === "/api/requests/seen");

    // The declined one, with news, isn't among the pending ones.
    expect(await screen.findByText("The Shawshank Redemption")).toBeInTheDocument();
    expect(seen()).toHaveLength(0);

    await user.click(within(screen.getByRole("group", { name: "Status" })).getByRole("button", { name: "All" }));
    expect(await screen.findByText("Old Film")).toBeInTheDocument();
    await waitFor(() => expect(seen()).toHaveLength(1));

    // Ready while the page is open.
    list = [makeRequest({ id: 1, mine: true, status: "available", stage: "available", unseen: true, updated_at: 1_700_000_200 })];
    await act(() => client.invalidateQueries({ queryKey: requestKeys.lists }));
    await waitFor(() => expect(seen()).toHaveLength(2));
  });

  it("asks before withdrawing", async () => {
    const user = userEvent.setup();
    let list: MediaRequest[] = [makeRequest({ id: 1, mine: true, actions: ["withdraw"] })];
    const requests = mockApi({
      "GET /api/requests/summary": () => makeSummary({ own: 1 }),
      "GET /api/requests": () => makeRequestList(list),
      "DELETE /api/requests/1": () => {
        list = [];
        return undefined;
      },
    });
    show(<RequestsPage />, "/requests", ann);
    await user.click(await screen.findByRole("button", { name: "Withdraw" }));
    const dialog = screen.getByRole("dialog", { name: "Withdraw your request for “The Shawshank Redemption”?" });
    expect(dialog).toHaveTextContent("It won't be asked for any more.");
    await user.click(within(dialog).getByRole("button", { name: "Withdraw" }));
    expect(await screen.findByText("You haven't asked for anything yet.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Find something to ask for" })).toHaveAttribute("href", "/discover");
    expect(requests.find((r) => r.method === "DELETE")?.path).toBe("/api/requests/1");
  });

  it("says when a declined request may be removed, from when the server says", async () => {
    const user = userEvent.setup();
    const declined = makeRequest({ id: 1, mine: true, status: "declined", actions: ["withdraw"] });
    const early = {
      code: "conflict",
      message: "a declined request stays for 30 days: it may be removed from 2026-11-30",
      request: declined,
      retry_at: Math.floor(Date.now() / 1000) + 3 * 86_400 - 60,
    };
    mockApi({
      "GET /api/requests/summary": () => makeSummary({ own: 1 }),
      "GET /api/requests": () => makeRequestList([declined]),
      "DELETE /api/requests/1": () => new Response(JSON.stringify(early), { status: 409, headers: { "content-type": "application/json" } }),
    });
    show(<RequestsPage />, "/requests", ann);
    await user.click(await screen.findByRole("button", { name: "Remove" }));
    await user.click(within(screen.getByRole("dialog", { name: "Remove “The Shawshank Redemption” from your requests?" })).getByRole("button", { name: "Remove" }));
    expect(await screen.findByText("You can remove it in 3 days.")).toBeInTheDocument();
    expect(screen.queryByText(/2026-11-30/)).not.toBeInTheDocument();
  });

  it("asks again for what was declined a while ago", async () => {
    const user = userEvent.setup();
    const old = makeRequest({ id: 4, mine: true, status: "declined", actions: ["ask_again", "withdraw"] });
    const requests = mockApi({
      "GET /api/requests/summary": () => makeSummary({ own: 1 }),
      "GET /api/requests": () => makeRequestList([old]),
      "POST /api/requests": () =>
        new Response(JSON.stringify(makeRequest({ id: 9, mine: true })), { status: 201, headers: { "content-type": "application/json" } }),
    });
    show(<RequestsPage />, "/requests", ann);
    await user.click(await screen.findByRole("button", { name: "Request again" }));
    expect(await screen.findByText("Asked for “The Shawshank Redemption”: an administrator decides.")).toBeInTheDocument();
    expect(requests.find((r) => r.method === "POST")?.body).toEqual({ kind: "movie", tmdb_id: 278 });
  });

  it("shows 50 at a time", async () => {
    const user = userEvent.setup();
    const many = Array.from({ length: 50 }, (_, i) => makeRequest({ id: 100 + i, title: `Title ${i}`, mine: true }));
    const requests = mockApi({
      "GET /api/requests/summary": () => makeSummary({ own: 51 }),
      "GET /api/requests": (req) =>
        req.path.includes("offset=50") ? makeRequestList([makeRequest({ id: 999, title: "The Last One", mine: true })], 51) : makeRequestList(many, 51),
    });
    show(<RequestsPage />, "/requests", ann);
    await user.click(await screen.findByRole("button", { name: "Show more" }));
    expect(await screen.findByText("The Last One")).toBeInTheDocument();
    expect(paths(requests)).toContain("/api/requests?offset=50&limit=50");
    expect(screen.queryByRole("button", { name: "Show more" })).not.toBeInTheDocument();
  });

  it("leaves the filters out for someone who never asked for anything", async () => {
    mockApi({ "GET /api/requests/summary": () => makeSummary(), "GET /api/requests": () => makeRequestList([]) });
    show(<RequestsPage />, "/requests", ann);
    expect(await screen.findByText("You haven't asked for anything yet.")).toBeInTheDocument();
    expect(screen.queryByRole("group", { name: "Status" })).not.toBeInTheDocument();
    expect(screen.queryByRole("searchbox")).not.toBeInTheDocument();
  });

  it("says requests are off when the server says so", async () => {
    mockApi({ "GET /api/requests/summary": () => makeSummary({ enabled: false }) });
    show(<RequestsPage />, "/requests", ann);
    expect(await screen.findByRole("heading", { name: "Requests are off" })).toBeInTheDocument();
  });

  it("says requests aren't open to an account that may not ask and never did", async () => {
    mockApi({ "GET /api/requests/summary": () => makeSummary({ access: "off" }) });
    show(<RequestsPage />, "/requests", ann);
    expect(await screen.findByRole("heading", { name: "Requests aren't open to this account" })).toBeInTheDocument();
  });
});

import { useNavigate } from "react-router";
import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { requestKeys, type RequestSummary } from "@/api/requests";
import type { ServerEvent, User } from "@/api/types";
import { itemUrl } from "@/lib/urls";
import { LocationDisplay, mockApi, renderWithProviders, testQueryClient, testUser } from "@/test/render";
import { makeRequest, makeRequestList, makeSummary } from "@/test/requests";
import { RequestNotices } from "./RequestNotices";

const listeners = vi.hoisted(() => new Set<(ev: ServerEvent) => void>());
vi.mock("@/api/events", async (importOriginal) => ({
  ...(await importOriginal<object>()),
  onServerEvent: (listener: (ev: ServerEvent) => void) => {
    listeners.add(listener);
    return () => listeners.delete(listener);
  },
}));

const emit = (ev: ServerEvent) => act(() => listeners.forEach((l) => l(ev)));
const ann: User = { ...testUser, id: 2, username: "ann", name: "Ann", is_admin: false };

const ready: ServerEvent = {
  type: "request_available",
  request_id: 4,
  title: "Heat",
  kind: "movie",
  tmdb_id: 949,
  item_id: 70,
  poster_url: null,
  seasons: null,
  complete: true,
};

const created: ServerEvent = {
  type: "request_created",
  request_id: 6,
  title: "Dark",
  kind: "show",
  tmdb_id: 1399,
  seasons: [1, 2, 3],
  requested_by: { id: 2, name: "Ann", avatar_url: null, avatar_color: null },
  status: "pending",
};

const failed: ServerEvent = { type: "request_failed", request_id: 7, title: "Heat", kind: "movie", reason: "Radarr isn't answering", retry_at: null };

function Leave() {
  const navigate = useNavigate();
  return (
    <button type="button" onClick={() => void navigate("/")}>
      Leave
    </button>
  );
}

/** The notices, with the summary already in. */
function render(summary: RequestSummary, { route = "/", user = ann }: { route?: string; user?: User } = {}) {
  const client = testQueryClient();
  client.setQueryData(requestKeys.summary, summary);
  return renderWithProviders(
    <>
      <RequestNotices />
      <Leave />
      <LocationDisplay />
    </>,
    { route, user, client },
  );
}

describe("RequestNotices", () => {
  beforeEach(() => {
    localStorage.clear();
    listeners.clear();
  });

  it("tells of a title ready to watch, with Open to it", async () => {
    const user = userEvent.setup();
    mockApi({ "GET /api/requests/summary": () => makeSummary() });
    render(makeSummary());
    emit(ready);
    expect(await screen.findByText("“Heat” is ready to watch.")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Open" }));
    expect(screen.getByTestId("location")).toHaveTextContent(itemUrl(70));
  });

  it("says which seasons are ready, and what was decided", async () => {
    mockApi({ "GET /api/requests/summary": () => makeSummary() });
    render(makeSummary());
    emit({ ...ready, kind: "show", title: "Dark", seasons: [1, 2], complete: false });
    expect(await screen.findByText("Seasons 1–2 of “Dark” are ready to watch.")).toBeInTheDocument();
    emit({ type: "request_decided", request_id: 5, title: "Old Film", kind: "movie", tmdb_id: 1, status: "declined", reason: "We have it on disc" });
    expect(await screen.findByText("“Old Film” was declined: “We have it on disc”.")).toBeInTheDocument();
  });

  it("tells managers of new requests waiting for them and of those that failed", async () => {
    const summary = makeSummary({ access: "auto", manage: true });
    mockApi({ "GET /api/requests/summary": () => summary });
    render(summary, { user: testUser });
    emit(created);
    expect(await screen.findByText("Ann asked for “Dark” (Seasons 1–3).")).toBeInTheDocument();
    emit(failed);
    expect(await screen.findByText("“Heat” couldn't be sent to Radarr: Radarr isn't answering.")).toBeInTheDocument();
    // Approved as they were made: nothing waits.
    emit({ ...created, request_id: 8, title: "Heat", status: "approved" });
    expect(screen.queryByText(/asked for “Heat”/)).not.toBeInTheDocument();
  });

  it("tells others nothing of anyone's new or failed requests", () => {
    mockApi({ "GET /api/requests/summary": () => makeSummary() });
    render(makeSummary());
    emit(created);
    emit(failed);
    expect(screen.queryByText(/asked for/)).not.toBeInTheDocument();
    expect(screen.queryByText(/couldn't be sent/)).not.toBeInTheDocument();
  });

  it("holds notices while a video plays, and shows the last three once it stops", async () => {
    const user = userEvent.setup();
    mockApi({ "GET /api/requests/summary": () => makeSummary() });
    render(makeSummary(), { route: "/play/5" });
    for (const [i, title] of ["One", "Two", "Three", "Four"].entries()) emit({ ...ready, request_id: 10 + i, title });
    expect(screen.queryByText(/is ready to watch/)).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Leave" }));
    expect(await screen.findByText("“Four” is ready to watch.")).toBeInTheDocument();
    expect(screen.getByText("“Two” is ready to watch.")).toBeInTheDocument();
    expect(screen.queryByText("“One” is ready to watch.")).not.toBeInTheDocument();
  });

  it("holds notices while a Live TV channel plays, and not on Live TV's page", async () => {
    const user = userEvent.setup();
    mockApi({ "GET /api/requests/summary": () => makeSummary() });
    const channel = render(makeSummary(), { route: "/live/a1b2/7" });
    emit(ready);
    expect(screen.queryByText("“Heat” is ready to watch.")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Leave" }));
    expect(await screen.findByText("“Heat” is ready to watch.")).toBeInTheDocument();
    channel.unmount();

    // An older link to a channel.
    const older = render(makeSummary(), { route: "/live/7" });
    emit({ ...ready, request_id: 5, title: "Ran" });
    expect(screen.queryByText("“Ran” is ready to watch.")).not.toBeInTheDocument();
    older.unmount();

    render(makeSummary(), { route: "/live" });
    emit({ ...ready, request_id: 6, title: "Alien" });
    expect(await screen.findByText("“Alien” is ready to watch.")).toBeInTheDocument();
  });

  it("tells what came while away in one notice, once", async () => {
    const summary = makeSummary({ unseen: 2, own: 2 });
    const news = [
      makeRequest({ id: 1, title: "Heat", status: "available", item_id: 70, unseen: true, updated_at: 1_700_000_500 }),
      makeRequest({ id: 2, title: "Old Film", status: "declined", unseen: true, updated_at: 1_700_000_400 }),
    ];
    const requests = mockApi({
      "GET /api/requests/summary": () => summary,
      "GET /api/requests": () => makeRequestList(news),
    });
    const { unmount } = render(summary);
    expect(await screen.findByText("2 of your requests have news")).toBeInTheDocument();
    expect(requests.find((r) => r.path.startsWith("/api/requests?"))?.path).toBe("/api/requests?unseen=1&limit=20");
    unmount();

    // Back again with nothing newer: nothing to tell.
    render(summary);
    await waitFor(() => expect(requests.filter((r) => r.path.startsWith("/api/requests?"))).toHaveLength(2));
    expect(screen.queryByText("2 of your requests have news")).not.toBeInTheDocument();
  });

  it("tells what came while away of a show partly ready, and not the seasons told as they came", async () => {
    let seasons = [1, 2];
    const dark = () =>
      makeRequest({
        id: 7,
        kind: "show",
        tmdb_id: 1399,
        title: "Dark",
        seasons: [1, 2, 3, 4],
        status: "approved",
        stage: "downloading",
        available_seasons: seasons,
        unseen: true,
        updated_at: 1_700_000_000 + seasons.length,
      });
    const requests = mockApi({
      "GET /api/requests/summary": () => makeSummary({ unseen: 1, own: 1 }),
      "GET /api/requests/7": () => dark(),
      "GET /api/requests": () => makeRequestList([dark()]),
    });
    const listed = () => requests.filter((r) => r.path.startsWith("/api/requests?"));
    const quiet = render(makeSummary());
    emit({ ...ready, request_id: 7, kind: "show", tmdb_id: 1399, title: "Dark", seasons: [2], complete: false });
    expect(await screen.findByText("Season 2 of “Dark” is ready to watch.")).toBeInTheDocument();
    // Season 1, ready before it, comes with the request.
    await waitFor(() => expect(requests.some((r) => r.path === "/api/requests/7")).toBe(true));
    quiet.unmount();

    // Back, with nothing more ready: nothing to tell.
    const back = render(makeSummary({ unseen: 1, own: 1 }));
    await waitFor(() => expect(listed()).toHaveLength(1));
    await act(() => new Promise((resolve) => setTimeout(resolve, 50)));
    expect(screen.queryByText(/of “Dark”/)).not.toBeInTheDocument();
    back.unmount();

    // Season 3 came while away, while Season 4 downloads.
    seasons = [1, 2, 3];
    render(makeSummary({ unseen: 1, own: 1 }));
    expect(await screen.findByText("Seasons 1–3 of “Dark” are ready to watch.")).toBeInTheDocument();
    expect(listed()).toHaveLength(2);
  });

  it("tells a single piece of news as it is", async () => {
    const summary = makeSummary({ unseen: 1, own: 1 });
    mockApi({
      "GET /api/requests/summary": () => summary,
      "GET /api/requests": () => makeRequestList([makeRequest({ id: 1, title: "Heat", status: "available", item_id: 70, unseen: true })]),
    });
    render(summary);
    expect(await screen.findByText("“Heat” is ready to watch.")).toBeInTheDocument();
  });
});

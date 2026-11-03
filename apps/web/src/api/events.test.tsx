import type { ReactNode } from "react";
import { QueryClientProvider, type InfiniteData, type QueryClient } from "@tanstack/react-query";
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { connectionLost, eventsLive } from "@/lib/connection";
import { makeItem } from "@/test/fixtures";
import { apiError, mockApi, testQueryClient } from "@/test/render";
import { makeDetail, makeRequest, makeRequestList, makeShowDetail, makeSummary, makeTitle } from "@/test/requests";
import { castKeys, type CastTarget } from "./cast";
import { LOST_AFTER_MS, onServerEvent, useServerEvents } from "./events";
import { keys } from "./hooks";
import { EVERY_REQUEST, requestKeys, type DiscoverPage, type DiscoverTitle } from "./requests";
import type { Activity, CastState, Library, ServerEvent } from "./types";

/** Stands in for the browser's EventSource; tests drive it. */
class FakeEventSource {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSED = 2;
  static all: FakeEventSource[] = [];

  readyState = FakeEventSource.CONNECTING;
  onopen: (() => void) | null = null;
  onmessage: ((msg: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;

  readonly url: string;

  constructor(url: string) {
    this.url = url;
    FakeEventSource.all.push(this);
  }

  close() {
    this.readyState = FakeEventSource.CLOSED;
  }

  open() {
    this.readyState = FakeEventSource.OPEN;
    this.onopen?.();
  }

  send(event: ServerEvent | string) {
    this.onmessage?.({ data: typeof event === "string" ? event : JSON.stringify(event) });
  }

  /** The server went away: the browser gives up on this connection. */
  fail() {
    this.readyState = FakeEventSource.CLOSED;
    this.onerror?.();
  }
}

const library = (id: number, scanning = false): Library => ({
  id,
  name: `Library ${id}`,
  kind: "movies",
  paths: [],
  item_count: 0,
  created_at: 0,
  scanned_at: null,
  scanning,
});

/** The events of account 1 (`null`: signed out). */
function setup(userId: number | null = 1) {
  const client = testQueryClient();
  const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  const hook = renderHook(({ user }) => useServerEvents(user), { wrapper, initialProps: { user: userId } });
  return { client, ...hook, source: () => FakeEventSource.all.at(-1) as FakeEventSource };
}

function invalidated(client: QueryClient, key: readonly unknown[]): boolean {
  return client.getQueryState(key)?.isInvalidated ?? false;
}

beforeEach(() => {
  FakeEventSource.all = [];
  vi.stubGlobal("EventSource", FakeEventSource);
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useServerEvents", () => {
  it("connects only while signed in, and closes when done", () => {
    const { rerender, unmount, source } = setup(null);
    expect(FakeEventSource.all).toHaveLength(0);

    rerender({ user: 1 });
    expect(source().url).toBe("/api/events");
    unmount();
    expect(source().readyState).toBe(FakeEventSource.CLOSED);
  });

  it("opens another stream for another profile, closing the last one's", () => {
    const { rerender, source } = setup();
    const first = source();
    rerender({ user: 1 });
    expect(FakeEventSource.all).toHaveLength(1);

    rerender({ user: 2 });
    expect(first.readyState).toBe(FakeEventSource.CLOSED);
    expect(FakeEventSource.all).toHaveLength(2);
    expect(source().readyState).toBe(FakeEventSource.CONNECTING);
  });

  it("follows scans", () => {
    const { client, source } = setup();
    client.setQueryData(keys.libraries, [library(1), library(2)]);
    client.setQueryData<Activity>(keys.activity, { scans: [], sessions: [] });

    act(() => source().send({ type: "scan_started", library_id: 1 }));
    expect(client.getQueryData<Library[]>(keys.libraries)?.map((l) => l.scanning)).toEqual([true, false]);
    expect(invalidated(client, keys.activity)).toBe(true);

    act(() => source().send({ type: "scan_progress", library_id: 1, phase: "probing", processed: 3, total: 10 }));
    act(() => source().send({ type: "scan_progress", library_id: 1, phase: "metadata", processed: 7, total: 10 }));
    expect(client.getQueryData<Activity>(keys.activity)?.scans).toEqual([
      expect.objectContaining({ library_id: 1, library_name: "Library 1", phase: "metadata", processed: 7, total: 10 }),
    ]);

    act(() => source().send({ type: "scan_finished", library_id: 1, added: 2, updated: 0, removed: 0 }));
    expect(client.getQueryData<Library[]>(keys.libraries)?.[0]?.scanning).toBe(false);
  });

  it("refreshes a changed library once per burst", () => {
    const { client, source } = setup();
    client.setQueryData(keys.library(1), library(1));
    client.setQueryData(keys.homeHubs, { hubs: [] });

    act(() => {
      for (let i = 0; i < 20; i++) source().send({ type: "library_changed", library_id: 1 });
    });
    expect(invalidated(client, keys.library(1))).toBe(false);

    act(() => {
      vi.advanceTimersByTime(1500);
    });
    expect(invalidated(client, keys.library(1))).toBe(true);
    expect(invalidated(client, keys.homeHubs)).toBe(true);
  });

  it("refreshes everything after a reconnect, not on the first connection", () => {
    const { client, source } = setup();
    client.setQueryData(keys.item(5), { id: 5 });

    act(() => source().open());
    expect(invalidated(client, keys.item(5))).toBe(false);

    act(() => source().open());
    expect(invalidated(client, keys.item(5))).toBe(true);
  });

  it("says the server is out of reach when it stays away, and stops once it is back", async () => {
    const health = vi.fn(() => Promise.reject(new TypeError("Failed to fetch")));
    vi.stubGlobal("fetch", health);
    const { source } = setup();
    act(() => source().open());

    act(() => source().fail());
    act(() => {
      vi.advanceTimersByTime(LOST_AFTER_MS - 1);
    });
    expect(connectionLost()).toBe(false);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    await vi.waitFor(() => expect(connectionLost()).toBe(true));
    expect(health).toHaveBeenCalledWith("/api/health", expect.anything());

    act(() => {
      vi.advanceTimersByTime(30_000);
    });
    act(() => source().open());
    expect(connectionLost()).toBe(false);
  });

  it("doesn't say so for a stream the server refuses while it answers", async () => {
    vi.stubGlobal("fetch", vi.fn(() => Promise.resolve(new Response("ok"))));
    const { source } = setup();

    act(() => source().fail());
    await act(async () => {
      await vi.advanceTimersByTimeAsync(LOST_AFTER_MS);
    });
    expect(connectionLost()).toBe(false);
  });

  it("reconnects after the server goes away, waiting longer each time", () => {
    const { source } = setup();

    act(() => source().fail());
    act(() => {
      vi.advanceTimersByTime(1_999);
    });
    expect(FakeEventSource.all).toHaveLength(1);
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(FakeEventSource.all).toHaveLength(2);

    act(() => source().fail());
    act(() => {
      vi.advanceTimersByTime(3_999);
    });
    expect(FakeEventSource.all).toHaveLength(2);
    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(FakeEventSource.all).toHaveLength(3);
  });

  it("refreshes My List when it changed on another device", () => {
    const { client, source } = setup();
    client.setQueryData(keys.watchlist, { items: [] });
    client.setQueryData(keys.homeHubs, { hubs: [] });
    client.setQueryData(keys.item(5), { id: 5 });

    act(() => source().send({ type: "watchlist_changed" }));
    expect(invalidated(client, keys.watchlist)).toBe(true);
    expect(invalidated(client, keys.homeHubs)).toBe(true);
    expect(invalidated(client, keys.item(5))).toBe(true);
  });

  it("follows the devices to cast to, and what they play", () => {
    const { client, source } = setup();
    const tv: CastTarget = { id: "tv-1", name: "Living room TV", platform: "android_tv", state: null };
    client.setQueryData<CastTarget[]>(castKeys.devices, [tv]);
    const state: CastState = { item: makeItem({ id: 7 }), position_ms: 1_000, duration_ms: null, state: "playing", age_ms: 0 };

    act(() => source().send({ type: "cast_state", device_id: "tv-1", state }));
    expect(client.getQueryData<CastTarget[]>(castKeys.devices)?.[0]?.state).toEqual({ ...state, heard_at: Date.now() });
    expect(invalidated(client, castKeys.devices)).toBe(false);

    act(() => source().send({ type: "cast_devices_changed" }));
    expect(invalidated(client, castKeys.devices)).toBe(true);
  });

  it("says, in the desktop app, that it may be cast to", () => {
    (window as unknown as { __PLEY_APP__?: string }).__PLEY_APP__ = "desktop";
    try {
      const { source } = setup();
      const url = new URL(source().url, "http://pley.test");
      expect(url.pathname).toBe("/api/events");
      expect(url.searchParams.get("cast_platform")).toBe("desktop");
      expect(url.searchParams.get("cast_device")).toMatch(/^[0-9a-f]{32}$/);
    } finally {
      delete (window as unknown as { __PLEY_APP__?: string }).__PLEY_APP__;
    }
  });

  it("refreshes Home and the items when watch state changed, after a moment", () => {
    const { client, source } = setup();
    client.setQueryData(keys.homeHubs, { hubs: [] });
    client.setQueryData(keys.libraryHubs(1), { hubs: [] });
    client.setQueryData(keys.watchlist, { items: [] });
    client.setQueryData(keys.item(5), { id: 5 });
    client.setQueryData(keys.children(4), { items: [] });
    client.setQueryData(keys.item(9), { id: 9 });

    act(() => source().send({ type: "user_state_changed", item_ids: [5, 4, 3], children: false }));
    act(() => {
      vi.advanceTimersByTime(299);
    });
    expect(invalidated(client, keys.homeHubs)).toBe(false);

    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(invalidated(client, keys.homeHubs)).toBe(true);
    expect(invalidated(client, keys.libraryHubs(1))).toBe(true);
    expect(invalidated(client, keys.watchlist)).toBe(true);
    expect(invalidated(client, keys.item(5))).toBe(true);
    // The season's episodes.
    expect(invalidated(client, keys.children(4))).toBe(true);
    expect(invalidated(client, keys.item(9))).toBe(false);
  });

  it("refreshes every item when what is under one changed", () => {
    const { client, source } = setup();
    client.setQueryData(keys.item(9), { id: 9 });

    act(() => source().send({ type: "user_state_changed", item_ids: [3], children: true }));
    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(invalidated(client, keys.item(9))).toBe(true);
  });

  it("leaves alone what was fetched since the change was heard", () => {
    const { client, source } = setup();
    client.setQueryData(keys.homeHubs, { hubs: [] });
    client.setQueryData(keys.item(5), { id: 5 });

    act(() => source().send({ type: "user_state_changed", item_ids: [5], children: false }));
    act(() => {
      vi.advanceTimersByTime(100);
    });
    // This page made the change and fetched Home again already.
    client.setQueryData(keys.homeHubs, { hubs: [] });
    act(() => {
      vi.advanceTimersByTime(200);
    });
    expect(invalidated(client, keys.homeHubs)).toBe(false);
    expect(invalidated(client, keys.item(5))).toBe(true);
  });

  it("refreshes a library change heard with a watch state change once, at once", () => {
    const { client, source } = setup();
    client.setQueryData(keys.library(1), library(1));
    client.setQueryData(keys.homeHubs, { hubs: [] });

    // A play that ends: its watch state, and its library's change.
    act(() => {
      source().send({ type: "user_state_changed", item_ids: [5], children: false });
      source().send({ type: "library_changed", library_id: 1 });
    });
    act(() => {
      vi.advanceTimersByTime(300);
    });
    expect(invalidated(client, keys.library(1))).toBe(true);
    expect(invalidated(client, keys.homeHubs)).toBe(true);

    // Fetched again: the library's flush doesn't come a second time.
    client.setQueryData(keys.library(1), library(1));
    client.setQueryData(keys.homeHubs, { hubs: [] });
    act(() => {
      vi.advanceTimersByTime(1500);
    });
    expect(invalidated(client, keys.library(1))).toBe(false);
    expect(invalidated(client, keys.homeHubs)).toBe(false);
  });

  describe("requests", () => {
    const list = requestKeys.list(EVERY_REQUEST, 50);
    const row = (titles: DiscoverTitle[]): InfiniteData<DiscoverPage> => ({
      pages: [{ page: 1, total_pages: 1, results: titles }],
      pageParams: [1],
    });
    const rowTitle = (client: QueryClient, tmdbId: number) =>
      client.getQueryData<InfiniteData<DiscoverPage>>(requestKeys.discover("trending"))?.pages[0]?.results.find((t) => t.tmdb_id === tmdbId);

    it("refreshes the count once a burst is over, and what shows the request a moment after", () => {
      mockApi({});
      const { client, source } = setup();
      client.setQueryData(requestKeys.summary, makeSummary());
      client.setQueryData(requestKeys.title("movie", 603), makeDetail());
      client.setQueryData(requestKeys.title("movie", 604), makeDetail({ tmdb_id: 604 }));
      client.setQueryData(list, { pages: [makeRequestList([])], pageParams: [0] });
      client.setQueryData(requestKeys.discover("trending"), row([makeTitle({ tmdb_id: 604 })]));

      act(() => {
        for (let i = 0; i < 5; i++) source().send({ type: "requests_changed", request_id: 7, kind: "movie", tmdb_id: 603 });
      });
      act(() => {
        vi.advanceTimersByTime(300);
      });
      expect(invalidated(client, requestKeys.title("movie", 603))).toBe(true);
      expect(invalidated(client, list)).toBe(true);
      expect(invalidated(client, requestKeys.title("movie", 604))).toBe(false);
      // TMDB's rows are never fetched again for it.
      expect(invalidated(client, requestKeys.discover("trending"))).toBe(false);
      expect(invalidated(client, requestKeys.summary)).toBe(false);

      act(() => {
        vi.advanceTimersByTime(700);
      });
      expect(invalidated(client, requestKeys.summary)).toBe(true);
    });

    it("changes the badge of a title shown in a row from its request as it is now", async () => {
      const requests = mockApi({
        "GET /api/requests/7": () => makeRequest({ id: 7, status: "declined", mine: true }),
        "GET /api/requests/8": () => apiError(404, "not_found", "no such request"),
        "GET /api/discover/movie/604": () => makeDetail({ tmdb_id: 604 }),
      });
      const { client, source } = setup();
      client.setQueryData(
        requestKeys.discover("trending"),
        row([makeTitle({ tmdb_id: 603, status: "requested", request_id: 7 }), makeTitle({ tmdb_id: 604, status: "requested", request_id: 8 })]),
      );

      act(() => {
        source().send({ type: "requests_changed", request_id: 7, kind: "movie", tmdb_id: 603 });
        source().send({ type: "requests_changed", request_id: 8, kind: "movie", tmdb_id: 604 });
        // Not shown: nothing to ask about.
        source().send({ type: "requests_changed", request_id: 9, kind: "movie", tmdb_id: 605 });
      });
      await act(async () => {
        await vi.advanceTimersByTimeAsync(300);
      });
      await vi.waitFor(() => expect(rowTitle(client, 603)?.status).toBe("declined"));
      // One it no longer sees: its title's details say how it stands.
      await vi.waitFor(() => expect(rowTitle(client, 604)).toMatchObject({ status: "none", request_id: null }));
      expect(requests.map((r) => r.path)).toEqual(["/api/requests/7", "/api/requests/8", "/api/discover/movie/604"]);
    });

    it("keeps a title requested when a request it no longer follows stands for others", async () => {
      mockApi({
        "GET /api/requests/8": () => apiError(404, "not_found", "no such request"),
        "GET /api/discover/movie/604": () => makeDetail({ tmdb_id: 604, status: "requested", request_id: null }),
      });
      const { client, source } = setup();
      client.setQueryData(requestKeys.discover("trending"), row([makeTitle({ tmdb_id: 604, status: "requested", request_id: 8 })]));
      act(() => source().send({ type: "requests_changed", request_id: 8, kind: "movie", tmdb_id: 604 }));
      await act(async () => {
        await vi.advanceTimersByTimeAsync(300);
      });
      await vi.waitFor(() => expect(rowTitle(client, 604)).toMatchObject({ status: "requested", request_id: null }));
    });

    it("puts a show whose request is ready in the library only where all of it is", async () => {
      const ready = makeRequest({ id: 7, kind: "show", tmdb_id: 1399, seasons: [1], status: "available", stage: "available", item_id: 12 });
      const requests = mockApi({
        "GET /api/requests/7": () => ready,
        // Season 2 aired, and no one asked for it.
        "GET /api/discover/show/1399": () => makeShowDetail({ status: "partial", item_id: 12 }),
      });
      const { client, source } = setup();
      const show = makeTitle({ kind: "show", tmdb_id: 1399, status: "requested", request_id: 7 });
      client.setQueryData(requestKeys.discover("trending"), row([show]));
      act(() =>
        source().send({
          type: "request_available",
          request_id: 7,
          title: "Dark",
          kind: "show",
          tmdb_id: 1399,
          item_id: 12,
          poster_url: null,
          seasons: [1],
          complete: true,
        }),
      );
      expect(rowTitle(client, 1399)?.status).toBe("requested");
      await act(async () => {
        await vi.advanceTimersByTimeAsync(300);
      });
      await vi.waitFor(() => expect(requests.map((r) => r.path)).toContain("/api/discover/show/1399"));
      await vi.waitFor(() => expect(rowTitle(client, 1399)).toMatchObject({ status: "partial", item_id: 12, request_id: null }));
    });

    it("puts a title that became available in the library at once", () => {
      mockApi({});
      const { client, source } = setup();
      client.setQueryData(requestKeys.discover("trending"), row([makeTitle({ tmdb_id: 603, status: "requested", request_id: 7 })]));
      act(() =>
        source().send({
          type: "request_available",
          request_id: 7,
          title: "The Matrix",
          kind: "movie",
          tmdb_id: 603,
          item_id: 12,
          poster_url: null,
          seasons: null,
          complete: true,
        }),
      );
      expect(rowTitle(client, 603)).toMatchObject({ status: "library", item_id: 12 });
    });

    it("refreshes every list and page when many changed, and Discover's rows only when next shown", () => {
      const { client, source } = setup();
      client.setQueryData(requestKeys.title("movie", 603), makeDetail());
      client.setQueryData(requestKeys.discover("trending"), row([makeTitle()]));
      act(() => source().send({ type: "requests_changed", request_id: null, kind: null, tmdb_id: null }));
      act(() => {
        vi.advanceTimersByTime(300);
      });
      expect(invalidated(client, requestKeys.title("movie", 603))).toBe(true);
      expect(invalidated(client, requestKeys.discover("trending"))).toBe(true);
    });

    it("refreshes what each page offers when the settings changed", () => {
      const { client, source } = setup();
      client.setQueryData(requestKeys.summary, makeSummary());
      client.setQueryData(requestKeys.title("movie", 603), makeDetail());
      client.setQueryData(requestKeys.options, { radarr: null, sonarr: null });
      act(() => source().send({ type: "requests_settings_changed" }));
      expect(invalidated(client, requestKeys.title("movie", 603))).toBe(true);
      expect(invalidated(client, requestKeys.options)).toBe(true);
      act(() => {
        vi.advanceTimersByTime(1000);
      });
      expect(invalidated(client, requestKeys.summary)).toBe(true);
    });

    it("refreshes requests after a reconnect, leaving Discover's rows for when they're next shown", () => {
      const { client, source } = setup();
      act(() => source().open());
      client.setQueryData(requestKeys.summary, makeSummary());
      client.setQueryData(requestKeys.discover("trending"), row([makeTitle()]));
      act(() => source().open());
      expect(invalidated(client, requestKeys.summary)).toBe(true);
      expect(invalidated(client, requestKeys.discover("trending"))).toBe(true);
    });

    it("says whether the stream is open, so what shows requests knows to ask again by itself", () => {
      const { client, source } = setup();
      client.setQueryData(requestKeys.summary, makeSummary());
      act(() => source().open());
      expect(eventsLive()).toBe(true);
      act(() => source().fail());
      expect(eventsLive()).toBe(false);
      expect(invalidated(client, requestKeys.summary)).toBe(true);
    });
  });

  it("passes events on to whoever listens", () => {
    const { source } = setup();
    const heard: ServerEvent[] = [];
    const stop = onServerEvent((ev) => heard.push(ev));
    const stopped: ServerEvent = { type: "session_stopped", session_id: "s1", message: "Bye", by_admin: true };

    act(() => source().send(stopped));
    stop();
    act(() => source().send({ type: "sessions_changed" }));
    expect(heard).toEqual([stopped]);
  });

  it("ignores events it can't read", () => {
    const { client, source } = setup();
    client.setQueryData(keys.libraries, [library(1)]);
    act(() => source().send("{not json"));
    act(() => source().send(JSON.stringify({ type: "something_new" })));
    expect(client.getQueryData<Library[]>(keys.libraries)).toEqual([library(1)]);
  });
});

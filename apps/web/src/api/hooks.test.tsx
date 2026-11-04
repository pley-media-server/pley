import type { ReactNode } from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { makeItem } from "@/test/fixtures";
import { apiError, mockApi, testQueryClient } from "@/test/render";
import { FIRST_PAGE, keys, sessionCommandError, useLibraryItems, useSessionCommand } from "./hooks";
import type { PlayingSession, SortOrder } from "./types";

const items = Array.from({ length: FIRST_PAGE + 2 }, (_, i) => makeItem({ id: i + 1 }));
const ascending = items.map((i) => i.id);
const descending = [...ascending].reverse();

/** Serves the library in pages, in either order; `delay` holds back the second half. */
function serveLibrary(list = items) {
  return mockApi({
    "GET /api/libraries/1/items": (req) => {
      const params = new URL(req.path, "http://pley.test").searchParams;
      const ordered = params.get("order") === "desc" ? [...list].reverse() : list;
      const offset = Number(params.get("offset") ?? 0);
      const limit = Number(params.get("limit"));
      return { total: ordered.length, items: ordered.slice(offset, offset + limit) };
    },
  });
}

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={testQueryClient()}>{children}</QueryClientProvider>;
}

function clientWrapper(client = testQueryClient()) {
  return {
    client,
    wrapper: ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>,
  };
}

describe("useLibraryItems", () => {
  it("loads the first page on its own, then the rest", async () => {
    const requests = serveLibrary();
    const { result } = renderHook(() => useLibraryItems(1, { sort: "title", order: "asc" }), { wrapper });

    await waitFor(() => expect(result.current.data?.items).toHaveLength(items.length));
    expect(result.current.data?.items.map((i) => i.id)).toEqual(ascending);
    expect(requests.map((r) => r.path)).toEqual([
      `/api/libraries/1/items?sort=title&order=asc&limit=${FIRST_PAGE}`,
      `/api/libraries/1/items?sort=title&order=asc&offset=${FIRST_PAGE}&limit=100000`,
    ]);
  });

  it("makes one request for small libraries", async () => {
    const requests = serveLibrary(items.slice(0, 10));
    const { result } = renderHook(() => useLibraryItems(1, {}), { wrapper });

    await waitFor(() => expect(result.current.data?.items).toHaveLength(10));
    expect(requests).toHaveLength(1);
    expect(result.current.isPending).toBe(false);
  });

  it("never mixes the new order with the old one while re-sorting", async () => {
    serveLibrary();
    const seen: number[][] = [];
    const { result, rerender } = renderHook(
      ({ order }: { order: SortOrder }) => {
        const state = useLibraryItems(1, { sort: "title", order });
        if (state.data) seen.push(state.data.items.map((i) => i.id));
        return state;
      },
      { wrapper, initialProps: { order: "asc" } },
    );
    await waitFor(() => expect(result.current.data?.items).toHaveLength(items.length));

    rerender({ order: "desc" });
    await waitFor(() => expect(result.current.data?.items.map((i) => i.id)).toEqual(descending));

    // Every list shown is the start of one order or the other.
    for (const ids of seen) {
      const start = (order: number[]) => order.slice(0, ids.length);
      expect([start(ascending), start(descending)]).toContainEqual(ids);
    }
  });

  it("refetches both halves", async () => {
    const requests = serveLibrary();
    const { result } = renderHook(() => useLibraryItems(1, {}), { wrapper });
    await waitFor(() => expect(result.current.data?.items).toHaveLength(items.length));

    await act(() => result.current.refetch());
    expect(requests).toHaveLength(4);
  });
});

describe("useSessionCommand", () => {
  const tv: PlayingSession = {
    session_id: "s 1",
    kind: "item",
    device: "Living room TV",
    item: makeItem({ id: 7 }),
    channel: null,
    state: "playing",
    position_ms: 60_000,
    duration_ms: 6_000_000,
    updated_at: 1_700_000_000,
    controls: ["pause", "seek"],
  };

  it("sends the command with what it needs", async () => {
    const requests = mockApi({ "POST /api/sessions/s%201/command": () => undefined });
    const { result } = renderHook(() => useSessionCommand(), { wrapper });

    await act(() => result.current.mutateAsync({ sessionId: "s 1", command: "seek", positionMs: 90_000 }));
    await act(() => result.current.mutateAsync({ sessionId: "s 1", command: "pause" }));
    expect(requests.map((r) => r.body)).toEqual([{ command: "seek", position_ms: 90_000 }, { command: "pause" }]);
  });

  it("drops a session whose player is gone, and asks again what one obeys when it no longer takes a command", async () => {
    mockApi({
      "POST /api/sessions/s%201/command": () => apiError(404, "not_found", "no such session"),
      "POST /api/sessions/s2/command": () => apiError(409, "not_controllable", "this player doesn't take that command"),
      "GET /api/sessions": () => [tv],
    });
    const { client, wrapper } = clientWrapper();
    client.setQueryData(keys.sessions, [tv, { ...tv, session_id: "s2" }]);
    const { result } = renderHook(() => useSessionCommand(), { wrapper });

    await act(() => result.current.mutateAsync({ sessionId: "s 1", command: "pause" }).catch(() => undefined));
    expect(client.getQueryData<PlayingSession[]>(keys.sessions)?.map((s) => s.session_id)).toEqual(["s2"]);

    client.setQueryData(keys.sessions, [{ ...tv, session_id: "s2" }]);
    await act(() => result.current.mutateAsync({ sessionId: "s2", command: "pause" }).catch(() => undefined));
    expect(client.getQueryState(keys.sessions)?.isInvalidated).toBe(true);
  });

  it("says why a command wasn't taken", async () => {
    const answer = async (status: number, code: string) => {
      mockApi({ "POST /api/sessions/s1/command": () => apiError(status, code, "the server's words") });
      const { result } = renderHook(() => useSessionCommand(), { wrapper });
      let err: unknown;
      await act(() => result.current.mutateAsync({ sessionId: "s1", command: "next" }).catch((e: unknown) => (err = e)));
      return err;
    };
    expect(sessionCommandError(await answer(404, "not_found"), "Living room TV")).toBe("Living room TV isn't playing any more");
    expect(sessionCommandError(await answer(409, "not_controllable"), null)).toBe("This device can't do that");
    expect(sessionCommandError(await answer(429, "too_many_requests"), "TV")).toBe("Too many at once: try again in a moment");
    expect(sessionCommandError(await answer(400, "bad_request"), "TV")).toBe("The server's words");
  });
});

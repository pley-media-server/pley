import type { ReactNode } from "react";
import { act, renderHook, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import type { ItemSummary } from "@/api/types";
import { makeItem } from "@/test/fixtures";
import { LocationDisplay, mockApi, TestProviders, testQueryClient } from "@/test/render";
import { loadQueue, nextInQueue, queuePlayUrl, useVideoQueue } from "./videoQueue";
import { encodeId } from "./ids";

function wrapper({ children }: { children: ReactNode }) {
  return (
    <TestProviders client={testQueryClient()}>
      {children}
      <LocationDisplay />
    </TestProviders>
  );
}

const playing = (position_ms: number, watched = false) => ({ position_ms, watched, play_count: watched ? 1 : 0 });

describe("queue storage", () => {
  it("loads saved queues", () => {
    window.sessionStorage.setItem("pley.queue.k1", JSON.stringify({ ids: [1, 2], returnTo: "/playlists/3" }));
    expect(loadQueue("k1")).toEqual({ key: "k1", ids: [1, 2], returnTo: "/playlists/3" });
  });

  it("ignores missing and damaged queues", () => {
    window.sessionStorage.setItem("pley.queue.bad", "{");
    window.sessionStorage.setItem("pley.queue.odd", JSON.stringify({ ids: "1,2" }));
    expect(loadQueue(null)).toBeNull();
    expect(loadQueue("missing")).toBeNull();
    expect(loadQueue("bad")).toBeNull();
    expect(loadQueue("odd")).toBeNull();
  });

  it("names the queue and position in player urls", () => {
    const queue = { key: "k 1", ids: [4, 5] };
    expect(queuePlayUrl(queue, 1, 5)).toBe(`/play/${encodeId(5)}?queue=k%201&qi=1`);
    expect(queuePlayUrl(queue, 0, 4, 90_000)).toBe(`/play/${encodeId(4)}?t=90000&queue=k%201&qi=0`);
  });
});

describe("nextInQueue", () => {
  it("fetches the next entry fresh, until the end", async () => {
    const next = makeItem({ id: 5, title: "Next" });
    mockApi({ "GET /api/items/5": () => next });
    const qc = testQueryClient();
    const queue = { key: "k", ids: [4, 5] };

    await expect(nextInQueue(qc, { queue, index: 0 })).resolves.toEqual(next);
    await expect(nextInQueue(qc, { queue, index: 1 })).resolves.toBeNull();
  });
});

describe("useVideoQueue", () => {
  const show = makeItem({ id: 1, kind: "show" });
  const watchedMovie = makeItem({ id: 2, user_state: playing(5_000, true) });
  const started = makeItem({ id: 3, kind: "episode", user_state: playing(60_000) });

  function start(items: ItemSummary[], opts?: Parameters<ReturnType<typeof useVideoQueue>>[1]) {
    const { result } = renderHook(() => useVideoQueue(), { wrapper });
    let played = false;
    act(() => {
      played = result.current(items, opts);
    });
    return played;
  }

  it("plays the videos from the chosen one, resuming where it stopped", () => {
    expect(start([show, started, watchedMovie], { startIndex: 1, returnTo: "/collection/9" })).toBe(true);

    const location = new URL(screen.getByTestId("location").textContent ?? "", "http://pley.test");
    expect(location.pathname).toBe(`/play/${encodeId(3)}`);
    expect(location.searchParams.get("t")).toBe("60000");
    expect(location.searchParams.get("qi")).toBe("0");
    expect(loadQueue(location.searchParams.get("queue"))).toMatchObject({ ids: [3, 2], returnTo: "/collection/9" });
  });

  it("starts watched videos over", () => {
    start([watchedMovie, started]);
    expect(screen.getByTestId("location").textContent?.startsWith(`/play/${encodeId(2)}?queue=`)).toBe(true);
  });

  it("keeps one queue per tab", () => {
    start([started]);
    start([watchedMovie]);
    const storage = window.sessionStorage;
    const keys = Array.from({ length: storage.length }, (_, i) => storage.key(i));
    expect(keys.filter((k) => k?.startsWith("pley.queue."))).toHaveLength(1);
  });

  it("says when there is nothing to play", () => {
    expect(start([show])).toBe(false);
    expect(screen.getByRole("status")).toHaveTextContent("Nothing to play");
    expect(screen.getByTestId("location").textContent).toBe("/");
  });
});

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { bufferedAhead, keepCushion } from "./cushion";

/** A video element whose buffered seconds and current time the test sets. */
function fakeVideo() {
  const video = document.createElement("video");
  let ranges: [number, number][] = [];
  Object.defineProperty(video, "buffered", {
    get: () => ({ length: ranges.length, start: (i: number) => ranges[i]![0], end: (i: number) => ranges[i]![1] }),
  });
  let now = 0;
  Object.defineProperty(video, "currentTime", { get: () => now, set: (t: number) => (now = t) });
  return {
    video,
    buffer: (...r: [number, number][]) => (ranges = r),
    at: (t: number) => (now = t),
  };
}

describe("bufferedAhead", () => {
  it("counts the range that plays, not one past a gap", () => {
    const { video, buffer, at } = fakeVideo();
    expect(bufferedAhead(video)).toBe(0);
    buffer([0, 5], [8, 20]);
    at(2);
    expect(bufferedAhead(video)).toBe(3);
    at(6);
    expect(bufferedAhead(video)).toBe(0);
  });
});

describe("keepCushion", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("starts once 2 s are buffered", () => {
    const { video, buffer } = fakeVideo();
    const play = vi.fn();
    const stop = keepCushion(video, play);
    buffer([0, 1]);
    vi.advanceTimersByTime(400);
    expect(play).not.toHaveBeenCalled();
    buffer([0, 2.1]);
    vi.advanceTimersByTime(200);
    expect(play).toHaveBeenCalledTimes(1);
    stop();
  });

  it("starts anyway after a while, for a channel sent slower than it plays", () => {
    const { video, buffer } = fakeVideo();
    const play = vi.fn();
    const stop = keepCushion(video, play);
    buffer([0, 0.5]);
    vi.advanceTimersByTime(5800);
    expect(play).not.toHaveBeenCalled();
    vi.advanceTimersByTime(400);
    expect(play).toHaveBeenCalledTimes(1);
    stop();
  });

  it("holds after a stall until 4 s are buffered, and says so", () => {
    const { video, buffer, at } = fakeVideo();
    const holds: boolean[] = [];
    const stop = keepCushion(video, vi.fn(), (holding) => holds.push(holding));
    buffer([0, 3]);
    vi.advanceTimersByTime(200);

    at(3);
    video.dispatchEvent(new Event("waiting"));
    expect(video.playbackRate).toBe(0);
    expect(holds).toEqual([true]);
    // The browser could go on with a second: it waits for four.
    buffer([0, 4]);
    vi.advanceTimersByTime(400);
    expect(video.playbackRate).toBe(0);
    buffer([0, 7.2]);
    vi.advanceTimersByTime(200);
    expect(video.playbackRate).toBe(1);
    expect(holds).toEqual([true, false]);
    stop();
  });

  it("leaves the rate at 1 when stopped while holding", () => {
    const { video, buffer } = fakeVideo();
    const holds: boolean[] = [];
    const stop = keepCushion(video, vi.fn(), (holding) => holds.push(holding));
    buffer([0, 3]);
    vi.advanceTimersByTime(200);
    video.dispatchEvent(new Event("waiting"));
    stop();
    expect(video.playbackRate).toBe(1);
    expect(holds).toEqual([true, false]);
    // Stopped: a stall after it holds nothing.
    video.dispatchEvent(new Event("waiting"));
    expect(video.playbackRate).toBe(1);
    // Stopped again, it says nothing more.
    stop();
    expect(holds).toEqual([true, false]);
  });
});

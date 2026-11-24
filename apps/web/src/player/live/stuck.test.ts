import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { whenStuck } from "./stuck";

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

describe("whenStuck", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  /** It played up to 10 s, then stands still there, with 0.2 s more buffered. */
  function playedThenStill() {
    const fake = fakeVideo();
    fake.buffer([0, 3]);
    const onStuck = vi.fn();
    let coming = true;
    const stop = whenStuck(fake.video, () => coming, onStuck);
    fake.buffer([0, 10.2]);
    fake.at(10);
    vi.advanceTimersByTime(1000);
    return { ...fake, onStuck, stop, comes: (c: boolean) => (coming = c) };
  }

  it("tells once the picture stands still while the stream comes, and that it played", () => {
    const { onStuck, stop } = playedThenStill();
    vi.advanceTimersByTime(7000);
    expect(onStuck).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1000);
    expect(onStuck).toHaveBeenCalledExactlyOnceWith(true);
    // Told once: it stopped looking.
    vi.advanceTimersByTime(20_000);
    expect(onStuck).toHaveBeenCalledOnce();
    stop();
  });

  it("waits while the stream doesn't come: a provider gone quiet, which the cushion waits for", () => {
    const { onStuck, stop, comes } = playedThenStill();
    comes(false);
    vi.advanceTimersByTime(60_000);
    expect(onStuck).not.toHaveBeenCalled();
    // The provider sends again: what comes has its seconds to be buffered.
    comes(true);
    vi.advanceTimersByTime(4000);
    expect(onStuck).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1000);
    expect(onStuck).toHaveBeenCalledExactlyOnceWith(true);
    stop();
  });

  it("starts counting again whenever more is buffered", () => {
    const { onStuck, stop, buffer } = playedThenStill();
    vi.advanceTimersByTime(6000);
    // Holding for the cushion, the picture still: the stream is buffered as it comes.
    buffer([0, 11]);
    vi.advanceTimersByTime(6000);
    buffer([0, 12]);
    vi.advanceTimersByTime(7000);
    expect(onStuck).not.toHaveBeenCalled();
    vi.advanceTimersByTime(2000);
    expect(onStuck).toHaveBeenCalledOnce();
    stop();
  });

  it("tells a video that never played, where going back to 0 isn't playing", () => {
    const { video, buffer, at } = fakeVideo();
    at(75);
    buffer([0, 75.1]);
    const onStuck = vi.fn();
    // A new player: the video starts again at 0, and nothing comes of the stream.
    const stop = whenStuck(video, () => true, onStuck);
    at(0);
    buffer();
    vi.advanceTimersByTime(8000);
    expect(onStuck).toHaveBeenCalledExactlyOnceWith(false);
    stop();
  });

  it("tells nothing once stopped", () => {
    const { onStuck, stop } = playedThenStill();
    stop();
    vi.advanceTimersByTime(20_000);
    expect(onStuck).not.toHaveBeenCalled();
  });
});

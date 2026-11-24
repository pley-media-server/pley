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
  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  /** It played on to 10 s, then stands still there, with 0.2 s more buffered. */
  function playedThenStill() {
    const fake = fakeVideo();
    fake.buffer([0, 3]);
    const onStuck = vi.fn();
    let coming = true;
    const stop = whenStuck(fake.video, () => coming, onStuck);
    fake.buffer([0, 8]);
    fake.at(5);
    vi.advanceTimersByTime(1000);
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

  it("tells only while the stream comes: not for a stream that came, then ended", () => {
    const { onStuck, stop, comes } = playedThenStill();
    vi.advanceTimersByTime(6000);
    comes(false);
    vi.advanceTimersByTime(30_000);
    expect(onStuck).not.toHaveBeenCalled();
    comes(true);
    vi.advanceTimersByTime(1000);
    expect(onStuck).toHaveBeenCalledOnce();
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

  it("counts a look that came late, in a page in the background, as a second and a half at most", () => {
    let clock = 0;
    vi.spyOn(performance, "now").mockImplementation(() => clock);
    const { video, buffer } = fakeVideo();
    buffer([0, 3]);
    const onStuck = vi.fn();
    const stop = whenStuck(video, () => true, onStuck);
    // Every look a minute after the last.
    for (let look = 1; look <= 5; look++) {
      clock += 60_000;
      vi.advanceTimersByTime(1000);
    }
    expect(onStuck).not.toHaveBeenCalled();
    clock += 60_000;
    vi.advanceTimersByTime(1000);
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

  it("doesn't count one jump forward as playing: mpegts.js seeks to what's buffered as it starts", () => {
    const { video, buffer, at } = fakeVideo();
    buffer([0.4, 3]);
    const onStuck = vi.fn();
    const stop = whenStuck(video, () => true, onStuck);
    vi.advanceTimersByTime(1000);
    at(0.4);
    vi.advanceTimersByTime(9000);
    expect(onStuck).toHaveBeenCalledExactlyOnceWith(false);
    stop();
  });

  it("doesn't count a page in the background that hasn't played: its video may not load until it's shown", () => {
    const hidden = vi.spyOn(document, "hidden", "get").mockReturnValue(true);
    const { video, buffer } = fakeVideo();
    buffer();
    const onStuck = vi.fn();
    const stop = whenStuck(video, () => true, onStuck);
    vi.advanceTimersByTime(60_000);
    expect(onStuck).not.toHaveBeenCalled();
    // Shown, and still nothing.
    hidden.mockReturnValue(false);
    vi.advanceTimersByTime(8000);
    expect(onStuck).toHaveBeenCalledExactlyOnceWith(false);
    stop();
  });

  it("counts a page in the background that played: its picture stands still there too", () => {
    const { onStuck, stop } = playedThenStill();
    vi.spyOn(document, "hidden", "get").mockReturnValue(true);
    vi.advanceTimersByTime(8000);
    expect(onStuck).toHaveBeenCalledExactlyOnceWith(true);
    stop();
  });

  it("tells nothing once stopped", () => {
    const { onStuck, stop } = playedThenStill();
    stop();
    vi.advanceTimersByTime(20_000);
    expect(onStuck).not.toHaveBeenCalled();
  });
});

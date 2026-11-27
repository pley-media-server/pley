import { signal } from "@preact/signals";
import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ItemSummary } from "../../src/api/types";
import { t } from "../../src/i18n";
import type { NowPlaying } from "../../src/player/music";
import { SLEEP_OFF } from "../../src/player/sleep";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import { NowPlayingScreen } from "../../src/ui/screens/music";

/** Now Playing's keys: the scrubber first, Left and Right seeking there, Down to the controls. */

const track = (id: number): ItemSummary =>
  ({ id, library_id: 3, kind: "track", title: `Track ${id}`, sort_title: `track ${id}`, added_at: 0, duration_ms: 240_000, user_state: { position_ms: 0, watched: false, play_count: 0 } }) as ItemSummary;

const fake = {
  now: signal<NowPlaying | null>(null),
  sleep: signal(SLEEP_OFF),
  position: 60_000,
  duration: 240_000,
  seekTo: vi.fn((_ms: number) => true),
  toggle: vi.fn(),
  previous: vi.fn(),
  next: vi.fn(),
  pause: vi.fn(),
  setShuffle: vi.fn(),
  stepSleep: vi.fn(),
  jumpTo: vi.fn(),
  positionMs: () => fake.position,
  durationMs: () => fake.duration,
  bufferedMs: () => 0,
  list: () => [track(1), track(2), track(3)],
};

vi.mock("../../src/player/music", () => ({ musicPlayer: () => fake }));

let root: HTMLElement | null = null;
let detach: (() => void) | null = null;

function setup() {
  root = document.createElement("div");
  document.body.appendChild(root);
  const focus = new FocusManager();
  focus.setPage(root);
  const input = new Input(focus);
  detach = input.attach(document);
  const ui = {
    focus,
    input,
    nav: { open: () => undefined, pop: () => true },
    images: { url: (u: string) => u, ready: () => null, isMissing: () => false, is: () => false, load: () => () => undefined },
    toast: () => undefined,
    claim: (el: HTMLElement, _fid: string | null, initial: boolean) => {
      if (initial && !focus.current) focus.focus(el);
    },
    toRail: () => false,
    scale: 1,
    tier: "high",
    reducedMotion: false,
  } as unknown as Ui;
  act(() =>
    render(
      <UiContext.Provider value={ui}>
        <NowPlayingScreen />
      </UiContext.Provider>,
      root!,
    ),
  );
  const key = (k: "left" | "right" | "up" | "down" | "ok" | "fastForward" | "rewind") =>
    act(() => {
      input.dispatch(k, "down", false);
      input.dispatch(k, "up", false);
    });
  const focused = () => focus.current;
  const labelled = (label: string) => root!.querySelector<HTMLElement>(`[aria-label="${label}"]`);
  /** Up from Play to the scrubber. */
  const toScrubber = () => {
    key("up");
    expect(focused()?.classList.contains("scrub-box")).toBe(true);
  };
  return { key, focused, labelled, toScrubber };
}

beforeEach(() => {
  vi.useFakeTimers();
  fake.position = 60_000;
  fake.duration = 240_000;
  fake.now.value = { track: track(2), index: 1, count: 3, playing: true, loading: false, ended: false, shuffle: false, error: null };
  for (const f of [fake.seekTo, fake.toggle, fake.previous, fake.next]) f.mockClear();
});
afterEach(() => {
  detach?.();
  detach = null;
  if (root) act(() => render(null, root!));
  root?.remove();
  root = null;
  vi.useRealTimers();
});

describe("Now Playing's keys", () => {
  it("opens with Play focused, as on Android TV and the native Samsung app", () => {
    const { focused } = setup();
    expect(focused()?.classList.contains("play-big")).toBe(true);
  });

  it("seeks 10 s with Right and Left on the scrubber", () => {
    const { key, focused, toScrubber } = setup();
    toScrubber();
    key("right");
    act(() => void vi.advanceTimersByTime(300));
    expect(fake.seekTo).toHaveBeenLastCalledWith(70_000);
    key("left");
    act(() => void vi.advanceTimersByTime(300));
    expect(fake.seekTo).toHaveBeenLastCalledWith(50_000);
    expect(focused()?.classList.contains("scrub-box")).toBe(true);
  });

  it("seeks forward while the track's length isn't known yet, rather than to its start", () => {
    fake.duration = 0;
    const { key, toScrubber } = setup();
    toScrubber();
    key("right");
    act(() => void vi.advanceTimersByTime(300));
    expect(fake.seekTo).toHaveBeenLastCalledWith(70_000);
  });

  it("moves the place shown at once for each of quick Right presses, and seeks once, to the last, as they stop", () => {
    const { key, labelled, toScrubber } = setup();
    toScrubber();
    const times = () => labelled(t("browse.music.position"))!.parentElement!.querySelector(".scrub-times")!.textContent;
    for (let i = 0; i < 5; i++) {
      key("right");
      act(() => void vi.advanceTimersByTime(60));
    }
    expect(times()).toContain("1:50");
    expect(fake.seekTo).not.toHaveBeenCalled();
    act(() => void vi.advanceTimersByTime(200));
    expect(fake.seekTo).not.toHaveBeenCalled();
    act(() => void vi.advanceTimersByTime(60));
    expect(fake.seekTo).toHaveBeenCalledTimes(1);
    expect(fake.seekTo).toHaveBeenLastCalledWith(110_000);
  });

  it("seeks once, to the last place, for quick presses of the remote's fast-forward and rewind keys", () => {
    const { key } = setup();
    for (let i = 0; i < 5; i++) {
      key("fastForward");
      act(() => void vi.advanceTimersByTime(60));
    }
    key("rewind");
    expect(fake.seekTo).not.toHaveBeenCalled();
    act(() => void vi.advanceTimersByTime(300));
    expect(fake.seekTo).toHaveBeenCalledTimes(1);
    expect(fake.seekTo).toHaveBeenLastCalledWith(100_000);
  });

  it("drops a seek still to go when another track starts meanwhile: the place was the last track's", () => {
    const { key, toScrubber } = setup();
    toScrubber();
    key("right");
    act(() => {
      fake.now.value = { track: track(3), index: 2, count: 3, playing: true, loading: false, ended: false, shuffle: false, error: null };
      fake.position = 0;
    });
    act(() => void vi.advanceTimersByTime(300));
    expect(fake.seekTo).not.toHaveBeenCalled();
  });

  it("goes down to Play, and along the controls from there with Left and Right", () => {
    const { key, focused, labelled, toScrubber } = setup();
    toScrubber();
    key("down");
    expect(focused()?.classList.contains("play-big")).toBe(true);
    key("left");
    expect(focused()).toBe(labelled(t("browse.music.previous")));
    key("right");
    key("right");
    expect(focused()).toBe(labelled(t("browse.music.next")));
    expect(fake.seekTo).not.toHaveBeenCalled();
    key("ok");
    expect(fake.next).toHaveBeenCalledTimes(1);
  });
});

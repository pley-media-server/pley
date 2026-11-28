import { signal } from "@preact/signals";
import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../../src/i18n";
import type { PlaybackController, PlayerClock, PlayerState } from "../../src/player/controller";
import { SLEEP_OFF } from "../../src/player/sleep";
import type { AppState } from "../../src/state";
import { NOTHING_SHOWN } from "../../src/subs/presenter";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import { PlayerView } from "../../src/ui/player/player";
import { AppContext } from "../../src/ui/screens/common";

/**
 * Up Next's card over the player (the Android app's TvPlayerScreenTest):
 * Play now holds the countdown, "Play now (7)" with a sweep across the
 * button, and keeps its width as the count shortens. How full the sweep is
 * at a moment is the rule of test/player/nextup.test.ts.
 */

let root: HTMLElement | null = null;
let detach: (() => void) | null = null;

const AT_CREDITS: PlayerState = {
  phase: { kind: "playing" },
  itemId: 70,
  kind: "episode",
  title: "Show",
  subtitle: "S1 E1",
  audio: [],
  subtitles: [],
  quality: "Original",
  qualities: [],
  method: "Direct play",
  methodKind: "direct",
  note: null,
  canPlayOriginal: false,
  suggestLowerQuality: false,
  skipLabel: null,
  skipAlone: false,
  markers: [],
  chapters: [],
  trickplay: null,
  upNext: { itemId: 71, title: "Second", heading: "Up next · S1 · E2", secondsLeft: 10 },
  details: null,
  notice: null,
  remoteNotice: null,
  remoteNoticeId: 0,
  pausedByRemote: false,
  subtitleOffsetMs: 0,
  subtitleSync: false,
  speed: 1,
  speeds: [],
  sleep: SLEEP_OFF,
  stoppedByServer: false,
  reconnecting: false,
  firstFrameMs: null,
  stalls: 0,
  switching: false,
};

function setup(secondsLeft: number | null = 10, reducedMotion = false) {
  document.body.innerHTML = "";
  root = document.createElement("div");
  document.body.appendChild(root);
  const focus = new FocusManager();
  focus.setPage(root);
  const input = new Input(focus);
  detach = input.attach(document);
  const ui = {
    focus,
    input,
    nav: { pop: () => true },
    images: { url: (u: string) => u, load: () => () => undefined },
    scale: 1,
    reducedMotion,
    claim: () => undefined,
  } as unknown as Ui;
  const app = {
    settings: signal({ playbackDetails: false }),
    platform: { keepAwake: () => undefined, clock24: () => true },
  } as unknown as AppState;
  const state = signal<PlayerState>({ ...AT_CREDITS, upNext: { ...AT_CREDITS.upNext!, secondsLeft } });
  const controller = {
    state,
    clock: signal<PlayerClock>({ positionMs: 2_640_000, durationMs: 2_700_000, bufferedMs: 0 }),
    subs: signal(NOTHING_SHOWN),
    engine: null,
    wantsToPlay: true,
    livePositionMs: () => 2_640_000,
    seekTo: () => undefined,
    close: () => undefined,
  } as unknown as PlaybackController;
  act(() =>
    render(
      <AppContext.Provider value={app}>
        <UiContext.Provider value={ui}>
          <PlayerView controller={controller} host={{ current: null }} queued={false} />
        </UiContext.Provider>
      </AppContext.Provider>,
      root!,
    ),
  );
  const button = () => root!.querySelector<HTMLElement>('[data-fid="playnow"]')!;
  return {
    button,
    /** The controller's countdown reaches another second, or stops. */
    left: (seconds: number | null) => act(() => void (state.value = { ...state.value, upNext: { ...state.value.upNext!, secondsLeft: seconds } })),
    wait: (ms: number) => act(() => void vi.advanceTimersByTime(ms)),
    words: () => button().querySelector(".playnow-words")!.textContent,
    /** The words that hold the label's room, whatever it reads. */
    widest: () => button().querySelector(".playnow-widest")!.textContent,
    /** How far the sweep reaches across the button, from 0 to 1. */
    swept: () => Number(/^scaleX\(([\d.]+)\)$/.exec(button().querySelector<HTMLElement>(".playnow-sweep")!.style.transform)?.[1] ?? Number.NaN),
  };
}

beforeEach(() => {
  // The sweep is drawn each frame, by the page's clock.
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "setInterval", "clearInterval", "Date", "performance", "requestAnimationFrame", "cancelAnimationFrame"] });
});
afterEach(() => {
  detach?.();
  detach = null;
  if (root) act(() => render(null, root!));
  root = null;
  setLanguage("en");
  vi.useRealTimers();
});

describe("Up Next's Play now", () => {
  it("holds the countdown and keeps its width as the count shortens", () => {
    const t = setup();
    expect([t.words(), t.widest()]).toEqual(["Play now (10)", "Play now (10)"]);
    t.left(9);
    // Nothing is laid out here: the label is as wide as its widest words, which stay the first second's.
    expect([t.words(), t.widest()]).toEqual(["Play now (9)", "Play now (10)"]);
    const widest = t.button().querySelector(".playnow-widest")!;
    expect([widest.getAttribute("aria-hidden"), widest.parentElement, t.button().querySelector("svg circle")]).toEqual(["true", t.button().querySelector(".playnow-label"), null]);
    // No countdown (autoplay off): the words alone.
    t.left(null);
    expect([t.words(), t.widest(), t.swept()]).toEqual(["Play now", "Play now", 0]);
  });

  it("sweeps across the button as far as the countdown has gone, smoothly", () => {
    const t = setup();
    expect(t.swept()).toBe(0);
    t.wait(500);
    expect(t.swept()).toBeCloseTo(0.05, 2);
    t.wait(500);
    expect(t.swept()).toBeCloseTo(0.1, 2);
    t.left(9);
    t.wait(500);
    expect(t.swept()).toBeCloseTo(0.15, 2);
    // A second that doesn't come: the sweep rests at that second's end.
    t.wait(2_000);
    expect(t.swept()).toBe(0.2);
  });

  it("counts down as Next in 7 with reduced motion, its sweep a step each second", () => {
    const t = setup(7, true);
    expect([t.words(), t.widest(), t.swept()]).toEqual(["Next in 7", "Next in 10", 0.3]);
    t.wait(900);
    expect(t.swept()).toBe(0.3);
    t.left(6);
    expect([t.words(), t.swept()]).toEqual(["Next in 6", 0.4]);
  });

  it("reads whole in Spanish and French with reduced motion, where the count is all the button says", () => {
    for (const [language, words] of [["es", "Siguiente en 7"], ["fr", "Suivant dans 7"]] as const) {
      setLanguage(language);
      expect(setup(7, true).words()).toBe(words);
      detach?.();
      act(() => render(null, root!));
    }
  });

  it("is announced once, as the card shows: its heading and title, not Play now's seconds", () => {
    const t = setup();
    const said = root!.querySelector('.upnext-card [aria-live="polite"]')!;
    expect(said.textContent).toBe("Up next · S1 · E2Second");
    expect(said.contains(t.button())).toBe(false);
    // The button's name stays the same through the countdown.
    t.left(9);
    expect([t.button().getAttribute("aria-label"), t.button().closest("[aria-live]")]).toEqual(["Play now", null]);
  });
});

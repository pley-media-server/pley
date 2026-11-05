import { signal } from "@preact/signals";
import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PlaybackController, PlayerClock, PlayerState } from "../../src/player/controller";
import { SLEEP_OFF } from "../../src/player/sleep";
import type { AppState } from "../../src/state";
import { NOTHING_SHOWN } from "../../src/subs/presenter";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import type { Key } from "../../src/ui/keys";
import { PlayerView } from "../../src/ui/player/player";
import { AppContext } from "../../src/ui/screens/common";

/** The player's screen as the user's remote, on another device, drives it: a controller that does as the test says. */

let root: HTMLElement | null = null;
let detach: (() => void) | null = null;

const PLAYING: PlayerState = {
  phase: { kind: "playing" },
  itemId: 70,
  kind: "movie",
  title: "Film (2024)",
  subtitle: null,
  audio: [],
  subtitles: [],
  quality: "Original",
  qualities: [],
  method: "Direct play",
  note: null,
  canPlayOriginal: false,
  suggestLowerQuality: false,
  skipLabel: null,
  markers: [],
  chapters: [],
  trickplay: null,
  upNext: null,
  details: null,
  notice: null,
  remoteNotice: null,
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

function setup() {
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
    reducedMotion: false,
    claim: () => undefined,
  } as unknown as Ui;
  const app = {
    settings: signal({ playbackDetails: false }),
    platform: { keepAwake: () => undefined, clock24: () => true },
  } as unknown as AppState;
  const state = signal<PlayerState>(PLAYING);
  const set = (patch: Partial<PlayerState>) => (state.value = { ...state.value, ...patch });
  let wantPlay = true;
  const controller = {
    state,
    clock: signal<PlayerClock>({ positionMs: 600_000, durationMs: 7_200_000, bufferedMs: 0 }),
    subs: signal(NOTHING_SHOWN),
    engine: null,
    get wantsToPlay() {
      return wantPlay;
    },
    // As the controller does: playing again ends the remote's pause; the TV's own pause isn't the remote's.
    play: () => {
      wantPlay = true;
      set({ phase: { kind: "playing" }, pausedByRemote: false });
    },
    pause: () => {
      wantPlay = false;
      set({ phase: { kind: "paused" } });
    },
    livePositionMs: () => 600_000,
    seekTo: () => undefined,
    close: () => undefined,
    dismissNotice: () => set({ notice: null }),
    dismissRemoteNotice: () => set({ remoteNotice: null }),
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
  const remote = {
    pause: () =>
      act(() => {
        wantPlay = false;
        set({ phase: { kind: "paused" }, pausedByRemote: true, remoteNotice: "Paused" });
      }),
    play: () => act(() => controller.play()),
    said: (text: string) => act(() => void set({ remoteNotice: text })),
  };
  const press = (key: Key) => {
    act(() => input.dispatch(key, "down", false));
    act(() => input.dispatch(key, "up", false));
  };
  const wait = (ms: number) => act(() => void vi.advanceTimersByTime(ms));
  const controlsShown = () => !!root!.querySelector(".player-controls.in");
  const toast = () => root!.querySelector(".toast")?.textContent ?? null;
  return { root: root!, remote, press, wait, controlsShown, toast };
}

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  detach?.();
  detach = null;
  if (root) act(() => render(null, root!));
  root = null;
  vi.useRealTimers();
});

describe("the player, driven by the user's remote on another device", () => {
  it("says the remote's pause for 2 s, and doesn't bring up the controls", () => {
    const t = setup();
    t.remote.pause();
    t.wait(100);
    expect(t.toast()).toBe("Paused");
    expect(t.controlsShown()).toBe(false);
    t.wait(1_850);
    expect(t.toast()).toBe("Paused");
    // Gone after 2 s, once it has faded.
    t.wait(200);
    t.wait(200);
    expect(t.toast()).toBeNull();
    expect(t.controlsShown()).toBe(false);
  });

  it("says what else it did the same way, over the controls the person has up, which stay", () => {
    const t = setup();
    t.press("up");
    expect(t.controlsShown()).toBe(true);
    t.remote.said("+30 s");
    t.wait(100);
    expect(t.toast()).toBe("+30 s");
    expect(t.root.querySelector(".toast.low")).not.toBeNull();
    expect(t.controlsShown()).toBe(true);
  });

  it("brings up the controls for the TV's own pause, which keeps them, as before", () => {
    const t = setup();
    t.press("ok");
    expect(t.controlsShown()).toBe(true);
    t.wait(10_000);
    expect(t.controlsShown()).toBe(true);
  });

  it("hides the controls up as the remote pauses 5 s after the last key, as while it plays", () => {
    const t = setup();
    t.press("up");
    t.wait(1_000);
    t.remote.pause();
    t.wait(3_500);
    expect(t.controlsShown()).toBe(true);
    t.wait(1_000);
    expect(t.controlsShown()).toBe(false);
  });

  it("makes the remote's pause the person's with a key here: the controls come up for it and stay", () => {
    const t = setup();
    t.remote.pause();
    t.wait(3_000);
    t.press("up");
    expect(t.controlsShown()).toBe(true);
    t.wait(10_000);
    expect(t.controlsShown()).toBe(true);
    // The TV's own pause key, after the remote's, brings them up as ever.
    t.press("back");
    expect(t.controlsShown()).toBe(false);
    t.press("pause");
    expect(t.controlsShown()).toBe(true);
    t.wait(10_000);
    expect(t.controlsShown()).toBe(true);
  });

  it("hides the controls again by themselves once it plays, whoever plays it", () => {
    const t = setup();
    t.press("up");
    t.remote.pause();
    t.wait(1_000);
    t.press("up");
    t.remote.play();
    t.wait(4_500);
    expect(t.controlsShown()).toBe(true);
    t.wait(1_000);
    expect(t.controlsShown()).toBe(false);
  });
});

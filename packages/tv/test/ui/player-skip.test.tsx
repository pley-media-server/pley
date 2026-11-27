import { signal } from "@preact/signals";
import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PlaybackController, PlayerClock, PlayerState } from "../../src/player/controller";
import type { AppState } from "../../src/state";
import { NOTHING_SHOWN } from "../../src/subs/presenter";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import type { Key } from "../../src/ui/keys";
import { PlayerView } from "../../src/ui/player/player";
import { AppContext } from "../../src/ui/screens/common";

/**
 * Skip Intro over the player: on its own (with focus, so OK skips) while the
 * controller says its first 8 s last, then only with the controls, where Up
 * from the scrubber reaches it. The controller's side is in
 * test/player/negotiation.test.ts, the rule's in test/player/markers.test.ts.
 */

let root: HTMLElement | null = null;
let detach: (() => void) | null = null;

const IN_INTRO: PlayerState = {
  phase: { kind: "playing" },
  itemId: 70,
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
  skipLabel: "Skip Intro",
  skipAlone: true,
  markers: [],
  chapters: [],
  trickplay: null,
  upNext: null,
  details: null,
  notice: null,
  remoteNotice: null,
  remoteNoticeId: 0,
  pausedByRemote: false,
  subtitleOffsetMs: 0,
  subtitleSync: false,
  speed: 1,
  speeds: [],
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
  const state = signal<PlayerState>(IN_INTRO);
  const set = (patch: Partial<PlayerState>) => (state.value = { ...state.value, ...patch });
  let wantPlay = true;
  const did: string[] = [];
  const controller = {
    state,
    clock: signal<PlayerClock>({ positionMs: 90_000, durationMs: 2_700_000, bufferedMs: 0 }),
    subs: signal(NOTHING_SHOWN),
    engine: null,
    get wantsToPlay() {
      return wantPlay;
    },
    play: () => {
      wantPlay = true;
      did.push("play");
      set({ phase: { kind: "playing" } });
    },
    pause: () => {
      wantPlay = false;
      did.push("pause");
      set({ phase: { kind: "paused" } });
    },
    skip: () => {
      did.push("skip");
      // The controller's next tick finds the playhead past the marker.
      setTimeout(() => set({ skipLabel: null, skipAlone: false }), 500);
    },
    livePositionMs: () => 90_000,
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
  const wait = (ms: number) => act(() => void vi.advanceTimersByTime(ms));
  const press = (key: Key) => {
    act(() => input.dispatch(key, "down", false));
    act(() => input.dispatch(key, "up", false));
    // Focus moves on the next turn.
    wait(1);
  };
  return {
    did,
    press,
    wait,
    /** The 8 s on its own are over (the controller's tick says so). */
    aloneOver: () => act(() => void set({ skipAlone: false })),
    /** Entering a marker again: on its own once more. */
    alone: () => act(() => void set({ skipAlone: true })),
    skipShown: () => !!root!.querySelector('[data-fid="skip"]'),
    controlsShown: () => !!root!.querySelector(".player-controls.in"),
    focused: () => focus.current?.getAttribute("data-fid") ?? null,
  };
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

describe("Skip over the player", () => {
  it("shows on its own with focus, so OK skips", () => {
    const t = setup();
    t.wait(1);
    expect([t.skipShown(), t.controlsShown(), t.focused()]).toEqual([true, false, "skip"]);
    t.press("ok");
    expect(t.did).toEqual(["skip"]);
  });

  it("goes after its 8 s and gives focus back to the video: OK pauses and brings up the controls", () => {
    const t = setup();
    t.wait(1);
    t.aloneOver();
    t.wait(1);
    expect([t.skipShown(), t.focused()]).toEqual([false, "video"]);
    t.press("ok");
    expect(t.did).toEqual(["pause"]);
    expect([t.controlsShown(), t.focused(), t.skipShown()]).toEqual([true, "scrubber", true]);
  });

  it("comes back with the controls after its 8 s, which open on the scrubber; Up reaches Skip", () => {
    const t = setup();
    t.aloneOver();
    t.wait(1);
    t.press("info");
    expect([t.controlsShown(), t.focused(), t.skipShown()]).toEqual([true, "scrubber", true]);
    t.press("up");
    expect(t.focused()).toBe("skip");
    t.press("ok");
    expect(t.did).toEqual(["skip"]);
  });

  it("leaves focus to the scrubber when pressed over the controls: OK pauses, Left and Right stay on it", () => {
    const t = setup();
    t.aloneOver();
    t.wait(1);
    t.press("up");
    t.press("up");
    expect([t.controlsShown(), t.focused()]).toEqual([true, "skip"]);
    t.press("ok");
    expect(t.did).toEqual(["skip"]);
    t.wait(500);
    t.wait(1);
    expect([t.skipShown(), t.controlsShown(), t.focused()]).toEqual([false, true, "scrubber"]);
    t.press("right");
    expect(t.focused()).toBe("scrubber");
    t.press("ok");
    expect(t.did).toEqual(["skip", "pause"]);
  });

  it("goes with the controls after its 8 s, focus back on the video", () => {
    const t = setup();
    t.aloneOver();
    t.press("up");
    expect([t.controlsShown(), t.skipShown()]).toEqual([true, true]);
    t.press("back");
    expect([t.controlsShown(), t.skipShown(), t.focused()]).toEqual([false, false, "video"]);
    // And as they hide by themselves, 5 s after the last key while it plays.
    t.press("up");
    expect(t.skipShown()).toBe(true);
    t.wait(5_500);
    t.wait(1);
    expect([t.controlsShown(), t.skipShown(), t.focused()]).toEqual([false, false, "video"]);
  });

  it("keeps focus as the controls open in its 8 s, and stays with them after", () => {
    const t = setup();
    t.wait(1);
    t.press("info");
    expect([t.controlsShown(), t.focused()]).toEqual([true, "skip"]);
    t.aloneOver();
    t.wait(1);
    expect([t.skipShown(), t.focused()]).toEqual([true, "skip"]);
  });

  it("shows on its own again, with focus, on entering a marker anew", () => {
    const t = setup();
    t.aloneOver();
    t.wait(1);
    expect(t.focused()).toBe("video");
    t.alone();
    t.wait(1);
    expect([t.skipShown(), t.focused()]).toEqual([true, "skip"]);
  });
});

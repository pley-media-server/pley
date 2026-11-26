import { signal } from "@preact/signals";
import { render } from "preact";
import { act } from "preact/test-utils";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LiveChannel, SessionPlan } from "../../src/api/types";
import type { EngineListener, EngineStats, PlayerEngine } from "../../src/player/engine";
import type { AppState } from "../../src/state";
import { UiContext, type Ui } from "../../src/ui/context";
import { FocusManager } from "../../src/ui/focus";
import { Input } from "../../src/ui/input";
import type { Key } from "../../src/ui/keys";
import { LivePlayerScreen } from "../../src/ui/player/live";
import { AppContext } from "../../src/ui/screens/common";

// No music plays in these tests: the player has none to put away.
vi.mock("../../src/player/music", async (original) => ({
  ...(await original<typeof import("../../src/player/music")>()),
  initMusic: () => ({ yieldToVideo: () => undefined }),
}));

/** The Live TV player's Playback info, badge and compact readout, on a server and a TV's player that answer as the test says. */

let root: HTMLElement | null = null;
let detach: (() => void) | null = null;

function channel(id: number): LiveChannel {
  return { id, source_id: 1, name: `Ch ${id}`, number: String(100 + id), group: "", logo: null, favorite: false, has_guide: false, now: null, next: null };
}

/** A channel's plan as the server says it: H.264 and AAC copied, unless `over` says otherwise. */
function plan(over: Partial<SessionPlan> = {}): SessionPlan {
  return {
    video_codec: "h264",
    width: 1920,
    height: 1080,
    frame_rate: 25,
    interlaced: false,
    bit_depth: 8,
    audio_codec: "aac",
    audio_channels: 2,
    video: "copy",
    audio: "copy",
    out_height: null,
    video_kbps: null,
    kbps: 6200,
    ...over,
  };
}

function setup(settings: { playbackDetails: boolean } = { playbackDetails: false }) {
  document.body.innerHTML = "";
  root = document.createElement("div");
  document.body.appendChild(root);
  const focus = new FocusManager();
  focus.setPage(root);
  const input = new Input(focus);
  detach = input.attach(document);
  const asked: string[] = [];
  /** What the server makes of a session's channel, by session: not known (null) unless the test says. */
  const plans: Record<string, SessionPlan> = {};
  let sessions = 0;
  const api = {
    watchChannel: (id: number) => {
      sessions++;
      asked.push(`watch ${id}`);
      return Promise.resolve({ session_id: `s${sessions}`, url: `/api/live/media/pley_mk_${sessions}/index.m3u8`, kind: "hls", channel: channel(id) });
    },
    leaveChannel: (session: string) => {
      asked.push(`leave ${session}`);
      return Promise.resolve();
    },
    whyLiveEnded: () => Promise.resolve(null),
    media: () => Promise.resolve("#EXTM3U"),
    livePlan: (session: string) => {
      asked.push(`plan ${session}`);
      return Promise.resolve(plans[session] ?? null);
    },
    url: (path: string) => `http://tv.test${path}`,
    // The category's channels, for CH+ and CH−: two, round their ends.
    liveBrowse: () => Promise.resolve({ total: 2, offset: 0, channels: [channel(1), channel(2)] }),
    // The channel list's categories: none but All.
    liveGroups: () => Promise.resolve({ total: 2, favorites: 0, recent: 0, groups: [] }),
  };
  const ui = {
    focus,
    input,
    api,
    nav: { pop: () => true },
    scale: 1,
    claim: (el: HTMLElement, _fid: string | null, initial: boolean) => {
      if (initial && focus.inScope(el)) focus.focus(el);
    },
  } as unknown as Ui;
  let listener: EngineListener | null = null;
  let stats: EngineStats = { width: 1920, height: 1080, frameRate: 25, codec: "h264", dropped: 3, frames: 500, bufferedAheadMs: 4200 };
  const engine = {
    name: "AVPlay",
    setListener: (l: EngineListener) => {
      listener = l;
    },
    prepare: () => undefined,
    stop: () => undefined,
    release: () => undefined,
    stats: () => stats,
  } as unknown as PlayerEngine;
  const app = {
    changes: signal(0),
    visible: signal(true),
    settings: signal(settings),
    knownLiveChannel: (id: number) => channel(id),
    keepLiveChannel: () => undefined,
    installId: () => "tizen-tv",
    onEvent: () => () => undefined,
    platform: { createEngine: () => engine, keepAwake: () => undefined, clock24: () => true },
  } as unknown as AppState;
  act(() =>
    render(
      <AppContext.Provider value={app}>
        <UiContext.Provider value={ui}>
          <LivePlayerScreen channelId={1} category="all" pressedAt={0} />
        </UiContext.Provider>
      </AppContext.Provider>,
      root!,
    ),
  );
  const press = (key: Key) => {
    act(() => input.dispatch(key, "down", false));
    act(() => input.dispatch(key, "up", false));
  };
  const wait = (ms: number) => act(async () => void (await vi.advanceTimersByTimeAsync(ms)));
  /** The channel's picture shows. */
  const shows = () => act(() => listener!.firstFrame());
  /** The controls come up, and OK on Info opens the panel. */
  const openInfo = async () => {
    press("ok");
    await wait(0);
    const button = root!.querySelector<HTMLElement>('.player-controls.in [data-fid="info"]');
    expect(button).not.toBeNull();
    act(() => void focus.focus(button!));
    press("ok");
    await wait(0);
  };
  const panel = () => root!.querySelector(".side-panel");
  /** The panel's sections, as "label: value" lines under each heading. */
  const read = () => {
    const out: Record<string, string[]> = {};
    let under: string[] = [];
    panel()
      ?.querySelectorAll(".readout-heading, .readout-fact")
      .forEach((el) => {
        if (el.classList.contains("readout-heading")) out[el.textContent ?? ""] = under = [];
        else under.push(`${el.querySelector(".readout-label")?.textContent}: ${el.querySelector(".readout-value")?.textContent}`);
      });
    return out;
  };
  const note = () => panel()?.querySelector(".panel-note")?.textContent ?? null;
  const plansAsked = () => asked.filter((a) => a.indexOf("plan") === 0);
  /** How it plays, beside Live in the controls: its words and its color, or null when it isn't drawn. */
  const badge = () => {
    const el = root!.querySelector(".player-controls .live-chip-box .method-badge");
    return el ? `${el.textContent} (${el.className.replace("method-badge t-caption ", "")})` : null;
  };
  /** The compact readout over the picture, as "label: value" lines; null when it isn't drawn. */
  const readout = () => {
    const el = root!.querySelector(".compact-readout");
    return el ? Array.from(el.querySelectorAll(".readout-fact")).map((f) => `${f.children[0]?.textContent}: ${f.children[1]?.textContent}`) : null;
  };
  return { root: root!, focus, asked, plans, press, wait, shows, openInfo, panel, read, note, plansAsked, badge, readout, setStats: (s: EngineStats) => (stats = s) };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(console, "info").mockImplementation(() => undefined);
});
afterEach(() => {
  detach?.();
  detach = null;
  if (root) act(() => render(null, root!));
  root = null;
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("the Live TV player's Playback info", () => {
  it("opens from Info in the controls, and says how the channel plays and what this TV decodes", async () => {
    const tv = setup();
    tv.plans.s1 = plan({ video_codec: "hevc", bit_depth: 10, width: 3840, height: 2160, frame_rate: 50, video: "h264_nvenc", out_height: 1080, video_kbps: 8000, kbps: 12_300 });
    await tv.wait(0);
    expect(tv.plansAsked()).toEqual([]);
    tv.shows();
    // Asked once as the channel shows, for the badge.
    expect(tv.plansAsked()).toEqual(["plan s1"]);
    await tv.openInfo();
    expect(tv.panel()?.querySelector(".panel-title")?.textContent).toBe("Playback info");
    expect(tv.note()).toBeNull();
    expect(tv.read()).toEqual({
      "How it plays": [
        "Playback: Transcoding",
        "Video: HEVC 10-bit · 3840×2160 · 50 fps → H.264 1080p",
        "Audio: AAC 2.0",
        "Bitrate: 12 Mbps → 8.0 Mbps",
        "Encoder: NVIDIA NVENC",
      ],
      "This TV": ["Player: AVPlay", "Decoded: 1920×1080 · 25 fps · H.264", "Dropped frames: 3 of 500", "Buffered ahead: 4 s", "Stalls: 0", "First frame: 0 ms"],
    });
  });

  it("asks the server again every 2.5 s while it is open, and the TV's player each second", async () => {
    const tv = setup();
    tv.plans.s1 = plan({ kbps: 0 });
    await tv.wait(0);
    tv.shows();
    await tv.openInfo();
    expect(tv.read()["How it plays"]).toEqual(["Playback: Direct stream", "Video: H.264 · 1920×1080 · 25 fps", "Audio: AAC 2.0"]);
    tv.plans.s1 = plan({ kbps: 6200 });
    tv.setStats({ width: 1920, height: 1080, dropped: 9, frames: 550 });
    await tv.wait(1_000);
    expect(tv.read()["This TV"]).toContain("Dropped frames: 9 of 550");
    expect(tv.read()["How it plays"]).not.toContain("Bitrate: 6.2 Mbps");
    await tv.wait(1_500);
    // Once as the channel showed (for the badge), as the panel opened, and 2.5 s after.
    expect(tv.plansAsked()).toEqual(["plan s1", "plan s1", "plan s1"]);
    expect(tv.read()["How it plays"]).toContain("Bitrate: 6.2 Mbps");
    // The controls stay up under it, however long it is read.
    await tv.wait(10_000);
    expect(tv.root.querySelector(".player-controls.in")).not.toBeNull();
    expect(tv.panel()).not.toBeNull();
  });

  it("says it is asking until the server knows the channel, with what the TV says meanwhile", async () => {
    const tv = setup();
    await tv.wait(0);
    tv.shows();
    await tv.openInfo();
    expect(tv.note()).toBe("Asking the server…");
    expect(Object.keys(tv.read())).toEqual(["This TV"]);
    tv.plans.s1 = plan();
    await tv.wait(2_500);
    expect(tv.note()).toBeNull();
    expect(Object.keys(tv.read())).toEqual(["How it plays", "This TV"]);
  });

  it("closes on Back, to Info in the controls, and asks the server no more", async () => {
    const tv = setup();
    tv.plans.s1 = plan();
    await tv.wait(0);
    tv.shows();
    await tv.openInfo();
    tv.press("back");
    await tv.wait(0);
    expect(tv.panel()).toBeNull();
    expect(tv.focus.current?.getAttribute("data-fid")).toBe("info");
    expect(tv.root.querySelector(".player-controls.in")).not.toBeNull();
    const before = tv.plansAsked().length;
    await tv.wait(4_000);
    expect(tv.plansAsked()).toHaveLength(before);
  });

  it("stays open over CH+, saying the new channel's and asking no more of the one before", async () => {
    const tv = setup();
    tv.plans.s1 = plan();
    tv.plans.s2 = plan({ video: "libx264", interlaced: true });
    await tv.wait(0);
    tv.shows();
    await tv.openInfo();
    expect(tv.read()["How it plays"]![0]).toBe("Playback: Direct stream");
    tv.press("channelUp");
    await tv.wait(0);
    expect(tv.asked).toContain("watch 2");
    expect(tv.panel()).not.toBeNull();
    expect(tv.read()["How it plays"]![0]).toBe("Playback: Transcoding");
    await tv.wait(5_000);
    // As the channel showed, for the badge, and as the panel opened.
    expect(tv.plansAsked().filter((a) => a === "plan s1")).toHaveLength(2);
    expect(tv.plansAsked().filter((a) => a === "plan s2")).toHaveLength(3);
  });
});

describe("the Live TV player's badge", () => {
  it("says how the channel plays beside Live, in a title's words and colors", async () => {
    const tv = setup();
    tv.plans.s1 = plan({ video_codec: "hevc", bit_depth: 10, video: "h264_nvenc", out_height: 1080 });
    await tv.wait(0);
    tv.shows();
    await tv.wait(0);
    expect(tv.badge()).toBe("Transcoding (transcode)");
    // Left of Live, in its box.
    const box = tv.root.querySelector(".live-chip-box")!;
    expect(Array.from(box.children).map((el) => el.className.split(" ")[0])).toEqual(["method-badge", "live-badge"]);
    // It takes no focus: a title's doesn't.
    expect(box.querySelector("[data-fid]")).toBeNull();
  });

  it("is a direct stream's while the server copies the video, saying so when it converts the sound", async () => {
    const tv = setup();
    tv.plans.s1 = plan();
    tv.plans.s2 = plan({ audio_codec: "ac3", audio_channels: 6, audio: "aac" });
    await tv.wait(0);
    tv.shows();
    await tv.wait(0);
    expect(tv.badge()).toBe("Direct stream (stream)");
    tv.press("channelUp");
    await tv.wait(0);
    // None while the next channel tunes, nor "…": the one before isn't said under its name.
    expect(tv.badge()).toBeNull();
    tv.shows();
    await tv.wait(0);
    expect(tv.badge()).toBe("Direct stream · Audio transcoding (stream)");
  });

  it("isn't drawn until the server has said how the channel plays", async () => {
    const tv = setup();
    await tv.wait(0);
    tv.shows();
    await tv.wait(5_000);
    expect(tv.badge()).toBeNull();
    expect(tv.root.querySelector(".live-chip-box .live-badge")).not.toBeNull();
    // Asked once, and no more while it plays.
    expect(tv.plansAsked()).toEqual(["plan s1"]);
    // Playback info asks, and the badge has it after.
    tv.plans.s1 = plan();
    await tv.openInfo();
    tv.press("back");
    await tv.wait(0);
    expect(tv.badge()).toBe("Direct stream (stream)");
  });
});

describe("Settings → Playback details over a live channel", () => {
  it("draws no readout while the setting is off", async () => {
    const tv = setup();
    tv.plans.s1 = plan();
    await tv.wait(0);
    tv.shows();
    await tv.wait(3_000);
    expect(tv.readout()).toBeNull();
    expect(tv.plansAsked()).toEqual(["plan s1"]);
  });

  it("keeps the compact readout over the picture: how it plays, its rows, then what the TV decodes, as over a title", async () => {
    const tv = setup({ playbackDetails: true });
    tv.plans.s1 = plan({ video_codec: "hevc", bit_depth: 10, width: 3840, height: 2160, frame_rate: 50, video: "h264_nvenc", out_height: 1080, video_kbps: 8000, kbps: 12_300 });
    await tv.wait(0);
    tv.shows();
    await tv.wait(0);
    expect(tv.readout()).toEqual([
      "Playback: Transcoding",
      "Video: HEVC 10-bit · 3840×2160 · 50 fps → H.264 1080p",
      "Audio: AAC 2.0",
      "Bitrate: 12 Mbps → 8.0 Mbps",
      "Encoder: NVIDIA NVENC",
      "Decoded: 1920×1080",
      "Dropped: 3 of 500",
      "Buffer: 4 s ahead",
    ]);
    // Asked again while it shows, and the TV's player read each second.
    tv.plans.s1 = plan({ kbps: 6200 });
    tv.setStats({ width: 1920, height: 1080, dropped: 9, frames: 550 });
    await tv.wait(2_500);
    expect(tv.readout()).toContain("Bitrate: 6.2 Mbps");
    expect(tv.readout()).toContain("Dropped: 9 of 550");
  });

  it("is hidden under the controls and Playback info, and back once they close", async () => {
    const tv = setup({ playbackDetails: true });
    tv.plans.s1 = plan();
    await tv.wait(0);
    tv.shows();
    await tv.wait(0);
    expect(tv.readout()).not.toBeNull();
    await tv.openInfo();
    expect(tv.readout()).toBeNull();
    expect(tv.read()["How it plays"]![0]).toBe("Playback: Direct stream");
    tv.press("back");
    await tv.wait(0);
    // The controls are still up.
    expect(tv.readout()).toBeNull();
    tv.press("back");
    await tv.wait(0);
    expect(tv.readout()).not.toBeNull();
  });

  it("is hidden beside the channel list, and says nothing until the server has said how the channel plays", async () => {
    const tv = setup({ playbackDetails: true });
    await tv.wait(0);
    tv.shows();
    await tv.wait(0);
    expect(tv.readout()).toBeNull();
    tv.plans.s1 = plan();
    await tv.wait(2_500);
    expect(tv.readout()![0]).toBe("Playback: Direct stream");
    tv.press("channelList");
    await tv.wait(0);
    expect(tv.readout()).toBeNull();
  });
});

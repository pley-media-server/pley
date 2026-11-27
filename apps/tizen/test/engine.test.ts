import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EngineListener, EngineSource } from "pley-tv/player/engine";
import { AvplayEngine } from "../src/engine";

/**
 * A pretend AVPlay: it records the calls that reach it, and the test says
 * how each seek ends (its success or error callback, a throw, or never)
 * and where the player is.
 */
function fakeAvplay() {
  const calls: string[] = [];
  const player = {
    state: "NONE" as "NONE" | "IDLE" | "READY" | "PLAYING" | "PAUSED",
    time: 0,
    /** How the next seek ends: "ok" and "error" wait for the test's `endSeek`. */
    seek: "ok" as "ok" | "error" | "throw",
    pendingSeek: null as null | { ok: () => void; err: (e: unknown) => void },
    prepared: null as null | (() => void),
    listener: null as null | AvplayListener,
  };
  const avplay = {
    open: () => {
      player.state = "IDLE";
      calls.push("open");
    },
    close: () => {
      player.state = "NONE";
    },
    stop: () => {
      player.state = "IDLE";
    },
    setListener: (l: AvplayListener) => {
      player.listener = l;
    },
    setDisplayRect: () => undefined,
    setDisplayMethod: () => undefined,
    setStreamingProperty: () => undefined,
    getStreamingProperty: () => "0",
    setSilentSubtitle: () => undefined,
    prepareAsync: (ok: () => void) => {
      player.prepared = () => {
        player.state = "READY";
        ok();
      };
    },
    getState: () => player.state,
    play: () => {
      player.state = "PLAYING";
      calls.push("play");
    },
    pause: () => {
      player.state = "PAUSED";
      calls.push("pause");
    },
    getCurrentTime: () => player.time,
    getDuration: () => 7_200_000,
    seekTo: (ms: number, ok: () => void, err: (e: unknown) => void) => {
      calls.push(`seekTo ${ms}`);
      if (player.seek === "throw") throw Object.assign(new Error("not now"), { name: "InvalidStateError" });
      player.pendingSeek = { ok, err };
    },
    getTotalTrackInfo: () => [],
    getCurrentStreamInfo: () => [],
  };
  /** Ends the seek in flight as `player.seek` says. */
  const endSeek = () => {
    const p = player.pendingSeek;
    player.pendingSeek = null;
    if (player.seek === "error") p?.err({ name: "PLAYER_ERROR_SEEK_FAILED" });
    else p?.ok();
  };
  return { avplay, player, calls, endSeek };
}

function source(startMs: number): EngineSource {
  return { url: "http://nas/f.mkv", hls: false, mime: "video/x-matroska", startMs, durationHintMs: null, video: null, audioCodec: null };
}

let fake: ReturnType<typeof fakeAvplay>;
let heard: string[];
let failed: number[];
/** The seeks the session heard of as slow (still loading), not refused. */
let slow: number[];
let engine: AvplayEngine;

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(console, "info").mockImplementation(() => undefined);
  fake = fakeAvplay();
  vi.stubGlobal("webapis", { avplay: fake.avplay });
  heard = [];
  failed = [];
  slow = [];
  // A set before 2020 (Tizen 5), whose AVPlay starts streams at their first segment.
  engine = new AvplayEngine(document.createElement("div"), false, true);
  const listener: EngineListener = {
    status: (s) => heard.push(s),
    firstFrame: () => heard.push("firstFrame"),
    error: (e) => heard.push(`error ${e.kind}`),
    tracks: () => undefined,
    seekFailed: (ms, s) => {
      failed.push(ms);
      if (s) slow.push(ms);
    },
  };
  engine.setListener(listener);
});

afterEach(() => {
  engine.release();
  vi.useRealTimers();
});

/** A source opened at `startMs` and playing, its resume seek (if any) ended as the fake says. */
function playing(startMs = 0) {
  engine.prepare(source(startMs), true);
  fake.player.prepared!();
  if (startMs >= 1000) fake.endSeek();
  fake.player.time = startMs + 1;
}

describe("AVPlay's seeks", () => {
  it("tells the session of a seek AVPlay refuses, and stops waiting for it", () => {
    playing();
    fake.player.time = 60_000;
    fake.player.seek = "error";
    engine.seekTo(1_200_000);
    expect(engine.status()).toBe("buffering");
    fake.endSeek();
    expect(failed).toEqual([1_200_000]);
    expect(engine.status()).toBe("ready");
    // Where it really is, not where it was asked to go.
    expect(engine.positionMs()).toBe(60_000);
  });

  it("tells the session of a seek AVPlay says it made but that never moved it, once it played a while", () => {
    playing();
    fake.player.time = 60_000;
    engine.seekTo(1_200_000);
    fake.endSeek();
    // AVPlay waits for the stream: nothing is decided meanwhile, however long.
    fake.player.listener!.onbufferingstart!();
    vi.advanceTimersByTime(10_000);
    expect(failed).toEqual([]);
    fake.player.listener!.onbufferingcomplete!();
    vi.advanceTimersByTime(3_500);
    expect(failed).toEqual([1_200_000]);
  });

  it("says nothing of a seek that landed", () => {
    playing();
    engine.seekTo(1_200_000);
    fake.player.time = 1_200_400;
    fake.endSeek();
    vi.advanceTimersByTime(20_000);
    expect(failed).toEqual([]);
  });

  it("gives up waiting for a stream that never shows where it went", () => {
    playing();
    fake.player.time = 60_000;
    engine.seekTo(1_200_000);
    fake.endSeek();
    fake.player.listener!.onbufferingstart!();
    vi.advanceTimersByTime(16_000);
    expect(failed).toEqual([1_200_000]);
    // Still loading: a slow network, not a refusal to remember for the kind of file.
    expect(slow).toEqual([1_200_000]);
  });

  it("tells the session of a seek that played on from where it was as refused, not slow", () => {
    playing();
    fake.player.time = 60_000;
    engine.seekTo(1_200_000);
    fake.endSeek();
    fake.player.time = 63_000;
    vi.advanceTimersByTime(3_500);
    expect(failed).toEqual([1_200_000]);
    expect(slow).toEqual([]);
  });

  it("doesn't judge a seek made while paused until it plays: paused, AVPlay's time doesn't move", () => {
    playing();
    fake.player.time = 1_200_000;
    engine.pause();
    engine.seekTo(1_210_000);
    fake.endSeek();
    // AVPlay ignored it, and stays where it was while paused.
    vi.advanceTimersByTime(60_000);
    expect(failed).toEqual([]);
    engine.play();
    fake.player.time = 1_203_000;
    vi.advanceTimersByTime(3_500);
    expect(failed).toEqual([1_210_000]);
  });

  it("takes a seek made while paused as landed once it plays on from there", () => {
    playing();
    fake.player.time = 1_200_000;
    engine.pause();
    engine.seekTo(1_210_000);
    fake.player.time = 1_210_000;
    fake.endSeek();
    vi.advanceTimersByTime(60_000);
    engine.play();
    fake.player.time = 1_213_000;
    vi.advanceTimersByTime(20_000);
    expect(failed).toEqual([]);
  });

  it("counts only the time it played toward a seek's check, not a pause in between", () => {
    playing();
    fake.player.time = 1_200_000;
    engine.seekTo(1_210_000);
    fake.endSeek();
    vi.advanceTimersByTime(1_000);
    engine.pause();
    vi.advanceTimersByTime(30_000);
    engine.play();
    // AVPlay ignored it: 1 s played before the pause, 3 s after, from where it was.
    fake.player.time = 1_204_000;
    vi.advanceTimersByTime(3_000);
    expect(failed).toEqual([1_210_000]);
  });

  it("survives a seek that throws: the session hears of it, and later calls still reach AVPlay", () => {
    playing();
    fake.player.seek = "throw";
    engine.seekTo(1_000_000);
    expect(failed).toEqual([1_000_000]);
    expect(engine.status()).toBe("ready");
    fake.player.seek = "ok";
    fake.calls.length = 0;
    engine.pause();
    engine.seekTo(2_000_000);
    fake.endSeek();
    engine.play();
    expect(fake.calls).toEqual(["pause", "seekTo 2000000", "play"]);
  });

  it("stops waiting for a seek AVPlay never answers", () => {
    playing();
    engine.seekTo(1_200_000);
    engine.pause();
    expect(fake.calls).not.toContain("pause");
    vi.advanceTimersByTime(10_000);
    expect(engine.status()).toBe("ready");
    expect(fake.calls).toContain("pause");
  });

  it("holds the position it was asked for while the seek is made, not AVPlay's stale time", () => {
    playing();
    fake.player.time = 60_000;
    engine.seekTo(1_200_000);
    expect(engine.positionMs()).toBe(1_200_000);
    fake.player.listener!.oncurrentplaytime!(60_500);
    expect(engine.positionMs()).toBe(1_200_000);
    fake.endSeek();
    // Just after: AVPlay's time catches up.
    expect(engine.positionMs()).toBe(1_200_000);
    fake.player.time = 1_201_000;
    expect(engine.positionMs()).toBe(1_201_000);
  });
});

describe("AVPlay's resume", () => {
  it("plays from where AVPlay is when it refuses the resume point, and tells the session", () => {
    fake.player.seek = "error";
    engine.prepare(source(3_600_000), true);
    fake.player.prepared!();
    expect(fake.calls).toEqual(["open", "seekTo 3600000"]);
    fake.endSeek();
    expect(fake.calls).toEqual(["open", "seekTo 3600000", "play"]);
    expect(failed).toEqual([3_600_000]);
    expect(heard).not.toContain("error other");
    // Its real place, not the resume point it never reached.
    fake.player.time = 2_000;
    expect(engine.positionMs()).toBe(2_000);
  });

  it("plays when the resume seek throws", () => {
    fake.player.seek = "throw";
    engine.prepare(source(3_600_000), true);
    fake.player.prepared!();
    expect(fake.calls).toEqual(["open", "seekTo 3600000", "play"]);
    expect(failed).toEqual([3_600_000]);
  });

  it("tells the session of a resume AVPlay says it made but that starts at the beginning", () => {
    engine.prepare(source(3_600_000), true);
    fake.player.prepared!();
    fake.endSeek();
    fake.player.time = 3_000;
    vi.advanceTimersByTime(3_500);
    expect(failed).toEqual([3_600_000]);
  });

  it("shows the resume point until AVPlay says where it is", () => {
    engine.prepare(source(3_600_000), true);
    expect(engine.positionMs()).toBe(3_600_000);
    fake.player.prepared!();
    expect(engine.positionMs()).toBe(3_600_000);
    fake.endSeek();
    fake.player.time = 3_600_200;
    expect(engine.positionMs()).toBe(3_600_200);
    vi.advanceTimersByTime(20_000);
    expect(failed).toEqual([]);
  });

  it("doesn't seek to a start under a second", () => {
    engine.prepare(source(500), true);
    fake.player.prepared!();
    expect(fake.calls).toEqual(["open", "play"]);
    expect(failed).toEqual([]);
  });
});

describe("AVPlay's seeks, one after another and after they land", () => {
  it("lets a seek asked while another is made decide the outcome (made)", () => {
    playing();
    fake.player.time = 60_000;
    engine.seekTo(600_000);
    engine.seekTo(1_200_000);
    expect(fake.calls.filter((c) => c.startsWith("seekTo"))).toEqual(["seekTo 600000"]);
    fake.endSeek();
    // The second goes to AVPlay now; the first's outcome is moot.
    expect(fake.calls.filter((c) => c.startsWith("seekTo"))).toEqual(["seekTo 600000", "seekTo 1200000"]);
    expect(engine.status()).toBe("buffering");
    expect(engine.positionMs()).toBe(1_200_000);
    vi.advanceTimersByTime(5_000);
    expect(failed).toEqual([]);
    fake.player.time = 1_200_300;
    fake.endSeek();
    vi.advanceTimersByTime(20_000);
    expect(failed).toEqual([]);
  });

  it("lets a seek asked while another is made decide the outcome (refused)", () => {
    playing();
    fake.player.time = 60_000;
    fake.player.seek = "error";
    engine.seekTo(600_000);
    engine.seekTo(1_200_000);
    fake.endSeek();
    expect(failed).toEqual([]);
    expect(engine.status()).toBe("buffering");
    expect(engine.positionMs()).toBe(1_200_000);
    fake.endSeek();
    expect(failed).toEqual([1_200_000]);
  });

  it("lets go of the place it held once the seek landed a little past it", () => {
    playing();
    fake.player.time = 60_000;
    engine.seekTo(1_200_000);
    fake.player.time = 1_203_000;
    fake.endSeek();
    for (let i = 1; i <= 5; i++) {
      vi.advanceTimersByTime(1_000);
      fake.player.time = 1_203_000 + i * 1_000;
    }
    expect(engine.positionMs()).toBe(1_208_000);
    expect(failed).toEqual([]);
  });

  it("takes a landing on a keyframe well before the target as landed", () => {
    playing();
    fake.player.time = 60_000;
    engine.seekTo(1_200_000);
    fake.player.time = 1_190_000;
    fake.endSeek();
    fake.player.time = 1_193_000;
    vi.advanceTimersByTime(20_000);
    expect(failed).toEqual([]);
  });

  it("takes a short skip landing on the keyframe before where it came from as made (a long-GOP file)", () => {
    playing();
    fake.player.time = 1_200_000;
    engine.seekTo(1_210_000);
    fake.endSeek();
    // On the keyframe 2 s before where it came from, played 3 s by the check.
    fake.player.time = 1_201_000;
    vi.advanceTimersByTime(20_000);
    expect(failed).toEqual([]);
  });

  it("tells the session of a short skip AVPlay says it made but that played on from where it was", () => {
    playing();
    fake.player.time = 1_200_000;
    engine.seekTo(1_210_000);
    fake.endSeek();
    // Where it came from, played 3 s by the check.
    fake.player.time = 1_203_000;
    vi.advanceTimersByTime(20_000);
    expect(failed).toEqual([1_210_000]);
  });

  it("takes a landing a little past the target, played a while, as landed", () => {
    playing();
    fake.player.time = 60_000;
    engine.seekTo(1_200_000);
    fake.endSeek();
    fake.player.time = 1_206_000;
    vi.advanceTimersByTime(20_000);
    expect(failed).toEqual([]);
  });

  it("takes a landing on a stream that keeps waiting as landed once it played on from there", () => {
    playing();
    fake.player.time = 60_000;
    engine.seekTo(1_200_000);
    fake.endSeek();
    let t = 1_200_000;
    for (let i = 0; i < 3; i++) {
      vi.advanceTimersByTime(2_500);
      t += 2_500;
      fake.player.time = t;
      fake.player.listener!.onbufferingstart!();
      vi.advanceTimersByTime(2_000);
      fake.player.listener!.onbufferingcomplete!();
    }
    vi.advanceTimersByTime(5_000);
    expect(failed).toEqual([]);
  });

  it("says nothing of a seek once the stream ended", () => {
    playing();
    fake.player.time = 60_000;
    engine.seekTo(7_198_000);
    fake.player.time = 7_198_100;
    fake.endSeek();
    fake.player.listener!.onbufferingstart!();
    fake.player.time = 7_200_000;
    fake.player.listener!.onstreamcompleted!();
    fake.player.time = 0;
    vi.advanceTimersByTime(20_000);
    expect(failed).toEqual([]);
    expect(engine.positionMs()).not.toBe(0);
  });

  it("starts at a place asked for while the source opens, not the resume point", () => {
    engine.prepare(source(3_600_000), true);
    engine.seekTo(1_000_000);
    expect(fake.calls).toEqual(["open"]);
    expect(engine.positionMs()).toBe(1_000_000);
    fake.player.prepared!();
    expect(fake.calls).toEqual(["open", "seekTo 1000000"]);
    fake.endSeek();
    expect(fake.calls).toEqual(["open", "seekTo 1000000", "play"]);
  });
});

describe("AVPlay's seeks in a stream the server makes", () => {
  function hls(startMs: number): EngineSource {
    return { ...source(startMs), url: "http://nas/master.m3u8", hls: true, mime: null };
  }

  it("plays a stream resumed part-way first, and seeks to the resume point once it plays", () => {
    // Tizen's player prepares a stream from its first segment and doesn't answer a seek before it plays.
    engine.prepare(hls(600_000), true);
    fake.player.prepared!();
    expect(fake.calls).toEqual(["open", "play"]);
    expect(engine.positionMs()).toBe(600_000);
    fake.player.time = 40;
    fake.player.listener!.oncurrentplaytime!(40);
    expect(fake.calls).toEqual(["open", "play", "seekTo 600000"]);
    // The resume point shows meanwhile, not the stream's start.
    expect(engine.positionMs()).toBe(600_000);
    vi.advanceTimersByTime(12_000);
    fake.player.time = 600_100;
    fake.endSeek();
    vi.advanceTimersByTime(20_000);
    fake.player.time = 620_000;
    expect(failed).toEqual([]);
    // Once only: later times don't seek again.
    fake.player.listener!.oncurrentplaytime!(620_000);
    expect(fake.calls.filter((c) => c.startsWith("seekTo"))).toEqual(["seekTo 600000"]);
  });

  it("says so to the session, which then has the server start a stream at its first segment too", () => {
    expect(engine.hlsFromFirstSegment).toBe(true);
  });

  it("seeks a stream to its resume point before it plays on 2020 and later sets", () => {
    engine.release();
    engine = new AvplayEngine(document.createElement("div"), false, false);
    engine.setListener({ status: () => undefined, firstFrame: () => undefined, error: () => undefined, tracks: () => undefined, seekFailed: (ms) => failed.push(ms) });
    expect(engine.hlsFromFirstSegment).toBe(false);
    engine.prepare(hls(600_000), true);
    fake.player.prepared!();
    expect(fake.calls).toEqual(["open", "seekTo 600000"]);
    fake.endSeek();
    expect(fake.calls).toEqual(["open", "seekTo 600000", "play"]);
  });

  it("drops the resume point for a seek to the start asked before the stream plays", () => {
    engine.prepare(hls(600_000), true);
    fake.player.prepared!();
    engine.seekTo(0);
    fake.player.time = 40;
    fake.player.listener!.oncurrentplaytime!(40);
    expect(fake.calls.filter((c) => c.startsWith("seekTo"))).toEqual([]);
    expect(engine.positionMs()).toBe(40);
  });

  it("seeks a stream opened paused to its resume point once it plays", () => {
    engine.prepare(hls(600_000), false);
    fake.player.prepared!();
    expect(fake.calls).toEqual(["open"]);
    engine.play();
    fake.player.time = 40;
    fake.player.listener!.oncurrentplaytime!(40);
    expect(fake.calls).toEqual(["open", "play", "seekTo 600000"]);
  });

  it("tells the session when a stream resumed part-way can't get to the resume point", () => {
    engine.prepare(hls(600_000), true);
    fake.player.prepared!();
    fake.player.seek = "error";
    fake.player.time = 40;
    fake.player.listener!.oncurrentplaytime!(40);
    fake.endSeek();
    expect(failed).toEqual([600_000]);
  });

  it("doesn't judge a seek AVPlay never answered by its time from before it", () => {
    engine.prepare(hls(0), true);
    fake.player.prepared!();
    fake.player.time = 60_000;
    engine.seekTo(1_800_000);
    vi.advanceTimersByTime(60_000);
    expect(failed).toEqual([]);
  });

  it("takes a seek the stream is still waiting for when the check is due as made", () => {
    engine.prepare(hls(0), true);
    fake.player.prepared!();
    fake.player.time = 60_000;
    engine.seekTo(1_800_000);
    fake.endSeek();
    fake.player.listener!.onbufferingstart!();
    vi.advanceTimersByTime(16_000);
    expect(failed).toEqual([]);
  });
});

describe("AVPlay plays on by itself", () => {
  function hls(startMs: number): EngineSource {
    return { ...source(startMs), url: "http://nas/master.m3u8", hls: true, mime: null };
  }
  /** Each status the session hears, with AVPlay's state then: the session reads `playing()` as it hears "ready". */
  function watchState(): string[] {
    const seen: string[] = [];
    engine.setListener({
      status: (s) => seen.push(`${s}:${fake.player.state}`),
      firstFrame: () => undefined,
      error: () => undefined,
      tracks: () => undefined,
      seekFailed: (ms) => failed.push(ms),
    });
    return seen;
  }
  /** The next seek leaves AVPlay paused, as Tizen 5's player can after a seek in a stream. */
  function seeksPause(): void {
    const seekTo = fake.avplay.seekTo;
    fake.avplay.seekTo = (ms, ok, err) => {
      fake.player.state = "PAUSED";
      seekTo(ms, ok, err);
    };
  }

  it("says a stream started part-way (a jump's new session) is ready only once AVPlay plays", () => {
    const seen = watchState();
    engine.prepare(hls(1_200_000), true);
    fake.player.prepared!();
    expect(seen.filter((s) => s.startsWith("ready"))).toEqual(["ready:PLAYING"]);
  });

  it("says a file is ready only once AVPlay plays", () => {
    const seen = watchState();
    engine.prepare(source(0), true);
    fake.player.prepared!();
    expect(seen.filter((s) => s.startsWith("ready"))).toEqual(["ready:PLAYING"]);
  });

  it("says a file resumed part-way is ready only once AVPlay plays", () => {
    const seen = watchState();
    engine.prepare(source(600_000), true);
    fake.player.prepared!();
    fake.player.time = 600_000;
    fake.endSeek();
    expect(seen.filter((s) => s.startsWith("ready"))).toEqual(["ready:PLAYING"]);
  });

  it("isn't ready while it prepares: AVPlay's buffering then is before it plays", () => {
    const seen = watchState();
    engine.prepare(source(0), true);
    fake.player.listener!.onbufferingcomplete!();
    expect(seen.filter((s) => s.startsWith("ready"))).toEqual([]);
  });

  it("isn't ready while it seeks to a file's resume point, before it plays", () => {
    const seen = watchState();
    engine.prepare(source(600_000), true);
    fake.player.prepared!();
    fake.player.listener!.onbufferingcomplete!();
    fake.player.time = 600_000;
    fake.endSeek();
    expect(seen.filter((s) => s.startsWith("ready"))).toEqual(["ready:PLAYING"]);
  });

  it("plays on after a seek in a stream that left AVPlay paused", () => {
    engine.prepare(hls(1_200_000), true);
    fake.player.prepared!();
    seeksPause();
    fake.player.time = 40;
    fake.player.listener!.oncurrentplaytime!(40);
    fake.player.time = 1_200_100;
    fake.endSeek();
    expect(fake.player.state).toBe("PLAYING");
  });

  it("plays on after a seek that left AVPlay paused while playing", () => {
    playing();
    seeksPause();
    engine.seekTo(1_200_000);
    fake.endSeek();
    expect(fake.player.state).toBe("PLAYING");
  });

  it("stays paused after a seek asked while paused", () => {
    playing();
    engine.pause();
    fake.calls.length = 0;
    engine.seekTo(1_200_000);
    fake.endSeek();
    expect(fake.calls).toEqual(["seekTo 1200000"]);
    expect(fake.player.state).toBe("PAUSED");
  });
});

describe("AVPlay's seek still being made", () => {
  it("says a seek in a stream is still being made, until AVPlay answers it", () => {
    engine.prepare({ ...source(0), url: "http://nas/master.m3u8", hls: true, mime: null }, true);
    fake.player.prepared!();
    expect(engine.seekPending()).toBe(false);
    engine.seekTo(1_200_000);
    expect(engine.seekPending()).toBe(true);
    fake.endSeek();
    expect(engine.seekPending()).toBe(false);
  });

  it("is already being made when the session hears it wait for it: the wait is the seek's, not a stall", () => {
    const pendingAtBuffering: boolean[] = [];
    engine.setListener({
      status: (s) => s === "buffering" && pendingAtBuffering.push(engine.seekPending()),
      firstFrame: () => undefined,
      error: () => undefined,
      tracks: () => undefined,
    });
    // A stream resumed part-way: its own seek to the resume point, once it plays.
    engine.prepare({ ...source(600_000), url: "http://nas/master.m3u8", hls: true, mime: null }, true);
    fake.player.prepared!();
    pendingAtBuffering.length = 0;
    fake.player.time = 40;
    fake.player.listener!.oncurrentplaytime!(40);
    expect(fake.calls.at(-1)).toBe("seekTo 600000");
    expect(pendingAtBuffering).toEqual([true]);
  });

  it("says nothing of a seek in a file: AVPlay answers those itself", () => {
    playing();
    engine.seekTo(1_200_000);
    expect(engine.seekPending()).toBe(false);
  });
});

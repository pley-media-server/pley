import { afterEach, describe, expect, it, vi } from "vitest";
import type { Api } from "../../src/api/client";
import type { Control, ItemSummary, PlaybackInfo, PlaybackRequest } from "../../src/api/types";
import { MusicPlayer } from "../../src/player/music";
import type { PlayerCommand } from "../../src/player/remote";
import { Reporter } from "../../src/player/reporter";
import type { AppState } from "../../src/state";
import type { KeyValue } from "../../src/auth/tokens";

function track(id: number): ItemSummary {
  return { id, library_id: 3, kind: "track", title: `Track ${id}`, sort_title: `track ${id}`, added_at: 0, duration_ms: 240_000, user_state: { position_ms: 0, watched: false, play_count: 0 } };
}

interface Options {
  /** A non-direct session's stream: HLS (seekable anywhere, as the server's VOD playlists are) or one progressive stream. */
  hls?: boolean;
  /** The session's duration, as the server says it. */
  durationMs?: number;
  /** Whether play() says at once that it plays (false: the test says when). */
  playsAtOnce?: boolean;
  /** Where a cast starts the first track. */
  fromMs?: number;
  /** The app's reporter (a test's own: one that never flushes). */
  reporter?: Pick<Reporter, "send" | "flush" | "drop">;
  /** The file's extension in its address: what the TV remembers its seeks by. */
  ext?: string;
  /** The TV's <audio> takes no move of `currentTime` in this file (Tizen 5 and FLAC): it plays on where it was. */
  ignoresSeeks?: boolean;
  /** Whether a fresh load from an address ending `#t=<s>` starts there (else at 0). */
  fragmentSeeks?: boolean;
  /** The app's storage (a test's own: shared between players, or one that fails). */
  store?: KeyValue;
  /**
   * As the HTML spec allows, `currentTime` reads the place asked for (by a
   * move or `#t=`) while the element still seeks there (`seeking`), until the
   * test says where it really went (`media.pending = null`).
   */
  reportsPending?: boolean;
}

function memoryStore(): KeyValue & { data: Map<string, string> } {
  const data = new Map<string, string>();
  return { data, get: (k) => data.get(k) ?? null, set: (k, v) => void data.set(k, v), remove: (k) => void data.delete(k) };
}

/** The music player with the server and an <audio> element the test drives: what was asked, and where it plays. */
async function world(tracks: number[], at: number, method: "direct" | "transcode" = "direct", o: Options = {}) {
  const calls: string[] = [];
  const requests: PlaybackRequest[] = [];
  let n = 0;
  const api = {
    url: (p: string) => `http://nas:7539${p}`,
    play: async (req: PlaybackRequest): Promise<PlaybackInfo> => {
      requests.push(req);
      n++;
      calls.push(`start ${req.item_id}`);
      const m = req.mode === "transcode" ? "transcode" : method;
      const hls = m === "direct" ? false : (o.hls ?? true);
      return { session_id: `t${n}`, method: m, url: `/api/media/pley_mk_${n}/file.${m === "direct" ? (o.ext ?? "flac") : "m3u8"}`, hls, file_id: req.item_id, duration_ms: o.durationMs ?? 240_000, audio_index: 0, subtitle_index: null, burn_subtitles: false, subtitles: [], reason: null };
    },
    progress: async (id: number, pos: number, _d: number | null, state: string) => void calls.push(`progress ${id} ${state} ${Math.round(pos)}`),
    stopSession: async (id: string) => void calls.push(`delete ${id}`),
    sessionStatus: async () => ({}),
  } as unknown as Api;
  const app = {
    api: { value: api },
    reporter: o.reporter ?? new Reporter(),
    caps: async () => null,
    installId: () => "tizen-tv",
    onLeave: () => undefined,
    store: o.store ?? memoryStore(),
  } as unknown as AppState;
  const player = new MusicPlayer(app);
  const audio = document.body.querySelector("audio:last-of-type") as HTMLAudioElement;
  // HAVE_ENOUGH_DATA unless a test says otherwise; 0 is a stream that hasn't told its length yet.
  const media = { paused: true, at: 0, ready: 4, playsAtOnce: o.playsAtOnce ?? true, pending: null as number | null };
  Object.defineProperty(audio, "paused", { get: () => media.paused });
  Object.defineProperty(audio, "seeking", { get: () => media.pending !== null });
  const sets: number[] = [];
  Object.defineProperty(audio, "currentTime", {
    get: () => media.pending ?? media.at,
    set: (s: number) => {
      sets.push(s);
      if (o.reportsPending) media.pending = s;
      if (!o.ignoresSeeks) media.at = s;
    },
  });
  // The address the element was last given, as it was given; a new one loads afresh.
  const loads: string[] = [];
  Object.defineProperty(audio, "src", {
    get: () => loads.at(-1) ?? "",
    set: (v: string) => {
      loads.push(v);
      if (o.ignoresSeeks && loads.length > 1) {
        const t = /#t=([\d.]+)$/.exec(v);
        media.at = t && o.fragmentSeeks ? Number(t[1]) : 0;
        if (t && o.reportsPending) media.pending = Number(t[1]);
      }
    },
  });
  audio.load = () => undefined;
  Object.defineProperty(audio, "readyState", { get: () => media.ready });
  // A conversion moves only where it has come to, and here it has come nowhere.
  Object.defineProperty(audio, "seekable", { get: () => ({ length: 0 }) });
  audio.play = () => {
    media.paused = false;
    if (media.playsAtOnce) audio.dispatchEvent(new Event("playing"));
    return Promise.resolve();
  };
  audio.pause = () => {
    if (media.paused) return;
    media.paused = true;
    audio.dispatchEvent(new Event("pause"));
  };
  player.play(tracks.map(track), at, false, o.fromMs ?? 0);
  await settle();
  calls.length = 0;
  const send = (session: string, command: Control, value: { position_ms?: number; offset_ms?: number } = {}) => {
    const e: PlayerCommand = { type: "player_command", session_id: session, command, position_ms: value.position_ms ?? null, offset_ms: value.offset_ms ?? null };
    player.obey(e);
  };
  const fire = (type: string) => audio.dispatchEvent(new Event(type));
  return { player, media, calls, requests, send, fire, audio, sets, loads };
}

/**
 * The calls, in any order between a track's last session and its next: a
 * start doesn't wait for the last session's reports (the server ends a
 * player's last session as its next starts), which still go out in order.
 */
function sameCalls(calls: string[], expected: string[]): void {
  expect(calls.slice().sort()).toEqual(expected.slice().sort());
  const sent = calls.filter((c) => !c.startsWith("start"));
  expect(sent).toEqual(expected.filter((c) => !c.startsWith("start")));
}

const settle = (): Promise<unknown> => (vi.isFakeTimers() ? vi.advanceTimersByTimeAsync(20) : new Promise((r) => setTimeout(r, 20)));

afterEach(() => {
  vi.useRealTimers();
  document.body.innerHTML = "";
});

describe("music and the user's remote, on another device", () => {
  it("is told the music obeys every command", async () => {
    const { requests } = await world([1, 2, 3], 1);
    expect(requests[0]!.controls).toEqual(["play", "pause", "seek", "skip", "next", "previous"]);
  });

  it("pauses and plays the track's session, reporting each at once, and leaves another session's commands alone", async () => {
    const { media, calls, send } = await world([1, 2, 3], 1);
    media.at = 60;
    send("t9", "pause");
    expect(media.paused).toBe(false);
    send("t1", "pause");
    send("t1", "play");
    await settle();
    expect(calls).toEqual(["progress 2 paused 60000", "progress 2 playing 60000"]);
  });

  it("says again that it plays, or is paused, when told so already, so a remote a little behind settles", async () => {
    const { media, calls, send } = await world([1, 2, 3], 1);
    media.at = 60;
    send("t1", "play");
    send("t1", "pause");
    send("t1", "pause");
    await settle();
    expect(media.paused).toBe(true);
    expect(calls).toEqual(["progress 2 playing 60000", "progress 2 paused 60000", "progress 2 paused 60000"]);
  });

  it("seeks within the track and skips from where it is, reporting where it went", async () => {
    const { media, calls, send } = await world([1, 2, 3], 1);
    media.at = 60;
    send("t1", "skip", { offset_ms: -90_000 });
    send("t1", "seek", { position_ms: 300_000 });
    await settle();
    expect(calls).toEqual(["progress 2 playing 0", "progress 2 playing 240000"]);
  });

  it("starts the track again past 3 s, else plays the one before, and plays the next while there is one", async () => {
    const w = await world([1, 2, 3], 1);
    w.media.at = 10;
    w.send("t1", "previous");
    await settle();
    expect(w.media.at).toBe(0);
    expect(w.calls).toEqual(["progress 2 playing 0"]);
    w.calls.length = 0;
    w.send("t1", "previous");
    await settle();
    sameCalls(w.calls, ["progress 2 stopped 0", "delete t1", "start 1"]);
    w.calls.length = 0;
    w.send("t2", "next");
    await settle();
    sameCalls(w.calls, ["progress 1 stopped 0", "delete t2", "start 2"]);
    const last = await world([1, 2], 1);
    last.send("t1", "next");
    await settle();
    expect(last.calls).toEqual([]);
  });

  it("starts a new conversion at the place for a seek in a session that isn't the file itself, rather than moving within its stream", async () => {
    const { player, media, calls, requests, send } = await world([1, 2, 3], 1, "transcode");
    media.at = 60;
    expect(player.seekTo(120_000)).toBe(false);
    expect(media.at).toBe(60);
    await settle();
    sameCalls(calls, ["progress 2 stopped 60000", "delete t1", "start 2"]);
    expect(requests.slice(1).map((r) => [r.mode, r.start_ms])).toEqual([["transcode", 120_000]]);
    send("t2", "seek", { position_ms: 30_000 });
    await settle();
    expect(requests.slice(2).map((r) => [r.mode, r.start_ms])).toEqual([["transcode", 30_000]]);
  });

  it("ends the track's session before a seek that can't move within one progressive stream starts another, so a pause meanwhile pauses nothing", async () => {
    const { media, calls, send } = await world([1, 2, 3], 1, "transcode", { hls: false });
    media.at = 60;
    send("t1", "seek", { position_ms: 120_000 });
    send("t1", "pause");
    await settle();
    sameCalls(calls, ["progress 2 stopped 60000", "delete t1", "start 2"]);
    expect(media.paused).toBe(false);
  });

  it("obeys nothing once the server stopped it", async () => {
    const { player, media, calls, send } = await world([1, 2, 3], 1);
    player.pause();
    player.stoppedByServer("t1");
    await settle();
    calls.length = 0;
    send("t1", "play");
    send("t1", "seek", { position_ms: 0 });
    send("t1", "next");
    await settle();
    expect(media.paused).toBe(true);
    expect(calls).toEqual([]);
  });

  it("has no tracks to change: audio and subtitles do nothing", async () => {
    const { player, calls, requests } = await world([1, 2, 3], 1);
    player.obey({ type: "player_command", session_id: "t1", command: "audio", position_ms: null, offset_ms: null, stream_index: 1 });
    player.obey({ type: "player_command", session_id: "t1", command: "subtitles", position_ms: null, offset_ms: null, stream_index: -1 });
    await settle();
    expect([calls, requests.length]).toEqual([[], 1]);
  });

  it("starts the track a cast sent where it says", async () => {
    const { player, requests } = await world([1, 2, 3], 1);
    player.play([track(4), track(5)], 0, false, 30_000);
    await settle();
    expect(requests.slice(1).map((r) => [r.item_id, r.start_ms])).toEqual([[4, 30_000]]);
  });
});

describe("music's seeking and starting where it was", () => {
  it("starts a conversion where a cast says: the stream's playlist begins at 0, so the place is set once it knows its length", async () => {
    const { media, fire, requests } = await world([1, 2], 0, "transcode", { playsAtOnce: false, fromMs: 30_000 });
    expect(requests[0]!.start_ms).toBe(30_000);
    expect(media.at).toBe(0);
    fire("loadedmetadata");
    expect(media.at).toBe(30);
  });

  it("sets the place again as it first plays when the stream went back to its start meanwhile", async () => {
    const { media, fire } = await world([1, 2], 0, "transcode", { playsAtOnce: false, fromMs: 30_000 });
    fire("loadedmetadata");
    media.at = 0;
    fire("playing");
    expect(media.at).toBe(30);
    // Once it plays, a seek of the user's own is left alone.
    media.at = 5;
    fire("playing");
    expect(media.at).toBe(5);
  });

  it("holds a seek made before the file knows its length, shows it, and goes there once it does", async () => {
    const { player, media, fire, requests } = await world([1, 2], 0, "direct", { playsAtOnce: false });
    media.ready = 0;
    expect(player.seekTo(90_000)).toBe(true);
    expect(player.positionMs()).toBe(90_000);
    expect(media.at).toBe(0);
    media.ready = 1;
    fire("loadedmetadata");
    expect(media.at).toBe(90);
    expect(requests).toHaveLength(1);
  });

  it("holds a seek, without playing, when a video took the music's session, and plays from there when asked", async () => {
    const { player, media, requests } = await world([1, 2], 0, "transcode");
    player.yieldToVideo();
    expect(media.paused).toBe(true);
    player.seekTo(50_000);
    await settle();
    expect(requests).toHaveLength(1);
    expect(media.paused).toBe(true);
    expect(player.positionMs()).toBe(50_000);
    player.toggle();
    await settle();
    expect(requests.slice(1).map((r) => [r.item_id, r.start_ms])).toEqual([[1, 50_000]]);
  });

  it("starts a new session at the place when the track ended", async () => {
    const { player, media, fire, requests } = await world([1], 0, "transcode");
    media.at = 240;
    fire("ended");
    expect(player.seekTo(50_000)).toBe(false);
    await settle();
    expect(requests.slice(1).map((r) => [r.item_id, r.start_ms])).toEqual([[1, 50_000]]);
  });

  it("converts a file the TV couldn't decode for one session only: the next seek, play or try asks the server afresh", async () => {
    vi.useFakeTimers();
    const { player, fire, requests } = await world([1, 2], 0, "direct", { playsAtOnce: false });
    fire("error");
    await settle();
    expect(requests.slice(1).map((r) => [r.mode, r.start_ms])).toEqual([["transcode", undefined]]);
    fire("playing");
    expect(player.seekTo(120_000)).toBe(false);
    await settle();
    expect(requests.slice(2).map((r) => [r.mode, r.start_ms])).toEqual([["auto", 120_000]]);
    // Played as a file again: it fails, converts once more, and a pause and play there asks afresh too.
    fire("error");
    await settle();
    expect(requests.at(-1)!.mode).toBe("transcode");
    player.yieldToVideo();
    player.toggle();
    await settle();
    expect(requests.at(-1)!.mode).toBe("auto");
  });

  it("plays a file the TV can't decode once more as a conversion where it was to start, and tries that conversion again as one when it fails", async () => {
    vi.useFakeTimers();
    const { fire, requests } = await world([1, 2], 0, "direct", { playsAtOnce: false, fromMs: 30_000 });
    fire("error");
    await settle();
    expect(requests.slice(1).map((r) => [r.mode, r.start_ms])).toEqual([["transcode", 30_000]]);
    fire("error");
    await settle();
    await vi.advanceTimersByTimeAsync(1_100);
    // The file failed already: the server would pick it again.
    expect(requests.slice(2).map((r) => [r.mode, r.start_ms])).toEqual([["transcode", 30_000]]);
  });

  it("counts a track's failures afresh once it played on well past where it failed, so trouble long after the last doesn't end it", async () => {
    vi.useFakeTimers();
    const { player, media, fire, requests } = await world([1, 2], 0, "direct", { playsAtOnce: false });
    // The conversion, then the most new sessions a failing track gets.
    for (let i = 0; i < 3; i++) {
      fire("error");
      await settle();
      await vi.advanceTimersByTimeAsync(1_100);
    }
    expect(requests).toHaveLength(4);
    fire("playing");
    media.at = 20;
    fire("timeupdate");
    fire("error");
    await settle();
    await vi.advanceTimersByTimeAsync(1_100);
    expect(requests).toHaveLength(5);
    expect(player.now.value?.error).toBe(null);
  });

  it("gives up on a track that fails at the same place each time it begins to play", async () => {
    vi.useFakeTimers();
    const { player, media, fire, requests } = await world([1, 2], 0, "direct", { playsAtOnce: false });
    media.at = 150;
    for (let i = 0; i < 6; i++) {
      fire("playing");
      fire("timeupdate");
      fire("error");
      await settle();
      await vi.advanceTimersByTimeAsync(1_100);
    }
    expect(requests).toHaveLength(4);
    expect(player.now.value?.error).toBe("unplayable");
  });

  it("starts again where a seek made while it waits to try a failed track again went", async () => {
    vi.useFakeTimers();
    const { player, media, fire, requests } = await world([1, 2], 0, "transcode", { playsAtOnce: false });
    media.at = 60;
    fire("error");
    await settle();
    expect(player.seekTo(70_000)).toBe(true);
    await vi.advanceTimersByTimeAsync(1_100);
    expect(requests.slice(1).map((r) => r.start_ms)).toEqual([70_000]);
  });

  it("counts failures afresh for the next track", async () => {
    vi.useFakeTimers();
    const { player, fire, requests } = await world([1, 2], 0, "direct", { playsAtOnce: false });
    for (let i = 0; i < 4; i++) {
      fire("error");
      await settle();
      await vi.advanceTimersByTimeAsync(1_100);
    }
    expect(player.now.value?.error).toBe("unplayable");
    player.next();
    await settle();
    fire("error");
    await settle();
    expect(requests.at(-1)!.mode).toBe("transcode");
    expect(requests.at(-1)!.item_id).toBe(2);
  });

  it("keeps a new session's place from the stream it replaces, which may still say it plays", async () => {
    const { player, media, fire } = await world([1, 2], 0, "transcode", { hls: false, playsAtOnce: false });
    media.at = 60;
    expect(player.seekTo(120_000)).toBe(false);
    // The old stream comes back from a stall before the new session's address is set.
    fire("playing");
    await settle();
    media.at = 0;
    fire("loadedmetadata");
    expect(media.at).toBe(120);
  });

  it("takes the session's length while the stream hasn't told its own", async () => {
    const { player } = await world([1, 2], 0, "transcode", { durationMs: 200_000 });
    expect(player.durationMs()).toBe(200_000);
  });
});

describe("music plays the file's bytes and the TV moves within them", () => {
  it("moves within the file for a seek, with no new session", async () => {
    const { player, media, requests } = await world([1, 2], 0, "direct");
    media.at = 60;
    expect(player.seekTo(120_000)).toBe(true);
    await settle();
    expect(media.at).toBe(120);
    expect(requests).toHaveLength(1);
  });

  it("doesn't move a file again as it begins to play: the TV went where it was told", async () => {
    const { media, fire, audio } = await world([1, 2], 0, "direct", { playsAtOnce: false, fromMs: 30_000 });
    expect(audio.src.endsWith("#t=30")).toBe(true);
    fire("loadedmetadata");
    expect(media.at).toBe(30);
    media.at = 0;
    fire("playing");
    expect(media.at).toBe(0);
  });

  it("starts a file from its beginning without a start in its address", async () => {
    const { audio } = await world([1, 2], 0, "direct");
    expect(audio.src).not.toContain("#t=");
  });

  it("tries the file again where a seek went when it fails just after, and converts only if it fails there again", async () => {
    vi.useFakeTimers();
    const { player, media, fire, requests } = await world([1, 2], 0, "direct", { playsAtOnce: false });
    media.at = 60;
    player.seekTo(120_000);
    await vi.advanceTimersByTimeAsync(500);
    fire("error");
    await settle();
    expect(requests.slice(1).map((r) => [r.mode, r.start_ms])).toEqual([["auto", 120_000]]);
    fire("loadedmetadata");
    expect(media.at).toBe(120);
    fire("playing");
    media.at = 122;
    fire("error");
    await settle();
    await vi.advanceTimersByTimeAsync(1_100);
    expect(requests.slice(2).map((r) => [r.mode, r.start_ms])).toEqual([["transcode", 122_000]]);
  });

  it("doesn't give up on a track after a few seeks once its file failed: each seek tries the file once, then converts", async () => {
    vi.useFakeTimers();
    const { player, media, fire, requests } = await world([1, 2], 0, "direct", { playsAtOnce: false });
    media.at = 5;
    fire("error");
    await settle();
    expect(requests.at(-1)!.mode).toBe("transcode");
    fire("playing");
    for (const to of [60_000, 70_000, 80_000]) {
      const before = requests.length;
      expect(player.seekTo(to)).toBe(false);
      await settle();
      // The file again, from the place: its error comes just after the TV moved there.
      fire("loadedmetadata");
      fire("error");
      await settle();
      await vi.advanceTimersByTimeAsync(1_100);
      expect(requests.slice(before).map((r) => [r.mode, r.start_ms])).toEqual([
        ["auto", to],
        ["transcode", to],
      ]);
      fire("playing");
    }
    expect(player.now.value?.error).toBe(null);
  });

  it("starts the next track without waiting for the last one's reports to go out", async () => {
    const reporter = { send: () => undefined, drop: () => undefined, flush: () => new Promise<void>(() => undefined) };
    const { requests } = await world([1, 2], 0, "direct", { reporter });
    expect(requests).toHaveLength(1);
  });
});

describe("a seek in a file the TV's <audio> won't move in: the seek ladder", () => {
  /** The requests after the first, as [mode, start_ms]. */
  const asked = (requests: PlaybackRequest[], from = 1) => requests.slice(from).map((r) => [r.mode, r.start_ms]);

  it("moves a file that takes the move and goes no further: no reload, no new session, nothing remembered", async () => {
    vi.useFakeTimers();
    const store = memoryStore();
    const { player, media, requests, loads } = await world([1, 2], 0, "direct", { ext: "mp3", store });
    media.at = 60;
    player.seekTo(120_000);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(media.at).toBe(120);
    expect(loads).toHaveLength(1);
    expect(requests).toHaveLength(1);
    expect([...store.data.values()].join("")).not.toContain("mp3");
  });

  it("takes a file playing on past the place as landed", async () => {
    vi.useFakeTimers();
    const { player, media, loads } = await world([1, 2], 0, "direct", { ext: "mp3" });
    player.seekTo(120_000);
    media.at = 121.4;
    await vi.advanceTimersByTimeAsync(5_000);
    expect(loads).toHaveLength(1);
  });

  it("loads the file again from the place (#t) when the move didn't land, then converts from there when that didn't either", async () => {
    vi.useFakeTimers();
    const info = vi.spyOn(console, "info").mockImplementation(() => undefined);
    const { player, media, fire, requests, loads, sets } = await world([1, 2], 0, "direct", { ignoresSeeks: true });
    media.at = 64.6;
    expect(player.seekTo(102_693)).toBe(true);
    expect(sets.at(-1)).toBeCloseTo(102.693);
    await vi.advanceTimersByTimeAsync(1_400);
    expect(loads).toHaveLength(1);
    // The TV paused and played on from where it was.
    media.at = 65.8;
    await vi.advanceTimersByTimeAsync(200);
    expect(loads).toHaveLength(2);
    expect(loads[1]).toBe("http://nas:7539/api/media/pley_mk_1/file.flac#t=102.7");
    expect(player.positionMs()).toBe(102_693);
    fire("loadedmetadata");
    expect(sets.at(-1)).toBeCloseTo(102.693);
    fire("playing");
    expect(requests).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(3_000);
    expect(asked(requests)).toEqual([["transcode", 102_693]]);
    expect(info.mock.calls.filter((c) => c[0] === "[music] seek ladder").map((c) => c[1])).toEqual([
      { kind: "flac", step: 1, landed: false },
      { kind: "flac", step: 2, landed: false },
      { kind: "flac", step: 3, landed: null },
    ]);
  });

  it("never waits more than 3 s on the reload, even when it never plays", async () => {
    vi.useFakeTimers();
    const { player, requests, loads } = await world([1, 2], 0, "direct", { ignoresSeeks: true, playsAtOnce: false });
    player.seekTo(146_000);
    await vi.advanceTimersByTimeAsync(1_500);
    expect(loads).toHaveLength(2);
    await vi.advanceTimersByTimeAsync(2_999);
    expect(requests).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(1);
    await settle();
    expect(asked(requests)).toEqual([["transcode", 146_000]]);
  });

  it("goes straight to the conversion for the next seek in that kind of file, and plays the next track as the server picks", async () => {
    vi.useFakeTimers();
    const store = memoryStore();
    const { player, media, fire, requests, loads } = await world([1, 2, 3], 0, "direct", { ignoresSeeks: true, store });
    player.seekTo(100_000);
    await vi.advanceTimersByTimeAsync(1_500);
    // The reload can play, and plays from elsewhere: a plain failure, which teaches.
    fire("canplay");
    await vi.advanceTimersByTimeAsync(3_500);
    expect(asked(requests)).toEqual([["transcode", 100_000]]);
    fire("playing");
    // The file can't be moved in on this TV: the rest of the track's seeks convert too, rather than load the file at a place it won't go.
    const before = loads.length;
    player.seekTo(30_000);
    await settle();
    expect(asked(requests, 2)).toEqual([["transcode", 30_000]]);
    fire("playing");
    player.seekTo(200_000);
    await settle();
    expect(asked(requests, 3)).toEqual([["transcode", 200_000]]);
    expect(loads.slice(before).every((u) => u.endsWith(".m3u8"))).toBe(true);
    expect(media.at).toBe(0);
    player.next();
    await settle();
    expect(requests.at(-1)!.mode).toBe("auto");
    // Another player on this TV (the app opened again) remembers it too, once a plain move, tried first since it opened, fails.
    expect(JSON.parse(store.get("pley.audioSeeks")!)["audio:flac"].step).toBe(3);
    const again = await world([4, 5], 0, "direct", { ignoresSeeks: true, store });
    again.player.seekTo(50_000);
    expect(again.sets).toEqual([50]);
    await vi.advanceTimersByTimeAsync(1_500);
    again.player.next();
    await settle();
    again.player.seekTo(60_000);
    await settle();
    // The retried move failed and went on at the remembered conversion; the next track's seek goes there straight,
    // with no move in the file (the conversion is moved to the place as it plays).
    expect(asked(again.requests)).toEqual([["transcode", 50_000], ["auto", undefined], ["transcode", 60_000]]);
    expect(again.loads.at(-2)).not.toMatch(/#t=/);
    expect(again.loads.at(-1)).toMatch(/\.m3u8$/);
  });

  it("goes straight to the reload when it was the reload that landed, and doesn't remember other kinds", async () => {
    vi.useFakeTimers();
    const store = memoryStore();
    const first = await world([1, 2], 0, "direct", { ignoresSeeks: true, fragmentSeeks: true, store });
    first.player.seekTo(100_000);
    await vi.advanceTimersByTimeAsync(1_500);
    expect(first.loads).toHaveLength(2);
    first.fire("loadedmetadata");
    first.fire("playing");
    await vi.advanceTimersByTimeAsync(5_000);
    expect(first.requests).toHaveLength(1);
    expect(first.media.at).toBe(100);
    const w = await world([1, 2], 0, "direct", { ignoresSeeks: true, fragmentSeeks: true, store });
    // The first seek since the app opened tries a plain move once (the TV may seek after all); later ones go straight to the reload.
    w.player.seekTo(60_000);
    expect(w.sets).toEqual([60]);
    await vi.advanceTimersByTimeAsync(1_500);
    w.player.next();
    await settle();
    w.player.seekTo(90_000);
    expect(w.sets).toEqual([60]);
    expect(w.loads.at(-1)).toMatch(/#t=90$/);
    const mp3 = await world([1, 2], 0, "direct", { ext: "mp3", store });
    mp3.player.seekTo(60_000);
    expect(mp3.sets).toEqual([60]);
    expect(mp3.loads).toHaveLength(1);
  });

  it("goes straight on to a remembered conversion when the move tried again since the app opened fails", async () => {
    vi.useFakeTimers();
    const store = memoryStore();
    store.set("pley.audioSeeks", JSON.stringify({ "audio:flac": { step: 3, at: Date.now() } }));
    const { player, sets, loads, requests } = await world([1, 2], 0, "direct", { ignoresSeeks: true, store });
    player.seekTo(100_000);
    expect(sets).toEqual([100]);
    await vi.advanceTimersByTimeAsync(1_500);
    // No reload (step 2, known to fail here) before the conversion.
    expect(loads.filter((u) => /#t=/.test(u))).toEqual([]);
    expect(asked(requests)).toEqual([["transcode", 100_000]]);
  });

  it("restarts a track by loading the file again, not by a retried move, when a later step is remembered", async () => {
    vi.useFakeTimers();
    const store = memoryStore();
    store.set("pley.audioSeeks", JSON.stringify({ "audio:flac": { step: 2, at: Date.now() } }));
    const { player, sets, loads } = await world([1, 2], 0, "direct", { ignoresSeeks: true, store });
    const before = loads.length;
    player.seekTo(0);
    expect(sets).toEqual([]);
    expect(loads.length).toBe(before + 1);
    // And the memory stays.
    expect(JSON.parse(store.get("pley.audioSeeks")!)["audio:flac"].step).toBe(2);
  });

  it("learns nothing from a seek that was only slow: the element was still loading when its time ran out", async () => {
    vi.useFakeTimers();
    const store = memoryStore();
    const { player, media, requests } = await world([1, 2], 0, "direct", { reportsPending: true, store });
    player.seekTo(100_000);
    // Still seeking at 1.5 s, and the reload never gets to play in 3 s: a slow network.
    await vi.advanceTimersByTimeAsync(5_000);
    expect(media.pending).not.toBeNull();
    expect(asked(requests)).toEqual([["transcode", 100_000]]);
    expect(store.get("pley.audioSeeks") ?? "{}").not.toContain("audio:flac");
  });

  it("goes where a seek asked while the track was starting said, not where the start was", async () => {
    vi.useFakeTimers();
    const { player, loads } = await world([1, 2], 0, "direct", { playsAtOnce: false });
    player.next();
    expect(player.seekTo(90_000)).toBe(true);
    await settle();
    expect(loads.at(-1)).toMatch(/#t=90$/);
    expect(player.positionMs()).toBe(90_000);
  });

  it("tries a plain move again after 30 days, and forgets the kind once it lands", async () => {
    vi.useFakeTimers();
    const store = memoryStore();
    store.set("pley.audioSeeks", JSON.stringify({ "audio:flac": { step: 3, at: Date.now() - 30 * 24 * 3600_000 }, "audio:ogg": { step: 2, at: Date.now() } }));
    const w = await world([1], 0, "direct", { store });
    w.player.seekTo(100_000);
    expect(w.sets).toEqual([100]);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(w.requests).toHaveLength(1);
    expect(w.loads).toHaveLength(1);
    expect(JSON.parse(store.get("pley.audioSeeks")!)).toEqual({ "audio:ogg": { step: 2, at: expect.any(Number) } });
  });

  it("lets a newer seek replace a ladder under way: only the last place counts", async () => {
    vi.useFakeTimers();
    const { player, fire, requests, loads, sets } = await world([1, 2], 0, "direct", { ignoresSeeks: true });
    player.seekTo(100_000);
    await vi.advanceTimersByTimeAsync(1_000);
    player.seekTo(150_000);
    await vi.advanceTimersByTimeAsync(1_000);
    // The first ladder's check never came: still the newer move's.
    expect(loads).toHaveLength(1);
    expect(sets).toEqual([100, 150]);
    await vi.advanceTimersByTimeAsync(500);
    expect(loads.at(-1)).toMatch(/#t=150$/);
    fire("loadedmetadata");
    await vi.advanceTimersByTimeAsync(1_000);
    // Superseded again mid-reload: the reload goes on with the newest place; the move that failed for this file isn't tried again.
    player.seekTo(40_000);
    expect(loads.at(-1)).toMatch(/#t=40$/);
    expect(sets.at(-1)).toBe(150);
    await vi.advanceTimersByTimeAsync(3_000);
    await settle();
    expect(asked(requests)).toEqual([["transcode", 40_000]]);
  });

  it("plays on when the TV's storage fails, trying every step each time", async () => {
    vi.useFakeTimers();
    const broken: KeyValue = {
      get: () => {
        throw new Error("blocked");
      },
      set: () => {
        throw new Error("full");
      },
      remove: () => undefined,
    };
    const { player, fire, requests, sets } = await world([1, 2], 0, "direct", { ignoresSeeks: true, store: broken });
    player.seekTo(100_000);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(asked(requests)).toEqual([["transcode", 100_000]]);
    fire("playing");
    // The track converts for its other seeks; the next track tries a plain move again.
    player.seekTo(30_000);
    await settle();
    expect(asked(requests, 2)).toEqual([["transcode", 30_000]]);
    player.next();
    await settle();
    expect(requests.at(-1)!.mode).toBe("auto");
    player.seekTo(60_000);
    expect(sets.at(-1)).toBe(60);
  });

  it("drops the ladder with the track", async () => {
    vi.useFakeTimers();
    const { player, requests } = await world([1, 2], 0, "direct", { ignoresSeeks: true });
    player.seekTo(100_000);
    player.next();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(asked(requests)).toEqual([["auto", undefined]]);
  });
});

describe("a seek ladder while paused", () => {
  it("reloads the file without playing it when the track was paused, and still checks where it landed", async () => {
    vi.useFakeTimers();
    const { player, media, fire, requests, loads } = await world([1, 2], 0, "direct", { ignoresSeeks: true, fragmentSeeks: true });
    player.pause();
    player.seekTo(90_000);
    await vi.advanceTimersByTimeAsync(1_500);
    expect(loads).toHaveLength(2);
    expect(media.paused).toBe(true);
    fire("canplay");
    await vi.advanceTimersByTimeAsync(5_000);
    expect(media.at).toBe(90);
    expect(requests).toHaveLength(1);
  });
});

describe("a seek ladder's places, checks and memory (review findings)", () => {
  const asked = (requests: PlaybackRequest[], from = 1) => requests.slice(from).map((r) => [r.mode, r.start_ms]);
  const ladderLog = (info: { mock: { calls: unknown[][] } }) => info.mock.calls.filter((c) => c[0] === "[music] seek ladder").map((c) => c[1]);

  it("doesn't take the reload as landed while the element only reports the place it seeks to, and converts when it played from 0", async () => {
    vi.useFakeTimers();
    const info = vi.spyOn(console, "info").mockImplementation(() => undefined);
    const store = memoryStore();
    const { player, media, fire, requests } = await world([1, 2], 0, "direct", { ignoresSeeks: true, reportsPending: true, store });
    media.at = 64.6;
    player.seekTo(146_000);
    // The move: the TV gives up on it and plays on where it was.
    await vi.advanceTimersByTimeAsync(1_000);
    media.pending = null;
    await vi.advanceTimersByTimeAsync(500);
    // The reload: it says it is at the place as it can play, then plays from 0.
    fire("loadedmetadata");
    fire("canplay");
    expect(player.positionMs()).toBe(146_000);
    media.pending = null;
    media.at = 0;
    fire("playing");
    await vi.advanceTimersByTimeAsync(3_000);
    expect(asked(requests)).toEqual([["transcode", 146_000]]);
    expect(ladderLog(info)).toEqual([
      { kind: "flac", step: 1, landed: false },
      { kind: "flac", step: 2, landed: false },
      { kind: "flac", step: 3, landed: null },
    ]);
    expect(JSON.parse(store.get("pley.audioSeeks")!)["audio:flac"].step).toBe(3);
  });

  it("doesn't take a move as landed while the element still seeks", async () => {
    vi.useFakeTimers();
    const { player, loads } = await world([1, 2], 0, "direct", { ignoresSeeks: true, reportsPending: true });
    player.seekTo(146_000);
    await vi.advanceTimersByTimeAsync(1_600);
    expect(loads).toHaveLength(2);
  });

  it("takes a reload as landed once it plays on from the place", async () => {
    vi.useFakeTimers();
    const info = vi.spyOn(console, "info").mockImplementation(() => undefined);
    const { player, media, fire, requests } = await world([1, 2], 0, "direct", { ignoresSeeks: true, fragmentSeeks: true });
    player.seekTo(100_000);
    await vi.advanceTimersByTimeAsync(1_500);
    fire("playing");
    media.at = 100.3;
    fire("timeupdate");
    media.at = 100.6;
    fire("timeupdate");
    expect(ladderLog(info).at(-1)).toEqual({ kind: "flac", step: 2, landed: true });
    await vi.advanceTimersByTimeAsync(5_000);
    expect(requests).toHaveLength(1);
  });

  it("remembers the reload when a newer seek's ladder began there after the move failed", async () => {
    vi.useFakeTimers();
    const store = memoryStore();
    const { player, fire } = await world([1, 2], 0, "direct", { ignoresSeeks: true, fragmentSeeks: true, playsAtOnce: false, store });
    fire("playing");
    player.seekTo(100_000);
    await vi.advanceTimersByTimeAsync(1_500);
    player.seekTo(120_000);
    fire("playing");
    await vi.advanceTimersByTimeAsync(5_000);
    expect(JSON.parse(store.get("pley.audioSeeks")!)["audio:flac"].step).toBe(2);
  });

  it("starts again at the seek's place, not the element's, when the reload fails", async () => {
    vi.useFakeTimers();
    const { player, media, fire, requests } = await world([1, 2], 0, "direct", { ignoresSeeks: true });
    media.at = 64;
    player.seekTo(146_000);
    await vi.advanceTimersByTimeAsync(1_500);
    expect(media.at).toBe(0);
    fire("error");
    await settle();
    await vi.advanceTimersByTimeAsync(1_100);
    expect(requests.slice(1).map((r) => r.start_ms)).toEqual([146_000]);
  });

  it("starts the track again without a conversion, even where only a conversion moves the file", async () => {
    vi.useFakeTimers();
    const store = memoryStore();
    store.set("pley.audioSeeks", JSON.stringify({ "audio:flac": { step: 3, at: Date.now() } }));
    const w = await world([1, 2], 1, "direct", { ignoresSeeks: true, store });
    w.media.at = 20;
    w.send("t1", "previous");
    await vi.advanceTimersByTimeAsync(5_000);
    expect(w.requests.filter((r) => r.mode === "transcode")).toEqual([]);
    expect(w.loads.at(-1)).toBe("http://nas:7539/api/media/pley_mk_1/file.flac");
    expect(w.media.at).toBe(0);
    // And after a conversion took a seek (after the move tried once since the app opened), Previous plays the file from its start.
    w.player.seekTo(100_000);
    await vi.advanceTimersByTimeAsync(1_520);
    expect(asked(w.requests)).toEqual([["transcode", 100_000]]);
    w.fire("playing");
    w.media.at = 30;
    w.player.previous();
    await settle();
    expect(w.requests.at(-1)!.mode).toBe("auto");
    expect(w.requests.at(-1)!.start_ms).toBeUndefined();
  });

  it("starts the track again, not the one before, when Previous comes as a seek's reload begins", async () => {
    vi.useFakeTimers();
    const { player, requests } = await world([1, 2], 1, "direct", { ignoresSeeks: true });
    player.seekTo(120_000);
    await vi.advanceTimersByTimeAsync(1_600);
    player.previous();
    await settle();
    expect(requests.map((r) => r.item_id)).toEqual([2]);
  });

  it("checks where a file started at a place went, and converts when it didn't go there", async () => {
    vi.useFakeTimers();
    const store = memoryStore();
    const { fire, requests } = await world([1, 2], 0, "direct", { ignoresSeeks: true, playsAtOnce: false, fromMs: 90_000, store });
    fire("loadedmetadata");
    fire("playing");
    await vi.advanceTimersByTimeAsync(3_000);
    expect(asked(requests)).toEqual([["transcode", 90_000]]);
    // The next start at a place in that kind converts at once.
    const again = await world([3], 0, "direct", { ignoresSeeks: true, fromMs: 40_000, store });
    expect(asked(again.requests, 0)).toEqual([["auto", 40_000], ["transcode", 40_000]]);
  });

  it("starts again by conversion where it was when the track's seeks converted and a video took its session", async () => {
    vi.useFakeTimers();
    const { player, media, fire, requests } = await world([1, 2], 0, "direct", { ignoresSeeks: true });
    player.seekTo(100_000);
    await vi.advanceTimersByTimeAsync(5_000);
    fire("playing");
    // The conversion plays at the place (this fake moves no stream).
    media.at = 100;
    player.yieldToVideo();
    player.toggle();
    await settle();
    expect(asked(requests, 2)).toEqual([["transcode", 100_000]]);
  });

  it("judges a reload made while paused once it plays, not by the place the paused element reports", async () => {
    vi.useFakeTimers();
    const info = vi.spyOn(console, "info").mockImplementation(() => undefined);
    const store = memoryStore();
    const { player, media, fire, requests, loads } = await world([1, 2], 0, "direct", { ignoresSeeks: true, store });
    player.pause();
    player.seekTo(90_000);
    await vi.advanceTimersByTimeAsync(1_500);
    expect(loads).toHaveLength(2);
    // Paused, the element reads the place it was told.
    media.at = 90;
    fire("canplay");
    await vi.advanceTimersByTimeAsync(5_000);
    expect(ladderLog(info)).toEqual([{ kind: "flac", step: 1, landed: false }]);
    expect(store.get("pley.audioSeeks")).toBeNull();
    // It plays from the file's start.
    media.at = 0;
    player.toggle();
    await vi.advanceTimersByTimeAsync(2_500);
    await settle();
    expect(ladderLog(info).slice(1, 2)).toEqual([{ kind: "flac", step: 2, landed: false }]);
    expect(asked(requests)).toEqual([["transcode", 90_000]]);
  });
});

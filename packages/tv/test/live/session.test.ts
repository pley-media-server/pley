import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError, SessionStoppedError } from "../../src/api/client";
import type { LiveChannel, LiveWatching, ServerEvent } from "../../src/api/types";
import { holders, LiveController, troubleOf, type LiveServer } from "../../src/live/session";
import type { EngineListener, EngineSource, PlayerEngine } from "../../src/player/engine";

function channel(id: number): LiveChannel {
  return { id, source_id: 1, name: `Ch ${id}`, number: String(100 + id), group: "", logo: null, favorite: false, has_guide: false, now: null, next: null };
}

/** The TV's player, as the controller drives it: what it was asked, and a way to answer as the player would. */
class FakeEngine implements PlayerEngine {
  readonly name = "fake";
  listener: EngineListener | null = null;
  sources: EngineSource[] = [];
  calls: string[] = [];
  position = 0;
  width = 1280;
  setListener(l: EngineListener) {
    this.listener = l;
  }
  prepare(source: EngineSource) {
    this.sources.push(source);
    this.calls.push("prepare");
  }
  play() {
    this.calls.push("play");
  }
  pause() {
    this.calls.push("pause");
  }
  seekTo(ms: number) {
    this.calls.push(`seek ${ms}`);
    this.position = ms;
  }
  stop() {
    this.calls.push("stop");
  }
  release() {
    this.calls.push("release");
  }
  positionMs() {
    return this.position;
  }
  durationMs() {
    return 0;
  }
  bufferedMs() {
    return this.position;
  }
  status() {
    return "ready" as const;
  }
  playing() {
    return true;
  }
  audioTracks() {
    return [];
  }
  selectAudio() {
    return false;
  }
  frameCounts() {
    return null;
  }
  stats() {
    return this.width ? { width: this.width, height: 720 } : {};
  }
}

/** The server: each watch answered by `answer` (a session and a keyed address unless it says otherwise). */
function fakeServer(answer?: (id: number, n: number) => Promise<LiveWatching>) {
  const asked: string[] = [];
  /** Why the server says a session ended, by session ("" when it gave no words). */
  const ended: Record<string, string> = {};
  /** Sessions whose playlist answers `410`. */
  const gone = new Set<string>();
  let n = 0;
  const server: LiveServer = {
    watchChannel: (id, client, _cancel, takeOver) => {
      asked.push(`watch ${id} ${client}${takeOver !== undefined ? ` taking ${takeOver}` : ""}`);
      n++;
      return answer ? answer(id, n) : Promise.resolve({ session_id: `s${n}`, url: `/api/live/media/pley_mk_${n}/index.m3u8`, kind: "hls", channel: channel(id) });
    },
    leaveChannel: (session) => {
      asked.push(`leave ${session}`);
      return Promise.resolve();
    },
    media: (method, url) => {
      asked.push(`${method} ${url.replace("http://tv.test", "")}`);
      const session = /pley_mk_(\d+)/.exec(url)?.[1];
      // A keyed address only says the session has gone, not why.
      if (session && gone.has(`s${session}`)) return Promise.reject(new ApiError(410, "gone", "the server refused (HTTP 410)"));
      return Promise.resolve("#EXTM3U");
    },
    whyLiveEnded: (session) => Promise.resolve(ended[session] ?? null),
    url: (path) => `http://tv.test${path}`,
  };
  return { server, asked, ended, gone };
}

/** The server's event stream: who listens, and the server's sending. */
function fakeEvents() {
  const listeners: ((e: ServerEvent) => void)[] = [];
  const events = (l: (e: ServerEvent) => void) => {
    listeners.push(l);
    return () => {
      listeners.splice(listeners.indexOf(l), 1);
    };
  };
  const send = (e: ServerEvent) => listeners.slice().forEach((l) => l(e));
  return { events, send, listeners };
}

function setup(answer?: (id: number, n: number) => Promise<LiveWatching>) {
  const engine = new FakeEngine();
  const { server, asked, ended, gone } = fakeServer(answer);
  const { events, send, listeners } = fakeEvents();
  const c = new LiveController(channel(1), { server, engine: () => engine, clientId: "tizen-tv", events });
  return { c, engine, asked, ended, gone, send, listeners };
}

/** What the server sends each viewer of channel `id` that another's watch ended. */
function takenOver(id: number, session: string, message: string | null): ServerEvent {
  return { type: "live_taken_over", session_id: session, channel_id: id, message };
}

const watches = (asked: string[]) => asked.filter((a) => a.indexOf("watch") === 0);

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(console, "warn").mockImplementation(() => undefined);
});
afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("a live channel on the TV's player", () => {
  it("watches with this TV's id, waits for the playlist, then plays its keyed address from the live edge", async () => {
    const { c, engine, asked } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    expect(asked).toEqual(["watch 1 tizen-tv", "GET /api/live/media/pley_mk_1/index.m3u8"]);
    expect(engine.sources[0]).toMatchObject({ url: "http://tv.test/api/live/media/pley_mk_1/index.m3u8", hls: true, live: true, startMs: 0 });
    expect(c.state.value.phase).toBe("tuning");
    engine.listener!.firstFrame();
    expect(c.state.value).toMatchObject({ phase: "playing", shown: true, audioOnly: false });
  });

  it("changing channels lets the last one go first, and holds one session", async () => {
    const { c, engine, asked } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    c.tune(channel(2));
    await vi.advanceTimersByTimeAsync(0);
    expect(asked.slice(2)).toEqual(["leave s1", "watch 2 tizen-tv", "GET /api/live/media/pley_mk_2/index.m3u8"]);
    expect(engine.calls).toContain("stop");
    expect(c.state.value).toMatchObject({ phase: "tuning", shown: false, tunes: 2 });
    expect(c.state.value.channel.id).toBe(2);
  });

  it("says the server's own words when every connection is in use, and tries again when asked", async () => {
    let busy = true;
    const { c, asked } = setup((id, n) =>
      busy
        ? Promise.reject(new ApiError(409, "conflict", "The one connection of Provider is in use, watching News."))
        : Promise.resolve({ session_id: `s${n}`, url: "/api/live/media/pley_mk_x/index.m3u8", kind: "hls", channel: channel(id) }),
    );
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    expect(c.state.value.phase).toBe("error");
    expect(c.state.value.trouble).toMatchObject({ kind: "busy", title: "All connections are in use", message: "The one connection of Provider is in use, watching News.", canRetry: true });
    busy = false;
    c.retry();
    await vi.advanceTimersByTimeAsync(0);
    expect(c.state.value.phase).toBe("tuning");
    expect(asked.filter((a) => a.indexOf("watch") === 0)).toHaveLength(2);
  });

  it("lists what holds each connection, and takes one over where it may", async () => {
    const connections = [
      { channel_id: 7, channel_name: "Sport", watchers: [], people: 1, yours: true, can_take_over: true },
      { channel_id: 8, channel_name: "Films", watchers: ["ana", "bo"], people: 2, yours: false, can_take_over: false },
    ];
    const { c, asked } = setup((id, n) =>
      n === 1
        ? Promise.reject(new ApiError(409, "conflict", "Provider allows 2 connections at once.", { code: "conflict", message: "", connections }))
        : Promise.resolve({ session_id: `s${n}`, url: "/api/live/media/pley_mk_x/index.m3u8", kind: "hls", channel: channel(id) }),
    );
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    expect(c.state.value.trouble?.connections).toEqual(connections);
    expect(c.state.value.trouble?.connections?.map(holders)).toEqual(["You, on another device", "ana, bo"]);
    c.takeOver(7);
    await vi.advanceTimersByTimeAsync(0);
    expect(asked.filter((a) => a.indexOf("watch") === 0)).toEqual(["watch 1 tizen-tv", "watch 1 tizen-tv taking 7"]);
    expect(c.state.value.phase).toBe("tuning");
  });

  it("asks a server without keyed addresses to be updated, rather than failing in the player", async () => {
    const { c, engine } = setup((id) => Promise.resolve({ session_id: "old", url: "/api/live/sessions/old/index.m3u8", kind: "hls", channel: channel(id) }));
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    expect(c.state.value.trouble?.kind).toBe("older");
    expect(engine.sources).toHaveLength(0);
  });

  it("watches anew when the player fails, twice, then says so", async () => {
    const { c, engine, asked } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    engine.listener!.error({ kind: "network", detail: "gone" });
    await vi.advanceTimersByTimeAsync(1_000);
    engine.listener!.error({ kind: "network", detail: "gone" });
    await vi.advanceTimersByTimeAsync(3_000);
    expect(asked.filter((a) => a.indexOf("watch") === 0)).toHaveLength(3);
    engine.listener!.error({ kind: "network", detail: "gone" });
    await vi.advanceTimersByTimeAsync(0);
    expect(c.state.value.phase).toBe("error");
    expect(c.state.value.trouble).toMatchObject({ title: "Ch 1 stopped", canRetry: true });
    // A session that ends with an error is let go.
    expect(asked[asked.length - 1]).toBe("leave s3");
  });

  it("says when the server stopped it", async () => {
    const { c, engine } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.error({ kind: "stopped", message: "Stopped by the administrator." });
    await vi.advanceTimersByTimeAsync(0);
    expect(c.state.value.trouble).toMatchObject({ kind: "stopped", title: "Live TV stopped", message: "Stopped by the administrator." });
  });

  it("says who took the connection over, as the server tells it, rather than watching again", async () => {
    const { c, engine, asked, ended } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    ended.s1 = "Sam is watching Films now, on the connection this channel used.";
    engine.listener!.error({ kind: "network", detail: "the playlist went" });
    await vi.advanceTimersByTimeAsync(5_000);
    expect(c.state.value.trouble).toMatchObject({ kind: "stopped", title: "Live TV stopped", message: "Sam is watching Films now, on the connection this channel used." });
    expect(watches(asked)).toHaveLength(1);
    // Its session is gone already: nothing to leave.
    expect(asked).not.toContain("leave s1");
  });

  it("says the channel isn't being watched any more when the server ended it without words", async () => {
    const { c, engine, ended } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    ended.s1 = "";
    engine.listener!.error({ kind: "network", detail: "the playlist went" });
    await vi.advanceTimersByTimeAsync(0);
    expect(c.state.value.trouble).toMatchObject({ kind: "stopped", title: "Live TV stopped", message: "This channel isn't being watched any more.", canRetry: true });
    // A `410` the playlist answered with no words says the same.
    expect(troubleOf(new SessionStoppedError(null), "Ch 1").message).toBe("This channel isn't being watched any more.");
    expect(troubleOf(new SessionStoppedError("Watching moved to Films on another device."), "Ch 1").message).toBe("Watching moved to Films on another device.");
  });

  it("asks why when the playlist has gone before it plays: the keyed address doesn't say", async () => {
    const { c, engine, asked, ended, gone } = setup();
    gone.add("s1");
    ended.s1 = "Someone else needed this channel's connection to watch another channel.";
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    expect(c.state.value.trouble).toMatchObject({ kind: "stopped", message: "Someone else needed this channel's connection to watch another channel." });
    expect(engine.sources).toHaveLength(0);
    expect(asked).not.toContain("leave s1");
  });
});

describe("a live channel taken over by another viewer", () => {
  it("stops at once on the server's event, says its words, and doesn't watch again by itself", async () => {
    const { c, engine, asked, send } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    send(takenOver(1, "s1", "Sam is watching Two now, on the connection this channel used."));
    expect(c.state.value.phase).toBe("error");
    expect(c.state.value.trouble).toEqual({ kind: "stopped", title: "Live TV stopped", message: "Sam is watching Two now, on the connection this channel used.", canRetry: true });
    expect(engine.calls[engine.calls.length - 1]).toBe("stop");
    // The stream breaking off after changes nothing: no retry, no other words.
    engine.listener!.error({ kind: "network", detail: "the stream ended" });
    engine.listener!.status("ended");
    await vi.advanceTimersByTimeAsync(60_000);
    expect(watches(asked)).toHaveLength(1);
    expect(c.state.value.trouble?.message).toBe("Sam is watching Two now, on the connection this channel used.");
    // Its session is gone on the server: nothing to leave.
    expect(asked).not.toContain("leave s1");
  });

  it("matches the channel, not the session: a stream's reader doesn't know its own", async () => {
    const { c, engine, send } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    send(takenOver(1, "a session of another device", "Watching moved to Two on another device."));
    expect(c.state.value.trouble).toMatchObject({ kind: "stopped", message: "Watching moved to Two on another device." });
  });

  it("leaves another channel's take-over alone", async () => {
    const { c, engine, asked, send } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    send(takenOver(2, "s9", "Sam is watching Two now, on the connection this channel used."));
    expect(c.state.value.phase).toBe("playing");
    expect(c.state.value.trouble).toBeNull();
    expect(asked).toHaveLength(2);
  });

  it("shows one alert however many of the channel's viewers the server ended", async () => {
    const { c, engine, asked, send } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    send(takenOver(1, "s1", "Sam is watching Two now, on the connection this channel used."));
    const shown = c.state.value;
    send(takenOver(1, "a stream's reader", "Sam is watching Two now, on the connection this channel used."));
    expect(c.state.value).toBe(shown);
    expect(engine.calls.filter((call) => call === "stop")).toHaveLength(1);
    expect(watches(asked)).toHaveLength(1);
  });

  it("takes the event's words over a 410 that came first without any", async () => {
    const { c, engine, ended, send } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    ended.s1 = "";
    engine.listener!.error({ kind: "network", detail: "the playlist went" });
    await vi.advanceTimersByTimeAsync(0);
    expect(c.state.value.trouble?.message).toBe("This channel isn't being watched any more.");
    send(takenOver(1, "s1", "Watching moved to Two on another device."));
    expect(c.state.value.trouble).toMatchObject({ kind: "stopped", message: "Watching moved to Two on another device." });
  });

  it("keeps the words of a stop that came first when a take-over's event comes after", async () => {
    const { c, engine, ended, send } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    ended.s1 = "The channel was stopped.";
    engine.listener!.error({ kind: "network", detail: "the playlist went" });
    await vi.advanceTimersByTimeAsync(0);
    const shown = c.state.value;
    expect(shown.trouble).toMatchObject({ kind: "stopped", message: "The channel was stopped." });
    // Later, the same account's phone watches the channel and is taken over.
    send(takenOver(1, "s7", "Watching moved to Two on another device."));
    expect(c.state.value).toBe(shown);
  });

  it("keeps its card when the TV hides the app and shows it again, watching nothing until Watch again", async () => {
    const { c, engine, asked, send } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    send(takenOver(1, "s1", "Watching moved to Two on another device."));
    const shown = c.state.value;
    c.suspend();
    c.resume();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(c.state.value).toBe(shown);
    expect(watches(asked)).toHaveLength(1);
    c.retry();
    await vi.advanceTimersByTimeAsync(0);
    expect(watches(asked)).toHaveLength(2);
  });

  it("isn't watched again when the event comes in the second before a failure's retry", async () => {
    const { c, engine, asked, send } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    // The stream broke off first; the server didn't say yet that watching ended.
    engine.listener!.error({ kind: "network", detail: "the stream ended" });
    await vi.advanceTimersByTimeAsync(500);
    expect(watches(asked)).toHaveLength(1);
    send(takenOver(1, "s1", "Someone else needed this channel's connection to watch another channel."));
    await vi.advanceTimersByTimeAsync(10_000);
    expect(watches(asked)).toHaveLength(1);
    expect(c.state.value.trouble).toMatchObject({ kind: "stopped", message: "Someone else needed this channel's connection to watch another channel." });
  });

  it("stops a channel still tuning, whose watch the take-over ended", async () => {
    let answer: ((w: LiveWatching) => void) | null = null;
    const { c, engine, asked, send } = setup(() => new Promise((resolve) => (answer = resolve)));
    c.tune(channel(1));
    send(takenOver(1, "s1", "Watching moved to Two on another device."));
    expect(c.state.value.trouble).toMatchObject({ kind: "stopped" });
    answer!({ session_id: "late", url: "/api/live/media/pley_mk_late/index.m3u8", kind: "hls", channel: channel(1) });
    await vi.advanceTimersByTimeAsync(0);
    // The watch that answers after is let go, never played.
    expect(engine.sources).toHaveLength(0);
    expect(asked).toEqual(["watch 1 tizen-tv", "leave late"]);
  });

  it("watches again only when asked, as any watch: never taking a connection over by itself", async () => {
    const connections = [{ channel_id: 2, channel_name: "Two", watchers: ["sam"], people: 1, yours: false, can_take_over: false }];
    const { c, engine, asked, send } = setup((id, n) =>
      n === 1
        ? Promise.resolve({ session_id: "s1", url: "/api/live/media/pley_mk_1/index.m3u8", kind: "hls", channel: channel(id) })
        : Promise.reject(new ApiError(409, "conflict", "Provider allows one connection at once, and it is in use: watching Two.", { code: "conflict", message: "", connections })),
    );
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    send(takenOver(1, "s1", "Sam is watching Two now, on the connection this channel used."));
    c.retry();
    await vi.advanceTimersByTimeAsync(0);
    expect(watches(asked)).toEqual(["watch 1 tizen-tv", "watch 1 tizen-tv"]);
    expect(c.state.value.trouble).toMatchObject({ kind: "busy", connections });
  });

  it("stops listening when the player closes", async () => {
    const { c, listeners } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    expect(listeners).toHaveLength(1);
    c.close();
    expect(listeners).toHaveLength(0);
  });

});

describe("a live channel that stalls or fails", () => {
  it("asks the server why a stall lasts, the TV's player saying nothing of a playlist gone", async () => {
    const { c, engine, asked, ended } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    engine.listener!.status("buffering");
    await vi.advanceTimersByTimeAsync(5_000);
    // Still watching on the server: it waits on.
    expect(c.state.value.phase).toBe("buffering");
    engine.listener!.status("ready");
    engine.listener!.status("buffering");
    ended.s1 = "Watching moved to Films on another device.";
    await vi.advanceTimersByTimeAsync(5_000);
    expect(c.state.value.trouble).toMatchObject({ kind: "stopped", message: "Watching moved to Films on another device." });
    expect(asked.filter((a) => a.indexOf("watch") === 0)).toHaveLength(1);
  });

  it("watches anew after a stall of 20 s", async () => {
    const { c, engine, asked } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    engine.listener!.status("buffering");
    await vi.advanceTimersByTimeAsync(21_000);
    expect(asked.filter((a) => a.indexOf("watch") === 0)).toHaveLength(2);
  });

  it("gives up on a channel whose picture never comes", async () => {
    const { c } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(30_000);
    // The first try failing is tried again, as any failure.
    expect(c.state.value.phase).toBe("tuning");
  });

  it("knows a channel with sound and no picture", async () => {
    const { c, engine } = setup();
    engine.width = 0;
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    await vi.advanceTimersByTimeAsync(1_500);
    expect(c.state.value.audioOnly).toBe(true);
  });

  it("plays from the live edge and nothing else: no pause, no seek, nothing asked of the server while it plays", async () => {
    const { c, engine, asked } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    expect(c.state.value.phase).toBe("playing");
    expect(engine.sources[0]?.startMs).toBe(0);
    const before = asked.length;
    await vi.advanceTimersByTimeAsync(120_000);
    expect(asked).toHaveLength(before);
    expect(engine.calls.filter((call) => call === "pause" || call.startsWith("seek"))).toEqual([]);
  });

  it("lets go of the session when the TV hides the app, and watches again when it shows", async () => {
    const { c, asked } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    c.suspend();
    expect(asked[asked.length - 1]).toBe("leave s1");
    c.resume();
    await vi.advanceTimersByTimeAsync(0);
    expect(asked.slice(-2)).toEqual(["watch 1 tizen-tv", "GET /api/live/media/pley_mk_2/index.m3u8"]);
  });

  it("tells the server at once when the player closes, and drops a watch that answers after", async () => {
    let answer: ((w: LiveWatching) => void) | null = null;
    const { c, engine, asked } = setup(() => new Promise((resolve) => (answer = resolve)));
    c.tune(channel(1));
    c.close();
    answer!({ session_id: "late", url: "/api/live/media/pley_mk_late/index.m3u8", kind: "hls", channel: channel(1) });
    await vi.advanceTimersByTimeAsync(0);
    expect(asked).toEqual(["watch 1 tizen-tv", "leave late"]);
    expect(engine.sources).toHaveLength(0);
  });
});

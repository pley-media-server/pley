import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../../src/api/client";
import type { LiveChannel, LiveWatching } from "../../src/api/types";
import { holders, LiveController, type LiveServer } from "../../src/live/session";
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
  /** Why the server says a session ended, by session. */
  const ended: Record<string, string> = {};
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
      return Promise.resolve("#EXTM3U");
    },
    whyLiveEnded: (session) => Promise.resolve(ended[session] ?? null),
    url: (path) => `http://tv.test${path}`,
  };
  return { server, asked, ended };
}

function setup(answer?: (id: number, n: number) => Promise<LiveWatching>) {
  const engine = new FakeEngine();
  const { server, asked, ended } = fakeServer(answer);
  const c = new LiveController(channel(1), { server, engine: () => engine, clientId: "tizen-tv" });
  return { c, engine, asked, ended };
}

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
    expect(c.state.value.trouble).toMatchObject({ kind: "stopped", message: "Stopped by the administrator." });
  });

  it("says who took the connection over, as the server tells it, rather than watching again", async () => {
    const { c, engine, asked, ended } = setup();
    c.tune(channel(1));
    await vi.advanceTimersByTimeAsync(0);
    engine.listener!.firstFrame();
    ended.s1 = "ana started watching Films on this connection.";
    engine.listener!.error({ kind: "network", detail: "the playlist went" });
    await vi.advanceTimersByTimeAsync(5_000);
    expect(c.state.value.trouble).toMatchObject({ kind: "stopped", title: "Watching stopped", message: "ana started watching Films on this connection." });
    expect(asked.filter((a) => a.indexOf("watch") === 0)).toHaveLength(1);
    // Its session is gone already: nothing to leave.
    expect(asked).not.toContain("leave s1");
  });

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

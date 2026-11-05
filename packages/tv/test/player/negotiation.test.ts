import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError, type Api } from "../../src/api/client";
import type { ClientCaps, ItemDetail, MediaFile, MediaStream, PlaybackInfo, PlaybackRequest, PlayingTracks, ServerEvent } from "../../src/api/types";
import type { KeyValue } from "../../src/auth/tokens";
import { PlaybackController, type PlayerEnv } from "../../src/player/controller";
import type { EngineListener, EngineSource, EngineStatus, PlayerEngine } from "../../src/player/engine";
import { dvContainerMode, dvRetryMode, initialAudio, initialSubtitle, pictureUrlAt, playbackRequest, playingAudio, withTracks, type Choices, type PlayRequest } from "../../src/player/plan";
import { storedMemory, type Remembered } from "../../src/player/autoquality";
import type { NetworkMemory } from "../../src/player/network";
import { ORIGINAL } from "../../src/player/quality";
import { Reporter } from "../../src/player/reporter";
import { NetworkError } from "../../src/net/transport";
import { OffsetStore } from "../../src/subs/sync";

// ---- fixtures

function stream(over: Partial<MediaStream>): MediaStream {
  return { index: 0, kind: "video", codec: "hevc", language: null, title: null, default: true, forced: false, ...over };
}

function file(over: Partial<MediaFile> = {}): MediaFile {
  return {
    id: 7,
    path: "/media/Film (2024).mkv",
    size: 40_000_000_000,
    container: "mkv",
    duration_ms: 7_200_000,
    bitrate: 40_000_000,
    width: 3840,
    height: 2160,
    video_codec: "hevc",
    audio_codec: "eac3",
    streams: [
      stream({ index: 0, kind: "video", codec: "hevc", width: 3840, height: 2160, bit_depth: 10, profile: "Main 10", dynamic_range: "hdr10", level: 153 }),
      stream({ index: 1, kind: "audio", codec: "eac3", language: "eng", channels: 6 }),
      stream({ index: 2, kind: "audio", codec: "truehd", language: "eng", channels: 8, default: false }),
      stream({ index: 3, kind: "subtitle", codec: "hdmv_pgs_subtitle", language: "eng", default: false, text_based: false }),
      stream({ index: 4, kind: "subtitle", codec: "subrip", language: "eng", default: false, text_based: true }),
    ],
    edition: null,
    selected_audio: null,
    selected_subtitle: null,
    chapters: [],
    ...over,
  };
}

function dvFile(profile: number, container = "mkv"): MediaFile {
  const f = file({ container });
  f.streams[0] = stream({ index: 0, kind: "video", codec: "hevc", width: 3840, height: 2160, bit_depth: 10, profile: "Main 10", dynamic_range: "dolby_vision", dv_profile: profile, dv_bl_compat: profile === 8 ? 1 : 0, level: 153 });
  return f;
}

function item(f: MediaFile, over: Partial<ItemDetail> = {}): ItemDetail {
  return {
    id: 70,
    library_id: 1,
    kind: "movie",
    title: "Film",
    sort_title: "film",
    year: 2024,
    added_at: 0,
    user_state: { position_ms: 600_000, watched: false, play_count: 0 },
    duration_ms: 7_200_000,
    original_title: null,
    tagline: null,
    genres: [],
    studio: null,
    content_rating: null,
    files: [f],
    ...over,
  };
}

/** An LG OLED of 2023 (webOS 23): HDR10, HLG, Dolby Vision 5 and 8, Dolby audio to the receiver. */
const LG: ClientCaps = {
  containers: ["mkv", "mp4", "ts"],
  video_codecs: ["h264", "hevc", "av1"],
  audio_codecs: ["aac", "ac3", "eac3"],
  hdr: true,
  dolby_vision: true,
  caps_version: 2,
  video: [{ codec: "hevc", hardware: true, profiles: ["Main", "Main 10"], max_width: 4096, max_height: 2160, max_fps: 60, max_bit_depth: 10, max_level: 153 }],
  hdr_types: ["hdr10", "hlg", "dolby_vision"],
  dolby_vision_profiles: [{ profile: 5 }, { profile: 8 }],
  audio_decode: ["aac", "ac3", "eac3"],
  audio_passthrough: ["ac3", "eac3"],
  audio_channels: 8,
  subtitle_codecs: ["subrip", "hdmv_pgs_subtitle"],
  audio_track_switching: true,
  picture_subtitles_sideload: true,
  hls: { master_playlist: true, segments: ["ts", "fmp4"] },
  device: { form: "tv", tier: "high", fire_tv: false, model: "OLED65C3PUA" },
};

function choices(over: Partial<Choices> = {}): Choices {
  const f = file();
  return { file: f, startMs: 600_000, audio: null, subtitle: null, quality: ORIGINAL, mode: "auto", streaming: false, sideload: true, ...over };
}

function info(over: Partial<PlaybackInfo> = {}): PlaybackInfo {
  return {
    session_id: "s1",
    method: "direct",
    url: "/api/media/pley_mk_KEY/file.mkv",
    hls: false,
    file_id: 7,
    duration_ms: 7_200_000,
    audio_index: 1,
    subtitle_index: null,
    burn_subtitles: false,
    subtitles: [],
    reason: null,
    ...over,
  };
}

// ---- the request

describe("the audio to start with", () => {
  /** A remux's tracks: TrueHD Atmos first and default, its E-AC-3, a commentary. */
  function remux(): MediaFile {
    const f = file();
    f.streams = [
      f.streams[0]!,
      stream({ index: 1, kind: "audio", codec: "truehd", language: "eng", channels: 8, default: true }),
      stream({ index: 2, kind: "audio", codec: "ac3", language: "eng", channels: 2, default: false, title: "Director's Commentary" }),
      stream({ index: 3, kind: "audio", codec: "eac3", language: "eng", channels: 6, default: false }),
      stream({ index: 4, kind: "audio", codec: "eac3", language: "fra", channels: 6, default: false }),
    ];
    return f;
  }

  it("is the track asked for; else none, for the server to choose the one this TV plays best", () => {
    expect(initialAudio(remux(), 2)?.index).toBe(2);
    expect(initialAudio(remux(), null)).toBeNull();
    expect(initialAudio(remux(), 9)).toBeNull();
  });

  it("is named in the request only when there is one", () => {
    const f = remux();
    expect(playbackRequest(70, choices({ file: f, audio: initialAudio(f, 3) }), "webos-abc", LG, true).audio_index).toBe(3);
    expect(playbackRequest(70, choices({ file: f, audio: initialAudio(f, null) }), "webos-abc", LG, true).audio_index).toBeUndefined();
  });

  it("is, once the session starts, the one the server says plays", () => {
    expect(playingAudio(remux(), info({ audio_index: 3 }))?.index).toBe(3);
    expect(playingAudio(remux(), info({ audio_index: null }))?.index).toBe(1);
  });
});

describe("tracks picked on the title's page", () => {
  it("are the ones a playback started from its details begins with, the other versions left alone", () => {
    const picked = withTracks(item(file()), 7, 2, 4);
    const f = picked.files[0]!;
    // The subtitles from the details kept; the audio is the server's to choose, the pick saved with it.
    expect([initialAudio(f, null), initialSubtitle(f, null)?.index]).toEqual([null, 4]);
    expect(initialSubtitle(withTracks(picked, 7, 2, null).files[0]!, null)).toBeNull();
    expect(withTracks(item(file()), 8, 2, 4).files[0]).toEqual(file());
  });

  it("say whether the audio is a pick: Automatic picks none, subtitles alone leave it as it was", () => {
    const picked = withTracks(item(file()), 7, 2, 4).files[0]!;
    expect([picked.selected_audio, picked.audio_picked]).toEqual([2, true]);
    const automatic = withTracks(item(file()), 7, null, 4).files[0]!;
    expect([automatic.selected_audio, automatic.audio_picked]).toEqual([null, false]);
    const subsAlone = withTracks(item(file({ selected_audio: 2, audio_picked: true })), 7, undefined, null).files[0]!;
    expect([subsAlone.selected_audio, subsAlone.audio_picked, subsAlone.selected_subtitle]).toEqual([2, true, null]);
  });
});

describe("the play request", () => {
  it("asks for keyed addresses and sends the TV's capabilities, with no track indexes for a direct play", () => {
    const req = playbackRequest(70, choices(), "webos-abc", LG, true);
    expect(req).toEqual({ item_id: 70, file_id: 7, start_ms: 600_000, mode: "auto", client_id: "webos-abc", caps: LG, keyed_urls: true });
  });

  it("names the audio track picked, even in a direct play", () => {
    const f = file();
    const req = playbackRequest(70, choices({ audio: f.streams[2]! }), "c", LG, true);
    expect(req.audio_index).toBe(2);
  });

  it("names picture subtitles to burn in only when the TV can't draw them from their .mks", () => {
    const f = file();
    const pgs = f.streams[3]!;
    expect(playbackRequest(70, choices({ subtitle: pgs, streaming: true, sideload: true }), "c", LG, true).subtitle_index).toBeUndefined();
    expect(playbackRequest(70, choices({ subtitle: pgs, streaming: true, sideload: false }), "c", LG, true).subtitle_index).toBe(3);
    expect(playbackRequest(70, choices({ subtitle: f.streams[4]!, streaming: true, sideload: false }), "c", LG, true).subtitle_index).toBeUndefined();
  });
});

describe("Dolby Vision", () => {
  it("asks for a remux of Dolby Vision in Matroska where the TV shows it only from MP4 and TS (LG before webOS 25)", () => {
    expect(dvContainerMode(dvFile(8), LG, ["mp4", "ts"])).toBe("remux");
    expect(dvContainerMode(dvFile(8, "mp4"), LG, ["mp4", "ts"])).toBeNull();
    expect(dvContainerMode(dvFile(8), LG, ["mp4", "ts", "mkv"])).toBeNull();
    expect(dvContainerMode(dvFile(8), LG, null)).toBeNull();
  });

  it("leaves files alone on a TV that shows no Dolby Vision, and HDR10 files everywhere", () => {
    const samsung = { ...LG, hdr_types: ["hdr10", "hdr10plus", "hlg"] as ClientCaps["hdr_types"], dolby_vision: false, dolby_vision_profiles: [] };
    expect(dvContainerMode(dvFile(8), samsung, ["mp4"])).toBeNull();
    expect(dvContainerMode(file(), LG, ["mp4"])).toBeNull();
  });

  it("keeps the Android app's guards: profile 7 played as it is becomes a remux to its HDR10 base", () => {
    expect(dvRetryMode(info({ method: "direct" }), dvFile(7), LG)).toBe("remux");
    expect(dvRetryMode(info({ method: "remux", hls: true }), dvFile(5), LG)).toBeNull();
    const noMaster = { ...LG, hls: { master_playlist: false, segments: ["ts"] } };
    expect(dvRetryMode(info({ method: "remux", hls: true }), dvFile(5), noMaster)).toBe("transcode");
  });
});

// ---- the controller, with a pretend server and player

class FakeEngine implements PlayerEngine {
  readonly name = "Fake";
  listener: EngineListener | null = null;
  prepared: EngineSource[] = [];
  state: EngineStatus = "idle";
  released = false;
  setListener(l: EngineListener) {
    this.listener = l;
  }
  prepare(source: EngineSource) {
    this.prepared.push(source);
    this.state = "buffering";
  }
  /** Times it was asked to keep its picture through the next prepare. */
  held = 0;
  holdPicture() {
    this.held++;
  }
  play() {}
  pause() {}
  seekTo() {}
  stop() {
    this.state = "idle";
  }
  release() {
    this.released = true;
  }
  /** Where it plays; tests that let time pass move it on. */
  pos = 600_000;
  positionMs() {
    return this.pos;
  }
  durationMs() {
    return 7_200_000;
  }
  /** How far it has loaded; tests of Automatic move it on. */
  buffered = 600_000;
  bufferedMs() {
    return this.buffered;
  }
  /** What its player measured the network carrying (Samsung's), kbps; `null` when it can't say. */
  bandwidth: number | null = null;
  bandwidthKbps() {
    return this.bandwidth;
  }
  status() {
    return this.state;
  }
  playing() {
    return this.state === "ready";
  }
  audioTracks() {
    return [];
  }
  /** The audio tracks switched to, by their place among the file's. */
  switched: number[] = [];
  selectAudio(position: number) {
    this.switched.push(position);
    return true;
  }
  /** Dropped and total frames, when a test counts them. */
  frames: { dropped: number; total: number } | null = null;
  frameCounts() {
    return this.frames;
  }
  stats() {
    return null;
  }
}

function memory(): KeyValue {
  const m = new Map<string, string>();
  return { get: (k) => m.get(k) ?? null, set: (k, v) => void m.set(k, v), remove: (k) => void m.delete(k) };
}

function world(detail: ItemDetail, answer: (req: PlaybackRequest) => PlaybackInfo, dvContainers: string[] | null = null) {
  const calls: string[] = [];
  const requests: PlaybackRequest[] = [];
  const listeners: ((e: ServerEvent) => void)[] = [];
  /** The tracks saved with PUT /api/items/{id}/streams: audio (undefined: left out), subtitles. */
  const saved: [number | null | undefined, number | null][] = [];
  /** What each progress report said plays, in order: "stopped 600000 audio 1 subtitles -1". */
  const reported: string[] = [];
  /** The Quality menu's pick each progress report said, in order: "playing 8m". */
  const qualities: string[] = [];
  /**
   * `down`: nothing answers (the Wi-Fi dropped, the server restarts). `busy`: no transcode free for a session asked for
   * beside another (503). `transcoderGone`: the server answers, but starts no session until its transcoder is back (503).
   */
  const net = { down: false, busy: false, transcoderGone: false };
  /** How a stream's playlists and segments are answered (`api.media`); at once, unless a test says. */
  const media = { answer: (method: string, url: string): Promise<string> => Promise.resolve(method === "HEAD" ? "" : playlistFor(url)) };
  const answered = <T>(value: () => T): Promise<T> => (net.down ? Promise.reject(new NetworkError("Can't reach the server")) : Promise.resolve(value()));
  /** How PUT /api/playback/{id}/bitrate is answered: what the stream goes on at, as asked unless a test says. */
  const bitrate = { answer: (kbps: number): Promise<{ bitrate: number }> => Promise.resolve({ bitrate: kbps }) };
  const api = {
    server: "http://nas:7539",
    url: (p: string) => (/^https?:/.test(p) ? p : `http://nas:7539${p}`),
    item: async () => detail,
    play: (req: PlaybackRequest) =>
      answered(() => {
        requests.push(req);
        calls.push(`start ${req.mode}`);
        if (net.busy && req.keep_previous) throw new ApiError(503, "unavailable", "no transcode free");
        if (net.transcoderGone) throw new ApiError(503, "unavailable", "the transcoder is not reachable", { code: "unavailable", message: "the transcoder is not reachable" });
        return answer(req);
      }),
    media: (method: string, url: string) => {
      calls.push(`${method} ${url.replace("http://nas:7539/api/media/", "")}`);
      return media.answer(method, url);
    },
    progress: async (_id: number, pos: number, _d: number | null, state: string, _session: string | null, tracks?: PlayingTracks) => {
      calls.push(`progress ${state} ${Math.round(pos)}`);
      reported.push(`${state} ${Math.round(pos)} audio ${tracks?.audio ?? "none"} subtitles ${tracks?.subtitle ?? "none"}`);
      qualities.push(`${state} ${tracks?.quality ?? "none"}`);
    },
    stopSession: async (id: string) => void calls.push(`delete ${id}`),
    setBitrate: (id: string, kbps: number) => answered(() => calls.push(`bitrate ${id} ${kbps}`)).then(() => bitrate.answer(kbps)),
    sessionStatus: () => answered(() => ({ session_id: "s1", method: "direct", transcode: null })),
    reachable: () => answered(() => ({})),
    setStreams: async (_item: number, _file: number | null, audio: number | null | undefined, subtitle: number | null) => void saved.push([audio, subtitle]),
    // Subtitle files: never answered here.
    send: () => new Promise(() => undefined),
    trickplay: async () => {
      throw new Error("not ready");
    },
    next: async () => ({ item: null }),
  } as unknown as Api;
  const engine = new FakeEngine();
  const env: PlayerEnv = {
    api,
    reporter: new Reporter(),
    engine: () => engine,
    caps: async () => LG,
    keyedUrls: true,
    dvContainers: () => dvContainers,
    clientId: "webos-abc",
    offsets: new OffsetStore(memory(), () => 1),
    quality: { remembered: () => "original", remember: () => undefined },
    autoplayNext: () => true,
    events: (l) => {
      listeners.push(l);
      return () => undefined;
    },
  };
  return { env, engine, calls, requests, saved, reported, qualities, net, media, bitrate, send: (e: ServerEvent) => listeners.forEach((l) => l(e)) };
}

/** A stream in two sizes, each in 4 s segments over the film's two hours. */
function playlistFor(url: string): string {
  if (/master\.m3u8$/.test(url)) {
    return [
      "#EXTM3U",
      "#EXT-X-STREAM-INF:BANDWIDTH=4400000,RESOLUTION=1280x720",
      "v0/index.m3u8",
      "#EXT-X-STREAM-INF:BANDWIDTH=2200000,RESOLUTION=854x480",
      "v1/index.m3u8",
    ].join("\n");
  }
  const lines = ["#EXTM3U", "#EXT-X-TARGETDURATION:4"];
  for (let n = 0; n < 1800; n++) lines.push("#EXTINF:4.000,", `${n}.ts`);
  lines.push("#EXT-X-ENDLIST");
  return lines.join("\n");
}

const settle = () => new Promise((r) => setTimeout(r, 20));

describe("a playback session", () => {
  it("starts at the resume point from the keyed address the server gave, with the container's type", async () => {
    const w = world(item(file()), () => info());
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    expect(w.requests[0]).toMatchObject({ item_id: 70, file_id: 7, start_ms: 600_000, keyed_urls: true, client_id: "webos-abc" });
    expect(w.engine.prepared[0]).toMatchObject({ url: "http://nas:7539/api/media/pley_mk_KEY/file.mkv", hls: false, mime: "video/x-matroska", startMs: 600_000 });
    c.close();
  });

  it("asks for a remux of Dolby Vision in Matroska on a TV that shows it only from MP4 and TS", async () => {
    const w = world(item(dvFile(8)), (req) => info({ method: req.mode === "remux" ? "remux" : "direct", hls: req.mode === "remux", url: "/api/media/pley_mk_KEY/master.m3u8" }), ["mp4", "ts"]);
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    expect(w.requests.map((r) => r.mode)).toEqual(["remux"]);
    expect(w.engine.prepared[0]).toMatchObject({ hls: true });
    c.close();
  });

  it("plays the audio the server chose for this TV, and its menu says so", async () => {
    // The server's pick: the TrueHD, second of the file's audio tracks (a receiver that takes it).
    const w = world(item(file()), () => info({ audio_index: 2 }));
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    expect(w.requests[0]!.audio_index).toBeUndefined();
    expect(w.engine.switched).toEqual([1]);
    expect(c.state.value.audio.filter((a) => a.selected).map((a) => a.index)).toEqual([2]);
    c.close();
  });

  it("lets the server choose the audio again for a lower quality, a stream that copies another track", async () => {
    // The TrueHD as it is; converted at a lower quality, the server copies the first track instead.
    const w = world(item(file()), (req) => info({ audio_index: req.max_bitrate ? 1 : 2 }));
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    c.selectQuality("4m");
    await settle();
    expect(w.requests.length).toBe(2);
    expect(w.requests[1]!.max_bitrate).toBeDefined();
    expect(w.requests[1]!.audio_index).toBeUndefined();
    expect(c.state.value.audio.filter((a) => a.selected).map((a) => a.index)).toEqual([1]);
    c.close();
  });

  it("saves subtitles picked without the audio the server chose, and the audio once picked", async () => {
    const w = world(item(file()), () => info());
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    c.selectSubtitle(4);
    c.selectAudio(2);
    c.selectSubtitle(null);
    await settle();
    expect(w.saved).toEqual([
      [undefined, 4],
      [2, 4],
      [2, null],
    ]);
    // And the request names it from then on.
    expect(w.requests[w.requests.length - 1]!.audio_index).toBe(2);
    c.close();
  });

  it("refuses a server too old for keyed addresses, ending its session unplayed", async () => {
    const w = world(item(file()), () => info({ url: "/api/files/7/stream" }));
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    expect(c.state.value.phase).toMatchObject({ kind: "error", canRetry: false });
    expect(w.engine.prepared).toEqual([]);
    expect(w.calls).toContain("delete s1");
    expect(w.calls.filter((x) => x.indexOf("progress") === 0)).toEqual([]);
    c.close();
  });

  it("reports the end, then ends the session, when the player closes", async () => {
    const w = world(item(file()), () => info());
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    c.close();
    await settle();
    expect(w.calls).toEqual(["start auto", "progress stopped 600000", "delete s1"]);
    expect(w.engine.released).toBe(true);
  });

  it("stops for good when the server stops the session, keeping the resume point", async () => {
    const w = world(item(file()), () => info());
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    w.send({ type: "session_stopped", session_id: "s1", message: "Bedtime", by_admin: true });
    await settle();
    const phase = c.state.value.phase;
    expect(phase).toMatchObject({ kind: "error", canRetry: false });
    expect(phase.kind === "error" && phase.message).toContain("Bedtime");
    expect(c.state.value.stoppedByServer).toBe(true);
    expect(w.calls.filter((x) => x.indexOf("delete") === 0)).toEqual([]);
    c.close();
  });

  it("tries again with a conversion when the TV can't decode the file", async () => {
    const w = world(item(file()), (req) =>
      info(req.mode === "auto" ? {} : { session_id: "s2", method: req.mode === "remux" ? "remux" : "transcode", hls: true, url: "/api/media/pley_mk_KEY2/master.m3u8" }),
    );
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    w.engine.listener?.error({ kind: "unsupported", audio: false, container: false, detail: "decoder" });
    await new Promise((r) => setTimeout(r, 2_200));
    expect(w.requests.map((r) => r.mode)).toEqual(["auto", "transcode"]);
    expect(w.engine.prepared[1]).toMatchObject({ hls: true, url: "http://nas:7539/api/media/pley_mk_KEY2/master.m3u8" });
    c.close();
  });

  it("starts one session when AVPlay says one failure twice", async () => {
    const w = world(item(file()), (req) =>
      info(req.mode === "auto" ? {} : { session_id: "s2", method: req.mode === "remux" ? "remux" : "transcode", hls: true, url: "/api/media/pley_mk_KEY2/master.m3u8" }),
    );
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    // Its onerror, and the call it failed in: the second a step further down the ladder, without the guard.
    w.engine.listener?.error({ kind: "unsupported", audio: true, container: false, detail: "audio" });
    w.engine.listener?.error({ kind: "unsupported", audio: true, container: false, detail: "audio" });
    await new Promise((r) => setTimeout(r, 2_200));
    expect(w.requests.map((r) => r.mode)).toEqual(["auto", "remux"]);
    c.close();
  });

});

describe("the server out of reach", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  /** A playback that has shown its picture, on a clock the test moves. */
  async function playing() {
    vi.useFakeTimers();
    const w = world(item(file()), () => info());
    w.env.now = () => Date.now();
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await vi.advanceTimersByTimeAsync(50);
    w.engine.state = "ready";
    w.engine.listener?.status("ready");
    return { w, c };
  }

  it("keeps the place and waits for the server, then starts again where it was", async () => {
    const { w, c } = await playing();
    w.net.down = true;
    w.engine.state = "buffering";
    w.engine.listener?.error({ kind: "network", detail: "connection failed" });
    await vi.advanceTimersByTimeAsync(10);
    expect(c.state.value.reconnecting).toBe(true);
    expect(c.state.value.phase.kind).toBe("buffering");
    // Away 30 s: no session spent, no conversion, no error.
    await vi.advanceTimersByTimeAsync(30_000);
    expect(w.requests.length).toBe(1);
    expect(c.state.value.phase.kind).toBe("buffering");
    w.net.down = false;
    await vi.advanceTimersByTimeAsync(2_100);
    expect(c.state.value.reconnecting).toBe(false);
    expect(w.requests.map((r) => [r.mode, r.start_ms])).toEqual([
      ["auto", 600_000],
      ["auto", 600_000],
    ]);
    c.close();
  });

  it("says so after 90 s, and Try again starts over", async () => {
    const { w, c } = await playing();
    w.net.down = true;
    w.engine.listener?.error({ kind: "network", detail: "connection failed" });
    await vi.advanceTimersByTimeAsync(89_000);
    expect(c.state.value.phase.kind).toBe("buffering");
    await vi.advanceTimersByTimeAsync(3_000);
    expect(c.state.value.phase).toMatchObject({ kind: "error", title: "Can't reach your server", canRetry: true });
    expect(c.state.value.reconnecting).toBe(false);
    w.net.down = false;
    c.retry();
    await vi.advanceTimersByTimeAsync(2_500);
    expect(w.requests.length).toBe(2);
    expect(c.state.value.phase.kind).not.toBe("error");
    c.close();
  });

  it("says so at once when the server is off as play is pressed", async () => {
    vi.useFakeTimers();
    const w = world(item(file()), () => info());
    w.net.down = true;
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await vi.advanceTimersByTimeAsync(50);
    expect(c.state.value.phase).toMatchObject({ kind: "error", title: "Can't reach your server", canRetry: true });
    c.close();
  });

  it("waits for the server, rather than converting the video, when a stall finds it gone", async () => {
    const { w, c } = await playing();
    w.net.down = true;
    w.engine.state = "buffering";
    // The TV's own player says nothing: 12 s without moving.
    await vi.advanceTimersByTimeAsync(13_000);
    expect(c.state.value.reconnecting).toBe(true);
    w.net.down = false;
    await vi.advanceTimersByTimeAsync(2_100);
    expect(w.requests.map((r) => r.mode)).toEqual(["auto", "auto"]);
    c.close();
  });

  it("waits for the transcoder as for the server when it restarts mid-film (503), then starts again where it was", async () => {
    const { w, c } = await playing();
    w.net.transcoderGone = true;
    w.engine.state = "buffering";
    w.engine.listener?.error({ kind: "http", status: 503, detail: "segment" });
    await vi.advanceTimersByTimeAsync(10);
    expect(c.state.value.reconnecting).toBe(true);
    expect(c.state.value.phase.kind).toBe("buffering");
    await vi.advanceTimersByTimeAsync(20_000);
    expect(c.state.value.reconnecting).toBe(true);
    w.net.transcoderGone = false;
    await vi.advanceTimersByTimeAsync(2_100);
    expect(c.state.value.reconnecting).toBe(false);
    expect(c.state.value.phase.kind).not.toBe("error");
    expect(w.requests[w.requests.length - 1]!.start_ms).toBe(600_000);
    c.close();
  });

  it("says what the server said once its transcoder has been gone 90 s, however often the server itself answered", async () => {
    const { w, c } = await playing();
    w.net.transcoderGone = true;
    w.engine.listener?.error({ kind: "http", status: 503, detail: "segment" });
    await vi.advanceTimersByTimeAsync(30_000);
    // Asked every 2 s by then, not as often as the server answers.
    const asked = w.requests.length;
    await vi.advanceTimersByTimeAsync(10_000);
    expect(w.requests.length - asked).toBeLessThanOrEqual(5);
    await vi.advanceTimersByTimeAsync(48_000);
    expect(c.state.value.phase.kind).toBe("buffering");
    await vi.advanceTimersByTimeAsync(4_000);
    expect(c.state.value.phase).toMatchObject({ kind: "error", message: "The transcoder is not reachable", canRetry: true });
    expect(c.state.value.reconnecting).toBe(false);
    c.close();
  });
});

describe("dropped frames", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  /**
   * A file played as it is, its picture shown and its frames counted, on a
   * clock the test moves: E-AC-3, and an AC-3 track the TV plays too.
   */
  async function counting() {
    vi.useFakeTimers();
    const f = file();
    f.streams[2] = stream({ index: 2, kind: "audio", codec: "ac3", language: "eng", channels: 6, default: false });
    const w = world(item(f), () => info());
    w.env.now = () => Date.now();
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await vi.advanceTimersByTimeAsync(50);
    const frames = { dropped: 0, total: 0 };
    w.engine.frames = frames;
    w.engine.state = "ready";
    w.engine.listener?.firstFrame();
    w.engine.listener?.status("ready");
    /** `seconds` of playback at 24 frames a second, `dropped` of them dropped as it starts. */
    const play = async (seconds: number, dropped = 0) => {
      frames.dropped += dropped;
      for (let i = 0; i < seconds * 2; i++) {
        frames.total += 12;
        w.engine.pos += 500;
        await vi.advanceTimersByTimeAsync(500);
      }
    };
    return { w, c, play };
  }

  it("convert a video that drops many", async () => {
    const { w, c, play } = await counting();
    await play(4);
    await play(14, 130);
    expect(w.requests.map((r) => r.mode).slice(0, 2)).toEqual(["auto", "transcode"]);
    c.close();
  });

  it("aren't counted as a track switch drops them (a desktop Chrome's 130)", async () => {
    const { w, c, play } = await counting();
    await play(4);
    c.selectAudio(2);
    expect(w.engine.switched.slice(-1)).toEqual([1]);
    await play(14, 130);
    expect(w.requests.map((r) => r.mode)).toEqual(["auto"]);
    // Counted again after: a video that drops them later still converts.
    await play(14, 130);
    expect(w.requests.map((r) => r.mode).slice(0, 2)).toEqual(["auto", "transcode"]);
    c.close();
  });
});

describe("stalls (Info's count)", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("count a playing video waiting for more, not the wait a seek of its own brings", async () => {
    vi.useFakeTimers();
    const w = world(item(file()), () => info());
    w.env.now = () => Date.now();
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await vi.advanceTimersByTimeAsync(50);
    const now = (s: EngineStatus) => {
      w.engine.state = s;
      w.engine.listener?.status(s);
    };
    now("ready");
    expect(c.state.value.phase.kind).toBe("playing");
    now("buffering");
    now("ready");
    expect(c.state.value.stalls).toBe(1);
    c.seekTo(1_200_000);
    now("buffering");
    now("ready");
    expect(c.state.value.stalls).toBe(1);
    await vi.advanceTimersByTimeAsync(3_000);
    now("buffering");
    expect(c.state.value.stalls).toBe(2);
    c.close();
  });
});

describe("Automatic", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  /** The original as it is; a conversion asked for, or the original converted at the server's quality for it. */
  const converted = (req: PlaybackRequest) =>
    info(
      req.mode === "auto"
        ? {}
        : { session_id: `s${req.max_bitrate ?? "t"}`, method: "transcode", hls: true, url: "/api/media/pley_mk_KEY2/master.m3u8", auto_quality: req.max_bitrate ? null : "30m" },
    );

  /** A playback in Automatic that has shown its picture, on a clock the test moves; `kept` is the TV's storage. */
  async function automatic(detail = item(file()), kept = memory(), dv: string[] | null = null, network: NetworkMemory | null = null, answer = converted) {
    vi.useFakeTimers();
    const w = world(detail, answer, dv);
    w.env.now = () => Date.now();
    const picked: string[] = [];
    w.env.quality = { remembered: () => "auto", remember: (id) => void picked.push(id) };
    w.env.autoMemory = storedMemory(kept, "pley.autoQuality.http://nas:7539");
    w.env.network = network;
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await vi.advanceTimersByTimeAsync(50);
    const now = (s: EngineStatus) => {
      w.engine.state = s;
      w.engine.listener?.status(s);
    };
    now("ready");
    w.engine.buffered = w.engine.pos + 8_000;
    let shown = 1;
    /** `seconds` of playback, the video arriving at `delivery` seconds a second; a new session shows at once. */
    const play = async (seconds: number, delivery: number) => {
      for (let i = 0; i < seconds * 2; i++) {
        if (w.engine.prepared.length > shown) {
          shown = w.engine.prepared.length;
          now("ready");
        }
        w.engine.pos += 500;
        w.engine.buffered += 500 * delivery;
        await vi.advanceTimersByTimeAsync(500);
      }
    };
    /** The picture stops for want of data for `ms`. */
    const wait = async (ms: number) => {
      now("buffering");
      await vi.advanceTimersByTimeAsync(ms);
    };
    return { w, c, now, play, wait, kept, picked };
  }

  it("plays the original, and its menu says so, in the menu's order", async () => {
    const { w, c } = await automatic();
    expect(w.requests.map((r) => [r.mode, r.max_bitrate])).toEqual([["auto", undefined]]);
    expect(c.state.value.quality).toBe("Automatic · Original");
    expect(c.state.value.qualities.map((q) => q.id)).toEqual(["original", "auto", "30m", "15m", "20m", "12m", "8m", "4m", "2m"]);
    expect(c.state.value.qualities.filter((q) => q.selected).map((q) => q.id)).toEqual(["auto"]);
    c.close();
  });

  it("keeps the Dolby Vision guard of the original (LG before webOS 25)", async () => {
    const { w, c } = await automatic(item(dvFile(8)), memory(), ["mp4", "ts"]);
    expect(w.requests.map((r) => r.mode)).toEqual(["remux"]);
    c.close();
  });

  it("steps down on a 2 s wait, from where it was, and says so once", async () => {
    const { w, c, play, wait, kept } = await automatic();
    const notices: string[] = [];
    c.state.subscribe((s) => {
      if (s.notice && notices[notices.length - 1] !== s.notice) notices.push(s.notice);
    });
    // A network measured at 20 Mbps, under 1.5 × the file's 40.
    w.engine.bandwidth = 20_000;
    // 40 Mbps arriving at half speed: 40 × 0.5 × 0.75 = 15 → 15 Mbps 4K.
    await play(12, 0.5);
    await wait(2_500);
    await vi.advanceTimersByTimeAsync(2_500);
    expect(w.requests.map((r) => [r.mode, r.max_bitrate, r.max_height])).toEqual([
      ["auto", undefined, undefined],
      ["transcode", 15_000, 2160],
    ]);
    expect(w.requests[1]!.start_ms).toBeGreaterThan(600_000);
    // Made ready beside the original, which ends once the step's stream is there.
    expect(w.requests[1]!.keep_previous).toBe(true);
    expect(w.calls.indexOf("delete s1")).toBeGreaterThan(w.calls.indexOf("start transcode"));
    expect(notices).toEqual(["Quality lowered to 15 Mbps 4K"]);
    expect(c.state.value.quality).toBe("Automatic · 15 Mbps 4K");
    expect(c.state.value.note).toBe("Lowered for your connection");
    expect(c.state.value.qualities.filter((q) => q.id === "auto")[0]!.hint).toBe("Now 15 Mbps 4K · Lowered for your connection");
    expect(JSON.parse(kept.get("pley.autoQuality.http://nas:7539")!)).toMatchObject({ kbps: 15_000 });
    c.close();
  });

  it("plays the original on through a wait on a network that carries 1.5 times the file: the server is why it waits", async () => {
    const { w, c, play, wait, now } = await automatic();
    w.engine.bandwidth = 80_000;
    await play(12, 0.5);
    await wait(2_500);
    await vi.advanceTimersByTimeAsync(2_500);
    now("ready");
    await play(4, 0.5);
    expect(w.requests.map((r) => r.mode)).toEqual(["auto"]);
    expect(c.state.value.quality).toBe("Automatic · Original");
    c.close();
  });

  it("doesn't count the wait of a seek, nor waits while paused", async () => {
    const { w, c, play, wait, now } = await automatic();
    await play(6, 1.2);
    c.seekTo(1_200_000);
    w.engine.pos = 1_200_000;
    w.engine.buffered = 1_200_000;
    await wait(3_000);
    now("ready");
    c.pause();
    await wait(3_000);
    await vi.advanceTimersByTimeAsync(3_000);
    expect(w.requests.length).toBe(1);
    c.close();
  });

  it("leaves the original's dropped frames to the fallback, and lowers a conversion that drops them", async () => {
    const { w, c, play, now } = await automatic();
    const frames = { dropped: 0, total: 0 };
    w.engine.frames = frames;
    w.engine.listener?.firstFrame();
    const drop = async (seconds: number, dropped: number) => {
      frames.dropped += dropped;
      for (let i = 0; i < seconds * 2; i++) {
        frames.total += 12;
        await play(0.5, 1.2);
      }
    };
    await drop(4, 0);
    await drop(14, 130);
    await vi.advanceTimersByTimeAsync(2_500);
    // As ever: the server's own quality for the file, once.
    expect(w.requests.map((r) => [r.mode, r.max_bitrate])).toEqual([
      ["auto", undefined],
      ["transcode", undefined],
    ]);
    now("ready");
    // Within 20 s of that change, nothing more.
    frames.dropped = 0;
    frames.total = 0;
    await drop(14, 130);
    expect(w.requests.length).toBe(2);
    // Then the conversion, at 30 Mbps 4K, goes one step down.
    await drop(4, 0);
    await drop(14, 130);
    await vi.advanceTimersByTimeAsync(2_500);
    expect(w.requests.map((r) => [r.mode, r.max_bitrate]).slice(2)).toEqual([["transcode", 15_000]]);
    expect(c.state.value.note).toBe("The original didn't play smoothly on this device.");
    c.close();
  });

  it("starts the next title at the remembered network, and picking Automatic again starts afresh", async () => {
    const kept = memory();
    const first = await automatic(item(file()), kept);
    first.w.engine.bandwidth = 20_000;
    await first.play(12, 0.5);
    await first.wait(2_500);
    await vi.advanceTimersByTimeAsync(2_500);
    first.c.close();
    vi.useRealTimers();

    const next = await automatic(item(file()), kept);
    expect(next.w.requests.map((r) => [r.mode, r.max_bitrate])).toEqual([["transcode", 15_000]]);
    expect(next.c.state.value.quality).toBe("Automatic · 15 Mbps 4K");
    next.c.selectQuality("auto");
    await vi.advanceTimersByTimeAsync(2_500);
    expect(next.w.requests.map((r) => [r.mode, r.max_bitrate]).slice(1)).toEqual([["auto", undefined]]);
    expect(kept.get("pley.autoQuality.http://nas:7539")).toBeNull();
    expect(next.picked).toEqual(["auto"]);
    next.c.close();
  });

  /** The server's memory of networks: `known` where Automatic may start; what it was told. */
  function serverMemory(known: Remembered | null | undefined) {
    const told: (number | null)[] = [];
    let asked = 0;
    const network: NetworkMemory = {
      start: () => {
        asked++;
        return Promise.resolve(known);
      },
      report: (kbps) => void told.push(kbps),
    };
    return { network, told, asked: () => asked };
  }

  it("starts where the server says this network allows, over the TV's own memory", async () => {
    const kept = memory();
    kept.set("pley.autoQuality.http://nas:7539", JSON.stringify({ kbps: 3_000, at: Date.now() - 60_000 }));
    const server = serverMemory({ kbps: 16_000, at: Date.now() - 3_600_000 });
    const { w, c } = await automatic(item(file()), kept, null, server.network);
    expect(server.asked()).toBe(1);
    expect(w.requests.map((r) => [r.mode, r.max_bitrate])).toEqual([["transcode", 15_000]]);
    expect(c.state.value.note).toBe("Lowered for your connection");
    c.close();
    vi.useRealTimers();

    const none = serverMemory(null);
    const fresh = await automatic(item(file()), kept, null, none.network);
    expect(fresh.w.requests.map((r) => [r.mode, r.max_bitrate])).toEqual([["auto", undefined]]);
    fresh.c.close();
  });

  it("goes by the TV's own memory when the server can't say", async () => {
    const kept = memory();
    kept.set("pley.autoQuality.http://nas:7539", JSON.stringify({ kbps: 9_000, at: Date.now() - 60_000 }));
    const { w, c } = await automatic(item(file()), kept, null, serverMemory(undefined).network);
    expect(w.requests.map((r) => [r.mode, r.max_bitrate])).toEqual([["transcode", 8_000]]);
    c.close();
  });

  it("asks nothing of the network for a quality picked", async () => {
    vi.useFakeTimers();
    const server = serverMemory({ kbps: 2_000, at: 0 });
    const w = world(item(file()), converted);
    w.env.network = server.network;
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await vi.advanceTimersByTimeAsync(50);
    expect(server.asked()).toBe(0);
    expect(w.requests.map((r) => r.mode)).toEqual(["auto"]);
    c.close();
  });

  it("tells the server the ceiling of a step down, and to forget it when Automatic is picked again", async () => {
    const server = serverMemory(null);
    const { w, c, play, wait } = await automatic(item(file()), memory(), null, server.network);
    w.engine.bandwidth = 20_000;
    await play(12, 0.5);
    await wait(2_500);
    await vi.advanceTimersByTimeAsync(2_500);
    expect(server.told).toEqual([15_000]);
    c.selectQuality("auto");
    expect(server.told).toEqual([15_000, null]);
    c.close();
  });

  it("doesn't start again when Original is picked while it plays the original", async () => {
    const { w, c, picked } = await automatic();
    c.selectQuality("original");
    await vi.advanceTimersByTimeAsync(2_500);
    expect(w.requests.length).toBe(1);
    expect(picked).toEqual(["original"]);
    expect(c.state.value.quality).toBe("Original");
    c.close();
  });

  describe("on Samsung and LG, whose players go black between streams", () => {
    /** As `converted`, from a server whose conversions' bitrate changes as they are sent. */
    const liveConverted = (req: PlaybackRequest) => ({ ...converted(req), live_bitrate: req.mode !== "auto" });
    /** The original, converted by the server at 30 Mbps 4K (a codec the TV doesn't decode), its bitrate changing as it is sent. */
    const convertsOriginal = () => info({ method: "transcode", hls: true, url: "/api/media/pley_mk_KEY2/master.m3u8", auto_quality: "30m", live_bitrate: true });
    /** A 1080p film of 20 Mbps. */
    function hd(): MediaFile {
      const f = file({ bitrate: 20_000_000, width: 1920, height: 1080 });
      f.streams[0] = stream({ index: 0, kind: "video", codec: "hevc", width: 1920, height: 1080 });
      return f;
    }
    const bitrates = (w: ReturnType<typeof world>) => w.calls.filter((x) => x.indexOf("bitrate ") === 0);
    const noticesOf = (c: PlaybackController) => {
      const notices: string[] = [];
      c.state.subscribe((s) => {
        if (s.notice && notices[notices.length - 1] !== s.notice) notices.push(s.notice);
      });
      return notices;
    };
    const autoHint = (c: PlaybackController) => c.state.value.qualities.filter((q) => q.id === "auto")[0]!.hint;

    it("steps a conversion's bitrate as it is sent: no new stream, the same size, and the menu says what plays", async () => {
      const server = serverMemory({ kbps: 9_000, at: Date.now() });
      const { w, c, now, play, wait } = await automatic(item(hd()), memory(), null, server.network, liveConverted);
      expect(w.requests.map((r) => [r.mode, r.max_bitrate, r.max_height])).toEqual([["transcode", 8_000, 1080]]);
      expect(c.state.value.quality).toBe("Automatic · 8 Mbps 1080p");
      const notices = noticesOf(c);
      // 8 Mbps arriving at 0.7 of its pace: 8 × 0.7 × 0.75 = 4.2 → 4 Mbps, still 1080p.
      await play(12, 0.7);
      await wait(2_500);
      now("ready");
      expect(bitrates(w)).toEqual(["bitrate s8000 4000"]);
      expect(w.requests.length).toBe(1);
      expect(w.engine.prepared.length).toBe(1);
      expect(c.state.value.quality).toBe("Automatic · 4 Mbps 1080p");
      expect(c.state.value.note).toBe("Lowered for your connection");
      expect(autoHint(c)).toBe("Now 4 Mbps 1080p · Lowered for your connection");
      expect(notices).toEqual(["Quality lowered to 4 Mbps 1080p"]);
      // What the network carries, for the next title.
      expect(server.told).toEqual([4_200]);

      // Again 20 s on: 4 × 0.3 × 0.75 is 0.9, but 1080p keeps 3 Mbps.
      await play(21, 0.3);
      await wait(2_500);
      now("ready");
      expect(bitrates(w)).toEqual(["bitrate s8000 4000", "bitrate s8000 3000"]);
      // Below that, the network waits: no new stream, ever.
      await play(21, 0.3);
      await wait(2_500);
      now("ready");
      await play(21, 0.3);
      expect(bitrates(w).length).toBe(2);
      expect(w.requests.length).toBe(1);
      expect(c.state.value.quality).toBe("Automatic · 3 Mbps 1080p");
      c.close();
    });

    it("steps back up the same way after calm minutes, and a new stream of the quality goes on where it came to", async () => {
      const { w, c, now, play, wait } = await automatic(item(hd()), memory(), null, serverMemory({ kbps: 9_000, at: Date.now() }).network, liveConverted);
      await play(12, 0.7);
      await wait(2_500);
      now("ready");
      expect(bitrates(w)).toEqual(["bitrate s8000 4000"]);
      // A recovery's new stream goes on at 4 Mbps too.
      w.engine.listener?.error({ kind: "network", detail: "connection reset" });
      await vi.advanceTimersByTimeAsync(2_500);
      expect(w.requests.map((r) => [r.mode, r.max_bitrate])).toEqual([
        ["transcode", 8_000],
        ["transcode", 8_000],
      ]);
      expect(bitrates(w)).toEqual(["bitrate s8000 4000", "bitrate s8000 4000"]);
      now("ready");
      // On LG the buffer refilling at 4 times its pace says the network carries 16 Mbps at least: 1.6 × 8 Mbps.
      await play(8, 4);
      await play(310, 1.02);
      expect(bitrates(w)).toEqual(["bitrate s8000 4000", "bitrate s8000 4000", "bitrate s8000 8000"]);
      expect(w.requests.length).toBe(2);
      expect(c.state.value.quality).toBe("Automatic · 8 Mbps 1080p");
      c.close();
    });

    it("plays on as it was when the server can't change the bitrate", async () => {
      const { w, c, now, play, wait } = await automatic(item(hd()), memory(), null, serverMemory({ kbps: 9_000, at: Date.now() }).network, liveConverted);
      w.bitrate.answer = () => Promise.reject(new ApiError(400, "bad_request", "this stream's bitrate can't change as it plays"));
      await play(12, 0.7);
      await wait(2_500);
      now("ready");
      expect(bitrates(w)).toEqual(["bitrate s8000 4000"]);
      expect(c.state.value.quality).toBe("Automatic · 8 Mbps 1080p");
      // From then on, a stream that can't change: short waits don't move it, and nothing is asked again.
      await play(21, 1.2);
      await wait(800);
      now("ready");
      await play(10, 1.2);
      await wait(800);
      now("ready");
      await play(10, 1.2);
      expect(bitrates(w).length).toBe(1);
      expect(w.requests.length).toBe(1);
      c.close();
    });

    it("steps the original the server converts too, and Automatic picked again sends it back to its own bitrate", async () => {
      const { w, c, now, play, wait } = await automatic(item(file()), memory(), null, null, convertsOriginal);
      expect(c.state.value.quality).toBe("Automatic · 30 Mbps 4K");
      // 30 × 0.5 × 0.75 = 11.25 → 11 Mbps, still 4K.
      await play(12, 0.5);
      await wait(2_500);
      now("ready");
      expect(bitrates(w)).toEqual(["bitrate s1 11000"]);
      expect(c.state.value.quality).toBe("Automatic · 11 Mbps 4K");
      expect(c.state.value.note).toBe("Lowered for your connection");
      c.selectQuality("auto");
      await vi.advanceTimersByTimeAsync(10);
      expect(bitrates(w)).toEqual(["bitrate s1 11000", "bitrate s1 30000"]);
      expect(w.requests.length).toBe(1);
      expect(c.state.value.quality).toBe("Automatic · 30 Mbps 4K");
      expect(c.state.value.note).toBeNull();
      c.close();
    });

    it("moves the original to a conversion only once its picture stops for 2 s, not for short waits or slow delivery", async () => {
      const { w, c, now, play, wait } = await automatic(item(file()), memory(), null, null, liveConverted);
      await wait(800);
      now("ready");
      await play(10, 1.2);
      await wait(800);
      now("ready");
      await play(40, 0.8);
      expect(w.requests.length).toBe(1);
      // 40 Mbps arriving at 0.8 of its pace: 40 × 0.8 × 0.75 = 24 → 15 Mbps 4K, a new stream made ready beside it.
      await wait(2_500);
      await vi.advanceTimersByTimeAsync(2_500);
      expect(w.requests.map((r) => [r.mode, r.max_bitrate, r.keep_previous])).toEqual([
        ["auto", undefined, undefined],
        ["transcode", 15_000, true],
      ]);
      expect(bitrates(w)).toEqual([]);
      c.close();
    });

    it("plays the original on through dropped frames past its first minute, and starts the next title a step lower", async () => {
      const server = serverMemory(null);
      const { w, c, play } = await automatic(item(file()), memory(), null, server.network);
      const frames = { dropped: 0, total: 0 };
      w.engine.frames = frames;
      w.engine.listener?.firstFrame();
      const drop = async (seconds: number, dropped: number) => {
        frames.dropped += dropped;
        for (let i = 0; i < seconds * 2; i++) {
          frames.total += 12;
          await play(0.5, 1.2);
        }
      };
      await drop(62, 0);
      await drop(14, 130);
      await vi.advanceTimersByTimeAsync(2_500);
      expect(w.requests.length).toBe(1);
      // 40 × 0.75 = 30 → 30 Mbps 4K, where the next title starts.
      expect(server.told).toEqual([30_000]);
      expect(c.state.value.quality).toBe("Automatic · Original");
      c.close();
    });
  });
});

describe("a change while it plays", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  /** The original as it is (s1); a conversion asked for (s4000, s2000…), a stream in two sizes. */
  const answer = (req: PlaybackRequest) =>
    info(req.max_bitrate ? { session_id: `s${req.max_bitrate}`, method: "transcode", hls: true, url: "/api/media/pley_mk_KEY2/master.m3u8" } : {});

  /** A playback whose picture shows, on a clock the test moves; the calls so far forgotten. */
  async function playing() {
    vi.useFakeTimers();
    const w = world(item(file()), answer);
    w.env.now = () => Date.now();
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await vi.advanceTimersByTimeAsync(50);
    w.engine.state = "ready";
    w.engine.listener?.firstFrame();
    w.engine.listener?.status("ready");
    w.calls.length = 0;
    return { w, c };
  }

  /** Segments the server answers only when the test says. */
  function slowSegments(w: ReturnType<typeof world>) {
    const made: (() => void)[] = [];
    w.media.answer = (method, url) => (method === "HEAD" ? new Promise((r) => made.push(() => r(""))) : Promise.resolve(playlistFor(url)));
    return made;
  }

  it("makes the new stream ready beside the one playing, then moves to it from where that one got to", async () => {
    const { w, c } = await playing();
    const made = slowSegments(w);
    c.selectQuality("4m");
    await vi.advanceTimersByTimeAsync(10);
    expect(w.requests[1]).toMatchObject({ keep_previous: true, max_bitrate: 4000, start_ms: 600_000 });
    // Asked for as the TV's player will: the playlist, the size it lists first, and the segment 4 s on, made before it answers.
    expect(w.calls).toEqual(["start transcode", "GET pley_mk_KEY2/master.m3u8", "GET pley_mk_KEY2/v0/index.m3u8", "HEAD pley_mk_KEY2/v0/151.ts"]);
    // The one playing plays on meanwhile, without a ring.
    expect(w.engine.prepared.length).toBe(1);
    expect(c.state.value.phase.kind).toBe("playing");
    w.engine.pos = 601_500;
    made[0]!();
    await vi.advanceTimersByTimeAsync(10);
    // Then it ends, at where it got to, and the engine moves on from there, keeping its picture where it can.
    expect(w.calls.slice(4)).toEqual(["progress stopped 601500", "delete s1"]);
    expect(w.engine.held).toBe(1);
    expect(w.engine.prepared[1]).toMatchObject({ hls: true, startMs: 601_500, url: "http://nas:7539/api/media/pley_mk_KEY2/master.m3u8" });
    expect(c.state.value.phase.kind).toBe("preparing");
    expect(c.state.value.switching).toBe(true);
    w.engine.listener?.status("ready");
    expect(c.state.value.switching).toBe(false);
    c.close();
  });

  it("moves after 20 s at most when the stream isn't ready by then", async () => {
    const { w, c } = await playing();
    slowSegments(w);
    c.selectQuality("4m");
    await vi.advanceTimersByTimeAsync(19_000);
    expect(w.engine.prepared.length).toBe(1);
    expect(w.calls).not.toContain("delete s1");
    await vi.advanceTimersByTimeAsync(1_100);
    expect(w.engine.prepared.length).toBe(2);
    expect(w.calls).toContain("delete s1");
    c.close();
  });

  it("ends the one playing first, as before, when no transcode is free for two at once (503)", async () => {
    const { w, c } = await playing();
    w.net.busy = true;
    c.selectQuality("4m");
    await vi.advanceTimersByTimeAsync(2_500);
    expect(w.requests.slice(1).map((r) => r.keep_previous)).toEqual([true, undefined]);
    expect(w.calls).toEqual(["start transcode", "progress stopped 600000", "delete s1", "start transcode"]);
    expect(w.engine.held).toBe(0);
    expect(w.engine.prepared[1]).toMatchObject({ hls: true, startMs: 600_000 });
    expect(c.state.value.switching).toBe(false);
    expect(c.state.value.phase.kind).not.toBe("error");
    c.close();
  });

  it("lets a session it was making ready go when another change comes first", async () => {
    const { w, c } = await playing();
    const made = slowSegments(w);
    c.selectQuality("4m");
    await vi.advanceTimersByTimeAsync(10);
    c.selectQuality("2m");
    await vi.advanceTimersByTimeAsync(10);
    expect(w.calls).toContain("delete s4000");
    expect(w.calls).not.toContain("delete s1");
    expect(w.requests[2]).toMatchObject({ keep_previous: true, max_bitrate: 2000 });
    made[made.length - 1]!();
    await vi.advanceTimersByTimeAsync(10);
    expect(w.engine.prepared.length).toBe(2);
    expect(w.calls).toContain("delete s1");
    c.close();
  });

  it("starts again at once to recover from an error, ending the one that failed first", async () => {
    const { w, c } = await playing();
    w.engine.listener?.error({ kind: "unsupported", audio: false, container: false, detail: "decoder" });
    await vi.advanceTimersByTimeAsync(2_500);
    expect(w.requests.map((r) => [r.mode, r.keep_previous])).toEqual([
      ["auto", undefined],
      ["transcode", undefined],
    ]);
    expect(w.calls.slice(0, 3)).toEqual(["progress stopped 600000", "delete s1", "start transcode"]);
    expect(w.calls.filter((x) => x.indexOf("HEAD") === 0)).toEqual([]);
    expect(c.state.value.switching).toBe(false);
    c.close();
  });

  it("starts at once when nothing plays yet", async () => {
    vi.useFakeTimers();
    const w = world(item(file()), answer);
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await vi.advanceTimersByTimeAsync(50);
    // Its picture hasn't shown: nothing to keep playing beside the next.
    c.selectQuality("4m");
    await vi.advanceTimersByTimeAsync(2_500);
    expect(w.requests.map((r) => r.keep_previous)).toEqual([undefined, undefined]);
    c.close();
  });
});

describe("speed", () => {
  it("isn't offered where the TV's player can't keep the sound at other speeds (Samsung's AVPlay)", async () => {
    const w = world(item(file()), () => info());
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    expect(c.state.value.speeds).toEqual([]);
    c.setSpeed(2);
    expect(c.state.value.speed).toBe(1);
    c.close();
  });

  it("plays at the speed picked, only those offered, and keeps it for the title's next session", async () => {
    const w = world(item(file()), (req) => info({ session_id: `s-${req.mode}` }));
    const rates: number[] = [];
    (w.engine as FakeEngine & { setSpeed(rate: number): boolean }).setSpeed = (rate: number) => {
      rates.push(rate);
      return true;
    };
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    expect(c.state.value.speeds).toEqual([0.5, 0.75, 1, 1.25, 1.5, 2]);
    c.setSpeed(1.5);
    c.setSpeed(3);
    expect([c.state.value.speed, rates]).toEqual([1.5, [1.5]]);
    // Another session of the same title (subtitles burned in, say) plays at it too.
    c.selectQuality("8m");
    await settle();
    expect(w.engine.prepared).toHaveLength(2);
    expect(rates).toEqual([1.5, 1.5]);
    c.close();
  });
});

describe("picture subtitles", () => {
  it("are asked for from where the player is: the server copies them out of the file from there first", async () => {
    const w = world(item(file({ selected_subtitle: 3 })), () => info());
    const sent: string[] = [];
    (w.env.api as unknown as Record<string, unknown>).send = (url: string) => {
      sent.push(url);
      return new Promise(() => undefined);
    };
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    expect(sent).toContain("/api/files/7/subtitles/3.mks?at=600000");
    c.close();
  });

  it("say where the player is at the beginning too", () => {
    expect(pictureUrlAt("/api/files/7/subtitles/3.mks", 0)).toBe("/api/files/7/subtitles/3.mks?at=0");
    expect(pictureUrlAt("/api/files/7/subtitles/3.mks", 1234.4)).toBe("/api/files/7/subtitles/3.mks?at=1234");
    expect(pictureUrlAt("/s.mks?k=1", 5000)).toBe("/s.mks?k=1&at=5000");
  });
});

describe("subtitles found on the server", () => {
  it("are searched in the language's two-letter code, and one added is listed with the file's and shows", async () => {
    const w = world(item(file()), () => info({ method: "remux", hls: true, url: "/api/media/pley_mk_KEY/master.m3u8" }));
    const searched: unknown[][] = [];
    const api = w.env.api as unknown as Record<string, unknown>;
    api.searchSubtitles = (...args: unknown[]) => {
      searched.push(args);
      return Promise.resolve([]);
    };
    api.addSubtitle = (_item: number, _file: number, result: string) =>
      Promise.resolve({ index: 2000, codec: "subrip", language: "fre", title: null, default: false, forced: false, external: true, text_based: true, url: `/api/files/7/subtitles/2000.vtt?r=${result}`, added: true });
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    await c.findSubtitles("fre");
    expect(searched[0]!.slice(0, 3)).toEqual([70, 7, ["fr"]]);

    await c.addFoundSubtitle({ id: "opensubtitles:9", provider: "opensubtitles", language: "fr", name: "Film", format: "srt", hearing_impaired: false, forced: false, downloads: 3, matched: "hash" });
    const subs = c.state.value.subtitles;
    expect(subs[subs.length - 1]).toMatchObject({ index: 2000, selected: true, added: true });
    expect(c.state.value.notice).toMatch(/^Subtitles added: French/);
    // Text: drawn by the TV from the same session, chosen for the title as any track picked.
    expect(w.requests).toHaveLength(1);
    expect(w.saved[w.saved.length - 1]).toEqual([undefined, 2000]);
    c.close();
  });
});

describe("the user's remote, on another device", () => {
  const command = (c: Omit<Extract<ServerEvent, { type: "player_command" }>, "type" | "position_ms" | "offset_ms"> & { position_ms?: number; offset_ms?: number }): ServerEvent => ({
    type: "player_command",
    position_ms: null,
    offset_ms: null,
    ...c,
  });

  async function playing(detail = item(file())) {
    const w = world(detail, () => info());
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    w.engine.state = "ready";
    w.engine.listener?.firstFrame();
    w.engine.listener?.status("ready");
    await settle();
    w.calls.length = 0;
    return { w, c };
  }

  it("is told what the player obeys: next only when something may come after it, audio and subtitles when the file has them, the quality always", async () => {
    const film = await playing();
    expect(film.w.requests[0]!.controls).toEqual(["play", "pause", "seek", "skip", "audio", "subtitles", "quality"]);
    film.c.close();
    const episode = await playing(item(file(), { kind: "episode" }));
    expect(episode.w.requests[0]!.controls).toEqual(["play", "pause", "seek", "skip", "next", "audio", "subtitles", "quality"]);
    episode.c.close();
    const bare = file();
    bare.streams = bare.streams.filter((s) => s.kind !== "subtitle");
    const noSubtitles = await playing(item(bare));
    expect(noSubtitles.w.requests[0]!.controls).toEqual(["play", "pause", "seek", "skip", "audio", "quality"]);
    noSubtitles.c.close();
  });

  it("pauses and plays, reporting each at once", async () => {
    const { w, c } = await playing();
    w.send(command({ session_id: "s1", command: "pause" }));
    await settle();
    expect(c.wantsToPlay).toBe(false);
    w.send(command({ session_id: "s1", command: "play" }));
    await settle();
    expect(c.wantsToPlay).toBe(true);
    expect(w.calls).toEqual(["progress paused 600000", "progress playing 600000"]);
    c.close();
  });

  it("says again that it plays, or is paused, when told so already, so a remote a little behind settles", async () => {
    const { w, c } = await playing();
    w.send(command({ session_id: "s1", command: "play" }));
    await settle();
    expect(c.wantsToPlay).toBe(true);
    w.send(command({ session_id: "s1", command: "pause" }));
    w.send(command({ session_id: "s1", command: "pause" }));
    await settle();
    expect(c.wantsToPlay).toBe(false);
    expect(w.calls).toEqual(["progress playing 600000", "progress paused 600000", "progress paused 600000"]);
    c.close();
  });

  it("leaves another session's commands alone", async () => {
    const { w, c } = await playing();
    w.send(command({ session_id: "s2", command: "pause" }));
    w.send(command({ session_id: "s2", command: "seek", position_ms: 0 }));
    await settle();
    expect(c.wantsToPlay).toBe(true);
    expect(w.calls).toEqual([]);
    c.close();
  });

  it("seeks within the title, skips from where it is, and reports where it went at once", async () => {
    const { w, c } = await playing();
    w.send(command({ session_id: "s1", command: "skip", offset_ms: -30_000 }));
    await settle();
    w.send(command({ session_id: "s1", command: "seek", position_ms: 9_000_000 }));
    await settle();
    // The end, less the margin every seek keeps.
    expect(w.calls).toEqual(["progress playing 570000", "progress playing 7199750"]);
    c.close();
  });

  it("does nothing for previous, nor for next with nothing after it, and says nothing of it", async () => {
    const { w, c } = await playing(item(file(), { kind: "episode" }));
    w.send(command({ session_id: "s1", command: "previous" }));
    w.send(command({ session_id: "s1", command: "next" }));
    await settle();
    expect(w.requests).toHaveLength(1);
    expect(c.state.value.notice).toBeNull();
    c.close();
  });

  it("obeys nothing once the server stopped its session", async () => {
    const { w, c } = await playing(item(file(), { kind: "episode" }));
    c.pause();
    w.send({ type: "session_stopped", session_id: "s1", message: null, by_admin: false });
    await settle();
    w.calls.length = 0;
    w.send(command({ session_id: "s1", command: "play" }));
    w.send(command({ session_id: "s1", command: "seek", position_ms: 0 }));
    w.send(command({ session_id: "s1", command: "next" }));
    w.send(command({ session_id: "s1", command: "audio", stream_index: 2 }));
    await settle();
    expect(c.wantsToPlay).toBe(false);
    expect(w.requests).toHaveLength(1);
    expect(w.calls).toEqual([]);
    c.close();
  });

  /** A film whose second audio track (French AC-3) the TV's player switches to in place. */
  function twoAudio(): MediaFile {
    const f = file();
    f.streams[2] = stream({ index: 2, kind: "audio", codec: "ac3", language: "fra", channels: 6, default: false });
    return f;
  }

  it("switches the audio as its menu does, in place where the TV's player can, saved as a pick, and reports the tracks at once", async () => {
    const { w, c } = await playing(item(twoAudio()));
    w.reported.length = 0;
    w.send(command({ session_id: "s1", command: "audio", stream_index: 2 }));
    await settle();
    expect(w.engine.switched).toEqual([0, 1]);
    expect(w.requests).toHaveLength(1);
    expect(w.reported).toEqual(["playing 600000 audio 2 subtitles -1"]);
    expect(w.saved).toEqual([[2, null]]);
    expect(c.state.value.audio.filter((a) => a.selected).map((a) => a.index)).toEqual([2]);
    c.close();
  });

  it("starts a new session for audio its player can't switch to, which says what it plays as it begins", async () => {
    let sessions = 0;
    const w = world(item(file()), (req) => info({ session_id: `s${++sessions}`, audio_index: req.audio_index ?? 1 }));
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    w.engine.state = "ready";
    w.engine.listener?.firstFrame();
    w.engine.listener?.status("ready");
    await settle();
    // The TrueHD: not this TV's to decode, so the server sends what it plays.
    w.send(command({ session_id: "s1", command: "audio", stream_index: 2 }));
    await settle();
    expect(w.requests[1]).toMatchObject({ audio_index: 2, keep_previous: true });
    // The one it leaves says what it played; the new one, what it plays.
    expect(w.reported).toEqual(["stopped 600000 audio 1 subtitles -1", "playing 600000 audio 2 subtitles -1"]);
    c.close();
  });

  it("turns subtitles on and off as its menu does, reporting the tracks at once", async () => {
    const { w, c } = await playing();
    w.reported.length = 0;
    w.send(command({ session_id: "s1", command: "subtitles", stream_index: 4 }));
    await settle();
    w.send(command({ session_id: "s1", command: "subtitles", stream_index: -1 }));
    await settle();
    expect(w.requests).toHaveLength(1);
    expect(w.reported).toEqual(["playing 600000 audio 1 subtitles 4", "playing 600000 audio 1 subtitles -1"]);
    expect(w.saved).toEqual([
      [undefined, 4],
      [undefined, null],
    ]);
    c.close();
  });

  it("says again what plays when picked again, and leaves another session's tracks and one the file lacks alone", async () => {
    const { w, c } = await playing();
    w.reported.length = 0;
    w.send(command({ session_id: "s1", command: "audio", stream_index: 1 }));
    w.send(command({ session_id: "s1", command: "subtitles", stream_index: -1 }));
    w.send(command({ session_id: "s2", command: "audio", stream_index: 2 }));
    w.send(command({ session_id: "s1", command: "audio", stream_index: 9 }));
    w.send(command({ session_id: "s1", command: "subtitles", stream_index: 1 }));
    await settle();
    expect(w.reported).toEqual(["playing 600000 audio 1 subtitles -1", "playing 600000 audio 1 subtitles -1"]);
    expect(w.requests).toHaveLength(1);
    expect(w.saved).toEqual([]);
    c.close();
  });

  it("says which tracks play in every report: a pause, the 10 s ones, and the end", async () => {
    vi.useFakeTimers();
    try {
      const w = world(item(file({ selected_subtitle: 4 })), () => info());
      w.env.now = () => Date.now();
      const c = new PlaybackController({ itemId: 70 }, w.env, 0);
      c.start();
      await vi.advanceTimersByTimeAsync(50);
      w.engine.state = "ready";
      w.engine.listener?.status("ready");
      await vi.advanceTimersByTimeAsync(10_000);
      c.pause();
      c.close();
      await vi.advanceTimersByTimeAsync(10);
      expect(w.reported).toEqual(["playing 600000 audio 1 subtitles 4", "paused 600000 audio 1 subtitles 4", "stopped 600000 audio 1 subtitles 4"]);
      expect(w.qualities).toEqual(["playing original", "paused original", "stopped original"]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("picks a quality as its Quality menu does, kept for the next titles, in a new session that says so as it begins", async () => {
    let sessions = 0;
    const w = world(item(file()), () => info({ session_id: `s${++sessions}` }));
    const picked: string[] = [];
    w.env.quality = { remembered: () => "original", remember: (id) => void picked.push(id) };
    const c = new PlaybackController({ itemId: 70 }, w.env, 0);
    c.start();
    await settle();
    w.engine.state = "ready";
    w.engine.listener?.firstFrame();
    w.engine.listener?.status("ready");
    await settle();
    w.qualities.length = 0;
    w.send(command({ session_id: "s1", command: "quality", quality: "8m" }));
    await settle();
    expect(w.requests[1]).toMatchObject({ mode: "transcode", keep_previous: true });
    expect(picked).toEqual(["8m"]);
    expect(c.state.value.quality).toBe("8 Mbps 1080p");
    // The one it leaves, then the new one as it begins.
    expect(w.qualities).toEqual(["stopped 8m", "playing 8m"]);
    c.close();
  });

  it("says again which quality is picked when picked again, and leaves another session's and one its menu lacks alone", async () => {
    const { w, c } = await playing();
    w.qualities.length = 0;
    w.send(command({ session_id: "s1", command: "quality", quality: "original" }));
    w.send(command({ session_id: "s2", command: "quality", quality: "8m" }));
    w.send(command({ session_id: "s1", command: "quality", quality: "best" }));
    w.send(command({ session_id: "s1", command: "quality", quality: null }));
    await settle();
    expect(w.qualities).toEqual(["playing original"]);
    expect(w.requests).toHaveLength(1);
    c.close();
  });

  /** What the player says, for the moment it says it, of each command sent, in order (`null`: nothing). */
  function said(c: PlaybackController, w: ReturnType<typeof world>, commands: ServerEvent[]): (string | null)[] {
    return commands.map((e) => {
      w.send(e);
      const notice = c.state.value.remoteNotice;
      c.dismissRemoteNotice();
      return notice;
    });
  }

  it("says for a moment what a play, pause, seek or skip did, and nothing of what changes nothing", async () => {
    const { w, c } = await playing(item(file(), { kind: "episode" }));
    expect(
      said(c, w, [
        command({ session_id: "s1", command: "play" }),
        command({ session_id: "s1", command: "pause" }),
        command({ session_id: "s1", command: "pause" }),
        command({ session_id: "s1", command: "play" }),
        command({ session_id: "s1", command: "skip", offset_ms: 30_000 }),
        command({ session_id: "s1", command: "skip", offset_ms: -10_000 }),
        command({ session_id: "s1", command: "seek", position_ms: 754_000 }),
        command({ session_id: "s1", command: "seek", position_ms: 3_723_000 }),
        command({ session_id: "s1", command: "previous" }),
        command({ session_id: "s2", command: "pause" }),
      ]),
    ).toEqual([null, "Paused", null, "Playing", "+30 s", "−10 s", "Jumped to 12:34", "Jumped to 1:02:03", null, null]);
    c.close();
  });

  it("says the audio, subtitles and quality picked as their menus name them, and nothing of a pick already made", async () => {
    const { w, c } = await playing(item(twoAudio()));
    expect(
      said(c, w, [
        command({ session_id: "s1", command: "audio", stream_index: 2 }),
        command({ session_id: "s1", command: "audio", stream_index: 2 }),
        command({ session_id: "s1", command: "subtitles", stream_index: 4 }),
        command({ session_id: "s1", command: "subtitles", stream_index: -1 }),
        command({ session_id: "s1", command: "subtitles", stream_index: -1 }),
        command({ session_id: "s1", command: "quality", quality: "original" }),
        command({ session_id: "s1", command: "quality", quality: "auto" }),
      ]),
    ).toEqual(["Audio: French (AC3 5.1)", null, "Subtitles: English (SRT)", "Subtitles off", null, null, "Quality: Automatic"]);
    // The menus' own words.
    expect(c.state.value.audio.filter((a) => a.index === 2)[0]!.label).toBe("French (AC3 5.1)");
    expect(c.state.value.qualities.filter((q) => q.id === "auto")[0]!.label).toBe("Automatic");
    c.close();
  });

  it("marks a pause as the remote's until it plays again, by anyone; the player's own pause is its own", async () => {
    const { w, c } = await playing(item(file(), { kind: "episode" }));
    c.pause();
    expect(c.state.value.pausedByRemote).toBe(false);
    // Paused already: the pause stays the person's.
    w.send(command({ session_id: "s1", command: "pause" }));
    expect(c.state.value.pausedByRemote).toBe(false);
    c.play();
    w.send(command({ session_id: "s1", command: "pause" }));
    expect(c.state.value.pausedByRemote).toBe(true);
    // A seek leaves it paused, and the remote's.
    w.send(command({ session_id: "s1", command: "skip", offset_ms: 10_000 }));
    expect(c.state.value.pausedByRemote).toBe(true);
    c.play();
    expect(c.state.value.pausedByRemote).toBe(false);
    w.send(command({ session_id: "s1", command: "pause" }));
    w.send(command({ session_id: "s1", command: "play" }));
    expect(c.state.value.pausedByRemote).toBe(false);
    c.close();
  });
});

describe("a title cast with the tracks picked on the remote's page", () => {
  async function started(request: PlayRequest) {
    const w = world(item(file({ selected_subtitle: 4 })), (req) => info({ audio_index: req.audio_index ?? 1 }));
    const c = new PlaybackController(request, w.env, 0);
    c.start();
    await settle();
    w.engine.state = "ready";
    w.engine.listener?.firstFrame();
    w.engine.listener?.status("ready");
    await settle();
    // What it plays, said at once.
    w.send({ type: "player_command", session_id: "s1", command: "play", position_ms: null, offset_ms: null });
    await settle();
    return { w, c };
  }

  it("plays with them for that play, saving nothing, and no subtitles for -1", async () => {
    const { w, c } = await started({ itemId: 70, fromStart: false, audioIndex: 2, subtitleIndex: -1 });
    expect(w.requests[0]!.audio_index).toBe(2);
    expect(w.reported[0]).toBe("playing 600000 audio 2 subtitles -1");
    expect(w.saved).toEqual([]);
    c.close();
  });

  it("leaves a track the file doesn't have to the saved choices", async () => {
    const { w, c } = await started({ itemId: 70, fromStart: false, audioIndex: 9, subtitleIndex: 12 });
    expect(w.requests[0]!.audio_index).toBeUndefined();
    expect(w.reported[0]).toBe("playing 600000 audio 1 subtitles 4");
    c.close();
  });

  it("starts with subtitles off for -1, else as asked, else the profile's choice", () => {
    const f = file({ selected_subtitle: 4 });
    expect([initialSubtitle(f, -1), initialSubtitle(f, 3)?.index, initialSubtitle(f, null)?.index]).toEqual([null, 3, 4]);
  });
});

import { afterEach, describe, expect, it, vi } from "vitest";
import type { EngineListener, EngineSource, EngineStatus, PlayerEngine } from "pley-tv/player/engine";
import type { EngineError } from "pley-tv/player/recovery";
import { chooseEngine, SelectingEngine, type PlayerPreference } from "../src/engine-select";

function source(over: Partial<EngineSource> = {}, video: Partial<NonNullable<EngineSource["video"]>> = {}): EngineSource {
  return {
    url: "http://nas:7539/api/media/pley_mk_KEY/file.mkv",
    hls: false,
    mime: "video/x-matroska",
    startMs: 600_000,
    durationHintMs: 7_200_000,
    video: { codec: "hevc", width: 1920, height: 1080, frameRate: 24, range: "sdr", dvProfile: null, bitDepth: 8, ...video },
    audioCodec: "eac3",
    audioDefault: true,
    ...over,
  };
}

/** A player that records what is asked of it into the test's shared log. */
class Fake implements PlayerEngine {
  listener: EngineListener | null = null;
  prepared: EngineSource[] = [];
  seeks: number[] = [];
  state: EngineStatus = "idle";
  area: unknown = undefined;
  readonly name: string;
  private readonly log: string[];
  constructor(name: string, log: string[]) {
    this.name = name;
    this.log = log;
    log.push(`${name} created`);
  }
  setListener(l: EngineListener) {
    this.listener = l;
  }
  prepare(s: EngineSource, play: boolean) {
    this.prepared.push(s);
    this.log.push(`${this.name} prepare ${s.startMs} ${play ? "play" : "paused"}`);
  }
  play() {}
  pause() {}
  seekTo(ms: number) {
    this.seeks.push(ms);
  }
  stop() {
    this.log.push(`${this.name} stop`);
  }
  release() {
    this.log.push(`${this.name} release`);
  }
  pos = 0;
  positionMs() {
    return this.pos;
  }
  durationMs() {
    return 7_200_000;
  }
  bufferedMs() {
    return this.pos;
  }
  setArea(a: unknown) {
    this.area = a;
  }
  status() {
    return this.state;
  }
  playing() {
    return false;
  }
  audioTracks() {
    return [];
  }
  selectAudio() {
    return false;
  }
  frameCounts() {
    return { dropped: 30, total: 100 };
  }
  stats() {
    return null;
  }
}

function setup(preference: () => PlayerPreference = () => "auto") {
  const log: string[] = [];
  const made: { video: Fake[]; avplay: Fake[] } = { video: [], avplay: [] };
  const engine = new SelectingEngine(
    {
      video: () => {
        const f = new Fake("video", log);
        made.video.push(f);
        return f;
      },
      avplay: () => {
        const f = new Fake("avplay", log);
        made.avplay.push(f);
        return f;
      },
    },
    preference,
    // A set before 2020, the PR's own: files start on <video>, streams at their first segment.
    { videoFirst: true, hlsFromFirstSegment: true },
  );
  const heard: string[] = [];
  engine.setListener({
    status: (s) => heard.push(`status ${s}`),
    firstFrame: () => heard.push("firstFrame"),
    error: (e) => heard.push(`error ${e.kind}`),
    tracks: () => heard.push("tracks"),
    seekFailed: (t) => heard.push(`seekFailed ${t}`),
    restarted: () => {
      heard.push("restarted");
      log.push("restarted");
    },
  });
  const video = () => made.video.at(-1)!;
  const avplay = () => made.avplay.at(-1)!;
  return { engine, log, made, heard, video, avplay };
}

const unsupported: EngineError = { kind: "unsupported", audio: false, container: true, detail: "MEDIA_ERR_SRC_NOT_SUPPORTED" };

afterEach(() => {
  vi.restoreAllMocks();
});

describe("which of the TV's players plays a source", () => {
  it("plays a file as it is on <video>: H.264 or HEVC up to 1080p, SDR, in MKV, MP4, TS or WebM, with the file's own audio", () => {
    expect(chooseEngine(source(), "auto").engine).toBe("video");
    expect(chooseEngine(source({ mime: "video/mp4", audioCodec: "aac" }, { codec: "h264" }), "auto").engine).toBe("video");
    expect(chooseEngine(source({ mime: "video/mp2t", audioCodec: "ac3" }), "auto").engine).toBe("video");
    expect(chooseEngine(source({ mime: "video/webm", audioCodec: "opus" }), "auto").engine).toBe("video");
    expect(chooseEngine(source({ audioCodec: "mp3" }, { width: 1920, height: 800 }), "auto").engine).toBe("video");
    for (const audio of ["flac", "pcm_s16le", null]) expect(chooseEngine(source({ audioCodec: audio }), "auto").engine).toBe("video");
  });

  it("leaves AVPlay what <video> isn't known to play here", () => {
    const avplay = (s: EngineSource) => chooseEngine(s, "auto");
    expect(avplay(source({ hls: true, mime: null }))).toMatchObject({ engine: "avplay", reason: "hls" });
    expect(avplay(source({ live: true }))).toMatchObject({ engine: "avplay", reason: "live" });
    expect(avplay(source({}, { width: 7680, height: 4320 }))).toMatchObject({ engine: "avplay", reason: "above 1080p" });
    // Samsung's <video> stops at 1080p (docs/TV_PLATFORMS.md); 4K is AVPlay's.
    expect(avplay(source({}, { width: 3840, height: 2160 }))).toMatchObject({ engine: "avplay", reason: "above 1080p" });
    expect(avplay(source({}, { range: "hdr10", bitDepth: 10 }))).toMatchObject({ engine: "avplay", reason: "hdr" });
    expect(avplay(source({}, { range: "sdr", dvProfile: 8 }))).toMatchObject({ engine: "avplay", reason: "hdr" });
    expect(avplay(source({ audioDefault: false }))).toMatchObject({ engine: "avplay", reason: "audio track" });
    expect(avplay(source({}, { codec: "vc1" }))).toMatchObject({ engine: "avplay", reason: "codec" });
    expect(avplay(source({ mime: "video/x-msvideo" }))).toMatchObject({ engine: "avplay", reason: "container" });
    expect(avplay(source({ mime: null }))).toMatchObject({ engine: "avplay", reason: "container" });
    expect(avplay(source({ audioCodec: "dts" }))).toMatchObject({ engine: "avplay", reason: "audio codec" });
    expect(avplay(source({ audioCodec: "truehd" }))).toMatchObject({ engine: "avplay", reason: "audio codec" });
    expect(avplay(source({ video: null }))).toMatchObject({ engine: "avplay", reason: "no video" });
  });

  it("plays everything on AVPlay when Settings › Playback says so", () => {
    expect(chooseEngine(source(), "avplay")).toMatchObject({ engine: "avplay", reason: "setting" });
  });
});

describe("the TV's players behind one engine", () => {
  it("says which it chose, and why", () => {
    const info = vi.spyOn(console, "info").mockImplementation(() => undefined);
    const { engine } = setup();
    engine.prepare(source(), true);
    expect(info).toHaveBeenCalledWith("[engine] chosen", { engine: "video", reason: "direct" });
    engine.prepare(source({ hls: true, mime: null }), true);
    expect(info).toHaveBeenCalledWith("[engine] chosen", { engine: "avplay", reason: "hls" });
  });

  it("plays a file on <video> and makes no AVPlay meanwhile", () => {
    const { engine, log } = setup();
    engine.prepare(source(), true);
    expect(log).toEqual(["video created", "video prepare 600000 play"]);
    expect(engine.activePlayer()).toBe("video");
  });

  it("lets one player go entirely before the other starts (they share the screen and the decoder)", () => {
    const { engine, log } = setup();
    engine.prepare(source(), true);
    engine.prepare(source({ hls: true, mime: null, startMs: 700_000 }), true);
    expect(log).toEqual(["video created", "video prepare 600000 play", "video release", "avplay created", "avplay prepare 700000 play"]);
    log.length = 0;
    engine.prepare(source({ startMs: 800_000 }), false);
    expect(log).toEqual(["avplay release", "video created", "video prepare 800000 paused"]);
  });

  it("keeps the player playing for the next source it takes", () => {
    const { engine, log } = setup();
    engine.prepare(source(), true);
    engine.prepare(source({ startMs: 0 }), true);
    expect(log).toEqual(["video created", "video prepare 600000 play", "video prepare 0 play"]);
  });

  it("takes AVPlay for everything when the setting says so, and <video> when reading it fails", () => {
    const a = setup(() => "avplay");
    a.engine.prepare(source(), true);
    expect(a.log).toEqual(["avplay created", "avplay prepare 600000 play"]);
    const b = setup(() => {
      throw new Error("storage refused");
    });
    b.engine.prepare(source(), true);
    expect(b.log).toEqual(["video created", "video prepare 600000 play"]);
  });

  it("goes on with AVPlay at the same place when <video> can't open the file, before its first picture", () => {
    const { engine, log, heard, video, avplay } = setup();
    engine.prepare(source(), true);
    video().listener!.status("buffering");
    video().listener!.error(unsupported);
    // The session hears of the new start before it, so its stall clock counts AVPlay's opening alone.
    expect(log).toEqual(["video created", "video prepare 600000 play", "restarted", "video release", "avplay created", "avplay prepare 600000 play"]);
    expect(heard).not.toContain("error unsupported");
    expect(engine.activePlayer()).toBe("avplay");
    // AVPlay's own events reach the session.
    avplay().listener!.firstFrame();
    expect(heard).toContain("firstFrame");
  });

  it("starts AVPlay where a seek asked meanwhile went", () => {
    const { engine, log, video } = setup();
    engine.prepare(source(), true);
    engine.seekTo(1_200_000);
    video().listener!.error({ kind: "unsupported", audio: true, container: false, detail: "MEDIA_ERR_DECODE" });
    expect(log.at(-1)).toBe("avplay prepare 1200000 play");
  });

  it("keeps that kind of file on AVPlay for the rest of the player screen", () => {
    const { engine, log, video } = setup();
    engine.prepare(source(), true);
    video().listener!.error(unsupported);
    log.length = 0;
    engine.prepare(source({ startMs: 900_000 }), true);
    expect(log).toEqual(["avplay prepare 900000 play"]);
  });

  it("tells the session of <video>'s errors after its first picture, and of the network's", () => {
    const a = setup();
    a.engine.prepare(source(), true);
    a.video().listener!.firstFrame();
    a.video().listener!.error(unsupported);
    expect(a.heard).toContain("error unsupported");
    expect(a.made.avplay).toEqual([]);
    const b = setup();
    b.engine.prepare(source(), true);
    b.video().listener!.error({ kind: "network", detail: "the network failed while loading" });
    expect(b.heard).toContain("error network");
    expect(b.made.avplay).toEqual([]);
  });

  it("ignores what a player let go says after", () => {
    const { engine, heard, video } = setup();
    engine.prepare(source(), true);
    const old = video();
    engine.prepare(source({ hls: true, mime: null }), true);
    old.listener!.firstFrame();
    old.listener!.seekFailed?.(5);
    expect(heard).toEqual([]);
  });

  it("places whichever player plays in the area asked, the one made after too", () => {
    const { engine, video, avplay } = setup();
    const area = { left: 480, top: 0, width: 480, height: 270 };
    engine.setArea(area);
    engine.prepare(source(), true);
    expect(video().area).toEqual(area);
    engine.prepare(source({ hls: true, mime: null }), true);
    expect(avplay().area).toEqual(area);
  });

  it("starts streams at their first segment, as AVPlay plays them", () => {
    const { engine } = setup();
    expect(engine.hlsFromFirstSegment).toBe(true);
  });

  it("counts no frames on <video>: dropped ones would have the session convert a file that plays as it is", () => {
    const { engine } = setup();
    engine.prepare(source(), true);
    expect(engine.frameCounts()).toBeNull();
    engine.prepare(source({ hls: true, mime: null }), true);
    expect(engine.frameCounts()).toEqual({ dropped: 30, total: 100 });
  });

  it("starts a file on AVPlay on 2020 and later sets, whose AVPlay seeks in it, and streams as they ask", () => {
    const log: string[] = [];
    const engine = new SelectingEngine({ video: () => new Fake("video", log), avplay: () => new Fake("avplay", log) }, () => "auto", {
      videoFirst: false,
      hlsFromFirstSegment: false,
    });
    engine.setListener({ status: () => undefined, firstFrame: () => undefined, error: () => undefined, tracks: () => undefined });
    engine.prepare(source(), true);
    expect(log).toEqual(["avplay created", "avplay prepare 600000 play"]);
    expect(engine.hlsFromFirstSegment).toBe(false);
    // A seek AVPlay refuses there still goes to <video>.
    expect(engine.switchForSeek(1_200_000)).toBe(true);
    expect(engine.activePlayer()).toBe("video");
  });

  it("leaves an audio track change on <video> to a new session: <video> can't be told which track", () => {
    const { engine, video } = setup();
    engine.prepare(source(), true);
    video().selectAudio = () => true;
    expect(engine.selectAudio(1, "fre")).toBe(false);
  });

  it("lets every player go when released", () => {
    const { engine, log } = setup();
    engine.prepare(source(), true);
    engine.release();
    expect(log.at(-1)).toBe("video release");
  });
});

describe("a seek AVPlay refused in a file played as it is", () => {
  const uhd = () => source({}, { width: 3840, height: 2160 });

  it("plays the same address on <video> at the place: a 4K HEVC file in SDR, as Jellyfin's player plays it on these TVs", () => {
    const { engine, log, made } = setup();
    engine.prepare(uhd(), true);
    expect(engine.activePlayer()).toBe("avplay");
    expect(engine.switchForSeek(1_200_000)).toBe(true);
    expect(log).toEqual(["avplay created", "avplay prepare 600000 play", "restarted", "avplay release", "video created", "video prepare 1200000 play"]);
    expect(made.video[0]!.prepared[0]!.url).toBe("http://nas:7539/api/media/pley_mk_KEY/file.mkv");
  });

  it("not for H.264 above 1080p, another audio track than the file's own, or 8K", () => {
    const a = setup();
    a.engine.prepare(source({}, { codec: "h264", width: 3840, height: 2160 }), true);
    expect(a.engine.switchForSeek(1_200_000)).toBe(false);
    const b = setup();
    b.engine.prepare(source({ audioDefault: false }, { width: 3840, height: 2160 }), true);
    expect(b.engine.switchForSeek(1_200_000)).toBe(false);
    const c = setup();
    c.engine.prepare(source({}, { width: 7680, height: 4320 }), true);
    expect(c.engine.switchForSeek(1_200_000)).toBe(false);
  });

  it("tells the session the seek failed when <video> can't open the file there either, with no AVPlay made again", () => {
    const { engine, heard, made, video } = setup();
    engine.prepare(uhd(), true);
    engine.switchForSeek(1_200_000);
    heard.length = 0;
    video().listener!.error(unsupported);
    expect(heard).toEqual(["seekFailed 1200000"]);
    expect(made.avplay.length).toBe(1);
    // And that kind stays on AVPlay: the remux the session starts plays there.
    engine.prepare(uhd(), true);
    expect(engine.activePlayer()).toBe("avplay");
  });

  it("doesn't when Settings › Playback says AVPlay, nor for a stream, nor for what <video> doesn't take", () => {
    const a = setup(() => "avplay");
    a.engine.prepare(source(), true);
    expect(a.engine.switchForSeek(1_200_000)).toBe(false);
    const b = setup();
    b.engine.prepare(source({ hls: true, mime: null }), true);
    expect(b.engine.switchForSeek(1_200_000)).toBe(false);
    const c = setup();
    c.engine.prepare(source({}, { range: "hdr10" }), true);
    expect(c.engine.switchForSeek(1_200_000)).toBe(false);
  });

  it("doesn't when <video> plays already, or failed on that kind of file", () => {
    const a = setup();
    a.engine.prepare(source(), true);
    expect(a.engine.switchForSeek(1_200_000)).toBe(false);
    const b = setup();
    b.engine.prepare(source(), true);
    b.video().listener!.error(unsupported);
    expect(b.engine.switchForSeek(1_200_000)).toBe(false);
  });
});

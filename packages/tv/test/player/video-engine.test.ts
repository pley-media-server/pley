import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { EngineListener, EngineSource } from "../../src/player/engine";
import { t } from "../../src/i18n";
import { mediaError, SEEK_LAND_NEAR_MS, SEEK_WATCHDOG_MS, VideoEngine } from "../../src/player/video-engine";

function source(over: Partial<EngineSource> = {}): EngineSource {
  return {
    url: "http://nas:7539/api/media/pley_mk_KEY/file.mkv",
    hls: false,
    mime: "video/x-matroska",
    startMs: 0,
    durationHintMs: 7_200_000,
    video: { codec: "hevc", width: 1920, height: 1080, frameRate: 24, range: "sdr", dvProfile: null, bitDepth: 8 },
    audioCodec: "eac3",
    ...over,
  };
}

/** What the engine told the session, in order. */
function listen(engine: VideoEngine) {
  const heard: string[] = [];
  const listener: EngineListener = {
    status: (s) => heard.push(`status ${s}`),
    firstFrame: () => heard.push("firstFrame"),
    error: (e) => heard.push(`error ${e.kind}`),
    tracks: () => heard.push("tracks"),
    seekFailed: (t) => heard.push(`seekFailed ${t}`),
  };
  engine.setListener(listener);
  return heard;
}

/** jsdom's <video> loads nothing: the test says how far it got and fires its events. */
function element(host: HTMLElement) {
  const video = host.querySelector("video")!;
  let readyState = 0;
  Object.defineProperty(video, "readyState", { get: () => readyState, configurable: true });
  let time = 0;
  Object.defineProperty(video, "currentTime", {
    get: () => time,
    set: (v: number) => {
      time = v;
    },
    configurable: true,
  });
  let error: { code: number; message: string } | null = null;
  Object.defineProperty(video, "error", { get: () => error, configurable: true });
  let seeking = false;
  Object.defineProperty(video, "seeking", { get: () => seeking, configurable: true });
  return {
    video,
    get time() {
      return time;
    },
    /** Where the element's own clock is, without a seek asked. */
    at(seconds: number) {
      time = seconds;
    },
    ready(state: number) {
      readyState = state;
    },
    fire(type: string) {
      if (type === "seeking") seeking = true;
      if (type === "seeked") seeking = false;
      video.dispatchEvent(new Event(type));
    },
    fail(code: number, message = "") {
      error = { code, message };
      video.dispatchEvent(new Event("error"));
    },
  };
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.spyOn(HTMLMediaElement.prototype, "load").mockImplementation(() => undefined);
  vi.spyOn(HTMLMediaElement.prototype, "pause").mockImplementation(() => undefined);
  vi.spyOn(HTMLMediaElement.prototype, "play").mockImplementation(() => Promise.resolve());
});

afterEach(() => {
  vi.useRealTimers();
  document.body.innerHTML = "";
});

function setup() {
  const host = document.createElement("div");
  document.body.appendChild(host);
  const engine = new VideoEngine(host);
  const heard = listen(engine);
  const el = element(host);
  return { host, engine, heard, el };
}

describe("a <video> engine", () => {
  it("plays the file's address as it is, and starts at the resume point once the element knows the duration", () => {
    const { engine, el } = setup();
    engine.prepare(source({ startMs: 600_000 }), true);
    expect(el.video.getAttribute("src")).toBe("http://nas:7539/api/media/pley_mk_KEY/file.mkv");
    // Not before the metadata: an element without it drops the position.
    expect(el.time).toBe(0);
    el.ready(1);
    el.fire("loadedmetadata");
    expect(el.time).toBe(600);
    engine.release();
  });

  it("seeks by setting the element's time (the TV fetches from there by HTTP range)", () => {
    const { engine, el, heard } = setup();
    engine.prepare(source(), true);
    el.ready(4);
    el.fire("loadedmetadata");
    el.fire("playing");
    engine.seekTo(1_200_000);
    expect(el.time).toBe(1200);
    el.fire("seeking");
    el.fire("seeked");
    vi.advanceTimersByTime(SEEK_WATCHDOG_MS * 2);
    expect(heard.filter((h) => h.startsWith("seekFailed"))).toEqual([]);
    engine.release();
  });

  it("keeps a seek asked before the metadata for when it comes", () => {
    const { engine, el } = setup();
    engine.prepare(source({ startMs: 600_000 }), true);
    engine.seekTo(1_500_000);
    el.ready(1);
    el.fire("loadedmetadata");
    expect(el.time).toBe(1500);
    engine.release();
  });

  it("says a seek failed when the element drops it: not seeking, never there, still where it was", () => {
    const { engine, el, heard } = setup();
    engine.prepare(source(), true);
    el.ready(4);
    el.fire("loadedmetadata");
    el.fire("playing");
    el.at(42);
    engine.seekTo(1_200_000);
    // The element let the seek go: its time is back where it played.
    el.at(43);
    vi.advanceTimersByTime(SEEK_WATCHDOG_MS - 1);
    expect(heard).not.toContain("seekFailed 1200000");
    vi.advanceTimersByTime(1);
    expect(heard).toContain("seekFailed 1200000");
    engine.release();
  });

  it("leaves a seek still fetching over a slow network to the session: no refusal while the element is seeking", () => {
    const { engine, el, heard } = setup();
    engine.prepare(source(), true);
    el.ready(4);
    el.fire("loadedmetadata");
    el.fire("playing");
    engine.seekTo(1_200_000);
    el.fire("seeking");
    vi.advanceTimersByTime(SEEK_WATCHDOG_MS * 2);
    expect(heard.filter((h) => h.startsWith("seekFailed"))).toEqual([]);
    // It gets there late: still judged, and made.
    el.at(1199);
    el.fire("seeked");
    expect(heard.filter((h) => h.startsWith("seekFailed"))).toEqual([]);
    engine.release();
  });

  it("takes a keyframe 4 s before the target as the seek made", () => {
    const { engine, el, heard } = setup();
    engine.prepare(source(), true);
    el.ready(4);
    el.fire("loadedmetadata");
    el.fire("playing");
    engine.seekTo(1_200_000);
    el.fire("seeking");
    el.at(1196);
    el.fire("seeked");
    expect(heard.filter((h) => h.startsWith("seekFailed"))).toEqual([]);
    engine.release();
  });

  it("takes a long-GOP keyframe 6-8 s before the target, far from where it was, as the seek made", () => {
    const { engine, el, heard } = setup();
    engine.prepare(source(), true);
    el.ready(4);
    el.fire("loadedmetadata");
    el.fire("playing");
    el.at(60);
    engine.seekTo(3_600_000);
    el.fire("seeking");
    el.at(3593);
    el.fire("seeked");
    expect(heard.filter((h) => h.startsWith("seekFailed"))).toEqual([]);
    engine.release();
  });

  it("takes a resume point landing on a keyframe 7 s early as made", () => {
    const { engine, el, heard } = setup();
    engine.prepare(source({ startMs: 3_600_000 }), true);
    el.ready(1);
    el.fire("loadedmetadata");
    el.at(3593);
    el.fire("seeked");
    expect(heard.filter((h) => h.startsWith("seekFailed"))).toEqual([]);
    engine.release();
  });

  it("says a seek failed when the element says it got there and plays on far from it", () => {
    const { engine, el, heard } = setup();
    engine.prepare(source(), true);
    el.ready(4);
    el.fire("loadedmetadata");
    el.fire("playing");
    engine.seekTo(1_200_000);
    el.at(42);
    el.fire("seeked");
    expect(heard).toContain("seekFailed 1200000");
    engine.release();
  });

  it("takes a keyframe near the target as the seek made", () => {
    const { engine, el, heard } = setup();
    engine.prepare(source(), true);
    el.ready(4);
    el.fire("loadedmetadata");
    el.fire("playing");
    engine.seekTo(1_200_000);
    el.at(1200 - (SEEK_LAND_NEAR_MS - 500) / 1000);
    el.fire("seeked");
    vi.advanceTimersByTime(SEEK_WATCHDOG_MS * 2);
    expect(heard.filter((h) => h.startsWith("seekFailed"))).toEqual([]);
    engine.release();
  });

  it("judges only the newest of seeks asked one after the other", () => {
    const { engine, el, heard } = setup();
    engine.prepare(source(), true);
    el.ready(4);
    el.fire("loadedmetadata");
    el.fire("playing");
    engine.seekTo(1_200_000);
    engine.seekTo(1_300_000);
    el.fire("seeked");
    vi.advanceTimersByTime(SEEK_WATCHDOG_MS * 2);
    expect(heard.filter((h) => h.startsWith("seekFailed"))).toEqual([]);
    engine.release();
  });

  it("says the resume point failed when the element starts elsewhere", () => {
    const { engine, el, heard } = setup();
    engine.prepare(source({ startMs: 600_000 }), true);
    el.ready(1);
    el.fire("loadedmetadata");
    el.at(0);
    el.fire("seeked");
    expect(heard).toContain("seekFailed 600000");
    engine.release();
  });

  it("asks for no seek of its own in a file (nothing the server makes)", () => {
    const { engine, el } = setup();
    engine.prepare(source(), true);
    el.ready(4);
    el.fire("loadedmetadata");
    engine.seekTo(1_200_000);
    expect(engine.seekPending?.() ?? false).toBe(false);
    engine.release();
  });

  it("stops judging a seek once the source is stopped", () => {
    const { engine, el, heard } = setup();
    engine.prepare(source(), true);
    el.ready(4);
    el.fire("loadedmetadata");
    engine.seekTo(1_200_000);
    engine.stop();
    vi.advanceTimersByTime(SEEK_WATCHDOG_MS * 2);
    expect(heard.filter((h) => h.startsWith("seekFailed"))).toEqual([]);
  });

  it("tells the session of the element's error, as the recovery reads it", async () => {
    const { engine, el, heard } = setup();
    engine.prepare(source(), true);
    el.fail(3, "MEDIA_ERR_DECODE");
    expect(heard).toContain("error unsupported");
    engine.release();
  });

  describe("a file it says it can't play (MEDIA_ERR_SRC_NOT_SUPPORTED)", () => {
    afterEach(() => vi.unstubAllGlobals());
    /** The element fails with code 4 as the file's address answers `answer` (a status, or nothing). */
    async function failsWith(answer: number | "unreachable" | "silent") {
      const asked: string[] = [];
      vi.stubGlobal("fetch", (url: string, init: RequestInit) => {
        asked.push(`${init.method} ${url}`);
        if (answer === "unreachable") return Promise.reject(new TypeError("Failed to fetch"));
        if (answer === "silent") return new Promise(() => undefined);
        return Promise.resolve({ status: answer });
      });
      const { engine, el, heard } = setup();
      engine.prepare(source(), true);
      el.fail(4, "MEDIA_ERR_SRC_NOT_SUPPORTED");
      await vi.advanceTimersByTimeAsync(answer === "silent" ? 5_000 : 0);
      engine.release();
      return { asked, errors: heard.filter((h) => h.startsWith("error")) };
    }

    it("asks the file's address, and takes it as the file's fault when the address answers", async () => {
      const { asked, errors } = await failsWith(206);
      expect(asked).toEqual(["HEAD http://nas:7539/api/media/pley_mk_KEY/file.mkv"]);
      expect(errors).toEqual(["error unsupported"]);
    });

    it("takes a failed address (404, or 410 once the server stopped the session) as the session's to look into", async () => {
      expect((await failsWith(404)).errors).toEqual(["error http"]);
      expect((await failsWith(410)).errors).toEqual(["error http"]);
    });

    it("takes an address nothing answers as the network's", async () => {
      expect((await failsWith("unreachable")).errors).toEqual(["error network"]);
    });

    it("takes it as the file's when the address doesn't answer in time", async () => {
      expect((await failsWith("silent")).errors).toEqual(["error unsupported"]);
    });

    it("says nothing of it once another source was asked for", async () => {
      vi.stubGlobal("fetch", () => Promise.resolve({ status: 404 }));
      const { engine, el, heard } = setup();
      engine.prepare(source(), true);
      el.fail(4, "MEDIA_ERR_SRC_NOT_SUPPORTED");
      engine.prepare(source({ url: "http://nas:7539/api/media/pley_mk_KEY/other.mkv" }), true);
      await vi.advanceTimersByTimeAsync(0);
      expect(heard.filter((h) => h.startsWith("error"))).toEqual([]);
      engine.release();
    });
  });

  it("names itself in the language shown, for Playback info", () => {
    const { engine } = setup();
    expect(engine.name).toBe(t("player.info.videoElement"));
    expect(engine.name).not.toBe("player.info.videoElement");
    engine.release();
  });

  it("leaves the host empty once released", () => {
    const { host, engine } = setup();
    engine.prepare(source(), true);
    engine.release();
    expect(host.querySelector("video")).toBeNull();
  });

  it("places the element in an area of the canvas, or over all of it", () => {
    const { engine, el } = setup();
    engine.setArea?.({ left: 480, top: 0, width: 480, height: 270 });
    expect([el.video.style.left, el.video.style.top, el.video.style.width, el.video.style.height]).toEqual(["480px", "0px", "480px", "270px"]);
    engine.setArea?.(null);
    expect(el.video.style.width).toBe("");
    engine.release();
  });
});

describe("the element's errors", () => {
  it("map to what the recovery does with them", () => {
    expect(mediaError(2, "", source())).toMatchObject({ kind: "network" });
    expect(mediaError(3, "", source())).toMatchObject({ kind: "unsupported", container: false });
    expect(mediaError(4, "", source())).toMatchObject({ kind: "unsupported" });
    expect(mediaError(4, "", source({ hls: true }))).toMatchObject({ kind: "playlist" });
    expect(mediaError(1, "", source())).toMatchObject({ kind: "other" });
  });
});

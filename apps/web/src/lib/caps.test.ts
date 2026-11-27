import { afterEach, describe, expect, it, vi } from "vitest";
import { PICTURE_CODECS } from "pley-picture-subtitles/picture-track";

/** A browser that plays `types` (`canPlayType`) and decodes `mse` through Media Source Extensions. */
function browser({ types = [], mse = [], hdrDisplay = false }: { types?: string[]; mse?: string[]; hdrDisplay?: boolean }) {
  vi.spyOn(HTMLMediaElement.prototype, "canPlayType").mockImplementation((type) => (types.includes(type) ? "probably" : ""));
  vi.stubGlobal("MediaSource", { isTypeSupported: (type: string) => mse.includes(type) });
  vi.stubGlobal("matchMedia", (query: string) => ({ matches: hdrDisplay && query === "(dynamic-range: high)" }));
}

/** A fresh copy of the module: it caches what it found. */
async function caps() {
  vi.resetModules();
  return import("./caps");
}

afterEach(() => {
  Reflect.deleteProperty(navigator, "userAgentData");
});

describe("videoCaps", () => {
  it("reports what a Chromium browser plays", async () => {
    Object.defineProperty(navigator, "userAgentData", { value: { brands: [{ brand: "Chromium" }] }, configurable: true });
    browser({
      types: ["video/mp4", "video/webm", 'video/mp4; codecs="avc1.640028"', 'video/webm; codecs="vp9"', 'video/webm; codecs="vp8"', 'audio/mp4; codecs="mp4a.40.2"', 'audio/webm; codecs="opus"', "audio/flac"],
      mse: ['video/mp4; codecs="av01.0.08M.08"'],
    });
    const { videoCaps } = await caps();

    expect(videoCaps()).toEqual({
      containers: ["mp4", "m4v", "mov", "webm", "mkv"],
      video_codecs: ["h264", "vp9", "av1", "vp8"],
      audio_codecs: ["aac", "opus", "flac"],
      hdr: false,
      dolby_vision: false,
      // Transcodes come in several sizes, which hls.js moves between.
      hls: { variants: true },
      // Pictures too: drawn over the video from their own file.
      subtitle_codecs: ["subrip", "ass", "ssa", "webvtt", "mov_text", "hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle"],
      picture_subtitles_sideload: true,
    });
  });

  it("stays a version 1 report, whose rules the server keeps for browsers", async () => {
    browser({ types: ["video/mp4"] });
    const { videoCaps } = await caps();
    expect(videoCaps()).not.toHaveProperty("caps_version");
  });

  it("names every picture subtitle codec the player decodes", async () => {
    const { PICTURE_SUBTITLES } = await caps();
    expect([...PICTURE_SUBTITLES].sort()).toEqual(Object.values(PICTURE_CODECS).sort());
  });

  it("reports HEVC, Dolby Vision and HDR where the hardware has them", async () => {
    browser({
      types: ["video/mp4", 'video/mp4; codecs="hvc1.1.6.L120.90"', 'video/mp4; codecs="hvc1.2.4.L153.B0"', 'video/mp4; codecs="dvh1.05.06"', 'audio/mp4; codecs="ec-3"', 'audio/mp4; codecs="alac"'],
      hdrDisplay: true,
    });
    const { videoCaps } = await caps();
    const found = videoCaps();

    expect(found.containers).not.toContain("mkv");
    expect(found.video_codecs).toEqual(["hevc"]);
    expect(found.audio_codecs).toEqual(["eac3", "alac"]);
    expect(found).toMatchObject({ hdr: true, dolby_vision: true });
  });

  it("needs an HDR screen as well as a 10-bit decoder for HDR", async () => {
    browser({ types: ['video/mp4; codecs="hvc1.2.4.L153.B0"'], hdrDisplay: false });
    const { videoCaps } = await caps();
    expect(videoCaps().hdr).toBe(false);
  });

  it("lets desktop Chromium tone-map HDR for an SDR screen", async () => {
    Object.defineProperty(navigator, "userAgentData", { value: { brands: [{ brand: "Chromium" }] }, configurable: true });
    browser({ types: ['video/mp4; codecs="hvc1.2.4.L153.B0"'], hdrDisplay: false });
    expect((await caps()).videoCaps().hdr).toBe(true);

    browser({ types: [], hdrDisplay: false });
    expect((await caps()).videoCaps().hdr).toBe(false);

    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Mozilla/5.0 (Linux; Android 15) Chrome/152.0.0.0 Mobile");
    browser({ types: ['video/mp4; codecs="hvc1.2.4.L153.B0"'], hdrDisplay: false });
    expect((await caps()).videoCaps().hdr).toBe(false);
  });

  it("probes once", async () => {
    browser({ types: ["video/mp4"] });
    const { videoCaps } = await caps();
    expect(videoCaps()).toBe(videoCaps());
  });
});

describe("audioCaps", () => {
  it("reports the containers and codecs of the audio element", async () => {
    browser({ types: ["audio/mpeg", "audio/mp4", "audio/flac", 'audio/mp4; codecs="mp4a.40.2"', "audio/wav"] });
    const { audioCaps } = await caps();

    expect(audioCaps()).toEqual({
      containers: ["mp3", "m4a", "mp4", "m4b", "flac", "wav"],
      video_codecs: [],
      audio_codecs: ["aac", "mp3", "flac", "pcm_s16le", "pcm_s24le"],
    });
  });
});

describe("canPlayNativeHls", () => {
  it("asks the element", async () => {
    browser({ types: ["application/vnd.apple.mpegurl"] });
    const { canPlayNativeHls } = await caps();
    expect(canPlayNativeHls(document.createElement("video"))).toBe(true);
  });
});

describe("streamCaps", () => {
  it("asks Media Source Extensions, not the element, for each codec at 8 and at 10 bits", async () => {
    browser({
      // What the element says doesn't count: mpegts.js plays through MSE.
      types: ['video/mp4; codecs="hvc1.2.4.L153.B0"', 'audio/mp4; codecs="opus"'],
      mse: [
        'video/mp4; codecs="avc1.640028"',
        'video/mp4; codecs="avc1.6E0028"',
        'video/mp4; codecs="hvc1.1.6.L120.90"',
        'audio/mp4; codecs="mp4a.40.2"',
        'audio/mp4; codecs="ac-3"',
      ],
    });
    const { streamCaps } = await caps();

    expect(streamCaps()).toEqual({
      video: [
        { codec: "h264", max_bit_depth: 10 },
        { codec: "hevc", max_bit_depth: 8 },
      ],
      // AAC however the stream carries it.
      audio_codecs: ["aac", "aac_latm", "ac3"],
    });
  });

  it("decodes HEVC at 10 bits where Main 10 is there", async () => {
    browser({ mse: ['video/mp4; codecs="hvc1.1.6.L120.90"', 'video/mp4; codecs="hvc1.2.4.L153.B0"'] });
    expect((await caps()).streamCaps().video).toEqual([{ codec: "hevc", max_bit_depth: 10 }]);
  });

  it("leaves out what mpegts.js doesn't play from a stream: E-AC-3, AV1 and MPEG-2 video", async () => {
    browser({ mse: ['video/mp4; codecs="av01.0.08M.08"', 'audio/mp4; codecs="ec-3"', 'audio/mp4; codecs="mp4a.40.2"', 'audio/mp4; codecs="opus"'] });
    expect((await caps()).streamCaps()).toEqual({ video: [], audio_codecs: ["aac", "aac_latm", "opus"] });
  });

  it("asks for MPEG audio as mpegts.js hands it over, and plays MP2 only in Chromium", async () => {
    // Chromium takes MP3 as it is, not in MP4, and its decoder plays MP2 too.
    Object.defineProperty(navigator, "userAgentData", { value: { brands: [{ brand: "Chromium" }] }, configurable: true });
    browser({ mse: ["audio/mpeg"] });
    expect((await caps()).streamCaps().audio_codecs).toEqual(["mp3", "mp2"]);

    Reflect.deleteProperty(navigator, "userAgentData");
    expect((await caps()).streamCaps().audio_codecs).toEqual(["mp3"]);

    // Firefox gets it in MP4.
    vi.spyOn(navigator, "userAgent", "get").mockReturnValue("Mozilla/5.0 (X11; Linux x86_64; rv:150.0) Gecko/20100101 Firefox/150.0");
    expect((await caps()).streamCaps().audio_codecs).toEqual([]);
    browser({ mse: ['audio/mp4; codecs="mp3"'] });
    expect((await caps()).streamCaps().audio_codecs).toEqual(["mp3"]);
  });

  it("plays nothing from a stream without Media Source Extensions", async () => {
    browser({ types: ["video/mp4", 'video/mp4; codecs="avc1.640028"'] });
    vi.stubGlobal("MediaSource", undefined);
    expect((await caps()).streamCaps()).toEqual({ video: [], audio_codecs: [] });
  });
});

describe("playsStreamTrack", () => {
  it("asks for the video as the stream has it", async () => {
    browser({ mse: ['video/mp4; codecs="hvc1.1.1.L123.B0"'] });
    const { playsStreamTrack } = await caps();
    expect(playsStreamTrack("video", "hvc1.1.1.L123.B0")).toBe(true);
    expect(playsStreamTrack("video", "hvc1.2.1.L153.B0")).toBe(false);
  });

  it("plays only the codecs a stream is played with", async () => {
    browser({ mse: ['video/mp4; codecs="vp09.00.10.08"', 'audio/mp4; codecs="ec-3"', 'audio/mp4; codecs="ac-3"', 'audio/mp4; codecs="mp4a.40.2"'] });
    const { playsStreamTrack } = await caps();
    expect(playsStreamTrack("video", "vp09.00.10.08")).toBe(false);
    expect(playsStreamTrack("audio", "ec-3")).toBe(false);
    expect(playsStreamTrack("audio", "ac-3")).toBe(true);
    expect(playsStreamTrack("audio", "mp4a.40.5", "aac_latm")).toBe(true);
    // Not the track the server named (a stream with several): it plays all the same.
    expect(playsStreamTrack("audio", "ac-3", "aac")).toBe(true);
  });

  it("plays MPEG audio the server didn't name only where MP2 plays too", async () => {
    browser({ mse: ["audio/mpeg"] });
    const elsewhere = await caps();
    // mpegts.js writes "mp3" for MP2 as well.
    expect(elsewhere.playsStreamTrack("audio", "mp3")).toBe(false);
    expect(elsewhere.playsStreamTrack("audio", "mp3", "mp2")).toBe(false);
    expect(elsewhere.playsStreamTrack("audio", "mp3", "mp3")).toBe(true);

    Object.defineProperty(navigator, "userAgentData", { value: { brands: [{ brand: "Chromium" }] }, configurable: true });
    const chromium = await caps();
    expect(chromium.playsStreamTrack("audio", "mp3")).toBe(true);
    expect(chromium.playsStreamTrack("audio", "mp3", "mp2")).toBe(true);
  });
});

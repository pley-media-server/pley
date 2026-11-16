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
      hdr_display: false,
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
    expect(found).toMatchObject({ hdr: true, hdr_display: true, dolby_vision: true });
  });

  it("needs an HDR screen as well as a 10-bit decoder for HDR", async () => {
    browser({ types: ['video/mp4; codecs="hvc1.2.4.L153.B0"'], hdrDisplay: false });
    const { videoCaps } = await caps();
    expect(videoCaps().hdr).toBe(false);
  });

  it("lets desktop Chromium tone-map HDR for an SDR screen", async () => {
    Object.defineProperty(navigator, "userAgentData", { value: { brands: [{ brand: "Chromium" }] }, configurable: true });
    browser({ types: ['video/mp4; codecs="hvc1.2.4.L153.B0"'], hdrDisplay: false });
    expect((await caps()).videoCaps()).toMatchObject({ hdr: true, hdr_display: false });

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

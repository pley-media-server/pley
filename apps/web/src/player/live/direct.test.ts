import { afterEach, describe, expect, it, vi } from "vitest";
import type { SessionPlan } from "@/api/livetv";
import { tracksPlay } from "./direct";

/** A browser whose Media Source Extensions take `types`. */
function mse(...types: string[]) {
  vi.stubGlobal("MediaSource", { isTypeSupported: (type: string) => types.includes(type) });
}

const AVC = 'video/mp4; codecs="avc1.640028"';
const MAIN10 = 'video/mp4; codecs="hvc1.2.1.L153.B0"';
const AAC = 'audio/mp4; codecs="mp4a.40.2"';
const AC3 = 'audio/mp4; codecs="ac-3"';
const hevcAac = { hasVideo: true, hasAudio: true, videoCodec: "hvc1.2.1.L153.B0", audioCodec: "mp4a.40.2" };

const plan = (p: Partial<SessionPlan>): SessionPlan => ({
  video_codec: "hevc",
  width: 3840,
  height: 2160,
  frame_rate: 50,
  interlaced: false,
  bit_depth: 10,
  audio_codec: "aac",
  audio_channels: 2,
  video: "h264_nvenc",
  audio: "copy",
  ...p,
});

afterEach(() => vi.unstubAllGlobals());

describe("tracksPlay", () => {
  it("plays H.264 and AAC whatever the browser says, and nothing it didn't find", () => {
    mse();
    expect(tracksPlay({ hasVideo: true, hasAudio: true, videoCodec: "avc1.640028", audioCodec: "mp4a.40.5" }, null)).toBe(true);
    expect(tracksPlay({ hasVideo: true, videoCodec: "avc1.640028" }, null)).toBe(true);
    expect(tracksPlay({}, null)).toBe(false);
    expect(tracksPlay(null, null)).toBe(false);
  });

  it("plays HEVC as the stream has it where the browser decodes that, ", () => {
    mse(MAIN10, AAC);
    expect(tracksPlay(hevcAac, null)).toBe(true);
    expect(tracksPlay(hevcAac, plan({}))).toBe(true);
    // Main only: the stream's Main 10 isn't decoded.
    mse('video/mp4; codecs="hvc1.1.1.L153.B0"', AAC);
    expect(tracksPlay(hevcAac, plan({}))).toBe(false);
  });

  it("plays AC-3 where the browser decodes it, never E-AC-3", () => {
    mse(AVC, AC3, 'audio/mp4; codecs="ec-3"', AAC);
    expect(tracksPlay({ hasVideo: true, hasAudio: true, videoCodec: "avc1.640028", audioCodec: "ac-3" }, null)).toBe(true);
    expect(tracksPlay({ hasVideo: true, hasAudio: true, videoCodec: "avc1.640028", audioCodec: "ec-3" }, null)).toBe(false);
    mse(AVC, AAC);
    expect(tracksPlay({ hasVideo: true, hasAudio: true, videoCodec: "avc1.640028", audioCodec: "ac-3" }, null)).toBe(false);
  });

  it("plays MPEG audio only when the server said it is MP3", () => {
    mse(AVC, "audio/mpeg");
    const mpeg = { hasVideo: true, hasAudio: true, videoCodec: "avc1.640028", audioCodec: "mp3" };
    // It may be MP2, which mpegts.js writes the same.
    expect(tracksPlay(mpeg, null)).toBe(false);
    expect(tracksPlay(mpeg, plan({ video_codec: "h264", video: "copy", audio_codec: "mp2", audio: "aac" }))).toBe(false);
    expect(tracksPlay(mpeg, plan({ video_codec: "h264", video: "copy", audio_codec: "mp3", audio: "aac" }))).toBe(true);
  });

  it("doesn't play a stream missing a track the server said the channel has", () => {
    mse(MAIN10, AAC);
    // Sound mpegts.js didn't read: the picture alone would play.
    expect(tracksPlay({ hasVideo: true, videoCodec: "hvc1.2.1.L153.B0" }, plan({}))).toBe(false);
    expect(tracksPlay({ hasAudio: true, audioCodec: "mp4a.40.2" }, plan({}))).toBe(false);
  });
});

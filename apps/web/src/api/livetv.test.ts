import { describe, expect, it } from "vitest";
import type { StreamCaps } from "@/lib/caps";
import { notAsSent, playsAsSent, type SessionPlan } from "./livetv";

/** A 1080p HEVC channel with AAC sound, as a server with Direct Play on plans it: its video transcoded. */
const hevc = (plan: Partial<SessionPlan> = {}): SessionPlan => ({
  video_codec: "hevc",
  width: 1920,
  height: 1080,
  frame_rate: 50,
  interlaced: false,
  bit_depth: 8,
  audio_codec: "aac",
  audio_channels: 2,
  video: "h264_nvenc",
  audio: "copy",
  out_height: 1080,
  video_kbps: 8000,
  direct_play: true,
  kbps: 6000,
  ...plan,
});

/** A browser that decodes HEVC up to `depth` bits, and AAC. */
const decodesHevc = (depth: number): StreamCaps => ({
  video: [
    { codec: "h264", max_bit_depth: 8 },
    { codec: "hevc", max_bit_depth: depth },
  ],
  audio_codecs: ["aac"],
});
const h264Only: StreamCaps = { video: [{ codec: "h264", max_bit_depth: 8 }], audio_codecs: ["aac"] };

describe("playsAsSent", () => {
  it("plays a channel sent as it is, whatever the device and the setting", () => {
    const copied = hevc({ video_codec: "h264", video: "copy", direct_play: false });
    expect(playsAsSent(copied)).toBe(true);
    expect(playsAsSent(copied, null)).toBe(true);
    expect(playsAsSent(copied, { video: [], audio_codecs: [] })).toBe(true);
    // Radio, and a channel without sound.
    expect(playsAsSent(hevc({ video_codec: null, video: "none" }))).toBe(true);
    expect(playsAsSent(hevc({ video_codec: "h264", video: "copy", audio_codec: null, audio: "none" }))).toBe(true);
    // Nothing in it at all.
    expect(playsAsSent(hevc({ video: "none", audio: "none" }), decodesHevc(10))).toBe(false);
  });

  it("plays HEVC where the device decodes it, at its bit depth", () => {
    expect(playsAsSent(hevc(), decodesHevc(8))).toBe(true);
    expect(playsAsSent(hevc({ bit_depth: 10 }), decodesHevc(10))).toBe(true);
    expect(playsAsSent(hevc({ bit_depth: 10 }), decodesHevc(8))).toBe(false);
    expect(playsAsSent(hevc({ bit_depth: 12 }), decodesHevc(10))).toBe(false);
    expect(playsAsSent(hevc(), h264Only)).toBe(false);
    expect(notAsSent(hevc({ bit_depth: 10 }), decodesHevc(8))).toEqual({ why: "video", codec: "hevc" });
  });

  it("takes video whose bit depth the server doesn't say as 8-bit", () => {
    expect(playsAsSent(hevc({ bit_depth: null }), decodesHevc(8))).toBe(true);
    expect(playsAsSent(hevc({ bit_depth: undefined }), decodesHevc(8))).toBe(true);
  });

  it("keeps only the rule for channels sent as they are without the device's decoders", () => {
    expect(playsAsSent(hevc())).toBe(false);
    expect(playsAsSent(hevc(), null)).toBe(false);
  });

  it("leaves interlaced video to the transcoder unless it is copied", () => {
    const interlaced = hevc({ video_codec: "h264", interlaced: true });
    expect(playsAsSent(interlaced, decodesHevc(10))).toBe(false);
    expect(notAsSent(interlaced, decodesHevc(10))).toEqual({ why: "interlaced" });
    // Deinterlace is off on the server: it copies the channel.
    expect(playsAsSent({ ...interlaced, video: "copy" }, decodesHevc(10))).toBe(true);
  });

  it("plays nothing more while Direct Play is off on the server", () => {
    expect(playsAsSent(hevc({ direct_play: false }), decodesHevc(10))).toBe(false);
    expect(notAsSent(hevc({ direct_play: false }), decodesHevc(10))).toEqual({ why: "off" });
  });

  it("plays as before with an older server, which says nothing of Direct Play", () => {
    // What an older server sends: none of the new fields.
    const old = { video_codec: "hevc", width: 1920, height: 1080, frame_rate: 50, interlaced: false, audio_codec: "aac", audio_channels: 2 };
    expect(playsAsSent({ ...old, video: "h264_nvenc", audio: "copy" }, decodesHevc(10))).toBe(false);
    expect(notAsSent({ ...old, video: "h264_nvenc", audio: "copy" }, decodesHevc(10))).toEqual({ why: "older" });
    expect(playsAsSent({ ...old, video_codec: "h264", video: "copy", audio: "copy" }, decodesHevc(10))).toBe(true);
  });

  it("needs the sound decoded too", () => {
    const ac3 = hevc({ audio_codec: "ac3", audio_channels: 6, audio: "aac" });
    expect(playsAsSent(ac3, decodesHevc(10))).toBe(false);
    expect(notAsSent(ac3, decodesHevc(10))).toEqual({ why: "audio", codec: "ac3" });
    expect(playsAsSent(ac3, { ...decodesHevc(10), audio_codecs: ["aac", "ac3"] })).toBe(true);
    // Copied video with sound the transcoder converts.
    const h264 = { ...ac3, video_codec: "h264", video: "copy" };
    expect(playsAsSent(h264, h264Only)).toBe(false);
    expect(playsAsSent(h264, { ...h264Only, audio_codecs: ["aac", "ac3"] })).toBe(true);
    // Sound the server couldn't name.
    expect(notAsSent(hevc({ audio_codec: null, audio: "aac" }), decodesHevc(10))).toEqual({ why: "audio", codec: null });
  });

  it("doesn't play video the server couldn't name", () => {
    expect(notAsSent(hevc({ video_codec: null }), decodesHevc(10))).toEqual({ why: "video", codec: null });
  });
});

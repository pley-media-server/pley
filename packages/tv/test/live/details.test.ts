import { afterEach, describe, expect, it } from "vitest";
import type { SessionPlan } from "../../src/api/types";
import { setLanguage } from "../../src/i18n";
import { liveFacts, liveTranscodes } from "../../src/live/details";
import { methodText } from "../../src/player/details";

/** A channel's plan as the server says it: H.264 and AAC copied, unless `over` says otherwise. */
function plan(over: Partial<SessionPlan> = {}): SessionPlan {
  return {
    video_codec: "h264",
    width: 1920,
    height: 1080,
    frame_rate: 25,
    interlaced: false,
    bit_depth: 8,
    audio_codec: "aac",
    audio_channels: 2,
    video: "copy",
    audio: "copy",
    out_height: null,
    video_kbps: null,
    kbps: 6200,
    ...over,
  };
}

/** The rows as "label: value" lines. */
const rows = (p: SessionPlan) => liveFacts(p).facts.map((f) => `${f.label}: ${f.value}`);

afterEach(() => setLanguage("en"));

describe("a live channel's Playback info", () => {
  it("is a direct stream when the server copies the video, with what the provider sends", () => {
    expect(liveFacts(plan()).title).toBe("How it plays");
    expect(rows(plan())).toEqual(["Playback: Direct stream", "Video: H.264 8-bit · 1920×1080 · 25 fps", "Audio: AAC Stereo", "Bitrate: 6.2 Mbps"]);
  });

  it("names HEVC at 10 bits, 4K and surround sound as the provider sends them", () => {
    const hevc = plan({ video_codec: "hevc", bit_depth: 10, width: 3840, height: 2160, frame_rate: 50, audio_codec: "eac3", audio_channels: 6, kbps: 18_400 });
    expect(rows(hevc)).toEqual(["Playback: Direct stream", "Video: HEVC 10-bit · 3840×2160 · 50 fps", "Audio: EAC3 5.1", "Bitrate: 18 Mbps"]);
  });

  it("is a transcode when the server converts the video: what it makes, at what bitrate, with which encoder", () => {
    const made = plan({ video_codec: "hevc", bit_depth: 10, width: 3840, height: 2160, frame_rate: 50, video: "h264_nvenc", out_height: 1080, video_kbps: 8000, kbps: 12_300 });
    expect(liveTranscodes(made)).toBe(true);
    expect(rows(made)).toEqual([
      "Playback: Transcoding",
      "Video: HEVC 10-bit · 3840×2160 · 50 fps → H.264 1080p",
      "Audio: AAC Stereo",
      "Bitrate: 12 Mbps → 8.0 Mbps",
      "Encoder: NVIDIA NVENC",
    ]);
  });

  it("says interlaced video, and a transcode that keeps the provider's lines", () => {
    const interlaced = plan({ video_codec: "mpeg2video", width: 720, height: 576, interlaced: true, video: "libx264", audio_codec: "mp2", audio: "aac", kbps: 850 });
    expect(rows(interlaced)).toEqual([
      "Playback: Transcoding",
      "Video: MPEG-2 8-bit · 720×576 · 25 fps, interlaced → H.264 576p",
      "Audio: MP2 Stereo → AAC Stereo",
      "Bitrate: 850 kbps",
      "Encoder: x264",
    ]);
  });

  it("stays a direct stream when only the sound is converted, and says so as a film does", () => {
    const sound = plan({ audio_codec: "ac3", audio_channels: 6, audio: "aac" });
    expect(liveTranscodes(sound)).toBe(false);
    expect(rows(sound)).toEqual(["Playback: Direct stream · Audio transcoding", "Video: H.264 8-bit · 1920×1080 · 25 fps", "Audio: AC3 5.1 → AAC Stereo", "Bitrate: 6.2 Mbps"]);
    // The film's own words for it.
    expect(liveFacts(sound).facts[0]!.value).toBe(methodText({ method: "remux", audio_converted: true }));
    // A transcode says Transcoding alone, whatever becomes of the sound.
    expect(rows(plan({ video: "libx264", audio: "aac" }))[0]).toBe("Playback: Transcoding");
  });

  it("names an encoder it doesn't know as the server does, and channels it has no name for by their count", () => {
    const odd = plan({ video: "h264_newthing", audio_channels: 4 });
    expect(rows(odd)).toContain("Encoder: h264_newthing");
    expect(rows(odd)).toContain("Audio: AAC 4ch");
  });

  it("leaves out what isn't known", () => {
    // An older server: no bit depth, no bitrate, nothing of a transcode's size.
    const older = plan({ bit_depth: null, kbps: 0, video: "libx264", out_height: null, video_kbps: null });
    expect(rows(older)).toEqual(["Playback: Transcoding", "Video: H.264 · 1920×1080 · 25 fps → H.264 1080p", "Audio: AAC Stereo", "Encoder: x264"]);
    // The provider's bitrate not measured yet: a transcode's isn't said alone.
    expect(rows(plan({ kbps: 0, video: "libx264", video_kbps: 8000 })).some((r) => r.indexOf("Bitrate") === 0)).toBe(false);
    const bare = plan({ video_codec: null, width: null, height: null, frame_rate: null, bit_depth: null, audio_codec: null, audio_channels: null, kbps: 0 });
    expect(rows(bare)).toEqual(["Playback: Direct stream"]);
    expect(rows(plan({ frame_rate: null, width: null, audio_channels: null }))).toEqual(["Playback: Direct stream", "Video: H.264 8-bit", "Audio: AAC", "Bitrate: 6.2 Mbps"]);
  });

  it("has no video row for a radio channel, and no audio row for a channel without sound", () => {
    const radio = plan({ video_codec: null, width: null, height: null, frame_rate: null, bit_depth: null, video: "none", audio_codec: "mp3", kbps: 128 });
    expect(rows(radio)).toEqual(["Playback: Direct stream", "Audio: MP3 Stereo", "Bitrate: 128 kbps"]);
    expect(rows({ ...radio, audio: "aac" })).toEqual(["Playback: Direct stream · Audio transcoding", "Audio: MP3 Stereo → AAC Stereo", "Bitrate: 128 kbps"]);
    expect(rows(plan({ audio_codec: null, audio_channels: null, audio: "none" }))).toEqual(["Playback: Direct stream", "Video: H.264 8-bit · 1920×1080 · 25 fps", "Bitrate: 6.2 Mbps"]);
  });

  it("never says Direct play, nor why: this TV plays Live TV through the server's HLS alone", () => {
    for (const p of [plan(), plan({ interlaced: true, video: "libx264" }), plan({ video_codec: "hevc", bit_depth: 10 })]) {
      const facts = liveFacts(p).facts;
      expect(facts[0]!.value).not.toBe("Direct play");
      expect(facts.map((f) => f.label)).not.toContain("Why");
    }
  });

  it("is in the language shown", () => {
    setLanguage("fr");
    expect(rows(plan({ interlaced: true, video: "libx264", kbps: 6200, video_kbps: 3500 }))).toEqual([
      "Lecture: Transcodage",
      "Vidéo: H.264 8 bits · 1920×1080 · 25 i/s, entrelacé → H.264 1080p",
      "Audio: AAC Stéréo",
      "Débit: 6,2 Mbps → 3,5 Mbps",
      "Encodeur: x264",
    ]);
    expect(rows(plan({ audio_codec: "ac3", audio: "aac" }))[0]).toBe("Lecture: Flux direct · Audio transcodé");
  });
});

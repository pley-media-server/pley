import { act, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SessionPlan } from "@/api/livetv";
import type { StreamCaps } from "@/lib/caps";
import { mockApi, renderWithProviders, type FakeRequest } from "@/test/render";
import { liveAudio, liveBitrate, LiveDetails, liveEncoder, liveMethod, liveVideo, liveWhy } from "./LiveDetails";

/** A 4K HEVC channel with 5.1 sound, both converted for the transcoder's segments. */
const uhd = (plan: Partial<SessionPlan> = {}): SessionPlan => ({
  video_codec: "hevc",
  width: 3840,
  height: 2160,
  frame_rate: 50,
  interlaced: false,
  bit_depth: 10,
  audio_codec: "eac3",
  audio_channels: 6,
  video: "h264_nvenc",
  audio: "aac",
  out_height: 1080,
  video_kbps: 8000,
  kbps: 12_300,
  ...plan,
});
/** An H.264 and AAC channel, sent as it is. */
const copied = (plan: Partial<SessionPlan> = {}): SessionPlan =>
  uhd({ video_codec: "h264", width: 1920, height: 1080, frame_rate: 25, bit_depth: 8, audio_codec: "aac", audio_channels: 2, video: "copy", audio: "copy", out_height: null, video_kbps: null, kbps: 850, ...plan });

const hevcAndAac: StreamCaps = { video: [{ codec: "hevc", max_bit_depth: 10 }], audio_codecs: ["aac"] };

describe("liveMethod", () => {
  it("names the way a channel plays as a file's are", () => {
    expect(liveMethod(true, uhd())).toEqual({ method: "direct" });
    expect(liveMethod(true, null)).toEqual({ method: "direct" });
    expect(liveMethod(false, copied())).toEqual({ method: "remux", audio_converted: false });
    expect(liveMethod(false, copied({ audio_codec: "mp2", audio: "aac" }))).toEqual({ method: "remux", audio_converted: true });
    expect(liveMethod(false, copied({ video_codec: null, video: "none" }))).toEqual({ method: "remux", audio_converted: false });
    expect(liveMethod(false, uhd())).toEqual({ method: "transcode" });
    // The transcoder hasn't said what it makes yet.
    expect(liveMethod(false, null)).toBeNull();
  });
});

describe("liveWhy", () => {
  it("says why a channel isn't played from its stream", () => {
    expect(liveWhy(uhd({ interlaced: true }), hevcAndAac)).toBe("Interlaced video is made progressive");
    expect(liveWhy(uhd({ video_codec: "mpeg2video" }), hevcAndAac)).toBe("This device doesn't play MPEG-2");
    expect(liveWhy(uhd(), { ...hevcAndAac, video: [{ codec: "hevc", max_bit_depth: 8 }] })).toBe("This device doesn't play HEVC");
    expect(liveWhy(uhd(), hevcAndAac)).toBe("This device doesn't play EAC3 sound");
  });

  it("says nothing when the rule would have played it, or the codec isn't known", () => {
    // It fell back from its stream, say.
    expect(liveWhy(copied(), hevcAndAac)).toBeNull();
    expect(liveWhy(uhd({ audio_codec: "aac", audio: "copy" }), hevcAndAac)).toBeNull();
    expect(liveWhy(uhd({ video_codec: null }), hevcAndAac)).toBeNull();
  });
});

describe("the rows", () => {
  it("show what the provider sends, and what the transcoder makes of it", () => {
    expect(liveVideo(uhd(), false)).toBe("HEVC 10-bit · 3840×2160 · 50 fps → H.264 1080p");
    expect(liveAudio(uhd(), false)).toBe("EAC3 5.1 → AAC 2.0");
    expect(liveBitrate(uhd(), false)).toBe("12 Mbps → 8.0 Mbps");
    expect(liveEncoder(uhd(), false)).toBe("NVIDIA NVENC");
  });

  it("show only what the provider sends for a channel played from its stream", () => {
    expect(liveVideo(uhd(), true)).toBe("HEVC 10-bit · 3840×2160 · 50 fps");
    expect(liveAudio(uhd(), true)).toBe("EAC3 5.1");
    expect(liveBitrate(uhd(), true)).toBe("12 Mbps");
    expect(liveEncoder(uhd(), true)).toBeNull();
  });

  it("show a copied channel as it is", () => {
    expect(liveVideo(copied(), false)).toBe("H.264 · 1920×1080 · 25 fps");
    expect(liveAudio(copied(), false)).toBe("AAC 2.0");
    expect(liveBitrate(copied(), false)).toBe("850 kbps");
    expect(liveEncoder(copied(), false)).toBeNull();
  });

  it("say interlaced video is, and keep a transcode at the source's lines when the server gives none", () => {
    const sd = uhd({ video_codec: "mpeg2video", width: 720, height: 576, frame_rate: 25, interlaced: true, bit_depth: 8, video: "libx264", out_height: null });
    expect(liveVideo(sd, false)).toBe("MPEG-2 · 720×576 · 25 fps · interlaced → H.264 576p");
    expect(liveEncoder(sd, false)).toBe("x264");
    expect(liveVideo(uhd({ frame_rate: 30000 / 1001 }), true)).toBe("HEVC 10-bit · 3840×2160 · 29.97 fps");
  });

  it("name the sound's channels", () => {
    expect(liveAudio(copied({ audio_channels: 1 }), false)).toBe("AAC 1.0");
    expect(liveAudio(copied({ audio_channels: 8 }), false)).toBe("AAC 7.1");
    expect(liveAudio(copied({ audio_channels: 4 }), false)).toBe("AAC 4ch");
    expect(liveAudio(copied({ audio_channels: null }), false)).toBe("AAC");
  });

  it("leave out what isn't known", () => {
    // Radio, and a channel without sound.
    expect(liveVideo(copied({ video_codec: null, width: null, height: null, frame_rate: null, video: "none" }), false)).toBeNull();
    expect(liveAudio(copied({ audio_codec: null, audio_channels: null, audio: "none" }), false)).toBeNull();
    // Nothing came yet, and an older server, which says neither.
    expect(liveBitrate(copied({ kbps: 0 }), false)).toBeNull();
    expect(liveBitrate(copied({ kbps: undefined }), false)).toBeNull();
    expect(liveVideo(uhd({ bit_depth: undefined, out_height: undefined }), false)).toBe("HEVC · 3840×2160 · 50 fps → H.264 2160p");
    expect(liveBitrate(uhd({ video_kbps: undefined }), false)).toBe("12 Mbps");
  });
});

describe("LiveDetails", () => {
  let answer: SessionPlan | null = null;
  let requests: FakeRequest[] = [];
  const asked = () => requests.filter((r) => r.path === "/api/live/sessions/s1/plan").length;
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    // This browser decodes HEVC through Media Source Extensions, and AAC.
    vi.stubGlobal("MediaSource", {
      isTypeSupported: (type: string) => ['video/mp4; codecs="hvc1.1.6.L120.90"', 'video/mp4; codecs="hvc1.2.4.L153.B0"', 'audio/mp4; codecs="mp4a.40.2"'].includes(type),
    });
    requests = mockApi({ "GET /api/live/sessions/s1/plan": () => answer });
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("shows a transcoded channel, and why it isn't played directly", async () => {
    answer = uhd();
    renderWithProviders(<LiveDetails sessionId="s1" playback={{ direct: false, reads: true }} open />);
    expect(await screen.findByText("Transcoding")).toBeInTheDocument();
    expect(screen.getByText("This device doesn't play EAC3 sound")).toBeInTheDocument();
    expect(screen.getByText("HEVC 10-bit · 3840×2160 · 50 fps → H.264 1080p")).toBeInTheDocument();
    expect(screen.getByText("EAC3 5.1 → AAC 2.0")).toBeInTheDocument();
    expect(screen.getByText("12 Mbps → 8.0 Mbps")).toBeInTheDocument();
    expect(screen.getByText("NVIDIA NVENC")).toBeInTheDocument();
  });

  it("asks again every few seconds while it shows, for the bitrate", async () => {
    answer = copied({ kbps: 0 });
    const { unmount } = renderWithProviders(<LiveDetails sessionId="s1" playback={{ direct: true, reads: true }} open />);
    expect(await screen.findByText("H.264 · 1920×1080 · 25 fps")).toBeInTheDocument();
    expect(screen.getByText("Direct play")).toBeInTheDocument();
    expect(screen.queryByText("Bitrate")).not.toBeInTheDocument();
    expect(asked()).toBe(1);

    answer = copied({ kbps: 4200 });
    await act(() => vi.advanceTimersByTimeAsync(3000));
    expect(await screen.findByText("4.2 Mbps")).toBeInTheDocument();
    expect(asked()).toBe(2);

    // Closed: nothing more is asked.
    unmount();
    await act(() => vi.advanceTimersByTimeAsync(10_000));
    expect(asked()).toBe(2);
  });

  it("shows how it plays alone when the server doesn't know the channel yet", async () => {
    answer = null;
    renderWithProviders(<LiveDetails sessionId="s1" playback={{ direct: true, reads: true }} open />);
    expect(screen.getByText("Direct play")).toBeInTheDocument();
    await act(() => vi.advanceTimersByTimeAsync(100));
    expect(screen.queryByText("Video")).not.toBeInTheDocument();
    expect(screen.queryByText("Audio")).not.toBeInTheDocument();
  });

  it("gives no reason where the browser reads no stream: every channel comes from the transcoder", async () => {
    answer = uhd({ interlaced: true });
    renderWithProviders(<LiveDetails sessionId="s1" playback={{ direct: false, reads: false }} open />);
    expect(await screen.findByText("Transcoding")).toBeInTheDocument();
    expect(screen.queryByText("Interlaced video is made progressive")).not.toBeInTheDocument();
  });

  it("asks nothing while the channel tunes", async () => {
    renderWithProviders(<LiveDetails sessionId={null} playback={null} open />);
    await act(() => vi.advanceTimersByTimeAsync(6000));
    expect(requests).toHaveLength(0);
  });
});

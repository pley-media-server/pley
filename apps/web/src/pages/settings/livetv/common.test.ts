import { describe, expect, it } from "vitest";
import type { LivePlan } from "@/api/livetv";
import { planLine } from "./common";

const mpeg2: LivePlan = {
  video_codec: "mpeg2video",
  width: 1920,
  height: 1080,
  frame_rate: 25,
  interlaced: true,
  bit_depth: 8,
  audio_codec: "mp2",
  audio_channels: 2,
  video: "h264_nvenc",
  audio: "aac",
  out_height: 1080,
  video_kbps: 8000,
};

describe("planLine", () => {
  it("says what the transcoder makes of a channel", () => {
    expect(planLine(mpeg2)).toBe("1080i MPEG-2 → H.264 on NVIDIA, deinterlaced · MP2 → AAC");
  });

  it("says what a channel is while nothing transcodes it", () => {
    // Every player reads the provider's stream: nothing is made.
    expect(planLine(mpeg2, false)).toBe("1080i MPEG-2 as it is · MP2 as it is");
    expect(planLine({ ...mpeg2, video: "none", audio: "copy" }, false)).toBe("radio · MP2 as it is");
  });
});

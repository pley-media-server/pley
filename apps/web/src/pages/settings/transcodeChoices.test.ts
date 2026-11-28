import { describe, expect, it } from "vitest";
import type { HwAccel, SystemInfo, ToneMapping } from "@/api/types";
import { decodingReason, fitToAccel, presetNote, tonemapAccelReason, tonemapChoices, tonemapGoesWith } from "./transcodeChoices";

/** Pley on Windows with an NVIDIA card whose NVENC encodes HEVC, but not in 10 bits. */
const nvidia = {
  os: "windows",
  transcoder: {
    available: true,
    managed: true,
    encoders: ["h264_nvenc", "hevc_nvenc", "libx264", "libx265"],
    accels: ["nvenc"],
    hardware_decoding: true,
    tonemapping: ["zscale"],
    with_accel: {
      auto: { hardware_decoding: true, software: ["hdr"] },
      nvenc: { hardware_decoding: true, software: ["hdr"] },
      none: { hardware_decoding: true, software: ["h264", "hevc", "hdr"] },
    },
  },
} as unknown as SystemInfo;

/** Pley on Linux with Intel graphics, where only libplacebo's frames and zimg's reached the VAAPI encoder. */
const intel = {
  os: "linux",
  transcoder: {
    available: true,
    managed: true,
    encoders: ["h264_vaapi", "libx264"],
    accels: ["vaapi"],
    hardware_decoding: true,
    tonemapping: ["libplacebo", "opencl", "zscale"],
    with_accel: {
      auto: { hardware_decoding: true, software: ["hevc", "hdr"], tonemappers: ["libplacebo", "zscale"] },
      vaapi: { hardware_decoding: true, software: ["hevc", "hdr"], tonemappers: ["libplacebo", "zscale"] },
      none: { hardware_decoding: false, software: ["h264", "hevc", "hdr"], tonemappers: null },
    },
  },
} as unknown as SystemInfo;

describe("what goes with the hardware acceleration chosen", () => {
  it("keeps Vulkan's tone mapping on libplacebo, and takes every tone mapper with the others", () => {
    const modes: ToneMapping[] = ["auto", "libplacebo", "opencl", "zscale", "none"];
    expect(modes.filter((mode) => tonemapGoesWith("vulkan", mode))).toEqual(["auto", "libplacebo"]);
    const others: HwAccel[] = ["auto", "none", "nvenc", "qsv", "amf", "vaapi", "videotoolbox", "rkmpp"];
    for (const accel of others) expect(modes.every((mode) => tonemapGoesWith(accel, mode))).toBe(true);
  });

  it("falls back to what goes with it, and says so", () => {
    expect(fitToAccel({ tonemapping: "opencl", hardware_decoding: false }, "vulkan")).toEqual({
      changes: { tonemapping: "auto", hardware_decoding: true },
      notes: {
        tonemapping: "Set to Automatic: Vulkan tone maps with libplacebo only.",
        hardware_decoding: "Turned on: Vulkan decodes on the GPU.",
      },
    });
    expect(fitToAccel({ tonemapping: "libplacebo", hardware_decoding: true }, "vulkan")).toEqual({ changes: {}, notes: {} });
    expect(fitToAccel({ tonemapping: "opencl", hardware_decoding: false }, "nvenc")).toEqual({ changes: {}, notes: {} });
  });

  it("decodes with CUDA on an NVIDIA computer, software encoding too", () => {
    expect(decodingReason("none", nvidia)).toBeNull();
    expect(decodingReason("nvenc", nvidia)).toBeNull();
    // Nobody knows before the transcoder says.
    expect(decodingReason("none", undefined)).toBeNull();
  });

  it("grays out the tone mappers whose frames don't reach VAAPI's encoder", () => {
    expect(tonemapAccelReason("opencl", "vaapi", intel)).toBe("not with VAAPI");
    expect(tonemapAccelReason("libplacebo", "vaapi", intel)).toBeNull();
    expect(tonemapAccelReason("opencl", "none", intel)).toBeNull();
    expect(tonemapChoices(intel, "vaapi").filter((c) => c.reason).map((c) => c.value)).toEqual(["opencl"]);
    expect(fitToAccel({ tonemapping: "opencl", hardware_decoding: true }, "vaapi", intel)).toEqual({
      changes: { tonemapping: "auto" },
      notes: { tonemapping: "Set to Automatic: OpenCL doesn't reach VAAPI here." },
    });
    // A transcoder that didn't say keeps every tone mapper.
    expect(tonemapAccelReason("opencl", "vaapi", nvidia)).toBeNull();
  });

  it("says what the software preset applies to", () => {
    expect(presetNote("nvenc", nvidia)).toBe("NVIDIA NVENC encodes here: this applies to transcodes that keep HDR, and when software encodes instead.");
    expect(presetNote("none", nvidia)).toBeNull();
  });
});

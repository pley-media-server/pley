import type { ClientCaps, VideoDecoderCaps } from "pley-tv/api/types";
import { t } from "pley-tv/i18n";
import type { CapsOverrides } from "pley-tv/platform";
import type { TizenFacts } from "./facts";

/**
 * What a Samsung TV plays, reported to the server (capabilities version 2)
 * so it sends files as they are whenever AVPlay takes them. From Samsung's
 * video specifications per model year (docs/TV_PLATFORMS.md), refined by
 * what the TV says of its panel, its HDR and its sound output. Samsung
 * TVs show no Dolby Vision; HDR10+ they do from 2017. Pure, but for
 * Diagnostics' lines, which are in the language shown.
 */

const CONTAINERS = ["mkv", "mp4", "m4v", "mov", "ts", "m2ts", "mpegts", "webm", "avi", "mpg", "mpeg", "mp3", "m4a", "aac", "flac", "ogg", "oga", "wav", "mka"];

/** Subtitles the app draws itself: text as WebVTT from the server, pictures from their .mks. */
const SUBTITLES = ["subrip", "ass", "ssa", "webvtt", "mov_text", "hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle"];

// ffprobe's levels: H.264 4.2 = 42, 5.1 = 51; HEVC 4.1 = 123, 5.1 = 153, 5.2 = 156, 6.1 = 183.

export interface CapsResult {
  caps: ClientCaps;
  /** Diagnostics' lines, written in the language shown when they are read (a getter), so the page draws them again in a new one. */
  readonly lines: string[];
}

export function tizenCaps(f: TizenFacts, o: CapsOverrides = { truehd: false, dts: false }): CapsResult {
  const y = f.year;
  const video: VideoDecoderCaps[] = [];
  const h264 = ["Constrained Baseline", "Baseline", "Main", "High"];
  if (f.uhd) video.push({ codec: "h264", hardware: true, profiles: h264, max_width: 3840, max_height: 2160, max_fps: 60, max_bit_depth: 8, max_level: 51 });
  video.push({ codec: "h264", hardware: true, profiles: h264, max_width: 1920, max_height: 1080, max_fps: y >= 2021 ? 120 : 60, max_bit_depth: 8, max_level: 42 });
  const hevc = ["Main", "Main 10"];
  if (f.uhd8k && y >= 2019) video.push({ codec: "hevc", hardware: true, profiles: hevc, max_width: 7680, max_height: 4320, max_fps: 60, max_bit_depth: 10, max_level: 183 });
  if (f.uhd) video.push({ codec: "hevc", hardware: true, profiles: hevc, max_width: 4096, max_height: 2160, max_fps: 60, max_bit_depth: 10, max_level: y >= 2025 ? 156 : 153 });
  else video.push({ codec: "hevc", hardware: true, profiles: hevc, max_width: 1920, max_height: 1080, max_fps: 60, max_bit_depth: 10, max_level: 123 });
  const big = f.uhd ? { max_width: 3840, max_height: 2160 } : { max_width: 1920, max_height: 1080 };
  video.push({ codec: "vp9", hardware: true, profiles: y >= 2018 ? ["Profile 0", "Profile 2"] : ["Profile 0"], ...big, max_fps: 60, max_bit_depth: y >= 2018 ? 10 : 8 });
  // AV1 from 2020's chips; Tizen 6.0 and later can say so themselves.
  const av1 = f.av1 ?? y >= 2020;
  if (av1 && y >= 2020) {
    video.push({ codec: "av1", hardware: true, profiles: ["Main"], ...(f.uhd8k ? { max_width: 7680, max_height: 4320 } : big), max_fps: 60, max_bit_depth: 10 });
  }
  video.push({ codec: "mpeg2video", hardware: true, max_width: 1920, max_height: 1080, max_fps: 60, max_bit_depth: 8 });
  video.push({ codec: "mpeg4", hardware: true, max_width: 1920, max_height: 1080, max_fps: 60, max_bit_depth: 8 });

  const hdrTypes: ClientCaps["hdr_types"] = [];
  if (f.hdr) {
    hdrTypes.push("hdr10");
    if (y >= 2017) hdrTypes.push("hdr10plus");
    if (y >= 2018) hdrTypes.push("hlg");
  }

  // Sound: what the TV decodes, what goes to a receiver as it is.
  const decode = ["aac", "ac3", "eac3", "mp3", "mp2", "flac", "vorbis", "pcm_s16le", "pcm_s24le"];
  if (y >= 2018) decode.push("opus");
  if (y >= 2018 && f.ac4 !== false) decode.push("ac4");
  // DTS until 2017: Samsung dropped it from 2018 sets on.
  const dtsDecoded = y <= 2017;
  if (dtsDecoded) decode.push("dts");
  const mode = (f.outputMode ?? "").toUpperCase();
  const passthrough: string[] = [];
  if (mode === "DOLBY_DIGITAL_PLUS") passthrough.push("ac3", "eac3");
  else if (mode === "DOLBY") passthrough.push("ac3");
  else if (mode === "DTS" && dtsDecoded) passthrough.push("dts");
  // Only when asked for: no Samsung app is known to send these to a receiver.
  if (o.truehd) passthrough.push("truehd");
  if (o.dts && !dtsDecoded) passthrough.push("dts");
  const caps: ClientCaps = {
    containers: CONTAINERS,
    video_codecs: unique(video.map((v) => v.codec)),
    audio_codecs: unique(decode.concat(passthrough)),
    hdr: hdrTypes.length > 0,
    dolby_vision: false,
    caps_version: 2,
    video,
    hdr_types: hdrTypes,
    dolby_vision_profiles: [],
    audio_decode: decode,
    audio_passthrough: unique(passthrough),
    audio_channels: passthrough.length ? 8 : 6,
    subtitle_codecs: SUBTITLES,
    audio_track_switching: true,
    picture_subtitles_sideload: true,
    // One size per stream (no `variants`): AVPlay may go black moving between sizes, as between streams (docs/QUALITY.md).
    hls: { master_playlist: true, segments: y >= 2020 ? ["ts", "fmp4"] : ["ts"] },
    display: f.uhd8k ? { width: 7680, height: 4320, refresh_rates: [50, 60] } : f.uhd ? { width: 3840, height: 2160, refresh_rates: [50, 60] } : { width: 1920, height: 1080, refresh_rates: [50, 60] },
    device: { form: "tv", tier: tierOf(f), fire_tv: false, model: f.model },
  };
  return {
    caps,
    get lines() {
      return lines(f, caps);
    },
  };
}

/** Low for 2018 sets, mid until 2021, high after. */
export function tierOf(f: TizenFacts): "low" | "mid" | "high" {
  if (f.year <= 2018) return "low";
  return f.year <= 2021 ? "mid" : "high";
}

function unique(list: string[]): string[] {
  return list.filter((x, i) => list.indexOf(x) === i);
}

function lines(f: TizenFacts, caps: ClientCaps): string[] {
  const hdr = caps.hdr_types.map((h) => (h === "hdr10" ? "HDR10" : h === "hdr10plus" ? "HDR10+" : h === "hlg" ? "HLG" : "Dolby Vision"));
  const sent = caps.audio_passthrough.length ? caps.audio_passthrough.join(", ") : t("settings.diagnostics.caps.nothing");
  return [
    f.firmware ? t("settings.diagnostics.caps.tvFirmware", { model: f.model, firmware: f.firmware }) : t("settings.diagnostics.caps.tv", { model: f.model }),
    `${t("settings.diagnostics.caps.tizen", { version: f.tizen.toFixed(1), year: f.year })}${f.chromium ? ` · Chromium ${f.chromium}` : ""}`,
    t("settings.diagnostics.caps.panel", { panel: f.uhd8k ? "8K" : f.uhd ? "4K" : "HD" }),
    t("settings.diagnostics.caps.hdr", { list: hdr.length ? hdr.join(", ") : t("settings.diagnostics.caps.none") }),
    t("settings.diagnostics.caps.video", { list: caps.video.map((v) => `${v.codec} ${v.max_width}×${v.max_height}@${v.max_fps}`).join(", ") }),
    t("settings.diagnostics.caps.audio", { list: caps.audio_decode.join(", ") }),
    f.outputMode ? t("settings.diagnostics.caps.receiverOutput", { list: sent, output: f.outputMode }) : t("settings.diagnostics.caps.receiver", { list: sent }),
    t("settings.diagnostics.caps.streams", { list: caps.hls.segments.join(", ") }),
  ];
}

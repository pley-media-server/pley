import type { MediaFile, MediaStream, PlaybackInfo, PlaybackMethod, TranscodeInfo } from "../api/types";
import { codecLabel, formatBitrate, resolutionLabel, streamRange } from "../format/format";
import { formatNumber, t } from "../i18n";
import { isPicture } from "./plan";
import type { FallbackReason } from "./recovery";

// What the player says is in the player area's words (i18n/locales/<language>/player.json), as the
// Android app's are in its strings_session.xml; these put together the lines made of parts.

/** The menu's value while Automatic plays: "Automatic · 8 Mbps 1080p". */
export function automaticAt(now: string): string {
  return t("player.quality.automaticAt", { quality: now });
}

/** Automatic's hint: what it plays now, and why that is lower when it is ("Now 4 Mbps 720p · Lowered for your connection"). */
export function automaticNow(now: string, why: string | null): string {
  return why ? t("player.quality.nowBecause", { quality: now, why }) : t("player.quality.now", { quality: now });
}

/** Why Automatic plays lower than the original. */
export function loweredText(why: "network" | "device"): string {
  return why === "network" ? t("player.quality.lowered.network") : t("player.quality.lowered.device");
}

/** Up Next's heading: "Up next · S1 · E2", or "Up next". */
export function upNextHeading(label: string | null): string {
  return label ? t("player.upNext.episode", { episode: label }) : t("player.upNext.title");
}

export function methodText(info: PlaybackInfo): string {
  const remux = t("player.details.method.remux");
  if (info.method === "remux") return info.audio_converted ? `${remux} · ${t("player.details.audioTranscoding")}` : remux;
  if (info.method === "transcode") return t("player.details.method.transcode");
  return t("player.details.method.direct");
}

export function fallbackText(reason: FallbackReason): string {
  return reason === "unplayable" ? t("player.errors.originalFailed") : t("player.errors.originalNotSmooth");
}

/** The server stopped the playback: who did, and their message when given. */
export function stoppedText(message: string | null, byAdmin: boolean): string {
  const who = byAdmin ? t("player.errors.stoppedByAdmin") : t("player.errors.stoppedElsewhere");
  return message && message.trim() ? `${who}\n${message.trim()}` : who;
}

/** How the subtitles chosen reach the screen. */
export type SubtitleDelivery = "server_text" | "server_pictures" | "burned_in";

export function deliveryOf(info: PlaybackInfo, s: MediaStream, sideload: boolean): SubtitleDelivery {
  if (info.burn_subtitles && info.subtitle_index === s.index) return "burned_in";
  if (isPicture(s) && sideload) return "server_pictures";
  return isPicture(s) ? "burned_in" : "server_text";
}

export interface DetailRow {
  label: string;
  value: string;
}

/** How the video plays, while the details are open. */
export interface PlaybackDetails {
  method: string;
  why: string | null;
  pipeline: string | null;
  rows: DetailRow[];
  playbackMethod: PlaybackMethod;
  file: MediaFile | null;
  video: MediaStream | null;
  audio: MediaStream | null;
  subtitle: MediaStream | null;
  subtitleDelivery: SubtitleDelivery | null;
  transcode: TranscodeInfo | null;
}

const ENCODERS: Record<string, string> = {
  libx264: "x264", libx265: "x265", h264_nvenc: "NVIDIA NVENC", hevc_nvenc: "NVIDIA NVENC", h264_qsv: "Intel Quick Sync",
  hevc_qsv: "Intel Quick Sync", h264_amf: "AMD AMF", hevc_amf: "AMD AMF", h264_vaapi: "VA-API", hevc_vaapi: "VA-API",
  h264_videotoolbox: "VideoToolbox", hevc_videotoolbox: "VideoToolbox", h264_vulkan: "Vulkan", hevc_vulkan: "Vulkan", h264_v4l2m2m: "V4L2",
};
/** Where the transcoder works: "GPU", "CPU", both, or "copy" (the video copied). */
function pipelineLabel(pipeline: string): string {
  if (pipeline === "gpu") return "GPU";
  if (pipeline === "cpu") return "CPU";
  if (pipeline === "hybrid") return t("player.details.pipeline.hybrid");
  if (pipeline === "copy") return t("player.details.pipeline.copy");
  return pipeline;
}
const CHANNELS: Record<number, string> = { 1: "1.0", 2: "2.0", 3: "2.1", 6: "5.1", 7: "6.1", 8: "7.1" };

/** A video stream as "HEVC 4K Dolby Vision P8.1", "HEVC 4K Dolby Vision P7 FEL". */
export function videoLabel(v: MediaStream | null): string {
  if (!v) return "";
  return [codecLabel(v.codec), resolutionLabel(v.width, v.height) ?? "", streamRange(v) ?? ""].filter(Boolean).join(" ");
}

/**
 * What the screen gets of HDR video: the live transcode's tone mapping when
 * there is one, else what the server said ("HDR10 (Dolby Vision base
 * layer)"), else `null` (SDR video, or an older server).
 */
export function shownAs(info: PlaybackInfo, live: TranscodeInfo | null): string | null {
  if (live?.tonemap) return t("player.details.toneMapped", { method: live.tonemap });
  return info.video_range ?? null;
}

/** An audio stream as "TrueHD 7.1". */
export function audioLabel(a: MediaStream | null): string {
  if (!a) return "";
  const channels = a.channels ? CHANNELS[a.channels] ?? `${a.channels}ch` : null;
  return [codecLabel(a.codec), channels].filter(Boolean).join(" ");
}

/** The transcoder's "H.264 1920×1080" as "H.264 1080p". */
export function simplified(text: string): string {
  return text.replace(/(\d+)×(\d+)/g, (m, w: string, h: string) => resolutionLabel(Number(w), Number(h)) ?? m);
}

export function encoderLabel(t: TranscodeInfo): string {
  const name = t.encoder ? ENCODERS[t.encoder] ?? t.encoder : null;
  const where = t.pipeline && t.pipeline !== "copy" ? `(${pipelineLabel(t.pipeline)})` : null;
  return [name, where].filter(Boolean).join(" ");
}

/** The details of `info` playing `file` (apps/web/src/player/PlaybackDetails.tsx, the Android app's PlaybackDetails). */
export function buildDetails(opts: {
  info: PlaybackInfo;
  file: MediaFile | null;
  audioIndex: number | null;
  note: string | null;
  positionMs: number;
  live: TranscodeInfo | null;
  subtitle: MediaStream | null;
  sideload: boolean;
}): PlaybackDetails {
  const { info, file } = opts;
  const transcode = info.method === "direct" ? null : opts.live;
  const streams = file?.streams ?? [];
  const video = streams.filter((s) => s.kind === "video")[0] ?? null;
  const audios = streams.filter((s) => s.kind === "audio");
  const want = opts.audioIndex ?? info.audio_index;
  const audio = audios.filter((s) => s.index === want)[0] ?? audios.filter((s) => s.default)[0] ?? audios[0] ?? null;
  // Nothing more specific known: the method says it.
  const why = info.method === "direct" ? opts.note ?? null : opts.note ?? ((info.reasons ?? []).join(" ") || null) ?? info.reason ?? null;
  const pending = "…";
  const videoIn = transcode?.video_in ? simplified(transcode.video_in) : videoLabel(video);
  const audioIn = transcode?.audio_in ?? audioLabel(audio);
  const converting = info.method !== "direct";
  const videoRow =
    converting && transcode?.video !== "copy" ? `${videoIn} → ${transcode?.video_out ? simplified(transcode.video_out) + (transcode.tonemap ? " SDR" : "") : pending}` : videoIn;
  const audioRow = converting && transcode?.audio !== "copy" ? `${audioIn} → ${transcode?.audio_out ?? pending}` : audioIn;
  const bitrate = formatBitrate(file?.bitrate);
  const ahead = transcode ? Math.max(0, Math.floor((transcode.transcoded_ms - opts.positionMs) / 1000)) : 0;
  const rows: DetailRow[] = [];
  if (video) rows.push({ label: t("player.details.video"), value: videoRow });
  if (audio) rows.push({ label: t("player.details.audio"), value: audioRow });
  if (bitrate) rows.push({ label: t("player.details.bitrate"), value: bitrate });
  if (transcode?.video === "transcode" && transcode.encoder) rows.push({ label: t("player.details.encoder"), value: encoderLabel(transcode) });
  const shown = video ? shownAs(info, transcode) : null;
  if (shown) rows.push({ label: t("player.details.shownAs"), value: shown });
  if (transcode?.subtitle_burn) rows.push({ label: t("player.details.subtitles"), value: t("player.details.burnedIn") });
  if (transcode?.speed && transcode.speed > 0) {
    const speed = formatNumber(transcode.speed, { minimumFractionDigits: 1, maximumFractionDigits: 1 });
    rows.push({ label: t("player.details.speed"), value: ahead > 0 ? t("player.details.realTimeReady", { speed, seconds: ahead }) : t("player.details.realTime", { speed }) });
  }
  return {
    method: methodText(info),
    why,
    pipeline: transcode?.pipeline && transcode.video === "transcode" ? pipelineLabel(transcode.pipeline) : null,
    rows,
    playbackMethod: info.method,
    file,
    video,
    audio,
    subtitle: opts.subtitle,
    subtitleDelivery: opts.subtitle ? deliveryOf(info, opts.subtitle, opts.sideload) : null,
    transcode,
  };
}

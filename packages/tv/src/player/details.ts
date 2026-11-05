import type { MediaFile, MediaStream, PlaybackInfo, PlaybackMethod, TranscodeInfo } from "../api/types";
import { codecLabel, formatBitrate, resolutionLabel, streamRange } from "../format/format";
import { isPicture } from "./plan";
import type { FallbackReason } from "./recovery";

/** What the player says, as the Android app's strings_session.xml does. */
export const T = {
  /** The menu's value while Automatic plays: "Automatic · 8 Mbps 1080p". */
  automatic: (now: string) => `Automatic · ${now}`,
  autoHint: "Dynamically adjust to conditions",
  autoNow: (now: string, why: string | null) => (why ? `Now ${now} · ${why}` : `Now ${now}`),
  lowered: { network: "Lowered for your connection", device: "Lowered for this device" },
  smoother: (label: string) => `Quality lowered to ${label}`,

  upNext: (label: string | null) => (label ? `Up next · ${label}` : "Up next"),
  lastEpisode: "This is the last episode",
  lastInQueue: "This is the last one",
  subtitlesOff: "Off",
  subtitlesFailed: "These subtitles couldn't be loaded.",
  subtitlesAdded: (label: string) => `Subtitles added: ${label}`,
  /** What the user's remote, on another device, did, said for a moment: a track or quality as its menu names it, `null` subtitles off. */
  remote: {
    paused: "Paused",
    playing: "Playing",
    jumpedTo: (at: string) => `Jumped to ${at}`,
    audio: (label: string) => `Audio: ${label}`,
    subtitles: (label: string | null) => (label === null ? "Subtitles off" : `Subtitles: ${label}`),
    quality: (label: string) => `Quality: ${label}`,
  },
  slowServer: "The server can't keep up with this video. Try a lower quality.",
  noFile: "This item has no file to play.",
  burnedIn: "Burned in",
  reconnecting: "Reconnecting…",
  offlineTitle: "Can't reach your server",
  offlineBody: "Check that the server is on and that this device is on its network.",
} as const;

export function methodText(info: PlaybackInfo): string {
  if (info.method === "remux") return info.audio_converted ? "Direct stream · Audio transcoding" : "Direct stream";
  if (info.method === "transcode") return "Transcoding";
  return "Direct play";
}

export function fallbackText(reason: FallbackReason): string {
  return reason === "unplayable" ? "This device couldn't play the original file." : "The original didn't play smoothly on this device.";
}

/** The server stopped the playback: who did, and their message when given. */
export function stoppedText(message: string | null, byAdmin: boolean): string {
  const who = byAdmin ? "Playback was stopped by the server's administrator." : "Playback was stopped from another device.";
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
const PIPELINE: Record<string, string> = { gpu: "GPU", hybrid: "GPU and CPU", cpu: "CPU", copy: "copy" };
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
  if (live?.tonemap) return `SDR (tone-mapped with ${live.tonemap})`;
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
  const where = t.pipeline && t.pipeline !== "copy" ? `(${PIPELINE[t.pipeline] ?? t.pipeline})` : null;
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
  if (video) rows.push({ label: "Video", value: videoRow });
  if (audio) rows.push({ label: "Audio", value: audioRow });
  if (bitrate) rows.push({ label: "Bitrate", value: bitrate });
  if (transcode?.video === "transcode" && transcode.encoder) rows.push({ label: "Encoder", value: encoderLabel(transcode) });
  const shown = video ? shownAs(info, transcode) : null;
  if (shown) rows.push({ label: "Shown as", value: shown });
  if (transcode?.subtitle_burn) rows.push({ label: "Subtitles", value: T.burnedIn });
  if (transcode?.speed && transcode.speed > 0) {
    const speed = transcode.speed.toFixed(1);
    rows.push({ label: "Speed", value: ahead > 0 ? `${speed}× real time · ${ahead} s ready` : `${speed}× real time` });
  }
  return {
    method: methodText(info),
    why,
    pipeline: transcode?.pipeline && transcode.video === "transcode" ? PIPELINE[transcode.pipeline] ?? transcode.pipeline : null,
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

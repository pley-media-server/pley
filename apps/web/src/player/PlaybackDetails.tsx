import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { usePlaybackStatus } from "@/api/hooks";
import type { MediaFile, MediaStream, PlaybackInfo, PlaybackMethod, TranscodeInfo, TranscodePipeline } from "@/api/types";
import { currentLanguage, t as translate } from "@/i18n";
import { codecLabel, dynamicRangeLabel, formatBitrate, resolutionLabel } from "@/lib/format";

/** What each way of playing is called (as in Plex): "Direct play", "Direct stream", "Transcoding". */
function methodName(method: PlaybackMethod): string {
  return translate(`details.method.${method}`, { ns: "player" });
}

/** How a session plays, in a few words: a direct stream says when its audio is converted. */
export function methodLabel(info: PlaybackInfo): string {
  return info.method === "remux" && info.audio_converted
    ? `${methodName("remux")} · ${translate("details.audioTranscoding", { ns: "player" })}`
    : methodName(info.method);
}

const ENCODERS: Record<string, string> = {
  libx264: "x264",
  libx265: "x265",
  h264_nvenc: "NVIDIA NVENC",
  hevc_nvenc: "NVIDIA NVENC",
  h264_qsv: "Intel Quick Sync",
  hevc_qsv: "Intel Quick Sync",
  h264_amf: "AMD AMF",
  hevc_amf: "AMD AMF",
  h264_vaapi: "VA-API",
  hevc_vaapi: "VA-API",
  h264_videotoolbox: "VideoToolbox",
  hevc_videotoolbox: "VideoToolbox",
  h264_vulkan: "Vulkan",
  hevc_vulkan: "Vulkan",
  h264_v4l2m2m: "V4L2",
};

/** Where the transcoder works: "GPU", "GPU and CPU", "CPU". */
function pipelineName(pipeline: TranscodePipeline): string {
  return translate(`details.pipeline.${pipeline}`, { ns: "player" });
}

/** A video stream as `"HEVC 4K Dolby Vision P8.1"`, `"HEVC 4K Dolby Vision P7 FEL"`. */
export function videoLabel(v: MediaStream | undefined): string {
  if (!v) return "";
  return [codecLabel(v.codec), resolutionLabel(v.width, v.height), dynamicRangeLabel(v)].filter(Boolean).join(" ");
}

/**
 * What the screen gets of HDR video: the live transcode's tone mapping when
 * there is one, else what the server said ("HDR10 (Dolby Vision base
 * layer)"), else nothing (SDR video, or an older server).
 */
export function shownAsLabel(info: PlaybackInfo, live: TranscodeInfo | null): string | null {
  if (live?.tonemap) return translate("details.toneMapped", { ns: "player", method: live.tonemap });
  return info.video_range ?? null;
}

/** An audio stream as `"TrueHD 7.1"`. */
export function audioLabel(a: MediaStream | undefined): string {
  if (!a) return "";
  const channels = a.channels ? ({ 1: "1.0", 2: "2.0", 3: "2.1", 6: "5.1", 7: "6.1", 8: "7.1" } as Record<number, string>)[a.channels] : null;
  return [codecLabel(a.codec), channels ?? (a.channels ? `${a.channels}ch` : null)].filter(Boolean).join(" ");
}

/** The transcoder's `"H.264 1920×1080"` as `"H.264 1080p"`. */
export function simplified(text: string): string {
  return text.replace(/(\d+)×(\d+)/, (whole, w: string, h: string) => resolutionLabel(Number(w), Number(h)) ?? whole);
}

/** The encoder, and where it runs: `"NVIDIA NVENC (GPU)"`, `"x264 veryfast (CPU)"`. */
export function encoderLabel(t: TranscodeInfo): string {
  const name = t.encoder ? (ENCODERS[t.encoder] ?? t.encoder) : null;
  const where = t.pipeline && t.pipeline !== "copy" ? `(${pipelineName(t.pipeline)})` : null;
  return [name, t.preset, where].filter(Boolean).join(" ");
}

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <dt className="text-white/45">{label}</dt>
      <dd className="min-w-0 text-white/85">{children}</dd>
    </>
  );
}

/**
 * How this video plays, in the menu's Playback details and the badge's
 * tooltip: the way (direct play, direct stream, transcoding), the server's
 * reasons, then what goes in and what comes out. A transcode's numbers come
 * live while it shows.
 */
export function PlaybackDetails({
  info,
  file,
  audioIndex,
  note,
  positionMs,
  open,
}: {
  info: PlaybackInfo | null;
  file: MediaFile | null;
  /** The audio track playing. */
  audioIndex: number | null;
  /** Why it plays this way, from the player (a fallback after the original didn't play). */
  note: string | null;
  positionMs: number;
  open: boolean;
}) {
  const { t } = useTranslation("player");
  const status = usePlaybackStatus(info?.session_id ?? null, open && !!info && info.method !== "direct");
  if (!info) return null;
  const live = info.method === "direct" ? null : (status.data?.transcode ?? null);
  const streams = file?.streams ?? [];
  const video = streams.find((s) => s.kind === "video");
  const audios = streams.filter((s) => s.kind === "audio");
  const audio =
    audios.find((s) => s.index === (audioIndex ?? info.audio_index)) ?? audios.find((s) => s.default) ?? audios[0];
  // Nothing more specific known: the method says it.
  const why = info.method === "direct" ? note : (note ?? info.reasons?.join(" ") ?? info.reason ?? null);
  const pending = "…";

  const videoIn = live?.video_in ? simplified(live.video_in) : videoLabel(video);
  const audioIn = live?.audio_in ?? audioLabel(audio);
  const converting = info.method !== "direct";
  const videoRow =
    converting && live?.video !== "copy"
      ? `${videoIn} → ${live?.video_out ? `${simplified(live.video_out)}${live.tonemap ? " SDR" : ""}` : pending}`
      : videoIn;
  const audioRow =
    converting && live?.audio !== "copy" ? `${audioIn} → ${live?.audio_out ?? pending}` : audioIn;
  const bitrate = formatBitrate(file?.bitrate);
  const bitrateRow = live?.bitrate_out && bitrate ? `${bitrate} → ${formatBitrate(live.bitrate_out)}` : bitrate;
  const ahead = live ? Math.max(0, Math.round((live.transcoded_ms - positionMs) / 1000)) : 0;
  const shownAs = video ? shownAsLabel(info, live) : null;

  return (
    <div className="px-4 pb-3 pt-1">
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-semibold text-accent">{methodLabel(info)}</span>
        {live?.pipeline && live.video === "transcode" ? (
          <span className="rounded bg-white/10 px-1.5 py-0.5 text-[10px] font-semibold tracking-wide text-white/75">
            {pipelineName(live.pipeline)}
          </span>
        ) : null}
      </div>
      {why ? <p className="mt-0.5 text-xs leading-snug text-white/65">{why}</p> : null}
      <dl className="mt-2.5 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs tabular-nums">
        {video ? <Row label={t("details.video")}>{videoRow}</Row> : null}
        {audio ? <Row label={t("details.audio")}>{audioRow}</Row> : null}
        {bitrateRow ? <Row label={t("details.bitrate")}>{bitrateRow}</Row> : null}
        {live?.video === "transcode" && live.encoder ? <Row label={t("details.encoder")}>{encoderLabel(live)}</Row> : null}
        {shownAs ? <Row label={t("details.shownAs")}>{shownAs}</Row> : null}
        {live?.subtitle_burn ? <Row label={t("details.subtitles")}>{t("details.burnedIn")}</Row> : null}
        {live?.speed ? (
          <Row label={t("details.speed")}>
            {t("details.realTime", {
              speed: live.speed.toLocaleString(currentLanguage(), { minimumFractionDigits: 1, maximumFractionDigits: 1 }),
            })}
            {ahead > 0 ? ` · ${t("details.ready", { seconds: ahead })}` : ""}
          </Row>
        ) : null}
      </dl>
    </div>
  );
}

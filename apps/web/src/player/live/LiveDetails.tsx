import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { asIs, notAsSent, useSessionPlan, type LivePlan, type SessionPlan } from "@/api/livetv";
import type { PlaybackInfo } from "@/api/types";
import { currentLocale, t as translate } from "@/i18n";
import { streamCaps, type StreamCaps } from "@/lib/caps";
import { codecLabel, formatBitrate, resolutionLabel } from "@/lib/format";
import { encoderName, methodLabel, Row, soundLabel } from "../PlaybackDetails";

/**
 * How the player plays a channel: from its stream (mpegts.js) or from the
 * transcoder's segments; `reads`: this browser can read a stream at all.
 */
export interface LivePlayback {
  direct: boolean;
  reads: boolean;
}

/**
 * How a channel plays, by the names a file's ways have: from its stream,
 * direct play; from the transcoder's segments, a direct stream while its
 * video is copied, else a transcode. Null while the server hasn't said
 * what the segments are made of.
 */
export function liveMethod(direct: boolean, plan: LivePlan | null): Pick<PlaybackInfo, "method" | "audio_converted"> | null {
  if (direct) return { method: "direct" };
  if (!plan) return null;
  return asIs(plan.video) ? { method: "remux", audio_converted: plan.audio === "aac" } : { method: "transcode" };
}

/** The provider's video codec with its bit depth above 8: `"HEVC 10-bit"`, `"H.264"`. */
function videoCodec(plan: LivePlan): string {
  const depth = plan.bit_depth && plan.bit_depth > 8 ? translate("format.bitDepth", { n: plan.bit_depth }) : null;
  return [codecLabel(plan.video_codec), depth].filter(Boolean).join(" ");
}

/** Why a channel isn't played from its stream, in a sentence; null when nothing says. */
export function liveWhy(plan: SessionPlan, caps: StreamCaps): string | null {
  const not = notAsSent(plan, caps);
  switch (not?.why) {
    case "interlaced":
      return translate("player.details.interlacedWhy", { ns: "livetv" });
    case "video":
      // With its bit depth: a browser that plays 8-bit HEVC isn't told it doesn't play HEVC.
      return not.codec ? translate("player.details.noVideo", { ns: "livetv", codec: videoCodec(plan) }) : null;
    case "audio":
      return not.codec ? translate("player.details.noAudio", { ns: "livetv", codec: codecLabel(not.codec) }) : null;
    default:
      return null;
  }
}

/**
 * Why this player doesn't play the channel from its stream, for the details
 * and the badge's name. Only where a stream could have been read: elsewhere
 * every channel comes from the transcoder.
 */
export function playbackWhy(plan: SessionPlan | null, playback: LivePlayback | null, caps: StreamCaps): string | null {
  return plan && playback?.reads && !playback.direct ? liveWhy(plan, caps) : null;
}

/**
 * The provider's video as `"HEVC 10-bit · 3840×2160 · 50 fps"`, and what
 * the transcoder makes of it when it isn't played from its stream:
 * `" → H.264 1080p"`, `" → H.264 4K"`.
 */
export function liveVideo(plan: LivePlan, direct: boolean): string | null {
  if (plan.video === "none") return null;
  const rate = plan.frame_rate ? (Math.round(plan.frame_rate * 100) / 100).toLocaleString(currentLocale()) : null;
  const source = [
    videoCodec(plan),
    plan.width && plan.height ? `${plan.width}×${plan.height}` : null,
    rate ? translate("mediaInfo.fps", { ns: "item", value: rate }) : null,
    plan.interlaced ? translate("player.details.interlaced", { ns: "livetv" }) : null,
  ]
    .filter(Boolean)
    .join(" · ");
  if (!source) return null;
  if (direct || asIs(plan.video)) return source;
  // The transcoder makes every channel it converts H.264 (docs/API.md, "The stream").
  // The server says its lines alone: they name its size as a video's details name one.
  const size = resolutionLabel(null, plan.out_height ?? plan.height);
  return `${source} → ${[codecLabel("h264"), size].filter(Boolean).join(" ")}`;
}

/** The provider's sound as `"AC3 5.1"`, and `" → AAC 2.0"` when the transcoder's segments have it converted, as a file's details write sound. */
export function liveAudio(plan: LivePlan, direct: boolean): string | null {
  if (plan.audio === "none") return null;
  const source = soundLabel(plan.audio_codec, plan.audio_channels);
  if (!source) return null;
  return direct || plan.audio !== "aac" ? source : `${source} → ${soundLabel("aac", 2)}`;
}

/** What the provider sends now as `"12 Mbps"`, and a transcode's video after it: `"12 Mbps → 8.0 Mbps"`. */
export function liveBitrate(plan: SessionPlan, direct: boolean): string | null {
  const sent = formatBitrate((plan.kbps ?? 0) * 1000);
  if (!sent) return null;
  const made = direct || asIs(plan.video) ? "" : formatBitrate((plan.video_kbps ?? 0) * 1000);
  return made ? `${sent} → ${made}` : sent;
}

/** The encoder of a channel played transcoded: `"NVIDIA NVENC"`. */
export function liveEncoder(plan: LivePlan, direct: boolean): string | null {
  return direct || asIs(plan.video) ? null : encoderName(plan.video);
}

/**
 * How this channel plays, in the player's Playback details, as a file's
 * are: the way (direct play from its stream; a direct stream or a transcode
 * from the transcoder's segments), why it isn't direct play, then what the
 * provider sends and what is made of it. The server is asked again every
 * few seconds while it shows, for the bitrate.
 */
export function LiveDetails({
  sessionId,
  playback,
  open,
}: {
  /** The watch session; null while the channel tunes. */
  sessionId: string | null;
  playback: LivePlayback | null;
  open: boolean;
}) {
  const { t } = useTranslation("player");
  const plan = useSessionPlan(sessionId, open).data ?? null;
  const caps = useMemo(() => streamCaps(), []);
  const direct = playback?.direct ?? false;
  const method = playback ? liveMethod(direct, plan) : null;
  const why = playbackWhy(plan, playback, caps);
  const video = plan ? liveVideo(plan, direct) : null;
  const audio = plan ? liveAudio(plan, direct) : null;
  const bitrate = plan ? liveBitrate(plan, direct) : null;
  const encoder = plan ? liveEncoder(plan, direct) : null;
  return (
    <div className="px-4 pb-3 pt-1">
      {method ? <span className="text-sm font-semibold text-accent">{methodLabel(method)}</span> : null}
      {why ? <p className="mt-0.5 text-xs leading-snug text-white/65">{why}</p> : null}
      {video || audio || bitrate || encoder ? (
        <dl className="mt-2.5 grid grid-cols-[auto_1fr] gap-x-3 gap-y-1 text-xs tabular-nums">
          {video ? <Row label={t("details.video")}>{video}</Row> : null}
          {audio ? <Row label={t("details.audio")}>{audio}</Row> : null}
          {bitrate ? <Row label={t("details.bitrate")}>{bitrate}</Row> : null}
          {encoder ? <Row label={t("details.encoder")}>{encoder}</Row> : null}
        </dl>
      ) : null}
    </div>
  );
}

import type { PlaybackInfo, SessionPlan } from "../api/types";
import { codecLabel, formatBitrate, resolutionLabel } from "../format/format";
import { frameRateLabel, type FactBlock } from "../format/media-info";
import { t } from "../i18n";
import { encoderName, methodText, soundLabel } from "../player/details";

/** The server converts the channel's video: it is neither copied nor absent. */
export function liveTranscodes(plan: SessionPlan): boolean {
  return plan.video !== "copy" && plan.video !== "none";
}

/** What an encoder makes, as people know it: "h264_nvenc" and "libx264" → "H.264". */
function encodedLabel(encoder: string): string {
  return codecLabel(/hevc|265/.test(encoder) ? "hevc" : "h264");
}

/**
 * The provider's video as "HEVC 10-bit · 3840×2160 · 50 fps" (its bit depth
 * only above 8, as a title's tracks say it), and what a transcode makes of
 * it: " → H.264 1080p", " → H.264 4K".
 */
function videoRow(plan: SessionPlan): string {
  const codec = [codecLabel(plan.video_codec), plan.bit_depth && plan.bit_depth > 8 ? t("format.track.bitDepth", { n: plan.bit_depth }) : ""].filter(Boolean).join(" ");
  const size = plan.width && plan.height ? `${plan.width}×${plan.height}` : "";
  const sent = [codec, size, frameRateLabel(plan.frame_rate, plan.interlaced) ?? ""].filter(Boolean).join(" · ");
  if (!sent || !liveTranscodes(plan)) return sent;
  // A transcode that keeps the size says no lines of its own: the provider's. The
  // server says no width, so the lines name the size, as a title's Playback info names one.
  const made = resolutionLabel(null, plan.out_height ?? plan.height) ?? "";
  return `${sent} → ${[encodedLabel(plan.video), made].filter(Boolean).join(" ")}`;
}

/** The provider's sound as "EAC3 5.1", and " → AAC 2.0" when the server converts it, as a title's Playback info writes sound. */
function audioRow(plan: SessionPlan): string {
  const sent = soundLabel(plan.audio_codec, plan.audio_channels);
  return sent && plan.audio === "aac" ? `${sent} → ${soundLabel("aac", 2)}` : sent;
}

/** What the provider sends now as "12 Mbps", and a transcode's video after it: "12 Mbps → 8.0 Mbps". */
function bitrateRow(plan: SessionPlan): string {
  const sent = formatBitrate(plan.kbps * 1000);
  const made = liveTranscodes(plan) && plan.video_kbps ? formatBitrate(plan.video_kbps * 1000) : "";
  return sent && made ? `${sent} → ${made}` : sent;
}

/**
 * The way a live channel plays, by the names a title's ways have, for the
 * player's badge and its Playback info. This TV plays Live TV through the
 * server's HLS alone, so a channel is a direct stream (its video as the
 * provider sends it, its sound converted or not) or a transcode, never the
 * provider's own stream.
 */
export function liveMethod(plan: SessionPlan): Pick<PlaybackInfo, "method" | "audio_converted"> {
  return liveTranscodes(plan) ? { method: "transcode" } : { method: "remux", audio_converted: plan.audio === "aac" };
}

/**
 * How a live channel plays, for the player's Playback info: the readout's
 * "How it plays", in a title's words: the way ("· Audio transcoding" after
 * a direct stream when the server converts the sound, as a film says), then
 * what the provider sends and what the server makes of it. Nothing here
 * could say why: the provider's own stream is never played. A row whose
 * value isn't known is left out.
 */
export function liveFacts(plan: SessionPlan): FactBlock {
  const transcodes = liveTranscodes(plan);
  const facts = [{ label: t("player.info.playback"), value: methodText(liveMethod(plan)) }];
  const add = (label: string, value: string) => {
    if (value) facts.push({ label, value });
  };
  if (plan.video !== "none") add(t("player.details.video"), videoRow(plan));
  if (plan.audio !== "none") add(t("player.details.audio"), audioRow(plan));
  add(t("player.details.bitrate"), bitrateRow(plan));
  if (transcodes) add(t("player.details.encoder"), encoderName(plan.video));
  return { title: t("player.info.howItPlays"), facts };
}

import type { SessionPlan } from "../api/types";
import { channelsLabel, codecLabel, formatBitrate } from "../format/format";
import { frameRateLabel, type FactBlock } from "../format/media-info";
import { t } from "../i18n";
import { encoderName } from "../player/details";

/** The server converts the channel's video: it is neither copied nor absent. */
export function liveTranscodes(plan: SessionPlan): boolean {
  return plan.video !== "copy" && plan.video !== "none";
}

/** What an encoder makes, as people know it: "h264_nvenc" and "libx264" → "H.264". */
function encodedLabel(encoder: string): string {
  return codecLabel(/hevc|265/.test(encoder) ? "hevc" : "h264");
}

/** The provider's video as "HEVC 10-bit · 3840×2160 · 50 fps", and what a transcode makes of it: " → H.264 1080p". */
function videoRow(plan: SessionPlan): string {
  const codec = [codecLabel(plan.video_codec), plan.bit_depth ? t("format.track.bitDepth", { n: plan.bit_depth }) : ""].filter(Boolean).join(" ");
  const size = plan.width && plan.height ? `${plan.width}×${plan.height}` : "";
  const sent = [codec, size, frameRateLabel(plan.frame_rate, plan.interlaced) ?? ""].filter(Boolean).join(" · ");
  if (!sent || !liveTranscodes(plan)) return sent;
  // A transcode that keeps the size says no lines of its own: the provider's.
  const lines = plan.out_height ?? plan.height;
  return `${sent} → ${[encodedLabel(plan.video), lines ? `${lines}p` : ""].filter(Boolean).join(" ")}`;
}

/** The provider's sound as "EAC3 5.1", and " → AAC Stereo" when the server converts it. */
function audioRow(plan: SessionPlan): string {
  const sent = [codecLabel(plan.audio_codec), channelsLabel(plan.audio_channels)].filter(Boolean).join(" ");
  return sent && plan.audio === "aac" ? `${sent} → ${codecLabel("aac")} ${channelsLabel(2)}` : sent;
}

/** What the provider sends now as "12 Mbps", and a transcode's video after it: "12 Mbps → 8.0 Mbps". */
function bitrateRow(plan: SessionPlan): string {
  const sent = formatBitrate(plan.kbps * 1000);
  const made = liveTranscodes(plan) && plan.video_kbps ? formatBitrate(plan.video_kbps * 1000) : "";
  return sent && made ? `${sent} → ${made}` : sent;
}

/**
 * How a live channel plays, for the player's Playback info: the readout's
 * "How it plays", in a title's words. This TV plays Live TV through the
 * server's HLS alone, so a channel is a direct stream (its video as the
 * provider sends it) or a transcode, never the provider's own stream, and
 * nothing here could say why. A row whose value isn't known is left out.
 */
export function liveFacts(plan: SessionPlan): FactBlock {
  const transcodes = liveTranscodes(plan);
  const facts = [{ label: t("player.info.playback"), value: transcodes ? t("player.details.method.transcode") : t("player.details.method.remux") }];
  const add = (label: string, value: string) => {
    if (value) facts.push({ label, value });
  };
  if (plan.video !== "none") add(t("player.details.video"), videoRow(plan));
  if (plan.audio !== "none") add(t("player.details.audio"), audioRow(plan));
  add(t("player.details.bitrate"), bitrateRow(plan));
  if (transcodes) add(t("player.details.encoder"), encoderName(plan.video));
  return { title: t("player.info.howItPlays"), facts };
}

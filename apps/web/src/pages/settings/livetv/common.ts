import type { LivePlan, XtreamAccount } from "@/api/livetv";
import { currentLocale, t } from "@/i18n";

/** "Active · expires 12 Mar 2027 · 2 connections": the status as the provider writes it. */
export function accountLine(a: XtreamAccount): string {
  const parts = [a.trial ? t("settings.account.trial", { ns: "livetv", status: a.status }) : a.status];
  if (a.expires_at) {
    const date = new Date(a.expires_at * 1000).toLocaleDateString(currentLocale(), { day: "numeric", month: "short", year: "numeric" });
    parts.push(t(a.expires_at * 1000 < Date.now() ? "settings.account.expired" : "settings.account.expires", { ns: "livetv", date }));
  } else parts.push(t("settings.account.noExpiry", { ns: "livetv" }));
  if (a.max_connections != null) parts.push(t("settings.account.connections", { ns: "livetv", count: a.max_connections }));
  return parts.join(" · ");
}

/** Days left before an account runs out, when that's soon (two weeks). */
export function expiresSoon(a: XtreamAccount | null): number | null {
  if (!a?.expires_at) return null;
  const days = Math.ceil((a.expires_at * 1000 - Date.now()) / 86_400_000);
  return days <= 14 ? days : null;
}

const CODECS: Record<string, string> = {
  h264: "H.264",
  hevc: "HEVC",
  mpeg2video: "MPEG-2",
  aac: "AAC",
  ac3: "Dolby Digital",
  eac3: "Dolby Digital Plus",
  mp2: "MP2",
  mp3: "MP3",
};

/** The hardware each encoder runs on; libx264, the processor, is named in the language shown. */
const ENCODERS: Record<string, string> = {
  h264_nvenc: "NVIDIA",
  h264_qsv: "Intel Quick Sync",
  h264_amf: "AMD",
  h264_videotoolbox: "VideoToolbox",
};

/** What the transcoder makes of a channel: "1080i MPEG-2 → H.264 on NVIDIA · MP2 → AAC". */
export function planLine(p: LivePlan): string {
  const lines = p.height ? `${p.height}${p.interlaced ? "i" : "p"}` : "";
  const source = [lines, CODECS[p.video_codec ?? ""] ?? p.video_codec].filter(Boolean).join(" ");
  const encoder = p.video === "libx264" ? t("settings.plan.processor", { ns: "livetv" }) : (ENCODERS[p.video] ?? p.video);
  const video =
    p.video === "none"
      ? t("settings.plan.radio", { ns: "livetv" })
      : p.video === "copy"
        ? t("settings.plan.asItIs", { ns: "livetv", source })
        : t(p.interlaced ? "settings.plan.transcodedDeinterlaced" : "settings.plan.transcoded", { ns: "livetv", source, encoder });
  const audioIn = CODECS[p.audio_codec ?? ""] ?? p.audio_codec ?? "";
  const audio =
    p.audio === "none" ? "" : p.audio === "copy" ? t("settings.plan.asItIs", { ns: "livetv", source: audioIn }) : `${audioIn} → AAC`;
  return [video, audio].filter(Boolean).join(" · ");
}

/** "4.2 Mbit/s". */
export function formatKbps(kbps: number): string {
  if (kbps >= 1000) {
    const value = (kbps / 1000).toLocaleString(currentLocale(), { minimumFractionDigits: 1, maximumFractionDigits: 1, useGrouping: false });
    return t("settings.rate.mbps", { ns: "livetv", value });
  }
  return t("settings.rate.kbps", { ns: "livetv", value: kbps });
}

/** "for 12 min", since `since`. */
export function formatFor(since: number): string {
  const minutes = Math.max(0, Math.floor((Date.now() / 1000 - since) / 60));
  if (minutes < 1) return t("settings.since.justNow", { ns: "livetv" });
  if (minutes < 60) return t("settings.since.minutes", { ns: "livetv", minutes });
  const h = Math.floor(minutes / 60);
  return t("settings.since.hoursMinutes", { ns: "livetv", hours: h, minutes: minutes % 60 });
}

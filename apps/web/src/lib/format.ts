import type { DynamicRange, ItemSummary, MediaFile, MediaStream, MediaSummary, RangeFilter, Resolution, TranscodeInfo } from "@/api/types";
import { currentLocale, t } from "@/i18n";
import { languageName } from "./languages";

/** `make`'s value for the locale shown (see `currentLocale`), made once for each. */
function perLocale<T>(make: (locale: string) => T): () => T {
  const made = new Map<string, T>();
  return () => {
    const locale = currentLocale();
    if (!made.has(locale)) made.set(locale, make(locale));
    return made.get(locale) as T;
  };
}

const decimalSeparator = perLocale(
  (locale) => new Intl.NumberFormat(locale).formatToParts(1.5).find((part) => part.type === "decimal")?.value ?? ".",
);

/** `value.toFixed(digits)` with the language's decimal separator: "1.5", "1,5" in French. */
function fixed(value: number, digits: number): string {
  const text = value.toFixed(digits);
  return digits > 0 ? text.replace(".", decimalSeparator()) : text;
}

/** 6720000 → "1h 52m", 2700000 → "45m", 29000 → "29s" (rounded to whole minutes from 30s up). */
export function formatDuration(ms: number | null | undefined): string {
  if (!ms || ms < 0) return "";
  const totalMin = Math.round(ms / 60000);
  if (totalMin < 1) return t("format.seconds", { n: Math.max(1, Math.round(ms / 1000)) });
  const h = Math.floor(totalMin / 60);
  const m = totalMin % 60;
  if (h === 0) return t("format.minutes", { n: m });
  return m ? t("format.hoursMinutes", { hours: h, minutes: m }) : t("format.hours", { n: h });
}

/** Seconds → "1:02:03" or "2:03". */
export function formatClock(seconds: number | null | undefined): string {
  if (seconds == null || !Number.isFinite(seconds) || seconds < 0) return "0:00";
  const s = Math.floor(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const mm = h > 0 ? String(m).padStart(2, "0") : String(m);
  return `${h > 0 ? `${h}:` : ""}${mm}:${String(sec).padStart(2, "0")}`;
}

/** Remaining time label like "34m left" ("" once nothing is left). */
export function formatRemaining(positionMs: number, durationMs: number | null | undefined): string {
  if (!durationMs) return "";
  const left = formatDuration(Math.max(0, durationMs - positionMs));
  return left ? t("format.left", { time: left }) : "";
}

/** About how long is left, said as people would: in minutes up to an hour and a half, then in hours. */
export function formatTimeLeft(seconds: number): string {
  if (seconds < 60) return t("format.timeLeft.underAMinute");
  const minutes = Math.ceil(seconds / 60);
  if (minutes < 90) return t("format.timeLeft.minutes", { count: minutes });
  return t("format.timeLeft.hours", { count: Math.round(minutes / 60) });
}

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

export function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null || !Number.isFinite(bytes)) return "";
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < BYTE_UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  return `${fixed(v, v >= 100 || i === 0 ? 0 : 1)} ${t(`format.bytes.${BYTE_UNITS[i] ?? "B"}`)}`;
}

/**
 * What sets a title's files apart, for its poster: `["4K", "DV"]`. Only 4K
 * of the resolutions, and no codec: "1080p" or "H.264" on most posters says
 * nothing worth the room. HDR10 and HDR10+ read as plain "HDR" and "HDR+"
 * there and in the filters; a title's page names them exactly.
 */
export function mediaBadges(media: MediaSummary | null | undefined): string[] {
  if (!media) return [];
  const range: Partial<Record<DynamicRange, string>> = { dolby_vision: "DV", hdr10plus: "HDR+", hdr10: "HDR", hlg: "HLG" };
  return [
    resolutionLabel(media.width, media.height) === "4K" ? "4K" : null,
    media.dynamic_range ? (range[media.dynamic_range] ?? null) : null,
  ].filter((b): b is string => !!b);
}

/** What the library filters by resolution call each one. */
export const RESOLUTION_LABELS: Record<Resolution, string> = { "4k": "4K", "1080": "1080p", "720": "720p", sd: "SD" };

/** What the library filters by dynamic range call each one. */
export const RANGE_FILTER_LABELS: Record<RangeFilter, string> = {
  // The one in words, read in the language shown.
  get hdr() {
    return t("format.anyHdr");
  },
  dolby_vision: "Dolby Vision",
  hdr10plus: "HDR+",
  hdr10: "HDR",
  hlg: "HLG",
  sdr: "SDR",
};

/** `value` if it's a resolution a library filters by (an address may hold anything). */
export function resolutionOf(value: string | null | undefined): Resolution | undefined {
  return value && Object.hasOwn(RESOLUTION_LABELS, value) ? (value as Resolution) : undefined;
}

/** `value` if it's a dynamic range a library filters by. */
export function rangeFilterOf(value: string | null | undefined): RangeFilter | undefined {
  return value && Object.hasOwn(RANGE_FILTER_LABELS, value) ? (value as RangeFilter) : undefined;
}

/** The line under a title's poster: `"42 Mbps · 23.4 GB"`. */
export function mediaLine(media: MediaSummary | null | undefined): string {
  if (!media) return "";
  return [formatBitrate(media.bitrate), formatBytes(media.size)].filter(Boolean).join(" · ") || t("format.bitrateUnknown");
}

/** Bits per second → "8.2 Mbps" / "320 kbps". */
export function formatBitrate(bps: number | null | undefined): string {
  if (!bps) return "";
  if (bps >= 1_000_000) return t("format.mbps", { n: fixed(bps / 1_000_000, bps >= 10_000_000 ? 0 : 1) });
  return t("format.kbps", { n: Math.round(bps / 1000) });
}

const relativeTime = perLocale((locale) =>
  typeof Intl !== "undefined" && "RelativeTimeFormat" in Intl ? new Intl.RelativeTimeFormat(locale, { numeric: "auto" }) : null,
);

/**
 * Unix seconds of something that already happened → "3 minutes ago". A
 * time the server gave that this device's clock, a little behind, puts in
 * the future reads as now, never "in 2 seconds".
 */
export function formatAgo(ts: number | null | undefined): string {
  if (!ts) return t("format.never");
  return formatRelative(Math.min(ts, Date.now() / 1000));
}

/** Unix seconds → "3 minutes ago", or "in 3 days" for a time to come. */
export function formatRelative(ts: number | null | undefined): string {
  if (!ts) return t("format.never");
  const rtf = relativeTime();
  const diff = ts - Date.now() / 1000;
  const abs = Math.abs(diff);
  // [below this many seconds, unit, seconds per unit]
  const table: [number, Intl.RelativeTimeFormatUnit, number][] = [
    [60, "second", 1],
    [3600, "minute", 60],
    [86400, "hour", 3600],
    [86400 * 7, "day", 86400],
    [86400 * 30, "week", 86400 * 7],
    [86400 * 365, "month", 86400 * 30],
    [Infinity, "year", 86400 * 365],
  ];
  for (const [limit, unit, seconds] of table) {
    if (abs < limit) {
      const value = Math.round(diff / seconds);
      return rtf ? rtf.format(value, unit) : `${Math.abs(value)} ${unit}s ago`;
    }
  }
  return "";
}

/** "2023-04-01" → localized date. */
export function formatDate(date: string | null | undefined): string {
  if (!date) return "";
  const d = new Date(`${date}T00:00:00`);
  if (Number.isNaN(d.getTime())) return date;
  return d.toLocaleDateString(currentLocale(), { year: "numeric", month: "short", day: "numeric" });
}

/** Unix seconds → localized date ("Oct 23, 2026"). */
export function formatDay(ts: number | null | undefined): string {
  if (!ts) return "";
  return new Date(ts * 1000).toLocaleDateString(currentLocale(), { year: "numeric", month: "short", day: "numeric" });
}

export function formatTimestamp(ts: number | null | undefined): string {
  if (!ts) return "";
  return new Date(ts * 1000).toLocaleString(currentLocale(), {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatUptime(secs: number): string {
  const d = Math.floor(secs / 86400);
  const h = Math.floor((secs % 86400) / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (d > 0) return t("format.daysHours", { days: d, hours: h });
  if (h > 0) return t("format.hoursMinutes", { hours: h, minutes: m });
  return t("format.minutes", { n: Math.max(m, 0) });
}


/** "S1 · E3" for episodes. */
export function episodeLabel(item: Pick<ItemSummary, "parent_index" | "index">): string {
  const { parent_index: season, index: episode } = item;
  if (season != null && episode != null) return t("format.episodeShort", { season, episode });
  if (season != null) return t("format.seasonShort", { season });
  if (episode != null) return t("format.episodeOnly", { episode });
  return "";
}

export function seasonTitle(item: Pick<ItemSummary, "title" | "index">): string {
  if (item.title) return item.title;
  if (item.index === 0) return t("format.specials");
  return item.index != null ? t("format.seasonNumber", { number: item.index }) : t("format.season");
}

export function resolutionLabel(width: number | null | undefined, height: number | null | undefined): string | null {
  if (!width && !height) return null;
  const w = width ?? 0;
  const h = height ?? 0;
  if (w >= 3800 || h >= 2100) return "4K";
  if (w >= 2500 || h >= 1400) return "1440p";
  if (w >= 1800 || h >= 1000) return "1080p";
  if (w >= 1200 || h >= 700) return "720p";
  if (h >= 540) return "576p";
  if (h >= 470) return "480p";
  return "SD";
}

export function fileResolution(file: MediaFile | undefined | null): string | null {
  if (!file) return null;
  return resolutionLabel(file.width, file.height);
}

const CODECS: Record<string, string> = {
  h264: "H.264",
  hevc: "HEVC",
  h265: "HEVC",
  av1: "AV1",
  vp9: "VP9",
  vp8: "VP8",
  mpeg4: "MPEG-4",
  mpeg2video: "MPEG-2",
  mpeg1video: "MPEG-1",
  vc1: "VC-1",
  wmv3: "WMV",
  msmpeg4v3: "DivX",
  theora: "Theora",
  prores: "ProRes",
  aac: "AAC",
  ac3: "AC3",
  eac3: "EAC3",
  dts: "DTS",
  truehd: "TrueHD",
  mlp: "MLP",
  flac: "FLAC",
  alac: "ALAC",
  mp3: "MP3",
  mp2: "MP2",
  opus: "Opus",
  vorbis: "Vorbis",
  wmav2: "WMA",
  pcm_s16le: "PCM",
  pcm_s24le: "PCM",
  pcm_s32le: "PCM",
  pcm_f32le: "PCM",
  subrip: "SRT",
  srt: "SRT",
  ass: "ASS",
  ssa: "SSA",
  webvtt: "WebVTT",
  mov_text: "MOV Text",
  hdmv_pgs_subtitle: "PGS",
  pgssub: "PGS",
  dvd_subtitle: "VobSub",
  dvdsub: "VobSub",
  dvb_subtitle: "DVB",
  dvb_teletext: "Teletext",
  eia_608: "CEA-608",
};

export function codecLabel(codec: string | null | undefined): string {
  if (!codec) return "";
  return CODECS[codec.toLowerCase()] ?? codec.toUpperCase();
}

export function channelsLabel(channels: number | undefined, layout?: string): string {
  if (!channels) return layout ?? "";
  switch (channels) {
    case 1:
      return t("format.mono");
    case 2:
      return t("format.stereo");
    case 3:
      return "2.1";
    case 6:
      return "5.1";
    case 7:
      return "6.1";
    case 8:
      return "7.1";
    default:
      return t("format.channels", { n: channels });
  }
}

/**
 * Subtitles the server counts as forced: flagged so, or a title saying so
 * ("English (Forced)", "Signs & Songs"), as streams::is_forced does.
 */
export function isForcedSubtitle(s: MediaStream): boolean {
  if (s.forced) return true;
  return (s.title ?? "")
    .toLowerCase()
    .split(/[^0-9a-zÀ-￿]+/)
    .some((w) => w === "forced" || w === "signs");
}

/**
 * What a subtitle title says beyond its language and "forced" (shown as a
 * flag): "SDH" of "English SDH", nothing of "English (Forced)", the whole
 * title when it doesn't name the language ("Signs & Songs").
 */
function subtitleTitleRest(title: string, lang: string): string {
  const at = title.toLowerCase().indexOf(lang.toLowerCase());
  // A title that doesn't name the language is shown whole, unless it only says "Forced".
  if (at < 0) return /[0-9a-zÀ-￿]/i.test(title.replace(/\bforced\b/gi, "")) ? title : "";
  return (title.slice(0, at) + " " + title.slice(at + lang.length))
    .replace(/\bforced\b/gi, " ")
    .replace(/[()[\]{}\-–—_.,:;|/]+/g, " ")
    .replace(/\s+/g, " ")
    .trim();
}

/**
 * The name of `code`'s language that `title` has: the one shown, or the
 * English one, which files' titles mostly use ("English SDH" in French).
 */
function nameInTitle(title: string, code: string | null | undefined): string | null {
  const lower = title.toLowerCase();
  for (const name of [languageName(code), languageName(code, "en")]) {
    if (name && lower.includes(name.toLowerCase())) return name;
  }
  return null;
}

/** Human readable label for an audio/subtitle/video stream. */
export function streamLabel(s: MediaStream): string {
  const lang = languageName(s.language);
  if (s.kind === "audio") {
    const details = [codecLabel(s.codec), channelsLabel(s.channels, s.channel_layout)].filter(Boolean).join(" ");
    const base = s.title && !lang ? s.title : lang ?? s.title ?? t("states.unknown");
    const extra = s.title && lang && !nameInTitle(s.title, s.language) ? ` – ${s.title}` : "";
    return `${base}${extra}${details ? ` (${details})` : ""}`;
  }
  if (s.kind === "subtitle") {
    const base = lang ?? s.title ?? t("states.unknown");
    const rest = s.title && lang ? subtitleTitleRest(s.title, nameInTitle(s.title, s.language) ?? lang) : "";
    const extra = rest ? ` – ${rest}` : "";
    const flags = [codecLabel(s.codec), isForcedSubtitle(s) ? t("format.forced") : null, s.external ? t("format.external") : null]
      .filter(Boolean)
      .join(", ");
    return `${base}${extra}${flags ? ` (${flags})` : ""}`;
  }
  const res = resolutionLabel(s.width, s.height);
  const depth = s.bit_depth && s.bit_depth > 8 ? t("format.bitDepth", { n: s.bit_depth }) : null;
  return [res, codecLabel(s.codec), dynamicRangeLabel(s), depth].filter(Boolean).join(" ");
}

/**
 * The audio a title's page offers while no track is picked: "Automatic (English)",
 * the language of the details' choice (else of the file's default track). Each
 * player then plays its best track of that language, so the page names no track.
 */
export function automaticAudioLabel(file: MediaFile): string {
  const audio = file.streams.filter((s) => s.kind === "audio");
  const track = audio.find((s) => s.index === file.selected_audio) ?? audio.find((s) => s.default) ?? audio[0];
  const lang = track?.language && !["und", "zxx", "mul"].includes(track.language) ? languageName(track.language) : null;
  return lang ? t("format.automaticLanguage", { language: lang }) : t("format.automatic");
}

/** Whether the person picked the file's audio track; older servers don't say, and their choice reads as picked. */
export function audioPicked(file: MediaFile): boolean {
  return file.audio_picked !== false;
}

type RangeFields =Pick<MediaStream, "dynamic_range" | "dv_profile" | "dv_bl_compat" | "dv_el" | "hdr10plus" | "hdr">;

/**
 * A Dolby Vision profile as the server names it: "P8.1", "P8.4", "P5",
 * "P7 FEL" (the base layer's compatibility only from profile 8 on: profile
 * 7's is always Blu-ray HDR10, never "P7.6").
 */
function dolbyVisionProfile(s: RangeFields): string | null {
  if (s.dv_profile == null) return null;
  const compat = s.dv_profile >= 8 && [1, 2, 4].includes(s.dv_bl_compat ?? -1) ? `.${s.dv_bl_compat}` : "";
  const layer = s.dv_profile === 7 && s.dv_el ? ` ${s.dv_el.toUpperCase()}` : "";
  return `P${s.dv_profile}${compat}${layer}`;
}

/**
 * "Dolby Vision P8.1", "Dolby Vision P7 FEL", "Dolby Vision P8.1 + HDR10+",
 * "HDR10+", "HDR10", "HLG", "HDR" (unknown HDR flavour) or null for SDR: the
 * server's names (docs/API.md, MediaStream), as Android and the TV apps say.
 */
export function dynamicRangeLabel(s: RangeFields | null | undefined): string | null {
  if (!s) return null;
  switch (s.dynamic_range) {
    case "dolby_vision": {
      const profile = dolbyVisionProfile(s);
      return `Dolby Vision${profile ? ` ${profile}` : ""}${s.hdr10plus ? " + HDR10+" : ""}`;
    }
    case "hdr10plus":
      return "HDR10+";
    case "hdr10":
      return "HDR10";
    case "hlg":
      return "HLG";
    case "sdr":
      return null;
    default:
      return s.hdr ? "HDR" : null;
  }
}

/**
 * Badge text for a dynamic range ("DV P8.1", "DV P7 FEL", "HDR10+", "HDR10",
 * "HLG", "HDR"), or null for SDR. Dolby Vision + HDR10+ takes two badges:
 * see {@link dynamicRangeBadges}.
 */
export function dynamicRangeBadge(s: RangeFields | null | undefined): string | null {
  if (!s) return null;
  switch (s.dynamic_range) {
    case "dolby_vision": {
      const profile = dolbyVisionProfile(s);
      return profile ? `DV ${profile}` : "DV";
    }
    case "hdr10plus":
      return "HDR10+";
    case "hdr10":
      return "HDR10";
    case "hlg":
      return "HLG";
    case "sdr":
      return null;
    default:
      return s.hdr ? "HDR" : null;
  }
}

/** The dynamic range's badges: "DV P8.1", and "HDR10+" too for Dolby Vision + HDR10+. */
export function dynamicRangeBadges(s: RangeFields | null | undefined): string[] {
  const badge = dynamicRangeBadge(s);
  if (!badge) return [];
  return s?.dynamic_range === "dolby_vision" && s.hdr10plus ? [badge, "HDR10+"] : [badge];
}

/**
 * One-line description of what the transcoder is doing, e.g.
 * "HEVC 3840×2160 Dolby Vision P8.1 → H.264 1920×1080 · h264_nvenc · libplacebo · 3.2×".
 */
export function transcodeSummary(job: TranscodeInfo): string {
  const parts: string[] = [];
  if (job.video === "transcode") {
    parts.push([job.video_in, job.video_out].filter(Boolean).join(" → ") || t("format.transcode.videoTranscode"));
  } else if (job.video === "copy") {
    parts.push(t("format.transcode.copy", { stream: job.video_in ?? t("format.transcode.video") }));
  }
  if (job.audio === "transcode") {
    const a = [job.audio_in, job.audio_out].filter(Boolean).join(" → ");
    if (a) parts.push(a);
  } else if (job.audio === "copy" && job.audio_in) {
    parts.push(t("format.transcode.copy", { stream: job.audio_in }));
  }
  if (job.encoder && job.video === "transcode") parts.push(job.encoder);
  // The CPU decoding a job whose GPU does the rest: the GPU couldn't decode.
  const decoded = job.decoder !== "cpu" || job.pipeline === "hybrid";
  if (job.decoder && decoded && job.video === "transcode") parts.push(t("format.transcode.decode", { decoder: job.decoder }));
  if (job.tonemap) parts.push(job.tonemap);
  // A stream in several sizes: which of them the player takes, the largest first.
  if (job.variants) parts.push(t("format.transcode.size", { n: (job.rendition ?? 0) + 1, total: job.variants + 1 }));
  if (job.subtitle_burn) parts.push(t("format.transcode.burnIn"));
  if (job.speed != null && job.speed > 0) parts.push(`${fixed(job.speed, 1)}×`);
  return parts.join(" · ");
}

/** Short subtitle line for a card. */
export function cardSubtitle(item: ItemSummary): string {
  switch (item.kind) {
    case "movie":
      return item.year ? String(item.year) : "";
    case "show":
      if (item.child_count != null && item.child_count > 0) return t("format.seasons", { count: item.child_count });
      if (item.leaf_count != null) return t("format.episodes", { count: item.leaf_count });
      return item.year ? String(item.year) : "";
    case "season":
      return item.leaf_count != null ? t("format.episodes", { count: item.leaf_count }) : "";
    case "episode":
      return episodeLabel(item);
    case "artist":
      if (item.child_count != null) return t("format.albums", { count: item.child_count });
      return "";
    case "album":
      return [item.parent_title, item.year].filter(Boolean).join(" · ");
    case "track":
      return [item.grandparent_title, item.parent_title].filter(Boolean).join(" · ");
    default:
      return "";
  }
}

/** Title + context, e.g. "The Office — S2 · E3". */
export function fullTitle(item: ItemSummary): string {
  if (item.kind === "episode") {
    const show = item.grandparent_title ?? "";
    const ep = episodeLabel(item);
    return [show, ep, item.title].filter(Boolean).join(" — ");
  }
  if (item.kind === "season") return [item.parent_title, seasonTitle(item)].filter(Boolean).join(" — ");
  if (item.kind === "track") return [item.title, item.grandparent_title].filter(Boolean).join(" — ");
  return item.year ? `${item.title} (${item.year})` : item.title;
}

export function progressFraction(item: Pick<ItemSummary, "user_state" | "duration_ms">): number {
  const pos = item.user_state?.position_ms ?? 0;
  if (!pos || !item.duration_ms) return 0;
  return Math.min(1, Math.max(0, pos / item.duration_ms));
}

export function unwatchedCount(item: ItemSummary): number {
  if (item.leaf_count == null) return 0;
  return Math.max(0, item.leaf_count - (item.viewed_leaf_count ?? 0));
}

/** A video the player plays (not a show, season or music). */
export function isPlayableVideo(item: Pick<ItemSummary, "kind">): boolean {
  return item.kind === "movie" || item.kind === "episode" || item.kind === "extra";
}

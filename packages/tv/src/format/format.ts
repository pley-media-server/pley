import type { DynamicRange, ItemSummary, MediaFile, MediaStream, MediaSummary } from "../api/types";
import { formatDate, formatNumber, t } from "../i18n";
import { catalogs, type Catalog } from "../i18n/catalogs";

/*
 * What files, streams, times and sizes are like, in words: the Android
 * app's labels (core/model/format), which follow the web's, in the
 * language shown (the format area's words). Codec and format names
 * (H.264, Dolby Vision, 5.1, Mbps) are the same in every language.
 */

/** A number as toFixed writes it ("23.976"), with the language's decimal separator: "23,976" in French. */
export function localDecimal(text: string): string {
  const separator = formatNumber(1.5).replace(/\d/g, "") || ".";
  return separator === "." ? text : text.replace(".", separator);
}

const CODECS: Record<string, string> = {
  h264: "H.264", hevc: "HEVC", h265: "HEVC", av1: "AV1", vp9: "VP9", vp8: "VP8", mpeg4: "MPEG-4", mpeg2video: "MPEG-2",
  mpeg1video: "MPEG-1", vc1: "VC-1", wmv3: "WMV", msmpeg4v3: "DivX", theora: "Theora", prores: "ProRes", aac: "AAC",
  ac3: "AC3", eac3: "EAC3", dts: "DTS", truehd: "TrueHD", mlp: "MLP", flac: "FLAC", alac: "ALAC", mp3: "MP3", mp2: "MP2",
  opus: "Opus", vorbis: "Vorbis", wmav2: "WMA", pcm_s16le: "PCM", pcm_s24le: "PCM", pcm_s32le: "PCM", pcm_f32le: "PCM",
  subrip: "SRT", srt: "SRT", ass: "ASS", ssa: "SSA", webvtt: "WebVTT", mov_text: "MOV Text", hdmv_pgs_subtitle: "PGS",
  pgssub: "PGS", dvd_subtitle: "VobSub", dvdsub: "VobSub", dvb_subtitle: "DVB", dvb_teletext: "Teletext", eia_608: "CEA-608",
};

/** ffprobe's codec name as people know it: "h264" → "H.264", "hdmv_pgs_subtitle" → "PGS"; others upper-cased. */
export function codecLabel(codec: string | null | undefined): string {
  if (!codec) return "";
  return CODECS[codec.toLowerCase()] ?? codec.toUpperCase();
}

/** 2 → "Stereo", 6 → "5.1", 8 → "7.1", 5 → "5ch"; the layout's name without a count. */
export function channelsLabel(channels: number | null | undefined, layout?: string | null): string {
  if (!channels) return layout ?? "";
  switch (channels) {
    case 1:
      return t("format.channels.mono");
    case 2:
      return t("format.channels.stereo");
    case 3:
      return "2.1";
    case 6:
      return "5.1";
    case 7:
      return "6.1";
    case 8:
      return "7.1";
    default:
      return t("format.channels.count", { n: channels });
  }
}

/** "4K", "1440p", "1080p", "720p", "576p", "480p" or "SD"; `null` without a size. */
export function resolutionLabel(width: number | null | undefined, height: number | null | undefined): string | null {
  const w = width ?? 0;
  const h = height ?? 0;
  if (!w && !h) return null;
  if (w >= 3800 || h >= 2100) return "4K";
  if (w >= 2500 || h >= 1400) return "1440p";
  if (w >= 1800 || h >= 1000) return "1080p";
  if (w >= 1200 || h >= 700) return "720p";
  if (h >= 540) return "576p";
  if (h >= 470) return "480p";
  return "SD";
}

/** The four steps badges use: "4K", "1080p", "720p", "SD"; `null` without a size. */
export function resolutionBadge(width: number | null | undefined, height: number | null | undefined): string | null {
  const w = width ?? 0;
  const h = height ?? 0;
  if (w >= 3800 || h >= 2100) return "4K";
  if (w >= 1800 || h >= 1000) return "1080p";
  if (w >= 1200 || h >= 700) return "720p";
  return h > 0 ? "SD" : null;
}

/**
 * "Dolby Vision P8.1" (the base layer's compatibility from profile 8 on:
 * profile 7's is always Blu-ray HDR10, never "P7.6"), "Dolby Vision P7 FEL"
 * (or MEL) with `dvEl`, "+ HDR10+" with `hdr10plus`, "HDR10+", "HDR10",
 * "HLG", "HDR" (an older server's flag), or `null` for SDR: the server's
 * names (docs/API.md, MediaStream), as the web and Android say.
 */
export function dynamicRangeLabel(
  range: DynamicRange | null | undefined,
  dvProfile?: number | null,
  dvBlCompat?: number | null,
  hdr?: boolean | null,
  dvEl?: "fel" | "mel" | null,
  hdr10plus?: boolean | null,
): string | null {
  switch (range) {
    case "dolby_vision": {
      let label = "Dolby Vision";
      if (dvProfile != null) {
        const compat = dvProfile >= 8 && (dvBlCompat === 1 || dvBlCompat === 2 || dvBlCompat === 4) ? `.${dvBlCompat}` : "";
        const layer = dvProfile === 7 && dvEl ? ` ${dvEl.toUpperCase()}` : "";
        label += ` P${dvProfile}${compat}${layer}`;
      }
      return hdr10plus ? `${label} + HDR10+` : label;
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
      return hdr ? "HDR" : null;
  }
}

/** A video stream's whole format: "Dolby Vision P8.1 + HDR10+", "Dolby Vision P7 FEL", "HDR10+"… */
export function streamRange(s: MediaStream | null | undefined): string | null {
  return s ? dynamicRangeLabel(s.dynamic_range, s.dv_profile, s.dv_bl_compat, s.hdr, s.dv_el, s.hdr10plus) : null;
}

/** "DV P8.1", "DV P7 FEL", "DV", "HDR10+", "HDR10", "HLG", "HDR", or `null`: HDR10+ with Dolby Vision is a badge of its own. */
export function dynamicRangeShort(
  range: DynamicRange | null | undefined,
  dvProfile?: number | null,
  dvBlCompat?: number | null,
  hdr?: boolean | null,
  dvEl?: "fel" | "mel" | null,
): string | null {
  const label = dynamicRangeLabel(range, dvProfile, dvBlCompat, hdr, dvEl);
  return label ? label.replace("Dolby Vision", "DV") : null;
}

/** A video stream's dynamic range badges: "DV P8.1", and "HDR10+" too for Dolby Vision + HDR10+. */
export function dynamicRangeBadges(s: MediaStream | null | undefined): string[] {
  if (!s) return [];
  const badge = dynamicRangeShort(s.dynamic_range, s.dv_profile, s.dv_bl_compat, s.hdr, s.dv_el);
  if (!badge) return [];
  return s.dynamic_range === "dolby_vision" && s.hdr10plus ? [badge, "HDR10+"] : [badge];
}

/** Dolby Atmos, as ffprobe names it in the stream's profile or a title says. */
export function isAtmos(s: MediaStream | null | undefined): boolean {
  if (!s || s.kind !== "audio") return false;
  return /atmos/i.test(s.profile ?? "") || /atmos/i.test(s.title ?? "");
}

/** "TrueHD Atmos 7.1", "EAC3 5.1", "AAC Stereo". */
export function audioFormatLabel(s: MediaStream): string {
  return [codecLabel(s.codec), isAtmos(s) ? "Atmos" : "", channelsLabel(s.channels, s.channel_layout)].filter(Boolean).join(" ");
}

/**
 * Subtitles the server counts as forced: flagged so, or a title saying so
 * ("English (Forced)", "Signs & Songs"), as its streams::is_forced does.
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
  for (const name of [languageName(code), englishLanguageName(code)]) {
    if (name && lower.indexOf(name.toLowerCase()) >= 0) return name;
  }
  return null;
}

/**
 * A track as the pickers list it. Audio: "English (EAC3 5.1)", "English –
 * Commentary (AAC Stereo)". Subtitles: "French – SDH (SRT, Forced)".
 * Video: "4K HEVC Dolby Vision P8.1 10-bit".
 */
export function streamLabel(s: MediaStream): string {
  const lang = languageName(s.language);
  const title = s.title || null;
  if (s.kind === "audio") {
    const extra = title && lang && !nameInTitle(title, s.language) ? ` – ${title}` : "";
    const details = audioFormatLabel(s);
    const base = title && !lang ? title : lang ?? title ?? t("format.track.unknown");
    return base + extra + (details ? ` (${details})` : "");
  }
  if (s.kind === "subtitle") {
    const base = lang ?? title ?? t("format.track.unknown");
    const rest = title && lang ? subtitleTitleRest(title, nameInTitle(title, s.language) ?? lang) : "";
    const flags = [codecLabel(s.codec), isForcedSubtitle(s) ? t("format.track.forced") : "", s.external ? t("format.track.external") : ""].filter(Boolean).join(", ");
    return base + (rest ? ` – ${rest}` : "") + (flags ? ` (${flags})` : "");
  }
  return [resolutionLabel(s.width, s.height), codecLabel(s.codec), streamRange(s), s.bit_depth && s.bit_depth > 8 ? t("format.track.bitDepth", { n: s.bit_depth }) : null]
    .filter(Boolean)
    .join(" ");
}

/** The short quality badges of a file: resolution, dynamic range, then "Atmos", else "7.1" or "5.1". */
export function qualityBadges(file: MediaFile | null | undefined): string[] {
  if (!file) return [];
  const video = file.streams.filter((s) => s.kind === "video")[0];
  const audio = file.streams.filter((s) => s.kind === "audio" && s.default)[0] ?? file.streams.filter((s) => s.kind === "audio")[0];
  const channels = audio?.channels ?? 0;
  return [
    resolutionBadge(file.width ?? video?.width, file.height ?? video?.height),
    ...dynamicRangeBadges(video),
    isAtmos(audio) ? "Atmos" : channels >= 8 ? "7.1" : channels >= 6 ? "5.1" : null,
  ].filter((b): b is string => !!b);
}

/** A poster's words for a dynamic range: HDR10 and HDR10+ read as plain "HDR" and "HDR+". */
const SUMMARY_RANGES: Record<string, string> = { dolby_vision: "DV", hdr10plus: "HDR+", hdr10: "HDR", hlg: "HLG" };

/** Badges a listing can give: 4K (no other resolution, as on the web) and the dynamic range. */
export function summaryBadges(media: MediaSummary | null | undefined): string[] {
  if (!media) return [];
  const resolution = resolutionBadge(media.width, media.height) === "4K" ? "4K" : null;
  const range = media.dynamic_range ? (SUMMARY_RANGES[media.dynamic_range] ?? null) : null;
  return [resolution, range].filter((b): b is string => !!b);
}

/** Whether the profile picked the file's audio track; older servers don't say, and their choice reads as picked. */
export function audioPicked(file: MediaFile): boolean {
  return file.audio_picked !== false;
}

/**
 * The audio track a file is described by: the profile's pick, else the
 * default, else the first. Without a pick each player plays its best track
 * of the language, so the details' choice isn't named as if it would play.
 */
export function startingAudio(file: MediaFile): MediaStream | null {
  const audio = file.streams.filter((s) => s.kind === "audio");
  const picked = audioPicked(file) ? audio.filter((s) => s.index === file.selected_audio)[0] : undefined;
  return picked ?? audio.filter((s) => s.default)[0] ?? audio[0] ?? null;
}

/**
 * The audio a title's page offers while no track is picked, as on the web:
 * "Automatic (English)", the language of the details' choice (else of the
 * file's default track).
 */
export function automaticAudioLabel(file: MediaFile): string {
  const audio = file.streams.filter((s) => s.kind === "audio");
  const track = audio.filter((s) => s.index === file.selected_audio)[0] ?? audio.filter((s) => s.default)[0] ?? audio[0];
  const code = track?.language ?? "";
  const lang = code && code !== "und" && code !== "zxx" && code !== "mul" ? languageName(code) : null;
  return lang ? t("format.track.automaticLanguage", { language: lang }) : t("format.track.automatic");
}

/** "Final Cut · 4K · HEVC · MKV · 58.2 GB", else "Version 2". */
export function versionLabel(file: MediaFile, position: number): string {
  const parts = [file.edition ?? "", resolutionLabel(file.width, file.height) ?? "", codecLabel(file.video_codec), (file.container ?? "").toUpperCase(), file.size > 0 ? formatBytes(file.size) : ""].filter(Boolean);
  return parts.length ? parts.join(" · ") : t("format.track.version", { number: position + 1 });
}

// ---- languages

/** A language Pley names: its two-letter code (ISO 639-1), else its three-letter one, as the format area's `languages` has them. */
type LanguageId = keyof Catalog["format"]["languages"];

/** ISO 639-2 codes (bibliographic and terminology) → the language's id. */
const LANGUAGE_IDS: Record<string, LanguageId> = {
  afr: "af", alb: "sq", sqi: "sq", amh: "am", ara: "ar", arm: "hy", hye: "hy", aze: "az", baq: "eu", eus: "eu", bel: "be",
  ben: "bn", bos: "bs", bre: "bre", bul: "bg", bur: "my", mya: "my", cat: "ca", chi: "zh", zho: "zh", cze: "cs", ces: "cs",
  dan: "da", dut: "nl", nld: "nl", eng: "en", est: "et", fao: "fao", fil: "fil", fin: "fi", fre: "fr", fra: "fr", geo: "ka",
  kat: "ka", ger: "de", deu: "de", gle: "ga", glg: "gl", gre: "el", ell: "el", guj: "gu", heb: "he", hin: "hi", hrv: "hr",
  hun: "hu", ice: "is", isl: "is", ind: "id", ita: "it", jpn: "ja", kan: "kn", kaz: "kk", khm: "km", kor: "ko", kur: "ku",
  lao: "lo", lat: "la", lav: "lv", lit: "lt", ltz: "lb", mac: "mk", mkd: "mk", mal: "ml", mao: "mi", mri: "mi", mar: "mr",
  may: "ms", msa: "ms", mlt: "mt", mon: "mn", nep: "ne", nob: "nb", nno: "nn", nor: "no", per: "fa", fas: "fa", pol: "pl",
  por: "pt", pan: "pa", rum: "ro", ron: "ro", rus: "ru", slo: "sk", slk: "sk", slv: "sl", som: "so", spa: "es", srp: "sr",
  swa: "sw", swe: "sv", tam: "ta", tel: "te", tgl: "tl", tha: "th", tib: "bo", bod: "bo", tur: "tr", ukr: "uk", urd: "ur",
  uzb: "uz", vie: "vi", wel: "cy", cym: "cy", yid: "yi", zul: "zu", yue: "yue", cmn: "cmn", nan: "nan", hat: "hat",
  haw: "haw", ibo: "ibo", yor: "yor", hau: "hau", sin: "sin", pus: "pus", tat: "tat", kir: "kir", tuk: "tuk", tgk: "tgk",
  und: "und", mul: "mul", zxx: "zxx", qaa: "qaa",
};

/** The two-letter codes Pley names, each the id of a language above. */
const TWO_LETTER: Record<string, true> = {};
for (const code in LANGUAGE_IDS) {
  const id = LANGUAGE_IDS[code]!;
  if (id.length === 2) TWO_LETTER[id] = true;
}

/** The id of the language `code` names ("eng", "en", "EN"), if Pley names it. */
function languageId(code: string): LanguageId | undefined {
  const base = code.trim().toLowerCase();
  if (Object.prototype.hasOwnProperty.call(LANGUAGE_IDS, base)) return LANGUAGE_IDS[base];
  return Object.prototype.hasOwnProperty.call(TWO_LETTER, base) ? (base as LanguageId) : undefined;
}

/**
 * A language's two-letter code (ISO 639-1), as subtitle sites name them:
 * "eng" → "en", "chi" → "zh"; a code without one comes back as it is.
 */
export function twoLetterCode(code: string): string {
  const base = code.trim().toLowerCase();
  if (base.length === 2) return base;
  const id = languageId(base);
  return id && id.length === 2 ? id : base;
}

/** `code` as `name` writes a language: "pt-BR" → "Portuguese (BR)"; unknown codes upper-cased; `null` without one. */
function nameOf(code: string | null | undefined, name: (id: LanguageId) => string): string | null {
  const raw = (code ?? "").trim();
  if (!raw) return null;
  const parts = raw.toLowerCase().split(/[-_]/);
  const id = languageId(parts[0]!);
  if (!id) return raw.length <= 3 ? raw.toUpperCase() : raw;
  return parts[1] ? `${name(id)} (${parts[1].toUpperCase()})` : name(id);
}

/** "eng" → "English" ("Anglais" in French), "pt-BR" → "Portuguese (BR)", unknown codes upper-cased; `null` without one. */
export function languageName(code: string | null | undefined): string | null {
  return nameOf(code, (id) => t(`format.languages.${id}`));
}

/** The language's English name, as files' titles mostly have it. */
function englishLanguageName(code: string | null | undefined): string | null {
  return nameOf(code, (id) => catalogs.en.format.languages[id]);
}

// ---- items

/** "S1 · E3", "E3", or "" with neither number. */
export function episodeLabel(season: number | null | undefined, episode: number | null | undefined): string {
  if (season != null && episode != null) return t("format.episode.both", { season, episode });
  if (season != null) return t("format.episode.season", { season });
  return episode != null ? t("format.episode.episode", { episode }) : "";
}

/** A season's own title, else "Specials" for season 0, else "Season 2". */
export function seasonTitle(title: string | null | undefined, index: number | null | undefined): string {
  if (title) return title;
  if (index === 0) return t("format.season.specials");
  return index != null ? t("format.season.numbered", { number: index }) : t("format.season.plain");
}

/** What `pluralize` counts, by its English word, and the format area's `counts` key for it. */
const COUNTED = {
  season: "seasons",
  episode: "episodes",
  album: "albums",
  track: "tracks",
  movie: "movies",
  show: "shows",
  artist: "artists",
  item: "items",
  title: "titles",
  collection: "collections",
} as const;

export type Counted = keyof typeof COUNTED;

/** "1 movie", "12,345 movies", in the language shown. */
export function pluralize(n: number, what: Counted): string {
  return t(`format.counts.${COUNTED[what]}`, { count: n });
}

/** The short line under a card, by kind: a year, "3 seasons", "S2 · E5", "Daft Punk · 2001"… */
export function cardSubtitle(item: ItemSummary): string {
  switch (item.kind) {
    case "movie":
      return item.year != null ? String(item.year) : "";
    case "show":
      if (item.child_count) return pluralize(item.child_count, "season");
      if (item.leaf_count != null) return pluralize(item.leaf_count, "episode");
      return item.year != null ? String(item.year) : "";
    case "season":
      return item.leaf_count != null ? pluralize(item.leaf_count, "episode") : "";
    case "episode":
      return episodeLabel(item.parent_index, item.index);
    case "artist":
      return item.child_count != null ? pluralize(item.child_count, "album") : "";
    case "album":
      return [item.parent_title || null, item.year != null ? String(item.year) : null].filter(Boolean).join(" · ");
    case "track":
      return [item.grandparent_title || null, item.parent_title || null].filter(Boolean).join(" · ");
    default:
      return "";
  }
}

/** How much of an item has been played, 0 to 1. */
export function progressFraction(positionMs: number, durationMs: number | null | undefined): number {
  if (positionMs <= 0 || !durationMs || durationMs <= 0) return 0;
  return Math.max(0, Math.min(1, positionMs / durationMs));
}

/** An item's resume point is worth showing: started, not finished, not marked watched. */
export function isInProgress(item: ItemSummary): boolean {
  const d = item.duration_ms;
  return !!d && !item.user_state.watched && item.user_state.position_ms > 0 && item.user_state.position_ms < d;
}

/** Episodes of a show or season not watched yet; 0 for other kinds. */
export function unwatchedCount(item: ItemSummary): number {
  return item.leaf_count != null ? Math.max(0, item.leaf_count - (item.viewed_leaf_count ?? 0)) : 0;
}

/** A video the player plays: a movie, an episode or an extra. */
export function isPlayableVideo(kind: ItemSummary["kind"]): boolean {
  return kind === "movie" || kind === "episode" || kind === "extra";
}

// ---- times

const MINUTE_MS = 60_000;

function wholeMinutes(ms: number): number {
  return Math.round(ms / MINUTE_MS);
}

/** 6720000 → "1h 52m", 2700000 → "45m", 29000 → "29s". */
export function formatDuration(ms: number | null | undefined): string {
  if (!ms || ms <= 0) return "";
  const total = wholeMinutes(ms);
  if (total < 1) return t("format.duration.seconds", { n: Math.max(1, Math.round(ms / 1000)) });
  const h = Math.floor(total / 60);
  const m = total % 60;
  if (!h) return t("format.duration.minutes", { n: m });
  return m ? t("format.duration.hoursMinutes", { hours: h, minutes: m }) : t("format.duration.hours", { n: h });
}

/** "58 min" under an hour, "1h 52m" from an hour, "29s" below half a minute. */
export function formatRuntime(ms: number | null | undefined): string {
  if (!ms || ms <= 0) return "";
  const total = wholeMinutes(ms);
  return total >= 1 && total <= 59 ? t("format.duration.runtime", { n: total }) : formatDuration(ms);
}

/** "26 min left", "1h 12m left"; empty without a duration or once nothing is left. */
export function formatTimeLeft(positionMs: number, durationMs: number | null | undefined): string {
  if (!durationMs || durationMs <= 0) return "";
  const left = formatRuntime(Math.max(0, durationMs - positionMs));
  return left ? t("format.duration.left", { time: left }) : "";
}

/** Milliseconds → "1:02:03" or "2:03" ("0:00" for nothing). */
export function formatClock(ms: number | null | undefined): string {
  if (ms == null || !isFinite(ms) || ms < 0) return "0:00";
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const mm = h > 0 ? pad2(m) : String(m);
  return (h > 0 ? `${h}:` : "") + mm + ":" + pad2(sec);
}

export function pad2(n: number): string {
  return n < 10 ? `0${n}` : String(n);
}

/** A time of day as the TV is set to show it: "21:05" or "9:05 PM". */
export function formatTimeOfDay(date: Date, hours24: boolean): string {
  const h = date.getHours();
  const m = pad2(date.getMinutes());
  if (hours24) return `${pad2(h)}:${m}`;
  const h12 = h % 12 === 0 ? 12 : h % 12;
  return `${h12}:${m} ${h < 12 ? "AM" : "PM"}`;
}

/*
 * Relative times are written here, with words of the format area: the
 * TVs' engines (Chromium 53 and 56) have no Intl.RelativeTimeFormat.
 */

/** Calendar days from `now`'s day to `at`'s, on the TV's clock (1: tomorrow). */
function daysBetween(now: Date, at: Date): number {
  const a = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime();
  const b = new Date(at.getFullYear(), at.getMonth(), at.getDate()).getTime();
  return Math.round((b - a) / 86_400_000);
}

/** Unix seconds → "Nov 4" ("4 nov." in French), with the year when it isn't `now`'s (or with `year`). */
export function shortDate(seconds: number, now = Date.now(), year = false): string {
  const d = new Date(seconds * 1000);
  const withYear = year || d.getFullYear() !== new Date(now).getFullYear();
  return formatDate(d, withYear ? { month: "short", day: "numeric", year: "numeric" } : { month: "short", day: "numeric" });
}

/** "3 hours ago", "yesterday", "now"; a month on, the date. */
export function relativeTime(seconds: number, now = Date.now()): string {
  const diff = Math.max(0, now / 1000 - seconds);
  if (diff < 60) return t("format.time.justNow");
  if (diff < 3600) return t("format.time.minutesAgo", { n: Math.floor(diff / 60) });
  if (diff < 86400) return t("format.time.hoursAgo", { count: Math.floor(diff / 3600) });
  const days = Math.floor(diff / 86400);
  if (days === 1) return t("format.time.yesterday");
  if (days < 30) return t("format.time.daysAgo", { count: days });
  return shortDate(seconds, now, true);
}

/**
 * How long until `seconds`, to follow a verb ("you can ask for more in 3
 * days"): "in 5 min" and "in 2 hours" within a day, then by the calendar,
 * "tomorrow", "in 3 days"; "now" once it has come.
 */
export function timeUntil(seconds: number, now = Date.now()): string {
  const diff = seconds - now / 1000;
  if (diff <= 30) return t("format.time.now");
  if (diff < 3600) return t("format.time.inMinutes", { n: Math.max(1, Math.round(diff / 60)) });
  if (diff < 86400) return t("format.time.inHours", { count: Math.max(1, Math.round(diff / 3600)) });
  const days = Math.max(1, daysBetween(new Date(now), new Date(seconds * 1000)));
  return days === 1 ? t("format.time.tomorrow") : t("format.time.inDays", { count: days });
}

/**
 * When something comes out, after "Out" or "Airs": its day ("today",
 * "tomorrow", "Nov 4"), and with `time` its time as well within the next
 * 24 hours ("today at 21:05").
 */
export function dayAndTime(seconds: number, opts: { time: boolean; hours24: boolean }, now = Date.now()): string {
  const at = new Date(seconds * 1000);
  const days = daysBetween(new Date(now), at);
  const ahead = seconds * 1000 - now;
  const day = days === 0 ? t("format.time.today") : days === 1 ? t("format.time.tomorrow") : shortDate(seconds, now);
  return opts.time && ahead > 0 && ahead <= 86_400_000 ? t("format.time.dayAt", { day, time: formatTimeOfDay(at, opts.hours24) }) : day;
}

// ---- sizes

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/** 1536 → "1.5 KB", 150 MiB → "150 MB": three significant digits, in units of 1024 ("1,5 Ko" in French). */
export function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null) return "";
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < BYTE_UNITS.length - 1) {
    v /= 1024;
    i++;
  }
  return `${localDecimal(v.toFixed(v >= 100 || i === 0 ? 0 : 1))} ${t(`format.bytes.${BYTE_UNITS[i]!}`)}`;
}

/** Bits per second → "8.2 Mbps", "12 Mbps", "320 kbps"; empty without a rate. */
export function formatBitrate(bps: number | null | undefined): string {
  if (!bps) return "";
  if (bps >= 1_000_000) return `${localDecimal((bps / 1_000_000).toFixed(bps >= 10_000_000 ? 0 : 1))} Mbps`;
  return `${Math.round(bps / 1000)} kbps`;
}

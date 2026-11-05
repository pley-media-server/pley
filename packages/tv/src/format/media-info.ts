import type { MediaFile, MediaStream } from "../api/types";
import { t } from "../i18n";
import { channelsLabel, codecLabel, formatBitrate, formatBytes, formatDuration, isAtmos, languageName, localDecimal, resolutionLabel, streamRange } from "./format";

/*
 * The item page's media info (TvMediaInfo): a file's facts in blocks of
 * "label: value" lines, as the Android app's MediaFacts words them, in the
 * language shown. The colour standards keep their own names.
 */

export interface FactBlock {
  title: string;
  facts: { label: string; value: string }[];
}

type Stream = MediaStream & { color_primaries?: string; color_transfer?: string; color_space?: string; field_order?: string };

/** A codec level as people write it: HEVC 153 → "5.1", H.264 51 → "5.1", AV1 13 → "5.1". */
export function levelLabel(codec: string, level: number | null | undefined): string | null {
  if (level == null || level <= 0) return null;
  const c = codec.toLowerCase();
  if (c === "av1") return `${2 + (level >> 2)}.${level & 3}`;
  const n = c === "hevc" || c === "h265" ? level / 30 : level / 10;
  return n.toFixed(1).replace(/\.0$/, "");
}

/** "Main 10 · Level 5.1". */
export function profileLevelLabel(s: MediaStream): string | null {
  const parts = [s.profile ?? "", (() => {
    const l = levelLabel(s.codec, s.level);
    return l ? t("format.media.level", { level: l }) : "";
  })()].filter(Boolean);
  return parts.length ? parts.join(" · ") : null;
}

/** "23.976 fps": up to three decimals, trailing zeros left out. */
export function frameRateLabel(fps: number | null | undefined, interlaced = false): string | null {
  if (!fps || fps <= 0) return null;
  const text = localDecimal(fps.toFixed(3).replace(/\.?0+$/, ""));
  return interlaced ? t("format.media.fpsInterlaced", { fps: text }) : t("format.media.fps", { fps: text });
}

/** "48 kHz", "44.1 kHz". */
export function sampleRateLabel(hz: number | null | undefined): string | null {
  if (!hz || hz <= 0) return null;
  return `${localDecimal((hz / 1000).toFixed(1).replace(/\.0$/, ""))} kHz`;
}

/** "10-bit · 4:2:0". */
export function bitDepthLabel(depth: number | null | undefined, pixFmt: string | null | undefined): string | null {
  const parts: string[] = [];
  if (depth) parts.push(t("format.track.bitDepth", { n: depth }));
  const f = (pixFmt ?? "").toLowerCase();
  const chroma = /^(yuv420|nv12|p010)/.test(f) ? "4:2:0" : /^yuv422/.test(f) ? "4:2:2" : /^yuv444/.test(f) ? "4:4:4" : /^gray/.test(f) ? "4:0:0" : "";
  if (chroma) parts.push(chroma);
  return parts.length ? parts.join(" · ") : null;
}

const PRIMARIES: Record<string, string> = {
  bt709: "BT.709", bt2020: "BT.2020", smpte170m: "BT.601 NTSC", bt470bg: "BT.601 PAL", bt470m: "BT.470 M", smpte240m: "SMPTE 240M",
  smpte431: "DCI-P3", smpte432: "Display P3", film: "Film",
};
const TRANSFERS: Record<string, string> = {
  smpte2084: "PQ (SMPTE ST 2084)", "arib-std-b67": "HLG (ARIB STD-B67)", bt709: "BT.709", smpte170m: "BT.601", bt470bg: "BT.470 BG",
  "bt2020-10": "BT.2020 (10-bit)", "bt2020-12": "BT.2020 (12-bit)", "iec61966-2-1": "sRGB", linear: "Linear",
};
const MATRICES: Record<string, string> = {
  bt709: "BT.709", bt2020nc: "BT.2020 non-constant", bt2020c: "BT.2020 constant", smpte170m: "BT.601", bt470bg: "BT.601", ictcp: "ICtCp",
  gbr: "RGB", ycgco: "YCgCo", fcc: "FCC",
};

function colour(table: Record<string, string>, name: string | null | undefined): string | null {
  if (!name || name === "unknown" || name === "reserved") return null;
  return table[name] ?? name;
}

/**
 * "Profile 8.1 · Level 6 · HDR10-compatible base layer"; "Profile 8 · … ·
 * Blu-ray HDR10-compatible base layer" for compatibility 6, as the labels
 * name it (8.1, 8.2 and 8.4 only).
 */
export function dolbyVisionDetails(s: MediaStream): string | null {
  if (s.dv_profile == null) return null;
  const compat = s.dv_bl_compat ?? 0;
  const parts = [t("format.media.dvProfile", { profile: `${s.dv_profile}${s.dv_profile >= 8 && [1, 2, 4].includes(compat) ? `.${compat}` : ""}` })];
  if (s.dv_level != null) parts.push(t("format.media.level", { level: s.dv_level }));
  const base: Record<number, string> = { 1: "HDR10", 2: "SDR", 4: "HLG", 6: "Blu-ray HDR10" };
  if (compat === 0) parts.push(t("format.media.noBaseLayer"));
  else if (base[compat]) parts.push(t("format.media.baseLayer", { range: base[compat]! }));
  if (s.dv_profile === 7 && s.dv_el) parts.push(s.dv_el === "fel" ? t("format.media.fullLayer") : t("format.media.minimalLayer"));
  return parts.join(" · ");
}

/** "3840×2160 (4K)". */
export function dimensionsLabel(w: number | null | undefined, h: number | null | undefined): string | null {
  if (!w || !h) return null;
  const r = resolutionLabel(w, h);
  return `${w}×${h}${r ? ` (${r})` : ""}`;
}

function flags(s: MediaStream): string | null {
  const f = [s.default ? t("format.media.default") : "", s.forced ? t("format.track.forced") : "", s.external ? t("format.media.beside") : ""].filter(Boolean);
  return f.length ? f.join(", ") : null;
}

function push(out: { label: string; value: string }[], label: string, value: string | null | undefined): void {
  if (value && value.trim()) out.push({ label, value });
}

/** The file name only, after the last / or \\. */
export function fileName(path: string): string {
  const i = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return i >= 0 ? path.slice(i + 1) : path;
}

/** HDR of a video stream: "Dolby Vision P8.1 + HDR10+", "Dolby Vision P7 FEL", "HDR10", "SDR". */
function hdrValue(s: MediaStream): string {
  return streamRange(s) ?? "SDR";
}

/** A file's blocks: the file, then each stream, video first. */
export function mediaFacts(file: MediaFile): FactBlock[] {
  const blocks: FactBlock[] = [];
  const f: { label: string; value: string }[] = [];
  push(f, t("format.media.name"), fileName(file.path));
  push(f, t("format.media.container"), file.container ? file.container.toUpperCase() : null);
  push(f, t("format.media.size"), file.size ? formatBytes(file.size) : null);
  push(f, t("format.media.duration"), file.duration_ms ? formatDuration(file.duration_ms) : null);
  push(f, t("format.media.overallBitrate"), file.bitrate ? formatBitrate(file.bitrate) : null);
  blocks.push({ title: t("format.media.file"), facts: f });
  const order = { video: 0, audio: 1, subtitle: 2 } as const;
  const streams = file.streams.slice().sort((a, b) => order[a.kind] - order[b.kind] || a.index - b.index) as Stream[];
  const counts = { video: 0, audio: 0, subtitle: 0 };
  for (const s of streams) counts[s.kind]++;
  const seen = { video: 0, audio: 0, subtitle: 0 };
  for (const s of streams) {
    seen[s.kind]++;
    const name = s.kind === "video" ? t("format.media.video") : s.kind === "audio" ? t("format.media.audio") : t("format.media.subtitles");
    blocks.push(streamBlock(s, counts[s.kind] > 1 ? `${name} ${seen[s.kind]}` : name));
  }
  return blocks;
}

/** One stream's facts, under `title`. */
export function streamBlock(stream: MediaStream, title: string): FactBlock {
  const s = stream as Stream;
  const facts: { label: string; value: string }[] = [];
  if (s.kind === "video") {
    push(facts, t("format.media.codec"), codecLabel(s.codec));
    push(facts, t("format.media.profile"), profileLevelLabel(s));
    push(facts, t("format.media.resolution"), dimensionsLabel(s.width, s.height));
    push(facts, t("format.media.frameRate"), frameRateLabel(s.frame_rate, !!s.field_order && s.field_order !== "progressive"));
    push(facts, t("format.media.bitDepth"), bitDepthLabel(s.bit_depth, s.pix_fmt));
    push(facts, "HDR", hdrValue(s));
    push(facts, "Dolby Vision", dolbyVisionDetails(s));
    push(facts, t("format.media.primaries"), colour(PRIMARIES, s.color_primaries));
    push(facts, t("format.media.transfer"), colour(TRANSFERS, s.color_transfer));
    push(facts, t("format.media.matrix"), colour(MATRICES, s.color_space));
    push(facts, t("format.media.bitrate"), s.bitrate ? formatBitrate(s.bitrate) : null);
    push(facts, t("format.media.title"), s.title);
  } else if (s.kind === "audio") {
    push(facts, t("format.media.codec"), codecLabel(s.codec) + (isAtmos(s) ? " Atmos" : ""));
    push(facts, t("format.media.profile"), s.profile);
    const ch = channelsLabel(s.channels, s.channel_layout);
    push(facts, t("format.media.channels"), s.channel_layout && ch && s.channel_layout !== ch ? `${ch} (${s.channel_layout})` : ch);
    push(facts, t("format.media.sampleRate"), sampleRateLabel(s.sample_rate));
    push(facts, t("format.media.language"), languageName(s.language) ?? s.language);
    push(facts, t("format.media.title"), s.title);
    push(facts, t("format.media.bitrate"), s.bitrate ? formatBitrate(s.bitrate) : null);
    push(facts, t("format.media.flags"), flags(s));
  } else {
    push(facts, t("format.media.format"), `${codecLabel(s.codec)}, ${s.text_based === false ? t("format.media.picture") : t("format.media.text")}`);
    push(facts, t("format.media.language"), languageName(s.language) ?? s.language);
    push(facts, t("format.media.title"), s.title);
    push(facts, t("format.media.flags"), flags(s));
  }
  return { title, facts };
}

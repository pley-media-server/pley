import type { MediaFile, PlaybackMode } from "../api/types";
import { formatBitrate, resolutionLabel } from "../format/format";
import { t } from "../i18n";
import type { Rung } from "./autoquality";

/**
 * The qualities the player offers (docs/QUALITY.md, the same on every
 * app): the original, Automatic, or a conversion capped at a bitrate and a
 * height. Ids are what the server's `auto_quality` names.
 */
export interface QualityPreset {
  id: string;
  label: string;
  maxBitrateKbps: number | null;
  maxHeight: number | null;
}

// The two named qualities' labels are words: read where they show, in the language shown then.
export const ORIGINAL: QualityPreset = {
  id: "original",
  get label() {
    return t("player.quality.original");
  },
  maxBitrateKbps: null,
  maxHeight: null,
};
/** The original, lower only when it doesn't play well (autoquality.ts). */
export const AUTO: QualityPreset = {
  id: "auto",
  get label() {
    return t("player.quality.automatic");
  },
  maxBitrateKbps: null,
  maxHeight: null,
};

/** The 4K qualities first, then 1080p and below. */
export const PRESETS: QualityPreset[] = [
  ORIGINAL,
  AUTO,
  { id: "30m", label: "30 Mbps 4K", maxBitrateKbps: 30000, maxHeight: 2160 },
  { id: "15m", label: "15 Mbps 4K", maxBitrateKbps: 15000, maxHeight: 2160 },
  { id: "20m", label: "20 Mbps 1080p", maxBitrateKbps: 20000, maxHeight: 1080 },
  { id: "12m", label: "12 Mbps 1080p", maxBitrateKbps: 12000, maxHeight: 1080 },
  { id: "8m", label: "8 Mbps 1080p", maxBitrateKbps: 8000, maxHeight: 1080 },
  { id: "4m", label: "4 Mbps 720p", maxBitrateKbps: 4000, maxHeight: 720 },
  { id: "2m", label: "2 Mbps 480p", maxBitrateKbps: 2000, maxHeight: 480 },
];

export function presetById(id: string | null | undefined): QualityPreset | null {
  return PRESETS.filter((p) => p.id === id)[0] ?? null;
}

/** The mode a quality asks for: the original lets the server pick, the others convert. Automatic asks as what it plays. */
export function modeOf(preset: QualityPreset): PlaybackMode {
  return preset.maxBitrateKbps === null ? "auto" : "transcode";
}

function heightOf(file: MediaFile | null): number | null {
  const video = file?.streams.filter((s) => s.kind === "video")[0];
  return file?.height ?? video?.height ?? null;
}

function widthOf(file: MediaFile | null): number | null {
  const video = file?.streams.filter((s) => s.kind === "video")[0];
  return file?.width ?? video?.width ?? null;
}

/**
 * A file's lines, as qualities are named: 2160, 1080, 720 or 480, by its
 * width too (a 1920×800 film is 1080p); `null` when neither is known.
 */
function linesOf(file: MediaFile | null): number | null {
  const w = widthOf(file) ?? 0;
  const h = heightOf(file) ?? 0;
  if (w <= 0 && h <= 0) return null;
  return w >= 3800 || h >= 2100 ? 2160 : w >= 1800 || h >= 1000 ? 1080 : w >= 1200 || h >= 700 ? 720 : 480;
}

/** A copy at more than this share of a file's bitrate, at its lines, plays no better than the file. */
const LIGHTER = 0.8;

/**
 * `preset` as it is named for `file`: by the file's own lines when the
 * preset is taller than the file ("8 Mbps 720p" for a 720p file, whose copy
 * is no taller), else as the table names it.
 */
export function namedFor(file: MediaFile | null, preset: QualityPreset): QualityPreset {
  const lines = linesOf(file);
  if (preset.maxBitrateKbps === null || preset.maxHeight === null || lines === null || preset.maxHeight <= lines) return preset;
  return { ...preset, label: `${preset.maxBitrateKbps / 1000} Mbps ${lines >= 2160 ? "4K" : `${lines}p`}` };
}

/**
 * The qualities worth offering for `file` (docs/QUALITY.md): the original
 * and Automatic, and each of `table` with fewer lines than the file or at
 * most 80 % of its bitrate (every one when neither is known), named by the
 * file's own lines when taller (`namedFor`), once for each bitrate and
 * lines. Renamed, a 4K quality falls among the file's own: the copies go
 * tallest first, then by bitrate, which is the table's order for a 4K file.
 */
export function qualitiesFor(file: MediaFile | null, table: QualityPreset[] = PRESETS): QualityPreset[] {
  const kbps = file?.bitrate ? file.bitrate / 1000 : null;
  const height = heightOf(file);
  if (kbps == null && height == null) return table.slice();
  const lines = linesOf(file);
  const out: QualityPreset[] = [];
  const copies: { preset: QualityPreset; lines: number }[] = [];
  const seen: string[] = [];
  for (const p of table) {
    if (p.maxBitrateKbps === null || p.maxHeight === null) {
      out.push(p);
      continue;
    }
    const fewerLines = height != null && p.maxHeight < height;
    const lighter = kbps != null && p.maxBitrateKbps <= kbps * LIGHTER;
    if (!fewerLines && !lighter) continue;
    const shown = lines === null ? p.maxHeight : Math.min(p.maxHeight, lines);
    const key = `${p.maxBitrateKbps}:${shown}`;
    if (seen.indexOf(key) >= 0) continue;
    seen.push(key);
    copies.push({ preset: namedFor(file, p), lines: shown });
  }
  copies.sort((a, b) => b.lines - a.lines || b.preset.maxBitrateKbps! - a.preset.maxBitrateKbps!);
  return out.concat(copies.map((c) => c.preset));
}

/**
 * The qualities the menu offers for `file`: those worth offering
 * (`qualitiesFor`), or the original alone on a server that converts no
 * video (`PlaybackInfo.original_only`), which has no other to give.
 */
export function offeredQualities(file: MediaFile | null, originalOnly: boolean): QualityPreset[] {
  return originalOnly ? [ORIGINAL] : qualitiesFor(file);
}

/**
 * The quality to start with: the remembered one when this file offers it;
 * the original when it doesn't need that one (an 8 Mbps cap on a 5 Mbps
 * file); Automatic when none was ever chosen.
 */
export function chooseQuality(remembered: string | null, offered: QualityPreset[]): QualityPreset {
  if (!remembered || !presetById(remembered)) return AUTO;
  return offered.filter((p) => p.id === remembered)[0] ?? ORIGINAL;
}

/**
 * What the server converts the original at (its closest_quality): the
 * file's lines, the highest bitrate up to 1.5 times the file's, else that
 * resolution's lowest. Automatic never goes above it.
 */
export function serverQuality(file: MediaFile | null): QualityPreset {
  const lines = linesOf(file) ?? 1080;
  const cap = file?.bitrate && file.bitrate > 0 ? (file.bitrate * 1.5) / 1000 : null;
  const tier = PRESETS.filter((p) => p.maxHeight === lines);
  return tier.filter((p) => cap === null || p.maxBitrateKbps! <= cap)[0] ?? tier[tier.length - 1]!;
}

/**
 * The qualities Automatic steps through below the original, highest first:
 * those offered for the file, at no more lines and bitrate than the
 * server's own quality for it, each lower in bitrate than the one above
 * (a 4K file goes 30 Mbps 4K, 15 Mbps 4K, then 12 Mbps 1080p).
 */
export function autoLadder(file: MediaFile | null): Rung[] {
  const top = serverQuality(file);
  const out: Rung[] = [];
  let below = Infinity;
  for (const p of qualitiesFor(file)) {
    if (p.maxBitrateKbps === null || p.maxHeight === null) continue;
    if (p.maxHeight > top.maxHeight! || p.maxBitrateKbps > top.maxBitrateKbps! || p.maxBitrateKbps >= below) continue;
    out.push({ id: p.id, label: p.label, kbps: p.maxBitrateKbps, height: p.maxHeight });
    below = p.maxBitrateKbps;
  }
  return out;
}

/** The lines of `file` converted at `preset`: the preset's, or the file's own when fewer. */
export function streamLines(file: MediaFile | null, preset: QualityPreset): number {
  const lines = linesOf(file);
  const most = preset.maxHeight ?? lines ?? 1080;
  return lines === null ? most : Math.min(most, lines);
}

/**
 * The least bitrate a converted stream of `lines` keeps when its bitrate
 * changes as it is sent (docs/QUALITY.md, "Players that go black between
 * streams"), kbps: 8 Mbps for 4K, 3 for 1080p, 1.5 for 720p, 1 below.
 */
export function liveFloor(lines: number): number {
  return lines >= 2160 ? 8000 : lines >= 1080 ? 3000 : lines >= 720 ? 1500 : 1000;
}

/** "4 Mbps 1080p": the stream of `file` asked for at `preset`, going on at `kbps`. */
export function liveLabel(file: MediaFile | null, preset: QualityPreset, kbps: number): string {
  const lines = streamLines(file, preset);
  return `${Math.round(kbps / 100) / 10} Mbps ${lines >= 2160 ? "4K" : `${lines}p`}`;
}

/** "1080p · 8.2 Mbps": what a file is, for the Original row's hint. */
export function fileHint(file: MediaFile | null): string {
  if (!file) return "";
  const video = file.streams.filter((s) => s.kind === "video")[0];
  return [resolutionLabel(file.width ?? video?.width, file.height ?? video?.height), formatBitrate(file.bitrate)].filter(Boolean).join(" · ");
}

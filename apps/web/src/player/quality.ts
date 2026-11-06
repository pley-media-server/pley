import type { MediaFile, PlaybackMode } from "@/api/types";
import { t } from "@/i18n";
import type { Rung } from "./autoQuality";

/**
 * A quality of the menu (docs/QUALITY.md, the same on every app): the
 * original, Automatic, or a conversion capped at a bitrate and a height.
 */
export interface QualityOption {
  id: string;
  /** Its name, as shown by `qualityLabel` (which says Original and Automatic in the app's language). */
  label: string;
  /** kbps */
  maxBitrate: number | null;
  maxHeight: number | null;
}

export const ORIGINAL: QualityOption = { id: "original", label: "Original", maxBitrate: null, maxHeight: null };
/** The original, lower only when it doesn't play well (autoQuality.ts). */
export const AUTO: QualityOption = { id: "auto", label: "Automatic", maxBitrate: null, maxHeight: null };

/** The 4K qualities first, then 1080p and below. */
export const QUALITIES: QualityOption[] = [
  ORIGINAL,
  AUTO,
  { id: "30m", label: "30 Mbps 4K", maxBitrate: 30_000, maxHeight: 2160 },
  { id: "15m", label: "15 Mbps 4K", maxBitrate: 15_000, maxHeight: 2160 },
  { id: "20m", label: "20 Mbps 1080p", maxBitrate: 20_000, maxHeight: 1080 },
  { id: "12m", label: "12 Mbps 1080p", maxBitrate: 12_000, maxHeight: 1080 },
  { id: "8m", label: "8 Mbps 1080p", maxBitrate: 8_000, maxHeight: 1080 },
  { id: "4m", label: "4 Mbps 720p", maxBitrate: 4_000, maxHeight: 720 },
  { id: "2m", label: "2 Mbps 480p", maxBitrate: 2_000, maxHeight: 480 },
];

export function qualityById(id: string | null | undefined): QualityOption {
  return QUALITIES.find((q) => q.id === id) ?? ORIGINAL;
}

/**
 * What a quality is called where people read it: Original and Automatic in
 * the app's language, a conversion by its bitrate and lines ("8 Mbps
 * 1080p"), the same in every language.
 */
export function qualityLabel(quality: Pick<QualityOption, "id" | "label">): string {
  if (quality.id === ORIGINAL.id) return t("quality.original", { ns: "player" });
  if (quality.id === AUTO.id) return t("quality.automatic", { ns: "player" });
  return quality.label;
}

type FileSize = Pick<MediaFile, "bitrate" | "width" | "height">;

/**
 * A file's lines, as qualities are named: 2160, 1080, 720 or 480, by its
 * width too (a 1920×800 film is 1080p); `null` when neither is known.
 */
function linesOf(file: FileSize | null): number | null {
  const w = file?.width ?? 0;
  const h = file?.height ?? 0;
  if (w <= 0 && h <= 0) return null;
  return w >= 3800 || h >= 2100 ? 2160 : w >= 1800 || h >= 1000 ? 1080 : w >= 1200 || h >= 700 ? 720 : 480;
}

/** A copy at more than this share of a file's bitrate, at its lines, plays no better than the file. */
const LIGHTER = 0.8;

/**
 * `quality` as it is named for `file`: by the file's own lines when the
 * quality is taller than the file ("8 Mbps 720p" for a 720p file, whose copy
 * is no taller), else as the table names it.
 */
export function namedFor(file: FileSize | null, quality: QualityOption): QualityOption {
  const lines = linesOf(file);
  if (quality.maxBitrate === null || quality.maxHeight === null || lines === null || quality.maxHeight <= lines) {
    return quality;
  }
  return { ...quality, label: `${quality.maxBitrate / 1000} Mbps ${lines >= 2160 ? "4K" : `${lines}p`}` };
}

/**
 * The qualities worth offering for a file (docs/QUALITY.md): the original
 * and Automatic, and each of `table` with fewer lines than the file or at
 * most 80 % of its bitrate (every one when neither is known), named by the
 * file's own lines when taller (`namedFor`), once for each bitrate and
 * lines. Renamed, a 4K quality falls among the file's own: the copies go
 * tallest first, then by bitrate, which is the table's order for a 4K file.
 */
export function qualitiesFor(file: FileSize | null, table: QualityOption[] = QUALITIES): QualityOption[] {
  const kbps = file?.bitrate ? file.bitrate / 1000 : null;
  const height = file?.height ?? null;
  if (kbps == null && height == null) return table;
  const lines = linesOf(file);
  const out: QualityOption[] = [];
  const copies: { quality: QualityOption; lines: number }[] = [];
  const seen = new Set<string>();
  for (const q of table) {
    if (q.maxBitrate === null || q.maxHeight === null) {
      out.push(q);
      continue;
    }
    const fewerLines = height != null && q.maxHeight < height;
    const lighter = kbps != null && q.maxBitrate <= kbps * LIGHTER;
    if (!fewerLines && !lighter) continue;
    const shown = lines === null ? q.maxHeight : Math.min(q.maxHeight, lines);
    const key = `${q.maxBitrate}:${shown}`;
    if (seen.has(key)) continue;
    seen.add(key);
    copies.push({ quality: namedFor(file, q), lines: shown });
  }
  copies.sort((a, b) => b.lines - a.lines || (b.quality.maxBitrate ?? 0) - (a.quality.maxBitrate ?? 0));
  return out.concat(copies.map((c) => c.quality));
}

/**
 * The quality to play: the one picked when this file offers it; the
 * original when it doesn't need that one (an 8 Mbps cap on a 5 Mbps file);
 * Automatic when none was ever picked.
 */
export function chooseQuality(picked: string | null | undefined, offered: QualityOption[]): QualityOption {
  if (!picked || !QUALITIES.some((q) => q.id === picked)) return AUTO;
  return offered.find((q) => q.id === picked) ?? ORIGINAL;
}

/** Whether the quality kept (`pley.quality`) plays Automatic: none was ever picked, or Automatic was. */
export function automaticChosen(picked: string | null | undefined): boolean {
  return chooseQuality(picked, QUALITIES).id === AUTO.id;
}

/**
 * How a quality is asked for: the original lets the server choose (direct
 * play when the browser can); any other quality is a transcode.
 */
export function modeFor(quality: QualityOption): PlaybackMode {
  return quality.maxBitrate === null ? "auto" : "transcode";
}

/**
 * What the server converts the original at (its closest_quality): the
 * file's lines, the highest bitrate up to 1.5 times the file's, else that
 * resolution's lowest. Automatic never goes above it.
 */
export function serverQuality(file: FileSize | null): QualityOption {
  const lines = linesOf(file) ?? 1080;
  const cap = file?.bitrate && file.bitrate > 0 ? (file.bitrate * 1.5) / 1000 : null;
  const tier = QUALITIES.filter((q) => q.maxHeight === lines);
  return tier.find((q) => cap === null || (q.maxBitrate ?? 0) <= cap) ?? tier[tier.length - 1]!;
}

/**
 * The qualities Automatic steps through below the original, highest first:
 * those offered for the file, at no more lines and bitrate than the
 * server's own quality for it, each lower in bitrate than the one above
 * (a 4K file goes 30 Mbps 4K, 15 Mbps 4K, then 12 Mbps 1080p).
 */
export function autoLadder(file: FileSize | null): Rung[] {
  const top = serverQuality(file);
  const out: Rung[] = [];
  let below = Infinity;
  for (const q of qualitiesFor(file)) {
    if (q.maxBitrate === null || q.maxHeight === null) continue;
    if (q.maxHeight > (top.maxHeight ?? 0) || q.maxBitrate > (top.maxBitrate ?? 0) || q.maxBitrate >= below) continue;
    out.push({ id: q.id, label: q.label, kbps: q.maxBitrate, height: q.maxHeight });
    below = q.maxBitrate;
  }
  return out;
}

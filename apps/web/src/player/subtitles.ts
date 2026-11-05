/**
 * How pley draws text subtitles (player/SubtitleOverlay.tsx): white, a little
 * heavy, with a dark outline and no box, as Plex does; in a size, with a
 * background, in a font and as high up as chosen on each device.
 */
import { currentLocale, t } from "@/i18n";

export type SubtitleSize = "small" | "normal" | "large" | "huge";
export type SubtitleBackground = "none" | "box";
export type SubtitleFont = "sans" | "serif" | "mono" | "casual";
export type SubtitlePosition = "bottom" | "raised" | "higher" | "highest";

export interface SubtitleStyle {
  size: SubtitleSize;
  background: SubtitleBackground;
  font: SubtitleFont;
  position: SubtitlePosition;
}

export const DEFAULT_SUBTITLE_STYLE: SubtitleStyle = { size: "normal", background: "none", font: "sans", position: "bottom" };

// The menu names each choice by its id, in the player's words (`settings.sizes.normal`).

/** The sizes offered, with the text's height as a share of the player's. */
export const SUBTITLE_SIZES: { id: SubtitleSize; share: number }[] = [
  { id: "small", share: 3.8 },
  { id: "normal", share: 4.8 },
  { id: "large", share: 6 },
  { id: "huge", share: 7.6 },
];

/** No background, or a dark box. */
export const SUBTITLE_BACKGROUNDS: { id: SubtitleBackground }[] = [{ id: "none" }, { id: "box" }];

/** The fonts offered: the app's own, and the browser's serif, monospace and casual faces. */
export const SUBTITLE_FONTS: { id: SubtitleFont; family: string }[] = [
  { id: "sans", family: "var(--font-sans)" },
  { id: "serif", family: 'Georgia, "Times New Roman", "Noto Serif", serif' },
  { id: "mono", family: 'ui-monospace, "SF Mono", Menlo, Consolas, "Liberation Mono", monospace' },
  { id: "casual", family: '"Comic Sans MS", "Comic Neue", "Chalkboard SE", cursive' },
];

/** How high bottom subtitles sit: raised from the usual place by a share of the player's height (%). */
export const SUBTITLE_POSITIONS: { id: SubtitlePosition; lift: number }[] = [
  { id: "bottom", lift: 0 },
  { id: "raised", lift: 5 },
  { id: "higher", lift: 10 },
  { id: "highest", lift: 16 },
];

/** The space under bottom subtitles, as a share of the player's height (%). */
export const SUBTITLE_BOTTOM = 6;

/** The share of the player's height `position` raises bottom subtitles by (%). */
export function subtitleLift(position: SubtitlePosition): number {
  return SUBTITLE_POSITIONS.find((p) => p.id === position)?.lift ?? 0;
}

/** The CSS font family for `font`. */
export function subtitleFontFamily(font: SubtitleFont): string {
  return SUBTITLE_FONTS.find((f) => f.id === font)?.family ?? SUBTITLE_FONTS[0]!.family;
}

/** The CSS font size for `size`: a share of the player's height, never unreadably small. */
export function subtitleFontSize(size: SubtitleSize): string {
  const share = SUBTITLE_SIZES.find((s) => s.id === size)?.share ?? 4.8;
  return `clamp(15px, ${share}cqh, 96px)`;
}

/** A style read back from storage: anything unknown falls back to the default. */
export function subtitleStyle(stored: unknown): SubtitleStyle {
  const value = (stored ?? {}) as Partial<SubtitleStyle>;
  return {
    size: SUBTITLE_SIZES.some((s) => s.id === value.size) ? (value.size as SubtitleSize) : DEFAULT_SUBTITLE_STYLE.size,
    background: SUBTITLE_BACKGROUNDS.some((b) => b.id === value.background)
      ? (value.background as SubtitleBackground)
      : DEFAULT_SUBTITLE_STYLE.background,
    font: SUBTITLE_FONTS.some((f) => f.id === value.font) ? (value.font as SubtitleFont) : DEFAULT_SUBTITLE_STYLE.font,
    position: SUBTITLE_POSITIONS.some((p) => p.id === value.position)
      ? (value.position as SubtitlePosition)
      : DEFAULT_SUBTITLE_STYLE.position,
  };
}

/** What the overlay needs of a cue (a `VTTCue` in browsers). */
export interface SubtitleCue {
  startTime: number;
  endTime: number;
  text: string;
  /** `"auto"` (the default) or a line number / percentage from the top. */
  line?: number | "auto" | null;
  snapToLines?: boolean;
  getCueAsHTML(): DocumentFragment;
}

/**
 * Whether a cue goes at the top: cues placed in the upper half (signs and
 * notes that a file moves out of the way of the dialogue). Everything else
 * goes at the bottom.
 */
export function cueAtTop(cue: SubtitleCue): boolean {
  const line = cue.line;
  if (line == null || line === "auto") return false;
  // Line numbers count from the top when positive; percentages are of the height.
  return cue.snapToLines === false ? line < 50 : line >= 0 && line < 4;
}

/** `"01:02:03.456"` (or `"02:03,456"`) → seconds. */
function cueSeconds(time: string): number {
  return time
    .replace(",", ".")
    .split(":")
    .reduce((total, part) => total * 60 + Number(part), 0);
}

const TIMING = /^\s*((?:\d+:)?\d{1,2}:\d{2}[.,]\d{1,3})\s+-->\s+((?:\d+:)?\d{1,2}:\d{2}[.,]\d{1,3})(.*)$/;

/**
 * The cues of a WebVTT file (`VTTCue`s in browsers; `Cue` stands in for
 * them in tests). Only what pley uses is read: the times, the text and the
 * line a cue is placed on.
 */
export function parseVtt<C extends Pick<VTTCue, "line" | "snapToLines">>(
  text: string,
  Cue: new (start: number, end: number, text: string) => C,
): C[] {
  const cues: C[] = [];
  for (const block of text.replace(/\r\n?/g, "\n").split(/\n{2,}/)) {
    const lines = block.split("\n");
    const at = lines.findIndex((l) => l.includes("-->"));
    const timing = at >= 0 ? TIMING.exec(lines[at] ?? "") : null;
    if (!timing) continue;
    const body = lines.slice(at + 1).join("\n").trim();
    if (!body) continue;
    const [, from = "", to = "", settings = ""] = timing;
    const start = cueSeconds(from);
    const end = cueSeconds(to);
    if (!(end > start)) continue;
    const cue = new Cue(start, end, body);
    const line = /(?:^|\s)line:(-?\d+(?:\.\d+)?)(%)?/.exec(settings);
    if (line) {
      cue.snapToLines = !line[2];
      cue.line = Number(line[1] ?? "");
    }
    cues.push(cue);
  }
  return cues;
}

/** How far subtitles can be moved, either way (seconds), and the step. Positive: later. */
export const SUBTITLE_OFFSET_MAX = 10;
export const SUBTITLE_OFFSET_STEP = 0.1;
const STEPS_PER_SECOND = 10;

/** `offset` in whole steps, within the limits. */
export function clampSubtitleOffset(offset: number): number {
  if (!Number.isFinite(offset)) return 0;
  const steps = Math.round(offset * STEPS_PER_SECOND);
  const limit = SUBTITLE_OFFSET_MAX * STEPS_PER_SECOND;
  // Divided rather than multiplied by the step, so 3 steps are 0.3, not 0.30000000000000004; `+ 0` turns -0 into 0.
  return Math.max(-limit, Math.min(limit, steps)) / STEPS_PER_SECOND + 0;
}

/** `"+0.5 s"`, `"−1.2 s"`, `"0.0 s"` (`"+0,5 s"` in French). */
export function formatSubtitleOffset(offset: number): string {
  const value = clampSubtitleOffset(offset);
  const sign = value > 0 ? "+" : value < 0 ? "−" : "";
  const amount = Math.abs(value).toLocaleString(currentLocale(), { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  return t("settings.offset", { ns: "player", value: `${sign}${amount}` });
}

/**
 * Offsets by profile and file, on this device: `{ "u1.f12": 0.5 }`, the
 * latest last. Each profile has its own, as subtitles one person finds late
 * may suit another. Offsets kept before, by file alone (`"f12"`), go to the
 * first profile that plays the file.
 */
const OFFSETS_KEY = "pley.subtitleOffsets";
/** Files whose offsets are kept: the oldest go first. */
const OFFSETS_KEPT = 300;

function readOffsets(): Record<string, unknown> {
  try {
    const value: unknown = JSON.parse(window.localStorage.getItem(OFFSETS_KEY) ?? "{}");
    return value && typeof value === "object" && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
  } catch {
    return {};
  }
}

function writeOffsets(offsets: Record<string, unknown>) {
  try {
    window.localStorage.setItem(OFFSETS_KEY, JSON.stringify(Object.fromEntries(Object.entries(offsets).slice(-OFFSETS_KEPT))));
  } catch {
    // storage full or unavailable: it lasts as long as the player
  }
}

/** The subtitle offset profile `userId` keeps for a file (0 when none). */
export function storedSubtitleOffset(userId: number, fileId: number | null | undefined): number {
  if (fileId == null) return 0;
  const offsets = readOffsets();
  const value = offsets[`u${userId}.f${fileId}`];
  if (typeof value === "number") return clampSubtitleOffset(value);
  const before = offsets[`f${fileId}`];
  if (typeof before !== "number") return 0;
  // Kept before offsets were per profile: this profile's from now on.
  delete offsets[`f${fileId}`];
  const moved = clampSubtitleOffset(before);
  if (moved !== 0) offsets[`u${userId}.f${fileId}`] = moved;
  writeOffsets(offsets);
  return moved;
}

/** Remembers profile `userId`'s subtitle offset for a file on this device; 0 forgets it. */
export function storeSubtitleOffset(userId: number, fileId: number | null | undefined, offset: number): void {
  if (fileId == null) return;
  const key = `u${userId}.f${fileId}`;
  const offsets = readOffsets();
  delete offsets[key];
  delete offsets[`f${fileId}`];
  const value = clampSubtitleOffset(offset);
  if (value !== 0) offsets[key] = value;
  writeOffsets(offsets);
}

/**
 * When the lines showing next change after `seconds`: the first start or
 * end still to come (`null`: none).
 */
export function nextCueChange(cues: readonly Pick<SubtitleCue, "startTime" | "endTime">[], seconds: number): number | null {
  let next: number | null = null;
  for (const c of cues) {
    const at = c.startTime > seconds ? c.startTime : c.endTime > seconds ? c.endTime : null;
    if (at !== null && (next === null || at < next)) next = at;
  }
  return next;
}

/**
 * The cues showing at `seconds`, in order. (Not the text track's
 * `activeCues`: after a seek browsers keep those as they were until the
 * video plays again, which can be seconds while it loads.)
 */
export function cuesAt<C extends Pick<SubtitleCue, "startTime" | "endTime">>(cues: readonly C[], seconds: number): C[] {
  return cues.filter((c) => c.startTime <= seconds && seconds < c.endTime).sort((a, b) => a.startTime - b.startTime);
}

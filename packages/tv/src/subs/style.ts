/**
 * How text subtitles look on the TV (Settings → Display), past their size
 * and background: a font, and how high above the bottom they sit (the
 * web's player/subtitles.ts offers the same). Fonts are the TV's own: one
 * is offered only when the TV has a face for it that differs from the
 * app's (`fontsHere`).
 */
import { t } from "../i18n";

export type SubtitleFont = "default" | "serif" | "mono" | "casual";
export type SubtitlePosition = "bottom" | "raised" | "higher" | "highest";

/** A font as Settings offers it; its `label` is read in the language shown. */
function font(id: SubtitleFont, family: string): { id: SubtitleFont; readonly label: string; family: string } {
  return {
    id,
    get label() {
      return t(`player.subtitleStyle.fonts.${id}`);
    },
    family,
  };
}

/** A position as Settings offers it; its `label` is read in the language shown. */
function position(id: SubtitlePosition, lift: number): { id: SubtitlePosition; readonly label: string; lift: number } {
  return {
    id,
    get label() {
      return t(`player.subtitleStyle.positions.${id}`);
    },
    lift,
  };
}

export const SUBTITLE_FONTS = [
  font("default", '"Roboto", "SamsungOne", "LG Smart UI", sans-serif'),
  font("serif", '"Roboto Serif", "Noto Serif", "DejaVu Serif", "Tizen Serif", Georgia, serif'),
  font("mono", '"Roboto Mono", "Noto Sans Mono", "DejaVu Sans Mono", monospace'),
  font("casual", '"Comic Sans MS", "Comic Neue", "Coming Soon", cursive'),
];

/** Raised from the usual place by this share of the screen's height (%), as the web's. */
export const SUBTITLE_POSITIONS = [position("bottom", 0), position("raised", 5), position("higher", 10), position("highest", 16)];

export function fontFamily(font: SubtitleFont): string {
  return (SUBTITLE_FONTS.filter((f) => f.id === font)[0] ?? SUBTITLE_FONTS[0]!).family;
}

/** Canvas px (of 540) `position` raises bottom subtitles by. */
export function liftPx(position: SubtitlePosition): number {
  const lift = SUBTITLE_POSITIONS.filter((p) => p.id === position)[0]?.lift ?? 0;
  return Math.round((540 * lift) / 100);
}

const SAMPLE = "The quick brown fox jumps over the lazy dog 0123456789 WMwmil";

/** Widths of `SAMPLE` in a font stack; `null` when the engine can't measure. */
export type Measure = (family: string) => number | null;

function canvasMeasure(): Measure {
  let context: CanvasRenderingContext2D | null = null;
  try {
    context = document.createElement("canvas").getContext("2d");
  } catch {
    context = null;
  }
  return (family) => {
    if (!context) return null;
    context.font = `48px ${family}`;
    return context.measureText(SAMPLE).width;
  };
}

let found: SubtitleFont[] | null = null;

/**
 * The fonts this TV draws differently from the app's own (a stack the TV
 * has no face for falls back to it, or to the same face for every
 * generic): those are offered. The default always is. Measured once, on
 * a canvas, unless a test measures.
 */
export function fontsHere(measure?: Measure): SubtitleFont[] {
  if (!measure && found) return found;
  const m = measure ?? canvasMeasure();
  const base = m(SUBTITLE_FONTS[0]!.family);
  const out: SubtitleFont[] = ["default"];
  for (const f of SUBTITLE_FONTS) {
    if (f.id === "default") continue;
    const width = m(f.family);
    if (base !== null && width !== null && Math.abs(width - base) > 0.5) out.push(f.id);
  }
  if (!measure) found = out;
  return out;
}

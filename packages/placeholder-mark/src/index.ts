import marks from "../marks.json";

/** A glyph cut out of the mark's front poster, by the kind of library a title is in. */
export type MarkGlyph = keyof typeof marks.glyphs;

/** One poster of the mark on its grid: filled white at `alpha`, even-odd when a glyph is cut out of it. */
export interface MarkLayer {
  path: string;
  alpha: number;
  evenOdd: boolean;
}

/** The mark's side of a box: `share` of its shorter side, at most `max`. */
export const MARK = { grid: marks.grid, share: marks.share, shareOverTitle: marks.shareOverTitle, max: marks.max, gap: marks.gap };

const kinds: Record<string, string | undefined> = marks.kinds;

/** The glyph for an item of `kind` (the API's `kind`): its library's, or play. */
export function markGlyph(kind?: string | null): MarkGlyph {
  return ((kind && Object.hasOwn(kinds, kind) && kinds[kind]) || marks.default) as MarkGlyph;
}

const layers = new Map<MarkGlyph, MarkLayer[]>();

/** The mark's posters back to front, `glyph` cut out of the front one. */
export function markLayers(glyph: MarkGlyph): MarkLayer[] {
  let out = layers.get(glyph);
  if (!out) {
    const last = marks.posters.length - 1;
    out = marks.posters.map((p, i) =>
      i === last ? { path: p.path + marks.glyphs[glyph], alpha: p.alpha, evenOdd: true } : { path: p.path, alpha: p.alpha, evenOdd: false },
    );
    layers.set(glyph, out);
  }
  return out;
}

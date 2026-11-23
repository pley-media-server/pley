import placeholder from "../placeholder.json";

const { gradient } = placeholder;

/**
 * The hue of `seed`'s gradient, 0 to 359: `h = h * 31 + unit` over its
 * UTF-16 code units, kept to an unsigned 32-bit number, then `h % 360`.
 */
export function placeholderHue(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return h % 360;
}

/** The gradient's two ends for `seed`, as CSS's hsl() takes them: hue in degrees, saturation and lightness in %. */
export function placeholderStops(seed: string): { from: [number, number, number]; to: [number, number, number] } {
  const hue = placeholderHue(seed);
  const percent = (n: number) => Math.round(n * 100);
  return {
    from: [hue, percent(gradient.from.saturation), percent(gradient.from.lightness)],
    to: [(hue + gradient.to.hueShift) % 360, percent(gradient.to.saturation), percent(gradient.to.lightness)],
  };
}

/**
 * The placeholder of a picture that hasn't come, for `seed` (its title): a
 * CSS gradient. hsl() takes commas, which every browser the apps run in
 * reads, the TVs' older ones too.
 */
export function placeholderGradient(seed: string): string {
  const { from, to } = placeholderStops(seed);
  const hsl = ([h, s, l]: [number, number, number]) => `hsl(${h}, ${s}%, ${l}%)`;
  return `linear-gradient(${gradient.angle}deg, ${hsl(from)}, ${hsl(to)})`;
}

/** A glyph cut out of the mark's front poster, by the kind of library a title is in. */
export type MarkGlyph = keyof typeof placeholder.glyphs;

/** One poster of the mark on its grid: filled white at `alpha`, even-odd when a glyph is cut out of it. */
export interface MarkLayer {
  path: string;
  alpha: number;
  evenOdd: boolean;
}

/** The mark's side of a box: `share` of its shorter side, at most `max`. */
export const MARK = {
  grid: placeholder.grid,
  share: placeholder.share,
  shareOverTitle: placeholder.shareOverTitle,
  max: placeholder.max,
  gap: placeholder.gap,
};

const kinds: Record<string, string | undefined> = placeholder.kinds;

/** The glyph for an item of `kind` (the API's `kind`): its library's, or play. */
export function markGlyph(kind?: string | null): MarkGlyph {
  return ((kind && Object.hasOwn(kinds, kind) && kinds[kind]) || placeholder.default) as MarkGlyph;
}

const layers = new Map<MarkGlyph, MarkLayer[]>();

/** The mark's posters back to front, `glyph` cut out of the front one. */
export function markLayers(glyph: MarkGlyph): MarkLayer[] {
  let out = layers.get(glyph);
  if (!out) {
    const last = placeholder.posters.length - 1;
    out = placeholder.posters.map((p, i) =>
      i === last ? { path: p.path + placeholder.glyphs[glyph], alpha: p.alpha, evenOdd: true } : { path: p.path, alpha: p.alpha, evenOdd: false },
    );
    layers.set(glyph, out);
  }
  return out;
}

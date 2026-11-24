import { describe, expect, it } from "vitest";
import marks from "../placeholder.json";
import { markGlyph, markLayers, placeholderGradient, placeholderHue, placeholderStops } from "../src/index";

/** CSS's hsl() as #rrggbb: hue in degrees, saturation and lightness in %. */
function hex([h, s, l]: [number, number, number]): string {
  const [sat, light] = [s / 100, l / 100];
  const c = (1 - Math.abs(2 * light - 1)) * sat;
  const sector = (h % 360) / 60;
  const x = c * (1 - Math.abs((sector % 2) - 1));
  const rgb = sector < 1 ? [c, x, 0] : sector < 2 ? [x, c, 0] : sector < 3 ? [0, c, x] : sector < 4 ? [0, x, c] : sector < 5 ? [x, 0, c] : [c, 0, x];
  return "#" + rgb.map((v) => Math.round((v + light - c / 2) * 255).toString(16).padStart(2, "0")).join("");
}

describe("the placeholder's gradient", () => {
  it("takes each example's hue from its title", () => {
    for (const e of marks.examples) expect(placeholderHue(e.seed), JSON.stringify(e.seed)).toBe(e.hue);
  });

  it("has each example's colors at its two ends", () => {
    // The examples are the rule worked out: a change to one without the other fails here.
    for (const e of marks.examples) {
      const { from, to } = placeholderStops(e.seed);
      expect([hex(from), hex(to)], JSON.stringify(e.seed)).toEqual([e.from, e.to]);
    }
  });

  it("counts a character beyond the first plane as its two UTF-16 units", () => {
    expect(marks.examples.some((e) => [...e.seed].length < e.seed.length)).toBe(true);
  });

  it("is CSS with commas, which the TVs' older browsers read", () => {
    expect(placeholderGradient("")).toBe("linear-gradient(135deg, hsl(0, 30%, 22%), hsl(40, 35%, 12%))");
  });
});

/** A path's commands and their numbers; anything but absolute M, L, C and Z fails. */
function commands(path: string): { cmd: string; args: number[] }[] {
  const out: { cmd: string; args: number[] }[] = [];
  const re = /([MLCZ])([^MLCZ]*)/g;
  let rest = path;
  for (const m of path.matchAll(re)) {
    const args = m[2]!.trim() === "" ? [] : m[2]!.trim().split(/[\s,]+/).map(Number);
    out.push({ cmd: m[1]!, args });
    rest = rest.replace(m[0], "");
  }
  expect(rest, `${path} has commands other than M, L, C and Z`).toBe("");
  return out;
}

const ARGS: Record<string, number> = { M: 2, L: 2, C: 6, Z: 0 };

/** Every point of a path (end and control points), checking each command's numbers. */
function points(path: string): [number, number][] {
  const pts: [number, number][] = [];
  for (const { cmd, args } of commands(path)) {
    expect(args.length, `${cmd} in ${path}`).toBe(ARGS[cmd]);
    for (const n of args) expect(Number.isFinite(n), `${cmd} in ${path}`).toBe(true);
    for (let i = 0; i < args.length; i += 2) pts.push([args[i]!, args[i + 1]!]);
  }
  return pts;
}

describe("the placeholder mark", () => {
  it("is three posters, each lighter than the one behind", () => {
    expect(marks.posters.map((p) => p.alpha)).toEqual([0.16, 0.24, 0.36]);
    for (const p of marks.posters) points(p.path);
  });

  it("cuts each glyph out of its front poster, 3 units inside it", () => {
    // The front poster spans 12 to 34 across and 15.5 to 48.5 down.
    for (const [name, path] of Object.entries(marks.glyphs)) {
      expect(path.startsWith("M") && path.endsWith("Z"), name).toBe(true);
      for (const [x, y] of points(path)) {
        expect(x, `${name} x`).toBeGreaterThanOrEqual(15);
        expect(x, `${name} x`).toBeLessThanOrEqual(31);
        expect(y, `${name} y`).toBeGreaterThanOrEqual(18.5);
        expect(y, `${name} y`).toBeLessThanOrEqual(45.5);
      }
    }
  });

  it("gives each kind of item its library's glyph, and play to the others", () => {
    for (const glyph of Object.values(marks.kinds)) expect(Object.keys(marks.glyphs)).toContain(glyph);
    expect(Object.keys(marks.glyphs)).toContain(marks.default);
    expect(markGlyph("movie")).toBe("movies");
    expect(markGlyph("episode")).toBe("shows");
    expect(markGlyph("album")).toBe("music");
    expect(markGlyph("part")).toBe("audiobooks");
    expect(markGlyph("extra")).toBe("play");
    expect(markGlyph(undefined)).toBe("play");
    expect(markGlyph("toString")).toBe("play");
  });

  it("draws the glyph as part of the front poster, filled even-odd", () => {
    const layers = markLayers("music");
    expect(layers.map((l) => l.evenOdd)).toEqual([false, false, true]);
    expect(layers[2]!.path).toBe(marks.posters[2]!.path + marks.glyphs.music);
    expect(markLayers("music")).toBe(layers);
  });
});

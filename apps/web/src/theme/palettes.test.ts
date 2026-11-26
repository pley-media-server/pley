import { describe, expect, it } from "vitest";
import css from "../index.css?raw";
import { ACCENTLESS, ACCENTS, THEMES, type Theme } from ".";

/**
 * Each `[data-theme="…"]` block of index.css: its custom properties by name,
 * as written. A color a theme reads from the accent picked is
 * `var(--accent-dark, #own)`.
 */
function written(): Map<string, Record<string, string>> {
  const found = new Map<string, Record<string, string>>();
  for (const [, selector, body] of css.matchAll(/([^{}]*\[data-theme="[a-z]+"\][^{}]*)\{([^{}]*)\}/g)) {
    const values = Object.fromEntries([...body!.matchAll(/(--[\w-]+):\s*([^;]+);/g)].map(([, name, value]) => [name!, value!.trim()]));
    // Each selector of the block that is a theme's alone (not "[data-theme] [data-dark]").
    for (const [, theme] of selector!.matchAll(/^\s*\[data-theme="([a-z]+)"\]\s*,?\s*$/gm)) found.set(theme!, values);
  }
  return found;
}

/** Each `[data-accent="…"]` block of index.css: what it sets, by name. */
function accentBlocks(): Map<string, Record<string, string>> {
  const found = new Map<string, Record<string, string>>();
  for (const [, accent, body] of css.matchAll(/\[data-accent="([a-z]+)"\]\s*\{([^{}]*)\}/g)) {
    found.set(accent!, Object.fromEntries([...body!.matchAll(/(--[\w-]+):\s*([^;]+);/g)].map(([, name, value]) => [name!, value!.trim()])));
  }
  return found;
}

const READ = /^var\((--[\w-]+),\s*(#[0-9a-f]{6})\)$/;

/** A theme's colors with an accent's variables over it (none: the theme's own). */
function colors(theme: Record<string, string>, accent: Record<string, string> = {}): Record<string, string> {
  return Object.fromEntries(
    Object.entries(theme).map(([name, value]) => {
      const read = READ.exec(value);
      return [name, read ? (accent[read[1]!] ?? read[2]!) : value];
    }),
  );
}

/** Each theme's own colors. */
function palettes(): Map<string, Record<string, string>> {
  return new Map([...written()].map(([theme, values]) => [theme, colors(values)]));
}

/** The colors @theme declares, Midnight's. */
function declared(): Record<string, string> {
  const block = /@theme\s*\{([\s\S]*?)\n\s*@keyframes/.exec(css)?.[1] ?? "";
  return Object.fromEntries([...block.matchAll(/(--color-[\w-]+):\s*(#[0-9a-f]{6});/g)].map(([, name, value]) => [name!, value!]));
}

const channels = (hex: string) => [1, 3, 5].map((at) => parseInt(hex.slice(at, at + 2), 16) / 255);
const linear = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);

function luminance(hex: string): number {
  const [r, g, b] = channels(hex).map(linear);
  return 0.2126 * r! + 0.7152 * g! + 0.0722 * b!;
}

/** WCAG's contrast ratio of two colors. */
function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi! + 0.05) / (lo! + 0.05);
}

/**
 * How far apart two colors' hues are, as their distance in OKLab's a and b
 * (lightness left out): an accent this close to a state color reads as it.
 */
function hueApart(x: string, y: string): number {
  const ab = (hex: string) => {
    const [r, g, b] = channels(hex).map(linear);
    const l = Math.cbrt(0.4122214708 * r! + 0.5363325363 * g! + 0.0514459929 * b!);
    const m = Math.cbrt(0.2119034982 * r! + 0.6806995451 * g! + 0.1073969566 * b!);
    const s = Math.cbrt(0.0883024619 * r! + 0.2817188376 * g! + 0.6299787005 * b!);
    return [1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s, 0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s];
  };
  const [a1, b1] = ab(x);
  const [a2, b2] = ab(y);
  return Math.hypot(a1! - a2!, b1! - b2!);
}

/** `color` laid at `alpha` over `under`, as `bg-success/15` draws over a card. */
function over(color: string, alpha: number, under: string): string {
  const channel = (hex: string, at: number) => parseInt(hex.slice(at, at + 2), 16);
  return `#${[1, 3, 5]
    .map((at) => Math.round(channel(color, at) * alpha + channel(under, at) * (1 - alpha)).toString(16).padStart(2, "0"))
    .join("")}`;
}

// [words, ground, least contrast]: WCAG AA for text (4.5), AAA for
// the main text (7).
const pairs: [string, string, number][] = [
  ["fg", "bg", 7],
  ["fg", "surface", 7],
  ["fg", "surface-2", 7],
  ["fg", "surface-3", 4.5],
  ["muted", "bg", 4.5],
  ["muted", "surface", 4.5],
  ["muted", "surface-2", 4.5],
  ["muted", "surface-3", 4.5],
  // Hints, inputs' placeholders among them.
  ["subtle", "bg", 4.5],
  ["subtle", "surface", 4.5],
  ["subtle", "surface-2", 4.5],
  ["accent", "bg", 4.5],
  ["accent", "surface", 4.5],
  ["accent", "surface-2", 4.5],
  ["accent", "surface-3", 4.5],
  ["accent-fg", "accent", 4.5],
  ["accent-fg", "accent-hover", 4.5],
  ["success", "surface", 4.5],
  ["danger", "surface", 4.5],
  ["info", "surface", 4.5],
  ["warning", "surface", 4.5],
  // Words on a fill: the attention badge, a danger button or badge.
  ["bg", "warning", 4.5],
  ["danger-fg", "danger", 4.5],
];

/** Holds a palette to every contrast the app needs of it. */
function expectReadable(colors: Record<string, string>) {
  const color = (name: string) => colors[`--color-${name}`]!;
  for (const [words, ground, least] of pairs) {
    expect(contrast(color(words), color(ground)), `${words} on ${ground}`).toBeGreaterThanOrEqual(least);
  }
  // A badge in a state's color on its own tint, up to the 20% the app uses.
  for (const state of ["accent", "success", "danger", "warning", "info"]) {
    const tint = over(color(state), 0.2, color("surface"));
    expect(contrast(color(state), tint), `${state} on its tint`).toBeGreaterThanOrEqual(4.5);
  }
  // The danger button, its fill at 90% over the card.
  expect(contrast(color("danger-fg"), over(color("danger"), 0.9, color("surface"))), "danger button").toBeGreaterThanOrEqual(4.5);
}

describe("each theme's colors", () => {
  const all = palettes();

  it("are every color @theme declares, Midnight's being @theme's own", () => {
    const names = Object.keys(declared());
    expect(names.length).toBeGreaterThan(20);
    expect([...all.keys()].sort()).toEqual([...THEMES].sort());
    for (const theme of THEMES) {
      expect(Object.keys(all.get(theme)!).filter((name) => name.startsWith("--color-")).sort(), theme).toEqual(names.sort());
    }
    const midnight = all.get("midnight")!;
    for (const name of names) expect(midnight[name], name).toBe(declared()[name]);
  });

  it("each set a backdrop and a sidebar picture, none or their own, so a preview in one doesn't show the page's", () => {
    for (const theme of THEMES) {
      expect(all.get(theme)!["--glow"], theme).toBeTruthy();
      expect(all.get(theme)!["--sidebar"], theme).toBeTruthy();
    }
  });

  it("keep the players dark in the light themes, in their dark twin's colors", () => {
    const twins = { midlight: "midnight", starlight: "moonlight", greenlight: "nightshade", sunshine: "midnight" };
    for (const [light, dark] of Object.entries(twins)) {
      const block = [...css.matchAll(/([^{}]*)\{[^{}]*\}/g)].find(([, selector]) => selector!.includes(`[data-theme="${dark}"],`));
      expect(block?.[1], light).toContain(`[data-theme="${light}"] [data-dark]`);
    }
  });

  it.each(THEMES)("are readable in %s", (theme: Theme) => {
    const colors = all.get(theme)!;
    expectReadable(colors);
    const color = (name: string) => colors[`--color-${name}`]!;
    // The logo on its tile: the front poster plain at one end of its
    // gradient, the others fading into the tile behind it.
    const onTile = (name: string) => contrast(color(name), color("brand-tile"));
    expect(Math.max(onTile("brand-light"), onTile("brand")), "logo, front poster").toBeGreaterThanOrEqual(4.5);
    expect(onTile("brand-deep"), "logo, middle poster").toBeGreaterThanOrEqual(2);
    expect(onTile("brand-far"), "logo, back poster").toBeGreaterThanOrEqual(1.3);
  });
});

describe("each accent", () => {
  const themes = written();
  const accents = accentBlocks();
  const STATES = ["success", "danger", "warning", "info"];
  // Nightshade's mint and its teal success, the closest a theme puts its
  // accent to a state on purpose.
  const APART = 0.035;

  it("is one src/theme offers, with a color, its hover and its words for the dark themes and the light ones", () => {
    expect([...accents.keys()].filter((accent) => accent !== "own").sort()).toEqual([...ACCENTS].sort());
    for (const accent of ACCENTS) {
      for (const kind of ["dark", "light"]) {
        for (const part of ["", "-hover", "-fg"]) expect(accents.get(accent)![`--accent-${kind}${part}`], `${accent} ${kind}${part}`).toMatch(/^#[0-9a-f]{6}$/);
      }
    }
  });

  it("is read by every theme but Blacklight, which keeps its white", () => {
    for (const theme of THEMES) {
      const reads = ["--color-accent", "--color-accent-hover", "--color-accent-fg"].every((name) => READ.test(themes.get(theme)![name]!));
      expect(reads, theme).toBe(theme !== ACCENTLESS);
    }
  });

  it("leaves the theme's own where a swatch shows it inside a page with another", () => {
    const own = accents.get("own")!;
    const read = new Set([...themes.values()].flatMap((values) => Object.values(values).map((value) => READ.exec(value)?.[1]).filter(Boolean)));
    expect([...read].sort()).toEqual(Object.keys(own).sort());
    for (const value of Object.values(own)) expect(value).toBe("initial");
  });

  const combinations = THEMES.filter((theme) => theme !== ACCENTLESS).flatMap((theme) => ACCENTS.map((accent) => [theme, accent] as const));

  it.each(combinations)("keeps %s readable in %s, apart from its states", (theme, accent) => {
    const set = accents.get(accent)!;
    const palette = colors(themes.get(theme)!, set);
    expectReadable(palette);
    const color = (name: string) => palette[`--color-${name}`]!;
    for (const state of STATES) expect(hueApart(color("accent"), color(state)), `accent and ${state}`).toBeGreaterThanOrEqual(APART);
    // A state the accent moves stays apart from the others too.
    const moved = STATES.filter((state) => Object.keys(set).some((name) => name.startsWith(`--${state}-`)));
    for (const state of moved) {
      for (const other of STATES.filter((s) => s !== state)) expect(hueApart(color(state), color(other)), `${state} and ${other}`).toBeGreaterThanOrEqual(APART);
    }
  });
});

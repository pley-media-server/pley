/**
 * Pley's themes and accents as every app draws them (docs/THEMES.md, "In
 * every app"), read from index.css, where the web app keeps them. It is
 * written to packages/themes/themes.json (`bun run themes` in apps/web),
 * which the other apps' tests hold their colors to; spec.test.ts keeps that
 * file as index.css has it.
 */

/** The colors of a theme, by token name without `--color-` (`bg`, `accent`, `brand-tile`). */
export type Colors = Record<string, string>;

export interface ThemeSpec {
  id: string;
  /** Dark or light, as its `color-scheme`; which of an accent's two sets it takes. */
  kind: "dark" | "light";
  /** A light theme's players draw in this dark theme's colors; null for a dark theme. */
  twin: string | null;
  /** Its own colors, with no accent picked. */
  colors: Colors;
  /** The tokens an accent picked replaces, from its set for this theme's kind. */
  follows: string[];
}

export interface AccentSpec {
  id: string;
  /** For the dark themes and the light ones: the colors it gives the tokens a theme follows. */
  dark: Colors;
  light: Colors;
}

export interface Spec {
  themes: ThemeSpec[];
  accents: AccentSpec[];
}

const properties = (body: string) => Object.fromEntries([...body.matchAll(/(--[\w-]+|color-scheme):\s*([^;]+);/g)].map(([, name, value]) => [name!, value!.trim()]));

/** The token an accent's variable gives (`--accent-dark-logo` is `brand`), or null for none. */
function token(variable: string, kind: string): string | null {
  const accent = new RegExp(`^--accent-${kind}(-hover|-fg|-logo-light|-logo|-logo-deep|-logo-far)?$`).exec(variable);
  if (accent) {
    const part = accent[1] ?? "";
    return { "": "accent", "-hover": "accent-hover", "-fg": "accent-fg", "-logo-light": "brand-light", "-logo": "brand", "-logo-deep": "brand-deep", "-logo-far": "brand-far" }[part]!;
  }
  const state = new RegExp(`^--(success|danger|warning|info)-${kind}$`).exec(variable);
  return state ? state[1]! : null;
}

const READ = /^var\((--[\w-]+),\s*(#[0-9a-f]{6})\)$/;

/** The themes, in the picker's order, and the accents, from index.css's text. */
export function themeSpec(css: string, order: readonly string[], accentOrder: readonly string[]): Spec {
  const blocks = [...css.matchAll(/([^{}]*)\{([^{}]*)\}/g)].map(([, selector, body]) => ({ selector: selector!, body: body! }));
  const themes = order.map((id): ThemeSpec => {
    const own = blocks.find(({ selector }) => new RegExp(`^\\s*\\[data-theme="${id}"\\]\\s*,?\\s*$`, "m").test(selector));
    if (!own) throw new Error(`no block for ${id}`);
    const values = properties(own.body);
    const kind = values["color-scheme"] === "light" ? "light" : "dark";
    const colors: Colors = {};
    const follows: string[] = [];
    for (const [name, value] of Object.entries(values)) {
      if (!name.startsWith("--color-")) continue;
      const name_ = name.slice("--color-".length);
      const read = READ.exec(value);
      colors[name_] = read ? read[2]! : value;
      if (read) {
        if (token(read[1]!, kind) !== name_) throw new Error(`${id}'s ${name_} reads ${read[1]}`);
        follows.push(name_);
      }
    }
    const twin = blocks.find(({ selector }) => selector.includes(`[data-theme="${id}"] [data-dark]`));
    const twinId = twin ? /\[data-theme="([a-z]+)"\]\s*,/.exec(twin.selector)?.[1] ?? null : null;
    return { id, kind, twin: kind === "light" ? twinId : null, colors, follows };
  });
  const accents = accentOrder.map((id): AccentSpec => {
    const block = blocks.find(({ selector }) => selector.trim().endsWith(`[data-accent="${id}"]`));
    if (!block) throw new Error(`no block for accent ${id}`);
    const set = (kind: "dark" | "light") => {
      const colors: Colors = {};
      for (const [variable, value] of Object.entries(properties(block.body))) {
        const name = token(variable, kind);
        if (name) colors[name] = value;
      }
      return colors;
    };
    return { id, dark: set("dark"), light: set("light") };
  });
  return { themes, accents };
}

/** A theme's color for a token under an accent (null for the theme's own), as every app draws it. */
export function colorOf(spec: Spec, theme: string, accent: string | null, name: string): string {
  const t = spec.themes.find(({ id }) => id === theme)!;
  const a = accent ? spec.accents.find(({ id }) => id === accent) : undefined;
  return (t.follows.includes(name) && a?.[t.kind][name]) || t.colors[name]!;
}

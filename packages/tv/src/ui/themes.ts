/**
 * Pley's themes and accents on the TV (docs/THEMES.md, "In every app"):
 * the colors are read from packages/themes/themes.json, the file every app
 * draws them from, and given to the app's own names for them (theme.css's
 * `--bg`, `--surface3`, `--on-accent`…) as one stylesheet. A pick writes that
 * stylesheet again: every color changes in the same frame, and nothing is
 * looked up while the remote moves.
 */
import spec from "../../../themes/themes.json";

/** The themes, in the order Appearance shows them. */
export const THEMES = ["midnight", "midday", "twilight", "starlight", "moonlight", "nightshade", "greenlight", "sunshine", "blacklight"] as const;
export type ThemeId = (typeof THEMES)[number];

/** The accents, after the theme's own (none picked). */
export const ACCENTS = ["sky", "violet", "miami", "gold", "mint", "amber"] as const;
export type AccentId = (typeof ACCENTS)[number];

interface ThemeSpec {
  id: string;
  kind: "dark" | "light";
  twin: string | null;
  colors: Record<string, string>;
  follows: string[];
}

interface AccentSpec {
  id: string;
  dark: Record<string, string>;
  light: Record<string, string>;
}

// Read as the shape the file promises (apps/web/src/theme/spec.ts); the test holds this app to it.
const SPEC = spec as unknown as { themes: ThemeSpec[]; accents: AccentSpec[] };

/**
 * The app's colors and the token each is (the Android TV app's names on
 * the left). Outlines of controls (`--control`) are the theme's hint color,
 * as they were Midnight's nearest gray.
 */
export const ROLES: readonly (readonly [string, string])[] = [
  ["bg", "bg"],
  ["surface", "surface"],
  ["surface2", "surface-2"],
  ["surface3", "surface-3"],
  ["surface4", "border"],
  ["outline", "border-strong"],
  ["control", "subtle"],
  ["fg", "fg"],
  ["muted", "muted"],
  ["subtle", "subtle"],
  ["accent", "accent"],
  ["accent-bright", "accent-hover"],
  ["on-accent", "accent-fg"],
  ["danger", "danger"],
  ["danger-fg", "danger-fg"],
  ["success", "success"],
  ["info", "info"],
  ["warning", "warning"],
  ["brand-light", "brand-light"],
  ["brand", "brand"],
  ["brand-deep", "brand-deep"],
  ["brand-far", "brand-far"],
  ["brand-tile", "brand-tile"],
];

/** Colors drawn see-through (`rgba(var(--fg-rgb), 0.12)`): these also come as their red, green and blue. */
const SEE_THROUGH = ["bg", "surface", "surface2", "surface3", "fg", "muted", "accent", "on-accent", "danger", "success", "info", "warning", "brand-deep"];

/** How strongly the theme's shade dims what is under a panel, the open rail, Now Playing's screen saver. */
const SHADES = [50, 60, 70];

/**
 * Sunshine's rainbow, down the open rail: the web's sidebar (`--sidebar` in
 * apps/web/src/index.css), which the test holds it to.
 */
export const SUNSHINE_SIDEBAR =
  "linear-gradient(to bottom, rgba(255, 99, 146, 0.22), rgba(255, 158, 64, 0.22) 20%, rgba(255, 214, 0, 0.24) 40%, rgba(74, 214, 140, 0.22) 60%, rgba(56, 182, 255, 0.22) 80%, rgba(150, 100, 255, 0.22))";

/** The theme kept for an id, Midnight for one this app doesn't know. */
export function themeId(id: string | null | undefined): ThemeId {
  return THEMES.find((t) => t === id) ?? "midnight";
}

/** The accent kept for an id; none (the theme's own) for one this app doesn't know. */
export function accentId(id: string | null | undefined): AccentId | null {
  return ACCENTS.find((a) => a === id) ?? null;
}

/** A profile of a server: user ids repeat from one server to the next, so a server's origin ("http://pley.local:7539") goes with its id. */
export interface ProfileOf {
  server: string;
  id: number;
}

/**
 * Where the TV keeps the picks: each profile's, by its server as its
 * sign-in is kept (`pley.theme.<server>.<user id>`), and the one shown last
 * (`pley.theme`), which shows before a profile is known.
 */
export const PICK_KEYS = {
  theme: (profile: ProfileOf | null) => (profile ? `pley.theme.${profile.server}.${profile.id}` : "pley.theme"),
  accent: (profile: ProfileOf | null) => (profile ? `pley.accent.${profile.server}.${profile.id}` : "pley.accent"),
};

export interface ThemePick {
  theme: ThemeId;
  accent: AccentId | null;
}

/**
 * The theme and accent to show: the profile's own on this TV once it is
 * known (Midnight in its own accent for one that picked none here), the
 * one shown last before then (packages/tv/test/fixtures/appearance.json).
 */
export function shownPick(get: (key: string) => string | null, profile: ProfileOf | null): ThemePick {
  return { theme: themeId(get(PICK_KEYS.theme(profile))), accent: accentId(get(PICK_KEYS.accent(profile))) };
}

function themeSpec(id: ThemeId): ThemeSpec {
  return SPEC.themes.find((t) => t.id === id) ?? SPEC.themes[0]!;
}

/** Whether a theme is light: its players draw in its dark twin. */
export function isLight(theme: ThemeId): boolean {
  return themeSpec(theme).kind === "light";
}

/** A light theme's dark twin, which its players draw in; a dark theme is its own. */
export function twinOf(theme: ThemeId): ThemeId {
  return themeId(themeSpec(theme).twin ?? theme);
}

/** A token's color in a theme under an accent (null: the theme's own), as every app draws it (`colorOf` on the web). */
export function tokenColor(theme: ThemeId, accent: AccentId | null, token: string): string {
  const t = themeSpec(theme);
  const a = accent ? SPEC.accents.find((x) => x.id === accent) : undefined;
  return (t.follows.indexOf(token) >= 0 && a?.[t.kind][token]) || t.colors[token]!;
}

/** "#0b0d10", "rgb(16 26 44 / 0.42)": red, green, blue and alpha. */
export function channels(color: string): [number, number, number, number] {
  const hex = /^#([0-9a-f]{6})$/i.exec(color);
  if (hex) {
    const n = parseInt(hex[1]!, 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 1];
  }
  const rgb = /^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)\s*(?:[/,]\s*([\d.]+)\s*)?\)$/.exec(color);
  if (!rgb) throw new Error(`not a color: ${color}`);
  return [Number(rgb[1]), Number(rgb[2]), Number(rgb[3]), rgb[4] === undefined ? 1 : Number(rgb[4])];
}

const round = (n: number) => Math.round(n * 1000) / 1000;

/**
 * The app's colors in a theme under an accent, by custom property: each
 * role's color, the see-through ones' channels (`--fg-rgb: 233, 235, 239`),
 * what sits on pictures (`--on-art`), the shade at each strength
 * (`--shade-60`) and the open rail's picture (`--sidebar`). All are written
 * out, none read from another: an element that takes the dark twin's
 * (`[data-dark]`) has all of its own.
 */
export function themeVariables(theme: ThemeId, accent: AccentId | null): Record<string, string> {
  const vars: Record<string, string> = {};
  for (const [name, token] of ROLES) vars[`--${name}`] = tokenColor(theme, accent, token);
  for (const name of SEE_THROUGH) vars[`--${name}-rgb`] = channels(vars[`--${name}`]!).slice(0, 3).join(", ");
  vars["--on-art"] = `rgba(${vars["--bg-rgb"]}, 0.8)`;
  const [r, g, b, a] = channels(tokenColor(theme, accent, "shade"));
  for (const strength of SHADES) vars[`--shade-${strength}`] = `rgba(${r}, ${g}, ${b}, ${round((a * strength) / 100)})`;
  vars["--sidebar"] = theme === "sunshine" ? SUNSHINE_SIDEBAR : "none";
  return vars;
}

const declarations = (vars: Record<string, string>) =>
  Object.keys(vars)
    .map((name) => `${name}:${vars[name]}`)
    .join(";");

/** The stylesheet for a theme under an accent: the page's colors, and in a light theme the players' (`[data-dark]`). */
export function themeCss(theme: ThemeId, accent: AccentId | null): string {
  const page = `:root{${declarations(themeVariables(theme, accent))}}`;
  return isLight(theme) ? `${page}\n[data-dark]{${declarations(themeVariables(twinOf(theme), accent))}}` : page;
}

/** A theme's colors written on one element (a preview of it), in an inline style. */
export function themeStyle(theme: ThemeId, accent: AccentId | null): string {
  return declarations(themeVariables(theme, accent));
}

let sheet: HTMLStyleElement | null = null;

/** Draws the app in a theme and an accent, from the next frame: one stylesheet, after theme.css's Midnight. */
export function applyTheme(theme: ThemeId, accent: AccentId | null): void {
  if (!sheet || !sheet.isConnected) {
    sheet = document.createElement("style");
    sheet.id = "pley-theme";
    document.head.appendChild(sheet);
  }
  sheet.textContent = themeCss(theme, accent);
  document.documentElement.setAttribute("data-theme", theme);
}

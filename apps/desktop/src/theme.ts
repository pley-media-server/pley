import themes from "../../../packages/themes/themes.json";

/**
 * The theme and accent picked in the server's web app (docs/THEMES.md),
 * which this page can't read: the web app keeps it in the window's storage
 * at the server's address. The server's pages tell the app their pick, and
 * the app opens this page with it in its address (`?theme=…&accent=…`,
 * src-tauri/src/theme.rs), so it draws in it from its first frame.
 */
export interface Pick {
  theme: string;
  /** null for the theme's own. */
  accent: string | null;
}

interface ThemeSpec {
  id: string;
  kind: "dark" | "light";
  colors: Record<string, string>;
  follows: string[];
}

interface AccentSpec {
  id: string;
  dark: Record<string, string>;
  light: Record<string, string>;
}

const { themes: THEMES, accents: ACCENTS } = themes as unknown as { themes: ThemeSpec[]; accents: AccentSpec[] };

/** The pick in the page's address; ids this app doesn't know are Midnight and the theme's own. */
export function pickFrom(search: string): Pick {
  const params = new URLSearchParams(search);
  const theme = THEMES.find(({ id }) => id === params.get("theme"))?.id ?? "midnight";
  const accent = ACCENTS.find(({ id }) => id === params.get("accent"))?.id ?? null;
  return { theme, accent };
}

function themeOf(id: string): ThemeSpec {
  return THEMES.find((t) => t.id === id) ?? THEMES[0]!;
}

/**
 * Every token's color in a pick, by name (`bg`, `accent`, `brand-tile`):
 * the accent's where the theme follows the token, as every app draws it
 * (`colorOf` in apps/web/src/theme/spec.ts).
 */
export function colorsOf({ theme, accent }: Pick): Record<string, string> {
  const t = themeOf(theme);
  const set = ACCENTS.find(({ id }) => id === accent)?.[t.kind];
  return Object.fromEntries(Object.entries(t.colors).map(([name, own]) => [name, (t.follows.includes(name) && set?.[name]) || own]));
}

/** `#38bdf8` at `alpha`, as `rgb(56 189 248 / 0.09)`. */
function translucent(hex: string, alpha: number): string {
  const [r, g, b] = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
  return `rgb(${r} ${g} ${b} / ${alpha})`;
}

/**
 * Draws the page in `pick`: each token as a `--name` property (style.css
 * draws in nothing else), the theme's id for its backdrop, and its kind for
 * the browser's own controls.
 */
export function showPick(pick: Pick, root: HTMLElement = document.documentElement): void {
  const colors = colorsOf(pick);
  for (const [name, color] of Object.entries(colors)) root.style.setProperty(`--${name}`, color);
  // The light at the top of the page, in the accent, where the theme has no backdrop of its own.
  root.style.setProperty("--halo", translucent(colors.accent ?? "#000000", 0.09));
  const { id, kind } = themeOf(pick.theme);
  root.style.colorScheme = kind;
  document.querySelector('meta[name="color-scheme"]')?.setAttribute("content", kind);
  root.dataset.theme = id;
}

import { useSyncExternalStore } from "react";
import { logoIcon } from "@/components/Logo";

/**
 * The theme Pley is drawn in (docs/THEMES.md): each device's own, kept in its
 * storage, Midnight until one is picked there. index.css holds each theme's
 * colors under `[data-theme="…"]`; the page's own is on <html>, set before
 * the first page draws by public/theme.js. An accent picked over the
 * theme's own is kept and shown alike, on <html data-accent>.
 */
export const THEMES = ["midnight", "midday", "twilight", "starlight", "moonlight", "nightshade", "greenlight", "sunshine", "blacklight"] as const;
export type Theme = (typeof THEMES)[number];

/** Pley's own, for a device that picked none. */
export const DEFAULT_THEME: Theme = "midnight";

/** Where this device keeps the theme it showed last; public/theme.js reads it too. */
export const THEME_KEY = "pley.theme";

export function isTheme(value: unknown): value is Theme {
  return typeof value === "string" && (THEMES as readonly string[]).includes(value);
}

/**
 * The accents picked over a theme's own (docs/THEMES.md, "Accents"): on
 * <html data-accent>, none for the theme's own.
 */
export const ACCENTS = ["sky", "violet", "miami", "gold", "mint", "amber"] as const;
export type Accent = (typeof ACCENTS)[number];

/** Where this device keeps the accent it showed last; public/theme.js reads it too. */
export const ACCENT_KEY = "pley.accent";

export function isAccent(value: unknown): value is Accent {
  return typeof value === "string" && (ACCENTS as readonly string[]).includes(value);
}

let shown: Theme = DEFAULT_THEME;
let shownAccent: Accent | null = null;
const listeners = new Set<() => void>();

/** The theme on the page. */
export function currentTheme(): Theme {
  return shown;
}

/** The accent on the page; null for the theme's own. */
export function currentAccent(): Accent | null {
  return shownAccent;
}

export function onTheme(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function draw(theme: Theme, accent: Accent | null) {
  const root = document.documentElement;
  // Every color changes at once: what eases its color (a link, a button)
  // would otherwise fade from the old theme while the rest has changed.
  root.classList.add("theme-changing");
  root.dataset.theme = theme;
  if (accent) root.dataset.accent = accent;
  else delete root.dataset.accent;
  // Draws the new colors now, while nothing eases.
  void document.body.offsetHeight;
  setTimeout(() => root.classList.remove("theme-changing"), 0);
  // The browser's own bars (a phone's, an installed app's) in the page's colors.
  const style = getComputedStyle(root);
  const background = style.getPropertyValue("--color-bg").trim();
  if (background) document.querySelector('meta[name="theme-color"]')?.setAttribute("content", background);
  if (style.colorScheme) document.querySelector('meta[name="color-scheme"]')?.setAttribute("content", style.colorScheme);
  // The tab's icon: Pley's own file in Midnight in its own accent, the logo
  // in the page's colors otherwise, as an accent colors it too.
  const icon = document.querySelector<HTMLLinkElement>('link[rel="icon"][type="image/svg+xml"]');
  if (icon && background) {
    icon.href = theme === DEFAULT_THEME && !accent ? "/favicon.svg" : `data:image/svg+xml,${encodeURIComponent(logoIcon(style))}`;
  }
}

/** Puts a theme on the page, and keeps it so this device's next visit starts in it. */
export function showTheme(theme: Theme) {
  if (theme === shown && document.documentElement.dataset.theme === theme) return;
  shown = theme;
  draw(theme, shownAccent);
  try {
    localStorage.setItem(THEME_KEY, theme);
  } catch {
    // Without storage, the next visit starts in Midnight.
  }
  for (const listener of listeners) listener();
}

/**
 * Puts an accent on the page over the theme's own (null goes back to it),
 * and keeps it on this device as the theme is kept.
 */
export function showAccent(accent: Accent | null) {
  if (accent === shownAccent && (document.documentElement.dataset.accent ?? null) === accent) return;
  shownAccent = accent;
  draw(shown, accent);
  try {
    if (accent) localStorage.setItem(ACCENT_KEY, accent);
    else localStorage.removeItem(ACCENT_KEY);
  } catch {
    // Without storage, the next visit starts in the theme's own.
  }
  for (const listener of listeners) listener();
}

/** The theme and accent this device keeps, Midnight in its own where it keeps none it knows. */
function kept(): [Theme, Accent | null] {
  let theme: string | null = null;
  let accent: string | null = null;
  try {
    theme = localStorage.getItem(THEME_KEY);
    accent = localStorage.getItem(ACCENT_KEY);
  } catch {
    // Nothing kept.
  }
  return [isTheme(theme) ? theme : DEFAULT_THEME, isAccent(accent) ? accent : null];
}

let following = false;

/**
 * Before the app draws: the theme and accent of this device's last visit.
 * Then follows a pick made in another of its tabs, which keeps it alike.
 */
export function startTheme() {
  [shown, shownAccent] = kept();
  draw(shown, shownAccent);
  if (following) return;
  following = true;
  window.addEventListener("storage", (event) => {
    // A key of its own, or the storage cleared (null).
    if (event.key !== null && event.key !== THEME_KEY && event.key !== ACCENT_KEY) return;
    const [theme, accent] = kept();
    if (theme === shown && accent === shownAccent) return;
    [shown, shownAccent] = [theme, accent];
    draw(theme, accent);
    for (const listener of listeners) listener();
  });
}

/** The theme on the page, following changes. */
export function useTheme(): Theme {
  return useSyncExternalStore(onTheme, currentTheme);
}

/** The accent on the page (null for the theme's own), following changes. */
export function useAccent(): Accent | null {
  return useSyncExternalStore(onTheme, currentAccent);
}

import { useLayoutEffect, useSyncExternalStore } from "react";
import { logoIcon } from "@/components/Logo";
import { tellShellTheme } from "@/lib/shell";

/**
 * The theme Pley is drawn in (docs/THEMES.md): each profile's own on each
 * device, kept in its storage, Midnight until one is picked there. index.css
 * holds each theme's colors under `[data-theme="…"]`; the page's own is on
 * <html>, set before the first page draws by public/theme.js from the one
 * the device showed last. An accent picked over the theme's own is kept and
 * shown alike, on <html data-accent>.
 */
export const THEMES = ["midnight", "midday", "twilight", "starlight", "moonlight", "nightshade", "greenlight", "sunshine", "blacklight"] as const;
export type Theme = (typeof THEMES)[number];

/** Pley's own, for a device that picked none. */
export const DEFAULT_THEME: Theme = "midnight";

/**
 * Where this device keeps the theme it showed last, which public/theme.js
 * reads too; each profile's own is under `pley.theme.<id>`.
 */
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

/** Where this device keeps the accent it showed last, as the theme; each profile's own under `pley.accent.<id>`. */
export const ACCENT_KEY = "pley.accent";

export function isAccent(value: unknown): value is Accent {
  return typeof value === "string" && (ACCENTS as readonly string[]).includes(value);
}

let shown: Theme = DEFAULT_THEME;
let shownAccent: Accent | null = null;
/** The profile signed in, once the app knows it; null before. */
let profile: string | null = null;
const listeners = new Set<() => void>();

/** Where a profile's pick is kept on this device; with none, the one it showed last. */
const keyOf = (key: string, of: string | null) => (of === null ? key : `${key}.${of}`);

/** Keeps a pick (null: none) as the profile's on this device, and as the one it showed last. */
function keep(key: string, value: string | null) {
  try {
    for (const name of new Set([key, keyOf(key, profile)])) {
      if (value) localStorage.setItem(name, value);
      else localStorage.removeItem(name);
    }
  } catch {
    // Without storage, the next visit starts in Midnight, in its own accent.
  }
}

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
  // The desktop app's own pages and its window, in the same.
  tellShellTheme(theme, accent);
}

/** Shows a theme and an accent, telling who follows, if the page doesn't show them already. */
function show(theme: Theme, accent: Accent | null) {
  const root = document.documentElement;
  if (theme === shown && accent === shownAccent && root.dataset.theme === theme && (root.dataset.accent ?? null) === accent) return;
  [shown, shownAccent] = [theme, accent];
  draw(theme, accent);
  for (const listener of listeners) listener();
}

/** Puts a theme on the page, and keeps it so this profile's next visit on this device starts in it. */
export function showTheme(theme: Theme) {
  show(theme, shownAccent);
  keep(THEME_KEY, theme);
}

/**
 * Puts an accent on the page over the theme's own (null goes back to it),
 * and keeps it for this profile on this device as the theme is kept.
 */
export function showAccent(accent: Accent | null) {
  show(shown, accent);
  keep(ACCENT_KEY, accent);
}

/** The theme and accent kept for a profile on this device (null: the last shown), Midnight in its own where none it knows is. */
function kept(of: string | null): [Theme, Accent | null] {
  let theme: string | null = null;
  let accent: string | null = null;
  try {
    theme = localStorage.getItem(keyOf(THEME_KEY, of));
    accent = localStorage.getItem(keyOf(ACCENT_KEY, of));
  } catch {
    // Nothing kept.
  }
  return [isTheme(theme) ? theme : DEFAULT_THEME, isAccent(accent) ? accent : null];
}

/** How many screens outside a profile are on the page (signing in, "Who's watching?", setup). */
let outside = 0;

/**
 * What the page is drawn in: Midnight in its own accent outside a profile,
 * the profile's own pick inside one, and before the app knows the profile,
 * the one shown last (its first page, while it asks).
 */
function wanted(): [Theme, Accent | null] {
  return outside > 0 ? [DEFAULT_THEME, null] : kept(profile);
}

/** Shows what the page should be drawn in, and keeps it as the one shown last for the next visit's first page. */
function refresh() {
  const [theme, accent] = wanted();
  show(theme, accent);
  try {
    localStorage.setItem(THEME_KEY, theme);
    if (accent) localStorage.setItem(ACCENT_KEY, accent);
    else localStorage.removeItem(ACCENT_KEY);
  } catch {
    // Nothing kept.
  }
}

/**
 * The profile signed in, once the app knows it (RequireAuth): shows its own
 * pick on this device, Midnight in its own accent where it made none.
 */
export function showProfile(id: string | number) {
  const of = String(id);
  if (of === profile) return;
  profile = of;
  refresh();
}

/**
 * The app leaves the signed-in session (signing out, the session gone): no
 * profile is known until the next one signs in, so nothing shown meanwhile
 * takes the one before's pick.
 */
export function forgetProfile() {
  profile = null;
}

/**
 * A screen outside a profile (signing in, "Who's watching?", setup, the
 * server out of reach) is drawn in Midnight in its own accent, whichever
 * profile was shown before; the profile's own comes back when it goes.
 */
export function useOutsideProfile() {
  useLayoutEffect(() => {
    outside += 1;
    refresh();
    return () => {
      outside -= 1;
      refresh();
    };
  }, []);
}

let following = false;

/**
 * Before the app draws, and before it knows the profile: the theme and
 * accent this device showed last, the profile's then signed in or Midnight.
 * Then follows a pick made in another of its tabs, which keeps them alike.
 */
export function startTheme() {
  profile = null;
  outside = 0;
  const [theme, accent] = kept(null);
  [shown, shownAccent] = [theme, accent];
  draw(theme, accent);
  if (following) return;
  following = true;
  window.addEventListener("storage", (event) => {
    // The profile's keys or the last shown, or the storage cleared (null).
    const keys = [THEME_KEY, ACCENT_KEY, keyOf(THEME_KEY, profile), keyOf(ACCENT_KEY, profile)];
    if (event.key !== null && !keys.includes(event.key)) return;
    show(...wanted());
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

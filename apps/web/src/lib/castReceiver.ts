import { appShell } from "./shell";

/** Where this install's receiver id is kept. */
const KEY = "pley.receiver";

/** A receiver id the server takes: 1 to 128 letters, digits, `.`, `_`, `:` and `-`. */
const VALID = /^[A-Za-z0-9._:-]{1,128}$/;

/**
 * Whether this app takes casts: the desktop app does, as a TV or a phone
 * does. A browser tab doesn't: the browser won't start sound in a tab in
 * the background, so a cast to it would wait for a click nobody makes.
 */
export function takesCasts(): boolean {
  return appShell() === "desktop";
}

/**
 * This install's own id as a receiver of casts, made once and kept: every
 * window of the desktop app is the one device to the user's remotes.
 */
export function receiverId(): string {
  try {
    const kept = localStorage.getItem(KEY);
    if (kept && VALID.test(kept)) return kept;
    const made = `desktop-${crypto.randomUUID()}`;
    localStorage.setItem(KEY, made);
    return made;
  } catch {
    // No storage: this window, for as long as it lasts.
    fallback ??= `desktop-${crypto.randomUUID()}`;
    return fallback;
  }
}
let fallback: string | undefined;

/**
 * What the user's remotes call this device: "Pley for Windows", "Pley for
 * Mac", "Pley for Linux". The desktop app's web view says only which
 * browser engine it runs ("Edge on Windows"), not that it is Pley.
 */
export function receiverName(userAgent: string = navigator.userAgent): string {
  if (/Windows/.test(userAgent)) return "Pley for Windows";
  if (/Mac OS X|Macintosh/.test(userAgent)) return "Pley for Mac";
  if (/Linux/.test(userAgent)) return "Pley for Linux";
  return "Pley for desktop";
}

/**
 * What a play request (a title, music, a channel) says of this device, for
 * remotes to show its sessions under the name and kind its receiver has:
 * the desktop app's; nothing in a browser, which the server names itself.
 */
export function deviceFields(): { device_name?: string; device_kind?: "computer" } {
  return takesCasts() ? { device_name: receiverName(), device_kind: "computer" } : {};
}

/**
 * The history state a cast opens the player with: a new one for each cast,
 * so the player starts again where, and with the tracks, the remote said,
 * even on the title it plays already (`castOf`, player/PlayerPage.tsx).
 */
export function castState(): { cast: number } {
  return { cast: Date.now() };
}

/** The cast a page was opened by (`castState`), if one was. */
export function castOf(state: unknown): number | null {
  const cast = (state as { cast?: unknown } | null)?.cast;
  return typeof cast === "number" ? cast : null;
}

/**
 * The event stream's query for this device as a receiver (docs/API.md,
 * "Casting"): a computer, by its name, that plays titles, music and
 * playlists, and Live TV's channels when the user has them (`live`).
 */
export function receiverQuery(live: boolean): string {
  const p = new URLSearchParams({
    receiver: receiverId(),
    device_name: receiverName(),
    device_kind: "computer",
    casts: live ? "item,live" : "item",
  });
  return p.toString();
}

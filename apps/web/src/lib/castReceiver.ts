import type { Id, ServerEvent } from "@/api/types";
import { appShell } from "./shell";

export type Cast = Extract<ServerEvent, { type: "cast" }>;

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
 * Whether `cast` comes from another profile than `profileId`, the one this
 * device is on: it switches to the caster's first (docs/API.md, "Casting").
 */
export function switchesProfile(cast: Cast, profileId: Id): boolean {
  return cast.profile_id != null && cast.profile_id !== profileId;
}

/** How long a cast handed over waits for its profile's receiver: as long as its ticket lasts. */
const HANDED_OVER_MS = 60_000;

let handedOver: { cast: Cast; at: number } | null = null;

/**
 * Keeps `cast` for the receiver of the profile it switches this device to:
 * the switch starts the app over as that profile, its receiver too, which
 * plays it (`takeHandedOver`). A later one takes its place.
 */
export function handOver(cast: Cast) {
  handedOver = { cast, at: Date.now() };
}

/** Drops `cast`, kept by `handOver`, when its switch didn't go through. */
export function dropHandedOver(cast: Cast) {
  if (handedOver?.cast === cast) handedOver = null;
}

/** The cast kept for `profileId`'s receiver, once: a later call, or another profile's, has none. */
export function takeHandedOver(profileId: Id): Cast | null {
  const kept = handedOver;
  if (!kept || kept.cast.profile_id !== profileId) return null;
  handedOver = null;
  return Date.now() - kept.at <= HANDED_OVER_MS ? kept.cast : null;
}

/**
 * The event stream's query for this device as a receiver (docs/API.md,
 * "Casting"): a computer, by its name, that plays titles, music and
 * playlists, and Live TV's channels when the user has them (`live`). It
 * switches to the profile of a remote that casts from another (`switch`):
 * without it, the household's other profiles wouldn't see it.
 */
export function receiverQuery(live: boolean): string {
  const p = new URLSearchParams({
    receiver: receiverId(),
    device_name: receiverName(),
    device_kind: "computer",
    casts: live ? "item,live,switch" : "item,switch",
  });
  return p.toString();
}

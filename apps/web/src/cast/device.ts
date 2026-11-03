import type { CastPlatform } from "@/api/types";
import { appShell } from "@/lib/shell";

/**
 * This device, as one the profile's other devices may cast to. Only the
 * desktop app is one: browser tabs are not, as every tab open would play.
 */
export interface CastSelf {
  id: string;
  name: string;
  platform: CastPlatform;
}

const ID_KEY = "pley.castDevice";
/** What the device is called when the app can't tell the computer's name. */
export const DESKTOP_NAME = "Pley for desktop";
/** As the server takes names: up to 64 characters. */
const NAME_MAX = 64;

/** Made once, kept for this page while storage can't keep it. */
let madeId: string | null = null;

function newId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/** The id this device made once and keeps: 32 hex digits. */
function deviceId(): string {
  try {
    const kept = window.localStorage.getItem(ID_KEY);
    if (kept && /^[0-9a-f]{32}$/.test(kept)) return kept;
  } catch {
    // Storage unavailable: the id lasts as long as the page.
  }
  madeId ??= newId();
  try {
    window.localStorage.setItem(ID_KEY, madeId);
  } catch {
    // As above.
  }
  return madeId;
}

/** The computer's name, which the desktop app tells its pages (apps/desktop/src-tauri/src/lib.rs). */
function computerName(): string {
  const told = (window as { __PLEY_DEVICE_NAME__?: unknown }).__PLEY_DEVICE_NAME__;
  if (typeof told !== "string") return DESKTOP_NAME;
  // eslint-disable-next-line no-control-regex
  const name = [...told.replace(/[\u0000-\u001f\u007f-\u009f]/g, "").trim()].slice(0, NAME_MAX).join("").trim();
  return name || DESKTOP_NAME;
}

/** This device as one to cast to, or `null` where it isn't one (a browser). */
export function castSelf(): CastSelf | null {
  if (typeof window === "undefined" || appShell() !== "desktop") return null;
  return { id: deviceId(), name: computerName(), platform: "desktop" };
}

/** The event stream's address: a device that may be cast to says so as it opens it. */
export function eventsUrl(self: CastSelf | null): string {
  if (!self) return "/api/events";
  const query = new URLSearchParams({ cast_device: self.id, cast_name: self.name, cast_platform: self.platform });
  return `/api/events?${query.toString()}`;
}

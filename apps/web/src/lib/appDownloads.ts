import { useEffect, useSyncExternalStore } from "react";
import { appShell } from "./shell";

/**
 * A download the desktop app makes for this server's page. The app fetches
 * the files the page downloads itself, to play while the server is away,
 * and tells the page how they go (apps/desktop/src-tauri/src/downloads.rs).
 */
export interface AppDownload {
  /** `0` for a file the page made (saved with the system's downloads, not followed). */
  id: number;
  name: string;
  size: number | null;
  received: number;
  /** `preparing`: the server is making the file for the app's player (`preparing` says how far, 0 to 1). */
  state: "queued" | "preparing" | "downloading" | "waiting" | "failed" | "done";
  /** Why it failed or waits, in English. */
  error: string | null;
  /** `error` as a key of the app's words and its values (`{ key: "notWritten", detail }`), to say it in the page's language. */
  why?: ({ key: string } & Record<string, unknown>) | null;
  preparing?: number | null;
}

/** The event the desktop app sends the page. */
export const APP_DOWNLOAD_EVENT = "pley:download";

const STATES = new Set<unknown>(["queued", "preparing", "downloading", "waiting", "failed", "done"]);

function isDownload(value: unknown): value is AppDownload {
  const d = value as Partial<AppDownload> | null;
  return (
    typeof d === "object" &&
    d !== null &&
    typeof d.id === "number" &&
    typeof d.name === "string" &&
    typeof d.received === "number" &&
    STATES.has(d.state)
  );
}

type Change = (download: AppDownload, before: AppDownload | undefined) => void;

let downloads: readonly AppDownload[] = [];
const listeners = new Set<() => void>();
const changes = new Set<Change>();
let listening = false;

function receive(event: Event) {
  const d = (event as CustomEvent<unknown>).detail;
  if (!isDownload(d)) return;
  const before = downloads.find((x) => x.id === d.id);
  if (d.id !== 0) {
    downloads = before ? downloads.map((x) => (x.id === d.id ? d : x)) : [...downloads, d];
    listeners.forEach((l) => l());
  }
  changes.forEach((c) => c(d, before));
}

/** Hears the desktop app, from the first page that asks (nothing elsewhere). */
function listen() {
  if (listening || typeof window === "undefined" || appShell() !== "desktop") return;
  listening = true;
  window.addEventListener(APP_DOWNLOAD_EVENT, receive);
}

function subscribe(listener: () => void) {
  listen();
  listeners.add(listener);
  return () => void listeners.delete(listener);
}

/** The downloads the desktop app told this page of, oldest first. */
export function useAppDownloads(): readonly AppDownload[] {
  return useSyncExternalStore(subscribe, () => downloads, () => downloads);
}

/** Calls `onChange` with each word from the desktop app and what was known before. */
export function useAppDownloadChanges(onChange: Change) {
  useEffect(() => {
    listen();
    changes.add(onChange);
    return () => void changes.delete(onChange);
  }, [onChange]);
}

/** Whether `d` is still to come. */
export const coming = (d: AppDownload) =>
  d.state === "queued" || d.state === "preparing" || d.state === "downloading" || d.state === "waiting";

/** How far `list` has come together, 0 to 1 (`null` before any size is known). */
export function fractionOf(list: readonly AppDownload[]): number | null {
  const sized = list.filter((d) => d.size);
  if (!sized.length) return null;
  const size = sized.reduce((sum, d) => sum + (d.size ?? 0), 0);
  return Math.min(1, sized.reduce((sum, d) => sum + d.received, 0) / size);
}

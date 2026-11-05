import type { KeyValue } from "../auth/tokens";
import { formatNumber } from "../i18n";

/**
 * Moving subtitles in time, for files whose subtitles are early or late:
 * positive is later (a subtitle at 10 s shows at 10.5 s with +0.5 s).
 * Steps of 100 ms, at most 10 s either way, remembered per profile and
 * file on this TV and never sent to the server (the Android app's rules).
 */
export const SYNC_STEP_MS = 100;
export const SYNC_MAX_MS = 10_000;

export function clampOffset(ms: number): number {
  if (!isFinite(ms)) return 0;
  const stepped = Math.round(ms / SYNC_STEP_MS) * SYNC_STEP_MS;
  return Math.max(-SYNC_MAX_MS, Math.min(SYNC_MAX_MS, stepped)) + 0;
}

/** `+0.5 s`, `−1.2 s`, `0.0 s` (`+0,5 s` in French). */
export function formatOffset(ms: number): string {
  const v = clampOffset(ms);
  const sign = v > 0 ? "+" : v < 0 ? "−" : "";
  return `${sign}${formatNumber(Math.abs(v) / 1000, { minimumFractionDigits: 1, maximumFractionDigits: 1 })} s`;
}

const KEY = "pley.subtitleOffsets";
/** Files whose offsets are kept: the oldest go first. */
const KEPT = 300;

/**
 * Offsets by profile and file, kept under the file's absolute address and
 * the profile (`<server>/api/files/<id>#u<profile>`): subtitles one person
 * finds late may suit another. Offsets kept before, by file alone
 * (`<server>/api/files/<id>`), go to the first profile that plays the file.
 * Without a profile nothing is read or kept.
 */
export class OffsetStore {
  private readonly store: KeyValue;
  private readonly profile: () => number | null;

  constructor(store: KeyValue, profile: () => number | null) {
    this.store = store;
    this.profile = profile;
  }

  private write(entries: [string, number][]): void {
    try {
      this.store.set(KEY, JSON.stringify(entries.slice(-KEPT)));
    } catch {
      // Storage full or unavailable: the offset lasts as long as the player.
    }
  }

  private read(): [string, number][] {
    try {
      const v: unknown = JSON.parse(this.store.get(KEY) ?? "[]");
      return Array.isArray(v) ? (v.filter((e) => Array.isArray(e) && typeof e[0] === "string" && typeof e[1] === "number") as [string, number][]) : [];
    } catch {
      return [];
    }
  }

  get(server: string, fileId: number): number {
    const profile = this.profile();
    if (profile === null) return 0;
    const file = `${server}/api/files/${fileId}`;
    const key = `${file}#u${profile}`;
    const entries = this.read();
    for (const [k, v] of entries) if (k === key) return clampOffset(v);
    const before = entries.filter(([k]) => k === file)[0];
    if (!before) return 0;
    // Kept before offsets were per profile: this profile's from now on.
    const moved = clampOffset(before[1]);
    const rest = entries.filter(([k]) => k !== file);
    if (moved !== 0) rest.push([key, moved]);
    this.write(rest);
    return moved;
  }

  /** Remembers `ms` for the file and the profile; 0 forgets it. */
  set(server: string, fileId: number, ms: number): void {
    const profile = this.profile();
    if (profile === null) return;
    const file = `${server}/api/files/${fileId}`;
    const key = `${file}#u${profile}`;
    const entries = this.read().filter(([k]) => k !== key && k !== file);
    const value = clampOffset(ms);
    if (value !== 0) entries.push([key, value]);
    this.write(entries);
  }
}

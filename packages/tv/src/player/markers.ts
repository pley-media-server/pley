import type { ItemDetail, Marker } from "../api/types";
import { t } from "../i18n";

/** The intro and credits of the file playing. */
export function markersOf(item: ItemDetail, fileId: number): Marker[] {
  return (item.markers ?? []).filter((m) => m.file_id === fileId && (m.kind === "intro" || m.kind === "credits"));
}

/** The Skip button leaves this long before its marker ends. */
export const SKIP_LEAVES_MS = 1000;

/** The marker the playhead is in, for the Skip button: its end is still more than a second away. */
export function activeMarker(markers: Marker[], positionMs: number): Marker | null {
  return markers.filter((m) => m.start_ms <= positionMs && positionMs < m.end_ms - SKIP_LEAVES_MS)[0] ?? null;
}

/**
 * Where Skip goes: an intro's end, less up to 750 ms (half a short one) so
 * the cut isn't missed; credits' end. The caller clamps to the duration.
 */
export function skipTarget(m: Marker): number {
  return m.kind === "intro" ? m.end_ms - Math.min(750, (m.end_ms - m.start_ms) / 2) : m.end_ms;
}

export function skipLabel(m: Marker): string {
  return m.kind === "intro" ? t("player.skip.intro") : t("player.skip.credits");
}

/** Skip shows on its own this long from when it appears; then only with the controls. */
export const SKIP_SHOWN_MS = 8_000;

/** Identifies a marker, to tell entering it again from entering another. */
function markerKey(m: Marker): string {
  return `${m.file_id}:${m.kind}:${m.start_ms}`;
}

/**
 * When Skip shows on its own: for SKIP_SHOWN_MS of the wall clock from the
 * playhead entering a marker (playing, seeking or starting into it), paused
 * or not; after that only with the controls, so it doesn't sit over the
 * picture for a whole intro or the credits. Entering a marker again, or
 * another, starts afresh; a seek within the one it is in doesn't. Up Next's
 * card takes Skip's place while it shows, so Skip starts afresh when the
 * card goes, however it goes (Cancel, a seek).
 * test/fixtures/skip-shown.json pins it for both Samsung apps.
 */
export class SkipWindow {
  private key: string | null = null;
  private since = 0;

  /**
   * Whether Skip, offered for `m` at `nowMs`, shows on its own. `m` is null
   * while the playhead is in no marker, or Skip is hidden under Up Next's card.
   */
  at(m: Marker | null, nowMs: number): boolean {
    const key = m ? markerKey(m) : null;
    if (key !== this.key) {
      this.key = key;
      this.since = nowMs;
    }
    return this.key !== null && nowMs - this.since < SKIP_SHOWN_MS;
  }

  /** A new item: the next marker starts afresh, even the same one. */
  reset(): void {
    this.key = null;
  }
}

/** Whether Skip shows, the playhead in a marker: on its own while its window lasts, else with the controls. */
export function skipShows(inMarker: boolean, onItsOwn: boolean, controls: boolean): boolean {
  return inMarker && (onItsOwn || controls);
}

/**
 * Credits ending this close to the end of their file run to it: only a
 * studio's logo or black follows (the server's CREDITS_TAIL_MS).
 */
export const CREDITS_TAIL_MS = 15_000;

/**
 * Whether `m` is credits that run to the end of its file, `durationMs` long
 * (0 or null when unknown). Only those bring Up Next: its countdown would
 * skip what follows other credits (an after-credits scene, a show's last
 * minutes after music every episode shares), which bring Skip Credits alone.
 * test/fixtures/markers.json pins it for both Samsung apps.
 */
export function reachesEnd(m: Marker, durationMs: number | null): boolean {
  return m.kind === "credits" && durationMs !== null && durationMs > 0 && durationMs - m.end_ms <= CREDITS_TAIL_MS;
}

/**
 * Credits that run to the end start Up Next once each: seeking back into
 * the same credits doesn't show the card again. Forgotten for a new item.
 */
export class CreditsGate {
  private seen = new Set<string>();

  /** Whether entering `m`, in a file `durationMs` long, should start Up Next (the first time only). */
  enter(m: Marker, durationMs: number | null): boolean {
    if (!reachesEnd(m, durationMs)) return false;
    const key = markerKey(m);
    if (this.seen.has(key)) return false;
    this.seen.add(key);
    return true;
  }

  reset(): void {
    this.seen = new Set();
  }
}

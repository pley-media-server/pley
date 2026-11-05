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

/**
 * Credits start Up Next once each: seeking back into the same credits
 * doesn't show the card again. Forgotten for a new item.
 */
export class CreditsGate {
  private seen = new Set<string>();

  /** Whether entering `m` should start Up Next (the first time only). */
  enter(m: Marker): boolean {
    if (m.kind !== "credits") return false;
    const key = `${m.file_id}:${m.kind}:${m.start_ms}`;
    if (this.seen.has(key)) return false;
    this.seen.add(key);
    return true;
  }

  reset(): void {
    this.seen = new Set();
  }
}

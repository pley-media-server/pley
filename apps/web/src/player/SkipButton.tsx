import { useEffect, useEffectEvent, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronsRight } from "lucide-react";
import type { ItemDetail, Marker } from "@/api/types";
import { cn } from "@/lib/cn";

/** The button disappears this long before the segment ends. */
const TAIL_MS = 1_000;
/** Skip Intro lands this long before the intro ends, so it plays out. */
const INTRO_LEAD_MS = 750;
/** Credits ending this close to the end of their file run to it (the server's `CREDITS_TAIL_MS`). */
const CREDITS_TAIL_MS = 15_000;

/** Markers of the file being played (the item's first file when unknown). */
export function fileMarkers(item: ItemDetail, fileId: number | null | undefined): Marker[] {
  const id = fileId ?? item.files[0]?.id;
  return (item.markers ?? []).filter((m) => m.file_id === id);
}

/**
 * The marker a place (in milliseconds) is in, if any: from its start until a
 * second before its end. A kind this app has no button for is left out.
 */
export function activeMarker(markers: Marker[], ms: number): Marker | null {
  return markers.find((m) => (m.kind === "intro" || m.kind === "credits") && ms >= m.start_ms && ms < m.end_ms - TAIL_MS) ?? null;
}

/** Where skipping a marker lands (milliseconds): the last moment of an intro, the end of credits. */
export function skipTarget(marker: Marker): number {
  const lead = marker.kind === "intro" ? Math.min(INTRO_LEAD_MS, (marker.end_ms - marker.start_ms) / 2) : 0;
  return marker.end_ms - lead;
}

/**
 * Credits that run to the end of their file, `durationMs` long (0 when
 * unknown, and then they don't). Only these bring Up Next: its countdown
 * would skip what follows other credits (an after-credits scene, a show's
 * last minutes), so they only offer Skip Credits.
 */
export function reachesEnd(marker: Marker, durationMs: number): boolean {
  return marker.kind === "credits" && durationMs > 0 && durationMs - marker.end_ms <= CREDITS_TAIL_MS;
}

/**
 * Skip Intro / Skip Credits behavior. Returns the marker the playhead is in.
 * Entering credits that run to the end of the file (`durationMs` long, 0
 * when unknown) calls `onCredits` (Up Next for episodes), once per segment,
 * so seeking back into them later lets them play.
 */
export function useSkipMarkers({
  markers,
  seconds,
  durationMs,
  enabled,
  onCredits,
}: {
  markers: Marker[];
  seconds: number;
  durationMs: number;
  enabled: boolean;
  onCredits: (marker: Marker) => void;
}): Marker | null {
  const marker = enabled ? activeMarker(markers, seconds * 1000) : null;
  const handled = useRef(new Set<string>());
  const key = marker ? `${marker.file_id}:${marker.kind}:${marker.start_ms}` : null;
  const reached = useEffectEvent((m: Marker) => onCredits(m));
  const final = marker !== null && reachesEnd(marker, durationMs);

  useEffect(() => {
    if (!marker || !key || !final || handled.current.has(key)) return;
    handled.current.add(key);
    reached(marker);
    // Runs when the playhead enters a different segment, or the file's
    // length becomes known inside the credits.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, final]);

  return marker;
}

/** Skip shows on its own this long after the playhead enters a segment; after that, only with the controls. */
export const SKIP_ALONE_MS = 8_000;

/**
 * Whether Skip shows on its own: for `SKIP_ALONE_MS` once the playhead is in
 * `marker` and Skip can show (`showable`: not while the stream still loads,
 * so a slow start inside a segment takes nothing from it), and again each
 * time the playhead enters a segment anew. Inside one the time counts on
 * whatever happens: a seek doesn't start it over, nor does the stream
 * loading again. After that Skip shows only with the controls, so it doesn't
 * sit over a whole intro or the credits.
 */
export function useSkipAlone(marker: Marker | null, showable: boolean): boolean {
  const key = marker ? `${marker.file_id}:${marker.kind}:${marker.start_ms}` : null;
  const [entered, setEntered] = useState(key);
  const [counting, setCounting] = useState(false);
  const [expired, setExpired] = useState(false);
  // A new segment (or none) starts over, at once rather than an effect later.
  if (key !== entered) {
    setEntered(key);
    setCounting(false);
    setExpired(false);
  } else if (key && showable && !counting) {
    setCounting(true);
  }
  useEffect(() => {
    if (!key || !counting) return;
    const timer = window.setTimeout(() => setExpired(true), SKIP_ALONE_MS);
    return () => window.clearTimeout(timer);
  }, [key, counting]);
  return key !== null && key === entered && !expired;
}

export function SkipButton({
  marker,
  onSkip,
  lifted,
  onHover,
}: {
  marker: Marker;
  onSkip: () => void;
  lifted: boolean;
  /** The mouse comes over the button (true) or leaves it: the controls stay while it is there, and Skip with them. */
  onHover: (over: boolean) => void;
}) {
  const { t } = useTranslation("player");
  const label = t(`skip.${marker.kind}`);
  const hovered = useRef(false);
  const hover = (over: boolean) => {
    hovered.current = over;
    onHover(over);
  };
  // A button that goes from under the mouse (skipped, or its segment over) hears no mouseleave.
  const gone = useEffectEvent(() => {
    if (hovered.current) onHover(false);
  });
  useEffect(() => () => gone(), []);
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        onSkip();
      }}
      onDoubleClick={(e) => e.stopPropagation()}
      onMouseEnter={() => hover(true)}
      onMouseLeave={() => hover(false)}
      title={`${label} (s)`}
      // Quiet over the picture: dark and see-through with a faint outline, firmer on hover or focus.
      className={cn(
        "absolute right-4 z-20 flex h-9 animate-fade-in items-center gap-1.5 rounded-md border border-white/25 bg-black/60 px-3.5 text-[13px] font-medium text-white/85 transition-[bottom,background-color,border-color,color] duration-300 hover:border-white/80 hover:bg-black/75 hover:text-white focus-visible:border-white/80 focus-visible:outline-none sm:right-8",
        lifted ? "bottom-28" : "bottom-10",
      )}
    >
      {label}
      <ChevronsRight className="size-3.5" />
    </button>
  );
}

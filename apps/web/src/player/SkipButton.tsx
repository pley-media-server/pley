import { useEffect, useEffectEvent, useRef } from "react";
import { ChevronsRight } from "lucide-react";
import type { ItemDetail, Marker } from "@/api/types";
import { cn } from "@/lib/cn";

/** The button disappears this long before the segment ends. */
const TAIL_MS = 1_000;
/** Skip Intro lands this long before the intro ends, so it plays out. */
const INTRO_LEAD_MS = 750;

/** What the button says; a remote offering the same skip says it too. */
export const SKIP_LABEL: Record<Marker["kind"], string> = {
  intro: "Skip Intro",
  credits: "Skip Credits",
};

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
  return markers.find((m) => Object.hasOwn(SKIP_LABEL, m.kind) && ms >= m.start_ms && ms < m.end_ms - TAIL_MS) ?? null;
}

/** Where skipping a marker lands (milliseconds): the last moment of an intro, the end of credits. */
export function skipTarget(marker: Marker): number {
  const lead = marker.kind === "intro" ? Math.min(INTRO_LEAD_MS, (marker.end_ms - marker.start_ms) / 2) : 0;
  return marker.end_ms - lead;
}

/**
 * Skip Intro / Skip Credits behaviour. Returns the marker the playhead is in.
 * Entering credits calls `onCredits` (Up Next for episodes), once per
 * segment, so seeking back into them later lets them play.
 */
export function useSkipMarkers({
  markers,
  seconds,
  enabled,
  onCredits,
}: {
  markers: Marker[];
  seconds: number;
  enabled: boolean;
  onCredits: (marker: Marker) => void;
}): Marker | null {
  const marker = enabled ? activeMarker(markers, seconds * 1000) : null;
  const handled = useRef(new Set<string>());
  const key = marker ? `${marker.file_id}:${marker.kind}:${marker.start_ms}` : null;
  const reached = useEffectEvent((m: Marker) => onCredits(m));

  useEffect(() => {
    if (!marker || !key || marker.kind !== "credits" || handled.current.has(key)) return;
    handled.current.add(key);
    reached(marker);
    // Runs when the playhead enters a different segment.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  return marker;
}

export function SkipButton({ marker, onSkip, lifted }: { marker: Marker; onSkip: () => void; lifted: boolean }) {
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation();
        onSkip();
      }}
      onDoubleClick={(e) => e.stopPropagation()}
      title={`${SKIP_LABEL[marker.kind]} (s)`}
      // Quiet over the picture: dark and see-through with a faint outline, firmer on hover or focus.
      className={cn(
        "absolute right-4 z-20 flex h-9 animate-fade-in items-center gap-1.5 rounded-md border border-white/25 bg-black/60 px-3.5 text-[13px] font-medium text-white/85 transition-[bottom,background-color,border-color,color] duration-300 hover:border-white/80 hover:bg-black/75 hover:text-white focus-visible:border-white/80 focus-visible:outline-none sm:right-8",
        lifted ? "bottom-28" : "bottom-10",
      )}
    >
      {SKIP_LABEL[marker.kind]}
      <ChevronsRight className="size-3.5" />
    </button>
  );
}

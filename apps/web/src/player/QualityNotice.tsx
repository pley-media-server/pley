import { useEffect, useState } from "react";
import { Gauge } from "lucide-react";
import { cn } from "@/lib/cn";

/** How long the notice shows, and how long it takes to fade. */
const SHOW_MS = 4_000;
const FADE_MS = 300;

export interface QualityNoticeText {
  text: string;
  /** Bumped for each notice, so the same words again show again. */
  n: number;
}

/**
 * What Automatic did, said once over the picture ("Playing at 8 Mbps 1080p
 * for a smoother picture"), then fading: at the top, clear of the
 * controls and Skip. Screen readers hear it once.
 */
export function QualityNotice({ notice }: { notice: QualityNoticeText | null }) {
  const [shown, setShown] = useState<{ n: number; leaving: boolean } | null>(null);
  const [heard, setHeard] = useState<number | null>(null);
  if (notice && notice.n !== heard) {
    setHeard(notice.n);
    setShown({ n: notice.n, leaving: false });
  }
  useEffect(() => {
    if (!shown || shown.leaving) return;
    const t = window.setTimeout(() => setShown({ n: shown.n, leaving: true }), SHOW_MS);
    return () => window.clearTimeout(t);
  }, [shown]);
  useEffect(() => {
    if (!shown?.leaving) return;
    const t = window.setTimeout(() => setShown(null), FADE_MS);
    return () => window.clearTimeout(t);
  }, [shown]);

  return (
    <div role="status" className="pointer-events-none absolute inset-x-0 top-20 z-20 flex justify-center px-4 sm:top-24">
      {shown && notice ? (
        <div
          key={shown.n}
          className={cn(
            "flex animate-pop-in items-center gap-2.5 rounded-full border border-white/20 bg-black/80 py-2 pl-3.5 pr-4 text-sm text-white shadow-2xl transition-opacity duration-300",
            shown.leaving && "opacity-0",
          )}
        >
          <Gauge className="size-4 shrink-0 text-accent" />
          <span>{notice.text}</span>
        </div>
      ) : null}
    </div>
  );
}

import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Play, X } from "lucide-react";
import type { ItemSummary } from "@/api/types";
import { Artwork } from "@/components/Artwork";
import { episodeLabel } from "@/lib/format";
import { thumbCandidates } from "@/lib/images";

const COUNTDOWN = 10;

/** `paused`: the viewer paused during the credits, and the countdown waits too. */
export function UpNext({
  next,
  paused,
  onPlay,
  onCancel,
}: {
  next: ItemSummary;
  paused: boolean;
  onPlay: () => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation("player");
  const [left, setLeft] = useState(COUNTDOWN);

  useEffect(() => {
    if (paused) return;
    const timer = window.setInterval(() => setLeft((l) => Math.max(0, l - 1)), 1000);
    return () => window.clearInterval(timer);
  }, [paused]);

  useEffect(() => {
    if (left <= 0) onPlay();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [left]);

  const progress = Math.max(0, Math.min(1, (COUNTDOWN - left) / COUNTDOWN));

  return (
    <div className="absolute bottom-28 right-4 z-30 w-[min(92vw,22rem)] animate-pop-in overflow-hidden rounded-xl border border-white/10 bg-black/90 text-white shadow-2xl sm:right-8">
      <div className="relative">
        <Artwork candidates={thumbCandidates(next)} width={352} alt="" title={next.title} className="aspect-video w-full" />
        <div className="absolute inset-0 bg-linear-to-t from-black/90 via-black/20 to-transparent" />
        <div className="absolute bottom-3 left-4 right-4">
          <div className="text-xs font-semibold text-accent">
            {next.kind === "episode" ? t("upNext.episode", { episode: episodeLabel(next) }) : t("upNext.title")}
          </div>
          <div className="truncate text-base font-semibold">{next.title}</div>
        </div>
      </div>
      <div className="flex items-center gap-2 p-3">
        <button
          type="button"
          onClick={onPlay}
          className="relative flex h-10 flex-1 items-center justify-center gap-2 overflow-hidden rounded-md bg-white/15 text-sm font-semibold transition hover:bg-white/25"
        >
          <span className="absolute inset-y-0 left-0 bg-accent/80 transition-[width] duration-1000 ease-linear" style={{ width: `${progress * 100}%` }} />
          <span className="relative flex items-center gap-2">
            <Play className="size-4 fill-current" /> {t("upNext.playNow")} {left > 0 ? `(${left})` : ""}
          </span>
        </button>
        <button
          type="button"
          aria-label={t("upNext.cancel")}
          onClick={onCancel}
          className="flex size-10 items-center justify-center rounded-md bg-white/10 transition hover:bg-white/20"
        >
          <X className="size-4" />
        </button>
      </div>
    </div>
  );
}

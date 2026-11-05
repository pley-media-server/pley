import { useRef, useState, type PointerEvent as ReactPointerEvent } from "react";
import { useTranslation } from "react-i18next";
import type { Chapter, Trickplay } from "@/api/types";
import { cn } from "@/lib/cn";
import { formatClock } from "@/lib/format";
import { chapterIndexAt, chapterTitle } from "./ChaptersMenu";

/** Width of the preview thumbnail on screen. */
const PREVIEW_WIDTH = 224;

export interface SeekBarProps {
  current: number;
  duration: number;
  /** Buffered ranges in seconds. */
  buffered: [number, number][];
  onSeek: (seconds: number) => void;
  /** Called while scrubbing (null when scrubbing ends). */
  onScrub?: (seconds: number | null) => void;
  /** Preview thumbnails, shown above the pointer when available. */
  previews?: { fileId: number; trickplay: Trickplay } | null;
  /** Chapter starts are marked on the bar and named in the preview. */
  chapters?: Chapter[];
}

/** "Chapter name · " for the chapter containing `seconds`, if the file has chapters. */
function chapterAt(chapters: Chapter[], seconds: number): string {
  if (chapters.length < 2) return "";
  const i = chapterIndexAt(chapters, seconds * 1000);
  const chapter = chapters[i];
  return chapter ? `${chapterTitle(chapter, i)} · ` : "";
}

/** Sprite-sheet tile showing the video at `seconds`. */
function Preview({ fileId, trickplay: tp, seconds }: { fileId: number; trickplay: Trickplay; seconds: number }) {
  const index = Math.min(tp.count - 1, Math.max(0, Math.floor((seconds * 1000) / tp.interval_ms)));
  const perSheet = tp.columns * tp.rows;
  const sheet = Math.floor(index / perSheet);
  const cell = index % perSheet;
  const scale = PREVIEW_WIDTH / tp.width;
  const height = Math.round(tp.height * scale);
  return (
    <div
      className="rounded-md bg-black bg-no-repeat shadow-lg ring-1 ring-white/20"
      style={{
        width: PREVIEW_WIDTH,
        height,
        backgroundImage: `url(/api/files/${fileId}/trickplay/${sheet}.jpg?v=${encodeURIComponent(tp.id)})`,
        backgroundSize: `${tp.columns * PREVIEW_WIDTH}px ${tp.rows * height}px`,
        backgroundPosition: `-${(cell % tp.columns) * PREVIEW_WIDTH}px -${Math.floor(cell / tp.columns) * height}px`,
      }}
    />
  );
}

export function SeekBar({ current, duration, buffered, onSeek, onScrub, previews, chapters = [] }: SeekBarProps) {
  const { t } = useTranslation("player");
  const ref = useRef<HTMLDivElement>(null);
  const [hover, setHover] = useState<{ x: number; t: number } | null>(null);
  const [drag, setDrag] = useState<number | null>(null);
  /** The bar's width at the last pointer event (to place the preview). */
  const [barWidth, setBarWidth] = useState(0);

  const timeAt = (clientX: number) => {
    const el = ref.current;
    if (!el || duration <= 0) return { x: 0, t: 0, width: 0 };
    const rect = el.getBoundingClientRect();
    const x = Math.min(Math.max(clientX - rect.left, 0), rect.width);
    return { x, t: (x / rect.width) * duration, width: rect.width };
  };

  /** Keep the preview inside the bar so it is never cut off at the edges. */
  const previewLeft = (x: number) => {
    const half = previews && previews.trickplay.count > 0 ? PREVIEW_WIDTH / 2 : 24;
    return `${barWidth > half * 2 ? Math.min(Math.max(x, half), barWidth - half) : x}px`;
  };

  const shown = drag ?? current;
  const pct = duration > 0 ? Math.min(100, (shown / duration) * 100) : 0;

  const onPointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (duration <= 0 || e.button !== 0) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    const { t, width } = timeAt(e.clientX);
    setBarWidth(width);
    setDrag(t);
    onScrub?.(t);
  };
  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const { x, t, width } = timeAt(e.clientX);
    setBarWidth(width);
    setHover({ x, t });
    if (drag != null) {
      setDrag(t);
      onScrub?.(t);
    }
  };
  const onPointerUp = (e: ReactPointerEvent<HTMLDivElement>) => {
    if (drag == null) return;
    const { t } = timeAt(e.clientX);
    setDrag(null);
    onScrub?.(null);
    onSeek(t);
  };

  return (
    <div
      ref={ref}
      role="slider"
      tabIndex={0}
      aria-label={t("controls.seek")}
      aria-valuemin={0}
      aria-valuemax={Math.round(duration)}
      aria-valuenow={Math.round(shown)}
      aria-valuetext={t("controls.seekValue", { time: formatClock(shown), duration: formatClock(duration) })}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={() => {
        setDrag(null);
        onScrub?.(null);
      }}
      onPointerLeave={() => setHover(null)}
      onKeyDown={(e) => {
        // Arrow keys are handled globally by the player; stop the page from scrolling.
        if (e.key === "Home") {
          e.preventDefault();
          onSeek(0);
        } else if (e.key === "End") {
          e.preventDefault();
          onSeek(Math.max(0, duration - 1));
        }
      }}
      className="group/seek relative flex h-6 cursor-pointer touch-none items-center"
    >
      <div
        className={cn(
          "relative h-1 w-full overflow-hidden rounded-full bg-white/20 transition-[height] duration-150 group-hover/seek:h-1.5",
          drag != null && "h-1.5",
        )}
      >
        {duration > 0
          ? buffered.map(([s, e], i) => (
              <div
                key={i}
                className="absolute inset-y-0 bg-white/30"
                style={{ left: `${(s / duration) * 100}%`, width: `${Math.max(0, ((e - s) / duration) * 100)}%` }}
              />
            ))
          : null}
        {hover && drag == null ? (
          <div className="absolute inset-y-0 left-0 bg-white/20" style={{ width: `${hover.x}px` }} />
        ) : null}
        <div className="absolute inset-y-0 left-0 bg-accent" style={{ width: `${pct}%` }} />
        {duration > 0
          ? chapters
              .filter((c) => c.start_ms > 1000 && c.start_ms / 1000 < duration - 1)
              .map((c) => (
                <div
                  key={c.start_ms}
                  className="absolute inset-y-0 w-0.5 -translate-x-1/2 bg-black/70"
                  style={{ left: `${(c.start_ms / 1000 / duration) * 100}%` }}
                />
              ))
          : null}
      </div>
      <div
        className={cn(
          "pointer-events-none absolute top-1/2 size-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-accent shadow-md transition-transform duration-150",
          drag != null ? "scale-125" : "scale-0 group-hover/seek:scale-100",
        )}
        style={{ left: `${pct}%` }}
      />
      {(hover || drag != null) && duration > 0 ? (
        <div
          className="pointer-events-none absolute bottom-full mb-2 flex -translate-x-1/2 flex-col items-center gap-1.5"
          style={{ left: previewLeft(drag != null ? (pct / 100) * barWidth : (hover?.x ?? 0)) }}
        >
          {previews && previews.trickplay.count > 0 ? (
            <Preview fileId={previews.fileId} trickplay={previews.trickplay} seconds={drag ?? hover?.t ?? 0} />
          ) : null}
          <div className="max-w-56 truncate rounded bg-black/85 px-2 py-1 text-xs font-medium tabular-nums text-white shadow">
            {chapterAt(chapters, drag ?? hover?.t ?? 0)}
            {formatClock(drag ?? hover?.t ?? 0)}
          </div>
        </div>
      ) : null}
    </div>
  );
}

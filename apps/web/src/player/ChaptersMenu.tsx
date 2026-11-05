import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ListOrdered } from "lucide-react";
import type { Chapter } from "@/api/types";
import { t as translate } from "@/i18n";
import { cn } from "@/lib/cn";
import { formatClock } from "@/lib/format";

export function chapterTitle(chapter: Chapter, index: number): string {
  return chapter.title?.trim() || translate("chapters.numbered", { ns: "player", number: index + 1 });
}

/** Index of the chapter playing at `ms` (chapters are sorted by start), or -1. */
export function chapterIndexAt(chapters: Chapter[], ms: number): number {
  let found = -1;
  for (const [i, chapter] of chapters.entries()) {
    if (chapter.start_ms > ms) break;
    found = i;
  }
  return found;
}

/** A chapter's picture, or an empty frame until it loads (or when it fails). */
function ChapterPicture({ url }: { url?: string }) {
  const [failed, setFailed] = useState(false);
  return (
    <span className="relative aspect-video w-24 shrink-0 overflow-hidden rounded bg-white/10">
      {url && !failed ? (
        <img src={url} alt="" loading="lazy" decoding="async" draggable={false} onError={() => setFailed(true)} className="absolute inset-0 size-full object-cover" />
      ) : null}
    </span>
  );
}

/** Chapter list above the control bar; picking one seeks to its start. */
export function ChaptersMenu({
  chapters,
  current,
  open,
  setOpen,
  onSeek,
}: {
  chapters: Chapter[];
  /** Playhead in seconds. */
  current: number;
  open: boolean;
  setOpen: (open: boolean) => void;
  onSeek: (seconds: number) => void;
}) {
  const { t } = useTranslation("player");
  const ref = useRef<HTMLDivElement>(null);
  const activeRef = useRef<HTMLButtonElement>(null);
  const currentMs = current * 1000;
  const active = chapterIndexAt(chapters, currentMs + 250);
  const pictures = chapters.some((c) => c.thumb_url);

  useEffect(() => {
    if (!open) return;
    activeRef.current?.scrollIntoView({ block: "nearest" });
    const onDown = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        setOpen(false);
      }
    };
    document.addEventListener("pointerdown", onDown);
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("pointerdown", onDown);
      document.removeEventListener("keydown", onKey, true);
    };
  }, [open, setOpen]);

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        aria-label={t("chapters.title")}
        title={t("chapters.title")}
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className={cn("flex size-10 items-center justify-center rounded-full transition hover:bg-white/15", open && "bg-white/15")}
      >
        <ListOrdered className="size-5" />
      </button>
      {open ? (
        <div
          className={cn(
            "absolute bottom-full right-0 mb-2 max-h-[min(60vh,28rem)] animate-pop-in overflow-y-auto rounded-lg border border-white/10 bg-black/90 p-1.5 shadow-2xl",
            pictures ? "w-[min(24rem,calc(100vw-2rem))]" : "w-72",
          )}
        >
          <div className="px-2.5 pb-1.5 pt-1 text-xs font-medium text-white/60">{t("chapters.title")}</div>
          {chapters.map((chapter, i) => (
            <button
              key={`${chapter.start_ms}-${i}`}
              ref={i === active ? activeRef : undefined}
              type="button"
              onClick={() => {
                onSeek(chapter.start_ms / 1000);
                setOpen(false);
              }}
              className={cn(
                "flex w-full items-center gap-3 rounded-md px-2.5 py-2 text-left text-sm transition-colors hover:bg-white/10",
                i === active ? "text-accent" : "text-white/90",
              )}
            >
              {pictures ? (
                <ChapterPicture url={chapter.thumb_url} />
              ) : (
                <span className="w-12 shrink-0 tabular-nums text-xs text-white/50">{formatClock(chapter.start_ms / 1000)}</span>
              )}
              <span className="min-w-0 flex-1">
                <span className="block truncate">{chapterTitle(chapter, i)}</span>
                {pictures ? (
                  <span className="block tabular-nums text-xs text-white/50">{formatClock(chapter.start_ms / 1000)}</span>
                ) : null}
              </span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

import { useEffect, useEffectEvent, useLayoutEffect, useMemo, useState } from "react";
import { useWindowVirtualizer } from "@tanstack/react-virtual";
import type { ItemSummary } from "@/api/types";
import { cn } from "@/lib/cn";
import { useElementWidth } from "@/lib/hooks";
import { aspectHeight, CardSkeleton, MediaCard, type CardAspect } from "./cards";

const TEXT_HEIGHT = 46; // title + subtitle under each card
const LETTERS = ["#", ..."ABCDEFGHIJKLMNOPQRSTUVWXYZ"];

function letterOf(item: ItemSummary): string {
  const ch = (item.sort_title || item.title || "").trim().charAt(0).toUpperCase();
  const plain = ch.normalize("NFD").replace(/[̀-ͯ]/g, "");
  return plain >= "A" && plain <= "Z" ? plain : "#";
}

export interface ItemGridProps {
  items: ItemSummary[];
  /** Number of cards, when more than `items` (the rest show as placeholders). */
  total?: number;
  aspect: CardAspect;
  /** Target card width in CSS px; the grid fits as many columns as possible. */
  cardWidth: number;
  /** Show the A–Z jump bar (only meaningful when sorted by title). */
  jumpBar?: boolean;
  /** Extra offset for sticky headers when jumping (px). */
  stickyOffset?: number;
  /** Cards put their files' bitrate and size under the title (sorted by bitrate). */
  media?: boolean;
  /** Posters carry their quality badges ("4K", "DV"): sorted by bitrate or filtered by quality. */
  badges?: boolean;
}

/** Columns and gaps of a grid `width` px wide with cards of about `cardWidth` px: at least two columns. */
function gridLayout(width: number, cardWidth: number) {
  const gap = width < 640 ? 12 : 18;
  const rowGap = width < 640 ? 18 : 26;
  const cols = width > 0 ? Math.max(2, Math.floor((width + gap) / (cardWidth + gap))) : 0;
  const colWidth = cols > 0 ? (width - gap * (cols - 1)) / cols : cardWidth;
  return { gap, rowGap, cols, colWidth };
}

/** Window-scrolled, row-virtualized responsive grid of media cards. */
export function ItemGrid({ items, total = items.length, aspect, cardWidth, jumpBar, stickyOffset = 136, media, badges }: ItemGridProps) {
  const [ref, width] = useElementWidth<HTMLDivElement>();
  const [scrollMargin, setScrollMargin] = useState(0);
  const { gap, rowGap, cols, colWidth } = gridLayout(width, cardWidth);
  const rowHeight = Math.round(aspectHeight(aspect, colWidth) + TEXT_HEIGHT + rowGap);
  const count = Math.max(total, items.length);
  const rowCount = cols > 0 ? Math.ceil(count / cols) : 0;

  // Distance between the top of the document and the grid: measured again
  // when the grid's width (the toolbar above may wrap) or size changes.
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const top = Math.round(el.getBoundingClientRect().top + window.scrollY);
    if (top !== scrollMargin) setScrollMargin(top);
  }, [ref, scrollMargin, width, count]);

  const virtualizer = useWindowVirtualizer({
    count: rowCount,
    estimateSize: () => rowHeight,
    overscan: 4,
    scrollMargin,
    scrollPaddingStart: stickyOffset,
  });

  const remeasure = useEffectEvent(() => virtualizer.measure());
  useEffect(() => {
    remeasure();
  }, [rowHeight, cols]);

  const letterIndex = useMemo(() => {
    // Letter positions are only known once every item is in.
    if (!jumpBar || items.length < total) return null;
    const map = new Map<string, number>();
    items.forEach((item, i) => {
      const l = letterOf(item);
      if (!map.has(l)) map.set(l, i);
    });
    return map;
  }, [items, total, jumpBar]);

  const jump = (letter: string) => {
    const idx = letterIndex?.get(letter);
    if (idx == null || cols === 0) return;
    virtualizer.scrollToIndex(Math.floor(idx / cols), { align: "start" });
  };

  return (
    <div className="relative">
      <div ref={ref} className={cn("relative w-full", jumpBar && "md:pr-7")} style={{ height: virtualizer.getTotalSize() }}>
        {cols > 0
          ? virtualizer.getVirtualItems().map((row) => {
              const start = row.index * cols;
              const rowItems = items.slice(start, start + cols);
              const placeholders = Math.min(cols, count - start) - rowItems.length;
              return (
                <div
                  key={row.key}
                  className="absolute left-0 top-0 grid w-full"
                  style={{
                    transform: `translateY(${row.start - virtualizer.options.scrollMargin}px)`,
                    gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`,
                    columnGap: gap,
                    height: rowHeight,
                  }}
                >
                  {rowItems.map((item) => (
                    <MediaCard key={item.id} item={item} aspect={aspect} width={colWidth} media={media} badges={badges} />
                  ))}
                  {Array.from({ length: placeholders }, (_, i) => (
                    <CardSkeleton key={`p${i}`} aspect={aspect} />
                  ))}
                </div>
              );
            })
          : null}
      </div>
      {jumpBar && letterIndex && count > 30 ? (
        <div className="fixed right-1.5 top-1/2 z-10 hidden -translate-y-1/2 flex-col items-center md:flex">
          {LETTERS.map((l) => {
            const enabled = letterIndex.has(l);
            return (
              <button
                key={l}
                type="button"
                disabled={!enabled}
                onClick={() => jump(l)}
                className={cn(
                  "w-6 py-[1px] text-center text-[10.5px] font-semibold leading-[1.35] transition-colors",
                  enabled ? "text-muted hover:text-accent" : "text-subtle/40",
                )}
              >
                {l}
              </button>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}

/** Placeholder cards laid out as the grid will lay out the real ones, so nothing moves when they arrive. */
export function ItemGridSkeleton({ aspect, cardWidth }: { aspect: CardAspect; cardWidth: number }) {
  const [ref, width] = useElementWidth<HTMLDivElement>();
  const { gap, rowGap, cols } = gridLayout(width, cardWidth);
  const count = cols > 0 ? cols * Math.ceil(18 / cols) : 0;
  return (
    <div
      ref={ref}
      className="grid w-full"
      style={{ gridTemplateColumns: `repeat(${Math.max(cols, 1)}, minmax(0, 1fr))`, columnGap: gap, rowGap }}
    >
      {Array.from({ length: count }, (_, i) => (
        <CardSkeleton key={i} aspect={aspect} />
      ))}
    </div>
  );
}

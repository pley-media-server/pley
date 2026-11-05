import { useCallback, useLayoutEffect, useRef, useState } from "react";
import { Link } from "react-router";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Hub } from "@/api/types";
import { cn } from "@/lib/cn";
import { smoothScroll, useMediaQuery } from "@/lib/hooks";
import { aspectFor, CardSkeleton, MediaCard, type CardAspect } from "./cards";
import { libraryUrl } from "@/lib/urls";

/** Smallest card width per shape: [mobile, desktop]. Cards grow from there to fill the row. */
const MIN_WIDTHS: Record<CardAspect, [number, number]> = {
  poster: [118, 160],
  square: [132, 168],
  thumb: [236, 296],
};
/** Gap between cards: `gap-3.5` / `sm:gap-4`. */
const GAPS: [number, number] = [14, 16];

export function hubAspect(hub: Hub): CardAspect {
  if (hub.key.startsWith("continue")) return "thumb";
  const first = hub.items[0];
  return first ? aspectFor(first) : "poster";
}

function hubLink(hub: Hub): string | null {
  if (hub.library_id == null) return null;
  if (hub.key.startsWith("recently_added")) return libraryUrl(hub.library_id, "tab=library&sort=added&order=desc");
  if (hub.key.startsWith("recently_released")) return libraryUrl(hub.library_id, "tab=library&sort=release&order=desc");
  if (hub.key.startsWith("top_rated")) return libraryUrl(hub.library_id, "tab=library&sort=rating&order=desc");
  return null;
}

/**
 * Card width that fits a whole number of cards into `available` px, so no
 * card is ever cut off at the edge. Big screens get somewhat larger cards.
 */
export function fitCards(available: number, minWidth: number, gap: number): { width: number; perPage: number } {
  const scaled = minWidth * Math.min(1.35, Math.max(1, available / 1800));
  const perPage = Math.max(1, Math.floor((available + gap) / (scaled + gap)));
  return { width: Math.floor((available - gap * (perPage - 1)) / perPage), perPage };
}

/**
 * Measures the content width of a padded row and fits cards of at least
 * `minWidths` ([mobile, desktop]) into it.
 */
export function useFittedRow(minWidths: [number, number]) {
  const desktop = useMediaQuery("(min-width: 640px)");
  const minWidth = minWidths[desktop ? 1 : 0];
  const gap = GAPS[desktop ? 1 : 0];
  const ref = useRef<HTMLDivElement>(null);
  const [available, setAvailable] = useState(0);

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const measure = () => {
      const style = getComputedStyle(el);
      setAvailable(el.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight));
    };
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const fit = available > 0 ? fitCards(available, minWidth, gap) : { width: minWidth, perPage: 1 };
  return { ref, gap, ...fit };
}

function useFittedCards(aspect: CardAspect) {
  return useFittedRow(MIN_WIDTHS[aspect]);
}

/** `inset`: inside a page that already has the side margin (item pages). */
export function HubRow({ hub, inset }: { hub: Hub; inset?: boolean }) {
  const { t } = useTranslation("components");
  const aspect = hubAspect(hub);
  const { ref: scroller, width, perPage, gap } = useFittedCards(aspect);
  const [canLeft, setCanLeft] = useState(false);
  const [canRight, setCanRight] = useState(false);
  const continueStyle = hub.key.startsWith("continue");
  const link = hubLink(hub);

  const update = useCallback(() => {
    const el = scroller.current;
    if (!el) return;
    setCanLeft(el.scrollLeft > 4);
    setCanRight(el.scrollLeft + el.clientWidth < el.scrollWidth - 4);
  }, [scroller]);

  useLayoutEffect(() => {
    update();
  }, [update, hub.items.length, width]);

  const scrollPage = (dir: 1 | -1) => {
    scroller.current?.scrollBy({ left: dir * perPage * (width + gap), behavior: smoothScroll() });
  };

  if (!hub.items.length) return null;
  const scrollable = canLeft || canRight;

  return (
    <section>
      <div className={cn("mb-2 flex items-center justify-between gap-4", !inset && "px-4 sm:px-8")}>
        <h2 className="truncate text-lg font-semibold tracking-tight">
          {link ? (
            <Link to={link} className="transition-colors hover:text-accent">
              {hub.title}
            </Link>
          ) : (
            hub.title
          )}
        </h2>
        <div className="flex shrink-0 items-center gap-1">
          {link ? (
            <Link to={link} className="mr-2 text-xs font-medium text-muted transition-colors hover:text-accent">
              {t("hubRow.seeAll")}
            </Link>
          ) : null}
          {scrollable ? (
            <>
              <ArrowButton side="left" enabled={canLeft} onClick={() => scrollPage(-1)} />
              <ArrowButton side="right" enabled={canRight} onClick={() => scrollPage(1)} />
            </>
          ) : null}
        </div>
      </div>
      {/* The page margin sits outside the scroll box (a scroll box shows content
          in its own padding, which would reveal a sliver of the next card); the
          4px inset only keeps hover outlines from being clipped. */}
      <div className={inset ? undefined : "px-4 sm:px-8"}>
        <div
          ref={scroller}
          onScroll={update}
          className="scrollbar-none -mx-1 flex snap-x snap-mandatory scroll-px-1 gap-3.5 overflow-x-auto px-1 pt-1 pb-2 sm:gap-4"
        >
          {hub.items.map((item) => (
            <div key={item.id} className="shrink-0 snap-start">
              <MediaCard item={item} aspect={aspect} width={width} fixed continueStyle={continueStyle} />
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

export function ArrowButton({ side, enabled, onClick }: { side: "left" | "right"; enabled: boolean; onClick: () => void }) {
  const { t } = useTranslation();
  const Icon = side === "left" ? ChevronLeft : ChevronRight;
  return (
    <button
      type="button"
      aria-label={side === "left" ? t("actions.previous") : t("actions.next")}
      onClick={onClick}
      disabled={!enabled}
      className={cn(
        "flex size-8 items-center justify-center rounded-full text-fg transition-colors",
        enabled ? "hover:bg-surface-3 hover:text-accent" : "opacity-30",
      )}
    >
      <Icon className="size-5" />
    </button>
  );
}

export function HubRowSkeleton({ aspect = "poster" }: { aspect?: CardAspect }) {
  const { ref, width, perPage } = useFittedCards(aspect);
  return (
    <section>
      <div className="mb-2 flex h-8 items-center px-4 sm:px-8">
        <div className="skeleton h-5 w-48 rounded" />
      </div>
      <div className="px-4 sm:px-8">
        <div ref={ref} className="-mx-1 flex gap-3.5 overflow-hidden px-1 pt-1 pb-2 sm:gap-4">
          {Array.from({ length: Math.max(perPage, 4) }, (_, i) => (
            <div key={i} className="shrink-0">
              <CardSkeleton aspect={aspect} width={width} fixed />
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

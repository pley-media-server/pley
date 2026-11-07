import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import { Link } from "react-router";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { Check, Info, Play, Plus } from "lucide-react";
import { errorMessage } from "@/api/client";
import { queries, useItem, useLibraries, useSetInWatchlist } from "@/api/hooks";
import type { ItemDetail } from "@/api/types";
import { Artwork } from "@/components/Artwork";
import { useToast } from "@/components/Toast";
import type { Featured } from "@/lib/featured";
import { cn } from "@/lib/cn";
import { formatDuration, formatRemaining } from "@/lib/format";
import { useMediaQuery } from "@/lib/hooks";
import { backdropCandidates } from "@/lib/images";
import { itemUrl } from "@/lib/urls";
import { usePlayItem } from "@/lib/usePlayItem";
import { ContentRating, RatingBadge } from "./item/shared";

/** How long each title stays before the next comes. */
const TURN_MS = 9000;
/** A touch moved this far sideways is a swipe to the next or previous title. */
const SWIPE_PX = 48;

/** The banner's height; the rows below rise into its bottom by `ROWS_OVERLAP`. */
const HEIGHT = "h-[74svh] min-h-[500px] max-h-[860px] sm:h-[min(86vh,50vw)] sm:min-h-[540px]";
export const ROWS_OVERLAP = "-mt-14 sm:-mt-24";

function usePageVisible(): boolean {
  const [visible, setVisible] = useState(() => document.visibilityState !== "hidden");
  useEffect(() => {
    const on = () => setVisible(document.visibilityState !== "hidden");
    document.addEventListener("visibilitychange", on);
    return () => document.removeEventListener("visibilitychange", on);
  }, []);
  return visible;
}

/**
 * The home screen's banner: the featured titles' backdrops, full width under
 * the top bar, one at a time with its name, plot and Play. It turns to the
 * next every few seconds, except while the pointer or focus is on it, the
 * page is hidden, or the system asks for less motion.
 */
export function HomeBanner({ featured }: { featured: Featured[] }) {
  const { t } = useTranslation("pages");
  const qc = useQueryClient();
  const [index, setIndex] = useState(0);
  // Titles whose backdrop has been asked for: the one shown, the next, and those shown before (to fade out).
  const [warm, setWarm] = useState(() => new Set([0, 1]));
  const [hovered, setHovered] = useState(false);
  const [focused, setFocused] = useState(false);
  // The first picture eases into its zoom too, rather than starting zoomed in.
  const [started, setStarted] = useState(false);
  const visible = usePageVisible();
  const calm = useMediaQuery("(prefers-reduced-motion: reduce)");
  const touch = useRef<{ x: number; y: number } | null>(null);

  const count = featured.length;
  const at = Math.min(index, count - 1);
  const current = featured[at]!;
  const rotating = count > 1 && !calm;
  const paused = hovered || focused || !visible;

  useEffect(() => {
    const id = requestAnimationFrame(() => setStarted(true));
    return () => cancelAnimationFrame(id);
  }, []);

  // Every featured title's details (plot, genres, rating) load up front: turning to one shows it whole.
  useEffect(() => {
    for (const f of featured) void qc.prefetchQuery(queries.item(f.item.id));
  }, [qc, featured]);

  const go = (to: number) => {
    const next = (to + count) % count;
    setIndex(next);
    setWarm((w) => (w.has(next) && w.has((next + 1) % count) ? w : new Set([...w, next, (next + 1) % count])));
  };

  return (
    <section
      data-hero
      aria-roledescription="carousel"
      aria-label={t("home.banner.label")}
      className={cn("relative -mt-16 touch-pan-y overflow-hidden bg-bg select-none", HEIGHT)}
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onFocus={() => setFocused(true)}
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget)) setFocused(false);
      }}
      onPointerDown={(e) => {
        touch.current = e.pointerType === "mouse" ? null : { x: e.clientX, y: e.clientY };
      }}
      onPointerUp={(e) => {
        const from = touch.current;
        touch.current = null;
        if (!from || count < 2) return;
        const dx = e.clientX - from.x;
        if (Math.abs(dx) > SWIPE_PX && Math.abs(dx) > Math.abs(e.clientY - from.y)) go(at + (dx < 0 ? 1 : -1));
      }}
    >
      {featured.map((f, i) => (
        <div
          key={f.item.id}
          aria-hidden
          className={cn(
            "absolute inset-0 transition-opacity duration-1000 ease-out",
            i === at ? "opacity-100" : "opacity-0",
          )}
        >
          <Artwork
            candidates={warm.has(i) ? backdropCandidates(f.item) : []}
            width={1920}
            alt=""
            title={f.item.title}
            bare
            eager
            className={cn(
              "size-full transition-transform duration-[20s] ease-out will-change-transform motion-reduce:scale-100 motion-reduce:transition-none",
              i === at && started ? "scale-110" : "scale-[1.02]",
            )}
            imgClassName="object-cover object-[center_30%]"
          />
        </div>
      ))}

      {/* Shades that keep the words and the top bar readable on any picture, and melt it into the page. */}
      <div aria-hidden className="pointer-events-none absolute inset-x-0 top-0 h-36 bg-linear-to-b from-black/65 to-transparent" />
      {/* Phones put the words in the middle of the picture, where only a dimmer picture keeps them readable. */}
      <div aria-hidden className="pointer-events-none absolute inset-0 bg-black/25 sm:hidden" />
      <div
        aria-hidden
        className="pointer-events-none absolute inset-y-0 left-0 hidden w-[78%] bg-linear-to-r from-bg/95 via-bg/60 to-transparent sm:block"
      />
      <div
        aria-hidden
        className="pointer-events-none absolute inset-x-0 bottom-0 h-[80%] bg-linear-to-t from-bg from-12% via-bg/75 via-50% to-transparent sm:h-[60%] sm:from-14% sm:via-bg/55"
      />

      <div className="absolute inset-x-0 bottom-0 flex flex-col items-center gap-5 px-4 pb-20 sm:flex-row sm:items-end sm:justify-between sm:gap-6 sm:px-8 sm:pb-32 xl:pb-36">
        <Slide key={current.item.id} featured={current} />
        {count > 1 ? (
          <div className="flex shrink-0 gap-1.5 sm:mb-3">
            {featured.map((f, i) => (
              <button
                key={f.item.id}
                type="button"
                onClick={() => go(i)}
                aria-label={t("home.banner.show", { title: f.item.title })}
                aria-current={i === at}
                className="group/turn flex h-6 items-center px-0.5"
              >
                <span
                  className={cn(
                    "relative block h-1 overflow-hidden rounded-full bg-white/25 transition-[width,background-color] duration-500 group-hover/turn:bg-white/50",
                    i === at ? "w-10" : "w-4",
                  )}
                >
                  {i === at ? (
                    <span
                      className={cn("absolute inset-0 origin-left rounded-full bg-white", rotating && "animate-hero-turn")}
                      style={rotating ? { animationDuration: `${TURN_MS}ms`, animationPlayState: paused ? "paused" : "running" } : undefined}
                      onAnimationEnd={() => go(at + 1)}
                    />
                  ) : null}
                </span>
              </button>
            ))}
          </div>
        ) : null}
      </div>
    </section>
  );
}

/** One title's words and buttons, rising in when it comes. */
function Slide({ featured: { item, from } }: { featured: Featured }) {
  const { t } = useTranslation(["pages", "common", "item"]);
  const play = usePlayItem();
  const libs = useLibraries();
  const detail = useItem(item.id).data;
  const library = libs.data?.find((l) => l.id === item.library_id)?.name;
  const resume = item.kind === "movie" && item.user_state.position_ms > 0;
  // Longer names come smaller, so the name never crowds out the rest.
  const size =
    item.title.length > 40
      ? "text-3xl sm:text-4xl xl:text-5xl"
      : item.title.length > 22
        ? "text-4xl sm:text-5xl xl:text-6xl"
        : "text-5xl sm:text-6xl xl:text-7xl";

  return (
    <div className="max-w-2xl min-w-0 animate-hero-rise text-center sm:text-left">
      <div className="mb-3 flex items-center justify-center gap-2 text-[11px] font-semibold uppercase tracking-[0.2em] text-white/85 [text-shadow:0_1px_8px_rgb(0_0_0/0.6)] sm:mb-4 sm:justify-start sm:text-xs">
        {from === "list" ? t("home.banner.onMyList") : library ? t("home.banner.newIn", { library }) : t("home.banner.new")}
      </div>
      <h2
        className={cn(
          "font-extrabold leading-[1.05] tracking-[-0.025em] text-white [text-shadow:0_2px_24px_rgb(0_0_0/0.45)] [text-wrap:balance]",
          size,
        )}
      >
        <Link to={itemUrl(item.id)} className="outline-none focus-visible:underline">
          {item.title}
        </Link>
      </h2>
      <Details item={item} detail={detail} />
      {detail?.summary ? (
        <p className="mx-auto mt-3 line-clamp-2 max-w-xl text-sm leading-relaxed sm:mt-4 text-white/80 sm:mx-0 sm:line-clamp-3 sm:text-base">
          {detail.summary}
        </p>
      ) : null}
      <div className="mt-6 flex items-center justify-center gap-3 sm:justify-start">
        <button
          type="button"
          onClick={() => void play(item)}
          className="inline-flex h-12 items-center gap-2.5 rounded-md bg-white px-6 text-[15px] font-semibold text-black shadow-lg shadow-black/30 transition hover:bg-white/85 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent active:scale-[0.98] sm:px-7 sm:text-base"
        >
          <Play className="size-5 fill-current" aria-hidden />
          {resume ? t("common:actions.resume") : t("common:actions.play")}
          {resume ? (
            <span className="font-normal text-black/60">{formatRemaining(item.user_state.position_ms, item.duration_ms)}</span>
          ) : null}
        </button>
        <Link
          to={itemUrl(item.id)}
          className="inline-flex h-12 items-center gap-2.5 rounded-md bg-white/15 px-5 text-[15px] font-semibold text-white ring-1 ring-white/15 backdrop-blur-md transition hover:bg-white/25 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent sm:px-6 sm:text-base"
        >
          <Info className="size-5" aria-hidden />
          {t("home.banner.moreInfo")}
        </Link>
        {detail ? <ListButton item={detail} /> : null}
      </div>
    </div>
  );
}

/** Year, length or seasons, age rating, rating and genres, as far as the details say. */
function Details({ item, detail }: { item: Featured["item"]; detail: ItemDetail | undefined }) {
  const { t } = useTranslation("item");
  const seasons = detail?.child_count ?? item.child_count;
  const parts: ReactNode[] = [
    item.year,
    item.kind === "show"
      ? seasons
        ? t("counts.seasons", { count: seasons })
        : null
      : item.duration_ms
        ? formatDuration(item.duration_ms)
        : null,
    detail?.content_rating ? <ContentRating key="cr" value={detail.content_rating} /> : null,
    detail?.rating != null && detail.rating > 0 ? <RatingBadge key="r" rating={detail.rating} /> : null,
    ...(detail?.genres.slice(0, 3) ?? []),
  ].filter((p) => p != null && p !== "");
  // Centred on phones, so dots go between the facts rather than before each (as MetaLine does).
  return (
    <div className="mt-4 flex flex-wrap items-center justify-center gap-x-2.5 gap-y-1 text-sm text-white/85 sm:justify-start sm:text-[15px]">
      {parts.map((part, i) => (
        <Fragment key={i}>
          {i > 0 ? (
            <span aria-hidden className="text-white/40">
              ·
            </span>
          ) : null}
          <span className="inline-flex items-center">{part}</span>
        </Fragment>
      ))}
    </div>
  );
}

function ListButton({ item }: { item: ItemDetail }) {
  const { t } = useTranslation("item");
  const setInWatchlist = useSetInWatchlist();
  const toast = useToast();
  const inList = !!item.in_watchlist;
  const label = inList ? t("myList.remove") : t("myList.add");
  return (
    <button
      type="button"
      aria-label={label}
      aria-pressed={inList}
      title={label}
      onClick={() => setInWatchlist.mutate({ item, on: !inList }, { onError: (e) => toast.error(errorMessage(e)) })}
      className="flex size-10 shrink-0 items-center justify-center rounded-full border-[1.5px] border-white/40 bg-black/25 text-white backdrop-blur-md transition hover:border-white focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
    >
      {inList ? <Check className="size-[18px] text-accent" strokeWidth={2.5} aria-hidden /> : <Plus className="size-[18px]" strokeWidth={2.5} aria-hidden />}
    </button>
  );
}

/** The banner's place while the home screen loads. */
export function HomeBannerSkeleton() {
  return (
    <div data-hero aria-hidden className={cn("relative -mt-16 overflow-hidden bg-linear-to-b from-surface-2/70 to-bg", HEIGHT)}>
      <div className="absolute inset-x-0 bottom-0 flex flex-col items-center gap-4 px-4 pb-20 sm:items-start sm:px-8 sm:pb-32 xl:pb-36">
        <div className="skeleton h-3 w-32 rounded" />
        <div className="skeleton h-14 w-[min(28rem,80%)] rounded-md" />
        <div className="skeleton h-4 w-56 rounded" />
        <div className="mt-2 flex gap-3">
          <div className="skeleton h-12 w-32 rounded-md" />
          <div className="skeleton h-12 w-36 rounded-md" />
        </div>
      </div>
    </div>
  );
}

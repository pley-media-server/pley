import { memo } from "react";
import { Link } from "react-router";
import { Check, Play } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ItemSummary } from "@/api/types";
import { cn } from "@/lib/cn";
import { cardSubtitle, episodeLabel, formatRemaining, mediaBadges, mediaLine, progressFraction, seasonTitle, unwatchedCount } from "@/lib/format";
import { posterCandidates, thumbCandidates } from "@/lib/images";
import { prefetchPlay } from "@/lib/prefetchPlay";
import { usePlayItem } from "@/lib/usePlayItem";
import { playIntent } from "@/player/preload";
import { Artwork } from "./Artwork";
import { useListMenu } from "./lists/ListMenu";
import { Skeleton } from "./ui";
import { itemUrl } from "@/lib/urls";

export type CardAspect = "poster" | "square" | "thumb";

export const ASPECT_CLASS: Record<CardAspect, string> = {
  poster: "aspect-[2/3]",
  square: "aspect-square",
  thumb: "aspect-video",
};

/** Height of an image box for a given width. */
export function aspectHeight(aspect: CardAspect, width: number): number {
  if (aspect === "poster") return width * 1.5;
  if (aspect === "thumb") return (width * 9) / 16;
  return width;
}

/** Best card shape for an item kind. */
export function aspectFor(item: Pick<ItemSummary, "kind">): CardAspect {
  switch (item.kind) {
    case "episode":
      return "thumb";
    case "artist":
    case "album":
    case "track":
      return "square";
    default:
      return "poster";
  }
}

export interface MediaCardProps {
  item: ItemSummary;
  aspect?: CardAspect;
  /** Rendered width in CSS px (used for image sizing; also the card width when `fixed`). */
  width: number;
  fixed?: boolean;
  title?: string;
  subtitle?: string;
  href?: string;
  /** Show a play button on hover. */
  playable?: boolean;
  /** A Continue Watching card: show name first, remaining time; its menu can take it off the row. */
  continueStyle?: boolean;
  /** Put the files' bitrate and size under the title: sorted by bitrate. */
  media?: boolean;
  /**
   * Draw the quality badges ("4K", "DV") on the poster: only where they're
   * what the grid is about (sorted by bitrate, filtered by quality), as on
   * most posters they'd say the same thing over and over.
   */
  badges?: boolean;
  className?: string;
}

export function defaultCardTitles(item: ItemSummary, continueStyle = false): { title: string; subtitle: string } {
  if (continueStyle) {
    if (item.kind === "episode") {
      return {
        title: item.grandparent_title ?? item.title,
        subtitle: [episodeLabel(item), item.title].filter(Boolean).join(" · "),
      };
    }
    const pos = item.user_state.position_ms;
    return {
      title: item.title,
      subtitle: pos > 0 ? formatRemaining(pos, item.duration_ms) : item.year ? String(item.year) : "",
    };
  }
  if (item.kind === "season") return { title: seasonTitle(item), subtitle: cardSubtitle(item) };
  if (item.kind === "episode") {
    return {
      title: item.title,
      subtitle: [item.grandparent_title, episodeLabel(item)].filter(Boolean).join(" · "),
    };
  }
  return { title: item.title, subtitle: cardSubtitle(item) };
}

export const MediaCard = memo(function MediaCard({
  item,
  aspect = aspectFor(item),
  width,
  fixed,
  title,
  subtitle,
  href,
  playable = item.kind !== "track",
  continueStyle,
  media,
  badges: showBadges,
  className,
}: MediaCardProps) {
  const { t } = useTranslation(["components", "common"]);
  const play = usePlayItem();
  const lists = useListMenu();
  const defaults = defaultCardTitles(item, continueStyle);
  const name = title ?? defaults.title;
  const line = subtitle ?? (media ? mediaLine(item.media) : defaults.subtitle);
  const badges = showBadges ? mediaBadges(item.media) : [];
  const to = href ?? itemUrl(item.id);
  const candidates = aspect === "thumb" ? thumbCandidates(item) : posterCandidates(item);
  const progress = item.user_state.watched ? 0 : progressFraction(item);
  const isContainer = item.kind === "show" || item.kind === "season";
  const unwatched = isContainer ? unwatchedCount(item) : 0;
  const watchedLeaf = (item.kind === "movie" || item.kind === "episode") && item.user_state.watched;
  const watchedContainer = isContainer && item.leaf_count != null && item.leaf_count > 0 && unwatched === 0;
  const round = item.kind === "artist";

  return (
    <div
      className={cn("group/card min-w-0", className)}
      style={fixed ? { width } : undefined}
      onContextMenu={
        lists
          ? (e) => {
              e.preventDefault();
              lists.openMenu(item, e.clientX, e.clientY, { continueWatching: continueStyle });
            }
          : undefined
      }
    >
      <div className="relative">
        <Link
          to={to}
          className={cn(
            "relative block overflow-hidden bg-surface-2 outline-none ring-1 ring-white/5 transition duration-200 hover:ring-2 hover:ring-accent focus-visible:ring-2 focus-visible:ring-accent",
            round ? "rounded-full" : "rounded-md",
          )}
          aria-label={name}
        >
          <Artwork
            candidates={candidates}
            width={width}
            alt=""
            title={name}
            className={cn(ASPECT_CLASS[aspect], "w-full")}
            imgClassName="transition-transform duration-300 motion-safe:group-hover/card:scale-[1.04]"
          />
          <div className="pointer-events-none absolute inset-0 bg-black/0 transition-colors duration-200 group-hover/card:bg-black/30" />
          {watchedLeaf || watchedContainer ? (
            <span className="absolute right-1.5 top-1.5 flex size-6 items-center justify-center rounded-full bg-black/75 text-accent shadow">
              <Check className="size-3.5" strokeWidth={3} />
            </span>
          ) : null}
          {unwatched > 0 ? (
            <span className="absolute right-1.5 top-1.5 min-w-6 rounded bg-accent px-1.5 py-0.5 text-center text-xs font-bold text-accent-fg shadow">
              {unwatched}
            </span>
          ) : null}
          {badges.length ? (
            <span className="absolute bottom-2 left-1.5 flex flex-wrap gap-1">
              {badges.map((b) => (
                <span
                  key={b}
                  className="rounded bg-black/80 px-1.5 py-0.5 text-[10px] font-semibold leading-tight tracking-wide text-white shadow"
                >
                  {b}
                </span>
              ))}
            </span>
          ) : null}
          {progress > 0 ? (
            <div className="absolute inset-x-0 bottom-0 h-1 bg-black/60">
              <div className="h-full bg-accent" style={{ width: `${progress * 100}%` }} />
            </div>
          ) : null}
        </Link>
        {playable ? (
          <button
            type="button"
            aria-label={t("cards.playTitle", { title: name })}
            title={t("common:actions.play")}
            onClick={() => void play(item)}
            {...playIntent(item.kind, () => prefetchPlay(item))}
            className="absolute left-1/2 top-1/2 flex size-12 -translate-x-1/2 -translate-y-1/2 scale-90 items-center justify-center rounded-full bg-accent text-accent-fg opacity-0 shadow-lg shadow-black/50 transition duration-200 hover:bg-accent-hover group-hover/card:scale-100 group-hover/card:opacity-100 group-has-focus-visible/card:scale-100 group-has-focus-visible/card:opacity-100"
          >
            <Play className="ml-0.5 size-5 fill-current" />
          </button>
        ) : null}
      </div>
      <div className={cn("mt-2 px-0.5", round && "text-center")}>
        {/* The artwork above is the same link: Tab stops there once, not twice. */}
        <Link to={to} tabIndex={-1} className="block truncate text-sm font-medium text-fg transition-colors hover:text-accent" title={name}>
          {name}
        </Link>
        {line ? (
          <div className="truncate text-xs text-muted" title={line}>
            {line}
          </div>
        ) : (
          <div className="h-4" />
        )}
      </div>
    </div>
  );
});

export function CardSkeleton({ aspect, width, fixed }: { aspect: CardAspect; width?: number; fixed?: boolean }) {
  return (
    <div className="min-w-0" style={fixed && width ? { width } : undefined}>
      <Skeleton className={cn(ASPECT_CLASS[aspect], "w-full rounded-md")} />
      {/* As tall as a card's title and subtitle lines, so nothing moves when the cards arrive. */}
      <div className="mt-2 px-0.5">
        <div className="flex h-5 items-center">
          <Skeleton className="h-3.5 w-3/4" />
        </div>
        <div className="flex h-4 items-center">
          <Skeleton className="h-3 w-1/3" />
        </div>
      </div>
    </div>
  );
}

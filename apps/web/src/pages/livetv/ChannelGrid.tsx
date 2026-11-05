import { memo, useEffect, useLayoutEffect, useRef, useState } from "react";
import { Link } from "react-router";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { TFunction } from "i18next";
import { Info, Play, Star } from "lucide-react";
import { useTranslation } from "react-i18next";
import { categoryKey, programProgress, useSetFavorite, type Channel, type LiveCategory, type Program } from "@/api/livetv";
import { cn } from "@/lib/cn";
import { useChannelList } from "./channelList";
import { ChannelLogo, channelName, formatTime, lastWatched, liveUrl, useIndexOf } from "./common";

/** The card's text under its logo: name, what's on, how far, what's next. */
const CARD_TEXT = 116;
const CARD_WIDTH = 236;

/** On a phone's width, channels are a list of rows this tall rather than cards. */
const LIST_ROW = 76;

/**
 * Channels as cards: the logo, what's on with how far it is, what's next;
 * on a phone's width, as a list. Rows are virtualized and channels fetched
 * a page at a time as they come into view.
 */
export function ChannelGrid({
  category,
  search,
  now,
  onDetails,
}: {
  category: LiveCategory;
  search: string;
  now: number;
  onDetails: (channel: Channel, program: Program) => void;
}) {
  const { t } = useTranslation("livetv");
  const scrollRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const measure = () => setWidth(el.clientWidth);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  const narrow = width > 0 && width < 560;
  const padding = narrow ? 0 : 16;
  const gap = narrow ? 0 : 16;
  const inner = Math.max(0, width - padding * 2);
  const cols = narrow ? 1 : inner > 0 ? Math.max(1, Math.floor((inner + gap) / (CARD_WIDTH + gap))) : 1;
  const colWidth = (inner - gap * (cols - 1)) / cols;
  const rowHeight = narrow ? LIST_ROW : Math.round(colWidth / 2 + CARD_TEXT + gap);
  const [range, setRange] = useState({ from: 0, to: 40 });
  const list = useChannelList(category, search, range.from, range.to);
  const total = list.total ?? 0;

  // TanStack Virtual: the compiler leaves this component as it is.
  // eslint-disable-next-line react-hooks/incompatible-library
  const rows = useVirtualizer({
    count: width > 0 ? Math.ceil(total / cols) : 0,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowHeight,
    overscan: 3,
    paddingStart: padding,
    paddingEnd: padding,
  });
  useEffect(() => rows.measure(), [rows, rowHeight, cols]);
  const items = rows.getVirtualItems();
  const firstIndex = (items[0]?.index ?? 0) * cols;
  const lastIndex = ((items[items.length - 1]?.index ?? 0) + 1) * cols - 1;
  useEffect(() => {
    setRange((r) => (r.from === firstIndex && r.to === lastIndex ? r : { from: firstIndex, to: lastIndex }));
  }, [firstIndex, lastIndex]);
  const [returnTo] = useState(lastWatched);
  const returnIndex = useIndexOf(category, search ? null : returnTo);
  const placed = useRef<string | null>(null);
  useLayoutEffect(() => {
    const key = `${categoryKey(category)}|${search}`;
    if (placed.current === key || returnIndex === undefined || list.total === undefined || width === 0) return;
    const first = placed.current === null;
    placed.current = key;
    if (first && returnIndex != null) rows.scrollToIndex(Math.floor(returnIndex / cols), { align: "center" });
    else scrollRef.current?.scrollTo({ top: 0 });
  }, [category, search, returnIndex, list.total, width, cols, rows]);

  return (
    <div
      ref={scrollRef}
      className="relative h-full overflow-y-auto overscroll-contain [scrollbar-width:thin]"
      aria-label={t("channels")}
      role="list"
    >
      <div className="relative w-full" style={{ height: rows.getTotalSize() }}>
        {items.map((row) => (
          <div
            key={row.key}
            className="absolute grid"
            style={{
              top: row.start,
              left: padding,
              right: padding,
              height: rowHeight - gap,
              gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`,
              columnGap: gap,
            }}
          >
            {Array.from({ length: Math.min(cols, total - row.index * cols) }, (_, i) => {
              const channel = list.at(row.index * cols + i);
              return channel ? (
                <div key={channel.id} role="listitem">
                  {narrow ? (
                    <ChannelListRow channel={channel} category={category} now={now} />
                  ) : (
                    <ChannelCard channel={channel} category={category} now={now} onDetails={onDetails} />
                  )}
                </div>
              ) : narrow ? (
                <div key={`p${i}`} aria-hidden className="flex h-full items-center gap-3 border-b border-border/60 px-3">
                  <div className="skeleton h-10 w-16 rounded-md" />
                  <div className="flex-1 space-y-2">
                    <div className="skeleton h-3.5 w-1/2" />
                    <div className="skeleton h-3 w-3/4" />
                  </div>
                </div>
              ) : (
                <div key={`p${i}`} aria-hidden className="flex flex-col overflow-hidden rounded-lg border border-border bg-surface">
                  <div className="skeleton aspect-[16/8] w-full rounded-none" />
                  <div className="space-y-2 p-3">
                    <div className="skeleton h-4 w-2/3" />
                    <div className="skeleton h-3.5 w-full" />
                  </div>
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

/** A channel as a row of a phone's list: logo, number and name, what's on and how far. */
const ChannelListRow = memo(function ChannelListRow({
  channel,
  category,
  now,
}: {
  channel: Channel;
  category: LiveCategory;
  now: number;
}) {
  const { t } = useTranslation("livetv");
  const on = channel.now && channel.now.start <= now && channel.now.stop > now ? channel.now : null;
  return (
    <Link
      to={liveUrl(channel, category)}
      className="flex h-full items-center gap-3 border-b border-border/60 px-3 transition-colors hover:bg-surface-2 active:bg-surface-2"
      aria-label={watchLabel(t, channel, on)}
    >
      <ChannelLogo channel={channel} width={80} showName={false} className="h-10 w-16 shrink-0 rounded-md bg-surface-3" />
      <div className="min-w-0 flex-1">
        <div className="flex items-baseline gap-2">
          {channel.number ? <span className="shrink-0 text-xs tabular-nums text-subtle">{channel.number}</span> : null}
          <span className="truncate text-sm font-medium">{channelName(channel, category)}</span>
          {channel.favorite ? <Star className="size-3 shrink-0 self-center fill-accent text-accent" /> : null}
        </div>
        {on ? (
          <>
            <div className="truncate text-xs text-muted">{on.title}</div>
            <div className="mt-1 h-0.5 overflow-hidden rounded-full bg-white/10">
              <div className="h-full bg-accent" style={{ width: `${programProgress(on, now) * 100}%` }} />
            </div>
          </>
        ) : (
          <div className="truncate text-xs text-subtle">{channel.has_guide ? t("nothingNow") : t("noGuide")}</div>
        )}
      </div>
    </Link>
  );
});

export const ChannelCard = memo(function ChannelCard({
  channel,
  category,
  now,
  onDetails,
  className,
}: {
  channel: Channel;
  /** Where it was chosen: channel up and down stay there. */
  category?: LiveCategory;
  now: number;
  onDetails: (channel: Channel, program: Program) => void;
  className?: string;
}) {
  const { t } = useTranslation("livetv");
  const favorite = useSetFavorite();
  const on = channel.now && channel.now.stop > now ? channel.now : null;
  const next = on ? channel.next : (channel.next ?? channel.now);
  return (
    <div
      className={cn(
        "group relative flex h-full flex-col overflow-hidden rounded-lg border border-border bg-surface transition-[border-color,transform,box-shadow] duration-200 hover:-translate-y-0.5 hover:border-border-strong hover:shadow-lg hover:shadow-black/30 focus-within:border-accent",
        className,
      )}
    >
      <Link
        to={liveUrl(channel, category)}
        className="relative block outline-none"
        aria-label={watchLabel(t, channel, on)}
      >
        <ChannelLogo
          channel={channel}
          width={200}
          className="aspect-[16/8] w-full bg-gradient-to-br from-surface-3 to-surface-2"
        />
        <div className="absolute inset-0 flex items-center justify-center bg-black/55 opacity-0 transition-opacity duration-200 group-hover:opacity-100 group-focus-within:opacity-100">
          <span className="flex size-12 items-center justify-center rounded-full bg-accent text-accent-fg shadow-lg shadow-black/40">
            <Play className="ml-0.5 size-5 fill-current" />
          </span>
        </div>
        {channel.number ? (
          <span className="absolute left-2 top-2 rounded bg-black/60 px-1.5 py-0.5 text-xs font-semibold tabular-nums text-white/90 backdrop-blur-sm">
            {channel.number}
          </span>
        ) : null}
      </Link>
      <button
        type="button"
        onClick={() => favorite.mutate({ id: channel.id, favorite: !channel.favorite })}
        aria-label={t(channel.favorite ? "favourites.removeChannel" : "favourites.addChannel", { name: channel.name })}
        aria-pressed={channel.favorite}
        className={cn(
          "absolute right-2 top-2 flex size-8 items-center justify-center rounded-full bg-black/50 text-white/80 backdrop-blur-sm transition-[opacity,color] hover:text-white",
          channel.favorite ? "opacity-100" : "opacity-0 group-hover:opacity-100 focus-visible:opacity-100",
        )}
      >
        <Star className={cn("size-4", channel.favorite && "fill-accent text-accent")} />
      </button>
      <div className="flex min-h-0 flex-1 flex-col gap-1.5 p-3">
        <div className="truncate text-sm font-semibold" title={channel.name}>
          {channelName(channel, category)}
        </div>
        {on ? (
          <button
            type="button"
            onClick={() => onDetails(channel, on)}
            className="-mx-1 -my-0.5 flex items-start gap-1 rounded px-1 py-0.5 text-left text-sm text-fg/90 transition-colors hover:bg-white/[0.05]"
          >
            <span className="line-clamp-1 flex-1">{on.title}</span>
            <Info className="mt-0.5 size-3.5 shrink-0 text-subtle opacity-0 transition-opacity group-hover:opacity-100" />
          </button>
        ) : (
          <div className="text-sm text-subtle">{channel.has_guide ? t("nothingNow") : t("noGuide")}</div>
        )}
        {on ? (
          <div className="flex items-center gap-2">
            <div className="h-1 flex-1 overflow-hidden rounded-full bg-white/10">
              <div
                className="h-full rounded-full bg-accent transition-[width] duration-700"
                style={{ width: `${programProgress(on, now) * 100}%` }}
              />
            </div>
            <span className="shrink-0 text-[11px] tabular-nums text-subtle">{formatTime(on.stop)}</span>
          </div>
        ) : null}
        {next ? (
          <button
            type="button"
            onClick={() => onDetails(channel, next)}
            className="mt-auto truncate pt-1 text-left text-xs text-subtle transition-colors hover:text-muted"
          >
            <span className="tabular-nums">{formatTime(next.start)}</span> · {next.title}
          </button>
        ) : null}
      </div>
    </div>
  );
});

/** "Watch 5 News", with what's on now. */
function watchLabel(t: TFunction<"livetv">, channel: Channel, on: Program | null): string {
  const name = `${channel.number ? `${channel.number} ` : ""}${channel.name}`;
  return on ? t("watchNow", { name, title: on.title }) : t("watch", { name });
}

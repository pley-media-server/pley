import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { useQueries } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Clock, Play, Star } from "lucide-react";
import { useTranslation } from "react-i18next";
import { CHANNEL_PAGE, categoryKey, liveQueries, type Channel, type LiveCategory, type Program } from "@/api/livetv";
import { Skeleton } from "@/components/ui";
import { t } from "@/i18n";
import { cn } from "@/lib/cn";
import { useMediaQuery } from "@/lib/hooks";
import { useChannelList } from "./channelList";
import { ChannelLogo, channelName, formatGuideDay, formatTime, lastWatched, liveUrl, useIndexOf } from "./common";

/** The guide fetched in chunks of this many seconds, as it is scrolled to. */
const CHUNK = 6 * 3600;
/** How far ahead the guide goes. */
const SPAN = 48 * 3600;
const SLOT = 1800;

interface Layout {
  pxPerMin: number;
  rowHeight: number;
  channelWidth: number;
  headerHeight: number;
}

const WIDE: Layout = { pxPerMin: 5.5, rowHeight: 68, channelWidth: 212, headerHeight: 44 };
const NARROW: Layout = { pxPerMin: 4, rowHeight: 60, channelWidth: 84, headerHeight: 40 };

/**
 * The programme guide: channels down, time across, now marked. Rows are
 * virtualized, channels fetched a page at a time and their programmes in
 * six-hour chunks as they come into view, so a lineup of tens of thousands
 * scrolls as easily as ten.
 */
export function GuideGrid({
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
  const wide = useMediaQuery("(min-width: 640px)");
  const layout = wide ? WIDE : NARROW;
  const pxPerSec = layout.pxPerMin / 60;
  // Half an hour before the slot now is in: fixed while the page is open.
  const [start] = useState(() => Math.floor(Date.now() / 1000 / SLOT) * SLOT - 2 * SLOT);
  const width = SPAN * pxPerSec;
  const scrollRef = useRef<HTMLDivElement>(null);
  const [view, setView] = useState({ left: 0, width: 1200 });
  const [range, setRange] = useState({ from: 0, to: 20 });
  const list = useChannelList(category, search, range.from, range.to);

  // TanStack Virtual: the compiler leaves this component as it is.
  // eslint-disable-next-line react-hooks/incompatible-library
  const rows = useVirtualizer({
    count: list.total ?? 0,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => layout.rowHeight,
    overscan: 8,
    paddingStart: layout.headerHeight,
  });
  const items = rows.getVirtualItems();
  const firstRow = items[0]?.index ?? 0;
  const lastRow = items[items.length - 1]?.index ?? 20;
  useEffect(() => {
    setRange((r) => (r.from === firstRow && r.to === lastRow ? r : { from: firstRow, to: lastRow }));
  }, [firstRow, lastRow]);
  // Another list starts at its top; coming back from the player, at the
  // channel watched.
  const [returnTo] = useState(lastWatched);
  const returnIndex = useIndexOf(category, search ? null : returnTo);
  const placed = useRef<string | null>(null);
  useLayoutEffect(() => {
    const key = `${categoryKey(category)}|${search}`;
    if (placed.current === key || returnIndex === undefined || list.total === undefined) return;
    const first = placed.current === null;
    placed.current = key;
    if (first && returnIndex != null) rows.scrollToIndex(returnIndex, { align: "center" });
    else scrollRef.current?.scrollTo({ top: 0, left: scrollRef.current.scrollLeft });
  }, [category, search, returnIndex, list.total, rows]);

  const onScroll = useCallback(() => {
    const el = scrollRef.current;
    if (!el) return;
    setView((v) =>
      Math.abs(v.left - el.scrollLeft) < 40 && v.width === el.clientWidth ? v : { left: el.scrollLeft, width: el.clientWidth },
    );
  }, []);

  // Opens at half an hour before now.
  const toNow = useCallback(
    (behavior: ScrollBehavior = "smooth") => {
      const el = scrollRef.current;
      if (!el) return;
      el.scrollTo({ left: Math.max(0, (Date.now() / 1000 - start - SLOT) * pxPerSec), behavior });
    },
    [start, pxPerSec],
  );
  useLayoutEffect(() => {
    toNow("auto");
    onScroll();
  }, [toNow, onScroll]);
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const ro = new ResizeObserver(onScroll);
    ro.observe(el);
    return () => ro.disconnect();
  }, [onScroll]);

  // The time in view, and the chunks covering it (and one either side).
  const viewFrom = start + Math.max(0, view.left - 0) / pxPerSec;
  const viewTo = viewFrom + (view.width - layout.channelWidth) / pxPerSec;
  const firstChunk = Math.max(0, Math.floor((viewFrom - start) / CHUNK) - 1);
  const lastChunk = Math.min(SPAN / CHUNK - 1, Math.floor((viewTo - start) / CHUNK) + 1);
  // Each page of channels in view, for each chunk of time in view.
  const pageIds = [...list.pages.entries()]
    .filter(([page]) => page >= Math.floor(firstRow / CHANNEL_PAGE) && page <= Math.floor(lastRow / CHANNEL_PAGE))
    .map(([, channels]) => channels.map((c) => c.id).join(","))
    .filter(Boolean);
  const chunks = useQueries({
    queries: pageIds.flatMap((ids) =>
      Array.from({ length: lastChunk - firstChunk + 1 }, (_, i) => {
        const from = start + (firstChunk + i) * CHUNK;
        return liveQueries.guide(from, from + CHUNK, ids);
      }),
    ),
  });
  const chunkData = chunks.map((c) => c.data);
  const programs = useMemo(() => {
    const byChannel = new Map<number, Program[]>();
    for (const guide of chunkData) {
      if (!guide) continue;
      for (const [id, list] of Object.entries(guide.programs)) {
        const key = Number(id);
        const known = byChannel.get(key) ?? [];
        const starts = new Set(known.map((p) => p.start));
        byChannel.set(key, [...known, ...list.filter((p) => !starts.has(p.start))].sort((a, b) => a.start - b.start));
      }
    }
    return byChannel;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- the chunks' data, not the query objects
  }, chunkData);
  const loaded = chunks.length > 0 && chunks.every((c) => !c.isPending);

  const nowX = (now - start) * pxPerSec;
  const leftDay = formatGuideDay(Math.max(viewFrom, start), now);
  const nowInView = nowX >= view.left && nowX <= view.left + view.width - layout.channelWidth;

  const slots = useMemo(() => {
    const out: number[] = [];
    const from = Math.max(start, Math.floor(viewFrom / SLOT) * SLOT - SLOT);
    for (let t = from; t < Math.min(start + SPAN, viewTo + SLOT); t += SLOT) out.push(t);
    return out;
  }, [start, viewFrom, viewTo]);

  return (
    <div className="relative h-full">
      <div
        ref={scrollRef}
        onScroll={onScroll}
        className="relative h-full overflow-auto overscroll-contain rounded-lg border border-border bg-surface [scrollbar-width:thin]"
        role="grid"
        aria-label={t("guide.label")}
        aria-rowcount={list.total}
      >
        <div className="relative" style={{ width: layout.channelWidth + width, height: rows.getTotalSize() }}>
          {/* Times across the top. */}
          <div
            className="sticky top-0 z-30 flex border-b border-border bg-surface/95 backdrop-blur"
            style={{ height: layout.headerHeight, width: layout.channelWidth + width }}
          >
            <div
              className="sticky left-0 z-10 flex shrink-0 items-center gap-2 border-r border-border bg-surface px-3 text-xs font-semibold"
              style={{ width: layout.channelWidth }}
            >
              <span className="truncate">{leftDay}</span>
            </div>
            <div className="relative flex-1">
              {slots.map((t) => (
                <div
                  key={t}
                  className="absolute top-0 flex h-full items-center border-l border-border/60 pl-2 text-xs tabular-nums text-muted"
                  style={{ left: (t - start) * pxPerSec }}
                >
                  {new Date(t * 1000).getHours() === 0 && new Date(t * 1000).getMinutes() === 0 ? (
                    <span className="font-semibold text-fg">{formatGuideDay(t, now)}</span>
                  ) : (
                    formatTime(t)
                  )}
                </div>
              ))}
              {nowX > 0 ? (
                <div
                  className="absolute bottom-0 z-10 -translate-x-1/2 rounded-t bg-accent px-1.5 pb-0.5 pt-0.5 text-[10px] font-bold tabular-nums text-accent-fg"
                  style={{ left: nowX }}
                >
                  {formatTime(now)}
                </div>
              ) : null}
            </div>
          </div>

          {/* Now, down the grid. */}
          {nowX > 0 ? (
            <div
              aria-hidden
              className="pointer-events-none absolute bottom-0 z-[5] w-0.5 bg-accent/80 shadow-[0_0_8px_rgba(56,189,248,0.6)]"
              style={{ left: layout.channelWidth + nowX - 1, top: layout.headerHeight }}
            />
          ) : null}

          {items.map((row) => {
            const channel = list.at(row.index);
            if (!channel) {
              return <LoadingRow key={`row${row.index}`} top={row.start} layout={layout} width={width} viewFrom={viewFrom} start={start} />;
            }
            return (
              <GuideRow
                key={channel.id}
                channel={channel}
                category={category}
                programs={programs.get(channel.id)}
                loaded={loaded}
                top={row.start}
                layout={layout}
                start={start}
                width={width}
                viewFrom={viewFrom}
                viewTo={viewTo}
                now={now}
                index={row.index}
                onDetails={onDetails}
              />
            );
          })}
        </div>
      </div>
      {!nowInView ? (
        <button
          type="button"
          onClick={() => toNow()}
          className="absolute bottom-4 right-4 z-40 flex h-9 items-center gap-1.5 rounded-full bg-accent px-4 text-sm font-semibold text-accent-fg shadow-lg shadow-black/40 transition-transform animate-pop-in hover:scale-105"
        >
          <Clock className="size-4" /> {t("guide.now")}
        </button>
      ) : null}
    </div>
  );
}

/** A row whose channel is still on its way. */
function LoadingRow({
  top,
  layout,
  width,
  viewFrom,
  start,
}: {
  top: number;
  layout: Layout;
  width: number;
  viewFrom: number;
  start: number;
}) {
  const left = Math.max(0, (viewFrom - start) * (layout.pxPerMin / 60));
  return (
    <div aria-hidden className="absolute left-0 flex border-b border-border/70" style={{ top, height: layout.rowHeight, width: layout.channelWidth + width }}>
      <div className="sticky left-0 z-10 flex shrink-0 items-center gap-2.5 border-r border-border bg-surface px-2.5" style={{ width: layout.channelWidth }}>
        <Skeleton className={cn("rounded-md", layout === WIDE ? "h-10 w-16" : "h-9 w-14")} />
        {layout === WIDE ? <Skeleton className="h-3.5 flex-1 rounded" /> : null}
      </div>
      <div className="relative flex-1">
        <div className="skeleton absolute inset-y-1 rounded-md" style={{ left: left + 2, width: 420 }} />
      </div>
    </div>
  );
}

const GuideRow = memo(function GuideRow({
  channel,
  category,
  programs,
  loaded,
  top,
  layout,
  start,
  width,
  viewFrom,
  viewTo,
  now,
  index,
  onDetails,
}: {
  channel: Channel;
  category: LiveCategory;
  programs: Program[] | undefined;
  loaded: boolean;
  top: number;
  layout: Layout;
  start: number;
  width: number;
  viewFrom: number;
  viewTo: number;
  now: number;
  index: number;
  onDetails: (channel: Channel, program: Program) => void;
}) {
  const { t } = useTranslation("livetv");
  const navigate = useNavigate();
  const pxPerSec = layout.pxPerMin / 60;
  const wide = layout === WIDE;
  // Only what's in view (and a little either side) is drawn.
  const margin = 3600;
  const visible = (programs ?? []).filter((p) => p.stop > viewFrom - margin && p.start < viewTo + margin && p.stop > start);
  return (
    <div
      role="row"
      aria-rowindex={index + 1}
      className="absolute left-0 flex border-b border-border/70"
      style={{ top, height: layout.rowHeight, width: layout.channelWidth + width }}
    >
      <button
        type="button"
        role="rowheader"
        onClick={() => void navigate(liveUrl(channel, category))}
        className="group/ch sticky left-0 z-10 flex shrink-0 items-center gap-2.5 border-r border-border bg-surface px-2.5 text-left transition-colors hover:bg-surface-2"
        style={{ width: layout.channelWidth }}
        title={t("watch", { name: channel.name })}
      >
        <div className="relative">
          <ChannelLogo
            channel={channel}
            width={wide ? 64 : 56}
            showName={false}
            className={cn("rounded-md bg-surface-3", wide ? "h-10 w-16" : "h-9 w-14")}
          />
          <span className="absolute inset-0 flex items-center justify-center rounded-md bg-black/60 opacity-0 transition-opacity group-hover/ch:opacity-100">
            <Play className="size-4 fill-white text-white" />
          </span>
        </div>
        {wide ? (
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1 text-xs tabular-nums text-subtle">
              {channel.number ?? ""}
              {channel.favorite ? <Star className="size-3 fill-accent text-accent" /> : null}
            </div>
            <div className="truncate text-sm font-medium" title={channel.name}>
              {channelName(channel, category)}
            </div>
          </div>
        ) : null}
      </button>
      <div className="relative flex-1">
        {visible.length === 0 && loaded ? (
          <div
            className="absolute inset-y-1 flex items-center rounded-md bg-white/[0.02] px-3 text-xs text-subtle"
            style={{ left: Math.max(0, (viewFrom - start) * pxPerSec) + 2, width: Math.max(0, (viewTo - viewFrom) * pxPerSec) - 4 }}
          >
            {channel.has_guide ? t("guide.noInformation") : <span className="truncate text-muted">{channelName(channel, category)}</span>}
          </div>
        ) : null}
        {visible.map((p) => (
          <ProgramCell
            key={p.start}
            channel={channel}
            program={p}
            left={(Math.max(p.start, start) - start) * pxPerSec}
            width={(p.stop - Math.max(p.start, start)) * pxPerSec}
            viewLeft={(viewFrom - start) * pxPerSec}
            now={now}
            onDetails={onDetails}
          />
        ))}
      </div>
    </div>
  );
});

/** Drawn by the hundred, so in its row's language rather than with a hook of its own. */
function ProgramCell({
  channel,
  program,
  left,
  width,
  viewLeft,
  now,
  onDetails,
}: {
  channel: Channel;
  program: Program;
  left: number;
  width: number;
  viewLeft: number;
  now: number;
  onDetails: (channel: Channel, program: Program) => void;
}) {
  const airing = program.start <= now && program.stop > now;
  const past = program.stop <= now;
  // A programme begun before the left edge keeps its title in view.
  const indent = Math.max(0, Math.min(viewLeft - left, width - 80));
  return (
    <button
      type="button"
      role="gridcell"
      onClick={() => onDetails(channel, program)}
      className={cn(
        "absolute inset-y-1 overflow-hidden rounded-md border text-left transition-colors",
        airing
          ? "border-accent/40 bg-accent/[0.09] hover:bg-accent/[0.16]"
          : "border-transparent bg-surface-2 hover:border-border-strong hover:bg-surface-3",
        past && "opacity-55",
      )}
      style={{ left: left + 1, width: Math.max(2, width - 2) }}
      title={`${program.title} · ${formatTime(program.start)} – ${formatTime(program.stop)}`}
    >
      {width > 34 ? (
        <div className="flex h-full flex-col justify-center px-2.5" style={{ paddingLeft: 10 + indent }}>
          <div className="flex min-w-0 items-center gap-1.5">
            {program.is_live ? (
              <span className="shrink-0 rounded-sm bg-danger px-1 text-[9px] font-bold uppercase leading-[14px] text-white">{t("live", { ns: "livetv" })}</span>
            ) : program.is_new ? (
              <span className="shrink-0 rounded-sm bg-accent/20 px-1 text-[9px] font-bold uppercase leading-[14px] text-accent">{t("new", { ns: "livetv" })}</span>
            ) : null}
            <span className={cn("truncate text-sm", airing ? "font-semibold" : "font-medium")}>{program.title}</span>
          </div>
          <div className="truncate text-xs tabular-nums text-subtle">
            {formatTime(program.start)} – {formatTime(program.stop)}
            {program.subtitle ? ` · ${program.subtitle}` : ""}
          </div>
        </div>
      ) : null}
      {airing ? (
        <span
          aria-hidden
          className="absolute bottom-0 left-0 h-0.5 bg-accent"
          style={{ width: `${((now - program.start) / (program.stop - program.start)) * 100}%` }}
        />
      ) : null}
    </button>
  );
}

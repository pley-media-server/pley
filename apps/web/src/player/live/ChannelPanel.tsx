import { useEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { useVirtualizer } from "@tanstack/react-virtual";
import { ArrowLeft, ChevronDown, Search, Star, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  categoryKey,
  liveQueries,
  programProgress,
  useLiveGroups,
  type Channel,
  type LiveCategory,
} from "@/api/livetv";
import { cn } from "@/lib/cn";
import { CategoryRail, categoryName } from "@/pages/livetv/CategoryRail";
import { useChannelList } from "@/pages/livetv/channelList";
import { ChannelLogo, channelName, formatTime } from "@/pages/livetv/common";

const ROW = 60;

/** The channels beside the picture: a group's, with what's on each, to change to one. */
export function ChannelPanel({
  open,
  category,
  current,
  now,
  onClose,
  onTune,
}: {
  open: boolean;
  /** The category being zapped through: the panel opens on it. */
  category: LiveCategory;
  current: number;
  now: number;
  onClose: () => void;
  onTune: (c: Channel, category: LiveCategory) => void;
}) {
  const { t } = useTranslation(["livetv", "common"]);
  const [shown, setShown] = useState<LiveCategory>(category);
  const [picking, setPicking] = useState(false);
  const [search, setSearch] = useState("");
  const groups = useLiveGroups(open);
  const scrollRef = useRef<HTMLDivElement>(null);

  // Each opening starts on the player's category, at the channel watched.
  const [opened, setOpened] = useState(open);
  if (open !== opened) {
    setOpened(open);
    if (open) {
      setShown(category);
      setSearch("");
      setPicking(false);
    }
  }

  const [range, setRange] = useState({ from: 0, to: 20 });
  // Search looks among every channel, whatever group is listed.
  const listed: LiveCategory = search.trim() ? { kind: "all" } : shown;
  const list = useChannelList(listed, search, range.from, range.to);
  const around = useQuery({ ...liveQueries.around(shown, current), enabled: open && !search });
  // TanStack Virtual: the compiler leaves this component as it is.
  // eslint-disable-next-line react-hooks/incompatible-library
  const rows = useVirtualizer({
    count: list.total ?? 0,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => ROW,
    overscan: 6,
    paddingStart: 8,
    paddingEnd: 8,
  });
  const items = rows.getVirtualItems();
  const firstRow = items[0]?.index ?? 0;
  const lastRow = items[items.length - 1]?.index ?? 20;
  useEffect(() => {
    setRange((r) => (r.from === firstRow && r.to === lastRow ? r : { from: firstRow, to: lastRow }));
  }, [firstRow, lastRow]);

  // The channel watched in the middle of the list, once its place is known.
  const currentIndex = around.data ? around.data.offset + around.data.channels.findIndex((c) => c.id === current) : -1;
  const placed = useRef<string | null>(null);
  const placeKey = `${categoryKey(shown)}:${open}`;
  useEffect(() => {
    if (!open) {
      placed.current = null;
      return;
    }
    if (picking || search || placed.current === placeKey || list.total === undefined) return;
    // Its page first, unless that can't be had.
    if (!around.data && !around.isError) return;
    placed.current = placeKey;
    if (around.data && currentIndex >= around.data.offset) rows.scrollToIndex(currentIndex, { align: "center" });
    else rows.scrollToOffset(0);
    window.requestAnimationFrame(() =>
      scrollRef.current?.querySelector<HTMLElement>('[aria-current="true"]')?.focus({ preventScroll: true }),
    );
  }, [open, picking, search, placeKey, list.total, currentIndex, around.data, around.isError, rows]);

  return (
    <aside
      className={cn(
        "absolute inset-y-0 right-0 z-20 flex w-full max-w-sm flex-col border-l border-white/10 bg-black/90 shadow-2xl backdrop-blur-sm transition-transform duration-300 ease-out",
        open ? "translate-x-0" : "pointer-events-none translate-x-full",
      )}
      aria-label={t("channels")}
      aria-hidden={!open}
      inert={!open}
      onKeyDown={(e) => {
        // The player's keys stay outside the list.
        if (e.key !== "Escape") e.stopPropagation();
      }}
    >
      <div className="flex items-center gap-2 border-b border-white/10 p-3">
        {picking ? (
          <button
            type="button"
            onClick={() => setPicking(false)}
            className="flex h-9 min-w-0 flex-1 items-center gap-2 rounded-md px-2 text-sm font-medium transition hover:bg-white/10"
          >
            <ArrowLeft className="size-4 shrink-0" /> {t("categories.label")}
          </button>
        ) : (
          <>
            <button
              type="button"
              onClick={() => setPicking(true)}
              className="flex h-9 min-w-0 max-w-[45%] items-center gap-1.5 rounded-md border border-white/10 bg-white/[0.06] px-2.5 text-sm font-medium transition hover:bg-white/10"
              title={t("panel.changeGroup")}
            >
              {shown.kind === "favorites" ? <Star className="size-3.5 shrink-0 fill-accent text-accent" /> : null}
              <span className="truncate">{categoryName(shown, groups.data?.groups)}</span>
              <ChevronDown className="size-3.5 shrink-0 text-white/50" />
            </button>
            <div className="relative min-w-0 flex-1">
              <Search className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-white/40" />
              <input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder={t("panel.search")}
                aria-label={t("panel.search")}
                className="h-9 w-full rounded-md border border-white/10 bg-white/[0.06] pl-9 pr-3 text-sm text-white placeholder:text-white/40 focus:border-accent focus:outline-none"
              />
            </div>
          </>
        )}
        <button type="button" aria-label={t("common:actions.close")} onClick={onClose} className="flex size-9 shrink-0 items-center justify-center rounded-md transition hover:bg-white/10">
          <X className="size-5" />
        </button>
      </div>

      {picking ? (
        <CategoryRail
          groups={groups.data}
          selected={shown}
          onSelect={(c) => {
            setShown(c);
            setSearch("");
            setPicking(false);
            placed.current = null;
          }}
          className="min-h-0 flex-1"
        />
      ) : (
        <div ref={scrollRef} className="relative min-h-0 flex-1 overflow-y-auto overscroll-contain [scrollbar-width:thin]">
          {list.total === 0 ? <p className="p-6 text-center text-sm text-white/50">{t("panel.noMatch")}</p> : null}
          <div className="relative" style={{ height: rows.getTotalSize() }}>
            {items.map((row) => {
              const c = list.at(row.index);
              return (
                <div key={row.key} className="absolute inset-x-2" style={{ top: row.start, height: ROW }}>
                  {c ? (
                    <PanelRow channel={c} name={channelName(c, listed)} active={c.id === current} now={now} onTune={() => onTune(c, listed)} />
                  ) : (
                    <div className="flex h-full items-center gap-3 p-2" aria-hidden>
                      <span className="w-8" />
                      <span className="skeleton h-9 w-14 rounded-md" />
                      <span className="skeleton h-3.5 flex-1 rounded" />
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </aside>
  );
}

function PanelRow({
  channel: c,
  name,
  active,
  now,
  onTune,
}: {
  channel: Channel;
  name: string;
  active: boolean;
  now: number;
  onTune: () => void;
}) {
  const on = c.now && c.now.start <= now && c.now.stop > now ? c.now : null;
  return (
    <button
      type="button"
      aria-current={active}
      onClick={onTune}
      className={cn(
        "flex h-full w-full items-center gap-3 rounded-lg p-2 text-left transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-accent",
        active ? "bg-accent/15" : "hover:bg-white/[0.07]",
      )}
    >
      <span className="w-8 shrink-0 text-right text-xs tabular-nums text-white/50">{c.number ?? ""}</span>
      <ChannelLogo channel={c} width={64} showName={false} className="h-9 w-14 shrink-0 rounded-md bg-white/[0.07]" />
      <span className="min-w-0 flex-1">
        <span className={cn("block truncate text-sm font-medium", active && "text-accent")} title={c.name}>
          {name}
        </span>
        {on ? (
          <>
            <span className="block truncate text-xs text-white/60">{on.title}</span>
            <span className="mt-1 block h-0.5 overflow-hidden rounded-full bg-white/10">
              <span className="block h-full bg-accent/80" style={{ width: `${programProgress(on, now) * 100}%` }} />
            </span>
          </>
        ) : c.next ? (
          <span className="block truncate text-xs text-white/45">
            {formatTime(c.next.start)} · {c.next.title}
          </span>
        ) : null}
      </span>
      {c.favorite ? <Star className="size-3.5 shrink-0 fill-accent text-accent" /> : null}
    </button>
  );
}

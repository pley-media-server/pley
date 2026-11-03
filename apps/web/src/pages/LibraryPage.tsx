import { useEffect, useMemo, useState } from "react";
import { useParams, useSearchParams } from "react-router";
import { ArrowDownWideNarrow, ArrowUpNarrowWide, FolderSearch, RefreshCw, Sparkles } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useLibrary, useLibraryFilters, useLibraryHubs, useLibraryItems, useScanLibrary } from "@/api/hooks";
import type { LibraryKind, LibrarySort, SortOrder } from "@/api/types";
import { useAuth } from "@/auth";
import { HubRow, HubRowSkeleton } from "@/components/HubRow";
import { TmdbKeyNotice } from "@/components/MetadataNotice";
import { ItemGrid, ItemGridSkeleton } from "@/components/ItemGrid";
import { CollectionsRow } from "@/components/lists/CollectionCard";
import { LibraryIcon } from "@/components/Sidebar";
import { useToast } from "@/components/Toast";
import { Button, EmptyState, IconButton, Select } from "@/components/ui";
import { NotFoundPage } from "./NotFoundPage";
import { SaveFiltersModal } from "./SaveFiltersModal";
import { CollectionsTab } from "./collections/CollectionsTab";
import type { CardAspect } from "@/components/cards";
import { cn } from "@/lib/cn";
import { pluralize, RANGE_FILTER_LABELS, rangeFilterOf, RESOLUTION_LABELS, resolutionOf } from "@/lib/format";
import { smartFilterOf } from "@/lib/smartFilter";
import { useDocumentTitle, useLocalStorage } from "@/lib/hooks";
import { idParam } from "@/lib/ids";
import { keepTab, keepView, leftTab, leftView, tabOf, tabStorageKey, viewOf, viewStorageKey, withTab, withView } from "@/lib/libraryView";

const SORT_LABELS: Record<LibrarySort, string> = {
  title: "Title",
  year: "Year",
  added: "Date added",
  rating: "Rating",
  release: "Release date",
  duration: "Duration",
  last_played: "Last played",
  user_rating: "Your rating",
  bitrate: "Bitrate",
};

const SORTS_BY_KIND: Record<LibraryKind, LibrarySort[]> = {
  movies: ["title", "year", "added", "rating", "user_rating", "release", "duration", "last_played", "bitrate"],
  shows: ["title", "year", "added", "rating", "user_rating", "release", "last_played", "bitrate"],
  music: ["title", "added", "user_rating", "last_played"],
};

const ITEM_NOUN: Record<LibraryKind, [string, string]> = {
  movies: ["movie", "movies"],
  shows: ["show", "shows"],
  music: ["artist", "artists"],
};

function defaultOrder(sort: LibrarySort): SortOrder {
  return sort === "title" ? "asc" : "desc";
}

export function LibraryPage() {
  const { id } = useParams();
  const libId = idParam(id);
  const lib = useLibrary(libId);
  const user = useAuth();
  const [params, setParams] = useSearchParams();
  const tab = tabOf(params);
  useDocumentTitle(lib.data?.name);

  // Opened without a tab, sort or filters in its address (from the sidebar),
  // a library opens on the tab it was last on, in the view it was left in,
  // before it lists anything.
  const viewKey = viewStorageKey(user.id, libId);
  const tabKey = tabStorageKey(user.id, libId);
  const tabbed = withTab(params, leftTab(tabKey));
  const restore = (withView(tabbed ?? params, leftView(viewKey)) ?? tabbed)?.toString() ?? "";
  useEffect(() => {
    if (restore) setParams(new URLSearchParams(restore), { replace: true });
  }, [restore, setParams]);
  // The tab shown is the one to open on next time (one chosen here is kept as it's chosen).
  useEffect(() => {
    if (!restore && Number.isFinite(libId)) keepTab(tabKey, tab);
  }, [restore, tab, tabKey, libId]);

  const setParam = (updates: Record<string, string | null>) => {
    const next = new URLSearchParams(params);
    for (const [k, v] of Object.entries(updates)) {
      if (v == null || v === "") next.delete(k);
      else next.set(k, v);
    }
    keepView(viewKey, viewOf(next));
    keepTab(tabKey, tabOf(next));
    setParams(next, { replace: true });
  };

  if (!Number.isFinite(libId)) return <NotFoundPage />;
  if (lib.isSuccess && !lib.data) return <NotFoundPage />;

  const kind = lib.data?.kind ?? "movies";

  return (
    <div className="pb-12">
      <div className="flex flex-wrap items-end justify-between gap-4 px-4 pb-1 pt-6 sm:px-8">
        <div className="flex min-w-0 items-center gap-3">
          <div className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-surface-2 text-accent">
            <LibraryIcon kind={kind} className="size-5" />
          </div>
          <div className="min-w-0">
            <h1 className="truncate text-2xl font-bold tracking-tight">{lib.data?.name ?? " "}</h1>
          </div>
        </div>
        <div className="flex rounded-lg bg-surface-2 p-1" role="tablist">
          {(kind === "music" ? (["recommended", "library"] as const) : (["recommended", "library", "collections"] as const)).map((t) => (
            <button
              key={t}
              type="button"
              role="tab"
              aria-selected={tab === t}
              onClick={() => setParam({ tab: t === "library" ? null : t })}
              className={cn(
                "rounded-md px-4 py-1.5 text-sm font-medium capitalize transition-colors",
                tab === t ? "bg-surface-3 text-fg shadow" : "text-muted hover:text-fg",
              )}
            >
              {t}
            </button>
          ))}
        </div>
      </div>

      {kind === "movies" ? <TmdbKeyNotice className="mx-4 mt-4 sm:mx-8" /> : null}
      {tab === "recommended" ? (
        <RecommendedTab libraryId={libId} kind={kind} />
      ) : tab === "collections" && kind !== "music" ? (
        <CollectionsTab libraryId={libId} kind={kind} />
      ) : (
        <LibraryTab
          libraryId={libId}
          kind={kind}
          params={params}
          setParam={setParam}
          ready={lib.isSuccess && !restore}
          libraryTotal={lib.data?.item_count}
        />
      )}
    </div>
  );
}

function RecommendedTab({ libraryId, kind }: { libraryId: number; kind: LibraryKind }) {
  const hubs = useLibraryHubs(libraryId);
  if (hubs.isPending) {
    return (
      <div className="space-y-10 pt-6">
        <HubRowSkeleton />
        <HubRowSkeleton />
      </div>
    );
  }
  if (hubs.isError) {
    return <EmptyState title="Couldn't load recommendations" description={errorMessage(hubs.error)} action={<Button onClick={() => void hubs.refetch()}>Retry</Button>} />;
  }
  const visible = hubs.data.hubs.filter((h) => h.items.length > 0);
  if (!visible.length) {
    return <EmptyState title="Nothing to recommend yet" description="Recommendations appear once the library has some items." />;
  }
  return (
    <div className="space-y-10 pt-6">
      {visible.map((hub) => (
        <HubRow key={hub.key} hub={hub} />
      ))}
      <CollectionsRow libraryId={libraryId} kind={kind} />
    </div>
  );
}

function LibraryTab({
  libraryId,
  kind,
  params,
  setParam,
  ready,
  libraryTotal,
}: {
  libraryId: number;
  kind: LibraryKind;
  params: URLSearchParams;
  setParam: (u: Record<string, string | null>) => void;
  ready: boolean;
  /** Everything the library holds for this profile: "23 of 342 movies" when filtered. */
  libraryTotal?: number;
}) {
  const user = useAuth();
  const toast = useToast();
  const scan = useScanLibrary();
  const [saving, setSaving] = useState(false);
  const sorts = SORTS_BY_KIND[kind];
  const rawSort = params.get("sort") as LibrarySort | null;
  const sort: LibrarySort = rawSort && sorts.includes(rawSort) ? rawSort : "title";
  const order: SortOrder = params.get("order") === "asc" || params.get("order") === "desc" ? (params.get("order") as SortOrder) : defaultOrder(sort);
  const watchedParam = params.get("watched");
  const watched = watchedParam === "true" ? true : watchedParam === "false" ? false : undefined;
  const genre = params.get("genre") ?? undefined;
  const yearParam = params.get("year");
  const year = yearParam ? Number(yearParam) : undefined;
  const resolution = kind === "music" ? undefined : resolutionOf(params.get("resolution"));
  const dynamicRange = kind === "music" ? undefined : rangeFilterOf(params.get("dynamic_range"));

  const query = useMemo(
    () => ({ sort, order, watched, genre, year, resolution, dynamic_range: dynamicRange }),
    [sort, order, watched, genre, year, resolution, dynamicRange],
  );
  const items = useLibraryItems(libraryId, query, ready);
  const filters = useLibraryFilters(libraryId, ready && kind !== "music");

  const aspect: CardAspect = kind === "music" ? "square" : "poster";
  const [cardWidth, setCardWidth] = useLocalStorage<number>(`pley.cardWidth.${aspect}`, aspect === "square" ? 170 : 160);
  const noun = ITEM_NOUN[kind];
  const filtered = watched !== undefined || !!genre || year !== undefined || !!resolution || !!dynamicRange;
  const clearFilters = { watched: null, genre: null, year: null, resolution: null, dynamic_range: null };
  // Offered when the library has two or more; one chosen stays offered so it can be cleared.
  const resolutions = filters.data?.resolutions ?? [];
  const ranges = filters.data?.dynamic_ranges ?? [];

  return (
    <>
      <div className="sticky top-16 z-10 mt-3 border-b border-white/[0.04] bg-bg/85 px-4 py-2.5 backdrop-blur-md sm:px-8">
        <div className="scrollbar-none -mx-4 flex items-center gap-2 overflow-x-auto px-4 sm:mx-0 sm:px-0 [&>*]:shrink-0">
          <Select
            aria-label="Sort by"
            value={sort}
            onChange={(e) => {
              const s = e.target.value as LibrarySort;
              setParam({ sort: s === "title" ? null : s, order: null });
            }}
          >
            {sorts.map((s) => (
              <option key={s} value={s}>
                {SORT_LABELS[s]}
              </option>
            ))}
          </Select>
          <IconButton
            size="sm"
            label={order === "asc" ? "Ascending" : "Descending"}
            onClick={() => setParam({ order: order === "asc" ? "desc" : "asc" })}
            className="rounded-md bg-surface-2 hover:bg-surface-3"
          >
            {order === "asc" ? <ArrowUpNarrowWide className="size-4" /> : <ArrowDownWideNarrow className="size-4" />}
          </IconButton>
          {kind !== "music" ? (
            <Select
              aria-label="Watched filter"
              value={watched === undefined ? "all" : watched ? "watched" : "unwatched"}
              onChange={(e) => {
                const v = e.target.value;
                setParam({ watched: v === "all" ? null : v === "watched" ? "true" : "false" });
              }}
            >
              <option value="all">All {noun[1]}</option>
              <option value="unwatched">Unwatched</option>
              <option value="watched">Watched</option>
            </Select>
          ) : null}
          {kind !== "music" && filters.data && filters.data.genres.length > 0 ? (
            <Select aria-label="Genre" value={genre ?? ""} onChange={(e) => setParam({ genre: e.target.value || null })}>
              <option value="">All genres</option>
              {filters.data.genres.map((g) => (
                <option key={g} value={g}>
                  {g}
                </option>
              ))}
            </Select>
          ) : null}
          {kind !== "music" && filters.data && filters.data.years.length > 0 ? (
            <Select aria-label="Year" value={year ?? ""} onChange={(e) => setParam({ year: e.target.value || null })}>
              <option value="">All years</option>
              {filters.data.years.map((y) => (
                <option key={y} value={y}>
                  {y}
                </option>
              ))}
            </Select>
          ) : null}
          {kind !== "music" && (resolutions.length > 0 || resolution) ? (
            <Select aria-label="Resolution" value={resolution ?? ""} onChange={(e) => setParam({ resolution: e.target.value || null })}>
              <option value="">All resolutions</option>
              {(resolution && !resolutions.includes(resolution) ? [...resolutions, resolution] : resolutions).map((r) => (
                <option key={r} value={r}>
                  {RESOLUTION_LABELS[r]}
                </option>
              ))}
            </Select>
          ) : null}
          {kind !== "music" && (ranges.length > 0 || dynamicRange) ? (
            <Select aria-label="Dynamic range" value={dynamicRange ?? ""} onChange={(e) => setParam({ dynamic_range: e.target.value || null })}>
              <option value="">All dynamic ranges</option>
              {(dynamicRange && !ranges.includes(dynamicRange) ? [...ranges, dynamicRange] : ranges).map((r) => (
                <option key={r} value={r}>
                  {RANGE_FILTER_LABELS[r]}
                </option>
              ))}
            </Select>
          ) : null}
          {filtered ? (
            <Button size="sm" variant="ghost" onClick={() => setParam(clearFilters)}>
              Clear filters
            </Button>
          ) : null}
          {filtered ? (
            <Button size="sm" variant="ghost" icon={<Sparkles className="size-4" />} onClick={() => setSaving(true)}>
              Save as…
            </Button>
          ) : null}
          {saving ? (
            <SaveFiltersModal
              onClose={() => setSaving(false)}
              libraryId={libraryId}
              filter={smartFilterOf(query)}
              allowCollection={user.is_admin && kind !== "music"}
            />
          ) : null}

          <div className="ml-auto flex items-center gap-3">
            <span className="hidden text-sm tabular-nums text-muted sm:inline">
              {items.data
                ? filtered && libraryTotal != null && libraryTotal >= items.data.total
                  ? `${items.data.total.toLocaleString()} of ${pluralize(libraryTotal, noun[0], noun[1])}`
                  : pluralize(items.data.total, noun[0], noun[1])
                : ""}
            </span>
            <input
              type="range"
              min={110}
              max={260}
              step={10}
              value={cardWidth}
              onChange={(e) => setCardWidth(Number(e.target.value))}
              aria-label="Card size"
              title="Card size"
              className="hidden w-24 md:block"
            />
          </div>
        </div>
      </div>

      <div className="px-4 pt-5 sm:px-8">
        {items.isPending || !ready ? (
          <ItemGridSkeleton aspect={aspect} cardWidth={cardWidth} />
        ) : items.isError || !items.data ? (
          <EmptyState title="Couldn't load this library" description={errorMessage(items.error)} action={<Button onClick={() => void items.refetch()}>Retry</Button>} />
        ) : items.data.items.length === 0 ? (
          filtered ? (
            <EmptyState
              title="No matches"
              description="Nothing in this library matches the current filters."
              action={<Button onClick={() => setParam(clearFilters)}>Clear filters</Button>}
            />
          ) : (
            <EmptyState
              icon={<FolderSearch />}
              title="This library is empty"
              description="Pley didn't find any media yet. Check the library folders, then scan again."
              action={
                user.is_admin ? (
                  <Button
                    variant="primary"
                    icon={<RefreshCw className="size-4" />}
                    loading={scan.isPending}
                    onClick={() =>
                      scan.mutate(
                        { id: libraryId },
                        {
                          onSuccess: () => toast.success("Scan started"),
                          onError: (e) => toast.error(errorMessage(e)),
                        },
                      )
                    }
                  >
                    Scan library
                  </Button>
                ) : undefined
              }
            />
          )
        ) : (
          <ItemGrid
            items={items.data.items}
            total={items.data.total}
            aspect={aspect}
            cardWidth={cardWidth}
            jumpBar={sort === "title"}
            media={sort === "bitrate"}
            badges={sort === "bitrate" || !!resolution || !!dynamicRange}
          />
        )}
      </div>
    </>
  );
}

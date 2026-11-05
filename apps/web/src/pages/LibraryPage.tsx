import { useEffect, useMemo, useState } from "react";
import { useParams, useSearchParams } from "react-router";
import { useTranslation } from "react-i18next";
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
import { RANGE_FILTER_LABELS, rangeFilterOf, RESOLUTION_LABELS, resolutionOf } from "@/lib/format";
import { smartFilterOf } from "@/lib/smartFilter";
import { useDocumentTitle, useLocalStorage } from "@/lib/hooks";
import { idParam } from "@/lib/ids";
import { keepTab, keepView, leftTab, leftView, tabOf, tabStorageKey, viewOf, viewStorageKey, withTab, withView } from "@/lib/libraryView";

const SORTS_BY_KIND: Record<LibraryKind, LibrarySort[]> = {
  movies: ["title", "year", "added", "rating", "user_rating", "release", "duration", "last_played", "bitrate"],
  shows: ["title", "year", "added", "rating", "user_rating", "release", "last_played", "bitrate"],
  music: ["title", "added", "user_rating", "last_played"],
};

function defaultOrder(sort: LibrarySort): SortOrder {
  return sort === "title" ? "asc" : "desc";
}

export function LibraryPage() {
  const { t } = useTranslation("pages");
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
          {(kind === "music" ? (["recommended", "library"] as const) : (["recommended", "library", "collections"] as const)).map((name) => (
            <button
              key={name}
              type="button"
              role="tab"
              aria-selected={tab === name}
              onClick={() => setParam({ tab: name === "library" ? null : name })}
              className={cn(
                "rounded-md px-4 py-1.5 text-sm font-medium capitalize transition-colors",
                tab === name ? "bg-surface-3 text-fg shadow" : "text-muted hover:text-fg",
              )}
            >
              {t(`library.tabs.${name}`)}
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
  const { t } = useTranslation(["pages", "common"]);
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
    return (
      <EmptyState
        title={t("library.cantLoadRecommendations")}
        description={errorMessage(hubs.error)}
        action={<Button onClick={() => void hubs.refetch()}>{t("common:actions.retry")}</Button>}
      />
    );
  }
  const visible = hubs.data.hubs.filter((h) => h.items.length > 0);
  if (!visible.length) {
    return <EmptyState title={t("library.nothingToRecommend")} description={t("library.nothingToRecommendHint")} />;
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
  const { t } = useTranslation(["pages", "common"]);
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
            aria-label={t("library.sortBy")}
            value={sort}
            onChange={(e) => {
              const s = e.target.value as LibrarySort;
              setParam({ sort: s === "title" ? null : s, order: null });
            }}
          >
            {sorts.map((s) => (
              <option key={s} value={s}>
                {t(`library.sorts.${s}`)}
              </option>
            ))}
          </Select>
          <IconButton
            size="sm"
            label={order === "asc" ? t("library.ascending") : t("library.descending")}
            onClick={() => setParam({ order: order === "asc" ? "desc" : "asc" })}
            className="rounded-md bg-surface-2 hover:bg-surface-3"
          >
            {order === "asc" ? <ArrowUpNarrowWide className="size-4" /> : <ArrowDownWideNarrow className="size-4" />}
          </IconButton>
          {kind !== "music" ? (
            <Select
              aria-label={t("library.watchedFilter")}
              value={watched === undefined ? "all" : watched ? "watched" : "unwatched"}
              onChange={(e) => {
                const v = e.target.value;
                setParam({ watched: v === "all" ? null : v === "watched" ? "true" : "false" });
              }}
            >
              <option value="all">{t(`library.all.${kind}`)}</option>
              <option value="unwatched">{t("library.unwatched")}</option>
              <option value="watched">{t("library.watched")}</option>
            </Select>
          ) : null}
          {kind !== "music" && filters.data && filters.data.genres.length > 0 ? (
            <Select aria-label={t("library.genre")} value={genre ?? ""} onChange={(e) => setParam({ genre: e.target.value || null })}>
              <option value="">{t("library.allGenres")}</option>
              {filters.data.genres.map((g) => (
                <option key={g} value={g}>
                  {g}
                </option>
              ))}
            </Select>
          ) : null}
          {kind !== "music" && filters.data && filters.data.years.length > 0 ? (
            <Select aria-label={t("library.year")} value={year ?? ""} onChange={(e) => setParam({ year: e.target.value || null })}>
              <option value="">{t("library.allYears")}</option>
              {filters.data.years.map((y) => (
                <option key={y} value={y}>
                  {y}
                </option>
              ))}
            </Select>
          ) : null}
          {kind !== "music" && (resolutions.length > 0 || resolution) ? (
            <Select
              aria-label={t("library.resolution")}
              value={resolution ?? ""}
              onChange={(e) => setParam({ resolution: e.target.value || null })}
            >
              <option value="">{t("library.allResolutions")}</option>
              {(resolution && !resolutions.includes(resolution) ? [...resolutions, resolution] : resolutions).map((r) => (
                <option key={r} value={r}>
                  {RESOLUTION_LABELS[r]}
                </option>
              ))}
            </Select>
          ) : null}
          {kind !== "music" && (ranges.length > 0 || dynamicRange) ? (
            <Select
              aria-label={t("library.dynamicRange")}
              value={dynamicRange ?? ""}
              onChange={(e) => setParam({ dynamic_range: e.target.value || null })}
            >
              <option value="">{t("library.allDynamicRanges")}</option>
              {(dynamicRange && !ranges.includes(dynamicRange) ? [...ranges, dynamicRange] : ranges).map((r) => (
                <option key={r} value={r}>
                  {RANGE_FILTER_LABELS[r]}
                </option>
              ))}
            </Select>
          ) : null}
          {filtered ? (
            <Button size="sm" variant="ghost" onClick={() => setParam(clearFilters)}>
              {t("library.clearFilters")}
            </Button>
          ) : null}
          {filtered ? (
            <Button size="sm" variant="ghost" icon={<Sparkles className="size-4" />} onClick={() => setSaving(true)}>
              {t("library.saveAs")}
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
                  ? t(`library.countOf.${kind}`, { shown: items.data.total, count: libraryTotal })
                  : t(`library.count.${kind}`, { count: items.data.total })
                : ""}
            </span>
            <input
              type="range"
              min={110}
              max={260}
              step={10}
              value={cardWidth}
              onChange={(e) => setCardWidth(Number(e.target.value))}
              aria-label={t("library.cardSize")}
              title={t("library.cardSize")}
              className="hidden w-24 md:block"
            />
          </div>
        </div>
      </div>

      <div className="px-4 pt-5 sm:px-8">
        {items.isPending || !ready ? (
          <ItemGridSkeleton aspect={aspect} cardWidth={cardWidth} />
        ) : items.isError || !items.data ? (
          <EmptyState
            title={t("library.cantLoad")}
            description={errorMessage(items.error)}
            action={<Button onClick={() => void items.refetch()}>{t("common:actions.retry")}</Button>}
          />
        ) : items.data.items.length === 0 ? (
          filtered ? (
            <EmptyState
              title={t("library.noMatches")}
              description={t("library.noMatchesHint")}
              action={<Button onClick={() => setParam(clearFilters)}>{t("library.clearFilters")}</Button>}
            />
          ) : (
            <EmptyState
              icon={<FolderSearch />}
              title={t("library.empty")}
              description={t("library.emptyHint")}
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
                          onSuccess: () => toast.success(t("library.scanStarted")),
                          onError: (e) => toast.error(errorMessage(e)),
                        },
                      )
                    }
                  >
                    {t("library.scan")}
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

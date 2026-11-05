import { useSignal, type Signal } from "@preact/signals";
import type { ComponentChildren } from "preact";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "preact/hooks";
import { errorMessage, isApiError } from "../../api/client";
import type { Collection, Hub, ItemSummary, Library, LibraryFilters, LibraryKind, LibrarySort } from "../../api/types";
import { formatBitrate, formatBytes, pluralize } from "../../format/format";
import { t } from "../../i18n";
import type { AppState } from "../../state";
import { useUi } from "../context";
import { nudge } from "../focus";
import { Chip, PanelRow, Pill, Tab } from "../kit/buttons";
import { F, G } from "../kit/focusable";
import { gridColumns, PosterGrid } from "../kit/grid";
import { Icon, type IconName } from "../kit/icons";
import { useRested } from "../kit/backdrop";
import { useSlides } from "../kit/slides";
import { PanelTitle, SidePanel } from "../kit/panel";
import { Row, rowHeight, SectionPage, type Section } from "../kit/row";
import { ArtTile, SeeAllTile, SkeletonTile, Tile, TILES, type TileKind } from "../kit/tile";
import type { Entry, LibraryTab } from "../shell/navigator";
import { ColumnMessage, failedTitle, problem, sentence, useApp, useLoad } from "./common";
import { seeAllSort } from "./home";
import { pageOf, prefetchPlayback } from "./item-actions";
import {
  AlphaIndex,
  cleared,
  countLabel,
  defaultOrder,
  dynamicRangeLabel,
  fitParams,
  gridStep,
  hasFilter,
  inOrder,
  LETTERS,
  Listing,
  moved,
  ParamsStore,
  qualityChoices,
  resolutionLabel,
  TabStore,
  sameOrder,
  showsBadges,
  showsJump,
  smartFilterOf,
  sortLabel,
  sortsFor,
  withChosen,
  type DynamicRangeFilter,
  type LibraryParams,
  type Resolution,
} from "./library-data";

const CHIPS_TOP = 80;
/** The wells of a grid's focused row, below the chips. */
const GRID_TOP = 136;
const GRID_CLIP = 122;
/** Skeleton cells while the total isn't known: two rows and a bit. */
const UNKNOWN_CELLS = 18;

export function kindIcon(kind: LibraryKind): IconName {
  return kind === "movies" ? "clapperboard" : kind === "shows" ? "tv" : "music";
}

function tabsFor(kind: LibraryKind): LibraryTab[] {
  return kind === "music" ? ["recommended", "library"] : ["recommended", "library", "collections"];
}

/** "12 movies", "3 shows" for a collection. */
export function collectionCount(c: Collection, kind: LibraryKind | null): string {
  return kind === "shows" ? pluralize(c.item_count, "show") : pluralize(c.item_count, "movie");
}

/**
 * The tab a library shows: the one shown before on this page, else the one
 * asked for (See all asks for Library), else the one it was last on, else
 * Library; one the library hasn't is Library.
 */
export function openingTab(tabs: LibraryTab[], before: LibraryTab | undefined, asked: LibraryTab | undefined, kept: LibraryTab | null): LibraryTab {
  const wanted = before ?? asked ?? kept ?? "library";
  return tabs.indexOf(wanted) >= 0 ? wanted : "library";
}

/**
 * A library (TvLibraryScreen): Recommended rows, the whole library as a
 * grid with its sort, filters and A–Z, and its collections.
 */
export function LibraryScreen(p: { entry: Entry; id: number; tab?: LibraryTab; sort?: LibrarySort }) {
  const app = useApp();
  const ui = useUi();
  const libraries = app.libraries.value;
  const library = libraries?.filter((l) => l.id === p.id)[0] ?? null;
  const entry = p.entry;
  const tabs = library ? tabsFor(library.kind) : ["library" as LibraryTab];
  // A library opens on the tab it was last on, else on the whole of it, as on
  // Android; music's Collections is its Library.
  const profile = app.me.value?.id ?? null;
  const tabStore = useMemo(() => new TabStore(app.store, app.server.value ?? "", profile), [app.server.value, profile]);
  const shown = openingTab(tabs, entry.state.tab as LibraryTab | undefined, p.tab, tabStore.get(p.id));
  useEffect(() => {
    if (library) tabStore.set(library.id, shown);
  }, [tabStore, library?.id, shown]);
  const opened = (entry.state.opened as LibraryTab | undefined) ?? shown;
  entry.state.opened = opened;
  const [, setTabState] = useState(0);
  const tabsRef = useRef<HTMLElement | null>(null);

  if (!libraries) return <LibrariesFailed />;
  if (!library) return <ColumnMessage title={t("browse.library.missing")} body={t("browse.library.missingHint")} />;

  const showTab = (tab: LibraryTab) => {
    entry.state.tab = tab;
    setTabState((n) => n + 1);
  };
  // A row's See all: the Library tab in that row's order, until the view is changed there.
  const seeAll = (sort: LibrarySort) => {
    entry.state.seeAll = sort;
    entry.state.params = undefined;
    showTab("library");
  };
  const asked = (entry.state.seeAll as LibrarySort | undefined) ?? p.sort;
  const focusShownTab = () => {
    const el = tabsRef.current?.querySelector<HTMLElement>(`[data-fid="tab/${shown}"]`);
    return !!el && ui.focus.focus(el);
  };
  const initial = shown === opened;
  return (
    <div class="library-page">
      <div class="library-header">
        <div class="t-page c-fg one-line" style="flex:1;min-width:0">
          {library.name}
        </div>
        <G
          kind="row"
          class="library-tabs"
          elRef={(el) => (tabsRef.current = el)}
          onEnter={() => tabsRef.current?.querySelector<HTMLElement>(`[data-fid="tab/${shown}"]`) ?? null}
          onEdge={(dir) => {
            // Recommended's rows are placed in a box of no height of its own, which a move by geometry passes
            // over: Down goes back to the tile focused last in them, else into the nearest row.
            const rows = dir === "down" ? document.querySelector<HTMLElement>(".library-rows .page-scroll") : null;
            if (!rows) return false;
            const target = ui.focus.rememberedIn(rows) ?? ui.focus.enter(rows, "down", tabsRef.current?.getBoundingClientRect() ?? null);
            return !!target && ui.focus.focus(target);
          }}
        >
          {tabs.map((tab) => (
            <Tab key={tab} fid={`tab/${tab}`} label={t(`browse.library.tabs.${tab}`)} shown={tab === shown} onSelect={() => showTab(tab)} />
          ))}
        </G>
      </div>
      {shown === "library" && <LibraryGridTab key={`grid-${library.id}`} entry={entry} library={library} asked={asked} initial={initial} focusTab={focusShownTab} />}
      {shown === "recommended" && (
        <RecommendedTab key={`rec-${library.id}`} entry={entry} library={library} initial={initial} focusTab={focusShownTab} onCollections={() => showTab("collections")} onSeeAll={seeAll} />
      )}
      {shown === "collections" && <CollectionsTab key={`col-${library.id}`} entry={entry} library={library} initial={initial} focusTab={focusShownTab} />}
    </div>
  );
}

// ---- the Library tab

type Panel = null | { kind: "sort" } | { kind: "filter" } | { kind: FilterList; from: "filter" | "chip" };

/** The filters whose choices are a list of their own, opened from the Filter panel or their chip. */
type FilterList = "genre" | "year" | "resolution" | "dynamicRange";

function LibraryGridTab(p: { entry: Entry; library: Library; asked?: LibrarySort; initial: boolean; focusTab: () => boolean }) {
  const app = useApp();
  const ui = useUi();
  const lib = p.library;
  const music = lib.kind === "music";
  const profile = app.me.value?.id ?? null;
  const store = useMemo(() => new ParamsStore(app.store, app.server.value ?? "", profile), [app.server.value, profile]);
  // As it was left on this page, else in a See all's order (unfiltered, not kept), else as it was left on this TV.
  const [params, setParamsState] = useState<LibraryParams>(
    () => (p.entry.state.params as LibraryParams | undefined) ?? (p.asked ? fitParams({ sort: p.asked }, lib.kind) : store.get(lib.id, lib.kind)),
  );
  const [version, setVersion] = useState(0);
  const listing = useMemo(
    () => new Listing(ui.api, lib.id, params, ui.tier === "low" ? 2000 : 5000, () => setVersion((v) => v + 1)),
    [lib.id, params.sort, params.order, params.watched, params.genre, params.year, params.resolution, params.dynamicRange],
  );
  useEffect(() => {
    void listing.start();
    return () => listing.stop();
  }, [listing]);
  // The server's changes: the same listing asked again, what shows stays meanwhile.
  const changes = app.changes.value;
  const firstChanges = useRef(changes);
  useEffect(() => {
    if (changes !== firstChanges.current) void listing.start();
  }, [changes]);
  const filters = useLoad((c) => (music ? Promise.resolve<LibraryFilters>({ genres: [], years: [] }) : ui.api.libraryFilters(lib.id, c)), [lib.id], { live: false });
  const [panel, setPanel] = useState<Panel>(null);
  const [jumpTo, setJumpTo] = useState<{ index: number; n: number } | null>(null);
  // Where focus is in the grid: only the A–Z jump reads it, so a move draws nothing else of the tab again.
  const focusedIndex = useSignal<number>((p.entry.state.gridIndex as number | undefined) ?? 0);
  const chipsRef = useRef<HTMLElement | null>(null);
  const countRef = useRef<HTMLDivElement>(null);
  const panelOpener = useRef<string | null>(null);
  /** How far the chips slide left: with many filters chosen they are wider than the screen. */
  const [chipsShift, setChipsShift] = useState(0);
  // The focused chip in view between the gutter and the safe area's edge, as Search's recent searches on Android and the native app.
  useEffect(
    () =>
      ui.focus.onChange((el) => {
        const row = chipsRef.current;
        if (!el || !row || !row.contains(el)) return;
        const max = Math.max(0, row.scrollWidth - row.clientWidth);
        setChipsShift((s) => nudge(s, el.offsetLeft, el.offsetLeft + el.offsetWidth, 0, row.clientWidth, 0, max));
      }),
    [],
  );
  useLayoutEffect(() => {
    const row = chipsRef.current;
    if (row) {
      // Fewer chips (a filter cleared): no further than they need.
      const max = Math.max(0, row.scrollWidth - row.clientWidth);
      if (chipsShift > max) setChipsShift(max);
    }
    // The count after the chips shows whole or not at all: with many filters chosen, cut short ("23 of 3…") it says nothing.
    const el = countRef.current;
    if (el) el.style.visibility = el.scrollWidth > el.clientWidth ? "hidden" : "";
  });

  const setParams = (next: LibraryParams) => {
    const fitted = fitParams(next, lib.kind);
    store.set(lib.id, fitted);
    p.entry.state.params = fitted;
    p.entry.state.gridIndex = 0;
    focusedIndex.value = 0;
    setParamsState(fitted);
  };

  const total = listing.total;
  const all = listing.all();
  const alpha = useMemo(() => (all ? new AlphaIndex(all) : null), [all ? all.length : -1, version, listing]);
  const jump = showsJump(params, total, listing.windowed) && !!alpha;
  const kind: TileKind = music ? "square" : "grid";
  const count = total ?? UNKNOWN_CELLS;
  const filterSet = hasFilter(params);
  void version;

  const closePanel = () => {
    setPanel(null);
    // Back to the chip that opened the first panel of the sequence, or to Filter when that chip went with its
    // value (Resolution or Dynamic range cleared from its own), as on Android: left on the chip that went, focus
    // would be nowhere, and the next key would go to the tabs.
    const fid = panelOpener.current;
    panelOpener.current = null;
    if (fid) {
      setTimeout(() => {
        const chips = chipsRef.current;
        const el = chips?.querySelector<HTMLElement>(`[data-fid="${fid}"]`) ?? chips?.querySelector<HTMLElement>('[data-fid="chip/filter"]');
        if (el) ui.focus.focus(el);
      }, 0);
    }
  };
  const openPanel = (next: Panel, chip: string) => {
    if (!panelOpener.current) panelOpener.current = chip;
    setPanel(next);
  };

  const cell = (i: number, place: { onFocus: (el: HTMLElement) => void; priority: number }) => {
    const item = listing.at(i);
    if (!item) {
      return <SkeletonTile kind={kind} fid={`grid/cell${i}`} focusable onFocus={place.onFocus} />;
    }
    const bitrate = params.sort === "bitrate";
    return (
      <Tile
        item={item}
        kind={kind}
        fid={`grid/${item.id}`}
        initial={p.initial && i === 0}
        badges={showsBadges(params)}
        subtitle={bitrate ? bitrateLine(item) : undefined}
        priority={place.priority}
        onFocus={place.onFocus}
        onSelect={() => ui.nav.open(pageOf(item))}
        onActions={() => ui.nav.open({ name: "actions", id: item.id })}
      />
    );
  };

  let message: ComponentChildren = null;
  if (listing.error && total === null) {
    message = (
      <ColumnMessage
        top={146}
        title={failedTitle(listing.error, t("browse.library.cantLoad"))}
        body={problem(listing.error)}
        action={{ label: t("common.actions.retry"), icon: "refresh-cw", onSelect: () => setParams({ ...params }) }}
      />
    );
  } else if (total === 0 && filterSet) {
    message = (
      <ColumnMessage
        top={146}
        title={t("browse.library.noMatches")}
        body={t("browse.library.noMatchesHint")}
        action={{ label: t("browse.library.clearFilters"), onSelect: () => setParams(cleared(params)) }}
      />
    );
  } else if (total === 0) {
    message = (
      <ColumnMessage
        top={146}
        title={t("browse.library.empty")}
        body={t("browse.library.emptyHint")}
        focusInstead={p.focusTab}
        takeFocus={p.initial}
      />
    );
  }

  const genres = filters.data?.genres ?? [];
  const years = filters.data?.years ?? [];
  const { resolutions, dynamicRanges } = qualityChoices(filters.data);
  return (
    <>
      <G
        kind="row"
        class="library-chips"
        style={chipsShift ? `transform:translateX(${-chipsShift}px)` : undefined}
        elRef={(el) => (chipsRef.current = el)}
        onEnter={(dir) => (dir === "down" ? chipsRef.current?.querySelector<HTMLElement>("[data-f]") ?? null : null)}
      >
        <Chip
          fid="chip/sort"
          label={sortLabel(params.sort)}
          icon={params.order === "asc" ? "arrow-up-narrow-wide" : "arrow-down-wide-narrow"}
          onSelect={() => openPanel({ kind: "sort" }, "chip/sort")}
        />
        {!music && <Chip fid="chip/filter" label={t("browse.library.filter")} icon="sliders-horizontal" onSelect={() => openPanel({ kind: "filter" }, "chip/filter")} />}
        {!music && (
          <Chip fid="chip/unwatched" label={t("browse.library.unwatched")} selected={params.watched === false} onSelect={() => setParams({ ...params, watched: params.watched === false ? null : false })} />
        )}
        {!music && (genres.length > 0 || params.genre) && (
          <Chip fid="chip/genre" label={params.genre ?? t("browse.library.genre")} selected={!!params.genre} maxWidth={220} onSelect={() => openPanel({ kind: "genre", from: "chip" }, "chip/genre")} />
        )}
        {!music && (years.length > 0 || params.year) && (
          <Chip fid="chip/year" label={params.year ? String(params.year) : t("browse.library.year")} selected={!!params.year} onSelect={() => openPanel({ kind: "year", from: "chip" }, "chip/year")} />
        )}
        {/* The row has no room for two more filters: these are chosen in the Filter panel, and show here while chosen. */}
        {!music && params.resolution && (
          <Chip fid="chip/resolution" label={resolutionLabel(params.resolution)!} selected onSelect={() => openPanel({ kind: "resolution", from: "chip" }, "chip/resolution")} />
        )}
        {!music && params.dynamicRange && (
          <Chip
            fid="chip/dynamicRange"
            label={dynamicRangeLabel(params.dynamicRange)!}
            selected
            onSelect={() => openPanel({ kind: "dynamicRange", from: "chip" }, "chip/dynamicRange")}
          />
        )}
        {total !== null && (
          <div ref={countRef} class="t-caption c-muted chips-count">
            {countLabel(total, lib.kind, filterSet && lib.item_count >= total ? lib.item_count : null)}
          </div>
        )}
      </G>
      {message ?? (
        <PosterGrid
          key={`${params.sort}.${params.order}.${params.watched}.${params.genre}.${params.year}.${params.resolution}.${params.dynamicRange}`}
          kind={kind}
          count={count}
          top={GRID_TOP}
          clipTop={GRID_CLIP}
          enterAt={0}
          startIndex={focusedIndex.peek()}
          focusTo={jumpTo}
          cell={cell}
          onFocusIndex={(i) => {
            focusedIndex.value = i;
            p.entry.state.gridIndex = i;
            listing.showing(Math.max(0, i - gridColumns(kind)));
          }}
          onRightEdge={
            jump
              ? () => {
                  const box = document.querySelector<HTMLElement>(".jump-box");
                  return !!box && ui.focus.focus(box);
                }
              : undefined
          }
        />
      )}
      {!message && <GridRest at={focusedIndex} itemAt={(i) => listing.at(i) ?? null} />}
      {/* Chips slid past the gutter go under the closed rail, as a row's tiles do. */}
      {chipsShift > 0 && <div class="rail-fade" />}
      {jump && alpha && (
        <JumpControl
          alpha={alpha}
          focusedIndex={focusedIndex}
          onJump={(index) => setJumpTo({ index, n: Date.now() })}
          onLeft={() => {
            const g = document.querySelector<HTMLElement>(".grid-cells");
            const back = g ? ui.focus.rememberedIn(g) : null;
            return !!back && ui.focus.focus(back);
          }}
        />
      )}
      {panel?.kind === "sort" && (
        <SortPanel
          kind={lib.kind}
          params={params}
          onClose={closePanel}
          onPick={(next) => {
            setParams(next);
            closePanel();
          }}
        />
      )}
      {panel?.kind === "filter" && (
        <FilterPanel
          music={music}
          params={params}
          resolutions={resolutions.length > 0}
          dynamicRanges={dynamicRanges.length > 0}
          onClose={closePanel}
          onChange={(next) => setParams(next)}
          onList={(kind) => setPanel({ kind, from: "filter" })}
          onClear={() => {
            setParams(cleared(params));
            closePanel();
          }}
          onSaveAs={() => {
            setPanel(null);
            panelOpener.current = null;
            // Back from it lands on the Filter chip.
            p.entry.memory = "chip/filter";
            ui.nav.open({ name: "newPlaylist", smart: { libraryId: lib.id, filter: smartFilterOf(params) } });
          }}
        />
      )}
      {panel?.kind === "genre" && (
        <ChoicePanel
          title={t("browse.library.genre")}
          all={t("browse.library.allGenres")}
          choices={genres}
          current={params.genre}
          onClose={closePanel}
          onPick={(g) => {
            setParams({ ...params, genre: g });
            closePanel();
          }}
        />
      )}
      {panel?.kind === "year" && (
        <ChoicePanel
          title={t("browse.library.year")}
          all={t("browse.library.allYears")}
          choices={years.map(String)}
          current={params.year ? String(params.year) : null}
          onClose={closePanel}
          onPick={(y) => {
            setParams({ ...params, year: y ? Number(y) : null });
            closePanel();
          }}
        />
      )}
      {panel?.kind === "resolution" && (
        <ChoicePanel
          title={t("browse.library.resolution")}
          all={t("browse.library.allResolutions")}
          choices={withChosen(resolutions, params.resolution)}
          labelOf={(r) => resolutionLabel(r) ?? r}
          current={params.resolution}
          onClose={closePanel}
          onPick={(r) => {
            setParams({ ...params, resolution: r as Resolution | null });
            closePanel();
          }}
        />
      )}
      {panel?.kind === "dynamicRange" && (
        <ChoicePanel
          title={t("browse.library.dynamicRange")}
          all={t("browse.library.allDynamicRanges")}
          choices={withChosen(dynamicRanges, params.dynamicRange)}
          labelOf={(d) => dynamicRangeLabel(d) ?? d}
          current={params.dynamicRange}
          onClose={closePanel}
          onPick={(d) => {
            setParams({ ...params, dynamicRange: d as DynamicRangeFilter | null });
            closePanel();
          }}
        />
      )}
    </>
  );
}

/**
 * Where focus rests in the grid (400 ms, no key held): the title's details
 * come, and the server gets a video ready, as on Home's rows, so OK opens
 * its page whole and Play starts at once.
 */
function GridRest(p: { at: Signal<number>; itemAt: (index: number) => ItemSummary | null }) {
  const app = useApp();
  const rested = useRested(p.itemAt(p.at.value));
  useEffect(() => {
    if (!rested) return;
    prefetchPlayback(app, rested);
    // Music has pages of its own (an artist's, an album's), which ask for their own.
    const music = rested.kind === "artist" || rested.kind === "album" || rested.kind === "track";
    if (!music && !app.cachedItem(rested.id)) void app.item(rested.id).catch(() => undefined);
  }, [rested?.id]);
  return null;
}

/** "42 Mbps · 23.4 GB", or "Bitrate unknown". */
function bitrateLine(item: ItemSummary): string {
  const m = item.media;
  const parts = [m?.bitrate ? formatBitrate(m.bitrate) : "", m?.size ? formatBytes(m.size) : ""].filter(Boolean);
  return parts.length ? parts.join(" · ") : t("browse.library.bitrateUnknown");
}

/**
 * The A–Z jump (JumpControl): a box level with the grid's first row that
 * shows the focused title's letter; Up and Down pick another, OK goes there.
 */
function JumpControl(p: { alpha: AlphaIndex; focusedIndex: Signal<number>; onJump: (index: number) => void; onLeft: () => boolean }) {
  const [letter, setLetter] = useState(p.alpha.letterAt(p.focusedIndex.peek()));
  const [focused, setFocused] = useState(false);
  const shownLetter = focused ? letter : p.alpha.letterAt(p.focusedIndex.value);
  const at = LETTERS.indexOf(shownLetter);
  const slot = (l: string | undefined) => (l ? <span class={p.alpha.has(l) ? "c-muted" : "jump-empty"}>{l}</span> : null);
  return (
    <div class="jump">
      <div class="jump-slot t-caption">{focused ? slot(LETTERS[at - 1]) : null}</div>
      <F
        class="jump-box t-title"
        label={t("browse.library.jumpTo", { letter: shownLetter })}
        onFocus={() => {
          setLetter(p.alpha.letterAt(p.focusedIndex.peek()));
          setFocused(true);
        }}
        onBlur={() => setFocused(false)}
        onSelect={() => p.onJump(p.alpha.jumpTarget(letter))}
        onKey={(key) => {
          if (key === "up") {
            const i = LETTERS.indexOf(letter);
            if (i <= 0) return false;
            setLetter(LETTERS[i - 1]!);
            return true;
          }
          if (key === "down") {
            const i = LETTERS.indexOf(letter);
            if (i < LETTERS.length - 1) setLetter(LETTERS[i + 1]!);
            return true;
          }
          if (key === "left") return p.onLeft();
          return key === "right";
        }}
      >
        {shownLetter}
      </F>
      <div class="jump-slot t-caption">{focused ? slot(LETTERS[at + 1]) : null}</div>
    </div>
  );
}

function SortPanel(p: { kind: LibraryKind; params: LibraryParams; onClose: () => void; onPick: (next: LibraryParams) => void }) {
  return (
    <SidePanel onClose={p.onClose} label={t("browse.library.sortBy")}>
      <PanelTitle>{t("browse.library.sortBy")}</PanelTitle>
      {sortsFor(p.kind).map((s: LibrarySort) => (
        <PanelRow
          key={s}
          label={sortLabel(s)}
          checked={s === p.params.sort}
          initial={s === p.params.sort}
          onSelect={() => p.onPick({ ...p.params, sort: s, order: defaultOrder(s) })}
        />
      ))}
      <div class="panel-heading-small t-caption c-muted">{t("browse.library.order")}</div>
      <PanelRow label={t("browse.library.ascending")} checked={p.params.order === "asc"} onSelect={() => p.onPick({ ...p.params, order: "asc" })} />
      <PanelRow label={t("browse.library.descending")} checked={p.params.order === "desc"} onSelect={() => p.onPick({ ...p.params, order: "desc" })} />
    </SidePanel>
  );
}

function FilterPanel(p: {
  music: boolean;
  params: LibraryParams;
  /** The server offers resolutions to choose from; and dynamic ranges. */
  resolutions: boolean;
  dynamicRanges: boolean;
  onClose: () => void;
  onChange: (next: LibraryParams) => void;
  /** A filter's own list. */
  onList: (kind: FilterList) => void;
  onClear: () => void;
  /** Save these filters as a smart playlist (Save as…). */
  onSaveAs: () => void;
}) {
  const w = p.params.watched;
  const { resolution, dynamicRange } = p.params;
  const value = (text: string) => <span class="panel-value t-caption">{text}</span>;
  const chevron = <Icon name="chevron-right" size={20} class="check" />;
  return (
    <SidePanel onClose={p.onClose} label={t("browse.library.filter")}>
      <PanelTitle>{t("browse.library.filter")}</PanelTitle>
      <div class="panel-heading-small t-caption c-muted">{t("browse.library.show")}</div>
      <PanelRow label={t("browse.library.all")} checked={w === null} initial={w === null} onSelect={() => p.onChange({ ...p.params, watched: null })} />
      <PanelRow label={t("browse.library.unwatched")} checked={w === false} initial={w === false} onSelect={() => p.onChange({ ...p.params, watched: false })} />
      <PanelRow label={t("browse.library.watched")} checked={w === true} initial={w === true} onSelect={() => p.onChange({ ...p.params, watched: true })} />
      {!p.music && (
        <>
          <PanelRow label={t("browse.library.genre")} trailing={<>{value(p.params.genre ?? t("browse.library.allGenres"))}{chevron}</>} onSelect={() => p.onList("genre")} />
          <PanelRow
            label={t("browse.library.year")}
            trailing={<>{value(p.params.year ? String(p.params.year) : t("browse.library.allYears"))}{chevron}</>}
            onSelect={() => p.onList("year")}
          />
          {/* When the library has two or more to choose from, or one is chosen, to change or clear it. */}
          {(p.resolutions || resolution) && (
            <PanelRow
              label={t("browse.library.resolution")}
              trailing={<>{value(resolution ? resolutionLabel(resolution)! : t("browse.library.allResolutions"))}{chevron}</>}
              onSelect={() => p.onList("resolution")}
            />
          )}
          {(p.dynamicRanges || dynamicRange) && (
            <PanelRow
              label={t("browse.library.dynamicRange")}
              trailing={<>{value(dynamicRange ? dynamicRangeLabel(dynamicRange)! : t("browse.library.allDynamicRanges"))}{chevron}</>}
              onSelect={() => p.onList("dynamicRange")}
            />
          )}
        </>
      )}
      {hasFilter(p.params) && <PanelRow label={t("browse.library.clearFilters")} onSelect={p.onClear} />}
      {hasFilter(p.params) && <PanelRow label={t("browse.library.saveAs")} icon="save" onSelect={p.onSaveAs} />}
    </SidePanel>
  );
}

/** A filter's list: its "None" choice ("All genres"), then each of the server's, in its words when `labelOf` gives them. */
function ChoicePanel(p: {
  title: string;
  all: string;
  choices: string[];
  labelOf?: (choice: string) => string;
  current: string | null;
  onClose: () => void;
  onPick: (v: string | null) => void;
}) {
  return (
    <SidePanel onClose={p.onClose} label={p.title}>
      <PanelTitle>{p.title}</PanelTitle>
      <PanelRow label={p.all} checked={p.current === null} initial={p.current === null} onSelect={() => p.onPick(null)} />
      {p.choices.map((c) => (
        <PanelRow key={c} label={p.labelOf ? p.labelOf(c) : c} checked={c === p.current} initial={c === p.current} onSelect={() => p.onPick(c)} />
      ))}
    </SidePanel>
  );
}

// ---- Recommended

function RecommendedTab(p: { entry: Entry; library: Library; initial: boolean; focusTab: () => boolean; onCollections: () => void; onSeeAll: (sort: LibrarySort) => void }) {
  const ui = useUi();
  const lib = p.library;
  const music = lib.kind === "music";
  const hubs = useLoad((c) => ui.api.libraryHubs(lib.id, c), [lib.id]);
  const collections = useLoad((c) => (music ? Promise.resolve({ collections: [] as Collection[] }) : ui.api.collections(lib.id, c)), [lib.id]);
  const rows = (hubs.data?.hubs ?? []).filter((h) => h.items.length > 0);
  const cols = collections.data?.collections ?? [];
  const rowIndex = (p.entry.state.recIndex as Record<string, number> | undefined) ?? {};
  p.entry.state.recIndex = rowIndex;
  if (!hubs.data && hubs.error) {
    return (
      <ColumnMessage
        top={CHIPS_TOP}
        title={failedTitle(hubs.error, t("browse.library.cantLoadRecommendations"))}
        body={problem(hubs.error)}
        action={{ label: t("common.actions.retry"), icon: "refresh-cw", onSelect: hubs.reload }}
      />
    );
  }
  if (!hubs.data) return <RowsSkeleton top={CHIPS_TOP} />;
  if (!rows.length && !cols.length) {
    return (
      <ColumnMessage
        top={CHIPS_TOP}
        title={t("browse.library.nothingToRecommend")}
        body={t("browse.library.nothingToRecommendHint")}
        focusInstead={p.focusTab}
        takeFocus={p.initial}
      />
    );
  }
  const sections: Section[] = rows.map((hub: Hub, h) => {
    const kind: TileKind = hub.key.indexOf("continue_watching") === 0 ? "landscape" : music ? "square" : "poster";
    const sort = seeAllSort(hub);
    return {
      key: hub.key,
      height: rowHeight(kind),
      anchor: CHIPS_TOP,
      render: (onFocus) => (
        <Row
          title={hub.title}
          kind={kind}
          items={hub.items}
          keyOf={(i) => i.id}
          enterAt={rowIndex[hub.key]}
          onLeftEdge={ui.toRail}
          onFocusIndex={(i) => {
            rowIndex[hub.key] = i;
            onFocus();
          }}
          tile={(item, i, place) => (
            <Tile
              item={item}
              kind={kind}
              fid={`${hub.key}/${item.id}`}
              initial={p.initial && h === 0 && i === 0}
              priority={place.priority}
              onFocus={place.onFocus}
              onSelect={() => ui.nav.open(pageOf(item))}
              onActions={() => ui.nav.open({ name: "actions", id: item.id, continueWatching: hub.key.indexOf("continue") === 0 })}
            />
          )}
          seeAll={sort ? (place) => <SeeAllTile kind={kind} fid={`${hub.key}/all`} onFocus={place.onFocus} onSelect={() => p.onSeeAll(sort)} /> : undefined}
        />
      ),
    };
  });
  if (cols.length) {
    sections.push({
      key: "collections",
      height: rowHeight("poster"),
      anchor: CHIPS_TOP,
      render: (onFocus) => (
        <Row
          title={t("browse.collections.title")}
          kind="poster"
          items={cols}
          keyOf={(c) => c.id}
          enterAt={rowIndex.collections}
          onLeftEdge={ui.toRail}
          onFocusIndex={(i) => {
            rowIndex.collections = i;
            onFocus();
          }}
          tile={(c, i, place) => (
            <ArtTile
              kind="poster"
              title={c.title}
              subtitle={collectionCount(c, lib.kind)}
              candidates={[c.poster_url]}
              fid={`collections/${c.id}`}
              initial={p.initial && !rows.length && i === 0}
              priority={place.priority}
              onFocus={place.onFocus}
              onSelect={() => ui.nav.open({ name: "collection", id: c.id })}
            />
          )}
          seeAll={(place) => <SeeAllTile kind="poster" fid="collections/all" onFocus={place.onFocus} onSelect={p.onCollections} />}
        />
      ),
    });
  }
  return (
    <div class="library-rows">
      <SectionPage
        sections={sections}
        top={CHIPS_TOP}
        clipAbove={CHIPS_TOP - 12}
        focusedKey={p.entry.state.recSection as string | undefined}
        onFocusSection={(k) => (p.entry.state.recSection = k)}
      />
      <div class="rail-fade" />
    </div>
  );
}

/** Two rows of grey wells while a page's rows load. */
export function RowsSkeleton(p: { top: number; kind?: TileKind }) {
  const spec = TILES[p.kind ?? "poster"];
  const one = (y: number) => (
    <div style={`position:absolute;left:96px;top:${y}px`}>
      <div class="skeleton" style="width:180px;height:18px" />
      <div style="display:flex;margin-top:22px">
        {[0, 1, 2, 3, 4, 5, 6].map((i) => (
          <div key={i} class="skeleton" style={`width:${spec.w}px;height:${spec.h}px;border-radius:4px;margin-right:${spec.gap}px`} />
        ))}
      </div>
    </div>
  );
  return (
    <div aria-label={t("common.states.loading")} role="status">
      {one(p.top)}
      {one(p.top + 18 + 22 + spec.h + 28)}
    </div>
  );
}

// ---- Collections

function CollectionsTab(p: { entry: Entry; library: Library; initial: boolean; focusTab: () => boolean }) {
  const ui = useUi();
  const lib = p.library;
  const loaded = useLoad((c) => ui.api.collections(lib.id, c), [lib.id]);
  const list = loaded.data?.collections ?? null;
  if (!list && loaded.error) {
    return (
      <ColumnMessage
        top={CHIPS_TOP}
        title={failedTitle(loaded.error, t("browse.collections.cantLoad"))}
        body={problem(loaded.error)}
        action={{ label: t("common.actions.retry"), icon: "refresh-cw", onSelect: loaded.reload }}
      />
    );
  }
  if (!list) return null;
  if (!list.length) {
    const body = lib.kind === "movies" ? t("browse.library.noCollectionsHint") : t("browse.library.noCollectionsHere");
    return <ColumnMessage top={CHIPS_TOP} title={t("browse.collections.none")} body={body} focusInstead={p.focusTab} takeFocus={p.initial} />;
  }
  return (
    <>
      <div class="t-caption c-muted collections-count">{pluralize(list.length, "collection")}</div>
      <PosterGrid
        kind="grid"
        count={list.length}
        top={GRID_TOP}
        clipTop={GRID_CLIP}
        startIndex={(p.entry.state.colIndex as number | undefined) ?? 0}
        onFocusIndex={(i) => (p.entry.state.colIndex = i)}
        cell={(i, place) => {
          const c = list[i]!;
          return (
            <ArtTile
              kind="grid"
              title={c.title}
              subtitle={collectionCount(c, lib.kind)}
              candidates={[c.poster_url]}
              badge={String(c.item_count)}
              fid={`collections/${c.id}`}
              initial={p.initial && i === 0}
              priority={place.priority}
              onFocus={place.onFocus}
              onSelect={() => ui.nav.open({ name: "collection", id: c.id })}
            />
          );
        }}
      />
    </>
  );
}

// ---- Libraries (the rail's entry with more than six)

/** "412 movies", "38 shows · Scanning…", "120 artists". */
export function libraryCountLine(l: Library): string {
  const base = l.kind === "music" ? pluralize(l.item_count, "artist") : l.kind === "shows" ? pluralize(l.item_count, "show") : pluralize(l.item_count, "movie");
  return l.scanning ? t("browse.libraries.scanning", { counts: base }) : base;
}

/**
 * The libraries couldn't be had and none were saved (TvLibraries' failure):
 * why, and Retry; nothing while they are on their way.
 */
export function LibrariesFailed(p: { top?: number }) {
  const app = useApp();
  const error = app.accountError.value;
  if (error === null) return null;
  return (
    <ColumnMessage
      top={p.top}
      title={failedTitle(error, t("browse.libraries.cantLoad"))}
      body={problem(error)}
      action={{ label: t("common.actions.retry"), icon: "refresh-cw", onSelect: () => void app.loadAccount().catch(() => undefined) }}
    />
  );
}

/** The cards: 3 across, 261 × 80 and 16 apart, from x 96 and y 8 of the view under the title (from y 81 down). */
const CARD_COLUMNS = 3;
const CARD_W = 261;
const CARD_H = 80;
const CARD_GAP = 16;
const CARDS_VIEW = 540 - 81;
/** While sorting, the hint at the bottom: the view ends above it. */
const HINT_ROOM = 80;

function cardPlace(i: number): [number, number] {
  return [96 + (i % CARD_COLUMNS) * (CARD_W + CARD_GAP), 8 + Math.floor(i / CARD_COLUMNS) * (CARD_H + CARD_GAP)];
}

/** Servers that told this TV it can't sort the libraries: too old for a profile's own order, or not from this app. */
const noSort = new Map<string, "older" | "refused">();

/**
 * Whether the libraries can be put in another order here: there are two
 * or more, and the sign-in may (scope play); "older" or "refused" when
 * the server said it can't.
 */
export function librarySorting(app: AppState): "yes" | "no" | "older" | "refused" {
  if ((app.libraries.value?.length ?? 0) < 2) return "no";
  if ((app.tokenStore?.scopes() ?? []).indexOf("play") < 0) return "refused";
  return noSort.get(app.server.value ?? "") ?? "yes";
}

/** What a server too old to sort the libraries is told: on this page, and in Settings. */
export function sortNeedsNewer(): string {
  return t("browse.libraries.needsNewer");
}

/** {@link sortNeedsNewer}'s words in English, whatever the language shown. */
export const SORT_NEEDS_NEWER = "Sorting libraries needs a newer Pley on the server.";

/**
 * Every library (TvLibrariesScreen): the rail lists six, and Libraries
 * opens this page. A card a library, in the profile's order: its kind's
 * icon, its name, how much it holds, and "Scanning…" while the server
 * scans it.
 *
 * With two or more, the profile may put them in its own order, which
 * every list of them follows on all its devices: Sort libraries by the
 * title, Move in a card's actions (holding OK, or Menu), or Settings' Sort
 * libraries, which opens the page sorting and goes back when it ends.
 * While sorting, OK lifts a card: the arrows move it through the grid
 * (the wheel a row, the pointer to where it points), OK drops it there and
 * the order is saved, Back puts it back. Done or Back ends sorting; "Use
 * the server's order" goes back to the order everyone sees.
 */
export function LibrariesScreen(p: { entry: Entry; sort?: boolean }) {
  const app = useApp();
  const ui = useUi();
  const libraries = app.libraries.value;
  useEffect(() => {
    if (!libraries) void app.loadAccount().catch(() => undefined);
  }, []);
  const sortable = librarySorting(app) === "yes";
  const [sorting, setSorting] = useState(!!p.sort);
  const [lifted, setLifted] = useState<number | null>(null);
  /** The libraries' ids as moved here, until the server's answer lists them so; `null`: as it lists them. */
  const [order, setOrderState] = useState<number[] | null>(null);
  const orderRef = useRef<number[] | null>(null);
  /** The order before the lifted card was lifted: Back puts it back. */
  const before = useRef<number[] | null>(null);
  const [actionsFor, setActionsFor] = useState<Library | null>(null);
  const [shift, setShift] = useState(0);
  const gridRef = useRef<HTMLElement | null>(null);
  const headerRef = useRef<HTMLElement | null>(null);
  const refocus = useRef<string | null>(null);
  const setOrder = (ids: number[] | null) => {
    orderRef.current = ids;
    setOrderState(ids);
  };
  const shown = inOrder(libraries ?? [], order);
  const inSort = sorting && sortable;
  const liftedId = inSort && lifted !== null && shown.some((l) => l.id === lifted) ? lifted : null;

  // Nothing left to sort (a library went, the server said no): the page as it is.
  useEffect(() => {
    if (libraries && !sortable && (sorting || lifted !== null)) {
      setSorting(false);
      setLifted(null);
    }
  }, [sortable, !libraries]);

  // Cards slide to their new places.
  const snapshot = useSlides(gridRef);

  useLayoutEffect(() => {
    const fid = refocus.current;
    refocus.current = null;
    const el = fid ? headerRef.current?.querySelector<HTMLElement>(`[data-fid="${fid}"]`) : null;
    if (el) ui.focus.focus(el);
  });

  const rows = Math.ceil(shown.length / CARD_COLUMNS);
  const height = 8 + rows * (CARD_H + CARD_GAP) - CARD_GAP + 27;
  const view = CARDS_VIEW - (inSort ? HINT_ROOM : 0);
  /** The view to the card at `i`: its row second from the top for the keys; for the pointer, only so far as shows it whole. */
  const place = (i: number, byPointer: boolean) => {
    const top = cardPlace(i)[1];
    const max = Math.max(0, height - view);
    setShift((s) => (byPointer ? nudge(s, top, top + CARD_H, 0, view, 0, max) : Math.min(max, Math.max(0, top - 8 - (CARD_H + CARD_GAP)))));
  };
  const idsNow = () => inOrder(app.libraries.value ?? [], orderRef.current).map((l) => l.id);
  const cardEl = (id: number) => gridRef.current?.querySelector<HTMLElement>(`[data-fid="library/${id}"]`) ?? null;

  const failed = (e: unknown) => {
    const server = app.server.value ?? "";
    if (isApiError(e, 404) || isApiError(e, 405)) {
      noSort.set(server, "older");
      ui.toast(sortNeedsNewer(), true);
    } else if (isApiError(e, 403)) {
      noSort.set(server, "refused");
      ui.toast(t("browse.libraries.refused"), true);
    } else if (isApiError(e, 409)) {
      // A library came or went since they were listed.
      void app.loadAccount().catch(() => undefined);
      ui.toast(t("browse.libraries.changed"), true);
    } else ui.toast(sentence(errorMessage(e)), true);
    if (noSort.has(server)) {
      setSorting(false);
      setLifted(null);
    }
  };
  const save = (ids: number[] | null) => {
    app.orderLibraries(ids).then(
      // The server's answer lists them so now; a newer move keeps its own.
      () => {
        if (orderRef.current === ids) setOrder(null);
      },
      (e) => {
        if (orderRef.current === ids) setOrder(null);
        if (before.current === ids) before.current = null;
        failed(e);
      },
    );
  };

  const startSorting = () => {
    setSorting(true);
    // Ready to lift: the first card.
    const first = gridRef.current?.querySelector<HTMLElement>("[data-f]");
    if (first) ui.focus.focus(first);
  };
  const endSorting = () => {
    // Opened by Settings to sort: back there.
    if (p.sort) {
      ui.nav.pop();
      return;
    }
    if (ui.focus.current?.getAttribute("data-fid") === "serverOrder") refocus.current = "sort";
    setSorting(false);
    setLifted(null);
  };
  const lift = (id: number) => {
    before.current = orderRef.current;
    setLifted(id);
  };
  const moveTo = (to: number, byPointer: boolean) => {
    if (liftedId === null) return;
    const ids = idsNow();
    const from = ids.indexOf(liftedId);
    if (from < 0 || to < 0 || to >= ids.length || to === from) return;
    snapshot();
    setOrder(moved(ids, from, to));
    place(to, byPointer);
  };
  const drop = () => {
    const was = before.current;
    before.current = null;
    setLifted(null);
    const ids = orderRef.current;
    if (!ids) return;
    if (sameOrder(ids, inOrder(app.libraries.value ?? [], was).map((l) => l.id))) setOrder(was);
    else save(ids);
  };
  const putBack = () => {
    const was = before.current;
    before.current = null;
    setLifted(null);
    snapshot();
    setOrder(was);
    if (liftedId !== null) place(Math.max(0, inOrder(app.libraries.value ?? [], was).map((l) => l.id).indexOf(liftedId)), false);
  };
  const serverOrder = () => {
    setLifted(null);
    before.current = null;
    setOrder(null);
    save(null);
  };

  // Back ends sorting (a lifted card's own Back puts it back first).
  useEffect(() => {
    if (!inSort || p.sort) return;
    return ui.input.push((key, e) => {
      if (key !== "back") return false;
      if (e.type === "down" && !e.repeat) endSorting();
      return true;
    });
  }, [inSort, p.sort]);

  if (!libraries) return <LibrariesFailed />;
  if (!libraries.length) return <ColumnMessage title={t("browse.home.noLibraries")} body={t("browse.home.noLibrariesHint")} />;
  const toTarget = (i: number) => {
    // The pointer, with a card lifted: the card goes where it points (past the last, to the end).
    if (ui.input.pointerMoved) moveTo(Math.min(i, shown.length - 1), true);
  };
  const dropByPointer = (e: MouseEvent) => {
    const el = liftedId === null ? null : cardEl(liftedId);
    if (el) ui.input.click(el, e);
  };
  return (
    <div class="libraries-page">
      <div class="libraries-header">
        <div class="t-page c-fg one-line" style="flex:1;min-width:0">
          {inSort ? t("browse.libraries.sort") : t("browse.libraries.title")}
        </div>
        {sortable && (
          <G kind="row" class="libraries-buttons" elRef={(el) => (headerRef.current = el)}>
            {inSort && <Pill key="server" fid="serverOrder" label={t("browse.libraries.serverOrder")} icon="rotate-ccw" disabled={liftedId !== null} onSelect={serverOrder} />}
            <Pill
              key="sort"
              fid="sort"
              label={inSort ? t("common.actions.done") : t("browse.libraries.sort")}
              icon={inSort ? "check" : "arrow-up-down"}
              disabled={liftedId !== null}
              onSelect={inSort ? endSorting : startSorting}
            />
          </G>
        )}
      </div>
      <div class="libraries-viewport">
        <div class="scroll-y" style={`transform:translateY(${-shift}px)`}>
          <G kind="grid" columns={CARD_COLUMNS} restore class="libraries-grid" style={`height:${height}px`} elRef={(el) => (gridRef.current = el)}>
            {shown.map((l, i) => (
              <LibraryCard
                key={l.id}
                library={l}
                index={i}
                count={shown.length}
                sorting={inSort}
                lifted={liftedId === l.id}
                onFocus={() => {
                  place(i, ui.focus.byPointer);
                  p.entry.state.lib = l.id;
                }}
                onSelect={() => {
                  if (liftedId !== null) drop();
                  else if (inSort) lift(l.id);
                  else ui.nav.open({ name: "library", id: l.id });
                }}
                onActions={sortable && liftedId === null ? () => setActionsFor(l) : undefined}
                onMove={(by) => moveTo(gridStep(i, by, shown.length, CARD_COLUMNS), false)}
                onDrop={drop}
                onPutBack={putBack}
                onLeave={putBack}
              />
            ))}
          </G>
          {liftedId !== null && (
            <div class="library-targets">
              {Array.from({ length: rows * CARD_COLUMNS }, (_, i) => {
                const [x, y] = cardPlace(i);
                return <div key={i} class="library-target" style={`left:${x - CARD_GAP / 2}px;top:${y - CARD_GAP / 2}px`} onMouseMove={() => toTarget(i)} onClick={dropByPointer} />;
              })}
            </div>
          )}
        </div>
      </div>
      {inSort && (
        <div class="moving-hint t-label">
          {liftedId !== null ? t("browse.libraries.liftedHint") : t("browse.libraries.sortHint")}
        </div>
      )}
      {actionsFor && (
        <SidePanel onClose={() => setActionsFor(null)} label={actionsFor.name}>
          <div class="panel-title">
            <div class="t-title c-fg lines-2">{actionsFor.name}</div>
            <div class="t-caption c-muted one-line">{libraryCountLine(actionsFor)}</div>
          </div>
          <PanelRow
            label={t("common.actions.open")}
            icon="arrow-right"
            initial
            onSelect={() => {
              setActionsFor(null);
              ui.nav.open({ name: "library", id: actionsFor.id });
            }}
          />
          <PanelRow
            label={t("browse.actions.move")}
            icon="arrow-up-down"
            onSelect={() => {
              setActionsFor(null);
              setSorting(true);
              lift(actionsFor.id);
            }}
          />
          <PanelRow
            label={t("browse.libraries.serverOrder")}
            icon="rotate-ccw"
            onSelect={() => {
              setActionsFor(null);
              serverOrder();
            }}
          />
        </SidePanel>
      )}
    </div>
  );
}

/**
 * A library's card: Surface2; focused, the accent fill at 1.05. While
 * sorting, a grip at its end; lifted, a ring and its own size, and the
 * arrows move it a place along or a row up or down until OK drops it or
 * Back puts it back. Focus going elsewhere (the rail) puts it back too.
 */
function LibraryCard(p: {
  library: Library;
  index: number;
  count: number;
  sorting: boolean;
  lifted: boolean;
  onFocus: () => void;
  onSelect: () => void;
  onActions?: () => void;
  onMove: (by: number) => void;
  onDrop: () => void;
  onPutBack: () => void;
  onLeave: () => void;
}) {
  const l = p.library;
  const [x, y] = cardPlace(p.index);
  const place = `translate(${x}px,${y}px)`;
  return (
    <div class={`library-slot${p.lifted ? " lifted" : ""}`} data-key={l.id} data-place={place} style={`transform:${place}`}>
      <F
        class={`library-card${p.lifted ? " lifted" : ""}`}
        fid={`library/${l.id}`}
        initial={p.index === 0}
        label={`${l.name}, ${libraryCountLine(l)}${p.sorting ? `, ${t("browse.libraries.position", { position: p.index + 1, total: p.count })}` : ""}`}
        onFocus={p.onFocus}
        onBlur={p.lifted ? p.onLeave : undefined}
        onSelect={p.onSelect}
        onLongSelect={p.onActions}
        onMenu={p.onActions}
        onKey={(key) => {
          if (!p.lifted) return false;
          const by = key === "left" ? -1 : key === "right" ? 1 : key === "up" ? -CARD_COLUMNS : key === "down" ? CARD_COLUMNS : 0;
          if (by) {
            p.onMove(by);
            return true;
          }
          if (key === "ok:up") {
            p.onDrop();
            return true;
          }
          if (key === "back") {
            p.onPutBack();
            return true;
          }
          // OK's press, Menu, and every key's release: nothing else while it is lifted.
          return key === "ok" || key === "menu" || key.indexOf(":up") > 0;
        }}
      >
        <div class="library-card-icon">
          <Icon name={kindIcon(l.kind)} size={20} />
        </div>
        <div style="flex:1;min-width:0;margin-left:14px">
          <div class="t-title one-line">{l.name}</div>
          <div class="t-caption one-line card-sub">{libraryCountLine(l)}</div>
        </div>
        {p.sorting && <Icon name="grip-vertical" size={20} class="library-grip" />}
      </F>
    </div>
  );
}

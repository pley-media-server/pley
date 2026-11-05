import { useSignal, type Signal } from "@preact/signals";
import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import type { DiscoverDetail, DiscoverList, DiscoverPage, DiscoverTitle, MediaRequest, RequestList } from "../../api/types";
import { Cancel } from "../../net/transport";
import {
  addTitles,
  DISCOVER_ROWS,
  hasRequests,
  moving,
  problemWords,
  requestsCount,
  requestsProblem,
  requestsTitle,
  searchText,
  seasonsLabel,
  askedLine,
  titleBadge,
  titleFacts,
  titleKey,
  titleSubtitle,
  titleTone,
  TMDB_CREDIT,
} from "../../requests";
import { useUi } from "../context";
import { Backdrop, useRested } from "../kit/backdrop";
import { Pill, Round } from "../kit/buttons";
import { F, G } from "../kit/focusable";
import { gridColumns, PosterGrid } from "../kit/grid";
import { Icon } from "../kit/icons";
import { Row, rowHeight, SectionPage, type Section } from "../kit/row";
import { SeeAllTile, SkeletonTile, TILES } from "../kit/tile";
import type { Entry } from "../shell/navigator";
import { failedTitle, Message, PageBadge, PageTitle, problem, useApp, useLoad } from "./common";
import { keepTitle, RequestsMessage, RequestTile, StagePill, stageOf, summaryProblem, TitleActions, TitleTile, useMarks, usePoll, useTitleChanges } from "./request-kit";

/** TMDB has at most 500 pages of anything. */
const MAX_PAGES = 500;
/**
 * A list reads at most this many pages at a time, as the web's does: for a
 * profile with an age limit, page after page can come empty, and each costs
 * the server TMDB's details of its titles.
 */
const FILL_PAGES = 5;
/** A row reads pages until it has this many titles: more than a screen of posters. */
const ROW_WANT = 12;
/** The first rows load at once; each other as focus comes within two rows of it. */
const LOAD_AHEAD = 2;
const HERO_H = 356;
const ROWS_ANCHOR = 172;
/** A grid of titles (See all, and the See all of search's Discover row) under the page's title. */
const LIST_CLIP = 72;
const LIST_TOP = 96;

/** A list from TMDB as far as it was read: page by page, each title once. */
export interface Pages {
  titles: DiscoverTitle[];
  /** The last page read. */
  page: number;
  totalPages: number;
  /** When the first page came: badges marked since are newer than the list's. */
  at: number;
  error: unknown;
  /** The last fill read its pages without finding the titles it wanted: more are read when asked for (Show more). */
  stopped: boolean;
}

export function noPages(): Pages {
  return { titles: [], page: 0, totalPages: 1, at: 0, error: null, stopped: false };
}

/** Every page read, or as many as TMDB gives: a short page, even an empty one, isn't the end. */
export function pagesDone(p: Pages): boolean {
  return p.page >= Math.min(p.totalPages, MAX_PAGES);
}

/** Nothing more is read unless asked for: every page read, or a fill stopped short or failed. */
export function pagesSettled(p: Pages): boolean {
  return pagesDone(p) || p.stopped || !!p.error;
}

/** `p` and its next page. */
export function withPage(p: Pages, next: DiscoverPage, now = Date.now()): Pages {
  return { titles: addTitles(p.titles, next.results), page: p.page + 1, totalPages: next.total_pages, at: p.at || now, error: null, stopped: false };
}

/**
 * A list read page by page until it holds `want` titles or TMDB has no
 * more, 5 pages at most at a time (`again` reads on, after a failure
 * too), kept in `holder` under `key` so that Back draws it at once; a
 * failure kept there is tried again then. It draws again as titles come
 * and when a fill ends, not for a page that brought none.
 */
export function usePages(
  holder: Record<string, unknown>,
  key: string,
  fetch: (page: number, cancel: Cancel) => Promise<DiscoverPage>,
  want: number,
  enabled = true,
): { pages: Pages; loading: boolean; again: () => void } {
  const [, redraw] = useState(0);
  const [tries, setTries] = useState(0);
  const first = useRef(true);
  if (first.current) {
    first.current = false;
    const kept = holder[key] as Pages | undefined;
    if (kept && kept.error) holder[key] = { ...kept, error: null };
  }
  const pages = (holder[key] as Pages | undefined) ?? noPages();
  const need = enabled && !pagesSettled(pages) && pages.titles.length < want;
  const latest = useRef(fetch);
  latest.current = fetch;
  useEffect(() => {
    if (!need) return;
    const cancel = new Cancel();
    let read = (holder[key] as Pages | undefined) ?? noPages();
    let left = FILL_PAGES;
    const step = (): Promise<void> =>
      latest.current(read.page + 1, cancel).then((page) => {
        if (cancel.cancelled) return;
        const had = read.titles.length;
        read = withPage(read, page);
        left--;
        const short = !pagesDone(read) && read.titles.length < want;
        if (short && !left) read = { ...read, stopped: true };
        holder[key] = read;
        if (read.titles.length !== had || !short || !left) redraw((n) => n + 1);
        return short && left ? step() : undefined;
      });
    step().catch((e: unknown) => {
      if (cancel.cancelled) return;
      holder[key] = { ...read, error: e };
      redraw((n) => n + 1);
    });
    return () => cancel.cancel();
  }, [key, need, want, tries]);
  return {
    pages,
    loading: need,
    again: () => {
      const now = (holder[key] as Pages | undefined) ?? noPages();
      holder[key] = { ...now, error: null, stopped: false };
      setTries((n) => n + 1);
    },
  };
}

/** What the info block and the backdrop follow in the rows. */
type Focused = { title: DiscoverTitle } | { request: MediaRequest };

/**
 * Discover: a hero for the first trending title with a line of its
 * overview and its main action, then TMDB's rows of posters with their
 * badges, then this profile's requests; the way to the requests at the
 * top. Each row loads as it comes near. TMDB's search is in Search.
 */
export function DiscoverScreen(p: { entry: Entry }) {
  const app = useApp();
  const ui = useUi();
  const entry = p.entry;
  const [inRows, setInRows] = useState<boolean>(!!entry.state.inRows);
  const [blocked, setBlocked] = useState(false);
  const bar = useRef<HTMLElement | null>(null);

  const gate = summaryProblem(app);
  if (gate) return <RequestsMessage problem={gate} title="Couldn't load Discover" onRetry={() => void app.loadRequests()} />;
  return (
    <div class="discover-page">
      <DiscoverRows entry={entry} inRows={inRows} setInRows={setInRows} onBlocked={setBlocked} />
      {!blocked && (
        <DiscoverBar
          hidden={inRows}
          elRef={(el) => (bar.current = el)}
          onDown={() => {
            // Down: the hero's button focused last, else its first.
            const page = bar.current?.parentElement;
            const buttons = page?.querySelector(".discover-hero [data-fg]") ?? null;
            const target = (buttons && ui.focus.rememberedIn(buttons)) ?? page?.querySelector<HTMLElement>(".discover-hero [data-f]:not([data-fdisabled])");
            return !!target && ui.focus.focus(target);
          }}
          onFocus={() => {
            if (inRows) setInRows(false);
            entry.state.inRows = false;
          }}
        />
      )}
    </div>
  );
}

/** Discover's top: the way to this profile's requests, with their number, for a profile that has any to see. */
function DiscoverBar(p: { hidden: boolean; elRef: (el: HTMLElement | null) => void; onDown: () => boolean; onFocus: () => void }) {
  const app = useApp();
  const ui = useUi();
  const s = app.requests.value;
  const { count, danger } = requestsCount(s);
  const label = requestsTitle(s);
  if (!hasRequests(s) || !s || !(s.own > 0 || s.manage)) return null;
  return (
    <G
      kind="row"
      class={`discover-bar${p.hidden ? " hidden" : ""}`}
      elRef={p.elRef}
      onEdge={(dir) => {
        if (dir === "down") return p.onDown();
        return dir === "right";
      }}
    >
      <F class="chip t-label requests-chip" fid="requests" label={count ? `${label}, ${count}` : label} onFocus={p.onFocus} onSelect={() => ui.nav.open({ name: "requests" })}>
        <Icon name="inbox" size={20} />
        <span class="one-line">{label}</span>
        {count > 0 && <span class={`chip-count t-badge${danger ? " danger" : ""}`}>{count > 99 ? "99+" : String(count)}</span>}
      </F>
    </G>
  );
}

function DiscoverRows(p: { entry: Entry; inRows: boolean; setInRows: (v: boolean) => void; onBlocked: (blocked: boolean) => void }) {
  const app = useApp();
  const ui = useUi();
  const entry = p.entry;
  const marked = useMarks();
  const s = app.requests.value;
  const focused = useSignal<Focused | null>(null);
  const rowIndex = (entry.state.rowIndex as Record<string, number> | undefined) ?? {};
  entry.state.rowIndex = rowIndex;
  const [section, setSection] = useState<string>((entry.state.section as string | undefined) ?? "hero");
  const at = section === "mine" ? DISCOVER_ROWS.length : DISCOVER_ROWS.map((r) => r.list as string).indexOf(section);
  const reach = Math.max(1, at + LOAD_AHEAD);
  // Trending with nothing for this profile: every row is asked at once, for a hero.
  const first = pagesOf(entry, DISCOVER_ROWS[0]!.list);
  const all = (pagesDone(first) || first.stopped) && !first.titles.length;
  // One list per row, in order: each loads once focus comes within two rows of it.
  const lists = DISCOVER_ROWS.map((r, i) => usePages(entry.state, `list:${r.list}`, (n, c) => ui.api.discover(r.list, n, c), ROW_WANT, all || i <= reach));
  const rows = DISCOVER_ROWS.map((r, i) => {
    const pages = lists[i]!.pages;
    return { ...r, pages, loading: lists[i]!.loading, titles: pages.titles.map((t) => marked(t, pages.at)) };
  });
  const trending = lists[0]!;
  // TMDB not answering for the first list, the page says so once rather than in every row.
  const blocked = !!trending.pages.error && !trending.pages.titles.length ? requestsProblem(trending.pages.error) : null;
  useEffect(() => p.onBlocked(!!blocked), [!!blocked]);

  // This profile's requests, at the foot: asked again when requests change, and while something downloads.
  const own = s ? s.own : 0;
  const changes = app.requestChanges.value.n;
  const mine = useLoad((c) => (own > 0 ? ui.api.requests({ mine: true, limit: 20 }, c) : Promise.resolve(null)), [own > 0, changes], {
    live: false,
    initial: (entry.state.mine as RequestList | undefined) ?? null,
  });
  if (!mine.loading) entry.state.mine = mine.data;
  const mineList = mine.data?.requests ?? [];
  usePoll(mine.reload, mineList.length && moving(mineList) ? 10_000 : app.eventsDown.value ? 30_000 : null);

  // The hero: the first title left (lists leave out what this profile can't ask for); its details for its main action.
  const hero = rows.map((r) => r.titles[0]).filter((t): t is DiscoverTitle => !!t)[0] ?? null;
  const heroKey = hero ? titleKey(hero) : "";
  const heroChanges = useTitleChanges(heroKey);
  const kept = entry.state.hero as DiscoverDetail | undefined;
  const heroLoad = useLoad((c) => (hero ? ui.api.discoverTitle(hero.kind, hero.tmdb_id, c) : Promise.resolve(null)), [heroKey, heroChanges], {
    initial: kept && titleKey(kept) === heroKey ? kept : null,
  });
  const detail = heroLoad.data && titleKey(heroLoad.data) === heroKey ? heroLoad.data : null;
  useEffect(() => {
    if (!detail) return;
    entry.state.hero = detail;
    keepTitle(app, detail);
  }, [detail]);
  // The hero's download, if it has one, moves without events.
  usePoll(heroLoad.reload, detail && moving(detail.requests ?? []) ? 10_000 : null);

  // Back from the rows: the hero again (its main action), then the rail, as on Home.
  const [toHero, setToHero] = useState(0);
  const backToHero = () => {
    const el = document.querySelector<HTMLElement>(".discover-hero [data-f]:not([data-fdisabled])");
    if (el) ui.focus.focus(el);
    else ui.toRail();
  };
  useEffect(
    () =>
      ui.input.push((key, e) => {
        if (key !== "back" || !p.inRows) return false;
        const page = document.querySelector(".discover-rows");
        const now = ui.focus.current;
        if (!page || !now || !page.contains(now)) return false;
        if (e.type !== "down" || e.repeat) return true;
        if (!hero || page.querySelector(".discover-hero [data-f]")) backToHero();
        else {
          entry.state.section = "hero";
          setSection("hero");
          setToHero((n) => n + 1);
        }
        return true;
      }),
    [p.inRows, !!hero],
  );
  useLayoutEffect(() => {
    if (toHero) backToHero();
  }, [toHero]);

  if (blocked) return <RequestsMessage problem={blocked} title="Couldn't load Discover" onRetry={trending.again} />;
  if (!hero) {
    if (rows.every((r) => pagesSettled(r.pages))) return <Message icon="compass" title="Nothing here right now" body="TMDB's lists have nothing this profile can ask for. Search for a title instead." />;
    return <DiscoverSkeleton />;
  }

  /** Focus in a row: on a title or request (`null`: on See all, which shows no info block, as on Home). */
  const inRow = (key: string, onFocus: () => void, f: Focused | null) => {
    onFocus();
    setSection(key);
    entry.state.section = key;
    focused.value = f;
    if (!p.inRows) p.setInRows(true);
    entry.state.inRows = true;
  };
  const sections: Section[] = [
    {
      key: "hero",
      height: HERO_H,
      anchor: null,
      render: (onFocus) => (
        <DiscoverHero
          title={hero}
          detail={detail}
          hidden={p.inRows}
          onFocus={() => {
            onFocus();
            setSection("hero");
            entry.state.section = "hero";
            if (p.inRows) p.setInRows(false);
            entry.state.inRows = false;
          }}
        />
      ),
    },
  ];
  for (const r of rows) {
    // A row TMDB gave nothing for (or couldn't give) goes; one still loading keeps its place.
    if (!r.titles.length && pagesSettled(r.pages)) continue;
    sections.push({
      key: r.list,
      height: rowHeight("poster"),
      anchor: ROWS_ANCHOR,
      render: (onFocus) =>
        r.titles.length ? (
          <Row
            title={r.title}
            kind="poster"
            items={r.titles}
            keyOf={(t) => titleKey(t)}
            enterAt={rowIndex[r.list]}
            onLeftEdge={ui.toRail}
            onFocusIndex={(i) => {
              rowIndex[r.list] = i;
              const t = r.titles[i];
              inRow(r.list, onFocus, t ? { title: t } : null);
            }}
            tile={(t, _i, place) => (
              <TitleTile
                name={t.title}
                subtitle={titleSubtitle(t, r.mixed)}
                poster={t.poster_url}
                badge={titleBadge(t)}
                tone={titleTone(t)}
                fid={`${r.list}/${titleKey(t)}`}
                priority={place.priority}
                onFocus={place.onFocus}
                onSelect={() => ui.nav.open({ name: "title", kind: t.kind, tmdbId: t.tmdb_id })}
              />
            )}
            seeAll={(place) => <SeeAllTile kind="poster" fid={`${r.list}/all`} onFocus={place.onFocus} onSelect={() => ui.nav.open({ name: "discoverList", list: r.list })} />}
          />
        ) : (
          <RowSkeleton title={r.title} />
        ),
    });
  }
  if (mineList.length) {
    sections.push({
      key: "mine",
      height: rowHeight("landscape"),
      anchor: ROWS_ANCHOR,
      render: (onFocus) => (
        <Row
          title="Your requests"
          kind="landscape"
          items={mineList}
          keyOf={(r) => r.id}
          enterAt={rowIndex.mine}
          onLeftEdge={ui.toRail}
          onFocusIndex={(i) => {
            rowIndex.mine = i;
            const r = mineList[i];
            inRow("mine", onFocus, r ? { request: r } : null);
          }}
          tile={(r, _i, place) => {
            const w = stageOf(app, r);
            return (
              <RequestTile
                name={r.title}
                art={[r.backdrop_url, r.poster_url].filter((u): u is string => !!u)}
                stage={w.text}
                tone={w.tone}
                progress={w.progress}
                unseen={r.unseen}
                fid={`mine/${r.id}`}
                priority={place.priority}
                onFocus={place.onFocus}
                onSelect={() => ui.nav.open({ name: "title", kind: r.kind, tmdbId: r.tmdb_id })}
              />
            );
          }}
          seeAll={(place) => <SeeAllTile kind="landscape" fid="mine/all" onFocus={place.onFocus} onSelect={() => ui.nav.open({ name: "requests" })} />}
        />
      ),
    });
  }
  sections.push({ key: "credit", height: 18, anchor: null, render: () => <div class="discover-credit t-caption c-subtle">{TMDB_CREDIT}</div> });

  return (
    <div class="discover-rows">
      <DiscoverFocus hero={detail ?? hero} inRows={p.inRows} focused={focused} />
      <SectionPage sections={sections} clipAbove={ROWS_ANCHOR - 12} focusedKey={entry.state.section as string | undefined} onFocusSection={(k) => (entry.state.section = k)} />
      <div class="rail-fade" />
    </div>
  );
}

/** The backdrop, and in the rows the info block, of where focus rests (400 ms with no key held), as on Home. */
function DiscoverFocus(p: { hero: DiscoverTitle; inRows: boolean; focused: Signal<Focused | null> }) {
  const app = useApp();
  const rested = useRested(p.focused.value);
  const shown = p.inRows ? rested : null;
  const t = shown && "title" in shown ? shown.title : null;
  const r = shown && "request" in shown ? shown.request : null;
  const stage = r ? stageOf(app, r) : null;
  const picture = t ? t.backdrop_url : r ? r.backdrop_url : p.hero.backdrop_url;
  const name = t ? t.title : r ? r.title : p.hero.title;
  return (
    <>
      <Backdrop candidates={picture ? [picture] : []} title={name} scrim={p.inRows ? "rows" : "hero"} />
      <div class={`info-block${shown ? " in" : ""}`} aria-hidden={!shown}>
        <div class="info-title">
          <div class="t-page c-fg one-line">{name}</div>
        </div>
        {t && (
          <>
            <div style="margin-top:10px">
              <TitleMeta title={t} detail={null} />
            </div>
            {t.overview && (
              <div class="t-body lines-2 info-summary" style="margin-top:8px">
                {t.overview}
              </div>
            )}
          </>
        )}
        {r && stage && (
          <>
            <div class="meta-line" style="margin-top:10px;max-width:464px;height:auto">
              <StagePill words={stage} small />
              {r.seasons && r.seasons.length > 0 && <span class="t-label c-muted one-line">{seasonsLabel(r.seasons)}</span>}
            </div>
            <div class="t-caption c-muted one-line" style="margin-top:8px">
              {askedLine(r, !!app.requests.value?.manage)}
            </div>
            {stage.note && (
              <div class="t-caption c-muted one-line" style="margin-top:2px">
                {stage.note}
              </div>
            )}
          </>
        )}
      </div>
    </>
  );
}

/** A title's facts in a line: year and length (or seasons), Movie or Show, the certification, TMDB's rating with a star. */
export function TitleMeta(p: { title: DiscoverTitle; detail: DiscoverDetail | null; maxWidth?: number }) {
  const t = p.detail ?? p.title;
  const facts = p.detail ? titleFacts(p.detail) : t.year ? String(t.year) : "";
  const words = [facts, t.kind === "show" ? "Show" : "Movie"].filter(Boolean).join(" · ");
  const cert = p.detail?.certification ?? null;
  return (
    <div class="meta-line" style={`max-width:${p.maxWidth ?? 464}px`}>
      <span class="t-label c-muted one-line">{words}</span>
      {cert && <span class="t-label c-subtle">·</span>}
      {cert && <PageBadge text={cert} />}
      {t.rating !== null && t.rating > 0 && <span class="t-label c-subtle">·</span>}
      {t.rating !== null && t.rating > 0 && (
        <span class="meta-rating" aria-label={`Rated ${t.rating.toFixed(1)} on TMDB`}>
          <Icon name="star" size={14} color="var(--accent)" />
          <span class="t-label c-fg" style="margin-left:4px">
            {t.rating.toFixed(1)}
          </span>
        </span>
      )}
    </div>
  );
}

function DiscoverHero(p: { title: DiscoverTitle; detail: DiscoverDetail | null; hidden: boolean; onFocus: () => void }) {
  const ui = useUi();
  const t = p.detail ?? p.title;
  const box = useRef<HTMLDivElement>(null);
  // The pointer moving back up into the hero's place, hidden while it was on the rows, brings it back.
  const pointedAt = () => {
    const group = box.current?.querySelector("[data-fg]");
    const el = (group && ui.focus.rememberedIn(group)) ?? box.current?.querySelector<HTMLElement>("[data-f]:not([data-fdisabled])") ?? null;
    if (el && ui.input.pointerMoved && ui.focus.inScope(el)) ui.focus.focus(el, true);
  };
  const open = (pick: boolean) => ui.nav.open({ name: "title", kind: t.kind, tmdbId: t.tmdb_id, pick: pick || undefined });
  return (
    <>
      {p.hidden && <div class="hero-zone" onMouseMove={pointedAt} />}
      <div ref={box} class={`discover-hero${p.hidden ? " hidden" : ""}`}>
        <div class="hero-title">
          <div class="t-hero c-fg lines-2">{t.title}</div>
        </div>
        <div style="margin-top:8px">
          <TitleMeta title={p.title} detail={p.detail} />
        </div>
        {t.overview && <div class="t-body-large one-line hero-summary">{t.overview}</div>}
        <div class="hero-buttons">
          <TitleActions
            detail={p.detail}
            title={p.title}
            fid={(n) => `hero/${n}`}
            initial
            compact
            onFocus={p.onFocus}
            onPick={() => open(true)}
            extra={(initial) => <Round key="details" label="Details" icon="info" fid="hero/details" initial={initial} onFocus={p.onFocus} onSelect={() => open(false)} />}
          />
        </div>
      </div>
    </>
  );
}

/** A row's place while its first page comes: its title and grey wells; nothing takes focus. */
export function RowSkeleton(p: { title: string }) {
  const spec = TILES.poster;
  return (
    <div class="row" aria-label={`Loading ${p.title}`} role="status">
      <div class="row-header t-section one-line">{p.title}</div>
      <div style="display:flex;margin:14px 0 0 96px">
        {[0, 1, 2, 3, 4, 5, 6].map((i) => (
          <div key={i} class="skeleton" style={`width:${spec.w}px;height:${spec.h}px;border-radius:4px;margin-right:${spec.gap}px;flex:none`} />
        ))}
      </div>
    </div>
  );
}

/** Discover while its first list comes (after 300 ms): grey blocks where the hero and the first row go. */
function DiscoverSkeleton() {
  const [slow, setSlow] = useState(false);
  useEffect(() => {
    const t = setTimeout(() => setSlow(true), 300);
    return () => clearTimeout(t);
  }, []);
  if (!slow) return null;
  const block = (x: number, y: number, w: number, h: number, r = 2) => (
    <div class="skeleton" style={`position:absolute;left:${x}px;top:${y}px;width:${w}px;height:${h}px;border-radius:${r}px`} />
  );
  const spec = TILES.poster;
  return (
    <div class="discover-rows" aria-label="Loading Discover" role="status">
      {block(96, 131, 300, 38)}
      {block(96, 180, 200, 18)}
      {block(96, 212, 440, 15)}
      {block(96, 234, 380, 15)}
      {block(96, 272, 128, 40, 6)}
      {block(96, 384, 180, 18)}
      {[0, 1, 2, 3, 4, 5].map((i) => block(96 + i * (spec.w + spec.gap), 422, spec.w, spec.h, 4))}
    </div>
  );
}

/** A grid's place while its first page comes. */
function GridSkeleton(p: { top: number }) {
  const spec = TILES.poster;
  return (
    <div aria-label="Loading" role="status">
      {[0, 1].map((r) =>
        [0, 1, 2, 3, 4, 5].map((c) => (
          <div
            key={`${r}.${c}`}
            class="skeleton"
            style={`position:absolute;left:${96 + c * (spec.w + 14)}px;top:${p.top + r * (spec.h + 68)}px;width:${spec.w}px;height:${spec.h}px;border-radius:4px`}
          />
        )),
      )}
    </div>
  );
}

/** What a grid of titles says instead of titles: none found, or why none came (Retry where it may help). */
function GridNote(p: { title: string; body: string | null; onRetry?: () => void }) {
  return (
    <div class="discover-note" role="status">
      <div class="t-title c-fg">{p.title}</div>
      {p.body && (
        <div class="t-body c-muted" style="margin-top:8px">
          {p.body}
        </div>
      )}
      {p.onRetry && (
        <G kind="row" style="margin-top:20px">
          <Pill label="Retry" icon="refresh-cw" fid="retry" onSelect={p.onRetry} />
        </G>
      )}
    </div>
  );
}

/** Why a grid's first page didn't come, in the words of requests or the app's. */
function gridProblem(e: unknown, title: string): { title: string; body: string | null; retry: boolean } {
  const words = problemWords(requestsProblem(e), false);
  return words ?? { title: failedTitle(e, title), body: problem(e), retry: true };
}

/**
 * Titles as a grid of posters, read a page at a time as focus comes near
 * the end, with a row of grey wells while more come; at the end, Show
 * more when 5 pages brought too few, which reads 5 more, or Retry when a
 * page failed.
 */
function TitleGrid(p: {
  pages: Pages;
  more: (want: number) => void;
  /** Reads on after a fill that stopped short or failed. */
  again: () => void;
  top: number;
  clipTop: number;
  fidPrefix: string;
  /** Where focus was in it (Back to the page). */
  startIndex?: number;
  onFocusIndex: (index: number) => void;
  initial?: boolean;
}) {
  const ui = useUi();
  const marked = useMarks();
  const cols = gridColumns("poster");
  const titles = p.pages.titles.map((t) => marked(t, p.pages.at));
  const coming = !pagesSettled(p.pages);
  const failed = !!p.pages.error;
  const endTile = p.pages.stopped || failed;
  const count = coming ? Math.ceil((titles.length + 1) / cols) * cols : titles.length + (endTile ? 1 : 0);
  return (
    <PosterGrid
      kind="poster"
      count={count}
      top={p.top}
      clipTop={p.clipTop}
      enterAt={0}
      startIndex={p.startIndex !== undefined ? Math.min(p.startIndex, Math.max(0, titles.length - 1)) : undefined}
      onFocusIndex={(i) => {
        p.onFocusIndex(i);
        // Two rows from the end: the next pages, until three rows more are in.
        if (i >= titles.length - 2 * cols) p.more(titles.length + 3 * cols);
      }}
      cell={(i, place) => {
        const t = titles[i];
        if (!t && endTile) {
          return (
            <SeeAllTile
              kind="poster"
              icon={failed ? "refresh-cw" : "plus"}
              label={failed ? "Retry" : "Show more"}
              fid={`${p.fidPrefix}/more`}
              initial={p.initial && i === 0}
              onFocus={place.onFocus}
              onSelect={p.again}
            />
          );
        }
        // The first well holds focus that was on Show more or Retry until the titles come.
        if (!t) return i === titles.length ? <SkeletonTile kind="poster" fid={`${p.fidPrefix}/more`} focusable onFocus={place.onFocus} /> : <SkeletonTile kind="poster" />;
        return (
          <TitleTile
            name={t.title}
            subtitle={titleSubtitle(t, true)}
            poster={t.poster_url}
            badge={titleBadge(t)}
            tone={titleTone(t)}
            fid={`${p.fidPrefix}/${titleKey(t)}`}
            initial={p.initial && i === 0}
            priority={place.priority}
            onFocus={place.onFocus}
            onSelect={() => ui.nav.open({ name: "title", kind: t.kind, tmdbId: t.tmdb_id })}
          />
        );
      }}
    />
  );
}

/**
 * No titles, but more pages to read since a fill stopped short: the grid
 * stays, its one tile Show more (Retry after a failure) and its wells
 * while it reads on, so focus stays in it.
 */
function gridGoesOn(p: Pages): boolean {
  return !p.titles.length && p.page >= FILL_PAGES && !pagesDone(p);
}

/** Beside a grid's lone Show more or Retry: why it has no titles yet. */
function GridAside(p: { pages: Pages; nothing: string; failed: string }) {
  if (p.pages.titles.length || !(p.pages.stopped || p.pages.error)) return null;
  const why = p.pages.error ? gridProblem(p.pages.error, p.failed) : { title: p.nothing, body: "TMDB's pages read so far have nothing this profile can ask for." };
  return (
    <div class="discover-aside" role="status">
      <div class="t-title c-fg">{why.title}</div>
      {why.body && (
        <div class="t-body c-muted" style="margin-top:8px">
          {why.body}
        </div>
      )}
    </div>
  );
}

/** One of Discover's rows, whole (its See all). */
export function DiscoverListScreen(p: { entry: Entry; list: DiscoverList }) {
  const ui = useUi();
  const row = DISCOVER_ROWS.filter((r) => r.list === p.list)[0] ?? DISCOVER_ROWS[0]!;
  return (
    <TitlesPage
      entry={p.entry}
      title={row.title}
      pagesKey={`list:${p.list}`}
      fetch={(n, c) => ui.api.discover(p.list, n, c)}
      fidPrefix={p.list}
      failed="Couldn't load it"
      none={{ title: "Nothing here right now", body: "TMDB's list has nothing this profile can ask for." }}
      noneYet="Nothing here yet"
    />
  );
}

/** Everything TMDB has for the words, each title with its badge (the See all of search's Discover row). */
export function DiscoverSearchScreen(p: { entry: Entry; query: string }) {
  const ui = useUi();
  const words = searchText(p.query);
  return (
    <TitlesPage
      entry={p.entry}
      title={`Results for “${words}”`}
      pagesKey={`q:${words}`}
      fetch={(n, c) => ui.api.discoverSearch(words, n, false, c)}
      fidPrefix="result"
      failed="Couldn't search TMDB"
      none={{ title: `Nothing on TMDB for “${words}”`, body: "Try another spelling, or the title in its own language." }}
      noneYet={`Nothing yet for “${words}”`}
    />
  );
}

/** TMDB's titles as a grid under the page's title, read page by page and kept in the entry under `pagesKey`. */
function TitlesPage(p: {
  entry: Entry;
  title: string;
  pagesKey: string;
  fetch: (page: number, cancel: Cancel) => Promise<DiscoverPage>;
  fidPrefix: string;
  /** What the page says when it couldn't load. */
  failed: string;
  /** What it says when TMDB had nothing. */
  none: { title: string; body: string };
  /** Beside a lone Show more: the pages read so far had nothing. */
  noneYet: string;
}) {
  const app = useApp();
  const [want, setWant] = useState(4 * gridColumns("poster"));
  const read = usePages(p.entry.state, p.pagesKey, p.fetch, want);
  const gate = summaryProblem(app);
  if (gate) return <RequestsMessage problem={gate} title={p.failed} onRetry={() => void app.loadRequests()} />;
  let body;
  if (!read.pages.titles.length && !gridGoesOn(read.pages)) {
    if (read.pages.error) {
      const why = gridProblem(read.pages.error, p.failed);
      body = <GridNote title={why.title} body={why.body} onRetry={why.retry ? read.again : undefined} />;
    } else if (read.loading) body = <GridSkeleton top={LIST_TOP} />;
    else body = <GridNote title={p.none.title} body={p.none.body} />;
  } else {
    body = (
      <>
        <TitleGrid
          pages={read.pages}
          more={(n) => setWant((w) => Math.max(w, n))}
          again={read.again}
          top={LIST_TOP}
          clipTop={LIST_CLIP}
          fidPrefix={p.fidPrefix}
          initial
          startIndex={p.entry.state.gridIndex as number | undefined}
          onFocusIndex={(i) => (p.entry.state.gridIndex = i)}
        />
        <GridAside pages={read.pages} nothing={p.noneYet} failed={p.failed} />
      </>
    );
  }
  return (
    <div class="discover-page">
      {body}
      <PageTitle>{p.title}</PageTitle>
      <div class="rail-fade" />
    </div>
  );
}

/** A list as Discover keeps it on its page. */
export function pagesOf(entry: Entry, list: DiscoverList): Pages {
  return (entry.state[`list:${list}`] as Pages | undefined) ?? noPages();
}

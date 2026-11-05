import { useSignal, type Signal } from "@preact/signals";
import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import type { Hub, ItemDetail, ItemSummary, LibrarySort } from "../../api/types";
import { cardSubtitle, episodeLabel, formatRuntime, formatTimeLeft, isInProgress, isPlayableVideo } from "../../format/format";
import { t } from "../../i18n";
import { useUi } from "../context";
import { artCandidates } from "../kit/art";
import { Backdrop, useRested } from "../kit/backdrop";
import { Pill, Round } from "../kit/buttons";
import { G } from "../kit/focusable";
import { Row, rowHeight, SectionPage, type Section } from "../kit/row";
import { SeeAllTile, Tile, TILES, type TileKind } from "../kit/tile";
import type { Entry } from "../shell/navigator";
import { failedTitle, Message, MetaLine, problem, useApp } from "./common";
import { pageOf, playItem, prefetchPlayback, setWatched } from "./item-actions";

/** A row's tiles (rowShape): Continue Watching wide, music square, episodes wide, the rest posters. */
export function rowKind(hub: Hub, musicLibraries: number[]): TileKind {
  if (hub.key.indexOf("continue") === 0) return "landscape";
  if (hub.library_id !== null && musicLibraries.indexOf(hub.library_id) >= 0) return "square";
  const first = hub.items[0];
  if (first?.kind === "episode") return "landscape";
  if (first && (first.kind === "artist" || first.kind === "album" || first.kind === "track")) return "square";
  return "poster";
}

/** Whether a row offers "See all": the library's newest, newly released or top rated. */
export function seeAllOf(hub: Hub): number | null {
  if (hub.library_id === null) return null;
  return seeAllSort(hub) ? hub.library_id : null;
}

/** The sort "See all" shows a row's library in, as the row lists it (newest or highest first), as on the web. */
export function seeAllSort(hub: Hub): LibrarySort | null {
  if (hub.key.indexOf("recently_added") === 0) return "added";
  if (hub.key.indexOf("recently_released") === 0) return "release";
  if (hub.key.indexOf("top_rated") === 0) return "rating";
  return null;
}

/** The hero: the first of Continue Watching, else the newest video, else the newest item. */
export function heroOf(hubs: Hub[]): ItemSummary | null {
  const cw = hubs.filter((h) => h.key.indexOf("continue") === 0)[0];
  if (cw && cw.items[0]) return cw.items[0];
  let best: ItemSummary | null = null;
  let bestAny: ItemSummary | null = null;
  for (const h of hubs) {
    for (const i of h.items) {
      if (!bestAny || i.added_at > bestAny.added_at) bestAny = i;
      if (isPlayableVideo(i.kind) && (!best || i.added_at > best.added_at)) best = i;
    }
  }
  return best ?? bestAny;
}

/** What the hero and the info block say of an item (homeInfo). */
export function homeInfo(item: ItemSummary, detail: ItemDetail | null) {
  const episode = item.kind === "episode";
  let runtime = "";
  if (item.kind === "movie" || episode) runtime = formatRuntime(item.duration_ms);
  else {
    runtime = cardSubtitle(item);
    if (item.kind === "show" && item.year && runtime === String(item.year)) runtime = "";
  }
  return {
    title: episode ? item.grandparent_title || item.title : item.title,
    context: episode ? [episodeLabel(item.parent_index, item.index), item.title].filter(Boolean).join(" · ") : "",
    runtime,
    summary: (detail?.summary ?? item.summary ?? "").trim(),
    timeLeft: isInProgress(item) ? formatTimeLeft(item.user_state.position_ms, item.duration_ms) : "",
    inProgress: isInProgress(item),
    watched: item.user_state.watched,
  };
}

/** An item's details when the app has them, fetched when it doesn't; `null` until then. */
export function useDetail(id: number | null): ItemDetail | null {
  const app = useApp();
  const [detail, setDetail] = useState<ItemDetail | null>(id !== null ? app.cachedItem(id) : null);
  useEffect(() => {
    if (id === null) {
      setDetail(null);
      return;
    }
    const cached = app.cachedItem(id);
    setDetail(cached);
    if (cached) return;
    let live = true;
    app.item(id).then(
      (d) => live && setDetail(d),
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [id, app.changes.value]);
  return detail && detail.id === id ? detail : null;
}

const HERO_H = 356;
const ROWS_ANCHOR = 172;

/**
 * Home (TvHome): the hero (Continue Watching's first, its buttons), the
 * rows under it, the backdrop of what focus rests on. Drawn at once from
 * what was saved, then from the server.
 */
export function HomeScreen(p: { entry: Entry }) {
  const app = useApp();
  const hubs = app.home.value;
  const error = app.homeError.value;
  const libraries = app.libraries.value;
  useEffect(() => {
    void app.loadHome();
    if (!app.me.value || !app.libraries.value) void app.loadAccount().catch(() => undefined);
  }, []);
  const [slow, setSlow] = useState(false);
  useEffect(() => {
    const t = setTimeout(() => setSlow(true), 300);
    return () => clearTimeout(t);
  }, []);
  const rows = (hubs ?? []).filter((h) => h.items.length > 0);
  if (!hubs) {
    if (libraries && !libraries.length) return <Message title={t("browse.home.noLibraries")} body={t("browse.home.noLibrariesHint")} />;
    if (error) {
      return (
        <Message
          title={failedTitle(error, t("browse.home.cantLoad"))}
          body={problem(error)}
          action={{ label: t("common.actions.retry"), icon: "refresh-cw", onSelect: () => void app.loadHome() }}
        />
      );
    }
    return slow ? <HomeSkeleton /> : null;
  }
  if (!rows.length) {
    if (libraries && !libraries.length) return <Message title={t("browse.home.noLibraries")} body={t("browse.home.noLibrariesHint")} />;
    const scanning = (libraries ?? []).some((l) => l.scanning);
    return scanning ? (
      <Message title={t("browse.home.scanning")} body={t("browse.home.scanningHint")} />
    ) : (
      <Message title={t("browse.home.empty")} body={t("browse.home.emptyHint")} />
    );
  }
  return <HomeRows entry={p.entry} rows={rows} />;
}

/** After the rows' first frame and their first pictures, about. */
const PROBE_AFTER_MS = 1_000;

function HomeRows(p: { entry: Entry; rows: Hub[] }) {
  const app = useApp();
  const ui = useUi();
  const entry = p.entry;
  const rows = p.rows;
  const music = (app.libraries.value ?? []).filter((l) => l.kind === "music").map((l) => l.id);
  const hero = heroOf(rows);
  const [inRows, setInRows] = useState<boolean>(!!entry.state.inRows);
  // The tile focus is on: only the backdrop and the info block read it, so a move draws neither the page nor its rows again.
  const focused = useSignal<ItemSummary | null>(null);
  const heroDetail = useDetail(hero?.id ?? null);
  const rowIndex = (entry.state.rowIndex as Record<string, number> | undefined) ?? {};
  entry.state.rowIndex = rowIndex;

  // The TV's capabilities once the rows are drawn (as Android warms its probe then): a first Play has them.
  useEffect(() => {
    const t = setTimeout(() => void app.caps(), PROBE_AFTER_MS);
    return () => clearTimeout(t);
  }, []);

  // Details of the first three in Continue Watching, ahead of a press.
  useEffect(() => {
    const cw = rows.filter((h) => h.key.indexOf("continue") === 0)[0];
    for (const i of (cw?.items ?? []).slice(0, 3)) if (!app.cachedItem(i.id)) void app.item(i.id).catch(() => undefined);
  }, []);

  // Back from the rows: the page goes back to its hero (focus on Play), then to the rail, as on Android TV.
  // Far down the rows the hero isn't drawn: the page goes to its top first, and Play takes focus once it's there.
  const [toHero, setToHero] = useState(0);
  const backToHero = () => {
    const play = document.querySelector<HTMLElement>('.home [data-fid="hero/play"]');
    if (play) ui.focus.focus(play);
    ui.toRail();
  };
  useEffect(
    () =>
      ui.input.push((key, e) => {
        if (key !== "back" || !inRows) return false;
        const home = document.querySelector(".home");
        const at = ui.focus.current;
        if (!home || !at || !home.contains(at)) return false;
        if (e.type !== "down" || e.repeat) return true;
        if (!hero || home.querySelector('[data-fid="hero/play"]')) backToHero();
        else {
          entry.state.section = "hero";
          setToHero((n) => n + 1);
        }
        return true;
      }),
    [inRows],
  );
  useLayoutEffect(() => {
    if (toHero) backToHero();
  }, [toHero]);

  // Back from a tile's actions that removed it: the tile now in its place.
  useLayoutEffect(() => {
    const memory = entry.memory;
    const standIn = entry.state.standIn as string | undefined;
    if (!memory || !standIn) return;
    entry.state.standIn = undefined;
    if (document.querySelector(`[data-fid="${cssEscape(memory)}"]`)) return;
    const el = document.querySelector<HTMLElement>(`[data-fid="${cssEscape(standIn)}"]`);
    if (el) {
      entry.memory = standIn;
      ui.focus.focus(el);
    }
  }, []);

  const fidOf = (hub: Hub, item: ItemSummary) => `${hub.key}/${item.id}`;
  const standInFor = (h: number, i: number): string | null => {
    const hub = rows[h]!;
    const near = hub.items[i + 1] ?? hub.items[i - 1];
    if (near) return fidOf(hub, near);
    const below = rows[h + 1];
    if (below?.items[0]) return fidOf(below, below.items[0]);
    const above = rows[h - 1];
    return above?.items[0] ? fidOf(above, above.items[0]) : null;
  };

  const sections: Section[] = [];
  if (hero) {
    sections.push({
      key: "hero",
      height: HERO_H,
      anchor: null,
      render: (onFocus) => (
        <Hero
          item={hero}
          detail={heroDetail}
          hidden={inRows}
          onFocus={() => {
            onFocus();
            if (inRows) setInRows(false);
            entry.state.inRows = false;
          }}
        />
      ),
    });
  }
  rows.forEach((hub, h) => {
    const kind = rowKind(hub, music);
    const library = seeAllOf(hub);
    const sort = seeAllSort(hub) ?? undefined;
    sections.push({
      key: hub.key,
      height: rowHeight(kind),
      anchor: ROWS_ANCHOR,
      render: (onFocus) => (
        <Row
          title={hub.title}
          kind={kind}
          items={hub.items}
          keyOf={(item) => item.id}
          enterAt={rowIndex[hub.key]}
          onLeftEdge={ui.toRail}
          onFocusIndex={(i) => {
            rowIndex[hub.key] = i;
            onFocus();
            focused.value = hub.items[i] ?? null;
            if (!inRows) setInRows(true);
            entry.state.inRows = true;
          }}
          tile={(item, i, place) => (
            <Tile
              item={item}
              kind={kind}
              fid={fidOf(hub, item)}
              priority={place.priority}
              onFocus={place.onFocus}
              onSelect={() => ui.nav.open(pageOf(item))}
              onActions={() => {
                entry.state.standIn = standInFor(h, i) ?? undefined;
                ui.nav.open({ name: "actions", id: item.id, continueWatching: hub.key.indexOf("continue") === 0 });
              }}
            />
          )}
          seeAll={
            library !== null
              ? (place) => <SeeAllTile kind={kind} fid={`${hub.key}/all`} onFocus={place.onFocus} onSelect={() => ui.nav.open({ name: "library", id: library, tab: "library", sort })} />
              : undefined
          }
        />
      ),
    });
  });

  return (
    <div class="home">
      <HomeFocus hero={hero} heroDetail={heroDetail} inRows={inRows} focused={focused} rows={rows} />
      <SectionPage
        sections={sections}
        clipAbove={ROWS_ANCHOR - 12}
        focusedKey={entry.state.section as string | undefined}
        onFocusSection={(key) => (entry.state.section = key)}
      />
      <div class="rail-fade" />
    </div>
  );
}

/**
 * The backdrop and, in the rows, the info block: the hero's, then the
 * tile's where focus rests (400 ms with no key held). The one part of Home
 * that follows each move.
 */
function HomeFocus(p: { hero: ItemSummary | null; heroDetail: ItemDetail | null; inRows: boolean; focused: Signal<ItemSummary | null>; rows: Hub[] }) {
  const app = useApp();
  const rested = useRested(p.focused.value);
  const shown = p.inRows ? rested ?? p.hero : p.hero;
  const restedDetail = useDetail(p.inRows && rested ? rested.id : null);

  // Where focus rests: the server gets that video ready, and the next two tiles' details come.
  useEffect(() => {
    if (!rested) return;
    prefetchPlayback(app, rested);
    for (const h of p.rows) {
      const at = h.items.findIndex((i) => i.id === rested.id);
      if (at < 0) continue;
      for (const n of h.items.slice(at + 1, at + 3)) if (!app.cachedItem(n.id)) void app.item(n.id).catch(() => undefined);
      break;
    }
  }, [rested?.id]);

  if (!shown) return null;
  const info = homeInfo(shown, p.inRows ? restedDetail ?? (shown === p.hero ? p.heroDetail : null) : p.heroDetail);
  return (
    <>
      <Backdrop candidates={artCandidates(shown, "backdrop")} title={shown.title} scrim={p.inRows ? "rows" : "hero"} />
      <div class={`info-block${p.inRows ? " in" : ""}`} aria-hidden={!p.inRows}>
        <div class="info-title">
          <div class="t-page c-fg one-line">{info.title}</div>
        </div>
        <div style="margin-top:10px">
          <MetaLine item={shown} detail={p.inRows ? restedDetail : p.heroDetail} lead={info.context} runtime={info.runtime} />
        </div>
        {info.summary && (
          <div class="t-body lines-2 info-summary" style="margin-top:8px">
            {info.summary}
          </div>
        )}
      </div>
    </>
  );
}

function Hero(p: { item: ItemSummary; detail: ItemDetail | null; hidden: boolean; onFocus: () => void }) {
  const app = useApp();
  const ui = useUi();
  const item = p.item;
  const info = homeInfo(item, p.detail);
  const resumeRef = useRef<HTMLElement | null>(null);
  const groupRef = useRef<HTMLElement | null>(null);
  const focus = () => {
    p.onFocus();
    prefetchPlayback(app, item);
  };
  // The pointer moving back up into the hero's place, hidden while it was on the rows, brings it back.
  const pointedAt = () => {
    const group = groupRef.current;
    const el = (group && ui.focus.rememberedIn(group)) ?? resumeRef.current?.querySelector<HTMLElement>("[data-f]") ?? null;
    if (el && ui.input.pointerMoved && ui.focus.inScope(el)) ui.focus.focus(el, true);
  };
  return (
    <>
      {p.hidden && <div class="hero-zone" onMouseMove={pointedAt} />}
      <div class={`hero${p.hidden ? " hidden" : ""}`}>
        <div class="hero-title">
          <div class="t-hero c-fg lines-2">{info.title}</div>
        </div>
        {info.context && <div class="t-title c-fg one-line" style="margin-top:10px;width:464px">{info.context}</div>}
        <div style="margin-top:8px">
          <MetaLine item={item} detail={p.detail} runtime={info.runtime} />
        </div>
        {info.timeLeft && <div class="t-caption c-fg one-line" style="margin-top:10px">{info.timeLeft}</div>}
        {info.summary && <div class="t-body-large lines-3 hero-summary">{info.summary}</div>}
        <G
          kind="row"
          restore
          class="buttons hero-buttons"
          elRef={(el) => (groupRef.current = el)}
          onEnter={() => (groupRef.current && ui.focus.rememberedIn(groupRef.current)) ?? resumeRef.current?.querySelector<HTMLElement>("[data-f]") ?? null}
        >
          <div ref={(el) => {
            resumeRef.current = el;
          }} style="display:flex">
            <Pill label={info.inProgress ? t("common.actions.resume") : t("common.actions.play")} icon="play" fid="hero/play" initial onFocus={focus} onSelect={() => void playItem(app, ui, item)} />
          </div>
          {info.inProgress && <Round label={t("browse.actions.fromStart")} icon="rotate-ccw" fid="hero/start" onFocus={focus} onSelect={() => void playItem(app, ui, item, { fromStart: true })} />}
          <Round label={t("browse.actions.info")} icon="info" fid="hero/info" onFocus={focus} onSelect={() => ui.nav.open(pageOf(item))} />
          <Round
            label={info.watched ? t("browse.actions.markUnwatched") : t("browse.actions.markWatched")}
            icon="circle-check"
            fid="hero/watched"
            onFocus={focus}
            onSelect={() => void setWatched(app, ui, item, !info.watched)}
          />
        </G>
      </div>
    </>
  );
}

/** Home while it loads (after 300 ms): grey blocks where the hero and the first row go; nothing takes focus. */
function HomeSkeleton() {
  const ui = useUi();
  useEffect(() => {
    const t = setTimeout(() => {
      if (!ui.focus.current || !ui.focus.current.isConnected) ui.toRail();
    }, 16);
    return () => clearTimeout(t);
  }, []);
  const block = (x: number, y: number, w: number, h: number, r = 2) => (
    <div class="skeleton" style={`position:absolute;left:${x}px;top:${y}px;width:${w}px;height:${h}px;border-radius:${r}px`} />
  );
  const l = TILES.landscape;
  return (
    <div class="home" aria-label={t("browse.home.loading")} role="status">
      {block(96, 84, 260, 38)}
      {block(96, 132, 210, 18)}
      {block(96, 168, 440, 15)}
      {block(96, 190, 400, 15)}
      {block(96, 212, 260, 15)}
      {block(96, 250, 128, 40, 6)}
      {block(96, 387, 180, 17)}
      {[0, 1, 2, 3].map((i) => block(96 + i * (l.w + l.gap), 422, l.w, l.h, 4))}
    </div>
  );
}

/** CSS.escape where the engine has it; ids and hub keys need little else. */
export function cssEscape(s: string): string {
  const css = (window as unknown as { CSS?: { escape?: (s: string) => string } }).CSS;
  return css?.escape ? css.escape(s) : s.replace(/["\\]/g, "\\$&");
}

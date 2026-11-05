import type { ComponentChildren } from "preact";
import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import { unreachable } from "../../api/client";
import type { DiscoverDetail, DiscoverSeason, RequestSummary, TitleKind } from "../../api/types";
import { t } from "../../i18n";
import {
  allowanceLines,
  askable,
  hasRequests,
  mainAction,
  moving,
  pickerButton,
  pickerSeasons,
  preselected,
  requestsProblem,
  requestsTitle,
  seasonLine,
  seasonName,
  seasonStage,
  titleKey,
  type Tone,
} from "../../requests";
import { useUi } from "../context";
import { Art } from "../kit/art";
import { Backdrop } from "../kit/backdrop";
import { PanelRow, Pill, Round } from "../kit/buttons";
import { F, G } from "../kit/focusable";
import { Icon } from "../kit/icons";
import { memo } from "../kit/memo";
import { PanelTitle, SidePanel, TextDialog } from "../kit/panel";
import { Row, rowHeight, SectionPage, type Section } from "../kit/row";
import { ArtTile, TileCaption, TILES } from "../kit/tile";
import type { Entry } from "../shell/navigator";
import { Message, problem, useApp, useAfterFrame, useLoad } from "./common";
import { TitleMeta } from "./discover";
import { askFor, hours24, keepLastWords, keepTitle, RequestsMessage, TitleActions, useMarks, usePoll, useTitleChanges } from "./request-kit";

/** The hero's section: 52 above it, the focused button's label and 8 below (as the item page). */
const HERO_PAD = 52 + 23 + 8;
const HERO_H = 380;
/** A short hero still puts the first row's header at 400, so the row's captions start under the screen rather than cut at its foot. */
const HERO_MIN = 372;
const ROWS_LINE = 172;

/** Whether a show's seasons may be picked to ask for now. */
export function canPick(d: DiscoverDetail, s: RequestSummary | null): boolean {
  if (d.kind !== "show" || !pickerSeasons(d).some((x) => askable(x, d))) return false;
  const a = mainAction(d, s);
  return a.kind === "request" || (a.kind === "more" && a.more && a.ask);
}

/** A season's badge on its poster: what the season is for this profile, none where it's still to ask for. */
export function seasonBadge(s: DiscoverSeason): { text: string; tone: Tone } | null {
  // A poster's badge has room for a title's short words.
  if (s.state === "library") return { text: t("requests.badge.library"), tone: "green" };
  if (s.state === "partial") return { text: t("requests.badge.partial"), tone: "green" };
  if (s.state === "requested") return { text: t("requests.season.requested"), tone: "blue" };
  if (s.declined) return { text: t("requests.season.declined"), tone: "red" };
  return null;
}

/**
 * A movie or show from TMDB: its backdrop, the hero with its facts,
 * tagline and overview and its one main action, its seasons each with its
 * state (a requested one with its own stage), the cast, and the season
 * picker (`pick`: it opens with the page, from Request more seasons).
 */
export function TitleScreen(p: { entry: Entry; kind: TitleKind; tmdbId: number; pick?: boolean }) {
  const app = useApp();
  const ui = useUi();
  const key = titleKey({ kind: p.kind, tmdb_id: p.tmdbId });
  const changes = useTitleChanges(key);
  const kept = p.entry.state.detail as DiscoverDetail | undefined;
  const loaded = useLoad((c) => ui.api.discoverTitle(p.kind, p.tmdbId, c), [key, changes], { initial: kept ?? null });
  const d = loaded.data;
  useEffect(() => {
    if (!d) return;
    p.entry.state.detail = d;
    keepTitle(app, d);
  }, [d]);
  // What was shown stays when asking again fails: a toast says why (the shell's notice, when the server doesn't answer).
  useEffect(() => {
    if (d && loaded.error && !unreachable(loaded.error)) ui.toast(problem(loaded.error), true);
  }, [loaded.error]);
  // Downloads move without events: asked again every 10 s while one for the title does (not after a failure, which said so once).
  usePoll(loaded.reload, d && !loaded.error && moving(d.requests ?? []) ? 10_000 : null);
  if (!d) {
    if (loaded.error) {
      const why = requestsProblem(loaded.error);
      if (why.kind === "missing") {
        return <Message title={t("requests.title.notOnTmdb")} body={t("requests.title.notOnTmdbText")} action={{ label: t("common.actions.back"), onSelect: () => ui.nav.pop() }} />;
      }
      return <RequestsMessage problem={why} title={t("requests.title.loadFailed")} onRetry={loaded.reload} />;
    }
    return <TitleSkeleton />;
  }
  return <TitlePage entry={p.entry} detail={d} pick={!!p.pick} />;
}

function TitleSkeleton() {
  const b = (y: number, w: number, h: number, r = 2) => <div class="skeleton" style={`position:absolute;left:96px;top:${y}px;width:${w}px;height:${h}px;border-radius:${r}px`} />;
  return (
    <div class="item-page" role="status" aria-label={t("common.states.loading")}>
      {b(82, 320, 40)}
      {b(136, 220, 18)}
      {b(166, 160, 14)}
      {b(196, 464, 15)}
      {b(218, 440, 15)}
      {b(240, 300, 15)}
      {b(282, 140, 40, 6)}
    </div>
  );
}

function TitlePage(p: { entry: Entry; detail: DiscoverDetail; pick: boolean }) {
  const app = useApp();
  const ui = useUi();
  const d = p.detail;
  const entry = p.entry;
  const s = app.requests.value;
  useMarks();
  const [section, setSection] = useState<string>((entry.state.section as string | undefined) ?? "hero");
  const rowIndex = (entry.state.rowIndex as Record<string, number> | undefined) ?? {};
  entry.state.rowIndex = rowIndex;
  const heroEl = useRef<HTMLDivElement | null>(null);
  const [heroH, setHeroH] = useState(HERO_H);
  const [picker, setPicker] = useState<{ start: number | null } | null>(null);
  const [overview, setOverview] = useState(false);
  const [cut, setCut] = useState(false);
  const pickable = canPick(d, s);

  // The hero's height, measured when what it says changes.
  useLayoutEffect(() => {
    const el = heroEl.current;
    if (!el) return;
    const h = HERO_PAD + el.offsetHeight;
    if (h !== heroH) setHeroH(h);
  }, [d, s, cut]);

  // Opened to pick (Request more seasons): the picker, once, over the page's main action.
  useEffect(() => {
    if (!p.pick || entry.state.picked) return;
    entry.state.picked = true;
    if (pickable) setPicker({ start: null });
  }, []);

  // The picker gone, focus is back on what opened it; where that went too (it opened with the page), on the main action.
  const picking = !!picker;
  useLayoutEffect(() => {
    if (picking) return;
    const at = ui.focus.current;
    if (at && at.isConnected) return;
    const main = document.querySelector<HTMLElement>('.title-page [data-fid="main"]:not([data-fdisabled])');
    if (main) ui.focus.focus(main);
  }, [picking]);

  // A page with nothing to focus (it can't be asked for, and no rows): the rail, so the remote is never dead.
  useAfterFrame(() => {
    const page = document.querySelector(".title-page");
    if (page && !page.querySelector("[data-f]:not([data-fdisabled])")) ui.toRail();
  }, [d.status]);

  const toSection = (key: string, onFocus: () => void) => {
    onFocus();
    setSection(key);
    entry.state.section = key;
  };
  const sections: Section[] = [];
  const pointedAtHero = () => {
    const group = heroEl.current?.querySelector("[data-fg]");
    const el = (group && ui.focus.rememberedIn(group)) ?? heroEl.current?.querySelector<HTMLElement>("[data-f]:not([data-fdisabled])") ?? null;
    if (el && ui.input.pointerMoved && ui.focus.inScope(el)) ui.focus.focus(el, true);
  };
  sections.push({
    key: "hero",
    height: Math.max(heroH, HERO_MIN),
    anchor: null,
    render: (onFocus) => [
      section !== "hero" && <div key="zone" class="hero-zone" onMouseMove={pointedAtHero} />,
      <TitleHero
        key="hero"
        heroRef={heroEl}
        detail={d}
        hidden={section !== "hero"}
        onFocus={() => toSection("hero", onFocus)}
        onPick={() => setPicker({ start: null })}
        onCut={setCut}
        extra={(initial) => (
          <>
            {initial && hasRequests(s) && (
              <Round key="requests" label={requestsTitle(s)} icon="inbox" fid="requests" initial onFocus={() => toSection("hero", onFocus)} onSelect={() => ui.nav.open({ name: "requests" })} />
            )}
            {cut && (
              <Round key="overview" label={t("requests.buttons.overview")} icon="ellipsis" fid="overview" onFocus={() => toSection("hero", onFocus)} onSelect={() => setOverview(true)} />
            )}
          </>
        )}
      />,
    ],
  });

  const seasons = d.kind === "show" ? pickerSeasons(d) : [];
  if (seasons.length) {
    const requests = d.requests ?? [];
    sections.push({
      key: "seasons",
      height: rowHeight("poster") + SEASON_STAGE_LINE,
      anchor: ROWS_LINE,
      render: (onFocus) => (
        <Row
          title={t("requests.title.seasons")}
          kind="poster"
          items={seasons}
          keyOf={(x) => x.number}
          enterAt={rowIndex.seasons}
          onLeftEdge={ui.toRail}
          onFocusIndex={(i) => {
            rowIndex.seasons = i;
            toSection("seasons", onFocus);
          }}
          tile={(x, _i, place) => {
            const r = x.request_id !== null ? requests.filter((q) => q.id === x.request_id)[0] ?? null : null;
            const badge = seasonBadge(x);
            const stage = r ? seasonStage(r, x.number, { manage: !!s?.manage, hours24: hours24(app) }) : null;
            const caption = seasonCaption(x, stage);
            return (
              <SeasonTile
                name={x.name || seasonName(x.number)}
                line={caption.text}
                lineTone={caption.tone}
                progress={stage ? stage.progress : null}
                badge={badge ? badge.text : null}
                badgeTone={badge ? badge.tone : "gray"}
                art={[x.poster_url, d.poster_url].filter((u): u is string => !!u)}
                fid={`season/${x.number}`}
                priority={place.priority}
                onFocus={place.onFocus}
                onSelect={() => {
                  if (pickable && askable(x, d)) setPicker({ start: x.number });
                }}
              />
            );
          }}
        />
      ),
    });
  }
  if (d.cast && d.cast.length) {
    sections.push({
      key: "cast",
      height: rowHeight("person"),
      anchor: ROWS_LINE,
      render: (onFocus) => (
        <Row
          title={t("requests.title.cast")}
          kind="person"
          items={d.cast}
          keyOf={(c, i) => `${i}.${c.name}`}
          enterAt={rowIndex.cast}
          onLeftEdge={ui.toRail}
          onFocusIndex={(i) => {
            rowIndex.cast = i;
            toSection("cast", onFocus);
          }}
          tile={(c, i, place) => (
            <ArtTile
              kind="person"
              title={c.name}
              subtitle={c.character ?? ""}
              candidates={c.photo_url ? [c.photo_url] : []}
              fid={`cast/${i}`}
              priority={place.priority}
              onFocus={place.onFocus}
            />
          )}
        />
      ),
    });
  }
  sections.push({ key: "credit", height: 18, anchor: null, render: () => <div class="discover-credit t-caption c-subtle">{t("requests.tmdb.credit")}</div> });

  const showInfo = section !== "hero";
  return (
    <div class="item-page title-page">
      <Backdrop candidates={d.backdrop_url ? [d.backdrop_url] : []} title={d.title} scrim="item" />
      {/* No backdrop on TMDB: the poster stands at the right instead. */}
      {!d.backdrop_url && d.poster_url && !showInfo && (
        <div class="title-poster" aria-hidden="true">
          <Art candidates={[d.poster_url]} width={200} title={d.title} class="tile-clip" />
        </div>
      )}
      {showInfo && <TitleInfo detail={d} />}
      <SectionPage sections={sections} clipAbove={showInfo ? ROWS_LINE - 12 : undefined} focusedKey={section} />
      <div class="rail-fade" />
      {picker && <SeasonPicker detail={d} start={picker.start} onClose={() => setPicker(null)} onAsked={() => setPicker(null)} />}
      {overview && <TextDialog title={d.title} text={[d.tagline ?? "", d.overview].filter(Boolean).join("\n\n")} onClose={() => setOverview(false)} />}
    </div>
  );
}

/** The block over the rows once focus leaves the hero: the title, its facts, the overview. */
function TitleInfo(p: { detail: DiscoverDetail }) {
  const d = p.detail;
  return (
    <div class="item-info in">
      <div class="t-hero c-fg one-line">{d.title}</div>
      <div style="margin-top:10px">
        <TitleMeta title={d} detail={d} />
      </div>
      {d.overview && (
        <div class="t-body lines-2 info-summary" style="margin-top:8px">
          {d.overview}
        </div>
      )}
    </div>
  );
}

function TitleHero(p: {
  heroRef: { current: HTMLDivElement | null };
  detail: DiscoverDetail;
  hidden: boolean;
  onFocus: () => void;
  onPick: () => void;
  onCut: (cut: boolean) => void;
  extra: (initial: boolean) => ComponentChildren;
}) {
  const d = p.detail;
  const facts = [d.network ?? "", (d.genres ?? []).join(" · ")].filter(Boolean).join(" · ");
  const overview = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = overview.current;
    p.onCut(!!el && el.scrollHeight > el.clientHeight + 1);
  }, [d.overview]);
  return (
    <div ref={p.heroRef} class={`item-hero${p.hidden ? " hidden" : ""}`}>
      <div class="hero-title">
        <div class="t-hero c-fg lines-2">{d.title}</div>
      </div>
      <div style="margin-top:12px">
        <TitleMeta title={d} detail={d} />
      </div>
      {facts && (
        <div class="t-caption c-muted one-line" style="margin-top:6px">
          {facts}
        </div>
      )}
      {d.tagline && (
        <div class="t-body c-fg one-line" style="margin-top:10px">
          {d.tagline}
        </div>
      )}
      {d.overview && (
        <div ref={overview} class="t-body-large lines-3 hero-summary">
          {d.overview}
        </div>
      )}
      <div class="item-buttons">
        <TitleActions detail={d} title={d} fid={(n) => n} initial onFocus={p.onFocus} onPick={p.onPick} extra={p.extra} />
      </div>
    </div>
  );
}

/** Under a season's name: its own stage from its request in the stage's colour, else what it holds or when it airs. */
function seasonCaption(s: DiscoverSeason, stage: { text: string; tone: Tone } | null): { text: string; tone: Tone | null } {
  if (stage) return { text: stage.text, tone: stage.tone };
  if (s.state === "unaired") return { text: seasonLine(s), tone: null };
  return { text: s.episodes ? t("requests.counts.episodes", { count: s.episodes }) : seasonLine(s), tone: null };
}

/** A season's stage may take a second line under its name ("Waiting for / approval"). */
const SEASON_STAGE_LINE = 17;

/** A season as a poster: its state's badge and a download's progress on it, its name, and its stage (whole) or episodes under it. */
const SeasonTile = memo(SeasonTileView, ["priority"]);

function SeasonTileView(p: {
  name: string;
  line: string;
  lineTone: Tone | null;
  /** The download's bar on the poster, 0–1, while the season downloads. */
  progress: number | null;
  badge: string | null;
  badgeTone: Tone;
  art: string[];
  fid: string;
  priority?: number;
  onFocus?: (el: HTMLElement) => void;
  onSelect: () => void;
}) {
  const spec = TILES.poster;
  const [first, ...rest] = p.line.split(" · ");
  return (
    <F class="tile" style={`width:${spec.w}px`} fid={p.fid} label={[p.name, p.badge, p.line].filter(Boolean).join(", ")} onSelect={p.onSelect} onFocus={p.onFocus}>
      <div class="tile-well" style={`width:${spec.w}px;height:${spec.h}px`}>
        <div class="tile-clip">
          <Art candidates={p.art} width={spec.w} title={p.name} showTitle class="tile-clip" priority={p.priority} />
          <div class="tile-hairline" />
          {p.progress !== null && (
            <div class="tile-progress">
              <div style={`width:${(Math.max(0, Math.min(1, p.progress)) * 100).toFixed(1)}%`} />
            </div>
          )}
          {p.badge && <div class={`title-badge t-badge tone-${p.badgeTone}`}>{p.badge}</div>}
        </div>
        <div class="tile-ring" />
      </div>
      {p.lineTone ? (
        <div class="tile-caption season-caption" aria-hidden="true">
          <div class="title t-caption one-line">{p.name}</div>
          <div class={`sub t-caption lines-2 tone-text-${p.lineTone}`}>
            {/* A download's time comes on the second line ("Downloading 70%" / "in 2 hours"), not after a dot that wrapped. */}
            {rest.length ? (
              <>
                {first}
                <br />
                {rest.join(" · ")}
              </>
            ) : (
              p.line
            )}
          </div>
        </div>
      ) : (
        <TileCaption title={p.name} subtitle={p.line} />
      )}
    </F>
  );
}

/**
 * Which seasons?: every season, specials last; the askable missing
 * and partly had ones ticked as it opens, lowest first, up to what may
 * still be asked for; seasons not aired yet and specials offered
 * unticked; the others say what they are and can't be ticked. Ticking
 * more than may be asked for disables the button and says why.
 */
export function SeasonPicker(p: { detail: DiscoverDetail; start: number | null; onClose: () => void; onAsked: () => void }) {
  const app = useApp();
  const ui = useUi();
  const d = p.detail;
  const s = app.requests.value;
  const left = d.seasons_left;
  const list = pickerSeasons(d);
  const [ticked, setTicked] = useState<number[]>(() => {
    const out = preselected(d, left);
    const start = p.start !== null ? list.filter((x) => x.number === p.start)[0] : undefined;
    // The season chosen to open it: ticked too.
    if (start && askable(start, d) && out.indexOf(start.number) < 0) out.push(start.number);
    return out;
  });
  const [busy, setBusy] = useState(false);
  const over = left !== null && ticked.length > left;
  const firstAskable = list.filter((x) => askable(x, d))[0]?.number ?? null;
  const focusAt = p.start ?? firstAskable;
  const toggle = (n: number) => setTicked((was) => (was.indexOf(n) >= 0 ? was.filter((x) => x !== n) : was.concat([n])));
  const submit = () => {
    if (busy || !ticked.length || over) return;
    setBusy(true);
    void askFor(app, ui, { kind: "show", tmdb_id: d.tmdb_id, seasons: ticked.slice().sort((a, b) => a - b) }).then((r) => {
      setBusy(false);
      if (r) p.onAsked();
    });
  };
  const lines = over || (left !== null && left <= 0)
    ? allowanceLines({
        movies_left: d.movies_left,
        seasons_left: d.seasons_left,
        movies_next_at: d.movies_next_at,
        seasons_next_at: d.seasons_next_at,
        movies_limit: s?.movies_limit ?? null,
        seasons_limit: s?.seasons_limit ?? null,
        kind: "show",
      })
    : null;
  return (
    <SidePanel onClose={p.onClose} label={t("requests.picker.title")}>
      <PanelTitle>{t("requests.picker.title")}</PanelTitle>
      <div class="picker-for t-caption c-muted one-line">{d.title}</div>
      {list.map((x) => {
        const can = askable(x, d);
        const on = ticked.indexOf(x.number) >= 0;
        return (
          <PanelRow
            key={x.number}
            fid={`pick/${x.number}`}
            label={x.name || seasonName(x.number)}
            sub={seasonLine(x)}
            disabled={!can}
            initial={x.number === focusAt}
            trailing={can ? <Tick on={on} /> : undefined}
            onSelect={() => toggle(x.number)}
          />
        );
      })}
      {lines && (
        <div class="picker-note t-caption c-muted">
          {lines.map((l, i) =>
            // More ticked than may be asked for: the line saying how many may, by the button it turned off, stands out.
            over && i === 0 ? (
              <div key={l} class="picker-over">
                <Icon name="circle-alert" size={14} />
                <span>{keepLastWords(l)}</span>
              </div>
            ) : (
              <div key={l}>{keepLastWords(l)}</div>
            ),
          )}
        </div>
      )}
      <G kind="row" class="picker-foot">
        <Pill label={pickerButton(ticked.length)} icon={ticked.length ? "plus" : undefined} fid="pick/ask" disabled={busy || !ticked.length || over} onSelect={submit} />
      </G>
    </SidePanel>
  );
}

/** A tick box: ticked or not, at a picker row's end. */
function Tick(p: { on: boolean }) {
  return <span class={`tick${p.on ? " on" : ""}`}>{p.on && <Icon name="check" size={14} />}</span>;
}

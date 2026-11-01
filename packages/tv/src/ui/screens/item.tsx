import { useSignal, type Signal } from "@preact/signals";
import type { ComponentChildren } from "preact";
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "preact/hooks";
import { errorMessage, isApiError, unreachable } from "../../api/client";
import type { Credit, CreditRole, ItemDetail, ItemSummary, MediaFile, MediaRequest, MediaStream } from "../../api/types";
import {
  codecLabel,
  episodeLabel,
  formatDuration,
  formatRuntime,
  formatTimeLeft,
  isInProgress,
  pluralize,
  qualityBadges,
  resolutionLabel,
  seasonTitle,
  streamLabel,
  streamRange,
  formatBytes,
  startingAudio,
  audioPicked,
  automaticAudioLabel,
  audioFormatLabel,
} from "../../format/format";
import { mediaFacts } from "../../format/media-info";
import { mayAsk } from "../../requests";
import { audioStreams, subtitleStreams, withTracks } from "../../player/plan";
import { useUi } from "../context";
import { artCandidates } from "../kit/art";
import { Backdrop, useRested } from "../kit/backdrop";
import { PanelRow, Round } from "../kit/buttons";
import { F, FocusWatch, G } from "../kit/focusable";
import { Icon } from "../kit/icons";
import { PanelHeading, PanelTitle, SidePanel, TextDialog } from "../kit/panel";
import { Row, rowHeight, SectionPage, type Section } from "../kit/row";
import { ArtTile, Tile, TILES } from "../kit/tile";
import type { Entry } from "../shell/navigator";
import { ActionsPanel } from "./actions";
import { failedTitle, Message, PageBadge, problem, sentence, useApp, useLoad } from "./common";
import { openPlayer, pageOf, prefetchPlayback, setInMyList, setWatched } from "./item-actions";
import { ItemRequestLine, useItemRequest } from "./request-kit";

/** Where the focused part of the page sits. */
const TABS_LINE = 218;
const EPISODES_LINE = 272;
const ROWS_LINE = 172;
/**
 * The hero's section: 52 above what it holds, the focused button's label
 * (23 under the row) and 8 below. It follows what the hero holds, as the
 * Android app's list does; until measured, the spec's movie (ends at 371).
 */
const HERO_PAD = 52 + 23 + 8;
const HERO_H = 371;

type PanelKind = null | "tracks" | "more" | { actions: ItemSummary };

/** "Trailer · 2m", "Behind the Scenes · 5m", "Featurette". */
export function extraCaption(item: ItemSummary): string {
  const names: Record<string, string> = {
    trailer: "Trailer",
    behind_the_scenes: "Behind the Scenes",
    behindthescenes: "Behind the Scenes",
    deleted: "Deleted Scene",
    deleted_scene: "Deleted Scene",
    featurette: "Featurette",
    interview: "Interview",
    scene: "Scene",
    short: "Short",
  };
  const kind = names[(item.extra_type ?? "").toLowerCase()] ?? "Extra";
  const dur = item.duration_ms ? formatDuration(item.duration_ms).replace(/ \d+s$/, "") : "";
  return dur && (kind === "Trailer" || kind === "Behind the Scenes") ? `${kind} · ${dur}` : kind;
}

/** A version's long form: "Final Cut · 4K · HEVC · Dolby Vision P8.1 · TrueHD Atmos 7.1 · 62.0 GB". */
export function versionLong(file: MediaFile): string {
  const video = file.streams.filter((s) => s.kind === "video")[0];
  const audio = startingAudio(file);
  return [
    file.edition ?? "",
    resolutionLabel(file.width, file.height) ?? "",
    codecLabel(file.video_codec),
    video ? streamRange(video) ?? "" : "",
    audio ? audioFormatLabel(audio) : "",
    file.size ? formatBytes(file.size) : "",
  ]
    .filter(Boolean)
    .join(" · ");
}

/** A version's short form for the Tracks panel: "Final Cut · 4K · HEVC · MKV · 58.2 GB", else "Version N". */
export function versionShort(file: MediaFile, n: number): string {
  const parts = [file.edition ?? "", resolutionLabel(file.width, file.height) ?? "", codecLabel(file.video_codec), file.container ? file.container.toUpperCase() : "", file.size ? formatBytes(file.size) : ""].filter(Boolean);
  return parts.length ? parts.join(" · ") : `Version ${n}`;
}

function isWatched(item: ItemSummary): boolean {
  if (item.kind === "show" || item.kind === "season") return (item.leaf_count ?? 0) > 0 && (item.viewed_leaf_count ?? 0) >= (item.leaf_count ?? 0);
  return item.user_state.watched;
}

/** The crew the page names, in this order, with what each is called on a tile. */
const CREW_ROLES: { role: CreditRole; label: string }[] = [
  { role: "creator", label: "Creator" },
  { role: "director", label: "Director" },
  { role: "writer", label: "Writer" },
];
/** At most this many people per crew role, then "and N more" (Credits.tsx). */
const CREW_NAMES = 4;

/** An item's people in `role`, each once. */
function crewIn(item: ItemDetail, role: CreditRole): Credit[] {
  const seen = new Set<number>();
  return (item.crew ?? []).filter((c) => c.role === role && !seen.has(c.id) && !!seen.add(c.id));
}

/**
 * The people row (the Android app's castAndCrew): the cast with their
 * characters, then the first four creators, directors and writers under
 * their role; nobody twice. Its title says which: Cast, Crew, Cast & Crew,
 * and an episode's cast are its Guest Stars.
 */
export function castAndCrew(item: ItemDetail): { title: string; people: { credit: Credit; subtitle: string }[] } | null {
  const cast = (item.cast ?? []).filter((c) => c.role === "actor");
  const crew = CREW_ROLES.map((r) => ({ label: r.label, people: crewIn(item, r.role).slice(0, CREW_NAMES) })).filter((l) => l.people.length);
  if (!cast.length && !crew.length) return null;
  const seen = new Set<number>();
  const people: { credit: Credit; subtitle: string }[] = [];
  for (const c of cast) if (!seen.has(c.id) && seen.add(c.id)) people.push({ credit: c, subtitle: c.character ?? "" });
  for (const line of crew) for (const c of line.people) if (!seen.has(c.id) && seen.add(c.id)) people.push({ credit: c, subtitle: line.label });
  const castTitle = item.kind === "episode" ? "Guest Stars" : "Cast";
  const title = !crew.length ? castTitle : !cast.length ? "Crew" : `${castTitle} & Crew`;
  return { title, people };
}

/** Credits line: "Directed by A, B · Starring C, D, E". */
function creditsLine(item: ItemDetail): ComponentChildren {
  const crew = item.crew ?? [];
  const directors = crew.filter((c) => c.role === "director").slice(0, 2);
  const creators = crew.filter((c) => c.role === "creator").slice(0, 2);
  const lead = directors.length ? { label: "Directed by", people: directors } : creators.length ? { label: "Created by", people: creators } : null;
  const actors = (item.cast ?? []).filter((c) => c.role === "actor").slice(0, 3);
  const parts: ComponentChildren[] = [];
  if (lead) {
    parts.push(
      <span key="lead">
        <span class="c-muted">{lead.label} </span>
        <span class="c-fg">{lead.people.map((p) => p.name).join(", ")}</span>
      </span>,
    );
  }
  if (actors.length) {
    if (parts.length) parts.push(<span key="sep" class="c-subtle"> · </span>);
    parts.push(
      <span key="cast">
        <span class="c-muted">Starring </span>
        <span class="c-fg">{actors.map((p) => p.name).join(", ")}</span>
      </span>,
    );
  }
  return parts.length ? parts : null;
}

/**
 * An item's page (TvItemScreen): a movie, a show, a season or an
 * episode, with its hero and buttons, its seasons and episodes, extras,
 * cast, more like it, and the details of its files.
 */
export function ItemScreen(p: { entry: Entry; id: number }) {
  const app = useApp();
  const ui = useUi();
  const loaded = useLoad(
    (c) =>
      ui.api.item(p.id, c).then((d) => {
        app.keepItem(d);
        return d;
      }),
    [p.id],
    { initial: app.cachedItem(p.id) },
  );
  const item = loaded.data;
  // Items with pages of their own replace this one.
  useEffect(() => {
    if (!item) return;
    if (item.kind === "artist" || item.kind === "album" || item.kind === "track") ui.nav.replace(pageOf(item));
    else if (item.kind === "extra" && item.parent_id) ui.nav.replace({ name: "item", id: item.parent_id });
  }, [item?.kind]);
  // A page that shows what was saved says nothing when the server doesn't answer: the shell's notice does.
  useEffect(() => {
    if (item && loaded.error && !unreachable(loaded.error)) ui.toast(problem(loaded.error), true);
  }, [loaded.error]);
  if (!item) {
    if (loaded.error) {
      if (isApiError(loaded.error, 404)) {
        return <Message title="Item not found" body="It may have been removed, or this profile can't see it." action={{ label: "Back", onSelect: () => ui.nav.pop() }} />;
      }
      return <Message title={failedTitle(loaded.error, "Couldn't load this item")} body={problem(loaded.error)} action={{ label: "Retry", icon: "refresh-cw", onSelect: loaded.reload }} />;
    }
    return <ItemSkeleton />;
  }
  if (item.kind !== "movie" && item.kind !== "show" && item.kind !== "season" && item.kind !== "episode") return null;
  return <ItemPage entry={p.entry} item={item} reload={loaded.reload} />;
}

function ItemSkeleton() {
  const b = (y: number, w: number, h: number, r = 2) => <div class="skeleton" style={`position:absolute;left:96px;top:${y}px;width:${w}px;height:${h}px;border-radius:${r}px`} />;
  return (
    <div class="item-page" role="status" aria-label="Loading">
      {b(82, 320, 40)}
      {b(136, 220, 18)}
      {b(178, 464, 15)}
      {b(200, 440, 15)}
      {b(222, 300, 15)}
      {b(275, 140, 40, 6)}
      {[0, 1, 2].map((i) => (
        <div key={i} class="skeleton" style={`position:absolute;left:${246 + i * 50}px;top:275px;width:40px;height:40px;border-radius:6px`} />
      ))}
    </div>
  );
}

function ItemPage(p: { entry: Entry; item: ItemDetail; reload: () => void }) {
  const app = useApp();
  const ui = useUi();
  const item = p.item;
  const entry = p.entry;
  const kind = item.kind;
  const tv = kind === "show" || kind === "season" || kind === "episode";

  // ---- what the page reads besides the item
  const showId = kind === "show" ? item.id : kind === "season" ? item.parent_id ?? null : kind === "episode" ? item.grandparent_id ?? null : null;
  const onDeck = useLoad((c) => (kind === "show" || kind === "season" ? ui.api.onDeck(item.id, c).then((r) => r.item) : Promise.resolve(null)), [item.id]);
  const seasons = useLoad(
    (c) => {
      if (kind === "show") return ui.api.children(item.id, c).then((r) => r.items.filter((i) => i.kind === "season"));
      if (kind === "season" && item.parent_id) return ui.api.children(item.parent_id, c).then((r) => r.items.filter((i) => i.kind === "season"));
      return Promise.resolve([] as ItemSummary[]);
    },
    [item.id],
  );
  const related = useLoad((c) => (kind === "movie" || kind === "show" ? ui.api.related(item.id, c).then((r) => r.items.filter((i) => i.id !== item.id)) : Promise.resolve([])), [item.id]);
  const partOf = useLoad((c) => (kind === "movie" || kind === "show" ? ui.api.itemCollections(item.id, c).then((r) => r.collections) : Promise.resolve([])), [item.id], {
    live: false,
  });

  // The season shown: the on-deck episode's, else the first numbered one, else the first.
  const [chosenSeason, setChosenSeason] = useState<number | null>((entry.state.season as number | undefined) ?? null);
  let seasonId: number | null = null;
  if (kind === "season") seasonId = chosenSeason ?? item.id;
  else if (kind === "episode") seasonId = item.parent_id ?? null;
  else if (kind === "show" && seasons.data && !onDeck.loading) {
    const list = seasons.data;
    seasonId = chosenSeason ?? onDeck.data?.parent_id ?? list.filter((s) => (s.index ?? 0) > 0)[0]?.id ?? list[0]?.id ?? null;
  }
  const episodes = useLoad((c) => (seasonId !== null ? ui.api.children(seasonId, c).then((r) => r.items.filter((i) => i.kind === "episode")) : Promise.resolve(null)), [
    seasonId,
  ]);
  const toWatch = kind === "episode" ? item.id : onDeck.data?.id ?? null;

  // ---- versions and tracks chosen on this page
  const [fileId, setFileId] = useState<number | null>(null);
  const file = item.files.filter((f) => f.id === fileId)[0] ?? item.files[0] ?? null;
  const [panel, setPanel] = useState<PanelKind>(null);
  const [summaryOpen, setSummaryOpen] = useState(false);
  const [section, setSection] = useState<string>((entry.state.section as string | undefined) ?? "hero");
  // Each row's tile focused last: a row drawn again (the page draws only what is near) comes back to it.
  const rowIndex = (entry.state.rowIndex as Record<string, number> | undefined) ?? {};
  entry.state.rowIndex = rowIndex;
  // The episode focus is on: only the info block reads it, so a move along the episodes draws nothing else again.
  const focusedEpisode = useSignal<ItemSummary | null>(null);
  const [readOn, setReadOn] = useState(0);
  const [summaryCut, setSummaryCut] = useState(false);
  const heroEl = useRef<HTMLDivElement | null>(null);
  const [heroH, setHeroH] = useState(HERO_H);
  const target: ItemSummary | null = kind === "show" || kind === "season" ? onDeck.data ?? null : item;
  // A movie's or show's request, once the page has drawn: its stage, and Request more seasons.
  const titleRequest = useItemRequest(item.id, kind);
  const requested = titleRequest?.request ?? null;
  const askMore = titleRequest && titleRequest.askable && titleRequest.askable.length > 0 && mayAsk(app.requests.value) ? titleRequest : null;
  // The hero's height, measured when what it says changes: reading it lays the page out.
  useLayoutEffect(() => {
    const el = heroEl.current;
    if (!el) return;
    const h = HERO_PAD + el.offsetHeight;
    if (h !== heroH) setHeroH(h);
  }, [item, target, fileId, partOf.data, !!requested, !!askMore]);

  // ---- the primary button
  const playable = kind === "movie" || kind === "episode" ? item.files.length > 0 : !!target;
  const resume = target ? isInProgress(target) : false;
  const epLabel = (kind === "show" || kind === "season") && target ? episodeLabel(target.parent_index, target.index) : "";
  const primary = !playable && !(onDeck.loading && (kind === "show" || kind === "season")) ? "Unavailable" : `${resume ? "Resume" : "Play"}${epLabel ? ` ${epLabel}` : ""}`;
  const play = (fromStart: boolean) => {
    if (!target || !playable) {
      ui.toast("Nothing to play: this item has no media files.");
      return;
    }
    const files = target.id === item.id ? item.files : [];
    openPlayer(ui, {
      itemId: target.id,
      fileId: files.length > 1 && file ? file.id : null,
      fromStart,
    });
  };

  const watched = isWatched(item);
  const listed = item.in_watchlist ?? false;
  const [inList, setInList] = useState(listed);
  useEffect(() => setInList(listed), [listed]);
  const hasTracks = !!file && (item.files.length > 1 || audioStreams(file).length >= 2 || subtitleStreams(file).length > 0);
  const trailer = (item.extras ?? []).find((x) => (x.extra_type ?? "").toLowerCase() === "trailer") ?? null;

  // ---- sections
  const sections: Section[] = [];
  // The pointer moving back up into the hero's place, hidden while it was on the rows below, brings it back.
  const pointedAtHero = () => {
    const buttons = document.querySelector<HTMLElement>(".item-buttons");
    const el = (buttons && ui.focus.rememberedIn(buttons)) ?? buttons?.querySelector<HTMLElement>("[data-f]:not([data-fdisabled])") ?? null;
    if (el && ui.input.pointerMoved && ui.focus.inScope(el)) ui.focus.focus(el, true);
  };
  sections.push({
    key: "hero",
    height: heroH,
    anchor: null,
    render: (onFocus) => [
      section !== "hero" && <div key="zone" class="hero-zone" onMouseMove={pointedAtHero} />,
      <ItemHero
        key="hero"
        heroRef={heroEl}
        item={item}
        target={target}
        file={file}
        partOf={partOf.data ?? []}
        primary={primary}
        resume={resume}
        playable={playable}
        watched={watched}
        inList={inList}
        hasTracks={hasTracks}
        hidden={section !== "hero"}
        onFocus={() => {
          onFocus();
          setSection("hero");
          entry.state.section = "hero";
          setReadOn(0);
        }}
        onPlay={play}
        // Holding OK on Play: the actions of what the page is (a show as a whole), as on Android TV.
        onPlayActions={() => setPanel({ actions: item })}
        onWatched={() => void setWatched(app, ui, item, !watched).then((ok) => ok && p.reload())}
        onTracks={() => setPanel("tracks")}
        onMyList={() => {
          const next = !inList;
          setInList(next);
          void setInMyList(app, ui, item, next).then((ok) => {
            if (!ok) setInList(!next);
          });
        }}
        onTrailer={trailer ? () => openPlayer(ui, { itemId: trailer.id, fromStart: true }) : null}
        onPlaylist={() => ui.nav.open({ name: "addToPlaylist", itemIds: [item.id], title: item.title })}
        onMore={() => setPanel("more")}
        onSummaryCut={setSummaryCut}
        request={requested}
        onRequestMore={askMore ? () => ui.nav.open({ name: "title", kind: askMore.kind, tmdbId: askMore.tmdb_id, pick: true }) : null}
      />,
    ],
  });
  const seasonList = seasons.data ?? [];
  const multiSeason = kind !== "episode" && seasonList.length > 1;
  const episodeKey = (e: ItemSummary) => `episode/${e.id}`;
  if (tv) {
    if (multiSeason) {
      sections.push({
        key: "seasons",
        height: 40,
        anchor: TABS_LINE,
        render: (onFocus) => (
          <SeasonTabs
            seasons={seasonList}
            shown={seasonId}
            onFocus={() => {
              onFocus();
              setSection("seasons");
              entry.state.section = "seasons";
            }}
            onShow={(id) => {
              entry.state.season = id;
              setChosenSeason(id);
            }}
          />
        ),
      });
    }
    const seasonName =
      kind === "episode" ? seasonTitle(null, item.parent_index) : seasonList.filter((s) => s.id === seasonId)[0] ? seasonTitle(seasonList.filter((s) => s.id === seasonId)[0]!.title, seasonList.filter((s) => s.id === seasonId)[0]!.index) : "Episodes";
    const eps = episodes.data;
    const epIndex = eps ? Math.max(0, eps.findIndex((e) => e.id === toWatch)) : 0;
    sections.push({
      key: "episodes",
      height: (multiSeason ? 0 : 38) + TILES.episode.h + 14 + 34,
      anchor: multiSeason ? EPISODES_LINE : EPISODES_LINE - 38,
      render: (onFocus) =>
        eps === null || eps === undefined ? (
          <div style={`height:${152 + (multiSeason ? 0 : 38)}px`} />
        ) : !eps.length ? (
          <div class="t-body c-muted" style="padding-left:96px;height:104px">
            No episodes found.
          </div>
        ) : (
          <Row
            key={`season-${seasonId}`}
            title={multiSeason ? undefined : seasonName}
            kind="episode"
            items={eps}
            keyOf={(e) => e.id}
            enterAt={rowIndex[`season-${seasonId}`] ?? epIndex}
            onLeftEdge={ui.toRail}
            onFocusIndex={(i) => {
              rowIndex[`season-${seasonId}`] = i;
              onFocus();
              setSection("episodes");
              entry.state.section = "episodes";
              focusedEpisode.value = eps[i] ?? null;
            }}
            tile={(e, _i, place) => (
              <Tile
                item={e}
                kind="episode"
                fid={episodeKey(e)}
                priority={place.priority}
                onFocus={place.onFocus}
                onSelect={() => ui.nav.open({ name: "item", id: e.id })}
                onActions={() => setPanel({ actions: e })}
              />
            )}
          />
        ),
    });
  }
  const extras = item.extras ?? [];
  if (extras.length) {
    sections.push(rowSection("extras", "Extras", "landscape", extras, (x, place) => (
      <Tile
        item={x}
        kind="landscape"
        subtitle={extraCaption(x)}
        fid={`extras/${x.id}`}
        priority={place.priority}
        onFocus={place.onFocus}
        onSelect={() => openPlayer(ui, { itemId: x.id, fromStart: true })}
      />
    )));
  }
  const people = castAndCrew(item);
  if (people) {
    sections.push(
      rowSection("cast", people.title, "person", people.people, ({ credit: c, subtitle }, place) => (
        <ArtTile
          kind="person"
          title={c.name}
          subtitle={subtitle}
          candidates={c.photo_url ? [c.photo_url] : []}
          fid={`cast/${c.id}`}
          priority={place.priority}
          onFocus={place.onFocus}
          onSelect={() => ui.nav.open({ name: "person", id: c.id })}
        />
      )),
    );
  }
  const more = related.data ?? [];
  if (more.length) {
    sections.push(
      rowSection("related", "More Like This", "poster", more, (r: ItemSummary, place) => (
        <Tile
          item={r}
          kind="poster"
          fid={`related/${r.id}`}
          priority={place.priority}
          onFocus={place.onFocus}
          onSelect={() => ui.nav.open(pageOf(r))}
          onActions={() => setPanel({ actions: r })}
        />
      )),
    );
  }
  const details = <Details item={item} file={file} onVersion={(f) => setFileId(f.id)} />;
  sections.push({
    key: "details",
    height: 600,
    anchor: ROWS_LINE,
    render: (onFocus) => (
      <FocusWatch
        class="item-details"
        onFocus={() => {
          onFocus();
          setSection("details");
          entry.state.section = "details";
        }}
      >
        {details}
      </FocusWatch>
    ),
  });

  function rowSection<T>(key: string, title: string, k: "landscape" | "person" | "poster", items: T[], tile: (t: T, place: { onFocus: (el: HTMLElement) => void; priority: number }) => ComponentChildren): Section {
    return {
      key,
      height: rowHeight(k),
      anchor: ROWS_LINE,
      render: (onFocus) => (
        <Row
          title={title}
          kind={k}
          items={items}
          keyOf={(x, i) => ((x as { id?: number }).id ?? i)}
          enterAt={rowIndex[key]}
          onLeftEdge={ui.toRail}
          onFocusIndex={(i) => {
            rowIndex[key] = i;
            onFocus();
            setSection(key);
            entry.state.section = key;
          }}
          tile={(x, _i, place) => tile(x, place)}
        />
      ),
    };
  }

  // Down past the details' last row reads on; Up takes those steps back first.
  useEffect(
    () =>
      ui.input.push((key, e) => {
        if (e.type !== "down" || section !== "details") return false;
        const box = document.querySelector<HTMLElement>(".item-details");
        if (!box) return false;
        const focusables = box.querySelectorAll("[data-f]");
        const last = focusables[focusables.length - 1];
        const onLast = !focusables.length || ui.focus.current === last;
        const room = Math.max(0, box.offsetHeight - (540 - 27 - ROWS_LINE));
        if (key === "down" && onLast && readOn < room) {
          setReadOn(Math.min(room, readOn + 160));
          return true;
        }
        if (key === "up" && readOn > 0) {
          setReadOn(Math.max(0, readOn - 160));
          return true;
        }
        return false;
      }),
    [section, readOn],
  );

  const showInfo = section !== "hero";
  const clip = section === "hero" ? undefined : section === "seasons" || section === "episodes" ? TABS_LINE - 12 : readOn > 0 ? 27 : ROWS_LINE - 12;
  const heroTitle = kind === "season" || kind === "episode" ? item.grandparent_title || item.parent_title || item.title : item.title;
  return (
    <div class="item-page">
      <Backdrop candidates={artCandidates(item, "backdrop")} title={item.title} scrim="item" />
      {showInfo && readOn === 0 && (
        <ItemInfo item={item} file={file} title={heroTitle} episodes={section === "seasons" || section === "episodes"} focused={focusedEpisode} />
      )}
      <SectionPage sections={sections} clipAbove={clip} extraShift={readOn} focusedKey={section} />
      <div class="rail-fade" />
      {panel === "tracks" && file && <TracksPanel item={item} file={file} files={item.files} onVersion={(f) => setFileId(f.id)} onClose={() => setPanel(null)} onSaved={p.reload} />}
      {panel === "more" && (
        <MorePanel
          item={item}
          summaryCut={summaryCut}
          partOf={partOf.data ?? []}
          showId={showId}
          onSummary={() => {
            setPanel(null);
            setSummaryOpen(true);
          }}
          onClose={() => setPanel(null)}
        />
      )}
      {panel && typeof panel === "object" && <ActionsPanel item={panel.actions} onClose={() => setPanel(null)} />}
      {summaryOpen && <TextDialog title={item.title} text={[item.tagline ?? "", item.summary ?? ""].filter(Boolean).join("\n\n")} onClose={() => setSummaryOpen(false)} />}
    </div>
  );
}

/**
 * The block over the rows once focus leaves the hero: the title and meta
 * line, then the episode where focus rests along the episodes, else the
 * item's summary.
 */
function ItemInfo(p: { item: ItemDetail; file: MediaFile | null; title: string; episodes: boolean; focused: Signal<ItemSummary | null> }) {
  const rested = useRested(p.focused.value);
  const episode = p.episodes ? rested : null;
  const item = p.item;
  return (
    <div class="item-info in">
      <div class="t-hero c-fg one-line">{p.title}</div>
      <div style="margin-top:10px">
        <ItemMeta item={item} file={p.file} />
      </div>
      {episode ? (
        <>
          <div class="t-title c-fg one-line" style="margin-top:14px">
            {episode.title}
          </div>
          <div class="t-caption c-muted one-line" style="margin-top:4px">
            {[`Season ${episode.parent_index ?? "?"} · Episode ${episode.index ?? "?"}`, formatRuntime(episode.duration_ms), episode.release_date ? `Aired ${longDate(episode.release_date)}` : ""]
              .filter(Boolean)
              .join(" · ")}
          </div>
          {episode.summary && (
            <div class="t-body lines-2 info-summary" style="margin-top:6px">
              {episode.summary}
            </div>
          )}
        </>
      ) : (
        item.summary && (
          <div class="t-body lines-2 info-summary" style="margin-top:8px">
            {item.summary}
          </div>
        )
      )}
    </div>
  );
}

/** "12 March 2024". */
function longDate(iso: string): string {
  const d = new Date(`${iso.slice(0, 10)}T12:00:00`);
  if (isNaN(d.getTime())) return iso;
  const months = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
  return `${d.getDate()} ${months[d.getMonth()]} ${d.getFullYear()}`;
}

/** "Mar 5, 2024". */
export function mediumDate(iso: string): string {
  const d = new Date(`${iso.slice(0, 10)}T12:00:00`);
  if (isNaN(d.getTime())) return iso;
  const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  return `${months[d.getMonth()]} ${d.getDate()}, ${d.getFullYear()}`;
}

/** The meta line of each kind (ItemPages): year, edition, runtime; seasons; air date. */
function ItemMeta(p: { item: ItemDetail; file: MediaFile | null }) {
  const item = p.item;
  let lead: string | null = null;
  if (item.kind === "movie") {
    lead = [item.year ? String(item.year) : "", p.file?.edition ?? "", formatRuntime(item.duration_ms)].filter(Boolean).join(" · ");
  } else if (item.kind === "show") {
    const seasons = item.child_count ?? 0;
    lead = [seasons ? pluralize(seasons, "season") : "", item.year ? String(item.year) : ""].filter(Boolean).join(" · ");
  } else if (item.kind === "season") {
    lead = [item.year ? String(item.year) : "", item.leaf_count ? pluralize(item.leaf_count, "episode") : ""].filter(Boolean).join(" · ");
  } else {
    lead = [item.release_date ? mediumDate(item.release_date) : "", p.file?.edition ?? "", formatRuntime(item.duration_ms)].filter(Boolean).join(" · ");
  }
  return <MetaLineOf detail={item} file={p.file} text={lead} />;
}

/** The meta line: its words, the content rating, the critics' rating with a star, the quality badges. */
function MetaLineOf(p: { detail: ItemDetail; file: MediaFile | null; text: string | null }) {
  const badges = qualityBadges(p.file);
  return (
    <div class="meta-line" style="max-width:464px">
      {p.text && <span class="t-label c-muted one-line">{p.text}</span>}
      {p.detail.content_rating && (
        <>
          {p.text && <span class="t-label c-subtle">·</span>}
          <PageBadge text={p.detail.content_rating} />
        </>
      )}
      {p.detail.rating && p.detail.rating > 0 ? (
        <>
          <span class="t-label c-subtle">·</span>
          <span class="meta-rating" aria-label={`Rated ${p.detail.rating.toFixed(1)}`}>
            <Icon name="star" size={16} color="var(--accent)" />
            <span class="t-label c-fg" style="margin-left:4px">
              {p.detail.rating.toFixed(1)}
            </span>
          </span>
        </>
      ) : null}
      {badges.length > 0 && (
        <>
          <span class="t-label c-subtle">·</span>
          <span class="meta-badges">
            {badges.map((b) => (
              <PageBadge key={b} text={b} />
            ))}
          </span>
        </>
      )}
    </div>
  );
}

function ItemHero(p: {
  heroRef: { current: HTMLDivElement | null };
  item: ItemDetail;
  target: ItemSummary | null;
  file: MediaFile | null;
  partOf: { id: number; title: string }[];
  primary: string;
  resume: boolean;
  playable: boolean;
  watched: boolean;
  inList: boolean;
  hasTracks: boolean;
  hidden: boolean;
  onFocus: () => void;
  onPlay: (fromStart: boolean) => void;
  onPlayActions: () => void;
  onWatched: () => void;
  onTracks: () => void;
  onMyList: () => void;
  /** The item's first trailer, from its start; `null` without one. */
  onTrailer: (() => void) | null;
  onPlaylist: () => void;
  onMore: () => void;
  onSummaryCut: (cut: boolean) => void;
  /** The title's request (any of its stage); `null` without one. */
  request: MediaRequest | null;
  /** The title's page with its season picker; `null` when nothing more may be asked for. */
  onRequestMore: (() => void) | null;
}) {
  const app = useApp();
  const item = p.item;
  const kind = item.kind;
  const title = kind === "season" || kind === "episode" ? item.grandparent_title || item.parent_title || item.title : item.title;
  const context = kind === "episode" ? [episodeLabel(item.parent_index, item.index), item.title].filter(Boolean).join(" · ") : kind === "season" ? seasonTitle(item.title, item.index) : "";
  const genres = (item.genres ?? []).concat(p.partOf.map((c) => `Part of ${c.title}`)).join(" · ");
  const summary = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = summary.current;
    if (el) p.onSummaryCut(el.scrollHeight > el.clientHeight + 1);
  }, [item.summary]);
  const listable = kind === "movie" || kind === "show";
  const focus = () => {
    p.onFocus();
    prefetchPlayback(app, p.target);
  };
  return (
    <div ref={p.heroRef} class={`item-hero${p.hidden ? " hidden" : ""}`}>
      <div class="hero-title">
        <div class="t-hero c-fg lines-2">{title}</div>
      </div>
      {context && (
        <div class="t-title c-fg one-line" style="margin-top:8px">
          {context}
        </div>
      )}
      <div style="margin-top:12px">
        <ItemMeta item={item} file={p.file} />
      </div>
      {p.request && <ItemRequestLine request={p.request} />}
      {p.resume && p.target && (
        <div class="t-caption c-fg" style="margin-top:8px">
          {formatTimeLeft(p.target.user_state.position_ms, p.target.duration_ms)}
        </div>
      )}
      {genres && (
        <div class="t-caption c-muted one-line" style="margin-top:6px">
          {genres}
        </div>
      )}
      {item.summary && (
        <div ref={summary} class="t-body-large lines-3 hero-summary">
          {item.summary}
        </div>
      )}
      {(() => {
        const credits = creditsLine(item);
        return credits ? (
          <div class="t-caption one-line" style="margin-top:8px">
            {credits}
          </div>
        ) : null;
      })()}
      <G kind="row" restore class="buttons item-buttons">
        <div style="display:flex">
          <F
            class="pill t-label"
            fid="play"
            initial={p.primary !== "Unavailable"}
            disabled={p.primary === "Unavailable"}
            label={p.primary}
            onFocus={focus}
            onSelect={() => p.onPlay(false)}
            onLongSelect={p.onPlayActions}
          >
            {p.primary !== "Unavailable" && <PlayIcon />}
            <span class="one-line">{p.primary}</span>
          </F>
        </div>
        {p.resume && <Round label="From start" icon="rotate-ccw" fid="fromStart" onFocus={focus} onSelect={() => p.onPlay(true)} />}
        {p.onRequestMore && <Round label="Request more seasons" icon="plus" fid="requestMore" onFocus={focus} onSelect={p.onRequestMore} />}
        <Round
          label={p.watched ? "Mark as unwatched" : "Mark as watched"}
          icon={p.watched ? "eye-off" : "circle-check"}
          fid="watched"
          initial={p.primary === "Unavailable"}
          onFocus={focus}
          onSelect={p.onWatched}
        />
        {p.hasTracks && <Round label="Audio and subtitles" icon="captions" fid="tracks" onFocus={focus} onSelect={p.onTracks} />}
        {listable && (
          <Round label={p.inList ? "In My List" : "Add to My List"} icon={p.inList ? "bookmark-check" : "bookmark-plus"} fid="myList" onFocus={focus} onSelect={p.onMyList} />
        )}
        {p.onTrailer && <Round label="Trailer" icon="film" fid="trailer" onFocus={focus} onSelect={p.onTrailer} />}
        <Round label="Add to playlist" icon="list-plus" fid="playlist" onFocus={focus} onSelect={p.onPlaylist} />
        <Round label="More" icon="ellipsis" fid="more" onFocus={focus} onSelect={p.onMore} />
      </G>
    </div>
  );
}

function PlayIcon() {
  return (
    <svg class="icon" width="20" height="20" viewBox="0 0 24 24" fill="currentColor" stroke="currentColor" stroke-width="2.25" stroke-linejoin="round" aria-hidden="true" style="margin-right:8px">
      <path d="M6 3v18l15-9z" />
    </svg>
  );
}

function SeasonTabs(p: { seasons: ItemSummary[]; shown: number | null; onFocus: () => void; onShow: (id: number) => void }) {
  const [focused, setFocused] = useState<number | null>(null);
  const rested = useRested(focused);
  useEffect(() => {
    if (rested !== null && rested !== p.shown) p.onShow(rested);
  }, [rested]);
  const group = useRef<HTMLElement | null>(null);
  return (
    <G
      kind="row"
      class="season-tabs"
      elRef={(el) => (group.current = el)}
      onEnter={() => group.current?.querySelector<HTMLElement>(`[data-fid="season/${p.shown}"]`) ?? null}
    >
      {p.seasons.map((s) => (
        <F
          key={s.id}
          class={`season-tab t-label${s.id === p.shown ? " shown" : ""}`}
          fid={`season/${s.id}`}
          label={seasonTitle(s.title, s.index)}
          onFocus={() => {
            p.onFocus();
            setFocused(s.id);
          }}
          onSelect={() => p.onShow(s.id)}
        >
          <span class="one-line">{seasonTitle(s.title, s.index)}</span>
        </F>
      ))}
    </G>
  );
}

/** Details (TvDetails): the facts, the versions to pick from, and every file's media info. */
function Details(p: { item: ItemDetail; file: MediaFile | null; onVersion: (f: MediaFile) => void }) {
  const item = p.item;
  const facts: { label: string; value: string }[] = [];
  const names = (list: Credit[]) => {
    const shown = list.slice(0, CREW_NAMES).map((c) => c.name);
    return list.length > CREW_NAMES ? `${shown.join(", ")} and ${list.length - CREW_NAMES} more` : shown.join(", ");
  };
  const creators = crewIn(item, "creator");
  const directors = crewIn(item, "director");
  const writers = crewIn(item, "writer");
  if (creators.length) facts.push({ label: "Created by", value: names(creators) });
  if (directors.length) facts.push({ label: directors.length > 1 ? "Directors" : "Director", value: names(directors) });
  if (writers.length) facts.push({ label: writers.length > 1 ? "Writers" : "Writer", value: names(writers) });
  if (item.original_title && item.original_title !== item.title) facts.push({ label: "Original title", value: item.original_title });
  if (item.release_date && item.kind !== "episode") facts.push({ label: "Released", value: mediumDate(item.release_date) });
  if (item.studio) facts.push({ label: item.kind === "show" || item.kind === "episode" ? "Network" : "Studio", value: item.studio });
  if (item.user_state.play_count > 0) facts.push({ label: "Plays", value: String(item.user_state.play_count) });
  if (!facts.length && !item.files.length) return null;
  return (
    <div class="details" style="padding-left:96px;width:664px">
      <div class="t-section c-fg">Details</div>
      {facts.length > 0 && (
        <div class="facts-card" style="margin-top:14px">
          {facts.map((f) => (
            <div key={f.label} class="fact-row">
              <div class="t-body c-subtle fact-label">{f.label}</div>
              <div class="t-body lines-2 fact-value">{f.value}</div>
            </div>
          ))}
        </div>
      )}
      {item.files.length > 0 && (
        <>
          <div class="t-title c-fg" style="margin-top:20px">
            Versions
          </div>
          <G kind="col" style="margin-top:8px">
            {item.files.map((f) => (
              <PanelRow key={f.id} fid={`version/${f.id}`} label={versionLong(f) || "Version"} checked={p.file?.id === f.id} onSelect={() => p.onVersion(f)} />
            ))}
          </G>
          <MediaInfo item={item} />
        </>
      )}
    </div>
  );
}

/** Media info, closed until OK opens it, as on the web and Android: few want every stream. */
function MediaInfo(p: { item: ItemDetail }) {
  const [open, setOpen] = useState(false);
  const files = p.item.files;
  return (
    <>
      <G kind="col" style="margin-top:20px">
        <PanelRow
          fid="media-info"
          label="Media info"
          sub="Each file's video, audio and subtitle streams"
          icon={open ? "chevron-up" : "chevron-down"}
          onSelect={() => setOpen(!open)}
        />
      </G>
      {open && (
        <div style="margin-top:8px">
          {files.map((f, n) => (
            <MediaInfoCard key={f.id} file={f} title={f.edition ?? (files.length > 1 ? `Version ${n + 1}` : null)} />
          ))}
        </div>
      )}
    </>
  );
}

function MediaInfoCard(p: { file: MediaFile; title: string | null }) {
  const blocks = useMemo(() => mediaFacts(p.file), [p.file]);
  return (
    <div class="facts-card media-card">
      {p.title && <div class="t-label c-fg" style="margin-bottom:10px">{p.title}</div>}
      <div class="media-blocks">
        {blocks.map((b) => (
          <div key={b.title} class="media-block">
            <div class="t-label c-fg one-line" style="margin-bottom:4px">
              {b.title}
            </div>
            {b.facts.map((f) => (
              <div key={f.label} class="media-fact">
                <div class="t-caption c-muted one-line media-label">{f.label}</div>
                <div class="t-caption c-fg lines-3 media-value">{f.value}</div>
              </div>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}

/** Audio and subtitles (TvPanels' Tracks): the version, the audio track, the subtitles; saved for the item. */
function TracksPanel(p: { item: ItemDetail; file: MediaFile; files: MediaFile[]; onVersion: (f: MediaFile) => void; onClose: () => void; onSaved: () => void }) {
  const app = useApp();
  const ui = useUi();
  const audio = audioStreams(p.file);
  const subs = subtitleStreams(p.file);
  // `null` is Automatic: no track picked, and each player plays its best of the language.
  const [chosenAudio, setChosenAudio] = useState<number | null>(audioPicked(p.file) ? p.file.selected_audio : null);
  const [chosenSub, setChosenSub] = useState<number | null>(p.file.selected_subtitle ?? null);
  // Subtitles picked alone leave the audio as it was picked before, or to the player.
  const [audioChosenHere, setAudioChosenHere] = useState(false);
  const save = (a: number | null | undefined, s: number | null) => {
    // Play, pressed at once, starts with these (the player starts from the
    // details kept); the show's other episodes are asked for again.
    app.tracksChanged(withTracks(p.item, p.file.id, a, s));
    ui.api.setStreams(p.item.id, p.file.id, a, s).then(p.onSaved, (e) => ui.toast(sentence(errorMessage(e)), true));
  };
  const firstChecked = p.files.length > 1 ? "version" : audio.length >= 2 ? "audio" : "subs";
  return (
    <SidePanel onClose={p.onClose} label="Audio and subtitles">
      <PanelTitle>Audio and subtitles</PanelTitle>
      {p.files.length > 1 && (
        <>
          <PanelHeading>Version</PanelHeading>
          {p.files.map((f, i) => (
            <PanelRow key={f.id} label={versionShort(f, i + 1)} checked={f.id === p.file.id} initial={firstChecked === "version" && f.id === p.file.id} onSelect={() => p.onVersion(f)} />
          ))}
        </>
      )}
      {audio.length >= 2 && (
        <>
          <PanelHeading>Audio</PanelHeading>
          <PanelRow
            label={automaticAudioLabel(p.file)}
            checked={chosenAudio === null}
            initial={firstChecked === "audio" && chosenAudio === null}
            onSelect={() => {
              setChosenAudio(null);
              setAudioChosenHere(true);
              save(null, chosenSub);
            }}
          />
          {audio.map((s: MediaStream) => (
            <PanelRow
              key={s.index}
              label={streamLabel(s)}
              checked={s.index === chosenAudio}
              initial={firstChecked === "audio" && s.index === chosenAudio}
              onSelect={() => {
                setChosenAudio(s.index);
                setAudioChosenHere(true);
                save(s.index, chosenSub);
              }}
            />
          ))}
        </>
      )}
      {subs.length > 0 && (
        <>
          <PanelHeading>Subtitles</PanelHeading>
          <PanelRow
            label="Off"
            checked={chosenSub === null}
            initial={firstChecked === "subs" && chosenSub === null}
            onSelect={() => {
              setChosenSub(null);
              save(audioChosenHere ? chosenAudio : undefined, null);
            }}
          />
          {subs.map((s) => (
            <PanelRow
              key={s.index}
              label={streamLabel(s)}
              checked={s.index === chosenSub}
              initial={firstChecked === "subs" && s.index === chosenSub}
              onSelect={() => {
                setChosenSub(s.index);
                save(audioChosenHere ? chosenAudio : undefined, s.index);
              }}
            />
          ))}
        </>
      )}
    </SidePanel>
  );
}

/** More (TvPanels): the full summary, the person's rating, the show and season, the collections. */
function MorePanel(p: {
  item: ItemDetail;
  summaryCut: boolean;
  partOf: { id: number; title: string }[];
  showId: number | null;
  onSummary: () => void;
  onClose: () => void;
}) {
  const ui = useUi();
  const item = p.item;
  const rateable = item.kind === "movie" || item.kind === "show" || item.kind === "episode";
  const [rating, setRating] = useState<number | null>(item.user_state.rating ?? null);
  const stars = rating ? Math.ceil(rating / 2) : 0;
  const rate = (n: number) => {
    const next = n ? n * 2 : null;
    const before = rating;
    setRating(next);
    ui.api.rate(item.id, next).catch((e) => {
      setRating(before);
      ui.toast(sentence(errorMessage(e)), true);
    });
  };
  const goShow = item.kind === "episode" || item.kind === "season" ? p.showId : null;
  const goSeason = item.kind === "episode" ? item.parent_id ?? null : null;
  const others = (rateable ? 1 : 0) + (goShow ? 1 : 0) + (goSeason ? 1 : 0) + p.partOf.length;
  const firstAction = !rateable && (goShow || goSeason || p.partOf.length);
  return (
    <SidePanel onClose={p.onClose} label="More">
      <PanelTitle>More</PanelTitle>
      {p.summaryCut && <PanelRow label="Full summary" icon="info" onSelect={p.onSummary} />}
      {rateable && (
        <>
          <PanelHeading>Your rating</PanelHeading>
          <PanelRow label="No rating" checked={stars === 0} initial={stars === 0} onSelect={() => rate(0)} />
          {[1, 2, 3, 4, 5].map((n) => (
            <PanelRow key={n} label={n === 1 ? "1 star" : `${n} stars`} icon="star" checked={stars === n} initial={stars === n} onSelect={() => rate(n)} />
          ))}
        </>
      )}
      {goShow && (
        <PanelRow
          label="Go to show"
          icon="tv"
          initial={!!firstAction}
          onSelect={() => {
            p.onClose();
            ui.nav.open({ name: "item", id: goShow });
          }}
        />
      )}
      {goSeason && (
        <PanelRow
          label="Go to season"
          icon="layers"
          initial={!!firstAction && !goShow}
          onSelect={() => {
            p.onClose();
            ui.nav.open({ name: "item", id: goSeason });
          }}
        />
      )}
      {p.partOf.map((c, i) => (
        <PanelRow
          key={c.id}
          label={`Part of ${c.title}`}
          icon="layers"
          initial={!!firstAction && !goShow && !goSeason && i === 0}
          onSelect={() => {
            p.onClose();
            ui.nav.open({ name: "collection", id: c.id });
          }}
        />
      ))}
      {others === 0 && (
        <PanelRow
          label="Add to playlist"
          icon="list-plus"
          initial
          onSelect={() => {
            p.onClose();
            ui.nav.open({ name: "addToPlaylist", itemIds: [item.id], title: item.title });
          }}
        />
      )}
    </SidePanel>
  );
}


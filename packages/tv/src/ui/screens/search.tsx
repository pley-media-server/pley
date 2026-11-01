import { useEffect, useRef, useState } from "preact/hooks";
import type { DiscoverTitle, ItemSummary, LiveChannel, PersonSummary } from "../../api/types";
import { cardSubtitle, episodeLabel } from "../../format/format";
import { nowAndNext, programProgress } from "../../live/data";
import { Cancel } from "../../net/transport";
import { addTitles, mayAsk, titleBadge, titleKey, titleSubtitle, titleTone } from "../../requests";
import { useUi } from "../context";
import { Chip, TextButton } from "../kit/buttons";
import { TextField } from "../kit/field";
import { F, G } from "../kit/focusable";
import { Row, ROW_GAP, rowHeight, SectionPage, type Section } from "../kit/row";
import { ArtTile, SeeAllTile, Tile, TILES, TileCaption, type TileKind } from "../kit/tile";
import type { Entry } from "../shell/navigator";
import { failedTitle, problem, useApp } from "./common";
import { RowSkeleton } from "./discover";
import { pageOf } from "./item-actions";
import { RowsSkeleton } from "./library";
import { ChannelLogo, ProgressLine, useNow } from "./live-kit";
import { TitleTile, useMarks } from "./request-kit";

const RECENTS = 10;
const ROWS_TOP = 96;
/** The line over On TMDB when the libraries have nothing. */
const NONE_H = 20;

/** The groups results come in, in order, with their headers and tiles. */
const GROUPS: { kind: ItemSummary["kind"] | "person" | "channel"; title: string; tile: TileKind }[] = [
  { kind: "person", title: "People", tile: "person" },
  { kind: "channel", title: "Live TV", tile: "landscape" },
  { kind: "movie", title: "Movies", tile: "poster" },
  { kind: "show", title: "Shows", tile: "poster" },
  { kind: "season", title: "Seasons", tile: "poster" },
  { kind: "episode", title: "Episodes", tile: "landscape" },
  { kind: "artist", title: "Artists", tile: "square" },
  { kind: "album", title: "Albums", tile: "square" },
  { kind: "track", title: "Tracks", tile: "square" },
];

type Results = { items: ItemSummary[]; people: PersonSummary[]; channels: LiveChannel[] };
/** What TMDB has for the words that the libraries lack (`at`: when it came, for the badges marked since); `failed`: no answer. */
type OnTmdb = { query: string; titles: DiscoverTitle[]; at: number; failed: boolean };

/** The recent searches kept on this TV, per server and profile. */
function recentKey(server: string | null, user: number | null): string {
  return `pley.searches.${server ?? ""}.${user ?? 0}`;
}

/** A result's caption under its title. */
function subtitleOf(item: ItemSummary): string {
  if (item.kind === "episode") return [episodeLabel(item.parent_index, item.index), item.title].filter(Boolean).join(" · ");
  if (item.kind === "album") return [item.parent_title ?? "", item.year ? String(item.year) : ""].filter(Boolean).join(" · ");
  if (item.kind === "track") return [item.grandparent_title ?? "", item.parent_title ?? ""].filter(Boolean).join(" · ");
  return cardSubtitle(item);
}

/**
 * Search (TvSearchScreen): the field, typed with the TV's keyboard (and
 * its microphone); results 250 ms after typing pauses, grouped in rows
 * (Live TV channels among them, where it is open to the profile), then On
 * TMDB for whoever may ask: what the libraries lack, asked in parallel so
 * it never holds them up; recent searches while the field is empty.
 */
export function SearchScreen(p: { entry: Entry; query?: string }) {
  const app = useApp();
  const ui = useUi();
  const [text, setText] = useState<string>((p.entry.state.text as string | undefined) ?? p.query ?? "");
  const [results, setResults] = useState<Results | null>((p.entry.state.results as Results | undefined) ?? null);
  const [shownQuery, setShownQuery] = useState<string>((p.entry.state.shownQuery as string | undefined) ?? "");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const key = recentKey(app.server.value, app.me.value?.id ?? null);
  const [recents, setRecents] = useState<string[]>(() => {
    try {
      return JSON.parse(app.store.get(key) ?? "[]") as string[];
    } catch {
      return [];
    }
  });
  const cancel = useRef<Cancel | null>(null);
  const now = useNow(30_000);
  const fieldRef = useRef<HTMLElement | null>(null);

  const remember = (q: string) => {
    const t = q.trim();
    if (!t) return;
    const next = [t].concat(recents.filter((r) => r.toLowerCase() !== t.toLowerCase())).slice(0, RECENTS);
    setRecents(next);
    app.store.set(key, JSON.stringify(next));
  };

  // The query 250 ms after typing pauses; a newer one cancels the one asked.
  const query = text.trim();
  useEffect(() => {
    p.entry.state.text = text;
    if (!query) {
      cancel.current?.cancel();
      setResults(null);
      setShownQuery("");
      setLoading(false);
      setError(null);
      return;
    }
    if (query === shownQuery && results) return;
    const t = setTimeout(() => {
      cancel.current?.cancel();
      const c = new Cancel();
      cancel.current = c;
      setLoading(true);
      ui.api.search(query, c).then(
        (r) => {
          if (c.cancelled) return;
          const res = { items: r.items.filter((i) => i.kind !== "extra"), people: r.people ?? [], channels: r.channels ?? [] };
          setResults(res);
          setShownQuery(query);
          // New results: Back from one of them comes to it, not to where the last ones were.
          p.entry.state.rowIndex = {};
          p.entry.state.section = undefined;
          setLoading(false);
          setError(null);
          p.entry.state.results = res;
          p.entry.state.shownQuery = query;
        },
        (e) => {
          if (c.cancelled) return;
          // A failure takes the place of what an earlier search found, which no longer matches the field.
          setResults(null);
          setShownQuery("");
          p.entry.state.results = undefined;
          p.entry.state.shownQuery = undefined;
          setLoading(false);
          setError(e);
        },
      );
    }, 250);
    return () => clearTimeout(t);
  }, [query]);

  // On TMDB: asked beside the libraries' search, with the same pause, for whoever may ask.
  const s = app.requests.value;
  const asks = mayAsk(s) && s?.sign_in_again !== "requests";
  const marked = useMarks();
  const [tmdb, setTmdb] = useState<OnTmdb | null>((p.entry.state.tmdb as OnTmdb | undefined) ?? null);
  useEffect(() => {
    if (!asks || !query || (tmdb && tmdb.query === query && !tmdb.failed)) return;
    const c = new Cancel();
    const t = setTimeout(() => {
      ui.api.discoverSearch(query, 1, true, c).then(
        (page) => {
          if (c.cancelled) return;
          const found: OnTmdb = { query, titles: addTitles([], page.results), at: Date.now(), failed: false };
          // New titles: the row starts at its first again.
          const rows = p.entry.state.rowIndex as Record<string, number> | undefined;
          if (rows) delete rows.tmdb;
          setTmdb(found);
          p.entry.state.tmdb = found;
        },
        () => {
          // Nothing to say in search: the section stays out.
          if (!c.cancelled) setTmdb({ query, titles: [], at: Date.now(), failed: true });
        },
      );
    }, 250);
    return () => {
      clearTimeout(t);
      c.cancel();
    };
  }, [query, asks]);

  const focusField = () => {
    const el = fieldRef.current?.querySelector<HTMLElement>("[data-f]");
    return !!el && ui.focus.focus(el);
  };
  const open = (item: ItemSummary) => {
    remember(query);
    ui.nav.open(pageOf(item));
  };

  // The rows: one per group with results. Each row's tile focused last, so a row drawn again comes back to it.
  const rowIndex = (p.entry.state.rowIndex as Record<string, number> | undefined) ?? {};
  p.entry.state.rowIndex = rowIndex;
  const sections: Section[] = [];
  if (results && query) {
    for (const g of GROUPS) {
      if (g.kind === "person") {
        if (!results.people.length) continue;
        sections.push({
          key: "people",
          height: rowHeight("person"),
          anchor: ROWS_TOP,
          render: (onFocus) => (
            <Row
              title={g.title}
              kind="person"
              items={results.people}
              keyOf={(x) => x.id}
              enterAt={rowIndex.people}
              onLeftEdge={ui.toRail}
              onFocusIndex={(i) => {
                rowIndex.people = i;
                onFocus();
              }}
              tile={(person, _i, place) => (
                <ArtTile
                  kind="person"
                  title={person.name}
                  candidates={person.photo_url ? [person.photo_url] : []}
                  fid={`people/${person.id}`}
                  priority={place.priority}
                  onFocus={place.onFocus}
                  onSelect={() => {
                    remember(query);
                    ui.nav.open({ name: "person", id: person.id });
                  }}
                />
              )}
            />
          ),
        });
        continue;
      }
      if (g.kind === "channel") {
        // Kept from a search before this TV knew of channels: none.
        const channels = results.channels ?? [];
        if (!channels.length) continue;
        sections.push({
          key: "channels",
          height: rowHeight("landscape"),
          anchor: ROWS_TOP,
          render: (onFocus) => (
            <Row
              title={g.title}
              kind="landscape"
              items={channels}
              keyOf={(x) => x.id}
              enterAt={rowIndex.channels}
              onLeftEdge={ui.toRail}
              onFocusIndex={(i) => {
                rowIndex.channels = i;
                onFocus();
              }}
              tile={(channel, _i, place) => (
                <ChannelTile
                  channel={channel}
                  now={now}
                  onFocus={place.onFocus}
                  onSelect={() => {
                    remember(query);
                    app.keepLiveChannel(channel);
                    ui.nav.open({ name: "livePlayer", channelId: channel.id, category: "all", pressedAt: Date.now() });
                  }}
                />
              )}
            />
          ),
        });
        continue;
      }
      const items = results.items.filter((i) => i.kind === g.kind);
      if (!items.length) continue;
      sections.push({
        key: g.kind,
        height: rowHeight(g.tile),
        anchor: ROWS_TOP,
        render: (onFocus) => (
          <Row
            title={g.title}
            kind={g.tile}
            items={items}
            keyOf={(x) => x.id}
            enterAt={rowIndex[g.kind]}
            onLeftEdge={ui.toRail}
            onFocusIndex={(i) => {
              rowIndex[g.kind] = i;
              onFocus();
            }}
            tile={(item, _i, place) => (
              <Tile
                item={item}
                kind={g.tile}
                subtitle={subtitleOf(item)}
                fid={`${g.kind}/${item.id}`}
                priority={place.priority}
                onFocus={place.onFocus}
                onSelect={() => open(item)}
                onActions={() => ui.nav.open({ name: "actions", id: item.id })}
              />
            )}
          />
        ),
      });
    }
  }

  // On TMDB after the libraries' rows: grey wells while it comes; with nothing in the libraries, the line saying so above it.
  const fromLibraries = sections.length;
  const onTmdb = tmdb && tmdb.query === query ? tmdb : null;
  if (asks && results && query && (!onTmdb || (!onTmdb.failed && onTmdb.titles.length > 0))) {
    if (!fromLibraries) {
      sections.push({ key: "none", height: NONE_H, anchor: null, render: () => <div class="t-body c-muted search-none">Nothing in your libraries matches</div> });
    }
    const titles = onTmdb ? onTmdb.titles.map((t) => marked(t, onTmdb.at)) : [];
    sections.push({
      key: "tmdb",
      height: rowHeight("poster"),
      // Under the line, where it is, so that the line stays in sight.
      anchor: fromLibraries ? ROWS_TOP : ROWS_TOP + NONE_H + ROW_GAP,
      render: (onFocus) =>
        titles.length ? (
          <Row
            title="On TMDB"
            kind="poster"
            items={titles}
            keyOf={(t) => titleKey(t)}
            enterAt={rowIndex.tmdb}
            onLeftEdge={ui.toRail}
            onFocusIndex={(i) => {
              rowIndex.tmdb = i;
              onFocus();
            }}
            tile={(t, _i, place) => (
              <TitleTile
                name={t.title}
                subtitle={titleSubtitle(t, true)}
                poster={t.poster_url}
                badge={titleBadge(t)}
                tone={titleTone(t)}
                fid={`tmdb/${titleKey(t)}`}
                priority={place.priority}
                onFocus={place.onFocus}
                onSelect={() => {
                  remember(query);
                  ui.nav.open({ name: "title", kind: t.kind, tmdbId: t.tmdb_id });
                }}
              />
            )}
            seeAll={(place) => (
              <SeeAllTile
                kind="poster"
                label="See all on Discover"
                fid="tmdb/all"
                onFocus={place.onFocus}
                onSelect={() => {
                  remember(query);
                  ui.nav.open({ name: "discover", query });
                }}
              />
            )}
          />
        ) : (
          <RowSkeleton title="On TMDB" />
        ),
    });
  }

  const empty = !!results && query && !sections.length;
  return (
    <div class="search-page">
      {sections.length > 0 && (
        <div class="search-rows">
          <SectionPage
            sections={sections}
            top={ROWS_TOP}
            clipAbove={ROWS_TOP - 12}
            focusedKey={p.entry.state.section as string | undefined}
            onFocusSection={(k) => (p.entry.state.section = k)}
          />
        </div>
      )}
      {!results && loading && <RowsSkeleton top={ROWS_TOP} />}
      <G
        kind="row"
        class="search-field"
        elRef={(el) => (fieldRef.current = el)}
        onEdge={(dir) => {
          if (dir === "down") {
            const target = document.querySelector<HTMLElement>(".search-rows [data-f], .search-recents [data-f]");
            return !!target && ui.focus.focus(target);
          }
          return dir === "right";
        }}
      >
        <TextField
          value={text}
          placeholder="Search movies, shows, people…"
          icon="search"
          type="search"
          width={520}
          initial
          onInput={setText}
          onSubmit={(v) => remember(v)}
        />
        {loading && <div class="t-caption c-muted search-busy">Searching…</div>}
      </G>
      {!query && recents.length > 0 && (
        <div class="search-recents">
          <div class="t-section c-fg" style="margin-left:96px">
            Recent searches
          </div>
          <G kind="row" class="recent-chips">
            {recents.map((r) => (
              <Chip
                key={r}
                label={r}
                icon="history"
                maxWidth={320}
                onSelect={() => {
                  setText(r);
                  remember(r);
                  focusField();
                }}
              />
            ))}
            <TextButton
              label="Clear"
              onSelect={() => {
                setRecents([]);
                app.store.remove(key);
                focusField();
              }}
            />
          </G>
        </div>
      )}
      {!query && !recents.length && (
        <div class="search-notice">
          <div class="t-title c-fg">Search your libraries</div>
          <div class="t-body c-muted" style="margin-top:8px">
            Find movies, shows, episodes, people, artists, albums, tracks and Live TV channels. Press OK on the field to type, or use your keyboard's microphone.
          </div>
        </div>
      )}
      {query && empty && (
        <div class="search-notice">
          <div class="t-title c-fg">{`No results for “${query}”`}</div>
          <div class="t-body c-muted" style="margin-top:8px">
            Try a different spelling or fewer words.
          </div>
        </div>
      )}
      {query && error && !results ? (
        <div class="search-notice">
          <div class="t-title c-fg">{failedTitle(error, "Couldn't search your libraries")}</div>
          <div class="t-body c-muted" style="margin-top:8px">
            {problem(error)}
          </div>
        </div>
      ) : null}
    </div>
  );
}

/** A Live TV channel found: its logo on the tile, what's on and how far it is; OK plays it. */
function ChannelTile(p: { channel: LiveChannel; now: number; onFocus: (el: HTMLElement) => void; onSelect: () => void }) {
  const spec = TILES.landscape;
  const c = p.channel;
  const { on } = nowAndNext(c, p.now);
  const title = `${c.number ? `${c.number} ` : ""}${c.name}`;
  const subtitle = on ? on.title : c.group;
  return (
    <F class="tile" style={`width:${spec.w}px`} fid={`channel/${c.id}`} label={[title, on ? `now: ${on.title}` : "", "Live TV"].filter(Boolean).join(", ")} onSelect={p.onSelect} onFocus={p.onFocus}>
      <div class="tile-well search-channel" style={`width:${spec.w}px;height:${spec.h}px`}>
        <div class="tile-clip">
          <ChannelLogo channel={c} width={136} height={76} showName />
          <span class="live-badge t-badge at-live search-channel-live" aria-hidden="true">
            <span class="live-dot" />
            LIVE
          </span>
          {on && <ProgressLine fraction={programProgress(on, p.now)} class="search-channel-progress" />}
          <div class="tile-hairline" />
        </div>
        <div class="tile-ring" />
      </div>
      <TileCaption title={title} subtitle={subtitle} />
    </F>
  );
}

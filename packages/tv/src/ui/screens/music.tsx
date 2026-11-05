import { useEffect, useRef, useState } from "preact/hooks";
import { errorMessage, isApiError } from "../../api/client";
import type { ItemDetail, ItemSummary } from "../../api/types";
import { formatClock, formatDuration, pluralize } from "../../format/format";
import { t } from "../../i18n";
import { musicPlayer } from "../../player/music";
import { sleepLabel } from "../../player/sleep";
import { useUi } from "../context";
import { nudge } from "../focus";
import { Art } from "../kit/art";
import { Backdrop } from "../kit/backdrop";
import { Pill, Round, TextButton } from "../kit/buttons";
import { F, G } from "../kit/focusable";
import { Icon } from "../kit/icons";
import { useScrollToFocus } from "../kit/panel";
import { Row } from "../kit/row";
import { Tile } from "../kit/tile";
import type { Entry } from "../shell/navigator";
import { failedTitle, problem, sentence, useLoad } from "./common";
import { TrackLine } from "./playlists";

/** A music page's message (MusicMessage): a title, a body, one button. */
function MusicMessage(p: { title: string; body: string; action: string; onAction: () => void }) {
  return (
    <div style="position:absolute;left:96px;right:48px;top:60px">
      <div class="t-title c-fg">{p.title}</div>
      <div class="t-body c-muted" style="margin-top:10px">
        {p.body}
      </div>
      <G kind="row" style="margin-top:22px">
        <Pill label={p.action} initial onSelect={p.onAction} />
      </G>
    </div>
  );
}

/** The art behind a music page, faint, fading out towards the bottom. */
function FaintArt(p: { path: string | null | undefined; title: string; alpha: number }) {
  if (!p.path) return null;
  return (
    <div class="faint-art" style={`opacity:${p.alpha}`}>
      <Art candidates={[p.path]} width={960} title={p.title} class="tile-clip" />
      <div class="faint-fade" />
    </div>
  );
}

/** Every track of `albums`, in their order, fetched afresh. */
async function tracksOf(ui: ReturnType<typeof useUi>, albums: ItemSummary[]): Promise<ItemSummary[]> {
  const lists = await Promise.all(albums.map((a) => ui.api.children(a.id).then((r) => r.items.filter((i) => i.kind === "track"))));
  return ([] as ItemSummary[]).concat(...lists);
}

/** An artist (TvArtistScreen): the picture and name, Play and Shuffle, the albums. */
export function ArtistScreen(p: { entry: Entry; id: number }) {
  const ui = useUi();
  const detail = useLoad((c) => ui.api.item(p.id, c), [p.id]);
  const children = useLoad((c) => ui.api.children(p.id, c).then((r) => r.items.filter((i) => i.kind === "album")), [p.id]);
  const [busy, setBusy] = useState(false);
  const [problemText, setProblemText] = useState<string | null>(null);
  const now = musicPlayer().now.value;
  const artist = detail.data;
  if (!artist) {
    if (detail.error && isApiError(detail.error, 404)) {
      return <MusicMessage title={t("browse.music.artistNotFound")} body={t("browse.notFoundHint")} action={t("common.actions.back")} onAction={() => ui.nav.pop()} />;
    }
    if (detail.error) {
      return <MusicMessage title={failedTitle(detail.error, t("browse.music.cantLoadArtist"))} body={problem(detail.error)} action={t("common.actions.tryAgain")} onAction={detail.reload} />;
    }
    return (
      <div class="music-page" role="status" aria-label={t("common.states.loading")}>
        <div class="skeleton" style="position:absolute;left:96px;top:40px;width:120px;height:120px;border-radius:50%" />
        <div class="skeleton" style="position:absolute;left:244px;top:72px;width:280px;height:36px" />
        <div class="skeleton" style="position:absolute;left:244px;top:118px;width:200px;height:18px" />
      </div>
    );
  }
  const albums = (children.data ?? []).slice().sort((a, b) => (b.year ?? 0) - (a.year ?? 0) || (a.sort_title || a.title).toLowerCase().localeCompare((b.sort_title || b.title).toLowerCase()));
  const counts = [artist.child_count ?? (children.data ? albums.length : 0) ? pluralize(artist.child_count ?? albums.length, "album") : "", artist.leaf_count ? pluralize(artist.leaf_count, "track") : ""]
    .filter(Boolean)
    .join(" · ");
  const meta = [counts, (artist.genres ?? []).slice(0, 3).join(" · ")].filter(Boolean).join("  ·  ");
  const play = async (shuffle: boolean) => {
    if (busy) return;
    setBusy(true);
    try {
      const fresh = (await ui.api.children(p.id)).items.filter((i) => i.kind === "album");
      const tracks = await tracksOf(ui, fresh);
      if (!tracks.length) setProblemText(t("browse.music.noTracksToPlay"));
      else {
        musicPlayer().play(tracks, shuffle ? Math.floor(Math.random() * tracks.length) : 0, shuffle);
        ui.nav.open({ name: "nowPlaying" });
      }
    } catch (e) {
      setProblemText(sentence(errorMessage(e)));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div class="music-page">
      <Backdrop candidates={artist.backdrop_url ? [artist.backdrop_url] : []} title={artist.title} scrim="item" />
      <div class="artist-header">
        <div class="artist-picture">
          <Art candidates={artist.poster_url ? [artist.poster_url] : []} width={120} title={artist.title} class="tile-clip" />
        </div>
        <div style="margin-left:28px;max-width:520px">
          <div class="t-caption c-muted">{t("browse.music.artist")}</div>
          <div class="t-page c-fg lines-2">{artist.title}</div>
          {meta && (
            <div class="t-label c-muted" style="margin-top:6px">
              {meta}
            </div>
          )}
          {artist.summary && (
            <div class="t-body lines-2 info-summary" style="margin-top:8px">
              {artist.summary}
            </div>
          )}
        </div>
      </div>
      <G kind="row" class="buttons artist-buttons">
        <Pill label={t("common.actions.play")} icon="play" fid="play" initial disabled={busy} onSelect={() => void play(false)} />
        <Round label={t("browse.actions.shuffle")} icon="shuffle" fid="shuffle" disabled={busy} onSelect={() => void play(true)} />
        {now && <Round label={t("browse.music.nowPlaying")} icon="audio-lines" fid="now" onSelect={() => ui.nav.open({ name: "nowPlaying" })} />}
        {problemText && <div class="t-caption c-danger" style="margin:14px 0 0 8px">{problemText}</div>}
      </G>
      <div class="artist-albums">
        {!children.data ? (
          <div class="t-body c-muted" style="padding-left:96px">
            {t("browse.loading")}
          </div>
        ) : !albums.length ? (
          <div class="t-body c-muted" style="padding-left:96px;margin-top:12px">
            {t("browse.music.noAlbums")}
          </div>
        ) : (
          <Row
            title={t("browse.music.albums")}
            kind="square"
            items={albums}
            keyOf={(a) => a.id}
            onLeftEdge={ui.toRail}
            tile={(a, _i, place) => (
              <Tile
                item={a}
                kind="square"
                subtitle={a.year ? String(a.year) : ""}
                fid={`albums/${a.id}`}
                priority={place.priority}
                onFocus={place.onFocus}
                onSelect={() => ui.nav.open({ name: "album", id: a.id })}
                onActions={() => ui.nav.open({ name: "actions", id: a.id })}
              />
            )}
          />
        )}
      </div>
      <div class="rail-fade" />
    </div>
  );
}

type AlbumRow = { kind: "disc"; disc: number } | { kind: "track"; track: ItemSummary; position: number };

/** Disc headers only when the tracks span more than one disc. */
export function albumRows(tracks: ItemSummary[]): AlbumRow[] {
  const discs = new Set(tracks.map((t) => t.parent_index ?? 1));
  const out: AlbumRow[] = [];
  let last: number | null = null;
  tracks.forEach((t, position) => {
    const disc = t.parent_index ?? 1;
    if (discs.size > 1 && disc !== last) out.push({ kind: "disc", disc });
    last = disc;
    out.push({ kind: "track", track: t, position });
  });
  return out;
}

/** An album (TvAlbumScreen): its art and facts at the left, its tracks at the right. */
export function AlbumScreen(p: { entry: Entry; id: number; trackId?: number }) {
  const ui = useUi();
  const detail = useLoad((c) => ui.api.item(p.id, c), [p.id]);
  const children = useLoad((c) => ui.api.children(p.id, c).then((r) => r.items.filter((i) => i.kind === "track")), [p.id]);
  const [problemText, setProblemText] = useState<string | null>(null);
  const player = musicPlayer();
  const now = player.now.value;
  const listRef = useRef<HTMLDivElement>(null);
  const shift = useScrollToFocus(listRef, 500);
  const album: ItemDetail | null = detail.data;
  if (!album) {
    if (detail.error && isApiError(detail.error, 404)) {
      return <MusicMessage title={t("browse.music.albumNotFound")} body={t("browse.notFoundHint")} action={t("common.actions.back")} onAction={() => ui.nav.pop()} />;
    }
    if (detail.error) {
      return <MusicMessage title={failedTitle(detail.error, t("browse.music.cantLoadAlbum"))} body={problem(detail.error)} action={t("common.actions.tryAgain")} onAction={detail.reload} />;
    }
    return (
      <div class="music-page" role="status" aria-label={t("common.states.loading")}>
        <div class="skeleton" style="position:absolute;left:96px;top:40px;width:220px;height:220px;border-radius:4px" />
        {[0, 1, 2, 3, 4, 5, 6, 7].map((i) => (
          <div key={i} class="skeleton" style={`position:absolute;left:356px;top:${80 + i * 44}px;width:480px;height:34px;border-radius:4px`} />
        ))}
      </div>
    );
  }
  const tracks = children.data ?? [];
  const rows = albumRows(tracks);
  const current = now && tracks.some((t) => t.id === now.track.id) ? now.track.id : null;
  const total = tracks.reduce((n, t) => n + (t.duration_ms ?? 0), 0);
  const meta = [album.year ? String(album.year) : "", tracks.length ? `${pluralize(tracks.length, "track")} · ${formatDuration(total)}` : ""].filter(Boolean).join(" · ");
  const rating = Math.round(album.user_state.rating ?? 0);
  const play = (position: number, shuffle = false) => {
    if (!tracks.length) {
      setProblemText(t("browse.music.noTracksToPlay"));
      return;
    }
    player.play(tracks, shuffle ? Math.floor(Math.random() * tracks.length) : position, shuffle);
    ui.nav.open({ name: "nowPlaying" });
  };
  return (
    <div class="music-page">
      <FaintArt path={album.poster_url} title={album.title} alpha={0.14} />
      <G kind="col" restore class="album-left">
        <div class="album-art">
          <Art candidates={album.poster_url ? [album.poster_url] : []} width={220} title={album.title} class="tile-clip" />
        </div>
        <div class="t-title c-fg lines-2" style="margin-top:12px">
          {album.title}
        </div>
        {album.parent_id ? (
          <div style="margin:2px 0 0 -16px">
            <TextButton label={album.parent_title ?? t("browse.music.artist")} color="var(--accent-bright)" variant="link" fid="artist" onSelect={() => ui.nav.open({ name: "artist", id: album.parent_id! })} />
          </div>
        ) : (
          album.parent_title && <div class="t-label c-fg" style="margin-top:2px">{album.parent_title}</div>
        )}
        {meta && (
          <div class="t-caption c-muted" style="margin-top:4px">
            {meta}
          </div>
        )}
        {rating > 0 && (
          <div class="stars">
            {[1, 2, 3, 4, 5].map((i) => (
              <Icon key={i} name="star" size={16} color={rating >= 2 * i || rating === 2 * i - 1 ? "var(--accent)" : "var(--surface4)"} />
            ))}
          </div>
        )}
        <G kind="row" class="buttons" style="margin-top:16px">
          <Pill label={t("common.actions.play")} icon="play" fid="play" initial={!p.trackId} disabled={!children.loading && !tracks.length} onSelect={() => play(0)} />
          <Round label={t("browse.actions.shuffle")} icon="shuffle" fid="shuffle" offArt disabled={!tracks.length} onSelect={() => play(0, true)} />
          {current !== null && <Round label={t("browse.music.nowPlaying")} icon="audio-lines" fid="now" offArt onSelect={() => ui.nav.open({ name: "nowPlaying" })} />}
        </G>
        {problemText && <div class="t-caption c-danger" style="margin-top:8px">{problemText}</div>}
      </G>
      <div class="album-list">
        <div ref={listRef} class="scroll-y" style={`transform:translateY(${-shift}px)`}>
          {!children.data ? (
            [0, 1, 2, 3, 4, 5, 6, 7].map((i) => <div key={i} class="skeleton" style="width:480px;height:36px;border-radius:4px;margin:4px 0" />)
          ) : !tracks.length ? (
            <div class="t-body c-muted">{t("browse.music.noTracks")}</div>
          ) : (
            <G kind="col" restore>
              {rows.map((r) =>
                r.kind === "disc" ? (
                  <div key={`disc-${r.disc}`} class="t-label c-subtle disc-header">{t("browse.music.disc", { number: r.disc })}</div>
                ) : (
                  <TrackLine
                    key={r.track.id}
                    fid={`track-${r.track.id}`}
                    number={r.track.index != null ? String(r.track.index) : ""}
                    title={r.track.title}
                    subtitle={r.track.grandparent_title && r.track.grandparent_title !== album.parent_title ? r.track.grandparent_title : undefined}
                    duration={r.track.duration_ms ? formatClock(r.track.duration_ms) : ""}
                    current={r.track.id === current}
                    playing={r.track.id === current && !!now?.playing}
                    highlighted={r.track.id === p.trackId}
                    initial={r.track.id === p.trackId}
                    onSelect={() => (r.track.id === current ? player.toggle() : play(r.position))}
                  />
                ),
              )}
            </G>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * Now Playing (TvNowPlayingScreen): the art, the track, the scrubber and
 * the controls, the sleep timer, and the queue. The page dims after two
 * minutes without a key.
 */
export function NowPlayingScreen() {
  const ui = useUi();
  const player = musicPlayer();
  const now = player.now.value;
  const sleep = player.sleep.value;
  const [dim, setDim] = useState(false);
  // Any key wakes the page; two minutes without one dim it.
  useEffect(() => {
    let timer = setTimeout(() => setDim(true), 120_000);
    const off = ui.input.push(() => {
      setDim(false);
      clearTimeout(timer);
      timer = setTimeout(() => setDim(true), 120_000);
      return false;
    });
    return () => {
      off();
      clearTimeout(timer);
    };
  }, []);
  // The remote's media keys.
  useEffect(
    () =>
      ui.input.push((key, e) => {
        if (e.type !== "down") return key === "playPause" || key === "play" || key === "pause" || key === "next" || key === "previous" || key === "fastForward" || key === "rewind";
        switch (key) {
          case "playPause":
            player.toggle();
            return true;
          case "play":
            if (!player.now.value?.playing) player.toggle();
            return true;
          case "pause":
            player.pause();
            return true;
          case "next":
            player.next();
            return true;
          case "previous":
            player.previous();
            return true;
          case "fastForward":
            player.seekTo(Math.min(player.durationMs(), player.positionMs() + 10_000));
            return true;
          case "rewind":
            player.seekTo(Math.max(0, player.positionMs() - 10_000));
            return true;
          default:
            return false;
        }
      }),
    [],
  );
  if (!now) return <MusicMessage title={t("browse.music.nothingPlaying")} body={t("browse.music.nothingPlayingHint")} action={t("common.actions.back")} onAction={() => ui.nav.pop()} />;
  const track = now.track;
  const art = track.poster_url ?? (track.parent_id ? `/api/items/${track.parent_id}/images/poster` : null);
  const byline = [track.grandparent_title ?? "", track.parent_title ?? ""].filter(Boolean).join(" · ");
  const status =
    now.error === "offline"
      ? t("browse.music.offline")
      : now.error === "signed_out"
        ? t("browse.music.signIn")
        : now.error === "stopped"
          ? t("browse.music.stopped")
          : now.error
            ? t("browse.music.cantPlay")
            : now.loading
              ? t("browse.loading")
              : sleep.secondsLeft !== null
                ? t("browse.music.pausingIn", { seconds: sleep.secondsLeft })
                : "";
  const sleepText =
    sleep.secondsLeft !== null
      ? t("browse.music.keepListening")
      : sleep.choice.kind === "off"
        ? t("browse.music.sleepTimer")
        : t("browse.music.sleepTimerSet", {
            choice:
              sleep.choice.kind === "end_of_item"
                ? t("browse.music.endOfTrack")
                : sleep.minutesLeft !== null
                  ? t("browse.music.minutesLeft", { minutes: sleep.minutesLeft })
                  : sleepLabel(sleep.choice),
          });
  const list = player.list();
  return (
    <div class="music-page">
      <FaintArt path={art} title={track.title} alpha={0.2} />
      <div class="now-art">
        <Art candidates={art ? [art] : []} width={300} title={track.parent_title ?? track.title} class="tile-clip" />
      </div>
      <G kind="col" class="now-column">
        <div class="t-page c-fg lines-2">{track.title}</div>
        <div class="t-body-large c-muted one-line" style="margin-top:4px">
          {byline}
        </div>
        <div class={`t-caption now-status ${now.error ? "c-danger" : "c-subtle"}`}>{status}</div>
        <NowScrubber playing={now.playing} />
        <G kind="row" class="now-controls">
          <div class="now-round">
            <Round label={now.shuffle ? t("browse.music.stopShuffling") : t("browse.actions.shuffle")} icon="shuffle" onSelect={() => player.setShuffle(!now.shuffle)} />
          </div>
          <div class="now-round">
            <Round label={t("browse.music.previous")} icon="skip-back" onSelect={() => player.previous()} />
          </div>
          <F class="play-big" initial label={now.playing || (now.loading && !now.error) ? t("browse.music.pause") : t("common.actions.play")} onSelect={() => player.toggle()}>
            <Icon name={now.playing || (now.loading && !now.error) ? "pause" : "play"} size={32} />
          </F>
          <div class="now-round">
            <Round label={t("browse.music.next")} icon="skip-forward" disabled={now.index + 1 >= now.count} onSelect={() => player.next()} />
          </div>
          <div class="now-round">
            <Round label={sleepText} icon="moon" onSelect={() => player.stepSleep()} />
          </div>
        </G>
        {list.length > 1 && (
          <>
            <div class="t-label c-subtle" style="margin:0 0 4px 12px">
              {t("browse.music.upNext")}
            </div>
            <NowQueue list={list} index={now.index} playing={now.playing} onJump={(i) => player.jumpTo(i)} />
          </>
        )}
      </G>
      {dim && <div class="now-dim" />}
    </div>
  );
}

/** A queue line's pitch (the line, 49, and 2 between lines), and the queue's height. */
const QUEUE_ROW = 51;
const QUEUE_H = 215;

/**
 * The queue (TvNowPlayingScreen's "Up next"): lines at a fixed pitch, only
 * those near what shows drawn, so a queue of hundreds opens and moves as
 * one of ten does. The focused line sits second in the box; while focus is
 * elsewhere, the line focused last, else the one playing. A line the
 * pointer focuses leaves the queue where it is, but for one cut off.
 */
function NowQueue(p: { list: ItemSummary[]; index: number; playing: boolean; onJump: (i: number) => void }) {
  const ui = useUi();
  const [focused, setFocused] = useState<number | null>(null);
  const [held, setHeld] = useState<number | null>(null);
  const group = useRef<HTMLElement | null>(null);
  const n = p.list.length;
  const anchor = Math.min(n - 1, focused ?? p.index);
  const max = Math.max(0, n * QUEUE_ROW - QUEUE_H);
  // Whole lines from the box's top, one above the focused line: no half line under "Up next".
  const shift = held ?? Math.max(0, Math.min(max, (anchor - 1) * QUEUE_ROW));
  const shown = useRef(shift);
  shown.current = shift;
  const first = Math.max(0, Math.floor(shift / QUEUE_ROW) - 3);
  const last = Math.min(n, first + Math.ceil(QUEUE_H / QUEUE_ROW) + 7);
  const lines = [];
  for (let i = first; i < last; i++) {
    const q = p.list[i]!;
    lines.push(
      <div key={`${q.id}-${i}`} class="queue-slot" style={`top:${i * QUEUE_ROW}px`}>
        <TrackLine
          number={String(i + 1)}
          title={q.title}
          subtitle={[q.grandparent_title ?? "", q.parent_title ?? ""].filter(Boolean).join(" · ")}
          duration={q.duration_ms ? formatClock(q.duration_ms) : ""}
          current={i === p.index}
          playing={i === p.index && p.playing}
          onFocus={() => {
            setFocused(i);
            setHeld(ui.focus.byPointer ? nudge(shown.current, i * QUEUE_ROW, i * QUEUE_ROW + QUEUE_ROW - 2, 0, QUEUE_H, 0, max) : null);
          }}
          onSelect={() => p.onJump(i)}
        />
      </div>,
    );
  }
  return (
    <div class="now-queue">
      <div class="queue-scroll" style={`transform:translateY(${-shift}px)`}>
        <G
          kind="col"
          restore
          class="queue-lines"
          style={`height:${n * QUEUE_ROW}px`}
          elRef={(el) => (group.current = el)}
          onEnter={() => group.current?.querySelector<HTMLElement>(".track-line.current") ?? null}
        >
          {lines}
        </G>
      </div>
    </div>
  );
}

/**
 * The scrubber and its times, the one part of Now Playing that follows the
 * clock: drawn again on each whole second of the track while it plays (so
 * no second shows for half a second or one and a half), each second while
 * paused, and at once after a seek; the page and its queue aren't.
 */
function NowScrubber(p: { playing: boolean }) {
  const player = musicPlayer();
  const [, setBeat] = useState(0);
  const pos = player.positionMs();
  useEffect(() => {
    const t = setTimeout(() => setBeat((n) => n + 1), p.playing ? Math.max(200, 1005 - (pos % 1000)) : 1000);
    return () => clearTimeout(t);
  });
  return (
    <Scrubber
      positionMs={pos}
      durationMs={player.durationMs()}
      bufferedMs={player.bufferedMs()}
      onSeek={(ms) => {
        player.seekTo(ms);
        setBeat((n) => n + 1);
      }}
      onToggle={() => player.toggle()}
    />
  );
}

/** The track's scrubber: Left and Right seek 10 s, OK plays or pauses. */
function Scrubber(p: { positionMs: number; durationMs: number; bufferedMs: number; onSeek: (ms: number) => void; onToggle: () => void }) {
  const dur = Math.max(0, p.durationMs);
  const played = dur ? Math.min(1, p.positionMs / dur) : 0;
  const buffered = dur ? Math.min(1, p.bufferedMs / dur) : 0;
  return (
    <div class="now-scrubber">
      <F
        class="scrub-box"
        label={t("browse.music.position")}
        onSelect={p.onToggle}
        onKey={(key) => {
          if (key === "left") {
            p.onSeek(Math.max(0, p.positionMs - 10_000));
            return true;
          }
          if (key === "right") {
            p.onSeek(Math.min(dur, p.positionMs + 10_000));
            return true;
          }
          return false;
        }}
      >
        <div class="scrub-track">
          <div class="scrub-buffered" style={`width:${buffered * 100}%`} />
          <div class="scrub-played" style={`width:${played * 100}%`} />
        </div>
        <div class="scrub-thumb" style={`left:${played * 100}%`} />
      </F>
      <div class="scrub-times t-caption c-fg">
        <span>{formatClock(p.positionMs)}</span>
        <span>{`−${formatClock(Math.max(0, dur - p.positionMs))}`}</span>
      </div>
    </div>
  );
}

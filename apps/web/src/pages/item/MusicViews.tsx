import { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { AudioLines, Cast, Play, Shuffle } from "lucide-react";
import { useChildren, type ChildItem } from "@/api/hooks";
import type { ItemDetail } from "@/api/types";
import { useAudioPlayer } from "@/audio/AudioPlayer";
import { CardSkeleton, MediaCard } from "@/components/cards";
import { PlayOn } from "@/components/PlayOn";
import { StarRating } from "@/components/StarRating";
import { Button, IconButton, SectionTitle, Skeleton } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { cn } from "@/lib/cn";
import { formatClock, formatDuration } from "@/lib/format";
import { useElementWidth } from "@/lib/hooks";
import { usePlayItem } from "@/lib/usePlayItem";
import { MoreActions } from "./ItemActions";
import { Genres, ItemArt, MetaLine, SectionError, Summary } from "./shared";
import { itemUrl } from "@/lib/urls";

function MusicHeader({ item, kindLabel, meta, actions }: { item: ItemDetail; kindLabel: string; meta: React.ReactNode; actions: React.ReactNode }) {
  return (
    <div className="flex flex-col gap-6 sm:flex-row sm:items-end sm:gap-8">
      <div className="w-44 shrink-0 sm:w-52 lg:w-60">
        <ItemArt item={item} aspect="square" width={240} />
      </div>
      <div className="min-w-0 flex-1 space-y-3">
        <div className="text-sm font-medium text-muted">{kindLabel}</div>
        <h1 className="text-3xl font-bold leading-tight tracking-tight text-balance sm:text-5xl">{item.title}</h1>
        {meta}
        <Genres genres={item.genres} />
        <div className="flex flex-wrap items-center gap-2.5 pt-1">{actions}</div>
      </div>
    </div>
  );
}

export function ArtistView({ item }: { item: ItemDetail }) {
  const { t } = useTranslation(["item", "common"]);
  const play = usePlayItem();
  const albums = useChildren(item.id);
  const [gridRef, gridWidth] = useElementWidth<HTMLDivElement>();
  const cols = gridWidth > 0 ? Math.max(2, Math.floor((gridWidth + 18) / (170 + 18))) : 6;
  const colWidth = gridWidth > 0 ? (gridWidth - 18 * (cols - 1)) / cols : 170;
  const sorted = useMemo(
    () =>
      [...(albums.data ?? [])].sort(
        (a, b) => (b.year ?? 0) - (a.year ?? 0) || a.sort_title.localeCompare(b.sort_title, currentLocale()),
      ),
    [albums.data],
  );

  return (
    <div className="animate-fade-in px-4 pb-16 pt-8 sm:px-8">
      <MusicHeader
        item={item}
        kindLabel={t("music.artist")}
        meta={
          <MetaLine
            parts={[
              item.child_count != null ? t("counts.albums", { count: item.child_count }) : null,
              item.leaf_count != null ? t("counts.tracks", { count: item.leaf_count }) : null,
            ]}
          />
        }
        actions={
          <>
            <Button variant="primary" size="lg" icon={<Play className="size-5 fill-current" />} onClick={() => void play(item)}>
              {t("common:actions.play")}
            </Button>
            <Button size="lg" icon={<Shuffle className="size-4" />} onClick={() => void play(item, { shuffle: true })}>
              {t("shuffle")}
            </Button>
            <PlayOn kind="item" cast={() => ({ item_id: item.id })} />
            <PlayOn kind="item" label={t("shuffleOn")} icon={<Shuffle className="size-4" />} cast={() => ({ item_id: item.id, shuffle: true })} />
            <MoreActions item={item} />
          </>
        }
      />
      <Summary text={item.summary} className="mt-6" />
      <section className="mt-10">
        <SectionTitle>{t("music.albums")}</SectionTitle>
        <div ref={gridRef} className="grid gap-x-[18px] gap-y-6" style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}>
          {albums.isPending
            ? Array.from({ length: Math.min(cols, 6) }, (_, i) => <CardSkeleton key={i} aspect="square" />)
            : sorted.map((a) => (
                <MediaCard key={a.id} item={a} aspect="square" width={colWidth} subtitle={a.year ? String(a.year) : ""} />
              ))}
        </div>
        {albums.data && albums.data.length === 0 ? <p className="text-sm text-muted">{t("music.noAlbums")}</p> : null}
        {albums.isError && !albums.data ? (
          <SectionError what="albums" error={albums.error} retry={() => void albums.refetch()} />
        ) : null}
      </section>
    </div>
  );
}

export function AlbumView({ item, highlightId }: { item: ItemDetail; highlightId?: number }) {
  const { t } = useTranslation(["item", "common"]);
  const play = usePlayItem();
  const audio = useAudioPlayer();
  const tracks = useChildren(item.id);
  const list = tracks.data ?? [];
  const totalMs = list.reduce((sum, t) => sum + (t.duration_ms ?? 0), 0);
  const discs = new Set(list.map((t) => t.parent_index ?? 1));
  const multiDisc = discs.size > 1;

  const playFrom = (i: number) => audio.playQueue(list, i);

  return (
    <div className="animate-fade-in px-4 pb-16 pt-8 sm:px-8">
      <MusicHeader
        item={item}
        kindLabel={t("music.album")}
        meta={
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
            <MetaLine
              parts={[
                item.parent_id ? (
                  <Link key="artist" to={itemUrl(item.parent_id)} className="font-semibold text-fg hover:text-accent">
                    {item.parent_title ?? t("music.artist")}
                  </Link>
                ) : (
                  item.parent_title
                ),
                item.year,
                list.length ? t("counts.tracks", { count: list.length }) : null,
                totalMs ? formatDuration(totalMs) : null,
              ]}
            />
            <StarRating item={item} />
          </div>
        }
        actions={
          <>
            <Button
              variant="primary"
              size="lg"
              icon={<Play className="size-5 fill-current" />}
              onClick={() => (list.length ? playFrom(0) : void play(item))}
              disabled={tracks.isSuccess && list.length === 0}
            >
              {t("common:actions.play")}
            </Button>
            <Button
              size="lg"
              icon={<Shuffle className="size-4" />}
              onClick={() => (list.length ? audio.playQueue(list, 0, { shuffle: true }) : void play(item, { shuffle: true }))}
              disabled={tracks.isSuccess && list.length === 0}
            >
              {t("shuffle")}
            </Button>
            {tracks.isSuccess && list.length === 0 ? null : (
              <>
                <PlayOn kind="item" cast={() => ({ item_id: item.id })} />
                <PlayOn kind="item" label={t("shuffleOn")} icon={<Shuffle className="size-4" />} cast={() => ({ item_id: item.id, shuffle: true })} />
              </>
            )}
            <MoreActions item={item} />
          </>
        }
      />
      <Summary text={item.summary} className="mt-6" />

      <section className="mt-8 max-w-4xl">
        {tracks.isPending ? (
          <div className="space-y-2">
            {Array.from({ length: 8 }, (_, i) => (
              <Skeleton key={i} className="h-10" />
            ))}
          </div>
        ) : tracks.isError && !tracks.data ? (
          <SectionError what="tracks" error={tracks.error} retry={() => void tracks.refetch()} />
        ) : (
          <div role="table" aria-label={t("music.tracks")} className="text-sm">
            <div role="row" className="grid grid-cols-[2.5rem_1fr_4rem] gap-3 border-b border-border px-3 pb-2 text-xs font-medium text-subtle">
              <span role="columnheader" className="text-right">#</span>
              <span role="columnheader">{t("music.title")}</span>
              <span role="columnheader" className="text-right">{t("music.time")}</span>
            </div>
            {list.map((t, i) => {
              const showDisc = multiDisc && (i === 0 || (list[i - 1]?.parent_index ?? 1) !== (t.parent_index ?? 1));
              return (
                <TrackRow
                  key={t.id}
                  track={t}
                  disc={showDisc ? (t.parent_index ?? 1) : null}
                  current={audio.current?.id === t.id}
                  playing={audio.current?.id === t.id && audio.playing}
                  highlighted={highlightId === t.id}
                  albumArtist={item.parent_title}
                  onPlay={() => (audio.current?.id === t.id ? audio.toggle() : playFrom(i))}
                />
              );
            })}
            {list.length === 0 ? <SectionTitle className="mt-6">{t("music.noTracks")}</SectionTitle> : null}
          </div>
        )}
      </section>
    </div>
  );
}

function TrackRow({
  track,
  disc,
  current,
  playing,
  highlighted,
  albumArtist,
  onPlay,
}: {
  track: ChildItem;
  disc: number | null;
  current: boolean;
  playing: boolean;
  highlighted: boolean;
  albumArtist?: string | null;
  onPlay: () => void;
}) {
  const { t } = useTranslation("item");
  const artist = track.grandparent_title && track.grandparent_title !== albumArtist ? track.grandparent_title : null;
  return (
    <>
      {disc != null ? (
        <div className="px-3 pb-1 pt-4 text-xs font-medium text-subtle">{t("music.disc", { number: disc })}</div>
      ) : null}
      <div
        role="row"
        onDoubleClick={onPlay}
        className={cn(
          "group grid grid-cols-[2.5rem_1fr_4rem] items-center gap-3 rounded-md px-3 py-2 transition-colors hover:bg-white/[0.05]",
          current && "bg-white/[0.04]",
          highlighted && !current && "bg-accent/10",
        )}
      >
        <span role="cell" className="relative flex justify-end">
          <span className={cn("tabular-nums text-subtle group-hover:invisible", current && "invisible")}>{track.index ?? ""}</span>
          <button
            type="button"
            aria-label={playing ? t("pauseTitle", { title: track.title }) : t("playTitle", { title: track.title })}
            onClick={onPlay}
            className={cn(
              "absolute inset-y-0 right-0 flex items-center text-fg",
              current ? "visible text-accent" : "invisible group-hover:visible",
            )}
          >
            {playing ? <AudioLines className="size-4 animate-pulse" /> : <Play className="size-4 fill-current" />}
          </button>
        </span>
        <span role="cell" className="flex min-w-0 items-center gap-2">
          <span className="min-w-0 flex-1">
            <span className={cn("block truncate font-medium", current ? "text-accent" : "text-fg")}>{track.title}</span>
            {artist ? <span className="block truncate text-xs text-muted">{artist}</span> : null}
          </span>
          {/* A double click here isn't one on the row, which plays the track here. */}
          <span onDoubleClick={(e) => e.stopPropagation()}>
            <PlayOn
              kind="item"
              align="right"
              cast={() => ({ item_id: track.id })}
              trigger={({ open, toggle }) => (
                <IconButton
                  label={t("playTitleOn", { title: track.title })}
                  size="xs"
                  aria-haspopup="menu"
                  aria-expanded={open}
                  active={open}
                  onClick={toggle}
                  // With the pointer over the row or focus in it, as the row's Play.
                  className="opacity-100 transition-opacity [@media(hover:hover)]:sm:opacity-0 sm:group-focus-within:opacity-100 sm:group-hover:opacity-100"
                >
                  <Cast className="size-3.5" />
                </IconButton>
              )}
            />
          </span>
        </span>
        <span role="cell" className="text-right tabular-nums text-muted">
          {track.duration_ms ? formatClock(track.duration_ms / 1000) : ""}
        </span>
      </div>
    </>
  );
}

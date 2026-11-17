import { useTranslation } from "react-i18next";
import { AudioLines, Check, Play, RotateCcw } from "lucide-react";
import { useChildren, useItem, useOnDeck, type ChildItem } from "@/api/hooks";
import type { Chapter, ItemDetail } from "@/api/types";
import { useAudioPlayer, useAudioTime } from "@/audio/AudioPlayer";
import { Button, ProgressBar, SectionTitle, Skeleton } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { cn } from "@/lib/cn";
import { formatClock, formatDuration } from "@/lib/format";
import { usePlayItem } from "@/lib/usePlayItem";
import { chapterTitle } from "@/player/ChaptersMenu";
import { Genres, ItemArt, Labels, MetaLine, SectionError, Summary, WatchedButton } from "./shared";

/** A list of names as people say it: "A", "A and B", "A, B and C". */
export function joinNames(names: string[], and: string): string {
  if (names.length <= 1) return names[0] ?? "";
  return `${names.slice(0, -1).join(", ")} ${and} ${names[names.length - 1]}`;
}

/** "37 %", in the language shown; never 100 % before the end. */
function percent(fraction: number): string {
  const floored = Math.min(0.99, Math.floor(fraction * 100) / 100);
  return new Intl.NumberFormat(currentLocale(), { style: "percent" }).format(floored);
}

/** How much of a book has been heard: its finished parts, and the one part way through. */
function heardMs(parts: ChildItem[]): number {
  return parts.reduce(
    (sum, p) => sum + (p.user_state.watched ? (p.duration_ms ?? 0) : p.user_state.position_ms),
    0,
  );
}

/**
 * An audiobook's page: its cover, who wrote and reads it, its series,
 * Play (where it was left) and its parts, or the chapters of a book in
 * one file.
 */
export function BookView({ item }: { item: ItemDetail }) {
  const { t } = useTranslation(["item", "common"]);
  const play = usePlayItem();
  const audio = useAudioPlayer();
  const parts = useChildren(item.id);
  const deck = useOnDeck(item.id);
  const list = parts.data ?? [];
  const book = item.book;
  const and = t("book.and");

  const totalMs = item.duration_ms ?? list.reduce((sum, p) => sum + (p.duration_ms ?? 0), 0);
  const heard = list.length ? heardMs(list) : 0;
  const finished = list.length > 0 && list.every((p) => p.user_state.watched);
  const started = !finished && heard > 0;
  const playingThis = audio.current?.parent_id === item.id;

  const playPart = (i: number, fromStartOfPart = false) => {
    const part = list[i];
    if (!part) return;
    if (audio.current?.id === part.id && !fromStartOfPart) {
      audio.toggle();
      return;
    }
    const at = fromStartOfPart || part.user_state.watched ? 0 : part.user_state.position_ms;
    audio.playQueue(list, i, { startAtMs: at });
  };

  const series =
    book?.series != null
      ? book.series_index != null
        ? t("book.seriesPlace", { series: book.series, number: book.series_index })
        : book.series
      : null;

  return (
    <div className="animate-fade-in px-4 pb-16 pt-8 sm:px-8">
      <div className="flex flex-col gap-6 sm:flex-row sm:items-end sm:gap-8">
        <div className="w-44 shrink-0 sm:w-52 lg:w-60">
          <ItemArt item={item} aspect="square" width={240} />
        </div>
        <div className="min-w-0 flex-1 space-y-3">
          <div className="flex flex-wrap items-center gap-2.5 text-sm font-medium text-muted">
            <span>{series ?? t("book.kind")}</span>
            {started && totalMs ? (
              <span className="rounded-full bg-accent/15 px-2.5 py-0.5 text-xs font-semibold text-accent">
                {t("book.percentListened", { percent: percent(heard / totalMs) })}
              </span>
            ) : null}
          </div>
          <h1 className="text-3xl font-bold leading-tight tracking-tight text-balance sm:text-5xl">{item.title}</h1>
          {book?.authors.length ? (
            <div className="text-lg font-semibold">{t("book.by", { names: joinNames(book.authors, and) })}</div>
          ) : null}
          {book?.narrators.length ? (
            <div className="text-sm text-muted">{t("book.readBy", { names: joinNames(book.narrators, and) })}</div>
          ) : null}
          <MetaLine
            parts={[
              item.year,
              totalMs ? formatDuration(totalMs) : null,
              list.length > 1 ? t("counts.parts", { count: list.length }) : null,
            ]}
          />
          {started && totalMs ? (
            <div className="flex max-w-sm items-center gap-3">
              <ProgressBar value={Math.min(1, heard / totalMs)} className="h-1.5 flex-1" label={t("book.progress")} />
              <span className="shrink-0 text-xs text-muted">
                {t("common:format.left", { time: formatDuration(Math.max(0, totalMs - heard)) })}
              </span>
            </div>
          ) : null}
          <Genres genres={item.genres} />
          <Labels labels={item.labels} />
          <div className="flex flex-wrap items-center gap-2.5 pt-1">
            <Button
              variant="primary"
              size="lg"
              icon={<Play className="size-5 fill-current" />}
              onClick={() => (playingThis ? audio.toggle() : void play(item))}
              disabled={parts.isSuccess && list.length === 0}
            >
              {playingThis && audio.playing
                ? t("book.pause")
                : started
                  ? t("book.resume")
                  : finished
                    ? t("book.listenAgain")
                    : t("common:actions.play")}
            </Button>
            {started ? (
              <Button size="lg" icon={<RotateCcw className="size-4" />} onClick={() => list.length && audio.playQueue(list, 0)}>
                {t("book.fromStart")}
              </Button>
            ) : null}
            <WatchedButton item={item} />
          </div>
          {started && deck.data && list.length > 1 ? (
            <div className="text-xs text-muted">{t("book.upTo", { part: deck.data.title })}</div>
          ) : null}
        </div>
      </div>

      <Summary text={item.summary} className="mt-6" />

      <section className="mt-8 max-w-4xl">
        {parts.isPending ? (
          <div className="space-y-2">
            {Array.from({ length: 6 }, (_, i) => (
              <Skeleton key={i} className="h-10" />
            ))}
          </div>
        ) : parts.isError && !parts.data ? (
          <SectionError what="parts" error={parts.error} retry={() => void parts.refetch()} />
        ) : list.length === 1 && list[0] ? (
          <Chapters part={list[0]} parts={list} />
        ) : (
          <>
            <SectionTitle>{t("book.parts")}</SectionTitle>
            <ol className="text-sm">
              {list.map((p, i) => (
                <PartRow
                  key={p.id}
                  part={p}
                  current={audio.current?.id === p.id}
                  playing={audio.current?.id === p.id && audio.playing}
                  onPlay={() => playPart(i)}
                />
              ))}
            </ol>
          </>
        )}
      </section>
    </div>
  );
}

function PartRow({ part, current, playing, onPlay }: { part: ChildItem; current: boolean; playing: boolean; onPlay: () => void }) {
  const { t } = useTranslation("item");
  const duration = part.duration_ms ?? 0;
  const fraction = !part.user_state.watched && duration ? part.user_state.position_ms / duration : 0;
  return (
    <li>
      <button
        type="button"
        onClick={onPlay}
        aria-label={playing ? t("pauseTitle", { title: part.title }) : t("playTitle", { title: part.title })}
        aria-current={current ? "true" : undefined}
        className={cn(
          "group grid w-full grid-cols-[2.5rem_1fr_5rem] items-center gap-3 rounded-md px-3 py-2 text-left transition-colors hover:bg-white/[0.05]",
          current && "bg-white/[0.04]",
        )}
      >
        <span className="flex justify-end text-subtle">
          {playing ? (
            <AudioLines className="size-4 animate-pulse text-accent" />
          ) : (
            <>
              <span className="tabular-nums group-hover:hidden">{part.index ?? ""}</span>
              <Play className="hidden size-4 fill-current text-fg group-hover:block" />
            </>
          )}
        </span>
        <span className="min-w-0">
          <span className={cn("block truncate font-medium", current ? "text-accent" : "text-fg")}>{part.title}</span>
          {fraction > 0 ? <ProgressBar value={fraction} className="mt-1 h-1 max-w-48" /> : null}
        </span>
        <span className="flex items-center justify-end gap-2 tabular-nums text-muted">
          {part.user_state.watched ? <Check className="size-4 text-accent" aria-label={t("book.heard")} /> : null}
          {duration ? formatClock(duration / 1000) : ""}
        </span>
      </button>
    </li>
  );
}

/** The chapters of a book in one file: choosing one plays from its start. */
function Chapters({ part, parts }: { part: ChildItem; parts: ChildItem[] }) {
  const { t } = useTranslation("item");
  const audio = useAudioPlayer();
  const time = useAudioTime();
  const detail = useItem(part.id);
  const chapters: Chapter[] = detail.data?.files[0]?.chapters ?? [];
  if (detail.isPending || chapters.length < 2) return null;
  const here = audio.current?.id === part.id;
  const nowMs = here ? time.position * 1000 : part.user_state.watched ? -1 : part.user_state.position_ms;
  const at = (c: Chapter) => {
    if (here) audio.seek(c.start_ms / 1000);
    else audio.playQueue(parts, 0, { startAtMs: c.start_ms });
  };
  return (
    <>
      <SectionTitle>{t("book.chapters")}</SectionTitle>
      <ol className="text-sm">
        {chapters.map((c, i) => {
          const isNow = nowMs >= c.start_ms && nowMs < c.end_ms;
          return (
            <li key={c.start_ms}>
              <button
                type="button"
                onClick={() => at(c)}
                aria-current={isNow ? "true" : undefined}
                className={cn(
                  "grid w-full grid-cols-[2.5rem_1fr_5rem] items-center gap-3 rounded-md px-3 py-2 text-left transition-colors hover:bg-white/[0.05]",
                  isNow && "bg-white/[0.04] text-accent",
                )}
              >
                <span className="text-right tabular-nums text-subtle">{i + 1}</span>
                <span className="truncate font-medium">{chapterTitle(c, i)}</span>
                <span className="text-right tabular-nums text-muted">{formatClock(c.start_ms / 1000)}</span>
              </button>
            </li>
          );
        })}
      </ol>
    </>
  );
}

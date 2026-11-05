import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { ChevronRight, Play } from "lucide-react";
import { useChildren, useOnDeck, type ChildItem } from "@/api/hooks";
import type { ItemDetail } from "@/api/types";
import { Artwork } from "@/components/Artwork";
import { PlayOn } from "@/components/PlayOn";
import { Button, SectionTitle, Skeleton } from "@/components/ui";
import { episodeLabel, formatDate, formatDuration, progressFraction, seasonTitle } from "@/lib/format";
import { thumbCandidates } from "@/lib/images";
import { prefetchPlay } from "@/lib/prefetchPlay";
import { usePlayItem } from "@/lib/usePlayItem";
import { playIntent } from "@/player/preload";
import { MoreActions } from "./ItemActions";
import { ItemArt, ItemPageShell, MetaLine, SectionError, Summary, WatchedButton } from "./shared";
import { CastRow } from "./Credits";
import { itemUrl } from "@/lib/urls";

function EpisodeRow({ ep }: { ep: ChildItem }) {
  const { t } = useTranslation("item");
  const play = usePlayItem();
  const progress = ep.user_state.watched ? 0 : progressFraction(ep);
  return (
    <div className="group flex gap-4 rounded-lg p-2 transition-colors hover:bg-white/[0.035] sm:gap-5">
      <div className="relative w-36 shrink-0 sm:w-56">
        <Link to={itemUrl(ep.id)} className="block overflow-hidden rounded-md ring-1 ring-white/5" aria-label={ep.title}>
          <Artwork candidates={thumbCandidates(ep)} width={224} alt="" title={ep.title} className="aspect-video w-full" />
          {progress > 0 ? (
            <div className="absolute inset-x-0 bottom-0 h-1 bg-black/60">
              <div className="h-full bg-accent" style={{ width: `${progress * 100}%` }} />
            </div>
          ) : null}
        </Link>
        <button
          type="button"
          aria-label={t("playTitle", { title: ep.title })}
          onClick={() => void play(ep)}
          {...playIntent(ep.kind, () => prefetchPlay(ep))}
          className="absolute left-1/2 top-1/2 flex size-10 -translate-x-1/2 -translate-y-1/2 items-center justify-center rounded-full bg-accent text-accent-fg opacity-0 shadow-lg transition hover:bg-accent-hover focus-visible:opacity-100 group-hover:opacity-100"
        >
          <Play className="ml-0.5 size-4 fill-current" />
        </button>
      </div>
      <div className="min-w-0 flex-1 py-0.5">
        <div className="flex items-start justify-between gap-2">
          <Link
            to={itemUrl(ep.id)}
            className="line-clamp-2 font-semibold leading-snug text-fg transition-colors hover:text-accent"
          >
            {ep.index != null ? `${ep.index}. ` : ""}
            {ep.title}
          </Link>
          <WatchedButton item={ep} compact />
        </div>
        <MetaLine
          className="mt-0.5 text-xs"
          parts={[formatDuration(ep.duration_ms), formatDate(ep.release_date ?? null)]}
        />
        {ep.summary ? <p className="mt-1.5 line-clamp-2 text-sm text-muted">{ep.summary}</p> : null}
      </div>
    </div>
  );
}

export function SeasonView({ item }: { item: ItemDetail }) {
  const { t } = useTranslation(["item", "common"]);
  const play = usePlayItem();
  const episodes = useChildren(item.id);
  const onDeck = useOnDeck(item.id);
  const ep = onDeck.data;
  const resuming = ep && ep.user_state.position_ms > 0 && !ep.user_state.watched;
  const title = seasonTitle(item);

  return (
    <ItemPageShell item={item}>
      <nav className="mb-4 flex items-center gap-1 text-sm text-muted" aria-label={t("season.breadcrumb")}>
        {item.parent_id ? (
          <Link to={itemUrl(item.parent_id)} className="font-medium transition-colors hover:text-fg">
            {item.parent_title ?? t("season.show")}
          </Link>
        ) : null}
        <ChevronRight className="size-4 text-subtle" />
        <span className="text-fg">{title}</span>
      </nav>
      <div className="flex flex-col gap-6 sm:flex-row sm:gap-10">
        <div className="hidden w-44 shrink-0 sm:block md:w-52">
          <ItemArt item={item} aspect="poster" width={208} />
        </div>
        <div className="min-w-0 flex-1 space-y-4">
          <div>
            {item.parent_title ? <div className="text-lg font-semibold text-fg/80">{item.parent_title}</div> : null}
            <h1 className="text-3xl font-bold tracking-tight sm:text-4xl">{title}</h1>
          </div>
          <MetaLine parts={[item.year, item.leaf_count ? t("counts.episodes", { count: item.leaf_count }) : null]} />
          <div className="flex flex-wrap items-center gap-2.5 pt-1">
            <Button
              variant="primary"
              size="lg"
              icon={<Play className="size-5 fill-current" />}
              onClick={() => void play(item)}
              {...playIntent(item.kind)}
              disabled={episodes.data?.length === 0}
            >
              {ep
                ? t(resuming ? "resumeEpisode" : "playEpisode", { episode: episodeLabel(ep) })
                : t("common:actions.play")}
            </Button>
            {/* The device plays the episode its own Play would. */}
            {episodes.data?.length === 0 ? null : <PlayOn kind="item" cast={() => ({ item_id: item.id })} />}
            <WatchedButton item={item} />
            <MoreActions item={item} />
          </div>
          <Summary text={item.summary} />
        </div>
      </div>

      <section className="mt-10 max-w-5xl">
        <SectionTitle>{t("season.episodes")}</SectionTitle>
        <div className="-mx-2 space-y-1">
          {episodes.isPending
            ? Array.from({ length: 4 }, (_, i) => (
                <div key={i} className="flex gap-5 p-2">
                  <Skeleton className="aspect-video w-36 sm:w-56" />
                  <div className="flex-1 space-y-2 py-1">
                    <Skeleton className="h-4 w-1/2" />
                    <Skeleton className="h-3 w-1/4" />
                    <Skeleton className="h-3 w-3/4" />
                  </div>
                </div>
              ))
            : episodes.data?.map((e) => <EpisodeRow key={e.id} ep={e} />)}
          {episodes.data && episodes.data.length === 0 ? <p className="px-2 text-sm text-muted">{t("season.noEpisodes")}</p> : null}
          {episodes.isError && !episodes.data ? (
            <div className="px-2">
              <SectionError what="episodes" error={episodes.error} retry={() => void episodes.refetch()} />
            </div>
          ) : null}
        </div>
      </section>
      <CastRow title={t("cast")} credits={item.cast} />
    </ItemPageShell>
  );
}

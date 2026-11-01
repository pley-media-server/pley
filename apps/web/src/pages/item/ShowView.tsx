import { Play } from "lucide-react";
import { useChildren, useOnDeck } from "@/api/hooks";
import type { ItemDetail } from "@/api/types";
import { CardSkeleton, MediaCard } from "@/components/cards";
import { PartOfCollections } from "@/components/lists/PartOfCollections";
import { StarRating } from "@/components/StarRating";
import { Button, SectionTitle } from "@/components/ui";
import { episodeLabel, pluralize } from "@/lib/format";
import { useElementWidth } from "@/lib/hooks";
import { usePlayItem } from "@/lib/usePlayItem";
import { playIntent } from "@/player/preload";
import { CastRow, CrewRows, hasCrew } from "./Credits";
import { ExtrasRow } from "./Extras";
import { MoreLikeThis } from "./MoreLikeThis";
import { MoreActions } from "./ItemActions";
import { ItemRequestLine } from "./ItemRequest";
import {
  ContentRating,
  Genres,
  ItemArt,
  ItemPageShell,
  MetaLine,
  MyListButton,
  RatingBadge,
  SectionError,
  Summary,
  WatchedButton,
} from "./shared";

export function ShowView({ item }: { item: ItemDetail }) {
  const play = usePlayItem();
  const seasons = useChildren(item.id);
  const onDeck = useOnDeck(item.id);
  const [gridRef, gridWidth] = useElementWidth<HTMLDivElement>();
  const cols = gridWidth > 0 ? Math.max(2, Math.floor((gridWidth + 18) / (150 + 18))) : 6;
  const colWidth = gridWidth > 0 ? (gridWidth - 18 * (cols - 1)) / cols : 150;

  const ep = onDeck.data;
  const resuming = ep && ep.user_state.position_ms > 0 && !ep.user_state.watched;
  const playLabel = ep ? `${resuming ? "Resume" : "Play"} ${episodeLabel(ep)}` : "Play";

  return (
    <ItemPageShell item={item}>
      <div className="flex flex-col gap-6 sm:flex-row sm:gap-10">
        <div className="hidden w-52 shrink-0 sm:block md:w-60 lg:w-64">
          <ItemArt item={item} aspect="poster" width={256} />
        </div>
        <div className="min-w-0 flex-1 space-y-4">
          <h1 className="text-3xl font-bold leading-tight tracking-tight text-balance sm:text-4xl">{item.title}</h1>
          <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
            <MetaLine
              parts={[
                item.year,
                item.child_count ? pluralize(item.child_count, "season") : null,
                item.leaf_count ? pluralize(item.leaf_count, "episode") : null,
                item.content_rating ? <ContentRating key="cr" value={item.content_rating} /> : null,
                item.rating != null && item.rating > 0 ? <RatingBadge key="r" rating={item.rating} /> : null,
                item.studio,
              ]}
            />
            <StarRating item={item} />
          </div>
          <Genres genres={item.genres} />
          <PartOfCollections item={item} />
          <div className="flex flex-wrap items-center gap-2.5 pt-1">
            <Button
              variant="primary"
              size="lg"
              icon={<Play className="size-5 fill-current" />}
              onClick={() => void play(item)}
              {...playIntent(item.kind)}
              disabled={!item.leaf_count && !ep}
            >
              {playLabel}
            </Button>
            <MyListButton item={item} />
            <WatchedButton item={item} />
            <MoreActions item={item} />
          </div>
          <ItemRequestLine item={item} />
          {item.tagline ? <p className="text-base italic text-fg/75">{item.tagline}</p> : null}
          <Summary text={item.summary} />
          {hasCrew(item.crew) ? (
            <dl className="grid max-w-3xl grid-cols-[auto_1fr] gap-x-6 gap-y-1.5 pt-2 text-sm">
              <CrewRows crew={item.crew} />
            </dl>
          ) : null}
        </div>
      </div>

      <section className="mt-12">
        <SectionTitle>Seasons</SectionTitle>
        <div
          ref={gridRef}
          className="grid gap-x-[18px] gap-y-6"
          style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}
        >
          {seasons.isPending
            ? Array.from({ length: Math.min(cols, 6) }, (_, i) => <CardSkeleton key={i} aspect="poster" />)
            : seasons.data?.map((s) => <MediaCard key={s.id} item={s} aspect="poster" width={colWidth} />)}
        </div>
        {seasons.data && seasons.data.length === 0 ? <p className="text-sm text-muted">No seasons found.</p> : null}
        {seasons.isError && !seasons.data ? (
          <SectionError what="the seasons" error={seasons.error} retry={() => void seasons.refetch()} />
        ) : null}
      </section>
      <ExtrasRow extras={item.extras} />
      <CastRow title="Cast" credits={item.cast} />
      <MoreLikeThis item={item} />
    </ItemPageShell>
  );
}

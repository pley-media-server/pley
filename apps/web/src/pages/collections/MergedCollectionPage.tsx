import { useTranslation } from "react-i18next";
import { Link, useLocation, useSearchParams } from "react-router";
import { Layers, Play, Shuffle } from "lucide-react";
import { errorMessage, isApiError } from "@/api/client";
import { useLibraries } from "@/api/hooks";
import { useMergedCollection, type MergedCollection } from "@/api/lists";
import type { Id } from "@/api/types";
import { CardSkeleton, MediaCard } from "@/components/cards";
import { Button, EmptyState, Skeleton } from "@/components/ui";
import { useDocumentTitle, useElementWidth } from "@/lib/hooks";
import { decodeId } from "@/lib/ids";
import { usePlayItem } from "@/lib/usePlayItem";
import { useVideoQueue } from "@/lib/videoQueue";
import { NotFoundPage } from "../NotFoundPage";
import { MetaLine } from "../item/shared";
import { collectionUrl } from "@/lib/urls";

/** The collections of one title in several libraries, shown as one row on Home: all their titles. */
export function MergedCollectionPage() {
  const { t } = useTranslation(["item", "common"]);
  const [params] = useSearchParams();
  const ids = (params.get("ids") ?? "")
    .split(",")
    .map((id) => decodeId(id))
    .filter((id): id is Id => id !== null);
  const merged = useMergedCollection(ids);
  useDocumentTitle(merged.data?.title);

  if (!ids.length) return <NotFoundPage />;
  if (merged.isPending) return <MergedSkeleton />;
  if (merged.isError) {
    if (isApiError(merged.error, 404)) return <NotFoundPage />;
    return (
      <EmptyState
        className="min-h-[60vh]"
        title={t("collection.loadError")}
        description={errorMessage(merged.error)}
        action={<Button onClick={() => void merged.refetch()}>{t("common:actions.retry")}</Button>}
      />
    );
  }
  return <MergedView merged={merged.data} />;
}

function MergedView({ merged }: { merged: MergedCollection }) {
  const { t } = useTranslation(["item", "common"]);
  const libs = useLibraries();
  const startQueue = useVideoQueue();
  const play = usePlayItem();
  const location = useLocation();
  const [ref, width] = useElementWidth<HTMLDivElement>();
  const gap = width > 0 && width < 640 ? 12 : 18;
  const cols = width > 0 ? Math.max(2, Math.floor((width + gap) / (160 + gap))) : 6;
  const colWidth = width > 0 ? (width - gap * (cols - 1)) / cols : 160;
  const items = merged.items;
  const movies = items.filter((i) => i.kind === "movie");
  const shows = items.filter((i) => i.kind === "show").length;
  const returnTo = `${location.pathname}${location.search}`;

  const onPlay = (shuffle: boolean) => {
    // Movies play one after another; a show plays from where it's up to.
    if (movies.length) return startQueue(movies, { shuffle, returnTo });
    const pick = shuffle ? anyOf(items) : items[0];
    if (pick) void play(pick);
  };

  return (
    <div className="px-4 pb-16 pt-6 animate-fade-in sm:px-8 sm:pt-10">
      <div className="space-y-4">
        <div className="flex items-center gap-2 text-sm font-medium text-muted">
          <Layers className="size-3.5" /> {t("collection.label")}
        </div>
        <h1 className="text-3xl font-bold leading-tight tracking-tight text-balance sm:text-4xl">{merged.title}</h1>
        <MetaLine
          parts={[
            movies.length ? t("counts.movies", { count: movies.length }) : null,
            shows ? t("counts.shows", { count: shows }) : null,
            // Each library's own collection, to open on its own.
            ...merged.collections.map((c) => {
              const lib = libs.data?.find((l) => l.id === c.library_id);
              return lib ? (
                <Link key={c.id} to={collectionUrl(c.id)} className="transition-colors hover:text-fg">
                  {lib.name}
                </Link>
              ) : null;
            }),
          ]}
        />
        <div className="flex flex-wrap items-center gap-2.5 pt-1">
          <Button variant="primary" size="lg" icon={<Play className="size-5 fill-current" />} onClick={() => onPlay(false)} disabled={!items.length}>
            {t("common:actions.play")}
          </Button>
          <Button size="lg" icon={<Shuffle className="size-4" />} onClick={() => onPlay(true)} disabled={items.length < 2}>
            {t("shuffle")}
          </Button>
        </div>
      </div>
      <div
        ref={ref}
        className="mt-10 grid"
        style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))`, columnGap: gap, rowGap: gap < 18 ? 18 : 26 }}
      >
        {items.map((item) => (
          <MediaCard key={item.id} item={item} aspect="poster" width={colWidth} />
        ))}
      </div>
    </div>
  );
}

function anyOf<T>(list: T[]): T | undefined {
  return list[Math.floor(Math.random() * list.length)];
}

function MergedSkeleton() {
  return (
    <div className="px-4 pb-16 pt-6 sm:px-8 sm:pt-10">
      <div className="space-y-4">
        <Skeleton className="h-4 w-24" />
        <Skeleton className="h-10 w-2/3" />
        <Skeleton className="h-4 w-1/3" />
      </div>
      <div className="mt-10 grid grid-cols-[repeat(auto-fill,minmax(160px,1fr))] gap-x-[18px] gap-y-[26px]">
        {Array.from({ length: 6 }, (_, i) => (
          <CardSkeleton key={i} aspect="poster" />
        ))}
      </div>
    </div>
  );
}

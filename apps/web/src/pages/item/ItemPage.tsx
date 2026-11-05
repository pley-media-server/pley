import { useTranslation } from "react-i18next";
import { Navigate, useLocation, useParams } from "react-router";
import { errorMessage, isApiError } from "@/api/client";
import { useItem } from "@/api/hooks";
import { Button, EmptyState, Skeleton } from "@/components/ui";
import { fullTitle } from "@/lib/format";
import { useDocumentTitle } from "@/lib/hooks";
import { NotFoundPage } from "../NotFoundPage";
import { AlbumView, ArtistView } from "./MusicViews";
import { SeasonView } from "./SeasonView";
import { ShowView } from "./ShowView";
import { VideoItemView } from "./VideoItemView";
import { itemUrl } from "@/lib/urls";
import { idParam } from "@/lib/ids";
import { usePreloadPlayerWhenIdle } from "@/player/preload";

function ItemSkeleton() {
  return (
    <div className="px-4 pb-16 pt-6 sm:px-8 sm:pt-[10vh] lg:pt-[14vh]">
      <div className="flex flex-col gap-8 sm:flex-row sm:gap-10">
        <Skeleton className="hidden aspect-[2/3] w-60 shrink-0 rounded-lg sm:block" />
        <div className="flex-1 space-y-4">
          <Skeleton className="h-10 w-2/3" />
          <Skeleton className="h-4 w-1/3" />
          <Skeleton className="h-12 w-48" />
          <Skeleton className="h-4 w-full max-w-2xl" />
          <Skeleton className="h-4 w-5/6 max-w-2xl" />
          <Skeleton className="h-4 w-2/3 max-w-2xl" />
        </div>
      </div>
    </div>
  );
}

export function ItemPage() {
  const { t } = useTranslation(["item", "common"]);
  const { id } = useParams();
  const location = useLocation();
  const highlight = (location.state as { highlight?: number } | null)?.highlight;
  const itemId = idParam(id);
  const item = useItem(itemId);
  useDocumentTitle(item.data ? fullTitle(item.data) : null);
  // Play is the likely next step: its code is fetched by then.
  usePreloadPlayerWhenIdle(item.data?.kind);

  if (!Number.isFinite(itemId) || itemId <= 0) return <NotFoundPage />;
  if (item.isPending) return <ItemSkeleton />;
  if (item.isError) {
    if (isApiError(item.error, 404)) return <NotFoundPage />;
    return (
      <EmptyState
        className="min-h-[60vh]"
        title={t("loadError")}
        description={errorMessage(item.error)}
        action={<Button onClick={() => void item.refetch()}>{t("common:actions.retry")}</Button>}
      />
    );
  }

  const data = item.data;
  // Key by id so local state (selected audio/subtitle, etc.) resets between items.
  switch (data.kind) {
    case "movie":
    case "episode":
      return <VideoItemView key={data.id} item={data} />;
    case "show":
      return <ShowView key={data.id} item={data} />;
    case "season":
      return <SeasonView key={data.id} item={data} />;
    case "artist":
      return <ArtistView key={data.id} item={data} />;
    case "album":
      return <AlbumView key={data.id} item={data} highlightId={highlight} />;
    case "extra":
      return data.parent_id ? <Navigate to={itemUrl(data.parent_id)} replace /> : <NotFoundPage />;
    case "track":
      return data.parent_id ? <Navigate to={itemUrl(data.parent_id)} replace state={{ highlight: data.id }} /> : <NotFoundPage />;
    default:
      return <NotFoundPage />;
  }
}

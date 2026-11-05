import { useTranslation } from "react-i18next";
import { Link } from "react-router";
import { Plus } from "lucide-react";
import { hasRequests, mayRequest, useItemRequest, useRequestSummary } from "@/api/requests";
import type { ItemDetail } from "@/api/types";
import { buttonClass } from "@/components/ui";
import { titleUrl } from "@/lib/urls";
import { StagePill } from "../requests/shared";

/**
 * How requests stand for a movie or show in a library: the stage of the
 * request this account made or follows (anyone's, for managers), and for
 * a show with seasons still to ask for, a way to ask. Asked for once the
 * page is on screen (its query starts after the page's first commit), and
 * only by accounts with requests to see; nothing shows otherwise.
 */
export function ItemRequestLine({ item }: { item: ItemDetail }) {
  const { t } = useTranslation("item");
  const summary = useRequestSummary().data;
  const titled = item.kind === "movie" || item.kind === "show";
  const state = useItemRequest(item.id, titled && hasRequests(summary)).data;
  if (!state || !summary) return null;
  const request = state.request;
  const more = state.kind === "show" && !!state.askable?.length && mayRequest(summary);
  if (!request && !more) return null;
  return (
    <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
      {request ? (
        <>
          <StagePill request={request} manage={summary.manage} className="max-w-sm" />
          <Link to={titleUrl(request.kind, request.tmdb_id)} className="text-xs font-medium text-muted transition-colors hover:text-accent">
            {t("request.see")}
          </Link>
        </>
      ) : null}
      {more ? (
        <Link to={`${titleUrl("show", state.tmdb_id)}?pick=1`} className={buttonClass("secondary", "sm")}>
          <Plus className="size-4" />
          {t("request.moreSeasons")}
        </Link>
      ) : null}
    </div>
  );
}

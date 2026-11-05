import { Link } from "react-router";
import { X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { useDeleteHistory, useHistory, type HistoryScope } from "@/api/hooks";
import type { HistoryEntry } from "@/api/types";
import { Artwork } from "@/components/Artwork";
import { Avatar } from "@/components/Avatar";
import { useToast } from "@/components/Toast";
import { Button, IconButton, Skeleton } from "@/components/ui";
import { cn } from "@/lib/cn";
import { episodeLabel, formatRelative, formatTimestamp } from "@/lib/format";
import { posterCandidates } from "@/lib/images";
import { itemUrl } from "@/lib/urls";

function describe(entry: HistoryEntry): { title: string; subtitle: string } {
  const { item } = entry;
  if (item.kind === "episode") {
    return {
      title: item.grandparent_title ?? item.title,
      subtitle: [episodeLabel(item), item.title].filter(Boolean).join(" · "),
    };
  }
  if (item.kind === "track") {
    return { title: item.title, subtitle: [item.grandparent_title, item.parent_title].filter(Boolean).join(" · ") };
  }
  return { title: item.title, subtitle: item.year ? String(item.year) : "" };
}

function HistoryRow({
  entry,
  showUser,
  removing,
  onRemove,
}: {
  entry: HistoryEntry;
  showUser?: boolean;
  removing: boolean;
  onRemove: () => void;
}) {
  const { t } = useTranslation("components");
  const { item, user } = entry;
  const { title, subtitle } = describe(entry);
  return (
    <li className="group flex items-center gap-3 px-3 py-2.5">
      <Link to={itemUrl(item.id)} className="shrink-0" tabIndex={-1} aria-hidden>
        <Artwork
          candidates={posterCandidates(item)}
          width={40}
          alt=""
          title=""
          bare
          className={cn("w-10 rounded", item.kind === "track" ? "aspect-square" : "aspect-[2/3]")}
        />
      </Link>
      <div className="min-w-0 flex-1">
        <Link to={itemUrl(item.id)} className="block truncate text-sm font-medium transition-colors hover:text-accent">
          {title}
        </Link>
        <div className="truncate text-xs text-subtle">
          {subtitle ? <span>{subtitle} · </span> : null}
          <time dateTime={new Date(entry.viewed_at * 1000).toISOString()} title={formatTimestamp(entry.viewed_at)}>
            {formatRelative(entry.viewed_at)}
          </time>
          {entry.device ? <span> · {entry.device}</span> : null}
        </div>
      </div>
      {showUser ? (
        <div className="flex shrink-0 items-center gap-2 text-xs text-muted" title={user.name}>
          <Avatar user={{ ...user, avatar_url: user.avatar_url ?? null, avatar_color: user.avatar_color ?? null }} size="xs" />
          <span className="hidden max-w-28 truncate sm:inline">{user.name}</span>
        </div>
      ) : null}
      <IconButton
        size="sm"
        label={t("historyList.remove")}
        disabled={removing}
        onClick={onRemove}
        className="shrink-0 [@media(hover:hover)]:sm:opacity-0 sm:focus-visible:opacity-100 sm:group-hover:opacity-100"
      >
        <X className="size-4" />
      </IconButton>
    </li>
  );
}

/** Plays, newest first, with "Show more" and per-entry removal. */
export function HistoryList({
  scope,
  showUser,
  pageSize = 20,
  empty,
}: {
  scope: HistoryScope;
  /** Show who played each entry (everyone's history). */
  showUser?: boolean;
  pageSize?: number;
  empty: string;
}) {
  const { t } = useTranslation();
  const toast = useToast();
  const history = useHistory(scope, pageSize);
  const remove = useDeleteHistory();
  const entries = history.data?.pages.flatMap((p) => p.entries) ?? [];

  if (history.isPending) {
    return (
      <div className="space-y-2">
        {Array.from({ length: 3 }, (_, i) => (
          <Skeleton key={i} className="h-16" />
        ))}
      </div>
    );
  }
  if (history.isError) return <p className="text-sm text-danger">{errorMessage(history.error)}</p>;
  if (!entries.length) {
    return <p className="rounded-lg border border-dashed border-border px-4 py-6 text-center text-sm text-subtle">{empty}</p>;
  }
  return (
    <div>
      <ul className="divide-y divide-white/[0.05] rounded-md border border-border">
        {entries.map((entry) => (
          <HistoryRow
            key={entry.id}
            entry={entry}
            showUser={showUser}
            removing={remove.isPending && remove.variables === entry.id}
            onRemove={() => remove.mutate(entry.id, { onError: (err) => toast.error(errorMessage(err)) })}
          />
        ))}
      </ul>
      {history.hasNextPage ? (
        <Button
          variant="ghost"
          size="sm"
          className="mt-3"
          loading={history.isFetchingNextPage}
          onClick={() => void history.fetchNextPage()}
        >
          {t("actions.showMore")}
        </Button>
      ) : null}
    </div>
  );
}

import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Link, useNavigate, useParams } from "react-router";
import { AudioLines, ChevronDown, ChevronUp, GripVertical, Pencil, Play, Shuffle, Trash2, X } from "lucide-react";
import { errorMessage, isApiError } from "@/api/client";
import { usePlaylist, useDeletePlaylist, useRemoveFromPlaylist, useReorderPlaylist, type PlaylistDetail } from "@/api/lists";
import { useAudioPlayer } from "@/audio/AudioPlayer";
import { Artwork } from "@/components/Artwork";
import { defaultCardTitles } from "@/components/cards";
import { PlaylistArt } from "@/components/lists/PlaylistArt";
import { moved, useDragReorder } from "@/components/lists/reorder";
import { ConfirmModal } from "@/components/Modal";
import { PlayOn } from "@/components/PlayOn";
import { useToast } from "@/components/Toast";
import { Badge, Button, EmptyState, IconButton, Skeleton } from "@/components/ui";
import { cn } from "@/lib/cn";
import { formatDuration, progressFraction } from "@/lib/format";
import { useDocumentTitle } from "@/lib/hooks";
import { posterCandidates, thumbCandidates } from "@/lib/images";
import { useVideoQueue } from "@/lib/videoQueue";
import { NotFoundPage } from "../NotFoundPage";
import { describeSmartFilter } from "@/lib/smartFilter";
import { MetaLine } from "../item/shared";
import { PlaylistFormModal } from "./PlaylistFormModal";
import { playlistMeta } from "./PlaylistsPage";
import { itemUrl, playlistUrl } from "@/lib/urls";
import { idParam } from "@/lib/ids";

export function PlaylistPage() {
  const { t } = useTranslation(["item", "common"]);
  const { id } = useParams();
  const playlistId = idParam(id);
  const playlist = usePlaylist(playlistId);
  useDocumentTitle(playlist.data?.title);

  if (!Number.isFinite(playlistId) || playlistId <= 0) return <NotFoundPage />;
  if (playlist.isPending) return <PlaylistSkeleton />;
  if (playlist.isError) {
    if (isApiError(playlist.error, 404)) return <NotFoundPage />;
    return (
      <EmptyState
        className="min-h-[60vh]"
        title={t("playlist.loadError")}
        description={errorMessage(playlist.error)}
        action={<Button onClick={() => void playlist.refetch()}>{t("common:actions.retry")}</Button>}
      />
    );
  }
  return <PlaylistView key={playlist.data.id} playlist={playlist.data} />;
}

function PlaylistView({ playlist: p }: { playlist: PlaylistDetail }) {
  const { t } = useTranslation(["item", "common"]);
  const navigate = useNavigate();
  const toast = useToast();
  const audio = useAudioPlayer();
  const startQueue = useVideoQueue();
  const reorder = useReorderPlaylist();
  const remove = useRemoveFromPlaylist();
  const deletePlaylist = useDeletePlaylist();
  const [renaming, setRenaming] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const items = p.entries.map((e) => e.item);
  const video = p.kind === "video";
  // A smart playlist plays its filter's finds in their order.
  const editable = !p.smart_filter;

  const playFrom = (index: number, shuffle = false) => {
    if (!items.length) return;
    if (video) startQueue(items, { startIndex: index, shuffle, returnTo: playlistUrl(p.id) });
    else audio.playQueue(items, index, { shuffle });
  };
  const move = (from: number, to: number) => {
    if (to < 0 || to >= p.entries.length || from === to) return;
    reorder.mutate(
      { id: p.id, entryIds: moved(p.entries.map((e) => e.entry_id), from, to) },
      { onError: (err) => toast.error(errorMessage(err)) },
    );
  };
  const { dragging, rowProps, dropMark } = useDragReorder(p.entries.map((e) => e.entry_id), move);

  return (
    <div className="animate-fade-in px-4 pb-16 pt-8 sm:px-8">
      <div className="flex flex-col gap-6 sm:flex-row sm:items-end sm:gap-8">
        <PlaylistArt playlist={p} width={240} className="w-44 shrink-0 shadow-2xl shadow-black/60 sm:w-52 lg:w-60" />
        <div className="min-w-0 flex-1 space-y-3">
          <div className="flex items-center gap-2 text-sm font-medium text-muted">
            {video ? t("playlist.video") : t("playlist.music")}
            {p.smart_filter ? <Badge tone="accent">{t("smart")}</Badge> : null}
          </div>
          <h1 className="text-3xl font-bold leading-tight tracking-tight text-balance sm:text-5xl">{p.title}</h1>
          <MetaLine parts={[playlistMeta(p), ...(p.smart_filter ? describeSmartFilter(p.smart_filter) : [])]} />
          <div className="flex flex-wrap items-center gap-2.5 pt-1">
            <Button variant="primary" size="lg" icon={<Play className="size-5 fill-current" />} onClick={() => playFrom(0)} disabled={!items.length}>
              {t("common:actions.play")}
            </Button>
            <Button size="lg" icon={<Shuffle className="size-4" />} onClick={() => playFrom(0, true)} disabled={items.length < 2}>
              {t("shuffle")}
            </Button>
            {items.length ? <PlayOn kind="item" cast={() => ({ playlist_id: p.id })} /> : null}
            {items.length > 1 ? (
              <PlayOn kind="item" label={t("shuffleOn")} icon={<Shuffle className="size-4" />} cast={() => ({ playlist_id: p.id, shuffle: true })} />
            ) : null}
            <IconButton label={t("playlist.rename")} size="lg" className="bg-surface-3/80 hover:bg-border" onClick={() => setRenaming(true)}>
              <Pencil className="size-5" />
            </IconButton>
            <IconButton label={t("playlist.delete")} size="lg" className="bg-surface-3/80 hover:bg-border hover:text-danger!" onClick={() => setDeleting(true)}>
              <Trash2 className="size-5" />
            </IconButton>
          </div>
        </div>
      </div>

      <section className="mt-10 max-w-5xl">
        {p.entries.length === 0 && p.smart_filter ? (
          <p className="text-sm text-muted">{t("playlist.noMatches")}</p>
        ) : p.entries.length === 0 ? (
          <p className="text-sm text-muted">{video ? t("playlist.emptyVideo") : t("playlist.emptyMusic")}</p>
        ) : (
          <ol className="-mx-2 space-y-0.5" aria-label={t("playlist.items")}>
            {p.entries.map((entry, i) => {
              const item = entry.item;
              const { title, subtitle } = defaultCardTitles(item);
              const current = !video && audio.current?.id === item.id;
              const progress = video && !item.user_state.watched ? progressFraction(item) : 0;
              const mark = dropMark(i);
              return (
                <li
                  key={entry.entry_id}
                  {...(editable ? rowProps(i) : {})}
                  onDoubleClick={() => playFrom(i)}
                  className={cn(
                    "group flex items-center gap-3 rounded-md border-y-2 border-transparent px-2 py-1.5 transition-colors hover:bg-white/[0.04]",
                    current && "bg-white/[0.04]",
                    dragging === i && "opacity-40",
                    mark === "before" && "border-t-accent",
                    mark === "after" && "border-b-accent",
                  )}
                >
                  {editable ? <GripVertical className="hidden size-4 shrink-0 cursor-grab text-subtle sm:block" aria-hidden /> : null}
                  <span className={cn("w-6 shrink-0 text-right text-sm tabular-nums", current ? "text-accent" : "text-subtle")}>
                    {current && audio.playing ? <AudioLines className="ml-auto size-4 animate-pulse" /> : i + 1}
                  </span>
                  <button
                    type="button"
                    onClick={() => playFrom(i)}
                    aria-label={t("playTitle", { title })}
                    className={cn("relative shrink-0 overflow-hidden rounded ring-1 ring-white/5", video ? "w-24 sm:w-32" : "w-12")}
                  >
                    <Artwork
                      candidates={video ? thumbCandidates(item) : posterCandidates(item)}
                      width={video ? 128 : 48}
                      alt=""
                      title={title}
                      className={video ? "aspect-video w-full" : "aspect-square w-full"}
                    />
                    {progress > 0 ? (
                      <span className="absolute inset-x-0 bottom-0 h-0.5 bg-black/60">
                        <span className="block h-full bg-accent" style={{ width: `${progress * 100}%` }} />
                      </span>
                    ) : null}
                    <span className="absolute inset-0 flex items-center justify-center bg-black/45 opacity-0 transition-opacity group-hover:opacity-100">
                      <Play className="size-5 fill-current text-white" />
                    </span>
                  </button>
                  <div className="min-w-0 flex-1">
                    <Link
                      to={itemUrl(item.id)}
                      draggable={false}
                      className={cn("block truncate text-sm font-medium transition-colors hover:text-accent", current ? "text-accent" : "text-fg")}
                    >
                      {title}
                    </Link>
                    {subtitle ? <div className="truncate text-xs text-muted">{subtitle}</div> : null}
                  </div>
                  <span className="hidden shrink-0 text-xs tabular-nums text-muted sm:inline">{formatDuration(item.duration_ms)}</span>
                  <div
                    hidden={!editable}
                    className="flex shrink-0 items-center opacity-100 transition-opacity [@media(hover:hover)]:sm:opacity-0 sm:group-focus-within:opacity-100 sm:group-hover:opacity-100"
                  >
                    <IconButton label={t("moveUp")} size="sm" disabled={i === 0} onClick={() => move(i, i - 1)}>
                      <ChevronUp className="size-4" />
                    </IconButton>
                    <IconButton label={t("moveDown")} size="sm" disabled={i === p.entries.length - 1} onClick={() => move(i, i + 1)}>
                      <ChevronDown className="size-4" />
                    </IconButton>
                    <IconButton
                      label={t("playlist.remove")}
                      size="sm"
                      className="hover:text-danger!"
                      onClick={() => remove.mutate({ id: p.id, entryId: entry.entry_id }, { onError: (err) => toast.error(errorMessage(err)) })}
                    >
                      <X className="size-4" />
                    </IconButton>
                  </div>
                </li>
              );
            })}
          </ol>
        )}
      </section>

      <PlaylistFormModal key={renaming ? `rename${p.updated_at}` : "closed"} open={renaming} onClose={() => setRenaming(false)} playlist={p} />
      <ConfirmModal
        open={deleting}
        onClose={() => setDeleting(false)}
        title={t("playlist.deleteTitle")}
        message={t("playlist.deleteMessage", { title: p.title })}
        confirmLabel={t("common:actions.delete")}
        danger
        loading={deletePlaylist.isPending}
        onConfirm={() =>
          deletePlaylist.mutate(p.id, {
            onSuccess: () => {
              toast.success(t("playlist.deleted"));
              void navigate("/playlists", { replace: true });
            },
            onError: (err) => toast.error(errorMessage(err)),
          })
        }
      />
    </div>
  );
}

function PlaylistSkeleton() {
  return (
    <div className="px-4 pb-16 pt-8 sm:px-8">
      <div className="flex flex-col gap-6 sm:flex-row sm:items-end sm:gap-8">
        <Skeleton className="aspect-square w-44 shrink-0 sm:w-52 lg:w-60" />
        <div className="flex-1 space-y-3">
          <Skeleton className="h-3 w-24" />
          <Skeleton className="h-10 w-2/3" />
          <Skeleton className="h-4 w-1/4" />
          <Skeleton className="h-12 w-56" />
        </div>
      </div>
      <div className="mt-10 max-w-5xl space-y-2">
        {Array.from({ length: 6 }, (_, i) => (
          <Skeleton key={i} className="h-16" />
        ))}
      </div>
    </div>
  );
}

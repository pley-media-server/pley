import { useState, type FormEvent } from "react";
import { Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { playlistKindOf, useAddToPlaylist, useCreatePlaylist, usePlaylists, type Playlist } from "@/api/lists";
import type { ItemSummary } from "@/api/types";
import { Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Button, Skeleton, TextInput } from "@/components/ui";
import { formatDuration, fullTitle } from "@/lib/format";
import { PlaylistArt } from "./PlaylistArt";

/** Pick (or start) a playlist for an item; shows, seasons, albums and artists add all their items. */
export function AddToPlaylistModal({ item, onClose }: { item: ItemSummary | null; onClose: () => void }) {
  const { t } = useTranslation(["components", "common"]);
  const open = item != null;
  const kind = item ? playlistKindOf(item.kind) : "video";
  const playlists = usePlaylists(open);
  const create = useCreatePlaylist();
  const add = useAddToPlaylist();
  const toast = useToast();
  const [title, setTitle] = useState("");
  // Smart playlists fill themselves.
  const matching = playlists.data?.filter((p) => p.kind === kind && !p.smart_filter) ?? [];
  const busy = create.isPending || add.isPending;

  const onCreate = (e: FormEvent) => {
    e.preventDefault();
    const name = title.trim();
    if (!item || !name) return;
    create.mutate(
      { title: name, kind, item_ids: [item.id] },
      {
        onSuccess: (p) => {
          toast.success(t("lists.addedTo", { title: p.title }));
          onClose();
        },
        onError: (err) => toast.error(errorMessage(err)),
      },
    );
  };

  const onAdd = (p: Playlist) => {
    if (!item) return;
    add.mutate(
      { id: p.id, itemIds: [item.id] },
      {
        onSuccess: ({ added }) => {
          if (added === 0) toast.info(t("addToPlaylist.nothingToAdd"));
          else toast.success(added === 1 ? t("lists.addedTo", { title: p.title }) : t("addToPlaylist.addedItems", { count: added, title: p.title }));
          onClose();
        },
        onError: (err) => toast.error(errorMessage(err)),
      },
    );
  };

  return (
    <Modal open={open} onClose={onClose} title={t("addToPlaylist.title")} size="sm" locked={busy}>
      {item ? <p className="-mt-1 mb-3 truncate text-sm text-muted">{fullTitle(item)}</p> : null}
      <form onSubmit={onCreate} className="flex gap-2">
        <TextInput
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={kind === "audio" ? t("addToPlaylist.newMusic") : t("addToPlaylist.newVideo")}
          aria-label={t("addToPlaylist.nameLabel")}
          maxLength={200}
        />
        <Button type="submit" variant="primary" icon={<Plus className="size-4" />} disabled={!title.trim() || add.isPending} loading={create.isPending}>
          {t("common:actions.create")}
        </Button>
      </form>
      <div className="mt-4">
        {playlists.isPending ? (
          <div className="space-y-2">
            <Skeleton className="h-12" />
            <Skeleton className="h-12" />
          </div>
        ) : playlists.isError ? (
          <p className="text-sm text-danger">{errorMessage(playlists.error)}</p>
        ) : matching.length === 0 ? (
          <p className="py-2 text-sm text-muted">{kind === "audio" ? t("addToPlaylist.emptyMusic") : t("addToPlaylist.emptyVideo")}</p>
        ) : (
          <ul className="-mx-2 max-h-72 overflow-y-auto">
            {matching.map((p) => (
              <li key={p.id}>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => onAdd(p)}
                  className="flex w-full items-center gap-3 rounded-md px-2 py-2 text-left transition-colors hover:bg-white/5 disabled:opacity-50"
                >
                  <PlaylistArt playlist={p} width={44} className="size-11 shrink-0" />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-medium text-fg">{p.title}</span>
                    <span className="block text-xs text-muted">
                      {[t("lists.items", { count: p.item_count }), formatDuration(p.duration_ms)].filter(Boolean).join(" · ")}
                    </span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </Modal>
  );
}

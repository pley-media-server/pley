import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { ListMusic, ListVideo } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useCreatePlaylist, useRenamePlaylist, type Playlist, type PlaylistDetail, type PlaylistKind } from "@/api/lists";
import { Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Button, Field, TextInput } from "@/components/ui";
import { cn } from "@/lib/cn";

/** Start a playlist, or rename one. */
export function PlaylistFormModal({
  open,
  onClose,
  playlist,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  /** The playlist to rename; a new one is created without it. */
  playlist?: Playlist;
  onCreated?: (created: PlaylistDetail) => void;
}) {
  const { t } = useTranslation(["item", "common"]);
  const create = useCreatePlaylist();
  const rename = useRenamePlaylist();
  const toast = useToast();
  const [title, setTitle] = useState(playlist?.title ?? "");
  const [kind, setKind] = useState<PlaylistKind>("video");
  const busy = create.isPending || rename.isPending;

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    const name = title.trim();
    if (!name) return;
    const onError = (err: unknown) => toast.error(errorMessage(err));
    if (playlist) {
      rename.mutate({ id: playlist.id, title: name }, { onSuccess: onClose, onError });
    } else {
      create.mutate(
        { title: name, kind },
        {
          onSuccess: (created) => {
            onClose();
            onCreated?.(created);
          },
          onError,
        },
      );
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={playlist ? t("playlist.rename") : t("playlistForm.new")}
      size="sm"
      locked={busy}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" type="submit" form="playlist-form" loading={busy} disabled={!title.trim()}>
            {playlist ? t("common:actions.save") : t("common:actions.create")}
          </Button>
        </>
      }
    >
      <form id="playlist-form" onSubmit={onSubmit} className="space-y-4">
        <Field label={t("playlistForm.title")} htmlFor="playlist-title">
          <TextInput id="playlist-title" value={title} onChange={(e) => setTitle(e.target.value)} maxLength={200} required />
        </Field>
        {playlist ? null : (
          <div className="space-y-1.5">
            <span className="block text-sm font-medium text-fg">{t("playlistForm.for")}</span>
            <div className="grid grid-cols-2 gap-2" role="radiogroup" aria-label={t("playlistForm.kind")}>
              {(
                [
                  ["video", t("playlistForm.video"), ListVideo],
                  ["audio", t("playlistForm.audio"), ListMusic],
                ] as const
              ).map(([value, label, Icon]) => (
                <button
                  key={value}
                  type="button"
                  role="radio"
                  aria-checked={kind === value}
                  onClick={() => setKind(value)}
                  className={cn(
                    "flex h-11 items-center justify-center gap-2 rounded-md border text-sm font-medium transition-colors",
                    kind === value ? "border-accent bg-accent/10 text-fg" : "border-border text-muted hover:border-border-strong hover:text-fg",
                  )}
                >
                  <Icon className="size-4" /> {label}
                </button>
              ))}
            </div>
          </div>
        )}
      </form>
    </Modal>
  );
}

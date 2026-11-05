import { useState, type FormEvent } from "react";
import { useNavigate } from "react-router";
import { useTranslation } from "react-i18next";
import { Layers, ListVideo } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useCreateCollection, useCreatePlaylist } from "@/api/lists";
import type { Id } from "@/api/types";
import { Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Button, Field, TextInput } from "@/components/ui";
import { cn } from "@/lib/cn";
import { describeSmartFilter } from "@/lib/smartFilter";
import { collectionUrl, playlistUrl } from "@/lib/urls";

type Target = "playlist" | "collection";

/** Each with its words in `saveFilters.targets`. */
const TARGETS: { value: Target; icon: typeof Layers }[] = [
  { value: "playlist", icon: ListVideo },
  { value: "collection", icon: Layers },
];

/** Save a library's filters as a smart playlist, or (administrators) a smart collection. */
export function SaveFiltersModal({
  onClose,
  libraryId,
  filter,
  allowCollection,
}: {
  onClose: () => void;
  libraryId: Id;
  /** The library's filters in Plex's syntax (`lib/smartFilter`). */
  filter: string;
  allowCollection: boolean;
}) {
  const { t } = useTranslation(["pages", "common"]);
  const navigate = useNavigate();
  const toast = useToast();
  const playlist = useCreatePlaylist();
  const collection = useCreateCollection();
  const [title, setTitle] = useState("");
  const [target, setTarget] = useState<Target>("playlist");
  const busy = playlist.isPending || collection.isPending;
  const onError = (err: unknown) => toast.error(errorMessage(err));

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    const name = title.trim();
    if (!name) return;
    if (target === "collection") {
      collection.mutate(
        { libraryId, title: name, smart_filter: filter },
        { onSuccess: (c) => (onClose(), void navigate(collectionUrl(c.id))), onError },
      );
    } else {
      playlist.mutate(
        { title: name, library_id: libraryId, smart_filter: filter },
        { onSuccess: (p) => (onClose(), void navigate(playlistUrl(p.id))), onError },
      );
    }
  };

  return (
    <Modal
      open
      onClose={onClose}
      title={t("saveFilters.title")}
      size="sm"
      locked={busy}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" type="submit" form="save-filters" loading={busy} disabled={!title.trim()}>
            {t("common:actions.save")}
          </Button>
        </>
      }
    >
      <form id="save-filters" onSubmit={onSubmit} className="space-y-4">
        <p className="text-sm text-muted">{t("saveFilters.describe", { filters: describeSmartFilter(filter).join(" · ") })}</p>
        <Field label={t("saveFilters.titleField")} htmlFor="save-filters-title">
          <TextInput id="save-filters-title" value={title} onChange={(e) => setTitle(e.target.value)} maxLength={200} required />
        </Field>
        {allowCollection ? (
          <div className="space-y-1.5">
            <span className="block text-sm font-medium text-fg">{t("saveFilters.saveAs")}</span>
            <div className="grid gap-2" role="radiogroup" aria-label={t("saveFilters.saveAs")}>
              {TARGETS.map(({ value, icon: Icon }) => (
                <button
                  key={value}
                  type="button"
                  role="radio"
                  aria-checked={target === value}
                  onClick={() => setTarget(value)}
                  className={cn(
                    "flex items-start gap-3 rounded-md border px-3 py-2.5 text-left transition-colors",
                    target === value ? "border-accent bg-accent/10" : "border-border hover:border-border-strong",
                  )}
                >
                  <Icon className="mt-0.5 size-4 shrink-0 text-accent" />
                  <span>
                    <span className="block text-sm font-medium text-fg">{t(`saveFilters.targets.${value}.label`)}</span>
                    <span className="block text-xs text-muted">{t(`saveFilters.targets.${value}.hint`)}</span>
                  </span>
                </button>
              ))}
            </div>
          </div>
        ) : null}
      </form>
    </Modal>
  );
}

import { useState, type FormEvent } from "react";
import { Check, Plus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { useAddToCollection, useCollections, useCreateCollection, useItemCollections, type Collection } from "@/api/lists";
import type { ItemSummary } from "@/api/types";
import { Artwork } from "@/components/Artwork";
import { Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Badge, Button, Skeleton, TextInput } from "@/components/ui";
import { fullTitle } from "@/lib/format";

/** Administrators: put a movie or show in a collection of its library (or a new one). */
export function AddToCollectionModal({ item, onClose }: { item: ItemSummary | null; onClose: () => void }) {
  const { t } = useTranslation(["components", "common"]);
  const open = item != null;
  const collections = useCollections(item?.library_id ?? 0, open);
  const memberOf = useItemCollections(item?.id ?? 0, open);
  const create = useCreateCollection();
  const add = useAddToCollection();
  const toast = useToast();
  const [title, setTitle] = useState("");
  const busy = create.isPending || add.isPending;
  const inside = new Set(memberOf.data?.map((c) => c.id) ?? []);

  const onCreate = (e: FormEvent) => {
    e.preventDefault();
    const name = title.trim();
    if (!item || !name) return;
    create.mutate(
      { libraryId: item.library_id, title: name, item_ids: [item.id] },
      {
        onSuccess: (c) => {
          toast.success(t("lists.addedTo", { title: c.title }));
          onClose();
        },
        onError: (err) => toast.error(errorMessage(err)),
      },
    );
  };

  const onAdd = (c: Collection) => {
    if (!item) return;
    add.mutate(
      { collection: c, itemIds: [item.id] },
      {
        onSuccess: () => {
          toast.success(t("lists.addedTo", { title: c.title }));
          onClose();
        },
        onError: (err) => toast.error(errorMessage(err)),
      },
    );
  };

  return (
    <Modal open={open} onClose={onClose} title={t("addToCollection.title")} size="sm" locked={busy}>
      {item ? <p className="-mt-1 mb-3 truncate text-sm text-muted">{fullTitle(item)}</p> : null}
      <form onSubmit={onCreate} className="flex gap-2">
        <TextInput
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder={t("addToCollection.placeholder")}
          aria-label={t("addToCollection.nameLabel")}
          maxLength={200}
        />
        <Button type="submit" variant="primary" icon={<Plus className="size-4" />} disabled={!title.trim() || add.isPending} loading={create.isPending}>
          {t("common:actions.create")}
        </Button>
      </form>
      <div className="mt-4">
        {collections.isPending ? (
          <div className="space-y-2">
            <Skeleton className="h-14" />
            <Skeleton className="h-14" />
          </div>
        ) : collections.isError ? (
          <p className="text-sm text-danger">{errorMessage(collections.error)}</p>
        ) : collections.data.filter((c) => !c.smart_filter).length === 0 ? (
          <p className="py-2 text-sm text-muted">{t("addToCollection.empty")}</p>
        ) : (
          <ul className="-mx-2 max-h-80 overflow-y-auto">
            {/* Smart collections fill themselves. */}
            {collections.data.filter((c) => !c.smart_filter).map((c) => {
              const member = inside.has(c.id);
              return (
                <li key={c.id}>
                  <button
                    type="button"
                    disabled={busy || member}
                    onClick={() => onAdd(c)}
                    className="flex w-full items-center gap-3 rounded-md px-2 py-1.5 text-left transition-colors enabled:hover:bg-white/5 disabled:opacity-60"
                  >
                    <Artwork candidates={[c.poster_url]} width={36} alt="" title="" bare className="aspect-[2/3] w-9 shrink-0 rounded" />
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-sm font-medium text-fg">{c.title}</span>
                      <span className="flex items-center gap-1.5 text-xs text-muted">
                        {t("lists.items", { count: c.item_count })}
                        {c.automatic ? <Badge className="px-1 py-0 text-[10px]">TMDB</Badge> : null}
                      </span>
                    </span>
                    {member ? <Check className="size-4 shrink-0 text-accent" aria-label={t("addToCollection.alreadyIn")} /> : null}
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </Modal>
  );
}

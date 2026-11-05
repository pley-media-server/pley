import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { useCreateCollection, useUpdateCollection, type Collection, type CollectionDetail, type CollectionSort } from "@/api/lists";
import type { Id } from "@/api/types";
import { Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Button, Field, Select, TextInput } from "@/components/ui";

const SORTS: CollectionSort[] = ["release", "title", "custom"];

/** Create a collection in a library, or edit one (administrators). */
export function CollectionFormModal({
  open,
  onClose,
  libraryId,
  collection,
  onCreated,
}: {
  open: boolean;
  onClose: () => void;
  libraryId: Id;
  /** The collection to edit; a new one is created without it. */
  collection?: Collection;
  onCreated?: (created: CollectionDetail) => void;
}) {
  const { t } = useTranslation(["item", "common"]);
  const create = useCreateCollection();
  const update = useUpdateCollection();
  const toast = useToast();
  const [title, setTitle] = useState(collection?.title ?? "");
  const [summary, setSummary] = useState(collection?.summary ?? "");
  const [sort, setSort] = useState<CollectionSort>(collection?.sort ?? "release");
  const busy = create.isPending || update.isPending;

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    const name = title.trim();
    const text = summary.trim() || null;
    if (!name) return;
    if (collection) {
      // Only what changed: an edited title or summary is kept over TMDB's.
      update.mutate(
        {
          id: collection.id,
          title: name !== collection.title ? name : undefined,
          summary: text !== collection.summary ? text : undefined,
          sort: sort !== collection.sort ? sort : undefined,
        },
        {
          onSuccess: () => {
            toast.success(t("collectionForm.saved"));
            onClose();
          },
          onError: (err) => toast.error(errorMessage(err)),
        },
      );
    } else {
      create.mutate(
        { libraryId, title: name, summary: text },
        {
          onSuccess: (created) => {
            onClose();
            onCreated?.(created);
          },
          onError: (err) => toast.error(errorMessage(err)),
        },
      );
    }
  };

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={collection ? t("collection.edit") : t("collectionForm.new")}
      locked={busy}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={busy}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" type="submit" form="collection-form" loading={busy} disabled={!title.trim()}>
            {collection ? t("common:actions.save") : t("common:actions.create")}
          </Button>
        </>
      }
    >
      <form id="collection-form" onSubmit={onSubmit} className="space-y-4">
        <Field label={t("collectionForm.title")} htmlFor="collection-title">
          <TextInput id="collection-title" value={title} onChange={(e) => setTitle(e.target.value)} maxLength={200} required />
        </Field>
        <Field label={t("collectionForm.summary")} htmlFor="collection-summary">
          <textarea
            id="collection-summary"
            rows={4}
            value={summary}
            onChange={(e) => setSummary(e.target.value)}
            className="w-full resize-y rounded-md border border-border bg-surface-2 px-3 py-2 text-sm text-fg placeholder:text-subtle transition-colors focus:border-accent focus:outline-none"
          />
        </Field>
        {collection && !collection.smart_filter ? (
          <Field
            label={t("collectionForm.order")}
            htmlFor="collection-sort"
            hint={collection.automatic ? t("collectionForm.orderHint") : undefined}
          >
            <Select id="collection-sort" value={sort} onChange={(e) => setSort(e.target.value as CollectionSort)}>
              {SORTS.map((s) => (
                <option key={s} value={s}>
                  {t(`collectionForm.sorts.${s}`)}
                </option>
              ))}
            </Select>
          </Field>
        ) : null}
      </form>
    </Modal>
  );
}

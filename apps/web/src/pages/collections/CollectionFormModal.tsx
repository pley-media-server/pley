import { useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import {
  useCategories,
  useCreateCollection,
  useUpdateCollection,
  type Collection,
  type CollectionDetail,
  type CollectionSort,
} from "@/api/lists";
import type { Id } from "@/api/types";
import { Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Button, Field, Select, TextInput, Toggle } from "@/components/ui";

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
  const categories = useCategories(open);
  // A new one starts under Other, as the server lists one without a category.
  const [category, setCategory] = useState(collection ? String(collection.category.id) : "");
  const [onHome, setOnHome] = useState(collection ? collection.home_position != null : false);
  const [onRecommended, setOnRecommended] = useState(collection ? collection.recommended_position != null : false);
  const busy = create.isPending || update.isPending;
  const other = categories.data?.find((c) => c.key === "other");
  const chosen = category || (other ? String(other.id) : "");
  const categoryId = chosen ? Number(chosen) : null;

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
          category_id: categoryId !== collection.category.id ? categoryId : undefined,
          on_home: onHome !== (collection.home_position != null) ? onHome : undefined,
          on_recommended: onRecommended !== (collection.recommended_position != null) ? onRecommended : undefined,
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
        { libraryId, title: name, summary: text, category_id: categoryId },
        {
          onSuccess: (created) => {
            // Rows are set apart from making it, before the form closes: a
            // failure is said while something is still here to say it.
            const rows =
              onHome || onRecommended
                ? update
                    .mutateAsync({ id: created.id, on_home: onHome || undefined, on_recommended: onRecommended || undefined })
                    .then(
                      () => undefined,
                      (err: unknown) => toast.error(errorMessage(err)),
                    )
                : Promise.resolve();
            void rows.then(() => {
              onClose();
              onCreated?.(created);
            });
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
        <Field label={t("collectionForm.category")} htmlFor="collection-category" hint={t("collectionForm.categoryHint")}>
          <Select id="collection-category" className="w-full" value={chosen} onChange={(e) => setCategory(e.target.value)}>
            {(categories.data ?? []).map((c) => (
              <option key={c.id} value={String(c.id)}>
                {c.title}
              </option>
            ))}
          </Select>
        </Field>
        <div className="space-y-3">
          <RowToggle id="collection-on-recommended" label={t("collectionForm.onRecommended")} checked={onRecommended} onChange={setOnRecommended} />
          <RowToggle id="collection-on-home" label={t("collectionForm.onHome")} checked={onHome} onChange={setOnHome} />
        </div>
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

function RowToggle({ id, label, checked, onChange }: { id: string; label: string; checked: boolean; onChange: (on: boolean) => void }) {
  return (
    <div className="flex items-center justify-between gap-4">
      <label htmlFor={id} className="text-sm font-medium text-fg">
        {label}
      </label>
      <Toggle id={id} label={label} checked={checked} onChange={onChange} />
    </div>
  );
}

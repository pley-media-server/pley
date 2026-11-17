import { useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, ChevronUp, GripVertical, Plus, RotateCcw, X } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useLibraries } from "@/api/hooks";
import {
  useCollections,
  useHomeRows,
  useOrderHomeRows,
  useOrderRecommended,
  useRestoreDefaultCollections,
  useUpdateCollection,
  type Collection,
} from "@/api/lists";
import type { Id } from "@/api/types";
import { moved, useDragReorder, useRowFocus, type RowButton } from "@/components/lists/reorder";
import { Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Button, IconButton, Select, Spinner } from "@/components/ui";
import { cn } from "@/lib/cn";

/** A row to arrange: what it's called, what it shows, and whether it can be taken off. */
interface Row {
  key: string;
  title: string;
  detail?: string;
  removable: boolean;
}

/** Rows dragged by a grip or moved a place with their arrows, each move saved at once. */
function RowList({
  label,
  rows,
  onMove,
  onRemove,
}: {
  label: string;
  rows: Row[];
  onMove: (from: number, to: number) => void;
  onRemove: (row: Row) => void;
}) {
  const { t } = useTranslation("item");
  const keys = rows.map((r) => r.key);
  const listRef = useRef<HTMLOListElement>(null);
  const pressed = useRowFocus(listRef, keys);
  // Read out to screen readers after each move.
  const [note, setNote] = useState("");
  const move = (from: number, to: number) => {
    const row = rows[from];
    if (!row || to < 0 || to >= rows.length || to === from) return;
    setNote(t("rows.moved", { title: row.title, position: to + 1, total: rows.length }));
    onMove(from, to);
  };
  const drag = useDragReorder(keys, (from, to) => {
    pressed(null);
    move(from, to);
  });
  const press = (row: Row, index: number, button: RowButton) => {
    pressed({ key: row.key, index, button });
    if (button === "remove") onRemove(row);
    else move(index, button === "up" ? index - 1 : index + 1);
  };
  return (
    <>
      <ol ref={listRef} aria-label={label} className="space-y-2" {...drag.listProps}>
        {rows.map((row, i) => {
          const mark = drag.dropMark(i);
          return (
            <li
              key={row.key}
              data-row={row.key}
              {...drag.rowProps(i, { handle: true })}
              className={cn("relative transition-opacity", drag.dragging === i && "opacity-40")}
            >
              {mark ? (
                <span aria-hidden className={cn("absolute inset-x-0 flex h-2 items-center", mark === "before" ? "-top-2" : "-bottom-2")}>
                  <span className="h-0.5 w-full rounded-full bg-accent" />
                </span>
              ) : null}
              <div className="flex items-center gap-2 rounded-lg border border-border bg-surface-2 py-1.5 pl-1.5 pr-1">
                <span
                  draggable
                  data-drag-handle
                  title={t("rows.drag")}
                  aria-hidden
                  className="shrink-0 cursor-grab rounded p-1 text-subtle transition-colors hover:text-fg active:cursor-grabbing"
                >
                  <GripVertical className="size-4" />
                </span>
                <div className="min-w-0 flex-1">
                  <div className="truncate text-sm font-medium">{row.title}</div>
                  {row.detail ? <div className="truncate text-xs text-muted">{row.detail}</div> : null}
                </div>
                <IconButton
                  label={t("rows.moveUp", { title: row.title })}
                  size="sm"
                  data-row-button="up"
                  disabled={i === 0}
                  onClick={() => press(row, i, "up")}
                >
                  <ChevronUp className="size-4" />
                </IconButton>
                <IconButton
                  label={t("rows.moveDown", { title: row.title })}
                  size="sm"
                  data-row-button="down"
                  disabled={i === rows.length - 1}
                  onClick={() => press(row, i, "down")}
                >
                  <ChevronDown className="size-4" />
                </IconButton>
                <IconButton
                  label={t("rows.remove", { title: row.title })}
                  size="sm"
                  className="hover:text-danger!"
                  data-row-button="remove"
                  disabled={!row.removable}
                  onClick={() => press(row, i, "remove")}
                >
                  <X className="size-4" />
                </IconButton>
              </div>
            </li>
          );
        })}
      </ol>
      <p role="status" className="sr-only">
        {note}
      </p>
    </>
  );
}

/** The list once it's loaded; a spinner until then, and the error with Retry if it fails. */
function Loaded({
  query,
  empty,
  children,
}: {
  query: { isPending: boolean; isError: boolean; error: unknown; refetch: () => unknown };
  empty: boolean;
  children: ReactNode;
}) {
  const { t } = useTranslation(["item", "common"]);
  if (query.isPending) return <Spinner className="mx-auto my-4 block" />;
  if (query.isError) {
    return (
      <div className="flex items-center gap-3 text-sm text-muted">
        <span className="min-w-0 flex-1">{errorMessage(query.error)}</span>
        <Button size="sm" onClick={() => void query.refetch()}>
          {t("common:actions.retry")}
        </Button>
      </div>
    );
  }
  return empty ? <p className="text-sm text-muted">{t("rows.noRows")}</p> : children;
}

function RowsModal({
  open,
  onClose,
  title,
  hint,
  children,
  footer,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  hint: string;
  children: ReactNode;
  footer?: ReactNode;
}) {
  const { t } = useTranslation("common");
  return (
    <Modal
      open={open}
      onClose={onClose}
      title={title}
      size="sm"
      footer={
        <>
          {footer}
          <Button variant="primary" onClick={onClose} data-autofocus>
            {t("actions.done")}
          </Button>
        </>
      }
    >
      <p className="mb-4 text-sm text-muted">{hint}</p>
      {children}
    </Modal>
  );
}

/** The collections on a library's Recommended tab, in their order. */
export function recommendedRows(collections: Collection[]): Collection[] {
  return collections
    .filter((c) => c.recommended_position != null)
    .sort((a, b) => (a.recommended_position ?? 0) - (b.recommended_position ?? 0));
}

/** Administrators: which collections are rows on a library's Recommended tab, and in what order. */
export function RecommendedRowsModal({ open, onClose, libraryId }: { open: boolean; onClose: () => void; libraryId: Id }) {
  const { t } = useTranslation("item");
  const collections = useCollections(libraryId, open);
  const order = useOrderRecommended();
  const update = useUpdateCollection();
  const restore = useRestoreDefaultCollections();
  const toast = useToast();
  const [adding, setAdding] = useState("");
  const all = collections.data ?? [];
  const shown = recommendedRows(all);
  const others = all.filter((c) => c.recommended_position == null);
  const rows: Row[] = shown.map((c) => ({ key: String(c.id), title: c.title, detail: c.category.key === "other" ? undefined : c.category.title, removable: true }));
  const onError = (e: unknown) => toast.error(errorMessage(e));

  return (
    <RowsModal
      open={open}
      onClose={onClose}
      title={t("rows.recommendedTitle")}
      hint={t("rows.recommendedHint")}
      footer={
        <Button
          variant="ghost"
          className="mr-auto"
          icon={<RotateCcw className="size-4" />}
          loading={restore.isPending}
          onClick={() =>
            restore.mutate(libraryId, {
              onSuccess: ({ restored }) =>
                restored
                  ? toast.success(t("rows.restored", { count: restored }))
                  : toast.info(t("rows.nothingToRestore")),
              onError,
            })
          }
        >
          {t("rows.restore")}
        </Button>
      }
    >
      <Loaded query={collections} empty={!rows.length}>
        <RowList
          label={t("rows.recommendedTitle")}
          rows={rows}
          onMove={(from, to) => {
            if (to < 0 || to >= rows.length || from === to) return;
            order.mutate({ libraryId, ids: moved(shown.map((c) => c.id), from, to) }, { onError });
          }}
          onRemove={(row) => update.mutate({ id: Number(row.key), on_recommended: false }, { onError })}
        />
      </Loaded>
      {others.length ? (
        <div className="mt-4 flex items-center gap-2">
          <Select aria-label={t("rows.add")} className="min-w-0 flex-1" value={adding} onChange={(e) => setAdding(e.target.value)}>
            <option value="">{t("rows.pick")}</option>
            {others.map((c) => (
              <option key={c.id} value={String(c.id)}>
                {c.title}
              </option>
            ))}
          </Select>
          <Button
            icon={<Plus className="size-4" />}
            disabled={!adding}
            loading={update.isPending}
            onClick={() => update.mutate({ id: Number(adding), on_recommended: true }, { onSuccess: () => setAdding(""), onError })}
          >
            {t("rows.add")}
          </Button>
        </div>
      ) : null}
    </RowsModal>
  );
}

/** Administrators: Home's rows after Continue Watching and My List, the same for everyone. */
export function HomeRowsModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation("item");
  const homeRows = useHomeRows(open);
  const libs = useLibraries();
  const order = useOrderHomeRows();
  const update = useUpdateCollection();
  const toast = useToast();
  const list = homeRows.data ?? [];
  const libraryName = (id: Id) => libs.data?.find((l) => l.id === id)?.name;
  const rows: Row[] = list.map((r) => ({
    key: r.key,
    title: r.title,
    // An untouched Recently Added is titled "Recently Added in <library>"
    // by the server already.
    detail: r.key.startsWith("recently_added:")
      ? ""
      : r.library_ids
          .map(libraryName)
          .filter((n): n is string => !!n)
          .join(", "),
    // A music or audiobook library's Recently Added is no collection: it stays.
    removable: r.collection_ids.length > 0,
  }));
  const onError = (e: unknown) => toast.error(errorMessage(e));

  return (
    <RowsModal open={open} onClose={onClose} title={t("rows.homeTitle")} hint={t("rows.homeHint")}>
      <Loaded query={homeRows} empty={!rows.length}>
        <RowList
          label={t("rows.homeTitle")}
          rows={rows}
          onMove={(from, to) => {
            if (to < 0 || to >= rows.length || from === to) return;
            order.mutate(moved(list.map((r) => r.key), from, to), { onError });
          }}
          onRemove={(row) => {
            // A row of several libraries' collections leaves Home with them
            // all; one failure is said once, whichever it was.
            const ids = list.find((r) => r.key === row.key)?.collection_ids ?? [];
            Promise.all(ids.map((id) => update.mutateAsync({ id, on_home: false }))).catch(onError);
          }}
        />
      </Loaded>
    </RowsModal>
  );
}

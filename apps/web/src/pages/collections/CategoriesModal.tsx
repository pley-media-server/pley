import { useEffect, useRef, useState, type FormEvent } from "react";
import { useTranslation } from "react-i18next";
import { Check, ChevronDown, ChevronUp, GripVertical, Pencil, Plus, Trash2, X } from "lucide-react";
import { errorMessage } from "@/api/client";
import {
  useCategories,
  useCreateCategory,
  useDeleteCategory,
  useOrderCategories,
  useRenameCategory,
  type Category,
} from "@/api/lists";
import { moved, useDragReorder, useRowFocus, type RowButton } from "@/components/lists/reorder";
import { ConfirmModal, Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Badge, Button, IconButton, TextInput } from "@/components/ui";
import { cn } from "@/lib/cn";

/** The longest name a category takes, as the server says. */
const MAX_TITLE = 100;

/**
 * Administrators: the categories collections are grouped in, shared by every
 * library. Pley's own (Highlights, Franchises) can be moved, not renamed or
 * deleted; a deleted one's collections go to Other.
 */
export function CategoriesModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation(["item", "common"]);
  const categories = useCategories(open);
  const order = useOrderCategories();
  const create = useCreateCategory();
  const toast = useToast();
  const [title, setTitle] = useState("");
  const [deleting, setDeleting] = useState<Category | null>(null);
  const remove = useDeleteCategory();
  const list = categories.data ?? [];
  const listRef = useRef<HTMLOListElement>(null);
  const pressed = useRowFocus(listRef, list.map((c) => String(c.id)));
  // Read out to screen readers after each move.
  const [note, setNote] = useState("");

  const move = (from: number, to: number) => {
    const category = list[from];
    if (!category || to < 0 || to >= list.length || from === to) return;
    setNote(t("categories.moved", { title: category.title, position: to + 1, total: list.length }));
    order.mutate(moved(list.map((c) => c.id), from, to), { onError: (e) => toast.error(errorMessage(e)) });
  };
  const drag = useDragReorder(list.map((c) => c.id), (from, to) => {
    pressed(null);
    move(from, to);
  });
  const press = (category: Category, index: number, button: RowButton) => {
    pressed({ key: String(category.id), index, button });
    if (button === "remove") setDeleting(category);
    else move(index, button === "up" ? index - 1 : index + 1);
  };

  const onAdd = (e: FormEvent) => {
    e.preventDefault();
    const name = title.trim();
    if (!name) return;
    create.mutate(name, {
      onSuccess: () => setTitle(""),
      onError: (err) => toast.error(errorMessage(err)),
    });
  };

  return (
    <>
      <Modal
        open={open}
        onClose={onClose}
        title={t("categories.title")}
        size="sm"
        footer={
          <Button variant="primary" onClick={onClose} data-autofocus>
            {t("common:actions.done")}
          </Button>
        }
      >
        <p className="mb-4 text-sm text-muted">{t("categories.hint")}</p>
        <ol ref={listRef} aria-label={t("categories.title")} className="space-y-2" {...drag.listProps}>
          {list.map((c, i) => {
            const mark = drag.dropMark(i);
            return (
              <li
                key={c.id}
                data-row={c.id}
                {...drag.rowProps(i, { handle: true })}
                className={cn("relative transition-opacity", drag.dragging === i && "opacity-40")}
              >
                {mark ? (
                  <span aria-hidden className={cn("absolute inset-x-0 flex h-2 items-center", mark === "before" ? "-top-2" : "-bottom-2")}>
                    <span className="h-0.5 w-full rounded-full bg-accent" />
                  </span>
                ) : null}
                <CategoryRow category={c} first={i === 0} last={i === list.length - 1} onPress={(button) => press(c, i, button)} />
              </li>
            );
          })}
        </ol>
        <p role="status" className="sr-only">
          {note}
        </p>
        <form onSubmit={onAdd} className="mt-4 flex items-center gap-2">
          <TextInput
            aria-label={t("categories.newName")}
            placeholder={t("categories.newName")}
            value={title}
            maxLength={MAX_TITLE}
            onChange={(e) => setTitle(e.target.value)}
          />
          <Button type="submit" icon={<Plus className="size-4" />} loading={create.isPending} disabled={!title.trim()}>
            {t("common:actions.add")}
          </Button>
        </form>
      </Modal>
      <ConfirmModal
        open={deleting != null}
        onClose={() => setDeleting(null)}
        title={t("categories.deleteTitle")}
        message={t("categories.deleteMessage", { title: deleting?.title ?? "" })}
        confirmLabel={t("common:actions.delete")}
        danger
        loading={remove.isPending}
        onConfirm={() => {
          if (!deleting) return;
          remove.mutate(deleting.id, {
            onSuccess: () => setDeleting(null),
            onError: (err) => toast.error(errorMessage(err)),
          });
        }}
      />
    </>
  );
}

function CategoryRow({
  category: c,
  first,
  last,
  onPress,
}: {
  category: Category;
  first: boolean;
  last: boolean;
  onPress: (button: RowButton) => void;
}) {
  const { t } = useTranslation(["item", "common"]);
  const rename = useRenameCategory();
  const toast = useToast();
  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState(c.title);
  const own = c.key != null;
  // Saved or cancelled, the name's field goes: the focus goes back to Rename.
  const renameButton = useRef<HTMLButtonElement>(null);
  const wasEditing = useRef(false);
  useEffect(() => {
    if (wasEditing.current && !editing) renameButton.current?.focus();
    wasEditing.current = editing;
  }, [editing]);

  const onSave = (e: FormEvent) => {
    e.preventDefault();
    const name = title.trim();
    if (!name || name === c.title) return setEditing(false);
    rename.mutate(
      { id: c.id, title: name },
      {
        onSuccess: () => setEditing(false),
        onError: (err) => toast.error(errorMessage(err)),
      },
    );
  };

  return (
    <div className="flex items-center gap-2 rounded-lg border border-border bg-surface-2 py-1.5 pl-1.5 pr-1">
      <span
        draggable
        data-drag-handle
        title={t("categories.drag")}
        aria-hidden
        className="shrink-0 cursor-grab rounded p-1 text-subtle transition-colors hover:text-fg active:cursor-grabbing"
      >
        <GripVertical className="size-4" />
      </span>
      {editing ? (
        <form onSubmit={onSave} className="flex min-w-0 flex-1 items-center gap-1">
          <TextInput
            aria-label={t("categories.rename", { title: c.title })}
            value={title}
            maxLength={MAX_TITLE}
            autoFocus
            onChange={(e) => setTitle(e.target.value)}
            className="h-8"
          />
          <IconButton label={t("common:actions.save")} size="sm" type="submit" disabled={rename.isPending}>
            <Check className="size-4" />
          </IconButton>
          <IconButton
            label={t("common:actions.cancel")}
            size="sm"
            onClick={() => {
              setTitle(c.title);
              setEditing(false);
            }}
          >
            <X className="size-4" />
          </IconButton>
        </form>
      ) : (
        <>
          <div className="min-w-0 flex-1">
            <div className="truncate text-sm font-medium">{c.title}</div>
            {c.from_label ? <div className="truncate text-xs text-muted">{t("categories.fromLabel", { label: c.from_label })}</div> : null}
          </div>
          {own ? <Badge>{t("categories.pleys")}</Badge> : null}
          <IconButton label={t("categories.moveUp", { title: c.title })} size="sm" data-row-button="up" disabled={first} onClick={() => onPress("up")}>
            <ChevronUp className="size-4" />
          </IconButton>
          <IconButton
            label={t("categories.moveDown", { title: c.title })}
            size="sm"
            data-row-button="down"
            disabled={last}
            onClick={() => onPress("down")}
          >
            <ChevronDown className="size-4" />
          </IconButton>
          {own ? null : (
            <>
              <IconButton ref={renameButton} label={t("categories.rename", { title: c.title })} size="sm" onClick={() => setEditing(true)}>
                <Pencil className="size-4" />
              </IconButton>
              <IconButton
                label={t("categories.delete", { title: c.title })}
                size="sm"
                className="hover:text-danger!"
                data-row-button="remove"
                onClick={() => onPress("remove")}
              >
                <Trash2 className="size-4" />
              </IconButton>
            </>
          )}
        </>
      )}
    </div>
  );
}

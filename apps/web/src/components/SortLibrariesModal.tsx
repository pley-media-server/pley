import { useLayoutEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { ChevronDown, ChevronUp, GripVertical } from "lucide-react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { keys, useForgetMyLibraryOrder, useLibraries, useSortMyLibraries } from "@/api/hooks";
import type { Id, Library } from "@/api/types";
import { cn } from "@/lib/cn";
import { moved, useDragReorder } from "./lists/reorder";
import { Modal } from "./Modal";
import { LibraryIcon } from "./Sidebar";
import { useToast } from "./Toast";
import { Button, IconButton } from "./ui";

type Direction = "up" | "down";

/**
 * The user's own order of their libraries: dragged by a grip, or moved a
 * place with a row's arrows (from a keyboard or a touch screen). Each move is
 * saved at once, for every device they use pley on; "Use the server's order"
 * goes back to the one everyone else sees.
 */
export function SortLibrariesModal({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation(["components", "common"]);
  const libs = useLibraries();
  const sort = useSortMyLibraries();
  const forget = useForgetMyLibraryOrder();
  const toast = useToast();
  const qc = useQueryClient();
  // Read out to screen readers after each move.
  const [note, setNote] = useState("");
  const listRef = useRef<HTMLOListElement>(null);
  // The library last moved with its buttons, and which one.
  const movedWith = useRef<{ id: Id; direction: Direction } | null>(null);

  const libraries = libs.data ?? [];
  const move = (from: number, to: number) => {
    const lib = libraries[from];
    if (!lib || to < 0 || to >= libraries.length || to === from) return;
    setNote(t("sortLibraries.moved", { name: lib.name, position: to + 1, total: libraries.length }));
    sort.mutate(moved(libraries.map((l) => l.id), from, to), {
      onError: (e) => {
        toast.error(errorMessage(e));
        const saved = qc.getQueryData<Library[]>(keys.libraries) ?? [];
        const back = saved.findIndex((l) => l.id === lib.id);
        setNote(
          back < 0
            ? t("sortLibraries.notMoved", { name: lib.name })
            : t("sortLibraries.movedBack", { name: lib.name, position: back + 1, total: saved.length }),
        );
      },
    });
  };
  const drag = useDragReorder(libraries.map((l) => l.id), (from, to) => {
    movedWith.current = null;
    move(from, to);
  });
  const moveBy = (index: number, direction: Direction) => {
    const lib = libraries[index];
    if (!lib) return;
    movedWith.current = { id: lib.id, direction };
    move(index, direction === "up" ? index - 1 : index + 1);
  };
  const backToServerOrder = () =>
    forget.mutate(undefined, {
      onSuccess: () => setNote(t("sortLibraries.backInServerOrder")),
      onError: (e) => toast.error(errorMessage(e)),
    });

  // A moved row takes the focus from the button that moved it, and at either
  // end of the list that button stops working: the focus goes back to it,
  // or to the row's other one (as Settings → Libraries does).
  const order = libraries.map((l) => l.id).join();
  const saving = sort.isPending;
  useLayoutEffect(() => {
    const target = movedWith.current;
    const row = target && listRef.current?.querySelector(`[data-library="${target.id}"]`);
    if (target && row) {
      const buttons = Array.from(row.querySelectorAll<HTMLButtonElement>("[data-move]")).filter((b) => !b.disabled);
      const button = buttons.find((b) => b.dataset.move === target.direction) ?? buttons[0];
      const focused = document.activeElement;
      const lost = !focused || focused === document.body || (row.contains(focused) && focused.matches(":disabled"));
      if (button && lost) button.focus();
    }
    if (!saving) movedWith.current = null;
  }, [order, saving]);

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t("sortLibraries.title")}
      size="sm"
      footer={
        <>
          <Button variant="ghost" className="mr-auto" onClick={backToServerOrder} loading={forget.isPending}>
            {t("sortLibraries.useServerOrder")}
          </Button>
          <Button variant="primary" onClick={onClose} data-autofocus>
            {t("common:actions.done")}
          </Button>
        </>
      }
    >
      <p className="mb-4 text-sm text-muted">{t("sortLibraries.hint")}</p>
      <ol ref={listRef} aria-label={t("sortLibraries.list")} className="space-y-2" {...drag.listProps}>
        {libraries.map((lib, i) => {
          const mark = drag.dropMark(i);
          return (
            <li
              key={lib.id}
              data-library={lib.id}
              {...drag.rowProps(i, { handle: true })}
              className={cn("relative transition-opacity", drag.dragging === i && "opacity-40")}
            >
              {/* Where a dragged library would land: a line in the gap, on a
                  strip that takes the drop too. */}
              {mark ? (
                <span aria-hidden className={cn("absolute inset-x-0 flex h-2 items-center", mark === "before" ? "-top-2" : "-bottom-2")}>
                  <span className="h-0.5 w-full rounded-full bg-accent" />
                </span>
              ) : null}
              <div className="flex items-center gap-3 rounded-lg border border-border bg-surface-2 py-1.5 pl-1.5 pr-1">
                <span
                  draggable
                  data-drag-handle
                  title={t("sortLibraries.drag")}
                  aria-hidden
                  className="shrink-0 cursor-grab rounded p-1 text-subtle transition-colors hover:text-fg active:cursor-grabbing"
                >
                  <GripVertical className="size-4" />
                </span>
                <span className="text-accent">
                  <LibraryIcon kind={lib.kind} />
                </span>
                <span className="min-w-0 flex-1 truncate text-sm font-medium">{lib.name}</span>
                <IconButton label={t("sortLibraries.moveUp", { name: lib.name })} size="sm" data-move="up" disabled={i === 0} onClick={() => moveBy(i, "up")}>
                  <ChevronUp className="size-4" />
                </IconButton>
                <IconButton
                  label={t("sortLibraries.moveDown", { name: lib.name })}
                  size="sm"
                  data-move="down"
                  disabled={i === libraries.length - 1}
                  onClick={() => moveBy(i, "down")}
                >
                  <ChevronDown className="size-4" />
                </IconButton>
              </div>
            </li>
          );
        })}
      </ol>
      <p role="status" className="sr-only">
        {note}
      </p>
    </Modal>
  );
}

import { useLayoutEffect, useRef, useState, type DragEvent, type RefObject } from "react";

/** `ids` with the one at `from` moved to `to` (unchanged when `from` is out of range). */
export function moved<T>(ids: readonly T[], from: number, to: number): T[] {
  const next = ids.slice();
  if (from < 0 || from >= next.length) return next;
  const [id] = next.splice(from, 1) as [T];
  next.splice(to, 0, id);
  return next;
}

/**
 * Drag-and-drop reordering of a list's rows (HTML5 drag and drop; rows also
 * offer move up / down buttons for keyboard and touch). `keys` are the rows'
 * ids, in order: the row picked up is followed by its id, so a list that
 * changes during the drag still moves that row.
 */
export function useDragReorder<K>(keys: readonly K[], onMove: (from: number, to: number) => void) {
  const [drag, setDrag] = useState<{ key: K; over: number } | null>(null);
  const from = drag ? keys.indexOf(drag.key) : -1;

  const drop = (to: number) => {
    if (from >= 0 && to !== from) onMove(from, to);
    setDrag(null);
  };
  const allowDrop = (e: DragEvent<HTMLElement>) => {
    e.preventDefault();
    e.dataTransfer.dropEffect = "move";
  };

  /**
   * A row's handlers. With `handle`, only an element in the row marked
   * `data-drag-handle` (and `draggable`) picks it up, so the rest of the row
   * keeps its text selectable.
   */
  const rowProps = (index: number, { handle = false } = {}) => ({
    ...(handle ? {} : { draggable: true }),
    onDragStart: (e: DragEvent<HTMLElement>) => {
      const key = keys[index];
      const byHandle = e.target instanceof Element && e.target.closest("[data-drag-handle]") !== null;
      if (key === undefined || (handle && !byHandle)) return;
      e.dataTransfer.effectAllowed = "move";
      // Firefox only starts a drag that carries data.
      e.dataTransfer.setData("text/plain", String(index));
      // Picked up by its handle, the whole row goes with the pointer.
      if (handle) {
        const box = e.currentTarget.getBoundingClientRect();
        e.dataTransfer.setDragImage(e.currentTarget, e.clientX - box.left, e.clientY - box.top);
      }
      setDrag({ key, over: index });
    },
    onDragOver: (e: DragEvent<HTMLElement>) => {
      if (!drag) return;
      allowDrop(e);
      if (drag.over !== index) setDrag({ key: drag.key, over: index });
    },
    onDrop: (e: DragEvent<HTMLElement>) => {
      e.preventDefault();
      // Not again by the list.
      e.stopPropagation();
      drop(index);
    },
    onDragEnd: () => setDrag(null),
  });

  /**
   * The list's handlers, for lists with gaps: a drop between rows lands
   * where the mark shows, and off the list no mark shows.
   */
  const listProps = {
    onDragOver: (e: DragEvent<HTMLElement>) => {
      if (drag) allowDrop(e);
    },
    onDragLeave: (e: DragEvent<HTMLElement>) => {
      const into = e.relatedTarget;
      if (drag && !(into instanceof Node && e.currentTarget.contains(into))) setDrag({ key: drag.key, over: from });
    },
    onDrop: (e: DragEvent<HTMLElement>) => {
      e.preventDefault();
      if (drag) drop(drag.over);
    },
  };

  /** Where a dragged row would land, relative to row `index`. */
  const dropMark = (index: number): "before" | "after" | null => {
    if (from < 0 || drag?.over !== index || from === index) return null;
    return from > index ? "before" : "after";
  };

  return { dragging: from >= 0 ? from : null, rowProps, listProps, dropMark };
}

/** The row button that last changed a list: an arrow, or the one that takes the row off. */
export type RowButton = "up" | "down" | "remove";

/**
 * Keeps the focus in a list whose rows are moved, or taken off, with their
 * own buttons. At either end of the list the arrow that moved a row stops
 * working, and a row taken off takes its button away: the focus would fall
 * out of the list (and out of a dialog around it). It goes to the moved row's
 * other arrow, or to the same button of the row that took the removed one's
 * place, as Settings → Libraries does. Rows carry `data-row` (their key), and
 * their buttons `data-row-button` ("up", "down" or "remove").
 */
export function useRowFocus(listRef: RefObject<HTMLElement | null>, keys: readonly string[]) {
  const pressed = useRef<{ key: string; index: number; button: RowButton } | null>(null);
  const order = keys.join("\n");

  useLayoutEffect(() => {
    const target = pressed.current;
    const list = listRef.current;
    if (!target || !list) return;
    const focused = document.activeElement;
    const lost = !focused || focused === document.body || (list.contains(focused) && focused.matches(":disabled"));
    const rows = Array.from(list.querySelectorAll<HTMLElement>("[data-row]"));
    const row =
      rows.find((r) => r.dataset.row === target.key) ??
      (target.button === "remove" ? rows[Math.min(target.index, rows.length - 1)] : undefined);
    // A row taken off is gone only once the list is loaded again.
    if (target.button === "remove" && row?.dataset.row === target.key) return;
    pressed.current = null;
    if (!lost || !row) return;
    const buttons = Array.from(row.querySelectorAll<HTMLButtonElement>("[data-row-button]")).filter((b) => !b.disabled);
    (buttons.find((b) => b.dataset.rowButton === target.button) ?? buttons[0])?.focus();
  }, [listRef, order]);

  /** Called before a row's button changes the list; with `null` for a change by dragging. */
  return (row: { key: string; index: number; button: RowButton } | null) => {
    pressed.current = row;
  };
}

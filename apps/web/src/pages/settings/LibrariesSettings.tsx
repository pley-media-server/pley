import { useLayoutEffect, useRef, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { Link, useSearchParams } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { ChevronDown, ChevronUp, EllipsisVertical, FolderPlus, GripVertical, LayoutList, Pencil, Plus, RefreshCw, ScanSearch, Trash2 } from "lucide-react";
import { errorMessage } from "@/api/client";
import { keys, useActivity, useDeleteLibrary, useReorderLibraries, useScanAll, useScanLibrary, useSharedLibraries } from "@/api/hooks";
import type { Id, Library, ScanStatus } from "@/api/types";
import { moved, useDragReorder } from "@/components/lists/reorder";
import { Menu, MenuItem, MenuSeparator } from "@/components/Menu";
import { ConfirmModal } from "@/components/Modal";
import { LibraryIcon } from "@/components/Sidebar";
import { useToast } from "@/components/Toast";
import { Button, EmptyState, IconButton, PageSpinner, ProgressBar } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { cn } from "@/lib/cn";
import { formatRelative } from "@/lib/format";
import { useDocumentTitle } from "@/lib/hooks";
import { HubLayoutModal } from "./HubLayoutModal";
import { LibraryFormModal } from "./LibraryFormModal";
import { libraryUrl } from "@/lib/urls";

type Direction = "up" | "down";

/** Where a library is in the list, and moving it one place from its row. */
interface Place {
  index: number;
  count: number;
  move: (direction: Direction) => void;
}

function LibraryRow({
  lib,
  scan,
  place,
  onEdit,
  onArrange,
  onDelete,
}: {
  lib: Library;
  scan?: ScanStatus;
  /** Without it (a single library), nothing to move. */
  place?: Place;
  onEdit: () => void;
  /** Its rows, on its page and on Home. */
  onArrange: () => void;
  onDelete: () => void;
}) {
  const { t } = useTranslation(["settings", "common"]);
  const toast = useToast();
  const scanMut = useScanLibrary();
  const running = lib.scanning || (scan && scan.phase !== "done");

  const startScan = (force: boolean) =>
    scanMut.mutate(
      { id: lib.id, force },
      {
        onSuccess: () => toast.success(force ? t("libraries.rescanStarted", { name: lib.name }) : t("libraries.scanStarted", { name: lib.name })),
        onError: (e) => toast.error(errorMessage(e)),
      },
    );

  return (
    <div className="group/library rounded-lg border border-border bg-surface p-4">
      <div className="flex items-start gap-4">
        {place ? (
          // What a library is dragged by, so the rest of the card keeps its
          // text selectable (its folders, to copy). The arrows do the same
          // from a keyboard or a touch screen.
          <span
            draggable
            data-drag-handle
            title={t("libraries.dragToReorder")}
            aria-hidden
            className="-ml-2 -mr-3 mt-2.5 hidden shrink-0 cursor-grab rounded p-1 text-subtle transition-colors hover:text-fg active:cursor-grabbing sm:block"
          >
            <GripVertical className="size-4" />
          </span>
        ) : null}
        <div className="flex size-11 shrink-0 items-center justify-center rounded-lg bg-surface-3 text-accent">
          <LibraryIcon kind={lib.kind} className="size-5" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
            <Link to={libraryUrl(lib.id)} draggable={false} className="truncate text-base font-semibold hover:text-accent">
              {lib.name}
            </Link>
            <span className="text-xs text-muted">
              {t(`libraries.items.${lib.kind}`, { count: lib.item_count })} ·{" "}
              {lib.scanned_at ? t("libraries.scanned", { when: formatRelative(lib.scanned_at) }) : t("libraries.neverScanned")}
            </span>
          </div>
          <ul className="mt-1.5 space-y-0.5">
            {lib.paths.map((p) => (
              <li key={p} className="truncate font-mono text-xs text-subtle" title={p}>
                {p}
              </li>
            ))}
          </ul>
          {running ? (
            <div className="mt-3 max-w-md">
              <div className="mb-1 flex items-center justify-between text-xs">
                <span className="flex items-center gap-1.5 text-accent">
                  <RefreshCw className="size-3 animate-spin" />
                  {scan ? t(`scanPhases.${scan.phase}`) : t("libraries.scanning")}
                </span>
                {scan && scan.total > 0 ? (
                  <span className="tabular-nums text-muted">
                    {scan.processed.toLocaleString(currentLocale())} / {scan.total.toLocaleString(currentLocale())}
                  </span>
                ) : null}
              </div>
              <ProgressBar
                value={scan && scan.total > 0 ? scan.processed / scan.total : 0}
                className="h-1.5"
                barClassName={!scan || scan.total === 0 ? "w-1/3 animate-pulse" : undefined}
              />
            </div>
          ) : null}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {place ? (
            // Stacked on phones, where the row is narrow; shown on hover
            // where there is a pointer to hover with, always on touch screens.
            <div className="flex flex-col items-center transition-opacity sm:flex-row [@media(hover:hover)]:sm:opacity-0 sm:group-focus-within/library:opacity-100 sm:group-hover/library:opacity-100">
              <IconButton
                label={t("libraries.moveUp", { name: lib.name })}
                size="sm"
                data-move="up"
                disabled={place.index === 0}
                onClick={() => place.move("up")}
              >
                <ChevronUp className="size-4" />
              </IconButton>
              <IconButton
                label={t("libraries.moveDown", { name: lib.name })}
                size="sm"
                data-move="down"
                disabled={place.index === place.count - 1}
                onClick={() => place.move("down")}
              >
                <ChevronDown className="size-4" />
              </IconButton>
            </div>
          ) : null}
          <Button size="sm" icon={<ScanSearch className="size-4" />} onClick={() => startScan(false)} loading={scanMut.isPending} disabled={!!running}>
            <span className="hidden sm:inline">{t("libraries.scan")}</span>
          </Button>
          <Menu
            trigger={({ toggle }) => (
              <IconButton label={t("libraries.moreActions")} size="sm" onClick={toggle}>
                <EllipsisVertical className="size-4" />
              </IconButton>
            )}
          >
            {(close) => (
              <>
                <MenuItem
                  icon={<RefreshCw />}
                  onClick={() => {
                    close();
                    startScan(true);
                  }}
                >
                  {t("libraries.forceRescan")}
                </MenuItem>
                <MenuItem
                  icon={<Pencil />}
                  onClick={() => {
                    close();
                    onEdit();
                  }}
                >
                  {t("common:actions.edit")}
                </MenuItem>
                <MenuItem
                  icon={<LayoutList />}
                  onClick={() => {
                    close();
                    onArrange();
                  }}
                >
                  {t("libraries.rows")}
                </MenuItem>
                <MenuSeparator />
                <MenuItem
                  icon={<Trash2 />}
                  danger
                  onClick={() => {
                    close();
                    onDelete();
                  }}
                >
                  {t("common:actions.delete")}
                </MenuItem>
              </>
            )}
          </Menu>
        </div>
      </div>
    </div>
  );
}

export function LibrariesSettings() {
  const { t } = useTranslation(["settings", "common"]);
  useDocumentTitle(t("libraries.title"));
  // In the shared order, whatever order the administrator sees them in.
  const libs = useSharedLibraries();
  const activity = useActivity();
  const toast = useToast();
  const qc = useQueryClient();
  const scanAll = useScanAll();
  const del = useDeleteLibrary();
  const reorder = useReorderLibraries();
  const [params, setParams] = useSearchParams();
  const adding = params.get("add") === "1";
  const [editing, setEditing] = useState<Library | null>(null);
  const [arranging, setArranging] = useState<Library | null>(null);
  const [deleting, setDeleting] = useState<Library | null>(null);
  // Read out to screen readers after each move.
  const [moveNote, setMoveNote] = useState("");
  const listRef = useRef<HTMLOListElement>(null);
  // The library last moved with its buttons, and which one.
  const movedWith = useRef<{ id: Id; direction: Direction } | null>(null);

  const libraries = libs.data ?? [];
  const reorderable = libraries.length > 1;
  const move = (from: number, to: number) => {
    const lib = libraries[from];
    if (!lib || to < 0 || to >= libraries.length || to === from) return;
    setMoveNote(t("libraries.moved", { name: lib.name, position: to + 1, total: libraries.length }));
    reorder.mutate(moved(libraries.map((l) => l.id), from, to), {
      onError: (e) => {
        toast.error(errorMessage(e));
        // The list is back in the saved order: say where the library went,
        // which also makes trying again read out (the note changes).
        const saved = qc.getQueryData<Library[]>(keys.sharedLibraries) ?? [];
        const back = saved.findIndex((l) => l.id === lib.id);
        setMoveNote(
          back < 0
            ? t("libraries.notMoved", { name: lib.name })
            : t("libraries.movedBack", { name: lib.name, position: back + 1, total: saved.length }),
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

  // Moving a row in the page takes the focus from the button that moved it,
  // and at either end of the list that button stops working: the focus goes
  // back to it, or to the row's other one. Kept up until the move is saved,
  // since a move that fails is undone, which moves the row again.
  const order = libraries.map((l) => l.id).join();
  const saving = reorder.isPending;
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

  const openAdd = () => {
    const next = new URLSearchParams(params);
    next.set("add", "1");
    setParams(next, { replace: true });
  };
  const closeAdd = () => {
    const next = new URLSearchParams(params);
    next.delete("add");
    setParams(next, { replace: true });
  };

  if (libs.isPending) return <PageSpinner />;

  const scans = activity.data?.scans ?? [];

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 className="text-lg font-semibold">{t("libraries.title")}</h2>
          <p className="text-sm text-muted">
            {t("libraries.description")}
            {reorderable ? ` ${t("libraries.sharedOrder")}` : null}
          </p>
        </div>
        <div className="flex gap-2">
          {libs.data && libs.data.length > 0 ? (
            <Button
              icon={<RefreshCw className="size-4" />}
              loading={scanAll.isPending}
              onClick={() =>
                scanAll.mutate(undefined, {
                  onSuccess: () => toast.success(t("libraries.scanningAll")),
                  onError: (e) => toast.error(errorMessage(e)),
                })
              }
            >
              {t("libraries.scanAll")}
            </Button>
          ) : null}
          <Button variant="primary" icon={<Plus className="size-4" />} onClick={openAdd}>
            {t("libraries.add")}
          </Button>
        </div>
      </div>

      {/* A list once loaded stays up when fetching it again fails. */}
      {!libs.data ? (
        <EmptyState
          title={t("libraries.couldntLoad")}
          description={errorMessage(libs.error)}
          action={<Button onClick={() => void libs.refetch()}>{t("common:actions.retry")}</Button>}
        />
      ) : libs.data.length === 0 ? (
        <div className="rounded-lg border border-dashed border-border-strong">
          <EmptyState
            icon={<FolderPlus />}
            title={t("libraries.emptyTitle")}
            description={t("libraries.emptyDescription")}
            action={
              <Button variant="primary" onClick={openAdd}>
                {t("libraries.add")}
              </Button>
            }
          />
        </div>
      ) : (
        <>
          <ol ref={listRef} aria-label={t("libraries.title")} className="space-y-3" {...(reorderable ? drag.listProps : {})}>
            {libs.data.map((lib, i) => {
              const mark = drag.dropMark(i);
              return (
                <li
                  key={lib.id}
                  data-library={lib.id}
                  {...(reorderable ? drag.rowProps(i, { handle: true }) : {})}
                  className={cn("relative transition-opacity", drag.dragging === i && "opacity-40")}
                >
                  {/* Where a dragged library would land: a line in the gap, on a
                      strip that takes the drop too (before the first card and
                      after the last, it is outside the list). */}
                  {mark ? (
                    <span
                      aria-hidden
                      className={cn("absolute inset-x-0 flex h-3 items-center", mark === "before" ? "-top-3" : "-bottom-3")}
                    >
                      <span className="h-0.5 w-full rounded-full bg-accent" />
                    </span>
                  ) : null}
                  <LibraryRow
                    lib={lib}
                    scan={scans.find((s) => s.library_id === lib.id)}
                    place={reorderable ? { index: i, count: libs.data.length, move: (direction) => moveBy(i, direction) } : undefined}
                    onEdit={() => setEditing(lib)}
                    onArrange={() => setArranging(lib)}
                    onDelete={() => setDeleting(lib)}
                  />
                </li>
              );
            })}
          </ol>
          <p role="status" className="sr-only">
            {moveNote}
          </p>
        </>
      )}

      <LibraryFormModal open={adding} onClose={closeAdd} />
      <LibraryFormModal open={!!editing} onClose={() => setEditing(null)} library={editing} />
      <HubLayoutModal library={arranging} onClose={() => setArranging(null)} />
      <ConfirmModal
        open={!!deleting}
        onClose={() => setDeleting(null)}
        title={t("libraries.deleteTitle")}
        danger
        confirmLabel={t("common:actions.delete")}
        loading={del.isPending}
        message={
          <Trans t={t} i18nKey="libraries.deleteMessage" values={{ name: deleting?.name ?? "" }} components={{ name: <strong className="text-fg" /> }} />
        }
        onConfirm={() => {
          if (!deleting) return;
          del.mutate(deleting.id, {
            onSuccess: () => {
              toast.success(t("libraries.deleted", { name: deleting.name }));
              setDeleting(null);
            },
            onError: (e) => toast.error(errorMessage(e)),
          });
        }}
      />
    </div>
  );
}

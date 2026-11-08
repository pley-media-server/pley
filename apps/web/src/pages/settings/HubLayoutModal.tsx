import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, ChevronDown, ChevronUp, GripVertical, Trash2 } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useArrangeHubs, useHubLayout } from "@/api/hooks";
import { useCollections } from "@/api/lists";
import type { HubLayoutRow, HubPlace, Library } from "@/api/types";
import { moved, useDragReorder } from "@/components/lists/reorder";
import { Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Button, IconButton, PageSpinner, Select } from "@/components/ui";
import { cn } from "@/lib/cn";

/** Where a row can show, as `HubPlace` names it. */
const PLACES = ["on_library", "on_admin_home", "on_home"] as const;
type Place = (typeof PLACES)[number];

const place = ({ hub, on_library, on_admin_home, on_home }: HubLayoutRow): HubPlace => ({ hub, on_library, on_admin_home, on_home });

/**
 * A library's rows: its built-in ones and the collections promoted to rows,
 * in order, each shown on the library's page, on administrators' Home and on
 * everyone else's Home, or not. Every change is saved at once; tools that
 * manage Plex hubs (Agregarr, Kometa) change the same rows.
 */
export function HubLayoutModal({ library, onClose }: { library: Library | null; onClose: () => void }) {
  const { t } = useTranslation(["settings", "common"]);
  const toast = useToast();
  const id = library?.id ?? 0;
  const layout = useHubLayout(id, !!library);
  const collections = useCollections(id, !!library);
  const { arrange, reset } = useArrangeHubs(id);
  // Read out to screen readers after each change.
  const [note, setNote] = useState("");

  const rows = layout.data ?? [];
  const save = (hubs: HubPlace[]) => arrange(hubs, { onError: (e) => toast.error(errorMessage(e)) });
  const move = (from: number, to: number) => {
    const row = rows[from];
    if (!row || to < 0 || to >= rows.length || to === from) return;
    setNote(t("hubLayout.moved", { name: row.title, position: to + 1, total: rows.length }));
    save(moved(rows.map(place), from, to));
  };
  const drag = useDragReorder(rows.map((r) => r.hub), move);
  const toggle = (index: number, where: Place) => save(rows.map((r, i) => (i === index ? { ...place(r), [where]: !r[where] } : place(r))));
  const remove = (index: number) => save(rows.filter((_, i) => i !== index).map(place));
  const promoted = new Set(rows.map((r) => r.hub));
  const addable = (collections.data ?? []).filter((c) => !promoted.has(`collection:${c.id}`));
  const add = (value: string) => {
    if (!value) return;
    save([...rows.map(place), { hub: `collection:${value}`, on_library: true, on_admin_home: false, on_home: false }]);
  };
  const backToUsual = () =>
    reset.mutate(undefined, {
      onSuccess: () => setNote(t("hubLayout.backToUsual")),
      onError: (e) => toast.error(errorMessage(e)),
    });

  return (
    <Modal
      open={!!library}
      onClose={onClose}
      title={t("hubLayout.title", { name: library?.name ?? "" })}
      footer={
        <>
          <Button variant="ghost" className="mr-auto" onClick={backToUsual} loading={reset.isPending}>
            {t("hubLayout.useUsual")}
          </Button>
          <Button variant="primary" onClick={onClose} data-autofocus>
            {t("common:actions.done")}
          </Button>
        </>
      }
    >
      <p className="mb-4 text-sm text-muted">{t("hubLayout.hint")}</p>
      {layout.isPending ? (
        <PageSpinner />
      ) : (
        <>
          <div aria-hidden className="mb-1 hidden grid-cols-[1fr_repeat(3,4.5rem)_5.5rem] gap-1 px-2 text-xs text-subtle sm:grid">
            <span />
            {PLACES.map((where) => (
              <span key={where} className="text-center">
                {t(`hubLayout.places.${where}`)}
              </span>
            ))}
            <span />
          </div>
          <ol aria-label={t("hubLayout.list")} className="space-y-2" {...drag.listProps}>
            {rows.map((row, i) => {
              const mark = drag.dropMark(i);
              const collection = row.hub.startsWith("collection:");
              return (
                <li key={row.hub} {...drag.rowProps(i, { handle: true })} className={cn("relative transition-opacity", drag.dragging === i && "opacity-40")}>
                  {mark ? (
                    <span aria-hidden className={cn("absolute inset-x-0 flex h-2 items-center", mark === "before" ? "-top-2" : "-bottom-2")}>
                      <span className="h-0.5 w-full rounded-full bg-accent" />
                    </span>
                  ) : null}
                  <div className="grid grid-cols-[1fr_auto] items-center gap-1 rounded-lg border border-border bg-surface-2 py-1.5 pl-1.5 pr-1 sm:grid-cols-[1fr_repeat(3,4.5rem)_5.5rem]">
                    <div className="flex min-w-0 items-center gap-2">
                      <span
                        draggable
                        data-drag-handle
                        title={t("hubLayout.drag")}
                        aria-hidden
                        className="shrink-0 cursor-grab rounded p-1 text-subtle transition-colors hover:text-fg active:cursor-grabbing"
                      >
                        <GripVertical className="size-4" />
                      </span>
                      <span className="min-w-0 truncate text-sm font-medium">{row.title}</span>
                      {collection ? <span className="shrink-0 text-xs text-subtle">{t("hubLayout.collection")}</span> : null}
                    </div>
                    <div className="col-span-2 row-start-2 flex flex-wrap gap-x-4 gap-y-1 pl-8 sm:contents">
                      {PLACES.map((where) => (
                        <label key={where} className="flex cursor-pointer items-center gap-2 text-xs text-muted sm:justify-center">
                          <input type="checkbox" checked={row[where]} onChange={() => toggle(i, where)} aria-label={t(`hubLayout.show.${where}`, { name: row.title })} className="peer sr-only" />
                          <span
                            aria-hidden
                            className={cn(
                              "flex size-4 shrink-0 items-center justify-center rounded border transition-colors peer-focus-visible:ring-2 peer-focus-visible:ring-accent",
                              row[where] ? "border-accent bg-accent text-accent-fg" : "border-border-strong",
                            )}
                          >
                            {row[where] ? <Check className="size-3" strokeWidth={3} /> : null}
                          </span>
                          <span className="sm:sr-only">{t(`hubLayout.places.${where}`)}</span>
                        </label>
                      ))}
                    </div>
                    <div className="col-start-2 row-start-1 flex justify-end sm:col-start-auto">
                      <IconButton label={t("hubLayout.moveUp", { name: row.title })} size="sm" disabled={i === 0} onClick={() => move(i, i - 1)}>
                        <ChevronUp className="size-4" />
                      </IconButton>
                      <IconButton label={t("hubLayout.moveDown", { name: row.title })} size="sm" disabled={i === rows.length - 1} onClick={() => move(i, i + 1)}>
                        <ChevronDown className="size-4" />
                      </IconButton>
                      {collection ? (
                        <IconButton label={t("hubLayout.remove", { name: row.title })} size="sm" onClick={() => remove(i)}>
                          <Trash2 className="size-4" />
                        </IconButton>
                      ) : null}
                    </div>
                  </div>
                </li>
              );
            })}
          </ol>
          {addable.length > 0 ? (
            <Select className="mt-4 w-full" aria-label={t("hubLayout.add")} value="" onChange={(e) => add(e.target.value)}>
              <option value="">{t("hubLayout.add")}</option>
              {addable.map((c) => (
                <option key={c.id} value={c.id}>
                  {c.title}
                </option>
              ))}
            </Select>
          ) : null}
        </>
      )}
      <p role="status" className="sr-only">
        {note}
      </p>
    </Modal>
  );
}

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Trans, useTranslation } from "react-i18next";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router";
import { Check, FileText, ImageUp, Lock, LockOpen, Pencil, RotateCcw, Search } from "lucide-react";
import { api, errorMessage, request } from "@/api/client";
import { keys, useRatingSystems } from "@/api/hooks";
import type { ItemDetail, RatingSystems } from "@/api/types";
import { useAuth } from "@/auth";
import { LabelPicker } from "@/components/LabelPicker";
import { Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Button, Field, Spinner, TextInput } from "@/components/ui";
import { t as translate } from "@/i18n";
import { cn } from "@/lib/cn";
import { systemName, writtenCountry } from "@/lib/access";
import { sameLabels } from "@/lib/labels";
import { itemUrl } from "@/lib/urls";

type ArtworkKind = "poster" | "backdrop" | "thumb";

interface MatchCandidate {
  tmdb_id: number;
  title: string;
  original_title: string | null;
  year: number | null;
  overview: string | null;
  poster_url: string | null;
}

/** A book Fix Match offers: an Open Library work, or a recording on Audible. */
interface BookCandidate {
  openlibrary_id: string | null;
  asin: string | null;
  title: string;
  authors: string[];
  narrators: string[];
  year: number | null;
  poster_url: string | null;
}

interface ArtworkCandidate {
  url: string;
  preview_url: string;
  /** 0 where the source doesn't say (book covers). */
  width: number;
  height: number;
  language: string | null;
  votes: number;
}

/** One of the editing dialogs of an item page. */
export type EditDialog = "details" | "match" | ArtworkKind;

/** What an administrator can edit on `item`: details, the match, artwork. */
export function editActions(item: ItemDetail): { dialog: EditDialog; label: string; icon: ReactNode }[] {
  const book = item.kind === "audiobook";
  const label = (dialog: EditDialog) =>
    translate(book && dialog === "poster" ? "edit.menu.cover" : `edit.menu.${dialog}`, { ns: "item" });
  const actions: { dialog: EditDialog; label: string; icon: ReactNode }[] = [
    { dialog: "details", label: label("details"), icon: <Pencil /> },
  ];
  if (item.kind === "movie" || item.kind === "show" || book) actions.push({ dialog: "match", label: label("match"), icon: <Search /> });
  // A book's only picture is its cover.
  const artwork: ArtworkKind[] =
    item.kind === "episode" ? ["thumb"] : item.kind === "season" || book ? ["poster"] : ["poster", "backdrop"];
  for (const kind of artwork) actions.push({ dialog: kind, label: label(kind), icon: <ImageUp /> });
  return actions;
}

/** The open editing dialog, if any (administrators only). */
export function EditDialogs({ item, dialog, onClose }: { item: ItemDetail; dialog: EditDialog | null; onClose: () => void }) {
  const user = useAuth();
  if (!user.is_admin || dialog === null) return null;
  if (dialog === "details") return <EditDetailsDialog item={item} onClose={onClose} />;
  if (dialog === "match") {
    return item.kind === "audiobook" ? <BookMatchDialog item={item} onClose={onClose} /> : <FixMatchDialog item={item} onClose={onClose} />;
  }
  return <ArtworkDialog item={item} kind={dialog} onClose={onClose} />;
}

function useItemRefresh(id: number) {
  const qc = useQueryClient();
  return (detail?: ItemDetail) => {
    if (detail) qc.setQueryData(["item", id], detail);
    void qc.invalidateQueries({ queryKey: ["item", id] });
  };
}

// ---------------------------------------------------------------- details

const FIELDS = [
  "title",
  "sort_title",
  "original_title",
  "authors",
  "narrators",
  "series",
  "series_index",
  "year",
  "release_date",
  "content_rating",
  "studio",
  "tagline",
  "genres",
  "summary",
] as const;
type FieldKey = (typeof FIELDS)[number];

/** Fields only audiobooks have (the server's `overrides::BOOK_FIELDS`). */
const BOOK_FIELDS: FieldKey[] = ["authors", "narrators", "series", "series_index"];
/** Fields of the kinds of item that have them. */
function fieldsOf(kind: ItemDetail["kind"]): FieldKey[] {
  if (kind === "audiobook") return FIELDS.filter((key) => !["original_title", "studio", "tagline", "content_rating"].includes(key));
  const fields = FIELDS.filter((key) => !BOOK_FIELDS.includes(key));
  if (kind === "episode" || kind === "season") {
    return fields.filter((key) => !["original_title", "studio", "tagline", "content_rating", "genres"].includes(key));
  }
  return fields;
}

/** Names edited one to a line: a name may have a comma ("Tolkien, J. R. R."). */
const NAME_LISTS: FieldKey[] = ["authors", "narrators"];

/** The kinds of item that take labels (the server's `labels::LABELLED`). */
const LABELLED = new Set(["movie", "show", "season", "episode", "artist", "album"]);

function initialValue(item: ItemDetail, key: FieldKey): string {
  switch (key) {
    case "genres":
      return item.genres.join(", ");
    case "year":
      return item.year != null ? String(item.year) : "";
    case "authors":
    case "narrators":
      return (item.book?.[key] ?? []).join("\n");
    case "series":
      return item.book?.series ?? "";
    case "series_index":
      return item.book?.series_index != null ? String(item.book.series_index) : "";
    default:
      return item[key] ?? "";
  }
}

function EditDetailsDialog({ item, onClose }: { item: ItemDetail; onClose: () => void }) {
  const { t } = useTranslation(["item", "common"]);
  const toast = useToast();
  const qc = useQueryClient();
  const refresh = useItemRefresh(item.id);
  const initial = useMemo(
    () => Object.fromEntries(FIELDS.map((key) => [key, initialValue(item, key)])) as Record<FieldKey, string>,
    [item],
  );
  const [values, setValues] = useState(initial);
  const [unlock, setUnlock] = useState<Set<FieldKey>>(new Set());
  const [labels, setLabels] = useState(item.labels ?? []);
  const { data: systems } = useRatingSystems();
  const labelled = LABELLED.has(item.kind) && item.labels !== undefined;
  const locked = new Set(item.locked_fields ?? []);
  const fromNfo = new Set(item.nfo_fields ?? []);
  const fields = fieldsOf(item.kind);

  const save = useMutation({
    mutationFn: () => {
      const body: Record<string, unknown> = {};
      for (const key of fields) {
        if (unlock.has(key)) {
          body[key] = null;
          continue;
        }
        if (values[key] === initial[key]) continue;
        const v = values[key].trim();
        if (key === "genres") body[key] = v.split(",").map((g) => g.trim()).filter(Boolean);
        else if (NAME_LISTS.includes(key)) body[key] = v.split("\n").map((n) => n.trim()).filter(Boolean);
        else if (key === "year") body[key] = v === "" ? null : Number(v);
        // Not a number: sent as typed, for the server to say what it takes.
        else if (key === "series_index") body[key] = v === "" ? null : Number.isFinite(Number(v)) ? Number(v) : v;
        else body[key] = v === "" ? null : v;
      }
      if (labelled && !sameLabels(labels, item.labels ?? [])) body.labels = labels;
      return api.patch<ItemDetail>(`/api/items/${item.id}`, body);
    },
    onSuccess: (detail) => {
      refresh(detail);
      if (labelled && !sameLabels(labels, item.labels ?? [])) void qc.invalidateQueries({ queryKey: keys.labels });
      toast.success(t("common:states.saved"));
      onClose();
    },
    onError: (e) => toast.error(errorMessage(e)),
  });

  return (
    <Modal
      open
      onClose={onClose}
      title={t("edit.details.title")}
      size="lg"
      locked={save.isPending}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={save.isPending}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="primary" onClick={() => save.mutate()} disabled={save.isPending}>
            {save.isPending ? t("common:actions.saving") : t("common:actions.save")}
          </Button>
        </>
      }
    >
      <p className="mb-4 text-sm text-muted">
        <Trans t={t} i18nKey="edit.details.hint" components={{ lock: <Lock className="inline size-3.5 align-[-2px]" /> }} />
        {fromNfo.size > 0 ? ` ${t("edit.details.nfoHint")}` : null}
      </p>
      <div className="grid gap-4 sm:grid-cols-2">
        {fields.map((key) => {
          const isLocked = locked.has(key) && !unlock.has(key);
          const wide = key === "summary" || key === "title" || key === "genres";
          const names = NAME_LISTS.includes(key);
          const input =
            key === "summary" || names ? (
              <textarea
                id={`edit-${key}`}
                value={values[key]}
                rows={names ? 3 : 5}
                placeholder={names ? t("edit.details.namesPlaceholder") : undefined}
                onChange={(e) => setValues({ ...values, [key]: e.target.value })}
                className="w-full rounded-md border border-border bg-surface-2 px-3 py-2 text-sm text-fg focus:border-accent focus:outline-none"
              />
            ) : (
              <TextInput
                id={`edit-${key}`}
                value={values[key]}
                inputMode={key === "year" ? "numeric" : key === "series_index" ? "decimal" : undefined}
                placeholder={key === "release_date" ? "1999-03-31" : key === "genres" ? t("edit.details.genresPlaceholder") : undefined}
                onChange={(e) => setValues({ ...values, [key]: e.target.value })}
              />
            );
          return (
            <div key={key} className={cn(wide && "sm:col-span-2")}>
              <Field
                label={t(`edit.details.fields.${key}`)}
                htmlFor={`edit-${key}`}
                hint={withRatingCountry(
                  key === "content_rating"
                    ? ratingCountry(item, systems, values.content_rating, initial.content_rating)
                    : null,
                  locked.has(key) ? (
                    <button
                      type="button"
                      className="inline-flex items-center gap-1 text-subtle hover:text-accent"
                      onClick={() => {
                        const next = new Set(unlock);
                        if (next.has(key)) next.delete(key);
                        else next.add(key);
                        setUnlock(next);
                      }}
                    >
                      {isLocked ? <Lock className="size-3.5" /> : <LockOpen className="size-3.5" />}
                      {isLocked ? t("edit.details.locked") : t("edit.details.willUnlock")}
                    </button>
                  ) : fromNfo.has(key) ? (
                    <span className="inline-flex items-center gap-1 text-subtle">
                      <FileText className="size-3.5" />
                      {t("edit.details.fromNfo")}
                    </span>
                  ) : undefined,
                )}
              >
                {input}
              </Field>
            </div>
          );
        })}
        {labelled ? (
          <div className="sm:col-span-2">
            <Field label={t("edit.details.fields.labels")} htmlFor="edit-labels" hint={t("edit.details.labelsHint")}>
              <LabelPicker
                id="edit-labels"
                value={labels}
                onChange={setLabels}
                removeLabel={(label) => t("edit.details.removeLabel", { label })}
              />
            </Field>
          </div>
        ) : null}
      </div>
    </Modal>
  );
}

// ---------------------------------------------------------------- fix match

function FixMatchDialog({ item, onClose }: { item: ItemDetail; onClose: () => void }) {
  const { t } = useTranslation(["item", "common"]);
  const toast = useToast();
  const refresh = useItemRefresh(item.id);
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [title, setTitle] = useState(item.title);
  const [year, setYear] = useState(item.year != null ? String(item.year) : "");
  const [query, setQuery] = useState({ title: item.title, year: item.year != null ? String(item.year) : "" });
  const results = useQuery({
    queryKey: ["matches", item.id, query.title, query.year],
    queryFn: ({ signal }) => {
      const p = new URLSearchParams({ title: query.title });
      if (query.year) p.set("year", query.year);
      return api.get<MatchCandidate[]>(`/api/items/${item.id}/matches?${p}`, { signal });
    },
    retry: false,
  });
  const apply = useMutation({
    mutationFn: (tmdbId: number) => api.post<ItemDetail>(`/api/items/${item.id}/match`, { tmdb_id: tmdbId }),
    onSuccess: (detail) => {
      toast.success(t("edit.match.matched", { title: detail.title }));
      onClose();
      if (detail.id === item.id) {
        refresh(detail);
        return;
      }
      // A movie matched to the title of another copy went into it: the
      // answer is that movie, and this one's page is gone.
      qc.setQueryData(["item", detail.id], detail);
      void navigate(itemUrl(detail.id), { replace: true });
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const undo = useUndoMatch(item, onClose);
  const busy = apply.isPending || undo.isPending;

  return (
    <Modal open onClose={onClose} title={t("edit.match.title")} size="lg" locked={busy}>
      <MatchedByHand item={item} undo={undo} busy={busy} />
      <form
        className="mb-4 flex flex-wrap gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          setQuery({ title: title.trim() || item.title, year: year.trim() });
        }}
      >
        <TextInput
          className="min-w-0 flex-1"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          aria-label={t("edit.details.fields.title")}
        />
        <TextInput
          className="w-24"
          value={year}
          inputMode="numeric"
          placeholder={t("edit.details.fields.year")}
          onChange={(e) => setYear(e.target.value)}
          aria-label={t("edit.details.fields.year")}
        />
        <Button type="submit" icon={<Search className="size-4" />}>
          {t("common:actions.search")}
        </Button>
      </form>
      {results.isPending ? (
        <div className="flex justify-center py-10">
          <Spinner />
        </div>
      ) : results.isError ? (
        <p className="py-6 text-center text-sm text-danger">{errorMessage(results.error)}</p>
      ) : results.data.length === 0 ? (
        <p className="py-6 text-center text-sm text-muted">{t("edit.match.noMatches")}</p>
      ) : (
        <ul className="max-h-[55vh] space-y-2 overflow-y-auto pr-1">
          {results.data.map((m) => {
            const current = m.tmdb_id === item.tmdb_id;
            return (
              <li key={m.tmdb_id}>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => apply.mutate(m.tmdb_id)}
                  className={cn(
                    "flex w-full gap-3 rounded-lg border p-2 text-left transition-colors hover:border-accent hover:bg-surface-2",
                    current ? "border-accent/60 bg-surface-2" : "border-border",
                  )}
                >
                  <div className="h-24 w-16 shrink-0 overflow-hidden rounded bg-surface-3">
                    {m.poster_url ? <img src={m.poster_url} alt="" loading="lazy" className="size-full object-cover" /> : null}
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline gap-2">
                      <span className="truncate font-semibold">{m.title}</span>
                      {m.year ? <span className="shrink-0 text-sm text-muted">{m.year}</span> : null}
                      {current ? (
                        <span className="ml-auto inline-flex shrink-0 items-center gap-1 text-xs font-medium text-accent">
                          <Check className="size-3.5" /> {t("edit.match.current")}
                        </span>
                      ) : null}
                    </div>
                    {m.original_title ? <div className="truncate text-xs text-subtle">{m.original_title}</div> : null}
                    {m.overview ? <p className="mt-1 line-clamp-3 text-sm text-fg/75">{m.overview}</p> : null}
                  </div>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </Modal>
  );
}

/**
 * Undoing Fix Match. The match is remembered so that an NFO file's ids (a
 * book's search, or its files' ASIN) don't undo it; undone, they decide the
 * match again.
 */
function useUndoMatch(item: ItemDetail, onClose: () => void) {
  const { t } = useTranslation("item");
  const toast = useToast();
  const refresh = useItemRefresh(item.id);
  return useMutation({
    mutationFn: () => api.patch<ItemDetail>(`/api/items/${item.id}`, { match: null }),
    onSuccess: (detail) => {
      refresh(detail);
      toast.success(t("edit.match.undone"));
      onClose();
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
}

/** Says the match was chosen here, with Undo, when it was. */
function MatchedByHand({ item, undo, busy }: { item: ItemDetail; undo: { mutate: () => void }; busy: boolean }) {
  const { t } = useTranslation("item");
  if (!(item.locked_fields ?? []).includes("match")) return null;
  return (
    <div className="mb-4 flex flex-wrap items-center gap-3 rounded-lg border border-border bg-surface-2 p-3 text-sm">
      <p className="min-w-0 flex-1 text-muted">{t(item.kind === "audiobook" ? "edit.match.bookByHand" : "edit.match.byHand")}</p>
      <Button size="sm" icon={<RotateCcw className="size-4" />} disabled={busy} onClick={() => undo.mutate()}>
        {t("edit.match.undo")}
      </Button>
    </div>
  );
}

/** Fix Match for an audiobook: an Open Library work, or a recording by its ASIN. */
function BookMatchDialog({ item, onClose }: { item: ItemDetail; onClose: () => void }) {
  const { t } = useTranslation(["item", "common"]);
  const toast = useToast();
  const refresh = useItemRefresh(item.id);
  const own = { title: item.title, author: item.book?.authors[0] ?? "" };
  const [title, setTitle] = useState(own.title);
  const [author, setAuthor] = useState(own.author);
  const [query, setQuery] = useState(own);
  const results = useQuery({
    queryKey: ["matches", item.id, query.title, query.author],
    queryFn: ({ signal }) => {
      // An empty author asks for none: the server goes by the book's own
      // writer only when it's left out.
      const p = new URLSearchParams({ title: query.title, author: query.author });
      return api.get<BookCandidate[]>(`/api/items/${item.id}/matches?${p}`, { signal });
    },
    retry: false,
  });
  const apply = useMutation({
    mutationFn: (m: BookCandidate) =>
      api.post<ItemDetail>(`/api/items/${item.id}/match`, m.asin ? { asin: m.asin } : { openlibrary_id: m.openlibrary_id }),
    onSuccess: (detail) => {
      refresh(detail);
      toast.success(t("edit.match.matched", { title: detail.title }));
      onClose();
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const undo = useUndoMatch(item, onClose);
  const busy = apply.isPending || undo.isPending;
  const current = (m: BookCandidate) =>
    m.asin ? m.asin === item.book?.asin : m.openlibrary_id != null && m.openlibrary_id === item.book?.openlibrary_id;

  return (
    <Modal open onClose={onClose} title={t("edit.match.title")} size="lg" locked={busy}>
      <MatchedByHand item={item} undo={undo} busy={busy} />
      <form
        className="mb-4 flex flex-wrap gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          setQuery({ title: title.trim() || item.title, author: author.trim() });
        }}
      >
        <TextInput
          className="min-w-0 flex-[2]"
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          aria-label={t("edit.match.bookTitle")}
          placeholder={t("edit.match.bookTitle")}
        />
        <TextInput
          className="min-w-0 flex-1"
          value={author}
          onChange={(e) => setAuthor(e.target.value)}
          aria-label={t("edit.match.author")}
          placeholder={t("edit.match.author")}
        />
        <Button type="submit" icon={<Search className="size-4" />}>
          {t("common:actions.search")}
        </Button>
      </form>
      {results.isPending ? (
        <div className="flex justify-center py-10">
          <Spinner />
        </div>
      ) : results.isError ? (
        <p className="py-6 text-center text-sm text-danger">{errorMessage(results.error)}</p>
      ) : results.data.length === 0 ? (
        <p className="py-6 text-center text-sm text-muted">{t("edit.match.noBookMatches")}</p>
      ) : (
        <ul className="max-h-[55vh] space-y-2 overflow-y-auto pr-1">
          {results.data.map((m) => {
            const isCurrent = current(m);
            return (
              <li key={m.asin ?? m.openlibrary_id}>
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => apply.mutate(m)}
                  className={cn(
                    "flex w-full gap-3 rounded-lg border p-2 text-left transition-colors hover:border-accent hover:bg-surface-2",
                    isCurrent ? "border-accent/60 bg-surface-2" : "border-border",
                  )}
                >
                  <div className="size-20 shrink-0 overflow-hidden rounded bg-surface-3">
                    {m.poster_url ? <img src={m.poster_url} alt="" loading="lazy" className="size-full object-cover" /> : null}
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-baseline gap-2">
                      <span className="truncate font-semibold">{m.title}</span>
                      {m.year ? <span className="shrink-0 text-sm text-muted">{m.year}</span> : null}
                      {isCurrent ? (
                        <span className="ml-auto inline-flex shrink-0 items-center gap-1 text-xs font-medium text-accent">
                          <Check className="size-3.5" /> {t("edit.match.current")}
                        </span>
                      ) : null}
                    </div>
                    {m.authors.length ? <div className="truncate text-sm">{t("book.by", { names: m.authors.join(", ") })}</div> : null}
                    {m.narrators.length ? (
                      <div className="truncate text-xs text-muted">{t("book.readBy", { names: m.narrators.join(", ") })}</div>
                    ) : null}
                    <div className="mt-1 text-xs text-subtle">
                      {m.asin ? t("edit.match.audible", { asin: m.asin }) : t("edit.match.openLibrary", { id: m.openlibrary_id })}
                    </div>
                  </div>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </Modal>
  );
}

// ---------------------------------------------------------------- artwork

function ArtworkDialog({ item, kind, onClose }: { item: ItemDetail; kind: ArtworkKind; onClose: () => void }) {
  const { t } = useTranslation("item");
  const toast = useToast();
  const refresh = useItemRefresh(item.id);
  const fileRef = useRef<HTMLInputElement>(null);
  const options = useQuery({
    queryKey: ["artwork", item.id, kind],
    queryFn: ({ signal }) => api.get<ArtworkCandidate[]>(`/api/items/${item.id}/artwork/${kind}`, { signal }),
    retry: false,
  });
  const choose = useMutation({
    mutationFn: (url: string) => api.put<ItemDetail>(`/api/items/${item.id}/artwork/${kind}`, { url }),
    onSuccess: (detail) => done(detail),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const upload = useMutation({
    mutationFn: (file: File) =>
      request<ItemDetail>(`/api/items/${item.id}/artwork/${kind}/upload`, { method: "POST", rawBody: file }),
    onSuccess: (detail) => done(detail),
    onError: (e) => toast.error(errorMessage(e)),
  });
  const reset = useMutation({
    mutationFn: () => api.del<void>(`/api/items/${item.id}/artwork/${kind}`),
    onSuccess: () => {
      refresh();
      toast.success(t("edit.artwork.resetDone"));
      onClose();
    },
    onError: (e) => toast.error(errorMessage(e)),
  });
  const busy = choose.isPending || upload.isPending || reset.isPending;
  const done = (detail: ItemDetail) => {
    refresh(detail);
    toast.success(t("edit.artwork.updated"));
    onClose();
  };
  const locked = (item.locked_fields ?? []).includes(kind);
  const book = item.kind === "audiobook";
  useEffect(() => {
    if (options.isError) fileRef.current?.focus();
  }, [options.isError]);

  return (
    <Modal
      open
      onClose={onClose}
      title={t(book ? "edit.artwork.titles.cover" : `edit.artwork.titles.${kind}`)}
      size="xl"
      locked={busy}
      footer={
        <>
          {locked ? (
            <Button variant="ghost" icon={<RotateCcw className="size-4" />} onClick={() => reset.mutate()} disabled={busy}>
              {t("edit.artwork.reset")}
            </Button>
          ) : null}
          <input
            ref={fileRef}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={(e) => {
              const file = e.target.files?.[0];
              if (file) upload.mutate(file);
              e.target.value = "";
            }}
          />
          <Button icon={<ImageUp className="size-4" />} onClick={() => fileRef.current?.click()} disabled={busy}>
            {upload.isPending ? t("edit.artwork.uploading") : t("edit.artwork.upload")}
          </Button>
        </>
      }
    >
      {options.isPending ? (
        <div className="flex justify-center py-12">
          <Spinner />
        </div>
      ) : options.isError ? (
        <p className="py-8 text-center text-sm text-muted">{t("edit.artwork.loadError", { error: errorMessage(options.error) })}</p>
      ) : options.data.length === 0 ? (
        <p className="py-8 text-center text-sm text-muted">{t(book ? "edit.artwork.noCovers" : "edit.artwork.none")}</p>
      ) : (
        <ArtworkGrid
          kind={kind}
          square={book}
          options={options.data}
          disabled={busy}
          onPick={(url) => choose.mutate(url)}
        />
      )}
    </Modal>
  );
}

function ArtworkGrid({
  kind,
  square = false,
  options,
  disabled,
  onPick,
}: {
  kind: ArtworkKind;
  /** Square pictures: book covers. */
  square?: boolean;
  options: ArtworkCandidate[];
  disabled: boolean;
  onPick: (url: string) => void;
}): ReactNode {
  const poster = kind === "poster";
  return (
    <div
      className={cn(
        "grid max-h-[60vh] gap-3 overflow-y-auto pr-1",
        poster ? "grid-cols-3 sm:grid-cols-4 md:grid-cols-5" : "grid-cols-2 md:grid-cols-3",
      )}
    >
      {options.map((o) => (
        <button
          key={o.url}
          type="button"
          disabled={disabled}
          onClick={() => onPick(o.url)}
          className="group relative overflow-hidden rounded-md bg-surface-3 ring-1 ring-white/5 transition hover:ring-2 hover:ring-accent focus-visible:ring-2 focus-visible:ring-accent"
        >
          <img
            src={o.preview_url}
            alt=""
            loading="lazy"
            className={cn("w-full object-cover", square ? "aspect-square" : poster ? "aspect-[2/3]" : "aspect-video")}
          />
          {o.width > 0 ? (
            <span className="absolute inset-x-0 bottom-0 flex justify-between bg-black/60 px-1.5 py-0.5 text-[10px] text-white/85 opacity-0 transition-opacity group-hover:opacity-100">
              <span>{o.language?.toUpperCase() ?? "—"}</span>
              <span>
                {o.width}×{o.height}
              </span>
            </span>
          ) : null}
        </button>
      ))}
    </div>
  );
}

/**
 * Which ratings a content rating's age is read with, as the server reads
 * it: the country written before it ("GB:15"), else the item's while it's
 * the rating the item has, else the server's country's. A country without
 * ratings here reads it by the US's names and others in common use.
 */
function ratingCountry(
  item: ItemDetail,
  systems: RatingSystems | undefined,
  value: string,
  initial: string,
): string | null {
  if (value.trim() === "") return null;
  const country = writtenCountry(systems, value) ?? (value === initial ? item.content_rating_country : undefined);
  if (country) return translate("edit.details.ratingCountry", { ns: "item", country: systemName(country) });
  if (systems && !systems.systems.some((s) => s.country === systems.region)) {
    return translate("edit.details.ratingCommon", { ns: "item" });
  }
  return translate("edit.details.ratingRegion", { ns: "item" });
}

/** A field's hint with the rating's country after it, when there's one. */
function withRatingCountry(country: string | null, hint: ReactNode): ReactNode {
  if (!country) return hint;
  if (!hint) return country;
  return (
    <span className="inline-flex flex-wrap items-center gap-x-2">
      {hint}
      <span aria-hidden>·</span>
      <span>{country}</span>
    </span>
  );
}

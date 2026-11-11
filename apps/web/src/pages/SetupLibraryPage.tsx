import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { Trans, useTranslation } from "react-i18next";
import { FolderOpen, Plus, X } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useCreateLibrary, useFs, useLibraries } from "@/api/hooks";
import type { Library, LibraryKind } from "@/api/types";
import { ErrorScreen, RequireAdmin, Splash } from "@/auth";
import { FolderPicker } from "@/components/FolderPicker";
import { LibraryIcon } from "@/components/Sidebar";
import { Button, IconButton, TextInput } from "@/components/ui";
import { cn } from "@/lib/cn";
import { useDocumentTitle } from "@/lib/hooks";
import { SETUP_PROGRESS } from "@/lib/setupFrame";
import { KINDS } from "./settings/LibraryFields";
import { SetupActions, SetupStep } from "./SetupLayout";

/** A library to be, as a row of Your libraries shows it. */
export interface LibraryRow {
  key: number;
  kind: LibraryKind;
  /** As typed; a suggested row left blank takes its type's name. */
  name: string;
  /** At least one, `""` until it's given. */
  folders: string[];
  /** One of the rows the step starts with (one a type), not one added. */
  suggested: boolean;
}

export type RowProblem = "noFolder" | "noName" | "nameTaken";

/** The rows the step starts with: one for each type not added yet, under its name. */
export function startingRows(libraries: readonly Library[], defaultName: (kind: LibraryKind) => string): LibraryRow[] {
  return KINDS.filter((kind) => !libraries.some((l) => l.kind === kind)).map((kind, key) => ({
    key,
    kind,
    name: defaultName(kind),
    folders: [""],
    suggested: true,
  }));
}

/** A row's folders, each once. */
export function rowFolders(row: LibraryRow): string[] {
  return Array.from(new Set(row.folders.map((f) => f.trim()).filter(Boolean)));
}

/** The name a row's library is added under; `""` when it has none. */
export function rowName(row: LibraryRow, defaultName: (kind: LibraryKind) => string): string {
  return row.name.trim() || (row.suggested ? defaultName(row.kind) : "");
}

/**
 * What stops each row being added, by its key. A suggested row left empty
 * is left out, not a problem; a row added and left empty is one, since it
 * was asked for.
 */
export function rowProblems(rows: readonly LibraryRow[], libraries: readonly Library[], defaultName: (kind: LibraryKind) => string): Record<number, RowProblem> {
  const problems: Record<number, RowProblem> = {};
  const names = new Set(libraries.map((l) => l.name.trim().toLowerCase()));
  for (const row of rows) {
    if (rowFolders(row).length === 0) {
      if (!row.suggested) problems[row.key] = "noFolder";
      continue;
    }
    const name = rowName(row, defaultName);
    if (!name) problems[row.key] = "noName";
    else if (names.has(name.toLowerCase())) problems[row.key] = "nameTaken";
    names.add(name.toLowerCase());
  }
  return problems;
}

/** `rows` with `row` after the others of its type, else where its type comes. */
export function insertRow(rows: readonly LibraryRow[], row: LibraryRow): LibraryRow[] {
  const order = KINDS.indexOf(row.kind);
  const at = rows.findIndex((r) => KINDS.indexOf(r.kind) > order);
  return at < 0 ? [...rows, row] : [...rows.slice(0, at), row, ...rows.slice(at)];
}

/**
 * The first run's Start fresh (`/setup/library`): a row for each library
 * to add, movies, shows and music to start with, and more of any type (Kids
 * Movies). Start watching adds those with a folder, each starting its scan,
 * and goes Home, as the Plex import's last step does. Later leaves with
 * none; Home's empty state leads back to adding one.
 */
export function SetupLibraryPage() {
  const { t } = useTranslation(["pages", "settings"]);
  useDocumentTitle(t("setupLibrary.title"));
  return (
    <SetupStep progress={SETUP_PROGRESS.libraries} title={t("setupLibrary.title")} subtitle={t("setupLibrary.subtitle")}>
      <RequireAdmin>
        <SetupLibrary />
      </RequireAdmin>
    </SetupStep>
  );
}

function SetupLibrary() {
  const libraries = useLibraries();
  if (libraries.isPending) return <Splash />;
  if (libraries.isError) return <ErrorScreen message={errorMessage(libraries.error)} onRetry={() => void libraries.refetch()} />;
  return <LibraryRows libraries={libraries.data} />;
}

function LibraryRows({ libraries }: { libraries: Library[] }) {
  const { t } = useTranslation(["pages", "settings", "common"]);
  const navigate = useNavigate();
  const create = useCreateLibrary();
  const roots = useFs(null);
  // In a container with nothing mounted, Browse has no media to show.
  const unmounted = roots.data?.container === true && roots.data.mounts?.length === 0;
  const defaultName = (kind: LibraryKind) => t(`settings:libraries.kinds.${kind}.defaultName`);
  const [rows, setRows] = useState(() => startingRows(libraries, defaultName));
  const nextKey = useRef(rows.length);
  // The row whose name is asked for, once added.
  const [named, setNamed] = useState<number | null>(null);
  const [picking, setPicking] = useState<{ key: number; index: number } | null>(null);
  // Problems show once Start watching has been pressed, not while typing.
  const [checked, setChecked] = useState(false);
  const [failed, setFailed] = useState<{ key: number; message: string } | null>(null);
  const [nothing, setNothing] = useState(false);
  const [busy, setBusy] = useState(false);
  const problems = checked ? rowProblems(rows, libraries, defaultName) : {};
  const started = libraries.length > 0;

  const change = (key: number, update: (row: LibraryRow) => LibraryRow) => {
    setRows((rs) => rs.map((r) => (r.key === key ? update(r) : r)));
    setNothing(false);
    if (failed?.key === key) setFailed(null);
  };
  const addRow = (kind: LibraryKind) => {
    const key = nextKey.current++;
    setRows((rs) => insertRow(rs, { key, kind, name: "", folders: [""], suggested: false }));
    setNamed(key);
  };

  const startWatching = async () => {
    setChecked(true);
    setFailed(null);
    if (Object.keys(rowProblems(rows, libraries, defaultName)).length > 0) return;
    const adding = rows.filter((r) => rowFolders(r).length > 0);
    if (adding.length === 0 && !started) {
      setNothing(true);
      return;
    }
    setBusy(true);
    for (const row of adding) {
      try {
        await create.mutateAsync({ name: rowName(row, defaultName), kind: row.kind, paths: rowFolders(row) });
        // Added: it's listed with the others, and pressing again doesn't add it twice.
        setRows((rs) => rs.filter((r) => r.key !== row.key));
      } catch (e) {
        setFailed({ key: row.key, message: errorMessage(e) });
        setBusy(false);
        return;
      }
    }
    void navigate("/", { replace: true });
  };

  const pickedRow = picking ? rows.find((r) => r.key === picking.key) : undefined;

  return (
    <>
      <div className="space-y-5">
        {unmounted ? (
          <p className="rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs">
            <Trans t={t} i18nKey="settings:libraries.form.nothingMounted" components={{ path: <span className="font-mono" /> }} />
          </p>
        ) : null}
        {started ? (
          <ul aria-label={t("setupLibrary.added")} className="divide-y divide-border border-y border-border">
            {libraries.map((library) => (
              <li key={library.id} className="flex items-center gap-3 py-2.5">
                <LibraryIcon kind={library.kind} className="size-5 shrink-0 text-accent" />
                <span className="min-w-0 flex-1">
                  <span className="block truncate text-sm font-medium">{library.name}</span>
                  <span className="block truncate font-mono text-xs text-subtle">{library.paths.join(", ")}</span>
                </span>
              </li>
            ))}
          </ul>
        ) : null}
        {rows.length > 0 ? (
          <ul aria-label={t("setupLibrary.toAdd")} className="space-y-3">
            {rows.map((row) => (
              <li key={row.key}>
                <Row
                  row={row}
                  defaultName={defaultName(row.kind)}
                  askName={named === row.key}
                  problem={problems[row.key]}
                  failure={failed?.key === row.key ? failed.message : undefined}
                  onChange={(update) => change(row.key, update)}
                  onBrowse={(index) => setPicking({ key: row.key, index })}
                  onRemove={() => setRows((rs) => rs.filter((r) => r.key !== row.key))}
                />
              </li>
            ))}
          </ul>
        ) : null}
        <AddAnother onAdd={addRow} />
        {nothing ? (
          <p role="alert" className="text-sm text-danger">
            {t("setupLibrary.pickOne")}
          </p>
        ) : null}
        <SetupActions
          back={
            // Until a library is added, bringing over Plex is still a step back.
            started ? null : (
              <Button variant="ghost" disabled={busy} onClick={() => void navigate("/setup/start", { replace: true })}>
                {t("common:actions.back")}
              </Button>
            )
          }
        >
          {started ? null : (
            <Button variant="ghost" disabled={busy} onClick={() => void navigate("/", { replace: true })}>
              {t("setupLibrary.later")}
            </Button>
          )}
          <Button variant="primary" loading={busy} onClick={() => void startWatching()}>
            {t("settings:plexImport.startWatching")}
          </Button>
        </SetupActions>
      </div>
      <FolderPicker
        open={picking !== null}
        onClose={() => setPicking(null)}
        initialPath={(picking && pickedRow?.folders[picking.index]?.trim()) || null}
        onSelect={(path) => {
          if (picking) change(picking.key, (r) => ({ ...r, folders: r.folders.map((f, i) => (i === picking.index ? path : f)) }));
          setPicking(null);
        }}
      />
    </>
  );
}

/** A library to be: its type, its name and its folders. */
function Row({
  row,
  defaultName,
  askName,
  problem,
  failure,
  onChange,
  onBrowse,
  onRemove,
}: {
  row: LibraryRow;
  defaultName: string;
  /** Just added: the name is asked for first. */
  askName: boolean;
  problem?: RowProblem;
  /** Why the server didn't add it. */
  failure?: string;
  onChange: (update: (row: LibraryRow) => LibraryRow) => void;
  onBrowse: (index: number) => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation(["pages", "settings", "common"]);
  const nameInput = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (askName) nameInput.current?.focus();
  }, [askName]);
  const shown = row.name.trim() || (row.suggested ? defaultName : t(`settings:libraries.kinds.${row.kind}.label`));
  const last = row.folders[row.folders.length - 1] ?? "";
  return (
    <div className="rounded-lg border border-border bg-surface-2 p-3.5">
      <div className="flex items-center gap-3">
        <div className="flex size-9 shrink-0 items-center justify-center rounded-md bg-surface-3 text-accent">
          <LibraryIcon kind={row.kind} className="size-4.5" />
        </div>
        <TextInput
          ref={nameInput}
          value={row.name}
          placeholder={row.suggested ? defaultName : t(`setupLibrary.namePlaceholder.${row.kind}`)}
          aria-label={t("setupLibrary.nameOf", { type: t(`settings:libraries.kinds.${row.kind}.label`) })}
          onChange={(e) => onChange((r) => ({ ...r, name: e.target.value }))}
          className={cn(
            "h-9 min-w-0 flex-1 font-medium",
            // The type's own name, there to be changed; a library added is named here first.
            row.suggested && "border-transparent bg-transparent hover:border-border focus:bg-surface-2",
          )}
        />
        {row.suggested ? null : (
          <IconButton label={t("setupLibrary.remove", { name: shown })} size="sm" onClick={onRemove}>
            <X className="size-4" />
          </IconButton>
        )}
      </div>
      <div className="mt-3 space-y-2">
        {row.folders.map((folder, index) => (
          <div key={index} className="flex items-center gap-2">
            <TextInput
              value={folder}
              spellCheck={false}
              placeholder={index === 0 ? t(`setupLibrary.where.${row.kind}`) : t("settings:libraries.form.typePath")}
              aria-label={t("setupLibrary.folderOf", { name: shown })}
              onChange={(e) => onChange((r) => ({ ...r, folders: r.folders.map((f, i) => (i === index ? e.target.value : f)) }))}
              className="h-9 min-w-0 flex-1 font-mono text-xs"
            />
            <Button size="sm" icon={<FolderOpen className="size-4" />} aria-label={t("setupLibrary.browseFor", { name: shown })} onClick={() => onBrowse(index)}>
              {/* On a phone the path needs the room. */}
              <span className="hidden sm:inline">{t("settings:libraries.form.browse")}</span>
            </Button>
            {index > 0 ? (
              <IconButton
                label={t("settings:libraries.form.removePath", { path: folder.trim() || t("setupLibrary.thisFolder") })}
                size="sm"
                onClick={() => onChange((r) => ({ ...r, folders: r.folders.filter((_, i) => i !== index) }))}
              >
                <X className="size-4" />
              </IconButton>
            ) : null}
          </div>
        ))}
        {/* Another folder, once this one's given: one library can hold several. */}
        {last.trim() ? (
          <button
            type="button"
            onClick={() => onChange((r) => ({ ...r, folders: [...r.folders, ""] }))}
            className="flex items-center gap-1 text-xs font-medium text-muted hover:text-fg"
          >
            <Plus className="size-3.5" />
            {t("setupLibrary.addFolder")}
          </button>
        ) : null}
      </div>
      {problem || failure ? (
        <p role="alert" className="mt-2 text-xs text-danger">
          {failure ?? (problem ? t(`setupLibrary.problem.${problem}`) : null)}
        </p>
      ) : null}
    </div>
  );
}

/** Another library of any type, such as Kids Movies, as a row of its own. */
function AddAnother({ onAdd }: { onAdd: (kind: LibraryKind) => void }) {
  const { t } = useTranslation(["pages", "settings"]);
  const id = "setup-add-another";
  return (
    <div role="group" aria-labelledby={id} className="flex flex-wrap items-center gap-2">
      <span id={id} className="w-full text-sm text-muted sm:me-1 sm:w-auto">
        {t("setupLibrary.addAnother")}
      </span>
      {KINDS.map((kind) => (
        <Button key={kind} size="sm" variant="ghost" icon={<LibraryIcon kind={kind} className="size-4" />} onClick={() => onAdd(kind)}>
          {t(`settings:libraries.kinds.${kind}.label`)}
        </Button>
      ))}
    </div>
  );
}

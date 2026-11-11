import { useEffect, useRef, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { ChevronDown, ChevronRight, CircleCheck, CircleX, FolderOpen, TriangleAlert } from "lucide-react";
import { api } from "@/api/client";
import { useFolderCheck, type ImportPlan, type PlanLibrary } from "@/api/plexImport";
import type { FsListing, Library } from "@/api/types";
import { FolderPicker } from "@/components/FolderPicker";
import { LibraryIcon } from "@/components/Sidebar";
import { Badge, Button, Field, Select, TextInput, Toggle } from "@/components/ui";
import { cn } from "@/lib/cn";
import { useDebouncedValue } from "@/lib/hooks";
import {
  bringsAny,
  fillIn,
  libraryProblems,
  swapFills,
  swapOf,
  type LibraryChoice,
  type LibraryChoices,
  type LibraryProblem,
  type Target,
} from "./choices";
import { StepFrame } from "./PlexImport";

export function LibrariesStep({
  plan,
  libraries,
  choices,
  onChange,
  onBack,
  onNext,
}: {
  plan: ImportPlan;
  /** Pley's libraries. */
  libraries: Library[];
  choices: LibraryChoices;
  onChange: (choices: LibraryChoices) => void;
  onBack: () => void;
  onNext: () => void;
}) {
  const { t } = useTranslation(["settings", "common"]);
  // Problems show once Continue has been pressed, not while typing.
  const [checked, setChecked] = useState(false);
  const problems = libraryProblems(plan, choices);
  const nothing = !bringsAny(choices);
  // The choices as they are when a folder check comes back.
  const latest = useRef(choices);
  useEffect(() => {
    latest.current = choices;
  }, [choices]);

  // A folder given says where the others are too: those the same swap
  // finds there are filled in.
  const fillFrom = async (location: string, path: string) => {
    if (!path.trim()) return;
    const fills = swapFills(plan, latest.current, swapOf(location, path.trim())).filter((f) => f.location !== location);
    const there = await Promise.all(
      fills.map((f) => api.post<FsListing>("/api/fs", { path: f.path }).then(
        () => true,
        () => false,
      )),
    );
    const found = fills.filter((_, i) => there[i]);
    if (found.length > 0) onChange(fillIn(latest.current, found));
  };

  const next = () => {
    setChecked(true);
    if (Object.keys(problems).length === 0 && !nothing) onNext();
  };

  return (
    <StepFrame
      stage="libraries"
      title={t("plexImport.libraries.title", { server: plan.server })}
      description={t("plexImport.libraries.description")}
      actions={
        <>
          <Button variant="ghost" onClick={onBack}>
            {t("common:actions.back")}
          </Button>
          <Button variant="primary" onClick={next} disabled={plan.libraries.length === 0}>
            {t("common:actions.continue")}
          </Button>
        </>
      }
    >
      {plan.libraries.length === 0 ? (
        <p className="rounded-lg border border-dashed border-border-strong px-4 py-6 text-center text-sm text-muted">{t("plexImport.libraries.none")}</p>
      ) : (
        <ul className="space-y-3">
          {plan.libraries.map((library) => {
            const choice = choices[library.key];
            if (!choice) return null;
            return (
              <li key={library.key}>
                <LibraryRow
                  library={library}
                  libraries={libraries}
                  choice={choice}
                  problem={checked ? problems[library.key] : undefined}
                  onChange={(c) => onChange({ ...choices, [library.key]: c })}
                  onFolderGiven={(location, path) => void fillFrom(location, path)}
                />
              </li>
            );
          })}
        </ul>
      )}
      {checked && nothing && plan.libraries.length > 0 ? (
        <p role="alert" className="text-sm text-danger">
          {t("plexImport.libraries.pickOne")}
        </p>
      ) : null}
    </StepFrame>
  );
}

function LibraryRow({
  library,
  libraries,
  choice,
  problem,
  onChange,
  onFolderGiven,
}: {
  library: PlanLibrary;
  libraries: Library[];
  choice: LibraryChoice;
  problem?: LibraryProblem;
  onChange: (choice: LibraryChoice) => void;
  /** A folder said, for the others to be looked for the same way. */
  onFolderGiven: (location: string, path: string) => void;
}) {
  const { t } = useTranslation(["settings", "common"]);
  // The Plex folder whose place for Pley is being picked.
  const [picking, setPicking] = useState<string | null>(null);
  const unknown = Object.values(choice.folders).some((f) => !f.trim());
  const [showFolders, setShowFolders] = useState(choice.target === "new" || (unknown && choice.target !== "leave"));
  const sameKind = libraries.filter((l) => l.kind === library.kind);
  const status = library.pairing.status;
  const id = `plex-library-${library.key}`;
  // Left out because Pley can't open its folders: they're asked for here,
  // and one given brings it.
  const asking = choice.target === "leave" && !choice.left && status === "not_in_pley";
  const foldersOpen = asking || (choice.target !== "leave" && (showFolders || choice.target === "new" || !!problem));
  // Going to one of Pley's libraries: its folders fold away.
  const listed = choice.target !== "new" && choice.target !== "leave";

  const setTarget = (value: string) => {
    const target: Target = value === "new" || value === "leave" ? value : Number(value);
    onChange({ ...choice, target, left: target === "leave" });
  };
  const setFolder = (location: string, path: string) => {
    const target = asking && path.trim() ? "new" : choice.target;
    onChange({ ...choice, target, folders: { ...choice.folders, [location]: path } });
  };
  const size = librarySize(library, t);

  return (
    <div className="rounded-lg border border-border bg-surface-2 p-3.5">
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex size-9 shrink-0 items-center justify-center rounded-md bg-surface-3 text-accent">
          <LibraryIcon kind={library.kind} className="size-4.5" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className="truncate font-medium">{library.title}</span>
            {/* On a server with no libraries, every one would say Not in Pley. */}
            {libraries.length > 0 ? (
              <Badge tone={status === "matched" ? "success" : status === "unsure" ? "warning" : "neutral"}>{t(`plexImport.libraries.status.${status}`)}</Badge>
            ) : null}
          </div>
          {size ? <p className="text-xs text-muted">{size}</p> : null}
        </div>
        {/* With no Pley library of its kind to go to, it's brought as a new one or not at all. */}
        {sameKind.length > 0 ? (
          <>
            <label htmlFor={`${id}-target`} className="sr-only">
              {t("plexImport.libraries.goesTo", { name: library.title })}
            </label>
            <Select id={`${id}-target`} value={String(choice.target)} onChange={(e) => setTarget(e.target.value)} className="w-full sm:w-56">
              <optgroup label={t("plexImport.libraries.pleyLibraries")}>
                {sameKind.map((l) => (
                  <option key={l.id} value={l.id}>
                    {l.name}
                  </option>
                ))}
              </optgroup>
              <option value="new">{t("plexImport.libraries.newLibrary")}</option>
              <option value="leave">{t("plexImport.libraries.leaveOut")}</option>
            </Select>
          </>
        ) : (
          <Toggle
            checked={choice.target !== "leave"}
            onChange={(on) => setTarget(on ? "new" : "leave")}
            label={t("plexImport.libraries.bring", { name: library.title })}
          />
        )}
      </div>

      {choice.target !== "leave" && status === "unsure" ? <p className="mt-2 text-xs text-warning">{t("plexImport.libraries.unsureHint")}</p> : null}

      {choice.target === "new" ? <NewName library={library} choice={choice} inputId={`${id}-name`} onChange={onChange} /> : null}

      {foldersOpen || listed ? (
        <div className="mt-3">
          {listed ? (
            <button
              type="button"
              aria-expanded={foldersOpen}
              onClick={() => setShowFolders(!foldersOpen)}
              className="flex items-center gap-1 text-xs font-medium text-muted hover:text-fg"
            >
              {foldersOpen ? <ChevronDown className="size-3.5" /> : <ChevronRight className="size-3.5" />}
              {t("plexImport.libraries.folders", { count: library.locations.length })}
            </button>
          ) : null}
          {foldersOpen ? (
            <div className="mt-2 space-y-3">
              {library.locations.map((location, i) => (
                <FolderLine
                  key={location}
                  library={library.key}
                  location={location}
                  inputId={`${id}-folder-${i}`}
                  value={choice.folders[location] ?? ""}
                  onChange={(path) => setFolder(location, path)}
                  onBlur={(path) => onFolderGiven(location, path)}
                  onBrowse={() => setPicking(location)}
                />
              ))}
            </div>
          ) : null}
          {problem ? (
            <p role="alert" className="mt-2 text-xs text-danger">
              {t(`plexImport.libraries.problem.${problem}`)}
            </p>
          ) : null}
        </div>
      ) : null}

      {/* Over the import's dialog in Settings: only the top one answers Escape. */}
      <FolderPicker
        open={picking !== null}
        onClose={() => setPicking(null)}
        initialPath={picking ? choice.folders[picking]?.trim() || null : null}
        onSelect={(path) => {
          if (picking !== null) {
            setFolder(picking, path);
            onFolderGiven(picking, path);
          }
          setPicking(null);
        }}
      />
    </div>
  );
}

/** How much a Plex library holds: "812 movies", "64 shows · 6,509 episodes". */
function librarySize(library: PlanLibrary, t: TFunction<["settings", "common"]>): string | null {
  if (library.items == null) return null;
  const items = library.items;
  const groups = library.groups ?? null;
  switch (library.kind) {
    case "movies":
      return t("plexImport.libraries.size.movies", { count: items });
    case "shows":
      return [groups != null ? t("plexImport.libraries.size.shows", { count: groups }) : null, t("plexImport.libraries.size.episodes", { count: items })]
        .filter(Boolean)
        .join(" · ");
    case "music":
      return [groups != null ? t("plexImport.libraries.size.artists", { count: groups }) : null, t("plexImport.libraries.size.tracks", { count: items })]
        .filter(Boolean)
        .join(" · ");
    default:
      return null;
  }
}

/** A new library's name: Plex's, until renamed. */
function NewName({
  library,
  choice,
  inputId,
  onChange,
}: {
  library: PlanLibrary;
  choice: LibraryChoice;
  inputId: string;
  onChange: (choice: LibraryChoice) => void;
}) {
  const { t } = useTranslation("settings");
  const [renaming, setRenaming] = useState(false);
  if (renaming) {
    return (
      <div className="mt-3">
        <Field label={t("plexImport.libraries.name")} htmlFor={inputId}>
          <TextInput id={inputId} autoFocus value={choice.name} placeholder={library.title} onChange={(e) => onChange({ ...choice, name: e.target.value })} />
        </Field>
      </div>
    );
  }
  return (
    <p className="mt-2 text-sm text-muted">
      <Trans
        t={t}
        i18nKey="plexImport.libraries.newNamed"
        values={{ name: choice.name.trim() || library.title }}
        components={{ name: <span className="font-medium text-fg" /> }}
      />{" "}
      <button type="button" className="whitespace-nowrap text-accent hover:underline" onClick={() => setRenaming(true)}>
        {t("plexImport.libraries.rename")}
      </button>
    </p>
  );
}

/** One of Plex's folders, where it is for Pley, and whether Plex's files are there. */
function FolderLine({
  library,
  location,
  inputId,
  value,
  onChange,
  onBlur,
  onBrowse,
}: {
  /** The Plex library's key. */
  library: string;
  location: string;
  inputId: string;
  value: string;
  onChange: (path: string) => void;
  onBlur: (path: string) => void;
  onBrowse: () => void;
}) {
  const { t } = useTranslation(["settings", "common"]);
  // Checked once the path has settled, not at each key.
  const settled = useDebouncedValue(value.trim(), 400);
  const check = useFolderCheck(library, location, settled).data;
  const said = settled === value.trim() && check && check !== "untold" ? check : null;
  const Icon = said === "found" ? CircleCheck : said === "none_found" ? TriangleAlert : CircleX;
  return (
    <div className="space-y-1.5">
      <label htmlFor={inputId} className="block text-xs text-muted">
        <Trans t={t} i18nKey="plexImport.libraries.plexSees" values={{ folder: location }} components={{ path: <span className="break-all font-mono text-fg" /> }} />
      </label>
      <div className="flex items-center gap-2">
        <TextInput
          id={inputId}
          value={value}
          spellCheck={false}
          className="h-8 min-w-0 flex-1 font-mono text-xs"
          onChange={(e) => onChange(e.target.value)}
          onBlur={(e) => onBlur(e.target.value)}
        />
        <Button size="sm" icon={<FolderOpen className="size-4" />} aria-label={t("plexImport.libraries.browseFor", { folder: location })} onClick={onBrowse}>
          {t("libraries.form.browse")}
        </Button>
      </div>
      {said ? (
        <p className={cn("flex items-center gap-1.5 text-xs", said === "found" ? "text-success" : said === "none_found" ? "text-warning" : "text-danger")}>
          <Icon className="size-3.5 shrink-0" aria-hidden />
          {t(`plexImport.libraries.check.${said}`)}
        </p>
      ) : null}
    </div>
  );
}

import { useEffect, useRef, useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { ChevronDown, ChevronRight, FolderOpen } from "lucide-react";
import { api } from "@/api/client";
import type { ImportPlan, PlanLibrary } from "@/api/plexImport";
import type { FsListing, Library } from "@/api/types";
import { FolderPicker } from "@/components/FolderPicker";
import { LibraryIcon } from "@/components/Sidebar";
import { Badge, Button, Field, Select, TextInput } from "@/components/ui";
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
    onChange({ ...choice, target, folders: { ...choice.folders, [location]: path }, found: choice.found.filter((l) => l !== location) });
  };
  const foundShown = choice.found.some((l) => choice.folders[l]?.trim());

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
        </div>
        <label htmlFor={`${id}-target`} className="sr-only">
          {t("plexImport.libraries.goesTo", { name: library.title })}
        </label>
        <Select id={`${id}-target`} value={String(choice.target)} onChange={(e) => setTarget(e.target.value)} className="w-full sm:w-56">
          {sameKind.length > 0 ? (
            <optgroup label={t("plexImport.libraries.pleyLibraries")}>
              {sameKind.map((l) => (
                <option key={l.id} value={l.id}>
                  {l.name}
                </option>
              ))}
            </optgroup>
          ) : null}
          <option value="new">{t("plexImport.libraries.newLibrary")}</option>
          <option value="leave">{t("plexImport.libraries.leaveOut")}</option>
        </Select>
      </div>

      {choice.target !== "leave" && status === "unsure" ? <p className="mt-2 text-xs text-warning">{t("plexImport.libraries.unsureHint")}</p> : null}

      {choice.target === "new" ? (
        <div className="mt-3">
          <Field label={t("plexImport.libraries.name")} htmlFor={`${id}-name`}>
            <TextInput id={`${id}-name`} value={choice.name} placeholder={library.title} onChange={(e) => onChange({ ...choice, name: e.target.value })} />
          </Field>
        </div>
      ) : null}

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
                <div key={location} className="space-y-1.5">
                  <label htmlFor={`${id}-folder-${i}`} className="block text-xs text-muted">
                    <Trans
                      t={t}
                      i18nKey="plexImport.libraries.plexSees"
                      values={{ folder: location }}
                      components={{ path: <span className="break-all font-mono text-fg" /> }}
                    />
                  </label>
                  <div className="flex items-center gap-2">
                    <TextInput
                      id={`${id}-folder-${i}`}
                      value={choice.folders[location] ?? ""}
                      spellCheck={false}
                      className="h-8 min-w-0 flex-1 font-mono text-xs"
                      onChange={(e) => setFolder(location, e.target.value)}
                      onBlur={(e) => onFolderGiven(location, e.target.value)}
                    />
                    <Button
                      size="sm"
                      icon={<FolderOpen className="size-4" />}
                      aria-label={t("plexImport.libraries.browseFor", { folder: location })}
                      onClick={() => setPicking(location)}
                    >
                      {t("libraries.form.browse")}
                    </Button>
                  </div>
                </div>
              ))}
              {foundShown ? <p className="text-xs text-subtle">{t("plexImport.libraries.found")}</p> : null}
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

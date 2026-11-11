import { useState, type FormEvent } from "react";
import { useNavigate } from "react-router";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { useCreateLibrary, useLibraries } from "@/api/hooks";
import type { Library, LibraryKind } from "@/api/types";
import { ErrorScreen, RequireAdmin, Splash } from "@/auth";
import { FolderPicker } from "@/components/FolderPicker";
import { LibraryIcon } from "@/components/Sidebar";
import { Button } from "@/components/ui";
import { useDocumentTitle } from "@/lib/hooks";
import { draftPaths, emptyDraft, KINDS, LibraryFields, type LibraryDraft } from "./settings/LibraryFields";
import { AuthLayout } from "./AuthLayout";

/** The type to start the next library with: the first not added yet (shows after movies). */
export function nextKind(libraries: readonly Library[]): LibraryKind {
  return KINDS.find((kind) => !libraries.some((l) => l.kind === kind)) ?? "movies";
}

/**
 * The first run's Start fresh (`/setup/library`): libraries are added on the
 * setup card, one after another, each starting its scan, and Start watching
 * goes Home, as the Plex import's last step does. Later leaves with none;
 * Home's empty state leads back to adding one.
 */
export function SetupLibraryPage() {
  const { t } = useTranslation(["pages", "settings"]);
  useDocumentTitle(t("setupLibrary.title"));
  return (
    <AuthLayout title={t("setupLibrary.title")} subtitle={t("setupLibrary.subtitle")}>
      <RequireAdmin>
        <SetupLibrary />
      </RequireAdmin>
    </AuthLayout>
  );
}

function SetupLibrary() {
  const libraries = useLibraries();
  if (libraries.isPending) return <Splash />;
  if (libraries.isError) return <ErrorScreen message={errorMessage(libraries.error)} onRetry={() => void libraries.refetch()} />;
  return <LibraryForm libraries={libraries.data} />;
}

function LibraryForm({ libraries }: { libraries: Library[] }) {
  const { t } = useTranslation(["pages", "settings", "common"]);
  const navigate = useNavigate();
  const create = useCreateLibrary();
  const [draft, setDraft] = useState<LibraryDraft>(() => emptyDraft(nextKind(libraries)));
  const [picker, setPicker] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const started = libraries.length > 0;

  /** Adds the library filled in; whether it was. */
  const add = async (): Promise<boolean> => {
    setError(null);
    const paths = draftPaths(draft);
    if (paths.length === 0) {
      setError(t("settings:libraries.form.atLeastOneFolder"));
      return false;
    }
    try {
      const name = draft.name.trim() || t(`settings:libraries.kinds.${draft.kind}.defaultName`);
      const added = await create.mutateAsync({ name, kind: draft.kind, paths });
      setDraft(emptyDraft(nextKind([...libraries, added])));
      return true;
    } catch (e) {
      setError(errorMessage(e));
      return false;
    }
  };

  const onSubmit = (e: FormEvent) => {
    e.preventDefault();
    void add();
  };

  // Folders filled in when Start watching is pressed are a library too.
  const startWatching = async () => {
    if (draftPaths(draft).length > 0 && !(await add())) return;
    void navigate("/", { replace: true });
  };

  return (
    <>
      <form onSubmit={onSubmit} className="space-y-5">
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
        <LibraryFields draft={draft} onChange={setDraft} onBrowse={() => setPicker(true)} chooseKind />
        {error ? (
          <p role="alert" className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">
            {error}
          </p>
        ) : null}
        <div className="space-y-2">
          {started ? (
            <>
              <Button type="button" variant="primary" size="lg" className="w-full" loading={create.isPending} onClick={() => void startWatching()}>
                {t("settings:plexImport.startWatching")}
              </Button>
              <Button type="submit" className="w-full" disabled={create.isPending}>
                {t("setupLibrary.addAnother")}
              </Button>
            </>
          ) : (
            <>
              <Button type="submit" variant="primary" size="lg" className="w-full" loading={create.isPending}>
                {t("settings:libraries.form.addTitle")}
              </Button>
              {/* Until a library is added, bringing over Plex is still a step back. */}
              <div className="flex gap-2">
                <Button type="button" variant="ghost" className="flex-1" disabled={create.isPending} onClick={() => void navigate("/setup/start", { replace: true })}>
                  {t("common:actions.back")}
                </Button>
                <Button type="button" variant="ghost" className="flex-1" disabled={create.isPending} onClick={() => void navigate("/", { replace: true })}>
                  {t("setupLibrary.later")}
                </Button>
              </div>
            </>
          )}
        </div>
      </form>
      <FolderPicker
        open={picker}
        onClose={() => setPicker(false)}
        initialPath={draft.paths[draft.paths.length - 1] ?? null}
        onSelect={(p) => {
          setDraft((d) => (d.paths.includes(p) ? d : { ...d, paths: [...d.paths, p] }));
          setPicker(false);
        }}
      />
    </>
  );
}

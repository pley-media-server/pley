import { Trans, useTranslation } from "react-i18next";
import { FolderOpen, Plus, Trash2 } from "lucide-react";
import { useFs } from "@/api/hooks";
import type { LibraryKind } from "@/api/types";
import { LibraryIcon } from "@/components/Sidebar";
import { Button, Field, IconButton, TextInput } from "@/components/ui";
import { cn } from "@/lib/cn";

/** Each kind's label, recommended layout and default name are in `libraries.kinds`. */
export const KINDS: LibraryKind[] = ["movies", "shows", "music"];

/** A library being added or edited, as its fields show it. */
export interface LibraryDraft {
  name: string;
  kind: LibraryKind;
  paths: string[];
  /** A path typed but not added to `paths` yet. */
  manual: string;
}

export const emptyDraft = (kind: LibraryKind = "movies"): LibraryDraft => ({ name: "", kind, paths: [], manual: "" });

/** The folders a draft adds: those listed, and one typed but not added yet. */
export const draftPaths = (draft: LibraryDraft): string[] =>
  draft.manual.trim() ? Array.from(new Set([...draft.paths, draft.manual.trim()])) : draft.paths;

/**
 * A library's type (when `chooseKind`), name and folders, for Settings ›
 * Libraries' dialog and setup's Your libraries step. Browse is the
 * parent's: it shows the folder picker and adds what's picked.
 */
export function LibraryFields({
  draft,
  onChange,
  onBrowse,
  chooseKind,
}: {
  draft: LibraryDraft;
  onChange: (draft: LibraryDraft) => void;
  onBrowse: () => void;
  chooseKind: boolean;
}) {
  const { t } = useTranslation(["settings", "common"]);
  const roots = useFs(null);
  // In a container with nothing mounted, Browse has no media to show.
  const unmounted = roots.data?.container === true && roots.data.mounts?.length === 0;
  const set = (change: Partial<LibraryDraft>) => onChange({ ...draft, ...change });
  const addManual = () => {
    const trimmed = draft.manual.trim();
    if (!trimmed) return;
    set({ paths: draft.paths.includes(trimmed) ? draft.paths : [...draft.paths, trimmed], manual: "" });
  };

  return (
    <div className="space-y-5">
      {chooseKind ? (
        <div>
          <div className="mb-2 text-sm font-medium">{t("libraries.form.type")}</div>
          <div className="grid grid-cols-3 gap-2">
            {KINDS.map((k) => (
              <button
                key={k}
                type="button"
                onClick={() => set({ kind: k })}
                aria-pressed={draft.kind === k}
                className={cn(
                  "flex flex-col items-center gap-2 rounded-lg border px-3 py-4 text-sm font-medium transition-colors",
                  draft.kind === k
                    ? "border-accent bg-accent/10 text-fg"
                    : "border-border bg-surface-2 text-muted hover:border-border-strong hover:text-fg",
                )}
              >
                <LibraryIcon kind={k} className={cn("size-6", draft.kind === k && "text-accent")} />
                {t(`libraries.kinds.${k}.label`)}
              </button>
            ))}
          </div>
        </div>
      ) : null}

      <Field label={t("libraries.form.name")} htmlFor="lib-name">
        <TextInput
          id="lib-name"
          value={draft.name}
          placeholder={t(`libraries.kinds.${draft.kind}.defaultName`)}
          onChange={(e) => set({ name: e.target.value })}
        />
      </Field>

      <div>
        <div className="mb-2 flex items-center justify-between">
          <span className="text-sm font-medium">{t("libraries.form.folders")}</span>
          <Button size="sm" icon={<FolderOpen className="size-4" />} onClick={onBrowse}>
            {t("libraries.form.browse")}
          </Button>
        </div>
        {unmounted ? (
          <p className="mb-2 rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs">
            <Trans t={t} i18nKey="libraries.form.nothingMounted" components={{ path: <span className="font-mono" /> }} />
          </p>
        ) : null}
        {draft.paths.length ? (
          <ul className="mb-2 divide-y divide-white/[0.05] rounded-md border border-border">
            {draft.paths.map((p) => (
              <li key={p} className="flex items-center gap-2 px-3 py-1.5">
                <span className="min-w-0 flex-1 truncate font-mono text-xs" title={p}>
                  {p}
                </span>
                <IconButton label={t("libraries.form.removePath", { path: p })} size="sm" onClick={() => set({ paths: draft.paths.filter((x) => x !== p) })}>
                  <Trash2 className="size-4" />
                </IconButton>
              </li>
            ))}
          </ul>
        ) : null}
        <div className="flex gap-2">
          <TextInput
            value={draft.manual}
            onChange={(e) => set({ manual: e.target.value })}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                addManual();
              }
            }}
            placeholder={t("libraries.form.typePath")}
            className="font-mono text-xs"
            aria-label={t("libraries.form.folderPath")}
          />
          <Button icon={<Plus className="size-4" />} onClick={addManual} disabled={!draft.manual.trim()}>
            {t("common:actions.add")}
          </Button>
        </div>
        <p className="mt-2 text-xs text-subtle">
          <Trans
            t={t}
            i18nKey="libraries.form.layout"
            values={{ layout: t(`libraries.kinds.${draft.kind}.layout`) }}
            components={{ path: <span className="font-mono" /> }}
          />
        </p>
      </div>
    </div>
  );
}

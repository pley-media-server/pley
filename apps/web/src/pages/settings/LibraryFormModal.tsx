import { useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { FolderOpen, Plus, Trash2 } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useCreateLibrary, useUpdateLibrary } from "@/api/hooks";
import type { Library, LibraryKind } from "@/api/types";
import { FolderPicker } from "@/components/FolderPicker";
import { Modal } from "@/components/Modal";
import { LibraryIcon } from "@/components/Sidebar";
import { useToast } from "@/components/Toast";
import { Button, Field, IconButton, TextInput } from "@/components/ui";
import { cn } from "@/lib/cn";
import { useOnChange } from "@/lib/hooks";

/** Each kind's label, recommended layout and default name are in `libraries.kinds`. */
const KINDS: LibraryKind[] = ["movies", "shows", "music"];

export function LibraryFormModal({ open, onClose, library }: { open: boolean; onClose: () => void; library?: Library | null }) {
  const { t } = useTranslation(["settings", "common"]);
  const editing = !!library;
  const toast = useToast();
  const create = useCreateLibrary();
  const update = useUpdateLibrary();
  const [name, setName] = useState("");
  const [kind, setKind] = useState<LibraryKind>("movies");
  const [paths, setPaths] = useState<string[]>([]);
  const [manual, setManual] = useState("");
  const [picker, setPicker] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Opening (for another library) starts from its saved values; background
  // refetches don't touch what is being edited.
  useOnChange(open ? (library?.id ?? "new") : null, (key) => {
    if (key === null) return;
    setName(library?.name ?? "");
    setKind(library?.kind ?? "movies");
    setPaths(library?.paths ?? []);
    setManual("");
    setError(null);
  });

  const defaultName = t(`libraries.kinds.${kind}.defaultName`);
  const busy = create.isPending || update.isPending;

  const addPath = (p: string) => {
    const trimmed = p.trim();
    if (!trimmed) return;
    setPaths((list) => (list.includes(trimmed) ? list : [...list, trimmed]));
  };

  const submit = () => {
    setError(null);
    const finalPaths = manual.trim() ? Array.from(new Set([...paths, manual.trim()])) : paths;
    if (finalPaths.length === 0) {
      setError(t("libraries.form.atLeastOneFolder"));
      return;
    }
    const finalName = name.trim() || defaultName;
    if (editing && library) {
      update.mutate(
        { id: library.id, name: finalName, paths: finalPaths },
        {
          onSuccess: () => {
            toast.success(t("libraries.form.updated"));
            onClose();
          },
          onError: (e) => setError(errorMessage(e)),
        },
      );
    } else {
      create.mutate(
        { name: finalName, kind, paths: finalPaths },
        {
          onSuccess: (lib) => {
            toast.success(t("libraries.form.added", { name: lib.name }));
            onClose();
          },
          onError: (e) => setError(errorMessage(e)),
        },
      );
    }
  };

  return (
    <>
      <Modal
        open={open && !picker}
        onClose={onClose}
        locked={busy}
        size="lg"
        title={editing ? t("libraries.form.editTitle", { name: library?.name }) : t("libraries.form.addTitle")}
        footer={
          <>
            <Button variant="ghost" onClick={onClose} disabled={busy}>
              {t("common:actions.cancel")}
            </Button>
            <Button variant="primary" onClick={submit} loading={busy}>
              {editing ? t("libraries.form.saveChanges") : t("libraries.form.addTitle")}
            </Button>
          </>
        }
      >
        <div className="space-y-5">
          {!editing ? (
            <div>
              <div className="mb-2 text-sm font-medium">{t("libraries.form.type")}</div>
              <div className="grid grid-cols-3 gap-2">
                {KINDS.map((k) => (
                  <button
                    key={k}
                    type="button"
                    onClick={() => setKind(k)}
                    aria-pressed={kind === k}
                    className={cn(
                      "flex flex-col items-center gap-2 rounded-lg border px-3 py-4 text-sm font-medium transition-colors",
                      kind === k
                        ? "border-accent bg-accent/10 text-fg"
                        : "border-border bg-surface-2 text-muted hover:border-border-strong hover:text-fg",
                    )}
                  >
                    <LibraryIcon kind={k} className={cn("size-6", kind === k && "text-accent")} />
                    {t(`libraries.kinds.${k}.label`)}
                  </button>
                ))}
              </div>
            </div>
          ) : null}

          <Field label={t("libraries.form.name")} htmlFor="lib-name">
            <TextInput id="lib-name" value={name} placeholder={defaultName} onChange={(e) => setName(e.target.value)} />
          </Field>

          <div>
            <div className="mb-2 flex items-center justify-between">
              <span className="text-sm font-medium">{t("libraries.form.folders")}</span>
              <Button size="sm" icon={<FolderOpen className="size-4" />} onClick={() => setPicker(true)}>
                {t("libraries.form.browse")}
              </Button>
            </div>
            {paths.length ? (
              <ul className="mb-2 divide-y divide-white/[0.05] rounded-md border border-border">
                {paths.map((p) => (
                  <li key={p} className="flex items-center gap-2 px-3 py-1.5">
                    <span className="min-w-0 flex-1 truncate font-mono text-xs" title={p}>
                      {p}
                    </span>
                    <IconButton label={t("libraries.form.removePath", { path: p })} size="sm" onClick={() => setPaths((list) => list.filter((x) => x !== p))}>
                      <Trash2 className="size-4" />
                    </IconButton>
                  </li>
                ))}
              </ul>
            ) : null}
            <div className="flex gap-2">
              <TextInput
                value={manual}
                onChange={(e) => setManual(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    addPath(manual);
                    setManual("");
                  }
                }}
                placeholder={t("libraries.form.typePath")}
                className="font-mono text-xs"
                aria-label={t("libraries.form.folderPath")}
              />
              <Button
                icon={<Plus className="size-4" />}
                onClick={() => {
                  addPath(manual);
                  setManual("");
                }}
                disabled={!manual.trim()}
              >
                {t("common:actions.add")}
              </Button>
            </div>
            <p className="mt-2 text-xs text-subtle">
              <Trans
                t={t}
                i18nKey="libraries.form.layout"
                values={{ layout: t(`libraries.kinds.${kind}.layout`) }}
                components={{ path: <span className="font-mono" /> }}
              />
            </p>
          </div>

          {error ? <p className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">{error}</p> : null}
        </div>
      </Modal>
      <FolderPicker
        open={open && picker}
        onClose={() => setPicker(false)}
        initialPath={paths[paths.length - 1] ?? null}
        onSelect={(p) => {
          addPath(p);
          setPicker(false);
        }}
      />
    </>
  );
}

import { useState } from "react";
import { Trans, useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { useCreateLibrary, useUpdateLibrary } from "@/api/hooks";
import type { Library } from "@/api/types";
import { FolderPicker } from "@/components/FolderPicker";
import { ConfirmModal, Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Button, Toggle } from "@/components/ui";
import { useOnChange } from "@/lib/hooks";
import { draftPaths, emptyDraft, LibraryFields, type LibraryDraft } from "./LibraryFields";

export function LibraryFormModal({ open, onClose, library }: { open: boolean; onClose: () => void; library?: Library | null }) {
  const { t } = useTranslation(["settings", "common"]);
  const editing = !!library;
  const toast = useToast();
  const create = useCreateLibrary();
  const update = useUpdateLibrary();
  const [draft, setDraft] = useState<LibraryDraft>(emptyDraft());
  const [tmdbCollections, setTmdbCollections] = useState(true);
  const [picker, setPicker] = useState(false);
  // Turning collections from TMDB off removes them, edits and all: asked first.
  const [confirmingOff, setConfirmingOff] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Opening (for another library) starts from its saved values; background
  // refetches don't touch what is being edited.
  useOnChange(open ? (library?.id ?? "new") : null, (key) => {
    if (key === null) return;
    setDraft(library ? { name: library.name, kind: library.kind, paths: library.paths, manual: "" } : emptyDraft());
    setTmdbCollections(library?.tmdb_collections ?? true);
    setError(null);
    setConfirmingOff(false);
  });

  const busy = create.isPending || update.isPending;

  const submit = (confirmed = false) => {
    setError(null);
    const finalPaths = draftPaths(draft);
    if (finalPaths.length === 0) {
      setError(t("libraries.form.atLeastOneFolder"));
      return;
    }
    const finalName = draft.name.trim() || t(`libraries.kinds.${draft.kind}.defaultName`);
    if (editing && library) {
      // Sent only when it changes: turned on, the library's metadata is
      // fetched again.
      const tmdb = tmdbCollections !== (library.tmdb_collections ?? true) ? { tmdb_collections: tmdbCollections } : {};
      if (tmdb.tmdb_collections === false && !confirmed) {
        setConfirmingOff(true);
        return;
      }
      update.mutate(
        { id: library.id, name: finalName, paths: finalPaths, ...tmdb },
        {
          onSuccess: () => {
            toast.success(t("libraries.form.updated"));
            setConfirmingOff(false);
            onClose();
          },
          onError: (e) => {
            setConfirmingOff(false);
            setError(errorMessage(e));
          },
        },
      );
    } else {
      create.mutate(
        { name: finalName, kind: draft.kind, paths: finalPaths, tmdb_collections: draft.kind !== "movies" || tmdbCollections },
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
        open={open && !picker && !confirmingOff}
        onClose={onClose}
        locked={busy}
        size="lg"
        title={editing ? t("libraries.form.editTitle", { name: library?.name }) : t("libraries.form.addTitle")}
        footer={
          <>
            <Button variant="ghost" onClick={onClose} disabled={busy}>
              {t("common:actions.cancel")}
            </Button>
            <Button variant="primary" onClick={() => submit()} loading={busy}>
              {editing ? t("libraries.form.saveChanges") : t("libraries.form.addTitle")}
            </Button>
          </>
        }
      >
        <div className="space-y-5">
          <LibraryFields draft={draft} onChange={setDraft} onBrowse={() => setPicker(true)} chooseKind={!editing} />
          {draft.kind === "movies" ? (
            <div className="flex items-start justify-between gap-4">
              <div>
                <label htmlFor="lib-tmdb-collections" className="text-sm font-medium">
                  {t("libraries.form.tmdbCollections")}
                </label>
                <p className="mt-1 text-xs text-subtle">{t("libraries.form.tmdbCollectionsHint")}</p>
              </div>
              <Toggle
                id="lib-tmdb-collections"
                checked={tmdbCollections}
                onChange={setTmdbCollections}
                label={t("libraries.form.tmdbCollections")}
              />
            </div>
          ) : null}
          {error ? <p className="rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">{error}</p> : null}
        </div>
      </Modal>
      <ConfirmModal
        open={open && confirmingOff}
        onClose={() => setConfirmingOff(false)}
        title={t("libraries.form.tmdbCollectionsOffTitle")}
        danger
        confirmLabel={t("common:actions.remove")}
        loading={update.isPending}
        message={
          <Trans
            t={t}
            i18nKey="libraries.form.tmdbCollectionsOffMessage"
            values={{ name: library?.name ?? "" }}
            components={{ name: <strong className="text-fg" /> }}
          />
        }
        onConfirm={() => submit(true)}
      />
      <FolderPicker
        open={open && picker}
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

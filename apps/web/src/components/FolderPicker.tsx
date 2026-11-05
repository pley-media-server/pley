import { useState } from "react";
import { ArrowUp, ChevronRight, Folder, HardDrive } from "lucide-react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import { useFs } from "@/api/hooks";
import { Modal } from "./Modal";
import { Button, IconButton, Spinner, TextInput } from "./ui";
import { useOnChange } from "@/lib/hooks";

export function FolderPicker({
  open,
  onClose,
  onSelect,
  initialPath,
}: {
  open: boolean;
  onClose: () => void;
  onSelect: (path: string) => void;
  initialPath?: string | null;
}) {
  const { t } = useTranslation(["components", "common"]);
  const [path, setPath] = useState<string | null>(initialPath ?? null);
  const [input, setInput] = useState(initialPath ?? "");
  const fs = useFs(path, open);

  // Opening starts from `initialPath` again.
  useOnChange(open, (isOpen) => {
    if (!isOpen) return;
    setPath(initialPath ?? null);
    setInput(initialPath ?? "");
  });
  // The box shows the folder being browsed.
  useOnChange(fs.data, (data) => {
    if (data) setInput(data.path ?? "");
  });

  const atRoots = path == null;
  const goUp = () => setPath(fs.data ? fs.data.parent : null);

  return (
    <Modal
      open={open}
      onClose={onClose}
      title={t("folderPicker.title")}
      size="lg"
      footer={
        <>
          <Button variant="ghost" onClick={onClose}>
            {t("common:actions.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={!fs.data?.path}
            onClick={() => {
              if (fs.data?.path) onSelect(fs.data.path);
            }}
          >
            {t("folderPicker.select")}
          </Button>
        </>
      }
    >
      <form
        className="mb-3 flex gap-2"
        onSubmit={(e) => {
          e.preventDefault();
          const p = input.trim();
          setPath(p ? p : null);
        }}
      >
        <IconButton label={t("folderPicker.up")} size="md" onClick={goUp} disabled={atRoots} className="rounded-md bg-surface-2">
          <ArrowUp className="size-4" />
        </IconButton>
        <TextInput
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder={t("folderPicker.placeholder")}
          className="font-mono text-xs"
          aria-label={t("folderPicker.pathLabel")}
        />
        <Button type="submit">{t("folderPicker.go")}</Button>
      </form>

      <div className="h-80 overflow-y-auto rounded-md border border-border bg-bg/40">
        {fs.isPending ? (
          <div className="flex h-full items-center justify-center">
            <Spinner />
          </div>
        ) : fs.isError ? (
          <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center text-sm">
            <p className="text-danger">{errorMessage(fs.error)}</p>
            <Button size="sm" onClick={() => setPath(null)}>
              {t("folderPicker.backToTop")}
            </Button>
          </div>
        ) : fs.data.entries.length === 0 ? (
          <div className="flex h-full items-center justify-center p-6 text-sm text-subtle">{t("folderPicker.empty")}</div>
        ) : (
          <ul className="divide-y divide-white/[0.04]">
            {fs.data.entries.map((e) => (
              <li key={e.path}>
                <button
                  type="button"
                  onClick={() => setPath(e.path)}
                  onDoubleClick={() => onSelect(e.path)}
                  className="flex w-full items-center gap-3 px-3 py-2 text-left text-sm transition-colors hover:bg-white/5"
                >
                  {atRoots ? <HardDrive className="size-4 shrink-0 text-subtle" /> : <Folder className="size-4 shrink-0 text-accent/80" />}
                  <span className="min-w-0 flex-1 truncate">{e.name}</span>
                  <ChevronRight className="size-4 shrink-0 text-subtle" />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      <p className="mt-2 text-xs text-subtle">{t("folderPicker.hint")}</p>
    </Modal>
  );
}

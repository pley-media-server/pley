import { useTranslation } from "react-i18next";
import type { ImportBring } from "@/api/plexImport";
import { Button, Toggle } from "@/components/ui";
import { ErrorNote, StepFrame } from "./PlexImport";

/** Everything, as the step starts. */
export const BRING_ALL: ImportBring = { watch_history: true, collections: true, edits: true };

const BOXES = [
  { key: "watch_history", label: "plexImport.bring.watchHistory", hint: "plexImport.bring.watchHistoryHint" },
  { key: "collections", label: "plexImport.bring.collections", hint: "plexImport.bring.collectionsHint" },
  { key: "edits", label: "plexImport.bring.edits", hint: "plexImport.bring.editsHint" },
] as const;

/** What comes over besides the libraries and people, then the import's start. */
export function BringStep({
  bring,
  onChange,
  onBack,
  onStart,
  starting,
  error,
}: {
  bring: ImportBring;
  onChange: (bring: ImportBring) => void;
  onBack: () => void;
  onStart: () => void;
  starting: boolean;
  error: string | null;
}) {
  const { t } = useTranslation(["settings", "common"]);
  return (
    <StepFrame
      stage="bring"
      title={t("plexImport.bring.title")}
      description={t("plexImport.bring.description")}
      actions={
        <>
          <Button variant="ghost" onClick={onBack} disabled={starting}>
            {t("common:actions.back")}
          </Button>
          <Button variant="primary" onClick={onStart} loading={starting}>
            {t("plexImport.bring.start")}
          </Button>
        </>
      }
    >
      <ul className="divide-y divide-border rounded-lg border border-border bg-surface-2">
        {BOXES.map((box) => (
          <li key={box.key} className="flex items-start gap-3 p-3.5">
            <div className="min-w-0 flex-1">
              <p className="font-medium">{t(box.label)}</p>
              <p className="text-xs text-muted">{t(box.hint)}</p>
            </div>
            <Toggle checked={bring[box.key]} onChange={(v) => onChange({ ...bring, [box.key]: v })} label={t(box.label)} />
          </li>
        ))}
      </ul>
      {error ? <ErrorNote>{error}</ErrorNote> : null}
    </StepFrame>
  );
}

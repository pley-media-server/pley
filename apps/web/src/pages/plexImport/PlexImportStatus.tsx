import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, Import, X } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useForgetPlex, usePlexImport, type ImportLibraryRow, type ImportProgress, type ImportStep } from "@/api/plexImport";
import { ConfirmModal } from "@/components/Modal";
import { Button, IconButton, ProgressBar } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { cn } from "@/lib/cn";
import { formatDay } from "@/lib/format";
import { useLocalStorage } from "@/lib/hooks";
import { StepList } from "./ImportProgressView";
import { ErrorNote } from "./PlexImport";

/** The `started_at` of the import whose steps were hidden: the next one shows them again. */
const HIDDEN_KEY = "pley.plexImport.stepsHidden";

/**
 * About how much of an import's time each step takes, for the bar of the
 * whole import: scanning the libraries and fetching their details are most
 * of it, the steps that only write what Plex said take seconds. Rough, and
 * only ever compared with each other.
 */
const STEP_WEIGHTS: Record<ImportStep, number> = {
  profiles: 1,
  scanning: 40,
  matching: 0,
  labels: 2,
  edits: 2,
  collections: 1,
  watch_state: 3,
  history: 3,
  playlists: 1,
  fetching: 35,
  matching_again: 12,
};

/**
 * The Plex import, on Home and under Settings › Libraries' buttons. While it
 * runs: which of its steps it's on, how far the whole import has come and,
 * open, its steps with Stop; hidden, the step's name and count. Once it
 * ended: when and how, with its report. `onReport` opens the report;
 * `onClose`, where given, puts an ended import away.
 */
export function PlexImportStatus({ onReport, onClose }: { onReport: () => void; onClose?: () => void }) {
  const { t } = useTranslation(["settings", "common"]);
  const data = usePlexImport().data;
  const [hidden, setHidden] = useLocalStorage<number | null>(HIDDEN_KEY, null);
  const forget = useForgetPlex();
  const [stopping, setStopping] = useState(false);
  if (!data) return null;

  const number = (n: number) => n.toLocaleString(currentLocale());
  const running = data.state === "running";
  const steps = running ? (data.steps ?? []) : [];
  // Open unless its steps were hidden for this import.
  const id = data.started_at ?? 0;
  const open = steps.length > 0 && hidden !== id;
  let text: string;
  let count: string | null = null;
  let overall: number | null = null;
  if (running) {
    const step = data.step ? t(`plexImport.progress.steps.${data.step}`) : t("plexImport.progress.starting");
    const at = steps.findIndex((row) => row.state === "running");
    const place = { at: number(at + 1), steps: number(steps.length) };
    // Open, the steps below name the step and count it.
    text = at < 0 ? t("plexImport.status.running", { step }) : open ? t("plexImport.status.runningAt", place) : t("plexImport.status.runningStep", { step, ...place });
    if (!open && data.total > 0) count = t("plexImport.status.count", { done: number(data.done), total: number(data.total) });
    if (at >= 0) overall = share(data);
  } else if (data.report) {
    const date = formatDay(data.report.finished_at);
    const outcome = data.report.outcome;
    text = outcome === "done" ? t("plexImport.status.done", { date }) : outcome === "failed" ? t("plexImport.status.failed", { date }) : t("plexImport.status.cancelled", { date });
  } else {
    return null;
  }
  const failure = data.report?.outcome === "failed" ? data.report.error : null;

  return (
    <div className="space-y-2 rounded-lg border border-border bg-surface-2 px-3 py-2 text-sm">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1" aria-live="polite">
        <Import className="size-4 shrink-0 text-muted" aria-hidden />
        <span>{text}</span>
        {count ? <span className="tabular-nums text-muted">{count}</span> : null}
        <div className="ms-auto flex items-center gap-1">
          {open ? (
            <Button variant="ghost" size="sm" onClick={() => setStopping(true)}>
              {t("plexImport.progress.stop")}
            </Button>
          ) : null}
          {steps.length > 0 ? (
            <Button variant="ghost" size="sm" aria-expanded={open} onClick={() => setHidden(open ? id : null)}>
              {open ? t("plexImport.status.hideSteps") : t("plexImport.status.showSteps")}
              <ChevronDown className={cn("size-4 transition-transform", open && "rotate-180")} aria-hidden />
            </Button>
          ) : null}
          {!running ? (
            <Button variant="ghost" size="sm" onClick={onReport}>
              {t("plexImport.status.report")}
            </Button>
          ) : null}
          {!running && onClose ? (
            <IconButton size="sm" label={t("common:actions.close")} onClick={onClose}>
              <X className="size-4" aria-hidden />
            </IconButton>
          ) : null}
        </div>
      </div>
      {overall != null ? (
        <div className="flex items-center gap-3">
          <ProgressBar value={overall} label={t("plexImport.status.overall")} className="h-1" />
          <span className="shrink-0 tabular-nums text-muted">
            {new Intl.NumberFormat(currentLocale(), { style: "percent", maximumFractionDigits: 0 }).format(Math.floor(overall * 100) / 100)}
          </span>
        </div>
      ) : null}
      {failure ? <p className="text-danger">{failure}</p> : null}
      {forget.error ? <ErrorNote>{errorMessage(forget.error)}</ErrorNote> : null}
      {open ? (
        <div className="border-t border-border pt-3">
          <StepList progress={data} compact />
        </div>
      ) : null}
      <ConfirmModal
        open={stopping}
        onClose={() => setStopping(false)}
        title={t("plexImport.progress.stopTitle")}
        message={t("plexImport.progress.stopMessage")}
        confirmLabel={t("plexImport.progress.stop")}
        danger
        loading={forget.isPending}
        onConfirm={() => forget.mutate(undefined, { onSettled: () => setStopping(false) })}
      />
    </div>
  );
}

/**
 * How far the whole import has come: its steps done and the part of the
 * running one that is, each weighed by how long it takes.
 */
function share(progress: ImportProgress): number {
  const steps = progress.steps ?? [];
  const total = steps.reduce((sum, row) => sum + STEP_WEIGHTS[row.step], 0);
  if (total === 0) return 0;
  const done = steps.reduce((sum, row) => {
    const weight = STEP_WEIGHTS[row.step];
    if (row.state === "done") return sum + weight;
    if (row.state === "running") return sum + weight * runningPart(progress, row.step);
    return sum;
  }, 0);
  return done / total;
}

/** The part of the running step done: for the scans, the libraries' files read; otherwise its count. */
function runningPart(progress: ImportProgress, step: ImportStep): number {
  const libraries = progress.libraries ?? [];
  if (step === "scanning" && libraries.length > 0) {
    return libraries.reduce((sum, library) => sum + libraryPart(library), 0) / libraries.length;
  }
  return progress.total > 0 ? Math.min(progress.done / progress.total, 1) : 0;
}

/** How far a library is in the scanning step: its files read, then Plex's titles found in it. */
function libraryPart(library: ImportLibraryRow): number {
  switch (library.phase) {
    case "done":
    case "matching":
      return 1;
    case "probing":
      return library.total > 0 ? Math.min(library.done / library.total, 1) : 0;
    default:
      return 0;
  }
}

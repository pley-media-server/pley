import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, Import, X } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useForgetPlex, usePlexImport, type ImportProgress } from "@/api/plexImport";
import { ConfirmModal } from "@/components/Modal";
import { Button, IconButton, ProgressBar } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { cn } from "@/lib/cn";
import { formatDay, formatTimeLeft } from "@/lib/format";
import { useLocalStorage } from "@/lib/hooks";
import { StepList } from "./ImportProgressView";
import { formatShare, importShare } from "./share";
import { ErrorNote } from "./PlexImport";

/** The `started_at` of the import whose steps were hidden: the next one shows them again. */
const HIDDEN_KEY = "pley.plexImport.stepsHidden";

/**
 * The Plex import, on Home and under Settings › Libraries' buttons. While it
 * runs: which of its steps it's on, how far the whole import has come and,
 * open, its steps with Stop; hidden, the step's name and count. Once it
 * ended: when and how, with its report, and the way to import again when it
 * didn't finish. `onReport` opens the report, `onAgain` the import's first
 * step; `onClose`, where given, puts an ended import away.
 */
export function PlexImportStatus({ onReport, onAgain, onClose }: { onReport: () => void; onAgain: () => void; onClose?: () => void }) {
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
  let left: string | null = null;
  let overall: number | null = null;
  if (running) {
    const step = data.step ? t(`plexImport.progress.steps.${data.step}`) : t("plexImport.progress.starting");
    const at = steps.findIndex((row) => row.state === "running");
    const place = { at: number(at + 1), steps: number(steps.length) };
    // Open, the steps below name the step and count it.
    text = at < 0 ? t("plexImport.status.running", { step }) : open ? t("plexImport.status.runningAt", place) : t("plexImport.status.runningStep", { step, ...place });
    if (!open && data.total > 0) count = t("plexImport.status.count", { done: number(data.done), total: number(data.total) });
    const seconds = stepLeft(data);
    if (!open && seconds != null) left = formatTimeLeft(seconds);
    if (at >= 0) overall = importShare(data);
  } else if (data.state === "interrupted" && data.started_at) {
    text = t("plexImport.status.interrupted", { date: formatDay(data.started_at) });
  } else if (data.report) {
    const date = formatDay(data.report.finished_at);
    const outcome = data.report.outcome;
    text = outcome === "done" ? t("plexImport.status.done", { date }) : outcome === "failed" ? t("plexImport.status.failed", { date }) : t("plexImport.status.cancelled", { date });
  } else {
    return null;
  }
  const failure = data.report?.outcome === "failed" ? data.report.error : null;
  // What came over stays: importing again brings the rest, and nothing twice.
  const unfinished = !running && (data.state === "interrupted" || (!!data.report && data.report.outcome !== "done"));

  return (
    <div className="space-y-2 rounded-lg border border-border bg-surface-2 px-3 py-2 text-sm">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        {/* The icon stays with the words, which wrap on a phone. */}
        <div className="flex min-w-0 flex-[1_1_15rem] items-start gap-3">
          <Import className="mt-0.5 size-4 shrink-0 text-muted" aria-hidden />
          <p>
            {/* Said when the step changes, not at every count. */}
            <span aria-live="polite">{text}</span>
            {/* A count is kept in one piece; the space before it can break. */}
            {count ? (
              <>
                {" "}
                <span className="whitespace-nowrap tabular-nums text-muted">· {count}</span>
              </>
            ) : null}
            {left ? <span className="text-muted"> · {left}</span> : null}
          </p>
        </div>
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
          {unfinished ? (
            <Button variant="ghost" size="sm" onClick={onAgain}>
              {t("plexImport.report.again")}
            </Button>
          ) : null}
          {!running && data.report ? (
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
            {formatShare(overall)}
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

/** About how many seconds the running step has left: in the scans, the library whose files are read. */
function stepLeft(progress: ImportProgress): number | null {
  if (progress.step === "scanning") return progress.libraries?.find((library) => library.phase === "probing")?.left_secs ?? null;
  return progress.left_secs ?? null;
}

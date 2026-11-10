import { useState } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown, Import, X } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useForgetPlex, usePlexImport, type ImportProgress } from "@/api/plexImport";
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
 * The Plex import, on Home and under Settings › Libraries' buttons. While it
 * runs: its step, which of its steps that is, how far the whole import has
 * come and, open, every step with Stop. Once it ended: when and how, with
 * its report. `onReport` opens the report; `onClose`, where given, puts an
 * ended import away.
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
  let text: string;
  let count: string | null = null;
  let overall: number | null = null;
  if (running) {
    const step = data.step ? t(`plexImport.progress.steps.${data.step}`) : t("plexImport.progress.starting");
    const at = steps.findIndex((row) => row.state === "running");
    text =
      at >= 0
        ? t("plexImport.status.runningStep", { step, at: number(at + 1), steps: number(steps.length) })
        : t("plexImport.status.running", { step });
    if (data.total > 0) count = t("plexImport.status.count", { done: number(data.done), total: number(data.total) });
    if (at >= 0) overall = share(data, at);
  } else if (data.report) {
    const date = formatDay(data.report.finished_at);
    const outcome = data.report.outcome;
    text = outcome === "done" ? t("plexImport.status.done", { date }) : outcome === "failed" ? t("plexImport.status.failed", { date }) : t("plexImport.status.cancelled", { date });
  } else {
    return null;
  }

  // Open unless its steps were hidden for this import.
  const id = data.started_at ?? 0;
  const open = steps.length > 0 && hidden !== id;
  const failure = data.report?.outcome === "failed" ? data.report.error : null;

  return (
    <div className="space-y-2 rounded-lg border border-border bg-surface-2 px-3 py-2 text-sm">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1" aria-live="polite">
        <Import className="size-4 shrink-0 text-muted" aria-hidden />
        <span>{text}</span>
        {count ? <span className="tabular-nums text-muted">{count}</span> : null}
        <div className="ms-auto flex items-center gap-1">
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
      {overall != null ? <ProgressBar value={overall} label={t("plexImport.status.overall")} className="h-1" /> : null}
      {failure ? <p className="text-danger">{failure}</p> : null}
      {open ? (
        <div className="space-y-3 border-t border-border pt-3">
          <StepList progress={data} />
          <div className="flex justify-end">
            <Button variant="ghost" size="sm" onClick={() => setStopping(true)}>
              {t("plexImport.progress.stop")}
            </Button>
          </div>
          {forget.error ? <ErrorNote>{errorMessage(forget.error)}</ErrorNote> : null}
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
 * How far the whole import has come: its steps done, and the part of the
 * running one (the step at `at`) that is.
 */
function share(progress: ImportProgress, at: number): number {
  const steps = progress.steps ?? [];
  const part = progress.total > 0 ? Math.min(progress.done / progress.total, 1) : 0;
  return (at + part) / steps.length;
}

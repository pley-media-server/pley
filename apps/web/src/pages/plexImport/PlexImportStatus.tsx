import { useTranslation } from "react-i18next";
import { Import } from "lucide-react";
import { usePlexImport, type ImportProgress } from "@/api/plexImport";
import { Button, ProgressBar } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { formatDay } from "@/lib/format";

/**
 * The Plex import, under Settings › Libraries' buttons and on Home: its
 * step while it runs, with how far the whole import has come, then when it
 * ended, with the way to its progress or its report. `onOpen(true)` opens
 * the report.
 */
export function PlexImportStatus({ onOpen }: { onOpen: (report: boolean) => void }) {
  const { t } = useTranslation("settings");
  const data = usePlexImport().data;
  if (!data) return null;

  const number = (n: number) => n.toLocaleString(currentLocale());
  let text: string;
  let count: string | null = null;
  let overall: number | null = null;
  if (data.state === "running") {
    const step = data.step ? t(`plexImport.progress.steps.${data.step}`) : t("plexImport.progress.starting");
    const steps = data.steps ?? [];
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

  const running = data.state === "running";
  return (
    <div className="space-y-2 rounded-lg border border-border bg-surface-2 px-3 py-2 text-sm" aria-live="polite">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1">
        <Import className="size-4 shrink-0 text-muted" aria-hidden />
        <span>{text}</span>
        {count ? <span className="tabular-nums text-muted">{count}</span> : null}
        <Button variant="ghost" size="sm" className="ms-auto" onClick={() => onOpen(!running)}>
          {running ? t("plexImport.status.show") : t("plexImport.status.report")}
        </Button>
      </div>
      {overall != null ? <ProgressBar value={overall} label={t("plexImport.status.overall")} className="h-1" /> : null}
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

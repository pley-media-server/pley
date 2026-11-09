import { useTranslation } from "react-i18next";
import { Import } from "lucide-react";
import { usePlexImport } from "@/api/plexImport";
import { Button } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { formatDay } from "@/lib/format";

/**
 * The Plex import, under Settings › Libraries' buttons: its step while it
 * runs, then when it ended, with the way to its progress or its report.
 * `onOpen(true)` opens the report.
 */
export function PlexImportStatus({ onOpen }: { onOpen: (report: boolean) => void }) {
  const { t } = useTranslation("settings");
  const data = usePlexImport().data;
  if (!data) return null;

  let text: string;
  let count: string | null = null;
  if (data.state === "running") {
    const step = data.step ? t(`plexImport.progress.steps.${data.step}`) : t("plexImport.progress.starting");
    text = t("plexImport.status.running", { step });
    if (data.total > 0) {
      const number = (n: number) => n.toLocaleString(currentLocale());
      count = t("plexImport.status.count", { done: number(data.done), total: number(data.total) });
    }
  } else if (data.report) {
    const date = formatDay(data.report.finished_at);
    const outcome = data.report.outcome;
    text = outcome === "done" ? t("plexImport.status.done", { date }) : outcome === "failed" ? t("plexImport.status.failed", { date }) : t("plexImport.status.cancelled", { date });
  } else {
    return null;
  }

  const running = data.state === "running";
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-lg border border-border bg-surface-2 px-3 py-2 text-sm" aria-live="polite">
      <Import className="size-4 shrink-0 text-muted" aria-hidden />
      <span>{text}</span>
      {count ? <span className="tabular-nums text-muted">{count}</span> : null}
      <Button variant="ghost" size="sm" className="ms-auto" onClick={() => onOpen(!running)}>
        {running ? t("plexImport.status.show") : t("plexImport.status.report")}
      </Button>
    </div>
  );
}

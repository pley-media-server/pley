import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import i18next from "i18next";
import { FolderSync } from "lucide-react";
import { useActivity } from "@/api/hooks";
import { usePlexImport, type ImportProgress } from "@/api/plexImport";
import type { ScanStatus } from "@/api/types";
import { useAuth } from "@/auth";
import { ProgressBar } from "@/components/ui";
import { currentLanguage, currentLocale, loadNamespaces } from "@/i18n";
import { cn } from "@/lib/cn";
import { useLocalStorage, useNow } from "@/lib/hooks";
import { PlexImportModal } from "./plexImport/PlexImportModal";
import { PlexImportStatus } from "./plexImport/PlexImportStatus";

/** The `finished_at` of the import whose ending was put away on Home. */
const CLOSED_KEY = "pley.plexImport.closed";

/** How long an import's ending stays on Home, unless put away: a week. */
const ENDING_SHOWN_FOR = 7 * 24 * 60 * 60;

/**
 * What Home says while new libraries fill in, to administrators: the Plex
 * import running, then how it ended until that's put away, else a
 * library's first scan. Nothing otherwise.
 */
export function HomeProgress({ className }: { className?: string }) {
  const user = useAuth();
  return user.is_admin ? <AdminProgress className={className} /> : null;
}

function AdminProgress({ className }: { className?: string }) {
  const progress = usePlexImport().data;
  const activity = useActivity();
  const [closed, setClosed] = useLocalStorage<number | null>(CLOSED_KEY, null);
  const now = useNow(60_000);

  if (progress && (progress.state === "running" || ending(progress, closed, now))) {
    const finished = progress.report?.finished_at ?? null;
    return <Importing progress={progress} onClose={() => setClosed(finished)} className={className} />;
  }

  // One library is scanned at a time: the one under way, else the next.
  const scans = (activity.data?.scans ?? []).filter((s) => s.first && s.phase !== "done");
  const scan = scans.find((s) => s.phase !== "queued") ?? scans[0];
  return scan ? <FirstScan scan={scan} className={className} /> : null;
}

/** Whether an import that ended shows on Home: it ended lately, and wasn't put away. */
function ending(progress: ImportProgress, closed: number | null, now: number): boolean {
  const report = progress.report;
  return !!report && report.finished_at !== closed && now - report.finished_at < ENDING_SHOWN_FOR;
}

/**
 * The Plex import, as Settings › Libraries shows it, with the way to put
 * its ending away. Its words are the settings', which Home doesn't come
 * with: they load first.
 */
function Importing({ progress, onClose, className }: { progress: ImportProgress; onClose: () => void; className?: string }) {
  const { t } = useTranslation("settings");
  const words = useSettingsWords();
  const [report, setReport] = useState(false);
  if (!words) return null;
  return (
    <div className={cn("space-y-1.5", className)}>
      <PlexImportStatus onReport={() => setReport(true)} onClose={onClose} />
      {progress.state === "running" && progress.first_pass_done ? (
        <p className="px-1 text-sm text-muted">{t("plexImport.status.fillingIn")}</p>
      ) : null}
      <PlexImportModal open={report} report onClose={() => setReport(false)} />
    </div>
  );
}

/** Whether the settings' words are here, loading them if they aren't. */
function useSettingsWords(): boolean {
  const [ready, setReady] = useState(() => i18next.hasResourceBundle(currentLanguage(), "settings"));
  useEffect(() => {
    if (ready) return;
    let live = true;
    void loadNamespaces(["settings"]).then(() => {
      if (live) setReady(true);
    });
    return () => {
      live = false;
    };
  }, [ready]);
  return ready;
}

/** A library's first scan: its files found, read, then its details fetched. */
function FirstScan({ scan, className }: { scan: ScanStatus; className?: string }) {
  const { t } = useTranslation("pages");
  const phase = scan.phase === "done" ? "metadata" : scan.phase;
  const text = t(`home.firstScan.${phase}`, { library: scan.library_name });
  const counted = (phase === "probing" || phase === "metadata") && scan.total > 0;
  const number = (n: number) => n.toLocaleString(currentLocale());
  return (
    <div className={cn("space-y-2 rounded-lg border border-border bg-surface-2 px-3 py-2 text-sm", className)} aria-live="polite">
      <div className="flex items-center gap-3">
        <FolderSync className="size-4 shrink-0 text-muted" aria-hidden />
        <span>{text}</span>
        {counted ? (
          <span className="ms-auto tabular-nums text-muted">
            {number(scan.processed)} / {number(scan.total)}
          </span>
        ) : null}
      </div>
      {counted ? <ProgressBar value={scan.processed / scan.total} label={text} className="h-1" /> : null}
    </div>
  );
}

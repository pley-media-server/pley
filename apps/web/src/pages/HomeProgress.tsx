import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import i18next from "i18next";
import { ChevronDown, Circle, CircleCheck, FolderSync, LoaderCircle } from "lucide-react";
import { useActivity, useLibraries } from "@/api/hooks";
import { usePlexImport, type ImportProgress } from "@/api/plexImport";
import type { Activity, Library, ScanStatus } from "@/api/types";
import { useAuth } from "@/auth";
import { Button, ProgressBar } from "@/components/ui";
import { currentLanguage, currentLocale, loadNamespaces } from "@/i18n";
import { cn } from "@/lib/cn";
import { useLocalStorage, useNow } from "@/lib/hooks";
import { PlexImportModal } from "./plexImport/PlexImportModal";
import { PlexImportStatus } from "./plexImport/PlexImportStatus";

/** When the import whose ending was put away on Home ended (see `endedAt`). */
const CLOSED_KEY = "pley.plexImport.closed";

/** How long an import's ending stays on Home, unless put away: a week. */
const ENDING_SHOWN_FOR = 7 * 24 * 60 * 60;

/** Whether the libraries of a new server's first scans were hidden. */
const LIBRARIES_HIDDEN_KEY = "pley.home.firstScansHidden";

/**
 * What Home says while new libraries fill in, to administrators: the Plex
 * import running, then how it ended until that's put away; else a new
 * server's libraries scanned for the first time, each ticked off; else a
 * library's first scan. Nothing otherwise.
 */
export function HomeProgress({ className }: { className?: string }) {
  const user = useAuth();
  return user.is_admin ? <AdminProgress className={className} /> : null;
}

function AdminProgress({ className }: { className?: string }) {
  const progress = usePlexImport().data;
  const activity = useActivity();
  const libraries = useLibraries().data;
  const [closed, setClosed] = useLocalStorage<number | null>(CLOSED_KEY, null);
  const now = useNow(60_000);

  if (progress && (progress.state === "running" || ending(progress, closed, now))) {
    return <Importing progress={progress} onClose={() => setClosed(endedAt(progress))} className={className} />;
  }

  const batch = activity.data && libraries ? newServerScans(activity.data, libraries) : null;
  if (batch) return <FirstScans scans={batch} className={className} />;

  // One library is scanned at a time: the one under way, else the next.
  const scans = (activity.data?.scans ?? []).filter((s) => s.first && s.phase !== "done");
  const scan = scans.find((s) => s.phase !== "queued") ?? scans[0];
  return scan ? <FirstScan scan={scan} className={className} /> : null;
}

/**
 * The first scans of a new server's libraries, done or not, in the order
 * they're scanned, with each library's name: while every library is new
 * and one of them isn't scanned yet, as after Start fresh. Null otherwise,
 * as when a library is added to a server that has others.
 */
function newServerScans(activity: Activity, libraries: Library[]): ScanStatus[] | null {
  const live = activity.scans.filter((s) => s.first);
  const done = (activity.first_scans_done ?? []).filter((d) => !live.some((s) => s.library_id === d.library_id));
  const scans = [...done, ...live].sort((a, b) => a.started_at - b.started_at);
  const every = libraries.length > 0 && libraries.every((l) => scans.some((s) => s.library_id === l.id));
  if (!every || scans.every((s) => s.phase === "done")) return null;
  return scans.flatMap((s) => {
    const library = libraries.find((l) => l.id === s.library_id);
    return library ? [{ ...s, library_name: library.name }] : [];
  });
}

/** How far a library's first scan has come: its files read is the first half, its details fetched the second. */
function scanShare(scan: ScanStatus): number {
  const part = scan.total > 0 ? Math.min(scan.processed / scan.total, 1) : 0;
  switch (scan.phase) {
    case "done":
      return 1;
    case "probing":
      return part / 2;
    case "metadata":
      return 0.5 + part / 2;
    default:
      return 0;
  }
}

/**
 * A new server's libraries scanned for the first time: which one, of how
 * many, how far they've all come and, open, each library ticked off once
 * done, with how far the one scanned is.
 */
function FirstScans({ scans, className }: { scans: ScanStatus[]; className?: string }) {
  const { t } = useTranslation("pages");
  const [hidden, setHidden] = useLocalStorage<boolean>(LIBRARIES_HIDDEN_KEY, false);
  const number = (n: number) => n.toLocaleString(currentLocale());
  const done = scans.filter((s) => s.phase === "done").length;
  // One library is scanned at a time: the one under way, else the next.
  const left = scans.filter((s) => s.phase !== "done");
  const scan = left.find((s) => s.phase !== "queued") ?? left[0]!;
  const text = t("home.firstScans.title", { at: number(done + 1), count: number(scans.length), library: scan.library_name });
  const counted = (scan.phase === "probing" || scan.phase === "metadata") && scan.total > 0;
  const overall = scans.reduce((sum, s) => sum + scanShare(s), 0) / scans.length;
  const open = !hidden;
  return (
    <div className={cn("space-y-2 rounded-lg border border-border bg-surface-2 px-3 py-2 text-sm", className)}>
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1" aria-live="polite">
        <FolderSync className="size-4 shrink-0 text-muted" aria-hidden />
        <span>{text}</span>
        {counted ? (
          <span className="tabular-nums text-muted">
            {number(scan.processed)} / {number(scan.total)}
          </span>
        ) : null}
        <Button variant="ghost" size="sm" className="ms-auto" aria-expanded={open} onClick={() => setHidden(open)}>
          {open ? t("home.firstScans.hide") : t("home.firstScans.show")}
          <ChevronDown className={cn("size-4 transition-transform", open && "rotate-180")} aria-hidden />
        </Button>
      </div>
      <ProgressBar value={overall} label={t("home.firstScans.overall")} className="h-1" />
      {open ? (
        <ol className="space-y-3 border-t border-border pt-3">
          {scans.map((s) => (
            <li key={s.library_id}>
              <LibraryLine scan={s} running={s === scan} />
            </li>
          ))}
        </ol>
      ) : null}
    </div>
  );
}

/** A library in a new server's first scans: done, being scanned (with how far), or to come. */
function LibraryLine({ scan, running }: { scan: ScanStatus; running: boolean }) {
  const { t } = useTranslation("pages");
  const phase = scan.phase === "walking" || scan.phase === "probing" || scan.phase === "metadata" ? scan.phase : null;
  const counted = running && (phase === "probing" || phase === "metadata") && scan.total > 0;
  const number = (n: number) => n.toLocaleString(currentLocale());
  return (
    <div className="space-y-1.5">
      <div className="flex items-center gap-2">
        {scan.phase === "done" ? (
          <CircleCheck className="size-4 shrink-0 text-success" aria-hidden />
        ) : running ? (
          <LoaderCircle className="size-4 shrink-0 animate-spin text-accent" aria-hidden />
        ) : (
          <Circle className="size-4 shrink-0 text-subtle" aria-hidden />
        )}
        <span className={scan.phase === "done" ? undefined : running ? "font-medium" : "text-subtle"}>{scan.library_name}</span>
        {counted ? (
          <span className="ml-auto tabular-nums text-muted">
            {number(scan.processed)} / {number(scan.total)}
          </span>
        ) : null}
      </div>
      {running && phase ? (
        <div className="space-y-1 pl-6">
          <p className="text-muted">{t(`home.firstScans.phases.${phase}`)}</p>
          <ProgressBar
            value={counted ? scan.processed / scan.total : 0}
            label={scan.library_name}
            className="h-2"
            barClassName={counted ? undefined : "w-1/3 animate-pulse"}
          />
        </div>
      ) : null}
    </div>
  );
}

/** When an import ended: its report's time or, cut short by a restart, when it started. */
function endedAt(progress: ImportProgress): number | null {
  if (progress.state === "interrupted") return progress.started_at || null;
  return progress.report?.finished_at ?? null;
}

/** Whether an import that ended shows on Home: it ended lately, and wasn't put away. */
function ending(progress: ImportProgress, closed: number | null, now: number): boolean {
  const at = endedAt(progress);
  return at != null && at !== closed && now - at < ENDING_SHOWN_FOR;
}

/**
 * The Plex import, as Settings › Libraries shows it, with the way to put
 * its ending away. Its words are the settings', which Home doesn't come
 * with: they load first.
 */
function Importing({ progress, onClose, className }: { progress: ImportProgress; onClose: () => void; className?: string }) {
  const { t } = useTranslation("settings");
  const words = useSettingsWords();
  // The import's dialog, on its report or its first step.
  const [open, setOpen] = useState<{ report: boolean } | null>(null);
  if (!words) return null;
  return (
    <div className={cn("space-y-1.5", className)}>
      <PlexImportStatus onReport={() => setOpen({ report: true })} onAgain={() => setOpen({ report: false })} onClose={onClose} />
      {progress.state === "running" && progress.first_pass_done ? (
        <p className="px-1 text-sm text-muted">{t("plexImport.status.fillingIn")}</p>
      ) : null}
      <PlexImportModal open={open !== null} report={open?.report} onClose={() => setOpen(null)} />
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

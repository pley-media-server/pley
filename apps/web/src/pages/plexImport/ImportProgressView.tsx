import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { Link } from "react-router";
import { Circle, CircleAlert, CircleCheck, CircleX, LoaderCircle, TriangleAlert } from "lucide-react";
import { errorMessage } from "@/api/client";
import {
  useForgetPlex,
  usePlexImport,
  type EditedField,
  type ImportProgress,
  type ImportReport,
  type ImportLibraryRow,
  type ImportScan,
  type ImportStep,
  type ImportStepRow,
  type ProfileReport,
} from "@/api/plexImport";
import { useRatingSystems } from "@/api/hooks";
import { ConfirmModal } from "@/components/Modal";
import { Badge, Button, PageSpinner, ProgressBar } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { ageBadge } from "@/lib/access";
import { formatDuration, formatTimeLeft } from "@/lib/format";
import { libraryUrl } from "@/lib/urls";
import { ErrorNote, StepFrame } from "./PlexImport";

/** The import running, then its report. `onFinish` goes on while it runs: Home and Settings › Libraries show it too. */
export function ImportProgressView({
  goOnLabel,
  onFinish,
  onAgain,
}: {
  goOnLabel: string;
  onFinish: () => void;
  onAgain: () => void;
}) {
  const { t } = useTranslation(["settings", "common"]);
  const progress = usePlexImport();
  const forget = useForgetPlex();
  const [stopping, setStopping] = useState(false);
  const data = progress.data;

  if (!data) return <PageSpinner />;

  // The server restarted while it ran, and kept no report (servers before
  // they said so: idle with none).
  if (data.state === "interrupted" || (data.state === "idle" && !data.report)) {
    return (
      <StepFrame
        title={t("plexImport.report.lost")}
        description={t("plexImport.report.lostHint")}
        actions={
          <Button variant="primary" onClick={onAgain}>
            {t("plexImport.report.again")}
          </Button>
        }
      >
        {null}
      </StepFrame>
    );
  }

  if (data.state === "running") {
    const share = data.total > 0 ? data.done / data.total : 0;
    return (
      <StepFrame
        title={data.first_pass_done ? t("plexImport.progress.inTitle") : t("plexImport.progress.title")}
        description={data.first_pass_done ? t("plexImport.progress.inDescription") : t("plexImport.progress.description")}
        actions={
          <>
            <Button variant="ghost" onClick={() => setStopping(true)}>
              {t("plexImport.progress.stop")}
            </Button>
            <Button variant="primary" onClick={onFinish}>
              {goOnLabel}
            </Button>
          </>
        }
      >
        {/* Servers before the list of steps give the one running alone. */}
        {data.steps?.length ? (
          <StepList progress={data} />
        ) : (
          <div className="space-y-2">
            <div className="flex items-center justify-between text-sm">
              <span className="font-medium" aria-live="polite">{data.step ? t(`plexImport.progress.steps.${data.step}`) : t("plexImport.progress.starting")}</span>
              {data.total > 0 ? (
                <span className="tabular-nums text-muted">
                  {data.done.toLocaleString(currentLocale())} / {data.total.toLocaleString(currentLocale())}
                </span>
              ) : null}
            </div>
            <ProgressBar
              value={share}
              label={t("plexImport.progress.title")}
              className="h-2"
              barClassName={data.total === 0 ? "w-1/3 animate-pulse" : undefined}
            />
            {data.scan ? <ScanLine scan={data.scan} /> : null}
          </div>
        )}
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
        {forget.error ? <ErrorNote>{errorMessage(forget.error)}</ErrorNote> : null}
      </StepFrame>
    );
  }

  const report = data.report;
  if (!report) return null;
  return <Report report={report} onFinish={onFinish} onAgain={onAgain} />;
}

/** The words for what a step brought, where it has a count. */
function stepCount(t: TFunction<"settings">, step: ImportStep, count: number): string | null {
  switch (step) {
    case "profiles":
      return t("plexImport.progress.counts.profiles", { count });
    case "scanning":
      return t("plexImport.report.matched", { count });
    case "labels":
      return t("plexImport.report.labelsAdded", { count });
    case "edits":
      return t("plexImport.progress.counts.edits", { count });
    case "collections":
      return t("plexImport.report.collectionsMade", { count });
    case "watch_state":
      return t("plexImport.report.watched", { count });
    case "history":
      return t("plexImport.report.plays", { count });
    case "playlists":
      return t("plexImport.report.playlists", { count });
    default:
      return null;
  }
}

/**
 * Every step the import goes through: those done with what they brought,
 * the one running with how far it is, the rest to come. `compact` lists
 * those to come after the next behind a button.
 */
export function StepList({ progress, compact = false }: { progress: ImportProgress; compact?: boolean }) {
  const { t } = useTranslation("settings");
  const [all, setAll] = useState(false);
  const steps = progress.steps ?? [];
  const running = steps.findIndex((row) => row.state === "running");
  const shown = compact && !all && running >= 0 ? running + 2 : steps.length;
  const more = steps.length - shown;
  const step = steps[running]?.step;
  return (
    <div className="space-y-2">
      {/* Said when the step changes, not at every count: the card's line says it where the list is compact. */}
      {compact ? null : (
        <p className="sr-only" aria-live="polite">
          {step ? t(`plexImport.progress.steps.${step}`) : null}
        </p>
      )}
      <ol className="space-y-3">
        {steps.slice(0, shown).map((row) => (
          <li key={row.step}>
            <StepLine row={row} progress={progress} />
          </li>
        ))}
      </ol>
      {more > 0 ? (
        <Button variant="ghost" size="sm" className="-ml-3" onClick={() => setAll(true)}>
          {t("plexImport.progress.moreSteps", { count: more })}
        </Button>
      ) : null}
    </div>
  );
}

function StepLine({ row, progress }: { row: ImportStepRow; progress: ImportProgress }) {
  const { t } = useTranslation("settings");
  const name = t(`plexImport.progress.steps.${row.step}`);
  const counted = row.state === "done" && row.count != null ? stepCount(t, row.step, row.count) : null;
  // Finding Plex's titles is part of the scans, and the second round's
  // steps are part of finding the rest: they show under their row.
  const within = progress.step && progress.step !== row.step ? t(`plexImport.progress.steps.${progress.step}`) : null;
  // The scans list each library the import brings, with how far it is,
  // rather than a bar of libraries done.
  const libraries = row.step === "scanning" && row.state === "running" && progress.libraries?.length ? progress.libraries : null;
  return (
    <div className="space-y-1.5">
      <div className="flex items-center gap-2 text-sm">
        {row.state === "done" ? (
          <CircleCheck className="size-4 shrink-0 text-success" aria-hidden />
        ) : row.state === "running" ? (
          <LoaderCircle className="size-4 shrink-0 animate-spin text-accent" aria-hidden />
        ) : (
          <Circle className="size-4 shrink-0 text-subtle" aria-hidden />
        )}
        <span className={row.state === "to_come" ? "text-subtle" : row.state === "running" ? "font-medium" : undefined}>{name}</span>
        {counted ? <span className="text-muted">· {counted}</span> : null}
        {row.state === "running" && !libraries && progress.left_secs != null ? (
          <span className="text-muted">· {formatTimeLeft(progress.left_secs)}</span>
        ) : null}
        {row.state === "running" && !libraries && progress.total > 0 ? (
          <span className="ml-auto tabular-nums text-muted">
            {progress.done.toLocaleString(currentLocale())} / {progress.total.toLocaleString(currentLocale())}
          </span>
        ) : null}
      </div>
      {libraries ? (
        <ol className="space-y-2 pl-6">
          {libraries.map((library, i) => (
            <li key={i}>
              <LibraryLine library={library} />
            </li>
          ))}
        </ol>
      ) : row.state === "running" ? (
        <div className="space-y-1 pl-6">
          {within ? <p className="text-sm text-muted">{within}</p> : null}
          <ProgressBar
            value={progress.total > 0 ? progress.done / progress.total : 0}
            label={name}
            className="h-2"
            barClassName={progress.total === 0 ? "w-1/3 animate-pulse" : undefined}
          />
          {progress.scan && row.step === "scanning" ? <ScanLine scan={progress.scan} /> : null}
        </div>
      ) : null}
    </div>
  );
}

/** A library the import brings: its files found then read, with about how long that has left, then Plex's titles found in it. */
function LibraryLine({ library }: { library: ImportLibraryRow }) {
  const { t } = useTranslation("settings");
  const number = (n: number) => n.toLocaleString(currentLocale());
  const { phase } = library;
  const reading = phase === "probing" && library.total > 0;
  const doing = phase === "walking" || phase === "probing" || phase === "matching" ? t(`plexImport.progress.library.${phase}`) : null;
  const left = reading && library.left_secs != null ? formatTimeLeft(library.left_secs) : null;
  return (
    <div className="space-y-1 text-sm">
      <div className="flex items-center gap-2">
        {phase === "done" ? (
          <CircleCheck className="size-3.5 shrink-0 text-success" aria-hidden />
        ) : phase === "queued" ? (
          <Circle className="size-3.5 shrink-0 text-subtle" aria-hidden />
        ) : (
          <LoaderCircle className="size-3.5 shrink-0 animate-spin text-accent" aria-hidden />
        )}
        <span className={phase === "queued" ? "text-subtle" : undefined}>{library.library}</span>
        {phase === "done" && library.found != null ? (
          <span className="text-muted">· {t("plexImport.report.matched", { count: library.found })}</span>
        ) : null}
        {reading ? (
          <span className="ml-auto shrink-0 whitespace-nowrap tabular-nums text-muted">
            {number(library.done)} / {number(library.total)}
          </span>
        ) : null}
      </div>
      {/* On a line of its own, so the name and count keep theirs on a phone. */}
      {doing ? (
        <div className="space-y-1 pl-5.5">
          <p className="text-muted">{left ? `${doing} · ${left}` : doing}</p>
          {reading ? <ProgressBar value={library.done / library.total} label={library.library} className="h-1.5" /> : null}
        </div>
      ) : null}
    </div>
  );
}

/** The library the import's scans are on, with its files read once they're counted. */
function ScanLine({ scan }: { scan: ImportScan }) {
  const { t } = useTranslation("settings");
  const text = t(`plexImport.progress.scan.${scan.phase}`, { library: scan.library });
  const counted = scan.phase === "probing" && scan.total > 0;
  return (
    <div className="space-y-1 pt-1">
      <div className="flex items-center justify-between text-sm text-muted">
        <span>{text}</span>
        {counted ? (
          <span className="tabular-nums">
            {scan.done.toLocaleString(currentLocale())} / {scan.total.toLocaleString(currentLocale())}
          </span>
        ) : null}
      </div>
      {counted ? <ProgressBar value={scan.done / scan.total} label={text} className="h-1" /> : null}
    </div>
  );
}

function Report({ report, onFinish, onAgain }: { report: ImportReport; onFinish: () => void; onAgain: () => void }) {
  const { t } = useTranslation(["settings", "common"]);
  const title =
    report.outcome === "done" ? t("plexImport.report.done") : report.outcome === "failed" ? t("plexImport.report.failed") : t("plexImport.report.cancelled");
  const Icon = report.outcome === "done" ? CircleCheck : report.outcome === "failed" ? CircleX : CircleAlert;
  // Reports kept from before Pley brought edits have none.
  const edits = (Object.entries(report.edits ?? {}) as [EditedField, number][]).filter(([, n]) => n > 0).sort((a, b) => b[1] - a[1]);

  return (
    <StepFrame
      title={title}
      description={report.outcome === "done" ? t("plexImport.report.doneHint") : t("plexImport.report.partHint")}
      actions={
        <>
          {report.outcome !== "done" ? (
            <Button variant="ghost" onClick={onAgain}>
              {t("plexImport.report.again")}
            </Button>
          ) : null}
          <Button variant="primary" onClick={onFinish}>
            {t("common:actions.done")}
          </Button>
        </>
      }
    >
      <div className="flex items-center gap-2">
        <Icon className={report.outcome === "done" ? "size-5 text-success" : report.outcome === "failed" ? "size-5 text-danger" : "size-5 text-warning"} />
        <span className="text-sm text-muted">{titleCounts(report, t).join(" · ")}</span>
      </div>
      {/* The server's own words, in English. */}
      {report.error ? <ErrorNote>{report.error}</ErrorNote> : null}
      {report.started_at ? <StepTimes report={report} started={report.started_at} /> : null}

      {report.libraries.length > 0 ? (
        <div>
          <h3 className="mb-2 text-sm font-semibold">{t("plexImport.report.libraries")}</h3>
          <ul className="divide-y divide-border rounded-lg border border-border bg-surface-2">
            {report.libraries.map((l) => (
              <li key={l.title} className="p-3 text-sm">
                <div className="flex flex-wrap items-center gap-2">
                  {l.library_id != null ? (
                    <Link to={libraryUrl(l.library_id)} className="font-medium hover:text-accent">
                      {l.title}
                    </Link>
                  ) : (
                    <span className="font-medium">{l.title}</span>
                  )}
                  {l.created ? <Badge tone="accent">{t("plexImport.report.created")}</Badge> : null}
                </div>
                <p className="text-xs text-muted">
                  {t("plexImport.report.matched", { count: l.matched })}
                  {l.missed > 0 ? ` · ${t("plexImport.report.missed", { count: l.missed })}` : null}
                </p>
                {l.missed_titles.length > 0 ? (
                  <details className="mt-1 text-xs text-subtle">
                    <summary className="cursor-pointer hover:text-fg">{t("plexImport.report.missedTitles")}</summary>
                    <p className="mt-1">{l.missed_titles.join(", ")}</p>
                  </details>
                ) : null}
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {edits.length > 0 ? (
        <div>
          <h3 className="mb-1 text-sm font-semibold">{t("plexImport.report.edits")}</h3>
          <p className="mb-2 text-xs text-muted">{t("plexImport.report.editsHint")}</p>
          <ul className="divide-y divide-border rounded-lg border border-border bg-surface-2">
            {edits.map(([field, count]) => (
              <li key={field} className="flex items-center justify-between p-3 text-sm">
                <span>{t(`plexImport.report.fields.${field}`)}</span>
                <span className="tabular-nums text-muted">{count.toLocaleString(currentLocale())}</span>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {report.profiles.length > 0 ? (
        <div>
          <h3 className="mb-2 text-sm font-semibold">{t("plexImport.report.people")}</h3>
          <ul className="divide-y divide-border rounded-lg border border-border bg-surface-2">
            {report.profiles.map((p) => (
              <li key={p.title} className="p-3 text-sm">
                <ProfileLine
                  profile={p}
                  history={report.bring?.watch_history ?? true}
                  finished={report.outcome === "done"}
                  missed={report.libraries.some((l) => l.missed > 0)}
                />
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </StepFrame>
  );
}

/** How long the import took, opening on how long each of its steps did. */
function StepTimes({ report, started }: { report: ImportReport; started: number }) {
  const { t } = useTranslation("settings");
  // A step of a few seconds still says it took some.
  const time = (secs: number) => formatDuration(Math.max(secs, 1) * 1000);
  return (
    <details className="text-sm text-muted">
      <summary className="cursor-pointer hover:text-fg">{t("plexImport.report.took", { time: time(report.finished_at - started) })}</summary>
      <ul className="mt-2 space-y-1">
        {(report.steps ?? []).map((row) => (
          <li key={row.step} className="flex justify-between gap-3">
            <span>{t(`plexImport.progress.steps.${row.step}`)}</span>
            <span className="tabular-nums">{time(row.secs)}</span>
          </li>
        ))}
      </ul>
    </details>
  );
}

/** What happened to titles: labels and dates always, the rest when it happened. */
function titleCounts(report: ImportReport, t: TFunction<["settings", "common"]>): string[] {
  const counts = [
    t("plexImport.report.labelsAdded", { count: report.labels_added }),
    t("plexImport.report.datesMoved", { count: report.dates_moved }),
  ];
  if (report.matches_fixed > 0) counts.push(t("plexImport.report.matchesFixed", { count: report.matches_fixed }));
  if (report.collections_made > 0) counts.push(t("plexImport.report.collectionsMade", { count: report.collections_made }));
  if (report.collection_items > 0) counts.push(t("plexImport.report.collectionItems", { count: report.collection_items }));
  if (report.smart_left > 0) counts.push(t("plexImport.report.smartLeft", { count: report.smart_left }));
  return counts;
}

/** A person's line; what they watched only when it was brought. */
function ProfileLine({
  profile: p,
  history,
  finished,
  missed,
}: {
  profile: ProfileReport;
  history: boolean;
  /** The import finished, turning held profiles on again. */
  finished: boolean;
  /** Some of Plex's titles weren't found. */
  missed: boolean;
}) {
  const { t } = useTranslation("settings");
  const systems = useRatingSystems();
  const age = ageBadge(systems.data, null, p.max_age);
  const counts = [
    ...(history
      ? [
          t("plexImport.report.watched", { count: p.watched }),
          t("plexImport.report.resumed", { count: p.resumed }),
          t("plexImport.report.rated", { count: p.rated }),
          t("plexImport.report.plays", { count: p.plays }),
        ]
      : []),
    ...(p.playlists > 0 ? [t("plexImport.report.playlists", { count: p.playlists })] : []),
  ];
  return (
    <>
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium">{p.title}</span>
        {p.created ? <Badge tone="accent">{t("plexImport.report.profileCreated")}</Badge> : null}
        {age ? <Badge tone="info">{age}</Badge> : null}
      </div>
      {p.skipped ? (
        <p className="mt-0.5 flex items-center gap-1 text-xs text-warning">
          <TriangleAlert className="size-3.5 shrink-0" />
          {t(`plexImport.report.skipped.${p.skipped}`)}
        </p>
      ) : counts.length > 0 ? (
        <p className="mt-0.5 text-xs text-muted">{counts.join(" · ")}</p>
      ) : null}
      {p.incomplete ? <p className="mt-0.5 text-xs text-warning">{t("plexImport.report.incomplete")}</p> : null}
      {p.playlists_incomplete ? <p className="mt-0.5 text-xs text-warning">{t("plexImport.report.playlistsIncomplete")}</p> : null}
      {p.not_kept.length > 0 ? (
        <p className="mt-0.5 text-xs text-warning">
          {t("plexImport.report.notKept")} <span className="font-mono">{p.not_kept.join(", ")}</span>
        </p>
      ) : null}
      {p.held && !finished ? <p className="mt-0.5 text-xs text-warning">{t("plexImport.report.held")}</p> : null}
      {/* Titles not found have none of Plex's labels to hide them by. */}
      {missed && p.hide_labels.length > 0 ? <p className="mt-0.5 text-xs text-warning">{t("plexImport.report.missedUnlabelled")}</p> : null}
    </>
  );
}

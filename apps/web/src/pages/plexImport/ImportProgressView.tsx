import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { Link } from "react-router";
import { CircleAlert, CircleCheck, CircleX, TriangleAlert } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useForgetPlex, usePlexImport, type EditedField, type ImportReport, type ProfileReport } from "@/api/plexImport";
import { useRatingSystems } from "@/api/hooks";
import { ConfirmModal } from "@/components/Modal";
import { Badge, Button, PageSpinner, ProgressBar } from "@/components/ui";
import { currentLocale } from "@/i18n";
import { ageBadge } from "@/lib/access";
import { libraryUrl } from "@/lib/urls";
import { ErrorNote, StepFrame } from "./PlexImport";

/** The import running, then its report. */
export function ImportProgressView({ onFinish, onAgain }: { onFinish: () => void; onAgain: () => void }) {
  const { t } = useTranslation(["settings", "common"]);
  const progress = usePlexImport();
  const forget = useForgetPlex();
  const [stopping, setStopping] = useState(false);
  const data = progress.data;

  if (!data) return <PageSpinner />;

  // Idle with no report: the server restarted while it ran, and kept nothing.
  if (data.state === "idle" && !data.report) {
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
        title={t("plexImport.progress.title")}
        description={t("plexImport.progress.description")}
        actions={
          <Button variant="ghost" onClick={() => setStopping(true)}>
            {t("plexImport.progress.stop")}
          </Button>
        }
      >
        <div className="space-y-2" aria-live="polite">
          <div className="flex items-center justify-between text-sm">
            <span className="font-medium">{data.step ? t(`plexImport.progress.steps.${data.step}`) : t("plexImport.progress.starting")}</span>
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
        </div>
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
                <ProfileLine profile={p} history={report.bring?.watch_history ?? true} />
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </StepFrame>
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
function ProfileLine({ profile: p, history }: { profile: ProfileReport; history: boolean }) {
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
    </>
  );
}

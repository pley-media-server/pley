import { useState, type FormEvent, type ReactNode } from "react";
import { Trans, useTranslation } from "react-i18next";
import { Link } from "react-router";
import {
  Activity as ActivityIcon,
  CircleStop,
  Cpu,
  FastForward,
  History,
  Images,
  MonitorPlay,
  Pause,
  Play,
  RefreshCw,
  Square,
} from "lucide-react";
import { errorMessage } from "@/api/client";
import { useActivity, useStopSession, useSystemInfo, useTranscoderStatus } from "@/api/hooks";
import type {
  LibraryWorkStatus,
  PlaybackMethod,
  SessionInfo,
  TranscodeJobState,
  TranscodePipeline,
  TranscoderInfo,
  TranscoderJob,
  TranscoderStatus,
} from "@/api/types";
import { Artwork } from "@/components/Artwork";
import { HistoryList } from "@/components/HistoryList";
import { Modal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { ResourcesSection } from "./ResourcesSection";
import { StorageSection } from "./StorageSection";
import { Badge, Button, Card, EmptyState, Field, ProgressBar, SectionTitle, Skeleton } from "@/components/ui";
import { currentLocale, t as translate } from "@/i18n";
import { versionLabel, WEB_COMMIT, WEB_VERSION } from "@/lib/build";
import { cn } from "@/lib/cn";
import { formatBytes, formatClock, formatTimestamp, formatUptime, fullTitle, transcodeSummary } from "@/lib/format";
import { thumbCandidates } from "@/lib/images";
import { useDocumentTitle } from "@/lib/hooks";
import { itemUrl } from "@/lib/urls";

const METHOD_TONE: Record<PlaybackMethod, "success" | "info" | "accent"> = {
  direct: "success",
  remux: "info",
  transcode: "accent",
};

const PIPELINE_TONE: Record<TranscodePipeline, "success" | "info" | "neutral"> = {
  gpu: "success",
  hybrid: "info",
  cpu: "neutral",
  copy: "neutral",
};

function PipelineBadge({ pipeline, className }: { pipeline: TranscodePipeline | null | undefined; className?: string }) {
  const { t } = useTranslation("settings");
  const tone = pipeline ? PIPELINE_TONE[pipeline] : undefined;
  if (!pipeline || !tone) return null;
  return (
    <Badge tone={tone} className={className}>
      <span title={t(`dashboard.pipelines.${pipeline}.title`)}>{t(`dashboard.pipelines.${pipeline}.label`)}</span>
    </Badge>
  );
}

const JOB_TONE: Record<TranscodeJobState, "success" | "info" | "accent" | "danger" | "neutral"> = {
  starting: "info",
  running: "success",
  idle: "neutral",
  finished: "neutral",
  failed: "danger",
};

/** Why the libraries' intros, credits and previews wait, if they do. */
function libraryWorkWaits(work: LibraryWorkStatus | null): string {
  if (!work) return "";
  switch (work.waiting) {
    case "playing":
      return translate("dashboard.waits.playing", { ns: "settings" });
    case "busy":
      return translate("dashboard.waits.busy", { ns: "settings" });
    case "hours": {
      const mode = work.mode === "auto" ? "auto" : "hours";
      return work.resumes_at
        ? translate(`dashboard.waits.${mode}From`, { ns: "settings", time: formatTimestamp(work.resumes_at) })
        : translate(`dashboard.waits.${mode}`, { ns: "settings" });
    }
    default:
      return "";
  }
}

/** Most a stop message may say (the server's limit). */
const STOP_MESSAGE_MAX = 200;

/** Ends someone's playback, with a message for their screen if wanted. */
function StopSessionModal({ session, onClose }: { session: SessionInfo | null; onClose: () => void }) {
  const { t } = useTranslation(["settings", "common"]);
  const stop = useStopSession();
  const toast = useToast();
  const [message, setMessage] = useState("");
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!session) return;
    stop.mutate(
      { sessionId: session.session_id, message: message.trim() || undefined },
      {
        onSuccess: () => {
          toast.success(t("dashboard.stop.stopped"));
          onClose();
        },
        onError: (err) => toast.error(errorMessage(err)),
      },
    );
  };
  return (
    <Modal
      open={!!session}
      onClose={onClose}
      title={t("dashboard.stop.title")}
      size="sm"
      locked={stop.isPending}
      footer={
        <>
          <Button variant="ghost" onClick={onClose} disabled={stop.isPending}>
            {t("common:actions.cancel")}
          </Button>
          <Button variant="danger" type="submit" form="stop-session-form" loading={stop.isPending}>
            {t("dashboard.stop.stop")}
          </Button>
        </>
      }
    >
      {session ? (
        <form id="stop-session-form" onSubmit={submit} className="space-y-4">
          <p className="text-sm text-muted">
            <Trans
              t={t}
              i18nKey="dashboard.stop.stopsFor"
              values={{ title: fullTitle(session.item), user: session.username }}
              components={{ title: <span className="font-medium text-fg" /> }}
            />
          </p>
          <Field label={t("dashboard.stop.message")} htmlFor="stop-session-message" hint={t("dashboard.stop.messageHint")}>
            <textarea
              id="stop-session-message"
              rows={3}
              value={message}
              maxLength={STOP_MESSAGE_MAX}
              onChange={(e) => setMessage(e.target.value)}
              className="w-full resize-y rounded-md border border-border bg-surface-2 px-3 py-2 text-sm text-fg placeholder:text-subtle transition-colors focus:border-accent focus:outline-none"
            />
          </Field>
        </form>
      ) : null}
    </Modal>
  );
}

function SessionCard({ s, onStop }: { s: SessionInfo; onStop: () => void }) {
  const { t } = useTranslation("settings");
  const duration = s.item.duration_ms ?? 0;
  const tc = s.transcode;
  return (
    <div className="flex gap-4 rounded-lg border border-border bg-surface p-3">
      <Link to={itemUrl(s.item.id)} className="w-32 shrink-0 sm:w-40">
        <Artwork
          candidates={thumbCandidates(s.item)}
          width={160}
          alt=""
          title={s.item.title}
          className={cn("w-full rounded-md", s.item.kind === "track" ? "aspect-square" : "aspect-video")}
        />
      </Link>
      <div className="min-w-0 flex-1 space-y-1.5">
        <div className="flex items-start justify-between gap-2">
          <Link to={itemUrl(s.item.id)} className="line-clamp-2 text-sm font-semibold hover:text-accent">
            {fullTitle(s.item)}
          </Link>
          <Badge tone={METHOD_TONE[s.method]} className="shrink-0">
            {t(`dashboard.methods.${s.method}`)}
          </Badge>
        </div>
        {/* On a line of its own: beside the user, the time and Stop there is no room for it in two columns. */}
        {s.device ? (
          <div className="truncate text-xs text-muted" title={s.device}>
            {s.device}
          </div>
        ) : null}
        <div className="flex items-center gap-1.5 text-xs text-muted">
          {s.state === "playing" ? (
            <Play className="size-3 fill-current text-success" />
          ) : s.state === "paused" ? (
            <Pause className="size-3 fill-current" />
          ) : (
            <Square className="size-3 fill-current" />
          )}
          <span className="font-medium text-fg/85">{s.username}</span>
          <span>·</span>
          <span className="shrink-0 whitespace-nowrap tabular-nums">
            {formatClock(s.position_ms / 1000)}
            {duration ? ` / ${formatClock(duration / 1000)}` : ""}
          </span>
          <button
            type="button"
            onClick={onStop}
            aria-label={
              s.device
                ? t("dashboard.stop.labelOn", { title: fullTitle(s.item), user: s.username, device: s.device })
                : t("dashboard.stop.label", { title: fullTitle(s.item), user: s.username })
            }
            title={t("dashboard.stop.title")}
            className="-my-1 ml-auto flex items-center gap-1 rounded-md px-2 py-1 font-medium text-muted transition-colors hover:bg-danger/10 hover:text-danger"
          >
            <CircleStop className="size-3.5" />
            {t("dashboard.stop.stop")}
          </button>
        </div>
        {duration ? <ProgressBar value={s.position_ms / duration} className="h-1" /> : null}
        {tc ? (
          <div className="space-y-1 pt-0.5">
            <div className="flex items-start gap-1.5">
              <PipelineBadge pipeline={tc.pipeline} className="mt-px shrink-0" />
              <div className="text-xs leading-snug text-fg/75" title={transcodeSummary(tc)}>
                {transcodeSummary(tc) || t("dashboard.preparing")}
              </div>
            </div>
            {tc.segments_total > 0 ? (
              <div className="flex items-center gap-2 text-[11px] text-subtle">
                <ProgressBar value={tc.segments_ready / tc.segments_total} className="h-0.5 flex-1" barClassName="bg-info" />
                <span className="tabular-nums">{t("dashboard.segments", { ready: tc.segments_ready, total: tc.segments_total })}</span>
              </div>
            ) : null}
            {tc.last_error ? <div className="text-xs text-danger">{tc.last_error}</div> : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}

function Stat({ label, value, sub }: { label: string; value: ReactNode; sub?: ReactNode }) {
  return (
    <div className="rounded-lg border border-border bg-surface px-4 py-3">
      <div className="text-xs font-medium text-subtle">{label}</div>
      <div className="mt-1 text-xl font-semibold tabular-nums">{value}</div>
      {sub ? <div className="text-xs text-muted">{sub}</div> : null}
    </div>
  );
}

function Chips({ items, active, empty }: { items: string[]; active?: string | null; empty?: string }) {
  const { t } = useTranslation("common");
  if (!items.length) return <span className="text-sm text-subtle">{empty ?? t("states.none")}</span>;
  return (
    <div className="flex flex-wrap gap-1.5">
      {items.map((e) => (
        <span
          key={e}
          className={cn(
            "rounded-full px-2 py-0.5 font-mono text-[11px]",
            e === active ? "bg-accent/15 text-accent ring-1 ring-accent/40" : "bg-white/[0.07] text-fg/80",
          )}
        >
          {e}
        </span>
      ))}
    </div>
  );
}

function InfoRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[8.5rem_1fr] gap-3 py-1.5 text-sm">
      <dt className="text-subtle">{label}</dt>
      <dd className="min-w-0">{children}</dd>
    </div>
  );
}

function TranscoderCard({ tc }: { tc: TranscoderInfo }) {
  const { t } = useTranslation("settings");
  const available = tc.available ? t("dashboard.transcoder.available") : t("dashboard.transcoder.unavailable");
  return (
    <Card>
      <div className="mb-3 flex flex-wrap items-center gap-2.5">
        <span
          className={cn("size-2.5 rounded-full ring-4", tc.available ? "bg-success ring-success/20" : "bg-danger ring-danger/20")}
          aria-label={available}
        />
        <h3 className="text-base font-semibold">{t("dashboard.transcoder.title")}</h3>
        <span className="text-xs text-muted">{tc.managed ? t("dashboard.transcoder.managed") : t("dashboard.transcoder.external")}</span>
      </div>
      {tc.error ? <p className="mb-3 rounded-md border border-danger/40 bg-danger/10 px-3 py-2 text-sm text-danger">{tc.error}</p> : null}
      <dl>
        <InfoRow label={t("dashboard.transcoder.status")}>{available}</InfoRow>
        {tc.version ? <InfoRow label={t("dashboard.transcoder.version")}>pley-transcoder {versionLabel(tc.version, tc.commit)}</InfoRow> : null}
        {tc.url ? (
          <InfoRow label="URL">
            <span className="break-all font-mono text-xs">{tc.url}</span>
          </InfoRow>
        ) : null}
        <InfoRow label="ffmpeg">{tc.ffmpeg_version ?? "—"}</InfoRow>
        <InfoRow label={t("dashboard.transcoder.encoders")}>
          <Chips items={tc.encoders} active={tc.active_encoder} />
        </InfoRow>
        <InfoRow label={t("dashboard.transcoder.activeEncoder")}>
          <span className="font-mono text-xs">{tc.active_encoder ?? "—"}</span>
        </InfoRow>
        <InfoRow label={t("dashboard.transcoder.hwAccel")}>
          <Chips items={tc.hwaccels} />
        </InfoRow>
        <InfoRow label={t("dashboard.transcoder.toneMapping")}>
          <Chips items={tc.tonemapping} empty={t("dashboard.transcoder.notAvailable")} />
        </InfoRow>
        <InfoRow label="Dolby Vision">
          {tc.dolby_vision ? (
            <Badge tone="success">{t("dashboard.transcoder.supported")}</Badge>
          ) : (
            <Badge>{t("dashboard.transcoder.notSupported")}</Badge>
          )}
        </InfoRow>
        <InfoRow label={t("dashboard.transcoder.jobs")}>
          <span className="tabular-nums">{t("dashboard.transcoder.jobCount", { active: tc.active_jobs, max: tc.max_jobs })}</span>
        </InfoRow>
      </dl>
    </Card>
  );
}

function JobsTable({ jobs }: { jobs: TranscoderJob[] }) {
  const { t } = useTranslation("settings");
  if (!jobs.length) {
    return <p className="rounded-lg border border-dashed border-border px-4 py-6 text-center text-sm text-subtle">{t("dashboard.jobs.none")}</p>;
  }
  return (
    <div className="overflow-x-auto rounded-lg border border-border">
      <table className="w-full min-w-[960px] text-left text-xs">
        <thead className="bg-surface-2 text-subtle">
          <tr>
            <th className="px-3 py-2 font-semibold">{t("dashboard.jobs.job")}</th>
            <th className="px-3 py-2 font-semibold">{t("dashboard.jobs.state")}</th>
            <th className="px-3 py-2 font-semibold">{t("dashboard.jobs.video")}</th>
            <th className="px-3 py-2 font-semibold">{t("dashboard.jobs.audio")}</th>
            <th className="px-3 py-2 font-semibold">{t("dashboard.jobs.codecs")}</th>
            <th className="px-3 py-2 font-semibold">{t("dashboard.jobs.toneMap")}</th>
            <th className="px-3 py-2 text-right font-semibold">{t("dashboard.jobs.speed")}</th>
            <th className="px-3 py-2 text-right font-semibold">{t("dashboard.jobs.fps")}</th>
            <th className="w-40 px-3 py-2 font-semibold">{t("dashboard.jobs.segments")}</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-white/[0.05] bg-surface">
          {jobs.map((j) => {
            const i = j.info;
            return (
              <tr key={j.id} className="align-top">
                <td className="max-w-56 px-3 py-2">
                  <div className="truncate font-medium text-fg" title={j.label ?? j.id}>
                    {j.label ?? j.id}
                  </div>
                  <div className="truncate text-subtle">
                    {j.owner ?? "—"}
                    {j.restarts > 0 ? ` · ${t("dashboard.jobs.restarts", { count: j.restarts })}` : ""}
                    {j.served_bytes > 0 ? ` · ${formatBytes(j.served_bytes)}` : ""}
                  </div>
                  {i.last_error ? <div className="mt-1 text-danger">{i.last_error}</div> : null}
                  {j.warnings.length ? <div className="mt-1 text-accent">{j.warnings.join("; ")}</div> : null}
                </td>
                <td className="px-3 py-2">
                  <Badge tone={JOB_TONE[j.state]}>{t(`dashboard.jobStates.${j.state}`)}</Badge>
                  {i.subtitle_burn ? <div className="mt-1 text-subtle">{t("dashboard.jobs.burnIn")}</div> : null}
                </td>
                <td className="px-3 py-2 text-fg/85">
                  {i.video === "none"
                    ? "—"
                    : i.video === "copy"
                      ? t("dashboard.jobs.copied", { codec: i.video_in ?? "" })
                      : `${i.video_in ?? "?"} → ${i.video_out ?? "?"}`}
                </td>
                <td className="px-3 py-2 text-fg/85">
                  {i.audio === "none"
                    ? "—"
                    : i.audio === "copy"
                      ? t("dashboard.jobs.copied", { codec: i.audio_in ?? "" })
                      : `${i.audio_in ?? "?"} → ${i.audio_out ?? "?"}`}
                </td>
                <td className="px-3 py-2 font-mono text-fg/80">
                  <div className="flex items-center gap-1.5">
                    {i.encoder ?? (i.video === "copy" ? "copy" : "—")}
                    <PipelineBadge pipeline={i.pipeline} className="font-sans" />
                  </div>
                  {i.preset ? <div className="text-subtle">{t("dashboard.jobs.preset", { preset: i.preset })}</div> : null}
                  <div className="text-subtle">{i.decoder ?? ""}</div>
                </td>
                <td className="px-3 py-2 font-mono text-fg/80">{i.tonemap ?? "—"}</td>
                <td className="px-3 py-2 text-right tabular-nums">{i.speed != null ? `${i.speed.toFixed(1)}×` : "—"}</td>
                <td className="px-3 py-2 text-right tabular-nums">{i.fps != null ? Math.round(i.fps) : "—"}</td>
                <td className="px-3 py-2">
                  {i.segments_total > 0 ? (
                    <div className="space-y-1">
                      <ProgressBar value={i.segments_ready / i.segments_total} className="h-1" />
                      <div className="tabular-nums text-subtle">
                        {i.segments_ready} / {i.segments_total}
                      </div>
                    </div>
                  ) : (
                    <span className="text-subtle">—</span>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function TranscoderLive({ status }: { status: TranscoderStatus }) {
  const { t } = useTranslation("settings");
  const c = status.counters;
  const avgProbe = c.probes > 0 ? Math.round(c.probe_ms_total / c.probes) : null;
  const lang = currentLocale();
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3 md:grid-cols-3 xl:grid-cols-6">
        <Stat
          label={t("dashboard.live.runningJobs")}
          value={`${status.running_processes} / ${status.max_jobs}`}
          sub={t("dashboard.live.created", { count: c.jobs_created })}
        />
        <Stat label={t("dashboard.live.hwSessions")} value={`${status.hw_processes} / ${status.max_hw_jobs}`} />
        <Stat
          label={t("dashboard.live.segmentsServed")}
          value={c.segments_served.toLocaleString(lang)}
          sub={t("dashboard.live.produced", { count: c.segments_produced })}
        />
        <Stat
          label={t("dashboard.live.dataServed")}
          value={formatBytes(c.bytes_served)}
          sub={t("dashboard.live.onDisk", { size: formatBytes(status.work_dir_bytes) })}
        />
        <Stat
          label={t("dashboard.live.probes")}
          value={c.probes.toLocaleString(lang)}
          sub={avgProbe != null ? t("dashboard.live.average", { ms: avgProbe }) : undefined}
        />
        <Stat
          label={t("dashboard.live.failures")}
          value={(c.jobs_failed + c.process_failures + c.probe_failures).toLocaleString(lang)}
          sub={t("dashboard.live.thumbsAndSubs", { thumbs: c.thumbnails.toLocaleString(lang), subs: c.subtitle_extractions.toLocaleString(lang) })}
        />
      </div>
      <JobsTable jobs={status.jobs} />
    </div>
  );
}

export function DashboardSettings() {
  const { t } = useTranslation(["settings", "common"]);
  useDocumentTitle(t("dashboard.title"));
  const activity = useActivity({ poll: 5_000 });
  const system = useSystemInfo();
  const transcoder = useTranscoderStatus(2_500);
  const [stopping, setStopping] = useState<SessionInfo | null>(null);

  const sessions = activity.data?.sessions ?? [];
  const scans = (activity.data?.scans ?? []).filter((s) => s.phase !== "done");
  const analysis = activity.data?.analysis ?? null;
  const seasonsPending = activity.data?.seasons_pending ?? 0;
  const previewsPending = activity.data?.previews_pending ?? 0;
  const previewsRunning = activity.data?.previews_running ?? 0;
  const waits = libraryWorkWaits(activity.data?.library_work ?? null);

  return (
    <div className="space-y-10">
      <section>
        <SectionTitle>
          <span className="flex items-center gap-2">
            <MonitorPlay className="size-5 text-accent" /> {t("dashboard.nowPlaying")}
            {sessions.length ? <span className="text-sm font-normal text-subtle">{sessions.length}</span> : null}
          </span>
        </SectionTitle>
        {activity.isPending ? (
          <Skeleton className="h-28" />
        ) : sessions.length === 0 ? (
          <p className="rounded-lg border border-dashed border-border px-4 py-6 text-center text-sm text-subtle">{t("dashboard.nobodyWatching")}</p>
        ) : (
          <div className="grid gap-3 xl:grid-cols-2">
            {sessions.map((s) => (
              <SessionCard key={s.session_id} s={s} onStop={() => setStopping(s)} />
            ))}
          </div>
        )}
        {/* Keyed so the message starts blank for every session. */}
        <StopSessionModal key={stopping?.session_id ?? ""} session={stopping} onClose={() => setStopping(null)} />
      </section>

      <ResourcesSection />

      <section>
        <SectionTitle>
          <span className="flex items-center gap-2">
            <History className="size-5 text-accent" /> {t("dashboard.recentlyPlayed")}
          </span>
        </SectionTitle>
        <HistoryList scope={{ all: true }} showUser pageSize={10} empty={t("dashboard.nothingPlayed")} />
      </section>

      <section>
        <SectionTitle>
          <span className="flex items-center gap-2">
            <Cpu className="size-5 text-accent" /> {t("dashboard.transcoder.title")}
          </span>
        </SectionTitle>
        {transcoder.isPending ? (
          <Skeleton className="h-40" />
        ) : transcoder.isError ? (
          <p className="rounded-lg border border-danger/40 bg-danger/10 px-4 py-3 text-sm text-danger">
            {t("dashboard.transcoder.statusUnavailable", { error: errorMessage(transcoder.error) })}
          </p>
        ) : (
          <TranscoderLive status={transcoder.data} />
        )}
      </section>

      {scans.length ? (
        <section>
          <SectionTitle>
            <span className="flex items-center gap-2">
              <RefreshCw className="size-5 animate-spin text-accent" /> {t("dashboard.libraryScans")}
            </span>
          </SectionTitle>
          <div className="space-y-2">
            {scans.map((s) => (
              <div key={s.library_id} className="rounded-lg border border-border bg-surface px-4 py-3">
                <div className="mb-1.5 flex items-center justify-between text-sm">
                  <span className="font-medium">{s.library_name}</span>
                  <span className="text-xs text-muted">
                    {t(`scanPhases.${s.phase}`)}
                    {s.total > 0 ? ` · ${s.processed.toLocaleString(currentLocale())} / ${s.total.toLocaleString(currentLocale())}` : ""}
                  </span>
                </div>
                <ProgressBar value={s.total > 0 ? s.processed / s.total : 0} className="h-1.5" />
              </div>
            ))}
          </div>
        </section>
      ) : null}

      {analysis || seasonsPending > 0 ? (
        <section>
          <SectionTitle>
            <span className="flex items-center gap-2">
              <FastForward className="size-5 text-accent" /> {t("dashboard.findingMarkers")}
            </span>
          </SectionTitle>
          <div className="space-y-2 rounded-lg border border-border bg-surface px-4 py-3">
            {analysis ? (
              <div>
                <div className="mb-1.5 flex items-center justify-between text-sm">
                  <span className="font-medium">{analysis.title}</span>
                  <span className="text-xs text-muted">{t("dashboard.fingerprints", { done: analysis.done, total: analysis.total })}</span>
                </div>
                <ProgressBar value={analysis.total > 0 ? analysis.done / analysis.total : 0} className="h-1.5" />
              </div>
            ) : null}
            {seasonsPending > 0 ? (
              <p className="text-sm text-muted">
                {t("dashboard.seasonsLeft", { count: seasonsPending })}
                {waits ? ` · ${waits}` : ""}
              </p>
            ) : null}
          </div>
        </section>
      ) : null}

      {previewsPending > 0 ? (
        <section>
          <SectionTitle>
            <span className="flex items-center gap-2">
              <Images className="size-5 text-accent" /> {t("dashboard.makingPreviews")}
            </span>
          </SectionTitle>
          <p className="rounded-lg border border-border bg-surface px-4 py-3 text-sm text-muted">
            {t("dashboard.filesLeft", { count: previewsPending })}
            {previewsRunning > 1 ? ` · ${t("dashboard.atATime", { n: previewsRunning })}` : ""}
            {waits ? ` · ${waits}` : ""}
          </p>
        </section>
      ) : null}

      <section>
        <SectionTitle>
          <span className="flex items-center gap-2">
            <ActivityIcon className="size-5 text-accent" /> {t("dashboard.system")}
          </span>
        </SectionTitle>
        {system.isPending ? (
          <Skeleton className="h-48" />
        ) : system.isError ? (
          <EmptyState
            title={t("dashboard.couldntLoadSystem")}
            description={errorMessage(system.error)}
            action={<Button onClick={() => void system.refetch()}>{t("common:actions.retry")}</Button>}
          />
        ) : (
          <div className="grid gap-4 xl:grid-cols-2">
            <div className="space-y-4">
              <Card>
                <h3 className="mb-3 text-base font-semibold">{system.data.name}</h3>
                <dl>
                  <InfoRow label={t("dashboard.version")}>Pley {versionLabel(system.data.version, system.data.commit)}</InfoRow>
                  <InfoRow label={t("dashboard.webApp")}>{versionLabel(WEB_VERSION, WEB_COMMIT)}</InfoRow>
                  <InfoRow label={t("dashboard.platform")}>
                    {system.data.os} · {system.data.arch}
                  </InfoRow>
                  <InfoRow label={t("dashboard.uptime")}>{formatUptime(system.data.uptime_secs)}</InfoRow>
                  <InfoRow label={t("dashboard.dataDirectory")}>
                    <span className="break-all font-mono text-xs">{system.data.data_dir}</span>
                  </InfoRow>
                </dl>
              </Card>
              <div className="grid grid-cols-3 gap-3">
                <Stat label={t("dashboard.counts.movies")} value={system.data.counts.movies.toLocaleString(currentLocale())} />
                <Stat
                  label={t("dashboard.counts.shows")}
                  value={system.data.counts.shows.toLocaleString(currentLocale())}
                  sub={t("dashboard.counts.episodes", { count: system.data.counts.episodes })}
                />
                <Stat
                  label={t("dashboard.counts.artists")}
                  value={system.data.counts.artists.toLocaleString(currentLocale())}
                  sub={t("dashboard.counts.albums", { count: system.data.counts.albums })}
                />
                <Stat label={t("dashboard.counts.tracks")} value={system.data.counts.tracks.toLocaleString(currentLocale())} />
                <Stat label={t("dashboard.counts.files")} value={system.data.counts.files.toLocaleString(currentLocale())} />
                <Stat
                  label={t("dashboard.counts.users")}
                  value={system.data.counts.users.toLocaleString(currentLocale())}
                  sub={t("dashboard.counts.libraries", { count: system.data.counts.libraries })}
                />
              </div>
            </div>
            <TranscoderCard tc={system.data.transcoder} />
          </div>
        )}
      </section>

      <StorageSection />
    </div>
  );
}

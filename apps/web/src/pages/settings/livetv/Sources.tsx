import { useState } from "react";
import { CalendarClock, CircleAlert, FileText, KeyRound, LoaderCircle, Pencil, Plus, Radio, RefreshCw, Satellite, Square, Trash2, Users } from "lucide-react";
import { useTranslation } from "react-i18next";
import { errorMessage } from "@/api/client";
import {
  useDeleteSource,
  useLiveSources,
  useRefreshSource,
  useStopTuner,
  useTuners,
  useUpdateSource,
  type LiveSource,
  type TunerStatus,
} from "@/api/livetv";
import { ConfirmModal } from "@/components/Modal";
import { useToast } from "@/components/Toast";
import { Badge, Button, Card, CardTitle, EmptyState, IconButton, PageSpinner, Toggle } from "@/components/ui";
import { currentLanguage } from "@/i18n";
import { cn } from "@/lib/cn";
import { formatAgo } from "@/lib/format";
import { accountLine, expiresSoon, formatFor, formatKbps, planLine } from "./common";
import { SourceModal } from "./SourceModal";

export function Sources() {
  const { t } = useTranslation(["livetv", "common"]);
  const sources = useLiveSources();
  const [editing, setEditing] = useState<LiveSource | null>(null);
  const [adding, setAdding] = useState(false);
  const [deleting, setDeleting] = useState<LiveSource | null>(null);
  const remove = useDeleteSource();
  const toast = useToast();

  if (sources.isPending) return <PageSpinner />;
  if (sources.isError) {
    return (
      <EmptyState
        title={t("settings.sources.couldntLoad")}
        description={errorMessage(sources.error)}
        action={<Button onClick={() => void sources.refetch()}>{t("common:actions.retry")}</Button>}
      />
    );
  }
  const list = sources.data;
  return (
    <div className="space-y-5">
      <Connections sources={list} />

      <div className="flex items-center justify-between gap-3">
        <div>
          <h3 className="text-base font-semibold">{t("settings.sources.title")}</h3>
          <p className="text-sm text-muted">{t("settings.sources.hint")}</p>
        </div>
        {list.length ? (
          <Button variant="primary" icon={<Plus className="size-4" />} onClick={() => setAdding(true)}>
            {t("settings.sources.add")}
          </Button>
        ) : null}
      </div>

      {list.length === 0 ? (
        <Card className="py-2">
          <EmptyState
            icon={<Satellite />}
            title={t("settings.sources.noneTitle")}
            description={t("settings.sources.noneHint")}
            action={
              <Button variant="primary" icon={<Plus className="size-4" />} onClick={() => setAdding(true)}>
                {t("settings.sources.addFirst")}
              </Button>
            }
          />
        </Card>
      ) : (
        <ul className="space-y-3">
          {list.map((s) => (
            <li key={s.id}>
              <SourceCard source={s} onEdit={() => setEditing(s)} onDelete={() => setDeleting(s)} />
            </li>
          ))}
        </ul>
      )}

      <SourceModal open={adding || editing !== null} source={editing} onClose={() => (setAdding(false), setEditing(null))} />
      <ConfirmModal
        open={deleting !== null}
        onClose={() => setDeleting(null)}
        title={t("settings.sources.removeTitle", { name: deleting?.name ?? t("settings.sources.thisSource") })}
        message={t("settings.sources.removeMessage")}
        confirmLabel={t("settings.sources.remove")}
        danger
        loading={remove.isPending}
        onConfirm={() =>
          deleting &&
          remove.mutate(deleting.id, {
            onSuccess: () => {
              toast.success(t("settings.sources.removed", { name: deleting.name }));
              setDeleting(null);
            },
            onError: (e) => toast.error(errorMessage(e)),
          })
        }
      />
    </div>
  );
}

function SourceCard({ source: s, onEdit, onDelete }: { source: LiveSource; onEdit: () => void; onDelete: () => void }) {
  const { t } = useTranslation(["livetv", "common"]);
  const refresh = useRefreshSource();
  const update = useUpdateSource();
  const toast = useToast();
  const soon = expiresSoon(s.account);
  const busy = s.refreshing || refresh.isPending;
  const visible = s.channel_count - s.hidden_count;
  const guided = s.channel_count ? Math.round((s.guided_count / s.channel_count) * 100) : 0;
  const Kind = s.kind === "xtream" ? KeyRound : FileText;
  return (
    <Card className={cn("p-4 transition-opacity sm:p-5", !s.enabled && "opacity-60")}>
      <div className="flex items-start gap-4">
        <div className="hidden size-11 shrink-0 items-center justify-center rounded-lg bg-surface-2 text-accent sm:flex">
          <Kind className="size-5" />
        </div>
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h4 className="truncate font-semibold">{s.name}</h4>
            <Badge>{s.kind === "xtream" ? "Xtream Codes" : "M3U"}</Badge>
            {!s.enabled ? <Badge>{t("settings.sources.off")}</Badge> : null}
            {busy ? (
              <Badge tone="info">
                <LoaderCircle className="size-3 animate-spin" /> {t("settings.sources.reading")}
              </Badge>
            ) : null}
          </div>
          <div className="mt-0.5 truncate font-mono text-xs text-subtle" title={s.url}>
            {s.url}
          </div>

          <dl className="mt-3 grid grid-cols-2 gap-x-6 gap-y-2 text-sm sm:grid-cols-4">
            <Stat
              label={t("channels")}
              value={s.channels_at ? visible.toLocaleString(currentLanguage()) : "—"}
              note={s.hidden_count ? t("settings.sources.hidden", { count: s.hidden_count }) : undefined}
            />
            <Stat
              label={t("settings.sources.inGuide")}
              value={s.guide_at ? (guided / 100).toLocaleString(currentLanguage(), { style: "percent" }) : "—"}
              note={s.guide_at ? t("settings.sources.guideChannels", { count: s.guided_count }) : t("settings.sources.noGuideYet")}
            />
            <Stat
              label={t("settings.sources.connections")}
              value={
                s.max_connections === 0
                  ? t("settings.sources.inUseCount", { count: s.connections_in_use })
                  : t("settings.sources.ofMax", { inUse: s.connections_in_use, max: s.max_connections })
              }
              note={
                s.max_connections === 0
                  ? t("settings.sources.noLimit")
                  : s.connections_in_use >= s.max_connections
                    ? t("settings.sources.allInUse")
                    : t("settings.sources.inUse")
              }
              meter={s.max_connections ? s.connections_in_use / s.max_connections : undefined}
            />
            <Stat
              label={t("settings.sources.read")}
              value={s.channels_at ? formatAgo(s.channels_at) : t("common:format.never")}
              note={s.guide_at ? t("settings.sources.guideAgo", { ago: formatAgo(s.guide_at) }) : undefined}
            />
          </dl>

          {s.account ? (
            <div className={cn("mt-3 flex items-center gap-1.5 text-xs", soon !== null ? "text-danger" : "text-muted")}>
              <CalendarClock className="size-3.5" />
              {accountLine(s.account)}
              {soon !== null ? (
                <strong className="ml-1">· {soon <= 0 ? t("settings.sources.renew") : t("settings.sources.daysLeft", { count: soon })}</strong>
              ) : null}
            </div>
          ) : null}
          {s.channels_error ? <Problem>{s.channels_error}</Problem> : null}
          {s.guide_error ? <Problem>{t("settings.sources.guideError", { error: s.guide_error })}</Problem> : null}
        </div>
        <div className="flex shrink-0 flex-col items-end gap-2 sm:flex-row sm:items-center">
          <Toggle
            checked={s.enabled}
            label={t(s.enabled ? "settings.sources.turnOff" : "settings.sources.turnOn", { name: s.name })}
            onChange={(enabled) =>
              update.mutate(
                {
                  id: s.id,
                  input: {
                    name: s.name,
                    kind: s.kind,
                    url: s.url,
                    username: s.username,
                    guide_url: s.guide_url,
                    user_agent: s.user_agent,
                    max_connections: s.max_connections,
                    enabled,
                  },
                },
                { onError: (e) => toast.error(errorMessage(e)) },
              )
            }
          />
          <div className="flex">
            <IconButton
              label={t("settings.sources.readNow")}
              size="sm"
              disabled={busy || !s.enabled}
              onClick={() => refresh.mutate(s.id, { onError: (e) => toast.error(errorMessage(e)) })}
            >
              <RefreshCw className={cn("size-4", busy && "animate-spin")} />
            </IconButton>
            <IconButton label={t("common:actions.edit")} size="sm" onClick={onEdit}>
              <Pencil className="size-4" />
            </IconButton>
            <IconButton label={t("settings.sources.remove")} size="sm" onClick={onDelete} className="hover:text-danger">
              <Trash2 className="size-4" />
            </IconButton>
          </div>
        </div>
      </div>
    </Card>
  );
}

function Stat({ label, value, note, meter }: { label: string; value: string; note?: string; meter?: number }) {
  return (
    <div className="min-w-0">
      <dt className="text-xs text-subtle">{label}</dt>
      <dd className="font-semibold tabular-nums">{value}</dd>
      {meter !== undefined ? (
        <div className="mt-1 h-1 w-full max-w-24 overflow-hidden rounded-full bg-white/10">
          <div className={cn("h-full rounded-full", meter >= 1 ? "bg-danger" : "bg-accent")} style={{ width: `${Math.min(1, meter) * 100}%` }} />
        </div>
      ) : null}
      {note ? <dd className="truncate text-xs text-subtle">{note}</dd> : null}
    </div>
  );
}

function Problem({ children }: { children: React.ReactNode }) {
  return (
    <p className="mt-2 flex items-start gap-1.5 text-xs text-danger">
      <CircleAlert className="mt-px size-3.5 shrink-0" />
      <span className="break-words">{children}</span>
    </p>
  );
}

/** What is being watched now: each connection, who is on it, what it carries. */
function Connections({ sources }: { sources: LiveSource[] }) {
  const { t } = useTranslation("livetv");
  const tuners = useTuners(sources.length > 0);
  const stop = useStopTuner();
  const toast = useToast();
  if (!sources.length) return null;
  const list = tuners.data ?? [];
  return (
    <Card>
      <CardTitle icon={<Radio />}>{t("settings.sources.watchingNow")}</CardTitle>
      {list.length === 0 ? (
        <p className="text-sm text-muted">{t("settings.sources.nobodyWatching")}</p>
      ) : (
        <ul className="divide-y divide-border">
          {list.map((t) => (
            <TunerRow
              key={t.channel_id}
              tuner={t}
              stopping={stop.isPending && stop.variables === t.channel_id}
              onStop={() => stop.mutate(t.channel_id, { onError: (e) => toast.error(errorMessage(e)) })}
            />
          ))}
        </ul>
      )}
    </Card>
  );
}

function TunerRow({ tuner, stopping, onStop }: { tuner: TunerStatus; stopping: boolean; onStop: () => void }) {
  const { t } = useTranslation("livetv");
  return (
    <li className="flex items-center gap-3 py-3 first:pt-0 last:pb-0">
      <span className={cn("size-2 shrink-0 rounded-full", tuner.error ? "bg-danger" : tuner.connected ? "bg-success animate-pulse" : "bg-white/30")} />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5">
          <span className="truncate font-medium">{tuner.channel_name}</span>
          <span className="text-xs text-subtle">{tuner.source_name}</span>
          {tuner.idle ? <Badge>{t("settings.sources.lettingGo")}</Badge> : null}
        </div>
        <div className="mt-0.5 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-muted">
          <span className="flex items-center gap-1">
            <Users className="size-3" />
            {tuner.viewers.length ? tuner.viewers.map((v) => v.username).join(", ") : t("settings.sources.nobody")}
          </span>
          {tuner.kbps ? <span className="tabular-nums">{formatKbps(tuner.kbps)}</span> : null}
          <span>{formatFor(tuner.started_at)}</span>
          {tuner.plan ? <span className="truncate">{planLine(tuner.plan)}</span> : null}
        </div>
        {tuner.error ? <div className="mt-0.5 truncate text-xs text-danger">{tuner.error}</div> : null}
      </div>
      <Button size="sm" variant="ghost" icon={<Square className="size-3.5 fill-current" />} loading={stopping} onClick={onStop}>
        {t("settings.sources.stop")}
      </Button>
    </li>
  );
}

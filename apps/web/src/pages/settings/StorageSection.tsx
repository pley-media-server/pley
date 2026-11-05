import { useTranslation } from "react-i18next";
import { AlertTriangle, DatabaseBackup, HardDrive, Sparkles } from "lucide-react";
import { errorMessage } from "@/api/client";
import { useBackups, useCleanStorage, useCreateBackup, useStorageUsage } from "@/api/hooks";
import { useToast } from "@/components/Toast";
import { Button, Card, SectionTitle, Skeleton } from "@/components/ui";
import { formatBytes, formatRelative, formatTimestamp } from "@/lib/format";

function Usage({ label, bytes, sub }: { label: string; bytes: number; sub: string }) {
  return (
    <div className="rounded-lg border border-border bg-surface px-4 py-3">
      <div className="text-xs font-medium text-subtle">{label}</div>
      <div className="mt-1 text-xl font-semibold tabular-nums">{formatBytes(bytes)}</div>
      <div className="text-xs text-muted">{sub}</div>
    </div>
  );
}

/** Space used, database health and backups (docs/STORAGE.md). */
export function StorageSection() {
  const { t } = useTranslation("settings");
  const toast = useToast();
  const usage = useStorageUsage();
  const backups = useBackups();
  const clean = useCleanStorage();
  const backup = useCreateBackup();
  const health = backups.data?.health;
  const list = backups.data?.backups ?? [];

  return (
    <section>
      <SectionTitle
        action={
          <Button
            size="sm"
            variant="ghost"
            icon={<Sparkles className="size-4" />}
            loading={clean.isPending}
            onClick={() =>
              clean.mutate(undefined, {
                onSuccess: (r) =>
                  toast.success(
                    r.bytes_freed > 0
                      ? t("storage.removed", { count: r.blobs_removed + r.media_folders_removed, size: formatBytes(r.bytes_freed) })
                      : t("storage.nothingToClean"),
                  ),
                onError: (e) => toast.error(errorMessage(e)),
              })
            }
          >
            {t("storage.cleanUp")}
          </Button>
        }
      >
        <span className="flex items-center gap-2">
          <HardDrive className="size-5 text-accent" /> {t("storage.title")}
        </span>
      </SectionTitle>

      {health?.ok === false ? (
        <div className="mb-3 flex gap-3 rounded-lg border border-danger/40 bg-danger/10 px-4 py-3 text-sm">
          <AlertTriangle className="mt-0.5 size-4 shrink-0 text-danger" />
          <div>
            <div className="font-medium text-danger">{t("storage.damaged")}</div>
            <div className="text-muted">{t("storage.damagedHint", { problem: health.problem })}</div>
          </div>
        </div>
      ) : null}

      <div className="grid gap-3 lg:grid-cols-[1fr_1fr]">
        {usage.isPending ? (
          <Skeleton className="h-24" />
        ) : usage.isError ? (
          <p className="text-sm text-danger">{errorMessage(usage.error)}</p>
        ) : (
          <div className="grid grid-cols-3 gap-3">
            <Usage label={t("storage.pictures")} bytes={usage.data.blob_bytes} sub={t("storage.storedOnce", { count: usage.data.blobs })} />
            <Usage label={t("storage.generated")} bytes={usage.data.media_bytes} sub={t("storage.previewsOf", { count: usage.data.media_folders })} />
            <Usage label={t("storage.cache")} bytes={usage.data.cache_bytes} sub={t("storage.safeToDelete")} />
          </div>
        )}
        <Card>
          <div className="mb-2 flex items-center justify-between gap-3">
            <h3 className="flex items-center gap-2 text-sm font-semibold">
              <DatabaseBackup className="size-4 text-accent" /> {t("storage.backups")}
            </h3>
            <Button
              size="sm"
              variant="secondary"
              loading={backup.isPending}
              onClick={() =>
                backup.mutate(undefined, {
                  onSuccess: (b) => toast.success(t("storage.backedUp", { size: formatBytes(b.bytes) })),
                  onError: (e) => toast.error(errorMessage(e)),
                })
              }
            >
              {t("storage.backUpNow")}
            </Button>
          </div>
          {backups.isPending ? (
            <Skeleton className="h-10" />
          ) : list.length === 0 ? (
            <p className="text-sm text-subtle">{t("storage.noBackups")}</p>
          ) : (
            <ul className="divide-y divide-white/[0.05] text-sm">
              {list.map((b) => (
                <li key={b.name} className="flex items-center justify-between gap-3 py-1.5">
                  <span className="truncate font-mono text-xs" title={b.path}>
                    {b.name}
                  </span>
                  <span className="shrink-0 text-xs text-muted" title={formatTimestamp(b.created_at)}>
                    {formatBytes(b.bytes)} · {formatRelative(b.created_at)}
                  </span>
                </li>
              ))}
            </ul>
          )}
        </Card>
      </div>
    </section>
  );
}

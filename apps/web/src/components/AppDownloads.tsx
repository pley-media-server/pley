import { useCallback, useRef, useState } from "react";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";
import {
  coming,
  fractionOf,
  useAppDownloadChanges,
  useAppDownloads,
  type AppDownload,
} from "@/lib/appDownloads";
import { cn } from "@/lib/cn";
import { formatBytes } from "@/lib/format";
import { useDismiss } from "@/lib/hooks";
import { useToast } from "./Toast";

/** The reasons the desktop app gives by key (`Said` in apps/desktop/src-tauri/src/said.rs). */
const WHY = [
  "notMedia",
  "notSaved",
  "invalidAddress",
  "noAnswer",
  "namedOtherwise",
  "anotherDevice",
  "notOnServer",
  "signedOut",
  "diskFull",
  "notWritten",
  "cantMake",
  "notMade",
  "notMadeBecause",
  "unreadable",
  "busy",
  "noReply",
  "unreachable",
  "dropped",
  "notSent",
  "stoppedSending",
  "downloadGone",
  "notWhole",
] as const;

/** Why a download failed, in the page's language when the app said it by a key this page knows, else as the app wrote it. */
function whyOf(d: AppDownload, t: TFunction<"components">): string {
  const key = WHY.find((k) => k === d.why?.key);
  if (!key || !d.why) return d.error ?? "";
  const detail = typeof d.why.detail === "string" ? d.why.detail : "";
  return t(`appDownloads.why.${key}`, { detail });
}

/** A ring that fills as the downloads come (and turns while their size is unknown). */
function Ring({ fraction }: { fraction: number | null }) {
  const r = 7;
  const around = 2 * Math.PI * r;
  return (
    <svg viewBox="0 0 18 18" className={cn("size-[18px] -rotate-90", fraction == null && "animate-spin")} aria-hidden>
      <circle cx="9" cy="9" r={r} fill="none" strokeWidth="2" className="stroke-white/15" />
      <circle
        cx="9"
        cy="9"
        r={r}
        fill="none"
        strokeWidth="2"
        strokeLinecap="round"
        strokeDasharray={around}
        strokeDashoffset={around * (1 - (fraction ?? 0.25))}
        className="stroke-accent transition-[stroke-dashoffset] duration-500 motion-reduce:transition-none"
      />
    </svg>
  );
}

function progressLine(d: AppDownload, t: TFunction<"components">): string {
  if (d.state === "preparing") {
    return d.preparing != null
      ? t("appDownloads.preparingPercent", { percent: Math.floor(d.preparing * 100) })
      : t("appDownloads.preparing");
  }
  if (d.state === "waiting") return t("appDownloads.waiting");
  if (d.state === "queued") return t("appDownloads.queued");
  return d.size ? t("appDownloads.progress", { received: formatBytes(d.received), size: formatBytes(d.size) }) : formatBytes(d.received);
}

/** How full a download's bar is: what came of the file, or how far the server has come making it. */
function barOf(d: AppDownload): number {
  if (d.state === "preparing") return d.preparing ?? 0;
  return d.size ? Math.min(1, d.received / d.size) : 0;
}

/**
 * In the desktop app, the files it downloads from this server: in the top
 * bar while they come, with a word when each starts, is done or fails.
 */
export function AppDownloads() {
  const { t } = useTranslation("components");
  const toast = useToast();
  const all = useAppDownloads();
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useDismiss(ref, open, () => setOpen(false));

  const onChange = useCallback(
    (d: AppDownload, before: AppDownload | undefined) => {
      if (d.id === 0) {
        // A file the page made, saved with the system's downloads.
        if (d.state === "done") toast.success(t("appDownloads.saved", { name: d.name }));
        else toast.error(t("appDownloads.failed", { name: d.name, error: whyOf(d, t) }));
        return;
      }
      if (d.state === before?.state) {
        if (d.state === "done") toast.info(t("appDownloads.already", { name: d.name }));
        return;
      }
      if (d.state === "done") toast.success(t("appDownloads.done", { name: d.name }));
      else if (d.state === "failed") toast.error(t("appDownloads.failed", { name: d.name, error: whyOf(d, t) }));
      else if (!before && d.state !== "waiting") toast.info(t("appDownloads.downloading", { name: d.name }));
    },
    [toast, t],
  );
  useAppDownloadChanges(onChange);

  const active = all.filter(coming);
  if (!active.length) return null;
  const fraction = fractionOf(active);
  const percent = fraction == null ? null : t("appDownloads.percent", { percent: Math.floor(fraction * 100) });
  const label =
    active.length === 1 ? t("appDownloads.downloading", { name: active[0]!.name }) : t("appDownloads.downloadingFiles", { count: active.length });

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-label={percent ? `${label}, ${percent}` : label}
        className="flex h-9 items-center gap-2 rounded-full px-3 text-muted transition-colors hover:bg-white/5 hover:text-fg"
      >
        <Ring fraction={fraction} />
        <span className="hidden max-w-48 truncate text-xs font-medium md:inline">{label}</span>
        {percent ? <span className="hidden text-xs tabular-nums md:inline">{percent}</span> : null}
      </button>
      {open ? (
        <div
          aria-label={t("appDownloads.panel")}
          className="absolute right-0 top-full z-40 mt-2 w-80 max-w-[calc(100vw-2rem)] animate-pop-in space-y-3 rounded-lg border border-border bg-surface-2 px-4 py-3 shadow-xl shadow-black/50"
        >
          {active.map((d) => (
            <div key={d.id} className="space-y-1.5">
              <div className="truncate text-sm font-medium text-fg">{d.name}</div>
              <div className="h-1 overflow-hidden rounded-full bg-white/10">
                <div
                  className={cn(
                    "h-full rounded-full transition-[width] duration-500 motion-reduce:transition-none",
                    d.state === "downloading" ? "bg-accent" : "bg-subtle",
                  )}
                  style={{ width: `${barOf(d) * 100}%` }}
                />
              </div>
              <div className="text-xs tabular-nums text-subtle">{progressLine(d, t)}</div>
            </div>
          ))}
          <p className="border-t border-border pt-2.5 text-xs text-muted">
            {/* The account menu's way to the desktop app's server picker, which lists its downloads. */}
            {t("appDownloads.hint", { place: t("appDownloads.place") })}
          </p>
        </div>
      ) : null}
    </div>
  );
}

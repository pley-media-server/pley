import { useId, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { ChevronDown } from "lucide-react";
import type { MediaFile, MediaStream } from "@/api/types";
import { useAuth } from "@/auth";
import { currentLocale, t as translate } from "@/i18n";
import { cn } from "@/lib/cn";
import {
  channelsLabel,
  codecLabel,
  dynamicRangeLabel,
  formatBitrate,
  formatBytes,
  formatDuration,
  resolutionLabel,
} from "@/lib/format";
import { languageName } from "@/lib/languages";

function Row({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid grid-cols-[7.5rem_1fr] gap-3 py-1 text-sm">
      <dt className="text-subtle">{label}</dt>
      <dd className="min-w-0 text-fg/90">{children}</dd>
    </div>
  );
}

function streamDetails(s: MediaStream): string[] {
  const parts: string[] = [];
  if (s.kind === "video") {
    const res = resolutionLabel(s.width, s.height);
    if (res) parts.push(res);
    if (s.width && s.height) parts.push(`${s.width}×${s.height}`);
    if (s.frame_rate) {
      const value = (Math.round(s.frame_rate * 100) / 100).toLocaleString(currentLocale());
      parts.push(translate("mediaInfo.fps", { ns: "item", value }));
    }
    if (s.bit_depth) parts.push(translate("mediaInfo.bitDepth", { ns: "item", depth: s.bit_depth }));
    const range = dynamicRangeLabel(s);
    if (range) parts.push(range);
    if (s.dynamic_range === "dolby_vision" && s.dv_level != null) parts.push(translate("mediaInfo.dvLevel", { ns: "item", level: s.dv_level }));
    if (s.profile) parts.push(s.profile);
  } else if (s.kind === "audio") {
    const ch = channelsLabel(s.channels, s.channel_layout);
    if (ch) parts.push(ch);
    if (s.sample_rate) parts.push(`${kilohertz(s.sample_rate)} kHz`);
  } else {
    parts.push(translate(s.text_based === false ? "mediaInfo.imageBased" : "mediaInfo.text", { ns: "item" }));
    if (s.external) parts.push(translate("mediaInfo.external", { ns: "item" }));
  }
  if (s.bitrate) parts.push(formatBitrate(s.bitrate));
  if (s.forced) parts.push(translate("mediaInfo.forced", { ns: "item" }));
  if (s.default) parts.push(translate("mediaInfo.default", { ns: "item" }));
  return parts;
}

/** 48000 → "48.0" ("48,0" in French). */
function kilohertz(hz: number): string {
  return (hz / 1000).toLocaleString(currentLocale(), { minimumFractionDigits: 1, maximumFractionDigits: 1 });
}

function StreamTable({ streams }: { streams: MediaStream[] }) {
  const { t } = useTranslation("item");
  if (!streams.length) return null;
  return (
    <div className="mt-3 overflow-x-auto">
      <table className="w-full min-w-[520px] text-left text-sm">
        <thead>
          <tr className="text-xs text-subtle">
            <th className="py-1.5 pr-4 font-semibold">{t("mediaInfo.columns.type")}</th>
            <th className="py-1.5 pr-4 font-semibold">{t("mediaInfo.columns.codec")}</th>
            <th className="py-1.5 pr-4 font-semibold">{t("mediaInfo.columns.language")}</th>
            <th className="py-1.5 pr-4 font-semibold">{t("mediaInfo.columns.details")}</th>
          </tr>
        </thead>
        <tbody className="divide-y divide-white/[0.05]">
          {streams.map((s) => (
            <tr key={`${s.kind}-${s.index}`} className="align-top">
              {/* Only the first letter goes up: `capitalize` would make the French "sous-titres" "Sous-Titres". */}
              <td className="py-1.5 pr-4 text-muted first-letter:uppercase">{t(`mediaInfo.kinds.${s.kind}`)}</td>
              <td className="py-1.5 pr-4 font-medium">{codecLabel(s.codec)}</td>
              <td className="py-1.5 pr-4 text-fg/85">
                {languageName(s.language) ?? "—"}
                {s.title ? <span className="block text-xs text-subtle">{s.title}</span> : null}
              </td>
              <td className="py-1.5 pr-4 text-fg/75">{streamDetails(s).join(" · ")}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

/** A button that shows or hides what follows it, closed to begin with. */
function Disclosure({
  label,
  className,
  panelClassName,
  children,
}: {
  label: ReactNode;
  className?: string;
  panelClassName?: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const panelId = useId();
  return (
    <>
      <button
        type="button"
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        aria-controls={panelId}
        className={cn("flex items-center gap-2 text-sm font-semibold transition-colors hover:text-fg", className)}
      >
        <ChevronDown aria-hidden className={cn("size-4 shrink-0 transition-transform", open && "rotate-180")} />
        {label}
      </button>
      <div id={panelId} hidden={!open} className={panelClassName}>
        {open ? children : null}
      </div>
    </>
  );
}

/** A path's last part: "Film (2024).mkv". */
function fileName(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).pop() ?? path;
}

function FileInfo({ file }: { file: MediaFile }) {
  const { t } = useTranslation("item");
  // Where the server keeps it is for its administrators; everyone else sees the file's name, as in the apps.
  const admin = useAuth().is_admin;
  const order: Record<MediaFile["streams"][number]["kind"], number> = { video: 0, audio: 1, subtitle: 2 };
  const streams = [...file.streams].sort((a, b) => order[a.kind] - order[b.kind] || a.index - b.index);
  return (
    <div className="rounded-lg border border-border bg-surface/80 p-4 backdrop-blur">
      <dl>
        <Row label={t("mediaInfo.rows.file")}>
          <span className="break-all font-mono text-xs text-fg/80">{admin ? file.path : fileName(file.path)}</span>
        </Row>
        <Row label={t("mediaInfo.rows.container")}>{file.container?.toUpperCase() ?? "—"}</Row>
        <Row label={t("mediaInfo.rows.size")}>{formatBytes(file.size)}</Row>
        {file.duration_ms ? <Row label={t("mediaInfo.rows.duration")}>{formatDuration(file.duration_ms)}</Row> : null}
        {file.bitrate ? <Row label={t("mediaInfo.rows.bitrate")}>{formatBitrate(file.bitrate)}</Row> : null}
        {file.video_codec ? (
          <Row label={t("mediaInfo.rows.video")}>
            {[
              codecLabel(file.video_codec),
              resolutionLabel(file.width, file.height),
              dynamicRangeLabel(file.streams.find((s) => s.kind === "video")),
            ]
              .filter(Boolean)
              .join(" · ")}
          </Row>
        ) : null}
        {file.audio_codec ? <Row label={t("mediaInfo.rows.audio")}>{codecLabel(file.audio_codec)}</Row> : null}
      </dl>
      <StreamTable streams={streams} />
    </div>
  );
}

/**
 * What the files are (codecs, sizes, streams), hidden until asked for. With
 * several versions, each one opens on its own too.
 */
export function MediaInfo({ files }: { files: MediaFile[] }) {
  const { t } = useTranslation("item");
  if (!files.length) return null;
  const only = files.length === 1 ? files[0] : undefined;
  return (
    <section className="mt-10 max-w-4xl">
      <Disclosure
        className="text-muted"
        panelClassName="mt-3"
        label={
          <>
            {t("mediaInfo.title")}{" "}
            {files.length > 1 ? <span className="text-subtle">{t("mediaInfo.versions", { count: files.length })}</span> : null}
          </>
        }
      >
        {only ? (
          <FileInfo file={only} />
        ) : (
          <ul className="space-y-2">
            {files.map((f, i) => (
              <li key={f.id}>
                <Disclosure className="text-fg/90" panelClassName="mt-2" label={f.edition ?? t("versionNumber", { number: i + 1 })}>
                  <FileInfo file={f} />
                </Disclosure>
              </li>
            ))}
          </ul>
        )}
      </Disclosure>
    </section>
  );
}

import { currentLanguage, say, t, type Said } from "./i18n";
import { displayAddress, normalizeUrl, type SavedServer } from "./servers";

/** A file the app downloaded from a server, or is downloading (src-tauri/src/downloads.rs). */
export interface Download {
  id: number;
  /** The server's origin. */
  server: string;
  name: string;
  size: number | null;
  received: number;
  /** `preparing`: the server is making the file this computer's player plays (a Mac's QuickTime: MP4). */
  state: "queued" | "preparing" | "downloading" | "waiting" | "failed" | "done";
  /** Why it failed, in English. */
  error: string | null;
  /** The same, by key: said in the language shown. */
  why?: Said | null;
  /** How far the server has come making it, 0 to 1. */
  preparing?: number | null;
}

/** `1.5 KB`, `150 MB`, `8.2 GB`: as the web app says sizes. */
export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"] as const;
  let v = bytes;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  const digits = v >= 100 || i === 0 ? 0 : 1;
  const number = new Intl.NumberFormat(currentLanguage(), {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
    useGrouping: false,
  });
  return `${number.format(v)} ${t(`bytes.${units[i] ?? "B"}`)}`;
}

/** `41%`, in the language shown (`41 %` in French). */
function percent(part: number): string {
  return new Intl.NumberFormat(currentLanguage(), { style: "percent", maximumFractionDigits: 0 }).format(
    Math.floor(part * 100) / 100,
  );
}

/** The name of the server a download came from, as saved (else its address). */
export function serverName(origin: string, saved: SavedServer[]): string {
  return saved.find((s) => normalizeUrl(s.url) === origin)?.name ?? displayAddress(origin);
}

/** Whether `d` is still to come. */
export const coming = (d: Download) =>
  d.state === "queued" || d.state === "preparing" || d.state === "downloading" || d.state === "waiting";

/** How far `d` has come, 0 to 1 (`null` before its size is known). */
export const fraction = (d: Download) =>
  d.state === "preparing" ? (d.preparing ?? null) : d.size ? Math.min(1, d.received / d.size) : null;

/** The line under a download's name. */
export function describe(d: Download, from: string): string {
  switch (d.state) {
    case "done":
      return t("downloads.line.sizeFrom", { size: formatBytes(d.size ?? d.received), from });
    case "downloading": {
      const part = fraction(d);
      return part == null
        ? t("downloads.line.sizeFrom", { size: formatBytes(d.received), from })
        : t("downloads.line.progress", {
            percent: percent(part),
            received: formatBytes(d.received),
            size: formatBytes(d.size ?? 0),
          });
    }
    case "queued":
      return t("downloads.line.queued", { from });
    case "preparing":
      return d.preparing != null
        ? t("downloads.line.preparingProgress", { from, percent: percent(d.preparing) })
        : t("downloads.line.preparing", { from });
    case "waiting":
      return t("downloads.line.waiting", { from });
    case "failed":
      return d.why ? say(d.why) : (d.error ?? t("downloads.line.failed"));
  }
}

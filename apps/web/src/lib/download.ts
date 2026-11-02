import type { ItemDetail, MediaFile } from "@/api/types";

/** What file names can't have on Windows, macOS or Linux, and control characters. */
const UNSAFE = /[<>:"/\\|?*\p{Cc}]+/gu;

const pad = (n: number) => String(n).padStart(2, "0");

/** The extension of a file on the server (`mkv`), from its name or else its container. */
export function extensionOf(file: Pick<MediaFile, "path" | "container">): string {
  const base = file.path.split(/[\\/]/).pop() ?? "";
  const dot = base.lastIndexOf(".");
  const ext = dot > 0 ? base.slice(dot + 1) : (file.container?.split(",")[0] ?? "");
  return /^[a-z0-9]{1,5}$/i.test(ext) ? ext.toLowerCase() : "mkv";
}

/**
 * What a download is called: what it is rather than what the file on the
 * server is called (often a release's name): "Vertigo (1958).mkv",
 * "The Expanse - S01E01 - Dulcinea.mkv", "Radiohead - Airbag.flac".
 */
export function downloadName(item: ItemDetail, file: MediaFile): string {
  let name = item.title;
  if (item.kind === "movie") {
    if (item.year) name += ` (${item.year})`;
    if (file.edition) name += ` - ${file.edition}`;
  } else if (item.kind === "episode") {
    const number =
      item.parent_index != null && item.index != null ? `S${pad(item.parent_index)}E${pad(item.index)}` : null;
    name = [item.grandparent_title, number, item.title].filter(Boolean).join(" - ");
  } else if (item.kind === "track" && item.grandparent_title) {
    name = `${item.grandparent_title} - ${item.title}`;
  }
  const safe = name.replace(UNSAFE, " ").replace(/\s+/g, " ").trim().replace(/[. ]+$/, "");
  return `${safe || "Download"}.${extensionOf(file)}`;
}

/**
 * The original file, as it is on the server; with the `item` it is of, for
 * the desktop app, which asks the server for a file its player plays (a
 * Mac's QuickTime opens no Matroska).
 */
export const fileUrl = (file: Pick<MediaFile, "id">, item?: Pick<ItemDetail, "id">) =>
  `/api/files/${file.id}/stream${item ? `?item=${item.id}` : ""}`;

/**
 * Downloads `href` as a file called `name`: the browser keeps it with its
 * downloads (the desktop app with its own, to play when the server is away).
 */
export function saveFile(href: string, name: string): void {
  const link = document.createElement("a");
  link.href = href;
  link.download = name;
  link.hidden = true;
  document.body.append(link);
  link.click();
  link.remove();
}

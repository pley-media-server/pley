import { getStored, writeStorage } from "./hooks";

/** What of a library page's address is its view: the sort, order and filters (not the tab). */
export const VIEW_KEYS = ["sort", "order", "watched", "genre", "year", "resolution", "dynamic_range"] as const;

const isViewKey = (key: string) => (VIEW_KEYS as readonly string[]).includes(key);

/** Where this browser keeps how profile `userId` left library `libraryId`. */
export const viewStorageKey = (userId: number, libraryId: number) => `pley.libraryView.${userId}.${libraryId}`;

/** The view part of a library page's address ("sort=added&watched=false"; "" for the default view). */
export function viewOf(params: URLSearchParams): string {
  const view = new URLSearchParams();
  for (const key of VIEW_KEYS) {
    const value = params.get(key);
    if (value) view.set(key, value);
  }
  return view.toString();
}

/**
 * The address a library page opened at `params` moves to: with the view
 * the library was left in, when the address names none (a library opened
 * from the sidebar). `null` when it stays as it is.
 */
export function withView(params: URLSearchParams, left: string): URLSearchParams | null {
  if (!left || VIEW_KEYS.some((key) => params.has(key))) return null;
  const next = new URLSearchParams(params);
  for (const [key, value] of new URLSearchParams(left)) {
    if (isViewKey(key) && value) next.set(key, value);
  }
  return next.toString() === params.toString() ? null : next;
}

/** How the library at `key` was left ("" if never changed). */
export function leftView(key: string): string {
  const value = getStored<unknown>(key, "");
  return typeof value === "string" ? value : "";
}

/** Remembers `view` (as `viewOf` makes it) for the library at `key`. */
export function keepView(key: string, view: string) {
  writeStorage(key, view);
}

/** A library page's tabs; "library" is the one an address without `tab` shows. */
export type LibraryTab = "recommended" | "library" | "collections";

/**
 * Where this browser keeps the tab profile `userId` was last on in library
 * `libraryId`. Each server is its own site, so its pages keep their own.
 */
export const tabStorageKey = (userId: number, libraryId: number) => `pley.libraryTab.${userId}.${libraryId}`;

/** The tab an address shows. */
export function tabOf(params: URLSearchParams): LibraryTab {
  const tab = params.get("tab");
  return tab === "recommended" || tab === "collections" ? tab : "library";
}

/** The tab the library at `key` was last on; `null` when none (or not one of the tabs) was kept. */
export function leftTab(key: string): LibraryTab | null {
  const value = getStored<unknown>(key, null);
  return value === "recommended" || value === "library" || value === "collections" ? value : null;
}

/** Remembers that the library at `key` is on `tab`. */
export function keepTab(key: string, tab: LibraryTab) {
  writeStorage(key, tab);
}

/**
 * The address a library page opened at `params` moves to: on the tab it was
 * last on, when the address names no tab and no view (a library opened from
 * the sidebar; a sort or filter in it is for the Library tab). `null` when
 * it stays as it is.
 */
export function withTab(params: URLSearchParams, left: LibraryTab | null): URLSearchParams | null {
  if (!left || left === "library" || params.has("tab") || VIEW_KEYS.some((key) => params.has(key))) return null;
  const next = new URLSearchParams(params);
  next.set("tab", left);
  return next;
}

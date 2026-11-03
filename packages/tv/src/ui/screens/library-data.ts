import type { Api } from "../../api/client";
import type { ItemSummary, LibraryFilters, LibraryKind, LibrarySort } from "../../api/types";
import type { KeyValue } from "../../auth/tokens";
import { Cancel } from "../../net/transport";
import type { LibraryTab } from "../shell/navigator";

/** The resolutions a library can be filtered by, as the server names them, best first. */
export type Resolution = "4k" | "1080" | "720" | "sd";
/** The dynamic ranges a library can be filtered by, as the server names them: `hdr` is any but SDR. */
export type DynamicRangeFilter = "hdr" | "dolby_vision" | "hdr10plus" | "hdr10" | "hlg" | "sdr";

const RESOLUTIONS: Resolution[] = ["4k", "1080", "720", "sd"];
const RESOLUTION_LABELS: string[] = ["4K", "1080p", "720p", "SD"];
const DYNAMIC_RANGES: DynamicRangeFilter[] = ["hdr", "dolby_vision", "hdr10plus", "hdr10", "hlg", "sdr"];
// HDR10 and HDR10+ read as plain "HDR" and "HDR+", as on the posters.
const DYNAMIC_RANGE_LABELS: string[] = ["Any HDR", "Dolby Vision", "HDR+", "HDR", "HLG", "SDR"];

export function isResolution(v: unknown): v is Resolution {
  return RESOLUTIONS.indexOf(v as Resolution) >= 0;
}

export function isDynamicRange(v: unknown): v is DynamicRangeFilter {
  return DYNAMIC_RANGES.indexOf(v as DynamicRangeFilter) >= 0;
}

/** A resolution's words ("4K", "1080p"); null for a value that isn't one. */
export function resolutionLabel(v: string): string | null {
  return isResolution(v) ? RESOLUTION_LABELS[RESOLUTIONS.indexOf(v)]! : null;
}

/** A dynamic range's words ("Dolby Vision", "HDR+"); null for a value that isn't one. */
export function dynamicRangeLabel(v: string): string | null {
  return isDynamicRange(v) ? DYNAMIC_RANGE_LABELS[DYNAMIC_RANGES.indexOf(v)]! : null;
}

/**
 * The resolutions and dynamic ranges the server offers for a library, in
 * its order (best first), those this app has words for; none from a server
 * from before these filters.
 */
export function qualityChoices(f: LibraryFilters | null | undefined): { resolutions: Resolution[]; dynamicRanges: DynamicRangeFilter[] } {
  return {
    resolutions: (f?.resolutions ?? []).filter(isResolution),
    dynamicRanges: (f?.dynamic_ranges ?? []).filter(isDynamicRange),
  };
}

/**
 * A filter's list with the value chosen in it: one the server no longer
 * offers (kept from before, the library changed) comes last, so it shows
 * checked and focused and can be changed, as on the web and Roku.
 */
export function withChosen<T extends string>(offered: T[], chosen: T | null): T[] {
  return chosen && offered.indexOf(chosen) < 0 ? offered.concat([chosen]) : offered;
}

/** How a library is listed: sort, order and filters (the web's query string). */
export interface LibraryParams {
  sort: LibrarySort;
  order: "asc" | "desc";
  /** false: unwatched only; true: watched only; null: all. */
  watched: boolean | null;
  genre: string | null;
  year: number | null;
  resolution: Resolution | null;
  dynamicRange: DynamicRangeFilter | null;
}

export const SORT_LABELS: Record<LibrarySort, string> = {
  title: "Title",
  year: "Year",
  added: "Date added",
  rating: "Rating",
  user_rating: "Your rating",
  release: "Release date",
  duration: "Duration",
  last_played: "Last played",
  bitrate: "Bitrate",
};

/** The sorts each kind of library offers, in order. */
export function sortsFor(kind: LibraryKind): LibrarySort[] {
  if (kind === "music") return ["title", "added", "user_rating", "last_played"];
  const all: LibrarySort[] = ["title", "year", "added", "rating", "user_rating", "release", "duration", "last_played", "bitrate"];
  return kind === "shows" ? all.filter((s) => s !== "duration") : all;
}

/** A sort's own order: A to Z for titles, newest or highest first for the rest. */
export function defaultOrder(sort: LibrarySort): "asc" | "desc" {
  return sort === "title" ? "asc" : "desc";
}

export function defaultParams(): LibraryParams {
  return { sort: "title", order: "asc", watched: null, genre: null, year: null, resolution: null, dynamicRange: null };
}

export function hasFilter(p: LibraryParams): boolean {
  return p.watched !== null || p.genre !== null || p.year !== null || p.resolution !== null || p.dynamicRange !== null;
}

/** Without any filter, in the same order: Clear filters. */
export function cleared(p: LibraryParams): LibraryParams {
  return { ...p, watched: null, genre: null, year: null, resolution: null, dynamicRange: null };
}

/**
 * Whether the grid's posters carry their quality badges ("4K", "DV"):
 * only when the files' quality is what the listing is about, sorted by
 * bitrate or filtered by resolution or dynamic range.
 */
export function showsBadges(p: LibraryParams): boolean {
  return p.sort === "bitrate" || p.resolution !== null || p.dynamicRange !== null;
}

/** Plex's name for each sort, as smart filters write it (docs/COMPATIBILITY.md). */
const PLEX_SORT: Record<LibrarySort, string> = {
  title: "titleSort",
  year: "year",
  added: "addedAt",
  rating: "rating",
  user_rating: "userRating",
  release: "originallyAvailableAt",
  duration: "duration",
  last_played: "lastViewedAt",
  bitrate: "mediaBitrate",
};

/**
 * The library page's filters and order as a smart filter, in Plex's syntax
 * (`genre=Comedy&resolution=4k&unwatched=1&sort=year:desc`): what "Save
 * as…" saves (the web's smartFilterOf, the Android app's SmartFilter.of).
 * The resolution is Plex's field, the dynamic range Pley's own.
 */
export function smartFilterOf(p: LibraryParams): string {
  const parts: [string, string][] = [];
  if (p.genre) parts.push(["genre", p.genre]);
  if (p.year !== null) parts.push(["year", String(p.year)]);
  if (p.resolution) parts.push(["resolution", p.resolution]);
  if (p.dynamicRange) parts.push(["dynamicRange", p.dynamicRange]);
  if (p.watched !== null) parts.push(["unwatched", p.watched ? "0" : "1"]);
  const field = PLEX_SORT[p.sort];
  parts.push(["sort", p.order === "desc" ? `${field}:desc` : field]);
  return parts.map(([k, v]) => `${encodeForm(k)}=${encodeForm(v)}`).join("&");
}

/** A form value as URLSearchParams and Java's URLEncoder write it: spaces as "+". */
function encodeForm(s: string): string {
  return encodeURIComponent(s).replace(/%20/g, "+");
}

function decodeForm(s: string): string {
  try {
    return decodeURIComponent(s.replace(/\+/g, " "));
  } catch {
    return s;
  }
}

const SORT_NOTES: Record<string, [string, string]> = {
  titleSort: ["By title", "By title, Z to A"],
  year: ["Oldest first", "Newest first"],
  originallyAvailableAt: ["Earliest release first", "Latest release first"],
  addedAt: ["First added first", "Recently added first"],
  rating: ["Lowest rated first", "Best rated first"],
  audienceRating: ["Lowest rated first", "Best rated first"],
  userRating: ["Your lowest rated first", "Your best rated first"],
  lastViewedAt: ["Least recently played first", "Recently played first"],
  duration: ["Shortest first", "Longest first"],
  viewCount: ["Least played first", "Most played first"],
};

function sortNote(value: string): string {
  const first = value.split(",")[0] ?? "";
  const at = first.indexOf(":");
  const field = at < 0 ? first : first.slice(0, at);
  const direction = at < 0 ? null : first.slice(at + 1);
  // python-plexapi may name the kind first: `movie.year`.
  const name = field.indexOf(".") >= 0 ? field.slice(field.indexOf(".") + 1) : field;
  const notes = SORT_NOTES[name];
  if (!notes) return `Sorted by ${name}`;
  return direction === "desc" ? notes[1] : notes[0];
}

/** What a smart filter finds, in words: "Genre: Comedy", "Unwatched", "Newest first"… */
export function describeSmartFilter(filter: string): string[] {
  const out: string[] = [];
  for (const part of filter.split("&")) {
    if (!part) continue;
    const at = part.indexOf("=");
    const key = decodeForm(at < 0 ? part : part.slice(0, at));
    const value = at < 0 ? "" : decodeForm(part.slice(at + 1));
    switch (key) {
      case "genre":
        out.push(`Genre: ${value}`);
        break;
      case "label":
        out.push(`Label: ${value}`);
        break;
      case "studio":
        out.push(`Studio: ${value}`);
        break;
      case "contentRating":
        out.push(`Rated ${value}`);
        break;
      case "unwatched":
        out.push(value === "0" || value === "false" ? "Watched" : "Unwatched");
        break;
      case "inProgress":
        out.push(value === "0" || value === "false" ? "Not in progress" : "In progress");
        break;
      case "year":
        out.push(`From ${value}`);
        break;
      case "year>>":
        out.push(`After ${value}`);
        break;
      case "year<<":
        out.push(`Before ${value}`);
        break;
      case "decade":
        out.push(`The ${value}s`);
        break;
      case "resolution":
        out.push(resolutionLabel(value) ?? `${key}: ${value}`);
        break;
      case "dynamicRange":
        out.push(dynamicRangeLabel(value) ?? `${key}: ${value}`);
        break;
      case "sort":
        out.push(sortNote(value));
        break;
      case "limit":
        out.push(`At most ${value}`);
        break;
      // Plumbing, not part of what is found.
      case "type":
      case "includeGuids":
        break;
      default:
        out.push(`${key}: ${value}`);
    }
  }
  return out;
}

/**
 * Params as kept, made fit for the library: music has no filters; a sort
 * the kind doesn't offer is Title; a resolution or dynamic range that
 * isn't one of the server's is none.
 */
export function fitParams(p: Partial<LibraryParams> | null, kind: LibraryKind): LibraryParams {
  const d = defaultParams();
  const sort = p?.sort && sortsFor(kind).indexOf(p.sort) >= 0 ? p.sort : d.sort;
  const order = p?.order === "asc" || p?.order === "desc" ? p.order : defaultOrder(sort);
  if (kind === "music") return { ...d, sort, order };
  return {
    sort,
    order,
    watched: typeof p?.watched === "boolean" ? p.watched : null,
    genre: typeof p?.genre === "string" && p.genre ? p.genre : null,
    year: typeof p?.year === "number" ? p.year : null,
    resolution: isResolution(p?.resolution) ? p!.resolution : null,
    dynamicRange: isDynamicRange(p?.dynamicRange) ? p!.dynamicRange : null,
  };
}

/** Each library's sort and filters, kept on the TV so it opens as it was left (per server). */
export class ParamsStore {
  private readonly store: KeyValue;
  private readonly server: string;
  private readonly profile: number | null;
  /** One view per server, profile and library; without a profile nothing is read or kept. */
  constructor(store: KeyValue, server: string, profile: number | null) {
    this.store = store;
    this.server = server;
    this.profile = profile;
  }
  private key(id: number): string | null {
    return this.profile === null ? null : `pley.library.${this.server}.${this.profile}.${id}`;
  }
  get(id: number, kind: LibraryKind): LibraryParams {
    const key = this.key(id);
    let saved: Partial<LibraryParams> | null = null;
    try {
      const text = key === null ? null : this.store.get(key);
      saved = text ? (JSON.parse(text) as Partial<LibraryParams>) : null;
    } catch {
      saved = null;
    }
    return fitParams(saved, kind);
  }
  set(id: number, p: LibraryParams): void {
    const key = this.key(id);
    if (key !== null) this.store.set(key, JSON.stringify(p));
  }
}

/**
 * The tab each library was last on (Recommended, Library or Collections),
 * so it opens on it again, after a restart too: one per server, profile and
 * library, kept on the TV. Without a profile nothing is read or kept.
 */
export class TabStore {
  private readonly store: KeyValue;
  private readonly server: string;
  private readonly profile: number | null;
  constructor(store: KeyValue, server: string, profile: number | null) {
    this.store = store;
    this.server = server;
    this.profile = profile;
  }
  private key(id: number): string | null {
    return this.profile === null ? null : `pley.libraryTab.${this.server}.${this.profile}.${id}`;
  }
  /** The tab library `id` was last on; `null` when none (or not one of the tabs) was kept. */
  get(id: number): LibraryTab | null {
    const key = this.key(id);
    let value: string | null = null;
    try {
      value = key === null ? null : this.store.get(key);
    } catch {
      value = null;
    }
    return value === "recommended" || value === "library" || value === "collections" ? value : null;
  }
  set(id: number, tab: LibraryTab): void {
    const key = this.key(id);
    if (key === null) return;
    try {
      if (this.store.get(key) !== tab) this.store.set(key, tab);
    } catch {
      // Storage full or unavailable: the library opens on Library next time.
    }
  }
}

/** "1,500 movies", "1 show", "12 artists"; of a filtered listing, "23 of 1,500 movies". */
export function countLabel(n: number, kind: LibraryKind, of: number | null = null): string {
  const m = of ?? n;
  const noun = kind === "movies" ? (m === 1 ? "movie" : "movies") : kind === "shows" ? (m === 1 ? "show" : "shows") : m === 1 ? "artist" : "artists";
  return of === null ? `${grouped(n)} ${noun}` : `${grouped(n)} of ${grouped(of)} ${noun}`;
}

/** 1500 → "1,500". */
export function grouped(n: number): string {
  const s = String(Math.round(n));
  return s.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
}

// ---- the libraries' own order

/** `list` in the order of `ids`; what they don't name comes after, as it came. */
export function inOrder<T extends { id: number }>(list: T[], ids: number[] | null): T[] {
  if (!ids) return list;
  const at = new Map<number, number>();
  ids.forEach((id, i) => at.set(id, i));
  const rank = new Map<T, number>();
  list.forEach((x, i) => rank.set(x, at.has(x.id) ? at.get(x.id)! : ids.length + i));
  return list.slice().sort((a, b) => rank.get(a)! - rank.get(b)!);
}

/** `ids` with the one at `from` moved to `to`. */
export function moved(ids: number[], from: number, to: number): number[] {
  const next = ids.slice();
  next.splice(to, 0, next.splice(from, 1)[0]!);
  return next;
}

export function sameOrder(a: number[], b: number[]): boolean {
  return a.length === b.length && a.every((id, i) => id === b[i]);
}

/**
 * Where a card at `at` of `count` lands moved `by` places through a grid
 * of `columns`: a place along (on to the next row at a row's end), or a
 * row up or down; down onto a shorter last row, at its end. -1 when it
 * can't go that way.
 */
export function gridStep(at: number, by: number, count: number, columns: number): number {
  const to = at + by;
  if (to < 0) return -1;
  if (to < count) return to;
  return by > 1 && Math.floor(at / columns) < Math.floor((count - 1) / columns) ? count - 1 : -1;
}

// ---- A–Z

export const LETTERS =["#", "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z"];

/** An item's letter: the first of its sort title, accents removed; "#" for anything that isn't A–Z. */
export function letterOf(item: Pick<ItemSummary, "sort_title" | "title">): string {
  const source = (item.sort_title || item.title || "").trim();
  let c = source.charAt(0);
  try {
    c = c.normalize("NFD").charAt(0);
  } catch {
    // Old engines without normalize: the character as it is.
  }
  c = c.toUpperCase();
  return c >= "A" && c <= "Z" ? c : "#";
}

/** The first position of each letter in a listing, and the letter at any position. */
export class AlphaIndex {
  readonly firsts = new Map<string, number>();
  private readonly runs: { letter: string; start: number }[] = [];

  constructor(items: Pick<ItemSummary, "sort_title" | "title">[]) {
    let last: string | null = null;
    items.forEach((it, i) => {
      const l = letterOf(it);
      if (!this.firsts.has(l)) this.firsts.set(l, i);
      if (l !== last) {
        this.runs.push({ letter: l, start: i });
        last = l;
      }
    });
  }

  /** The letter of the run `position` is in. */
  letterAt(position: number): string {
    let lo = 0;
    let hi = this.runs.length - 1;
    let found = this.runs[0]?.letter ?? "#";
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (this.runs[mid]!.start <= position) {
        found = this.runs[mid]!.letter;
        lo = mid + 1;
      } else hi = mid - 1;
    }
    return found;
  }

  has(letter: string): boolean {
    return this.firsts.has(letter);
  }

  /** Where a letter jumps to: its first title, else the next letter's, else the nearest before it, else 0. */
  jumpTarget(letter: string): number {
    const at = this.firsts.get(letter);
    if (at !== undefined) return at;
    const i = LETTERS.indexOf(letter);
    for (let j = i + 1; j < LETTERS.length; j++) {
      const p = this.firsts.get(LETTERS[j]!);
      if (p !== undefined) return p;
    }
    for (let j = i - 1; j >= 0; j--) {
      const p = this.firsts.get(LETTERS[j]!);
      if (p !== undefined) return p;
    }
    return 0;
  }
}

/** The A–Z shows for title sorts of a listing held whole, of more than 30 titles. */
export function showsJump(p: LibraryParams, total: number | null, windowed: boolean): boolean {
  return p.sort === "title" && !windowed && total !== null && total > 30;
}

// ---- the listing

export const HEAD = 200;
export const PAGE = 1000;
/** Pages kept around the visible part of a big library. */
export const KEPT_PAGES = 4;

/**
 * A library's listing (the Android app's LibraryListing): the first 200
 * at once, then the rest in pages of 1000, two at a time; a library above
 * `windowAbove` keeps only the pages around what shows.
 */
export class Listing {
  total: number | null = null;
  error: unknown = null;
  readonly windowAbove: number;
  private readonly items: (ItemSummary | undefined)[] = [];
  private readonly pages = new Map<number, "loading" | "done">();
  private readonly order: number[] = [];
  private readonly cancel = new Cancel();
  private readonly api: Api;
  private readonly id: number;
  private readonly params: LibraryParams;
  private readonly onChange: () => void;

  constructor(api: Api, id: number, params: LibraryParams, windowAbove: number, onChange: () => void) {
    this.api = api;
    this.id = id;
    this.params = params;
    this.windowAbove = windowAbove;
    this.onChange = onChange;
  }

  get windowed(): boolean {
    return this.total !== null && this.total > this.windowAbove;
  }

  at(i: number): ItemSummary | undefined {
    return this.items[i];
  }

  /** Every item, when the whole listing is held; `null` otherwise. */
  all(): ItemSummary[] | null {
    if (this.total === null) return null;
    for (let i = 0; i < this.total; i++) if (!this.items[i]) return null;
    return this.items.slice(0, this.total) as ItemSummary[];
  }

  stop(): void {
    this.cancel.cancel();
  }

  private query(offset: number, limit: number) {
    const p = this.params;
    // In this order: the server's listing ETag hashes the query as written.
    return {
      sort: p.sort,
      order: p.order,
      watched: p.watched === null ? undefined : p.watched,
      genre: p.genre ?? undefined,
      year: p.year ?? undefined,
      resolution: p.resolution ?? undefined,
      dynamic_range: p.dynamicRange ?? undefined,
      offset,
      limit,
    };
  }

  private put(offset: number, list: ItemSummary[]): void {
    for (let i = 0; i < list.length; i++) this.items[offset + i] = list[i];
  }

  async start(): Promise<void> {
    try {
      const head = await this.api.libraryItems(this.id, this.query(0, HEAD), this.cancel);
      this.total = head.total;
      this.put(0, head.items);
      this.onChange();
      if (!this.windowed) await this.rest(HEAD);
      else this.showing(0);
    } catch (e) {
      if (this.cancel.cancelled) return;
      this.error = e;
      this.onChange();
    }
  }

  /** Everything after the head, two pages at a time; a short page ends it. */
  private async rest(from: number): Promise<void> {
    let offset = from;
    while (this.total !== null && offset < this.total && !this.cancel.cancelled) {
      const batch = [offset, offset + PAGE].filter((o) => o < (this.total ?? 0));
      const answers = await Promise.all(batch.map((o) => this.api.libraryItems(this.id, this.query(o, PAGE), this.cancel)));
      let short = false;
      answers.forEach((a, i) => {
        this.put(batch[i]!, a.items);
        if (a.items.length < PAGE) short = true;
      });
      this.onChange();
      if (short) return;
      offset += PAGE * 2;
    }
  }

  /** A big library: the pages around the first visible position, fetched when missing. */
  showing(first: number): void {
    if (!this.windowed || this.total === null) return;
    const page = Math.floor(first / PAGE);
    const start = first % PAGE < PAGE / 2 ? Math.max(0, page - 1) : page;
    const last = Math.floor((this.total - 1) / PAGE);
    for (const p of [start, start + 1]) {
      if (p > last || this.pages.has(p)) continue;
      this.pages.set(p, "loading");
      this.api.libraryItems(this.id, this.query(p * PAGE, PAGE), this.cancel).then(
        (a) => {
          this.pages.set(p, "done");
          this.put(p * PAGE, a.items);
          this.keep(p);
          this.onChange();
        },
        () => this.pages.delete(p),
      );
    }
  }

  /** At most four pages stay; the oldest others go (the head stays too). */
  private keep(p: number): void {
    const at = this.order.indexOf(p);
    if (at >= 0) this.order.splice(at, 1);
    this.order.push(p);
    while (this.order.length > KEPT_PAGES) {
      const old = this.order.shift()!;
      this.pages.delete(old);
      for (let i = Math.max(old * PAGE, HEAD); i < (old + 1) * PAGE; i++) this.items[i] = undefined;
    }
  }
}

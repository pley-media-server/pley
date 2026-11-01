import type { LiveChannel, LiveGroup, LiveGuide, LiveProgram } from "../api/types";
import { formatTimeOfDay } from "../format/format";

/**
 * Live TV's rules that draw nothing (docs/API.md, "Live TV"): what's on,
 * the filters, changing channels, and the guide's chunks and cells. The
 * web app's (apps/web/src/pages/livetv) on a TV.
 */

/** The guide's columns: half an hour each. */
export const SLOT = 1800;
/** The guide is asked for in chunks of six hours, as it comes into view. */
export const GUIDE_CHUNK = 6 * 3600;
/** How far ahead the guide goes from where it opens. */
export const GUIDE_SPAN = 48 * 3600;
/** A gap in a channel's guide shorter than this belongs to the programme before it. */
const MIN_GAP = 60;

/** Unix seconds now. */
export function nowSeconds(): number {
  return Date.now() / 1000;
}

export function airing(p: Pick<LiveProgram, "start" | "stop">, now: number): boolean {
  return p.start <= now && p.stop > now;
}

/**
 * What's on a channel at `now`, and what comes next. The channel list is
 * asked for again every minute, so its `now` may have just ended: its
 * `next` is on then.
 */
export function nowAndNext(c: Pick<LiveChannel, "now" | "next">, now: number): { on: LiveProgram | null; next: LiveProgram | null } {
  if (c.now && airing(c.now, now)) return { on: c.now, next: c.next };
  if (c.next && airing(c.next, now)) return { on: c.next, next: null };
  const next = [c.now, c.next].filter((p): p is LiveProgram => !!p && p.start > now)[0] ?? null;
  return { on: null, next };
}

/** How far a programme is at `now`, 0–1. */
export function programProgress(p: Pick<LiveProgram, "start" | "stop">, now: number): number {
  return Math.max(0, Math.min(1, (now - p.start) / Math.max(1, p.stop - p.start)));
}

/** "12 min left", "1 h 5 min left". */
export function timeLeft(p: Pick<LiveProgram, "stop">, now: number): string {
  const minutes = Math.max(1, Math.round((p.stop - now) / 60));
  if (minutes < 60) return `${minutes} min left`;
  const h = Math.floor(minutes / 60);
  const m = minutes % 60;
  return m ? `${h} h ${m} min left` : `${h} h left`;
}

/** "20:00" or "8:00 PM", as the TV writes the time. */
export function formatTime(ts: number, hours24: boolean): string {
  return formatTimeOfDay(new Date(ts * 1000), hours24);
}

/** "20:00 – 21:00". */
export function formatSpan(p: Pick<LiveProgram, "start" | "stop">, hours24: boolean): string {
  return `${formatTime(p.start, hours24)} – ${formatTime(p.stop, hours24)}`;
}

const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/** "Today", "Tomorrow", "Yesterday", or "Fri 30 Oct". */
export function formatGuideDay(ts: number, now: number): string {
  // Dates apart, not 24-hour steps: a day is 23 or 25 hours when daylight saving starts or ends.
  const date = (t: number) => {
    const d = new Date(t * 1000);
    return Date.UTC(d.getFullYear(), d.getMonth(), d.getDate()) / 86_400_000;
  };
  const apart = date(ts) - date(now);
  if (apart === 0) return "Today";
  if (apart === 1) return "Tomorrow";
  if (apart === -1) return "Yesterday";
  const d = new Date(ts * 1000);
  return `${DAYS[d.getDay()]} ${d.getDate()} ${MONTHS[d.getMonth()]}`;
}

/** "S2 E14", when the guide numbers the episode. */
export function episodeText(p: Pick<LiveProgram, "season" | "episode">): string | null {
  if (p.season != null && p.episode != null) return `S${p.season} E${p.episode}`;
  if (p.episode != null) return `E${p.episode}`;
  return null;
}

/** "101 Pley News HD", or the name alone. */
export function channelTitle(c: Pick<LiveChannel, "number" | "name">): string {
  return c.number ? `${c.number} ${c.name}` : c.name;
}

// ---- categories

/**
 * Which channels a list has: the marked ones, every one, a group's (the
 * web's LiveCategory), or every channel whose name, number or what's on
 * has `q` in it (the one search, whatever category was open).
 */
export type LiveCategory = { kind: "all" } | { kind: "favorites" } | { kind: "recent" } | { kind: "group"; group: string } | { kind: "search"; q: string };

/** Channels asked for at once. */
export const CHANNEL_PAGE = 100;

/** "all", "favorites", "recent", "group:<name>" or "search:<words>": kept on the TV and in a page's key. */
export function categoryKey(c: LiveCategory): string {
  return c.kind === "group" ? `group:${c.group}` : c.kind === "search" ? `search:${c.q}` : c.kind;
}

export function parseCategory(key: string | null | undefined): LiveCategory {
  if (key === "favorites" || key === "recent") return { kind: key };
  if (key && key.indexOf("group:") === 0) return { kind: "group", group: key.slice(6) };
  if (key && key.indexOf("search:") === 0 && key.length > 7) return { kind: "search", q: key.slice(7) };
  return { kind: "all" };
}

/** What `browse` is asked for a category's channels. */
export function browseQuery(c: LiveCategory): { category: "all" | "favorites" | "recent" | "group"; group?: string; q?: string } {
  if (c.kind === "search") return { category: "all", q: c.q };
  if (c.kind === "group") return { category: "group", group: c.group };
  return { category: c.kind };
}

/** A region's groups, gathered. */
export interface Region {
  region: string;
  groups: LiveGroup[];
  count: number;
}

/**
 * The groups as the categories list them (the web's groupSections): a
 * provider naming hundreds of groups by region ("UK| Sport", "UK|
 * News"…) has them gathered under each region, the regions in
 * alphabetical order, and the rest after, in its own order; a region with
 * one group is one of the rest. With fewer than two regions, the groups as
 * the provider lists them.
 */
export function groupSections(groups: LiveGroup[]): { regions: Region[]; others: LiveGroup[] } {
  const byRegion: Record<string, LiveGroup[]> = {};
  const order: string[] = [];
  for (const g of groups) {
    if (!g.region) continue;
    if (!byRegion[g.region]) {
      byRegion[g.region] = [];
      order.push(g.region);
    }
    byRegion[g.region]!.push(g);
  }
  const regions = order
    .filter((r) => byRegion[r]!.length > 1)
    .map((r) => ({ region: r, groups: byRegion[r]!, count: byRegion[r]!.reduce((n, g) => n + g.count, 0) }))
    .sort((a, b) => a.region.localeCompare(b.region));
  if (regions.length < 2) return { regions: [], others: groups };
  const gathered: Record<string, true> = {};
  for (const r of regions) gathered[r.region] = true;
  return { regions, others: groups.filter((g) => !g.region || !gathered[g.region]) };
}

/** The groups whose names have `q` in them, for a search's quick links. */
export function groupsMatching(groups: LiveGroup[], q: string, max: number): LiveGroup[] {
  const s = q.trim().toLowerCase();
  if (!s) return [];
  return groups.filter((g) => g.name.toLowerCase().indexOf(s) >= 0).slice(0, max);
}

/** What a category is called: "Favourites", "UK · Sport", "Results for “bbc”". */
export function categoryName(c: LiveCategory, groups: LiveGroup[] | null | undefined): string {
  switch (c.kind) {
    case "all":
      return "All channels";
    case "favorites":
      return "Favourites";
    case "recent":
      return "Recently watched";
    case "search":
      return `Results for “${c.q}”`;
    default: {
      const g = (groups ?? []).filter((x) => x.name === c.group)[0];
      return g?.region ? `${g.region} · ${g.label}` : g?.label ?? c.group;
    }
  }
}

/**
 * A name's region and the rest, when it begins with one the way providers
 * write them ("UK| Sport", "[UK] Sport", "UK: Sport"), as the server splits
 * group names.
 */
export function splitRegion(name: string): [string, string] | null {
  const n = name.trim().replace(/^\|+\s*/, "");
  const m = /^\[([^\]]{1,16})\]\s*(.+)$/.exec(n) ?? /^([^|]{1,16})\|\s*(.+)$/.exec(n) ?? /^([A-Z]{1,4}):\s*(.+)$/.exec(n);
  if (!m) return null;
  const rest = m[2]!.replace(/^[|:\-\s]+/, "").trim();
  const region = m[1]!.trim();
  return region && rest ? [region, rest] : null;
}

/**
 * A channel's name as a list shows it: in a group, without the region its
 * group already says ("Sky Sports 1" in UK · Sport, not "UK| Sky Sports 1").
 */
export function channelName(c: Pick<LiveChannel, "name" | "group">, category?: LiveCategory): string {
  if (!category || category.kind !== "group") return c.name;
  const name = splitRegion(c.name);
  const group = splitRegion(c.group);
  return name && group && name[0].toLowerCase() === group[0].toLowerCase() ? name[1] : c.name;
}

// ---- changing channels

/** The channel numbered `digits` ("7" finds "007" too); `null` when none is. */
export function channelByNumber(list: LiveChannel[], digits: string): LiveChannel | null {
  const exact = list.filter((c) => c.number === digits)[0];
  if (exact) return exact;
  const n = Number(digits);
  if (!isFinite(n)) return null;
  return list.filter((c) => c.number !== null && /^\d+$/.test(c.number) && Number(c.number) === n)[0] ?? null;
}

// ---- the guide

/** The starts of the chunks covering `from`–`to`, counted from `origin` and kept within the guide's span. */
export function chunkStarts(from: number, to: number, origin: number): number[] {
  const first = Math.max(0, Math.floor((from - origin) / GUIDE_CHUNK));
  const last = Math.min(GUIDE_SPAN / GUIDE_CHUNK - 1, Math.floor((Math.max(from, to - 1) - origin) / GUIDE_CHUNK));
  const out: number[] = [];
  for (let i = first; i <= last; i++) out.push(origin + i * GUIDE_CHUNK);
  return out;
}

/** A chunk's programmes added to what was had, each once (by its start), in order. */
export function mergeGuide(into: Map<number, LiveProgram[]>, guide: LiveGuide): void {
  for (const key in guide.programs) {
    const id = Number(key);
    const known = into.get(id) ?? [];
    const starts: Record<number, true> = {};
    for (const p of known) starts[p.start] = true;
    const added = (guide.programs[key] ?? []).filter((p) => !starts[p.start]);
    if (!added.length) {
      into.set(id, known);
      continue;
    }
    into.set(id, known.concat(added).sort((a, b) => a.start - b.start));
  }
}

/** One cell of a guide row: a programme, or a stretch the guide says nothing of (`program` null). */
export interface GuideCell {
  key: string;
  start: number;
  stop: number;
  program: LiveProgram | null;
}

/**
 * A channel's row between `from` and `to`: its programmes there, with what
 * the guide says nothing of between them as cells of their own, so the
 * remote moves through time without jumping over a hole. A channel without
 * a guide is one cell.
 */
export function guideCells(programs: LiveProgram[] | undefined, from: number, to: number): GuideCell[] {
  const out: GuideCell[] = [];
  let at = from;
  for (const p of programs ?? []) {
    if (p.stop <= from || p.stop <= at) continue;
    if (p.start >= to) break;
    if (p.start - at >= MIN_GAP) out.push({ key: `gap/${at}`, start: at, stop: p.start, program: null });
    const start = Math.max(p.start, at);
    out.push({ key: String(p.start), start, stop: p.stop, program: p });
    at = p.stop;
  }
  if (to - at >= MIN_GAP || !out.length) out.push({ key: `gap/${at}`, start: at, stop: Math.max(to, at + MIN_GAP), program: null });
  return out;
}

/** The cell at time `t` (the last one when it is past them all). */
export function cellAt(cells: GuideCell[], t: number): number {
  for (let i = 0; i < cells.length; i++) if (cells[i]!.stop > t) return i;
  return cells.length - 1;
}


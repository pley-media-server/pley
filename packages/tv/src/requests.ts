import { ApiError, errorMessage, unreachable, type RequestQuery } from "./api/client";
import type {
  DiscoverDetail,
  DiscoverList,
  DiscoverSeason,
  DiscoverTitle,
  MediaRequest,
  RequestAction,
  RequestList,
  RequestStatus,
  RequestSummary,
  TitleKind,
  TitleStatus,
} from "./api/types";
import { dayAndTime, formatRuntime, relativeTime, shortDate, timeUntil } from "./format/format";
import { t } from "./i18n";
import type { PageKey } from "./ui/shell/navigator";

/*
 * Requests and Discover in words and rules, as every client has them
 * (docs/API.md, "Requests"): who sees what, a title's badge and its one
 * main action, the season picker, a request's stage and actions, the
 * allowance, and what each of the server's refusals says. A platform may
 * shorten these words, never reword them.
 */

/** Discover's rows, in order (their titles: {@link discoverRowTitle}); `mixed` rows hold movies and shows both, so their tiles say which. */
export const DISCOVER_ROWS: { list: DiscoverList; mixed: boolean }[] = [
  { list: "trending", mixed: true },
  { list: "movies", mixed: false },
  { list: "shows", mixed: false },
  { list: "upcoming", mixed: false },
  { list: "anime", mixed: false },
];

/** A row of Discover's title: "Trending this week". */
export function discoverRowTitle(list: DiscoverList): string {
  return t(`requests.discover.lists.${list}`);
}

/** The status tabs of a list of requests, after All. */
export const STATUS_TABS: RequestStatus[] = ["pending", "approved", "available", "declined", "failed"];

/** A tab of a list of requests: Needs attention (managers), All, or one status. */
export type RequestsTab = "attention" | "all" | RequestStatus;

/** The tabs, Needs attention first for managers (they open on it). */
export function requestsTabs(manage: boolean): { tab: RequestsTab; label: string }[] {
  const tabs: { tab: RequestsTab; label: string }[] = [{ tab: "all", label: t("requests.page.all") }];
  for (const status of STATUS_TABS) tabs.push({ tab: status, label: t(`requests.page.statuses.${status}`) });
  const attention: { tab: RequestsTab; label: string }[] = [{ tab: "attention", label: t("requests.page.attention") }];
  return manage ? attention.concat(tabs) : tabs;
}

/** What a tab with nothing in it says (`everyone`: a manager's list of everyone's). */
export function requestsEmpty(tab: RequestsTab, everyone: boolean): string {
  if (tab === "attention") return t("requests.page.empty.attention");
  const status = tab === "all" ? "none" : tab;
  return everyone ? t(`requests.page.empty.everyone.${status}`) : t(`requests.page.empty.yours.${status}`);
}

/** The filters of the managers' list: movies or shows, one person's, a title's words. */
export interface RequestFilters {
  kind: TitleKind | null;
  userId: number | null;
  q: string;
}

/** The listing for a tab (Needs attention: `attention=1&order=oldest`) and the filters, the first `limit`. */
export function requestsQuery(tab: RequestsTab, f: RequestFilters, limit: number): RequestQuery {
  const q: RequestQuery = {};
  if (tab === "attention") {
    q.attention = true;
    q.order = "oldest";
  } else if (tab !== "all") q.status = [tab];
  if (f.kind) q.kind = f.kind;
  if (f.userId !== null) q.userId = f.userId;
  const words = f.q.trim();
  if (words) q.q = words;
  q.offset = 0;
  q.limit = limit;
  return q;
}

/** Whether a request (as an action left it) still belongs on a tab: what was approved leaves Needs attention and Pending at once. */
export function inRequestsTab(r: MediaRequest, tab: RequestsTab): boolean {
  if (tab === "all") return true;
  if (tab === "attention") return r.status === "pending" || (r.status === "failed" && r.retry_at === null);
  return r.status === tab;
}

/** A tab's number: its status's count (every filter but the status applied), their sum for All, what waits for managers. */
export function tabCount(tab: RequestsTab, counts: Record<RequestStatus, number> | null, s: RequestSummary | null): number | null {
  if (tab === "attention") return s ? s.pending + s.failed : null;
  if (!counts) return null;
  if (tab === "all") return STATUS_TABS.reduce((n, status) => n + (counts[status] ?? 0), 0);
  return counts[tab] ?? 0;
}

// ---- who sees what

/** Discover shows, and asking works: requests are on and this profile may ask. */
export function mayAsk(s: RequestSummary | null): boolean {
  return !!s && s.enabled && s.access !== "off";
}

/** Requests show at all: on, and this sign-in can reach them; an account that may not ask still sees what it asked for before. */
export function hasRequests(s: RequestSummary | null): boolean {
  return !!s && s.enabled && s.sign_in_again !== "requests" && (s.access !== "off" || s.own > 0 || s.manage);
}

/** "Requests" for managers (it opens on Needs attention), "Your requests" for everyone else. */
export function requestsTitle(s: RequestSummary | null): string {
  return s && s.manage ? t("requests.names.requests") : t("requests.names.yourRequests");
}

/**
 * The one number on the way to requests: for managers what waits for
 * them and their own news, in the danger colour while some failed; for
 * everyone else their news.
 */
export function requestsCount(s: RequestSummary | null): { count: number; danger: boolean } {
  if (!s || !s.enabled) return { count: 0, danger: false };
  if (s.manage) return { count: s.pending + s.failed + s.unseen, danger: s.failed > 0 };
  return { count: s.unseen, danger: false };
}

/** A summary kept on the TV is trusted only in shape. */
export function isSummary(x: unknown): x is RequestSummary {
  const s = x as RequestSummary | null;
  return !!s && typeof s === "object" && typeof s.enabled === "boolean" && typeof s.access === "string" && typeof s.own === "number";
}

// ---- titles

export function titleKey(title: { kind: TitleKind; tmdb_id: number }): string {
  return `${title.kind}:${title.tmdb_id}`;
}

/** `more` after `have`, each kind and tmdb_id once (a title can come again on a later page). */
export function addTitles(have: DiscoverTitle[], more: DiscoverTitle[]): DiscoverTitle[] {
  const seen = new Set(have.map(titleKey));
  const out = have.slice();
  for (const title of more) {
    const k = titleKey(title);
    if (seen.has(k)) continue;
    seen.add(k);
    out.push(title);
  }
  return out;
}

/** The most characters TMDB's search takes, as the server has it. */
export const SEARCH_MAX = 200;

/** Words as TMDB's search takes them: trimmed, and cut to {@link SEARCH_MAX} characters (never halfway through one). */
export function searchText(q: string): string {
  return Array.from(q.trim()).slice(0, SEARCH_MAX).join("").trim();
}

/** A title's badge: "In library", "Partly in library" (where it can be opened), "Requested", "Declined"; none otherwise. */
export function titleBadge(title: { status: TitleStatus; item_id: number | null }): string | null {
  switch (title.status) {
    case "library":
      return t("requests.badge.library");
    case "partial":
      return title.item_id !== null ? t("requests.badge.partial") : t("requests.badge.requested");
    case "requested":
      return t("requests.badge.requested");
    case "declined":
      return t("requests.badge.declined");
    default:
      return null;
  }
}

/** A badge's colour: what can be watched green, what is on its way blue, what was refused red. */
export function titleTone(title: { status: TitleStatus; item_id: number | null }): Tone {
  if (title.status === "library" || (title.status === "partial" && title.item_id !== null)) return "green";
  if (title.status === "declined") return "red";
  return "blue";
}

/** "Movie" or "Show". */
export function kindWord(kind: TitleKind): string {
  return kind === "show" ? t("requests.kind.show") : t("requests.kind.movie");
}

/** A tile's second line: the year, and in rows of both kinds which it is. */
export function titleSubtitle(title: { kind: TitleKind; year: number | null }, mixed: boolean): string {
  return [title.year ? String(title.year) : "", mixed ? kindWord(title.kind) : ""].filter(Boolean).join(" · ");
}

/**
 * Where a title's poster opens: the library's title when a library this
 * account can open has it whole, since its Discover page would offer only
 * Open; else its Discover page. A show partly there opens its Discover
 * page, where more seasons can be asked for.
 */
export function titlePage(title: { kind: TitleKind; tmdb_id: number; status: TitleStatus; item_id: number | null }): PageKey {
  return title.status === "library" && title.item_id !== null ? { name: "item", id: title.item_id } : { name: "title", kind: title.kind, tmdbId: title.tmdb_id };
}

/** A title page's facts line: year, runtime or seasons, the certification is shown apart. */
export function titleFacts(d: DiscoverDetail): string {
  const seasons = (d.seasons ?? []).filter((s) => s.number > 0).length;
  const length = d.kind === "movie" ? formatRuntime(d.runtime_minutes ? d.runtime_minutes * 60_000 : null) : seasons ? t("requests.counts.seasons", { count: seasons }) : "";
  return [d.year ? String(d.year) : "", length].filter(Boolean).join(" · ");
}

/**
 * What a title's badge becomes from the news of a request this profile
 * made or follows: active → Requested, declined → Declined, a movie
 * available → In library, some of a show's seasons ready → Partly in
 * library, gone → none. `"ask"` where only the title's details can say:
 * a show whose request is wholly ready (it may lack seasons nobody asked
 * for, or have them all), and a request this profile no longer sees
 * (`null`: withdrawn, deleted or no longer followed, while others may
 * have asked). `null` for anyone else's request (a manager hears of
 * everyone's), which says nothing of the title as this profile sees it.
 */
export function statusFromRequest(r: MediaRequest | null): TitleStatus | "ask" | null {
  if (!r) return "ask";
  if (!r.mine && !r.watching) return null;
  if (r.stage === "gone") return "none";
  if (r.status === "declined") return "declined";
  if (r.status === "available") return r.kind === "show" ? "ask" : "library";
  if (r.stage === "partial") return "partial";
  return "requested";
}

/** Pending, approved or failed: it still covers what it asks for. */
export function isActive(r: MediaRequest): boolean {
  return r.status === "pending" || r.status === "approved" || r.status === "failed";
}

// ---- a title page's main action

export type MainAction =
  /** The app signed in before it could ask: the page says so, and offers to sign in again. */
  | { kind: "signIn" }
  /** No button: the restriction is said instead. */
  | { kind: "restricted"; text: string }
  | { kind: "open"; itemId: number | null }
  /** No button: "Declined: “…”. You can ask again in 12 days." */
  | { kind: "declined"; text: string }
  /** [Requested], which does nothing, and its stage. */
  | { kind: "requested"; request: MediaRequest | null }
  /** [Following], which does nothing, its stage, and [Stop following]. */
  | { kind: "following"; request: MediaRequest | null }
  /** Someone asked for it already: hearing when it's ready costs nothing (a show's: the seasons asked for). */
  | { kind: "tellMeToo"; seasons: number[] | undefined }
  /** Partly here: [Open], and [Request more seasons] where some are left to ask for. */
  | { kind: "more"; itemId: number | null; more: boolean; ask: boolean; disabled: boolean }
  | { kind: "request"; disabled: boolean }
  /** No Request: "Requests aren't open to this account." */
  | { kind: "noAccess" };

/**
 * A title page's one main action, the first that applies: sign in again,
 * restricted, in the library, declined, requested (by this profile, or
 * followed, or by someone else), partly in the library, and Request,
 * disabled when nothing is left to ask for.
 */
export function mainAction(d: DiscoverDetail, s: RequestSummary | null, now = Date.now()): MainAction {
  if (s && s.sign_in_again === "requests") return { kind: "signIn" };
  if (d.status === "restricted") return { kind: "restricted", text: d.restriction || t("requests.restricted") };
  if (d.status === "library") return { kind: "open", itemId: d.item_id };
  if (d.status === "declined") return { kind: "declined", text: declinedText(d.declined, now) };
  const ask = !s || s.access !== "off";
  const active = (d.requests ?? []).filter(isActive);
  if (d.status === "requested") {
    const mine = active.filter((r) => r.mine);
    if (mine.length) return { kind: "requested", request: pick(mine, d.request_id) };
    if (d.watching) return { kind: "following", request: pick(active.filter((r) => r.watching), d.request_id) ?? pick(active, d.request_id) };
    if (!ask) return { kind: "noAccess" };
    return { kind: "tellMeToo", seasons: d.kind === "show" ? requestedSeasons(d) : undefined };
  }
  const left = d.kind === "movie" ? d.movies_left : d.seasons_left;
  const none = left !== null && left <= 0;
  if (d.status === "partial") return { kind: "more", itemId: d.item_id, more: hasMoreSeasons(d), ask, disabled: none };
  return ask ? { kind: "request", disabled: none } : { kind: "noAccess" };
}

/**
 * The main action from a list's title alone, while its details load or
 * when they can't be had: never "Tell me too" for what this profile may
 * have asked for itself.
 */
export function listAction(title: DiscoverTitle, s: RequestSummary | null): MainAction {
  if (s && s.sign_in_again === "requests") return { kind: "signIn" };
  if (title.status === "restricted") return { kind: "restricted", text: t("requests.restricted") };
  if (title.status === "library") return { kind: "open", itemId: title.item_id };
  if (title.status === "declined") return { kind: "declined", text: t("requests.declined.plain") };
  if (title.status === "requested") return { kind: "requested", request: null };
  const ask = !s || s.access !== "off";
  const left = s ? (title.kind === "movie" ? s.movies_left : s.seasons_left) : null;
  const none = left !== null && left <= 0;
  if (title.status === "partial") return { kind: "more", itemId: title.item_id, more: true, ask, disabled: none };
  return ask ? { kind: "request", disabled: none } : { kind: "noAccess" };
}

function pick(list: MediaRequest[], id: number | null): MediaRequest | null {
  return (id !== null ? list.filter((r) => r.id === id)[0] : undefined) ?? list[0] ?? null;
}

/** "Declined: “Not on any service”. You can ask again in 12 days.", without the reason when there's none. */
export function declinedText(declined: DiscoverDetail["declined"], now = Date.now()): string {
  const reason = declined && declined.reason ? declined.reason.trim().replace(/[.\s]+$/, "") : "";
  const lead = reason ? t("requests.declined.because", { reason }) : t("requests.declined.plain");
  return declined ? `${lead} ${t("requests.declined.askAgain", { when: timeUntil(declined.again_at, now) })}` : lead;
}

/** The request a title page's stage pill follows: this profile's own, else one it follows, else the newest active one. */
export function titleRequest(d: DiscoverDetail): MediaRequest | null {
  const active = (d.requests ?? []).filter(isActive);
  return pick(active.filter((r) => r.mine), d.request_id) ?? pick(active.filter((r) => r.watching), d.request_id) ?? pick(active, d.request_id);
}

// ---- seasons

/** "Season 2", "Seasons 1–3, 5", "Seasons 1–2 and specials", "Specials". */
export function seasonsLabel(seasons: number[] | null | undefined): string {
  const list = (seasons ?? []).filter((n, i, all) => all.indexOf(n) === i).sort((a, b) => a - b);
  const specials = list.indexOf(0) >= 0;
  const numbered = list.filter((n) => n > 0);
  if (!numbered.length) return specials ? t("requests.seasons.specials") : "";
  const runs: string[] = [];
  let start = numbered[0]!;
  let prev = start;
  for (const n of numbered.slice(1).concat([-1])) {
    if (n === prev + 1) {
      prev = n;
      continue;
    }
    runs.push(start === prev ? String(start) : `${start}–${prev}`);
    start = n;
    prev = n;
  }
  const label = t("requests.seasons.list", { count: numbered.length, list: runs.join(", ") });
  return specials ? t("requests.seasons.andSpecials", { seasons: label }) : label;
}

/** The seasons asked for already: Tell me too follows those. */
export function requestedSeasons(d: DiscoverDetail): number[] {
  return (d.seasons ?? []).filter((s) => s.state === "requested").map((s) => s.number);
}

/** A season that may be ticked: missing, partly had or not aired yet, not declined, and the title not restricted. */
export function askable(s: DiscoverSeason, d: { status: TitleStatus }): boolean {
  return d.status !== "restricted" && !s.declined && (s.state === "missing" || s.state === "partial" || s.state === "unaired");
}

/** "Request more seasons" shows: a numbered season is askable, and missing or partly had. */
export function hasMoreSeasons(d: DiscoverDetail): boolean {
  return (d.seasons ?? []).some((s) => s.number > 0 && askable(s, d) && (s.state === "missing" || s.state === "partial"));
}

/** The picker lists every season: the numbered ones in order, specials last. */
export function pickerSeasons(d: DiscoverDetail): DiscoverSeason[] {
  const all = d.seasons ?? [];
  return all
    .filter((s) => s.number > 0)
    .sort((a, b) => a.number - b.number)
    .concat(all.filter((s) => s.number <= 0));
}

/**
 * What the picker ticks as it opens: the askable missing and partly had
 * seasons other than specials, lowest first, as many as may be asked for;
 * seasons not aired yet and specials are offered unticked.
 */
export function preselected(d: DiscoverDetail, left: number | null): number[] {
  const out = pickerSeasons(d)
    .filter((s) => s.number > 0 && askable(s, d) && (s.state === "missing" || s.state === "partial"))
    .map((s) => s.number);
  return left === null ? out : out.slice(0, Math.max(0, left));
}

/** The picker's button: "Request 3 seasons", "Request 1 season"; "Pick a season." while none is ticked. */
export function pickerButton(n: number): string {
  if (n <= 0) return t("requests.picker.pickOne");
  return t("requests.picker.request", { count: n });
}

/** A season's line under its name: its state ("In library", "Requested", "Declined"…), else what it holds. */
export function seasonLine(s: DiscoverSeason, now = Date.now()): string {
  if (s.state === "library") return t("requests.season.library");
  if (s.state === "requested") return t("requests.season.requested");
  if (s.declined) return t("requests.season.declined");
  if (s.state === "partial") return t("requests.season.partial");
  if (s.state === "unaired") return s.air_date ? t("requests.season.airs", { day: isoDay(s.air_date, now) }) : t("requests.season.notAired");
  return t("requests.counts.episodes", { count: s.episodes });
}

/** A season's name where TMDB gives none: "Season 2", "Specials" for season 0. */
export function seasonName(number: number): string {
  return number === 0 ? t("requests.seasons.specials") : t("requests.seasons.season", { number });
}

/** "2026-11-04" → "Nov 4" (with the year when it isn't this one's). */
function isoDay(iso: string, now: number): string {
  const m = /^(\d{4})-(\d{2})-(\d{2})/.exec(iso);
  if (!m) return iso;
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]), 12);
  return shortDate(Math.floor(d.getTime() / 1000), now);
}

// ---- what may still be asked for

export interface Allowance {
  movies_left: number | null;
  seasons_left: number | null;
  movies_next_at: number | null;
  seasons_next_at: number | null;
  movies_limit?: number | null;
  seasons_limit?: number | null;
  /** What is being asked for: its lines alone (a movie's page says nothing of seasons). */
  kind?: TitleKind;
}

/**
 * What this profile may still ask for, in a line or two, the limits last
 * ("per 7 days": they're a rolling week, never "this week"); `null`
 * without limits.
 */
export function allowanceLines(given: Allowance, now = Date.now()): string[] | null {
  const a = given.kind ? allowanceFor(given, given.kind) : given;
  const m = a.movies_left;
  const s = a.seasons_left;
  if (m === null && s === null) return null;
  const mOut = m !== null && m <= 0;
  const sOut = s !== null && s <= 0;
  const when = (at: number | null) => (at !== null ? timeUntil(at, now) : null);
  const lines: string[] = [];
  if (mOut && sOut) {
    const mw = when(a.movies_next_at);
    const sw = when(a.seasons_next_at);
    lines.push(mw && sw ? t("requests.allowance.allUsedUpUntil", { movies: mw, seasons: sw }) : t("requests.allowance.allUsedUp"));
  } else if (mOut || sOut) {
    const w = when(mOut ? a.movies_next_at : a.seasons_next_at);
    if (mOut) lines.push(w ? t("requests.allowance.moviesUsedUpUntil", { when: w }) : t("requests.allowance.moviesUsedUp"));
    else lines.push(w ? t("requests.allowance.seasonsUsedUpUntil", { when: w }) : t("requests.allowance.seasonsUsedUp"));
    const other = mOut ? s : m;
    if (other !== null) {
      const more = mOut ? t("requests.allowance.moreSeasons", { count: other }) : t("requests.allowance.moreMovies", { count: other });
      lines.push(t("requests.allowance.canAsk", { list: more }));
    }
  } else {
    const parts = [m !== null ? t("requests.allowance.moreMovies", { count: m }) : "", s !== null ? t("requests.allowance.moreSeasons", { count: s }) : ""].filter(Boolean);
    lines.push(t("requests.allowance.canAsk", { list: both(parts) }));
  }
  const limits = [
    a.movies_limit ? t("requests.allowance.movies", { count: a.movies_limit }) : "",
    a.seasons_limit ? t("requests.counts.seasons", { count: a.seasons_limit }) : "",
  ].filter(Boolean);
  if (limits.length) lines.push(t("requests.allowance.limits", { list: both(limits) }));
  return lines;
}

/** One count or two in a line: "2 more movies and 3 more seasons". */
function both(parts: string[]): string {
  return parts.length === 2 ? t("requests.allowance.and", { first: parts[0]!, second: parts[1]! }) : (parts[0] ?? "");
}

/** The allowance of one kind: the other's counts and limit left out. */
function allowanceFor(a: Allowance, kind: TitleKind): Allowance {
  return kind === "movie"
    ? { ...a, seasons_left: null, seasons_next_at: null, seasons_limit: null }
    : { ...a, movies_left: null, movies_next_at: null, movies_limit: null };
}

// ---- a request's stage

/** The stage pill's colour: waiting for a decision, on its way, ready, refused or failed, gone. */
export type Tone = "amber" | "blue" | "green" | "red" | "gray";

export interface StageWords {
  text: string;
  tone: Tone;
  /** The download bar, 0–1, while something downloads. */
  progress: number | null;
  /** A line under it: why it couldn't be sent (managers), "Trying again in 5 min.". */
  note: string | null;
}

/** Where a request has got to, in words and a colour (managers read why one failed). */
export function stageWords(r: MediaRequest, opts: { manage: boolean; hours24: boolean }, now = Date.now()): StageWords {
  const words = (text: string, tone: Tone, progress: number | null = null, note: string | null = null): StageWords => ({ text, tone, progress, note });
  switch (r.stage) {
    case "pending":
      return words(t("requests.stage.pending"), "amber");
    case "declined":
      return words(t("requests.stage.declined"), "red", null, r.reason ? t("requests.quoted", { text: r.reason.trim() }) : null);
    case "failed": {
      const retry = r.retry_at !== null ? t("requests.stage.retry", { when: timeUntil(r.retry_at, now) }) : null;
      // "Couldn't send: {reason}" in two parts, so a long reason is read whole under a short pill.
      if (opts.manage) return words(t("requests.stage.failed"), "red", null, [r.reason ? tidy(r.reason) : "", retry ?? ""].filter(Boolean).join(" ") || null);
      return words(t("requests.stage.notSent"), "red", null, retry ?? t("requests.stage.notSentLine"));
    }
    case "approved":
      return words(t("requests.stage.approved"), "blue");
    case "waiting":
      if (r.stage_at === null) return words(t("requests.stage.notOut"), "blue");
      // A release is known by its day only (sent as 12:00 UTC of it); an episode's airing exactly.
      return r.kind === "movie"
        ? words(t("requests.stage.out", { day: dayAndTime(utcDay(r.stage_at), { time: false, hours24: opts.hours24 }, now) }), "blue")
        : words(t("requests.stage.airs", { day: dayAndTime(r.stage_at, { time: true, hours24: opts.hours24 }, now) }), "blue");
    case "searching":
      return words(t("requests.stage.searching"), "blue");
    case "downloading":
      return downloadWords(r, now);
    case "importing":
      return words(t("requests.stage.importing"), "blue");
    case "partial": {
      const seasons = seasonsLabel(r.available_seasons);
      return words(seasons ? t("requests.stage.partial", { seasons }) : t("requests.stage.partialSome"), "green", r.download ? r.download.progress : null);
    }
    case "available":
      return words(t("requests.stage.available"), "green", null, r.item_id === null ? t("requests.stage.outOfReach") : null);
    case "gone":
      return words(t("requests.stage.gone"), "gray");
    default:
      // A stage this app doesn't know (or a server from before stages): its status says as much.
      return r.stage !== r.status && STAGE_OF_STATUS.indexOf(r.status) >= 0 ? stageWords({ ...r, stage: r.status }, opts, now) : words(t("requests.stage.approved"), "blue");
  }
}

const STAGE_OF_STATUS: string[] = ["pending", "approved", "declined", "failed", "available"];

/** The release day sent as 12:00 UTC of it, as noon of that day on the TV's clock, so every time zone shows the same day. */
function utcDay(seconds: number): number {
  const d = new Date(seconds * 1000);
  return Math.floor(new Date(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate(), 12).getTime() / 1000);
}

function downloadWords(r: MediaRequest, now: number): StageWords {
  const d = r.download;
  const at = (text: string): StageWords => ({ text, tone: "blue", progress: d ? d.progress : null, note: null });
  if (!d) return at(t("requests.download.downloading"));
  switch (d.state) {
    case "queued":
      return at(t("requests.download.queued"));
    case "paused":
      return at(t("requests.download.paused"));
    case "warning":
      return at(t("requests.download.warning"));
    case "importing":
      return at(t("requests.download.importing"));
    default: {
      // The time left after " · ", which a season's caption puts on a line of its own.
      const eta = d.eta !== null && d.eta > now / 1000 + 30 ? ` · ${timeUntil(d.eta, now)}` : "";
      return at(`${t("requests.download.progress", { percent: Math.floor(Math.max(0, Math.min(1, d.progress)) * 100) })}${eta}`);
    }
  }
}

/**
 * A season's own stage from the request asking for it: of a show's
 * request partly ready, a season is ready, or downloads while the request
 * does; `null` for one still to come with nothing downloading (its own
 * state says the rest). Other stages are the request's.
 */
export function seasonStage(r: MediaRequest, season: number, opts: { manage: boolean; hours24: boolean }, now = Date.now()): StageWords | null {
  if (r.stage !== "partial") return stageWords(r, opts, now);
  if ((r.available_seasons ?? []).indexOf(season) >= 0) return { text: t("requests.stage.available"), tone: "green", progress: null, note: null };
  return r.download ? downloadWords(r, now) : null;
}

/** Something on the list downloads or goes into the library: the list is asked for again every 10 s. */
export function moving(list: MediaRequest[]): boolean {
  return list.some((r) => r.stage === "downloading" || r.stage === "importing" || (r.stage === "partial" && !!r.download));
}

/**
 * `next`, a list asked for again, with each request that the server
 * answered just as in `before` kept as the object it was, so its card
 * isn't drawn again: of 200 cards, only those whose download moved are.
 */
export function keepUnchanged(before: RequestList | undefined, next: RequestList): RequestList {
  if (!before) return next;
  const was = new Map<number, MediaRequest>();
  for (const r of before.requests) was.set(r.id, r);
  return {
    ...next,
    requests: next.requests.map((r) => {
      const old = was.get(r.id);
      return old && sameJson(old, r) ? old : r;
    }),
  };
}

/** Two answers' values alike, field by field all the way down. */
function sameJson(a: unknown, b: unknown): boolean {
  if (a === b) return true;
  if (!a || !b || typeof a !== "object" || typeof b !== "object" || Array.isArray(a) !== Array.isArray(b)) return false;
  const x = a as Record<string, unknown>;
  const y = b as Record<string, unknown>;
  const keys = Object.keys(x);
  if (keys.length !== Object.keys(y).length) return false;
  for (const k of keys) if (!sameJson(x[k], y[k])) return false;
  return true;
}

/** Who asked and when: "Asked by you · 3 days ago", "Asked by Ann · yesterday", "Asked for by someone else · now". */
export function askedLine(r: MediaRequest, manage: boolean, now = Date.now()): string {
  const who = r.mine ? t("requests.asked.byYou") : r.requested_by ? t("requests.asked.byName", { name: r.requested_by.name }) : t("requests.asked.bySomeone");
  const parts = [who, relativeTime(r.created_at, now)];
  if (r.watching && !r.mine) parts.unshift(t("requests.asked.youFollow"));
  if (manage && r.watchers > 0) parts.push(t("requests.asked.following", { count: r.watchers }));
  return parts.join(" · ");
}

/** What the server tells managers of a request ("In Movies 4K, which Ann can't open."), a sentence as the stage's line is. */
export function requestNote(r: MediaRequest): string | null {
  return r.note && r.note.trim() ? tidy(r.note) : null;
}

// ---- a request's actions

export type ActionId = RequestAction | "decline_reason";

export interface ActionRow {
  id: ActionId;
  label: string;
  danger?: boolean;
  /** Asked first: what can't be undone. */
  confirm?: { title: string; body?: string; button: string };
}

/**
 * The actions the server lists for a request, in its order, as rows: TVs
 * offer Decline and "Decline with a reason…" (the TV's keyboard), and no
 * approval dialog.
 */
export function actionRows(r: MediaRequest): ActionRow[] {
  const out: ActionRow[] = [];
  for (const a of r.actions) {
    switch (a) {
      case "withdraw":
        if (r.status === "declined") {
          const remove = t("requests.actions.remove");
          out.push({ id: a, label: remove, confirm: { title: t("requests.confirm.remove", { title: r.title }), button: remove } });
        } else {
          const withdraw = t("requests.actions.withdraw");
          out.push({ id: a, label: withdraw, confirm: { title: t("requests.confirm.withdraw", { title: r.title }), button: withdraw } });
        }
        break;
      case "unwatch":
        out.push({ id: a, label: t("requests.actions.unwatch") });
        break;
      case "approve":
        out.push({ id: a, label: t("requests.actions.approve") });
        break;
      case "retry":
        out.push({ id: a, label: t("requests.actions.retry") });
        break;
      case "decline":
        out.push({ id: a, label: t("requests.actions.decline") }, { id: "decline_reason", label: t("requests.actions.declineReason") });
        break;
      case "handled":
        out.push({ id: a, label: t("requests.actions.handled") });
        break;
      case "delete": {
        const label = t("requests.actions.delete");
        out.push({ id: a, label, danger: true, confirm: { title: t("requests.confirm.delete"), body: t("requests.confirm.deleteBody"), button: label } });
        break;
      }
      case "ask_again":
        out.push({ id: a, label: t("requests.actions.askAgain") });
        break;
      default:
        // An action a newer server offers: the web has it.
        break;
    }
  }
  return out;
}

/** What a toast says once an action is done. */
export function actionDone(a: ActionId, r: MediaRequest): string {
  const title = { title: r.title };
  switch (a) {
    case "withdraw":
      return r.status === "declined" ? t("requests.done.removed", title) : t("requests.done.withdrew", title);
    case "unwatch":
      return t("requests.done.unfollowed", title);
    case "approve":
      return t("requests.done.approved", title);
    case "retry":
      return t("requests.done.retrying", title);
    case "decline":
    case "decline_reason":
      return t("requests.done.declined", title);
    case "handled":
      return t("requests.done.handled", title);
    case "delete":
      return t("requests.done.deleted", title);
    default:
      return "";
  }
}

// ---- asking, and news

/** What asking answered: a new request, by how it was decided, or the requests followed. */
export function askedText(r: MediaRequest, created: boolean): string {
  const title = { title: r.title };
  if (!created) return t("requests.asking.following", title);
  if (r.status === "failed") return t("requests.asking.failed", title);
  if (r.status === "pending") return t("requests.asking.pending", title);
  return t("requests.asking.approved", title);
}

/** "“X” is ready to watch.", "Season 2 of “X” is ready to watch.", "Seasons 1–2 of “X” are ready to watch." */
export function readyText(title: string, seasons: number[] | null | undefined, complete: boolean): string {
  if (complete || !seasons || !seasons.length) return t("requests.news.ready", { title });
  if (seasons.length === 1 && seasons[0] === 0) return t("requests.news.specialsReady", { title });
  // The count isn't written: it says whether one season is ready ("is") or several are ("are").
  return t("requests.news.seasonsReady", { count: seasons.length > 1 ? 2 : 1, seasons: seasonsLabel(seasons), title });
}

/** Several pieces of news at once. */
export function newsText(n: number): string {
  return t("requests.news.several", { count: n });
}

/** A request whose news is that it's ready to watch, whole or in part: the TVs pop up only these. */
export function readyNews(r: MediaRequest): boolean {
  return r.stage === "available" || r.stage === "partial";
}

/**
 * What ready-to-watch news is remembered by, so none is announced twice:
 * its request whole, or each of a show's seasons ready so far, since they
 * come a batch at a time.
 */
export function newsKeys(requestId: number, complete: boolean, seasons: number[] | null | undefined): string[] {
  if (complete) return [`${requestId}:a`];
  if (!seasons || !seasons.length) return [`${requestId}:p`];
  return seasons.map((n) => `${requestId}:s${n}`);
}

// ---- refusals

export type RequestsProblem =
  | { kind: "off" }
  | { kind: "tmdbKey" }
  | { kind: "tmdbDown" }
  | { kind: "noAccess" }
  | { kind: "signIn" }
  | { kind: "restricted"; text: string }
  | { kind: "declined"; retryAt: number | null }
  | { kind: "limit"; retryAt: number | null }
  | { kind: "conflict"; text: string; request: MediaRequest | null }
  | { kind: "missing" }
  | { kind: "other"; error: unknown };

/** What the server's refusal of a request route means (its `code`, the fields beside it). */
export function requestsProblem(e: unknown): RequestsProblem {
  if (!(e instanceof ApiError) || unreachable(e)) return { kind: "other", error: e };
  const body = (e.body ?? {}) as { setup?: unknown; retry_at?: unknown; request?: unknown };
  const retryAt = typeof body.retry_at === "number" ? body.retry_at : null;
  if (e.status === 503) {
    if (e.code === "tmdb_key") return { kind: "tmdbKey" };
    if (e.code === "tmdb_down") return { kind: "tmdbDown" };
    // A server from before the reasons said TMDB's trouble with `setup: true`.
    if (e.code === "unavailable") return body.setup === true ? { kind: "tmdbDown" } : { kind: "off" };
    return { kind: "other", error: e };
  }
  if (e.status === 403) {
    if (e.code === "insufficient_scope") return { kind: "signIn" };
    if (e.code === "restricted") return { kind: "restricted", text: tidy(e.message) };
    return { kind: "noAccess" };
  }
  if (e.status === 409 && e.code === "declined") return { kind: "declined", retryAt };
  if (e.status === 429) return { kind: "limit", retryAt };
  if (e.status === 409) {
    const r = body.request as MediaRequest | undefined;
    return { kind: "conflict", text: tidy(e.message), request: r && typeof r === "object" && typeof r.id === "number" ? r : null };
  }
  if (e.status === 404) return { kind: "missing" };
  return { kind: "other", error: e };
}

/** A page that can't show requests: why, and whether Retry may help; `null` for what isn't about requests being usable. */
export function problemWords(p: RequestsProblem, manage: boolean): { title: string; body: string | null; retry: boolean } | null {
  switch (p.kind) {
    case "off":
      return { title: t("requests.problem.off"), body: t("requests.problem.offText"), retry: false };
    case "noAccess":
      return { title: t("requests.problem.noAccess"), body: t("requests.problem.noAccessText"), retry: false };
    case "tmdbDown":
      return { title: t("requests.problem.tmdbDown"), body: t("requests.problem.tmdbDownText"), retry: true };
    case "tmdbKey":
      return { title: t("requests.problem.tmdbKey"), body: manage ? t("requests.problem.tmdbKeyManage") : t("requests.problem.tmdbKeyText"), retry: false };
    case "signIn":
      return { title: t("requests.problem.signIn"), body: null, retry: false };
    default:
      return null;
  }
}

/**
 * What a toast says when asking was refused: when one may ask again (from
 * `retry_at`, never the message's date), the limit, the restriction.
 */
export function askRefusal(p: RequestsProblem, kind: TitleKind, limits: { movies_limit: number | null; seasons_limit: number | null } | null, now = Date.now()): string {
  switch (p.kind) {
    case "declined":
      return p.retryAt !== null ? t("requests.declined.askAgain", { when: timeUntil(p.retryAt, now) }) : t("requests.problem.declined");
    case "limit": {
      if (p.retryAt !== null) return t("requests.problem.askMore", { when: timeUntil(p.retryAt, now) });
      const limit = limits ? (kind === "movie" ? limits.movies_limit : limits.seasons_limit) : null;
      if (!limit) return t("requests.problem.tooMany");
      return kind === "movie" ? t("requests.problem.atMostMovies", { count: limit }) : t("requests.problem.atMostSeasons", { count: limit });
    }
    case "restricted":
      return p.text;
    case "conflict":
      return p.text;
    case "missing":
      return t("requests.problem.missing");
    case "other":
      return tidy(errorMessage(p.error));
    default: {
      const words = problemWords(p, false);
      return words ? `${words.title}.` : "";
    }
  }
}

/** The server's message as a sentence. */
function tidy(text: string): string {
  const trimmed = text.trim();
  if (!trimmed) return trimmed;
  const s = trimmed.charAt(0).toUpperCase() + trimmed.slice(1);
  return /[.…!?]$/.test(s) ? s : `${s}.`;
}

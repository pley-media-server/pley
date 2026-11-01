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
import { dayAndTime, formatRuntime, pluralize, relativeTime, shortDate, timeUntil } from "./format/format";

/*
 * Requests and Discover in words and rules, as every client has them
 * (docs/API.md, "Requests"): who sees what, a title's badge and its one
 * main action, the season picker, a request's stage and actions, the
 * allowance, and what each of the server's refusals says. A platform may
 * shorten these words, never reword them.
 */

/** Discover's foot and every title page say where their details come from. */
export const TMDB_CREDIT = "Details and pictures from TMDB.";
/** Settings → About. */
export const TMDB_NOTICE = "This product uses the TMDB API but is not endorsed or certified by TMDB.";

export const SIGN_IN_TITLE = "Sign in again to ask for movies and shows";
export const SIGN_IN_BODY = "This app signed in before it could ask. Signing in again lets it.";
export const SIGN_IN_APPROVALS = "Sign in again to approve and decline here.";
export const NO_ACCESS = "Requests aren't open to this account.";

/** Discover's rows, in order; `mixed` rows hold movies and shows both, so their tiles say which. */
export const DISCOVER_ROWS: { list: DiscoverList; title: string; mixed: boolean }[] = [
  { list: "trending", title: "Trending this week", mixed: true },
  { list: "movies", title: "Popular movies", mixed: false },
  { list: "shows", title: "Popular shows", mixed: false },
  { list: "upcoming", title: "Coming to cinemas", mixed: false },
  { list: "anime", title: "Popular anime", mixed: false },
];

/** The status tabs of a list of requests, after All. */
export const STATUS_TABS: { status: RequestStatus; label: string }[] = [
  { status: "pending", label: "Pending" },
  { status: "approved", label: "Approved" },
  { status: "available", label: "Available" },
  { status: "declined", label: "Declined" },
  { status: "failed", label: "Failed" },
];

/** A tab of a list of requests: Needs attention (managers), All, or one status. */
export type RequestsTab = "attention" | "all" | RequestStatus;

/** The tabs, Needs attention first for managers (they open on it). */
export function requestsTabs(manage: boolean): { tab: RequestsTab; label: string }[] {
  const tabs: { tab: RequestsTab; label: string }[] = [{ tab: "all", label: "All" }];
  for (const t of STATUS_TABS) tabs.push({ tab: t.status, label: t.label });
  const attention: { tab: RequestsTab; label: string }[] = [{ tab: "attention", label: "Needs attention" }];
  return manage ? attention.concat(tabs) : tabs;
}

/** What a tab with nothing in it says (`everyone`: a manager's list of everyone's). */
export function requestsEmpty(tab: RequestsTab, everyone: boolean): string {
  const whose = everyone ? "" : " of yours";
  switch (tab) {
    case "attention":
      return "No requests need attention.";
    case "pending":
      return `No requests${whose} wait for a decision.`;
    case "approved":
      return `No requests${whose} are on their way.`;
    case "available":
      return `No requests${whose} are ready to watch yet.`;
    case "declined":
      return `No requests${whose} were declined.`;
    case "failed":
      return `No requests${whose} failed.`;
    default:
      return everyone ? "No one has asked for anything yet." : "You haven't asked for anything yet.";
  }
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
  if (tab === "all") return STATUS_TABS.reduce((n, t) => n + (counts[t.status] ?? 0), 0);
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
  return s && s.manage ? "Requests" : "Your requests";
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

export function titleKey(t: { kind: TitleKind; tmdb_id: number }): string {
  return `${t.kind}:${t.tmdb_id}`;
}

/** `more` after `have`, each kind and tmdb_id once (a title can come again on a later page). */
export function addTitles(have: DiscoverTitle[], more: DiscoverTitle[]): DiscoverTitle[] {
  const seen = new Set(have.map(titleKey));
  const out = have.slice();
  for (const t of more) {
    const k = titleKey(t);
    if (seen.has(k)) continue;
    seen.add(k);
    out.push(t);
  }
  return out;
}

/** A title's badge: "In library", "Partly in library" (where it can be opened), "Requested", "Declined"; none otherwise. */
export function titleBadge(t: { status: TitleStatus; item_id: number | null }): string | null {
  switch (t.status) {
    case "library":
      return "In library";
    case "partial":
      return t.item_id !== null ? "Partly in library" : "Requested";
    case "requested":
      return "Requested";
    case "declined":
      return "Declined";
    default:
      return null;
  }
}

/** A badge's colour: what can be watched green, what is on its way blue, what was refused red. */
export function titleTone(t: { status: TitleStatus; item_id: number | null }): Tone {
  if (t.status === "library" || (t.status === "partial" && t.item_id !== null)) return "green";
  if (t.status === "declined") return "red";
  return "blue";
}

/** A tile's second line: the year, and in rows of both kinds which it is. */
export function titleSubtitle(t: { kind: TitleKind; year: number | null }, mixed: boolean): string {
  return [t.year ? String(t.year) : "", mixed ? (t.kind === "show" ? "Show" : "Movie") : ""].filter(Boolean).join(" · ");
}

/** A title page's facts line: year, runtime or seasons, the certification is shown apart. */
export function titleFacts(d: DiscoverDetail): string {
  const seasons = (d.seasons ?? []).filter((s) => s.number > 0).length;
  const length = d.kind === "movie" ? formatRuntime(d.runtime_minutes ? d.runtime_minutes * 60_000 : null) : seasons ? pluralize(seasons, "season") : "";
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
  if (d.status === "restricted") return { kind: "restricted", text: d.restriction || "Above this account's age limit" };
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
export function listAction(t: DiscoverTitle, s: RequestSummary | null): MainAction {
  if (s && s.sign_in_again === "requests") return { kind: "signIn" };
  if (t.status === "restricted") return { kind: "restricted", text: "Above this account's age limit" };
  if (t.status === "library") return { kind: "open", itemId: t.item_id };
  if (t.status === "declined") return { kind: "declined", text: "Declined." };
  if (t.status === "requested") return { kind: "requested", request: null };
  const ask = !s || s.access !== "off";
  const left = s ? (t.kind === "movie" ? s.movies_left : s.seasons_left) : null;
  const none = left !== null && left <= 0;
  if (t.status === "partial") return { kind: "more", itemId: t.item_id, more: true, ask, disabled: none };
  return ask ? { kind: "request", disabled: none } : { kind: "noAccess" };
}

function pick(list: MediaRequest[], id: number | null): MediaRequest | null {
  return (id !== null ? list.filter((r) => r.id === id)[0] : undefined) ?? list[0] ?? null;
}

/** "Declined: “Not on any service”. You can ask again in 12 days.", without the reason when there's none. */
export function declinedText(declined: DiscoverDetail["declined"], now = Date.now()): string {
  const reason = declined && declined.reason ? declined.reason.trim().replace(/[.\s]+$/, "") : "";
  const lead = reason ? `Declined: “${reason}”.` : "Declined.";
  return declined ? `${lead} You can ask again ${timeUntil(declined.again_at, now)}.` : lead;
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
  if (!numbered.length) return specials ? "Specials" : "";
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
  return `${numbered.length === 1 ? "Season" : "Seasons"} ${runs.join(", ")}${specials ? " and specials" : ""}`;
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
  if (n <= 0) return "Pick a season.";
  return n === 1 ? "Request 1 season" : `Request ${n} seasons`;
}

/** A season's line under its name: its state ("In library", "Requested", "Declined"…), else what it holds. */
export function seasonLine(s: DiscoverSeason, now = Date.now()): string {
  if (s.state === "library") return "In library";
  if (s.state === "requested") return "Requested";
  if (s.declined) return "Declined";
  if (s.state === "partial") return "Partly in library";
  if (s.state === "unaired") return s.air_date ? `Airs ${isoDay(s.air_date, now)}` : "Not aired yet";
  return pluralize(s.episodes, "episode");
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
    lines.push(mw && sw ? `You've asked for all you can for now: more movies ${mw}, more seasons ${sw}.` : "You've asked for all you can for now.");
  } else if (mOut || sOut) {
    const what = mOut ? "movies" : "seasons";
    const w = when(mOut ? a.movies_next_at : a.seasons_next_at);
    lines.push(w ? `You've asked for all the ${what} you can for now: you can ask for more ${w}.` : `You've asked for all the ${what} you can for now.`);
    const other = mOut ? s : m;
    if (other !== null) lines.push(`You can ask for ${pluralize(other, mOut ? "more season" : "more movie", mOut ? "more seasons" : "more movies")}.`);
  } else {
    const parts = [m !== null ? pluralize(m, "more movie", "more movies") : "", s !== null ? pluralize(s, "more season", "more seasons") : ""].filter(Boolean);
    lines.push(`You can ask for ${parts.join(" and ")}.`);
  }
  const limits = [a.movies_limit ? pluralize(a.movies_limit, "movie") : "", a.seasons_limit ? pluralize(a.seasons_limit, "season") : ""].filter(Boolean);
  if (limits.length) lines.push(`Limits: ${limits.join(" and ")} per 7 days.`);
  return lines;
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
      return words("Waiting for approval", "amber");
    case "declined":
      return words("Declined", "red", null, r.reason ? `“${r.reason.trim()}”` : null);
    case "failed": {
      const retry = r.retry_at !== null ? `Trying again ${timeUntil(r.retry_at, now)}.` : null;
      // "Couldn't send: {reason}" in two parts, so a long reason is read whole under a short pill.
      if (opts.manage) return words("Couldn't send", "red", null, [r.reason ? tidy(r.reason) : "", retry ?? ""].filter(Boolean).join(" ") || null);
      return words("Not sent yet", "red", null, retry ?? "It couldn't be sent to download yet: an administrator will look at it.");
    }
    case "approved":
      return words("Approved", "blue");
    case "waiting":
      if (r.stage_at === null) return words("Not out yet", "blue");
      // A release is known by its day only (sent as 12:00 UTC of it); an episode's airing exactly.
      return r.kind === "movie"
        ? words(`Out ${dayAndTime(utcDay(r.stage_at), { time: false, hours24: opts.hours24 }, now)}`, "blue")
        : words(`Airs ${dayAndTime(r.stage_at, { time: true, hours24: opts.hours24 }, now)}`, "blue");
    case "searching":
      return words("Searching", "blue");
    case "downloading":
      return downloadWords(r, now);
    case "importing":
      return words("Adding to the library", "blue");
    case "partial":
      return words(`${seasonsLabel(r.available_seasons) || "Some seasons"} ready`, "green", r.download ? r.download.progress : null);
    case "available":
      return words("Ready to watch", "green", null, r.item_id === null ? "Ready, in a library you can't open: ask an administrator" : null);
    case "gone":
      return words("No longer in a library", "gray");
    default:
      // A stage this app doesn't know (or a server from before stages): its status says as much.
      return r.stage !== r.status && STAGE_OF_STATUS.indexOf(r.status) >= 0 ? stageWords({ ...r, stage: r.status }, opts, now) : words("Approved", "blue");
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
  if (!d) return at("Downloading");
  switch (d.state) {
    case "queued":
      return at("Waiting to download");
    case "paused":
      return at("Download paused");
    case "warning":
      return at("The download has a problem");
    case "importing":
      return at("Adding to the library");
    default: {
      const eta = d.eta !== null && d.eta > now / 1000 + 30 ? ` · ${timeUntil(d.eta, now)}` : "";
      return at(`Downloading ${Math.floor(Math.max(0, Math.min(1, d.progress)) * 100)}%${eta}`);
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
  if ((r.available_seasons ?? []).indexOf(season) >= 0) return { text: "Ready to watch", tone: "green", progress: null, note: null };
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
  const who = r.mine ? "Asked by you" : r.requested_by ? `Asked by ${r.requested_by.name}` : "Asked for by someone else";
  const parts = [who, relativeTime(r.created_at, now)];
  if (r.watching && !r.mine) parts.unshift("You follow it");
  if (manage && r.watchers > 0) parts.push(`${r.watchers} following`);
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
        out.push(
          r.status === "declined"
            ? { id: a, label: "Remove", confirm: { title: `Remove “${r.title}” from your requests?`, button: "Remove" } }
            : { id: a, label: "Withdraw", confirm: { title: `Withdraw your request for “${r.title}”?`, button: "Withdraw" } },
        );
        break;
      case "unwatch":
        out.push({ id: a, label: "Stop following" });
        break;
      case "approve":
        out.push({ id: a, label: "Approve" });
        break;
      case "retry":
        out.push({ id: a, label: "Try again" });
        break;
      case "decline":
        out.push({ id: a, label: "Decline" }, { id: "decline_reason", label: "Decline with a reason…" });
        break;
      case "handled":
        out.push({ id: a, label: "Mark as added by hand" });
        break;
      case "delete":
        out.push({ id: a, label: "Delete", danger: true, confirm: { title: "Delete this request?", body: "Radarr and Sonarr keep what they have.", button: "Delete" } });
        break;
      case "ask_again":
        out.push({ id: a, label: "Request again" });
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
  const x = `“${r.title}”`;
  switch (a) {
    case "withdraw":
      return r.status === "declined" ? `Removed ${x} from your requests.` : `Withdrew your request for ${x}.`;
    case "unwatch":
      return `You no longer follow ${x}.`;
    case "approve":
      return `Approved ${x}.`;
    case "retry":
      return `Trying ${x} again.`;
    case "decline":
    case "decline_reason":
      return `Declined ${x}.`;
    case "handled":
      return `Marked ${x} as added by hand.`;
    case "delete":
      return `Deleted the request for ${x}.`;
    default:
      return "";
  }
}

// ---- asking, and news

/** What asking answered: a new request, by how it was decided, or the requests followed. */
export function askedText(r: MediaRequest, created: boolean): string {
  const x = `“${r.title}”`;
  if (!created) return `You'll hear when ${x} is ready.`;
  if (r.status === "failed") return `Asked for ${x}, but it couldn't be sent to download yet.`;
  if (r.status === "pending") return `Asked for ${x}: an administrator decides.`;
  return `${x} is approved and on its way.`;
}

/** "“X” is ready to watch.", "Season 2 of “X” is ready to watch.", "Seasons 1–2 of “X” are ready to watch." */
export function readyText(title: string, seasons: number[] | null | undefined, complete: boolean): string {
  if (complete || !seasons || !seasons.length) return `“${title}” is ready to watch.`;
  const several = seasons.length > 1 || seasons[0] === 0;
  return `${seasonsLabel(seasons)} of “${title}” ${several ? "are" : "is"} ready to watch.`;
}

/** Several pieces of news at once. */
export function newsText(n: number): string {
  return `${n} of your requests have news`;
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
export function problemWords(p: RequestsProblem, manage: boolean): { title: string; body: string; retry: boolean } | null {
  switch (p.kind) {
    case "off":
      return { title: "Requests are off", body: "This server doesn't take requests for movies and shows.", retry: false };
    case "noAccess":
      return { title: "Requests aren't open to this account", body: "An administrator can let it ask for movies and shows.", retry: false };
    case "tmdbDown":
      return { title: "TMDB isn't answering", body: "Try again in a moment.", retry: true };
    case "tmdbKey":
      return { title: "TMDB refused this server's key", body: manage ? "Check it in Settings → Server, on the web." : "An administrator needs to check it.", retry: false };
    case "signIn":
      return { title: SIGN_IN_TITLE, body: SIGN_IN_BODY, retry: false };
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
      return p.retryAt !== null ? `You can ask again ${timeUntil(p.retryAt, now)}.` : "It was declined.";
    case "limit": {
      if (p.retryAt !== null) return `You can ask for more ${timeUntil(p.retryAt, now)}.`;
      const limit = limits ? (kind === "movie" ? limits.movies_limit : limits.seasons_limit) : null;
      return limit ? `You can ask for at most ${pluralize(limit, kind === "movie" ? "movie" : "season")} per 7 days: pick fewer.` : "That's more than you can ask for at once: pick fewer.";
    }
    case "restricted":
      return p.text;
    case "conflict":
      return p.text;
    case "missing":
      return "TMDB doesn't know this title.";
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
  const t = text.trim();
  if (!t) return t;
  const s = t.charAt(0).toUpperCase() + t.slice(1);
  return /[.…!?]$/.test(s) ? s : `${s}.`;
}

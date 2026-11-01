// How requests stand and what they say (docs/API.md, Requests): the words
// every page uses, what a title's page offers, which seasons the picker
// ticks, and a title's badge after one of its requests changed.

import type {
  Asked,
  DiscoverDetail,
  DiscoverSeason,
  DiscoverTitle,
  MediaRequest,
  RequestAction,
  RequestDownload,
  RequestProblem,
  RequestStage,
  RequestStatus,
  RequestSummary,
  TitleKind,
} from "@/api/requests";
import type { Id, Timestamp } from "@/api/types";
import { pluralize } from "./format";

const nowSeconds = () => Date.now() / 1000;

export function kindLabel(kind: TitleKind): string {
  return kind === "movie" ? "Movie" : "Show";
}

/** A title as running text quotes it: “Heat”. */
export const quoted = (title: string) => `“${title}”`;

/** "Season 2", "Seasons 1–3, 5", "Seasons 1–2 and specials", "Specials"; "" for none (movies). */
export function seasonsLabel(seasons: number[] | null): string {
  if (!seasons?.length) return "";
  const numbers = [...new Set(seasons.filter((n) => n > 0))].sort((a, b) => a - b);
  const ranges: string[] = [];
  let start: number | null = null;
  let prev = 0;
  const close = () => {
    if (start !== null) ranges.push(start === prev ? `${start}` : `${start}–${prev}`);
  };
  for (const n of numbers) {
    if (start !== null && n === prev + 1) {
      prev = n;
      continue;
    }
    close();
    start = n;
    prev = n;
  }
  close();
  if (!ranges.length) return "Specials";
  const label = `${numbers.length === 1 ? "Season" : "Seasons"} ${ranges.join(", ")}`;
  return seasons.includes(0) ? `${label} and specials` : label;
}

/**
 * How long until `ts`, never sooner than it is: "in 5 min", "in 3 hours",
 * "in 12 days"; "soon" once it has passed. (Written here rather than by
 * Intl, which says "in 2 weeks" where a limit means 12 days.)
 */
export function untilText(ts: Timestamp, now = nowSeconds()): string {
  const diff = ts - now;
  if (diff <= 0) return "soon";
  if (diff < 3600) return `in ${Math.max(1, Math.ceil(diff / 60))} min`;
  if (diff < 86400) return `in ${pluralize(Math.round(diff / 3600), "hour")}`;
  return `in ${pluralize(Math.ceil(diff / 86400), "day")}`;
}

/** A day, "Nov 4" (with its year when it isn't this one); within the next 24 hours, the time: "today at 9:00 PM". */
export function dayText(ts: Timestamp, now = nowSeconds()): string {
  const date = new Date(ts * 1000);
  const today = new Date(now * 1000);
  if (ts > now && ts - now < 86400) {
    const time = date.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
    return `${date.toDateString() === today.toDateString() ? "today" : "tomorrow"} at ${time}`;
  }
  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: date.getFullYear() === today.getFullYear() ? undefined : "numeric",
  });
}

/** The badge on a title's poster; `null` for none (restricted titles, and those that may be asked for). */
export function titleBadge(title: Pick<DiscoverTitle, "status" | "item_id">): string | null {
  switch (title.status) {
    case "library":
      return "In library";
    case "partial":
      return title.item_id !== null ? "Partly in library" : "Requested";
    case "requested":
      return "Requested";
    case "declined":
      return "Declined";
    default:
      return null;
  }
}

/** A title's name for screen readers, with where it stands: "Heat, 1995, Movie, In library". */
export function titleName(title: Pick<DiscoverTitle, "title" | "year" | "kind" | "status" | "item_id">): string {
  return [title.title, title.year, kindLabel(title.kind), titleBadge(title)].filter(Boolean).join(", ");
}

/** Asked for and not settled: waiting for a decision, on its way, or failed. */
export function isActive(request: Pick<MediaRequest, "status" | "stage">): boolean {
  return (request.status === "pending" || request.status === "approved" || request.status === "failed") && request.stage !== "gone";
}

/**
 * How a title already shown in a row stands after one of its requests
 * changed (`null`: that request is gone for this account): an active one
 * makes it Requested, a declined one Declined (for those who made or
 * follow it), an available movie In library, and otherwise it has no
 * badge. An available show is at least partly in a library: whether all
 * of it is, only its details say. What a library has of it, and another
 * active request for it, stay.
 */
export function titleAfter(title: DiscoverTitle, requestId: Id, request: MediaRequest | null): DiscoverTitle {
  const settled = title.status === "library" || title.status === "restricted";
  if (request && isActive(request)) {
    const request_id = title.request_id ?? request.id;
    return settled ? { ...title, request_id } : { ...title, status: "requested", request_id };
  }
  const others = title.request_id !== null && title.request_id !== requestId;
  const request_id = others ? title.request_id : null;
  if (request?.status === "available" && request.stage !== "gone" && request.item_id !== null) {
    const status = request.kind === "movie" ? "library" : settled ? title.status : "partial";
    return { ...title, status, item_id: request.item_id, request_id };
  }
  if (others || settled) return { ...title, request_id };
  if (request?.status === "declined" && (request.mine || request.watching)) return { ...title, status: "declined", request_id };
  return { ...title, status: title.status === "partial" && title.item_id !== null ? "partial" : "none", request_id };
}

/** What's left of the limits, as the summary or a title's page has it. */
export type Allowance = Pick<RequestSummary, "movies_left" | "seasons_left" | "movies_next_at" | "seasons_next_at"> &
  Partial<Pick<RequestSummary, "movies_limit" | "seasons_limit">>;

/**
 * What the account may still ask for in the 7 days, and when it may ask
 * for more of what it used up, for one kind or both ("You can ask for 2
 * more movies and 5 more seasons."), with the limits ("Limits: 3 movies
 * and 10 seasons per 7 days."); `null` without a limit.
 */
export function allowanceText(allowance: Allowance, kind?: TitleKind, now = nowSeconds()): { text: string; limits: string | null } | null {
  const all = [
    { kind: "movie", one: "movie", many: "movies", left: allowance.movies_left, next: allowance.movies_next_at, limit: allowance.movies_limit ?? null },
    { kind: "show", one: "season", many: "seasons", left: allowance.seasons_left, next: allowance.seasons_next_at, limit: allowance.seasons_limit ?? null },
  ];
  const limited = all.filter((p) => (!kind || p.kind === kind) && p.left !== null);
  if (!limited.length) return null;
  const more = limited.filter((p) => (p.left ?? 0) > 0);
  const used = limited.filter((p) => (p.left ?? 0) <= 0);
  const sentences: string[] = [];
  if (more.length) sentences.push(`You can ask for ${more.map((p) => pluralize(p.left ?? 0, `more ${p.one}`, `more ${p.many}`)).join(" and ")}.`);
  const [only] = used;
  if (only && used.length === 1) {
    sentences.push(`You've asked for all the ${only.many} you can for now${only.next ? `: you can ask for more ${untilText(only.next, now)}` : ""}.`);
  } else if (used.length > 1) {
    const when = used.flatMap((p) => (p.next ? [`more ${p.many} ${untilText(p.next, now)}`] : []));
    sentences.push(`You've asked for all you can for now${when.length ? `: ${when.join(", ")}` : ""}.`);
  }
  const limits = limited.filter((p) => p.limit !== null);
  return {
    text: sentences.join(" "),
    limits: limits.length ? `Limits: ${limits.map((p) => pluralize(p.limit ?? 0, p.one, p.many)).join(" and ")} per 7 days.` : null,
  };
}

/** Nothing more of this kind may be asked for now. */
export function spent(allowance: Allowance, kind: TitleKind): boolean {
  return (kind === "movie" ? allowance.movies_left : allowance.seasons_left) === 0;
}

/** A season may be asked for: no library has it whole, no request asks for it, it wasn't declined, and the title isn't restricted. */
export function seasonAskable(season: DiscoverSeason, title: Pick<DiscoverDetail, "status">): boolean {
  return (season.state === "missing" || season.state === "partial" || season.state === "unaired") && !season.declined && title.status !== "restricted";
}

/** What the picker ticks to begin with, without a limit: the askable seasons but specials with aired episodes missing, lowest first. */
function missingSeasons(detail: Pick<DiscoverDetail, "status" | "seasons">): number[] {
  return (detail.seasons ?? [])
    .filter((s) => s.number !== 0 && (s.state === "missing" || s.state === "partial") && seasonAskable(s, detail))
    .map((s) => s.number)
    .sort((a, b) => a - b);
}

/** "Request more seasons" shows: a season but specials may be asked for, and lacks aired episodes. */
export function canAskMoreSeasons(detail: Pick<DiscoverDetail, "status" | "seasons">): boolean {
  return missingSeasons(detail).length > 0;
}

/** The picker's seasons: every one, specials last. */
export function pickerSeasons(seasons: DiscoverSeason[]): DiscoverSeason[] {
  return [...seasons].sort((a, b) => Number(a.number === 0) - Number(b.number === 0) || a.number - b.number);
}

/** The seasons the picker ticks as it opens: those {@link canAskMoreSeasons} counts, up to `left` of them. */
export function defaultPicks(detail: Pick<DiscoverDetail, "status" | "seasons">, left: number | null): number[] {
  const picks = missingSeasons(detail);
  return left === null ? picks : picks.slice(0, Math.max(0, left));
}

/** The picker's button: "Request 3 seasons", "Request 1 season". */
export function pickerButton(count: number): string {
  return `Request ${pluralize(count, "season")}`;
}

/** How a season stands for this account, in the seasons' list and the picker; `null` when it may simply be asked for. */
export function seasonStateText(season: DiscoverSeason, now = nowSeconds()): string | null {
  if (season.declined && season.state !== "library" && season.state !== "requested") return "Declined";
  switch (season.state) {
    case "library":
      return "In library";
    case "partial":
      return "Partly in library";
    case "requested":
      return "Requested";
    case "unaired": {
      const day = season.air_date ? Date.parse(`${season.air_date}T12:00:00Z`) / 1000 : NaN;
      return Number.isFinite(day) && day > now ? `Airs ${dayText(day, now)}` : "Not aired yet";
    }
    default:
      return null;
  }
}

/** The one thing a title's page offers, the first that applies (docs/API.md, Discover). */
export type MainAction =
  /** This sign-in can't ask: it says so, with Sign in again. */
  | { kind: "sign_in" }
  /** Above the account's age limit: no button, the reason. */
  | { kind: "restricted"; text: string }
  /** In a library: Open. */
  | { kind: "open"; itemId: Id | null }
  /** Declined in the last 30 days: no button, when it may be asked for again; `more`: other seasons may still be asked for. */
  | { kind: "declined"; text: string; more: boolean }
  /** This account asked for it: Requested (inert), with its stage. */
  | { kind: "requested"; request: MediaRequest | null }
  /** This account follows the request: Following (inert), with its stage, and Stop following. */
  | { kind: "following"; request: MediaRequest | null }
  /** Someone else asked for it: Tell me too (`seasons`: those a show's requests cover). */
  | { kind: "tell_me_too"; seasons: number[] | null }
  /** Partly in a library, or partly asked for: Open (with `itemId`), and Request more seasons. */
  | { kind: "partial"; itemId: Id | null; more: boolean }
  /** It may be asked for: Request. */
  | { kind: "request" };

/** "Declined: “Not on this server”. You can ask again in 12 days." */
export function declinedText(declined: DiscoverDetail["declined"], now = nowSeconds()): string {
  if (!declined) return "Declined.";
  const reason = declined.reason ? `: ${quoted(declined.reason)}` : "";
  return `Declined${reason}. You can ask again ${untilText(declined.again_at, now)}.`;
}

export function mainAction(detail: DiscoverDetail, summary: Pick<RequestSummary, "sign_in_again">, now = nowSeconds()): MainAction {
  if (summary.sign_in_again === "requests") return { kind: "sign_in" };
  switch (detail.status) {
    case "restricted":
      return { kind: "restricted", text: detail.restriction ?? "Above this account's age limit." };
    case "library":
      return { kind: "open", itemId: detail.item_id };
    case "declined":
      return { kind: "declined", text: declinedText(detail.declined, now), more: canAskMoreSeasons(detail) };
    case "requested": {
      const active = detail.requests.filter(isActive);
      const mine = active.find((r) => r.mine);
      if (mine) return { kind: "requested", request: mine };
      const followed = active.find((r) => r.watching);
      if (followed || detail.watching) return { kind: "following", request: followed ?? null };
      const seasons = detail.seasons ? detail.seasons.filter((s) => s.state === "requested").map((s) => s.number) : null;
      return { kind: "tell_me_too", seasons };
    }
    case "partial":
      return { kind: "partial", itemId: detail.item_id, more: canAskMoreSeasons(detail) };
    default:
      return { kind: "request" };
  }
}

/** A stage pill's colour: waiting for a decision amber, on its way blue, ready green, declined and failed red, gone gray. */
export type StageTone = "warning" | "info" | "success" | "danger" | "neutral";

const STAGE_TONES: Record<RequestStage, StageTone> = {
  pending: "warning",
  approved: "info",
  waiting: "info",
  searching: "info",
  downloading: "info",
  importing: "info",
  partial: "success",
  available: "success",
  gone: "neutral",
  declined: "danger",
  failed: "danger",
};

export interface StageWords {
  /** The pill's words ("Downloading 42% · in 20 min"). */
  label: string;
  tone: StageTone;
  /** What more there is to say, under it. */
  line: string | null;
  /** Shows a progress bar. */
  download: RequestDownload | null;
}

/** "Downloading 42% · in 20 min", or what the queue says instead ("Download paused"). */
export function downloadText(download: RequestDownload | null, now = nowSeconds()): string {
  if (!download) return "Downloading";
  switch (download.state) {
    case "queued":
      return "Waiting to download";
    case "paused":
      return "Download paused";
    case "warning":
      return "The download has a problem";
    case "importing":
      return "Adding to the library";
    default: {
      const eta = download.eta && download.eta > now ? ` · ${untilText(download.eta, now)}` : "";
      return `Downloading ${Math.round(download.progress * 100)}%${eta}`;
    }
  }
}

/**
 * A request's stage pill. A failed one tells managers why, under it (the
 * service's own words, often long); others hear that an administrator
 * will look at it. Both hear when it's tried again.
 */
export function stageWords(request: MediaRequest, manage: boolean, now = nowSeconds()): StageWords {
  let label: string;
  let line: string | null = null;
  switch (request.stage) {
    case "pending":
      label = "Waiting for approval";
      break;
    case "approved":
      label = "Approved";
      break;
    case "waiting":
      label = request.stage_at ? `${request.kind === "movie" ? "Out" : "Airs"} ${dayText(request.stage_at, now)}` : "Not out yet";
      break;
    case "searching":
      label = "Searching";
      break;
    case "downloading":
      label = downloadText(request.download, now);
      break;
    case "importing":
      label = "Adding to the library";
      break;
    case "partial":
      label = request.available_seasons.length ? `${seasonsLabel(request.available_seasons)} ready` : "Partly ready";
      break;
    case "available":
      label = "Ready to watch";
      break;
    case "gone":
      label = "No longer in a library";
      break;
    case "declined":
      label = "Declined";
      break;
    case "failed": {
      const reason = manage ? request.reason : null;
      label = manage ? "Couldn't send" : "Not sent yet";
      line = manage ? reason : "It couldn't be sent to download yet: an administrator will look at it.";
      if (request.retry_at) line = [reason && sentence(reason), `Trying again ${untilText(request.retry_at, now)}.`].filter(Boolean).join(" ");
      break;
    }
  }
  const download = request.stage === "downloading" || request.stage === "importing" || request.stage === "partial" ? request.download : null;
  return { label, tone: STAGE_TONES[request.stage], line, download };
}

/** What asking did, for a notice. */
export function askedText({ request, following }: Asked): string {
  const title = quoted(request.title);
  if (following) return `You'll hear when ${title} is ready.`;
  if (request.status === "pending") return `Asked for ${title}: an administrator decides.`;
  if (request.status === "failed") return `Asked for ${title}, but it couldn't be sent to download yet.`;
  return `${title} is approved and on its way.`;
}

/** An action's button. */
export function actionLabel(action: RequestAction, request: Pick<MediaRequest, "status">): string {
  switch (action) {
    case "withdraw":
      return request.status === "declined" ? "Remove" : "Withdraw";
    case "unwatch":
      return "Stop following";
    case "approve":
      return "Approve";
    case "retry":
      return "Try again";
    case "decline":
      return "Decline";
    case "handled":
      return "Mark as added by hand";
    case "delete":
      return "Delete";
    case "ask_again":
      return "Request again";
  }
}

/** What's asked before an action that can't be taken back, as a dialog's title; `null` for one that needs no asking. */
export function actionQuestion(action: RequestAction, request: Pick<MediaRequest, "status" | "title">): string | null {
  switch (action) {
    case "withdraw":
      return request.status === "declined"
        ? `Remove ${quoted(request.title)} from your requests?`
        : `Withdraw your request for ${quoted(request.title)}?`;
    case "delete":
      return "Delete this request?";
    default:
      return null;
  }
}

/** "“Heat” is ready to watch.", "Season 2 of “Dark” is ready to watch.", "Seasons 1–2 of “Dark” are ready to watch." */
export function readyText(title: string, seasons: number[] | null, complete: boolean): string {
  if (complete || !seasons?.length) return `${quoted(title)} is ready to watch.`;
  const many = seasons.length > 1 || seasons[0] === 0;
  return `${seasonsLabel(seasons)} of ${quoted(title)} ${many ? "are" : "is"} ready to watch.`;
}

/** "“Heat” was approved.", "“Heat” was declined: “Not here”." */
export function decidedText(title: string, status: "approved" | "declined", reason: string | null): string {
  if (status === "approved") return `${quoted(title)} was approved.`;
  return reason ? `${quoted(title)} was declined: ${quoted(reason)}.` : `${quoted(title)} was declined.`;
}

/** Managers: "Ann asked for “Dark” (Seasons 1–3)." */
export function createdText(name: string, title: string, seasons: number[] | null): string {
  return `${name} asked for ${quoted(title)}${seasons?.length ? ` (${seasonsLabel(seasons)})` : ""}.`;
}

/** Managers: "“Heat” couldn't be sent to Radarr: {reason}." */
export function failedText(title: string, kind: TitleKind, reason: string | null): string {
  const to = `${quoted(title)} couldn't be sent to ${kind === "movie" ? "Radarr" : "Sonarr"}`;
  return reason ? `${to}: ${sentence(reason)}` : `${to}.`;
}

/**
 * What an unseen request's news was, as a notice says it: all of it ready
 * to watch, some of a show's seasons (while others still download, too),
 * or declined; `null` for none to tell.
 */
export function newsText(request: MediaRequest): string | null {
  switch (request.status) {
    case "available":
      return request.stage === "gone" ? null : readyText(request.title, null, true);
    case "approved":
      return request.available_seasons.length ? readyText(request.title, request.available_seasons, false) : null;
    case "declined":
      return decidedText(request.title, "declined", request.reason);
    default:
      return null;
  }
}

/** "3 of your requests have news". */
export function newsCountText(count: number): string {
  return `${count} of your requests ${count === 1 ? "has" : "have"} news`;
}

/** How a request stood when a notice told of it: its status, and the seasons of a show ready then. */
export interface ToldNews {
  status: RequestStatus;
  seasons: number[];
}

/** What this profile's notices have told already, so nothing is announced twice. */
export interface NewsMark {
  /** The newest `updated_at` announced from a catch-up (the server's clock). */
  at: Timestamp;
  /** Requests told of, as their news came or by a catch-up: told again once they move on, or more of a show's seasons are ready. */
  told: Record<string, ToldNews>;
}

export const NO_NEWS: NewsMark = { at: 0, told: {} };

const TOLD_KEPT = 100;

/** `request`'s news was told: its status is the same and, while it is partly ready, no season of it is ready that wasn't then. */
function toldAlready(told: ToldNews | undefined, request: MediaRequest): boolean {
  if (told?.status !== request.status) return false;
  return request.status !== "approved" || request.available_seasons.every((n) => told.seasons.includes(n));
}

/** Of the unseen requests, those whose news no notice has told. */
export function freshNews(requests: MediaRequest[], mark: NewsMark): MediaRequest[] {
  return requests.filter((r) => r.updated_at > mark.at && newsText(r) !== null && !toldAlready(mark.told[r.id], r));
}

/** The mark once `requests` were announced by a catch-up. */
export function markAnnounced(mark: NewsMark, requests: MediaRequest[]): NewsMark {
  return requests.reduce((m, r) => markTold({ ...m, at: Math.max(m.at, r.updated_at) }, r.id, r.status, r.available_seasons), mark);
}

/** The mark once a notice told of a request: its status now, and the seasons of a show ready, with those told before. */
export function markTold(mark: NewsMark, id: Id, status: RequestStatus, seasons: number[]): NewsMark {
  const before = mark.told[id]?.seasons ?? [];
  const all: Record<string, ToldNews> = { ...mark.told, [id]: { status, seasons: [...new Set([...before, ...seasons])].sort((a, b) => a - b) } };
  // The newest requests' (the highest ids): older ones' news was told, and seen, long ago.
  const kept = Object.entries(all)
    .sort(([a], [b]) => Number(b) - Number(a))
    .slice(0, TOLD_KEPT);
  return { ...mark, told: Object.fromEntries(kept) };
}

/** The number on the entry that opens requests: managers' include what waits for them, in the danger colour while some failed. */
export function requestsBadge(summary: RequestSummary | undefined): { count: number; danger: boolean } {
  if (!summary?.enabled || summary.sign_in_again === "requests") return { count: 0, danger: false };
  if (!summary.manage) return { count: summary.unseen, danger: false };
  return { count: summary.pending + summary.failed + summary.unseen, danger: summary.failed > 0 };
}

/** What a problem with requests says: a heading, a sentence under it, and whether Retry may help. */
export interface ProblemWords {
  title: string;
  text: string | null;
  retry: boolean;
  /** Managers: where it's fixed (Settings → Server). */
  settings: boolean;
}

export function problemWords(
  problem: RequestProblem,
  context: { manage: boolean; kind?: TitleKind; limits?: Pick<RequestSummary, "movies_limit" | "seasons_limit"> },
  now = nowSeconds(),
): ProblemWords {
  const words = (title: string, text: string | null = null, more: Partial<ProblemWords> = {}): ProblemWords => ({
    title,
    text,
    retry: false,
    settings: false,
    ...more,
  });
  switch (problem.kind) {
    case "off":
      return words("Requests are off", "This server doesn't take requests for movies and shows.");
    case "tmdb_key":
      return context.manage
        ? words("TMDB refused this server's key", "Check it in Settings → Server.", { settings: true })
        : words("TMDB refused this server's key", "An administrator needs to check it.");
    case "tmdb_down":
      return words("TMDB isn't answering", "Try again in a moment.", { retry: true });
    case "no_access":
      return words("Requests aren't open to this account", "An administrator can let it ask for movies and shows.");
    case "sign_in":
      return words("Sign in again to ask for movies and shows", "This app signed in before it could ask. Signing in again lets it.");
    case "restricted":
      return words(capitalize(problem.message));
    case "declined":
      return words(problem.retry_at ? `You can ask again ${untilText(problem.retry_at, now)}.` : "It was declined.");
    case "too_many": {
      if (problem.retry_at) return words(`You can ask for more ${untilText(problem.retry_at, now)}.`);
      const movies = context.kind === "movie";
      const limit = movies ? context.limits?.movies_limit : context.limits?.seasons_limit;
      return words(
        limit != null
          ? `You can ask for at most ${pluralize(limit, movies ? "movie" : "season", movies ? "movies" : "seasons")} per 7 days: pick fewer.`
          : "That's more than the limit allows: pick fewer.",
      );
    }
    case "conflict": {
      if (problem.retry_at === null) return words(capitalize(problem.message));
      // Its message says when as a date, in the server's words: said here from `retry_at` instead.
      const when = untilText(problem.retry_at, now);
      return words(problem.request?.status === "declined" ? `You can remove it ${when}.` : `You can try again ${when}.`);
    }
  }
}

/** A problem as one line, for a notice. */
export function problemText(...args: Parameters<typeof problemWords>): string {
  const { title, text } = problemWords(...args);
  return text ? `${sentence(title)} ${text}` : title;
}

function capitalize(text: string): string {
  return text ? text.charAt(0).toUpperCase() + text.slice(1) : text;
}

/** Words that end as a sentence does, with a full stop if they had none. */
function sentence(text: string): string {
  return /[.!?]$/.test(text) ? text : `${text}.`;
}

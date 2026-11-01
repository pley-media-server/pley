// Requests (docs/API.md): movies and shows found on TMDB, asked for or
// followed, decided on by a manager, sent to Radarr or Sonarr, and seen
// through to a library.

import { useEffect } from "react";
import {
  queryOptions,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
  type InfiniteData,
  type QueryClient,
  type UseInfiniteQueryResult,
} from "@tanstack/react-query";
import { eventsLive } from "@/lib/connection";
import { titleAfter } from "@/lib/requests";
import { api, ApiError, qs, requestWithStatus } from "./client";
import type { Id, RequestAccess, Timestamp } from "./types";

export type { RequestAccess } from "./types";

export type TitleKind = "movie" | "show";
/** The decision. */
export type RequestStatus = "pending" | "approved" | "declined" | "failed" | "available";
/** Where a request is on its way to a library. */
export type RequestStage =
  /** Waits for an administrator. */
  | "pending"
  | "declined"
  /** Couldn't be sent to Radarr or Sonarr (see `retry_at`). */
  | "failed"
  /** Approved, not sent: no Radarr or Sonarr for its kind, being sent, or added by hand. */
  | "approved"
  /** Sent; not out yet (`stage_at`). */
  | "waiting"
  /** Sent and out, nothing downloading yet. */
  | "searching"
  /** See `download`. */
  | "downloading"
  /** Downloaded; going into the library. */
  | "importing"
  /** Shows: some of the seasons asked for are ready (`available_seasons`). */
  | "partial"
  /** Ready to watch, in a library its maker can open. */
  | "available"
  /** Was available, and no library has it any more. */
  | "gone";
/** What this session may do to a request now, in the order it lists them. */
export type RequestAction = "withdraw" | "unwatch" | "approve" | "retry" | "decline" | "handled" | "delete" | "ask_again";
/** How a title stands for this account: the first of these that holds (docs/API.md, Discover). */
export type TitleStatus = "library" | "partial" | "requested" | "declined" | "restricted" | "none";
/** How a season stands for this account, by the server's rule for a whole season. */
export type SeasonState = "library" | "partial" | "requested" | "missing" | "unaired";
export type DiscoverList = "trending" | "movies" | "shows" | "upcoming" | "anime";
export type ArrService = "radarr" | "sonarr";

export interface RequestSummary {
  /** Requests are on (and TMDB has a key). */
  enabled: boolean;
  /** What asking from this session does. */
  access: RequestAccess;
  /** Movies it may still ask for in the last 7 days; `null`: no limit. */
  movies_left: number | null;
  seasons_left: number | null;
  /** The settings' limits per 7 days (`null`: none). */
  movies_limit: number | null;
  seasons_limit: number | null;
  /** None left: when one more may be asked for. */
  movies_next_at: Timestamp | null;
  seasons_next_at: Timestamp | null;
  /** This session approves and declines, and sees everyone's requests. */
  manage: boolean;
  /** Managers: requests waiting for a decision (others: 0). */
  pending: number;
  /** Managers: failed requests the server won't try again by itself (others: 0). */
  failed: number;
  /** Requests this account made or follows that became available, partly available or declined since it last looked. */
  unseen: number;
  /** Requests this account made or follows, whatever their status. */
  own: number;
  /** The newest change among the requests this session may see. */
  changed_at: Timestamp | null;
  /** This sign-in can't do something it could after signing in again (never for the web's own session). */
  sign_in_again: "requests" | "approvals" | null;
}

export interface DiscoverTitle {
  kind: TitleKind;
  tmdb_id: number;
  title: string;
  year: number | null;
  overview: string;
  /** Through the server (`/api/discover/images/poster/…`), sized as library artwork is. */
  poster_url: string | null;
  backdrop_url: string | null;
  /** TMDB's users', 0–10; `null` with too few votes. */
  rating: number | null;
  status: TitleStatus;
  /** In a library this account can open: the title. */
  item_id: Id | null;
  /** An active request for it this account made or follows (any, for managers), whatever `status` says. */
  request_id: Id | null;
}

export interface DiscoverPage {
  page: number;
  /** 0 to 500. */
  total_pages: number;
  /** Up to 20, fewer (even none) where titles were left out: not the end while `page < total_pages`. */
  results: DiscoverTitle[];
}

export interface DiscoverSeason {
  /** 0: specials. */
  number: number;
  name: string;
  /** Episodes TMDB lists. */
  episodes: number;
  /** Of those, the ones aired by now. */
  aired: number;
  /** Its first episode's ("2024-05-01"). */
  air_date: string | null;
  poster_url: string | null;
  /** Episodes of it in libraries this account can open. */
  in_library: number;
  /** An active request asks for it. */
  requested: boolean;
  /** For this account: drawn from this, never from the counts. */
  state: SeasonState;
  /** A request for it this account made or followed was declined in the last 30 days. */
  declined: boolean;
  /** The active request asking for it, where this account made or follows it (any, for managers). */
  request_id: Id | null;
}

export interface CastMember {
  name: string;
  character: string | null;
  photo_url: string | null;
}

export interface DiscoverDetail extends DiscoverTitle {
  /** Where it differs from `title`. */
  original_title: string | null;
  tagline: string | null;
  genres: string[];
  /** A movie's release, a show's first air date ("2024-05-01"). */
  release_date: string | null;
  /** Movies. */
  runtime_minutes: number | null;
  /** "PG-13", "TV-MA": the server's region's. */
  certification: string | null;
  /** A show's first network, a movie's first studio. */
  network: string | null;
  /** Shows: TMDB's ("Returning Series", "Ended"). */
  show_status: string | null;
  /** Requests for it go to anime's folder and profile. */
  anime: boolean;
  /** 12 at most, in billing order. */
  cast: CastMember[];
  /** Shows, in TMDB's order (specials first where there are some); `null` for movies. */
  seasons: DiscoverSeason[] | null;
  imdb_id: string | null;
  /** This account's (or one it followed), within its 30 days: for shows, of the declined seasons. `reason` is `null` for a follower. */
  declined: { reason: string | null; at: Timestamp; again_at: Timestamp } | null;
  /** Why it can't be asked for ("Rated R: above this account's age limit"). */
  restriction: string | null;
  /** This account follows an active request for it. */
  watching: boolean;
  /** This account's and those it follows (everyone's, for managers), newest first. */
  requests: MediaRequest[];
  /** As the summary has them. */
  movies_left: number | null;
  seasons_left: number | null;
  movies_next_at: Timestamp | null;
  seasons_next_at: Timestamp | null;
}

export interface RequestUser {
  id: Id;
  name: string;
  avatar_url: string | null;
  avatar_color: string | null;
}

export interface RequestDownload {
  /** 0–1, of everything of it queued. */
  progress: number;
  state: "queued" | "downloading" | "paused" | "importing" | "warning";
  /** The queue's estimate, where it has one. */
  eta: Timestamp | null;
  /** Shows: episodes queued (movies: 1). */
  episodes: number;
}

export interface MediaRequest {
  id: Id;
  kind: TitleKind;
  tmdb_id: number;
  /** As TMDB had it when asked. */
  title: string;
  year: number | null;
  poster_url: string | null;
  backdrop_url: string | null;
  /** Shows: the seasons asked for; `null` for movies. */
  seasons: number[] | null;
  /** Shows: those ready. */
  available_seasons: number[];
  anime: boolean;
  status: RequestStatus;
  stage: RequestStage;
  /** Waiting: when it's out (an episode's airing; a release known only by its day comes at 12:00 UTC). */
  stage_at: Timestamp | null;
  /** Declined: the administrator's (`null` for followers); failed: what went wrong, for managers only. */
  reason: string | null;
  /** Managers: what to know ("Added by hand", "Sonarr numbers these seasons differently"). */
  note: string | null;
  /** This account made it. */
  mine: boolean;
  /** This account follows it. */
  watching: boolean;
  /** Managers: how many follow it (others: 0). */
  watchers: number;
  /** For its maker or a follower: it became available, partly available or declined since the account last looked. */
  unseen: boolean;
  actions: RequestAction[];
  /** `null` for someone else's request seen by someone who doesn't manage them. */
  requested_by: RequestUser | null;
  /** Managers and the maker see it. */
  decided_by: RequestUser | null;
  decided_at: Timestamp | null;
  created_at: Timestamp;
  updated_at: Timestamp;
  available_at: Timestamp | null;
  /** Failed: when the server tries again by itself; `null`: it won't. */
  retry_at: Timestamp | null;
  /** Managers: where it went, with the profile's name. */
  sent_to: { service: ArrService; quality_profile: string | null; root_folder: string | null } | null;
  /** The library's title once there, where this account can open it. */
  item_id: Id | null;
  download: RequestDownload | null;
}

export interface RequestList {
  total: number;
  /** Each status's requests under every filter but `status`. */
  counts: Record<RequestStatus, number>;
  requests: MediaRequest[];
  /** Managers: everyone with a request under every filter but the person, by name (others: none). */
  people: RequestUser[];
}

/** Which requests a list shows (`GET /api/requests`). */
export interface RequestFilter {
  /** One status or several; none: all of them. */
  status: RequestStatus[];
  /** Managers: pending requests, and failed ones the server won't try again by itself. */
  attention: boolean;
  kind: TitleKind | null;
  /** Managers: one person's. */
  userId: Id | null;
  /** Titles containing this. */
  q: string;
  /** By when they were asked for. */
  order: "newest" | "oldest";
  /** Only this account's and those it follows (others only ever see those). */
  mine: boolean;
  /** Only unseen ones, newest news first. */
  unseen: boolean;
}

/** Every request this session may see, newest first. */
export const EVERY_REQUEST: RequestFilter = {
  status: [],
  attention: false,
  kind: null,
  userId: null,
  q: "",
  order: "newest",
  mine: false,
  unseen: false,
};

export function requestFilter(filter: Partial<RequestFilter>): RequestFilter {
  return { ...EVERY_REQUEST, ...filter };
}

/** What a library title's page shows of requests (`GET /api/items/{id}/request`). */
export interface ItemRequest {
  kind: TitleKind;
  tmdb_id: number;
  status: TitleStatus;
  /** The newest active request for it this account made or follows (any, for managers). */
  request: MediaRequest | null;
  /** The seasons the picker would tick without a limit (`[]` for movies); `null` while the server has no details from TMDB. */
  askable: number[] | null;
}

export interface NewRequest {
  kind: TitleKind;
  tmdb_id: number;
  /** Shows: TMDB's season numbers; left out, the aired ones the libraries lack. */
  seasons?: number[];
}

/** What asking did: made a request (201), or follows one there already (200). */
export interface Asked {
  request: MediaRequest;
  following: boolean;
}

/** Where approving sends a request, instead of the settings' choices. */
export interface Approval {
  /** A profile's id, of `ArrChoices.quality_profiles`. */
  quality_profile?: number;
  root_folder?: string;
  /** Only these of the seasons asked for: the others leave the request. */
  seasons?: number[];
}

/** A connected Radarr's or Sonarr's profiles and folders, with the settings' choices. */
export interface ArrChoices {
  quality_profiles: { id: number; name: string }[];
  root_folders: { path: string; free_bytes: number | null }[];
  quality_profile: number | null;
  root_folder: string | null;
  anime_quality_profile: number | null;
  anime_root_folder: string | null;
  /** It didn't answer ("Radarr isn't answering"): the lists are the last ones known, maybe none. */
  error: string | null;
}

/** What approving may choose from (`GET /api/requests/options`): `null` where that service isn't connected. */
export interface ApprovalOptions {
  radarr: ArrChoices | null;
  sonarr: ArrChoices | null;
}

/** What Radarr or Sonarr offers requests (`POST /api/settings/arr`, Settings → Requests). */
export interface ArrOptions {
  version: string;
  quality_profiles: { id: number; name: string }[];
  root_folders: { path: string; free_bytes: number | null }[];
  /** Sonarr 3 only; empty otherwise. */
  language_profiles: { id: number; name: string }[];
}

/** An address and key typed in (not saved yet); the saved ones where left out. */
export interface ArrConnection {
  service: ArrService;
  url?: string;
  api_key?: string;
}

export const requestKeys = {
  /** All of them. */
  all: ["requests"] as const,
  summary: ["requests", "summary"] as const,
  /** TMDB's lists and searches: request events change their badges, never fetch them again. */
  discovers: ["requests", "discover"] as const,
  discover: (list: DiscoverList) => ["requests", "discover", list] as const,
  searches: ["requests", "search"] as const,
  search: (q: string, missing: boolean) => ["requests", "search", q, missing] as const,
  titles: ["requests", "title"] as const,
  title: (kind: TitleKind, tmdbId: number) => ["requests", "title", kind, tmdbId] as const,
  lists: ["requests", "list"] as const,
  list: (filter: RequestFilter, limit: number) => ["requests", "list", filter, limit] as const,
  ones: ["requests", "one"] as const,
  one: (id: Id) => ["requests", "one", id] as const,
  /** Library titles' pages. */
  items: ["requests", "item"] as const,
  item: (itemId: Id) => ["requests", "item", itemId] as const,
  options: ["requests", "options"] as const,
  /** With the settings: what the saved Radarr or Sonarr offers. */
  arr: (service: ArrService) => ["settings", "arr", service] as const,
};

/** Why something about requests didn't work, from an answer's error (docs/API.md, Requests, Errors). */
export type RequestProblem =
  /** Requests are off (503 `unavailable`). */
  | { kind: "off" }
  /** TMDB refuses the server's key (503 `tmdb_key`): trying again won't help. */
  | { kind: "tmdb_key" }
  /** TMDB isn't answering (503 `tmdb_down`). */
  | { kind: "tmdb_down" }
  /** This account may not ask (403 `no_access`). */
  | { kind: "no_access" }
  /** This sign-in was made without the scope (403 `insufficient_scope`). */
  | { kind: "sign_in" }
  /** Above this account's age limit (403 `restricted`): the message says so. */
  | { kind: "restricted"; message: string }
  /** Declined for this account in the last 30 days (409 `declined`): it may ask again at `retry_at`. */
  | { kind: "declined"; retry_at: Timestamp | null }
  /** Past the limit (429): more at `retry_at`; `null`: more was asked for at once than the limit allows. */
  | { kind: "too_many"; retry_at: Timestamp | null }
  /**
   * In a library already, or the request changed meanwhile (409): `request` as it is now, where the answer has
   * it; `retry_at`: when what was asked may be done (a declined request removed too early), else `null`.
   */
  | { kind: "conflict"; message: string; request: MediaRequest | null; retry_at: Timestamp | null };

function isRequest(value: unknown): value is MediaRequest {
  return !!value && typeof value === "object" && typeof (value as { id?: unknown }).id === "number" && "stage" in value;
}

/**
 * What an error says about requests; `null` for anything else (the server
 * away, a bug). A 503 with pley's own words is an answer, not the server
 * being away: it is never asked again by itself.
 */
export function requestProblem(err: unknown): RequestProblem | null {
  if (!(err instanceof ApiError)) return null;
  const body = (err.body ?? {}) as { code?: unknown; setup?: unknown; retry_at?: unknown; request?: unknown };
  // Without pley's own words (a proxy's page), the server didn't answer.
  if (typeof body.code !== "string") return null;
  const at = typeof body.retry_at === "number" ? body.retry_at : null;
  switch (err.status) {
    case 503:
      if (err.code === "tmdb_key") return { kind: "tmdb_key" };
      if (err.code === "tmdb_down" || body.setup === true) return { kind: "tmdb_down" };
      return { kind: "off" };
    case 403:
      if (err.code === "insufficient_scope") return { kind: "sign_in" };
      if (err.code === "restricted") return { kind: "restricted", message: err.message };
      return { kind: "no_access" };
    case 409:
      if (err.code === "declined") return { kind: "declined", retry_at: at };
      return { kind: "conflict", message: err.message, request: isRequest(body.request) ? body.request : null, retry_at: at };
    case 429:
      return { kind: "too_many", retry_at: at };
    default:
      return null;
  }
}

/** Only failures the server didn't answer are tried again: its refusals, and its own 503s, are answers. */
function retryRequest(count: number, err: unknown): boolean {
  if (err instanceof ApiError && err.status >= 400 && err.status < 500) return false;
  if (requestProblem(err)) return false;
  return count < 2;
}

/** Going into a library: its progress comes again every 10 s (no event says it moved). */
export function progressing(request: Pick<MediaRequest, "stage">): boolean {
  return request.stage === "downloading" || request.stage === "importing";
}

/** The longest search the server takes, in characters (longer ones it refuses). */
export const SEARCH_MAX = 200;

/** What's typed as the server takes it: trimmed, and cut to {@link SEARCH_MAX} characters (not halfway through one). */
export function searchText(q: string): string {
  return Array.from(q.trim()).slice(0, SEARCH_MAX).join("").trim();
}

const PROGRESS_MS = 10_000;
/** Without the event stream: how often what shows requests asks again. */
const QUIET_MS = 30_000;
const LIST_PAGE = 50;
const MAX_PAGES = 500;

function pollEvery(requests: Pick<MediaRequest, "stage">[]): number | false {
  if (requests.some(progressing)) return PROGRESS_MS;
  return requests.length && !eventsLive() ? QUIET_MS : false;
}

export const requestQueries = {
  summary: () =>
    queryOptions({
      queryKey: requestKeys.summary,
      queryFn: () => api.get<RequestSummary>("/api/requests/summary"),
      staleTime: 60_000,
      // Without the event stream nothing says it changed: asked every minute while the page is in front.
      refetchInterval: () => (eventsLive() ? false : 60_000),
      retry: retryRequest,
    }),
  title: (kind: TitleKind, tmdbId: number) =>
    queryOptions({
      queryKey: requestKeys.title(kind, tmdbId),
      queryFn: ({ signal }) => api.get<DiscoverDetail>(`/api/discover/${kind}/${tmdbId}`, { signal }),
      refetchInterval: (query) => pollEvery(query.state.data?.requests ?? []),
      retry: retryRequest,
    }),
  one: (id: Id) =>
    queryOptions({
      queryKey: requestKeys.one(id),
      queryFn: ({ signal }) => api.get<MediaRequest>(`/api/requests/${id}`, { signal }),
      retry: retryRequest,
    }),
};

/** This account may look for titles and ask for them. */
export function mayRequest(summary: RequestSummary | undefined): boolean {
  return !!summary?.enabled && summary.access !== "off" && summary.sign_in_again !== "requests";
}

/** This account has its requests to look at: it may ask, or asked before. */
export function hasRequests(summary: RequestSummary | undefined): boolean {
  return !!summary?.enabled && summary.sign_in_again !== "requests" && (summary.access !== "off" || summary.own > 0);
}

/**
 * Whether this account may ask, how much more, and what waits for it.
 * Servers without requests answer 404: no `data`, as when they're off.
 */
export function useRequestSummary(enabled = true) {
  return useQuery({ ...requestQueries.summary(), enabled });
}

function nextPage(last: DiscoverPage): number | undefined {
  return last.page < Math.min(last.total_pages, MAX_PAGES) ? last.page + 1 : undefined;
}

/** Pages of titles as one list, each once (a title can come again on a later page). */
export function uniqueTitles(pages: { results: DiscoverTitle[] }[] | undefined): DiscoverTitle[] {
  const seen = new Set<string>();
  const out: DiscoverTitle[] = [];
  for (const page of pages ?? []) {
    for (const t of page.results) {
      const key = `${t.kind}-${t.tmdb_id}`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push(t);
    }
  }
  return out;
}

/**
 * Loads a list's next pages until it holds `want` titles, it ends, or
 * `most` pages are in: a page comes short, even empty, where titles were
 * left out (above an age limit), and that isn't the list's end.
 */
export function useFillPages(query: UseInfiniteQueryResult<InfiniteData<DiscoverPage>, unknown>, want: number, most = 5) {
  const pages = query.data?.pages.length ?? 0;
  const short = !!query.data && uniqueTitles(query.data.pages).length < want;
  const go = short && query.hasNextPage && !query.isFetchingNextPage && !query.isError && pages < most;
  const { fetchNextPage } = query;
  useEffect(() => {
    if (go) void fetchNextPage();
  }, [go, pages, fetchNextPage]);
}

/** One of TMDB's lists, a page at a time. */
export function useDiscoverList(list: DiscoverList, enabled = true) {
  return useInfiniteQuery({
    queryKey: requestKeys.discover(list),
    queryFn: ({ pageParam, signal }) =>
      api.get<DiscoverPage>(`/api/discover/${list}${qs({ page: pageParam > 1 ? pageParam : null })}`, { signal }),
    initialPageParam: 1,
    getNextPageParam: nextPage,
    enabled,
    retry: retryRequest,
  });
}

/**
 * Movies and shows on TMDB matching `q` (`missing`: leaving out those a
 * library has, for the libraries' search); the last answer stays while
 * the next one comes.
 */
export function useDiscoverSearch(q: string, { enabled = true, missing = false }: { enabled?: boolean; missing?: boolean } = {}) {
  const trimmed = searchText(q);
  return useInfiniteQuery({
    queryKey: requestKeys.search(trimmed, missing),
    queryFn: ({ pageParam, signal }) =>
      api.get<DiscoverPage>(
        `/api/discover/search${qs({ q: trimmed, page: pageParam > 1 ? pageParam : null, missing: missing ? 1 : null })}`,
        { signal },
      ),
    initialPageParam: 1,
    getNextPageParam: nextPage,
    enabled: enabled && trimmed.length > 0,
    placeholderData: (prev) => prev,
    retry: retryRequest,
  });
}

/** A movie or show as TMDB has it, with what this account (or anyone, for managers) asked of it. */
export function useDiscoverTitle(kind: TitleKind, tmdbId: number, enabled = true) {
  return useQuery({ ...requestQueries.title(kind, tmdbId), enabled: enabled && Number.isInteger(tmdbId) && tmdbId > 0 });
}

function listPath(filter: RequestFilter, offset: number, limit: number): string {
  return `/api/requests${qs({
    status: filter.status.length ? filter.status.join(",") : null,
    attention: filter.attention ? 1 : null,
    kind: filter.kind,
    user_id: filter.userId,
    q: searchText(filter.q) || null,
    order: filter.order === "oldest" ? "oldest" : null,
    mine: filter.mine ? 1 : null,
    unseen: filter.unseen ? 1 : null,
    offset: offset || null,
    limit,
  })}`;
}

/**
 * Requests, `limit` at a time: again every 10 s while one shown is
 * downloading or importing, and every 30 s while no event would say what
 * changed.
 */
export function useRequests(filter: RequestFilter, { limit = LIST_PAGE, enabled = true }: { limit?: number; enabled?: boolean } = {}) {
  return useInfiniteQuery({
    queryKey: requestKeys.list(filter, limit),
    queryFn: ({ pageParam, signal }) => api.get<RequestList>(listPath(filter, pageParam, limit), { signal }),
    initialPageParam: 0,
    getNextPageParam: (last, pages) => {
      const loaded = pages.reduce((n, p) => n + p.requests.length, 0);
      return loaded < last.total && last.requests.length > 0 ? loaded : undefined;
    },
    refetchInterval: (query) => pollEvery(query.state.data?.pages.flatMap((p) => p.requests) ?? []),
    enabled,
    retry: retryRequest,
  });
}

/** How requests stand for a library title this account can open; `enabled` once its page has drawn. */
export function useItemRequest(itemId: Id, enabled: boolean) {
  return useQuery({
    queryKey: requestKeys.item(itemId),
    queryFn: ({ signal }) => api.get<ItemRequest>(`/api/items/${itemId}/request`, { signal }),
    enabled: enabled && itemId > 0,
    refetchInterval: (query) => pollEvery(query.state.data?.request ? [query.state.data.request] : []),
    retry: retryRequest,
  });
}

function sameTitle(data: unknown, kind: TitleKind, tmdbId: number): boolean {
  const d = data as { kind?: unknown; tmdb_id?: unknown } | undefined;
  return d?.kind === kind && d.tmdb_id === tmdbId;
}

/**
 * The rows and search results already shown say how a title stands now
 * (`update`), and nothing is fetched again for it: request events change
 * badges, not TMDB's lists.
 */
export function patchTitles(qc: QueryClient, kind: TitleKind, tmdbId: number, update: (title: DiscoverTitle) => DiscoverTitle) {
  const patch = (old: InfiniteData<DiscoverPage> | undefined) => {
    if (!old?.pages.some((p) => p.results.some((t) => t.kind === kind && t.tmdb_id === tmdbId))) return old;
    return {
      ...old,
      pages: old.pages.map((p) => ({
        ...p,
        results: p.results.map((t) => (t.kind === kind && t.tmdb_id === tmdbId ? update(t) : t)),
      })),
    };
  };
  qc.setQueriesData<InfiniteData<DiscoverPage>>({ queryKey: requestKeys.discovers }, patch);
  qc.setQueriesData<InfiniteData<DiscoverPage>>({ queryKey: requestKeys.searches }, patch);
}

/**
 * The badges shown for a title, from one of its requests as it is now
 * (`null`: there's no such request any more). A show whose request is
 * available may still lack aired seasons it didn't ask for: its details
 * are asked for, which say.
 */
export function patchBadges(qc: QueryClient, kind: TitleKind, tmdbId: number, requestId: Id, request: MediaRequest | null) {
  patchTitles(qc, kind, tmdbId, (t) => titleAfter(t, requestId, request));
  if (request?.kind === "show" && request.status === "available" && request.stage !== "gone") refetchBadges(qc, kind, tmdbId);
}

/**
 * The badges shown for a title, from its details asked for again, where
 * its requests can't say how it stands (whether a show is all there,
 * whether someone else's request for it stands).
 */
export function refetchBadges(qc: QueryClient, kind: TitleKind, tmdbId: number) {
  if (!titleShown(qc, kind, tmdbId)) return;
  void qc.fetchQuery({ ...requestQueries.title(kind, tmdbId), staleTime: 0 }).then(
    (detail) => patchTitles(qc, kind, tmdbId, (t) => ({ ...t, status: detail.status, item_id: detail.item_id, request_id: detail.request_id })),
    // Unknown now (TMDB away, say): the badge stays as it was.
    () => {},
  );
}

/** Whether a title shows in the rows or search results already loaded. */
export function titleShown(qc: QueryClient, kind: TitleKind, tmdbId: number): boolean {
  return [requestKeys.discovers, requestKeys.searches].some((queryKey) =>
    qc
      .getQueriesData<InfiniteData<DiscoverPage>>({ queryKey })
      .some(([, data]) => data?.pages.some((p) => p.results.some((t) => t.kind === kind && t.tmdb_id === tmdbId))),
  );
}

/** A request as it is now, wherever it shows: the lists, its title's pages, the rows' badges. */
export function putRequest(qc: QueryClient, request: MediaRequest, { added = false }: { added?: boolean } = {}) {
  const swap = (list: MediaRequest[]) =>
    list.some((r) => r.id === request.id) ? list.map((r) => (r.id === request.id ? request : r)) : added ? [request, ...list] : list;
  qc.setQueriesData<InfiniteData<RequestList>>({ queryKey: requestKeys.lists }, (old) =>
    old ? { ...old, pages: old.pages.map((p) => ({ ...p, requests: p.requests.map((r) => (r.id === request.id ? request : r)) })) } : old,
  );
  qc.setQueryData<DiscoverDetail>(requestKeys.title(request.kind, request.tmdb_id), (old) =>
    old ? { ...old, requests: swap(old.requests) } : old,
  );
  qc.setQueriesData<ItemRequest>({ queryKey: requestKeys.items }, (old) =>
    old && sameTitle(old, request.kind, request.tmdb_id) && (old.request?.id === request.id || (added && !old.request))
      ? { ...old, request }
      : old,
  );
  if (qc.getQueryData(requestKeys.one(request.id))) qc.setQueryData(requestKeys.one(request.id), request);
  patchBadges(qc, request.kind, request.tmdb_id, request.id, request);
}

/** A request gone for this account (withdrawn, deleted): out of its lists and its title's pages. */
function dropRequest(qc: QueryClient, gone: Pick<MediaRequest, "id" | "kind" | "tmdb_id">) {
  qc.setQueriesData<InfiniteData<RequestList>>({ queryKey: requestKeys.lists }, (old) =>
    old ? { ...old, pages: old.pages.map((p) => ({ ...p, requests: p.requests.filter((r) => r.id !== gone.id) })) } : old,
  );
  qc.setQueryData<DiscoverDetail>(requestKeys.title(gone.kind, gone.tmdb_id), (old) =>
    old ? { ...old, requests: old.requests.filter((r) => r.id !== gone.id) } : old,
  );
  qc.setQueriesData<ItemRequest>({ queryKey: requestKeys.items }, (old) => (old?.request?.id === gone.id ? { ...old, request: null } : old));
  patchBadges(qc, gone.kind, gone.tmdb_id, gone.id, null);
}

/**
 * After this page changed a request (or tried to): what it changes is
 * asked for again (statuses, counts, what's left to ask for). The event
 * that follows finds them fresh and leaves them be.
 */
function refreshAfter(qc: QueryClient, kind: TitleKind, tmdbId: number) {
  void qc.invalidateQueries({ queryKey: requestKeys.summary });
  void qc.invalidateQueries({ queryKey: requestKeys.lists });
  void qc.invalidateQueries({ queryKey: requestKeys.title(kind, tmdbId) });
  void qc.invalidateQueries({ queryKey: requestKeys.items, predicate: (q) => sameTitle(q.state.data, kind, tmdbId) });
}

/** Refused because the request changed meanwhile: it shows as it is now. */
function raced(qc: QueryClient, err: unknown, target: { kind: TitleKind; tmdb_id: number }) {
  const problem = requestProblem(err);
  if (problem?.kind === "conflict" && problem.request) putRequest(qc, problem.request);
  if (problem) refreshAfter(qc, target.kind, target.tmdb_id);
}

/** Asks for a movie or a show's seasons, or follows what someone asked for already ("Tell me too"). */
export function useAskRequest() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async (body: NewRequest): Promise<Asked> => {
      const { status, data } = await requestWithStatus<MediaRequest>("/api/requests", { method: "POST", body });
      return { request: data, following: status === 200 };
    },
    onSuccess: ({ request }) => {
      putRequest(qc, request, { added: true });
      refreshAfter(qc, request.kind, request.tmdb_id);
    },
    onError: (err, body) => raced(qc, err, body),
  });
}

/** The request a decision is about, to find where it shows. */
type Target = Pick<MediaRequest, "id" | "kind" | "tmdb_id">;

/** Approves a request (with choices of its own), or tries a failed one again. */
export function useApproveRequest() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ request, ...body }: Approval & { request: Target }) => api.post<MediaRequest>(`/api/requests/${request.id}/approve`, body),
    onSuccess: (request) => {
      putRequest(qc, request);
      refreshAfter(qc, request.kind, request.tmdb_id);
    },
    onError: (err, { request }) => raced(qc, err, request),
  });
}

export function useDeclineRequest() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ request, reason }: { request: Target; reason?: string }) =>
      api.post<MediaRequest>(`/api/requests/${request.id}/decline`, reason ? { reason } : {}),
    onSuccess: (request) => {
      putRequest(qc, request);
      refreshAfter(qc, request.kind, request.tmdb_id);
    },
    onError: (err, { request }) => raced(qc, err, request),
  });
}

/** Approves without sending: someone added it to Radarr or Sonarr by hand, and it waits for the library. */
export function useHandledRequest() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (request: Target) => api.post<MediaRequest>(`/api/requests/${request.id}/handled`),
    onSuccess: (request) => {
      putRequest(qc, request);
      refreshAfter(qc, request.kind, request.tmdb_id);
    },
    onError: (err, request) => raced(qc, err, request),
  });
}

/** Withdraws or removes this account's own request; managers delete any (Radarr and Sonarr keep what they have). */
export function useDeleteRequest() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (request: Target) => api.del<void>(`/api/requests/${request.id}`),
    onSuccess: (_data, request) => {
      dropRequest(qc, request);
      refreshAfter(qc, request.kind, request.tmdb_id);
    },
    onError: (err, request) => raced(qc, err, request),
  });
}

/** Stops following someone's request. */
export function useUnwatchRequest() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (request: Target) => api.del<void>(`/api/requests/${request.id}/watch`),
    onSuccess: (_data, request) => refreshAfter(qc, request.kind, request.tmdb_id),
    onError: (err, request) => raced(qc, err, request),
  });
}

/** Approves or declines several requests, one after another; what each answered, in order. */
export function useDecideMany() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: async ({ requests, decision, reason }: { requests: Target[]; decision: "approve" | "decline"; reason?: string }) => {
      const done: { request: Target; result: MediaRequest | null; error: unknown }[] = [];
      for (const request of requests) {
        try {
          const result =
            decision === "approve"
              ? await api.post<MediaRequest>(`/api/requests/${request.id}/approve`, {})
              : await api.post<MediaRequest>(`/api/requests/${request.id}/decline`, reason ? { reason } : {});
          putRequest(qc, result);
          done.push({ request, result, error: null });
        } catch (error) {
          const problem = requestProblem(error);
          if (problem?.kind === "conflict" && problem.request) putRequest(qc, problem.request);
          done.push({ request, result: null, error });
        }
      }
      return done;
    },
    onSettled: () => {
      void qc.invalidateQueries({ queryKey: requestKeys.summary });
      void qc.invalidateQueries({ queryKey: requestKeys.lists });
      void qc.invalidateQueries({ queryKey: requestKeys.titles });
    },
  });
}

/** This account has seen its requests' news: the number on Requests clears (on its other screens too). */
export function useMarkSeen() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post<void>("/api/requests/seen"),
    onSuccess: () => void qc.invalidateQueries({ queryKey: requestKeys.summary }),
  });
}

/** The profiles and folders approving may choose from, with the settings' choices. */
export function useApprovalOptions(enabled: boolean) {
  return useQuery({
    queryKey: requestKeys.options,
    queryFn: () => api.get<ApprovalOptions>("/api/requests/options"),
    enabled,
    staleTime: 60_000,
    retry: retryRequest,
  });
}

/** What the saved Radarr or Sonarr offers requests (`enabled` once an address and key are saved). */
export function useArrOptions(service: ArrService, enabled: boolean) {
  return useQuery({
    queryKey: requestKeys.arr(service),
    queryFn: () => api.post<ArrOptions>("/api/settings/arr", { service }),
    enabled,
    staleTime: 5 * 60_000,
  });
}

/** Asks Radarr or Sonarr at an address typed in what it offers (the settings aren't saved). */
export function useConnectArr() {
  return useMutation({
    mutationFn: (body: ArrConnection) => api.post<ArrOptions>("/api/settings/arr", body),
  });
}

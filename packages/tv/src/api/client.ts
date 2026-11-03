import { atHome, origin } from "../auth/addresses";
import { NOT_YOUR_SERVER, SignedOutError, type Tokens } from "../auth/tokens";
import { Cancel, NetworkError, TIMED_OUT, withQuery, type HttpRequest, type HttpResponse, type Transport } from "../net/transport";
import type {
  Collection,
  DiscoverDetail,
  DiscoverList,
  DiscoverPage,
  Hub,
  HistoryEntry,
  HouseholdProfile,
  Id,
  ItemDetail,
  ItemRequest,
  ItemSummary,
  Library,
  LiveChannel,
  LiveChannelPage,
  LiveGroups,
  LiveGuide,
  LiveSummary,
  LiveWatching,
  MediaRequest,
  NetworkAnswer,
  NetworkReport,
  NewRequest,
  LibrarySort,
  PersonDetail,
  PersonSummary,
  PlaybackInfo,
  PlaybackRequest,
  PlaybackState,
  Playlist,
  PlaylistEntry,
  RequestList,
  RequestStatus,
  RequestSummary,
  SessionStatus,
  SubtitleMode,
  SubtitleResult,
  SubtitleTrack,
  TitleKind,
  Timestamp,
  Trickplay,
  User,
} from "./types";

/** Which requests to list (`GET /api/requests`): each left out means any. */
export interface RequestQuery {
  status?: RequestStatus[];
  /** Managers: pending ones, and failed ones the server won't try again by itself. */
  attention?: boolean;
  kind?: TitleKind;
  /** Managers: one person's. */
  userId?: Id;
  /** The title has these words. */
  q?: string;
  order?: "newest" | "oldest";
  /** Managers: only this profile's and those it follows. */
  mine?: boolean;
  /** Only those with news since Your requests was last seen, newest news first. */
  unseen?: boolean;
  changedSince?: Timestamp;
  offset?: number;
  limit?: number;
}

/** An answer that isn't 2xx, with pley's `{ code, message }`. */
export class ApiError extends Error {
  readonly status: number;
  readonly code: string;
  /** The answer's JSON, when it had some (fields past `code` and `message`, as `setup`). */
  readonly body: unknown;
  constructor(status: number, code: string, message: string, body?: unknown) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.body = body;
  }
}

/** A playback session stopped from elsewhere (`410 session_stopped`), with what the one who stopped it wrote. */
export class SessionStoppedError extends ApiError {
  /** What the one who stopped it wrote, if anything. */
  readonly note: string | null;
  constructor(note: string | null) {
    super(410, "session_stopped", note ?? "Playback was stopped by the server's administrator");
    this.name = "SessionStoppedError";
    this.note = note;
  }
}

export function isApiError(e: unknown, status?: number): e is ApiError {
  return e instanceof ApiError && (status === undefined || e.status === status);
}

/** What to show for an error, as a sentence. */
export function errorMessage(e: unknown): string {
  const text = e instanceof Error ? e.message : String(e);
  return text ? text.charAt(0).toUpperCase() + text.slice(1) : text;
}

/**
 * The server answered nothing: no connection, no answer in time, or a
 * gateway in front of it saying it has none (502, 504, and a 503 that
 * isn't Pley's). A 503 with Pley's `{ code, … }` is the server's answer
 * (requests are off, TMDB isn't answering), as Roku has it: nothing goes
 * offline for one, though the player waits out a `503 unavailable`
 * mid-film as it does the server.
 */
export function unreachable(e: unknown): boolean {
  if (e instanceof NetworkError) return !e.cancelled;
  if (!(e instanceof ApiError)) return false;
  if (e.status === 502 || e.status === 504) return true;
  return e.status === 503 && !pleyAnswer(e);
}

/** The answer is pley's own JSON, with its `code`. */
function pleyAnswer(e: ApiError): boolean {
  const body = e.body as { code?: unknown } | undefined;
  return !!body && typeof body.code === "string";
}

/** Why the server answered nothing, as the Android app says it: "Nothing answers there." or "No answer in time." */
export function unreachableReason(e: unknown): string {
  if (e instanceof NetworkError && e.message === NOT_YOUR_SERVER) return "Another device answers there.";
  return e instanceof NetworkError && e.message === TIMED_OUT ? "No answer in time." : "Nothing answers there.";
}

/** A server at home accepts at once or never: after this long without an answer, a quick question tells which. */
export const HOME_WAIT_MS = 3_000;

/**
 * `req` to a server on the home network, which a TV can't give a connect
 * timeout of its own (the Android app gives it 3 s): when it hasn't
 * answered in 3 s, the server is asked a quick question, and if that gets
 * no answer in 3 s either, `req` gives up with "no answer in time". A
 * server that is only slow (a conversion starting) answers the question,
 * and `req` waits as long as it was going to.
 */
function watched(http: Transport, req: HttpRequest, ping: string): Promise<HttpResponse> {
  const own = new Cancel();
  const forget = req.cancel?.onCancel(() => own.cancel());
  return new Promise<HttpResponse>((resolve, reject) => {
    let done = false;
    let timer: ReturnType<typeof setTimeout> | null = null;
    const finish = (then: () => void) => {
      if (done) return;
      done = true;
      if (timer !== null) clearTimeout(timer);
      forget?.();
      then();
    };
    http({ ...req, cancel: own }).then(
      (res) => finish(() => resolve(res)),
      (e: unknown) => finish(() => reject(e)),
    );
    timer = setTimeout(() => {
      timer = null;
      http({ method: "GET", url: ping, headers: { Accept: "application/json" }, responseType: "json", timeoutMs: HOME_WAIT_MS, cancel: own }).then(
        () => undefined,
        (e: unknown) => {
          if (!(e instanceof NetworkError) || e.cancelled) return;
          finish(() => {
            own.cancel();
            reject(new NetworkError(TIMED_OUT));
          });
        },
      );
    }, HOME_WAIT_MS);
  });
}

interface CallOptions {
  method?: "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE";
  query?: Record<string, string | number | boolean | null | undefined>;
  body?: unknown;
  cancel?: Cancel;
  timeoutMs?: number;
  responseType?: "json" | "blob" | "text" | "arraybuffer";
  headers?: Record<string, string>;
  onProgress?: (loaded: number) => void;
}

/**
 * pley's API for one server, as one signed-in app: every request carries
 * the access token in `Authorization: Bearer` (the only place pley takes
 * an app's token), refreshed once on `invalid_token`.
 */
export class Api {
  readonly server: string;
  private readonly tokens: Tokens;
  private readonly http: Transport;
  /** The server is on the home network: a request it doesn't answer within 3 s gets a quick question. */
  private readonly homeNetwork: boolean;

  constructor(server: string, tokens: Tokens, http: Transport) {
    this.server = server;
    this.tokens = tokens;
    this.http = http;
    this.homeNetwork = atHome(server);
  }

  /** An absolute address on this server for `path` ("/api/…"). */
  url(path: string): string {
    return /^https?:\/\//.test(path) ? path : this.server + path;
  }

  /** The raw answer of an authenticated request; errors as [ApiError]. */
  async send(path: string, opts: CallOptions = {}): Promise<HttpResponse> {
    try {
      const token = await this.tokens.accessToken();
      if (!token) throw new SignedOutError();
      let res = await this.once(path, opts, token);
      if (res.status === 401 && invalidToken(res) && this.ownServer(path)) {
        // Refused just after a refresh: the server no longer takes this sign-in.
        if (await this.tokens.refusedAfterRefresh(token)) throw new SignedOutError();
        const fresh = await this.tokens.refresh(token);
        res = await this.once(path, opts, fresh);
      }
      if (res.status >= 200 && res.status < 300) return res;
      throw toError(res);
    } catch (e) {
      // Out of reach, the TV may be on another network now: what answers
      // at this address next proves it is the server before a token goes.
      if (e instanceof NetworkError && !e.cancelled) this.tokens.doubtServer();
      throw e;
    }
  }

  /**
   * Whether `path` is on this server: an address the server hands out
   * (a picture's, a playlist's) that names another origin gets no token,
   * as Android's (SECURITY.md).
   */
  private ownServer(path: string): boolean {
    return origin(this.url(path)) === origin(this.server);
  }

  private once(path: string, opts: CallOptions, token: string): Promise<HttpResponse> {
    const headers: Record<string, string> = { Accept: "application/json" };
    if (this.ownServer(path)) headers.Authorization = `Bearer ${token}`;
    let body: string | null = null;
    if (opts.body !== undefined) {
      headers["Content-Type"] = "application/json";
      body = JSON.stringify(opts.body);
    }
    if (opts.headers) for (const k in opts.headers) headers[k] = opts.headers[k]!;
    const req: HttpRequest = {
      method: opts.method ?? "GET",
      url: withQuery(this.url(path), opts.query),
      headers,
      body,
      responseType: opts.responseType ?? "json",
      timeoutMs: opts.timeoutMs,
      cancel: opts.cancel,
      onProgress: opts.onProgress,
    };
    return this.homeNetwork ? watched(this.http, req, this.url("/api/server")) : this.http(req);
  }

  async get<T>(path: string, opts: CallOptions = {}): Promise<T> {
    return (await this.send(path, opts)).body as T;
  }

  async call(method: "POST" | "PUT" | "PATCH" | "DELETE", path: string, body?: unknown, cancel?: Cancel): Promise<unknown> {
    return (await this.send(path, { method, body, cancel })).body;
  }

  // ---- account

  me(cancel?: Cancel): Promise<User> {
    return this.get("/api/auth/me", { cancel });
  }

  profiles(cancel?: Cancel): Promise<HouseholdProfile[]> {
    return this.get("/api/oauth/profiles", { cancel });
  }

  setTrackDefaults(audio: string | null, subtitle: string | null, mode: SubtitleMode): Promise<unknown> {
    return this.call("PUT", "/api/me/track-defaults", { audio_language: audio, subtitle_language: subtitle, subtitle_mode: mode });
  }

  // ---- browsing

  home(cancel?: Cancel): Promise<{ hubs: Hub[] }> {
    return this.get("/api/hubs/home", { cancel });
  }

  libraries(cancel?: Cancel): Promise<Library[]> {
    return this.get("/api/libraries", { cancel });
  }

  /** The profile's own order of its libraries, `ids` naming each once (scope play); them in it. */
  sortLibraries(ids: Id[]): Promise<Library[]> {
    return this.call("PUT", "/api/me/libraries/order", { ids }) as Promise<Library[]>;
  }

  /** The order everyone sees, for this profile again (scope play); the libraries in it. */
  forgetLibraryOrder(): Promise<Library[]> {
    return this.call("DELETE", "/api/me/libraries/order") as Promise<Library[]>;
  }

  libraryItems(
    id: Id,
    query: { sort?: LibrarySort; order?: "asc" | "desc"; watched?: boolean; genre?: string; year?: number; offset?: number; limit?: number },
    cancel?: Cancel,
  ): Promise<{ total: number; items: ItemSummary[] }> {
    return this.get(`/api/libraries/${id}/items`, { query, cancel });
  }

  libraryHubs(id: Id, cancel?: Cancel): Promise<{ hubs: Hub[] }> {
    return this.get(`/api/libraries/${id}/hubs`, { cancel });
  }

  libraryFilters(id: Id, cancel?: Cancel): Promise<{ genres: string[]; years: number[] }> {
    return this.get(`/api/libraries/${id}/filters`, { cancel });
  }

  collections(libraryId: Id, cancel?: Cancel): Promise<{ collections: Collection[] }> {
    return this.get(`/api/libraries/${libraryId}/collections`, { cancel });
  }

  collection(id: Id, cancel?: Cancel): Promise<Collection & { items: ItemSummary[] }> {
    return this.get(`/api/collections/${id}`, { cancel });
  }

  item(id: Id, cancel?: Cancel): Promise<ItemDetail> {
    return this.get(`/api/items/${id}`, { cancel });
  }

  children(id: Id, cancel?: Cancel): Promise<{ items: ItemSummary[] }> {
    return this.get(`/api/items/${id}/children`, { cancel });
  }

  onDeck(id: Id, cancel?: Cancel): Promise<{ item: ItemSummary | null }> {
    return this.get(`/api/items/${id}/on-deck`, { cancel });
  }

  next(id: Id, cancel?: Cancel): Promise<{ item: ItemSummary | null }> {
    return this.get(`/api/items/${id}/next`, { cancel });
  }

  related(id: Id, cancel?: Cancel): Promise<{ items: ItemSummary[] }> {
    return this.get(`/api/items/${id}/related`, { query: { limit: 20 }, cancel });
  }

  person(id: Id, cancel?: Cancel): Promise<PersonDetail> {
    return this.get(`/api/people/${id}`, { cancel });
  }

  /** Items, people and (where Live TV is open to the profile) up to 12 Live TV channels. */
  search(q: string, cancel?: Cancel): Promise<{ items: ItemSummary[]; people?: PersonSummary[]; channels?: LiveChannel[] }> {
    return this.get("/api/search", { query: { q, limit: 60 }, cancel });
  }

  playlists(cancel?: Cancel): Promise<{ playlists: Playlist[] }> {
    return this.get("/api/playlists", { cancel });
  }

  playlist(id: Id, cancel?: Cancel): Promise<Playlist & { entries: PlaylistEntry[] }> {
    return this.get(`/api/playlists/${id}`, { cancel });
  }

  /** The collections an item is part of. */
  itemCollections(id: Id, cancel?: Cancel): Promise<{ collections: { id: Id; title: string }[] }> {
    return this.get(`/api/items/${id}/collections`, { cancel });
  }

  addToPlaylist(playlistId: Id, itemIds: Id[]): Promise<{ added: number }> {
    return this.call("POST", `/api/playlists/${playlistId}/items`, { item_ids: itemIds }) as Promise<{ added: number }>;
  }

  createPlaylist(title: string, kind: "video" | "audio" | null, itemIds: Id[]): Promise<Playlist> {
    return this.call("POST", "/api/playlists", { title, ...(kind ? { kind } : {}), item_ids: itemIds }) as Promise<Playlist>;
  }

  renamePlaylist(id: Id, title: string): Promise<unknown> {
    return this.call("PATCH", `/api/playlists/${id}`, { title });
  }

  deletePlaylist(id: Id): Promise<unknown> {
    return this.call("DELETE", `/api/playlists/${id}`);
  }

  /** Takes one entry out (the item stays in its library). */
  removeFromPlaylist(id: Id, entryId: Id): Promise<unknown> {
    return this.call("DELETE", `/api/playlists/${id}/items/${entryId}`);
  }

  /** The entries in their new order, every one of them. */
  reorderPlaylist(id: Id, entryIds: Id[]): Promise<unknown> {
    return this.call("PUT", `/api/playlists/${id}/order`, { entry_ids: entryIds });
  }

  /** A smart playlist: a library's listing by `filter` (Plex's syntax), which keeps up with the library by itself. */
  createSmartPlaylist(title: string, libraryId: Id, filter: string): Promise<Playlist> {
    return this.call("POST", "/api/playlists", { title, smart_filter: filter, library_id: libraryId }) as Promise<Playlist>;
  }

  // ---- watching

  history(offset: number, limit: number, cancel?: Cancel): Promise<{ total: number; entries: HistoryEntry[] }> {
    return this.get("/api/history", { query: { offset, limit }, cancel });
  }

  removeHistory(id: Id): Promise<unknown> {
    return this.call("DELETE", `/api/history/${id}`);
  }

  addToMyList(id: Id): Promise<unknown> {
    return this.call("PUT", `/api/watchlist/${id}`);
  }

  removeFromMyList(id: Id): Promise<unknown> {
    return this.call("DELETE", `/api/watchlist/${id}`);
  }

  hideFromContinueWatching(id: Id): Promise<unknown> {
    return this.call("DELETE", `/api/continue-watching/${id}`);
  }

  /** The person's rating, 0–10 (halves), or none. */
  rate(id: Id, rating: number | null): Promise<unknown> {
    return this.call("PUT", `/api/items/${id}/rating`, rating === null ? {} : { rating });
  }

  markWatched(id: Id): Promise<unknown> {
    return this.call("POST", `/api/items/${id}/watched`);
  }

  markUnwatched(id: Id): Promise<unknown> {
    return this.call("POST", `/api/items/${id}/unwatched`);
  }

  prefetch(id: Id): Promise<unknown> {
    return this.call("POST", `/api/items/${id}/prefetch`);
  }

  /** The tracks picked for an item (the whole show for an episode); `audio` left out keeps the audio picked before. */
  setStreams(itemId: Id, fileId: Id | null, audio: number | null | undefined, subtitle: number | null): Promise<unknown> {
    return this.call("PUT", `/api/items/${itemId}/streams`, { file_id: fileId ?? undefined, audio_index: audio, subtitle_index: subtitle });
  }

  // ---- playback

  play(req: PlaybackRequest, cancel?: Cancel): Promise<PlaybackInfo> {
    return this.send("/api/playback", { method: "POST", body: req, cancel, timeoutMs: 60_000 }).then((r) => r.body as PlaybackInfo);
  }

  /**
   * A session's playlist (`GET`, as text) or segment (`HEAD`: answered
   * once the transcoder has made it), asked for as its player will:
   * keyed addresses as they are (they carry their own key), others with the
   * token.
   */
  media(method: "GET" | "HEAD", url: string, cancel?: Cancel, timeoutMs?: number): Promise<string> {
    const full = this.url(url);
    const answered = (res: HttpResponse): string => {
      if (res.status < 200 || res.status >= 300) throw toError(res);
      return typeof res.body === "string" ? res.body : "";
    };
    if (/^(https?:\/\/[^/]+)?\/api\/(live\/)?media\//.test(full)) {
      return this.http({ method, url: full, headers: { Accept: "*/*" }, responseType: "text", timeoutMs, cancel }).then(answered);
    }
    return this.send(full, { method, responseType: "text", headers: { Accept: "*/*" }, cancel, timeoutMs }).then(answered);
  }

  sessionStatus(session: string, cancel?: Cancel, timeoutMs?: number): Promise<SessionStatus> {
    return this.get(`/api/playback/${encodeURIComponent(session)}`, { cancel, timeoutMs });
  }

  /**
   * The session's converted video goes on at `kbps` from the segments its
   * player hasn't fetched yet, the same size and playlist
   * (`PlaybackInfo.live_bitrate`); the answer is what it goes on at.
   */
  setBitrate(session: string, kbps: number): Promise<{ bitrate: number }> {
    return this.send(`/api/playback/${encodeURIComponent(session)}/bitrate`, { method: "PUT", body: { max_bitrate: Math.round(kbps) }, timeoutMs: 10_000 }).then(
      (r) => r.body as { bitrate: number },
    );
  }

  /** Whether the server answers at all (a player reconnecting without a session to ask about). */
  reachable(timeoutMs: number): Promise<unknown> {
    return this.get("/api/server", { timeoutMs });
  }

  /** What the server knows of the network this TV plays on, for Automatic's start (docs/QUALITY.md). */
  network(clientId: string, timeoutMs: number, cancel?: Cancel): Promise<NetworkAnswer> {
    return this.get("/api/playback/network", { query: { client_id: clientId }, timeoutMs, cancel });
  }

  /** Tells the server a speed test's result or Automatic's new ceiling; its answer as `network`'s. */
  tellNetwork(clientId: string, report: NetworkReport, timeoutMs?: number): Promise<NetworkAnswer> {
    return this.send("/api/playback/network", { method: "PUT", body: { client_id: clientId, ...report }, timeoutMs }).then((r) => r.body as NetworkAnswer);
  }

  /** The speed test's `bytes` of noise, `onProgress` hearing how much has arrived; `cancel` ends it. */
  speedTest(bytes: number, onProgress: (loaded: number) => void, cancel: Cancel): Promise<unknown> {
    return this.send("/api/playback/speedtest", { query: { bytes }, responseType: "arraybuffer", onProgress, cancel, timeoutMs: 10_000 });
  }

  stopSession(session: string): Promise<unknown> {
    return this.call("DELETE", `/api/playback/${encodeURIComponent(session)}`);
  }

  progress(itemId: Id, positionMs: number, durationMs: number | null, state: PlaybackState, session: string | null): Promise<unknown> {
    return this.call("POST", `/api/items/${itemId}/progress`, {
      position_ms: Math.max(0, Math.round(positionMs)),
      duration_ms: durationMs && durationMs > 0 ? Math.round(durationMs) : undefined,
      state,
      session_id: session ?? undefined,
    });
  }

  /** Subtitle files found for an item's file, in `languages` (two-letter codes; the profile's own when left out). */
  searchSubtitles(itemId: Id, fileId: Id | null, languages: string[] | null, cancel?: Cancel): Promise<SubtitleResult[]> {
    return this.get<{ results?: SubtitleResult[] }>(`/api/items/${itemId}/subtitles/search`, {
      query: { file_id: fileId, language: languages && languages.length ? languages.join(",") : null },
      cancel,
      timeoutMs: 30_000,
    }).then((r) => r.results ?? []);
  }

  /** Adds a subtitle file `searchSubtitles` found, for everyone; the new track. */
  addSubtitle(itemId: Id, fileId: Id | null, result: string): Promise<SubtitleTrack> {
    return this.send(`/api/items/${itemId}/subtitles`, { method: "POST", body: fileId === null ? { result } : { result, file_id: fileId }, timeoutMs: 30_000 }).then(
      (r) => r.body as SubtitleTrack,
    );
  }

  trickplay(fileId: Id, cancel?: Cancel): Promise<Trickplay> {
    return this.get(`/api/files/${fileId}/trickplay`, { cancel });
  }

  // ---- Live TV

  /** Whether to show Live TV, and the channels watched last. */
  liveSummary(cancel?: Cancel): Promise<LiveSummary> {
    return this.get("/api/live", { cancel });
  }

  /** The groups, and how many channels in all, marked and watched lately. */
  liveGroups(cancel?: Cancel): Promise<LiveGroups> {
    return this.get("/api/live/groups", { cancel });
  }

  /**
   * A page of a category's channels (`limit`, 500 at most), each with
   * what's on now and next; `around` gives the page holding that channel.
   */
  liveBrowse(
    query: { category: "all" | "favorites" | "recent" | "group"; group?: string; q?: string; offset?: number; limit?: number; around?: Id },
    cancel?: Cancel,
  ): Promise<LiveChannelPage> {
    return this.get("/api/live/browse", { query, cancel });
  }

  liveChannel(id: Id, cancel?: Cancel): Promise<LiveChannel> {
    return this.get(`/api/live/channels/${id}`, { cancel });
  }

  /** The guide of channels `ids` between `from` and `to` (unix seconds; two days at most). */
  liveGuide(from: number, to: number, ids: Id[], cancel?: Cancel): Promise<LiveGuide> {
    return this.get("/api/live/guide", { query: { from, to, ids: ids.join(",") }, cancel });
  }

  setLiveFavorite(channelId: Id, favorite: boolean): Promise<unknown> {
    return this.call(favorite ? "PUT" : "DELETE", `/api/live/channels/${channelId}/favorite`);
  }

  /**
   * Starts watching a channel on this TV (`clientId`: what it watched
   * before ends). The playlist's address carries a key of its own, for
   * the TV's player, which can't send the token. The server waits for
   * the provider when it tunes the channel. Every connection being in use
   * (`409`, with `connections`), `takeOver` is the channel whose
   * connection to take, one the `409` said may be.
   */
  watchChannel(channelId: Id, clientId: string, cancel?: Cancel, takeOver?: Id): Promise<LiveWatching> {
    const body: Record<string, unknown> = { client_id: clientId, keyed_urls: true };
    if (takeOver !== undefined) body.take_over = takeOver;
    return this.send(`/api/live/channels/${channelId}/watch`, { method: "POST", body, cancel, timeoutMs: 45_000 }).then((r) => r.body as LiveWatching);
  }

  /**
   * Why a watching session ended (someone took its connection over, say),
   * or null while it goes on. Asked with the token: a keyed address only
   * says the session has gone.
   */
  whyLiveEnded(session: string, cancel?: Cancel): Promise<string | null> {
    return this.get(`/api/live/sessions/${encodeURIComponent(session)}/plan`, { cancel, timeoutMs: 8_000 }).then(
      () => null,
      (e: unknown) => {
        // Ended: the server's words, or "" when it gave none (not a video's "stopped by the administrator").
        if (e instanceof SessionStoppedError) return e.note ?? "";
        if (isApiError(e, 410)) return e.message;
        throw e;
      },
    );
  }

  /** Stops watching: the connection is let go once nobody else watches. */
  leaveChannel(session: string): Promise<unknown> {
    return this.call("DELETE", `/api/live/sessions/${encodeURIComponent(session)}`);
  }

  // ---- requests and Discover

  /** What requests are for this sign-in: on or off, what asking does, the limits and the counts (any app may ask). */
  requestSummary(cancel?: Cancel): Promise<RequestSummary> {
    return this.get("/api/requests/summary", { cancel });
  }

  /** One page of a Discover list. */
  discover(list: DiscoverList, page: number, cancel?: Cancel): Promise<DiscoverPage> {
    return this.get(`/api/discover/${list}`, { query: { page }, cancel });
  }

  /** TMDB's movies and shows for `q`; `missing`: without those a library has whole (search, which shows those already). */
  discoverSearch(q: string, page: number, missing = false, cancel?: Cancel): Promise<DiscoverPage> {
    return this.get("/api/discover/search", { query: { q, page, missing: missing ? 1 : undefined }, cancel });
  }

  /** A movie's or show's page; TMDB numbers the two separately, so `kind` is the one it came with. */
  discoverTitle(kind: TitleKind, tmdbId: number, cancel?: Cancel): Promise<DiscoverDetail> {
    return this.get(`/api/discover/${kind}/${tmdbId}`, { cancel });
  }

  /** A library title's place in requests: its status, the request for it, the seasons left to ask for. */
  itemRequest(id: Id, cancel?: Cancel): Promise<ItemRequest> {
    return this.get(`/api/items/${id}/request`, { cancel });
  }

  requests(q: RequestQuery, cancel?: Cancel): Promise<RequestList> {
    const flag = (on: boolean | undefined) => (on ? 1 : undefined);
    return this.get("/api/requests", {
      query: {
        status: q.status && q.status.length ? q.status.join(",") : undefined,
        attention: flag(q.attention),
        kind: q.kind,
        user_id: q.userId,
        q: q.q ? q.q : undefined,
        order: q.order,
        mine: flag(q.mine),
        unseen: flag(q.unseen),
        changed_since: q.changedSince,
        offset: q.offset,
        limit: q.limit,
      },
      cancel,
    });
  }

  request(id: Id, cancel?: Cancel): Promise<MediaRequest> {
    return this.get(`/api/requests/${id}`, { cancel });
  }

  /**
   * Asks for a title, or follows the requests that cover it: `created` is
   * true for a new request (201), false when it only follows (200).
   */
  askFor(body: NewRequest): Promise<{ request: MediaRequest; created: boolean }> {
    return this.send("/api/requests", { method: "POST", body, timeoutMs: 30_000 }).then((r) => ({ request: r.body as MediaRequest, created: r.status === 201 }));
  }

  /** Withdraws this profile's own request (or removes a declined one); a manager's deletes anyone's. */
  deleteRequest(id: Id): Promise<unknown> {
    return this.call("DELETE", `/api/requests/${id}`);
  }

  /** Stops following someone else's request. */
  unwatchRequest(id: Id): Promise<unknown> {
    return this.call("DELETE", `/api/requests/${id}/watch`);
  }

  /** Your requests was seen: its news is no longer new, on this profile's other devices too. */
  markRequestsSeen(): Promise<unknown> {
    return this.call("POST", "/api/requests/seen");
  }

  /** Managers: approves (or tries again) with the settings' choices; the request as it is now. */
  approveRequest(id: Id): Promise<MediaRequest> {
    return this.call("POST", `/api/requests/${id}/approve`) as Promise<MediaRequest>;
  }

  /** Managers: declines, with what its maker reads as the reason, if any. */
  declineRequest(id: Id, reason: string | null): Promise<MediaRequest> {
    return this.call("POST", `/api/requests/${id}/decline`, reason ? { reason } : undefined) as Promise<MediaRequest>;
  }

  /** Managers: approved without sending it anywhere, as added to the library by hand. */
  markRequestHandled(id: Id): Promise<MediaRequest> {
    return this.call("POST", `/api/requests/${id}/handled`) as Promise<MediaRequest>;
  }
}

function invalidToken(res: HttpResponse): boolean {
  const challenge = res.header("WWW-Authenticate") ?? "";
  // A bare 401 from an older server counts too: the token is what failed.
  return !challenge || /invalid_token/.test(challenge);
}

function toError(res: HttpResponse): ApiError {
  let code = statusCode(res.status);
  let message = `the server refused (HTTP ${res.status})`;
  const body = res.body;
  if (body && typeof body === "object" && !(body instanceof Blob)) {
    const o = body as { code?: unknown; message?: unknown };
    if (typeof o.code === "string") code = o.code;
    if (typeof o.message === "string") message = o.message;
  }
  if (res.status === 410 && code === "session_stopped") {
    const note = body && typeof body === "object" ? (body as { message?: unknown }).message : null;
    return new SessionStoppedError(typeof note === "string" ? note : null);
  }
  return new ApiError(res.status, code, message, body && typeof body === "object" && !(body instanceof Blob) ? body : undefined);
}

function statusCode(status: number): string {
  switch (status) {
    case 400:
      return "bad_request";
    case 401:
      return "unauthorized";
    case 403:
      return "forbidden";
    case 404:
      return "not_found";
    case 409:
      return "conflict";
    case 410:
      return "gone";
    case 429:
      return "too_many_requests";
    case 503:
      return "unavailable";
    default:
      return status >= 500 ? "internal" : "bad_request";
  }
}

export { Cancel, NetworkError };

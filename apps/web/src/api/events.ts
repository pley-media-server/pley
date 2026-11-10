import { useEffect } from "react";
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { connectionLost, eventsLive, serverAnswers, setConnectionLost, setEventsLive } from "@/lib/connection";
import { isApiError } from "./client";
import { keys } from "./hooks";
import { liveKeys } from "./livetv";
import { patchBadges, patchTitles, refetchBadges, requestKeys, requestQueries, titleShown, type TitleKind } from "./requests";
import type { Activity, Id, Library, ServerEvent } from "./types";

/** How long the stream may be down before the pages say the server is out of reach: a restart takes a moment. */
export const LOST_AFTER_MS = 5_000;

type Listener = (ev: ServerEvent) => void;
const listeners = new Set<Listener>();

/**
 * Calls `listener` with each event from the server's event stream (the one
 * {@link useServerEvents} keeps open) until the returned function is called.
 */
export function onServerEvent(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** The stream's query while this app takes casts (`receiver=…&casts=…`), else null. */
let receiverQuery: string | null = null;
const receiverChanged = new Set<() => void>();

/**
 * Opens the event stream as a receiver of casts with `query` (docs/API.md,
 * "Casting"), or as a plain stream with `null`: a stream already open opens
 * again with it, for the user's remotes to list this device, or no longer.
 */
export function setReceiver(query: string | null) {
  if (query === receiverQuery) return;
  receiverQuery = query;
  for (const changed of receiverChanged) changed();
}

/** `user_state_changed` events heard and not acted on yet. */
interface WatchStateChange {
  ids: Set<Id>;
  /** Everything under one of them changed too (a show marked watched). */
  children: boolean;
  /** When the last one was heard (ms). */
  heardAt: number;
}

/**
 * Whether the query under `key` shows the user's watch state of what
 * `change` names: Home, the libraries' rows and grids, My List, search, and
 * the items' own queries (every item's when what is under one changed).
 * What a flush of `libraries` refetches anyway is left to it.
 */
function showsWatchState(key: readonly unknown[], change: WatchStateChange, libraries: Id[]): boolean {
  const flushed = libraries.length > 0;
  switch (key[0]) {
    case "hubs":
    case "search":
      return !flushed;
    case "library":
      return !libraries.includes(key[1] as Id);
    case "watchlist":
      return true;
    case "item":
      return !flushed && (change.children || change.ids.has(key[1] as Id));
    default:
      return false;
  }
}

/** TMDB's lists and searches, under `requestKeys.all`. */
const isTmdbList = (key: readonly unknown[]) => key[1] === "discover" || key[1] === "search";

/** A request that changed, heard and not acted on yet (`id` `null`: many did). */
interface RequestChange {
  id: Id | null;
  kind: TitleKind | null;
  tmdbId: number | null;
  /** When it was heard (ms). */
  heardAt: number;
}

/**
 * What shows requests, after some changed: the lists and the titles'
 * pages on screen are asked again (those fetched before the change was
 * heard: what this page changed itself is fresh), and the badges of rows
 * already shown follow the request as it is now (or the title's details,
 * where the request can't say). Discover's rows and search results are
 * never fetched again for it.
 */
function refreshRequests(qc: QueryClient, changes: RequestChange[]) {
  const heardAt = Math.max(...changes.map((c) => c.heardAt));
  const before = (q: { state: { dataUpdatedAt: number } }) => q.state.dataUpdatedAt <= heardAt;
  void qc.invalidateQueries({ queryKey: requestKeys.lists, predicate: before });
  if (changes.some((c) => c.id === null)) {
    void qc.invalidateQueries({ queryKey: requestKeys.titles, predicate: before });
    void qc.invalidateQueries({ queryKey: requestKeys.items, predicate: before });
    void qc.invalidateQueries({ queryKey: requestKeys.ones, predicate: before });
    // Which badges changed isn't known: rows say so when next shown.
    void qc.invalidateQueries({ queryKey: requestKeys.discovers, refetchType: "none" });
    void qc.invalidateQueries({ queryKey: requestKeys.searches, refetchType: "none" });
    return;
  }
  for (const { id, kind, tmdbId } of changes) {
    if (id === null) continue;
    void qc.invalidateQueries({ queryKey: requestKeys.one(id), predicate: before });
    if (kind === null || tmdbId === null) continue;
    void qc.invalidateQueries({ queryKey: requestKeys.title(kind, tmdbId), predicate: before });
    void qc.invalidateQueries({
      queryKey: requestKeys.items,
      predicate: (q) => {
        const data = q.state.data as { kind?: unknown; tmdb_id?: unknown } | undefined;
        return before(q) && data?.kind === kind && data.tmdb_id === tmdbId;
      },
    });
    if (!titleShown(qc, kind, tmdbId)) continue;
    // What the rows' badges follow: the request as this account sees it now. One it no longer sees
    // (404: withdrawn, deleted, no longer followed) may still stand for others: the title's details say.
    void qc.fetchQuery({ ...requestQueries.one(id), staleTime: 0 }).then(
      (request) => patchBadges(qc, kind, tmdbId, id, request),
      (err: unknown) => {
        if (isApiError(err, 404)) refetchBadges(qc, kind, tmdbId);
      },
    );
  }
}

/**
 * Keeps one `EventSource('/api/events')` open while logged in (`userId`)
 * and turns server events into React Query cache updates/invalidations.
 * Another profile gets a stream of its own: the events are the account's.
 */
export function useServerEvents(userId: Id | null) {
  const qc = useQueryClient();

  useEffect(() => {
    if (userId === null || typeof EventSource === "undefined") return;

    let es: EventSource | null = null;
    let retryTimer: number | undefined;
    let flushTimer: number | undefined;
    let flushAt = 0;
    let closed = false;
    let failures = 0;
    let connectedBefore = false;
    let lostTimer: number | undefined;
    const pendingLibraries = new Set<Id>();
    let pendingWatchState: WatchStateChange | null = null;
    let summaryTimer: number | undefined;
    let requestsTimer: number | undefined;
    let pendingRequests: RequestChange[] = [];

    // Down a while, and the server doesn't answer at all: the pages say so
    // (a stream refused, a sign-in that ended, is something else).
    const dropped = () => {
      if (connectionLost() || lostTimer !== undefined) return;
      lostTimer = window.setTimeout(() => {
        lostTimer = undefined;
        void serverAnswers().then((answers) => {
          if (!closed && !answers && es?.readyState !== EventSource.OPEN) setConnectionLost(true);
        });
      }, LOST_AFTER_MS);
    };
    const back = () => {
      window.clearTimeout(lostTimer);
      lostTimer = undefined;
      setConnectionLost(false);
    };

    // The server neither replays events nor queues them for slow clients:
    // after a reconnect anything may have changed (e.g. a scan finished while
    // the server restarted), so refetch what events would have updated.
    const resync = () => {
      void qc.invalidateQueries({ queryKey: keys.libraries });
      void qc.invalidateQueries({ queryKey: keys.activity });
      void qc.invalidateQueries({ queryKey: keys.sessions });
      void qc.invalidateQueries({ queryKey: keys.remoteDevices });
      void qc.invalidateQueries({ queryKey: ["hubs"] });
      void qc.invalidateQueries({ queryKey: ["library"] });
      void qc.invalidateQueries({ queryKey: ["item"] });
      void qc.invalidateQueries({ queryKey: ["search"] });
      void qc.invalidateQueries({ queryKey: ["history"] });
      void qc.invalidateQueries({ queryKey: keys.watchlist });
      void qc.invalidateQueries({ queryKey: liveKeys.all });
      // Requests: all but TMDB's rows, whose badges say so when next shown.
      void qc.invalidateQueries({ queryKey: requestKeys.all, predicate: (q) => !isTmdbList(q.queryKey) });
      void qc.invalidateQueries({ queryKey: requestKeys.all, predicate: (q) => isTmdbList(q.queryKey), refetchType: "none" });
    };

    // The summary once a burst of request events is over (the number on
    // Requests, what's left to ask for); what shows the requests that
    // changed a moment after.
    const refreshSummary = () => {
      if (summaryTimer !== undefined) return;
      summaryTimer = window.setTimeout(() => {
        summaryTimer = undefined;
        void qc.invalidateQueries({ queryKey: requestKeys.summary });
      }, 1000);
    };
    const requestChanged = (id: Id | null, kind: TitleKind | null, tmdbId: number | null) => {
      refreshSummary();
      pendingRequests.push({ id, kind, tmdbId, heardAt: Date.now() });
      if (requestsTimer !== undefined) return;
      requestsTimer = window.setTimeout(() => {
        requestsTimer = undefined;
        const changes = pendingRequests;
        pendingRequests = [];
        refreshRequests(qc, changes);
      }, 300);
    };
    // Turned on or off, other limits, another age limit or role: what
    // each page offers may have changed.
    const requestsSettingsChanged = () => {
      refreshSummary();
      for (const queryKey of [requestKeys.titles, requestKeys.items, requestKeys.lists, requestKeys.options]) {
        void qc.invalidateQueries({ queryKey });
      }
      void qc.invalidateQueries({ queryKey: requestKeys.discovers, refetchType: "none" });
      void qc.invalidateQueries({ queryKey: requestKeys.searches, refetchType: "none" });
    };
    // Without the stream, what shows requests asks again now and then (and
    // the summary every minute). A query sets its next poll as it answers,
    // so what shows requests is asked at once: idle ones start polling.
    const live = (value: boolean) => {
      if (eventsLive() === value) return;
      setEventsLive(value);
      if (!value) void qc.invalidateQueries({ queryKey: requestKeys.all, predicate: (q) => !isTmdbList(q.queryKey) });
    };

    // Metadata refreshes can emit many `library_changed` events in a burst;
    // batch them so we refetch at most every ~1.5 s. Watch state changes are
    // batched for ~0.3 s, and bring a library flush due later forward: a play
    // that ends sends both, and both are refetched once.
    const flush = () => {
      window.clearTimeout(flushTimer);
      flushTimer = undefined;
      const libraries = [...pendingLibraries];
      pendingLibraries.clear();
      const watchState = pendingWatchState;
      pendingWatchState = null;
      if (libraries.length > 0) {
        for (const id of libraries) void qc.invalidateQueries({ queryKey: keys.library(id) });
        void qc.invalidateQueries({ queryKey: keys.libraries });
        void qc.invalidateQueries({ queryKey: ["hubs"] });
        void qc.invalidateQueries({ queryKey: ["item"] });
        void qc.invalidateQueries({ queryKey: ["search"] });
        // Finishing something is a library change too.
        void qc.invalidateQueries({ queryKey: ["history"] });
      }
      if (watchState) {
        // Only what was fetched before the change was heard: what this page
        // changed itself is refetched already.
        void qc.invalidateQueries({
          predicate: (q) =>
            q.state.dataUpdatedAt <= watchState.heardAt && showsWatchState(q.queryKey, watchState, libraries),
        });
      }
    };
    const schedule = (delayMs: number) => {
      const at = Date.now() + delayMs;
      if (flushTimer !== undefined && flushAt <= at) return;
      window.clearTimeout(flushTimer);
      flushAt = at;
      flushTimer = window.setTimeout(flush, delayMs);
    };
    const scheduleLibrary = (id: Id) => {
      pendingLibraries.add(id);
      schedule(1500);
    };
    const scheduleWatchState = (itemIds: Id[], children: boolean) => {
      const heardAt = Date.now();
      const pending = pendingWatchState;
      pendingWatchState = pending
        ? { ids: new Set([...pending.ids, ...itemIds]), children: pending.children || children, heardAt }
        : { ids: new Set(itemIds), children, heardAt };
      schedule(300);
    };

    const setScanning = (id: Id, scanning: boolean) => {
      // The user's order and the shared one (Settings) alike.
      qc.setQueriesData<Library[]>({ queryKey: keys.libraries }, (old) =>
        old?.map((l) => (l.id === id && l.scanning !== scanning ? { ...l, scanning } : l)),
      );
    };

    const handle = (ev: ServerEvent) => {
      switch (ev.type) {
        case "scan_started":
          setScanning(ev.library_id, true);
          void qc.invalidateQueries({ queryKey: keys.activity });
          break;
        case "scan_progress": {
          setScanning(ev.library_id, ev.phase !== "done");
          // Kept as fetched when it was: an event says less than the server
          // (whether it's a first scan, those done), so Home still fetches
          // the activity once it's due, and an event never makes one up.
          const fetched = qc.getQueryState<Activity>(keys.activity)?.dataUpdatedAt;
          const before = qc.getQueryData<Activity>(keys.activity);
          qc.setQueryData<Activity>(
            keys.activity,
            (old) => {
              if (!old) return old;
              const existing = old.scans.find((s) => s.library_id === ev.library_id);
              if (existing) {
                return {
                  ...old,
                  scans: old.scans.map((s) =>
                    s.library_id === ev.library_id
                      ? { ...s, phase: ev.phase, processed: ev.processed, total: ev.total }
                      : s,
                  ),
                };
              }
              const name = qc.getQueryData<Library[]>(keys.libraries)?.find((l) => l.id === ev.library_id)?.name ?? "";
              return {
                ...old,
                scans: [
                  ...old.scans,
                  {
                    library_id: ev.library_id,
                    library_name: name,
                    phase: ev.phase,
                    processed: ev.processed,
                    total: ev.total,
                    started_at: Math.floor(Date.now() / 1000),
                  },
                ],
              };
            },
            { updatedAt: fetched },
          );
          // A scan not heard of yet: what the server says of it is fetched.
          if (before && !before.scans.some((s) => s.library_id === ev.library_id)) {
            void qc.invalidateQueries({ queryKey: keys.activity });
          }
          break;
        }
        case "scan_finished":
          setScanning(ev.library_id, false);
          void qc.invalidateQueries({ queryKey: keys.activity });
          scheduleLibrary(ev.library_id);
          break;
        case "library_changed":
          scheduleLibrary(ev.library_id);
          break;
        case "libraries_changed":
          void qc.invalidateQueries({ queryKey: keys.libraries });
          void qc.invalidateQueries({ queryKey: ["hubs"] });
          break;
        // A device that takes casts coming or going says so too.
        case "sessions_changed":
          void qc.invalidateQueries({ queryKey: keys.activity });
          void qc.invalidateQueries({ queryKey: keys.sessions });
          void qc.invalidateQueries({ queryKey: keys.remoteDevices });
          break;
        case "session_stopped":
          void qc.invalidateQueries({ queryKey: keys.sessions });
          break;
        case "watchlist_changed":
          void qc.invalidateQueries({ queryKey: keys.watchlist });
          void qc.invalidateQueries({ queryKey: keys.homeHubs });
          void qc.invalidateQueries({ queryKey: ["item"] });
          break;
        case "user_state_changed":
          scheduleWatchState(ev.item_ids, ev.children);
          break;
        case "live_channels_changed":
          void qc.invalidateQueries({ queryKey: liveKeys.all });
          break;
        case "live_tuners_changed":
          void qc.invalidateQueries({ queryKey: liveKeys.tuners });
          void qc.invalidateQueries({ queryKey: liveKeys.sources });
          break;
        // Statuses on the titles' pages and the badges in rows already
        // shown, the lists of requests, the number on Requests.
        case "requests_changed":
          requestChanged(ev.request_id, ev.kind, ev.tmdb_id);
          break;
        case "request_available": {
          const itemId = ev.item_id;
          // A movie in a library this account can open: the badge says so at once. A show may still lack
          // aired seasons it didn't ask for: its badge follows its details.
          if (itemId !== null && ev.complete && ev.kind === "movie") {
            patchTitles(qc, ev.kind, ev.tmdb_id, (t) => ({ ...t, status: "library", item_id: itemId, request_id: null }));
          }
          requestChanged(ev.request_id, ev.kind, ev.tmdb_id);
          break;
        }
        case "request_decided":
        case "request_created":
          requestChanged(ev.request_id, ev.kind, ev.tmdb_id);
          break;
        case "request_failed":
          requestChanged(ev.request_id, ev.kind, null);
          break;
        case "requests_settings_changed":
          requestsSettingsChanged();
          break;
      }
      for (const listener of listeners) listener(ev);
    };

    const connect = () => {
      if (closed) return;
      es = new EventSource(receiverQuery ? `/api/events?${receiverQuery}` : "/api/events");
      es.onopen = () => {
        failures = 0;
        back();
        live(true);
        // Also fires when the browser reconnects on its own.
        if (connectedBefore) resync();
        connectedBefore = true;
      };
      es.onmessage = (msg) => {
        try {
          handle(JSON.parse(String(msg.data)) as ServerEvent);
        } catch {
          // ignore malformed events
        }
      };
      es.onerror = () => {
        dropped();
        live(false);
        // While CONNECTING the browser retries on its own; once CLOSED
        // (e.g. a non-200 response) we have to reconnect manually.
        if (es && es.readyState === EventSource.CLOSED) {
          es.close();
          es = null;
          failures += 1;
          const delay = Math.min(30_000, 2_000 * failures);
          retryTimer = window.setTimeout(connect, delay);
        }
      };
    };

    connect();
    // Taking casts or no longer: the stream opens again saying so. Events
    // missed between the two are caught up as after any reconnect.
    const reopen = () => {
      if (closed || !es) return;
      es.close();
      es = null;
      window.clearTimeout(retryTimer);
      connect();
    };
    receiverChanged.add(reopen);
    return () => {
      closed = true;
      receiverChanged.delete(reopen);
      es?.close();
      window.clearTimeout(retryTimer);
      window.clearTimeout(flushTimer);
      window.clearTimeout(lostTimer);
      window.clearTimeout(summaryTimer);
      window.clearTimeout(requestsTimer);
      setConnectionLost(false);
      setEventsLive(false);
    };
  }, [userId, qc]);
}

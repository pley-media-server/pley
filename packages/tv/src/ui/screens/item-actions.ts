import { errorMessage } from "../../api/client";
import type { Hub, ItemSummary } from "../../api/types";
import { isPlayableVideo } from "../../format/format";
import { t } from "../../i18n";
import type { PlayRequest } from "../../player/plan";
import type { AppState } from "../../state";
import type { Ui } from "../context";
import type { PageKey } from "../shell/navigator";
import { sentence } from "./common";

/** The page an item opens (pageOf): its item page; an artist's or an album's own; a track's album. */
export function pageOf(item: ItemSummary): PageKey {
  switch (item.kind) {
    case "artist":
      return { name: "artist", id: item.id };
    case "album":
      return { name: "album", id: item.id };
    case "track":
      return item.parent_id ? { name: "album", id: item.parent_id, trackId: item.id } : { name: "item", id: item.id };
    default:
      return { name: "item", id: item.id };
  }
}

/** Opens the player for `request`, timed from now (time to first frame is measured from the press). */
export function openPlayer(ui: Ui, request: PlayRequest): void {
  // The same clock as the player's, which measures the time to the first frame from here.
  ui.nav.open({ name: "player", request, pressedAt: typeof performance !== "undefined" ? performance.now() : Date.now() });
}

/**
 * Plays an item: a movie or an episode at once (from where it was left,
 * unless `fromStart`); a show or a season from its next episode.
 */
export async function playItem(app: AppState, ui: Ui, item: ItemSummary, opts: { fromStart?: boolean; fileId?: number | null; queue?: PlayRequest["queue"] } = {}): Promise<void> {
  if (item.kind === "movie" || item.kind === "episode" || item.kind === "extra") {
    openPlayer(ui, { itemId: item.id, fileId: opts.fileId ?? null, fromStart: !!opts.fromStart, queue: opts.queue ?? null });
    return;
  }
  if (item.kind === "show" || item.kind === "season") {
    try {
      const { item: next } = await app.api.value!.onDeck(item.id);
      if (!next) {
        ui.toast(t("browse.actions.nothingToPlay"));
        return;
      }
      openPlayer(ui, { itemId: next.id, fromStart: !!opts.fromStart });
    } catch (e) {
      ui.toast(sentence(errorMessage(e)), true);
    }
    return;
  }
  ui.nav.open(pageOf(item));
}

let prefetching: { id: number; at: number } | null = null;
let prefetchBusy = false;

/**
 * Asks the server to get an item's file ready (POST /api/items/{id}/prefetch)
 * when Play gets focus or focus rests on a movie or an episode: one at a
 * time, and not again for the same item within a minute.
 */
export function prefetchPlayback(app: AppState, item: ItemSummary | null): void {
  if (!item || !isPlayableVideo(item.kind) || item.kind === "show" || item.kind === "season") return;
  const api = app.api.value;
  if (!api || prefetchBusy) return;
  const now = Date.now();
  if (prefetching && prefetching.id === item.id && now - prefetching.at < 60_000) return;
  prefetching = { id: item.id, at: now };
  prefetchBusy = true;
  api.prefetch(item.id).then(
    () => (prefetchBusy = false),
    () => (prefetchBusy = false),
  );
}

/** Home's hubs with `fn` applied to each item of `id` (an optimistic change). */
function patchItems(hubs: Hub[], id: number, fn: (item: ItemSummary) => ItemSummary): Hub[] {
  return hubs.map((h) => (h.items.some((i) => i.id === id) ? { ...h, items: h.items.map((i) => (i.id === id ? fn(i) : i)) } : h));
}

/** Marks an item watched or not, at once on screen, then on the server; Home and the pages listing it follow. */
export async function setWatched(app: AppState, ui: Ui, item: ItemSummary, watched: boolean): Promise<boolean> {
  const api = app.api.value;
  if (!api) return false;
  const before = app.home.value;
  if (before) {
    app.home.value = patchItems(before, item.id, (i) => ({
      ...i,
      user_state: { ...i.user_state, watched, position_ms: 0, play_count: watched ? i.user_state.play_count + 1 : i.user_state.play_count },
      viewed_leaf_count: i.leaf_count != null ? (watched ? i.leaf_count : 0) : i.viewed_leaf_count,
    }));
  }
  try {
    await (watched ? api.markWatched(item.id) : api.markUnwatched(item.id));
    app.changes.value++;
    void app.loadHome();
    return true;
  } catch (e) {
    if (before) app.home.value = before;
    ui.toast(sentence(errorMessage(e)), true);
    return false;
  }
}

/**
 * Removes an item from Continue Watching: an episode takes its whole show
 * with it. At once on screen, then on the server; put back if it refuses.
 */
export async function hideFromContinueWatching(app: AppState, ui: Ui, item: ItemSummary): Promise<boolean> {
  const api = app.api.value;
  if (!api) return false;
  const before = app.home.value;
  if (before) {
    const show = item.kind === "episode" ? item.grandparent_id : null;
    app.home.value = before
      .map((h) => (h.key.indexOf("continue") === 0 ? { ...h, items: h.items.filter((i) => i.id !== item.id && !(show && i.grandparent_id === show)) } : h))
      .filter((h) => h.items.length > 0);
  }
  try {
    await api.hideFromContinueWatching(item.id);
    void app.loadHome();
    return true;
  } catch (e) {
    if (before) app.home.value = before;
    ui.toast(sentence(errorMessage(e)), true);
    return false;
  }
}

/** Adds to or removes from My List: added first in the row (made after Continue Watching when missing), removed at once. */
export async function setInMyList(app: AppState, ui: Ui, item: ItemSummary, add: boolean): Promise<boolean> {
  const api = app.api.value;
  if (!api) return false;
  const before = app.home.value;
  if (before) {
    let hubs = before.map((h) => (h.key === "my_list" ? { ...h, items: h.items.filter((i) => i.id !== item.id) } : h));
    if (add) {
      const at = hubs.findIndex((h) => h.key === "my_list");
      if (at >= 0) hubs[at] = { ...hubs[at]!, items: [item].concat(hubs[at]!.items) };
      else {
        const cw = hubs.findIndex((h) => h.key.indexOf("continue") === 0);
        hubs.splice(cw + 1, 0, { key: "my_list", title: t("browse.home.myList"), library_id: null, items: [item] });
      }
    }
    hubs = hubs.filter((h) => h.items.length > 0);
    app.home.value = hubs;
  }
  try {
    await (add ? api.addToMyList(item.id) : api.removeFromMyList(item.id));
    app.changes.value++;
    void app.loadHome();
    return true;
  } catch (e) {
    if (before) app.home.value = before;
    ui.toast(sentence(errorMessage(e)), true);
    return false;
  }
}

/** Whether Home's My List holds `id` (when Home is known). */
export function inMyList(app: AppState, id: number): boolean | null {
  const hubs = app.home.value;
  if (!hubs) return null;
  const list = hubs.filter((h) => h.key === "my_list")[0];
  return list ? list.items.some((i) => i.id === id) : false;
}

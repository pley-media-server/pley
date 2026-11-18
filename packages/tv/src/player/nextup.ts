import type { Api } from "../api/client";
import type { ItemKind, ItemSummary, Trickplay } from "../api/types";
import { episodeLabel, isPlayableVideo } from "../format/format";
import type { PlayRequest } from "./plan";

/**
 * What plays after an item, fetched fresh so its watch state is today's:
 * the queue's next video when playing through a playlist or collection,
 * else the next episode; movies and extras have none.
 */
export function hasNext(kind: ItemKind, queue: PlayRequest["queue"]): boolean {
  return !!queue || kind === "episode";
}

export async function nextAfter(api: Api, itemId: number, kind: ItemKind, queue: PlayRequest["queue"]): Promise<ItemSummary | null> {
  if (queue?.kind === "playlist") return nextIn((await api.playlist(queue.id)).entries.map((e) => e.item), itemId);
  if (queue?.kind === "collection") return nextIn((await api.collection(queue.id)).items, itemId);
  if (queue?.kind === "merged") return nextIn((await api.mergedCollection(queue.ids)).items, itemId);
  return kind === "episode" ? (await api.next(itemId)).item : null;
}

/** The first video after `itemId` in `items`; `null` when it is the last, or isn't there. */
export function nextIn(items: ItemSummary[], itemId: number): ItemSummary | null {
  const videos = items.filter((i) => isPlayableVideo(i.kind));
  let at = -1;
  for (let i = 0; i < videos.length; i++) if (videos[i]!.id === itemId) at = i;
  return at < 0 ? null : videos[at + 1] ?? null;
}

/** Where `next` starts: from the start when watched, else its resume point. */
export function startOf(next: ItemSummary): number {
  return next.user_state.watched ? 0 : next.user_state.position_ms;
}

/** "S1 · E2" for an episode, `null` otherwise. */
export function labelOf(next: ItemSummary): string | null {
  return next.kind === "episode" ? episodeLabel(next.parent_index, next.index) || null : null;
}

/** Seek previews of a file (`GET /api/files/{id}/trickplay`): `404` until they are made. */
export const TRICKPLAY_RETRY_MS = 20_000;
export const TRICKPLAY_GIVE_UP_MS = 10 * 60_000;

/** Where the thumbnail for `positionMs` is: its sheet and its place in it. */
export function thumbnailAt(info: Trickplay, positionMs: number): { sheet: number; x: number; y: number } {
  const k = Math.max(0, Math.min(info.count - 1, Math.floor(positionMs / info.interval_ms)));
  const perSheet = info.columns * info.rows;
  const sheet = Math.floor(k / perSheet);
  const column = k % info.columns;
  const row = Math.floor(k / info.columns) % info.rows;
  return { sheet, x: column * info.width, y: row * info.height };
}

export function sheetPath(fileId: number, info: Trickplay, sheet: number): string {
  return `/api/files/${fileId}/trickplay/${sheet}.jpg?v=${encodeURIComponent(info.id)}`;
}

import { useCallback } from "react";
import { useNavigate } from "react-router";
import type { QueryClient } from "@tanstack/react-query";
import { api } from "@/api/client";
import { keys } from "@/api/hooks";
import type { Id, ItemDetail, ItemSummary } from "@/api/types";
import { useOptionalAudioPlayer } from "@/audio/AudioPlayer";
import { useToast } from "@/components/Toast";
import { isPlayableVideo } from "./format";
import { shuffled } from "./shuffle";
import { playUrl } from "./usePlayItem";

/**
 * Videos played one after the other (a playlist, a collection). Kept in
 * sessionStorage and named in the player URL (`?queue=<key>&qi=<index>`), so
 * reloading the player keeps its place.
 */
export interface VideoQueue {
  key: string;
  ids: Id[];
  /** Where to go after the last one. */
  returnTo?: string;
}

/** The queue being played and the position in it. */
export interface QueuePosition {
  queue: VideoQueue;
  index: number;
}

const PREFIX = "pley.queue.";

export function loadQueue(key: string | null): VideoQueue | null {
  if (!key) return null;
  try {
    const raw = window.sessionStorage.getItem(PREFIX + key);
    if (!raw) return null;
    const saved = JSON.parse(raw) as Omit<VideoQueue, "key">;
    return Array.isArray(saved.ids) ? { ...saved, key } : null;
  } catch {
    return null;
  }
}

function saveQueue(queue: Omit<VideoQueue, "key">): VideoQueue {
  const key = `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
  try {
    // One queue per tab: starting another replaces it.
    for (let i = window.sessionStorage.length - 1; i >= 0; i--) {
      const k = window.sessionStorage.key(i);
      if (k?.startsWith(PREFIX)) window.sessionStorage.removeItem(k);
    }
    window.sessionStorage.setItem(PREFIX + key, JSON.stringify(queue));
  } catch {
    // storage unavailable: the first video still plays, alone
  }
  return { ...queue, key };
}

/** Player URL of entry `index` (item `id`) of the queue. */
export function queuePlayUrl(queue: VideoQueue, index: number, id: Id, startMs?: number): string {
  const url = playUrl(id, { startMs });
  return `${url}${url.includes("?") ? "&" : "?"}queue=${encodeURIComponent(queue.key)}&qi=${index}`;
}

/** The entry after `position`, loaded fresh (for its watch state). */
export async function nextInQueue(qc: QueryClient, position: QueuePosition): Promise<ItemSummary | null> {
  const id = position.queue.ids[position.index + 1];
  if (id == null) return null;
  return qc.fetchQuery({ queryKey: keys.item(id), queryFn: () => api.get<ItemDetail>(`/api/items/${id}`), staleTime: 0 });
}

/**
 * Returns `start(items, opts)`: plays the movies and episodes of `items` in
 * order. It returns whether one plays; when none does, it has said so.
 */
export function useVideoQueue() {
  const navigate = useNavigate();
  const audio = useOptionalAudioPlayer();
  const toast = useToast();
  return useCallback(
    (items: ItemSummary[], opts: { startIndex?: number; shuffle?: boolean; returnTo?: string } = {}): boolean => {
      const start = items[opts.startIndex ?? 0];
      let videos = items.filter(isPlayableVideo);
      if (opts.shuffle) videos = shuffled(videos);
      else if (start) videos = videos.slice(Math.max(0, videos.indexOf(start)));
      const first = videos[0];
      if (!first) {
        toast.info("Nothing to play");
        return false;
      }
      const queue = saveQueue({ ids: videos.map((v) => v.id), returnTo: opts.returnTo });
      audio?.pause();
      void navigate(queuePlayUrl(queue, 0, first.id, first.user_state.watched ? 0 : first.user_state.position_ms));
      return true;
    },
    [audio, navigate, toast],
  );
}

import { useCallback } from "react";
import { useNavigate } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { errorMessage } from "@/api/client";
import { fetchChildren, fetchOnDeck } from "@/api/hooks";
import type { Id, ItemSummary } from "@/api/types";
import { useOptionalAudioPlayer } from "@/audio/AudioPlayer";
import { useToast } from "@/components/Toast";
import { encodeId } from "./ids";

export interface PlayUrlOptions {
  startMs?: number;
  fileId?: Id | null;
  audio?: number | null;
  sub?: number | null;
}

/** `/play/:id?t=&file=&audio=&sub=` */
export function playUrl(id: Id, opts: PlayUrlOptions = {}): string {
  const p = new URLSearchParams();
  if (opts.startMs && opts.startMs > 0) p.set("t", String(Math.floor(opts.startMs)));
  if (opts.fileId != null) p.set("file", encodeId(opts.fileId));
  if (opts.audio != null) p.set("audio", String(opts.audio));
  if (opts.sub != null) p.set("sub", String(opts.sub));
  const q = p.toString();
  return `/play/${encodeId(id)}${q ? `?${q}` : ""}`;
}

export interface PlayOptions extends Omit<PlayUrlOptions, "startMs"> {
  fromStart?: boolean;
  shuffle?: boolean;
}

/**
 * Returns `play(item)` which does the right thing for every kind:
 * video → player page (resuming), show/season → on-deck episode,
 * album/artist/track → global audio queue. It resolves to whether
 * something plays; when nothing does, it has said why.
 */
export function usePlayItem() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const audio = useOptionalAudioPlayer();
  const toast = useToast();

  return useCallback(
    async (item: ItemSummary, opts: PlayOptions = {}): Promise<boolean> => {
      try {
        switch (item.kind) {
          case "movie":
          case "episode":
          case "extra": {
            const startMs = opts.fromStart ? 0 : item.user_state.watched ? 0 : item.user_state.position_ms;
            audio?.pause();
            void navigate(playUrl(item.id, { startMs, fileId: opts.fileId, audio: opts.audio, sub: opts.sub }));
            return true;
          }
          case "show":
          case "season": {
            const ep = await fetchOnDeck(qc, item.id);
            if (!ep) {
              toast.info("Nothing to play yet");
              return false;
            }
            audio?.pause();
            void navigate(playUrl(ep.id, { startMs: opts.fromStart ? 0 : ep.user_state.position_ms }));
            return true;
          }
          case "album": {
            const tracks = await fetchChildren(qc, item.id);
            if (!tracks.length) {
              toast.info("This album has no tracks");
              return false;
            }
            audio?.playQueue(tracks, 0, { shuffle: opts.shuffle });
            return true;
          }
          case "artist": {
            const albums = await fetchChildren(qc, item.id);
            const lists = await Promise.all(albums.map((a) => fetchChildren(qc, a.id)));
            const tracks = lists.flat();
            if (!tracks.length) {
              toast.info("No tracks found for this artist");
              return false;
            }
            audio?.playQueue(tracks, 0, { shuffle: opts.shuffle });
            return true;
          }
          case "track": {
            if (item.parent_id) {
              const tracks = await fetchChildren(qc, item.parent_id);
              const i = tracks.findIndex((t) => t.id === item.id);
              audio?.playQueue(tracks.length ? tracks : [item], Math.max(0, i));
            } else {
              audio?.playQueue([item], 0);
            }
            return true;
          }
        }
      } catch (err) {
        toast.error(errorMessage(err));
      }
      return false;
    },
    [audio, navigate, qc, toast],
  );
}

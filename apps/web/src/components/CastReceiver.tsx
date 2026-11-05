import { useEffect, useEffectEvent } from "react";
import { useLocation, useNavigate } from "react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api, errorMessage } from "@/api/client";
import { onServerEvent, setReceiver } from "@/api/events";
import { queries } from "@/api/hooks";
import type { PlaylistDetail } from "@/api/lists";
import { liveQueries } from "@/api/livetv";
import type { ServerEvent } from "@/api/types";
import { useOptionalAudioPlayer } from "@/audio/AudioPlayer";
import { receiverId, receiverQuery, takesCasts } from "@/lib/castReceiver";
import { playUrl, usePlayItem } from "@/lib/usePlayItem";
import { playlistUrl } from "@/lib/urls";
import { useVideoQueue } from "@/lib/videoQueue";
import { liveUrl } from "@/pages/livetv/common";
import { useToast } from "./Toast";

type Cast = Extract<ServerEvent, { type: "cast" }>;

/**
 * In the desktop app, this device takes casts (docs/API.md, "Casting"): its
 * event stream opens as a receiver, and what a remote sends it plays as its
 * own Play would, in place of what plays, saying where it came from. Live
 * TV's channels are taken while the user has them. A browser tab takes
 * none (see `takesCasts`).
 */
export function CastReceiver() {
  const desktop = takesCasts();
  const summary = useQuery({ ...liveQueries.summary(), enabled: desktop });
  const live = summary.data?.available ?? false;
  useEffect(() => {
    if (!desktop) return;
    setReceiver(receiverQuery(live));
    return () => setReceiver(null);
  }, [desktop, live]);

  const receive = useCastPlayer();
  const location = useLocation();
  const onCast = useEffectEvent((ev: ServerEvent) => {
    if (ev.type !== "cast" || !desktop || ev.receiver_id !== receiverId()) return;
    // Choosing a profile (behind its PIN, maybe) comes first: a cast doesn't get past it.
    if (location.pathname.startsWith("/profiles")) return;
    void receive(ev);
  });
  useEffect(() => onServerEvent(onCast), []);
  return null;
}

/** Plays a cast as the page's own Play would: a title, a show's next episode, music, a playlist or a channel. */
function useCastPlayer() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const play = usePlayItem();
  const audio = useOptionalAudioPlayer();
  const startQueue = useVideoQueue();
  const toast = useToast();
  return async (cast: Cast) => {
    const from = `Playing from ${cast.from ?? "another device"}`;
    try {
      if (cast.channel_id != null) {
        const channel = await qc.fetchQuery(liveQueries.channel(cast.channel_id));
        audio?.pause();
        void navigate(liveUrl(channel));
      } else if (cast.playlist_id != null) {
        const playlist = await api.get<PlaylistDetail>(`/api/playlists/${cast.playlist_id}`);
        const items = playlist.entries.map((e) => e.item);
        if (!items.length) return;
        if (playlist.kind === "video") startQueue(items, { shuffle: cast.shuffle, returnTo: playlistUrl(playlist.id) });
        else audio?.playQueue(items, 0, { shuffle: cast.shuffle });
      } else if (cast.item_id != null) {
        const item = await qc.fetchQuery(queries.item(cast.item_id));
        if (item.kind === "movie" || item.kind === "episode") {
          // Where the remote said, else where its own Play would; with the tracks picked there.
          const startMs = cast.start_ms ?? (item.user_state.watched ? 0 : item.user_state.position_ms);
          audio?.pause();
          void navigate(playUrl(item.id, { startMs, audio: cast.audio_index ?? null, sub: cast.subtitle_index ?? null }));
        } else {
          await play(item, { shuffle: cast.shuffle });
        }
      } else {
        return;
      }
      toast.info(from);
    } catch (err) {
      toast.error(errorMessage(err));
    }
  };
}

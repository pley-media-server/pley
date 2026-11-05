import { useEffect, useEffectEvent, useRef } from "react";
import { useLocation, useNavigate } from "react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { api, errorMessage } from "@/api/client";
import { onServerEvent, setReceiver } from "@/api/events";
import { queries, useSwitchProfile } from "@/api/hooks";
import type { PlaylistDetail } from "@/api/lists";
import { liveQueries } from "@/api/livetv";
import type { ServerEvent } from "@/api/types";
import { useOptionalAudioPlayer } from "@/audio/AudioPlayer";
import { useAuth } from "@/auth";
import {
  castState,
  dropHandedOver,
  handOver,
  receiverId,
  receiverQuery,
  switchesProfile,
  takeHandedOver,
  takesCasts,
  type Cast,
} from "@/lib/castReceiver";
import { holdReports, sessionsEnded } from "@/lib/progress";
import { playUrl, usePlayItem } from "@/lib/usePlayItem";
import { itemUrl, playlistUrl } from "@/lib/urls";
import { useVideoQueue } from "@/lib/videoQueue";
import { liveUrl } from "@/pages/livetv/common";
import { useToast } from "./Toast";

/**
 * In the desktop app, this device takes casts (docs/API.md, "Casting"): its
 * event stream opens as a receiver, and what a remote sends it plays as its
 * own Play would, in place of what plays, saying where it came from. Live
 * TV's channels are taken while the user has them. A browser tab takes
 * none (see `takesCasts`).
 *
 * A cast from another profile of the household switches this device to
 * that profile first, with the cast's ticket. The switch starts the app
 * over as that profile, this receiver too (it is the profile's own): the
 * cast is handed over to the new one, which plays it once. Other casts
 * heard until the switch is done are dropped.
 */
export function CastReceiver() {
  const desktop = takesCasts();
  const me = useAuth();
  const summary = useQuery({ ...liveQueries.summary(), enabled: desktop });
  const live = summary.data?.available ?? false;
  useEffect(() => {
    if (!desktop) return;
    setReceiver(receiverQuery(live));
    return () => setReceiver(null);
  }, [desktop, live]);

  const receive = useCastPlayer();
  const switchTo = useCastSwitch();
  const location = useLocation();
  const onCast = useEffectEvent((ev: ServerEvent) => {
    if (ev.type !== "cast" || !desktop || ev.receiver_id !== receiverId()) return;
    // While a switch is under way, what plays is the cast it switches for, once it is done: others are dropped.
    if (switching) return;
    // Choosing a profile (behind its PIN, maybe) comes first: a cast doesn't get past it.
    if (location.pathname.startsWith("/profiles")) return;
    if (switchesProfile(ev, me.id)) {
      // Without a ticket there is no switching to it: it isn't for this device as it is.
      if (ev.switch_ticket) void switchTo(ev);
      return;
    }
    void receive(ev, PLAYER_PAGE.test(location.pathname));
  });
  useEffect(() => onServerEvent(onCast), []);

  // A cast the previous profile's receiver switched to this one for, having closed what played there.
  const handedOver = useEffectEvent(() => {
    const cast = takeHandedOver(me.id);
    if (cast) void receive(cast, PLAYER_PAGE.test(location.pathname));
  });
  useEffect(() => {
    if (desktop) handedOver();
  }, [desktop]);
  return null;
}

/**
 * Whether a cast's switch to another profile is under way, from its start
 * until the switch is done: casts heard meanwhile are dropped (`onCast`).
 * Module-wide, as the switch outlives the receiver that started it.
 */
let switching = false;

/** How long a switch waits for what played here to have ended, as the profile that played. */
const ENDED_WITHIN_MS = 5_000;

/**
 * Switches this device to the profile `cast` came from, with its ticket,
 * having handed the cast over to that profile's receiver. What plays here
 * is closed first, and the switch waits up to 5 seconds for its last
 * reports to be answered, sent with the profile that played; what hasn't
 * gone by then is dropped (`holdReports`): sent with the new profile's
 * session, it would count for that profile. A switch that doesn't go
 * through says so, and nothing plays.
 */
function useCastSwitch() {
  const switchProfile = useSwitchProfile();
  const closePlayers = useClosePlayers();
  const toast = useToast();
  const { t } = useTranslation("components");
  return async (cast: Cast) => {
    if (cast.profile_id == null || !cast.switch_ticket || switching) return;
    switching = true;
    handOver(cast);
    let release: () => void = () => undefined;
    try {
      await closePlayers(ENDED_WITHIN_MS);
      release = holdReports();
      await switchProfile.mutateAsync({ user_id: cast.profile_id, ticket: cast.switch_ticket });
    } catch {
      dropHandedOver(cast);
      toast.error(t("castReceiver.switchFailed"));
    } finally {
      release();
      switching = false;
    }
  };
}

/**
 * Closes what plays here: a player's page, for the home page, and music.
 * Resolves once their last reports ("stopped", and the session's release)
 * are answered (`sessionsEnded`), or `withinMs` from now at most.
 */
function useClosePlayers() {
  const navigate = useNavigate();
  const audio = useOptionalAudioPlayer();
  const onPlayer = PLAYER_PAGE.test(useLocation().pathname);
  // Waiting for the player's page to go: a player ends its session as it unmounts,
  // which is done before this receiver's effects of the same render run.
  const left = useRef<(() => void) | null>(null);
  useEffect(() => {
    if (onPlayer) return;
    left.current?.();
    left.current = null;
  }, [onPlayer]);
  return async (withinMs: number) => {
    const until = Date.now() + withinMs;
    audio?.close();
    if (onPlayer) {
      await new Promise<void>((resolve) => {
        const before = left.current;
        left.current = () => {
          before?.();
          resolve();
        };
        void navigate("/", { replace: true });
      });
    }
    await sessionsEnded(Math.max(0, until - Date.now()));
  };
}

/** A player's page (a title's, a channel's), which fills the window. */
const PLAYER_PAGE = /^\/(play|live)\/./;
const MUSIC = new Set(["album", "artist", "track"]);

/**
 * Plays a cast as the page's own Play would: a title, a show's next
 * episode, music, a playlist or a channel. Music sent while a player is
 * open (`onPlayer`) leaves it for the page that Play is on: the video would
 * otherwise go on playing over the music.
 */
function useCastPlayer() {
  const { t } = useTranslation("components");
  const navigate = useNavigate();
  const qc = useQueryClient();
  const play = usePlayItem();
  const audio = useOptionalAudioPlayer();
  const startQueue = useVideoQueue();
  const toast = useToast();
  return async (cast: Cast, onPlayer: boolean) => {
    const from = t("castReceiver.playingFrom", { device: cast.from ?? t("castReceiver.anotherDevice") });
    try {
      // When nothing plays (a show with nothing next, say), the page said why: no "Playing from".
      let played = true;
      if (cast.channel_id != null) {
        const channel = await qc.fetchQuery(liveQueries.channel(cast.channel_id));
        audio?.pause();
        void navigate(liveUrl(channel));
      } else if (cast.playlist_id != null) {
        const playlist = await api.get<PlaylistDetail>(`/api/playlists/${cast.playlist_id}`);
        const items = playlist.entries.map((e) => e.item);
        if (!items.length) return;
        if (playlist.kind === "video") {
          played = startQueue(items, { shuffle: cast.shuffle, returnTo: playlistUrl(playlist.id) });
        } else {
          audio?.playQueue(items, 0, { shuffle: cast.shuffle });
          if (onPlayer) void navigate(playlistUrl(playlist.id));
        }
      } else if (cast.item_id != null) {
        const item = await qc.fetchQuery(queries.item(cast.item_id));
        if (item.kind === "movie" || item.kind === "episode") {
          // Where the remote said, else where its own Play would; with the tracks picked there.
          const startMs = cast.start_ms ?? (item.user_state.watched ? 0 : item.user_state.position_ms);
          audio?.pause();
          const url = playUrl(item.id, { startMs, audio: cast.audio_index ?? null, sub: cast.subtitle_index ?? null });
          void navigate(url, { state: castState() });
        } else {
          played = await play(item, { shuffle: cast.shuffle });
          if (played && onPlayer && MUSIC.has(item.kind)) {
            void navigate(itemUrl(item.kind === "track" && item.parent_id ? item.parent_id : item.id));
          }
        }
      } else {
        return;
      }
      if (played) toast.info(from);
    } catch (err) {
      toast.error(errorMessage(err));
    }
  };
}

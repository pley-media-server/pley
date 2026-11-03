import { useEffect, useEffectEvent, type RefObject } from "react";
import { useLocation, useNavigate } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { putCastState, type CastReport } from "@/api/cast";
import { onServerEvent } from "@/api/events";
import { queries } from "@/api/hooks";
import type { CastState, Id, ServerEvent } from "@/api/types";
import { useLatest } from "@/lib/hooks";
import { playUrl } from "@/lib/usePlayItem";
import type { CastSelf } from "./device";

/**
 * Being cast to (the desktop app): the profile's other devices send this
 * one a title to play (`cast_play`) and steer its video player
 * (`cast_control`), and it says what it plays (docs/API.md, "Casting to
 * another device"). Only the video player: music and Live TV are left as
 * they are.
 */

/** How often a playing player says where it is. */
export const REPORT_INTERVAL_MS = 10_000;
/** A report the server doesn't answer within this long is let go: the next one goes. */
const REPORT_TIMEOUT_MS = 10_000;

/** The open video player, as cast commands steer it. */
export interface CastPlayer {
  pause: () => void;
  resume: () => void;
  seek: (positionMs: number) => void;
  /** Leaves the player, as its Back does. */
  stop: () => void;
}

/** The video player open now, if any. */
let openPlayer: CastPlayer | null = null;

/** Reports go one after the other, so the server hears them in order (a title's end before the next one's start). */
let reports: Promise<void> = Promise.resolve();

/** Says what this device plays: fire and forget, failures ignored. */
export function reportCast(self: CastSelf, report: CastReport) {
  reports = reports.then(() => {
    const signal = typeof AbortSignal.timeout === "function" ? AbortSignal.timeout(REPORT_TIMEOUT_MS) : undefined;
    return putCastState(self.id, report, signal).catch(() => undefined);
  });
}

/** Settles once the reports made so far have gone (or failed). */
export function castReportsSent(): Promise<void> {
  return reports;
}

/** Where a player sent a title plays from: the position sent, or where the profile left off (from the start once watched). */
async function startOf(qc: ReturnType<typeof useQueryClient>, itemId: Id, positionMs: number | null): Promise<number> {
  if (positionMs !== null) return positionMs;
  try {
    const item = await qc.fetchQuery({ ...queries.item(itemId), staleTime: 0 });
    return item.user_state.watched ? 0 : item.user_state.position_ms;
  } catch {
    // The player says what's wrong with the title.
    return 0;
  }
}

/** A player sent a title opens afresh, even on the title it plays (the page's `cast` state tells them apart). */
let plays = 0;

/**
 * Carries out the commands sent to this device (`self`; `null`: not one to
 * cast to). Mounted once, for the signed-in profile.
 */
export function useCastCommands(self: CastSelf | null) {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const path = useLatest(useLocation().pathname);
  const selfId = self?.id ?? null;

  const onEvent = useEffectEvent((ev: ServerEvent) => {
    if (selfId === null) return;
    if (ev.type === "cast_play" && ev.device_id === selfId) {
      void startOf(qc, ev.item_id, ev.position_ms).then((startMs) => {
        // What plays makes way, as its Back would: the new title takes its place in the history.
        const playing = /^\/(play|live)\//.test(path.current);
        plays += 1;
        void navigate(playUrl(ev.item_id, { startMs, fileId: ev.file_id }), { replace: playing, state: { cast: plays } });
      });
    } else if (ev.type === "cast_control" && ev.device_id === selfId) {
      const player = openPlayer;
      if (!player) return;
      switch (ev.action) {
        case "pause":
          player.pause();
          break;
        case "resume":
          player.resume();
          break;
        case "seek":
          if (ev.position_ms !== null) player.seek(ev.position_ms);
          break;
        case "stop":
          player.stop();
          break;
      }
    }
  });
  useEffect(() => onServerEvent(onEvent), []);
}

/** Which title a player sent by cast is (its page's `cast` state): a new one opens a new player. */
export function castPlayKey(state: unknown): number | null {
  const cast = (state as { cast?: unknown } | null)?.cast;
  return typeof cast === "number" ? cast : null;
}

export interface CastPlayerOptions {
  itemId: Id;
  video: RefObject<HTMLVideoElement | null>;
  /** Where the player is, and how long the title is (ms; `undefined`: not known yet). */
  position: () => { position_ms: number; duration_ms: number | undefined };
  controls: CastPlayer;
}

/**
 * The video player of a device that may be cast to (`self`): steered by
 * cast commands while it is open, cast or not, and saying what it plays as
 * it starts (buffering, then playing), pauses, resumes, seeks and waits,
 * every 10 s while it plays, and that it plays nothing once it closes or
 * the title ends.
 */
export function useCastPlayer(self: CastSelf | null, { itemId, video, position, controls }: CastPlayerOptions) {
  const latest = useLatest({ position, controls });
  const selfId = self?.id ?? null;
  useEffect(() => {
    const v = video.current;
    if (!self || !v) return;
    const send = (state: CastState["state"] | null) => {
      if (state === null) {
        reportCast(self, { item_id: null });
        return;
      }
      const { position_ms, duration_ms } = latest.current.position();
      reportCast(self, { item_id: itemId, position_ms: Math.max(0, Math.round(position_ms)), duration_ms, state });
    };
    const player: CastPlayer = {
      pause: () => latest.current.controls.pause(),
      resume: () => latest.current.controls.resume(),
      seek: (ms) => latest.current.controls.seek(ms),
      stop: () => latest.current.controls.stop(),
    };
    openPlayer = player;
    let over = false;
    send("buffering");
    const onPlaying = () => {
      over = false;
      send("playing");
    };
    const onPause = () => {
      if (!v.ended) send("paused");
    };
    const onWaiting = () => send("buffering");
    const onSeeked = () => send(v.paused ? "paused" : "playing");
    const onEnded = () => {
      over = true;
      send(null);
    };
    v.addEventListener("playing", onPlaying);
    v.addEventListener("pause", onPause);
    v.addEventListener("waiting", onWaiting);
    v.addEventListener("seeked", onSeeked);
    v.addEventListener("ended", onEnded);
    const timer = window.setInterval(() => {
      if (!v.paused && !v.ended) send("playing");
    }, REPORT_INTERVAL_MS);
    return () => {
      v.removeEventListener("playing", onPlaying);
      v.removeEventListener("pause", onPause);
      v.removeEventListener("waiting", onWaiting);
      v.removeEventListener("seeked", onSeeked);
      v.removeEventListener("ended", onEnded);
      window.clearInterval(timer);
      if (openPlayer === player) openPlayer = null;
      if (!over) send(null);
    };
    // One player per title: the element and the options' latest values stay with it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selfId, itemId]);
}

import type { Control, ServerEvent } from "../api/types";

/**
 * The user's remote, on another of their devices (docs/API.md, "Remote
 * control"): what each player here obeys, and what it does with a
 * command. The TV obeys and shows no remote of its own. The rules are the
 * web's and the Android app's, pinned for every TV app by the shared
 * fixture (test/fixtures/remote.json).
 */

export type PlayerCommand = Extract<ServerEvent, { type: "player_command" }>;

/** "Previous" this far into a track starts it again, as the music's own button does. */
const RESTART_AFTER_MS = 3_000;

const VIDEO_CONTROLS: Control[] = ["play", "pause", "seek", "skip"];
export const MUSIC_CONTROLS: Control[] = ["play", "pause", "seek", "skip", "next", "previous"];
/** Live is live here: no pause or seek, only channel up and down. */
const LIVE_CONTROLS: Control[] = ["next", "previous"];

/** What a video obeys: next only when something may come after it. */
export function videoControls(hasNext: boolean): Control[] {
  return hasNext ? [...VIDEO_CONTROLS, "next"] : VIDEO_CONTROLS.slice();
}

/** What a live channel obeys: channel up and down when the player has an order to go through, else nothing but Stop. */
export function liveControls(hasOrder: boolean): Control[] {
  return hasOrder ? LIVE_CONTROLS.slice() : [];
}

/** What a player is doing, as much as a command needs. */
export type RemotePlayer =
  | {
      kind: "video" | "music";
      /** The session it plays now; `null` while it has none. */
      session: string | null;
      playing: boolean;
      positionMs: number;
      /** `null` while the length isn't known. */
      durationMs: number | null;
      /** Something may come after it (an episode, the queue's next). */
      hasNext: boolean;
      /** The track's place in the music's play order. */
      index?: number;
    }
  | { kind: "live"; session: string | null; hasOrder: boolean };

/**
 * What the player does with a command. A play, pause or seek tells the
 * server where it is at once, so the remote sees it (a pause reports as
 * any pause does); another track or channel tells it with its own session.
 * A play while playing, or a pause while paused, changes nothing but says
 * where it is again (`report`), so a remote a little behind settles.
 */
export type RemoteAction =
  | { act: "none" }
  | { act: "report" }
  | { act: "play" | "pause" }
  | { act: "seek"; positionMs: number }
  | { act: "next" | "previous" }
  | { act: "channel"; by: 1 | -1 };

const NONE: RemoteAction = { act: "none" };
const REPORT: RemoteAction = { act: "report" };

/**
 * The rule every receiver follows: a command for another session is
 * someone else's player's; play while playing and pause while paused
 * say again what it does; a seek stays within the title and a skip
 * counts from where it is; next with nothing next, or previous on a
 * video, does nothing; previous on music starts the track again past 3 s
 * or on the first track, else plays the one before; a live channel's
 * next and previous are channel up and down. A command the player didn't
 * declare does nothing.
 */
export function remoteAction(p: RemotePlayer, c: PlayerCommand): RemoteAction {
  if (p.session === null || c.session_id !== p.session) return NONE;
  if (p.kind === "live") {
    if (!p.hasOrder) return NONE;
    if (c.command === "next") return { act: "channel", by: 1 };
    if (c.command === "previous") return { act: "channel", by: -1 };
    return NONE;
  }
  switch (c.command) {
    case "play":
      return p.playing ? REPORT : { act: "play" };
    case "pause":
      return p.playing ? { act: "pause" } : REPORT;
    case "seek":
      return c.position_ms === null ? NONE : { act: "seek", positionMs: within(c.position_ms, p.durationMs) };
    case "skip":
      return c.offset_ms === null ? NONE : { act: "seek", positionMs: within(p.positionMs + c.offset_ms, p.durationMs) };
    case "next":
      return p.hasNext ? { act: "next" } : NONE;
    case "previous":
      if (p.kind === "video") return NONE;
      return restartsTrack(p.positionMs, p.index ?? 0) ? { act: "seek", positionMs: 0 } : { act: "previous" };
    default:
      // A command a newer server knows.
      return NONE;
  }
}

/** Previous starts the track again (rather than playing the one before) past 3 s, or on the first track. */
export function restartsTrack(positionMs: number, index: number): boolean {
  return positionMs > RESTART_AFTER_MS || index === 0;
}

/** From the start to the end; past the start only while the length isn't known. */
function within(ms: number, durationMs: number | null): number {
  const start = Math.max(0, ms);
  return durationMs !== null && durationMs > 0 ? Math.min(start, durationMs) : start;
}

/**
 * The place `by` from `at` in an order of `total` channels, round its
 * ends; the first when the channel isn't in it (tuned by its number, say);
 * `null` when it is empty.
 */
export function channelStep(at: number | null, by: number, total: number): number | null {
  if (total <= 0) return null;
  if (at === null) return 0;
  return (((at + by) % total) + total) % total;
}

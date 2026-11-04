import type { Control, PlayingSession } from "@/api/types";

/** How long a remote waits for a player to show it acted on a command. */
export const ANSWER_WAIT_MS = 5_000;

/** How far the remote's skip buttons go, back and forward. */
export const SKIP_BACK_MS = 10_000;
export const SKIP_FORWARD_MS = 30_000;

/**
 * Where `session` is at `nowMs`: its last report, moved on by the time since
 * while it plays (never past its end, nor back for a clock that is behind).
 */
export function estimatedPosition(session: PlayingSession, nowMs: number): number {
  if (session.state !== "playing") return session.position_ms;
  const at = session.position_ms + Math.max(0, nowMs - session.updated_at * 1000);
  return session.duration_ms != null ? Math.min(at, session.duration_ms) : at;
}

/** A command sent, and what its session looked like then. */
export interface SentCommand {
  command: Control;
  before: PlayingSession;
  /** This browser's clock, as `nowMs` given to `commandAnswered` is. */
  sentAt: number;
  /** `seek`, `skip` and a `previous` that starts the title over: where it should be once the player acted. */
  target?: number;
}

/** Farther than this from where it should be, a report has moved (the server's rule for a seek). */
const JUMP_MS = 5_000;

/**
 * Whether the list shows the player acted on `sent`, by `nowMs`: it is gone
 * (next and previous end a session to start the next), plays something
 * else, changed state, or, for a seek, a skip or a previous on the same
 * title, has reported since from somewhere new or where it was sent to.
 *
 * Server times are only compared with server times, and this browser's with
 * its own: the two clocks can be seconds apart.
 */
export function commandAnswered(sent: SentCommand, now: PlayingSession | undefined, nowMs: number): boolean {
  const { before } = sent;
  if (!now) return true;
  if (now.item?.id !== before.item?.id || now.channel?.id !== before.channel?.id) return true;
  if (now.state !== before.state) return true;
  if (sent.command !== "seek" && sent.command !== "skip" && sent.command !== "previous") return false;
  const reportedSince = now.updated_at > before.updated_at || now.position_ms !== before.position_ms;
  if (!reportedSince) return false;
  const moved = Math.abs(now.position_ms - estimatedPosition(before, now.updated_at * 1000)) > JUMP_MS;
  // It may have played on from the target since, for as long as the command has been out.
  const there = sent.target != null && Math.abs(now.position_ms - sent.target) <= JUMP_MS + Math.max(0, nowMs - sent.sentAt);
  return moved || there;
}

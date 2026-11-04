import type { Control, PlayingSession } from "@/api/types";

/** How long a remote waits for a player to show it acted on a command. */
export const ANSWER_WAIT_MS = 5_000;

/** How far the remote's skip buttons go, back and forward. */
export const SKIP_BACK_MS = 10_000;
export const SKIP_FORWARD_MS = 30_000;

/** A session as the remote keeps it: when its player last reported, by this browser's clock. */
export interface RemoteSession extends PlayingSession {
  reported_at_ms: number;
}

/**
 * The list `GET /api/sessions` answered at `nowMs`, this browser's clock.
 * The server says how long ago each report was rather than when, so this
 * browser never compares its clock with the server's.
 */
export function received(sessions: PlayingSession[], nowMs: number): RemoteSession[] {
  return sessions.map((s) => ({ ...s, reported_at_ms: nowMs - s.age_ms }));
}

/** Where `session` is at `nowMs`: its last report, moved on by the time since while it plays (never past its end). */
export function estimatedPosition(session: RemoteSession, nowMs: number): number {
  if (session.state !== "playing") return session.position_ms;
  const at = session.position_ms + Math.max(0, nowMs - session.reported_at_ms);
  return session.duration_ms != null ? Math.min(at, session.duration_ms) : at;
}

/** A command sent, and what its session looked like then. */
export interface SentCommand {
  command: Control;
  before: RemoteSession;
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
 * its own (`reported_at_ms`): the two clocks can be seconds apart.
 */
export function commandAnswered(sent: SentCommand, now: RemoteSession | undefined, nowMs: number): boolean {
  const { before } = sent;
  if (!now) return true;
  if (now.item?.id !== before.item?.id || now.channel?.id !== before.channel?.id) return true;
  if (now.state !== before.state) return true;
  if (sent.command !== "seek" && sent.command !== "skip" && sent.command !== "previous") return false;
  const reportedSince = now.updated_at > before.updated_at || now.position_ms !== before.position_ms;
  if (!reportedSince) return false;
  const moved = Math.abs(now.position_ms - estimatedPosition(before, now.reported_at_ms)) > JUMP_MS;
  // It may have played on from the target since, for as long as the command has been out.
  const there = sent.target != null && Math.abs(now.position_ms - sent.target) <= JUMP_MS + Math.max(0, nowMs - sent.sentAt);
  return moved || there;
}

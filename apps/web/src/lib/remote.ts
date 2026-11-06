import { useSyncExternalStore } from "react";
import type { CastKind, Control, DeviceKind, PlayingSession, RemoteDevice, TrackOption } from "@/api/types";
import { t } from "@/i18n";
import { streamLabel } from "./format";

/** How long a remote waits for a player to show it acted on a command. */
export const ANSWER_WAIT_MS = 5_000;

/** How far the remote's skip buttons go, back and forward. */
export const SKIP_BACK_MS = 30_000;
export const SKIP_FORWARD_MS = 30_000;


/** This browser's clock, in milliseconds: the one `reported_at_ms` and a command's `sentAt` are on. */
export const remoteClock = (): number => Date.now();

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

/**
 * One of the user's other devices, as the remote lists it: one that takes
 * casts (a receiver), one that plays something, or both, by `device_id`.
 * A receiver another profile of the household is on is listed too, by its
 * profile (`profile`), without what it plays.
 */
export interface OtherDevice {
  /** Its `device_id`. */
  id: string;
  /** What the remote calls it: two of the same name are numbered ("Living room TV (2)"). */
  name: string;
  kind: DeviceKind;
  /** Where casts go; null for a device that plays without taking them (a Jellyfin or Plex app). */
  receiver: RemoteDevice | null;
  /** What it plays: its newest session; null while it plays nothing. */
  session: RemoteSession | null;
  /** The household profile it is on, when it isn't the user's: a cast switches it to theirs. */
  profile: NonNullable<RemoteDevice["profile"]> | null;
}

/**
 * The devices of `receivers` and `sessions` (oldest first, as the server
 * lists them), each once: those that play first, then the user's own, then
 * those on another profile, each by name. A device with several sessions
 * shows its newest. Two of the same name are numbered, so every label and
 * message says which one it means.
 */
export function otherDevices(receivers: RemoteDevice[], sessions: RemoteSession[]): OtherDevice[] {
  const byId = new Map<string, { receiver: RemoteDevice | null; session: RemoteSession | null }>();
  for (const receiver of receivers) byId.set(receiver.device_id, { receiver, session: null });
  for (const session of sessions) {
    const known = byId.get(session.device_id);
    if (known) known.session = session;
    else byId.set(session.device_id, { receiver: null, session });
  }
  const devices = [...byId].map(([id, { receiver, session }]) => ({
    id,
    // A device the server couldn't name.
    name: receiver?.name ?? session?.device ?? t("remote.unknownDevice"),
    kind: receiver?.kind ?? session?.device_kind ?? "other",
    receiver,
    session,
    profile: receiver?.profile ?? null,
  }));
  devices.sort(
    (a, b) =>
      Number(!a.session) - Number(!b.session) || Number(!!a.profile) - Number(!!b.profile) || a.name.localeCompare(b.name),
  );
  const total = new Map<string, number>();
  for (const d of devices) total.set(d.name, (total.get(d.name) ?? 0) + 1);
  const seen = new Map<string, number>();
  return devices.map((d) => {
    const n = (seen.get(d.name) ?? 0) + 1;
    seen.set(d.name, n);
    return (total.get(d.name) ?? 0) > 1 ? { ...d, name: `${d.name} (${n})` } : d;
  });
}

/** The devices that take `kind` when cast to, in the list's order. */
export function castTargets(devices: OtherDevice[], kind: CastKind): OtherDevice[] {
  return devices.filter((d) => d.receiver?.casts.includes(kind));
}

/** A page's wish for the header's remote to open on a device: one cast just went there. */
export interface RemoteAsk {
  deviceId: string;
}

let ask: RemoteAsk | null = null;
const askListeners = new Set<() => void>();

function asked(next: RemoteAsk | null) {
  ask = next;
  for (const listener of askListeners) listener();
}

/**
 * Opens the header's remote on `deviceId`: at once, or as the header shows
 * again (a player, which hides it, casts and leaves).
 */
export function openRemoteOn(deviceId: string) {
  asked({ deviceId });
}

/** The remote opened as `done` asked: a later wish stays. */
export function remoteOpened(done: RemoteAsk) {
  if (ask === done) asked(null);
}

function subscribeAsk(listener: () => void): () => void {
  askListeners.add(listener);
  return () => {
    askListeners.delete(listener);
  };
}

/** The wish not yet heard, kept up to date. */
export function useRemoteAsk(): RemoteAsk | null {
  return useSyncExternalStore(subscribeAsk, () => ask);
}

/**
 * Whether a player may end its session for `command`, to start another:
 * the next or previous title, or a track or a quality it plays anew.
 */
export function mayEndSession(command: Control): boolean {
  return command === "next" || command === "previous" || command === "audio" || command === "subtitles" || command === "quality";
}

/**
 * `sessions` with `ended` still in while its device plays nothing else.
 * A command may end a session to start another (`mayEndSession`): between
 * the two, the device would look as if it played nothing, or leave the
 * list if it takes no casts.
 */
export function withEnded(sessions: RemoteSession[], ended: RemoteSession | null): RemoteSession[] {
  if (!ended || sessions.some((s) => s.device_id === ended.device_id)) return sessions;
  return [...sessions, ended];
}

/** `device`'s newest session in `sessions`, leaving out those `mine` says play here. */
export function newestOn(sessions: RemoteSession[], device: string, mine: (sessionId: string) => boolean): RemoteSession | undefined {
  return sessions.filter((s) => s.device_id === device && !mine(s.session_id)).at(-1);
}

/** A track as the players' own menus name it: "English – Commentary (AC3 Stereo)", "French (SRT, Forced)". */
export function trackLabel(track: TrackOption, kind: "audio" | "subtitle"): string {
  return streamLabel({
    index: track.index,
    kind,
    codec: track.codec ?? "",
    language: track.language,
    title: track.title,
    default: track.default,
    forced: track.forced,
    channels: track.channels ?? undefined,
  });
}

/** A command sent, and what its session looked like then. */
export interface SentCommand {
  command: Control;
  before: RemoteSession;
  /** This browser's clock, as `nowMs` given to `commandAnswered` is. */
  sentAt: number;
  /**
   * `seek`, `skip` and a `previous` that starts the title over: where it should be once the player acted;
   * `play` and `pause`: where the controls showed it as it went.
   */
  target?: number;
  /** `audio` and `subtitles`: the track picked (subtitles -1: off). */
  track?: number;
  /** `quality`: the quality picked, by id. */
  quality?: string;
}

/**
 * `session` as a remote shows it while `sent` is out: as the command asks
 * (played or paused, at the place a seek or a skip goes, with the track or
 * quality picked), so the controls don't wait on the player's report to
 * move. Once the wait is over without an answer, the session as it is shows
 * again.
 */
export function asSent(session: RemoteSession, sent: SentCommand | null): RemoteSession {
  if (!sent) return session;
  // A place given at the moment the command went, moving on from there while it plays.
  const at = (position: number) => ({ position_ms: position, reported_at_ms: sent.sentAt });
  switch (sent.command) {
    // Where the controls showed it as it went: a skip just before it still counts.
    case "play":
      return { ...session, state: "playing", ...at(sent.target ?? estimatedPosition(session, sent.sentAt)) };
    case "pause":
      return { ...session, state: "paused", ...at(sent.target ?? estimatedPosition(session, sent.sentAt)) };
    case "audio":
      return { ...session, audio_index: sent.track ?? session.audio_index };
    case "subtitles":
      return { ...session, subtitle_index: sent.track ?? session.subtitle_index };
    case "quality":
      return { ...session, quality: sent.quality ?? session.quality };
    default:
      return sent.target != null ? { ...session, ...at(sent.target) } : session;
  }
}

/** Farther than this from where it should be, a report has moved (the server's rule for a seek). */
const JUMP_MS = 5_000;

/**
 * Whether the list shows the player acted on `sent`, by `nowMs`: it is gone
 * (next and previous end a session to start the next), plays something
 * else, plays the track picked, changed state, or, for a seek, a skip or a
 * previous on the same title, has reported since from somewhere new or
 * where it was sent to.
 *
 * Server times are only compared with server times, and this browser's with
 * its own (`reported_at_ms`): the two clocks can be seconds apart.
 */
export function commandAnswered(sent: SentCommand, now: RemoteSession | undefined, nowMs: number): boolean {
  const { before } = sent;
  if (!now) return true;
  if (now.item?.id !== before.item?.id || now.channel?.id !== before.channel?.id) return true;
  // A track is taken once its player says it plays that one: pausing meanwhile isn't an answer.
  if (sent.command === "audio") return now.audio_index === sent.track;
  if (sent.command === "subtitles") return now.subtitle_index === sent.track;
  if (sent.command === "quality") return now.quality === sent.quality;
  if (now.state !== before.state) return true;
  if (sent.command !== "seek" && sent.command !== "skip" && sent.command !== "previous") return false;
  const reportedSince = now.updated_at > before.updated_at || now.position_ms !== before.position_ms;
  if (!reportedSince) return false;
  const moved = Math.abs(now.position_ms - estimatedPosition(before, now.reported_at_ms)) > JUMP_MS;
  // It may have played on from the target since, for as long as the command has been out.
  const there = sent.target != null && Math.abs(now.position_ms - sent.target) <= JUMP_MS + Math.max(0, nowMs - sent.sentAt);
  return moved || there;
}

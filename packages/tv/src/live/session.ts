import { signal } from "@preact/signals";
import { errorMessage, isApiError, unreachable, unreachableReason } from "../api/client";
import type { LiveBusyConnection, LiveChannel, LiveWatching } from "../api/types";
import { Cancel, NetworkError } from "../net/transport";
import { HLS_MIME, type EngineStatus, type PlayerEngine } from "../player/engine";
import type { EngineError } from "../player/recovery";

/** What a live player needs of the server (the Api; tests give their own). */
export interface LiveServer {
  watchChannel(channelId: number, clientId: string, cancel?: Cancel, takeOver?: number): Promise<LiveWatching>;
  leaveChannel(session: string): Promise<unknown>;
  /** Why a session ended (its connection taken over, say), or null while it goes on. */
  whyLiveEnded(session: string, cancel?: Cancel): Promise<string | null>;
  /** A playlist or segment, as the TV's player asks for it. */
  media(method: "GET" | "HEAD", url: string, cancel?: Cancel, timeoutMs?: number): Promise<string>;
  url(path: string): string;
}

export interface LiveEnv {
  server: LiveServer;
  engine: () => PlayerEngine;
  /** This TV's own id: watching on it ends what it watched before, so changing channels holds one connection. */
  clientId: string;
}

/** Why a channel isn't playing, in words for the person; `canRetry` offers Try again. */
export interface Trouble {
  kind: "busy" | "unavailable" | "stopped" | "missing" | "older" | "away" | "other";
  title: string;
  message: string;
  canRetry: boolean;
  /** All connections in use: what holds each, and whether it may be taken over. */
  connections?: LiveBusyConnection[];
}

/** The connections a `409` from watching says are in use. */
export function busyConnections(e: unknown): LiveBusyConnection[] {
  const body = isApiError(e) ? (e.body as { connections?: unknown } | undefined) : undefined;
  return Array.isArray(body?.connections) ? (body.connections as LiveBusyConnection[]) : [];
}

/** Who holds a connection: "You, on another device", their names, or how many. */
export function holders(c: LiveBusyConnection): string {
  if (c.yours) return "You, on another device";
  if (c.watchers.length) return c.watchers.join(", ");
  return c.people === 1 ? "Someone else" : `${c.people} people`;
}

/** The server's answer as a trouble (the web's, with the TVs' words for a server out of reach). */
export function troubleOf(e: unknown, channel: string): Trouble {
  const message = errorMessage(e);
  if (isApiError(e, 409)) return { kind: "busy", title: "All connections are in use", message, canRetry: true, connections: busyConnections(e) };
  // Stopped on the server, or let go after the TV asked for nothing for a while: watching again may well work.
  if (isApiError(e, 410)) return { kind: "stopped", title: "Watching stopped", message, canRetry: true };
  if (isApiError(e, 404)) return { kind: "missing", title: "This channel isn't here any more", message, canRetry: false };
  if (isApiError(e, 403)) return { kind: "other", title: "Live TV isn't open to this profile", message, canRetry: false };
  if (isApiError(e, 503)) return { kind: "unavailable", title: `${channel} isn't playing`, message, canRetry: true };
  if (unreachable(e)) {
    return {
      kind: "away",
      title: "Can't reach your server",
      message: `${unreachableReason(e)} Check that the server is on and that this device is on its network.`,
      canRetry: true,
    };
  }
  return { kind: "other", title: `${channel} didn't start`, message, canRetry: true };
}

export type LivePhase = "tuning" | "playing" | "buffering" | "error";

export interface LiveState {
  channel: LiveChannel;
  phase: LivePhase;
  trouble: Trouble | null;
  /** The channel's picture (or sound) has started. */
  shown: boolean;
  /** It has sound and no picture (a radio channel). */
  audioOnly: boolean;
  /** Counts each channel tuned by the person (not a recovery), so the banner shows anew. */
  tunes: number;
}

/** The server waits for the channel's first segments before it answers the playlist. */
const PLAYLIST_WAIT_MS = 45_000;
/** No picture this long after the playlist came: the channel didn't start. */
const START_WAIT_MS = 30_000;
/** Trying again after the engine fails, this many times, a second, then three later. */
const RETRIES = 2;
/** The audio is checked for a picture this long after it starts. */
const PICTURE_CHECK_MS = 1_500;
/**
 * A stall this long and the server is asked whether watching ended there:
 * a TV's player says nothing of a playlist that has gone (`410`), it only
 * stops once what it holds runs out.
 */
const STALL_CHECK_MS = 5_000;
/** A stall this long: watched anew, as a failure. */
const STALL_GIVE_UP_MS = 20_000;

/**
 * A live channel playing on the TV's own player (the web's LivePlayerPage
 * without the page): watching it on the server with this TV's id and a
 * keyed playlist address, the engine on that playlist from its live edge,
 * changing channels, the server told at once when watching ends. Live is
 * live: no pause, rewind or skip.
 */
export class LiveController {
  readonly state = signal<LiveState>(null as unknown as LiveState);
  private readonly env: LiveEnv;
  private engineRef: PlayerEngine | null = null;
  private session: string | null = null;
  private generation = 0;
  private cancel: Cancel | null = null;
  private retries = 0;
  private startTimer: ReturnType<typeof setTimeout> | null = null;
  private pictureTimer: ReturnType<typeof setTimeout> | null = null;
  private retryTimer: ReturnType<typeof setTimeout> | null = null;
  private stallTimer: ReturnType<typeof setTimeout> | null = null;
  private closed = false;

  constructor(channel: LiveChannel, env: LiveEnv) {
    this.env = env;
    this.state.value = { channel, phase: "tuning", trouble: null, shown: false, audioOnly: false, tunes: 0 };
  }

  get engine(): PlayerEngine | null {
    return this.engineRef;
  }

  private patch(p: Partial<LiveState>): void {
    this.state.value = { ...this.state.value, ...p };
  }

  private ensureEngine(): PlayerEngine {
    if (this.engineRef) return this.engineRef;
    const engine = this.env.engine();
    engine.setListener({
      status: (s) => this.onStatus(s),
      firstFrame: () => this.onFirstFrame(),
      error: (e) => this.onEngineError(e),
      tracks: () => undefined,
    });
    this.engineRef = engine;
    return engine;
  }

  /** Watches `channel` from its live edge; what was watched before ends. */
  tune(channel: LiveChannel): void {
    this.retries = 0;
    this.patch({ tunes: this.state.value.tunes + 1 });
    void this.start(channel);
  }

  /** The same channel again (Try again). */
  retry(): void {
    this.tune(this.state.value.channel);
  }

  /** Every connection in use: the same channel, on the connection channel `channelId` holds (Watch here instead). */
  takeOver(channelId: number): void {
    this.retries = 0;
    void this.start(this.state.value.channel, channelId);
  }

  private async start(channel: LiveChannel, takeOver?: number): Promise<void> {
    if (this.closed) return;
    const mine = ++this.generation;
    this.clearTimers();
    this.cancel?.cancel();
    const cancel = new Cancel();
    this.cancel = cancel;
    this.engineRef?.stop();
    this.leave();
    this.patch({ channel, phase: "tuning", trouble: null, shown: false, audioOnly: false });
    const server = this.env.server;
    let watching: LiveWatching;
    try {
      watching = await server.watchChannel(channel.id, this.env.clientId, cancel, takeOver);
    } catch (e) {
      if (mine === this.generation && !isCancelled(e)) this.fail(troubleOf(e, channel.name));
      return;
    }
    if (mine !== this.generation) {
      void server.leaveChannel(watching.session_id).catch(() => undefined);
      return;
    }
    this.session = watching.session_id;
    this.patch({ channel: { ...channel, ...watching.channel } });
    // An older server's address needs the token, which the TV's player can't send.
    if (!/\/api\/live\/media\//.test(watching.url)) {
      this.fail({
        kind: "older",
        title: "Live TV needs a newer Pley on the server",
        message: "This server can't send live channels to the TV's own player. Update Pley on the server, then try again.",
        canRetry: false,
      });
      return;
    }
    const playlist = server.url(watching.url);
    try {
      // The channel's first segments, before the player asks: a TV's player gives up sooner than the server takes to tune.
      await server.media("GET", playlist, cancel, PLAYLIST_WAIT_MS);
    } catch (e) {
      if (mine === this.generation && !isCancelled(e)) this.fail(troubleOf(e, channel.name));
      return;
    }
    if (mine !== this.generation) return;
    const engine = this.ensureEngine();
    engine.prepare({ url: playlist, hls: true, live: true, mime: HLS_MIME, startMs: 0, durationHintMs: null, video: null, audioCodec: null }, true);
    this.startTimer = setTimeout(() => {
      if (mine === this.generation && !this.state.value.shown) this.onEngineError({ kind: "other", detail: "it didn't start in time" });
    }, START_WAIT_MS);
  }

  private onStatus(s: EngineStatus): void {
    const st = this.state.value;
    if (st.phase === "error" || st.phase === "tuning") return;
    if (s === "buffering") {
      this.patch({ phase: "buffering" });
      this.watchStall();
    } else if (s === "ready") {
      this.clearStall();
      this.patch({ phase: "playing" });
    } else if (s === "ended") this.onEngineError({ kind: "other", detail: "the stream ended" });
  }

  /** A stall that lasts: the server says whether watching ended (and why); one that goes on is a failure. */
  private watchStall(): void {
    if (this.stallTimer !== null) return;
    const mine = this.generation;
    const stalled = () => mine === this.generation && this.state.value.phase === "buffering";
    this.stallTimer = setTimeout(() => {
      this.stallTimer = setTimeout(() => {
        this.stallTimer = null;
        if (stalled()) this.onEngineError({ kind: "other", detail: "it stalled" });
      }, STALL_GIVE_UP_MS - STALL_CHECK_MS);
      const session = this.session;
      if (!stalled() || !session) return;
      void this.env.server
        .whyLiveEnded(session)
        .catch(() => null)
        .then((why) => {
          if (why === null || !stalled()) return;
          this.session = null;
          this.fail({ kind: "stopped", title: "Watching stopped", message: why, canRetry: true });
        });
    }, STALL_CHECK_MS);
  }

  private clearStall(): void {
    if (this.stallTimer !== null) clearTimeout(this.stallTimer);
    this.stallTimer = null;
  }

  private onFirstFrame(): void {
    if (this.startTimer !== null) clearTimeout(this.startTimer);
    this.startTimer = null;
    this.patch({ shown: true, phase: "playing" });
    const mine = this.generation;
    if (this.pictureTimer !== null) clearTimeout(this.pictureTimer);
    this.pictureTimer = setTimeout(() => {
      this.pictureTimer = null;
      if (mine !== this.generation) return;
      const stats = this.engineRef?.stats() ?? null;
      this.patch({ audioOnly: !(stats && stats.width) });
    }, PICTURE_CHECK_MS);
  }

  /**
   * The TV's player failed: the server is asked first whether watching
   * ended there (someone took the connection over, or it was stopped),
   * which the player can't say, then it is watched anew, twice.
   */
  private onEngineError(e: EngineError): void {
    if (this.closed) return;
    const channel = this.state.value.channel;
    console.warn(`pley: live ${channel.name}: the player failed (${e.kind}${"detail" in e ? `: ${e.detail}` : ""})`);
    const mine = ++this.generation;
    this.clearTimers();
    this.engineRef?.stop();
    this.patch({ phase: "tuning" });
    const session = this.session;
    const ended = session ? this.env.server.whyLiveEnded(session).catch(() => null) : Promise.resolve(null);
    void ended.then((why) => {
      if (mine !== this.generation || this.closed) return;
      const stopped = e.kind === "stopped" || (e.kind === "http" && e.status === 410);
      if (why !== null || stopped) {
        // Its session is gone on the server: nothing to leave.
        if (why !== null) this.session = null;
        this.fail({
          kind: "stopped",
          title: "Watching stopped",
          message: why ?? ((e.kind === "stopped" && e.message) || "The channel was stopped on the server."),
          canRetry: true,
        });
        return;
      }
      this.afterFailure(e, channel);
    });
  }

  private afterFailure(e: EngineError, channel: LiveChannel): void {
    if (this.retries < RETRIES) {
      // Watched anew: the server may have let the channel go, or the network blinked.
      this.retries++;
      const mine = ++this.generation;
      this.clearTimers();
      this.engineRef?.stop();
      this.patch({ phase: "tuning" });
      this.retryTimer = setTimeout(() => {
        this.retryTimer = null;
        if (mine === this.generation) void this.start(channel);
      }, this.retries === 1 ? 1_000 : 3_000);
      return;
    }
    const detail = "detail" in e ? e.detail : "";
    this.fail({ kind: "other", title: `${channel.name} stopped`, message: detail ? `The TV's player said: ${detail}` : "", canRetry: true });
  }

  private fail(trouble: Trouble): void {
    this.generation++;
    this.clearTimers();
    this.engineRef?.stop();
    this.leave();
    this.patch({ phase: "error", trouble });
  }

  /** The TV hid the app: watching ends on the server, to start again when it shows. */
  suspend(): void {
    this.generation++;
    this.cancel?.cancel();
    this.clearTimers();
    this.engineRef?.stop();
    this.leave();
  }

  /** The TV shows the app again: the channel from its live edge. */
  resume(): void {
    if (this.closed) return;
    this.retries = 0;
    void this.start(this.state.value.channel);
  }

  /** The player goes: watching ends on the server at once. */
  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.generation++;
    this.cancel?.cancel();
    this.clearTimers();
    this.leave();
    this.engineRef?.release();
    this.engineRef = null;
  }

  private leave(): void {
    const session = this.session;
    this.session = null;
    if (session) void this.env.server.leaveChannel(session).catch(() => undefined);
  }

  private clearTimers(): void {
    if (this.startTimer !== null) clearTimeout(this.startTimer);
    if (this.pictureTimer !== null) clearTimeout(this.pictureTimer);
    if (this.retryTimer !== null) clearTimeout(this.retryTimer);
    this.startTimer = this.pictureTimer = this.retryTimer = null;
    this.clearStall();
  }
}

function isCancelled(e: unknown): boolean {
  return e instanceof NetworkError && e.cancelled;
}


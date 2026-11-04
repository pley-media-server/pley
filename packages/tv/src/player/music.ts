import { signal } from "@preact/signals";
import { isApiError, SessionStoppedError, unreachable } from "../api/client";
import type { ItemSummary, PlaybackInfo, PlaybackMode } from "../api/types";
import { SignedOutError } from "../auth/tokens";
import { NetworkError } from "../net/transport";
import type { AppState } from "../state";
import { SLEEP_CHOICES, SLEEP_OFF, SleepTimer, type SleepChoice, type SleepState } from "./sleep";
import { emptyCaps } from "./plan";
import { MUSIC_CONTROLS, remoteAction, restartsTrack, type PlayerCommand } from "./remote";

/** What the music player is doing, for the pages that show it. */
export interface NowPlaying {
  track: ItemSummary;
  /** The place in play order, and how many there are. */
  index: number;
  count: number;
  playing: boolean;
  loading: boolean;
  ended: boolean;
  shuffle: boolean;
  error: "offline" | "signed_out" | "stopped" | "unplayable" | null;
}

/** Progress reports while music plays. */
const REPORT_MS = 10_000;
/** While paused: the server forgets players it hasn't heard from for 90 s, and their addresses with them. */
const PAUSED_REPORT_MS = 30_000;
/** New sessions for a track that keeps failing. */
const MAX_RETRIES = 3;

/** Fisher-Yates, `first` kept first. */
export function shuffled<T>(list: T[], first: number, random: () => number = Math.random): T[] {
  const rest = list.filter((_, i) => i !== first);
  for (let i = rest.length - 1; i > 0; i--) {
    const j = Math.floor(random() * (i + 1));
    const t = rest[i]!;
    rest[i] = rest[j]!;
    rest[j] = t;
  }
  if (first < 0 || first >= list.length) return rest;
  rest.unshift(list[first] as T);
  return rest;
}

/**
 * The music player (the Android app's MusicPlayback): one queue of
 * tracks, played through an <audio> element from keyed addresses, one
 * server session per track, its progress reported. It plays on behind
 * every page; a video starting pauses it.
 */
export class MusicPlayer {
  readonly now = signal<NowPlaying | null>(null);
  readonly sleep = signal<SleepState>(SLEEP_OFF);
  private readonly app: AppState;
  private readonly audio: HTMLAudioElement;
  private queue: ItemSummary[] = [];
  private order: ItemSummary[] = [];
  private index = 0;
  private session: PlaybackInfo | null = null;
  private generation = 0;
  private retries = 0;
  private reportTimer: ReturnType<typeof setInterval> | null = null;
  private lastReportAt = 0;
  private readonly timer: SleepTimer;

  constructor(app: AppState) {
    this.app = app;
    const audio = document.createElement("audio");
    audio.preload = "auto";
    audio.style.display = "none";
    document.body.appendChild(audio);
    this.audio = audio;
    audio.addEventListener("playing", () => this.update({ playing: true, loading: false }));
    audio.addEventListener("pause", () => {
      this.update({ playing: false });
      this.report("paused");
    });
    audio.addEventListener("waiting", () => this.update({ loading: true }));
    audio.addEventListener("ended", () => this.trackEnded());
    audio.addEventListener("error", () => this.failed());
    this.timer = new SleepTimer({
      itemLeftMs: () => (this.audio.duration ? Math.max(0, this.audio.duration - this.audio.currentTime) * 1000 : null),
      itemKey: () => this.order[this.index]?.id ?? null,
      onFire: () => this.audio.pause(),
      onChange: (s) => (this.sleep.value = s),
    });
    // A profile switch, signing out, another server or the app closing: the music's session ends with them.
    app.onLeave(() => this.stop());
  }

  /** Plays `tracks` from `at`; shuffled, `at` first and the rest in a random order. */
  play(tracks: ItemSummary[], at: number, shuffle: boolean): void {
    if (!tracks.length) return;
    this.queue = tracks.slice();
    const start = Math.max(0, Math.min(at, tracks.length - 1));
    this.order = shuffle ? shuffled(this.queue, start) : this.queue.slice();
    this.index = shuffle ? 0 : start;
    void this.start(0, null);
  }

  toggle(): void {
    const now = this.now.value;
    if (!now) return;
    if (now.ended) {
      void this.start(0, null);
      return;
    }
    // An error, or a video took over and its session ended: a new one, where it was.
    if (now.error || !this.session) {
      void this.start(this.audio.currentTime * 1000, null);
      return;
    }
    if (this.audio.paused) void this.audio.play().catch(() => undefined);
    else this.audio.pause();
  }

  /** Nothing plays any more: the session ends and the queue goes (Now Playing leaves the rail). */
  stop(): void {
    this.generation++;
    this.audio.pause();
    this.endSession();
    this.timer.cancel();
    this.queue = [];
    this.order = [];
    this.index = 0;
    this.retries = 0;
    this.audio.removeAttribute("src");
    this.now.value = null;
  }

  pause(): void {
    if (!this.audio.paused) this.audio.pause();
  }

  next(): void {
    if (this.index + 1 >= this.order.length) return;
    this.endSession();
    this.index++;
    void this.start(0, null);
  }

  previous(): void {
    if (restartsTrack(this.audio.currentTime * 1000, this.index)) {
      this.seekTo(0);
      return;
    }
    this.endSession();
    this.index--;
    void this.start(0, null);
  }

  jumpTo(position: number): void {
    if (position < 0 || position >= this.order.length) return;
    this.endSession();
    this.index = position;
    void this.start(0, null);
  }

  /** Whether it moved within the session playing; else a new session starts there. */
  seekTo(ms: number): boolean {
    if (this.session?.method === "direct" || this.audio.seekable.length) {
      this.audio.currentTime = Math.max(0, ms) / 1000;
      return true;
    }
    void this.start(ms, null);
    return false;
  }

  setShuffle(on: boolean): void {
    const current = this.order[this.index];
    if (!current) return;
    if (on) {
      this.order = shuffled(this.queue, Math.max(0, this.queue.indexOf(current)));
      this.index = 0;
    } else {
      this.order = this.queue.slice();
      this.index = Math.max(0, this.order.indexOf(current));
    }
    this.update({ shuffle: on, index: this.index });
  }

  /** The sleep timer steps through its choices: Off, 15 … 90 min, end of this track. */
  stepSleep(): void {
    const s = this.sleep.value;
    if (s.secondsLeft !== null) {
      this.timer.extend();
      return;
    }
    const at = SLEEP_CHOICES.findIndex((c) => c.kind === s.choice.kind && (c.kind !== "minutes" || c.minutes === (s.choice as { minutes: number }).minutes));
    const next: SleepChoice = SLEEP_CHOICES[(at + 1) % SLEEP_CHOICES.length]!;
    if (!this.order.length && next.kind !== "off") return;
    this.timer.set(next);
  }

  /**
   * A video takes the screen and the TV's one media pipeline: the music
   * pauses and lets go of its session, which would otherwise be kept
   * through a whole film; the queue and the place stay, and Play starts a
   * new session there.
   */
  yieldToVideo(): void {
    if (!this.now.value) return;
    this.audio.pause();
    this.endSession();
  }

  positionMs(): number {
    const n = this.now.value;
    if (n?.ended) return this.durationMs();
    return (this.audio.currentTime || 0) * 1000;
  }

  durationMs(): number {
    const d = this.audio.duration;
    if (isFinite(d) && d > 0) return d * 1000;
    return this.order[this.index]?.duration_ms ?? 0;
  }

  bufferedMs(): number {
    const b = this.audio.buffered;
    return b.length ? b.end(b.length - 1) * 1000 : 0;
  }

  /** The queue in play order. */
  list(): ItemSummary[] {
    return this.order;
  }

  // ---- inside

  private update(patch: Partial<NowPlaying>): void {
    const cur = this.now.value;
    if (!cur) return;
    this.now.value = { ...cur, ...patch };
  }

  private async start(fromMs: number, mode: PlaybackMode | null): Promise<void> {
    const track = this.order[this.index];
    const api = this.app.api.value;
    if (!track || !api) return;
    const mine = ++this.generation;
    this.now.value = { track, index: this.index, count: this.order.length, playing: false, loading: true, ended: false, shuffle: this.now.value?.shuffle ?? false, error: null };
    // Earlier reports first, so the server ends the last session before this one starts.
    await this.app.reporter.flush(3_000);
    if (mine !== this.generation) return;
    try {
      const probe = await this.app.caps();
      const info = await api.play({
        item_id: track.id,
        start_ms: fromMs > 0 ? Math.round(fromMs) : undefined,
        mode: mode ?? "auto",
        client_id: `${this.app.installId()}-music`,
        keyed_urls: true,
        controls: MUSIC_CONTROLS,
        caps: probe?.caps ?? emptyCaps(),
      });
      if (mine !== this.generation) {
        void api.stopSession(info.session_id).catch(() => undefined);
        return;
      }
      this.session = info;
      this.audio.src = api.url(info.url);
      if (info.method === "direct" && fromMs > 0) {
        const at = fromMs / 1000;
        const seek = () => {
          this.audio.currentTime = at;
          this.audio.removeEventListener("loadedmetadata", seek);
        };
        this.audio.addEventListener("loadedmetadata", seek);
      }
      await this.audio.play().catch(() => undefined);
      this.startReports();
    } catch (e) {
      if (mine !== this.generation) return;
      this.update({ loading: false, playing: false, error: errorKind(e) });
    }
  }

  /**
   * The <audio> element gave up, which it says no more about. The session
   * tells: gone (its address with it, after a long pause) means a new one
   * where it was; stopped means the server stopped it; no answer, the
   * server is out of reach; alive, the track is at fault.
   */
  private failed(): void {
    const session = this.session;
    const api = this.app.api.value;
    if (!session || !this.now.value || !api) return;
    const mine = this.generation;
    api.sessionStatus(session.session_id).then(
      () => mine === this.generation && this.trackFailed(session),
      (e: unknown) => {
        if (mine !== this.generation) return;
        if (e instanceof SessionStoppedError) this.stoppedByServer(session.session_id);
        else if (isApiError(e, 404)) {
          this.session = null;
          void this.start(this.audio.currentTime * 1000, session.method === "direct" ? null : "transcode");
        } else if (unreachable(e)) this.update({ playing: false, loading: false, error: "offline" });
        else this.trackFailed(session);
      },
    );
  }

  private trackFailed(session: PlaybackInfo): void {
    const mine = this.generation;
    // A file the TV can't decode: once more as a conversion; other trouble: a few new sessions.
    if (session.method === "direct" && this.retries === 0) {
      this.retries++;
      this.endSession();
      void this.start(this.audio.currentTime * 1000, "transcode");
      return;
    }
    if (this.retries < MAX_RETRIES) {
      this.retries++;
      const at = this.audio.currentTime * 1000;
      this.endSession();
      setTimeout(() => mine === this.generation && void this.start(at, session.method === "direct" ? null : "transcode"), 1000);
      return;
    }
    this.update({ playing: false, loading: false, error: "unplayable" });
  }

  private trackEnded(): void {
    const track = this.order[this.index];
    this.report("stopped");
    this.endSession();
    this.retries = 0;
    if (track && this.timer.itemEnded(track.id)) {
      // The timer ends with this track: the next one waits, paused.
      if (this.index + 1 < this.order.length) {
        this.index++;
        this.now.value = { ...this.now.value!, track: this.order[this.index]!, index: this.index, playing: false, loading: false };
      }
      return;
    }
    if (this.index + 1 < this.order.length) {
      this.index++;
      void this.start(0, null);
    } else {
      this.update({ playing: false, ended: true });
    }
  }

  /** Every 10 s while it plays, every 30 s paused: the server keeps the session, and its address, as long as it hears from it. */
  private startReports(): void {
    if (this.reportTimer !== null) clearInterval(this.reportTimer);
    this.reportTimer = setInterval(() => {
      if (!this.audio.paused) this.report("playing");
      else if (Date.now() - this.lastReportAt >= PAUSED_REPORT_MS) this.report("paused");
    }, REPORT_MS);
  }

  private report(state: "playing" | "paused" | "stopped"): void {
    const api = this.app.api.value;
    const s = this.session;
    const track = this.order[this.index];
    if (!api || !s || !track) return;
    const pos = this.audio.currentTime * 1000;
    const dur = this.durationMs();
    this.lastReportAt = Date.now();
    this.app.reporter.send(() => api.progress(track.id, pos, dur || null, state, s.session_id));
  }

  /** The track's session ends: its last position, then the session itself. */
  private endSession(): void {
    const api = this.app.api.value;
    const s = this.session;
    if (!s || !api) return;
    this.report("stopped");
    this.app.reporter.send(() => api.stopSession(s.session_id));
    this.session = null;
    if (this.reportTimer !== null) clearInterval(this.reportTimer);
    this.reportTimer = null;
  }

  /**
   * A command of the user's remote, on another device, for the track
   * playing (remote.ts has the rule), as the buttons here do it: what
   * changed is reported at once, so the remote sees it. A pause reports as
   * any pause does, another track with its own session.
   */
  obey(command: PlayerCommand): void {
    const s = this.session;
    const now = this.now.value;
    if (!s || !now || now.error || now.ended) return;
    const action = remoteAction(
      {
        kind: "music",
        session: s.session_id,
        playing: !this.audio.paused,
        positionMs: this.audio.currentTime * 1000,
        durationMs: this.durationMs() || null,
        hasNext: this.index + 1 < this.order.length,
        index: this.index,
      },
      command,
    );
    switch (action.act) {
      case "play":
        void this.audio.play().then(
          () => this.session === s && this.report("playing"),
          () => undefined,
        );
        break;
      case "pause":
        this.audio.pause();
        break;
      case "seek":
        if (this.seekTo(action.positionMs)) this.report(this.audio.paused ? "paused" : "playing");
        break;
      case "next":
        this.next();
        break;
      case "previous":
        this.previous();
        break;
    }
  }

  /** The server stopped this session (session_stopped). */
  stoppedByServer(session: string): void {
    if (this.session?.session_id !== session) return;
    this.audio.pause();
    this.session = null;
    this.update({ playing: false, error: "stopped" });
  }
}

function errorKind(e: unknown): NowPlaying["error"] {
  if (e instanceof SessionStoppedError) return "stopped";
  if (e instanceof SignedOutError) return "signed_out";
  if (e instanceof NetworkError) return "offline";
  if (isApiError(e, 401)) return "signed_out";
  return "unplayable";
}

let player: MusicPlayer | null = null;

/** The app's music player, made with the app's state once. */
export function initMusic(app: AppState): MusicPlayer {
  if (!player) {
    player = new MusicPlayer(app);
    app.onEvent((e) => {
      if (e.type === "session_stopped") player?.stoppedByServer(e.session_id);
      else if (e.type === "player_command") player?.obey(e);
    });
  }
  return player;
}

export function musicPlayer(): MusicPlayer {
  if (!player) throw new Error("music player not started");
  return player;
}

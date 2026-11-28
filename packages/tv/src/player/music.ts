import { signal } from "@preact/signals";
import { isApiError, SessionStoppedError, unreachable } from "../api/client";
import type { ItemSummary, PlaybackInfo, PlaybackMode } from "../api/types";
import { SignedOutError } from "../auth/tokens";
import { NetworkError } from "../net/transport";
import type { AppState } from "../state";
import { emptyCaps } from "./plan";
import { MUSIC_CONTROLS, remoteAction, restartsTrack, type PlayerCommand } from "./remote";
import { audioKind, storedAudioSeekMemory, type AudioSeekMemory, type AudioSeekStep } from "./seekmemory";

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
/** HTMLMediaElement.HAVE_METADATA: the stream knows its length, and can be moved. */
const HAVE_METADATA = 1;
/** How far from where it was to start a track may first play before it is moved there again. */
const LANDED_MS = 2_000;
/** How far past where a track last failed it plays before its failures count afresh. */
const RECOVERED_MS = 10_000;
/** An error this soon after the file was moved may be the move's: the file is tried again there first. */
const AFTER_SEEK_MS = 1_500;
/** A second failure this near the last one is the file's: it converts. */
const SAME_PLACE_MS = 5_000;
/** How near the place a seek's step must bring the file (plus however long it played since) to have landed. */
const SEEK_NEAR_MS = 3_000;
/** How long a move (step 1) has to land, unless the element says it `seeked` there sooner. */
const MOVE_CHECK_MS = 1_500;
/** How long a reload (step 2) has to land once it can play. */
const RELOAD_CHECK_MS = 2_500;
/** No step waits longer than this for its answer. */
const STEP_MAX_MS = 3_000;
/** A place this near the track's start is its start: the file plays from there as it is, with no step to check. */
const RESTART_MS = 1_000;
/** Where this TV's audio seek steps are kept (`AudioSeekMemory`). */
export const AUDIO_SEEKS_KEY = "pley.audioSeeks";

/** A seek's ladder under way in the file: its place, its step, and how to stop waiting on it. */
interface Ladder {
  target: number;
  step: AudioSeekStep;
  end(): void;
}

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
  private readonly app: AppState;
  private readonly audio: HTMLAudioElement;
  private queue: ItemSummary[] = [];
  private order: ItemSummary[] = [];
  private index = 0;
  private session: PlaybackInfo | null = null;
  private generation = 0;
  private retries = 0;
  /**
   * Where the track is to be, not yet applied or not yet playing there: a
   * start's place (a cast, a new session after trouble), or a seek before
   * the stream knew its length. Null once it plays there.
   */
  private pendingMs: number | null = null;
  /** Whether the element has the stream of the session starting now: events before are the last stream's. */
  private streamSet = false;
  /** Whether this session is a conversion started because the file failed: it ends with the session, not the track. */
  private converted = false;
  /** When the element was last moved within its stream (Date.now()). */
  private lastSeekAt = -Infinity;
  /** Where the track last failed: once it played well past there, its failures count afresh. */
  private failedAtMs: number | null = null;
  /** Whether the track's file failed on the TV: played as a file again (a seek asks afresh), a failure converts at once. */
  private directFailed = false;
  /** The kind of file ("flac") the session plays as it is, its seeks remembered by; null for any other stream. */
  private kind: string | null = null;
  /** A seek in the file making its way through the steps; a newer seek replaces it. */
  private ladder: Ladder | null = null;
  /**
   * The first step not yet failed in the track's file: a later seek starts
   * no lower. At 3 the track's places are reached by conversion for the
   * rest of the track (a start at a place too); the next track starts at 1.
   */
  private floor: AudioSeekStep = 1;
  /** Kinds of file a plain move was tried in since the app started (`seekStep`). */
  private readonly movedOnce = new Set<string>();
  private readonly seeks: AudioSeekMemory;
  private reportTimer: ReturnType<typeof setInterval> | null = null;
  private lastReportAt = 0;

  constructor(app: AppState, seeks: AudioSeekMemory = storedAudioSeekMemory(app.store, AUDIO_SEEKS_KEY)) {
    this.app = app;
    this.seeks = seeks;
    const audio = document.createElement("audio");
    audio.preload = "auto";
    audio.style.display = "none";
    document.body.appendChild(audio);
    this.audio = audio;
    audio.addEventListener("loadedmetadata", () => {
      if (!this.streamSet || this.pendingMs === null) return;
      // A seek held in a file until it knew its length goes through the steps, as any seek there.
      if (this.session?.method === "direct" && !this.ladder && this.pendingMs >= RESTART_MS) {
        const at = this.pendingMs;
        this.pendingMs = null;
        this.climb(at, this.seekStep(), !audio.paused);
        return;
      }
      this.moveTo(this.pendingMs);
    });
    audio.addEventListener("playing", () => {
      if (this.streamSet) {
        // Some conversions go back to their start as they begin: the place a start or a seek asked for, again.
        // A file goes where it was told and stays there.
        const want = this.pendingMs;
        this.pendingMs = null;
        if (want !== null && this.session?.method !== "direct" && Math.abs(audio.currentTime * 1000 - want) > LANDED_MS) this.moveTo(want);
      }
      this.update({ playing: true, loading: false });
    });
    // Playing again isn't enough (a track can fail at the same place each time): played well past it is.
    audio.addEventListener("timeupdate", () => {
      if (this.streamSet && this.failedAtMs !== null && audio.currentTime * 1000 > this.failedAtMs + RECOVERED_MS) {
        this.failedAtMs = null;
        this.retries = 0;
      }
    });
    audio.addEventListener("pause", () => {
      this.update({ playing: false });
      this.report("paused");
    });
    audio.addEventListener("waiting", () => this.update({ loading: true }));
    audio.addEventListener("ended", () => this.trackEnded());
    audio.addEventListener("error", () => this.failed());
    // A profile switch, signing out, another server or the app closing: the music's session ends with them.
    app.onLeave(() => this.stop());
  }

  /** Plays `tracks` from `at`, `fromMs` into it (a cast's place); shuffled, `at` first and the rest in a random order. */
  play(tracks: ItemSummary[], at: number, shuffle: boolean, fromMs = 0): void {
    if (!tracks.length) return;
    this.queue = tracks.slice();
    const start = Math.max(0, Math.min(at, tracks.length - 1));
    this.order = shuffle ? shuffled(this.queue, start) : this.queue.slice();
    this.index = shuffle ? 0 : start;
    this.newTrack();
    void this.start(Math.max(0, fromMs), null);
  }

  toggle(): void {
    const now = this.now.value;
    if (!now) return;
    if (now.ended) {
      void this.start(0, null);
      return;
    }
    // An error, or a video took over and its session ended: a new one, where it was, its failures counted afresh.
    if (now.error || !this.session) {
      this.retries = 0;
      const at = this.here();
      void this.start(at, this.placeMode(at));
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
    this.queue = [];
    this.order = [];
    this.index = 0;
    this.newTrack();
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
    this.newTrack();
    void this.start(0, null);
  }

  previous(): void {
    if (restartsTrack(this.here(), this.index)) {
      this.seekTo(0);
      return;
    }
    this.endSession();
    this.index--;
    this.newTrack();
    void this.start(0, null);
  }

  jumpTo(position: number): void {
    if (position < 0 || position >= this.order.length) return;
    this.endSession();
    this.index = position;
    this.newTrack();
    void this.start(0, null);
  }

  /**
   * Whether it moved within the session playing (or, paused with none,
   * holds the place for the next play); else a new session starts there.
   * A file moves in place, through `climb`'s steps: the TV asks the server
   * for the bytes from there, or loads the file again from there, or the
   * server converts it from there. Any other stream (a remux or a conversion) starts again at the place,
   * as the server makes it; a conversion made only because the file failed
   * asks the server afresh, so the file is tried again.
   */
  seekTo(ms: number): boolean {
    const target = Math.max(0, ms);
    const s = this.session;
    const a = this.audio;
    const now = this.now.value;
    console.info("[music] seek", { target, method: s?.method ?? null, hls: s?.hls ?? null, readyState: a.readyState });
    if (!s && now && !now.playing && !now.ended && !now.error) {
      // Paused with no session (a video took it), or one starting: held, and played from when asked.
      this.pendingMs = target;
      return true;
    }
    if (s && s.method === "direct") {
      // The track's start needs no retried move (it reloads the file once a later step is known).
      const step = target < RESTART_MS ? this.firstStep() : this.seekStep();
      if (a.readyState < HAVE_METADATA && step === 1 && !this.ladder) {
        // Not movable yet: held, and applied once it knows its length.
        this.pendingMs = target;
        return true;
      }
      this.pendingMs = null;
      if (target < RESTART_MS && step > 1) {
        // The track's start: the file loaded again, no place to reach.
        this.dropLadder();
        this.reload(0, !a.paused);
        return true;
      }
      this.climb(target, step, !a.paused);
      return this.session !== null;
    }
    // A conversion because the file failed asks afresh; one because the file can't be moved in keeps converting,
    // except to start the track again, which the file does as it is.
    const afresh = this.converted && this.floor < 3;
    let mode: PlaybackMode | null = s && !afresh ? (s.method === "remux" ? "remux" : "transcode") : null;
    if (this.floor === 3 && target < RESTART_MS) mode = null;
    // The track's session ends first, as for the next track: a pause while the new one loads has nothing to pause.
    this.endSession();
    // A session the user asked for: its failures aren't the last session's.
    this.retries = 0;
    void this.start(target, mode);
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
    return this.here();
  }

  /** The stream's own length, else the session's (the file's, as the server read it), else the track's. */
  durationMs(): number {
    const d = this.audio.duration;
    if (isFinite(d) && d > 0) return d * 1000;
    return this.session?.duration_ms || this.order[this.index]?.duration_ms || 0;
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

  /** Where the track is, or is about to be: a start's place or a seek not yet applied counts. */
  private here(): number {
    return this.pendingMs ?? this.ladder?.target ?? (this.audio.currentTime || 0) * 1000;
  }

  /** Where a seek in the file starts: the step this TV remembers for its kind, never one this session's file failed. */
  private firstStep(): AudioSeekStep {
    const known = this.kind ? this.seeks.step(this.kind) : 1;
    return Math.max(known, this.floor) as AudioSeekStep;
  }

  /**
   * The step a seek in the file starts at: `firstStep`, except that the
   * first seek in a kind of file since the app started tries a plain move,
   * so a TV that seeks after all (a slow seek taught the steps, a firmware
   * update) forgets them.
   */
  private seekStep(): AudioSeekStep {
    const step = this.firstStep();
    const kind = this.kind;
    // Marked once a move was made and judged (`climb`): one asked and replaced first is tried again.
    if (!kind || step <= this.floor || this.movedOnce.has(kind)) return step;
    return this.floor;
  }

  /**
   * The mode a new session at `fromMs` asks for: a conversion when the
   * track's file is known not to reach places on this TV, else the server's pick.
   */
  private placeMode(fromMs: number): PlaybackMode | null {
    return this.floor === 3 && fromMs >= RESTART_MS ? "transcode" : null;
  }

  /**
   * A new session at `fromMs` for one that failed by no fault of the
   * file's (a network blip, its address gone): a conversion because the
   * file failed stays one, as the file would fail again.
   */
  private retry(fromMs: number): void {
    const converted = this.converted;
    void this.start(fromMs, converted ? "transcode" : this.placeMode(fromMs), converted);
  }

  /**
   * A seek in the file, at step `step`: 1 moves the element and waits for
   * it to land (it says `seeked`, or 1.5 s at most); 2 loads the file again
   * from the place (`#t=`, and the place set as it knows its length) and
   * waits for it to land once it can play (2.5 s at most); 3 ends the
   * session and converts from the place, and the track's later places too
   * (the next track asks the server afresh). A step that fails goes on to
   * the next; a later step that worked is remembered for the kind, and a
   * plain move that works forgets it. No step waits more than 3 s. A reload
   * plays only if the track played as the seek was made (`plays`); one
   * made paused is judged once it plays, not by the place it was told.
   * `loaded`: the file was just started at the place (`#t=`), so step 2
   * checks that start rather than loading it again; its landing teaches
   * nothing (a start isn't a seek), its failure does. `taught`: the steps
   * before failed plainly (the element played on elsewhere), not by being
   * slow to load; only then is a later step remembered.
   */
  private climb(target: number, step: AudioSeekStep, plays: boolean, loaded = false, taught = true): void {
    this.dropLadder();
    const s = this.session;
    const api = this.app.api.value;
    if (!s || !api) return;
    const kind = this.kind;
    if (target < RESTART_MS && step > 1) {
      // The track's start needs no step: the file as it is.
      if (!loaded) this.reload(0, plays);
      return;
    }
    if (step === 3) {
      console.info("[music] seek ladder", { kind, step, landed: null });
      // Learned now, by the steps before failing plainly; a step remembered already isn't, so the memory still runs out.
      if (taught && kind && this.seeks.step(kind) < 3) this.seeks.landed(kind, 3);
      this.floor = 3;
      this.endSession();
      this.retries = 0;
      void this.start(target, "transcode");
      return;
    }
    const a = this.audio;
    let begun = Date.now();
    const timers: ReturnType<typeof setTimeout>[] = [];
    const events: [string, () => void][] = [];
    const ladder: Ladder = {
      target,
      step,
      end: () => {
        for (const t of timers) clearTimeout(t);
        for (const [type, f] of events) a.removeEventListener(type, f);
      },
    };
    this.ladder = ladder;
    const near = (at: number): boolean => at >= target - SEEK_NEAR_MS && at <= target + SEEK_NEAR_MS + (Date.now() - begun);
    // Near the place, or played on past it since; never while it still seeks (it then reads the place asked for).
    const landed = (): boolean => !a.seeking && near((a.currentTime || 0) * 1000);
    // Step 2: the reload can play.
    let armed = false;
    const finish = (ok: boolean): void => {
      if (this.ladder !== ladder) return;
      // Still loading when its time ran out (a slow network): the next step is tried, but nothing is learned.
      const slow = !ok && (a.seeking || (step === 2 && !armed));
      this.dropLadder();
      if (kind && step === 1) this.movedOnce.add(kind);
      console.info("[music] seek ladder", slow ? { kind, step, landed: ok, slow } : { kind, step, landed: ok });
      if (ok) {
        // A plain move forgets the kind; a later step is kept when it's later than what's remembered.
        if (kind && !loaded && (step === 1 || (taught && step > this.seeks.step(kind)))) this.seeks.landed(kind, step);
        return;
      }
      // A move tried again where a later step is remembered goes on at that step, not the ones known to fail.
      const next = Math.max(step + 1, step === 1 && kind ? this.seeks.step(kind) : 1) as AudioSeekStep;
      this.floor = Math.max(this.floor, next) as AudioSeekStep;
      this.climb(target, next, plays, false, taught && !slow);
    };
    const on = (type: string, f: () => void): void => {
      a.addEventListener(type, f);
      events.push([type, f]);
    };
    // A reload while paused plays nothing yet: its time is the place it was told until it plays.
    const onPlay = step === 2 && !plays;
    if (!onPlay) timers.push(setTimeout(() => finish(landed()), step === 1 ? MOVE_CHECK_MS : STEP_MAX_MS));
    if (step === 1) {
      on("seeked", () => landed() && finish(true));
      this.moveTo(target);
      return;
    }
    // The reload is judged once it can play, and not by a first reading, which may be the place it was told:
    // by the time allowed, or sooner as it plays on from near the place.
    let last: number | null = null;
    const ready = (): void => {
      if (armed || this.ladder !== ladder || (onPlay && a.paused)) return;
      armed = true;
      if (onPlay) begun = Date.now();
      const wait = onPlay ? RELOAD_CHECK_MS : Math.min(RELOAD_CHECK_MS, Math.max(0, STEP_MAX_MS - (Date.now() - begun)));
      timers.push(setTimeout(() => finish(landed()), wait));
    };
    on("loadedmetadata", () => this.moveTo(target));
    on("canplay", ready);
    on("playing", ready);
    on("timeupdate", () => {
      if (!armed || a.paused || a.seeking) return;
      const at = (a.currentTime || 0) * 1000;
      if (!near(at)) {
        last = null;
        return;
      }
      if (last !== null && at > last) finish(true);
      else last = at;
    });
    if (!loaded) this.reload(target, plays);
  }

  /** The file loaded again from `ms` (`#t=`; none from its start): the TV asks for the bytes from there. */
  private reload(ms: number, plays: boolean): void {
    const s = this.session;
    const api = this.app.api.value;
    if (!s || !api) return;
    const a = this.audio;
    this.lastSeekAt = Date.now();
    a.src = api.url(s.url) + (ms >= RESTART_MS ? fragment(ms) : "");
    try {
      a.load();
    } catch {
      // Setting the address loads it anyway.
    }
    if (plays) void a.play().catch(() => undefined);
  }

  /** The seek's ladder under way, if any, waits no more. */
  private dropLadder(): void {
    this.ladder?.end();
    this.ladder = null;
  }

  /** The element goes to `ms` within its stream; an error soon after may be the move's. */
  private moveTo(ms: number): void {
    this.lastSeekAt = Date.now();
    this.audio.currentTime = ms / 1000;
  }

  /** Another track (or none): its failures counted afresh, played as the server picks. */
  private newTrack(): void {
    this.retries = 0;
    this.pendingMs = null;
    this.failedAtMs = null;
    this.directFailed = false;
    this.lastSeekAt = -Infinity;
    this.floor = 1;
  }

  private update(patch: Partial<NowPlaying>): void {
    const cur = this.now.value;
    if (!cur) return;
    this.now.value = { ...cur, ...patch };
  }

  /** `converted`: a conversion only because the file failed, for this session alone. */
  private async start(fromMs: number, mode: PlaybackMode | null, converted = false): Promise<void> {
    const track = this.order[this.index];
    const api = this.app.api.value;
    if (!track || !api) return;
    const mine = ++this.generation;
    this.dropLadder();
    this.pendingMs = fromMs > 0 ? fromMs : null;
    this.streamSet = false;
    this.kind = null;
    this.converted = converted;
    this.now.value = { track, index: this.index, count: this.order.length, playing: false, loading: true, ended: false, shuffle: this.now.value?.shuffle ?? false, error: null };
    // Earlier reports go out meanwhile, not first: the server ends this player's last session as the new one starts
    // (one per client_id), and a start waits on nothing slow.
    void this.app.reporter.flush(3_000);
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
      this.kind = info.method === "direct" ? audioKind(info.url) : null;
      // A file goes where a seek held meanwhile asked (a conversion was made from `fromMs`, and moves there as it loads).
      if (info.method === "direct") fromMs = this.pendingMs ?? 0;
      // Every stream begins at the track's start (a conversion's playlist too, made from start_ms on):
      // the place is set as it knows its length, and again as it first plays if it went back.
      console.info("[music] start", { item: track.id, method: info.method, hls: info.hls, fromMs: Math.round(fromMs), mode: mode ?? "auto" });
      // A file started at a place says so in its address too, so the TV asks for the bytes from there at once.
      const place = info.method === "direct" && fromMs >= RESTART_MS;
      if (place && Math.max(this.firstStep(), 2) === 3) {
        // This kind of file reaches no place on this TV: converted from there at once.
        // Known by the memory or by this track's failures: nothing new to learn.
        this.climb(fromMs, 3, true, false, false);
        return;
      }
      const at = info.method === "direct" && fromMs > 0 ? fragment(fromMs) : "";
      this.audio.src = api.url(info.url) + at;
      this.streamSet = true;
      // Started at a place: checked as a reload is, and converted from there if it didn't go there.
      if (place) this.climb(fromMs, 2, true, true);
      await this.audio.play().catch(() => undefined);
      if (mine === this.generation) this.startReports();
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
    // The failure's path decides now: a seek's steps wait no more. Its place is kept unless the element is there.
    const ladder = this.ladder;
    if (ladder && Math.abs((this.audio.currentTime || 0) * 1000 - ladder.target) > SEEK_NEAR_MS) this.pendingMs = ladder.target;
    this.dropLadder();
    const mine = this.generation;
    api.sessionStatus(session.session_id).then(
      () => mine === this.generation && this.trackFailed(session),
      (e: unknown) => {
        if (mine !== this.generation) return;
        if (e instanceof SessionStoppedError) this.stoppedByServer(session.session_id);
        else if (isApiError(e, 404)) {
          this.session = null;
          this.retry(this.here());
        } else if (unreachable(e)) this.update({ playing: false, loading: false, error: "offline" });
        else this.trackFailed(session);
      },
    );
  }

  /**
   * The track failed with its session alive. A file failing just after a
   * move (the TV's request from there) is tried again there first; a file
   * failing otherwise, again at the same place, or once its file failed
   * before, plays one session as a conversion; anything else gets a few
   * new sessions, as the server picks.
   */
  private trackFailed(session: PlaybackInfo): void {
    const mine = this.generation;
    if (this.retries >= MAX_RETRIES) {
      this.update({ playing: false, loading: false, error: "unplayable" });
      return;
    }
    this.retries++;
    const at = this.here();
    const again = this.failedAtMs !== null && Math.abs(at - this.failedAtMs) <= SAME_PLACE_MS;
    const afterSeek = Date.now() - this.lastSeekAt <= AFTER_SEEK_MS;
    this.failedAtMs = at;
    if (session.method === "direct") {
      const failing = again || !afterSeek || this.directFailed;
      const mode: PlaybackMode | null = failing ? "transcode" : this.placeMode(at);
      this.directFailed = true;
      console.info("[music] direct play failed", { file: session.file_id, atMs: Math.round(at), afterSeek, again, mode: mode ?? "auto" });
      this.endSession();
      void this.start(at, mode, failing);
      return;
    }
    this.endSession();
    // Held meanwhile: a seek while it waits moves where it starts again.
    this.pendingMs = at;
    setTimeout(() => {
      if (mine !== this.generation) return;
      this.retry(this.here());
    }, 1000);
  }

  private trackEnded(): void {
    this.report("stopped");
    this.endSession();
    this.newTrack();
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
    const pos = this.here();
    const dur = this.durationMs();
    this.lastReportAt = Date.now();
    this.app.reporter.send(() => api.progress(track.id, pos, dur || null, state, s.session_id));
  }

  /** The track's session ends: its last position, then the session itself. */
  private endSession(): void {
    const api = this.app.api.value;
    const s = this.session;
    this.dropLadder();
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
   * any pause does, another track with its own session; a play or pause
   * that finds it so already says it again.
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
        positionMs: this.here(),
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
      case "none":
        // Playing or paused already: said again, so a remote a little behind settles.
        if (action.report) this.report(this.audio.paused ? "paused" : "playing");
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

/** A file's address from `ms` on: the TV asks for the bytes from there as it loads. */
function fragment(ms: number): string {
  return `#t=${Math.round(ms / 100) / 10}`;
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

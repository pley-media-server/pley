import type { EngineListener, EngineSource, EngineStats, EngineStatus, EngineTrack, PlayerEngine } from "pley-tv/player/engine";
import type { EngineError } from "pley-tv/player/recovery";

/** AVPlay's error names as the session's recovery reads them. */
export function avplayError(error: string, source: EngineSource | null, httpStatus: number | null): EngineError {
  const e = String(error);
  if (httpStatus === 410) return { kind: "stopped", message: null };
  if (httpStatus && httpStatus >= 400) return { kind: "http", status: httpStatus, detail: e };
  if (/NOT_SUPPORTED_FILE/.test(e)) return { kind: "unsupported", audio: false, container: true, detail: e };
  if (/NOT_SUPPORTED_FORMAT|NOT_SUPPORTED/.test(e)) return { kind: "unsupported", audio: /audio/i.test(e), container: false, detail: e };
  if (/CONNECTION_FAILED|NETWORK|TIMEOUT/.test(e)) return { kind: "network", detail: e };
  if (/INVALID_URI|NO_SUCH_FILE/.test(e)) return { kind: "http", status: 404, detail: e };
  if (source?.hls && /INVALID|PLAYLIST|MANIFEST/.test(e)) return { kind: "playlist", detail: e };
  return { kind: "other", detail: e };
}

/** The HTTP status in AVPlay's error message (Tizen 6.0 and later), when it gives one. */
export function statusOf(message: string): number | null {
  const m = /HTTP\/[\d.]+\s+(\d{3})/.exec(message) ?? /"(?:http_)?status(?:_code)?"\s*:\s*"?(\d{3})/.exec(message);
  return m ? Number(m[1]) : null;
}

/** A seek AVPlay answers neither way in this long is taken as made. */
export const SEEK_WATCHDOG_MS = 10_000;
/**
 * In a stream, the server may take this long to make the segment a seek
 * goes to (a transcode's restart on a small server): a seek unanswered
 * then is taken as made, and isn't judged by AVPlay's time from before it.
 */
export const HLS_SEEK_WATCHDOG_MS = 30_000;
/** A seek made, once played this long, should have the player this near where it went. */
export const LAND_CHECK_MS = 3_000;
export const LAND_NEAR_MS = 5_000;
/** However long the stream waits, a seek is checked this long after AVPlay said it was made. */
export const LAND_CAP_MS = 15_000;
/**
 * A seek shorter than this failed only when AVPlay plays on as if it had
 * never been asked (within `STAYED_MS` of where it came from): it lands on
 * a keyframe, which in a long-GOP file can be as far behind the target as
 * a short skip is long, nearer where it came from than where it went.
 */
export const LAND_JUDGED_MS = 30_000;
export const STAYED_MS = 1_000;
/** AVPlay's time this near a seek's target is taken as its own, not a stale one. */
const HOLD_NEAR_MS = 2_000;

/** The name of what AVPlay failed with, for the console. */
function nameOf(e: unknown): string {
  return String((e as { name?: string } | null)?.name ?? e);
}

function info(track: AvplayTrack): Record<string, string> {
  try {
    return JSON.parse(track.extra_info) as Record<string, string>;
  } catch {
    return {};
  }
}

/**
 * Samsung's AVPlay: hardware decoders on a video plane under the page,
 * HDR10 and HDR10+ from the stream itself, Dolby audio sent to a receiver
 * as the TV is set. AVPlay takes one call at a time while it seeks, can't
 * seek to 0, and must be stopped and closed before the next source.
 */
export class AvplayEngine implements PlayerEngine {
  readonly name = "AVPlay";
  /**
   * The player of sets before 2020 (Tizen 4 and 5) prepares a stream from
   * its first segment and answers no seek before it plays: a resume point
   * is sought once it plays (`startAfterPlay`). Later sets seek before
   * playing.
   */
  readonly hlsFromFirstSegment: boolean;
  private readonly player: Avplay;
  private readonly object: HTMLObjectElement;
  private listener: EngineListener | null = null;
  private source: EngineSource | null = null;
  private state: EngineStatus = "idle";
  private wantPlay = false;
  private shown = false;
  private seeking = false;
  private queued: (() => void)[] = [];
  private lastTime = 0;
  private generation = 0;
  private httpStatus: number | null = null;
  private readonly uhd: boolean;
  /** Bumped by each seek and each close: a callback of an older seek is ignored. */
  private seekToken = 0;
  private watchdog: ReturnType<typeof setTimeout> | null = null;
  /**
   * Where a seek went (from `origin`), until AVPlay's own time comes near
   * it (or `until` passes): its stale times meanwhile, nearer where it came
   * from, aren't the position.
   */
  private hold: { target: number; origin: number; until: number } | null = null;
  /**
   * A seek AVPlay said it made, to be checked once it plays (`LAND_CHECK_MS`)
   * or at `LAND_CAP_MS`: `played` while it played since (`since`: playing
   * now, from then). Paused, it doesn't move, and isn't judged until it plays.
   */
  private landing: { target: number; origin: number; played: number; since: number | null } | null = null;
  /** Bumped by each seek asked for, as it is asked: a seek's outcome once a newer one was asked is moot. */
  private requested = 0;
  /** Whether the source playing is prepared: before, a seek moves where it starts. */
  private opened = false;
  /** Whether it has started (asked to play when meant to): before, AVPlay's buffering isn't "ready". */
  private started = false;
  /** Where the source playing starts (its resume point, or a seek asked for before it was prepared). */
  private startMs = 0;
  /**
   * A stream's resume point, sought once it plays: Tizen's player prepares
   * a stream from its first segment, and doesn't answer a seek before it
   * plays (the native app gives it playlists that begin there instead).
   */
  private startAfterPlay: number | null = null;
  private landTimer: ReturnType<typeof setTimeout> | null = null;
  private capTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(host: HTMLElement, uhd: boolean, hlsFromFirstSegment = false) {
    this.player = webapis!.avplay;
    this.uhd = uhd;
    this.hlsFromFirstSegment = hlsFromFirstSegment;
    const object = document.createElement("object");
    object.setAttribute("type", "application/avplayer");
    object.className = "engine-avplay";
    host.appendChild(object);
    this.object = object;
    document.documentElement.classList.add("avplay");
  }

  setListener(listener: EngineListener): void {
    this.listener = listener;
  }

  private set(status: EngineStatus): void {
    if (this.state === status) return;
    this.state = status;
    const l = this.landing;
    if (l) {
      // A seek's landing is judged once it plays, not while the stream waits, and not at all once it ended.
      if (status === "ended" || status === "idle") this.clearLanding();
      else if (status === "ready") this.landingRuns();
      else this.landingWaits();
    }
    this.listener?.status(status);
  }

  /** Calls after a seek in flight wait for it (AVPlay refuses them meanwhile). */
  private run(fn: () => void): void {
    if (this.seeking) this.queued.push(fn);
    else {
      try {
        fn();
      } catch {
        // The state allows nothing now; the listener hears what went wrong.
        // A seek that threw waits for nothing: the calls after it go on.
        if (this.seeking) this.seekDone();
      }
    }
  }

  private clearTimer(name: "watchdog" | "landTimer" | "capTimer"): void {
    const timer = this[name];
    if (timer !== null) clearTimeout(timer);
    this[name] = null;
  }

  /**
   * Has AVPlay seek to `target`; exactly one of `made`, `refused` (its
   * error callback, or the call throwing) and `unanswered` (neither
   * callback in `SEEK_WATCHDOG_MS`, `HLS_SEEK_WATCHDOG_MS` in a stream) follows, before the calls that waited
   * for the seek run (a seek among them, once begun, is the newer one).
   */
  private seekPlayer(target: number, made: () => void, refused: (name: string) => void, unanswered: () => void): void {
    const mine = ++this.seekToken;
    this.seeking = true;
    const once =
      <A extends unknown[]>(fn: (...a: A) => void) =>
      (...a: A) => {
        if (mine !== this.seekToken) return;
        this.seekToken++;
        this.clearTimer("watchdog");
        try {
          fn(...a);
        } finally {
          this.seekDone();
        }
      };
    this.clearTimer("watchdog");
    this.watchdog = setTimeout(once(unanswered), this.source?.hls ? HLS_SEEK_WATCHDOG_MS : SEEK_WATCHDOG_MS);
    try {
      this.player.seekTo(target, once(made), once((e: unknown) => refused(nameOf(e))));
    } catch (e) {
      once(() => refused(nameOf(e)))();
    }
  }

  /** A position AVPlay gave: the position, unless it is a stale one from before a seek. */
  private accept(t: number): void {
    const h = this.hold;
    if (h) {
      if (this.seeking) return;
      const stale = Math.abs(t - h.target) > HOLD_NEAR_MS && Math.abs(t - h.origin) < Math.abs(t - h.target);
      if (stale && Date.now() < h.until) return;
      this.hold = null;
    }
    this.lastTime = t;
  }

  /** AVPlay's time, when it can say. */
  private currentTime(): number | null {
    try {
      return this.player.getCurrentTime();
    } catch {
      return null;
    }
  }

  private expectLanding(target: number, origin: number): void {
    this.clearLanding();
    this.landing = { target, origin, played: 0, since: null };
    this.armCap();
    if (this.state === "ready") this.landingRuns();
  }

  private armCap(): void {
    this.clearTimer("capTimer");
    this.capTimer = setTimeout(() => this.checkLanding(true), LAND_CAP_MS);
  }

  /** It plays (ready, and meant to play): the seek's landing is checked once it played a while. */
  private landingRuns(): void {
    const l = this.landing;
    if (!l || l.since !== null || this.state !== "ready" || !this.wantPlay) return;
    l.since = Date.now();
    this.armLandCheck();
  }

  /** It waits, or is paused: the time it played so far is kept, and the check waits for it to play again. */
  private landingWaits(): void {
    const l = this.landing;
    if (!l) return;
    if (l.since !== null) l.played += Date.now() - l.since;
    l.since = null;
    this.clearTimer("landTimer");
  }

  private armLandCheck(): void {
    this.clearTimer("landTimer");
    this.landTimer = setTimeout(() => this.checkLanding(false), LAND_CHECK_MS);
  }

  private clearLanding(): void {
    this.landing = null;
    this.clearTimer("landTimer");
    this.clearTimer("capTimer");
  }

  /**
   * A seek AVPlay said it made: it failed if it plays on nearer where it
   * came from than where it went (each moved on by the time it played), and
   * not near where it went; a short one (`LAND_JUDGED_MS`) only if it plays
   * on from where it was. A keyframe a little before or after the target,
   * or a stream that keeps waiting, is a seek made. A stream the server makes still waiting at `LAND_CAP_MS` is
   * the server's to make, not a seek failed: AVPlay's time then is still
   * the one from before it. A file still waiting then is given up on, but
   * as slow, not refused: a slow network isn't the kind of file's to
   * remember. Paused, the check waits until it plays.
   */
  private checkLanding(capped: boolean): void {
    const l = this.landing;
    if (!l || this.state === "ended" || (!capped && this.state === "buffering")) return;
    if (!this.wantPlay) {
      // Paused: AVPlay's time says nothing yet. Checked once it plays, the cap again from then.
      this.clearTimer("capTimer");
      return;
    }
    const slow = capped && this.state === "buffering";
    if (slow && this.source?.hls) {
      this.clearLanding();
      return;
    }
    const played = l.played + (l.since !== null ? Date.now() - l.since : 0);
    this.clearLanding();
    const t = this.currentTime();
    if (t === null) return;
    const fromTarget = Math.abs(t - (l.target + played));
    const fromOrigin = Math.abs(t - (l.origin + played));
    const short = Math.abs(l.target - l.origin) < LAND_JUDGED_MS;
    if (fromTarget <= LAND_NEAR_MS || (short ? fromOrigin > STAYED_MS : fromTarget <= fromOrigin)) {
      // Landed: AVPlay's time is the position again, wherever near the target it went.
      this.hold = null;
      if (t > 0) this.lastTime = t;
      return;
    }
    console.info(slow ? "[avplay] seek still loading" : "[avplay] seek didn't land", l.target, t);
    this.refused(l.target, t, slow);
  }

  /** A seek that didn't get to `target`: the position is where AVPlay is, and the session hears of it. */
  private refused(target: number, at: number | null, slow = false): void {
    this.hold = null;
    this.lastTime = at !== null && at >= 0 ? at : this.lastTime;
    this.listener?.seekFailed?.(target, slow);
  }

  private seekDone(): void {
    this.seeking = false;
    const q = this.queued;
    this.queued = [];
    for (const fn of q) this.run(fn);
  }

  prepare(source: EngineSource, play: boolean): void {
    const mine = ++this.generation;
    this.closePlayer();
    this.source = source;
    this.wantPlay = play;
    this.shown = false;
    this.httpStatus = null;
    this.opened = false;
    this.started = false;
    this.startMs = source.startMs;
    this.startAfterPlay = null;
    this.lastTime = source.startMs;
    this.set("buffering");
    const p = this.player;
    try {
      p.open(source.url);
      p.setListener({
        onbufferingstart: () => mine === this.generation && this.set("buffering"),
        // Not ready before it started (prepared, at its resume point, asked to play): the session would take it as paused.
        onbufferingcomplete: () => mine === this.generation && this.started && this.set(this.state === "ended" ? "ended" : "ready"),
        oncurrentplaytime: (ms) => {
          if (mine !== this.generation) return;
          this.accept(ms);
          if (!this.shown && this.wantPlay) {
            this.shown = true;
            this.set("ready");
            this.listener?.firstFrame();
          }
          const resume = this.startAfterPlay;
          if (resume !== null) {
            this.startAfterPlay = null;
            this.seekTo(resume);
          }
        },
        onstreamcompleted: () => {
          if (mine !== this.generation) return;
          this.set("ended");
          try {
            p.stop();
          } catch {
            // Already stopped.
          }
        },
        onerror: (e) => mine === this.generation && this.listener?.error(avplayError(e, this.source, this.httpStatus)),
        onerrormsg: (_id, message) => {
          if (mine !== this.generation) return;
          const status = statusOf(message);
          if (status) this.httpStatus = status;
        },
        onevent: () => undefined,
        onsubtitlechange: () => undefined,
      });
      // AVPlay places its plane in a 1920×1080 space, whatever the page's size.
      this.placePlane();
      p.setDisplayMethod("PLAYER_DISPLAY_MODE_LETTER_BOX");
      if (source.hls && this.uhd) {
        try {
          p.setStreamingProperty("ADAPTIVE_INFO", "FIXED_MAX_RESOLUTION=3840X2160");
        } catch {
          // Older sets take 4K streams without it.
        }
      }
    } catch (e) {
      this.listener?.error(avplayError(String((e as Error)?.name ?? e), source, null));
      return;
    }
    p.prepareAsync(
      () => {
        if (mine !== this.generation) return;
        this.opened = true;
        try {
          // AVPlay draws a file's own subtitles if asked; the app draws them.
          p.setSilentSubtitle(true);
        } catch {
          // Not on this set.
        }
        const start = () => {
          if (mine !== this.generation) return;
          this.started = true;
          // Asked to play before it says ready, so the session finds it playing.
          if (this.wantPlay) this.resume();
          this.set("ready");
          if (!this.wantPlay && !this.shown) {
            this.shown = true;
            this.listener?.firstFrame();
          }
          this.listener?.tracks();
        };
        // AVPlay can't seek to 0; the start of a file needs no seek.
        if (this.startMs >= 1000 && source.hls && this.hlsFromFirstSegment) {
          const target = Math.round(this.startMs);
          this.startAfterPlay = target;
          this.hold = { target, origin: 0, until: Date.now() + HLS_SEEK_WATCHDOG_MS };
          this.lastTime = target;
          start();
        } else if (this.startMs >= 1000) {
          const target = Math.round(this.startMs);
          const asked = this.requested;
          // A source opens at its start.
          this.hold = { target, origin: 0, until: Date.now() + LAND_CAP_MS };
          const made = (judged = true) => {
            if (mine !== this.generation) return;
            start();
            if (judged && asked === this.requested) this.expectLanding(target, 0);
          };
          this.seekPlayer(target, () => made(), (name) => {
            if (mine !== this.generation) return;
            console.info("[avplay] resume refused", target, name);
            // It plays from where it is rather than not at all; the session decides what next.
            start();
            if (asked !== this.requested) return;
            const at = this.currentTime();
            this.refused(target, at !== null && at > 0 ? at : 0);
          }, () => made(!source.hls));
        } else start();
      },
      (e) => mine === this.generation && this.listener?.error(avplayError(String((e as { name?: string })?.name ?? e), source, this.httpStatus)),
    );
  }

  /**
   * AVPlay plays, when it is ready or paused. Called directly rather than
   * through `run`: from a seek's own success callback the seek is over.
   */
  private resume(): void {
    try {
      const s = this.player.getState();
      if (s === "READY" || s === "PAUSED") this.player.play();
    } catch {
      // The state allows nothing now.
    }
  }

  play(): void {
    this.wantPlay = true;
    if (this.landing && this.capTimer === null) this.armCap();
    this.landingRuns();
    this.run(() => {
      const s = this.player.getState();
      if (s === "READY" || s === "PAUSED") this.player.play();
    });
  }

  pause(): void {
    this.wantPlay = false;
    this.landingWaits();
    this.run(() => {
      if (this.player.getState() === "PLAYING") this.player.pause();
    });
  }

  seekTo(ms: number): void {
    if (this.source && !this.opened) {
      // Not prepared yet: it starts there instead of at the resume point.
      this.startMs = Math.max(0, Math.round(ms));
      this.lastTime = this.startMs;
      return;
    }
    // A seek asked before the stream plays is where it goes once it does.
    if (this.startAfterPlay !== null) {
      if (ms < 1000) {
        // The start: the stream plays from there already.
        this.startAfterPlay = null;
        this.hold = null;
        this.lastTime = Math.max(0, Math.round(ms));
        return;
      }
      this.startAfterPlay = Math.round(ms);
      this.lastTime = this.startAfterPlay;
      if (this.hold) this.hold.target = this.startAfterPlay;
      return;
    }
    // Not to 0: AVPlay refuses it.
    const target = Math.max(100, Math.round(ms));
    const mine = ++this.requested;
    const origin = this.currentTime() ?? this.lastTime;
    this.lastTime = target;
    this.hold = { target, origin, until: Date.now() + LAND_CAP_MS };
    this.clearLanding();
    this.run(() => {
      // A newer seek was asked meanwhile: only that one goes to AVPlay.
      if (mine !== this.requested) return;
      const before = this.state;
      const from = this.currentTime() ?? origin;
      if (this.hold) this.hold.origin = from;
      // Making the seek before saying it waits: the session takes the wait for the seek's, not a stall.
      this.seeking = true;
      this.set("buffering");
      const made = (judged = true) => {
        if (mine !== this.requested) return;
        // Tizen 5's player can be left paused by a seek in a stream: it plays on when meant to.
        if (this.wantPlay) this.resume();
        if (this.state === "buffering") this.set("ready");
        if (judged) this.expectLanding(target, from);
      };
      const hls = this.source?.hls === true;
      // A seek AVPlay refuses leaves it playing where it was: it doesn't stay waiting.
      this.seekPlayer(target, () => made(), (name) => {
        if (mine !== this.requested) return;
        console.info("[avplay] seek refused", target, name);
        if (this.state === "buffering") this.set(before === "buffering" || before === "idle" ? "ready" : before);
        this.refused(target, this.currentTime());
      }, () => made(!hls));
    });
  }

  private closePlayer(): void {
    this.seeking = false;
    this.queued = [];
    this.seekToken++;
    this.hold = null;
    this.startAfterPlay = null;
    this.clearTimer("watchdog");
    this.clearLanding();
    try {
      const s = this.player.getState();
      if (s !== "NONE" && s !== "IDLE") this.player.stop();
      if (s !== "NONE") this.player.close();
    } catch {
      // Nothing open.
    }
  }

  stop(): void {
    this.generation++;
    this.wantPlay = false;
    this.closePlayer();
    this.source = null;
    this.set("idle");
  }

  release(): void {
    this.stop();
    this.object.parentNode?.removeChild(this.object);
    document.documentElement.classList.remove("avplay");
    this.listener = null;
  }

  positionMs(): number {
    try {
      const t = this.player.getCurrentTime();
      if (t > 0) this.accept(t);
    } catch {
      // Not playing.
    }
    return this.lastTime;
  }

  durationMs(): number {
    try {
      return Math.max(0, this.player.getDuration());
    } catch {
      return 0;
    }
  }

  /** AVPlay doesn't say how far it has buffered: just the position. */
  bufferedMs(): number {
    return this.positionMs();
  }

  /** Where the picture goes, in the canvas's 960×540 px; `null`: the whole screen. */
  private area: { left: number; top: number; width: number; height: number } | null = null;

  /** The plane to the area, in AVPlay's 1920×1080 space (twice the canvas); letterboxed inside it. */
  setArea(area: { left: number; top: number; width: number; height: number } | null): void {
    this.area = area;
    if (this.source) this.placePlane();
  }

  private placePlane(): void {
    const a = this.area;
    try {
      if (a) this.player.setDisplayRect(Math.round(a.left * 2), Math.round(a.top * 2), Math.round(a.width * 2), Math.round(a.height * 2));
      else this.player.setDisplayRect(0, 0, 1920, 1080);
    } catch {
      // Nothing open yet: placed as the next source opens.
    }
  }

  readonly knowsBuffer = false;

  /** A stream's bandwidth as AVPlay measures it (bits a second); nothing for a file. */
  bandwidthKbps(): number | null {
    try {
      const bps = Number(this.player.getStreamingProperty("CURRENT_BANDWIDTH"));
      return bps > 0 ? bps / 1000 : null;
    } catch {
      return null;
    }
  }


  /** A seek in a stream AVPlay hasn't answered yet (its watchdog bounds it): the server is making the place. */
  seekPending(): boolean {
    return this.seeking && this.source?.hls === true;
  }

  status(): EngineStatus {
    return this.state;
  }

  playing(): boolean {
    try {
      return this.player.getState() === "PLAYING";
    } catch {
      return false;
    }
  }

  private tracksOf(type: "AUDIO" | "TEXT" | "VIDEO"): AvplayTrack[] {
    try {
      return this.player.getTotalTrackInfo().filter((t) => t.type === type);
    } catch {
      return [];
    }
  }

  audioTracks(): EngineTrack[] {
    return this.tracksOf("AUDIO").map((t) => {
      const i = info(t);
      return { language: i.language || null, codec: i.fourCC || null, channels: i.channels ? Number(i.channels) : null };
    });
  }

  selectAudio(ordinal: number, language: string | null): boolean {
    const tracks = this.tracksOf("AUDIO");
    if (!tracks.length) return false;
    const lang = (language ?? "").toLowerCase();
    let pick = tracks[ordinal];
    if (pick && lang && info(pick).language && info(pick).language!.toLowerCase().indexOf(lang.slice(0, 2)) !== 0) pick = undefined;
    if (!pick && lang) pick = tracks.filter((t) => (info(t).language ?? "").toLowerCase().indexOf(lang.slice(0, 2)) === 0)[0];
    if (!pick) return false;
    const index = pick.index;
    this.run(() => this.player.setSelectTrack("AUDIO", index));
    return true;
  }

  /** AVPlay counts no frames. */
  frameCounts(): { dropped: number; total: number } | null {
    return null;
  }

  stats(): EngineStats | null {
    let streams: AvplayTrack[] = [];
    try {
      streams = this.player.getCurrentStreamInfo();
    } catch {
      return null;
    }
    const out: EngineStats = { decoder: "AVPlay", hardware: true };
    for (const s of streams) {
      const i = info(s);
      if (s.type === "VIDEO") {
        if (i.Width) out.width = Number(i.Width);
        if (i.Height) out.height = Number(i.Height);
        if (i.fourCC) out.codec = i.fourCC.toLowerCase();
        if (i.Bit_rate && Number(i.Bit_rate) > 0) out.bitrate = Number(i.Bit_rate);
      } else if (s.type === "AUDIO") {
        if (i.fourCC) out.audioCodec = i.fourCC.toLowerCase();
        if (i.channels) out.audioChannels = Number(i.channels);
      }
    }
    try {
      const bw = Number(this.player.getStreamingProperty("CURRENT_BANDWIDTH"));
      if (bw > 0) out.bandwidth = bw;
    } catch {
      // A file, not a stream.
    }
    return out;
  }
}

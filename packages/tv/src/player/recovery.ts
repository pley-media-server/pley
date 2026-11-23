import type { PlaybackMethod, PlaybackMode } from "../api/types";
import { t } from "../i18n";

/**
 * Why a fallback plays something other than the original: the file didn't
 * play, its audio didn't (another track plays, or the same converted), or
 * it didn't play smoothly.
 */
export type FallbackReason = "unplayable" | "audio_unplayable" | "not_smooth";

/** Why playback gave up. */
export type Failure = "unplayable" | "stuck" | "short" | "stopped" | "network" | "engine";

/** What went wrong in the TV's player, in the terms recovery decides on. */
export type EngineError =
  /**
   * It can't decode or open this: `audio` when the audio failed, `container` when the file did.
   * `audioCodec`: the server's name for the audio that failed, when the engine knows it.
   */
  | { kind: "unsupported"; audio: boolean; container: boolean; detail: string; audioCodec?: string | null }
  | { kind: "http"; status: number; detail: string }
  /** 410: stopped from elsewhere. */
  | { kind: "stopped"; message: string | null }
  | { kind: "playlist"; detail: string }
  | { kind: "network"; detail: string }
  | { kind: "other"; detail: string };

export type RecoveryAction =
  | { kind: "new_session"; mode: PlaybackMode | null; reason: FallbackReason | null }
  /** Ask the server how its transcode is doing, then call `afterServerCheck`. */
  | { kind: "check_server" }
  /** The server can't keep up: offer a lower quality, keep playing. */
  | { kind: "suggest_lower_quality" }
  | { kind: "fail"; failure: Failure; detail: string | null };

export const RECOVERED_AFTER_MS = 5_000;
export const MAX_RECREATE = 3;
export const MAX_NETWORK = 3;
export const MAX_STUCK = 1;
export const MAX_SHORT = 1;
/** Videos a remux copies, which the TV takes in another container. */
const COPYABLE_VIDEO = ["h264", "hevc"];
/** The server's last resort when it converts audio: refusing it would leave nothing to play. */
const AAC = "aac";

/**
 * The Android app's recovery ladder (core/player/session/Recovery.kt):
 *
 * - Audio the TV fails to play, in any session: a new session in the same
 *   mode whose capabilities leave that codec out (`refusedAudio`), so the
 *   server plays another track as it is or converts this one, the video
 *   untouched; once per codec, never AAC. A remux or a transcode would only
 *   copy the same audio again: the server copies what the capabilities say
 *   plays. The TV's player doesn't tell its audio output's failures from
 *   its decoders', so every one counts at once (Android first gives an
 *   output that failed a new session as it was).
 * - Direct play the TV can't decode or open: a remux when the video is
 *   H.264 or HEVC and only the container or the audio failed, else a
 *   transcode; each once.
 * - 404 or 410 ("playback session expired") or an unreadable playlist: a
 *   new session at the position, at most 3 times.
 * - Network errors: a new session, at most 3.
 * - A 12 s stall: a direct play that has data but doesn't move becomes a
 *   transcode; a stream asks the server first; a stream that keeps
 *   stopping gets one new session, a starving one up to 3.
 * - Media that ends where it started, long before its end: a direct play
 *   becomes a transcode, a stream gets one new session, then it fails.
 *
 * Budgets start over after 5 s of playback and on Retry.
 */
export class Recovery {
  fallbackMode: PlaybackMode | null = null;
  fallbackReason: FallbackReason | null = null;
  struggleSwitched = false;
  /** Audio codecs (the server's names) this TV failed to play in this item: left out of the capabilities sent from then on. */
  refusedAudio: readonly string[] = [];
  private remuxTried = false;
  private transcodeTried = false;
  recreate = 0;
  network = 0;
  stuck = 0;
  short = 0;
  private playedFrom: number | null = null;

  /** Where playback is now: 5 s played on from where it last (re)started means the trouble is over. */
  onPosition(positionMs: number): void {
    const from = this.playedFrom;
    if (from === null || positionMs < from) {
      this.playedFrom = positionMs;
    } else if (positionMs - from >= RECOVERED_AFTER_MS) {
      this.recreate = 0;
      this.network = 0;
      this.stuck = 0;
      this.short = 0;
      this.playedFrom = positionMs;
    }
  }

  /** A seek or a new session: progress counts from where it lands. */
  onJump(): void {
    this.playedFrom = null;
  }

  /**
   * What to do about `error` in a session played as `method`, whose video is `videoCodec` and whose
   * audio, as it reached the TV's player, is `audioCodec` (the server's name, one the capabilities
   * sent list; `null` when not known).
   */
  decide(error: EngineError, method: PlaybackMethod, videoCodec: string | null, audioCodec: string | null = null): RecoveryAction {
    switch (error.kind) {
      case "unsupported":
        return (
          this.refuseAudio(error.audio, (audioCodec ?? "").toLowerCase()) ??
          this.fallback(method, videoCodec, error.audio || error.container, error.detail)
        );
      case "http":
        return error.status === 404 || error.status === 410 ? this.expired(error.detail) : this.networkError(error.detail);
      case "playlist":
        return this.expired(error.detail);
      case "network":
        return this.networkError(error.detail);
      case "stopped":
        return { kind: "fail", failure: "stopped", detail: error.message };
      case "other":
        return { kind: "fail", failure: "engine", detail: error.detail };
    }
  }

  /**
   * The audio `codec` failed: refused, and a new session in the mode asked for now; `null` when the
   * ladder's next step is what helps instead (a video failure, a codec unknown or refused already, AAC).
   */
  private refuseAudio(audio: boolean, codec: string): RecoveryAction | null {
    if (!audio || !codec || codec === AAC || this.refusedAudio.indexOf(codec) >= 0) return null;
    this.refusedAudio = this.refusedAudio.concat([codec]);
    this.fallbackReason = "audio_unplayable";
    return { kind: "new_session", mode: this.fallbackMode, reason: "audio_unplayable" };
  }

  private fallback(method: PlaybackMethod, videoCodec: string | null, partial: boolean, detail: string): RecoveryAction {
    const copyable = COPYABLE_VIDEO.indexOf((videoCodec ?? "").toLowerCase()) >= 0;
    if (method === "direct" && partial && copyable && !this.remuxTried) {
      this.remuxTried = true;
      return this.switchTo("remux", "unplayable");
    }
    if (method !== "transcode" && !this.transcodeTried) {
      this.transcodeTried = true;
      return this.switchTo("transcode", "unplayable");
    }
    return { kind: "fail", failure: "unplayable", detail };
  }

  private switchTo(mode: PlaybackMode, reason: FallbackReason): RecoveryAction {
    this.fallbackMode = mode;
    this.fallbackReason = reason;
    return { kind: "new_session", mode, reason };
  }

  private expired(detail: string): RecoveryAction {
    if (this.recreate < MAX_RECREATE) {
      this.recreate++;
      return { kind: "new_session", mode: null, reason: null };
    }
    return { kind: "fail", failure: "stopped", detail };
  }

  private networkError(detail: string): RecoveryAction {
    if (this.network < MAX_NETWORK) {
      this.network++;
      return { kind: "new_session", mode: null, reason: null };
    }
    return { kind: "fail", failure: "network", detail };
  }

  /** Playback stood still for 12 s while meant to play; `starving` when nothing is buffered ahead. */
  onStall(method: PlaybackMethod, starving: boolean): RecoveryAction {
    if (!starving && method === "direct" && !this.transcodeTried) {
      this.transcodeTried = true;
      return this.switchTo("transcode", "not_smooth");
    }
    if (method !== "direct") return { kind: "check_server" };
    return this.afterStall(starving);
  }

  /** The server's transcode runs at `speed` (`null` when it can't say). */
  afterServerCheck(speed: number | null, starving: boolean): RecoveryAction {
    return speed !== null && speed < 1 ? { kind: "suggest_lower_quality" } : this.afterStall(starving);
  }

  private afterStall(starving: boolean): RecoveryAction {
    if (!starving) {
      if (this.stuck < MAX_STUCK) {
        this.stuck++;
        return { kind: "new_session", mode: null, reason: null };
      }
      return { kind: "fail", failure: "stuck", detail: null };
    }
    if (this.recreate < MAX_RECREATE) {
      this.recreate++;
      return { kind: "new_session", mode: null, reason: null };
    }
    return { kind: "fail", failure: "stopped", detail: null };
  }

  /** The media ended right where it was started, long before its end. */
  onEndedShort(method: PlaybackMethod): RecoveryAction {
    if (method === "direct" && !this.transcodeTried) {
      this.transcodeTried = true;
      return this.switchTo("transcode", "unplayable");
    }
    if (this.short < MAX_SHORT) {
      this.short++;
      return { kind: "new_session", mode: null, reason: null };
    }
    return { kind: "fail", failure: "short", detail: null };
  }

  /** The decoder drops frames: one switch to a transcode per item; `null` when it switched already or transcodes. */
  onStruggling(method: PlaybackMethod): RecoveryAction | null {
    if (this.struggleSwitched || method === "transcode") return null;
    this.struggleSwitched = true;
    this.transcodeTried = true;
    return this.switchTo("transcode", "not_smooth");
  }

  playOriginal(): void {
    this.fallbackMode = null;
    this.fallbackReason = null;
  }

  qualityChanged(): void {
    this.playOriginal();
  }

  retry(): void {
    this.recreate = 0;
    this.network = 0;
    this.stuck = 0;
    this.short = 0;
    this.playedFrom = null;
  }

  newItem(): void {
    this.retry();
    this.fallbackMode = null;
    this.fallbackReason = null;
    this.struggleSwitched = false;
    this.refusedAudio = [];
    this.remuxTried = false;
    this.transcodeTried = false;
  }
}

/** What people read for a failure (core/player strings_session.xml). */
export function failureText(failure: Failure, detail: string | null): string {
  switch (failure) {
    case "unplayable":
      return t("player.errors.unplayable");
    case "stuck":
      return t("player.errors.sameSpot");
    case "short":
      return t("player.errors.endsEarly");
    case "stopped":
      return t("player.errors.stalled");
    case "network":
      return t("player.errors.reachHint");
    case "engine": {
      // The TV's own words when it gives any ("PIPELINE_ERROR_DECODE"), not its generic ones.
      const short = (detail ?? "").split(":")[0]!.trim();
      return short && !/^playback failed$/i.test(short) ? t("player.errors.playerGaveUpWith", { detail: short }) : t("player.errors.playerGaveUp");
    }
  }
}

/**
 * Dropped frames, sampled every 2 s while playing (the Android app's
 * StruggleMeter): struggling when more than 1 % of the frames of a 10 s
 * window were dropped. Counts that go back (a new decoder) restart it.
 */
export class StruggleMeter {
  private samples: { at: number; dropped: number; total: number }[] = [];

  static readonly WINDOW_MS = 10_000;
  static readonly SHARE = 0.01;

  /** A sample of the decoder's counters at `at` ms; whether it struggles now. */
  sample(at: number, dropped: number, total: number): boolean {
    const last = this.samples[this.samples.length - 1];
    if (last && (dropped < last.dropped || total < last.total)) this.samples = [];
    this.samples.push({ at, dropped, total });
    while (this.samples.length > 1 && at - this.samples[0]!.at > StruggleMeter.WINDOW_MS) this.samples.shift();
    const first = this.samples[0]!;
    if (at - first.at < StruggleMeter.WINDOW_MS - 2_000) return false;
    const frames = total - first.total;
    const drops = dropped - first.dropped;
    if (frames <= 0) return false;
    if (drops / frames > StruggleMeter.SHARE) {
      this.samples = [];
      return true;
    }
    return false;
  }

  reset(): void {
    this.samples = [];
  }
}

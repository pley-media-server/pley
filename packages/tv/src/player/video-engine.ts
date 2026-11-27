import { t } from "../i18n";
import { HLS_MIME, type EngineListener, type EngineSource, type EngineStats, type EngineStatus, type EngineTrack, type PlayerEngine } from "./engine";
import type { EngineError } from "./recovery";

export { HLS_MIME };

/**
 * A seek the element hasn't said it made (`seeked`) in this long is judged
 * by where the element is: less than the session's stall (`STALL_MS`), so
 * the session hears of a dropped seek before it would take the wait for a
 * stall. One the element is still fetching (`seeking`) isn't judged then:
 * a slow network is the session's stall to weigh, not a refusal.
 */
export const SEEK_WATCHDOG_MS = 8_000;
/**
 * A seek is made when the element's time is this near the target, or
 * nearer the target than where it came from: the TV's pipeline lands on a
 * keyframe, and a long-GOP HEVC file's are 10 s apart. As AVPlay's rule.
 */
export const SEEK_LAND_NEAR_MS = 5_000;
/**
 * A seek shorter than this failed only when the element stayed where it
 * was (within `SEEK_STAYED_MS`): the keyframe before the target can be as
 * far behind it as a short skip is long. As AVPlay's rule.
 */
export const SEEK_JUDGED_MS = 30_000;
export const SEEK_STAYED_MS = 1_000;

/** How long a failed file's address is given to answer (`addressStatus`) before the failure is taken as the file's. */
const ADDRESS_ASK_MS = 5_000;

/** A picture held across a switch (`holdPicture`) is kept at most this wide: the app's screen is 1920 pixels. */
const HOLD_MAX_WIDTH = 1920;

interface AudioTrackLike {
  enabled: boolean;
  language: string;
  label: string;
  kind: string;
  id: string;
}

interface AudioTrackListLike {
  length: number;
  [index: number]: AudioTrackLike;
}

type VideoWithTracks = HTMLVideoElement & {
  audioTracks?: AudioTrackListLike;
  webkitDroppedFrameCount?: number;
  webkitDecodedFrameCount?: number;
};

/**
 * A player on a <video> element: the TV's web engine hands its address to
 * the TV's own media pipeline, which fetches a file played as it is by
 * HTTP ranges and seeks in it from there (on Samsung TVs, as Jellyfin's
 * player does); in a desktop browser it plays what the browser plays.
 * Subclasses say how a source is given to the element.
 */
export class VideoEngine implements PlayerEngine {
  readonly name: string;
  protected readonly video: VideoWithTracks;
  protected listener: EngineListener | null = null;
  protected source: EngineSource | null = null;
  private state: EngineStatus = "idle";
  private shown = false;
  private wantPlay = false;
  private pendingSeek: number | null = null;
  private generation = 0;
  private readonly off: (() => void)[] = [];
  /** The last picture of a source left for one made ready beside it, shown until that one shows its own. */
  private held: HTMLCanvasElement | null = null;
  /** A seek (or the start at the resume point) the element hasn't said it made: where to, from where, and its watchdog. */
  private landing: { target: number; origin: number; timer: ReturnType<typeof setTimeout> | null } | null = null;

  constructor(host: HTMLElement, name = t("player.info.videoElement")) {
    this.name = name;
    const video = document.createElement("video") as VideoWithTracks;
    video.className = "engine-video";
    video.preload = "auto";
    video.setAttribute("playsinline", "");
    video.controls = false;
    host.appendChild(video);
    this.video = video;
    const on = (type: string, fn: () => void) => {
      video.addEventListener(type, fn);
      this.off.push(() => video.removeEventListener(type, fn));
    };
    on("waiting", () => this.set("buffering"));
    on("seeking", () => this.set("buffering"));
    on("canplay", () => {
      if (this.state !== "ended") this.set("ready");
    });
    on("playing", () => {
      this.set("ready");
      this.firstPicture();
    });
    on("seeked", () => {
      this.landed();
      if (this.state === "buffering" && this.video.readyState >= 3) this.set("ready");
    });
    on("loadedmetadata", () => {
      // An element can't start at a position: it seeks once the duration is known.
      if (this.pendingSeek !== null) {
        const at = this.pendingSeek;
        this.pendingSeek = null;
        if (at > 0) this.moveTo(at);
      }
      this.listener?.tracks();
    });
    on("loadeddata", () => {
      if (!this.wantPlay) this.firstPicture();
    });
    on("ended", () => this.set("ended"));
    on("error", () => this.failed());
  }

  setListener(listener: EngineListener): void {
    this.listener = listener;
  }

  /** How the element is given `source` (LG adds its mediaOption here). */
  protected attach(source: EngineSource): void {
    this.pendingSeek = source.startMs > 0 && !source.live ? source.startMs : null;
    this.video.src = source.url;
  }

  prepare(source: EngineSource, play: boolean): void {
    this.generation++;
    this.clearLanding();
    this.detach();
    this.source = source;
    this.shown = false;
    this.wantPlay = play;
    this.set("buffering");
    this.attach(source);
    this.video.load();
    if (play) this.startPlaying();
  }

  private startPlaying(): void {
    const mine = this.generation;
    const p = this.video.play() as Promise<void> | undefined;
    if (p && typeof p.catch === "function") {
      p.catch((e: unknown) => {
        // A pause or a new source interrupts play(): nothing went wrong.
        if (mine !== this.generation || (e instanceof Error && e.name === "AbortError")) return;
        if (e instanceof Error && e.name === "NotSupportedError") this.failed();
      });
    }
  }

  play(): void {
    this.wantPlay = true;
    if (this.state === "ended") this.video.currentTime = 0;
    this.startPlaying();
  }

  pause(): void {
    this.wantPlay = false;
    this.video.pause();
  }

  seekTo(ms: number): void {
    if (this.video.readyState < 1) {
      this.pendingSeek = ms;
      return;
    }
    if (this.state === "ended") this.set("buffering");
    this.moveTo(Math.max(0, ms));
  }

  /** Sets the element's time to `target`, and judges the seek once the element says it made it, or in `SEEK_WATCHDOG_MS`. */
  private moveTo(target: number): void {
    this.clearLanding();
    const mine = this.generation;
    const origin = (this.video.currentTime || 0) * 1000;
    const timer = setTimeout(() => {
      const l = this.landing;
      if (mine !== this.generation || !l || l.timer !== timer) return;
      l.timer = null;
      // Still fetching from the place: judged when it gets there; the session's stall clock runs meanwhile.
      if (this.video.seeking) {
        console.info("[video] seek still loading", target);
        return;
      }
      console.info("[video] seek unanswered", target);
      this.landed();
    }, SEEK_WATCHDOG_MS);
    this.landing = { target, origin, timer };
    this.video.currentTime = target / 1000;
  }

  /**
   * The element said a seek was made (or let it go): it failed when the
   * element's time is far from the target and no nearer it than where it
   * came from; a short seek (`SEEK_JUDGED_MS`), only when it stayed there.
   */
  private landed(): void {
    const l = this.landing;
    if (!l) return;
    this.clearLanding();
    const at = (this.video.currentTime || 0) * 1000;
    const fromTarget = Math.abs(at - l.target);
    const fromOrigin = Math.abs(at - l.origin);
    const short = Math.abs(l.target - l.origin) < SEEK_JUDGED_MS;
    if (fromTarget <= SEEK_LAND_NEAR_MS || (short ? fromOrigin > SEEK_STAYED_MS : fromTarget < fromOrigin)) return;
    console.info("[video] seek didn't land", l.target, at);
    this.listener?.seekFailed?.(l.target);
  }

  private clearLanding(): void {
    if (this.landing?.timer) clearTimeout(this.landing.timer);
    this.landing = null;
  }

  /** A seek in a stream the server makes, not yet made; a file's seeks are the TV's own. */
  seekPending(): boolean {
    return this.landing !== null && this.source?.hls === true;
  }

  /** Where the element goes, in the canvas's px; `null`: all of its host. */
  setArea(area: { left: number; top: number; width: number; height: number } | null): void {
    const s = this.video.style;
    s.left = area ? `${area.left}px` : "";
    s.top = area ? `${area.top}px` : "";
    s.width = area ? `${area.width}px` : "";
    s.height = area ? `${area.height}px` : "";
  }

  /**
   * Draws the picture showing now onto a canvas over the element, kept
   * until the next source shows its first picture. A desktop browser
   * draws it; where the TV's pipeline keeps its pictures to itself (LG's,
   * on a plane of its own) nothing is drawn, and the screen is black
   * meanwhile, as without it.
   */
  holdPicture(): void {
    const v = this.video;
    if (!v.videoWidth || !v.videoHeight || v.readyState < 2) return;
    let canvas = this.held;
    if (!canvas) {
      canvas = document.createElement("canvas");
      canvas.className = "engine-held";
      canvas.style.display = "none";
      v.parentNode?.insertBefore(canvas, v.nextSibling);
      this.held = canvas;
    }
    const scale = Math.min(1, HOLD_MAX_WIDTH / v.videoWidth);
    canvas.width = Math.max(1, Math.round(v.videoWidth * scale));
    canvas.height = Math.max(1, Math.round(v.videoHeight * scale));
    try {
      const context = canvas.getContext("2d");
      if (!context) throw new Error("no 2d context");
      context.drawImage(v, 0, 0, canvas.width, canvas.height);
    } catch {
      this.letGo();
      return;
    }
    canvas.style.display = "block";
  }

  /** The held picture goes: the new source shows, failed, or stopped. */
  private letGo(): void {
    const canvas = this.held;
    if (!canvas) return;
    canvas.style.display = "none";
    // Its pixels go too: a 1080p picture is 8 MB a TV can use elsewhere.
    if (canvas.width > 1) {
      canvas.width = 1;
      canvas.height = 1;
    }
  }

  stop(): void {
    this.generation++;
    this.clearLanding();
    this.wantPlay = false;
    this.letGo();
    this.detach();
    this.source = null;
    this.set("idle");
  }

  /** Empties the element so the pipeline lets the source go. */
  protected detach(): void {
    this.video.pause();
    while (this.video.firstChild) this.video.removeChild(this.video.firstChild);
    this.video.removeAttribute("src");
    try {
      this.video.load();
    } catch {
      // Nothing loaded.
    }
  }

  release(): void {
    this.stop();
    for (const f of this.off) f();
    this.off.length = 0;
    this.video.parentNode?.removeChild(this.video);
    this.held?.parentNode?.removeChild(this.held);
    this.held = null;
    this.listener = null;
  }

  /** The element's own rate: its default one too, which each new source starts at; browsers keep the voices' pitch. */
  setSpeed(rate: number): boolean {
    this.video.defaultPlaybackRate = rate;
    this.video.playbackRate = rate;
    return true;
  }

  positionMs(): number {
    return Math.max(0, (this.video.currentTime || 0) * 1000);
  }

  durationMs(): number {
    const d = this.video.duration;
    return isFinite(d) && d > 0 ? d * 1000 : 0;
  }

  bufferedMs(): number {
    const b = this.video.buffered;
    const at = this.video.currentTime;
    for (let i = 0; i < b.length; i++) {
      if (b.start(i) <= at + 0.25 && b.end(i) >= at) return b.end(i) * 1000;
    }
    return at * 1000;
  }

  status(): EngineStatus {
    return this.state;
  }

  playing(): boolean {
    return !this.video.paused && !this.video.ended;
  }

  audioTracks(): EngineTrack[] {
    const list = this.video.audioTracks;
    if (!list) return [];
    const out: EngineTrack[] = [];
    for (let i = 0; i < list.length; i++) {
      const t = list[i]!;
      out.push({ language: t.language || null, codec: null, channels: null });
    }
    return out;
  }

  /**
   * The element's tracks follow the pipeline's own order, which isn't
   * always the file's (LG moves the default first): the ordinal is taken
   * when its language agrees, else the first track in that language.
   */
  selectAudio(ordinal: number, language: string | null): boolean {
    const list = this.video.audioTracks;
    if (!list || !list.length) return false;
    const want = norm(language);
    let pick = -1;
    const at = list[ordinal];
    if (at && (!want || norm(at.language) === want || !at.language)) pick = ordinal;
    if (pick < 0 && want) {
      for (let i = 0; i < list.length; i++) {
        if (norm(list[i]!.language) === want) {
          pick = i;
          break;
        }
      }
    }
    if (pick < 0) return false;
    for (let i = 0; i < list.length; i++) list[i]!.enabled = i === pick;
    return true;
  }

  frameCounts(): { dropped: number; total: number } | null {
    const v = this.video as VideoWithTracks & { getVideoPlaybackQuality?: () => { droppedVideoFrames: number; totalVideoFrames: number } };
    if (v.getVideoPlaybackQuality) {
      const q = v.getVideoPlaybackQuality();
      return { dropped: q.droppedVideoFrames, total: q.totalVideoFrames };
    }
    if (typeof v.webkitDroppedFrameCount === "number" && typeof v.webkitDecodedFrameCount === "number") {
      return { dropped: v.webkitDroppedFrameCount, total: v.webkitDecodedFrameCount };
    }
    return null;
  }

  stats(): EngineStats | null {
    const v = this.video;
    if (!v.videoWidth) return null;
    const ahead = this.bufferedMs() - this.positionMs();
    return { width: v.videoWidth, height: v.videoHeight, bufferedAheadMs: ahead > 0 ? ahead : 0 };
  }

  protected set(status: EngineStatus): void {
    if (this.state === status) return;
    this.state = status;
    this.listener?.status(status);
  }

  protected firstPicture(): void {
    if (this.shown) return;
    this.shown = true;
    this.letGo();
    this.listener?.firstFrame();
  }

  /**
   * The element's error, as the session's recovery reads it. The TV says
   * the same (MEDIA_ERR_SRC_NOT_SUPPORTED) of a file it can't play and of
   * an address that failed (404, or 410 once the server stopped the
   * session): a file's address is asked first, and a failed one is the
   * session's to look into, not the file's fault.
   */
  protected failed(): void {
    const e = this.video.error;
    const source = this.source;
    if (!e || !source) return;
    this.letGo();
    const error = mediaError(e.code, e.message || "", source);
    if (e.code !== 4 || source.hls) {
      this.listener?.error(error);
      return;
    }
    const mine = this.generation;
    const detail = e.message || "the address failed";
    void this.addressStatus(source.url).then((status) => {
      if (mine !== this.generation || this.source !== source) return;
      if (status === 0) this.listener?.error({ kind: "network", detail });
      else if (status !== null && status >= 400) this.listener?.error({ kind: "http", status, detail });
      else this.listener?.error(error);
    });
  }

  /**
   * The status a file's address answers now (`HEAD`), 0 when nothing
   * answers, `null` when it can't be told in `ADDRESS_ASK_MS`.
   */
  protected addressStatus(url: string): Promise<number | null> {
    if (typeof fetch !== "function") return Promise.resolve(null);
    return new Promise((resolve) => {
      const timer = setTimeout(() => resolve(null), ADDRESS_ASK_MS);
      fetch(url, { method: "HEAD", cache: "no-store" }).then(
        (r) => {
          clearTimeout(timer);
          resolve(r.status);
        },
        () => {
          clearTimeout(timer);
          resolve(0);
        },
      );
    });
  }
}

function norm(language: string | null | undefined): string {
  const l = (language ?? "").toLowerCase();
  // Two- and three-letter codes of the same language compare equal.
  return LANG3[l] ?? l;
}

const LANG3: Record<string, string> = {
  en: "eng", fr: "fre", fra: "fre", de: "ger", deu: "ger", es: "spa", it: "ita", ja: "jpn", ko: "kor", zh: "chi", zho: "chi",
  pt: "por", ru: "rus", nl: "dut", nld: "dut", sv: "swe", no: "nor", da: "dan", fi: "fin", pl: "pol", tr: "tur", ar: "ara",
  he: "heb", hi: "hin", cs: "cze", ces: "cze", el: "gre", ell: "gre", hu: "hun", ro: "rum", ron: "rum", th: "tha",
};

/** MediaError codes as the recovery ladder reads them. */
export function mediaError(code: number, message: string, source: EngineSource): EngineError {
  switch (code) {
    case 2:
      return { kind: "network", detail: message || "the network failed while loading" };
    case 3:
      return { kind: "unsupported", audio: /audio/i.test(message), container: false, detail: message || "the TV couldn't decode it" };
    case 4:
      return source.hls
        ? { kind: "playlist", detail: message || "the stream couldn't be opened" }
        : { kind: "unsupported", audio: /audio/i.test(message), container: /demux|container|format/i.test(message), detail: message || "the TV can't play this file" };
    default:
      return { kind: "other", detail: message || "playback failed" };
  }
}

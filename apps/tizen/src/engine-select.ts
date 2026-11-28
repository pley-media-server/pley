import type { EngineListener, EngineSource, EngineStats, EngineStatus, EngineTrack, PlayerEngine } from "pley-tv/player/engine";
import type { EngineError } from "pley-tv/player/recovery";
import type { PlayerChoice } from "pley-tv/platform";

/** Settings › Playback › Player: `auto` picks per source, `avplay` plays everything on AVPlay (the way back). */
export type PlayerPreference = PlayerChoice;

export type PlayerName = "video" | "avplay";

export interface EngineChoice {
  engine: PlayerName;
  /** Why, for the console: "direct", "hls", "hdr"… */
  reason: string;
}

/** The containers the TV's <video> is given a file in, as it is. */
const ELEMENT_CONTAINERS = ["video/x-matroska", "video/mp4", "video/mp2t", "video/webm"];
const ELEMENT_VIDEO = ["h264", "avc", "avc1", "hevc", "h265", "hvc1", "hev1"];
const ELEMENT_AUDIO = ["aac", "ac3", "eac3", "mp3", "flac", "opus"];
/** The largest picture <video> is given: Samsung's <video> stops at 1080p, and 4K needs AVPlay (docs/TV_PLATFORMS.md). */
const MAX_WIDTH = 1920;
const MAX_HEIGHT = 1080;
/**
 * At a seek AVPlay refused in a file, <video> is tried with HEVC up to 4K
 * before the server remuxes it: Jellyfin's player plays such files on
 * <video> on these TVs. A file it can't open there goes to the remux.
 */
const SEEK_HEVC_MAX_WIDTH = 3840;
const SEEK_HEVC_MAX_HEIGHT = 2160;
const HEVC = ["hevc", "h265", "hvc1", "hev1"];

/**
 * Which of the TV's players plays `source`. A file played as it is goes to
 * <video>, whose pipeline fetches it by HTTP range and seeks in it from
 * there (as Jellyfin's player does on these TVs), when it is H.264 or HEVC
 * up to 1080p in SDR, in MKV, MP4, TS or WebM, with the audio the file
 * starts with in a codec the TV decodes. AVPlay plays the rest: streams
 * (HLS, live channels), 4K and 8K, HDR and Dolby Vision, which AVPlay
 * takes from the stream itself, and another audio track than the file's
 * own (<video> can't be told which).
 */
export function chooseEngine(source: EngineSource, preference: PlayerPreference, forSeek = false): EngineChoice {
  const avplay = (reason: string): EngineChoice => ({ engine: "avplay", reason });
  if (preference === "avplay") return avplay("setting");
  if (source.live) return avplay("live");
  if (source.hls) return avplay("hls");
  const v = source.video;
  if (!v) return avplay("no video");
  const hevc = HEVC.indexOf(v.codec.toLowerCase()) >= 0;
  const [maxWidth, maxHeight] = forSeek && hevc ? [SEEK_HEVC_MAX_WIDTH, SEEK_HEVC_MAX_HEIGHT] : [MAX_WIDTH, MAX_HEIGHT];
  if ((v.width ?? 0) > maxWidth || (v.height ?? 0) > maxHeight) return avplay("above 1080p");
  if (v.range !== "sdr" || v.dvProfile !== null) return avplay("hdr");
  if (ELEMENT_VIDEO.indexOf(v.codec.toLowerCase()) < 0) return avplay("codec");
  if (!source.mime || ELEMENT_CONTAINERS.indexOf(source.mime) < 0) return avplay("container");
  if (source.audioDefault === false) return avplay("audio track");
  const audio = (source.audioCodec ?? "").toLowerCase();
  if (audio && ELEMENT_AUDIO.indexOf(audio) < 0 && audio.indexOf("pcm") !== 0) return avplay("audio codec");
  return { engine: "video", reason: "direct" };
}

/** What a kind of file failed on <video> by: its container and codecs. */
function kindOf(source: EngineSource): string {
  return `${source.mime ?? ""}/${source.video?.codec ?? ""}/${source.audioCodec ?? ""}`;
}

type Area = { left: number; top: number; width: number; height: number } | null;

/** How the TV's players are used: by the TV's year. */
export interface SelectingOptions {
  /**
   * A file played as it is starts on <video> (`chooseEngine`): sets before
   * 2020, whose AVPlay refuses seeks in such files. Later sets start it on
   * AVPlay, and go to <video> at a seek AVPlay refused.
   */
  videoFirst: boolean;
  /** AVPlay starts a stream at its first segment (`AvplayEngine.hlsFromFirstSegment`). */
  hlsFromFirstSegment: boolean;
}

/**
 * Samsung's two players behind one engine: <video> for a file played as it
 * is on sets before 2020 (`chooseEngine`, `videoFirst`) and at a seek AVPlay
 * refused (`switchForSeek`), AVPlay for the rest. They share the screen and the
 * decoder, so one is let go entirely (`release`) before the other is made.
 * A file <video> can't open (MEDIA_ERR_SRC_NOT_SUPPORTED, MEDIA_ERR_DECODE)
 * before its first picture goes on with AVPlay at the same place, the
 * session none the wiser, and that kind of file stays on AVPlay while the
 * player screen is open.
 */
export class SelectingEngine implements PlayerEngine {
  private active: PlayerEngine | null = null;
  private activeName: PlayerName | null = null;
  private listener: EngineListener | null = null;
  private source: EngineSource | null = null;
  private wantPlay = false;
  private shown = false;
  /** Where the source playing was last asked to be (its start, or a seek). */
  private askedMs = 0;
  private area: Area = null;
  private areaSet = false;
  /** Kinds of file <video> couldn't open here. */
  private readonly failedOnElement: string[] = [];
  /** <video> plays the source from a seek AVPlay refused (`switchForSeek`): if it can't open it, the seek failed. */
  private forSeek = false;
  /** As AVPlay starts streams: it plays every stream. */
  readonly hlsFromFirstSegment: boolean;

  private readonly make: { video(): PlayerEngine; avplay(): PlayerEngine };
  private readonly preference: () => PlayerPreference;
  private readonly videoFirst: boolean;

  constructor(
    make: { video(): PlayerEngine; avplay(): PlayerEngine },
    preference: () => PlayerPreference,
    options: SelectingOptions = { videoFirst: false, hlsFromFirstSegment: false },
  ) {
    this.make = make;
    this.preference = preference;
    this.videoFirst = options.videoFirst;
    this.hlsFromFirstSegment = options.hlsFromFirstSegment;
  }

  get name(): string {
    return this.active?.name ?? "AVPlay";
  }

  /** AVPlay doesn't say how far it has loaded; <video> does. */
  get knowsBuffer(): boolean | undefined {
    return this.active?.knowsBuffer;
  }

  private pref(): PlayerPreference {
    try {
      return this.preference() === "avplay" ? "avplay" : "auto";
    } catch {
      return "auto";
    }
  }

  private choose(source: EngineSource, forSeek = false): EngineChoice {
    const choice = chooseEngine(source, this.pref(), forSeek);
    if (choice.engine !== "video") return choice;
    if (this.failedOnElement.indexOf(kindOf(source)) >= 0) return { engine: "avplay", reason: "video failed" };
    // A set whose AVPlay seeks in files starts them there; <video> is for a seek it refuses.
    if (!forSeek && !this.videoFirst) return { engine: "avplay", reason: "avplay first" };
    return choice;
  }

  setListener(listener: EngineListener): void {
    this.listener = listener;
  }

  /** The player `name`, made after the one playing is let go; its events reach the session while it is the one. */
  private use(name: PlayerName): PlayerEngine {
    if (this.active && this.activeName === name) return this.active;
    this.letGo();
    const engine = name === "video" ? this.make.video() : this.make.avplay();
    this.active = engine;
    this.activeName = name;
    const mine = () => this.active === engine;
    engine.setListener({
      status: (s) => mine() && this.listener?.status(s),
      firstFrame: () => {
        if (!mine()) return;
        this.shown = true;
        this.listener?.firstFrame();
      },
      error: (e) => mine() && this.onError(name, e),
      tracks: () => mine() && this.listener?.tracks(),
      seekFailed: (t, slow) => mine() && this.listener?.seekFailed?.(t, slow),
    });
    if (this.areaSet) engine.setArea?.(this.area);
    return engine;
  }

  private letGo(): void {
    const engine = this.active;
    this.active = null;
    this.activeName = null;
    engine?.release();
  }

  /**
   * <video> couldn't open the file before its first picture: AVPlay goes on
   * at the same place, or, at a seek AVPlay refused, the session hears the
   * seek failed (AVPlay would refuse it again; the session remuxes).
   */
  private onError(name: PlayerName, error: EngineError): void {
    const source = this.source;
    if (name === "video" && !this.shown && source && error.kind === "unsupported") {
      console.info("[engine] <video> can't play it", error.detail);
      this.failedOnElement.push(kindOf(source));
      if (this.forSeek) {
        this.forSeek = false;
        this.listener?.seekFailed?.(this.askedMs);
        return;
      }
      console.info("[engine] chosen", { engine: "avplay", reason: "video failed" });
      this.start("avplay", { ...source, startMs: this.askedMs }, this.wantPlay, true);
      return;
    }
    this.listener?.error(error);
  }

  /** `again`: the engine's own new start of the session's source, which the session hears of. */
  private start(name: PlayerName, source: EngineSource, play: boolean, again = false): void {
    if (again) this.listener?.restarted?.();
    const engine = this.use(name);
    this.source = source;
    this.wantPlay = play;
    this.shown = false;
    this.askedMs = source.startMs;
    engine.prepare(source, play);
  }

  prepare(source: EngineSource, play: boolean): void {
    const choice = this.choose(source);
    console.info("[engine] chosen", choice);
    this.forSeek = false;
    this.start(choice.engine, source, play);
  }

  activePlayer(): string | null {
    return this.activeName;
  }

  switchForSeek(targetMs: number): boolean {
    const source = this.source;
    if (!source || this.activeName !== "avplay") return false;
    if (this.choose(source, true).engine !== "video") return false;
    console.info("[engine] chosen", { engine: "video", reason: "seek refused" });
    this.start("video", { ...source, startMs: Math.max(0, targetMs) }, this.wantPlay, true);
    this.forSeek = true;
    return true;
  }

  holdPicture(): void {
    this.active?.holdPicture?.();
  }

  play(): void {
    this.wantPlay = true;
    this.active?.play();
  }

  pause(): void {
    this.wantPlay = false;
    this.active?.pause();
  }

  seekTo(ms: number): void {
    this.askedMs = ms;
    this.active?.seekTo(ms);
  }

  stop(): void {
    this.wantPlay = false;
    this.source = null;
    this.forSeek = false;
    this.active?.stop();
  }

  release(): void {
    this.source = null;
    this.letGo();
    this.listener = null;
  }

  positionMs(): number {
    return this.active?.positionMs() ?? 0;
  }

  durationMs(): number {
    return this.active?.durationMs() ?? 0;
  }

  bufferedMs(): number {
    return this.active?.bufferedMs() ?? 0;
  }

  setArea(area: Area): void {
    this.area = area;
    this.areaSet = true;
    this.active?.setArea?.(area);
  }

  bandwidthKbps(): number | null {
    return this.active?.bandwidthKbps?.() ?? null;
  }

  seekPending(): boolean {
    return this.active?.seekPending?.() === true;
  }

  status(): EngineStatus {
    return this.active?.status() ?? "idle";
  }

  playing(): boolean {
    return this.active?.playing() ?? false;
  }

  audioTracks(): EngineTrack[] {
    return this.active?.audioTracks() ?? [];
  }

  /**
   * Not on <video>, which can't be told which track to play here (another
   * track than the file's own goes to AVPlay): the session starts again on
   * AVPlay with the track asked for.
   */
  selectAudio(ordinal: number, language: string | null): boolean {
    if (this.activeName === "video") return false;
    return this.active?.selectAudio(ordinal, language) ?? false;
  }

  /**
   * None on <video>, as on AVPlay: the frames <video> says it dropped would
   * have the session convert a file that plays as it is.
   */
  frameCounts(): { dropped: number; total: number } | null {
    if (this.activeName === "video") return null;
    return this.active?.frameCounts() ?? null;
  }

  stats(): EngineStats | null {
    return this.active?.stats() ?? null;
  }
}

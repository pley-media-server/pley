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

  constructor(host: HTMLElement, uhd: boolean) {
    this.player = webapis!.avplay;
    this.uhd = uhd;
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
      }
    }
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
    this.lastTime = source.startMs;
    this.set("buffering");
    const p = this.player;
    try {
      p.open(source.url);
      p.setListener({
        onbufferingstart: () => mine === this.generation && this.set("buffering"),
        onbufferingcomplete: () => mine === this.generation && this.set(this.state === "ended" ? "ended" : "ready"),
        oncurrentplaytime: (ms) => {
          if (mine !== this.generation) return;
          this.lastTime = ms;
          if (!this.shown && this.wantPlay) {
            this.shown = true;
            this.set("ready");
            this.listener?.firstFrame();
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
        try {
          // AVPlay draws a file's own subtitles if asked; the app draws them.
          p.setSilentSubtitle(true);
        } catch {
          // Not on this set.
        }
        const start = () => {
          if (mine !== this.generation) return;
          this.set("ready");
          if (this.wantPlay) this.run(() => p.play());
          else if (!this.shown) {
            this.shown = true;
            this.listener?.firstFrame();
          }
          this.listener?.tracks();
        };
        // AVPlay can't seek to 0; the start of a file needs no seek.
        if (source.startMs >= 1000) {
          this.seeking = true;
          p.seekTo(
            Math.round(source.startMs),
            () => {
              this.seekDone();
              start();
            },
            () => {
              this.seekDone();
              start();
            },
          );
        } else start();
      },
      (e) => mine === this.generation && this.listener?.error(avplayError(String((e as { name?: string })?.name ?? e), source, this.httpStatus)),
    );
  }

  play(): void {
    this.wantPlay = true;
    this.run(() => {
      const s = this.player.getState();
      if (s === "READY" || s === "PAUSED") this.player.play();
    });
  }

  pause(): void {
    this.wantPlay = false;
    this.run(() => {
      if (this.player.getState() === "PLAYING") this.player.pause();
    });
  }

  seekTo(ms: number): void {
    // Not to 0: AVPlay refuses it.
    const target = Math.max(100, Math.round(ms));
    this.lastTime = target;
    this.run(() => {
      this.seeking = true;
      this.set("buffering");
      // A seek AVPlay refuses leaves it playing where it was: neither way does it stay waiting.
      const done = () => {
        this.seekDone();
        if (this.state === "buffering") this.set("ready");
      };
      this.player.seekTo(target, done, done);
    });
  }

  private closePlayer(): void {
    this.seeking = false;
    this.queued = [];
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
      if (t > 0) this.lastTime = t;
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

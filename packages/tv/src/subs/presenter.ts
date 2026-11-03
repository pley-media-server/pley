import { ApiError, type Api } from "../api/client";
import { Cancel } from "../net/transport";
import { pictureUrlAt } from "../player/plan";
import { readPictureTrack } from "pley-picture-subtitles/mks";
import { PictureSubtitles } from "pley-picture-subtitles/picture-track";
import type { PictureFrame } from "pley-picture-subtitles/pictures";
import { CueIndex, mergeCues, parseVtt, type Cue } from "./vtt";

/** What shows now: text cues, or a picture frame, or nothing. */
export interface ShownSubtitles {
  cues: Cue[];
  picture: PictureFrame | null;
}

export const NOTHING_SHOWN: ShownSubtitles = { cues: [], picture: null };

/** A partial answer (a track still being read out of its file) is asked for again this often. */
export const PARTIAL_RETRY_MS = 3_000;
/** The header that marks a partial answer. */
const COMPLETE_HEADER = "X-Pley-Complete";

export interface PresenterHooks {
  /** The player's position, in ms of the file. */
  position(): number;
  /** Positive is later. */
  offsetMs(): number;
  onShown(shown: ShownSubtitles): void;
  /** The track couldn't be loaded (told once per choice). */
  onFailed(): void;
  /** The server can't make picture subtitles (404 from an old server, 503 from an old transcoder). */
  onPicturesUnavailable(): void;
}

type Track = { kind: "text"; url: string } | { kind: "pictures"; url: string };

/**
 * The subtitles this TV draws: the server's WebVTT, merged as partial
 * answers come in, or the server's .mks of a picture track, decoded when
 * shown. Updates follow the cues: at their next change, between 20 and
 * 250 ms away (500 ms when nothing changes any more).
 */
export class SubtitlePresenter {
  private readonly api: Api;
  private readonly hooks: PresenterHooks;
  private track: Track | null = null;
  private cues: Cue[] | null = null;
  private index: CueIndex | null = null;
  private pictures: PictureSubtitles | null = null;
  private cancel: Cancel | null = null;
  private retry: ReturnType<typeof setTimeout> | null = null;
  private tick: ReturnType<typeof setTimeout> | null = null;
  private shownKey = "";
  private active = true;

  constructor(api: Api, hooks: PresenterHooks) {
    this.api = api;
    this.hooks = hooks;
  }

  get showing(): Track | null {
    return this.track;
  }

  /** Shows the server's WebVTT at `url`; the same track again keeps what was fetched. */
  showText(url: string): void {
    if (this.track?.kind === "text" && this.track.url === url) {
      this.refresh();
      return;
    }
    this.reset({ kind: "text", url });
    this.fetchText(url);
  }

  /** Shows the server's .mks of a picture track at `url`, asked for from where the player is. */
  showPictures(url: string): void {
    if (this.track?.kind === "pictures" && this.track.url === url) {
      this.refresh();
      return;
    }
    this.reset({ kind: "pictures", url });
    this.fetchPictures(url);
  }

  /**
   * The position jumped to `atMs` (the player may not say so yet): a picture track still being read is asked for
   * again at once, from there.
   */
  sought(atMs: number): void {
    const track = this.track;
    if (track?.kind === "pictures" && (this.retry !== null || this.cancel !== null)) {
      this.cancel?.cancel();
      if (this.retry !== null) clearTimeout(this.retry);
      this.retry = null;
      this.fetchPictures(track.url, atMs);
    }
    this.refresh();
  }

  hide(): void {
    this.reset(null);
    this.publish(NOTHING_SHOWN);
  }

  /** The position jumped, or the offset changed: look again now. */
  refresh(): void {
    if (this.tick !== null) clearTimeout(this.tick);
    this.tick = null;
    this.update();
  }

  /** Stops the clock while the player is hidden or paused for good; `resume` starts it again. */
  pause(): void {
    this.active = false;
    if (this.tick !== null) clearTimeout(this.tick);
    this.tick = null;
  }

  resume(): void {
    this.active = true;
    this.refresh();
  }

  dispose(): void {
    this.hide();
    this.active = false;
  }

  private reset(track: Track | null): void {
    this.cancel?.cancel();
    this.cancel = null;
    if (this.retry !== null) clearTimeout(this.retry);
    if (this.tick !== null) clearTimeout(this.tick);
    this.retry = null;
    this.tick = null;
    this.track = track;
    this.cues = null;
    this.index = null;
    this.pictures = null;
    this.shownKey = "";
  }

  private fetchText(url: string): void {
    const cancel = new Cancel();
    this.cancel = cancel;
    this.api
      .send(url, { responseType: "text", headers: { Accept: "text/vtt" }, cancel, timeoutMs: 120_000 })
      .then((res) => {
        if (cancel.cancelled || this.track?.url !== url) return;
        const merged = mergeCues(this.cues, parseVtt(String(res.body ?? "")));
        if (merged !== this.cues) {
          this.cues = merged;
          this.index = new CueIndex(merged);
          this.refresh();
        }
        if ((res.header(COMPLETE_HEADER) ?? "").trim() === "0") {
          this.retry = setTimeout(() => this.fetchText(url), PARTIAL_RETRY_MS);
        }
      })
      .catch(() => {
        if (cancel.cancelled || this.track?.url !== url) return;
        this.hooks.onFailed();
      });
  }

  /**
   * The track at `url`, asked for from where the player is (`atMs`, else now): while the server reads the track, it
   * answers with the pictures around there.
   */
  private fetchPictures(url: string, atMs = this.hooks.position()): void {
    const cancel = new Cancel();
    this.cancel = cancel;
    // An older server reads the whole file to make it the first time: minutes for a big one on a network share.
    this.api
      .send(pictureUrlAt(url, atMs), { responseType: "arraybuffer", headers: { Accept: "video/x-matroska" }, cancel, timeoutMs: 15 * 60_000 })
      .then((res) => {
        if (cancel.cancelled || this.track?.url !== url) return;
        this.cancel = null;
        const subs = PictureSubtitles.of(readPictureTrack(res.body as ArrayBuffer));
        if (!subs) {
          this.hooks.onPicturesUnavailable();
          return;
        }
        this.pictures = subs;
        this.refresh();
        if ((res.header(COMPLETE_HEADER) ?? "").trim() === "0") {
          this.retry = setTimeout(() => {
            this.retry = null;
            this.fetchPictures(url);
          }, PARTIAL_RETRY_MS);
        }
      })
      .catch((e: unknown) => {
        if (cancel.cancelled || this.track?.url !== url) return;
        this.cancel = null;
        if (e instanceof ApiError && (e.status === 404 || e.status === 503)) this.hooks.onPicturesUnavailable();
        else this.hooks.onFailed();
      });
  }

  private update(): void {
    if (!this.track || !this.active) return;
    const at = this.hooks.position() - this.hooks.offsetMs();
    let shown: ShownSubtitles = NOTHING_SHOWN;
    let key = "";
    let next: number | null = null;
    if (this.index) {
      const cues = this.index.at(at);
      shown = { cues, picture: null };
      key = cues.map((c) => `${c.startMs}-${c.endMs}`).join(",");
      next = this.index.nextChange(at);
    } else if (this.pictures) {
      const frame = this.pictures.frameAt(at);
      shown = { cues: [], picture: frame?.frame ?? null };
      key = frame ? `p${frame.index}` : "";
      next = this.pictures.nextChange(at);
    }
    if (key !== this.shownKey) {
      this.shownKey = key;
      this.publish(shown);
    }
    const wait = next === null ? 500 : Math.max(20, Math.min(250, next - at));
    this.tick = setTimeout(() => {
      this.tick = null;
      this.update();
    }, wait);
  }

  private publish(shown: ShownSubtitles): void {
    this.hooks.onShown(shown);
  }
}

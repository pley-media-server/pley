/**
 * Picture subtitles (PGS, VobSub, DVB) drawn by the player rather than
 * burned into the video by the server: the server's `.mks` of the track
 * (docs/API.md), decoded with the TVs' decoders (packages/picture-subtitles)
 * and drawn on a canvas over the video, at device pixels, in step with the
 * frame on screen. Direct play, repackaged streams, Dolby Vision and HDR
 * all stay as they are.
 */
import { matroskaEnd, readPictureTrack } from "pley-picture-subtitles/mks";
import { PictureSubtitles } from "pley-picture-subtitles/picture-track";
import { CACHE_BYTES, frameBytes, type PictureFrame } from "pley-picture-subtitles/pictures";

export type { PictureSubtitles };

/** Where the server serves picture track `index` of file `fileId`. */
export function picturePath(fileId: number, index: number): string {
  return `/api/files/${fileId}/subtitles/${index}.mks`;
}

/**
 * `url` asked for from where the player is (`atMs` of the file's time): a
 * track the server hasn't copied out of its file yet comes from there first.
 * At the start too: a copy under way from elsewhere answers with the
 * pictures there to a player that doesn't say.
 */
export function pictureUrlAt(url: string, atMs: number): string {
  const at = Math.max(0, Math.round(atMs)) || 0;
  return `${url}${url.includes("?") ? "&" : "?"}at=${at}`;
}

/** A partial answer (a track still being read out of its file) is asked for again this often, as on the TVs. */
export const PICTURES_AGAIN_MS = 3_000;
const COMPLETE_HEADER = "X-Pley-Complete";

/**
 * How far ahead of the frame's time a cue is looked up (ms): a frame's
 * time, a float of seconds, can land a hair before a cue that starts on
 * that very frame.
 */
export const LOOKAHEAD_MS = 1;

/**
 * The time on the track (the file's, in ms) that shows at `mediaSeconds`
 * of the player: the session's offset added (`hlsShiftMs`; 0: the player's
 * time is the file's), and the offset the person set taken away (positive:
 * subtitles later).
 */
export function trackMs(mediaSeconds: number, sessionOffsetMs: number, personOffsetSeconds: number): number {
  return mediaSeconds * 1000 + sessionOffsetMs - personOffsetSeconds * 1000 + LOOKAHEAD_MS;
}

/**
 * How far hls.js's time runs ahead of the file's in a session (ms), from
 * the first segment it parsed: where its first picture lands on the
 * element's timeline, against that segment's place in the playlist (the
 * file's time). hls.js starts its timeline at the segment's earliest
 * decoding time, so a stream with B-frames shows its first picture two or
 * three frames after its place in the playlist; and a conversion started
 * part-way into a file can be most of a second out. `undefined`: nothing
 * to go by yet (no video); `null`: a measure past a second, not trusted.
 */
export function hlsLeadMs(frag: {
  playlistOffset?: number;
  elementaryStreams: { video?: { startPTS: number } | null };
}): number | null | undefined {
  const video = frag.elementaryStreams.video;
  if (!video || typeof frag.playlistOffset !== "number" || !Number.isFinite(video.startPTS)) return undefined;
  const ahead = video.startPTS - frag.playlistOffset;
  return Math.abs(ahead) < 1 ? ahead * 1000 : null;
}

/**
 * What an HLS stream's media time takes (ms) to be the file's: hls.js's
 * time is the stream's timestamps less its `initPTS` (`initPtsMs`), and the
 * server's timestamps are the file's plus `pts_offset_ms`, so with both
 * known it is exact. Otherwise the lead measured from the first segment
 * (`hlsLeadMs`) is taken off; with neither, nothing.
 */
export function hlsShiftMs(ptsOffsetMs: number | null | undefined, initPtsMs: number | null, leadMs: number | null): number {
  if (typeof ptsOffsetMs === "number" && initPtsMs !== null) return initPtsMs - ptsOffsetMs;
  return leadMs === null ? 0 : -leadMs;
}

export interface Box {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** A `width`×`height` picture fitted inside `view`, centred (`object-fit: contain`). */
export function contain(view: { width: number; height: number }, width: number, height: number): Box {
  if (!(view.width > 0 && view.height > 0 && width > 0 && height > 0)) return { x: 0, y: 0, width: view.width, height: view.height };
  const scale = Math.min(view.width / width, view.height / height);
  const w = width * scale;
  const h = height * scale;
  return { x: (view.width - w) / 2, y: (view.height - h) / 2, width: w, height: h };
}

/**
 * Where a subtitle plane goes in the video element (`view`, CSS px): over
 * the picture (the video's content box, letterboxed or pillarboxed as the
 * element shows it). A DVD's or SD DVB's plane (720×576, 720×480) stretches
 * over it, as the video it came with is stored anamorphic. An HD plane of
 * another shape than the video (a Blu-ray's 1920×1080 over a film cropped
 * to 1920×800) keeps its own shape, fitted in the element: its lines go
 * where the disc puts them, in the black bars when there are any, and
 * never off the screen.
 */
export function planeBox(view: { width: number; height: number }, video: { width: number; height: number }, plane: { width: number; height: number }): Box {
  const picture = contain(view, video.width || plane.width, video.height || plane.height);
  if (!video.width || !video.height || plane.height <= 576) return picture;
  const shapes = plane.width / plane.height / (video.width / video.height);
  return Math.abs(shapes - 1) < 0.02 ? picture : contain(view, plane.width, plane.height);
}

// ---------------------------------------------------------------------------
// Loading

export type PictureLoad =
  /** `bytes`: the file as far as it has come, to ask for the rest of while it isn't `complete`. */
  | { kind: "track"; subs: PictureSubtitles; complete: boolean; bytes: Uint8Array }
  /** The server doesn't make these files (404 from an old server, 503 from an old transcoder), or none this player decodes. */
  | { kind: "unavailable" }
  | { kind: "failed" }
  /** Too little of a file still being made to read yet, or nothing new. */
  | { kind: "early"; bytes: Uint8Array | null }
  /** What came before may not be the start of the file now (whole now, or another file): to ask for all of it. */
  | { kind: "anew" };

/**
 * One request for the track at `url`. A partial answer (`X-Pley-Complete:
 * 0`) comes with what the server has read out of the file so far; with
 * `have`, the bytes of the last one, only the rest is asked for (`Range`),
 * and a server that answers with the whole file again is fine too. Once
 * the track is whole, what came before may not be its start: the pictures
 * around where the player is come first, and a file being made has its
 * sizes written as it is finished. Nor is it when a file that came whole
 * (its size written: the pictures around the player, a file of their own)
 * has more, or a length of its own: that is another file (the pictures
 * around where the player is now, maybe fewer).
 */
export async function fetchPictures(url: string, signal: AbortSignal, have: Uint8Array | null = null, again = false): Promise<PictureLoad> {
  const headers: Record<string, string> = { Accept: "video/x-matroska" };
  if (have?.length) headers.Range = `bytes=${have.length}-`;
  let res: Response;
  try {
    res = await fetch(url, {
      credentials: "same-origin",
      signal,
      headers,
      // More of a file still being made: never an earlier answer kept.
      cache: again ? "no-store" : "default",
    });
  } catch (err) {
    if (signal.aborted) throw err;
    return { kind: "failed" };
  }
  if (res.status === 404 || res.status === 503) return { kind: "unavailable" };
  const said = (res.headers.get(COMPLETE_HEADER) ?? "").trim();
  const complete = said !== "0";
  // Nothing past what came before, of a track that is whole now (in so many words).
  if (have && (res.ok ? complete : res.status === 416 && said === "1")) {
    void res.body?.cancel();
    return { kind: "anew" };
  }
  // A file that came whole (its size written): the pictures around where
  // the player was, a file of their own.
  const end = have && matroskaEnd(have);
  const cameWhole = have !== null && end !== null && end <= have.length;
  // Nothing past what came before: the same file, or another one shorter
  // than it, which the length the server gives tells apart (one just as
  // long is taken for the same; the next place's comes soon after). With
  // no length (a proxy that drops it), a file that came whole may be
  // another place's, and one still being written only grows.
  if (res.status === 416 && have) {
    const total = /\/(\d+)$/.exec(res.headers.get("Content-Range") ?? "");
    const same = total ? Number(total[1]) === have.length : !cameWhole;
    return same ? { kind: "early", bytes: have } : { kind: "anew" };
  }
  if (!res.ok) return { kind: "failed" };
  // More past a file that came whole: another file, not the rest of it.
  if (res.status === 206 && cameWhole) {
    void res.body?.cancel();
    return { kind: "anew" };
  }
  let bytes: Uint8Array;
  try {
    const body = new Uint8Array(await res.arrayBuffer());
    const from = /^bytes (\d+)-/.exec(res.headers.get("Content-Range") ?? "");
    if (res.status === 206 && have && from && Number(from[1]) === have.length) {
      bytes = new Uint8Array(have.length + body.length);
      bytes.set(have, 0);
      bytes.set(body, have.length);
    } else if (res.status === 206) {
      // Some other part: not what was asked for.
      return complete ? { kind: "failed" } : { kind: "early", bytes: null };
    } else {
      bytes = body;
    }
  } catch (err) {
    if (signal.aborted) throw err;
    return { kind: "failed" };
  }
  let track: ReturnType<typeof readPictureTrack>;
  try {
    track = readPictureTrack(bytes.buffer as ArrayBuffer);
  } catch {
    return complete ? { kind: "failed" } : { kind: "early", bytes };
  }
  const subs = PictureSubtitles.of(track);
  if (!subs) return complete ? { kind: "unavailable" } : { kind: "early", bytes };
  // No picture yet: maybe a file the server stands in with while the copy
  // starts, not the start of the one that comes next. Asked for whole again.
  if (!complete && track.samples.length === 0) return { kind: "early", bytes: null };
  return { kind: "track", subs, complete, bytes };
}

/** The tracks this page has had whole: asked for again without where the player is, the browser's cache of them serves. */
const wholeTracks = new Set<string>();

export interface PictureHooks {
  /** The track, or more of it: what shows from now on. */
  onTrack(subs: PictureSubtitles): void;
  /** The server can't make it: the player has it burned in instead (told once). */
  onUnavailable(): void;
  /** It couldn't be loaded (told once). */
  onFailed(): void;
}

/** A picture track loading (`loadPictures`). */
export interface PictureLoader {
  /** The player jumped: a track that isn't whole yet is asked for again at once, from there. */
  sought(): void;
  stop(): void;
}

/**
 * Loads the track at `url`, asking again every `PICTURES_AGAIN_MS` while
 * the answers are partial, as the TVs do (packages/tv/src/subs/presenter.ts),
 * but only for what it doesn't have yet. Once the server says it has it all,
 * all of it is asked for (`fetchPictures`). Each ask says where the player
 * is then (`position`, `pictureUrlAt`), as the server answers with the
 * pictures around there while it reads the rest.
 */
export function loadPictures(url: string, hooks: PictureHooks, position: () => number = () => 0): PictureLoader {
  // The ask under way, dropped by the next one (a seek's), as the TVs drop
  // theirs: an answer for where the player was can wait seconds for the
  // read there, holding one of the few connections a browser opens to the
  // server, which the video's segments need.
  let asking: AbortController | null = null;
  let stopped = false;
  let timer: number | undefined;
  let whole = false;
  const load = (have: Uint8Array | null, again: boolean) => {
    asking?.abort();
    const abort = new AbortController();
    asking = abort;
    const asked = wholeTracks.has(url) ? url : pictureUrlAt(url, position());
    fetchPictures(asked, abort.signal, have, again).then(
      (got) => {
        // Stopped, or asked for again from elsewhere since.
        if (abort.signal.aborted) return;
        if (got.kind === "unavailable") hooks.onUnavailable();
        else if (got.kind === "failed") hooks.onFailed();
        else if (got.kind === "anew") load(null, true);
        else {
          if (got.kind === "track") hooks.onTrack(got.subs);
          if (got.kind === "track" && got.complete) {
            whole = true;
            wholeTracks.add(url);
          }
          if (got.kind === "early" || !got.complete) timer = window.setTimeout(() => load(got.bytes, true), PICTURES_AGAIN_MS);
        }
      },
      () => undefined,
    );
  };
  load(null, false);
  return {
    sought: () => {
      if (whole || stopped) return;
      window.clearTimeout(timer);
      load(null, true);
    },
    stop: () => {
      stopped = true;
      asking?.abort();
      window.clearTimeout(timer);
    },
  };
}

// ---------------------------------------------------------------------------
// Drawing

type Image = ImageBitmap | HTMLCanvasElement;

interface Decoded {
  frame: PictureFrame;
  images: Image[];
}

/** Frames kept decoded: the one showing and those coming up. */
const KEPT = 8;

/** The bytes an entry holds: its frame's pictures, and its images as many again. */
const entryBytes = (entry: Decoded | null | undefined) => 2 * frameBytes(entry?.frame);
/** Frames decoded ahead, while the browser is idle. */
const AHEAD = 3;

/**
 * Draws a picture track on `canvas`, laid over `video` (the same box):
 * backed at device pixels, sized with the element (a ResizeObserver, and
 * the screen's pixel ratio), redrawn on each frame the video presents
 * (`requestVideoFrameCallback`, with that frame's own time; animation
 * frames where there is none), and on seeks, pauses and changes. The
 * frames coming up are decoded ahead, when the browser is idle.
 */
export class PictureRenderer {
  private readonly video: HTMLVideoElement;
  private readonly canvas: HTMLCanvasElement;
  private readonly g: CanvasRenderingContext2D | null;
  private subs: PictureSubtitles | null = null;
  private shiftMs = 0;
  private personSeconds = 0;
  private hold = false;
  /** The time of the frame last presented (`requestVideoFrameCallback`), until the next jump. */
  private presented: number | null = null;
  /** The sample drawn (-1: nothing), and whether it must be drawn again anyway. */
  private shown = -1;
  private dirty = true;
  private readonly cache = new Map<number, Decoded | null>();
  private cacheBytes = 0;
  /** The most bytes the frames kept hold, with their images. */
  private readonly keptBytes: number;
  private frameHandle: number | null = null;
  private animation: number | null = null;
  private idle: number | null = null;
  private dpr = 1;
  private readonly off: (() => void)[] = [];
  private stopDpr: (() => void) | null = null;
  private gone = false;

  constructor(video: HTMLVideoElement, canvas: HTMLCanvasElement, keptBytes = CACHE_BYTES) {
    this.video = video;
    this.canvas = canvas;
    this.keptBytes = keptBytes;
    this.g = canvas.getContext("2d");
    const on = (target: EventTarget, type: string, fn: () => void) => {
      target.addEventListener(type, fn);
      this.off.push(() => target.removeEventListener(type, fn));
    };
    // What the clock alone doesn't say: a jump, a stop, a new source.
    // A frame from before a jump or a new source says nothing of the next.
    for (const type of ["seeking", "emptied"]) on(video, type, () => (this.presented = null));
    for (const type of ["seeked", "seeking", "pause", "ratechange", "emptied", "timeupdate"]) on(video, type, () => this.fromElement());
    for (const type of ["play", "playing", "loadeddata"]) {
      on(video, type, () => {
        this.follow();
        this.redraw();
      });
    }
    if (typeof ResizeObserver !== "undefined") {
      const observer = new ResizeObserver(() => this.layout());
      observer.observe(video);
      this.off.push(() => observer.disconnect());
    }
    // A window moved to a screen of another pixel ratio, or zoomed.
    on(window, "resize", () => this.layout());
    this.watchDpr();
    this.layout();
    this.follow();
  }

  /** The track to draw (`null`: none yet); more of the same track keeps nothing decoded of the last. */
  setTrack(subs: PictureSubtitles | null): void {
    if (subs === this.subs) return;
    this.subs = subs;
    this.forget();
    this.redraw();
  }

  /** The session's offset in ms (`hlsShiftMs`), and the person's in seconds (positive: later). */
  setOffsets(sessionMs: number, personSeconds: number): void {
    if (sessionMs === this.shiftMs && personSeconds === this.personSeconds) return;
    this.shiftMs = sessionMs;
    this.personSeconds = personSeconds;
    this.redraw();
  }

  /** While the last session's picture stands in for the next (`freezeFrame`), its subtitles stay too. */
  setHold(hold: boolean): void {
    if (hold === this.hold) return;
    this.hold = hold;
    this.redraw();
  }

  destroy(): void {
    this.gone = true;
    for (const f of this.off) f();
    this.off.length = 0;
    this.stopDpr?.();
    if (this.frameHandle !== null) this.video.cancelVideoFrameCallback(this.frameHandle);
    if (this.animation !== null) cancelAnimationFrame(this.animation);
    if (this.idle !== null) window.clearTimeout(this.idle);
    this.forget();
    this.g?.clearRect(0, 0, this.canvas.width, this.canvas.height);
  }

  /** Draws again from the element's time, now. */
  private redraw(): void {
    this.dirty = true;
    this.fromElement();
  }

  /**
   * The element's time: what shows while paused or after a jump. Playing,
   * frames say their own time (`follow`), which `timeupdate` would only
   * blur (the element's time runs ahead of the frame on screen). Paused,
   * the frame on screen's own time when it said it, as a seek's time can
   * fall anywhere in its frame.
   */
  private fromElement(): void {
    const v = this.video;
    if (!v.paused && !v.seeking && this.frameHandle !== null && !this.dirty) return;
    const frame = this.presented;
    this.showAt(frame !== null && !v.seeking && Math.abs(v.currentTime - frame) < 0.1 ? frame : v.currentTime);
  }

  /** Follows the frames presented while it plays. */
  private follow(): void {
    if (this.gone) return;
    const v = this.video;
    // Firefox before 132 has none.
    if (typeof v.requestVideoFrameCallback === "function") {
      // Asked afresh: a new source may not answer what was asked of the last.
      if (this.frameHandle !== null) v.cancelVideoFrameCallback(this.frameHandle);
      const tick = (_now: number, meta: VideoFrameCallbackMetadata) => {
        this.frameHandle = null;
        if (this.gone) return;
        this.presented = meta.mediaTime;
        this.showAt(meta.mediaTime);
        this.frameHandle = v.requestVideoFrameCallback(tick);
      };
      this.frameHandle = v.requestVideoFrameCallback(tick);
      return;
    }
    if (this.animation !== null) return;
    const tick = () => {
      this.animation = null;
      if (this.gone) return;
      this.showAt(v.currentTime);
      if (!v.paused && !v.ended) this.animation = requestAnimationFrame(tick);
    };
    this.animation = requestAnimationFrame(tick);
  }

  private watchDpr(): void {
    this.stopDpr?.();
    this.stopDpr = null;
    const dpr = window.devicePixelRatio || 1;
    if (typeof window.matchMedia !== "function") return;
    const query = window.matchMedia(`(resolution: ${dpr}dppx)`);
    const changed = () => {
      this.watchDpr();
      this.layout();
    };
    query.addEventListener?.("change", changed);
    this.stopDpr = () => query.removeEventListener?.("change", changed);
  }

  /** The backing store at the element's size in device pixels. */
  private layout(): void {
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(this.video.clientWidth * dpr));
    const h = Math.max(1, Math.round(this.video.clientHeight * dpr));
    this.dpr = dpr;
    if (this.canvas.width !== w || this.canvas.height !== h) {
      this.canvas.width = w;
      this.canvas.height = h;
    }
    this.redraw();
  }

  private showAt(seconds: number): void {
    const subs = this.subs;
    const g = this.g;
    if (!g) return;
    // The pixel ratio changed without a word (not every browser says so): backed afresh first.
    if ((window.devicePixelRatio || 1) !== this.dpr) {
      this.layout();
      return;
    }
    const v = this.video;
    // A reset element (a new session on its way) shows nothing: neither do its subtitles, unless held.
    const playable = v.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA;
    if (!playable && this.hold) return;
    const at = trackMs(seconds, this.shiftMs, this.personSeconds);
    const i = subs && playable ? subs.sampleAt(at) : -1;
    if (i === this.shown && !this.dirty) return;
    this.shown = i;
    this.dirty = false;
    g.clearRect(0, 0, this.canvas.width, this.canvas.height);
    if (!subs) return;
    const entry = i >= 0 ? this.decode(i, at) : null;
    if (entry) this.draw(g, entry);
    this.decodeAhead(at);
  }

  private draw(g: CanvasRenderingContext2D, { frame, images }: Decoded): void {
    const v = this.video;
    const view = { width: this.canvas.width / this.dpr, height: this.canvas.height / this.dpr };
    const box = planeBox(view, { width: v.videoWidth, height: v.videoHeight }, { width: frame.planeWidth, height: frame.planeHeight });
    const sx = (box.width / frame.planeWidth) * this.dpr;
    const sy = (box.height / frame.planeHeight) * this.dpr;
    const ox = box.x * this.dpr;
    const oy = box.y * this.dpr;
    g.imageSmoothingEnabled = true;
    g.imageSmoothingQuality = "high";
    frame.bitmaps.forEach((b, k) => {
      // Whole device pixels at both edges: no edge blurred across two.
      const x0 = Math.round(ox + b.x * sx);
      const y0 = Math.round(oy + b.y * sy);
      const x1 = Math.round(ox + (b.x + b.width) * sx);
      const y1 = Math.round(oy + (b.y + b.height) * sy);
      const image = images[k];
      if (image && x1 > x0 && y1 > y0) g.drawImage(image, x0, y0, x1 - x0, y1 - y0);
    });
  }

  /** Sample `i` (showing at `at`) decoded, from what was decoded ahead when it was. */
  private decode(i: number, at: number): Decoded | null {
    if (this.cache.has(i)) return this.cache.get(i) ?? null;
    const shown = this.subs?.frameAt(at);
    const entry = shown && shown.index === i ? decoded(shown.frame) : null;
    this.keep(i, entry);
    return entry;
  }

  private keep(i: number, entry: Decoded | null): void {
    this.cache.set(i, entry);
    this.cacheBytes += entryBytes(entry);
    // The oldest go first, never the frame showing nor the one just decoded.
    for (const [kept, old] of this.cache) {
      if (this.cache.size <= KEPT && this.cacheBytes <= this.keptBytes) break;
      if (kept === i || kept === this.shown) continue;
      this.cache.delete(kept);
      this.cacheBytes -= entryBytes(old);
      release(old);
    }
  }

  /** The next few frames after `at`, decoded while the browser is idle. */
  private decodeAhead(at: number): void {
    if (this.idle !== null) return;
    this.idle = window.setTimeout(() => {
      this.idle = null;
      const subs = this.subs;
      if (!subs || this.gone) return;
      let t: number | null = at;
      for (let n = 0; n < AHEAD && t !== null; n++) {
        t = subs.nextChange(t);
        if (t === null) break;
        const i = subs.sampleAt(t);
        if (i >= 0 && !this.cache.has(i)) {
          const shown = subs.frameAt(t);
          this.keep(i, shown ? decoded(shown.frame) : null);
        }
      }
    }, 0);
  }

  private forget(): void {
    for (const entry of this.cache.values()) release(entry);
    this.cache.clear();
    this.cacheBytes = 0;
    this.shown = -1;
  }
}

/** A frame's bitmaps as images: canvases at once, ImageBitmaps (kept on the GPU) once made. */
function decoded(frame: PictureFrame): Decoded {
  const images: Image[] = frame.bitmaps.map((b) => {
    const c = document.createElement("canvas");
    c.width = b.width;
    c.height = b.height;
    const g = c.getContext("2d");
    if (g) {
      const data = g.createImageData(b.width, b.height);
      data.data.set(b.rgba);
      g.putImageData(data, 0, 0);
    }
    return c;
  });
  const entry: Decoded = { frame, images };
  if (typeof createImageBitmap === "function") {
    images.forEach((image, k) => {
      createImageBitmap(image).then(
        (bitmap) => {
          if (entry.images.length) entry.images[k] = bitmap;
          else bitmap.close();
        },
        () => undefined,
      );
    });
  }
  return entry;
}

function release(entry: Decoded | null | undefined): void {
  if (!entry) return;
  for (const image of entry.images) if ("close" in image) image.close();
  entry.images.length = 0;
}

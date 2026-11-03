/**
 * What picture subtitles become once decoded: bitmaps placed on the
 * subtitle plane (1920×1080 for a Blu-ray's PGS, 720×576 for a DVD's
 * VobSub), drawn over the whole screen at the same fractions.
 */

export interface SubtitleBitmap {
  x: number;
  y: number;
  width: number;
  height: number;
  /** RGBA, `width × height × 4` bytes, not premultiplied. */
  rgba: Uint8ClampedArray;
}

export interface PictureFrame {
  /** The plane the bitmaps are placed on. */
  planeWidth: number;
  planeHeight: number;
  /** Nothing to show: the screen clears. */
  bitmaps: SubtitleBitmap[];
}

/** A decoder for one track's samples, which may need earlier samples to decode a later one. */
export interface PictureDecoder {
  /** The frame sample `index` shows; `null` when it can't be decoded. */
  decode(index: number): PictureFrame | null;
  /**
   * How long sample `index` shows when the sample itself says (VobSub's
   * stop command), else `null`: until the next sample.
   */
  ownDurationMs(index: number): number | null;
}

/**
 * A picture's side at most, in pixels (a PGS object, a DVB region):
 * subtitles are drawn for 4K screens at most, and a file's sizes are
 * outside input.
 */
export const MAX_SIDE = 4096;

/**
 * The most pixels a frame decodes, all its pictures together: a 4K
 * screen's worth. A file can place a 4096² picture many times over, and
 * each pixel costs 4 bytes once coloured.
 */
export const MAX_PIXELS = 3840 * 2160;

/** Decoded frames kept per track, and the most bytes they hold together. */
export const CACHE_FRAMES = 25;
export const CACHE_BYTES = 128 << 20;

/** The bytes of a frame's pictures. */
export function frameBytes(frame: PictureFrame | null | undefined): number {
  let bytes = 0;
  for (const b of frame?.bitmaps ?? []) bytes += b.rgba.byteLength;
  return bytes;
}

/**
 * The frames decoded last, the oldest dropped first: a few around the
 * playhead are enough, as seeking decodes again. A crafted file's frames
 * can each be a 4K screen's worth, so they are kept by bytes too, but the
 * frame just decoded always stays: it is the one showing.
 */
export class FrameCache {
  private readonly frames = new Map<number, PictureFrame | null>();
  private bytes = 0;
  private readonly most: number;
  private readonly mostBytes: number;

  constructor(most = CACHE_FRAMES, mostBytes = CACHE_BYTES) {
    this.most = most;
    this.mostBytes = mostBytes;
  }

  has(index: number): boolean {
    return this.frames.has(index);
  }

  /** The frame of `index`, from `decode` unless kept. */
  get(index: number, decode: (index: number) => PictureFrame | null): PictureFrame | null {
    if (this.frames.has(index)) return this.frames.get(index)!;
    const frame = decode(index);
    this.frames.set(index, frame);
    this.bytes += frameBytes(frame);
    for (const [kept, old] of this.frames) {
      if (this.frames.size === 1 || (this.frames.size <= this.most && this.bytes <= this.mostBytes)) break;
      this.frames.delete(kept);
      this.bytes -= frameBytes(old);
    }
    return frame;
  }
}

/** Limited-range YCbCr to RGB: BT.709 for HD planes, BT.601 for SD (as FFmpeg's decoders). */
export function ycbcrToRgb(y: number, cb: number, cr: number, hd: boolean): [number, number, number] {
  const yy = 1.164383 * (y - 16);
  const u = cb - 128;
  const v = cr - 128;
  let r: number;
  let g: number;
  let b: number;
  if (hd) {
    r = yy + 1.792741 * v;
    g = yy - 0.213249 * u - 0.532909 * v;
    b = yy + 2.112402 * u;
  } else {
    r = yy + 1.596027 * v;
    g = yy - 0.391762 * u - 0.812968 * v;
    b = yy + 2.017232 * u;
  }
  const c = (x: number) => (x < 0 ? 0 : x > 255 ? 255 : Math.round(x));
  return [c(r), c(g), c(b)];
}

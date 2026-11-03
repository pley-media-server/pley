import { FrameCache, MAX_PIXELS, type PictureDecoder, type PictureFrame } from "./pictures";

/**
 * DVD subtitles (VobSub, `S_VOBSUB`): each sample is one subpicture unit
 * (SPU): two interlaced fields of 2-bit run-length pixels, then control
 * sequences that say when it shows and stops, which 4 of the 16 palette
 * colours its pixels take, their alpha, and where it goes. The palette
 * and the plane's size come from the track's private data, the `.idx`
 * header (`size: 720x480`, `palette: 000000, ffffff, …`).
 */

export interface VobSubHeader {
  width: number;
  height: number;
  /** 16 RGB colours. */
  palette: [number, number, number][];
}

export function parseIdx(codecPrivate: Uint8Array | null): VobSubHeader {
  const text = codecPrivate ? latin1(codecPrivate) : "";
  const size = /size:\s*(\d+)\s*x\s*(\d+)/i.exec(text);
  const palette: [number, number, number][] = [];
  const p = /palette:\s*([0-9a-f,\s]+)/i.exec(text);
  if (p) {
    for (const hex of p[1]!.split(",")) {
      const v = parseInt(hex.trim(), 16);
      if (!isNaN(v)) palette.push([(v >> 16) & 0xff, (v >> 8) & 0xff, v & 0xff]);
    }
  }
  // A DVD's default greys when the header has none.
  while (palette.length < 16) palette.push(palette.length % 2 ? [255, 255, 255] : [0, 0, 0]);
  return { width: size ? Number(size[1]) : 720, height: size ? Number(size[2]) : 576, palette };
}

interface Spu {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
  /** Palette entries for pixel values 0-3. */
  colors: [number, number, number, number];
  /** Alpha 0-15 for pixel values 0-3. */
  alpha: [number, number, number, number];
  top: number;
  bottom: number;
  /** From the stop command, in ms; `null` without one. */
  stopMs: number | null;
  startMs: number;
}

/** Reads the control sequences of an SPU; `null` when it is cut short. */
export function parseSpu(data: Uint8Array): Spu | null {
  if (data.length < 4) return null;
  const size = (data[0]! << 8) | data[1]!;
  const ctrl = (data[2]! << 8) | data[3]!;
  const end = Math.min(size || data.length, data.length);
  const spu: Spu = { x1: 0, y1: 0, x2: -1, y2: -1, colors: [0, 1, 2, 3], alpha: [0, 15, 15, 15], top: 0, bottom: 0, stopMs: null, startMs: 0 };
  let seq = ctrl;
  const seen = new Set<number>();
  while (seq + 4 <= end && !seen.has(seq)) {
    seen.add(seq);
    // Delays count in 1024 ticks of a 90 kHz clock.
    const delayMs = Math.round((((data[seq]! << 8) | data[seq + 1]!) * 1024) / 90);
    const next = (data[seq + 2]! << 8) | data[seq + 3]!;
    let at = seq + 4;
    let done = false;
    while (at < end && !done) {
      const cmd = data[at++]!;
      switch (cmd) {
        case 0x00: // forced start
        case 0x01: // start
          spu.startMs = delayMs;
          break;
        case 0x02: // stop
          spu.stopMs = delayMs;
          break;
        case 0x03:
          if (at + 2 > end) return null;
          spu.colors = [data[at + 1]! & 0x0f, data[at + 1]! >> 4, data[at]! & 0x0f, data[at]! >> 4];
          at += 2;
          break;
        case 0x04:
          if (at + 2 > end) return null;
          spu.alpha = [data[at + 1]! & 0x0f, data[at + 1]! >> 4, data[at]! & 0x0f, data[at]! >> 4];
          at += 2;
          break;
        case 0x05:
          if (at + 6 > end) return null;
          spu.x1 = (data[at]! << 4) | (data[at + 1]! >> 4);
          spu.x2 = ((data[at + 1]! & 0x0f) << 8) | data[at + 2]!;
          spu.y1 = (data[at + 3]! << 4) | (data[at + 4]! >> 4);
          spu.y2 = ((data[at + 4]! & 0x0f) << 8) | data[at + 5]!;
          at += 6;
          break;
        case 0x06:
          if (at + 4 > end) return null;
          spu.top = (data[at]! << 8) | data[at + 1]!;
          spu.bottom = (data[at + 2]! << 8) | data[at + 3]!;
          at += 4;
          break;
        case 0xff:
          done = true;
          break;
        default:
          // An unknown command: its length isn't known, so the sequence ends here.
          done = true;
      }
    }
    if (next === seq) break;
    seq = next;
  }
  return spu;
}

/** One field's lines (every other line of the picture) into `out`, starting at line `first`. */
function decodeField(data: Uint8Array, offset: number, end: number, width: number, height: number, first: number, out: Uint8Array): void {
  let nibble = offset * 2;
  const limit = end * 2;
  const read = (): number => {
    if (nibble >= limit) return 0;
    const byte = data[nibble >> 1]!;
    const v = nibble & 1 ? byte & 0x0f : byte >> 4;
    nibble++;
    return v;
  };
  for (let y = first; y < height; y += 2) {
    let x = 0;
    while (x < width) {
      let v = read();
      if (v < 0x4) {
        v = (v << 4) | read();
        if (v < 0x10) {
          v = (v << 4) | read();
          if (v < 0x40) {
            v = (v << 4) | read();
          }
        }
      }
      const color = v & 3;
      let count = v >> 2;
      if (count === 0) count = width - x; // To the end of the line.
      const stop = Math.min(width, x + count);
      const row = y * width;
      for (let px = x; px < stop; px++) out[row + px] = color;
      x = stop;
      if (nibble >= limit) break;
    }
    // Lines start on a byte.
    if (nibble & 1) nibble++;
  }
}

export class VobSubDecoder implements PictureDecoder {
  private readonly samples: Uint8Array[];
  private readonly header: VobSubHeader;
  private readonly spus: (Spu | null | undefined)[];
  private readonly cache = new FrameCache();

  constructor(samples: Uint8Array[], codecPrivate: Uint8Array | null) {
    this.samples = samples;
    this.header = parseIdx(codecPrivate);
    this.spus = new Array<Spu | null | undefined>(samples.length);
  }

  private spu(index: number): Spu | null {
    let s = this.spus[index];
    if (s === undefined) {
      s = parseSpu(this.samples[index]!);
      this.spus[index] = s;
    }
    return s;
  }

  ownDurationMs(index: number): number | null {
    const s = this.spu(index);
    return s && s.stopMs !== null && s.stopMs > s.startMs ? s.stopMs - s.startMs : null;
  }

  decode(index: number): PictureFrame | null {
    return this.cache.get(index, (i) => this.render(i));
  }

  private render(index: number): PictureFrame | null {
    const spu = this.spu(index);
    const plane = { planeWidth: this.header.width, planeHeight: this.header.height };
    if (!spu) return null;
    const width = spu.x2 - spu.x1 + 1;
    const height = spu.y2 - spu.y1 + 1;
    if (width <= 0 || height <= 0) return { ...plane, bitmaps: [] };
    // Its corners allow 4096² pixels: more than a frame's isn't decoded.
    if (width * height > MAX_PIXELS) return null;
    const data = this.samples[index]!;
    const size = (data[0]! << 8) | data[1]!;
    const ctrl = (data[2]! << 8) | data[3]!;
    const end = Math.min(ctrl || size || data.length, data.length);
    const pixels = new Uint8Array(width * height);
    decodeField(data, spu.top, end, width, height, 0, pixels);
    decodeField(data, spu.bottom, end, width, height, 1, pixels);
    const rgba = new Uint8ClampedArray(width * height * 4);
    const lut: number[][] = [0, 1, 2, 3].map((v) => {
      const c = this.header.palette[spu.colors[v]!]!;
      return [c[0], c[1], c[2], spu.alpha[v]! * 17];
    });
    for (let i = 0; i < pixels.length; i++) {
      const c = lut[pixels[i]!]!;
      const o = i * 4;
      rgba[o] = c[0]!;
      rgba[o + 1] = c[1]!;
      rgba[o + 2] = c[2]!;
      rgba[o + 3] = c[3]!;
    }
    return { ...plane, bitmaps: [{ x: spu.x1, y: spu.y1, width, height, rgba }] };
  }
}

function latin1(bytes: Uint8Array): string {
  let s = "";
  for (let i = 0; i < bytes.length; i++) s += String.fromCharCode(bytes[i]!);
  return s;
}

import { FrameCache, MAX_PIXELS, MAX_SIDE, ycbcrToRgb, type PictureDecoder, type PictureFrame, type SubtitleBitmap } from "./pictures";

/**
 * Blu-ray subtitles (HDMV PGS, `S_HDMV/PGS`): each sample is one display
 * set of segments: a presentation composition (PCS) placing objects on
 * the plane, windows (WDS), palettes (PDS) and run-length coded objects
 * (ODS). An epoch start or acquisition point holds everything it needs;
 * other display sets may reuse palettes and objects defined since.
 */

const PDS = 0x14;
const ODS = 0x15;
const PCS = 0x16;
const WDS = 0x17;
const END = 0x80;

interface Composition {
  width: number;
  height: number;
  /** 0x00 normal, 0x40 acquisition point, 0x80 epoch start. */
  state: number;
  paletteId: number;
  objects: { id: number; x: number; y: number; crop: { x: number; y: number; w: number; h: number } | null }[];
}

interface ObjectData {
  width: number;
  height: number;
  rle: Uint8Array;
}

interface DisplaySet {
  pcs: Composition | null;
  palettes: { id: number; entries: { index: number; y: number; cr: number; cb: number; a: number }[] }[];
  objects: { id: number; first: boolean; last: boolean; width: number; height: number; data: Uint8Array }[];
}

export function parseDisplaySet(data: Uint8Array): DisplaySet {
  const set: DisplaySet = { pcs: null, palettes: [], objects: [] };
  let at = 0;
  // Some muxers keep the "PG" header of the .sup format: 2 magic bytes, PTS and DTS.
  const hasHeader = () => data[at] === 0x50 && data[at + 1] === 0x47;
  while (at + 3 <= data.length) {
    if (hasHeader()) at += 10;
    const type = data[at]!;
    const size = (data[at + 1]! << 8) | data[at + 2]!;
    const body = data.subarray(at + 3, Math.min(data.length, at + 3 + size));
    at += 3 + size;
    switch (type) {
      case PCS:
        set.pcs = readPcs(body);
        break;
      case PDS: {
        if (body.length < 2) break;
        const entries = [];
        for (let i = 2; i + 5 <= body.length; i += 5) {
          entries.push({ index: body[i]!, y: body[i + 1]!, cr: body[i + 2]!, cb: body[i + 3]!, a: body[i + 4]! });
        }
        set.palettes.push({ id: body[0]!, entries });
        break;
      }
      case ODS: {
        if (body.length < 4) break;
        const id = (body[0]! << 8) | body[1]!;
        const flags = body[3]!;
        const first = (flags & 0x80) !== 0;
        const last = (flags & 0x40) !== 0;
        if (first) {
          if (body.length < 11) break;
          const width = (body[7]! << 8) | body[8]!;
          const height = (body[9]! << 8) | body[10]!;
          set.objects.push({ id, first, last, width, height, data: body.subarray(11) });
        } else {
          set.objects.push({ id, first, last, width: 0, height: 0, data: body.subarray(4) });
        }
        break;
      }
      case WDS:
      case END:
      default:
        break;
    }
    if (type === END) break;
  }
  return set;
}

function readPcs(b: Uint8Array): Composition | null {
  if (b.length < 11) return null;
  const width = (b[0]! << 8) | b[1]!;
  const height = (b[2]! << 8) | b[3]!;
  const state = b[7]!;
  const paletteId = b[9]!;
  const count = b[10]!;
  const objects: Composition["objects"] = [];
  let at = 11;
  for (let i = 0; i < count && at + 8 <= b.length; i++) {
    const id = (b[at]! << 8) | b[at + 1]!;
    // 0x80 says a crop follows; 0x40 is forced.
    const cropped = (b[at + 3]! & 0x80) !== 0;
    const x = (b[at + 4]! << 8) | b[at + 5]!;
    const y = (b[at + 6]! << 8) | b[at + 7]!;
    at += 8;
    let crop = null;
    if (cropped && at + 8 <= b.length) {
      crop = {
        x: (b[at]! << 8) | b[at + 1]!,
        y: (b[at + 2]! << 8) | b[at + 3]!,
        w: (b[at + 4]! << 8) | b[at + 5]!,
        h: (b[at + 6]! << 8) | b[at + 7]!,
      };
      at += 8;
    }
    objects.push({ id, x, y, crop });
  }
  return { width, height, state, paletteId, objects };
}

/** Decodes PGS run-length data into palette indexes, `width × height`. */
export function decodeRle(rle: Uint8Array, width: number, height: number): Uint8Array {
  const out = new Uint8Array(width * height);
  let x = 0;
  let y = 0;
  let i = 0;
  while (i < rle.length && y < height) {
    const b = rle[i++]!;
    let color = 0;
    let count = 1;
    if (b !== 0) {
      color = b;
    } else {
      const flags = rle[i++] ?? 0;
      if (flags === 0) {
        // End of line.
        x = 0;
        y++;
        continue;
      }
      const long = (flags & 0x40) !== 0;
      count = long ? ((flags & 0x3f) << 8) | (rle[i++] ?? 0) : flags & 0x3f;
      color = flags & 0x80 ? (rle[i++] ?? 0) : 0;
    }
    const row = y * width;
    const end = Math.min(width, x + count);
    if (color !== 0) for (let px = x; px < end; px++) out[row + px] = color;
    x = end;
  }
  return out;
}

export class PgsDecoder implements PictureDecoder {
  private readonly samples: Uint8Array[];
  private readonly sets: (DisplaySet | null)[];
  private readonly cache = new FrameCache();

  constructor(samples: Uint8Array[]) {
    this.samples = samples;
    this.sets = new Array<DisplaySet | null>(samples.length).fill(null);
  }

  private set(index: number): DisplaySet {
    let s = this.sets[index];
    if (!s) {
      s = parseDisplaySet(this.samples[index]!);
      this.sets[index] = s;
    }
    return s;
  }

  ownDurationMs(): number | null {
    return null;
  }

  decode(index: number): PictureFrame | null {
    return this.cache.get(index, (i) => this.compose(i));
  }

  private compose(index: number): PictureFrame | null {
    const pcs = this.set(index).pcs;
    if (!pcs) return null;
    if (!pcs.objects.length) return { planeWidth: pcs.width, planeHeight: pcs.height, bitmaps: [] };
    // Palettes and objects as they stand at `index`: from the last epoch start or acquisition point.
    let from = index;
    while (from > 0) {
      const state = this.set(from).pcs?.state ?? 0;
      if (state & 0xc0) break;
      from--;
    }
    const palettes = new Map<number, Map<number, { y: number; cr: number; cb: number; a: number }>>();
    const objects = new Map<number, ObjectData>();
    const pending = new Map<number, { width: number; height: number; parts: Uint8Array[] }>();
    for (let i = from; i <= index; i++) {
      const s = this.set(i);
      for (const p of s.palettes) {
        let palette = palettes.get(p.id);
        if (!palette) palettes.set(p.id, (palette = new Map()));
        for (const e of p.entries) palette.set(e.index, e);
      }
      for (const o of s.objects) {
        if (o.first) pending.set(o.id, { width: o.width, height: o.height, parts: [] });
        const part = pending.get(o.id);
        if (!part) continue;
        part.parts.push(o.data);
        if (o.last) {
          objects.set(o.id, { width: part.width, height: part.height, rle: join(part.parts) });
          pending.delete(o.id);
        }
      }
    }
    const palette = palettes.get(pcs.paletteId);
    if (!palette) return null;
    const hd = pcs.height > 576;
    const rgba = new Uint8ClampedArray(256 * 4);
    palette.forEach((e, i) => {
      const [r, g, b] = ycbcrToRgb(e.y, e.cb, e.cr, hd);
      rgba[i * 4] = r;
      rgba[i * 4 + 1] = g;
      rgba[i * 4 + 2] = b;
      rgba[i * 4 + 3] = e.a;
    });
    const bitmaps: SubtitleBitmap[] = [];
    // Each placement is a picture of its own, the same object's too: together, no more than a frame's pixels.
    let pixelsLeft = MAX_PIXELS;
    for (const placed of pcs.objects) {
      const object = objects.get(placed.id);
      if (!object || !object.width || !object.height || object.width > MAX_SIDE || object.height > MAX_SIDE) continue;
      pixelsLeft -= object.width * object.height;
      if (pixelsLeft < 0) break;
      const indexes = decodeRle(object.rle, object.width, object.height);
      const crop = placed.crop ?? { x: 0, y: 0, w: object.width, h: object.height };
      const w = Math.max(0, Math.min(crop.w, object.width - crop.x));
      const h = Math.max(0, Math.min(crop.h, object.height - crop.y));
      if (!w || !h) continue;
      const pixels = new Uint8ClampedArray(w * h * 4);
      for (let row = 0; row < h; row++) {
        const src = (crop.y + row) * object.width + crop.x;
        const dst = row * w * 4;
        for (let col = 0; col < w; col++) {
          const c = indexes[src + col]! * 4;
          const o = dst + col * 4;
          pixels[o] = rgba[c]!;
          pixels[o + 1] = rgba[c + 1]!;
          pixels[o + 2] = rgba[c + 2]!;
          pixels[o + 3] = rgba[c + 3]!;
        }
      }
      bitmaps.push({ x: placed.x, y: placed.y, width: w, height: h, rgba: pixels });
    }
    return { planeWidth: pcs.width, planeHeight: pcs.height, bitmaps };
  }
}

function join(parts: Uint8Array[]): Uint8Array {
  if (parts.length === 1) return parts[0]!;
  let length = 0;
  for (const p of parts) length += p.length;
  const out = new Uint8Array(length);
  let at = 0;
  for (const p of parts) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}

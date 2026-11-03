import { FrameCache, MAX_PIXELS, MAX_SIDE, MAX_WORK, ycbcrToRgb, type PictureDecoder, type PictureFrame, type SubtitleBitmap } from "./pictures";

/**
 * Broadcast subtitles (DVB, ETSI EN 300 743, `S_DVBSUB`): each sample is a
 * display set of segments. A page composition places regions on the
 * display; each region has a colour table (CLUT) and objects drawn into
 * it as 2, 4 or 8-bit run-length coded pixels, a field at a time. A page
 * in "normal case" only updates what an acquisition point or mode change
 * defined, so a sample is decoded from the last of those.
 */

const PAGE = 0x10;
const REGION = 0x11;
const CLUT = 0x12;
const OBJECT = 0x13;
const DISPLAY = 0x14;

/** The objects a region lists at most: a real one has a few, one for each line of text. */
const MAX_REGION_OBJECTS = 64;

interface PageRegion {
  id: number;
  x: number;
  y: number;
}

interface Page {
  timeoutS: number;
  /** 0 normal case, 1 acquisition point, 2 mode change. */
  state: number;
  regions: PageRegion[];
}

interface RegionObject {
  id: number;
  x: number;
  y: number;
}

interface Region {
  width: number;
  height: number;
  /** 2, 4 or 8 bits a pixel. */
  depth: number;
  clut: number;
  fill: boolean;
  background: number;
  objects: RegionObject[];
  pixels: Uint8Array;
}

interface Segment {
  type: number;
  body: Uint8Array;
}

export function segments(data: Uint8Array): Segment[] {
  const out: Segment[] = [];
  let at = 0;
  // data_identifier 0x20 and subtitle_stream_id 0x00 open a PES data field.
  if (data[0] === 0x20 && data[1] === 0x00) at = 2;
  while (at + 6 <= data.length && data[at] === 0x0f) {
    const type = data[at + 1]!;
    const length = (data[at + 4]! << 8) | data[at + 5]!;
    out.push({ type, body: data.subarray(at + 6, Math.min(data.length, at + 6 + length)) });
    at += 6 + length;
  }
  return out;
}

function readPage(b: Uint8Array): Page {
  const regions: PageRegion[] = [];
  for (let i = 2; i + 6 <= b.length; i += 6) {
    regions.push({ id: b[i]!, x: (b[i + 2]! << 8) | b[i + 3]!, y: (b[i + 4]! << 8) | b[i + 5]! });
  }
  return { timeoutS: b[0] ?? 0, state: ((b[1] ?? 0) >> 2) & 3, regions };
}

/** The colour tables of ETSI EN 300 743 §10 for pixels a CLUT doesn't define. */
function defaultClut(depth: number): Uint8ClampedArray {
  const n = 1 << depth;
  const out = new Uint8ClampedArray(n * 4);
  const set = (i: number, r: number, g: number, b: number, a: number) => {
    out[i * 4] = r;
    out[i * 4 + 1] = g;
    out[i * 4 + 2] = b;
    out[i * 4 + 3] = a;
  };
  if (depth === 2) {
    set(0, 0, 0, 0, 0);
    set(1, 255, 255, 255, 255);
    set(2, 0, 0, 0, 255);
    set(3, 127, 127, 127, 255);
    return out;
  }
  if (depth === 4) {
    for (let i = 0; i < 16; i++) {
      const v = i < 8 ? 255 : 127;
      set(i, i & 1 ? v : 0, i & 2 ? v : 0, i & 4 ? v : 0, i === 0 ? 0 : 255);
    }
    return out;
  }
  for (let i = 0; i < 256; i++) {
    const r = i & 0x01 ? 85 : 0;
    const g = i & 0x02 ? 85 : 0;
    const b = i & 0x04 ? 85 : 0;
    const hi = i & 0x88;
    if (i < 8) {
      set(i, r * 3, g * 3, b * 3, i === 0 ? 0 : 64);
    } else if (hi === 0x00) {
      set(i, r + (i & 0x10 ? 170 : 0), g + (i & 0x20 ? 170 : 0), b + (i & 0x40 ? 170 : 0), 255);
    } else if (hi === 0x08) {
      set(i, r + (i & 0x10 ? 170 : 0), g + (i & 0x20 ? 170 : 0), b + (i & 0x40 ? 170 : 0), 127);
    } else if (hi === 0x80) {
      set(i, 127 + r / 2 + (i & 0x10 ? 43 : 0), 127 + g / 2 + (i & 0x20 ? 43 : 0), 127 + b / 2 + (i & 0x40 ? 43 : 0), 255);
    } else {
      set(i, r / 2 + (i & 0x10 ? 43 : 0), g / 2 + (i & 0x20 ? 43 : 0), b / 2 + (i & 0x40 ? 43 : 0), 255);
    }
  }
  return out;
}

class Bits {
  private bit = 0;
  private readonly data: Uint8Array;
  private at: number;
  private readonly end: number;

  constructor(data: Uint8Array, at: number, end: number) {
    this.data = data;
    this.at = at;
    this.end = end;
  }

  get done(): boolean {
    return this.at >= this.end;
  }

  read(n: number): number {
    let v = 0;
    for (let i = 0; i < n; i++) {
      const byte = this.at < this.end ? this.data[this.at]! : 0;
      v = (v << 1) | ((byte >> (7 - this.bit)) & 1);
      if (++this.bit === 8) {
        this.bit = 0;
        this.at++;
      }
    }
    return v;
  }

  /** The byte after the string: strings end on a byte. */
  align(): number {
    if (this.bit) {
      this.bit = 0;
      this.at++;
    }
    return this.at;
  }
}

/** Runs of one pixel string: `[count, pixel]` pairs, until its end code. */
function pixelString(bits: Bits, depth: number, emit: (count: number, pixel: number) => void): void {
  for (;;) {
    if (bits.done) return;
    if (depth === 2) {
      const v = bits.read(2);
      if (v) emit(1, v);
      else if (bits.read(1)) emit(bits.read(3) + 3, bits.read(2));
      else if (bits.read(1)) emit(1, 0);
      else {
        const s = bits.read(2);
        if (s === 0) return;
        if (s === 1) emit(2, 0);
        else if (s === 2) emit(bits.read(4) + 12, bits.read(2));
        else emit(bits.read(8) + 29, bits.read(2));
      }
    } else if (depth === 4) {
      const v = bits.read(4);
      if (v) emit(1, v);
      else if (!bits.read(1)) {
        const run = bits.read(3);
        if (!run) return;
        emit(run + 2, 0);
      } else if (!bits.read(1)) {
        const run = bits.read(2) + 4;
        emit(run, bits.read(4));
      } else {
        const s = bits.read(2);
        if (s === 0) emit(1, 0);
        else if (s === 1) emit(2, 0);
        else if (s === 2) emit(bits.read(4) + 9, bits.read(4));
        else emit(bits.read(8) + 25, bits.read(4));
      }
    } else {
      const v = bits.read(8);
      if (v) emit(1, v);
      else if (!bits.read(1)) {
        const run = bits.read(7);
        if (!run) return;
        emit(run, 0);
      } else {
        const run = bits.read(7);
        emit(run, bits.read(8));
      }
    }
  }
}

/**
 * Draws one field of an object's pixel data into `region` at (`ox`, `oy`),
 * every other line from `first`; the work it took, bytes read and pixels
 * written.
 */
function drawField(data: Uint8Array, region: Region, ox: number, oy: number, first: number): number {
  let written = 0;
  let at = 0;
  let x = ox;
  let y = oy + first;
  // Map tables, for codes shallower than the region.
  let map24 = [0x0, 0x7, 0x8, 0xf];
  let map28 = [0x00, 0x77, 0x88, 0xff];
  let map48 = Array.from({ length: 16 }, (_, i) => i * 0x11);
  while (at < data.length) {
    const type = data[at++]!;
    if (type === 0xf0) {
      x = ox;
      y += 2;
      continue;
    }
    if (type === 0x20) {
      map24 = [data[at]! >> 4, data[at]! & 0xf, data[at + 1]! >> 4, data[at + 1]! & 0xf];
      at += 2;
      continue;
    }
    if (type === 0x21) {
      map28 = [data[at]!, data[at + 1]!, data[at + 2]!, data[at + 3]!];
      at += 4;
      continue;
    }
    if (type === 0x22) {
      map48 = [];
      for (let i = 0; i < 8; i++) map48.push(data[at + i]! >> 4, data[at + i]! & 0xf);
      at += 8;
      continue;
    }
    const depth = type === 0x10 ? 2 : type === 0x11 ? 4 : type === 0x12 ? 8 : 0;
    if (!depth) break;
    const bits = new Bits(data, at, data.length);
    const map = (p: number): number => {
      if (depth === region.depth) return p;
      if (depth === 2) return region.depth === 4 ? map24[p]! : map28[p]!;
      if (depth === 4 && region.depth === 8) return map48[p]!;
      // Deeper codes than the region: keep the low bits.
      return p & ((1 << region.depth) - 1);
    };
    pixelString(bits, depth, (count, pixel) => {
      if (y < region.height) {
        const start = Math.max(0, x);
        const stop = Math.min(region.width, x + count);
        const value = map(pixel);
        const row = y * region.width;
        for (let px = start; px < stop; px++) region.pixels[row + px] = value;
        if (stop > start) written += stop - start;
      }
      x += count;
    });
    at = bits.align();
  }
  return at + written;
}

export class DvbDecoder implements PictureDecoder {
  private readonly samples: Uint8Array[];
  private readonly parsed: (Segment[] | undefined)[];
  private readonly cache = new FrameCache();

  constructor(samples: Uint8Array[]) {
    this.samples = samples;
    this.parsed = new Array<Segment[] | undefined>(samples.length);
  }

  private segs(index: number): Segment[] {
    let s = this.parsed[index];
    if (!s) {
      s = segments(this.samples[index]!);
      this.parsed[index] = s;
    }
    return s;
  }

  private page(index: number): Page | null {
    const seg = this.segs(index).filter((s) => s.type === PAGE)[0];
    return seg ? readPage(seg.body) : null;
  }

  ownDurationMs(index: number): number | null {
    const page = this.page(index);
    return page && page.timeoutS > 0 ? page.timeoutS * 1000 : null;
  }

  decode(index: number): PictureFrame | null {
    return this.cache.get(index, (i) => this.render(i));
  }

  private render(index: number): PictureFrame | null {
    let from = index;
    while (from > 0) {
      const page = this.page(from);
      if (page && page.state !== 0) break;
      from--;
    }
    let width = 720;
    let height = 576;
    let page: Page | null = null;
    const regions = new Map<number, Region>();
    const cluts = new Map<number, Uint8ClampedArray[]>();
    const hd = () => height > 576;
    // Bytes read, pixels filled and drawn: a crafted epoch could fill a 4K
    // region, or draw an object into each of thousands of places, in every
    // segment of every display set. What it shows stops there.
    let work = MAX_WORK;
    for (let i = from; i <= index && work > 0; i++) {
      for (const seg of this.segs(i)) {
        const b = seg.body;
        work -= 1 + b.length;
        if (work <= 0) break;
        switch (seg.type) {
          case DISPLAY:
            if (b.length >= 5) {
              width = ((b[1]! << 8) | b[2]!) + 1;
              height = ((b[3]! << 8) | b[4]!) + 1;
            }
            break;
          case PAGE:
            page = readPage(b);
            if (page.state !== 0) regions.clear();
            break;
          case REGION: {
            if (b.length < 10) break;
            const id = b[0]!;
            const fill = (b[1]! & 0x08) !== 0;
            const w = (b[2]! << 8) | b[3]!;
            const h = (b[4]! << 8) | b[5]!;
            if (w > MAX_SIDE || h > MAX_SIDE) break;
            // Up to 256 regions are kept at once: together, no more than a frame's pixels.
            let held = w * h;
            regions.forEach((other, otherId) => {
              if (otherId !== id) held += other.pixels.length;
            });
            if (held > MAX_PIXELS) break;
            const depth = [0, 2, 4, 8][(b[6]! >> 2) & 7] ?? 4;
            const clut = b[7]!;
            const background = depth === 8 ? b[8]! : depth === 4 ? b[9]! >> 4 : (b[9]! >> 2) & 3;
            const objects: RegionObject[] = [];
            for (let o = 10; o + 6 <= b.length && objects.length < MAX_REGION_OBJECTS; ) {
              const objectType = b[o + 2]! >> 6;
              objects.push({ id: (b[o]! << 8) | b[o + 1]!, x: ((b[o + 2]! & 0x0f) << 8) | b[o + 3]!, y: ((b[o + 4]! & 0x0f) << 8) | b[o + 5]! });
              o += objectType === 1 || objectType === 2 ? 8 : 6;
            }
            let region = regions.get(id);
            if (!region || region.width !== w || region.height !== h || region.depth !== depth) {
              work -= w * h;
              region = { width: w, height: h, depth, clut, fill, background, objects, pixels: new Uint8Array(w * h) };
              regions.set(id, region);
            } else {
              region.clut = clut;
              region.fill = fill;
              region.background = background;
              region.objects = objects;
            }
            if (fill) {
              work -= w * h;
              region.pixels.fill(background);
            }
            break;
          }
          case CLUT: {
            if (b.length < 2) break;
            const id = b[0]!;
            const tables = cluts.get(id) ?? [defaultClut(2), defaultClut(4), defaultClut(8)];
            for (let at = 2; at + 2 <= b.length; ) {
              const entry = b[at]!;
              const flags = b[at + 1]!;
              let y: number;
              let cr: number;
              let cb: number;
              let t: number;
              if (flags & 0x01) {
                if (at + 6 > b.length) break;
                y = b[at + 2]!;
                cr = b[at + 3]!;
                cb = b[at + 4]!;
                t = b[at + 5]!;
                at += 6;
              } else {
                if (at + 4 > b.length) break;
                y = b[at + 2]! & 0xfc;
                cr = ((b[at + 2]! & 0x03) << 6) | ((b[at + 3]! >> 2) & 0x30);
                cb = (b[at + 3]! << 2) & 0xf0;
                t = (b[at + 3]! & 0x03) * 0x55;
                at += 4;
              }
              // Y 0 is transparent whatever T says.
              const [r, g, bl] = ycbcrToRgb(y, cb, cr, hd());
              const alpha = y === 0 ? 0 : 255 - t;
              const put = (table: Uint8ClampedArray, i: number) => {
                if (i * 4 + 3 >= table.length) return;
                table[i * 4] = r;
                table[i * 4 + 1] = g;
                table[i * 4 + 2] = bl;
                table[i * 4 + 3] = alpha;
              };
              if (flags & 0x80) put(tables[0]!, entry);
              if (flags & 0x40) put(tables[1]!, entry);
              if (flags & 0x20) put(tables[2]!, entry);
            }
            cluts.set(id, tables);
            break;
          }
          case OBJECT: {
            if (b.length < 7) break;
            const id = (b[0]! << 8) | b[1]!;
            const coding = (b[2]! >> 2) & 3;
            if (coding !== 0) break;
            const topLength = (b[3]! << 8) | b[4]!;
            const bottomLength = (b[5]! << 8) | b[6]!;
            const top = b.subarray(7, 7 + topLength);
            const bottom = bottomLength ? b.subarray(7 + topLength, 7 + topLength + bottomLength) : top;
            regions.forEach((region) => {
              work -= region.objects.length;
              for (const o of region.objects) {
                if (work <= 0) return;
                if (o.id !== id) continue;
                work -= 1 + drawField(top, region, o.x, o.y, 0) + drawField(bottom, region, o.x, o.y, 1);
              }
            });
            break;
          }
        }
      }
    }
    if (!page) return null;
    const bitmaps: SubtitleBitmap[] = [];
    // A page may place a region any number of times, each a picture of its own.
    let pixelsLeft = MAX_PIXELS;
    for (const placed of page.regions) {
      const region = regions.get(placed.id);
      if (!region || !region.width || !region.height) continue;
      pixelsLeft -= region.width * region.height;
      if (pixelsLeft < 0) break;
      const table = (cluts.get(region.clut) ?? [defaultClut(2), defaultClut(4), defaultClut(8)])[region.depth === 2 ? 0 : region.depth === 4 ? 1 : 2]!;
      const rgba = new Uint8ClampedArray(region.width * region.height * 4);
      let visible = false;
      for (let i = 0; i < region.pixels.length; i++) {
        const c = region.pixels[i]! * 4;
        const o = i * 4;
        rgba[o] = table[c]!;
        rgba[o + 1] = table[c + 1]!;
        rgba[o + 2] = table[c + 2]!;
        rgba[o + 3] = table[c + 3]!;
        if (table[c + 3]) visible = true;
      }
      if (visible) bitmaps.push({ x: placed.x, y: placed.y, width: region.width, height: region.height, rgba });
    }
    return { planeWidth: width, planeHeight: height, bitmaps };
  }
}

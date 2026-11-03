import { describe, expect, it } from "vitest";
import { MAX_SAMPLES, readPictureTrack } from "../src/mks";
import { decodeRle, PgsDecoder } from "../src/pgs";
import { parseIdx, parseSpu, VobSubDecoder } from "../src/vobsub";
import { DvbDecoder, segments } from "../src/dvb";
import { PictureSubtitles } from "../src/picture-track";
import { FrameCache, ycbcrToRgb, type PictureFrame } from "../src/pictures";

// ---- builders

const u16 = (v: number) => [(v >> 8) & 0xff, v & 0xff];

function pgsSegment(type: number, body: number[]): number[] {
  return [type, ...u16(body.length), ...body];
}

/**
 * A display set placing a `w`×`h` object at (x, y) on a 1920×1080 plane,
 * with palette entry 1 white; or as the PCS's own entries `placed` say.
 */
function pgsDisplaySet(opts: { state: number; objects: boolean; x?: number; y?: number; rle?: number[]; w?: number; h?: number; placed?: number[][] }): Uint8Array {
  const { x = 100, y = 900, w = 4, h = 2 } = opts;
  const placed = opts.placed ?? (opts.objects ? [[...u16(0), 0, 0x00, ...u16(x), ...u16(y)]] : []);
  const pcs = [...u16(1920), ...u16(1080), 0x10, ...u16(1), opts.state, 0x00, 0x00, placed.length, ...placed.flat()];
  const out = [...pgsSegment(0x16, pcs)];
  if (opts.rle) {
    out.push(...pgsSegment(0x17, [1, 0, ...u16(x), ...u16(y), ...u16(w), ...u16(h)]));
    // Palette 0: entry 1 white (Y 235, Cr 128, Cb 128), opaque; entry 2 half-transparent black.
    out.push(...pgsSegment(0x14, [0, 0, 1, 235, 128, 128, 255, 2, 16, 128, 128, 128]));
    const data = [...u16(w), ...u16(h), ...opts.rle];
    const length = data.length;
    out.push(...pgsSegment(0x15, [...u16(0), 0, 0xc0, (length >> 16) & 0xff, (length >> 8) & 0xff, length & 0xff, ...data]));
  }
  out.push(...pgsSegment(0x80, []));
  return new Uint8Array(out);
}

/** An epoch start placing one `w`×`h` object `times` times on a 1920×1080 plane; its pixels are all transparent. */
function pgsPlaced(w: number, h: number, times: number): Uint8Array {
  const pcs = [...u16(1920), ...u16(1080), 0x10, ...u16(1), 0x80, 0x00, 0x00, times];
  for (let i = 0; i < times; i++) pcs.push(...u16(0), 0, 0x00, ...u16(i * 10), ...u16(0));
  const data = [...u16(w), ...u16(h)];
  return new Uint8Array([
    ...pgsSegment(0x16, pcs),
    ...pgsSegment(0x14, [0, 0, 1, 235, 128, 128, 255]),
    ...pgsSegment(0x15, [...u16(0), 0, 0xc0, 0, 0, data.length, ...data]),
    ...pgsSegment(0x80, []),
  ]);
}

/** An EBML element. */
function el(id: number, body: number[]): number[] {
  const idBytes: number[] = [];
  for (let v = id; v > 0; v = Math.floor(v / 256)) idBytes.unshift(v & 0xff);
  const size = body.length;
  // 8-byte sizes keep the builder simple.
  const sizeBytes = [0x01, 0, 0, 0, (size >>> 24) & 0xff, (size >> 16) & 0xff, (size >> 8) & 0xff, size & 0xff];
  return [...idBytes, ...sizeBytes, ...body];
}

const str = (s: string) => Array.from(s, (c) => c.charCodeAt(0));

function mks(codecId: string, blocks: { time: number; data: Uint8Array; duration?: number }[], codecPrivate?: number[]): ArrayBuffer {
  const track = el(0xae, [
    ...el(0xd7, [1]),
    ...el(0x83, [0x11]),
    ...el(0x86, str(codecId)),
    ...(codecPrivate ? el(0x63a2, codecPrivate) : []),
  ]);
  const cluster: number[] = [...el(0xe7, [0])];
  for (const b of blocks) {
    const block = [0x81, ...u16(b.time), 0x00, ...b.data];
    if (b.duration !== undefined) cluster.push(...el(0xa0, [...el(0xa1, block), ...el(0x9b, u16(b.duration))]));
    else cluster.push(...el(0xa3, [0x81, ...u16(b.time), 0x80, ...b.data]));
  }
  const segment = el(0x18538067, [...el(0x1549a966, [...el(0x2ad7b1, [0x0f, 0x42, 0x40])]), ...el(0x1654ae6b, track), ...el(0x1f43b675, cluster)]);
  const bytes = new Uint8Array([...el(0x1a45dfa3, [...el(0x4282, str("matroska"))]), ...segment]);
  return bytes.buffer;
}

// ---- tests

describe("PGS", () => {
  it("decodes runs, line ends and long runs", () => {
    // Line 1: 2 × colour 1, 2 × colour 0. Line 2: 4 × colour 2 (short coloured run).
    const rle = [1, 1, 0, 0x02, 0, 0, 0, 0x84, 2, 0, 0];
    expect(Array.from(decodeRle(new Uint8Array(rle), 4, 2))).toEqual([1, 1, 0, 0, 2, 2, 2, 2]);
    const long = [0, 0xc1, 0x00, 7, 0, 0];
    expect(decodeRle(new Uint8Array(long), 300, 1).every((p) => p === 7)).toBe(false);
    expect(Array.from(decodeRle(new Uint8Array(long), 300, 1).slice(0, 256)).every((p) => p === 7)).toBe(true);
  });

  it("places an object on the plane in its palette's colours", () => {
    const set = pgsDisplaySet({ state: 0x80, objects: true, rle: [1, 1, 0, 0x02, 0, 0, 0, 0x84, 2, 0, 0] });
    const frame = new PgsDecoder([set]).decode(0)!;
    expect(frame.planeWidth).toBe(1920);
    expect(frame.planeHeight).toBe(1080);
    expect(frame.bitmaps).toHaveLength(1);
    const b = frame.bitmaps[0]!;
    expect([b.x, b.y, b.width, b.height]).toEqual([100, 900, 4, 2]);
    expect(Array.from(b.rgba.slice(0, 4))).toEqual([255, 255, 255, 255]);
    expect(b.rgba[3 + 4 * 2]).toBe(0);
    expect(Array.from(b.rgba.slice(16, 20))).toEqual([0, 0, 0, 128]);
  });

  it("decodes a normal-case display set from its epoch's definitions", () => {
    const start = pgsDisplaySet({ state: 0x80, objects: true, rle: [1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0] });
    const clear = pgsDisplaySet({ state: 0x00, objects: false });
    const again = pgsDisplaySet({ state: 0x00, objects: true, x: 200, y: 950 });
    const decoder = new PgsDecoder([start, clear, again]);
    expect(decoder.decode(1)!.bitmaps).toEqual([]);
    const reused = decoder.decode(2)!.bitmaps[0]!;
    expect([reused.x, reused.y]).toEqual([200, 950]);
    expect(reused.rgba[3]).toBe(255);
  });

  it("crops an object only where its place says so, and shows a forced one whole", () => {
    const rle = [1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0];
    const boxes = (placed: number[][]) =>
      new PgsDecoder([pgsDisplaySet({ state: 0x80, objects: true, rle, placed })]).decode(0)!.bitmaps.map((b) => [b.x, b.y, b.width, b.height]);
    // Cropped (0x80) to 2×1 from (1, 1).
    expect(boxes([[...u16(0), 0, 0x80, ...u16(100), ...u16(900), ...u16(1), ...u16(1), ...u16(2), ...u16(1)]])).toEqual([[100, 900, 2, 1]]);
    // Forced (0x40), with a second place after it: no crop to read.
    expect(boxes([[...u16(0), 0, 0x40, ...u16(100), ...u16(900)], [...u16(0), 0, 0x00, ...u16(200), ...u16(950)]])).toEqual([
      [100, 900, 4, 2],
      [200, 950, 4, 2],
    ]);
  });

  it("decodes a frame's worth of pixels at most", () => {
    const sizes = (set: Uint8Array) => new PgsDecoder([set]).decode(0)!.bitmaps.map((b) => [b.width, b.height]);
    // One 2048×2048 object placed three times: twice would pass a 4K screen's pixels.
    expect(sizes(pgsPlaced(2048, 2048, 3))).toEqual([[2048, 2048]]);
    // A Blu-ray's whole plane, twice, is well within; twice its height, twice, just so.
    expect(sizes(pgsPlaced(1920, 1080, 2))).toEqual([
      [1920, 1080],
      [1920, 1080],
    ]);
    expect(sizes(pgsPlaced(1920, 2160, 2))).toEqual([
      [1920, 2160],
      [1920, 2160],
    ]);
  });

  it("places two objects at most, as Blu-ray does", () => {
    const sizes = (set: Uint8Array) => new PgsDecoder([set]).decode(0)!.bitmaps.map((b) => [b.width, b.height]);
    expect(sizes(pgsPlaced(1920, 2160, 3))).toHaveLength(2);
    expect(sizes(pgsPlaced(1, 1, 255))).toHaveLength(2);
  });

  it("reads 256 segments of a display set at most, and 256 entries of a palette", () => {
    const rle = [1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0];
    // The composition, then empty windows, then the palette and object.
    const padded = (windows: number) => {
      const set = pgsDisplaySet({ state: 0x80, objects: true, rle });
      const pcs = set.subarray(0, 3 + ((set[1]! << 8) | set[2]!));
      return new Uint8Array([...pcs, ...Array.from({ length: windows }, () => pgsSegment(0x17, [])).flat(), ...set.subarray(pcs.length)]);
    };
    expect(new PgsDecoder([padded(200)]).decode(0)!.bitmaps).toHaveLength(1);
    expect(new PgsDecoder([padded(300)]).decode(0)).toBeNull();
    // Entry 1 opaque white, then 255 more, then entry 1 again, transparent: not read.
    const entries = [1, 235, 128, 128, 255, ...Array.from({ length: 255 }, () => [2, 16, 128, 128, 255]).flat(), 1, 235, 128, 128, 0];
    const composition = pgsDisplaySet({ state: 0x80, objects: true });
    const set = new Uint8Array([
      ...composition.subarray(0, composition.length - 3),
      ...pgsSegment(0x14, [0, 1, ...entries]),
      ...pgsSegment(0x15, [...u16(0), 0, 0xc0, 0, 0, 16, ...u16(4), ...u16(2), ...rle]),
      ...pgsSegment(0x80, []),
    ]);
    expect(new PgsDecoder([set]).decode(0)!.bitmaps[0]!.rgba[3]).toBe(255);
  });

  it("decodes a frame from 1024 display sets back at most", () => {
    const start = pgsDisplaySet({ state: 0x80, objects: true, rle: [1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0] });
    const again = pgsDisplaySet({ state: 0x00, objects: true });
    const decoder = new PgsDecoder([start, ...Array.from({ length: 1025 }, () => again)]);
    expect(decoder.decode(1024)!.bitmaps).toHaveLength(1);
    expect(decoder.decode(1025)).toBeNull();
  });

  it("decodes objects of 4096 pixels a side at most", () => {
    const sizes = (set: Uint8Array) => new PgsDecoder([set]).decode(0)!.bitmaps.map((b) => [b.width, b.height]);
    expect(sizes(pgsPlaced(4096, 1, 1))).toEqual([[4096, 1]]);
    expect(sizes(pgsPlaced(4097, 1, 1))).toEqual([]);
    expect(sizes(pgsPlaced(1, 4097, 1))).toEqual([]);
  });
});

describe("FrameCache", () => {
  const frame = (bytes: number): PictureFrame => ({
    planeWidth: 1920,
    planeHeight: 1080,
    bitmaps: [{ x: 0, y: 0, width: 1, height: 1, rgba: new Uint8ClampedArray(bytes) }],
  });

  it("keeps the frames decoded last within their bytes and their number, always the one just decoded", () => {
    const cache = new FrameCache(25, 300);
    for (const i of [0, 1, 2]) cache.get(i, () => frame(101));
    expect([0, 1, 2].map((i) => cache.has(i))).toEqual([false, true, true]);
    // A frame over the bound alone is still kept: it is the one showing.
    const big = frame(301);
    expect(cache.get(3, () => big)).toBe(big);
    expect([1, 2, 3].map((i) => cache.has(i))).toEqual([false, false, true]);
    expect(cache.get(3, () => frame(1))).toBe(big);
    // Frames with nothing to show are kept by number.
    for (let i = 10; i < 40; i++) cache.get(i, () => null);
    expect([3, 14, 15, 39].map((i) => cache.has(i))).toEqual([false, false, true, true]);
  });
});

describe("Matroska", () => {
  it("reads the track, its samples' times and durations", () => {
    const set = pgsDisplaySet({ state: 0x80, objects: true, rle: [1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0] });
    const track = readPictureTrack(
      mks("S_HDMV/PGS", [
        { time: 1000, data: set },
        { time: 3000, data: pgsDisplaySet({ state: 0, objects: false }) },
        { time: 5000, data: set, duration: 1500 },
      ]),
    );
    expect(track.codecId).toBe("S_HDMV/PGS");
    expect(track.samples.map((s) => [s.timeMs, s.durationMs])).toEqual([
      [1000, null],
      [3000, null],
      [5000, 1500],
    ]);
    const subs = PictureSubtitles.of(track)!;
    expect(subs.frameAt(500)).toBeNull();
    expect(subs.frameAt(1500)!.index).toBe(0);
    expect(subs.frameAt(3200)).toBeNull();
    expect(subs.frameAt(6000)!.index).toBe(2);
    expect(subs.frameAt(6600)).toBeNull();
    expect(subs.nextChange(1500)).toBe(3000);
    expect(subs.nextChange(4000)).toBe(5000);
    expect(subs.nextChange(5100)).toBe(6500);
  });

  it("reads a file cut short up to the cut, without the sample cut in two", () => {
    const set = pgsDisplaySet({ state: 0x80, objects: true, rle: [1, 1, 1, 1, 0, 0, 1, 1, 1, 1, 0, 0] });
    const whole = mks("S_HDMV/PGS", [
      { time: 1000, data: set },
      { time: 3000, data: pgsDisplaySet({ state: 0, objects: false }) },
      { time: 5000, data: set },
    ]);
    // Up to the middle of the last block.
    const cut = whole.slice(0, whole.byteLength - Math.floor(set.length / 2));
    const track = readPictureTrack(cut);
    expect(track.samples.map((s) => s.timeMs)).toEqual([1000, 3000]);
    expect(track.samples[0]!.data).toEqual(set);
    // Cut inside the track list: no track yet.
    expect(() => readPictureTrack(whole.slice(0, 60))).toThrow(/no track/);
  });

  it("refuses what isn't Matroska", () => {
    expect(() => readPictureTrack(new Uint8Array([1, 2, 3, 4]).buffer)).toThrow(/not a Matroska file/);
  });

  it("reads 100,000 samples of a track at most", () => {
    const blocks = Array.from({ length: MAX_SAMPLES + 5 }, (_, i) => ({ time: i % 60_000, data: new Uint8Array([0]) }));
    expect(readPictureTrack(mks("S_HDMV/PGS", blocks)).samples.length).toBe(MAX_SAMPLES);
  });

  it("reads elements 8 deep at most, however deep they go", () => {
    // 100,000 block groups, each in the one before, around a block.
    const n = 100_000;
    const block = el(0xa1, [0x81, ...u16(0), 0x00, 1]);
    const nested = new Uint8Array(n * 9 + block.length);
    for (let i = 0; i < n; i++) {
      const size = (n - 1 - i) * 9 + block.length;
      nested.set([0xa0, 0x01, 0, 0, 0, (size >>> 24) & 0xff, (size >> 16) & 0xff, (size >> 8) & 0xff, size & 0xff], i * 9);
    }
    nested.set(block, n * 9);
    const track = el(0xae, [...el(0xd7, [1]), ...el(0x83, [0x11]), ...el(0x86, str("S_HDMV/PGS"))]);
    const cluster = el(0x1f43b675, [...el(0xe7, [0]), ...nested]);
    const segment = el(0x18538067, [...el(0x1549a966, [...el(0x2ad7b1, [0x0f, 0x42, 0x40])]), ...el(0x1654ae6b, track), ...cluster]);
    const file = new Uint8Array([...el(0x1a45dfa3, [...el(0x4282, str("matroska"))]), ...segment]);
    expect(readPictureTrack(file.buffer).samples).toEqual([]);
  });
});

describe("VobSub", () => {
  const idx = str("size: 720x480\npalette: 000000, ffffff, 808080, ff0000, 00ff00, 0000ff, 000000, 000000, 000000, 000000, 000000, 000000, 000000, 000000, 000000, 000000\n");

  /**
   * An SPU of a 4×2 picture at (10, 400): top line pattern colour, bottom
   * line emphasis 1, shown for 2 s; or of the corners `area` says.
   */
  function spu(area = [0x00, 0xa0, 0x0d, 0x19, 0x01, 0x91]): Uint8Array {
    // Nibble codes: 0x11 → 4 px... use 8-bit codes: 0001 00cc = count 4, colour cc.
    const top = [0x11, 0x00]; // count 4 colour 1, then padding nibble byte align
    const bottom = [0x12, 0x00]; // count 4 colour 2
    const pixels = [...top, ...bottom];
    const ctrlAt = 4 + pixels.length;
    const stopSeqAt = ctrlAt + 4 + 1 + 3 + 3 + 7 + 5 + 1;
    const ctrl = [
      ...u16(0), ...u16(stopSeqAt), 0x01,
      0x03, 0x32, 0x10, // colours: e2=3 e1=2 p=1 b=0
      0x04, 0xff, 0xf0, // alpha: e2 15, e1 15, p 15, b 0
      0x05, ...area, // x 10..13, y 400..401
      0x06, ...u16(4), ...u16(6),
      0xff,
    ];
    const stop = [...u16(175), ...u16(stopSeqAt), 0x02, 0xff];
    const all = [...pixels, ...ctrl, ...stop];
    return new Uint8Array([...u16(4 + all.length), ...u16(ctrlAt), ...all]);
  }

  it("reads the idx header", () => {
    const h = parseIdx(new Uint8Array(idx));
    expect([h.width, h.height]).toEqual([720, 480]);
    expect(h.palette[1]).toEqual([255, 255, 255]);
    expect(h.palette[3]).toEqual([255, 0, 0]);
  });

  it("reads control sequences: place, colours and when it stops", () => {
    const s = parseSpu(spu())!;
    expect([s.x1, s.x2, s.y1, s.y2]).toEqual([10, 13, 400, 401]);
    expect(s.colors).toEqual([0, 1, 2, 3]);
    expect(s.alpha).toEqual([0, 15, 15, 15]);
    expect(s.stopMs).toBe(Math.round((175 * 1024) / 90));
  });

  it("decodes interlaced fields in the palette's colours", () => {
    const decoder = new VobSubDecoder([spu()], new Uint8Array(idx));
    const frame = decoder.decode(0)!;
    expect([frame.planeWidth, frame.planeHeight]).toEqual([720, 480]);
    const b = frame.bitmaps[0]!;
    expect([b.x, b.y, b.width, b.height]).toEqual([10, 400, 4, 2]);
    expect(Array.from(b.rgba.slice(0, 4))).toEqual([255, 255, 255, 255]);
    expect(Array.from(b.rgba.slice(16, 20))).toEqual([128, 128, 128, 255]);
    expect(decoder.ownDurationMs(0)).toBe(Math.round((175 * 1024) / 90));
  });

  it("refuses a picture of more than a frame's pixels", () => {
    const sizes = (area: number[]) => {
      const frame = new VobSubDecoder([spu(area)], new Uint8Array(idx)).decode(0);
      return frame && frame.bitmaps.map((b) => [b.width, b.height]);
    };
    // x 0..3839, y 0..2159: a 4K screen's worth, just so.
    expect(sizes([0x00, 0x0e, 0xff, 0x00, 0x08, 0x6f])).toEqual([[3840, 2160]]);
    // One line more.
    expect(sizes([0x00, 0x0e, 0xff, 0x00, 0x08, 0x70])).toBeNull();
  });
});

describe("DVB", () => {
  function dvbSegment(type: number, body: number[]): number[] {
    return [0x0f, type, ...u16(1), ...u16(body.length), ...body];
  }

  /** A mode-change page with one 4-bit region of 4×2 at (60, 500), CLUT entry 1 red, object filled with entry 1. */
  function displaySet(): Uint8Array {
    const page = dvbSegment(0x10, [5, (0 << 4) | (2 << 2), 0, 0, ...u16(60), ...u16(500)]);
    const region = dvbSegment(0x11, [0, 0x00, ...u16(4), ...u16(2), (2 << 5) | (2 << 2), 0, 0, 0, ...u16(0), (0 << 6) | 0, 0, 0, 0]);
    // CLUT 0: entry 1 for 4-bit tables, full range: Y 81 Cr 240 Cb 90 (red), opaque.
    const clut = dvbSegment(0x12, [0, 0, 1, 0x40 | 0x01, 81, 240, 90, 0]);
    // 4-bit string: 4 × pixel 1 ("0001" ×4), then end of string (0000 0 000), end of line.
    const line = [0x11, 0x11, 0x11, 0x00, 0x00, 0xf0];
    const object = dvbSegment(0x13, [...u16(0), 0x00, ...u16(line.length), ...u16(0), ...line]);
    const end = dvbSegment(0x80, []);
    return new Uint8Array([0x20, 0x00, ...page, ...region, ...clut, ...object, ...end, 0xff]);
  }

  it("splits a display set into segments", () => {
    expect(segments(displaySet()).map((s) => s.type)).toEqual([0x10, 0x11, 0x12, 0x13, 0x80]);
  });

  it("draws objects into regions placed by the page", () => {
    const decoder = new DvbDecoder([displaySet()]);
    const frame = decoder.decode(0)!;
    expect([frame.planeWidth, frame.planeHeight]).toEqual([720, 576]);
    const b = frame.bitmaps[0]!;
    expect([b.x, b.y, b.width, b.height]).toEqual([60, 500, 4, 2]);
    const [r, g, bl] = ycbcrToRgb(81, 90, 240, false);
    expect(Array.from(b.rgba.slice(0, 4))).toEqual([r, g, bl, 255]);
    // The bottom field is the top one: both lines drawn.
    expect(b.rgba[4 * 4 + 3]).toBe(255);
    expect(decoder.ownDurationMs(0)).toBe(5000);
  });

  /** A mode-change page placing regions, each `[id, x, y]`. */
  const placing = (...regions: [number, number, number][]) => dvbSegment(0x10, [5, 2 << 2, ...regions.flatMap(([id, x, y]) => [id, 0, ...u16(x), ...u16(y)])]);
  /** A 4-bit region filled with entry 1 of CLUT 0. */
  const filled = (id: number, w: number, h: number) => dvbSegment(0x11, [id, 0x08, ...u16(w), ...u16(h), 2 << 2, 0, 0, 1 << 4]);
  /** CLUT 0 with entry 1 opaque red. */
  const red = dvbSegment(0x12, [0, 0, 1, 0x40 | 0x01, 81, 240, 90, 0]);
  const sizes = (...segs: number[][]) => {
    const set = new Uint8Array([0x20, 0x00, ...segs.flat(), ...dvbSegment(0x80, []), 0xff]);
    return new DvbDecoder([set]).decode(0)!.bitmaps.map((b) => [b.width, b.height]);
  };

  it("decodes a frame's worth of pixels at most", () => {
    // One 3000×2000 region placed twice: twice would pass a 4K screen's pixels.
    expect(sizes(placing([0, 0, 0], [0, 10, 10]), filled(0, 3000, 2000), red)).toEqual([[3000, 2000]]);
  });

  it("places 32 regions at most", () => {
    const places = Array.from({ length: 1000 }, (_, i): [number, number, number] => [0, i % 700, 0]);
    expect(sizes(placing(...places), filled(0, 1, 1), red)).toHaveLength(32);
  });

  it("reads 256 segments of a display set at most", () => {
    const empty = Array.from({ length: 300 }, () => dvbSegment(0x16, []));
    const set = new Uint8Array([0x20, 0x00, ...placing([0, 0, 0]), ...empty.flat(), ...filled(0, 4, 2), ...red, 0xff]);
    expect(segments(set)).toHaveLength(256);
    expect(new DvbDecoder([set]).decode(0)!.bitmaps).toEqual([]);
    expect(sizes(placing([0, 0, 0]), ...empty.slice(0, 200), filled(0, 4, 2), red)).toEqual([[4, 2]]);
  });

  it("decodes a frame from 1024 display sets back at most", () => {
    const start = new Uint8Array([0x20, 0x00, ...placing([0, 0, 0]), ...filled(0, 4, 2), ...red, 0xff]);
    // Normal case: the page again.
    const again = new Uint8Array([0x20, 0x00, ...dvbSegment(0x10, [5, 0, 0, 0, ...u16(0), ...u16(0)]), 0xff]);
    const decoder = new DvbDecoder([start, ...Array.from({ length: 1025 }, () => again)]);
    expect(decoder.decode(1024)!.bitmaps).toHaveLength(1);
    expect(decoder.decode(1025)!.bitmaps).toEqual([]);
  });

  it("draws an object in the first 64 places a region lists at most", () => {
    // A 100×2 region listing object 1 at (0, 0) 64 times, then at (50, 0).
    const places = [...Array.from({ length: 64 }, () => [...u16(1), 0, 0, 0, 0]), [...u16(1), 0, 50, 0, 0]];
    const region = dvbSegment(0x11, [0, 0x00, ...u16(100), ...u16(2), 2 << 2, 0, 0, 0, ...places.flat()]);
    // One pixel of entry 1, then the end of the string and of the line.
    const line = [0x11, 0x10, 0x00, 0xf0];
    const object = dvbSegment(0x13, [...u16(1), 0x00, ...u16(line.length), ...u16(0), ...line]);
    const set = new Uint8Array([0x20, 0x00, ...placing([0, 0, 0]), ...region, ...red, ...object, ...dvbSegment(0x80, []), 0xff]);
    const b = new DvbDecoder([set]).decode(0)!.bitmaps[0]!;
    expect([b.rgba[3], b.rgba[50 * 4 + 3]]).toEqual([255, 0]);
  });

  it("stops decoding a frame once it has taken eight 4K screens' work", () => {
    // Region 0, 3840×2000, filled again and again; then region 1, the one placed.
    const fills = (n: number) => Array.from({ length: n }, () => filled(0, 3840, 2000));
    expect(sizes(placing([1, 0, 0]), ...fills(2), filled(1, 4, 2), red)).toEqual([[4, 2]]);
    expect(sizes(placing([1, 0, 0]), ...fills(10), filled(1, 4, 2), red)).toEqual([]);
  });

  it("keeps a frame's worth of regions at most, of 4096 pixels a side at most", () => {
    // The second 3000×2000 region would pass a 4K screen's pixels with the first: it isn't kept.
    expect(sizes(placing([1, 0, 0]), filled(0, 3000, 2000), filled(1, 3000, 2000), red)).toEqual([]);
    // A region made again counts once.
    expect(sizes(placing([0, 0, 0]), filled(0, 3000, 2000), filled(0, 3840, 2160), red)).toEqual([[3840, 2160]]);
    expect(sizes(placing([0, 0, 0]), filled(0, 4096, 1), red)).toEqual([[4096, 1]]);
    expect(sizes(placing([0, 0, 0]), filled(0, 4097, 1), red)).toEqual([]);
  });
});

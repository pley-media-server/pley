/**
 * The Matroska files the server makes of a picture subtitle track
 * (`/api/files/{id}/subtitles/{n}.mks`, docs/API.md): one track of PGS,
 * VobSub or DVB, copied as it is, with the original's times from the start
 * of the file. Only what that needs is read: the track's codec and private
 * data, and every block with its time and duration.
 */

export interface PictureTrack {
  /** `S_HDMV/PGS`, `S_VOBSUB` or `S_DVBSUB`. */
  codecId: string;
  codecPrivate: Uint8Array | null;
  samples: PictureSample[];
}

export interface PictureSample {
  timeMs: number;
  /** From the block group; `null` when the sample shows until the next one. */
  durationMs: number | null;
  data: Uint8Array;
}

const ID = {
  EBML: 0x1a45dfa3,
  Segment: 0x18538067,
  Info: 0x1549a966,
  TimecodeScale: 0x2ad7b1,
  Tracks: 0x1654ae6b,
  TrackEntry: 0xae,
  TrackNumber: 0xd7,
  TrackType: 0x83,
  CodecID: 0x86,
  CodecPrivate: 0x63a2,
  ContentEncodings: 0x6d80,
  ContentEncoding: 0x6240,
  ContentCompression: 0x5034,
  ContentCompAlgo: 0x4254,
  ContentCompSettings: 0x4255,
  Cluster: 0x1f43b675,
  Timecode: 0xe7,
  SimpleBlock: 0xa3,
  BlockGroup: 0xa0,
  Block: 0xa1,
  BlockDuration: 0x9b,
} as const;

/** Elements whose children are read; everything else is skipped by its size. */
const MASTERS = new Set<number>([
  ID.Segment,
  ID.Info,
  ID.Tracks,
  ID.TrackEntry,
  ID.ContentEncodings,
  ID.ContentEncoding,
  ID.ContentCompression,
  ID.Cluster,
  ID.BlockGroup,
]);

/** The Segment's children: where an element of unknown size inside one ends. */
const LEVEL_ONE = new Set<number>([0x114d9b74, ID.Info, ID.Tracks, ID.Cluster, 0x1c53bb6b, 0x1043a770, 0x1254c367, 0x1941a469]);

const SUBTITLE_TRACK = 0x11;

/**
 * The samples read of a track: a film's subtitles have a few thousand, a
 * day's broadcast tens of thousands. A crafted file of tiny blocks could
 * hold millions, each kept as an object many times its size.
 */
export const MAX_SAMPLES = 100_000;

/**
 * How deep elements are read: the deepest this reader needs is 6 (a
 * track's compression), and a crafted file nesting thousands deep would
 * run the stack out.
 */
const MAX_DEPTH = 8;

/** The tracks read: the server's files have one. */
const MAX_TRACKS = 64;

export class MatroskaError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "MatroskaError";
  }
}

interface Track {
  number: number;
  type: number;
  codecId: string;
  codecPrivate: Uint8Array | null;
  /** Bytes stripped from the start of every frame (ContentCompAlgo 3). */
  stripped: Uint8Array | null;
  /** Compressed with something this reader can't undo (zlib). */
  compressed: boolean;
}

/**
 * Reads a `.mks` file, or as much of one as there is: a file cut short
 * gives the samples before the cut. Throws [MatroskaError] for anything
 * that isn't one.
 */
export function readPictureTrack(buffer: ArrayBuffer): PictureTrack {
  const bytes = new Uint8Array(buffer);
  const r = new Reader(bytes);
  const header = r.element(0, bytes.length);
  if (!header || header.id !== ID.EBML) throw new MatroskaError("not a Matroska file");

  let scaleNs = 1_000_000;
  const tracks: Track[] = [];
  const raw: { track: number; time: number; duration: number | null; data: Uint8Array }[] = [];
  let track: Track | null = null;
  let compAlgo = 0;
  let compSettings: Uint8Array | null = null;
  let clusterTime = 0;
  let groupBlock: { track: number; time: number; data: Uint8Array } | null = null;
  let groupDuration: number | null = null;

  const endGroup = () => {
    if (groupBlock && raw.length < MAX_SAMPLES) raw.push({ ...groupBlock, duration: groupDuration });
    groupBlock = null;
    groupDuration = null;
  };

  /** Reads the elements from `start` to `end`, `depth` deep; where it stopped (an unknown-size cluster ends at the next level-1 element). */
  const walk = (start: number, end: number, unknownSize: boolean, depth: number): number => {
    let at = start;
    while (at < end) {
      const el = r.element(at, end);
      if (!el) return end;
      if (unknownSize && LEVEL_ONE.has(el.id)) return at;
      // A file still being made (a partial answer, `X-Pley-Complete: 0`)
      // ends inside an element: a block cut short isn't read as a sample.
      if (el.size !== null && el.dataStart + el.size > bytes.length && !MASTERS.has(el.id)) return end;
      const dataEnd = el.size === null ? end : Math.min(end, el.dataStart + el.size);
      if (el.id === ID.Cluster) clusterTime = 0;
      if (el.id === ID.TrackEntry) {
        track = { number: 0, type: 0, codecId: "", codecPrivate: null, stripped: null, compressed: false };
      }
      if (el.id === ID.ContentCompression) {
        compAlgo = 0;
        compSettings = null;
      }
      let next = el.size === null ? end : el.dataStart + el.size;
      if (MASTERS.has(el.id)) {
        if (depth < MAX_DEPTH) {
          const stopped = walk(el.dataStart, dataEnd, el.size === null && el.id !== ID.Segment, depth + 1);
          if (el.size === null) next = stopped;
        }
      } else {
        const body = bytes.subarray(el.dataStart, dataEnd);
        switch (el.id) {
          case ID.TimecodeScale:
            scaleNs = uint(body) || 1_000_000;
            break;
          case ID.TrackNumber:
            if (track) track.number = uint(body);
            break;
          case ID.TrackType:
            if (track) track.type = uint(body);
            break;
          case ID.CodecID:
            if (track) track.codecId = ascii(body);
            break;
          case ID.CodecPrivate:
            if (track) track.codecPrivate = body.slice();
            break;
          case ID.ContentCompAlgo:
            compAlgo = uint(body);
            break;
          case ID.ContentCompSettings:
            compSettings = body.slice();
            break;
          case ID.Timecode:
            clusterTime = uint(body);
            break;
          case ID.SimpleBlock: {
            const b = raw.length < MAX_SAMPLES ? block(body, clusterTime) : null;
            if (b) raw.push({ ...b, duration: null });
            break;
          }
          case ID.Block:
            groupBlock = raw.length < MAX_SAMPLES ? block(body, clusterTime) : null;
            break;
          case ID.BlockDuration:
            groupDuration = uint(body);
            break;
        }
      }
      if (el.id === ID.ContentCompression && track) {
        const t: Track = track;
        if (compAlgo === 3) t.stripped = compSettings;
        else t.compressed = true;
      }
      if (el.id === ID.TrackEntry && track) {
        if (tracks.length < MAX_TRACKS) tracks.push(track);
        track = null;
      }
      if (el.id === ID.BlockGroup) endGroup();
      at = next;
    }
    return end;
  };

  const segmentStart = header.dataStart + (header.size ?? 0);
  walk(segmentStart, bytes.length, false, 0);

  const subtitle = tracks.filter((t) => t.type === SUBTITLE_TRACK)[0] ?? tracks[0];
  if (!subtitle) throw new MatroskaError("no track");
  if (subtitle.compressed) throw new MatroskaError("compressed subtitle frames");
  const toMs = (ticks: number) => (ticks * scaleNs) / 1_000_000;
  const samples: PictureSample[] = [];
  for (const b of raw) {
    if (b.track !== subtitle.number) continue;
    const data = subtitle.stripped ? concat(subtitle.stripped, b.data) : b.data;
    samples.push({ timeMs: Math.round(toMs(b.time)), durationMs: b.duration === null ? null : Math.round(toMs(b.duration)), data });
  }
  samples.sort((a, b) => a.timeMs - b.timeMs);
  return { codecId: subtitle.codecId, codecPrivate: subtitle.codecPrivate, samples };
}

/**
 * Where the Matroska file `bytes` start with ends, when its Segment's size
 * is written: the server writes its own files whole, FFmpeg writes the size
 * once it has finished one. `null` for a file still being written (its
 * Segment of unknown size), or what isn't one.
 */
export function matroskaEnd(bytes: Uint8Array): number | null {
  const r = new Reader(bytes);
  const header = r.element(0, bytes.length);
  if (!header || header.id !== ID.EBML || header.size === null) return null;
  const segment = r.element(header.dataStart + header.size, bytes.length);
  return segment?.id === ID.Segment && segment.size !== null ? segment.dataStart + segment.size : null;
}

/** A block's track, absolute time in ticks and its one frame (subtitles are never laced). */
function block(body: Uint8Array, clusterTime: number): { track: number; time: number; data: Uint8Array } | null {
  const v = vint(body, 0, true);
  if (!v || v.length + 3 > body.length) return null;
  const relative = (body[v.length]! << 8) | body[v.length + 1]!;
  const signed = relative & 0x8000 ? relative - 0x10000 : relative;
  const flags = body[v.length + 2]!;
  const lacing = (flags >> 1) & 3;
  let data = body.subarray(v.length + 3);
  if (lacing !== 0) {
    // Laced subtitle frames don't occur in practice: keep the first frame's bytes as they are.
    const frames = data[0]! + 1;
    if (frames < 1) return null;
    data = data.subarray(1);
  }
  return { track: v.value, time: clusterTime + signed, data: data.slice() };
}

interface Element {
  id: number;
  /** `null`: unknown size (to the end of the parent). */
  size: number | null;
  dataStart: number;
}

class Reader {
  private readonly bytes: Uint8Array;

  constructor(bytes: Uint8Array) {
    this.bytes = bytes;
  }

  element(at: number, end: number): Element | null {
    const id = vint(this.bytes, at, false);
    if (!id || at + id.length > end) return null;
    const size = vint(this.bytes, at + id.length, true);
    if (!size) return null;
    const dataStart = at + id.length + size.length;
    if (dataStart > end) return null;
    return { id: id.value, size: size.unknown ? null : size.value, dataStart };
  }
}

/**
 * An EBML variable-length integer at `at`: an element id keeps its length
 * marker (`stripMarker` false), a size or track number loses it.
 */
function vint(bytes: Uint8Array, at: number, stripMarker: boolean): { value: number; length: number; unknown: boolean } | null {
  const first = bytes[at];
  if (first === undefined || first === 0) return null;
  let length = 1;
  let mask = 0x80;
  while (!(first & mask)) {
    mask >>= 1;
    length++;
  }
  if (at + length > bytes.length) return null;
  let value = stripMarker ? first & (mask - 1) : first;
  let allOnes = (first & (mask - 1)) === mask - 1;
  for (let i = 1; i < length; i++) {
    const b = bytes[at + i]!;
    if (b !== 0xff) allOnes = false;
    value = value * 256 + b;
  }
  return { value, length, unknown: stripMarker && allOnes };
}

function uint(body: Uint8Array): number {
  let value = 0;
  for (let i = 0; i < body.length; i++) value = value * 256 + body[i]!;
  return value;
}

/** A codec id: 64 characters are far more than any has. */
function ascii(body: Uint8Array): string {
  let s = "";
  for (let i = 0; i < body.length && i < 64 && body[i] !== 0; i++) s += String.fromCharCode(body[i]!);
  return s;
}

function concat(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length + b.length);
  out.set(a, 0);
  out.set(b, a.length);
  return out;
}

import type { MediaFile } from "../api/types";

/**
 * Files this TV's player refused to seek in, played as they are, by their
 * container and video codec ("mkv/hevc"). A seek it refuses costs a try
 * (up to the engine's watchdog) before the remux that gets there, so once
 * it refused seeks in that kind of file on `SEEK_REFUSALS_TRUSTED` items, a
 * seek in another goes straight to the remux. One file alone (an index it
 * can't read) doesn't decide for the others. Watching without a seek still
 * plays the file as it is; a refused resume point isn't counted here.
 */
export interface SeekMemory {
  /** The TV's player refused a seek in item `itemId`, a file of `kind`. */
  refused(kind: string, itemId: number): void;
  /** Seeks in files of `kind` are refused here: go straight to a remux. */
  known(kind: string): boolean;
  /** The TV's other player (Samsung's <video>) took item `itemId`, a file of `kind`, at a seek this one refused. */
  seekedViaElement(kind: string, itemId: number): void;
  /** Seeks refused in files of `kind` go to the TV's other player first, rather than to a remux. */
  seeksViaElement(kind: string): boolean;
}

/** Where a kind's seeks by the TV's other player are kept, beside its refusals. */
const ELEMENT = "element:";

export const SEEK_REFUSALS_TRUSTED = 2;
/** What it learned is forgotten this long after: a firmware update or files made again may seek after all. */
export const SEEK_MEMORY_MS = 30 * 24 * 3600_000;

/** "mkv/hevc": what a refusal is remembered by; `null` when the file doesn't say. */
export function seekKind(file: MediaFile | null): string | null {
  if (!file?.container || !file.video_codec) return null;
  const container = file.container.toLowerCase().replace("matroska", "mkv").split(",")[0]!;
  return `${container}/${file.video_codec.toLowerCase()}`;
}

/** Each kind's items refused, and when the last of them was. */
type Refusals = Record<string, { items: number[]; at: number }>;

/** Kept in the app's storage under `key` (per TV); storage that fails forgets, and seeks are tried again. */
export function storedSeekMemory(
  store: { get(key: string): string | null; set(key: string, value: string): void },
  key: string,
  now: () => number = () => Date.now(),
): SeekMemory {
  const read = (): Refusals => {
    try {
      const v = JSON.parse(store.get(key) ?? "{}") as unknown;
      return v && typeof v === "object" ? (v as Refusals) : {};
    } catch {
      return {};
    }
  };
  /** The kind's refusals still remembered, or none. */
  const current = (all: Refusals, kind: string): number[] => {
    const r = all[kind];
    return r && Array.isArray(r.items) && typeof r.at === "number" && now() - r.at < SEEK_MEMORY_MS ? r.items : [];
  };
  return {
    refused(kind, itemId) {
      const all = read();
      const items = current(all, kind);
      if (items.indexOf(itemId) >= 0 || items.length >= SEEK_REFUSALS_TRUSTED) return;
      all[kind] = { items: items.concat(itemId), at: now() };
      try {
        store.set(key, JSON.stringify(all));
      } catch {
        // Storage full or blocked: the next item tries its seek first.
      }
    },
    known(kind) {
      return current(read(), kind).length >= SEEK_REFUSALS_TRUSTED;
    },
    seekedViaElement(kind, itemId) {
      const all = read();
      all[ELEMENT + kind] = { items: [itemId], at: now() };
      try {
        store.set(key, JSON.stringify(all));
      } catch {
        // Storage full or blocked: the next refusal tries the other player again.
      }
    },
    seeksViaElement(kind) {
      return current(read(), ELEMENT + kind).length > 0;
    },
  };
}

/**
 * How music reaches a place in a file the TV plays as it is: 1 moves the
 * <audio> element (`currentTime`), 2 loads the file again from the place
 * (`#t=`), 3 converts it from there on the server for the rest of the
 * track. Tizen 5's <audio> takes no move in a FLAC file it fetches by
 * range, and plays on where it was.
 */
export type AudioSeekStep = 1 | 2 | 3;

/**
 * Which step reached the place, by kind of audio file ("flac"), on this
 * TV: a seek in that kind starts there, the steps that failed skipped. A
 * plain move that lands forgets the kind.
 */
export interface AudioSeekMemory {
  /** Where a seek in a file of `kind` starts: 1 unless a later step is remembered. */
  step(kind: string): AudioSeekStep;
  /** Step `step` reached the place in a file of `kind`. */
  landed(kind: string, step: AudioSeekStep): void;
}

/** Where an audio kind is kept, apart from the video's refusals. */
const AUDIO = "audio:";

/** "flac": the audio file's extension in its address, which names what the TV plays; `null` when it has none. */
export function audioKind(url: string): string | null {
  const m = /\.([a-z0-9]{1,8})(?:[?#].*)?$/i.exec(url);
  return m ? m[1]!.toLowerCase() : null;
}

type AudioSteps = Record<string, { step: number; at: number }>;

/** Kept in the app's storage under `key` (per TV) for `SEEK_MEMORY_MS`; storage that fails forgets, and every step is tried. */
export function storedAudioSeekMemory(
  store: { get(key: string): string | null; set(key: string, value: string): void } | null | undefined,
  key: string,
  now: () => number = () => Date.now(),
): AudioSeekMemory {
  const read = (): AudioSteps => {
    try {
      const v = JSON.parse(store?.get(key) ?? "{}") as unknown;
      return v && typeof v === "object" && !Array.isArray(v) ? (v as AudioSteps) : {};
    } catch {
      return {};
    }
  };
  const write = (all: AudioSteps): void => {
    try {
      store?.set(key, JSON.stringify(all));
    } catch {
      // Storage full or blocked: the next seek tries every step again.
    }
  };
  return {
    step(kind) {
      const r = read()[AUDIO + kind];
      if (!r || typeof r.at !== "number" || now() - r.at >= SEEK_MEMORY_MS) return 1;
      return r.step === 2 || r.step === 3 ? r.step : 1;
    },
    landed(kind, step) {
      const all = read();
      let changed = false;
      // Kept by the step that worked, and dropped once a plain move works; kinds forgotten by now go with it.
      for (const k of Object.keys(all)) {
        const r = all[k];
        if (!r || typeof r.at !== "number" || now() - r.at >= SEEK_MEMORY_MS) {
          delete all[k];
          changed = true;
        }
      }
      if (step !== 1) {
        all[AUDIO + kind] = { step, at: now() };
        changed = true;
      } else if (AUDIO + kind in all) {
        delete all[AUDIO + kind];
        changed = true;
      }
      if (changed) write(all);
    },
  };
}

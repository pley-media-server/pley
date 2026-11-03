/**
 * Getting a new session's stream ready while the last one plays on (a
 * quality, audio or Automatic change, as the web player does): its
 * playlists and the segment it starts with are asked for, so the
 * transcoder makes them, and the TV's player moves to it once that segment
 * is there rather than waiting for it on a black screen.
 *
 * The TV's own player can't use what is fetched here (AVPlay has HTTP of
 * its own), so the segment is asked for with HEAD: the server answers once
 * it is made, and no picture crosses the network
 * twice, which matters most when Automatic steps down for a slow one.
 */

import type { Cancel } from "../net/transport";

/** How long a session prepared beside the one playing may take to get ready before the player moves to it anyway. */
export const PREPARE_MAX_MS = 20_000;
/** How far past where it starts a prepared session's first segment is asked for: where the player will be by then. */
export const PREPARE_LEAD_MS = 4_000;

/** A playlist (`text`) or a segment (`head`: answered once it is made), as the TV's player would ask for it. */
export interface StreamFetch {
  text(url: string, cancel: Cancel, timeoutMs: number): Promise<string>;
  head(url: string, cancel: Cancel, timeoutMs: number): Promise<void>;
}

/** `ref` against `base`, as a playlist names its playlists and segments. */
export function resolve(ref: string, base: string): string {
  return new URL(ref, base).href;
}

/** The sizes a multivariant playlist lists, in its order; none for a media playlist. */
export function variantsOf(master: string, base: string): string[] {
  const lines = master.split(/\r?\n/).map((l) => l.trim());
  const found: string[] = [];
  for (let i = 0; i < lines.length; i++) {
    if (lines[i]!.indexOf("#EXT-X-STREAM-INF") !== 0) continue;
    for (let j = i + 1; j < lines.length; j++) {
      const uri = lines[j]!;
      if (!uri || uri.charAt(0) === "#") continue;
      found.push(resolve(uri, base));
      break;
    }
  }
  return found;
}

/** The segment of a media playlist that holds `ms`: the last one for a time past its end; `null` for none. */
export function segmentAt(playlist: string, base: string, ms: number): string | null {
  let at = 0;
  let duration: number | null = null;
  let last: string | null = null;
  for (const raw of playlist.split(/\r?\n/)) {
    const line = raw.trim();
    if (line.indexOf("#EXTINF:") === 0) {
      duration = parseFloat(line.slice("#EXTINF:".length)) * 1000;
    } else if (line && line.charAt(0) !== "#" && duration !== null) {
      last = resolve(line, base);
      if (ms < at + duration) return last;
      at += duration;
      duration = null;
    }
  }
  return last;
}

/**
 * Asks for the stream at `url` as the TV's player starting at `startMs`
 * would: its playlist, the size it lists first of a stream in several
 * (where the TV's player starts), and the segment that holds `startMs`.
 * Resolves once that segment is made, after `PREPARE_MAX_MS` at most, when
 * `cancel` is cancelled, or when anything fails: the player moves anyway,
 * and finds out for itself.
 */
export function warmStream(fetch: StreamFetch, url: string, startMs: number, cancel: Cancel, now: () => number = Date.now): Promise<void> {
  const until = now() + PREPARE_MAX_MS;
  const left = () => Math.max(1, until - now());
  const timer = setTimeout(() => cancel.cancel(), PREPARE_MAX_MS);
  // Cancelled (a newer change, 20 s gone): done, whether or not a request still runs.
  const stop = new Promise<never>((_, reject) => cancel.onCancel(() => reject(new Error("cancelled"))));
  stop.catch(() => undefined);
  const step = <T>(p: Promise<T>): Promise<T> => Promise.race([p, stop]);
  const run = async () => {
    let playlistUrl = url;
    let playlist = await step(fetch.text(playlistUrl, cancel, left()));
    const sizes = variantsOf(playlist, playlistUrl);
    if (sizes.length) {
      playlistUrl = sizes[0]!;
      playlist = await step(fetch.text(playlistUrl, cancel, left()));
    }
    const segment = segmentAt(playlist, playlistUrl, startMs);
    if (segment) await step(fetch.head(segment, cancel, left()));
  };
  return run().then(
    () => clearTimeout(timer),
    () => clearTimeout(timer),
  );
}

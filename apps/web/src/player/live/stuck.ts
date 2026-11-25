/**
 * A channel played from its stream that stands still for good while the
 * stream goes on coming. mpegts.js drops sound whose timestamps are behind
 * what it already has, and puts such pictures where they never play: a
 * provider whose stream starts its clock over (as the server connects to it
 * again, after it ended or dropped the stream) is never heard or seen again.
 * A new player, reading the stream again from its last keyframe, plays it.
 */
export interface Stuck {
  /** Seconds the picture stands still with nothing more buffered… */
  after: number;
  /** …the stream coming for this many of them: not a provider gone quiet, which the cushion waits for. */
  coming: number;
}

export const STUCK: Stuck = { after: 8, coming: 5 };

/** How often it looks, ms. */
const EVERY = 1000;

/** Where what's buffered ends: its last range's end. */
function bufferedEnd(video: HTMLMediaElement): number {
  const b = video.buffered;
  return b.length ? b.end(b.length - 1) : 0;
}

/**
 * Watches `video` for standing still while its stream comes (`coming`: some
 * came just now), and tells `onStuck` once, with whether it played while
 * watched: returns what stops it.
 */
export function whenStuck(
  video: HTMLMediaElement,
  coming: () => boolean,
  onStuck: (played: boolean) => void,
  stuck: Stuck = STUCK,
): () => void {
  let time = video.currentTime;
  let end = bufferedEnd(video);
  let movedBefore = false;
  let played = false;
  let still = 0;
  let came = 0;
  let last = performance.now();
  const check = () => {
    const now = performance.now();
    // A page in the background may be looked at less often: no more than a look's worth at once.
    const seconds = Math.min(now - last, EVERY * 1.5) / 1000;
    last = now;
    const t = video.currentTime;
    const e = bufferedEnd(video);
    // Forward only: a new player's video starts again at 0.
    const moved = t > time;
    const more = e > end;
    time = t;
    end = e;
    // Played: on from one look to the next, not a jump (mpegts.js seeks
    // to what's buffered as it starts).
    played ||= moved && movedBefore;
    movedBefore = moved;
    // A page in the background that hasn't played isn't counted: the
    // browser may not load its video until it's shown.
    if (moved || more || (document.hidden && !played)) {
      still = 0;
      came = 0;
      return;
    }
    still += seconds;
    const comingNow = coming();
    if (comingNow) came += seconds;
    if (still >= stuck.after && came >= stuck.coming && comingNow) {
      stop();
      onStuck(played);
    }
  };
  const timer = window.setInterval(check, EVERY);
  const stop = () => window.clearInterval(timer);
  return stop;
}

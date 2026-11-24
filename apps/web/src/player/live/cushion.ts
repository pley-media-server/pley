/**
 * A cushion for a channel played from its stream as it comes. Providers
 * send in bursts, and a player a second behind live stalls at every gap
 * between them; one a few seconds behind plays through. Playback starts
 * once `start` seconds are buffered, and after a stall it waits for `again`
 * seconds, so the next gaps of that length play through. It waits by a
 * playback rate of 0, which neither fires `pause` (Live TV plays on after
 * one) nor is undone by mpegts.js's live sync.
 */
export interface Cushion {
  /** Seconds buffered before playback starts… */
  start: number;
  /** …and before it goes on after a stall. */
  again: number;
  /** The longest it waits for either, ms: a channel sent slower than it plays starts anyway. */
  wait: number;
}

export const CUSHION: Cushion = { start: 2, again: 4, wait: 6000 };

/** Seconds of the picture buffered past what plays. */
export function bufferedAhead(video: HTMLMediaElement): number {
  const b = video.buffered;
  for (let i = 0; i < b.length; i++) {
    if (b.start(i) <= video.currentTime + 0.1 && video.currentTime <= b.end(i)) return b.end(i) - video.currentTime;
  }
  return 0;
}

/**
 * Plays `video` (`play`) once the cushion is there, and keeps it, telling
 * `onHold` when a stall waits for it and when it goes on: returns what
 * stops it, the playback rate left at 1.
 */
export function keepCushion(
  video: HTMLMediaElement,
  play: () => void,
  onHold: (holding: boolean) => void = () => {},
  cushion: Cushion = CUSHION,
): () => void {
  let started = false;
  let since = performance.now();
  let holding = true;
  const check = () => {
    if (!holding) return;
    const enough = bufferedAhead(video) >= (started ? cushion.again : cushion.start);
    if (!enough && performance.now() - since < cushion.wait) return;
    holding = false;
    if (started) {
      video.playbackRate = 1;
      onHold(false);
    } else {
      started = true;
      play();
    }
  };
  const stalled = () => {
    if (!started || holding) return;
    holding = true;
    since = performance.now();
    video.playbackRate = 0;
    onHold(true);
  };
  const timer = window.setInterval(check, 200);
  video.addEventListener("waiting", stalled);
  check();
  return () => {
    window.clearInterval(timer);
    video.removeEventListener("waiting", stalled);
    if (video.playbackRate === 0) video.playbackRate = 1;
    // Stopped twice (a stream that broke off, then the page that let it go): told once.
    if (holding && started) onHold(false);
    holding = false;
  };
}

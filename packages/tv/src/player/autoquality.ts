/**
 * Automatic quality (docs/QUALITY.md): the original, unless it doesn't
 * play well; then the quality that does. The player tells it what it sees
 * (waits for data, how fast video arrives, how fast the network loads,
 * a decoder falling behind) and starts again at the quality it names.
 * Nothing here knows the player or the clock: times come in, in ms.
 *
 * The web player's (apps/web/src/player/autoQuality.ts), with what the TVs
 * add since their players go black between streams (`blanks`, "Players
 * that go black between streams"): a converted stream's bitrate changes as
 * it is sent (`LiveStream`), and the original moves only once its picture
 * has stopped.
 */

export const ORIGINAL_ID = "original";

/** A wait for data this long is a play issue; two shorter ones within a minute are one too. */
export const LONG_WAIT_MS = 2_000;
export const SHORT_WAITS_WITHIN_MS = 60_000;
/**
 * The original as the file is moves for a wait only where the network, over
 * the last 2 minutes, isn't shown to carry this many times its bitrate: on a
 * network that does, the server is why it waits (its disk, a remux catching
 * up), which a conversion only slows down.
 */
export const ORIGINAL_NETWORK = 1.5;
export const ORIGINAL_NETWORK_MS = 2 * 60_000;
/** Where nothing measured the network, the original moves at the second such wait within this. */
export const ORIGINAL_WAITS_WITHIN_MS = 5 * 60_000;
/** Delivery is measured over this much time, once there is at least the second figure of it. */
export const DELIVERY_WINDOW_MS = 20_000;
export const DELIVERY_MIN_SPAN_MS = 5_000;
/** Video arriving slower than 1.1 s a second, with under 10 s ahead and falling, for 15 s: a play issue before it waits. */
export const DELIVERY_LOW = 1.1;
export const DELIVERY_LOW_FOR_MS = 15_000;
export const DELIVERY_AHEAD_S = 10;
/** The new ceiling keeps a quarter in hand. */
export const SAFETY = 0.75;
/** No change comes sooner than this after the last. */
export const CHANGE_GAP_MS = 20_000;
/** A step back up needs this long without an issue, and this long since the last step down. */
export const CALM_MS = 3 * 60_000;
export const AFTER_DOWN_MS = 5 * 60_000;
/** The network must carry this many times the next quality up. */
export const HEADROOM = 1.6;
/** A step up a step down undoes this soon was one too far: it isn't tried again during the title. */
export const SETTLE_MS = 10 * 60_000;
/** Each step up undone doubles the calm the next one waits for, up to this. */
export const MAX_CALM_MS = 4 * CALM_MS;
/** On a player that goes black, frames the original drops move it in this much of a title's playing only. */
export const FIRST_MINUTE_MS = 60_000;
/** A slow network is remembered this long, per device and server. */
export const MEMORY_MS = 30 * 60_000;
/** Network figures older than this go. */
const KEEP_MS = 6 * 60_000;

/** A quality Automatic can play below the original: highest first, each lower than the one before. */
export interface Rung {
  id: string;
  label: string;
  kbps: number;
  height: number;
}

/** Why Automatic went below the original. */
export type Lowered = "network" | "device";

/** A change Automatic decided on: the player starts again at `to`, or its stream goes on at `bitrate`. */
export interface AutoStep {
  /** "original" or a rung's id. */
  to: string;
  /** Why it went down; `null` for a step back up. */
  why: Lowered | null;
  /**
   * A stream whose bitrate changes as it is sent (`LiveStream`): it goes
   * on at this many kbps, the same stream and size, and `to` is where it
   * plays still. Left out for a new stream.
   */
  bitrate?: number;
}

/**
 * A converted stream whose bitrate changes as it is sent
 * (`PlaybackInfo.live_bitrate`, `PUT /api/playback/{id}/bitrate`): its own
 * bitrate, the most it goes back up to, and the least its size keeps
 * (8 Mbps for 4K, 3 for 1080p), kbps.
 */
export interface LiveStream {
  top: number;
  floor: number;
}

/** What the network carried when it last stepped down (kbps), and when (ms since 1970). */
export interface Remembered {
  kbps: number;
  at: number;
}

export interface AutoMemory {
  get(): Remembered | null;
  set(value: Remembered | null): void;
}

/** A memory kept in a key-value store under `key` (one per device and server). */
export function storedMemory(store: { get(key: string): string | null; set(key: string, value: string): void; remove(key: string): void }, key: string): AutoMemory {
  return {
    get() {
      try {
        const text = store.get(key);
        const v = text ? (JSON.parse(text) as Remembered) : null;
        return v && typeof v.kbps === "number" && typeof v.at === "number" ? v : null;
      } catch {
        return null;
      }
    },
    set(value) {
      try {
        if (value) store.set(key, JSON.stringify(value));
        else store.remove(key);
      } catch {
        // Storage full or blocked: the next title starts at the original.
      }
    },
  };
}

export interface AutoOptions {
  /** The qualities below the original it may play (see `Rung`). */
  ladder: Rung[];
  /** What the original plays at, kbps: the file's bitrate, or the server's quality when it converts it; `null` when unknown. */
  originalKbps: number | null;
  memory?: AutoMemory | null;
  /** The time the memory is kept in (ms since 1970); `Date.now` unless a test says. */
  wall?: () => number;
  /**
   * The remembered ceiling changed (kbps; `null`: forgotten), for the
   * server to keep for this network too (docs/QUALITY.md).
   */
  onCeiling?: (kbps: number | null) => void;
  /**
   * The player goes black between streams (Samsung's AVPlay): it never
   * moves to another stream while one plays well. A stream whose bitrate
   * changes as it is sent steps that; any other moves
   * only once its picture has stopped (a wait of 2 s or more, a decoder
   * failing), and never back up.
   */
  blanks?: boolean;
}

/**
 * One title's Automatic: where it plays now, and when to change. Created
 * for each title; `start` says where it begins.
 */
export class AutoQuality {
  /** "original" or a rung's id. */
  playing = ORIGINAL_ID;
  /** Why it plays below the original; `null` at the original. */
  lowered: Lowered | null = null;
  /** The bitrate a stream whose bitrate changes as it is sent goes on at (kbps); `null` for any other stream. */
  liveKbps: number | null = null;

  private readonly ladder: Rung[];
  private originalKbps: number | null;
  /** The original plays converted (`setOriginalKbps`), not as the file is. */
  private converted = false;
  private readonly memory: AutoMemory | null;
  private readonly wall: () => number;
  private readonly onCeiling: ((kbps: number | null) => void) | null;
  private readonly blanks: boolean;
  private begun = false;

  private lastChange = -1e15;
  private lastDown = -1e15;
  private lastIssue = -1e15;
  private pending: Lowered | null = null;
  /** The last step up, until a step down or `SETTLE_MS` says whether it held. */
  private lastUp: { at: number; to: string } | null = null;
  /** Qualities a step down undid soon after a step up to them: not tried again this title. */
  private readonly tooHigh = new Set<string>();
  /** The calm a step up waits for: `CALM_MS`, doubled by each step up undone. */
  private calmMs = CALM_MS;

  /** The stream playing, when its bitrate changes as it is sent; and the quality it was asked at. */
  private live: LiveStream | null = null;
  private liveOf: string | null = null;
  /** Where the last bitrate step started from, until the server says it took it. */
  private liveUndo: { kbps: number; lowered: Lowered | null } | null = null;
  /** Frames dropped where it doesn't move have lowered where the next title starts. */
  private struggleKept = false;

  private waitFrom: number | null = null;
  /** The wait under way lasted 2 s (`longWait`): what's left of it is no short wait. */
  private waitLong = false;
  /** The wait under way came on a network nothing measured, and is among `originalWaits`. */
  private waitUnmeasured = false;
  /** When the original's long waits came on a network nothing measured. */
  private originalWaits: number[] = [];
  private shortWaits: number[] = [];
  private fills: { at: number; end: number }[] = [];
  /** The last delivery measured, seconds of video a second; `null` while unknown. */
  private delivery: number | null = null;
  private lowFrom: number | null = null;
  private lowAhead = 0;
  private loads: { at: number; bytes: number; ms: number }[] = [];
  private rates: { at: number; kbps: number }[] = [];
  /** What the network carried at least, from the buffer filling faster than it plays. */
  private floors: { at: number; kbps: number }[] = [];

  constructor(o: AutoOptions) {
    this.ladder = o.ladder;
    this.originalKbps = o.originalKbps && o.originalKbps > 0 ? o.originalKbps : null;
    this.memory = o.memory ?? null;
    this.wall = o.wall ?? (() => Date.now());
    this.onCeiling = o.onCeiling ?? null;
    this.blanks = !!o.blanks;
  }

  /** Whether `start` has said where it begins. */
  get started(): boolean {
    return this.begun;
  }

  /**
   * Where it starts: the original, or below it when a network too slow for
   * it was found in the last 30 minutes. `now` is the player's clock.
   * `known`: what the server knows of this network instead (a ceiling, or
   * `null` for none), which keeps its own time; left out when it can't say.
   */
  start(now: number, known?: Remembered | null): string {
    this.begun = true;
    this.lastIssue = now;
    const fromServer = known !== undefined;
    const kept = fromServer ? known : this.memory ? this.memory.get() : null;
    // A clock set back a little since doesn't make it older than now.
    const age = kept ? Math.max(0, this.wall() - kept.at) : 0;
    if (kept && !fromServer && (age >= MEMORY_MS || kept.at - this.wall() >= MEMORY_MS)) this.memory!.set(null);
    else if (kept && this.ladder.length && (this.originalKbps === null || kept.kbps < this.originalKbps)) {
      this.playing = this.highestAtMost(kept.kbps, null)!.id;
      this.lowered = "network";
      this.lastDown = now - age;
    }
    return this.playing;
  }

  /**
   * On a player that goes black, a stream whose bitrate can't change as it
   * plays (the original, a remux, an older server's conversion): only a
   * stopped picture moves it.
   */
  get patient(): boolean {
    return this.blanks && this.live === null;
  }

  /** On a player that goes black, the original as the file is (direct play or a remux): a wait moves it only where the network is to blame (`longWait`). */
  private get asTheFile(): boolean {
    return this.patient && this.playing === ORIGINAL_ID && !this.converted;
  }

  /** The stream playing goes on below its own bitrate. */
  get liveLowered(): boolean {
    return this.live !== null && this.liveKbps !== null && this.liveKbps < this.live.top;
  }

  /** The own bitrate of the stream playing, when it changes as it is sent (kbps). */
  get liveTop(): number | null {
    return this.live ? this.live.top : null;
  }

  /** The bitrate playing now (kbps); `null` for an original of unknown bitrate. */
  playingKbps(): number | null {
    if (this.live && this.liveKbps !== null) return this.liveKbps;
    const rung = this.rung(this.playing);
    return rung ? rung.kbps : this.originalKbps;
  }

  /** The original plays converted (a fallback of the player's, or a file it can't play), at `kbps`. */
  setOriginalKbps(kbps: number | null): void {
    this.converted = true;
    if (kbps && kbps > 0) this.originalKbps = kbps;
  }

  /**
   * A new session began (a change, a recovery): what was measured of the
   * one before goes, but the network's speed. `live`: its bitrate changes
   * as it is sent. A new stream of the same quality (a recovery, a track)
   * goes on at the bitrate the last one had come to: `liveKbps`, for the
   * player to ask for when below its own.
   */
  began(live: LiveStream | null = null): void {
    this.waitFrom = null;
    this.shortWaits = [];
    this.pending = null;
    this.interrupted();
    const kept = live && this.live && this.liveKbps !== null && this.liveOf === this.playing && this.live.top === live.top ? this.liveKbps : null;
    this.live = live;
    this.liveOf = live ? this.playing : null;
    this.liveKbps = live ? (kept === null ? live.top : Math.max(live.floor, Math.min(live.top, kept))) : null;
    this.liveUndo = null;
  }

  /**
   * The server says what the stream goes on at (kbps) after the step that
   * asked for `asked`. Less than asked: the stream's own bitrate is lower
   * than its quality's, and that is as high as it goes.
   */
  liveAt(kbps: number, asked: number): void {
    this.liveUndo = null;
    const live = this.live;
    if (!live || !(kbps > 0)) return;
    if (kbps < asked) this.live = { top: Math.min(live.top, kbps), floor: Math.min(live.floor, kbps) };
    this.liveKbps = kbps;
    if (this.playing === ORIGINAL_ID && !this.liveLowered) this.lowered = null;
  }

  /**
   * The server didn't change the bitrate: the stream goes on as it was.
   * `final` (it can't, an older transcoder): from now on it is a stream
   * whose bitrate can't change.
   */
  liveRefused(final: boolean): void {
    const undo = this.liveUndo;
    this.liveUndo = null;
    if (undo && this.live) {
      this.liveKbps = undo.kbps;
      this.lowered = undo.lowered;
    }
    if (final) {
      this.live = null;
      this.liveOf = null;
      this.liveKbps = null;
    }
  }

  /** Paused, seeking, or out of sight: what it was measuring starts over, and a wait under way isn't one. */
  interrupted(): void {
    this.waitFrom = null;
    this.waitLong = false;
    this.waitUnmeasured = false;
    this.fills = [];
    this.delivery = null;
    this.lowFrom = null;
  }

  /** The picture stopped for want of data while meant to play (not after a seek, a pause or a new session). */
  waiting(now: number): void {
    if (this.waitFrom !== null) return;
    this.waitFrom = now;
    this.waitLong = false;
    this.waitUnmeasured = false;
  }

  /**
   * Whether the wait under way, 2 s long by `now`, is a play issue. At the
   * original as the file is, only on a network not measured at
   * `ORIGINAL_NETWORK` times its bitrate; where nothing measured the
   * network, the second such wait within 5 minutes. Asked again as the wait
   * goes on, with what the network showed since.
   */
  private longWait(now: number): boolean {
    this.waitLong = true;
    if (!this.asTheFile) return true;
    const net = this.network(now - ORIGINAL_NETWORK_MS);
    if (net !== null && this.originalKbps !== null) return net < ORIGINAL_NETWORK * this.originalKbps;
    if (!this.waitUnmeasured) {
      this.waitUnmeasured = true;
      this.originalWaits = this.originalWaits.filter((t) => now - t < ORIGINAL_WAITS_WITHIN_MS);
      this.originalWaits.push(now);
    }
    return this.originalWaits.length >= 2;
  }

  /** It plays on after a wait. */
  resumed(now: number): void {
    const from = this.waitFrom;
    if (from === null) return;
    this.waitFrom = null;
    if (now - from >= LONG_WAIT_MS && this.longWait(now)) this.pending = this.pending || "network";
    const long = this.waitLong;
    this.waitLong = false;
    this.waitUnmeasured = false;
    // Short waits don't move a stream that can't change as it plays, on a player that goes black.
    if (long || this.patient) return;
    const recent = this.shortWaits.filter((t) => now - t < SHORT_WAITS_WITHIN_MS);
    recent.push(now);
    if (recent.length >= 2) {
      this.pending = this.pending || "network";
      this.shortWaits = [];
    } else {
      this.shortWaits = recent;
    }
  }

  /**
   * Where the playhead is and where what's loaded ahead of it ends, in
   * seconds, while it plays; `bufferedEnd` `null` when the player can't say.
   */
  sample(now: number, position: number, bufferedEnd: number | null): void {
    if (bufferedEnd === null || !isFinite(bufferedEnd)) return;
    const last = this.fills[this.fills.length - 1];
    if (last && bufferedEnd < last.end - 0.5) this.fills = [];
    this.fills.push({ at: now, end: bufferedEnd });
    while (this.fills.length > 2 && now - this.fills[1]!.at >= DELIVERY_WINDOW_MS) this.fills.shift();
    const first = this.fills[0]!;
    const span = now - first.at;
    if (span < DELIVERY_MIN_SPAN_MS) return;
    const delivery = Math.max(0, (bufferedEnd - first.end) / (span / 1000));
    this.delivery = delivery;
    const ahead = Math.max(0, bufferedEnd - position);
    const kbps = this.playingKbps();
    // Filling faster than it plays: the network carries at least that much.
    if (delivery > 1 && kbps !== null) this.floor(now, delivery * kbps);
    if (delivery < DELIVERY_LOW && ahead < DELIVERY_AHEAD_S) {
      if (this.lowFrom === null) {
        this.lowFrom = now;
        this.lowAhead = ahead;
      } else if (now - this.lowFrom >= DELIVERY_LOW_FOR_MS) {
        // Nor does slow delivery: the picture hasn't stopped.
        if (ahead < this.lowAhead && !this.patient) this.pending = this.pending || "network";
        this.lowFrom = now;
        this.lowAhead = ahead;
      }
    } else {
      this.lowFrom = null;
    }
  }

  /** A download of `bytes` took `ms` (an HLS segment). */
  loaded(now: number, bytes: number, ms: number): void {
    if (bytes > 0 && ms > 0) this.loads.push({ at: now, bytes, ms });
    this.trim(now);
  }

  /** The player measured its network at `kbps`. */
  bandwidth(now: number, kbps: number): void {
    if (kbps > 0) this.rates.push({ at: now, kbps });
    this.trim(now);
  }

  /**
   * The network carried at least `kbps` (the buffer filled that much faster
   * than the video plays). A full buffer fills only as fast as it plays, so
   * most of these are far below what the network carries: their highest
   * says the most, where a measurement's median would say the least (LG's
   * TVs, whose players tell nothing else of their network).
   */
  private floor(now: number, kbps: number): void {
    this.floors.push({ at: now, kbps });
    this.trim(now);
  }

  /** The decoder fell behind (frames dropped, a decoder failing) while it converts. */
  decoding(): void {
    this.pending = "device";
  }

  /**
   * Frames dropped where Automatic doesn't move for them (a player that
   * goes black, past the title's first minute): this title plays on, and
   * the next starts a step lower, through the network's memory. Once a title.
   */
  struggled(): void {
    if (this.struggleKept) return;
    const kbps = this.playingKbps();
    const ceiling = kbps === null ? null : Math.round(kbps * SAFETY);
    const target = this.highestAtMost(ceiling, kbps);
    if (ceiling === null || !target) return;
    this.struggleKept = true;
    this.remember(Math.max(ceiling, target.kbps));
  }

  /** The player itself turned the original into a conversion (it couldn't play it well): a change, as one of its own. */
  fellBack(now: number): void {
    this.lastChange = now;
    this.lastIssue = now;
    this.pending = null;
  }

  /**
   * How fast the network carried data since `since`, kbps: its downloads,
   * the player's own figures (their median), or at least the most a refill
   * of the buffer showed; `null` when nothing says.
   */
  network(since: number): number | null {
    let bytes = 0;
    let ms = 0;
    let count = 0;
    for (const l of this.loads) {
      if (l.at < since) continue;
      bytes += l.bytes;
      ms += l.ms;
      count++;
    }
    let best: number | null = count >= 2 && ms > 0 ? (bytes * 8) / ms : null;
    const said = this.rates.filter((r) => r.at >= since).map((r) => r.kbps);
    if (said.length) {
      said.sort((a, b) => a - b);
      const median = said[Math.floor(said.length / 2)]!;
      if (best === null || median > best) best = median;
    }
    for (const f of this.floors) if (f.at >= since && (best === null || f.kbps > best)) best = f.kbps;
    return best;
  }

  /**
   * What to do now: a step down after a play issue, a step back up after a
   * calm spell, or nothing. The step is taken here; the player starts again
   * at `to`.
   */
  check(now: number): AutoStep | null {
    if (this.waitFrom !== null && now - this.waitFrom >= LONG_WAIT_MS && this.longWait(now)) {
      this.pending = this.pending || "network";
      // Waiting on: another 2 s is another issue.
      this.waitFrom = now;
    }
    const issue = this.pending;
    this.pending = null;
    if (issue) {
      this.lastIssue = now;
      if (now - this.lastChange < CHANGE_GAP_MS) return null;
      return this.live ? this.liveDown(now, issue) : this.stepDown(now, issue);
    }
    if (this.live) return this.liveUp(now);
    // A player that goes black doesn't leave a stream that plays well for another.
    return this.blanks ? null : this.stepUp(now);
  }

  private stepDown(now: number, why: Lowered): AutoStep | null {
    const kbps = this.playingKbps();
    let ceiling: number | null = null;
    if (kbps !== null) {
      let delivery = 1;
      if (why === "network") {
        const net = this.network(now - DELIVERY_WINDOW_MS);
        if (this.delivery !== null) delivery = this.delivery;
        else if (net !== null) delivery = net / kbps;
        delivery = Math.min(1, delivery);
      }
      ceiling = Math.round(kbps * delivery * SAFETY);
    } else {
      // An original of unknown bitrate: what the network carries, when it says.
      const net = this.network(now - DELIVERY_WINDOW_MS);
      if (net !== null) ceiling = Math.round(net * SAFETY);
    }
    const target = this.highestAtMost(ceiling, kbps);
    if (!target || target.id === this.playing) return null;
    // Down again from where it just stepped up to: that was one step too far.
    if (this.lastUp && this.lastUp.to === this.playing && now - this.lastUp.at < SETTLE_MS) {
      this.tooHigh.add(this.playing);
      this.calmMs = Math.min(MAX_CALM_MS, this.calmMs * 2);
    }
    this.lastUp = null;
    this.playing = target.id;
    this.lowered = why;
    this.lastChange = now;
    this.lastDown = now;
    this.interrupted();
    if (why === "network") this.remember(ceiling !== null ? Math.max(ceiling, target.kbps) : target.kbps);
    return { to: target.id, why };
  }

  private stepUp(now: number): AutoStep | null {
    if (this.playing === ORIGINAL_ID) return null;
    const calmFrom = Math.max(this.lastIssue, this.lastChange);
    if (now - calmFrom < this.calmMs || now - this.lastDown < AFTER_DOWN_MS) return null;
    const at = this.indexOf(this.playing);
    const next = at > 0 ? this.ladder[at - 1]! : null;
    // Settled: the step above didn't hold this title.
    if (this.tooHigh.has(next ? next.id : ORIGINAL_ID)) return null;
    const need = next ? next.kbps : this.originalKbps;
    const net = this.network(calmFrom);
    if (need === null || net === null || net < HEADROOM * need) return null;
    this.playing = next ? next.id : ORIGINAL_ID;
    this.lastChange = now;
    this.lastUp = { at: now, to: this.playing };
    this.interrupted();
    if (!next) {
      this.lowered = null;
      this.remember(null);
    } else if (this.lowered === "network") {
      // The network carries this one now: the next title starts here rather than lower.
      this.remember(next.kbps);
    }
    return { to: this.playing, why: null };
  }

  /**
   * A step down of a stream whose bitrate changes as it is sent: the same
   * ceiling as a new stream's, in whole Mbps, within the least its size
   * keeps and its own bitrate. At that least already, the network waits.
   */
  private liveDown(now: number, why: Lowered): AutoStep | null {
    const live = this.live!;
    const kbps = this.liveKbps ?? live.top;
    let delivery = 1;
    if (why === "network") {
      const net = this.network(now - DELIVERY_WINDOW_MS);
      if (this.delivery !== null) delivery = this.delivery;
      else if (net !== null) delivery = net / kbps;
      delivery = Math.min(1, delivery);
    }
    const ceiling = Math.round(kbps * delivery * SAFETY);
    const to = Math.max(live.floor, Math.min(live.top, Math.floor(ceiling / 1000) * 1000));
    if (to >= kbps) return null;
    // Down again from where it just stepped up to: that was one step too far.
    if (this.lastUp && this.lastUp.to === this.place() && now - this.lastUp.at < SETTLE_MS) {
      this.tooHigh.add(this.place());
      this.calmMs = Math.min(MAX_CALM_MS, this.calmMs * 2);
    }
    this.lastUp = null;
    this.liveUndo = { kbps, lowered: this.lowered };
    this.liveKbps = to;
    this.lowered = why;
    this.lastChange = now;
    this.lastDown = now;
    this.interrupted();
    // What the network carries, though the stream keeps more for its size: the next title starts by it.
    if (why === "network") this.remember(ceiling);
    return { to: this.playing, why, bitrate: to };
  }

  /**
   * A step back up of a stream whose bitrate changes as it is sent, on
   * `stepUp`'s rules: to the next of the file's qualities' bitrates above
   * it, or to its own.
   */
  private liveUp(now: number): AutoStep | null {
    const live = this.live!;
    const kbps = this.liveKbps ?? live.top;
    if (kbps >= live.top) return null;
    const calmFrom = Math.max(this.lastIssue, this.lastChange);
    if (now - calmFrom < this.calmMs || now - this.lastDown < AFTER_DOWN_MS) return null;
    let next = live.top;
    for (const r of this.ladder) if (r.kbps > kbps && r.kbps < next) next = r.kbps;
    // Settled: the step above didn't hold this title.
    if (this.tooHigh.has(this.place(next))) return null;
    const net = this.network(calmFrom);
    if (net === null || net < HEADROOM * next) return null;
    this.liveUndo = { kbps, lowered: this.lowered };
    this.liveKbps = next;
    this.lastChange = now;
    this.lastUp = { at: now, to: this.place() };
    this.interrupted();
    if (next >= live.top && this.playing === ORIGINAL_ID) {
      // The original as this player gets it: a step back up to the original.
      this.lowered = null;
      this.remember(null);
    } else if (this.lowered === "network") {
      this.remember(next);
    }
    return { to: this.playing, why: null, bitrate: next };
  }

  /** Where it plays, as steps up are told apart: the quality, and the bitrate (`kbps`, else now's) of a stream that changes as it is sent. */
  private place(kbps: number | null = this.liveKbps): string {
    return this.live && kbps !== null ? `${this.playing}@${kbps}` : this.playing;
  }

  /** Remembers `kbps` as the network's ceiling (`null`: forgets it), here and on the server. */
  private remember(kbps: number | null): void {
    this.memory?.set(kbps === null ? null : { kbps, at: this.wall() });
    this.onCeiling?.(kbps);
  }

  /**
   * The highest rung at or below `ceiling` kbps (any, when `null`) and
   * below `under` kbps when given: at least one step down. The lowest rung
   * when none is that low; `null` when there is no rung below `under`.
   */
  private highestAtMost(ceiling: number | null, under: number | null): Rung | null {
    const below = this.ladder.filter((r) => under === null || r.kbps < under);
    const fits = below.filter((r) => ceiling === null || r.kbps <= ceiling);
    return fits[0] ?? below[below.length - 1] ?? null;
  }

  private rung(id: string): Rung | null {
    return this.ladder.filter((r) => r.id === id)[0] ?? null;
  }

  private indexOf(id: string): number {
    for (let i = 0; i < this.ladder.length; i++) if (this.ladder[i]!.id === id) return i;
    return -1;
  }

  private trim(now: number): void {
    while (this.loads.length && now - this.loads[0]!.at > KEEP_MS) this.loads.shift();
    while (this.rates.length && now - this.rates[0]!.at > KEEP_MS) this.rates.shift();
    while (this.floors.length && now - this.floors[0]!.at > KEEP_MS) this.floors.shift();
  }
}

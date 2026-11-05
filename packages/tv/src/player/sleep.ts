import type { ItemKind } from "../api/types";
import { t } from "../i18n";

/** What the sleep timer is set to. Not remembered between playbacks. */
export type SleepChoice = { kind: "off" } | { kind: "minutes"; minutes: number } | { kind: "end_of_item" };

/** The menu's choices, in order. */
export const SLEEP_CHOICES: SleepChoice[] = [
  { kind: "off" },
  { kind: "minutes", minutes: 15 },
  { kind: "minutes", minutes: 30 },
  { kind: "minutes", minutes: 45 },
  { kind: "minutes", minutes: 60 },
  { kind: "minutes", minutes: 90 },
  { kind: "end_of_item" },
];

/** "Off", "30 min", "End of this episode" (of this movie, track or video, by what plays), as on the web. */
export function sleepLabel(c: SleepChoice, playing: ItemKind | null = "episode"): string {
  if (c.kind === "off") return t("player.sleep.off");
  if (c.kind === "minutes") return t("player.sleep.minutes", { minutes: c.minutes });
  if (playing === "episode") return t("player.sleep.end.episode");
  if (playing === "movie") return t("player.sleep.end.movie");
  return playing === "track" ? t("player.sleep.end.track") : t("player.sleep.end.video");
}

export function sameChoice(a: SleepChoice, b: SleepChoice): boolean {
  return a.kind === b.kind && (a.kind !== "minutes" || a.minutes === (b as { minutes: number }).minutes);
}

export interface SleepState {
  choice: SleepChoice;
  /** Seconds before it pauses, during the notice (its last 30 s); `null` otherwise. */
  secondsLeft: number | null;
  /** Minutes before it pauses, rounded up, for the menu; `null` when off or unknown. */
  minutesLeft: number | null;
  /** It paused the playback, which hasn't played since. */
  fired: boolean;
}

export const SLEEP_OFF: SleepState = { choice: { kind: "off" }, secondsLeft: null, minutesLeft: null, fired: false };

/** The notice shows this long before the timer pauses. */
export const NOTICE_MS = 30_000;
/** Paused by the timer this long, the player leaves. */
export const LEAVE_AFTER_MS = 10 * 60_000;
const SECOND_MS = 1_000;
const MINUTE_MS = 60_000;

export interface SleepClock {
  now(): number;
  setTimeout(fn: () => void, ms: number): unknown;
  clearTimeout(handle: unknown): void;
}

const realClock: SleepClock = {
  now: () => (typeof performance !== "undefined" ? performance.now() : Date.now()),
  setTimeout: (fn, ms) => setTimeout(fn, ms),
  clearTimeout: (h) => clearTimeout(h as ReturnType<typeof setTimeout>),
};

/**
 * The sleep timer (the Android app's SleepTimer): after the minutes chosen,
 * or at the end of what plays, it pauses (`onFire`). 30 s before, the
 * state says so once a second for a notice; `extend` ("Keep watching")
 * adds the same time again. Minutes keep running while paused.
 */
export class SleepTimer {
  state: SleepState = SLEEP_OFF;
  private timer: unknown = null;
  private deadline = 0;
  /** "Keep watching" at the end of this item: the timer waits for the next one's end. */
  private spared: unknown = null;
  private readonly itemLeftMs: () => number | null;
  private readonly itemKey: () => unknown;
  private readonly onFire: () => void;
  private readonly onChange: (s: SleepState) => void;
  private readonly clock: SleepClock;

  constructor(opts: {
    itemLeftMs: () => number | null;
    itemKey: () => unknown;
    onFire: () => void;
    onChange: (s: SleepState) => void;
    clock?: SleepClock;
  }) {
    this.itemLeftMs = opts.itemLeftMs;
    this.itemKey = opts.itemKey;
    this.onFire = opts.onFire;
    this.onChange = opts.onChange;
    this.clock = opts.clock ?? realClock;
  }

  private publish(next: SleepState): void {
    this.state = next;
    this.onChange(next);
  }

  set(choice: SleepChoice): void {
    this.stop();
    this.spared = null;
    this.publish({ ...SLEEP_OFF, choice });
    if (choice.kind === "off") return;
    if (choice.kind === "minutes") this.deadline = this.clock.now() + choice.minutes * MINUTE_MS;
    this.tick();
  }

  /** Adds the same time again: the minutes chosen, or until the end of the next item. */
  extend(): void {
    const choice = this.state.choice;
    if (choice.kind === "off") return;
    if (choice.kind === "minutes") this.deadline += choice.minutes * MINUTE_MS;
    else this.spared = this.itemKey();
    this.publish({ ...this.state, secondsLeft: null });
    this.stop();
    this.tick();
  }

  /** Whether the timer ends with the item `key`: what comes next waits. */
  endsWith(key: unknown): boolean {
    return this.state.choice.kind === "end_of_item" && key !== this.spared;
  }

  /** The item `key` played to its end: true when that is the timer's end, which then fires. */
  itemEnded(key: unknown): boolean {
    if (!this.endsWith(key)) return false;
    this.fire();
    return true;
  }

  /** It plays again: the pause the timer made is over. */
  resumed(): void {
    if (this.state.fired) this.publish({ ...this.state, fired: false });
  }

  cancel(): void {
    this.stop();
    this.publish(SLEEP_OFF);
  }

  private stop(): void {
    if (this.timer !== null) this.clock.clearTimeout(this.timer);
    this.timer = null;
  }

  private tick(): void {
    this.timer = null;
    const choice = this.state.choice;
    if (choice.kind === "off") return;
    const key = this.itemKey();
    const left = choice.kind === "minutes" ? this.deadline - this.clock.now() : key === this.spared ? null : this.itemLeftMs();
    if (this.spared !== null && key !== this.spared) this.spared = null;
    if (left === null) this.set2(null, null);
    else if (choice.kind === "minutes" && left <= 0) {
      this.fire();
      return;
    } else if (left <= 0) this.set2(null, null);
    else if (left <= NOTICE_MS) this.set2(Math.ceil(left / SECOND_MS), 1);
    else this.set2(null, Math.ceil(left / MINUTE_MS));
    this.timer = this.clock.setTimeout(() => this.tick(), this.wait(choice, left));
  }

  private set2(secondsLeft: number | null, minutesLeft: number | null): void {
    if (this.state.secondsLeft === secondsLeft && this.state.minutesLeft === minutesLeft) return;
    this.publish({ ...this.state, secondsLeft, minutesLeft });
  }

  /** Until the next second during the notice; else until the notice or the next minute, polling an item's end each second. */
  private wait(choice: SleepChoice, left: number | null): number {
    if (left === null || left <= 0 || choice.kind === "end_of_item") return SECOND_MS;
    if (left <= NOTICE_MS) return left % SECOND_MS || SECOND_MS;
    return Math.min(left - NOTICE_MS, left % MINUTE_MS || MINUTE_MS);
  }

  private fire(): void {
    this.stop();
    this.spared = null;
    this.publish({ ...SLEEP_OFF, fired: true });
    this.onFire();
  }
}

/**
 * The remote's keys, by meaning. Each platform maps its key codes onto
 * these (Tizen: tizen.tvinputdevice; webOS: LG's codes), so the screens
 * never see a number.
 */
export type Key =
  | "left"
  | "right"
  | "up"
  | "down"
  | "ok"
  | "back"
  /** The context menu: a tile's actions. */
  | "menu"
  | "info"
  | "playPause"
  | "play"
  | "pause"
  | "stop"
  | "fastForward"
  | "rewind"
  | "next"
  | "previous"
  | "captions"
  | "audio"
  /** CH+ and CH−: Live TV's next and previous channel. */
  | "channelUp"
  | "channelDown"
  /** The remote's channel list and guide keys (Samsung's CH LIST and GUIDE). */
  | "channelList"
  | "guide"
  | "red"
  | "green"
  | "yellow"
  | "blue"
  | "backspace"
  /** "." on a keyboard or a numpad: the address field types it. */
  | "period"
  | `digit${number}`;

export function isDirection(k: Key | null): k is "left" | "right" | "up" | "down" {
  return k === "left" || k === "right" || k === "up" || k === "down";
}

/** Keys every keyboard and remote sends with standard codes. */
export function standardKey(e: KeyboardEvent): Key | null {
  switch (e.keyCode) {
    case 37:
      return "left";
    case 38:
      return "up";
    case 39:
      return "right";
    case 40:
      return "down";
    case 13:
      return "ok";
    case 8:
    case 46:
      return "backspace";
    case 110:
    case 190:
      return "period";
    case 27:
      return "back";
    case 93:
      return "menu";
    case 179:
      return "playPause";
    case 415:
      return "play";
    case 19:
      return "pause";
    case 413:
      return "stop";
    case 417:
      return "fastForward";
    case 412:
      return "rewind";
    case 176:
      return "next";
    case 177:
      return "previous";
    case 403:
      return "red";
    case 404:
      return "green";
    case 405:
      return "yellow";
    case 406:
      return "blue";
    case 457:
      return "info";
    // LG's CH+ and CH− (and a keyboard's Page Up and Page Down).
    case 33:
      return "channelUp";
    case 34:
      return "channelDown";
    default:
      if (e.keyCode >= 48 && e.keyCode <= 57) return `digit${e.keyCode - 48}`;
      if (e.keyCode >= 96 && e.keyCode <= 105) return `digit${e.keyCode - 96}`;
      // Keyboards that leave keyCode out name the key instead.
      if (e.key === "." || e.key === "Decimal") return "period";
      if (e.key && e.key.length === 1 && e.key >= "0" && e.key <= "9") return `digit${Number(e.key)}`;
      return null;
  }
}

/**
 * The Android app's KeyRepeatGate: while a direction key repeats, at most
 * one move every 100 ms goes through (with the TV's own repeat every
 * 50 ms, every other one); focus has rested once 400 ms passed with no
 * direction key held and no move. A lost key-up ends the hold after 500 ms.
 */
export class KeyRepeatGate {
  static readonly MOVE_INTERVAL = 100;
  static readonly REST = 400;
  static readonly LOST_KEY_UP = 500;

  holding = false;
  private lastMove = -1e9;
  private lastKey = -1e9;
  private readonly now: () => number;

  constructor(now: () => number = () => (typeof performance !== "undefined" ? performance.now() : Date.now())) {
    this.now = now;
  }

  /** Whether a direction key event goes through. */
  admit(type: "down" | "up", repeat: boolean): boolean {
    const now = this.now();
    if (type === "up") {
      this.holding = false;
      this.lastMove = now;
      return true;
    }
    this.lastKey = now;
    if (!repeat) {
      this.holding = false;
      this.lastMove = now;
      return true;
    }
    this.holding = true;
    if (now - this.lastMove < KeyRepeatGate.MOVE_INTERVAL) return false;
    this.lastMove = now;
    return true;
  }

  private expireHold(): void {
    if (this.holding && this.now() - this.lastKey > KeyRepeatGate.LOST_KEY_UP) {
      this.holding = false;
      this.lastMove = this.lastKey;
    }
  }

  rested(): boolean {
    this.expireHold();
    return !this.holding && this.now() - this.lastMove >= KeyRepeatGate.REST;
  }

  /** Milliseconds until focus counts as rested (0 when it has); polls while a key is held. */
  untilRest(): number {
    this.expireHold();
    if (this.holding) return 50;
    return Math.max(0, this.lastMove + KeyRepeatGate.REST - this.now());
  }
}

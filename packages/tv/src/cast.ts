/**
 * Being cast to (docs/API.md, "Casting to another device"): a phone or a
 * browser of the profile sends this TV a movie, an episode or an extra,
 * then pauses, seeks and stops it, and sees what it plays. The TV is
 * listed while its event stream is open, plays what it is sent as if it
 * had been picked here, and says what its video player does. TVs don't
 * cast themselves. The native Samsung app has the same rules
 * (fixtures/cast.json).
 */

/** What a sender asks of the player. */
export type CastAction = "pause" | "resume" | "seek" | "stop";

/** How this TV is listed for casting: an id it makes once and keeps, its name, its platform. */
export interface CastIdentity {
  id: string;
  name: string;
  platform: "samsung" | "lg";
}

/** A command for this TV, read from its event stream. */
export type CastCommand =
  /** `positionMs` null: where the profile left off, without asking. */
  | { kind: "play"; itemId: number; fileId: number | null; positionMs: number | null }
  /** `positionMs` is a seek's target, and null for the others. */
  | { kind: "control"; action: CastAction; positionMs: number | null };

/** What the video player is doing, as the cast report reads it. */
export interface CastView {
  itemId: number;
  phase: "preparing" | "buffering" | "playing" | "paused" | "ended" | "error";
  positionMs: number;
  durationMs: number;
  /** The title played to its end (Up Next may show): it plays nothing now. */
  ended: boolean;
}

/** The body of PUT /api/cast/devices/{id}/state. */
export type CastStateBody =
  | { item_id: null }
  | { item_id: number; position_ms: number; duration_ms: number | null; state: "playing" | "paused" | "buffering" };

/** While a video plays, what it plays is said this often. */
export const CAST_REPORT_MS = 10_000;
/** Positions and lengths the server takes: up to a week. */
const MAX_MS = 7 * 24 * 3600 * 1000;
const ACTIONS: CastAction[] = ["pause", "resume", "seek", "stop"];

/** The name and platform this app ("tizen" or "webos") is listed with. */
export function castIdentity(app: "tizen" | "webos", id: string): CastIdentity {
  return app === "tizen" ? { id, name: "Samsung TV", platform: "samsung" } : { id, name: "LG TV", platform: "lg" };
}

/** A new device id: 32 hex digits, random. */
export function newCastId(): string {
  const bytes = new Uint8Array(16);
  try {
    crypto.getRandomValues(bytes);
  } catch {
    for (let i = 0; i < bytes.length; i++) bytes[i] = Math.floor(Math.random() * 256);
  }
  return Array.prototype.map.call(bytes, (b: number) => (b < 16 ? "0" : "") + b.toString(16)).join("");
}

/** The event stream's query that lists the TV for casting while it is open; "" without. */
export function castQuery(cast: CastIdentity | null): string {
  if (!cast) return "";
  return `?cast_device=${encodeURIComponent(cast.id)}&cast_name=${encodeURIComponent(cast.name)}&cast_platform=${encodeURIComponent(cast.platform)}`;
}

function id(v: unknown): number | null {
  return typeof v === "number" && Number.isInteger(v) && v > 0 ? v : null;
}

/** A position: missing or null is none (`null`); anything but a whole number from 0 is unreadable (`undefined`). */
function position(v: unknown): number | null | undefined {
  if (v === undefined || v === null) return null;
  return typeof v === "number" && Number.isInteger(v) && v >= 0 ? v : undefined;
}

/**
 * The command `e` gives the TV listed as `deviceId`; `null` for any other
 * event, another device's, or one that lacks what its kind needs (a seek
 * without its position).
 */
export function castCommand(e: unknown, deviceId: string): CastCommand | null {
  if (!e || typeof e !== "object") return null;
  const o = e as Record<string, unknown>;
  if ((o.type !== "cast_play" && o.type !== "cast_control") || o.device_id !== deviceId) return null;
  const at = position(o.position_ms);
  if (at === undefined) return null;
  if (o.type === "cast_play") {
    const itemId = id(o.item_id);
    if (itemId === null) return null;
    let fileId: number | null = null;
    if (o.file_id !== undefined && o.file_id !== null) {
      fileId = id(o.file_id);
      if (fileId === null) return null;
    }
    return { kind: "play", itemId, fileId, positionMs: at };
  }
  const action = typeof o.action === "string" ? ACTIONS.filter((a) => a === o.action)[0] : undefined;
  if (!action || (action === "seek" && at === null)) return null;
  return { kind: "control", action, positionMs: action === "seek" ? at : null };
}

function ms(v: number): number {
  return Math.min(MAX_MS, Math.max(0, Math.round(v)));
}

/**
 * What the TV says it plays: the item, where it is, how long it is and
 * whether it plays, waits or is paused; `{ item_id: null }` when it plays
 * nothing (no player, the title ended, an error).
 */
export function castStateBody(view: CastView | null): CastStateBody {
  if (!view || view.ended || view.itemId <= 0 || view.phase === "ended" || view.phase === "error") return { item_id: null };
  return {
    item_id: view.itemId,
    position_ms: isFinite(view.positionMs) ? ms(view.positionMs) : 0,
    duration_ms: isFinite(view.durationMs) && view.durationMs > 0 ? ms(view.durationMs) : null,
    state: view.phase === "playing" ? "playing" : view.phase === "paused" ? "paused" : "buffering",
  };
}

/**
 * Whether `next` says something `last` (what was said last; `null`
 * before anything, as nothing playing) didn't: another item, or another
 * state. A position moving on is said every 10 s, and after a seek.
 */
export function castReportDue(last: CastStateBody | null, next: CastStateBody): boolean {
  const was = last ?? { item_id: null };
  if (was.item_id !== next.item_id) return true;
  return was.item_id !== null && next.item_id !== null && was.state !== next.state;
}

/**
 * The reports of one video player (PUT /api/cast/devices/{id}/state): as
 * it starts (buffering, then playing), pauses, resumes, waits and seeks,
 * every 10 s while it plays, and nothing playing as it closes or the title
 * ends; the next episode is a new report. Best effort: `send` drops what
 * fails.
 */
export class CastReporter {
  private last: CastStateBody | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private closed = false;
  private readonly view: () => CastView | null;
  private readonly send: (body: CastStateBody) => void;

  constructor(view: () => CastView | null, send: (body: CastStateBody) => void) {
    this.view = view;
    this.send = send;
  }

  /** The player changed: said when it plays another item, or plays, waits or pauses now. */
  changed(): void {
    if (this.closed) return;
    const next = castStateBody(this.view());
    if (castReportDue(this.last, next)) this.say(next);
  }

  /** Said at once: after a seek, or when the event stream connects again (the server knows nothing of it then). */
  now(): void {
    if (!this.closed) this.say(castStateBody(this.view()));
  }

  /** The player closed: it plays nothing, unless that was said already. */
  close(): void {
    if (this.closed) return;
    this.closed = true;
    this.disarm();
    if (this.last && this.last.item_id !== null) this.send({ item_id: null });
    this.last = { item_id: null };
  }

  private say(body: CastStateBody): void {
    this.last = body;
    this.send(body);
    this.disarm();
    if (body.item_id !== null && body.state === "playing") {
      this.timer = setTimeout(() => {
        this.timer = null;
        this.now();
      }, CAST_REPORT_MS);
    }
  }

  private disarm(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }
}

import type { Tokens } from "../auth/tokens";
import { castQuery, type CastIdentity } from "../cast";
import type { ServerEvent } from "./types";

/** First wait after a stream fails, doubled each time up to the last. */
export const FIRST_RETRY_MS = 2_000;
export const MAX_RETRY_MS = 30_000;
/** A stream that ran this long and then ended ended as planned (its token expired): it reconnects at once. */
export const STABLE_MS = 30_000;

/**
 * The server's events (`GET /api/events`) over XMLHttpRequest: EventSource
 * can't send the Authorization header, which is the only place pley takes
 * an app's token. Reconnects as the Android app's EventStream does; tells
 * `onReconnect` on every connection, since nothing is replayed, and
 * `onDown` whenever it isn't connected (until it is again), so what
 * follows the events can ask instead meanwhile. With `cast`, the TV is
 * listed for casting while the stream is open; a server that refuses it
 * (`400`, `403`, `429`) gets the stream without it until the next start.
 */
export class EventStream {
  private xhr: XMLHttpRequest | null = null;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private failures = 0;
  private stopped = true;
  /** The server refused the TV as a device to cast to: the stream goes on without it until the next start. */
  private castRefused = false;
  /** The stream open now lists the TV for casting. */
  private castOpen = false;
  private readonly server: string;
  private readonly tokens: Tokens;
  private readonly onEvent: (e: ServerEvent) => void;
  private readonly onReconnect: () => void;
  private readonly onDown: () => void;
  private readonly cast: CastIdentity | null;

  constructor(
    server: string,
    tokens: Tokens,
    onEvent: (e: ServerEvent) => void,
    onReconnect: () => void,
    onDown: () => void = () => undefined,
    cast: CastIdentity | null = null,
  ) {
    this.server = server;
    this.tokens = tokens;
    this.onEvent = onEvent;
    this.onReconnect = onReconnect;
    this.onDown = onDown;
    this.cast = cast;
  }

  /** A stream is open that lists the TV for casting: what it plays is worth saying. */
  get casting(): boolean {
    return this.castOpen;
  }

  start(): void {
    if (!this.stopped) return;
    this.stopped = false;
    this.castRefused = false;
    void this.connect();
  }

  stop(): void {
    this.stopped = true;
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
    this.xhr?.abort();
    this.xhr = null;
  }

  /** Another profile signed in: a stream that runs starts over with its token, so the last one's events end here. */
  restart(): void {
    if (this.stopped) return;
    this.stop();
    this.failures = 0;
    this.start();
  }

  /** The server answers again: a stream waiting out its back-off connects now. False when none was waiting. */
  retryNow(): boolean {
    if (this.stopped || this.xhr || this.timer === null) return false;
    clearTimeout(this.timer);
    this.timer = null;
    this.failures = 0;
    void this.connect();
    return true;
  }

  private retryDelay(): number {
    let wait = FIRST_RETRY_MS;
    for (let i = 1; i < this.failures; i++) wait = Math.min(wait * 2, MAX_RETRY_MS);
    return Math.min(wait, MAX_RETRY_MS);
  }

  private later(ms: number): void {
    if (this.stopped) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.connect();
    }, ms);
  }

  private async connect(): Promise<void> {
    if (this.stopped) return;
    let token: string | null;
    try {
      token = await this.tokens.accessToken();
    } catch {
      token = null;
    }
    if (!token || this.stopped) {
      if (!this.stopped) this.onDown();
      this.failures++;
      this.later(this.retryDelay());
      return;
    }
    const xhr = new XMLHttpRequest();
    this.xhr = xhr;
    let read = 0;
    let buffer = "";
    let openedAt = -1;
    const cast = this.castRefused ? null : this.cast;
    xhr.open("GET", `${this.server}/api/events${castQuery(cast)}`, true);
    xhr.setRequestHeader("Accept", "text/event-stream");
    xhr.setRequestHeader("Authorization", `Bearer ${token}`);
    xhr.onreadystatechange = () => {
      if (xhr.readyState === 2 && xhr.status === 200 && openedAt < 0) {
        openedAt = Date.now();
        this.failures = 0;
        this.castOpen = cast !== null;
        this.onReconnect();
      }
    };
    xhr.onprogress = () => {
      const text = xhr.responseText;
      buffer += text.slice(read);
      read = text.length;
      const events = buffer.split(/\r?\n\r?\n/);
      buffer = events.pop() ?? "";
      for (const block of events) {
        const data = block
          .split(/\r?\n/)
          .filter((l) => l.indexOf("data:") === 0)
          .map((l) => l.slice(5).replace(/^ /, ""))
          .join("\n");
        if (!data) continue;
        try {
          const e = JSON.parse(data) as ServerEvent;
          if (e && typeof e.type === "string") this.onEvent(e);
        } catch {
          // Not an event this app reads.
        }
      }
      // A stream that runs for hours: start a fresh one rather than keep its whole text.
      if (read > 4 * 1024 * 1024) xhr.abort();
    };
    const ended = () => {
      if (this.xhr !== xhr) return;
      this.xhr = null;
      this.castOpen = false;
      if (this.stopped) return;
      this.onDown();
      // Refused as a device to cast to (an id or name it doesn't take, a token without `play`, too many devices): the events still come.
      if (cast && openedAt < 0 && (xhr.status === 400 || xhr.status === 403 || xhr.status === 429)) {
        this.castRefused = true;
        this.later(0);
        return;
      }
      if (xhr.status === 401) {
        this.tokens.refresh(token).then(
          () => this.later(0),
          () => {
            this.failures++;
            this.later(this.retryDelay());
          },
        );
        return;
      }
      if (xhr.status === 403) return;
      if (openedAt >= 0 && Date.now() - openedAt >= STABLE_MS) {
        this.later(0);
        return;
      }
      this.failures++;
      this.later(this.retryDelay());
    };
    xhr.onload = ended;
    xhr.onerror = () => {
      // Nothing answered: the server proves itself again before the next token.
      this.tokens.doubtServer();
      ended();
    };
    xhr.onabort = ended;
    xhr.send();
  }
}

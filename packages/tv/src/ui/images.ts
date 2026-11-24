import { ApiError, type Api } from "../api/client";

/** The widths the server's resizer keeps (apps/android core/common ImageUrls). */
export const WIDTH_BUCKETS = [160, 240, 320, 480, 640, 960, 1280, 1920];

/** The bucket for `px`: the smallest at least that wide, at most `cap`. */
export function bucket(px: number, cap = 1920): number {
  const limit = WIDTH_BUCKETS.filter((b) => b <= cap).pop() ?? WIDTH_BUCKETS[0]!;
  for (const b of WIDTH_BUCKETS) if (b >= px) return Math.min(b, limit);
  return limit;
}

/** `path` with `w=` set to `width`, any `w=` before replaced and other parameters (`v=`) kept. */
export function sized(path: string, width: number): string {
  const [base, query = ""] = path.split("?");
  const params = query.split("&").filter((p) => p && p.indexOf("w=") !== 0);
  params.push(`w=${width}`);
  return `${base}?${params.join("&")}`;
}

interface Entry {
  url: string;
  objectUrl: string | null;
  failed: boolean;
  refs: number;
  waiting: ((objectUrl: string | null) => void)[];
  lastUsed: number;
  loading: boolean;
  priority: number;
}

/**
 * Pictures from the server, which take the app's token only in the
 * Authorization header: fetched as blobs and shown from object URLs. Kept
 * while shown, and the most recent others besides; fetched a few at a
 * time, nearest first; addresses that answered 404 aren't asked again.
 */
export class ImageLoader {
  private readonly api: Api;
  private readonly entries = new Map<string, Entry>();
  private readonly missing = new Set<string>();
  private readonly queue: Entry[] = [];
  private active = 0;
  private readonly concurrency: number;
  private readonly keep: number;
  /** The screen's widest bucket: no picture is asked for wider. */
  readonly cap: number;

  constructor(api: Api, opts: { concurrency?: number; keep?: number; cap?: number } = {}) {
    this.api = api;
    this.concurrency = opts.concurrency ?? 6;
    this.keep = opts.keep ?? 160;
    this.cap = opts.cap ?? 1920;
  }

  /** The address of `path` for a picture `px` wide on screen. */
  url(path: string, px: number, cap = this.cap): string {
    return this.api.url(sized(path, bucket(px, Math.min(cap, this.cap))));
  }

  isMissing(url: string): boolean {
    return this.missing.has(url);
  }

  /** The object URL of `url` when it is loaded already (shown at once, without a fade). */
  ready(url: string): string | null {
    return this.entries.get(url)?.objectUrl ?? null;
  }

  /**
   * Loads `url` and calls `done` with its object URL (`null` when it can't
   * be had). The picture is kept until the returned release is called.
   */
  load(url: string, done: (objectUrl: string | null) => void, priority = 0): () => void {
    if (this.missing.has(url)) {
      done(null);
      return () => {};
    }
    let e = this.entries.get(url);
    if (!e) {
      e = { url, objectUrl: null, failed: false, refs: 0, waiting: [], lastUsed: Date.now(), loading: false, priority };
      this.entries.set(url, e);
    }
    e.refs++;
    e.lastUsed = Date.now();
    e.priority = Math.min(e.priority, priority);
    let released = false;
    const callback = (u: string | null) => {
      if (!released) done(u);
    };
    if (e.objectUrl || e.failed) callback(e.objectUrl);
    else {
      e.waiting.push(callback);
      if (!e.loading && this.queue.indexOf(e) < 0) this.queue.push(e);
      this.pump();
    }
    const entry = e;
    return () => {
      if (released) return;
      released = true;
      entry.refs--;
      entry.lastUsed = Date.now();
      const i = entry.waiting.indexOf(callback);
      if (i >= 0) entry.waiting.splice(i, 1);
      if (entry.refs <= 0 && !entry.loading && !entry.objectUrl) {
        // Not needed any more before it started: drop it from the queue.
        const q = this.queue.indexOf(entry);
        if (q >= 0) this.queue.splice(q, 1);
        this.entries.delete(entry.url);
      }
      this.trim();
    };
  }

  /** Fetches `url` ahead of need (the HTTP cache keeps it). */
  prefetch(url: string): void {
    const release = this.load(url, () => release(), 10);
  }

  private pump(): void {
    while (this.active < this.concurrency && this.queue.length) {
      this.queue.sort((a, b) => a.priority - b.priority);
      const e = this.queue.shift()!;
      if (e.refs <= 0) {
        this.entries.delete(e.url);
        continue;
      }
      this.active++;
      e.loading = true;
      this.api
        .send(e.url, { responseType: "blob", headers: { Accept: "image/*" }, timeoutMs: 30_000 })
        .then(
          (res) => {
            e.objectUrl = URL.createObjectURL(res.body as Blob);
          },
          (err: unknown) => {
            e.failed = true;
            if (err instanceof ApiError && (err.status === 404 || err.status === 410)) this.missing.add(e.url);
            // Other failures may pass: forget it so the next showing asks again.
            if (!(err instanceof ApiError && err.status === 404)) this.entries.delete(e.url);
          },
        )
        .then(() => {
          this.active--;
          e.loading = false;
          const waiting = e.waiting;
          e.waiting = [];
          for (const w of waiting) w(e.objectUrl);
          this.trim();
          this.pump();
        });
    }
  }

  /** Revokes the least recently used pictures nobody shows, beyond `keep`. */
  private trim(): void {
    if (this.entries.size <= this.keep) return;
    const idle: Entry[] = [];
    this.entries.forEach((e) => {
      if (e.refs <= 0 && !e.loading) idle.push(e);
    });
    idle.sort((a, b) => a.lastUsed - b.lastUsed);
    let excess = this.entries.size - this.keep;
    for (const e of idle) {
      if (excess-- <= 0) break;
      if (e.objectUrl) URL.revokeObjectURL(e.objectUrl);
      this.entries.delete(e.url);
    }
  }
}

/** A title's own gradient for its picture's place until it loads (packages/placeholder, as every app draws it). */
export { placeholderGradient } from "pley-placeholder";

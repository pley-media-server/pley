import { signal } from "@preact/signals";
import type { LiveChannel, LiveChannelPage } from "../api/types";
import { channelStep } from "../player/remote";
import { browseQuery, CHANNEL_PAGE, type LiveCategory } from "./data";

/** What the pages need of the server (the Api; tests give their own). */
export interface LiveBrowser {
  liveBrowse(query: { category: "all" | "favorites" | "recent" | "group"; group?: string; q?: string; offset?: number; limit?: number; around?: number }): Promise<LiveChannelPage>;
}

/** A page asked for this long ago is asked for again as it shows: what's on moves on. */
const STALE_MS = 60_000;
/** A page that failed waits this long before it is asked for again. */
const RETRY_MS = 10_000;

interface Page {
  channels: LiveChannel[] | null;
  at: number;
  pending: boolean;
}

/**
 * A category's channels, a page of 100 at a time as they come into view
 * (the web's useChannelList): a lineup of tens of thousands costs what's
 * on screen. Knows where a channel is in it (`indexOf`, by the server's
 * `around`), and the channel before or after one, round the ends (channel
 * up and down within the category).
 */
export class ChannelPages {
  readonly category: LiveCategory;
  /** Channels in the whole list; `null` until the first page is in. */
  readonly total = signal<number | null>(null);
  /** Bumped as pages come in. */
  readonly version = signal(0);
  /** The first page failed (and none came since). */
  readonly error = signal<unknown>(null);
  private readonly api: LiveBrowser;
  private readonly pages = new Map<number, Page>();
  private readonly waiting = new Map<number, Promise<void>>();
  /** Pages refreshed only when asked for again: the player's list keeps the order it opened with. */
  private readonly frozen: boolean;

  constructor(api: LiveBrowser, category: LiveCategory, opts: { frozen?: boolean } = {}) {
    this.api = api;
    this.category = category;
    this.frozen = !!opts.frozen;
  }

  /** The channel at `index`, once its page is in. */
  at(index: number): LiveChannel | undefined {
    return this.pages.get(Math.floor(index / CHANNEL_PAGE))?.channels?.[index % CHANNEL_PAGE];
  }

  /** Where channel `id` is among the pages in, if it is. */
  find(id: number): number | null {
    let found: number | null = null;
    this.pages.forEach((p, n) => {
      if (found !== null || !p.channels) return;
      const i = p.channels.findIndex((c) => c.id === id);
      if (i >= 0) found = n * CHANNEL_PAGE + i;
    });
    return found;
  }

  /** Asks for the pages covering `from` to `to` (and the first, which says how many there are) that it doesn't have fresh. */
  want(from: number, to: number): void {
    const total = this.total.value;
    const last = total === null ? 0 : Math.max(0, Math.ceil(total / CHANNEL_PAGE) - 1);
    const first = Math.min(last, Math.max(0, Math.floor(from / CHANNEL_PAGE)));
    const end = Math.min(last, Math.max(first, Math.floor(Math.max(from, to) / CHANNEL_PAGE)));
    void this.load(0);
    for (let n = first; n <= end; n++) void this.load(n);
  }

  /** Page `n`, asked for unless it is fresh or on its way. */
  load(n: number): Promise<void> {
    const p = this.pages.get(n);
    const now = Date.now();
    const fresh = p && (p.channels ? this.frozen || now - p.at < STALE_MS : now - p.at < RETRY_MS);
    if (p && (p.pending || fresh)) return this.waiting.get(n) ?? Promise.resolve();
    const page: Page = { channels: p?.channels ?? null, at: now, pending: true };
    this.pages.set(n, page);
    const run = this.ask({ offset: n * CHANNEL_PAGE }).then(
      (answer) => {
        page.pending = false;
        page.at = Date.now();
        this.keep(answer);
        if (n === 0) this.error.value = null;
      },
      (e: unknown) => {
        page.pending = false;
        page.at = Date.now();
        if (n === 0 && this.total.value === null) this.error.value = e;
        this.version.value++;
      },
    );
    this.waiting.set(n, run);
    void run.then(() => this.waiting.delete(n));
    return run;
  }

  /** Where channel `id` is in the whole list (the page holding it comes with the answer); `null` when it isn't in it. */
  async indexOf(id: number): Promise<number | null> {
    const known = this.find(id);
    if (known !== null) return known;
    const answer = await this.ask({ around: id });
    this.keep(answer);
    const at = answer.channels.findIndex((c) => c.id === id);
    return at < 0 ? null : answer.offset + at;
  }

  /**
   * The channel `by` places from `id`, round the ends of the list; the
   * first channel when `id` isn't in it (tuned by its number, say).
   */
  async neighbor(id: number, by: number): Promise<LiveChannel | null> {
    const at = await this.indexOf(id);
    const j = channelStep(at, by, this.total.value ?? 0);
    if (j === null) return null;
    if (!this.at(j)) await this.load(Math.floor(j / CHANNEL_PAGE));
    return this.at(j) ?? null;
  }

  /** A channel changed here (a favourite marked): the pages show it at once. */
  patch(id: number, change: Partial<LiveChannel>): void {
    this.pages.forEach((p) => {
      if (!p.channels) return;
      const i = p.channels.findIndex((c) => c.id === id);
      if (i >= 0) p.channels = p.channels.slice(0, i).concat([{ ...p.channels[i]!, ...change }], p.channels.slice(i + 1));
    });
    this.version.value++;
  }

  /** Everything is asked for again as it shows (the lineup changed on the server). */
  stale(): void {
    this.pages.forEach((p) => (p.at = 0));
    this.version.value++;
  }

  private ask(extra: { offset?: number; around?: number }): Promise<LiveChannelPage> {
    return this.api.liveBrowse({ ...browseQuery(this.category), limit: CHANNEL_PAGE, ...extra });
  }

  private keep(answer: LiveChannelPage): void {
    const n = Math.floor(answer.offset / CHANNEL_PAGE);
    const p = this.pages.get(n);
    if (p) {
      p.channels = answer.channels;
      p.at = Date.now();
    } else this.pages.set(n, { channels: answer.channels, at: Date.now(), pending: false });
    this.total.value = answer.total;
    this.version.value++;
  }
}

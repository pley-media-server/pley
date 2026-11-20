import { signal } from "@preact/signals";
import type { LiveGuide, LiveProgram } from "../api/types";
import { chunkStarts, GUIDE_CHUNK, mergeGuide } from "./data";

/** What the guide needs of the server (the Api; tests give their own). */
export interface GuideSource {
  liveGuide(from: number, to: number, ids: number[]): Promise<LiveGuide>;
}

/** A channel's chunk asked for this long ago is asked for again when it shows. */
const STALE_MS = 5 * 60_000;
/** One that failed waits this long before it is asked for again. */
const RETRY_MS = 15_000;

/**
 * The guide's programs, asked for in six-hour chunks as the guide moves
 * through time, and only for the channels in view (`ids`), counted from
 * `origin`, where the guide opens: a lineup of tens of thousands costs the
 * rows on screen.
 */
export class GuideStore {
  readonly origin: number;
  /** Programs by channel id, in order. */
  readonly programs = new Map<number, LiveProgram[]>();
  /** Bumped as chunks come in: rows read `programs` again. */
  readonly version = signal(0);
  private readonly api: GuideSource;
  /** When each channel's chunk was asked for (negative: failed then), by chunk start. */
  private readonly asked = new Map<number, Map<number, { at: number; ok: boolean; pending: boolean }>>();

  constructor(api: GuideSource, origin: number) {
    this.api = api;
    this.origin = origin;
  }

  /** Whether channel `id`'s programs around `t` have been asked for and come (or failed). */
  answered(id: number, t: number): boolean {
    const start = chunkStarts(t, t + 1, this.origin)[0];
    const a = start === undefined ? undefined : this.asked.get(start)?.get(id);
    return !!a && !a.pending;
  }

  /** Asks for channels `ids`' chunks covering `from`–`to` that it doesn't have fresh. */
  want(from: number, to: number, ids: number[]): void {
    if (!ids.length) return;
    const now = Date.now();
    for (const start of chunkStarts(from, to, this.origin)) {
      let byId = this.asked.get(start);
      if (!byId) this.asked.set(start, (byId = new Map()));
      const chunk = byId;
      const missing = ids.filter((id) => {
        const a = chunk.get(id);
        return !a || (!a.pending && now - a.at >= (a.ok ? STALE_MS : RETRY_MS));
      });
      if (!missing.length) continue;
      const entries = missing.map((id) => {
        const e = { at: now, ok: false, pending: true };
        chunk.set(id, e);
        return e;
      });
      const done = (ok: boolean) => {
        for (const e of entries) {
          e.pending = false;
          e.ok = ok;
          e.at = Date.now();
        }
        this.version.value++;
      };
      this.api.liveGuide(start, start + GUIDE_CHUNK, missing).then(
        (guide) => {
          mergeGuide(this.programs, guide);
          done(true);
        },
        () => done(false),
      );
    }
  }

  /** The guide changed on the server: everything is asked for again as it shows. */
  forget(): void {
    this.asked.clear();
    this.programs.clear();
    this.version.value++;
  }
}

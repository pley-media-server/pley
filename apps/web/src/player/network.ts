import { api, isApiError, qs } from "@/api/client";
import type { Remembered } from "./autoQuality";

/**
 * What the server knows of the network this browser plays on, for
 * Automatic's start (docs/QUALITY.md, "Automatic"): the most it starts at
 * there, and whether to measure the network first. Only Automatic asks.
 */
export interface NetworkAnswer {
  /** The server's own network: never a speed test. */
  local: boolean;
  /** The most Automatic starts at here, kbps; `null`: no limit known. */
  ceiling_kbps: number | null;
  source: "playback" | "probe" | null;
  /** When what set it happened, ms since 1970. */
  since: number | null;
  /** Measure the network before starting. */
  probe: boolean;
}

/** What a speed test measured: kbps, and whether all of it arrived within the time it had. */
export interface SpeedResult {
  kbps: number;
  complete: boolean;
}

/** How long the server is given to say what it knows; past that, the browser's own memory. */
export const NETWORK_ASK_MS = 1_500;
/** What a speed test asks for, and what one that can't read as it arrives asks for whole. */
export const SPEED_TEST_BYTES = 32 * 1024 * 1024;
export const SPEED_TEST_WHOLE_BYTES = 8 * 1024 * 1024;
/** The first byte must come this soon; from it, the test reads this long. */
export const SPEED_TEST_FIRST_BYTE_MS = 1_000;
export const SPEED_TEST_WINDOW_MS = 2_000;
/** Less than this arrived says nothing of the network. */
export const SPEED_TEST_MIN_BYTES = 256 * 1024;
/** A test that couldn't read all of it keeps 30 % in hand, as the server does. */
export const PROBE_SHARE = 0.7;
/** The longest a speed test's report is waited for before starting. */
export const REPORT_WAIT_MS = 500;
/** A speed test isn't run again this soon, whatever came of the last. */
export const PROBE_AGAIN_MS = 10 * 60_000;
/** One answer serves the playbacks that start this soon after it (the route's prefetch, then the player). */
const ANSWER_KEPT_MS = 10_000;
const MAX_KBPS = 10_000_000;

export interface Timers {
  now(): number;
  set(fn: () => void, ms: number): unknown;
  clear(handle: unknown): void;
}

const realTimers: Timers = {
  now: () => performance.now(),
  set: (fn, ms) => window.setTimeout(fn, ms),
  clear: (handle) => window.clearTimeout(handle as number),
};

/** What `bytes` read over `ms` (from the first byte) say; `null` when too little arrived to say anything. */
export function speedOf(bytes: number, ms: number, complete: boolean): SpeedResult | null {
  if (bytes < SPEED_TEST_MIN_BYTES) return null;
  // bytes × 8 / seconds / 1000 = bits per millisecond.
  const kbps = Math.round((bytes * 8) / Math.max(1, ms));
  return { kbps: Math.max(1, Math.min(MAX_KBPS, kbps)), complete };
}

/**
 * A download read as it arrives: `onData` hears each piece's size, `done`
 * settles when all of it arrived (or it failed), `stop` ends it early.
 */
export type Download = (onData: (bytes: number) => void) => { done: Promise<void>; stop: () => void };

/**
 * A speed test over `download`: timed from the first byte, stopped 2 s
 * after it (or 1 s without one), complete when all of it came within the
 * 2 s. `null` when it says nothing (too little arrived).
 */
export function measureSpeed(download: Download, timers: Timers = realTimers): Promise<SpeedResult | null> {
  return new Promise((resolve) => {
    let first: number | null = null;
    let bytes = 0;
    let settled = false;
    let timer: unknown = null;
    let stop: () => void = () => undefined;
    const finish = (complete: boolean) => {
      if (settled) return;
      settled = true;
      timers.clear(timer);
      stop();
      resolve(first === null ? null : speedOf(bytes, timers.now() - first, complete));
    };
    timer = timers.set(() => finish(false), SPEED_TEST_FIRST_BYTE_MS);
    const run = download((n) => {
      if (settled) return;
      if (first === null) {
        first = timers.now();
        timers.clear(timer);
        timer = timers.set(() => finish(false), SPEED_TEST_WINDOW_MS);
      }
      bytes += n;
    });
    stop = run.stop;
    run.done.then(
      () => finish(first !== null && timers.now() - first <= SPEED_TEST_WINDOW_MS),
      () => finish(false),
    );
  });
}

/**
 * A speed test for an engine that can't read a body as it arrives: the
 * smaller download whole, timed from the request, never complete.
 */
export async function measureWhole(fetchWhole: () => Promise<number>, now: () => number = () => performance.now()): Promise<SpeedResult | null> {
  const from = now();
  const bytes = await fetchWhole();
  return speedOf(bytes, now() - from, false);
}

/** What the server is told: a speed test's result, or Automatic's ceiling (`null`: forgotten). */
export type NetworkReport = { probe_kbps: number; probe_complete: boolean } | { ceiling_kbps: number | null };

export interface NetworkIo {
  /** GET /api/playback/network, given 1.5 s. */
  ask(): Promise<NetworkAnswer>;
  /** PUT /api/playback/network. */
  tell(report: NetworkReport): Promise<NetworkAnswer>;
  speedTest(): Promise<SpeedResult | null>;
  /** The time remembered ceilings are kept in (ms since 1970). */
  wall(): number;
}

function isAnswer(a: unknown): a is NetworkAnswer {
  const v = a as Partial<NetworkAnswer> | null;
  return !!v && typeof v === "object" && typeof v.probe === "boolean" && (v.ceiling_kbps === null || typeof v.ceiling_kbps === "number");
}

function ceilingOf(a: NetworkAnswer, wall: number): Remembered | null {
  return a.ceiling_kbps && a.ceiling_kbps > 0 ? { kbps: a.ceiling_kbps, at: a.since ?? wall } : null;
}

/**
 * Where Automatic may start, from the server (docs/QUALITY.md): the
 * ceiling it knows for this network (`null`: none), after a speed test
 * when it asks for one; `undefined` when it can't say (no answer, an older
 * server, a test that measured nothing), and the browser's own memory
 * decides as before.
 */
export async function startCeiling(io: NetworkIo): Promise<Remembered | null | undefined> {
  let answer: NetworkAnswer;
  try {
    const asked = await io.ask();
    if (!isAnswer(asked)) return undefined;
    answer = asked;
  } catch {
    return undefined;
  }
  if (answer.probe && !answer.local) {
    const speed = await io.speedTest().catch(() => null);
    if (!speed) return undefined;
    try {
      const told = await io.tell({ probe_kbps: speed.kbps, probe_complete: speed.complete });
      if (!isAnswer(told)) throw new Error("no answer");
      answer = told;
    } catch {
      // Not heard in time: what the server would make of it.
      return speed.complete ? null : { kbps: Math.round(speed.kbps * PROBE_SHARE), at: io.wall() };
    }
  }
  return ceilingOf(answer, io.wall());
}

/** `promise`, or a rejection after `ms`, `abort` called then. */
function within<T>(promise: Promise<T>, ms: number, abort?: () => void): Promise<T> {
  return new Promise<T>((resolve, reject) => {
    const timer = window.setTimeout(() => {
      abort?.();
      reject(new Error("timed out"));
    }, ms);
    promise.then(
      (v) => {
        window.clearTimeout(timer);
        resolve(v);
      },
      (e: unknown) => {
        window.clearTimeout(timer);
        reject(e instanceof Error ? e : new Error(String(e)));
      },
    );
  });
}

// ---------------------------------------------------------------------------
// This browser's.

const CLIENT_KEY = "pley.networkClient";
let networkClient: string | null = null;

/**
 * Who the server remembers networks for: this browser, for good (the
 * playback's client id is the tab's, and would ask for a speed test in each
 * new tab).
 */
export function networkClientId(): string {
  if (networkClient) return networkClient;
  let id: string | null = null;
  try {
    id = window.localStorage.getItem(CLIENT_KEY);
  } catch {
    // storage blocked: one for this page
  }
  if (!id || id.length > 128) {
    const bytes = new Uint8Array(16);
    crypto.getRandomValues(bytes);
    id = `web-${Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("")}`;
    try {
      window.localStorage.setItem(CLIENT_KEY, id);
    } catch {
      // kept for this page
    }
  }
  networkClient = id;
  return id;
}

/** The server answered 404: it is older, and isn't asked or told again until the page loads again. */
let olderServer = false;
let lastProbe = -Infinity;
let kept: { at: number; ceiling: Promise<Remembered | null | undefined> } | null = null;

/** Streams the speed test, or reads its smaller form whole where the browser can't read a body as it comes. */
async function browserSpeedTest(): Promise<SpeedResult | null> {
  const streams = typeof ReadableStream !== "undefined" && "body" in Response.prototype;
  if (!streams) {
    const abort = new AbortController();
    const whole = async () => {
      const res = await fetch(`/api/playback/speedtest?bytes=${SPEED_TEST_WHOLE_BYTES}`, { credentials: "same-origin", cache: "no-store", signal: abort.signal });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return (await res.arrayBuffer()).byteLength;
    };
    return within(measureWhole(whole), SPEED_TEST_FIRST_BYTE_MS + SPEED_TEST_WINDOW_MS + 500, () => abort.abort());
  }
  return measureSpeed((onData) => {
    const abort = new AbortController();
    const done = (async () => {
      const res = await fetch(`/api/playback/speedtest?bytes=${SPEED_TEST_BYTES}`, { credentials: "same-origin", cache: "no-store", signal: abort.signal });
      if (!res.ok || !res.body) throw new Error(`HTTP ${res.status}`);
      const reader = res.body.getReader();
      for (;;) {
        const { done: end, value } = await reader.read();
        if (end) return;
        onData(value.byteLength);
      }
    })();
    return { done, stop: () => abort.abort() };
  });
}

const browserIo: NetworkIo = {
  async ask() {
    const abort = new AbortController();
    try {
      return await within(
        api.get<NetworkAnswer>(`/api/playback/network${qs({ client_id: networkClientId() })}`, { signal: abort.signal, noAuthRedirect: true }),
        NETWORK_ASK_MS,
        () => abort.abort(),
      );
    } catch (err) {
      if (isApiError(err, 404)) olderServer = true;
      throw err;
    }
  },
  tell(report) {
    const abort = new AbortController();
    return within(
      api.put<NetworkAnswer>("/api/playback/network", { client_id: networkClientId(), ...report }, { signal: abort.signal, noAuthRedirect: true }),
      REPORT_WAIT_MS,
      () => abort.abort(),
    );
  },
  async speedTest() {
    // One test at a time, and not again soon after one that told nothing.
    if (performance.now() - lastProbe < PROBE_AGAIN_MS) return null;
    lastProbe = performance.now();
    return browserSpeedTest();
  },
  wall: () => Date.now(),
};

/**
 * Where Automatic may start (`startCeiling`), asked once for the playbacks
 * starting now: the player's route asks as it loads, in parallel with the
 * item, and the player takes that answer.
 */
export function networkStart(): Promise<Remembered | null | undefined> {
  if (olderServer) return Promise.resolve(undefined);
  const now = performance.now();
  if (kept && now - kept.at < ANSWER_KEPT_MS) return kept.ceiling;
  const ceiling = startCeiling(browserIo);
  kept = { at: now, ceiling };
  return ceiling;
}

/**
 * Automatic's remembered ceiling changed (a step down, a step back up, or
 * Automatic picked again: `null`): the server keeps it for this network.
 * Fire and forget.
 */
export function reportCeiling(kbps: number | null): void {
  kept = null;
  if (olderServer) return;
  api
    .put<NetworkAnswer>("/api/playback/network", { client_id: networkClientId(), ceiling_kbps: kbps === null ? null : Math.round(kbps) }, { noAuthRedirect: true })
    .catch((err: unknown) => {
      if (isApiError(err, 404)) olderServer = true;
    });
}

/**
 * Whether the server's last play answer said it converts no video
 * (`original_only`): the next title starts as the original, rather than at
 * a quality the server would only answer with the original again.
 */
let originalOnly = false;

export function serverOriginalOnly(): boolean {
  return originalOnly;
}

/** A play answer came: whether it said the server converts no video. */
export function heardOriginalOnly(only: boolean): void {
  originalOnly = only;
}

/** Forgets what was heard of the server (tests). */
export function forgetNetwork(): void {
  originalOnly = false;
  olderServer = false;
  lastProbe = -Infinity;
  kept = null;
  networkClient = null;
}

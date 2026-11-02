import { NetworkError, type Transport } from "../net/transport";
import { candidates, DEFAULT_PORT, displayAddress, isLocalHost, origin } from "./addresses";

/** A pley server that answered at `url`. */
export interface ServerInfo {
  url: string;
  name: string;
  version: string;
  /**
   * It requires two-step sign-in of every account with a password: signing
   * in on a phone sets it up for whoever hasn't. False from servers before it.
   */
  twoStepRequired: boolean;
}

/** Why an address isn't a server to sign in to. */
export type ConnectProblem =
  | { kind: "bad_address" }
  | { kind: "not_pley"; address: string }
  | { kind: "unreachable"; address: string; reason: string };

export class ConnectError extends Error {
  readonly problem: ConnectProblem;
  constructor(problem: ConnectProblem) {
    super(problemText(problem));
    this.name = "ConnectError";
    this.problem = problem;
  }
}

/** The Android app's words for each problem (TvServersScreen's problemText). */
export function problemText(p: ConnectProblem): string {
  switch (p.kind) {
    case "bad_address":
      return "Enter an address such as nas, 192.168.1.20 or pley.example.com.";
    case "not_pley":
      return `Something answers at ${p.address}, but it isn't Pley.`;
    case "unreachable":
      return `Couldn't reach ${p.address}: ${p.reason}`;
  }
}

/** `host:port` as the server list shows it, never the scheme ("192.168.1.20:7539", "pley.example.com"). */
export function hostPort(url: string): string {
  const m = /^(https?):\/\/([^/]+)/.exec(url);
  if (!m) return url;
  const scheme = m[1];
  const host = m[2]!;
  if (/:\d+$/.test(host.replace(/^\[[^\]]*\]/, ""))) return host;
  return scheme === "http" ? `${host}` : host;
}

function timeoutFor(url: string): number {
  const host = /^https?:\/\/(\[[^\]]+\]|[^/:]+)/.exec(url)?.[1] ?? "";
  return isLocalHost(host) ? 3_000 : 10_000;
}

/**
 * Asks `url` whether it is pley (GET /api/server, no token): its name and
 * version, `null` when something else answered. Network trouble throws
 * [NetworkError].
 */
export async function probe(http: Transport, url: string, timeoutMs = timeoutFor(url)): Promise<ServerInfo | null> {
  const res = await http({ method: "GET", url: `${url}/api/server`, headers: { Accept: "application/json" }, responseType: "json", timeoutMs });
  if (res.status !== 200) return null;
  const body = res.body as { name?: unknown; version?: unknown; two_step_required?: unknown } | null;
  if (!body || typeof body !== "object" || typeof body.version !== "string") return null;
  return {
    url,
    name: typeof body.name === "string" && body.name.trim() ? body.name.trim() : displayAddress(url),
    version: body.version,
    twoStepRequired: body.two_step_required === true,
  };
}

function reasonOf(e: unknown): string {
  return e instanceof NetworkError && /too long/.test(e.message) ? "no answer in time" : "nothing answers there";
}

/**
 * The server someone typed (Connect under the address): each candidate in
 * turn, https first for names on the internet; the first pley answer
 * wins. Throws [ConnectError] with why none did.
 */
export async function connect(http: Transport, input: string): Promise<ServerInfo> {
  const list = candidates(input);
  if (!list.length) throw new ConnectError({ kind: "bad_address" });
  let notPley: string | null = null;
  let last: { address: string; reason: string } | null = null;
  for (const url of list) {
    try {
      const info = await probe(http, url);
      if (info) return info;
      notPley = notPley ?? hostPort(url);
    } catch (e) {
      if (!(e instanceof NetworkError)) throw e;
      last = { address: hostPort(url), reason: reasonOf(e) };
    }
  }
  if (notPley) throw new ConnectError({ kind: "not_pley", address: notPley });
  throw new ConnectError({ kind: "unreachable", address: last?.address ?? input, reason: last?.reason ?? "nothing answers there" });
}

/** A server already known (a row): it answers as pley, or why not. */
export async function check(http: Transport, url: string): Promise<ServerInfo> {
  try {
    const info = await probe(http, url);
    if (info) return info;
    throw new ConnectError({ kind: "not_pley", address: hostPort(url) });
  } catch (e) {
    if (e instanceof NetworkError) throw new ConnectError({ kind: "unreachable", address: hostPort(url), reason: reasonOf(e) });
    throw e;
  }
}


/** The /24 of an IPv4 address without its last number ("192.168.1."); `null` for anything else. */
export function subnetOf(address: string | null | undefined): string | null {
  const m = /^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/.exec(address ?? "");
  if (!m || m[1] === "0" || m[1] === "127") return null;
  return `${m[1]}.${m[2]}.${m[3]}.`;
}

/** How many addresses a sweep asks at once, how long each may take, and how long it may run. */
export const SWEEP_PARALLEL = 48;
export const SWEEP_TIMEOUT_MS = 1_500;
export const SWEEP_TOTAL_MS = 6_000;

/**
 * "Search this network": GET /api/server on every address of the TV's /24
 * at pley's port, a few dozen at a time, for at most 6 s. Servers are
 * reported as they answer.
 */
export async function sweep(http: Transport, subnet: string, onFound: (s: ServerInfo) => void, stopped: () => boolean, now: () => number = Date.now): Promise<void> {
  const started = now();
  let next = 1;
  const worker = async () => {
    while (next <= 254 && !stopped() && now() - started < SWEEP_TOTAL_MS) {
      const host = next++;
      const url = `http://${subnet}${host}:${DEFAULT_PORT}`;
      try {
        const info = await probe(http, url, Math.min(SWEEP_TIMEOUT_MS, Math.max(1, SWEEP_TOTAL_MS - (now() - started))));
        if (info && !stopped()) onFound(info);
      } catch {
        // Nothing there.
      }
    }
  };
  const workers: Promise<void>[] = [];
  for (let i = 0; i < SWEEP_PARALLEL; i++) workers.push(worker());
  await Promise.all(workers);
}

/** Whether two addresses are the same server. */
export function sameServer(a: string, b: string): boolean {
  return origin(a) === origin(b);
}

import { NetworkError, type Transport } from "../net/transport";
import { origin, unencrypted, UNENCRYPTED } from "./addresses";
import { AuthError, DEVICE_CODE_GRANT, fetchMetadata, NO_CODES, signsInWithCodes, UnreachableError, type ServerMetadata } from "./metadata";
import { parseTokenResponse, postForm, stored, TokenError, type StoredTokens } from "./tokens";

/** Everything pley's own TV apps may ask for (docs/API.md, "Scopes"). */
export const SCOPES = ["library", "play", "playlists", "profiles", "requests", "approvals"];
/** Asked for only where the server lists them: one that doesn't know a scope refuses the whole sign-in. */
const LISTED_ONLY = ["requests", "approvals"];

/**
 * A code for signing in a TV (RFC 8628): shown as `userCode` with
 * `verificationUri` and a QR code of `verificationUriComplete`, and approved
 * on a phone or computer at the server's /link page.
 */
export interface DeviceCode {
  server: string;
  issuer: string;
  /** What the TV polls with. Never shown. */
  deviceCode: string;
  /** As shown: `BCDF-GHJK`. */
  userCode: string;
  verificationUri: string;
  verificationUriComplete: string | null;
  /** When the code stops working, in ms since the epoch. */
  expiresAt: number;
  intervalSeconds: number;
  tokenEndpoint: string;
  revocationEndpoint: string;
  scope: string;
}

/** Where a code stands. */
export type DevicePoll =
  | { kind: "waiting" }
  | { kind: "signed_in"; tokens: StoredTokens }
  | { kind: "denied" }
  | { kind: "expired" }
  /** The server can't be reached: polling goes on by itself in `retryInMs`. */
  | { kind: "unreachable"; retryInMs: number }
  | { kind: "failed"; message: string };

/** RFC 8628 §3.2: without an interval, 5 seconds. */
export const DEFAULT_INTERVAL_S = 5;
/** What a slow_down adds to this wait and every later one (§3.5). */
export const SLOW_DOWN_MS = 5_000;
/** The waits after network failures in a row. */
export const BACKOFF_MS = [2_000, 4_000, 8_000, 16_000, 30_000];

/** The scope to ask for: what the server offers of pley's own; a server that lists none gets the ones every server knows. */
export function scopeFor(metadata: ServerMetadata): string {
  const offered = metadata.scopesSupported;
  return SCOPES.filter((s) => (offered ? offered.indexOf(s) >= 0 : LISTED_ONLY.indexOf(s) < 0)).join(" ");
}

/**
 * The device authorization grant (RFC 8628 §3) against a pley server: POST
 * client_id and scope to the metadata's device_authorization_endpoint for a
 * code, then ask the token endpoint with it until someone answers.
 */
export class DeviceSignIn {
  private readonly http: Transport;
  private readonly clientId: string;
  private readonly clock: () => number;
  private readonly sleep: (ms: number) => Promise<void>;

  constructor(
    http: Transport,
    clientId: string,
    clock: () => number = Date.now,
    sleep: (ms: number) => Promise<void> = (ms) => new Promise((r) => setTimeout(r, ms)),
  ) {
    this.http = http;
    this.clientId = clientId;
    this.clock = clock;
    this.sleep = sleep;
  }

  async start(server: string): Promise<DeviceCode> {
    const home = origin(server);
    if (!home) throw new AuthError("that isn't a server address");
    if (unencrypted(home)) throw new AuthError(UNENCRYPTED);
    const metadata = await fetchMetadata(this.http, home);
    if (!signsInWithCodes(metadata)) throw new AuthError(NO_CODES);
    const scope = scopeFor(metadata);
    let answer: unknown;
    try {
      answer = await postForm(this.http, metadata.deviceAuthorizationEndpoint!, [
        ["client_id", this.clientId],
        ["scope", scope],
      ]);
    } catch (e) {
      if (e instanceof TokenError && (e.oauth?.error === "invalid_client" || e.oauth?.error === "unauthorized_client")) {
        // A server from before pley's TV apps were built in.
        throw new AuthError("this server doesn't know Pley's TV apps yet: update it");
      }
      if (e instanceof TokenError && e.status === 429) throw new AuthError("too many TVs are signing in here right now: try again in a minute");
      if (e instanceof NetworkError) throw new UnreachableError();
      throw e;
    }
    return parseDeviceCode(answer, home, metadata, scope, this.clock());
  }

  /**
   * Asks whether `code` was approved, every interval (slower when the
   * server says so), until it is signed in, denied or expired; `onUpdate`
   * hears each change. Stops early when `stopped()` turns true.
   */
  async poll(code: DeviceCode, onUpdate: (state: DevicePoll) => void, stopped: () => boolean): Promise<DevicePoll> {
    let interval = (code.intervalSeconds > 0 ? code.intervalSeconds : DEFAULT_INTERVAL_S) * 1000;
    let failures = 0;
    let troubled = false;
    onUpdate({ kind: "waiting" });
    for (;;) {
      const wait = failures === 0 ? interval : Math.max(interval, BACKOFF_MS[Math.min(failures, BACKOFF_MS.length) - 1]!);
      await this.sleep(Math.min(wait, Math.max(0, code.expiresAt - this.clock())));
      if (stopped()) return { kind: "waiting" };
      if (this.clock() >= code.expiresAt) return done({ kind: "expired" });
      let outcome: DevicePoll | null = null;
      try {
        const answer = await postForm(this.http, code.tokenEndpoint, [
          ["grant_type", DEVICE_CODE_GRANT],
          ["device_code", code.deviceCode],
          ["client_id", this.clientId],
        ]);
        failures = 0;
        const parsed = parseTokenResponse(answer, code.scope);
        outcome = {
          kind: "signed_in",
          tokens: stored(parsed, code.issuer, code.tokenEndpoint, code.revocationEndpoint, this.clock()),
        };
      } catch (e) {
        if (e instanceof TokenError) {
          failures = 0;
          switch (e.oauth?.error) {
            case "authorization_pending":
              break;
            case "slow_down":
              interval += SLOW_DOWN_MS;
              break;
            case "access_denied":
              outcome = { kind: "denied" };
              break;
            case "expired_token":
              outcome = { kind: "expired" };
              break;
            case "invalid_grant":
              outcome = { kind: "failed", message: "this code doesn't work anymore: get a new one" };
              break;
            default:
              outcome = { kind: "failed", message: e.message };
          }
        } else if (e instanceof NetworkError) {
          failures++;
        } else if (e instanceof AuthError) {
          outcome = { kind: "failed", message: e.message };
        } else {
          throw e;
        }
      }
      if (stopped()) return { kind: "waiting" };
      if (outcome) return done(outcome);
      if (failures > 0) {
        troubled = true;
        onUpdate({ kind: "unreachable", retryInMs: Math.max(interval, BACKOFF_MS[Math.min(failures, BACKOFF_MS.length) - 1]!) });
      } else if (troubled) {
        troubled = false;
        onUpdate({ kind: "waiting" });
      }
    }

    function done(state: DevicePoll): DevicePoll {
      onUpdate(state);
      return state;
    }
  }
}

/** The code in `body`, the answer of `server`'s device authorization endpoint (§3.2). */
export function parseDeviceCode(body: unknown, server: string, metadata: ServerMetadata, scope: string, now: number): DeviceCode {
  const bad = new AuthError("the server's answer can't be read: try again");
  if (!body || typeof body !== "object") throw bad;
  const o = body as Record<string, unknown>;
  const deviceCode = o.device_code;
  const userCode = o.user_code;
  const uri = o.verification_uri;
  if (typeof deviceCode !== "string" || !deviceCode || typeof userCode !== "string" || !userCode || typeof uri !== "string") throw bad;
  // The page to open must be this server's own: a code typed elsewhere would sign someone else's TV in.
  if (origin(uri) !== metadata.issuer) throw new AuthError("this server sends people to another address to approve TVs: Pley doesn't");
  const complete = typeof o.verification_uri_complete === "string" && origin(o.verification_uri_complete) === metadata.issuer
    ? o.verification_uri_complete
    : null;
  const expiresIn = typeof o.expires_in === "number" && o.expires_in > 0 ? o.expires_in : 600;
  const interval = typeof o.interval === "number" && o.interval > 0 ? o.interval : DEFAULT_INTERVAL_S;
  return {
    server,
    issuer: metadata.issuer,
    deviceCode,
    userCode,
    verificationUri: uri,
    verificationUriComplete: complete,
    expiresAt: now + expiresIn * 1000,
    intervalSeconds: interval,
    tokenEndpoint: metadata.tokenEndpoint,
    revocationEndpoint: metadata.revocationEndpoint,
    scope,
  };
}

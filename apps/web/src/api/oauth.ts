import { api } from "./client";

/** An app that asks to use an account, as the server checked it. */
export interface OAuthClient {
  /** The client ID: the address of the app's description. */
  id: string;
  /** The ASCII host of that address. */
  host: string;
  /** What the app calls itself. pley vouches for it only for its own apps. */
  name: string | null;
  /** One of pley's own apps (built into the server). */
  first_party: boolean;
}

/** Where an app's answer goes back to: a website (its host), an app on this computer, or an app (its scheme). */
export interface AppRedirect {
  kind: "app" | "loopback" | "web";
  to: string;
}

/** Where the answer goes when a TV or a Jellyfin app shows a code: that code, and what the device says it is. */
export interface ShowingCode<Kind extends "device" | "quick-connect"> {
  kind: Kind;
  to: string;
  device?: string | null;
  address?: string | null;
}

/** What an app may ask for. */
export type OAuthScope = "library" | "play" | "playlists" | "profiles";

/**
 * `POST /api/oauth/requests`: a request to show for consent. Also
 * `POST /api/oauth/device/verify`, for a TV that showed a code.
 */
export interface ConsentRequest {
  status: "consent";
  id: string;
  expires_at: number;
  client: OAuthClient;
  /**
   * Where the answer goes: back to a website (its host) or an app (its
   * scheme), or to the TV showing a code ("BCDF-GHJK"), with what that
   * device says it is ("Kodi 21.3; Windows", unchecked) and the address it
   * asked from; or to the Jellyfin app showing a Quick Connect code, with
   * the name of the device it says it is on.
   */
  redirect: AppRedirect | ShowingCode<"device"> | ShowingCode<"quick-connect">;
  scopes: (OAuthScope | (string & {}))[];
}

/** `POST /api/oauth/requests`: a mistake that goes back to the app, but only on a click. */
export interface RequestError {
  status: "error";
  error: string;
  message: string;
  back_url: string;
  client: OAuthClient;
  /** Where "Back to the app" goes, shown before the click. */
  redirect: AppRedirect;
}

export type RequestAnswer = ConsentRequest | RequestError;

/** `POST /api/oauth/requests/{id}`: where the browser goes next. */
export interface Decision {
  continue_url: string;
}

/** `POST /api/oauth/requests/{id}` for a TV: nothing to go back to, the TV hears the answer when it next asks. */
export interface DeviceDecision {
  done: true;
}

/** Hands the authorize page's query (without "?") to the server, which checks it. */
export function startRequest(query: string) {
  return api.post<RequestAnswer>("/api/oauth/requests", { query });
}

/** Allows the app, or not. */
export function decide(id: string, allow: boolean) {
  return api.post<Decision>(`/api/oauth/requests/${encodeURIComponent(id)}`, { allow });
}

/** Hands over the code a TV shows; the TV's request, to allow or not. */
export function verifyCode(userCode: string) {
  return api.post<ConsentRequest>("/api/oauth/device/verify", { user_code: userCode });
}

/** Allows the TV, or not. */
export function decideDevice(id: string, allow: boolean) {
  return api.post<DeviceDecision>(`/api/oauth/requests/${encodeURIComponent(id)}`, { allow });
}

/** `POST /api/oauth/quick-connect`: a Jellyfin app's request to sign in with Quick Connect. */
export interface QuickConnectRequest {
  status: "consent";
  id: string;
  expires_at: number;
  /** What the app says it is, its version and the device it is on: unchecked. */
  app: string | null;
  version: string | null;
  device: string | null;
  /** The address it asked from. */
  address: string | null;
  /** The code, as the app shows it. */
  code: string;
  /** What its token may do: "play". */
  scopes: string[];
}

/** Hands over the Quick Connect code a Jellyfin app shows; its request, to allow or not. */
export function verifyQuickConnect(code: string) {
  return api.post<QuickConnectRequest>("/api/oauth/quick-connect", { code });
}

/** Allows the Jellyfin app, or not. */
export function decideQuickConnect(id: string, allow: boolean) {
  return api.post<DeviceDecision>(`/api/oauth/quick-connect/${encodeURIComponent(id)}`, { allow });
}

/**
 * A Jellyfin app's request as the consent shows it: as an app Pley can't
 * vouch for, named only in its warning, since anyone may call an app anything.
 */
export function quickConnectConsent(request: QuickConnectRequest): ConsentRequest {
  const said = [request.app, request.version].filter(Boolean).join(" ");
  return {
    status: "consent",
    id: request.id,
    expires_at: request.expires_at,
    client: { id: "", host: "A Jellyfin app", name: said || null, first_party: false },
    redirect: { kind: "quick-connect", to: request.code, device: request.device, address: request.address },
    scopes: request.scopes,
  };
}

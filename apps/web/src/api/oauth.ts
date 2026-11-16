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
   * asked from.
   */
  redirect: AppRedirect | { kind: "device"; to: string; device?: string | null; address?: string | null };
  /** What the account can give of what the app asked for (no administrator rights from an account that isn't one). */
  scopes: (OAuthScope | (string & {}))[];
  /** Those of `scopes` the person may leave out: an administrator's rights ("approvals", "server"), given only when ticked. */
  optional: string[];
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

/** The body of an answer: the scopes go with Allow only. */
function answer(allow: boolean, scopes: string[]) {
  return allow ? { allow, scopes } : { allow };
}

/** Allows the app with `scopes` (of those it asked for), or not. */
export function decide(id: string, allow: boolean, scopes: string[]) {
  return api.post<Decision>(`/api/oauth/requests/${encodeURIComponent(id)}`, answer(allow, scopes));
}

/** Hands over the code a TV shows; the TV's request, to allow or not. */
export function verifyCode(userCode: string) {
  return api.post<ConsentRequest>("/api/oauth/device/verify", { user_code: userCode });
}

/** Allows the TV with `scopes` (of those it asked for), or not. */
export function decideDevice(id: string, allow: boolean, scopes: string[]) {
  return api.post<DeviceDecision>(`/api/oauth/requests/${encodeURIComponent(id)}`, answer(allow, scopes));
}

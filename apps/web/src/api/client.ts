import { currentLanguage, t } from "@/i18n";

export type ApiErrorCode =
  | "bad_request"
  | "unauthorized"
  | "forbidden"
  | "not_found"
  | "conflict"
  | "too_many_requests"
  /** The second step of a sign-in: a code from the authenticator app is missing or wrong. */
  | "two_step"
  /** A sign-in that sets up two-step sign-in first, which the server requires: the answer has the secret (`setup`). */
  | "two_step_setup"
  /** A profile switch: the PIN is missing or wrong. */
  | "wrong_pin"
  /** A profile switch: the PIN is locked after too many wrong ones in a row. */
  | "pin_locked"
  | "unavailable"
  | "internal"
  | "network";

export class ApiError extends Error {
  readonly status: number;
  /** One of `ApiErrorCode`, or a code this client doesn't know yet. */
  readonly code: ApiErrorCode | (string & {});
  /** The answer's JSON, when it had some (fields past `code` and `message`, as `setup`). */
  readonly body: unknown;

  constructor(status: number, code: ApiErrorCode | (string & {}), message: string, body?: unknown) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
    this.body = body;
  }
}

export function isApiError(err: unknown, status?: number): err is ApiError {
  return err instanceof ApiError && (status === undefined || err.status === status);
}

export function errorMessage(err: unknown): string {
  const message = err instanceof ApiError ? err.message || err.code : err instanceof Error ? err.message : String(err);
  // Server messages are lowercase phrases ("this account is disabled").
  return message ? message.charAt(0).toUpperCase() + message.slice(1) : message;
}

let unauthorizedHandler: (() => void) | null = null;

/** Called whenever an authenticated request comes back with 401. */
export function setUnauthorizedHandler(handler: (() => void) | null) {
  unauthorizedHandler = handler;
}

/**
 * Whether an expired session at `pathname` sends the person to sign in:
 * not from the pages that come before one (signing in, making the first
 * account, an invitation), but from setup's steps after that.
 */
export function signInAgainFrom(pathname: string): boolean {
  return !(pathname.startsWith("/login") || /^\/setup\/?$/.test(pathname) || pathname.startsWith("/invite/"));
}

export interface RequestOptions {
  method?: "GET" | "POST" | "PUT" | "PATCH" | "DELETE";
  body?: unknown;
  /** Sent as is (file uploads) instead of JSON-encoding `body`. */
  rawBody?: Blob;
  /** Sent as multipart/form-data (a file with fields), the browser naming its boundary. */
  form?: FormData;
  signal?: AbortSignal;
  keepalive?: boolean;
  /** Don't trigger the global 401 redirect (used by auth probes). */
  noAuthRedirect?: boolean;
}

export async function request<T>(path: string, opts: RequestOptions = {}): Promise<T> {
  return (await requestWithStatus<T>(path, opts)).data;
}

/** As {@link request}, with the answer's status, where 200 and 201 say different things. */
export async function requestWithStatus<T>(path: string, opts: RequestOptions = {}): Promise<{ status: number; data: T }> {
  const { method = "GET", body, rawBody, form, signal, keepalive, noAuthRedirect } = opts;
  // The server words some answers itself (Home's rows) in the language asked for.
  const headers: Record<string, string> = { Accept: "application/json", "Accept-Language": currentLanguage() };
  if (rawBody) headers["Content-Type"] = rawBody.type || "application/octet-stream";
  else if (body !== undefined && !form) headers["Content-Type"] = "application/json";
  let res: Response;
  try {
    res = await fetch(path, {
      method,
      credentials: "same-origin",
      headers,
      body: rawBody ?? form ?? (body !== undefined ? JSON.stringify(body) : undefined),
      signal,
      keepalive,
    });
  } catch (err) {
    if (err instanceof DOMException && err.name === "AbortError") throw err;
    throw new ApiError(0, "network", t("errors.unreachable"));
  }

  if (!res.ok) {
    let code: string = statusCode(res.status);
    // Without pley's own words (a proxy's page, say), plain ones rather than "Bad Gateway".
    let message = res.status >= 502 && res.status <= 504 ? t("errors.notAnswering") : t("errors.failed", { status: res.status });
    let answer: unknown;
    try {
      const data = (await res.json()) as { code?: string; message?: string };
      if (data && typeof data === "object") {
        answer = data;
        if (data.code) code = data.code;
        if (data.message) message = data.message;
      }
    } catch {
      // not JSON
    }
    // A second step asked for, or to set up, is not an expired session.
    if (res.status === 401 && !noAuthRedirect && code !== "two_step" && code !== "two_step_setup") unauthorizedHandler?.();
    throw new ApiError(res.status, code, message, answer);
  }

  if (res.status === 204 || res.status === 202) {
    // Body may be empty (or ignored).
    const text = await res.text().catch(() => "");
    return { status: res.status, data: (text ? safeJson(text) : undefined) as T };
  }
  const contentType = res.headers.get("content-type") ?? "";
  if (contentType.includes("application/json")) return { status: res.status, data: (await res.json()) as T };
  const text = await res.text();
  return { status: res.status, data: (text ? safeJson(text) : undefined) as T };
}

function safeJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

function statusCode(status: number): ApiErrorCode {
  switch (status) {
    case 400:
      return "bad_request";
    case 401:
      return "unauthorized";
    case 403:
      return "forbidden";
    case 404:
      return "not_found";
    case 409:
      return "conflict";
    case 429:
      return "too_many_requests";
    case 503:
      return "unavailable";
    default:
      return "internal";
  }
}

export const api = {
  get: <T>(path: string, opts?: Omit<RequestOptions, "method" | "body">) => request<T>(path, { ...opts, method: "GET" }),
  post: <T>(path: string, body?: unknown, opts?: Omit<RequestOptions, "method" | "body">) =>
    request<T>(path, { ...opts, method: "POST", body }),
  put: <T>(path: string, body?: unknown, opts?: Omit<RequestOptions, "method" | "body">) =>
    request<T>(path, { ...opts, method: "PUT", body }),
  patch: <T>(path: string, body?: unknown, opts?: Omit<RequestOptions, "method" | "body">) =>
    request<T>(path, { ...opts, method: "PATCH", body }),
  del: <T>(path: string, opts?: Omit<RequestOptions, "method" | "body">) => request<T>(path, { ...opts, method: "DELETE" }),
};

/** Build a query string, skipping undefined/null/empty values. */
export function qs(params: Record<string, string | number | boolean | null | undefined>): string {
  const sp = new URLSearchParams();
  for (const [k, v] of Object.entries(params)) {
    if (v === undefined || v === null || v === "") continue;
    sp.set(k, String(v));
  }
  const s = sp.toString();
  return s ? `?${s}` : "";
}

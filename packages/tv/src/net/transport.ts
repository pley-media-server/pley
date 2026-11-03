/**
 * HTTP for the TV apps: XMLHttpRequest, which every TV's web engine has
 * with timeouts and aborts (fetch can't be aborted before Chromium 66,
 * Tizen 5.5 and webOS 6). Tests swap in their own [Transport].
 */

export interface HttpRequest {
  method: "GET" | "HEAD" | "POST" | "PUT" | "PATCH" | "DELETE";
  url: string;
  headers?: Record<string, string>;
  body?: string | Blob | null;
  /** "text" (default), "json", "blob" or "arraybuffer". */
  responseType?: "text" | "json" | "blob" | "arraybuffer";
  timeoutMs?: number;
  cancel?: Cancel;
  /** Hears how much of the body has arrived so far, as it arrives (a speed test). */
  onProgress?: (loaded: number) => void;
}

export interface HttpResponse {
  status: number;
  /** A response header, case-insensitively; `null` when absent. */
  header(name: string): string | null;
  /** The body, as `responseType` asked. */
  body: unknown;
  /** Where the answer came from, after any redirect, when the transport says. */
  url?: string;
}

export type Transport = (req: HttpRequest) => Promise<HttpResponse>;

/** No answer: the network, a timeout, or a request cancelled. */
export class NetworkError extends Error {
  readonly cancelled: boolean;
  constructor(message: string, cancelled = false) {
    super(message);
    this.name = "NetworkError";
    this.cancelled = cancelled;
  }
}

/** Something that can stop requests: a screen that went away, a newer search. */
export class Cancel {
  private listeners: (() => void)[] = [];
  cancelled = false;

  cancel(): void {
    if (this.cancelled) return;
    this.cancelled = true;
    const listeners = this.listeners;
    this.listeners = [];
    for (const l of listeners) l();
  }

  /** Runs `fn` when cancelled (at once when it is already); returns a function that forgets it. */
  onCancel(fn: () => void): () => void {
    if (this.cancelled) {
      fn();
      return () => {};
    }
    this.listeners.push(fn);
    return () => {
      const i = this.listeners.indexOf(fn);
      if (i >= 0) this.listeners.splice(i, 1);
    };
  }
}

/** A request's timeout, as its error says it. */
export const TIMED_OUT = "The server took too long to answer";

/** Requests of more than this wait as long as they need: segments, images over a slow network. */
export const DEFAULT_TIMEOUT_MS = 20_000;

export const xhrTransport: Transport = (req) =>
  new Promise<HttpResponse>((resolve, reject) => {
    if (req.cancel?.cancelled) {
      reject(new NetworkError("cancelled", true));
      return;
    }
    const xhr = new XMLHttpRequest();
    xhr.open(req.method, req.url, true);
    xhr.timeout = req.timeoutMs ?? DEFAULT_TIMEOUT_MS;
    const type = req.responseType ?? "text";
    // Old engines parse JSON slower through responseType "json" than JSON.parse.
    xhr.responseType = type === "json" ? "text" : type;
    const headers = req.headers ?? {};
    for (const name in headers) xhr.setRequestHeader(name, headers[name]!);
    const forget = req.cancel?.onCancel(() => {
      xhr.abort();
      reject(new NetworkError("cancelled", true));
    });
    const progress = req.onProgress;
    if (progress) xhr.onprogress = (e) => progress(e.loaded);
    xhr.onload = () => {
      forget?.();
      let body: unknown = xhr.response;
      if (type === "json") {
        const text = xhr.responseText;
        try {
          body = text ? JSON.parse(text) : null;
        } catch {
          body = text;
        }
      }
      resolve({ status: xhr.status, header: (name) => xhr.getResponseHeader(name), body, url: xhr.responseURL || undefined });
    };
    xhr.onerror = () => {
      forget?.();
      reject(new NetworkError("Can't reach the server"));
    };
    xhr.ontimeout = () => {
      forget?.();
      reject(new NetworkError(TIMED_OUT));
    };
    xhr.send(req.body ?? null);
  });

/** `application/x-www-form-urlencoded` of `fields`, in order. */
export function form(fields: [string, string][]): string {
  return fields.map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join("&");
}

/** `path` with `query` added, values left out when null or undefined. */
export function withQuery(path: string, query?: Record<string, string | number | boolean | null | undefined>): string {
  if (!query) return path;
  const parts: string[] = [];
  for (const key in query) {
    const value = query[key];
    if (value === null || value === undefined) continue;
    parts.push(`${encodeURIComponent(key)}=${encodeURIComponent(String(value))}`);
  }
  if (!parts.length) return path;
  return path + (path.indexOf("?") >= 0 ? "&" : "?") + parts.join("&");
}

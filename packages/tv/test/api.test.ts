import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Api, unreachable, unreachableReason } from "../src/api/client";
import type { Tokens } from "../src/auth/tokens";
import { NetworkError, TIMED_OUT, type HttpRequest, type HttpResponse } from "../src/net/transport";
import { Listing } from "../src/ui/screens/library-data";

const tokens = { accessToken: () => Promise.resolve("token"), doubtServer: () => {} } as unknown as Tokens;

/** A transport whose answers the test decides: `null` never answers, so the request times out (as XHR does) unless cancelled first. */
function transport(answer: (req: HttpRequest) => Promise<HttpResponse> | null) {
  const asked: string[] = [];
  const http = (req: HttpRequest): Promise<HttpResponse> => {
    asked.push(`${req.method} ${req.url.replace(/^https?:\/\/[^/]+/, "")}`);
    const a = answer(req);
    if (a) return a;
    return new Promise((_resolve, reject) => {
      const timer = setTimeout(() => reject(new NetworkError(TIMED_OUT)), req.timeoutMs ?? 20_000);
      req.cancel?.onCancel(() => {
        clearTimeout(timer);
        reject(new NetworkError("cancelled", true));
      });
    });
  };
  return { http, asked };
}

const ok = (body: unknown): Promise<HttpResponse> => Promise.resolve({ status: 200, header: () => "application/json", body });

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

describe("a request nothing answers", () => {
  it("has the next token wait for the server to prove itself again, unless it was cancelled", async () => {
    const doubtServer = vi.fn();
    const watched = { accessToken: () => Promise.resolve("token"), doubtServer } as unknown as Tokens;
    const unanswered = new Api("http://192.168.1.20:7539", watched, () => Promise.reject(new NetworkError("nothing answers")));
    await expect(unanswered.send("/api/home")).rejects.toThrow("nothing answers");
    expect(doubtServer).toHaveBeenCalledTimes(1);
    const cancelled = new Api("http://192.168.1.20:7539", watched, () => Promise.reject(new NetworkError("cancelled", true)));
    await expect(cancelled.send("/api/home")).rejects.toThrow("cancelled");
    expect(doubtServer).toHaveBeenCalledTimes(1);
  });
});

describe("a server on the home network", () => {
  it("gives up after 3 s plus a quick question nobody answers, saying there was no answer in time", async () => {
    const t = transport(() => null);
    const api = new Api("http://192.168.1.20:7539", tokens, t.http);
    let failed: unknown = null;
    void api.home().catch((e: unknown) => (failed = e));
    await vi.advanceTimersByTimeAsync(3_000);
    expect(t.asked).toEqual(["GET /api/hubs/home", "GET /api/server"]);
    expect(failed).toBeNull();
    await vi.advanceTimersByTimeAsync(3_000);
    // 6 s, not the request's 20.
    expect(unreachable(failed)).toBe(true);
    expect(unreachableReason(failed)).toBe("No answer in time.");
  });

  it("waits as long as it takes for a server that answers the question (a conversion starting)", async () => {
    let answerPlay: ((r: HttpResponse) => void) | null = null;
    const t = transport((req) => {
      if (req.url.endsWith("/api/server")) return ok({ name: "Home" });
      return new Promise<HttpResponse>((resolve) => (answerPlay = resolve));
    });
    const api = new Api("http://nas.local:7539", tokens, t.http);
    const result = api.home();
    await vi.advanceTimersByTimeAsync(10_000);
    answerPlay!({ status: 200, header: () => null, body: { hubs: [] } });
    await expect(result).resolves.toEqual({ hubs: [] });
  });

  it("gives up with no answer in time when the question fails", async () => {
    const t = transport((req) => (req.url.endsWith("/api/server") ? Promise.reject(new NetworkError(TIMED_OUT)) : null));
    const api = new Api("http://192.168.1.20:7539", tokens, t.http);
    const result = api.home().then(
      () => null,
      (e: unknown) => e,
    );
    await vi.advanceTimersByTimeAsync(3_100);
    const e = await result;
    expect(unreachable(e)).toBe(true);
    expect(unreachableReason(e)).toBe("No answer in time.");
  });

  it("leaves servers elsewhere (and Tailscale ones) to their own timeouts", async () => {
    for (const server of ["https://pley.example.com", "http://100.101.102.103:7539", "http://nas.tail1234.ts.net:7539"]) {
      const t = transport(() => null);
      const api = new Api(server, tokens, t.http);
      void api.home().catch(() => undefined);
      await vi.advanceTimersByTimeAsync(10_000);
      expect(t.asked).toEqual(["GET /api/hubs/home"]);
    }
  });
});

describe("the token", () => {
  it("goes to the server's own origin only, never to an address elsewhere it hands out", async () => {
    const seen: [string, string | undefined][] = [];
    const http = (req: HttpRequest): Promise<HttpResponse> => {
      seen.push([req.url, req.headers?.Authorization]);
      return ok({});
    };
    const api = new Api("https://pley.example.com", tokens, http);
    await api.get("/api/hubs/home");
    await api.get("https://pley.example.com:443/api/items/1");
    for (const elsewhere of ["https://evil.example.com/api/items/1", "http://pley.example.com/api/items/1", "https://pley.example.com:8443/x"]) {
      await api.get(elsewhere);
    }
    expect(seen).toEqual([
      ["https://pley.example.com/api/hubs/home", "Bearer token"],
      ["https://pley.example.com:443/api/items/1", "Bearer token"],
      ["https://evil.example.com/api/items/1", undefined],
      ["http://pley.example.com/api/items/1", undefined],
      ["https://pley.example.com:8443/x", undefined],
    ]);
  });
});

describe("a library's listing", () => {
  it("asks for its filters in one order, the resolution and dynamic range after the year, as the listing's ETag hashes it", async () => {
    const t = transport(() => ok({ total: 0, items: [] }));
    const api = new Api("http://192.168.1.20:7539", tokens, t.http);
    const params = { sort: "year", order: "desc", watched: false, genre: "Drama", year: 1999, resolution: "4k", dynamicRange: "hdr" } as const;
    await new Listing(api, 3, params, 5000, () => undefined).start();
    expect(t.asked).toEqual(["GET /api/libraries/3/items?sort=year&order=desc&watched=false&genre=Drama&year=1999&resolution=4k&dynamic_range=hdr&offset=0&limit=200"]);
  });
});

describe("search", () => {
  it("asks for the server's most, 200 of each, which its rows show whole", async () => {
    const t = transport(() => ok({ items: [], people: [] }));
    const api = new Api("http://192.168.1.20:7539", tokens, t.http);
    await api.search("dune");
    expect(t.asked).toEqual(["GET /api/search?q=dune&limit=200"]);
  });
});

describe("a live session", () => {
  it("says why it ended in the server's own words", async () => {
    const answers: Record<string, HttpResponse> = {
      "/api/live/sessions/s1/plan": { status: 410, header: () => "application/json", body: { code: "session_stopped", message: "Watching moved to Sky on another device." } },
      "/api/live/sessions/s2/plan": { status: 410, header: () => "application/json", body: { code: "session_stopped", message: null } },
      "/api/live/sessions/s3/plan": { status: 200, header: () => "application/json", body: { video: "copy", audio: "copy" } },
    };
    const t = transport((req) => Promise.resolve(answers[req.url.replace(/^https?:\/\/[^/]+/, "")]!));
    const api = new Api("https://pley.example.com", tokens, t.http);
    expect(await api.whyLiveEnded("s1")).toBe("Watching moved to Sky on another device.");
    // None: "", not a video's "stopped by the administrator".
    expect(await api.whyLiveEnded("s2")).toBe("");
    expect(await api.whyLiveEnded("s3")).toBeNull();
  });
});

describe("no answer", () => {
  it("is a failed connection, a timeout or a gateway with nothing behind it, never a cancelled request", () => {
    expect(unreachable(new NetworkError("Can't reach the server"))).toBe(true);
    expect(unreachableReason(new NetworkError("Can't reach the server"))).toBe("Nothing answers there.");
    expect(unreachable(new NetworkError("cancelled", true))).toBe(false);
  });
});

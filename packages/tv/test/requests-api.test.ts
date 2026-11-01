import { describe, expect, it } from "vitest";
import { Api, ApiError, unreachable } from "../src/api/client";
import { scopeFor } from "../src/auth/device";
import type { ServerMetadata } from "../src/auth/metadata";
import type { Tokens } from "../src/auth/tokens";
import { NetworkError, type HttpRequest, type HttpResponse } from "../src/net/transport";

const tokens = { accessToken: () => Promise.resolve("token") } as unknown as Tokens;

/** An API whose server answers `status` and `body` to everything, keeping what it was asked. */
function server(status = 200, body: unknown = {}) {
  const asked: { line: string; body: unknown; type: string | undefined }[] = [];
  const http = (req: HttpRequest): Promise<HttpResponse> => {
    asked.push({ line: `${req.method} ${req.url.replace(/^https?:\/\/[^/]+/, "")}`, body: req.body ? JSON.parse(req.body as string) : null, type: req.headers?.["Content-Type"] });
    return Promise.resolve({ status, header: () => "application/json", body });
  };
  return { api: new Api("https://pley.example.com", tokens, http), asked };
}

describe("signing in", () => {
  const metadata = (scopes: string[] | null) => ({ issuer: "https://pley.example.com", scopesSupported: scopes }) as ServerMetadata;

  it("asks for requests and approvals only where the server lists them: one that doesn't know a scope refuses the whole sign-in", () => {
    expect(scopeFor(metadata(["library", "play", "playlists", "profiles", "requests", "approvals", "admin"]))).toBe("library play playlists profiles requests approvals");
    expect(scopeFor(metadata(["library", "play", "playlists", "profiles"]))).toBe("library play playlists profiles");
    expect(scopeFor(metadata(null))).toBe("library play playlists profiles");
  });
});

describe("a 503", () => {
  it("is the server's answer when it is pley's JSON: nothing goes offline or tries again for it", async () => {
    const { api } = server(503, { code: "unavailable", message: "requests are off", setup: false });
    const e = await api.requestSummary().catch((x: unknown) => x);
    expect(e).toBeInstanceOf(ApiError);
    expect((e as ApiError).code).toBe("unavailable");
    expect(unreachable(e)).toBe(false);
    const tmdb = await server(503, { code: "tmdb_down", message: "TMDB isn't answering", setup: true }).api.discover("trending", 1).catch((x: unknown) => x);
    expect(unreachable(tmdb)).toBe(false);
  });

  it("is a gateway with nothing behind it otherwise, as 502 and 504 are", async () => {
    for (const [status, body] of [
      [503, null],
      [503, { message: "no code" }],
      [502, null],
      [504, { code: "gateway" }],
    ] as const) {
      const e = await server(status, body).api.requestSummary().catch((x: unknown) => x);
      expect(unreachable(e)).toBe(true);
    }
    expect(unreachable(new ApiError(500, "internal", "boom"))).toBe(false);
    expect(unreachable(new NetworkError("cancelled", true))).toBe(false);
  });
});

describe("the request routes", () => {
  it("list with the filters asked for, flags as 1 and statuses joined", async () => {
    const { api, asked } = server(200, { total: 0, counts: {}, requests: [], people: [] });
    await api.requests({ attention: true, order: "oldest", limit: 30, offset: 0 });
    await api.requests({ status: ["pending", "failed"], kind: "show", userId: 4, q: "sev", mine: true });
    await api.requests({ unseen: true, limit: 20 });
    expect(asked.map((a) => a.line)).toEqual([
      "GET /api/requests?attention=1&order=oldest&offset=0&limit=30",
      "GET /api/requests?status=pending%2Cfailed&kind=show&user_id=4&q=sev&mine=1",
      "GET /api/requests?unseen=1&limit=20",
    ]);
  });

  it("search Discover leaving out what the libraries have, for global search", async () => {
    const { api, asked } = server(200, { page: 1, total_pages: 0, results: [] });
    await api.discoverSearch("dune", 1, true);
    await api.discoverSearch("dune", 2);
    expect(asked.map((a) => a.line)).toEqual(["GET /api/discover/search?q=dune&page=1&missing=1", "GET /api/discover/search?q=dune&page=2"]);
  });

  it("tell a new request (201) from requests followed (200)", async () => {
    const made = server(201, { id: 1, title: "Dune" });
    expect(await made.api.askFor({ kind: "movie", tmdb_id: 438631 })).toMatchObject({ created: true, request: { id: 1 } });
    expect(made.asked[0]).toEqual({ line: "POST /api/requests", body: { kind: "movie", tmdb_id: 438631 }, type: "application/json" });
    const followed = server(200, { id: 2, title: "Severance", watching: true });
    expect(await followed.api.askFor({ kind: "show", tmdb_id: 95396, seasons: [2] })).toMatchObject({ created: false });
  });

  it("decide with a body only where there is something to say", async () => {
    const { api, asked } = server(200, { id: 3 });
    await api.approveRequest(3);
    await api.declineRequest(3, null);
    await api.declineRequest(3, "Not on any service");
    await api.markRequestHandled(3);
    expect(asked).toEqual([
      { line: "POST /api/requests/3/approve", body: null, type: undefined },
      { line: "POST /api/requests/3/decline", body: null, type: undefined },
      { line: "POST /api/requests/3/decline", body: { reason: "Not on any service" }, type: "application/json" },
      { line: "POST /api/requests/3/handled", body: null, type: undefined },
    ]);
  });

  it("withdraw, stop following, mark as seen, and read one title's request", async () => {
    const { api, asked } = server(204, null);
    await api.deleteRequest(5);
    await api.unwatchRequest(5);
    await api.markRequestsSeen();
    await api.itemRequest(9);
    await api.request(5);
    await api.discoverTitle("show", 95396);
    await api.requestSummary();
    expect(asked.map((a) => a.line)).toEqual([
      "DELETE /api/requests/5",
      "DELETE /api/requests/5/watch",
      "POST /api/requests/seen",
      "GET /api/items/9/request",
      "GET /api/requests/5",
      "GET /api/discover/show/95396",
      "GET /api/requests/summary",
    ]);
  });
});

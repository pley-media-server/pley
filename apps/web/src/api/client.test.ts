import { afterEach, describe, expect, it, vi } from "vitest";
import { apiError, mockApi } from "@/test/render";
import { api, ApiError, errorMessage, isApiError, qs, request, setUnauthorizedHandler, signInAgainFrom } from "./client";

afterEach(() => {
  setUnauthorizedHandler(null);
});

describe("request", () => {
  it("sends JSON and reads JSON", async () => {
    const requests = mockApi({ "POST /api/things": (req) => ({ echo: req.body }) });

    await expect(api.post("/api/things", { name: "x" })).resolves.toEqual({ echo: { name: "x" } });

    const [req] = requests;
    expect(req?.init).toMatchObject({
      method: "POST",
      credentials: "same-origin",
      headers: { Accept: "application/json", "Content-Type": "application/json" },
    });
  });

  it("sends files as they are", async () => {
    const requests = mockApi({ "PUT /api/upload": () => undefined });
    const file = new Blob(["png"], { type: "image/png" });

    await api.put("/api/upload", undefined, { rawBody: file });

    expect(requests[0]?.init.body).toBe(file);
    expect(requests[0]?.init.headers).toMatchObject({ "Content-Type": "image/png" });
  });

  it("reads empty, accepted and plain text answers", async () => {
    mockApi({
      "DELETE /api/thing": () => undefined,
      "POST /api/scan": () => new Response('{"queued":true}', { status: 202 }),
      "GET /api/text": () => new Response("hello", { headers: { "content-type": "text/plain" } }),
    });

    await expect(api.del("/api/thing")).resolves.toBeUndefined();
    await expect(api.post("/api/scan")).resolves.toEqual({ queued: true });
    await expect(api.get("/api/text")).resolves.toBe("hello");
  });

  it("turns error answers into ApiErrors", async () => {
    mockApi({
      "GET /api/explained": () => apiError(403, "forbidden", "this account is disabled"),
      "GET /api/plain": () => new Response("<html>", { status: 503, statusText: "Service Unavailable" }),
      "GET /api/bare": () => new Response(null, { status: 500 }),
    });

    const explained = await request("/api/explained").catch((err: unknown) => err);
    expect(isApiError(explained, 403)).toBe(true);
    expect(explained).toMatchObject({ code: "forbidden", message: "this account is disabled" });

    await expect(request("/api/plain")).rejects.toMatchObject({ status: 503, code: "unavailable", message: "the server isn't answering right now" });
    await expect(request("/api/bare")).rejects.toMatchObject({ status: 500, code: "internal", message: "the server couldn't answer (error 500)" });
  });

  it("reports expired sessions, except for probes", async () => {
    mockApi({ "GET /api/auth/me": () => apiError(401, "unauthorized", "not signed in") });
    const onUnauthorized = vi.fn();
    setUnauthorizedHandler(onUnauthorized);

    await expect(api.get("/api/auth/me", { noAuthRedirect: true })).rejects.toBeInstanceOf(ApiError);
    expect(onUnauthorized).not.toHaveBeenCalled();

    await expect(api.get("/api/auth/me")).rejects.toBeInstanceOf(ApiError);
    expect(onUnauthorized).toHaveBeenCalledTimes(1);
  });

  it("doesn't take a second step, or setting it up, for an expired session", async () => {
    mockApi({
      "POST /api/code": () => apiError(401, "two_step", "enter the 6-digit code"),
      "POST /api/setup-first": () => apiError(401, "two_step_setup", "set up two-step sign-in"),
    });
    const onUnauthorized = vi.fn();
    setUnauthorizedHandler(onUnauthorized);
    await expect(api.post("/api/code", {})).rejects.toMatchObject({ status: 401, code: "two_step" });
    await expect(api.post("/api/setup-first", {})).rejects.toMatchObject({ status: 401, code: "two_step_setup" });
    expect(onUnauthorized).not.toHaveBeenCalled();
  });

  it("explains network failures and passes aborts through", async () => {
    vi.stubGlobal("fetch", vi.fn().mockRejectedValueOnce(new TypeError("Failed to fetch")));
    await expect(api.get("/api/x")).rejects.toMatchObject({ status: 0, code: "network" });

    const abort = new DOMException("The operation was aborted.", "AbortError");
    vi.stubGlobal("fetch", vi.fn().mockRejectedValueOnce(abort));
    await expect(api.get("/api/x")).rejects.toBe(abort);
  });
});

describe("signInAgainFrom", () => {
  it("sends setup's steps after the first account to sign in, not the pages before one", () => {
    for (const path of ["/", "/setup/server", "/setup/start", "/setup/plex", "/settings/users"]) expect(signInAgainFrom(path), path).toBe(true);
    for (const path of ["/login", "/setup", "/setup/", "/invite/abc"]) expect(signInAgainFrom(path), path).toBe(false);
  });
});

describe("errorMessage", () => {
  it("starts server messages with a capital", () => {
    expect(errorMessage(new ApiError(403, "forbidden", "this account is disabled"))).toBe("This account is disabled");
    expect(errorMessage(new ApiError(409, "conflict", ""))).toBe("Conflict");
    expect(errorMessage(new Error("boom"))).toBe("Boom");
    expect(errorMessage("plain")).toBe("Plain");
    expect(errorMessage("")).toBe("");
  });

  it("checks the status when asked", () => {
    const err = new ApiError(404, "not_found", "gone");
    expect(isApiError(err)).toBe(true);
    expect(isApiError(err, 404)).toBe(true);
    expect(isApiError(err, 401)).toBe(false);
    expect(isApiError(new Error("x"))).toBe(false);
  });
});

describe("qs", () => {
  it("skips empty values and encodes the rest", () => {
    expect(qs({ q: "a&b", limit: 10, empty: "", none: null, missing: undefined, off: false, zero: 0 })).toBe(
      "?q=a%26b&limit=10&off=false&zero=0",
    );
    expect(qs({ none: null })).toBe("");
  });
});

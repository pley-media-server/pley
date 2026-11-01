import { describe, expect, it } from "vitest";
import { parseDeviceCode } from "../src/auth/device";
import { parseMetadata } from "../src/auth/metadata";
import { parseOAuthError, parseTokenResponse } from "../src/auth/tokens";
// Shared with the native Samsung app's tests, which must answer the same.
import fixture from "./fixtures/auth.json";

const outcome = (f: () => unknown) => {
  try {
    return { ok: f() };
  } catch (e) {
    return { error: (e as Error).message };
  }
};

describe("sign-in answers, as the shared fixture has them", () => {
  it("keeps a server's metadata only when it is the server's own", () => {
    for (const c of fixture.metadata) expect([c.in, outcome(() => parseMetadata(c.in.text, c.in.server))]).toEqual([c.in, c.out]);
  });

  it("reads token answers", () => {
    for (const c of fixture.tokenResponse) expect([c.in, outcome(() => parseTokenResponse(c.in.body, c.in.asked))]).toEqual([c.in, c.out]);
  });

  it("reads OAuth errors", () => {
    for (const c of fixture.oauthError) expect([c.in, parseOAuthError(c.in)]).toEqual([c.in, c.out]);
  });

  it("keeps a code only when it is approved at the server's own address", () => {
    const metadata = parseMetadata(fixture.metadata[0]!.in.text, fixture.metadata[0]!.in.server);
    for (const c of fixture.deviceCode) {
      expect([c.in, outcome(() => parseDeviceCode(c.in, "https://pley.example.com", metadata, "library play", 1_700_000_000_000))]).toEqual([c.in, c.out]);
    }
  });
});

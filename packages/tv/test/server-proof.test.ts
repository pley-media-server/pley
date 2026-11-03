import { describe, expect, it } from "vitest";
import { base64url, fromBase64url, hmacSha256, sha256, utf8 } from "../src/auth/hmac";
import { networkMayHaveChanged, NOT_YOUR_SERVER, parseTokenResponse, stored, Tokens, type KeyValue, type StoredTokens } from "../src/auth/tokens";
import { NetworkError, type HttpRequest, type HttpResponse } from "../src/net/transport";

const hex = (bytes: Uint8Array) => Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");

const SERVER = "http://192.168.1.20:7539";
const ID = "0123456789abcdef0123456789abcdef01234567";
const KEY = base64url(new Uint8Array(32).fill(1));
/** The challenge the server's own test answers: bytes 0 to 15. */
const CHALLENGE = Uint8Array.from({ length: 16 }, (_, i) => i);

describe("SHA-256 and HMAC-SHA256", () => {
  it("hash as FIPS 180-4's examples", () => {
    expect(hex(sha256(utf8("")))).toBe("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    expect(hex(sha256(utf8("abc")))).toBe("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    expect(hex(sha256(utf8("abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq")))).toBe(
      "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
    );
    expect(hex(sha256(new Uint8Array(1_000).fill(0x61)))).toBe("41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3");
  });

  it("sign as RFC 4231's test cases", () => {
    expect(hex(hmacSha256(new Uint8Array(20).fill(0x0b), utf8("Hi There")))).toBe("b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7");
    expect(hex(hmacSha256(utf8("Jefe"), utf8("what do ya want for nothing?")))).toBe("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    // A key longer than a block is hashed first.
    expect(hex(hmacSha256(new Uint8Array(131).fill(0xaa), utf8("Test Using Larger Than Block-Size Key - Hash Key First")))).toBe(
      "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54",
    );
  });

  it("write base64url without padding both ways", () => {
    expect(base64url(CHALLENGE)).toBe("AAECAwQFBgcICQoLDA0ODw");
    expect(base64url(utf8("\xff\xfe"))).toBe("w7_Dvg");
    for (const n of [0, 1, 2, 3, 31, 32, 33]) {
      const b = Uint8Array.from({ length: n }, (_, i) => (i * 37) & 0xff);
      expect(hex(fromBase64url(base64url(b))!)).toBe(hex(b));
    }
    expect(fromBase64url("A")).toBeNull();
    expect(fromBase64url("AA==")).toBeNull();
    expect(fromBase64url("A+")).toBeNull();
  });

  it("make the proof the server makes", () => {
    const key = fromBase64url(KEY)!;
    const proof = base64url(hmacSha256(key, utf8(`pley-server-proof\n${ID}\n${SERVER}\n${base64url(CHALLENGE)}`)));
    expect(proof).toBe("YwkvaCQfaIQCZkoOP5yOE3yR4xRKU5T5wYWz4b85SCM");
  });
});

describe("token answers", () => {
  const answer = { access_token: "pley_at_a", token_type: "Bearer", expires_in: 3600, scope: "library play", refresh_token: "pley_rt_b" };

  it("keep the server's id and proof key", () => {
    expect(parseTokenResponse({ ...answer, server_id: ID, proof_id: "17", proof_key: KEY }, "library play").proof).toEqual({ serverId: ID, proofId: "17", proofKey: KEY });
  });

  it("leave out a proof that isn't written as it should be", () => {
    for (const odd of [
      { server_id: ID, proof_id: "17" },
      { server_id: "", proof_id: "17", proof_key: KEY },
      { server_id: ID, proof_id: "017", proof_key: KEY },
      { server_id: ID, proof_id: 17, proof_key: KEY },
      { server_id: ID, proof_id: "17", proof_key: KEY.slice(1) },
      { server_id: ID, proof_id: "17", proof_key: `${KEY}==` },
    ]) {
      expect(parseTokenResponse({ ...answer, ...odd }, "library play").proof).toBeUndefined();
    }
  });

  it("keep the proof through a refresh that brings none", () => {
    const first = stored(parseTokenResponse({ ...answer, server_id: ID, proof_id: "17", proof_key: KEY }, "library play"), SERVER, `${SERVER}/t`, `${SERVER}/r`, 0);
    const refreshed = stored(parseTokenResponse(answer, "library play"), SERVER, `${SERVER}/t`, `${SERVER}/r`, 1, first);
    expect([refreshed.serverId, refreshed.proofId, refreshed.proofKey]).toEqual([ID, "17", KEY]);
    // A sign-in's key never changes: an answer bringing another one doesn't replace it.
    const other = base64url(new Uint8Array(32).fill(2));
    const swapped = stored(parseTokenResponse({ ...answer, server_id: "another", proof_id: "18", proof_key: other }, "library play"), SERVER, `${SERVER}/t`, `${SERVER}/r`, 2, first);
    expect([swapped.serverId, swapped.proofId, swapped.proofKey]).toEqual([ID, "17", KEY]);
  });
});

function memoryStore(): KeyValue {
  const m = new Map<string, string>();
  return { get: (k) => m.get(k) ?? null, set: (k, v) => void m.set(k, v), remove: (k) => void m.delete(k) };
}

const response = (status: number, body: unknown): HttpResponse => ({ status, header: () => null, body });

/** A server at SERVER that answers proofs as `answer` says, and counts what it was asked. */
function setup(answer: (challenge: string, proofId: string) => HttpResponse | null = (c) => response(200, { id: ID, proof: proofOf(c) })) {
  const asked: HttpRequest[] = [];
  let clock = 1_000_000;
  const http = async (req: HttpRequest) => {
    asked.push(req);
    const url = new URL(req.url);
    if (url.pathname === "/api/server") {
      const r = answer(url.searchParams.get("challenge")!, url.searchParams.get("proof_id")!);
      if (!r) throw new NetworkError("nothing answers");
      return r;
    }
    return response(200, { access_token: "pley_at_new", token_type: "Bearer", expires_in: 3600, refresh_token: "pley_rt_new" });
  };
  const store = memoryStore();
  const tokens: StoredTokens = {
    issuer: SERVER,
    accessToken: "pley_at_a",
    refreshToken: "pley_rt_b",
    expiresAt: clock + 3_600_000,
    scope: "library play",
    tokenEndpoint: `${SERVER}/api/oauth/token`,
    revocationEndpoint: `${SERVER}/api/oauth/revoke`,
    serverId: ID,
    proofId: "17",
    proofKey: KEY,
  };
  store.set(`pley.tokens.${SERVER}`, JSON.stringify(tokens));
  const t = new Tokens(SERVER, "app", store, http, () => {}, () => clock, () => CHALLENGE);
  return { t, asked, store, advance: (ms: number) => (clock += ms), proofs: () => asked.filter((r) => r.url.indexOf("/api/server?") >= 0).length };
}

function proofOf(challenge: string): string {
  return base64url(hmacSha256(fromBase64url(KEY)!, utf8(`pley-server-proof\n${ID}\n${SERVER}\n${challenge}`)));
}

async function refusal(p: Promise<unknown>): Promise<string> {
  try {
    await p;
    return "no refusal";
  } catch (e) {
    return (e as Error).message;
  }
}

describe("a server proves itself before a token goes to it", () => {
  it("asks once, and gives the token when the proof is right", async () => {
    const { t, asked, proofs } = setup();
    expect(await t.accessToken()).toBe("pley_at_a");
    expect(asked[0]!.url).toBe(`${SERVER}/api/server?challenge=AAECAwQFBgcICQoLDA0ODw&proof_id=17`);
    expect(asked[0]!.headers?.Authorization).toBeUndefined();
    await Promise.all([t.accessToken(), t.accessToken()]);
    expect(proofs()).toBe(1);
  });

  it("asks again after half an hour, or after a request failed", async () => {
    const { t, advance, proofs } = setup();
    await t.accessToken();
    advance(29 * 60_000);
    await t.accessToken();
    expect(proofs()).toBe(1);
    advance(2 * 60_000);
    await t.accessToken();
    expect(proofs()).toBe(2);
    t.doubtServer();
    await t.accessToken();
    expect(proofs()).toBe(3);
  });

  it("asks again on another network, and after the clock went back", async () => {
    const { t, advance, proofs } = setup();
    await t.accessToken();
    networkMayHaveChanged();
    await t.accessToken();
    expect(proofs()).toBe(2);
    advance(-60 * 60_000);
    await t.accessToken();
    expect(proofs()).toBe(3);
  });

  it("proves the server again when the network changed while a proof was under way", async () => {
    const { t, proofs } = setup();
    const first = t.accessToken();
    networkMayHaveChanged();
    expect(await first).toBe("pley_at_a");
    expect(proofs()).toBe(2);
  });

  it("takes no proof through a redirect, and calls a server error out of reach", async () => {
    const elsewhere = (c: string) => ({ ...response(200, { id: ID, proof: proofOf(c) }), url: `https://pley.example.com/api/server?challenge=${c}&proof_id=17` });
    expect(await refusal(setup(elsewhere).t.accessToken())).toBe(NOT_YOUR_SERVER);
    const here = (c: string) => ({ ...response(200, { id: ID, proof: proofOf(c) }), url: `${SERVER}/api/server?challenge=${c}&proof_id=17` });
    expect(await setup(here).t.accessToken()).toBe("pley_at_a");
    const { t } = setup(() => response(502, "Bad Gateway"));
    const e = await t.accessToken().catch((e: unknown) => e);
    expect(e).toBeInstanceOf(NetworkError);
    expect((e as Error).message).not.toBe(NOT_YOUR_SERVER);
  });

  it("keeps the token from another device at the address", async () => {
    for (const answer of [
      () => response(200, { id: ID, proof: "AAAA" }),
      () => response(200, { id: "another", proof: proofOf(base64url(CHALLENGE)) }),
      () => response(200, { id: ID, proof: null }),
      () => response(200, { id: ID }),
      () => response(404, "Not found"),
      () => response(200, "<html>a router</html>"),
    ]) {
      const { t, asked } = setup(answer);
      expect(await refusal(t.accessToken())).toBe(NOT_YOUR_SERVER);
      expect(await refusal(t.refresh("pley_at_a"))).toBe(NOT_YOUR_SERVER);
      expect(asked.every((r) => r.url.indexOf("/api/server?") >= 0 && !r.body)).toBe(true);
    }
  });

  it("is out of reach when nothing answers, and tries again", async () => {
    let up = false;
    const { t } = setup((c) => (up ? response(200, { id: ID, proof: proofOf(c) }) : null));
    expect(await refusal(t.accessToken())).toBe("nothing answers");
    up = true;
    expect(await t.accessToken()).toBe("pley_at_a");
  });

  it("refreshes only after a proof, and keeps the key", async () => {
    const { t, asked, store } = setup();
    expect(await t.refresh("pley_at_a")).toBe("pley_at_new");
    expect(asked.map((r) => new URL(r.url).pathname)).toEqual(["/api/server", "/api/oauth/token"]);
    expect(JSON.parse(store.get(`pley.tokens.${SERVER}`)!).proofKey).toBe(KEY);
  });

  it("proves the server anew for a refresh, even just after a proof", async () => {
    const { t, proofs } = setup();
    await t.accessToken();
    expect(proofs()).toBe(1);
    await t.refresh("pley_at_a");
    expect(proofs()).toBe(2);
  });

  it("proves the server once for a token about to end, as it is refreshed", async () => {
    const { t, asked, advance } = setup();
    advance(60 * 60_000);
    expect(await t.accessToken()).toBe("pley_at_new");
    expect(asked.map((r) => new URL(r.url).pathname)).toEqual(["/api/server", "/api/oauth/token"]);
  });

  it("proves the server before a token another refresh gave goes out", async () => {
    const { t, asked } = setup();
    expect(await t.refresh("pley_at_old")).toBe("pley_at_a");
    expect(asked.map((r) => new URL(r.url).pathname)).toEqual(["/api/server"]);
  });

  it("switches profiles only after a proof made then", async () => {
    const { t, asked, proofs } = setup();
    await t.accessToken();
    await t.switchProfile(2, "1234");
    expect(proofs()).toBe(2);
    expect(asked.map((r) => new URL(r.url).pathname)).toEqual(["/api/server", "/api/server", "/api/oauth/token"]);
    const impostor = setup(() => response(200, { id: ID, proof: "AAAA" }));
    expect(await refusal(impostor.t.switchProfile(2, "1234"))).toBe(NOT_YOUR_SERVER);
    expect(impostor.asked.some((r) => r.url.indexOf("/api/oauth/token") >= 0)).toBe(false);
  });

  it("signs out without sending the token to another device", async () => {
    const { t, asked, store } = setup(() => response(200, { id: ID, proof: "AAAA" }));
    await t.signOut();
    expect(asked.some((r) => r.url.indexOf("/revoke") >= 0)).toBe(false);
    expect(store.get(`pley.tokens.${SERVER}`)).toBeNull();
  });

  it("ends the sign-in before a new one only where the server proves its key", async () => {
    for (const proves of [true, false]) {
      const { t, asked } = setup((c) => response(200, { id: ID, proof: proves ? proofOf(c) : "AAAA" }));
      const fresh: StoredTokens = {
        issuer: SERVER,
        accessToken: "pley_at_c",
        refreshToken: "pley_rt_d",
        expiresAt: 9e15,
        scope: "library play",
        tokenEndpoint: `${SERVER}/api/oauth/token`,
        revocationEndpoint: `${SERVER}/api/oauth/revoke`,
      };
      await t.install(fresh, "library play");
      expect(asked.some((r) => r.url.indexOf("/revoke") >= 0)).toBe(proves);
    }
  });

  it("checks nothing for a sign-in from a server without proofs", async () => {
    const { t, asked, store } = setup();
    const kept = JSON.parse(store.get(`pley.tokens.${SERVER}`)!) as StoredTokens;
    delete kept.serverId;
    delete kept.proofId;
    delete kept.proofKey;
    store.set(`pley.tokens.${SERVER}`, JSON.stringify(kept));
    expect(await t.accessToken()).toBe("pley_at_a");
    expect(asked).toEqual([]);
  });

  it("counts a proof kept wrong as none, as the other apps do", async () => {
    const { t, asked, store } = setup();
    const kept = JSON.parse(store.get(`pley.tokens.${SERVER}`)!) as StoredTokens;
    delete kept.serverId;
    store.set(`pley.tokens.${SERVER}`, JSON.stringify(kept));
    expect(t.load()?.proofKey).toBeUndefined();
    expect(await t.accessToken()).toBe("pley_at_a");
    expect(asked).toEqual([]);
  });
});

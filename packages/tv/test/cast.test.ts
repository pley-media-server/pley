import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { EventStream } from "../src/api/events";
import type { KeyValue, Tokens } from "../src/auth/tokens";
import { castCommand, castIdentity, castQuery, CastReporter, castStateBody, newCastId, type CastStateBody, type CastView } from "../src/cast";
import type { Platform } from "../src/platform";
import { AppState } from "../src/state";
import { castOpening, type Entry, type PageKey } from "../src/ui/shell/navigator";
// Shared with the native Samsung app's tests, which must answer the same.
import fixture from "./fixtures/cast.json";

function memoryStore(): KeyValue {
  const kept = new Map<string, string>();
  return {
    get: (k) => kept.get(k) ?? null,
    set: (k, v) => void kept.set(k, v),
    remove: (k) => void kept.delete(k),
  };
}

describe("casting, as the shared fixture has it", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("query", () => {
    for (const c of fixture.query) {
      expect([c.in, castQuery(c.in as Parameters<typeof castQuery>[0])]).toEqual([c.in, c.out]);
    }
  });

  it("command", () => {
    for (const c of fixture.command) {
      expect([c.in, castCommand(c.in.event, c.in.device)]).toEqual([c.in, c.out]);
    }
  });

  it("state", () => {
    for (const c of fixture.state) {
      expect([c.in, castStateBody(c.in as CastView | null)]).toEqual([c.in, c.out]);
    }
  });

  it("reports", () => {
    for (const c of fixture.reports) {
      const start = Date.now();
      let view: CastView | null = null;
      const sent: { at: number; body: CastStateBody }[] = [];
      const reporter = new CastReporter(
        () => view,
        (body) => sent.push({ at: Date.now() - start, body }),
      );
      for (const step of c.in as { at: number; view?: CastView; seek?: boolean; connected?: boolean; close?: boolean }[]) {
        vi.advanceTimersByTime(start + step.at - Date.now());
        if (step.view) {
          view = step.view;
          reporter.changed();
        }
        if (step.seek || step.connected) reporter.now();
        if (step.close) reporter.close();
      }
      // Nothing more once it closed.
      vi.advanceTimersByTime(60_000);
      expect([c.name, sent]).toEqual([c.name, c.out]);
    }
  });
});

describe("this TV as a device to cast to", () => {
  it("is Samsung TV or LG TV, by its app", () => {
    expect(castIdentity("tizen", "a")).toEqual({ id: "a", name: "Samsung TV", platform: "samsung" });
    expect(castIdentity("webos", "a")).toEqual({ id: "a", name: "LG TV", platform: "lg" });
  });

  it("makes its id once, 32 hex digits, and keeps it", () => {
    expect(newCastId()).toMatch(/^[0-9a-f]{32}$/);
    expect(newCastId()).not.toBe(newCastId());
    const store = memoryStore();
    const platform = { name: "webos", clientId: "test", storage: store } as unknown as Platform;
    const first = new AppState(platform, () => Promise.reject(new Error("no network in tests"))).castIdentity();
    expect(first.id).toMatch(/^[0-9a-f]{32}$/);
    expect(store.get("pley.castDevice")).toBe(first.id);
    expect(new AppState(platform, () => Promise.reject(new Error("no network in tests"))).castIdentity()).toEqual(first);
    // A damaged one is made again.
    store.set("pley.castDevice", "x");
    expect(new AppState(platform, () => Promise.reject(new Error("no network in tests"))).castIdentity().id).toMatch(/^[0-9a-f]{32}$/);
  });

  it("says what plays only while its events list it, one report after the other", async () => {
    const platform = { name: "tizen", clientId: "test", storage: memoryStore() } as unknown as Platform;
    const app = new AppState(platform, () => Promise.reject(new Error("no network in tests")));
    const puts: { path: string; body: unknown }[] = [];
    (app.api as { value: unknown }).value = {
      send: (path: string, opts: { method: string; body: unknown }) => {
        puts.push({ path, body: opts.body });
        return puts.length === 1 ? Promise.reject(new Error("refused")) : Promise.resolve();
      },
    };
    const hidden = app as unknown as { events: { casting: boolean } | null };
    app.castState({ item_id: null });
    hidden.events = { casting: false };
    app.castState({ item_id: null });
    expect(puts).toEqual([]);
    hidden.events = { casting: true };
    app.castState({ item_id: 1, position_ms: 0, duration_ms: null, state: "buffering" });
    app.castState({ item_id: null });
    await Promise.resolve();
    await new Promise((r) => setTimeout(r, 0));
    const id = app.castIdentity().id;
    // A failure is dropped, and the next goes all the same.
    expect(puts).toEqual([
      { path: `/api/cast/devices/${id}/state`, body: { item_id: 1, position_ms: 0, duration_ms: null, state: "buffering" } },
      { path: `/api/cast/devices/${id}/state`, body: { item_id: null } },
    ]);
  });
});

describe("the event stream of a TV that may be cast to", () => {
  const opened: string[] = [];
  let fail: (status: number) => void = () => undefined;

  beforeEach(() => {
    vi.useFakeTimers();
    opened.length = 0;
    class FakeXhr {
      readyState = 0;
      status = 0;
      responseText = "";
      onreadystatechange: (() => void) | null = null;
      onprogress: (() => void) | null = null;
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      onabort: (() => void) | null = null;
      private url = "";
      open(_method: string, url: string): void {
        this.url = url;
      }
      setRequestHeader(): void {}
      send(): void {
        opened.push(this.url);
        fail = (status) => {
          this.status = status;
          this.readyState = 4;
          this.onload?.();
        };
      }
      abort(): void {
        this.onabort?.();
      }
    }
    vi.stubGlobal("XMLHttpRequest", FakeXhr);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  const tokens = { accessToken: () => Promise.resolve("token"), doubtServer: () => undefined } as unknown as Tokens;
  const cast = { id: "0123456789abcdef0123456789abcdef", name: "LG TV", platform: "lg" } as const;

  it("lists the TV, and goes on without it once the server refuses it, until the next start", async () => {
    for (const status of [400, 403, 429]) {
      const stream = new EventStream("https://pley.example.com", tokens, () => undefined, () => undefined, () => undefined, cast);
      opened.length = 0;
      stream.start();
      await vi.advanceTimersByTimeAsync(0);
      fail(status);
      await vi.advanceTimersByTimeAsync(0);
      expect(opened).toEqual([`https://pley.example.com/api/events${castQuery(cast)}`, "https://pley.example.com/api/events"]);
      expect(stream.casting).toBe(false);
      stream.stop();
      stream.start();
      await vi.advanceTimersByTimeAsync(0);
      expect(opened[2]).toBe(`https://pley.example.com/api/events${castQuery(cast)}`);
      stream.stop();
    }
  });

  it("is plain without a device to list", async () => {
    const stream = new EventStream("https://pley.example.com", tokens, () => undefined, () => undefined);
    stream.start();
    await vi.advanceTimersByTimeAsync(0);
    expect(opened).toEqual(["https://pley.example.com/api/events"]);
    stream.stop();
  });
});

describe("where a title cast to the TV opens", () => {
  const stack = (...keys: PageKey[]): Entry[] => keys.map((key, id) => ({ key, id, memory: null, state: {} }));
  const player: PageKey = { name: "player", request: { itemId: 1 }, pressedAt: 0 };

  it("over the page on top, in place of a player, and nowhere before the app is in use", () => {
    expect(castOpening(stack({ name: "home" }))).toBe("open");
    expect(castOpening(stack({ name: "home" }, { name: "item", id: 3 }))).toBe("open");
    expect(castOpening(stack({ name: "settings" }, player))).toBe("replace");
    expect(castOpening(stack({ name: "live" }, { name: "livePlayer", channelId: 2, category: "all", pressedAt: 0 }))).toBe("replace");
    expect(castOpening(stack({ name: "profiles", pin: true }))).toBe(null);
    expect(castOpening(stack({ name: "profiles" }))).toBe(null);
    expect(castOpening(stack({ name: "link", server: "https://pley.example.com" }))).toBe(null);
    expect(castOpening(stack({ name: "servers" }))).toBe(null);
    expect(castOpening([])).toBe(null);
  });
});

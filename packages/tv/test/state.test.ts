import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../src/api/client";
import type { ItemDetail, ServerEvent } from "../src/api/types";
import type { KeyValue } from "../src/auth/tokens";
import type { Platform } from "../src/platform";
import { AppState } from "../src/state";

function memoryStore(): KeyValue {
  const kept = new Map<string, string>();
  return {
    get: (k) => kept.get(k) ?? null,
    set: (k, v) => void kept.set(k, v),
    remove: (k) => void kept.delete(k),
  };
}

/** An AppState with an API that only answers Home, and a way to hand it the server's events. */
function setup() {
  const platform = { name: "tizen", clientId: "test", storage: memoryStore() } as unknown as Platform;
  const app = new AppState(platform, () => Promise.reject(new Error("no network in tests")));
  let homes = 0;
  (app.api as { value: unknown }).value = {
    home: () => {
      homes++;
      return Promise.resolve({ hubs: [] });
    },
    reachable: () => Promise.reject(new Error("nothing answers there")),
  };
  const hear = (e: ServerEvent) => (app as unknown as { handleEvent(e: ServerEvent): void }).handleEvent(e);
  return { app, hear, homes: () => homes };
}

function detail(id: number, parent: number | null = null, grandparent: number | null = null): ItemDetail {
  return { id, parent_id: parent, grandparent_id: grandparent } as ItemDetail;
}

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

describe("watch state changed here or elsewhere", () => {
  it("forgets the items named, and asks again once for a burst", () => {
    const { app, hear, homes } = setup();
    app.keepItem(detail(1, 10, 100));
    app.keepItem(detail(10, 100));
    app.keepItem(detail(2));
    const before = app.changes.value;
    hear({ type: "user_state_changed", item_ids: [1, 10, 100] });
    hear({ type: "library_changed", library_id: 1 });
    vi.advanceTimersByTime(200);
    hear({ type: "user_state_changed", item_ids: [1, 10, 100] });
    expect(app.changes.value).toBe(before);
    vi.advanceTimersByTime(100);
    expect(app.cachedItem(1)).toBeNull();
    expect(app.cachedItem(10)).toBeNull();
    expect(app.cachedItem(2)).not.toBeNull();
    expect(app.changes.value).toBe(before + 1);
    expect(homes()).toBe(1);
    // The library change went with it, not 1.5 s later.
    vi.advanceTimersByTime(2_000);
    expect(app.changes.value).toBe(before + 1);
    expect(homes()).toBe(1);
  });

  it("forgets everything under a show marked as a whole", () => {
    const { app, hear } = setup();
    app.keepItem(detail(1, 10, 100));
    app.keepItem(detail(10, 100));
    app.keepItem(detail(2, 20, 200));
    hear({ type: "user_state_changed", item_ids: [100], children: true });
    vi.advanceTimersByTime(300);
    expect(app.cachedItem(1)).toBeNull();
    expect(app.cachedItem(10)).toBeNull();
    expect(app.cachedItem(2)).not.toBeNull();
  });

  it("waits for the playback on top, then asks again and forgets what it kept", () => {
    const { app, hear, homes } = setup();
    app.keepItem(detail(1));
    app.keepItem(detail(2));
    const before = app.changes.value;
    app.holdRefresh(true);
    hear({ type: "user_state_changed", item_ids: [1] });
    vi.advanceTimersByTime(300);
    expect(app.cachedItem(1)).toBeNull();
    expect(app.changes.value).toBe(before);
    expect(homes()).toBe(0);
    app.holdRefresh(false);
    expect(app.changes.value).toBe(before + 1);
    expect(homes()).toBe(1);
    // Its own reports moved resume points: nothing kept from before it.
    expect(app.cachedItem(2)).toBeNull();
  });

  it("keeps the title tracks were picked for, as picked, and forgets the rest", () => {
    const { app } = setup();
    app.keepItem(detail(1, 10, 100));
    app.keepItem(detail(2, 10, 100));
    const picked = { ...detail(1, 10, 100), title: "with the tracks picked" };
    app.tracksChanged(picked);
    expect(app.cachedItem(1)).toBe(picked);
    // Another episode of the show starts with the choice too: asked again.
    expect(app.cachedItem(2)).toBeNull();
    // New defaults: every title is asked again.
    app.tracksChanged(null);
    expect(app.cachedItem(1)).toBeNull();
  });

  it("says the server is out of reach for 6 s, then keeps the mark until it answers", () => {
    const { app } = setup();
    app.serverAway();
    expect(app.offline.value).toBe(true);
    expect(app.offlineNotice.value).toBe(true);
    vi.advanceTimersByTime(6_000);
    expect(app.offlineNotice.value).toBe(false);
    expect(app.offline.value).toBe(true);
    app.serverAnswered();
    expect(app.offline.value).toBe(false);
    // Going out of reach again says so again.
    app.serverAway();
    expect(app.offlineNotice.value).toBe(true);
  });

  it("keeps an item's details for 5 minutes", () => {
    const { app } = setup();
    app.keepItem(detail(3));
    vi.advanceTimersByTime(4 * 60_000);
    expect(app.cachedItem(3)).not.toBeNull();
    vi.advanceTimersByTime(2 * 60_000);
    expect(app.cachedItem(3)).toBeNull();
  });
});

describe("who is watching and the libraries", () => {
  const me = { id: 1, name: "Ana" };
  const libraries = [{ id: 1, name: "Movies", kind: "movies" }];
  function signedIn(store: KeyValue, answer: () => Promise<unknown>) {
    const platform = { name: "tizen", clientId: "test", storage: store } as unknown as Platform;
    const app = new AppState(platform, () => Promise.reject(new Error("no network in tests")));
    (app.api as { value: unknown }).value = {
      me: () => answer().then(() => me),
      libraries: () => answer().then(() => libraries),
      profiles: () => Promise.resolve([]),
      home: () => Promise.resolve({ hubs: [] }),
      reachable: () => Promise.reject(new Error("nothing answers there")),
    };
    return app;
  }

  it("are kept, so the next start has them before the server answers", async () => {
    const store = memoryStore();
    await signedIn(store, () => Promise.resolve()).loadAccount();
    const next = signedIn(store, () => new Promise(() => undefined));
    expect(next.me.value).toEqual(me);
    expect(next.libraries.value).toEqual(libraries);
  });

  it("keep a failure for the library pages' Retry, and what was saved stays", async () => {
    const store = memoryStore();
    await signedIn(store, () => Promise.resolve()).loadAccount();
    const app = signedIn(store, () => Promise.reject(new Error("refused")));
    await expect(app.loadAccount()).rejects.toThrow("refused");
    expect(app.accountError.value).toBeInstanceOf(Error);
    expect(app.libraries.value).toEqual(libraries);
  });

  it("start as the profile the TV was left on with Always start as, its PIN first when it has one", () => {
    const store = memoryStore();
    const server = "http://a.local:7539";
    store.set("pley.server", server);
    store.set(`pley.tokens.${server}`, JSON.stringify({ issuer: server, accessToken: "pley_at_0", refreshToken: "pley_rt_0" }));
    store.set(`pley.household.${server}`, "2");
    const start = () => signedIn(store, () => Promise.resolve()).start();
    // Who the TV was left on was never heard: whether it has a PIN isn't known.
    store.set(`pley.alwaysStartAs.${server}`, "2");
    expect(start()).toEqual({ page: "profiles" });
    store.set("pley.me", JSON.stringify({ ...me, has_pin: true }));
    expect(start()).toEqual({ page: "pin" });
    store.set("pley.me", JSON.stringify({ ...me, has_pin: false }));
    expect(start()).toEqual({ page: "home" });
    store.remove(`pley.alwaysStartAs.${server}`);
    expect(start()).toEqual({ page: "profiles" });
  });

  it("go with the profile", async () => {
    const store = memoryStore();
    const app = signedIn(store, () => Promise.resolve());
    await app.loadAccount();
    (app as unknown as { forgetAccount(): void }).forgetAccount();
    expect(store.get("pley.me")).toBeNull();
    expect(store.get("pley.libraries")).toBeNull();
  });

  it("go with the server when another is chosen, and each server keeps its own household", async () => {
    const store = memoryStore();
    store.set("pley.server", "http://a.local:7539");
    store.set("pley.household", "3");
    const app = signedIn(store, () => Promise.resolve());
    expect(store.get("pley.household")).toBeNull();
    expect(app.householdSize()).toBe(3);
    await app.loadAccount();
    app.keepHousehold([{ id: 1 }, { id: 2 }] as never);
    app.keepItem(detail(42));
    app.profileGatePassed = true;
    app.use("http://b.local:7539");
    expect(app.me.value).toBeNull();
    expect(app.libraries.value).toBeNull();
    expect(app.home.value).toBeNull();
    expect(app.cachedItem(42)).toBeNull();
    expect(app.profileGatePassed).toBe(false);
    expect(store.get("pley.me")).toBeNull();
    expect(app.householdSize()).toBe(1);
    expect(app.savedHousehold()).toEqual([]);
    app.use("http://a.local:7539");
    expect(app.householdSize()).toBe(2);
  });

  it("are left out when what was saved isn't what it should be", () => {
    const store = memoryStore();
    store.set("pley.servers", "{}");
    store.set("pley.me", "[1]");
    store.set("pley.libraries", '"x"');
    store.set("pley.home", "{}");
    const app = signedIn(store, () => new Promise(() => undefined));
    expect(app.savedServers()).toEqual([]);
    expect(app.me.value).toBeNull();
    expect(app.libraries.value).toBeNull();
    expect(app.home.value).toBeNull();
  });
});

describe("the quality setting", () => {
  const open = (store: KeyValue) => new AppState({ name: "tizen", clientId: "test", storage: store } as unknown as Platform, () => Promise.reject(new Error("no network in tests")));

  it("is Automatic on a TV where none was picked", () => {
    expect(open(memoryStore()).settings.value.quality).toBe("auto");
  });

  it("is Automatic where Original was saved only as the old default, along with another setting", () => {
    const store = memoryStore();
    store.set("pley.settings", JSON.stringify({ autoplayNext: false, quality: "original" }));
    const app = open(store);
    expect(app.settings.value.quality).toBe("auto");
    expect(app.settings.value.autoplayNext).toBe(false);
  });

  it("keeps a quality picked, Original included", () => {
    const store = memoryStore();
    open(store).updateSettings({ quality: "original" });
    expect(open(store).settings.value.quality).toBe("original");
    open(store).updateSettings({ autoplayNext: false });
    expect(open(store).settings.value.quality).toBe("original");
    const other = memoryStore();
    other.set("pley.settings", JSON.stringify({ quality: "8m" }));
    expect(open(other).settings.value.quality).toBe("8m");
  });
});

describe("Live TV", () => {
  const platform = (store: KeyValue) => ({ name: "tizen", clientId: "test", storage: store }) as unknown as Platform;
  const summary = { available: true, allowed: true, channels: 5, sources: 1, recent: [3] };

  it("shows where the server says so, kept for the next launch, and goes on a server without it", async () => {
    const store = memoryStore();
    const app = new AppState(platform(store), () => Promise.reject(new Error("no network in tests")));
    let answer: () => Promise<unknown> = () => Promise.resolve(summary);
    (app.api as { value: unknown }).value = { liveSummary: () => answer() };
    await app.loadLive();
    expect(app.live.value).toEqual(summary);
    expect(new AppState(platform(store), () => Promise.reject(new Error("no network"))).live.value).toEqual(summary);
    // Out of reach: what was kept stays.
    answer = () => Promise.reject(new Error("nothing answers there"));
    await app.loadLive();
    expect(app.live.value).toEqual(summary);
    // A server without Live TV.
    answer = () => Promise.reject(new ApiError(404, "not_found", "no route"));
    await app.loadLive();
    expect(app.live.value).toBeNull();
    expect(store.get("pley.live")).toBeNull();
  });

  it("asks again when its channels change on the server", () => {
    const { app, hear } = setup();
    let asked = 0;
    (app.api.value as unknown as { liveSummary: () => Promise<unknown> }).liveSummary = () => {
      asked++;
      return Promise.resolve(summary);
    };
    const before = app.liveChanges.value;
    hear({ type: "live_channels_changed" });
    expect(app.liveChanges.value).toBe(before + 1);
    expect(asked).toBe(1);
  });
});

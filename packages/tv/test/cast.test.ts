import { afterEach, describe, expect, it, vi } from "vitest";
import { ApiError } from "../src/api/client";
import { EventStream } from "../src/api/events";
import type { ItemDetail, ItemKind, ItemSummary, LiveSummary, Playlist, PlaylistEntry } from "../src/api/types";
import type { KeyValue, Tokens } from "../src/auth/tokens";
import type { Platform } from "../src/platform";
import { CAST_BANNER_MS, CAST_SWITCH_WAIT_MS, CastReceiver, NOTHING_TO_PLAY, receiverQuery, SWITCH_FAILED, type CastEnv, type CastEvent, type CastServer } from "../src/player/cast";
import type { PlayRequest } from "../src/player/plan";
import { AppState } from "../src/state";

function summary(id: number, kind: ItemKind, over: Partial<ItemSummary> = {}): ItemSummary {
  return { id, library_id: 1, kind, title: `${kind} ${id}`, sort_title: `${kind} ${id}`, added_at: 0, user_state: { position_ms: 0, watched: false, play_count: 0 }, ...over } as ItemSummary;
}

function detail(id: number, kind: ItemKind, over: Partial<ItemDetail> = {}): ItemDetail {
  return { ...summary(id, kind), original_title: null, tagline: null, genres: [], studio: null, content_rating: null, files: [], ...over } as ItemDetail;
}

function cast(over: Partial<CastEvent> = {}): CastEvent {
  return { type: "cast", receiver_id: "tizen-1", item_id: null, playlist_id: null, channel_id: null, start_ms: null, shuffle: false, from: "Living room phone", ...over };
}

/**
 * A TV taking casts: a server whose items, children, next episodes and
 * playlists the test gives, and what the TV did with them, in order.
 */
function tv(shown?: () => boolean) {
  const items = new Map<number, ItemDetail>();
  const children = new Map<number, ItemSummary[]>();
  const onDeck = new Map<number, ItemSummary | null>();
  const playlists = new Map<number, Playlist & { entries: PlaylistEntry[] }>();
  const did: string[] = [];
  const videos: PlayRequest[] = [];
  const queues: { tracks: number[]; at: number; shuffle: boolean; fromMs: number }[] = [];
  const said: string[] = [];
  /**
   * The profile the app acts as; a switch's answer, held until `release` when the test holds it, refused when
   * `refuse` says so, and held again once the app acts as the new profile when `afterSwitched` holds it.
   */
  const state = {
    shown: true,
    held: null as null | Promise<void>,
    profile: 2 as number | null,
    switchHeld: null as null | Promise<void>,
    refuse: null as null | Error,
    afterSwitched: null as null | Promise<void>,
  };
  const wait = async () => {
    if (state.held) await state.held;
  };
  const server: CastServer = {
    item: async (id) => {
      did.push(`item ${id}`);
      await wait();
      const d = items.get(id);
      if (!d) throw new ApiError(404, "not_found", "item not found");
      return d;
    },
    onDeck: async (id) => {
      did.push(`on deck ${id}`);
      return { item: onDeck.get(id) ?? null };
    },
    children: async (id) => {
      did.push(`children ${id}`);
      return { items: children.get(id) ?? [] };
    },
    playlist: async (id) => {
      did.push(`playlist ${id}`);
      const p = playlists.get(id);
      if (!p) throw new ApiError(404, "not_found", "playlist not found");
      return p;
    },
  };
  const env: CastEnv = {
    id: () => "tizen-1",
    shown: shown ?? (() => state.shown),
    profile: () => state.profile,
    switchTo: async (profile, ticket, switched) => {
      did.push(`switch to ${profile} with ${ticket}`);
      if (state.switchHeld) await state.switchHeld;
      if (state.refuse) throw state.refuse;
      state.profile = profile;
      switched();
      if (state.afterSwitched) await state.afterSwitched;
    },
    server: () => server,
    video: (r) => {
      did.push("video");
      videos.push(r);
    },
    music: (tracks, at, shuffle, fromMs) => {
      did.push("music");
      queues.push({ tracks: tracks.map((t) => t.id), at, shuffle, fromMs });
    },
    channel: (id) => did.push(`channel ${id}`),
    say: (text) => said.push(text),
    failed: (e) => said.push(`failed: ${(e as Error).message}`),
    random: () => 0.5,
  };
  const receiver = new CastReceiver(env);
  return { items, children, onDeck, playlists, did, videos, queues, said, state, receiver };
}

const settle = () => new Promise((r) => setTimeout(r, 0));

describe("a cast to this TV", () => {
  it("plays a movie or an episode from where it was sent, else where Play resumes, and says where it came from", async () => {
    const t = tv();
    t.items.set(5, detail(5, "movie"));
    t.items.set(6, detail(6, "episode"));
    t.receiver.take(cast({ item_id: 5, start_ms: 60_000 }));
    await settle();
    t.receiver.take(cast({ item_id: 6, from: null }));
    await settle();
    expect(t.videos).toEqual([
      { itemId: 5, startMs: 60_000 },
      { itemId: 6, fromStart: false },
    ]);
    expect(t.said).toEqual(["Playing from Living room phone", "Playing from another device"]);
    expect(CAST_BANNER_MS).toBe(4_000);
  });

  it("plays a movie or an episode with the tracks picked on the remote's page, for that play", async () => {
    const t = tv();
    t.items.set(5, detail(5, "movie"));
    t.items.set(6, detail(6, "episode"));
    t.receiver.take(cast({ item_id: 5, start_ms: 60_000, audio_index: 2, subtitle_index: -1 }));
    await settle();
    t.receiver.take(cast({ item_id: 6, audio_index: null, subtitle_index: 4 }));
    await settle();
    expect(t.videos).toEqual([
      { itemId: 5, startMs: 60_000, audioIndex: 2, subtitleIndex: -1 },
      { itemId: 6, fromStart: false, subtitleIndex: 4 },
    ]);
  });

  it("plays a show's or a season's next episode, as their Play does, or says there is nothing to play", async () => {
    const t = tv();
    t.items.set(20, detail(20, "show"));
    t.items.set(21, detail(21, "season"));
    t.onDeck.set(20, summary(23, "episode"));
    t.receiver.take(cast({ item_id: 20 }));
    await settle();
    t.receiver.take(cast({ item_id: 21 }));
    await settle();
    expect(t.videos).toEqual([{ itemId: 23, fromStart: false }]);
    expect(t.said).toEqual(["Playing from Living room phone", NOTHING_TO_PLAY]);
  });

  it("plays an album's tracks in order, or shuffled from a random one, as the music's queue", async () => {
    const t = tv();
    t.items.set(30, detail(30, "album"));
    t.children.set(30, [summary(31, "track"), summary(32, "track"), summary(33, "track")]);
    t.receiver.take(cast({ item_id: 30 }));
    await settle();
    t.receiver.take(cast({ item_id: 30, shuffle: true }));
    await settle();
    expect(t.queues).toEqual([
      { tracks: [31, 32, 33], at: 0, shuffle: false, fromMs: 0 },
      { tracks: [31, 32, 33], at: 1, shuffle: true, fromMs: 0 },
    ]);
    expect(t.said).toEqual(["Playing from Living room phone", "Playing from Living room phone"]);
  });

  it("plays an artist's tracks album by album, as the artist's Play does", async () => {
    const t = tv();
    t.items.set(40, detail(40, "artist"));
    t.children.set(40, [summary(41, "album"), summary(42, "album")]);
    t.children.set(41, [summary(43, "track"), summary(44, "track")]);
    t.children.set(42, [summary(45, "track")]);
    t.receiver.take(cast({ item_id: 40 }));
    await settle();
    expect(t.queues).toEqual([{ tracks: [43, 44, 45], at: 0, shuffle: false, fromMs: 0 }]);
  });

  it("plays a track in its album from it, at the place sent; one without an album alone", async () => {
    const t = tv();
    t.items.set(52, detail(52, "track", { parent_id: 50 }));
    t.items.set(60, detail(60, "track"));
    t.children.set(50, [summary(51, "track"), summary(52, "track"), summary(53, "track")]);
    t.receiver.take(cast({ item_id: 52, start_ms: 30_000, shuffle: true }));
    await settle();
    t.receiver.take(cast({ item_id: 60 }));
    await settle();
    expect(t.queues).toEqual([
      { tracks: [51, 52, 53], at: 1, shuffle: false, fromMs: 30_000 },
      { tracks: [60], at: 0, shuffle: false, fromMs: 0 },
    ]);
  });

  it("plays a playlist as its page does: music as the queue, video in the player through the playlist", async () => {
    const t = tv();
    const entries = (ids: number[], kind: ItemKind) => ids.map((id, i) => ({ entry_id: 100 + i, item: summary(id, kind) }));
    t.playlists.set(3, { id: 3, title: "Songs", kind: "audio", item_count: 3, duration_ms: 0, artwork: [], smart_filter: null, entries: entries([71, 72, 73], "track") });
    t.playlists.set(4, { id: 4, title: "Films", kind: "video", item_count: 2, duration_ms: 0, artwork: [], smart_filter: null, entries: entries([81, 82], "movie") });
    t.playlists.set(5, { id: 5, title: "Empty", kind: "video", item_count: 0, duration_ms: 0, artwork: [], smart_filter: null, entries: [] });
    t.receiver.take(cast({ playlist_id: 3, shuffle: true }));
    await settle();
    t.receiver.take(cast({ playlist_id: 4 }));
    await settle();
    t.receiver.take(cast({ playlist_id: 5 }));
    await settle();
    expect(t.queues).toEqual([{ tracks: [71, 72, 73], at: 1, shuffle: true, fromMs: 0 }]);
    expect(t.videos).toEqual([{ itemId: 81, queue: { kind: "playlist", id: 4 } }]);
    expect(t.said).toEqual(["Playing from Living room phone", "Playing from Living room phone", NOTHING_TO_PLAY]);
  });

  it("tunes a channel in the live player at once", () => {
    const t = tv();
    t.receiver.take(cast({ channel_id: 7, from: "Firefox on Windows" }));
    expect(t.did).toEqual(["channel 7"]);
  });

  it("does nothing for another TV, nor while the app doesn't show", async () => {
    const t = tv();
    t.items.set(5, detail(5, "movie"));
    t.receiver.take(cast({ item_id: 5, receiver_id: "tizen-2" }));
    t.state.shown = false;
    t.receiver.take(cast({ item_id: 5 }));
    await settle();
    expect([t.did, t.said]).toEqual([[], []]);
  });

  it("drops a cast hidden away, or overtaken by a newer one, while what it plays is fetched", async () => {
    const t = tv();
    t.items.set(5, detail(5, "movie"));
    t.items.set(6, detail(6, "movie"));
    let release: () => void = () => undefined;
    t.state.held = new Promise<void>((r) => (release = r));
    t.receiver.take(cast({ item_id: 5 }));
    t.state.held = null;
    t.receiver.take(cast({ item_id: 6 }));
    release();
    await settle();
    expect(t.videos).toEqual([{ itemId: 6, fromStart: false }]);
    t.state.held = new Promise<void>((r) => (release = r));
    t.receiver.take(cast({ item_id: 5 }));
    t.state.shown = false;
    release();
    await settle();
    expect(t.videos).toHaveLength(1);
    expect(t.said).toEqual(["Playing from Living room phone"]);
  });

  it("says why when what it was sent can't be found", async () => {
    const t = tv();
    t.receiver.take(cast({ item_id: 9 }));
    await settle();
    expect(t.said).toEqual(["failed: item not found"]);
  });
});

describe("a cast from another profile's remote", () => {
  it("switches to the remote's profile with its ticket, then plays it once", async () => {
    const t = tv();
    t.items.set(5, detail(5, "movie"));
    t.receiver.take(cast({ item_id: 5, start_ms: 60_000, profile_id: 4, switch_ticket: "t1" }));
    await settle();
    expect(t.did).toEqual(["switch to 4 with t1", "item 5", "video"]);
    expect(t.videos).toEqual([{ itemId: 5, startMs: 60_000 }]);
    expect(t.said).toEqual(["Playing from Living room phone"]);
  });

  it("plays at once when the TV is on that profile already, and does nothing without a ticket", async () => {
    const t = tv();
    t.items.set(5, detail(5, "movie"));
    t.receiver.take(cast({ item_id: 5, profile_id: 3 }));
    t.receiver.take(cast({ item_id: 5, profile_id: 2, switch_ticket: "t1" }));
    await settle();
    expect(t.did).toEqual(["item 5", "video"]);
  });

  it("says the switch failed and plays nothing, nor says anything once signed out", async () => {
    const { app } = signedIn();
    // Taking casts as the app does: shown, signed in, not signed out.
    const t = tv(() => app.visible.value && app.signedIn && !app.signedOut.value);
    t.items.set(5, detail(5, "movie"));
    t.state.refuse = new Error("this cast's switch has expired or was used: cast again");
    t.receiver.take(cast({ item_id: 5, profile_id: 4, switch_ticket: "t1" }));
    await settle();
    expect([t.did, t.said]).toEqual([["switch to 4 with t1"], [SWITCH_FAILED]]);
    expect(t.state.profile).toBe(2);
    let release: () => void = () => undefined;
    t.state.switchHeld = new Promise<void>((r) => (release = r));
    t.receiver.take(cast({ item_id: 5, profile_id: 4, switch_ticket: "t2" }));
    await app.signOut();
    release();
    await settle();
    expect([app.signedIn, app.signedOut.value]).toEqual([false, true]);
    expect(t.said).toEqual([SWITCH_FAILED]);
  });

  it("drops every cast heard while the switch is made: the left profile's, the same remote's next, another profile's", async () => {
    const t = tv();
    t.items.set(5, detail(5, "movie"));
    t.items.set(6, detail(6, "movie"));
    let release: () => void = () => undefined;
    t.state.switchHeld = new Promise<void>((r) => (release = r));
    t.receiver.take(cast({ item_id: 5, profile_id: 4, switch_ticket: "t1" }));
    t.receiver.take(cast({ item_id: 6, from: "Kitchen tablet" }));
    t.receiver.take(cast({ item_id: 6, profile_id: 4, switch_ticket: "t2" }));
    t.receiver.take(cast({ channel_id: 7, profile_id: 3, switch_ticket: "t3" }));
    release();
    await settle();
    expect(t.did).toEqual(["switch to 4 with t1", "item 5", "video"]);
    expect(t.videos).toEqual([{ itemId: 5, fromStart: false }]);
    expect(t.said).toEqual(["Playing from Living room phone"]);
  });

  it("drops a cast heard while a failed switch is made, and takes the next as before", async () => {
    const t = tv();
    t.items.set(6, detail(6, "movie"));
    let release: () => void = () => undefined;
    t.state.switchHeld = new Promise<void>((r) => (release = r));
    t.state.refuse = new Error("this cast's switch has expired or was used: cast again");
    t.receiver.take(cast({ item_id: 5, profile_id: 4, switch_ticket: "t1" }));
    t.receiver.take(cast({ item_id: 6, from: "Kitchen tablet" }));
    release();
    await settle();
    expect([t.did, t.said]).toEqual([["switch to 4 with t1"], [SWITCH_FAILED]]);
    t.receiver.take(cast({ item_id: 6, from: "Kitchen tablet" }));
    await settle();
    expect(t.videos).toEqual([{ itemId: 6, fromStart: false }]);
  });

  it("plays the new profile's own cast heard the moment it is switched to, in place of the one that switched", async () => {
    const t = tv();
    t.items.set(5, detail(5, "movie"));
    t.items.set(6, detail(6, "movie"));
    let release: () => void = () => undefined;
    // The app acts as profile 4 now; its Home is still to show.
    t.state.afterSwitched = new Promise<void>((r) => (release = r));
    t.receiver.take(cast({ item_id: 5, profile_id: 4, switch_ticket: "t1" }));
    await settle();
    expect(t.state.profile).toBe(4);
    t.receiver.take(cast({ item_id: 6, from: "Kitchen tablet" }));
    release();
    await settle();
    expect(t.did).toEqual(["switch to 4 with t1", "item 6", "video"]);
    expect(t.videos).toEqual([{ itemId: 6, fromStart: false }]);
    expect(t.said).toEqual(["Playing from Kitchen tablet"]);
  });
});

/**
 * A signed-in app on profile 2 whose switch to another profile makes the
 * tokens that profile's; `report` queues a report of what plays, which says,
 * as it goes out, the token it goes with.
 */
function signedIn() {
  const did: string[] = [];
  let token = "pley_at_2";
  const tokens = {
    signedIn: true,
    accessToken: () => Promise.resolve(token),
    switchProfile: async (profile: number, _pin: string | null, ticket: string | null) => {
      did.push(`switch to ${profile} with ${ticket}`);
      token = `pley_at_${profile}`;
    },
    signOut: async () => {
      tokens.signedIn = false;
    },
  };
  const app = new AppState({ name: "tizen", clientId: "tizen", storage: memoryStore() } as unknown as Platform, () => Promise.reject(new Error("no network in tests")));
  (app as unknown as { tokens: unknown }).tokens = tokens;
  const report = (what: string) =>
    app.reporter.send(async () => {
      did.push(`${what} with ${await tokens.accessToken()}`);
    });
  return { app, did, report };
}

describe("the switch a cast from another profile makes", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  /** A playback whose end reports its last position, `stopped`, and its session's end; `hung` holds the first. */
  function playing(t: ReturnType<typeof signedIn>, hung?: Promise<void>) {
    let ended = false;
    t.app.onLeave(() => {
      if (ended) return;
      ended = true;
      t.did.push("end");
      if (hung) t.app.reporter.send(() => hung);
      else t.report("progress");
      t.report("stopped");
      t.report("session end");
    });
  }

  it("ends what plays, sends its last reports with the profile before's token, then redeems the ticket", async () => {
    const t = signedIn();
    playing(t);
    await t.app.switchForCast(4, "t1");
    expect(t.did).toEqual(["end", "progress with pley_at_2", "stopped with pley_at_2", "session end with pley_at_2", "switch to 4 with t1"]);
    t.report("progress");
    await settle();
    expect(t.did.slice(-1)).toEqual(["progress with pley_at_4"]);
  });

  it("waits 5 s for those reports, then drops what hasn't gone rather than send it with the new token", async () => {
    vi.useFakeTimers();
    expect(CAST_SWITCH_WAIT_MS).toBe(5_000);
    const t = signedIn();
    let answer: () => void = () => undefined;
    // The server doesn't answer the last progress report.
    playing(t, new Promise<void>((r) => (answer = r)));
    const switching = t.app.switchForCast(4, "t1");
    await vi.advanceTimersByTimeAsync(4_999);
    expect(t.did).toEqual(["end"]);
    await vi.advanceTimersByTimeAsync(1);
    await switching;
    expect(t.did).toEqual(["end", "switch to 4 with t1"]);
    answer();
    await vi.advanceTimersByTimeAsync(0);
    expect(t.did).toEqual(["end", "switch to 4 with t1"]);
    // The new profile's own reports go as ever.
    t.report("progress");
    await vi.advanceTimersByTimeAsync(0);
    expect(t.did).toEqual(["end", "switch to 4 with t1", "progress with pley_at_4"]);
  });
});

function memoryStore(): KeyValue {
  const kept = new Map<string, string>();
  return { get: (k) => kept.get(k) ?? null, set: (k, v) => void kept.set(k, v), remove: (k) => void kept.delete(k) };
}

describe("this TV as a receiver", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it("names itself by its install id, a TV taking titles, and channels where Live TV shows; the server names it", () => {
    expect(receiverQuery("tizen-0a1b", false)).toBe("receiver=tizen-0a1b&device_kind=tv&casts=item,switch");
    expect(receiverQuery("tizen-0a1b", true)).toBe("receiver=tizen-0a1b&device_kind=tv&casts=item,live,switch");
  });

  it("opens the event stream as one, and opens it again when Live TV comes or goes", async () => {
    vi.useFakeTimers();
    const opened: string[] = [];
    class FakeXhr {
      readyState = 0;
      status = 0;
      responseText = "";
      onreadystatechange: (() => void) | null = null;
      onprogress: (() => void) | null = null;
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      onabort: (() => void) | null = null;
      open(_method: string, url: string): void {
        opened.push(url);
      }
      setRequestHeader(): void {}
      send(): void {}
      abort(): void {
        this.onabort?.();
      }
    }
    vi.stubGlobal("XMLHttpRequest", FakeXhr);
    const store = memoryStore();
    store.set("pley.installId", "tizen-0a1b");
    const app = new AppState({ name: "tizen", clientId: "tizen", storage: store } as unknown as Platform, () => Promise.reject(new Error("no network in tests")));
    let live: LiveSummary = { available: false, allowed: false, channels: 0, sources: 0, recent: [] };
    (app.api as { value: unknown }).value = { liveSummary: () => Promise.resolve(live) };
    const hidden = app as unknown as { tokens: unknown };
    hidden.tokens = { signedIn: true, accessToken: () => Promise.resolve("token") } as unknown as Tokens;
    app.server.value = "https://pley.example.com";
    app.listen(true);
    await vi.advanceTimersByTimeAsync(0);
    // The same, known already: nothing opens again.
    await app.loadLive();
    await vi.advanceTimersByTimeAsync(0);
    live = { ...live, available: true, allowed: true, channels: 12 };
    await app.loadLive();
    await vi.advanceTimersByTimeAsync(0);
    expect(opened).toEqual([
      "https://pley.example.com/api/events?receiver=tizen-0a1b&device_kind=tv&casts=item,switch",
      "https://pley.example.com/api/events?receiver=tizen-0a1b&device_kind=tv&casts=item,live,switch",
    ]);
    app.listen(false);
  });

  it("asks for the stream once when started over while its token comes", async () => {
    vi.useFakeTimers();
    let sent = 0;
    class FakeXhr {
      onabort: (() => void) | null = null;
      open(): void {}
      setRequestHeader(): void {}
      send(): void {
        sent++;
      }
      abort(): void {
        this.onabort?.();
      }
    }
    vi.stubGlobal("XMLHttpRequest", FakeXhr);
    let answer: (t: string) => void = () => undefined;
    const tokens = { accessToken: () => new Promise<string>((r) => (answer = r)) } as unknown as Tokens;
    const stream = new EventStream("https://pley.example.com", tokens, () => undefined, () => undefined);
    stream.start();
    const first = answer;
    stream.restart();
    first("token");
    answer("token");
    await vi.advanceTimersByTimeAsync(0);
    expect(sent).toBe(1);
    stream.stop();
  });
});

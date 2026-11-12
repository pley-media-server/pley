import { describe, expect, it } from "vitest";
import type { MediaFile } from "../../src/api/types";
import { AutoQuality, MEMORY_MS, storedMemory, type AutoMemory, type Remembered } from "../../src/player/autoquality";
import { AUTO, autoLadder, chooseQuality, modeOf, namedFor, offeredQualities, ORIGINAL, presetById, PRESETS, qualitiesFor, serverQuality } from "../../src/player/quality";

function file(width: number | null, height: number | null, mbps: number | null): MediaFile {
  return {
    id: 1,
    path: "/f.mkv",
    size: 1,
    container: "mkv",
    duration_ms: 7_200_000,
    bitrate: mbps === null ? null : mbps * 1_000_000,
    width,
    height,
    video_codec: "hevc",
    audio_codec: "eac3",
    streams: [],
    edition: null,
    selected_audio: null,
    selected_subtitle: null,
    chapters: [],
  } as MediaFile;
}

const ids = (list: { id: string }[]) => list.map((q) => q.id);

describe("the Quality menu", () => {
  it("lists Original, Automatic, the 4K qualities, then 1080p and below", () => {
    expect(PRESETS.map((p) => [p.id, p.label])).toEqual([
      ["original", "Original"],
      ["auto", "Automatic"],
      ["30m", "30 Mbps 4K"],
      ["15m", "15 Mbps 4K"],
      ["20m", "20 Mbps 1080p"],
      ["12m", "12 Mbps 1080p"],
      ["8m", "8 Mbps 1080p"],
      ["4m", "4 Mbps 720p"],
      ["2m", "2 Mbps 480p"],
    ]);
  });

  it("always offers Original and Automatic, and qualities that make a file smaller: fewer lines, or 80 % of its bitrate at most", () => {
    // 8 Mbps is too close to a 9 Mbps file to play any better; to a 10 Mbps one it isn't.
    expect(ids(qualitiesFor(file(1920, 1080, 9)))).toEqual(["original", "auto", "4m", "2m"]);
    expect(ids(qualitiesFor(file(1920, 1080, 10)))).toEqual(["original", "auto", "8m", "4m", "2m"]);
    expect(ids(qualitiesFor(file(720, 480, 1.5)))).toEqual(["original", "auto"]);
    expect(ids(qualitiesFor(file(3840, 2160, 60)))).toEqual(["original", "auto", "30m", "15m", "20m", "12m", "8m", "4m", "2m"]);
    // Every one, by the table's names, without a file (Settings' default quality).
    expect(qualitiesFor(null)).toEqual(PRESETS);
  });

  it("names a quality taller than the file by the file's own lines", () => {
    const named = (f: MediaFile) => qualitiesFor(f).map((q) => q.label);
    expect(named(file(1280, 720, 12))).toEqual(["Original", "Automatic", "8 Mbps 720p", "4 Mbps 720p", "2 Mbps 480p"]);
    // A film 800 lines tall in 1080p's width is 1080p: its 4K qualities are named 1080p, and fall in among them.
    expect(named(file(1920, 800, 25))).toEqual(["Original", "Automatic", "20 Mbps 1080p", "15 Mbps 1080p", "12 Mbps 1080p", "8 Mbps 1080p", "4 Mbps 720p", "2 Mbps 480p"]);
    // And so wherever the player names it (Automatic's line, its notice), but not without a file.
    expect(namedFor(file(1280, 720, 12), presetById("8m")!).label).toBe("8 Mbps 720p");
    expect(namedFor(file(3840, 2160, 60), presetById("8m")!).label).toBe("8 Mbps 1080p");
    expect(namedFor(null, presetById("8m")!).label).toBe("8 Mbps 1080p");
  });

  it("shows two qualities that come to the same bitrate and lines once", () => {
    const table = [ORIGINAL, AUTO, { id: "8m", label: "8 Mbps 1080p", maxBitrateKbps: 8000, maxHeight: 1080 }, { id: "8s", label: "8 Mbps 720p", maxBitrateKbps: 8000, maxHeight: 720 }];
    expect(qualitiesFor(file(1280, 720, 12), table).map((q) => [q.id, q.label])).toEqual([
      ["original", "Original"],
      ["auto", "Automatic"],
      ["8m", "8 Mbps 720p"],
    ]);
  });

  it("starts with Automatic when nothing was picked, and keeps a pick, Original included", () => {
    const offered = qualitiesFor(file(1920, 1080, 12));
    expect(chooseQuality(null, offered)).toBe(AUTO);
    expect(chooseQuality("nonsense", offered)).toBe(AUTO);
    expect(chooseQuality("original", offered)).toBe(ORIGINAL);
    expect(chooseQuality("auto", offered)).toBe(AUTO);
    expect(chooseQuality("8m", offered).id).toBe("8m");
    // A quality this file doesn't need plays the original.
    expect(chooseQuality("20m", offered)).toBe(ORIGINAL);
  });

  it("offers the original alone on a server that converts no video", () => {
    const film = file(3840, 2160, 60);
    expect(offeredQualities(film, true)).toEqual([ORIGINAL]);
    expect(offeredQualities(null, true)).toEqual([ORIGINAL]);
    expect(offeredQualities(film, false)).toEqual(qualitiesFor(film));
  });

  it("asks the server to choose for the original and Automatic, and to convert the rest", () => {
    expect(modeOf(ORIGINAL)).toBe("auto");
    expect(modeOf(AUTO)).toBe("auto");
    expect(modeOf(PRESETS[6]!)).toBe("transcode");
  });
});

describe("the server's own quality for a file", () => {
  it("matches the server's closest_quality", () => {
    const pick = (w: number, h: number, mbps: number) => serverQuality(file(w, h, mbps)).id;
    expect(pick(3840, 2160, 57)).toBe("30m");
    expect(pick(3840, 2160, 18)).toBe("15m");
    expect(pick(3840, 2160, 6)).toBe("15m");
    expect(pick(3840, 1600, 40)).toBe("30m");
    expect(pick(1920, 1080, 25)).toBe("20m");
    expect(pick(1920, 1080, 10)).toBe("12m");
    expect(pick(1920, 1080, 4)).toBe("8m");
    expect(pick(1920, 800, 9)).toBe("12m");
    expect(pick(1280, 720, 3)).toBe("4m");
    expect(pick(720, 480, 2)).toBe("2m");
    expect(serverQuality(file(null, null, null)).id).toBe("20m");
  });
});

describe("Automatic's steps", () => {
  it("go down in bitrate, never above the server's quality nor the file's lines", () => {
    expect(ids(autoLadder(file(3840, 2160, 60)))).toEqual(["30m", "15m", "12m", "8m", "4m", "2m"]);
    // 15 Mbps is too close to an 18 Mbps file to play any better: 12 Mbps 1080p is the first step.
    expect(ids(autoLadder(file(3840, 2160, 18)))).toEqual(["12m", "8m", "4m", "2m"]);
    expect(ids(autoLadder(file(1920, 1080, 40)))).toEqual(["20m", "12m", "8m", "4m", "2m"]);
    expect(ids(autoLadder(file(1920, 1080, 5)))).toEqual(["4m", "2m"]);
    expect(ids(autoLadder(file(720, 480, 1.5)))).toEqual([]);
  });
});

// ---- the decisions

function memory(initial: Remembered | null = null): AutoMemory & { value: Remembered | null } {
  const m = {
    value: initial,
    get: () => m.value,
    set: (v: Remembered | null) => {
      m.value = v;
    },
  };
  return m;
}

/**
 * Automatic for a 4K file of `mbps`, on a clock the test moves (`t`, ms; the
 * wall clock is 1e12 + t); `blanks`: on a player that goes black between streams.
 */
function auto(mbps = 60, mem = memory(), f = file(3840, 2160, mbps), known?: Remembered | null, blanks = false) {
  /** The ceilings told to the server, in order. */
  const told: (number | null)[] = [];
  const clock = { t: 0 };
  const a = new AutoQuality({
    ladder: autoLadder(f),
    originalKbps: f.bitrate ? f.bitrate / 1000 : null,
    memory: mem,
    wall: () => 1e12 + clock.t,
    onCeiling: (k) => told.push(k),
    blanks,
  });
  a.start(clock.t, known);
  /**
   * Plays `seconds` at `delivery` seconds of video a second, starting with
   * `ahead` seconds loaded ahead; checks each second and returns the first step.
   */
  let pos = 100;
  let end = 130;
  const play = (seconds: number, delivery: number, ahead?: number) => {
    if (ahead !== undefined) end = pos + ahead;
    for (let i = 0; i < seconds; i++) {
      clock.t += 1000;
      // The playhead moves on as far as what has loaded.
      pos += Math.max(0, Math.min(1, end - pos));
      end += delivery;
      a.sample(clock.t, pos, end);
      const step = a.check(clock.t);
      if (step) return step;
    }
    return null;
  };
  /** A wait for data of `ms`. */
  const wait = (ms: number) => {
    a.waiting(clock.t);
    let step = null;
    for (let left = ms; left > 0; left -= 500) {
      clock.t += Math.min(500, left);
      step = step ?? a.check(clock.t);
    }
    a.resumed(clock.t);
    return step ?? a.check(clock.t);
  };
  /** Up to `seconds` pass with segments loading at `kbps`, one every 4 s, until a step. */
  const network = (seconds: number, kbps: number) => {
    for (let i = 0; i < seconds; i += 4) {
      clock.t += 4000;
      const ms = 1000;
      a.loaded(clock.t, (kbps * ms) / 8, ms);
      const step = a.check(clock.t);
      if (step) return step;
    }
    return null;
  };
  return { a, clock, mem, told, play, wait, network };
}

describe("Automatic", () => {
  it("starts at the original", () => {
    const { a } = auto();
    expect(a.playing).toBe("original");
    expect(a.lowered).toBeNull();
  });

  it("plays on while the video arrives fast enough", () => {
    const { a, play } = auto();
    expect(play(120, 1.2, 8)).toBeNull();
    expect(a.playing).toBe("original");
  });

  describe("on a wait for data", () => {
    it("of 2 s steps down, to the bitrate playing times the delivery seen times 0.75", () => {
      const { a, play, wait } = auto(20, memory(), file(1920, 1080, 20));
      // 20 Mbps arriving at half speed: 20 × 0.5 × 0.75 = 7.5 → 4 Mbps 720p.
      play(20, 0.5, 9);
      expect(wait(2_000)).toEqual({ to: "4m", why: "network" });
      expect(a.playing).toBe("4m");
      expect(a.lowered).toBe("network");
    });

    it("of 2 s counts while it lasts, not only once it ends", () => {
      const { a, clock } = auto();
      a.waiting(clock.t);
      clock.t += 1_500;
      expect(a.check(clock.t)).toBeNull();
      clock.t += 500;
      expect(a.check(clock.t)?.why).toBe("network");
    });

    it("shorter: two within a minute step down, one doesn't, nor two a minute apart", () => {
      const one = auto();
      expect(one.wait(800)).toBeNull();
      expect(one.play(30, 1.2)).toBeNull();
      expect(one.wait(800)?.why).toBe("network");

      const apart = auto();
      expect(apart.wait(800)).toBeNull();
      apart.clock.t += 61_000;
      expect(apart.wait(800)).toBeNull();
      expect(apart.a.playing).toBe("original");
    });

    it("isn't one when a seek, a pause or a new session ends it", () => {
      const { a, clock } = auto();
      a.waiting(clock.t);
      clock.t += 1_000;
      a.interrupted();
      clock.t += 5_000;
      a.resumed(clock.t);
      expect(a.check(clock.t)).toBeNull();
      // Short waits of the session before don't pair with the next session's.
      a.waiting(clock.t);
      clock.t += 500;
      a.resumed(clock.t);
      a.began();
      a.waiting(clock.t);
      clock.t += 500;
      a.resumed(clock.t);
      expect(a.check(clock.t)).toBeNull();
    });
  });

  describe("on slow delivery", () => {
    it("steps down before it waits: under 1.1 for 15 s, under 10 s ahead and falling", () => {
      const { a, play } = auto(20, memory(), file(1920, 1080, 20));
      const step = play(40, 0.8, 9);
      // 20 × 0.8 × 0.75 = 12 → 12 Mbps 1080p.
      expect(step).toEqual({ to: "12m", why: "network" });
      expect(a.playing).toBe("12m");
    });

    it("doesn't with 10 s or more ahead, or ahead not falling", () => {
      expect(auto().play(60, 0.9, 25)).toBeNull();
      expect(auto().play(60, 1.05, 5)).toBeNull();
    });

    it("can't tell when the player doesn't know what it has loaded", () => {
      const { a, clock } = auto();
      for (let i = 0; i < 60; i++) {
        clock.t += 1000;
        a.sample(clock.t, 100 + i, null);
        expect(a.check(clock.t)).toBeNull();
      }
    });
  });

  describe("stepping down", () => {
    it("never goes above the server's own quality for the file", () => {
      const { play, wait } = auto(60);
      // 60 × 0.9 × 0.75 = 40.5, above the server's 30 Mbps 4K.
      play(20, 0.9, 9);
      expect(wait(2_000)?.to).toBe("30m");
    });

    it("never below 2 Mbps 480p", () => {
      const { a, play, wait, clock } = auto(20, memory(), file(1920, 1080, 20));
      play(20, 0.05, 2);
      expect(wait(2_000)?.to).toBe("2m");
      clock.t += 30_000;
      expect(wait(3_000)).toBeNull();
      expect(a.playing).toBe("2m");
    });

    it("goes one quality down at least, again on more issues", () => {
      const { a, play, wait, clock } = auto(60);
      play(20, 1.0, 9);
      expect(wait(2_000)?.to).toBe("30m");
      clock.t += 25_000;
      // At 30 Mbps, arriving as fast as it plays: 30 × 1 × 0.75 = 22.5, still one down.
      play(20, 1.0, 9);
      expect(wait(2_000)?.to).toBe("15m");
      expect(a.playing).toBe("15m");
    });

    it("waits 20 s after a change before the next", () => {
      const { a, play, wait, clock } = auto(60);
      play(20, 0.9, 9);
      expect(wait(2_000)?.to).toBe("30m");
      clock.t += 5_000;
      expect(wait(2_000)).toBeNull();
      expect(a.playing).toBe("30m");
      clock.t += 15_000;
      expect(wait(2_000)?.to).toBe("15m");
    });

    it("on frames the decoder drops: one quality down, said as the device's, not remembered", () => {
      const { a, mem, play, wait, clock } = auto(60);
      play(20, 0.9, 9);
      wait(2_000);
      const kept = mem.value;
      clock.t += 21_000;
      a.decoding();
      expect(a.check(clock.t)).toEqual({ to: "15m", why: "device" });
      expect(a.lowered).toBe("device");
      expect(mem.value).toBe(kept);
    });

    it("waits after the player's own fallback as after a change of its own", () => {
      const { a, clock } = auto();
      a.fellBack(clock.t);
      clock.t += 10_000;
      a.decoding();
      expect(a.check(clock.t)).toBeNull();
    });

    it("steps from what the original plays at once the player converts it", () => {
      const { a, clock } = auto(60);
      a.setOriginalKbps(15_000);
      a.decoding();
      clock.t += 1_000;
      // 15 × 0.75 = 11.25 → 8 Mbps 1080p (12 is above it).
      expect(a.check(clock.t)?.to).toBe("8m");
    });
  });

  describe("stepping back up", () => {
    function lowered() {
      const t = auto(20, memory(), file(1920, 1080, 20));
      t.play(20, 0.5, 9);
      expect(t.wait(2_000)?.to).toBe("4m");
      return t;
    }

    it("waits 5 minutes after a step down, then goes one step up when the network carries 1.6 times it", () => {
      const { a, network } = lowered();
      // 3 calm minutes aren't enough so soon after the step down.
      expect(network(180, 50_000)).toBeNull();
      const step = network(124, 50_000);
      expect(step).toEqual({ to: "8m", why: null });
      expect(a.playing).toBe("8m");
    });

    it("stays when the network doesn't carry 1.6 times the next quality, or can't say", () => {
      const { a, network, clock } = lowered();
      expect(network(400, 8_000 * 1.5)).toBeNull();
      clock.t += 400_000;
      expect(a.check(clock.t)).toBeNull();
      expect(a.playing).toBe("4m");
    });

    it("needs 3 minutes without an issue", () => {
      const { a, play, wait, network } = auto(20, memory(), file(1920, 1080, 20));
      play(20, 0.05, 2);
      expect(wait(2_000)?.to).toBe("2m");
      expect(network(296, 50_000)).toBeNull();
      // An issue at the lowest quality changes nothing, but 3 calm minutes count from it.
      wait(800);
      expect(wait(800)).toBeNull();
      expect(a.playing).toBe("2m");
      expect(network(176, 50_000)).toBeNull();
      expect(network(8, 50_000)?.to).toBe("4m");
    });

    it("one step every 3 minutes, back to the original, which forgets the network", () => {
      const { a, mem, network } = lowered();
      expect(mem.value).not.toBeNull();
      expect(network(304, 50_000)?.to).toBe("8m");
      expect(network(176, 50_000)).toBeNull();
      expect(network(8, 50_000)?.to).toBe("12m");
      expect(network(184, 50_000)?.to).toBe("original");
      expect(a.lowered).toBeNull();
      expect(mem.value).toBeNull();
    });

    it("settles: a step up undone within 10 minutes isn't tried again during the title", () => {
      const { a, mem, play, wait, network } = lowered();
      expect(network(304, 50_000)?.to).toBe("8m");
      // Two minutes on, the network can't carry 8 Mbps after all (8 × 0.8 × 0.75 = 4.8).
      network(120, 50_000);
      play(20, 0.8, 9);
      expect(wait(2_000)?.to).toBe("4m");
      const kept = mem.value;
      // However fast the network says it is, it stays where it held, for the rest of the title.
      expect(network(3_600, 80_000)).toBeNull();
      expect(a.playing).toBe("4m");
      expect(mem.value).toBe(kept);
    });

    it("a step down more than 10 minutes after a step up leaves that quality open", () => {
      const { a, play, wait, network } = lowered();
      expect(network(304, 50_000)?.to).toBe("8m");
      network(604, 9_000);
      play(20, 0.8, 9);
      expect(wait(2_000)?.to).toBe("4m");
      expect(network(304, 50_000)?.to).toBe("8m");
      expect(a.playing).toBe("8m");
    });

    it("each step up undone doubles the calm the next one waits for", () => {
      const { a, play, wait, network } = lowered();
      expect(network(304, 50_000)?.to).toBe("8m");
      expect(network(184, 50_000)?.to).toBe("12m");
      // 12 Mbps doesn't hold (12 × 0.9 × 0.75 = 8.1): back to 8, and 12 is off for the title.
      play(20, 0.9, 9);
      expect(wait(2_000)?.to).toBe("8m");
      // Later 8 Mbps falters too, more than 10 minutes after it was reached: 4, with 8 still open.
      network(604, 9_000);
      play(20, 0.8, 9);
      expect(wait(2_000)?.to).toBe("4m");
      // 5 minutes after the step down 3 calm minutes would have done; now it takes 6.
      expect(network(352, 50_000)).toBeNull();
      expect(network(16, 50_000)?.to).toBe("8m");
      expect(a.playing).toBe("8m");
    });

    it("on LG, whose player says nothing of its network, goes up on the fastest refill the buffer showed", () => {
      const t = auto(20, memory(), file(1920, 1080, 20));
      t.play(20, 0.5, 9);
      expect(t.wait(2_000)?.to).toBe("4m");
      // Back at 4 Mbps, the buffer refills at 4 times the video's pace: the network carries 16 Mbps at least.
      t.play(8, 4);
      // Full, it fills only as fast as it plays: every figure since says hardly more than 4 Mbps.
      expect(t.play(300, 1.02)).toEqual({ to: "8m", why: null });
    });

    it("on LG stays down when no refill carried 1.6 times the next quality", () => {
      const t = auto(20, memory(), file(1920, 1080, 20));
      t.play(20, 0.5, 9);
      expect(t.wait(2_000)?.to).toBe("4m");
      t.play(8, 1.3);
      expect(t.play(330, 1.02)).toBeNull();
      expect(t.a.playing).toBe("4m");
    });

    it("measures the network from engine figures too, by their median", () => {
      const { a, clock } = lowered();
      clock.t += 301_000;
      // 1.6 × 8 Mbps is 12.8: the median, 20, carries it (the mean of the first three wouldn't).
      for (const kbps of [100, 7_000, 20_000, 25_000, 30_000]) a.bandwidth(clock.t, kbps);
      expect(a.check(clock.t)).toEqual({ to: "8m", why: null });
    });
  });

  describe("its memory", () => {
    it("keeps the ceiling of a step down for the device and server", () => {
      const { mem, play, wait } = auto(20, memory(), file(1920, 1080, 20));
      play(20, 0.5, 9);
      wait(2_000);
      expect(mem.value).toEqual({ kbps: 7_500, at: 1e12 + 22_000 });
    });

    it("starts the next title at most at it, when below the file, and not up again before 5 minutes from then", () => {
      const mem = memory({ kbps: 7_500, at: 1e12 - 60_000 });
      const { a, network } = auto(20, mem, file(1920, 1080, 20));
      expect(a.playing).toBe("4m");
      expect(a.lowered).toBe("network");
      expect(network(200, 50_000)).toBeNull();
      expect(network(44, 50_000)?.to).toBe("8m");
    });

    it("starts at the original when the file needs less than it", () => {
      const { a } = auto(5, memory({ kbps: 7_500, at: 1e12 - 60_000 }), file(1920, 1080, 5));
      expect(a.playing).toBe("original");
    });

    it("forgets it after 30 minutes", () => {
      const mem = memory({ kbps: 2_000, at: 1e12 - MEMORY_MS });
      const { a } = auto(20, mem, file(1920, 1080, 20));
      expect(a.playing).toBe("original");
      expect(mem.value).toBeNull();
    });

    it("lives in the TV's storage", () => {
      const m = new Map<string, string>();
      const store = { get: (k: string) => m.get(k) ?? null, set: (k: string, v: string) => void m.set(k, v), remove: (k: string) => void m.delete(k) };
      const kept = storedMemory(store, "pley.autoQuality.http://nas:7539");
      expect(kept.get()).toBeNull();
      kept.set({ kbps: 4_000, at: 5 });
      expect(storedMemory(store, "pley.autoQuality.http://nas:7539").get()).toEqual({ kbps: 4_000, at: 5 });
      expect(storedMemory(store, "pley.autoQuality.http://other:7539").get()).toBeNull();
      kept.set(null);
      expect(m.size).toBe(0);
      m.set("pley.autoQuality.x", "{broken");
      expect(storedMemory(store, "pley.autoQuality.x").get()).toBeNull();
    });
  });
  describe("what the server knows of the network", () => {
    it("takes the place of the device's memory: its ceiling, or none, whatever this device remembers", () => {
      const kept = memory({ kbps: 2_000, at: 1e12 - 60_000 });
      const server = auto(20, kept, file(1920, 1080, 20), { kbps: 9_000, at: 1e12 - 3_600_000 });
      expect(server.a.playing).toBe("8m");
      expect(server.a.lowered).toBe("network");
      const none = auto(20, memory({ kbps: 2_000, at: 1e12 - 60_000 }), file(1920, 1080, 20), null);
      expect(none.a.playing).toBe("original");
      expect(none.a.lowered).toBeNull();
      // Nothing is told back for what was only heard.
      expect([server.told, none.told]).toEqual([[], []]);
    });

    it("keeps its own time: a ceiling from yesterday still holds, and the step back up isn't held 5 minutes", () => {
      const { a, network } = auto(20, memory(), file(1920, 1080, 20), { kbps: 4_500, at: 1e12 - 86_000_000 });
      expect(a.playing).toBe("4m");
      expect(network(184, 50_000)?.to).toBe("8m");
    });

    it("plays the original when the file needs less, and never below 2 Mbps 480p", () => {
      expect(auto(5, memory(), file(1920, 1080, 5), { kbps: 7_000, at: 1e12 }).a.playing).toBe("original");
      expect(auto(20, memory(), file(1920, 1080, 20), { kbps: 300, at: 1e12 }).a.playing).toBe("2m");
    });

    it("hears each change of the ceiling: a step down, a step up to a quality, back to the original", () => {
      const { a, mem, told, play, wait, network } = auto(20, memory(), file(1920, 1080, 20));
      expect(a.started).toBe(true);
      play(20, 0.5, 9);
      expect(wait(2_000)?.to).toBe("4m");
      expect(network(304, 50_000)?.to).toBe("8m");
      // The step up remembers the quality it plays now.
      expect(mem.value?.kbps).toBe(8_000);
      expect(network(184, 50_000)?.to).toBe("12m");
      expect(network(184, 50_000)?.to).toBe("original");
      expect(told).toEqual([7_500, 8_000, 12_000, null]);
    });

    it("isn't told of a step down for the device, nor of a memory that ran out", () => {
      const { a, told } = auto(20, memory({ kbps: 2_000, at: 1e12 - MEMORY_MS }), file(1920, 1080, 20));
      a.decoding();
      expect(a.check(30_000)?.why).toBe("device");
      expect(told).toEqual([]);
    });
  });

  describe("on a player that goes black between streams (Samsung, LG)", () => {
    /** Automatic for a 1080p file of 20 Mbps (or `f`), started where `known` says. */
    const blank = (known?: Remembered | null, f = file(1920, 1080, 20)) => auto(f.bitrate! / 1e6, memory(), f, known, true);

    describe("playing the original", () => {
      it("moves to a conversion on a wait of 2 s or more on a network too slow for the file: one new stream, as anywhere", () => {
        const { a, clock, play, wait } = blank();
        // Samsung's player measured 16 Mbps: under 1.5 × the file's 20.
        a.bandwidth(clock.t, 16_000);
        // 20 × 0.5 × 0.75 = 7.5 → 4 Mbps 720p.
        play(20, 0.5, 9);
        expect(wait(2_000)).toEqual({ to: "4m", why: "network" });
        expect(a.playing).toBe("4m");
      });

      it("plays on through waits on a network that carries 1.5 times the file: the server is why it waits", () => {
        const { a, clock, play, wait } = blank();
        a.bandwidth(clock.t, 30_000);
        play(20, 0.5, 9);
        expect(wait(2_000)).toBeNull();
        expect(play(20, 0.5)).toBeNull();
        expect(wait(6_000)).toBeNull();
        expect(a.playing).toBe("original");
      });

      it("where nothing measured the network, moves at the second wait of 2 s within 5 minutes", () => {
        const { play, wait } = blank();
        play(20, 0.5, 9);
        expect(wait(3_000)).toBeNull();
        expect(play(60, 0.5, 9)).toBeNull();
        expect(wait(2_000)).toEqual({ to: "4m", why: "network" });
        const apart = blank();
        expect(apart.wait(2_000)).toBeNull();
        apart.clock.t += 5 * 60_000;
        expect(apart.wait(2_000)).toBeNull();
        expect(apart.a.playing).toBe("original");
      });

      it("counts a wait as one nothing measured once the network's figures run out as it goes on", () => {
        const { a, clock, wait } = blank();
        a.bandwidth(clock.t, 30_000);
        // 2.5 s into this wait the figure is 2 minutes old: from then on nothing measured the network.
        clock.t = 118_000;
        expect(wait(5_000)).toBeNull();
        clock.t += 10_000;
        expect(wait(2_000)?.why).toBe("network");
      });

      it("moves the original the server converts on any wait of 2 s: a lower quality is less for it to make", () => {
        const { a, clock, play, wait } = blank();
        a.setOriginalKbps(15_000);
        a.bandwidth(clock.t, 80_000);
        // 15 × 0.5 × 0.75 = 5.6 → 4 Mbps 720p.
        play(20, 0.5, 9);
        expect(wait(2_000)).toEqual({ to: "4m", why: "network" });
      });

      it("doesn't move for two short waits or slow delivery: the picture hasn't stopped", () => {
        const { a, play, wait } = blank();
        expect(a.patient).toBe(true);
        expect(wait(800)).toBeNull();
        expect(play(30, 1.2)).toBeNull();
        expect(wait(800)).toBeNull();
        expect(play(60, 0.8, 9)).toBeNull();
        expect(a.playing).toBe("original");
      });

      it("never goes back up to another stream, however fast the network", () => {
        const { a, clock, play, wait, network } = blank();
        a.bandwidth(clock.t, 16_000);
        play(20, 0.5, 9);
        expect(wait(2_000)?.to).toBe("4m");
        // A conversion whose bitrate can't change (an older server): it stays.
        a.began(null);
        expect(network(3_600, 80_000)).toBeNull();
        expect(a.playing).toBe("4m");
      });

      it("plays on through dropped frames past the first minute, and starts the next title a step lower, once", () => {
        const { a, told, mem } = blank(undefined, file(3840, 2160, 40));
        a.struggled();
        // 40 × 0.75 = 30 → 30 Mbps 4K next time.
        expect(told).toEqual([30_000]);
        expect(mem.value?.kbps).toBe(30_000);
        a.struggled();
        expect(told).toEqual([30_000]);
        expect(a.playing).toBe("original");
      });
    });

    describe("a conversion whose bitrate changes as it is sent", () => {
      /** Started at 8 Mbps 1080p by the network's memory, its stream's bitrate changing as it is sent, at 3 Mbps at least. */
      function live() {
        const t = blank({ kbps: 9_000, at: 1e12 });
        expect(t.a.playing).toBe("8m");
        t.a.began({ top: 8_000, floor: 3_000 });
        return t;
      }

      it("steps the bitrate down, not the stream: the same quality and size, in whole Mbps", () => {
        const { a, told, play, wait } = live();
        // 8 × 0.7 × 0.75 = 4.2 → 4 Mbps, still 1080p.
        play(20, 0.7, 9);
        expect(wait(2_000)).toEqual({ to: "8m", why: "network", bitrate: 4_000 });
        expect([a.playing, a.liveKbps, a.lowered, a.liveLowered]).toEqual(["8m", 4_000, "network", true]);
        expect(a.playingKbps()).toBe(4_000);
        // The network's memory keeps what it carries, for the next title.
        expect(told).toEqual([4_200]);
      });

      it("keeps 3 Mbps for 1080p and 8 Mbps for 4K at least; below that the network waits", () => {
        const { a, play, wait, clock } = live();
        // 8 × 0.3 × 0.75 = 1.8, but 1080p keeps 3.
        play(20, 0.3, 9);
        expect(wait(2_000)?.bitrate).toBe(3_000);
        clock.t += 25_000;
        play(20, 0.3, 9);
        expect(wait(3_000)).toBeNull();
        expect([a.playing, a.liveKbps]).toEqual(["8m", 3_000]);

        const uhd = auto(60, memory(), file(3840, 2160, 60), { kbps: 16_000, at: 1e12 }, true);
        expect(uhd.a.playing).toBe("15m");
        uhd.a.began({ top: 15_000, floor: 8_000 });
        uhd.play(20, 0.3, 9);
        expect(uhd.wait(2_000)?.bitrate).toBe(8_000);
      });

      it("steps on short waits and slow delivery too, which a new bitrate doesn't black out", () => {
        const one = live();
        expect(one.a.patient).toBe(false);
        expect(one.wait(800)).toBeNull();
        expect(one.play(30, 1.2)).toBeNull();
        // Arriving as fast as it plays: 8 × 1 × 0.75 = 6.
        expect(one.wait(800)?.bitrate).toBe(6_000);

        const slow = live();
        // 8 × 0.8 × 0.75 = 4.8 → 4.
        expect(slow.play(40, 0.8, 9)).toEqual({ to: "8m", why: "network", bitrate: 4_000 });
      });

      it("steps for a decoder falling behind too, without telling the network's memory", () => {
        const { a, told, clock } = live();
        clock.t += 1_000;
        a.decoding();
        expect(a.check(clock.t)).toEqual({ to: "8m", why: "device", bitrate: 6_000 });
        expect(told).toEqual([]);
      });

      it("steps back up after calm minutes, as a new stream would: the next of the file's bitrates, then its own", () => {
        const { a, told, play, wait, network } = live();
        play(20, 0.3, 9);
        expect(wait(2_000)?.bitrate).toBe(3_000);
        // 5 minutes after the step down, the network carries 1.6 × 4 Mbps (4 Mbps 720p's bitrate, at 1080p here).
        expect(network(304, 50_000)).toEqual({ to: "8m", why: null, bitrate: 4_000 });
        expect(network(176, 50_000)).toBeNull();
        expect(network(8, 50_000)).toEqual({ to: "8m", why: null, bitrate: 8_000 });
        expect(a.liveLowered).toBe(false);
        // Its own bitrate is as high as it goes: never another stream.
        expect(network(3_600, 80_000)).toBeNull();
        // Each step told as the network's ceiling: the next title starts there.
        expect(told).toEqual([1_800, 4_000, 8_000]);
      });

      it("settles: a step up undone within 10 minutes isn't tried again, and the next calm is twice as long", () => {
        const { a, play, wait, network } = live();
        play(20, 0.7, 9);
        expect(wait(2_000)?.bitrate).toBe(4_000);
        expect(network(304, 50_000)?.bitrate).toBe(8_000);
        // Two minutes on, 8 Mbps doesn't hold after all: 8 × 0.8 × 0.75 = 4.8 → 4.
        network(120, 50_000);
        play(20, 0.8, 9);
        expect(wait(2_000)?.bitrate).toBe(4_000);
        expect(network(3_600, 80_000)).toBeNull();
        expect(a.liveKbps).toBe(4_000);
        // Later 4 Mbps falters too: 3, with 4 still open, after 6 calm minutes rather than 3.
        play(20, 0.5, 9);
        expect(wait(2_000)?.bitrate).toBe(3_000);
        expect(network(352, 50_000)).toBeNull();
        expect(network(16, 50_000)?.bitrate).toBe(4_000);
        expect(network(3_600, 80_000)).toBeNull();
        expect(a.liveKbps).toBe(4_000);
      });

      it("steps the original the server converts as well; back at its own bitrate it is the original again", () => {
        const { a, told, play, wait, network } = blank(undefined, file(3840, 2160, 60));
        a.setOriginalKbps(30_000);
        a.began({ top: 30_000, floor: 8_000 });
        // 30 × 0.5 × 0.75 = 11.25 → 11 Mbps 4K.
        play(20, 0.5, 9);
        expect(wait(2_000)).toEqual({ to: "original", why: "network", bitrate: 11_000 });
        expect([a.lowered, a.liveLowered]).toEqual(["network", true]);
        // Up through the file's qualities' bitrates: 12, 15, then its own 30.
        expect(network(304, 80_000)?.bitrate).toBe(12_000);
        expect(network(184, 80_000)?.bitrate).toBe(15_000);
        expect(network(184, 80_000)?.bitrate).toBe(30_000);
        expect([a.playing, a.lowered, a.liveLowered]).toEqual(["original", null, false]);
        expect(told).toEqual([11_250, 12_000, 15_000, null]);
      });

      it("goes on at the bitrate it came to in a new stream of the same quality (a recovery, a track), afresh in another", () => {
        const { a, play, wait } = live();
        play(20, 0.7, 9);
        expect(wait(2_000)?.bitrate).toBe(4_000);
        a.began({ top: 8_000, floor: 3_000 });
        expect([a.liveKbps, a.liveLowered]).toEqual([4_000, true]);
        a.began({ top: 12_000, floor: 3_000 });
        expect([a.liveKbps, a.liveLowered]).toEqual([12_000, false]);
        a.began(null);
        expect([a.liveKbps, a.patient]).toEqual([null, true]);
      });

      it("goes by the bitrate the server says: one lower than asked is as high as the stream goes", () => {
        const { a, play, wait, network } = live();
        play(20, 0.7, 9);
        expect(wait(2_000)?.bitrate).toBe(4_000);
        a.liveAt(4_000, 4_000);
        expect(network(304, 50_000)?.bitrate).toBe(8_000);
        a.liveAt(6_000, 8_000);
        expect([a.liveKbps, a.liveTop, a.liveLowered]).toEqual([6_000, 6_000, false]);
        expect(network(3_600, 80_000)).toBeNull();
      });

      it("goes on as it was when the server doesn't change the bitrate, and stops asking when it can't", () => {
        const { a, play, wait, clock } = live();
        play(20, 0.7, 9);
        expect(wait(2_000)?.bitrate).toBe(4_000);
        a.liveRefused(false);
        expect([a.liveKbps, a.lowered, a.liveLowered]).toEqual([8_000, "network", false]);
        clock.t += 25_000;
        play(20, 0.7, 9);
        expect(wait(2_000)?.bitrate).toBe(4_000);
        // An older transcoder: a stream whose bitrate can't change, which short waits don't move.
        a.liveRefused(true);
        expect([a.liveKbps, a.patient]).toEqual([null, true]);
        clock.t += 25_000;
        expect(wait(800)).toBeNull();
        expect(wait(800)).toBeNull();
        expect(a.playing).toBe("8m");
      });
    });
  });
});

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Api } from "../../src/api/client";
import type { Cancel } from "../../src/net/transport";
import { PARTIAL_RETRY_MS, SubtitlePresenter } from "../../src/subs/presenter";

/** A Matroska file of a PGS track with no picture yet, as the server answers while it reads the track. */
function pgsTrack(): ArrayBuffer {
  const el = (id: number, body: number[]) => {
    const idBytes: number[] = [];
    for (let v = id; v > 0; v = Math.floor(v / 256)) idBytes.unshift(v & 0xff);
    const n = body.length;
    return [...idBytes, 0x01, 0, 0, 0, (n >>> 24) & 0xff, (n >> 16) & 0xff, (n >> 8) & 0xff, n & 0xff, ...body];
  };
  const codec = Array.from("S_HDMV/PGS", (c) => c.charCodeAt(0));
  const entry = el(0xae, [...el(0xd7, [1]), ...el(0x83, [0x11]), ...el(0x86, codec)]);
  const segment = el(0x18538067, el(0x1654ae6b, entry));
  return new Uint8Array([...el(0x1a45dfa3, el(0x4282, Array.from("matroska", (c) => c.charCodeAt(0)))), ...segment]).buffer;
}

interface Asked {
  url: string;
  cancel: Cancel;
  answer(complete: boolean): void;
}

function presenter() {
  const asked: Asked[] = [];
  const api = {
    send: (url: string, opts: { cancel: Cancel }) =>
      new Promise((resolve) => {
        asked.push({
          url,
          cancel: opts.cancel,
          answer: (complete) => resolve({ status: 200, body: pgsTrack(), header: (name: string) => (name === "X-Pley-Complete" ? (complete ? "1" : "0") : null) }),
        });
      }),
  } as unknown as Api;
  const player = { positionMs: 0 };
  const hooks = { position: () => player.positionMs, offsetMs: () => 0, onShown: vi.fn(), onFailed: vi.fn(), onPicturesUnavailable: vi.fn() };
  return { p: new SubtitlePresenter(api, hooks), asked, player, hooks };
}

describe("picture tracks", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it("are asked for from where the player is each time, the start too, until the server has all of it", async () => {
    const { p, asked, player, hooks } = presenter();
    p.showPictures("/s.mks");
    asked[0]!.answer(false);
    player.positionMs = 3_000;
    await vi.advanceTimersByTimeAsync(PARTIAL_RETRY_MS);
    asked[1]!.answer(false);
    player.positionMs = 6_000;
    await vi.advanceTimersByTimeAsync(PARTIAL_RETRY_MS);
    asked[2]!.answer(true);
    await vi.advanceTimersByTimeAsync(PARTIAL_RETRY_MS * 3);
    expect(asked.map((a) => a.url)).toEqual(["/s.mks?at=0", "/s.mks?at=3000", "/s.mks?at=6000"]);
    expect(hooks.onFailed).not.toHaveBeenCalled();
    expect(hooks.onPicturesUnavailable).not.toHaveBeenCalled();
    p.dispose();
  });

  it("are asked for again at once from where a seek went, the answer from before it let go of", async () => {
    const { p, asked } = presenter();
    p.showPictures("/s.mks");
    // Before the player says where it went.
    p.sought(200_000);
    expect(asked.map((a) => a.url)).toEqual(["/s.mks?at=0", "/s.mks?at=200000"]);
    expect(asked[0]!.cancel.cancelled).toBe(true);
    asked[1]!.answer(false);
    await vi.advanceTimersByTimeAsync(0);
    // Waiting to ask again: at once instead.
    p.sought(100_000);
    expect(asked.map((a) => a.url).slice(2)).toEqual(["/s.mks?at=100000"]);
    asked[2]!.answer(true);
    await vi.advanceTimersByTimeAsync(0);
    // All of it: nothing more to ask.
    p.sought(50_000);
    await vi.advanceTimersByTimeAsync(PARTIAL_RETRY_MS * 3);
    expect(asked).toHaveLength(3);
    p.dispose();
  });
});

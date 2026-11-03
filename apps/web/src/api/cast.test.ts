import { afterEach, describe, expect, it, vi } from "vitest";
import { makeItem } from "@/test/fixtures";
import { testQueryClient } from "@/test/render";
import { castKeys, castPosition, patchCastState, type CastTarget, type HeardCastState } from "./cast";
import type { CastState } from "./types";

const item = makeItem({ id: 7, title: "A Movie", duration_ms: 6_000_000 });

function heard(overrides: Partial<HeardCastState> = {}): HeardCastState {
  return { item, position_ms: 60_000, duration_ms: 6_000_000, state: "playing", age_ms: 0, heard_at: 1_000_000, ...overrides };
}

afterEach(() => {
  vi.useRealTimers();
});

describe("castPosition", () => {
  it("moves on while the device plays: its age when said, and the time since it was heard", () => {
    expect(castPosition(heard({ age_ms: 2_000 }), 1_000_000)).toBe(62_000);
    expect(castPosition(heard({ age_ms: 2_000 }), 1_005_000)).toBe(67_000);
  });

  it("stands still while paused or loading", () => {
    expect(castPosition(heard({ state: "paused", age_ms: 2_000 }), 1_005_000)).toBe(60_000);
    expect(castPosition(heard({ state: "buffering" }), 1_005_000)).toBe(60_000);
  });

  it("never goes past the end, nor back before the state was heard", () => {
    expect(castPosition(heard({ position_ms: 5_999_000 }), 1_010_000)).toBe(6_000_000);
    expect(castPosition(heard({ duration_ms: null, position_ms: 5_999_000 }), 1_010_000)).toBe(6_009_000);
    expect(castPosition(heard(), 900_000)).toBe(60_000);
  });
});

describe("patchCastState", () => {
  const device = (id: string, state: HeardCastState | null = null): CastTarget => ({ id, name: `Device ${id}`, platform: "android_tv", state });
  const said: CastState = { item, position_ms: 90_000, duration_ms: 6_000_000, state: "paused", age_ms: 0 };

  it("says what a listed device plays, and when it was heard", () => {
    vi.useFakeTimers({ now: 2_000_000 });
    const qc = testQueryClient();
    qc.setQueryData<CastTarget[]>(castKeys.devices, [device("tv-1"), device("tv-2", heard())]);
    patchCastState(qc, "tv-1", said);
    patchCastState(qc, "tv-2", null);
    expect(qc.getQueryData<CastTarget[]>(castKeys.devices)).toEqual([
      device("tv-1", { ...said, heard_at: 2_000_000 }),
      device("tv-2"),
    ]);
    expect(qc.getQueryState(castKeys.devices)?.isInvalidated).toBe(false);
  });

  it("asks for the list again for a device it doesn't know", () => {
    const qc = testQueryClient();
    qc.setQueryData<CastTarget[]>(castKeys.devices, [device("tv-1")]);
    patchCastState(qc, "tv-9", said);
    expect(qc.getQueryState(castKeys.devices)?.isInvalidated).toBe(true);
  });
});

import { describe, expect, it } from "vitest";
import type { PlayingSession } from "@/api/types";
import { makeItem } from "@/test/fixtures";
import { commandAnswered, estimatedPosition, received, type RemoteSession, type SentCommand } from "./remote";

// The server's clock, in seconds, and this browser's, in milliseconds: they needn't agree.
const at = 1_700_000_000;
const seen = 1_800_000_000_000;
const report: PlayingSession = {
  session_id: "s1",
  kind: "item",
  device: "Living room TV",
  item: makeItem({ id: 7 }),
  channel: null,
  state: "playing",
  position_ms: 60_000,
  duration_ms: 120_000,
  updated_at: at,
  age_ms: 0,
  controls: ["play", "pause", "seek", "skip", "next"],
};
const tv: RemoteSession = { ...report, reported_at_ms: seen };

/** The player's next report, `secs` after the last by both clocks. */
function later(secs: number, overrides: Partial<RemoteSession> = {}): RemoteSession {
  return { ...tv, updated_at: at + secs, reported_at_ms: seen + secs * 1000, ...overrides };
}

describe("received", () => {
  it("dates each report by this browser's clock, from how long ago the server says it was", () => {
    const [s] = received([{ ...report, age_ms: 4_000 }], seen);
    expect(s).toEqual({ ...report, age_ms: 4_000, reported_at_ms: seen - 4_000 });
  });
});

describe("estimatedPosition", () => {
  it("moves on from the last report while playing, up to the end", () => {
    expect(estimatedPosition(tv, seen + 15_000)).toBe(75_000);
    expect(estimatedPosition(tv, seen + 600_000)).toBe(120_000);
    // Never back before the report.
    expect(estimatedPosition(tv, seen - 5_000)).toBe(60_000);
    expect(estimatedPosition({ ...tv, duration_ms: null }, seen + 600_000)).toBe(660_000);
  });

  it("stays where it was when paused or buffering", () => {
    expect(estimatedPosition({ ...tv, state: "paused" }, seen + 15_000)).toBe(60_000);
    expect(estimatedPosition({ ...tv, state: "buffering" }, seen + 15_000)).toBe(60_000);
  });

  it("is the same whatever time the server's clock gave the report", () => {
    // A server clock minutes behind this browser's, the same, or minutes ahead.
    for (const updatedAt of [at - 300, at, at + 300]) {
      const [s] = received([{ ...report, updated_at: updatedAt, age_ms: 5_000 }], seen);
      expect(estimatedPosition(s!, seen + 10_000)).toBe(75_000);
    }
  });
});

describe("commandAnswered", () => {
  const sentAt = seen + 2_000;
  const sent = (command: SentCommand["command"], target?: number): SentCommand => ({ command, before: tv, sentAt, target });
  const by = sentAt + 1_000;

  it("sees a pause, another title, and a session gone", () => {
    expect(commandAnswered(sent("pause"), tv, by)).toBe(false);
    expect(commandAnswered(sent("pause"), later(2, { state: "paused" }), by)).toBe(true);
    expect(commandAnswered(sent("next"), { ...tv, item: makeItem({ id: 8 }) }, by)).toBe(true);
    expect(commandAnswered(sent("next"), undefined, by)).toBe(true);
  });

  it("sees a seek or skip in a report from somewhere new", () => {
    // Only the usual report since, where it would be by then.
    expect(commandAnswered(sent("skip", 92_000), later(4, { position_ms: 64_000 }), by)).toBe(false);
    expect(commandAnswered(sent("skip", 92_000), later(3, { position_ms: 93_000 }), by)).toBe(true);
    expect(commandAnswered(sent("seek", 10_000), later(3, { position_ms: 10_500 }), by)).toBe(true);
    // No report since says nothing of it.
    expect(commandAnswered(sent("seek", 10_000), tv, by)).toBe(false);
  });

  it("compares server times with server times and this browser's with its own, however far apart the clocks are", () => {
    // A browser a minute ahead of the server, or behind it.
    for (const skew of [60_000, -60_000]) {
      const before: RemoteSession = { ...tv, reported_at_ms: seen + skew };
      const off: SentCommand = { ...sent("skip", 92_000), before, sentAt: sentAt + skew };
      const next = (secs: number, position_ms: number) =>
        later(secs, { position_ms, reported_at_ms: before.reported_at_ms + secs * 1000 });
      expect(commandAnswered(off, next(3, 93_000), by + skew)).toBe(true);
      expect(commandAnswered(off, next(4, 64_000), by + skew)).toBe(false);
      expect(commandAnswered(off, before, by + skew)).toBe(false);
    }
  });

  it("sees a previous that starts the same title over", () => {
    expect(commandAnswered(sent("previous", 0), later(2, { position_ms: 0 }), by)).toBe(true);
    // A few seconds in, too near the start to have moved far.
    const early = { ...tv, position_ms: 4_000 };
    expect(commandAnswered({ ...sent("previous", 0), before: early }, later(2, { position_ms: 0 }), by)).toBe(true);
    expect(commandAnswered(sent("previous", 0), tv, by)).toBe(false);
  });
});

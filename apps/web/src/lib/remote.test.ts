import { describe, expect, it } from "vitest";
import type { PlayingSession } from "@/api/types";
import { makeItem } from "@/test/fixtures";
import { commandAnswered, estimatedPosition, type SentCommand } from "./remote";

const at = 1_700_000_000;
const tv: PlayingSession = {
  session_id: "s1",
  kind: "item",
  device: "Living room TV",
  item: makeItem({ id: 7 }),
  channel: null,
  state: "playing",
  position_ms: 60_000,
  duration_ms: 120_000,
  updated_at: at,
  controls: ["play", "pause", "seek", "skip", "next"],
};

describe("estimatedPosition", () => {
  it("moves on from the last report while playing, up to the end", () => {
    expect(estimatedPosition(tv, at * 1000 + 15_000)).toBe(75_000);
    expect(estimatedPosition(tv, at * 1000 + 600_000)).toBe(120_000);
    // A clock behind the server's doesn't go back.
    expect(estimatedPosition(tv, at * 1000 - 5_000)).toBe(60_000);
    expect(estimatedPosition({ ...tv, duration_ms: null }, at * 1000 + 600_000)).toBe(660_000);
  });

  it("stays where it was when paused or buffering", () => {
    expect(estimatedPosition({ ...tv, state: "paused" }, at * 1000 + 15_000)).toBe(60_000);
    expect(estimatedPosition({ ...tv, state: "buffering" }, at * 1000 + 15_000)).toBe(60_000);
  });
});

describe("commandAnswered", () => {
  const sentAt = at * 1000 + 2_000;
  const sent = (command: SentCommand["command"], target?: number): SentCommand => ({ command, before: tv, sentAt, target });
  const by = sentAt + 1_000;

  it("sees a pause, another title, and a session gone", () => {
    expect(commandAnswered(sent("pause"), tv, by)).toBe(false);
    expect(commandAnswered(sent("pause"), { ...tv, state: "paused", updated_at: at + 2 }, by)).toBe(true);
    expect(commandAnswered(sent("next"), { ...tv, item: makeItem({ id: 8 }) }, by)).toBe(true);
    expect(commandAnswered(sent("next"), undefined, by)).toBe(true);
  });

  it("sees a seek or skip in a report from somewhere new", () => {
    // Only the usual report since, where it would be by then.
    expect(commandAnswered(sent("skip", 92_000), { ...tv, position_ms: 64_000, updated_at: at + 4 }, by)).toBe(false);
    expect(commandAnswered(sent("skip", 92_000), { ...tv, position_ms: 93_000, updated_at: at + 3 }, by)).toBe(true);
    expect(commandAnswered(sent("seek", 10_000), { ...tv, position_ms: 10_500, updated_at: at + 3 }, by)).toBe(true);
    // No report since says nothing of it.
    expect(commandAnswered(sent("seek", 10_000), tv, by)).toBe(false);
  });

  it("goes by the server's times alone, whatever this browser's clock says", () => {
    // A browser a minute ahead of the server, or behind it.
    for (const skew of [60_000, -60_000]) {
      const off: SentCommand = { ...sent("skip", 92_000), sentAt: sentAt + skew };
      expect(commandAnswered(off, { ...tv, position_ms: 93_000, updated_at: at + 3 }, by + skew)).toBe(true);
      expect(commandAnswered(off, tv, by + skew)).toBe(false);
    }
  });

  it("sees a previous that starts the same title over", () => {
    expect(commandAnswered(sent("previous", 0), { ...tv, position_ms: 0, updated_at: at + 2 }, by)).toBe(true);
    // A few seconds in, too near the start to have moved far.
    const early = { ...tv, position_ms: 4_000 };
    expect(commandAnswered({ ...sent("previous", 0), before: early }, { ...early, position_ms: 0, updated_at: at + 2 }, by)).toBe(true);
    expect(commandAnswered(sent("previous", 0), tv, by)).toBe(false);
  });
});

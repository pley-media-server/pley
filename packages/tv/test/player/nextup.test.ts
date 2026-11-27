import { describe, expect, it } from "vitest";
import type { Api } from "../../src/api/client";
import type { ItemSummary } from "../../src/api/types";
import { CountdownFill, hasNext, nextAfter } from "../../src/player/nextup";

const item = (id: number, kind: ItemSummary["kind"] = "movie") => ({ id, kind }) as ItemSummary;

describe("what plays next through a merged collection", () => {
  it("is the next movie in the row's order, across its libraries", async () => {
    const asked: number[][] = [];
    const api = {
      mergedCollection: (ids: number[]) => {
        asked.push(ids);
        return Promise.resolve({ title: "Award Winners", collections: [], items: [item(1), item(9, "show"), item(2), item(3)] });
      },
    } as unknown as Api;
    const queue = { kind: "merged" as const, ids: [12, 31] };
    expect(hasNext("movie", queue)).toBe(true);
    expect((await nextAfter(api, 1, "movie", queue))?.id).toBe(2);
    expect(await nextAfter(api, 3, "movie", queue)).toBeNull();
    expect(asked).toEqual([
      [12, 31],
      [12, 31],
    ]);
  });
});

describe("Up Next's ring between the countdown's whole seconds", () => {
  it("glides through each second to where it stands at that second's end", () => {
    const fill = new CountdownFill(10);
    fill.set(10, 0);
    expect(fill.at(0)).toBeCloseTo(0);
    expect(fill.at(500)).toBeCloseTo(0.05);
    expect(fill.moving(500)).toBe(true);
    expect(fill.at(1_000)).toBeCloseTo(0.1);
    expect(fill.moving(1_000)).toBe(false);
    fill.set(9, 1_000);
    expect(fill.at(1_500)).toBeCloseTo(0.15);
  });

  it("is full as the countdown reaches nothing", () => {
    const fill = new CountdownFill(10);
    for (let left = 10; left >= 1; left--) fill.set(left, (10 - left) * 1_000);
    expect(fill.at(10_000)).toBeCloseTo(1);
  });

  it("carries on from where it is when a second comes a little early", () => {
    const fill = new CountdownFill(10);
    fill.set(10, 0);
    fill.set(9, 960);
    expect(fill.at(960)).toBeCloseTo(0.096);
    expect(fill.at(1_960)).toBeCloseTo(0.2);
  });

  it("jumps back for a countdown started again, and starts where a countdown shown midway is", () => {
    const fill = new CountdownFill(10);
    for (let left = 10; left >= 6; left--) fill.set(left, (10 - left) * 1_000);
    fill.set(10, 5_000);
    expect(fill.at(5_000)).toBeCloseTo(0);
    const midway = new CountdownFill(10);
    midway.set(4, 0);
    expect(midway.at(0)).toBeCloseTo(0.6);
  });
});

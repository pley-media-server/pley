import { describe, expect, it } from "vitest";
import type { Api } from "../../src/api/client";
import type { ItemSummary } from "../../src/api/types";
import { CountdownFill, hasNext, nextAfter } from "../../src/player/nextup";
// Shared with the native Samsung app's tests, which must answer the same.
import countdown from "../fixtures/countdown-fill.json";

const item = (id: number, kind: ItemSummary["kind"] = "movie") => ({ id, kind }) as ItemSummary;

/** A step of the fixture: the countdown shows `set` at `ms`, or the fill there is `fill` (and moving or not). */
type CountdownStep = { ms: number; set?: number; fill?: number; moving?: boolean };

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

describe("Up Next's ring between the countdown's whole seconds, as the shared fixture has it", () => {
  for (const c of countdown.cases) {
    it(c.name, () => {
      const fill = new CountdownFill(countdown.total);
      for (const step of c.steps as CountdownStep[]) {
        if (step.set !== undefined) {
          fill.set(step.set, step.ms);
          continue;
        }
        expect([step.ms, Number(fill.at(step.ms).toFixed(3))]).toEqual([step.ms, step.fill]);
        if (step.moving !== undefined) expect([step.ms, fill.moving(step.ms)]).toEqual([step.ms, step.moving]);
      }
    });
  }
});

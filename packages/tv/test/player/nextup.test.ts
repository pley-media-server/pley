import { describe, expect, it } from "vitest";
import type { Api } from "../../src/api/client";
import type { ItemSummary } from "../../src/api/types";
import { hasNext, nextAfter } from "../../src/player/nextup";

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

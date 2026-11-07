import { describe, expect, it } from "vitest";
import type { Hub, ItemSummary } from "@/api/types";
import { makeItem } from "@/test/fixtures";
import { pickFeatured } from "./featured";

const art = (id: number, overrides: Partial<ItemSummary> = {}) =>
  makeItem({ id, title: `Title ${id}`, backdrop_url: `/api/items/${id}/images/backdrop`, ...overrides });

const watched = { position_ms: 0, watched: true, play_count: 1 };

const hub = (key: string, items: ItemSummary[]): Hub => ({ key, title: key, library_id: null, items });

describe("pickFeatured", () => {
  it("takes the newest of each library in turn", () => {
    const hubs = [
      hub("continue_watching", [art(1)]),
      hub("recently_added:1", [art(10), art(11), art(12)]),
      hub("recently_added:2", [art(20, { kind: "show" })]),
    ];
    expect(pickFeatured(hubs).map((f) => f.item.id)).toEqual([10, 20, 11, 12]);
  });

  it("leaves out what was watched, has no backdrop, or isn't a film or show", () => {
    const hubs = [
      hub("recently_added:1", [
        art(1, { user_state: watched }),
        art(2, { backdrop_url: null }),
        art(3, { kind: "album" }),
        art(4, { kind: "episode" }),
        art(5),
      ]),
    ];
    expect(pickFeatured(hubs).map((f) => f.item.id)).toEqual([5]);
  });

  it("fills up from My List, once each, and stops at the count", () => {
    const hubs = [hub("recently_added:1", [art(1), art(2)]), hub("my_list", [art(2), art(3), art(4), art(5)])];
    expect(pickFeatured(hubs, 4).map((f) => [f.item.id, f.from])).toEqual([
      [1, "new"],
      [2, "new"],
      [3, "list"],
      [4, "list"],
    ]);
  });

  it("features watched titles when nothing else is left", () => {
    const hubs = [
      hub("recently_added:1", [art(1, { user_state: watched }), art(2, { user_state: watched, backdrop_url: null })]),
      hub("recently_added:2", [art(3, { kind: "show", user_state: watched })]),
    ];
    expect(pickFeatured(hubs).map((f) => f.item.id)).toEqual([1, 3]);
  });

  it("features nothing without pictures", () => {
    expect(pickFeatured([hub("recently_added:1", [makeItem()])])).toEqual([]);
  });
});

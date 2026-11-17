import { describe, expect, it } from "vitest";
import type { CategoryRef, CollectionCategory } from "../../src/api/types";
import { collectionsView, namedCategory, type CategoryChoice } from "../../src/ui/screens/collection-groups";
import fixture from "../fixtures/collections.json";

/** The Android app's CollectionsView, pinned for both Samsung apps by collections.json. */
interface Case {
  name: string;
  categories?: CollectionCategory[];
  /** `category` left out: a server older than categories names none. */
  collections: { id: number; category?: number; title?: string }[];
  chosen: CategoryChoice;
  out: {
    groups: { key: string; title: string; ids: number[] }[];
    choices: string[];
    category: CategoryChoice;
    headed: boolean;
    offered: boolean;
    count: number;
    total: number;
  };
}

const all = fixture.categories as CollectionCategory[];

/** A collection of the case, its category named as the server names it. */
function collection(c: Case["collections"][number]): { id: number; category?: CategoryRef } {
  if (c.category === undefined) return { id: c.id };
  const known = all.find((x) => x.id === c.category);
  return { id: c.id, category: { id: c.category, key: known?.key ?? null, title: c.title ?? known?.title ?? "" } };
}

describe("the Collections tab's groups", () => {
  for (const c of fixture.cases as Case[]) {
    it(c.name, () => {
      const view = collectionsView(c.collections.map(collection), c.categories ?? all, c.chosen);
      expect(view.groups.map((g) => ({ key: g.key, title: g.category.title, ids: g.collections.map((x) => x.id) }))).toEqual(c.out.groups);
      expect(view.choices.map((g) => g.key)).toEqual(c.out.choices);
      expect(view.category).toBe(c.out.category);
      expect(view.headed).toBe(c.out.headed);
      expect(view.offered).toBe(c.out.offered);
      expect(view.count).toBe(c.out.count);
      expect(view.total).toBe(c.out.total);
    });
  }
});

describe("collections from a server older than categories", () => {
  it("make one group, without headings or a filter", () => {
    const view = collectionsView<{ id: number; category?: CategoryRef }>([{ id: 1 }, { id: 2 }], [], "all");
    expect(view.groups.map((g) => g.collections.map((x) => x.id))).toEqual([[1, 2]]);
    expect(view.headed).toBe(false);
    expect(view.offered).toBe(false);
  });
});

describe("the category a collection's page names, as collections.json has it", () => {
  for (const c of fixture.named as { name: string; category?: CategoryRef; named: string | null }[]) {
    it(c.name, () => {
      expect(namedCategory(c.category ? { category: c.category } : {})).toBe(c.named);
    });
  }
});

describe("the category a collection's page names", () => {
  it("is its category's title, but not Other's, and none without one", () => {
    expect(namedCategory({ category: { id: 2, key: "franchises", title: "Franchises" } })).toBe("Franchises");
    expect(namedCategory({ category: { id: 5, key: null, title: "Seasonal" } })).toBe("Seasonal");
    expect(namedCategory({ category: { id: 3, key: "other", title: "Autres" } })).toBeNull();
    expect(namedCategory({})).toBeNull();
  });
});

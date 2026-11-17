import type { CategoryRef, CollectionCategory } from "../../api/types";

/** The category a collection's page names beside "Collection": none for Other, which only says it has none. */
export function namedCategory(c: { category?: CategoryRef }): string | null {
  return c.category && c.category.key !== "other" ? c.category.title : null;
}

/** The Collections tab's Category filter: every one, or one category by its id. */
export type CategoryChoice = "all" | number;

/** Collections of one category (Other, the server's, for those without one). */
export interface CollectionGroup<T> {
  category: CategoryRef;
  collections: T[];
  /** The key of its heading and its row: the category's id. */
  key: string;
  /** The choice that picks it. */
  choice: CategoryChoice;
}

/**
 * A library's collections as the Collections tab shows them (apps/web's
 * CollectionsTab; the Android app's CollectionsView): those of the
 * category chosen, by category. `packages/tv/test/fixtures/collections.json`
 * pins it for both Samsung apps.
 */
export interface CollectionsView<T> {
  groups: CollectionGroup<T>[];
  /** The categories that have collections, offered in the Category filter. */
  choices: CollectionGroup<T>[];
  /** The category chosen: one that has no collections any more chooses every one. */
  category: CategoryChoice;
  /** How many collections the library has, whatever the filter. */
  total: number;
  /** How many collections show. */
  count: number;
  /** The groups have headings: not when only one shows. */
  headed: boolean;
  /** A category is chosen. */
  filtered: boolean;
  /** The Category filter is offered: more than one category to choose from, or one chosen. */
  offered: boolean;
}

/**
 * `collections` by the category the server gives each (its Other for those
 * without one), in the order of `categories`; empty groups left out. A
 * collection of a category not listed (made since the list was read)
 * comes after the rest, under its own category.
 */
export function groupByCategory<T extends { category?: CategoryRef }>(collections: T[], categories: CollectionCategory[]): CollectionGroup<T>[] {
  const groups: CollectionGroup<T>[] = categories.map((c) => group({ id: c.id, key: c.key, title: c.title }));
  for (const x of collections) {
    // A server older than categories names none: its collections make one group, shown without a heading.
    const category = x.category ?? NONE;
    let g = groups.filter((y) => y.category.id === category.id)[0];
    if (!g) {
      g = group(category);
      groups.push(g);
    }
    g.collections.push(x);
  }
  return groups.filter((g) => g.collections.length > 0);
}

const NONE: CategoryRef = { id: 0, key: null, title: "" };

function group<T>(category: CategoryRef): CollectionGroup<T> {
  return { category, collections: [], key: String(category.id), choice: category.id };
}

/** `collections` of the category `chosen`, grouped by `categories` in their order. */
export function collectionsView<T extends { category?: CategoryRef }>(collections: T[], categories: CollectionCategory[], chosen: CategoryChoice): CollectionsView<T> {
  const choices = groupByCategory(collections, categories);
  const category = chosen === "all" || choices.some((g) => g.choice === chosen) ? chosen : "all";
  const groups = choices.filter((g) => category === "all" || g.choice === category);
  const filtered = category !== "all";
  let count = 0;
  for (const g of groups) count += g.collections.length;
  return { groups, choices, category, total: collections.length, count, headed: groups.length > 1, filtered, offered: choices.length > 1 || filtered };
}

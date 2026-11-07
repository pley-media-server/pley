import type { Hub, Id, ItemSummary } from "@/api/types";

/** How many titles the home screen's banner takes turns showing. */
export const FEATURED_COUNT = 6;

/** Kinds the banner shows: those with a backdrop to fill it and a page to open. */
const FEATURED_KINDS = new Set<ItemSummary["kind"]>(["movie", "show"]);

/** A title on the banner, and why it's there: new in its library, or on the user's My List. */
export interface Featured {
  item: ItemSummary;
  from: "new" | "list";
}

function hasArt(item: ItemSummary): boolean {
  return FEATURED_KINDS.has(item.kind) && !!item.backdrop_url;
}

const unwatched = (item: ItemSummary) => hasArt(item) && !item.user_state.watched;

/**
 * The titles the home screen's banner features: the newest unwatched films
 * and shows with a backdrop, taken in turn from each library's Recently Added
 * (so films and shows alternate), then from My List when that is too few.
 * When everything has been watched, the newest are featured all the same.
 */
export function pickFeatured(hubs: Hub[], count = FEATURED_COUNT): Featured[] {
  const picked = pick(hubs, count, unwatched);
  return picked.length ? picked : pick(hubs, count, hasArt);
}

function pick(hubs: Hub[], count: number, featurable: (item: ItemSummary) => boolean): Featured[] {
  const sources = hubs.filter((h) => h.key.startsWith("recently_added")).map((h) => h.items.filter(featurable));
  const picked: Featured[] = [];
  const seen = new Set<Id>();
  const take = (item: ItemSummary, from: Featured["from"]) => {
    if (seen.has(item.id) || picked.length >= count) return;
    seen.add(item.id);
    picked.push({ item, from });
  };
  for (let i = 0; picked.length < count && sources.some((s) => i < s.length); i++) {
    for (const source of sources) {
      const item = source[i];
      if (item) take(item, "new");
    }
  }
  for (const item of hubs.find((h) => h.key === "my_list")?.items ?? []) {
    if (featurable(item)) take(item, "list");
  }
  return picked;
}

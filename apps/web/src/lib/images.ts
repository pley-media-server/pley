import type { Id, ItemSummary } from "@/api/types";

export const IMAGE_BUCKETS = [160, 240, 320, 480, 640, 960, 1280, 1920] as const;

/** Smallest bucket that covers `cssWidth` CSS pixels at the current DPR. */
export function bucketFor(cssWidth: number): number {
  const dpr = typeof window !== "undefined" ? window.devicePixelRatio || 1 : 1;
  const need = Math.ceil(cssWidth * Math.min(dpr, 3));
  return IMAGE_BUCKETS.find((b) => b >= need) ?? Math.max(...IMAGE_BUCKETS);
}

/** Append `w=<bucket>` to an image url returned by the API (which already carries `?v=`). */
export function sizedImage(url: string | null | undefined, cssWidth: number): string | null {
  if (!url) return null;
  const sep = url.includes("?") ? "&" : "?";
  return `${url}${sep}w=${bucketFor(cssWidth)}`;
}

export type ImageKind = "poster" | "backdrop" | "thumb";

/** Direct image url for an item id (used for parent/grandparent fallbacks). */
export function itemImageUrl(id: Id, kind: ImageKind): string {
  return `/api/items/${id}/images/${kind}`;
}

/** Ordered list of candidate urls for a backdrop: own → show/artist → parent. */
export function backdropCandidates(item: ItemSummary): string[] {
  const out: string[] = [];
  if (item.backdrop_url) out.push(item.backdrop_url);
  if (item.grandparent_id) out.push(itemImageUrl(item.grandparent_id, "backdrop"));
  if (item.parent_id) out.push(itemImageUrl(item.parent_id, "backdrop"));
  return out;
}

/** Ordered list of candidate urls for a poster: own → parent → grandparent. */
export function posterCandidates(item: ItemSummary): string[] {
  const out: string[] = [];
  if (item.poster_url) out.push(item.poster_url);
  if (item.parent_id) out.push(itemImageUrl(item.parent_id, "poster"));
  if (item.grandparent_id) out.push(itemImageUrl(item.grandparent_id, "poster"));
  return out;
}

/**
 * Ordered list of candidate urls for a 16:9 thumb: a movie's backdrop first
 * (its thumb is a frame grab, made on demand), otherwise the own thumb, then
 * the show's backdrop.
 */
export function thumbCandidates(item: ItemSummary): string[] {
  const out: string[] = [];
  if (item.kind === "movie" && item.backdrop_url) out.push(item.backdrop_url);
  if (item.thumb_url) out.push(item.thumb_url);
  if (item.kind === "episode" && item.grandparent_id) out.push(itemImageUrl(item.grandparent_id, "backdrop"));
  if (item.kind !== "episode" && item.backdrop_url && !out.includes(item.backdrop_url)) out.push(item.backdrop_url);
  return out;
}

/** A title's own gradient for its picture's place (packages/placeholder, as every app draws it). */
export { placeholderGradient } from "pley-placeholder";

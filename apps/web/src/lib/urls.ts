import type { Id } from "@/api/types";
import { encodeId } from "./ids";

/** Page URLs, with short ids (see `ids.ts`). `query` goes after the `?`. */
export const itemUrl = (id: Id) => `/item/${encodeId(id)}`;
export const libraryUrl = (id: Id, query?: string) => `/library/${encodeId(id)}${query ? `?${query}` : ""}`;
export const personUrl = (id: Id) => `/person/${encodeId(id)}`;
export const collectionUrl = (id: Id) => `/collection/${encodeId(id)}`;
export const playlistUrl = (id: Id) => `/playlists/${encodeId(id)}`;
/** Search: the libraries' titles and, for those who may ask, TMDB's. */
export const searchUrl = (q?: string) => `/search${q?.trim() ? `?q=${encodeURIComponent(q.trim())}` : ""}`;
/** Discover: TMDB's lists, or all it finds for `q` (the See all of Search's Discover row). */
export const discoverUrl = (q?: string) => `/discover${q?.trim() ? `?q=${encodeURIComponent(q.trim())}` : ""}`;
/** A movie or show on TMDB, to ask for (TMDB's id). */
export const titleUrl = (kind: "movie" | "show", tmdbId: number) => `/discover/${kind}/${tmdbId}`;
export const requestsUrl = "/requests";

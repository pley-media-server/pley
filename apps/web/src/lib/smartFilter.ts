// Smart collection filters: a library listing in Plex's syntax
// (`genre=Comedy&unwatched=1&sort=year:desc`), see docs/COMPATIBILITY.md.

import type { LibrarySort, RangeFilter, Resolution, SortOrder } from "@/api/types";
import { RANGE_FILTER_LABELS, rangeFilterOf, RESOLUTION_LABELS, resolutionOf } from "./format";

const PLEX_SORT: Record<LibrarySort, string> = {
  title: "titleSort",
  year: "year",
  added: "addedAt",
  rating: "rating",
  user_rating: "userRating",
  release: "originallyAvailableAt",
  duration: "duration",
  last_played: "lastViewedAt",
  bitrate: "mediaBitrate",
};

/** The library page's filters and order as a smart collection filter. */
export function smartFilterOf(query: {
  sort: LibrarySort;
  order: SortOrder;
  watched?: boolean;
  genre?: string;
  year?: number;
  resolution?: Resolution;
  dynamic_range?: RangeFilter;
}): string {
  const params = new URLSearchParams();
  if (query.genre) params.set("genre", query.genre);
  if (query.year !== undefined) params.set("year", String(query.year));
  // Plex's `resolution`, and Pley's own `dynamicRange`.
  if (query.resolution) params.set("resolution", query.resolution);
  if (query.dynamic_range) params.set("dynamicRange", query.dynamic_range);
  if (query.watched !== undefined) params.set("unwatched", query.watched ? "0" : "1");
  const field = PLEX_SORT[query.sort];
  params.set("sort", query.order === "desc" ? `${field}:desc` : field);
  return params.toString();
}

const SORT_NOTES: Record<string, [string, string]> = {
  titleSort: ["By title", "By title, Z to A"],
  year: ["Oldest first", "Newest first"],
  originallyAvailableAt: ["Earliest release first", "Latest release first"],
  addedAt: ["First added first", "Recently added first"],
  rating: ["Lowest rated first", "Best rated first"],
  audienceRating: ["Lowest rated first", "Best rated first"],
  userRating: ["Your lowest rated first", "Your best rated first"],
  lastViewedAt: ["Least recently played first", "Recently played first"],
  duration: ["Shortest first", "Longest first"],
  viewCount: ["Least played first", "Most played first"],
};

function sortNote(value: string): string {
  const first = value.split(",")[0] ?? "";
  const [field = "", direction] = first.split(":");
  // python-plexapi may name the kind first: `movie.year`.
  const name = field.includes(".") ? field.slice(field.indexOf(".") + 1) : field;
  const notes = SORT_NOTES[name];
  if (!notes) return `Sorted by ${name}`;
  return direction === "desc" ? notes[1] : notes[0];
}

/** What a smart collection's filter finds, in words: "Genre: Comedy", "Unwatched", … */
export function describeSmartFilter(filter: string): string[] {
  const parts: string[] = [];
  for (const [key, value] of new URLSearchParams(filter)) {
    switch (key) {
      case "genre":
        parts.push(`Genre: ${value}`);
        break;
      case "label":
        parts.push(`Label: ${value}`);
        break;
      case "studio":
        parts.push(`Studio: ${value}`);
        break;
      case "contentRating":
        parts.push(`Rated ${value}`);
        break;
      case "unwatched":
        parts.push(value === "0" || value === "false" ? "Watched" : "Unwatched");
        break;
      case "inProgress":
        parts.push(value === "0" || value === "false" ? "Not in progress" : "In progress");
        break;
      case "year":
        parts.push(`From ${value}`);
        break;
      case "year>>":
        parts.push(`After ${value}`);
        break;
      case "year<<":
        parts.push(`Before ${value}`);
        break;
      case "decade":
        parts.push(`The ${value}s`);
        break;
      case "resolution": {
        const resolution = resolutionOf(value.toLowerCase());
        parts.push(resolution ? RESOLUTION_LABELS[resolution] : `Resolution: ${value}`);
        break;
      }
      case "dynamicRange": {
        const range = rangeFilterOf(value.toLowerCase());
        parts.push(range ? RANGE_FILTER_LABELS[range] : `Dynamic range: ${value}`);
        break;
      }
      case "sort":
        parts.push(sortNote(value));
        break;
      case "limit":
        parts.push(`At most ${value}`);
        break;
      // Plumbing, not part of what is found.
      case "type":
      case "includeGuids":
        break;
      default:
        parts.push(`${key}: ${value}`);
    }
  }
  return parts;
}

// Smart collection filters: a library listing in Plex's syntax
// (`genre=Comedy&unwatched=1&sort=year:desc`), see docs/COMPATIBILITY.md.

import type { LibrarySort, RangeFilter, Resolution, SortOrder } from "@/api/types";
import { t } from "@/i18n";
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

/** The sort fields that have words, each by its words' key (the audience's rating reads as the rating). */
const SORT_NOTES = {
  titleSort: "titleSort",
  year: "year",
  originallyAvailableAt: "originallyAvailableAt",
  addedAt: "addedAt",
  rating: "rating",
  audienceRating: "rating",
  userRating: "userRating",
  lastViewedAt: "lastViewedAt",
  duration: "duration",
  viewCount: "viewCount",
} as const;

function sortNote(value: string): string {
  const first = value.split(",")[0] ?? "";
  const [field = "", direction] = first.split(":");
  // python-plexapi may name the kind first: `movie.year`.
  const name = field.includes(".") ? field.slice(field.indexOf(".") + 1) : field;
  if (!Object.hasOwn(SORT_NOTES, name)) return t("smartFilter.sortedBy", { name });
  const notes = SORT_NOTES[name as keyof typeof SORT_NOTES];
  return t(`smartFilter.sort.${notes}.${direction === "desc" ? "desc" : "asc"}`);
}

/** What a smart collection's filter finds, in words: "Genre: Comedy", "Unwatched", … */
export function describeSmartFilter(filter: string): string[] {
  const parts: string[] = [];
  for (const [key, value] of new URLSearchParams(filter)) {
    switch (key) {
      case "genre":
        parts.push(t("smartFilter.genre", { value }));
        break;
      case "label":
        parts.push(t("smartFilter.label", { value }));
        break;
      case "studio":
        parts.push(t("smartFilter.studio", { value }));
        break;
      case "contentRating":
        parts.push(t("smartFilter.rated", { value }));
        break;
      case "unwatched":
        parts.push(value === "0" || value === "false" ? t("smartFilter.watched") : t("smartFilter.unwatched"));
        break;
      case "inProgress":
        parts.push(value === "0" || value === "false" ? t("smartFilter.notInProgress") : t("smartFilter.inProgress"));
        break;
      case "year":
        parts.push(t("smartFilter.year", { value }));
        break;
      case "year>>":
        parts.push(t("smartFilter.after", { value }));
        break;
      case "year<<":
        parts.push(t("smartFilter.before", { value }));
        break;
      case "decade":
        parts.push(t("smartFilter.decade", { value }));
        break;
      case "resolution": {
        const resolution = resolutionOf(value.toLowerCase());
        parts.push(resolution ? RESOLUTION_LABELS[resolution] : t("smartFilter.resolution", { value }));
        break;
      }
      case "dynamicRange": {
        const range = rangeFilterOf(value.toLowerCase());
        parts.push(range ? RANGE_FILTER_LABELS[range] : t("smartFilter.dynamicRange", { value }));
        break;
      }
      case "sort":
        parts.push(sortNote(value));
        break;
      case "limit":
        parts.push(t("smartFilter.limit", { value }));
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

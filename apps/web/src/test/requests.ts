import type { DiscoverDetail, DiscoverSeason, DiscoverTitle, MediaRequest, RequestList, RequestStatus, RequestSummary } from "@/api/requests";

/** Requests on, for an account that asks an administrator, without limits. */
export function makeSummary(overrides: Partial<RequestSummary> = {}): RequestSummary {
  return {
    enabled: true,
    access: "ask",
    movies_left: null,
    seasons_left: null,
    movies_limit: null,
    seasons_limit: null,
    movies_next_at: null,
    seasons_next_at: null,
    manage: false,
    pending: 0,
    failed: 0,
    unseen: 0,
    own: 0,
    changed_at: null,
    sign_in_again: null,
    ...overrides,
  };
}

/** A movie on TMDB no library has. */
export function makeTitle(overrides: Partial<DiscoverTitle> = {}): DiscoverTitle {
  return {
    kind: "movie",
    tmdb_id: 603,
    title: "The Matrix",
    year: 1999,
    overview: "A hacker learns what the world is.",
    poster_url: "/api/discover/images/poster/matrix.jpg",
    backdrop_url: "/api/discover/images/backdrop/matrix.jpg",
    rating: 8.2,
    status: "none",
    item_id: null,
    request_id: null,
    ...overrides,
  };
}

/** A whole aired season no library has. */
export function makeSeason(number: number, overrides: Partial<DiscoverSeason> = {}): DiscoverSeason {
  return {
    number,
    name: number === 0 ? "Specials" : `Season ${number}`,
    episodes: 10,
    aired: 10,
    air_date: "2020-01-01",
    poster_url: null,
    in_library: 0,
    requested: false,
    state: "missing",
    declined: false,
    request_id: null,
    ...overrides,
  };
}

export function makeDetail(overrides: Partial<DiscoverDetail> = {}): DiscoverDetail {
  return {
    ...makeTitle(),
    original_title: null,
    tagline: null,
    genres: ["Action"],
    release_date: "1999-03-31",
    runtime_minutes: 136,
    certification: "R",
    network: "Warner Bros.",
    show_status: null,
    anime: false,
    cast: [],
    seasons: null,
    imdb_id: "tt0133093",
    declined: null,
    restriction: null,
    watching: false,
    requests: [],
    movies_left: null,
    seasons_left: null,
    movies_next_at: null,
    seasons_next_at: null,
    ...overrides,
  };
}

/** A show on TMDB with three aired seasons and specials, none of them in a library. */
export function makeShowDetail(overrides: Partial<DiscoverDetail> = {}): DiscoverDetail {
  return makeDetail({
    kind: "show",
    tmdb_id: 1399,
    title: "Dark",
    year: 2017,
    release_date: "2017-12-01",
    runtime_minutes: null,
    certification: "TV-MA",
    network: "Netflix",
    show_status: "Ended",
    seasons: [makeSeason(0), makeSeason(1), makeSeason(2), makeSeason(3)],
    ...overrides,
  });
}

let nextId = 1;

/** A pending request for the movie, by account 2 (Ann). */
export function makeRequest(overrides: Partial<MediaRequest> = {}): MediaRequest {
  return {
    id: overrides.id ?? nextId++,
    kind: "movie",
    tmdb_id: 603,
    title: "The Matrix",
    year: 1999,
    poster_url: null,
    backdrop_url: null,
    seasons: null,
    available_seasons: [],
    anime: false,
    status: "pending",
    stage: overrides.stage ?? overrides.status ?? "pending",
    stage_at: null,
    reason: null,
    note: null,
    mine: false,
    watching: false,
    watchers: 0,
    unseen: false,
    actions: [],
    requested_by: { id: 2, name: "Ann", avatar_url: null, avatar_color: null },
    decided_by: null,
    decided_at: null,
    created_at: 1_700_000_000,
    updated_at: 1_700_000_000,
    available_at: null,
    retry_at: null,
    sent_to: null,
    item_id: null,
    download: null,
    ...overrides,
  };
}

/** A page of `requests`, counted as they are. */
export function makeRequestList(requests: MediaRequest[], total = requests.length, people: RequestList["people"] = []): RequestList {
  const counts: Record<RequestStatus, number> = { pending: 0, approved: 0, declined: 0, failed: 0, available: 0 };
  for (const r of requests) counts[r.status] += 1;
  return { total, counts, requests, people };
}

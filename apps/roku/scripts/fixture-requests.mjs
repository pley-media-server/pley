// Requests and Discover for the fake server (scripts/fixture-server.mjs),
// answered as docs/API.md "Requests" has them: TMDB's titles made up, the
// requests of the household and what each profile may do with them. Anna
// (1) is an administrator: with `approvals` she manages requests, and
// what she asks for is approved as she asks. Ben (2) asks, 3 movies and 10
// seasons per 7 days. Nothing goes anywhere: there is no TMDB, Radarr or
// Sonarr, and pictures are plain PNGs.

const DAY = 86400;
const PAGE = 20;
const STATUSES = ["pending", "approved", "declined", "failed", "available"];
const ACTIVE = ["pending", "approved", "failed"];
const LISTS = ["trending", "movies", "shows", "upcoming", "anime"];
const PICTURE_KINDS = ["poster", "backdrop", "profile", "still"];
const PICTURE_FILE = /^[A-Za-z0-9]{1,64}\.(jpg|png)$/;
// Pictures in these colours, picked by the file's name.
const PALETTE = [[0x2b, 0x6c, 0xb0], [0x1f, 0x8a, 0x70], [0x8a, 0x5a, 0x3c], [0x6b, 0x4c, 0x9a], [0xa8, 0x3f, 0x5b], [0x3f, 0x7f, 0x8f], [0x9a, 0x7b, 0x2e], [0x4a, 0x5d, 0x7a]];

// Who asks (RequestUser): the household's two, and an administrator of
// the server who isn't in it.
const USERS = {
  1: { id: 1, name: "Anna", avatar_url: null, avatar_color: "#38bdf8" },
  2: { id: 2, name: "Ben", avatar_url: null, avatar_color: "#f472b6" },
  3: { id: 3, name: "Sam", avatar_url: null, avatar_color: "#f59e0b" },
};
// What each may do. An administrator's asks from pley's own apps are
// approved as made, with no limits.
const ACCOUNTS = {
  1: { admin: true, access: "auto", moviesLimit: null, seasonsLimit: null },
  2: { admin: false, access: "ask", moviesLimit: 3, seasonsLimit: 10 },
};

const MOVIES = [
  "The Last Lighthouse", "The Glass Orchard", "Copper Sky", "Paper Moons", "Small Hours", "The Cartographer", "Salt and Stone", "Echo Valley",
  "North of Nowhere", "The Velvet Hour", "Quiet Machines", "Lanterns", "The Long Field", "Morning Tide", "Iron Garden", "Blue Meridian",
  "The Clockmaker's Daughter", "Wild Orchard Road", "A Harbor in Winter", "Fable of Ash", "Second Light", "The Borrowed Coat", "Riverbend",
  "Glass Houses", "The Night Ferry", "Under the Linden", "Static", "The Weather Map", "Kites in March", "Orchard Hill", "The Silent Choir",
  "Brass Compass", "Low Tide", "The Painted Door", "Ember", "Winter Swimmers", "The Map Room", "Hollow Pines", "A Season of Rain", "The Glass Bell",
];
const UPCOMING = ["Starfall", "The Lantern Keeper", "Northbound", "Afterglow", "The Tin Crown", "Seven Bridges", "The Quiet Year", "Moth and Flame", "Harvest Moon Hotel", "Copperline"];
const SHOWS = [
  "Severance", "Northern Lights Station", "The Archive", "Tidewater", "Signal Lost", "Moonrise Kitchen", "Parallel Lines", "Desert Radio",
  "The Understudy", "Harbor Lights", "Cold Case Garden", "The Night Desk", "Field Notes", "Lighthouse Keepers", "Paper Trail", "The Long Winter",
];
const ANIME = ["Sky Lantern Academy", "Starlit Courier", "Blade of Morning", "The Paper Dragon", "Moonwake", "Spirit Ferry", "Neon Shrine", "Little Comet"];
const CAST = [["Ada Brook", "The Keeper"], ["Milo Hart", "Her Brother"], ["June Okafor", "The Captain"], ["Theo Lind", "The Stranger"], ["Rosa Vale", "Dr. Marsh"], ["Ivan Petrov", "The Neighbour"]];

const now = () => Math.floor(Date.now() / 1000);
const newest = (a, b) => b.createdAt - a.createdAt || b.id - a.id;
const ok = (body) => ({ status: 200, body });
const fail = (status, code, message, extra = {}) => ({ status, body: { code, message, ...extra } });
const flag = (query, name) => ["1", "true", "yes"].includes(query.get(name) ?? "");

// Seasons aired whole, numbered from 1: [episodes, …].
function airedSeasons(counts, firstYear) {
  return counts.map((episodes, i) => ({ number: i + 1, name: `Season ${i + 1}`, episodes, aired: episodes, air_date: `${firstYear + i}-03-0${1 + (i % 9)}`, poster_url: null }));
}

// fixture: the fake server's (requestScopes, requestsOn, household, and
// what the walk checks: requestsAsked, requestsActed, discoverPictures).
// png(width, height, colorAt) makes the pictures.
export function fakeRequests(fixture, png) {
  const start = now();
  const titles = new Map();
  const add = (kind, tmdbId, name, extra = {}) => {
    const n = titles.size;
    const year = extra.year ?? 2008 + (n % 17);
    titles.set(`${kind}:${tmdbId}`, {
      kind,
      tmdb_id: tmdbId,
      title: name,
      year,
      overview: `${name}: a made-up ${kind === "movie" ? "film" : "series"} of the fake server, with a sentence or two of what happens in it.`,
      poster_url: `/api/discover/images/poster/p${tmdbId}.jpg`,
      backdrop_url: `/api/discover/images/backdrop/b${tmdbId}.jpg`,
      rating: n % 9 === 8 ? null : (60 + ((n * 7) % 31)) / 10,
      anime: extra.anime ?? false,
      tagline: extra.tagline ?? null,
      genres: extra.anime ? ["Animation", "Adventure"] : n % 2 ? ["Drama", "Mystery"] : ["Comedy", "Romance"],
      release_date: extra.release ?? `${year}-0${1 + (n % 9)}-${10 + (n % 18)}`,
      runtime_minutes: kind === "movie" ? 88 + ((n * 3) % 60) : null,
      certification: kind === "movie" ? ["PG", "PG-13", "R"][n % 3] : ["TV-PG", "TV-14", "TV-MA"][n % 3],
      network: kind === "movie" ? "Fixture Pictures" : "Fixture Network",
      show_status: kind === "show" ? (extra.showStatus ?? "Returning Series") : null,
      seasons: kind === "show" ? (extra.seasons ?? airedSeasons([10, 10, 8], 2016 + (n % 6))) : null,
      // The libraries' copy: { itemId, seasons: { number: episodes } }.
      library: extra.library ?? null,
    });
  };
  MOVIES.forEach((name, i) => add("movie", 7001 + i, name, i === 0 ? { tagline: "Every light goes out once." } : i === 2 ? { library: { itemId: 1002 } } : {}));
  UPCOMING.forEach((name, i) => add("movie", 7101 + i, name, { year: 2027, release: `2027-${String(1 + i).padStart(2, "0")}-14` }));
  SHOWS.forEach((name, i) => {
    let extra = {};
    // The libraries' show: seasons 1 and 2 whole, 3 to ask for, 4 to come.
    if (i === 0) {
      extra = {
        year: 2022,
        seasons: [
          { number: 0, name: "Specials", episodes: 2, aired: 2, air_date: "2022-02-11", poster_url: null },
          { number: 1, name: "Season 1", episodes: 9, aired: 9, air_date: "2022-02-18", poster_url: "/api/discover/images/poster/s8001n1.jpg" },
          { number: 2, name: "Season 2", episodes: 9, aired: 9, air_date: "2025-01-17", poster_url: "/api/discover/images/poster/s8001n2.jpg" },
          { number: 3, name: "Season 3", episodes: 10, aired: 10, air_date: "2026-03-20", poster_url: null },
          { number: 4, name: "Season 4", episodes: 10, aired: 0, air_date: "2027-09-03", poster_url: null },
        ],
        library: { itemId: 99, seasons: { 1: 9, 2: 9 } },
      };
    }
    if (i === 2) extra = { seasons: [{ number: 1, name: "Season 1", episodes: 8, aired: 2, air_date: "2026-10-16", poster_url: null }] };
    if (i === 3) extra = { tagline: "The sea keeps what it takes.", seasons: [...airedSeasons([8, 8], 2023), { number: 3, name: "Season 3", episodes: 6, aired: 3, air_date: "2026-10-09", poster_url: null }] };
    add("show", 8001 + i, name, extra);
  });
  ANIME.forEach((name, i) => add("show", 8101 + i, name, { anime: true, seasons: airedSeasons([12, 12], 2019 + i) }));
  const keys = (kind, from, count) => Array.from({ length: count }, (_, i) => `${kind}:${from + i}`);
  const lists = {
    trending: ["movie:7001", "show:8004", "show:8001", "movie:7002", "movie:7003", "movie:7005", "show:8002", "show:8003", "show:8105", "movie:7004", "movie:7101", "show:8005", ...keys("movie", 7006, 8)],
    movies: keys("movie", 7001, MOVIES.length),
    shows: keys("show", 8001, SHOWS.length),
    upcoming: keys("movie", 7101, UPCOMING.length),
    anime: keys("show", 8101, ANIME.length),
  };

  // The requests as the server keeps them: seasons [] for a movie, and
  // watchers [{ id, at }] earliest first.
  const made = (fields) => ({
    seasons: [], availableSeasons: [], stageAt: null, reason: null, note: null, watchers: [], decidedBy: null, decidedAt: null,
    availableAt: null, retryAt: null, sentTo: null, download: null, newsAt: null, updatedAt: fields.createdAt, ...fields,
  });
  const requests = [
    made({ id: 1, userId: 2, kind: "movie", tmdbId: 7002, status: "pending", stage: "pending", createdAt: start - 3 * DAY }),
    made({
      id: 2, userId: 2, kind: "show", tmdbId: 8002, seasons: [1, 2], status: "failed", stage: "failed", reason: "Sonarr refused: the root folder isn't writable",
      decidedBy: 3, decidedAt: start - 2 * DAY + 600, createdAt: start - 2 * DAY, updatedAt: start - 2 * DAY + 900,
    }),
    made({
      id: 3, userId: 1, kind: "movie", tmdbId: 7004, status: "approved", stage: "downloading", decidedBy: 1, decidedAt: start - DAY, createdAt: start - DAY,
      updatedAt: start - 3600, sentTo: "radarr", download: { progress: 0.42, state: "downloading", etaIn: 1200, episodes: 1 },
    }),
    made({
      id: 4, userId: 2, kind: "show", tmdbId: 8003, seasons: [1], status: "approved", stage: "waiting", stageAt: start + 3 * DAY, note: "Sonarr numbers these seasons differently",
      decidedBy: 3, decidedAt: start - 5 * DAY + 3600, createdAt: start - 5 * DAY, updatedAt: start - 5 * DAY + 3600, sentTo: "sonarr",
    }),
    made({
      id: 5, userId: 2, kind: "movie", tmdbId: 7005, status: "declined", stage: "declined", reason: "We have it on disc already.", decidedBy: 3, decidedAt: start - DAY,
      createdAt: start - 6 * DAY, updatedAt: start - DAY, newsAt: start - DAY, watchers: [{ id: 1, at: start - 6 * DAY + 600 }],
    }),
    made({
      id: 6, userId: 1, kind: "movie", tmdbId: 7003, status: "available", stage: "available", decidedBy: 1, decidedAt: start - 9 * DAY, createdAt: start - 9 * DAY,
      updatedAt: start - 4 * DAY, availableAt: start - 4 * DAY, sentTo: "radarr", newsAt: start - 4 * DAY,
    }),
  ];
  let nextId = 7;
  // When each profile last looked at its requests: Anna hasn't seen that
  // the one she follows was declined.
  const seenAt = new Map([[1, start - 2 * DAY], [2, start]]);
  const pictures = new Map();

  const titleOf = (r) => titles.get(`${r.kind}:${r.tmdbId}`);
  const find = (id) => requests.find((r) => r.id === id);
  const drop = (r) => requests.splice(requests.indexOf(r), 1);
  const acted = (id, action, body = {}) => fixture.requestsActed.push({ id, action, ...body });
  const watches = (r, viewer) => r.watchers.some((w) => w.id === viewer.id);
  const ownOrFollowed = (r, viewer) => r.userId === viewer.id || watches(r, viewer);
  const sees = (r, viewer) => viewer.manage || ownOrFollowed(r, viewer);
  const isUnseen = (r, viewer) => ownOrFollowed(r, viewer) && r.newsAt !== null && r.newsAt > (seenAt.get(viewer.id) ?? 0);
  const conflict = (r, viewer, message) => fail(409, "conflict", message, { request: view(r, viewer) });

  // --- What each title is to the profile asking ---

  const activeFor = (title) => requests.filter((r) => r.kind === title.kind && r.tmdbId === title.tmdb_id && ACTIVE.includes(r.status));
  const haveOf = (title, number) => title.library?.seasons?.[number] ?? 0;
  const seasonWhole = (title, season) => season.aired > 0 && season.aired === season.episodes && haveOf(title, season.number) >= season.episodes;
  // The profile's requests (made or followed) declined in the last 30
  // days, the latest first.
  const declinedFor = (title, viewer) =>
    requests
      .filter((r) => r.kind === title.kind && r.tmdbId === title.tmdb_id && r.status === "declined" && ownOrFollowed(r, viewer) && now() < r.decidedAt + 30 * DAY)
      .sort((a, b) => b.decidedAt - a.decidedAt);
  const seasonState = (title, season) => {
    if (seasonWhole(title, season)) return "library";
    if (activeFor(title).some((r) => r.seasons.includes(season.number))) return "requested";
    if (haveOf(title, season.number) > 0) return "partial";
    return season.aired > 0 ? "missing" : "unaired";
  };
  const statusOf = (title, viewer) => {
    const active = activeFor(title);
    const declined = declinedFor(title, viewer);
    if (title.kind === "movie") {
      if (title.library) return "library";
      if (active.length > 0) return "requested";
      return declined.length > 0 ? "declined" : "none";
    }
    const lacking = title.seasons.filter((s) => s.number > 0 && s.aired > 0 && !seasonWhole(title, s));
    if (lacking.length === 0 && title.library) return "library";
    const covered = new Set(active.flatMap((r) => r.seasons));
    const left = lacking.filter((s) => !covered.has(s.number));
    if (lacking.length > 0 && left.length === 0) return "requested";
    if (declined.some((r) => r.seasons.some((n) => left.some((s) => s.number === n)))) return "declined";
    if ((title.library || covered.size > 0) && left.length > 0) return "partial";
    return "none";
  };
  const titleJson = (title, viewer) => ({
    kind: title.kind,
    tmdb_id: title.tmdb_id,
    title: title.title,
    year: title.year,
    overview: title.overview,
    poster_url: title.poster_url,
    backdrop_url: title.backdrop_url,
    rating: title.rating,
    status: statusOf(title, viewer),
    item_id: title.library?.itemId ?? null,
    request_id: activeFor(title).filter((r) => sees(r, viewer)).sort(newest)[0]?.id ?? null,
  });

  // --- Limits ---

  // Counted against the maker's limit: active or available, but not failed
  // ones the server won't retry.
  const counts = (r) => r.status === "pending" || r.status === "approved" || r.status === "available" || (r.status === "failed" && r.retryAt !== null);
  const allowanceOf = (viewer) => {
    const out = { movies_left: null, seasons_left: null, movies_next_at: null, seasons_next_at: null };
    const counted = requests.filter((r) => r.userId === viewer.id && r.createdAt > now() - 7 * DAY && counts(r)).sort((a, b) => a.createdAt - b.createdAt);
    if (viewer.moviesLimit !== null) {
      const movies = counted.filter((r) => r.kind === "movie");
      out.movies_left = Math.max(0, viewer.moviesLimit - movies.length);
      if (out.movies_left === 0) out.movies_next_at = movies[movies.length - viewer.moviesLimit].createdAt + 7 * DAY;
    }
    if (viewer.seasonsLimit !== null) {
      const shows = counted.filter((r) => r.kind === "show");
      const used = shows.reduce((sum, r) => sum + r.seasons.length, 0);
      out.seasons_left = Math.max(0, viewer.seasonsLimit - used);
      if (out.seasons_left === 0) out.seasons_next_at = shows[0].createdAt + 7 * DAY;
    }
    return out;
  };
  // When the oldest counted request of a kind stops counting.
  const freedAt = (viewer, kind) => {
    const oldest = requests.filter((r) => r.userId === viewer.id && r.kind === kind && r.createdAt > now() - 7 * DAY && counts(r)).sort((a, b) => a.createdAt - b.createdAt)[0];
    return oldest ? oldest.createdAt + 7 * DAY : null;
  };
  const tooMany = (retryAt, message) => ({
    status: 429,
    body: { code: "too_many_requests", message, retry_at: retryAt },
    headers: retryAt ? { "Retry-After": String(Math.max(1, retryAt - now())) } : {},
  });

  // --- A request (MediaRequest) as the profile sees it ---

  const actionsOf = (r, viewer) => {
    const out = [];
    const mine = r.userId === viewer.id;
    const following = watches(r, viewer) && !mine;
    const over = r.decidedAt !== null && now() >= r.decidedAt + 30 * DAY;
    const notSent = r.status === "approved" && r.sentTo === null;
    if (mine && (r.status === "pending" || r.status === "failed" || (r.status === "declined" && over))) out.push("withdraw");
    if (following) out.push("unwatch");
    if (viewer.manage) {
      if (r.status === "pending" || r.status === "declined" || notSent) out.push("approve");
      if (r.status === "failed") out.push("retry");
      if (r.status === "pending" || r.status === "failed" || r.status === "approved") out.push("decline");
      if (r.status === "pending" || r.status === "failed" || r.status === "declined" || notSent) out.push("handled");
      out.push("delete");
    }
    if (viewer.access !== "off" && ((r.stage === "gone" && (mine || following)) || (mine && r.status === "declined" && over))) out.push("ask_again");
    return out;
  };
  const view = (r, viewer) => {
    const title = titleOf(r);
    const mine = r.userId === viewer.id;
    let reason = null;
    if (r.status === "declined" && (mine || viewer.manage)) reason = r.reason;
    if (r.status === "failed" && viewer.manage) reason = r.reason;
    const arr = r.sentTo === "radarr" || r.sentTo === "sonarr";
    return {
      id: r.id,
      kind: r.kind,
      tmdb_id: r.tmdbId,
      title: title.title,
      year: title.year,
      poster_url: title.poster_url,
      backdrop_url: title.backdrop_url,
      seasons: r.kind === "show" ? r.seasons : null,
      available_seasons: r.availableSeasons,
      anime: title.anime,
      status: r.status,
      stage: r.stage,
      stage_at: r.stageAt,
      reason,
      note: viewer.manage ? r.note : null,
      mine,
      watching: watches(r, viewer),
      watchers: viewer.manage ? r.watchers.length : 0,
      unseen: isUnseen(r, viewer),
      actions: actionsOf(r, viewer),
      requested_by: viewer.manage || mine ? USERS[r.userId] : null,
      decided_by: (viewer.manage || mine) && r.decidedBy !== null ? USERS[r.decidedBy] : null,
      decided_at: r.decidedAt,
      created_at: r.createdAt,
      updated_at: r.updatedAt,
      available_at: r.availableAt,
      retry_at: r.retryAt,
      sent_to: viewer.manage && arr ? { service: r.sentTo, quality_profile: "HD-1080p", root_folder: r.sentTo === "radarr" ? "/media/movies" : "/media/shows" } : null,
      item_id: r.stage === "available" || r.stage === "partial" ? (title.library?.itemId ?? null) : null,
      download: r.download ? { progress: r.download.progress, state: r.download.state, eta: now() + r.download.etaIn, episodes: r.download.episodes } : null,
    };
  };

  // --- The routes ---

  // The profile signed in (the household's current one) and what its
  // token allows.
  const viewerOf = (scope) => {
    const id = fixture.household.find((p) => p.current)?.id ?? 1;
    const account = ACCOUNTS[id] ?? ACCOUNTS[2];
    const words = scope.split(" ");
    const approvals = words.includes("approvals");
    return { id, ...account, requests: words.includes("requests"), approvals, manage: account.admin && approvals };
  };

  const summaryOf = (viewer) => {
    const off = {
      enabled: fixture.requestsOn, access: viewer.access, movies_left: null, seasons_left: null, movies_limit: null, seasons_limit: null,
      movies_next_at: null, seasons_next_at: null, manage: false, pending: 0, failed: 0, unseen: 0, own: 0, changed_at: null, sign_in_again: null,
    };
    if (!fixture.requestsOn) return ok(off);
    if (!viewer.requests) return ok({ ...off, sign_in_again: viewer.access !== "off" ? "requests" : null });
    const shown = requests.filter((r) => sees(r, viewer));
    return ok({
      ...off,
      ...allowanceOf(viewer),
      movies_limit: viewer.moviesLimit,
      seasons_limit: viewer.seasonsLimit,
      manage: viewer.manage,
      pending: viewer.manage ? requests.filter((r) => r.status === "pending").length : 0,
      failed: viewer.manage ? requests.filter((r) => r.status === "failed" && r.retryAt === null).length : 0,
      unseen: requests.filter((r) => isUnseen(r, viewer)).length,
      own: requests.filter((r) => ownOrFollowed(r, viewer)).length,
      changed_at: shown.length > 0 ? Math.max(...shown.map((r) => r.updatedAt)) : null,
      sign_in_again: viewer.admin && !viewer.approvals ? "approvals" : null,
    });
  };

  // A page of titles: up to 20, page from 1 to 500.
  const pageOf = (titleKeys, query, viewer, keep = () => true) => {
    const asked = query.get("page") ?? "";
    const page = asked === "" ? 1 : /^\d+$/.test(asked) ? Number(asked) : NaN;
    if (!(page >= 1 && page <= 500)) return fail(400, "bad_request", "page is a number from 1 to 500");
    const shown = titleKeys.map((key) => titles.get(key)).filter((title) => title && keep(title));
    return ok({ page, total_pages: Math.ceil(shown.length / PAGE), results: shown.slice((page - 1) * PAGE, page * PAGE).map((title) => titleJson(title, viewer)) });
  };
  const listOf = (name, query, viewer) => {
    if (!LISTS.includes(name)) return fail(404, "not_found", `there's no list ${name}`);
    if (viewer.access === "off") return fail(403, "no_access", "Requests aren't open to this account");
    return pageOf(lists[name], query, viewer);
  };
  // Titles whose name has the words; missing=1 leaves out what a library
  // has.
  const searchOf = (query, viewer) => {
    if (viewer.access === "off") return fail(403, "no_access", "Requests aren't open to this account");
    const q = (query.get("q") ?? "").trim().toLowerCase();
    if (!q) return ok({ page: 1, total_pages: 0, results: [] });
    const missing = flag(query, "missing");
    return pageOf([...titles.keys()], query, viewer, (title) => title.title.toLowerCase().includes(q) && (!missing || statusOf(title, viewer) !== "library"));
  };
  const detailOf = (kind, tmdbId, viewer) => {
    const title = titles.get(`${kind}:${tmdbId}`);
    if (!title) return fail(404, "not_found", "TMDB doesn't know this title");
    if (viewer.access === "off") return fail(403, "no_access", "Requests aren't open to this account");
    const declined = declinedFor(title, viewer)[0];
    const seasonJson = (season) => ({
      ...season,
      in_library: haveOf(title, season.number),
      state: seasonState(title, season),
      declined: declinedFor(title, viewer).some((r) => r.seasons.includes(season.number)),
      request_id: activeFor(title).filter((r) => r.seasons.includes(season.number) && sees(r, viewer)).sort(newest)[0]?.id ?? null,
    });
    return ok({
      ...titleJson(title, viewer),
      imdb_id: null,
      original_title: null,
      tagline: title.tagline,
      genres: title.genres,
      release_date: title.release_date,
      runtime_minutes: title.runtime_minutes,
      certification: title.certification,
      network: title.network,
      show_status: title.show_status,
      anime: title.anime,
      cast: CAST.map(([name, character], i) => ({ name, character, photo_url: i === 4 ? null : `/api/discover/images/profile/c${title.tmdb_id}x${i}.jpg` })),
      seasons: title.kind === "show" ? title.seasons.map(seasonJson) : null,
      declined: declined ? { reason: declined.userId === viewer.id ? declined.reason : null, at: declined.decidedAt, again_at: declined.decidedAt + 30 * DAY } : null,
      restriction: null,
      watching: activeFor(title).some((r) => watches(r, viewer)),
      requests: requests.filter((r) => r.kind === kind && r.tmdbId === tmdbId && sees(r, viewer)).sort(newest).map((r) => view(r, viewer)),
      ...allowanceOf(viewer),
    });
  };
  const pictureOf = (kind, file, query) => {
    if (!PICTURE_KINDS.includes(kind) || !PICTURE_FILE.test(file)) return fail(404, "not_found", "no such picture");
    fixture.discoverPictures.push({ kind, file, w: query.get("w") });
    const key = `${kind}/${file}`;
    if (!pictures.has(key)) {
      const hash = [...file].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7);
      const [r, g, b] = PALETTE[hash % PALETTE.length];
      const wide = kind === "backdrop" || kind === "still";
      const width = wide ? 64 : 44;
      const height = wide ? 36 : 66;
      // A darker foot, so a poster reads as one.
      pictures.set(key, png(width, height, (x, y) => (y > height * 0.75 ? [r >> 1, g >> 1, b >> 1] : [r, g, b])));
    }
    return { status: 200, picture: pictures.get(key) };
  };
  // A library's title: its match on TMDB, its request and the seasons the
  // picker would tick.
  const itemRequest = (itemId, viewer) => {
    const title = [...titles.values()].find((t) => t.library?.itemId === itemId);
    if (!title) return fail(404, "not_found", "this title has no match on TMDB");
    const request = activeFor(title).filter((r) => sees(r, viewer)).sort(newest)[0];
    const declined = declinedFor(title, viewer).flatMap((r) => r.seasons);
    const askable = title.kind === "movie" ? [] : title.seasons.filter((s) => s.number > 0 && ["missing", "partial"].includes(seasonState(title, s)) && !declined.includes(s.number)).map((s) => s.number);
    return ok({ kind: title.kind, tmdb_id: title.tmdb_id, status: statusOf(title, viewer), request: request ? view(request, viewer) : null, askable });
  };

  const listRequests = (query, viewer) => {
    const number = (name, fallback) => {
      const value = query.get(name) ?? "";
      if (value === "") return fallback;
      return /^-?\d+$/.test(value) ? Number(value) : NaN;
    };
    const statuses = (query.get("status") ?? "").split(",").filter(Boolean);
    if (statuses.some((s) => !STATUSES.includes(s))) return fail(400, "bad_request", "status is pending, approved, declined, failed or available");
    const kind = query.get("kind") ?? "";
    if (kind !== "" && kind !== "movie" && kind !== "show") return fail(400, "bad_request", "kind is movie or show");
    const order = query.get("order") ?? "newest";
    if (order !== "newest" && order !== "oldest") return fail(400, "bad_request", "order is newest or oldest");
    const userId = number("user_id", 0);
    const offset = number("offset", 0);
    const limit = number("limit", 50);
    const since = number("changed_since", 0);
    if ([userId, offset, limit, since].some(Number.isNaN)) return fail(400, "bad_request", "user_id, offset, limit and changed_since are numbers");
    const q = (query.get("q") ?? "").trim().toLowerCase();
    const unseen = flag(query, "unseen");
    let base = requests.filter((r) => (flag(query, "mine") || !viewer.manage ? ownOrFollowed(r, viewer) : true));
    if (flag(query, "attention")) base = base.filter((r) => r.status === "pending" || (r.status === "failed" && r.retryAt === null));
    if (kind) base = base.filter((r) => r.kind === kind);
    if (q) base = base.filter((r) => titleOf(r).title.toLowerCase().includes(q));
    if (unseen) base = base.filter((r) => isUnseen(r, viewer));
    if (since > 0) base = base.filter((r) => r.updatedAt > since);
    const withStatus = (list) => (statuses.length > 0 ? list.filter((r) => statuses.includes(r.status)) : list);
    // The person filter's people: everyone matching but for user_id.
    const people = viewer.manage ? [...new Set(withStatus(base).map((r) => r.userId))].map((id) => USERS[id]).sort((a, b) => a.name.localeCompare(b.name)) : [];
    const byUser = viewer.manage && userId > 0 ? base.filter((r) => r.userId === userId) : base;
    const countsOf = Object.fromEntries(STATUSES.map((s) => [s, byUser.filter((r) => r.status === s).length]));
    const listed = withStatus(byUser);
    if (unseen) listed.sort((a, b) => b.newsAt - a.newsAt || b.id - a.id);
    else if (order === "oldest") listed.sort((a, b) => a.createdAt - b.createdAt || a.id - b.id);
    else listed.sort(newest);
    const from = Math.max(0, offset);
    const size = Math.min(200, Math.max(1, limit));
    return ok({ total: listed.length, counts: countsOf, requests: listed.slice(from, from + size).map((r) => view(r, viewer)), people });
  };

  // Asking (POST /api/requests): follows what others asked for already,
  // asks for the rest.
  const create = (body, viewer) => {
    if (viewer.access === "off") return fail(403, "no_access", "this account may not ask for movies and shows");
    let asked;
    try {
      asked = JSON.parse(body);
    } catch {
      return fail(400, "bad_request", "the body isn't JSON");
    }
    if (asked === null || typeof asked !== "object" || Array.isArray(asked)) return fail(400, "bad_request", "the body is an object");
    fixture.requestsAsked.push({ ...asked, profile: viewer.id });
    const { kind, tmdb_id: tmdbId, seasons: given } = asked;
    if (kind !== "movie" && kind !== "show") return fail(400, "bad_request", "kind is movie or show");
    if (!Number.isInteger(tmdbId)) return fail(400, "bad_request", "tmdb_id is a number");
    if (given != null && (!Array.isArray(given) || !given.every((n) => Number.isInteger(n) && n >= 0))) return fail(400, "bad_request", "seasons are numbers");
    const title = titles.get(`${kind}:${tmdbId}`);
    if (!title) return fail(404, "not_found", "TMDB doesn't know this title");
    if (kind === "movie" && given != null) return fail(400, "bad_request", "movies have no seasons");
    const seasonOf = (n) => title.seasons.find((s) => s.number === n);
    let wanted = [];
    if (kind === "show") {
      if (given != null) {
        const unknown = given.find((n) => !seasonOf(n));
        if (unknown !== undefined) return fail(400, "bad_request", `TMDB lists no season ${unknown} of ${title.title}`);
        wanted = [...new Set(given)].sort((a, b) => a - b);
        if (wanted.length === 0) return fail(400, "bad_request", "ask for at least one season");
      } else {
        const declined = declinedFor(title, viewer).flatMap((r) => r.seasons);
        wanted = title.seasons.filter((s) => s.number > 0 && s.aired > 0 && !seasonWhole(title, s) && !declined.includes(s.number)).map((s) => s.number);
        if (!title.seasons.some((s) => s.number > 0 && s.aired > 0)) wanted = title.seasons.filter((s) => s.number > 0).map((s) => s.number);
      }
      if (wanted.every((n) => seasonWhole(title, seasonOf(n)))) return fail(409, "in_library", "a library has these seasons whole already");
    } else if (title.library) {
      return fail(409, "in_library", "a library has this movie already");
    }
    const active = activeFor(title);
    const followed = [];
    const left = [];
    if (kind === "movie") {
      if (active.length > 0) followed.push(active[0]);
      else left.push(null);
    } else {
      for (const n of wanted) {
        if (seasonWhole(title, seasonOf(n))) continue;
        const covering = active.find((r) => r.seasons.includes(n));
        if (!covering) left.push(n);
        else if (!followed.includes(covering)) followed.push(covering);
      }
    }
    const t = now();
    const service = kind === "movie" ? "radarr" : "sonarr";
    for (const r of followed) {
      if (r.userId !== viewer.id && !watches(r, viewer)) {
        r.watchers.push({ id: viewer.id, at: t });
        r.updatedAt = t;
      }
      if (viewer.access === "auto" && r.status === "pending") Object.assign(r, { status: "approved", stage: "searching", sentTo: service, decidedBy: viewer.id, decidedAt: t, updatedAt: t });
    }
    if (left.length === 0) return ok(view(followed[0], viewer));
    const declined = declinedFor(title, viewer).filter((r) => kind === "movie" || r.seasons.some((n) => left.includes(n)));
    if (declined.length > 0) return fail(409, "declined", "this was declined in the last 30 days", { retry_at: declined[0].decidedAt + 30 * DAY });
    const allowance = allowanceOf(viewer);
    if (kind === "movie" && allowance.movies_left !== null && allowance.movies_left < 1) {
      return tooMany(allowance.movies_next_at, "this account has asked for all the movies it may for now");
    }
    if (kind === "show" && allowance.seasons_left !== null && left.length > allowance.seasons_left) {
      return tooMany(left.length > viewer.seasonsLimit ? null : freedAt(viewer, "show"), `this account may ask for ${allowance.seasons_left} more seasons for now`);
    }
    const auto = viewer.access === "auto";
    const r = made({
      id: nextId,
      userId: viewer.id,
      kind,
      tmdbId,
      seasons: kind === "show" ? left : [],
      status: auto ? "approved" : "pending",
      stage: auto ? "searching" : "pending",
      sentTo: auto ? service : null,
      decidedBy: auto ? viewer.id : null,
      decidedAt: auto ? t : null,
      createdAt: t,
    });
    nextId += 1;
    requests.push(r);
    return { status: 201, body: view(r, viewer) };
  };

  // Withdrawing (its maker) or deleting (a manager).
  const remove = (id, viewer) => {
    const r = find(id);
    if (!r || !sees(r, viewer)) return fail(404, "not_found", "no such request");
    if (viewer.manage) {
      drop(r);
      acted(id, "delete");
      return { status: 204 };
    }
    if (r.userId !== viewer.id) return conflict(r, viewer, "only the one who asked for it withdraws it: stop following it instead");
    if (r.status === "pending" || r.status === "failed") {
      acted(id, "withdraw");
      // Others follow it: the earliest of them asks for it now.
      const next = r.watchers.shift();
      if (next) {
        r.userId = next.id;
        r.updatedAt = now();
      } else {
        drop(r);
      }
      return { status: 204 };
    }
    if (r.status === "declined") {
      const again = r.decidedAt + 30 * DAY;
      if (now() < again) return fail(409, "conflict", "a declined request stays for 30 days", { retry_at: again, request: view(r, viewer) });
      acted(id, "withdraw");
      drop(r);
      return { status: 204 };
    }
    return conflict(r, viewer, "an approved request stays: an administrator can delete it");
  };

  const unwatch = (id, viewer) => {
    const r = find(id);
    if (!r || !sees(r, viewer)) return fail(404, "not_found", "no such request");
    r.watchers = r.watchers.filter((w) => w.id !== viewer.id);
    acted(id, "unwatch");
    return { status: 204 };
  };

  // Approve (and Try again), Decline, Mark as added by hand.
  const decide = (action, id, body, viewer) => {
    if (!viewer.manage) return fail(403, "forbidden", "only an administrator manages requests");
    const r = find(id);
    if (!r) return fail(404, "not_found", "no such request");
    let asked = {};
    if (body.trim()) {
      try {
        asked = JSON.parse(body);
      } catch {
        return fail(400, "bad_request", "the body isn't JSON");
      }
    }
    if (asked === null || typeof asked !== "object" || Array.isArray(asked)) return fail(400, "bad_request", "the body is an object");
    const t = now();
    const notSent = r.status === "approved" && r.sentTo === null;
    if (action === "approve") {
      if (r.status === "approved" && !notSent) return ok(view(r, viewer));
      if (!(r.status === "pending" || r.status === "failed" || r.status === "declined" || notSent)) return conflict(r, viewer, `this request is ${r.status}`);
      if (Array.isArray(asked.seasons) && r.kind === "show") {
        const kept = r.seasons.filter((n) => asked.seasons.includes(n));
        if (kept.length > 0) r.seasons = kept;
      }
      Object.assign(r, { status: "approved", stage: "approved", reason: null, note: null, retryAt: null, decidedBy: viewer.id, decidedAt: t, updatedAt: t });
      acted(id, "approve", asked);
      const answer = view(r, viewer);
      // Sent at once (in the background, on a real server): searching.
      Object.assign(r, { stage: "searching", sentTo: r.kind === "movie" ? "radarr" : "sonarr" });
      return ok(answer);
    }
    if (action === "decline") {
      if (!(r.status === "pending" || r.status === "failed" || r.status === "approved")) return conflict(r, viewer, `this request is ${r.status}`);
      const reason = typeof asked.reason === "string" && asked.reason.trim() ? asked.reason.trim() : null;
      if (reason && reason.length > 500) return fail(400, "bad_request", "a reason is 500 characters at most");
      Object.assign(r, { status: "declined", stage: "declined", reason, retryAt: null, download: null, decidedBy: viewer.id, decidedAt: t, updatedAt: t, newsAt: t });
      acted(id, "decline", asked);
      return ok(view(r, viewer));
    }
    if (!(r.status === "pending" || r.status === "failed" || r.status === "declined" || notSent)) return conflict(r, viewer, `this request is ${r.status}`);
    Object.assign(r, { status: "approved", stage: "approved", sentTo: "hand", note: "Added by hand", reason: null, retryAt: null, decidedBy: viewer.id, decidedAt: t, updatedAt: t });
    acted(id, "handled", asked);
    return ok(view(r, viewer));
  };

  const options = (viewer) => {
    if (!viewer.manage) return fail(403, "forbidden", "only an administrator manages requests");
    const service = (folders, anime) => ({
      quality_profiles: [{ id: 1, name: "HD-1080p" }, { id: 2, name: "Ultra-HD" }],
      root_folders: folders.map((path) => ({ path, free_bytes: 1200000000000 })),
      quality_profile: 1,
      root_folder: folders[0],
      anime_quality_profile: anime ? 1 : null,
      anime_root_folder: anime ? folders[1] : null,
      error: null,
    });
    return ok({ radarr: service(["/media/movies"], false), sonarr: service(["/media/shows", "/media/anime"], true) });
  };

  // [path, method, scope needed, answer]. The first whose path matches
  // and method is the request's answers; requests off answer 503 first,
  // a sign-in without the scope 403.
  const routes = [
    [/^\/api\/discover\/(movie|show)\/(\d+)$/, "GET", "requests", (m, viewer) => detailOf(m[1], Number(m[2]), viewer)],
    [/^\/api\/discover\/images\/([^/]+)\/([^/]+)$/, "GET", "requests", (m, viewer, query) => pictureOf(m[1], m[2], query)],
    [/^\/api\/discover\/search$/, "GET", "requests", (m, viewer, query) => searchOf(query, viewer)],
    [/^\/api\/discover\/([^/]+)$/, "GET", "requests", (m, viewer, query) => listOf(m[1], query, viewer)],
    [/^\/api\/items\/(\d+)\/request$/, "GET", "requests", (m, viewer) => itemRequest(Number(m[1]), viewer)],
    [/^\/api\/requests$/, "GET", "requests", (m, viewer, query) => listRequests(query, viewer)],
    [/^\/api\/requests$/, "POST", "requests", (m, viewer, query, body) => create(body, viewer)],
    [/^\/api\/requests\/seen$/, "POST", "requests", (m, viewer) => {
      seenAt.set(viewer.id, now());
      acted(null, "seen");
      return { status: 204 };
    }],
    [/^\/api\/requests\/options$/, "GET", "approvals", (m, viewer) => options(viewer)],
    [/^\/api\/requests\/(\d+)$/, "GET", "requests", (m, viewer) => {
      const r = find(Number(m[1]));
      return r && sees(r, viewer) ? ok(view(r, viewer)) : fail(404, "not_found", "no such request");
    }],
    [/^\/api\/requests\/(\d+)$/, "DELETE", "requests", (m, viewer) => remove(Number(m[1]), viewer)],
    [/^\/api\/requests\/(\d+)\/watch$/, "DELETE", "requests", (m, viewer) => unwatch(Number(m[1]), viewer)],
    [/^\/api\/requests\/(\d+)\/(approve|decline|handled)$/, "POST", "approvals", (m, viewer, query, body) => decide(m[2], Number(m[1]), body, viewer)],
  ];

  // The answer to a request for path, as { status, body, headers } or {
  // status, picture }; null where requests have nothing to say (an older
  // server, without `requestScopes`, knows none of these).
  const answer = (method, path, query, body, scope) => {
    if (!fixture.requestScopes) return null;
    const viewer = viewerOf(scope ?? "");
    if (path === "/api/requests/summary") return method === "GET" ? summaryOf(viewer) : fail(405, "method_not_allowed", `${method} isn't allowed here`);
    let known = false;
    for (const [pattern, verb, need, run] of routes) {
      const match = path.match(pattern);
      if (!match) continue;
      known = true;
      if (verb !== method) continue;
      if (!fixture.requestsOn) return fail(503, "unavailable", "requests are off: an administrator can turn them on", { setup: false });
      if (!(need === "requests" ? viewer.requests : viewer.approvals)) {
        return {
          status: 403,
          body: { code: "insufficient_scope", message: `this app's sign-in doesn't allow it: sign in again`, scope: need },
          headers: { "WWW-Authenticate": `Bearer error="insufficient_scope", scope="${need}"` },
        };
      }
      return run(match, viewer, query, body);
    }
    if (known) return fail(405, "method_not_allowed", `${method} isn't allowed here`);
    if (/^\/api\/requests\/[^/]+$/.test(path)) return fail(400, "bad_request", "a request's id is a number");
    if (path.startsWith("/api/requests/") || path.startsWith("/api/discover/")) return fail(404, "not_found", `nothing at ${path}`);
    return null;
  };

  // Time passing, for the walk: a movie request's download comes into a
  // library as item itemId, which its maker hasn't seen yet. No event
  // tells of it (the fake server has no stream).
  const arrive = (id, itemId) => {
    const r = find(id);
    const at = now();
    titleOf(r).library = { itemId };
    Object.assign(r, { status: "available", stage: "available", download: null, availableAt: at, updatedAt: at, newsAt: at });
  };
  return { answer, arrive };
}

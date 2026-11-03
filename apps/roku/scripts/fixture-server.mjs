// A fake pley server on 127.0.0.1 for scripts/smoke.mjs: answers written
// from docs/API.md, never a real server. It also checks what the app sends:
// the User-Agent on every request, the token in the Authorization header
// only (and not for a QR code), each refresh token used once and never two
// refreshes at once, and no token before the server proved it holds the
// sign-in's key (docs/API.md, "Telling servers apart"); a refresh token
// only after a proof made then. Requests and Discover are in
// fixture-requests.mjs.

import { createHash, createHmac } from "node:crypto";
import { createServer } from "node:http";
import { deflateSync } from "node:zlib";
import { fakeRequests } from "./fixture-requests.mjs";

const USER_AGENT = /^pleyRoku\/\d+\.\d+\.\d+ \(Roku OS [^;()]+; [^;()]+\)$/;
const CLIENT_ID = "https://pley-media-server.github.io/pley/oauth/roku.json";
const DEVICE_GRANT = "urn:ietf:params:oauth:grant-type:device_code";
const SCOPE = "library play playlists profiles";
// Asked for only where the metadata offers them (`requestScopes`).
const REQUEST_SCOPES = "requests approvals";
const SWITCH_GRANT = "urn:pley:params:oauth:grant-type:switch-profile";
// A new code for each sign-in.
const USER_CODES = ["BCDF-GHJK", "LMNP-QRST"];
// The server's id, and what its sign-ins' proof keys derive from (a real
// server's secret.key).
const SERVER_ID = "f1c7e5a0d2b94c6e8a3f7d1b5c9e2a4f6b8d0c13";
const PROOF_ROOT = createHash("sha256").update("pley fixture server").digest();
const b64url = (bytes) => bytes.toString("base64url");
const proofKey = (grant) => createHmac("sha256", PROOF_ROOT).update(`grant ${grant}`).digest();
// How soon after a proof a refresh token may come: "a proof made then".
const PROOF_THEN_MS = 10_000;

// A PNG of colorAt(x, y) ([r, g, b]) for every pixel.
function png(width, height, colorAt) {
  const crcTable = Array.from({ length: 256 }, (_, n) => {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    return c >>> 0;
  });
  const crc = (buf) => {
    let c = 0xffffffff;
    for (const byte of buf) c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
    return (c ^ 0xffffffff) >>> 0;
  };
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type), data]);
    const out = Buffer.alloc(body.length + 8);
    out.writeUInt32BE(data.length, 0);
    body.copy(out, 4);
    out.writeUInt32BE(crc(body), body.length + 4);
    return out;
  };
  const stride = 1 + width * 3;
  const raw = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) raw.set(colorAt(x, y), y * stride + 1 + x * 3);
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header.set([8, 2, 0, 0, 0], 8);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", header),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

const POSTER = png(44, 66, () => [0x2b, 0x6c, 0xb0]);
const STILL = png(64, 36, () => [0x6b, 0x4c, 0x9a]);

// Something that looks like the server's QR codes (black on white, 8
// pixels a module, a quiet zone of 4): three finder squares and modules
// made up from the text. The app only shows it.
function qrPicture(text) {
  const size = 25;
  let seed = [...text].reduce((hash, c) => (hash * 31 + c.charCodeAt(0)) >>> 0, 7);
  const modules = Array.from({ length: size * size }, () => {
    seed = (seed * 1103515245 + 12345) >>> 0;
    return (seed >>> 16) & 1;
  });
  const dark = (x, y) => {
    for (const [fx, fy] of [[0, 0], [size - 7, 0], [0, size - 7]]) {
      const dx = x - fx;
      const dy = y - fy;
      if (dx >= 0 && dx < 7 && dy >= 0 && dy < 7) return Math.max(Math.abs(dx - 3), Math.abs(dy - 3)) !== 2;
      if (dx >= -1 && dx <= 7 && dy >= -1 && dy <= 7) return false;
    }
    return modules[y * size + x] === 1;
  };
  const side = (size + 8) * 8;
  return png(side, side, (px, py) => {
    const x = Math.floor(px / 8) - 4;
    const y = Math.floor(py / 8) - 4;
    const inside = x >= 0 && y >= 0 && x < size && y < size;
    return inside && dark(x, y) ? [0, 0, 0] : [255, 255, 255];
  });
}

const state = (position_ms = 0, watched = false) => ({ position_ms, watched, play_count: watched ? 1 : 0 });

// The movies' first words, seven movies each, in the order of their
// titles: under "#" and most letters (one with an accent, under E), none
// under Q, X or Z. Three pages of the app's 70; W's first is on the third.
const WORDS = [
  "2001", "Amber", "Birch", "Cedar", "Dune", "Écho", "Falcon", "Granite", "Harbor", "Iron", "Juniper", "Kestrel", "Lantern",
  "Meadow", "North", "Orchid", "Pine", "Raven", "Silver", "Tide", "Umber", "Violet", "Willow", "Yarrow", "Yonder",
];
const movies = Array.from({ length: WORDS.length * 7 }, (_, i) => ({
  id: 1000 + i,
  library_id: 1,
  kind: "movie",
  title: `${WORDS[Math.floor(i / 7)]} Movie ${String(i + 1).padStart(3, "0")}`,
  sort_title: `${WORDS[Math.floor(i / 7)]} Movie ${String(i + 1).padStart(3, "0")}`,
  year: 1990 + (i % 30),
  duration_ms: 6000000 + i * 1000,
  added_at: 1700000000 + i,
  poster_url: `/api/items/${1000 + i}/images/poster?v=1`,
  backdrop_url: `/api/items/${1000 + i}/images/backdrop?v=1`,
  user_state: state(i % 5 === 0 ? 600000 : 0, i % 7 === 3),
}));
// What a library listing says of each title's best file (MediaSummary),
// whatever the sort: a 4K Dolby Vision, a 4K HDR10 and a 1080p SDR one in
// turn, so the grid has posters with quality badges and without.
const MOVIE_MEDIA = [
  { bitrate: 61000000, size: 66571993088, width: 3840, height: 2160, video_codec: "hevc", dynamic_range: "dolby_vision" },
  { bitrate: 24000000, size: 25125558681, width: 3840, height: 2160, video_codec: "hevc", dynamic_range: "hdr10" },
  { bitrate: 8000000, size: 6979321856, width: 1920, height: 1080, video_codec: "h264", dynamic_range: "sdr" },
];
const mediaOf = (item) =>
  item.kind === "movie" ? MOVIE_MEDIA[(item.id - 1000) % MOVIE_MEDIA.length]
  : item.kind === "show" ? { bitrate: 8000000, size: 18000000, width: 1920, height: 1080, video_codec: null, dynamic_range: "sdr" }
  : null;
// A listing's resolution and dynamic_range filters, by the title's best
// file as the server matches them.
const RESOLUTIONS = [
  ["4k", (w, h) => w >= 3800 || h >= 2100],
  ["1080", (w, h) => w >= 1800 || h >= 1000],
  ["720", (w, h) => w >= 1200 || h >= 700],
  ["sd", () => true],
];
const resolutionOf = (media) => RESOLUTIONS.find(([, fits]) => fits(media?.width ?? 0, media?.height ?? 0))[0];
function matchesQuality(item, resolution, range) {
  const media = mediaOf(item);
  if (resolution && resolutionOf(media) !== resolution) return false;
  if (range === "hdr") return !!media?.dynamic_range && media.dynamic_range !== "sdr";
  return !range || media?.dynamic_range === range;
}

const show = {
  id: 99, library_id: 2, kind: "show", title: "Severance", sort_title: "Severance", year: 2022,
  added_at: 1700000000, poster_url: "/api/items/99/images/poster?v=1", backdrop_url: "/api/items/99/images/backdrop?v=1",
  user_state: state(), leaf_count: 18, viewed_leaf_count: 2, child_count: 2,
};
const seasons = [1, 2].map((n) => ({
  id: n * 100, library_id: 2, kind: "season", title: `Season ${n}`, sort_title: `Season ${n}`, index: n,
  parent_id: 99, parent_title: "Severance", added_at: 1700000000, user_state: state(), leaf_count: 9, viewed_leaf_count: n === 1 ? 2 : 0,
}));
const episodes = seasons.flatMap((season) => Array.from({ length: 9 }, (_, i) => ({
  id: season.id + i + 1, library_id: 2, kind: "episode", title: `Episode ${i + 1}`, sort_title: `Episode ${i + 1}`,
  index: i + 1, parent_id: season.id, parent_title: season.title, parent_index: season.index,
  grandparent_id: 99, grandparent_title: "Severance", duration_ms: 3300000, added_at: 1700000000,
  thumb_url: `/api/items/${season.id + i + 1}/images/thumb?v=1`, summary: `What happens in episode ${i + 1}.`,
  user_state: season.index === 1 && i < 2 ? state(0, true) : season.index === 1 && i === 2 ? state(900000) : state(),
})));
// Music: an artist, their albums (the server lists the older first) and
// their tracks; the older album's last track is on its second disc
// (parent_index).
const artist = {
  id: 5000, library_id: 3, kind: "artist", title: "The Fixtures", sort_title: "Fixtures", added_at: 0, poster_url: "/api/items/5000/images/poster?v=1",
  user_state: state(), child_count: 2, leaf_count: 5,
};
const album = {
  id: 5100, library_id: 3, kind: "album", title: "Blue", sort_title: "Blue", year: 2001, parent_id: 5000, parent_title: "The Fixtures",
  added_at: 0, poster_url: "/api/items/5100/images/poster?v=1", user_state: state(), leaf_count: 3,
};
const newerAlbum = {
  id: 5101, library_id: 3, kind: "album", title: "Red", sort_title: "Red", year: 2005, parent_id: 5000, parent_title: "The Fixtures",
  added_at: 0, poster_url: "/api/items/5101/images/poster?v=1", user_state: state(), leaf_count: 2,
};
// An album's tracks from id first: each one's disc and its place on it.
const albumTracks = (of, first, places) => places.map(([disc, index], i) => ({
  id: first + i, library_id: 3, kind: "track", title: `Song ${i + 1}`, sort_title: `Song ${i + 1}`, index, parent_id: of.id, parent_title: of.title,
  parent_index: disc, grandparent_id: 5000, grandparent_title: "The Fixtures", duration_ms: 200000, added_at: 0, user_state: state(),
}));
const tracks = [...albumTracks(album, 5201, [[1, 1], [1, 2], [2, 1]]), ...albumTracks(newerAlbum, 5211, [[1, 1], [1, 2]])];
// Every movie's trailer, among its extras, which the title's Trailer plays.
const trailer = {
  id: 4000, library_id: 1, kind: "extra", title: "Official Trailer", sort_title: "Official Trailer", extra_type: "trailer", duration_ms: 150000,
  added_at: 0, thumb_url: "/api/items/4000/images/thumb?v=1", user_state: state(),
};
// The movie a ready-to-watch notice opens has a summary as long as its page
// shows, over six buttons: the page near its fullest.
movies[11].summary = "A linguist is called in when twelve ships come to rest above the earth, and she must find a way to speak with those inside before the nations watching them lose patience. As she learns their language, her memories start to come loose from time, and what she remembers of her daughter changes what she will choose to do.";
const items = new Map([...movies, show, ...seasons, ...episodes, artist, album, newerAlbum, ...tracks, trailer].map((item) => [item.id, item]));
// Collections: TMDB's of five movies, and a smart one a filter fills with
// the show.
const collections = [
  { id: 1, library_id: 1, title: "Fixture Collection", summary: "Five films of the fake server, in the order they came out.", automatic: true, sort: "release", smart_filter: null, poster_url: "", backdrop_url: "", created_at: 0, updated_at: 0, items: movies.slice(0, 5) },
  { id: 2, library_id: 2, title: "Fixture Dramas", summary: null, automatic: false, sort: "release", smart_filter: "type=2&genre=Drama&sort=year%3Adesc", poster_url: "", backdrop_url: "", created_at: 0, updated_at: 0, items: [show] },
];
const collectionOf = ({ items: held, ...collection }) => ({ ...collection, item_count: held.length });

// Every file has text and picture subtitles; the choices pick the picture
// ones for episode 103 (the one the walk plays), which the app asks the
// server to draw. A movie's file has French audio too. Tracks picked
// (PUT /api/items/{id}/streams) are the item's choices after, as a
// server keeps them; Automatic is English.
const PICTURE_SUBTITLES = 4;
function detail(item, myList, picks) {
  const file = {
    id: 7000 + item.id, path: `${item.title}.mkv`, size: 1000000, container: item.kind === "movie" ? "mkv" : "mp4",
    duration_ms: item.duration_ms ?? null, bitrate: 8000000, width: 1920, height: 1080, video_codec: "h264", audio_codec: "aac",
    streams: [
      { index: 0, kind: "video", codec: "h264" },
      { index: 1, kind: "audio", codec: "aac", language: "eng", channels: 2, default: true },
      ...(item.kind === "movie" ? [{ index: 2, kind: "audio", codec: "ac3", language: "fre", channels: 6 }] : []),
      { index: 3, kind: "subtitle", codec: "subrip", language: "eng", text_based: true },
      { index: PICTURE_SUBTITLES, kind: "subtitle", codec: "hdmv_pgs_subtitle", language: "eng", forced: false, text_based: false },
    ],
    edition: null, selected_audio: 1, selected_subtitle: item.id === 103 ? PICTURE_SUBTITLES : 3, audio_picked: false,
    chapters: [{ start_ms: 0, end_ms: 600000, title: "Opening" }, { start_ms: 600000, end_ms: item.duration_ms ?? 1200000, title: null }],
  };
  const pick = picks?.get(item.id);
  if (pick && (pick.file_id ?? file.id) === file.id) {
    if ("audio_index" in pick) {
      file.selected_audio = pick.audio_index ?? 1;
      file.audio_picked = pick.audio_index !== null;
    }
    file.selected_subtitle = pick.subtitle_index ?? null;
  }
  // A movie has a second version: its Director's Cut, in 4K.
  const cut = {
    id: 9000 + item.id, path: `${item.title} {edition-Director's Cut}.mkv`, size: 66571993088, container: "mkv",
    duration_ms: item.duration_ms ?? null, bitrate: 61000000, width: 3840, height: 2160, video_codec: "hevc", audio_codec: "truehd",
    streams: [
      { index: 0, kind: "video", codec: "hevc", width: 3840, height: 2160, frame_rate: 23.976, bit_depth: 10, pix_fmt: "yuv420p10le", profile: "Main 10", level: 153, dynamic_range: "dolby_vision", dv_profile: 8, dv_bl_compat: 1, dv_level: 6, color_primaries: "bt2020", color_transfer: "smpte2084", color_space: "bt2020nc" },
      { index: 1, kind: "audio", codec: "truehd", language: "eng", channels: 8, channel_layout: "7.1", sample_rate: 48000, profile: "Dolby TrueHD + Dolby Atmos", default: true },
      { index: 2, kind: "subtitle", codec: "subrip", language: "eng", title: "English SDH", text_based: true },
    ],
    edition: "Director's Cut", selected_audio: 1, selected_subtitle: null, audio_picked: false, chapters: [],
  };
  if (item.kind === "track") {
    file.container = "mp3";
    file.streams = [{ index: 0, kind: "audio", codec: "mp3", channels: 2 }];
    file.chapters = [];
  }
  const playable = item.kind === "movie" || item.kind === "episode" || item.kind === "track" || item.kind === "extra";
  return {
    ...item, summary: item.summary ?? `About ${item.title}.`, original_title: null, tagline: item.kind === "movie" ? "A tagline." : null,
    genres: ["Drama", "Mystery"], studio: null, content_rating: "PG-13", tmdb_id: null, imdb_id: null,
    files: playable ? (item.kind === "movie" ? [file, cut] : [file]) : [],
    markers: item.kind === "episode" ? [{ file_id: file.id, kind: "credits", start_ms: item.duration_ms - 60000, end_ms: item.duration_ms, source: "chapter" }] : [],
    extras: item.kind === "movie" ? [trailer] : [],
    cast: [{ id: 7, name: "Ada Lovelace", role: "actor", character: "Herself", photo_url: "/api/people/7/photo?v=1" }, { id: 8, name: "Alan Turing", role: "actor", character: "The Machine" }],
    crew: [{ id: 9, name: "Grace Hopper", role: "director" }, { id: 9, name: "Grace Hopper", role: "writer" }],
    in_watchlist: myList.includes(item.id),
  };
}

// accessSeconds: how long refreshed access tokens last (a TV code's last an
// hour). refuse: paths whose first request answers 401 invalid_token, as
// for a token the server dropped.
// The returned `down` makes the server stop answering, as when it's off;
// `qr` false makes it answer 404 for QR codes, as an older one does;
// `requestScopes` false makes it offer neither requests nor approvals,
// and know nothing of requests, as an older one; `requestsOn` false
// turns requests off (the summary says so, the rest answers 503);
// `twoStep` makes GET /api/server say it requires two-step sign-in, which
// it otherwise leaves out, as an older one does;
// `playbackDelayMs` holds back the answers to POST /api/playback.
// What the app asked for and did is kept: requestsAsked (POST
// /api/requests bodies), requestsActed ({ id, action }), and
// discoverPictures ({ kind, file, w }), and smartPlaylists (POST
// /api/playlists bodies with a smart_filter); requestArrives(id, itemId)
// brings a movie request's title into a library, as time would.
export function startFixture({ accessSeconds = 3600, refuse = [] } = {}) {
  const problems = [];
  const requests = [];
  const toRefuse = new Set(refuse);
  let issuer = "";
  let tokens = 0;
  let sessions = 0;
  let refreshing = 0;
  let devices = 0;
  const polled = new Map(); // device code -> polls
  const deviceScope = new Map(); // device code -> the scope asked for
  const access = new Map(); // access token -> its scope
  const refresh = new Map(); // refresh token -> "fresh" | "used"
  const refreshScope = new Map(); // refresh token -> its scope: a refresh keeps it
  let grants = 0;
  const grantOf = new Map(); // access or refresh token -> its sign-in: a refresh keeps its key
  const provedAt = new Map(); // sign-in -> when the server last proved it holds its key
  const progress = [];
  const closed = [];

  const issue = (seconds, scope, grant = ++grants) => {
    tokens += 1;
    const pair = {
      access_token: `pley_at_${tokens}`, token_type: "Bearer", expires_in: seconds, scope, refresh_token: `pley_rt_${tokens}`,
      server_id: SERVER_ID, proof_id: String(grant), proof_key: b64url(proofKey(grant)),
    };
    access.set(pair.access_token, scope);
    refresh.set(pair.refresh_token, "fresh");
    refreshScope.set(pair.refresh_token, scope);
    grantOf.set(pair.access_token, grant);
    grantOf.set(pair.refresh_token, grant);
    return pair;
  };
  // A refresh token goes only to a server that proved itself then.
  const provedThen = (token, what) => {
    const at = provedAt.get(grantOf.get(token));
    if (at === undefined || Date.now() - at > PROOF_THEN_MS) problems.push(`${what} without a proof made then`);
  };

  const server = createServer(async (req, res) => {
    if (fixture.down) {
      req.socket.destroy();
      return;
    }
    const url = new URL(req.url, issuer);
    const body = await new Promise((resolve) => {
      let text = "";
      req.on("data", (chunk) => (text += chunk));
      req.on("end", () => resolve(text));
    });
    const agent = req.headers["user-agent"] ?? "";
    const auth = req.headers.authorization ?? "";
    requests.push({ method: req.method, path: url.pathname + url.search, agent, auth: auth ? auth.slice(0, 20) : "", at: Date.now() });
    if (!USER_AGENT.test(agent)) problems.push(`${req.method} ${url.pathname}: User-Agent ${JSON.stringify(agent)}`);
    if (url.searchParams.has("token")) problems.push(`${req.method} ${url.pathname}: a token in the address`);

    const json = (status, value) => {
      res.writeHead(status, { "Content-Type": "application/json", "Cache-Control": "no-store" });
      res.end(value === undefined ? "" : JSON.stringify(value));
    };
    const oauthError = (error, description) => json(400, { error, error_description: description });
    const form = new URLSearchParams(body);

    // OAuth: metadata, device codes, tokens, revocation.
    if (url.pathname === "/.well-known/oauth-authorization-server") {
      return json(200, {
        issuer, authorization_endpoint: `${issuer}/oauth/authorize`, token_endpoint: `${issuer}/api/oauth/token`,
        revocation_endpoint: `${issuer}/api/oauth/revoke`, device_authorization_endpoint: `${issuer}/api/oauth/device`,
        response_types_supported: ["code"], grant_types_supported: ["authorization_code", "refresh_token", DEVICE_GRANT],
        code_challenge_methods_supported: ["S256"],
        scopes_supported: ["library", "play", "playlists", "profiles", ...(fixture.requestScopes ? REQUEST_SCOPES.split(" ") : [])],
      });
    }
    if (url.pathname === "/api/oauth/device" && req.method === "POST") {
      if (form.get("client_id") !== CLIENT_ID) problems.push(`device code for ${form.get("client_id")}`);
      // An unknown scope fails the whole sign-in on an older server.
      const expected = fixture.requestScopes ? `${SCOPE} ${REQUEST_SCOPES}` : SCOPE;
      if (form.get("scope") !== expected) problems.push(`device code scope ${form.get("scope")}`);
      fixture.scopesAsked.push(form.get("scope"));
      devices += 1;
      const userCode = USER_CODES[(devices - 1) % USER_CODES.length];
      polled.set(`dc-${devices}`, 0);
      deviceScope.set(`dc-${devices}`, form.get("scope") ?? SCOPE);
      return json(200, {
        device_code: `dc-${devices}`, user_code: userCode, verification_uri: `${issuer}/link`,
        verification_uri_complete: `${issuer}/link?code=${userCode.replace("-", "")}`, expires_in: 600, interval: 1,
      });
    }
    // Anyone may fetch it, so the app sends no token. Without `qr`, the
    // server is older than this and has none.
    if (url.pathname === "/api/oauth/device/qr" && req.method === "GET") {
      if (auth) problems.push("the QR code asked for with a token");
      if (!fixture.qr) return json(404, { code: "not_found", message: "nothing at /api/oauth/device/qr" });
      const code = (url.searchParams.get("code") ?? "").replace(/[-\s]/g, "").toUpperCase();
      if (!/^[BCDFGHJKLMNPQRSTVWXZ]{8}$/.test(code)) return json(400, { code: "bad_request", message: "that isn't a code" });
      res.writeHead(200, { "Content-Type": "image/png", "Cache-Control": "no-store" });
      return res.end(qrPicture(`${issuer}/link?code=${code}`));
    }
    if (url.pathname === "/api/oauth/token" && req.method === "POST") {
      if (form.get("client_id") !== CLIENT_ID) problems.push(`token request for ${form.get("client_id")}`);
      if (form.get("grant_type") === DEVICE_GRANT) {
        // Approved at the third poll, so the code stays up a few seconds.
        const code = form.get("device_code");
        if (!polled.has(code) || polled.get(code) >= 3) {
          problems.push(`a poll with device code ${code}, unknown or approved already`);
          return oauthError("invalid_grant", "unknown device code");
        }
        polled.set(code, polled.get(code) + 1);
        return polled.get(code) < 3 ? oauthError("authorization_pending", "nobody has answered yet") : json(200, issue(3600, deviceScope.get(code)));
      }
      if (form.get("grant_type") === "refresh_token") {
        const token = form.get("refresh_token");
        fixture.refreshes += 1;
        refreshing += 1;
        if (refreshing > 1) problems.push("two refreshes at once");
        await new Promise((resolve) => setTimeout(resolve, 150));
        refreshing -= 1;
        if (refresh.get(token) !== "fresh") {
          problems.push(`refresh token ${token} used again`);
          return oauthError("invalid_grant", "refresh token used again: the app's access is ended");
        }
        provedThen(token, "a refresh");
        refresh.set(token, "used");
        return json(200, issue(accessSeconds, refreshScope.get(token), grantOf.get(token)));
      }
      // Another profile of the household: its tokens, the old ones stopped
      // at once (docs/API.md). Never beside a refresh: both spend the
      // refresh token.
      if (form.get("grant_type") === SWITCH_GRANT) {
        const token = form.get("refresh_token");
        refreshing += 1;
        if (refreshing > 1) problems.push("a profile switch beside a refresh");
        await new Promise((resolve) => setTimeout(resolve, 150));
        refreshing -= 1;
        if (refresh.get(token) !== "fresh") {
          problems.push(`refresh token ${token} used again`);
          return oauthError("invalid_grant", "refresh token used again: the app's access is ended");
        }
        const profile = fixture.household.find((p) => String(p.id) === form.get("profile"));
        if (!profile) return json(400, { error: "invalid_grant", reason: "not_allowed", error_description: "not a profile of the household" });
        provedThen(token, "a profile switch");
        refresh.set(token, "used");
        access.clear();
        fixture.switched.push(profile.id);
        fixture.household = fixture.household.map((p) => ({ ...p, current: p.id === profile.id }));
        return json(200, issue(accessSeconds, refreshScope.get(token), grantOf.get(token)));
      }
      return oauthError("unsupported_grant_type", "");
    }
    if (url.pathname === "/api/oauth/revoke" && req.method === "POST") {
      const token = form.get("token");
      if (grantOf.has(token)) provedThen(token, "a revocation");
      return json(200);
    }

    // Everything else needs the access token, in the header, but a keyed
    // session's media, which its key opens.
    if (!url.pathname.startsWith("/api/") || url.pathname === "/api/server") {
      const server = { id: SERVER_ID, name: "Fixture", version: "0.1.0", setup_required: false, setup_code_required: false, ...(fixture.twoStep ? { two_step_required: true } : {}) };
      const challenge = url.searchParams.get("challenge");
      const grant = Number(url.searchParams.get("proof_id"));
      if (url.pathname !== "/api/server" || challenge === null) return json(200, server);
      if (auth) problems.push("a proof asked for with a token");
      if (!/^[A-Za-z0-9_-]{22,86}$/.test(challenge) || !(grant >= 1 && grant <= grants)) return json(400, { code: "bad_request", message: "challenge and proof_id" });
      fixture.proofs += 1;
      provedAt.set(grant, Date.now());
      const proof = b64url(createHmac("sha256", proofKey(grant)).update(["pley-server-proof", SERVER_ID, issuer, challenge].join("\n")).digest());
      return json(200, { ...server, proof, address: issuer });
    }
    const bearer = auth.startsWith("Bearer ") ? auth.slice(7) : "";
    if (bearer && access.has(bearer) && !provedAt.has(grantOf.get(bearer))) problems.push(`${url.pathname}: the token before the server proved itself`);
    if (toRefuse.delete(url.pathname)) access.delete(bearer);
    if (!access.has(bearer) && !url.pathname.startsWith("/api/media/pley_mk_")) {
      res.writeHead(401, { "Content-Type": "application/json", "WWW-Authenticate": 'Bearer error="invalid_token"' });
      return res.end(JSON.stringify({ code: "unauthorized", message: "sign in again" }));
    }

    const path = url.pathname;
    let match;
    // Requests and Discover, as the profile signed in sees them.
    const answered = requestsApi.answer(req.method, path, url.searchParams, body, access.get(bearer));
    if (answered) {
      if (answered.picture) {
        res.writeHead(200, { "Content-Type": "image/png", "Cache-Control": "private, max-age=604800, immutable" });
        return res.end(answered.picture);
      }
      if (answered.status === 204) {
        res.writeHead(204);
        return res.end();
      }
      res.writeHead(answered.status, { "Content-Type": "application/json", "Cache-Control": "no-store", ...answered.headers });
      return res.end(JSON.stringify(answered.body));
    }
    const me = () => ({ id: 1, username: "anna", name: "Anna", ...fixture.trackDefaults });
    if (path === "/api/auth/me") return json(200, me());
    // The profile's track defaults (docs/API.md): all three at once.
    if (path === "/api/me/track-defaults" && req.method === "PUT") {
      const asked = JSON.parse(body);
      fixture.trackDefaultsSaved.push(asked);
      const keys = Object.keys(asked).sort().join();
      if (keys !== "audio_language,subtitle_language,subtitle_mode") problems.push(`track defaults with ${keys}`);
      if (!["off", "forced", "foreign", "always"].includes(asked.subtitle_mode)) return json(400, { code: "bad_request", message: "no such mode" });
      fixture.trackDefaults = { audio_language: asked.audio_language ?? null, subtitle_language: asked.subtitle_language ?? null, subtitle_mode: asked.subtitle_mode };
      return json(200, me());
    }
    // Find subtitles…: OpenSubtitles' results for the file, and the one
    // picked added as an ordinary text track.
    if ((match = path.match(/^\/api\/items\/(\d+)\/subtitles\/search$/)) && req.method === "GET") {
      if (!url.searchParams.get("file_id")) problems.push("a subtitle search without the file playing");
      return json(200, { results: [
        { id: "opensubtitles:1", provider: "opensubtitles", language: "en", name: "Heat.1995.1080p.BluRay.x264", format: "srt", hearing_impaired: false, forced: false, downloads: 1234, matched: "hash" },
        { id: "opensubtitles:2", provider: "opensubtitles", language: "en", name: "Heat.1995.WEB", format: "srt", hearing_impaired: true, forced: false, downloads: 12, matched: "title" },
      ] });
    }
    if ((match = path.match(/^\/api\/items\/(\d+)\/subtitles$/)) && req.method === "POST") {
      const asked = JSON.parse(body);
      fixture.subtitlesAdded.push({ item: Number(match[1]), ...asked });
      const fileId = asked.file_id ?? 7000 + Number(match[1]);
      res.writeHead(201, { "Content-Type": "application/json" });
      return res.end(JSON.stringify({ index: 10001, codec: "subrip", language: "eng", title: null, default: false, forced: false, external: true, text_based: true, added: true, url: `/api/files/${fileId}/subtitles/10001.vtt` }));
    }
    if ((match = path.match(/^\/api\/items\/(\d+)\/streams$/)) && req.method === "PUT") {
      const asked = JSON.parse(body);
      fixture.streams.push({ item: Number(match[1]), ...asked });
      // The audio left out stays as picked before; subtitles left out are off.
      const before = fixture.picks.get(Number(match[1])) ?? {};
      fixture.picks.set(Number(match[1]), { ...before, ...asked, subtitle_index: asked.subtitle_index ?? null });
      res.writeHead(204);
      return res.end();
    }
    const libraries = [
      { id: 1, name: "Movies", kind: "movies", paths: [], item_count: movies.length, created_at: 0, scanned_at: 0, scanning: false },
      { id: 2, name: "TV Shows", kind: "shows", paths: [], item_count: 1, created_at: 0, scanned_at: 0, scanning: false },
      { id: 3, name: "Music", kind: "music", paths: [], item_count: 12, created_at: 0, scanned_at: 0, scanning: false },
    ];
    // The profile's own order (docs/API.md): every library, once.
    const ordered = () => fixture.libraryOrder.map((id) => libraries.find((l) => l.id === id));
    if (path === "/api/libraries") return json(200, ordered());
    if (path === "/api/me/libraries/order" && req.method === "PUT") {
      const ids = JSON.parse(body).ids;
      fixture.orders.push(ids);
      if (!Array.isArray(ids) || [...ids].sort().join() !== "1,2,3") return json(409, { code: "conflict", message: "these aren't your libraries" });
      fixture.libraryOrder = ids;
      return json(200, ordered());
    }
    if (path === "/api/me/libraries/order" && req.method === "DELETE") {
      fixture.orders.push(null);
      fixture.libraryOrder = [1, 2, 3];
      return json(200, ordered());
    }
    if (path === "/api/hubs/home") {
      return json(200, {
        hubs: [
          { key: "continue_watching", title: "Continue Watching", library_id: null, items: [items.get(103), items.get(1000), items.get(1005)] },
          ...(fixture.myList.length ? [{ key: "my_list", title: "My List", library_id: null, items: fixture.myList.map((id) => items.get(id)).filter(Boolean) }] : []),
          { key: "recently_added:1", title: "Recently Added in Movies", library_id: 1, items: movies.slice(0, 20) },
          { key: "recently_added:2", title: "Recently Added in TV Shows", library_id: 2, items: [show] },
          { key: "recently_added:3", title: "Recently Added in Music", library_id: 3, items: [album] },
        ],
      });
    }
    // The household: two profiles until the walk switched, then one, so the
    // second start opens on Home, as the walk times it.
    if (path === "/api/oauth/profiles") return json(200, fixture.switched.length ? fixture.household.filter((p) => p.current) : fixture.household);
    // Playlists: the profile's one, its entries, taking one out, and those
    // made since, empty, of the kind asked for.
    const playlist = () => ({
      id: 1, title: "Road trip", kind: "video", item_count: fixture.playlistEntries.length, duration_ms: 9300000,
      artwork: ["/api/items/1000/images/poster?v=1"], smart_filter: null, library_id: null, created_at: 0, updated_at: 0,
    });
    // Smart playlists made from a library's filters (Save as…); they play
    // the library's titles. Those made here, smart or not, are numbered on
    // from 2.
    const smart = (made) => ({
      id: made.id, title: made.title, kind: "video", item_count: 3, duration_ms: 18000000, artwork: [],
      smart_filter: made.smart_filter, library_id: made.library_id, created_at: 0, updated_at: 0,
      entries: movies.slice(0, 3).map((item) => ({ entry_id: item.id, item })),
    });
    if (path === "/api/playlists" && req.method === "GET") {
      const listed = [playlist(), ...fixture.playlistsMade, ...fixture.smartPlaylists.filter((made) => made.id).map((made) => ({ ...smart(made), entries: undefined }))];
      return json(200, { playlists: listed.sort((a, b) => a.title.localeCompare(b.title)) });
    }
    if (path === "/api/playlists" && req.method === "POST" && body && "smart_filter" in JSON.parse(body)) {
      const made = JSON.parse(body);
      fixture.smartPlaylists.push(made);
      if (typeof made.title !== "string" || !made.title.trim() || !made.library_id || !made.smart_filter) return json(400, { code: "bad_request", message: "a smart playlist needs a title, a library and a filter" });
      made.id = fixture.nextPlaylist++;
      return json(201, smart(made));
    }
    if (path === "/api/playlists" && req.method === "POST") {
      const asked = JSON.parse(body);
      fixture.playlistsAsked.push(asked);
      if (typeof asked.title !== "string" || !asked.title.trim()) return json(400, { code: "bad_request", message: "A playlist needs a title." });
      if (asked.kind !== undefined && asked.kind !== "video" && asked.kind !== "audio") return json(400, { code: "bad_request", message: "kind is video or audio" });
      const made = {
        id: fixture.nextPlaylist++, title: asked.title.trim(), kind: asked.kind ?? "video", item_count: 0, duration_ms: 0,
        artwork: [], smart_filter: null, library_id: null, created_at: 0, updated_at: 0,
      };
      fixture.playlistsMade.push(made);
      return json(201, { ...made, entries: [] });
    }
    if (path === "/api/playlists/1" && req.method === "GET") {
      return json(200, { ...playlist(), entries: fixture.playlistEntries.map(([entry_id, id]) => ({ entry_id, item: items.get(id) })) });
    }
    if ((match = path.match(/^\/api\/playlists\/(\d+)$/)) && req.method === "GET" && fixture.smartPlaylists.some((made) => made.id === Number(match[1]))) {
      return json(200, smart(fixture.smartPlaylists.find((made) => made.id === Number(match[1]))));
    }
    if (path === "/api/playlists/1/order" && req.method === "PUT") {
      const ids = JSON.parse(body).entry_ids;
      fixture.playlistOrders.push(ids);
      fixture.playlistEntries = ids.map((id) => fixture.playlistEntries.find(([entry]) => entry === id)).filter(Boolean);
      return json(200, { ...playlist(), entries: fixture.playlistEntries.map(([entry_id, id]) => ({ entry_id, item: items.get(id) })) });
    }
    if ((match = path.match(/^\/api\/playlists\/(\d+)$/)) && req.method === "GET") {
      const made = fixture.playlistsMade.find((p) => p.id === Number(match[1]));
      return made ? json(200, { ...made, entries: [] }) : json(404, { code: "not_found", message: `playlist ${match[1]} not found` });
    }
    if ((match = path.match(/^\/api\/playlists\/1\/items\/(\d+)$/)) && req.method === "DELETE") {
      fixture.playlistRemoved.push(Number(match[1]));
      fixture.playlistEntries = fixture.playlistEntries.filter(([entry]) => entry !== Number(match[1]));
      res.writeHead(204);
      return res.end();
    }
    // The watch history: one play.
    if (path === "/api/history" && req.method === "GET") {
      return json(200, { total: 1, entries: [{ id: 1, viewed_at: Math.floor(Date.now() / 1000) - 7200, device: "Roku Ultra", user: { id: 1, name: "Anna" }, item: items.get(101) }] });
    }
    // A library's tabs: its rows, its collections, and what it filters by.
    if ((match = path.match(/^\/api\/libraries\/(\d+)\/hubs$/))) {
      const listed = match[1] === "1" ? movies.slice(0, 20) : match[1] === "2" ? [show] : [];
      return json(200, { hubs: listed.length ? [{ key: "recently_added", title: "Recently Added", library_id: Number(match[1]), items: listed }, { key: "top_rated", title: "Top Rated", library_id: Number(match[1]), items: listed }] : [] });
    }
    if ((match = path.match(/^\/api\/libraries\/(\d+)\/collections$/))) {
      return json(200, { collections: collections.filter((c) => c.library_id === Number(match[1])).map(collectionOf) });
    }
    // Resolutions and dynamic ranges only where the titles have two or
    // more, as the server lists them: the movies differ, the one show and
    // music don't.
    if ((match = path.match(/^\/api\/libraries\/(\d+)\/filters$/))) {
      const quality = match[1] === "1" ? { resolutions: ["4k", "1080"], dynamic_ranges: ["hdr", "dolby_vision", "hdr10", "sdr"] } : { resolutions: [], dynamic_ranges: [] };
      return json(200, { genres: ["Drama", "Mystery"], years: [2022, 2019], ...quality });
    }
    if ((match = path.match(/^\/api\/libraries\/(\d+)\/items$/))) {
      let listed = match[1] === "1" ? movies : match[1] === "2" ? [show] : [artist];
      const resolution = url.searchParams.get("resolution") ?? "";
      const range = url.searchParams.get("dynamic_range") ?? "";
      const known = (resolution === "" || RESOLUTIONS.some(([token]) => token === resolution)) && (range === "" || ["hdr", "dolby_vision", "hdr10plus", "hdr10", "hlg", "sdr"].includes(range));
      if (!known) {
        problems.push(`a library listed by resolution ${JSON.stringify(resolution)} and dynamic range ${JSON.stringify(range)}`);
        return json(400, { code: "bad_request", message: "unknown resolution or dynamic range" });
      }
      listed = listed.filter((item) => matchesQuality(item, resolution, range));
      // By title, as the server sorts (accents with their letter); other
      // sorts as listed.
      if ((url.searchParams.get("sort") ?? "title") === "title") {
        listed = [...listed].sort((a, b) => a.sort_title.localeCompare(b.sort_title, "en", { sensitivity: "base" }));
        if (url.searchParams.get("order") === "desc") listed.reverse();
      }
      const offset = Number(url.searchParams.get("offset") ?? 0);
      const limit = Number(url.searchParams.get("limit") ?? 50);
      return json(200, { total: listed.length, items: listed.slice(offset, offset + limit).map((item) => ({ ...item, media: mediaOf(item) })) });
    }
    if ((match = path.match(/^\/api\/items\/(\d+)\/images\/(\w+)$/))) {
      res.writeHead(200, { "Content-Type": "image/png" });
      return res.end(match[2] === "poster" ? POSTER : STILL);
    }
    if ((match = path.match(/^\/api\/items\/(\d+)\/children$/))) {
      const id = Number(match[1]);
      return json(200, { items: [...items.values()].filter((item) => item.parent_id === id) });
    }
    if ((match = path.match(/^\/api\/items\/(\d+)\/on-deck$/))) return json(200, { item: items.get(103) });
    if ((match = path.match(/^\/api\/items\/(\d+)\/next$/))) {
      const id = Number(match[1]);
      return json(200, { item: items.get(id + 1)?.kind === "episode" ? items.get(id + 1) : null });
    }
    // A title's page: More Like This, its collections; people and collections.
    if ((match = path.match(/^\/api\/items\/(\d+)\/related$/))) return json(200, { items: movies.slice(10, 20) });
    if ((match = path.match(/^\/api\/items\/(\d+)\/collections$/))) return json(200, { collections: [{ id: 1, title: "Fixture Collection" }] });
    if ((match = path.match(/^\/api\/people\/(\d+)$/))) {
      return json(200, { id: Number(match[1]), name: "Ada Lovelace", photo_url: `/api/people/${match[1]}/photo?v=1`, credits: [{ item: show, roles: [{ role: "actor", character: "Herself", episodes: 9 }] }, ...movies.slice(0, 3).map((item) => ({ item, roles: [{ role: "director" }] }))] });
    }
    if ((match = path.match(/^\/api\/collections\/(\d+)$/))) {
      const collection = collections.find((c) => c.id === Number(match[1]));
      if (!collection) return json(404, { code: "not_found", message: `collection ${match[1]} not found` });
      return json(200, { ...collectionOf(collection), items: collection.items });
    }
    if (/^\/api\/(people\/\d+\/photo|collections\/\d+\/images\/\w+)$/.test(path)) {
      res.writeHead(200, { "Content-Type": "image/png" });
      return res.end(POSTER);
    }
    // Marks, My List and Continue Watching (the Options key, a title's buttons).
    if ((match = path.match(/^\/api\/items\/(\d+)\/(watched|unwatched)$/)) && req.method === "POST") {
      fixture.marks.push({ item: Number(match[1]), action: match[2] });
      res.writeHead(204);
      return res.end();
    }
    if ((match = path.match(/^\/api\/watchlist\/(\d+)$/)) && (req.method === "PUT" || req.method === "DELETE")) {
      const id = Number(match[1]);
      fixture.myList = fixture.myList.filter((other) => other !== id);
      if (req.method === "PUT") fixture.myList.unshift(id);
      fixture.marks.push({ item: id, action: req.method === "PUT" ? "listAdd" : "listRemove" });
      res.writeHead(204);
      return res.end();
    }
    // The profile's rating: 0 to 10, or null for none.
    if ((match = path.match(/^\/api\/items\/(\d+)\/rating$/)) && req.method === "PUT") {
      const { rating } = JSON.parse(body);
      if (rating !== null && !(rating >= 0 && rating <= 10)) return json(400, { code: "bad_request", message: "a rating is 0 to 10" });
      fixture.ratings.push({ item: Number(match[1]), rating });
      res.writeHead(204);
      return res.end();
    }
    if ((match = path.match(/^\/api\/continue-watching\/(\d+)$/)) && req.method === "DELETE") {
      fixture.marks.push({ item: Number(match[1]), action: "hide" });
      res.writeHead(204);
      return res.end();
    }
    if ((match = path.match(/^\/api\/items\/(\d+)\/progress$/)) && req.method === "POST") {
      progress.push({ item: Number(match[1]), ...JSON.parse(body) });
      res.writeHead(204);
      return res.end();
    }
    if ((match = path.match(/^\/api\/items\/(\d+)$/))) {
      const item = items.get(Number(match[1]));
      return item ? json(200, detail(item, fixture.myList, fixture.picks)) : json(404, { code: "not_found", message: `item ${match[1]} not found` });
    }
    if (path === "/api/playback" && req.method === "POST") {
      const ask = JSON.parse(body);
      if (ask.caps?.caps_version !== 2) problems.push("a playback request without caps_version 2");
      if (!ask.caps?.hls?.master_playlist) problems.push("a playback request without hls.master_playlist");
      const item = items.get(ask.item_id);
      // A movie's Director's Cut is its second file.
      const files = item?.kind === "movie" ? [7000 + ask.item_id, 9000 + ask.item_id] : [7000 + ask.item_id];
      if (ask.file_id != null && !files.includes(ask.file_id)) problems.push(`playback of file ${ask.file_id} for item ${ask.item_id}`);
      const fileId = ask.file_id ?? files[0];
      // As the server decides: PGS the caps list is the player's to show,
      // on a file it plays as it is, and isn't burned in.
      const drawsPgs = (ask.caps?.subtitle_codecs ?? []).includes("hdmv_pgs_subtitle");
      if (ask.subtitle_index === PICTURE_SUBTITLES && drawsPgs) problems.push("PGS to burn in, with caps saying the app shows it: the server would burn nothing in");
      const burn = drawsPgs && ask.subtitle_index === PICTURE_SUBTITLES ? null : (ask.subtitle_index ?? null);
      if (burn !== null && burn !== PICTURE_SUBTITLES) problems.push(`subtitles ${burn} to draw, which aren't pictures`);
      fixture.playbacks.push({ item: ask.item_id, kind: item?.kind, file_id: fileId, start_ms: ask.start_ms ?? 0, subtitle_index: burn, mode: ask.mode, max_bitrate: ask.max_bitrate ?? null, keyed: ask.keyed_urls === true, draws_pgs: drawsPgs });
      sessions += 1;
      const session = `s${sessions}`;
      // Keyed addresses carry the session's key, beside which are its seek
      // previews and its picture subtitles (docs/API.md, "Keyed URLs").
      const media = ask.keyed_urls === true ? `/api/media/pley_mk_${session}` : null;
      if (fixture.playbackDelayMs > 0) await new Promise((resolve) => setTimeout(resolve, fixture.playbackDelayMs));
      // Drawn subtitles make a conversion.
      return json(200, {
        session_id: session, method: burn === null ? "direct" : "transcode",
        url: media ? `${media}/${burn === null ? "file.mkv" : "master.m3u8"}` : burn === null ? `/api/files/${fileId}/stream` : `/api/playback/${session}/master.m3u8`,
        hls: burn !== null, file_id: fileId,
        duration_ms: item?.duration_ms ?? 0, audio_index: 1, subtitle_index: burn, burn_subtitles: burn !== null, audio_converted: false,
        subtitles: [{ index: 3, codec: "subrip", language: "eng", title: "English", default: false, forced: false, external: false, text_based: true, url: `/api/files/${fileId}/subtitles/3.vtt` }],
        reasons: [], reason: null, auto_quality: null,
      });
    }
    // What the server knows of this Roku's network: a speed test asked for
    // until one is told, then no limit (docs/API.md).
    if (path === "/api/playback/network") {
      if (req.method === "PUT") {
        const told = JSON.parse(body);
        fixture.networkTold.push(told);
        if (told.probe_kbps != null) fixture.probed = true;
      } else if (!url.searchParams.get("client_id")) {
        return json(400, { code: "bad_request", message: "client_id is missing" });
      }
      return json(200, { local: false, ceiling_kbps: null, source: fixture.probed ? "probe" : null, since: null, probe: !fixture.probed });
    }
    if (path === "/api/playback/speedtest") {
      const size = Math.min(Math.max(Number(url.searchParams.get("bytes") ?? 33554432), 65536), 67108864);
      fixture.speedTests.push(size);
      res.writeHead(200, { "Content-Type": "application/octet-stream", "Content-Length": size, "Cache-Control": "no-store" });
      return res.end(Buffer.alloc(size, 7));
    }
    if ((match = path.match(/^\/api\/playback\/([\w-]+)$/))) {
      if (req.method === "DELETE") {
        closed.push(match[1]);
        res.writeHead(204);
        return res.end();
      }
      const playback = fixture.playbacks[Number(match[1].slice(1)) - 1];
      const converted = playback?.subtitle_index != null;
      return json(200, {
        session_id: match[1], method: converted ? "transcode" : "direct",
        transcode: converted ? { job_id: "j1", state: "running", video: "transcode", audio: "copy", video_in: "H.264 1920×1080", video_out: "H.264 1920×1080", audio_in: "AAC 2.0", audio_out: null, encoder: "libx264", preset: "veryfast", decoder: "cpu", pipeline: "cpu", tonemap: null, dolby_vision: null, hdr10plus_stripped: false, subtitle_burn: true, speed: 2.5, fps: 60, transcoded_ms: 960000, segments_ready: 10, segments_total: 500, bitrate_out: 8000000, last_error: null } : null,
      });
    }
    // Live TV (docs/API.md, "Live TV"): three channels, one with no
    // guide; the source has one connection, which channel 3 never gets.
    if (path === "/api/live") return json(200, { available: true, allowed: true, channels: liveChannels().length, sources: 0, recent: fixture.liveRecent });
    if (path === "/api/live/channels") {
      problems.push("every channel asked for: the app asks a page at a time");
      return json(200, liveChannels());
    }
    if (path === "/api/live/groups") {
      const channels = liveChannels();
      const groups = [...new Set(channels.map((c) => c.group))].map((name) => {
        const [region, label] = name.includes("| ") ? name.split("| ") : [null, name];
        return { name, region, label, count: channels.filter((c) => c.group === name).length };
      });
      return json(200, { total: channels.length, favorites: fixture.liveFavorites.length, recent: fixture.liveRecent.length, groups });
    }
    if (path === "/api/live/browse") {
      const category = url.searchParams.get("category") ?? "all";
      const group = url.searchParams.get("group");
      const limit = Math.min(500, Number(url.searchParams.get("limit") ?? 100));
      const around = Number(url.searchParams.get("around") ?? 0);
      fixture.liveBrowses.push({ category, group, offset: url.searchParams.get("offset"), limit, around });
      let list = liveChannels();
      if (category === "favorites") list = list.filter((c) => c.favorite);
      if (category === "recent") list = fixture.liveRecent.map((id) => list.find((c) => c.id === id)).filter(Boolean);
      if (category === "group") list = list.filter((c) => c.group === group);
      const q = (url.searchParams.get("q") ?? "").toLowerCase();
      if (q) list = list.filter((c) => c.name.toLowerCase().includes(q) || (c.number ?? "").startsWith(q) || (c.now?.title ?? "").toLowerCase().includes(q));
      let offset = Number(url.searchParams.get("offset") ?? 0);
      if (around) {
        const at = list.findIndex((c) => c.id === around);
        offset = at < 0 ? 0 : Math.floor(at / limit) * limit;
      }
      return json(200, { total: list.length, offset, channels: list.slice(offset, offset + limit) });
    }
    if (path === "/api/live/guide") {
      const from = Number(url.searchParams.get("from"));
      const to = Number(url.searchParams.get("to"));
      if (!(to > from)) problems.push(`a guide from ${from} to ${to}`);
      const ids = (url.searchParams.get("ids") ?? "").split(",").filter(Boolean).map(Number);
      if (ids.length === 0) problems.push("a guide of every channel: the app asks for the channels in view");
      fixture.liveGuides.push({ from, to, ids });
      const programs = {};
      for (const channel of liveChannels()) {
        if (!channel.has_guide || !ids.includes(channel.id)) continue;
        programs[channel.id] = [];
        for (let start = Math.floor(from / 1800) * 1800; start < to; start += 1800) programs[channel.id].push(liveProgram(channel.id, start));
      }
      return json(200, { from, to, programs });
    }
    if ((match = path.match(/^\/api\/live\/channels\/(\d+)\/favorite$/)) && (req.method === "PUT" || req.method === "DELETE")) {
      const id = Number(match[1]);
      fixture.liveFavorites = fixture.liveFavorites.filter((f) => f !== id);
      if (req.method === "PUT") fixture.liveFavorites.push(id);
      fixture.liveFavoriteChanges.push({ id, favorite: req.method === "PUT" });
      res.writeHead(204);
      return res.end();
    }
    if ((match = path.match(/^\/api\/live\/channels\/(\d+)\/logo$/))) {
      res.writeHead(200, { "Content-Type": "image/png" });
      return res.end(STILL);
    }
    if ((match = path.match(/^\/api\/live\/channels\/(\d+)\/watch$/)) && req.method === "POST") {
      const id = Number(match[1]);
      const ask = body ? JSON.parse(body) : {};
      fixture.liveWatches.push({ id, client_id: ask.client_id ?? null, take_over: ask.take_over ?? null });
      if (!ask.client_id) problems.push("a watch without a client_id");
      const channel = liveChannels().find((c) => c.id === id);
      if (!channel) return json(404, { code: "not_found", message: "no such channel" });
      // Channel 3's source has one connection, Ben's other device on
      // Fixture News holds it: taken over, it is this one's.
      if (id === 3 && ask.take_over !== 1) {
        return json(409, {
          code: "conflict",
          message: "Fixture IPTV allows one connection at once, and it is in use: watching Fixture News.",
          connections: [{ channel_id: 1, channel_name: "Fixture News", watchers: [], people: 1, yours: true, can_take_over: true }],
        });
      }
      // The same client's earlier session ends with this one.
      for (const [session, watching] of fixture.liveSessions) if (watching.client === ask.client_id) fixture.liveSessions.delete(session);
      const session = `live${fixture.liveWatches.length}`;
      fixture.liveSessions.set(session, { client: ask.client_id, channel: id });
      fixture.liveRecent = [id, ...fixture.liveRecent.filter((r) => r !== id)];
      return json(200, { session_id: session, url: `/api/live/sessions/${session}/index.m3u8`, kind: "hls", channel });
    }
    if ((match = path.match(/^\/api\/live\/sessions\/(\w+)\/index\.m3u8$/)) && req.method === "GET") {
      if (!fixture.liveSessions.has(match[1])) return json(410, { code: "session_stopped", message: "this channel isn't being watched any more" });
      fixture.livePlaylists.push(match[1]);
      res.writeHead(200, { "Content-Type": "application/vnd.apple.mpegurl" });
      return res.end("#EXTM3U\n#EXT-X-VERSION:6\n#EXT-X-TARGETDURATION:2\n#EXT-X-MEDIA-SEQUENCE:0\n#EXTINF:2.000000,\ns0.ts\n");
    }
    if ((match = path.match(/^\/api\/live\/sessions\/(\w+)$/)) && req.method === "DELETE") {
      fixture.liveLeft.push(match[1]);
      const known = fixture.liveSessions.delete(match[1]);
      if (!known) return json(404, { code: "not_found", message: "not watching" });
      res.writeHead(204);
      return res.end();
    }
    if (path === "/api/search") {
      const q = (url.searchParams.get("q") ?? "").toLowerCase();
      const channels = liveChannels().filter((c) => c.name.toLowerCase().includes(q) || c.number.startsWith(q)).slice(0, 12);
      // A person whose name matches, for search's last row before Discover.
      const people = [{ id: 11, name: "Monty Fixture", photo_url: "/api/people/11/photo?v=1" }].filter((p) => p.name.toLowerCase().includes(q));
      return json(200, { items: [...movies, show].filter((item) => item.title.toLowerCase().includes(q)).slice(0, 40), people, channels });
    }
    // A keyed session's picture subtitles, which the app draws: their list
    // around where the player is (the first time a session asks, those
    // alone, as a server still reading the track has them), then all of
    // them; and each one's PNG. Without the token: the key opens them.
    // `picturesMissing` answers as a server from before them.
    if ((match = path.match(/^\/api\/media\/pley_mk_(\w+)\/subtitles-(\d+)\.json$/)) && req.method === "GET") {
      const [, session, index] = match;
      const playback = fixture.playbacks[Number(session.slice(1)) - 1];
      const at = url.searchParams.get("at");
      if (!/^\d+$/.test(at ?? "")) problems.push(`picture subtitles asked for at ${JSON.stringify(at)}`);
      if (auth) problems.push("picture subtitles asked for with the token: the key opens them");
      if (!playback?.keyed || closed.includes(session)) return json(404, { code: "not_found", message: "playback session expired" });
      // As keyed media of a server from before them: a name it doesn't know.
      if (fixture.picturesMissing) return json(404, { code: "not_found", message: playback.subtitle_index === null ? "unknown file" : "unknown playlist file" });
      const asked = fixture.picturesAsked.filter((p) => p.session === session && p.index === Number(index)).length;
      fixture.picturesAsked.push({ session, index: Number(index), at: Number(at) });
      const duration = items.get(playback.item)?.duration_ms ?? 1800000;
      const cues = [];
      // Each picture named with the list's `at`, as the server names them.
      for (let start = 5000; start + 2500 <= duration; start += 10000) {
        cues.push({ start_ms: start, end_ms: start + 2500, x: 560, y: 900, width: 800, height: 100, image: `subtitles-${index}-${start}.png?at=${at}` });
      }
      const around = cues.filter((c) => c.end_ms >= Number(at) - 60000 && c.start_ms <= Number(at) + 120000);
      return json(200, { complete: asked > 0, width: 1920, height: 1080, cues: asked > 0 ? cues : around });
    }
    if ((match = path.match(/^\/api\/media\/pley_mk_(\w+)\/(subtitles-\d+-\d+\.png)$/)) && req.method === "GET") {
      if (auth) problems.push("a picture subtitle fetched with the token: the key opens it");
      if (!/^\d+$/.test(url.searchParams.get("at") ?? "")) problems.push(`${match[2]} fetched without its list's at`);
      fixture.picturesFetched.push(match[2]);
      res.writeHead(200, { "Content-Type": "image/png", "Cache-Control": "private, max-age=3600" });
      return res.end(png(800, 100, (x, y) => (y > 30 && y < 70 && x > 40 && x < 760 ? [235, 235, 235] : [0, 0, 0])));
    }
    if (path.startsWith("/api/files/") || path.startsWith("/api/media/")) {
      res.writeHead(200, { "Content-Type": "application/octet-stream" });
      return res.end(Buffer.alloc(16));
    }
    return json(404, { code: "not_found", message: `nothing at ${path}` });
  });

  const fixture = { issuer: "", down: false, qr: true, proofs: 0, requestScopes: true, requestsOn: true, twoStep: false, scopesAsked: [], playbackDelayMs: 0, refreshes: 0, playbacks: [], libraryOrder: [1, 2, 3], orders: [], trackDefaults: { audio_language: null, subtitle_language: null, subtitle_mode: "forced" }, trackDefaultsSaved: [], streams: [], picks: new Map(), ratings: [], myList: [1003], marks: [], playlistEntries: [[1, 1000], [2, 101], [3, 102]], playlistRemoved: [], playlistOrders: [], playlistsAsked: [], playlistsMade: [], smartPlaylists: [], nextPlaylist: 2,
    household: [{ id: 1, name: "Anna", avatar_url: null, avatar_color: "#38bdf8", current: true, needs: "none" }, { id: 2, name: "Ben", avatar_url: null, avatar_color: "#f472b6", current: false, needs: "none" }],
    switched: [], subtitlesAdded: [], picturesMissing: false, picturesAsked: [], picturesFetched: [], probed: false, networkTold: [], speedTests: [], requestsAsked: [], requestsActed: [], discoverPictures: [], problems, requests, progress, closed, close: () => server.close(),
    liveRecent: [2], liveBrowses: [], liveFavorites: [], liveFavoriteChanges: [], liveGuides: [], liveWatches: [], liveSessions: new Map(), livePlaylists: [], liveLeft: [] };
  // The channels, each with what's on now and next, by the half hour.
  function liveProgram(channelId, start) {
    const titles = { 1: ["Fixture Morning", "Fixture at One"], 2: ["Match Day", "Tennis Live"] }[channelId];
    const n = Math.floor(start / 1800);
    return { channel_id: channelId, start, stop: start + 1800, title: `${titles[n % 2]} ${n % 7}`, subtitle: null, description: "What the fixture shows.", categories: [], image: null, season: 1, episode: n % 7, is_new: n % 2 === 0, is_live: channelId === 2, rating: null };
  }
  function liveChannels() {
    const now = Math.floor(Date.now() / 1000);
    const slot = Math.floor(now / 1800) * 1800;
    return [
      { id: 1, name: "Fixture News", number: "101", group: "News" },
      { id: 2, name: "UK| Fixture Sport", number: "102", group: "UK| Sport" },
      { id: 3, name: "Fixture Movies", number: "103", group: "News" },
    ].map((c) => ({
      ...c, source_id: 1, logo: `/api/live/channels/${c.id}/logo`, favorite: fixture.liveFavorites.includes(c.id), has_guide: c.id !== 3,
      now: c.id !== 3 ? liveProgram(c.id, slot) : null, next: c.id !== 3 ? liveProgram(c.id, slot + 1800) : null,
    }));
  }
  const requestsApi = fakeRequests(fixture, png);
  fixture.requestArrives = requestsApi.arrive;
  return new Promise((resolve) => {
    server.listen(0, "127.0.0.1", () => {
      issuer = `http://127.0.0.1:${server.address().port}`;
      fixture.issuer = issuer;
      resolve(fixture);
    });
  });
}

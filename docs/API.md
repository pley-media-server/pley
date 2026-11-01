# Pley HTTP API (v1)

The API the web app and Pley's own apps (Android, desktop, Kodi, Roku,
Samsung and LG) use, for anyone writing a client or a script. Tools written
for the widely used media-server API that Pley also speaks go through the
compatibility API instead: [COMPATIBILITY.md](COMPATIBILITY.md).

The base URL is the server's address with `/api`:
`http://pley.local:7539/api`, or `https://media.example.com/api` behind a
reverse proxy. Sign in with `POST /api/auth/login` and send the token it
answers as `Authorization: Bearer <token>`; the web app uses the `HttpOnly`
cookie the same call sets. Apps by other publishers, and Pley's own apps,
sign in through OAuth 2.1 instead and get tokens limited to what the person
allowed ([Apps](#apps-oauth-21)). [Authentication](#authentication) has the
details.

## Contents

- [Conventions](#conventions)
- [Authentication](#authentication): [Who](#who), [Headers](#headers),
  [CORS for TV apps](#cors-for-tv-apps), [Errors](#errors)
- [Types](#types)
- [Public endpoints](#public-endpoints), [Auth](#auth),
  [Two-step sign-in](#two-step-sign-in), [Your account](#your-account)
- [Apps (OAuth 2.1)](#apps-oauth-21): [Discovery](#discovery),
  [Signing in](#signing-in), [Tokens](#tokens), [TVs](#tvs),
  [Lifetimes and limits](#lifetimes-and-limits), [Scopes](#scopes),
  [Signed-in apps](#signed-in-apps), [Profiles](#profiles),
  [Pley's own apps](#pleys-own-apps),
  [Apps by other publishers](#apps-by-other-publishers),
  [The consent page's endpoints](#the-consent-pages-endpoints)
- [Access rules](#access-rules)
- [Libraries](#libraries), [Items](#items),
  [Added subtitles](#added-subtitles)
- [Home & search](#home--search), [Continue Watching](#continue-watching),
  [My List](#my-list), [Cast and crew](#cast-and-crew),
  [Collections](#collections), [Playlists](#playlists)
- [Playback](#playback): [Play request](#play-request),
  [The audio track](#the-audio-track), [Keyed URLs](#keyed-urls),
  [Network memory and speed test](#network-memory-and-speed-test),
  [Version 2](#version-2), [HDR transcodes](#hdr-transcodes),
  [Several sizes](#several-sizes),
  [Dolby Vision remuxes](#dolby-vision-remuxes)
- [Downloads](#downloads)
- [Intro and credits detection](#intro-and-credits-detection),
  [Library work](#library-work)
- [Watch history](#watch-history)
- [Activity & events](#activity--events):
  [Stopping a stream](#stopping-a-stream),
  [Watch state on other devices](#watch-state-on-other-devices)
- [Live TV](#live-tv)
- [Users](#users-admin), [Invitations](#invitations),
  [System](#system-admin)

## Conventions

- Every endpoint is under `/api`. Unknown `/api` paths answer `404`.
- Requests and responses are JSON with `snake_case` keys, unless a row says
  otherwise.
- Timestamps are unix seconds. Durations and positions are milliseconds.
  Ids are integers unless stated otherwise.
- Optional fields are left out when they have no value: treat a missing
  field like `null`.
- Paging: `GET /api/libraries/{id}/items` and `GET /api/history` take
  `offset` and `limit` and answer a `total` that counts every match, not
  just the page.
- API answers carry `Cache-Control: no-store` unless a row says otherwise.
- Text answers over 1 KiB are compressed when the request accepts it; media,
  images and the event stream never are.
- A request still running after 30 minutes answers `408`.
- The compatibility API answers on the same port by default, but only to
  requests that are its own (its headers, its token in the query, or a path
  only it has). `/api/…` and `/.well-known/…` always belong to this API.

## Authentication

`POST /api/auth/login` signs in (so do `/api/setup`, `/api/auth/switch` and
accepting an invitation). The answer sets an `HttpOnly` cookie named
`pley_token`. Clients that send no `Origin` header (not browsers) also get
the token in the body; a browser keeps it in the cookie only, so no script
on the page sees it.

Every endpoint but the `public` ones takes the token in one of these:

| Where | Notes |
|---|---|
| `pley_token` cookie | the web app: `<img>`, `<video>` and hls.js requests carry it (same origin) |
| `Authorization: Bearer <token>` | the only place an app's token may go |
| `X-Pley-Token: <token>` | |
| `?token=<token>` | for external players such as VLC |

A missing or invalid token answers `401`; a non-administrator on an `admin`
route, `403`.

Apps sign in through OAuth ([Apps](#apps-oauth-21)). Their access tokens
(`pley_at_…`) go in `Authorization: Bearer` only, and the compatibility API
refuses them. Refusals of an app's token carry `WWW-Authenticate`
(RFC 6750 §3):

| Status | `code` | `WWW-Authenticate` | When |
|---|---|---|---|
| `400` | `bad_request` | `Bearer error="invalid_request"` | the token was sent anywhere but `Authorization` |
| `401` | `unauthorized` | `Bearer error="invalid_token"` | expired, signed out or unknown token; a refresh token sent as a bearer token; a disabled account |
| `403` | `insufficient_scope` | `Bearer error="insufficient_scope", scope="play"` | the token's scopes don't open the route. `scope` names the one that would; it is left out when none does ("apps can't do that: use Pley's web app") |

### Who

Each endpoint table has a **Who** column:

| Who | May call |
|---|---|
| `public` | anyone, without a token |
| `user` | any signed-in user; an app's token is refused (`403 insufficient_scope`) |
| `any app` | any signed-in user, and every app's token |
| `library`, `play`, `playlists`, `profiles`, `requests`, `approvals` | any signed-in user, and an app's token with that [scope](#scopes) (`HEAD` as `GET`) |
| `admin` | administrators only (`403` for others), never an app's token |
| `page` | Pley's consent page only ([its endpoints](#the-consent-pages-endpoints)) |

### Headers

- Every answer carries `X-Content-Type-Options: nosniff`,
  `X-Frame-Options: DENY`, `Referrer-Policy: same-origin`,
  `Cross-Origin-Opener-Policy: same-origin` and a `Content-Security-Policy`
  that loads only Pley's own scripts and styles.
- Password checks queue, a few at a time. One that has waited 10 seconds
  answers `503`.

### CORS for TV apps

Pley's Samsung and LG apps are web apps loaded from the TV itself, and
their web engines apply CORS. Their requests come with `Origin: file://`,
`null` or an `app://` address. For those origins only, the API and
`/.well-known/oauth-authorization-server` answer CORS:

- a preflight lets `Authorization`, `Content-Type`, `Accept` and `Range`
  through, for 10 minutes;
- answers carry `Access-Control-Allow-Origin: *` and expose
  `WWW-Authenticate`, `X-Pley-Complete`, `ETag`, `Content-Range` and
  `Retry-After`.

A wildcard origin never lets a page read an answer made with cookies, so
these answers only carry what a token in `Authorization` opens. The consent
page's endpoints (`/api/oauth/requests…`, `/api/oauth/device/verify`) never
answer CORS.

### Errors

Non-2xx answers carry:

```json
{ "code": "not_found", "message": "item 42 not found" }
```

| `code` | Status | When |
|---|---|---|
| `bad_request` | `400` | |
| `unauthorized` | `401` | |
| `two_step` | `401` | the account has two-step sign-in, and the code is missing or wrong |
| `two_step_setup` | `401` | the server requires two-step sign-in, and the account sets it up first: the body also has `setup`, the secret to add to an authenticator app ([Two-step sign-in](#two-step-sign-in)) |
| `forbidden` | `403` | |
| `insufficient_scope` | `403` | an app's token on a route its scopes don't open |
| `not_found` | `404` | also anything the user [may not see](#access-rules) |
| `conflict` | `409` | |
| `session_stopped` | `410` | the playback session was [stopped from elsewhere](#stopping-a-stream); the body also has `"error": "session_stopped"`, and `message` is what the person who stopped it wrote, or `null` |
| `too_many_requests` | `429` | attempt limits, below |
| `internal` | `500` | |
| `not_implemented` | `501` | |
| `unavailable` | `503` | |

**Attempt limits.** Sign-in, PIN, password and code attempts: 5 failures in
15 minutes, counting attempts still being checked. Sign-ins count per
account from each address, and 50 from all addresses together. Then `429`.

The OAuth token and revocation endpoints answer errors in OAuth's own form
instead (see [Tokens](#tokens)).

---

## Types

```ts
type Id = number;        // a library's, item's, file's or user's id is never reused, also once deleted
type Timestamp = number; // unix seconds

interface User {
  id: Id;
  username: string;
  name: string;                     // display name, or the username
  display_name: string | null;
  is_admin: boolean;
  managed: boolean;                 // profile without a password, reached by switching
  home: boolean;                    // listed in the household's profile switcher
  has_pin: boolean;
  avatar_url: string | null;
  avatar_color: string | null;      // "#38bdf8"
  all_libraries: boolean;           // false: only `library_ids`
  library_ids: Id[];
  max_age: number | null;           // age limit for movies and shows; null = none
  disabled: boolean;
  created_at: Timestamp;
  // Default tracks for titles without a remembered choice (see Your account):
  audio_language: string | null;    // ISO 639 code ("eng", "fr"); null = the file's default track
  subtitle_language: string | null; // null = the audio language
  subtitle_mode: "off" | "forced" | "foreign" | "always";
  has_password: boolean;            // false: a managed profile, or an account without a password
  two_step: boolean;                // two-step sign-in is on
  request_access: "off" | "ask" | "auto"; // Requests: may not ask, asks an administrator, or approved as
                                    // made. Administrators: "auto" (in /api/auth/me, "ask" for an app by
                                    // another publisher: what asking from there does). Left out when made,
                                    // and for an invitation's accounts: "off" for managed profiles and
                                    // accounts with an age limit, else "ask". Adding an age limit to an
                                    // "ask" account makes it "off", unless the same change sets it
}

type LibraryKind = "movies" | "shows" | "music";

interface Library {
  id: Id;
  name: string;
  kind: LibraryKind;
  paths: string[];                  // empty except for administrators (not their apps' tokens)
  item_count: number;               // top-level items (movies, shows, artists) this user can see
  created_at: Timestamp;
  scanned_at: Timestamp | null;
  scanning: boolean;                // a scan is queued or running
}

type ItemKind = "movie" | "show" | "season" | "episode" | "artist" | "album" | "track"
              | "extra";            // trailers and other bonus material (ItemDetail.extras)

interface UserState {
  position_ms: number;              // resume position, 0 if none
  watched: boolean;                 // containers: every leaf is watched
  play_count: number;
  last_played_at?: Timestamp;
  rating?: number;                  // the user's own rating, 0-10 (two points per star)
}

interface ItemSummary {
  id: Id;
  library_id: Id;
  kind: ItemKind;
  title: string;
  sort_title: string;
  year?: number;
  index?: number;                   // season, episode or track number
  parent_id?: Id;                   // episode: season; season: show; track: album; album: artist
  parent_title?: string;
  parent_index?: number;            // episode: season number; track: disc number
  grandparent_id?: Id;              // episode: show; track: artist
  grandparent_title?: string;
  duration_ms?: number;
  rating?: number;                  // 0..10
  added_at: Timestamp;
  poster_url?: string;              // "/api/items/12/images/poster?v=1729600000" (append &w=<px>)
  backdrop_url?: string;
  thumb_url?: string;               // episode stills, video frame grabs (16:9)
  user_state: UserState;
  leaf_count?: number;              // show, season: episodes; artist, album: tracks
  viewed_leaf_count?: number;       // show, season: watched episodes
  child_count?: number;             // show: seasons; artist: albums
  summary?: string;                 // lists: episodes only, or every kind with `summaries=1`; always in ItemDetail
  release_date?: string;            // "YYYY-MM-DD"
  extra_type?: string;              // kind "extra": trailer | featurette | behind_the_scenes | deleted_scene |
                                    // interview | scene | short | other
  media?: MediaSummary;             // only in library listings
}

interface MediaSummary {            // a movie's best file, or a show's episodes together
  bitrate: number | null;           // bits per second (show: its episodes' average)
  size: number | null;              // bytes (show: all its episodes)
  width: number | null;
  height: number | null;            // show: its largest episode
  video_codec: string | null;       // null for shows
  dynamic_range: "sdr" | "hdr10" | "hdr10plus" | "hlg" | "dolby_vision" | null; // null for shows
}

interface ItemDetail extends ItemSummary {
  original_title: string | null;
  tagline: string | null;
  genres: string[];
  studio: string | null;
  content_rating: string | null;    // "PG-13"
  tmdb_id: number | null;
  imdb_id: string | null;
  locked_fields: string[];          // fields an administrator edited (PATCH /api/items/{id})
  files: MediaFile[];               // empty for containers (show, season, artist, album)
  markers: Marker[];                // intro and credits (Skip Intro, Skip Credits)
  extras: ItemSummary[];            // movies and shows: trailers first, then other bonus material (kind "extra")
  cast: Credit[];                   // actors in billing order; episodes: guest stars; seasons: the show's
  crew: Credit[];                   // creators, directors, writers, producers
  in_watchlist: boolean;            // in the user's My List (movies and shows)
}

interface Credit {
  id: Id;                           // the person: GET /api/people/{id}
  name: string;
  role: "actor" | "creator" | "director" | "writer" | "producer";
  character?: string;               // actors: the part played
  photo_url?: string;               // "/api/people/7/photo?v=…" (append &w=<px>)
}

interface PersonSummary {
  id: Id;
  name: string;
  photo_url?: string;
}

interface PersonDetail extends PersonSummary {
  credits: {                        // newest first; episode credits count towards their show
    item: ItemSummary;              // a movie or show
    roles: { role: Credit["role"]; character?: string; episodes?: number }[];
  }[];
}

interface Marker {
  file_id: Id;                      // markers belong to a file: versions can be different cuts
  kind: "intro" | "credits";
  start_ms: number;
  end_ms: number;                   // credits that run to the end of the file end at its duration
  source: "chapter" | "fingerprint" | "frames";
}

interface MediaFile {
  id: Id;
  path: string;                     // the file name only, except for administrators (not their apps' tokens)
  size: number;                     // bytes
  container: string | null;         // "mkv" | "mp4" | "webm" | "avi" | "mp3" | "flac" | ...
  duration_ms: number | null;
  bitrate: number | null;           // bits per second
  width: number | null;
  height: number | null;
  video_codec: string | null;       // ffprobe codec name: "h264", "hevc", "av1", ...
  audio_codec: string | null;       // codec of the default audio stream
  streams: MediaStream[];
  edition: string | null;           // from `{edition-Final Cut}` in the file's or its folder's name
  selected_audio: number | null;    // track to start with for this user (their choice, else their
                                    // defaults); null = the file's default. Not player-specific:
                                    // a play request picks that language's best track for its
                                    // player (see The audio track)
  selected_subtitle: number | null; // likewise; null = off
  audio_picked: boolean;            // true: the user picked selected_audio for the show or movie;
                                    // false: automatic, so a title's page names the language, not a track
  chapters: Chapter[];              // from the container, by start time (empty if none)
}

interface Chapter {
  start_ms: number;
  end_ms: number;
  title: string | null;
  thumb_url?: string;               // picture of the chapter, once the file has seek previews
}

interface MediaStream {
  index: number;                    // ffprobe stream index (below 1000); subtitle files next to the video
                                    // 1000-8999; added subtitles 9000-9999 (see Added subtitles)
  kind: "video" | "audio" | "subtitle";
  codec: string;
  language: string | null;          // ISO 639-2 as found in the file: "eng"
  title: string | null;
  default: boolean;
  forced: boolean;
  bitrate?: number;
  // video
  width?: number;
  height?: number;
  frame_rate?: number;
  bit_depth?: number;
  hdr?: boolean;
  dynamic_range?: "sdr" | "hdr10" | "hdr10plus" | "hlg" | "dolby_vision"; // "hdr10plus": PQ with HDR10+ in its frames
  dv_profile?: number;              // Dolby Vision profile (5, 7, 8, …)
  dv_bl_compat?: number;            // DV base-layer compatibility id: 1 = HDR10, 2 = SDR, 4 = HLG,
                                    // 6 = Blu-ray HDR10 (profile 7), 0 = none
  dv_el?: "fel" | "mel";            // profile 7's enhancement layer: full (picture data) or minimal;
                                    // absent: not known
  decode_order_times?: boolean;     // frames timed in decode order, not display order: the video is converted
  dv_level?: number;                // Dolby Vision level
  hdr10plus?: boolean;              // Dolby Vision and PQ streams: HDR10+ in the first frames (absent: not
                                    // looked for). On Dolby Vision, true means Dolby Vision + HDR10+
  // Clients name the dynamic range the same way everywhere: "Dolby Vision P8.1", "Dolby Vision P8.4",
  // "Dolby Vision P5", "Dolby Vision P7 FEL" (or MEL; "P7" when not known, never "P7.6"),
  // "Dolby Vision P8.1 + HDR10+", "HDR10+", "HDR10", "HLG". Badges shorten "Dolby Vision" to "DV".
  level?: number;                   // codec level as ffprobe gives it (HEVC 5.1 = 153)
  pix_fmt?: string;                 // "yuv420p10le", …
  interlaced?: boolean;
  rotation?: number;                // degrees counterclockwise the picture is turned to be shown (phone
                                    // video: 90 or -90); absent when not turned
  has_b_frames?: boolean;
  color_transfer?: string;          // "smpte2084", "arib-std-b67", …
  color_primaries?: string;
  color_space?: string;
  // video and audio
  profile?: string;                 // codec profile as ffprobe names it: "Main 10", "High", "DTS-HD MA",
                                    // "Dolby TrueHD + Dolby Atmos"
  // audio
  channels?: number;
  channel_layout?: string;
  sample_rate?: number;
  // subtitle
  external?: boolean;               // a file next to the media, or added subtitles
  added?: boolean;                  // added subtitles (uploaded, or from OpenSubtitles); external too
  text_based?: boolean;             // srt/ass/webvtt/mov_text: true; pgs/vobsub/dvb: false (drawn by players that can, else burned in)
}

interface Hub {
  key: string;                      // "continue_watching" | "my_list" | "recently_added:<library_id>" | ...
  title: string;
  library_id: Id | null;
  items: ItemSummary[];
}

interface SubtitleTrack {
  index: number;
  codec: string;
  language: string | null;
  title: string | null;
  default: boolean;
  forced: boolean;
  external: boolean;                // a file next to the video, or added subtitles
  text_based: boolean;
  url: string | null;               // WebVTT url for text subtitles; null for picture subtitles
  added: boolean;                   // added subtitles (see Added subtitles)
  deletable?: true;                 // added subtitles this user may delete (an administrator, or who added
                                    // them); left out otherwise
}

interface PlaybackInfo {
  session_id: string;
  method: "direct" | "remux" | "transcode";
  url: string;                      // direct: the file's url. Otherwise the HLS playlist: master.m3u8 for
                                    // version 2 players with hls.master_playlist and for an adaptive
                                    // stream, else index.m3u8
  hls: boolean;
  file_id: Id;
  duration_ms: number;
  audio_index: number | null;
  subtitle_index: number | null;    // the subtitle burned in (only when burn_subtitles)
  burn_subtitles: boolean;
  audio_converted: boolean;         // the audio is converted (also in a remux, where the video is copied)
  subtitles: SubtitleTrack[];       // every subtitle track of the file
  reasons: string[];                // why it isn't played as it is, one short sentence each
  reason: string | null;            // the same in one line
  auto_quality: string | null;      // the original converted: the closest quality players offer ("30m" =
                                    // 30 Mbps 4K, "20m", "15m" = 15 Mbps 4K, "12m", "8m", "4m", "2m");
                                    // null when a quality was asked for
  video_range: string | null;       // what the screen gets of HDR video, for the player's details (null for
                                    // SDR): "Dolby Vision P8.1", "Dolby Vision P7 FEL", "HDR10+", "HLG",
                                    // "HDR10 (Dolby Vision base layer, as this screen doesn't show Dolby
                                    // Vision)", "HDR10+ (Dolby Vision base layer, as this player doesn't
                                    // decode profile 7)": the base layer plays for the screen, the
                                    // player's profiles or level, a remux that doesn't name Dolby Vision,
                                    // or HDR10+ in the stream; "as Dolby Vision isn't shown here" for a
                                    // player that lists no profiles (Kodi and Roku can't tell their
                                    // screen from their decoders);
                                    // "HDR10 (Dolby Vision base layer)" when the video is converted from it,
                                    // "Dolby Vision P8.1 (converted from P7 FEL)",
                                    // "Dolby Vision P8.1 (HDR10+ left out)",
                                    // "HDR10 (this screen doesn't show HDR10+)", "HDR10 (HDR10+ left out)",
                                    // "HDR10 (converted from HLG)", "SDR (tone-mapped)", and
                                    // "Dolby Vision P5 not decoded (wrong colours)" for a direct play the
                                    // player insisted on. Version 1 players don't say which HDR formats
                                    // their screen shows, so their HDR10+ is named as it is
  adaptive: boolean;                // the transcode comes in several sizes the player moves between
  live_bitrate: boolean;            // the video is converted, and PUT …/bitrate changes its bitrate as it
                                    // plays. Not for a player that moves between sizes itself
                                    // (hls.variants; see Several sizes)
  pts_offset_ms?: number | null;    // how far the stream's timestamps run ahead of the file's (its start
                                    // taken off, as `.mks` subtitles are timed): 1400 for HLS, wherever it
                                    // started; null for a direct play. A player that knows the timestamp
                                    // of the frame it shows (hls.js: media time + initPTS) draws picture
                                    // subtitles at that minus this
}
```

**`summaries=1`.** Lists leave out synopses (a big library's would be
several times larger). `summaries=1` in the query of `GET /api/hubs/home`,
`/api/libraries/{id}/items`, `/api/items/{id}/children`,
`/api/collections/{id}`, `/api/playlists/{id}` and `/api/search` brings
every item's `summary`, for clients whose information page shows only what
lists carry (Kodi).

---

## Public endpoints

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/health` | `public` | `{ "status": "ok" }` |
| GET | `/api/server` | `public` | `{ name, version, commit, source, setup_required, setup_code_required, two_step_required }` |
| GET | `/api/oauth/device/qr?code=` | `public` | where to type a TV's code, as a QR code (PNG): see [TVs](#tvs) |
| POST | `/api/setup` | `public` | `{ username, password, setup_code }` → `{ user: User, token? }`: creates the first (admin) account and signs in |
| POST | `/api/auth/login` | `public` | `{ username, password, code?, setup_id? }` → `{ user: User, token?, recovery_codes? }` |

- `/api/server`: `commit` is the build's short hash, `"unknown"` when built
  without the repository. `source` is where the server's source code is
  offered; the web app's Account page links to it. The setup code is in
  the server's log. `two_step_required`: the server requires two-step
  sign-in of every account with a password
  ([Two-step sign-in](#two-step-sign-in)), so signing in may set it up
  first.
- `/api/setup`: `409` once a user exists; `403` without the right
  `setup_code`. 5 tries per 15 minutes from each address and 1000 from all
  addresses, then `429`.
- `/api/auth/login`: `401` for bad credentials. With two-step sign-in,
  `code` is a code from the authenticator app or a recovery code; without
  it, or with a wrong one, `401` with `code: "two_step"`. When the server
  requires two-step sign-in and the account doesn't use it yet, the same
  request sets it up: `401` with `code: "two_step_setup"` and `setup` (the
  secret, as `POST /api/me/two-step/setup` answers it, and its `id`) until
  `code` is the first code of that secret, sent with `setup_id`, its `id`;
  then the answer signs in and has `recovery_codes`, shown once. Only a
  request with `setup_id` gets the same secret again; any other gets a new
  one, which replaces it, so two people with the password never share
  one. Attempts are limited as in
  [Errors](#errors). A disabled account gets `403`, and its sessions stop
  working.

## Auth

| Method | Path | Who | What |
|---|---|---|---|
| POST | `/api/auth/logout` | `public` | `204`, cookie cleared; ends the session of the token sent, if any. With an app's token, wherever it was sent, signs that app out (its whole grant ends) |
| GET | `/api/auth/me` | `any app` | `User` |
| PUT | `/api/auth/password` | `user` | `{ current_password, new_password }` → `204` |
| GET | `/api/auth/profiles` | `user` | `User[]`: home profiles this user can switch to (empty for non-home users) |
| POST | `/api/auth/switch` | `user` | `{ user_id, pin?, password?, code?, setup_id? }` → `{ user, token?, recovery_codes? }`: this device becomes that profile |

- `GET /api/auth/me` with an app's token: `User & { scope: string }`.
  `scope` is what the app was allowed (`"library play"`); `is_admin`,
  `all_libraries` and `max_age` are the app's rights (`is_admin` is always
  false).
- `PUT /api/auth/password` signs the user out everywhere but this device,
  and signs out every app using the account or allowed by it.
- `POST /api/auth/switch` ends this device's old session. What the switch
  needs:
  - a profile with a PIN: the PIN. After 100 wrong PINs in a row it stops
    working until it is set again;
  - a profile without a PIN that can see more than yours (an administrator,
    a higher age limit, libraries yours can't open): its password, and its
    `code` with two-step sign-in, unless you are an administrator;
  - a managed profile without a PIN can't be switched to;
  - while the server requires two-step sign-in, a profile with a password
    that doesn't use it yet: its password, whatever else it has and
    whoever switches (`403` without one). Then it sets two-step sign-in up
    as a sign-in does (`two_step_setup` until `code` is the secret's first
    code, with `setup_id`), and the answer has its `recovery_codes`.

  `429` after 5 wrong tries in 15 minutes. A PIN refused is `403` with
  `code` `wrong_pin` (missing or wrong) or `pin_locked`, as the apps'
  [switch-profile grant](#tokens) says by its `reason`.

### Two-step sign-in

Optional for each account with a password, unless an administrator
requires it of every one (below). After the password comes a 6-digit code
from an authenticator app (TOTP: SHA-1, 30-second steps) or one of ten
recovery codes. Each code works once. Wrong codes count like wrong
passwords; after 100 in a row only a recovery code signs in.
`pley reset-password` turns it off.

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/me/two-step` | `user` | `{ enabled, since: Timestamp \| null, recovery_codes_left }` |
| POST | `/api/me/two-step/setup` | `user` | `{ password, code? }` → `{ secret, uri, qr: { size, path }, expires_at, id }` |
| POST | `/api/me/two-step` | `user` | `{ code }` → `{ recovery_codes: string[] }`: turns it on, or moves it to the new secret |
| DELETE | `/api/me/two-step` | `user` | `{ password, code }` → `204`: turns it off; `403` while the server requires it |
| POST | `/api/me/two-step/recovery-codes` | `user` | `{ password }` → `{ recovery_codes }`: new codes; the old ones stop working |

- `setup` makes a new secret: `secret` is base32, `uri` its `otpauth://`
  URI (on a phone, a link to an authenticator app on it), and `qr` a QR
  code of it (`path` draws its dark modules on a `size`-wide grid).
- While it is on, `setup` moves it to another authenticator app, and takes
  `code` too: a code from the current app or a recovery code, as turning
  it off does (`401 two_step` without a right one). Until the new secret's
  first code, the current app keeps working.
- While the server can't use its `secret.key` (the log says why), `setup`
  answers `503` before checking anything, so no recovery code is spent,
  and codes from the authenticator app get `503` at sign-in: recovery
  codes still sign in.
- `POST /api/me/two-step` takes a code from the new secret within 30
  minutes, 5 tries (then `setup` again). It answers new recovery codes,
  shown once; after a move the old app's codes and the old recovery codes
  stop working.

**Requiring it.** An administrator who uses two-step sign-in can require
it of every account with a password (`require_two_step` in the
[settings](#system-admin), Settings → Server → Sign-in):

- Turning it on signs out everyone without it, on every device and app
  (sessions, apps acting as them or allowed by them, consent requests and
  codes in progress). Their sessions and app tokens stop working, and
  refreshes get `invalid_grant`, even ones that slip past the sign-out.
- Each sets it up at their next sign-in, right after the password, in the
  same request: `/api/auth/login` (above), an invitation's acceptance
  ([Invitations](#invitations)) and a switch to their profile
  ([Auth](#auth)) answer `two_step_setup` with the secret until its first
  code comes. Apps sign in on Pley's own pages, which do the same: a TV's
  person sets it up on the phone where they type the TV's code.
- Turning it off is refused (`403`): it can move to another app instead.
  An administrator turning it off for someone ([Users](#users-admin)) signs
  them out everywhere, and they set it up again at their next sign-in, as
  after `pley reset-password`.
- Managed profiles and accounts without a password have nothing to set up.
- While the server can't use its `secret.key` (the log says why, and
  `secret_key_ok` in [`/api/system`](#system-admin) is `false`), it stays
  required and nobody can set it up: a sign-in, invitation or switch that
  would set it up answers `503` instead, and makes no session or account.
  Accounts that use it sign in with a recovery code, and an administrator
  signed in so can stop requiring it.

## Your account

| Method | Path | Who | What |
|---|---|---|---|
| PATCH | `/api/me` | `user` | `{ display_name?, avatar_color?, audio_language?, subtitle_language?, subtitle_mode? }` (`null` clears) → `User` |
| PUT | `/api/me/track-defaults` | `play` | `{ audio_language?, subtitle_language?, subtitle_mode? }` → the profile as [`GET /api/auth/me`](#auth) shows it |
| PUT | `/api/me/pin` | `user` | `{ pin: string \| null, password }` → `204`. 4–8 digits; `null` removes it; `403` for a wrong password, and for a managed profile (an administrator sets its PIN) |
| PUT | `/api/me/avatar` | `user` | raw image bytes (JPEG, PNG, WebP, GIF, BMP; ≤ 10 MB) → `User`. Stored as a 256 px square JPEG |
| DELETE | `/api/me/avatar` | `user` | `204` |
| GET | `/api/users/{id}/avatar` | `any app` | `image/jpeg`, for the profile switcher. Apps: only their own profile's and their household's, else `404` |
| GET | `/api/me/devices` | `user` | `Device[]`: `{ id, name, user_agent, ip, created_at, last_seen_at, current }` |
| DELETE | `/api/me/devices/{id}` | `user` | `204`, signs that device out |

`PUT /api/me/track-defaults` sets all three at once: a language left out is
`null`, a mode left out is `forced`. `400` for a language that isn't 2 or 3
lower-case letters, or an unknown mode. Apps use it, never `PATCH /api/me`
(name and picture stay in the web app).

**Track defaults.** Titles without a track choice start with:

- audio: the main track in `audio_language`, else the file's default;
- subtitles, by `subtitle_mode`:

  | Mode | Subtitles shown |
  |---|---|
  | `off` | never |
  | `forced` (default) | forced subtitles in the audio's language (they translate foreign dialogue) |
  | `foreign` | full subtitles in `subtitle_language` (or the audio language) when the audio is in another language |
  | `always` | full subtitles in that language whenever the file has them |

  Every mode but `off` falls back to forced subtitles.

A choice made for a show or movie wins over these defaults. Subtitles count
as forced when flagged so, or when their title says so ("forced", or
"signs" as in "Signs & Songs"). Languages match in any ISO 639 form (`en`,
`eng`; `fr`, `fre`, `fra`); an untagged or `und` track has no language.

## Apps (OAuth 2.1)

Apps sign in on Pley's own page and get tokens that open only what the
person allowed. Pley is an OAuth 2.1 authorization server
(draft-ietf-oauth-v2-1-16) with PKCE and client metadata documents
(draft-ietf-oauth-client-id-metadata-document-02), not an OpenID Connect
provider. The security design is summarized in
[SECURITY.md](../SECURITY.md#what-pley-does).

- Every app is public: no client secrets, no registration.
- An app's client ID is the https address of a JSON document that describes
  it.
- [Pley's own apps](#pleys-own-apps) are built in. [Apps by other
  publishers](#apps-by-other-publishers) sign in only when an administrator
  allows them.

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/.well-known/oauth-authorization-server` | `public` | server metadata ([Discovery](#discovery)) |
| GET | `/oauth/authorize` | `public` | the web app's consent page ([Signing in](#signing-in)) |
| POST | `/api/oauth/token` | `public` | tokens for a code, a refresh, a profile switch or a TV ([Tokens](#tokens)) |
| POST | `/api/oauth/revoke` | `public` | signs an app out ([Tokens](#tokens)) |
| POST | `/api/oauth/device` | `public` | a TV's code ([TVs](#tvs)) |
| GET | `/api/oauth/device/qr?code=` | `public` | the code's address as a QR code ([TVs](#tvs)) |
| GET | `/link` | `public` | the web app's page where a TV's code is typed ([TVs](#tvs)) |
| GET | `/api/oauth/profiles` | `profiles` | the household's profiles ([Profiles](#profiles)) |

### Discovery

**Finding servers on the network.** An app broadcasts the 5 bytes `pley?`
to UDP port 7539 (servers listen on their web port, on every address but
loopback). Each server answers from the address to use with one JSON
datagram:

```json
{ "pley": 1, "id": "3f2a…", "name": "Living room", "version": "0.9.0", "port": 7539 }
```

`pley` is the protocol version, `id` stays the same across restarts and
renames, and `port` is the web port on the address the answer came from.
Only senders on the local network get an answer, at most 100 a second.
Administrators turn it off in Settings → Network (`discovery`) or with
`--no-discovery` (`PLEY_NO_DISCOVERY`).

**Finding the sign-in endpoints.**
`GET /.well-known/oauth-authorization-server` (RFC 8414) says where
everything is, for the address it was asked at (`Cache-Control: no-store`).
Every other `/.well-known/` path answers `404`.

```json
{
  "issuer": "http://pley.local:7539",
  "authorization_endpoint": "http://pley.local:7539/oauth/authorize",
  "token_endpoint": "http://pley.local:7539/api/oauth/token",
  "revocation_endpoint": "http://pley.local:7539/api/oauth/revoke",
  "device_authorization_endpoint": "http://pley.local:7539/api/oauth/device",
  "response_types_supported": ["code"],
  "response_modes_supported": ["query"],
  "grant_types_supported": ["authorization_code", "refresh_token",
                            "urn:pley:params:oauth:grant-type:switch-profile",
                            "urn:ietf:params:oauth:grant-type:device_code"],
  "token_endpoint_auth_methods_supported": ["none"],
  "revocation_endpoint_auth_methods_supported": ["none"],
  "code_challenge_methods_supported": ["S256"],
  "scopes_supported": ["library", "play", "playlists", "profiles", "requests", "approvals"],
  "authorization_response_iss_parameter_supported": true,
  "client_id_metadata_document_supported": false,
  "service_documentation": "https://github.com/pley-media-server/pley/blob/master/docs/API.md"
}
```

- `issuer` is the address the request came to: `https` when the first
  `X-Forwarded-Proto` value says so (else `http`), then the `Host` header,
  with a lower-case host, no default port and no trailing slash. A server
  reached at a home and a public address has a different issuer at each.
- Apps check that the issuer is the address they used (RFC 8414 §3.3), so a
  reverse proxy must pass the original `Host`
  ([deploy/README.md](../deploy/README.md)). For a proxy that drops the
  port, list the public address in the settings' `proxy_addresses`
  (`https://media.example.com:9999`): a `Host` without a port, for that
  scheme and host name, takes that port.
- A missing `Host`, or one with a path or a user name: `400`.
- `client_id_metadata_document_supported` is the
  [apps by other publishers](#apps-by-other-publishers) setting.

### Signing in

1. The app opens `/oauth/authorize` (a web app page) in the browser with
   these query parameters, each at most once. A parameter without a value
   counts as left out (OAuth 2.1 §3.1); other parameters are ignored, but
   `request` and `request_uri` are refused.

   | Parameter | Value |
   |---|---|
   | `response_type` | `code` |
   | `client_id` | the app's client ID |
   | `redirect_uri` | one of the app's return addresses, exactly as registered; a loopback one (`http://127.0.0.1/…`, `http://[::1]/…`) with any port |
   | `code_challenge` | `BASE64URL(SHA-256(code_verifier))`: 43 characters (PKCE, RFC 7636) |
   | `code_challenge_method` | `S256` (the only one) |
   | `scope` | optional: scopes separated by spaces; none means `library` |
   | `state` | optional, at most 1,024 bytes; sent back as it came |
   | `resource` | optional: this server's issuer, with or without a trailing `/` (RFC 8707) |
   | `response_mode` | optional: `query` |

2. The page signs the person in if needed and shows the app, the profile it
   will act as, what it will be able to do and where it goes back to.
   People are asked every time, for Pley's own apps too, and allow all that
   was asked or nothing.
3. The browser goes back to the return address by a `303`, keeping the
   address's own query, with:
   - allowed: `code`, `state` (when the app sent one) and `iss` (the
     issuer, RFC 9207);
   - refused: `error=access_denied`, `state` and `iss`.
4. The app exchanges the code at the [token endpoint](#tokens) within 60
   seconds.

An app Pley can't accept (unknown, refused, or apps by other publishers
off), or a return address it didn't register, is shown on the page and
never sent to the app. Other mistakes go back to the app with `error`,
`error_description`, `state` and `iss`, but only when the person clicks
"Back to the app" on a page that says where that goes (RFC 9700 §4.11.2):

| `error` | When |
|---|---|
| `unsupported_response_type` | `response_type` is not `code` |
| `invalid_request` | `response_type` is missing; `response_mode` isn't `query`; `request` or `request_uri` is given; the challenge is missing or not S256; `state` is over 1,024 bytes |
| `invalid_scope` | a scope Pley doesn't offer (`openid` included); `profiles` for an app that isn't Pley's own; more than the app's description allows |
| `invalid_target` | `resource` names another server |

### Tokens

`POST /api/oauth/token`, `POST /api/oauth/revoke` and
[`POST /api/oauth/device`](#tvs):

- take a form (`application/x-www-form-urlencoded`) of at most 16 KiB and
  never read cookies;
- read each parameter at most once; one without a value counts as left out,
  unknown ones are ignored (OAuth 2.1 §3.2);
- answer with `Cache-Control: no-store` and `Pragma: no-cache`;
- answer errors as `{ "error": "invalid_grant", "error_description": "…" }`
  (RFC 6749 §5.2), `400` unless said otherwise;
- refuse a `client_secret` (`400 invalid_client`) and an
  `Authorization: Basic` header (`401 invalid_client` with
  `WWW-Authenticate: Basic realm="pley"`): Pley's apps have no secrets;
- take `resource` only for this server (else `invalid_target`);
- answer `invalid_request` without a `grant_type`, `unsupported_grant_type`
  for an unknown one;
- have no rate limit (codes and tokens carry 256 random bits); TVs have
  [their own limits](#tvs).

A token answer:

```json
{
  "access_token": "pley_at_…",
  "token_type": "Bearer",
  "expires_in": 3600,
  "scope": "library play",
  "refresh_token": "pley_rt_…"
}
```

`scope` is everything the app was allowed. `refresh_token` comes only for
apps whose description lists the `refresh_token` grant type (Pley's own
apps do).

**`grant_type=authorization_code`**: `code`, `client_id` and
`code_verifier` (43 to 128 of `A-Z a-z 0-9 . _ ~ -`). `redirect_uri`, when
sent, must be the one the code was sent to. A code works for 60 s. A
profile has at most 10 codes waiting (a new one drops its oldest) and 50
within a minute; the server keeps 1,000 at most. `invalid_grant` when:

- the code is unknown, expired or another app's, the return address differs
  or the verifier is wrong (the code stays usable);
- the code was exchanged already ("the code was used already"): the access
  the first exchange got ends too, since someone else has the code;
- the account was disabled since, or apps by other publishers were turned
  off (for theirs);
- the browser session that allowed it signed out since, or the account's
  sign-ins ended (a new password, "Sign out everywhere"): "the person who
  allowed it signed out since".

**`grant_type=refresh_token`**: `refresh_token` and `client_id`; `scope`
optional (wider than allowed: `invalid_scope`; narrower: ignored).

- Each refresh token works once; the answer has a new one. Apps refresh one
  at a time: a token used again, however many refreshes later, or two
  refreshes with the same token at once, end the app's access
  (`invalid_grant`, "refresh token used again: the app's access is ended").
  A refresh answer lost on the network means signing in again.
- `invalid_grant` also for an unknown, expired or other app's refresh
  token; a disabled account (the profile's, or the one that allowed it);
  while the server [requires two-step sign-in](#two-step-sign-in), either
  account signing in with a password alone ("this server requires
  two-step sign-in: sign in again to set it up"); apps by other publishers
  turned off; a grant with 10,000 refresh tokens on record (which ends).
- Refreshes never read an app's description again.

**`grant_type=urn:pley:params:oauth:grant-type:switch-profile`**: Pley's own
apps allowed `profiles` move to another profile of the household. Send
`client_id`, `refresh_token`, `profile` (an id from
[`/api/oauth/profiles`](#profiles)) and `pin` when the profile has one.

- The answer is new tokens acting as that profile. The app keeps its scope
  and who allowed it; its old tokens stop at once. A refusal spends
  nothing.
- The rules are the web app's [switch](#auth), from the rights of the
  profile the app acts as, without the administrators' exemption.
- Switching to the current profile with its `pin` checks that PIN (a TV
  asking "Who's watching?" at start). It follows a switch's rules (the
  same wrong-PIN answers, throttle and lock, counted with switches to it);
  a right PIN answers as a switch does. Without a `pin`, or for a profile
  without one, it is `invalid_request` with no `reason`, which apps read as
  "this server can't check PINs".

| Answer | When |
|---|---|
| `unauthorized_client` | not one of Pley's own apps, or not allowed `profiles` |
| `invalid_request` | `profile` is missing or not an id, or it is the current profile and there is no PIN to check |
| `invalid_grant`, `"reason": "wrong_pin"` | the profile has a PIN, and it is missing or wrong |
| `invalid_grant`, `"reason": "pin_locked"` | its PIN stopped working after 100 wrong ones in a row |
| `invalid_grant`, `"reason": "throttled"` | too many wrong PINs for it lately (5 in 15 minutes) |
| `invalid_grant`, `"reason": "browser"` | it can see more than the app's profile and has no PIN: sign in to the app again and pick it in the browser (apps never see passwords) |
| `invalid_grant`, `"reason": "not_allowed"` | not a household profile, disabled, or can see more with neither PIN nor password |
| `invalid_grant` | the refresh token doesn't work, as for a refresh (a spent one ends the grant) |

**`POST /api/oauth/revoke`** (RFC 7009): `token` and `client_id`
(`invalid_request` without them; `token_type_hint` is ignored).

- An access or refresh token of that app, current, spent or expired, ends
  its whole grant: `200` with an empty body. The same for a token Pley
  doesn't know.
- Another app's token: `400 unauthorized_client`, and it stays (§2.1).
- A database failure: `503 temporarily_unavailable` with `Retry-After`. The
  token still works; the app tries again later (§2.2.1).
- `POST /api/auth/logout` with an app's token ends its grant too.

### TVs

TVs and other devices without a usable browser sign in with a code they
show (OAuth 2.0 Device Authorization Grant, RFC 8628). Only Pley's own apps
may: the Android app (also on Android TV and Fire TV), Pley for Kodi, Pley
for Roku, and the Samsung and LG apps.

1. The TV calls **`POST /api/oauth/device`** with `client_id` and an
   optional `scope` (as at `/oauth/authorize`: none means `library`):

   ```json
   {
     "device_code": "…",
     "user_code": "BCDF-GHJK",
     "verification_uri": "http://pley.local:7539/link",
     "verification_uri_complete": "http://pley.local:7539/link?code=BCDFGHJK",
     "expires_in": 600,
     "interval": 5
   }
   ```

   - The TV keeps `device_code` (256 random bits) to itself, and shows
     `user_code` and `verification_uri`, or a QR code of
     `verification_uri_complete`.
   - User codes are 8 letters from `BCDFGHJKLMNPQRSTVWXZ` (RFC 8628 §6.1),
     shown as `BCDF-GHJK`.
   - Errors: `invalid_client` for an app that isn't Pley's own;
     `unauthorized_client` for one of Pley's own that doesn't sign in this
     way (the desktop app); `invalid_scope` as at `/oauth/authorize`;
     `429 temporarily_unavailable` past the limits below.

   A TV that can't draw a QR code shows
   **`GET /api/oauth/device/qr?code=BCDF-GHJK`**: the address as a PNG
   (black on white, 8 pixels a module, a quiet zone of 4 modules,
   `Cache-Control: no-store`). It looks nothing up, so anyone may ask; `400`
   for something that isn't a code.
2. The person opens **`/link`** (a web app page) on a phone or computer,
   signs in if needed (setting up two-step sign-in first when the server
   [requires it](#two-step-sign-in) and they don't use it yet), and types
   the code (any case, with or without the dash and spaces; from the QR
   code it comes filled in). The page shows the
   same consent as `/oauth/authorize`, with what the TV said it is and where
   it asked from. Nothing goes back through the browser.
3. Meanwhile the TV asks the token endpoint every `interval` seconds with
   **`grant_type=urn:ietf:params:oauth:grant-type:device_code`**,
   `device_code` and `client_id`:

   | Answer | When |
   |---|---|
   | `authorization_pending` | nobody has answered yet |
   | `slow_down` | it asked sooner than its interval, which is now 5 seconds longer for good |
   | tokens | the person allowed it: as for a code, acting as their profile; once |
   | `access_denied` | the person refused; or their sign-ins ended before the TV had its tokens (a new password, "Sign out everywhere", disabling, deletion); or the browser session that allowed it signed out since; once |
   | `expired_token` | 10 minutes went by; once |
   | `invalid_grant` | an unknown device code, another app's, one already answered, or a disabled account |

**Limits.** At most 20 sign-ins in progress from one address (an IPv6 /64
counts as one), 50 from one IPv6 /56, and 1,000 in all; nothing in progress
is dropped to make room. The address is the connection's or, behind a
reverse proxy on the same computer or network, the client it names last in
`X-Forwarded-For`. Past 10 wrong codes from one person, or 300 from
everyone, in 10 minutes, no code is looked up (`429`).

### Lifetimes and limits

| What | Lasts |
|---|---|
| Access token | 1 hour |
| Refresh tokens | rotated on every use; the app's access ends 90 days after its last token request |
| Apps by other publishers | asked about again a year after they were allowed |
| Code | 60 seconds, once |
| A request on the consent page | 10 minutes (a TV's: no longer than the TV's own) |
| A TV's code | 10 minutes, once |

- An access token never outlasts its grant. A grant keeps its 10 newest
  access tokens (older ones stop working) and ends with 10,000 refresh
  tokens on record.
- A profile has at most 100 apps signed in; past that, the least recently
  used is signed out.
- Apps are signed out when they or the person sign them out
  ([Signed-in apps](#signed-in-apps)), and by a new password, "Sign out
  everywhere", `pley reset-password`, deleting the account, and requiring
  two-step sign-in of an account without it. That covers
  every app acting as the account or allowed by it, and the apps it was
  signing in (codes not exchanged yet, allowed TVs that haven't fetched
  their answer).
- After `pley reset-password`, a running server honors app tokens it checked
  in the last minute for up to a minute, as it does sessions.
- Apps of a disabled account, or allowed by one, don't work while it is
  disabled. Turning apps by other publishers off signs all of them out.

### Scopes

| Scope | Allows |
|---|---|
| `library` | see the profile's libraries, playlists and watch history |
| `play` | play titles; keep the profile's place, watched marks, ratings, track choices and library order |
| `playlists` | make, change and delete the profile's playlists |
| `profiles` | switch to the household's other profiles. Pley's own apps only, since a profile's PIN is typed into the app |
| `requests` | ask for movies and shows the libraries don't have ([Requests](#requests)), and follow the profile's requests |
| `approvals` | approve and decline what people ask for, where the account is an administrator. Pley's own apps only; it does nothing on other accounts |

An app's token opens only the routes whose **Who** is `any app` or one of
its scopes (`HEAD` as `GET`). Every other route answers
`403 insufficient_scope`: administration and sign-in settings among them.
Name, picture (`PATCH /api/me`), password, PIN, two-step sign-in, devices,
apps, activity, switching by `/api/auth/switch` and all administration stay
in the web app.

What apps see:

- An app's token never has administrator rights. On an administrator's
  account it sees every library without an age limit, as the account does,
  and administers nothing: history, sessions and collections answer it as
  they would anyone else. The one exception: on an administrator's account,
  one of Pley's own apps allowed `approvals` manages requests (approves and
  declines them, marks them as added by hand, sees and deletes everyone's,
  and hears the managers' request events; see [Requests](#requests)).
- Libraries' `paths` are empty, and files' `path` is the file name only.
- `/api/users/{id}/avatar` shows only the app's own profile, its
  household, and, to an app allowed `requests` while requests are on, the
  people named in the requests it may read: who decided its own, and for a
  manager who made or decided any.
- When the transcoder can't read a file, its words (which name the file)
  are replaced by Pley's own: "this title's file can't be read right now",
  and "the conversion failed" for a session's `last_error`.
- No address Pley hands out carries a token: players send the header with
  every request, HLS playlists and segments included (see
  [Keyed URLs](#keyed-urls) for players that can't).
- The event stream ends when its token expires or the app is signed out;
  the app connects again with a new token. It carries request events only
  with `requests` ([Request events](#request-events)).

### Signed-in apps

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/me/apps` | `user` | `AppGrant[]`: apps acting as you or allowed by you, most recently used first |
| DELETE | `/api/me/apps/{id}` | `user` | `204`, signs that app out; `404` unless it acts as you or you allowed it |

Administrators see and sign out other users' apps under
[Users](#users-admin).

```ts
interface AppGrant {
  id: Id;
  client_id: string;
  name: string | null;              // Pley's own apps: their name; others: what the app calls itself (unchecked)
  first_party: boolean;             // one of Pley's own apps
  host: string;                     // the client ID's host
  scopes: string[];
  profile: { id: Id; name: string };     // the profile it acts as
  approved_by: { id: Id; name: string };
  device: string | null;            // of its last token request: "Chrome on Windows". Pley's Android, Kodi,
                                    // Roku, Samsung and LG apps: their platform, e.g. "Android 15",
                                    // "Kodi 21.2; Windows", "Tizen 7.0", "webOS 23"
  ip: string | null;
  created_at: Timestamp;
  last_used_at: Timestamp;
}
```

### Profiles

`GET /api/oauth/profiles` (Who: `profiles`) lists the household's enabled
profiles when the app's profile is in the household, else nothing:
`{ id, name, avatar_url, avatar_color, current, needs }[]`.

- `current`: the profile the app acts as.
- `needs`: what switching to it takes, by the rules above: `none`, `pin`,
  `password` (in the browser) or `locked`. A profile that has to set up
  two-step sign-in, which the server requires, needs `password`: picked in
  the browser, its password sets it up there.
- The current profile's `needs` is `none`: the app acts as it already.
  Pley's TV apps asking "Who's watching?" as they start ask its PIN when
  [`GET /api/auth/me`](#auth) says `has_pin`, and check it with the
  [switch-profile grant](#tokens).
- Usernames, roles, libraries, age limits and sign-in settings are left
  out.

### Pley's own apps

| App | Client ID | Return address |
|---|---|---|
| Pley for Android | `https://pley-media-server.github.io/pley/oauth/android.json` | `app.pley.android:/oauth/callback` |
| Pley for desktop | `https://pley-media-server.github.io/pley/oauth/desktop.json` | `http://127.0.0.1/oauth/callback`, `http://[::1]/oauth/callback` (any port) |
| Pley for Kodi | `https://pley-media-server.github.io/pley/oauth/kodi.json` | none: a code only |
| Pley for Roku | `https://pley-media-server.github.io/pley/oauth/roku.json` | none: a code only |
| Pley for Samsung TV | `https://pley-media-server.github.io/pley/oauth/tizen.json` | none: a code only |
| Pley for LG TV | `https://pley-media-server.github.io/pley/oauth/webos.json` | none: a code only |

- Their descriptions are built into the server and never fetched, so they
  sign in to servers without internet access.
- All get refresh tokens and may ask for every scope, but the desktop app
  for `requests` and `approvals`: its window shows the server's own pages,
  signed in as the web app is.
- The Android app may also sign in with a code, on a TV ([TVs](#tvs)). The
  Kodi, Roku, Samsung and LG apps sign in only that way: with no return
  address, `/oauth/authorize` never sends them a code.
- The Samsung app can't set its own User-Agent: the TV's web engine names
  the TV ("Samsung TV"; for an app's grant its system, "Tizen 7.0"). The LG
  app says `pleyLG/<version> (webOS <n>; <model>)`; a TV still on the
  earlier LG web app is named by its web engine ("LG TV", "webOS 23").
- The consent page marks them "Pley's own app". No other app may use a
  client ID on their host.

### Apps by other publishers

Off until an administrator turns them on (**Settings → Server → Apps**,
`third_party_apps`). Then anyone signed in may allow one for their own
profile. Turning it off signs them all out; codes and refreshes check it
too.

**Client ID.** An https address of at most 512 characters, in its standard
form (lower-case host, punycode, no default port, no dot segments, no stray
spaces), on a domain name with a dot (not an IP address, `localhost` or
`*.localhost`), with a path other than `/`, and with no port, user name,
query or fragment. It may not be on the host of Pley's own apps or on this
server's.

**Description.** A JSON object of at most 5,120 bytes, no field given twice:

| Field | Rule |
|---|---|
| `client_id` | the address it was read from, exactly |
| `redirect_uris` | 1 to 10 return addresses of at most 512 characters, each in its standard form and one of: https on exactly the client ID's host, without a port; a private-use scheme that is the client ID's host reversed (`apps.example.com` → `com.example.apps`), written `scheme:/path`; `http://127.0.0.1/…` or `http://[::1]/…`. Never `localhost`, a fragment or a user name |
| `client_name` | optional: at most 64 characters, shown without control and direction characters, as something Pley can't check |
| `grant_types` | optional (default `["authorization_code"]`): must list `authorization_code`; refresh tokens only with `refresh_token` |
| `response_types` | optional: must list `code` |
| `token_endpoint_auth_method` | optional: `none` |
| `scope` | optional: the most the app may ask for (never `profiles` or `approvals`) |

`client_secret` and `client_secret_expires_at` are refused. Other fields
(`logo_uri`, `client_uri`, `tos_uri`, `policy_uri`, `jwks`, `jwks_uri`, …)
are ignored and never fetched.

**Fetching.** Pley reads a description only when someone signed in opens
the consent page for the app, never for a TV:

- over https on port 443, from public addresses only (the resolver checks
  the address actually connected to), without a proxy, redirects, cookies or
  compression;
- 3 seconds to connect, 5 in all;
- the answer must be a `200` of JSON (`application/json` or
  `application/*+json`) of at most 5,120 bytes;
- at most 4 read at once (one per client ID), and 10 a minute per person
  (then `429`);
- valid descriptions are cached for their `Cache-Control: max-age`, between
  5 minutes and 24 hours (an hour without one; not at all with `no-store`
  or `no-cache`), at most 256. Failures are never cached;
- refreshes never read them: changes apply at the app's next sign-in.

The consent page names such an app by its client ID's host, then what it
calls itself, and says it isn't one of Pley's own. Logos aren't shown.

### The consent page's endpoints

Pley's own page uses these (Who: `page`); apps never do.

- They take the session cookie only, never `Authorization`, `X-Pley-Token`
  or `?token=`.
- A `POST` needs an `Origin` that is this server, and
  `Sec-Fetch-Site: same-origin` when the browser sends it. The `GET` takes
  `Sec-Fetch-Site` `same-origin` or `none`. Anything else: `403` ("this page
  must be opened from Pley itself").
- None of them answers CORS.

| Method | Path | Who | What |
|---|---|---|---|
| POST | `/api/oauth/requests` | `page` | `{ query }` (the page's query without `?`, at most 8 KiB) → a consent request |
| POST | `/api/oauth/device/verify` | `page` | `{ user_code }` (as typed) → a consent request for a TV |
| POST | `/api/oauth/requests/{id}` | `page` | `{ allow: boolean }` → `{ continue_url }`, or `{ done: true }` for a TV's request |
| GET | `/api/oauth/requests/{id}/continue` | `page` | `303` to the app with the answer |

**`POST /api/oauth/requests`** answers
`{ status: "consent", id, expires_at, client: { id, host, name, first_party }, redirect: { kind: "web" | "app" | "loopback", to }, scopes }`,
or for a mistake that goes back to the app
`{ status: "error", error, message, back_url, client, redirect }`, where
`redirect` says where "Back to the app" goes. `400` for an app or return
address Pley can't accept, `429` with too many descriptions read, `503`
with too many sign-ins in progress.

**`POST /api/oauth/device/verify`** answers the consent as above, with
`redirect: { kind: "device", to: "BCDF-GHJK", device, address }`.

- `device`: what the TV said it is when it asked for the code, unchecked
  (Pley's apps give their platform in their User-Agent's brackets, "Kodi
  21.3; Windows"; anything else is named as the sign-ins list names it).
  `address`: where it asked from. Each `null` when unknown.
- `400` for something that can't be a code; `404` for a code no TV shows
  (it counts as a wrong one); `409` for one answered already or shown on
  another session's page; `429` past the wrong codes' limit; `503` with too
  many sign-ins in progress.
- A code shown on a page whose session ended since (signed out, or switched
  profile) goes to whoever types it next. Typed again in the same session,
  the earlier page can't answer any more.

**`POST /api/oauth/requests/{id}`**: a TV gets its answer when it next
asks. The same answer again gets the same reply; the other answer `409`.
`404` when the request is over (for a TV, also once its code expired).

**`GET /api/oauth/requests/{id}/continue`** repeats while the code works and
wasn't exchanged, and for a minute after a refusal or a mistake. After
that, a browser following the link is sent (`303`) to the web app's
`/oauth/over` page, which says the request is over; a script gets `404`.
Never for a TV's request.

A request belongs to the person and browser session that opened the page,
and lasts 10 minutes. Each person has at most 10 waiting (the oldest goes),
the server 1,000. Requests end with the account's sign-ins (a new password,
"Sign out everywhere", disabling, deletion).

## Access rules

Administrators see everything. Other users see every library, or only those
in `library_ids`. With `max_age` they see only movies and shows whose
content rating allows that age:

- US, UK, DE, FR, AU, CA and numeric certifications are understood;
- seasons and episodes go by their show;
- unrated titles are hidden;
- music is exempt.

Hidden libraries and items answer `404` everywhere: lists, hubs, search,
artwork, streams and the compatibility API.

## Libraries

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/libraries` | `library` | `Library[]` in the user's order (query `order=shared`: the shared order) |
| POST | `/api/libraries` | `admin` | `{ name, kind, paths: string[] }` → `201` `Library`; queues a scan |
| PUT | `/api/libraries/order` | `admin` | `{ ids: Id[] }` (every library, once) → `Library[]` in the new shared order |
| PUT | `/api/me/libraries/order` | `play` | `{ ids: Id[] }` (every library the user may open, once) → `Library[]` in the user's own new order |
| DELETE | `/api/me/libraries/order` | `play` | `Library[]` in the shared order, which the user sees again |
| GET | `/api/libraries/{id}` | `library` | `Library` |
| PATCH | `/api/libraries/{id}` | `admin` | `{ name?, paths? }` → `Library` |
| DELETE | `/api/libraries/{id}` | `admin` | `204` |
| POST | `/api/libraries/{id}/scan` | `admin` | `{ force?: boolean }` → `202`. `force` re-probes files and refreshes metadata |
| POST | `/api/libraries/scan` | `admin` | `202`: scans every library |
| GET | `/api/libraries/{id}/items` | `library` | `{ total, items: ItemSummary[] }`: top-level items, paged |
| GET | `/api/libraries/{id}/filters` | `library` | `{ genres: string[], years: number[] }` |
| GET | `/api/libraries/{id}/hubs` | `library` | `{ hubs: Hub[] }`: `continue_watching:{id}`, `recently_added:{id}`, `recently_released:{id}`, `top_rated:{id}`; music libraries only the second; empty hubs left out |

**Order.** Libraries have one shared order, chosen by administrators (new
libraries go last). A user may choose their own (Settings → Libraries).
Lists of libraries, Home's rows and the compatibility API's sections follow
the user's order when there is one (libraries it doesn't name, and new
ones, after it in the shared order), else the shared order. Users who may
open only some libraries see those, in that order.

- `PUT /api/libraries/order`: `400` when an id is listed twice, `409` when
  `ids` aren't the libraries there are (one was added or deleted since they
  were listed).
- `PUT /api/me/libraries/order`: `400` when an id is listed twice, `409`
  when `ids` aren't the libraries the user may open. It and `DELETE` send
  `libraries_changed` to the user's event streams.

**`GET /api/libraries/{id}/items`** query:

| Parameter | Values |
|---|---|
| `sort` | `title` (default), `year`, `added`, `rating`, `user_rating`, `release`, `duration`, `last_played`, `bitrate`; another value: `400` |
| `order` | `asc`, `desc`. Default: `desc` for `added`, `rating`, `user_rating`, `release`, `last_played` and `bitrate`, else `asc` |
| `watched` | `true`, `false`; `true` matches shows whose every episode is watched |
| `genre` | a genre name |
| `year` | `yyyy` |
| `offset`, `limit` | paging |
| `summaries` | `1`: with synopses ([`summaries=1`](#types)) |

Items leave out `backdrop_url` and `thumb_url` (grids show posters);
sorted by `bitrate`, they carry `media`. Answers carry an `ETag` and
`Cache-Control: private, no-cache`;
`If-None-Match` with the same tag answers `304` while the library, the
user's watch state and their access are unchanged.

## Items

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/items/{id}` | `library` | `ItemDetail` |
| GET | `/api/items/{id}/children` | `library` | `{ items: ItemSummary[] }` by index: a show's seasons, a season's episodes, an artist's albums, an album's tracks |
| GET | `/api/items/{id}/on-deck` | `library` | `{ item: ItemSummary \| null }`: the episode a show's or season's *Play* starts |
| GET | `/api/items/{id}/next` | `library` | `{ item: ItemSummary \| null }`: the episode or track after this one (autoplay) |
| GET | `/api/items/{id}/related?limit=` | `library` | `{ items: ItemSummary[] }`: "More Like This" (default 20, max 60) |
| GET | `/api/items/{id}/images/{poster\|backdrop\|thumb}` | `library` | `image/jpeg`; query `w=<px>` (optional resize, bucketed), `v=` (cache buster, ignored) |
| POST | `/api/items/{id}/prefetch` | `play` | `202` at once: read ahead what *Play* reads first |
| POST | `/api/items/{id}/progress` | `play` | `{ position_ms, duration_ms?, state, session_id?, age_ms? }` → `204` |
| POST | `/api/items/{id}/watched` | `play` | `204`; containers: every leaf |
| POST | `/api/items/{id}/unwatched` | `play` | `204` |
| PUT | `/api/items/{id}/rating` | `play` | `{ rating: number \| null }` → `204`: the user's own rating (`user_state.rating`); `400` outside 0-10 |
| PUT | `/api/items/{id}/streams` | `play` | `{ file_id?, audio_index?, subtitle_index? }` → `204`: remembers track choices |
| PATCH | `/api/items/{id}` | `admin` | `{ title?, sort_title?, original_title?, year?, summary?, tagline?, studio?, content_rating?, release_date?, genres? }` → `ItemDetail` |
| GET | `/api/items/{id}/matches?title=&year=` | `admin` | TMDB candidates `{ tmdb_id, title, original_title, year, overview, poster_url }[]` for movies and shows (needs a TMDB key) |
| POST | `/api/items/{id}/match` | `admin` | `{ tmdb_id }` → `ItemDetail`, re-matched and refreshed |
| GET | `/api/items/{id}/artwork/{poster\|backdrop\|thumb}` | `admin` | TMDB images `{ url, preview_url, width, height, language, votes }[]` |
| PUT | `/api/items/{id}/artwork/{kind}` | `admin` | `{ url }` (a TMDB image) → `ItemDetail`; downloaded and locked |
| POST | `/api/items/{id}/artwork/{kind}/upload` | `admin` | raw image bytes (≤ 25 MB) → `ItemDetail`; stored as JPEG and locked |
| DELETE | `/api/items/{id}/artwork/{kind}` | `admin` | `204`; back to local or downloaded artwork |
| POST | `/api/items/{id}/refresh` | `admin` | `202`: re-fetch metadata from the item's source |
| POST | `/api/items/{id}/analyze` | `admin` | `202`: find the intro and credits again |
| GET | `/api/items/{id}/subtitles/search` | `play` | `{ results: SubtitleResult[] }` ([Added subtitles](#added-subtitles)) |
| POST | `/api/items/{id}/subtitles` | `play` | `201` `SubtitleTrack` ([Added subtitles](#added-subtitles)) |
| DELETE | `/api/items/{id}/subtitles/{index}` | `play` | `204`; an added track of the title, by an administrator or whoever added it |

- **`GET /api/items/{id}`** of a movie or episode also starts reading its
  video ahead from where *Play* starts for the user, as `prefetch` does,
  without waiting.
- **`prefetch`**: *Play* is about to be pressed (a card's Play button has
  the pointer or focus). The transcoder reads ahead the parts a playback
  from the user's resume point reads first (`POST /v1/prefetch` in
  [TRANSCODER.md](TRANSCODER.md)), so they come from a cache; worth up to
  seconds for files on a network share. Movies, episodes and extras; a show
  or season: its on-deck episode; other kinds: nothing. At most once every
  two minutes per file.
- **`on-deck`**: the episode in progress, else next up (as in
  [Continue Watching](#continue-watching)), else the first unwatched, else
  the first.
- **`related`**: movies and shows of the same library sharing collections,
  directors, creators, top-billed cast, genres and a period, best first.
  Empty for other kinds.
- **`images`**: long-lived cache headers. Falls back to the parent's
  artwork (season → show); `404` if none.
- **`progress`**: `state` is `playing` (the default), `paused`, `stopped`
  or `buffering`. Movies, episodes, tracks and extras only; other kinds
  answer `400`.
  - At 90 % or more the item is marked watched and the resume point
    cleared.
  - `age_ms`: how long ago a late report was made (a download played while
    the server was out of reach, up to 90 days). It changes nothing when
    the item's state changed since (played elsewhere, marked watched).
  - A report that changes what the user's devices show sends
    `user_state_changed` ([Watch state on other devices](#watch-state-on-other-devices)).
  - When to send it: see [Playback](#playback).
- **`watched`, `unwatched`** send `user_state_changed` to the user's
  streams, unless nothing changed.
- **`rating`**: 0-10, rounded to half stars; `null` clears it.
- **`streams`** remembers the choice for the whole show (episodes) or the
  movie, as language, title and forced flag, so every episode starts with
  its own matching tracks.
  - `file_id` defaults to the item's first file.
  - `subtitle_index` `null` or left out: off.
  - `audio_index` left out keeps the audio chosen before (only subtitles
    were picked); `null` remembers none, so each play chooses as for a
    title without a choice. Players send it only when the person picked an
    audio track.
- **`PATCH`**: a value edits and locks the field (refreshes and scans keep
  it); `null` unlocks it. `ItemDetail.locked_fields` lists locked fields.
- **`match`**: a show whose TV source is TVmaze or TheTVDB is found there by
  the chosen entry's IMDb and TheTVDB ids.
- **`refresh`** sources: movies, TMDB; shows, seasons and episodes, the TV
  source; artists, albums and tracks, MusicBrainz. `400` when that source
  is off or lacks its key.
- **`analyze`**: a movie's chapters, or every episode of the season of an
  episode, a season or a show. `400` for other kinds.

### Added subtitles

Anyone who can see a movie, episode or extra may add subtitles to one of its
files: a file uploaded from the web app, or one found on OpenSubtitles.

- Pley keeps them in its data folder (`subtitles/`, see
  [STORAGE.md](STORAGE.md)), never next to the media.
- Clients get them like a subtitle file next to the video: an external text
  track (`external: true`, `text_based: true`, `added: true`) in
  `PlaybackInfo.subtitles` and the file's `MediaStream`s, with a WebVTT
  `url`. They are chosen by `subtitle_index` in `POST /api/playback` and
  `PUT /api/items/{id}/streams`, listed by the compatibility API
  (`/library/streams/{id}` serves them as they are), and shown like any text
  track (a transcode that burns one in is given its file).
- Track indexes: a file's own tracks are below 1000, subtitle files next to
  the video 1000 to 8999 (in path order; more aren't listed), added ones
  9000 to 9999.
- An added track's index is unique among the files of its title, counting
  on from the highest (the lowest free one once 9999 is taken), so
  `DELETE /api/items/{id}/subtitles/{index}` needs no file. Indexes are
  never reused.

**Search**: `GET /api/items/{id}/subtitles/search?file_id=&language=`

- `file_id`: the title's file (default: its first).
- `language`: OpenSubtitles' or Pley's language codes, comma separated
  (`en,fr`, `eng`, `pt-BR`; Portuguese and Chinese ask for both of their
  regions). Default: the user's subtitle language from their track
  choices, else their audio language, else `en`. `400` for anything that
  isn't a language code, or more than 20.
- OpenSubtitles is asked by the file's OpenSubtitles hash and the film's
  IMDb and TMDB ids (an episode: its show's, with its season and number),
  else by title and year.
- Results made for the file come first, then the most downloaded.
  Subtitles split over several files (CDs) are left out.

```ts
interface SubtitleResult {
  id: string;                 // opaque, passed back to POST: "opensubtitles:1923552:en"
                              // (plus ":hi", ":forced", ":srt" when they apply)
  provider: "opensubtitles";
  language: string;           // OpenSubtitles' code: "en", "pt-BR"
  name: string;               // the release it was made for, or its file's name
  format: "srt" | "vtt" | "ass" | "ssa"; // what it is added as (other formats come as SubRip)
  hearing_impaired: boolean;
  forced: boolean;            // foreign parts only
  downloads: number;
  matched: "hash" | "title";  // "hash": made for this very file
}
```

**Add**: `POST /api/items/{id}/subtitles` takes either:

- a search result: `application/json` `{ "result": "<id>", "file_id"?: n }`.
  It is downloaded from OpenSubtitles (with the account in the settings, if
  any, for its larger quota) and added in the result's language, named
  after its release;
- a file: `multipart/form-data` with `file` (`.srt`, `.vtt`, `.ass` or
  `.ssa`, at most 2 MiB), `language` (a code or English name: `en`, `eng`,
  `English`, `pt-BR`), and optionally `hearing_impaired`, `forced`
  (`true`/`false`) and `file_id`. Named after the file.

Either way the text is converted to UTF-8 (UTF-16 by its byte order mark,
else UTF-8 when valid, else Windows-1252) and checked to be the subtitles
it says (SRT cues; WebVTT with its header and cues; ASS/SSA with events; a
WebVTT file named `.srt` is taken as WebVTT). Its language is kept as ISO
639-2 where Pley knows it (`eng`). The answer is the new `SubtitleTrack`,
`deletable` for whoever added it.

| Status | When |
|---|---|
| `400` | not valid subtitles |
| `404` | a title the user can't see |
| `409` | the title has 1000 added tracks |
| `429` | adding a result when the day's OpenSubtitles downloads are used up (with OpenSubtitles' words: when the quota renews) |
| `503` | search or adding a result. `{ code: "unavailable", message, setup: false }` when the server has no OpenSubtitles API key: clients say "Subtitle search isn't set up on this server" (and point administrators to Settings → Server). `setup: true` when OpenSubtitles rejects the key or account, or can't be reached |

## Home & search

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/hubs/home` | `library` | `{ hubs: Hub[] }` |
| DELETE | `/api/continue-watching/{id}` | `play` | `204`: takes a title out of the user's Continue Watching |
| GET | `/api/search?q=&limit=` | `library` | `{ items: ItemSummary[], people: PersonSummary[], channels: LiveChannel[] }`: `items` up to `limit` (default 40, 1 to 200); `people` up to 12; `channels` up to 12 Live TV channels by name, number or what's on, for those Live TV is open to |

- **Home** hubs, empty ones left out: `continue_watching` (in progress and
  next up), then `my_list` ("My List", its 20 newest), then
  `recently_added:<library_id>` per library in the libraries' order.
- **Search**: items ranked by relevance; people whose name has words
  starting with each word of `q`, most credited first.

### Continue Watching

Continue Watching holds the movies and episodes the user is part way
through, plus the *next up* episode of each show they've watched lately.
Next up is also what the show's *Play* starts (`on-deck`) when no episode
is in progress.

Next up is the episode right after the one watched most recently, when
that one is unwatched. When it was seen before:

- **A rewatch goes on in order**: the episode watched most recently was a
  rewatch (seen twice or more), and either nothing after it is unwatched,
  or the episode before it was rewatched too, after anything later was
  seen. Next up is the episode right after it, seen or not, so going
  through a show again never skips episodes.
- **Otherwise** (a gap filled, or one favorite seen again): the first
  unwatched episode after it, where the user had got to.

A seen episode stopped part way stays out of the in-progress rows (apps
play a seen episode from its start), and can still be next up.

**`DELETE /api/continue-watching/{id}`** takes a movie, or for an episode
(in progress or next up) its whole show, out of the user's Continue
Watching. A show's or season's id does the same; other kinds answer `400`.

- It stays out of every Continue Watching (Home's, the libraries' hubs, the
  compatibility API's On Deck and `continueWatching` hubs) until the user
  plays that movie or any episode of that show again: a progress report or
  watched mark newer than the hide.
- Progress and watched marks are kept.
- The compatibility API's `PUT /actions/removeFromContinueWatching?ratingKey=`
  does the same.
- Either sends `user_state_changed` for the movie or show to the user's
  event streams, so their other devices fetch Home again.

### My List

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/watchlist` | `library` | `{ items: ItemSummary[] }`, newest added first |
| PUT | `/api/watchlist/{id}` | `play` | `204`: adds a movie or show (already there: no change). Other kinds: `400` |
| DELETE | `/api/watchlist/{id}` | `play` | `204`: takes it out (not there: no change) |

- Each user has their own list. Titles the user can't see (library access,
  age limit) are left out, and come back when they can again.
- A movie leaves the list once watched (played to the end, or marked
  watched here or through the compatibility API). A show stays until
  removed.
- `ItemDetail.in_watchlist` says whether a title is on it.
- Every change, a movie leaving by itself included, sends
  `watchlist_changed` to the user's own event streams.

## Cast and crew

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/people/{id}` | `library` | `PersonDetail`; `404` when the person has no credits this user can see |
| GET | `/api/people/{id}/photo` | `library` | image bytes; query `w=<px>`, `v=`. `404` without a photo |

- Credits come with the rest of the metadata. TMDB: movies' cast and crew;
  shows' everyone in any episode, and the creators; episodes' guest stars,
  directors and writers. TVmaze or TheTVDB: shows' main cast and creators.
- A person is known by the id of the source that credited them, so the same
  actor from two sources is two people.
- People appear only where the user can see an item they are credited in.
- Photos are downloaded from the source (TMDB, TVmaze, TheTVDB) the first
  time.

## Collections

Groups of movies or shows of one library, shared by every user.

- *Automatic* collections mirror TMDB's movie collections: each matched
  movie is filed under its `belongs_to_collection` (with that collection's
  poster and backdrop), and collections left empty are dropped. One is
  listed once two of its movies are visible to the user.
- Administrators make their own and can edit either kind; an edited title
  is kept over TMDB's.
- The [access rules](#access-rules) apply: hidden items are left out, and a
  collection with nothing left to show answers `404` (empty manual
  collections are shown to administrators only).

```ts
interface Collection {
  id: Id;
  library_id: Id;
  title: string;
  summary: string | null;
  automatic: boolean;               // from TMDB
  sort: "release" | "title" | "custom";
  smart_filter: string | null;      // smart collections: the library filter that fills them
  item_count: number;               // items this user can see
  poster_url: string;               // own artwork, else the first item's that has some
  backdrop_url: string;
  created_at: Timestamp;
  updated_at: Timestamp;
}
```

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/libraries/{id}/collections` | `library` | `{ collections: Collection[] }` by title |
| POST | `/api/libraries/{id}/collections` | `admin` | `{ title, summary?, item_ids?: Id[], smart_filter? }` → `201` `Collection & { items: ItemSummary[] }`; `400` for music libraries |
| GET | `/api/collections/{id}` | `library` | `Collection & { items: ItemSummary[] }` in the collection's order |
| PATCH | `/api/collections/{id}` | `admin` | `{ title?, summary? (null clears), sort?, smart_filter? }` → `Collection & { items }` |
| DELETE | `/api/collections/{id}` | `admin` | `204` (an automatic one comes back when its movies are refreshed) |
| POST | `/api/collections/{id}/items` | `admin` | `{ item_ids: Id[] }` → `{ added: number }`. Only movies and shows of the same library; `400` if none could be added |
| DELETE | `/api/collections/{id}/items/{item_id}` | `admin` | `204` |
| PUT | `/api/collections/{id}/order` | `admin` | `{ item_ids: Id[] }` → `204`: these first in this order, the rest after; sets `sort` to `custom` |
| GET | `/api/collections/{id}/images/{poster\|backdrop}` | `library` | image bytes (`w`, `v` as for items), or a `307` to the item artwork it falls back to |
| GET | `/api/items/{id}/collections` | `library` | `{ collections: { id, title }[] }` the movie or show is in ("Part of …") |

**Smart collections** hold no items. `smart_filter` is a library listing in
the compatibility API's syntax: the query string of
`/library/sections/{id}/all`, such as `genre=Comedy&unwatched=1&sort=year:desc`
(see [COMPATIBILITY.md](COMPATIBILITY.md)). The collection shows what it
finds, in its order, as each user may see it. Its items can't be added,
removed or reordered (`400`); change the filter instead.

## Playlists

Ordered lists of a user's own, private to them (administrators included):
another user's playlist answers `404`.

- A playlist is for `video` (movies and episodes) or `audio` (tracks).
  Adding a show, season, album or artist adds its episodes or tracks in
  playing order. An item may be in a playlist more than once.
- Entries the user can no longer see are skipped in every answer but kept,
  so they come back if access is restored.
- At most 5,000 entries.
- The web player plays a video playlist in order (its *Next* is the next
  entry); music playlists go to the audio queue.

```ts
interface Playlist {
  id: Id;
  title: string;
  kind: "video" | "audio";
  item_count: number;               // visible entries
  duration_ms: number;
  artwork: string[];                // artwork urls of the first entries (≤ 4), for a mosaic
  smart_filter: string | null;      // smart playlists: the listing of library_id they play
  library_id: Id | null;
  created_at: Timestamp;
  updated_at: Timestamp;
}

interface PlaylistEntry {
  entry_id: Id;                     // identifies this entry (an item may appear twice)
  item: ItemSummary;
}
```

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/playlists` | `library` | `{ playlists: Playlist[] }` by title |
| POST | `/api/playlists` | `playlists` | `{ title, kind?, item_ids?: Id[] }` or `{ title, library_id, smart_filter }` → `201` `Playlist & { entries: PlaylistEntry[] }`. Without `kind`, the first visible item decides |
| GET | `/api/playlists/{id}` | `library` | `Playlist & { entries }` |
| PATCH | `/api/playlists/{id}` | `playlists` | `{ title?, smart_filter? }` → `Playlist & { entries }` |
| DELETE | `/api/playlists/{id}` | `playlists` | `204` |
| POST | `/api/playlists/{id}/items` | `playlists` | `{ item_ids: Id[] }` → `{ added: number }`; `400` when an item doesn't fit the playlist's kind |
| DELETE | `/api/playlists/{id}/items/{entry_id}` | `playlists` | `204` |
| PUT | `/api/playlists/{id}/order` | `playlists` | `{ entry_ids: Id[] }` → `Playlist & { entries }`: these first in this order, the rest after |

**Smart playlists** play what a listing of their library finds
(`smart_filter`, the syntax of smart collections), shows and albums as their
episodes and tracks, in the filter's order, at most 5,000 entries, as the
owner may see them. Their entries are keyed by item and can't be added,
removed or reordered (`400`).

## Playback

| Method | Path | Who | What |
|---|---|---|---|
| POST | `/api/playback` | `play` | play request (below) → `PlaybackInfo` |
| GET | `/api/playback/{session_id}` | `play` | `{ session_id, method, transcode: TranscodeInfo \| null }`: the player's details (`transcode` null for a direct play) |
| DELETE | `/api/playback/{session_id}` | `play` | `204`; stops any transcode |
| PUT | `/api/playback/{session_id}/bitrate` | `play` | `{ max_bitrate }` (kbps) → `{ bitrate }`: change a converted video's bitrate as it plays |
| GET | `/api/playback/network?client_id=` | `play` | `NetworkAnswer` ([Network memory and speed test](#network-memory-and-speed-test)) |
| PUT | `/api/playback/network` | `play` | `{ client_id, probe_kbps, probe_complete? }` or `{ client_id, ceiling_kbps }` → `NetworkAnswer` |
| GET | `/api/playback/speedtest?bytes=` | `play` | that many bytes to time (`application/octet-stream`) |
| GET | `/api/playback/{session_id}/index.m3u8` | `play` | HLS VOD playlist (`application/vnd.apple.mpegurl`) |
| GET | `/api/playback/{session_id}/master.m3u8` | `play` | multivariant playlist naming the codecs ([Dolby Vision remuxes](#dolby-vision-remuxes)): `index.m3u8`, then a transcode's smaller sizes `v1.m3u8`… ([Several sizes](#several-sizes)) |
| GET | `/api/playback/{session_id}/{n}.ts` | `play` | MPEG-TS segment |
| GET | `/api/playback/{session_id}/v{k}.m3u8`, `…/v{k}-{n}.ts` | `play` | a smaller size's playlist and segments, laid out as `index.m3u8`'s |
| GET | `/api/media/{key}/{file}` | `public` (the key) | a keyed session's media ([Keyed URLs](#keyed-urls)) |
| GET | `/api/files/{id}/stream` | `play` | the raw file, with `Range` (direct play) |
| GET | `/api/files/{id}/subtitles/{index}.vtt?offset_ms=` | `play` | WebVTT of a text subtitle track |
| GET | `/api/files/{id}/subtitles/{index}.mks` | `play` | a picture subtitle track in a Matroska file of its own (`video/x-matroska`) |
| GET | `/api/files/{id}/subtitles/{index}.sup` | `play` | a PGS track as a `.sup` file (Kodi's) |
| GET | `/api/files/{id}/trickplay` | `play` | seek-bar previews: `{ id, interval_ms, width, height, columns, rows, count, sheets, version }` |
| GET | `/api/files/{id}/trickplay/{n}.jpg?v={id}` | `play` | preview sprite sheet `n` |
| GET | `/api/files/{id}/chapters/{n}.jpg` | `play` | picture of chapter `n` (0-based) |

- **`GET /api/playback/{session_id}`**: `403` for someone else's session
  (administrators see any); `404` "playback session expired" once it is
  gone; `410` when it was stopped.
- **`bitrate`**: for `live_bitrate` sessions. The video goes on at another
  bitrate from the segments its player hasn't fetched yet, at the same size
  and in the same playlist, so the player plays on without a new stream
  ([QUALITY.md](QUALITY.md#players-that-go-black-between-streams)). The
  answer is the bitrate it goes on at: within 1 Mbps and the session's own.
  A job the transcoder loses comes back at it. `400` for a direct play, a
  remux, or a transcoder too old.
- **`/api/media/{key}/{file}`**: `master.m3u8`, `index.m3u8` and `{n}.ts`
  of a stream, `file.{ext}` of a direct play, `previews.bif` of either.
  `HEAD` answers as `GET` without the body: for a segment, once it is made
  (the TV apps get a stream ready that way).
- **`.vtt`**: extracted or converted on demand, and cached.
  - `offset_ms` moves every cue that much later (earlier when negative,
    never before 0; at most ±600000), for players that can't move a track
    themselves (Roku's subtitle sync). `400` when it isn't a whole number.
  - An embedded track still being read out of its file comes partial: the
    cues found so far, with `X-Pley-Complete: 0` (not cached). Check that
    header and ask again for the rest. A Matroska file's track usually comes
    whole at once, read through its Cues when they list it.
  - Requests for a track being converted share that conversion, which goes
    on (to the cache) when they stop waiting. One that failed is tried again
    on the next request.
- **`.mks`**: PGS, VobSub (with its palette) or DVB, from the file or a file
  next to it, copied as it is. For players that show picture subtitles next
  to an HLS stream instead of having them burned in
  (`picture_subtitles_sideload`). Times are the original's from the start
  of the file, so it lines up with the playback position (also for an HLS
  stream started at a seek point; see `pts_offset_ms`). Made once per file
  version by the transcoder, cached, served with `Range`. `404` for a text
  track or anything but subtitles; `503` with a transcoder too old to make
  it.
  - A Matroska file whose Cues list the track's blocks is read in seconds
    and the track comes whole. Otherwise the transcoder reads the whole
    file (minutes for a big one on a network share): meanwhile the answer
    is what is copied so far, up to its last whole cluster, with
    `X-Pley-Complete: 0` and `Cache-Control: no-store`; a `Range` of it says
    `*` for the length. Every answer is a Matroska file players read (the
    track's header at least). Ask again for the rest, as for `.vtt`.
  - The copy starts as a playback starts whose player draws the track
    (`picture_subtitles_sideload`, the track picked or the profile's
    choice), rather than when it asks, after the playback's own start.
- **`.sup`**: a PGS track (in the file or next to it) as a `.sup` file, for
  players that read those beside a stream but no Matroska (Kodi): a `.sup`
  next to the media as it is, an embedded track made from its `.mks`, with
  an empty display set at 0 first (a reader that takes a file's first time
  for its start starts it where the video does). The answer waits up to
  8 s for the whole track, then has what is copied so far
  (`X-Pley-Complete: 0`). `404` for anything but PGS.
- **`trickplay`**: `404` while previews are being made (asking, or opening
  the item, queues them). None start during a playback's first 30 s nor
  while anything is transcoded: a playback that starts puts off every file
  being made, and a transcode all but one (which the transcoder pauses);
  they are made again later, in the order they were in, and never count as
  failed. A file the transcoder couldn't reach (a share not mounted) is
  asked for again the next time it is opened or scanned. Where the
  transcoder makes several at once, a title someone opens is made beside
  the libraries' previews rather than after them, unless a library file
  is read from the same disk, which waits for it. Where the transcoder has
  fewer than 4 cores (a container's limit counted), or the title's storage
  can't be told, it is made alone: every library file being made waits.
  Those of whole libraries wait for [library work](#library-work)'s
  hours, one file at a time or more ([How many at once](#how-many-at-once)).
  Previews an earlier version made (`version` lower than what the
  transcoder makes now) are made again as library work, after the files
  that have none and only in the hours for the rest of the libraries;
  they serve until then.
- **Sprite sheets**: thumbnail `k` is at column `k % columns`, row
  `k / columns` of sheet `k / (columns × rows)`, and shows the video at
  `k × interval_ms`.
- **Chapter pictures** are cut from the seek previews (a few seconds into
  the chapter); `404` until the file has previews.

### Play request

`POST /api/playback` body:

```ts
{
  item_id: Id;
  file_id?: Id;                   // default: the item's first file
  audio_index?: number | null;    // ffprobe stream index; left out: the server picks (below).
                                  // Players name one only when the person picked it
  subtitle_index?: number | null; // only for picture subtitles to burn in (forces a transcode);
                                  // players that draw them (`picture_subtitles_sideload`) leave it out
  max_bitrate?: number | null;    // kbps; null or absent = original quality
  max_height?: number | null;     // 1080, 720, 480
  start_ms?: number;              // resume position; transcodes start producing there
  mode?: "auto" | "direct" | "remux" | "transcode"; // default "auto"
  client_id?: string;             // identifies the player (browser tab, device)
  keep_previous?: boolean;        // the client's last session plays on: don't end it (the player ends it
                                  // as it moves over)
  keyed_urls?: boolean;           // the player can't send the Authorization header: its addresses carry a
                                  // key (Keyed URLs)
  caps?: {                        // what the client decodes natively (left out: nothing)
    containers: string[];         // ["mp4","m4v","mov","webm","mp3","m4a","flac","ogg","wav"]
    video_codecs: string[];       // ["h264","hevc","vp9","av1"]
    audio_codecs: string[];       // ["aac","mp3","opus","flac","vorbis"]
    hdr?: boolean;                // shows HDR10 and HLG without tone mapping
    dolby_vision?: boolean;       // decodes Dolby Vision (needed for profile 5)
    // Version 2 (the Android app); every field optional. Codec names are ffprobe's.
    caps_version?: 2;
    video?: {                     // every video decoder; a stream plays when one entry takes it
      codec: string;              // "h264", "hevc", "vp9", "av1"…
      hardware?: boolean;
      profiles?: string[];        // as ffprobe names them: "Main", "Main 10", "High", "Profile 2"…
      max_width?: number;         // limits left out aren't checked
      max_height?: number;
      max_fps?: number;
      max_bit_depth?: number;
      max_level?: number;         // numbered as a stream's `level` (HEVC 5.1 = 153, H.264 5.1 = 51)
    }[];
    hdr_types?: ("hdr10" | "hdr10plus" | "hlg" | "dolby_vision")[]; // what the screen shows, a decoder feeding it
    dolby_vision_profiles?: { profile: number; max_level?: number }[]; // decoded; used only with
                                  // `dolby_vision` in hdr_types, else they say the screen is what lacks it
    dolby_vision_hdr10plus?: boolean; // shows Dolby Vision + HDR10+ streams as Dolby Vision (below)
    audio_decode?: string[];      // decoded on the device
    audio_passthrough?: string[]; // sent as they are to a receiver: "ac3", "eac3", "truehd", "dts"
    audio_channels?: number;      // the most channels the output takes: 2, 6, 8
    subtitle_codecs?: string[];   // shown by the player itself, pictures included ("hdmv_pgs_subtitle")
    picture_subtitles_sideload?: boolean; // shows picture subtitles from their `.mks` next to HLS too
    audio_track_switching?: boolean; // switches a file's audio tracks itself
    hls?: { master_playlist?: boolean; segments?: string[]; // "ts", "fmp4"
            variants?: boolean }; // moves between a stream's sizes itself: transcodes come in several
                                  // (any caps version)
    display?: { width: number; height: number; refresh_rates?: number[] };
    device?: { form?: "phone" | "tablet" | "tv" | "unknown"; tier?: "low" | "mid" | "high";
               fire_tv?: boolean; model?: string };
  };
}
```

**How it plays** (`mode: "auto"`, version 1 caps; for `caps_version: 2` see
[Version 2](#version-2)):

- **direct** when the container and codecs are supported, the dynamic range
  can be shown, the requested audio track is the default one, nothing must
  be burned in and the bitrate fits;
- else **remux** (HLS, video stream copied) or **transcode** (HLS,
  H.264/AAC).

Rules:

- HDR10 and HLG need `caps.hdr`. Dolby Vision profile 5 needs
  `caps.dolby_vision`; profiles 7 and 8 play directly with either, since
  their base layer is HDR10 (8.1) or HLG (8.4). Otherwise the video is
  tone-mapped to SDR.
- Profile 5 plays only as it is: version 1 remuxes never name Dolby Vision,
  and unnamed it shows the wrong colors. So a profile 5 file that needs
  repackaging (Safari, and Matroska) is converted instead.
- Converted audio is stereo AAC. AAC and MP3 the player decodes are copied.
- `mode: "direct"` always returns the original file (for a browser that
  plays more than it reports; other audio tracks are then unavailable, and
  so are picture subtitles unless the player draws them itself).
  `"remux"` and `"transcode"` force HLS; `"transcode"` always re-encodes
  the video.
- A session started with a `client_id` ends that user's previous sessions
  on the same client (a quality or audio switch, the next episode) before
  the transcode limit is checked.
- If the transcoder loses a job (restart, expiry), the server recreates it
  with the same segment layout. When it can't, segments answer `404
  not_found` ("playback session expired"): start a new session at the
  current position.
- The HLS playlist covers the whole file (VOD), so seeking works natively;
  the server restarts FFmpeg at the right segment on seeks.
- Audio-only items (tracks) use the same endpoint and get a direct URL or an
  audio-only HLS stream.

**Reporting.** The player sends `POST /api/items/{id}/progress` (with
`session_id`) every ~10 s while playing, on pause, every ~30 s while
paused, and with `state: "stopped"` when closing, then
`DELETE /api/playback/{session_id}`.

- A session not heard from for 90 s has lost its player: it leaves "Now
  playing" and its conversion stops (segments then answer `404`, "playback
  session expired"). After 15 minutes it is forgotten.
- A downloaded file played as it is reports without a session. Reports made
  while the server was out of reach are sent once it is back, each with its
  `age_ms`.

### The audio track

Without `audio_index` the server picks, and `PlaybackInfo.audio_index` says
which track plays (a player that switches tracks itself plays that one in
direct play).

**Language**: the one the profile chose for the show or movie (a track it
picked), else its audio language, else the language of the file's default
track. Tracks of undetermined (`und`) or missing language are never swapped
for one another.

**Track**: of that language's tracks, the main sound (never a commentary or
an audio description, unless picked), then the one this player plays best:

1. as it is (decoded or passed through) rather than converted. A track that
   needs a stream counts as it is only when the stream copies it (AAC and
   MP3 for version 1 players and browsers; AC-3 and E-AC-3 too for version
   2). So an old Fire TV app that can't play a file's TrueHD default gets
   its stereo AAC copied, not its AC-3 5.1 converted to stereo;
2. a picked track's title (a stereo mix picked on purpose stays);
3. with the file as it is: the file's default, or any track for a player
   that switches tracks itself (`audio_track_switching`); other tracks need
   a stream;
4. as many channels as `audio_channels` says the output takes, mixed for no
   more than that (a stereo mix beats a 5.1 one folded down on stereo);
   lossless (TrueHD, DTS-HD MA, FLAC, PCM); Atmos or DTS:X when passed
   through; the bitrate. A track passed through counts its own channels,
   since the receiver decodes it: a 2.0 player passing AC-3 to a receiver
   gets the AC-3 5.1, not the stereo mix. Converted tracks count the
   channels and mix they come out with;
5. of converted tracks alike, the cheapest to convert: lossy before
   lossless, then fewer bits (Chrome's stereo AAC comes from a file's AC-3
   5.1, not its TrueHD 7.1);
6. the file's default, then the first.

A file whose video is converted anyway (a quality below the original,
which Automatic picks when the original doesn't play well, or a codec the
player lacks) plays no track as it is, so the track is chosen again for the
stream: one the stream copies beats the one the file would have played and
the stream would convert (a DTS default next to AC-3, on a box that decodes
both: the AC-3, copied).

Examples: a TV whose receiver takes TrueHD gets the TrueHD Atmos track;
with a 5.1 soundbar, the Dolby Digital track as it is rather than the
TrueHD converted; a phone, a stereo mix; Chrome, an AAC track copied into a
stream. Item details (`selected_audio`) don't know the player, so they give
the language's default track.

### Keyed URLs

Some players can't send headers with their media requests (Samsung's AVPlay
sets only a cookie and a User-Agent; LG's media pipeline takes only an
address), and an app's token goes only in the Authorization header.
A play request with `keyed_urls: true` gets a session whose addresses carry
a key of their own instead. Pley's Samsung, LG and Roku apps ask for keyed
URLs.

- `PlaybackInfo.url` is `/api/media/{key}/master.m3u8` (or `index.m3u8`)
  for a stream, whose playlists name their segments relatively, so they
  carry the key too; `/api/media/{key}/file.mkv` for a direct play, named
  for the file's container, with `Range` and
  `Cache-Control: private, no-store`.
- The key is `pley_mk_` and 256 random bits, kept in memory only. It opens
  that session's playlists and segments, or its file, and its seek previews,
  and nothing else: no API route takes it as a token.
- `/api/media/{key}/previews.bif` is the file's seek previews as a BIF file,
  for a player that fetches them itself (Roku's); `404` while they are being
  made, as for `trickplay`.
- It ends with the session (closed by its player, or forgotten after 15
  quiet minutes: `404` "playback session expired"), the account (disabled)
  and the app's sign-in (signed out, a new password): `404`. A session
  stopped from elsewhere answers `410 session_stopped` here too, with its
  message.
- Logs leave the key out (`/api/media/…/12.ts`).
- Everything else the player asks for (subtitles, sprite sheets, its
  reports) goes with the token as usual.
- **Live TV** works the same way: `POST /api/live/channels/{id}/watch`
  with `keyed_urls: true` answers `url` as
  `/api/live/media/{key}/index.m3u8`, whose segments are named relatively.
  The key opens that watching session's playlist and segments only, and
  ends with it (left, gone quiet for 45 s, or ended by another `watch`
  from the device), the account and the app's sign-in: `410
  session_stopped` then, as for the session's own address.

### Network memory and speed test

Automatic quality ([QUALITY.md](QUALITY.md)) starts at what the network
carries. The server remembers, per signed-in user, client and network, the
ceiling Automatic's last change left (for 24 hours) and the last speed test
(for 7 days). Only Automatic uses it; Original and a quality picked in the
menu never do.

The network is the one the request came from: the other end of the
connection, or the client a proxy on the server's own network forwards for.
An address on the server's own network (private, loopback, link-local,
Tailscale) is `"local"`; any other is an IPv4 address as it is, or an IPv6
address by its /64. Only a keyed hash of the address is stored, and rows
nobody wrote for 7 days are deleted.

```ts
// GET /api/playback/network?client_id=<id>, and PUT's answer
interface NetworkAnswer {
  local: boolean;              // this server's own network: never a speed test
  ceiling_kbps: number | null; // the most Automatic starts at here; null = no limit known
  source: "playback" | "probe" | null; // what set it
  since: number | null;        // when (unix ms)
  probe: boolean;              // run a speed test before starting Automatic
}
```

- `client_id`: the one the client sends in `POST /api/playback`, 1 to 128
  characters; missing or longer: `400 bad_request`.
- `ceiling_kbps`: a ceiling from playback set within 24 hours wins
  (`source: "playback"`); else a speed test within 7 days gives 70 % of what
  it measured (`"probe"`), or no limit when the whole test arrived in time
  (`probe_complete`); else `null`. `source` and `since` are `null` whenever
  `ceiling_kbps` is.
- `probe` is true only away from the server's network, with no speed test
  (or forget, below) from this client on this network within 7 days and no
  ceiling from playback within 24 hours.

`PUT /api/playback/network` stores what the client reports, with exactly one
of `probe_kbps` and `ceiling_kbps`, and answers as `GET`:

```ts
{ client_id, probe_kbps: 52000, probe_complete: false } // a speed test's result
{ client_id, ceiling_kbps: 8000 }  // Automatic's ceiling changed: a step down, or up to a quality
{ client_id, ceiling_kbps: null }  // back to Original, or Automatic picked again: forget
```

- Speeds are kbps from 1 to 10000000 (fractions are rounded). Anything
  else, both fields, or neither: `400 bad_request`.
- `probe_complete: true` (the whole test arrived inside the client's time)
  says the network is at least that fast: no limit is kept. Left out, it is
  `false`.
- `ceiling_kbps: null` clears the ceiling from playback and counts as a
  speed test that found no limit, for 7 days: nothing limits, and no test is
  due.

**`GET /api/playback/speedtest?bytes=<n>`** answers exactly `n` bytes
(default 33554432, 32 MiB; clamped to 65536–67108864) of incompressible
pseudo-random data, made as it is sent, with `Content-Length`,
`Cache-Control: no-store, no-transform`, never compressed.

- `bytes` that isn't a whole number: `400`.
- A user runs two at a time; a third answers `429 too_many_requests` until
  one ends or is dropped. One still going after 2 minutes is cut off.
- The client times it from the first byte and stops 2 s later (or when all
  has arrived): kbps = bytes × 8 / seconds / 1000, and `probe_complete` when
  it all arrived within the 2 s. Under 256 KiB arrived: report nothing.
- A client that can't read a body as it arrives (Roku) fetches
  `bytes=8388608` whole, times the whole request, and reports
  `probe_complete: false`.
- A server without these routes answers `404`: clients then use their own
  memory.

### Version 2

`caps_version: 2` turns on these rules. A report without it (the web app,
the Android app's web view, compatibility API clients) decides as version 1,
whatever else it sends: the web app's `subtitle_codecs` and
`picture_subtitles_sideload` only tell the server it draws picture
subtitles from their `.mks` (it asks for no `subtitle_index` of a track it
draws, and for a burn-in when the `.mks` answers `404` or `503`). Version 2
reports still send the version 1 fields: `containers` always counts;
`video_codecs` and `audio_codecs` stand in only when `video`, or both
`audio_decode` and `audio_passthrough`, are left out.

- **Video.** Direct only when an entry of `video` takes the stream: its
  codec; its profile when the entry lists profiles (10-bit HEVC needs
  `Main 10`); its size (either way round), frame rate (0.01 fps of slack),
  bit depth and level. A codec may have several entries (4K at 30 fps and
  1080p at 60): any one will do. `max_bitrate` and `max_height` in the
  request are only the quality the person picked.
- **Dynamic range.** HDR10 needs `hdr10` in `hdr_types`; HDR10+ needs
  `hdr10plus` or `hdr10`; HLG needs `hlg`. Without them the video is
  converted: to an HDR format the screen shows when the player takes it
  ([HDR transcodes](#hdr-transcodes)), else tone-mapped to SDR. The reason
  says which ("HLG converted to HDR10.").
- **Dolby Vision** plays as it is when `hdr_types` has `dolby_vision` and
  `dolby_vision_profiles` lists the profile at or above its level.
  Otherwise:
  - 8.1 and 8.4 (and 9, and 10 on an AV1 entry) play their HDR10, HLG or
    SDR base layer when the screen shows it, by the rules above;
  - 7 is **repackaged** when the screen shows HDR10: the video is copied
    into HLS and plays as its HDR10 base layer (Media3 has no profile 7
    fallback), or is converted to 8.1 for a Dolby Vision player that reads
    master playlists (below). Converted otherwise;
  - 5, which has no base layer, is converted (to HDR10 or HLG where the
    screen shows it, else to SDR), and never repackaged without a master
    playlist: MPEG-TS segments lose the Dolby Vision signaling, which leaves
    its colors wrong.

  Players that read master playlists keep Dolby Vision in remuxes, and
  Dolby Vision + HDR10+ streams change: see
  [Dolby Vision remuxes](#dolby-vision-remuxes).
- **Audio** plays as it is when its codec is in `audio_decode` or
  `audio_passthrough`. With `audio_track_switching`, a track other than the
  file's default needs no HLS: the player switches to it itself.
- **Picture subtitles** (PGS, VobSub, DVB) in `subtitle_codecs` are shown
  by the player when the file plays as it is. When it doesn't, or a
  stream is asked for (`remux`, `transcode`), they are burned in as for
  version 1 (HLS segments carry no subtitles). Subtitle
  files next to the media are always burned in. A player with
  `picture_subtitles_sideload` shows them next to the HLS stream from
  `/api/files/{id}/subtitles/{index}.mks` instead: it asks for no
  `subtitle_index`, so nothing is burned in (and HDR can stay HDR). A
  `subtitle_index` it does send is burned in as for any player.
- **Converting.** Video is converted to H.264, no bigger than the player's
  largest H.264 entry (a 4K file is made in 1080p for a 1080p stick, even
  when a higher quality is picked). HDR video the player can show stays
  HDR, in HEVC Main 10 ([HDR transcodes](#hdr-transcodes)). Audio in AAC,
  MP3, AC-3 or E-AC-3 that the player takes is copied; anything else is
  converted to:
  - E-AC-3 5.1 at 640 kbit/s when `audio_channels` is 6 or more, the source
    has more than two channels, and `eac3` is decoded or passed through;
  - else AC-3 5.1 at 640 kbit/s when `ac3` is;
  - else AAC with as many channels as the output takes, up to 5.1 (stereo
    for stereo outputs; a stereo source stays stereo).

### HDR transcodes

HDR video a version 2 player gets converted (a lower quality picked, a
stream its decoders don't take, Dolby Vision profile 5, or HLG its screen
doesn't show) stays HDR when:

- an entry of `video` is HEVC, `hardware`, lists `Main 10` (or no profiles)
  with a `max_bit_depth` of 10 or more (or none), and takes the video's
  frame rate;
- the screen shows HDR10 (`hdr10` or `hdr10plus` in `hdr_types`) or HLG
  (`hlg`);
- the server's transcoder can (`hdr_transcode` in its capabilities: a
  10-bit HEVC encoder works);
- no picture subtitles are burned in.

The video is then HEVC Main 10, no bigger than the largest such decoder
takes:

| Source | Screen shows HDR10 | Screen shows HLG only | Screen shows both |
|---|---|---|---|
| HDR10, HDR10+ | HDR10, as it is | HLG (libplacebo) | HDR10 |
| Dolby Vision 7, 8.1 | HDR10: the base layer | HLG (libplacebo) | HDR10 |
| Dolby Vision 5 | HDR10 (libplacebo applies the RPU) | HLG (libplacebo) | HDR10 |
| HLG, Dolby Vision 8.4 | HDR10 (libplacebo) | HLG, as it is | HLG |

- The source's HDR10 static metadata (mastering display, MaxCLL, MaxFALL)
  is kept; Dolby Vision's RPUs and HDR10+ are not (converted video never
  carries Dolby Vision). Video converted from HLG or Dolby Vision profile 5
  has no static metadata unless the source had some.
- Everything else (no such decoder, an SDR screen, an older transcoder,
  picture subtitles, a transcoder without libplacebo for a conversion) is
  tone-mapped to H.264 SDR. Version 1 players always are.
- The master playlist names it `hvc1.2.4.LLL.B0` with `VIDEO-RANGE=PQ` or
  `HLG`; the player's details say `video_out: "HEVC 3840×2160 HDR10"`.

### Several sizes

A player that moves between a multivariant playlist's variants itself
(`caps.hls.variants`, any caps version) gets each transcode in several
sizes from `master.m3u8` (`PlaybackInfo.adaptive`). See
[QUALITY.md](QUALITY.md#several-sizes-adaptive-bitrate) for what players do.

- Sizes: the one asked for (or the original's closest quality,
  `auto_quality`); the same size at its usual bitrate when that is lower
  (8 Mbps under 20 Mbps 1080p, 20 under 30 Mbps 4K); then 1080p, 720p, 540p
  and 360p below it, each at 8, 4, 2 or 0.7 Mbps and never more than the one
  above. A film wider than 16:9 goes by its width (1920×800 is 1080p).
- A smaller size with more than two thirds of the bits of the one above is
  left out. A remux, a direct play and a download come in one size.
- A smaller size's playlist is `v{k}.m3u8`, its segments `v{k}-{n}.ts`.
  Segment `n` of every size starts at the same time with a keyframe, so the
  player can move at any segment.
- The transcoder makes only the size asked for last (what the others made
  stays). A size it can't make in real time arrives slower than it plays, so
  the player moves down for it too.
- `TranscodeInfo.rendition` and `variants` say which size plays. A
  transcoder without `capabilities.hls_variants` makes one size.

### Dolby Vision remuxes

MPEG-TS segments carry a Dolby Vision stream's RPUs but can't name the
format, so players see plain HEVC (wrong colors for profile 5); a
multivariant playlist can name it. A version 2 player sending
`hls: { master_playlist: true }` gets `PlaybackInfo.url` pointing to
`master.m3u8` for every HLS session, remux or transcode, with one variant
(then a transcode's smaller sizes, see [Several sizes](#several-sizes)):

```
#EXTM3U
#EXT-X-VERSION:3
#EXT-X-INDEPENDENT-SEGMENTS
#EXT-X-STREAM-INF:BANDWIDTH=85293600,AVERAGE-BANDWIDTH=43050000,CODECS="hvc1.2.4.L153.B0,ec-3",SUPPLEMENTAL-CODECS="dvh1.08.06/db1p",RESOLUTION=3840x2160,FRAME-RATE=23.976,VIDEO-RANGE=PQ
index.m3u8
```

- `CODECS` per RFC 6381: `avc1.PPCCLL`, `hvc1.P.C.LLLL.B0`, `mp4a.40.2`
  (AAC), `mp4a.40.34` (MP3), `ac-3`, `ec-3`.
- Dolby Vision kept in the remux is named: profile 5 as `dvh1.05.LL` alone
  (no base layer anything else decodes); profile 8 as the HEVC codec with
  `SUPPLEMENTAL-CODECS="dvh1.08.LL/db1p"` (8.1, `VIDEO-RANGE=PQ`), `/db4h`
  (8.4, `HLG`) or `/db2g` (8.2, `SDR`), which Media3 turns into a
  `video/dolby-vision` track; profile 7 as `dvh1.07.LL`.
- `VIDEO-RANGE` is `SDR`, `PQ` or `HLG`. `FRAME-RATE` has three decimals.
  `BANDWIDTH` is twice the copied video's average (discs peak about there),
  or the encoder's rate. TS segments are the same as `index.m3u8`'s.
- Without `master_playlist` (and for every version 1 player, the web player
  and compatibility API clients) the session starts at `index.m3u8`, and
  remuxes name nothing.

For players with master playlists, the Dolby Vision rules of version 2
become:

- A **listed profile** (with a Dolby Vision screen, at or above the
  stream's level) still plays as it is. When the file needs repackaging
  anyway (the container, the audio, another track), the video is copied
  with Dolby Vision kept and named, **profile 5 included**.
- A **level the file overstates** (a re-encode that says 10 for 4K at 24
  fps): Dolby Vision is named at the level its size and frame rate need
  (the lowest whose pixel rate takes them in Dolby's table), when that is
  below what the file says. A decoder whose level is under the file's but
  takes the one the stream needs gets the stream repackaged, Dolby Vision
  kept and named ("…is repackaged to stay Dolby Vision: its file says level
  10, above this player's 9, where its picture needs 6."). As it is, Media3
  would refuse the level and play the base layer. A download stays the file
  as it is.
- **Profile 8 with compatibility id 6** (the Blu-ray's HDR10, a profile 7
  converted elsewhere with its id kept) has an HDR10 base layer, as 8.1:
  named `/db1p`, and played as HDR10 where Dolby Vision isn't shown.
- **Profile 7** for a player that lists 8 but not 7 (every Fire TV, Google
  TV devices) is **converted to 8.1** while repackaged: the enhancement
  layer is left out and each RPU rewritten (no residual, no enhancement
  layer resampling, P8.1 color coefficients; a full enhancement layer's
  curves become no-ops), as `dovi_tool`'s mode 2 does. The reason names the
  format as clients do: "Dolby Vision P7 FEL is converted to profile 8.1
  for this player.", ending ", without its enhancement layer" for a full
  one (`dv_el: "fel"`, whose 12-bit detail is lost), ", without its HDR10+"
  for Dolby Vision + HDR10+ (below), or both. Profile 8 must take the
  stream's level; otherwise the HDR10 base layer plays.
- **Dolby Vision + HDR10+** (`hdr10plus: true` on the stream, found when the
  file is analyzed) never plays as it is for a player that would decode it
  as Dolby Vision, unless the player says its decoding shows these
  (`dolby_vision_hdr10plus`, Kodi on CoreELEC): many Android TV devices'
  Dolby Vision decoders show black or freeze with HDR10+ in the stream
  (androidx/media#1895), whatever app plays it. It is repackaged with the
  HDR10+ messages taken out and Dolby Vision kept ("…plays as Dolby Vision
  without its HDR10+…"); without a master playlist, as its base layer with
  HDR10+ ("…plays as its HDR10+ base layer, repackaged…"). Players without
  Dolby Vision play these files normally. Files analyzed by an older server
  don't carry the flag until a forced scan of their library analyzes them
  again.
- A server whose transcoder lacks `hls_master_playlist` in its capabilities
  decides as if no player read master playlists.

The player's details (`GET /api/playback/{session}`) say what became of it:
`transcode.dolby_vision` (`"kept"`, `"converted to 8.1"`) and
`transcode.hdr10plus_stripped`.

## Downloads

A device that keeps a title to play away from the server downloads its
original (`GET /api/files/{id}/stream`, with ranges) when it plays it as it
is. Otherwise it downloads a file the server makes: the streams a playback
would stream, in one Matroska file, or MP4 for a player whose
`caps.containers` name `mp4` but not `mkv` (QuickTime).

| Method | Path | Who | What |
|---|---|---|---|
| POST | `/api/conversions` | `play` | the play request's body (`item_id`, `file_id?`, `caps`, `audio_index?`, `max_height?`, `max_bitrate?`, `mode?`) → `DownloadInfo` |
| GET | `/api/conversions/{id}` | `play` | `DownloadInfo` |
| DELETE | `/api/conversions/{id}` | `play` | `204`: stopped, unless done |
| GET | `/api/conversions/{id}/file` | `play` | the converted file (`video/x-matroska` or `video/mp4`) once whole, with `Range` |

```ts
interface DownloadInfo {
  id: string | null;         // the conversion; null for the original
  state: "direct" | "queued" | "running" | "done" | "failed";
  progress: number;          // 0 to 1 (1 for the original)
  size: number | null;       // bytes: the original's, or the converted file's once done
  error: string | null;      // why it failed (null for apps: see below)
  url: string;               // the original, or the converted file
}
```

- The server decides as for a playback with the same body: what the player
  plays as it is downloads as it is (`direct`). Otherwise the transcoder
  makes the file: the video copied or made anew at `max_height` and
  `max_bitrate`, the sound made AAC. One conversion at a time, from the
  start, at full speed.
- Video is copied only when the player plays it as it is (a Dolby Vision
  profile it doesn't decode would stay in the file); otherwise it is made
  anew, no bigger than a version 2 player's screen (`display`: 1080p for a
  phone, also of a 4K film) and never with more than 1.5 times the
  original's bits (which a playback's quality could ask for a file with few
  bits).
- Asked again for the same file and choices, from any device, the same
  conversion answers (or the file it made); a failed one is tried again.
- Poll `GET /api/conversions/{id}` every few seconds until it is `done`,
  then fetch `url`.
- Converted files are kept a week after their last download (a download cut
  short goes on with `Range`), within the transcoder's limit; once gone,
  asking again makes them again.
- A conversion is known to the server once asked for since it started
  (else `404`: ask again), and to those who may see its title.
- `503` while the transcoder isn't running or the file can't be read (ask
  again later); `501` with a transcoder too old to convert, or to make MP4.
- Apps aren't told why a conversion failed (`error` is `null`): the reason
  names the server's files, and the server's log keeps it.
- `DELETE` before the file is made stops the conversion; a file already
  made stays for whoever asks next. A conversion nobody asks about for an
  hour while it waits its turn is dropped (the device that wanted it went):
  asking again starts it again.

## Intro and credits detection

Markers come from three sources, best first:

1. **Chapters** (`source: "chapter"`) titled like an intro or credits
   (*Intro*, *Opening*, *OP*, *Opening Credits*, *Ending*, *ED*, *Credits*,
   *End Credits*, …), for movies and episodes. *Credits* chapters shorter than 15 s (a logo at the
   very end) are ignored.
2. **Audio fingerprints** of episodes (`source: "fingerprint"`), from
   Chromaprint fingerprints of the first half (at most 10 minutes) and the
   last 35 % (at most 8 minutes) of each episode. An intro is audio of 12 s
   to 3 minutes near the start that the nearest episodes of the season
   share (up to eight are compared; for a season with nothing shared, up to
   six of the show's other seasons). Credits are shared audio near the end.
   Fingerprints are cached per file version, so a new episode needs only
   its own.
3. **Key frames** (`source: "frames"`), for credits no other episode shares:
   the last stretch of at least 15 s that is mostly text on black, in the
   last 35 % (at most 8 minutes), ending less than 2 minutes before the end
   of the file.

A background worker analyzes one season at a time after library scans (and
30 s after start-up) and waits while anything plays. A newer detection
version analyzes everything again by itself.

- A season not analyzed yet goes first as soon as one of its episodes
  plays: the episodes playing first, then the rest, each followed by a
  `library_changed` event so players pick up the markers.
- Seasons an administrator asks for (`POST /api/items/{id}/analyze`) go
  next. The seasons scans find wait for library work's hours (below).
- Nothing is analyzed while something is transcoded (a playback's or a
  download's), not even that season.
- Skipped when `detect_markers` is off or the transcoder's FFmpeg lacks
  Chromaprint (chapters are still used).

## Library work

Intro and credits detection and seek previews for whole libraries
(`previews_in_background`) read whole files. They wait while anything
plays, and for a minute after. They also wait while other programs keep
more than `library_work_max_cpu` % of the processors busy, averaged over a
minute (Pley and the transcoder it starts don't count; a transcoder run
separately on the same machine does). Then `library_work` decides:

- **`auto`** (the default): what is likely to be watched soon goes first,
  at any hour:
  - shows someone played an episode of in the last 30 days;
  - movies someone began in that time and hasn't finished;
  - titles on a watchlist;
  - shows with episodes added in the last 14 days that someone watched
    before;
  - the 20 newest movies of the last 14 days.

  The rest waits for quiet hours, learned from when something plays:
  - each hour of the week counts the minutes something played, older weeks
    weighing less (0.8 a week); an hour is quiet when something played in
    less than a tenth of its minutes;
  - each day's four quietest hours in a row are always quiet (counting only
    hours the server was up);
  - work begins only where this hour and the next are quiet;
  - until about a week is known, any hour is quiet; at first start the
    hours come from the last 8 weeks of watch history;
  - after 24 hours of the server being up without a quiet hour, the rest
    goes on whenever nothing plays.
- **`hours`**: all of it from `library_work_from` to `library_work_to`
  (hours of the day in the server's time; past midnight when `to` comes
  first, all day when they are the same).
- **`idle`**: whenever nothing plays.

A season whose hours end is put off and goes first at its next turn;
seek previews being made when their hours end are finished. The hours and
the processors hold back only the libraries' work: a title someone opens
gets its previews, a season being watched its markers, and an analysis an
administrator asks for runs, at any hour.

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/system/library-work` | `admin` | `WatchingHours` |

```ts
interface LibraryWorkStatus {
  mode: "auto" | "hours" | "idle";
  waiting: "playing" | "busy" | "hours" | null; // why the rest of the libraries waits now
  resumes_at: Timestamp | null;     // when the hours it waits for begin
  learned: boolean;                 // about a week of playback is known
}

interface WatchingHours extends LibraryWorkStatus {
  now: number;                      // the hour of the week now: 0 is Sunday 0:00, server time
  hours: {                          // 168, from Sunday 0:00
    playing: number | null;         // share of its minutes something played in, 0-1; null: not seen enough
    open: boolean;                  // work on the rest of the libraries may begin then
  }[];
}
```

### How many at once

The libraries' seek previews are made one file at a time. More go at once
where that disturbs nobody, four at most:

- a second in library work's own hours (above), once nothing has played
  for 10 minutes: someone who just stopped often starts something else,
  and a playback that starts puts off every file being made;
- a third and a fourth where this hour and the next were rarely watched
  (something played in under a tenth of their minutes, the four quietest
  hours a day kept for library work not counted; any hour until about a
  week is known);
- each only while the transcoder has a core idle, at least what each file
  being made takes, and 2 GB of memory free, as it measures them (a
  container's limits counted), and 20 s after the one before.

One file at a time is read from each hard disk, array of hard disks
(RAID), ZFS pool, network server, or disk of an Unraid or mergerfs pool,
and up to four from a solid-state disk or an array of nothing else: two
whole files read at once from one hard disk each go at a fraction of the
speed. Pley tells disks apart from the server's mounts (Linux, in a
container too: a partition is its disk's, an array its disks') or
Windows' volumes; for a file in an Unraid user share or a mergerfs pool,
it asks the pool which disk holds it. A file whose storage it can't tell
(the pool doesn't say which disk holds it, a file system on no disk it
knows, a share that didn't answer within a minute) is made alone; disks
it can't trace, and shares whose server it can't tell, count as one
storage. With a transcoder it didn't start, all files are one
storage, read one at a time. Something that plays puts the
libraries' files off within seconds. Transcoders from before this, and
those on macOS (which can't pause their work for a playback), make one at
a time; `PLEY_TRANSCODER_MAX_PREVIEWS=1` makes any transcoder do so
([docs/TRANSCODER.md](TRANSCODER.md)).

## Watch history

A play is recorded, with the device it played on, once it reaches the point
where the item counts as watched (90 %). Marking something watched by hand
is not a play.

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/history` | `library` | `{ total, entries: HistoryEntry[] }`, newest first |
| DELETE | `/api/history/{id}` | `play` | `204`; your own entries, or any for administrators |

`GET /api/history` query: `offset`, `limit` (default 50, max 200),
`item_id` (the item, or the episodes or tracks under it), `library_id`,
`since` (unix seconds); administrators also `user_id=<id>` or `all=1`.
Your own plays unless asked otherwise; entries of items you can't see are
left out.

```ts
interface HistoryEntry {
  id: Id;
  viewed_at: Timestamp;
  device?: string;                  // "Chrome on Windows", or a compatibility API client's name
  user: { id: Id; name: string; avatar_url?: string; avatar_color?: string };
  item: ItemSummary;
}
```

## Requests

People ask for movies and shows the libraries don't have, found on TMDB
(*Discover*). A manager approves or declines each request; administrators'
own requests, and those of accounts whose `request_access` is `"auto"`, are
approved as they're made. Asking for what someone asked for already follows
their request instead, so its news comes to the follower too. An approved
request goes to Radarr (movies) or Sonarr (shows) where the
[settings](#system-admin) connect them, which find and download it, and is
*available* once a library has it: the movie, or every season asked for,
whole.

**Who.** A *manager* approves, declines and marks requests as added by
hand, and sees and deletes everyone's: an administrator's own sign-in, or
one of Pley's own apps on an administrator's account whose token has
`approvals` (other publishers' apps can't be allowed it). Anyone else, an
administrator's app without `approvals` included, sees only the requests it
made or follows. Discover and asking need `request_access` `"ask"` or
`"auto"`; an account whose `request_access` is `"off"` still lists, reads
and withdraws its own requests, with their pictures, and stops following
others'. An
administrator's account asks without limits, and its requests are approved
as made from the web, the desktop app and Pley's own apps; from other
publishers' apps they wait for a manager (its `access` there is `"ask"`, in
the summary and in `/api/auth/me`).

Requests are off until `requests` is on in the settings, and need a TMDB
key. Until then every endpoint here but the summary answers `503
unavailable`. Every endpoint answers in the error envelope, malformed
requests too (a body that isn't JSON or has the wrong types, an id that
isn't a number: `400 bad_request`), and checks in this order: requests on
(`503`), who asks (`403`), the query and the body (`400`), then the rest.
An app's token without the route's scope is refused before all of it
(`403 insufficient_scope`, as everywhere).

| Status | `code` | When |
|---|---|---|
| `503` | `unavailable` (`setup: false`) | requests are off |
| `503` | `tmdb_key` (`setup: true`, `retry: false`) | TMDB refuses the server's key: an administrator fixes it in Settings → Server, trying again won't help |
| `503` | `tmdb_down` (`setup: true`) | TMDB isn't answering: try again |
| `403` | `no_access` | the account may not ask |
| `403` | `forbidden` | a managers' route, for anyone else |
| `403` | `restricted` | the title is above the account's age limit; `message` says why |
| `409` | `in_library` | the libraries the account can open have all that was asked for |
| `409` | `declined` (`retry_at`) | asking: what is left to ask for was declined for the account in the last 30 days; `retry_at` is when all of it may be asked for again |
| `409` | `conflict` (`request`, `retry_at`) | what was asked can't be done to the request as it is now: `request` is the request as this session reads it, `retry_at` when it may be done (a declined request its maker may remove, or a follower stop following, once its 30 days are over), else `null` |
| `429` | `too_many_requests` (`retry_at`) | past the account's limits (below), with `Retry-After` when `retry_at` is set |
| `502` | `tmdb_down` | a picture: TMDB's picture server isn't answering (`Cache-Control: no-store`) |

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/requests/summary` | `any app` | `RequestSummary`: what this session may do with requests, and what there is to see |
| GET | `/api/discover/{list}?page=` | `requests` | `DiscoverPage`. `list`: `trending`, `movies`, `shows`, `upcoming` or `anime` |
| GET | `/api/discover/search?q=&page=&missing=` | `requests` | `DiscoverPage`: movies and shows matching `q` (1-200 characters); `missing=1` leaves out those whose `status` is `library` |
| GET | `/api/discover/movie/{tmdb_id}` | `requests` | `DiscoverDetail` |
| GET | `/api/discover/show/{tmdb_id}` | `requests` | `DiscoverDetail`, with `seasons` |
| GET | `/api/discover/images/{kind}/{file}?w=` | `requests` | one of TMDB's pictures (below); with `request_access` `"off"`, only the posters and backdrops of the requests the account made or follows (others: `403 no_access`) |
| GET | `/api/items/{id}/request` | `requests` | `ItemRequest`: a library's movie or show as Discover sees it, whatever the account's `request_access` |
| GET | `/api/requests?status=&attention=&kind=&user_id=&q=&order=&mine=&unseen=&changed_since=&offset=&limit=` | `requests` | `RequestList` (below) |
| POST | `/api/requests` | `requests` | `{ kind, tmdb_id, seasons? }` → `201` with the new `MediaRequest`, or `200` with the one followed when nothing was left to ask for |
| POST | `/api/requests/seen` | `requests` | `204`: what had news for this account is seen (`unseen` 0) |
| GET | `/api/requests/{id}` | `requests` | `MediaRequest` |
| DELETE | `/api/requests/{id}` | `requests` | `204`: its maker withdraws it (below); managers delete it, for everyone |
| DELETE | `/api/requests/{id}/watch` | `requests` | `204`: the account follows it no more |
| POST | `/api/requests/{id}/approve` | `approvals` | managers: `{ quality_profile?, root_folder?, seasons? }` → `MediaRequest` |
| POST | `/api/requests/{id}/decline` | `approvals` | managers: `{ reason? }` → `MediaRequest` |
| POST | `/api/requests/{id}/handled` | `approvals` | managers: → `MediaRequest`, approved as added by hand |
| GET | `/api/requests/options` | `approvals` | managers: `RequestOptions`, what approving may choose in Radarr and Sonarr |

- **The summary** answers every session and every app, so an app learns
  whether to show requests at all. `enabled` and `access` are always real.
  While requests are off, and for an app whose token hasn't `requests`,
  every number is 0, `manage` false and `changed_at` null; such an app gets
  `sign_in_again: "requests"` while requests are on and `access` isn't
  `"off"` (signing in again lets it ask). An administrator's account on one
  of Pley's own apps whose token has `requests` but not `approvals` gets
  `sign_in_again: "approvals"`.
- **Limits** (`request_limit_movies`, `request_limit_seasons`, 0 = none)
  count an account's own requests of the last 7 days (a show its seasons):
  those it made in them, and those it took over in them as their earliest
  follower (below); pending, approved and available ones count, declined
  and failed ones don't, and following costs nothing. They never apply to
  administrators. Asking for more than is left answers `429` with
  `retry_at`: when enough of what counts is older than 7 days for all that
  was asked; `null` when more was asked than the limit itself (pick
  fewer). `*_next_at` in the summary and `DiscoverDetail` is when one more
  may be asked for, once none is left.
- **Lists** (`/api/discover/…`): up to 20 titles a page in TMDB's order,
  `page` from 1 (default) to 500. `trending`: this week's movies and shows;
  `movies` and `shows`: the most popular; `upcoming`: movies coming to
  cinemas in the server's region (`metadata_language`'s country);
  `anime`: the most popular anime shows. Adult titles are left out for
  everyone (a title page and asking answer `404` to all but managers).
  TMDB's answers are reused for an hour, details for six. For an account
  with an age limit, titles above it, unrated ones and those whose details
  TMDB doesn't give are left out, as in the libraries; its `movies` and
  `upcoming` come from TMDB's own search rated at most the highest
  certification of the region within the limit, so that few are. A page
  may hold fewer titles than 20, even none, before the last: load the next
  one. The same title may show on two pages.
- **A title's `status`** for the account asking, from the libraries it can
  open only, is the first of these that holds: `library` (a library has the
  movie, or every aired season of the show but specials, whole);
  `restricted` (above the account's age limit, unrated titles included:
  only on a title page, with `restriction`); `requested` (active requests,
  pending, approved or failed, whoever made them, ask for all it lacks);
  `declined` (the last request for it that the account made or follows was
  declined in the last 30 days, or what a show lacks was); `partial`
  (shows: a library or active requests have some of it, and aired seasons
  are left to ask for); `none`. A movie matches by TMDB's or IMDb's id, a
  show by TMDB's, TheTVDB's or IMDb's (lists look up a show's other ids
  where a library show has no TMDB id). On lists, a show a library has or a
  request asks for is looked at season by season from its details (8 at a
  time); without them it is `library` where a library has it, else
  `requested`.
- **Seasons** (`DiscoverDetail.seasons`): `state` for the account, the
  first that holds: `library` (whole in a library it can open: as many
  episodes as have aired), `requested` (an active request asks for it),
  `unaired`, `partial` (some of its episodes are there), `missing`.
  `declined`: a request for it that the account made or follows was
  declined in the last 30 days. `request_id`: the active request asking for
  it, where the account made or follows it (any, for managers).
- **Pictures** (`/api/discover/images/{kind}/{file}`): every picture field
  here is such a path, so no client contacts TMDB. `kind` is `poster`,
  `backdrop`, `profile` (cast) or `still`; `file` is TMDB's name, 1-64
  letters and digits then `.jpg` or `.png` (anything else: `404`). `w`, the
  width drawn, picks the smallest of TMDB's sizes at least that wide
  (posters w92 w154 w185 w342 w500 w780, backdrops w300 w780 w1280,
  profiles w45 w185 h632, which counts as 421, stills w92 w185 w300), the
  largest above them, and TMDB's original from 2000; without `w` (or one
  that isn't a number), posters w342, backdrops w1280, profiles w185, stills
  w300. Pley fetches each once however many ask at once, 8 pictures at a
  time (past 256 being fetched or waiting, more answer `502` until some
  are done), keeps them under `cache/discover` (docs/STORAGE.md), 256 MB
  at most, and answers with `Cache-Control: private, max-age=604800,
  immutable`. TMDB's "no such picture" is `404`, believed for a day.
- **A library title's page** (`GET /api/items/{id}/request`): `404` for an
  item the account can't open, one that isn't a movie or show, or one TMDB
  doesn't know (Pley looks it up by TheTVDB's or IMDb's id where the item
  has no TMDB id; the answer is kept). It waits for TMDB 2 seconds at most
  in all: `askable` is `null` until the details come (later calls have
  them), and `503 tmdb_down` while TMDB hasn't said which title an item
  known only by those ids is (`tmdb_key` when it refuses the key).
- **Asking** (`POST /api/requests`) checks, in this order: requests on
  (`503`), the account may ask (`403 no_access`), the body (`400`: seasons
  for a movie, `seasons: []`), the title (`404` for one TMDB doesn't know,
  and for an adult one but to managers), its age limit (`403 restricted`),
  every season one TMDB lists (`400`), the libraries it can open (`409
  in_library` when they have the movie, or every season asked for whole).
  `seasons` (shows only) are TMDB's season numbers, 0 for specials. Left
  out, they are the aired seasons but specials that the libraries it can
  open lack whole, less those declined for the account in the last 30 days
  (all of them, should each have been; every aired one, where none lacks);
  before any has aired, every season TMDB lists but specials (`400` when it
  lists none). Seasons a library it can open has whole are dropped. Then:
  - the account follows each active request (pending, approved or failed),
    whoever made it, that asks for the movie or for any of the seasons
    asked for; its own change nothing; one that stops being active
    meanwhile (declined, deleted or available) is left out, as if it
    weren't there;
  - approved as it asks (`access` `"auto"`), it approves those of them
    still pending, as if it had just asked for them (`decided_by`: the
    account), and they are sent;
  - what no active request asks for yet becomes a new request, answered
    `201`: unless some of it was declined for the account in the last 30
    days (a request it made or follows), `409 declined` with `retry_at`, or
    it is past its limits, `429`;
  - with nothing left to ask for, the answer is `200` with the request for
    the movie, or for the lowest season asked for (the account's own first,
    then the newest): `watching: true`, or `mine: true` for its own.

  A refused ask changes nothing: nothing is followed or approved. Following
  costs nothing against the limits, so a title page's *Tell me too* is this
  call (for a show, with the seasons whose `state` is `requested`).
- **Following**: an account follows requests by asking (above), and
  `DELETE /api/requests/{id}/watch` stops it (`409` where it doesn't follow
  the request, its own included, and for a declined one until its 30 days
  are over, with `retry_at`: they hold for its followers as for its maker;
  `404` for one it can't read). Followers read a request as *Who sees
  what* says, and hear its news ([events](#request-events)). When its
  maker withdraws a request others follow, or the maker's account is
  deleted, the request stays: its earliest follower becomes its maker, and
  it counts toward their limits from then as if they had asked for it; the
  others keep following.
- **Withdrawing and deleting** (`DELETE /api/requests/{id}`): its maker
  withdraws a pending or failed request, and removes a declined one once
  its 30 days are over (before then `409 conflict` with `retry_at`, when it
  may; an approved or available one, `409`). A follower gets `409`: it
  stops following instead. A manager's delete, of its own requests too,
  removes the request for everyone, its followers included, and deleting a
  declined one lifts its 30 days. Nothing is removed from Radarr or Sonarr.
- **Approving** (managers) a pending, failed or declined request, or an
  approved one not sent (one added by hand too): it answers at once with
  the request `approved`, its stage `approved` while it is being sent. In
  the background, where Radarr or Sonarr is connected for its kind (its
  address, key, quality profile and root folder are all set), the title is
  added there, monitored, with the settings' quality profile and root
  folder (anime's own where set) or those given, and searched for unless
  `radarr_search` / `sonarr_search` is off. A title already there is
  monitored again, the seasons asked for too, and searched for. Should that
  fail, the request is `failed`, `reason` says why and managers hear it
  (`request_failed`); approving it again (`retry`) tries again. The request
  keeps what its approval chose while it is being sent: a send a restart
  cut short is made again when the server starts, and should how it went
  not be saved, the request is `failed` too. Without Radarr or Sonarr
  connected for its kind, it is simply approved, for an administrator to
  add by hand. The body may choose `quality_profile` (a profile's id, from
  the options below), `root_folder` (a path, 512 characters at most) and
  `seasons`, which keeps only those of the seasons asked for: the others
  leave the request, stop counting toward its maker's limits, and may be
  asked for again (`400` for an id below 1, an empty path, a season it
  doesn't ask for, none, or seasons of a movie). A request sent already,
  or being sent, is answered as it is (`200`: a second approve), and so is
  one another session approved meanwhile (a race); an available one
  answers `409`.
- **Declining** (managers) a pending or failed request, or an approved one
  not yet available (others: `409`), keeps `reason` (500 characters at
  most) for its maker to read. The title is declined for its maker and its
  followers for 30 days (`status` above), and they hear it
  (`request_decided`), followers without the reason.
- **Added by hand** (managers, `POST /api/requests/{id}/handled`): a
  pending, failed or declined request, or an approved one not sent, is
  approved as added to Radarr or Sonarr by hand. It is never sent
  afterwards (`sent_to` `null`, `note` "Added by hand", stage `approved`),
  and waits for the library like any other; approving it makes it one to
  send after all (sent at once where Radarr or Sonarr is connected). One
  added by hand already, or by another session meanwhile, is answered as
  it is; one sent, being sent or available answers `409`.
- **Options** (managers, `GET /api/requests/options`): what approving may
  choose in Radarr and in Sonarr, each `null` where it isn't connected:
  their quality profiles and root folders, and the settings' choices, which
  an approval preselects (anime's for anime). The lists are kept 5 minutes,
  then asked for again (one question at a time) and waited for 3 seconds at
  most: a service that hasn't answered by then has the lists it gave last
  (none, if it never did), and `error` says why ("Radarr isn't answering",
  or what went wrong).
- **Actions** (`MediaRequest.actions`): what this session may do to the
  request now, in the order clients list them. Clients offer exactly these
  and work out none themselves; one refused anyway, after a race, answers
  `409 conflict` with the request as it is now.
  - `withdraw`: its maker, unless a manager (who deletes), while pending or
    failed, or declined once its 30 days are over: `DELETE
    /api/requests/{id}` (*Remove*, on a declined one);
  - `unwatch`: a follower, but of a declined request only once its 30
    days are over: `DELETE /api/requests/{id}/watch`;
  - `approve`: managers, while pending or declined, or approved and not
    sent where Radarr or Sonarr is connected for its kind;
  - `retry`: managers, while failed: `POST /api/requests/{id}/approve`;
  - `decline`: managers, while pending, failed or approved;
  - `handled`: managers, while pending, failed or declined, or approved and
    neither sent nor added by hand;
  - `delete`: managers: `DELETE /api/requests/{id}`, for everyone;
  - `ask_again`: an account that may ask, on a `gone` request it made or
    follows, or its own declined one once its 30 days are over: `POST
    /api/requests` with its `kind`, `tmdb_id` and `seasons`.

  A request being sent offers neither `approve` nor `handled`.
- **Available**: looked at after every library scan and every 5 minutes,
  for active requests: the title in a library its maker can open, matched
  as in `status` above, with every season asked for whole there (as many
  episodes as TMDB says have aired; with fewer, the last one there and
  Sonarr saying it has them all, as files of several episodes are counted
  once). Libraries its maker can't open don't count, as it may ask for
  what only they have. `available_seasons` lists those whole so far. A
  pending request whose title arrives this way is available too.
- **Unseen**: a request has news for its maker and its followers when it
  becomes available, more of its seasons do, or it is declined; `unseen`
  counts those of the account's since it last called `POST
  /api/requests/seen`. That sends `requests_changed` (`request_id` `null`)
  to the account's other streams, not those of the session that called it,
  so its other devices clear their badges.
- **Listing** (`GET /api/requests`): the requests the account made or
  follows; a manager's has everyone's, unless `mine=1`. Each filter given
  narrows it:
  - `status`: one status, or several separated by commas
    (`pending,failed`);
  - `attention=1`: pending and failed requests: what the summary's
    `pending` and `failed` count for managers (*Needs attention* is
    `attention=1&order=oldest`);
  - `kind`: `movie` or `show`;
  - `user_id`: managers: the requests that account made (ignored for
    others);
  - `q`: titles containing it, at most 200 characters, whatever the case
    of the letters A to Z;
  - `unseen=1`: only those the account made or follows with news it
    hasn't seen, for managers too, newest news first;
  - `changed_since`: those whose `updated_at` is later than it;
  - `order`: `newest` (the default) or `oldest`, by `created_at` (with
    `unseen=1`, by when their news came);
  - `offset` (default 0) and `limit` (default 50, at most 200).

  `1`, `true` and `yes` turn a flag on; another `status`, `kind` or
  `order`, or a number that isn't one, answers `400`. `total` counts what
  the filters keep, `counts` each status of it with every filter but
  `status`, and `people`, for managers, everyone who made a request that
  every filter but `user_id` keeps, by name (the person filter, as apps
  can't list users; `[]` for others).
- **Events**: see [Request events](#request-events).
  `requests_settings_changed` says the summary may have changed.
- **`download`**: an approved request's download in Radarr or Sonarr while
  one is queued or running, from their queues as last fetched: answers
  never wait for Radarr or Sonarr. A queue is fetched again in the
  background once 10 seconds old (a minute, after it couldn't be read), and
  when the server starts.
- **`stage`**: a pending, declined or failed request's is its `status`; an
  available one whose title no library has any more is `gone` (available
  again once a library has it, without news). An approved
  one is `downloading` or `importing` while `download` says so, `partial`
  while some of the seasons asked for are ready, `searching` once sent to
  Radarr or Sonarr, and `approved` while it isn't sent: being sent, with no
  Radarr or Sonarr connected for its kind, or added by hand.
- **Who sees what**: a request's maker and the accounts following it read
  it; managers read everyone's. Followers don't see who asked or decided,
  nor why it was declined, and one who became its maker still doesn't
  for a decision made before then. What went wrong, `note`, `sent_to` and
  `watchers` are for managers. `item_id` is there only for an account that
  can open the library's title, and `actions` are this session's.

```ts
type TitleKind = "movie" | "show";
type RequestStatus = "pending" | "approved" | "declined" | "failed" | "available";
type RequestAccess = "off" | "ask" | "auto"; // may not ask; asks an administrator; approved as made

interface RequestSummary {
  enabled: boolean;                 // requests are on (and TMDB has a key)
  access: RequestAccess;            // what asking from this session does: an administrator's "auto", but
                                    // "ask" from other publishers' apps; given to apps without `requests` too
  movies_left: number | null;       // movies it may still ask for (last 7 days); null = no limit
  seasons_left: number | null;
  movies_limit: number | null;      // the limits that apply to it, per 7 days; null = none
  seasons_limit: number | null;
  movies_next_at: Timestamp | null; // with no movies left: when one more may be asked for
  seasons_next_at: Timestamp | null; // with no seasons left: likewise
  manage: boolean;                  // this session is a manager
  pending: number;                  // managers: requests waiting for a decision (others: 0)
  failed: number;                   // managers: failed requests, which wait for one (others: 0)
  unseen: number;                   // the requests it made or follows with news since it last looked
  own: number;                      // the requests it made or follows, any status
  changed_at: Timestamp | null;     // the newest updated_at of the requests this session sees (managers:
                                    // everyone's): clients without events refetch when it moves
  sign_in_again: "requests" | "approvals" | null; // the scope signing in again would give this app
}

interface DiscoverTitle {
  kind: TitleKind;
  tmdb_id: number;
  title: string;
  year: number | null;
  overview: string;
  poster_url: string | null;        // "/api/discover/images/poster/abc.jpg"
  backdrop_url: string | null;      // "/api/discover/images/backdrop/def.jpg"
  rating: number | null;            // TMDB's users', 0-10; null with too few votes
  status: "library" | "restricted" | "requested" | "declined" | "partial" | "none";
  item_id: Id | null;               // in a library this account can open: the title
  request_id: Id | null;            // an active request for it this account made or follows (any, for
                                    // managers), whatever `status` says: never work status out from it
}

interface DiscoverPage {
  page: number;
  total_pages: number;
  results: DiscoverTitle[];
}

interface DiscoverDetail extends DiscoverTitle {
  original_title: string | null;    // where it differs from `title`
  tagline: string | null;
  genres: string[];
  release_date: string | null;      // "2024-05-01": a movie's release, a show's first air date
  runtime_minutes: number | null;   // movies
  certification: string | null;     // "PG-13", "TV-MA": the server's region's
  network: string | null;           // a show's first network, a movie's first studio
  show_status: string | null;       // shows: TMDB's ("Returning Series", "Ended")
  anime: boolean;                   // TMDB's "anime" keyword, or Japanese animation: anime's folder and profile
  imdb_id: string | null;
  cast: { name: string; character: string | null; photo_url: string | null }[]; // 12 at most;
                                    // photo_url: "/api/discover/images/profile/…"
  seasons: DiscoverSeason[] | null; // shows, in order (specials, 0, first)
  declined: { reason: string | null; at: Timestamp; again_at: Timestamp } | null; // the newest request for
                                    // it that the account made or follows, declined in the last 30 days
                                    // (shows: for some seasons); reason: null for a follower, then too
                                    // once it became its maker
  restriction: string | null;       // why the account can't ask: "Rated R: above this account's age limit"
  watching: boolean;                // the account follows an active request for it
  requests: MediaRequest[];         // for it: this account's and those it follows (everyone's for managers),
                                    // newest first
  movies_left: number | null;       // as in the summary, so the page needs no other call
  seasons_left: number | null;
  movies_next_at: Timestamp | null;
  seasons_next_at: Timestamp | null;
}

interface DiscoverSeason {
  number: number;                   // 0: specials
  name: string;
  episodes: number;                 // TMDB lists
  aired: number;                    // of those, aired by now
  air_date: string | null;          // its first episode's
  poster_url: string | null;        // "/api/discover/images/poster/…"
  in_library: number;               // episodes of it in the libraries this account can open
  requested: boolean;               // an active request asks for it
  state: "library" | "requested" | "unaired" | "partial" | "missing"; // draw a season from this
  declined: boolean;                // a request for it that the account made or follows was declined in
                                    // the last 30 days
  request_id: Id | null;            // the active request asking for it, where this account made or follows
                                    // it (any, for managers)
}

interface ItemRequest {
  kind: TitleKind;
  tmdb_id: number;
  status: DiscoverTitle["status"];
  request: MediaRequest | null;     // the newest active request for it this account made or follows (any,
                                    // for managers)
  askable: number[] | null;         // the seasons the season picker would tick were there no limit: those
                                    // missing or partly there, but specials and those declined lately; [] for
                                    // movies and titles above the age limit; null without TMDB's details yet
}

type RequestStage =
  | "pending" | "declined" | "failed"
  | "approved"                      // approved, not sent: being sent, no Radarr or Sonarr connected for its
                                    // kind, or added by hand
  | "searching"                     // sent; nothing downloading yet
  | "downloading"                   // see `download`
  | "importing"                     // downloaded; going into the library
  | "partial"                       // shows: some of the seasons asked for are ready
  | "available"
  | "gone";                         // was available; no library has it any more

interface MediaRequest {
  id: Id;
  kind: TitleKind;
  tmdb_id: number;
  title: string;                    // as TMDB had it when asked
  year: number | null;
  poster_url: string | null;        // "/api/discover/images/poster/…"
  backdrop_url: string | null;
  seasons: number[] | null;         // shows: the seasons asked for; null for movies
  available_seasons: number[];      // shows: those a library has whole
  anime: boolean;
  status: RequestStatus;            // the decision
  stage: RequestStage;              // where it is: draw a request from this
  stage_at: null;                   // always null: no stage says when it ends
  reason: string | null;            // declined: the administrator's (or null), for its maker and managers;
                                    // failed: what went wrong, for managers; null for others
  note: string | null;              // managers: what to know of it ("Added by hand")
  mine: boolean;                    // this account made it
  watching: boolean;                // this account follows it
  watchers: number;                 // managers: how many follow it; 0 for others
  unseen: boolean;                  // its maker's or a follower's: it became available, partly available or
                                    // declined since the account last looked
  actions: RequestAction[];         // what this session may do to it now, in the order to list them
  requested_by: RequestUser | null; // its maker and managers; null for followers
  decided_by: RequestUser | null;   // who approved or declined it, for its maker and managers: the maker where
                                    // approved as made; null for an account deleted since, for followers, and
                                    // for a maker who inherited it after the decision
  decided_at: Timestamp | null;
  created_at: Timestamp;
  updated_at: Timestamp;
  available_at: Timestamp | null;
  retry_at: null;                   // always null: a failed request waits for a manager
  sent_to: { service: "radarr" | "sonarr"; quality_profile: string | null; root_folder: string | null } | null;
                                    // managers: where it was sent (the profile's name), where known
  item_id: Id | null;               // a library's title once there, where this account may open it
  download: RequestDownload | null;
}

interface RequestUser {
  id: Id;
  name: string;
  avatar_url: string | null;
  avatar_color: string | null;
}

interface RequestDownload {
  progress: number;                 // 0-1, of everything of it queued
  state: "queued" | "downloading" | "paused" | "importing" | "warning";
  eta: Timestamp | null;            // the queue's estimate, where it has one
  episodes: number;                 // shows: episodes queued (movies: 1)
}

type RequestAction =                // see Actions above
  | "withdraw"                      // its maker: DELETE /api/requests/{id} ("Remove" on a declined one)
  | "unwatch"                       // a follower: DELETE /api/requests/{id}/watch
  | "approve"                       // managers: POST /api/requests/{id}/approve
  | "retry"                         // managers, a failed one: POST /api/requests/{id}/approve
  | "decline"                       // managers: POST /api/requests/{id}/decline
  | "handled"                       // managers: POST /api/requests/{id}/handled
  | "delete"                        // managers: DELETE /api/requests/{id}, for everyone
  | "ask_again";                    // POST /api/requests with its kind, tmdb_id and seasons

interface RequestList {
  total: number;                    // the requests the filters keep (requests: a page of them)
  counts: Record<RequestStatus, number>; // of each status, with every filter but `status`
  requests: MediaRequest[];
  people: RequestUser[];            // managers: who made requests every filter but `user_id` keeps, by
                                    // name; [] for others
}

interface RequestOptions {          // GET /api/requests/options
  radarr: ApprovalOptions | null;   // null: not connected
  sonarr: ApprovalOptions | null;
}

interface ApprovalOptions {
  quality_profiles: { id: number; name: string }[]; // [] while it has never answered
  root_folders: { path: string; free_bytes: number | null }[];
  quality_profile: number | null;   // the settings' choices: what an approval preselects
  root_folder: string | null;
  anime_quality_profile: number | null; // for anime requests; null: as above
  anime_root_folder: string | null;
  error: string | null;             // it didn't answer: "Radarr isn't answering"; the lists are its last
}
```

A `409 conflict` about a request is `{ code: "conflict", message, request:
MediaRequest, retry_at: Timestamp | null }`, and a `409 declined` is
`{ code: "declined", message, retry_at: Timestamp }`. Clients show when from
`retry_at`, never from the date in `message`.

## Activity & events

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/activity` | `user` | `{ scans: ScanStatus[], sessions: SessionInfo[], analysis: AnalysisStatus \| null, seasons_pending: number, previews_pending: number, previews_running: number, library_work: LibraryWorkStatus \| null }` |
| DELETE | `/api/sessions/{id}` | `play` | optional body `{ message?: string }` (at most 200 characters, else `400`) → `204`: stops a playback session at once |
| GET | `/api/events` | `library` | Server-Sent Events; each `data:` is a JSON `ServerEvent` |

`/api/activity`: `seasons_pending` counts seasons waiting for intro and
credits detection, `previews_pending` files waiting for seek previews
(those being made included), `previews_running` those being made now, and
`library_work` says why they wait while some do ([Library work](#library-work)).
Non-administrators see only their own sessions and the scans of libraries
they may open, with no `analysis` or `library_work`, and the counts 0.

`/api/events`: each stream gets the events of the libraries its user may
open, and the user's own events. A comment line keeps it alive every 15 s.
The token is checked again every 30 s: the stream ends once it is signed
out, expired or its account disabled, and the client connects again with a
valid token. One token may keep 16 streams open; another answers `429`.

### Stopping a stream

`DELETE /api/sessions/{id}` ends a playback session from somewhere other
than its player. Administrators stop any session (the dashboard's *Stop*);
anyone else, their own (on another of their devices). Someone else's
answers `403`, an unknown one `404`, one stopped already `204`. Its
transcode and HLS end at once. Then:

- the user's event streams get `{ type: "session_stopped", session_id,
  message, by_admin }`. `message` is the one given (trimmed) or `null`;
  `by_admin` is true when an administrator stopped another user's session.
  Only that user's streams get it; `sessions_changed` goes to everyone, as
  when any session ends;
- for 30 minutes, the session's next requests answer `410`
  `{ code: "session_stopped", error: "session_stopped", message }`: its
  status, playlists and segments, and progress reports naming it (which
  still keep the place). So a player without the event stream stops too.
  Closing it (`DELETE /api/playback/{id}`) answers `204`;
- the player stops, leaves, and says "Playback was stopped by the server's
  administrator", with the message if any.

The compatibility API's `GET /status/sessions/terminate?sessionId=&reason=`
(Tautulli's) does the same for administrators, with `reason` as the
message. A player's progress reports through the compatibility API don't
restart its stopped session.

### Watch state on other devices

`{ type: "user_state_changed", item_ids, children }` goes to the user's own
event streams (the device that made the change included) when their watch
state changes in a way their screens show:

- something marked watched or unwatched (`POST /api/items/{id}/watched`,
  `…/unwatched`, the compatibility API's `/:/scrobble` and `/:/unscrobble`),
  unless nothing changed (marked watched again);
- a progress report (Pley's, or a compatibility API player's `/:/timeline`
  and `/:/progress`) that crosses 90 %, starts a resume point (15 s in) or
  clears one, or pauses or stops the player somewhere new (where *Play* on
  another device starts). Not the reports that only keep the place every
  ~10 s, nor late ones that change nothing (`age_ms`);
- a title taken out of Continue Watching, or back in it as it plays again.

`item_ids` is the item, then its parent and grandparent: an episode with its
season and show, a track with its album and artist, whose counts and On
Deck follow it. A show, season, album or artist marked as a whole sends one
event for itself with `children: true`: everything under the first id
changed too, unlisted.

Clients then fetch Home and the libraries' rows (Continue Watching) again,
and what shows those items: their pages, children, On Deck, and lists
holding them. A play reaching 90 % also sends `library_changed`. Pley's apps
gather both for 0.3 s and ask once, and leave alone what they fetched after
the event came (their own change).

### Request events

A [request](#requests)'s events say whom they are for: the accounts they
name (its maker, its followers) and whether managers get them. A stream gets
one when its account is named, or when it is a manager's and the event is
for managers. An app's stream gets none without `requests`, and those for
managers only on an administrator's account whose token has `approvals`
too. Whether a stream is a manager's is checked again as its session is
(an account made an administrator, or no longer one).

- `requests_changed`: a request was made, followed or no longer, approved,
  declined, added by hand, sent, failed, partly or wholly available,
  withdrawn or deleted: to its maker, its followers and managers (those it
  had before, for one withdrawn, deleted or no longer followed).
  `request_id`, `kind` and `tmdb_id` are all `null` when many changed: to
  an account's other streams after `POST /api/requests/seen` (not those of
  the session that called it), and to managers and those who inherited one
  when an account with requests is deleted.
- `request_available`: a request became available (`complete: true`), or
  more of a show's seasons did (`complete: false`); `seasons` are a show's
  newly ready ones (`null` for a movie). One event each to its maker and to
  each follower who can open the library's title, with `item_id` where that
  account may open it (`null` for a maker who can't); other followers get
  only `requests_changed`.
- `request_decided`: a pending or declined request was approved (by a
  manager, marked as added by hand too, or by an account approved as it
  asks that asked for it too), or a request was declined: to its maker
  unless they decided it, with `reason` when declined; and when declined,
  to its followers but the one who declined it, with `reason` `null`.
- `request_created`: a new request, `pending` or approved as made: to
  managers but its maker.
- `request_failed`: a request couldn't be sent to Radarr or Sonarr and
  waits for a manager: to managers (`retry_at` is always `null`).
- `requests_settings_changed` has no audience: see the union below.

Clients fetch the request summary again on each, and what shows that
request (`GET /api/requests/{id}` when only it shows). A title's badge
comes from the title (`GET /api/discover/{kind}/{tmdb_id}`), never from
one of its requests: it depends on all of them and on who asks. Nothing
is replayed, so a stream that connects again fetches the summary too.

```ts
interface AnalysisStatus {          // intro and credits detection of one season
  item_id: Id;                      // the season
  title: string;                    // "Show — Season 1"
  done: number;                     // audio fingerprints computed
  total: number;
}

interface ScanStatus {
  library_id: Id;
  library_name: string;
  phase: "queued" | "walking" | "probing" | "metadata" | "done";
  processed: number;
  total: number;
  started_at: Timestamp;
}

interface SessionInfo {
  session_id: string;
  user_id: Id;
  username: string;
  item: ItemSummary;
  method: "direct" | "remux" | "transcode";
  position_ms: number;
  state: "playing" | "paused" | "stopped";
  started_at: Timestamp;
  updated_at: Timestamp;
  transcode: null | TranscodeInfo;
}

interface TranscodeInfo {
  job_id: string;
  state: "starting" | "running" | "idle" | "finished" | "failed";
  video: "copy" | "transcode" | "none";
  audio: "copy" | "transcode" | "none";
  video_in: string | null;      // "HEVC 3840×2160 Dolby Vision P8.1", "… Dolby Vision P7 FEL", "… HDR10+"
  video_out: string | null;     // "H.264 1920×1080"; "HEVC 3840×2160 HDR10" when HDR is kept
  audio_in: string | null;      // "EAC3 5.1"
  audio_out: string | null;     // "AAC 2.0"
  encoder: string | null;       // "h264_nvenc", "libx264"
  preset: string | null;        // the software encoder's current preset (the automatic one changes)
  decoder: string | null;       // "cuda", "vulkan", "cpu", …
  pipeline: string | null;      // "gpu" (zero-copy), "hybrid", "cpu" or "copy"
  tonemap: string | null;       // "libplacebo" | "opencl" | "zscale" when HDR/DV is tone-mapped to SDR
  dolby_vision: string | null;  // remuxes: "kept" (named in master.m3u8), "converted to 8.1" (profile 7)
  hdr10plus_stripped: boolean;  // the remux leaves out HDR10+ (Dolby Vision + HDR10+ streams)
  subtitle_burn: boolean;
  speed: number | null;         // FFmpeg speed multiplier (2.5 = 2.5× real time)
  fps: number | null;
  transcoded_ms: number;        // how far FFmpeg got (absolute media position)
  segments_ready: number;
  segments_total: number;
  bitrate_out: number | null;   // what a conversion aims for, video and audio together (bits per second);
                                // null for copied video
  last_error: string | null;
  rendition: number;            // a stream in several sizes: the one the player takes now (0 = the largest)
  variants: number;             // how many smaller sizes it has (0: one size)
}

type ServerEvent =
  | { type: "scan_started"; library_id: Id }
  | { type: "scan_progress"; library_id: Id; phase: ScanStatus["phase"]; processed: number; total: number }
  | { type: "scan_finished"; library_id: Id; added: number; updated: number; removed: number }
  | { type: "library_changed"; library_id: Id }      // items or metadata changed: refetch
  | { type: "libraries_changed" }                    // a library created, updated or deleted, or the order
                                                     // changed. To one user's streams only: their own order,
                                                     // or what they may see (is_admin, all_libraries,
                                                     // library_ids, max_age changed)
  | { type: "sessions_changed" }
  | { type: "watchlist_changed" }                    // the user's My List changed (their streams only)
  | { type: "user_state_changed"; item_ids: Id[]; children: boolean } // their watch state (their streams only)
  | { type: "session_stopped"; session_id: string; message: string | null; by_admin: boolean } // theirs only
  | { type: "live_channels_changed" }                // Live TV's sources, channels or guide changed: refetch
  | { type: "live_tuners_changed" }                  // a channel was tuned or let go, or its viewers changed
  | { type: "requests_changed"; request_id: Id | null; kind: TitleKind | null; tmdb_id: number | null }
                                                     // a request changed; all three null: many did (Request events)
  | { type: "request_available"; request_id: Id; title: string; kind: TitleKind; tmdb_id: number;
      item_id: Id | null; poster_url: string | null; seasons: number[] | null; complete: boolean }
                                                     // ready, or more of a show's seasons are (`seasons`: those);
                                                     // item_id: where this account may open it
  | { type: "request_decided"; request_id: Id; title: string; kind: TitleKind; tmdb_id: number;
      status: "approved" | "declined"; reason: string | null } // reason: declined, to its maker only
  | { type: "request_created"; request_id: Id; title: string; kind: TitleKind; tmdb_id: number;
      seasons: number[] | null; requested_by: RequestUser; status: "pending" | "approved" } // to managers
  | { type: "request_failed"; request_id: Id; title: string; kind: TitleKind; reason: string;
      retry_at: null }                               // to managers: it waits for one
  | { type: "requests_settings_changed" };           // fetch the request summary again: requests were turned on or
                                                     // off, their limits or the TMDB key changed (every stream, apps'
                                                     // included), or one account's request_access, role, age limit
                                                     // or libraries did (its streams only)
```

## Live TV

IPTV channels and their guide, watched through the server. The server holds
the connections to the provider: a channel watched on several devices
takes one connection, read once and given to every player of it. A source's
`max_connections` is how many channels it may pull at once; a channel
nobody watches any more is let go 20 s after the last viewer left (at once
when another channel needs the connection).

**Who may watch.** Administrators always (their apps too). Others as
`LiveSettings` says: `access` is `everyone`, `home` (accounts in the
household) or `admins`; profiles with an age limit only with `age_limited`;
`allowed_users` and `denied_users` override both. Every route below but
`GET /api/live` answers `403` to an account Live TV isn't open to.

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/live` | `library` | `{ available, allowed, channels, sources, recent: Id[] }`: whether to show Live TV (`allowed`: open to this account; `available`: and it has channels), the channel count, the sources set up (administrators; else 0) and the 12 channels watched last, newest first |
| GET | `/api/live/channels` | `library` | `LiveChannel[]`, the shown channels by number, each with what's on now and next |
| GET | `/api/live/groups` | `library` | `{ total, favorites, recent, groups: LiveGroup[] }`: the groups in the lineup's order, and how many channels in all, marked and watched lately |
| GET | `/api/live/browse` | `library` | `?category=all\|favorites\|recent\|group&group=&q=&offset=&limit=&around=` → `{ total, offset, channels: LiveChannel[] }`: a page (100 by default, 500 at most) of a category's channels matching `q` (a name, a number's beginning, or the title of what's on); `around` gives the page with that channel in it |
| GET | `/api/live/channels/{id}` | `library` | `LiveChannel` |
| GET | `/api/live/channels/{id}/logo` | `library` | the logo, kept a week at a time (no larger than 1 MB and 400 px; one drawn dark on a see-through background is made light, for dark screens); `?w=` resizes it, a see-through one staying see-through |
| GET | `/api/live/channels/{id}/programs/{start}/image` | `library` | a programme's picture; `?w=` resizes it |
| GET | `/api/live/guide` | `library` | `?from=&to=&ids=` (unix seconds; default half an hour ago to six hours on, two days at most; `ids`: the channels wanted, with commas between, else every one) → `{ from, to, programs: { [channel_id]: LiveProgram[] } }` |
| PUT | `/api/live/channels/{id}/favorite` | `play` | `204` |
| DELETE | `/api/live/channels/{id}/favorite` | `play` | `204` |
| POST | `/api/live/channels/{id}/watch` | `play` | optional `{ client_id?, keyed_urls?, take_over? }` → `{ session_id, url, kind: "hls", channel: LiveChannel }`: `url` is the channel's live HLS playlist; with `keyed_urls`, an address carrying a key of its own ([Keyed URLs](#keyed-urls)); `take_over`: see Watching, below |
| GET | `/api/live/channels/{id}/stream.ts` | `play` | `?client_id=&take_over=`: the channel's MPEG-TS (`video/mp2t`) as the provider sends it, for as long as it is read (VLC, Kodi, TVs that read MPEG-TS) |
| GET | `/api/live/sessions/{session}/plan` | `play` | `LivePlan \| null`: what the transcoder makes of the channel, waited for up to 3 s; `null` when not known in time. A player that reads MPEG-TS plays a channel whose video and audio are copied from `stream.ts` (with a client id of its own), sooner than its first segments |
| GET | `/api/live/sessions/{session}/{file}` | `play` | `index.m3u8` or a segment of a watching session; `410 session_stopped` once the session is over, with why as `message` |
| GET | `/api/live/media/{key}/{file}` | `public` (the key) | the same, for a session watched with `keyed_urls` |
| DELETE | `/api/live/sessions/{session}` | `play` | `204`: stops watching; `404` when not yours (administrators: any) or already gone |
| GET | `/api/live/settings` | `admin` | `LiveSettings` |
| PUT | `/api/live/settings` | `admin` | `LiveSettings` → `LiveSettings`; `400` outside the ranges below |
| GET | `/api/live/sources` | `admin` | `LiveSource[]` |
| POST | `/api/live/sources` | `admin` | `LiveSourceInput` → `201 LiveSource`; its channels and guide are read in the background |
| POST | `/api/live/sources/test` | `admin` | `LiveSourceInput & { source_id? }` → `{ channels, groups, guide_url, account }` without saving; `source_id` lends a saved source's password |
| PUT | `/api/live/sources/{id}` | `admin` | `LiveSourceInput` → `LiveSource`; no `password` keeps the saved one |
| DELETE | `/api/live/sources/{id}` | `admin` | `204`, with its channels |
| POST | `/api/live/sources/{id}/refresh` | `admin` | `202`: reads its channels and guide again now |
| GET | `/api/live/lineup` | `admin` | `?source_id=` → `LiveLineupChannel[]`, hidden ones too |
| PATCH | `/api/live/lineup` | `admin` | `{ ids?: Id[], groups?: string[], filter?: { source_id?, group?, state?, q? }, hidden?, name?, number?, guide_id? }` → `204`: those channels, every one of `groups` and every one `filter` keeps; `""` gives back the source's own value; 20,000 ids and 5,000 groups at most |
| GET | `/api/live/lineup/page` | `admin` | `?source_id=&group=&state=all\|shown\|hidden\|no-guide\|offline&q=&offset=&limit=` → `{ total, hidden, offset, channels: LiveLineupChannel[] }`: a page of the lineup, filtered |
| GET | `/api/live/lineup/groups` | `admin` | `(LiveGroup & { hidden, offline })[]`: every group, hidden and offline channels counted |
| GET | `/api/live/tuners` | `admin` | `LiveTuner[]`: the channels being pulled and who watches them |
| DELETE | `/api/live/tuners/{channel}` | `admin` | `204`: stops pulling the channel, for everyone watching it |

```ts
interface LiveChannel {
  id: Id; source_id: Id; name: string; number: string | null; group: string;
  source_uid: string;                  // the source's short id
  logo: string | null;                 // the logo's address on this server
  favorite: boolean; has_guide: boolean;
  now: LiveProgram | null; next: LiveProgram | null;
}
interface LiveGroup {
  name: string;
  region: string | null;               // "UK" of "UK| Sport", "[UK] Sport", "UK: Sport"
  label: string;                       // the name without its region: "Sport"
  count: number;
}
interface LiveProgram {
  channel_id: Id; start: number; stop: number; title: string;
  subtitle: string | null; description: string | null; categories: string[];
  image: string | null; season: number | null; episode: number | null;
  is_new: boolean; is_live: boolean; rating: string | null;
}
interface LiveSourceInput {
  name: string; kind: "m3u" | "xtream";
  url: string;                         // the playlist (m3u) or the Xtream server
  username?: string; password?: string; // Xtream
  guide_url?: string;                  // XMLTV; empty: the playlist's own, or Xtream's
  user_agent?: string; max_connections?: number /* 1 */; enabled?: boolean /* true */;
}
interface LiveSource extends Omit<LiveSourceInput, "password"> {
  id: Id; uid: string;                 // uid: its short id, as in LiveChannel.source_uid
  password_set: boolean; created_at: number;
  channels_at: number | null; channels_error: string | null;
  guide_at: number | null; guide_error: string | null;
  account: { status: string;           // Xtream: "Active", "Expired", "Banned", "Disabled"
             expires_at: number | null; max_connections: number | null;
             active_connections: number | null; trial: boolean } | null;
  channel_count: number; hidden_count: number; guided_count: number;
  connections_in_use: number; refreshing: boolean;
}
interface LiveSettings {                // defaults in /* */
  access: "everyone" | "home" | "admins"; /* "everyone" */
  allowed_users: Id[];                 // may watch whatever `access` says
  denied_users: Id[];                  // may not (administrators always may)
  age_limited: boolean;                // /* false */ profiles with an age limit see Live TV too
  deinterlace: boolean;                // /* true */ interlaced channels are made progressive
  max_height: number | null;           // /* null */ transcoded channels at most this many lines, 240-2160
  timeshift_minutes: number;           // /* 5 */ how far back a channel can be paused and rewound, 1-60
  refresh_hours: number;               // /* 24 */ channels and guide read again this often, 1-168
  guide_days: number;                  // /* 7 */ days of guide kept ahead, 1-14
  hide_offline: boolean;               // /* true */ channels that don't play, and divider rows, left out
}
interface LiveLineupChannel {          // a channel as administrators see it
  id: Id; source_id: Id; source_name: string;
  name: string; number: string | null; group: string; logo: string | null;
  guide_id: string;                    // the guide channel it uses
  hidden: boolean; has_guide: boolean;
  source_number: string | null; source_guide_id: string; // as the source has them
  matched_guide_id: string | null;     // the guide's channel of its name, when the source names none
  custom_name: string | null; custom_number: string | null; custom_guide_id: string | null; // as set
  offline: boolean; offline_since: number | null;
}
interface LivePlan {
  video_codec: string | null;          // the input's: "h264", "mpeg2video", "hevc"…
  width: number | null; height: number | null; frame_rate: number | null;
  interlaced: boolean;
  audio_codec: string | null;          // the input's: "aac", "mp2", "ac3"…
  audio_channels: number | null;
  video: string;                       // "copy", or the encoder: "h264_nvenc", "libx264"…
  audio: "copy" | "aac" | "none";
}
interface LiveTuner {                  // a connection in use
  channel_id: Id; channel_name: string; source_id: Id; source_name: string;
  started_at: number;
  connected: boolean;                  // the provider is sending
  error: string | null;
  bytes_in: number; kbps: number;      // what the provider sends
  viewers: { session_id: string; user_id: Id; username: string;
             kind: "hls" | "stream"; started_at: number }[];
  idle: boolean;                       // nobody watches: it goes soon
  plan: LivePlan | null;               // what the transcoder makes of it, for HLS viewers
}
```

- **Watching.** `POST …/watch` tunes the channel when nobody watches it
  yet. A `client_id` ends what that device watched before, so zapping
  doesn't hold two connections. When every connection of its source is in
  use (and none is idle), it answers `409` with
  `connections: { channel_id, channel_name, watchers: string[], people, yours, can_take_over }[]`
  besides `code` and `message`. `watchers` names who watches, for
  administrators only; `yours`: only the asker watches it. Asking again
  with `take_over` set to a `channel_id` whose `can_take_over` is true (all
  its viewers are the asker, or the asker is an administrator and none of
  them is) ends that channel's viewers and takes its connection; their
  sessions answer `410` with why. Taking over another's connection
  otherwise: `403`.
- **The stream.** H.264 is sent as it is; other video, and interlaced
  video while `deinterlace` is on, is made H.264 by the transcoder; audio
  that isn't AAC is made AAC stereo. Every player of a channel reads the
  same HLS stream, made once.
- **Leaving.** An HLS viewer that asked for nothing for 45 s has left;
  `DELETE /api/live/sessions/{session}` says so at once.
- **Large lineups.** Providers list tens of thousands of channels in
  hundreds of groups. Apps list them by group (`groups`), a page at a
  time (`browse`), and ask the guide for the channels in view (`ids`);
  `GET /api/live/channels` and a guide without `ids` give every channel,
  which is fine for a few hundred.
- **Offline channels.** A channel the provider gives nothing for, however
  often it is asked (no answer, a 404, no data; not a refusal of the
  account), is offline until it plays again; so is a provider's divider
  row ("##### UK #####"). While `hide_offline` is on they are left out of
  every list, the guide and the counts, but can still be watched by id.
  Offline channels are tried again in the background every six hours,
  one at a time, while nothing of their source is being watched. The
  lineup says `offline` and `offline_since` for each channel.
- **Hidden channels** are nobody's to watch, administrators' included,
  until shown again. Names, numbers and guide channels set in the lineup
  stay when the source is read again.

## Users *(admin)*

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/users` | `admin` | `User[]` |
| POST | `/api/users` | `admin` | `{ username, password?, display_name?, is_admin?, managed?, home? (true), all_libraries? (true), library_ids?, max_age?, pin?, request_access? }` → `201` `User`; managed profiles need no password and can't be administrators (`400`) |
| PATCH | `/api/users/{id}` | `admin` | `{ password?, display_name?, is_admin?, home?, disabled?, all_libraries?, library_ids?, max_age?, pin?, request_access? }` (`null` clears nullable fields) → `User` |
| DELETE | `/api/users/{id}` | `admin` | `204` (not yourself); their apps end with them, and each of their [requests](#requests) someone follows goes to its earliest follower (the others go with them) |
| GET | `/api/users/{id}/devices` | `admin` | `Device[]` |
| DELETE | `/api/users/{id}/devices` | `admin` | `204`: signs the user out everywhere, apps included |
| GET | `/api/users/{id}/apps` | `admin` | `AppGrant[]` ([Signed-in apps](#signed-in-apps)) |
| DELETE | `/api/users/{id}/apps/{grant}` | `admin` | `204`: signs that app out; `404` unless it acts as that user or they allowed it |
| DELETE | `/api/users/{id}/two-step` | `admin` | `204`: turns two-step sign-in off (for someone who lost their phone and recovery codes). While the server requires it, they are signed out everywhere and set it up again at their next sign-in |

`PATCH /api/users/{id}`:

- Pley always keeps one enabled administrator.
- A new `password` signs the user out on every device (an administrator's
  own device stays) and in every app using the account or allowed by it.
- A change to `is_admin`, `all_libraries`, `library_ids` or `max_age` sends
  `libraries_changed` to the user's streams, so their devices fetch what
  they may see again; a change to those or to `request_access`,
  `requests_settings_changed`, so they fetch the request summary again.
- An age limit added to an account whose `request_access` is `"ask"` makes
  it `"off"`, unless the same change sets `request_access`.

## Invitations

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/invites` | `admin` | `Invite[]`: `{ code, created_at, expires_at, uses_left, all_libraries, library_ids, max_age, note }` |
| POST | `/api/invites` | `admin` | `{ expires_in_days? (7; 0 = never), uses? (1, at most 1000), all_libraries? (true), library_ids?, max_age?, note? (200 characters kept) }` → `201` `Invite`; share `/invite/{code}` |
| DELETE | `/api/invites/{code}` | `admin` | `204` |
| GET | `/api/invites/{code}` | `public` | `{ server_name, expires_at }`; `404` when invalid, used up or expired |
| POST | `/api/invites/{code}/accept` | `public` | `{ username, password, display_name? }` → `{ user, token? }` (as `/api/auth/login`): a new (non-home) account with the invitation's access, signed in; with an age limit, its `request_access` is `"off"`; `404` when the invitation is invalid, used up or expired, `409` for a username taken. While the server requires two-step sign-in, the account is made but not signed in: `401 two_step_setup` with the secret, and the client goes on with `POST /api/auth/login` and its first code |

## System *(admin)*

| Method | Path | Who | What |
|---|---|---|---|
| GET | `/api/system` | `admin` | `SystemInfo` |
| PUT | `/api/system/transcode-dir` | `admin` | `{ path: string \| null }` → `SystemInfo.transcode_dir` as it now is: the folder for the next start |
| GET | `/api/system/resources?since=` | `admin` | CPU and memory use, sampled every 2 s for the last five minutes |
| GET | `/api/system/library-work` | `admin` | `WatchingHours`: when [library work](#library-work) runs |
| GET | `/api/system/storage` | `admin` | `{ blobs, blob_bytes, media_folders, media_bytes, cache_bytes }` |
| POST | `/api/system/storage/clean` | `admin` | `{ blobs_removed, media_folders_removed, subtitles_removed, bytes_freed, cache_files_removed, cache_bytes_freed }` |
| GET | `/api/system/backups` | `admin` | `{ health: { ok, problem?, checked_at }, backups: { name, path, bytes, created_at }[] }`, newest first (`ok` and `checked_at` `null` until checked) |
| POST | `/api/system/backups` | `admin` | the new backup: a checked `VACUUM INTO` copy; the last 3 are kept |
| GET | `/api/transcoder` | `admin` | `TranscoderStatus`: the transcoder's live metrics (all jobs, counters) |
| GET | `/api/settings` | `admin` | `Settings` (never with `opensubtitles_password`, `radarr_api_key` or `sonarr_api_key`) |
| PUT | `/api/settings` | `admin` | partial `Settings` → `Settings` |
| POST | `/api/settings/test` | `admin` | `{ service, api_key?, pin?, username?, password? }` → `{ ok, message, ms }` |
| POST | `/api/settings/arr` | `admin` | `{ service: "radarr" \| "sonarr", url?, api_key? }` → `ArrOptions`: what Radarr or Sonarr offers requests |
| GET | `/api/fs?path=` | `admin` | `{ path: string \| null, parent: string \| null, entries: { name, path }[] }`: directories only (`path` left out: roots and drives) |
| GET | `/api/network` | `admin` | `Network`: where Pley listens, and could |
| PUT | `/api/network` | `admin` | `{ web, plex, plex_on_web?, discovery }` → `Network` |

- **`transcode-dir`** is saved in the config file for the next start
  (`null`, or the default folder, removes `transcode_dir`). `400` for a
  relative path, a whole drive, the data folder itself, a folder Pley can't
  write in, and when the command line or the environment sets it.
- **`resources`** answers
  `{ available, interval_ms, cpus, memory_total, samples: [{ at, cpu, pley_cpu, memory, pley_memory }] }`,
  oldest first:
  - `since` (unix ms) keeps only later samples; `at` is unix ms too;
  - `cpu` and `pley_cpu`: shares of all processors' time, 0–100;
  - `memory`: bytes in use on the machine, without what only caches files;
    `pley_memory`: its processes' resident memory; `memory_total`: the
    machine's; `cpus`: its logical processors;
  - "Pley" is the server and every process it started (a managed
    transcoder and its FFmpegs);
  - `available` is false where the system can't be sampled (Windows, Linux
    and macOS can).
- **`storage`**: space used by stored pictures, generated media files and
  the cache ([STORAGE.md](STORAGE.md)). **`clean`** removes what nothing
  uses any more, and old cache files; `subtitles_removed` counts added
  subtitles whose media file is gone.
- **`PUT /api/settings`** answers `400` for a `hwaccel` or `tonemapping`
  that can't work here, when changed from what is saved:
  - VAAPI off Linux, VideoToolbox off macOS, NVENC, Quick Sync or AMF on
    macOS (for a managed transcoder, which runs on Pley's system);
  - one the transcoder found no working encoder or tone mapper for
    (`transcoder.accels`, `transcoder.tonemapping`; not checked while it
    isn't running);
  - on any hardware, `"vulkan"` with a `tonemapping` other than `"auto"` or
    `"libplacebo"`, or with `hardware_decoding` off. Its pipeline decodes on Vulkan and tone
    maps with libplacebo; the transcoder encodes everything else in
    software.

  A value saved before stays and saves with the rest; the transcoder does
  without it (software encoding, Automatic's tone mapper). Changing
  `hwaccel` to `"vulkan"` resets a saved incompatible `tonemapping` to
  `"auto"` and turns `hardware_decoding` on; a combination saved before
  stays.
- **`settings/test`** makes one request to `service` (`"tmdb"`, `"tvdb"`,
  `"tvmaze"`, `"musicbrainz"`, `"transcoder"` or `"opensubtitles"`) with the
  key and PIN given (not saved), else the saved ones. OpenSubtitles: its
  key, or with a `username`, a sign-in of that account (`password` left
  out: the saved one, for the saved username). `message` says what
  happened, for people.
- **`settings/arr`** asks Radarr or Sonarr at `url` with `api_key` (not
  saved), else the saved ones, for what [requests](#requests) can be sent
  with. The saved key goes only to the saved address: a new `url` needs its
  `api_key` (`400`). Where it can't be reached or refuses the key, `400`
  with what went wrong in `message`, for people. An error answer without
  Radarr's or Sonarr's own JSON message (`{ message }`, or validation
  errors) is reported by its HTTP status only (`HTTP 502`), as when sending
  requests.
- **`PUT /api/settings`** keeps `radarr_url` and `sonarr_url` as URLs are
  written (`HTTP://Box:7878/` is `http://box:7878`), less a trailing `/`
  (`""` = none): http or https, with a host and no password, not a
  link-local address (`169.254.0.0/16`, `fe80::/10`), else `400`; a name
  is never connected to at a link-local address it resolves to either. A new address needs its `radarr_api_key` or `sonarr_api_key` too
  (`400`: the saved key goes to the saved address only), and `""` forgets
  the key with the address. `requests` on without a TMDB key (turning them
  on, or removing the key while they are), a limit above 1000, and a
  `radarr_minimum_availability` other than those below answer `400`. A
  change to `requests`, `request_limit_movies`, `request_limit_seasons` or
  `tmdb_api_key` sends `requests_settings_changed` to every event stream.
- **`PUT /api/network`** answers once the new listeners listen or have
  tried for a moment (3 s at most). Saved in the config file for the next
  start (`plex_bind`: `"web"`, `"off"` or the addresses). `400` for
  addresses Pley can't listen on together, `plex_on_web` with `plex`
  addresses too, more than 32 addresses of a kind, and a change to what the
  command line or the environment sets; `503` when the change can't be
  made.

```ts
interface Network {
  web: string[];          // the web app and API: "127.0.0.1:7539", "192.168.1.10:7539", "0.0.0.0:7539"
  plex: string[];         // the compatibility API on addresses of its own ("0.0.0.0:32400");
                          // [] = on Pley's port (plex_on_web) or off
  plex_on_web: boolean;   // the compatibility API answers its clients on Pley's port (the default);
                          // never with addresses in `plex`
  discovery: boolean;     // answer Pley's apps on the networks (never on loopback addresses)
  fixed: { web: boolean; plex: boolean; discovery: boolean }; // set by --bind/PLEY_BIND,
                          // --plex-bind/PLEY_PLEX_BIND, --no-discovery
  listeners: { kind: "web" | "plex" | "discovery"; address: string; error: string | null }[];
                          // error: why not (yet); tried again every 30 s
  adapters: { name: string; description: string | null; up: boolean; addresses: string[]; // IPv4, no loopback
              network: { name: string; kind: "public" | "private" | "domain" } | null }[]; // as Windows knows it
  config_file: string;    // where a change is saved
  windows: boolean;       // Windows Firewall decides which kinds of network reach Pley
}

interface Settings {
  server_name: string;
  tmdb_api_key: string;             // "" = movies only from file names and local artwork
  metadata_language: string;        // "en-US" (TMDB and TheTVDB; TVmaze is English)
  tv_metadata: "auto" | "tmdb" | "tvdb" | "tvmaze"; // TV source. "auto" (default): TMDB if it has a key,
                                    // else TheTVDB if it has one, else TVmaze (no key needed).
                                    // "tmdb" or "tvdb" without their key: 400. A new source refetches every show
  tvdb_api_key: string;             // TheTVDB v4 key; "" = not set
  tvdb_pin: string;                 // TheTVDB subscriber PIN, for user-supported keys
  music_metadata: boolean;          // MusicBrainz + Cover Art Archive for music libraries (default true)
  hwaccel: "auto" | "none" | "nvenc" | "qsv" | "amf" | "vaapi" | "videotoolbox" | "vulkan";
  hardware_decoding: boolean;       // decode on the GPU when possible
  tonemapping: "auto" | "libplacebo" | "opencl" | "zscale" | "none"; // HDR10/HLG/Dolby Vision to SDR
  max_transcodes: number;
  transcode_preset: "ultrafast" | "superfast" | "veryfast" | "faster" | "fast" | "medium";
  throttle_transcodes: boolean;     // real time once a 120 s buffer is built; paused 90 s ahead of the player
  transcode_in_memory: boolean;     // keep throttled transcodes' segments in RAM where the transcoder can
                                    // (Linux's /dev/shm, within a budget), else on disk (default true).
                                    // Turning it on where `transcoder.memory.available` is false: 400;
                                    // saved on before, it stays
  scan_interval_minutes: number;    // 0 disables periodic scans
  watch_libraries: boolean;         // pick up file changes automatically
  detect_markers: boolean;          // find intros and credits in the background (default true)
  seek_previews: boolean;           // make seek-bar thumbnails when an item is first opened (default true)
  previews_in_background: boolean;  // also for whole libraries after scans (default true)
  library_work: "auto" | "hours" | "idle"; // when those and intro detection run (see Library work; default "auto")
  library_work_from: number;        // hours of the day 0-23, server time, for "hours" (default 1 to 7);
  library_work_to: number;          // past midnight when `to` comes first. Above 23: 400
  library_work_max_cpu: number;     // library work waits while other programs use more of the processors
                                    // (%, 10-100, default 50; 100: never). Outside: 400
  third_party_apps: boolean;        // apps by other publishers may sign in (default false); turning it off
                                    // signs them out. Pley's own apps always may
  require_two_step: boolean;        // every account with a password uses two-step sign-in (default false):
                                    // turning it on signs out everyone without it, who sets it up at their
                                    // next sign-in. Only an administrator who uses it may turn it on (else
                                    // 400), and only while the server can use its secret.key (else 400)
  proxy_addresses: string[];        // addresses with a port Pley is reached at through a reverse proxy that
                                    // drops the port from `Host` ("https://media.example.com:9999"; see
                                    // Discovery). http or https with a port and no path, one per scheme and
                                    // host, 16 at most; empty strings are dropped. Anything else: 400
  opensubtitles_api_key: string;    // OpenSubtitles.com API key; "" = subtitle search is off. Each server's
                                    // administrator gets one from opensubtitles.com (Consumers)
  opensubtitles_username: string;   // an opensubtitles.com account, for its daily download quota; "" = the
                                    // API key's own (smaller) quota. A new username forgets the saved password
  opensubtitles_password?: string;  // PUT only, never sent back. Left out: the saved one stays; "" = none
  opensubtitles_password_set: boolean; // GET: a password is saved
  // Requests (see Requests):
  requests: boolean;                // people may ask for movies and shows (default false); needs a TMDB key
  request_limit_movies: number;     // requests an account may make in 7 days, 0-1000 (administrators:
  request_limit_seasons: number;    // no limit); 0 (default) = no limit. A show counts its seasons
  radarr_url: string;               // "http://localhost:7878"; "" = no Radarr: movies approved are added by hand
  radarr_api_key?: string;          // PUT only, never sent back. Left out: the saved one stays; "" = none
  radarr_api_key_set: boolean;      // GET: a key is saved
  radarr_quality_profile: number;   // Radarr's id; 0 = not chosen (approving fails until one is)
  radarr_root_folder: string;       // "" = not chosen (approving fails until one is)
  radarr_anime_quality_profile: number; // for anime; 0 = radarr_quality_profile
  radarr_anime_root_folder: string; // for anime; "" = radarr_root_folder
  radarr_minimum_availability: "announced" | "in_cinemas" | "released"; // when Radarr may take a release
                                    // (default "released")
  radarr_search: boolean;           // search as soon as a movie is added (default true)
  sonarr_url: string;               // "http://localhost:8989"; "" = no Sonarr
  sonarr_api_key?: string;          // as radarr_api_key
  sonarr_api_key_set: boolean;
  sonarr_quality_profile: number;
  sonarr_root_folder: string;
  sonarr_language_profile: number;  // Sonarr 3's language profile; 0 = its first (Sonarr 4 has none)
  sonarr_anime_quality_profile: number;
  sonarr_anime_root_folder: string;
  sonarr_season_folders: boolean;   // a folder for each season (default true)
  sonarr_search: boolean;           // search for the seasons asked for as soon as they're added (default true)
}

interface ArrOptions {
  version: string;                  // "5.14.0.9383"
  quality_profiles: { id: number; name: string }[];
  root_folders: { path: string; free_bytes: number | null }[];
  language_profiles: { id: number; name: string }[]; // Sonarr 3 only; [] otherwise
}

interface SystemInfo {
  name: string;
  version: string;
  commit: string;                   // the build's short hash; "unknown" without the repository
  os: string;
  arch: string;
  uptime_secs: number;
  data_dir: string;
  database: { ok: boolean | null; problem?: string; checked_at: Timestamp | null }; // integrity check at start
  secret_key_ok: boolean;           // the server can use its secret.key: without it, authenticator apps can't
                                    // be set up or checked (the log says why)
  transcode_dir: {                  // the transcoder's work folder
    in_use: string;
    next: string | null;            // the one the next start uses, when another
    default: string;                // `transcode` in the data folder
    problem: { folder: string; reason: string } | null; // the chosen one couldn't be used at start
    fixed: boolean;                 // set by the command line or the environment
  };
  transcoder: {
    available: boolean;
    managed: boolean;               // started and supervised by Pley (vs. an external URL)
    url: string | null;
    version: string | null;
    commit: string | null;
    license: string | null;         // the transcoder's, as SPDX: "AGPL-3.0-only"
    ffmpeg_version: string | null;
    encoders: string[];             // working video encoders: ["h264_nvenc", "libx264"]
    active_encoder: string | null;  // encoder new transcodes use with the current settings
    hwaccels: string[];             // FFmpeg's decoding hwaccels (`ffmpeg -hwaccels`: built in, not tried)
    accels: ("nvenc" | "qsv" | "amf" | "vaapi" | "videotoolbox")[]; // with a working H.264 encoder, best
                                    // first: the first is the one "auto" uses ([] while unavailable)
    hardware_decoding: boolean;     // some video is decoded on a GPU, so the setting changes something
    with_accel: { [hwaccel: string]: {   // what each `hwaccel` choice does here, all eight ({} while unavailable)
      hardware_decoding: boolean;   // some video is decoded on a GPU with it (software encoding gets CUDA's
                                    // decoder where an NVIDIA card encodes, VideoToolbox's on a Mac)
      software: ("h264" | "hevc" | "hdr")[]; // what it leaves to software encoders, where one works
                                    // ("hdr": HEVC Main 10, for transcodes that keep HDR)
      tonemappers: string[] | null; // the tone mappers that work with it, where the transcoder tried them
                                    // (a VAAPI encoder's, whose frames reached it, and zscale);
                                    // null: every one in `tonemapping`
    } };
    tonemapping: string[];          // available tone mappers: ["libplacebo", "opencl", "zscale"]
    dolby_vision: boolean;          // Dolby Vision profile 5 can be tone-mapped correctly
    chromaprint: boolean;           // audio fingerprints find intros and credits; without, only chapters
    throttling: boolean;            // FFmpeg can throttle transcodes (6.1 and newer)
    memory: { available: boolean; reason?: string; path?: string; budget_bytes: number } | null;
                                    // segments can be kept in RAM (`transcode_in_memory`), and at most how
                                    // much; reason: why not ("works on Linux only"); null while unavailable
    max_jobs: number;
    active_jobs: number;
    error: string | null;
  };
  counts: {
    libraries: number; movies: number; shows: number; episodes: number;
    artists: number; albums: number; tracks: number; files: number; users: number;
  };
}

interface TranscoderStatus {
  available: boolean;
  version: string | null;
  uptime_secs: number;
  max_jobs: number;
  running_processes: number;        // FFmpeg processes running now
  hw_processes: number;
  max_hw_jobs: number;
  work_dir_bytes: number;           // disk used by segments
  counters: {
    jobs_created: number; jobs_failed: number; processes_started: number; process_failures: number;
    segments_produced: number; segments_served: number; bytes_served: number;
    probes: number; probe_failures: number; probe_ms_total: number;
    thumbnails: number; subtitle_extractions: number; keyframe_scans: number;
  };
  jobs: {
    id: string;
    label: string | null;
    owner: string | null;
    state: "starting" | "running" | "idle" | "finished" | "failed";
    created_at_ms: number;
    last_access_ms: number;
    info: TranscodeInfo;
    restarts: number;
    served_bytes: number;
    warnings: string[];
  }[];
}
```

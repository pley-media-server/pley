# Compatibility API

Besides [its own API](API.md), Pley speaks a widely used media-server API,
so tools written for it work with Pley: scripts using
[`python-plexapi`](https://github.com/pushingkarmaorg/python-plexapi) and
tools built on the same API (Tautulli, Kometa, direct-play clients). This
page is for people pointing such a tool at Pley, and for anyone extending
the API. It uses the same libraries, users, watch state and playback
sessions as Pley's own API, so changes made through either show up in both.

The base URL is Pley's own address, `http://pley.local:7539` or
`https://media.example.com` (no `/api`), unless the API has a port of its
own ([Enabling and port](#enabling-and-port)). Every endpoint but
`/identity` needs a token for other apps, a compatibility token, sent as
`X-Plex-Token` ([Authentication](#authentication)).

## Contents

- [Enabling and port](#enabling-and-port)
- [Authentication](#authentication)
- [Formats](#formats)
- [Mapping](#mapping)
- [Endpoints](#endpoints)
- [Editing](#editing)
- [Searching and filtering](#searching-and-filtering)
- [Tested](#tested)
- [Not supported](#not-supported)

## Enabling and port

By default the API answers on Pley's own port (7539), wherever Pley does,
including behind a reverse proxy's HTTPS. Requests are told apart like
this:

- A request with this API's headers (`X-Plex-Token`,
  `X-Plex-Client-Identifier`, …) or `X-Plex-Token` in its query belongs to
  this API: its clients send them, browsers never do.
- So does a path only this API has (`/identity`, `/library/sections/…`,
  `/:/timeline`, a stream's `/video/:/transcode/…` segments, which players
  fetch without the headers).
- Everything else (`/`, `/search`, `/library` and `/playlists` among them)
  is the web app's. `/api/…` and `/.well-known/…` are always Pley's own.

Responses are never pages: they carry `Content-Security-Policy:
default-src 'none'` (with `sandbox` on Pley's port), `X-Frame-Options:
DENY`, `X-Content-Type-Options: nosniff`, `X-Plex-Protocol: 1.0`,
`Access-Control-Allow-Origin: *` and `Cache-Control: no-cache` unless set
otherwise. On Pley's port, paths both answer vary with `X-Plex-Token` and
`X-Plex-Client-Identifier` for caches.

Settings → Network turns the API off or gives it a port of its own:

| Where | Setting |
|---|---|
| Command line | `--plex-bind web` (the default), `--plex-bind 0.0.0.0:32400` (several addresses separated by commas) or `--plex-bind off` |
| Environment | `PLEY_PLEX_BIND` |
| `pley.toml` | `plex_bind = "web"` |

A port of its own suits tools that expect port 32400. If that port is taken
(by another media server on the same machine, say), Pley logs a warning,
runs without it, and retries every 30 seconds; Settings → Network shows why
it isn't listening.

## Authentication

This API takes compatibility tokens (`pley_ct_…`): tokens for other apps
and tools that work on this API only, never on Pley's own, and only for
what they were allowed. Make one for each app or tool, so that one can be
revoked without the others. There are two ways to get one:

- **In Pley:** Account → Tokens for other apps → New token. Name it after
  the app, choose what it may do, and copy the token into the app: Pley
  shows it once.
- **By signing in**, for tools and scripts: `POST /api/auth/login` with
  `client`, the token's name, and `scope`, what it may do (`play` without
  one). The answer has the token, whatever asked; no session is made and no
  cookie is set.

```sh
curl -s -X POST http://localhost:7539/api/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"alice","password":"…","client":"Tautulli","scope":"activity"}'
# {"user":{…},"token":"pley_ct_…"}
```

Signing in for a token goes through every check of a sign-in: attempt
limits, disabled accounts and two-step sign-in. An account with two-step
sign-in adds `"code"`, a code from its authenticator app. When the server
requires two-step sign-in and the account doesn't use it yet, the same
request sets it up, as on the web
([API.md](API.md#two-step-sign-in)), and the answer has its recovery codes
too; setting it up on the web first and making the token there is simpler.

The token is then the API's token, as tools expect:

```python
from plexapi.server import PlexServer
server = PlexServer('http://localhost:7539', '<token>')
for movie in server.library.section('Movies').search(unwatched=True):
    print(movie.title, movie.media[0].videoResolution)
```

### Scopes

| Scope | In Pley | What it allows |
|---|---|---|
| `play` | Watch | Every token has it. Browsing what the account may open, playing, its own watch state, ratings, sessions, history, playlists and play queues, and notifications about its own sessions and the libraries it may open. |
| `activity` | See everyone's activity | Every account's sessions, history and accounts, notifications about everyone's sessions, and where files are, as Tautulli needs. It changes nothing. |
| `manage` | Manage libraries | Refreshing libraries and items, editing metadata, artwork and collections, stopping anyone's session and deleting anyone's history, as Kometa needs. It shows no one else's sessions or history, nor where files are or empty collections: Kometa needs `activity` too. |

A token sees only what Pley's [access rules](API.md#access-rules) let its
account see, with the account's own watch state and Continue Watching.
Only administrators make tokens with `activity` or `manage` (others get
`403`). Those scopes are checked against the account at every request: an
account that is no longer an administrator loses what they allow at once,
and its tokens do what `play` allows. A token without `manage` gets `403`
for a change only administrators make, and one without `activity` sees what
a viewer would: its account's own sessions and history, and files' names
rather than where they are.

### Where they work

Accepted:

- this API's way: the `X-Plex-Token` header or query parameter;
- Pley's way: `Authorization: Bearer`, `X-Pley-Token` or `?token=`.

Refused (`401`):

- Pley's session tokens, which `POST /api/auth/login` answers without
  `client`: the answer says to use a token for other apps;
- the web app's `pley_token` cookie: browsers send it to this port too, and
  some of this API's endpoints change things on a GET;
- tokens of apps signed in on Pley's own page (`pley_at_…`, `pley_rt_…`)
  and the keyed addresses of their playback.

A compatibility token opens nothing of Pley's own API (`/api/…`): `401`
wherever it is sent there.

### When they end

A token ends when:

- it is revoked in Account, or a tool signs out with it
  (`POST /api/auth/logout` with the token);
- it goes 90 days unused;
- the account's password changes (its owner's change, an administrator's
  or `pley reset-password`), it is signed out everywhere, or it is deleted.

It doesn't work while the account is disabled, nor while the server
requires two-step sign-in of an account that doesn't use it, and works again
when that ends, as the account's sessions do.

### Moving from session tokens

Before compatibility tokens, this API took Pley's session tokens. It
refuses them now (`401`): a session token opens everything the account may
do on Pley's own API too, administration included. To move a tool over:

1. In Pley, open Account → Tokens for other apps → New token, name the
   token after the tool, and allow what it needs: Tautulli, See everyone's
   activity; Kometa, both See everyone's activity and Manage libraries (it
   reads where files are, and finds collections still empty); a player
   such as Plezy or Infuse, Watch alone.
2. Put the new token where the tool kept the old one (for Kometa, `token`
   under `plex` in its `config.yml`).
3. A script that signs in adds `"client"`, and `"scope"` if it needs more
   than `play`, to its `POST /api/auth/login`.

The session the old token belonged to stays signed in until it is signed
out in Account → Devices, or goes 90 days unused.

## Formats

Responses are XML `MediaContainer`s. With `Accept: application/json` they
are JSON in the same layout (`{"MediaContainer": {"Metadata": […]}}`).
Paging works through `X-Plex-Container-Start` and `X-Plex-Container-Size`,
as headers or query parameters; containers report `size`, `totalSize` and
`offset`. Paths may use `/:/` segments and trailing slashes as clients send
them.

## Mapping

- **IDs.** `ratingKey` is the Pley item id and `librarySectionID` the library
  id. Media and parts use the media file id. Stream ids are
  `file id × 10000 + stream index`.
- **Order.** Sections, and the home hubs of each, come in the user's library
  order in Pley (Settings → Libraries), else the administrators' order; new
  ones come last.
- **Types.** Movies and episodes are `Video` elements, shows, seasons,
  artists and albums are `Directory` elements, and tracks are `Track`
  elements, with the API's `type` values.
- **GUIDs.** Every item gets a `plex://<type>/<id>` guid. With
  `includeGuids=1`, and always in details, `Guid` children carry the ids
  metadata found, in the API's usual form: `imdb://`, `tmdb://` (movies and
  shows), `tvdb://` (shows) and `mbid://` (artists and albums). The `guid`
  filter finds items by any of them.
- **Editions.** Movies whose versions are tagged with an edition
  (`{edition-Director's Cut}` in the file or folder name) carry the default
  version's `editionTitle`.
- **Watch state.** `viewCount` is the play count of watched items and 0 once
  an item is marked unplayed. `viewOffset` is the resume position.
  `/:/progress` and `/:/timeline` count as Pley's own progress reports (90 %
  marks watched); `/:/scrobble` and `/:/unscrobble` mark watched and
  unwatched.
- **Sessions.** A client that reports `/:/timeline` while playing becomes a
  Pley playback session. It is listed in `/status/sessions` (with `User`,
  `Player` and `Session`, plus `TranscodeSession` for HLS) and in Pley's
  dashboard.
- **Notifications.** `/:/websockets/notifications` (token as `X-Plex-Token`
  query parameter or header) streams JSON `NotificationContainer`s:
  - `playing`: a session starts, pauses, resumes or stops, and every 10 s
    while its position moves (`PlaySessionStateNotification` with the
    `sessionKey` of `/status/sessions`);
  - `timeline`: an item was added, first with `state` 0 and `metadataState`
    `created`, then `state` 5 (what Tautulli's recently-added notifications
    wait for);
  - `activity`: library scans;
  - `status` (`LIBRARY_UPDATE`): a scan ended.

  Each token hears about its own account's sessions (everyone's with
  `activity`) and the libraries its account may open. The token is checked
  every 10 s, and the socket closes once it is revoked or ends.
- **Streaming.**
  - `/library/parts/{id}/{version}/file.ext` serves the file with range
    requests (direct play; `?download=1` adds an attachment header).
  - `/video/:/transcode/universal/start.m3u8` (or `start`; `/audio/…` for
    tracks) starts a Pley HLS session: remuxed when the video can be copied,
    transcoded otherwise (`directStream=0` forces transcoding). It honors
    `offset`, `maxVideoBitrate`, `videoResolution`, `audioStreamID` and
    burned-in `subtitleStreamID`. Without `audioStreamID`, Pley picks the
    track as for its own players
    ([API.md](API.md#the-audio-track)).
  - The master playlist points at `session/<id>/base/index.m3u8`. Session
    URLs need no token: the 128-bit session id is the credential. They stop
    working (`404`) when the account is disabled, and while the server
    requires two-step sign-in of an account that doesn't use it.
- **Preview thumbnails and chapters.** Parts of files with seek previews have
  `indexes="sd"`; the BIF index is cut from Pley's previews. Details list the
  first version's `Chapter`s, with a `thumb` once the file has previews.
- **Markers.** With `includeMarkers=1`, details list the intro and credits
  Pley found in the first version as `Marker`s (`type`, `startTimeOffset`,
  `endTimeOffset` in ms; credits are `final`), for Skip Intro and Skip
  Credits in clients.
- **Cast and crew.** Details carry `Role` (with the part as `role`),
  `Director`, `Writer` and `Producer` tags. Their `id` is the Pley person id
  and `thumb` the TMDB photo.
- **Artwork.** `thumb` and `art` attributes point at
  `/library/metadata/{id}/thumb/{version}` and `/art/{version}`.
  `/photo/:/transcode?url=…&width=…` resizes them through Pley's image cache.

## Endpoints

`GET` unless a method is given.

| Area | Endpoints |
|---|---|
| Server | `/`, `/identity`, `/servers`, `/accounts`, `/myplex/account`, `/system`, `/prefs`, `/:/prefs` (`PUT` is accepted and changes nothing), `/media/providers`, `/:/websockets/notifications`; `/clients`, `/devices`, `/activities`, `/butler` and `/updater/status` answer empty containers |
| Sections | `/library`, `/library/sections`, `/library/sections/{id}`, `…/all`, `…/albums`, `…/recentlyAdded`, `…/onDeck`, `…/search`, `…/firstCharacter`, `…/collections`, `…/{genre,year,decade,contentRating,studio,actor,director,writer,producer,label}` (filter choices), `…/refresh` and `/library/sections/all/refresh` (`manage`); `…/emptyTrash` and `…/analyze` are accepted and change nothing |
| Items | `/library/metadata/{id}` (comma-separated ids allowed), `…/extras` (trailers and other bonus material as `clip`s with the API's `extraType` and `subtype`), `…/children`, `…/allLeaves`, `…/grandchildren`, `…/related` ("More Like This" hub), `…/similar` (the same items), `…/thumb[/{version}]`, `…/art[/{version}]`, `…/refresh` (`manage`) |
| Lists | `/library/all`, `/library/onDeck`, `/library/recentlyAdded`, `/hubs`, `/hubs/sections/{id}`, `/hubs/continueWatching/items`, `/hubs/sections/{id}/continueWatching/items`, `/hubs/home/recentlyAdded`, `/hubs/search`, `/search`, `PUT /actions/removeFromContinueWatching?ratingKey=` (the movie, or the episode's show, leaves the user's Continue Watching until it is played again, as `DELETE /api/continue-watching/{id}`) |
| Collections | `/library/sections/{id}/collections` (also `…/all?type=18`), `/library/collections/{id}`, `…/children`, `…/{thumb,art,composite}[/{version}]`; changes under [Editing](#editing) |
| Playlists | `GET`/`POST /playlists` (`?type=&title=&uri=…/library/metadata/1,2`, or `smart=1&uri=…/library/sections/{id}/all?…` for a smart playlist of that listing; `?smart=0\|1` filters the list), `GET`/`PUT` (`?title=`)/`DELETE /playlists/{id}`, `GET`/`PUT /playlists/{id}/items` (`?uri=`), `DELETE /playlists/{id}/items/{playlistItemID}`, `PUT …/{playlistItemID}/move?after=`, `/playlists/{id}/composite/{version}` |
| Previews | `/library/parts/{id}/indexes/sd` (BIF), `/library/parts/{id}/indexes/sd/{offset}` (the frame at `offset` ms), `/library/media/{id}/chapterImages/{n}` |
| Playback | `/library/parts/…`, `/library/streams/{id}` (subtitles), `/:/timeline`, `/:/progress`, `/:/scrobble`, `/:/unscrobble`, `/:/rate?key=&rating=` (the user's rating, 0-10; negative clears it), `/status/sessions`, `/status/sessions/terminate?sessionId=&reason=` (`manage`: stops the session whose `Session` `id` it is, `reason` as the message its player shows, as `DELETE /api/sessions/{id}`; that client's reports don't start it again), `/transcode/sessions`, `/{video,audio}/:/transcode/universal/{start.m3u8,start,decision,stop,ping,session/…}` |
| Play queues | `POST /playQueues` (`uri=` items, a container, a section listing or a playlist, or `playlistID=`; `key=` the item to start with; `shuffle=1`; `continuous=1` plays the rest of an episode's show after it), `GET`/`PUT /playQueues/{id}` (`uri=`/`playlistID=` adds, `next=1` right after the playing item), `DELETE /playQueues/{id}/items[/{playQueueItemID}]`, `PUT …/items/{playQueueItemID}/move?after=`, `PUT /playQueues/{id}/shuffle`, `…/unshuffle`; kept in memory for a day, for the user who made them |
| History | `/status/sessions/history/all` (`accountID`, `librarySectionID`, `metadataItemID`, `viewedAt>=`; `activity` sees every account, others their own), `DELETE /status/sessions/history/{id}` (anyone's with `manage`, others their own) |
| Images | `/photo/:/transcode` |

## Editing

Tokens with `manage` edit metadata the way `python-plexapi` (and so Kometa)
does:

- `PUT /library/sections/{id}/all?type=1&id=1,2&title.value=…&title.locked=1`,
  or `PUT /library/metadata/{id}?…`, for `title`, `titleSort`,
  `originalTitle`, `year`, `summary`, `tagline`, `studio`, `contentRating`
  and `originallyAvailableAt`. Edited fields are kept through refreshes
  (Pley's locked fields, listed in details as `Field` elements with
  `locked="1"`); `field.locked=1` alone keeps the current value,
  `field.locked=0` gives the field back to the metadata agent, and a value
  sent with `field.locked=0` is written but not kept.
- Tags: `genre[0].tag.tag=Horror` adds a genre, `genre[].tag.tag-=Horror,Drama`
  removes (each name percent-encoded once more, as `python-plexapi` sends
  them). The same for `label`, which details list as `Label` and the
  `label` filter and `/library/sections/{id}/label` find, and for
  `collection`: the item joins or leaves the collection of that name in its
  library, which is made when missing. Details list it as `Collection`.
- Posters and backgrounds: `POST /library/metadata/{id}/posters` (or
  `/arts`) with the picture as the body (25 MB at most, re-encoded), or
  `?url=` and `PUT /library/metadata/{id}/poster?url=` (or `/art`) with a
  TMDB image. Other URLs are refused: the server doesn't fetch arbitrary
  addresses.
- Collections: their title, sort title (kept as set, and listed as
  `titleSort`) and summary through their section (`type=18&id=…`, as
  `python-plexapi` edits them), their poster and background through
  `POST /library/collections/{id}/posters` (or `/arts`).
- `POST /library/collections?type=1&title=…&sectionId=…&smart=0&uri=server://…/library/metadata/1,2`
  makes a collection of those items; with `smart=1` and
  `uri=server://…/library/sections/{id}/all?genre=…` a smart collection,
  filled by that listing (any filter and sort of `…/all`), as each user may
  see it. Collections report `smart` and, for smart ones, the listing as
  `content`. `PUT /library/collections/{id}/items?uri=…` adds items (or
  gives a smart collection a new listing), `DELETE …/items/{item}` removes
  one and `DELETE /library/collections/{id}` deletes the collection.

## Searching and filtering

`/library/sections/{id}/all` accepts `type` (1 movie, 2 show, 3 season, 4
episode, 8 artist, 9 album, 10 track), `sort` (`titleSort`, `addedAt`,
`originallyAvailableAt`, `year`, `rating`, `audienceRating`, `lastViewedAt`,
`viewCount`, `duration`, `index`, `id`, `updatedAt`; with `:desc`, several
comma-separated), `limit` and the API's filters with their operators:

- strings (`title`, `studio`): contains, `!=` does not contain, `==` is,
  `!==` is not, `<=` begins with, `>=` ends with;
- numbers (`year`, `decade`, `viewCount`, `duration`, `rating`, `index`,
  `id`): is, `!=`, `>>=`, `<<=`;
- tags (`genre`, `label`, `contentRating`), booleans (`unwatched`,
  `inProgress`), dates (`addedAt`, `lastViewedAt`, `originallyAvailableAt`,
  as unix time or relative like `-30d`) and `guid`;
- people (`actor`, `director`, `writer`, `producer`): a person id (the tag
  `id`, also listed by `/library/sections/{id}/actor` and the like) or a
  name; `!=` excludes.

Filters on ancestors work (`artist.id=5` on albums, `show.title=…` on
episodes). With `includeMeta=1` the response describes these filters, sorts
and fields in a `Meta` element; `python-plexapi` validates searches against
it. Pley's smart collections and smart playlists use the same query string
([API.md](API.md#collections)).

## Tested

With `python-plexapi` 4.18 the following work against the test media:

- connecting, the library and its sections, `all()`, `get()`, `search()` with
  title, `unwatched`, year, sort and paging, `recentlyAdded()`,
  `listSorts()` and `listFilterChoices()`;
- item details: media, parts, streams, guids, genres, `reload()`;
- shows (`seasons()`, `episodes()`, `episode(season=, episode=)`, `onDeck()`),
  music (`albums()`, `tracks()`, `album()`, `artist()`);
- `markPlayed()` and `markUnplayed()`, `updateProgress()`, `updateTimeline()`;
- `server.search()`, `library.search()`, `onDeck()`, hubs, `continueWatching()`;
- `systemAccounts()`, `account()`, `clients()`, `playlists()`, `history()`,
  `totalDuration` and `totalStorage`;
- `getStreamURL()` with the playlist and segments it leads to, direct-play
  range requests, `sessions()` and `transcodeSessions()`;
- a non-administrator only sees their own sessions and accounts and cannot
  refresh libraries;
- `editTitle()`, `editSummary()` and their `locked` argument, `fields`,
  `addGenre()`/`removeGenre()`, `addLabel()`/`removeLabel()`,
  `search(label=…)`, `listFilterChoices('label')`, `addCollection()`,
  `collections()` and `uploadPoster()`;
- collections' `editSummary()`, `editSortTitle()`, `uploadPoster()`,
  `createCollection()` (also `smart=True`), `filters()`, `updateFilters()`,
  `removeItems()` and `delete()`; `createPlaylist(smart=True)`;
- play queues: `PlayQueue.create()` (from items, a section listing or a
  playlist, with `continuous`), `get()`, `addItem(playNext=True)`,
  `moveItem()`, `removeItem()` and `clear()`.

## Not supported

- The vendor's cloud service: its sign-in, server claiming, sharing and
  remote access, and the apps that depend on it to find servers.
- Collections' order and mode are set in Pley, not through this API.
- Editing episodes' and seasons' artwork from URLs other than TMDB images
  (upload the file instead), and per-item preferences (`/prefs`).
- Filters on descendants (`episode.title` on shows) and on `resolution`,
  `hdr`, audio or subtitle language: ignored, so they don't narrow results.
- Player remote control, photos and live TV.
- Notifications about transcodes (`transcodeSession.*`), deleted items and
  reachability.
- Per-client capability profiles: universal transcode streams are H.264/AAC
  HLS.

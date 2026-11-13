# Compatibility APIs

Besides [its own API](API.md), Pley speaks two widely used media-server
APIs, so that apps and tools written for them work with Pley:

- the **Plex-compatible API**, for tools built on it: scripts using
  [`python-plexapi`](https://github.com/pushingkarmaorg/python-plexapi),
  library tools such as Kometa, activity monitors such as Tautulli, and
  direct-play clients. It is on by default, on Pley's own port.
- the **Jellyfin-compatible API**, for Jellyfin's apps and others built on
  its API (Findroid, JellyCon, Plezy, Infuse), and activity monitors such
  as Tracearr. It is off until an administrator turns it on.

Both use the same libraries, users, watch state and playback sessions as
Pley's own API, so what is watched or changed in one shows up in the others.
Both take tokens for other apps, which an account makes for each app or
tool, and nothing else. This page is for people pointing such an app or
tool at Pley, and for anyone extending the APIs.

## Contents

- [Turning them on](#turning-them-on)
  - [Ports and firewalls](#ports-and-firewalls)
- [Tokens for other apps](#tokens-for-other-apps)
- [The Plex-compatible API](#the-plex-compatible-api)
- [The Jellyfin-compatible API](#the-jellyfin-compatible-api)

## Turning them on

**Settings → Network** sets each API: off, on Pley's port, or on a port of
its own.

- **On Pley's port**: the API answers on Pley's own address, behind a
  reverse proxy's HTTPS too. Apps and tools connect to the address the web
  app has, such as `https://media.example.com`. Both APIs can be on Pley's
  port at once.
- **On a port of its own**: the API listens on that port of the addresses
  Pley listens on: Plex Media Server's 32400, Jellyfin's 8096, which some
  apps and tools expect.

The same is set elsewhere as:

| Where | Plex-compatible API | Jellyfin-compatible API |
|---|---|---|
| Command line | `--plex-bind web` (the default), `--plex-bind 0.0.0.0:32400` or `--plex-bind off` | `--jellyfin-bind off` (the default), `--jellyfin-bind web` or `--jellyfin-bind 0.0.0.0:8096` |
| Environment | `PLEY_PLEX_BIND` | `PLEY_JELLYFIN_BIND` |
| `pley.toml` | `plex_bind = "web"` | `jellyfin_bind = "web"` |

Several addresses are separated by commas. Set on the command line or in
the environment, a setting can't be changed in Settings → Network. A port
that is taken (by a Plex or Jellyfin server on the same computer, say) is
retried every 30 seconds, and Settings → Network shows why Pley isn't
listening there.

On Pley's port, a request that names itself the Plex-compatible API's (an
`X-Plex-…` header, or such a parameter in the query) goes to that API; the
rest is told apart as each API's section says. `/api/…` and
`/.well-known/…` are always Pley's own.

### Ports and firewalls

| Port | What answers there |
|---|---|
| TCP 7539, Pley's port | The Plex-compatible API (its default), and the Jellyfin-compatible API when it is on Pley's port |
| TCP 32400 | The Plex-compatible API, only when given a port of its own |
| TCP 8096 | The Jellyfin-compatible API, only when given a port of its own |

Neither API uses another port. Pley doesn't answer Plex's or Jellyfin's
network discovery (UDP 32410 to 32414, and 7359): apps and tools are given
the server's address.

An API answers on Pley's port or on a port of its own, not both. On
Pley's port it needs nothing more than the web app does, reverse proxy
and HTTPS included. A port of its own has to be reachable too, which
depends on how Pley runs:

| Pley runs | For a port of its own |
|---|---|
| On Windows | Nothing: the installer's firewall rule lets Pley in on any port, on the same networks as Pley's port. |
| With Docker Compose | Use the add-on, `compose.plex.yaml` or `compose.jellyfin.yaml`, rather than Settings → Network: it puts the API on that port and publishes it, on this computer only unless `PLEY_PLEX_LISTEN` or `PLEY_JELLYFIN_LISTEN` says otherwise (`PLEY_PLEX_LISTEN=0.0.0.0:32400` in `.env` lets the other devices in; [deploy/docker](../deploy/docker/README.md)). Docker lets in only the ports it publishes, so a port chosen in Settings → Network alone can't be reached. Tools in containers on a Docker network Pley is on (Tautulli, Kometa) need no published port: they reach the API where it answers, `http://pley:7539` on Pley's port or `http://pley:32400` with the add-on. |
| With `docker run` | Add `-p 32400:32400 -e PLEY_PLEX_BIND=0.0.0.0:32400` (or `-p 8096:8096 -e PLEY_JELLYFIN_BIND=0.0.0.0:8096`). With `--network host`, no `-p`: the container has the computer's ports already, and Settings → Network chooses the port (or the variable sets it). |
| On Unraid | On the default *Bridge*: edit the container, add a Port (32400 in the container and on the server, TCP) and a Variable `PLEY_PLEX_BIND` = `0.0.0.0:32400` (8096 and `PLEY_JELLYFIN_BIND` for Jellyfin's). On **br0** or **Host**: nothing, choose it in Settings → Network. |
| On a Synology NAS | With DSM's firewall on, add a rule for the port (**Control Panel → Security → Firewall → Edit Rules → Create → Ports: Custom**, TCP 32400 or 8096). The package registers Pley's own ports, not these. |
| As a service on Linux | Allow the port in the computer's firewall, if it has one: `sudo ufw allow 32400/tcp` on Ubuntu and Debian, `sudo firewall-cmd --permanent --add-port=32400/tcp` then `sudo firewall-cmd --reload` on Fedora and RHEL. |
| On a Mac | Nothing more than for Pley's port: the Mac's firewall lets apps in, not ports. |

A port of its own isn't behind the reverse proxy and has no HTTPS, so an
API used from outside your network stays on Pley's port.

A Plex Media Server or Jellyfin still running on the same computer has
32400 or 8096 already. Pley installed directly, not in a container,
tries the port again every 30 seconds, and Settings → Network says why
it can't listen there; a container that publishes a port already taken
doesn't start at all.
Choose another port, or keep the API on Pley's port, until the other
server is gone.

## Tokens for other apps

The compatibility APIs take tokens for other apps (`pley_ct_…`) alone:
tokens that work on these APIs only, never on Pley's own, and only for what
they were allowed, from where they were allowed. Make one for each app or
tool, so that one can be revoked without the others.

- **In Pley:** Account → Tokens for other apps → New token. Name it after
  the app or tool, choose what it may do and where it may be used from, and
  copy the token into it: Pley shows it once.
- **By signing in**, for Jellyfin's apps: an app signing in to the
  Jellyfin-compatible API gets a token of its own that may play, from
  anywhere ([Signing in](#signing-in)).

Account → Tokens for other apps lists each token, what it may do, where it
may be used from, and where and with what it was last used, and revokes it.

### Scopes

| Scope | In Pley | What it allows |
|---|---|---|
| `play` | Watch | Every token has it. Browsing what the account may open, playing, its own watch state, ratings, sessions, history, playlists and play queues, and notifications about its own sessions and the libraries it may open. |
| `activity` | See everyone's activity | Every account's sessions, history and accounts, notifications about everyone's sessions, and where files are, as activity monitors such as Tautulli and Tracearr need. It changes nothing. |
| `manage` | Manage libraries | Refreshing libraries and items, editing metadata, artwork and collections (those still empty listed too), stopping anyone's session and deleting anyone's history, as library tools such as Kometa need. It shows no one else's sessions or history, nor where files are: a tool that reads them, as Kometa does, needs `activity` too. |

A token is never an administrator: each scope allows what it says, checked
by name where it applies. No scope administers accounts: no token makes,
changes or deletes a user, changes who may open what, or changes how anyone
signs in or the server's settings (`PUT /:/prefs` answers `403`). A token
sees only what Pley's [access rules](API.md#access-rules) let its account
see, with the account's own watch state and Continue Watching.

Only administrators make tokens with `activity` or `manage` (others get
`403`). Those scopes are checked against the account at every request: an
account that is no longer an administrator loses what they allow at once,
and its tokens do what `play` allows. On the Plex-compatible API, a token
without `manage` gets `403` for a change only administrators make, and one
without `activity` sees what a viewer would: its account's own sessions and
history, and files' names rather than where they are. The
Jellyfin-compatible API takes every token for what `play` allows, one
with `manage` for changing collections and editing metadata too
([Playlists and collections](#playlists-and-collections),
[Editing metadata](#editing-metadata)), and one with `activity` for
seeing what everyone is playing ([Activity monitors](#activity-monitors));
it never acts as an administrator otherwise.

### Origins

A token may be limited to where it is used from: addresses
(`192.168.1.20`), networks (`10.0.0.0/24`, `fd00::/8`) and `local`, this
computer and the home network (loopback, private, link-local and
Tailscale's addresses). Without any, it works from anywhere. Up to 16.

Used from elsewhere, it is refused (`401`) as if it didn't exist, and that
use isn't listed as its last. The address is the one Pley's sign-in limits
go by: the other end of the connection, or the client a proxy on this
computer or its network names (`X-Forwarded-For`). Behind a proxy, the
limit holds only if the proxy adds the client's address to that header
([deploy](../deploy/README.md#firewall-and-remote-access)). The addresses a
token hands out in its stead (a stream's segments, a Jellyfin media source)
follow its origins too. A tool on a server of its own does well with that
server's address, and one on the home network with `local`.

### Where they work

The Plex-compatible API takes a token in `X-Plex-Token` (a header or a
query parameter), and as Pley's tokens go: `Authorization: Bearer`,
`X-Pley-Token` or `?token=`. The Jellyfin-compatible API takes one as
Jellyfin's apps send it ([Signing in](#signing-in)).

Refused (`401`):

- Pley's session tokens: the Plex-compatible API's answer says to use a
  token for other apps;
- the web app's `pley_token` cookie: browsers send it to these ports too,
  and some of the Plex-compatible API's endpoints change things on a GET;
- tokens of apps signed in on Pley's own page (`pley_at_…`, `pley_rt_…`)
  and the keyed addresses of their playback.

A token for other apps opens nothing of Pley's own API (`/api/…`): `401`
wherever it is sent there.

### When they end

A token ends when:

- it is revoked in Account, or its app signs out (`POST /api/auth/logout`
  with the token, or Jellyfin's `POST /Sessions/Logout`);
- it goes 90 days unused;
- the account's password changes (its owner's change, an administrator's
  or `pley reset-password`), it is signed out everywhere, or it is deleted.

It doesn't work while the account is disabled, and works again once the
account is enabled. Requiring two-step sign-in ends the tokens of every
account that doesn't use it, as it signs those accounts out everywhere.

### Moving from session tokens

Pley 0.1.4 and older let the Plex-compatible API take Pley's session
tokens. It refuses them now (`401`): a session token opens everything the
account may do on Pley's own API too, administration included. To move a
tool over:

1. In Pley, open Account → Tokens for other apps → New token, name the
   token after the tool, and allow what it needs: an activity monitor such
   as Tautulli, See everyone's activity; a library tool such as Kometa,
   Manage libraries, and See everyone's activity too if it reads where files
   are, as Kometa does; a player, Watch alone. Limit
   it to the address the tool runs at.
2. Put the new token where the tool kept the old one (for Kometa, `token`
   under `plex` in its `config.yml`; for Tautulli, `pms_token` in its
   `config.ini`, while it is stopped).

The session the old token belonged to stays signed in until it is signed
out in Account → Devices, or goes 90 days unused.

## The Plex-compatible API

The base URL is Pley's own address, `http://pley.local:7539` or
`https://media.example.com` (no `/api`), unless the API has a port of its
own. Every endpoint but `/identity` needs a token for other apps:

```python
from plexapi.server import PlexServer
server = PlexServer('http://localhost:7539', '<token>')
for movie in server.library.section('Movies').search(unwatched=True):
    print(movie.title, movie.media[0].videoResolution)
```

### Which tools work

A tool works when it takes a server's address and a token, as these do:
Kometa, `python-plexapi` scripts, poster and artwork tools, refresh
triggers in the manner of Autoscan (a refresh's `?path=` is ignored: the
whole library is scanned again), and Maintainerr's collections.

Kometa takes Pley's address and a token under `plex` in its `config.yml`
(`url: http://pley.local:7539`, `token: …`). The token may Manage
libraries and See everyone's activity (Kometa reads where files are, for
its asset folders and overlays), and is best limited to Kometa's address.
What Pley doesn't do for it:

- collections and overlays on what Pley doesn't filter by, such as audio
  or subtitle language, Plex's `hdr` and `dovi`, country or network: their
  searches answer `400` and Kometa reports that collection's error, rather
  than acting on the whole library;
- collection modes and promoting collections to home pages, which Pley
  doesn't have (changes answer `400`);
- what Kometa does through plex.tv: playlists' `sync_to_users`,
  `plex_watchlist`, and what it asks a Plex Pass for.

Tautulli works in part. Its setup asks for a plex.tv sign-in and has no
field for a token, so it is set up in its `config.ini` while it is stopped:
`first_run_complete = 1` under `[General]`, and under `[PMS]` Pley's
address (`pms_ip`, `pms_port`, `pms_url` with `pms_url_manual = 1`), the
server's `machineIdentifier` from `/identity` as `pms_identifier`, and as
`pms_token` a token that may See everyone's activity. It then shows what is
playing, keeps the history and lists the libraries, what was added and its
statistics. What it can't do:

- tell accounts apart: its users come from plex.tv, so every play is
  listed under its Local user;
- stop a stream: it asks plex.tv for a Plex Pass first.

Its settings say the token is no longer valid, as plex.tv never knew it;
that changes nothing. Tautulli sends the token to plex.tv as it would a
Plex one, so limit the token to Tautulli's address.

Tools that reach a server only through a plex.tv sign-in don't, as Pley has
no plex.tv account: Overseerr, Ombi, Sonarr's and Radarr's Plex connection,
PlexTraktSync and Plex Auto Languages among them.

### On Pley's port

Requests are told apart like this:

- A request with this API's headers (`X-Plex-Token`,
  `X-Plex-Client-Identifier`, …) or `X-Plex-Token` in its query belongs to
  this API: its clients send them, browsers never do.
- So does a path only this API has (`/identity`, `/library/sections/…`,
  `/:/timeline`, a stream's `/video/:/transcode/…` segments, which players
  fetch without the headers).
- Everything else (`/`, `/search`, `/library` and `/playlists` among them)
  is the web app's, or the Jellyfin-compatible API's while it is on Pley's
  port too.

Responses are never pages: they carry `Content-Security-Policy:
default-src 'none'` (with `sandbox` on Pley's port), `X-Frame-Options:
DENY`, `X-Content-Type-Options: nosniff`, `X-Plex-Protocol: 1.0`,
`Access-Control-Allow-Origin: *` and `Cache-Control: no-cache` unless set
otherwise. On Pley's port, paths both answer vary with `X-Plex-Token` and
`X-Plex-Client-Identifier` for caches.

### Formats

Responses are XML `MediaContainer`s. With `Accept: application/json` they
are JSON in the same layout (`{"MediaContainer": {"Metadata": […]}}`).
Paging works through `X-Plex-Container-Start` and `X-Plex-Container-Size`,
as headers or query parameters; containers report `size`, `totalSize` and
`offset`. Paths may use `/:/` segments and trailing slashes as clients send
them.

### Mapping

- **IDs.** `ratingKey` is the Pley item id and `librarySectionID` the library
  id. Collections and playlists have keys of their own, apart from items':
  collection `n` is `1000000000 + n` and playlist `n` is `2000000000 + n`.
  Clients address them as they address items, under
  `/library/metadata/{key}`, and `/library/collections/{id}` and
  `/playlists/{id}` take the plain ids too. Media and parts use the media
  file id. Stream ids are `file id × 10000 + stream index`.
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
  dashboard. The user's Pley apps list it among their sessions, and can
  only stop it: Plex clients get no other remote commands from Pley.
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
  every 10 s, from where the socket was opened, and the socket closes once
  it is revoked or ends.
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
    URLs need no token: the session's id (128 random bits) stands for the
    token that started it, and works while that token does and from where
    it may be used. Not once the token is revoked or ends, nor while the
    account is disabled.
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

### Endpoints

`GET` unless a method is given.

| Area | Endpoints |
|---|---|
| Server | `/`, `/identity`, `/servers`, `/accounts` (listed, never changed), `/myplex/account`, `/system`, `/prefs`, `/:/prefs` (`PUT` answers `403`: no token changes the server's settings), `/media/providers`, `/:/websockets/notifications`; `/clients`, `/devices`, `/activities`, `/butler` and `/updater/status` answer empty containers |
| Sections | `/library`, `/library/sections` (`allowSync="1"` for a token with `manage`: Kometa takes a section without it for a read-only token), `/library/sections/{id}`, `…/all`, `…/albums`, `…/recentlyAdded`, `…/onDeck`, `…/search`, `…/firstCharacter`, `…/collections`, `…/{genre,year,decade,contentRating,studio,actor,director,writer,producer,label}` (filter choices), `…/prefs` (no settings; `PUT` answers `403`), `…/refresh` and `/library/sections/all/refresh` (`manage`); `…/emptyTrash` and `…/analyze` are accepted and change nothing |
| Items | `/library/metadata/{id}` (comma-separated ids allowed; a collection's or playlist's key answers as `/library/collections/{key}` or `/playlists/{key}` does, and `…/children`, its pictures and `…/refresh` reach it too), `…/extras` (trailers and other bonus material as `clip`s with the API's `extraType` and `subtype`), `…/children`, `…/allLeaves`, `…/grandchildren`, `…/related` ("More Like This" hub), `…/similar` (the same items), `…/thumb[/{version}]`, `…/art[/{version}]`, `…/posters` and `…/arts` (the picture shown, a `Photo` named `upload://…` when it was set by hand and `metadata://…` when it is the metadata agent's), `…/file?url=` (the picture a `Photo` names), `…/themes`, `…/clearLogos` and `…/squareArts` (none), `…/refresh` (`manage`) |
| Lists | `/library/all`, `/library/onDeck`, `/library/recentlyAdded`, `/hubs`, `/hubs/sections/{id}`, `/hubs/continueWatching/items`, `/hubs/sections/{id}/continueWatching/items`, `/hubs/sections/{id}/manage` (empty: Pley promotes collections to no hubs, so `POST` promoting one and `PUT …/manage/{identifier}[/move]` answer `400`), `/hubs/home/recentlyAdded`, `/hubs/search`, `/search`, `PUT /actions/removeFromContinueWatching?ratingKey=` (the movie, or the episode's show, leaves the user's Continue Watching until it is played again, as `DELETE /api/continue-watching/{id}`) |
| Collections | `/library/sections/{id}/collections` (also `…/all?type=18`), `/library/collections/{id}` (`includePreferences=1`), `…/children`, `…/{thumb,art,composite}[/{version}]`, `PUT …/prefs`, `PUT …/items/{item}/move?after=`, `PUT …/refresh` (changes nothing: a collection has no metadata of its own to fetch); changes under [Editing](#editing) |
| Playlists | `GET`/`POST /playlists` (`?type=&title=&uri=…/library/metadata/1,2`, or `smart=1&uri=…/library/sections/{id}/all?…` for a smart playlist of that listing; `?smart=0\|1` filters the list), `GET`/`PUT` (`?title.value=` or `?title=`)/`DELETE /playlists/{id}`, `GET`/`PUT /playlists/{id}/items` (`?uri=`), `DELETE /playlists/{id}/items/{playlistItemID}`, `PUT …/{playlistItemID}/move?after=`, `/playlists/{id}/composite/{version}` |
| Previews | `/library/parts/{id}/indexes/sd` (BIF), `/library/parts/{id}/indexes/sd/{offset}` (the frame at `offset` ms), `/library/media/{id}/chapterImages/{n}` |
| Playback | `/library/parts/…`, `/library/streams/{id}` (subtitles), `/:/timeline`, `/:/progress`, `/:/scrobble`, `/:/unscrobble`, `/:/rate?key=&rating=` (the user's rating, 0-10; negative clears it), `/status/sessions`, `/status/sessions/terminate?sessionId=&reason=` (`manage`: stops the session whose `Session` `id` it is, `reason` as the message its player shows, as `DELETE /api/sessions/{id}`; that client's reports don't start it again), `/transcode/sessions`, `/{video,audio}/:/transcode/universal/{start.m3u8,start,decision,stop,ping,session/…}` |
| Play queues | `POST /playQueues` (`uri=` items, a container, a section listing, a collection or a playlist, or `playlistID=`; `key=` the item to start with; `shuffle=1`; `continuous=1` plays the rest of an episode's show after it), `GET`/`PUT /playQueues/{id}` (`uri=`/`playlistID=` adds, `next=1` right after the playing item), `DELETE /playQueues/{id}/items[/{playQueueItemID}]`, `PUT …/items/{playQueueItemID}/move?after=`, `PUT /playQueues/{id}/shuffle`, `…/unshuffle`; kept in memory for a day, for the user who made them |
| History | `/status/sessions/history/all` (`accountID`, `librarySectionID`, `metadataItemID`, `viewedAt>=`; `activity` sees every account, others their own), `DELETE /status/sessions/history/{id}` (anyone's with `manage`, others their own) |
| Images | `/photo/:/transcode` |

### Editing

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
  `label` filter and `/library/sections/{id}/label` find (labels are listed
  to tokens with `manage` only: profiles' label rules go by them; other
  tokens' `label` filters, and Jellyfin's `Tags`, go as if titles had no
  labels, while smart collections and playlists keep theirs), and for
  `collection`: the item joins or leaves the collection of that name in its
  library, which is made when missing. Details list it as `Collection`.
- `userRating.value` (0-10; empty clears it) is the token's account's own
  rating, as `/:/rate` sets it. `thumb.locked` and `art.locked` keep the
  poster and background (`1`) or give them back to the metadata agent
  (`0`), and `genre.locked=1` keeps the genres as they are.
- Anything else answers `400` naming it, and nothing of that request is
  applied: the critic and audience ratings (Pley shows TMDB's), cast and
  crew, countries and other tags, added dates and editions.
- Posters and backgrounds: `POST /library/metadata/{id}/posters` (or
  `/arts`) with the picture as the body, or `?url=` and
  `PUT /library/metadata/{id}/poster?url=` (or `/art`) with an HTTPS address
  on TMDB, TheTVDB, TVmaze, the Cover Art Archive, fanart.tv
  (`assets.fanart.tv`), ThePosterDB or GitHub's raw files
  (`raw.githubusercontent.com`, where Kometa's own images are). Other
  addresses answer `400`, so the server is never sent into its own network.
  Either way the picture (25 MB at most) is re-encoded, keeping none of the
  file's metadata but Kometa's overlay mark. A collection's key sets the
  collection's; a playlist's answers `400`, as a playlist shows its first
  entry's picture. An episode's poster is its
  still, as Plex shows it. Pley keeps one picture of each kind, the one
  shown: `PUT …/poster?url=` with the name `/posters` lists
  (`python-plexapi`'s `setPoster()`) changes nothing, and with a name it no
  longer keeps answers `400`.
- Collections: their title, sort title (kept as set, and listed as
  `titleSort`), summary and labels (as items' labels, listed as `Label`)
  through their section (`type=18&id=…`, as `python-plexapi` edits them)
  or `PUT /library/metadata/{key}`, their poster and background through
  `POST /library/metadata/{key}/posters` or
  `/library/collections/{key}/posters` (or `/arts`). Pley keeps every edit
  of a collection: locks are taken, unlocking and other fields answer
  `400`. Playlists take their title alone (`title.value`).
- Collections' order: `collectionSort` (0 release, 1 title, 2 custom),
  through `PUT /library/collections/{key}/prefs?collectionSort=` as
  `python-plexapi`'s `sortUpdate()` sends it, and read with
  `?includePreferences=1`. `collectionMode` is always `-1`, the library's
  default: Pley has no collection modes, and changing it answers `400`.
  Smart collections report `collectionFilterBasedOnUser` `1`, as they show
  each viewer what they may see, and refuse `0` and a new order. The
  preferences list only the values Pley takes, so `python-plexapi`'s
  `modeUpdate()` refuses another itself.
  `PUT /library/collections/{key}/items/{item}/move?after={item}` moves an
  item right after another (or first), from the order shown, and the
  collection keeps that order (`collectionSort` 2).
- `POST /library/collections?type=1&title=…&sectionId=…&smart=0&uri=server://…/library/metadata/1,2`
  makes a collection of those items; with `smart=1` and
  `uri=server://…/library/sections/{id}/all?genre=…` a smart collection,
  filled by that listing (any filter and sort of `…/all`), as each user may
  see it. Collections report `smart` and, for smart ones, the listing as
  `content`. `PUT /library/collections/{id}/items?uri=…` adds items (or
  gives a smart collection a new listing), `DELETE …/items/{item}` removes
  one and `DELETE /library/collections/{id}` deletes the collection.

### Searching and filtering

`/library/sections/{id}/all` accepts `type` (1 movie, 2 show, 3 season, 4
episode, 8 artist, 9 album, 10 track), `sort` (`titleSort`, `addedAt`,
`originallyAvailableAt`, `year`, `rating`, `audienceRating`, `lastViewedAt`,
`viewCount`, `duration`, `index`, `id`, `updatedAt`; with `:desc`, several
comma-separated), `limit` and the API's filters with their operators:

- strings (`title`, `studio`): contains, `!=` does not contain, `==` is,
  `!==` is not, `<=` begins with, `>=` ends with;
- numbers (`year`, `decade`, `viewCount`, `duration`, `rating`,
  `audienceRating`, `userRating`, `index`, `id`): is, `!=`, `>>=` greater,
  `<<=` less, `>=` at least, `<=` at most. To `=` and `!=` a missing value
  is `-1`, so `rating!=-1` finds what is rated;
- dates (`addedAt`, `lastViewedAt`, `originallyAvailableAt`, as unix time,
  a date or relative like `-30d`): the same day, `!=`, `>>=` after, `<<=`
  before, `>=` on or after, `<=` on or before;
- tags (`genre`, `label`, `contentRating`), booleans (`unwatched`,
  `inProgress`) and `guid`: is, `!=`;
- people (`actor`, `director`, `writer`, `producer`): a person id (the tag
  `id`, also listed by `/library/sections/{id}/actor` and the like) or a
  name; `!=` excludes.

The operator's `=` may come in the key, percent-encoded, as Kometa sends it:
`title%3D=Alien` is `title==Alien`, `year%3E=1979` is `year>=1979`. Several
values (`genre=Comedy,Horror`) match any of them. Filters are all to match,
or joined as the query string joins them: `or=1` between two filters
matches either, `and=1` both, and `push=1` … `pop=1` group them, as in
`push=1&genre=Comedy&or=1&genre=Horror&pop=1&year>>=1980`. `and` binds
tighter than `or`, and groups nest eight deep at most.

Filters on ancestors work (`artist.id=5` on albums, `show.title=…` on
episodes). A filter Pley can't apply answers `400` (`Pley can't filter by
hdr`) rather than being ignored, as a tool acting on what it finds would
act on the whole library: one on descendants (`episode.title` on shows), on
a field Pley doesn't read (`hdr`, `dovi`, `audioLanguage`,
`subtitleLanguage`, `editionTitle`, `collection`, `country`, `network`,
`unmatched` and others), or with an operator its field doesn't take
(`genre>>=`). Other query parameters, a client's own, change nothing.

`resolution` is a title's best file's (a show's best episode's): `4k`,
`1080` (up to 1440p), `720`, or `sd` for anything smaller (Plex's `576` and
`480` are SD). A movie section's `Meta` lists it, with its choices at
`/library/sections/{id}/resolution`, so Kometa's movie collections by
resolution work; for shows Kometa asks for `episode.resolution`, a filter on
descendants. Pley also reads `dynamicRange`, a title's best: `hdr` (any but
SDR), `hdr10` (HDR10, HDR10+ and Dolby Vision), or `dolby_vision`,
`hdr10plus`, `hlg` or `sdr` alone; its apps save these two in smart
collections and playlists.

With `includeMeta=1` the response describes these filters, sorts and fields
in a `Meta` element; `python-plexapi` validates searches against it. Pley's
smart collections and smart playlists use the same query string
([API.md](API.md#collections)).

Collections (`type=18`, and `/library/sections/{id}/collections`) are
filtered by `title` (as strings are) and `label` (`=`, `!=`), with or
without the `collection.` prefix, sorted by `titleSort`, `addedAt` or
`updatedAt`, and cut by `limit`. Any other filter answers `400`, never the
whole list: tools such as Kometa find the collections they made by label.
With `includeMeta=1`, `/library/sections/{id}/collections` describes them
in a `Meta` element of the `collection` type, which `python-plexapi`
validates collection searches against. A section's `label` choices name
its collections' labels too, and `/library/sections/{id}/label?type=18`
names only those.

### Tested

The server's tests (`apps/server/tests/plex*.rs`, `compatibility_tokens.rs`
and the playback tests that go through this API), and `python-plexapi` 4.18
against test media: browsing, searching and filtering, watch state,
playback and sessions, editing, collections, playlists and play queues.

### Not supported

- The vendor's cloud service: its sign-in, server claiming, sharing and
  remote access, and the apps that depend on it to find servers.
- Accounts: they are listed, never made, changed or deleted.
- Collection modes (`collectionMode`): Pley has none.
- Promoting collections to home pages and recommendations (hub
  management), theme music, logos and square art: changes answer `400`.
- Artwork from addresses on other hosts (upload the file instead), and
  per-item preferences (`/prefs`).
- Filters on descendants (`episode.title` on shows) and on `hdr`, `dovi`,
  audio or subtitle language, among others: refused
  ([Searching and filtering](#searching-and-filtering)).
- Player remote control, photos and live TV.
- Notifications about transcodes (`transcodeSession.*`), deleted items and
  reachability.
- Per-client capability profiles: universal transcode streams are H.264/AAC
  HLS.

## The Jellyfin-compatible API

Apps connect to Pley's address, or the API's own port, as they would to a
Jellyfin server, and sign in with a Pley account.

### On Pley's port

Requests are told apart like this:

- A path only Jellyfin's API has (`/Items`, `/Users`, `/Videos`, `/Audio`,
  `/Shows`, `/System`, `/Sessions`, `/UserViews`, `/Socket`, and every other
  family of its API, those Pley doesn't answer too) goes to it, whatever the
  request carries: players fetch streams and pictures without a header, and
  a path Pley doesn't answer gets Jellyfin's `404`, not a page. A request
  that names itself the Plex-compatible API's goes to that one while it is
  on Pley's port too: their paths meet (`/library`, `/system`, `/devices`).
- A path both have (`/Library`, `/Playlists`, `/Search`) goes to it only
  with a Jellyfin app's `Authorization: MediaBrowser …` header or token,
  which browsers asking for the web app never send. Those answers vary with
  `Authorization`, `X-Emby-Authorization`, `X-Emby-Token` and
  `X-MediaBrowser-Token` for caches.
- Everything else, `/api/…` included, is Pley's own.

Paths and query parameters are read in any case (`/Items` and `/items`,
`ParentId` and `parentId`), and lists repeated or joined by commas. Answers
are never pages: they carry `Content-Security-Policy: default-src 'none';
frame-ancestors 'none'; sandbox`, `X-Frame-Options: DENY` and
`X-Content-Type-Options: nosniff`, and errors are plain text. The API
needs the server's `secret.key` ([STORAGE.md](STORAGE.md)), from which its
ids are made: without one it can use, it answers `503`.

The server tells apps it is `Jellyfin Server` 12.4.0, with its name and an
id of its own (`/System/Info/Public`): apps check the version, and some
take no server older than 10.11 or 12.

### Signing in

An app signs in with a Pley account's name and password
(`POST /Users/AuthenticateByName` with `Username` and `Pw`). The answer's
`AccessToken` is a token for other apps (`pley_ct_…`) that may play, from
anywhere, named after the app and its device as its `Authorization` header
gives them ("Findroid on Phone"). No session of Pley's is made.

Signing in goes through the checks of Pley's own sign-in: attempt limits
per name and address, and disabled accounts (`403`). An account with
two-step sign-in can't sign in this way (`401`), nor one without it while
the server requires it: an app has nowhere to type the code. Quick Connect
isn't offered: such an app takes a token made in Account. The body must be
JSON (`400` otherwise), as Jellyfin's is: another site's page could send
text without asking the server first, and lock an account out with wrong
passwords.

Apps send the token as Jellyfin's apps do:

- `Authorization: MediaBrowser Client="…", Device="…", DeviceId="…",
  Version="…", Token="…"` (also written `Emby`, and sent as
  `X-Emby-Authorization`);
- `X-Emby-Token` or `X-MediaBrowser-Token`;
- `api_key` or `ApiKey` in the query.

A token sees what Pley's [access rules](API.md#access-rules) let its
account see, with the account's own watch state, and never acts as an
administrator, whatever its scopes: nothing administers through this API.
`Policy.IsAdministrator` is `true` only for a token with `activity`, because
activity monitors such as Tracearr refuse a key that isn't an
administrator's ([Activity monitors](#activity-monitors)). Apps' sign-ins
never have `activity`, so apps don't offer a dashboard that would fail. A
token with `manage` changes collections and edits metadata, as
administrators do in Pley, and `Policy.EnableCollectionManagement` says
so. `/Users` lists the caller alone and `/Users/Public` no one, so no
names are shown before signing in; a user id in a path
(`/Users/{id}/Items`) must be the caller's (`404` otherwise).

### Ids, streams and pictures

Ids are 32 lowercase hexadecimal characters, as Jellyfin writes them, and
are read with dashes or in capitals too. They are keyed with the server's
`secret.key`: an id can't be guessed or made up, and another server's ids
name nothing here. They stay the same while `secret.key` does; with a new
one, apps sign in again.

Many players open streams, subtitles and pictures without a token:

- **Media sources.** A file's id as a media source (64 hexadecimal
  characters, `MediaSourceId`) stands for the sign-in it was listed to:
  `/Videos/{id}/stream?MediaSourceId=…`, the subtitles' `DeliveryUrl` and
  the seek previews' sheets open with it alone, while that sign-in works and
  from where its token may be used, for what its user may see. Each sign-in
  is given its own.
- **HLS.** A stream's playlists and segments are reached by its session's
  id (128 random bits), for streams started through this API, while the
  token that started them works and from where it may be used: not once the
  session ends, the token is revoked or ends, or the account is disabled.
- **Pictures.** `/Items/{id}/Images/{type}` answers without a token for an
  id it was given; with a token, only for what its user may see. A
  collection's picture, its own or its first title's, is chosen by whose
  list the collection was in: its `ImageTags` say whose, so that a title
  someone may not see never shows through it.

Files are shown by their names alone (`Path: "/Film (2020).mkv"`), never
where the server keeps them.

### Playing

`/Items/{id}/PlaybackInfo` decides as Pley's own players are decided for
([QUALITY.md](QUALITY.md)). A device profile's `DirectPlayProfiles` are
read as the device's capabilities; an app that sends none (players built on
mpv or ExoPlayer) plays the files as they are. What plays as it is gets
`SupportsDirectPlay`; the rest gets a `TranscodingUrl`, an HLS stream from
Pley's transcoder (`/Videos/{id}/master.m3u8`).

The stream is decided as a Pley app's is ([API.md](API.md#version-2)), for
what its address asks. `PlaybackInfo` writes there what the device profile
says, in the names Jellyfin's server uses. From its first HLS transcoding
profile for video: the video codecs (`VideoCodec`), the sound's
(`AudioCodec`) and its channels (`TranscodingMaxAudioChannels`, the fewest
that profile and the `VideoAudio` codec profiles say), and the segments
(`SegmentContainer`: `mp4` for fMP4, else MPEG-TS). From its video codec
profiles, each codec's profiles, level, bit depth, size and dynamic ranges
(`hevc-profile`, `hevc-level`, `hevc-videobitdepth`, `hevc-maxwidth`,
`hevc-maxheight`, `hevc-rangetype`), those that apply to some videos only
(`ApplyConditions`) as they apply to the file's. An app that writes its own
address is read the same way: without codecs, H.264 and AAC; without
dynamic ranges, HDR10, HDR10+ and HLG as they are, and no Dolby Vision
without a base layer.

- The video is copied when its codec is listed and within its limits:
  H.264 and HEVC in either kind of segment, VP9 and AV1 in fMP4 only.
  Otherwise it is converted, to HEVC Main 10 where HDR stays HDR (the app
  lists HEVC Main 10 and shows the range, [API.md](API.md#hdr-transcodes)),
  else to H.264, tone-mapped. `AllowVideoStreamCopy=false` converts it
  whatever it is.
- Dolby Vision is kept and named where the app shows `DOVI` (profiles 5
  and 8, 7 too with `DOVIWithEL`, 10 for AV1), and otherwise plays as its
  base layer, or is converted, as for Pley's apps
  ([API.md](API.md#dolby-vision-remuxes)).
- The sound is copied when its codec is listed (AAC, MP3, AC-3, E-AC-3),
  else converted: to E-AC-3 or AC-3 5.1 where the app lists them and takes
  six channels, else to AAC on as many channels as it takes, up to 5.1
  (stereo when it doesn't say).
- The answer is the stream's own multivariant playlist, which names what
  it carries (`CODECS`, `SUPPLEMENTAL-CODECS` for Dolby Vision,
  `VIDEO-RANGE`), in one size; `TranscodingContainer` says `mp4` for fMP4.
  A transcoder too old to make fMP4 makes MPEG-TS.

`MaxStreamingBitrate`, `MaxHeight`/`MaxWidth`, `MaxVideoBitDepth`,
`StartTimeTicks`, `AudioStreamIndex` and a picture subtitle
(`SubtitleStreamIndex` with `SubtitleMethod=Encode`), burned in, apply.
Opened again with the same settings, as players do to probe it, it is the
same stream. `DELETE /Videos/ActiveEncodings?DeviceId=…` ends a device's
streams.

Files play with ranges at `/Videos/{id}/stream`, `/Audio/{id}/stream` or
`/Audio/{id}/universal`, and `/Items/{id}/Download` hands one over as it is.
Text subtitles, embedded or beside the file or added in Pley, are served as
WebVTT or SubRip at their `DeliveryUrl`.

A player's reports (`/Sessions/Playing`, `/Progress`, `/Stopped`) keep the
resume point and the watched state by Pley's rules, and show as playback on
Pley's Dashboard, by the device's `DeviceId`. Playback stopped from Pley
tells the device's socket (`Playstate` `Stop`), and its reports don't start
it again until it stops. `/MediaSegments/{id}` gives the intro and credits
Pley found as `Intro` and `Outro`, for Skip buttons.

`POST /UserItems/{id}/UserData` sets what it is given that differs from
the item's state: `Played`, `IsFavorite`, and `PlaybackPositionTicks`,
which moves the resume point of a movie, episode, track or extra (to 90 %
or more, the item is watched) and clears it at `0`. When the item is
marked or its place moves, `LastPlayedDate` is kept as when it, or a
show's or season's episodes, were played, today at the latest; a play it
counts is dated so in the history. Its keys go in any case, and numbers as
text, as Jellyfin reads them: sync tools such as WatchState copy a place
in a video from another server this way, sending the ticks as text. Ticks
that aren't a whole number, or a date that isn't one, are refused (`400`).

Seek-bar previews are the sprite sheets Pley makes for its own players
([API.md](API.md#playback)), at the one width it makes them. An item's
`Trickplay` (in its details, and in lists with `Fields=Trickplay`) gives
each media source's, and `/Videos/{id}/Trickplay/{width}/{n}.jpg` serves
sheet `n` (`MediaSourceId` names another version than the first);
`tiles.m3u8` there lists them as an HLS playlist of tiles. They are frames
of the video, so they open as its stream does: with a token, or a media
source's id alone, never by the item's id. Opening an item or its
`PlaybackInfo` has them made when the file has none, as Pley's own item page
does; until then `Trickplay` lists none and the sheets answer `404`.
Jellyfin for Android TV shows none: it reads a media source's id as a GUID,
and Pley's are longer.

The user's Pley apps list a Jellyfin app's playback among their sessions,
and control it as their own ([API.md](API.md#remote-control)) once the device
has said it takes media control (`SupportsMediaControl` in the query of
`POST /Sessions/Capabilities` or the body of `/Sessions/Capabilities/Full`)
and while its socket is open. Pley remembers what it said, in memory, until
it says otherwise or the server restarts. The commands reach the socket as
`Playstate`: pause as `Pause`, play as `Unpause`, a seek as `Seek` with
`SeekPositionTicks`, a skip as a `Seek` from where the player is by then,
next and previous as `NextTrack` and `PreviousTrack`. A change of track is
a `GeneralCommand`: `SetAudioStreamIndex` or `SetSubtitleStreamIndex`,
with the stream's `Index` (as the device's media streams number them;
`-1` turns subtitles off) in `Arguments`. The `AudioStreamIndex` and
`SubtitleStreamIndex` of the device's reports are what the remotes show it
plays. A device that never said so, or has no socket open, can only be
stopped.

The socket (`/socket`) asks to be kept alive every 60 seconds, and tells of
changes to the libraries the user may open (`LibraryChanged`), to the
user's watch state (`UserDataChanged`), playback stopped from Pley and the
commands of the user's remotes (`Playstate`, `GeneralCommand`). It checks
the token again every 30 seconds, from where the socket was opened, and
closes once it ends.

### Jellyfin's names for Pley's things

| Pley | Jellyfin |
|---|---|
| Library | View (`CollectionFolder`, `CollectionType` `movies`, `tvshows` or `music`) |
| Movie, show, season, episode | `Movie`, `Series`, `Season`, `Episode` |
| Artist, album, track | `MusicArtist`, `MusicAlbum`, `Audio` |
| A title's versions | its `MediaSources`, named by their edition |
| Trailers, bonus material | `LocalTrailers`, `SpecialFeatures` |
| Collection | `BoxSet` |
| Playlist | `Playlist`, each entry with a `PlaylistItemId` of its own |
| My List | Favorites: movies and shows (`400` for anything else) |
| Rating | `Likes`: a like is 10 of Pley's 10 points, a dislike 2 |
| Continue Watching | `Resume` (in progress) and `NextUp` (next episodes), with titles taken out of Continue Watching left out |
| Cast and crew | `People`, and `Person` items |

Jellyfin's grouping of libraries into views isn't offered: apps asking
which libraries could be grouped (`/UserViews/GroupingOptions`) are given
the user's movie and TV libraries, as Jellyfin gives them, and Pley's
views stay as they are. `/Library/VirtualFolders`, the libraries' folders
on disk, is Jellyfin's for administrators and answers `403` here, as
Jellyfin answers everyone else. `/Plugins` and `/Packages`, where apps
look for a plugin of their own (Infuse's library mode does), list none.

### Playlists and collections

Jellyfin's apps make and change playlists and collections as Pley's own
apps do, by Pley's rules ([API.md](API.md#playlists)):

- A playlist is its owner's alone: any token makes and changes its
  account's own, and another account's answers `404`. Making one public or
  sharing it (`IsPublic`, `Users`) answers `400`, so `GET /Playlists/{id}`
  says `OpenAccess` `false` with no `Shares`, and `/Playlists/{id}/Users`
  lists no one. A playlist is for video or for music, from its `MediaType`
  or else its first title; what doesn't fit answers `400`.
- Each entry has an id of its own, `PlaylistItemId`, in
  `/Playlists/{id}/Items` and in `/Items?ParentId=` of a playlist: an item
  may be there twice. Entries are removed and moved by it, or by the
  item's own id, which names its first entry.
  `POST /Playlists/{id}` with `Ids` puts those titles in place of the
  entries the caller sees, in that order, as apps that save a playlist as a
  whole send it; entries the caller can't see stay.
- Smart playlists and smart collections are filled by their filter: their
  entries aren't added, removed or moved (`400`).
- Collections are a library's, for everyone who sees it: only a token with
  `manage` makes and changes them (`403` otherwise), with movies and shows
  of its library. Without a library (`parentId`), a new one is in its first
  title's.
- `DELETE /Items/{id}` deletes the caller's playlist, or with `manage` a
  collection; `CanDelete` says which. A title is never deleted (`403`).

### Editing metadata

A token with `manage` (an administrator's, made in Account) edits titles'
metadata and pictures as Pley's own editor does
([API.md](API.md#items)); other tokens get `403`. Jellyfin's apps that
have an editor show it only to administrators, which apps' sign-ins never
are here, so this is for scripts and tools. A token with `activity` says it
is an administrator, but edits nothing without `manage` too.

- `POST /Items/{id}` takes a `BaseItemDto`, as apps send back the one they
  were given with their changes. The fields Pley keeps that differ from
  what it has are edited: `Name`, `ForcedSortName`, `OriginalTitle`,
  `ProductionYear`, `PremiereDate`, `Overview`, `Taglines` (the first),
  `Studios` (the first), `OfficialRating`, `Genres`, and `Tags` (Pley's
  labels, which details list to tokens with `manage` only). Each edit is locked, as in Pley, so refreshes keep it.
  Collections take `Name`, `Overview`, `ForcedSortName` and `Tags`.
- `LockedFields` lists `Name`, `Overview`, `Genres`, `Studios` and
  `OfficialRating` while Pley keeps them; sent without one of them, that
  field goes back to the metadata source. `LockData` is `true` while every
  field that holds something is kept, and sent `true` keeps them all.
  Pley's labels are never refreshed, so `Tags` needs no lock.
- A change to what Pley doesn't keep (`People`, `CommunityRating`,
  `CriticRating`, `ProductionLocations`, or a lock of `Cast`, `Runtime` or
  `ProductionLocations`) answers `400` naming it, and nothing of that
  request is applied. Ids change by Fix Match. Other fields, read-only in
  Jellyfin too, are left as they are.
- Titles' pictures (`Primary`, `Backdrop`, `Thumb`; an episode's
  `Primary` is its still; collections' are set through the
  Plex-compatible API): `GET /Items/{id}/RemoteImages?type=` lists TMDB's
  (none without a TMDB key), `POST /Items/{id}/RemoteImages/Download?type=&imageUrl=`
  takes one of them, or a picture on another host Pley takes pictures from
  ([Editing](#editing); others answer `400`), and
  `POST /Items/{id}/Images/{type}` one uploaded in base64, as Jellyfin
  takes it (25 MB at most, re-encoded). Each is kept through refreshes;
  `DELETE /Items/{id}/Images/{type}` goes back to the automatic one.
- Identify: `POST /Items/RemoteSearch/Movie` and `/Series` list TMDB's
  candidates for `SearchInfo.Name` and `Year` (`400` without a TMDB key),
  and `POST /Items/RemoteSearch/Apply/{id}` fixes the match to one, by its
  `ProviderIds.Tmdb`.
- `POST /Items/{id}/Refresh` fetches a title's metadata again, or scans a
  library.

### Activity monitors

Tracearr, and other activity monitors that speak Jellyfin's API, connect
to Pley as a Jellyfin server with a token that has `activity` (See
everyone's activity), made by an administrator in Account. Give the
monitor Pley's address and the token as its API key.

- `GET /Sessions` lists what everyone is playing: who, on which device,
  the title (with its show, season and episode), how far in it is, whether
  it is paused, and whether it plays as it is (`DirectPlay`), remuxed
  (`DirectStream`) or converted (`Transcode`). Other tokens get an empty
  list.
- `/Users/Me` says the token is an administrator
  (`Policy.IsAdministrator`), because monitors take only an
  administrator's key. That is all it does: the token still can't change
  anything without `manage`, nor administer anything at all.
- `/Users` lists the token's own account; monitors add other people when
  they start playing.
- Monitors don't get players' addresses, so they can't show where someone
  watches from. They can't stop a session or send it a message, and see
  no history from before they were connected.

### Jellyfin endpoints

| Area | Endpoints |
|---|---|
| Server | `GET /System/Info/Public`, `/System/Info`, `/System/Ping`, `/System/Endpoint`, `/Branding/Configuration`, `/Branding/Css`, `/QuickConnect/Enabled` (`false`; `/QuickConnect/Initiate` and `/Connect` answer `401`), `/Playback/BitrateTest` (`size` random bytes for a speed test, at most 100 MB) |
| Users | `POST /Users/AuthenticateByName`, `GET /Users/Me`, `/Users`, `/Users/Public`, `/Users/{id}`, `/Users/{id}/Images/Primary`, `/UserImage?userId=`, `POST /Sessions/Logout`, `/Sessions/Capabilities[/Full]`, `GET|POST /DisplayPreferences/{id}` |
| Browsing | `GET /UserViews`, `/Items` (by `ParentId`, `Ids`, `SearchTerm`, `PersonIds`, `ArtistIds`; `IncludeItemTypes`, `Recursive`, `Filters`, `Genres`, `Years`, `OfficialRatings`, `Tags`, `Studios`, `NameStartsWith`, `SortBy`, `SortOrder`, `StartIndex`, `Limit`), `/Items/{id}`, `/Items/Latest`, `/Items/Suggestions`, `/UserItems/Resume`, `/Shows/NextUp`, `/Shows/Upcoming` (none), `/Shows/{id}/Seasons`, `/Shows/{id}/Episodes`, `/Items/{id}/Similar`, `/LocalTrailers`, `/SpecialFeatures`, `/Ancestors`, `/Items/Filters`, `/Items/Filters2`, `/Genres`, `/MusicGenres`, `/Artists`, `/Artists/AlbumArtists`, `/Studios`, `/Years`, `/Tags` (none), `/Persons?SearchTerm=`, `/Search/Hints`, `/Playlists/{id}/Items`, and the `/Users/{id}/…` forms of these |
| Playlists | `POST /Playlists` (a `CreatePlaylistDto` body, or `Name`, `Ids`, `MediaType` in the query), `GET|POST /Playlists/{id}`, `POST|DELETE /Playlists/{id}/Items` (`ids`, `position`; `entryIds`), `POST /Playlists/{id}/Items/{entry}/Move/{index}`, `GET /Playlists/{id}/Users[/{user}]`, `DELETE /Items/{id}` |
| Collections | `POST /Collections?name=&ids=&parentId=`, `POST|DELETE /Collections/{id}/Items?ids=`, `DELETE /Items/{id}` (`manage`) |
| Editing | `POST /Items/{id}`, `GET /Items/{id}/MetadataEditor`, `/Items/{id}/ExternalIdInfos` (none), `POST /Items/{id}/Refresh`, `GET /Items/{id}/RemoteImages`, `/Items/{id}/RemoteImages/Providers`, `POST /Items/{id}/RemoteImages/Download`, `POST|DELETE /Items/{id}/Images/{type}[/0]`, `POST /Items/RemoteSearch/Movie`, `/Items/RemoteSearch/Series`, `/Items/RemoteSearch/Apply/{id}` (`manage`) |
| Pictures | `GET|HEAD /Items/{id}/Images/{type}[/0]` (`Primary`, `Backdrop`, `Thumb`; `maxWidth`, `fillWidth`, `width`, `tag`) |
| Watch state | `POST|DELETE /UserPlayedItems/{id}`, `/UserFavoriteItems/{id}`, `/UserItems/{id}/Rating`; `GET|POST /UserItems/{id}/UserData`, and the `/Users/{id}/…` forms |
| Playback | `GET|POST /Items/{id}/PlaybackInfo`, `/Videos/{id}/stream[.{container}]`, `/Audio/{id}/stream`, `/Audio/{id}/universal`, `/Items/{id}/Download`, `/Videos/{id}/master.m3u8`, `/Audio/{id}/master.m3u8`, `/Videos/{id}/{source}/Subtitles/{index}[/{ticks}]/Stream.{vtt|srt}`, `/Videos/{id}/Trickplay/{width}/{n}.jpg`, `/Videos/{id}/Trickplay/{width}/tiles.m3u8`, `DELETE /Videos/ActiveEncodings`, `/MediaSegments/{id}` |
| Reports | `POST /Sessions/Playing`, `/Sessions/Playing/Progress`, `/Sessions/Playing/Stopped`, `/Sessions/Playing/Ping`; `GET /Sessions` (everyone's with `activity`, none otherwise) |
| Socket | `GET /socket` |

### Jellyfin apps tested

Run against Pley with the API on Pley's port and on a port of its own:

- the server's tests (`apps/server/tests/jellyfin.rs` and the access,
  token, playback and watch-state tests that go through this API);
- Jellyfin for Android TV, Findroid on Android, JellyCon in Kodi 21 and
  Plezy on Windows: signing in, browsing, playing, resuming and the watch
  state they report;
- Jellyfin's Kotlin SDK, whose models every answer was read into.

### Not supported by the Jellyfin-compatible API

- Quick Connect, and so signing in from an app with an account that uses
  two-step sign-in.
- Administration: the dashboard, users, libraries, plugins, scheduled
  tasks and logs. Activity monitors see what is playing
  ([Activity monitors](#activity-monitors)).
- Live TV, SyncPlay, remote control of other sessions, and finding the
  server on the network (UDP 7359): apps are given its address.
- Shared and public playlists: Pley's are their owner's alone.
- Editing cast and crew, ratings and countries, which Pley takes from the
  metadata source, and searching other sources than TMDB.
- Lyrics, instant mixes, theme songs and videos, and intros before a
  title.
- Downloads converted for a device: `/Items/{id}/Download` hands over the
  file as it is.

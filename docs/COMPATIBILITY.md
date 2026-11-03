# Jellyfin-compatible API

Besides [its own API](API.md), Pley can speak Jellyfin's API, so that
Jellyfin's apps and other apps built on that API browse and play Pley's
libraries with Pley's accounts. It is off until an administrator turns it
on. It uses the same libraries, users, watch state and playback sessions as
Pley's own API, so what is watched in one shows up in the other. This page
is for people pointing such an app at Pley, and for anyone extending the
API.

## Contents

- [Turning it on](#turning-it-on)
- [Signing in](#signing-in)
- [Ids, streams and pictures](#ids-streams-and-pictures)
- [Playing](#playing)
- [Mapping](#mapping)
- [Endpoints](#endpoints)
- [Tested](#tested)
- [Not supported](#not-supported)

## Turning it on

**Settings → Network → Jellyfin-compatible API** turns it on:

- **On Pley's port**: the API answers on Pley's own address, behind a
  reverse proxy's HTTPS too. Apps connect to the address the web app has,
  such as `https://media.example.com`.
- **On a port of its own**: the API listens on that port of the addresses
  Pley listens on. Jellyfin's own is 8096, which some apps try first.

The same is set elsewhere as:

| Where | Setting |
|---|---|
| Command line | `--jellyfin-bind off` (the default), `--jellyfin-bind web` (Pley's port), or addresses of its own such as `--jellyfin-bind 0.0.0.0:8096` (several separated by commas) |
| Environment | `PLEY_JELLYFIN_BIND` |
| `pley.toml` | `jellyfin_bind = "web"` |

Set on the command line or in the environment, it can't be changed in
Settings → Network. A port that is taken is retried every 30 seconds, and
Settings → Network shows why Pley isn't listening there. Docker's
`compose.jellyfin.yaml` add-on publishes port 8096
([deploy/docker](../deploy/docker/README.md)).

On Pley's port, requests are told apart like this:

- A path only Jellyfin's API has (`/Items`, `/Users`, `/Videos`, `/Audio`,
  `/Shows`, `/System`, `/Sessions`, `/UserViews`, `/Socket`, and every other
  family of its API, those Pley doesn't answer too) goes to it, whatever the
  request carries: players fetch streams and pictures without a header, and
  a path Pley doesn't answer gets Jellyfin's `404`, not a page.
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

Older versions had a Plex-compatible API instead. It is gone:
`--plex-bind`, `PLEY_PLEX_BIND` and `plex_bind` are read and ignored, with a
warning in the log, and Settings → Network's next change takes
`plex_bind` out of `pley.toml`.

## Signing in

An app signs in with a Pley account's name and password
(`POST /Users/AuthenticateByName` with `Username` and `Pw`). The answer's
`AccessToken` is a token for other apps (`pley_ct_…`), named after the app
and its device as its `Authorization` header gives them ("Findroid on
Phone"). No session of Pley's is made.

Signing in goes through the checks of Pley's own sign-in: attempt limits
per name and address, and disabled accounts (`403`). An account with
two-step sign-in can't sign in this way (`401`), nor one without it while
the server requires it: an app has nowhere to type the code. Quick Connect
isn't offered. The body must be JSON (`400` otherwise), as Jellyfin's is:
another site's page could send text without asking the server first, and
lock an account out with wrong passwords.

Apps send the token as Jellyfin's apps do:

- `Authorization: MediaBrowser Client="…", Device="…", DeviceId="…",
  Version="…", Token="…"` (also written `Emby`, and sent as
  `X-Emby-Authorization`);
- `X-Emby-Token` or `X-MediaBrowser-Token`;
- `api_key` or `ApiKey` in the query.

Refused (`401`): Pley's session tokens, tokens of apps signed in on Pley's
own page (`pley_at_…`), and the web app's `pley_token` cookie, which
browsers send to this port too. A token for other apps opens nothing of
Pley's own API (`/api/…`).

A token sees what Pley's [access rules](API.md#access-rules) let its
account see, with the account's own watch state, and never acts as an
administrator: `Policy.IsAdministrator` is `false` for everyone, and nothing
administers through this API. `/Users` lists the caller alone and
`/Users/Public` no one, so no names are shown before signing in; a user id
in a path (`/Users/{id}/Items`) must be the caller's (`404` otherwise).

### Tokens for other apps

Account → Tokens for other apps lists each app signed in, with its device
and last use, and revokes it. **New token** makes one by hand, for an app
or script that takes a token: name it after the app, and copy it into the
app, as Pley shows it once.

A token ends when:

- it is revoked in Account, or the app signs out (`POST /Sessions/Logout`,
  or `POST /api/auth/logout` with the token);
- it goes 90 days unused;
- the account's password changes (its owner's change, an administrator's
  or `pley reset-password`), it is signed out everywhere, or it is deleted.

It doesn't work while the account is disabled, and works again once the
account is enabled. Requiring two-step sign-in ends the tokens of every
account that doesn't use it, as it signs those accounts out everywhere.

## Ids, streams and pictures

Ids are 32 lowercase hexadecimal characters, as Jellyfin writes them, and
are read with dashes or in capitals too. They are keyed with the server's
`secret.key`: an id can't be guessed or made up, and another server's ids
name nothing here. They stay the same while `secret.key` does; with a new
one, apps sign in again.

Many players open streams, subtitles and pictures without a token:

- **Media sources.** A file's id as a media source (64 hexadecimal
  characters, `MediaSourceId`) stands for the sign-in it was listed to:
  `/Videos/{id}/stream?MediaSourceId=…` and the subtitles' `DeliveryUrl`
  open with it alone, while that sign-in works, for what its user may see.
  Each sign-in is given its own.
- **HLS.** A stream's playlists and segments are reached by its session's
  id (128 random bits), for streams started through this API, while the
  token that started them works: not once the session ends, the token is
  revoked or ends, or the account is disabled.
- **Pictures.** `/Items/{id}/Images/{type}` answers without a token for an
  id it was given; with a token, only for what its user may see. A
  collection's picture, its own or its first title's, is chosen by whose
  list the collection was in: its `ImageTags` say whose, so that a title
  someone may not see never shows through it.

Files are shown by their names alone (`Path: "/Film (2020).mkv"`), never
where the server keeps them.

## Playing

`/Items/{id}/PlaybackInfo` decides as Pley's own players are decided for
([QUALITY.md](QUALITY.md)). A device profile's `DirectPlayProfiles` are
read as the device's capabilities; an app that sends none (players built on
mpv or ExoPlayer) plays the files as they are. What plays as it is gets
`SupportsDirectPlay`; the rest gets a `TranscodingUrl`, an HLS stream from
Pley's transcoder (`/Videos/{id}/master.m3u8`): the video copied when its
codec is one the request names (`VideoCodec`, H.264 or HEVC), converted to
H.264 otherwise, and the sound made AAC. `MaxStreamingBitrate`,
`MaxHeight`/`MaxWidth`, `StartTimeTicks`, `AudioStreamIndex` and a picture
subtitle (`SubtitleStreamIndex` with `SubtitleMethod=Encode`), burned in,
apply. Opened again with the same settings, as players do to probe it, it is
the same stream. `DELETE /Videos/ActiveEncodings?DeviceId=…` ends a device's
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

The socket (`/socket`) asks to be kept alive every 60 seconds, and tells of
changes to the libraries the user may open (`LibraryChanged`), to the
user's watch state (`UserDataChanged`), and playback stopped from Pley. It
checks the token again every 30 seconds and closes once it ends.

## Mapping

| Pley | Jellyfin |
|---|---|
| Library | View (`CollectionFolder`, `CollectionType` `movies`, `tvshows` or `music`) |
| Movie, show, season, episode | `Movie`, `Series`, `Season`, `Episode` |
| Artist, album, track | `MusicArtist`, `MusicAlbum`, `Audio` |
| A title's versions | its `MediaSources`, named by their edition |
| Trailers, bonus material | `LocalTrailers`, `SpecialFeatures` |
| Collection | `BoxSet` |
| Playlist | `Playlist` |
| My List | Favorites: movies and shows (`400` for anything else) |
| Rating | `Likes`: a like is 10 of Pley's 10 points, a dislike 2 |
| Continue Watching | `Resume` (in progress) and `NextUp` (next episodes), with titles taken out of Continue Watching left out |
| Cast and crew | `People`, and `Person` items |

## Endpoints

| Area | Endpoints |
|---|---|
| Server | `GET /System/Info/Public`, `/System/Info`, `/System/Ping`, `/System/Endpoint`, `/Branding/Configuration`, `/Branding/Css`, `/QuickConnect/Enabled` (`false`; `/QuickConnect/Initiate` and `/Connect` answer `401`), `/Playback/BitrateTest` (`size` random bytes for a speed test, at most 100 MB) |
| Users | `POST /Users/AuthenticateByName`, `GET /Users/Me`, `/Users`, `/Users/Public`, `/Users/{id}`, `/Users/{id}/Images/Primary`, `/UserImage?userId=`, `POST /Sessions/Logout`, `/Sessions/Capabilities[/Full]`, `GET|POST /DisplayPreferences/{id}` |
| Browsing | `GET /UserViews`, `/Items` (by `ParentId`, `Ids`, `SearchTerm`, `PersonIds`, `ArtistIds`; `IncludeItemTypes`, `Recursive`, `Filters`, `Genres`, `Years`, `OfficialRatings`, `Tags`, `Studios`, `NameStartsWith`, `SortBy`, `SortOrder`, `StartIndex`, `Limit`), `/Items/{id}`, `/Items/Latest`, `/Items/Suggestions`, `/UserItems/Resume`, `/Shows/NextUp`, `/Shows/Upcoming` (none), `/Shows/{id}/Seasons`, `/Shows/{id}/Episodes`, `/Items/{id}/Similar`, `/LocalTrailers`, `/SpecialFeatures`, `/Ancestors`, `/Items/Filters`, `/Items/Filters2`, `/Genres`, `/MusicGenres`, `/Artists`, `/Artists/AlbumArtists`, `/Studios`, `/Years`, `/Tags` (none), `/Persons?SearchTerm=`, `/Search/Hints`, `/Playlists/{id}/Items`, and the `/Users/{id}/…` forms of these |
| Pictures | `GET|HEAD /Items/{id}/Images/{type}[/0]` (`Primary`, `Backdrop`, `Thumb`; `maxWidth`, `fillWidth`, `width`, `tag`) |
| Watch state | `POST|DELETE /UserPlayedItems/{id}`, `/UserFavoriteItems/{id}`, `/UserItems/{id}/Rating`; `GET|POST /UserItems/{id}/UserData`, and the `/Users/{id}/…` forms |
| Playback | `GET|POST /Items/{id}/PlaybackInfo`, `/Videos/{id}/stream[.{container}]`, `/Audio/{id}/stream`, `/Audio/{id}/universal`, `/Items/{id}/Download`, `/Videos/{id}/master.m3u8`, `/Audio/{id}/master.m3u8`, `/Videos/{id}/{source}/Subtitles/{index}[/{ticks}]/Stream.{vtt|srt}`, `DELETE /Videos/ActiveEncodings`, `/MediaSegments/{id}` |
| Reports | `POST /Sessions/Playing`, `/Sessions/Playing/Progress`, `/Sessions/Playing/Stopped`, `/Sessions/Playing/Ping`; `GET /Sessions` (none listed) |
| Socket | `GET /socket` |

## Tested

Run against Pley with the API on Pley's port and on a port of its own:

- the server's tests (`apps/server/tests/jellyfin.rs` and the access,
  token, playback and watch-state tests that go through this API);
- Jellyfin for Android TV, Findroid on Android, JellyCon in Kodi 21 and
  Plezy on Windows: signing in, browsing, playing, resuming and the watch
  state they report;
- Jellyfin's Kotlin SDK, whose models every answer was read into.

## Not supported

- Quick Connect, and so signing in from an app with an account that uses
  two-step sign-in.
- Administration: the dashboard, users, libraries, plugins, scheduled
  tasks, logs and activity.
- Live TV, SyncPlay, remote control of other sessions (`/Sessions` lists
  none), and finding the server on the network (UDP 7359): apps are given
  its address.
- Making or changing playlists and collections, and editing metadata.
- Lyrics, seek previews (trickplay), instant mixes, theme songs and
  videos, and intros before a title.
- Downloads converted for a device: `/Items/{id}/Download` hands over the
  file as it is.

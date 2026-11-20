# Configuration

How to set Pley up once it runs. Installing it is in
[deploy/README.md](../deploy/README.md).

## First start

On start, Pley prints a link with a one-time setup code
(`http://localhost:7539/setup?code=…`; on a desktop the tray icon opens it).
Nobody without the code can claim a new server. Setup is one card, with how
far it is along its top:

1. **The account.** The administrator's username and password, and the
   setup code, filled in when the link carries it.
2. **Your server.** What Pley will use, a row each, to confirm or change:
   the server's name, Pley's language, the language of titles and summaries
   (the browser's, where TMDB and TheTVDB have it), the country (whose age
   ratings are read), a TMDB key for movies' posters and details
   (optional), and, where devices on the network can reach Pley, whether
   Apple devices find it by Bonjour (yes, but on what looks like Docker's
   own network, where they couldn't).
3. **How to start.** Bring over a Plex server's libraries, profiles and
   what each person watched ([MOVING_FROM_PLEX.md](MOVING_FROM_PLEX.md)),
   or start fresh.
4. **Your libraries**, starting fresh. A row each for Movies, TV Shows and
   Music: give each its folders, remove any you don't have, and add more
   of any type, such as Kids Movies, under their own names. Start watching
   adds each one given a folder and starts its scan; Later leaves with
   none. In a container with no media folder mounted, the step says so and
   how to mount one.

From how to start onwards, Back goes a step back; the account, once made,
stays. Everything setup asks for is in Settings afterwards. Home then shows
administrators how far the libraries' first scans are, which of them it's
on, and each library ticked off as it's done; seek previews and intro
detection wait until they are.

`pley reset-password <user> [--admin]` resets a lost password. It asks for
the new one (or reads it from standard input) and turns that account's
two-step sign-in off; on a server that requires it, it is set up again at
the next sign-in.

## Options

Options come from command-line flags, `PLEY_*` environment variables, or
`pley.toml` in the data folder ([example](../pley.example.toml)). For
example, `--plex-bind` is also `PLEY_PLEX_BIND` and `plex_bind`.

| Flag | Default | |
|---|---|---|
| `--bind` | `0.0.0.0:7539` | Listen address (7539 spells PLEY on a keypad); several separated by commas |
| `--plex-bind` | `web` (Pley's own port) | [Plex-compatible API](COMPATIBILITY.md): `off`, or addresses of its own (`0.0.0.0:32400`), several as with `--bind` |
| `--jellyfin-bind` | `web` (Pley's own port) | [Jellyfin-compatible API](COMPATIBILITY.md): `off`, or addresses of its own (`0.0.0.0:8096`), several as with `--bind` |
| `--data-dir` | `%LOCALAPPDATA%\pley`, `~/Library/Application Support/pley`, `~/.local/share/pley` | Database, caches and artwork ([STORAGE.md](STORAGE.md)) |
| `--ffmpeg`, `--ffprobe` | from `PATH` | Passed to the transcoder |
| `--transcoder-url` | | Use a transcoder running elsewhere instead of starting one |
| `--web-dir` | | Serve the web app from a folder instead of the embedded copy |
| `--log` | `info` | Log filter, e.g. `info,pley=debug`; also written to `logs/pley.log` |
| `--no-tray` | | Never show the tray icon |
| `--no-discovery` | | Don't answer apps looking for servers on the local network, nor announce Pley there by Bonjour |
| `--no-bonjour` | | Keep Bonjour off, whatever Settings → Network says: Apple's apps need the server's address typed in |

Everything else is set in the web app:

- **Settings → Server**: the server name, metadata sources and keys, scanning.
- **Settings → Transcoder**: hardware acceleration, tone mapping, whether
  video is transcoded at all, and how many streams run at once (video
  transcodes, and all of them).
- **Settings → Network**: where Pley listens (networks, port, the Plex- and
  Jellyfin-compatible APIs, discovery, Bonjour), changed while Pley runs and
  saved in `pley.toml`, unless `--bind`, `--plex-bind`, `--jellyfin-bind`,
  `--no-discovery` or `--no-bonjour` (or their `PLEY_*` variables) set it.
  Bonjour is off until turned on there, and has the networks it announces
  Pley on (`bonjour_adapters`, every network Pley listens on unless some
  are chosen, 100.64.0.0/10's only when chosen) and its host name (`bonjour_host_name`, `pley-` and the
  server id's start unless one is chosen).
- **Settings → Live TV**: IPTV sources, which channels show, and who may
  watch.
- **Settings → Requests**: Discover and requests, their limits, and Radarr and
  Sonarr ([API.md](API.md#requests)); who may ask is in **Settings → Users**.

Pley stops cleanly, giving playback a few seconds to finish, on Ctrl+C,
`SIGTERM`, the tray's Quit and, on Windows, `taskkill` without `/F`, closing
its console window, or signing out.

## Metadata sources

| Library | Source | Key |
|---|---|---|
| Movies | [TMDB](https://www.themoviedb.org) | Free API key |
| TV shows | TMDB, [TheTVDB](https://thetvdb.com) or [TVmaze](https://www.tvmaze.com) | TMDB's key; TheTVDB's key (and subscriber PIN for user-supported keys); none for TVmaze |
| Music | [MusicBrainz](https://musicbrainz.org) and the [Cover Art Archive](https://coverartarchive.org) | None |
| Audiobooks | [Open Library](https://openlibrary.org), and [Audnexus](https://audnex.us) for books with an Audible ASIN | None |

- *Automatic* uses TMDB for TV when it has a key, then TheTVDB, else TVmaze.
  Changing the source fetches every show again.
- Without a TMDB key, movies keep their file names and local artwork, or
  what their [NFO files](#nfo-files) say: no keyless source has movie
  artwork.
- TVmaze is in English and has no content ratings. Its specials are season 0,
  in air date order.
- MusicBrainz only fills in what the tags lack and allows one request a
  second, so the first look-up of a big music library takes a while.
- Audiobooks are looked up on Open Library by their title and writer, for
  a summary, year and cover. Books whose files carry an Audible ASIN tag
  (`asin`, as Libation and OpenAudible write it) are looked up by it on
  Audnexus instead, which adds narrators, the series and the book's place
  in it, and genres; it is asked in the Audible region of the server's
  country, then the United States'. While Audnexus doesn't answer, such a
  book gets Open Library's details and Audnexus is asked again after the
  next scan. Audnexus can't search, so a book without an ASIN goes to Open
  Library alone, and Pley never asks Audible itself. Both fill in only what
  the files' tags and folders lack, and covers in or beside the files win.
  A book whose files change is looked up again after the scan.

This product uses the TMDB API but is not endorsed or certified by TMDB. TV
data from TVmaze is licensed
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). TV metadata
from TheTVDB; please consider adding missing information or subscribing.
Music data from MusicBrainz is public domain (CC0); covers from the Cover Art
Archive belong to their owners. Open Library's data is public domain (CC0);
audiobook data and covers from Audnexus belong to their owners.

## NFO files

Kodi, Jellyfin, Emby, Radarr, Sonarr, tinyMediaManager and MediaElch keep a
title's metadata in an `.nfo` file next to it. Pley reads them in movie and
TV libraries, at each scan:

| Item | File |
|---|---|
| Movie | `<video name>.nfo`, else `movie.nfo` in a folder that holds no other movie |
| Show | `tvshow.nfo` in the show's folder |
| Season | `season.nfo` in the season's folder: its title and summary only |
| Episode | `<video name>.nfo`; a file of several episodes holds one `<episodedetails>` each |

- **Ids**: TMDB, IMDb, TheTVDB and TVmaze (`<uniqueid>`, or the older
  `<tmdbid>`, `<imdbid>`, `<tvdbid>`, `<id>`), or a link to the title's page
  on one of those sites, as in Kodi's NFO files that hold only a link. A
  movie or show with ids is looked up by them instead of searched for by
  name: they win over a match found by name, or an id in the names
  (`{tmdb-603}`), unless that match has one of them.
- **Fields**: title, original title, sort title, year, release date
  (`premiered`, `releasedate` or `aired`), summary (`plot`, else
  `outline`), tagline, content rating (`mpaa` or `certification`), genres,
  studio and rating. Of several countries' ratings (`GB:15 / US:R`), the
  server's country's is taken (Settings › Server), else the US's, else
  the first.
- **Lock**: Jellyfin's `<lockdata>true</lockdata>` keeps the item from being
  fetched online at all, Refresh included.
- Cast, collections (`<set>`), tags and watch state aren't read. Picture
  addresses (`<thumb>`, `<fanart>`) are never fetched: Pley takes the
  `poster.jpg`, `fanart.jpg` and other pictures next to the title, which
  the same tools save.

What wins, field by field, highest first:

1. An edit made in Pley, and for the match, Fix Match.
2. The NFO file.
3. The metadata source (TMDB, TheTVDB or TVmaze).
4. The file and folder names.

An NFO file's fields describe the title its ids name. When Fix Match
chooses another, the file's fields and lockdata stop applying and the
chosen title's metadata shows, until Fix Match is undone or the file is
corrected to name that title. An NFO file without ids always applies.

An NFO file edited since the last scan is read again, even when its video
didn't change. When one is removed, or stops setting a field, the field
goes back to the metadata source's value at the next refresh, or to the
names'. When it is removed or stops giving ids, the movie or show is
matched by its names again. Whatever an NFO file changes, the item stays
the same item, with its watch state and its place in My List.

Pley never writes NFO files. A file bigger than 1 MiB, or a link to
anything but an NFO file, is skipped.

Reading NFO files is on by default; Settings → Server's NFO files switch
(the `read_nfo` setting, [API.md](API.md#system-admin)) turns it off.
Turning it on or off scans every library; turned off, what NFO files set
goes back as when they are removed.

In an item's Edit details, fields the NFO file sets say so. Fix match says
when a title was matched by hand, and Undo Fix Match lets its NFO file
decide the match again.

## .strm files

A `.strm` file is a text file that names where a video is, instead of
being one: Kodi, Jellyfin and Emby read them, and tools that list what a
hosting service, a cloud drive or another server holds write them. Pley
reads them in movie and TV libraries once an administrator turns them on
(Settings → Server → Library scanning). Off, Pley lists none of them and
asks none of their addresses; turned off again, they leave the libraries
as deleted files would.

```
Movies/Alien (1979)/Alien (1979).strm
Movies/Alien (1979)/Alien (1979).en.srt
Shows/The Expanse/Season 01/The Expanse - S01E01.strm
```

- **Its name places it** like any video's: title and year, season and
  episode, edition, `-trailer`. Artwork, subtitle files and an NFO file
  beside it are its own, and several `.strm` files of one movie are its
  versions.
- **Its first line is the address**: the first that isn't empty and
  doesn't start with `#`. It must be an `http://` or `https://` address of
  a media file (MKV, MP4, WebM, MPEG-TS…), with ranges for seeking. A name
  and password in it (`https://name:password@host/…`) are sent as the
  address asks.
- **Kodi's headers** after the address are sent with every request:
  `https://host/film.mkv|User-Agent=VLC%2F3.0&Referer=https%3A%2F%2Fhost%2F`.
  Pley takes `User-Agent`, `Referer`, `Origin`, `Cookie`, `Authorization`,
  `Accept` and `Accept-Language`, and leaves Kodi's other options out.
- **What isn't read**: a path (`/mnt/…`, `C:\…`, `\\nas\…`, `file://`),
  a relative address, any other protocol (`rtsp`, `rtmp`, `smb`, `ftp`,
  `plugin://`), a playlist (an `.m3u8` address) and a file over 64 KiB. Such
  a file is listed, and playing it says what is wrong with it. To have a
  video elsewhere on the disk in a library, link to it instead.
- **The address stays on the server.** No app, no player and no log gets
  it. A player that plays the video as it is reads it through Pley, which
  asks the address; a stream is made by the transcoder from what Pley
  hands it. A password or token in the address is never shown to anyone,
  administrators included: the `.strm` file is where to read it.
- **A scan doesn't ask the address.** A library of links is scanned as
  fast as its files are listed, and a hosting service isn't asked for each
  of them. What the video is (its tracks, its length, its size, 4K and HDR)
  is found the first time someone opens its page or plays it, and kept
  until the file names another address. Until then it has no badges in
  listings and reads `STRM` where its format goes.
- **What isn't made for it**: seek previews, chapter pictures, a picture
  grabbed from the video, and intro and credits detection, which all read
  the whole video. Chapters and a subtitle track in the video work as for
  a file, read through Pley at the address.
- **An address on the internet stays on the internet.** It can't redirect
  to an address of your own network or of the server's computer, also
  when an address of your network sent Pley there. An
  address the file itself names on your network (a NAS, a download
  service) is asked as it is. It may redirect to another address of your
  network written as a number, or to a name the `.strm` files give
  ([SECURITY.md](../SECURITY.md#strm-files)).
- A link that stops answering leaves its item in the library: playing it
  says the address didn't answer, or has nothing there any more.

# Configuration

How to set Pley up once it runs. Installing it is in
[deploy/README.md](../deploy/README.md).

## First start

On start, Pley prints a link with a one-time setup code
(`http://localhost:7539/setup?code=…`; on a desktop the tray icon opens it).
Open it, create the administrator account and add a library. Nobody without
the code can claim a new server.

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
| `--jellyfin-bind` | `off` | [Jellyfin-compatible API](COMPATIBILITY.md): `web` (Pley's own port), or addresses of its own (`0.0.0.0:8096`), several as with `--bind` |
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
- **Settings → Transcoder**: hardware acceleration, tone mapping and transcode
  limits.
- **Settings → Network**: where Pley listens (networks, port, the Plex- and
  Jellyfin-compatible APIs, discovery, Bonjour), changed while Pley runs and
  saved in `pley.toml`, unless `--bind`, `--plex-bind`, `--jellyfin-bind`,
  `--no-discovery` or `--no-bonjour` (or their `PLEY_*` variables) set it.
  Bonjour is off until turned on there, and has the networks it announces
  Pley on (`bonjour_adapters`, every network Pley listens on unless some
  are chosen).
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

- *Automatic* uses TMDB for TV when it has a key, then TheTVDB, else TVmaze.
  Changing the source fetches every show again.
- Without a TMDB key, movies keep their file names and local artwork: no
  keyless source has movie artwork.
- TVmaze is in English and has no content ratings. Its specials are season 0,
  in air date order.
- MusicBrainz only fills in what the tags lack and allows one request a
  second, so the first look-up of a big music library takes a while.

This product uses the TMDB API but is not endorsed or certified by TMDB. TV
data from TVmaze is licensed
[CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/). TV metadata
from TheTVDB; please consider adding missing information or subscribing.
Music data from MusicBrainz is public domain (CC0); covers from the Cover Art
Archive belong to their owners.

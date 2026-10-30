# Pley

Pley is a self-hosted media server. It organizes your movies, TV shows and
music, and live TV from your IPTV provider, and plays them on your devices:
directly when a device can play the file, transcoded on the fly when it
can't, with hardware encoding and HDR10, HLG and Dolby Vision tone mapping.

![Pley's web app: Home, with Continue Watching, My List and the libraries](docs/images/web-home.jpg)

## Screenshots

The web app, on a demo library of made-up titles.

<table>
  <tr>
    <td width="50%"><img src="docs/images/web-library.jpg" alt="A movie library: sorting, filters and watched marks"></td>
    <td width="50%"><img src="docs/images/web-movie.jpg" alt="A movie's page: Resume, From start, My List, Watched and its media info"></td>
  </tr>
  <tr>
    <td align="center">A library: sorting, filters, what's been watched</td>
    <td align="center">A movie: resume where you left off, My List, media info</td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/images/web-season.jpg" alt="A season's page: resume the next episode, and the episodes with their descriptions"></td>
    <td width="50%"><img src="docs/images/web-player.jpg" alt="The player, playing a movie as it is (direct play)"></td>
  </tr>
  <tr>
    <td align="center">A season: the next episode to resume, every episode</td>
    <td align="center">The player, here playing the file as it is</td>
  </tr>
</table>

## What it does

- **Libraries** of movies, shows and music, scanned as folders change, with
  metadata from TMDB, TheTVDB, TVmaze and MusicBrainz.
- **Playback** as the file is whenever a device can, otherwise transcoded on
  the GPU; Automatic quality follows the network ([QUALITY.md](docs/QUALITY.md)).
- **Live TV** from M3U and Xtream Codes sources, with their XMLTV guide.
- **Accounts** with profiles for children, PINs, two-step sign-in, and apps
  that get only what you allow.
- **Works offline** on the home network, and with Tautulli and Kometa
  ([COMPATIBILITY.md](docs/COMPATIBILITY.md)).

The full list is in [docs/FEATURES.md](docs/FEATURES.md).

## Apps

| App | Runs on | Docs |
|---|---|---|
| Web app | Any modern browser; installs as an app | |
| Pley for Windows, macOS and Linux | Desktop: finds servers on the network and shows their web app, with downloads to watch offline | |
| Pley for Android | Phones, tablets, Android TV, Google TV, Fire TV | [ANDROID.md](docs/ANDROID.md) |
| Pley for Kodi | Kodi 20 and newer (LibreELEC, CoreELEC, Android TV boxes, computers) | [KODI.md](docs/KODI.md) |
| Pley for Roku | Roku players and TVs | [ROKU.md](docs/ROKU.md) |
| Pley for Samsung TV, Pley for LG TV | Tizen and webOS TVs | [TV_PLATFORMS.md](docs/TV_PLATFORMS.md) |

## Getting started

- **Docker** on a Linux home server:
  `git clone https://github.com/hhaidar/pley.git && cd pley/deploy/docker && ./setup.sh`
  ([guide](deploy/docker/README.md); Unraid: [template](deploy/unraid/README.md)).
- **Windows**: the release's `pley-server-<version>-windows-<arch>-setup.exe`
  ([guide](deploy/windows/README.md)).
- **Linux and macOS** services, and release archives: [deploy/README.md](deploy/README.md).

On its first start Pley prints a link with a one-time setup code: open it
to create the administrator account ([CONFIGURATION.md](docs/CONFIGURATION.md)).

## How it fits together

<p align="center">
  <img src="docs/images/architecture.svg" alt="Pley's architecture: the apps talk to the pley server over HTTP; the server starts the transcoder and talks to it over HTTP and JSON; the transcoder runs FFmpeg, on the GPU where there is one; the server reads metadata sources and IPTV providers and scans your media folders, which the transcoder reads too." width="1000">
</p>

The server is written in Rust, the web app in React and TypeScript. Building
and testing Pley: [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Documentation

| Doc | What |
|---|---|
| [docs/FEATURES.md](docs/FEATURES.md) | Everything Pley does |
| [deploy/README.md](deploy/README.md) | Running Pley: Docker, Unraid, systemd, launchd, Windows, reverse proxies |
| [docs/CONFIGURATION.md](docs/CONFIGURATION.md) | First start, options, metadata sources |
| [docs/QUALITY.md](docs/QUALITY.md) | The Quality menu, Original and Automatic quality |
| [docs/STORAGE.md](docs/STORAGE.md) | What Pley keeps on disk, backups and clean-up |
| [docs/ANDROID.md](docs/ANDROID.md), [docs/KODI.md](docs/KODI.md), [docs/ROKU.md](docs/ROKU.md), [docs/TV_PLATFORMS.md](docs/TV_PLATFORMS.md) | The Android, Kodi, Roku, Samsung and LG apps |
| [docs/API.md](docs/API.md) | Pley's HTTP API, including app sign-in (OAuth 2.1) |
| [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md) | The compatibility API for `python-plexapi`, Tautulli and Kometa |
| [docs/TRANSCODER.md](docs/TRANSCODER.md) | The transcoder and its protocol |
| [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) | Building, testing and contributing |
| [docs/RELEASING.md](docs/RELEASING.md) | Versions, changelogs, releases and CI |
| [SECURITY.md](SECURITY.md) | Security model and reporting |
| [LICENSING.md](LICENSING.md) | Pley's licence, the Kodi add-on's, FFmpeg's and third-party notices |

## License

Pley is **Restricted**, its licence terms to be determined
([LICENSE](LICENSE)): no right to copy, modify or distribute it is granted
until they are published, except by a separate written agreement. The Kodi
add-on is GPL-2.0-or-later, as Kodi requires, and third-party code keeps its
own licences ([LICENSING.md](LICENSING.md)).

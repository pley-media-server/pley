<p align="center"><img src="apps/web/public/favicon.svg" width="96" height="96" alt=""></p>

<h1 align="center">Pley</h1>

<p align="center">A self-hosted media server for movies, shows, music, audiobooks and live TV.</p>

<p align="center">
<a href="LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0-F0B27A" alt="License: AGPL-3.0"></a>
<a href="https://discord.gg/wSsVdRRnT3"><img src="https://img.shields.io/badge/discord-join-5865F2?logo=discord&logoColor=white" alt="Pley's Discord"></a>
<a href="https://github.com/pley-media-server/pley/pkgs/container/pley"><img src="https://img.shields.io/badge/docker-ghcr.io-2496ED?logo=docker&logoColor=white" alt="Docker image"></a>
<a href="https://ca.unraid.net/apps/pley-0qqukvc0gs8hmc"><img src="https://img.shields.io/badge/unraid-community%20apps-F15A2C?logo=unraid&logoColor=white" alt="Pley in Unraid's Community Apps"></a>
<br>
<!-- latest releases: written by scripts/release.mjs when an app is released -->
<a href="https://github.com/pley-media-server/pley/releases/tag/release-2026-11-22"><img src="https://img.shields.io/badge/Media%20Server-0.1.15-0284C7" alt="Media Server 0.1.15"></a>
<a href="https://github.com/pley-media-server/pley/releases/tag/release-2026-11-22"><img src="https://img.shields.io/badge/Desktop-0.1.7-0284C7" alt="Desktop 0.1.7"></a>
<a href="https://github.com/pley-media-server/pley/releases/tag/release-2026-11-22"><img src="https://img.shields.io/badge/Android-0.1.17-0284C7" alt="Android 0.1.17"></a>
<a href="https://github.com/pley-media-server/pley/releases/tag/release-2026-11-22"><img src="https://img.shields.io/badge/Samsung-0.1.13-0284C7" alt="Samsung 0.1.13"></a>
<a href="https://github.com/pley-media-server/pley/releases/tag/release-2026-11-22"><img src="https://img.shields.io/badge/LG-0.1.13-0284C7" alt="LG 0.1.13"></a>
<a href="https://github.com/pley-media-server/pley/releases/tag/release-2026-11-22"><img src="https://img.shields.io/badge/Kodi-0.1.11-0284C7" alt="Kodi 0.1.11"></a>
<a href="https://github.com/pley-media-server/pley/releases/tag/release-2026-11-22"><img src="https://img.shields.io/badge/Roku-0.1.12-0284C7" alt="Roku 0.1.12"></a>
<!-- /latest releases -->
</p>

> [!WARNING]
> Pley is still in testing: expect rough edges, and keep backups of your data.

![Pley TV and phone apps](docs/images/apps.webp)

## Features

- Support for Movies, TV Shows, Music and Audiobooks
- IPTV from M3U playlists and Xtream Codes, with XMLTV guides
- HW accelerated transcoding with dynamic quality adjustment
- HDR10, HLG and Dolby Vision, tone mapping for SDR screens
- Skip Intro and Skip Credits
- Customizable Collections (TMDB, hand-picked or smart)
- Control playback on your other devices, or cast to Chromecast and AirPlay
- [Home Assistant integration](https://github.com/pley-media-server/pley-homeassistant)
  to see and control what's playing
- Works offline on your home network, with downloads for when you're away
- Profiles, PINs, two-step sign-in and age-based parental controls
- Requests for movies and shows, sent to Radarr and Sonarr
- Import from Plex: libraries, users and watch history
- Plex and Jellyfin compatible APIs for tools like Kometa

[All features](docs/FEATURES.md)

## Apps

Web, Windows, macOS, Linux, [Android](docs/ANDROID.md) (phones, tablets,
Android TV, Google TV and Fire TV), [Kodi](docs/KODI.md),
[Roku](docs/ROKU.md), [Samsung TV](docs/TV_PLATFORMS.md),
[LG TV](docs/LG.md). Apps for iPhone, iPad, Mac and Apple TV are
[in development](docs/APPLE.md). Jellyfin clients, Kometa and Tautulli
also work, through the [compatibility APIs](docs/COMPATIBILITY.md).

## Install

- [Docker](deploy/docker/README.md)
- [Unraid](deploy/unraid/README.md)
- [Synology](deploy/synology/README.md)
- [Windows](deploy/windows/README.md)
- [Linux and macOS](deploy/README.md)

Then [first start](docs/CONFIGURATION.md#first-start), and
[moving from Plex](docs/MOVING_FROM_PLEX.md) if you're coming from it.

## Architecture

![Pley's architecture: the apps talk to the server, which runs the transcoder, casts to Chromecast and AirPlay devices, reads metadata sources and IPTV providers, sends requests to Radarr and Sonarr, and scans your media folders](docs/images/architecture.svg)

## Docs

[Features](docs/FEATURES.md) ·
[Configuration](docs/CONFIGURATION.md) ·
[Moving from Plex](docs/MOVING_FROM_PLEX.md) ·
[Quality](docs/QUALITY.md) ·
[Storage](docs/STORAGE.md) ·
[API](docs/API.md) ·
[Compatibility APIs](docs/COMPATIBILITY.md) ·
[Transcoder](docs/TRANSCODER.md) ·
[Development](docs/DEVELOPMENT.md) ·
[Releasing](docs/RELEASING.md) ·
[Security](SECURITY.md)

## License

Pley is free software under the GNU Affero General Public License, version
3 ([LICENSE](LICENSE)). The Kodi add-on is GPL-2.0-or-later. What that
means, and third-party notices: [LICENSING.md](LICENSING.md). How to
contribute, and the license of a contribution:
[CONTRIBUTING.md](CONTRIBUTING.md).

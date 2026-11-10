<p align="center"><img src="apps/web/public/favicon.svg" width="96" height="96" alt=""></p>

<h1 align="center">Pley</h1>

<p align="center">A self-hosted media server for movies, shows, music and live TV.</p>

<p align="center">
<a href="https://github.com/pley-media-server/pley/pkgs/container/pley"><img src="https://img.shields.io/badge/docker-ghcr.io-2496ED?logo=docker&logoColor=white" alt="Docker image"></a>
<a href="LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0-F0B27A" alt="License: AGPL-3.0"></a>
<a href="https://discord.gg/wSsVdRRnT3"><img src="https://img.shields.io/badge/discord-join-5865F2?logo=discord&logoColor=white" alt="Pley's Discord"></a>
</p>

<p align="center">
<!-- latest releases: written by scripts/release.mjs when an app is released -->
<a href="https://github.com/pley-media-server/pley/releases/tag/release-2026-11-08"><img src="https://img.shields.io/badge/Media%20Server-0.1.9-0284C7" alt="Media Server 0.1.9"></a>
<a href="https://github.com/pley-media-server/pley/releases/tag/release-2026-11-06"><img src="https://img.shields.io/badge/Desktop-0.1.4-0284C7" alt="Desktop 0.1.4"></a>
<a href="https://github.com/pley-media-server/pley/releases/tag/release-2026-11-10.2"><img src="https://img.shields.io/badge/Android-0.1.11-0284C7" alt="Android 0.1.11"></a>
<a href="https://github.com/pley-media-server/pley/releases/tag/release-2026-11-08"><img src="https://img.shields.io/badge/Samsung-0.1.8-0284C7" alt="Samsung 0.1.8"></a>
<a href="https://github.com/pley-media-server/pley/releases/tag/release-2026-11-08"><img src="https://img.shields.io/badge/LG-0.1.8-0284C7" alt="LG 0.1.8"></a>
<a href="https://github.com/pley-media-server/pley/releases/tag/release-2026-11-06"><img src="https://img.shields.io/badge/Kodi-0.1.7-0284C7" alt="Kodi 0.1.7"></a>
<a href="https://github.com/pley-media-server/pley/releases/tag/release-2026-11-06"><img src="https://img.shields.io/badge/Roku-0.1.7-0284C7" alt="Roku 0.1.7"></a>
<!-- /latest releases -->
</p>

> [!WARNING]
> Pley is still in testing: expect rough edges, and keep backups of your data.

![Pley's Home in the web app](docs/images/web-home.jpg)

## Features

- Plays files as they are, or transcodes on the GPU
- HDR10, HLG and Dolby Vision tone mapping
- Live TV from M3U and Xtream Codes, with XMLTV guides
- Requests for movies and shows, fetched by Radarr and Sonarr
- Profiles, PINs and two-step sign-in
- Works offline on the home network

[All features](docs/FEATURES.md)

## Apps

Web, Windows, macOS, Linux, [Android](docs/ANDROID.md), [Kodi](docs/KODI.md),
[Roku](docs/ROKU.md), [Samsung TV](docs/TV_PLATFORMS.md),
[LG TV](docs/LG.md). Jellyfin's apps work too, and tools built on Plex's API
such as Kometa and Tautulli, through the
[compatibility APIs](docs/COMPATIBILITY.md).

## Install

- [Docker](deploy/docker/README.md)
- [Unraid](deploy/unraid/README.md)
- [Synology](deploy/synology/README.md)
- [Windows](deploy/windows/README.md)
- [Linux and macOS](deploy/README.md)

Then [first start](docs/CONFIGURATION.md#first-start).

## Architecture

![Pley's architecture: the apps talk to the server, which runs the transcoder, reads metadata sources and IPTV providers, sends requests to Radarr and Sonarr, and scans your media folders](docs/images/architecture.svg)

## Docs

[Features](docs/FEATURES.md) ·
[Configuration](docs/CONFIGURATION.md) ·
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
contribute, and the licence of a contribution:
[CONTRIBUTING.md](CONTRIBUTING.md).

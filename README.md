<p align="center"><img src="apps/web/public/favicon.svg" width="96" height="96" alt=""></p>

<h1 align="center">Pley</h1>

<p align="center">A self-hosted media server for movies, shows, music and live TV.</p>

<p align="center">
<a href="https://github.com/pley-media-server/pley/releases/latest"><img src="https://img.shields.io/github/v/release/pley-media-server/pley?display_name=release&label=release" alt="Latest release"></a>
<a href="https://github.com/pley-media-server/pley/pkgs/container/pley"><img src="https://img.shields.io/badge/docker-ghcr.io-2496ED?logo=docker&logoColor=white" alt="Docker image"></a>
<a href="LICENSE"><img src="https://img.shields.io/badge/license-AGPL--3.0-F0B27A" alt="License: AGPL-3.0"></a>
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

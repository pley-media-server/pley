# Changelog

Notable changes to Pley Desktop, the app for Windows, macOS and Linux, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `desktop-v<version>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

## [0.1.1] - 2026-11-01

### Added

- The system's notifications tell of requests while the window isn't in
  front: a movie or show asked for is ready to watch (or some of its
  seasons are), or was approved or declined; administrators hear when
  someone asks for one, and when one couldn't be sent to Radarr or Sonarr.
  After the computer slept or the network went, what became ready to
  watch or was declined meanwhile comes in one notification.

### Changed

- Pley Desktop is free software under the GNU Affero General Public
  License, version 3 (`AGPL-3.0-only`, `LICENSE`).

## [0.1.0] - 2026-10-30

### Changed

- Its links point at `github.com/pley-media-server/pley`.
- Pley Desktop is Restricted, its licence terms to be determined (`LICENSE`).

### Fixed

- Searching the network finds servers on Windows computers that also have
  virtual network adapters (Hyper-V, VirtualBox, VPNs), and no longer stops
  short when no server runs on the computer itself.

## [0.0.1] - 2026-10-29

### Added

- First release with a version of its own (builds before it carried 0.2.0,
  the version every app shared).

[Unreleased]: https://github.com/pley-media-server/pley/compare/desktop-v0.1.1...HEAD
[0.1.1]: https://github.com/pley-media-server/pley/releases/tag/desktop-v0.1.1
[0.1.0]: https://github.com/pley-media-server/pley/releases/tag/desktop-v0.1.0
[0.0.1]: https://github.com/pley-media-server/pley/releases/tag/desktop-v0.0.1

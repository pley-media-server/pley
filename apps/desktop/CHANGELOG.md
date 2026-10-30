# Changelog

Notable changes to Pley Desktop, the app for Windows, macOS and Linux, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `desktop-v<version>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

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

[Unreleased]: https://github.com/pley-media-server/pley/compare/desktop-v0.0.1...HEAD
[0.0.1]: https://github.com/pley-media-server/pley/releases/tag/desktop-v0.0.1

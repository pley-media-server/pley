# Changelog

Notable changes to Pley Desktop, the app for Windows, macOS and Linux, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `release-<date>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Added

- The app may be cast to, under the computer's name: the profile's other
  devices send it a movie or an episode, which its player opens, then
  pause, seek and stop it and see what it plays. It casts to them too, as
  the web app does. Needs a server from the same release.

## [0.1.2] - 2026-11-02

### Security

- Before the app opens a server, and before its notifications and
  downloads use the window's session there, the server proves it is the
  one the window signed in to, at that address. Another device at the
  address (on another network, or after the router hands it out again)
  can't open, and the app says another device answers there. When the
  computer's way to the server changes, or it wakes from sleep, a window
  showing a plain-http server leaves it within a second and opens it again
  only once the server proves itself; a page may still send a request in
  that second. A sign-in from before starts checking the next time the app
  opens the server. Needs a server from the same release.
- Forgetting a server signs the window out of it, so a device answering at
  its address later gets nothing. Opening a new server at another port of
  a server's address signs out first too, as the window's sign-in goes to
  every port of an address.
- A plain-http address beyond the home network is no longer opened, even
  after a warning: open the server by its https address.

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

[Unreleased]: https://github.com/pley-media-server/pley/compare/release-2026-11-02.2...HEAD
[0.1.2]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-02.2
[0.1.1]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-01
[0.1.0]: https://github.com/pley-media-server/pley/tree/94985f768f39f4a230060cb47af9498ee8779ddf
[0.0.1]: https://github.com/pley-media-server/pley/tree/7e14bb9d498d054a73b2176cd8949b6864ffacd9

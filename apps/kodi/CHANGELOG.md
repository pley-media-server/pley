# Changelog

Notable changes to Pley for Kodi, the add-on `plugin.video.pley`, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `kodi-v<version>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Added

- Live TV, where the server has channels, laid out as TiviMate does: in
  the rail after Home, the categories (Search, Favourites, Recently
  watched, All channels, then the groups, a region's gathered under it),
  the channels of the one shown by number with what's on now and how far
  into it, a page at a time, and the focused one's programme and what
  comes next beside them; a channel's schedule; number keys to go to a
  channel. Kodi's lists have the same categories. A channel plays in
  Kodi's own player as the provider sends it (Kodi deinterlaces; surround
  sound passes through), Next and Previous change channel within its
  category, and every connection in use says what holds each, with Watch
  here instead where it may be taken over. Search finds channels too. A
  channel has no pause, back or forward, as in every app. Lineups of tens
  of thousands of channels open at once. Needs a server with Live TV.

### Security

- Plain http goes only to addresses on the home network, checked on the
  connection: sign-in codes and tokens never cross the internet
  unencrypted. Use the server's https address from elsewhere.
- The proxy refuses a negative or unreadable Content-Length, and checks
  its secret in constant time.

### Changed

- The sign-in page names this app by its address on
  `pley-media-server.github.io`. It needs a server from the same release,
  which keeps it signed in.
- Its links point at `github.com/pley-media-server/pley`.
- PGS subtitles come to Kodi as a `.sup` file beside a converted stream,
  or for a `.sup` file next to the media, rather than burned in: the
  picture keeps its quality and HDR. Needs a server that makes them.
- Buttons and rows read in sentence case, as in the other apps: "Skip intro",
  "Skip credits", "More like this", "Cast & crew", "Guest stars". Sign out
  asks "You'll be signed out of <server>."

### Fixed

- Automatic quality no longer leaves the original for a wait the server
  caused (a slow disk, a remux catching up): a wait of 2 s or more counts
  only on a network the proxy measured below 1.5 times the file's bitrate
  in the last 2 minutes, or, where it measured nothing (a direct play),
  the second such wait within 5 minutes.

## [0.0.1] - 2026-10-29

### Added

- First release with a version of its own (builds before it carried 0.2.0,
  the version every app shared).

[Unreleased]: https://github.com/pley-media-server/pley/compare/kodi-v0.0.1...HEAD
[0.0.1]: https://github.com/pley-media-server/pley/releases/tag/kodi-v0.0.1

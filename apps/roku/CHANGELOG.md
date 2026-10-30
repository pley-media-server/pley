# Changelog

Notable changes to Pley for Roku, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `roku-v<version>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Added

- Live TV, where the server has channels, as TiviMate lays it out:
  Search pinned at the top (every channel, and the groups whose names
  match; Replay opens it from anywhere), Favourites,
  Recently watched, All channels, the regions in alphabetical order and
  then the other groups down the left, and the category's channels as a
  guide (Roku's TimeGrid, the programme in focus described above it) or
  as cards, 50 at a time, so lineups of tens of thousands of channels
  open at once. The live player changes channel with up and down within
  the category, lists it at the left, shows what's on with a Live chip
  when asked (OK, Right or the Options key), plays live only (no pause,
  rewind or skip), and when every connection is in use says what holds
  each, with Watch here instead where one may be taken over. Search
  finds Live TV channels too, in a row of their own. The Options key adds a channel to the favourites or takes it
  out; back from the player, the guide is at the channel watched.

### Changed

- The sign-in page names this app by its address on
  `pley-media-server.github.io`. It needs a server from the same release,
  which keeps it signed in.
- "Can't reach" offers Try again, as the other apps do, and the skip
  buttons read "Skip intro" and "Skip credits". Sign out asks "You'll be
  signed out of <server>."

### Fixed

- Automatic quality no longer leaves the original for a wait the server
  caused (a slow disk, a remux catching up): a wait of 2 s or more counts
  only on a network its segments showed below 1.5 times the file's
  bitrate in the last 2 minutes, or, where none came (a direct play), the
  second such wait within 5 minutes.

## [0.0.1] - 2026-10-29

### Added

- First release with a version of its own (builds before it carried 0.2.0,
  the version every app shared).

[Unreleased]: https://github.com/pley-media-server/pley/compare/roku-v0.0.1...HEAD
[0.0.1]: https://github.com/pley-media-server/pley/releases/tag/roku-v0.0.1

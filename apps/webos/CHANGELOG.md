# Changelog

Notable changes to Pley for LG TV (webOS), newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `lg-v<version>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Security

- The token goes only to the server's own origin, never to an address
  elsewhere that the server hands out.

### Added

- Live TV, where the server has IPTV channels for the profile, by
  category as TiviMate shows it: Favourites, Recently watched, All
  channels, the regions in alphabetical order, each opening to its
  groups, and the other groups; one search, with the TV's keyboard, of
  every channel by name, number or what's on and of the groups; a guide that
  moves through time and channel cards with what's on and next, a page at
  a time, so a lineup of tens of thousands opens at once. The player plays
  a channel through LG's media pipeline from its live edge, live only (no
  pause, rewind or skip), with what's on, a Live badge, the channel list
  beside the picture, and CH+ and CH−, Up and Down within the
  category, or a channel's number to change channels; Back lands on the
  channel watched. When every connection is in use, the player lists what
  each watches and who, with "Watch here instead" where one may be taken
  over; a connection taken from the TV says who took it. The app's search
  finds channels too, in a Live TV row. Needs a server that gives the
  TV's player keyed live addresses.

### Changed

- The sign-in page names this app by its address on
  `pley-media-server.github.io`. It needs a server from the same release,
  which keeps it signed in.

### Fixed

- Picture subtitles the server is still copying out of a big file show
  as far as they are, without a picture cut in two.
- Switching to another server no longer shows the last server's Home,
  libraries or titles, and asks "Who's watching?" as that server's
  household needs.
- Automatic quality no longer leaves the original for a wait the server
  caused (a slow disk, a remux catching up): a wait of 2 s or more counts
  only on a network the TV measured below 1.5 times the file's bitrate in
  the last 2 minutes, or, where it measured nothing, the second such wait
  within 5 minutes.

## [0.0.1] - 2026-10-29

### Added

- First release with a version of its own (builds before it carried 0.2.0,
  the version every app shared).

[Unreleased]: https://github.com/pley-media-server/pley/compare/lg-v0.0.1...HEAD
[0.0.1]: https://github.com/pley-media-server/pley/releases/tag/lg-v0.0.1

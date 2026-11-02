# Changelog

Notable changes to Pley for LG TV (webOS), newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `release-<date>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Fixed

- "Who's watching?" as the app starts asks the PIN of the profile it was
  left on when that profile has one, as on Android TV, and shows its lock.

## [0.1.1] - 2026-11-01

### Added

- Requests, on a server that has them turned on. Discover, in the rail,
  shows what is trending and popular on TMDB, coming to cinemas, and
  anime, each title marked with what the libraries have, and searches
  TMDB. A title's page asks for a movie, or for a show's seasons picked
  in a panel beside it, and says where its request is. Your requests
  follows them, and a notice pops up once one is ready to watch;
  administrators get Requests, opening on those that need attention, to
  approve, decline (with a reason typed on the TV's keyboard), mark as
  added by hand or delete. A number on the rail counts what needs
  attention or has news. Search lists what TMDB has under On TMDB, and a
  movie's or show's page in a library shows its request's stage, with
  Request more seasons for a show missing some.
- Signing in with a code says when the server requires two-step sign-in:
  the phone that approves the code sets it up for whoever hasn't.

### Changed

- Pley for LG TV is free software under the GNU Affero General Public
  License, version 3 (`AGPL-3.0-only`, `LICENSE`).

### Fixed

- Live TV's guide says Today, Tomorrow and Yesterday on the days daylight
  saving starts or ends too: a programme just after midnight tomorrow
  showed its date, or the day after tomorrow showed as Tomorrow.
- A server answering that it can't do something right now (the
  transcoder isn't running, a file can't be read) has its own message
  shown, instead of "Can't reach your server", and the TV doesn't go
  offline for it. Mid-film, the player waits for the transcoder as it
  does for the server, 90 s at most, then shows the server's message.
- The search field no longer shows a small cross at its end while you
  type, one the remote's arrows can't reach.
- Dolby Vision details call a profile 8 with the Blu-ray's base layer
  "Profile 8", as its label does, not "Profile 8.6".

## [0.1.0] - 2026-10-30

### Security

- The token goes only to the server's own origin, never to an address
  elsewhere that the server hands out.

### Added

- Live TV, where the server has IPTV channels for the profile, by
  category: Favourites, Recently watched, All
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

[Unreleased]: https://github.com/pley-media-server/pley/compare/release-2026-11-01...HEAD
[0.1.1]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-01
[0.1.0]: https://github.com/pley-media-server/pley/releases/tag/lg-v0.1.0
[0.0.1]: https://github.com/pley-media-server/pley/releases/tag/lg-v0.0.1

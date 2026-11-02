# Changelog

Notable changes to Pley for Kodi, the add-on `plugin.video.pley`, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `release-<date>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Changed

- Discover has no Search of its own: Search shows what can be asked for
  under Discover after the libraries' results, and says when TMDB isn't
  answering or refuses, with Try again where it helps.

### Fixed

- A wrong PIN, or too many, asks again with the reason in the dialog's
  heading, and the profile in use shows its lock on "Who's watching?".
  Switch profile in the settings asks the PIN of a profile that got one
  after the list was loaded.

## [0.1.1] - 2026-11-01

### Added

- Requests: ask for movies and shows the server doesn't have yet. Discover
  searches TMDB and lists Trending this week, Popular movies, Popular
  shows, Coming to cinemas and Popular anime, each title with its badge
  ("In library", "Requested", "Declined"); OK does its one main action
  (Request, Open, Tell me too…) and the rest is in its context menu. A
  show's seasons each say their state, and Which seasons? picks the ones
  to ask for. Needs a server with requests turned on; for a sign-in from
  before, Discover offers "Sign in again".
- Your requests: each request's stage ("Waiting for approval",
  "Downloading 42% · in 20 min", "Ready to watch"…), what it says and when
  it was asked for, with Withdraw and Stop following. Administrators get
  Requests, opening on Needs attention, with filters (Yours for their
  own), Approve, Decline, Decline with a reason… (typed on Kodi's
  keyboard), Mark as added by hand and Delete. The entry carries the
  number of what is new or waits.
- Search shows On TMDB after the libraries' results, and a library show's
  menu has Request more seasons. In Pley's look, the profile's menu opens
  Discover and Your requests in Kodi's lists, search has an On TMDB row,
  and a show's More has Request more seasons.
- "“Heat” is ready to watch." pops up with the poster as soon as it is
  ("Season 2 of “X” is ready to watch." for a show), and as Kodi starts
  for news that came while it was off; never over a playing video, where
  the last three wait for it to stop.
- Signing in with a code says when the server requires two-step sign-in:
  the phone that approves the code sets it up for whoever hasn't.

### Fixed

- Dolby Vision the server repackages (with HDR10+, or profile 7 outside
  CoreELEC) plays as its HDR10 or HDR10+ base layer, as the server says,
  instead of being sent as Dolby Vision that Kodi showed as HDR10.
- On CoreELEC, Dolby Vision with HDR10+ plays as Dolby Vision, as it is,
  rather than as its HDR10+ base layer (with a server from the same
  release).
- The rail of Pley's window shows or drops Live TV as soon as it is set up
  or its last source removed, without reopening the window.
- Live TV's guide says Today, Tomorrow and Yesterday on the days daylight
  saving starts or ends too: a programme just after midnight tomorrow
  showed its date, or the day after tomorrow showed as Tomorrow.

## [0.1.0] - 2026-10-30

### Added

- Live TV, where the server has channels: in
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

[Unreleased]: https://github.com/pley-media-server/pley/compare/release-2026-11-01...HEAD
[0.1.1]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-01
[0.1.0]: https://github.com/pley-media-server/pley/releases/tag/kodi-v0.1.0
[0.0.1]: https://github.com/pley-media-server/pley/releases/tag/kodi-v0.0.1

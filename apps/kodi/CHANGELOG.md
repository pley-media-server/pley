# Changelog

Notable changes to Pley for Kodi, the add-on `plugin.video.pley`, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `release-<date>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Added

- Libraries filter by resolution and dynamic range, in the Filter menu
  after Year, where their titles differ in them. The Filter chip says
  what is chosen, and Save as… keeps them. Needs a server from the same
  release.
- Library posters show "4K" and their dynamic range ("DV", "HDR10+",
  "HDR10", "HLG") while the library is sorted by bitrate or filtered by
  resolution or dynamic range.
- Libraries sort by your rating and by bitrate, as on the other apps;
  sorted by bitrate, the line under each poster is its bitrate and size.

### Changed

- Search shows movies, shows and episodes first, then music, then Live
  TV, then people, the closest matches first in each.
- The rail's entries are smaller, as on the other TV apps, and it lists
  five libraries (four beside Live TV) instead of three before Libraries
  takes Collections' place.
- A library's sorts are the other apps' for its kind: shows no longer
  sort by duration, and music sorts by title, date added, your rating and
  last played.

## [0.1.3] - 2026-11-02

### Changed

- Starting a stream with VobSub or DVB subtitles no longer has the server
  copy them out of the film for nothing: Kodi has them drawn onto the
  video, and on a network share the copy could read the whole film.
  Needs a server from the same release.

### Fixed

- Stopping a show or movie no longer turns its subtitles off for the next
  time: Kodi turns them off as it stops, and that could be saved as the
  choice. A track changed in Kodi's player is saved once it has held 2
  seconds, so one changed just before stopping isn't.
- Changing the audio no longer turns a show's or movie's subtitles off for
  the next time when Kodi couldn't show them (PGS whose file didn't load):
  the add-on turns them off then, and that was saved with the audio.

### Security

- Before the token goes to the server's address, the server proves it is
  the one Kodi signed in to, at that address: the access token waits for a
  proof made on this network within the half hour, the refresh token, a
  profile's PIN and signing out for one made then. Another device at the
  address on another network gets nothing, nor does one passing the
  challenge on to the server from elsewhere; Pley says another device
  answers at the server's address. One that takes the address on Kodi's own
  network may get an access token (an hour's) within the half hour, never
  the refresh token. A sign-in from before starts checking within the hour,
  as its token is renewed.
- After Change server, a request still waiting on the server left behind
  is never sent there again with the new server's token: when that server
  refuses its token, it gets nothing more and the request ends signed out.
- The server left behind can't sign the add-on out of the new one either,
  by refusing late a token renewal it held.

## [0.1.2] - 2026-11-02

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

[Unreleased]: https://github.com/pley-media-server/pley/compare/release-2026-11-02.2...HEAD
[0.1.3]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-02.2
[0.1.2]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-02
[0.1.1]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-01
[0.1.0]: https://github.com/pley-media-server/pley/tree/94985f768f39f4a230060cb47af9498ee8779ddf
[0.0.1]: https://github.com/pley-media-server/pley/tree/7e14bb9d498d054a73b2176cd8949b6864ffacd9

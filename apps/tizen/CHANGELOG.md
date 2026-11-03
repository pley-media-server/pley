# Changelog

Notable changes to Pley for Samsung TV (Tizen), newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `release-<date>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Security

- A crafted file's picture subtitles (PGS, DVB, VobSub) can no longer keep
  the app busy: decoding one moment of them stops after a set amount of
  work, far more than real subtitles take, and that moment isn't shown.
- A crafted file's picture subtitles can no longer run the app out of
  memory: a track is read up to 100,000 samples and 512 MiB, a moment's
  pictures are a 4K screen's pixels at most (2 pictures for PGS, 32 for
  DVB), and the frames kept decoded hold 128 MiB besides the one showing.
  Real subtitles show as before.
- The token goes only to the server's own origin, never to an address
  elsewhere that the server hands out.
- Before the token goes to the server's address, the server proves it is
  the one the TV signed in to, at that address: the access token waits for
  a proof made on this network within the half hour, the refresh token and
  a profile's PIN for one made then. Another device at the address on
  another network gets nothing, nor does one passing the challenge on to
  the server from elsewhere; the TV says "Another device answers there."
  One that takes the address on the TV's own network may get an access
  token (an hour's) within the half hour, never the refresh token. A
  sign-in from before starts checking within the hour, as its token is
  renewed.

### Added

- Live TV, where the server has IPTV channels for the profile, by
  category: Favourites, Recently watched, All
  channels, the regions in alphabetical order, each opening to its
  groups, and the other groups; one search, with the TV's keyboard, of
  every channel by name, number or what's on and of the groups; a guide that
  moves through time and channel cards with what's on and next, a page at
  a time, so a lineup of tens of thousands opens at once. The player plays
  a channel through AVPlay from its live edge, live only (no pause, rewind
  or skip), with what's on, a Live badge, the channel list beside the
  picture, and CH+ and CH−, Up and Down within the category, or a
  channel's number to change channels; the remote's CH LIST and GUIDE keys
  work, and Back lands on the channel watched. When every connection is in
  use, the player lists what each watches and who, with "Watch here
  instead" where one may be taken over; a connection taken from the TV
  says who took it. The app's search finds channels too, in a Live TV
  row. Needs a server that gives the TV's player keyed live addresses.
- Requests, on a server that has them turned on. Discover, in the rail,
  shows what is trending and popular on TMDB, coming to cinemas, and
  anime, each title marked with what the libraries have. A title's page
  asks for a movie, or for a show's seasons picked in a panel beside it,
  and says where its request is. Your requests follows them, and a
  notice pops up once one is ready to watch; administrators get
  Requests, opening on those that need attention, to approve, decline
  (with a reason typed on the TV's keyboard), mark as added by hand or
  delete. A number on the rail counts what needs attention or has news.
  Search shows what can be asked for in a Discover row after the
  libraries' results; a movie's or show's page in a library shows its
  request's stage, with Request more seasons for a show missing some.
- Signing in with a code says when the server requires two-step sign-in:
  the phone that approves the code sets it up for whoever hasn't.
- Movie and show libraries filter by resolution (4K, 1080p, 720p, SD) and
  dynamic range (Any HDR, HDR including Dolby Vision and HDR+, Dolby
  Vision, HDR+, HLG, SDR), where the
  server offers more than one of them: in the Filter panel after Year,
  with a chip saying the one chosen. Clear filters and Save as… take them
  in, and the library opens with them as it was left.

### Changed

- The app's icon is square, as the TV's menu draws the other apps, where
  it was wider than the rest.
- Picture subtitles (PGS, VobSub, DVB) show within a few seconds of a
  playback starting or a seek, where they could take up to a minute: the
  TV tells the server where it plays, and the server reads those around it
  first.
- "Enter an address" opens the TV's keyboard at once, on an empty field,
  in place of the number pad, so a server's name or a whole https address
  is as easy to type as an IP. The remote's digit keys still type once the
  keyboard is closed.
- The app's icon says WEB on a blue band, so it is told apart from the
  native app on a TV that has both.
- Pley for Samsung TV is free software under the GNU Affero General Public
  License, version 3 (`AGPL-3.0-only`, `LICENSE`).
- The sign-in page names this app by its address on
  `pley-media-server.github.io`. It needs a server from the same release,
  which keeps it signed in.
- A library's posters show 4K and the dynamic range (DV, HDR10) when it is
  sorted by bitrate or filtered by resolution or dynamic range, and no
  longer a lower resolution (1080p, 720p); otherwise they carry no
  quality badges.
- Search shows movies, shows and episodes first, then music, then Live
  TV, then people, the closest matches first in each.
- A Live TV channel whose connection someone else takes over stops at
  once and says why, under "Live TV stopped", with Watch again, where it
  said so only once its picture had stalled for 5 seconds.
- The rail's entries are smaller and list up to six libraries, where
  four or more went behind Libraries; nothing leaves the rail to make room
  for Live TV, Discover or Now Playing any more, and the entries scroll
  between the avatar and Settings when they don't all fit, keeping the
  focused entry fourth from the top, as the app's lists keep theirs.
- Search's rows hold every result, up to 200 titles, 200 people and 200
  Live TV channels, where they stopped at 60 titles and 12 people and
  channels.

### Removed

- The player's Back 10 seconds and Forward 10 seconds buttons. Left and
  Right still seek.

### Fixed

- Blu-ray subtitles that show two pictures at once, the first one forced,
  show both, and a picture the disc crops shows cropped.
- Settings' Always start as no longer starts the TV as a profile with a
  PIN without it: the start opens on that profile's PIN pad, and Back shows
  the other profiles. While the TV doesn't know whether the profile it was
  left on has a PIN, the start asks "Who's watching?".
- A profile that got a PIN after "Who's watching?" was loaded opens the
  PIN pad, and one whose PIN is locked or asked too often says so on it,
  as on Android TV; a refusal of another profile's switch no longer goes
  on as the profile in use.
- "Who's watching?" as the app starts asks the PIN of the profile it was
  left on when that profile has one, as on Android TV, and shows its lock.
- A server answering that it can't do something right now (the
  transcoder isn't running, a file can't be read) has its own message
  shown, instead of "Can't reach your server", and the TV doesn't go
  offline for it. Mid-film, the player waits for the transcoder as it
  does for the server, 90 s at most, then shows the server's message.
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
- Live TV's guide says Today, Tomorrow and Yesterday on the days daylight
  saving starts or ends too: a programme just after midnight tomorrow
  showed its date, or the day after tomorrow showed as Tomorrow.
- The search field no longer shows a small cross at its end while you
  type, one the remote's arrows can't reach.
- Dolby Vision details call a profile 8 with the Blu-ray's base layer
  "Profile 8", as its label does, not "Profile 8.6".

## [0.0.1] - 2026-10-29

### Added

- First release with a version of its own (builds before it carried 0.2.0,
  the version every app shared).

[Unreleased]: https://github.com/pley-media-server/pley/compare/7e14bb9d498d054a73b2176cd8949b6864ffacd9...HEAD
[0.0.1]: https://github.com/pley-media-server/pley/tree/7e14bb9d498d054a73b2176cd8949b6864ffacd9

# Changelog

Notable changes to Pley for Android: phones, tablets, Android TV and Fire TV, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `release-<date>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Changed

- Search shows movies, shows and episodes first, then music, then Live
  TV, then people, the closest matches first in each.

### Removed

- The player's −10 s and +10 s buttons, on phones and on TV. Double-tapping
  the sides of the picture (phones) and Left and Right (TV) still seek.

### Fixed

- The player offers the subtitle style (Subtitle style on phones, the
  size, background, font and position under the tracks on TV) only while
  text subtitles show, where it changes something: picture subtitles (PGS,
  VobSub, DVB) keep their own look, and subtitles off have nothing to
  style.

## [0.1.4] - 2026-11-02

### Changed

- Picture subtitles (PGS, VobSub, DVB) drawn beside a stream show within
  a few seconds of a playback starting or a seek, where they could take up
  to a minute: the app tells the server where it plays, and the server
  reads those around it first.

### Fixed

- Settings' Always start as no longer starts the TV as a profile with a
  PIN without it: the start opens on that profile's PIN pad, and Back shows
  the other profiles. While the TV doesn't know whether the profile it was
  left on has a PIN, the start asks "Who's watching?".

### Security

- Before the token goes to the server's address, the server proves it is
  the one the app signed in to, at that address: the access token waits
  for a proof made on this network within the half hour, the refresh token
  and a profile's PIN for one made then. Another device at the address on
  another network gets nothing, nor does one passing the challenge on to
  the server from elsewhere; the app says "Another device answers there."
  One that takes the address on the device's own network may get an
  access token (an hour's) within the half hour, never the refresh token.
  A sign-in from before starts checking within the hour, as its token is
  renewed.

## [0.1.3] - 2026-11-02

### Changed

- A library's posters show 4K and the dynamic range (DV, HDR10) whatever
  the sort, where they showed only when sorted by bitrate, and no longer
  the codec (H.264, HEVC) or a lower resolution (1080p, 720p).
- On Android TV and Fire TV, "Enter an address" opens the TV's keyboard at
  once, on an empty field, in place of the number pad, so a server's name
  or a whole https address is as easy to type as an IP. The remote's digit
  keys still type once the keyboard is closed.

### Fixed

- Too many wrong PINs for the profile a TV starts as say how long to wait,
  as a switch does, rather than "a few minutes".

## [0.1.2] - 2026-11-01

### Changed

- Playback speed works with sound that goes to a receiver as it is: the
  device decodes it while the speed isn't Normal, or the server converts
  it when nothing on the device can, and it goes to the receiver as it is
  again back at Normal. Where the server can't convert it either, the
  speed stays at Normal and says why, and the title plays on. Speed and
  audio delay also work where tunneled playback is on, which they turn
  off while they're used.

## [0.1.1] - 2026-11-01

### Added

- Requests on phones and tablets, where the server takes them: Discover
  (from Search and Me) shows what TMDB has, with trending and popular rows
  and TMDB's search, each title marked In library, Requested or Declined.
  A title's page asks for a movie, or for the seasons picked of a show,
  and shows its seasons, its cast and how far its request is, with the
  download's progress kept up to date.
- Your requests, under Me: each request's stage, the download's progress,
  why one was declined, and Watch once it's ready, with Withdraw, Remove
  and Stop following. Pull to read them again.
- Administrators get Requests, opening on Needs attention: Approve,
  Approve… with the quality profile, folder and seasons, Decline with an
  optional reason, Try again, Mark as added by hand and Delete. Me's tab
  shows how many wait, in red while one couldn't be sent.
- Search shows what TMDB has and the libraries don't, after the libraries'
  results ("On TMDB", with See all on Discover), and when the libraries
  have nothing.
- A library's movie or show shows its request's stage and download, and a
  show partly there offers Request more seasons.
- News of requests (ready to watch, approved, declined; for administrators
  new requests and ones that couldn't be sent) shows in a toast with Open,
  never over the player. About credits TMDB.
- Signing in says when the server requires two-step sign-in: on a phone,
  you set it up on the server's page as you sign in; on a TV, the phone
  that approves the code sets it up for whoever hasn't.

### Changed

- Pley for Android is free software under the GNU Affero General Public
  License, version 3 (`AGPL-3.0-only`, `LICENSE`).

### Fixed

- Live TV's guide says Today, Tomorrow and Yesterday on the days daylight
  saving starts or ends too: a programme just after midnight tomorrow
  showed its date, or the day after tomorrow showed as Tomorrow.
- Each video is promised Dolby Vision only where the display mode it plays
  in shows it: a trailer, or a film once frame rate matching has given up,
  stays in the mode in use. Dolby Vision turned off in the system's
  settings is remembered once a mode shows it off. Those videos played as
  HDR10 while their details said Dolby Vision.
- Dolby Vision the screen shows starts with its own decoder even before
  the TV has switched to the video's mode, instead of as HDR10.
- With a server from the same release, the player's details say why
  Dolby Vision plays as its base layer, telling a screen that doesn't show
  it from a device that doesn't decode it.
- The player's Decoded line says HEVC and HDR10 for Dolby Vision played
  as its base layer, not Dolby Vision.
- Dolby Vision details call a profile 8 with the Blu-ray's base layer
  "Profile 8", as its label does, not "Profile 8.6".

## [0.1.0] - 2026-10-30

### Added

- Live TV, where the server has channels: from the Libraries tab on
  phones and the rail on TVs. Favourites, Recently watched, All channels
  and the provider's groups are a picker on phones
  and a column beside the guide on TVs. The guide scrolls through time
  with now marked, or the channels show as cards; both load a page of
  channels at a time, ask for the programmes of the rows in view only,
  and open back at the channel last watched. In a group, names leave out
  the region the group says. A programme's details come with Watch and
  the favourite.
- The live player: channel up and down within the group it was opened
  from, a channel list, a typed number, what's on and next when asked
  for (Info on a TV, a tap on a phone), Live as it airs (no pause, no
  going back, no picture in picture, as on the web), every connection in
  use listed with what holds it and Watch here instead where it may be
  taken over, the channel's logo for radio, and a spinner only once a
  stall lasts. Channels show in the app's search too.
  Leaving lets the provider's connection go. A channel the server sends
  as it is plays straight from its MPEG-TS, sooner than its HLS.
- Live TV opens as it was left, after a restart too: the category, the
  guide or the cards, and the channel last watched, for each server and
  profile. Its groups read as Regions (alphabetical, each opening to its
  groups), then More groups in the provider's order.
- One search across every channel, whatever category is open, naming
  the groups it matches; on TVs it is at the top of the categories, with
  the system keyboard. The live player's channel list searches every
  channel too, and its heading lists another group: a channel tuned from
  there makes channel up and down follow that group.

### Security

- No other app on a TV can make Pley play a title through a Watch Next
  link: links carry this installation's signature.
- A stream's subtitles are never read from a file on the device that the
  server names.

### Changed

- The sign-in page names this app by its address on
  `pley-media-server.github.io`. It needs a server from the same release,
  which keeps it signed in.

### Fixed

- Picture subtitles in a file next to the media show over the video, from
  the server's copy of them.
- Automatic quality keeps the original (direct play or remux) unless the
  picture stops for want of data for 2 s or more. Short waits and slow
  delivery moved it to a transcode though the network carried the file:
  a big file's buffer holds only seconds of it. From a stream in several
  sizes it now steps back up after calm minutes too, to the original when
  the network carries 1.6 times it.
- Automatic quality no longer leaves the original for a wait the server
  caused (a slow disk, a remux catching up): a wait of 2 s or more counts
  only on a network shown below 1.5 times the file's bitrate in the last
  2 minutes (where nothing showed it, the second such wait within 5
  minutes), and the server converting slower than the video plays doesn't
  count at the original. A title that starts lower for what was
  remembered of the network goes back up after 20 s calm, as high as the
  network carries, the original included.

## [0.0.1] - 2026-10-29

### Added

- First release with a version of its own (builds before it carried 0.2.0,
  the version every app shared).

[Unreleased]: https://github.com/pley-media-server/pley/compare/release-2026-11-02.2...HEAD
[0.1.4]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-02.2
[0.1.3]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-02
[0.1.2]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-01.2
[0.1.1]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-01
[0.1.0]: https://github.com/pley-media-server/pley/tree/94985f768f39f4a230060cb47af9498ee8779ddf
[0.0.1]: https://github.com/pley-media-server/pley/tree/7e14bb9d498d054a73b2176cd8949b6864ffacd9

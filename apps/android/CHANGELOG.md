# Changelog

Notable changes to Pley for Android: phones, tablets, Android TV and Fire TV, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `android-v<version>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

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

[Unreleased]: https://github.com/pley-media-server/pley/compare/android-v0.0.1...HEAD
[0.0.1]: https://github.com/pley-media-server/pley/releases/tag/android-v0.0.1

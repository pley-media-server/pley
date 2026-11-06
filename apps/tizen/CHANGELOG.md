# Changelog

Notable changes to Pley for Samsung TV (Tizen), newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `release-<date>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Added

- A globe at the bottom right of Choose your server and the address
  screen, and at the top right of the sign-in code, picks the TV's language before anyone signs in:
  Automatic (the TV's own), English, Français or Español. The TV keeps the
  choice, and a profile set to Automatic uses it.
- Someone else in your household can cast to the TV from their phone or
  browser while it is on your profile: the TV switches to their profile
  (their phone or browser asks their PIN, not the TV) and plays what they
  sent. A switch that fails says so and plays nothing.
- When your phone or browser pauses, plays, seeks or skips a film or
  episode on the TV, or changes its audio, subtitles or quality, the TV
  says so for 2 s at the top right ("Paused", "+30 s", "Jumped to 12:34",
  "Subtitles off") and doesn't bring up the player's controls. Controls
  already up hide as they do while it plays, even once it is paused, until
  a key is pressed on the TV.

### Changed

- The TV starts straight into Home as the profile it was left on when it
  has no PIN, where a household of several profiles was asked "Who's
  watching?" each time. A profile with a PIN still asks first, and Switch
  profile still shows every profile. Settings' Always start as
  calls this Profile used last, where it said Ask who's watching.

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

- Pley for Samsung TV is in French and Spanish as well as English: the
  account's language, else the TV's, else the server's. Settings → Profile
  → Language picks the account's, on all its devices, and the app changes
  to it at once.
- Your phone or browser changes the quality of a film or episode playing
  on the TV, as the player's Quality menu does (the pick is kept for the
  next ones), and sees which is picked. A film or episode sent from a
  title's page plays with the audio and subtitles picked there.
- Your phone or browser plays a film, an episode, a show, an album, an
  artist, a track, a playlist or a Live TV channel on the TV ("Play on…"):
  it starts as the TV's own Play would, in place of what plays, and the
  TV says "Playing from" the device it came from.
- Your phone or browser changes the audio and subtitles of a film or
  episode playing on the TV, as the player's own Audio and Subtitles do,
  and sees which play.
- Your phone or browser controls what plays on the TV: a film or episode
  plays, pauses, seeks and skips, and goes on to the next one; music does
  the same, and goes back a track; Live TV changes channel. Live TV
  stopped from another device, or by the server's administrator, stops
  on the TV and says who, with their message.
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
  anime, each title marked with what the libraries have; one a library
  has whole opens its library page, where it plays, here and in Search.
  A title's page asks for a movie, or for a show's seasons picked in a
  panel beside it, and says where its request is. Your requests follows
  them, and a notice pops up once one is ready to watch; administrators get
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

- Requests: a sign-in from before the app could ask says "Sign in again to ask for movies
  and shows" without a second line, and its button reads Sign in.
- The app's icon is square, as the TV's menu draws the other apps, where
  it was wider than the rest.
- When the server can't be reached, the app says "Pley is offline", where
  it said "Showing what Pley had saved. Reconnecting…". It still shows
  what it saved and reconnects by itself.
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
  said so only once its picture had stalled for 5 seconds. "Live TV
  stopped" stays when the app is hidden and shown again, until Watch
  again.
- The rail's entries are smaller and list up to six libraries, where
  four or more went behind Libraries; nothing leaves the rail to make room
  for Live TV, Discover or Now Playing any more, and the entries scroll
  between the avatar and Settings when they don't all fit, keeping the
  focused entry fourth from the top, as the app's lists keep theirs.
- Search's rows hold every result, up to 200 titles, 200 people and 200
  Live TV channels, where they stopped at 60 titles and 12 people and
  channels.
- A programme's details in Live TV have Watch and the favourite side by
  side, the favourite a star as in the live player, filled for a
  favourite, where Watch and Add to or Remove from favourites were rows of
  the panel. In the live player, the star no longer shows its words under
  it, which ran under Channels.

### Removed

- The player's Back 10 seconds and Forward 10 seconds buttons. Left and
  Right still seek.

### Fixed

- Live TV's channel up or down, still finding the next channel when another
  is picked from the list or the app is hidden, no longer changes channel
  a moment later.
- Seeking in a track the server converts ends the track's session before
  the new one starts, so a pause meanwhile no longer pauses the old one.
- A film the TV can't play falls back one step at a time, to a conversion
  of its audio before one of the whole film: the TV's player says a failure
  twice, and the second skipped a step and started a second session.
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

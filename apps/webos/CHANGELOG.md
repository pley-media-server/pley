# Changelog

Notable changes to Pley for LG TV (webOS), newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `release-<date>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Added

- Videos on a USB drive plugged into the TV play in Pley's player, as in
  the Samsung app: the rail's USB drive lists the drive's folders and
  videos, each saying where it resumes, and a video plays as it is with
  its own tracks, chapters and subtitles and the subtitle files beside it.
  The page's title is the drive's make and model when it says them, the
  line under it the folder's path ("/Films/2019"), and an empty page says
  so in the middle under an icon, with the rail closed. A video that can
  no longer be read, the drive pulled out, says so with Try again, as the
  Android app does, also when it stands still after playing; one whose
  drive was pulled out while the TV showed Home says so and leaves.
  Subtitle files saved as UTF-16 ("Unicode") read right, and a link that
  leads off the drive isn't listed, played or read.
- The player's Info says the TV's network speed under How it plays,
  beside the server's Speed, as in the Samsung app: a video that stalls
  while the server keeps up is on a network that can't carry it.
- Live TV plays a channel from its own MPEG-TS stream, as the Samsung app
  does, when the server sends it as it is or with sound the TV decodes
  (AC-3, E-AC-3, MP2): it starts sooner than the server's HLS, which
  plays it otherwise and whenever the stream fails.
- In Live TV's guide, ◀◀ and ▶▶, Previous and Next, move it two hours
  back or on, while music plays too, as in the Android TV app.
- In the Live TV player, Next and Previous change channel as CH+ and CH−
  do, once a press, as in the Android TV app.

### Changed

- Pley for LG TV is a native app now, in place of the web app: it starts
  and moves faster, and looks and works as the Android TV app does. It
  installs over the web app, in Developer Mode or on a rooted TV; what the
  web app kept doesn't carry over, so choose the server and sign in again.
- The server lists this TV's sign-in as Pley for LG TV, with its webOS and
  model, with a server from the same release.
- With the TV's Audio Guidance on, the app reads each page's name, what
  has focus, dialogs and messages aloud, in the TV's own voice.
- Requests work in the native app as they did in the web app: Discover,
  a title's page with its season picker, Your requests (Requests for
  administrators), the "On TMDB" row in Search, a library title's request
  on its page, and what is ready to watch popping up once.
- Each frame takes less of the TV's processor to put its text and shapes
  together, leaving more room for scrolling and animations to stay smooth.
- Live TV's guide moves through time as the Samsung app's does: on a
  channel without programmes each Left or Right moves it a half hour, Left
  goes back through a long programme before it leaves the guide, and a
  held Left stops at the guide's start. Back in the guide or the channel
  tiles goes to the categories before the rail, and Info on a tile opens
  what's on.
- OK on a live channel opens the channel list at once, while it tunes too;
  holding OK shows the controls.
- The remote's media keys work the music from every page, not only the
  music pages, and Stop pauses it. Music that was playing when the TV
  hid the app plays on from there when the app is back.
- The player's keys work as in the Samsung app: OK with the controls
  hidden shows them rather than pausing, Down shows them on Play/Pause,
  and Left and Right show them with the seek's preview and wait for OK
  (5 s at most) before seeking, which Back cancels. A held key scrubs
  smoothly faster the longer it is held, and waits for OK when let go.
- As each title or episode begins playing, its name shows at the top of
  the picture for a few seconds, clear of Playback details when they are
  on, as in the Android TV app.
- Chapters show as a row of cards over the scrubber, each with its picture,
  name and start, in place of the Chapters panel: the Chapters button opens
  them, and so does Up while a seek waits for OK.
- A video the TV's Home interrupted comes back paused where it was, with
  the controls showing, as in the Samsung and Android apps, rather than
  playing on by itself.
- A held arrow key moves focus as in the Samsung app: from 180 ms on, about
  22 times a second at an even pace, rather than every other one of the
  TV's uneven repeats.
- Under a held arrow key, rows, pages, lists and poster grids glide
  smoothly from tile to tile instead of jumping, and each tile grows and
  shrinks with focus at the same pace rather than popping. A row slides
  under one still ring at its start, as in the Samsung app, and so do the
  rows of a library, Collections, playlists and a collection's page; the
  pictures load once the key is let go.
- A held arrow key stops at the end of a list instead of leaving it, as in
  the Samsung app: a held Left no longer opens the rail (a press does), and
  a held Up or Right stays in a library's grid, the Collections tab and
  Live TV's categories, guide and cards. Down on a grid's last row stays.
- The rail opens as in the Samsung app: its panel widens out of the closed
  rail as it fades in, and the labels slide in beside the icons.
- Search shows movies, shows and episodes first, then music, then Live
  TV, then people, as the other apps do.
- A tile's actions (OK held, or the menu key) open in a side panel over
  the page, as in the Android TV and Samsung apps, rather than on a page
  of their own. A choice closes the panel and focus is back on the tile
  first, so Back from what it opened comes to the tile, and a tile the
  choice takes away hands focus to its neighbour.
- Settings draws the Android TV app's icons: a pulse for Diagnostics, a
  speaker for each pass-through format, and their own icons for Subtitle
  background, Subtitle position, Reduce motion and Manage your account on
  the web.
- On webOS 6 and later, Back in the rail asks "Exit Pley?", as LG asks of
  apps and as the Samsung app does: Exit closes Pley, Cancel or Back
  stays. On webOS 4 and 5 it still shows the TV's Home.
- A person's round picture, in rows and on their page, sits on a plain
  grey circle while it loads or when there is none, as in the Android and
  Samsung apps, rather than on a coloured gradient.
- A video's first picture fades in from black, as in the Android TV and
  Samsung apps, rather than cutting in (at once with Reduce motion).
- An empty requests list says why in the Android TV app's words: "Nothing
  asked for yet" with where to find titles, "Nothing needs you" in Needs
  attention, "No requests match" for a status or a filter. A Discover
  search with no results says "Nothing on TMDB matches" it.
- A season partly in the library says how much of it is, "4 of 10
  episodes in library", as in the Android TV app, under its name on a
  Discover title's page.

### Removed

- The player's Back 10 seconds and Forward 10 seconds buttons, as in the
  Samsung and Android TV apps. Left and Right still seek.

### Fixed

- A Live TV channel played from its own stream no longer falls back to
  the server's HLS when something else on the TV holds connections to the
  player's relay open without asking for a stream, and a network watch
  that keeps failing as soon as it answers waits longer each time rather
  than asking again every 5 seconds.
- A programme's details in Live TV keep its description above Watch and
  the favourite, and the guide no longer lays out the whole of a long
  description each time focus moves onto a programme.
- While a seek is on its way, the scrubber's blue and its times follow
  the thumb, and seek previews and chapter pictures are letterboxed
  rather than cropped, so a wide film's preview keeps its sides.
- A chapter named only by where it starts ("00:06:19.212", as mkvmerge
  names them) is called "Chapter 3" in the player.
- An audio track whose language the file and the TV write differently
  ("vie" and "vi", "ind" and "id") switches in place instead of starting
  the stream again, and on the track asked for.
- In the player, Skip Intro and a click on the bar no longer come undone
  a few seconds later by a seek still waiting for OK.
- While the player shows an error, the media keys other than Stop do
  nothing, as in the Android TV and Samsung apps, rather than seeking,
  changing a track or playing the next episode under it.
- The Play/Pause key no longer makes a seek that waits for OK at once, as
  in the Samsung app: it only plays or pauses, and the seek lands by
  itself. OK still makes it at once.
- On a Live TV channel with sound only, the logo, "Sound only" and what's
  on rise above the bar at the foot while it shows, rather than sitting
  among its words.
- A server that keeps refusing the sign-in's events, or a proxy asking for
  its own login, no longer has Pley refresh the sign-in over and over.
- A Live TV channel that plays as HLS lets go of its stream.ts at once, as
  in the Samsung app, so the provider's connection isn't held for up to 45
  s.

### Security

- The native app sends its token to the server's address only once the
  server has proved there that it is the one the TV signed in to, as the
  web app did: the access token waits for a proof made on this network
  within the half hour, the refresh token, a profile's PIN and signing out
  for one made then. Coming back to the app, the TV's connection changing
  and a request that gets no answer ask for a new one. Another device at
  the address gets nothing, and the TV says "Another device answers
  there."

## [0.1.3] - 2026-11-02

### Changed

- Picture subtitles (PGS, VobSub, DVB) show within a few seconds of a
  playback starting or a seek, where they could take up to a minute: the
  TV tells the server where it plays, and the server reads those around it
  first.

### Fixed

- Settings' Always start as no longer starts the TV as a profile with a
  PIN without it: the start opens on that profile's PIN pad, and Back shows
  the other profiles. While the TV doesn't know whether the profile it was
  left on has a PIN, the start asks "Who's watching?".

### Security

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

## [0.1.2] - 2026-11-02

### Changed

- A library's posters show 4K and the dynamic range (DV, HDR10) whatever
  the sort, where they showed only when sorted by bitrate, and no longer a
  lower resolution (1080p, 720p).
- "Enter an address" opens the TV's keyboard at once, on an empty field,
  in place of the number pad, so a server's name or a whole https address
  is as easy to type as an IP. The remote's digit keys still type once the
  keyboard is closed.

### Fixed

- A profile that got a PIN after "Who's watching?" was loaded opens the
  PIN pad, and one whose PIN is locked or asked too often says so on it,
  as on Android TV; a refusal of another profile's switch no longer goes
  on as the profile in use.
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

[Unreleased]: https://github.com/pley-media-server/pley/compare/release-2026-11-02.2...HEAD
[0.1.3]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-02.2
[0.1.2]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-02
[0.1.1]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-01
[0.1.0]: https://github.com/pley-media-server/pley/tree/94985f768f39f4a230060cb47af9498ee8779ddf
[0.0.1]: https://github.com/pley-media-server/pley/tree/7e14bb9d498d054a73b2176cd8949b6864ffacd9

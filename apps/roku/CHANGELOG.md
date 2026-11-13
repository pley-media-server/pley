# Changelog

Notable changes to Pley for Roku, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `release-<date>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Fixed

- The remote works on the screen that opens, such as a show's Episodes,
  where on some Rokus it kept moving the page underneath.
- On some Rokus, lists no longer skip presses, such as Down through a
  season's episodes, and pages no longer stall on Loading or say the
  server can't be reached while it can.
- Home's rows scroll to the row you move to, and on some Rokus the title
  you're on in a row is highlighted again.

## [0.1.8] - 2026-11-12

### Added

- When the server has video transcoding turned off, the Quality menu
  offers only Original.

## [0.1.7] - 2026-11-06

### Added

- Pley for Roku is in French and Spanish as well as English: your
  account's language, else the one picked on the Roku, else the Roku's,
  else the server's. Settings → Language picks your account's, on all
  your devices, or Automatic; on a server from before languages, which
  can't keep your account's, it keeps your choice on this Roku. Dates,
  times, numbers and counts follow the language, and the app starts in
  the one it showed last.
- A globe at the bottom right of Choose your server, and at the top right
  of the sign-in code, picks the Roku's language before anyone signs in:
  Automatic (the Roku's own), English, Français or Español.
- "Play on…" from someone else's profile in your household plays on the
  Roku as their profile: the Roku switches to it first, without asking
  its PIN (the phone or browser did). What was playing stops first, and
  where it got to is saved for the profile that was watching. When the
  switch can't be made, the Roku says so and plays nothing.

### Changed

- The Roku starts straight into Home as the profile it was left on when it
  has no PIN, where a household of several profiles was asked "Who's
  watching?" each time. A profile with a PIN still asks first, and Switch
  profile still shows every profile. Settings' Always start as
  calls this Profile used last, where it said Ask who's watching.
- A pause from your phone or browser no longer brings up Roku's controls
  on the TV: the player says what the remote did for a moment at the top
  ("Paused", "Playing", "+30 s", "Jumped to 12:34", the audio, subtitles
  or quality picked). Your own key brings the controls back.

## [0.1.6] - 2026-11-05

### Added

- Your phone or browser changes the quality of a film or episode playing
  on the Roku, as the player's Quality menu does (the pick is kept as the
  Roku's), and sees which is picked. A film or episode sent with "Play
  on…" plays with the audio and subtitles picked on its page.
- Your phone or browser plays a film, an episode, a show, an album, an
  artist, a track, a playlist or a Live TV channel on the Roku ("Play
  on…"): it starts as the Roku's own Play would, in place of what plays,
  and the Roku says "Playing from" the device it came from. It needs an
  `http://` server, as the rest of the remote does.
- Your phone or browser changes the audio and subtitles of a film or
  episode playing on the Roku, as the player's own menu does (a pick is
  saved for the show or movie), and sees which play. Like the rest of the
  remote, it needs an `http://` server.
- Your phone or browser controls what plays on the Roku: a film or
  episode plays, pauses, seeks and skips, and goes on to the next one;
  music does the same, and goes back a track; Live TV changes channel.
  Remotes list the Roku by the name it has in Roku's settings, and need
  an `http://` server: with `https://`, they offer only Stop. Music
  stopped from another device or by the server's administrator now stops
  too, and Live TV stopped that way says who stopped it, with their
  message.
- The player's menu (Down) has Subtitles and Audio, listing every track of
  the file, picture subtitles too, as the other apps' players do. A pick is
  saved for the show or movie.
- The player shows the title at the top for a few seconds as each item
  begins.
- When the server is out of reach for a moment, the player says
  "Reconnecting…" and goes on where it was, its queue kept, for up to
  90 s, where it used to leave for a screen saying the server can't be
  reached.
- Home has a hero for the first of Continue Watching, with Resume, From
  start, Info and the watched mark, and in its rows an info block about
  the title focus rests on, the backdrop following it, as on the other TVs.
- Collections, in Home's menu, shows every library's collections, a row
  each.
- A title's page says who made it and who is in it ("Directed by …
  · Starring …"), and its More has Details: the whole summary, the crew,
  the original title, the release date, the studio or network and how
  many times it was played.
- An album's page has Go to artist, and Now playing while music plays;
  Now Playing's scrubber, Up from its buttons, moves 10 s with Left and
  Right.
- A playlist entry's Options key goes to its title's page.
- The live player's Options key adds the channel playing to the
  favourites or takes it out.
- The live player's channel list shows another category or a search, and
  a channel chosen there makes channel up and down go through that one.
- The Options key on a programme in Live TV's guide, or on a card, shows
  its details.
- The requests' tabs say how many each holds, and Discover's way to the
  requests how many wait there, in red while some couldn't be sent.
- A Discover grid whose next page fails ends in a tile to try again.

### Changed

- Requests: a sign-in from before the app could ask says "Sign in again to ask for movies
  and shows" without a second line, and its button reads Sign in.
- Up Next waits at the end of an episode once cancelled, or when the next
  episode doesn't play by itself, where the player used to leave.
- A video that stands still for 12 s is converted or opened again, and the
  player says when the server can't convert it fast enough.
- A title the server refuses to play offers Try again where asking again
  may help.
- Playback details stay up to date while they show.
- Home's menu is in the other TVs' order: Search, Home, Discover and the
  requests, Live TV, the libraries, Playlists, Collections, Now Playing,
  then Settings.
- Home with nothing to show says why: no libraries yet, a scan under way,
  or nothing in them yet.
- Live TV opens at the last channel watched, after a restart too.
- Back in Live TV's guide goes back to now first, and from a search to the
  category before, then to the categories; Clear in the search goes back
  to the category before too.
- A Live TV channel that fails is tuned again after 1 s and then 3 s
  before the player says why.
- A page of channels or a part of the guide that didn't load is asked for
  again; with nothing to show, Live TV offers Try again.
- What's on in Live TV and in the live player moves on as programmes end.
- When every connection is in use and none can be taken over, the notice
  says what can be done instead.
- When trending has nothing the profile may ask for, Discover's hero is
  the first title of the next row that has one.
- Search's Discover row looks through up to 3 of TMDB's pages for up to
  20 titles the libraries lack, so it no longer goes missing when TMDB's
  first page is all in the libraries.
- Without the event stream, the requests and Discover's Your requests are
  asked again every 30 s.
- Ready-to-watch news shows in a card at the bottom right with the
  title's poster, Open and Close, once the remote rests, as on the other
  TVs. It gives the focus back when closed, goes by itself after 10 s,
  and never shows on the requests.
- A movie or show in Discover or Search's Discover row that a library has
  whole opens its library page, where it plays, not a page with only Open
  on it.

### Fixed

- Live TV's Up or Down, still finding the next channel when another is
  picked from the list, no longer changes channel a moment later.
- A video resumed past the end of what its file holds is converted or
  opened again, instead of being counted as watched.
- The server hears of a seek at once, so the title resumes from there on
  other devices.
- Chapters are offered in the player's menu only for a file with two or
  more.
- Choosing a Live TV category or searching takes the focus to its
  channels.

## [0.1.5] - 2026-11-04

### Changed

- The channel's tile on the home screen shows Pley's mark above its name,
  centred.

## [0.1.4] - 2026-11-03

### Added

- A library of movies or shows filters by resolution (4K, 1080p, 720p, SD)
  and dynamic range (Any HDR, HDR including Dolby Vision and HDR+, Dolby
  Vision, HDR+, HLG, SDR) in its
  Filter panel, offering those its titles have. Clear filters clears them,
  the library keeps them, and Save as… saves them.
- A library sorted by bitrate, or filtered by resolution or dynamic range,
  badges its posters with "4K" and their dynamic range ("DV", "HDR+",
  "HDR", "HLG"), as the other apps do. Sorted by bitrate, each poster
  shows its bitrate and size under it ("42 Mbps · 23.4 GB").

### Changed

- Search shows movies, shows and episodes first, then music, then Live
  TV, then people, the closest matches first in each.
- A Live TV channel whose connection someone else takes over stops at
  once and says why, under "Live TV stopped", with Watch again, where it
  said so only once its stream had broken, with no way to watch again
  but Other channels.
- Search shows every title, person and Live TV channel it finds, up to 200
  of each, where it stopped at 40 titles and 12 people and channels.

## [0.1.3] - 2026-11-02

### Changed

- PGS subtitles are drawn by the app over the video instead of being burned
  in by the server, so a file with them plays as it is and keeps its HDR,
  and subtitle sync moves them without the video stopping. A server too old
  to make their pictures still burns them in, and so does every server for
  VobSub and DVB.

### Fixed

- Settings' Always start as no longer starts the Roku as a profile with a
  PIN without it: the start opens on that profile's PIN pad, and Cancel
  shows the other profiles. While the Roku doesn't know whether the
  profile it was left on has a PIN, as at the first start after this
  update, the start asks "Who's watching?".

### Security

- Before the token goes to the server's address, the server proves it is
  the one the Roku signed in to, at that address: the access token, which
  the player and the pictures use too, waits for a proof made on this
  network within the half hour, the refresh token and a profile's PIN for
  one made then. Another device at the address on another network gets
  nothing, nor does one passing the challenge on to the server from
  elsewhere; the Roku says "Another device answers there." One that takes
  the address on the Roku's own network may get an access token (an
  hour's) within the half hour, never the refresh token. A sign-in from
  before starts checking within the hour, as its token is renewed.
- Changing servers no longer lets the event stream connect to the server
  chosen with the token of the one before, before either proved anything:
  that token is withdrawn before the address changes.

## [0.1.2] - 2026-11-02

### Added

- Seek previews: fast forward and rewind show pictures of where they go, in
  Roku's own bar, once the server has made the file's previews. Needs a
  server from the same release.
- A title's page rates it: No rating, or 1 to 5 stars, as on the other
  apps; once given, More says it (Your rating: 4 stars).
- A title's page has More…, as on the other TVs: the rating, Go to show
  and Add to playlist, in a panel at the right, which keeps the column of
  buttons short.
- A movie's or an episode's page has Audio and subtitles: the audio
  (Automatic, named by its language, or a track) and the subtitles (Off or
  a track) to play, saved for the movie or show before it plays.
- A title with several files plays the version chosen in Audio and
  subtitles, one line each (4K · HEVC · DV P8.1 · TrueHD Atmos 7.1 · 62.0
  GB).
- More… → Media info: each file of a movie or an episode and every one of
  its streams in full, as on the other apps.
- A title with a trailer among its extras has Trailer on its page, which
  plays it from the start.
- An episode's More… and the Options key on its tile have Go to season,
  its season's episodes; Go to show opens the show's page, as on the
  other apps.
- A playlist's entries move: the Options key's Move lifts one, Up and Down
  carry it, OK puts it there and Back puts it back.
- Settings has Always start as, for a household of several profiles: the
  app starts without asking who's watching.
- The player's menu has a sleep timer: 15 to 90 minutes or the end of
  what plays, then the video pauses; in its last 30 s, Keep watching adds
  the same time again.
- Settings says where to manage your account on the web: name, picture,
  password, PIN, two-step sign-in, devices and apps.
- Find subtitles… looks in another language too: Language, above the
  results, picks it.
- Settings has Diagnostics, what this Roku tells the server it plays, and
  the open-source licences, as the other TV apps do.
- A collection has a page of its own, as on the other TVs: its backdrop,
  TMDB or Smart, what it holds and its summary, Play and Shuffle, then its
  posters. Play plays a collection of movies in its order, Up Next
  bringing on each next; a collection of shows plays a show's next
  episode.
- A person's page has their round photo and how many of their movies and
  shows are in the libraries, then a row for each thing they did (Acting,
  Directing, Creating, Writing, Producing), each title with its year and
  the part.
- An artist's page has Play and Shuffle, which play every track of theirs
  with the music player, and Now playing while music plays; their albums
  are newest first, each with its year.
- Playlists has New playlist: Movies & TV or Music, then a name typed on
  the Roku's keyboard; the new playlist opens.
- Now Playing lists Up next, the queue with the track playing in bold, as
  the other apps do; OK on one plays it.
- Now Playing has Shuffle, and Stop shuffling puts the album's or
  playlist's order back. An album or a music playlist played from a track
  plays whole, so Previous goes back past where it started.
- Music has a sleep timer, on Now Playing: each OK steps through 15 to 90
  minutes and the end of the track, the button saying what is left; in
  its last 30 s, Keep listening adds the same time again; then the music
  pauses until Play.
- An album of several discs lists its tracks under Disc 1, Disc 2 and so
  on, as the other apps do.
- A library sorted by title has an A to Z beside its grid: Up and Down pick
  a letter (those no title starts with are dimmed) and OK goes to its first
  title, however far down.
- Save as… in a library's Filter panel saves the filters and the order as
  a smart playlist, named on the keyboard, which keeps up with the library.
- See all ends Home's Recently Added rows, and a library's Recently Added,
  Recently Released and Top Rated rows: the library's titles in the row's
  order.
- Search shows its results in rows by kind, as the other apps do: People,
  Live TV, Movies, Shows, Seasons, Episodes, Artists, Albums and Tracks,
  then Discover.
- Search lists your recent searches before anything is typed: OK searches
  one again, Clear forgets them.

### Changed

- Entering a server's address says that port 7539 is used by default.

### Fixed

- Too many wrong PINs keep the PIN pad, saying how long to wait, and a
  profile that got a PIN after "Who's watching?" was loaded asks it
  without calling it wrong.
- "Who's watching?" as the app starts asks the PIN of the profile it was
  left on when that profile has one, as on the other TVs, and shows "PIN"
  under it.
- A paused video or track goes on telling the server where it is, every
  30 s, as the other apps do, so a long pause no longer drops the server's
  session: it stays on the dashboard's Now playing and plays on at once.
- A library's Continue Watching row has wide pictures, as Home's, and the
  Options key on it offers Remove from Continue Watching.

## [0.1.1] - 2026-11-01

### Added

- Requests: where the server has them on, people ask for movies and shows
  found on TMDB, and administrators approve them. Discover, in the menu,
  has a hero for the first trending title and rows (Trending this week, Popular
  movies, Popular shows, Coming to cinemas, Popular anime, Your requests)
  of posters marked "In library", "Requested" or "Declined", each loading
  as it comes into view, and its own search.
- A TMDB title's page: its one main action (Request, Request more seasons,
  Open, Tell me too) or why there is none, its request's stage, and its
  seasons, each saying how it stands; Which seasons? picks those to ask
  for.
- Your requests: tabs by status and a card per request with its stage
  (downloading with a bar and the time left), the reason or note; OK opens
  the title, the Options key offers Watch, Withdraw, Remove, Stop following
  or Request again.
- Administrators' Requests open on Needs attention and filter by movies or
  shows, person and title; the Options key offers Approve, Try again,
  Decline, Decline with a reason… (typed on the Roku's keyboard), Mark as
  added by hand and Delete. The menu's number counts what waits, red while
  some failed.
- A requested title that arrives is told of ("… is ready to watch." with
  Open), never over playing video.
- Search shows titles "On TMDB" after the libraries' own, for those who
  may ask. A library title that was asked for shows its request's stage
  under the poster, and a show with seasons left to ask for ends its
  seasons with Request more seasons.
- Signing in asks for requests and approvals where the server offers them.
  Settings carries TMDB's notice.
- Signing in with a code says when the server requires two-step sign-in:
  the phone that approves the code sets it up for whoever hasn't.

### Changed

- Pley for Roku is free software under the GNU Affero General Public
  License, version 3 (`AGPL-3.0-only`, `LICENSE`).

### Fixed

- Home's menu shows or drops Live TV as soon as it is set up or its last
  source removed, as it does for libraries.

## [0.1.0] - 2026-10-30

### Added

- Live TV, where the server has channels, by category:
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

[Unreleased]: https://github.com/pley-media-server/pley/compare/release-2026-11-12...HEAD
[0.1.8]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-12
[0.1.7]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-06
[0.1.6]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-05
[0.1.5]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-04
[0.1.4]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-03
[0.1.3]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-02.2
[0.1.2]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-02
[0.1.1]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-01
[0.1.0]: https://github.com/pley-media-server/pley/tree/94985f768f39f4a230060cb47af9498ee8779ddf
[0.0.1]: https://github.com/pley-media-server/pley/tree/7e14bb9d498d054a73b2176cd8949b6864ffacd9

# Changelog

Notable changes to Pley for Android: phones, tablets, Android TV and Fire TV, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `release-<date>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Added

- The phone is a remote for your other devices: while something plays on
  one of them, a bar above the tabs says where and what, and opens a
  remote with what plays on each, how far, and the controls its player
  obeys (play and pause, back 10 s and forward 30 s, next and previous,
  channel up and down, a slider to seek), and Stop. A device that doesn't
  act on a command within a few seconds is said not to have answered.
  TVs show no remote.
- The players obey the remote on your other devices, on phones and TVs:
  videos play, pause, seek, skip and go to the next episode; music does
  all that and goes back a track; Live TV changes channel, and stops with
  a word when stopped from elsewhere. Music playing with the screen off
  still hears it, until it has been paused for a minute and a half. Remotes show the device by its name in Android's
  settings.
- The video players change their audio and subtitles when your remote on
  another device asks, as their own track menus do, and tell your remotes
  which tracks they play.
- The phone's remote lists your devices: every other device that is on,
  playing or ready to play, those that play first, each with an icon for
  its kind. Choosing one shows its controls, now with Next for every
  episode, track and channel (dimmed where it can't go on), and Audio and
  Subtitles menus that change what it plays.
- Phones, tablets and TVs play what your remotes send them while Pley is
  open: a title, a show's next episode, music, a playlist or a Live TV
  channel plays as Play would play it there, and says for a few seconds
  which device it came from.
- Play on… sends a title, a show, a season, music or a playlist from the
  phone to another of your devices, and Watch on… a Live TV channel:
  beside Play on their pages, in an item's actions and in the live
  player. Choose the device (an album, an artist or a playlist can go
  shuffled), and the remote opens on it.
- The video players change their quality when your remote on another
  device asks, as their own Quality menu does, and tell your remotes
  which quality they play. Phones and TVs play a title cast to them with
  the audio and subtitles picked on the remote's page, for that play
  only.
- The phone's remote has a Quality menu beside Audio and Subtitles, all
  three as icons, and Stop asks first. A Live TV channel shows its
  programme, how far it is, its times and what's next, with a channel
  rocker in place of the seek bar and the skips. Play on… on a title's
  page is a square button as tall as Play, and sends the audio and
  subtitles picked on the page.
- Diagnostics can show the frame rate (on TVs, Settings → Diagnostics):
  while something moves, a small box at the top gives the frames
  drawn a second, the longest frame and how many were late, and the same
  goes to the device's log.

### Changed

- Requests: a sign-in from before the app could ask says "Sign in again to ask for movies
  and shows" without a second line, and its button reads Sign in.
- On Android TV and Fire TV, a key held through a library's grid, along
  Home's rows or down through them scrolls more smoothly: tiles cost far
  less to bring in and to draw, rows are made ahead of the screen, and a
  held step waits a frame or two for its row rather than stall the
  picture. A show's page and the guide make their next rows ahead too.
- On Android TV and Fire TV, a held direction key stops at the end of
  the list it moves along, on every page: Up held through Home's rows
  stops on the first row rather than going on to the hero, Down held
  through a library stops on its last row. A press of its own goes on.
- When the server can't be reached, the app says "Pley is offline", with
  Downloads beside it on phones and tablets, where it said "Showing what
  Pley had saved. Reconnecting…". It still shows what it saved and
  reconnects by itself.

### Fixed

- On Fire TV, Amazon's keyboard shows what you type: its box kept the
  words it opened with (nothing for a new playlist, the old name for a
  rename) while each key reached the field. Its Back closes it and keeps
  the words, as Gboard's does, where it acted as Done: it saved the
  name, made the playlist or ran the search.
- On Android TV and Fire TV, a press of a direction key that lasts a
  little long (200 or 300 ms) moves focus once, not twice: a held key
  starts moving by itself after 400 ms, as the system's own repeats do,
  rather than 180.
- On Android TV and Fire TV, OK held on a tile opens its actions and leaves
  them open: the key, still held as they came, went on to choose the first
  of them (Resume or Play) as it came up.
- On Android TV and Fire TV, screen readers say each card on Playlists,
  its title and what it holds; they said nothing.
- On phones and tablets, the player's controls are in their place as it
  opens, rather than moving a moment later: the buttons along the bottom
  show once the server has answered, and the controls keep clear of the
  status and navigation bars whether they show or not.

## [0.1.6] - 2026-11-04

### Changed

- The banner on Android TV and Google TV's home screen shows Pley's mark
  and name centred, larger, the name closer to the mark.

### Fixed

- Search no longer closes the app when a recent search is "discover" or
  "header" on a phone, or "pley.clear" on a TV.

## [0.1.5] - 2026-11-03

### Added

- Requests on Android TV and Fire TV, as on the Samsung and LG apps: the
  rail's Discover after Home (Your requests where the profile may not
  ask, Requests for administrators) with the number that waits there, in
  red while a request couldn't be sent; Discover's hero, rows and See all
  grids; a title's page with Which seasons? in a side panel; and the
  requests, with their tabs, the administrators' filters and a side panel
  of what may be done to each. What became ready to watch pops up at the
  bottom right once the remote rests, with Open and Close.
- On Android TV and Fire TV, Search shows Discover after the libraries'
  results, up to 20 of TMDB's titles the libraries lack, and a library's
  movie or show from a request shows its stage, with Request more seasons
  for a show missing some.
- On Android TV and Fire TV, Settings → About credits TMDB.
- On TVs, the title shows at the top for a few seconds as each item
  begins, the next episode too, while the player's controls are hidden
  and Playback details aren't over the video.
- Android TV and Fire TV play videos from a USB drive, with no server: while
  one is plugged in, the rail has USB drive after Now Playing. Its page,
  titled with the drive's make and model when it says them, gives the
  folder's path and lists the drive's folders and videos, sorted as a
  person sorts names, with where each video resumes, and a video plays in
  Pley's player with the subtitle files beside it (UTF-8, UTF-16 or
  UTF-32, or Windows' Western code page, its curly quotes and euro
  included), from where it stopped. A video whose picture never starts
  says the TV doesn't play it; one that played and then stands still,
  that the drive may have been pulled out. An empty folder or no drive
  says so under an icon, with focus on the page, so the rail closes over
  it. The page asks for leave to read the drive as it opens.
- Movie and show libraries filter by resolution (4K, 1080p, 720p, SD) and
  dynamic range (Any HDR, HDR including Dolby Vision and HDR+, Dolby
  Vision, HDR+, HLG, SDR) where they
  have more than one: chips beside Genre and Year on phones, rows of the
  Filter panel on TVs with a chip for the one chosen. Clear filters clears
  them, Save as… keeps them, and the library opens with them next time.

### Changed

- Library posters carry their quality badges ("4K", "DV") while filtered
  by resolution or dynamic range, as they do sorted by bitrate, and
  nowhere else.
- On TVs, the rail lists up to six libraries before Libraries, and no
  entry leaves it to make room for another: its entries are a little
  smaller, and scroll between the avatar and Settings when they don't all
  fit, keeping the focused entry fourth from the top, as the app's lists
  keep theirs.
- Search shows every title, person and Live TV channel the server finds,
  up to 200 of each, where it stopped at 60 titles and 12 people and
  channels.
- On phones, the first scrolls after an install or an update are smoother:
  the app comes compiled ahead for the phone's screens too, not only the
  TV's.
- Chapters named only by where they start ("00:06:19.212", as mkvmerge
  names them) show as "Chapter 3" on phones and TVs, as untitled ones do.
- On TVs, a held Left or Right scrubs on a smooth curve, twice as fast
  every 1.5 s, up to a whole video in 10 s, as in the native Samsung app.
- While a seek is coming on TVs, the scrubber's played part and both its
  times stand where it goes, with the thumb, rather than at the playhead.
- On TVs, Left and Right show the controls with the preview and wait for
  OK before they seek, as YouTube's TV app does: OK goes there, it goes by
  itself 5 s after the last key, the controls going with it, and Back
  lets it go. A held key let go waits there too, and Previous, Next or
  Skip take the place of a seek waiting. Fast forward and rewind seek as
  before.
- On TVs, OK with the player's controls hidden shows them on the scrubber
  instead of pausing; OK there plays and pauses.
- On TVs, Play/Pause, the player's button or the remote's key, plays and
  pauses even just after a seek, and a seek waiting still goes; only OK on
  the scrubber goes there at once, as in the native Samsung app.
- On TVs, Down with the player's controls hidden shows them on Play/Pause,
  in the middle of the row, rather than on Subtitles.
- On TVs, the chapters show as a row of cards over the scrubber, with
  their pictures, as in YouTube's TV app, in place of the Chapters panel:
  Up while a seek waits opens them on the chapter it was going to, the
  Chapters button on the one playing; OK goes to one, and Down, Back or a
  media key's seek (Previous, Next, fast forward or rewind) closes them.
- Seek previews keep the video's shape, letterboxed on black, on phones
  and TVs, rather than cropped to fill their box: a wide film's no longer
  loses its sides.
- On TVs, a video's first picture fades in from black, as the player's
  controls do, rather than appearing at once (at once with reduced motion).
- In the TV live player, OK opens the channel list, as in the native
  Samsung app; holding OK shows the controls (the channel list and the
  favourite).
- Previous and Next change channel in the TV live player, as Channel up
  and down do.
- The live player shows the channel's logo, name and what's on as it
  opens from Live TV or search, where it was black until the server had
  answered for the channel.
- A live channel whose player fails is tuned again twice, after 1 s and
  then 3 s, before the player says why, as on Samsung TVs; not when the
  server refused it.
- Live channels start a round trip sooner: the player reads the
  channel's stream as it asks the server for the channel, rather than once
  the server has answered, as in the native Samsung app.
- A live channel with AC3 or E-AC3 sound starts sooner where the device
  decodes that sound or sends it to a receiver: it plays from the
  channel's own stream, as in the native Samsung app, rather than from the
  HLS the server makes with the sound converted.
- In the TV guide, Left goes back through a programme that began before
  the guide, a screen at a time, before it leaves for the categories; on a
  channel without a guide, Left and Right move the guide a half hour at a
  time, as on Samsung TVs.
- In the TV guide, rewind and fast forward (or Previous and Next) move it
  two hours, as in the Samsung and LG web apps, and the time now shows in a
  pill over the now line.
- Back on Live TV's page on TVs goes up a step at a time, as in the native
  Samsung app: the guide back to now, a search back to the category
  before, the guide or the tiles to the categories, and only then the
  rail.
- On Live TV's page on TVs, a held Left stops at the start of the guide
  or the first column of tiles, and a held Up at the top, rather than
  running on to the categories, the rail or the tabs; a press of its own
  goes on, and in the guide not within half a second of Left moving.
- The TV live player's channel list moves as the app's other lists do
  under a held key: at an even pace, without asking for every logo it
  passes, its focused row a third of the way down.
- Info on a channel's tile on Live TV's page on TVs opens its details, as
  it does in the guide and on Samsung TVs.
- A programme with a long description no longer holds each move along a
  row of the TV guide: what shows over the grid is cut to its two lines.
- The TV guide keeps up with the remote on slower TVs: moving along a
  row or down the channels no longer stutters.
- Under a held Up or Down the TV guide's channels keep up with focus, a
  row at a time, where each repeat began their scroll again; it opens on
  the channel last watched without scrolling there.
- When the TV guide's programmes come in for a screen of channels, the
  channel with focus shows them at once and the others over the next few
  frames, nearest first, as on Samsung TVs, rather than all in one long
  frame.
- On phones and tablets, the posters of search results, a person's page, a
  collection and a library's Collections tab load ahead of the scroll, as
  Home's rows and a library's grid already did.
- On phones and tablets, a video's first picture fades in from black,
  rather than appearing at once.
- On phones and tablets, the next episode, or a queue's next, shows the
  controls with its title as it begins, as the first does.
- On Android TV and Fire TV, a held direction key starts moving after
  180 ms rather than half a second, and moves about 22 times a second at
  an even pace, as on the native Samsung app.
- On Android TV and Fire TV, a held Left or Right slides a row on evenly
  under a ring that stays still, its tiles at rest, rather than jumping a
  tile at a time; focus lands on the tile under the ring when the key
  comes up, as on the native Samsung app. A season's episodes move as
  every other row does, and a press moves a row over 150 ms, eased out.
- On Android TV and Fire TV, a held Up or Down glides a page of rows or a
  library grid on without stopping, rather than jumping a row at a time;
  in a grid the posters stay at rest under one ring until the key comes
  up. A press moves pages and grids over 150 ms, eased out, as on the
  native Samsung app.
- On Android TV and Fire TV, a held Left stops at a row's first tile
  rather than opening the rail, and a held Right or Up stays in a
  library's grid rather than running on to the A to Z jump, the chips and
  the tabs; a press of its own still reaches them, as on the native
  Samsung app.
- On Android TV and Fire TV, pictures wait while a direction key is held:
  the posters a row or page passes ask for nothing, and those where the
  key comes up load then, so a weak TV spends nothing on what flies by.
  Pictures already in memory still show at once. The rows of item pages,
  search, people, collections and Recommended now load their pictures
  ahead of the scroll, as Home's do, and a list stops loading pictures it
  has moved past.
- On Android TV and Fire TV, Search's recent searches stay put while the
  chip you move to shows, and move only when it would pass the edge,
  gliding as rows do, rather than bringing each chip to a third of the
  way across.
- On Android TV and Fire TV, a tile's focus look comes in and goes with
  the native Samsung app's curves (150 ms eased out, 120 ms eased in), the
  tile a held key stops on takes its look the same way, and Up from Home's
  rows glides back to the hero over 150 ms rather than springing there.
- On Android TV and Fire TV, an item page glides to its episodes and rows,
  and back to its buttons, as Home does: over 150 ms, eased out, for a
  press, and on without a stop under a held Up or Down.
- On Android TV and Fire TV, a title's actions (holding Select, or Menu)
  open in a panel over Home, a library, search, a collection, a person's
  or an artist's page, as they do on an item's page, rather than on a
  page of their own: the page stays as it was, and focus comes back to
  the title.
- On Android TV and Fire TV, Back to the page the rail opened (Home, a
  library, search) shows it at once as it was left, focus included, rather
  than building it again with its pictures.
- On Android TV and Fire TV, the rail and the side panels move as in the
  native Samsung app: they open quickly and settle, the rail's names sliding
  in as they appear, the rail's entries grow on focus the same way and
  shrink back slow to start, and panels leave the same way.
- Search shows movies, shows and episodes first, then music, then Live
  TV, then people, the closest matches first in each.

### Fixed

- When someone takes the connection of the Live TV channel you watch, the
  player stops at once and says why ("Live TV stopped", the person and
  their channel only when you are both in the home), with Watch again,
  where it played on until the stream ran out.
- On Android TV, the rail's grey panel fades into the dark under its
  entries over the last of its slide closed, where it went at once at the
  end.
- On TVs, Back from a title or from a tile's actions, and Right from the
  rail, land on the library grid's tile focus left, in its first row too,
  the grid as it was, where they went to the first tile or to the Sort
  chip.
- On TVs, a held Up in a library grid keeps its column and its focus, as
  a held Down does, where it could jump to the end of the row above, end
  on another column or leave no tile focused.
- On TVs, Up from a show's episodes goes to the tab of the season shown,
  where it could go to the next season's tab and show that season.
- Discover's hero is the first title of the next row when the trending
  row has nothing the profile may ask for, where it stayed a grey
  placeholder.
- Back leaves the TV guide on a channel without guide data, or a row
  still loading, where it went on bringing the guide back to now.
- The progress bar of a programme on now in the TV guide ends at the now
  line; for one begun before the guide, it ran past it.
- A programme's details on TVs keep Watch and the favourite whole, one
  under the other, and cut what the guide says to the lines left above
  them, as on Samsung TVs; with a picture and a long title the buttons
  were squeezed, and the favourite's label wrapped.
- A page of Live TV channels that didn't load is asked for again once
  10 s have passed, as the list moves, where its rows stayed blank until
  another category was chosen.
- Moving through Home's rows stays smooth on coming back from a title,
  and on Android TV and Fire TV moving along a row no longer stutters.
- On Android TV and Fire TV, a library's Genre and Year panels open with
  focus on the genre or year chosen, however far down the list it is,
  where focus was left behind the panel.
- Pictures seen before come back at once when a page shows them again:
  no title in their place while they load, and no fade.
- With Reduce motion on (or the system's Remove animations), pictures
  appear without fading in, as the setting says they will.
- On Android TV and Fire TV, Reduce motion (in Settings, or the system's
  Remove animations) stills the rail too: it opens and closes in place.
- On Android TV and Fire TV, Up, Down or Right pressed while nothing on
  the page has focus, as when what had it went away, brings focus
  back to the page, where the key did nothing.
- Paused music stays in the server's Now playing, and a converted track
  plays on at once after a long pause, where it started over. A video or
  a live channel ends the music's session as well as pausing it, and Play
  starts the music again where it was.
- A remote's Stop key pauses music and keeps its queue, where it ended
  the music and took Now Playing out of the TV's rail.
- A track that stops for a reason the player can't name (the server
  answering an error, say) no longer ends at once as "This track can't be
  played": the app asks the server, and plays it again as a conversion or
  in a new session, or says the server stopped it.
- On TVs, Now Playing dims and wakes with a short fade (none with reduced
  motion), and Down from its controls lands on the track playing.
- On TVs, the player's Info shows the playback's details as soon as the
  server sends them, rather than "Asking the server…" for up to a second.
- While the TV player shows an error, the remote's media keys and CAPTIONS
  do nothing, rather than seeking, changing the subtitles or playing the
  next episode under it; Stop still leaves.
- On TVs, the sleep timer's notice keeps its width as the count goes under
  10 s, so Keep watching no longer moves under the focus.
- On phones and tablets, the seek bar, the volume and the brightness keep
  up with a finger dragging them, without stutters.
- On phones and tablets, the sleep timer's notice keeps its width as the
  count goes under 10 s, so Keep watching no longer moves under the finger.
- On phones and tablets, seek previews that arrive while a finger rests on
  the seek bar show at once, rather than when the finger moves again.
- On Android TV and Fire TV, Search's Live TV channels move as the other
  rows do: the channel focused glides to the row's left edge, and a held
  Left or Right slides the row under a still ring, as in the native
  Samsung app.
- On TVs, Up from a collection's titles to its buttons glides the page
  back to its top over 150 ms, as Home does, and with Reduce motion goes
  there at once.
- A reason for declining a request cut to the server's 500 characters no
  longer ends in "?" where the cut fell inside an emoji.

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

[Unreleased]: https://github.com/pley-media-server/pley/compare/release-2026-11-04...HEAD
[0.1.6]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-04
[0.1.5]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-03
[0.1.4]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-02.2
[0.1.3]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-02
[0.1.2]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-01.2
[0.1.1]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-01
[0.1.0]: https://github.com/pley-media-server/pley/tree/94985f768f39f4a230060cb47af9498ee8779ddf
[0.0.1]: https://github.com/pley-media-server/pley/tree/7e14bb9d498d054a73b2176cd8949b6864ffacd9

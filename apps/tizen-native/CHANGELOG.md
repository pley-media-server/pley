# Changelog

Notable changes to Pley for Samsung TV, native (Tizen .NET), newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
It has no releases yet: it is built and installed by hand
([docs/TV_PLATFORMS.md](../../docs/TV_PLATFORMS.md#the-native-samsung-app)).

## [Unreleased]

### Changed

- The icon shows Pley's name under the mark, as the other apps in the
  TV's apps bar show theirs, in the Android app's icon's colours.
- A film resumed part-way starts sooner: the TV's player begins at the
  resume point rather than fetching the film's start and seeking, its
  playlists are asked for while the player is set up, and the screen and
  its spinner keep moving while the film opens.

### Added

- A profiling build of the native app is an app of its own, "Pley (debug)",
  with a DEBUG icon on a yellow band, so it installs beside the app rather
  than in its place; the app's own icon is the plain logo. The metrics
  listener takes its screenshots and presses the remote's keys for it, so
  the app can be driven from the computer. A command reaches the TV at
  once, where the app asked for one every second. It records an animation
  frame by frame, slowed down, for a pull request to show
  (`scripts/mark-animation.mjs`).
- A native app for Samsung TVs with Tizen 9, beside the web app: so far the
  Android TV app's "Choose your server" screen, with servers found on the
  network, an address typed on the TV's keyboard and forgetting a server; its
  sign-in with a code and a QR code, the tokens kept sealed by the TV's key
  manager; Who's watching? with PINs; and Home, with the hero, the
  server's rows and the rail; item pages, with seasons and episodes, Extras,
  Cast, More Like This and the side panels; in Roboto as on Android, driven
  by the remote, a held key sliding a row along under a still highlight;
  pictures load as rows come near the screen, and a tile without one shows
  its title on a gradient of its own, as on Android.
- Library pages in the native app: the name and tabs, the chips (Sort,
  Filter, Unwatched, Genre, Year) and their side panels, the grid of
  posters with the A to Z jump, Recommended and Collections; and the
  Libraries page, where the profile can sort them. The rail's libraries open
  them.
- A tile's actions in the native app: OK held or the Menu key on a tile on
  Home or a library page opens them in a side panel (Play, the watched
  marks, Continue Watching, My List and the pages they lead to).
- Playlists in the native app: the rail's Playlists opens their covers, and
  a playlist's page plays or shuffles it; holding OK or More on an entry
  plays from it, goes to it, moves it up and down or removes it, and the
  playlist can be renamed or deleted.
- Adding to playlists in the native app: "Add to playlist" on an item or a
  tile's actions opens a side panel to add it to one of your playlists or
  name a new one; New playlist makes an empty one for movies and TV or
  music, and a library's "Save as…" saves its filters as a smart playlist.
- Item pages in the native app end with Details: who made it and the
  facts, the versions (OK chooses which plays) and Media info, which OK
  opens: each file and every stream in full.
- Rows with more than they show end with See all, which opens the library
  sorted and filtered as the row is (Recommended's collections: the
  Collections tab).
- Playing in the native app: a session on the server with the TV's
  capabilities, as the web Samsung app reports them, on the TV's own
  player; Left and Right seek 10 s, presses adding up and a held key
  scrubbing; the title, clock, scrubber and Play/Pause over a scrim; where
  it stopped goes to the server.
- Search in the native app, from the rail: OK on the field opens the TV's
  keyboard, results come as you type, in rows (People, Movies, Shows,
  Seasons, Episodes, Artists, Albums, Tracks), and the last 10 searches
  show as chips before you type.
- People's pages in the native app, from Cast on an item's page or from
  search: the photo, the name and what of theirs is in your libraries,
  then a row per role (Acting, Directing…) with the year and the part
  under each title.
- Collections in the native app: the rail's Collections lists every movie
  and show library's collections, a row each; a collection's page has its
  backdrop, TMDB or Smart badge, what it holds, Play and Shuffle, and its
  titles in a grid that scrolls up over the backdrop.
- Watch history in the native app: the profile's plays, newest first, with
  when and on which device; OK opens one, the cross or OK held removes it,
  and Show more loads the next 15. It opens from Settings' Profile.
- Settings in the native app, from the rail, as on Android TV: Profile
  (switching profile, the audio and subtitle defaults, Watch history,
  sorting libraries, always starting as a profile, and the account on the
  web, with its address and QR code), Playback (quality, the next episode
  by itself, playback details), Audio (passing surround sound through per
  format, which the TV now tells the server when it plays), Subtitles and
  display, Server (switching server), the TV's facts, About with the
  licences, and Sign out, which asks first. Choices open in a side panel.
- Diagnostics in the native app, from Settings: the app's version and the
  TV's facts, then what it tells the server it plays (HDR, the decoders and
  their limits, the sound it decodes and sends to a receiver, the streams
  it reads), a line each, with Check again if the check fails.
- Live TV in the native app, in the rail after Home while the profile has
  channels: the categories under the search (Favourites, Recently watched,
  All channels, the provider's groups and regions), and the guide, which
  moves through time with Left and Right and pages with Channel up and
  down, or the channels as tiles; OK held, Menu or Info opens what's on
  with Watch and the favourite. It opens as it was left, at the channel
  last watched.
- Watching Live TV in the native app: the channel full screen on the TV's
  own player; Up, Down and Channel up and down change channel, number keys
  tune one; OK shows what's on with the channel list and the favourite,
  Info shows what's on for a moment, Menu or Guide opens the channel list,
  where another group or a search can be chosen. A channel that doesn't
  start says why, with Try again or, every connection being in use, Watch
  here instead. The channel lets go of its connection while the app is
  hidden and tunes again when it comes back.
- The native app's player plays as Android TV's does: it chooses how the
  file plays with the TV's capabilities, recovers by itself from a stall
  or a format the TV refuses, waits for a server out of reach
  ("Reconnecting…") and plays on where it was, and says why it can't play,
  with Try again and Back. It pauses when the app goes to the background.
- Subtitles in the native app's player: text subtitles over the video in
  Settings' size, background, font and position, gliding up above the
  controls while they show; the remote's CAPTIONS key turns them off and
  back on. Skip Intro and Skip Credits, Up Next with its countdown (the
  next episode, or the next video of a playlist or collection), seek
  previews while scrubbing, chapters on the scrubber with Previous and
  Next, how it plays beside the clock, and messages at the top right. A
  playback the server stops says why and leaves after 20 seconds.
- The native app's player has Android TV's control row and side panels:
  Subtitles (the track, Sync, and the subtitles' size, background, font
  and position), Audio, Quality (with Play original after it switched by
  itself), Sleep timer, Speed where the TV plays other speeds, and Info,
  the playback's details, read again each second; Chapters opens the
  chapters' cards over the scrubber. The sleep timer says "Pausing in
  30 s" with Keep watching before it pauses.
- Music's pages in the native app, from a music library, Home or search:
  an artist's page with their picture, Play and Shuffle for every track of
  theirs and their albums, newest first; an album's page with its art, the
  artist (which opens their page), its rating and its tracks by disc, where
  OK plays the album from a track or pauses the one playing. Music plays on
  behind the other pages, and pauses for a film or a channel.
- Now Playing in the native app, in the rail while music plays: the album
  art, the track, the scrubber (Left and Right seek 10 s), Shuffle,
  Previous, Play/Pause, Next and the sleep timer, and the queue, where OK
  plays a track. It dims after two minutes without a key. The remote's
  media keys play, pause and skip the music from any page; it pauses while
  the app is hidden and plays on when it comes back.
- Picture subtitles (Blu-ray PGS, DVD VobSub and DVB) in the native app's
  player, drawn by the app as the web app draws them: a film plays on as it
  is when one is chosen, and Sync moves them like text. A server too old to
  send them has them burned into the video instead.
- In the native app's player, OK with the controls hidden shows them on
  the scrubber, as Up does, rather than pausing; OK on the scrubber plays
  and pauses. Seeking with Left and Right waits for OK, as YouTube's TV app
  does: the preview shows where it goes, OK goes there, Back lets it go,
  and it goes by itself 5 s after the last key; a held key runs faster the
  longer it's held. While a seek waits, Up opens the film's chapters as a
  row of cards in the preview's place and size (each one's picture, name
  and start), the video playing on: Left and Right go through them,
  moving the scrubber to each one's start, and OK goes there. Chapters named only for where they start
  ("00:06:19.212") are numbered instead. The scrubber is blue up to its
  thumb while seeking. Skip Intro and Skip Credits show over the scrubber
  as on Android TV, where Up reaches them, and give way to a seek's preview
  and the chapters. As each item begins playing (the next episode by
  itself too), its title shows at the top for a few seconds.
- Back on the native app's first screen asks "Exit Pley?" (Cancel, Exit) as
  the web Samsung app does, rather than leaving at once.
- The native app's Home no longer pauses to draw its rows again when the
  server's answer is the same as what it shows.
- The native app's rows (Home, libraries, item pages) show sooner: the
  tiles past the screen's edge are made just after, and the app no longer
  pauses to save what is already saved.
- The native app's tiles draw their badges as the Android app's do: a
  quality badge that doesn't fit the tile is left out rather than running
  past its edge, the unwatched count sits on the accent and the watched
  tick is an accent check, both with square corners.
- A library's posters in the native app show 4K and the dynamic range (DV,
  HDR10) when it is sorted by bitrate or filtered by resolution or dynamic
  range, as the other apps do, and otherwise no quality badges: no codec
  and no lower resolution.
- Library pages in the native app filter movies and shows by resolution
  (4K, 1080p, 720p, SD) and dynamic range (Any HDR, HDR including Dolby
  Vision and HDR+, Dolby Vision, HDR+, HLG, SDR), where the server offers more than one of them: in the
  Filter panel after Year, with a chip saying the one chosen. Clear
  filters and Save as… take them in, and the library opens with them as it
  was left.
- Live TV in the native app starts a channel in about 2 seconds, from its
  stream as the provider sends it, where it waited for the server's first
  segments before; channels with surround sound the TV decodes (AC3) play
  that way too. A channel the TV can't play this way falls back to the
  server's HLS by itself.
- Changing channel in the native app no longer flashes the TV's menu; a
  channel buffers as on Android and shows its picture as soon as it moves.
- In the native app's live player, the controls go once a channel is
  picked from the list they opened, and Back no longer leaves the channel
  while they seem to show.
- OK on the native app's live player opens the channel list at once;
  holding OK shows the controls (the list, the favourite), as OK did.
- The native app's seek previews and chapter cards show a widescreen
  film's frames letterboxed, as the film plays, rather than stretched to
  fill their box.
- In the native app's libraries, focus stays on the chip after a sort or
  filter is chosen, where it was lost and no key moved it; anywhere a page
  draws a row again under focus, the next key brings focus back to it.
- The native app's cast and crew circles no longer sit on dark squares.
- The native app no longer stops answering, or closes, after a film's
  chapters were looked through or its picture subtitles were on: it holds
  only the chapter pictures near the one shown, lets them go as the player
  goes, and reads a film's subtitle pictures once rather than every few
  seconds while the server makes them.
- Picture subtitles chosen in the native app wait up to a minute, as
  Android's do, for a server that hasn't made them yet, rather than saying
  after 10 s that they couldn't be loaded.
- Films start about half a second sooner in the native app: the player
  starts once it has as much as Android's does, and asks for the film
  while its screen is being made.
- Back in the native app's Live TV guide goes to the categories, then the
  rail, rather than straight to the rail.
- The native app's "Exit Pley?" is a narrower dialog.
- The native player's Info says "TV network speed", how fast the video
  reaches the TV, beside the server's Speed: a video that stalls with the
  server keeping up has a network that can't carry it.
- The native app's search finds Live TV channels, as Android TV's does: by
  name, number or what's on, in a Live TV row after the titles and before
  People, each tile with the channel's logo, number and what's on. OK
  plays the channel, changing channel among every channel.
- Signing in to the native app with a code says when the server requires
  two-step sign-in: the phone that approves the code sets it up for
  whoever hasn't.
- The native app's rail opens and closes as Android TV's does: it widens
  with its labels fading in, the page behind it dims, and its entries grow
  and shrink as focus moves; with Reduce motion, it opens in place.
- Coming back from a film in the native app, posters already shown stay
  as they were rather than flashing to their titles and fading in again.
- Holding Up or Down in the native app's libraries slides the posters
  under a highlight that stays still, as rows do for Left and Right.
- The native app's seek preview keeps its picture inside its rounded
  edge, where the picture's square corners showed past it and its sides
  covered the edge, so it seemed wider than its box.
- A channel's number and favourite star in the native app's Live TV
  cards stay on top of its logo, which covered them once it loaded.
- Posters in the native app no longer stay empty, until the app starts
  again, after a library's sort or filter changes while they load.
- In the native app's player, the seek preview and the chapters' cards
  show their pictures as soon as the server has made them, where a card
  asked before then stayed an empty box; a chapter whose own picture the
  server lacks shows the preview at its start.
- What the native app stops stays stopped: Up Next no longer moves on
  after it was stopped, a sign-in code no longer changes while it is
  read, and the subtitles and the reports to the server no longer do
  more work after each seek or pause.
- Signing out of the native app goes back to signing in each time, also
  after signing in again to the same server.
- The native app follows the server's events while it shows, as the other
  apps do: its pages change with what changes on the server and on other
  devices, and Home and the libraries show their scans. Back from a film,
  its page has the new resume point and watched mark.
- Held keys and a held OK keep working in the native app on a TV that has
  been on for more than 106 days.
- Choosing a profile in the native app no longer leaves Who's watching?
  stuck when the server fails to send the profile's libraries after the
  switch: the app goes on as that profile, and the pages offer Retry.
- Exiting the native app while music plays ends the music's session on
  the server, with where it stopped.
- The native app's Info no longer closes the app during a film: it reads
  the TV player's stream details once, and again when the picture changes,
  rather than every second; it opens with them, where it said "Asking the
  server…" for a moment first.
- Switching to a Japanese (or other) audio track in the native app works
  where the file names its language in three letters and the TV in two
  ("jpn", "ja"); the player no longer starts again at each try.
- The native app's animations (a tile's focus, a film's title as it
  begins, pictures fading in) are held while they run, where one could be
  collected mid-way and close the app.
- The native app's black bars meet a letterboxed film's picture exactly,
  whatever its shape, where a sliver of the TV's menu could show between
  them.
- Picture subtitles (PGS, VobSub, DVB) in the native app no longer close
  the app as a track still being made comes again: the same picture is
  kept, and an old one's texture goes a second after it leaves the screen.
- Videos on a USB drive play in the native app: the rail's USB drive,
  while one is plugged in, lists its folders and videos; a video plays in
  Pley's player from where it stopped, with its audio tracks, chapters,
  HDR, its own subtitles and the subtitle files beside it (UTF-8, UTF-16 or
  UTF-32, or Windows' Western code page, as in the Android app; 2 MB at
  most).

### Changed

- The native app's icon, and its debug build's, are square, as the TV's
  menu draws the other apps, where they were wider than the rest.
- Picture subtitles (PGS, VobSub, DVB) show within a few seconds of a
  playback starting or a seek, where they could take up to a minute: the
  TV tells the server where it plays, and the server reads those around it
  first.
- Search shows movies, shows and episodes first, then music, then
  people, the closest matches first in each.
- A Live TV channel whose connection someone else takes over stops at
  once and says why, under "Live TV stopped", with Watch again, where it
  said so only once its picture had stopped, after a video's "Playback was
  stopped by the server's administrator." "Live TV stopped" stays when the
  app is hidden and shown again, until Watch again.
- The native app's rail has smaller entries and lists up to six
  libraries, where four or more went behind Libraries; nothing leaves the
  rail to make room for Live TV, Now Playing or a USB drive any more, and
  the entries scroll between the avatar and Settings when they don't all
  fit, keeping the focused entry fourth from the top, as the app's lists
  keep theirs.
- The native app's search rows hold every result, up to 200 titles, 200
  people and 200 Live TV channels, where they stopped at 60 titles and 12
  people and channels. A row makes its tiles as focus comes near them, so
  a long one shows as quickly as a short one.

### Fixed

- A film the TV can't play falls back one step at a time, to a conversion
  of its audio before one of the whole film: the TV's player says a failure
  twice, and the second skipped a step and started a second session.
- The rail's grey panel fades into the dark under its entries over the
  last of its slide closed, where it went at once at the end.
- Blu-ray subtitles that show two pictures at once, the first one forced,
  show both, and a picture the disc crops shows cropped.
- On an item page in the native app, Details moves as on Android TV: a
  version or Media info moves it up only as far as keeps that row on the
  screen, nothing of it shows under the title and summary, Down reads on
  through open Media info a step at a time to its end, where a file's
  streams taller than the screen were cut off, and it comes back down
  under More Like This when focus goes up, where it had stayed over the
  row's titles.
- An item page's "Details" and "Versions" headings show in the native app,
  where the rail covered them.
- An empty USB drive's page keeps focus, so the rail closes rather than
  covering its message, which shows under a folder icon; at the drive's top,
  the line under the title is the folder's path ("/Films/2019"), and "/"
  at the drive's top beside how full it is. The page's title is the drive's
  make and model ("Kingston DataTraveler 3.0").
- The rail marks the page a rail entry opened, not Home, when the page has
  nothing to focus yet, as the first page opened after Who's watching?.
- The native app's rail keeps focus on its entry when its entries change
  (a library scan, Live TV's channels, Now Playing, a USB drive), or puts
  it on the entry now in its place, where nothing showed focus and Down
  went to the avatar.
- Diagnostics, Remove from My List and Settings' audio and subtitle rows
  show their icons.
- The native app carries none of the profiling build's code: its marks, the
  listener, screenshots and remote keys were compiled into it, unused.
- A film's audio track, chosen or saved, plays from the start: it was asked
  for before the TV's player was ready, and the first track played. Beside a
  TrueHD track the TV leaves out, the right track is chosen, and Info shows
  the track playing.
- Live TV no longer pauses for a quarter of a second as it opens and as a
  channel is left, with a provider's hundreds of categories, and moving
  through the guide is smoother: a row's programmes are painted again
  rather than made again as focus moves, and a long description is cut to
  what the panel shows.
- Settings' Always start as no longer starts the TV as a profile with a
  PIN without it: the start opens on that profile's PIN pad, and Back shows
  the other profiles. While the TV doesn't know whether the profile it was
  left on has a PIN, the start asks "Who's watching?".
- The app no longer closes now and then as a screen is left (a disc's
  titles, the Live TV guide, a film): a view tidied up after it had gone
  stopped an animation that had gone with it.
- Too many wrong PINs for the profile the TV starts as say how long to
  wait, as a switch does, rather than "a few minutes".
- The player's Subtitles panel lists the subtitles' size, background, font
  and position only while text subtitles show, where they change
  something: picture subtitles keep their own look, and subtitles off have
  nothing to style.

### Security

- A crafted file's picture subtitles (PGS, DVB, VobSub) can no longer keep
  the app busy: decoding one moment of them stops after a set amount of
  work, far more than real subtitles take, and that moment isn't shown.
- A crafted file's picture subtitles can no longer run the app out of
  memory, as in the Samsung web app: a track is read up to 100,000
  samples and 512 MiB, a moment's pictures are a 4K screen's pixels at
  most (2 pictures for PGS, 32 for DVB), and the frames kept decoded hold
  128 MiB besides the one showing. Real subtitles show as before.
- Pictures leaving the screen, whose copies and textures go a second
  later, hold 128 MiB at most: past that, a crafted file's subtitles
  changing every moment wait for some to go before the next show.
- Before the token goes to the server's address, the server proves it is
  the one the TV signed in to, at that address, as in the Samsung web app
  and on Android: the access token waits for a proof made on this network
  within the half hour, the refresh token and a profile's PIN for one made
  then. Another device at the address on another network gets nothing, nor
  does one passing the challenge on to the server from elsewhere; the TV
  says "Another device answers there." One that takes the address on the
  TV's own network may get an access token (an hour's) within the half
  hour, never the refresh token. A sign-in from before starts checking
  within the hour, as its token is renewed.

# Changelog

Notable changes to Pley for Samsung TV, native (Tizen .NET), newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
It has no releases yet: it is built and installed by hand
([docs/TV_PLATFORMS.md](../../docs/TV_PLATFORMS.md#the-native-samsung-app)).

## [Unreleased]

### Added

- A native app for Samsung TVs with Tizen 9, beside the web app: so far the
  Android TV app's "Choose your server" screen, with servers found on the
  network, an address pad, the TV's keyboard and forgetting a server; its
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
  scrubbing; the title, clock, scrubber and Back 10 · Play/Pause ·
  Forward 10 over a scrim; where it stopped goes to the server.
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
- Films start about half a second sooner in the native app: the player
  starts once it has as much as Android's does, and asks for the film
  while its screen is being made.
- Back in the native app's Live TV guide goes to the categories, then the
  rail, rather than straight to the rail.
- The native app's "Exit Pley?" is a narrower dialog.
- Coming back from a film in the native app, posters already shown stay
  as they were rather than flashing to their titles and fading in again.
- Holding Up or Down in the native app's libraries slides the posters
  under a highlight that stays still, as rows do for Left and Right.
- The native app's seek preview keeps its picture inside its rounded
  edge, where the picture's square corners showed past it.
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

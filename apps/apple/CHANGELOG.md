# Changelog

Notable changes to Pley for iPhone, iPad, Mac and Apple TV, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Finds Pley servers on your network, keeps the ones you opened, and adds
  one by its address.
- Sign in with your browser on iPhone, iPad and Mac, or with a code and its
  QR code on Apple TV, iPhone and iPad. Your sign-in stays in the Keychain
  on this device only.
- Choose who's watching from the household's profiles, with the profile's
  PIN when it has one.
- A tab bar on iPhone, a sidebar on iPad and Mac, and tabs across the top on
  Apple TV; your profile, servers and sign-out are under Me.
- Home shows your rows with their posters, moved through with the remote
  on Apple TV.
- Browse your libraries: sort, filter, jump by letter, and put libraries
  in your own order. Home has See all, and long press or right-click for a
  title's actions. Screens update when something changes on the server.
- Full title pages: seasons and episodes, cast and crew, extras and
  trailers, More Like This, media info, your rating, watched marks and My
  List; people's pages; artist and album pages.
- Collections and playlists: play or shuffle them, add titles to playlists,
  make, rename, reorder and delete playlists, and save a library's filters
  as a smart playlist.
- Search with recent searches; Settings for quality, the next episode,
  subtitle style and grid size; History, Diagnostics and About. On Apple
  TV, choose who's watching at start (a profile with a PIN asks for it,
  even the one used last) or always start as one profile.
- On the Mac: File and View menus with shortcuts, Settings (⌘,), About
  Pley, hover and right-click. On Apple TV, a row puts the focus back on
  the tile you left it on.
- Sign out of a server, or forget it.
- Your token goes only to the server you signed in to: before it goes, the
  server proves it holds your sign-in's key, again after half an hour, on
  another network, or when the server didn't answer. When another device
  answers at its address, Pley shows what it saved and says "Another device
  answers there."
- Play movies and episodes: Play or Resume on an item's page, the system
  player on each device (its own window on the Mac, with keyboard
  shortcuts), and each file sent as this device plays it best, in HDR and
  Dolby Vision where the screen shows them.
- A Quality menu with Original, Automatic and each quality; Automatic
  lowers the quality when the video doesn't play smoothly and goes back up
  when it can.
- Resume where you left off, Skip Intro and Skip Credits, Up Next for the
  next episode, chapters, and an audio track choice the server remembers
  for the whole show.
- Subtitles, text and picture (PGS, VobSub, DVB) alike, drawn over the
  video in your caption style, with a sync control; picture subtitles
  never force a conversion. Find Subtitles… in the player searches online
  through your server and adds the subtitles you choose, which show at
  once.
- Playback speed from 0.5× to 2×, and a sleep timer for videos with Keep
  watching in its last 30 seconds.
- Picture in Picture on iPhone, iPad, Apple TV and Mac: from the player's
  button, and by itself when you leave the app on iPhone and iPad; its ✕
  ends the playback.
- Live TV on iPhone, iPad, Mac and Apple TV: the categories, the guide and
  the channel cards, favourites and recently watched, a programme's
  details, and the live player with channel up and down, a channel number
  typed or on Apple TV's number pad, AirPlay, and Watch here instead when
  every connection is in use. Search shows the channels it finds.
- Music: Play and Shuffle on albums, artists and music playlists, a track
  plays its album from there, and it plays on in the background and with
  the screen locked. A mini player and Now Playing (a window on the Mac,
  a tab on Apple TV) with Up next, Shuffle, a sleep timer and AirPlay; the
  lock screen, Control Center, headphones and the Mac's media keys play,
  pause and skip music and videos.
- Download movies, episodes and whole seasons on iPhone, iPad and Mac to
  watch without your server: as they are, or smaller (Settings →
  Downloads), in the background, on Wi-Fi unless you allow mobile data.
  They're in Me → Downloads, and play with their subtitles, Resume and Up
  Next while the server is away; what you watched reaches the server once
  it is back.
- When the server doesn't answer, the app opens on what it saved of your
  screens and says so, and reconnects by itself (Apple TV too).
- Continue Watching on Apple TV's Top Shelf: with Pley in the top row,
  your titles in progress show with their pictures and progress, and one
  opens its page or plays.

### Fixed

- A video that loses the server plays what it has loaded and keeps trying
  to reach the server for 90 seconds, then plays on where it stood,
  instead of stopping at once.
- Apple TV's tabs stay on screen: seven at most, with a Libraries tab for
  the libraries that don't fit.
- The iPhone's Quality menu keeps each choice's second line when it is
  checked.
- VoiceOver no longer reads the page behind the player.
- The Mac's player menu no longer flickers while playing.
- Larger Tiles takes ⌘= on the Mac, as well as ⌘+.
- On the Mac, an HDR video starts again where it is when its window moves
  to a screen that shows HDR differently, so it plays as that screen shows it.
- A playlist entry moved while a removal can still be undone keeps its
  place, and the removed one comes back where it was.

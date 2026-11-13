# Changelog

Notable changes to Pley Media Server: the server, its transcoder and the web app it
embeds, with the Docker image and the Windows installer, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `release-<date>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Added

- Activity monitors such as Tracearr can connect through the
  Jellyfin-compatible API with a token that sees everyone's activity, and
  show what everyone is playing.
- Setup asks for the server's name, languages, country, TMDB key and
  whether Apple devices find it by Bonjour, then whether to bring over a
  Plex server or start fresh with Movies, TV Shows and Music libraries.
- Moving from Plex: setup and Settings → Libraries can import a Plex
  server's libraries, its Home users as profiles with their restrictions
  and the PINs typed for them, and each person's watch history, ratings,
  labels, collections, playlists and matches (docs/MOVING_FROM_PLEX.md).
- Home and Settings → Libraries show a Plex import step by step while it
  runs, and its report once it ends.
- Home shows administrators how far new libraries' first scans are.
- Settings → Server has a Country, for age ratings, age limits and Coming
  to cinemas.
- Settings → Network says when Pley is on Docker's own network, where
  devices can't find it, and how to fix that.
- In a container with no media folder mounted, adding a library says how
  to mount one.

### Changed

- Settings → Server calls its languages Pley's language and Titles and
  summaries, and names them instead of showing codes.
- Seek previews, intro detection and fingerprinting wait until first
  scans and Plex imports are done.
- A video that has to be remuxed or converted starts sooner while a
  library is being scanned.
- Home and a library's page show a new library's titles as its files are
  read.

### Fixed

- Infuse opens a Pley server through the Jellyfin-compatible API instead
  of saying "An error occurred" after signing in.

## [0.1.10] - 2026-11-12

### Added

- Settings → Transcoder can turn video transcoding off. Videos then play
  as they are or with only their sound transcoded, and one a device can't
  play says why. The Plex- and Jellyfin-compatible APIs follow it too.
- "Max simultaneous video transcodes" limits how many streams transcode
  video at once, within the total limit.

### Changed

- The note under "Transcode in memory" is shorter.
- Dropdowns' carets sit in from the edge, and a dropdown that can't be
  changed is greyed out.
- Number fields in Settings and a title's Edit are only as wide as their
  numbers need.

### Fixed

- A movie titled with a number takes its year from its file name, like
  other movies. A full rescan merges any that showed twice.
- Sync tools such as WatchState copy resume points to Pley through the
  Jellyfin-compatible API.

## [0.1.9] - 2026-11-08

### Added

- In Settings → Users and Invitations, the age limit is set "By rating"
  (one of 29 countries' ratings, the metadata region's first) or "By
  age", each showing only its own fields, and lists show it that way
  ("Up to PG-13"). A limit the old list didn't offer shows as its age, no
  longer as "No limit". `GET /api/ratings/systems` gives the ratings, and
  users and invitations keep the system in `max_age_system`.
- An account's age limit can follow its age: "By birthday" asks for the
  month and year of a birthday, and the limit goes up on the first of that
  month each year, until it ends at 21. Only administrators see the
  birthday, never the account itself or its apps.
- Exceptions by label: in Settings → Users and Invitations, a profile
  can always see titles with some labels, even above its age limit, and
  never see titles with others, whatever their rating (a label on a show
  or album counts for its episodes and tracks). Discover leaves the hidden
  ones out, and they can't be asked for. Labels are set in a title's Edit
  details, and only administrators see them; `GET /api/labels` lists them.
- Setting an age limit without a TMDB key says that unrated movies are
  hidden, with a link to add a key.
- The web and desktop apps' home screen opens on a banner: the newest
  films and shows you haven't watched, full width under the top bar, each
  with its plot, Play, More info and My List, taking turns every few
  seconds (holding still under the pointer, and for those who ask their
  system for less motion). Swipe between them on a phone.

### Changed

- The Docker image comes with FFmpeg 9, as the Windows installer does,
  in place of Debian's 7.1.
- Age limits read a rating with its country's ratings, so that one of the
  same name in another country isn't misread: "A" is for all ages in
  Denmark, Spain and Mexico and for adults in India, and the UK's PG is
  8. Ratings from TMDB, TheTVDB and NFO files keep their country; NFO
  files with several countries' give the metadata region's. Changing the
  region in Settings works every age out again.
- Edit details says which country's ratings a title's rating is read
  with: the country written before it ("GB:15"), the one it came with, or
  the metadata region's, or where the region has none here, the US's and
  others in common use.
- A user's settings are in three tabs, Profile, Access and Devices, and
  label exceptions fold into one line, such as "Never show 2 labels",
  until they're opened.
- Labels are picked in one field of chips: the arrow keys move through the
  labels titles carry, Enter or a click adds one, and a label no title has
  yet can still be added.
- Plex and Jellyfin apps no longer show titles' labels, which label
  exceptions go by, except to tokens that may manage libraries (Kometa's).
  Their other apps filter by label as if titles had none, so a label
  guessed tells them nothing; smart collections made by a label still
  find their titles.

### Fixed

- An NFO file's rating with a slash in it, such as India's "U/A 13+" or
  Portugal's "M/18", is read whole, no longer as "U" or "M", which allowed
  younger viewers. A file that gives a bare `<mpaa>` and a
  `<certification>` naming countries gives the latter's.
- A Live TV channel given a guide channel by hand, or a channel without
  one renamed as the guide names it, shows its programmes once the
  source's guide is read again for it, no longer after the next guide
  refresh.
- Refreshing a Live TV source, or changing it, while it is being read
  reads it again after that read, no longer nothing.
- HDR video that libplacebo tone maps plays again in the Docker image and
  with other FFmpeg 7 installs: the transcoder no longer asks it for a
  shader cache that FFmpeg before 8.0 doesn't have.
- A Live TV channel whose playlist id isn't in the guide shows the
  programmes of the guide channel found for it, rather than "No guide".
  That guide channel is now also found when the guide writes the same id
  in another case or with other dots and dashes (`Kika.de` and `KIKA.de`).
- A quality fits a film wider than 16:9 into its box: 1080p of a 2.40:1
  film is 1920×800, no longer 2592×1080, so it isn't wider than the
  screen and plays on players that decode H.264 up to level 4.1.
- Live TV channels served as HLS with their sound in an audio track of
  its own, as many broadcasters serve theirs, play with sound: the
  default track is put into the picture's stream.

## [0.1.8] - 2026-11-06

### Added

- Pley is in English, French and Spanish: each device's language by
  default (the sign-in page has a globe to pick another), or the one an
  account picks on its Account page, which follows it everywhere. The
  server language, in Settings → General, is for the server's own words
  (Home's rows, the tray's menu) and for devices in a language Pley
  doesn't have.
- Jellyfin apps that show seek-bar previews, such as Findroid and
  Streamyfin, show Pley's as you scrub: the same previews Pley's own players
  use. Jellyfin for Android TV doesn't show them.
- The web and desktop apps' remote offers Skip Intro and Skip Credits while
  what a device plays is in an intro or the credits, as its player would;
  `GET /api/sessions` gives each session's intro and credits. Its back
  button goes 30 seconds, as far as forward.
- Your remote lists the TVs and other devices of everyone in your
  household, not only those on your profile, each with the profile it is
  on. Sending one something switches it to your profile first, so it plays
  as you, after your PIN when your profile has one.
- A device that gives itself a TV's name and id on the TV's profile is
  listed beside the TV ("Living room TV (2)"), never in its place: the TV
  is listed again the moment it is back from being off or reconnecting,
  and what you send the TV reaches the TV alone.
- The web app's Play on… lists your household's devices too, and asks for
  your PIN in its menu's place when your profile has one. The desktop app
  switches to the profile of the remote that sends it something, as a TV
  does, and says so when it can't. An administrator's remote sends only to
  Pley's apps, never to a browser or the desktop app, which would stay
  signed in with the administrator's rights.
- Jellyfin apps make, rename, fill, reorder and delete your playlists, and
  with a token that may manage libraries, make and change collections. Each
  playlist entry has an id of its own, so an app removes or moves the one
  you picked when a title is there twice.
- Pley reads the NFO files Kodi, Jellyfin, Emby, Radarr, Sonarr,
  tinyMediaManager and MediaElch keep next to movies, shows, seasons and
  episodes. Their ids decide what a title is matched to, and their titles,
  summaries, dates, genres, content ratings and other fields win over
  TMDB's, TheTVDB's and TVmaze's; your own edits still win over them, and
  Fix Match to another title sets them aside. Without a TMDB key, movies
  with NFO files get their metadata from them. Jellyfin's lockdata keeps an
  item from being fetched online. Settings → Server has a switch for it, on
  by default.
- Scripts and tools edit metadata through the Jellyfin-compatible API with
  a token that may manage libraries: titles' fields, locks, pictures and
  Fix Match, as Pley's own editor does them.
- In the web and desktop apps, an item's Edit details says which fields
  come from its NFO file, and Fix match says when a title was matched by
  hand, with Undo Fix Match to let its NFO file decide again.

### Changed

- The web and desktop apps' remote shows a command at once (the place a
  seek goes, playing or paused), and keeps what plays while a next starts
  the one after. Previous and Next sit at its edges, with the skips and
  Play or Pause centred between them. A device that plays is chosen for
  its controls, whose bar is the only one shown; one ready to play is only
  listed.
- Signing in on the web and in the desktop app goes straight in as the
  profile the browser was last used as, when it has no PIN, where a
  household of several profiles was asked "Who's watching?" each time. A
  profile with a PIN, or another person signing in, still picks first.
- The web and desktop apps' video player no longer brings up its controls
  when a remote pauses it, seeks or changes its audio, subtitles or
  quality: a brief notice says what changed ("Paused", "+30 s", "Jumped
  to 12:34", "Subtitles off"), and the controls show as before once you
  move the mouse, tap or press a key. On Live TV a remote's change of
  channel shows only the channel's tuning screen.
- With TMDB as the TV source, a show with an IMDb or TheTVDB id but no
  TMDB id is found by that id before it is searched for by name.
- The web and desktop apps' menus open above their button where the
  window has no room for them below, as in a short window.
- Jellyfin apps' streams go as far as Pley's own apps' do, by what their
  device profile says: HEVC copied, or made in HEVC to stay HDR, in fMP4
  segments where the app asks for them, VP9 and AV1 copied into fMP4, HDR
  and Dolby Vision kept where the app shows them, and 5.1 sound (E-AC-3 or
  AC-3) where it takes it. They were H.264 or copied HEVC, in SDR, with
  stereo AAC.
- Why a request couldn't be sent to Radarr or Sonarr ("Radarr isn't
  answering", "Radarr refused: …") is in the language each administrator
  sees Pley in, on Requests and in the notice that it failed, as is "Added
  by hand". What Radarr and Sonarr say themselves stays as they said it.
  A send that can't reach them says it isn't answering, as approving
  does, where it gave the network's error.

## [0.1.7] - 2026-11-05

### Added

- Pley for Home Assistant can sign in with a code, as Pley's TV apps do.
  It shows as Home Assistant in your signed-in apps and on a TV it plays
  something on.
- A new permission, activity, lets an app see what's playing and stop it.
  On an administrator's account, Pley's own apps allowed it see what
  everyone plays and stop it, as the dashboard does; other apps, and
  everyone else's, see and stop only the profile's own.
- In the desktop app the web app takes casts, as a TV does: your other
  devices' Play on… lists it by its system ("Pley for Windows"), and what
  they send it plays there with the tracks picked, saying where it came
  from. A browser tab is still a remote only.
- The server lets a user's devices control what plays on their other
  devices: it lists the user's own sessions with the device each plays
  on, Live TV channels included, and passes play, pause, seek, skip, next
  and previous to players that say they obey them (on Live TV, next and
  previous change channel). Only the session's own user controls it;
  administrators can still only stop other people's, a Live TV channel
  now too. `/api/activity`'s sessions say which device they play on.
- Jellyfin apps that say they take media control obey a user's remotes
  through their socket: pause, play, seek, skip, next and previous. Plex
  apps can still only be stopped.
- The web and desktop apps have a remote in the header while something
  plays on another of your devices: it shows what plays there and how far
  along, and plays, pauses, seeks, skips, goes to the next or previous
  title or channel and stops it. Their players obey the remote too, and
  the dashboard says which device each session plays on.
- Every session says which device plays it, the same for everything one
  sign-in plays, and what kind of device that is (a TV, a phone, a
  tablet, a computer, a browser): what the player says, else what the
  server tells from the app. Remotes and dashboards can key players by
  it, and pick TVs out.
- A user's remotes can cast to their other devices: an app that keeps its
  event stream open as a receiver is listed, and plays the title,
  playlist or Live TV channel sent to it, as its own Play would, and says
  which device sent it, by the name that device gave itself. Only the
  user's own devices are listed and cast to, an administrator's too.
- Remotes change the audio track and the subtitles of what plays on
  another device: sessions list their file's tracks and which play, and
  players that say they take a change of track switch to the one picked.
  Jellyfin apps that take media control switch track too.
- Remotes change the quality of what plays on another device, from the
  qualities its Quality menu offers for the file, and show which it plays.
  A cast plays with the audio track and subtitles picked on the remote's
  page, and a Live TV channel on a remote shows how far along the
  programme is and what's on next.
- The web and desktop apps' remote has Audio, Subtitles and a new Quality
  as icons beside Stop, and asks before it stops what plays. On a Live TV
  channel it shows the programme's progress and what's next, with
  channel buttons. Play on… from a title's page plays with the audio and
  subtitles picked there, and the video player takes a remote's change of
  quality as its own menu does.
- The web and desktop apps' remote lists your other devices that are on,
  those playing first, then those ready to play, and changes the audio
  track and the subtitles of what plays on one. Play on… beside Play
  (Shuffle on… beside Shuffle, Watch on… for a Live TV channel) plays a
  title, music, a playlist or a channel on one of them, and opens the
  remote on it. Their video player takes a remote's change of audio and
  subtitles as its own menu does, and tells remotes which it plays.

### Changed

- Requests on the web: a sign-in from before the app could ask says "Sign in again to ask
  for movies and shows" without a second line, and its button reads Sign in.
- The setup page says more plainly where the setup code is: opening Pley
  from its tray icon fills it in, and on a server without one it's in
  Pley's log.
- When the server can't be reached, the web app and the desktop app say
  "Pley is offline", where they said "Can't reach the server.
  Reconnecting…". The desktop app's Downloads stays beside it.
- In the web and desktop apps, a movie or show in Discover or Search's
  Discover results that a library has whole opens its library page, where
  it plays, not a page with only Open on it. A show partly there still
  opens its Discover page, to ask for more seasons.

### Fixed

- Someone who isn't an administrator no longer hears when other people's
  playback starts or stops: the event for it goes only to that person's
  own devices and to administrators.
- Music paused in the web app stays on your other devices' remotes: it
  says it is paused every 30 seconds, where it used to drop off after a
  minute and a half.

## [0.1.6] - 2026-11-04

### Added

- Search finds titles, people and Live TV channels typed with a slip ("the
  matirx", "kaenu reeves"), with words run together or apart
  ("spiderman"), or without an apostrophe ("the captains log"); when
  nothing is found that way, titles and names alike enough to what was
  typed are.
- Live TV search finds a channel by the words of its name in any order
  ("f1 sports"), and with a quality its name doesn't have ("news hd"), and
  what's on by its title's words, punctuation and accents aside.

### Fixed

- The web app's search no longer keeps the Live TV channels of an earlier
  query on the page, under results that don't match them.
- A search for a title with its year ("alien 1979") finds that year's
  title instead of nothing.

## [0.1.5] - 2026-11-03

### Added

- Pley's apps for iPhone and iPad, Apple TV and Mac can sign in, as
  "Pley for iPhone and iPad", "Pley for Apple TV" and "Pley for Mac". The
  Apple TV signs in with a code, as the other TVs do; an iPhone or iPad
  may too.
- Devices and watch history name Pley's Apple apps by device: Pley for
  iPhone, iPad, Mac or Apple TV.
- A player may say it runs on a desktop (`form: "desktop"`).
- The server can announce itself on the local network by Bonjour
  (`_pley._tcp`), with its id, name and version, so Apple's devices find it
  without its address. It is off until turned on in Settings → Network
  (`bonjour = true`; `--no-bonjour` keeps it off, as `--no-discovery`
  does), which also chooses the networks it is announced on, for a
  server that is also on a VPN, a container network or a guest Wi-Fi, and
  the host name its addresses go by (`pley-` and the server id's start
  unless another is chosen). It follows a rename, and goes out only from
  adapters on the local network, answering only the devices there.
  Windows Firewall's rule for Pley
  already lets it through (UDP 5353), and the Synology package offers
  DSM's firewall a **Pley (Bonjour)** rule for it.
- Apple's devices can stream HEVC, HDR and Dolby Vision as they are: a
  player that asks for fMP4 segments first gets its streams in fMP4 rather
  than MPEG-TS, with VP9 and AV1 copied too where it decodes them. Other
  players stream as before.
- Streams say when their video carries no closed captions, so Apple TV
  shows one Subtitles menu, not two. Files scanned before this say nothing
  until they are scanned again; captions in a TV recording's frames are
  found and still offered.
- A Jellyfin-compatible API, for Jellyfin's apps and others built on its
  API: they sign in with Pley's accounts, browse the libraries, play files
  as they are or as HLS from Pley's transcoder, with subtitles and Skip
  Intro, and keep watch state, favorites (My List) and ratings with Pley's.
  It is off until turned on in Settings → Network, on Pley's port or one of
  its own (Jellyfin's is 8096; `--jellyfin-bind`, and `compose.jellyfin.yaml`
  for Docker). Each sign-in gets a token of its own, which works on the
  compatibility APIs only, never on Pley's own, and never as an
  administrator; accounts with two-step sign-in can't sign in from these
  apps yet ([COMPATIBILITY.md](../../docs/COMPATIBILITY.md)). It shares
  Pley's port with the Plex-compatible API: a request that names itself
  Plex's goes to that one.
- Account has Tokens for other apps: one for each tool or player that uses
  the Plex- or Jellyfin-compatible API, made there and shown once to copy,
  or by a Jellyfin app signing in, with its device and last use, to revoke.
  A token does only what it was allowed: watching and, for an
  administrator's, seeing everyone's activity (for activity monitors such
  as Tautulli) or managing libraries (for library tools such as Kometa:
  refreshes, edits, collections). Neither makes the token an administrator
  or lets it manage accounts or the server's settings, and both go with the
  administrator's role, at once. A token can be limited
  to the addresses or networks it is used from, or the home network; from
  elsewhere it is refused, and so are the streams it started.
- Movie and show libraries can be filtered by resolution (4K, 1080p, 720p,
  SD) and by dynamic range (Any HDR, HDR including Dolby Vision and HDR+,
  Dolby Vision, HDR+, HLG, SDR),
  each title by its best file (a show by its best episode), and shown and
  sorted by the version that matches: a film filtered by Dolby Vision shows
  its Dolby Vision file's badges and bitrate. The choices are those the
  library has, and smart collections and playlists saved from a filtered
  library keep them. Kometa's movie collections by resolution work too.

### Changed

- Search finds as many people and Live TV channels as titles, up to 200 of
  each, where it stopped at 12 people and 12 channels. The web app shows
  two rows of each group of results and **Show all** for the rest.
- Library posters on the web carry their quality badges ("4K", "DV",
  "HDR") only when the library is sorted by bitrate or filtered by
  resolution or dynamic range: on every poster they mostly said the same
  thing.
- The web app's sidebar is more compact, so more libraries and playlists
  fit without scrolling; Settings' list of pages matches it.
- Updating the Windows service no longer takes minutes on a large library:
  Setup no longer sets the permissions of every file in the data
  folder again when the folder is set up already.
- The server's id is kept in `server-id` in the data folder. An older
  version's `plex-machine-id` is renamed to it on start, so apps keep
  recognising the server.
- Search puts the closest matches first: a title, person or Live TV
  channel named exactly what was typed, then those whose name begins with
  it, then the rest.
- The web app's search shows movies, shows and episodes first, then
  music, then Live TV, then people.
- The LG TV app's sign-ins are listed as "Pley for LG TV", with the TV's
  webOS and model, now that the app names itself.
- The Plex-compatible API no longer takes Pley's session tokens, which
  opened all of Pley's own API too, administration included: Tautulli,
  Kometa, players and scripts that use one get `401` until they have a token
  for other apps. Make one for each in Account → Tokens for other apps
  (Tautulli needs "See everyone's activity", Kometa that and "Manage
  libraries") and put it where the tool had the old one
  ([COMPATIBILITY.md](../../docs/COMPATIBILITY.md#moving-from-session-tokens)).
  `PUT /:/prefs` answers `403` where it accepted and ignored the change.
- The Plex-compatible API's stream addresses, which players fetch without a
  token, work only for streams started with a token for other apps, while
  that token works; a web-player's playback can't be opened through them.
- The Plex-compatible API's filters take the operators and groups Kometa
  writes (`title%3D=`, `year%3E=`, `push=1` … `or=1` … `pop=1`), and numbers
  and dates take at least (`>=`) and at most (`<=`). A filter Pley can't
  apply (on `resolution`, `hdr`, audio language, an episode's fields under
  a show, and the like) answers `400` where it was ignored, which let a
  tool's change reach the whole library; a smart collection or playlist
  isn't made of one.
- Pictures chosen by address, through the Plex-compatible API or the
  artwork picker, may come from fanart.tv, ThePosterDB and GitHub's raw
  files (where Kometa's own images are) as well as TMDB, TheTVDB, TVmaze and
  the Cover Art Archive, and are re-encoded as uploads are. Other addresses
  are still refused.
- Through the Plex-compatible API, collections keep labels and are
  found by title and label; their order (`collectionSort`) and an item's
  place in it can be changed; items and collections list their pictures;
  and a playlist's title can be changed. Edits Pley can't keep (critic and
  audience ratings, cast and crew, countries, editions, collection modes,
  logos, theme music) answer `400` where they answered `200` and kept
  nothing.

### Removed

- The web player's Back 10 seconds and Forward 30 seconds buttons. Left
  and Right (or J and L) still seek 10 s.

### Fixed

- When someone takes the connection of the Live TV channel you watch, your
  players stop at once and say why ("Live TV stopped"), with Watch again,
  in the web app and the apps, where they used to play on until the
  stream ran out. Who took it, and for which channel, is said only to
  their own devices and, when you are both in the home, to you; anyone
  else is told only that the connection was needed. Someone starting the
  channel you watch shares its connection and stops nothing.
- Dolby Vision files whose container doesn't say they are Dolby Vision
  (some remuxes and older muxers leave that out) are found by what their
  frames carry. They are labelled Dolby Vision P8.1, P7 FEL and so on
  rather than HDR10 or HDR10+, and the next scan finds those already in a
  library. Players that show Dolby Vision get them repackaged with Dolby
  Vision named, since played as they are they would show the HDR10 base
  layer.
- Blu-ray subtitles that show two pictures at once, the first one forced,
  show both in the web app, and a picture the disc crops shows cropped.
- A password or PIN checked while the server is out of memory gets "Pley
  is busy checking passwords: try again in a moment" rather than "wrong",
  and no longer counts towards locking the account or the PIN.
- A sign-in with an unknown username takes as long as one with a real
  username even after the server was once too busy to prepare for it, so
  its timing no longer tells which usernames exist. A new password or PIN
  made while the server is out of memory, and a PIN an app checks while
  it is busy, are told "busy, try again" rather than failing.
- A picture sent to a collection or playlist through the Plex-compatible
  API no longer lands on the movie or show that had the same number:
  collections and playlists have keys of their own there. An episode's
  poster sent there becomes its still, the picture apps show.
- An uploaded poster keeps Kometa's overlay mark, so Kometa doesn't take a
  poster it drew on for an original and draw on it again.
- Kometa connects to the Plex-compatible API with a token that may manage
  libraries: library sections say so (`allowSync`), and a library's
  settings answer, with none to change.
- Tautulli no longer takes Pley for a server that is down when nothing has
  happened for 15 seconds: the Plex-compatible API's notifications send
  pings, as Plex's server does.
- The web player offers Subtitle style only while text subtitles show,
  where it changes something: picture subtitles (PGS, VobSub, DVB) keep
  their own look, and subtitles off have nothing to style.

### Security

- A crafted file's picture subtitles (PGS, DVB, VobSub) can no longer keep
  the web app busy: decoding one moment of them stops after a set amount
  of work, far more than real subtitles take, and that moment isn't shown.
- A crafted file's picture subtitles can no longer run the web app out of
  memory: a track is read up to 100,000 samples and 512 MiB, a moment's
  pictures are a 4K screen's pixels at most (2 pictures for PGS, 32 for
  DVB), and the frames kept decoded hold 128 MiB besides the one showing,
  their pictures ready to draw as much again. Real subtitles show as
  before.

## [0.1.4] - 2026-11-02

### Added

- Apps can make sure an address still leads to the server they signed in
  to before sending it their token: each sign-in gets a key of its own,
  and the server proves it holds it, for the address it was asked at. The
  desktop app gets a key for its window's session. A proof for a plain-http
  address goes only to the home network (403 from the internet), one for an
  https address only through a proxy on it, and none goes to a website's
  page, so a device elsewhere can't get one passed on. A reverse proxy must
  now pass on only the server's own names (deploy/README.md).
- A playback with keyed addresses has its file's PGS subtitles beside its
  media as pictures, each placed and timed (`subtitles-{index}.json` and its
  PNGs), for a player that can't decode PGS: Roku draws them over the video
  rather than have them burned in.

### Changed

- Seek previews, fingerprints and other background work pause while the
  subtitles of a film being played are read out of its Matroska file, a
  minute and a half at most each time, so on a NAS those come sooner while
  seek previews are being made.
- Picture subtitles (PGS, VobSub, DVB) that the apps draw show within a few
  seconds of a playback starting, where they could take up to a minute:
  the transcoder reads those around where the playback is out of a
  Matroska file first, at once, and the rest of the track after the
  playback's start. A `.mks` asked for with `at` (where the player is)
  starts there, and a seek while the rest is read has the pictures where
  the player went read at once. Kodi's `.sup` has everything read by the
  time it opens, not only the pictures around the start.
- A stream for Kodi with VobSub or DVB subtitles no longer starts a copy
  of them that Kodi never reads, which, in a Matroska file whose index
  doesn't list them, could read the whole film from a network share.
  Needs Pley for Kodi from the same release, which names the picture
  subtitles it shows beside a stream (`picture_subtitles_sideload_codecs`).
- A PGS subtitle file next to the media (`.sup`) is no longer copied as
  each playback starts, only when a player asks for it: Kodi reads it as
  it is.
- PGS subtitles of a Blu-ray remux come whole about twice as fast from a NAS
  or a slow disk: each picture is one read of the file, where it was two or
  three.
- A remux's picture subtitles aren't left to a read of the whole file (a
  quarter of an hour or more from a NAS) when a busy NAS makes reading
  them take over a minute, and a seek while the whole file is read for
  them still has the pictures where the player went, rather than none.
- The rest of a film's picture subtitles, read after those around where it
  plays, pauses while a playback starts or the pictures where a player
  seeks are read, so on a NAS a seek soon after a film starts has its
  subtitles in about a second, where it could take four.

### Security

- A Live TV playlist's own guide is used only when it is a web address
  (http or https). One naming a file or a network share
  (`url-tvg="//host/share/guide.xml"`) had the server open it, on Windows
  signing in to that share.
- People who aren't administrators no longer see FFmpeg's words, which
  name the server's files, in the activity's sessions or a download that
  failed, as apps already didn't.
- The folder browser (Settings, picking a library's or the transcoder's
  folder) is asked with `POST /api/fs`, no longer `GET`. Over plain HTTP
  browsers don't say which page asked for a GET, so another site's link
  or picture could have the server open a folder of its choosing while an
  administrator was signed in, and on Windows a share named so had the
  server sign in to it.
- A library's files are sent from the file checked: a file played as it is,
  pictures, a `.sup` beside the media and the compatibility API's files and
  subtitle files are opened once, checked, and read from what was opened.
  The file was checked and then opened again by its name, so someone who
  could write in a library folder could swap a link in between and have
  the server hand out its database or `secret.key`.
- Transcodes, downloads, frame grabs, album covers, subtitle copies and
  seek previews check that a file hasn't become a link to another kind of
  file since the scan, as playing it as it is does: the transcoder was
  handed whatever the link led to.
- Live TV's segments are kept to the transcoder's account in a transcode
  folder chosen elsewhere (`/tmp`, `/dev/shm`, a folder on a second Windows
  drive), as a playback's are: other accounts on the computer could read
  what played, or swap a segment. A link or a junction another account put
  in place of one of the transcoder's folders there is taken away rather
  than followed: the hourly clean-up of its caches could delete whatever
  it led to. One put back while the transcoder makes the folder is
  refused, and what it leads to keeps its permissions.
- While the transcoder isn't running, people and apps are told just that,
  no longer its last words as it went, which named files and accounts:
  administrators still find them in Settings → Transcoder and the log.
- A guide saved as a file on this computer is read only as one, and only
  up to a guide's size: no longer one on another computer
  (`//host/share/…`, `\??\UNC\host\share\…`, which a playlist could have
  saved before), a device or a pipe. A source with a guide file it couldn't read isn't saved.
- A channel's stream asked for with `take_over` takes another channel's
  connection only with a token: over plain HTTP, another site's link signed
  in by an administrator's cookie could end what others watched.

## [0.1.3] - 2026-11-02

### Added

- Pley installs on a Synology NAS as a package (`.spk`) for DSM 7 on Intel
  models, with FFmpeg from SynoCommunity's package and the NAS's GPU for
  transcoding. A static Linux build, `linux-x86_64-static`, is published
  with the other archives.
- A playback with keyed addresses has its seek previews beside its media,
  as a BIF file (`previews.bif`), which Roku's own player fetches and shows
  as you fast forward and rewind.

### Changed

- The web app's PIN pad stops once a profile's PIN is locked after too
  many wrong tries, as the other apps' pads do; a profile that got a PIN
  after the list was loaded asks it; and the profile in use shows its lock.
  `POST /api/auth/switch` answers a refused PIN with `code` `wrong_pin` or
  `pin_locked`.
- A library's posters in the web app show 4K and the dynamic range (DV,
  HDR10) whatever the sort, where they showed only when sorted by bitrate,
  and no longer the codec (H.264, HEVC) or a lower resolution (1080p,
  720p). The next start reads each file's dynamic range once, for the
  libraries to list it quickly.

## [0.1.2] - 2026-11-01

### Changed

- Seek-bar previews take about a fifth less space: they are saved at a
  JPEG quality that looks the same at their size. Previews made before are
  made again in the background, after the files that have none, in the
  hours library work runs; they serve until then.
- The seek previews Plex apps take (their BIF index, larger than the
  previews themselves) are kept in the cache for 30 days instead of for good
  beside the previews, and made again when asked for. Those kept so far are
  removed at the first start.
- Seek previews are made several at once where that is faster and disturbs
  nobody. A title someone opens no longer waits for the libraries'
  previews (made alone, though, where the transcoder has fewer than 4
  cores). The libraries' go one file per disk, up to four at once, in the
  hours library work runs: a second once nothing has played for 10
  minutes, more in the hours people rarely watch, while the processors and
  memory have room. A hard disk, an array of them, a ZFS pool or a network
  share is still read one file at a time; a solid-state disk takes up to
  four. With a transcoder from before this, on macOS, or with
  `PLEY_TRANSCODER_MAX_PREVIEWS=1`, previews are made one at a time, as
  before. The dashboard says how many are made at once.
- On Unraid, searching Apps for Plex, Jellyfin, Emby or IPTV also finds
  Pley, and Apps offers it only on Unraid 6.12 or newer.
- The Docker image has signed build provenance, as the release files do,
  and its page on GitHub lists only `linux/amd64` and `linux/arm64`.

### Fixed

- Streams play with an FFmpeg build whose segment list names segments by
  number alone, such as SynoCommunity's for Synology DSM. Before, the
  transcoder never saw a segment finish with such a build, so every stream
  that wasn't direct play waited forever.
- Seek previews of a file the transcoder couldn't reach for a moment (a
  share whose disks slept, a share not mounted yet) are made when the file
  is next opened or scanned, instead of being given up until a restart.
- Seek previews of very large files on slow disks are no longer given up
  after an hour, and those paused through a long transcode keep their work.
- In a container limited to a few processors, making seek previews no
  longer slows the server beside them.

## [0.1.1] - 2026-11-01

### Security

- Radarr's and Sonarr's addresses can't be link-local (`169.254.0.0/16`,
  `fe80::/10`), typed in or as their names resolve, and whatever answers
  there in their stead is never quoted back: only its HTTP status. Their
  answers are read 32 MB at most.

### Added

- Requests. Discover finds movies and shows on TMDB (trending, popular,
  upcoming, anime, or a search) and marks what the libraries have. People
  ask for a movie or a show's missing seasons; administrators approve or
  decline them, and accounts may be allowed to have theirs approved as
  asked, within limits per 7 days. Approved requests go to Radarr and
  Sonarr, whose downloads' progress shows, and a request's maker is told
  once a library has it. Turned on in Settings → Requests; who may ask is
  in Settings → Users, where profiles without a password and accounts with
  an age limit start without. `/api/discover`, `/api/requests` and
  `POST /api/settings/arr` ([docs/API.md](../../docs/API.md#requests)).
- Requests in Pley's apps: they ask with the new `requests` scope, and an
  administrator's Pley apps approve and decline with `approvals`; an app
  signed in before is told to sign in again. Discover's pictures come
  through the server at the size each screen draws them, kept for 30 days
  and 256 MB at most, so no app contacts TMDB. Discover marks shows partly
  in a library, titles declined lately, and each season's state; accounts
  with an age limit see only titles within it. A library title's page can
  show its request (`GET /api/items/{id}/request`). The request summary
  says what an account may do and what is new for it, and apps hear when
  to fetch it again.
- Accounts made from an invitation with an age limit, or given one later,
  don't ask for titles until an administrator lets them; accounts that may
  not ask still see and withdraw their own requests.
- A request says where it is (waiting for an administrator, sent to Radarr
  or Sonarr, downloading, importing, partly or wholly ready, or gone from
  the library since) and whether it has news its maker hasn't seen. Why a
  request failed, and where it was sent, are for administrators.
- Asking for a title someone has asked for already follows their request
  (*Tell me too*): its news comes to each follower, and following costs
  nothing against the limits. When its maker withdraws a request others
  follow, or their account is deleted, the earliest follower keeps it, and
  it counts against their limits from then.
  What was declined for an account may be asked for again after 30 days,
  and the answer says from when.
- Administrators' approvals answer at once while the request goes to
  Radarr or Sonarr in the background. An approval may keep only some of
  the seasons asked for and choose among Radarr's and Sonarr's quality
  profiles and root folders (`GET /api/requests/options`); an approved
  request not yet available may still be declined, and one may be marked
  as added by hand, so it is never sent.
- Requests are listed by several statuses at once, by what needs an
  administrator, by kind, person, title or news, and oldest first. Each
  request says what may be done to it, and request events say which
  request changed and go only to whom it concerns: a new request to
  administrators, a decision to its maker and followers, a request ready
  to each account that can open it, a failure to administrators.

- Administrators who use two-step sign-in can require it of everyone
  (Settings → Server → Sign-in). Whoever hasn't set it up is signed out
  everywhere, and sets it up at their next sign-in, right after the
  password: a QR code, or on a phone a link to the authenticator app, then
  the first code and the recovery codes. Joining with an invitation,
  switching to their profile and signing in a TV at `/link` set it up the
  same way. Settings → Users shows who hasn't set it up yet.
- Two-step sign-in moves to another authenticator app (Account → Use
  another app) with the password and a code of the current one, without
  turning it off.
- A code from the authenticator app pasted with a space in it ("123 456")
  signs in.

### Changed

- Pley is free software under the GNU Affero General Public License,
  version 3 (`AGPL-3.0-only`, `LICENSE`). The server and the transcoder
  give the address of their source (`GET /api/server`, which the web
  app's Account page links to; `GET /v1/info` and `--license`).

### Fixed

- The player's details say why Dolby Vision plays as its HDR10, HLG or
  SDR base layer: the screen doesn't show it, the player doesn't decode
  its profile or level, its remux doesn't name it, or HDR10+ comes with
  it on a player that doesn't show the two together ("HDR10 (Dolby Vision
  base layer, as this screen doesn't show Dolby Vision)").
- Dolby Vision with HDR10+ plays as it is for a player that says it shows
  the two together (`dolby_vision_hdr10plus`, Kodi on CoreELEC), rather
  than without its HDR10+ or as its base layer.
- The web app's title page shows the quality badges (4K, DV P8.1,
  HDR10…) of the version picked, not always the first version's.
- The web app's sidebar shows Live TV only once it has channels, for
  administrators too; they set it up in Settings → Live TV.
- Live TV's guide says Today, Tomorrow and Yesterday on the days daylight
  saving starts or ends too: a programme just after midnight tomorrow
  showed its date, or the day after tomorrow showed as Tomorrow.

## [0.1.0] - 2026-10-30

### Security

- A session signed out, or whose account was disabled or changed, could
  keep working for up to a minute when it was being checked at that
  moment.
- An administrator's own PIN changes only in Account, with the password,
  no longer through the users list with the session alone.
- The transcoder's folder for segments in memory (`/dev/shm`) is never one
  another account on the computer made first.
- A file's attached fonts are taken 1,000 at most.
- An account keeps at most 200 networks remembered for Automatic quality.
- Subtitles from OpenSubtitles download over https only.
- ctutils 0.4.7 (RUSTSEC-2026-0385).
- quick-xml 0.41 for XMLTV guides (RUSTSEC-2026-0194, RUSTSEC-2026-0195).
  A guide's title or description holding an HTML entity (`&nbsp;`) no
  longer comes out empty.

### Added

- Live TV: IPTV channels from an M3U playlist or an Xtream Codes account,
  with their XMLTV guide, watched in the web and desktop apps through the
  server, at home or away. A guide that scrolls through time, channel
  cards with what's on, favourites, recently watched, and a player with
  channel up and down, a channel list, pause and rewind (five minutes by
  default) and picture in picture. The server holds the provider's
  connections: a channel watched on several devices takes one, and when
  all are in use the apps say which channels hold them. Interlaced and
  non-H.264 channels are converted, on the graphics card where there is
  one. Settings → Live TV has the sources, the groups (a whole group or
  region hidden at once), the lineup (hide, rename, renumber, pick a
  guide channel) and who is watching what. Channels are browsed by group:
  groups named by region ("UK| Sport") fold under it,
  and channel up and down stay in the group. Lineups of tens of
  thousands of channels open at once: the apps fetch a page of channels
  and the guide of those in view.
- Live TV for players that can't send the token: a channel watched with
  `keyed_urls` gets a playlist address with a key of its own
  (`/api/live/media/{key}/index.m3u8`), so the Samsung and LG apps play it
  through the TV's own player.
- The web app draws picture subtitles (PGS, VobSub, DVB) over the video
  itself, so the file still plays as it is, or its stream keeps HDR,
  rather than having them burned in.
- A picture subtitle track still being copied out of a big file comes as
  far as it is (`X-Pley-Complete: 0`), and its copy starts as the playback
  does; one a Matroska file's Cues list comes whole in seconds.
- PGS tracks as `.sup` files (`/api/files/{id}/subtitles/{index}.sup`),
  for Kodi. Play responses say `pts_offset_ms`.
- Finding intros and credits and making previews for whole libraries run
  when they disturb nobody. Once nothing plays, what's likely to be watched
  soon goes first, at any hour: shows someone is watching, titles on a
  list or begun, what's new. The rest of the libraries waits for the hours
  people rarely watch, which the server learns from when something plays
  (from the watch history at first). Both wait while other programs keep
  the processors busy. Settings → Server → Library scanning sets when it
  runs (automatically, between set hours, or whenever nothing plays), shows
  the week as learned, and the dashboard says what waits and why.
- An Unraid template for the Docker image (`deploy/unraid`), with a
  step-by-step guide to installing Pley and giving it an Intel, AMD or
  NVIDIA graphics card.

### Changed

- Pley's apps are known by client IDs on `pley-media-server.github.io`,
  and the sign-ins they hold move to them.
- Links, the Docker image (`ghcr.io/pley-media-server/pley`) and the Unraid
  template are on `github.com/pley-media-server/pley`.
- Pley is Restricted, its licence terms to be determined (`LICENSE`), and
  the dashboard no longer shows the transcoder's licence.
- The Docker image can be started as root, as Unraid, Synology and TrueNAS
  start containers: Pley then runs as `PUID`:`PGID` (1000 unless set),
  gives that user the config folder, joins the groups of the graphics cards
  passed in, and keeps no capabilities. Started as a user, as the Compose
  setup does, it runs as that user, as before.
- The Docker image has Intel's full VAAPI driver, so 6th to 10th generation
  Intel graphics (Skylake to Comet Lake) transcode on the graphics card
  rather than the processor.
- The web app's sidebar has no line above Settings.
- Picture subtitles a player shows itself are burned in on any stream it
  asks for (Remux, Transcode), not only when the file can't play as it is.
- AMD graphics on Windows transcode faster: the frames reach AMD's encoder
  from the graphics card, where it copied them from system memory itself,
  several times slower.
- HDR tone mapping does less where the graphics card would fall behind the
  player: Pley times it when it starts, and each transcode takes the best
  quality that keeps ahead, counting the other HDR transcodes running.
- Several HDR transcodes at once keep up: one already running moves to the
  quality the graphics card can share as soon as another starts, and where
  AMD or Intel graphics fall behind even so, the processor decodes the
  video while it has cores to spare. Two 4K HDR films tone mapped to 4K at
  once went from 0.9× to 1.2× real time each on a Ryzen 5 5600G.
- The fastest tone mapping quality is faster where the picture shrinks by
  half at most (4K to 4K or 1080p), with the same picture: two 4K films
  made 1080p at once went from 1.0× to 1.65× real time each.
- HDR transcodes share the graphics card by what each needs of it: a 4K
  film made 1080p beside one kept 4K takes less than two kept 4K. The
  processor decodes only with cores to spare beyond what other programs
  use. Where transcodes fall behind anyway, the transcoder learns from it
  for a while: a cheaper quality, then the processor decoding, or the
  graphics card decoding again where the processor had no time to spare. A
  transcode waiting for its hard disk teaches nothing.
  `PLEY_TRANSCODER_CPU_DECODE=never` keeps the decoding on the graphics
  card.

### Fixed

- H.264 from AMD's encoder says its colours (BT.709), which it left out:
  players guessed, and some took 4K video for BT.2020.
- In a container given part of the computer's processors (`--cpus`,
  `--cpuset-cpus`) or under systemd's `CPUQuota=`, the transcoder counts
  what other programs leave it of those, not of the whole computer, before
  it decodes on the processor.
- Automatic quality in the web app keeps the original (direct play or
  remux) unless the picture stops for want of data for 2 s or more. Short
  waits and slow delivery moved it to a transcode (30 Mbps 4K for a 4K
  film) though the network carried the file: a browser keeps only seconds
  of a big file loaded ahead. From a stream in several sizes it now steps
  back up after calm minutes too, to the original when the network carries
  1.6 times it, measured from each segment's first byte rather than from
  the wait while the transcoder made it.
- Automatic quality in the web app no longer leaves the original for a
  wait the server caused: a remux whose audio is converted, waiting on a
  slow disk or the server's library work, went to a transcode and then
  between its sizes. A wait of 2 s or more at the original counts only on
  a network measured below 1.5 times the file's bitrate over the last 2
  minutes (where nothing measured it, the second such wait within 5
  minutes), and not when the browser had a second or more loaded ahead.
  A title that starts lower for what was remembered of the network goes
  back up after 20 s calm, as high as the network carries, the original
  included.
- A transcode is made at the bitrate of the quality picked: 20 and 12 Mbps
  1080p came out at 8 Mbps, 30 Mbps 4K at 20 and 2 Mbps 480p at 1.2. The
  original converted for a player that can't play it gets its quality's
  bitrate too (30 Mbps for a 4K remux). A quality never gets more than half
  again the file's own bitrate, nor more than the highest quality of its
  lines, whatever a player asks (an app of the compatibility API asking
  for its most). A stream in several sizes also comes at its own size's
  usual bitrate below such a quality (8 Mbps for 1080p), for a network
  that can't carry it.
- The web app's menus, tooltips and buttons over the video no longer
  change its colours as they show: their blurred backgrounds made Chrome
  and Edge take the video off the graphics card's overlay, where the
  driver's colour settings for video apply. They are plain dark now.

## [0.0.1] - 2026-10-29

### Added

- First release with a version of its own (builds before it carried 0.2.0,
  the version every app shared).

[Unreleased]: https://github.com/pley-media-server/pley/compare/release-2026-11-12...HEAD
[0.1.10]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-12
[0.1.9]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-08
[0.1.8]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-06
[0.1.7]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-05
[0.1.6]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-04
[0.1.5]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-03
[0.1.4]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-02.2
[0.1.3]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-02
[0.1.2]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-01.2
[0.1.1]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-01
[0.1.0]: https://github.com/pley-media-server/pley/tree/14792b986c3f69d55bd07b39e84716da0e8e1645
[0.0.1]: https://github.com/pley-media-server/pley/tree/7e14bb9d498d054a73b2176cd8949b6864ffacd9

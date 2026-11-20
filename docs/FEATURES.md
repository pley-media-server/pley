# Features

What Pley does, in full. The [README](../README.md) has the short version.

## Libraries

- Movies, TV shows, music and audiobooks, scanned incrementally and
  rescanned when files change. An offline drive isn't mistaken for deleted
  media.
- Common naming schemes: `Movie (2019)`, release names, `{tmdb-123}` hints,
  editions (`{edition-Director's Cut}`), `S01E02`, `1x02`, date-based and
  anime episodes, and extras (`Trailers/` folders,
  `Movie (2019)-trailer.mkv`). Looser episode names work too: `04 - Title` or
  `104 - Title` in a `Season 1` folder, `Season 1 Episode 4`, or a title
  alone, which TMDB numbers by its title. Two files never merge into one
  episode for lack of a number.
  [deploy/docker/media](../deploy/docker/media/README.md) shows a layout.
- Copies of a movie are one movie with versions, even when their names
  differ (TMDB corrected the year or title after the first was named):
  movies in a library matched to the same TMDB title merge, with what
  everyone watched, rated, listed and edited of either. Fix Match on it
  lets the copies whose names don't make it go back to being movies of
  their own, and they stay apart from it until a Fix Match on one of them
  picks its title again.
- `.strm` files, as Kodi, Jellyfin and Emby read them: a text file in a
  movie or TV library that names the `http` or `https` address its video
  is at. It is listed, played, converted and downloaded like a video on
  disk, on every app, and its address never leaves the server.
  Administrators turn them on in Settings → Server, where they're off by
  default ([.strm files](CONFIGURATION.md#strm-files)).
- Each profile can order its libraries, on every app.
- Moving from Plex: its libraries, its Home users as profiles with their
  restrictions, labels, dates added, and what everyone watched
  ([MOVING_FROM_PLEX.md](MOVING_FROM_PLEX.md)).

### Audiobooks

Audiobook libraries are in the web and desktop apps. The other apps and
the Plex- and Jellyfin-compatible APIs don't get them.

- Books are found as Audiobookshelf finds them. A book is a folder of
  audio files (with `CD 1`, `Disc 2`… folders inside it if it has them),
  an `.m4b` file, or `.m4b` files of the same `album` tag in one folder.
  Files directly in the library's folder are books of their own; other
  files side by side in a folder are parts of one book, `.m4b` files
  excepted.
- Folders are read as `Author/Book`, `Author/Series/Book`,
  `Author/Book/Book.m4b` or `Author/Book.m4b`: of the folders above a
  book's own, the first is its author's and the second its series'. A
  folder at the top of the library is a book's own (`Book/01.mp3`), but
  one with an `.m4b` in it is its author's (`Author/Book.m4b`), unless
  it is named as the book or the file is. A book's folder may begin with
  its place in the series (`Book 2 - Title`, `2.5 - Novella`), end with
  its year (`Title (2007)`), and name its narrator in braces
  (`Title {Narrator B}`), as Audiobookshelf's do. An `.m4b` without an
  `album` tag is named by its file.
- A book's parts play by disc and number (their tags', or the one their
  files' names begin with), then in their files' names' order, numbers
  read as numbers: `Part 2` before `Part 10`.
- A book is what its first part says, by its tags, which win over its
  folders: `album` is the book, `album_artist` or `artist` its authors
  (`;`, `&` or `/` between several), `composer` or `narrator` its
  narrators, `series` and `series-part` (or `part`, as Libation and tone
  write it) its series, or else a `grouping` as Audiobookshelf writes it
  (`Series #2`, `Series, Book 2`), `description` or `synopsis` its
  summary (`comment` without them). A scan reads them again when the
  book's files change.
- The cover is a picture named after an `.m4b` beside it, `cover.jpg` or
  `folder.jpg` in the book's folder, or else the one inside its files.
  Books side by side in their author's or their series' folder don't take
  its pictures.
- An `.m4b` file's chapters are read, and it plays directly where `.m4a`
  does.
- Play on a book goes on in the part heard last, where it was left. A part
  is finished only in its last 30 seconds, not at 90 % as a film is.
  Books part way through are in Continue Listening, on Home and in their
  library.
- A book's page has its writers, narrators and series, how much of it is
  listened to and how much is left, and its parts, or the chapters of a
  book in one file, each played from where it starts. The player's bar
  runs through the chapter playing, named under the book. The player goes back 15 seconds and on 30, plays from
  0.75× to 3× (kept for every book), and has a sleep timer: 15 to 60
  minutes, or the end of the chapter or part.
- Summaries, genres, narrators, series and covers that a book's files and
  folders don't give come from Open Library, and from Audnexus for books
  whose files carry an Audible ASIN (see
  [Metadata sources](CONFIGURATION.md#metadata-sources)).
- Books have no pages of their writers or narrators, no series as collections and no downloads.

## Metadata

- Movies from TMDB (free API key); TV shows from TMDB, TheTVDB or TVmaze (no
  key); music from MusicBrainz and the Cover Art Archive (no key);
  audiobooks from Open Library and Audnexus (no key). See
  [Metadata sources](CONFIGURATION.md#metadata-sources).
- NFO files from Kodi, Jellyfin, Radarr, Sonarr and the like, for movies,
  shows, seasons and episodes: their ids decide the match and their fields
  win over the online source. See [NFO files](CONFIGURATION.md#nfo-files).
- Local artwork and hand edits always win. Cast and crew have their own
  pages.

## Playback

- Direct play, remux or transcode, chosen per device, with the reason shown.
  One Quality menu: Original, Automatic, or a lower quality
  ([QUALITY.md](QUALITY.md)).
- Hardware encoding (NVENC, Quick Sync, AMF, VAAPI, VideoToolbox) and GPU
  tone mapping with libplacebo, including Dolby Vision profile 5.
- Dolby Vision profile 7 (FEL or MEL) and HDR10+ are told apart from plain
  HDR10 and shown as badges; the player's details say what the screen gets.
- Subtitles: text tracks as WebVTT; picture tracks (PGS, VobSub, DVB)
  drawn over the video by the web, desktop, Android, Samsung and LG apps
  (PGS by Kodi too), so the file still plays as it is, and burned in
  elsewhere; a timing offset in every player, the TV apps included.
- Audio and subtitle defaults per profile, on every app; a choice carries
  over to the next episode. Among tracks in the profile's language, the
  server picks the one the device plays best (as is rather than converted;
  the 5.1 track for a player that passes it to a receiver).
- Resume, Up Next, Skip Intro and Skip Credits (from chapters, and by
  comparing the audio of episodes), seek-bar previews and chapter pictures.
- A remote for your other devices: the web and desktop apps' header lists
  those that are on, the ones playing first, then those ready to play,
  then the devices of your household's other profiles whose app can switch
  to yours, each with the profile it is on and never what it plays. For
  the one chosen it plays, pauses, seeks, skips back and forward 30
  seconds, goes to the next or previous title (or channel, on Live TV),
  changes the audio track, the subtitles and the quality, and stops what
  plays there, once asked to. In an intro or the credits there, it offers
  Skip Intro or Skip Credits, as the player there would. On Live TV it
  shows the program's progress and what's next. Each player obeys the
  commands it can, without bringing up its controls there: a brief notice
  says what changed. Any can be stopped ([API.md](API.md#remote-control)).
- Play on… in the web and desktop apps, beside Play on a movie, an episode,
  a show, a season, an artist, an album, a track and a playlist (Shuffle
  on… beside Shuffle), and Watch on… for a Live TV channel: it plays there
  on another of your devices that takes casts, as that device's own Play
  would, with the audio and subtitles picked on the page, and the remote
  opens on it ([API.md](API.md#casting)). One on another profile of your
  household switches to yours first, so it plays as you; when your
  profile has a PIN, Play on… asks for it in its menu's place. The
  desktop app takes casts too, as "Pley for Windows", "Pley for Mac" or
  "Pley for Linux", switching to the profile that sent one; a browser tab
  doesn't, as a browser won't start sound in a tab in the background.
- Chromecasts, Google TVs and TVs with Cast built in, Apple TVs and TVs
  with AirPlay 2 on the server's network take casts of movies and episodes
  from the household: the server plays them there itself, in HEVC, 4K and
  HDR on models that play them, with the subtitles picked, keeps their
  place, and the remote controls them as it does Pley's own players. An
  AirPlay device that asks for a code is paired once, with the code it
  shows. Administrators turn casting to these devices on in Settings →
  Network, where it's off by default, and forget pairings there
  ([API.md](API.md#casting)).

## Live TV

- IPTV channels from M3U playlists and Xtream Codes accounts, with their
  guide from XMLTV, on every app: categories, a guide, a channel's number
  typed on the remote, and one search with the rest of the library.
- A channel is pulled once however many watch it, and a source never plays
  more channels at once than its provider allows; changing channels frees
  the connection at once.
- Channels the TV can play as they are play straight from the provider's
  stream; the others through the transcoder. Channels that don't play, and
  divider rows, can be left out.
- Channels a provider serves as HLS are read segment by segment, their
  sound taken from the default audio track when it is kept apart from
  the picture (AAC, or audio in MPEG-TS).

## Watching

- Progress, history, star ratings, Continue Watching and Next Up per user.
- Recently Added, Recently Released, Top Rated, More Like This and full-text
  search.
- Collections (from TMDB, curated, or smart) and playlists (ordered or smart).
  A film library can do without the collections from TMDB, for a tool such
  as Kometa to make its own.

## Requests

- Discover shows TMDB's movies and shows (trending, popular, upcoming,
  anime) and marks what the libraries have. Search lists them too, after
  the libraries' results, for those who may ask.
- People ask for a movie, or for the seasons of a show the libraries lack;
  administrators approve or decline, with a reason. Accounts may be allowed
  to have theirs approved as asked, and limited to so many movies and
  seasons a week. Profiles without a password and accounts with an age
  limit don't ask until an administrator allows it.
- Approved requests go to Radarr and Sonarr (with a quality profile and
  root folder of their own for anime), and their downloads' progress shows.
  A request is available, and its maker told, once a library has the movie
  or every season asked for.

## Accounts

- Administrators, users and managed profiles for children, each with their
  own libraries and age limit.
- A profile switcher with optional PINs, invitation links, and a list of
  signed-in devices that can be signed out remotely.
- PINs work the same way on every app. A profile with one shows a lock, and
  switching to it asks the PIN: a wrong one asks again, too many in 15
  minutes say how long to wait, and after 100 wrong ones in a row the PIN
  stops working until its owner or an administrator sets it again. A TV
  starts straight into Home as the profile it was left on when that profile
  has no PIN (Apple TV asks "Who's watching?" in a household of several,
  unless Always start as names one). When it has one, the "Who's
  watching?" a TV asks as it starts asks that PIN too, and one set to
  always start as a profile asks it first, so a TV never starts as a
  profile with a PIN without it. Signing in again on the web goes straight
  in as the profile the browser was last used as, when it has no PIN. A
  profile that can see more than yours and has no PIN takes its password,
  in the browser; a TV signs in to it again with a code.
- Two-step sign-in with an authenticator app, with recovery codes:
  optional, or required of everyone by an administrator, in which case each
  person sets it up at their next sign-in (for a TV, on their phone).
- Apps sign in on Pley's own page and get only what you allow (browsing,
  playing, playlists, and for an administrator's Android phone, managing
  the server), never its settings or keys: OAuth 2.1 with client metadata
  documents. Pley's own apps work without internet access; apps by other
  publishers are off until an administrator allows them. TVs show a code to
  type on Pley's `/link` page ([API.md](API.md#apps-oauth-21)).
- Apps and tools that use the compatibility APIs get tokens of their own,
  from Account or by signing in to the Jellyfin-compatible API, which work on
  those APIs only and do only what you allow: watching and, for
  administrators, seeing everyone's activity or managing libraries, never
  accounts. A token can be limited to the addresses it is used from; Account
  lists each one and revokes it
  ([COMPATIBILITY.md](COMPATIBILITY.md#tokens-for-other-apps)).
- First-run setup needs a one-time code from the server's log, which the tray
  icon fills in. Passwords have at least 8 characters and can't be common
  ones ([SECURITY.md](../SECURITY.md)).

## Administration

- Metadata editing with locked fields, Fix Match and a choice of artwork.
- Settings with a Test button for each API key and service, a live dashboard
  (playback, transcoder, scans, and charts of Pley's and the computer's CPU
  and memory) and daily database backups ([STORAGE.md](STORAGE.md)).
- On an Android phone or tablet, Me → Manage server has the dashboard,
  libraries, users and invitations; the server's settings, network,
  transcoder, backups and Live TV stay in the web app
  ([ANDROID.md](ANDROID.md)). TVs, signed in with a code, never manage the
  server.

## Compatibility APIs

The Plex-compatible API, on Pley's own port by default (or 32400), for
`python-plexapi`, library tools such as Kometa and activity monitors such
as Tautulli: browsing with their filters, editing, collections, playlists,
play queues, sessions, history and notifications.

The Jellyfin-compatible API, on Pley's own port by default (or 8096):
Jellyfin's apps and others built on its API browse the libraries, play as
they are or as HLS from Pley's transcoder, with subtitles, skip intro and
credits, and keep watch state, favorites (My List) and ratings with Pley's
accounts ([COMPATIBILITY.md](COMPATIBILITY.md)).

## Offline

On the home network everything works without the internet. Item pages
download a file's original, which the Android app (phones and tablets) and
the desktop app keep to play while the server is away; the Android app
reports what was watched once it is back.

## Performance

Single binaries; SQLite in WAL mode with one writer; listings filtered,
sorted and paged in SQL; compressed responses; pictures resized once and
cached; transcodes throttled to about twice real time.

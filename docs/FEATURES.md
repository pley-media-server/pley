# Features

Everything Pley does. The [README](../README.md) has the short version.

## Libraries

- Movie, TV, music and audiobook libraries. Scans are incremental, and
  Pley picks up changed files on its own. A drive that goes offline isn't
  treated as deleted media.
- Understands the usual naming schemes: `Movie (2019)`, release names,
  `{tmdb-123}` hints, editions (`{edition-Director's Cut}`), `S01E02`,
  `1x02`, date-based and anime episodes, and extras (`Trailers/` folders,
  `Movie (2019)-trailer.mkv`). Looser episode names work too, like
  `04 - Title` or `104 - Title` inside a `Season 1` folder,
  `Season 1 Episode 4`, or just the episode's title, which TMDB looks up.
  Two files are never merged into one episode just because neither has a
  number. [deploy/docker/media](../deploy/docker/media/README.md) shows an
  example layout.
- `.strm` files, the way Kodi, Jellyfin and Emby use them: a text file
  with the `http` or `https` address of a video. Pley lists, plays,
  converts and downloads it like any other video, on every app, and the
  address never leaves the server. They're off by default; turn them on
  in Settings → Server ([.strm files](CONFIGURATION.md#strm-files)).
- Each profile can reorder its libraries, on every app.
- Filter movies and shows by resolution (4K, 1080p, 720p, SD) and dynamic
  range (HDR, Dolby Vision, HDR10+, HLG, SDR), on every app. A title
  counts by its best file. Smart collections and playlists saved from a
  filtered view keep the filter.
- Coming from Plex? Pley imports its libraries, Home users (as profiles,
  with their restrictions), labels, dates added and everyone's watch
  history ([MOVING_FROM_PLEX.md](MOVING_FROM_PLEX.md)).

### Audiobooks

Audiobook libraries work in the web and desktop apps for now. The other
apps and the Plex- and Jellyfin-compatible APIs don't show them yet.

- Books are found the same way Audiobookshelf finds them. A book can be a
  folder of audio files (with `CD 1`, `Disc 2` subfolders if needed), a
  single `.m4b`, or several `.m4b` files with the same `album` tag in one
  folder. Files sitting directly in the library folder are each their own
  book.
- Folder layouts: `Author/Book`, `Author/Series/Book`,
  `Author/Book/Book.m4b` or `Author/Book.m4b`. A top-level folder is
  treated as a book (`Book/01.mp3`), unless it holds `.m4b` files, in which
  case it's the author (`Author/Book.m4b`). Book folders can start with the
  series number (`Book 2 - Title`, `2.5 - Novella`), end with the year
  (`Title (2007)`), and name the narrator in braces (`Title {Narrator B}`),
  like Audiobookshelf's.
- Parts play in disc and track order (from tags or file names), then by
  file name, with numbers sorted properly: `Part 2` before `Part 10`.
- Tags win over folder names: `album` is the title, `album_artist` or
  `artist` the author (several separated by `;`, `&` or `/`), `composer`
  or `narrator` the narrator, and `series` with `series-part` (or `part`,
  as Libation and tone write it) the series. An Audiobookshelf-style
  `grouping` (`Series #2`, `Series, Book 2`) works too. The summary comes
  from `description`, `synopsis` or `comment`. Tags are read again when
  the files change.
- Covers: an image named after the `.m4b`, `cover.jpg` or `folder.jpg` in
  the book's folder, or the one embedded in the files. A book doesn't pick
  up images from its author's or series' folder.
- Chapters in `.m4b` files are read, and `.m4b` plays directly anywhere
  `.m4a` does.
- Play resumes in the part you were last listening to, where you left off.
  A part only counts as finished in its last 30 seconds (films use 90 %).
  Books in progress show in Continue Listening on Home and in the library.
- A book's page shows its authors, narrators and series, your progress and
  time left, and its parts or chapters, each playable from its start. The
  player's progress bar follows the current chapter. You can skip back 15
  seconds and forward 30, change speed from 0.75× to 3× (remembered for
  every book), and set a sleep timer for 15 to 60 minutes or the end of
  the chapter or part.
- Whatever the files and folders don't say (summary, genres, narrators,
  series, cover) comes from Open Library, or from Audnexus when the files
  have an Audible ASIN
  ([Metadata sources](CONFIGURATION.md#metadata-sources)).
- Not there yet: author and narrator pages, series as collections, and
  downloads.

## Metadata

- Movies from TMDB (free API key). TV shows from TMDB, TheTVDB or TVmaze
  (no key needed). Music from MusicBrainz and the Cover Art Archive.
  Audiobooks from Open Library and Audnexus. See
  [Metadata sources](CONFIGURATION.md#metadata-sources).
- Reads NFO files from Kodi, Jellyfin, Radarr, Sonarr and similar tools,
  for movies, shows, seasons and episodes. Their ids decide the match, and
  their fields take priority over online sources
  ([NFO files](CONFIGURATION.md#nfo-files)).
- Local artwork and your own edits always win. Cast and crew have their
  own pages.

## Playback

- Direct play, remux or transcode, decided per device, and the player
  tells you why. One Quality menu: Original, Automatic, or a lower quality
  ([QUALITY.md](QUALITY.md)).
- Automatic quality adjusts to your connection while you watch. It starts
  at what the network can carry, drops a step when playback starts to
  stall, and moves back up once the connection recovers. The web and
  Android apps switch without a pause or a black screen.
- Hardware encoding (NVENC, Quick Sync, AMF, VAAPI, VideoToolbox) and GPU
  tone mapping with libplacebo, Dolby Vision profile 5 included.
- Dolby Vision profile 7 (FEL or MEL) and HDR10+ are detected and shown as
  badges, and the player's details say what your screen is actually
  getting.
- Subtitles: text subtitles as WebVTT. Image subtitles (PGS, VobSub, DVB)
  are drawn by the web, desktop, Android, Samsung and LG apps (and PGS by
  Kodi), so the video doesn't need transcoding; other players get them
  burned in. Every player, TVs included, can shift subtitle timing.
- Audio and subtitle defaults per profile, on every app, and your choice
  carries over to the next episode. When several tracks match your
  language, the server picks the one your device handles best: one it can
  play without conversion, or the 5.1 track if you pass audio through to a
  receiver.
- Resume, Up Next, Skip Intro and Skip Credits (from chapters, or by
  comparing the audio across episodes), seek-bar previews and chapter
  thumbnails.
- Picture in picture for videos and Live TV in the web app and on Android
  phones and tablets. On Live TV, next and previous change the channel.
- Android TV, Google TV, Fire TV, Samsung and LG TVs can play videos from
  a USB drive plugged into the TV, without a server
  ([TV_PLATFORMS.md](TV_PLATFORMS.md#videos-on-a-usb-drive)).
- Use the web or desktop app as a remote for your other devices. The
  header lists your devices that are on (playing ones first), plus devices
  on other profiles in your household that can switch to yours; you see
  which profile each is on, never what it's playing. You can play, pause,
  seek, skip 30 seconds back or forward, go to the next or previous title
  (or channel), change the audio, subtitles and quality, and stop
  playback. Skip Intro and Skip Credits show up on the remote when they
  would on the device. On Live TV you see the current program's progress
  and what's next. The device shows a short notice for each change instead
  of popping up its controls ([API.md](API.md#remote-control)).
- Play on… sends a movie, episode, show, season, artist, album, track or
  playlist to another of your devices (Shuffle on… and, for Live TV, Watch
  on… work the same way), with the audio and subtitles you picked, and
  opens the remote for it ([API.md](API.md#casting)). A device on another
  profile in your household switches to yours first; if your profile has a
  PIN, you're asked for it. The desktop app can receive casts too, as
  "Pley for Windows", "Pley for Mac" or "Pley for Linux". Browser tabs
  can't, since browsers won't start audio in a background tab.
- Cast movies and episodes to Chromecasts, Google TVs, TVs with Cast
  built in, Apple TVs and AirPlay 2 TVs on the server's network. The
  server streams to them directly, in HEVC, 4K and HDR where the device
  supports it, with your subtitles, keeps track of where you are, and the
  remote controls them like any Pley player. AirPlay devices that ask for
  a code are paired once. Off by default; administrators turn it on, and
  manage pairings, in Settings → Network ([API.md](API.md#casting)).

## Live TV

- IPTV channels from M3U playlists and Xtream Codes accounts, with XMLTV
  guides, on every app: categories, a guide, typing a channel number on
  the remote, and channels in the same search as everything else.
- Each channel is pulled from the provider once, however many people
  watch it, and Pley never opens more streams than the provider allows.
  Switching channels frees the connection right away.
- Channels your TV can play natively come straight from the provider;
  the rest go through the transcoder. Broken channels and divider rows
  can be hidden.
- HLS channels are read segment by segment, including ones that send
  audio as a separate track (AAC, or audio in MPEG-TS).

## Watching

- Progress, history, star ratings, Continue Watching and Next Up, per
  user.
- Recently Added, Recently Released, Top Rated and More Like This.
- Search covers titles, people and Live TV channels, and it's quick even
  on large libraries. It copes with typos ("the matirx", "kaenu reeves"),
  words run together or split ("spiderman"), missing apostrophes ("the
  captains log") and words in any order. Add a year to narrow it down
  ("alien 1979"). Channels can be found by name, number or what's on now.
  Exact matches come first.
- Collections (from TMDB, hand-picked, or smart) and playlists (ordered or
  smart). A movie library can skip TMDB's collections so a tool like
  Kometa can make its own.
- Collections are sorted into categories (Highlights, Box sets, Other, or
  your own) on every app. Administrators choose which collections appear
  as rows on Home and on each library's Recommended tab, and in what
  order. Kometa's group labels and visibility settings work too.
- A collection without its own poster shows a grid of its first four
  posters. Administrators can add a colored icon on top, use an icon on a
  plain color, or upload a poster.

## Requests

- Discover shows what's trending, popular, upcoming and anime on TMDB,
  and marks what you already have. People who can make requests also see
  these results in Search, after your libraries.
- People request a movie, or the missing seasons of a show, and
  administrators approve or decline them, optionally with a reason.
  Accounts can be set to auto-approve, and limited to a number of movies
  and seasons per week. Profiles without a password and accounts with an
  age limit can't request until an administrator allows it.
- Approved requests go to Radarr and Sonarr (with a separate quality
  profile and root folder for anime), and download progress shows in
  Pley. Once everything requested is in a library, the request is marked
  available and the person who asked is notified.

## Accounts

- Administrators, users and managed profiles for kids, each with their
  own libraries and age limit.
- Set an age limit by rating, from 29 countries' rating systems ("Up to
  PG-13"), by age, or by birthday, so it goes up each year until 21.
  Ratings are read in their own country's system, so a UK "PG" or a Danish
  "A" means what it should.
- A profile switcher with optional PINs, invite links, and a list of
  signed-in devices you can sign out remotely.
- PINs work the same on every app. Profiles with a PIN show a lock.
  Too many wrong tries in 15 minutes and you're told how long to wait;
  after 100 wrong tries in a row the PIN is disabled until its owner or an
  administrator sets a new one. A TV opens straight to Home on the last
  profile used, unless that profile has a PIN, in which case "Who's
  watching?" asks for it. (Apple TV asks "Who's watching?" in a household
  of several profiles, unless Always start as picks one.) On the web,
  signing in again goes straight to the last profile used if it has no
  PIN. Switching to a profile without a PIN that can see more than yours
  asks for its password in a browser; a TV signs in to it again with a
  code.
- Two-step sign-in with an authenticator app and recovery codes. It's
  optional, or an administrator can require it for everyone; people then
  set it up the next time they sign in (on their phone, for a TV).
- Apps sign in through Pley's own page and only get what you allow:
  browsing, playing, playlists and, for an administrator's Android phone,
  managing the server. Never the server's settings or keys. It's OAuth 2.1
  with client metadata documents. Pley's own apps work without internet
  access; third-party apps are blocked until an administrator allows them.
  TVs show a code to enter at Pley's `/link` page
  ([API.md](API.md#apps-oauth-21)).
- Apps and tools that use the compatibility APIs get their own tokens,
  made in Account or by signing in through the Jellyfin-compatible API.
  These only work on those APIs and only for what you allowed: watching
  and, for administrators, seeing everyone's activity or managing
  libraries, never accounts. Tokens can be restricted to certain
  addresses, and Account lists and revokes them
  ([COMPATIBILITY.md](COMPATIBILITY.md#tokens-for-other-apps)).
- First-time setup needs a one-time code from the server's log, which the
  tray icon fills in for you. Passwords need at least 8 characters and
  can't be common ones ([SECURITY.md](../SECURITY.md)).

## Administration

- Edit metadata, lock fields, fix matches and choose artwork.
- Settings has a Test button for every API key and service. There's a live
  dashboard (playback, transcoder, scans, and CPU and memory charts for
  Pley and the computer) and daily database backups
  ([STORAGE.md](STORAGE.md)).
- On an Android phone or tablet, Me → Manage server has the dashboard,
  libraries, users and invites. Server settings, network, transcoder,
  backups and Live TV stay in the web app ([ANDROID.md](ANDROID.md)). TVs
  signed in with a code can never manage the server.

## Compatibility APIs

A Plex-compatible API, on Pley's own port by default (or 32400), for
`python-plexapi`, library tools like Kometa and activity monitors like
Tautulli: browsing with their filters, editing, collections, playlists,
play queues, sessions, history and notifications.

A Jellyfin-compatible API, on Pley's own port by default (or 8096).
Jellyfin's apps, and others built on its API, can browse your libraries,
play files directly or as HLS from Pley's transcoder, with subtitles and
intro and credit skipping, and keep watch state, favorites (My List) and
ratings in sync with Pley's accounts
([COMPATIBILITY.md](COMPATIBILITY.md)).

## Offline

Everything works on your home network without internet access. You can
download a file's original from its page; the Android app (phones and
tablets) and the desktop app keep downloads to play while the server is
out of reach, and the Android app syncs what you watched once it's back.

## Performance

Single binaries. SQLite in WAL mode with a single writer. Listings are
filtered, sorted and paged in SQL. Responses are compressed. Images are
resized once and cached. Transcodes are throttled to about twice real
time.

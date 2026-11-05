# Features

What Pley does, in full. The [README](../README.md) has the short version.

## Libraries

- Movies, TV shows and music, scanned incrementally and rescanned when files
  change. An offline drive isn't mistaken for deleted media.
- Common naming schemes: `Movie (2019)`, release names, `{tmdb-123}` hints,
  editions (`{edition-Director's Cut}`), `S01E02`, `1x02`, date-based and
  anime episodes, and extras (`Trailers/` folders,
  `Movie (2019)-trailer.mkv`). Looser episode names work too: `04 - Title` or
  `104 - Title` in a `Season 1` folder, `Season 1 Episode 4`, or a title
  alone, which TMDB numbers by its title. Two files never merge into one
  episode for lack of a number.
  [deploy/docker/media](../deploy/docker/media/README.md) shows a layout.
- Each profile can order its libraries, on every app.

## Metadata

- Movies from TMDB (free API key); TV shows from TMDB, TheTVDB or TVmaze (no
  key); music from MusicBrainz and the Cover Art Archive (no key). See
  [Metadata sources](CONFIGURATION.md#metadata-sources).
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
  those that are on, the ones playing first, then those ready to play. For
  the one chosen it plays, pauses, seeks, skips, goes to the next or
  previous title (or channel, on Live TV), changes the audio track, the
  subtitles and the quality, and stops what plays there, once asked to.
  On Live TV it shows the programme's progress and what's next. Each
  player obeys the commands it can, without bringing up its controls
  there: a brief notice says what changed. Any can be stopped
  ([API.md](API.md#remote-control)).
- Play on… in the web and desktop apps, beside Play on a movie, an episode,
  a show, a season, an artist, an album, a track and a playlist (Shuffle
  on… beside Shuffle), and Watch on… for a Live TV channel: it plays there
  on another of your devices that takes casts, as that device's own Play
  would, with the audio and subtitles picked on the page, and the remote
  opens on it ([API.md](API.md#casting)). The desktop app takes casts too,
  as "Pley for Windows", "Pley for Mac" or "Pley for Linux"; a browser
  tab doesn't, as a browser won't start sound in a tab in the background.

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

## Watching

- Progress, history, star ratings, Continue Watching and Next Up per user.
- Recently Added, Recently Released, Top Rated, More Like This and full-text
  search.
- Collections (from TMDB, curated, or smart) and playlists (ordered or smart).

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
  playing, playlists), never administration: OAuth 2.1 with client metadata
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

## Compatibility APIs

The Plex-compatible API, on Pley's own port by default (or 32400), for
`python-plexapi`, library tools such as Kometa and activity monitors such
as Tautulli: browsing with their filters, editing, collections, playlists,
play queues, sessions, history and notifications.

The Jellyfin-compatible API, off until it is turned on, on Pley's own port
or one of its own (8096):
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

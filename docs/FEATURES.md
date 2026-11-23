# Features

Everything Pley does. The [README](../README.md) has the short version.

## Libraries

- Movies, TV shows, music and audiobooks, scanned incrementally as files
  change. A drive that goes offline isn't treated as deleted.
- The usual naming schemes: `Movie (2019)`, release names, `{tmdb-123}`,
  editions, `S01E02`, `1x02`, date-based and anime episodes, extras, and
  looser episode names like `04 - Title` in a `Season 1` folder
  ([example layout](../deploy/docker/media/README.md)).
- Copies of a movie matched to the same TMDB title become one movie with
  several versions, even when their file names differ. Fix Match splits
  them again.
- `.strm` files, as in Kodi, Jellyfin and Emby. Off by default
  ([.strm files](CONFIGURATION.md#strm-files)).
- Filter movies and shows by resolution and HDR type, on every app.
- Each profile can reorder its libraries.
- Import from Plex: libraries, Home users, labels, dates added and watch
  history ([MOVING_FROM_PLEX.md](MOVING_FROM_PLEX.md)).

### Audiobooks

In the web and desktop apps only, for now.

- Books are found the way Audiobookshelf finds them: a folder of audio
  files, an `.m4b`, or several `.m4b` files of one book, in
  `Author/Series/Book` layouts.
- Titles, authors, narrators and series come from tags first, then folder
  names, then Open Library and Audnexus.
- `.m4b` chapters, resume across parts, and Continue Listening on Home.
- Skip back 15 s and forward 30 s, 0.75× to 3× speed, and a sleep timer.

## Metadata

- Movies from TMDB (free API key), TV from TMDB, TheTVDB or TVmaze, music
  from MusicBrainz, audiobooks from Open Library and Audnexus
  ([Metadata sources](CONFIGURATION.md#metadata-sources)).
- NFO files from Kodi, Jellyfin, Radarr, Sonarr and others decide the
  match and override online sources
  ([NFO files](CONFIGURATION.md#nfo-files)).
- Local artwork and your own edits always win.

## Playback

- Direct play, remux or transcode, chosen per device, with the reason
  shown ([QUALITY.md](QUALITY.md)).
- Automatic quality adjusts to your connection while you watch, stepping
  down when playback stalls and back up when it recovers.
- Hardware transcoding (NVENC, Quick Sync, AMF, VAAPI, VideoToolbox) and
  GPU tone mapping for HDR10, HLG and Dolby Vision.
- Image subtitles (PGS, VobSub, DVB) drawn by the apps instead of burned
  in, and subtitle timing adjustment on every player.
- Audio and subtitle defaults per profile, carried over to the next
  episode.
- Resume, Up Next, Skip Intro, Skip Credits and seek-bar previews.
- Picture in picture in the web app and on Android phones and tablets.
- Android TV, Fire TV, Samsung and LG play videos from a USB drive, with no
  server ([TV_PLATFORMS.md](TV_PLATFORMS.md#videos-on-a-usb-drive)).

## Remote and casting

- Control what plays on your other devices from the web or desktop app:
  play, pause, seek, skip, change tracks and quality, or stop
  ([API.md](API.md#remote-control)).
- Play on… sends a title, music, a playlist or a Live TV channel to another
  device ([API.md](API.md#casting)).
- Cast to Chromecast, Google TV, Apple TV and AirPlay 2 TVs, in 4K and HDR
  where supported. Off by default; turn it on in Settings → Network.
- [Pley for Home Assistant](https://github.com/pley-media-server/pley-homeassistant)
  signs in with a code, shows what's playing, controls it, and sends
  titles to your TVs.

## Live TV

- IPTV from M3U playlists and Xtream Codes, with XMLTV guides, on every
  app. A channel its source's guide lacks can take a guide of its own by
  its address, such as epg.pw's, or a country's from IPTV-EPG.org with the
  channel named after `#`.
- One connection per channel however many people watch, within the
  provider's limit. A channel several sources carry can play on another's
  connection when its own source's are all in use.
- Channels play straight from the provider when the device can, through
  the transcoder when it can't.
- Groups in the provider's order, or A to Z for a provider that lists them
  in none.

## Watching

- Progress, history, ratings, Continue Watching and Next Up, per user.
- Recently Added, Recently Released, Top Rated and More Like This.
- Search across titles, people and channels that handles typos and words
  in any order.
- Collections from TMDB, hand-picked or smart, grouped into categories,
  with administrators choosing which appear on Home.
- Playlists, ordered or smart.

## Requests

- Discover what's trending, popular and upcoming on TMDB.
- Request movies or missing seasons; administrators approve or decline,
  or let some accounts auto-approve, with weekly limits.
- Approved requests go to Radarr and Sonarr, with download progress and a
  notification once it's ready.

## Accounts

- Administrators, users and managed profiles for kids, each with their
  own libraries.
- Age limits by rating (29 countries), by age, or by birthday.
- Profile PINs, invite links, and remote sign-out of devices.
- Two-step sign-in, optional or required for everyone.
- Apps sign in through Pley and only get what you allow; TVs sign in with
  a code ([API.md](API.md#apps-oauth-21)).
- Separate tokens for tools that use the compatibility APIs
  ([COMPATIBILITY.md](COMPATIBILITY.md#tokens-for-other-apps)).

## Administration

- Edit metadata, lock fields, fix matches and choose artwork.
- A live dashboard (the processors, memory and network over the last five
  minutes, and who Pley is sending to how fast), a Test button for every
  service, and daily database backups ([STORAGE.md](STORAGE.md)).
- Manage libraries, users and invites from an Android phone
  ([ANDROID.md](ANDROID.md)).

## Compatibility APIs

- A Plex-compatible API for tools like Kometa, Tautulli and
  `python-plexapi`.
- A Jellyfin-compatible API for Jellyfin clients, with playback, subtitles,
  skip intro and watch state ([COMPATIBILITY.md](COMPATIBILITY.md)).

## Offline

- Everything works on your home network without internet access.
- The Android and desktop apps download files to play while away.

## Performance

Single binaries, SQLite in WAL mode, listings filtered and paged in SQL,
compressed responses, images resized once and cached, and transcodes
throttled to about twice real time.

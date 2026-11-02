# Changelog

Notable changes to Pley Media Server: the server, its transcoder and the web app it
embeds, with the Docker image and the Windows installer, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `release-<date>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

### Added

- A playback with keyed addresses has its file's PGS subtitles beside its
  media as pictures, each placed and timed (`subtitles-{index}.json` and its
  PNGs), for a player that can't decode PGS: Roku draws them over the video
  rather than have them burned in.

### Changed

- Seek previews, fingerprints and other background work pause while the
  picture subtitles of a film being played are read, so on a NAS those come
  sooner while seek previews are being made.
- Picture subtitles (PGS, VobSub, DVB) that the apps draw show within a few
  seconds of a playback starting, where they could take up to a minute:
  the transcoder reads those around where the playback is out of a
  Matroska file first, at once, and the rest of the track after the
  playback's start. A `.mks` asked for with `at` (where the player is)
  starts there, and a seek while the rest is read has the pictures where
  the player went read at once. Kodi's `.sup` has everything read by the
  time it opens, not only the pictures around the start.

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

[Unreleased]: https://github.com/pley-media-server/pley/compare/release-2026-11-02...HEAD
[0.1.3]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-02
[0.1.2]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-01.2
[0.1.1]: https://github.com/pley-media-server/pley/releases/tag/release-2026-11-01
[0.1.0]: https://github.com/pley-media-server/pley/tree/14792b986c3f69d55bd07b39e84716da0e8e1645
[0.0.1]: https://github.com/pley-media-server/pley/tree/7e14bb9d498d054a73b2176cd8949b6864ffacd9

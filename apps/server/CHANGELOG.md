# Changelog

Notable changes to Pley Media Server: the server, its transcoder and the web app it
embeds, with the Docker image and the Windows installer, newest first.
The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Releases are tagged `server-v<version>`; [docs/RELEASING.md](../../docs/RELEASING.md)
describes how they are made.

## [Unreleased]

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
  guide channel) and who is watching what. Channels are browsed by group,
  as TiviMate does: groups named by region ("UK| Sport") fold under it,
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

[Unreleased]: https://github.com/pley-media-server/pley/compare/server-v0.0.1...HEAD
[0.0.1]: https://github.com/pley-media-server/pley/releases/tag/server-v0.0.1

# Pley for LG TV

A native app for LG TVs with webOS 4.0 and later (sets from 2018 on), in
`apps/webos` (paths below are relative to it). It follows the
[Android TV app](ANDROID.md) screen for screen: the same 960×540 layout,
D-pad focus, screens and player features. It is written in Rust, draws
everything itself, and hands each video and song to the TV's own media
pipeline, with its hardware decoders, HDR, Dolby Vision and sound output.
LG's Content Store takes no native apps, so you install it in Developer
Mode or on a rooted TV. Over the network, the app talks only to the Pley
server.

- [Install](#install)
- [Using it](#using-it)
- [How it works](#how-it-works)
- [What an LG TV plays](#what-an-lg-tv-plays)
- [Data and logs](#data-and-logs)
- [Building and testing](#building-and-testing)
- [Releases](#releases)
- [Known limits](#known-limits)

## Install

Use the package attached to an LG release (`pley-lg-<version>.ipk`), or
build one ([The TV build](#the-tv-build)). Its id is `app.pley.tv`, the
earlier LG web app's, so it installs over it. What the web app kept
(servers, sign-ins, settings) stays with the TV's web engine: choose the
server and sign in again.

### In Developer Mode

Any LG TV can install it this way.

1. Install **Developer Mode** from LG's Content Store and sign in with an
   LG developer account (free). Turn Dev Mode Status on (the TV restarts),
   then Key Server on, and note the TV's address and the passphrase it
   shows.
2. Install the package with LG's CLI (`npm install -g @webos-tools/cli`,
   Node.js 20 or newer):

   ```sh
   ares-setup-device --add tv -i "host=<TV address>" -i "port=9922" -i "username=prisoner"
   ares-novacom --device tv --getkey      # asks for the passphrase
   ares-install -d tv pley-lg-<version>.ipk
   ares-launch -d tv app.pley.tv
   ```

   [webOS Dev Manager](https://github.com/webosbrew/dev-manager-desktop)
   (Windows, macOS, Linux) installs the same package from a window.
3. A Developer Mode session lasts at most 1,000 hours. Extend it in the
   Developer Mode app (or Dev Manager) before it runs out: when Developer
   Mode turns off, the TV uninstalls the apps it installed. Ten restarts
   without a network connection, or the same LG account signed in on
   another TV, end it too.

Installing again replaces the app and keeps its data.

### On a rooted TV

A TV rooted with webosbrew's tools, with the Homebrew Channel, has no
session to renew. Install the same package over its SSH, as `root` on
port 22 (`ares-setup-device` with `-i "port=22" -i "username=root"`), or
with Dev Manager.

## Using it

Sign-in, server discovery (a broadcast, then a sweep of the TV's subnet),
the screens, the player's keys and panels, music and offline behaviour
follow the Android TV app. The TV shows a code to type on the server's
`/link` page (or a QR code to it), and asks "Who's watching?" at start
when the household has more than one profile. With Always start as, a
start never goes on as a profile with a PIN without it: the TV keeps the
profile it was left on (`GET /api/auth/me`, with `has_pin`), opens on its
PIN pad when it has one, and until it knows, asks "Who's watching?". What
differs on LG:

### Remote and Magic Remote

| Key | What it does |
|---|---|
| Back | Closes what the page has open, then goes back. On a first page it opens the rail; in the rail (or on a first page without it) it leaves the app as LG's [Back-button guide](https://webostv.developer.lge.com/develop/guides/back-button) says. From webOS 6 it asks "Exit Pley?", as the Samsung app does: Exit (focused) ends playback, tells the server and closes the app; Cancel or Back stays. On webOS 4 and 5 the TV's Home shows, and Pley waits behind it where it was |
| Exit | Closes the app |
| OK held, or the menu key | A title's actions, in a side panel over the page as in the Android TV app: a choice closes it, focus goes back to the tile, then it runs. In the guide, a programme's details |
| CH+, CH− | The next or previous channel in the Live TV player, as Next and Previous do there; a page of channels in the guide |
| Digits | A channel's number in Live TV; the address and PIN pads |
| Red, green, yellow, blue | Nothing, as on the Android TV app |

- **The Magic Remote** works as a pointer: what it points at takes focus
  without the page moving, and a tile cut off at the edge comes into
  view; a click is OK; the wheel moves a row a notch. Pointing at the rail
  opens it, unless a panel is open. A click on the player's bar seeks
  there. The TV hides the pointer after a while; any key goes back to the
  remote's focus.
- **A held arrow** moves focus by itself once held 180 ms, then every
  45 ms on the app's own clock, as in the Samsung app: sooner and more
  evenly than the TV's repeats, which only say the key is still down.
  Rows, pages, lists and grids glide evenly under it, each move running
  into the next, and a tile grows and shrinks with focus at the same
  pace, as in the Samsung app; on Low-tier TVs the glide runs too, and a
  tile losing focus snaps. A row
  slides under one ring that stays still at its start, the tiles keeping
  their resting look, and near its end, where tiles stop coming to the
  start, focus steps across them. In a grid of posters (a library,
  Collections, playlists, a collection's page) one ring stays still on
  the focused line while the rows slide under it. The pictures of what
  goes by load once the key is let go. With Reduce motion everything
  steps. Backdrops and details follow focus once it has rested 400 ms.
- **A held arrow stops at the end of a list**, where a press goes on, as
  in the Samsung app: a held Left stops at the start of a row, list or
  grid and only a press opens the rail; in a library's grid a held Up or
  Right stays in the grid (a press reaches the chips or the A–Z jump), as
  does a held Up at the top of the Collections tab and of Live TV's
  categories, guide and channel cards. Right at a row's end and Down on a
  grid's last row always stay.
- **In the player** the keys follow the Samsung app's. With the controls
  hidden, OK and Up show them on the scrubber and Down on Play/Pause.
  Left and Right show them and step 10 s, and a held key scrubs faster
  the longer it is held (from 20×, doubling every 1.5 s, up to a whole
  video in 10 s). The seek waits for OK, going by itself 5 s after the
  last Left or Right (the Play/Pause key only plays or pauses), and Back lets it
  go. ◀◀ and ▶▶ (30 s) seek 400 ms after the last press. The control row
  has no Back 10 or Forward 10, as in the Android TV app: Left and Right
  seek. Up on the scrubber while a seek waits, or the Chapters button,
  opens the chapters as a row of cards over it: Left and Right go
  through them, OK goes to one, Down or Back closes them. As each item
  begins playing, its title shows at the top for 4 s, unless Playback
  details show there (Settings → Playback). Each stream's first picture
  fades in from black over 200 ms, at once with Reduce motion. The media
  keys act while a video starts, before its first picture, as in the
  Android TV app; while an error shows, Stop is the only one that acts.
- **Typing** uses the app's own keyboard and pads (a search, a server's
  address, a PIN); the TV's keyboard doesn't open.
- **On webOS 4**, Home opens as a ribbon over the app, which keeps
  running underneath.
- **The screen dims** after 2 minutes without a key, outside playback:
  the TV never blanks a native app that is showing.

### Audio Guidance

With the TV's Audio Guidance on (Settings → Accessibility), the app reads
in the TV's own voice (`com.webos.service.tts`): a page's name as it
opens, then what has focus (a button's label, a title and its second
line, a row with what supports it, a field with what it holds), a dialog
or the ready-to-watch notice as it opens, and toasts. Each control names
itself as it draws (`Ui::name`); a new reading cuts off the one before,
as the TV's menus do.

### Audio and subtitles

- The server picks the audio track that this TV plays best in the
  profile's language, unless you pick one on the title's page or in the
  player. Picks are saved for the show or movie; subtitles picked alone
  are saved without the audio.
- **Settings → Profile** sets the defaults (Audio language, Subtitles,
  Subtitle language).
- **Settings → Audio:** the TV passes Dolby Digital, Dolby Digital Plus
  and Atmos to a receiver as its own sound settings say; Pass through
  Dolby TrueHD and Pass through DTS are for a receiver that takes them,
  and otherwise the server converts those.
- **Pley draws every subtitle itself**, over the picture: text tracks come
  from the server as WebVTT, and picture subtitles (PGS, VobSub, DVB) are
  decoded from the server's `.mks` copy of the track, so the file still
  plays as it is. As in the Samsung app, every ask for a picture track
  says where the player is (`at`, 0 at the start too), and while the
  server is still reading the track out of its file it answers with the
  pictures around there. It is asked again 3 s after the ask, then twice
  as long each time up to 15 s, never sooner than 3 s after the answer; an
  answer over 4 MB, a copy of the whole track so far, is asked for again
  up to 24 s after it came. A seek asks at once from where it went and
  drops the answer on its way. No ask is for part of a file (`Range`).
- **Subtitle sync:** 0.1 s steps, up to 10 s either way, kept per file
  and profile on the TV (the latest 300).
- **Settings → Subtitles and display:** size, background, font and
  position (raised 5, 10 or 15 % of the height). A font is offered only
  if the TV has a face for it; text in other scripts uses the TV's own
  fonts. Picture subtitles keep their own position.
- **Find subtitles…** searches OpenSubtitles in the profile's subtitle
  language.
- **Speed:** 0.5× to 2×, with the sound, through the pipeline's play
  rate.
- **Info** says, under How it plays, the server's Speed and beside it
  the TV's network speed: the fastest second of the last 10 s of what the
  TV's network interfaces received (`/proc/net/dev`, read at most once a
  second while Info shows), as in the Samsung app, since LG's pipeline
  doesn't say how fast its own downloads go. A stalling video whose server
  keeps up has a network that can't carry it. Where the jail hides the
  file, the row isn't shown.

### Quality

The menu, Original and Automatic follow [QUALITY.md](QUALITY.md#the-menu),
in the player's Quality panel and Settings → Playback → Quality.
Automatic is the default until a quality is picked on this TV.

LG's player goes black between streams, so Automatic follows the
[rules for such players](QUALITY.md#players-that-go-black-between-streams)
(`crates/player/src/autoquality.rs`). A transcode comes in one size (the
app doesn't send `hls.variants`).

- **Signals:** a wait is the pipeline buffering while playing (not a
  pause, seek or new session). Delivery and network speed come from how
  fast the pipeline's buffer fills. The pipeline counts no dropped frames.
- **Decoding:** an original that fails falls back once to the server's
  quality.
- **Bitrate changed in place:** a new stream of the same quality
  (recovery, a track change) starts at the bitrate the last one reached.
- **Memory:** 30 minutes per server on the TV, used when the server
  doesn't answer.
- **Speed test:** 32 MiB read for 2 s from its first byte; not repeated
  within 10 minutes.

**Switching streams** (a quality picked, an audio track the pipeline
can't switch, burned-in picture subtitles, Automatic's step from the
original; `crates/player/src/prepare.rs`): the new session starts beside
the old one (`keep_previous`). The app waits up to 20 s for its playlist
and the segment at start + 4 s (a `HEAD` request: the TV's pipeline
fetches the media itself), then moves the pipeline there. The screen is
black meanwhile. With no transcode free for two (`503`), the old session
ends first.

### Live TV

Live TV works as in the [Android app](ANDROID.md#live-tv): the rail shows
it after Home only where `GET /api/live` says `available`, and it comes
and goes with `live_channels_changed`. On LG:

- **Tuning:** `POST …/watch` with this TV's id (`pley.installId`,
  `webos-<32 hex digits>`) and `keyed_urls`, so the pipeline can play the
  playlist without the token. As in the Samsung app, the channel's MPEG-TS
  (`stream.ts`) is read at once beside it, under the TV's id with
  `-stream` after it, while the server is asked what the channel is made
  into (`GET /api/live/sessions/{id}/plan`).
- **The channel's own stream:** a channel the server sends as it is, or
  whose only change is sound the TV decodes itself (AC-3, E-AC-3, MP2),
  plays from its `stream.ts`, from its first keyframe rather than after the
  first HLS segments. The pipeline can't send the token, so the app reads
  the stream with it and hands it on from the TV's loopback address
  (`http://127.0.0.1:<port>/relay/<128 random bits>/stream.ts`): only that
  one stream, to at most four players (a place taken only by a request
  that names it), closed when the channel changes or the player goes
  (`crates/player/src/live/relay.rs`). LG's pipeline
  takes no buffering setting from an app, so the stream starts as the TV
  buffers it (the Samsung app asks for 0.5 s, and 1.5 s after a stall).
- **The server's HLS** otherwise: the app waits for the playlist (up to
  45 s), then plays it from its live edge. So does a channel whose
  `stream.ts` fails or ends before it plays, or doesn't play within 10 s,
  at once; one that breaks off later does once the server has said
  watching didn't end there. Where the app can't listen on the loopback
  address, every channel plays its HLS.
- **The guide:** ◀◀ and ▶▶, Previous and Next, move it two hours back
  or on, a press at a time, as in the Android TV app. While the guide has
  focus these keys are its own even when music plays; elsewhere on the
  page they go to the music.
- **The player:** Up and Down, CH+ and CH−, Next and Previous change
  channel within the category; digits tune a number. OK, the menu or the
  guide key opens the channel list beside the picture, which moves aside
  to make room; holding OK shows the controls, as in the Samsung app.
- **Leaving:** `DELETE /api/live/sessions/{id}` on Back, on each change
  of channel, and when the TV hides the app.
- **Failures:** a player failure is retried twice (after 1 s, then 3 s).
  The pipeline says nothing of a playlist gone (`410`), so a failure, or a
  stall of 5 s, asks the server why watching ended
  (`GET /api/live/sessions/{id}/plan`) and shows its words (not a stall
  while the `stream.ts` plays: its HLS session, unread, would say watching
  ended); a stall of 20 s is a failure.
- **Taken over:** the server's `live_taken_over` event for the channel
  playing stops the player at once, before its stream breaks off, so it
  isn't tuned again into a connection that is gone. The card says "Live TV
  stopped" with the server's words, and Watch again in place of Try again.
  The card stays when the app is hidden and shown again; nothing is tuned
  until Watch again.

### Music

Music plays through the same pipeline and keeps playing while you browse,
with Now Playing in the rail. A song plays as it is or, where the TV
can't, as an audio-only stream. While music is paused the app still
reports every 30 s: the server forgets a player it hasn't heard from in
90 s, and its keyed addresses with it. Music pauses when the TV hides the
app and, if it was playing, plays on from there when the app is back. The
remote's media keys work it from every page but a player's: play, pause
and Stop (which pauses), play/pause, next, previous, and ◀◀ and ▶▶ 10 s.

### Requests

Where the server takes requests, they work as in the Samsung app
([TV_PLATFORMS.md](TV_PLATFORMS.md#requests)): Discover after Home in the
rail for a profile that may ask, else Your requests (Requests for
administrators), with its number; a title's page with its one main
action and the season picker; Your requests with each request's stage
and what may be done to it; an "On TMDB" row in Search; and a library
title's request on its page, with Request more seasons. What is ready to
watch pops up once, over the pages, never over a video. The words and
rules are `crates/core/src/requests`, the pages `crates/screens/requests`.

Sign-in asks for `requests` and `approvals` only where the server lists
them (`scopes_supported`): a server that doesn't know a scope refuses the
whole sign-in. A TV signed in before is asked to sign in again to ask
for titles.

### Videos on a USB drive

As in the Samsung app
([TV_PLATFORMS.md](TV_PLATFORMS.md#videos-on-a-usb-drive)), while a drive
the app can read is plugged into the TV, the rail ends with **USB drive**.
Its page lists the drive's folders, then the videos the TV's player opens
(MKV, MP4, M4V, MOV, TS, M2TS, MTS, AVI, MPEG; LG's pipeline lists no
WebM), sorted as a person sorts names; hidden files, what Windows and macOS
leave on a drive, and other files aren't shown. A folder opens over the one
before, with ".." first to go back up. The page's title is the drive's make
and model as the disk reports them under `/sys/block/<disk>/device`
("Kingston DataTraveler 3.0"), or USB drive when it says nothing or the
jail hides it; the line under the title is the folder's path
("/Films/2019"), at the drive's top "/" and how full the drive is. A video
started before says where it resumes, with a bar of how far through it is.
A page with nothing to list (an empty folder, no drive, a folder that
can't be read) says why in the middle, under a folder, USB or alert icon,
below ".." in a folder under the drive's top, which keeps focus. Where
not even ".." shows (no drive, or the drive's top), the message keeps
focus, unmarked, so the rail closes; Left opens it.
Nothing leaves the rail to make room for the drive's entry: with more
entries than its nine places, those between the avatar and Settings
scroll, keeping the focused entry fourth from the top as on Android TV.
The Magic Remote moves them only as far as brings a cut-off entry in
whole, as it does a tile.

webOS mounts a drive at `/tmp/usb/<disk>/<partition>`, and the letters
change with what is plugged in, so the app looks for the first drive in
its own mount table (`/proc/self/mountinfo`) and follows drives as they
come and go (`crates/webos/src/drives.rs`). The app runs in LG's jail: a
drive the TV mounted that the jail doesn't show to the app gets no entry
in the rail. A link on the drive is followed only while it leads somewhere
on the drive; one leading off it isn't listed, played or read.

A video plays in Pley's player as it is, with no server
(`crates/local`), so seeking, the panels, the sleep timer and the
subtitles' look are the same; what only a server does isn't there:
converting, Quality and Automatic, seek previews (the TV can't make
them), the next episode, and Find subtitles.

- **Tracks, chapters and HDR** come from the file itself, as on Samsung:
  an MKV's head, an MP4's movie box, and the first HEVC frames. The audio
  it starts with is the default track the TV decodes, else the first it
  decodes; TrueHD only when Settings sends it to a receiver, DTS where the
  table below says. A switch of audio track never reloads the file.
- **What the screen shows** follows LG's rules, and Playback details says
  it as the server does for a file played as it is. Dolby Vision shows as
  Dolby Vision where the TV takes it from the file's container (MP4 and TS
  always, MKV from webOS 25); otherwise the TV shows its HDR10 base layer
  (profile 7 too, since nothing converts it), and a profile 5 file, which
  has none, shows with wrong colours. LG shows no HDR10+: such a file
  shows as HDR10.
- **Subtitles**: the `.srt` and `.vtt` files beside the video (UTF-16 or
  UTF-32 by its byte-order mark, as Windows saves "Unicode" text, UTF-16
  without one by its NUL bytes, else UTF-8, else Windows' Western code
  page, Windows-1252), and an MKV's own tracks read through its Cues, text
  as WebVTT and pictures handed to the app's decoders as they are; a track
  still being read is asked for again after 1 s, then 2, then every 4 s,
  and at once after a seek.
- **Where it stopped** is kept on the TV, by the file's path on the drive
  and its size, so a drive mounted under another letter still resumes;
  past 95 % it was watched. Delete all local data forgets it too.
- **The TV's pipeline** opens the file at its `file://` address. A file
  whose picture never starts says so after 12 s, a file the TV refuses
  says that there is no server to convert it, and a file that can no
  longer be read says "This video can't be read. The drive may have been
  pulled out.", with Try again, as in the Android app.
- **The log** never names a drive's folders or files: everything after
  `/tmp/usb/<disk>/<partition>/` up to the extension is left out.

## How it works

- **One binary for every webOS from 4.0.** It is 32-bit ARMv7 code tuned
  for the Cortex-A9, which every TV's CPU runs, with the floating-point
  unit on and webOS's soft-float calling convention. It is built against
  the toolchain's glibc 2.12, so it loads on every firmware, and links
  only the TV's C library (glibc's libraries and `libgcc_s`). SDL2, EGL
  and GLES, Wayland, luna-service2, GLib and LG's media libraries are the
  TV's own: the app opens them at run time with `dlopen`, so one missing
  on some firmware can't stop it before it starts, and the package ships
  none of them.
- **Drawing.** A window from the TV's SDL2 on its 1920×1080 interface
  plane, with a GLES 2 context. The app lays out the Android TV app's
  960×540 dp canvas and draws it ×2 with its own renderer, pictures
  decoded off the main thread and text shaped by the app itself (Roboto
  in the package; the TV's fonts in `/usr/share/fonts` for other
  scripts). It draws only when something changes, and the first frame
  comes from the saved Home before any network.
- **The picture.** The TV's pipeline draws the video on its own plane
  under the interface, which is transparent where the picture shows. On
  webOS 5 and later the app exports a window for it and names it in the
  load; on webOS 4 it tries the ways the TV may bind the picture to the
  app, in order, and remembers the one that works for the firmware's
  build (`video-plane.json` in the data folder).
- **Playing.** By address, through uMediaServer
  (`luna://com.webos.media`), the pipeline the web app's `<video>` used;
  LG documents it only for webOS OSE. The pipeline takes only an address,
  no request headers, so the app plays from
  [keyed addresses](API.md#keyed-urls), and a live channel's `stream.ts`
  from the app's own relay on the loopback address. The start position
  goes in the load (`option.transmission.playTime.start`), since a seek
  after loading buffers from the beginning first; a live channel gets
  none and starts at its live edge. An HLS stream declares the most it
  needs (`adaptiveStreaming`): H.264 (and a transcode) Full HD when it fits
  1920×1088 and 4K otherwise, at its frame rate's class (24, 25, 30, 50,
  60); HEVC, VP9 and AV1 4K at 60, or 8K. The TV's codec table caps each,
  because from webOS 25 the pipeline reserves its decoder for what is
  declared, and refuses an H.264 stream declared at 4K 60.
- **The TV's services** (settings, configuration, Home, the screen saver)
  go through luna-service2 on a thread of the app's own, with one
  anonymous registration, every call bounded by a timeout. The main thread
  never waits on luna, the network or the disk.
- **Coming and going.** When the TV hides the app, a video stops where it
  was and, when the app shows, comes back there paused, with the controls
  showing, as in the Samsung and Android apps; music pauses and plays
  on when the app shows, if it was playing; the server's event stream
  stops until the app is back. When the TV takes a
  paused video's decoder back (as when its screen saver comes on), play
  or a seek reloads it where it was.
- **The network.** The app's own HTTP/1.1 client, with TLS through rustls
  and its own root certificates (a TV's own are as old as its firmware),
  and connections kept alive. It follows no redirect by itself, so the
  bearer token goes only to the server's own origin, hop by hop. It says
  `pleyLG/<version> (webOS <n>; <model>)` as its User-Agent, which the
  server shows for its sign-in ([API.md](API.md#apps-oauth-21)).
- **Telling the server apart.** Before a token goes to the server's
  address, the server proves there that it is the one the TV signed in
  to, with the key of the sign-in that its token answers carry
  ([API.md](API.md#telling-servers-apart)). An access token waits for a
  proof made on this network within the half hour; a refresh token, a
  profile's PIN and a sign-out wait for one made then. The app showing
  again, the TV's connection changing (a link, its address or gateway, or
  the Wi-Fi network, as the connection manager tells them) and a request
  that gets no answer each ask for a new proof. What can't prove itself is
  the server out of reach: the TV says "Another device answers there."
  and sends it nothing, and signing out then forgets the sign-in on the TV
  only (the server's Account → Apps ends it there). A sign-in from a server
  without proofs is checked from its next refresh, when it gets its key.
- **Signing in** uses a code (OAuth's device grant) with the client
  `https://pley-media-server.github.io/pley/oauth/webos.json`, built into
  the server, asking for `library`, `play`, `playlists` and `profiles`,
  and `requests` and `approvals` where the server lists them (those the
  server offers).
- **The package** is `app.pley.tv_<version>_arm.ipk`, with
  `"type": "native"` in `appinfo.json`. `requiredPermissions` and
  `requiredACG` there list the system services the app uses:
  `systemconfig.query`, `settings.query`, `arc.query` and
  `network.query`, and `tts.operation` for Audio Guidance's voice.

The code, by crate:

| Folder | What |
|---|---|
| `crates/base` | Geometry, colours, input, the display list: what the crates share |
| `crates/text` | Fonts, shaping, right-to-left text, line breaking, glyphs |
| `crates/gfx` | The GLES 2 renderer, its atlas, icons, the picture cache, screenshots |
| `crates/platform` | The window, GL and input as the app sees them; the simulator's window |
| `crates/core` | The server's API, HTTP, sign-in, discovery, events, saved data, formatting |
| `crates/ui`, `crates/kit` | Focus, keys, the pointer, animation and the theme; the controls built on them |
| `crates/subs` | WebVTT, subtitle timing and style; the PGS, VobSub and DVB decoders |
| `crates/local` | Videos on a USB drive: folders, MKV and MP4 heads, subtitle files and tracks, resume points, and the player's server for a file |
| `crates/player` | Playback: the controller, Automatic, recovery, Up Next, music, Live TV sessions |
| `crates/webos` | The TV: SDL2, luna-service2, its facts, the capability report, the uMediaServer engine, the data folder, the log |
| `crates/shell` | Routes, the back stack, the rail, the app's state |
| `crates/screens/*` | The screens: onboarding, Home, browsing, a title, search, music, settings, Live TV, the player |
| `crates/screens/storage` | The USB drive's page: a folder's folders and videos |
| `crates/host`, `crates/app` | The frame loop and the command line; the binary `pley` (package `pley-lg`) |
| `crates/fixtures` | A fake Pley server for the simulator and the tests, and a sample USB drive |
| `xtask` | `cargo xtask`: the TV build, its checks, the notices and the package |

## What an LG TV plays

From LG's specifications, the TVs' own reports and what other players
found on real sets:

| Year | webOS | Notes |
|---|---|---|
| 2018 | 4.0 | DTS decoded; VP9 Profile 0 up to 1080p; HLS with TS segments |
| 2019 | 4.5 | Same |
| 2020 | 5 | AV1 and VP9 at the panel's size; fMP4 in HLS; the picture goes in an exported window; no DTS |
| 2021 | 6 | |
| 2022 | 22 | |
| 2023 | 23 | DTS on some models; LG's HLS player sends surround as stereo |
| 2024 | 24 | Same |
| 2025 | 25 | Dolby Vision from MKV; stereo bug fixed; decoders reserved for what a stream declares; no DTS |
| 2026 | 26 | Stereo bug again |

Some 2022 and later sets get newer webOS versions (webOS Re:New), so the
app goes by the running version, never the model year. The TV calls its
releases 4 to 11 (7 is webOS 22, 11 is webOS 26).

- **Containers:** MKV, MP4 (and M4V, MOV), TS, AVI and MPEG, all from an
  address; for music also MP3, M4A, AAC, FLAC, Ogg, WAV and MKA.
- **Video:** H.264 to 1080p at 60 and, on 4K sets, to 2160p (at 60 on
  webOS 4.0, 30 later). HEVC Main and Main 10 to the panel's size (8K on
  8K sets) at 60. VP9 (Profiles 0 and 2) and AV1 to the panel's size from
  webOS 5. MPEG-2, and MPEG-4 Part 2 at 1080p.
- **HDR:** HDR10 and HLG where the TV says it shows HDR
  (`tv.model.supportHDR`). Dolby Vision profiles 5 and 8 where it says it
  shows Dolby Vision (`tv.config.supportDolbyHDRContents`); profile 7
  plays its HDR10 base layer. Before webOS 25 only from MP4 and TS, so a
  Dolby Vision MKV is requested as a remux (`crates/player/src/plan.rs`).
  **No LG TV shows HDR10+.**
- **Sound:** AAC, AC-3, E-AC-3, MP3, MP2, FLAC, Vorbis, Opus and PCM. AC-3
  and E-AC-3 (Atmos included) pass through over ARC, eARC or optical when
  the digital output isn't PCM; apps can read these settings but not
  change them. TrueHD isn't known to reach a receiver from any app, so it
  is sent only when Settings → Audio asks. **DTS:** decoded on 2018–2019
  sets, passed through on some 2023–2024 sets whose receiver takes it
  (and wherever Settings → Audio asks), otherwise none.
- **Surround in LG's HLS player:** on webOS 23, 24 and 26 it reaches a
  receiver as stereo. Files played as they are aren't affected.
- **Streams:** HLS from a master playlist, in one size per stream: the
  pipeline may go black moving between sizes, as between streams.
- **Subtitles:** the app draws them all, so the TV gets none.

Pley's capability report (`crates/webos/src/caps.rs`,
[version 2](API.md#version-2)) comes from:

- the webOS version, read from the TV's platform version
  (`tv.nyx.platformVersion`, else `/var/run/nyx/os_info.json`);
- the TV's configuration (`com.webos.service.config`): the panel, OLED,
  HDR, Dolby Vision, Atmos, memory, and the receiver's formats
  (`tv.model.edidType`);
- the sound settings (`com.webos.settingsservice`) and the ARC state
  (`com.webos.service.arccontroller`);
- the panel's resolution, from LG's SDL.

For the same facts it is, byte for byte, the report of the earlier web
app, whose snapshots are its tests. The report rates the TV too: low for
webOS 4 and sets with less than 1.5 GB of memory, high from webOS 22, mid
otherwise; the app keeps 64, 128 or 192 MB of pictures on the GPU to
match. Settings → Diagnostics shows the report's lines, with the
pipeline's own codec table
(`/etc/umediaserver/device_codec_capability_config.json`) when the TV
lets the app read it.

## Data and logs

**Data.** No folder is writable on every TV, so the app takes the first
of these it can use:

| Folder | Where |
|---|---|
| `/media/developer/app.pley.tv-data` | Developer Mode |
| `/media/internal/.app.pley.tv-data` | A rooted TV's Homebrew Channel |
| `/media/developer/temp/app.pley.tv-data` | Some older sets |
| `/tmp/app.pley.tv/data` | Anywhere, but a restart empties it: the app then says it can't keep the sign-in |

Never the app's own folder, which every install replaces. The folder is
the app's alone (0700, files 0600), and each file is written whole (a
temporary file, flushed, then renamed over the old one). It holds `store/`
(the servers, sign-ins, this TV's id, settings, subtitle sync, Automatic's
memory, Live TV's view, the household's profiles, library views, recent
searches, where each video on a USB drive stopped), `snapshots/` (what
screens last showed, per server and profile, within 25 MB),
`cache/pictures/` (half the free space, at most 128 MiB, none below
8 MiB) and `video-plane.json`.

Signing out ends the sign-in on the server, or only on the TV while the
server can't prove itself there (the server's Account → Apps ends it
then), and forgets that server's tokens and saved answers. **Settings →
Sign out → Delete all local data** forgets every server, sign-in,
setting and saved picture, as if the app were just installed.

**Logs** go to `/tmp/app.pley.tv/pley.log`, rotated at 1 MiB into
`pley.log.1`, and a restart of the TV empties them. Everything the app
and the TV's libraries print goes through one writer, which stamps each
line with the time since start and takes out tokens, codes, keyed
addresses, the relay's path and a drive's folder and file names. The
files are 0640, readable by the app's group.

## Building and testing

### Commands

`apps/webos` is a Cargo workspace of its own (edition 2024, Rust 1.95 or
newer). Run cargo there: `.cargo/config.toml` applies only there, and on
Windows run it from PowerShell.

```powershell
cargo run -- --fixtures                  # the simulator, signed in to the fake server
cargo test --workspace                   # every crate's tests (CI runs them)
cargo clippy --workspace --all-targets   # lint, as CI does with -D warnings
cargo clippy -p pley-lg --no-default-features --features tv --all-targets   # the TV's code
cargo fmt --all
cargo xtask check --licenses-only        # the TV binary's crates against deny.toml
```

- Tests run headless: the capability report against the web app's
  snapshots, the engine against scripted uMediaServer messages, the
  player against `crates/fixtures`, focus and the controls against their
  display lists.
- **Icons:** `bun scripts/render-icons.mjs` at the repository root draws
  `assets/icon.png` (80 px), `assets/largeIcon.png` (130 px) and the
  1920×1080 launch picture, `assets/splash.png`.

### The simulator

`cargo run` opens the app in a window on Windows, macOS or Linux: the
same screens and code as on the TV, with a player that shows no picture
but keeps time, seeks, changes speed, waits and ends as the TV's does.

| Option | What |
|---|---|
| `--server <url>` | Use this Pley server |
| `--fixtures` | Start the fake server (`crates/fixtures`) and sign in to it with a code, as a TV does |
| `--data <dir>` | Keep the app's data here (by default `pley-lg-sim` in the temporary folder, or a new folder each run with `--fixtures`); the log is `logs/pley.log` in it |
| `--drive <dir>` | A folder of the computer's stands in for a USB drive plugged into the TV; `--drive fixtures` makes a sample drive in the data folder (`crates/fixtures`: films that are heads only, with tracks, chapters, HDR10+, Dolby Vision profile 7, a subtitle file, a folder and what a drive holds that is never listed) |
| `--size <w>x<h>` | The window's size (1280x720; `1920x1080` for the TV's) |
| `--route <route>` | Open a page: `home`, `item:12`, `library:3`, `search:<query>`, `settings:about`, `storage` (the drive's top) or `storage:<folder>`… |
| `--keys "<script>"` | Keys to press once the first page shows: `up down left right ok back menu info play pause playpause stop ff rew next prev chup chdown red green yellow blue`, digits, `hold:<key>:<ms>` (held, repeating as the remote does), `wait:<ms>`, `text:<chars>`, `point:<x>,<y>` (dp), `click`, and `drive:out` and `drive:in` (the drive pulled out and plugged in again) |
| `--screenshot <file>` | After the keys (and a `wait:` at their end), once the frames go quiet, write the frame as PNG and leave |
| `--screenshot-after <ms>` | Take the screenshot this long after the keys instead: a playing video never goes quiet, and otherwise the shot comes 15 s after the last key |
| `--tier low\|mid\|high` | Act as a TV of that tier |
| `--reduced-motion` | As Settings' Reduce motion |
| `--stats` | Print the time to the first frame and the frame times on leaving |
| `--verbose` | Log debug lines too |

Keys: arrows, Enter (OK), Escape or Backspace (Back), M or F1 (menu), I
(info), F2–F5 (colour keys), Space (play/pause), `,` and `.` (rewind,
fast forward), Page Up and Page Down (CH+, CH−), digits; the mouse is the
Magic Remote.

The fake server listens on 127.0.0.1: a demo household with profiles
(Sam's PIN is 1234), libraries of films, shows and music, pictures it
draws itself, Live TV with a guide, the code sign-in approved at once
(and the proofs that tell the server apart, keyed the same on every run),
playback of the Android app's test media, subtitles, seek previews and
the event stream.

### The TV build

The TV binary is `armv7-unknown-linux-gnueabi`, linked by the webOS
toolchain ([openlgtv/buildroot-nc4](https://github.com/openlgtv/buildroot-nc4):
GCC, with a glibc 2.12 older than any TV's as its sysroot), which runs
only on Linux. `.github/workflows/lg.yml` pins its release by address and
SHA-256.

- **With Docker** (any system): `pwsh scripts/build-tv.ps1` builds,
  checks, packs and verifies `out/app.pley.tv_<version>_arm.ipk` in an
  Ubuntu image with Rust and the pinned toolchain, running the same tasks
  as CI. `-Shell` opens a shell in it instead; `-Rebuild` makes the image
  again (a newer Rust). Docker volumes keep cargo's downloads and the
  build between runs.
- **On Linux:** unpack the toolchain, run its `relocate-sdk.sh`, put its
  `bin` on `PATH`, and `rustup target add armv7-unknown-linux-gnueabi`.
  Then the tasks below.

| Task | What |
|---|---|
| `cargo xtask build [--locked]` | The release binary, `target/armv7-unknown-linux-gnueabi/release/pley`, keeping its symbols, with this machine's folders replaced in the paths it records |
| `cargo xtask check` | Every crate the binary is built from has a licence `deny.toml` allows; the binary is soft-float ARMv7 for the TVs' loader, needs only glibc's libraries and `libgcc_s`, no glibc version above webOS 4.0's (2.24), takes glibc's maths functions rather than slower copies, holds no path of the build machine, and weighs at most 16 MiB stripped. `--licenses-only` skips the binary, so it runs anywhere |
| `cargo xtask notices` | `out/NOTICES.txt`: the licences of the crates in the binary, the typeface, the icons and Rust's standard library |
| `cargo xtask ipk` | Builds (unless `--no-build`) and packs the package, with a stripped copy of the binary; the same inputs give the same bytes |
| `cargo xtask ipk-verify [<file>]` | Reads a package back and checks what the TV relies on |

The package is an `ar` archive (`debian-binary`, `control.tar.gz`,
`data.tar.gz`) holding `usr/palm/applications/app.pley.tv/` (`pley`,
`appinfo.json`, the icons and launch picture, `assets/fonts`,
`assets/licenses`, `LICENSE.txt`, `LICENSING.md`, `NOTICES.txt`) and
`usr/palm/packages/app.pley.tv/packageinfo.json`.

## Releases

The app is the `lg` unit in [RELEASING.md](RELEASING.md), released under
the release's `release-<date>` tag. Its version is in `appinfo.json`
(three numbers, as webOS requires) and in the `pley-lg` crate
(`crates/app/Cargo.toml`, and `Cargo.lock`); `bun scripts/version.mjs set
lg <version>` sets them all.
The Release workflow runs `lg.yml` with `release: true` and attaches the
package as `pley-lg-<version>.ipk`.

Every CI run keeps the package (`lg-ipk`) and the binary before
stripping (`lg-symbols`), which turns a crash's addresses into names:
keep a release's.

## Known limits

- **No store.** LG's Content Store takes only web apps. An app installed
  in Developer Mode lasts as long as its session, renewed at least every
  1,000 hours.
- **Black between streams.** A quality picked, an audio track the pipeline
  can't switch, or burned-in subtitles start a new stream, and the
  pipeline can't hold the last picture meanwhile.
- **No dropped-frame count:** Automatic goes by waits and the buffer
  alone.
- **No audio delay:** the pipeline plays the sound itself and offers apps
  no delay to set.
- **No HDR10+** on any LG TV; DTS only where the table above says.
- **Surround over HLS** reaches a receiver as stereo on webOS 23, 24 and
  26.
- **No folder that survives a restart** on some older sets: there the
  sign-in lasts until the TV restarts.
- **No subtitle upload or delete:** the TV has no files to send, so
  subtitles are searched for and downloaded only.

## References

- LG:
  [Developer Mode app](https://webostv.developer.lge.com/develop/getting-started/developer-mode-app),
  [CLI guide](https://webostv.developer.lge.com/develop/tools/cli-dev-guide),
  [appinfo.json](https://webostv.developer.lge.com/develop/references/appinfo-json),
  [streaming protocols](https://webostv.developer.lge.com/develop/specifications/streaming-protocol-drm),
  [resuming media](https://webostv.developer.lge.com/develop/guides/resuming-media-with-mediaoption),
  [Back button](https://webostv.developer.lge.com/develop/guides/back-button),
  [uMediaServer for webOS OSE](https://www.webosose.org/docs/reference/ls2-api/com-webos-media/).
- webosbrew:
  [native toolchain](https://github.com/webosbrew/native-toolchain),
  [webOS Dev Manager](https://github.com/webosbrew/dev-manager-desktop),
  [Homebrew Channel](https://github.com/webosbrew/webos-homebrew-channel).
- LG forum threads:
  [multichannel audio in HLS](https://forum.webostv.developer.lge.com/t/stereo-only-for-dolby-vision-video-with-multi-channel-audio-in-hls-streams/23069),
  [Developer Mode sessions](https://forum.webostv.developer.lge.com/t/whats-the-way-to-extend-dev-time-with-webos-cli/28033).
- Field reports:
  [jellyfin #16092](https://github.com/jellyfin/jellyfin/issues/16092)
  (Dolby Vision from MKV),
  [HDTVTest on DTS](https://www.hdtvtest.co.uk/news/lg-quitely-drops-support-for-dts-sound-on-its-2025-t-vs).

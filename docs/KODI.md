# Pley for Kodi

A Kodi add-on, `plugin.video.pley`, for Kodi 20 (Nexus) and newer on every
platform Kodi runs on: Windows, macOS, Linux, LibreELEC, CoreELEC and
Android TV boxes. It shows Pley's own screens, laid out like the
[Android TV app](ANDROID.md), and plays with Kodi's own player: files as
they are whenever Kodi can, the server's conversion otherwise. No add-on
repository carries it yet.

The add-on is GPL-2.0-or-later, as Kodi is, and talks to the server only
over HTTP ([LICENSING.md](../LICENSING.md)). It is in
`apps/kodi/plugin.video.pley`, its tests in `apps/kodi/tests`.

- [Install](#install)
- [Using it](#using-it)
- [Building and testing](#building-and-testing)
- [Releases](#releases)
- [Known limits](#known-limits)

## Install

Use the zip attached to a Kodi release (`plugin.video.pley-<version>.zip`),
or pack one yourself:

```sh
python apps/kodi/package.py        # dist/plugin.video.pley-<version>.zip
```

1. In Kodi, turn on Settings → System → Add-ons → Unknown sources.
2. Settings → Add-ons → Install from zip file, and pick the zip.

## Using it

### Signing in

1. Open the add-on and choose a server. Servers on the local network answer
   a broadcast; you can also type an address (`192.168.1.5`,
   `nas.local:7539`, `https://pley.example.com`).
2. Kodi shows a code and a QR code. Enter the code on the server's `/link`
   page from a phone or computer that is signed in.

- The add-on signs in only with a code, and gets a viewer's rights
  (`library play playlists profiles`), and `requests approvals` where the
  server lists them in `scopes_supported` (ask for movies and shows; decide
  on them, on an administrator's account). A sign-in from before is
  offered Sign in again where it would help ([Requests](#requests)).
- When the server requires two-step sign-in, the screen with the code says
  so under its notes. Someone who doesn't use it yet sets it up on the
  phone or computer where they approve the code.
- Kodi starts as the profile it was left on, straight into Home, when that
  profile has no PIN. In a household of more than one profile it asks
  "Who's watching?" each time it starts when that profile has one, and
  going on as it asks the PIN, as the other TVs do; a profile with a PIN
  shows a lock.
  A PIN is typed in Kodi's number dialog and never kept. A wrong one, or
  too many, asks again with the reason in the dialog's heading, the only
  line the dialog has; a PIN locked after too many wrong tries is said on
  the screen.
- The same server found at another address must be signed in to again,
  since anything on the network can answer the broadcast.
- Before its token goes to the server's address, the add-on has the server
  prove it is the one signed in to, at that address
  ([API.md](API.md#telling-servers-apart)): for the access token, a proof
  made within the half hour on the same network (Kodi's address changing,
  its waking from sleep, or a request there that got no answer asks for a
  new one); for the refresh token, a profile's PIN and signing out, one
  made then. What can't prove itself gets no token: Pley says another
  device answers at the server's address.
- Switch profile, Sign out and Choose a server are in the profile's menu
  (the rail's avatar) and in the add-on's settings.

### Screens

- **The rail:** the profile, Search, Home, Live TV (where the server has
  channels), the first five libraries, Playlists, Collections and
  Settings. With more libraries, Libraries takes Collections' place. Its
  entries keep fixed places, nine of them over Settings at the other TV
  apps' size, so beside Live TV it lists four libraries.
- **The profile's menu** has Discover and Your requests where the server
  takes requests; they open in Kodi's lists ([Requests](#requests)).
- **The menu key** on a tile: mark watched, My List, add to a playlist,
  remove from Continue Watching, go to the show. In a playlist it also
  moves and removes entries.
- **A library's Library tab** sorts as on the other apps (Bitrate and
  Your rating too; shows have no Duration, music only Title, Date added,
  Your rating and Last played) and filters by what is watched, genre,
  year, resolution and dynamic range. Resolution and Dynamic range are in
  the Filter menu after Year where the library's titles differ in them,
  and the Filter chip says what is chosen ("4K · DV"): the chip
  row has no room for a chip each. Sorted by bitrate, the line under each
  poster is its bitrate and size ("42 Mbps · 23.4 GB"); sorted by bitrate
  or filtered by resolution or dynamic range, posters show "4K" and their
  dynamic range ("DV", "HDR+", "HDR", "HLG"). The tab a library was
  left on is kept; its sort and filters start afresh each time it opens.
- **Library order** is the profile's, saved for all its devices: Sort
  libraries in the profile's menu, or a library's menu key.
- **Search** asks the server for up to 200 titles, 200 people and 200
  Live TV channels, and shows a row for each kind holding all it found.
  Kodi's lists show every title and channel found.
- **Kodi's lists:** Settings → Screens → "Kodi's lists, in this skin's
  views" shows the add-on as folders in the skin's own views. Skin widgets
  and favourites always get these folders.

### Audio and subtitles

Tracks follow the profile, not Kodi's language settings.

- The server picks the audio and subtitles: the profile's language, the
  choice saved for the show or movie, and what Kodi's audio settings pass
  through. Tracks you change in Kodi's player are saved for every device
  once they have held 2 seconds.
- The defaults are under Audio and subtitles, in the profile's menu and in
  the add-on's settings (Playback).
- Text subtitles reach Kodi as WebVTT files. PGS reaches it as a `.sup`
  file, so the video keeps its picture and HDR. VobSub and DVB are burned
  in by the server.
- Kodi's Subtitle offset counts the other way from Pley's other apps:
  +0.3 s shows subtitles earlier.
- **Find subtitles:** in the player, Subtitles → Download lists "pley". It
  searches in the profile's subtitle language (not Kodi's), and the one
  you pick is added on the server and saved as the profile's choice.

The add-on adds no controls to Kodi's player and changes none of Kodi's
settings. Use Kodi's own:

- **Speed:** Play speed in the player's settings (0.8× to 1.5×). Kodi
  offers it only with Settings → Player → Videos → Sync playback to display
  on.
- **Audio offset:** the player's audio settings.
- **Subtitle look:** Settings → Player → Language.

### Quality

Settings → Playback → Quality offers [the menu](QUALITY.md#the-menu):
Original, Automatic (the default), 30 Mbps 4K down to 2 Mbps 480p. Kodi's
player has no Pley quality menu, so you choose before playing, and one
setting serves every title: the server converts a file above it and plays
one below it as it is. The remote on your other devices changes it while
a title plays ([Remote control](#remote-control)), and the title plays
again where it was at the new quality.

Automatic follows the rules for
[players that go black between streams](QUALITY.md#players-that-go-black-between-streams).
Kodi's part (`autoquality.py`):

- It asks for one size only (no `hls.variants`): Kodi's HLS player keeps
  the first one it takes.
- Waits are `Player.Caching` or a position that doesn't move; delivery is
  JSON-RPC's `cachepercentage`. Kodi reports no dropped frames.
- A conversion's bitrate is lowered in place
  (`PUT /api/playback/{session}/bitrate`). If Kodi's player gives up on the
  original, the title plays again converted, once, from where it was.
- A step down shows as a Kodi notification. Each is kept 30 minutes per
  server in `remembered.json`, and sent to the server's network memory.
- Kodi reads a full buffer only as fast as it plays, so it rarely measures
  a way up; a stream that can't go higher raises where the next title
  starts instead.

### Live TV

Live TV works as in the [Android app](ANDROID.md#live-tv). On Kodi:

- **The page** (`livetv.py`): the categories, then the focused category's
  channels (100 to a page) with what's on, then the focused channel's
  programme.
- **A channel:** Select watches it. Menu has Watch, the favourite and
  Schedule; Info is Schedule, the channel's next six hours. Number keys
  typed together go to that channel.
- **Search channels** opens Kodi's keyboard.
- **Kodi's lists** show Live TV after Home's rows, 300 channels to a page.
- **Playing:** Kodi plays the provider's MPEG-TS passed through the server
  (`GET /api/live/channels/{id}/stream.ts`), not the server's HLS. Kodi
  deinterlaces it itself, surround reaches a receiver, the server converts
  nothing, and it starts at once.
- **Tuning:** the add-on first asks `POST …/watch` with this Kodi's
  `client_id`, then Kodi reads the stream for that session as
  `<client_id>-stream`, so changing channel never holds two connections,
  and the watch session stays while Kodi reads it: it is the one your
  other devices see and control ([Remote control](#remote-control)).
- **Leaving:** when Kodi stops reading a channel (you stop it, another
  channel or a title plays, Kodi exits), the service ends its session
  with `DELETE /api/live/sessions/{session}` at once, rather than the
  server waiting 45 s, so it doesn't hold a connection or stay on your
  remotes. A session the server ended (taken over, stopped from
  elsewhere) is left as it is.
- **No pause or seeking:** the stream has no length and no
  `Accept-Ranges`, so Kodi's controls offer only Previous, Stop and Next.
- **Channel up and down:** the channels around the one playing (100 each
  side) are Kodi's video queue, so the player's Next and Previous (the
  remote's skip keys, `.` and `,`) change channel. Up and Down don't.
- **Every connection in use (`409`):** Kodi's list offers "Watch here
  instead" where the server allows it.
- **Taken over:** the server's `live_taken_over` event for the channel
  playing stops it at once and empties Kodi's queue of channels, so Kodi
  doesn't tune the next one. A dialog, "Live TV stopped", gives the
  server's words with OK and Watch again, which plays the channel as any
  channel plays. A stream that ends without that event within a second
  says "The server stopped this stream."
- **Stopped from another device** (`session_stopped` for the channel's
  session): it stops as when taken over, and the dialog says "Live TV was
  stopped from another device." or "Live TV was stopped by the server's
  administrator.", with their words on the next line.

### Remote control

Your other devices control what Kodi plays (see
[API.md](API.md#remote-control)); Kodi itself shows no remote. The
service (`service.py`) acts on `player_command` for the session playing
here, by the shared remote-control rule, written in `remote.py` and
tested against every case of `packages/tv/test/fixtures/remote.json`:

- **What it says:** each play request and Live TV watch lists what Kodi
  obeys (`controls`), names it by the name it was given in Kodi's Settings,
  Services, General, Device name (at most 64 characters) and says it is a
  TV (`device_kind: tv`): Kodi is made for the TV, on a computer too, so
  remotes list it with the TVs. Until it is given a name, Kodi calls itself
  "Kodi" with the computer's host name after it, which isn't sent: the
  server names it from the add-on instead.
- **Movies and episodes:** play, pause, seek, skip, audio, subtitles and
  quality, and next when Kodi's queue has something after it. An episode
  always lists next: with nothing queued after it, Next plays the episode
  after it, as Up Next does (and does nothing on a show's last).
- **Audio and subtitles:** a video lists `audio` and `subtitles` when its
  file has tracks of them, and both while its details are still loading
  (they come with the server's answer). A pick switches Kodi's player to
  that track, found by its place in Kodi's lists as a pick in Kodi's own
  menu is (`tracks.py`), and is saved for the show or movie the same way
  once it has held 2 seconds. Kodi switches only to what it lists: a
  converted stream carries only the audio the server put in, subtitles
  drawn onto the video stay, and picture subtitles Kodi wasn't given as a
  file can't be picked. Such a pick does nothing.
- **Quality:** the Quality setting is Kodi's Quality menu. A video lists
  `quality`, and the remote picks among what the menu offers for the file
  ([QUALITY.md](QUALITY.md#the-menu)). A pick changes the setting, as
  choosing it there does (Automatic picked again starts afresh), and the
  title plays again where it is at it, a new session that the remote
  sees as it starts. The quality picked already reports again; one the
  file isn't offered does nothing.
- **What plays:** every progress report of a video says the audio and
  subtitles Kodi plays, by the server's stream indexes (`audio_index`,
  `subtitle_index`, `-1` for none), so remotes tick them. A track the
  server doesn't know (captions Kodi found in the video, subtitles added
  from Kodi's dialog) is left out, and the server keeps the one it knew.
  It says the quality picked too (`quality`): the setting, or Original
  for a file that isn't offered it, which then plays as it is.
- **Music:** play, pause, seek, skip, next and previous. Previous starts the
  track again past 3 seconds in, or on the first track, else plays the one
  before.
- **Live TV:** next and previous change channel within Kodi's queue of
  channels, round its ends, when there is one; the stream can't pause or
  go back, so nothing else. Without a queue (a channel played again with
  Watch again), it obeys only Stop.
- Play while playing, pause while paused, or the audio, subtitles or
  quality that play already change nothing and report again, so a remote
  a little behind settles. A seek, a skip or another track is reported at
  once; another title, channel or quality reports with its own session.
- Kodi stopped or playing something else obeys nothing. A command it
  doesn't know, or a seek or skip without its place, does nothing.
- **What shows:** the add-on brings up nothing of Kodi's player for a
  command. What the remote did to a video shows for 2 seconds as a Kodi
  notification with the add-on's icon, without its sound: "Paused",
  "Playing", the skip asked for ("+30 s", "−10 s"), where a seek went
  ("Jumped to 12:34"), the audio or subtitles switched to, by the
  language and title Kodi's own Audio and Subtitles menus name them by
  ("Audio: French - Commentary", "Subtitles off"), and the quality picked
  as the Quality setting names it. A command that changes nothing shows nothing; next
  and channel up and down show what Kodi shows as it plays the title or
  channel, and music shows nothing. Kodi's own pause and seek bar are its
  skin's: most skins show the seek bar while paused and for a moment
  after a seek, whoever made it, and the add-on can't keep it away (a
  skin opens it again while its condition holds).

### Casting

Your other devices play things on Kodi (their Play on…, see
[API.md](API.md#casting)); Kodi itself casts to nothing. The service does
it (`casting.py`):

- **Taking casts:** its event stream makes Kodi a receiver, by the
  `client_id` its plays are made with, as a TV named as its plays are
  named. It takes titles and playlists always, and Live TV's channels
  where the account has Live TV as the stream opens, and says it switches
  to the profile of a remote of another profile that casts to it
  (`casts=…,switch`).
- **What plays:** what is sent plays as the add-on's own Play plays it,
  through the plugin's play routes, in place of what plays:
  - a movie or an episode from where it was sent, else where Play starts
    it (its resume point), with the audio and subtitles picked on the
    remote's page in place of the account's choice for that play, where
    the file it plays (its first) has them: the audio is asked for with
    the play request's `audio_index`, the subtitles shown as the account's
    choice would be, and neither is saved. A track of another version
    leaves the account's choice. Played again (another quality,
    Automatic), it keeps them;
  - a show or a season its next episode;
  - an album or an artist as Kodi's music queue, shuffled when asked, and
    a track its album from it;
  - a playlist what it holds: music as Kodi's music queue, titles as
    Kodi's video queue, played through;
  - a channel in Kodi's queue of channels, the lineup's channels around
    it, so channel up and down work as for a channel played from Live TV.
- **Where it came from:** "Playing from {device}" ("Playing from another
  device" when the remote's has no name) shows for 4 seconds as a Kodi
  notification with the add-on's icon, and "Nothing to play" when a show
  has no next episode or an album or a playlist is empty.
- **From another profile:** a remote on another profile of the household
  casts with its profile and a ticket, and Kodi switches to that profile
  first, with the switch-profile grant and the ticket in place of a PIN
  (the remote gave its own). What played ends as the profile it played
  as, its last report and its live channel's session sent before the
  switch. The add-on then acts as the remote's profile:
  Pley's window goes to its Home, a listing of the add-on on screen shows
  again, and "Who's watching?" counts as answered until Kodi starts over.
  A switch that fails (the ticket spent or out of time, the server out of
  reach) says "Couldn't switch to the remote's profile" for 4 seconds,
  plays nothing and stops what played, which was ended for the cast.
- **Not taken:** a cast for another device, one naming another profile
  without a ticket, or one that comes while the add-on is signed out or
  while Pley's window asks "Who's watching?" as Kodi starts (a cast
  neither plays past its PINs nor switches profile while one is picked),
  does nothing. Of two casts close together, the later plays.

### Requests

Where the server takes requests (its summary, `GET /api/requests/summary`,
says `enabled`, and the account may ask), the add-on asks TMDB, through the
server, for movies and shows its libraries don't have, and the server's
Radarr and Sonarr fetch them once approved. The add-on's root lists
**Discover** and **Your requests** after Search; in Pley's look the
profile's menu opens them, in Kodi's lists (the window closes, Back from
their top goes back to Kodi, and the add-on opens Pley's look again).

- **Discover**: Trending this week, Popular movies, Popular shows, Coming
  to cinemas and Popular anime as folders, then Your requests. Each list
  shows at least 20 titles at once (TMDB's pages come short where titles
  are left out), with Next page, and sorts by the server's order, by name
  or by year. Words are looked for on TMDB from Search.
- **A title** shows its poster, backdrop, year, rating and overview, and
  its badge (In library, Partly in library, Requested, Declined) on the
  right and atop its plot. OK does its one main action: Request; Open for
  one in a library; Tell me too for one someone asked for; what its request
  says, or why it can't be asked for (declined until a date, the
  allowance used up, an age limit, an account that may not ask). The
  context menu has the rest: a show's Seasons, Open, Request more seasons,
  and what may be done to its request.
- **A show's seasons**: its main action atop, then each season with its
  state ("In library", "4 of 10 episodes in library", "Waiting for
  approval", "Airs Nov 4", "Declined"); OK on one that may be asked for
  opens the picker with it ticked, and on the others opens or says what
  they are.
- **Which seasons?** is Kodi's multiselect, of the seasons that may be
  asked for, each with its state: the missing ones are ticked, lowest
  first and as many as may be asked for, specials and seasons not out yet
  left unticked. Picking none says "Pick a season."; picking more than
  the allowance says what it allows.
- **Your requests**, newest first: each one's stage on the right ("Waiting
  for approval", "Downloading 42% · in 20 min", "Ready to watch"), then its
  seasons, what it says and when it was asked for. OK opens the title once
  a library has it, else offers what may be done: Withdraw (Remove for a
  declined one), Stop following, Request again. Opening the list marks its
  news seen; it is shown again every 10 seconds while something on it
  downloads.
- **Requests**, for an administrator's account (signed in with
  `approvals`), opens on Needs attention, oldest first, with filters atop
  (which requests, movies or shows, whose, a title's words). Yours, the
  last choice of which requests, lists the account's own and marks their
  news seen; the others leave it unseen. Each request offers Approve, Try
  again, Decline, Decline with a reason… (typed on Kodi's keyboard), Mark
  as added by hand and Delete (asked first). The entry carries what
  waits, new requests and news, red while one couldn't be sent.
- **Search** lists Discover after the libraries' results: TMDB's first
  titles the libraries don't have, then See all, which lists all TMDB
  has for the words ("Results for “dune”"), each title with its badge,
  with Next page. TMDB gets the words' first 200 characters, the most
  the server takes. It is looked for beside the libraries' search and
  shows once it comes, without holding them up. Kodi's lists wait 3
  seconds for it at most, as Kodi keeps the add-on's Python busy
  meanwhile; slower, the Discover entry opens it on Discover.
- **Search's Discover, otherwise**: with nothing in the libraries either,
  "Nothing found for “dune”"; when TMDB isn't answering, refuses the
  server's key or the account may not ask, what is wrong, with Try again
  where it may help. In Pley's look the row's title says what is wrong,
  and its one tile is Try again, or See all, whose page says what to do.
- **Library shows** have Request more seasons in their context menu while
  the profile may ask: the show's picker, or what stops it.
- **Notices**: "“Heat” is ready to watch." (with "Season 2 of “X” is ready
  to watch." for a show partly there) pops up with the poster as soon as
  it is, from the event stream; news that came while Kodi was off or the
  stream was down shows once as Kodi starts ("3 of your requests have
  news" for several). A request ready, or a season of it, is said once per
  server and account (`remembered.json` keeps the last 300), whatever else
  changes in the request; none shows over a playing video: the last three
  wait for it to stop. As on the other TV apps, decisions, new requests
  and failures show only in the lists and the entry's number. Unlike
  theirs, the notices have no Open: Kodi's notifications take no button,
  so the title opens from Your requests.
- **Off, or not open to the account**: no Discover, no Your requests
  (an account that may no longer ask keeps its own requests). A sign-in
  made before the server took requests shows only "Sign in again to ask
  for movies and shows" on Discover; an administrator's without
  `approvals`, "Sign in again to approve and decline here." atop the list.
  "TMDB isn't answering" offers Try again; a key TMDB refuses tells
  administrators where to fix it.

Discover's plots end with "Details and pictures from TMDB."; the add-on's
description, in its information in Kodi, ends with "This product uses the
TMDB API but is not endorsed or certified by TMDB."

## Building and testing

### How it works

- **The service** (`service.py`) starts with Kodi. It keeps the tokens in
  `addon_data/plugin.video.pley/tokens.json` and alone renews them, since
  each refresh token works once.
- **The proxy:** Pley takes a token only in the `Authorization` header, and
  Kodi's player and artwork cache know only addresses. So the service runs
  a proxy on `127.0.0.1` (`http://127.0.0.1:<port>/<secret>/api/…`) that
  adds the token. The secret keeps other programs out.
- **The plugin** (`default.py`) opens Pley's window, lists folders for
  Kodi's lists, and asks to play with what this Kodi plays
  (`resources/lib/pley/caps.py`): every container and codec, the formats
  its audio settings pass through, HDR10, HLG, and Dolby Vision where Kodi
  says the screen shows it (profiles 5 and 8; on CoreELEC 7 too, and
  Dolby Vision with HDR10+ beside it, which many Android devices' decoders
  show black). Kodi doesn't tell its decoders apart from its screen, so a
  box on a screen without Dolby Vision lists no profiles, and its details
  read "as Dolby Vision isn't shown here". It doesn't ask for master
  playlists: Kodi's HLS player reads no `CODECS`, and MPEG-TS segments
  carry no Dolby Vision configuration, so Dolby Vision plays as it is only
  from the file. A remux plays its HDR10 (or HDR10+) base layer; profile
  5, which has none, is converted instead.
- **Progress** goes to the server every 10 seconds while playing, every 30
  while paused, and on stop.
- **Events:** the service keeps `GET /api/events` open, as a receiver of
  casts ([Casting](#casting)), and refreshes what is on screen, never
  behind a playing video.
- **Requests** (`requests.py` holds the rules and words, `asking.py` the
  listings, `news.py` the service's part): the service reads the summary
  as Kodi starts, on another sign-in or profile, when the event stream
  opens again, right after the add-on did something to a request, a second
  after request events (once for a burst), and every 60 seconds while the
  stream is down or the summary, or the news it says there is, can't be
  read; a server without requests (404, or 403 from a build before apps
  could ask) is asked again only when the stream opens again.
  It keeps the summary in a window property for the plugin and Pley's
  window, so listings ask nothing more. TMDB's pictures
  (`/api/discover/images/…`) come through the proxy as library artwork
  does, at their drawn width.
- **Discovery:** a broadcast of `pley?` to UDP port 7539
  (`discovery.py`, answered by `apps/server/src/discovery.rs`).
- **The window:** one window (`window.py`) holds every screen.
  `apps/kodi/skin.py` writes its XML from `resources/lib/pley/layout.py`;
  run `python apps/kodi/skin.py` after changing the layout, or the tests
  fail. `bun scripts/render-kodi-skin.mjs` renders its pictures from the
  Android app's icons, and `bun scripts/render-icons.mjs` the add-on's icon
  and fanart.
- **Python 3.8:** Kodi 21 runs Python 3.8 on Windows, so the add-on uses
  nothing newer. CI checks.

### Tests

Unit tests need any Python 3.8 or newer and nothing installed (Kodi's
modules are stubbed in `tests/stubs`):

```sh
cd apps/kodi/tests
python -m unittest discover -s . -p "test_*.py"
```

`test_e2e.py` runs the add-on against a real server: sign-in, browsing,
direct play, a converted stream, progress, a cast, token renewal and
sign-out. It
needs the server and transcoder built
(`cargo build -p pley-server -p pley-transcoder`) and FFmpeg:

```sh
PLEY_BIN=target/debug/pley PLEY_FFMPEG_DIR=/usr/bin python -m unittest test_e2e
```

`test_asking.py` covers Discover, search's Discover entry, the picker
and the requests' lists and actions, `test_news.py` the service's
summary, refreshes and notices, and `test_requests.py` the rules and
words; none reaches TMDB. CI runs all of them.

By hand, use a portable Kodi (`kodi.exe -p` on Windows) with the add-on
copied into `portable_data/addons`. With JSON-RPC on (Settings → Services →
Control → Allow remote control from applications on this system) you can
drive it over TCP port 9090: `Files.GetDirectory` on
`plugin://plugin.video.pley/…` lists screens and `Player.Open` plays. Kodi
redraws only what changed, so for screenshots put this in
`portable_data/userdata/advancedsettings.xml`:

```xml
<advancedsettings><gui><algorithmdirtyregions>0</algorithmdirtyregions></gui></advancedsettings>
```

## Releases

The add-on is the `kodi` unit in [RELEASING.md](RELEASING.md). A release
also writes its notes into `addon.xml`'s `<news>`, which Kodi shows as the
add-on's changelog.

## Known limits

- Not yet: finding subtitles before playing or in another language than the
  profile's; a title's Details, the library's letter bar and a
  person's page; downloads; playing a video playlist through (a title in
  one opens its page).
- Picture subtitles picked in Kodi's player during a converted stream don't
  show; the profile's choice is burned in from the start.
- Discover and the requests open in Kodi's lists, not Pley's own screens.
  Approve takes what was asked: no choice of profile, folder or seasons.
  With no title page to stop on, a show partly in a library opens there,
  as one a library has whole does, with Request more seasons in its
  context menu; the other apps open its Discover page.
- Live TV has no guide grid: Kodi draws one only for PVR add-ons. Kodi's
  PVR IPTV Simple client isn't used either: it would mean another add-on to
  set up, and favourites and recents the server doesn't know.

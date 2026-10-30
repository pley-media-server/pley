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
  (`library play playlists profiles`).
- In a household of more than one profile, Kodi asks "Who's watching?" each
  time it starts. A PIN is typed in Kodi's number dialog and never kept.
- The same server found at another address must be signed in to again,
  since anything on the network can answer the broadcast.
- Switch profile, Sign out and Choose a server are in the profile's menu
  (the rail's avatar) and in the add-on's settings.

### Screens

- **The rail:** the profile, Search, Home, Live TV (where the server has
  channels), the first three libraries, Playlists, Collections and
  Settings. With more than three libraries, Libraries takes Collections'
  place.
- **The menu key** on a tile: mark watched, My List, add to a playlist,
  remove from Continue Watching, go to the show. In a playlist it also
  moves and removes entries.
- **Library order** is the profile's, saved for all its devices: Sort
  libraries in the profile's menu, or a library's menu key.
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
one below it as it is.

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
  `client_id`, then Kodi reads the stream under the same id, so changing
  channel never holds two connections.
- **No pause or seeking:** the stream has no length and no
  `Accept-Ranges`, so Kodi's controls offer only Previous, Stop and Next.
- **Channel up and down:** the channels around the one playing (100 each
  side) are Kodi's video queue, so the player's Next and Previous (the
  remote's skip keys, `.` and `,`) change channel. Up and Down don't.
- **Every connection in use (`409`):** Kodi's list offers "Watch here
  instead" where the server allows it. A connection taken by someone else
  ends with "The server stopped this stream." (`stream.ts` can't say who).

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
  says the screen shows it (profiles 5 and 8; 7 too on CoreELEC).
- **Progress** goes to the server every 10 seconds while playing, every 30
  while paused, and on stop.
- **Events:** the service keeps `GET /api/events` open and refreshes what
  is on screen, never behind a playing video.
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
direct play, a converted stream, progress, token renewal and sign-out. It
needs the server and transcoder built
(`cargo build -p pley-server -p pley-transcoder`) and FFmpeg:

```sh
PLEY_BIN=target/debug/pley PLEY_FFMPEG_DIR=/usr/bin python -m unittest test_e2e
```

CI runs all of them.

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
- Live TV has no guide grid: Kodi draws one only for PVR add-ons. Kodi's
  PVR IPTV Simple client isn't used either: it would mean another add-on to
  set up, and favourites and recents the server doesn't know.

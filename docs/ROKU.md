# Pley for Roku

A native SceneGraph app for Roku players and Roku TVs, in `apps/roku`
(paths below are relative to it). It is not in Roku's channel store: you
sideload it in developer mode.
It talks only to the Pley server, so it works without internet.

- [Install](#install)
- [Using it](#using-it)
- [Building and testing](#building-and-testing)
- [Releases](#releases)
- [Known limits](#known-limits)

## Install

1. Turn on developer mode: on the Roku remote press Home three times, Up
   twice, then Right, Left, Right, Left, Right. Note the address shown,
   choose a password and let the Roku restart.
2. Get the zip: the one attached to a Roku release
   (`pley-roku-<version>.zip`), or `bun run package`, which writes
   `out/pley-roku.zip`.
3. Open the Roku's address in a browser, sign in as `rokudev` with that
   password, upload the zip and press Install. Pley appears on the Home
   screen; installing again replaces it.

`bunx bsc --deploy --host <address> --password <password>` packages and
installs in one go. The debug console (`telnet <address> 8085`) shows what
the app prints and its errors.

## Using it

The app finds Pley servers on the local network (or takes an address) and
signs in with a code shown on the TV, approved on the server's `/link` page
or by its QR code. It asks "Who's watching?" at start when the household
has more than one profile.

### Remote keys

| Key | Where | What it does |
|---|---|---|
| Options (\*) | A title | Mark watched, My List, Add to playlist, Remove from Continue Watching, Go to show |
| Options (\*) | Watch history, a playlist entry | Removes it |
| Down | The player | Pley's menu: Quality, Subtitle sync, Chapters, Find subtitles…, Playback details |
| Options (\*) | The player | Roku's own options: subtitles and audio tracks |
| OK | Skip intro, Skip credits | Skips; fast forward and rewind move 10 s |
| Replay | Live TV | Search (Roku remotes have no search key for apps) |

Settings has the audio and subtitle defaults, Quality, Play the next
episode by itself (Up Next counts down only while it is on), Watch
history, Switch profile, Sort libraries, Switch server and Sign out.

### Audio and subtitles

- Roku shows a subtitle track only with captions on, a Roku-wide setting.
  When Pley picks a track it turns captions on for that playback and
  restores them after.
- Subtitles look as the Roku's caption settings say (Settings →
  Accessibility → Captions style). The app never sets `captionStyle`,
  which would change them for the whole Roku.
- Text subtitles go to the Video node as WebVTT. Picture subtitles (PGS,
  VobSub, DVB) are burned in by the server.
- **Subtitle sync:** Left and Right move subtitles 0.1 s, up to 10 s either
  way. The server shifts the cues (`offset_ms`) and the stream reopens
  where it was. The offset is kept per file per server (the latest 50).
- **Find subtitles…** searches OpenSubtitles in the profile's subtitle
  language (`components/lib/SubtitleSearch.brs`). OK adds the one in focus
  on the server and shows it.
- The server picks the audio this Roku plays best in the profile's
  language. Any audio but the file's default makes the file a stream with
  its video copied.
- Picks in Roku's options are saved for the show or movie, so the next
  episode starts with the same tracks. Roku's audio tracks are matched to
  the file's only when their count and languages agree.

### Quality

The menu, Original and Automatic follow [QUALITY.md](QUALITY.md#the-menu);
the code is `components/lib/AutoQuality.brs`. On Roku:

- **Settings → Quality** is kept on the Roku. One chosen in the player's
  menu becomes the setting, and the title reopens at it.
- **Several sizes:** in an adaptive session the Video node moves between
  sizes itself. Other streams come in one size, so the app steps down.
- **Signals:** a wait is the Video node's `buffering` after at least 2 s of
  playing, and only when `bufferingStatus.isUnderrun` says it is for want
  of data. Network speed comes from the HLS segments' sizes and download
  times. Roku reports neither read-ahead nor dropped frames.
- Roku follows the rules for
  [players that go black between streams](QUALITY.md#players-that-go-black-between-streams):
  a conversion whose bitrate can change is lowered in place; the original,
  or a fixed-bitrate conversion, moves to a new session at least one
  quality lower after a wait of 2 s or more.
- **Memory:** each step down is kept 30 minutes per server and told to the
  server. When the server asks for a speed test, the app times 8 MiB of
  `GET /api/playback/speedtest`, giving up after about 3 s.

### Live TV

Live TV works as in the [Android app](ANDROID.md#live-tv), laid out as
TiviMate does. On Roku (`components/screens/LiveTvScreen`,
`LivePlayerScreen`, `components/lib/Live.brs`):

- **Paging:** channels come 50 at a time, and the guide only for those 50
  (`GET /api/live/guide?ids=`).
- **Categories** run down the left under Search. Rewind and fast forward
  move a screen at a time. The category and the tab are kept per server
  and profile.
- **Guide:** Roku's TimeGrid, 50 channels by two hours. Rewind and fast
  forward move two hours, Play goes back to now. OK on what's on watches
  the channel; on a later programme it shows its details.
- **Channels:** the other tab, as cards with what's on and what's next.
- **The player** tunes with `POST …/watch`, waits for the server's live HLS
  playlist (up to 50 s), then plays it with `Live = true`. Up and Down
  change channel, tuning 0.7 s after the last press. Left opens the
  channel list; Right, OK or Options show the banner. Leaving, or changing
  channel, sends `DELETE /api/live/sessions/{session}`.
- **Radio:** a channel with no picture keeps its card with what's on.

## Building and testing

### How it works

| Path | What |
|---|---|
| `source/main.brs` | Starts the scene |
| `components/MainScene.*` | The screen stack, the HTTP agent for pictures, sign-in when the server ends it |
| `components/MusicPlayer.*` | The music queue on the scene's Audio node |
| `components/screens/` | One component per screen |
| `components/shared/` | Screen helpers (`Ui.brs`), title actions (`Titles.brs`) |
| `components/items/` | Tiles, rows, the player's skip button |
| `components/tasks/ApiTask` | The only code that talks to the server |
| `components/tasks/DiscoveryTask` | `pley?` to UDP port 7539 (`apps/server/src/discovery.rs`) |
| `components/tasks/EventsTask` | The server's event stream, on a socket |
| `components/lib/` | Plain BrightScript (no SceneGraph): the logic the unit tests cover |
| `tests/` | Unit tests of `components/lib` |
| `scripts/` | Test runner, smoke run, preview, bench, images |
| `licenses/` | Notices for bundled third-party code (BrighterScript's `bslib.brs`) |

Rules the code keeps:

- All HTTP goes through `ApiTask`; the render thread never makes a request.
- The access token goes only in `Authorization: Bearer`, never in an
  address, including the Video node's and the pictures' HTTP agents.
- Refreshes run one at a time per server, and the new token pair is saved
  before use (a refresh token works once).
- The User-Agent is `pleyRoku/<version> (Roku OS <version>; <model>)`; the
  server's `/link` page shows it.
- The event stream (`GET /api/events`) works with `http://` servers only:
  Roku's sockets have no TLS.
- Settings and views are kept in registry sections: `playback`,
  `subtitleSync`, `libraryViews`, `liveViews`, `household`.
- Launch beacons: `AppLaunchComplete` once the first screen is usable;
  `AppDialogInitiate` and `AppDialogComplete` around a sign-in at launch.

### Commands

In `apps/roku`, with Bun and Node 22 or newer. Bun runs the scripts, and
the tools (bsc, bslint) run on Node, since bslint's plugin loader needs
Node's `module.require`; so `apps/roku` has no `bunfig.toml`.

```powershell
bun install
bun run check     # bsc's compile check, bslint, the unit tests (CI runs this)
bun run smoke     # the whole app in brs-node, against a fake server
bun run bench     # the list work timed in brs-node
bun run preview   # the app in a browser, the keyboard as its remote
bun run images    # the nine-patch focus and watched-mark images
```

- **Preview** serves the app on http://127.0.0.1:8086/. Arrows, Enter
  (OK), Esc or Backspace (Back), Space (play/pause), `[` and `]` (rewind,
  fast forward) and `*` (options) are the remote. It uses the smoke run's
  fake server, which approves the code itself; `--server
  http://<address>:7539` uses a real one; `--port` changes the port.
- **Unit tests** run `components/lib` and `tests/*.test.brs` in brs-node;
  each `sub test_…()` is a test.
- **Smoke run** starts a fake server (`scripts/fixture-server.mjs`) and
  walks every screen, saving each in `out/smoke/`. It fails on a
  BrightScript error, a request without the User-Agent, a token in an
  address, or a refresh token used twice.
- **brs-node limits:** its own fonts, no video (Skip intro and Up Next have
  only unit tests), one fetch at a time, the odd dropped key, and no
  measure of a Roku's speed.
- **Images:** `bun run images` runs `scripts/make_images.py` (Python
  standard library). The home screen icons and splash screens come from
  `bun scripts/render-icons.mjs` at the repository root.

## Releases

The app is the `roku` unit in [RELEASING.md](RELEASING.md); its version is
the manifest's `major_version`, `minor_version` and `build_version`.

The manifest asks for SceneGraph 1.2, which every Roku OS from 9 runs.

## Known limits

- No playback speed or audio delay: the Video node has neither.
- Find subtitles… searches only the profile's subtitle language.
- Live updates need an `http://` server; with `https://`, Home refreshes
  once a minute.
- Audio other than the file's default isn't played as it is.
- Not yet: seek previews, rating titles, reordering a
  playlist, "Always start as", an editable music queue.


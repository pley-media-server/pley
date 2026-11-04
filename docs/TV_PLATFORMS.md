# Pley for Samsung TV

Pley for Samsung TV is a web app for Samsung's Tizen TVs, in `apps/tizen`.
Everything that doesn't call into the TV is in `packages/tv` (TypeScript
and Preact): the API client, sign-in, focus, screens, the player's logic and
subtitles. `apps/tizen` adds the TV's player, remote keys, capability probe
and packaging. It follows the [Android TV app](ANDROID.md): the same
960×540 layout, D-pad focus, screens and player features. (Pley for LG TV
is a native app: [LG.md](LG.md).)

- **Package:** a Tizen web app (`.wgt`).
- **Player:** AVPlay, Samsung's player, on the hardware video plane.
- **Oldest TV:** 2018 (Tizen 4.0, Chromium 56); Samsung's store takes new
  media apps for 2021 (Tizen 6.0) and later.

The app is a web app that hands the file's address to the TV's own player,
with its hardware decoders, HDR and sound output. No video goes through a
JavaScript player. Samsung's TVs also run Tizen .NET apps, and a native
Samsung app is being built beside the web one
([The native Samsung app](#the-native-samsung-app)).

It isn't in a store: it is installed on a TV in developer mode.

- [Install](#install)
- [Using it](#using-it)
- [Building and testing](#building-and-testing)
- [Releases](#releases)
- [Known limits](#known-limits)
- [The native Samsung app](#the-native-samsung-app)

## Install

There is no store release: build the package ([Commands](#commands)), then
install it on a TV in developer mode.

1. **Certificate.** TVs from 2023 (Tizen 7.0) in developer mode, and the TV
   emulator, install only packages signed with a Samsung certificate
   ([install policy](https://developer.samsung.com/smarttv/develop/getting-started/using-sdk/tv-emulator/application-install-policy.html)).
   Make one in Samsung's Certificate Manager with a Samsung account; it
   lists the TVs' DUIDs. Older TVs take a Tizen author certificate.
2. **Developer mode** on the TV: Apps, `12345` on the remote, Developer
   mode On with your computer's IP address, then restart the TV.
3. `sdb connect <TV address>`; `sdb devices` names the target.
4. With a Samsung certificate, run `tizen install-permit -t <target>` once.
5. `tizen install -n pley.wgt -t <target> -- apps/tizen/out`.

The native Samsung app installs the same way, as a `.tpk`
([Setting up](#setting-up)).

## Using it

Sign-in, server discovery (a sweep of the TV's subnet), "Who's watching?"
and its PINs, the screens, the player's keys and panels, and offline
behaviour follow the Android TV app. With Always start as too, a start
never goes on as a profile with a PIN without it: the TV keeps the profile
it was left on (`GET /api/auth/me`, with `has_pin`), and until it has,
asks "Who's watching?". What differs:

### Remote keys

- Next and Previous move between a film's chapters (Next plays a playlist's
  or collection's next title).
- In the player, OK with the controls hidden pauses, Down shows them on
  Subtitles, Left and Right seek 400 ms after the last press (a held key
  when let go), and the chapters are a side panel. The Android TV app and
  the native Samsung app show the controls on OK (Down: on Play/Pause),
  wait for OK before a seek, as YouTube's TV app does, and show the
  chapters as a row of cards over the scrubber.
- Samsung's `ChannelUp`, `ChannelDown`, `ChannelList` and `Guide` keys are
  registered.

### Audio and subtitles

- The server picks the audio track that this TV plays best in the
  profile's language, unless you pick one on the title's page or in the
  player. Picks are saved for the show or movie; subtitles picked alone are
  saved without the audio.
- **Settings → Profile** sets the defaults (Audio language, Subtitles,
  Subtitle language).
- **Subtitle sync:** 0.1 s steps, up to 10 s either way, kept per file on
  the TV.
- **Settings → Subtitles and display:** size, background, font and
  position (raised 5, 10 or 16 % of the height). A font is offered only if
  the TV has a face for it. They are for text subtitles: picture subtitles
  keep their own look.
- **Pley draws every subtitle itself:** AVPlay takes external files from
  local paths only. Picture subtitles (PGS, VobSub, DVB) are decoded from
  the server's `.mks` onto a canvas.
- **Find subtitles** searches OpenSubtitles in the profile's subtitle
  language (or another picked there). The TV has no file upload and no
  delete.
- **Speed:** none. AVPlay's speeds are trick play, 2× and up, with no
  promise about sound.

### Quality

The menu, Original and Automatic follow [QUALITY.md](QUALITY.md#the-menu),
in the player's Quality panel and Settings → Playback → Quality. Automatic
is the default until a quality is picked on this TV (`pley.qualityPicked`).

The TV's player goes black between streams, so Automatic follows the
[rules for such players](QUALITY.md#players-that-go-black-between-streams),
in `packages/tv/src/player/autoquality.ts`. A transcode comes in one size
(the app doesn't say `hls.variants`).

- **Signals:** a wait is the engine buffering while playing (not a pause,
  seek or new session). Samsung has only waits and, for a stream, AVPlay's
  `CURRENT_BANDWIDTH`. The TV counts no dropped frames.
- **Decoding:** an original that fails falls back once to the server's
  quality.
- **Bitrate changed in place:** a new stream of the same quality (recovery,
  a track change) starts at the bitrate the last one reached. A `400` from
  an older transcoder makes the stream one whose bitrate can't change.
- **Memory:** 30 minutes per server on the TV
  (`pley.autoQuality.<server>`), used when the server doesn't answer.
- **Speed test:** 32 MiB read through XMLHttpRequest progress, stopped 2 s
  after the first byte (or 1 s without one), so it delays the start by
  about 3.5 s at most; not repeated within 10 minutes.

**Switching streams** (a quality picked, an audio track the engine can't
switch, burned-in picture subtitles, Automatic's step from the original;
`player/prepare.ts`): the new session starts beside the old one
(`keep_previous`). The app waits up to 20 s for its playlist and the
segment at start + 4 s (a `HEAD` request), then moves the engine there. The
screen is black meanwhile, and the loading ring waits 2.5 s. With no
transcode free for two (`503`), the old session ends first.

### Live TV

Live TV works as in the [Android app](ANDROID.md#live-tv). The rail
shows it after Home, remembered on the TV so it shows at launch. On this TV:

- **Categories:** Left from the channels opens them, with Search at their
  head (the TV's own keyboard). Back leaves search results for the category
  before.
- **Guide:** two and a half hours across. Left and Right go programme by
  programme, rewind and fast forward (or Previous and Next) move two hours,
  CH+ and CH− a screen of channels, and Back returns to now before it goes
  to the rail. A long OK or the menu key opens a programme's details.
- **Tuning:** `POST …/watch` with this TV's id (`pley.installId`) and
  `keyed_urls`, so AVPlay can play the playlist without the token. The app
  waits for the playlist, then plays the server's live HLS from its live
  edge.
- **The player:** Up and Down, CH+ and CH−, Previous and Next change
  channel. The menu key (and CH LIST) opens the channel list beside the
  picture, which moves aside into the 520 px left of it.
- **Leaving:** `DELETE /api/live/sessions/{id}` on Back, on each change of
  channel, and when the TV hides the app.
- **Failures:** a player failure is retried twice (after 1 s, then 3 s).
  The TV's player says nothing of a playlist gone (`410`), so a failure,
  or a stall of 5 s, asks the server why watching ended
  (`GET /api/live/sessions/{id}/plan`) and shows its words; a stall of 20 s
  is a failure.
- **Taken over:** the server's `live_taken_over` event for the channel
  playing stops the player at once, before its stream breaks off, so it
  isn't tuned again into a connection that is gone. The card says "Live TV
  stopped" with the server's words, and Watch again in place of Try again.
  The card stays when the app is hidden and shown again; nothing is tuned
  until Watch again.

### Requests

Where the server takes requests ([API.md](API.md#requests)), the rail has
Discover after Home for a profile that may ask, else Your requests
(Requests, for administrators) for one with requests to see. Its number
is what has news since the profile last looked, and for administrators
also what waits for them, in red while some failed.

- **What the profile may do:** `GET /api/requests/summary` at start, on
  reconnecting, after a profile switch (the event stream starts over with
  the new profile's token then, so none of the last one's news comes) and
  1 s after a burst of request events, kept on the TV (`pley.requests`)
  so the rail has its entry before the server answers.
  A `404` (an older server) or `403` leaves requests out until the app
  reconnects. Sign-in asks for `requests approvals` only where the server
  lists them (`scopes_supported`); with a sign-in from before, Discover
  says "Sign in again to ask for movies and shows".
- **Live:** pages showing a title hear of its events 300 ms after the
  last; while the event stream is down the summary is asked every minute,
  and the pages when it moved. Rows already drawn change a title's badge
  from the requests the events name that the profile made or follows,
  each asked for once in a round: a movie ready is In library at once,
  but for a show whose request is wholly ready (it may still lack
  seasons nobody asked for) and for a request the profile no longer sees,
  the title's details (`GET /api/discover/{kind}/{tmdb_id}`) say, asked
  in the same round. What became ready to watch pops up once
  (`pley.requestNews.<server>.<profile>`), each batch of a show's
  seasons too, with the title's poster, Open and Close, for 10 s; it
  waits while a video plays (the last three).
  What came while the app wasn't listening (before it started, while the
  event stream was down) is found among the 20 newest unseen requests.
  Other news shows only as the number and in the lists.
- **Discover:** a hero for the first trending title, then Trending this
  week, Popular movies, Popular shows, Coming to cinemas and Popular
  anime, each loaded as it comes near the screen, then Your requests;
  at the top, the way to the requests with their number. It has no
  search field: Search finds TMDB's titles. Posters say In library,
  Partly in library, Requested or Declined. A list reads TMDB's pages
  until it has more than a screenful, 5 at most at a time, since an age
  limit can leave page after page empty: a row shows what they had, and
  a grid (See all, and Search's Discover row's) ends in Show more,
  which reads 5 more, or in Retry after a page failed. Back on the page,
  what failed is asked again.
- **A title's page:** backdrop, poster, facts, cast and seasons with their
  state, and one main action: Request, Request more seasons, Tell me too,
  Open (in a library), or why not (declined, with when it may be asked
  again; the limit, with when it frees up; no access). Requested and
  Following show the stage of the request beside them, with a download's
  progress. A show's seasons are ticked in a panel beside the page; those
  in a library or declined can't be.
- **Requests:** tabs (Needs attention first for administrators, then All,
  Pending, Approved, Available, Declined, Failed), administrators'
  filters (movies or shows, a person, a title's words) and cards with
  poster, seasons, stage, who asked and when. OK on a card opens what may
  be done:
  Withdraw, Stop following, Approve, Decline, Decline with a reason… (the
  TV's keyboard), Mark as added by hand, Delete, Try again, Request
  again. Approve is plain: the settings' profile and folder, the seasons
  asked.
- **Search:** under the libraries' results, Discover for a profile that
  may ask: up to 20 movies and shows the libraries lack, read from at
  most 3 of TMDB's pages, asked beside the libraries so they never wait
  for it. A title opens its page, where it is requested. See all opens
  a grid titled Results for “…” with every title TMDB has for the words,
  each with its badge, and Back goes back to Search. When TMDB doesn't
  answer, the row says why ("TMDB isn't answering", "TMDB refused this
  server's key", "Requests aren't open to this account"), with Retry
  where it can help. TMDB gets the words cut to the server's 200
  characters. A movie's or show's page in a library shows its request's
  stage, and Request more seasons for a show missing some.
- **Answers:** a requests route's `503` with Pley's `{ code }` is an
  answer, never the server being away: "TMDB isn't answering" with
  Retry, "TMDB refused this server's key", "Requests are off". The app
  writes "in 3 days" and "yesterday" itself, as the TVs' engines have no
  `Intl.RelativeTimeFormat`. Settings → About says the app uses the TMDB
  API.

## Building and testing

### Commands

With Bun, from the repository root (`bun install` first). A `bunfig.toml`
in each package has Bun run the tools (TypeScript, Vitest).

```sh
bun run tv:check                     # type checks and tests: picture-subtitles, packages/tv, the Samsung app
bun run tizen                        # apps/tizen/out/pley.wgt, signed
bun run --cwd apps/tizen build       # only the app's dist/; --dev keeps it readable
```

- Tests run in Vitest with jsdom.
- **Packaging** needs the Tizen SDK 10.0 CLI
  ([download.tizen.org](https://download.tizen.org/sdk/Installer/tizen-sdk_10.0/);
  Tizen Studio is deprecated), found through `TIZEN_STUDIO` or `tizen` on
  the PATH, and a security profile named by `PLEY_TIZEN_PROFILE` (default
  `pley-dev`).
- **Icons:** `bun scripts/render-icons.mjs` draws Samsung's icons, square
  as the TV's menu draws its other apps. The native app's has the mark
  above the name, as the other apps' tiles there show theirs. One TV may
  have more than one of Pley's Samsung apps, so the web app's says WEB on
  blue under the mark, and a profiling build of the native app (an app of
  its own, `app.pley.tizen.debug`, "Pley (debug)") says DEBUG on yellow.

### Testing without a TV

The **Samsung TV emulator 10.0**
(`download.tizen.org/sdk/extensions/tv_extensions`) has no 4K or DRM, and
it installs only Samsung-certificate packages. Its installer wants Intel
HAXM, which can't run beside Hyper-V; on Windows 11 with Hyper-V, unpack
its packages into the SDK by hand and it runs on WHPX.

### How it works

`packages/tv/src`:

| Path | What |
|---|---|
| `api/`, `net/`, `auth/` | HTTP API, event stream, server discovery, sign-in with a code, profiles and PINs, the server's proof before a token goes ([Telling servers apart](API.md#telling-servers-apart)) |
| `state.ts`, `platform.ts` | What the TV keeps (servers, sign-ins, settings, a per-install id, what requests are for the profile); what the app's shell provides |
| `requests.ts` | Requests' words and rules: badges, a title's one main action, the season picker, stages, what may be done to a request, the server's answers |
| `ui/focus.ts`, `ui/input.ts`, `ui/keys.ts` | Focus, remote keys, a pointer and its wheel |
| `ui/shell/`, `ui/screens/`, `ui/kit/` | The rail, back stack and screens |
| `player/` | Playback: the play request, recovery, quality, Up Next, skip markers, seek previews, sleep timer, reports, music |
| `live/` | Live TV: categories, the guide's pages, a channel on the TV's player |
| `subs/` | Subtitles drawn by the app |
| `caps/` | The server's direct-play rules, so Playback details can say why |

- The picture subtitle decoders are in `packages/picture-subtitles`, which
  the web app uses too: plain TypeScript without dependencies, compiled by
  the TV build for the TV's engine. The native Samsung app has them in C#.
- `apps/tizen` has `facts.ts` (what the TV says about itself), `caps.ts`
  (the capability report, with snapshot tests of real sets), `engine.ts`
  (AVPlay), `platform.ts`, `main.ts`, its manifest (`config.xml`) and
  `scripts/`.
- The server's side is in [API.md](API.md): the `tizen.json` OAuth client,
  [keyed addresses](API.md#keyed-urls) and CORS.

**Built for a 2018 TV:** one script and one stylesheet packaged with the
fonts; the first screen drawn from saved data before any network; compiled
for Chrome 56 with small polyfills; laid out at 960×540 and zoomed to the
TV's graphics plane. Only `transform` and `opacity` animate, rows and grids
draw only what is near the screen, and focus resting 400 ms on a title
prefetches what playing it needs.

### What a Samsung TV plays

From Samsung's per-year
[specifications](https://developer.samsung.com/smarttv/develop/specifications/general-specifications.html)
and field reports:

| Year | Tizen | Chromium | Beyond H.264 and HEVC Main 10 | Notes |
|---|---|---|---|---|
| 2018 | 4.0 | 56 | VP9 Profile 0 and 2 | |
| 2019 | 5.0 | 63 | VP9; 8K HEVC on 8K sets | |
| 2020 | 5.5 | 69 | AV1 | fMP4 in HLS |
| 2021 | 6.0 | 76 | AV1 | Codec queries, HTTP status in player errors; the store's floor |
| 2022 | 6.5 | 85 | AV1 | |
| 2023 | 7.0 | 94 | AV1 | OS upgrades start; developer mode needs a Samsung certificate |
| 2024 | 8.0 | 108 | AV1 | HLS version 10 |
| 2025 | 9.0 | 120 | AV1 | |
| 2026 | 10.0 | 130 | AV1 | "HDR10+ Advanced" on some sets |

- **Containers:** MKV, MP4, TS and older ones (AVI, MOV, ASF); HEVC only
  from MKV, MP4 and TS.
- **HDR:** HDR10 and HDR10+ on all UHD sets, HLG on HDR sets. **No Samsung
  TV shows Dolby Vision**: the server sends the HDR10 base layer or
  converts. `webapis.avinfo.isHdrTvSupport()` says whether the panel shows
  HDR.
- **Sound:** AAC, AC-3, E-AC-3 (5.1 decoded), Opus, AC-4, FLAC, MP3,
  Vorbis, PCM. **No DTS**, decoded or passed through. E-AC-3 with Atmos
  reaches a receiver when the TV's digital output passes Dolby; apps can
  read that setting (`tizen.tvaudiocontrol.getOutputMode()`) but not change
  it. TrueHD doesn't leave an app: on a 2025 S95F the native player lists
  a file's audio tracks without its TrueHD, and the E-AC-3 (Atmos) beside it
  plays instead. The native app switches to a track by its place in the
  player's list, once the player is ready, and Info shows the one playing.
- **`<video>` and MSE** stop H.264 at 1080p; 4K and 8K need AVPlay.
- **AVPlay sends no request headers** but a cookie and a User-Agent, so
  the app plays from [keyed addresses](API.md#keyed-urls).
- **CORS is enforced** for packaged apps (requests from `file://`); the
  server answers CORS for those origins.

Pley's capability report (`apps/tizen/src/caps.ts`,
[version 2](API.md#version-2)) comes from the model year (from the Tizen
version), the panel (4K, 8K, HDR), codec queries on Tizen 6.0 and later,
and the sound output. AC-3 and E-AC-3 are passed through when the TV's
output is set to Dolby; TrueHD and DTS only when Settings → Playback asks.

AVPlay rules the engine keeps: one call at a time while a seek runs; no
seek to 0; stop and close before the next source; the picture placed in
1920×1080 coordinates; the screen saver off during playback. AVPlay, and
the native player too, can say one failure twice (its error, and the call
that failed): the apps act on a session's first only, so the fallbacks go
one step at a time. When the TV hides the app, the session closes and
reopens at the same place.

## Releases

The app is the `samsung` unit in [RELEASING.md](RELEASING.md). The package
is signed with a Samsung certificate, so it is made by hand
(`bun run tizen`); keep the author certificate, since every store update
must be signed with it.

Store requirements (**Samsung Apps TV Seller Office**): a public seller
launches in the United States only; other countries need a partner
contract. Reviewers need a demo server. Mandatory: launch within 10 s (5 s
ideal), no screen saver during playback, subtitles in sync, a message when
the network drops, Back on the first screen asks before exiting, and the
Exit key always exits
([checklist](https://developer.samsung.com/smarttv/develop/development-checklist/mandatory-features.html)).

## Known limits

- No playback speed.
- No subtitle upload or delete.

## The native Samsung app

Pley for Samsung TV, native, is the Samsung app rewritten in C# for Tizen
.NET, so its screens are drawn by the TV's own graphics engine (NUI) rather
than a web engine. It runs on Tizen 9 TVs and is built beside the web app,
which stays the Samsung app until the native one has its features. So far
it has the Android TV app's "Choose your server" screen (servers found on
the network, an address typed on the TV's keyboard, forgetting a server),
its sign-in with a code and a QR code (the server proving itself before
a token goes, [Telling servers apart](API.md#telling-servers-apart), and
saying when the server requires two-step sign-in, which the approving
phone sets up), Who's watching? with PINs, and
Home: the hero, the server's rows and the rail, and item pages: the hero
and its buttons, seasons and episodes, Extras, Cast and More Like This,
and the side panels; library pages (the chips and panels, the filters by
resolution and dynamic range among them, the grid with the A to Z jump,
Recommended and Collections) and the Libraries page;
search (the field and the TV's keyboard, results in rows, each with all
the server finds, up to 200, Live TV's channels in a row before People
that play the one chosen, recent searches); people's pages (the photo, the name and a row per role); the
rail's Collections and a collection's page, with Play and Shuffle; the
watch history page; playlists (their covers, a playlist's page with
moving and removing entries, Rename and Delete, Add to playlist, New
playlist and saving a library's filters as a smart playlist); Settings
(its categories and their rows, the side panels of choices, the web
page's address and QR code, signing out); Diagnostics (the TV's facts and
what it tells the server it plays); Live TV's page (the categories and
their search, the guide and the channels' tiles, a programme's details
and favourites) and its live player (changing channel, number keys, what's
on, the channel list on OK and the controls on OK held, and why a channel
doesn't play); music's artist and
album pages (Play, Shuffle, the albums, the tracks by disc) and Now Playing
(the scrubber, the controls, the sleep timer and the queue), with the
music playing on behind the other pages, driven by the remote's media keys
from any of them, and pausing for a video; and the player: the stream on
the TV's own player, chosen with the TV's capabilities, seeking with Left
and Right with previews of where to (as YouTube's TV app: it waits for OK,
and a held key runs faster the longer it's held), OK showing the
controls, the scrubber with its chapters (Up while a seek waits opens
them as a row of cards to go to) and the control row, the title
for a few seconds as each item begins, text subtitles in Settings' look
(the CAPTIONS key turns them off and on), picture subtitles (PGS, VobSub,
DVB) decoded from the server's `.mks` and drawn as the web app draws them,
burned in by the server only when it can't make that file, the side
panels (Subtitles with Sync and, for text subtitles, their look, Audio,
Quality, the sleep timer, Speed where the TV's player has other speeds, and Info,
where the server's speed sits beside the TV's network speed: the fastest
second of the last ten, from the bytes the TV's network received, since
Tizen's player doesn't say how fast it downloads),
Chapters opening the chapters' cards, Skip Intro and Skip Credits (over the scrubber, where Up
reaches them, as on Android TV), Up
Next (the next episode, or the next in a playlist or collection),
recovering by itself from stalls and a server out of reach, and Try again
when it can't play. Back on its first screen asks "Exit Pley?". Unlike
Android's, its music pauses while the app is hidden, since a hidden app
gives up the TV's player, and plays on from there when the app is back.
The other pages come next.

Tizen's player prepares an HLS stream from its first segment and ignores
`EXT-X-START`, so a film resumed part-way had it fetch the film's start,
and the server make the stream there, before seeking to where it resumed.
A film resumed 10 s or more in is given its playlists by the app instead,
from the TV's loopback, its media playlist beginning at the segment the
film resumes in and naming every segment by the server's own address: the
player prepares there, a second or two before the resume point, with no
seek, and the video still comes straight from the server. The app asks the
server for both playlists as soon as it has the stream, while the TV's
player is still being set up: the server takes a third of a second for
the first. The player then
counts from that segment, which the engine adds back to every position,
seek and duration; a seek before it starts a session there, as any new
stream starts, under its black cover.
The player's calls that hold their caller (making it and giving it its
source, its position, its prepare, a seek, its streams' details) are made
off the UI thread, one at a time, so the screen and its spinner keep
moving while a film opens; those bound to the window (its display, the
picture's region, starting it, letting it go) stay on it.
Tizen's player says neither when its first picture is out nor when it
waits for data, only where it is, so the engine reads its position again
and again and `Player/PlaybackWatch.cs` decides: waiting from its prepare
until the picture moves, after a seek until it moves on from where it
landed (a little off where it was sent), and when it stands still 1.5 s
while it should play; it puts the position on the film's timeline too.
The engine says it is prepared apart (`Opened`), for the audio chosen to
be switched to before it plays.

A channel starts as on Android: its `stream.ts` (the provider's MPEG-TS,
from its last keyframe) is read while the server says what it makes of the
channel, and plays when that is the channel as it is, or with sound every
Samsung TV decodes itself (AC3, E-AC3, MP2), which the server converts for
other players; otherwise, or when it fails, the channel's HLS. The TV's
player can't send the `Authorization` header `stream.ts` needs, and the
server takes an app's token nowhere else, so the app reads it with its
token and hands it on to the player as it comes, on the TV's loopback
address under a random path that lasts one channel (`Net/StreamRelay.cs`).
Live channels start once 0.5 s is buffered, 1.5 s after a stall, as on
Android; films start once 0.5 s is buffered from a server at home and 1 s
from elsewhere, a film resumed part-way (its playlists trimmed) once 1.5 s
is, as its 4K remux ran dry within a second on less. Tizen's player waits
for its buffer after every seek too, not only after a stall, so films wait
for 0.5 s there, where Android waits 1.5 s or 2 s after a stall: a seek
took 2 s, now under one. A film asks the server for its stream before its player
screen is made, which takes the UI thread half a second the first time.

Someone taking over a channel's connection (the server's
`live_taken_over`) stops it at once, as on Android TV: the card says "Live
TV stopped" with the server's words, Watch again first and focused. The
card stays when the app is hidden and shown again; nothing is tuned until
Watch again.

| | |
|---|---|
| Folder | `apps/tizen-native` |
| Package | Tizen .NET app (`.tpk`, id `app.pley.tizen`), named "Pley (native)" on the TV |
| Screens | NUI, Samsung's C# toolkit on its DALi engine |
| Oldest TV | Tizen 9 |

### How it is built

- It is a port of the Android TV app: its screens, its words and its rules
  (sign-in, servers, the API client, the rail, tiles). The web TV app
  (`packages/tv`) is itself a port of the Android one.
- `src/Pley.Tv.Core` is the logic without Tizen (.NET Standard 2.0). It
  runs on any computer, so its tests need no TV; they include the Android
  app's tests for the same rules.
- `src/Pley.Tv.Tizen` is the app: NUI screens on the Core, the TV's player,
  remote keys and keyboard, and packaging.
- The picture subtitle decoders (`src/Pley.Tv.Core/Subtitles/Pictures`)
  are `packages/picture-subtitles` in C#, with its tests. Each bitmap's
  RGBA bytes become a NUI texture (`PixelData`), shown by an image scaled
  from the subtitle plane to the screen.
- The Core's tests also run the fixtures in `packages/tv/test/fixtures`,
  inputs with the answers `packages/tv` gives, so the two Samsung apps
  answer the same questions the same way: `profiles.json` pins "Who's
  watching?", which profile is current and locked, what choosing one
  leads to, where each refusal of a PIN leaves the screen, and the first
  screen a start opens (with Always start as, the PIN pad of the profile
  the TV was left on when it has one); `rail.json` pins the rail's
  entries (up to six libraries, then Libraries; nothing left out for
  Live TV or Now Playing) and how far they scroll for the entry focus is
  on. Where the Android app is
  stricter (a token answer without `token_type`, a device code without
  `expires_in`), the native app follows it, and its test says which
  fixture case it answers differently.

### Setting up

On the computer:

1. The **.NET 8 SDK**. The build fetches Tizen .NET's own build tools
   (`Tizen.NET.Sdk`) from NuGet by itself.
2. The **Tizen SDK 10.0** ([Commands](#commands)), for its CLI (`tizen`,
   `sdb`) and Certificate Manager; `TIZEN_STUDIO` names its folder when
   it isn't on the PATH. In its Package Manager, under Extension SDK,
   install **Samsung Certificate Extension**.

For each TV:

1. **Developer mode:** Apps, `12345` on the remote, Developer mode On
   with the computer's IP address, then restart the TV.
2. **Its DUID**, which the certificate lists: `duid` in
   `http://<TV address>:8001/api/v2/`, or Certificate Manager reads it
   from a TV connected with `sdb connect <TV address>`.
3. **A certificate that lists it.** In Certificate Manager, `+`, Samsung,
   TV: an author certificate (keep its password, and reuse it for later
   profiles), then a distributor certificate with privilege Public and
   the DUIDs of every TV the package is for. The profile's name is
   `PLEY_TIZEN_PROFILE` when packaging. A TV added later takes a new
   profile with the same author certificate and a distributor certificate
   listing all the DUIDs, and a package signed again with it.
4. **Its permit,** once per TV and certificate:
   `tizen install-permit -t <target>`, with the target `sdb devices`
   names.

### Building and installing

```sh
dotnet test apps/tizen-native/test/Pley.Tv.Core.Tests   # the Core's tests
PLEY_TIZEN_PROFILE=<profile> bun run tizen-native        # apps/tizen-native/out/pley-tizen.tpk, signed
tizen install -n pley-tizen.tpk -t <target> -- apps/tizen-native/out
tizen run -p app.pley.tizen -t <target>
bun scripts/check-nuget-licenses.mjs                     # the licences of the packages it ships with
```

`tizen run` brings the app to the front when it is running already; an
install ends it, so installing again starts it afresh. It is named
"Pley (native)" among the TV's apps.

### Profiling

A profiling build measures the app on a TV: what happens when (startup,
pages drawn, the player, Live TV's guide), each time the UI thread is held
for more than 66 ms and what came before it, pictures, memory every 5
seconds, and the frame rate, which also shows over the screen's top right.
It keeps its last marks in the app's data folder, so the marks before a
crash are sent at the next start. Its marks go as lines to a listener on a
computer the TV reaches:

```sh
node apps/tizen-native/scripts/metrics.mjs     # writes apps/tizen-native/out/metrics.log, prints its address
PLEY_TIZEN_METRICS=http://<computer>:7531 bun run tizen-native
```

While the listener runs, the same script drives the app from the computer.
The app asks it for something to do, and the listener holds the ask until
it has a command, so a command reaches the TV at once: a screenshot of its
window, saved in `apps/tizen-native/out` (the video's plane comes out
black), or the remote's keys, pressed 300 ms apart. `--tv` picks the TV
whose model name holds the text given.

```sh
node apps/tizen-native/scripts/metrics.mjs shot                 # prints the screenshot's path
node apps/tizen-native/scripts/metrics.mjs keys Left Down OK    # Up, Down, Left, Right, OK, Back, Menu
node apps/tizen-native/scripts/metrics.mjs --tv S89C shot
```

An animation takes the TV longer to capture than it lasts, so `record`
slows the animations that allow it (those whose code calls
`Profile.Slowed`: the rail, and Home's tiles, rows and hero) as many times
as it says, presses the keys given, and catches an area of the window, in
its pixels, as JPEGs one after another for as long as it says. The frames
are saved in `apps/tizen-native/out/rec-<time>-<model>`, each named with
when it was caught at the animation's own speed; at 20 times slower, a
frame comes every 3 ms of it for half the screen, every 6 for all of it.

```sh
node apps/tizen-native/scripts/metrics.mjs record 20 6000 0,0,960,1080 Right   # 20× slower, 6 s, the left half
```

While a video streams, it marks the network once a second, off the UI
thread: what each interface received, TCP's segments and retransmissions,
the Wi-Fi driver's signal and discards, the access point's signal, link
rate and band, the bytes waiting unread in the TV's sockets (many: the
player reads slowly; none: the network brings slowly) and the player's
buffering. `speed`, with the video paused, has the TV fetch the stream's
next segments itself, one at a time as the player does, then the last one
again (ready on the server, so the network alone), then four at once, and
marks how long each took to start and how fast it came.
`wifi` marks the access point the TV is joined to (its BSSID, which tells
a mesh network's nodes apart) and those of the same network in the TV's
last scan, with their signal, band and rate; `wifi scan` scans first (the
profiling build alone has `network.set` for it), as Tizen lists a network
once in an ordinary scan, however many access points it has.

```sh
node apps/tizen-native/scripts/metrics.mjs --tv S89C speed
node apps/tizen-native/scripts/metrics.mjs --tv S89C wifi
```

A build without `PLEY_TIZEN_METRICS` has none of this: the calls of
`Kit/Profile.cs`, the code behind them (the listener, screenshots, keys,
the watchdog and sampler) and Core's marks are left out when it is compiled.

### Videos on a USB drive

While a drive is plugged into the TV, the rail ends with **USB drive**,
after every other entry; the rail's entries scroll when they don't all
fit, the focused entry fourth from the top, as Android TV's do. The drive's page lists the drive's folders, then
the videos the TV's player opens (MKV, MP4,
M4V, MOV, TS, M2TS, WebM, AVI, MPEG), sorted as a person sorts names;
hidden files and what Windows and macOS leave on a drive aren't shown. A
folder opens over the one before, with ".." first to go back up. The
page's title is the drive's make and model as it reports them ("Kingston
DataTraveler 3.0"), or USB drive when it says nothing; the name given to
the drive isn't to be read, since an app may not open the device. At the
drive's top, the line under the title says "/" and how full the drive is;
in a folder, its path ("/Films/2019"). An empty folder says so in the
middle of the page, under a folder icon, and keeps focus there, so the
rail stays closed; Left opens it.

A video plays in Pley's player as it is, with no server
(`Local/LocalPlayerSource.cs`), so seeking, the panels, the sleep timer and
the subtitles' look are the same; what only a server does isn't there:
converting, seek previews, the next episode. Android TV and Fire TV have
the same page ([ANDROID.md](ANDROID.md#videos-on-a-usb-drive)), with
Media3 reading the file, and without M2TS.

- **Tracks and chapters** come from the file's head: an MKV's Info, Tracks
  and Chapters (`MatroskaHead`), an MP4's movie box (`Mp4Head`). The audio
  it starts with is the default track the TV decodes, else the first it
  decodes (TrueHD and DTS don't play).
- **HDR** is read from the head (Matroska's Colour and Dolby Vision
  configuration, MP4's `colr` and `dvcC`) and from the first HEVC frames
  (`HevcFrames`): HDR10's mastering display, HLG's transfer, HDR10+'s
  metadata and Dolby Vision's RPU, which many files' heads leave out.
  Dolby Vision plays as its HDR10 base layer, profile 7 too: there is no
  server to convert it.
- **Subtitles**: the `.srt` and `.vtt` files beside the video
  (`Film.en.forced.srt`; 2 MB at most, in the encodings the Android app
  reads, `LocalFolder.Decode`), and an MKV's own tracks, read block by
  block through its Cues (`MatroskaSubtitles`): text as WebVTT, pictures
  handed to the decoder as they are. A track still being read is asked for
  again after 1 s, then 2, then every 4 s, and at once after a seek. An
  MKV without Cues for them (FFmpeg writes them for video alone) shows
  only the files beside it.
- **Where it stopped** is kept on the TV; past 95 % it was watched.
- **The TV's player** opens the file at its path. Its buffering is left as
  the TV has it: read or set for a file, the player said Playing and stood
  at 0. A file whose picture never starts says so after 12 s.
- **Thumbnails can't be made on the TV**: frames taken out of a file decode
  in software (15 to 20 s each, 4K not at all), and the player's frame
  capture isn't on Samsung TVs (`multimedia.raw_video`).

### On the TV

- **60 frames a second** is the most an app's screens get, also on panels
  sold as 120 Hz, which reach 120 from HDMI sources. On a 2023 set with
  Tizen 9, DALi updates the screens 60 times a second even with its own
  limit lifted (`DALI_DISABLE_VSYNC_RENDER`,
  `NUIApplication.SetMaximumRenderFrameRate`), and Samsung's cloud gaming
  specification says 60 too. It draws nothing while nothing moves.
- **Packages the TV has itself** (`System.Text.Json` and the rest of .NET)
  are loaded from the TV, whatever version the app carries, so the app must
  ask for one no newer than the TV's: a newer one fails to load
  (`FileLoadException`).
- **NUI objects the app doesn't dispose are freed on the UI thread.** A
  view, animation, timer or picture that .NET collects goes into NUI's
  dispose queue, and the UI thread frees all those collected together just
  after, in no set order. A mistake there closes the app natively, with no
  .NET exception to catch, so the app keeps to three rules:
  - An animation or timer that runs is held until it ends
    (`Kit/Held.cs`, the player's timers): collected while it ran, the TV
    called into it later.
  - A callback NUI calls from native code (a frame callback) is held in a
    field for as long as NUI has it.
  - A view's `Dispose(DisposeTypes)` calls into another NUI object (stops
    an animation or a timer, hides or removes a child) only when the app
    disposes it, `type == DisposeTypes.Explicit`: from the queue, that
    object may have been freed first. `DisposeRulesTests` holds every
    override to this.

## References

- Samsung:
  [AVPlay guide](https://developer.samsung.com/smarttv/develop/guides/multimedia/media-playback/using-avplay.html),
  [AVPlay API](https://developer.samsung.com/smarttv/develop/api-references/samsung-product-api-references/avplay-api.html),
  [web engine](https://developer.samsung.com/smarttv/develop/specifications/web-engine-specifications.html),
  [CORS](https://github.com/Samsung/tizen-docs/blob/master/docs/application/web/guides/w3c/security/cors.md),
  [certificates](https://developer.samsung.com/smarttv/develop/getting-started/setting-up-sdk/creating-certificates.html),
  [TV device setup](https://developer.samsung.com/smarttv/develop/getting-started/using-sdk/tv-device.html),
  [Return and Exit](https://developer.samsung.com/smarttv/develop/guides/fundamentals/terminating-applications.html).
- Field reports:
  [jellyfin-tizen #222](https://github.com/jellyfin/jellyfin-tizen/issues/222),
  [CE Pro on HDR10+](https://www.cepro.com/news/samsung_amazon_hdr10/2495/),
  [BGR on Dolby Vision](https://www.bgr.com/2260222/why-samsung-tv-doesnt-support-dolby-vision/).

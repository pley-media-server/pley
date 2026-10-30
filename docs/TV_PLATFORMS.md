# Pley for Samsung and LG TVs

Pley for Samsung TV and Pley for LG TV are one TV app in two shells.
Everything that doesn't call into the TV is in `packages/tv` (TypeScript
and Preact): the API client, sign-in, focus, screens, the player's logic and
subtitles. Each shell adds its TV's player, remote keys, capability probe
and packaging. Both follow the [Android TV app](ANDROID.md): the same
960×540 layout, D-pad focus, screens and player features.

| | Samsung | LG |
|---|---|---|
| Folder | `apps/tizen` | `apps/webos` |
| Package | Tizen web app (`.wgt`) | webOS web app (`.ipk`, id `app.pley.tv`) |
| Player | AVPlay, Samsung's player, on the hardware video plane | `<video src>`, handed to LG's media pipeline (uMediaServer) |
| Oldest TV | 2018 (Tizen 4.0, Chromium 56); Samsung's store takes new media apps for 2021 (Tizen 6.0) and later | 2018 (webOS 4.0, Chromium 53) |

Neither maker lets outside developers ship native apps, so both apps are web
apps that hand the file's address to the TV's own player, with its hardware
decoders, HDR and sound output. No video goes through a JavaScript player.

Neither app is in a store: both are installed on a TV in developer mode.

- [Install](#install)
- [Using it](#using-it)
- [Building and testing](#building-and-testing)
- [Releases](#releases)
- [Known limits](#known-limits)

## Install

There is no store release: build the package ([Commands](#commands)), then
install it on a TV in developer mode.

### On a Samsung TV

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

### On an LG TV

1. Install **Developer Mode** from LG's store and sign in with an LG
   developer account. Turn Dev Mode Status on (the TV restarts), then Key
   Server on. A session lasts about 1,000 hours and can be extended;
   turning it off removes the apps it installed.
2. `bunx ares-setup-device --add tv -i "host=<TV address>" -i "port=9922" -i "username=prisoner"`
3. `bunx ares-novacom --device tv --getkey`, with the passphrase Developer
   Mode shows.
4. `bunx ares-install -d tv apps/webos/out/app.pley.tv_<version>_all.ipk`,
   then `bunx ares-launch -d tv app.pley.tv`.
   `bunx ares-inspect -d tv -a app.pley.tv --open` opens the web inspector.

LG's CLI (`@webos-tools/cli`) is a dev dependency, installed by
`bun install`.

## Using it

Sign-in, server discovery (a sweep of the TV's subnet), the screens, the
player's keys and panels, and offline behaviour follow the Android TV app.
What differs:

### Remote and pointer

- Next and Previous move between a film's chapters (Next plays a playlist's
  or collection's next title).
- LG's Magic Remote works as a mouse: tiles come into view when pointed
  at, the wheel moves a row a notch, and a click on the player's bar seeks
  there.
- Samsung's `ChannelUp`, `ChannelDown`, `ChannelList` and `Guide` keys are
  registered. LG's CH+ and CH− are key codes 33 and 34 (Page Up and Page
  Down on a keyboard).

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
  the TV has a face for it. Picture subtitles keep their own
  position.
- **Pley draws every subtitle itself:** AVPlay takes external files from
  local paths only, and LG lists WebVTT only, without embedded tracks.
  Picture subtitles (PGS, VobSub, DVB) are decoded from the server's `.mks`
  onto a canvas.
- **Find subtitles** searches OpenSubtitles in the profile's subtitle
  language (or another picked there). The TVs have no file upload and no
  delete.
- **Speed:** 0.5× to 2× on LG, through `playbackRate`, which keeps pitch.
  None on Samsung: AVPlay's speeds are trick play, 2× and up, with no
  promise about sound.

### Quality

The menu, Original and Automatic follow [QUALITY.md](QUALITY.md#the-menu),
in the player's Quality panel and Settings → Playback → Quality. Automatic
is the default until a quality is picked on this TV (`pley.qualityPicked`).

Both TVs' players go black between streams, so Automatic follows the
[rules for such players](QUALITY.md#players-that-go-black-between-streams),
in `packages/tv/src/player/autoquality.ts`. A transcode comes in one size
(neither TV says `hls.variants`).

- **Signals:** a wait is the engine buffering while playing (not a pause,
  seek or new session). LG measures delivery and network speed from its
  buffer; Samsung has only waits and, for a stream, AVPlay's
  `CURRENT_BANDWIDTH`. Neither TV counts dropped frames.
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

Live TV works as in the [Android app](ANDROID.md#live-tv), laid out as
TiviMate does. The rail shows it after Home, remembered on the TV so it
shows at launch. On these TVs:

- **Categories:** Left from the channels opens them, with Search at their
  head (the TV's own keyboard). Back leaves search results for the category
  before.
- **Guide:** two and a half hours across. Left and Right go programme by
  programme, rewind and fast forward (or Previous and Next) move two hours,
  CH+ and CH− a screen of channels, and Back returns to now before it goes
  to the rail. A long OK or the menu key opens a programme's details.
- **Tuning:** `POST …/watch` with this TV's id (`pley.installId`) and
  `keyed_urls`, so AVPlay and LG's pipeline can play the playlist without
  the token. The app waits for the playlist, then plays the server's live
  HLS from its live edge (on LG, with no `playTime` in the `mediaOption`).
- **The player:** Up and Down, CH+ and CH−, Previous and Next change
  channel. The menu key (and Samsung's CH LIST) opens the channel list
  beside the picture, which moves aside into the 520 px left of it.
- **Leaving:** `DELETE /api/live/sessions/{id}` on Back, on each change of
  channel, and when the TV hides the app.
- **Failures:** a player failure is retried twice (after 1 s, then 3 s).
  The TV's player says nothing of a playlist gone (`410`), so a stall of
  5 s asks the server why watching ended
  (`GET /api/live/sessions/{id}/plan`) and shows its words; a stall of 20 s
  is a failure.

## Building and testing

### Commands

With Bun, from the repository root (`bun install` first). A `bunfig.toml`
in each package has Bun run the tools (TypeScript, Vitest, LG's CLI).

```sh
bun run tv:check                     # type checks and tests: picture-subtitles, packages/tv, Samsung, LG
bun run tizen                        # apps/tizen/out/pley.wgt, signed
bun run webos                        # apps/webos/out/app.pley.tv_<version>_all.ipk
bun run --cwd apps/webos build       # only an app's dist/ (apps/tizen too); --dev keeps it readable
```

- Tests run in Vitest with jsdom.
- **Samsung packaging** needs the Tizen SDK 10.0 CLI
  ([download.tizen.org](https://download.tizen.org/sdk/Installer/tizen-sdk_10.0/);
  Tizen Studio is deprecated), found through `TIZEN_STUDIO` or `tizen` on
  the PATH, and a security profile named by `PLEY_TIZEN_PROFILE` (default
  `pley-dev`).
- **Icons:** `bun scripts/render-icons.mjs` draws Samsung's 512×423 icon,
  LG's 80 and 130 px icons and LG's 1920×1080 launch picture.

### Testing without a TV

- **LG app in a browser:** `bun run --cwd apps/webos serve` builds with
  `--dev`, serves on `http://localhost:7650` (`PORT` to change) and proxies
  everything else to a Pley server (`PLEY_SERVER`, default
  `http://127.0.0.1:7640`). Arrows, Enter and Escape (Back) are the remote.
  It plays what the browser plays. `localStorage["pley.screenReader"] =
  "on"` stands in for the TV's screen reader.
- **Samsung TV emulator 10.0** (`download.tizen.org/sdk/extensions/tv_extensions`):
  no 4K or DRM, and it installs only Samsung-certificate packages. Its
  installer wants Intel HAXM, which can't run beside Hyper-V; on Windows 11
  with Hyper-V, unpack its packages into the SDK by hand and it runs on
  WHPX.
- **LG Simulator** (webOS 22 and later): the TV's Chromium without its
  media pipeline, so no `mediaOption`, DRM or TV formats.

### How it works

`packages/tv/src`:

| Path | What |
|---|---|
| `api/`, `net/`, `auth/` | HTTP API, event stream, server discovery, sign-in with a code, profiles and PINs |
| `state.ts`, `platform.ts` | What the TV keeps (servers, sign-ins, settings, a per-install id); what a shell provides |
| `ui/focus.ts`, `ui/input.ts`, `ui/keys.ts` | Focus, remote keys, LG's Magic Remote pointer and wheel |
| `ui/shell/`, `ui/screens/`, `ui/kit/` | The rail, back stack and screens |
| `player/` | Playback: the play request, recovery, quality, Up Next, skip markers, seek previews, sleep timer, reports, music |
| `live/` | Live TV: categories, the guide's pages, a channel on the TV's player |
| `subs/` | Subtitles drawn by the app |
| `caps/` | The server's direct-play rules, so Playback details can say why |

- The picture subtitle decoders are in `packages/picture-subtitles`, which
  the web app uses too: plain TypeScript without dependencies, compiled by
  each TV build for its own engine.
- Each shell has `facts.ts` (what the TV says about itself; LG's system
  services in `luna.ts`), `caps.ts` (the capability report, with snapshot
  tests of real sets), `engine.ts` (AVPlay, or `<video>` with
  `mediaOption`), `platform.ts`, `main.ts`, its manifest (`config.xml`,
  `appinfo.json`) and `scripts/`.
- The server's side is in [API.md](API.md): the `tizen.json` and
  `webos.json` OAuth clients, [keyed addresses](API.md#keyed-urls) and
  CORS.

**Built for a 2018 TV:** one script and one stylesheet packaged with the
fonts; the first screen drawn from saved data before any network; compiled
for Chrome 53 (LG) and 56 (Samsung) with small polyfills; laid out at
960×540 and zoomed to the TV's graphics plane. Only `transform` and
`opacity` animate, rows and grids draw only what is near the screen, and
focus resting 400 ms on a title prefetches what playing it needs.

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
  it. Apps reportedly can't pass TrueHD over eARC.
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
1920×1080 coordinates; the screen saver off during playback. When the TV
hides the app, the session closes and reopens at the same place.

### What an LG TV plays

From LG's per-version
[specifications](https://webostv.developer.lge.com/develop/specifications/web-api-and-web-engine)
and field reports:

| Year | webOS | Chromium | HLS | Notes |
|---|---|---|---|---|
| 2018 | 4.0 | 53 | v5 | DTS decoded |
| 2019 | 4.5 | 53 | v5 | DTS decoded |
| 2020 | 5 | 68 | v7 | AV1 and VP9 at 4K; no DTS |
| 2021 | 6 | 79 | v7 | |
| 2022 | 22 | 87 | v7 | |
| 2023 | 23 | 94 | v7 | DTS on some models; LG's HLS player sends surround as stereo |
| 2024 | 24 | 108 | v7 | Same |
| 2025 | 25 | 120 | v7 | Dolby Vision from MKV; stereo bug fixed; no DTS |
| 2026 | 26 | 132 | v7 | Stereo bug again; `requiredACG` honored |

Some 2022 and later sets get newer webOS versions (webOS Re:New), so the
app goes by the running version, never the model year.

- **Containers:** MKV, MP4, TS, AVI and older ones. H.264 and HEVC Main 10
  at 4K; AV1 and VP9 at 4K from webOS 5; 8K HEVC and AV1 on 8K sets.
- **HDR:** HDR10 and HLG where `webOS.deviceInfo()` says `hdr10`. Dolby
  Vision profiles 5 and 8 where it says `dolbyVision` (profile 7 plays its
  HDR10 base layer); before webOS 25 only from MP4 and TS, so a Dolby
  Vision MKV is requested as a remux (`packages/tv/src/player/plan.ts`).
  **No LG TV shows HDR10+.**
- **Sound:** AAC, AC-3, E-AC-3, Opus, FLAC (2 channels), MP3, PCM. AC-3
  and E-AC-3 (Atmos included) pass through over ARC, eARC or optical when
  the digital output isn't PCM; apps can read these settings but not
  change them. Multichannel PCM and TrueHD never leave an app. **DTS:**
  decoded on 2018–2019 sets, passed through on some 2023–2024 sets,
  otherwise none.
- **Surround in LG's HLS player:** on webOS 23, 24 and 26 it reaches a
  receiver as stereo. Files played as they are aren't affected.
- **Start position** goes in the source's `mediaOption`
  (`option.transmission.playTime.start`); seeking after load buffers from
  the start first. The `<video>` gets one `<source>` whose type carries
  the `mediaOption`: start position, the pipeline's reports
  (`umsmediainfo`, for Playback details) and, for HLS, 4K allowed.
- **No request headers, no cookies:** the app plays from keyed addresses.
- **CORS** varies by firmware, so the server always answers it for
  `file://` and `null` origins, and the app signs in with bearer tokens
  only. Scripts are classic bundles: module scripts fail from `file://`.
- **Permissions:** `requiredACG` in `appinfo.json` lists the system
  services the app calls (honored from webOS 26, required by the store from
  webOS 27): `systemconfig.query`, `settings.query`, `arc.query` and
  `network.query`.

Pley's capability report (`apps/webos/src/caps.ts`) comes from the webOS
version, `webOS.deviceInfo()` (panel, HDR, Dolby Vision, Atmos) and the
sound settings (`com.webos.settingsservice`; the receiver's formats from
`tv.model.edidType`).

## Releases

The apps are the `samsung` and `lg` units in [RELEASING.md](RELEASING.md).
The Samsung package is signed with a Samsung certificate, so it is made by
hand (`bun run tizen`); keep the author certificate, since every store
update must be signed with it. LG signs store builds itself.

Store requirements:

- **Samsung Apps TV Seller Office:** a public seller launches in the
  United States only; other countries need a partner contract. Reviewers
  need a demo server. Mandatory: launch within 10 s (5 s ideal), no screen
  saver during playback, subtitles in sync, a message when the network
  drops, Back on the first screen asks before exiting, and the Exit key
  always exits
  ([checklist](https://developer.samsung.com/smarttv/develop/development-checklist/mandatory-features.html)).
- **LG Seller Lounge:** free registration; review in three stages
  (pretest, function, content), every update re-tested. It needs an IPK, a
  UX scenario, the self-checklist, a privacy policy and a server reviewers
  can sign in to. Back on the first screen opens the TV's exit popup
  (webOS 6 and later; older ones go Home)
  ([self-checklist](https://webostv.developer.lge.com/distribute/app-self-checklist)).

## Known limits

- No playback speed on Samsung.
- No subtitle upload or delete on either TV.

## References

- Samsung:
  [AVPlay guide](https://developer.samsung.com/smarttv/develop/guides/multimedia/media-playback/using-avplay.html),
  [AVPlay API](https://developer.samsung.com/smarttv/develop/api-references/samsung-product-api-references/avplay-api.html),
  [web engine](https://developer.samsung.com/smarttv/develop/specifications/web-engine-specifications.html),
  [CORS](https://github.com/Samsung/tizen-docs/blob/master/docs/application/web/guides/w3c/security/cors.md),
  [certificates](https://developer.samsung.com/smarttv/develop/getting-started/setting-up-sdk/creating-certificates.html),
  [TV device setup](https://developer.samsung.com/smarttv/develop/getting-started/using-sdk/tv-device.html),
  [Return and Exit](https://developer.samsung.com/smarttv/develop/guides/fundamentals/terminating-applications.html).
- LG:
  [Developer Mode app](https://webostv.developer.lge.com/develop/getting-started/developer-mode-app),
  [CLI guide](https://webostv.developer.lge.com/develop/tools/cli-dev-guide),
  [appinfo.json](https://webostv.developer.lge.com/develop/references/appinfo-json),
  [ACG guide](https://webostv.developer.lge.com/develop/guides/acg-guide),
  [streaming protocols](https://webostv.developer.lge.com/develop/specifications/streaming-protocol-drm),
  [resuming media](https://webostv.developer.lge.com/develop/guides/resuming-media-with-mediaoption).
- LG forum threads:
  [multichannel audio in HLS](https://forum.webostv.developer.lge.com/t/stereo-only-for-dolby-vision-video-with-multi-channel-audio-in-hls-streams/23069),
  [CORS in packaged apps](https://forum.webostv.developer.lge.com/t/how-are-we-supposted-to-handle-cors-with-a-non-hosted-app/28241),
  [module scripts under file://](https://forum.webostv.developer.lge.com/t/module-script-loads-blocked-under-file-origin-for-installed-webos-app-no-cors-workaround-found/28351).
- Field reports:
  [jellyfin-tizen #222](https://github.com/jellyfin/jellyfin-tizen/issues/222),
  [jellyfin #16092](https://github.com/jellyfin/jellyfin/issues/16092),
  [CE Pro on HDR10+](https://www.cepro.com/news/samsung_amazon_hdr10/2495/),
  [BGR on Dolby Vision](https://www.bgr.com/2260222/why-samsung-tv-doesnt-support-dolby-vision/),
  [HDTVTest on DTS](https://www.hdtvtest.co.uk/news/lg-quitely-drops-support-for-dts-sound-on-its-2025-t-vs).

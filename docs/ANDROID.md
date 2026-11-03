# Pley for Android

One app, `com.formandelement.pley`, for phones, tablets, Android TV, Google
TV and Fire TV (Fire OS 7 and later), on Android 8 or later. It is native
Kotlin and Jetpack Compose, playing with Media3: `MainActivity` on phones
and tablets, `TvActivity` on TVs. Pley's other TV apps ([Kodi](KODI.md),
[Roku](ROKU.md), [Samsung and LG](TV_PLATFORMS.md)) follow its TV screens.
No store carries it yet, so you sideload it.

- [Install](#install)
- [Using it](#using-it)
- [Building and testing](#building-and-testing)
- [Releases](#releases)
- [Known limits](#known-limits)

## Install

Use the APK attached to an Android release (`pley-android-<version>.apk`),
or a debug build ([Commands](#commands)).

- **From a computer:** `adb install -r <file>.apk`. For a TV, run
  `adb connect <ip>:5555` first.
- **On a phone:** open the APK in a file manager, and let that app install
  apps when asked.
- **On a Fire TV:** turn on the developer options (select the device name
  7 times in Settings → My Fire TV → About), allow "Install unknown apps"
  for the Downloader app, and open the APK's address in Downloader. For
  adb, turn on ADB debugging there too.

Uninstall a debug build once before installing a release over it: they are
signed with different keys. Installing over the earlier web view app (0.1)
keeps its saved servers and asks you to sign in again.

## Using it

### Signing in

Phones sign in on the server's own page in the browser; TVs, and phones
without a browser, show a code and a QR code to approve at the server's
`/link` page, on a phone or computer. When the server requires two-step
sign-in, the sign-in screen says so, and whoever hasn't set it up yet does
it on that page as they sign in (for a code, on the device that approves
it).

### Profiles

A TV asks "Who's watching?" as it starts when the household has more than
one profile, unless Settings' Always start as names one; Switch profile
opens it at any time, on phones too. A profile with a PIN shows a lock and
asks it on a number pad: a wrong one clears the dots, too many say how
long to wait, and a PIN locked after 100 wrong ones in a row stops the
pad. Going on as the profile a TV was left on asks its PIN at a start. A
profile that can see more and has no PIN takes its password: a phone signs
in again in the browser, and a TV shows a code to approve on a phone,
where its owner picks it.

Always start as only skips the question: the TV starts as the profile it
was left on, but never past its PIN. When that profile has one, the start
opens "Who's watching?" on its PIN pad, and Back shows the other profiles.
Whether it has one is the profile's `has_pin` from `GET /api/auth/me`, saved
with the session, so the start reads it from the device; until the TV has
heard it for that profile, the start asks "Who's watching?".

### TVs and Fire TV

- Select opens, holding Select or Menu (☰) opens a title's actions, Back
  goes back.
- **The player**: OK with the controls hidden shows them on the scrubber,
  as Up does (Down shows them on Play/Pause), and OK there plays and
  pauses. It seeks as YouTube's TV app does: Left and Right show the
  controls and move where it goes, 10 s a press (held, faster the longer
  it's held), and it goes there on OK, or by itself 5 s after the last
  key; Back lets it go. Fast forward and rewind (30 s) and the control
  row's 10 s skips go without OK, 400 ms after the last press. OK on the
  scrubber within two seconds of a seek plays on from there; later, it
  pauses. Play/Pause, the button or the remote's key, plays and pauses,
  and a seek waiting still goes. Up while a seek waits shows the chapters
  as a row of cards over the scrubber, on the one it was going to; the
  Chapters button shows them on the one playing. Left and Right go
  through them, OK goes to one, Down or Back closes them.
- **Fire TV** runs the same APK and TV screens. The app spots it by the
  `amazon.hardware.fire_tv` feature and follows Amazon's player guidelines
  (asynchronous decoder queueing, surround from Fire OS's audio setting,
  Alexa's play and pause). Fire OS 16 asks for the nearby-devices
  permission before the app searches the network.
- **Watch Next** (Android TV and Google TV): the home screen's row shows
  the profile's Continue Watching, ten at most.
- **Dolby Vision and HDR** go by the display mode each video plays in: the
  one in use, or the one of its size the TV switches to for the frame rate
  (the device's match-content setting, or Settings → Playback; not for
  items under 10 minutes, nor once switching has timed out twice on that
  display). The app judges this before asking the server, so the server is
  told Dolby Vision only for a video the screen will show it in. A format
  turned off in the system's settings isn't used; Android says so only in
  a mode that lists it, so the app remembers it for each display. Dolby
  Vision the screen doesn't show plays as its HDR10, HLG or SDR base layer
  (a download of profile 5, which has none, on its Dolby Vision decoder),
  and the player's details say why ("HDR10 (Dolby Vision base layer, as
  this screen doesn't show Dolby Vision)"). Their Decoded line names what
  the decoder makes: HEVC and HDR10 for a base layer.

### Audio and subtitles

Unless you pick a track, the server picks the audio in the profile's
language and the best format this device plays. Tracks you pick in the
player are remembered for the show or movie; changing only the subtitles
leaves the audio unpinned.

| Setting | Where | What it does |
|---|---|---|
| Audio language, Subtitles, Subtitle language | Me (phones), Settings (TVs) | The profile's defaults |
| Sync | The subtitle menu | 0.1 s steps, up to 10 s either way, kept per file on the device |
| Subtitle style | Settings, or the player's subtitle menu | Size, background, font, and position (bottom, or raised 5, 10 or 15 %) |
| Find subtitles… | End of the subtitle menu, while streaming | Searches online in the profile's subtitle language |
| Speed | Phones: player settings. TVs: a button at the player's left | 0.5× to 2×, pitch kept, this title only |
| Audio delay | Above the audio tracks (Left and Right on TVs) | Up to 5 s either way in 50 ms steps, this title only |

A chosen font overrides the system's caption style; position applies to
text subtitles only.

Media3 changes the speed of decoded sound only, and a tunneled output
times the picture against the sound itself. So while the speed isn't
Normal, sound that goes to a receiver as it is (AC3, E-AC3, TrueHD, DTS)
is decoded on the device instead, and tunneled playback stops; an audio
delay stops tunneled playback too. Decoding the sound reloads the title
where it is, a short pause, and so does going back to Normal, where the
sound goes to the receiver as it is again; tunneled playback comes back
with Normal and no delay. A decoder counts only where it takes the sound
as it is (its channels and rate). When nothing on the device decodes that
sound, a stream asks the server again with the device's decoders only,
made ready beside the one playing: the server converts the sound and
copies the video. A download has no server to ask, and stays at Normal,
as does a stream the server can't convert (its transcoder away, or every
transcode taken); the speed menu says why, and the title plays on.

### Quality

The menu, Original and Automatic work as in
[QUALITY.md](QUALITY.md#the-menu). On Android:

- The choice is kept per device and network kind: Settings → Quality on
  Wi-Fi and Quality on mobile data on phones, Quality on TVs.
- Automatic asks the server's network memory while preparing; without an
  answer it uses its own, which a restart clears.
- Dropped frames at the original switch to the server's quality.
- With [several sizes](QUALITY.md#several-sizes-adaptive-bitrate), Media3
  moves between them, never above the screen's size.
- A new stream is prepared beside the playing one (`keep_previous`), so
  the last picture stays until it starts.
- Downloads play as they are.

### Live TV

This is how every Pley app shows Live TV; the [Kodi](KODI.md#live-tv),
[Roku](ROKU.md#live-tv) and [Samsung and LG](TV_PLATFORMS.md#live-tv) docs
list only what differs. It appears only where `GET /api/live` says
`available` (a source with channels this profile may watch),
administrators included: in the web app's sidebar, the phone's Libraries
tab, after Home in the TV's rail. It comes and goes as sources are added
or removed (`live_channels_changed`). Sources, the lineup and connections
are set up in the web app's Settings → Live TV ([API.md](API.md#live-tv)).

- **Categories:** Favourites, Recently watched, All channels, then the
  regions in alphabetical order, each opening to its groups ("UK| Sport"),
  then More groups in the provider's order. The category, the view (guide
  or cards) and the last channel are kept per server and profile.
- **Search** looks across every channel by name, number or what's on, and
  offers the groups whose names match. The app's own search has a Live TV
  row too.
- **Paging:** channels come 100 at a time (`GET /api/live/browse`), and the
  guide asks only for the rows in view, six hours at a time, so a lineup of
  50,000 channels opens at once.
- **Live only:** no pause, rewind or seeking; media keys do nothing.
- **Changing channel:** channel up and down stay in the category the player
  was opened from, wrapping round its ends. A typed number tunes that
  channel among all channels.
- **One connection per device:** the player tunes with this device's client
  id, so the server ends the channel watched before.
- **Every connection in use (`409`):** the player lists each connection's
  channel and who holds it, with **Watch here instead** where the server
  allows it (`can_take_over`). A connection taken from you shows the
  server's `410` message.

On Android the player reads the channel's MPEG-TS (`stream.ts`) at once
while it asks for the session's `plan`: a channel the server sends as it is
plays that MPEG-TS from its first keyframe; anything else plays the
server's HLS. Leaving the player ends the session.

### Requests

Where the server takes requests ([API.md](API.md#requests)), people ask
for movies and shows found on TMDB and administrators decide; Radarr and
Sonarr fetch what is approved. On phones and tablets:

- **Ways in**: Search before typing (Discover), Search's Discover row
  after the libraries' results, and Me's rows: Discover, and Your requests
  (Requests for administrators) with its number, which Me's tab carries
  too, in red while a request couldn't be sent. An account that may not
  ask and has no requests sees none of it.
- **Search's Discover row**, after the libraries' results, shows TMDB's
  titles that no library has, or why TMDB can't be searched (not
  answering, its key refused, no access), with Retry where trying again
  may help. See all opens every title TMDB finds for the words, with its
  badge, in a grid under "Results for “dune”", read page by page; Back
  goes back to Search. Words past the server's 200 characters are cut to
  them.
- **Discover**: a hero for the first trending title with its main action;
  rows (Trending this week, Popular movies, Popular shows, Coming to
  cinemas, Popular anime), each read as it comes into view and page by
  page; the person's requests. It has no search of its own.
- **A title's page** has the one main action (Request, Requested,
  Following, Tell me too, Open, Request more seasons, or why not), the
  requests for it with their stage, each season's state, and the cast.
  Request on a show opens "Which seasons?", the missing seasons ticked up
  to what the account may still ask for.
- **Requests**: views with their counts (Needs attention, oldest first,
  for administrators), movies or shows, and for administrators a person,
  their own and a title. Cards show the stage, the download's progress,
  why one was declined, and Watch once it's ready. Approve… picks the
  quality profile, folder and seasons; Decline takes an optional reason;
  Withdraw, Remove and Delete ask first. Pull to read the list again.
- **A library title** from a request shows its stage under its actions,
  and a show partly there offers Request more seasons.
- **News** (ready to watch, approved, declined; for administrators, new
  requests and ones that couldn't be sent) shows as a toast with Open
  while the app is open, never over the player; what came while it was
  closed is told at the next start.
- **Signed in before requests**: Discover offers Sign in again, which asks
  for the `requests` and `approvals` scopes the server now lists.
- The summary follows the event stream; without it, the app reads the
  summary every minute in the foreground and an open list every 30 s.
  Download progress sends no events: a list, a title's page, a library
  title or Discover's requests showing a download are read again every
  10 s while they show.

### Offline and downloads

The app needs no internet. On a phone whose Wi-Fi has no internet, traffic
to the home server still goes over the Wi-Fi (`LocalRoute`).

- **Server out of reach:** screens show what was saved ("Showing what Pley
  had saved. Reconnecting…") and recover on their own. A home address gets
  3 s to connect.
- **Playback that loses the server** plays what it has, then retries for
  90 s.
- **Downloads** (phones and tablets): Download is under Play, beside each
  episode and on a season; they are listed under Me → Downloads. A
  download is the original file (of several versions, the sharpest up to
  1080p the device plays as it is) with its text subtitles. A file the
  device can't play, or any file when Settings → Downloads → Quality picks
  a smaller size, is converted by the server first
  ([API.md](API.md#downloads)). Downloads wait for Wi-Fi or Ethernet
  unless Settings → Downloads allows mobile data. Progress made offline is
  sent when the server is back.

## Building and testing

### Modules

Everything is in `apps/android`, sources in `src/main/kotlin`. Each module
sees only what it builds on.

| Module | What it holds | Builds on |
|---|---|---|
| `:app` | `PleyApplication`, `AppGraph`, the two launchers, Watch Next, the web view clean-up | everything |
| `:core:common` | Address rules, discovery, local network access, device profile, trace names | nothing |
| `:core:model` | The server's requests and answers, events, the capability report (plain Kotlin) | nothing |
| `:core:auth` | Sign-in: the browser page, the TV's code, tokens | common |
| `:core:network` | OkHttp clients, `PleyApi`, server events | common, model, auth |
| `:core:data` | Repositories, snapshots, servers, session, settings | network, auth, model |
| `:core:caps` | What the device plays: decoders, Dolby Vision, HDR, display modes, passthrough | common, model, Media3 |
| `:core:player` | The Media3 engine, playback sessions, subtitles, music | caps, data, network |
| `:core:downloads` | The download queue, its service, offline progress | data |
| `:core:designsystem` | Colours, icons, image loading, tiles (Compose foundation only) | common, model |
| `:core:screens` | One ViewModel per screen, shared by phone and TV; routes (`PleyKey`), `Navigator` | data, player, caps, auth |
| `:core:testing` | Fakes, JSON fixtures from [API.md](API.md), test rules | the contracts |
| `:ui:phone` | The phone and tablet interface (Material 3) | screens, designsystem, player |
| `:ui:tv` | The TV interface (tv-material, the rail, D-pad rules, the TV player) | screens, designsystem, player |
| `:testing:fixtures` | `FixtureServer`, a fake Pley server on 127.0.0.1 | model |
| `:benchmark` | Macrobenchmarks and the Baseline and Startup Profile generators | `:app` |

Rules the build keeps:

- `:ui:tv` never sees Material 3 or `:ui:phone`: their `MaterialTheme`s
  would mix.
- Screen modules meet only through `PleyKey` routes and the `Navigator`.
- Shared build setup is in `build-logic` (convention plugins) and
  `gradle/libs.versions.toml`.
- No native code, so the app runs on Fire TV's 32-bit userspace.
- The only exported components are the two launchers,
  `SignInRedirectActivity`, `MusicService` and `WatchNextImageProvider`
  (`CutoverTest` checks).

### Commands

In `apps/android`, with JDK 17 or newer (on Windows, Android Studio's):

```powershell
$env:JAVA_HOME = "C:\Program Files\Android\Android Studio\jbr"
.\gradlew.bat testDebugUnitTest lintDebug assembleDebug assembleStoreRelease   # as CI does
pwsh ..\..\scripts\check-android.ps1   # the release's libraries, native code and size
.\gradlew.bat :app:generateBaselineProfile   # by hand, on API 33 or later; commit the result
```

- Two flavors: `store` (Google Play and the Amazon Appstore, no
  self-updater) and `direct` (the APK to sideload, with an update check).
- Debug APKs: `app/build/outputs/apk/{store,direct}/debug/`.
- `-Ppley.composeReports=true` writes Compose compiler reports to each
  module's `build/compose/`.
- Baseline and Startup Profiles live in
  `app/src/main/generated/baselineProfiles`. A sideloaded install uses them
  only after the device's overnight compile; to apply them at once, run
  `adb shell cmd package compile -m speed-profile -f com.formandelement.pley`.

### Emulators

| Name | Image | For |
|---|---|---|
| `pley_test` | API 36 phone (`google_apis`) | The phone interface, UI tests |
| `pley-tv-34` | `system-images;android-34;google-tv;x86` on `tv_1080p` | D-pad and focus tests, profiles |
| an API 37 phone | Android 17 | The local network permission |
| (optional) | `system-images;android-36;google-tv;x86_64` | Fire OS 16's level |
| (optional) | `system-images;android-28;android-tv;x86`, 1 GB | Fire OS 7's level; can't play video |

```powershell
sdkmanager "system-images;android-34;google-tv;x86"
avdmanager create avd -n pley-tv-34 -k "system-images;android-34;google-tv;x86" -d tv_1080p
emulator -avd pley-tv-34 -gpu host
```

Emulators can't show HDR, Dolby Vision, display-mode switching,
passthrough or real decoder limits: check those on devices.

### A server to test against

Never test against a Pley server people use: signing in creates grants and
playing writes progress. Unit and UI tests use `FixtureServer`. For a real
server, start a disposable one:

```powershell
$data = Join-Path $env:TEMP "pley-android-test"
pley --data-dir $data reset-password admin <password> --admin
pley --data-dir $data --bind 127.0.0.1:7712 --plex-bind off --no-discovery --no-tray
adb reverse tcp:7712 tcp:7712
```

The app then opens `http://127.0.0.1:7712` (an emulator also reaches the
computer at `http://10.0.2.2:7712`). Delete the folder afterwards.

### Performance

Measure on devices (a mid-range phone, a 1 GB Fire TV stick, a Chromecast
with Google TV HD), never on emulators. `TvStartupBenchmark` starts the TV
app cold ten times against the demo server built into benchmark builds, and
records the time to the first frame and to Home fully drawn, with a
Perfetto trace of each. Run it with adb on one device; a connected Gradle
task installs the app on every attached device.

```powershell
.\gradlew.bat :app:assembleStoreBenchmarkRelease :benchmark:assembleBenchmarkRelease
adb -s <device> install -r app\build\outputs\apk\store\benchmarkRelease\app-store-benchmarkRelease.apk
adb -s <device> install -r benchmark\build\outputs\apk\benchmarkRelease\benchmark-benchmarkRelease.apk
adb -s <device> shell am instrument -w -e class app.pley.android.benchmark.TvStartupBenchmark `
  app.pley.android.benchmark/androidx.test.runner.AndroidJUnitRunner
```

On an emulator, add `-e androidx.benchmark.suppressErrors EMULATOR`. Traces
are in `/storage/emulated/0/Android/data/app.pley.android.benchmark/cache`.

## Releases

Versions, version codes and the release steps are in
[RELEASING.md](RELEASING.md) (the `android` unit). Store pictures and
listings are in [deploy/stores](../deploy/stores/README.md).

Release builds are minified, and signed with the key these environment
variables name (unsigned without them):

| Variable | |
|---|---|
| `PLEY_KEYSTORE` | The keystore file |
| `PLEY_KEYSTORE_PASSWORD` | Its password |
| `PLEY_KEY_ALIAS` | The key's alias |
| `PLEY_KEY_PASSWORD` | The key's password, when not the keystore's |

```powershell
$env:PLEY_KEYSTORE = "$HOME\keys\pley-release.keystore"   # and the others
.\gradlew.bat :app:assembleDirectRelease :app:bundleStoreRelease
```

This writes `app/build/outputs/apk/direct/release/app-direct-release.apk`
(to sideload) and `app/build/outputs/bundle/storeRelease/app-store-release.aab`
(for the stores). The Release workflow builds both with the key from the
repository's secrets: `ANDROID_KEYSTORE_BASE64` (the keystore,
base64-encoded), `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and, when
it differs, `ANDROID_KEY_PASSWORD`.

Make the key once and keep it safe: every update must be signed with it.
With Play App Signing it is Google Play's upload key, which Play can reset
if it is lost.

```sh
keytool -genkeypair -v -keystore pley-release.keystore -alias pley \
  -keyalg RSA -keysize 4096 -validity 10000
base64 -w0 pley-release.keystore   # the ANDROID_KEYSTORE_BASE64 secret
```

## Known limits

- Downloads are on phones and tablets only; TVs stream everything.
- Requests are on phones and tablets only: Android TV and Fire TV have no
  Discover, no requests and none of their news, and their code sign-in
  doesn't ask for the `requests` and `approvals` scopes.
- No Watch Next row on phones or Fire TV.
- Live TV has no picture in picture, and the TV's display mode isn't
  matched for live channels.

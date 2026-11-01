# Pley for Apple

Native apps for iPhone, iPad, Mac and Apple TV, in `apps/apple` (paths
below are relative to it). They are in development and not released yet.

- [What it is](#what-it-is)
- [Screens](#screens)
- [Finding servers](#finding-servers)
- [Plain HTTP and HTTPS](#plain-http-and-https)
- [Signing in](#signing-in)
- [Profiles](#profiles)
- [What each device plays](#what-each-device-plays)
- [Playing](#playing)
- [Building and testing](#building-and-testing)
- [Code layout](#code-layout)
- [A server to test against](#a-server-to-test-against)
- [Known limits](#known-limits)

## What it is

Two apps built from one Xcode project, `Pley.xcodeproj`:

- **Pley**, one app for iPhone, iPad and Mac: native SwiftUI on each (not
  Mac Catalyst, not "Designed for iPad").
- **Pley for Apple TV**, a SwiftUI app built for the focus engine.

They share the bundle ID `com.formandelement.pley`, so they are one
purchase on every Apple device. The oldest systems they run on are iOS and
iPadOS 16, macOS 15 and tvOS 18. On the Mac the app runs in the App Sandbox
with outgoing network connections only.

Requests introduce themselves as
`pleyApple/<version> (<system> <version>; <device>)`, for example
`pleyApple/0.1.0 (iPadOS 16.7; iPad)` or `pleyApple/0.1.0 (tvOS 18.2; Apple TV)`.
The device part is the kind of device (iPhone, iPad, Mac, Apple TV), never
the name someone gave it.

The apps ask for local network access (to find and reach servers at home)
and browse for Bonjour's `_pley._tcp`.

## Screens

Both apps have the same screens, each built for its device: the iPhone,
iPad and Mac app in a navigation stack with Back, the Apple TV app for the
focus engine and the Siri Remote, where Menu goes back and leaves the app
from Home.

- **Servers**: the servers found on this network ("On this network") and
  the ones opened before ("Your servers", which can be forgotten: that
  signs out of the server too), and "Add a server" by its address. A found
  server with the name of a saved one at another address says so. When the
  system refuses local network access, the screen explains how to allow it
  (on iPhone and iPad with a link to Settings).
- **Sign in**: the server's name and address, with a warning when it is
  reached over plain http outside the home network. iPhone, iPad and Mac
  offer "Sign in with your browser"; iPhone and iPad also "Sign in with a
  code". Apple TV shows a code straight away: a QR code of the server's
  `/link` page with the code in it, the code, the page's address and how
  long the code lasts. A code that runs out or is refused offers a new one.
- **Who's watching?**: after signing in, when the household has more than
  one profile, and from Home's menu. Each profile shows its picture, or its
  initials on its colour; one with a PIN asks for it (4 to 8 digits, typed
  on the number pad), one that needs its password says how to choose it,
  and a locked one can't be chosen.
- **Home**: the server's rows for the profile, each a line of posters with
  their titles, drawn at the screen's size from pictures fetched at that
  width. iPhone and iPad pull to refresh. A menu (on Apple TV, buttons
  above the rows) switches profile, goes to the servers, or signs out
  after asking. Choosing a poster opens its page.
- **An item's page**: the poster, the title, the year and running time,
  the file's video and sound ("1080p · H.264 · AAC Stereo") and the
  summary. A show or season shows its on-deck episode ("S1 E2 · Pilot")
  and plays that. **Play**, or **Resume from 12:34** and **Play from
  start** when the item was left part way. Pickers for the version (when
  there are several), the audio track (when there are several) and the
  subtitles; the audio and subtitle choices are saved on the server for
  the item, as on the web. After the player closes, the page reads the
  item again for its new resume point.
- **The player**: the system's player on each device, below.

At start, the app opens the server used last when it is still signed in
to it, and shows the servers otherwise. When a sign-in ends without the
person asking (the server ended it, or the address rule below), the app
goes back to that server's sign-in and says why.

## Finding servers

The apps find servers on the home network by Bonjour: Pley servers
announce `_pley._tcp` ([API.md, Discovery](API.md#discovery)), and the
apps browse for it with Network.framework's `NWBrowser`. They don't send
the `pley?` broadcast the other apps use, so they need no multicast
entitlement. A server that isn't found (another network, Bonjour turned
off on the server, a router that drops it) is added by its address.

The first browse makes the system ask for local network access, with the
reason from the app's Info.plist. If it is refused, the search reports it
as refused rather than as an empty network; Settings → Privacy & Security
→ Local Network turns it back on. Adding a server by its address still
works for servers beyond the home network.

Anyone on the network can announce a `_pley._tcp` service, so an
announcement is only a lead:

- **The TXT record is bounded**: `pley` must be `1` and `id` 1 to 64
  letters, digits and hyphens; the name is cut to 64 characters without
  control or format characters, and falls back to the instance name; a
  version is at most 32 printable characters. A record with more than 16
  keys or 1,300 bytes is ignored, and so are servers beyond the first 32.
- **The address must be private**: the service is resolved by opening a
  connection to it, an IPv4 address preferred, and IPv6 used only when it
  isn't link-local. An address outside the private ranges below is
  dropped.
- **The server must confirm it**: the app asks `GET /api/server` at that
  address, at most 4 at a time, and lists the server only if it answers
  as Pley within 5 seconds. The name and version shown are from that
  answer; the id is from the TXT record, and two announcements with one id
  show once. An announcement that isn't confirmed is checked again after
  5, 15 and 60 seconds while it is still announced, since one slow answer
  would otherwise hide the server until it announces something new.

A server that says goodbye (it stopped, or its announcement went) leaves
the list. Servers are listed by name.

## Plain HTTP and HTTPS

A server is reached over https, or over plain http on the home network.
Plain http carries sign-in codes and tokens only to private addresses:
this device (127.0.0.0/8, `::1`), the home network's ranges (10.0.0.0/8,
172.16.0.0/12, 192.168.0.0/16), link-local addresses (169.254.0.0/16,
`fe80::/10`), Tailscale's 100.64.0.0/10 and IPv6 unique local addresses
(`fc00::/7`), the same rule as the Android app's.

URLSession sends a request as soon as it connects, so the rule is checked
on both sides of it:

- **Before**: an IP address is checked as it is; a name is looked up, and
  every address it has must be private. Otherwise nothing is sent.
- **After**: the address the connection actually reached (from the
  request's metrics) must be private too. If it isn't, the answer is thrown
  away, and a sign-in whose token went with the request ends on the device.

https needs nothing more: certificates are checked by the system, against
its own certificate authorities only.

Tokens go in the `Authorization` header only, never in an address, and
only to the origin they were given for (the same scheme, host and port).
The API follows redirects within that origin only: never from http to
https or back, and never to another host or port. The sign-in endpoints
follow none. No cookies are kept or sent, and nothing is cached on disk.
Every answer has a size limit: 64 KiB for the server's name and the
sign-in endpoints, 8 MiB for the API's JSON and 10 MiB for pictures, which
are shown only as JPEG, PNG, WebP or GIF.

The player's media follows the same rule. AVPlayer is given only a keyed
address the server named in its answer (`/api/media/{key}/…`, on the
server's own origin), checked before AVPlayer has it; over plain http,
its host must pass the check above. The key in the address stands for the
session, so AVPlayer sends no token and no header is added to its
requests.

Apple's App Transport Security allows plain http to IP addresses, names
without dots and `.local` names (`NSAllowsLocalNetworking`); the apps ask
for nothing wider. A server whose http address is a name with dots
(`nas.home.arpa`) is opened by its IP address or over https instead.

## Signing in

The apps sign in with OAuth as Pley's own apps
([API.md, Apps](API.md#apps-oauth-21)), each with its client ID built into
every server: `https://pley-media-server.github.io/pley/oauth/ios.json`,
`tvos.json` and `macos.json`. They ask for `library play playlists
profiles` (the ones the server offers), and hold a viewer's tokens only,
whoever signs in.

Before signing in, the app reads the server's metadata
(`/.well-known/oauth-authorization-server`) and keeps it only when its
issuer is exactly the address the app used and every endpoint is on that
address. A server without it is too old for the app.

- **iPhone, iPad and Mac** sign in on the server's own page, in the system's
  sign-in sheet (`ASWebAuthenticationSession`), with PKCE (S256). The
  browser comes back to `app.pley.apple:/oauth/callback`, which the sheet
  catches itself: the app registers no URL scheme. The answer must carry the
  `state` the app sent and the server's issuer as `iss`. The sheet is a
  private session, so no cookie is shared with Safari or left behind: the
  password is typed at every sign-in.
- **Apple TV** shows a code, `BCDF-GHJK`, and the server's `/link` address,
  where it is typed on a phone or computer; the page must be on the
  server's own address. The app asks the server every 5 seconds, 5 seconds
  more for good after each `slow_down`, until the code is allowed, refused
  or runs out (after 10 minutes).
  iPhone and iPad can sign in with a code too. The Mac can't: the server
  refuses it codes.

A token answer that allows more than the app asked for is revoked at once
and refused. Signing in again to a server ends the sign-in it replaces.

Tokens are kept in the Keychain, one item per server (service
`app.pley.apple.signin`), readable after the first unlock and on this
device only: never synchronised or restored to another device. On the Mac
they go to the data protection keychain; a build signed to run locally
without a team uses the login keychain. The list of servers is kept in the
Keychain the same way (`app.pley.apple.servers`).

Access tokens last an hour and are refreshed a minute before they end, or
when the server refuses one. One refresh runs at a time, and the new
refresh token is kept before the new access token is used. The app is
signed out, and shown sign-in again, when:

- the server refuses a refresh (`invalid_grant`), or refuses a token
  moments after the refresh that gave it;
- a refresh allows more than the sign-in had;
- a request with a token reached an address that isn't private over plain
  http ([Plain HTTP and HTTPS](#plain-http-and-https)).

Signing out revokes the sign-in on the server (best effort) and deletes
the tokens.

## Profiles

A sign-in acts as one profile of the household. The app lists the
household (`GET /api/oauth/profiles`) and moves the sign-in to another
profile with Pley's switch-profile grant, sending the profile's PIN when it
has one. A PIN is 4 to 8 digits, checked before it is sent. On success the
tokens are replaced, as the old ones stop at once; a refusal changes
nothing. The server's reasons are shown in words: a wrong PIN, too many
wrong PINs lately, a PIN locked after too many, a profile that needs its
password, or one that can't be used from the app.

A profile that needs its password is chosen by signing in again: on
iPhone, iPad and Mac in the browser, where the person picks it; on Apple TV
with a new code, picking it on the consent page.

## What each device plays

Before playing, the apps tell the server what the device plays: a version 2
caps report ([API.md, Version 2](API.md#version-2)), the same kind the
Android app sends. The server decides from it whether a file plays as it
is, is repackaged into HLS, or is converted. The report is built from what
the system says and from a table of Apple's devices (`PleyCaps`):

- **Containers**: MP4, M4V, MOV, M4A, MP3, FLAC and WAV play as they are.
  AVPlayer opens no Matroska, so an MKV file is repackaged into HLS, its
  video copied, whenever the device decodes it.
- **Video**: H.264 on every device; HEVC where VideoToolbox decodes it in
  hardware; AV1 where it does too (A17 Pro, A18 and later, M3 and later);
  VP9 only where its hardware decoder is registered (macOS, and iOS and
  tvOS 26.2 and later). The sizes, frame rates, levels and 10-bit come from
  the device's class, since VideoToolbox only says whether a codec is
  decoded in hardware:

  | Class | Devices | H.264 | HEVC | AV1 | Dolby Vision |
  |---|---|---|---|---|---|
  | A8 | Apple TV HD | 1080p60 | no | no | no |
  | A9 | iPhone 6s and SE, iPad 5th generation, the first iPad Pros | 4K30 | 4K30, 8-bit only | no | no |
  | A10 | iPhone 7, iPad 6th and 7th generation | 4K30 | 4K30, 10-bit | no | level 7 |
  | A10X | iPad Pro 2nd generation, Apple TV 4K 1st generation | 4K60 | 4K60, 10-bit | no | level 9 |
  | A11 to A16, M1, M2 | iPhone 8 to 15, the iPads of those years, Apple TV 4K 2nd and 3rd generation, the first Apple silicon Macs | 4K60 | 4K60, 10-bit | no | level 9 |
  | A17 Pro and later, M3 and later | iPhone 15 Pro and later, iPad mini A17 Pro, iPads with M3 or later | 4K60 | 4K60, 10-bit | 4K60, 10-bit | level 9 |
  | Intel | Intel Macs | 4K60 | 4K60, 10-bit, where VideoToolbox decodes it | no | no |

  Devices are matched by model identifier (`iPhone15,2`, `AppleTV14,1`),
  Macs by chip. A device the table doesn't know takes the newest class when
  its model number (a Mac's chip generation) is newer than any the table
  has, else the A11 to A16 class's.
  VideoToolbox can always take a codec away, never add one.
- **HDR**: reported only when AVPlayer says the device is eligible for HDR
  playback (`eligibleForHDRPlayback`), since an HDR-only stream doesn't open
  on an SDR screen, and only with a 10-bit HEVC decoder. iPhone, iPad and
  Apple TV report the HDR modes the system lists (HDR10, HLG, Dolby
  Vision), which on Apple TV are the TV's. The Mac reports HDR10 and HLG
  when the screen its window is on shows more than SDR's white (an XDR or
  HDR display), and Dolby Vision too on Apple silicon. HDR10+ is never
  reported: AVPlayer shows it as HDR10.
- **Dolby Vision**: with a Dolby Vision screen, profiles 5 and 8 at the
  class's level. Never profile 7, whose two layers AVPlayer doesn't decode:
  the server makes it 8.1 for these apps, or plays its HDR10 base layer.
- **Audio**: AAC, MP3, ALAC, FLAC, Opus, AC-3 and E-AC-3 are decoded on the
  device; nothing is passed through, since the system decodes everything
  and sends PCM (or Dolby Atmos in E-AC-3 on Apple TV) to a receiver.
  TrueHD and DTS are converted by the server, to E-AC-3 5.1 when the output
  takes surround sound. The channels are the output's: 6 with spatial
  audio (AirPods), 2 for Bluetooth, headphones and built-in speakers, what
  HDMI reports on Apple TV (2, 6 or 8), and the default output device's on
  the Mac.
- **Subtitles**: text subtitles (SRT, ASS/SSA, WebVTT, MP4 text) are left
  to the app to draw; picture subtitles (PGS, VobSub, DVB) are burned in by
  the server.
- **HLS**: master playlists, a stream's several sizes (AVPlayer moves
  between them), and fMP4 segments before MPEG-TS, which the server sends
  these apps for every HLS stream ([API.md, fMP4 segments](API.md#fmp4-segments)):
  Apple's players take HEVC, HDR and Dolby Vision only from fMP4.
- **Screen and device**: the screen's size in pixels and refresh rate (on
  Apple TV with Match Content's frame rate on, the film and TV rates it
  switches to); the device's form (`phone`, `tablet`, `tv`, `desktop` for
  the Mac), its model identifier and a tier (low, mid or high) from its
  class and memory.

The report is read once and again when something it depends on changes:
the HDR the system reports (a TV switched, a display connected), the
audio route, or on the Mac the screen the player's window moves to. A new
report applies to the next playback.

What a device reports can be seen without playing anything: on a Mac,
`swift run --package-path apps/apple/PleyKit pley-apple-check --print-caps`
prints what its probe read and the report built from it; a debug build of
the apps logs the same once, under the subsystem `app.pley.apple`, category
`caps`. Neither holds personal data.

## Playing

The player is `PlaybackController` (`PleyPlayer`), a port of the Android
app's session player, over AVPlayer. It plays one item, in Original quality
for now: the server picks a file's way to play from the caps report (`mode:
auto`).

**Starting.** The item's details and the device's caps report are read
together. The file is the one asked for, else the one the device plays
best (the Android app's rule: as it is before repackaged before converted,
then one that fits the screen, the largest, the best dynamic range and
the highest bitrate), and it starts at the resume point unless asked to start
over. `POST /api/playback` asks for keyed addresses (`keyed_urls: true`)
and names the audio or subtitle track only where the server needs it. A
repackaged stream whose audio isn't the one chosen is asked for again with
the track named. AVPlayer then opens the keyed address the answer gives,
at the start position exactly.

**Reports.** `POST /api/items/{id}/progress` with the session, on the web
and Android players' cadence:

- `playing` at the first frame, then every 10 seconds while playing (or
  buffering to play);
- `paused` at once on a pause, then every 30 seconds while paused;
- one second after a seek, so the dashboard follows;
- `stopped` at the end, then `DELETE /api/playback/{id}`, which ends any
  conversion.

Reports go out one after the other, in the order they were made, on a
queue that outlives the player, so a player that closes still sends its
last position and ends its session. A new session (a fallback, an expired
session started again) ends the one before first. Closing waits for the
reports at most 2 seconds.

**Answers.** A report answered `410` (`session_stopped`) means the session
was stopped from elsewhere: the player stops, keeps the place with one
last `stopped` report, deletes nothing, and says "Playback was stopped by
the server's administrator", with the administrator's message, without
Try again. The media of a stopped session answers `410` too; the player
asks the session's status before taking that for an expiry. A report
answered `404` means the session expired (90 seconds without one): the
next play or seek starts a new session where it was.

**Reports that can't go out.** A `paused` or `stopped` report the server
can't be reached for is kept on the device, in a file per server and
profile in `Application Support/Pley` (the item, the position, the state
and when it was made: never a token or a session). The newest per item
is kept, for 100 items and 90 days. They are sent with their `age_ms`, so
the server keeps whatever happened to the item since, at the next
playback on that server and profile.

**When it fails.** As on Android:

- A file AVPlayer can't open as it is gets a repackaged stream, then a
  converted one; a video it can't decode goes straight to a conversion.
- An expired session or a broken stream is started again where it was,
  three times, and the count starts over after 5 seconds of playback.
- Stalled for 12 seconds with data loaded (decoding too slow), a file
  played as it is gets a conversion. A stream that stalls is checked on
  the server: a conversion slower than real time suggests a lower
  quality, else the session starts again, and stuck twice it stops.
- A repackaged stream whose time runs on for 5 seconds without a picture
  (AVPlayer plays HEVC copied into MPEG-TS without one) gets a conversion,
  once.

AVPlayer decodes video with or without a view on screen: a video output
is attached to every item, and the frames it gives are counted for that
last rule.

**Leaving the app.** On iPhone, iPad and Apple TV, going to the background
pauses and reports at once. Back after more than a minute, the session is
checked, and one the server forgot starts again where it was, paused.

### The player on each device

Each device shows the system's own player over `PlaybackController`
(`PleyPlayerViews`), so the controls, the scrubber, AirPlay and the
remote's gestures are the ones people know. The system's controls act on
AVPlayer itself; the controller follows them, so a play or pause from the
controls, the Control Center or a headset is reported, and a move of the
scrubber counts as a seek (one after the end starts a new session there).
One playback plays at a time: starting another, signing out, switching
profile or going back to the servers closes the one before, and its
session ends on the server. The player shows the item's title and
episode, and AVPlayer picks no subtitle or audio track of its own.

- **iPhone and iPad**: full screen over the app. The system's Close ends
  the playback. An Options button under the top row (with the controls,
  and while paused) has Quality and Details: how the file plays (Direct
  play, Direct stream or Transcoding, and whether the audio is
  converted), why, and the video. Picture in Picture is off.
- **Apple TV**: full screen, with Quality in the transport bar and a
  Details tab in the info panel (swipe down). Menu closes the player when
  the transport bar isn't showing. The display's frame rate and dynamic
  range follow the video when the Apple TV is set to match content.
- **Mac**: a window of its own per playback, opened from the item's page;
  closing the window ends the session, and starting another playback
  closes it. The controls float over the picture, with Quality and
  Details in their menu, and a Playback menu in the menu bar (Play or
  Pause, Back and Forward 10 Seconds, Quality) acts on the window in
  front. The keys are the web player's: Space or K plays and pauses, ← or
  J and → or L go back and forward 10 seconds, ↑ and ↓ change the volume,
  M mutes, F goes full screen and Esc leaves it. The Mac going to sleep
  pauses, a move to another screen reads the device's caps again, and
  quitting waits for the last reports (2 seconds at most).

Quality has Original only for now.

The player logs under the subsystem `app.pley.apple`, category `player`:
the session's method and the first 8 characters of its id, never a keyed
address or a token.

## Building and testing

Requirements: a Mac with Xcode 16 or later (the commands below were checked
with Xcode 27.2). The apps use Apple's frameworks only: no Swift package is
downloaded.

Signing lives in `Config/Signing.xcconfig`. Without an Apple account, the
Mac app and simulator builds are signed to run on the building Mac only
("Sign to Run Locally"). Keep them signed: an app built with
`CODE_SIGNING_ALLOWED=NO` can't use the Keychain, so it can't keep a
sign-in and opening any server fails with a Keychain error. Only the
package's tests turn signing off. To
sign with an Apple account, create `Config/Local.xcconfig` (git ignores
it) with your team and a bundle ID of your own:

```
DEVELOPMENT_TEAM = ABCDE12345
PLEY_BUNDLE_ID = com.example.pley
CODE_SIGN_IDENTITY[sdk=macosx*] = Apple Development
```

With a free Apple account, apps installed on an iPhone, iPad or Apple TV
stop opening after 7 days and a device takes at most 3 of them; Mac builds
don't expire.

From the repository root:

```sh
# The packages' tests on the Mac
swift test --package-path apps/apple/PleyKit

# The same tests on the simulators (names from `xcrun simctl list devices available`)
IOS_SIM="iPhone 17"; TV_SIM="Apple TV 4K (4th generation)"
(cd apps/apple/PleyKit && xcodebuild test -scheme PleyKit-Package -destination "platform=iOS Simulator,name=$IOS_SIM" CODE_SIGNING_ALLOWED=NO)
(cd apps/apple/PleyKit && xcodebuild test -scheme PleyKit-Package -destination "platform=tvOS Simulator,name=$TV_SIM" CODE_SIGNING_ALLOWED=NO)

# The apps
xcodebuild -project apps/apple/Pley.xcodeproj -scheme Pley   -destination 'generic/platform=iOS Simulator'  build
xcodebuild -project apps/apple/Pley.xcodeproj -scheme PleyTV -destination 'generic/platform=tvOS Simulator' build
xcodebuild -project apps/apple/Pley.xcodeproj -scheme Pley   -destination 'platform=macOS' build

# Run the iPhone app on a simulator
xcrun simctl boot "$IOS_SIM"; open -a Simulator
xcodebuild -project apps/apple/Pley.xcodeproj -scheme Pley -destination "platform=iOS Simulator,name=$IOS_SIM" -derivedDataPath /tmp/pley-dd build
xcrun simctl install booted /tmp/pley-dd/Build/Products/Debug-iphonesimulator/Pley.app
xcrun simctl launch booted com.formandelement.pley
```

The package's tests read the Android app's JSON fixtures
(`apps/android/core/testing/src/main/resources/fixtures`) where they are,
so both apps are tested against the same answers. The network tests talk
to a fixture server the tests start on 127.0.0.1 (two of them where a
second origin is needed), over a real socket, so they need no Pley server
and no network beyond this device.

The caps tests build each recorded device class's report from its probe
(`PleyKit/Tests/PleyCapsTests/Probes`) and compare it with the server's
fixture of the same name (`apps/server/tests/fixtures/caps/apple_<class>_v2.json`),
which the server's tests decide from (`apps/server/tests/playback_apple.rs`):
the report the app sends and the server's answer to it can't drift apart.
After a change to what the apps report, rewrite the fixtures and check the
server's answers:

```sh
PLEY_UPDATE_CAPS_FIXTURES=1 swift test --package-path apps/apple/PleyKit --filter RecordedDevicesTests
cargo test -p pley-server --test playback_apple
```

The recorded probes follow Apple's published specifications; a device's
own reading, from `--print-caps` or a debug build's log, replaces its
class's when one is recorded.

One test finds a server by Bonjour for real: a fixture server announces
`_pley._tcp` on the Mac's loopback interface, and `LocalServers` must find
and confirm it. It needs mDNSResponder and local network access, so it
runs only when asked:

```sh
PLEY_BONJOUR_TEST=1 swift test --package-path apps/apple/PleyKit --filter aServerAnnouncedByBonjourIsFound
```

`dns-sd -B _pley._tcp` lists the servers announced on the network. A
server bound only to 127.0.0.1 announces nothing: announcements go out on
the adapters with addresses on the local network.

## Code layout

| Path | What |
|---|---|
| `Pley.xcodeproj` | The project: two targets, `Pley` and `PleyTV`, with shared schemes of the same names. Its folders are synchronized, so adding a file doesn't change `project.pbxproj` |
| `Config/` | Build settings (`.xcconfig`), the apps' Info.plist keys and the Mac's entitlements; `project.pbxproj` holds no settings |
| `Pley/` | The iPhone, iPad and Mac app's screens |
| `tvOS/` | The Apple TV app's screens |
| `PleyKit/` | One Swift package with everything else, a module per target |

PleyKit's modules, each depending only on those before it:

| Module | What |
|---|---|
| `PleyModel` | The server's answers as `Codable` values: unknown fields are skipped, missing ones read as defaults, and unknown kinds and states still read; the caps report the apps send; the words shown for codecs, tracks, versions and times (`Words`, the Android app's) |
| `PleyCommon` | Server addresses and the server list (the Android app's rules), private addresses, host lookups, the User-Agent, Bonjour (browsing, TXT records, resolving), logging |
| `PleyNetwork` | The HTTP client: one ephemeral URLSession, the plain http rule, redirects, size limits, tokens on the server's own origin with a refresh on `invalid_token`, the API's errors, the sign-in endpoints' client, the server check behind "Add a server", servers found on the network and confirmed (`LocalServers`), and the typed API calls |
| `PleyAuth` | Signing in: the server's metadata, PKCE, the sign-in sheet, sign-in with a code, the tokens (refreshes, sign-out, the Keychain), the scope guard and profile switching |
| `PleyData` | The server list, the open server and its sign-in (`SessionManager`), Home's rows, and pictures made small and cached |
| `PleyCaps` | What this device plays: the probe (VideoToolbox, AVFoundation, the audio route, the screen; a file per platform), the table of Apple's devices, the caps report built from them, and `DeviceCaps`, which reads it again when the screen or audio route changes |
| `PleyPlayer` | Playing: the play request's rules, sessions on the server and their reports (in order, on the app's queue), the reports kept for later, the recovery rules, the playback controller, the AVPlayer engine behind a protocol the tests replace, and the Mac player's keys |
| `PleyPlayerViews` | The system's player around the controller on each platform (`PlayerSurface`: AVPlayerViewController on iPhone, iPad and Apple TV, AVPlayerView on the Mac), its Quality and Details, and the audio session |
| `PleyScreens` | What the screens show and do, shared by both apps without any views: the route between screens, the servers, signing in (with the QR code, from Core Image), profiles, Home and an item's page, as `ObservableObject`s; `AppModel` also owns the one playback at a time, the device's caps and the session reports |
| `PleyTesting` | What the tests share: the fixtures, a fixture server (real HTTP on 127.0.0.1, with a household, the sign-in endpoints, tokens, playback sessions with keyed media, and a journal of the requests it got), small MP4 files made at test time with AVAssetWriter, and the administrator's side of the end-to-end checks |
| `pley-apple-check` | A command-line tool, on the Mac only, that signs in to a real server ([A server to test against](#a-server-to-test-against)), or prints this Mac's caps report (`--print-caps`) |
| `pley-apple-play` | A command-line tool, on the Mac only, that plays a real server's test media through the player ([A server to test against](#a-server-to-test-against)) |

The rules: modules depend only downwards; the two apps never import each
other's code, and their views hold no logic of their own beyond layout (it
lives in `PleyScreens`, where it is tested); no third-party Swift package.

## A server to test against

Never test against a Pley server people use: signing in creates grants.
The package tests use the fixture server. For a real one, a script builds
the server, starts a disposable one and signs in to it as the apps do:

```sh
PLEY_CHECK_PASSWORD=$(openssl rand -hex 16) apps/apple/scripts/check-server.sh
```

It needs Rust and the web app built in `apps/web/dist`. The server gets a
temporary data folder, a free port on 127.0.0.1, an administrator
`checkadmin` with that password, and no Bonjour, Plex port or tray.
`pley-apple-check` then signs in with a code as Apple TV, and again as
iPhone:

1. It checks the server and its metadata, and asks for a code.
2. It allows the code as the administrator, through the consent page's
   endpoints (`/api/oauth/device/verify`, then `/api/oauth/requests/{id}`).
3. It collects the tokens, then fetches `/api/auth/me` (with the app's four
   scopes and no administrator rights), the household and Home.
4. It refreshes once, and checks that `/api/me/apps` names the grant
   ("Pley for Apple TV", device `tvOS 18.2; Apple TV`).
5. It signs out, and checks that the old token gets `401`.

The server is stopped and its folder deleted afterwards.

Playback is checked the same way, with the server's transcoder:

```sh
PLEY_CHECK_PASSWORD=$(openssl rand -hex 16) apps/apple/scripts/check-playback.sh
```

It builds the server and its transcoder, makes the test media with FFmpeg
(`scripts/make-test-media.sh`; set `PLEY_CHECK_MEDIA` to a folder to make
it once and keep it), and starts a disposable server as above, its
transcoder working in the same temporary folder. `pley-apple-play` makes
the Movies and Shows libraries, signs in with a code as the app does, and
plays each file through `PlaybackController` and AVPlayer with this Mac's
caps report, decoding without a window. It checks, through the server's
activity, the session's status and the item:

- an H.264 MP4 plays as it is: pictures, the position after 22 seconds,
  the dashboard showing it playing, still there after 35 seconds paused,
  and after a seek and closing, the place saved within 1.5 seconds and the
  session gone;
- an MKV with H.264 and AC-3 is repackaged, in fMP4 segments
  (`#EXT-X-MAP` and `.m4s` in its playlist);
- HEVC HDR10 is repackaged with `VIDEO-RANGE=PQ` on a screen that shows
  HDR, and converted to SDR, saying so, on one that doesn't;
- MPEG-2 is converted, and DTS repackaged with its audio converted;
- for every recorded Apple device class's caps report
  (`apps/server/tests/fixtures/caps`), the server's decision for six of
  the files (method, converted audio and reasons) is the one
  `playback_apple.rs` expects.

It prints a line per check and stops at the first failure; nothing it
prints, nor the server's lines it shows on a failure, holds a token or a
keyed address.

`scripts/make-test-media.sh <dir>` makes, from FFmpeg's test pictures and
tones: an H.264 MP4 with an SRT beside it, an MP4 with two audio tracks, an
MKV with H.264, AC-3, an SRT and a PGS track (written by
`scripts/make-pgs.py`, since FFmpeg can't encode PGS), HEVC HDR10 with
E-AC-3, MPEG-2 with MP2, H.264 with DTS, AV1 10-bit, and a show's two
episodes with an intro and credits chapters. It needs FFmpeg with libx264,
libx265 and libsvtav1.

To try the apps
by hand, start a server the same way and keep it running:

```sh
data=$(mktemp -d)
pley --data-dir "$data" reset-password admin <password> --admin
pley --data-dir "$data" --bind 127.0.0.1:7712 --plex-bind off --no-discovery --no-tray
```

The simulators share the Mac's network, so the apps open
`http://127.0.0.1:7712`. Leave out `--no-discovery`, and bind a LAN
address, to have it found by Bonjour. Delete the folder afterwards.

## Known limits

- Quality has Original only; no skip intro or credits, Up Next, chapters,
  audio switching inside the player or subtitles drawn over the picture
  yet.
- No seek-bar previews.
- A subtitle isn't shown over AirPlay.
- No Live TV, music or downloads.
- A playback stopped from elsewhere is noticed at the next report, within
  10 seconds.
- Home loads when it is shown, after a profile switch and on pull to
  refresh; it doesn't follow changes on the server as they happen.
- The Mac app's only menu of its own is Playback, and iPad and Mac show
  the same stack of screens as iPhone, without a sidebar.
- The app doesn't ask "Who's watching?" by itself at start, so a profile
  with a PIN stays chosen until someone switches.
- Plain http reaches IP addresses, names without dots and `.local` names
  only; an http name with dots (`nas.home.arpa`) is refused by the system,
  even when it points to the home network. Such a server is reached by its
  IP address or over https.
- No Top Shelf on Apple TV.

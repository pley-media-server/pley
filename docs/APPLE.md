# Pley for Apple

Native apps for iPhone, iPad, Mac and Apple TV, in `apps/apple` (paths
below are relative to it). They are in development and not released yet.

- [What it is](#what-it-is)
- [Plain HTTP and HTTPS](#plain-http-and-https)
- [Building and testing](#building-and-testing)
- [Code layout](#code-layout)
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

Apple's App Transport Security allows plain http to IP addresses, names
without dots and `.local` names (`NSAllowsLocalNetworking`); the apps ask
for nothing wider. A server whose http address is a name with dots
(`nas.home.arpa`) is opened by its IP address or over https instead.

## Building and testing

Requirements: a Mac with Xcode 16 or later (the commands below were checked
with Xcode 27.2). The apps use Apple's frameworks only: no Swift package is
downloaded.

Signing lives in `Config/Signing.xcconfig`. Without an Apple account, the
Mac app is signed to run on the building Mac only ("Sign to Run Locally"),
and simulator builds turn signing off with `CODE_SIGNING_ALLOWED=NO`. To
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
xcodebuild -project apps/apple/Pley.xcodeproj -scheme Pley   -destination 'generic/platform=iOS Simulator'  CODE_SIGNING_ALLOWED=NO build
xcodebuild -project apps/apple/Pley.xcodeproj -scheme PleyTV -destination 'generic/platform=tvOS Simulator' CODE_SIGNING_ALLOWED=NO build
xcodebuild -project apps/apple/Pley.xcodeproj -scheme Pley   -destination 'platform=macOS' build

# Run the iPhone app on a simulator
xcrun simctl boot "$IOS_SIM"; open -a Simulator
xcodebuild -project apps/apple/Pley.xcodeproj -scheme Pley -destination "platform=iOS Simulator,name=$IOS_SIM" -derivedDataPath /tmp/pley-dd CODE_SIGNING_ALLOWED=NO build
xcrun simctl install booted /tmp/pley-dd/Build/Products/Debug-iphonesimulator/Pley.app
xcrun simctl launch booted com.formandelement.pley
```

The package's tests read the Android app's JSON fixtures
(`apps/android/core/testing/src/main/resources/fixtures`) where they are,
so both apps are tested against the same answers. The network tests talk
to a fixture server the tests start on 127.0.0.1 (two of them where a
second origin is needed), over a real socket, so they need no Pley server
and no network beyond this device.

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
| `PleyModel` | The server's answers as `Codable` values: unknown fields are skipped, missing ones read as defaults, and unknown kinds and states still read |
| `PleyCommon` | Server addresses and the server list (the Android app's rules), private addresses, host lookups, the User-Agent, logging |
| `PleyNetwork` | The HTTP client: one ephemeral URLSession, the plain http rule, redirects, size limits, tokens on the server's own origin with a refresh on `invalid_token`, the API's errors, the sign-in endpoints' client, the server check behind "Add a server", and the typed API calls |
| `PleyTesting` | What the tests share: the fixtures, and a fixture server (real HTTP on 127.0.0.1, with a household, tokens and a journal of the requests it got) |

The rules: modules depend only downwards; the two apps never import each
other's code; no third-party Swift package.

## Known limits

- The apps show their version only: finding servers, signing in and
  browsing are being built.
- No playback yet.
- Plain http reaches IP addresses, names without dots and `.local` names
  only; an http name with dots (`nas.home.arpa`) is refused by the system,
  even when it points to the home network. Such a server is reached by its
  IP address or over https.
- No Top Shelf on Apple TV.

# Pley for Apple

Native apps for iPhone, iPad, Mac and Apple TV, in `apps/apple` (paths
below are relative to it). They are in development and not released yet.

- [What it is](#what-it-is)
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
and browse for Bonjour's `_pley._tcp`. App Transport Security allows plain
http to local names only (`NSAllowsLocalNetworking`): IP addresses, names
without dots and `.local` names.

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
so both apps are tested against the same answers.

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
| `PleyTesting` | What the tests share: the fixtures |

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

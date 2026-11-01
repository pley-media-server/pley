# Pley for Apple

Native apps for iPhone, iPad, Mac and Apple TV, in `apps/apple` (paths
below are relative to it). They are in development and not released yet.

- [What it is](#what-it-is)
- [Screens](#screens)
- [Finding servers](#finding-servers)
- [Plain HTTP and HTTPS](#plain-http-and-https)
- [Signing in](#signing-in)
- [Profiles](#profiles)
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
  after asking.

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
  show once.

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
| `PleyModel` | The server's answers as `Codable` values: unknown fields are skipped, missing ones read as defaults, and unknown kinds and states still read |
| `PleyCommon` | Server addresses and the server list (the Android app's rules), private addresses, host lookups, the User-Agent, Bonjour (browsing, TXT records, resolving), logging |
| `PleyNetwork` | The HTTP client: one ephemeral URLSession, the plain http rule, redirects, size limits, tokens on the server's own origin with a refresh on `invalid_token`, the API's errors, the sign-in endpoints' client, the server check behind "Add a server", servers found on the network and confirmed (`LocalServers`), and the typed API calls |
| `PleyAuth` | Signing in: the server's metadata, PKCE, the sign-in sheet, sign-in with a code, the tokens (refreshes, sign-out, the Keychain), the scope guard and profile switching |
| `PleyData` | The server list, the open server and its sign-in (`SessionManager`), Home's rows, and pictures made small and cached |
| `PleyScreens` | What the screens show and do, shared by both apps without any views: the route between screens, the servers, signing in (with the QR code, from Core Image), profiles and Home, as `ObservableObject`s |
| `PleyTesting` | What the tests share: the fixtures, and a fixture server (real HTTP on 127.0.0.1, with a household, the sign-in endpoints, tokens and a journal of the requests it got) |
| `pley-apple-check` | A command-line tool, on the Mac only, that signs in to a real server ([A server to test against](#a-server-to-test-against)) |

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

The server is stopped and its folder deleted afterwards. To try the apps
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

- No playback yet, and no item pages: choosing a poster does nothing.
- Home loads when it is shown, after a profile switch and on pull to
  refresh; it doesn't follow changes on the server as they happen.
- The Mac app has no menus or keyboard shortcuts of its own yet, and iPad
  and Mac show the same stack of screens as iPhone, without a sidebar.
- The app doesn't ask "Who's watching?" by itself at start, so a profile
  with a PIN stays chosen until someone switches.
- Plain http reaches IP addresses, names without dots and `.local` names
  only; an http name with dots (`nas.home.arpa`) is refused by the system,
  even when it points to the home network. Such a server is reached by its
  IP address or over https.
- No Top Shelf on Apple TV.

# Development

How Pley is built, tested and released.

## Architecture

<p align="center">
  <img src="images/architecture.svg" alt="Pley's architecture: the apps, and Jellyfin's, talk to the pley server over HTTP; the server starts the transcoder and talks to it over HTTP and JSON; the transcoder runs FFmpeg, on the GPU where there is one; the server reads metadata sources and IPTV providers, sends requests to Radarr and Sonarr, and scans your media folders, which the transcoder reads too." width="1000">
</p>

The transcoder is a separate program, the only one that runs FFmpeg, and the
server talks to it only over HTTP ([TRANSCODER.md](TRANSCODER.md)).

| Path | What |
|---|---|
| `apps/server` | The media server (crate `pley-server`, binary `pley`) |
| `apps/transcoder` | The FFmpeg engine (crate and binary `pley-transcoder`) |
| `apps/web` | The web app (React, TypeScript, Vite, Tailwind), embedded into the server |
| `apps/desktop` | Pley for Windows, macOS and Linux (Tauri) |
| `apps/android` | Pley for Android phones, tablets and TVs (Kotlin, Compose, Media3) |
| `apps/kodi` | Pley for Kodi, an add-on (Python, GPL-2.0-or-later) |
| `apps/roku` | Pley for Roku (BrightScript, SceneGraph) |
| `apps/tizen`, `apps/webos` | Pley for Samsung TV and Pley for LG TV |
| `apps/tizen-native` | Pley for Samsung TV, native (C#, Tizen .NET, NUI), built beside the web app |
| `packages/tv` | The TV app both share (TypeScript, Preact) |
| `packages/transcoder-api` | Protocol types shared by server and transcoder |
| `deploy/` | Docker, systemd and launchd setups, the Windows installer, store listings |
| `docs/` | Documentation |
| `scripts/` | Versions and releases (`version.mjs`, `release.mjs`), release builds, the license checks, the apps' icons and store pictures (`render-icons.mjs`), marking a pull request's screenshots (`mark-screenshot.mjs`) |

## Requirements

- Rust 1.95 or newer.
- [Bun](https://bun.sh) 1.4 or newer (`package.json` pins 1.4.4) to build the
  web app and the other JavaScript apps. Bun installs packages from
  `bun.lock` and runs their scripts and tools (each package's `bunfig.toml`
  says so).
- Node.js 22 or newer, for the Roku app's tools only.
- The .NET 8 SDK, for the native Samsung app only.
- FFmpeg and ffprobe on `PATH` or configured; 6.1 or newer throttles
  transcodes. For hardware transcoding and GPU tone mapping, use a build
  with NVENC, QSV, AMF or VAAPI and libplacebo: gyan.dev's "full" build on
  Windows, Debian 13's or jellyfin-ffmpeg on Linux, Homebrew's `ffmpeg-full`
  on macOS ([deploy/README.md](../deploy/README.md)).

## Running from source

```powershell
bun install; bun run build                       # build the web app (embedded into the server)
cargo build                                      # build pley and pley-transcoder
.\target\debug\pley.exe --data-dir .\data        # http://localhost:7539
```

Then follow [First start](CONFIGURATION.md#first-start).

## Building and testing

| Command | What |
|---|---|
| `bun run dev` | The web app with hot reload on `http://localhost:5173`, proxying `/api` to the server on port 7539. Debug builds of the server read `apps/web/dist` from disk, so `bun run build` shows up without recompiling |
| `bun install --frozen-lockfile` | Installs exactly what `bun.lock` holds, as CI and the Docker build do |
| `cargo test --workspace` | The Rust tests, including end-to-end tests of both APIs |
| `cargo test -p pley-server --test providers -- --ignored` | Also checks the real TVmaze and MusicBrainz (and TheTVDB with `PLEY_TEST_TVDB_KEY` and, if needed, `PLEY_TEST_TVDB_PIN`) |
| `cargo clippy --workspace --all-targets`, `cargo fmt --all` | Lint and format the Rust code |
| `scripts/check-licenses.ps1` | Checks that no copyleft-only crate is in the server or the transcoder ([LICENSING.md](../LICENSING.md)) |
| `bun run check` | The web app's and desktop app's type check, ESLint and tests (Vitest) |
| `bun run desktop` | Runs the desktop app (on Linux needs WebKitGTK: `libwebkit2gtk-4.1-dev`); `bun run --cwd apps/desktop build` makes its installers |
| `bun run tv:check` | Type-checks and tests the Samsung and LG apps; `bun run tizen` and `bun run webos` package them ([TV_PLATFORMS.md](TV_PLATFORMS.md)) |
| `dotnet test apps/tizen-native/test/Pley.Tv.Core.Tests` | Tests the native Samsung app's logic; `bun run tizen-native` packages it ([TV_PLATFORMS.md](TV_PLATFORMS.md#the-native-samsung-app)) |
| `bun install && bun run check` in `apps/roku` | Compiles, lints and tests the Roku app ([ROKU.md](ROKU.md)) |
| `python -m unittest discover -s tests` in `apps/kodi` | Tests the Kodi add-on ([KODI.md](KODI.md)) |
| `scripts/build.ps1` (or `build.sh`) | Builds the server's release archive locally |
| `scripts/build-spk.sh` | Builds the Synology package from the static Linux archive ([deploy/synology](../deploy/synology/README.md)) |

The Android app builds with the Android SDK and JDK 17 or newer (on Windows,
Android Studio's: `JAVA_HOME=C:\Program Files\Android\Android Studio\jbr`).
In `apps/android`:

```sh
./gradlew testDebugUnitTest lintDebug assembleDebug assembleStoreRelease   # as CI does
pwsh ../../scripts/check-android.ps1                                      # the release's libraries, native code and size
adb install app/build/outputs/apk/store/debug/app-store-debug.apk         # the debug app
```

Builds come in two flavors, `store` and `direct`; [ANDROID.md](ANDROID.md)
has the modules and the rest.

[GitHub Actions](../.github/workflows) runs these checks on every push and
pull request, each app's workflow only when that app (or what it is built
from) changes. The server's runs on Linux, macOS and Windows, with a build on
the oldest supported Rust, the license check and the Docker build
([RELEASING.md](RELEASING.md#ci)).

The README's screenshot (`docs/images/web-home.jpg`) shows a demo library of
made-up titles ([scripts/store-library](../scripts/store-library/README.md)):
`bun scripts/store-library/web.mjs <dir> <address>` takes it from a server
holding it.

## Versions and releases

Each app (the server with its transcoder and web app, desktop, Android,
Samsung, LG, Kodi and Roku) has its own version and `CHANGELOG.md`. A release
takes the apps whose versions went up, under one tag (`release-2026-11-02`):
the Prepare release workflow bumps them in a pull request, and merging it
builds and publishes them. Every app shows the commit it was built from
beside its version, as "0.0.1 (1a2b3c4d5)". [RELEASING.md](RELEASING.md) has
the details.

## Screenshots for a pull request

A pull request that changes what people see shows it: the same screen
before and after, through the same steps, before on the base branch and
after on the pull request's. Each app is captured its own way:

| App | Screenshot |
|---|---|
| Web and desktop | The browser's or the system's own |
| Android, phone and TV | `adb exec-out screencap -p > shot.png` |
| Native Samsung app | Its debug build takes them from the computer: `metrics.mjs shot` ([TV_PLATFORMS.md](TV_PLATFORMS.md), "Profiling"); a video's picture comes out black |
| Kodi | Kodi's own (the `screenshot` action); [KODI.md](KODI.md) has the setting that keeps the whole screen drawn |
| Roku | The developer page's Utilities → Screenshot, of a sideloaded app; a video's picture doesn't come out |
| Samsung web app, LG | A photo of the screen |

`scripts/mark-screenshot.mjs` marks where to look, in the screenshot's own
pixels, and writes a PNG; it takes a phone's JPEG too:

```sh
bun scripts/mark-screenshot.mjs before.png before-marked.png --box 176,40,550,370 --arrow 1000,560,740,410
bun scripts/mark-screenshot.mjs before.png corner.png --zoom 84,416,160,100 --box 88,454,20,60   # 5× there, pixel for pixel
```

The pictures are attached to the pull request, never committed:
`gh pr create --attach ./after.png` (gh 2.102 or later) uploads each one and
points `![alt](./after.png)` in the description at it.

## Contributing

- Run the checks for what you changed ([Building and testing](#building-and-testing));
  CI runs them too.
- Commit messages start with the area they touch, as in the history:
  `transcoder: …`, `kodi, roku: …`, `docs: …`.
- Add a line under `## [Unreleased]` in the changed app's `CHANGELOG.md`
  ([RELEASING.md](RELEASING.md#releasing)).
- Report security issues as [SECURITY.md](../SECURITY.md) describes, not in a
  public issue.

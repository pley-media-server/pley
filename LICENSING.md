# Licensing

Pley is free software under the GNU Affero General Public License, version
3 only ([LICENSE](LICENSE), `AGPL-3.0-only`). Copyright (c) 2026 the Pley
authors. The Kodi add-on is the one exception, and third-party code keeps
its own licences.

In short, the AGPL means:

- Anyone may run, study, change and share Pley, for any purpose.
- Whoever distributes Pley, changed or not (a build, an image, an app),
  passes on the same rights: its source code, under the AGPL.
- Whoever changes Pley and lets people use the changed version over a
  network, a server they host for others say, offers those people its
  source code too (the licence's section 13). Running Pley unchanged asks
  nothing more.

| Component | Path | License | Contents |
|---|---|---|---|
| **Pley server** | `apps/server`, `apps/web` | AGPL-3.0-only | Libraries, scanner, metadata, users, sessions, HTTP API, web UI |
| **Pley transcoder** | `apps/transcoder` | AGPL-3.0-only | Everything that runs FFmpeg: probing, HLS transcoding/remuxing, HDR/Dolby Vision tone mapping, subtitles, thumbnails, metrics |
| **protocol types** | `packages/transcoder-api` | AGPL-3.0-only | Plain serde structs shared by the server and the transcoder |
| **Pley for desktop** | `apps/desktop` | AGPL-3.0-only | A window on a server's web app (Tauri, MIT OR Apache-2.0) |
| **Pley for Android** | `apps/android` | AGPL-3.0-only | A native app for phones, tablets and TVs (Kotlin and Compose): AndroidX, Media3, OkHttp, Coil and kotlinx under Apache-2.0, Lucide's icons under ISC, Nayuki's QR code generator under MIT |
| **Pley for Roku** | `apps/roku` | AGPL-3.0-only | A native SceneGraph app in BrightScript, built with BrighterScript (MIT), which adds its runtime helpers, bslib (MIT), to the package |
| **Pley for Samsung TV** | `apps/tizen`, `packages/tv` | AGPL-3.0-only | A web app for the TV's own engine in TypeScript: Preact and its signals (MIT), Lucide's icons (ISC, and MIT for those from Feather), a QR code generator (MIT) and the Roboto font (OFL-1.1) |
| **Pley for Samsung TV, native** | `apps/tizen-native` | AGPL-3.0-only | A Tizen .NET app in C# with NUI screens, on Tizen.NET (Apache-2.0) and .NET Standard (MIT), which the TV provides, Lucide's icons (ISC, and MIT for those from Feather), Nayuki's QR code generator (MIT, `Net.Codecrete.QrCodeGenerator`) and the Roboto font (OFL-1.1) |
| **Pley for LG TV** | `apps/webos` | AGPL-3.0-only | A native webOS app in Rust, on permissively licensed crates (MIT, Apache-2.0, BSD, ISC, Zlib and the like), with Lucide's icons (ISC, and MIT for those from Feather) and the Roboto font (OFL-1.1); the TV's own libraries and media pipeline do the rest |
| **Pley for Kodi** | `apps/kodi` | **GPL-2.0-or-later** (`plugin.video.pley/LICENSE.txt`) | A Kodi add-on in Python, on Python's standard library alone, with pictures drawn from Lucide's icons under ISC |

## Source code

Pley's source is at <https://github.com/pley-media-server/pley>, and each
release is tagged there (`release-<date>`,
[docs/RELEASING.md](docs/RELEASING.md)). The web app's Account page
links to it, from the server's `GET /api/server`, and the transcoder gives
it in `GET /v1/info` and `--license`. A changed version that others use
over a network points these at its own source.

## The Kodi add-on

The add-on is under Kodi's licence, the GPL version 2 or later, because it
has to be: it runs inside Kodi (GPL-2.0-or-later) and calls Kodi's own
modules (`xbmc`, `xbmcgui`, `xbmcplugin` and the rest), which makes it part
of a work under Kodi's licence. Like the other apps, it talks to the server
over HTTP, as a browser does, so each keeps its own licence. Its code is
never copied into the server or another app, and it ships with its
`LICENSE.txt` and `NOTICES.txt` (the zip of `apps/kodi/package.py` holds its
whole folder, source included).

## FFmpeg

Pley contains no FFmpeg. The transcoder runs `ffmpeg` and `ffprobe` as
separate programs and links none of FFmpeg's libraries: linking them would
make the transcoder one work with FFmpeg, bound by FFmpeg's licence (the
GPL, for the builds Pley uses) as well as Pley's. Where a build of Pley
ships FFmpeg beside it, FFmpeg keeps its own licence:

- **The Docker image** carries pley-ffmpeg
  ([deploy/ffmpeg](deploy/ffmpeg/README.md)): FFmpeg 9 (`--enable-gpl`, so
  GPL) with patches from jellyfin-ffmpeg, which are under FFmpeg's licence
  as the files they change, and libplacebo (LGPL), unmodified, built from
  their release sources against Debian's libraries. Their licences and
  sources, the patches and FFmpeg's build configuration are in
  `/usr/share/doc/ffmpeg` and `/usr/share/doc/libplacebo`. Debian's
  packages keep their licences in `/usr/share/doc/<package>/copyright`;
  their sources are at <https://sources.debian.org>. On amd64 the image
  also has Intel's VAAPI driver from Debian's non-free section
  (`intel-media-va-driver-non-free`, redistributable, with GPU shaders that
  come without source), its licence in
  `/usr/share/doc/intel-media-va-driver-non-free/copyright`.
- **The Windows installer** carries pley-ffmpeg's Windows build
  ([deploy/ffmpeg](deploy/ffmpeg/README.md)): the same FFmpeg and patches as
  the Docker image's, with its libraries built in (`--enable-gpl
  --enable-version3`, so GPLv3). It comes with its `LICENSE.txt`,
  `README.txt` and source, which the installer carries in `ffmpeg\source`:
  FFmpeg's release source, the patches and the script that builds it, its
  configuration, and the libraries built in with their versions and the
  image they come from (BtbN/FFmpeg-Builds, whose scripts name each
  library's source; libzvbi's source, which the script builds itself, is
  there too). Whoever distributes the installer must also offer that source
  (GPLv3 §6; <https://ffmpeg.org/legal.html>), which the installer does.
  An installer can be built with another FFmpeg
  ([deploy/windows/README.md](deploy/windows/README.md)), with that build's
  licence and source; or ship without FFmpeg and let users install it: the
  transcoder finds it on `PATH` or via `--ffmpeg`/`--ffprobe`.
- **The Synology package** carries no FFmpeg: it runs the one in
  SynoCommunity's `ffmpeg7` or `ffmpeg8` package, which the user installs
  from SynoCommunity's package source under that package's own licence
  ([deploy/synology/README.md](deploy/synology/README.md)).
- **The other server archives** don't contain FFmpeg.

## What keeps it this way

Pley is under the AGPL alone, the Kodi add-on aside, and so are
contributions to it ([CONTRIBUTING.md](CONTRIBUTING.md)). A dependency
that is only available under a copyleft licence (GPL, LGPL, AGPL and the
like), or FFmpeg linked in, would bind the programs that use it to that
licence's terms too, and some of them, the GPL version 2 only for one,
can't be combined with the AGPL at all.

- **No copyleft code in Pley's own programs.** `scripts/check-licenses.ps1`
  walks the real dependency graphs of the server, the transcoder, the LG
  TV app and the desktop app and fails on any crate that is only available
  under a copyleft licence (GPL, LGPL, AGPL and the like), and on the
  server depending on the transcoder crate (they are separate programs
  that talk over HTTP, `docs/TRANSCODER.md`). `cargo deny check licenses`
  allows only permissive licences for third-party crates (`deny.toml`). CI
  runs both.
- **Nothing of the TV's in the LG TV app.** It plays through the TV's own
  media pipeline, with no FFmpeg. The TV's libraries it uses (SDL2, EGL and
  GLES, luna-service2, GLib, LG's media libraries) are part of the TV's
  system: the app opens them at run time and neither links nor ships them.
  It links only the TV's C library (glibc and `libgcc_s`), and leaves out
  the webOS toolchain's glibc polyfills, whose licence isn't known (the app
  defines the one function it needs from them). `cargo xtask check`, run
  by CI on the TV build, fails on a crate whose licence `deny.toml` doesn't
  allow, on a binary that would need any other library, and on the
  polyfills.
- **No FFmpeg and no GPL code in the Android app.** It plays with Media3 and
  the device's own decoders; the server's transcoder converts what the
  device can't play. `scripts/check-android.ps1`, run by CI after the release
  build, enforces this:
  - every library in the store release (`:app`'s
    `storeReleaseRuntimeClasspath`) comes from an allowed group: AndroidX
    (Compose and Media3 included), the annotations and Guava pieces AndroidX
    brings (`com.google.guava`, `com.google.errorprone`, `com.google.j2objc`,
    `com.google.code.findbugs`, `org.checkerframework`, `org.jspecify`),
    Kotlin and JetBrains' (`org.jetbrains*`), OkHttp and Okio, Coil and the
    Accompanist drawable painter it uses, and the QR code generator
    (`io.nayuki`). A new group is a decision, made in the script.
  - the release APK has no native code (`lib/**.so`) except two small
    AndroidX libraries: Compose's path iterator
    (`libandroidx.graphics.path.so`) and DataStore's counter
    (`libdatastore_shared_counter.so`). FFmpeg's Media3 extension or mpv
    would bring native code, so neither can slip in under an allowed group.
- **The Samsung TV app names every package it bundles.** Its build fails on
  a package that `packages/tv/src/licenses.ts` doesn't name.
- **Only permissive packages in the native Samsung app.**
  `scripts/check-nuget-licenses.mjs` reads the licence of every NuGet
  package the app ships with, transitive ones included, and fails on one
  that isn't MIT, Apache-2.0, BSD, ISC or Zlib, or whose licence it can't
  tell; a package that puts nothing into the app is named there with why.
  It also fails when the app's `NOTICES.txt` doesn't hold the full licence
  of every package the .tpk carries (`--write` writes it again). CI runs it.
- **Rules to keep:**
  - Never link FFmpeg's libraries into the transcoder, or any GPL library
    into the server, the transcoder or an app; Pley runs FFmpeg as a program.
  - Never add `pley-transcoder` as a dependency of `pley-server`.
  - Never add FFmpeg, mpv or a GPL library to the Android app.
  - Never link the LG TV app against a library of the TV's beyond its C
    library, nor ship one in its package.
  - Never copy the Kodi add-on's code into the server or another app.

## Third-party notices

What each build contains of others' work, and where its notices are:

- The server's, the transcoder's and the desktop app's Rust dependencies
  are permissive (MIT, Apache-2.0, BSD, ISC, Zlib, Unicode-3.0, MPL-2.0
  unmodified). `scripts/check-licenses.ps1` prints the full list.
- The web UI uses React, React Router, TanStack Query and Virtual,
  i18next and react-i18next, and Tailwind (all MIT), hls.js and mpegts.js
  (Apache-2.0) and lucide (ISC). The desktop app's own pages use i18next
  (MIT).
- The Android app uses AndroidX, Media3, Kotlin and kotlinx, OkHttp and
  Okio, Coil, Accompanist and the Guava and annotation pieces AndroidX
  brings (Apache-2.0), Lucide's icons (ISC; the notice ships in the app's
  `assets/licenses/lucide.txt`) and Nayuki's QR code generator (MIT).
- The Roku app ships bslib (MIT), BrighterScript's runtime helpers; its
  notice is in the package (`licenses/bslib.txt`).
- The Kodi add-on uses Python's standard library alone, and pictures drawn
  from Lucide's icons (ISC, and MIT for those Lucide took from Feather); the
  notice ships in the add-on's `NOTICES.txt`, and its `addon.xml` gives the
  licence as `GPL-2.0-or-later AND ISC AND MIT`.
- The Samsung TV app uses Preact and `@preact/signals` (MIT), Lucide's
  icons (ISC, and MIT for those Lucide took from Feather),
  `qrcode-generator` (MIT) and Roboto (OFL-1.1, from
  `@fontsource/roboto`). The build writes the full licences of the packages
  in the bundle into `NOTICES.txt` in `dist/` (so into the .wgt). Settings →
  About lists them.
- The native Samsung app draws Lucide's icons (ISC, and MIT for those
  Lucide took from Feather), copied as SVG files by
  `apps/tizen-native/scripts/icons.mjs` with their licence beside them
  (`shared/res/icons/LICENSE.txt`, in the .tpk).
- The native Samsung app draws its sign-in QR code with
  `Net.Codecrete.QrCodeGenerator` (MIT), Nayuki's generator, as the Android
  app has it.
- The native Samsung app's .tpk carries, in `shared/res`, `NOTICES.txt`
  with the full licences of the NuGet packages in it (written by
  `scripts/check-nuget-licenses.mjs --write` from the build's packages),
  `LICENSE.txt` and this file.
- The native Samsung app draws its text in Roboto (OFL-1.1): the files the
  web TV app ships from `@fontsource/roboto`, unpacked to TrueType by
  `apps/tizen-native/scripts/fonts.mjs`, with their licence beside them
  (`shared/res/fonts/LICENSE.txt`, in the .tpk).
- The LG TV app is built from Rust crates under permissive licences (MIT,
  Apache-2.0, BSD, ISC, Zlib, Unicode-3.0, CDLA-Permissive-2.0 and the
  like, as `deny.toml` allows) and carries Lucide's icons (ISC, and MIT for
  those Lucide took from Feather) and Roboto (OFL-1.1). `cargo xtask
  notices` writes the licences of every crate compiled into the binary, of
  the icons and the typeface, and of Rust's standard library into
  `NOTICES.txt` in the .ipk, beside the icons' and the typeface's own
  licence files. Settings → About lists them.
- The tools that build the JavaScript apps (Bun, Vite, TypeScript and the
  rest) and the LG TV app (cargo, the webOS toolchain) aren't shipped.
- Builds of Pley carry `LICENSE` (as `LICENSE.txt`) and this file.

> This document describes the technical setup. It is not legal advice.

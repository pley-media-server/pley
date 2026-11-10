# Versions, changelogs and releases

Each app (a "unit") has its own version and changelog. A release takes the
units whose versions went up since the last release, each at its own
version, under one tag named after the day it was prepared
(`release-2026-11-02`, then `release-2026-11-02.2` for a second that day),
and one GitHub release, named after that day and what it holds ("Pley
2026-11-02.2: Media Server, Android", by the [units](#units)' short
names), with each unit's notes. Each
unit's notes are folded under its name and version, so readers open only
their app's; a release of one unit shows them open. The first release is `release-2026-11-01`, for 0.1.1; the
versions before it have no tag, and the changelogs link them to the source
at their commit.

## Releasing

1. As you work, write what changed under `## [Unreleased]` in the unit's
   `CHANGELOG.md` (Added, Changed, Fixed, Removed), or draft it from the
   commits with `bun scripts/release.mjs <unit> draft` and edit. The
   entries become the release's notes, so they read as a list: an entry
   doesn't open with the app or the device it is for ("On Android TV, …").
   Entries for one device alone follow the others in their section, under
   one line that names it ("On Android TV:").
2. On GitHub: **Actions → Prepare release → Run workflow**, or
   `gh workflow run prepare-release.yml`. Leave **Units** blank for every
   unit with Unreleased entries but the web Samsung app (`samsung`), or name
   them, each with its own bump where it differs from the default:
   `server kodi:minor`. On the branch
   `release/next`, made from master, it bumps each unit and opens a pull
   request whose description is the release's notes.
3. Review the pull request and merge it. The merge starts the Release
   workflow, which publishes the release.

If master changes a unit in the release before you merge, run Prepare
release again: it remakes the branch from master. Bumping by hand works the
same way: `bun scripts/release.mjs <unit> patch --commit` for each unit on a
branch, then a pull request.

The Release workflow runs on pushes to master that change a unit's version,
and by hand (**Actions → Release → Run workflow**, or `gh workflow run
release.yml`). It releases the units waiting, those whose version isn't the
one they had at the last release tag. It refuses one whose CHANGELOG still
has entries under Unreleased, since those changes would ship without being in
its notes, and warns of commits to a unit after its version was set. It then
runs each unit's own workflow with its release builds, tags the commit, and
publishes the GitHub release with each unit's notes, `SHA256SUMS` and build
provenance (`gh attestation verify <file> --repo pley-media-server/pley`).

- **Keep the GitHub release a draft**, when run by hand, leaves the release
  for you to publish; the tag is made either way.
- A release with the server also pushes the Docker image, attested like the
  files (`oci://ghcr.io/pley-media-server/pley:<version>` for `<file>`), and
  the repository's "Latest" release is the newest with the server.
- Publish tags and publishes nothing until every other job in the run has
  passed: a build that fails, or that GitHub cancels for want of a runner,
  holds back the whole release. **Re-run failed jobs** runs what didn't
  pass, then publishes, reusing the tag if an earlier attempt made it. If
  **What waits** refuses the release, fix master (move Unreleased entries
  left behind under the unit's new version, in a pull request) and run the
  workflow by hand.
- Prepare release commits in the name of whoever runs it. It opens the pull
  request itself only with **Settings → Actions → General → Allow GitHub
  Actions to create and approve pull requests**; without it, it pushes the
  branch and links to where you open the pull request. Checks don't run on a
  pull request it opens (GitHub's rule for its token); the units' workflows
  run on the merge.
- The Android release key comes from the secrets `ANDROID_KEYSTORE_BASE64`,
  `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and `ANDROID_KEY_PASSWORD`
  ([ANDROID.md](ANDROID.md#releases)). Without them, a release has Android's
  notes but no APK or bundle.
- Required reviewers on the `release` environment, if you set any, hold the
  Docker image until someone approves.

## Commands

```sh
bun scripts/version.mjs                  # every unit's version
bun scripts/version.mjs check [unit]     # manifests and CHANGELOG agree (CI runs it)
bun scripts/version.mjs set kodi 0.1.0   # sets one unit's manifests, nothing else

bun scripts/release.mjs                  # each unit: version, the release it waits for, commits since, entries waiting
bun scripts/release.mjs kodi draft       # adds the commits since kodi's version was set to Unreleased
bun scripts/release.mjs kodi patch --dry-run
bun scripts/release.mjs kodi patch --commit
bun scripts/release.mjs kodi notes 0.0.2 # a version's notes
bun scripts/release.mjs pending          # the release HEAD waits for: its tag, units, and what stops it
bun scripts/release.mjs check            # no unit below the version it had at the last release, the README's latest releases right (CI runs it)
bun scripts/release.mjs readme           # writes the README's latest releases from the CHANGELOGs
bun test ./scripts                       # the scripts' tests
```

`release.mjs <unit> <patch|minor|major|X.Y.Z>` sets the version in the
unit's manifests and moves its Unreleased entries under the new version,
dated today and linked to the release's tag: the one the units bumped before
it wait for, else the day's first free one. It puts the version, linked to
that release, in the [README](../README.md#latest-releases)'s table of each
unit's latest release, which `check` holds to the CHANGELOGs and `readme`
writes again. For Kodi it also writes the notes into `addon.xml`'s `<news>`. The release's tag and the units waiting come from
the tags, so fetch them first (`git fetch --tags`).

- `--draft` first adds the commits since the unit's version was set, as
  `draft` does.
- `--since <ref>` drafts from another commit.
- `--date YYYY-MM-DD` sets another date.
- It refuses an empty Unreleased, a version not above the current one, a
  unit already waiting for its release, or manifests that disagree.

## Units

| Unit | What | Version in | Release files |
|---|---|---|---|
| `server` | The server, its transcoder and the web app it embeds | `apps/server/Cargo.toml`, `apps/transcoder/Cargo.toml`, `packages/transcoder-api/Cargo.toml`, `apps/web/package.json` | `pley-server-<version>-<os>-<arch>.zip`/`.tar.gz` (with `linux-x86_64-static`), the Windows installers `…-setup.exe`, the Synology package `…-synology-x86_64.spk`, the Docker image `ghcr.io/pley-media-server/pley:<version>`, `:<major>.<minor>` and `:latest` |
| `desktop` | The desktop app (Tauri) | `apps/desktop/package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` | `pley-desktop-<version>-<os>-<arch>` `.msi`, `-setup.exe`, `.dmg`, `.deb`, `.rpm`, `.AppImage` |
| `android` | Phones, tablets, Android TV, Fire TV | `apps/android/app/build.gradle.kts` (`appVersion`) | `pley-android-<version>.apk`, `pley-android-<version>-play.aab` |
| `samsung` | Samsung TVs, the web app (`apps/tizen`) | `apps/tizen/package.json`, `config.xml` | None: the package is signed with a Samsung certificate, by hand (`bun run --cwd apps/tizen package`) |
| `samsung-native` | Samsung TVs, the native app (`apps/tizen-native`, Tizen .NET) | `apps/tizen-native/Directory.Build.props`, `src/Pley.Tv.Tizen/tizen-manifest.xml` | `pley-samsung-native-<version>.tpk`, with Tizen's default certificate, which its user signs again with their own Samsung one ([TV_PLATFORMS.md](TV_PLATFORMS.md#installing-a-release)) |
| `lg` | LG TVs (`apps/webos`, a native app) | `apps/webos/appinfo.json`, `apps/webos/crates/app/Cargo.toml` (`pley-lg`) | `pley-lg-<version>.ipk` |
| `kodi` | The Kodi add-on | `apps/kodi/plugin.video.pley/addon.xml` | `plugin.video.pley-<version>.zip` |
| `roku` | The Roku app | `apps/roku/manifest` (major, minor, build), `apps/roku/package.json` | `pley-roku-<version>.zip` |

- In a release's name the units are Media Server, Desktop, Android,
  Samsung (the native app; the web one is Samsung (web)), LG, Kodi and
  Roku, in that order.
- The lock files record the versions too; `version.mjs set` updates them.
- The transcoder, the protocol crate and the web app carry the server's
  version: they ship only inside the server's release files. The server and
  transcoder check compatibility with the protocol's `api_version`.
- `packages/tv` and `packages/picture-subtitles` have no version: the apps
  that use them build them in. Neither do the LG app's other crates: they
  are built into its one binary.
- webOS takes three numbers for a version, with no suffix.
- Each unit's `CHANGELOG.md` is in its folder.
- Builds show their commit beside the version, as "0.0.1 (1a2b3c4d5)"
  (`scripts/commit.mjs`, `PLEY_COMMIT`).

### Android's version code

`10,000,000 + major × 1,000,000 + minor × 10,000 + patch × 100`, so 0.0.1 is
10000100 and 1.2.3 is 11020300. Minor and patch stay below 100, and every
code is above 20003, which Google Play already has. The stores take each
code once: to upload the same version again, build it with `PLEY_BUILD`
(1 to 99), which is added to the code.

## CI

Each unit has a workflow named after it. It runs by hand, and on pushes to
master and pull requests that change its paths or its workflow file. Each
workflow's `on.paths` has the exact list.

| Workflow | Runs for changes in |
|---|---|
| Server | `apps/server`, `apps/transcoder`, `packages/transcoder-api`, `apps/web`, `deploy/{docker,ffmpeg,windows,systemd,launchd}` (not their Markdown), `Cargo.*`, the root `package.json`, `LICENSING.md`, `docs/TRANSCODER.md`, the build, installer, license and source scripts |
| Desktop | `apps/desktop`, the root `package.json` |
| Android | `apps/android`, `scripts/check-android.ps1` |
| Samsung TV | `apps/tizen`, `packages/tv`, the root `package.json` and `bun.lock`, `scripts/commit.mjs` |
| Samsung TV (native) | `apps/tizen-native`, `packages/tv/test/fixtures`, `scripts/check-nuget-licenses.mjs`, `scripts/commit.mjs` |
| LG TV | `apps/webos`, `deny.toml`, `rustfmt.toml`, `LICENSE`, `LICENSING.md`, the license and version scripts |
| Kodi | `apps/kodi`, and the server's Rust code: its end-to-end test runs against a server built from the same commit |
| Roku | `apps/roku`, the root `package.json` (Bun's version) |
| FFmpeg | `deploy/ffmpeg` on master: publishes the Docker image's FFmpeg ([deploy/ffmpeg](../deploy/ffmpeg/README.md)) |
| Repository | `.github/workflows/`, the version and release scripts, any `CHANGELOG.md`, `README.md`, a unit's version (the first file in its "Version in") |
| Release | A unit's version on master (the first file in its "Version in") |

Repository also fails a pull request whose merge would leave a unit's
version below the one it had at the last release tag (`release.mjs check`).
A branch carrying a unit's manifests and CHANGELOG from before that release
passes `version.mjs check`, since they agree with each other, yet would take
the release back. What waits for the next release, a higher version or
entries under Unreleased, passes.

A CHANGELOG change alone runs only Repository. Prepare release and Audit
(known vulnerabilities in the lock files) run only by hand.

LG TV runs two jobs. One formats, lints and tests `apps/webos` on Linux
(clippy also with the TV's code, the `tv` feature) and checks its
licences. The other builds the TV binary with the webOS toolchain, pinned
by address and SHA-256 and cached, then runs `cargo xtask check`, packs
the .ipk and reads it back; it keeps the package (`lg-ipk`) and the binary
before stripping (`lg-symbols`) as artifacts. A release adds the package
as `pley-lg-<version>.ipk` ([LG.md](LG.md#releases)).

Samsung TV (native) checks its version, tests the app's logic, builds its
.tpk and checks the licences of its NuGet packages and its `NOTICES.txt`; a
release adds that .tpk as `pley-samsung-native-<version>.tpk`
([TV_PLATFORMS.md](TV_PLATFORMS.md#installing-a-release)).

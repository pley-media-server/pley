# Versions, changelogs and releases

Each app (a "unit") has its own version and changelog. A release takes the
units whose versions went up since the last release, each at its own
version, under one tag named after the day it was prepared
(`release-2026-11-02`, then `release-2026-11-02.2` for a second that day),
and one GitHub release with each unit's notes. Releases up to 0.1.1 were
tagged per unit (`kodi-v0.1.1`).

## Releasing

1. As you work, write what changed under `## [Unreleased]` in the unit's
   `CHANGELOG.md` (Added, Changed, Fixed, Removed), or draft it from the
   commits with `bun scripts/release.mjs <unit> draft` and edit.
2. On GitHub: **Actions → Prepare release → Run workflow**, or
   `gh workflow run prepare-release.yml`. Leave **Units** blank for every
   unit with Unreleased entries but Samsung, or name them, each with its own
   bump where it differs from the default: `server kodi:minor`. On the branch
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
- A release with the server also pushes the Docker image, and the
  repository's "Latest" release is the newest with the server.
- If a run fails after tagging, **Re-run failed jobs** reuses its tag. If it
  fails before, fix master (move Unreleased entries left behind under the
  unit's new version, in a pull request) and run the workflow by hand.
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
bun test ./scripts                       # the scripts' tests
```

`release.mjs <unit> <patch|minor|major|X.Y.Z>` sets the version in the
unit's manifests and moves its Unreleased entries under the new version,
dated today and linked to the release's tag: the one the units bumped before
it wait for, else the day's first free one. For Kodi it also writes the notes
into `addon.xml`'s `<news>`. The release's tag and the units waiting come from
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
| `server` | The server, its transcoder and the web app it embeds | `apps/server/Cargo.toml`, `apps/transcoder/Cargo.toml`, `packages/transcoder-api/Cargo.toml`, `apps/web/package.json` | `pley-server-<version>-<os>-<arch>.zip`/`.tar.gz`, the Windows installers `…-setup.exe`, the Docker image `ghcr.io/pley-media-server/pley:<version>`, `:<major>.<minor>` and `:latest` |
| `desktop` | The desktop app (Tauri) | `apps/desktop/package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` | `pley-desktop-<version>-<os>-<arch>` `.msi`, `-setup.exe`, `.dmg`, `.deb`, `.rpm`, `.AppImage` |
| `android` | Phones, tablets, Android TV, Fire TV | `apps/android/app/build.gradle.kts` (`appVersion`) | `pley-android-<version>.apk`, `pley-android-<version>-play.aab` |
| `samsung` | Samsung TVs (`apps/tizen`) | `apps/tizen/package.json`, `config.xml` | None: the package is signed with a Samsung certificate, by hand (`bun run --cwd apps/tizen package`) |
| `lg` | LG TVs (`apps/webos`) | `apps/webos/package.json`, `appinfo.json` | `pley-lg-<version>.ipk` |
| `kodi` | The Kodi add-on | `apps/kodi/plugin.video.pley/addon.xml` | `plugin.video.pley-<version>.zip` |
| `roku` | The Roku app | `apps/roku/manifest` (major, minor, build), `apps/roku/package.json` | `pley-roku-<version>.zip` |

- The lock files record the versions too; `version.mjs set` updates them.
- The transcoder, the protocol crate and the web app carry the server's
  version: they ship only inside the server's release files. The server and
  transcoder check compatibility with the protocol's `api_version`.
- `packages/tv` and `packages/picture-subtitles` have no version: the apps
  that use them build them in.
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
| Server | `apps/server`, `apps/transcoder`, `packages/transcoder-api`, `apps/web`, `deploy/{docker,windows,systemd,launchd}` (not their Markdown), `Cargo.*`, the root `package.json`, `LICENSING.md`, `docs/TRANSCODER.md`, the build, installer, license and source scripts |
| Desktop | `apps/desktop`, the root `package.json` |
| Android | `apps/android`, `scripts/check-android.ps1` |
| Samsung TV, LG TV | `apps/tizen` or `apps/webos`, `packages/tv`, the root `package.json`, `scripts/commit.mjs` |
| Samsung TV (native) | `apps/tizen-native`, `packages/tv/test/fixtures`, `scripts/check-nuget-licenses.mjs`, `scripts/commit.mjs`; it has no release unit yet |
| Kodi | `apps/kodi`, and the server's Rust code: its end-to-end test runs against a server built from the same commit |
| Roku | `apps/roku`, the root `package.json` (Bun's version) |
| Repository | `.github/workflows/`, the version and release scripts, any `CHANGELOG.md` |
| Release | A unit's version on master (the first file in its "Version in") |

A CHANGELOG change alone runs only Repository. Prepare release and Audit
(known vulnerabilities in the lock files) run only by hand.

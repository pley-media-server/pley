# Versions, changelogs and releases

Each app (a "unit") is released on its own, with its own version, changelog
and tag (`<unit>-v<version>`, such as `kodi-v0.0.2`).

## Releasing

1. As you work, write what changed under `## [Unreleased]` in the unit's
   `CHANGELOG.md` (Added, Changed, Fixed, Removed), or draft it from the
   commits with `bun scripts/release.mjs <unit> draft` and edit.
2. Run `bun scripts/release.mjs <unit> patch --dry-run`, then the same with
   `--commit`, and push the commit to master.
3. On GitHub: **Actions → Release → Run workflow**, and choose the unit.

The Release workflow refuses to run off master, checks that the unit's
manifests and CHANGELOG agree, and stops if the tag exists. It then runs the
unit's own workflow with its release builds, tags the commit, and publishes
the GitHub release with the version's CHANGELOG section as notes,
`SHA256SUMS` and build provenance
(`gh attestation verify <file> --repo hhaidar/pley`).

- **Keep the GitHub release a draft** leaves the release for you to
  publish; the tag is made either way.
- A server release also pushes the Docker image, and the repository's
  "Latest" release is always the server's.
- If a run fails after tagging, **Re-run failed jobs** reuses its tag.
- The Android release key comes from the secrets `ANDROID_KEYSTORE_BASE64`,
  `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_ALIAS` and `ANDROID_KEY_PASSWORD`
  ([ANDROID.md](ANDROID.md#releases)). Without them, an Android release has
  its tag and notes but no APK or bundle.
- Required reviewers on the `release` environment, if you set any, hold the
  Docker image until someone approves.

## Commands

```sh
bun scripts/version.mjs                  # every unit's version
bun scripts/version.mjs check [unit]     # manifests and CHANGELOG agree (CI runs it)
bun scripts/version.mjs set kodi 0.1.0   # sets one unit's manifests, nothing else

bun scripts/release.mjs                  # each unit: version, last tag, commits since, entries waiting
bun scripts/release.mjs kodi draft       # adds the commits since kodi's last tag to Unreleased
bun scripts/release.mjs kodi patch --dry-run
bun scripts/release.mjs kodi patch --commit
bun scripts/release.mjs kodi notes 0.0.2 # a version's notes, as the release shows them
bun test ./scripts                       # the scripts' tests
```

`release.mjs <unit> <patch|minor|major|X.Y.Z>` sets the version in the
unit's manifests and moves its Unreleased entries under the new version,
dated today. For Kodi it also writes the notes into `addon.xml`'s `<news>`.

- `--draft` first adds the commits since the last tag, as `draft` does.
- `--since <ref>` drafts from another commit (needed before a unit's first
  tag, after `git fetch --tags`).
- `--date YYYY-MM-DD` sets another date.
- It refuses an empty Unreleased, a version not above the current one, an
  existing tag, or manifests that disagree.

## Units

| Unit | What | Version in | Release files |
|---|---|---|---|
| `server` | The server, its transcoder and the web app it embeds | `apps/server/Cargo.toml`, `apps/transcoder/Cargo.toml`, `packages/transcoder-api/Cargo.toml`, `apps/web/package.json` | `pley-server-<version>-<os>-<arch>.zip`/`.tar.gz`, the Windows installers `…-setup.exe`, the Docker image `ghcr.io/hhaidar/pley:<version>`, `:<major>.<minor>` and `:latest` |
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
| Kodi | `apps/kodi`, and the server's Rust code: its end-to-end test runs against a server built from the same commit |
| Roku | `apps/roku`, the root `package.json` (Bun's version) |
| Repository | `.github/workflows/`, the version and release scripts, any `CHANGELOG.md` |

A CHANGELOG change alone runs only Repository. Audit (known vulnerabilities
in the lock files) runs only by hand.

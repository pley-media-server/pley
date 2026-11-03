# Instructions for coding agents

Pley is a self-hosted media server: a Rust server (`apps/server`) with the
React web app it embeds (`apps/web`), a separate Rust transcoder that runs
FFmpeg (`apps/transcoder`), and apps for desktop, Android, Kodi, Roku,
Samsung TV and LG TV. [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md) has the
architecture, the repository's layout and every build and test command.

## Before you change something

Read the doc for the part you touch; they say how things work and why.

| Part | Read |
|---|---|
| Server, its HTTP API, app sign-in | [docs/API.md](docs/API.md), [SECURITY.md](SECURITY.md) |
| Transcoder and its protocol | [docs/TRANSCODER.md](docs/TRANSCODER.md) |
| Quality, Automatic, playback decisions | [docs/QUALITY.md](docs/QUALITY.md) |
| Data folder, caches, backups | [docs/STORAGE.md](docs/STORAGE.md) |
| Jellyfin-compatible API, tokens for other apps | [docs/COMPATIBILITY.md](docs/COMPATIBILITY.md) |
| Android, Kodi, Roku, Samsung and LG apps | [docs/ANDROID.md](docs/ANDROID.md), [docs/KODI.md](docs/KODI.md), [docs/ROKU.md](docs/ROKU.md), [docs/TV_PLATFORMS.md](docs/TV_PLATFORMS.md) |
| Docker, Unraid, Windows, services | [deploy/README.md](deploy/README.md) |
| Licences and dependencies | [LICENSING.md](LICENSING.md) |
| Versions, changelogs, CI | [docs/RELEASING.md](docs/RELEASING.md) |

## Check your work

Run the checks for what you changed before you call it done; CI runs them
too, each app's workflow when that app changes.

| You changed | Run |
|---|---|
| Rust (server, transcoder, protocol) | `cargo clippy --workspace --all-targets`, `cargo fmt --all`, `cargo test -p <crate>` (the whole workspace before a large change) |
| A Rust dependency | `scripts/check-licenses.ps1` and `cargo deny check licenses` |
| Web or desktop app | `bun run check` (type check, ESLint, Vitest) |
| Samsung or LG app, `packages/tv` | `bun run tv:check` |
| Native Samsung app (`apps/tizen-native`), `packages/tv/test/fixtures` | `dotnet test apps/tizen-native/test/Pley.Tv.Core.Tests`, `dotnet build apps/tizen-native/src/Pley.Tv.Tizen` (and `bun run tv:check` for a fixture) |
| A NuGet package | `bun scripts/check-nuget-licenses.mjs` |
| Roku app | `bun install && bun run check` in `apps/roku` |
| Kodi add-on | `python -m unittest discover -s tests` in `apps/kodi` |
| Android app | `./gradlew testDebugUnitTest lintDebug` in `apps/android`; `pwsh ../../scripts/check-android.ps1` after `assembleStoreRelease` when libraries change |
| Docker image | built and checked by CI (`.github/workflows/server.yml`) |

Fix what fails, including tests a change breaks elsewhere. Don't weaken or
skip a test to make it pass; if a test is wrong, say why in the commit.
Something you can't run (no device, no Docker) is said plainly, not
claimed as checked.

## Rules that must hold

**Licensing** ([LICENSING.md](LICENSING.md)). Pley is under the GNU
AGPL, version 3 only (`AGPL-3.0-only`, the root [LICENSE](LICENSE)); only
the Kodi add-on is GPL-2.0-or-later. Contributions come under the same
licences ([CONTRIBUTING.md](CONTRIBUTING.md)).

- Add no dependency that is only available under a copyleft licence (GPL,
  LGPL, AGPL and the like) to the server, the transcoder or any app other
  than the Kodi add-on. `scripts/check-licenses.ps1` and `deny.toml` enforce
  this for Rust, `scripts/check-nuget-licenses.mjs` for the native Samsung
  app's NuGet packages.
- The transcoder runs `ffmpeg` and `ffprobe` as programs. Never link
  FFmpeg's libraries into it: that would bind it to FFmpeg's GPL.
- The server never depends on the `pley-transcoder` crate; it talks to the
  transcoder over HTTP with the types in `packages/transcoder-api`.
- Never copy the Kodi add-on's code into another part of Pley.
- The Android app takes no FFmpeg, mpv or GPL library and no new native
  code (`scripts/check-android.ps1` has the allowed library groups). The TV
  apps' build fails on a bundled package that `packages/tv/src/licenses.ts`
  doesn't name.
- Don't add licence texts or headers of your own; new transcoder files
  start with `// SPDX-License-Identifier: AGPL-3.0-only`, as the
  others do.

**Security** ([SECURITY.md](SECURITY.md)).

- Every API route checks who is asking: an `AuthUser`, and the access check
  for its area (administrators, Live TV's access, a library's).
- A route apps may use is listed in `APP_ROUTES` in
  `apps/server/src/auth/scopes.rs` with the scope it needs, and the count
  in `every_app_route_is_on_a_real_path` goes up with it. Administration
  never goes to apps.
- Input from outside (requests, files, IPTV providers, metadata sources) is
  bounded and checked: sizes, paths, formats. Pictures are served only as
  JPEG, PNG, WebP or GIF, never SVG or HTML from Pley's address.
- No secrets, tokens or personal data in logs, code, tests or docs.

**Every app.** A feature people see is expected on every app (web and
desktop, Android phone and TV, Samsung, LG, Kodi, Roku), the same way,
unless the platform can't; the app's doc says what it can't do. The native
Samsung app ports the Android TV app to C#, its screens and its rules. A
rule it shares with `packages/tv` is pinned by a fixture in
`packages/tv/test/fixtures`, and changes in both, with it; where Android
differs, the native app follows Android and its test says so.

**The brand** is written Pley in everything people read. Ids, binaries,
paths, packages and protocol tokens stay lowercase `pley`.

**Shared docs are for readers.** Docs and comments say what is and why.
Notes to the maintainer (open questions, assumptions, to-dos, test logs,
untested-on-device lists, a particular person's devices, addresses or
providers) never go in a tracked file, and neither do question numbers.
They go in `docs/QUESTIONS.md` or `docs/*.local.md`, which git ignores.

## Writing

- **Code** reads like the code around it: its naming, structure, error
  handling and comment density. Comments explain why, in plain sentences.
- **Docs** are plain and in the present tense: what Pley does, for whom,
  and how. Update the doc a change affects in the same commit (a new route
  goes in [docs/API.md](docs/API.md)).
- **Changelogs**: a line under `## [Unreleased]` in each changed app's
  `CHANGELOG.md`, saying what changed as a user sees it.
- **Formatting**: `cargo fmt` (max width 120) for Rust; ESLint and the
  existing style for TypeScript. The repository doesn't use Prettier: don't
  run it.
- Line endings are LF, except `.ps1`, `.cmd` and `.bat` (CRLF), as
  `.gitattributes` says.

## Commits

- One logical change per commit, with what it needs (code, tests, docs,
  changelog).
- The subject starts with the area and says, briefly, what changed as a
  user sees it: `server: Live TV starts at the last keyframe`, `web: …`,
  `kodi, roku: …`, `docs: …`. The body is short and to the point: why, and
  anything a reviewer should know, not a retelling of the diff. No
  `Co-Authored-By` trailer and no "Generated with …" line.
- Never commit build output (`target/`, `dist/`), keys, `.env` files, local
  data, or files git ignores.

## Branches and pull requests

- A branch is `<kind>/<topic>`: the kind is `fix`, `feature`,
  `optimization`, `experiment` or `deployment`, and the topic is a few words
  in snake_case saying what it's for, as in `fix/server_address_input`.
  `release/` is the release workflow's.
- Never push a branch a tool named, such as `claude/…` or `worktree-…`:
  branch lists, pull requests and merge commits show the name, and it says
  what the change is for. A session or worktree that starts on one renames
  it with `git branch -m <kind>/<topic>` before it pushes.
- A pull request's title is written like a commit subject, and kept short:
  `android, samsung, lg: Enter an address opens the keyboard`. A change to
  every app says `apps:`.
- Its description is short and plain: why and what changed in a sentence
  or two, then what a reviewer should know, by app or area, then what was
  checked and what wasn't.
- A change people can see shows it, in a table under the description: a
  row per change, the screen before it and after it (N/A before something
  new), taken on the same screen through the same steps.
  [DEVELOPMENT.md](docs/DEVELOPMENT.md#screenshots-for-a-pull-request)
  says how for each app.
  - Each screenshot marks where to look with a red box and an arrow
    (`scripts/mark-screenshot.mjs`), and has a caption of a few words
    under it in `<sub>`.
  - A detail a few pixels wide (an edge, a border, a badge) is shown
    zoomed in, pixel for pixel, until it's plain without the caption.
  - Screenshots are attached with `gh pr create --attach`, never
    committed.
  - The description, pictures and all, is shown to whoever asked for the
    change before it's posted.
- Write it as a person would: no bold labels, no headings, no tables for
  a few lines (the screenshots' table aside), no review history, no
  "Generated with …" line. Don't repeat the diff or the docs; point to
  the doc.

## Platform notes

- On Windows, run `cargo` from PowerShell: Git Bash's `link` can shadow
  MSVC's linker.
- Debug builds of the server read the web app from `apps/web/dist`, so
  `bun run build` shows up without recompiling the server.

// bun test ./scripts: the version, changelog and release scripts.

import { afterAll, describe, expect, test } from "bun:test";
import { execFileSync, spawnSync } from "node:child_process";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { addEntries, entryOf, findSection, notes, plainText, release, sections } from "./changelog.mjs";
import { RELEASE_TAG, lowered, parseArgs, pending, plan, status } from "./release.mjs";
import {
  ROOT,
  UNITS,
  UNIT_NAMES,
  androidVersionCode,
  bump,
  compare,
  limitProblem,
  planSet,
  problems,
  versionOf,
  versions,
  write,
} from "./version.mjs";

const temps = [];
afterAll(() => temps.forEach((t) => rmSync(t, { recursive: true, force: true })));

/** A repository holding only the units' manifests and changelogs, committed. */
function fixture() {
  const dir = mkdtempSync(join(tmpdir(), "pley-release-"));
  temps.push(dir);
  const files = new Set(Object.values(UNITS).flatMap((u) => [u.changelog, ...u.manifests.map((m) => m.path)]));
  for (const f of files) {
    mkdirSync(dirname(join(dir, f)), { recursive: true });
    cpSync(join(ROOT, f), join(dir, f));
  }
  // Every unit as just after its 0.0.1, whatever the repository's own versions and changelogs say now.
  for (const [name, u] of Object.entries(UNITS)) {
    write(planSet(name, "0.0.1", dir), dir);
    const path = join(dir, u.changelog);
    writeFileSync(path, readFileSync(path, "utf8").replace(/## \[Unreleased\]\n[\s\S]*?(?=\n## \[0\.0\.1\])/, "## [Unreleased]\n"));
  }
  const git = (...args) =>
    execFileSync("git", ["-c", "user.name=Test", "-c", "user.email=test@example.com", "-c", "commit.gpgsign=false", ...args], {
      cwd: dir,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "pipe"],
    }).trim();
  git("init", "-q", "-b", "main");
  git("add", "-A");
  git("commit", "-q", "-m", "start");
  const change = (path, text, subject) => {
    mkdirSync(dirname(join(dir, path)), { recursive: true });
    writeFileSync(join(dir, path), text);
    git("add", "-A");
    git("commit", "-q", "-m", subject);
  };
  return { dir, git, change };
}

/** The unit's Unreleased with a fix in it, not committed. */
function addEntry(dir, unit, entry = "A fix") {
  const path = join(dir, UNITS[unit].changelog);
  writeFileSync(path, readFileSync(path, "utf8").replace("## [Unreleased]\n", `## [Unreleased]\n\n### Fixed\n\n- ${entry}\n`));
}

/** Bumps the unit, as `release.mjs <unit> <how> --commit` does. */
function bumped({ dir, git }, unit, how, date = "2026-11-01") {
  const p = plan({ unit, how, date, root: dir });
  write(p.changes, dir);
  git("commit", "-q", "-am", `${unit}: release ${p.to}`);
  return p;
}

const SAMPLE = `# Changelog

Intro.

## [Unreleased]

### Fixed

- A bug

## [0.0.1] - 2026-10-29

### Added

- First release.

[Unreleased]: https://example.com/compare/kodi-v0.0.1...HEAD
[0.0.1]: https://example.com/releases/tag/kodi-v0.0.1
`;

describe("versions", () => {
  test("bumps", () => {
    expect(bump("0.0.1", "patch")).toBe("0.0.2");
    expect(bump("0.0.9", "minor")).toBe("0.1.0");
    expect(bump("0.4.2", "major")).toBe("1.0.0");
    expect(bump("0.0.1", "1.2.3")).toBe("1.2.3");
    expect(() => bump("0.0.1", "next")).toThrow(/patch, minor, major/);
    expect(() => bump("0.0.1", "1.2")).toThrow();
  });

  test("compares numerically", () => {
    expect(compare("0.0.10", "0.0.9")).toBeGreaterThan(0);
    expect(compare("1.0.0", "0.99.99")).toBeGreaterThan(0);
    expect(compare("0.1.0", "0.1.0")).toBe(0);
  });

  test("Android's version codes stay above Google Play's 20003 and only go up", () => {
    expect(androidVersionCode("0.0.1")).toBe(10_000_100);
    expect(androidVersionCode("0.0.1")).toBeGreaterThan(20003);
    expect(androidVersionCode("1.2.3", 4)).toBe(11_020_304);
    const order = ["0.0.1", "0.0.2", "0.0.99", "0.1.0", "0.99.99", "1.0.0", "2.3.4", "99.99.99"];
    const codes = order.map((v) => androidVersionCode(v, 99));
    for (let i = 1; i < order.length; i++) expect(androidVersionCode(order[i])).toBeGreaterThan(codes[i - 1]);
    expect(androidVersionCode("2089.99.99", 99)).toBeLessThanOrEqual(2_100_000_000);
  });

  test("the Gradle build computes the same code", () => {
    const gradle = readFileSync(join(ROOT, "apps/android/app/build.gradle.kts"), "utf8");
    expect(gradle).toContain("return 10_000_000 + major * 1_000_000 + minor * 10_000 + patch * 100");
  });

  test("platform limits", () => {
    expect(limitProblem("android", "0.100.0")).toMatch(/below 100/);
    expect(limitProblem("android", "0.99.99")).toBeNull();
    expect(limitProblem("samsung", "256.0.0")).toMatch(/Tizen/);
    expect(limitProblem("samsung-native", "0.256.0")).toMatch(/Tizen/);
    expect(limitProblem("kodi", "300.0.0")).toBeNull();
  });

  test("every unit's manifests and changelog agree in the repository", () => {
    for (const unit of UNIT_NAMES) {
      expect({ unit, problems: problems(unit) }).toEqual({ unit, problems: [] });
      for (const v of versions(unit)) expect(v.version).toBe(versionOf(unit));
    }
  });

  test("setting a unit writes its manifests only", () => {
    const { dir } = fixture();
    const before = Object.fromEntries(UNIT_NAMES.map((u) => [u, versionOf(u, dir)]));
    const changes = planSet("samsung", "0.3.0", dir);
    expect(changes.map((c) => c.path).sort()).toEqual(["apps/tizen/config.xml", "apps/tizen/package.json", "bun.lock"]);
    write(changes, dir);
    expect(versions("samsung", dir).every((v) => v.version === "0.3.0")).toBe(true);
    for (const u of UNIT_NAMES.filter((x) => x !== "samsung")) expect(versionOf(u, dir)).toBe(before[u]);
    // Its changelog has no 0.3.0 section yet.
    expect(problems("samsung", dir)).toEqual([`${UNITS.samsung.changelog} has no section for 0.3.0`]);
  });

  test("the native Samsung app's version is its build's and its manifest's, not Tizen's api-version", () => {
    const { dir } = fixture();
    const manifest = "apps/tizen-native/src/Pley.Tv.Tizen/tizen-manifest.xml";
    const changes = planSet("samsung-native", "0.2.0", dir);
    expect(changes.map((c) => c.path).sort()).toEqual(["apps/tizen-native/Directory.Build.props", manifest]);
    write(changes, dir);
    expect(readFileSync(join(dir, "apps/tizen-native/Directory.Build.props"), "utf8")).toContain("<Version>0.2.0</Version>");
    const text = readFileSync(join(dir, manifest), "utf8");
    expect(text).toContain('version="0.2.0" api-version="9.0"');
    expect(text).toContain('<?xml version="1.0"');
  });

  test("the server's version reaches the transcoder, its API crate and the web app", () => {
    const { dir } = fixture();
    write(planSet("server", "1.4.0", dir), dir);
    const found = versions("server", dir);
    expect(found.length).toBe(9);
    expect(found.every((v) => v.version === "1.4.0")).toBe(true);
    expect(readFileSync(join(dir, "apps/transcoder/Cargo.toml"), "utf8")).toContain('pley-transcoder-api = { path = "../../packages/transcoder-api", version = "1.4.0" }');
  });

  test("Roku's manifest takes major, minor and build", () => {
    const { dir } = fixture();
    write(planSet("roku", "2.5.17", dir), dir);
    const manifest = readFileSync(join(dir, "apps/roku/manifest"), "utf8");
    expect(manifest).toMatch(/^major_version=2$/m);
    expect(manifest).toMatch(/^minor_version=5$/m);
    expect(manifest).toMatch(/^build_version=17$/m);
  });

  test("refuses what a platform can't take", () => {
    expect(() => planSet("android", "0.100.0")).toThrow(/below 100/);
    expect(() => planSet("kodi", "1.2")).toThrow();
    expect(() => planSet("nope", "1.2.3")).toThrow(/isn't a unit/);
  });
});

describe("changelog", () => {
  test("finds sections", () => {
    expect(sections(SAMPLE).map((s) => [s.name, s.date])).toEqual([
      ["Unreleased", null],
      ["0.0.1", "2026-10-29"],
    ]);
    expect(findSection(SAMPLE, "Unreleased").body).toBe("### Fixed\n\n- A bug");
    expect(notes(SAMPLE, "0.0.1")).toBe("### Added\n\n- First release.");
    expect(() => notes(SAMPLE, "0.0.2")).toThrow(/no section/);
  });

  test("moves Unreleased under the new version, with its links", () => {
    const out = release(SAMPLE, { version: "0.0.2", date: "2026-11-01", tag: "kodi-v0.0.2", repository: "https://example.com" });
    expect(findSection(out, "Unreleased").body).toBe("");
    expect(notes(out, "0.0.2")).toBe("### Fixed\n\n- A bug");
    expect(notes(out, "0.0.1")).toBe("### Added\n\n- First release.");
    expect(out).toContain("## [Unreleased]\n\n## [0.0.2] - 2026-11-01\n\n### Fixed\n\n- A bug\n\n## [0.0.1] - 2026-10-29");
    expect(out.trimEnd().split("\n").slice(-3)).toEqual([
      "[Unreleased]: https://example.com/compare/kodi-v0.0.2...HEAD",
      "[0.0.2]: https://example.com/releases/tag/kodi-v0.0.2",
      "[0.0.1]: https://example.com/releases/tag/kodi-v0.0.1",
    ]);
  });

  test("refuses an empty Unreleased or a version it has", () => {
    const released = release(SAMPLE, { version: "0.0.2", date: "2026-11-01", tag: "t", repository: "r" });
    expect(() => release(released, { version: "0.0.3", date: "2026-11-02", tag: "t", repository: "r" })).toThrow(/empty/);
    expect(() => release(SAMPLE, { version: "0.0.1", date: "2026-11-02", tag: "t", repository: "r" })).toThrow(/already/);
  });

  test("adds drafted entries under Changed, once", () => {
    const { text, added } = addEntries(SAMPLE, ["Faster start", "A bug", "Faster start"]);
    expect(added).toEqual(["Faster start"]);
    expect(findSection(text, "Unreleased").body).toBe("### Fixed\n\n- A bug\n\n### Changed\n\n- Faster start");
    const again = addEntries(text, ["Smaller package"]);
    expect(findSection(again.text, "Unreleased").body).toBe("### Fixed\n\n- A bug\n\n### Changed\n\n- Faster start\n- Smaller package");
    expect(notes(again.text, "0.0.1")).toBe("### Added\n\n- First release.");
    expect(addEntries(again.text, ["Faster start"]).added).toEqual([]);
  });

  test("adds to a Changed section that has others after it", () => {
    const text = SAMPLE.replace("### Fixed\n\n- A bug", "### Changed\n\n- One\n\n### Fixed\n\n- A bug");
    const out = addEntries(text, ["Two"]).text;
    expect(findSection(out, "Unreleased").body).toBe("### Changed\n\n- One\n- Two\n\n### Fixed\n\n- A bug");
  });

  test("an entry is a commit's subject without its area", () => {
    expect(entryOf("kodi, roku: Automatic settles on the highest quality")).toBe("Automatic settles on the highest quality");
    expect(entryOf("transcoder: frames are scaled by the GPU")).toBe("Frames are scaled by the GPU");
    expect(entryOf("Fix the build")).toBe("Fix the build");
    expect(entryOf("kodi: release 0.0.2")).toBeNull();
  });

  test("plain text for Kodi's news", () => {
    expect(plainText("0.0.2", "2026-11-01", "### Fixed\n\n- A `bug`, which\n  wrapped\n- [Linked](https://x) one")).toBe(
      "0.0.2 (2026-11-01)\nFixed:\n\n- A bug, which wrapped\n- Linked one",
    );
  });
});

describe("release", () => {
  test("bumps a unit, drafts its commits since its version was set, and writes Kodi's news", () => {
    const { dir, change } = fixture();
    change("apps/kodi/plugin.video.pley/resources/a.py", "a\n", "kodi: subtitles load faster");
    change("apps/roku/source/b.brs", "b\n", "roku: something else");
    change("apps/kodi/tests/c.py", "c\n", "kodi, roku: Automatic settles");
    const p = plan({ unit: "kodi", how: "patch", date: "2026-11-01", withDraft: true, root: dir });
    expect([p.from, p.to, p.tag]).toEqual(["0.0.1", "0.0.2", "release-2026-11-01"]);
    expect(p.drafted.commits).toBe(2);
    expect(p.notes).toBe("### Changed\n\n- Subtitles load faster\n- Automatic settles");
    expect(p.changes.map((c) => c.path)).toEqual(["apps/kodi/plugin.video.pley/addon.xml", "apps/kodi/CHANGELOG.md"]);
    const addon = p.changes[0].after;
    expect(addon).toContain('version="0.0.2"');
    expect(addon).toContain("<news>0.0.2 (2026-11-01)\nChanged:\n\n- Subtitles load faster\n- Automatic settles</news>");
    // Nothing is written by a plan.
    expect(versionOf("kodi", dir)).toBe("0.0.1");
  });

  test("refuses an empty Unreleased, a lower version and a wrong date", () => {
    const { dir } = fixture();
    expect(() => plan({ unit: "roku", how: "patch", root: dir })).toThrow(/Unreleased .* is empty/);
    expect(() => plan({ unit: "roku", how: "patch", withDraft: true, root: dir })).toThrow(/none of the 0 commits/);
    addEntry(dir, "roku");
    expect(() => plan({ unit: "roku", how: "0.0.1", root: dir })).toThrow(/isn't above/);
    expect(plan({ unit: "roku", how: "minor", root: dir, date: "2026-11-01" }).to).toBe("0.1.0");
    expect(() => plan({ unit: "roku", how: "minor", root: dir, date: "1 Nov" })).toThrow(/YYYY-MM-DD/);
  });

  test("Android's bump keeps its version code in the Gradle build", () => {
    const { dir } = fixture();
    addEntry(dir, "android");
    const p = plan({ unit: "android", how: "minor", root: dir, date: "2026-11-01" });
    expect(p.changes[0].after).toContain('\nval appVersion = "0.1.0"');
  });

  test("parses options", () => {
    expect(parseArgs(["kodi", "patch", "--dry-run", "--since", "abc", "--draft"])).toEqual({
      positional: ["kodi", "patch"],
      opts: { dryRun: true, draft: true, commit: false, json: false, since: "abc", date: undefined },
    });
    expect(() => parseArgs(["kodi", "patch", "--since"])).toThrow(/needs a value/);
    expect(() => parseArgs(["kodi", "patch", "--force"])).toThrow(/unknown option/);
  });

  test("prints each unit's notes", () => {
    for (const unit of UNIT_NAMES) {
      const out = spawnSync("bun", [join(ROOT, "scripts/release.mjs"), unit, "notes"], { encoding: "utf8" });
      expect(out.status).toBe(0);
      const changelog = readFileSync(join(ROOT, UNITS[unit].changelog), "utf8");
      expect(out.stdout.trimEnd()).toBe(notes(changelog, versionOf(unit)));
      expect(out.stdout).toStartWith("### ");
    }
    const missing = spawnSync("bun", [join(ROOT, "scripts/release.mjs"), "kodi", "notes", "9.9.9"], { encoding: "utf8" });
    expect(missing.status).toBe(2);
    expect(missing.stderr).toContain("no section for 9.9.9");
  });
});

// These run git many times, which takes seconds on Windows.
const SLOW = 30_000;

describe("releases", () => {
  test("take the units bumped since the last release, under one tag", () => {
    const f = fixture();
    const { dir, git } = f;
    git("tag", "release-2026-10-31");
    expect(pending(dir)).toMatchObject({ since: "release-2026-10-31", tag: null, units: [], problems: [] });

    addEntry(dir, "kodi", "Kodi's fix");
    bumped(f, "kodi", "patch");
    const one = pending(dir);
    expect(one).toMatchObject({ tag: "release-2026-11-01", title: "Pley for Kodi 0.0.2", problems: [] });
    expect(one.units.map((u) => [u.unit, u.from, u.to, u.later])).toEqual([["kodi", "0.0.1", "0.0.2", []]]);
    expect(one.notes).toBe("### Fixed\n\n- Kodi's fix");

    // Another unit joins the release, with its own version.
    addEntry(dir, "roku", "Roku's fix");
    expect(bumped(f, "roku", "minor").tag).toBe("release-2026-11-01");
    const two = pending(dir);
    expect(two).toMatchObject({ tag: "release-2026-11-01", title: "Pley 2026-11-01", problems: [] });
    expect(two.units.map((u) => `${u.unit} ${u.to}`)).toEqual(["kodi 0.0.2", "roku 0.1.0"]);
    expect(two.notes).toBe(
      "<details>\n<summary><strong>Pley for Kodi 0.0.2</strong></summary>\n\n### Fixed\n\n- Kodi's fix\n\n</details>\n\n" +
        "<details>\n<summary><strong>Pley for Roku 0.1.0</strong></summary>\n\n### Fixed\n\n- Roku's fix\n\n</details>",
    );
    expect(status(dir).filter((u) => u.waits).map((u) => u.unit)).toEqual(["kodi", "roku"]);

    // A unit waiting for its release isn't bumped again.
    addEntry(dir, "kodi");
    expect(() => plan({ unit: "kodi", how: "patch", date: "2026-11-01", root: dir })).toThrow(/isn't released yet/);
    git("checkout", "-q", "--", ".");

    // Once tagged, nothing waits, and the day's next release takes the next tag.
    git("tag", "release-2026-11-01");
    expect(pending(dir)).toMatchObject({ since: "release-2026-11-01", units: [] });
    addEntry(dir, "android");
    expect(bumped(f, "android", "patch").tag).toBe("release-2026-11-01.2");
    expect(RELEASE_TAG.test("release-2026-11-01.2")).toBe(true);
    expect(RELEASE_TAG.test("kodi-v0.0.2")).toBe(false);
  }, SLOW);

  test("tell what would ship without being in the notes", () => {
    const f = fixture();
    const { dir, git, change } = f;
    git("tag", "release-2026-10-31");
    addEntry(dir, "kodi");
    bumped(f, "kodi", "patch");
    change("apps/kodi/plugin.video.pley/resources/late.py", "x\n", "kodi: a late fix");
    const changelog = readFileSync(join(dir, UNITS.kodi.changelog), "utf8");
    change(UNITS.kodi.changelog, changelog.replace("## [Unreleased]\n", "## [Unreleased]\n\n### Fixed\n\n- A late fix\n"), "kodi: its entry");
    const p = pending(dir);
    expect(p.units[0].later.map((c) => c.replace(/^\S+ /, ""))).toEqual(["kodi: a late fix", "kodi: its entry"]);
    expect(p.problems).toEqual([
      `${UNITS.kodi.changelog} has entries under Unreleased, which would ship in kodi 0.0.2 without being in its notes: move them under 0.0.2`,
    ]);
  }, SLOW);

  test("the Release workflow runs when a unit's version changes, and builds every unit", () => {
    const workflow = readFileSync(join(ROOT, ".github/workflows/release.yml"), "utf8");
    for (const unit of UNIT_NAMES) {
      expect(workflow).toContain(`\n      - ${UNITS[unit].manifests[0].path}\n`);
      expect(workflow).toContain(`contains(fromJSON(needs.prepare.outputs.units || '[]'), '${unit}')`);
    }
  });

  test("never take a unit below its last release, while what waits for the next passes", () => {
    const f = fixture();
    const { dir, git } = f;
    // The command as CI runs it, with the fixture as its repository.
    mkdirSync(join(dir, "scripts"));
    for (const s of ["release.mjs", "version.mjs", "changelog.mjs"]) cpSync(join(ROOT, "scripts", s), join(dir, "scripts", s));
    const check = () => spawnSync("bun", [join(dir, "scripts/release.mjs"), "check"], { encoding: "utf8" });
    expect(lowered(dir)).toEqual({ since: null, problems: [] });
    addEntry(dir, "kodi");
    bumped(f, "kodi", "patch");
    git("tag", "release-2026-11-01");

    // Between releases: a unit above its release's version, entries under Unreleased.
    addEntry(dir, "roku");
    bumped(f, "roku", "patch");
    addEntry(dir, "roku", "Later");
    addEntry(dir, "kodi", "Next");
    expect(pending(dir).problems).not.toEqual([]);
    expect(lowered(dir)).toEqual({ since: "release-2026-11-01", problems: [] });
    expect(check().status).toBe(0);

    // A branch carrying Kodi's manifest and CHANGELOG from before the release,
    // which agree with each other.
    git("checkout", "-q", "release-2026-11-01~1", "--", UNITS.kodi.manifests[0].path, UNITS.kodi.changelog);
    expect(problems("kodi", dir)).toEqual([]);
    const down = "kodi went down, from 0.0.2 to 0.0.1";
    expect(lowered(dir).problems).toEqual([down]);
    expect(pending(dir).problems).toContain(down);
    const out = check();
    expect(out.status).toBe(1);
    expect(out.stderr).toContain(down);
  }, SLOW);

  test("the Repository workflow checks every change to a unit's version against the last release", () => {
    const workflow = readFileSync(join(ROOT, ".github/workflows/repo.yml"), "utf8");
    // On pushes to master and on pull requests.
    for (const unit of UNIT_NAMES) {
      const on = workflow.split(`\n      - ${UNITS[unit].manifests[0].path}\n`).length - 1;
      expect({ unit, on }).toEqual({ unit, on: 2 });
    }
    expect(workflow).toContain("fetch-depth: 0");
    expect(workflow).toContain("\n      - run: bun scripts/release.mjs check\n");
  });

  test("count only release-<date> tags as releases", () => {
    const f = fixture();
    f.git("tag", "release-2026-10-31");
    addEntry(f.dir, "kodi");
    bumped(f, "kodi", "patch");
    f.git("tag", "kodi-v0.0.2");
    const p = pending(f.dir);
    expect(p).toMatchObject({ since: "release-2026-10-31", tag: "release-2026-11-01", problems: [] });
    expect(p.units.map((u) => u.unit)).toEqual(["kodi"]);
  }, SLOW);
});

#!/usr/bin/env bun
// Each app's version, kept in its own manifests (docs/RELEASING.md).
//
//   bun scripts/version.mjs                   every unit's version
//   bun scripts/version.mjs get <unit>        one unit's version
//   bun scripts/version.mjs info <unit>       ...with its name, as JSON
//   bun scripts/version.mjs set <unit> 1.2.3  writes it into the unit's manifests
//   bun scripts/version.mjs check [unit...]   fails when a unit's manifests disagree
//
// A unit is what has its own version: server (with its transcoder and the
// web app it embeds), desktop, android, samsung, lg, kodi and roku. A release
// takes the units whose versions went up, under one tag.
// `bun scripts/release.mjs` bumps a unit and moves its CHANGELOG's Unreleased
// section under the new version.
// Builds add the commit they were made from (scripts/commit.mjs), which every
// app shows beside its version.

import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { findSection } from "./changelog.mjs";

export const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
export const REPOSITORY = "https://github.com/pley-media-server/pley";
const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

/** A manifest's version, found and replaced by a pattern whose second group is the version. */
function pattern(path, regex, label = path) {
  return {
    path,
    label,
    get: (text) => text.match(regex)?.[2],
    set: (text, v) => text.replace(regex, (_, before, _old, after) => `${before}${v}${after}`),
  };
}

const json = (path) => pattern(path, /("version":\s*")([^"]*)(")/);
const cargoPackage = (path) => pattern(path, /(\[package\][^[]*?\nversion = ")([^"]*)(")/);
const cargoLock = (path, name) =>
  pattern(path, new RegExp(`(\\[\\[package\\]\\]\\r?\\nname = "${name}"\\r?\\nversion = ")([^"]*)(")`), `${path} (${name})`);
/** Bun's lock records each workspace's version after its name. */
const bunLock = (workspace) =>
  pattern("bun.lock", new RegExp(`("${workspace}": \\{\\s*"name": "[^"]+",\\s*"version": ")([^"]*)(")`), `bun.lock (${workspace})`);
const xmlAttribute = (path, element) => pattern(path, new RegExp(`(<${element} [^>]*?\\bversion=")([^"]*)(")`));

/** Roku's manifest: major_version, minor_version and build_version. */
const rokuManifest = {
  path: "apps/roku/manifest",
  label: "apps/roku/manifest",
  get(text) {
    const n = (key) => text.match(new RegExp(`^${key}=(\\d+)\\r?$`, "m"))?.[1];
    const parts = [n("major_version"), n("minor_version"), n("build_version")];
    return parts.every((p) => p !== undefined) ? parts.join(".") : undefined;
  },
  set(text, v) {
    const [major, minor, patch] = v.split(".");
    return text
      .replace(/^major_version=\d+/m, `major_version=${major}`)
      .replace(/^minor_version=\d+/m, `minor_version=${minor}`)
      .replace(/^build_version=\d+/m, `build_version=${patch}`);
  },
};

/**
 * Android's version code: 10,000,000 + major × 1,000,000 + minor × 10,000 +
 * patch × 100, plus PLEY_BUILD (1 to 99) for a store rebuild of the same
 * version. Google Play had 20003 (0.2.0, the single version before), so the
 * offset keeps every code above it and the codes only go up. Mirrors
 * versionCodeOf in apps/android/app/build.gradle.kts.
 */
export const ANDROID_CODE_OFFSET = 10_000_000;
export function androidVersionCode(version, build = 0) {
  const [major, minor, patch] = parse(version);
  return ANDROID_CODE_OFFSET + major * 1_000_000 + minor * 10_000 + patch * 100 + build;
}

/** What each platform takes, beyond major.minor.patch. */
const LIMITS = {
  // Version codes stay in order only while minor and patch fit their digits.
  android: ([, minor, patch]) => (minor > 99 || patch > 99 ? "Android's version code needs minor and patch below 100" : null),
  // Tizen: x.y.z, x and y at most 255, z at most 65535.
  samsung: ([major, minor, patch]) =>
    major > 255 || minor > 255 || patch > 65535 ? "Tizen takes major and minor up to 255 and patch up to 65535" : null,
};

export const UNITS = {
  server: {
    title: "Pley Media Server",
    about: "the server, its transcoder and the web app it embeds; the Docker image and the Windows installer are its packages",
    changelog: "apps/server/CHANGELOG.md",
    // What its changes are drafted from (scripts/release.mjs).
    paths: ["apps/server", "apps/transcoder", "packages/transcoder-api", "apps/web", "packages/picture-subtitles", "deploy/docker", "deploy/windows", "deploy/systemd", "deploy/launchd"],
    manifests: [
      cargoPackage("apps/server/Cargo.toml"),
      cargoPackage("apps/transcoder/Cargo.toml"),
      pattern("apps/transcoder/Cargo.toml", /(pley-transcoder-api = \{[^}]*\bversion = ")([^"]*)(")/, "apps/transcoder/Cargo.toml (pley-transcoder-api)"),
      cargoPackage("packages/transcoder-api/Cargo.toml"),
      cargoLock("Cargo.lock", "pley-server"),
      cargoLock("Cargo.lock", "pley-transcoder"),
      cargoLock("Cargo.lock", "pley-transcoder-api"),
      json("apps/web/package.json"),
      bunLock("apps/web"),
    ],
  },
  desktop: {
    title: "Pley Desktop",
    about: "the desktop app (Tauri) for Windows, macOS and Linux",
    changelog: "apps/desktop/CHANGELOG.md",
    paths: ["apps/desktop"],
    manifests: [
      json("apps/desktop/package.json"),
      bunLock("apps/desktop"),
      json("apps/desktop/src-tauri/tauri.conf.json"),
      cargoPackage("apps/desktop/src-tauri/Cargo.toml"),
      cargoLock("apps/desktop/src-tauri/Cargo.lock", "pley-desktop"),
    ],
  },
  android: {
    title: "Pley for Android",
    about: "the Android app for phones, tablets, Android TV and Fire TV",
    changelog: "apps/android/CHANGELOG.md",
    paths: ["apps/android"],
    manifests: [pattern("apps/android/app/build.gradle.kts", /(\nval appVersion = ")([^"]*)(")/)],
  },
  samsung: {
    title: "Pley for Samsung TV",
    about: "the Samsung TV app (Tizen), built with packages/tv",
    changelog: "apps/tizen/CHANGELOG.md",
    paths: ["apps/tizen", "packages/tv", "packages/picture-subtitles"],
    manifests: [json("apps/tizen/package.json"), bunLock("apps/tizen"), xmlAttribute("apps/tizen/config.xml", "widget")],
  },
  lg: {
    title: "Pley for LG TV",
    about: "the LG TV app (webOS), built with packages/tv",
    changelog: "apps/webos/CHANGELOG.md",
    paths: ["apps/webos", "packages/tv", "packages/picture-subtitles"],
    manifests: [json("apps/webos/package.json"), bunLock("apps/webos"), json("apps/webos/appinfo.json")],
  },
  kodi: {
    title: "Pley for Kodi",
    about: "the Kodi add-on, plugin.video.pley",
    changelog: "apps/kodi/CHANGELOG.md",
    paths: ["apps/kodi"],
    manifests: [xmlAttribute("apps/kodi/plugin.video.pley/addon.xml", "addon")],
  },
  roku: {
    title: "Pley for Roku",
    about: "the Roku app",
    changelog: "apps/roku/CHANGELOG.md",
    paths: ["apps/roku"],
    manifests: [rokuManifest, json("apps/roku/package.json")],
  },
};

export const UNIT_NAMES = Object.keys(UNITS);

/** The unit by name, or an error that lists them. */
export function unitOf(name) {
  if (!Object.hasOwn(UNITS, name ?? "")) {
    throw new UsageError(`${name ? `"${name}" isn't a unit` : "No unit given"}: one of ${UNIT_NAMES.join(", ")}`);
  }
  return UNITS[name];
}

export class UsageError extends Error {}

export function parse(version) {
  const m = SEMVER.exec(version ?? "");
  if (!m) throw new UsageError(`"${version}" isn't a version like 1.2.3`);
  return [Number(m[1]), Number(m[2]), Number(m[3])];
}

export const isVersion = (v) => SEMVER.test(v ?? "");

/** Negative, zero or positive as a is below, at or above b. */
export function compare(a, b) {
  const [x, y] = [parse(a), parse(b)];
  return x[0] - y[0] || x[1] - y[1] || x[2] - y[2];
}

/** `version` raised by patch, minor or major, or replaced by an exact version. */
export function bump(version, how) {
  const [major, minor, patch] = parse(version);
  if (how === "patch") return `${major}.${minor}.${patch + 1}`;
  if (how === "minor") return `${major}.${minor + 1}.0`;
  if (how === "major") return `${major + 1}.0.0`;
  if (!isVersion(how)) throw new UsageError(`"${how}": patch, minor, major or a version like 1.2.3`);
  return how;
}

/** Why `version` can't be the unit's, or null. */
export function limitProblem(unit, version) {
  return LIMITS[unit]?.(parse(version)) ?? null;
}

const reader = (root) => {
  const cache = new Map();
  return (path) => {
    if (!cache.has(path)) {
      const file = join(root, path);
      cache.set(path, existsSync(file) ? readFileSync(file, "utf8") : null);
    }
    return cache.get(path);
  };
};

/** Every manifest's version for the unit: [{ label, path, version }]. */
export function versions(unit, root = ROOT) {
  const read = reader(root);
  return unitOf(unit).manifests.map((m) => {
    const text = read(m.path);
    return { label: m.label, path: m.path, version: text === null ? undefined : m.get(text) };
  });
}

/** The unit's version: its first manifest's. */
export function versionOf(unit, root = ROOT) {
  const v = versions(unit, root)[0].version;
  if (!v) throw new Error(`${unitOf(unit).manifests[0].path}: no version found`);
  return v;
}

/** What is wrong with the unit's versions: [] when every manifest agrees and its CHANGELOG has the version. */
export function problems(unit, root = ROOT) {
  const found = versions(unit, root);
  const want = found[0].version;
  const wrong = [];
  if (!isVersion(want)) wrong.push(`${found[0].label}: "${want ?? "no version"}" isn't major.minor.patch`);
  for (const f of found.slice(1)) {
    if (f.version !== want) wrong.push(`${f.label}: ${f.version ?? "no version"}`);
  }
  if (isVersion(want)) {
    const limit = limitProblem(unit, want);
    if (limit) wrong.push(limit);
    const changelog = join(root, UNITS[unit].changelog);
    if (!existsSync(changelog)) wrong.push(`${UNITS[unit].changelog} is missing`);
    else {
      const text = readFileSync(changelog, "utf8");
      if (!findSection(text, "Unreleased")) wrong.push(`${UNITS[unit].changelog} has no "## [Unreleased]" section`);
      if (!findSection(text, want)) wrong.push(`${UNITS[unit].changelog} has no section for ${want}`);
    }
  }
  return wrong;
}

/** The files `set` would change: [{ path, before, after }], nothing written. */
export function planSet(unit, version, root = ROOT) {
  parse(version);
  const limit = limitProblem(unit, version);
  if (limit) throw new UsageError(`${unit} ${version}: ${limit}`);
  const read = reader(root);
  const files = new Map();
  for (const m of unitOf(unit).manifests) {
    const before = files.get(m.path) ?? read(m.path);
    if (before === null) throw new Error(`${m.path} is missing`);
    if (m.get(before) === undefined) throw new Error(`${m.label}: the version wasn't found`);
    const after = m.set(before, version);
    if (m.get(after) !== version) throw new Error(`${m.label}: the version couldn't be set`);
    files.set(m.path, after);
  }
  return [...files].map(([path, after]) => ({ path, before: read(path), after })).filter((f) => f.before !== f.after);
}

export function write(changes, root = ROOT) {
  for (const c of changes) writeFileSync(join(root, c.path), c.after);
}

function main(args) {
  const [command, ...rest] = args;
  if (!command) {
    const width = Math.max(...UNIT_NAMES.map((u) => u.length));
    for (const unit of UNIT_NAMES) {
      const v = versionOf(unit);
      const extra = unit === "android" ? `  (version code ${androidVersionCode(v)})` : "";
      console.log(`${unit.padEnd(width)}  ${v}${extra}`);
    }
  } else if (command === "get") {
    unitOf(rest[0]);
    console.log(versionOf(rest[0]));
  } else if (command === "info") {
    const unit = rest[0];
    const { title } = unitOf(unit);
    const version = versionOf(unit);
    console.log(JSON.stringify({ unit, version, title, changelog: UNITS[unit].changelog }));
  } else if (command === "set") {
    const [unit, version] = rest;
    unitOf(unit);
    if (!isVersion(version)) throw new UsageError("usage: bun scripts/version.mjs set <unit> <major.minor.patch>");
    const changes = planSet(unit, version);
    write(changes);
    console.log(`${unit} ${version}${changes.length ? `: ${changes.map((c) => c.path).join(", ")}` : " (already)"}`);
  } else if (command === "check") {
    const units = rest.length ? rest : UNIT_NAMES;
    units.forEach(unitOf);
    let failed = false;
    for (const unit of units) {
      const wrong = problems(unit);
      if (wrong.length) {
        failed = true;
        console.error(`${unit} ${versions(unit)[0].version ?? "?"}, but:\n  ${wrong.join("\n  ")}`);
      } else {
        console.log(`${unit} ${versionOf(unit)}: its manifests and CHANGELOG agree`);
      }
    }
    if (failed) {
      console.error("Set a unit's version everywhere with: bun scripts/version.mjs set <unit> <version>");
      process.exit(1);
    }
  } else {
    throw new UsageError("usage: bun scripts/version.mjs [get <unit> | info <unit> | set <unit> X.Y.Z | check [unit...]]");
  }
}

if (import.meta.main) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(e instanceof UsageError ? e.message : `error: ${e.message}`);
    process.exit(2);
  }
}

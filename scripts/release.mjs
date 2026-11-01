#!/usr/bin/env bun
// Prepares a release (docs/RELEASING.md): bumps a unit's version in every
// manifest and moves its CHANGELOG's Unreleased entries under the new version.
// A release takes every unit whose version went up since the last release tag,
// under one tag, release-<date>. Merging the bumps to master starts the
// Release workflow, which builds those units, tags the commit and publishes
// the release with their notes.
//
//   bun scripts/release.mjs                                  each unit's version and what waits
//   bun scripts/release.mjs <unit> <patch|minor|major|X.Y.Z>  bump the unit and date its Unreleased entries
//   bun scripts/release.mjs <unit> draft                      add its commits since its version was set to Unreleased
//   bun scripts/release.mjs <unit> notes [X.Y.Z]              print a version's notes (default: the current one)
//   bun scripts/release.mjs pending                           the release HEAD waits for: its tag, units and notes
//
//   --dry-run      show what would change and write nothing
//   --draft        with a bump: add the commits since its version was set to Unreleased first
//   --since <ref>  draft from this commit or tag instead
//   --date <date>  the release's date, YYYY-MM-DD (default: today)
//   --commit       commit the files changed, as "<unit>: release X.Y.Z"
//   --json         the status, or the pending release, as JSON
//
// Units: server, desktop, android, samsung, lg, kodi, roku (scripts/version.mjs).

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { addEntries, entryOf, findSection, notes, plainText, release } from "./changelog.mjs";
import {
  REPOSITORY,
  ROOT,
  UNITS,
  UNIT_NAMES,
  UsageError,
  androidVersionCode,
  bump,
  compare,
  isVersion,
  planSet,
  problems,
  unitOf,
  versionOf,
} from "./version.mjs";

const color = process.stdout.isTTY && !process.env.NO_COLOR;
const paint = (code) => (s) => (color ? `\x1b[${code}m${s}\x1b[0m` : String(s));
const bold = paint(1);
const dim = paint(2);
const green = paint(32);
const yellow = paint(33);

/** A release's tag: the day it was prepared, with .2, .3 for that day's next ones. */
export const RELEASE_TAG = /^release-\d{4}-\d{2}-\d{2}(?:\.\d+)?$/;
// Releases up to 0.1.1 were tagged per unit, <unit>-v<version>.
const TAG_PATTERNS = ["release-*", ...UNIT_NAMES.map((u) => `${u}-v*`)];

function git(args, root = ROOT) {
  return execFileSync("git", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

/** The newest release tag HEAD has, or null. */
export function lastRelease(root = ROOT) {
  try {
    return git(["describe", "--tags", "--abbrev=0", ...TAG_PATTERNS.flatMap((t) => ["--match", t]), "HEAD"], root) || null;
  } catch {
    return null;
  }
}

/** The unit's version at a commit, or null. */
function versionAt(unit, ref, root) {
  const manifest = UNITS[unit].manifests[0];
  try {
    return manifest.get(git(["show", `${ref}:${manifest.path}`], root)) ?? null;
  } catch {
    return null;
  }
}

const escaped = (version) => version.replaceAll(".", "\\.");

/** The commit that set the unit's version: the newest to write the version's heading in its CHANGELOG, or null. */
export function versionCommit(unit, version, root = ROOT) {
  return git(["log", "-1", "--format=%H", "-G", `^## \\[${escaped(version)}]`, "--", UNITS[unit].changelog], root) || null;
}

/** The commits since `since` that touch the unit's files, oldest first, as `format` gives them. */
export function commitsSince(unit, since, root = ROOT, format = "%s") {
  const out = git(["log", "--no-merges", "--reverse", `--format=${format}`, `${since}..HEAD`, "--", ...UNITS[unit].paths], root);
  return out ? out.split("\n") : [];
}

const tagExists = (tag, root) => {
  try {
    git(["rev-parse", "-q", "--verify", `refs/tags/${tag}`], root);
    return true;
  } catch {
    return false;
  }
};

const today = () => {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
};

const read = (root, path) => readFileSync(join(root, path), "utf8");

/** The tag the link under a CHANGELOG gives a version, or null. */
function linkedTag(changelog, version) {
  return new RegExp(`^\\[${escaped(version)}\\]:\\s*\\S*/releases/tag/(\\S+)[ \\t]*$`, "m").exec(changelog)?.[1] ?? null;
}

/**
 * The release HEAD waits for: every unit whose version isn't the one it had
 * at the last release tag. { since, tag, title, units: [{ unit, title, from,
 * to, notes, later }], notes, problems }. A unit's `later` are its commits
 * after its version was set, which its notes may not tell.
 */
export function pending(root = ROOT) {
  const since = lastRelease(root);
  const units = [];
  const wrong = [];
  for (const unit of since ? UNIT_NAMES : []) {
    const from = versionAt(unit, since, root);
    const to = versionOf(unit, root);
    if (from === to) continue;
    const { title, changelog } = UNITS[unit];
    const text = read(root, changelog);
    wrong.push(...problems(unit, root).map((p) => `${unit}: ${p}`));
    if (isVersion(from) && isVersion(to) && compare(to, from) < 0) wrong.push(`${unit} went down, from ${from} to ${to}`);
    if (findSection(text, "Unreleased")?.body) {
      wrong.push(`${changelog} has entries under Unreleased, which would ship in ${unit} ${to} without being in its notes: move them under ${to}`);
    }
    const tag = linkedTag(text, to);
    if (!RELEASE_TAG.test(tag ?? "")) wrong.push(`${changelog} doesn't link ${to} to a release-<date> tag: bump ${unit} with scripts/release.mjs`);
    let body = "";
    try {
      body = notes(text, to);
    } catch (e) {
      wrong.push(`${changelog}: ${e.message}`);
    }
    const set = versionCommit(unit, to, root);
    units.push({ unit, title, from, to, tag, notes: body, later: set ? commitsSince(unit, set, root, "%h %s") : [] });
  }
  const tags = [...new Set(units.map((u) => u.tag).filter(Boolean))];
  if (tags.length > 1) wrong.push(`the units waiting link their versions to different tags: ${tags.join(", ")}`);
  const tag = tags[0] ?? null;
  if (tag && tagExists(tag, root)) wrong.push(`the tag ${tag} exists already`);
  const title = units.length === 1 ? `${units[0].title} ${units[0].to}` : `Pley ${tag?.replace(/^release-/, "") ?? ""}`.trim();
  const body = units.length === 1 ? units[0].notes : units.map((u) => `## ${u.title} ${u.to}\n\n${u.notes}`).join("\n\n");
  return { since, tag, title, units, notes: body, problems: wrong };
}

/** The tag for a release prepared on `date`: the one HEAD waits for, else that day's first free one. */
function nextTag(waiting, date, root) {
  if (waiting.tag) return waiting.tag;
  for (let n = 1; ; n++) {
    const tag = `release-${date}${n > 1 ? `.${n}` : ""}`;
    if (!tagExists(tag, root)) return tag;
  }
}

/** The commits to draft from: since `since`, else since the unit's version was set. */
function draftSource(unit, since, root) {
  const from = since ?? versionCommit(unit, versionOf(unit, root), root);
  if (!from) {
    throw new UsageError(`no commit writes ${unit}'s version ${versionOf(unit, root)} in its CHANGELOG: pass --since <commit or tag>`);
  }
  try {
    git(["rev-parse", "--verify", "-q", `${from}^{commit}`], root);
  } catch {
    throw new UsageError(`--since ${from}: no such commit or tag`);
  }
  return from;
}

/** Unreleased with the commits since `from` added. */
export function draft(unit, text, from, root = ROOT) {
  const subjects = commitsSince(unit, from, root);
  const entries = subjects.map(entryOf).filter(Boolean);
  return { ...addEntries(text, entries), commits: subjects.length };
}

/** Kodi shows the add-on's <news> as its changelog: the new version's notes, as plain text. */
function kodiNews(addonXml, version, date, body) {
  const escape = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  let news = plainText(version, date, body);
  if (news.length > 1500) news = `${news.slice(0, 1497).replace(/\s+\S*$/, "")}...`;
  if (!/<news>[\s\S]*?<\/news>/.test(addonXml)) throw new Error("addon.xml has no <news> element");
  return addonXml.replace(/<news>[\s\S]*?<\/news>/, () => `<news>${escape(news)}</news>`);
}

/**
 * What releasing the unit changes, nothing written: { from, to, tag, date,
 * drafted, changes: [{ path, before, after, what }], notes }. Units bumped
 * before it and not released yet share its tag.
 */
export function plan({ unit, how, date = today(), withDraft = false, since = null, root = ROOT }) {
  const { changelog } = unitOf(unit);
  if (!/^\d{4}-\d{2}-\d{2}$/.test(date)) throw new UsageError(`--date ${date}: not YYYY-MM-DD`);
  const wrong = problems(unit, root);
  if (wrong.length) throw new UsageError(`${unit}'s versions disagree; fix them first:\n  ${wrong.join("\n  ")}`);
  const from = versionOf(unit, root);
  const to = bump(from, how);
  if (compare(to, from) <= 0) throw new UsageError(`${unit} is at ${from}: ${to} isn't above it`);
  const waiting = pending(root);
  if (waiting.units.some((u) => u.unit === unit)) {
    throw new UsageError(`${unit} ${from} isn't released yet (it waits for ${waiting.tag}): edit its notes under ${from} rather than bumping it again`);
  }
  const tag = nextTag(waiting, date, root);

  const before = read(root, changelog);
  let text = before;
  let drafted = null;
  if (withDraft) {
    const from = draftSource(unit, since, root);
    drafted = { ...draft(unit, text, from, root), from };
    text = drafted.text;
  }
  if (!findSection(text, "Unreleased")?.body) {
    const why = withDraft ? `, and none of the ${drafted.commits} commits since ${drafted.from} gave an entry` : "";
    const hint = withDraft ? "" : ", or pass --draft to start from the commits";
    throw new UsageError(`Unreleased in ${changelog} is empty${why}: write what changed there${hint}`);
  }
  text = release(text, { version: to, date, tag, repository: REPOSITORY });
  const body = notes(text, to);

  const changes = planSet(unit, to, root).map((c) => ({ ...c, what: `version ${from} -> ${to}` }));
  changes.push({ path: changelog, before, after: text, what: `Unreleased -> ${to} (${date})` });
  if (unit === "kodi") {
    const c = changes.find((x) => x.path.endsWith("addon.xml"));
    c.after = kodiNews(c.after, to, date, body);
    c.what += ", <news>";
  }
  return { unit, from, to, tag, date, drafted, changes, notes: body };
}

function commit(unit, version, paths, root) {
  git(["add", "--", ...paths], root);
  git(["commit", "-q", "-m", `${unit}: release ${version}`, "--", ...paths], root);
  return git(["rev-parse", "--short", "HEAD"], root);
}

function printChanges(changes) {
  const width = Math.max(...changes.map((c) => c.path.length));
  for (const c of changes) console.log(`  ${c.path.padEnd(width)}  ${dim(c.what)}`);
}

function indent(text) {
  return text
    .split("\n")
    .map((l) => (l ? `    ${l}` : ""))
    .join("\n");
}

function releaseCommand(unit, how, opts) {
  const p = plan({ unit, how, date: opts.date, withDraft: opts.draft, since: opts.since, root: ROOT });
  // Only the CHANGELOG may hold uncommitted edits (the entries just written).
  if (opts.commit && !opts.dryRun) {
    const dirty = git(["status", "--porcelain", "--", ...p.changes.map((c) => c.path).filter((x) => x !== UNITS[unit].changelog)]);
    if (dirty) throw new UsageError(`--commit: these have uncommitted changes; commit or set them aside first:\n${dirty}`);
  }
  console.log(`${bold(UNITS[unit].title)} ${p.from} -> ${bold(green(p.to))}  ${dim(`(in the release tagged ${p.tag})`)}`);
  if (unit === "android") console.log(dim(`  Android version code ${androidVersionCode(p.from)} -> ${androidVersionCode(p.to)}`));
  if (p.drafted) {
    console.log(
      p.drafted.added.length
        ? `  drafted ${p.drafted.added.length} entr${p.drafted.added.length === 1 ? "y" : "ies"} from ${p.drafted.commits} commit${p.drafted.commits === 1 ? "" : "s"} under "Changed"`
        : dim(`  nothing new to draft (${p.drafted.commits} commit${p.drafted.commits === 1 ? "" : "s"} since its version was set)`),
    );
  }
  console.log("");
  printChanges(p.changes);
  console.log(`\n  ${bold(`## [${p.to}] - ${p.date}`)}\n\n${indent(p.notes)}\n`);
  if (opts.dryRun) {
    console.log(yellow("Dry run: nothing was written."));
    return;
  }
  for (const c of p.changes) writeFileSync(join(ROOT, c.path), c.after);
  const paths = p.changes.map((c) => c.path);
  const next = "Bump the other units the release takes the same way; merging the bumps to master releases them.";
  if (opts.commit) {
    const hash = commit(unit, p.to, paths, ROOT);
    console.log(`${green("Committed")} ${hash} "${unit}: release ${p.to}".`);
    console.log(next);
  } else {
    console.log(`${green("Written.")} Review the entries${p.drafted?.added.length ? " (the drafted ones especially)" : ""}, then commit:`);
    console.log(`  git commit -m "${unit}: release ${p.to}" -- ${paths.join(" ")}`);
    console.log(next);
  }
}

function draftCommand(unit, opts) {
  const { changelog } = unitOf(unit);
  const from = draftSource(unit, opts.since, ROOT);
  const before = read(ROOT, changelog);
  const d = draft(unit, before, from, ROOT);
  console.log(`${bold(UNITS[unit].title)}: ${d.commits} commit${d.commits === 1 ? "" : "s"} since ${from}`);
  if (!d.added.length) {
    console.log(dim("  nothing new for Unreleased"));
    return;
  }
  console.log(`\n${d.added.map((e) => `  - ${e}`).join("\n")}\n`);
  if (opts.dryRun) {
    console.log(yellow(`Dry run: ${changelog} wasn't written.`));
    return;
  }
  writeFileSync(join(ROOT, changelog), d.text);
  console.log(`${green("Added")} to Unreleased in ${changelog}, under "Changed": sort them into Added, Fixed and the rest, and edit them.`);
}

function notesCommand(unit, version) {
  const { changelog } = unitOf(unit);
  const v = version ?? versionOf(unit);
  const path = join(ROOT, changelog);
  if (!existsSync(path)) throw new UsageError(`${changelog} is missing`);
  try {
    process.stdout.write(`${notes(readFileSync(path, "utf8"), v)}\n`);
  } catch (e) {
    throw new UsageError(`${changelog}: ${e.message}`);
  }
}

function pendingCommand(opts) {
  const p = pending();
  if (opts.json) return console.log(JSON.stringify(p));
  if (!p.since) console.log(yellow("No release tag here yet: git fetch --tags brings GitHub's."));
  else if (!p.units.length) console.log(`Nothing waits: every unit has the version it had at ${p.since}.`);
  else {
    console.log(`${bold(p.title)}  ${dim(`(tag ${p.tag ?? "?"}, after ${p.since})`)}`);
    for (const u of p.units) {
      console.log(`  ${u.unit.padEnd(8)} ${u.from ?? "new"} -> ${green(u.to)}`);
      for (const c of u.later) console.log(dim(`           after its bump: ${c}`));
    }
  }
  if (p.problems.length) {
    console.log(`\n${p.problems.map((x) => yellow(x)).join("\n")}`);
    process.exitCode = 1;
  }
}

/** Each unit's version, the tag it waits for, its commits since its version was set and its Unreleased entries. */
export function status(root = ROOT) {
  const waiting = pending(root);
  return UNIT_NAMES.map((unit) => {
    const version = versionOf(unit, root);
    const set = versionCommit(unit, version, root);
    const body = findSection(read(root, UNITS[unit].changelog), "Unreleased")?.body ?? "";
    return {
      unit,
      version,
      waits: waiting.units.some((u) => u.unit === unit) ? waiting.tag : null,
      commits: set ? commitsSince(unit, set, root).length : null,
      entries: body.split("\n").filter((l) => /^[-*] /.test(l)).length,
    };
  });
}

function statusCommand(opts) {
  const units = status();
  if (opts.json) return console.log(JSON.stringify(units));
  const rows = units.map((u) => [u.unit, u.version, u.waits ?? "-", u.commits === null ? "-" : String(u.commits), String(u.entries)]);
  const head = ["unit", "version", "waits for", "commits since", "unreleased entries"];
  const widths = head.map((h, i) => Math.max(h.length, ...rows.map((r) => r[i].length)));
  const line = (cells) => cells.map((c, i) => c.padEnd(widths[i])).join("  ").trimEnd();
  console.log(bold(line(head)));
  for (const r of rows) console.log(line(r));
  console.log(dim("\nbun scripts/release.mjs <unit> <patch|minor|major|X.Y.Z> [--draft] [--dry-run] [--commit]"));
}

export function parseArgs(argv) {
  const opts = { dryRun: false, draft: false, commit: false, json: false, since: null, date: undefined };
  const positional = [];
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const value = () => {
      const v = argv[++i];
      if (v === undefined || v.startsWith("--")) throw new UsageError(`${a} needs a value`);
      return v;
    };
    if (a === "--dry-run" || a === "-n") opts.dryRun = true;
    else if (a === "--draft") opts.draft = true;
    else if (a === "--commit") opts.commit = true;
    else if (a === "--json") opts.json = true;
    else if (a === "--since") opts.since = value();
    else if (a === "--date") opts.date = value();
    else if (a === "--help" || a === "-h") opts.help = true;
    else if (a.startsWith("-")) throw new UsageError(`unknown option ${a}`);
    else positional.push(a);
  }
  return { positional, opts };
}

const USAGE = `usage:
  bun scripts/release.mjs [--json]                         each unit's version and what waits
  bun scripts/release.mjs <unit> <patch|minor|major|X.Y.Z>  bump the unit and date its Unreleased entries
      [--draft] [--since <ref>] [--date YYYY-MM-DD] [--dry-run] [--commit]
  bun scripts/release.mjs <unit> draft [--since <ref>] [--dry-run]
  bun scripts/release.mjs <unit> notes [X.Y.Z]
  bun scripts/release.mjs pending [--json]                  the release HEAD waits for
units: ${UNIT_NAMES.join(", ")}`;

function main(argv) {
  const { positional, opts } = parseArgs(argv);
  const [unit, action, extra] = positional;
  if (opts.help) return console.log(USAGE);
  if (!unit) return statusCommand(opts);
  if (unit === "pending") {
    if (action) throw new UsageError(`unexpected ${action}\n${USAGE}`);
    return pendingCommand(opts);
  }
  unitOf(unit);
  if (!action) throw new UsageError(`${USAGE}\n\nWhat for ${unit}? A bump (patch, minor, major or X.Y.Z), draft or notes.`);
  if (action === "notes") return notesCommand(unit, extra);
  if (extra) throw new UsageError(`unexpected ${extra}\n${USAGE}`);
  if (action === "draft") return draftCommand(unit, opts);
  return releaseCommand(unit, action, opts);
}

if (import.meta.main) {
  try {
    main(process.argv.slice(2));
  } catch (e) {
    console.error(e instanceof UsageError ? e.message : `error: ${e.message}`);
    process.exit(e instanceof UsageError ? 2 : 1);
  }
}

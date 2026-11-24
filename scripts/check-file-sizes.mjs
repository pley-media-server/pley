#!/usr/bin/env bun
// Source files stay small enough to read and review: each language has a
// limit of lines, and a file that reaches it is split rather than grown.
// Files that were past it before this check are listed in file-sizes.json
// with the lines they had: they may shrink, never grow, and --write lowers
// their numbers as they do.
//
//   bun scripts/check-file-sizes.mjs           lists what is past its limit, and fails if anything is; notes what shrank
//   bun scripts/check-file-sizes.mjs --write   lowers file-sizes.json to the files' sizes, dropping those within the limit
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";

// Lines a file may have, by extension. Rust keeps its tests in the file
// they test, hence its higher limit; scripts are the shortest.
const limits = {
  rs: 2000,
  kt: 1000,
  kts: 1000,
  swift: 1000,
  ts: 1000,
  tsx: 1000,
  js: 1000,
  mjs: 1000,
  cs: 1000,
  brs: 1000,
  py: 1000,
  css: 1000,
  sh: 500,
  ps1: 500,
};

const listFile = new URL("file-sizes.json", import.meta.url);
const listed = JSON.parse(readFileSync(listFile, "utf8"));
const write = process.argv.includes("--write");
const number = (n) => n.toLocaleString("en-US");

const sizes = new Map();
// Files not committed yet count too, so the check can run before a commit.
const files = execFileSync("git", ["ls-files", "-z", "--cached", "--others", "--exclude-standard"], { encoding: "utf8" });
for (const file of files.split("\0")) {
  const extension = file.slice(file.lastIndexOf(".") + 1);
  if (!file.includes(".") || !(extension in limits) || !existsSync(file)) continue;
  const bytes = readFileSync(file);
  // Binary files named as source (MPEG-TS test media) aren't source.
  if (bytes.includes(0)) continue;
  const text = bytes.toString("utf8");
  sizes.set(file, text.length === 0 ? 0 : text.split("\n").length - (text.endsWith("\n") ? 1 : 0));
}

const problems = [];
// Listed numbers that can come down: not a failure, so that pull requests
// that shrink the same file don't clash over the list.
const notes = [];
const kept = {};
for (const [file, lines] of sizes) {
  const limit = limits[file.slice(file.lastIndexOf(".") + 1)];
  const allowed = listed[file];
  if (allowed === undefined) {
    if (lines > limit) {
      problems.push(`${file}: ${number(lines)} lines, past the ${number(limit)} a .${file.split(".").pop()} file may have. Split it into smaller files (a file renamed takes its old name's place in the list).`);
    }
  } else if (lines > allowed) {
    problems.push(`${file}: ${number(lines)} lines, past the ${number(allowed)} it had. Move code out of it rather than into it.`);
  } else if (lines < allowed) {
    notes.push(`${file}: ${number(lines)} lines, fewer than the ${number(allowed)} listed.`);
  }
  if (allowed !== undefined && lines > limit) kept[file] = Math.min(lines, allowed);
}
for (const file of Object.keys(listed)) {
  if (!sizes.has(file)) notes.push(`${file} is listed but gone.`);
}

if (write) {
  const sorted = Object.fromEntries(Object.keys(kept).sort().map((file) => [file, kept[file]]));
  writeFileSync(listFile, `${JSON.stringify(sorted, null, 2)}\n`);
}
if (notes.length > 0 && !write) {
  console.log(`${notes.join("\n")}\n--write lowers or drops their numbers in scripts/file-sizes.json.\n`);
}
if (problems.length > 0) {
  console.error(problems.join("\n"));
  console.error(
    "\nThe limits are in scripts/check-file-sizes.mjs; the files past them from before, in scripts/file-sizes.json.",
  );
  process.exit(1);
}
console.log(`${sizes.size} source files within their limits`);

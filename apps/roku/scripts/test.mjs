// Runs the unit tests of the app's plain BrightScript (components/lib) on
// this computer with brs-node, lvcabral's BrightScript engine: no Roku and
// no SceneGraph. Every `sub test_…()` in tests/*.test.brs is a test; this
// script writes the Main that runs them all, then reads the results. The
// catalogs of words are checked first (scripts/locales.mjs).
//
//   node scripts/test.mjs            failures and the count
//   node scripts/test.mjs --verbose  every test

import { createRequire } from "node:module";
import { readdirSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { checkLocales } from "./locales.mjs";

const require = createRequire(import.meta.url);
const brs = require("brs-node");

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const verbose = process.argv.includes("--verbose");

const wordProblems = checkLocales(root);
for (const problem of wordProblems) console.log(`words: ${problem}`);
console.log(`words: ${wordProblems.length === 0 ? "every language has every word" : `${wordProblems.length} problems`}`);

const brsFiles = (dir) =>
  readdirSync(join(root, dir))
    .filter((name) => name.endsWith(".brs"))
    .sort()
    .map((name) => `${dir}/${name}`);

const libs = brsFiles("components/lib");
const tests = brsFiles("tests");

const testNames = [];
for (const file of tests.filter((f) => f.endsWith(".test.brs"))) {
  const source = readFileSync(join(root, file), "utf8");
  for (const match of source.matchAll(/^\s*sub\s+(test_[A-Za-z0-9_]+)\s*\(\s*\)/gim)) {
    testNames.push(match[1]);
  }
}
if (testNames.length === 0) {
  console.error("no tests found in tests/*.test.brs");
  process.exit(1);
}

const main = [
  "sub Main()",
  ...testNames.map((name) => `    tkRun("${name}", ${name})`),
  "    tkFinish()",
  "end sub",
  "",
].join("\n");

// The engine reads a file as code when its path is under source/.
const files = new Map();
const pathOf = new Map();
for (const file of [...libs, ...tests]) {
  const packaged = `source/${file}`;
  files.set(packaged, new Blob([readFileSync(join(root, file))]));
  pathOf.set(`pkg:/${packaged}`, file);
}
files.set("source/run.brs", new Blob([main]));
// The rules every TV app shares (packages/tv/test/fixtures), which the
// tests read as pkg:/fixtures/<name>.json.
const fixtures = join(root, "../../packages/tv/test/fixtures");
for (const name of ["remote.json", "language.json"]) {
  files.set(`fixtures/${name}`, new Blob([readFileSync(join(fixtures, name))]));
}
// The words, as the package has them (pkg:/locales/<language>/<area>.json).
for (const language of readdirSync(join(root, "locales"))) {
  for (const name of readdirSync(join(root, "locales", language)).filter((n) => n.endsWith(".json"))) {
    files.set(`locales/${language}/${name}`, new Blob([readFileSync(join(root, "locales", language, name))]));
  }
}
files.set("manifest", new Blob([readFileSync(join(root, "manifest"))]));

const lines = [];
brs.registerCallback((message) => {
  if (typeof message !== "string") return;
  const comma = message.indexOf(",");
  const kind = comma < 0 ? message : message.slice(0, comma);
  let text = comma < 0 ? "" : message.slice(comma + 1);
  for (const [packaged, file] of pathOf) text = text.split(packaged).join(file);
  if (kind === "print") lines.push(...text.replace(/\n$/, "").split("\n"));
  else if (kind === "warning" || kind === "error") lines.push(`${kind}: ${text.trim()}`);
});

const payload = await brs.createPayloadFromFileMap(files, {});
const result = await brs.executeFile(payload);

let summary = null;
let failed = false;
let current = null;
for (const line of lines) {
  if (line.startsWith("ok ")) {
    current = null;
    if (verbose) console.log(line);
  } else if (line.startsWith("not ok ")) {
    failed = true;
    current = line;
    console.log(line);
  } else if (line.startsWith("tests: ")) {
    summary = line;
  } else if (current !== null || verbose || line.startsWith("error: ")) {
    console.log(line);
  }
}

if (summary === null) {
  console.error(`the tests didn't finish (${result?.exitReason ?? "no exit reason"})`);
  process.exit(1);
}
console.log(summary);
process.exit(failed || wordProblems.length > 0 ? 1 : 0);

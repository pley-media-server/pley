#!/usr/bin/env bun
// Pley's English is American. This finds British spellings in the words
// people read most: the docs, the apps' English words and the unreleased
// changelog lines. Code, addresses and quoted names are left out, as they
// keep the spelling of what they name (XMLTV's <programme>, for one).
//
//   bun scripts/check-spelling.mjs          lists what it finds, and fails if anything
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const british = [
  "colour(s|ed|ing|ful|less)?",
  "favourite(s|d)?",
  "behaviour(s|al)?",
  "honour(s|ed)?",
  "neighbour(s|ing)?",
  "humour",
  "centre(s|d)?",
  "metre(s)?",
  "litre(s)?",
  "theatre(s)?",
  "fibre(s)?",
  "licence(s|d)?",
  "defence",
  "offence",
  "cancell(ed|ing)",
  "programme(s|'s)?",
  "catalogue(s|d)?",
  "grey(s|ed)?",
  "(label|travel|model|signal|level|channel|fuel|total)l(ed|ing)",
  "analys(e|ed|ing|er)",
  "judgement(s)?",
  "ageing",
  "practis(e|ed|ing)",
  "spelt",
  "learnt",
  "whilst",
  "artefact(s)?",
  "analogue",
  "enquir(y|ies)",
  "(organ|recogn|custom|priorit|minim|maxim|optim|normal|initial|serial|synchron|util|summar|categor|author|sanit|personal|memor|standard|visual|real|final|capital)is(e|ed|es|ing|ation|ations|er|able)",
];
const word = new RegExp(`\\b(${british.join("|")})\\b`, "gi");
// Code, link targets, tags, quoted names and addresses keep their spelling.
const kept = /(`[^`]*`|\]\([^)]*\)|<\/?[A-Za-z][^>]*>|"[A-Za-z0-9_./-]*"|'[A-Za-z0-9_./-]+'|https?:\/\/\S+)/g;

const files = execFileSync("git", ["ls-files"], { encoding: "utf8" })
  .split("\n")
  .filter(
    (f) =>
      (f.endsWith(".md") && !/(^|\/)LICENSE/.test(f)) ||
      /\/locales\/en(\/[^/]+)?\.json$/i.test(f) ||
      /\/res\/values\/strings[^/]*\.xml$/.test(f) ||
      /\/resource\.language\.en_gb\/strings\.po$/.test(f),
  );

const found = [];
for (const file of files) {
  const lines = readFileSync(file, "utf8").split("\n");
  let fence = false;
  for (const [i, line] of lines.entries()) {
    // A changelog's released sections are history, as they were written.
    if (file.endsWith("CHANGELOG.md") && /^## \[(?!Unreleased)/.test(line)) break;
    if (line.trimStart().startsWith("```")) {
      fence = !fence;
      continue;
    }
    if (fence) continue;
    let text = line;
    // In the apps' words, only what people read: values, not keys or names.
    if (file.endsWith(".json")) text = [...line.matchAll(/:\s*"((?:[^"\\]|\\.)*)"/g)].map((m) => m[1]).join(" ");
    else if (file.endsWith(".xml")) text = [...line.matchAll(/>([^<]+)</g)].map((m) => m[1]).join(" ");
    else if (file.endsWith(".po")) text = /^(msgid|msgstr|#)/.test(line) ? line : "";
    for (const m of text.replace(kept, " ").matchAll(word)) found.push(`${file}:${i + 1}: ${m[0]}`);
  }
}

if (found.length) {
  console.error(found.join("\n"));
  console.error(`\n${found.length} British spellings. Pley's English is American: color, favorite, center, license, canceled, program, -ize.`);
  process.exit(1);
}
console.log(`Spelling: ${files.length} files, all American.`);

// Checks the app's words (locales/<language>/<area>.json) and the code that
// names them, as the other apps' tests check theirs: every language has
// English's areas and keys, every plural form it counts in and the same
// {{values}}; French and Spanish keep their punctuation; every key the
// code names with t("…") is in English, and every English word is named
// by the code. scripts/test.mjs runs it before the unit tests.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const LANGUAGES = ["en", "fr", "es"];
// The plural forms each language counts in, as CLDR has them.
const FORMS = { en: ["one", "other"], fr: ["one", "many", "other"], es: ["one", "many", "other"] };
const PLURAL = /_(zero|one|two|few|many|other)$/;
const VALUE = /\{\{\s*([^,}\s]+)[^}]*\}\}/g;

function files(dir, test) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) out.push(...files(path, test));
    else if (test(name)) out.push(path);
  }
  return out;
}

// { "servers.title": "Choose your server" } from an area's nested words;
// problems with anything that isn't words, or keys only case tells apart
// (BrightScript's associative arrays may not).
function flatten(json, at, out, problems) {
  const seen = new Map();
  for (const [key, value] of Object.entries(json)) {
    const path = at ? `${at}.${key}` : key;
    const lower = key.toLowerCase();
    if (seen.has(lower)) problems.push(`${path}: only its case tells it from ${seen.get(lower)}`);
    seen.set(lower, key);
    if (key.includes(".")) problems.push(`${path}: a key has no dot in it`);
    if (value && typeof value === "object" && !Array.isArray(value)) flatten(value, path, out, problems);
    else if (typeof value === "string") out[path] = value;
    else problems.push(`${path} isn't words`);
  }
  return out;
}

const values = (key, text) =>
  [...text.matchAll(VALUE)]
    .map((m) => m[1])
    // A form for one may leave the count out ("un film").
    .filter((v) => !(v === "count" && key.endsWith("_one")))
    .sort()
    .join(", ");

export function checkLocales(root) {
  const problems = [];
  const catalogs = {};
  for (const language of LANGUAGES) {
    catalogs[language] = {};
    for (const name of readdirSync(join(root, "locales", language)).filter((n) => n.endsWith(".json")).sort()) {
      const area = name.slice(0, -5);
      if (!/^[a-z][a-zA-Z0-9]*$/.test(area)) problems.push(`${language}/${name}: an area is named in letters and digits`);
      let json;
      try {
        json = JSON.parse(readFileSync(join(root, "locales", language, name), "utf8"));
      } catch (e) {
        problems.push(`${language}/${name}: ${e.message}`);
        continue;
      }
      const areaProblems = [];
      catalogs[language][area] = flatten(json, "", {}, areaProblems);
      problems.push(...areaProblems.map((p) => `${language}/${area}: ${p}`));
    }
  }

  const english = catalogs.en;
  const areas = Object.keys(english).sort();
  if (!areas.includes("common")) problems.push("en: no common area");
  for (const language of LANGUAGES) {
    const have = Object.keys(catalogs[language]).sort();
    if (have.join() !== areas.join()) problems.push(`${language}: areas ${have.join(", ")}, English has ${areas.join(", ")}`);
  }

  for (const [area, words] of Object.entries(english)) {
    for (const key of Object.keys(words).filter((k) => PLURAL.test(k))) {
      const stem = key.replace(PLURAL, "");
      if (!(`${stem}_one` in words && `${stem}_other` in words)) problems.push(`en/${area}: ${stem} needs _one and _other`);
    }
  }

  for (const language of LANGUAGES.filter((l) => l !== "en")) {
    for (const [area, source] of Object.entries(english)) {
      const words = catalogs[language][area];
      if (!words) continue;
      const expected = new Set();
      for (const key of Object.keys(source)) {
        if (!PLURAL.test(key)) expected.add(key);
        else for (const form of FORMS[language]) expected.add(key.replace(PLURAL, `_${form}`));
      }
      for (const key of expected) if (!(key in words)) problems.push(`${language}/${area}: ${key} is missing`);
      for (const [key, text] of Object.entries(words)) {
        if (!expected.has(key)) {
          problems.push(`${language}/${area}: ${key} isn't in English`);
          continue;
        }
        if (!text.trim()) problems.push(`${language}/${area}: ${key} is empty`);
        const from = key in source ? source[key] : source[key.replace(PLURAL, "_other")];
        if (values(key, from) !== values(key, text)) problems.push(`${language}/${area}: ${key} has {{${values(key, text)}}}, English {{${values(key, from)}}}`);
      }
    }
  }

  for (const language of LANGUAGES) {
    for (const [area, words] of Object.entries(catalogs[language])) {
      for (const [key, text] of Object.entries(words)) {
        const where = `${language}/${area}: ${key}`;
        if (text.includes("...")) problems.push(`${where} has three dots (…): ${text}`);
        if (language === "en") continue;
        if (text.includes("'")) problems.push(`${where} has a straight apostrophe (’): ${text}`);
        if (language === "fr") {
          // A no-break space (U+00A0) before : ; ? ! » and after «, never an ordinary one or none.
          if (/ [:;?!»]|« /.test(text)) problems.push(`${where} has an ordinary space where a no-break one goes: ${text}`);
          if (/[\p{L}\d}][;?!»]|[\p{L}\d}]:(\s|$)|«[\p{L}{]/u.test(text)) problems.push(`${where} needs a no-break space: ${text}`);
        }
        if (language === "es") {
          const count = (c) => [...text].filter((x) => x === c).length;
          if (count("?") > count("¿")) problems.push(`${where} closes a question it doesn't open (¿…?): ${text}`);
          if (count("!") > count("¡")) problems.push(`${where} closes an exclamation it doesn't open (¡…!): ${text}`);
        }
      }
    }
  }

  // Every key the code names whole: t("area.key") or t("area.key", { … }).
  const named = /\bt\(\s*"([^"]+)"\s*[,)]/g;
  let found = 0;
  let code = "";
  for (const file of files(join(root, "components"), (n) => n.endsWith(".brs"))) {
    // Comments name keys too, as examples.
    const source = readFileSync(file, "utf8")
      .split("\n")
      .filter((line) => !line.trimStart().startsWith("'"))
      .join("\n");
    code += source + "\n";
    for (const m of source.matchAll(named)) {
      found++;
      const key = m[1];
      const area = key.split(".")[0];
      const rest = key.slice(area.length + 1);
      const words = english[area] ?? {};
      if (!(rest in words) && !(`${rest}_one` in words && `${rest}_other` in words)) {
        problems.push(`${relative(root, file)}: t("${key}") isn't in English`);
      }
    }
  }
  if (found === 0) problems.push("no t(\"…\") found in components/");

  // And every English word is shown somewhere: its key is in the code
  // whole ("area.key", t()'s or kept for it), or under a prefix the code
  // builds keys from ("area.sorts." + id). A word nothing shows is left
  // over from a change.
  for (const [area, words] of Object.entries(english)) {
    for (const key of Object.keys(words)) {
      const parts = `${area}.${key.replace(PLURAL, "")}`.split(".");
      let shown = code.includes(`"${parts.join(".")}"`);
      for (let i = parts.length - 1; i >= 1 && !shown; i--) shown = code.includes(`"${parts.slice(0, i).join(".")}."`);
      if (!shown) problems.push(`en/${area}: ${key} isn't shown anywhere`);
    }
  }
  return problems;
}

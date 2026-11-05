// Verifies the licensing rules (LICENSING.md) for the native Samsung app's
// .NET packages: every NuGet package its projects ship with, transitive ones
// included, must be under a permissive licence. Copyleft licences ((L/A)GPL,
// MPL and the like) and licences this script doesn't know fail it, so a new
// one is looked at before it ships. The tests' packages don't ship and aren't
// checked. Run after a build (`dotnet build`): licences are read from the
// packages' .nuspec files in the NuGet cache.
//
// It also checks the app's NOTICES.txt (shared/res, so in the .tpk): the full
// licence of each package whose files the .tpk carries, as their licences ask
// of a copy. `--write` writes it again, after a package changes.
// Exit code 1 on violations.

import { execFileSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const projects = ["apps/tizen-native/src/Pley.Tv.Core", "apps/tizen-native/src/Pley.Tv.Tizen"];
const app = join(root, "apps/tizen-native/src/Pley.Tv.Tizen");
const noticesFile = join(app, "shared", "res", "NOTICES.txt");
const cache = process.env.NUGET_PACKAGES ?? join(homedir(), ".nuget", "packages");
const write = process.argv.includes("--write");

const ALLOWED = new Set(["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "0BSD"]);

// Licence URLs older packages give instead of an SPDX expression.
const URLS = [
  [/^https?:\/\/(www\.)?apache\.org\/licenses\/LICENSE-2\.0(\.txt|\.html)?$/i, "Apache-2.0"],
  [/^https?:\/\/opensource\.org\/licenses\/MIT$/i, "MIT"],
  [/^https?:\/\/github\.com\/dotnet\/(corefx|runtime|standard)\/blob\/(master|main)\/LICENSE\.TXT$/i, "MIT"],
];

// Packages NuGet reads while restoring that put nothing into the app, so their licence doesn't reach it.
const NOT_SHIPPED = {
  // Its old versions are under Microsoft's .NET library licence; they hold only runtime.json and empty `_._` folders.
  "Microsoft.NETCore.Platforms": "only the runtime graph NuGet restores with",
};

const MIT = `Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.`;

// Licence texts of packages that carry none as a file: their repository's LICENSE at the commit
// their .nuspec names.
const TEXTS = {
  "Net.Codecrete.QrCodeGenerator": `MIT License\n\nCopyright (c) 2018 Manuel Bleichenbacher\n\n${MIT}`,
};

const packageDir = (id, version) => join(cache, id.toLowerCase(), version.toLowerCase());

/** The package's .nuspec, or null when it isn't in the cache. */
function nuspecOf(id, version) {
  const nuspec = join(packageDir(id, version), `${id.toLowerCase()}.nuspec`);
  return existsSync(nuspec) ? readFileSync(nuspec, "utf8") : null;
}

/** The package's licence as an SPDX expression, from its .nuspec; null when it can't be told. */
function licenseOf(id, version) {
  const xml = nuspecOf(id, version);
  if (!xml) return null;
  const expression = /<license\s+type="expression"\s*>([^<]+)<\/license>/.exec(xml);
  if (expression) return expression[1].trim();
  const url = /<licenseUrl>([^<]+)<\/licenseUrl>/.exec(xml)?.[1].trim();
  if (!url) return null;
  const nuget = /^https:\/\/licenses\.nuget\.org\/(.+)$/.exec(url);
  if (nuget) return decodeURIComponent(nuget[1]);
  return URLS.find(([pattern]) => pattern.test(url))?.[1] ?? null;
}

/** Whether an SPDX expression of ORs and ANDs (without parentheses) leaves a permissive choice. */
function permitted(expression) {
  return expression
    .split(/\s+OR\s+/)
    .some((choice) => choice.split(/\s+AND\s+/).every((l) => ALLOWED.has(l.replace(/[()]/g, "").trim())));
}

/** A file of the package's folder whose name matches, as LF text without trailing spaces; null when none does. */
function packageText(id, version, name) {
  const dir = packageDir(id, version);
  const file = existsSync(dir) ? readdirSync(dir).find((f) => name.test(f)) : undefined;
  if (!file) return null;
  return readFileSync(join(dir, file), "utf8")
    .replace(/^﻿/, "")
    .replace(/\r\n?/g, "\n")
    .replace(/[ \t]+$/gm, "")
    .trim();
}

/**
 * The packages the .tpk carries, from the newest build's deps.json: those it gives files to run.
 * The rest (Tizen.NET, .NET Standard's) are the TV's own. Null without a build.
 */
function shippedPackages() {
  const deps = ["Release", "Debug"]
    .map((c) => join(app, "bin", c, "tizen90", "Pley.Tv.Tizen.deps.json"))
    .filter((f) => existsSync(f))
    .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0];
  if (!deps) return null;
  const json = JSON.parse(readFileSync(deps, "utf8"));
  const found = new Map();
  for (const target of Object.values(json.targets ?? {}))
    for (const [key, lib] of Object.entries(target)) {
      if (json.libraries?.[key]?.type !== "package" || !Object.keys(lib.runtime ?? {}).length) continue;
      const [id, version] = key.split("/");
      found.set(key, { id, version });
    }
  return [...found.values()].sort((a, b) => a.id.localeCompare(b.id));
}

/** NOTICES.txt for the packages shipped: a list, then each licence in full. Pushes what it lacks onto `failures`. */
function noticesText(shipped, failures) {
  const line = "-".repeat(72);
  const parts = [
    "Pley for Samsung TV (native) includes the following open-source software,",
    "each under its licence, copied in full below. Pley is under the GNU AGPL,",
    "version 3 only (LICENSE.txt beside this file; LICENSING.md says the rest).",
    "The icons' and the typeface's licences are in icons/LICENSE.txt and",
    "fonts/LICENSE.txt.",
    "",
    ...shipped.map((p) => `- ${p.id} ${p.version} (${licenseOf(p.id, p.version) ?? "?"})`),
  ];
  // .NET's packages carry the same third-party notices, those of the whole runtime: each text once.
  const noticesOf = new Map();
  for (const p of shipped) {
    const text = packageText(p.id, p.version, /^licen[cs]e(\.(txt|md))?$/i) ?? TEXTS[p.id];
    if (!text) failures.push(`${p.id} ${p.version}: no licence file in the package; add its text to TEXTS in scripts/check-nuget-licenses.mjs`);
    const url = /<projectUrl>([^<]+)<\/projectUrl>/.exec(nuspecOf(p.id, p.version) ?? "")?.[1].trim();
    parts.push("", line, `${p.id} ${p.version}${url ? `, ${url}` : ""}`, line, "", text ?? "");
    const notices = packageText(p.id, p.version, /^third-party-notices\.txt$/i);
    if (!notices) continue;
    if (noticesOf.has(notices)) parts.push("", `Its third-party notices are ${noticesOf.get(notices)}'s, above.`);
    else {
      noticesOf.set(notices, p.id);
      parts.push("", notices);
    }
  }
  return `${parts.join("\n")}\n`;
}

const failures = [];
const seen = new Map();
for (const project of projects) {
  const json = execFileSync("dotnet", ["list", join(root, project), "package", "--include-transitive", "--format", "json"], {
    encoding: "utf8",
  });
  for (const p of JSON.parse(json).projects ?? [])
    for (const f of p.frameworks ?? [])
      for (const pkg of [...(f.topLevelPackages ?? []), ...(f.transitivePackages ?? [])])
        seen.set(`${pkg.id}@${pkg.resolvedVersion}`, { id: pkg.id, version: pkg.resolvedVersion });
}
for (const { id, version } of [...seen.values()].sort((a, b) => a.id.localeCompare(b.id))) {
  const license = licenseOf(id, version);
  if (NOT_SHIPPED[id] && (license === null || !permitted(license))) console.log(`${id} ${version}: not shipped (${NOT_SHIPPED[id]})`);
  else if (license === null) failures.push(`${id} ${version}: licence unknown (restore first, or add its licence URL here once checked)`);
  else if (!permitted(license)) failures.push(`${id} ${version}: ${license} is not a permissive licence`);
  else console.log(`${id} ${version}: ${license}`);
}

const shipped = shippedPackages();
const where = relative(root, noticesFile).replaceAll("\\", "/");
if (!shipped) failures.push(`no build of the app to read its packages from: dotnet build ${relative(root, app).replaceAll("\\", "/")}`);
else {
  const text = noticesText(shipped, failures);
  if (write) {
    writeFileSync(noticesFile, text);
    console.log(`\nWrote ${where}: ${shipped.map((p) => p.id).join(", ")}.`);
  } else if (!existsSync(noticesFile) || readFileSync(noticesFile, "utf8").replace(/\r\n/g, "\n") !== text) {
    failures.push(`${where} doesn't have the licences of the packages the app ships: bun scripts/check-nuget-licenses.mjs --write`);
  } else console.log(`\n${where} has the licences of the ${shipped.length} packages the app ships.`);
}

if (failures.length) {
  console.error(`\n${failures.length} problem(s) with the licensing rules (LICENSING.md):`);
  for (const f of failures) console.error(`  ${f}`);
  process.exit(1);
}
console.log(`${seen.size} packages, all under permissive licences.`);

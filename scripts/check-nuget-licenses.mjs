// Verifies the licensing rules (LICENSING.md) for the native Samsung app's
// .NET packages: every NuGet package its projects ship with, transitive ones
// included, must be under a permissive licence. Copyleft licences ((L/A)GPL,
// MPL and the like) and licences this script doesn't know fail it, so a new
// one is looked at before it ships. The tests' packages don't ship and aren't
// checked. Run after a restore (`dotnet build` or `dotnet restore`): licences
// are read from the packages' .nuspec files in the NuGet cache.
// Exit code 1 on violations.

import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const projects = ["apps/tizen-native/src/Pley.Tv.Core", "apps/tizen-native/src/Pley.Tv.Tizen"];
const cache = process.env.NUGET_PACKAGES ?? join(homedir(), ".nuget", "packages");

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

/** The package's licence as an SPDX expression, from its .nuspec; null when it can't be told. */
function licenseOf(id, version) {
  const dir = join(cache, id.toLowerCase(), version.toLowerCase());
  const nuspec = join(dir, `${id.toLowerCase()}.nuspec`);
  if (!existsSync(nuspec)) return null;
  const xml = readFileSync(nuspec, "utf8");
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

if (failures.length) {
  console.error(`\n${failures.length} package(s) fail the licensing rules (LICENSING.md):`);
  for (const f of failures) console.error(`  ${f}`);
  process.exit(1);
}
console.log(`\n${seen.size} packages, all under permissive licences.`);

// Builds the native Samsung app and signs it into out/pley-tizen.tpk with
// Samsung's Tizen CLI and a security profile: PLEY_TIZEN_PROFILE (default
// "pley-dev"), as the web app's package script does. A TV in developer mode
// installs only packages signed with a Samsung certificate. The .NET SDK
// builds the .tpk; the CLI, found on PATH or in TIZEN_STUDIO, signs it.
// PLEY_TIZEN_METRICS (an address of scripts/metrics.mjs, http://<pc>:7531)
// makes a profiling build that sends its marks there; without it, none. A profiling build is an
// app of its own, app.pley.tizen.debug ("Pley (debug)"), in out/pley-tizen-debug.tpk, so it
// installs beside a release.
// --unsigned leaves the package as the .NET SDK signs it, with Tizen's default certificate, which
// no TV takes: out/pley-tizen-unsigned.tpk, the release's package, which whoever installs it signs
// with their own Samsung certificate. It needs no Tizen CLI.

import { execFileSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, readdirSync, rmSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { commit } from "../../../scripts/commit.mjs";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const project = join(root, "src", "Pley.Tv.Tizen");
const bin = join(project, "bin", "Release");
const out = join(root, "out");
const profile = process.env.PLEY_TIZEN_PROFILE ?? "pley-dev";
const unsigned = process.argv.includes("--unsigned");
const win = process.platform === "win32";
const studio = process.env.TIZEN_STUDIO;
const cli = studio ? join(studio, "tools", "ide", "bin", win ? "tizen.bat" : "tizen") : win ? "tizen.bat" : "tizen";
if (!unsigned && studio && !existsSync(cli)) {
  console.error(`No Tizen CLI at ${cli}`);
  process.exit(1);
}

// The commit goes into the informational version, which the app shows beside its version.
// Always given, so a build after a profiling one compiles the profiling out again.
const metrics = process.env.PLEY_TIZEN_METRICS ?? "";
// A release and a profiling build share the package's staging folder, and the SDK copies a file
// there only when it is newer than the one there: the icon of the build before would stay (DEBUG
// on a release, or none on a profiling build). Emptied first, it is the build's own.
if (existsSync(bin)) {
  for (const framework of readdirSync(bin)) rmSync(join(bin, framework, "tpkroot"), { recursive: true, force: true });
}
execFileSync("dotnet", ["build", project, "-c", "Release", "-nologo", `-p:SourceRevisionId=${commit()}`, `-p:PleyMetrics=${metrics}`], {
  stdio: "inherit",
});
if (metrics) console.log(`A profiling build: its marks go to ${metrics}`);

const tpk = readdirSync(bin, { recursive: true })
  .filter((f) => String(f).endsWith(".tpk"))
  .map((f) => join(bin, String(f)))
  .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0];
if (!tpk) {
  console.error("The build made no .tpk");
  process.exit(1);
}

// The SDK signs with Tizen's default certificate; this signs it again with the profile's.
if (!unsigned) {
  const args = ["package", "-t", "tpk", "-s", profile, "--", tpk];
  // Windows runs the CLI's .bat through cmd.
  if (win) execFileSync("cmd.exe", ["/d", "/c", cli].concat(args), { stdio: "inherit" });
  else execFileSync(cli, args, { stdio: "inherit" });
}

mkdirSync(out, { recursive: true });
const name = `${metrics ? "pley-tizen-debug" : "pley-tizen"}${unsigned ? "-unsigned" : ""}.tpk`;
copyFileSync(tpk, join(out, name));
console.log(`out/${name} (${unsigned ? "with Tizen's default certificate only" : `signed with the "${profile}" profile`})`);

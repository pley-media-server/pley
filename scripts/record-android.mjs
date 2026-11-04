// Records an animation of the Android app, phone or TV, frame by frame, for a
// pull request: the frames mark-animation.mjs turns into an animated PNG, as
// the native Samsung app's `metrics.mjs record` gives them (docs/DEVELOPMENT.md,
// "Screenshots for a pull request"). It needs only adb and FFmpeg, and no
// change to the app: the build installed is the one recorded.
//
//   bun scripts/record-android.mjs 20 6000 0,0,960,1080 Right [--serial <device>] [--out <folder>]
//
// The device's animations run 20 times slower (its animator duration scale,
// which Compose follows at once), the keys are pressed (Up, Down, Left, Right,
// OK, Back, Menu, 300 ms apart), and the screen is recorded for 6 s, cut to
// that area of it, in the recording's pixels: the display's size, or the
// override size where `adb shell wm size` shows one. The frames go to
// apps/android/build/rec-<time>-<model>/ as NNN-<ms>.jpg, ms when each was
// caught at the animation's own speed; the scale goes back to what it was.
// screenrecord gives a frame each time the screen changes, up to 60 a second,
// but some devices give five or so a second while an animation plays slowly:
// at 20 times slower, that is still a frame every 10 ms of the animation. A
// video's picture comes out only where the device lets it be recorded.

import { execFileSync, spawn, spawnSync } from "node:child_process";
import { mkdirSync, readdirSync, renameSync, rmSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const KEYS = { Up: "DPAD_UP", Down: "DPAD_DOWN", Left: "DPAD_LEFT", Right: "DPAD_RIGHT", OK: "DPAD_CENTER", Back: "BACK", Menu: "MENU" };
// The recording starts this long before the first key, so the first frames show the screen still.
const LEAD_MS = 1000;
const ON_DEVICE = "/sdcard/pley-recording.mp4";

const words = [];
let serial = process.env.ANDROID_SERIAL ?? "";
let out = "";
const args = process.argv.slice(2);
for (let i = 0; i < args.length; i++) {
  if (args[i] === "--serial") serial = args[++i] ?? "";
  else if (args[i] === "--out") out = args[++i] ?? "";
  else words.push(args[i]);
}
const [slowWord, forWord, areaWord, ...keys] = words;
const slow = Number(slowWord);
const forMs = Number(forWord);
const area = (areaWord ?? "").split(",").map(Number);
if (!(slow >= 1) || !(forMs > 0) || area.length !== 4 || area.some((n) => !Number.isInteger(n) || n < 0) || !(area[2] > 0 && area[3] > 0)) usage();
const unknown = keys.filter((k) => !KEYS[k]);
if (unknown.length) usage(`Not a key: ${unknown.join(" ")}`);

const adb = (...rest) => execFileSync("adb", [...(serial ? ["-s", serial] : []), ...rest], { encoding: "utf8" }).trim();
const shell = (command) => adb("shell", command);

const model = shell("getprop ro.product.model").replace(/[^A-Za-z0-9_-]+/g, "_") || "android";
const dir = out || join(dirname(fileURLToPath(import.meta.url)), "..", "apps", "android", "build", `rec-${new Date().toISOString().slice(11, 19).replace(/:/g, "")}-${model}`);
mkdirSync(dir, { recursive: true });

// The scale before, "null" where it was never set, put back however the recording ends.
const before = shell("settings get global animator_duration_scale");
let restored = false;
const restore = () => {
  if (restored) return;
  restored = true;
  shell(before === "null" ? "settings delete global animator_duration_scale" : `settings put global animator_duration_scale ${before}`);
};
process.on("SIGINT", () => {
  restore();
  process.exit(130);
});

const video = join(dir, "recording.mp4");
try {
  shell(`settings put global animator_duration_scale ${slow}`);
  shell(`rm -f ${ON_DEVICE}`);
  // screenrecord's limit is in whole seconds; FFmpeg cuts the rest.
  const seconds = Math.ceil((LEAD_MS + forMs) / 1000);
  const recorder = spawn("adb", [...(serial ? ["-s", serial] : []), "shell", `screenrecord --bit-rate 20000000 --time-limit ${seconds} ${ON_DEVICE}`], { stdio: "inherit" });
  // Settled with what went wrong rather than rejected: screenrecord can fail during the lead,
  // before anything awaits it, and an unhandled rejection would end the script past `finally`,
  // leaving the device's animations slowed.
  let failed = null;
  const recorded = new Promise((resolve) => {
    recorder.on("error", (error) => resolve((failed = error)));
    recorder.on("exit", (code) => resolve(code === 0 ? null : (failed = new Error(`screenrecord ended with ${code}`))));
  });
  await sleep(LEAD_MS);
  for (const key of failed ? [] : keys) {
    shell(`input keyevent KEYCODE_${KEYS[key]}`);
    await sleep(300);
  }
  const error = await recorded;
  if (error) throw error;
  restore();
  adb("pull", ON_DEVICE, video);
  shell(`rm -f ${ON_DEVICE}`);
} finally {
  restore();
}

// The recording's size: the display's, or its override size where it has one (`adb shell wm size`
// lists both), which isn't always the size people expect.
const [width, height] = execFileSync("ffprobe", ["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width,height", "-of", "csv=p=0", video], { encoding: "utf8" })
  .trim().split(",").map(Number);
if (area[0] + area[2] > width || area[1] + area[3] > height) throw new Error(`The recording is ${width}x${height}: ${areaWord} isn't inside it`);

// Every frame screenrecord wrote, cut to the area, with when it was shown (showinfo's pts_time).
const ffmpeg = spawnSync(
  "ffmpeg",
  ["-v", "info", "-hide_banner", "-t", String((LEAD_MS + forMs) / 1000), "-i", video,
    "-vf", `crop=${area[2]}:${area[3]}:${area[0]}:${area[1]},showinfo`, "-fps_mode", "passthrough", "-enc_time_base", "1/90000", "-q:v", "2", join(dir, "%05d.jpg")],
  { encoding: "utf8", stdio: ["ignore", "ignore", "pipe"], maxBuffer: 64 * 1024 * 1024 },
);
if (ffmpeg.status !== 0) throw new Error(`FFmpeg failed:\n${(ffmpeg.stderr ?? String(ffmpeg.error)).split("\n").filter((l) => !l.includes("showinfo")).slice(-10).join("\n")}`);
const times = [...ffmpeg.stderr.matchAll(/n:\s*(\d+)\s+pts:\s*\S+\s+pts_time:([\d.]+)/g)].map((m) => Number(m[2]));
const files = readdirSync(dir).filter((f) => /^\d{5}\.jpg$/.test(f)).sort();
if (files.length === 0 || files.length !== times.length) throw new Error(`FFmpeg wrote ${files.length} frames and showed ${times.length}`);
files.forEach((file, n) => {
  const ms = Math.round(((times[n] - times[0]) * 1000) / slow);
  renameSync(join(dir, file), join(dir, `${String(n).padStart(3, "0")}-${ms}.jpg`));
});
rmSync(video);
console.log(`${files.length} frames of ${areaWord} in a ${width}x${height} recording, in ${dir}`);

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function usage(said) {
  if (said) console.error(said);
  console.error("bun scripts/record-android.mjs <slower> <ms> <x,y,w,h> [keys: Up Down Left Right OK Back Menu]... [--serial <device>] [--out <folder>]");
  process.exit(2);
}

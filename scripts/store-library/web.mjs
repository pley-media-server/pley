// Takes the README's pictures of the web app (docs/images): five screenshots and a tour
// as a GIF, through a headless Chrome, from a server holding the store library.
// Usage: bun web.mjs <dir> <address>, with <dir> as setup.mjs left it (admin.json).
import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chrome } from "../svg-render.mjs";

const [dir, base] = process.argv.slice(2);
if (!dir || !base) throw new Error("usage: bun web.mjs <dir> <address>");
const out = join(import.meta.dir, "../../docs/images");
const width = 1440;
const height = 900;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

const profile = mkdtempSync(join(tmpdir(), "pley-web-"));
const browser = spawn(
  chrome(),
  ["--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "--no-first-run", "--hide-scrollbars", "--force-color-profile=srgb", "--autoplay-policy=no-user-gesture-required", "--mute-audio", "about:blank"],
  { stdio: ["ignore", "ignore", "pipe"] },
);
const url = await new Promise((resolve, reject) => {
  let said = "";
  browser.stderr.on("data", (d) => {
    said += d;
    const m = said.match(/DevTools listening on (ws:\/\/\S+)/);
    if (m) resolve(m[1]);
  });
  browser.on("exit", (code) => reject(new Error(`the browser stopped (${code}): ${said}`)));
});
const socket = new WebSocket(url);
await new Promise((resolve) => socket.addEventListener("open", resolve, { once: true }));
let next = 0;
const waiting = new Map();
const events = [];
socket.addEventListener("message", (e) => {
  const msg = JSON.parse(e.data);
  if (msg.id === undefined) return events.forEach((on) => on(msg));
  const call = waiting.get(msg.id);
  waiting.delete(msg.id);
  if (msg.error) call?.reject(new Error(msg.error.message));
  else call?.resolve(msg.result);
});
let sessionId;
const send = (method, params = {}) =>
  new Promise((resolve, reject) => {
    const id = ++next;
    waiting.set(id, { resolve, reject });
    socket.send(JSON.stringify({ id, method, params, sessionId }));
  });
const { targetId } = await send("Target.createTarget", { url: "about:blank" });
({ sessionId } = await send("Target.attachToTarget", { targetId, flatten: true }));
await send("Page.enable");
await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });

async function evaluate(expression) {
  const res = await send("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (res.exceptionDetails) throw new Error(res.exceptionDetails.exception?.description ?? res.exceptionDetails.text);
  return res.result.value;
}
async function goto(path) {
  const loaded = new Promise((resolve) => {
    const on = (msg) => {
      if (msg.method !== "Page.loadEventFired") return;
      events.splice(events.indexOf(on), 1);
      resolve();
    };
    events.push(on);
  });
  await send("Page.navigate", { url: base + path });
  await loaded;
}
/** Until every picture has loaded, then [ms] more. */
async function settle(ms) {
  for (let i = 0; i < 50 && (await evaluate("[...document.images].some((i) => !i.complete)")); i++) await sleep(100);
  await sleep(ms);
}
const shot = async (quality) => Buffer.from((await send("Page.captureScreenshot", { format: "jpeg", quality })).data, "base64");

// The mouse, moved as a hand would, so hover states show in the tour.
let mouse = { x: width / 2, y: height / 2 };
async function moveTo(x, y, steps = 14) {
  const from = mouse;
  for (let i = 1; i <= steps; i++) {
    const t = i / steps;
    const eased = t < 0.5 ? 2 * t * t : 1 - (-2 * t + 2) ** 2 / 2;
    mouse = { x: from.x + (x - from.x) * eased, y: from.y + (y - from.y) * eased };
    await send("Input.dispatchMouseEvent", { type: "mouseMoved", ...mouse });
    await sleep(22);
  }
}
/** Clicks the first visible [selector] whose text has [text]. */
async function click(selector, text) {
  for (let i = 0; i < 40; i++) {
    const at = await evaluate(`(() => {
      const el = [...document.querySelectorAll(${JSON.stringify(selector)})]
        .find((e) => e.textContent.includes(${JSON.stringify(text)}) && e.getBoundingClientRect().width > 0);
      if (!el) return null;
      const r = el.getBoundingClientRect();
      return { x: Math.round(r.x + r.width / 2), y: Math.round(r.y + Math.min(r.height / 2, 120)) };
    })()`);
    if (at) {
      await moveTo(at.x, at.y);
      await sleep(250);
      for (const type of ["mousePressed", "mouseReleased"]) await send("Input.dispatchMouseEvent", { type, ...at, button: "left", clickCount: 1 });
      return;
    }
    await sleep(100);
  }
  throw new Error(`nothing to click: ${selector} with "${text}"`);
}
const still = async (name) => writeFileSync(join(out, `${name}.jpg`), await shot(80));
const library = "aside a[href^='/library/'], nav a[href^='/library/']";

// Frames as fast as the browser gives them, each with when it was taken.
const frames = [];
let recording = false;
async function record() {
  while (recording) frames.push({ t: performance.now(), jpeg: await shot(92) });
}

try {
  mkdirSync(out, { recursive: true });
  const { username, password } = JSON.parse(readFileSync(join(dir, "admin.json"), "utf8"));
  await goto("/login");
  const signedIn = await evaluate(
    `fetch("/api/auth/login", { method: "POST", headers: { "content-type": "application/json" }, body: ${JSON.stringify(JSON.stringify({ username, password }))} }).then((r) => r.ok)`,
  );
  if (!signedIn) throw new Error("couldn't sign in with admin.json");
  // The notice asking for a TMDB key, which the made-up library goes without.
  await goto("/");
  await settle(500);
  await click(library, "Movies");
  await settle(500);
  await evaluate(`[...document.querySelectorAll("button")].find((b) => /dismiss|close/i.test(b.getAttribute("aria-label") ?? ""))?.click()`);
  await goto("/");
  await settle(1500);
  await still("web-home");

  recording = true;
  const recorder = record();
  await sleep(1600);
  await click("a[href^='/item/']", "Paper Moons");
  await settle(1800);
  await still("web-movie");
  await click("button", "Resume");
  await sleep(2600);
  await moveTo(700, 600, 8); // shows the player's controls
  await sleep(600);
  await still("web-player");
  await evaluate("history.back()");
  await settle(200);
  await click(library, "Movies");
  await settle(1600);
  await still("web-library");
  await click(library, "TV Shows");
  await settle(900);
  await click("a[href^='/item/']", "Harbor Town");
  await settle(1000);
  await evaluate("window.scrollBy({ top: 440, behavior: 'smooth' }); document.querySelector('main')?.scrollBy({ top: 440, behavior: 'smooth' })");
  await sleep(1100);
  await click("a[href^='/item/']", "Season 1");
  await settle(2200);
  await still("web-season");
  recording = false;
  await recorder;

  // The frames, each shown until the next was taken, as a 960-pixel GIF at 12 frames a second.
  const work = mkdtempSync(join(tmpdir(), "pley-tour-"));
  const list = [];
  frames.forEach((f, i) => {
    writeFileSync(join(work, `${i}.jpg`), f.jpeg);
    list.push(`file '${i}.jpg'`, `duration ${(((frames[i + 1]?.t ?? f.t + 1500) - f.t) / 1000).toFixed(3)}`);
  });
  list.push(`file '${frames.length - 1}.jpg'`);
  writeFileSync(join(work, "list.txt"), list.join("\n") + "\n");
  const graph = "fps=12,scale=960:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=192:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle";
  const ff = spawnSync("ffmpeg", ["-hide_banner", "-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i", join(work, "list.txt"), "-vf", graph, "-loop", "0", join(out, "tour.gif")], { stdio: "inherit" });
  rmSync(work, { recursive: true, force: true });
  if (ff.status !== 0) throw new Error("ffmpeg couldn't make the GIF");
  console.log(`${frames.length} frames; the pictures are in ${out}`);
} finally {
  socket.close();
  browser.kill();
  setTimeout(() => rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 }), 500);
}

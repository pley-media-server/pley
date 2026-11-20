// Takes the README's tour of the web app (docs/images/web-tour.webp) through a headless
// Chrome, from a server holding the store library. It needs ffmpeg on PATH.
// Usage: bun web.mjs <dir> <address>, with <dir> as setup.mjs left it (admin.json).
import { spawn, spawnSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { chrome } from "../svg-render.mjs";

const [dir, base] = process.argv.slice(2);
if (!dir || !base) throw new Error("usage: bun web.mjs <dir> <address>");
const out = join(import.meta.dir, "../../docs/images/web-tour.webp");
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
await send("Emulation.setDeviceMetricsOverride", { width: 1440, height: 900, deviceScaleFactor: 1, mobile: false });

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
const hover = (x, y) => send("Input.dispatchMouseEvent", { type: "mouseMoved", x, y });
/** Clicks the first visible [selector] whose text has [text]. */
async function click(selector, text) {
  for (let i = 0; i < 40; i++) {
    const at = await evaluate(`(() => {
      const el = [...document.querySelectorAll(${JSON.stringify(selector)})]
        .find((e) => e.textContent.includes(${JSON.stringify(text)}) && e.getBoundingClientRect().width > 0);
      if (!el) return null;
      el.scrollIntoView({ block: "nearest", behavior: "instant" });
      const r = el.getBoundingClientRect();
      return { x: Math.round(r.x + r.width / 2), y: Math.round(r.y + Math.min(r.height / 2, 120)) };
    })()`);
    if (at) {
      await hover(at.x, at.y);
      for (const type of ["mousePressed", "mouseReleased"]) await send("Input.dispatchMouseEvent", { type, ...at, button: "left", clickCount: 1 });
      return;
    }
    await sleep(100);
  }
  throw new Error(`nothing to click: ${selector} with "${text}"`);
}
const screens = [];
/** Waits for every picture to load and then [ms] more, and keeps the screen. */
async function shoot(ms) {
  for (let i = 0; i < 50 && (await evaluate("[...document.images].some((i) => !i.complete)")); i++) await sleep(100);
  await sleep(ms);
  screens.push(Buffer.from((await send("Page.captureScreenshot", { format: "png" })).data, "base64"));
}
const library = "aside a[href^='/library/'], nav a[href^='/library/']";

try {
  const { username, password } = JSON.parse(readFileSync(join(dir, "admin.json"), "utf8"));
  await goto("/login");
  const signedIn = await evaluate(
    `fetch("/api/auth/login", { method: "POST", headers: { "content-type": "application/json" }, body: ${JSON.stringify(JSON.stringify({ username, password }))} }).then((r) => r.ok)`,
  );
  if (!signedIn) throw new Error("couldn't sign in with admin.json");
  // Hides the notice asking for a TMDB key, which the made-up library goes without.
  await evaluate(`localStorage.setItem("pley.notice.tmdbKey", "true")`);

  await goto("/");
  await shoot(4000); // the first visit, while the server makes Home's pictures
  await click(library, "Movies");
  await shoot(1500);
  await click("a[href^='/item/']", "Paper Moons");
  await shoot(1500);
  await click("button", "Resume");
  await sleep(5000);
  await hover(700, 600); // shows the player's controls
  await shoot(700);
  await evaluate("history.back()");
  await click(library, "TV Shows");
  await click("a[href^='/item/']", "Harbor Town");
  await click("a[href^='/item/']", "Season 1");
  await hover(1100, 150); // off the episode the click left it on
  await shoot(1500);

  // Three seconds a screen. Cuts rather than fades, which would make the file several times larger.
  const ffmpeg = spawnSync(
    "ffmpeg",
    ["-hide_banner", "-loglevel", "error", "-y", "-f", "image2pipe", "-framerate", "1/3", "-i", "-", "-c:v", "libwebp_anim", "-quality", "80", "-loop", "0", out],
    { input: Buffer.concat(screens), stdio: ["pipe", "inherit", "inherit"] },
  );
  if (ffmpeg.status !== 0) throw new Error("ffmpeg couldn't make the tour");
  console.log(`the tour is in ${out}`);
} finally {
  socket.close();
  browser.kill();
  setTimeout(() => rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 }), 500);
}

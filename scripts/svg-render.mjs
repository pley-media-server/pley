// Draws SVG into PNG and JPEG pictures through a headless Chrome or Edge, for
// the scripts that render what the apps ship (render-icons.mjs,
// render-kodi-skin.mjs) and mark-screenshot.mjs. Chrome comes from $CHROME,
// else the usual place on Windows, macOS or Linux.

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

export function chrome() {
  const candidates = [
    process.env.CHROME,
    "C:/Program Files/Google/Chrome/Application/chrome.exe",
    "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
  ];
  const found = candidates.find((p) => p && existsSync(p));
  if (!found) throw new Error("No Chrome or Edge found: set CHROME to one.");
  return found;
}

/** A headless browser page that turns an SVG into a PNG of exactly width × height, transparent around it. */
export async function renderer() {
  const profile = mkdtempSync(join(tmpdir(), "pley-icons-"));
  const browser = spawn(
    chrome(),
    ["--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "--no-first-run", "--hide-scrollbars", "--force-color-profile=srgb", "about:blank"],
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
  socket.addEventListener("message", (e) => {
    const msg = JSON.parse(e.data);
    const call = waiting.get(msg.id);
    if (!call) return;
    waiting.delete(msg.id);
    if (msg.error) call.reject(new Error(msg.error.message));
    else call.resolve(msg.result);
  });
  const send = (method, params = {}, sessionId) =>
    new Promise((resolve, reject) => {
      const id = ++next;
      waiting.set(id, { resolve, reject });
      socket.send(JSON.stringify({ id, method, params, sessionId }));
    });

  const { targetId } = await send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
  await send("Emulation.setDefaultBackgroundColorOverride", { color: { r: 0, g: 0, b: 0, a: 0 } }, sessionId);
  const { frameTree } = await send("Page.getFrameTree", {}, sessionId);

  /** [svg] drawn width × height, as a PNG, or a JPEG of [quality] (0 to 100). */
  async function capture(svg, width, height, quality) {
    await send("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false }, sessionId);
    const sized = svg.replace("<svg", `<svg width="${width}" height="${height}" style="display:block"`);
    await send("Page.setDocumentContent", { frameId: frameTree.frame.id, html: `<!doctype html><body style="margin:0">${sized}</body>` }, sessionId);
    const format = quality === undefined ? { format: "png" } : { format: "jpeg", quality };
    const { data } = await send("Page.captureScreenshot", { ...format, clip: { x: 0, y: 0, width, height, scale: 1 } }, sessionId);
    return Buffer.from(data, "base64");
  }

  return {
    png: (svg, width, height = width) => capture(svg, width, height),
    jpeg: (svg, width, height, quality) => capture(svg, width, height, quality),
    close() {
      socket.close();
      browser.kill();
      // The browser lets go of its profile a moment after it stops; one it still holds stays
      // in the temporary folder rather than failing the run.
      setTimeout(() => {
        try {
          rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 200 });
        } catch {}
      }, 500);
    },
  };
}

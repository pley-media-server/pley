// Receives a profiling build's marks (PLEY_TIZEN_METRICS, scripts/package.mjs)
// from the TV and appends them to out/metrics.log, each with the time it came.
// Runs under Node or Bun; the TV reaches it on port 7531 (PLEY_METRICS_PORT),
// which the computer's firewall has to let in.
//
// It also drives the app: run again with a command while it listens, and the
// next TV to ask (every second) is given it.
//   metrics.mjs shot                  a screenshot, saved in out/, its path printed
//   metrics.mjs keys Down Down OK     the remote's keys: Up, Down, Left, Right, OK, Back, Menu
//   --tv S89C                         only the TV whose model name holds this

import { appendFileSync, mkdirSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import { networkInterfaces } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const out = join(dirname(fileURLToPath(import.meta.url)), "..", "out");
const log = join(out, "metrics.log");
const port = Number(process.env.PLEY_METRICS_PORT ?? 7531);
// A mark is a line; anything longer is cut.
const most = 4000;
// A screenshot is a PNG of the whole screen.
const mostShot = 16 * 1024 * 1024;
// How long a command waits for a TV to take it (and a screenshot to come).
const waitMs = 30_000;
const stamp = () => new Date().toISOString().slice(11, 23);

const args = process.argv.slice(2);
const at = args.indexOf("--tv");
const tv = at >= 0 ? (args.splice(at, 2)[1] ?? "") : "";
if (args.length > 0) await send(args.join(" "));
else listen();

/** Hands a command to the listener and prints what came of it. */
async function send(command) {
  try {
    const res = await fetch(`http://127.0.0.1:${port}/ask?tv=${encodeURIComponent(tv)}`, { method: "POST", body: command });
    console.log(await res.text());
    if (!res.ok) process.exitCode = 1;
  } catch {
    console.error(`No listener on port ${port}: start this script without a command first.`);
    process.exitCode = 1;
  }
}

function listen() {
  // Commands waiting for a TV, and screenshots asked for waiting for their picture.
  const waiting = [];
  const shots = [];
  const matches = (wanted, model) => !wanted || model.includes(wanted);
  const settle = (list, entry, status, text) => {
    const i = list.indexOf(entry);
    if (i < 0) return;
    list.splice(i, 1);
    clearTimeout(entry.timer);
    entry.res.writeHead(status).end(text);
  };

  mkdirSync(out, { recursive: true });
  createServer((req, res) => {
    const url = new URL(req.url ?? "/", "http://tv");
    const model = (url.searchParams.get("tv") ?? "").replace(/[^A-Za-z0-9_-]/g, "");
    let size = 0;
    const chunks = [];
    req.on("data", (chunk) => {
      size += chunk.length;
      if (size <= mostShot) chunks.push(chunk);
    });
    req.on("end", () => {
      const body = Buffer.concat(chunks);
      if (req.method === "POST" && url.pathname === "/ask") {
        // Only from this computer: a command is for whoever runs the TV's debug build.
        if (!["127.0.0.1", "::1", "::ffff:127.0.0.1"].includes(req.socket.remoteAddress ?? "")) return res.writeHead(403).end();
        const entry = { tv: url.searchParams.get("tv") ?? "", command: body.toString("utf8").trim().slice(0, most), res };
        entry.timer = setTimeout(() => settle(waiting, entry, 504, "No TV asked for it: is a debug build open?"), waitMs);
        waiting.push(entry);
        return;
      }
      if (req.method === "GET" && url.pathname === "/command") {
        const entry = waiting.find((w) => matches(w.tv, model));
        if (!entry) return res.end("");
        res.end(entry.command);
        appendFileSync(log, `${stamp()} COMMAND ${entry.command} taken by ${model}\n`);
        if (entry.command !== "shot") return settle(waiting, entry, 200, `${model} took it`);
        // A screenshot's asker hears once the picture is in.
        waiting.splice(waiting.indexOf(entry), 1);
        clearTimeout(entry.timer);
        entry.model = model;
        entry.timer = setTimeout(() => settle(shots, entry, 504, `${model} sent no picture`), waitMs);
        shots.push(entry);
        return;
      }
      if (req.method === "POST" && url.pathname === "/shot") {
        const file = join(out, `shot-${new Date().toISOString().slice(11, 19).replace(/:/g, "")}-${model}.png`);
        if (size <= mostShot) writeFileSync(file, body);
        appendFileSync(log, `${stamp()} SHOT ${file} (${size} bytes)\n`);
        const entry = shots.find((s) => s.model === model);
        if (entry) settle(shots, entry, size <= mostShot ? 200 : 413, size <= mostShot ? file : "The picture was too big");
        return res.end("ok");
      }
      if (req.method === "POST") appendFileSync(log, `${stamp()} ${body.toString("utf8").slice(0, most)}\n`);
      res.end("ok");
    });
  }).listen(port, "0.0.0.0", () => {
    const addresses = Object.values(networkInterfaces())
      .flat()
      .filter((a) => a && a.family === "IPv4" && !a.internal)
      .map((a) => `http://${a.address}:${port}`);
    console.log(`Writing ${log}`);
    console.log(`PLEY_TIZEN_METRICS can be ${addresses.join(" or ")}`);
  });
}

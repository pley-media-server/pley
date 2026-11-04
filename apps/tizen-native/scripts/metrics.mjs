// Receives a profiling build's marks (PLEY_TIZEN_METRICS, scripts/package.mjs)
// from the TV and appends them to out/metrics.log, each with the time it came.
// Runs under Node or Bun; the TV reaches it on port 7531 (PLEY_METRICS_PORT),
// which the computer's firewall has to let in.
//
// It also drives the app: run again with a command while it listens, and the
// TV asking (the listener holds its ask until a command comes) is given it.
//   metrics.mjs shot                  a screenshot, saved in out/, its path printed
//   metrics.mjs keys Down Down OK     the remote's keys: Up, Down, Left, Right, OK, Back, Menu
//   metrics.mjs record 20 6000 0,0,960,1080 Right
//                                     the keys pressed with the animations that can be slowed
//                                     20 times slower, and that area of the screen caught frame
//                                     after frame for 6 s, into out/rec-<time>-<model>/ as
//                                     NNN-<ms>.jpg, ms at the animation's own speed
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
// How long a TV's ask is held: under the 30 s the app waits, so it asks again then.
const holdMs = 25_000;
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
  // Commands waiting for a TV; screenshots and recordings asked for, waiting for their pictures;
  // TVs' asks for a command, held until one comes.
  const waiting = [];
  const shots = [];
  const recordings = [];
  const held = [];
  const matches = (wanted, model) => !wanted || model.includes(wanted);
  const settle = (list, entry, status, text) => {
    const i = list.indexOf(entry);
    if (i < 0) return;
    list.splice(i, 1);
    clearTimeout(entry.timer);
    entry.res.writeHead(status).end(text);
  };
  /** Gives a TV its command: its asker hears once the TV takes it, or once what it asked for is in. */
  const hand = (entry, model, res) => {
    const recording = entry.command.startsWith("record ");
    // One recording a TV at a time: its frames and its end don't say which recording they're for,
    // so a second would take the first's frames, and the first the second's refusal.
    if (recording && recordings.some((e) => e.model === model)) {
      res.end("");
      return settle(waiting, entry, 409, `${model} is recording already`);
    }
    res.end(entry.command);
    appendFileSync(log, `${stamp()} COMMAND ${entry.command} taken by ${model}\n`);
    if (entry.command !== "shot" && !recording) return settle(waiting, entry, 200, `${model} took it`);
    waiting.splice(waiting.indexOf(entry), 1);
    clearTimeout(entry.timer);
    entry.model = model;
    if (recording) {
      // The frames come once they are all caught, which takes as long as they do. Their folder is
      // made with the first: a recording the TV refuses leaves none.
      entry.dir = join(out, `rec-${new Date().toISOString().slice(11, 19).replace(/:/g, "")}-${model}`);
      entry.timer = setTimeout(() => settle(recordings, entry, 504, `${model} sent no frames`), 5 * 60_000);
      recordings.push(entry);
      return;
    }
    entry.timer = setTimeout(() => settle(shots, entry, 504, `${model} sent no picture`), waitMs);
    shots.push(entry);
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
        // A TV already asking has it at once.
        const ask = held.find((h) => matches(entry.tv, h.model));
        if (ask) {
          held.splice(held.indexOf(ask), 1);
          clearTimeout(ask.timer);
          hand(entry, ask.model, ask.res);
        }
        return;
      }
      if (req.method === "GET" && url.pathname === "/command") {
        const entry = waiting.find((w) => matches(w.tv, model));
        if (entry) return hand(entry, model, res);
        if (!url.searchParams.has("hold")) return res.end("");
        const ask = { model, res };
        const drop = () => {
          const i = held.indexOf(ask);
          if (i < 0) return false;
          held.splice(i, 1);
          clearTimeout(ask.timer);
          return true;
        };
        ask.timer = setTimeout(() => drop() && res.end(""), holdMs);
        // An ask whose TV has gone (the app closed, the TV asleep) takes no command: it would be lost.
        res.on("close", drop);
        held.push(ask);
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
      if (req.method === "POST" && url.pathname === "/frame") {
        // A recording's frame: its number, and when it was caught at the animation's own speed.
        const entry = recordings.find((e) => e.model === model);
        const n = String(Math.trunc(Number(url.searchParams.get("n")) || 0)).padStart(3, "0");
        const t = Math.round(Number(url.searchParams.get("t")) || 0);
        if (entry && size <= mostShot) {
          mkdirSync(entry.dir, { recursive: true });
          writeFileSync(join(entry.dir, `${n}-${t}.jpg`), body);
        }
        return res.end("ok");
      }
      if (req.method === "POST" && url.pathname === "/recorded") {
        const n = Math.trunc(Number(url.searchParams.get("n")) || 0);
        // Why the TV recorded nothing, where it couldn't: its words, a recording already running.
        const refused = url.searchParams.get("refused")?.slice(0, most);
        appendFileSync(log, `${stamp()} RECORDED ${n} frames from ${model}${refused ? `, refused: ${refused}` : ""}\n`);
        const entry = recordings.find((e) => e.model === model);
        const said = refused ? `${model} refused it: ${refused}` : n > 0 ? `${n} frames in ${entry?.dir}` : `${model} caught no frames`;
        if (entry) settle(recordings, entry, refused ? 400 : 200, said);
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

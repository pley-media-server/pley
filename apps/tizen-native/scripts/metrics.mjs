// Receives a profiling build's marks (PLEY_TIZEN_METRICS, scripts/package.mjs)
// from the TV and appends them to out/metrics.log, each with the time it came.
// Runs under Node or Bun; the TV reaches it on port 7531 (PLEY_METRICS_PORT),
// which the computer's firewall has to let in.

import { appendFileSync, mkdirSync } from "node:fs";
import { createServer } from "node:http";
import { networkInterfaces } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const out = join(dirname(fileURLToPath(import.meta.url)), "..", "out");
const log = join(out, "metrics.log");
const port = Number(process.env.PLEY_METRICS_PORT ?? 7531);
// A mark is a line; anything longer is cut.
const most = 4000;

mkdirSync(out, { recursive: true });
createServer((req, res) => {
  let body = "";
  req.on("data", (chunk) => {
    if (body.length < most) body += chunk;
  });
  req.on("end", () => {
    if (req.method === "POST") appendFileSync(log, `${new Date().toISOString().slice(11, 23)} ${body.slice(0, most)}\n`);
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

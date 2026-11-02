// Development only: serves dist/ in a desktop browser and passes every
// other request to a pley server (PLEY_SERVER, default
// http://127.0.0.1:7640), so the app and the server share an origin.
// Sign in by typing this page's own address (localhost:7650) in the field;
// approve the code at http://localhost:7650/link in another tab.

import { createReadStream, existsSync, statSync } from "node:fs";
import { request as httpRequest, createServer } from "node:http";
import { extname, join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(new URL("..", import.meta.url)), "dist");
const target = new URL(process.env.PLEY_SERVER ?? "http://127.0.0.1:7640");
const port = Number(process.env.PORT ?? 7650);
const TYPES = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".woff2": "font/woff2",
  ".png": "image/png",
  ".json": "application/json",
};

createServer((req, res) => {
  const path = decodeURIComponent((req.url ?? "/").split("?")[0]);
  const file = normalize(join(root, path === "/" ? "/index.html" : path));
  if (file.startsWith(root) && existsSync(file) && statSync(file).isFile() && req.method === "GET") {
    res.writeHead(200, { "Content-Type": TYPES[extname(file)] ?? "application/octet-stream", "Cache-Control": "no-store" });
    createReadStream(file).pipe(res);
    return;
  }
  const upstream = httpRequest(
    { host: target.hostname, port: target.port, method: req.method, path: req.url, headers: req.headers },
    (answer) => {
      res.writeHead(answer.statusCode ?? 502, answer.headers);
      answer.pipe(res);
    },
  );
  upstream.on("error", (e) => {
    // A stream the server dropped half way (the event stream, when it stops) just ends.
    if (res.headersSent) {
      res.destroy();
      return;
    }
    res.writeHead(502, { "Content-Type": "text/plain" });
    res.end(String(e));
  });
  // The page went (a reload, a closed event stream): so does its request.
  res.on("close", () => upstream.destroy());
  req.pipe(upstream);
}).listen(port, () => console.log(`Pley for LG TV (development): http://localhost:${port} → ${target.origin}`));

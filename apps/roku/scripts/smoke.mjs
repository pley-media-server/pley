// Runs the whole app, SceneGraph and Tasks included, in brs-node against
// a fake pley server on 127.0.0.1 (scripts/fixture-server.mjs), and walks
// it with the remote: sign in with the TV code and its QR code on a server
// that requires two-step sign-in, Home, a title's details, the player and
// subtitles picked in its menu, the Options key, a library's grid, its A
// to Z and the pages a jump passes over, a sort by bitrate and a filter
// by resolution with their quality badges, filters saved as a smart
// playlist, collections played (movies and a smart one of shows), a
// show's page, its episodes and a person's
// page, music (an artist, an album, Now Playing), playlists, Live TV (its
// guide, a channel watched, the next one, the channel list, a favourite
// and another category listed there, every connection in use, the
// Channels tab), Search (and its Discover row's
// See all), Discover (a movie asked for, a show's seasons picked), the
// managers' Requests (Approve, then Yours), a ready-to-watch notice on
// Home whose movie's audio and subtitles are picked on its page, Settings
// (its Language chosen for the account, Home asked for again in it), the
// server gone and Try again, then signing out and in again on a server
// too old for QR codes, requests and two-step sign-in. Then it starts
// the app again, signed in,
// and times how soon Home is asked for. Fails on a BrightScript error or
// on anything the fake server found wrong; saves what the screen showed
// at each step in out/smoke/. brs-node is a simulator: layouts and the
// player differ from a Roku, and so does its speed.
//
//   node scripts/smoke.mjs

import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { startFixture } from "./fixture-server.mjs";

const require = createRequire(import.meta.url);
const brs = require("brs-node");
const { createCanvas } = createRequire(require.resolve("brs-node"))("canvas");

const script = fileURLToPath(import.meta.url);
const root = join(dirname(script), "..");
const shots = join(root, "out", "smoke");
mkdirSync(shots, { recursive: true });

const messages = [];
let frame = null;
let registry = null;
let mainAt = 0;
brs.subscribeHost("smoke", (event, data) => {
  if (event === "message" && typeof data === "string") {
    messages.push(data);
    if (data.startsWith("start,") && mainAt === 0) mainAt = Date.now();
  } else if (event === "frame") frame = data;
  else if (event === "registry") registry = data.current;
});

// The remote: key codes as brs-node's CLI sends them, in its shared buffer.
const KEYS = { back: 0, up: 2, down: 3, left: 4, right: 5, select: 6, options: 10 };
const shared = new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT * (brs.DataBufferIndex + brs.DataBufferSize));
const control = new Int32Array(shared).fill(-1);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
// Waits for a free slot: the engine empties them as it reads the keys, which
// can wait while it loads pictures (one at a time, on its render thread).
async function send(code, mod) {
  const until = Date.now() + 30000;
  while (Date.now() < until) {
    for (let slot = 0; slot < brs.KeyArraySpots; slot++) {
      const at = slot * brs.KeyBufferSize;
      if (Atomics.load(control, brs.DataType.KEY + at) < 0) {
        Atomics.store(control, brs.DataType.RID + at, brs.RemoteType.SIM);
        Atomics.store(control, brs.DataType.MOD + at, mod);
        Atomics.store(control, brs.DataType.KEY + at, code + mod);
        return at;
      }
    }
    await sleep(20);
  }
  throw new Error(`the app read no key for 30 s`);
}
// The key's release goes only once the engine has read its press: read
// in the other order, a busy engine drops the key.
async function read(at) {
  const until = Date.now() + 30000;
  while (Date.now() < until && Atomics.load(control, brs.DataType.KEY + at) >= 0) await sleep(10);
}
async function press(key, times = 1) {
  for (let i = 0; i < times; i++) {
    const code = key.startsWith("lit_") ? key.codePointAt(4) : KEYS[key];
    await read(await send(code, 0));
    await sleep(80);
    await send(code, 100);
    await sleep(600);
  }
}
let step = 0;
function shoot(name) {
  step += 1;
  if (!frame) return;
  const canvas = createCanvas(frame.width, frame.height);
  canvas.getContext("2d").putImageData(frame, 0, 0);
  writeFileSync(join(shots, `${String(step).padStart(2, "0")}-${name}.png`), canvas.toBuffer("image/png"));
}

// Whether the sign-in screen shows a QR code: its quiet zone is white where
// the screen is dark (LinkScreen.xml centres the picture in a column from
// 1384 to 1824, from 432 down).
function qrDrawn() {
  if (!frame) return false;
  const x = Math.round((1384 + 220) * (frame.width / 1920));
  const y = Math.round((432 + 12) * (frame.height / 1080));
  const at = (y * frame.width + x) * 4;
  return frame.data[at] > 200 && frame.data[at + 1] > 200 && frame.data[at + 2] > 200;
}
// Whether the sign-in screen says the server requires two-step sign-in:
// muted text under the server's name (LinkScreen.xml, from 96, 306), where
// the walk's sign-ins show nothing else (no reason to sign in again).
function twoStepDrawn() {
  if (!frame) return false;
  for (let y = 310; y < 334; y += 2) {
    for (let x = 100; x < 700; x += 2) {
      const at = (Math.round(y * (frame.height / 1080)) * frame.width + Math.round(x * (frame.width / 1920))) * 4;
      if (frame.data[at] > 130 && frame.data[at + 1] > 130 && frame.data[at + 2] > 140) return true;
    }
  }
  return false;
}
// The subtitle sync panel's title, in the accent, at the top of the player
// (from 608, 112).
function syncDrawn() {
  if (!frame) return false;
  for (let y = 112; y < 144; y += 2) {
    for (let x = 608; x < 800; x += 2) {
      const at = (Math.round(y * (frame.height / 1080)) * frame.width + Math.round(x * (frame.width / 1920))) * 4;
      if (frame.data[at] < 120 && frame.data[at + 1] > 150 && frame.data[at + 2] > 200) return true;
    }
  }
  return false;
}

// A title's media info: lines of text far down its page's panel (from 776,
// 150), where More's few entries never reach.
function infoDrawn() {
  if (!frame) return false;
  for (let y = 760; y < 1000; y += 2) {
    for (let x = 780; x < 1400; x += 2) {
      const at = (Math.round(y * (frame.height / 1080)) * frame.width + Math.round(x * (frame.width / 1920))) * 4;
      if (frame.data[at] > 200 && frame.data[at + 1] > 200 && frame.data[at + 2] > 200) return true;
    }
  }
  return false;
}

// The live player's notice: its dark panel over the whole screen, its
// title in the app's text colour at the left (from 96, 120 when it lists what
// holds each connection).
function noticeDrawn() {
  if (!frame) return false;
  for (let y = 130; y < 180; y += 2) {
    for (let x = 100; x < 600; x += 2) {
      const at = (Math.round(y * (frame.height / 1080)) * frame.width + Math.round(x * (frame.width / 1920))) * 4;
      if (frame.data[at] > 200 && frame.data[at + 1] > 200 && frame.data[at + 2] > 200) return true;
    }
  }
  return false;
}

// The ready-to-watch card's Open, focused (in the accent), at the bottom
// right (MainScene.xml's card at 1104, 702; RequestNotice.xml's buttons
// from 308, 180 in it).
function readyNoticeDrawn() {
  if (!frame) return false;
  for (let y = 896; y < 930; y += 4) {
    for (let x = 1424; x < 1580; x += 4) {
      const at = (Math.round(y * (frame.height / 1080)) * frame.width + Math.round(x * (frame.width / 1920))) * 4;
      if (frame.data[at] < 120 && frame.data[at + 1] > 150 && frame.data[at + 2] > 200) return true;
    }
  }
  return false;
}

// A Settings text page's text (TextScreen.xml, from 96, 180), in the app's
// text colour.
function pageTextDrawn() {
  if (!frame) return false;
  for (let y = 186; y < 230; y += 2) {
    for (let x = 100; x < 600; x += 2) {
      const at = (Math.round(y * (frame.height / 1080)) * frame.width + Math.round(x * (frame.width / 1920))) * 4;
      if (frame.data[at] > 200 && frame.data[at + 1] > 200 && frame.data[at + 2] > 200) return true;
    }
  }
  return false;
}

// Whether Now Playing's sleep timer button, focused (in the accent), says
// more than "Sleep timer": it is the last of the buttons, which start at
// 96, 560 (NowPlayingScreen.brs: the five before it take 1100), and only
// a longer label reaches past 1500.
function sleepButtonLong() {
  if (!frame) return false;
  for (let y = 566; y < 610; y += 4) {
    for (let x = 1500; x < 1580; x += 4) {
      const at = (Math.round(y * (frame.height / 1080)) * frame.width + Math.round(x * (frame.width / 1920))) * 4;
      if (frame.data[at] < 120 && frame.data[at + 1] > 150 && frame.data[at + 2] > 200) return true;
    }
  }
  return false;
}

// The A to Z's mark on a letter (LibraryScreen.xml's column from 1812, 212,
// 30 pixels a letter), left of the letter: the accent while the letters have
// the focus, else the lighter ground behind the letter of the title the
// grid's focus is on.
function letterMarked(index, focused) {
  if (!frame) return false;
  const x = Math.round(1815 * (frame.width / 1920));
  const y = Math.round((212 + index * 30 + 15) * (frame.height / 1080));
  const at = (y * frame.width + x) * 4;
  const [r, g, b] = [frame.data[at], frame.data[at + 1], frame.data[at + 2]];
  if (focused) return r < 120 && g > 150 && b > 200;
  return Math.abs(r - 0x2e) < 8 && Math.abs(g - 0x34) < 8 && Math.abs(b - 0x40) < 8;
}

async function drawnWithin(ms, test) {
  const until = Date.now() + ms;
  while (Date.now() < until) {
    if (test()) return true;
    await sleep(100);
  }
  return test();
}

function launch(saved) {
  return brs.createPayloadFromFiles([], { developerId: "smoke", displayMode: "1080p", registry: saved }, undefined, root).then((payload) => {
    payload.extensions = [brs.SupportedExtension.SceneGraph];
    payload.device.extensions = new Map([[brs.SupportedExtension.SceneGraph, "brs-sg.node.js"]]);
    return brs.executeApp(payload, { sharedBuffer: shared });
  });
}

// The second start runs in a process of its own: brs-node runs one app per
// process. It runs until the first process has seen Home's pictures asked
// for (it says so on stdin), and prints its BrightScript errors for the
// first to check.
const restarting = process.argv.indexOf("--restart");
if (restarting > 0) {
  const [file, number] = process.argv.slice(restarting + 1);
  const running = launch(new Map(JSON.parse(readFileSync(file, "utf8"))));
  await new Promise((resolve) => {
    process.stdin.once("data", resolve);
    process.stdin.once("end", resolve);
    setTimeout(resolve, 90000);
  });
  step = Number(number) - 1;
  shoot("restarted");
  brs.terminateApp("smoke done", 3000);
  await running;
  console.log(`main,${mainAt}`);
  for (const m of messages.filter((m) => m.startsWith("error,"))) console.log(m);
  process.exit(0);
}

// Refreshed access tokens need a refresh two seconds after they're given
// (the app refreshes 5 minutes ahead), so the walk refreshes often. The
// first request for a title's details is refused (401 invalid_token): the
// app refreshes and asks again.
const fixture = await startFixture({ accessSeconds: 302, refuse: ["/api/items/103"] });
// The first sign-in is on a server that requires two-step sign-in.
fixture.twoStep = true;

// Saved and chosen: the app opens at sign-in (no search of the network).
const server = fixture.issuer;
const running = launch(new Map([
  ["smoke.servers.list", JSON.stringify([{ url: server, name: "Fixture", id: "fixture", last_used: 1 }])],
  ["smoke.servers.current", server],
]));

const mark = () => fixture.requests.length;
const seen = (pattern, since) => fixture.requests.slice(since).some((r) => pattern.test(`${r.method} ${r.path}`));

// Until the app has asked for nothing for a second (15 s at most): brs-node
// loads a screen's pictures one by one, and keys wait meanwhile.
async function settle() {
  const until = Date.now() + 15000;
  let count = mark();
  let quietSince = Date.now();
  while (Date.now() < until && Date.now() - quietSince < 1000) {
    await sleep(100);
    if (mark() !== count) {
      count = mark();
      quietSince = Date.now();
    }
  }
}

// Each step waits for the request it leads to (refreshes make answers
// come at their own pace), then for the screen to settle, unless it asks
// for something every second (the sign-in screen).
async function waitFor(pattern, since, what, { ms = 30000, settles = true } = {}) {
  const until = Date.now() + ms;
  while (Date.now() < until) {
    if (seen(pattern, since)) {
      if (settles) await settle();
      return;
    }
    await sleep(100);
  }
  throw new Error(`no request for ${what}`);
}

// brs-node drops a key now and then: menu entries are reached from the
// top of the menu (twelve entries at most, Settings the last, which 11
// reaches whatever the menu holds) and checked by the request they lead
// to.
async function openFromMenu(index, pattern, what, then = async () => {}) {
  for (let attempt = 0; attempt < 3; attempt++) {
    await press("left", 3);
    await press("up", 11);
    await press("down", index);
    const since = mark();
    await press("select");
    await then();
    try {
      await waitFor(pattern, since, what);
      return;
    } catch {
      await press("back");
    }
  }
  throw new Error(`couldn't open ${what} from the menu`);
}

// From Settings: Sign out, and Sign out again in the dialog. Sign out is
// the last of eleven buttons, Subtitle language showing once subtitles
// come on always.
async function signOut() {
  for (let attempt = 0; attempt < 3; attempt++) {
    const since = mark();
    await press("down", 10);
    await press("select");
    await sleep(800);
    await press("select");
    const until = Date.now() + 6000;
    while (Date.now() < until && !seen(/^POST \/api\/oauth\/revoke$/, since)) await sleep(100);
    if (seen(/^POST \/api\/oauth\/revoke$/, since)) return;
    await press("back"); // a dialog left open
  }
  throw new Error("couldn't sign out");
}

const qr = { shown: false, hidden: false };
// Home asked for in the language chosen in Settings.
const language = { home: false, refusedHome: false, afterRefused: "" };
// The line saying the server requires two-step sign-in.
const twoStep = { shown: false, hidden: false };
// The player's notice when every connection is in use.
const live = { busy: false };
const sync = { label: false };
// The picture subtitles the app draws: one of their PNGs loaded.
const pictures = { loaded: false };
// Subtitles picked in the player's own menu.
const menuTracks = { picked: false };
// Within ms, whether a picture subtitle's PNG was fetched.
async function loadedWithin(ms) {
  const until = Date.now() + ms;
  while (Date.now() < until) {
    if (fixture.picturesFetched.length > 0) return true;
    await sleep(100);
  }
  return false;
}
const pages = { diagnostics: false, licences: false };
// Home asks again only after playing, or when its rows are a minute old.
const home = { kept: false };
// From when the server knew nothing of requests (an older one).
const older = { since: -1 };
// The ready-to-watch notice: it waited while the requests showed, then
// showed on Home, where its Open led to the title.
const notice = { waited: false, shown: false, opened: false };
// What Play played on a collection's page: its first movie, a show's next
// episode.
const collections = { movies: false, shows: false };
// A title opened from a person's row of a role.
const person = { opened: false };
// An artist's Play: each album's tracks asked for, the first played.
const artistPlayed = { gathered: false, first: false };
// Now Playing: what OK on Up next played, Previous once shuffled and once
// in order again, and the sleep timer's button: short while off, long
// once set; then what OK on the album's second disc played.
const music = { upNext: null, shuffled: null, unshuffled: null, sleepOff: false, sleepSet: false, sleepEnd: false, sleepCycled: false, disc: null };

// The track OK plays: the next one the server is asked to play.
async function playedBy(what) {
  const since = mark();
  const before = fixture.playbacks.length;
  await press("select");
  await waitFor(/^POST \/api\/playback$/, since, what);
  return fixture.playbacks[before]?.item ?? null;
}
// The A to Z showed with W in focus, and the grid went to a W title.
const library = { letters: false, jumped: false };
// From when Home's See all was left.
const seeAll = { after: -1 };
// A recent search was searched again from the list.
const searched = { again: false };
// A title's media info filled its panel.
const mediaInfo = { shown: false };
// An episode's More went to its season.
const season = { opened: false };

async function walk() {
  // A cold start of brs-node can take a while.
  await waitFor(/^POST \/api\/oauth\/device$/, 0, "a TV code", { ms: 60000, settles: false });
  await waitFor(/^GET \/api\/oauth\/device\/qr\?code=BCDFGHJK$/, 0, "the code's QR code", { settles: false });
  qr.shown = await drawnWithin(2500, qrDrawn);
  twoStep.shown = seen(/^GET \/api\/server$/, 0) && (await drawnWithin(1000, twoStepDrawn));
  shoot("sign-in");
  await waitFor(/^GET \/api\/hubs\/home$/, 0, "Home");
  await settle();
  shoot("home");
  let since = mark();
  // The hero's Resume has the focus: Down to the rows.
  await press("down");
  await press("select"); // the first of Continue Watching: an episode
  await waitFor(/^GET \/api\/items\/103$/, since, "its details");
  shoot("details");
  // More…, after Resume, Play from start, the watched mark and Audio and
  // subtitles: Go to season, its episodes; Back to the episode's page, up
  // to Resume.
  await press("down", 4);
  await press("select"); // Rate, Go to show, Go to season, Media info, Add to playlist
  await sleep(500);
  shoot("episode-more");
  since = mark();
  await press("down", 2);
  await press("select");
  await waitFor(/^GET \/api\/items\/100\/children$/, since, "the episode's season");
  season.opened = true;
  since = mark();
  await press("back");
  await waitFor(/^GET \/api\/items\/103$/, since, "the episode's page again");
  await press("up", 4);
  // Back while the player starts: the session that comes after is closed.
  since = mark();
  fixture.playbackDelayMs = 2500;
  await press("select"); // Resume
  await waitFor(/^POST \/api\/playback$/, since, "a playback session", { settles: false });
  await press("back");
  fixture.playbackDelayMs = 0;
  await waitFor(/^DELETE \/api\/playback\/s1$/, since, "closing the session answered after Back");
  // Played this time: the picture subtitles picked are the app's to draw,
  // so nothing is burned in. Their list is asked for from where the
  // playback starts, with the session's key, then again until it is
  // whole; the first picture to come loads. Then left.
  since = mark();
  await press("select"); // Resume
  await waitFor(/^POST \/api\/playback$/, since, "a playback session");
  await waitFor(/^GET \/api\/media\/pley_mk_s2\/subtitles-4\.json\?at=\d+$/, since, "the picture subtitles where the playback starts");
  const firstList = mark();
  await waitFor(/^GET \/api\/media\/pley_mk_s2\/subtitles-4\.json\?at=\d+$/, firstList, "the rest of the picture subtitles", { ms: 10000 });
  pictures.loaded = await loadedWithin(5000);
  shoot("player-pictures");
  since = mark();
  await press("back");
  await waitFor(/^DELETE \/api\/playback\/s2$/, since, "closing the session that drew pictures");
  // A server without them (404): burned in after all, in a new session
  // from where the video was, and for the rest of this playback.
  fixture.picturesMissing = true;
  since = mark();
  const sessionsBefore = fixture.playbacks.length;
  await press("select"); // Resume
  await waitFor(/^GET \/api\/media\/pley_mk_s3\/subtitles-4\.json\?at=\d+$/, since, "the picture subtitles of a server without them", { settles: false });
  const burnedUntil = Date.now() + 10000;
  while (fixture.playbacks.length < sessionsBefore + 2 && Date.now() < burnedUntil) await sleep(100);
  if (fixture.playbacks.length < sessionsBefore + 2) throw new Error("no request for a session with the picture subtitles burned in");
  await settle();
  shoot("player");
  // pley's menu (Down), and its subtitle sync: right moves the subtitles
  // later, and Back closes it, the stream opening again with its text
  // track moved.
  await press("down");
  await sleep(300);
  shoot("player-menu");
  await press("down", 2); // past Subtitles and Quality: Subtitle sync
  await press("select");
  await press("right", 3);
  sync.label = await drawnWithin(2500, syncDrawn);
  shoot("player-sync");
  await press("back");
  // The menu again: the first chapter gone to, then the playback's details,
  // what the server makes of the conversion asked for.
  await press("down");
  await press("down"); // Chapters
  await press("select");
  await press("up");
  await press("select");
  // The sleep timer, after Chapters: 15 minutes, which the menu then
  // counts down.
  await press("down");
  await press("down"); // Sleep timer
  await press("select");
  await sleep(300);
  shoot("player-sleep");
  await press("down"); // 15 min
  await press("select");
  await press("down");
  await sleep(300);
  shoot("player-menu-sleep");
  since = mark();
  await press("down", 2); // past Find subtitles…, Playback details
  await press("select");
  await waitFor(/^GET \/api\/playback\/s\d+$/, since, "the conversion's details");
  shoot("player-details");
  await press("back", 2);
  // Find subtitles…: the server's results listed, the first added, saved
  // as the pick, and the stream opened again with it (a new session, as
  // the server drew the picture subtitles onto this one).
  since = mark();
  await press("down");
  await press("up"); // Find subtitles…
  await press("select");
  await waitFor(/^GET \/api\/items\/103\/subtitles\/search\?file_id=7103$/, since, "the subtitles found for the file");
  shoot("player-find-subtitles");
  // Another language: Language, above the results, then Arabic.
  since = mark();
  await press("up");
  await press("select");
  await sleep(300);
  await press("down"); // Arabic
  await press("select");
  await waitFor(/^GET \/api\/items\/103\/subtitles\/search\?file_id=7103&language=ara$/, since, "the subtitles found in another language");
  since = mark();
  await press("select");
  await waitFor(/^POST \/api\/items\/103\/subtitles$/, since, "the subtitles picked, added");
  await waitFor(/^PUT \/api\/items\/103\/streams$/, since, "the added subtitles saved as the pick");
  await waitFor(/^POST \/api\/playback$/, since, "a session without the picture subtitles drawn");
  await settle();
  // The menu's Subtitles, at its top: the file's every track, English
  // (SRT) picked, saved as the pick, and the stream opened again with it.
  since = mark();
  const streamsBefore = fixture.streams.length;
  await press("down");
  await press("up", 8);
  await press("select");
  await press("up", 3); // Off
  await press("down"); // English (SRT)
  await press("select");
  await waitFor(/^PUT \/api\/items\/103\/streams$/, since, "the subtitles picked in the player's menu, saved");
  await waitFor(/^POST \/api\/playback$/, since, "the stream opened again with the subtitles picked");
  menuTracks.picked = fixture.streams.slice(streamsBefore).some((s) => s.item === 103 && s.subtitle_index === 3);
  since = mark();
  await press("back"); // stopped, then the session closed
  await waitFor(/^DELETE \/api\/playback\//, since, "closing the session");
  since = mark();
  await press("back"); // Home, which asks for its rows again after playing
  await waitFor(/^GET \/api\/hubs\/home$/, since, "Home's rows after playing");
  // The Options key on the first of Continue Watching: Mark as watched, and
  // Home asks for its rows again.
  since = mark();
  await press("options");
  await sleep(500);
  shoot("home-options");
  await press("select");
  await waitFor(/^POST \/api\/items\/103\/watched$/, since, "a title marked watched");
  // See all at the end of Recently Added in Music: the library's Library
  // tab, newest added first, for this visit only; Back to Home.
  await settle();
  since = mark();
  await press("down", 4); // past Continue Watching, My List, Movies and TV Shows
  await press("right"); // See all, after the album
  await press("select");
  await waitFor(/^GET \/api\/libraries\/3\/items\?.*sort=added/, since, "Recently Added's See all");
  shoot("see-all");
  await press("back");
  await settle();
  seeAll.after = mark();
  // Live TV: the categories and the guide of every channel (a page, its
  // programmes for those channels), OK on what's on now watches the
  // channel, Up the next one (the server ends the first by the
  // client_id), the channel list, a favourite and another category
  // listed there, then a channel whose
  // source has no connection left; Back leaves to the guide at that
  // channel, then the Channels tab.
  await openFromMenu(4, /^GET \/api\/live\/guide\?/, "Live TV's guide");
  await sleep(1500);
  shoot("live-guide");
  await press("right"); // the guide
  since = mark();
  await press("select");
  await waitFor(/^POST \/api\/live\/channels\/1\/watch$/, since, "a channel watched");
  await waitFor(/^GET \/api\/live\/sessions\/live\d+\/index\.m3u8$/, since, "its playlist");
  await sleep(1000);
  shoot("live-player");
  since = mark();
  await press("up");
  await waitFor(/^POST \/api\/live\/channels\/2\/watch$/, since, "the next channel watched");
  await waitFor(/^GET \/api\/live\/sessions\/live\d+\/index\.m3u8$/, since, "the next channel's playlist");
  await press("left");
  await sleep(800);
  shoot("live-list");
  since = mark();
  await press("options");
  await waitFor(/^PUT \/api\/live\/channels\/2\/favorite$/, since, "a channel put in the favourites");
  // Left in the list: the categories, on All channels; Favourites, two up,
  // lists its channels there. Back closes the list: up and down still go
  // through All channels.
  since = mark();
  await press("left");
  await waitFor(/^GET \/api\/live\/groups$/, since, "the categories in the player's list");
  await settle();
  await press("up", 2);
  since = mark();
  await press("select");
  await waitFor(/^GET \/api\/live\/browse\?.*category=favorites/, since, "the favourites listed in the player");
  await sleep(800);
  shoot("live-list-favourites");
  await press("back");
  since = mark();
  await press("up");
  await waitFor(/^POST \/api\/live\/channels\/3\/watch$/, since, "a channel with no connection left");
  await sleep(1500);
  live.busy = await drawnWithin(2500, noticeDrawn);
  shoot("live-busy");
  // Watch here instead: the same channel, taking over the connection.
  since = mark();
  await press("select");
  await waitFor(/^POST \/api\/live\/channels\/3\/watch$/, since, "a channel watched taking over the connection");
  await waitFor(/^GET \/api\/live\/sessions\/live\d+\/index\.m3u8$/, since, "the playlist of the channel taken over");
  since = mark();
  await press("back"); // leaves: the guide opens at the channel watched
  await waitFor(/^GET \/api\/live\/browse\?.*around=3/, since, "the guide around the channel watched");
  await settle();
  shoot("live-back");
  await press("back"); // the categories, on All channels
  await press("up", 4); // the tabs, above Search and Favourites
  await press("right"); // Channels
  since = mark();
  await press("select");
  await waitFor(/^GET \/api\/live\/browse\?/, since, "the channels as cards");
  await settle();
  shoot("live-channels");
  await press("left"); // Guide again, which the walk's next start expects
  await press("select");
  await settle();
  await press("down"); // the categories
  await press("back"); // Home
  await settle();
  since = mark();
  await openFromMenu(5, /^GET \/api\/libraries\/1\/items\?.*offset=0/, "Movies");
  // Sorted by title: the A to Z, once every title's letter came, right of
  // the grid's last column (a right press more than needed, which the
  // letters take). Down from "#" to W, whose first title is on the third
  // page: OK asks for that page and the grid goes there.
  await waitFor(/^GET \/api\/libraries\/1\/items\?.*limit=1000/, since, "the library's letters");
  shoot("library");
  await press("right", 8);
  await press("down", 23);
  library.letters = await drawnWithin(2500, () => letterMarked(23, true));
  shoot("library-letters");
  since = mark();
  await press("select");
  await waitFor(/^GET \/api\/libraries\/1\/items\?.*offset=140/, since, "the page of the letter jumped to");
  // brs-node takes seconds to hand a task's ContentNodes over.
  library.jumped = await drawnWithin(20000, () => letterMarked(23, false));
  shoot("library-jumped");
  // Up from there: the page before, skipped by the jump, comes.
  since = mark();
  for (let i = 0; i < 4 && !seen(/^GET \/api\/libraries\/1\/items\?.*offset=70/, since); i++) await press("up");
  await waitFor(/^GET \/api\/libraries\/1\/items\?.*offset=70/, since, "the page before");
  shoot("library-scrolled");
  await press("back");
  // Movies again, sorted by bitrate: the posters' quality badges, and
  // their bitrate and size under them. Then the Filter panel, which adds
  // Resolution and Dynamic range once the server says the movies differ in
  // them, and Resolution's 4K: the grid lists 4K titles only, and the chip
  // says so.
  await openFromMenu(5, /^GET \/api\/libraries\/1\/items\?.*offset=0/, "Movies again");
  await settle();
  await press("up"); // the tabs, on Library
  await press("right", 2); // past Collections to the sort
  await press("select");
  await sleep(500);
  since = mark();
  await press("down", 10); // Bitrate, the last
  await press("select");
  await waitFor(/^GET \/api\/libraries\/1\/items\?.*sort=bitrate/, since, "the library sorted by bitrate");
  await settle();
  shoot("library-bitrate");
  await press("right"); // the filters
  since = mark();
  await press("select");
  await waitFor(/^GET \/api\/libraries\/1\/filters$/, since, "what a library can be filtered by");
  await settle();
  shoot("library-filters");
  await press("down", 3); // Resolution, after Year
  await press("select");
  await sleep(500);
  shoot("library-resolutions");
  since = mark();
  await press("down"); // 4K
  await press("select");
  await waitFor(/^GET \/api\/libraries\/1\/items\?.*resolution=4k/, since, "the library filtered by resolution");
  await settle();
  shoot("library-filtered");
  await press("back"); // the chips
  await settle();
  shoot("library-4k");
  await press("back");
  await openFromMenu(6, /^GET \/api\/libraries\/2\/items/, "TV Shows");
  // Its tabs: Recommended, Collections, then Library again, sorted by year.
  since = mark();
  await press("up"); // the tabs, on Library
  await press("left"); // Recommended
  await press("select");
  await waitFor(/^GET \/api\/libraries\/2\/hubs$/, since, "a library's recommendations");
  shoot("library-recommended");
  // See all after Recently Added's one show: the Library tab, newest added
  // first.
  since = mark();
  await press("down"); // the rows
  await press("right"); // See all
  await press("select");
  await waitFor(/^GET \/api\/libraries\/2\/items\?.*sort=added/, since, "a library row's See all");
  shoot("library-see-all");
  await press("up"); // the tabs, on Library
  await press("right"); // Collections
  await press("select");
  await waitFor(/^GET \/api\/libraries\/2\/collections$/, since, "a library's collections");
  shoot("library-collections");
  // Its smart collection of shows: Play asks for the show's next episode
  // and plays it. Back to the collections, then the tabs.
  await press("down"); // the collections
  since = mark();
  await press("select");
  await waitFor(/^GET \/api\/collections\/2$/, since, "a smart collection's page");
  shoot("collection-smart");
  since = mark();
  await press("select"); // Play
  await waitFor(/^GET \/api\/items\/99\/on-deck$/, since, "the next episode of the collection's show");
  await waitFor(/^POST \/api\/playback$/, since, "the collection's show played");
  collections.shows = fixture.playbacks.at(-1)?.item === 103;
  since = mark();
  await press("back"); // stopped, then the session closed
  await waitFor(/^DELETE \/api\/playback\//, since, "closing the session of the collection's show");
  await press("back"); // the library's collections
  await settle();
  await press("up"); // the tabs, on Collections
  await press("left"); // Library
  await press("select");
  await press("right", 2); // past Collections to the sort
  await press("select");
  await sleep(500);
  await press("up"); // Year, above Date added (See all's order)
  await press("select");
  await waitFor(/^GET \/api\/libraries\/2\/items\?.*sort=year/, since, "the library sorted by year");
  shoot("library-sorted");
  // Filter: Unwatched, then Save as…: the filters and the order as a smart
  // playlist, named on the keyboard, and its page; Back to the library.
  await press("right"); // the filters
  await press("select");
  await sleep(500);
  since = mark();
  await press("select"); // Show
  await press("down"); // Unwatched
  await press("select");
  await waitFor(/^GET \/api\/libraries\/2\/items\?.*watched=false/, since, "the library filtered");
  await press("down", 4); // Save as…, after Clear filters
  await press("select");
  await sleep(800);
  for (const c of "Unwatched") await press(`lit_${c}`);
  shoot("save-as");
  since = mark();
  await press("down", 6); // past the keyboard, to Cancel
  await press("up"); // Save
  await press("select");
  await waitFor(/^GET \/api\/playlists\/2$/, since, "the smart playlist's page");
  shoot("smart-playlist");
  await press("back"); // the library, on the filters
  await settle();
  await press("left", 2); // the sort, then the tabs
  await press("down"); // the grid
  since = mark();
  await press("select"); // Severance: its page, with its next episode first
  await waitFor(/^GET \/api\/items\/99\/on-deck$/, since, "the show's next episode");
  shoot("show-details");
  since = mark();
  await press("down"); // Episodes
  await press("select");
  await waitFor(/^GET \/api\/items\/100\/children$/, since, "a season's episodes");
  shoot("show");
  await press("right"); // the episodes
  shoot("show-episodes");
  since = mark();
  await press("back"); // the show's page, its buttons back with its next episode
  await waitFor(/^GET \/api\/items\/99\/on-deck$/, since, "the show's next episode again");
  // More…, its last button: 4 stars from the rating's choices, saved at
  // once; Back from More to the buttons.
  since = mark();
  await press("down", 3); // from Episodes, past the watched mark and My List
  await press("select");
  await sleep(500);
  shoot("show-more");
  await press("select"); // Rate: its choices, No rating checked
  await press("down", 4);
  shoot("show-rating");
  await press("select"); // 4 stars, then More again
  await waitFor(/^PUT \/api\/items\/99\/rating$/, since, "the show rated");
  await press("back");
  // Down from More… to its rows, the seasons then cast and crew, whose
  // first opens a person's page.
  since = mark();
  await press("down", 2);
  shoot("show-rows");
  await press("select");
  await waitFor(/^GET \/api\/people\/7$/, since, "a person's page");
  shoot("person");
  // Their Directing row, under Acting: its first title opened, then Back.
  await press("down");
  since = mark();
  await press("select");
  await waitFor(/^GET \/api\/items\/1000$/, since, "a title of a person's Directing row");
  person.opened = true;
  await press("back");
  await settle();
  // Back on the show's page, its last row: the collections it is part of,
  // whose first, of five movies, plays them in its order. Down past its
  // buttons slides its page up to its posters, Up back to the buttons.
  await press("back");
  await settle();
  await press("down", 2); // past More like this to Collections
  since = mark();
  await press("select");
  await waitFor(/^GET \/api\/collections\/1$/, since, "a collection of movies");
  shoot("collection");
  await press("down", 2); // past Shuffle
  await sleep(500);
  shoot("collection-titles");
  await press("up", 2); // Shuffle, then Play
  since = mark();
  await press("select"); // Play
  await waitFor(/^POST \/api\/playback$/, since, "the collection played");
  collections.movies = fixture.playbacks.at(-1)?.item === 1000;
  since = mark();
  await press("back"); // stopped, then the session closed
  await waitFor(/^DELETE \/api\/playback\//, since, "closing the collection's session");
  // The collection, the show's page and its buttons, the library, Home:
  // one at a time, each screen settled before the next key.
  for (let i = 0; i < 4; i++) {
    await press("back");
    await settle();
  }
  // Music: the artist's page, whose Play gathers every album's tracks for
  // the music player; Now Playing stopped. Then their older album, its
  // tracks played; the next in Now Playing's Up next played, then Shuffle,
  // which puts the track playing first (Previous plays it again), and Stop
  // shuffling, the album's order again (Previous plays the one before), and
  // the sleep timer's button. Now Playing stopped; then the album's second
  // disc played from its list and stopped, and Back to Home.
  await openFromMenu(7, /^GET \/api\/libraries\/3\/items\?.*offset=0/, "Music");
  shoot("music");
  since = mark();
  await press("select"); // the artist
  await waitFor(/^GET \/api\/items\/5000\/children$/, since, "an artist's albums");
  shoot("artist");
  since = mark();
  await press("select"); // Play
  await waitFor(/^POST \/api\/playback$/, since, "the artist's tracks played");
  artistPlayed.gathered = seen(/^GET \/api\/items\/5100\/children$/, since) && seen(/^GET \/api\/items\/5101\/children$/, since);
  artistPlayed.first = fixture.playbacks.at(-1)?.item === 5201;
  await sleep(1500);
  shoot("now-playing");
  since = mark();
  await press("right", 2); // Stop: back to the artist
  await press("select");
  await waitFor(/^DELETE \/api\/playback\//, since, "the artist's track's session closed");
  await press("right"); // the albums, the newest first
  await press("right"); // the older one
  since = mark();
  await press("select");
  await waitFor(/^GET \/api\/items\/5100\/children$/, since, "an album's tracks");
  shoot("album");
  since = mark();
  await press("select"); // Play
  await waitFor(/^POST \/api\/playback$/, since, "a track played");
  await sleep(1500);
  shoot("now-playing");
  await press("down"); // Up next, on the track playing
  await press("down"); // the next
  await sleep(300);
  shoot("now-playing-up-next");
  music.upNext = await playedBy("a track of Up next played");
  await press("up", 2); // the first track, then the buttons, on Pause
  await press("left", 2); // Shuffle
  await press("select");
  await sleep(300);
  shoot("now-playing-shuffled");
  await press("right"); // Previous
  music.shuffled = await playedBy("Previous, shuffled");
  await press("left"); // Stop shuffling
  await press("select");
  await press("right"); // Previous
  music.unshuffled = await playedBy("Previous, in the album's order");
  // The sleep timer's button: 15 minutes, saying what is left, then on
  // through its choices to End of this track and Off again.
  await press("right", 4); // Sleep timer
  await sleep(300);
  music.sleepOff = !sleepButtonLong();
  await press("select");
  music.sleepSet = await drawnWithin(2000, sleepButtonLong);
  shoot("now-playing-sleep");
  await press("select", 5); // 30, 45, 60 and 90 min, End of this track
  music.sleepEnd = await drawnWithin(2000, sleepButtonLong);
  await press("select"); // Off
  music.sleepCycled = await drawnWithin(2000, () => !sleepButtonLong());
  since = mark();
  await press("left"); // Stop
  await press("select");
  await waitFor(/^DELETE \/api\/playback\//, since, "the track's session closed");
  // The album again: its tracks under "Disc 1" and "Disc 2", headings the
  // focus passes over; OK on the second disc's track plays that one, and
  // stopped, the album shows again.
  await settle();
  await press("right"); // the tracks, on the first under "Disc 1"
  await press("down", 2); // the second, then past "Disc 2" to its track
  shoot("album-discs");
  music.disc = await playedBy("a track of the album's second disc");
  since = mark();
  await press("right", 2); // Stop
  await press("select");
  await waitFor(/^DELETE \/api\/playback\//, since, "its session closed");
  await settle();
  for (let i = 0; i < 3; i++) {
    await press("back"); // the artist, the library, Home
    await settle();
  }
  await openFromMenu(0, /^GET \/api\/search\?.*q=mo/, "Search", async () => {
    await sleep(800);
    await press("lit_m");
    await press("lit_o");
  });
  shoot("search");
  // The results in rows by kind, People last, just above Discover: Right
  // past the keyboard's last column reaches them, and OK on the person
  // opens their page.
  since = mark();
  await press("right", 7);
  await press("down", 12); // Discover, the last row
  await press("up"); // People
  await press("select");
  await waitFor(/^GET \/api\/people\/11$/, since, "the person found, first of the results");
  shoot("search-person");
  await press("back"); // Search, on the rows
  await settle();
  // See all, ending the Discover row (the last row, under the
  // libraries'): all of TMDB's titles for the words, in a grid of their
  // own; Back to Search's rows, then Back again to its keyboard (Left
  // would only go along the row).
  since = mark();
  await press("down", 12); // Discover, the last row
  await press("right", 21); // See all, after a page of titles at most
  await press("select");
  await waitFor(/^GET \/api\/discover\/search\?q=mo&page=1$/, since, "TMDB's results for the words");
  shoot("search-tmdb");
  await press("back"); // Search, on the rows
  await settle();
  await press("back"); // the keyboard
  // Back from Search nothing played: Home stays as it is, unless its rows
  // are a minute old by now (a slow run), when it asks again. They are as
  // old as Home's last asking.
  since = mark();
  const loads = fixture.requests.filter((r) => r.method === "GET" && r.path === "/api/hubs/home");
  const age = Date.now() - loads[loads.length - 1].at;
  await press("back");
  await settle();
  const asked = seen(/^GET \/api\/hubs\/home$/, since);
  home.kept = age < 55000 ? !asked : age > 65000 ? asked : true;
  // Search again: before anything is typed, the recent searches, "mo"
  // first since one of its results was opened; OK on it searches it again
  // at once. Back to Home.
  since = mark();
  await openFromMenu(0, /^GET \/api\/search\?.*q=mo/, "a recent search searched again", async () => {
    await sleep(800);
    shoot("search-recent");
    await press("right", 7); // past the keyboard's last column
    await press("select");
  });
  searched.again = seen(/^GET \/api\/search\?.*q=mo/, since);
  await press("back");
  await settle();
  // Playlists: one opened, its first entry moved down one with the Options
  // key's Move, then taken out, and the playlist played from the top. Then
  // New playlist, for Movies & TV, named on the keyboard: it opens, made.
  // Back to Home.
  await openFromMenu(8, /^GET \/api\/playlists$/, "Playlists");
  shoot("playlists");
  since = mark();
  await press("select"); // the first playlist
  await waitFor(/^GET \/api\/playlists\/1$/, since, "a playlist");
  shoot("playlist");
  since = mark();
  await press("right"); // the entries
  await press("options");
  await sleep(500);
  await press("select"); // Move
  await sleep(300);
  await press("down");
  await sleep(300);
  shoot("playlist-move");
  await press("select"); // put there
  await waitFor(/^PUT \/api\/playlists\/1\/order$/, since, "the playlist's new order");
  await settle();
  since = mark();
  await press("options"); // the entry moved, now second
  await sleep(500);
  await press("down");
  await press("select"); // Remove from playlist
  await waitFor(/^DELETE \/api\/playlists\/1\/items\/\d+$/, since, "an entry removed");
  since = mark();
  await press("left"); // the buttons, Play first
  await press("select");
  await waitFor(/^POST \/api\/playback$/, since, "the playlist played");
  since = mark();
  await press("back"); // the playlist
  await waitFor(/^DELETE \/api\/playback\//, since, "the playlist's session closed");
  await press("back"); // the playlists, on the one played
  await settle();
  await press("up"); // New playlist
  await press("select");
  await sleep(800);
  shoot("new-playlist");
  await press("select"); // Movies & TV
  await sleep(800);
  for (const letter of "Trip") await press(`lit_${letter}`);
  shoot("new-playlist-name");
  since = mark();
  await press("down", 4); // past the keyboard's four rows to Create
  await press("select");
  await waitFor(/^POST \/api\/playlists$/, since, "a new playlist");
  // Numbered after the smart playlist the library's Save as… made.
  await waitFor(/^GET \/api\/playlists\/3$/, since, "the new playlist opened");
  shoot("new-playlist-made");
  since = mark();
  await press("back"); // the playlists, the new one among them
  await waitFor(/^GET \/api\/playlists$/, since, "the playlists again");
  shoot("playlists-made");
  await press("back"); // Home
  await settle();
  // Discover: the first trending title over its backdrop, its Request
  // asked after a question (Anna's asks are approved as she asks), then a
  // show of the trending row, its seasons picked.
  await openFromMenu(2, /^GET \/api\/discover\/trending\?page=1$/, "Discover");
  shoot("discover");
  since = mark();
  await press("select"); // the hero's Request: its page, asking at once
  await waitFor(/^GET \/api\/discover\/movie\/7001$/, since, "the hero's title on TMDB");
  shoot("discover-ask");
  since = mark();
  await press("select"); // Request
  await waitFor(/^POST \/api\/requests$/, since, "a movie asked for");
  shoot("discover-asked");
  await press("select"); // OK: its page, saying how it stands
  await settle();
  shoot("discover-title");
  await press("back"); // Discover, on the hero
  await settle();
  await press("down"); // the rows
  await press("right"); // a show
  since = mark();
  await press("select");
  await waitFor(/^GET \/api\/discover\/show\/8004$/, since, "a show on TMDB");
  shoot("discover-show");
  since = mark();
  await press("select"); // Request: which seasons, those missing ticked
  await sleep(800);
  shoot("request-seasons");
  await press("right"); // Request 3 seasons
  await press("select");
  await waitFor(/^POST \/api\/requests$/, since, "the seasons asked for");
  shoot("request-seasons-asked");
  await press("select"); // OK: the show's page
  for (let i = 0; i < 3; i++) {
    await press("back"); // Discover's rows, its hero, Home
    await settle();
  }
  // The managers' Requests, on Needs attention: the Options key on the
  // oldest, Approve. Meanwhile Anna's download came into a library, which
  // no event told of: the summary asked after Approve has news, whose
  // notice waits while the requests show. Then Yours, the last tab, which
  // marks Anna's news seen.
  await openFromMenu(3, /^GET \/api\/requests\?.*attention=1/, "Requests");
  shoot("requests");
  since = mark();
  await press("options");
  await sleep(500);
  shoot("requests-options");
  fixture.requestArrives(3, 1011);
  await press("select"); // Approve
  await waitFor(/^POST \/api\/requests\/1\/approve$/, since, "a request approved");
  await waitFor(/^GET \/api\/requests\?.*unseen=1/, since, "the requests with news");
  notice.waited = !(await drawnWithin(2500, readyNoticeDrawn));
  shoot("requests-approved");
  for (let attempt = 0; attempt < 3 && !seen(/^GET \/api\/requests\?.*mine=1/, since); attempt++) {
    await press("up"); // the tabs
    await press("left", 8);
    await press("right", 7); // Yours
    await press("select");
    await sleep(1500);
  }
  await waitFor(/^POST \/api\/requests\/seen$/, since, "the news marked seen");
  shoot("requests-yours");
  // Home: the notice's card, once the remote has rested, with Open
  // focused.
  await press("back");
  notice.shown = await drawnWithin(8000, readyNoticeDrawn);
  // An OK in the card's first 0.6 s was meant for the page.
  await sleep(800);
  shoot("ready-to-watch");
  since = mark();
  await press("select"); // Open
  await waitFor(/^GET \/api\/items\/1011$/, since, "the title the notice opens");
  notice.opened = true;
  // Its page: Audio and subtitles, after Play and the watched mark. French
  // picked, saved with the subtitles shown; then subtitles off, saved with
  // the French picked here; then the Director's Cut, which Play plays.
  await settle();
  let pickedSince = mark();
  await press("down", 2);
  await press("select"); // Version, Audio: Automatic (English), Subtitles: English (SRT)
  await sleep(500);
  shoot("movie-tracks");
  await press("down");
  await press("select"); // Audio's choices, Automatic checked
  await press("down", 2);
  shoot("movie-audio");
  await press("select"); // French (AC3 5.1), then the menu again
  await waitFor(/^PUT \/api\/items\/1011\/streams$/, pickedSince, "the audio picked, saved");
  pickedSince = mark();
  await press("down");
  await press("select"); // Subtitles' choices, English (SRT) checked
  await press("up");
  await press("select"); // Off
  await waitFor(/^PUT \/api\/items\/1011\/streams$/, pickedSince, "the subtitles turned off, saved");
  await press("up", 2);
  await press("select"); // Version's choices, the first checked
  await press("down");
  shoot("movie-versions");
  await press("select"); // the Director's Cut, then the menu: its subtitles
  await sleep(500);
  await press("back"); // the buttons, on Audio and subtitles
  await press("up", 2);
  pickedSince = mark();
  await press("select"); // Play
  await waitFor(/^POST \/api\/playback$/, pickedSince, "the version chosen played");
  pickedSince = mark();
  await press("back");
  await waitFor(/^DELETE \/api\/playback\//, pickedSince, "the version's session closed");
  // Back on its page, its Trailer, past My List, played from its start;
  // then More…: Media info, its two files and their streams; Back to More,
  // to the buttons, to Home.
  await settle();
  shoot("movie-details");
  pickedSince = mark();
  await press("down", 4);
  await press("select"); // Trailer
  await waitFor(/^POST \/api\/playback$/, pickedSince, "the trailer played");
  pickedSince = mark();
  await press("back");
  await waitFor(/^DELETE \/api\/playback\//, pickedSince, "the trailer's session closed");
  await settle();
  await press("down");
  await press("select"); // Rate, Media info, Add to playlist
  await sleep(500);
  await press("down");
  await press("select");
  mediaInfo.shown = await drawnWithin(2500, infoDrawn);
  shoot("movie-media-info");
  await press("back");
  await press("back");
  await press("back"); // Home
  await settle();
  await openFromMenu(11, /^GET \/api\/auth\/me$/, "Settings");
  shoot("settings");
  // Diagnostics and the licences, at the right; Left back to the settings.
  await press("right");
  await press("select"); // Diagnostics
  pages.diagnostics = await drawnWithin(3000, pageTextDrawn);
  shoot("diagnostics");
  await press("back");
  await settle();
  await press("down");
  await press("select"); // Open-source licences
  pages.licences = await drawnWithin(3000, pageTextDrawn);
  shoot("licences");
  await press("back");
  await settle();
  await press("left");
  // Quality, after Language, Audio language and Subtitles: Automatic
  // checked, then 8 Mbps 1080p chosen, kept on the Roku.
  await press("down", 3);
  await press("select");
  await sleep(500);
  shoot("settings-quality");
  await press("down", 5);
  await press("select");
  await settle();
  // Watch history, two rows down: its plays; Back to Settings.
  since = mark();
  await press("down", 2);
  await press("select");
  await waitFor(/^GET \/api\/history\?/, since, "the watch history");
  shoot("history");
  await press("back");
  await settle();
  // Always start as, after Switch profile (a household of two): Anna
  // chosen, kept on the Roku.
  await press("down", 2);
  await press("select");
  await sleep(500);
  await press("down"); // Anna
  shoot("settings-start-as");
  await press("select");
  await settle();
  // Sort libraries, next: Movies lifted and put after TV Shows, saved at
  // once; then the server's order again, which the rest of the walk
  // expects.
  since = mark();
  await press("down");
  await press("select"); // Sort libraries
  await waitFor(/^GET \/api\/libraries$/, since, "the libraries to sort");
  await press("select"); // lifts Movies
  await press("down");
  await press("select"); // puts it after TV Shows
  await waitFor(/^PUT \/api\/me\/libraries\/order$/, since, "the new order saved");
  shoot("sort-libraries");
  await press("right");
  await press("select"); // Use the server's order
  await waitFor(/^DELETE \/api\/me\/libraries\/order$/, since, "the server's order again");
  await press("back"); // Settings
  since = mark();
  await press("back"); // Home, which asks for its libraries again
  await waitFor(/^GET \/api\/libraries$/, since, "Home's libraries after sorting");
  await press("right"); // the rows, where the next steps start
  await openFromMenu(11, /^GET \/api\/auth\/me$/, "Settings");
  // Language, the first row: Español for the account, the screen in
  // Spanish once the server has it, and Home asked for again in it, its
  // rows' titles being the server's; then Automatic again, which the rest
  // of the walk expects.
  await settle();
  since = mark();
  await press("select"); // Language: its choices, Automatic checked
  await sleep(500);
  await press("down", 3); // Español
  await press("select");
  await waitFor(/^PUT \/api\/me\/language$/, since, "the account's language saved");
  shoot("settings-language");
  since = mark();
  await press("back"); // Home, made again in Spanish
  await waitFor(/^GET \/api\/hubs\/home$/, since, "Home in Spanish");
  language.home = fixture.requests.slice(since).some((r) => r.path === "/api/hubs/home" && r.language === "es");
  shoot("home-spanish");
  await press("right");
  await openFromMenu(11, /^GET \/api\/auth\/me$/, "Settings");
  await settle();
  since = mark();
  await press("select"); // Language, Español checked
  await sleep(500);
  await press("up", 3); // Automatic
  await press("select");
  await waitFor(/^PUT \/api\/me\/language$/, since, "Automatic saved");
  // A server without the route, though it names a language: Français is
  // answered 404 and kept on this Roku, the screen and Home in French.
  // Settings opened again still says Français, and Automatic there forgets
  // it with no request, the screen in English again.
  fixture.languageRoute = false;
  await settle();
  since = mark();
  await press("select"); // Language, Automatic checked
  await sleep(500);
  await press("down", 2); // Français
  await press("select");
  await waitFor(/^PUT \/api\/me\/language$/, since, "Français answered 404");
  await settle();
  since = mark();
  await press("back"); // Home, made again in French
  await waitFor(/^GET \/api\/hubs\/home$/, since, "Home in French");
  language.refusedHome = fixture.requests.slice(since).some((r) => r.path === "/api/hubs/home" && r.language === "fr");
  await press("right");
  await openFromMenu(11, /^GET \/api\/auth\/me$/, "Settings");
  await settle();
  await press("select"); // Language, Français checked
  await sleep(500);
  await press("up", 2); // Automatic
  await press("select");
  await settle();
  fixture.languageRoute = true;
  // Subtitles: from "Only for foreign dialogue" to "Always", saved at once
  // with the languages left as they were; Subtitle language shows then.
  await settle();
  since = mark();
  await press("down", 2);
  await press("select"); // Subtitles: its choices, Forced focused
  await sleep(500);
  await press("down", 2);
  shoot("settings-subtitles");
  await press("select"); // Always
  await waitFor(/^PUT \/api\/me\/track-defaults$/, since, "the subtitle mode saved");
  language.afterRefused = fixture.requests.slice(since).find((r) => r.path === "/api/me/track-defaults")?.language ?? "";
  await settle();
  shoot("settings-saved");
  // Switch profile, five rows down: "Who's watching?", Ben chosen, and
  // Home again as Ben, the old tokens stopped.
  since = mark();
  await press("down", 5);
  await press("select");
  await waitFor(/^GET \/api\/oauth\/profiles$/, since, "the household's profiles");
  shoot("profiles");
  since = mark();
  await press("right"); // Ben
  await press("select");
  await waitFor(/^GET \/api\/hubs\/home$/, since, "Home as another profile");
  // The server goes away: a library can't load, and says so.
  await sleep(1500);
  fixture.down = true;
  await press("left", 3);
  await press("up", 11);
  await press("down", 5); // Movies, after Search, Home, Discover, Your requests and Live TV
  await press("select");
  await sleep(3000);
  shoot("unreachable");
  fixture.down = false;
  since = mark();
  await press("select"); // Try again
  await waitFor(/^GET \/api\/libraries\/1\/items\?.*offset=0/, since, "the library after Try again");
  shoot("retried");
  // Signed out, and in again with a server that has no QR codes yet, nor
  // requests, and doesn't say whether it requires two-step sign-in: the
  // code shows on its own, the sign-in asks for no scope the server
  // doesn't know, and nothing is said of two-step sign-in.
  await press("back");
  await openFromMenu(11, /^GET \/api\/auth\/me$/, "Settings");
  fixture.qr = false;
  fixture.requestScopes = false;
  fixture.twoStep = false;
  since = mark();
  older.since = since;
  await signOut();
  await waitFor(/^POST \/api\/oauth\/device$/, since, "a new TV code", { settles: false });
  await waitFor(/^GET \/api\/oauth\/device\/qr\?code=LMNPQRST$/, since, "the new code's QR code", { settles: false });
  // The code stays up until the third poll, two seconds on.
  qr.hidden = !(await drawnWithin(1500, qrDrawn)) && seen(/^POST \/api\/oauth\/device$/, since) && !seen(/^GET \/api\/hubs\/home$/, since);
  twoStep.hidden = seen(/^GET \/api\/server$/, since) && !twoStepDrawn() && !seen(/^GET \/api\/hubs\/home$/, since);
  shoot("sign-in-again");
  await waitFor(/^GET \/api\/hubs\/home$/, since, "Home after signing in again");
}

// Started again, signed in, from what the first run saved: how long until
// Home's rows are asked for, and the first picture.
async function restart() {
  const file = join(shots, "registry.json");
  writeFileSync(file, JSON.stringify([...registry]));
  const since = mark();
  const start = Date.now();
  const child = spawn(process.execPath, [script, "--restart", file, String(step + 1)], { stdio: ["pipe", "pipe", "inherit"] });
  let output = "";
  child.stdout.on("data", (chunk) => (output += chunk));
  const exited = new Promise((resolve) => child.on("exit", resolve));
  try {
    await waitFor(/images\//, since, "a picture after a restart", { ms: 60000 });
  } finally {
    child.stdin.end("done\n");
  }
  const code = await exited;
  let main = start;
  for (const line of output.split("\n")) {
    if (line.startsWith("error,")) messages.push(line);
    if (line.startsWith("main,")) main = Number(line.slice(5));
  }
  if (code !== 0) throw new Error(`the second start's process ended with ${code}`);
  const first = (pattern) => fixture.requests.slice(since).find((r) => pattern.test(`${r.method} ${r.path}`));
  if (first(/^POST \/api\/oauth\/device$/)) throw new Error("signed out by a restart");
  // From the app's Main, once brs-node itself has loaded.
  return { rows: first(/^GET \/api\/hubs\/home$/).at - main, picture: first(/images\//).at - main };
}

const failures = [];
let walked = null;
let started = null;
try {
  await walk();
} catch (error) {
  walked = error;
  shoot("failed");
}
brs.terminateApp("smoke done", 3000);
const result = await running;
if (result?.exitReason !== "smoke done") failures.push(`the app ended by itself: ${result?.exitReason}`);
if (!walked && registry) {
  try {
    started = await restart();
  } catch (error) {
    walked = error;
  }
}
fixture.close();

if (walked) failures.push(`the walk failed: ${walked.message}`);
const errors = messages.filter((m) => m.startsWith("error,"));
for (const error of errors) failures.push(`BrightScript: ${error.slice(6).trim()}`);
for (const problem of fixture.problems) failures.push(`server: ${problem}`);

const asked = (pattern) => fixture.requests.some((r) => pattern.test(`${r.method} ${r.path}`));
const checks = [
  ["asked for a TV code", asked(/^POST \/api\/oauth\/device$/)],
  ["showed the code's QR code", qr.shown],
  ["said the server requires two-step sign-in", twoStep.shown],
  ["polled with the device code", asked(/^POST \/api\/oauth\/token$/)],
  ["refreshed its token", fixture.refreshes > 0],
  // The fake server says so when a token goes before a proof.
  ["had the server prove itself before its token went", fixture.proofs > 0],
  ["loaded Home", asked(/^GET \/api\/hubs\/home$/)],
  ["asked to play with its capabilities", asked(/^POST \/api\/playback$/)],
  ["drew the PGS subtitles picked itself: nothing burned in, their list asked for where the playback starts", fixture.playbacks.some((p) => p.item === 103 && p.subtitle_index === null && p.keyed && p.draws_pgs) && fixture.picturesAsked.some((p) => p.session === "s2" && p.index === 4)],
  ["asked again for picture subtitles not whole yet", fixture.picturesAsked.filter((p) => p.session === "s2").length >= 2],
  ["loaded the picture subtitles drawn", pictures.loaded && fixture.picturesFetched.every((name) => /^subtitles-4-\d+\.png$/.test(name))],
  ["burned the PGS subtitles in where the server has no pictures of them, from then on, saying it doesn't draw them", fixture.playbacks[3]?.subtitle_index === 4 && fixture.playbacks[3]?.draws_pgs === false && !fixture.requests.some((r) => /^\/api\/media\/pley_mk_s4\/subtitles/.test(r.path))],
  ["found subtitles from the player's menu and added the first", fixture.subtitlesAdded.some((a) => a.item === 103 && a.result === "opensubtitles:1" && a.file_id === 7103)],
  ["looked for subtitles in a language picked above the results", asked(/^GET \/api\/items\/103\/subtitles\/search\?file_id=7103&language=ara$/)],
  ["saved the added subtitles as the pick", fixture.streams.some((s) => s.item === 103 && s.subtitle_index === 10001)],
  ["picked subtitles in the player's own menu, saved and played", menuTracks.picked],
  ["asked the server what it knows of the network before playing in Automatic", asked(/^GET \/api\/playback\/network\?client_id=/)],
  ["ran a speed test of 8 MiB where the server asked for one", fixture.speedTests.length > 0 && fixture.speedTests.every((n) => n === 8388608)],
  ["told the server what the speed test measured", fixture.networkTold.some((t) => t.probe_kbps > 0 && t.probe_complete === false && t.client_id)],
  ["kept the server's answer instead of asking before each play", fixture.requests.filter((r) => r.method === "GET" && r.path.startsWith("/api/playback/network")).length < fixture.playbacks.filter((p) => p.kind !== "track").length],
  ["played the original in Automatic, the default", fixture.playbacks.length > 0 && fixture.playbacks.filter((p) => p.kind !== "track").every((p) => p.mode === "auto" && p.max_bitrate === null)],
  ["chose Español for the account in Settings, Home asked for again in it, then Automatic", JSON.stringify(fixture.languagesSaved) === '["es",null]' && language.home],
  [
    "on a server without the language route, Français kept on the Roku, and Automatic in Settings opened again forgot it with no request",
    JSON.stringify(fixture.languagesRefused) === '["fr"]' && language.refusedHome && language.afterRefused === "en",
  ],
  ["kept 8 Mbps 1080p chosen in Settings", registry !== null && /playback.quality","8m"/.test(JSON.stringify([...registry]))],
  ["kept Anna as the profile to always start as", registry !== null && /startAs\.http[^"]*","1"/.test(JSON.stringify([...registry]))],
  ["showed subtitle sync", sync.label],
  ["showed Diagnostics and the open-source licences from Settings", pages.diagnostics && pages.licences],
  ["showed the playback's details", asked(/^GET \/api\/playback\/s\d+$/)],
  ["saved Movies after TV Shows, every library in the order", fixture.orders.some((ids) => JSON.stringify(ids) === "[2,1,3]")],
  ["went back to the server's order", fixture.orders.includes(null) && fixture.libraryOrder.join() === "1,2,3"],
  [
    "saved subtitles always, the languages left to the file and the audio",
    fixture.trackDefaultsSaved.some((d) => d.subtitle_mode === "always" && d.audio_language === null && d.subtitle_language === null),
  ],
  ["remembered the subtitles 0.3 s later", registry !== null && /offsets.*\|\d+\\",300\]/.test(JSON.stringify([...registry]))],
  ["closed a session that came after Back", fixture.closed.includes("s1")],
  ["closed the session it played", fixture.closed.length > 1],
  ["reported where it stopped", fixture.progress.some((p) => p.state === "stopped" && p.session_id)],
  // The fake server says so when a report has them wrong, or music's has them.
  ["said which audio and subtitles play in a video's reports", fixture.videoReports > 0],
  ["loaded a library's next page", asked(/^GET \/api\/libraries\/1\/items\?.*offset=70/)],
  ["showed the A to Z beside a library sorted by title, W in focus", library.letters],
  [
    "jumped to W's first title, its page loaded before the one between",
    library.jumped && fixture.requests.findIndex((r) => /^\/api\/libraries\/1\/items\?.*offset=140/.test(r.path)) < fixture.requests.findIndex((r) => /^\/api\/libraries\/1\/items\?.*offset=70/.test(r.path)),
  ],
  ["came back to Home asking again only for rows a minute old", home.kept],
  ["loaded a season's episodes", asked(/^GET \/api\/items\/100\/children$/)],
  ["showed a library's Recommended and Collections tabs", asked(/^GET \/api\/libraries\/2\/hubs$/) && asked(/^GET \/api\/libraries\/2\/collections$/)],
  ["opened a library from Home's See all, newest added first", asked(/^GET \/api\/libraries\/3\/items\?.*order=desc.*sort=added/)],
  [
    "kept nothing of See all's order: the library opened from the menu by title",
    seeAll.after >= 0 && /sort=title/.test(fixture.requests.slice(seeAll.after).find((r) => /^\/api\/libraries\/3\/items\?/.test(r.path))?.path ?? ""),
  ],
  ["showed a library's Library tab from its Recommended row's See all", asked(/^GET \/api\/libraries\/2\/items\?.*order=desc.*sort=added/)],
  ["sorted a library by year, newest first", asked(/^GET \/api\/libraries\/2\/items\?.*order=desc.*sort=year/)],
  ["kept the library's tab and sort on the Roku", registry !== null && /libraryViews.*\\"sort\\":\\"year\\"/.test(JSON.stringify([...registry]))],
  ["sorted a library by bitrate", asked(/^GET \/api\/libraries\/1\/items\?.*order=desc.*sort=bitrate/)],
  ["filtered a library by a resolution its titles have", asked(/^GET \/api\/libraries\/1\/filters$/) && asked(/^GET \/api\/libraries\/1\/items\?.*resolution=4k/)],
  ["kept the library's resolution on the Roku", registry !== null && /libraryViews.*\\"resolution\\":\\"4k\\"/.test(JSON.stringify([...registry]))],
  [
    "saved a library's filters and order as a smart playlist, named on the keyboard",
    fixture.smartPlaylists.some((p) => p.library_id === 2 && p.title.length > 0 && p.smart_filter === "unwatched=1&sort=year%3Adesc"),
  ],
  ["marked a title watched with the Options key", fixture.marks.some((m) => m.item === 103 && m.action === "watched")],
  ["rated a show 4 stars from its page's More", fixture.ratings.some((r) => r.item === 99 && r.rating === 8)],
  ["picked a movie's audio on its page, saved with the subtitles shown", fixture.streams.some((s) => s.item === 1011 && s.file_id === 8011 && s.audio_index === 2 && s.subtitle_index === 3)],
  ["turned its subtitles off on its page, the audio picked there kept", fixture.streams.some((s) => s.item === 1011 && s.audio_index === 2 && s.subtitle_index === null)],
  ["played the version chosen on its page", fixture.playbacks.some((p) => p.item === 1011 && p.file_id === 10011)],
  ["showed a movie's media info from its More", mediaInfo.shown],
  ["went to an episode's season from its More", season.opened],
  ["played a movie's trailer from its page, from the start", fixture.playbacks.some((p) => p.item === 4000 && p.start_ms === 0)],
  ["asked for a show's More Like This and collections", asked(/^GET \/api\/items\/99\/related\?limit=20$/) && asked(/^GET \/api\/items\/99\/collections$/)],
  ["opened a person's page", asked(/^GET \/api\/people\/7$/)],
  ["opened a title from a person's row of what they directed", person.opened],
  ["played a collection of movies from its first", collections.movies],
  ["played the next episode of a smart collection's show", collections.shows],
  ["moved a playlist's first entry down one", JSON.stringify(fixture.playlistOrders[0]) === JSON.stringify([2, 1, 3])],
  ["made a playlist for Movies & TV, named on the keyboard", fixture.playlistsAsked.some((a) => a.title === "Trip" && a.kind === "video")],
  ["took the entry moved out of the playlist", fixture.playlistRemoved[0] === 1],
  ["switched to another profile of the household", fixture.switched.includes(2)],
  ["showed the watch history", asked(/^GET \/api\/history\?/)],
  ["played every track of an artist, each album's asked for", artistPlayed.gathered && artistPlayed.first],
  ["played an album's tracks with the music player", fixture.playbacks.some((p) => p.item === 5201) && fixture.progress.some((p) => p.item >= 5201 && p.item <= 5203)],
  ["played the track after the one playing from Now Playing's Up next", music.upNext === 5202],
  ["shuffled Now Playing's queue, the track playing first", music.shuffled === 5202],
  ["put the album's order back with Stop shuffling", music.unshuffled === 5201],
  ["set Now Playing's sleep timer, its button saying what is left", music.sleepOff && music.sleepSet],
  ["stepped the sleep timer through End of this track back to Off", music.sleepEnd && music.sleepCycled],
  ["played the track OK was on under an album's second disc", music.disc === 5203],
  ["played a playlist from its top, the next entry to come", fixture.playbacks.some((p) => p.item === 101)],
  ["searched, asking for every title, person and channel found", asked(/^GET \/api\/search\?limit=200&/)],
  ["showed search's results in rows by kind, People last, a person's page opened from it", asked(/^GET \/api\/people\/11$/)],
  ["searched a recent search again from the list shown before typing", searched.again],
  ["kept the recent searches on the Roku for the server and profile", registry !== null && /searches\.http[^"]*\|1","\[\\"mo\\"\]"/.test(JSON.stringify([...registry]))],
  ["showed Live TV's guide, its programmes for the channels in view", fixture.liveGuides.length > 0 && fixture.liveGuides.every((g) => g.ids.length > 0 && g.ids.length <= 50)],
  ["listed channels a page at a time", fixture.liveBrowses.length > 0 && fixture.liveBrowses.every((b) => b.limit <= 50)],
  ["opened Live TV back at the channel watched", fixture.liveBrowses.some((b) => b.around === 3)],
  ["kept the last channel watched on the Roku for the server and profile", registry !== null && /liveViews\.http[^"]*\|1","\{[^}]*\\"channel\\":3\b/.test(JSON.stringify([...registry]))],
  ["watched channels with one client_id, so zapping holds one connection", fixture.liveWatches.length >= 2 && new Set(fixture.liveWatches.map((w) => w.client_id)).size === 1 && fixture.liveWatches[0].client_id !== null],
  ["asked for a channel's playlist before playing it", fixture.livePlaylists.length >= 2],
  ["put a channel in the favourites from the player's list", fixture.liveFavoriteChanges.some((c) => c.id === 2 && c.favorite)],
  ["listed another category's channels in the player's list", fixture.liveBrowses.some((b) => b.category === "favorites")],
  ["said every connection is in use", live.busy],
  ["took over the connection with Watch here instead", fixture.liveWatches.some((w) => w.id === 3 && w.take_over === 1)],
  ["left the channels it watched", fixture.liveLeft.length > 0 && fixture.liveSessions.size === 0],
  ["asked what requests let the profile do", asked(/^GET \/api\/requests\/summary$/)],
  ["asked at the start which requests had news", asked(/^GET \/api\/requests\?.*unseen=1/)],
  ["searched TMDB beside the libraries for what they lack", asked(/^GET \/api\/discover\/search\?.*missing=1/)],
  ["showed all of TMDB's results for the words searched", asked(/^GET \/api\/discover\/search\?q=mo&page=1$/)],
  ["showed Discover's rows", asked(/^GET \/api\/discover\/trending\?page=1$/) && asked(/^GET \/api\/discover\/movies\?page=1$/)],
  ["asked for TMDB's pictures through the server, at the size drawn", fixture.discoverPictures.length > 0 && fixture.discoverPictures.every((p) => /^\d+$/.test(p.w ?? ""))],
  ["asked for a movie once its question was answered", fixture.requestsAsked.some((a) => a.kind === "movie" && a.tmdb_id === 7001 && !("seasons" in a))],
  ["asked for a show's missing seasons, ticked to start with", fixture.requestsAsked.some((a) => a.kind === "show" && a.tmdb_id === 8004 && JSON.stringify(a.seasons) === "[1,2,3]")],
  ["approved a request from the Options key", fixture.requestsActed.some((a) => a.id === 1 && a.action === "approve")],
  ["marked the profile's news seen once its requests showed", fixture.requestsActed.some((a) => a.action === "seen")],
  ["kept a ready-to-watch notice from the requests, showing it in a card once they were left and the remote rested", notice.waited && notice.shown],
  ["told of a title ready to watch, which Open opened", notice.opened],
  ["asked a show's page what of it may still be asked for", asked(/^GET \/api\/items\/99\/request$/)],
  [
    "asked a server without requests for nothing of them but the summary",
    older.since >= 0 && fixture.requests.slice(older.since).every((r) => !/^\/api\/(discover|requests)([/?]|$)/.test(r.path) || r.path === "/api/requests/summary"),
  ],
  ["asked again with a new token after a 401", new Set(fixture.requests.filter((r) => r.path === "/api/items/103").map((r) => r.auth)).size > 1],
  ["sent the token in the header only", fixture.requests.every((r) => !/[?&]token=/.test(r.path))],
  ["signed out", asked(/^POST \/api\/oauth\/revoke$/)],
  ["left out a QR code the server doesn't draw", qr.hidden],
  [
    "asked for requests and approvals only where the server offers them",
    fixture.scopesAsked[0] === "library play playlists profiles requests approvals" && fixture.scopesAsked.at(-1) === "library play playlists profiles",
  ],
  ["said nothing of two-step sign-in on a server that doesn't", twoStep.hidden],
  ["started again signed in", started !== null],
];
for (const [what, ok] of checks) if (!ok) failures.push(`never ${what}`);

console.log(`${fixture.requests.length} requests to the fake server, ${checks.filter(([, ok]) => ok).length}/${checks.length} checks passed; screens in out/smoke/`);
if (started) console.log(`signed-in start in brs-node, from Main: Home's rows asked for after ${started.rows} ms, the first picture after ${started.picture} ms`);
if (failures.length > 0) {
  for (const failure of failures) console.log(`FAIL ${failure}`);
  if (process.argv.includes("--verbose")) {
    for (const r of fixture.requests) console.log(`  ${r.method} ${r.path} ${r.auth}`);
    for (const m of messages) console.log(`  engine: ${m.trim()}`);
  }
  process.exit(1);
}
console.log("smoke: passed");
process.exit(0);

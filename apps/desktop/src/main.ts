import "./style.css";
import { invoke } from "@tauri-apps/api/core";
import { coming, describe, fraction, serverName, type Download } from "./downloads";
import {
  candidates,
  displayAddress,
  lastUsed,
  mergeServers,
  unencrypted,
  type FoundServer,
  type SavedServer,
  type ServerEntry,
  type ServerInfo,
} from "./servers";

function element<T extends HTMLElement>(id: string): T {
  const el = document.getElementById(id);
  if (!el) throw new Error(`#${id} is missing`);
  return el as T;
}

const list = element<HTMLUListElement>("servers");
const status = element<HTMLParagraphElement>("status");
const searchAgain = element<HTMLButtonElement>("search-again");
const form = element<HTMLFormElement>("add");
const address = element<HTMLInputElement>("address");
const connect = element<HTMLButtonElement>("connect");
const addError = element<HTMLParagraphElement>("add-error");
const chooser = element<HTMLElement>("choose-section");
const connecting = element<HTMLElement>("connecting");
const connectingText = element<HTMLParagraphElement>("connecting-text");
const downloadsSection = element<HTMLElement>("downloads-section");
const downloadsList = element<HTMLUListElement>("downloads");
const downloadsStatus = element<HTMLParagraphElement>("downloads-status");

let saved: SavedServer[] = [];
let found: FoundServer[] = [];
let searching = false;
/** Whether each server answered lately (by address). */
const reachable = new Map<string, boolean>();
let busy: string | null = null;
let autoOpen: AbortController | null = null;
let downloads: Download[] = [];
/** The download whose Delete was pressed once: pressed again, it goes. */
let confirming: number | null = null;
let confirmTimer = 0;
let pollTimer = 0;
/** What the picker says once it has looked for servers. */
let note = "";

function message(err: unknown): string {
  return typeof err === "string" ? err : err instanceof Error ? err.message : String(err);
}

function render(): void {
  const entries = mergeServers(saved, found);
  list.replaceChildren(...entries.map(row));
  if (searching) status.textContent = "Looking for servers on this network…";
  else if (!entries.length) status.textContent = "No server answered. Check that Pley is running on it, or enter its address.";
  else status.textContent = note;
  searchAgain.hidden = searching;
}

function row(entry: ServerEntry): HTMLLIElement {
  const li = document.createElement("li");
  const open = document.createElement("button");
  open.type = "button";
  open.className = "server";
  open.disabled = busy !== null;
  const state = reachable.get(entry.url);
  const dot = document.createElement("span");
  dot.className = `dot${entry.found || state ? " online" : state === false ? " offline" : ""}`;
  const name = document.createElement("span");
  name.className = "name";
  name.textContent = entry.name;
  const where = document.createElement("span");
  where.className = "address";
  where.textContent = busy === entry.url ? "Connecting…" : displayAddress(entry.url);
  const tag = document.createElement("span");
  tag.className = entry.lookalike || unencrypted(entry.url) ? "tag warning" : "tag";
  tag.textContent = entry.lookalike
    ? "Not your saved server"
    : unencrypted(entry.url)
      ? "Not encrypted"
      : entry.saved
        ? entry.found
          ? "On this network"
          : ""
        : "New";
  open.append(dot, name, where, tag);
  open.addEventListener("click", () => void openServer(entry.url, entry.id));
  li.append(open);
  if (entry.saved) {
    const forget = document.createElement("button");
    forget.type = "button";
    forget.className = "forget";
    forget.textContent = "×";
    forget.title = "Forget this server";
    forget.setAttribute("aria-label", `Forget ${entry.name}`);
    forget.addEventListener("click", () => void forgetServer(entry.url));
    li.append(forget);
  }
  return li;
}

const SVG = "http://www.w3.org/2000/svg";

/** A small picture: filled, or drawn in lines. */
function glyph(d: string, filled: boolean): SVGSVGElement {
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("class", "glyph");
  svg.setAttribute("aria-hidden", "true");
  const path = document.createElementNS(SVG, "path");
  path.setAttribute("d", d);
  if (filled) {
    path.setAttribute("fill", "currentColor");
  } else {
    path.setAttribute("fill", "none");
    path.setAttribute("stroke", "currentColor");
    path.setAttribute("stroke-width", "2");
    path.setAttribute("stroke-linecap", "round");
    path.setAttribute("stroke-linejoin", "round");
  }
  svg.append(path);
  return svg;
}

const PLAY = "M8 5.14v13.72a1 1 0 0 0 1.52.85l10.6-6.86a1 1 0 0 0 0-1.7L9.52 4.3A1 1 0 0 0 8 5.14z";
const ARROW = "M12 4v11m-5-5 5 5 5-5M5 20h14";
const ALERT = "M12 8v5m0 3.5v.01M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z";

/** What a row shows besides its words and bar: rebuilt when it changes. */
const shapeOf = (d: Download) => `${d.state}:${confirming === d.id}`;

function act(command: string, id: number, failure: string): void {
  downloadsStatus.textContent = "";
  invoke(command, { id })
    .catch((err: unknown) => {
      downloadsStatus.textContent = `${failure}: ${message(err)}`;
    })
    .finally(() => void refreshDownloads());
}

function actionButton(text: string, onClick: () => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "action";
  button.textContent = text;
  button.addEventListener("click", onClick);
  return button;
}

function downloadRow(d: Download): HTMLLIElement {
  const li = document.createElement("li");
  li.className = `download ${d.state}`;
  li.dataset.id = String(d.id);
  li.dataset.shape = shapeOf(d);
  const playable = d.state === "done";
  const main = document.createElement(playable ? "button" : "div");
  main.className = playable ? "server playable" : "server";
  if (main instanceof HTMLButtonElement) {
    main.type = "button";
    main.title = "Play in your video player";
    main.addEventListener("click", () => act("play_download", d.id, "Couldn't play it"));
  }
  const name = document.createElement("span");
  name.className = "name";
  name.textContent = d.name;
  const line = document.createElement("span");
  line.className = "address";
  main.append(glyph(playable ? PLAY : d.state === "failed" ? ALERT : ARROW, playable), name, line);
  if (coming(d)) {
    const bar = document.createElement("div");
    bar.className = "bar";
    bar.append(document.createElement("span"));
    main.append(bar);
  }
  li.append(main);
  if (playable) li.append(actionButton("Show in folder", () => act("show_download", d.id, "Couldn't show it")));
  if (d.state === "failed") li.append(actionButton("Retry", () => act("retry_download", d.id, "Couldn't retry it")));
  const remove = document.createElement("button");
  remove.type = "button";
  const sure = confirming === d.id;
  remove.className = sure ? "forget confirm" : "forget";
  remove.textContent = sure ? "Delete" : "×";
  remove.title = playable ? "Delete the file" : "Stop and delete what came";
  remove.setAttribute("aria-label", sure ? `Delete ${d.name} for good` : `Delete ${d.name}`);
  remove.addEventListener("click", () => {
    window.clearTimeout(confirmTimer);
    if (confirming === d.id) {
      confirming = null;
      act("remove_download", d.id, "Couldn't delete it");
      return;
    }
    confirming = d.id;
    confirmTimer = window.setTimeout(() => {
      confirming = null;
      renderDownloads();
    }, 4000);
    renderDownloads();
    downloadsList.querySelector<HTMLButtonElement>(`li[data-id="${d.id}"] .forget`)?.focus();
  });
  li.append(remove);
  updateRow(li, d);
  return li;
}

/** The words and the bar of a row, as the download goes. */
function updateRow(li: HTMLLIElement, d: Download): void {
  const line = li.querySelector(".address");
  if (line) line.textContent = describe(d, serverName(d.server, saved));
  const fill = li.querySelector<HTMLElement>(".bar > span");
  if (fill) fill.style.width = `${(fraction(d) ?? 0) * 100}%`;
}

function renderDownloads(): void {
  downloadsSection.hidden = chooser.hidden || downloads.length === 0;
  const rows = downloads.map((d) => {
    const row = downloadsList.querySelector<HTMLLIElement>(`li[data-id="${d.id}"]`);
    if (row?.dataset.shape !== shapeOf(d)) return downloadRow(d);
    updateRow(row, d);
    return row;
  });
  // The same rows in the same order stay where they are (and keep focus).
  const same = rows.length === downloadsList.children.length && rows.every((row, i) => downloadsList.children[i] === row);
  if (!same) downloadsList.replaceChildren(...rows);
}

/** The app's downloads, again every second while some are coming. */
async function refreshDownloads(): Promise<void> {
  window.clearTimeout(pollTimer);
  try {
    downloads = await invoke<Download[]>("downloads");
  } catch (err) {
    downloadsStatus.textContent = message(err);
  }
  renderDownloads();
  if (downloads.some(coming)) pollTimer = window.setTimeout(() => void refreshDownloads(), 1000);
}

function notEncrypted(url: string): string {
  return `${displayAddress(url)} isn't encrypted: your password and what you watch could be read on the way. Open it by its https address.`;
}

/** Checks the server, remembers it and shows its web app in this window. */
async function openServer(url: string, id: string | null): Promise<void> {
  if (unencrypted(url)) {
    status.textContent = notEncrypted(url);
    return;
  }
  busy = url;
  render();
  try {
    const info = await invoke<ServerInfo>("check_server", { url });
    reachable.set(url, true);
    await invoke("save_server", { server: { url, name: info.name, id } });
    await invoke("open_server", { url });
  } catch (err) {
    reachable.set(url, false);
    busy = null;
    render();
    status.textContent = `Couldn't reach ${displayAddress(url)}: ${message(err)}`;
  }
}

async function forgetServer(url: string): Promise<void> {
  await invoke("forget_server", { url });
  saved = saved.filter((s) => s.url !== url);
  render();
}

async function search(): Promise<void> {
  searching = true;
  render();
  const checks = saved.map(async (s) => {
    try {
      await invoke<ServerInfo>("check_server", { url: s.url });
      reachable.set(s.url, true);
    } catch {
      reachable.set(s.url, false);
    }
  });
  try {
    found = await invoke<FoundServer[]>("discover");
  } catch (err) {
    status.textContent = message(err);
  }
  await Promise.all(checks);
  searching = false;
  render();
}

function showChooser(): void {
  autoOpen?.abort();
  connecting.hidden = true;
  chooser.hidden = false;
  renderDownloads();
  void search();
}

/** At start, the server opened last opens by itself if it answers. */
async function openLastUsed(server: SavedServer): Promise<void> {
  const cancelled = new AbortController();
  autoOpen = cancelled;
  chooser.hidden = true;
  renderDownloads();
  connecting.hidden = false;
  connectingText.textContent = `Opening ${server.name}…`;
  try {
    const info = await invoke<ServerInfo>("check_server", { url: server.url });
    if (cancelled.signal.aborted) return;
    await invoke("save_server", { server: { ...server, name: info.name } });
    await invoke("open_server", { url: server.url });
  } catch (err) {
    if (cancelled.signal.aborted) return;
    reachable.set(server.url, false);
    showChooser();
    status.textContent = `${server.name} didn't answer: ${message(err)}`;
  }
}

/** The first of `urls` where a pley server answers (https before http for names on the internet). */
async function firstAnswering(urls: string[]): Promise<string> {
  let failure: unknown = null;
  for (const url of urls) {
    try {
      await invoke<ServerInfo>("check_server", { url });
      return url;
    } catch (err) {
      failure ??= err;
    }
  }
  throw failure;
}

async function addServer(input: string): Promise<void> {
  const urls = candidates(input);
  if (!urls.length) {
    addError.textContent = "Enter an address such as nas, 192.168.1.20 or https://pley.example.com.";
    addError.hidden = false;
    return;
  }
  let url: string;
  try {
    url = await firstAnswering(urls);
  } catch (err) {
    addError.textContent = `Couldn't reach ${displayAddress(urls[urls.length - 1] ?? input)}: ${message(err)}`;
    addError.hidden = false;
    return;
  }
  // Plain http across the internet: refused, as Pley's other apps do. The
  // session would travel readable, and the server proves itself to plain
  // http on its own networks only.
  if (unencrypted(url)) {
    addError.textContent = notEncrypted(url);
    addError.hidden = false;
    return;
  }
  await openServer(url, null);
}

form.addEventListener("submit", (event) => {
  event.preventDefault();
  addError.hidden = true;
  connect.disabled = true;
  void addServer(address.value).finally(() => {
    connect.disabled = false;
  });
});
searchAgain.addEventListener("click", () => void search());
element<HTMLButtonElement>("choose").addEventListener("click", showChooser);

async function start(): Promise<void> {
  saved = await invoke<SavedServer[]>("saved_servers");
  void refreshDownloads();
  // "Switch server" in the web app comes back here with ?pick: no opening by itself then.
  const params = new URLSearchParams(window.location.search);
  const pick = params.has("pick");
  // The app left a server whose address now leads to another device (a
  // saved one: a server's page could open the picker with any address).
  const notYours = saved.find((s) => s.url === params.get("not-yours")?.replace(/\/$/, ""));
  if (notYours) note = `Another device answers at ${displayAddress(notYours.url)} now, so Pley came back here.`;
  const last = lastUsed(saved);
  // A plain-http address across the internet isn't opened (see addServer).
  if (last && !pick && !unencrypted(last.url)) void openLastUsed(last);
  else showChooser();
}

void start();

/**
 * WebVTT as the server writes it (`/api/files/{id}/subtitles/{n}.vtt`):
 * cues with their times, text with `<i>`, `<b>`, `<u>`, and the settings
 * that place a cue (`line`, `position`, `align`, `size`).
 */

export interface Cue {
  startMs: number;
  endMs: number;
  /** The text as safe HTML: only `<i>`, `<b>`, `<u>` and `<br>` survive. */
  html: string;
  /** A line number (`snapToLines`) or a percentage from the top; `null`: the last line. */
  line: number | null;
  snapToLines: boolean;
  /** Percentage of the width the cue's box is placed at; `null`: centered. */
  position: number | null;
  align: "start" | "center" | "end" | "left" | "right";
  /** Percentage of the width the cue's box takes; `null`: 100. */
  size: number | null;
}

const TIMING = /^\s*((?:\d+:)?\d{1,2}:\d{2}[.,]\d{1,3})\s+-->\s+((?:\d+:)?\d{1,2}:\d{2}[.,]\d{1,3})(.*)$/;

/** `"01:02:03.456"` (or `"02:03,456"`) → milliseconds. */
export function timeMs(time: string): number {
  const parts = time.replace(",", ".").split(":");
  let seconds = 0;
  for (const p of parts) seconds = seconds * 60 + Number(p);
  return Math.round(seconds * 1000);
}

export function parseVtt(text: string): Cue[] {
  const cues: Cue[] = [];
  const blocks = text.replace(/^﻿/, "").replace(/\r\n?/g, "\n").split(/\n{2,}/);
  for (const block of blocks) {
    const lines = block.split("\n");
    let at = -1;
    for (let i = 0; i < lines.length; i++) {
      if (lines[i]!.indexOf("-->") >= 0) {
        at = i;
        break;
      }
    }
    if (at < 0) continue;
    const timing = TIMING.exec(lines[at]!);
    if (!timing) continue;
    const body = lines.slice(at + 1).join("\n").trim();
    if (!body) continue;
    const startMs = timeMs(timing[1]!);
    const endMs = timeMs(timing[2]!);
    if (!(endMs > startMs)) continue;
    cues.push(withSettings({ startMs, endMs, html: cueHtml(body), line: null, snapToLines: true, position: null, align: "center", size: null }, timing[3] ?? ""));
  }
  cues.sort((a, b) => a.startMs - b.startMs || a.endMs - b.endMs);
  return cues;
}

function withSettings(cue: Cue, settings: string): Cue {
  for (const setting of settings.trim().split(/\s+/)) {
    const [name, raw] = setting.split(":");
    if (!raw) continue;
    const value = raw.split(",")[0]!;
    switch (name) {
      case "line": {
        const m = /^(-?\d+(?:\.\d+)?)(%)?$/.exec(value);
        if (m) {
          cue.line = Number(m[1]);
          cue.snapToLines = !m[2];
        }
        break;
      }
      case "position": {
        const m = /^(\d+(?:\.\d+)?)%$/.exec(value);
        if (m) cue.position = Number(m[1]);
        break;
      }
      case "size": {
        const m = /^(\d+(?:\.\d+)?)%$/.exec(value);
        if (m) cue.size = Number(m[1]);
        break;
      }
      case "align":
        if (value === "start" || value === "center" || value === "end" || value === "left" || value === "right") cue.align = value;
        else if (value === "middle") cue.align = "center";
        break;
    }
  }
  return cue;
}

const ENTITIES: Record<string, string> = { amp: "&", lt: "<", gt: ">", nbsp: " ", lrm: "‎", rlm: "‏", quot: '"', apos: "'" };

function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

/**
 * The cue's text as HTML that is safe to insert: entities decoded then
 * escaped again, `<i>`, `<b>`, `<u>` kept (closed when the cue forgets
 * to), voice and class spans and timestamps dropped, lines as `<br>`.
 */
export function cueHtml(body: string): string {
  let out = "";
  const open: string[] = [];
  const re = /<(\/?)([a-zA-Z]+|\d[\d:.]*)[^>]*>|&(#x[0-9a-fA-F]+|#\d+|[a-zA-Z]+);|\n|[^<&\n]+|[<&]/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(body))) {
    const token = m[0];
    if (token === "\n") {
      out += "<br>";
    } else if (m[2] !== undefined) {
      const tag = m[2].toLowerCase();
      if (tag === "i" || tag === "b" || tag === "u") {
        if (m[1]) {
          const at = open.lastIndexOf(tag);
          if (at >= 0) {
            // Close what was opened after it, then reopen it.
            const reopen = open.splice(at);
            for (let i = reopen.length - 1; i >= 0; i--) out += `</${reopen[i]}>`;
            for (let i = 1; i < reopen.length; i++) {
              out += `<${reopen[i]}>`;
              open.push(reopen[i]!);
            }
          }
        } else {
          out += `<${tag}>`;
          open.push(tag);
        }
      }
    } else if (m[3] !== undefined) {
      const name = m[3];
      let ch: string | undefined;
      if (name[0] === "#") {
        const code = name[1] === "x" || name[1] === "X" ? parseInt(name.slice(2), 16) : parseInt(name.slice(1), 10);
        ch = code > 0 && code < 0x110000 ? String.fromCodePoint(code) : undefined;
      } else {
        ch = ENTITIES[name.toLowerCase()];
      }
      out += escapeHtml(ch ?? token);
    } else {
      out += escapeHtml(token);
    }
  }
  for (let i = open.length - 1; i >= 0; i--) out += `</${open[i]}>`;
  return out;
}

/**
 * Two answers for the same track: the one that reaches further (a later
 * last end, or as far with more cues). A partial answer never removes
 * cues an earlier one had.
 */
export function mergeCues(old: Cue[] | null, fresh: Cue[]): Cue[] {
  if (!old) return fresh;
  const end = (cues: Cue[]) => {
    let last = 0;
    for (const c of cues) if (c.endMs > last) last = c.endMs;
    return last;
  };
  const oldEnd = end(old);
  const freshEnd = end(fresh);
  if (freshEnd > oldEnd || (freshEnd === oldEnd && fresh.length > old.length)) return fresh;
  return old;
}

/** Cues of a track, for fast lookups while playing. */
export class CueIndex {
  private readonly cues: Cue[];
  /** The longest cue: how far back a lookup has to look. */
  private readonly longest: number;

  constructor(cues: Cue[]) {
    this.cues = cues.slice().sort((a, b) => a.startMs - b.startMs);
    let longest = 0;
    for (const c of this.cues) longest = Math.max(longest, c.endMs - c.startMs);
    this.longest = longest;
  }

  get size(): number {
    return this.cues.length;
  }

  /** The cues showing at `ms`, earliest first. */
  at(ms: number): Cue[] {
    const out: Cue[] = [];
    // The first cue that may still show: started at most `longest` ago.
    let i = this.firstStartingAtOrAfter(ms - this.longest);
    for (; i < this.cues.length; i++) {
      const c = this.cues[i]!;
      if (c.startMs > ms) break;
      if (ms < c.endMs) out.push(c);
    }
    return out;
  }

  /** When the cues showing change after `ms`: the next start or end; `null` when nothing changes any more. */
  nextChange(ms: number): number | null {
    let next: number | null = null;
    let i = this.firstStartingAtOrAfter(ms - this.longest);
    for (; i < this.cues.length; i++) {
      const c = this.cues[i]!;
      if (c.startMs > ms) {
        if (next === null || c.startMs < next) next = c.startMs;
        break;
      }
      if (c.endMs > ms && (next === null || c.endMs < next)) next = c.endMs;
    }
    return next;
  }

  private firstStartingAtOrAfter(ms: number): number {
    let lo = 0;
    let hi = this.cues.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (this.cues[mid]!.startMs < ms) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }
}

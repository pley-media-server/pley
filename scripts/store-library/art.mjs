// Draws the screenshot library's artwork (posters, backdrops, season posters, episode stills)
// through a headless Chrome, into <out>/art/. Usage: bun art.mjs <out>
import { spawn } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { movies, shows } from "./catalog.mjs";

const out = process.argv[2];
mkdirSync(join(out, "art"), { recursive: true });

function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
const hash = (s) => [...s].reduce((h, c) => (Math.imul(h, 31) + c.charCodeAt(0)) | 0, 7);

// Color helpers.
const hex = (c) => [1, 3, 5].map((i) => parseInt(c.slice(i, i + 2), 16));
const toHex = (v) => "#" + v.map((x) => Math.max(0, Math.min(255, Math.round(x))).toString(16).padStart(2, "0")).join("");
const mix = (a, b, t) => toHex(hex(a).map((x, i) => x + (hex(b)[i] - x) * t));

function ridge(w, baseY, amp, r, step, color, opacity = 1) {
  let d = `M0 ${baseY}`;
  for (let x = 0; x <= w + step; x += step) d += ` L${x} ${baseY - amp * (0.3 + 0.7 * r())}`;
  return `<path d="${d} L${w + step} 99999 L0 99999Z" fill="${color}" opacity="${opacity}"/>`;
}
function hill(w, baseY, amp, phase, freq, color, opacity = 1) {
  let d = `M0 ${baseY}`;
  for (let x = 0; x <= w; x += w / 60) d += ` L${x.toFixed(1)} ${(baseY - amp * Math.sin(phase + (x / w) * Math.PI * freq)).toFixed(1)}`;
  return `<path d="${d} L${w} 99999 L0 99999Z" fill="${color}" opacity="${opacity}"/>`;
}
function stars(w, h, n, r, maxY, color = "#fff") {
  let s = "";
  for (let i = 0; i < n; i++) s += `<circle cx="${(r() * w).toFixed(1)}" cy="${(r() * maxY).toFixed(1)}" r="${(0.4 + r() * 1.4) * (w / 1000)}" fill="${color}" opacity="${(0.3 + r() * 0.7).toFixed(2)}"/>`;
  return s;
}
const glow = (id, color) => `<radialGradient id="${id}"><stop offset="0" stop-color="${color}" stop-opacity="0.95"/><stop offset="0.35" stop-color="${color}" stop-opacity="0.35"/><stop offset="1" stop-color="${color}" stop-opacity="0"/></radialGradient>`;
const sky = (id, stops) => `<linearGradient id="${id}" x1="0" y1="0" x2="0" y2="1">${stops.map(([o, c]) => `<stop offset="${o}" stop-color="${c}"/>`).join("")}</linearGradient>`;

/** A scene, w × h, from palette [p0 darkest .. p3 lightest]; [v] varies it (episode stills). */
function scene(kind, w, h, pal, seed, v = 0) {
  const r = rng(seed);
  const [p0, p1, p2, p3] = pal;
  const u = w / 1000;
  const defs = [];
  const body = [];
  const H = (f) => h * f;
  const dusk = v % 3; // 0 the poster's light, 1 later, 2 earlier
  const shade = (c) => (dusk === 1 ? mix(c, "#000000", 0.25) : dusk === 2 ? mix(c, "#ffffff", 0.08) : c);
  switch (kind) {
    case "aurora": {
      defs.push(sky("s", [[0, p0], [0.7, p1], [1, shade(mix(p1, p2, 0.25))]]), `<filter id="b"><feGaussianBlur stdDeviation="${28 * u}"/></filter>`);
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, stars(w, h, 220, r, H(0.7)));
      // Curtains: rays hanging from a wavy line, bright at the foot and fading upwards.
      defs.push(`<linearGradient id="ray" x1="0" y1="1" x2="0" y2="0"><stop offset="0" stop-color="${p2}" stop-opacity="0.9"/><stop offset="0.35" stop-color="${p2}" stop-opacity="0.45"/><stop offset="1" stop-color="${p3}" stop-opacity="0"/></linearGradient>`, `<filter id="soft"><feGaussianBlur stdDeviation="${6 * u}"/></filter>`);
      for (let band = 0; band < 2; band++) {
        const phase = r() * 6, amp = H(0.06 + band * 0.03), foot = H(0.42 + band * 0.1), tall = H(0.22 + band * 0.06);
        for (let x = -20 * u; x < w + 20 * u; x += 9 * u) {
          const y = foot - amp * Math.sin(phase + (x / w) * Math.PI * (1.6 + band));
          const t = tall * (0.6 + 0.4 * Math.sin(x / (37 * u) + band) * Math.sin(x / (91 * u)));
          body.push(`<rect x="${x}" y="${y - t}" width="${11 * u}" height="${t}" fill="url(#ray)" opacity="${(0.35 + 0.35 * r()) * (band ? 0.7 : 1)}" filter="url(#soft)"/>`);
        }
      }
      body.push(`<ellipse cx="${w * 0.5}" cy="${H(0.45)}" rx="${w * 0.7}" ry="${H(0.12)}" fill="${p2}" opacity="0.12" filter="url(#b)"/>`);
      body.push(ridge(w, H(0.72), H(0.16), r, w / 14, mix(p0, p1, 0.6)), ridge(w, H(0.8), H(0.12), r, w / 10, mix(p0, p1, 0.3)), ridge(w, H(0.9), H(0.08), r, w / 7, p0));
      break;
    }
    case "lighthouse": {
      defs.push(sky("s", [[0, shade(p1)], [0.55, shade(mix(p2, p3, 0.4))], [0.62, p3]]), glow("g", p3), sky("sea", [[0, mix(p1, p2, 0.4)], [1, p0]]));
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, `<circle cx="${w * 0.3}" cy="${H(0.6)}" r="${w * 0.45}" fill="url(#g)" opacity="0.7"/>`);
      body.push(`<rect y="${H(0.62)}" width="${w}" height="${H(0.38)}" fill="url(#sea)"/>`);
      for (let i = 0; i < 26; i++) body.push(`<rect x="${r() * w}" y="${H(0.63 + r() * 0.3)}" width="${(40 + r() * 160) * u}" height="${2 * u}" fill="${p3}" opacity="${0.15 + r() * 0.3}"/>`);
      const lx = w * 0.7, ly = H(0.38);
      body.push(`<path d="M${w * 0.5} ${H(0.7)} Q${w * 0.62} ${H(0.56)} ${w * 0.78} ${H(0.55)} T${w * 1.05} ${H(0.6)} L${w * 1.05} ${h} L${w * 0.45} ${h}Z" fill="${p0}"/>`);
      body.push(`<path d="M${lx - 30 * u} ${H(0.56)} L${lx - 18 * u} ${ly} L${lx + 18 * u} ${ly} L${lx + 30 * u} ${H(0.56)}Z" fill="${mix(p0, "#ffffff", 0.85)}"/>`);
      for (let k = 0; k < 3; k++) body.push(`<rect x="${lx - 27 * u}" y="${ly + (k + 1) * H(0.045)}" width="${54 * u}" height="${H(0.018)}" fill="#b91c1c" opacity="0.85"/>`);
      body.push(`<rect x="${lx - 22 * u}" y="${ly - 40 * u}" width="${44 * u}" height="${40 * u}" fill="${p3}"/>`, `<path d="M${lx - 28 * u} ${ly - 40 * u} L${lx} ${ly - 70 * u} L${lx + 28 * u} ${ly - 40 * u}Z" fill="${p0}"/>`);
      body.push(`<path d="M${lx} ${ly - 20 * u} L${-w * 0.2} ${ly - H(0.12)} L${-w * 0.2} ${ly + H(0.06)}Z" fill="${p3}" opacity="${dusk === 2 ? 0.08 : 0.22}"/>`, `<circle cx="${lx}" cy="${ly - 20 * u}" r="${90 * u}" fill="url(#g)"/>`);
      break;
    }
    case "city": {
      defs.push(sky("s", [[0, p0], [0.75, shade(p1)], [1, mix(p1, p2, 0.5)]]), glow("g", p2), glow("g2", p3));
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, `<circle cx="${w * 0.25}" cy="${H(0.55)}" r="${w * 0.5}" fill="url(#g)" opacity="0.6"/>`, `<circle cx="${w * 0.85}" cy="${H(0.65)}" r="${w * 0.4}" fill="url(#g2)" opacity="0.45"/>`);
      for (let layer = 0; layer < 3; layer++) {
        let x = -20 * u;
        const tone = mix(p0, p1, 0.5 - layer * 0.22);
        while (x < w) {
          const bw = (50 + r() * 110) * u, bh = H(0.18 + r() * (0.42 - layer * 0.08));
          const top = H(0.82 + layer * 0.04) - bh;
          body.push(`<rect x="${x}" y="${top}" width="${bw}" height="${h}" fill="${tone}"/>`);
          for (let wy = top + 12 * u; wy < H(0.9); wy += 18 * u) for (let wx = x + 8 * u; wx < x + bw - 10 * u; wx += 16 * u) if (r() < 0.22) body.push(`<rect x="${wx}" y="${wy}" width="${6 * u}" height="${8 * u}" fill="${r() < 0.5 ? p2 : p3}" opacity="${0.4 + layer * 0.2}"/>`);
          x += bw + r() * 12 * u;
        }
      }
      for (let i = 0; i < 160; i++) { const x = r() * w, y = r() * h; body.push(`<line x1="${x}" y1="${y}" x2="${x - 8 * u}" y2="${y + 40 * u}" stroke="${p3}" stroke-width="${1.2 * u}" opacity="0.25"/>`); }
      break;
    }
    case "waves": {
      defs.push(sky("s", [[0, shade(p3)], [0.5, shade(mix(p2, p3, 0.5))]]), glow("g", "#ffffff"));
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, `<circle cx="${w * 0.7}" cy="${H(0.3)}" r="${w * 0.25}" fill="url(#g)"/>`);
      for (let i = 0; i < 6; i++) body.push(hill(w, H(0.5 + i * 0.09), H(0.025 + i * 0.006), r() * 6, 3 + i, mix(p2, p0, i / 5)));
      for (let i = 0; i < 40; i++) body.push(`<ellipse cx="${r() * w}" cy="${H(0.55 + r() * 0.4)}" rx="${(20 + r() * 60) * u}" ry="${2 * u}" fill="#fff" opacity="${0.2 + r() * 0.3}"/>`);
      break;
    }
    case "dunes": {
      defs.push(sky("s", [[0, shade(p2)], [0.6, shade(p3)]]), glow("g", "#fff7ed"));
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, `<circle cx="${w * 0.6}" cy="${H(0.42)}" r="${w * 0.17}" fill="${p3}"/>`, `<circle cx="${w * 0.6}" cy="${H(0.42)}" r="${w * 0.45}" fill="url(#g)" opacity="0.5"/>`);
      for (let i = 0; i < 4; i++) body.push(hill(w, H(0.58 + i * 0.1), H(0.06 + i * 0.02), r() * 6, 1.2 + r(), mix(p2, p0, 0.25 + i * 0.25)));
      break;
    }
    case "paper": {
      defs.push(sky("s", [[0, p0], [1, shade(p1)]]), `<filter id="sh"><feDropShadow dx="0" dy="${-6 * u}" stdDeviation="${8 * u}" flood-color="#000" flood-opacity="0.4"/></filter>`);
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, stars(w, h, 80, r, H(0.5)), `<circle cx="${w * 0.5}" cy="${H(0.32)}" r="${w * 0.2}" fill="${p3}"/>`);
      for (let i = 0; i < 5; i++) body.push(`<g filter="url(#sh)">${hill(w, H(0.52 + i * 0.1), H(0.05 + i * 0.01), r() * 6, 1.5 + r() * 1.5, mix(p2, p0, 0.15 + i * 0.2))}</g>`);
      break;
    }
    case "tower": {
      defs.push(sky("s", [[0, p0], [1, shade(p1)]]), `<filter id="b"><feGaussianBlur stdDeviation="${40 * u}"/></filter>`);
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, `<path d="M${-w * 0.1} ${H(0.65)} Q${w * 0.4} ${H(0.2)} ${w * 1.1} ${H(0.05)}" stroke="${p2}" stroke-width="${180 * u}" fill="none" opacity="0.35" filter="url(#b)"/>`, stars(w, h, 420, r, H(0.85)));
      const cx = w * 0.5, base = H(0.82);
      body.push(ridge(w, H(0.84), H(0.05), r, w / 12, p0));
      body.push(`<g stroke="${p0}" stroke-width="${6 * u}" fill="none"><path d="M${cx - 90 * u} ${base} L${cx} ${H(0.3)} L${cx + 90 * u} ${base}"/>`);
      for (let y = H(0.35); y < base; y += H(0.05)) { const k = (y - H(0.3)) / (base - H(0.3)); body.push(`<path d="M${cx - 90 * u * k} ${y} L${cx + 90 * u * k} ${y + H(0.05)} M${cx + 90 * u * k} ${y} L${cx - 90 * u * k} ${y + H(0.05)}"/>`); }
      body.push(`</g><circle cx="${cx}" cy="${H(0.3)}" r="${7 * u}" fill="#ef4444"/><circle cx="${cx}" cy="${H(0.3)}" r="${30 * u}" fill="#ef4444" opacity="0.25"/>`);
      for (let k = 1; k <= 3; k++) body.push(`<circle cx="${cx}" cy="${H(0.3)}" r="${k * 70 * u}" stroke="${p3}" stroke-width="${2 * u}" fill="none" opacity="${0.5 - k * 0.12}"/>`);
      break;
    }
    case "fields": {
      defs.push(sky("s", [[0, shade(p1)], [0.45, shade(p2)], [0.62, p3]]), glow("g", p3));
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, `<circle cx="${w * 0.45}" cy="${H(0.6)}" r="${w * 0.6}" fill="url(#g)" opacity="0.7"/>`, `<circle cx="${w * 0.45}" cy="${H(0.6)}" r="${w * 0.09}" fill="#fff7ed"/>`);
      body.push(`<rect y="${H(0.62)}" width="${w}" height="${H(0.38)}" fill="${mix(p0, p1, 0.4)}"/>`);
      for (let i = -12; i <= 12; i++) body.push(`<path d="M${w * 0.45} ${H(0.62)} L${w * 0.45 + i * w * 0.16} ${h}" stroke="${p2}" stroke-width="${3 * u}" opacity="0.3"/>`);
      for (let i = 0; i < 9; i++) { const x = r() * w, s = (30 + r() * 40) * u; body.push(`<circle cx="${x}" cy="${H(0.6) - s * 0.6}" r="${s}" fill="${p0}"/><rect x="${x - 4 * u}" y="${H(0.6) - s * 0.6}" width="${8 * u}" height="${s * 1.2}" fill="${p0}"/>`); }
      break;
    }
    case "underwater": {
      defs.push(sky("s", [[0, shade(p3)], [0.25, p2], [1, p0]]), `<linearGradient id="ray" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#fff" stop-opacity="0.35"/><stop offset="1" stop-color="#fff" stop-opacity="0"/></linearGradient>`);
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`);
      for (let i = 0; i < 9; i++) { const x = r() * w; body.push(`<path d="M${x} 0 L${x + 40 * u} 0 L${x + 200 * u + r() * 200 * u} ${h} L${x + 80 * u} ${h}Z" fill="url(#ray)"/>`); }
      for (let i = 0; i < 40; i++) body.push(`<circle cx="${r() * w}" cy="${r() * h}" r="${(2 + r() * 8) * u}" fill="none" stroke="#fff" stroke-width="${1.5 * u}" opacity="${0.2 + r() * 0.4}"/>`);
      for (let i = 0; i < 7; i++) { const x = r() * w, y = H(0.3 + r() * 0.35), s = (14 + r() * 26) * u; body.push(`<path d="M${x} ${y} q${s} ${-s * 0.6} ${s * 2} 0 q${-s} ${s * 0.6} ${-s * 2} 0 M${x + s * 2} ${y} l${s * 0.7} ${-s * 0.5} l0 ${s}z" fill="${p0}" opacity="0.7"/>`); }
      body.push(ridge(w, H(0.92), H(0.1), r, w / 25, p0));
      break;
    }
    case "train": {
      defs.push(sky("s", [[0, p0], [1, shade(p1)]]), glow("g", p3));
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, stars(w, h, 200, r, H(0.6)), `<circle cx="${w * 0.75}" cy="${H(0.22)}" r="${w * 0.25}" fill="url(#g)" opacity="0.5"/>`, `<circle cx="${w * 0.75}" cy="${H(0.22)}" r="${w * 0.07}" fill="${p3}"/>`);
      body.push(ridge(w, H(0.6), H(0.2), r, w / 9, mix(p0, p1, 0.5)));
      const deck = H(0.66);
      body.push(`<rect y="${deck}" width="${w}" height="${14 * u}" fill="${p0}"/>`);
      for (let x = 0; x < w; x += 120 * u) body.push(`<path d="M${x} ${deck} L${x + 120 * u} ${deck} L${x + 120 * u} ${h} L${x + 100 * u} ${h} Q${x + 60 * u} ${deck + 60 * u} ${x + 20 * u} ${h} L${x} ${h}Z" fill="${p0}"/>`);
      const ty = deck - 46 * u;
      for (let c = 0; c < 6; c++) {
        const x = w * 0.08 + c * 150 * u;
        body.push(`<rect x="${x}" y="${ty}" width="${142 * u}" height="${44 * u}" rx="${6 * u}" fill="${p0}"/>`);
        if (c > 0) for (let k = 0; k < 5; k++) body.push(`<rect x="${x + 10 * u + k * 26 * u}" y="${ty + 10 * u}" width="${18 * u}" height="${14 * u}" fill="${p2}" opacity="${0.6 + r() * 0.4}"/>`);
      }
      body.push(`<path d="M${w * 0.08 + 142 * u * 6 + 50 * u} ${ty + 20 * u} L${w * 1.2} ${ty - 30 * u} L${w * 1.2} ${ty + 70 * u}Z" fill="${p2}" opacity="0.15"/>`);
      body.push(ridge(w, H(0.98), H(0.1), r, w / 20, p0));
      break;
    }
    case "flowers": {
      defs.push(sky("s", [[0, shade(mix(p3, "#bae6fd", 0.6))], [0.55, shade(p3)]]), glow("g", "#ffffff"));
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, `<circle cx="${w * 0.25}" cy="${H(0.2)}" r="${w * 0.35}" fill="url(#g)" opacity="0.8"/>`);
      body.push(hill(w, H(0.55), H(0.05), 1, 1.3, mix(p1, p3, 0.4)), hill(w, H(0.65), H(0.06), 3, 1.8, p1), hill(w, H(0.78), H(0.05), 5, 1.1, mix(p0, p1, 0.5)));
      for (let i = 0; i < 520; i++) { const y = H(0.6 + r() * 0.4), s = (2 + ((y / h) - 0.6) * 30 * r()) * u; body.push(`<circle cx="${r() * w}" cy="${y}" r="${s}" fill="${r() < 0.7 ? p2 : r() < 0.5 ? "#ffffff" : "#facc15"}" opacity="${0.75 + r() * 0.25}"/>`); }
      for (let i = 0; i < 12; i++) body.push(`<circle cx="${r() * w}" cy="${r() * H(0.5)}" r="${(10 + r() * 30) * u}" fill="#fff" opacity="${0.08 + r() * 0.1}"/>`);
      break;
    }
    case "planet": {
      defs.push(sky("s", [[0, p0], [1, mix(p0, p1, 0.6)]]), glow("g", p1), `<radialGradient id="pl" cx="0.35" cy="0.3" r="0.8"><stop offset="0" stop-color="${p3}"/><stop offset="0.5" stop-color="${p2}"/><stop offset="1" stop-color="${p0}"/></radialGradient>`);
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, `<circle cx="${w * 0.2}" cy="${H(0.25)}" r="${w * 0.6}" fill="url(#g)" opacity="0.6"/>`, stars(w, h, 380, r, h));
      const cx = w * (0.55 + (v % 2) * 0.1), cy = H(0.45), pr = Math.min(w, h) * 0.3;
      body.push(`<ellipse cx="${cx}" cy="${cy}" rx="${pr * 1.9}" ry="${pr * 0.42}" fill="none" stroke="${p3}" stroke-width="${pr * 0.12}" opacity="0.35" transform="rotate(-14 ${cx} ${cy})"/>`);
      body.push(`<circle cx="${cx}" cy="${cy}" r="${pr}" fill="url(#pl)"/>`);
      body.push(`<path d="M${cx - pr * 1.9} ${cy} A${pr * 1.9} ${pr * 0.42} 0 0 0 ${cx + pr * 1.9} ${cy}" fill="none" stroke="${p3}" stroke-width="${pr * 0.12}" opacity="0.6" transform="rotate(-14 ${cx} ${cy})"/>`);
      body.push(`<circle cx="${w * 0.15}" cy="${H(0.8)}" r="${pr * 0.12}" fill="${mix(p1, p3, 0.5)}"/>`);
      break;
    }
    case "harbor": {
      defs.push(sky("s", [[0, shade(p1)], [0.5, shade(mix(p1, p2, 0.7))], [0.58, p3]]), glow("g", p2), sky("sea", [[0, mix(p1, p0, 0.3)], [1, p0]]));
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, `<circle cx="${w * 0.7}" cy="${H(0.56)}" r="${w * 0.5}" fill="url(#g)" opacity="0.55"/>`);
      const quay = H(0.58);
      // Houses on the quay, windows lit.
      let x = -10 * u;
      while (x < w) {
        const hw = (60 + r() * 70) * u, hh = (70 + r() * 80) * u;
        body.push(`<path d="M${x} ${quay} L${x} ${quay - hh} L${x + hw / 2} ${quay - hh - 34 * u} L${x + hw} ${quay - hh} L${x + hw} ${quay}Z" fill="${mix(p0, p1, 0.35)}"/>`);
        for (let k = 0; k < 2; k++) if (r() < 0.75) body.push(`<rect x="${x + hw * (0.22 + k * 0.38)}" y="${quay - hh * 0.62}" width="${14 * u}" height="${18 * u}" fill="${p2}" opacity="0.9"/>`);
        x += hw + 2 * u;
      }
      body.push(`<rect y="${quay}" width="${w}" height="${h - quay}" fill="url(#sea)"/>`);
      for (let i = 0; i < 30; i++) body.push(`<rect x="${r() * w}" y="${quay + r() * (h - quay)}" width="${(20 + r() * 70) * u}" height="${2 * u}" fill="${p2}" opacity="${0.15 + r() * 0.3}"/>`);
      for (let b = 0; b < 4; b++) {
        const bx = w * (0.1 + b * 0.24 + r() * 0.05), by = quay + H(0.1 + r() * 0.2), bl = (90 + r() * 60) * u;
        body.push(`<path d="M${bx} ${by} L${bx + bl} ${by} L${bx + bl * 0.85} ${by + 22 * u} L${bx + bl * 0.12} ${by + 22 * u}Z" fill="${p0}"/><rect x="${bx + bl * 0.45}" y="${by - 120 * u}" width="${4 * u}" height="${120 * u}" fill="${p0}"/><path d="M${bx + bl * 0.47} ${by - 110 * u} L${bx + bl * 0.47} ${by - 10 * u} L${bx + bl * 0.95} ${by - 10 * u}Z" fill="${mix(p3, "#ffffff", 0.5)}" opacity="0.85"/>`);
      }
      break;
    }
    case "meteors": {
      defs.push(sky("s", [[0, p0], [0.7, shade(p1)], [1, mix(p1, p2, 0.4)]]), `<linearGradient id="trail" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="${p3}" stop-opacity="0"/><stop offset="1" stop-color="#ffffff" stop-opacity="0.95"/></linearGradient>`, `<filter id="b"><feGaussianBlur stdDeviation="${3 * u}"/></filter>`);
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, stars(w, h, 360, r, H(0.75)));
      for (let i = 0; i < 14; i++) {
        const x = r() * w * 1.1, y = r() * H(0.5), len = (120 + r() * 320) * u;
        body.push(`<line x1="${x - len}" y1="${y - len * 0.55}" x2="${x}" y2="${y}" stroke="url(#trail)" stroke-width="${(2 + r() * 3) * u}" stroke-linecap="round"/><circle cx="${x}" cy="${y}" r="${5 * u}" fill="#fff" filter="url(#b)"/>`);
      }
      body.push(ridge(w, H(0.7), H(0.14), r, w / 12, mix(p0, p1, 0.45)), ridge(w, H(0.78), H(0.08), r, w / 8, p0));
      body.push(`<rect y="${H(0.8)}" width="${w}" height="${H(0.2)}" fill="${mix(p0, p1, 0.25)}"/>`);
      for (let i = 0; i < 40; i++) body.push(`<rect x="${r() * w}" y="${H(0.82 + r() * 0.16)}" width="${(10 + r() * 60) * u}" height="${2 * u}" fill="${p2}" opacity="${0.15 + r() * 0.35}"/>`);
      break;
    }
    case "glasshouse": {
      defs.push(sky("s", [[0, p0], [1, shade(mix(p0, p1, 0.6))]]), glow("g", "#fde68a"));
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, stars(w, h, 90, r, H(0.4)));
      const cx = w * 0.5, base = H(0.8), gw = w * 0.42, gh = H(0.3);
      body.push(`<circle cx="${cx}" cy="${base - gh * 0.5}" r="${w * 0.55}" fill="url(#g)" opacity="0.35"/>`);
      body.push(`<path d="M${cx - gw} ${base} L${cx - gw} ${base - gh * 0.55} A${gw} ${gh * 0.9} 0 0 1 ${cx + gw} ${base - gh * 0.55} L${cx + gw} ${base}Z" fill="#fde68a" opacity="0.28"/>`);
      for (let i = 0; i < 26; i++) { const px = cx - gw + r() * gw * 2, ph = (40 + r() * 140) * u; body.push(`<path d="M${px} ${base} q${-30 * u} ${-ph * 0.6} ${-6 * u} ${-ph} q${18 * u} ${ph * 0.5} ${12 * u} ${ph}z" fill="${mix(p1, p0, r() * 0.6)}" opacity="0.9"/>`); }
      body.push(`<g stroke="${mix(p3, "#ffffff", 0.3)}" stroke-width="${4 * u}" fill="none" opacity="0.85"><path d="M${cx - gw} ${base} L${cx - gw} ${base - gh * 0.55} A${gw} ${gh * 0.9} 0 0 1 ${cx + gw} ${base - gh * 0.55} L${cx + gw} ${base}"/>`);
      for (let k = 1; k < 8; k++) { const px = cx - gw + (k * gw * 2) / 8; const dy = gh * 0.9 * Math.sqrt(Math.max(0, 1 - ((px - cx) / gw) ** 2)); body.push(`<line x1="${px}" y1="${base}" x2="${px}" y2="${base - gh * 0.55 - dy}"/>`); }
      body.push(`<line x1="${cx - gw}" y1="${base - gh * 0.55}" x2="${cx + gw}" y2="${base - gh * 0.55}"/><line x1="${cx - gw}" y1="${base - gh * 0.25}" x2="${cx + gw}" y2="${base - gh * 0.25}"/></g>`);
      body.push(`<rect y="${base}" width="${w}" height="${h - base}" fill="${p0}"/>`);
      for (let i = 0; i < 18; i++) { const px = r() * w, ph = (60 + r() * 120) * u; body.push(`<path d="M${px} ${base + 6 * u} q${-40 * u} ${-ph * 0.5} ${-10 * u} ${-ph} q${30 * u} ${ph * 0.5} ${20 * u} ${ph}z" fill="${p0}"/>`); }
      break;
    }
    case "cliffs": {
      defs.push(sky("s", [[0, shade(mix(p3, "#ffffff", 0.3))], [0.6, shade(p2)]]), glow("g", "#ffffff"), sky("sea", [[0, p1], [1, p0]]));
      body.push(`<rect width="${w}" height="${h}" fill="url(#s)"/>`, `<circle cx="${w * 0.72}" cy="${H(0.32)}" r="${w * 0.3}" fill="url(#g)" opacity="0.8"/>`);
      body.push(`<rect y="${H(0.6)}" width="${w}" height="${H(0.4)}" fill="url(#sea)"/>`);
      for (let i = 0; i < 6; i++) body.push(hill(w, H(0.64 + i * 0.07), H(0.012 + i * 0.004), r() * 6, 4 + i, mix(p1, p0, i / 6), 0.8));
      body.push(`<path d="M0 ${H(0.22)} L${w * 0.18} ${H(0.26)} L${w * 0.26} ${H(0.4)} L${w * 0.31} ${H(0.62)} L${w * 0.36} ${H(0.78)} L${w * 0.4} ${h} L0 ${h}Z" fill="${mix(p0, "#3f3f46", 0.4)}"/><path d="M0 ${H(0.22)} L${w * 0.18} ${H(0.26)} L${w * 0.2} ${H(0.3)} L0 ${H(0.27)}Z" fill="#4d7c0f"/>`);
      body.push(`<path d="M${w * 0.82} ${H(0.48)} L${w * 0.86} ${H(0.4)} L${w * 0.9} ${H(0.42)} L${w * 0.92} ${H(0.62)} L${w * 0.8} ${H(0.62)}Z" fill="${mix(p0, "#3f3f46", 0.4)}"/>`);
      for (let i = 0; i < 16; i++) { const bx = w * (0.4 + r() * 0.5), by = H(0.1 + r() * 0.3), s = (6 + r() * 10) * u; body.push(`<path d="M${bx - s} ${by - s * 0.4} Q${bx - s * 0.4} ${by - s * 0.6} ${bx} ${by} Q${bx + s * 0.4} ${by - s * 0.6} ${bx + s} ${by - s * 0.4}" stroke="${p0}" stroke-width="${2.2 * u}" fill="none"/>`); }
      break;
    }
  }
  // A vignette, as a lens leaves it.
  defs.push(`<radialGradient id="vig" cx="0.5" cy="0.45" r="0.75"><stop offset="0.6" stop-color="#000" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity="0.45"/></radialGradient>`);
  body.push(`<rect width="${w}" height="${h}" fill="url(#vig)"/>`);
  return { defs: defs.join(""), body: body.join("") };
}

const FONTS = {
  serif: { family: "Georgia, 'Cambria', serif", weight: 700, transform: "none", spacing: "0", style: "normal" },
  sans: { family: "Bahnschrift, 'Segoe UI', sans-serif", weight: 700, transform: "uppercase", spacing: "0.06em", style: "normal", stretch: "condensed" },
  rounded: { family: "'Segoe UI', sans-serif", weight: 900, transform: "none", spacing: "-0.01em", style: "normal" },
  mono: { family: "Consolas, 'Cascadia Mono', monospace", weight: 700, transform: "uppercase", spacing: "0.12em", style: "normal" },
};

function poster(t, w, h, label) {
  const { defs, body } = scene(t.scene, w, h, t.pal, hash(t.title));
  const f = FONTS[t.font];
  const title = label ?? t.title;
  const size = Math.min(w * 0.13, (w * 1.6) / Math.max(6, title.length * (t.font === "sans" || t.font === "mono" ? 0.9 : 0.75)));
  return `<!doctype html><html><head><style>
  body{margin:0;width:${w}px;height:${h}px;position:relative;overflow:hidden;font-family:${f.family}}
  svg{position:absolute;inset:0}
  .shade{position:absolute;inset:0;background:linear-gradient(to bottom, rgba(0,0,0,0.35) 0%, rgba(0,0,0,0) 22%, rgba(0,0,0,0) 62%, rgba(0,0,0,0.78) 100%)}
  .tag{position:absolute;top:${h * 0.045}px;left:0;right:0;text-align:center;color:rgba(255,255,255,0.88);font:600 ${w * 0.026}px 'Segoe UI',sans-serif;letter-spacing:0.22em;text-transform:uppercase}
  .title{position:absolute;bottom:${h * 0.085}px;left:${w * 0.07}px;right:${w * 0.07}px;text-align:center;color:#fff;font-weight:${f.weight};font-size:${size}px;line-height:1.02;text-transform:${f.transform};letter-spacing:${f.spacing};font-stretch:${f.stretch ?? "normal"};text-shadow:0 ${w * 0.004}px ${w * 0.03}px rgba(0,0,0,0.55)}
  .sub{position:absolute;bottom:${h * 0.045}px;left:0;right:0;text-align:center;color:rgba(255,255,255,0.75);font:500 ${w * 0.022}px 'Segoe UI',sans-serif;letter-spacing:0.3em;text-transform:uppercase}
  </style></head><body><svg width="${w}" height="${h}" viewBox="0 0 ${w} ${h}"><defs>${defs}</defs>${body}</svg><div class="shade"></div>
  <div class="tag">${t.tagline ?? ""}</div><div class="title">${title}</div><div class="sub">${label ? t.title : t.year}</div></body></html>`;
}
function plain(t, w, h, v) {
  const { defs, body } = scene(t.scene, w, h, t.pal, hash(t.title) + v * 7919, v);
  return `<!doctype html><html><head><style>body{margin:0}svg{display:block}</style></head><body><svg width="${w}" height="${h}" viewBox="0 0 ${w} ${h}"><defs>${defs}</defs>${body}</svg></body></html>`;
}

/** Chrome or Edge: $CHROME, else the usual place on Windows, macOS or Linux. */
function chrome() {
  const candidates = [
    process.env.CHROME,
    "C:/Program Files/Google/Chrome/Application/chrome.exe",
    "C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
  ];
  const found = candidates.find((p) => p && existsSync(p));
  if (!found) throw new Error("No Chrome or Edge found: set CHROME to one.");
  return found;
}

async function renderer() {
  const profile = mkdtempSync(join(tmpdir(), "pley-shots-"));
  const browser = spawn(chrome(), ["--headless=new", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "--no-first-run", "--hide-scrollbars", "--force-color-profile=srgb", "about:blank"], { stdio: ["ignore", "ignore", "pipe"] });
  const url = await new Promise((resolve) => {
    let said = "";
    browser.stderr.on("data", (d) => {
      said += d;
      const m = said.match(/DevTools listening on (ws:\/\/\S+)/);
      if (m) resolve(m[1]);
    });
  });
  const socket = new WebSocket(url);
  await new Promise((r) => socket.addEventListener("open", r, { once: true }));
  let next = 0;
  const waiting = new Map();
  socket.addEventListener("message", (e) => {
    const m = JSON.parse(e.data);
    const c = waiting.get(m.id);
    if (!c) return;
    waiting.delete(m.id);
    m.error ? c.reject(new Error(m.error.message)) : c.resolve(m.result);
  });
  const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => { const id = ++next; waiting.set(id, { resolve, reject }); socket.send(JSON.stringify({ id, method, params, sessionId })); });
  const { targetId } = await send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
  const { frameTree } = await send("Page.getFrameTree", {}, sessionId);
  return {
    async jpeg(html, w, h, file) {
      await send("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: 1, mobile: false }, sessionId);
      await send("Page.setDocumentContent", { frameId: frameTree.frame.id, html }, sessionId);
      await send("Runtime.evaluate", { expression: "document.fonts.ready.then(() => true)", awaitPromise: true }, sessionId);
      const { data } = await send("Page.captureScreenshot", { format: "jpeg", quality: 90, clip: { x: 0, y: 0, width: w, height: h, scale: 1 } }, sessionId);
      writeFileSync(file, Buffer.from(data, "base64"));
    },
    close() { socket.close(); browser.kill(); },
  };
}

const r = await renderer();
const art = (name) => join(out, "art", name);
const slug = (s) => s.toLowerCase().replace(/[^a-z0-9]+/g, "-");
try {
  for (const m of movies) {
    await r.jpeg(poster(m, 1000, 1500), 1000, 1500, art(`${slug(m.title)}-poster.jpg`));
    await r.jpeg(plain(m, 1920, 1080, 0), 1920, 1080, art(`${slug(m.title)}-fanart.jpg`));
  }
  for (const s of shows) {
    await r.jpeg(poster(s, 1000, 1500), 1000, 1500, art(`${slug(s.title)}-poster.jpg`));
    await r.jpeg(plain(s, 1920, 1080, 0), 1920, 1080, art(`${slug(s.title)}-fanart.jpg`));
    for (let n = 0; n < s.seasons.length; n++) {
      await r.jpeg(poster({ ...s, tagline: "" }, 1000, 1500, `Season ${n + 1}`), 1000, 1500, art(`${slug(s.title)}-season${n + 1}.jpg`));
      for (let e = 0; e < s.seasons[n].length; e++) await r.jpeg(plain(s, 1280, 720, 1 + n * 20 + e), 1280, 720, art(`${slug(s.title)}-s${n + 1}e${e + 1}.jpg`));
    }
  }
  console.log("art done");
} finally {
  r.close();
}

import type { ComponentChildren } from "preact";
import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import qrcode from "qrcode-generator";
import { t } from "../../../i18n";
import { useUi } from "../../context";
import { F, G } from "../../kit/focusable";
import { Icon } from "../../kit/icons";

/** The onboarding screens' background: Bg with BrandDeep glowing up from the bottom-right corner. */
export function BrandGlow(p: { children: ComponentChildren }) {
  return <div class="brand-glow">{p.children}</div>;
}

/** The Pley mark: three posters on a shelf, play cut out of the front one, on its tile (the web's LogoMark). */
export function LogoMark(p: { size: number }) {
  return (
    <svg width={p.size} height={p.size} viewBox="0 0 64 64" aria-hidden="true" style="flex:none">
      <defs>
        <linearGradient id="pley-mark" x1="12" y1="15" x2="34" y2="49" gradientUnits="userSpaceOnUse">
          <stop offset="0" stop-color="#bae6fd" />
          <stop offset="1" stop-color="#38bdf8" />
        </linearGradient>
      </defs>
      <rect width="64" height="64" rx="15" fill="#0a1622" />
      <path fill="#0c4a6e" d="M46 21.5h3a3 3 0 0 1 3 3v15a3 3 0 0 1-3 3h-3z" />
      <path fill="#0284c7" d="M36 18.5h4.5a3.5 3.5 0 0 1 3.5 3.5v20a3.5 3.5 0 0 1-3.5 3.5H36z" />
      <path fill="url(#pley-mark)" fill-rule="evenodd" d="M16 15.5h14a4 4 0 0 1 4 4v25a4 4 0 0 1-4 4H16a4 4 0 0 1-4-4v-25a4 4 0 0 1 4-4zM19 27v10a1 1 0 0 0 1.51.86l8.5-5a1 1 0 0 0 0-1.72l-8.5-5A1 1 0 0 0 19 27z" />
    </svg>
  );
}

/** The logo (PleyLogo): the mark, then "pley" at three quarters of its height. */
export function PleyLogo(p: { height: number }) {
  const h = p.height;
  return (
    <div class="pley-logo" style={`height:${h}px`} aria-label="Pley" role="img">
      <LogoMark size={h} />
      <span style={`margin-left:${h * 0.4}px;font-size:${h * 0.75}px;line-height:${h}px;font-weight:700;letter-spacing:-0.25px;color:var(--fg)`}>Pley</span>
    </div>
  );
}

/** An 8 px Accent dot stepping through 100, 70, 40 and 70 % every half second. */
export function BreathingDot() {
  return <span class="breathing-dot" aria-hidden="true" />;
}

/** A status line: the breathing dot while waiting, then the text (at most 2 lines). */
export function StatusLine(p: { text: string; waiting?: boolean; class?: string; style?: string }) {
  return (
    <div class="status-line" style={p.style} aria-live="polite">
      {p.waiting && <BreathingDot />}
      <span class={`${p.class ?? "t-label c-muted"} lines-2`}>{p.text}</span>
    </div>
  );
}

/**
 * The modules of a QR code for `text` at ECC M, raised to Q or H when that
 * fits in the same version (as Nayuki's boostEcl does); `null` when it
 * can't be encoded.
 */
export function qrModules(text: string): boolean[][] | null {
  const make = (ecc: "M" | "Q" | "H") => {
    try {
      const qr = qrcode(0, ecc);
      qr.addData(text, "Byte");
      qr.make();
      return qr;
    } catch {
      return null;
    }
  };
  const base = make("M");
  if (!base) return null;
  let best = base;
  for (const ecc of ["Q", "H"] as const) {
    const q = make(ecc);
    if (q && q.getModuleCount() === base.getModuleCount()) best = q;
  }
  const n = best.getModuleCount();
  const rows: boolean[][] = [];
  for (let r = 0; r < n; r++) {
    const row: boolean[] = [];
    for (let c = 0; c < n; c++) row.push(best.isDark(r, c));
    rows.push(row);
  }
  return rows;
}

/**
 * A QR code in a white box `size` canvas px wide (QrCodeImage): a 4-module
 * quiet zone, whole device pixels per module, centred. Drawn on a canvas at
 * the screen's own pixels so it stays sharp.
 */
export function QrCode(p: { text: string; size: number }) {
  const ui = useUi();
  const canvas = useRef<HTMLCanvasElement>(null);
  const device = Math.round(p.size * ui.scale);
  useLayoutEffect(() => {
    const el = canvas.current;
    if (!el) return;
    el.width = device;
    el.height = device;
    const g = el.getContext("2d");
    if (!g) return;
    g.fillStyle = "#ffffff";
    g.fillRect(0, 0, device, device);
    const modules = qrModules(p.text);
    if (!modules) return;
    const n = modules.length;
    const unit = Math.max(1, Math.floor(device / (n + 8)));
    const offset = Math.floor((device - unit * (n + 8)) / 2) + unit * 4;
    g.fillStyle = "#000000";
    for (let r = 0; r < n; r++) {
      const row = modules[r]!;
      for (let c = 0; c < n; c++) if (row[c]) g.fillRect(offset + c * unit, offset + r * unit, unit, unit);
    }
  }, [p.text, device]);
  return <canvas ref={canvas} style={`width:${p.size}px;height:${p.size}px;display:block`} aria-hidden="true" />;
}

/** Colours offered for profiles; also the one picked from a name (the web's Avatar). */
export const AVATAR_COLORS = ["#38bdf8", "#a78bfa", "#f472b6", "#fb923c", "#facc15", "#4ade80", "#2dd4bf", "#f87171"];

/** The profile's colour when it is one ("#rgb", "#rrggbb"), else one from its name. */
export function avatarColor(name: string, color: string | null | undefined): string {
  if (color && /^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.test(color)) return color;
  let h = 0;
  for (let i = 0; i < name.length; i++) h = (Math.imul(h, 31) + name.charCodeAt(i)) >>> 0;
  return AVATAR_COLORS[h % AVATAR_COLORS.length]!;
}

/** Bg on light colours, white on dark ones. */
export function textOn(hex: string): string {
  const m = /^#?([0-9a-fA-F]{3}|[0-9a-fA-F]{6})$/.exec(hex);
  if (!m) return "#ffffff";
  const d = m[1]!;
  const full = d.length === 3 ? d.split("").map((c) => c + c).join("") : d;
  const ch = (at: number) => parseInt(full.slice(at, at + 2), 16) / 255;
  return 0.2126 * ch(0) + 0.7152 * ch(2) + 0.0722 * ch(4) > 0.45 ? "#0b0d10" : "#ffffff";
}

export function initialOf(name: string): string {
  const c = name.trim().charAt(0);
  return c ? c.toUpperCase() : "?";
}

/**
 * A profile's avatar (ProfileAvatar): its colour, its picture over it
 * (fetched with the app's token), else its initial; the lock badge when it
 * has a PIN.
 */
export function ProfileAvatar(p: { name: string; color: string | null; picture: string | null; size: number; locked?: boolean }) {
  const ui = useUi();
  const fill = avatarColor(p.name, p.color);
  const [src, setSrc] = useState<string | null>(null);
  useEffect(() => {
    setSrc(null);
    if (!p.picture || !ui.images) return;
    const url = ui.images.url(p.picture, p.size * ui.scale);
    return ui.images.load(url, (objectUrl) => setSrc(objectUrl), -1);
  }, [p.picture]);
  return (
    <div class="profile-avatar" style={`width:${p.size}px;height:${p.size}px;background:${fill}`}>
      {p.picture ? (
        src && <img src={src} alt="" />
      ) : (
        <span style={`font-size:${p.size * 0.44}px;line-height:${p.size * 0.5}px;font-weight:700;color:${textOn(fill)}`}>{initialOf(p.name)}</span>
      )}
      {p.locked && (
        <span class="lock-badge">
          <span>
            <Icon name="lock" size={14} />
          </span>
        </span>
      )}
    </div>
  );
}

/** A key of a number pad: a digit, or Delete / OK. */
export type PadKey = { kind: "text"; text: string } | { kind: "delete" } | { kind: "ok" };

/** What the screen reader says for a key. */
function padLabel(k: PadKey): string {
  if (k.kind === "delete") return t("onboarding.pad.delete");
  if (k.kind === "ok") return t("onboarding.pad.ok");
  return t("onboarding.pad.digit", { digit: k.text });
}

/**
 * A number pad (TvKeypad): rows of 64 px keys 14 apart; a pressed key
 * flashes for 100 ms, and so does the key the remote typed, wherever focus
 * is. Disabled, the whole pad is at 40 % and takes no focus.
 */
export function NumberPad(p: {
  rows: PadKey[][];
  onKey: (key: PadKey) => void;
  enabled?: boolean;
  /** The key the remote typed last, and a counter that changes with each press: it flashes. */
  flash?: { key: string; n: number } | null;
  fid?: string;
  initialKey?: string;
  extraRow?: ComponentChildren;
}) {
  const enabled = p.enabled !== false;
  const [flashing, setFlashing] = useState<string | null>(null);
  useEffect(() => {
    if (!p.flash) return;
    setFlashing(p.flash.key);
    const t = setTimeout(() => setFlashing(null), 100);
    return () => clearTimeout(t);
  }, [p.flash?.n]);
  const press = (key: PadKey, id: string) => {
    setFlashing(id);
    setTimeout(() => setFlashing((f) => (f === id ? null : f)), 100);
    p.onKey(key);
  };
  const cols = p.rows[0]?.length ?? 3;
  return (
    <div class={`number-pad${enabled ? "" : " off"}`} style={`width:${cols * 64 + (cols - 1) * 14}px`}>
      <G kind="grid" columns={cols} class="pad-grid">
        {([] as PadKey[]).concat(...p.rows).map((k) => {
          const id = keyId(k);
          return (
            <F
              key={id}
              class={`pad-key${flashing === id ? " flash" : ""}`}
              fid={p.fid ? `${p.fid}/${id}` : undefined}
              initial={enabled && id === (p.initialKey ?? "5")}
              disabled={!enabled}
              label={padLabel(k)}
              onSelect={() => press(k, id)}
            >
              <PadGlyph k={k} />
            </F>
          );
        })}
      </G>
      {p.extraRow}
    </div>
  );
}

export function keyId(k: PadKey): string {
  return k.kind === "text" ? k.text : k.kind;
}

function PadGlyph(p: { k: PadKey }) {
  if (p.k.kind === "delete") return <Icon name="delete" size={28} />;
  if (p.k.kind === "ok") return <Icon name="check" size={28} />;
  return <span class="pad-glyph">{p.k.text}</span>;
}

const digits = (from: number) => [0, 1, 2].map((i) => ({ kind: "text" as const, text: String(from + i) }));

/** The PIN pad's keys: 1-9, then Delete, 0, OK. */
export const PIN_KEYS: PadKey[][] = [digits(1), digits(4), digits(7), [{ kind: "delete" }, { kind: "text", text: "0" }, { kind: "ok" }]];

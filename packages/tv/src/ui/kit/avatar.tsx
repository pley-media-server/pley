import { useEffect, useState } from "preact/hooks";
import { useUi } from "../context";
import { Icon } from "./icons";

/**
 * Colors offered for profiles; also the one picked from a name. Every app
 * draws a profile's avatar by the web's rule (its Avatar), which
 * packages/tv/test/fixtures/avatar.json pins (docs/THEMES.md, "Profile
 * colors"). They are the profile's, not the theme's: the same under every
 * theme and accent.
 */
export const AVATAR_COLORS = ["#38bdf8", "#a78bfa", "#f472b6", "#fb923c", "#facc15", "#4ade80", "#2dd4bf", "#f87171"];

/** The initial's two colors: Midnight's background on light circles, white on dark ones. */
export const AVATAR_INK = { dark: "#0b0d10", light: "#ffffff" };

/** A color the server keeps (#rgb or #rrggbb, any case) as #rrggbb; null for anything else. */
function hexColor(color: string | null | undefined): string | null {
  const digits = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(color ?? "")?.[1]?.toLowerCase();
  if (!digits) return null;
  return `#${digits.length === 3 ? digits.replace(/./g, "$&$&") : digits}`;
}

/** The profile's color, or, without a valid one, one picked from its name's code points (the name untrimmed). */
export function avatarColor(name: string, color: string | null | undefined): string {
  const own = hexColor(color);
  if (own) return own;
  let h = 0;
  for (const ch of name) h = (Math.imul(h, 31) + ch.codePointAt(0)!) >>> 0;
  return AVATAR_COLORS[h % AVATAR_COLORS.length]!;
}

/** WCAG's relative luminance of #rrggbb. */
function luminance(hex: string): number {
  const linear = (at: number) => {
    const c = parseInt(hex.slice(at, at + 2), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  };
  return 0.2126 * linear(1) + 0.7152 * linear(3) + 0.0722 * linear(5);
}

/** The initial's color on a circle: dark or white, whichever has the higher WCAG contrast with it (dark on a tie). */
export function avatarInk(color: string): string {
  const hex = hexColor(color);
  if (!hex) return AVATAR_INK.light;
  const on = luminance(hex) + 0.05;
  const dark = on / (luminance(AVATAR_INK.dark) + 0.05);
  const light = (luminance(AVATAR_INK.light) + 0.05) / on;
  return dark >= light ? AVATAR_INK.dark : AVATAR_INK.light;
}

/** The trimmed name's first code point in capitals, "?" for a blank name. */
export function avatarInitial(name: string): string {
  const first = name.trim().codePointAt(0);
  return first === undefined ? "?" : String.fromCodePoint(first).toUpperCase();
}

/**
 * A profile's avatar, wherever the app draws one (the rail, Settings,
 * "Who's watching?"): its picture (fetched with the app's token); while it
 * loads, without one, or when it fails, a circle in the profile's color
 * with its initial. The lock badge when it has a PIN.
 */
export function ProfileAvatar(p: { name: string; color: string | null; picture: string | null; size: number; locked?: boolean }) {
  const ui = useUi();
  const url = p.picture && ui.images ? ui.images.url(p.picture, p.size * ui.scale) : null;
  const [src, setSrc] = useState<string | null>(() => (url ? ui.images.ready(url) : null));
  useEffect(() => {
    setSrc(url ? ui.images.ready(url) : null);
    if (!url) return;
    return ui.images.load(url, setSrc, -1);
  }, [url]);
  const fill = avatarColor(p.name, p.color);
  return (
    <div class="profile-avatar" style={`width:${p.size}px;height:${p.size}px${src ? "" : `;background:${fill}`}`}>
      {src ? (
        <img src={src} alt="" onError={() => setSrc(null)} />
      ) : (
        <span style={`font-size:${p.size * 0.44}px;line-height:${p.size * 0.5}px;font-weight:700;color:${avatarInk(fill)}`}>{avatarInitial(p.name)}</span>
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

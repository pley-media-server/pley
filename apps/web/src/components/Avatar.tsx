import { useState } from "react";
import type { User } from "@/api/types";
import { cn } from "@/lib/cn";

/**
 * Colors offered for profiles; also the fallback picked from the name. Every
 * app draws a profile's avatar by the same rule, which
 * packages/tv/test/fixtures/avatar.json pins (docs/THEMES.md, "Profile colors").
 */
export const AVATAR_COLORS = ["#38bdf8", "#a78bfa", "#f472b6", "#fb923c", "#facc15", "#4ade80", "#2dd4bf", "#f87171"];

/** The initial's two colors: Midnight's background on light circles, white on dark ones. */
const INK_DARK = "#0b0d10";
const INK_LIGHT = "#ffffff";

const SIZES = {
  xs: "size-6 text-[11px]",
  sm: "size-8 text-xs",
  md: "size-9 text-sm",
  lg: "size-14 text-xl",
  xl: "size-28 text-4xl sm:size-32 sm:text-5xl",
} as const;

export type AvatarUser = Pick<User, "id" | "name" | "avatar_url" | "avatar_color">;

/** A color the server keeps (#rgb or #rrggbb, any case) as #rrggbb; null for anything else. */
function hexColor(color: string | null | undefined): string | null {
  const digits = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(color ?? "")?.[1]?.toLowerCase();
  if (!digits) return null;
  return `#${digits.length === 3 ? [...digits].map((c) => c + c).join("") : digits}`;
}

/** The profile's color, or, without a valid one, a stable one derived from its name. */
export function avatarColor(user: Pick<User, "name" | "avatar_color">): string {
  const own = hexColor(user.avatar_color);
  if (own) return own;
  let hash = 0;
  for (const ch of user.name) hash = (hash * 31 + (ch.codePointAt(0) ?? 0)) >>> 0;
  return AVATAR_COLORS[hash % AVATAR_COLORS.length] ?? "#38bdf8";
}

/** WCAG's relative luminance of #rrggbb. */
function luminance(hex: string): number {
  const linear = (at: number) => {
    const c = parseInt(hex.slice(at, at + 2), 16) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * linear(1) + 0.7152 * linear(3) + 0.0722 * linear(5);
}

/** The initial's color on a circle: dark or white, whichever reads better on it (WCAG's contrast). */
export function textOn(color: string): string {
  const hex = hexColor(color.startsWith("#") ? color : `#${color}`);
  if (!hex) return INK_LIGHT;
  const on = luminance(hex) + 0.05;
  const dark = on / (luminance(INK_DARK) + 0.05);
  const light = (luminance(INK_LIGHT) + 0.05) / on;
  return dark >= light ? INK_DARK : INK_LIGHT;
}

/** The name's first letter in capitals, "?" for a blank name. */
export function avatarInitial(name: string): string {
  const [first] = name.trim();
  return first ? first.toUpperCase() : "?";
}

/** Profile picture, or a colored circle with the initial. */
export function Avatar({
  user,
  size = "md",
  className,
}: {
  user: AvatarUser;
  size?: keyof typeof SIZES;
  className?: string;
}) {
  const [broken, setBroken] = useState<string | null>(null);
  const picture = user.avatar_url && broken !== user.avatar_url ? user.avatar_url : null;
  const color = avatarColor(user);
  return (
    <span
      aria-hidden
      className={cn(
        "relative inline-flex shrink-0 select-none items-center justify-center overflow-hidden rounded-full font-bold",
        SIZES[size],
        className,
      )}
      style={picture ? undefined : { backgroundColor: color, color: textOn(color) }}
    >
      {picture ? (
        <img
          src={picture}
          alt=""
          draggable={false}
          className="size-full object-cover"
          onError={() => setBroken(picture)}
        />
      ) : (
        avatarInitial(user.name)
      )}
    </span>
  );
}

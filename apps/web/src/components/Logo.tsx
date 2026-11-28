import { useId } from "react";
import { cn } from "@/lib/cn";

/** The mark's shapes, on a 64 × 64 tile: the back and middle posters, and the front one with play cut out. */
const FAR = "M46 21.5h3a3 3 0 0 1 3 3v15a3 3 0 0 1-3 3h-3z";
const DEEP = "M36 18.5h4.5a3.5 3.5 0 0 1 3.5 3.5v20a3.5 3.5 0 0 1-3.5 3.5H36z";
const FRONT =
  "M16 15.5h14a4 4 0 0 1 4 4v25a4 4 0 0 1-4 4H16a4 4 0 0 1-4-4v-25a4 4 0 0 1 4-4zM19 27v10a1 1 0 0 0 1.51.86l8.5-5a1 1 0 0 0 0-1.72l-8.5-5A1 1 0 0 0 19 27z";

/**
 * The Pley mark: three posters on a shelf, play cut out of the front one,
 * in the theme's colors (index.css, `--color-brand-*`).
 */
export function LogoMark({ className, tile = true }: { className?: string; tile?: boolean }) {
  const gradient = useId();
  return (
    <svg viewBox="0 0 64 64" className={cn("shrink-0", className)} aria-hidden>
      <defs>
        <linearGradient id={gradient} x1="12" y1="15" x2="34" y2="49" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="var(--color-brand-light)" />
          <stop offset="1" stopColor="var(--color-brand)" />
        </linearGradient>
      </defs>
      {tile ? <rect width="64" height="64" rx="15" fill="var(--color-brand-tile)" /> : null}
      <path fill="var(--color-brand-far)" d={FAR} />
      <path fill="var(--color-brand-deep)" d={DEEP} />
      <path fill={`url(#${gradient})`} fillRule="evenodd" d={FRONT} />
    </svg>
  );
}

/**
 * The mark on its tile as an SVG file's text, in the colors a theme's
 * styles give (`--color-brand-*`): the browser tab's icon.
 */
export function logoIcon(style: CSSStyleDeclaration): string {
  const color = (name: string) => style.getPropertyValue(`--color-${name}`).trim();
  return [
    `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64">`,
    `<defs><linearGradient id="front" x1="12" y1="15" x2="34" y2="49" gradientUnits="userSpaceOnUse">`,
    `<stop offset="0" stop-color="${color("brand-light")}"/><stop offset="1" stop-color="${color("brand")}"/>`,
    `</linearGradient></defs>`,
    `<rect width="64" height="64" rx="15" fill="${color("brand-tile")}"/>`,
    `<path fill="${color("brand-far")}" d="${FAR}"/>`,
    `<path fill="${color("brand-deep")}" d="${DEEP}"/>`,
    `<path fill="url(#front)" fill-rule="evenodd" d="${FRONT}"/>`,
    `</svg>`,
  ].join("");
}

export function Logo({ className }: { className?: string }) {
  return (
    <span className={cn("inline-flex h-8 items-center gap-2.5", className)}>
      <LogoMark className="h-full w-auto" />
      <span className="text-xl font-extrabold tracking-tight text-fg">Pley</span>
    </span>
  );
}

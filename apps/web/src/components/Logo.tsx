import { useId } from "react";
import { cn } from "@/lib/cn";

/** The mark's three posters on its 64 by 64 grid, back to front; play is cut out of the front one (even-odd). */
export const MARK_PATHS = {
  back: "M46 21.5h3a3 3 0 0 1 3 3v15a3 3 0 0 1-3 3h-3z",
  middle: "M36 18.5h4.5a3.5 3.5 0 0 1 3.5 3.5v20a3.5 3.5 0 0 1-3.5 3.5H36z",
  front:
    "M16 15.5h14a4 4 0 0 1 4 4v25a4 4 0 0 1-4 4H16a4 4 0 0 1-4-4v-25a4 4 0 0 1 4-4zM19 27v10a1 1 0 0 0 1.51.86l8.5-5a1 1 0 0 0 0-1.72l-8.5-5A1 1 0 0 0 19 27z",
};

/** The Pley mark: three posters on a shelf, play cut out of the front one. */
export function LogoMark({ className, tile = true }: { className?: string; tile?: boolean }) {
  const gradient = useId();
  return (
    <svg viewBox="0 0 64 64" className={cn("shrink-0", className)} aria-hidden>
      <defs>
        <linearGradient id={gradient} x1="12" y1="15" x2="34" y2="49" gradientUnits="userSpaceOnUse">
          <stop offset="0" stopColor="var(--color-brand-light)" />
          <stop offset="1" stopColor="var(--color-accent)" />
        </linearGradient>
      </defs>
      {tile ? <rect width="64" height="64" rx="15" fill="var(--color-brand-tile)" /> : null}
      <path fill="var(--color-brand-far)" d={MARK_PATHS.back} />
      <path fill="var(--color-brand-deep)" d={MARK_PATHS.middle} />
      <path fill={`url(#${gradient})`} fillRule="evenodd" d={MARK_PATHS.front} />
    </svg>
  );
}

/**
 * The mark on a picture that won't come (the Android app's placeholder):
 * white posters, each lighter than the one behind, centred in its box at
 * 40 % of the box's shorter side and 120 px at most.
 */
export function PlaceholderMark() {
  return (
    <div className="pointer-events-none absolute inset-0 flex items-center justify-center [container-type:size]" aria-hidden>
      <svg viewBox="0 0 64 64" className="aspect-square w-[min(40cqmin,120px)]">
        <path fill="#fff" fillOpacity={0.16} d={MARK_PATHS.back} />
        <path fill="#fff" fillOpacity={0.24} d={MARK_PATHS.middle} />
        <path fill="#fff" fillOpacity={0.36} fillRule="evenodd" d={MARK_PATHS.front} />
      </svg>
    </div>
  );
}

export function Logo({ className }: { className?: string }) {
  return (
    <span className={cn("inline-flex h-8 items-center gap-2.5", className)}>
      <LogoMark className="h-full w-auto" />
      <span className="text-xl font-extrabold tracking-tight text-fg">Pley</span>
    </span>
  );
}

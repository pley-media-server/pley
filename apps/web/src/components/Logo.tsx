import { useId } from "react";
import { cn } from "@/lib/cn";

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
      <path fill="var(--color-brand-far)" d="M46 21.5h3a3 3 0 0 1 3 3v15a3 3 0 0 1-3 3h-3z" />
      <path fill="var(--color-brand-deep)" d="M36 18.5h4.5a3.5 3.5 0 0 1 3.5 3.5v20a3.5 3.5 0 0 1-3.5 3.5H36z" />
      <path
        fill={`url(#${gradient})`}
        fillRule="evenodd"
        d="M16 15.5h14a4 4 0 0 1 4 4v25a4 4 0 0 1-4 4H16a4 4 0 0 1-4-4v-25a4 4 0 0 1 4-4zM19 27v10a1 1 0 0 0 1.51.86l8.5-5a1 1 0 0 0 0-1.72l-8.5-5A1 1 0 0 0 19 27z"
      />
    </svg>
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

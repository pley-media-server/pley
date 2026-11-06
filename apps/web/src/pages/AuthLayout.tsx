import type { ReactNode } from "react";
import { cn } from "@/lib/cn";
import { useOptionalAuth } from "@/auth";
import { Logo } from "@/components/Logo";
import { LanguagePicker } from "@/i18n/LanguagePicker";

/**
 * Centered card used by the login and setup screens. Without an account
 * signed in, a small globe under the card picks this device's
 * language: once signed in, the account's comes first, and Account changes it.
 */
export function AuthLayout({
  title,
  subtitle,
  children,
  wide,
}: {
  title: string;
  subtitle?: ReactNode;
  children: ReactNode;
  /** For a page with more to it than a form (moving from Plex). */
  wide?: boolean;
}) {
  const signedIn = useOptionalAuth() !== null;
  return (
    <div className="relative flex min-h-screen items-center justify-center overflow-hidden px-4 py-12">
      <div
        aria-hidden
        className="pointer-events-none absolute inset-0 opacity-60"
        style={{
          background:
            "radial-gradient(900px 500px at 15% -10%, color-mix(in oklab, var(--color-accent) 18%, transparent), transparent 60%), radial-gradient(700px 500px at 110% 110%, color-mix(in oklab, var(--color-info) 12%, transparent), transparent 60%)",
        }}
      />
      <div className={cn("relative w-full animate-pop-in", wide ? "max-w-2xl" : "max-w-sm")}>
        <div className="mb-8 flex justify-center">
          <Logo className="h-10" />
        </div>
        <div className="rounded-xl border border-border bg-surface/90 p-6 shadow-2xl shadow-black/50 backdrop-blur sm:p-8">
          <h1 className="text-xl font-semibold tracking-tight">{title}</h1>
          {subtitle ? <p className="mt-1.5 text-sm text-muted">{subtitle}</p> : null}
          <div className="mt-6">{children}</div>
        </div>
        {signedIn ? null : (
          <div className="mt-4 flex justify-center">
            <LanguagePicker />
          </div>
        )}
      </div>
    </div>
  );
}

import type { ReactNode } from "react";
import { cn } from "@/lib/cn";
import { useOptionalAuth } from "@/auth";
import { AuthBackground } from "@/components/AuthBackground";
import { Logo } from "@/components/Logo";
import { LanguagePicker } from "@/i18n/LanguagePicker";
import { useOutsideProfile } from "@/theme";

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
  bare,
}: {
  title: string;
  subtitle?: ReactNode;
  children: ReactNode;
  /** For a page with more to it than a form (moving from Plex). */
  wide?: boolean;
  /** No card around it, for a choice whose options are cards of their own (how to start, in setup). */
  bare?: boolean;
}) {
  // Signing in, and approving an app or a TV, are drawn as Pley's own, outside a profile.
  useOutsideProfile();
  const signedIn = useOptionalAuth() !== null;
  return (
    <div className="relative flex min-h-screen items-center justify-center overflow-hidden px-4 py-12">
      <AuthBackground />
      <div className={cn("relative w-full animate-pop-in", wide ? "max-w-2xl" : "max-w-sm")}>
        <div className="mb-8 flex justify-center">
          <Logo className="h-10" />
        </div>
        <div
          className={cn(
            bare ? "text-center" : "rounded-xl border border-border bg-surface/90 p-6 shadow-2xl shadow-shade/50 backdrop-blur sm:p-8",
          )}
        >
          <h1 className={cn("font-semibold tracking-tight", bare ? "text-2xl" : "text-xl")}>{title}</h1>
          {subtitle ? <p className="mt-1.5 text-sm text-muted">{subtitle}</p> : null}
          <div className={cn("mt-6", bare && "mt-8 text-left")}>{children}</div>
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

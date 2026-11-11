import { useEffect, useId, useMemo, useState, type ReactNode, type Ref } from "react";
import { Outlet, useLocation } from "react-router";
import { useTranslation } from "react-i18next";
import { Logo } from "@/components/Logo";
import { ProgressBar } from "@/components/ui";
import { LanguagePicker } from "@/i18n/LanguagePicker";
import { SetupFrameContext, useSetupFrame } from "@/lib/setupFrame";
import { AuthBackground } from "./AuthLayout";

/**
 * Setup, from the administrator's account to the first libraries or the
 * Plex import: one card that stays while its steps change inside it, with
 * how far setup is along its top.
 */
export function SetupLayout() {
  const { t } = useTranslation("pages");
  const { pathname } = useLocation();
  const [progress, setProgress] = useState(0);
  const frame = useMemo(() => ({ setProgress }), []);
  return (
    <div className="relative flex min-h-screen items-center justify-center overflow-hidden px-4 py-12">
      <AuthBackground />
      <div className="relative w-full max-w-xl">
        <div className="mb-8 flex justify-center">
          <Logo className="h-10" />
        </div>
        <div className="animate-pop-in overflow-hidden rounded-xl border border-border bg-surface/90 shadow-2xl shadow-black/50 backdrop-blur">
          <ProgressBar value={progress} label={t("setup.progress")} className="rounded-none bg-white/5" barClassName="rounded-none duration-500" />
          <div className="p-6 sm:p-8">
            <SetupFrameContext.Provider value={frame}>
              {/* A step comes in over the last; the card stays. */}
              <div key={pathname} className="animate-fade-in">
                <Outlet />
              </div>
            </SetupFrameContext.Provider>
          </div>
        </div>
        {/* Before the account is made, this device's language; afterwards the account's. */}
        {pathname === "/setup" ? (
          <div className="mt-4 flex justify-center">
            <LanguagePicker />
          </div>
        ) : null}
      </div>
    </div>
  );
}

/**
 * A step of setup: how far setup is once it's shown, its title and the
 * words under it, then what it asks, with `SetupActions` last.
 */
export function SetupStep({
  progress,
  title,
  subtitle,
  headingRef,
  children,
}: {
  progress: number;
  title: string;
  subtitle?: ReactNode;
  /** For a step that puts the focus on its title. */
  headingRef?: Ref<HTMLHeadingElement>;
  children: ReactNode;
}) {
  const frame = useSetupFrame();
  const titleId = useId();
  useEffect(() => frame?.setProgress(progress), [frame, progress]);
  return (
    <section aria-labelledby={titleId} className="space-y-6">
      <header>
        <h1 id={titleId} ref={headingRef} tabIndex={headingRef ? -1 : undefined} className="text-xl font-semibold tracking-tight outline-none">
          {title}
        </h1>
        {subtitle ? <p className="mt-1.5 text-sm text-muted">{subtitle}</p> : null}
      </header>
      {children}
    </section>
  );
}

/** A step's buttons: the way back on the left, the way on at the right. */
export function SetupActions({ back, children }: { back?: ReactNode; children?: ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-2 pt-1">
      <div className="me-auto">{back}</div>
      {children}
    </div>
  );
}

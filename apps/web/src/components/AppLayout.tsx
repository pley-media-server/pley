import { Suspense, useCallback, useEffect, useState, type CSSProperties } from "react";
import { Outlet, ScrollRestoration } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { invalidateWatchState } from "@/api/hooks";
import { AudioPlayerBar } from "@/audio/AudioPlayerBar";
import { useAudioPlayer } from "@/audio/AudioPlayer";
import { cn } from "@/lib/cn";
import { useLocalStorage } from "@/lib/hooks";
import { APP_RESUMED } from "@/lib/shell";
import { ImportTabTitle } from "@/pages/plexImport/ImportTabTitle";
import { ConnectionBanner } from "./ConnectionBanner";
import { ListMenuProvider } from "./lists/ListMenu";
import { Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";
import { PageSpinner } from "./ui";

export function AppLayout() {
  const [drawer, setDrawer] = useState(false);
  // Narrowed to its icons on a wide screen, as this browser last left it.
  const [collapsed, setCollapsed] = useLocalStorage("pley.sidebar.collapsed", false);
  const closeDrawer = useCallback(() => setDrawer(false), []);
  const audio = useAudioPlayer();
  const qc = useQueryClient();

  // The Android app says so when its own player closes: progress and watched marks changed.
  useEffect(() => {
    const refresh = () => invalidateWatchState(qc);
    window.addEventListener(APP_RESUMED, refresh);
    return () => window.removeEventListener(APP_RESUMED, refresh);
  }, [qc]);

  return (
    <div
      className="group/shell min-h-screen"
      // How wide the sidebar is, for what sits beside it (the page, the audio bar).
      style={{ "--sidebar-width": collapsed ? "4rem" : "15rem" } as CSSProperties}
    >
      <ImportTabTitle />
      <Sidebar open={drawer} onClose={closeDrawer} collapsed={collapsed} onToggle={() => setCollapsed((c) => !c)} />
      <div className="flex min-h-screen flex-col transition-[padding] duration-200 motion-reduce:transition-none lg:pl-[var(--sidebar-width)]">
        <TopBar onMenu={() => setDrawer(true)} />
        <ConnectionBanner />
        <main className={cn("flex-1", audio.current && "pb-24")}>
          <ListMenuProvider>
            <Suspense fallback={<PageSpinner />}>
              <Outlet />
            </Suspense>
          </ListMenuProvider>
        </main>
      </div>
      {audio.current ? <AudioPlayerBar /> : null}
      <ScrollRestoration />
    </div>
  );
}

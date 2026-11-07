import { Suspense, useCallback, useEffect, useState } from "react";
import { Outlet, ScrollRestoration } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { invalidateWatchState } from "@/api/hooks";
import { AudioPlayerBar } from "@/audio/AudioPlayerBar";
import { useAudioPlayer } from "@/audio/AudioPlayer";
import { cn } from "@/lib/cn";
import { APP_RESUMED } from "@/lib/shell";
import { ConnectionBanner } from "./ConnectionBanner";
import { ListMenuProvider } from "./lists/ListMenu";
import { Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";
import { PageSpinner } from "./ui";

export function AppLayout() {
  const [drawer, setDrawer] = useState(false);
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
    <div className="group/shell min-h-screen">
      <Sidebar open={drawer} onClose={closeDrawer} />
      <div className="flex min-h-screen flex-col lg:pl-60">
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

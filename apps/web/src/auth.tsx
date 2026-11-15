import { createContext, useContext, useEffect, type ReactNode } from "react";
import { Navigate, Outlet, useLocation } from "react-router";
import { useTranslation } from "react-i18next";
import { errorMessage, isApiError } from "@/api/client";
import { useMe, useServer } from "@/api/hooks";
import { useServerEvents } from "@/api/events";
import type { User } from "@/api/types";
import { AudioPlayerProvider } from "@/audio/AudioPlayer";
import { CastReceiver } from "@/components/CastReceiver";
import { Logo } from "@/components/Logo";
import { RequestNotices } from "@/components/RequestNotices";
import { Button, EmptyState, Spinner } from "@/components/ui";
import { cn } from "@/lib/cn";
import { rememberProfile } from "@/lib/lastProfile";
import { useSetupFrame } from "@/lib/setupFrame";
import { TriangleAlert } from "lucide-react";

const AuthContext = createContext<User | null>(null);

/** Provides the signed-in user (what `<RequireAuth>` does once it is known). */
export function AuthUserProvider({ user, children }: { user: User; children: ReactNode }) {
  return <AuthContext.Provider value={user}>{children}</AuthContext.Provider>;
}

/** The signed-in user where there is one (`null` on the pages shown before signing in). */
export function useOptionalAuth(): User | null {
  return useContext(AuthContext);
}

export function useAuth(): User {
  const user = useContext(AuthContext);
  if (!user) throw new Error("useAuth must be used inside <RequireAuth>");
  return user;
}

export function Splash() {
  // In setup's card, between its steps: the card stays, and waits in place.
  if (useSetupFrame()) {
    return (
      <div className="flex justify-center py-16">
        <Spinner className="size-6" />
      </div>
    );
  }
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-6">
      <Logo className="h-9" />
      <Spinner className="size-6" />
    </div>
  );
}

export function ErrorScreen({ title, message, onRetry }: { title?: string; message: string; onRetry?: () => void }) {
  const { t } = useTranslation();
  const inSetup = useSetupFrame() !== null;
  return (
    <div className={cn("flex items-center justify-center", inSetup ? "py-6" : "min-h-screen")}>
      <EmptyState
        icon={<TriangleAlert />}
        title={title ?? t("states.somethingWentWrong")}
        description={message}
        action={
          onRetry ? (
            <Button variant="primary" onClick={onRetry}>
              {t("actions.tryAgain")}
            </Button>
          ) : undefined
        }
      />
    </div>
  );
}

/** Route guard: needs a finished setup and a valid session. */
export function RequireAuth() {
  const { t } = useTranslation("pages");
  const location = useLocation();
  const server = useServer();
  const setupRequired = server.data?.setup_required ?? false;
  // Asked for together with the server status (not after it): one round trip
  // less before the first page. Its answer is only used once setup is done.
  const me = useMe(!setupRequired);
  const profileId = me.data?.id ?? null;
  useServerEvents(profileId);
  // Signing in here again goes straight in as it (LoginPage).
  useEffect(() => {
    if (profileId !== null) rememberProfile(profileId);
  }, [profileId]);

  if (server.isPending) return <Splash />;
  if (server.isError) {
    return (
      <ErrorScreen
        title={t("auth.cantReachServer")}
        message={errorMessage(server.error)}
        onRetry={() => void server.refetch()}
      />
    );
  }
  if (setupRequired) return <Navigate to="/setup" replace />;
  if (me.isPending) return <Splash />;
  if (me.isError) {
    if (isApiError(me.error, 401)) {
      const next = location.pathname + location.search;
      return <Navigate to={next && next !== "/" ? `/login?next=${encodeURIComponent(next)}` : "/login"} replace />;
    }
    return <ErrorScreen message={errorMessage(me.error)} onRetry={() => void me.refetch()} />;
  }

  return (
    <AuthContext.Provider value={me.data}>
      <AudioPlayerProvider>
        <Outlet />
        {/* Out of the pages, so news heard during a video waits for it to stop; each profile's own, so what one
            heard or held back isn't told to the next. */}
        <RequestNotices key={me.data.id} />
        {/* In the desktop app, what the user's remotes cast here plays here, over any page. */}
        <CastReceiver key={`cast-${me.data.id}`} />
      </AudioPlayerProvider>
    </AuthContext.Provider>
  );
}

export function RequireAdmin({ children }: { children: ReactNode }) {
  const { t } = useTranslation("pages");
  const user = useAuth();
  if (!user.is_admin) {
    return (
      <EmptyState
        className="min-h-[60vh]"
        icon={<TriangleAlert />}
        title={t("auth.adminsOnly")}
        description={t("auth.adminsOnlyHint")}
      />
    );
  }
  return <>{children}</>;
}

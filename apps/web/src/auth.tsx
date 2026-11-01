import { createContext, useContext, type ReactNode } from "react";
import { Navigate, Outlet, useLocation } from "react-router";
import { errorMessage, isApiError } from "@/api/client";
import { useMe, useServer } from "@/api/hooks";
import { useServerEvents } from "@/api/events";
import type { User } from "@/api/types";
import { AudioPlayerProvider } from "@/audio/AudioPlayer";
import { Logo } from "@/components/Logo";
import { RequestNotices } from "@/components/RequestNotices";
import { Button, EmptyState, Spinner } from "@/components/ui";
import { TriangleAlert } from "lucide-react";

const AuthContext = createContext<User | null>(null);

/** Provides the signed-in user (what `<RequireAuth>` does once it is known). */
export function AuthUserProvider({ user, children }: { user: User; children: ReactNode }) {
  return <AuthContext.Provider value={user}>{children}</AuthContext.Provider>;
}

export function useAuth(): User {
  const user = useContext(AuthContext);
  if (!user) throw new Error("useAuth must be used inside <RequireAuth>");
  return user;
}

export function Splash() {
  return (
    <div className="flex min-h-screen flex-col items-center justify-center gap-6">
      <Logo className="h-9" />
      <Spinner className="size-6" />
    </div>
  );
}

export function ErrorScreen({ title = "Something went wrong", message, onRetry }: { title?: string; message: string; onRetry?: () => void }) {
  return (
    <div className="flex min-h-screen items-center justify-center">
      <EmptyState
        icon={<TriangleAlert />}
        title={title}
        description={message}
        action={
          onRetry ? (
            <Button variant="primary" onClick={onRetry}>
              Try again
            </Button>
          ) : undefined
        }
      />
    </div>
  );
}

/** Route guard: needs a finished setup and a valid session. */
export function RequireAuth() {
  const location = useLocation();
  const server = useServer();
  const setupRequired = server.data?.setup_required ?? false;
  // Asked for together with the server status (not after it): one round trip
  // less before the first page. Its answer is only used once setup is done.
  const me = useMe(!setupRequired);
  useServerEvents(me.data?.id ?? null);

  if (server.isPending) return <Splash />;
  if (server.isError) {
    return (
      <ErrorScreen
        title="Can't reach the Pley server"
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
      </AudioPlayerProvider>
    </AuthContext.Provider>
  );
}

export function RequireAdmin({ children }: { children: ReactNode }) {
  const user = useAuth();
  if (!user.is_admin) {
    return (
      <EmptyState
        className="min-h-[60vh]"
        icon={<TriangleAlert />}
        title="Admins only"
        description="You need an administrator account to manage this server."
      />
    );
  }
  return <>{children}</>;
}

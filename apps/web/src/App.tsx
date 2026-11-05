import type { ComponentType } from "react";
import { createBrowserRouter, Navigate, RouterProvider, type Params, type RouteObject } from "react-router";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { LoaderCircle } from "lucide-react";
import { ApiError, setUnauthorizedHandler } from "@/api/client";
import { queries } from "@/api/hooks";
import { liveQueries } from "@/api/livetv";
import { requestQueries } from "@/api/requests";
import { RequireAuth, Splash } from "@/auth";
import { AppLayout } from "@/components/AppLayout";
import { ToastProvider } from "@/components/Toast";
import { LanguageSync } from "@/i18n/LanguageSync";
import { PageSpinner } from "@/components/ui";
import { AccountPage } from "@/pages/AccountPage";
import { HomePage } from "@/pages/HomePage";
import { InvitePage } from "@/pages/InvitePage";
import { ItemPage } from "@/pages/item/ItemPage";
import { LibraryPage } from "@/pages/LibraryPage";
import { LoginPage } from "@/pages/LoginPage";
import { NotFoundPage, RouteErrorPage } from "@/pages/NotFoundPage";
import { PersonPage } from "@/pages/PersonPage";
import { ProfilesPage } from "@/pages/ProfilesPage";
import { SearchPage } from "@/pages/SearchPage";
import { SetupPage } from "@/pages/SetupPage";
import { loadPlayerPage, playerData } from "@/player/preload";

export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      gcTime: 10 * 60_000,
      refetchOnWindowFocus: false,
      retry: (count, err) => {
        if (err instanceof ApiError && [400, 401, 403, 404, 409].includes(err.status)) return false;
        // Not set up on this server (requests, subtitle search): asking again won't change that.
        if (err instanceof ApiError && (err.body as { setup?: unknown } | undefined)?.setup === false) return false;
        return count < 2;
      },
    },
    mutations: { retry: false },
  },
});

function PlayerFallback() {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black">
      <LoaderCircle className="size-12 animate-spin text-white/70" />
    </div>
  );
}

/**
 * A route whose code (the player with hls.js, the admin screens) loads on
 * demand, together with the data the page opens with: the router waits for
 * both before showing it, so it appears complete, without a chain of
 * requests (code, then data) or a spinner in between.
 */
function onDemand<M>(
  load: () => Promise<M>,
  component: (module: M) => ComponentType,
  data: (prefetch: QueryClient["prefetchQuery"], params: Params) => Promise<void>[] = () => [],
  fallback = <PageSpinner />,
): Pick<RouteObject, "lazy" | "loader" | "hydrateFallbackElement"> {
  return {
    lazy: async () => ({ Component: component(await load()) }),
    // Failures are the page's to show (or RequireAdmin's): prefetching never throws.
    loader: async ({ params }) => {
      await Promise.all(data(queryClient.prefetchQuery.bind(queryClient), params));
      return null;
    },
    hydrateFallbackElement: fallback,
  };
}

const router = createBrowserRouter([
  { path: "/setup", element: <SetupPage />, errorElement: <RouteErrorPage /> },
  { path: "/login", element: <LoginPage />, errorElement: <RouteErrorPage /> },
  { path: "/invite/:code", element: <InvitePage />, errorElement: <RouteErrorPage /> },
  // Where the way back to an app leads once its request is over.
  {
    path: "/oauth/over",
    errorElement: <RouteErrorPage />,
    ...onDemand(
      () => import("@/pages/AuthorizePage"),
      (m) => m.RequestOverPage,
      () => [],
      <Splash />,
    ),
  },
  {
    element: <RequireAuth />,
    errorElement: <RouteErrorPage />,
    children: [
      { path: "/profiles", element: <ProfilesPage /> },
      // The OAuth authorization endpoint: an app asks to use the account.
      {
        path: "/oauth/authorize",
        ...onDemand(
          () => import("@/pages/AuthorizePage"),
          (m) => m.AuthorizePage,
          () => [],
          <Splash />,
        ),
      },
      // A TV signs in with the code it shows (OAuth device authorization).
      {
        path: "/link",
        ...onDemand(
          () => import("@/pages/LinkPage"),
          (m) => m.LinkPage,
          () => [],
          <Splash />,
        ),
      },
      {
        path: "/play/:id",
        ...onDemand(
          loadPlayerPage,
          (m) => m.PlayerPage,
          playerData,
          <PlayerFallback />,
        ),
      },
      // A channel, named with its source's short id; `/live/:channelId`
      // (older links) goes on to it.
      ...["/live/:sourceUid/:channelId", "/live/:channelId"].map((path) => ({
        path,
        ...onDemand(
          () => import("@/player/live/LivePlayerPage"),
          (m) => m.LivePlayerPage,
          (prefetch, params) => [prefetch(liveQueries.channel(Number(params.channelId)))],
          <PlayerFallback />,
        ),
      })),
      {
        element: <AppLayout />,
        children: [
          { index: true, element: <HomePage /> },
          { path: "library/:id", element: <LibraryPage /> },
          { path: "item/:id", element: <ItemPage /> },
          { path: "person/:id", element: <PersonPage /> },
          { path: "collection/:id", ...onDemand(() => import("@/pages/collections/CollectionPage"), (m) => m.CollectionPage) },
          { path: "playlists", ...onDemand(() => import("@/pages/playlists/PlaylistsPage"), (m) => m.PlaylistsPage) },
          { path: "playlists/:id", ...onDemand(() => import("@/pages/playlists/PlaylistPage"), (m) => m.PlaylistPage) },
          {
            path: "live",
            ...onDemand(
              () => import("@/pages/livetv/LiveTvPage"),
              (m) => m.LiveTvPage,
              (prefetch) => [prefetch(liveQueries.summary()), prefetch(liveQueries.groups())],
            ),
          },
          { path: "search", element: <SearchPage /> },
          {
            path: "discover",
            ...onDemand(
              () => import("@/pages/requests/DiscoverPage"),
              (m) => m.DiscoverPage,
              (prefetch) => [prefetch(requestQueries.summary())],
            ),
          },
          {
            path: "discover/:kind/:tmdbId",
            ...onDemand(
              () => import("@/pages/requests/TitlePage"),
              (m) => m.TitlePage,
              (prefetch, { kind, tmdbId }) =>
                (kind === "movie" || kind === "show") && Number.isInteger(Number(tmdbId)) && Number(tmdbId) > 0
                  ? [prefetch(requestQueries.summary()), prefetch(requestQueries.title(kind, Number(tmdbId)))]
                  : [],
            ),
          },
          {
            path: "requests",
            ...onDemand(
              () => import("@/pages/requests/RequestsPage"),
              (m) => m.RequestsPage,
              (prefetch) => [prefetch(requestQueries.summary())],
            ),
          },
          { path: "account", element: <AccountPage /> },
          {
            path: "settings",
            ...onDemand(() => import("@/pages/settings/SettingsLayout"), (m) => m.SettingsLayout),
            children: [
              { index: true, element: <Navigate to="dashboard" replace /> },
              {
                path: "libraries",
                ...onDemand(
                  () => import("@/pages/settings/LibrariesSettings"),
                  (m) => m.LibrariesSettings,
                  (prefetch) => [prefetch(queries.sharedLibraries()), prefetch(queries.activity())],
                ),
              },
              {
                path: "livetv",
                ...onDemand(
                  () => import("@/pages/settings/LiveTvSettings"),
                  (m) => m.LiveTvSettings,
                  (prefetch) => [prefetch(liveQueries.sources()), prefetch(liveQueries.settings())],
                ),
              },
              {
                path: "users",
                ...onDemand(
                  () => import("@/pages/settings/UsersSettings"),
                  (m) => m.UsersSettings,
                  (prefetch) => [prefetch(queries.users()), prefetch(queries.libraries())],
                ),
              },
              {
                path: "invitations",
                ...onDemand(
                  () => import("@/pages/settings/InvitesSettings"),
                  (m) => m.InvitesSettings,
                  (prefetch) => [prefetch(queries.invites()), prefetch(queries.libraries())],
                ),
              },
              {
                path: "requests",
                ...onDemand(
                  () => import("@/pages/settings/RequestsSettings"),
                  (m) => m.RequestsSettings,
                  (prefetch) => [prefetch(queries.settings())],
                ),
              },
              {
                path: "server",
                ...onDemand(
                  () => import("@/pages/settings/ServerSettings"),
                  (m) => m.ServerSettings,
                  (prefetch) => [prefetch(queries.settings()), prefetch(queries.system())],
                ),
              },
              {
                path: "transcoder",
                ...onDemand(
                  () => import("@/pages/settings/TranscoderSettings"),
                  (m) => m.TranscoderSettings,
                  (prefetch) => [prefetch(queries.settings()), prefetch(queries.system())],
                ),
              },
              {
                path: "network",
                ...onDemand(
                  () => import("@/pages/settings/NetworkSettings"),
                  (m) => m.NetworkSettings,
                  (prefetch) => [prefetch(queries.network())],
                ),
              },
              {
                path: "dashboard",
                ...onDemand(
                  () => import("@/pages/settings/DashboardSettings"),
                  (m) => m.DashboardSettings,
                  (prefetch) => [prefetch(queries.activity()), prefetch(queries.system())],
                ),
              },
            ],
          },
          { path: "*", element: <NotFoundPage /> },
        ],
      },
    ],
  },
]);

// Any authenticated request that comes back 401 (expired/revoked session) sends the user to the login screen.
setUnauthorizedHandler(() => {
  const { pathname, search } = window.location;
  if (pathname.startsWith("/login") || pathname.startsWith("/setup") || pathname.startsWith("/invite/")) return;
  const next = pathname + search;
  void router
    .navigate(next && next !== "/" ? `/login?next=${encodeURIComponent(next)}` : "/login", { replace: true })
    .then(() => queryClient.clear());
});

export function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <LanguageSync />
      <ToastProvider>
        <RouterProvider router={router} />
      </ToastProvider>
    </QueryClientProvider>
  );
}

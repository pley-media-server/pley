import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { t } from "@/i18n";
import { api } from "./client";
import type { Id, Timestamp } from "./types";

/**
 * An app someone allowed to use an account on pley's own page (OAuth). It
 * acts as `profile`; `approved_by` allowed it.
 */
export interface AppGrant {
  id: Id;
  /** The address of the app's description (its client metadata document). */
  client_id: string;
  /** pley's own apps: their name. Others: what the app calls itself, which pley can't check. */
  name: string | null;
  /** One of pley's own apps, built into the server. */
  first_party: boolean;
  /** The host of `client_id`. */
  host: string;
  /** What it may do: "library", "play", "playlists", "profiles". */
  scopes: string[];
  profile: { id: Id; name: string };
  approved_by: { id: Id; name: string };
  /** Of its last sign-in, from the User-Agent: "Android", "Firefox on Windows". */
  device: string | null;
  ip: string | null;
  created_at: Timestamp;
  last_used_at: Timestamp;
}

export const appKeys = {
  mine: ["me", "apps"] as const,
  user: (id: Id) => ["users", id, "apps"] as const,
};

const SCOPES = ["library", "play", "playlists", "profiles", "requests", "approvals", "activity"] as const;
const isNamedScope = (scope: string): scope is (typeof SCOPES)[number] => (SCOPES as readonly string[]).includes(scope);

/** What an app may do, in a few words: "Libraries · Play · Playlists". */
export function scopesLabel(scopes: string[]): string {
  return scopes.map((s) => (isNamedScope(s) ? t(`apps.scopes.${s}`) : s)).join(" · ");
}

/** How an app is called: pley's own apps by name, others by host. */
export function appLabel(app: AppGrant): string {
  return app.first_party && app.name ? app.name : app.host;
}

/** The apps acting as the signed-in account, or allowed by it. */
export function useMyApps() {
  return useQuery({ queryKey: appKeys.mine, queryFn: () => api.get<AppGrant[]>("/api/me/apps") });
}

export function useSignOutMyApp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: Id) => api.del<void>(`/api/me/apps/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: appKeys.mine }),
  });
}

/** A user's apps (administrators only). */
export function useUserApps(id: Id | null) {
  return useQuery({
    queryKey: appKeys.user(id ?? 0),
    queryFn: () => api.get<AppGrant[]>(`/api/users/${id}/apps`),
    enabled: id != null,
  });
}

export function useSignOutUserApp() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ user, app }: { user: Id; app: Id }) => api.del<void>(`/api/users/${user}/apps/${app}`),
    onSuccess: (_data, { user }) => {
      void qc.invalidateQueries({ queryKey: appKeys.user(user) });
      // The administrator may be the one who allowed it.
      void qc.invalidateQueries({ queryKey: appKeys.mine });
    },
  });
}

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./client";
import type { Id, Timestamp } from "./types";

/**
 * A token for another app that uses one of Pley's compatible APIs: one a
 * Jellyfin app signed in for (named after the app and its device), or one
 * made here for an app, tool or script. It works there only, for what its
 * scopes allow, and from where it may be used.
 */
export interface CompatibilityToken {
  id: Id;
  /** What the person called it. */
  name: string;
  /** What it may do: "play", and "activity" or "manage" (administrators). */
  scopes: string[];
  /** The addresses, networks and `local` it may be used from: anywhere when empty. */
  allowed_from: string[];
  created_at: Timestamp;
  /** `null` until it is first used. */
  last_used_at: Timestamp | null;
  /** It ends then unless it is used before. */
  expires_at: Timestamp;
  /** Of its last use, from the User-Agent: "Infuse/8.1", "Firefox on Windows". */
  device: string | null;
  ip: string | null;
}

/** A new token: the token itself, shown this once. */
export interface CreatedCompatibilityToken extends CompatibilityToken {
  token: string;
}

export const compatibilityTokenKeys = {
  mine: ["me", "compatibility-tokens"] as const,
};

/** What a token may do, in the account page's words, in the order they are shown. */
export const TOKEN_SCOPES: { scope: string; label: string; hint: string; admin: boolean }[] = [
  {
    scope: "play",
    label: "Watch",
    hint: "Browse and play what you can, and keep your place, watched marks and playlists.",
    admin: false,
  },
  {
    scope: "activity",
    label: "See everyone's activity",
    hint: "Everyone's sessions, history and accounts, to read only, for a tool such as Tautulli.",
    admin: true,
  },
  {
    scope: "manage",
    label: "Manage libraries",
    hint: "Refresh libraries and items, edit details, make and change collections, and stop anyone's playback, for a tool such as Kometa. It never manages users.",
    admin: true,
  },
];

/** The most entries a token's "only from" takes. */
export const MAX_ALLOWED_FROM = 16;

/** The entry of a token's "only from" for this computer and the home network. */
export const FROM_LOCAL = "local";

/** What a token may do, in a few words: "Watch · See everyone's activity". */
export function tokenScopesLabel(scopes: string[]): string {
  return TOKEN_SCOPES.filter((s) => scopes.includes(s.scope))
    .map((s) => s.label)
    .join(" · ");
}

/** Where a token may be used from, in a few words: "only from 10.0.0.0/24, home network", or `null` for anywhere. */
export function allowedFromLabel(allowed: string[]): string | null {
  if (allowed.length === 0) return null;
  return `only from ${allowed.map((a) => (a === FROM_LOCAL ? "home network" : a)).join(", ")}`;
}

/** The signed-in account's tokens for other apps, most recently used first. */
export function useCompatibilityTokens() {
  return useQuery({
    queryKey: compatibilityTokenKeys.mine,
    queryFn: () => api.get<CompatibilityToken[]>("/api/me/compatibility-tokens"),
  });
}

export function useCreateCompatibilityToken() {
  const qc = useQueryClient();
  return useMutation({
    /** `scope` is space-separated ("play" without one); `allowed_from` empty for anywhere. */
    mutationFn: (body: { name: string; scope?: string; allowed_from?: string[] }) =>
      api.post<CreatedCompatibilityToken>("/api/me/compatibility-tokens", body),
    onSuccess: () => qc.invalidateQueries({ queryKey: compatibilityTokenKeys.mine }),
  });
}

export function useRevokeCompatibilityToken() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: Id) => api.del<void>(`/api/me/compatibility-tokens/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: compatibilityTokenKeys.mine }),
  });
}

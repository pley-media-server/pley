import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./client";
import type { Id, Timestamp } from "./types";

/**
 * A token for another app or tool that uses Pley's compatibility API
 * (Tautulli, Kometa, Plezy, Infuse): it works there only, for what its
 * scopes allow.
 */
export interface CompatibilityToken {
  id: Id;
  /** What the person called it. */
  name: string;
  /** What it may do: "play", and "activity" or "manage" (administrators). */
  scopes: string[];
  created_at: Timestamp;
  /** `null` until it is first used. */
  last_used_at: Timestamp | null;
  /** It ends then unless it is used before. */
  expires_at: Timestamp;
  /** Of its last use, from the User-Agent: "Tautulli/2.15", "Firefox on Windows". */
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
    hint: "What everyone is playing, and everyone's history, as Tautulli shows them.",
    admin: true,
  },
  {
    scope: "manage",
    label: "Manage libraries",
    hint: "Refresh libraries, edit details and collections, and stop anyone's playback, as Kometa does.",
    admin: true,
  },
];

/** What a token may do, in a few words: "Watch · See everyone's activity". */
export function tokenScopesLabel(scopes: string[]): string {
  return TOKEN_SCOPES.filter((s) => scopes.includes(s.scope))
    .map((s) => s.label)
    .join(" · ");
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
    mutationFn: (body: { name: string; scope: string }) =>
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

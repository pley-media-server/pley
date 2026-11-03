import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "./client";
import type { Id, Timestamp } from "./types";

/**
 * A token for another app that uses Pley's Jellyfin-compatible API: one an
 * app signed in for (named after the app and its device), or one made here
 * for a script. It works there only, to browse and play.
 */
export interface CompatibilityToken {
  id: Id;
  /** What the person called it. */
  name: string;
  /** What it may do: "play". */
  scopes: string[];
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
    mutationFn: (body: { name: string }) =>
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

import { useEffect } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { api, isApiError } from "./client";
import { onServerEvent } from "./events";
import type { Id, LibraryKind } from "./types";

/** A Plex server the account owns (docs/API.md, "Moving from Plex"). */
export interface PlexServer {
  id: string;
  name: string;
  owned: boolean;
  version: string | null;
}

export interface PlexFolder {
  plex: string;
  pley: string;
}

export type PairingStatus = "matched" | "unsure" | "not_in_pley";

export interface PlanLibrary {
  key: string;
  title: string;
  kind: LibraryKind;
  /** Plex's folders. */
  locations: string[];
  pairing: {
    status: PairingStatus;
    /** The Pley library most of its files are in. */
    library_id: Id | null;
    /** Where its folders are for Pley, read off the files found. */
    folders: PlexFolder[];
    /** Plex's folders this server can open. */
    folders_here: string[];
    /** Where Plex's other folders seem to be for Pley, found by their path's end and a file in them. */
    folders_found: PlexFolder[];
  };
}

export interface PlanLimits {
  max_age: number | null;
  max_age_system: string | null;
  show_labels: string[];
  hide_labels: string[];
  /** Labels past the most a profile keeps. */
  left_over: string[];
  /** Parts of Plex's rules Pley can't keep, as Plex wrote them. */
  not_kept: string[];
}

export interface PlanUser {
  id: number;
  title: string;
  /** The account signed in, which comes as the administrator. */
  admin: boolean;
  restricted: boolean;
  /** Plex may ask for their PIN. */
  protected: boolean;
  /** The profile an earlier import made for them. */
  profile_id: Id | null;
  limits: PlanLimits;
}

export interface ImportPlan {
  server: string;
  libraries: PlanLibrary[];
  users: PlanUser[];
}

export interface ImportChoices {
  libraries: { key: string; library_id?: Id; new?: { name: string; paths: string[] } }[];
  folders?: PlexFolder[];
  users?: { id: number; pin?: string }[];
  /** What comes besides the libraries and people; each on when left out. */
  bring?: ImportBring;
}

/** The "What to bring" step's boxes. */
export interface ImportBring {
  watch_history: boolean;
  collections: boolean;
  edits: boolean;
}

export type ImportState = "idle" | "running" | "done" | "failed" | "cancelled";
export type ImportStep =
  | "scanning"
  | "profiles"
  | "matching"
  | "labels"
  | "edits"
  | "collections"
  | "watch_state"
  | "history"
  | "playlists"
  | "fetching"
  | "matching_again";
export type ProfileSkipped = "pin_needed" | "refused" | "no_access";

export interface LibraryReport {
  title: string;
  library_id: Id | null;
  created: boolean;
  matched: number;
  missed: number;
  missed_titles: string[];
}

export interface ProfileReport {
  title: string;
  user_id: Id | null;
  created: boolean;
  skipped: ProfileSkipped | null;
  max_age: number | null;
  show_labels: string[];
  hide_labels: string[];
  left_over: string[];
  not_kept: string[];
  watched: number;
  resumed: number;
  rated: number;
  plays: number;
  /** Their playlists, made or added to. */
  playlists: number;
  /** Plex refused some of their watch history. */
  incomplete: boolean;
  /** Plex refused some of their playlists. */
  playlists_incomplete?: boolean;
}

export interface ImportReport {
  finished_at: number;
  outcome: Exclude<ImportState, "idle" | "running">;
  /** Why it failed, in the server's words (English). */
  error: string | null;
  libraries: LibraryReport[];
  profiles: ProfileReport[];
  labels_added: number;
  dates_moved: number;
  /** Titles matched again to Plex's TMDB entry. */
  matches_fixed: number;
  /** Locked fields brought as edits, by field ("title", "content_rating"). */
  edits: Partial<Record<EditedField, number>>;
  collections_made: number;
  /** Titles put in collections, new or found again. */
  collection_items: number;
  /** Smart collections and playlists, which aren't brought. */
  smart_left: number;
  /** What was chosen; missing from reports kept before the choice existed. */
  bring?: ImportBring;
}

export type EditedField =
  | "title"
  | "sort_title"
  | "original_title"
  | "summary"
  | "tagline"
  | "year"
  | "release_date"
  | "content_rating"
  | "studio"
  | "genres";

export interface ImportProgress {
  state: ImportState;
  step: ImportStep | null;
  done: number;
  total: number;
  error: string | null;
  report: ImportReport | null;
  /** What was found by file has come over; metadata and the titles found by id follow. */
  first_pass_done: boolean;
  /** The library being scanned while the import waits for the scans; absent from servers before it. */
  scan?: ImportScan | null;
}

/** A library's scan the import waits for: its files found, then read (`done` of `total`). */
export interface ImportScan {
  library: string;
  phase: "queued" | "walking" | "probing";
  done: number;
  total: number;
}

export const plexImportKeys = {
  progress: ["import", "plex"] as const,
  servers: ["import", "plex", "servers"] as const,
  plan: ["import", "plex", "plan"] as const,
  signedIn: ["import", "plex", "pin"] as const,
};

/**
 * Where the import is, or the last one's report: fetched again on each
 * `import_changed` event, and every few seconds while it runs in case the
 * event stream is down.
 */
export function usePlexImport() {
  const qc = useQueryClient();
  useEffect(
    () =>
      onServerEvent((ev) => {
        if (ev.type === "import_changed") void qc.invalidateQueries({ queryKey: plexImportKeys.progress, exact: true });
      }),
    [qc],
  );
  return useQuery({
    queryKey: plexImportKeys.progress,
    queryFn: () => api.get<ImportProgress>("/api/import/plex"),
    refetchInterval: (query) => (query.state.data?.state === "running" ? 5_000 : false),
    staleTime: 0,
  });
}

/**
 * Whether Pley holds a Plex sign-in: `true`, `false` while Plex's sign-in
 * page waits for the person (asked again every `poll` ms), or `null` when
 * nobody has started signing in.
 */
export function usePlexSignedIn(poll: number | false) {
  return useQuery({
    queryKey: plexImportKeys.signedIn,
    queryFn: async () => {
      try {
        return (await api.get<{ signed_in: boolean }>("/api/import/plex/pin")).signed_in;
      } catch (e) {
        if (isApiError(e) && e.code === "plex_signed_out") return null;
        throw e;
      }
    },
    // Not after an error (an expired PIN), whose last answer was still false.
    refetchInterval: (query) => (poll && query.state.status === "success" && query.state.data === false ? poll : false),
    staleTime: 0,
    gcTime: 0,
  });
}

/** Starts Plex's sign-in: the page the person allows Pley on. */
export function useStartPlexSignIn() {
  return useMutation({
    mutationFn: () => api.post<{ code: number; auth_url: string }>("/api/import/plex/pin", {}),
  });
}

export function usePlexToken() {
  return useMutation({
    mutationFn: (token: string) => api.post<void>("/api/import/plex/token", { token }),
  });
}

export function usePlexServers(enabled: boolean) {
  return useQuery({
    queryKey: plexImportKeys.servers,
    queryFn: () => api.get<PlexServer[]>("/api/import/plex/servers"),
    enabled,
    staleTime: 0,
    gcTime: 0,
  });
}

export function useChoosePlexServer() {
  // The plan is the caller's to drop when the server is another one.
  return useMutation({
    mutationFn: (server: { id: string } | { address: string }) => api.post<{ name: string }>("/api/import/plex/server", server),
  });
}

export function usePlexPlan(enabled: boolean) {
  return useQuery({
    queryKey: plexImportKeys.plan,
    queryFn: () => api.get<ImportPlan>("/api/import/plex/plan"),
    enabled,
    staleTime: Infinity,
    gcTime: 0,
  });
}

export function useStartPlexImport() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (choices: ImportChoices) => api.post<void>("/api/import/plex/start", choices),
    onSuccess: () => {
      // Running from here, not the last import's report, until the server says more.
      qc.setQueryData<ImportProgress>(plexImportKeys.progress, { state: "running", step: null, done: 0, total: 0, error: null, report: null, first_pass_done: false });
      void qc.invalidateQueries({ queryKey: plexImportKeys.progress, exact: true });
    },
  });
}

/** Stops the import, and forgets the Plex sign-in. */
export function useForgetPlex() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.del<void>("/api/import/plex"),
    onSettled: () => {
      // The sign-in goes with it: importing again starts by signing in.
      qc.setQueryData(plexImportKeys.signedIn, null);
      qc.removeQueries({ queryKey: plexImportKeys.servers });
      qc.removeQueries({ queryKey: plexImportKeys.plan });
      void qc.invalidateQueries({ queryKey: plexImportKeys.progress, exact: true });
    },
  });
}

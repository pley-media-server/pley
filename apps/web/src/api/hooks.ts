import { useCallback, useMemo } from "react";
import { queryOptions, useInfiniteQuery, useMutation, useMutationState, useQuery, useQueryClient, type QueryClient } from "@tanstack/react-query";
import { api, errorMessage, isApiError, qs, request } from "./client";
import { reordered } from "./lists";
import { t } from "@/i18n";
import { received, type RemoteSession } from "@/lib/remote";
import type {
  Activity,
  Backup,
  Backups,
  CastRequest,
  CleanReport,
  Control,
  Device,
  FsListing,
  HistoryPage,
  Hub,
  Id,
  Invite,
  InviteCheck,
  ItemDetail,
  ItemSummary,
  Library,
  LibraryFilters,
  LibraryItemsQuery,
  LibraryItemsResponse,
  LibraryKind,
  ListenChoice,
  LoginResponse,
  NetworkSettings,
  PersonDetail,
  PlayingSession,
  RatingSystems,
  RecoveryCodes,
  RemoteDevice,
  RequestAccess,
  Resources,
  ProfilePatch,
  SearchResults,
  ServerStatus,
  SessionStatus,
  ServiceTest,
  ServiceTestResult,
  Settings,
  StorageUsage,
  SystemInfo,
  TranscodeDir,
  TranscoderStatus,
  Trickplay,
  TwoStepSetup,
  TwoStepStatus,
  User,
  WatchingHours,
} from "./types";

export const keys = {
  server: ["server"] as const,
  me: ["me"] as const,
  libraries: ["libraries"] as const,
  sharedLibraries: ["libraries", "shared"] as const,
  library: (id: Id) => ["library", id] as const,
  libraryItems: (id: Id, q: LibraryItemsQuery) => ["library", id, "items", q] as const,
  libraryFilters: (id: Id) => ["library", id, "filters"] as const,
  libraryHubs: (id: Id) => ["library", id, "hubs"] as const,
  homeHubs: ["hubs", "home"] as const,
  watchlist: ["watchlist"] as const,
  item: (id: Id) => ["item", id] as const,
  children: (id: Id) => ["item", id, "children"] as const,
  onDeck: (id: Id) => ["item", id, "on-deck"] as const,
  next: (id: Id) => ["item", id, "next"] as const,
  search: (q: string, limit: number) => ["search", q, limit] as const,
  person: (id: Id) => ["person", id] as const,
  related: (id: Id) => ["item", id, "related"] as const,
  history: (scope: HistoryScope, pageSize: number) => ["history", scope, pageSize] as const,
  activity: ["activity"] as const,
  sessions: ["sessions"] as const,
  remoteDevices: ["remote", "devices"] as const,
  users: ["users"] as const,
  userDevices: (id: Id) => ["users", id, "devices"] as const,
  profiles: ["profiles"] as const,
  devices: ["devices"] as const,
  invites: ["invites"] as const,
  ratingSystems: ["ratings", "systems"] as const,
  invite: (code: string) => ["invite", code] as const,
  settings: ["settings"] as const,
  network: ["network"] as const,
  twoStep: ["me", "two-step"] as const,
  system: ["system"] as const,
  storage: ["system", "storage"] as const,
  resources: ["system", "resources"] as const,
  backups: ["system", "backups"] as const,
  transcoder: ["transcoder"] as const,
  fs: (path: string | null) => ["fs", path ?? ""] as const,
};

/**
 * Children responses are `ItemSummary`s. Episodes may additionally carry
 * `summary`/`release_date` if the server includes them (not in the v1 contract).
 */
export type ChildItem = ItemSummary & { summary?: string | null; release_date?: string | null };

// ---------------------------------------------------------------------------
// Server / auth
// ---------------------------------------------------------------------------

export function useServer() {
  return useQuery({
    queryKey: keys.server,
    queryFn: () => api.get<ServerStatus>("/api/server", { noAuthRedirect: true }),
    staleTime: 60_000,
    retry: 1,
  });
}

export function useMe(enabled = true) {
  return useQuery({
    queryKey: keys.me,
    queryFn: () => api.get<User>("/api/auth/me", { noAuthRedirect: true }),
    enabled,
    retry: false,
    staleTime: 5 * 60_000,
  });
}

export function useLogin() {
  const qc = useQueryClient();
  return useMutation({
    /**
     * `code`: the second step, for accounts with two-step sign-in; with
     * `setup_id`, the first code of the secret it shows (see TwoStepSetup).
     */
    mutationFn: (body: { username: string; password: string; code?: string; setup_id?: string }) =>
      api.post<LoginResponse>("/api/auth/login", body, { noAuthRedirect: true }),
    onSuccess: (res) => {
      qc.setQueryData(keys.me, res.user);
    },
  });
}

export function useSetup() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: { username: string; password: string; setup_code?: string }) =>
      api.post<LoginResponse>("/api/setup", body, { noAuthRedirect: true }),
    onSuccess: (res) => {
      qc.setQueryData(keys.me, res.user);
      qc.setQueryData<ServerStatus | undefined>(keys.server, (old) => (old ? { ...old, setup_required: false } : old));
    },
  });
}

export function useLogout() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post<void>("/api/auth/logout", undefined, { noAuthRedirect: true }),
    onSettled: () => {
      qc.clear();
    },
  });
}

/** Two-step sign-in for the signed-in user. */
export function useTwoStep(enabled = true) {
  return useQuery({ queryKey: keys.twoStep, queryFn: () => api.get<TwoStepStatus>("/api/me/two-step"), enabled });
}

/**
 * A new secret to add to an authenticator app; two-step sign-in turns on with
 * its first code. While it is on, the secret moves it to another app, and
 * takes a `code` of the current one (or a recovery code) too.
 */
export function useStartTwoStep() {
  return useMutation({
    mutationFn: (body: { password: string; code?: string }) =>
      api.post<TwoStepSetup>("/api/me/two-step/setup", body, { noAuthRedirect: true }),
  });
}

function twoStepChanged(qc: QueryClient, on: boolean) {
  void qc.invalidateQueries({ queryKey: keys.twoStep });
  qc.setQueryData<User | undefined>(keys.me, (u) => (u ? { ...u, two_step: on } : u));
  void qc.invalidateQueries({ queryKey: keys.users });
}

/** Turns two-step sign-in on with a code from the app: the recovery codes, to show once. */
export function useEnableTwoStep() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (code: string) => api.post<RecoveryCodes>("/api/me/two-step", { code }, { noAuthRedirect: true }),
    onSuccess: () => twoStepChanged(qc, true),
  });
}

/** Turns it off: takes the password and a code (or a recovery code). */
export function useDisableTwoStep() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: { password: string; code: string }) =>
      request<void>("/api/me/two-step", { method: "DELETE", body, noAuthRedirect: true }),
    onSuccess: () => twoStepChanged(qc, false),
  });
}

/** New recovery codes; the old ones stop working. */
export function useNewRecoveryCodes() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (password: string) =>
      api.post<RecoveryCodes>("/api/me/two-step/recovery-codes", { password }, { noAuthRedirect: true }),
    onSuccess: () => void qc.invalidateQueries({ queryKey: keys.twoStep }),
  });
}

/** An administrator turns two-step sign-in off for someone who lost their phone and recovery codes. */
export function useResetUserTwoStep() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: Id) => api.del<void>(`/api/users/${id}/two-step`),
    onSuccess: () => void qc.invalidateQueries({ queryKey: keys.users }),
  });
}

export function useChangePassword() {
  return useMutation({
    // A wrong current password must not trigger the global "session expired" redirect.
    mutationFn: (body: { current_password: string; new_password: string }) =>
      api.put<void>("/api/auth/password", body, { noAuthRedirect: true }),
  });
}

// ---------------------------------------------------------------------------
// Profiles and your account
// ---------------------------------------------------------------------------

/** Home profiles this user can switch to (empty for people outside the home). */
export function useProfiles(enabled = true) {
  return useQuery({
    queryKey: keys.profiles,
    queryFn: () => api.get<User[]>("/api/auth/profiles"),
    enabled,
    staleTime: 60_000,
  });
}

export function fetchProfiles(qc: QueryClient) {
  return qc.fetchQuery({ queryKey: keys.profiles, queryFn: () => api.get<User[]>("/api/auth/profiles"), staleTime: 0 });
}

/**
 * Switch this device to another home profile: with what it asks for, or
 * with the `ticket` of a cast from it (components/CastReceiver.tsx).
 * Everything cached belongs to the previous profile, so the cache starts
 * over with the new user. Who is signed in changes in place: the app
 * around the pages hears it at once, as the receiver of a cast needs,
 * without a page change to show it.
 */
export function useSwitchProfile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: { user_id: Id; pin?: string; password?: string; code?: string; setup_id?: string; ticket?: string }) =>
      api.post<LoginResponse>("/api/auth/switch", body, { noAuthRedirect: true }),
    onSuccess: (res) => {
      const kept = (key: readonly unknown[]) => key.length === 1 && (key[0] === keys.me[0] || key[0] === keys.server[0]);
      qc.removeQueries({ predicate: (q) => !kept(q.queryKey) });
      qc.getMutationCache().clear();
      qc.setQueryData(keys.me, res.user);
    },
  });
}

function rememberMe(qc: QueryClient, user: User) {
  qc.setQueryData(keys.me, user);
  void qc.invalidateQueries({ queryKey: keys.users });
  void qc.invalidateQueries({ queryKey: keys.profiles });
}

export function useUpdateProfile() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: ProfilePatch) => api.patch<User>("/api/me", body),
    onSuccess: (user, body) => {
      rememberMe(qc, user);
      // Titles start with the tracks these pick (each file's `selected_*`).
      if ("audio_language" in body || "subtitle_language" in body || "subtitle_mode" in body) {
        void qc.invalidateQueries({ queryKey: ["item"] });
      }
    },
  });
}

export function useUploadAvatar() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (file: Blob) => request<User>("/api/me/avatar", { method: "PUT", rawBody: file }),
    onSuccess: (user) => rememberMe(qc, user),
  });
}

export function useDeleteAvatar() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.del<void>("/api/me/avatar"),
    onSuccess: () => {
      qc.setQueryData<User | undefined>(keys.me, (u) => (u ? { ...u, avatar_url: null } : u));
      void qc.invalidateQueries({ queryKey: keys.users });
      void qc.invalidateQueries({ queryKey: keys.profiles });
    },
  });
}

export function useSetPin() {
  const qc = useQueryClient();
  return useMutation({
    // A wrong password must not look like an expired session.
    mutationFn: (body: { pin: string | null; password: string }) =>
      api.put<void>("/api/me/pin", body, { noAuthRedirect: true }),
    onSuccess: (_data, body) => {
      qc.setQueryData<User | undefined>(keys.me, (u) => (u ? { ...u, has_pin: !!body.pin } : u));
      void qc.invalidateQueries({ queryKey: keys.profiles });
      void qc.invalidateQueries({ queryKey: keys.users });
    },
  });
}

export function useDevices() {
  return useQuery({ queryKey: keys.devices, queryFn: () => api.get<Device[]>("/api/me/devices") });
}

export function useSignOutDevice() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => api.del<void>(`/api/me/devices/${encodeURIComponent(id)}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.devices }),
  });
}

// ---------------------------------------------------------------------------
// Libraries
// ---------------------------------------------------------------------------

/**
 * What pages open with, as query options: their routes fetch it alongside
 * their code (App.tsx), and the pages read it from the cache.
 */
export const queries = {
  /** The libraries in the user's order: their own, else the shared one. */
  libraries: () =>
    queryOptions({ queryKey: keys.libraries, queryFn: () => api.get<Library[]>("/api/libraries"), staleTime: 30_000 }),
  /** The libraries in the order everyone who hasn't sorted their own sees (Settings). */
  sharedLibraries: () =>
    queryOptions({
      queryKey: keys.sharedLibraries,
      queryFn: () => api.get<Library[]>("/api/libraries?order=shared"),
      staleTime: 30_000,
    }),
  users: () => queryOptions({ queryKey: keys.users, queryFn: () => api.get<User[]>("/api/users") }),
  invites: () => queryOptions({ queryKey: keys.invites, queryFn: () => api.get<Invite[]>("/api/invites") }),
  settings: () => queryOptions({ queryKey: keys.settings, queryFn: () => api.get<Settings>("/api/settings") }),
  network: () => queryOptions({ queryKey: keys.network, queryFn: () => api.get<NetworkSettings>("/api/network") }),
  system: () => queryOptions({ queryKey: keys.system, queryFn: () => api.get<SystemInfo>("/api/system"), staleTime: 10_000 }),
  activity: () => queryOptions({ queryKey: keys.activity, queryFn: () => api.get<Activity>("/api/activity"), staleTime: 2_000 }),
  /** The user's own sessions whose players are there: what a remote controls. */
  sessions: () =>
    queryOptions({
      queryKey: keys.sessions,
      queryFn: () => api.get<PlayingSession[]>("/api/sessions").then((list) => received(list, Date.now())),
      staleTime: 2_000,
    }),
  /**
   * The devices that take casts, which a remote lists: the user's own, with
   * what they play, and those of the household's other profiles that switch
   * to the user's for a cast (`household=1`).
   */
  remoteDevices: () =>
    queryOptions({
      queryKey: keys.remoteDevices,
      queryFn: () => api.get<RemoteDevice[]>("/api/remote/devices?household=1"),
      staleTime: 2_000,
    }),
  item: (id: Id) => queryOptions({ queryKey: keys.item(id), queryFn: ({ signal }) => api.get<ItemDetail>(`/api/items/${id}`, { signal }) }),
};

/** The keys of the moves of the shared order ({@link useReorderLibraries}) and of the user's own ({@link useSortMyLibraries}). */
const libraryOrderKey = ["libraries", "order"] as const;
const ownLibraryOrderKey = ["libraries", "own-order"] as const;

/** The newest move of `key` still being saved: the ids in their new order. */
function usePendingOrder(key: readonly unknown[]): Id[] | undefined {
  return useMutationState({
    filters: { mutationKey: key, status: "pending" },
    select: (mutation) => mutation.state.variables as Id[] | undefined,
  }).at(-1);
}

/** Both lists hold the same libraries in the same order. */
function sameOrder(a: Library[] | undefined, b: Library[] | undefined): boolean {
  return !!a && !!b && a.length === b.length && a.every((l, i) => l.id === b[i]?.id);
}

/**
 * The libraries in the user's order, as of the newest move still being
 * saved ({@link useSortMyLibraries}); a move of the shared order shows too
 * while the user sees that one. The cache only ever holds what the server
 * said, so a fetch that lands meanwhile can't undo a move on screen, and a
 * move that isn't saved simply drops out.
 */
export function useLibraries() {
  const qc = useQueryClient();
  const own = usePendingOrder(ownLibraryOrderKey);
  const shared = usePendingOrder(libraryOrderKey);
  const select = useCallback(
    (list: Library[]) => {
      if (own) return reordered(list, own, (l) => l.id);
      if (shared && sameOrder(list, qc.getQueryData<Library[]>(keys.sharedLibraries))) return reordered(list, shared, (l) => l.id);
      return list;
    },
    [own, shared, qc],
  );
  return useQuery({ ...queries.libraries(), select });
}

/** The libraries in the shared order (Settings), as of the newest move still being saved ({@link useReorderLibraries}). */
export function useSharedLibraries() {
  const moving = usePendingOrder(libraryOrderKey);
  const select = useCallback((list: Library[]) => (moving ? reordered(list, moving, (l) => l.id) : list), [moving]);
  return useQuery({ ...queries.sharedLibraries(), select });
}

export function useLibrary(id: Id) {
  const libs = useLibraries();
  return { ...libs, data: libs.data?.find((l) => l.id === id) };
}

export const DEFAULT_ITEMS_LIMIT = 100_000;

/** Items fetched on their own first, so large libraries paint right away. */
export const FIRST_PAGE = 300;

function libraryItemsQuery(id: Id, q: LibraryItemsQuery, enabled: boolean) {
  return {
    queryKey: keys.libraryItems(id, q),
    queryFn: ({ signal }: { signal: AbortSignal }) =>
      api.get<LibraryItemsResponse>(
        `/api/libraries/${id}/items${qs({
          sort: q.sort,
          order: q.order,
          watched: q.watched,
          genre: q.genre,
          year: q.year,
          resolution: q.resolution,
          dynamic_range: q.dynamic_range,
          offset: q.offset,
          limit: q.limit ?? DEFAULT_ITEMS_LIMIT,
        })}`,
        { signal },
      ),
    enabled,
    // Keep showing the list while sorting/filtering, but never another library's items.
    placeholderData: (prev: LibraryItemsResponse | undefined, prevQuery?: { queryKey: readonly unknown[] }) =>
      prevQuery?.queryKey[1] === id ? prev : undefined,
  };
}

export interface LibraryItemsState {
  /** `items` may be shorter than `total` while the rest is loading. */
  data: LibraryItemsResponse | undefined;
  isPending: boolean;
  isError: boolean;
  error: unknown;
  refetch: () => Promise<unknown>;
}

/** A whole library listing: the first {@link FIRST_PAGE} items, then the rest. */
export function useLibraryItems(id: Id, q: LibraryItemsQuery, enabled = true): LibraryItemsState {
  const first = useQuery(libraryItemsQuery(id, { ...q, offset: undefined, limit: FIRST_PAGE }, enabled));
  const fresh = first.data !== undefined && !first.isPlaceholderData;
  const more = fresh && first.data.total > FIRST_PAGE;
  const rest = useQuery(libraryItemsQuery(id, { ...q, offset: FIRST_PAGE, limit: undefined }, enabled && more));

  const data = useMemo(() => {
    if (!first.data) return undefined;
    // While a new sort/filter loads both halves are the old list's; once the
    // new first page is in, never append the old rest to it.
    const tail = !fresh ? rest.data : more && !rest.isPlaceholderData ? rest.data : undefined;
    return tail ? { total: first.data.total, items: first.data.items.concat(tail.items) } : first.data;
  }, [first.data, fresh, more, rest.data, rest.isPlaceholderData]);

  return {
    data,
    isPending: first.isPending,
    isError: first.isError || rest.isError,
    error: first.error ?? rest.error,
    refetch: () => Promise.all([first.refetch(), more ? rest.refetch() : null]),
  };
}

export function useLibraryFilters(id: Id, enabled = true) {
  return useQuery({
    queryKey: keys.libraryFilters(id),
    queryFn: () => api.get<LibraryFilters>(`/api/libraries/${id}/filters`),
    enabled,
    staleTime: 60_000,
  });
}

export function useLibraryHubs(id: Id, enabled = true) {
  return useQuery({
    queryKey: keys.libraryHubs(id),
    queryFn: () => api.get<{ hubs: Hub[] }>(`/api/libraries/${id}/hubs`),
    enabled,
  });
}

export interface LibraryInput {
  name: string;
  kind: LibraryKind;
  paths: string[];
}

export function useCreateLibrary() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: LibraryInput) => api.post<Library>("/api/libraries", body),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.libraries });
      void qc.invalidateQueries({ queryKey: keys.activity });
      void qc.invalidateQueries({ queryKey: ["hubs"] });
    },
  });
}

export function useUpdateLibrary() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...body }: { id: Id; name?: string; paths?: string[] }) =>
      api.patch<Library>(`/api/libraries/${id}`, body),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.libraries });
    },
  });
}

export function useDeleteLibrary() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: Id) => api.del<void>(`/api/libraries/${id}`),
    onSuccess: (_data, id) => {
      qc.removeQueries({ queryKey: keys.library(id) });
      void qc.invalidateQueries({ queryKey: keys.libraries });
      void qc.invalidateQueries({ queryKey: ["hubs"] });
      void qc.invalidateQueries({ queryKey: ["search"] });
    },
  });
}

/**
 * The order libraries are listed in, for everyone who hasn't sorted their
 * own (`ids`: all of them). Lists show it at once, the sidebar too while it
 * shows this order ({@link useLibraries}), and go back to the saved order by
 * themselves if it isn't saved. Moves are saved one at a time, in the order
 * they were made.
 */
export function useReorderLibraries() {
  const qc = useQueryClient();
  // Who is signed in: after a profile switch the cache is someone else's.
  const me = () => qc.getQueryData<User>(keys.me)?.id;
  return useMutation({
    mutationKey: libraryOrderKey,
    scope: { id: "library-order" },
    mutationFn: (ids: Id[]) => api.put<Library[]>("/api/libraries/order", { ids }),
    onMutate: () => ({ by: me() }),
    onSuccess: (data, _ids, move) => {
      if (me() !== move.by) return;
      // Without an order of their own, the user sees this one: at once, then
      // as the server says (an own order can look just like the shared one).
      const followed = sameOrder(qc.getQueryData<Library[]>(keys.libraries), qc.getQueryData<Library[]>(keys.sharedLibraries));
      qc.setQueryData(keys.sharedLibraries, data);
      if (followed) qc.setQueryData(keys.libraries, data);
      void qc.invalidateQueries({ queryKey: keys.libraries, exact: true });
      // The home page has a row for each library, in this order.
      void qc.invalidateQueries({ queryKey: keys.homeHubs });
    },
    onError: () => {
      // Refused, perhaps because libraries came or went meanwhile: once the
      // last move is settled, see how they are now.
      if (qc.isMutating({ mutationKey: libraryOrderKey }) <= 1) void qc.invalidateQueries({ queryKey: keys.sharedLibraries, exact: true });
    },
  });
}

/**
 * The order the user sees their libraries in, theirs alone (`ids`: every
 * library they see). Shown at once ({@link useLibraries}) and saved one move
 * at a time, as {@link useReorderLibraries} does the shared order.
 */
export function useSortMyLibraries() {
  const qc = useQueryClient();
  const me = () => qc.getQueryData<User>(keys.me)?.id;
  return useMutation({
    mutationKey: ownLibraryOrderKey,
    scope: { id: "own-library-order" },
    mutationFn: (ids: Id[]) => api.put<Library[]>("/api/me/libraries/order", { ids }),
    onMutate: () => ({ by: me() }),
    onSuccess: (data, _ids, move) => {
      if (me() !== move.by) return;
      qc.setQueryData(keys.libraries, data);
      // The home page has a row for each library, in this order.
      void qc.invalidateQueries({ queryKey: keys.homeHubs });
    },
    onError: () => {
      if (qc.isMutating({ mutationKey: ownLibraryOrderKey }) <= 1) void qc.invalidateQueries({ queryKey: keys.libraries, exact: true });
    },
  });
}

/** The user sees their libraries in the shared order again: after the moves still being saved. */
export function useForgetMyLibraryOrder() {
  const qc = useQueryClient();
  const me = () => qc.getQueryData<User>(keys.me)?.id;
  return useMutation({
    scope: { id: "own-library-order" },
    mutationFn: () => api.del<Library[]>("/api/me/libraries/order"),
    onMutate: () => ({ by: me() }),
    onSuccess: (data, _vars, forgot) => {
      if (me() !== forgot.by) return;
      qc.setQueryData(keys.libraries, data);
      void qc.invalidateQueries({ queryKey: keys.homeHubs });
    },
  });
}

export function useScanLibrary() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, force }: { id: Id; force?: boolean }) =>
      api.post<void>(`/api/libraries/${id}/scan`, { force: !!force }),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.activity });
      void qc.invalidateQueries({ queryKey: keys.libraries });
    },
  });
}

export function useScanAll() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post<void>("/api/libraries/scan"),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.activity });
      void qc.invalidateQueries({ queryKey: keys.libraries });
    },
  });
}

// ---------------------------------------------------------------------------
// Items
// ---------------------------------------------------------------------------

export function useItem(id: Id | null | undefined) {
  return useQuery({ ...queries.item(id ?? 0), enabled: id != null && Number.isFinite(id) && id > 0 });
}

/** Seek previews of a file. They are made in the background on first use, so keep asking until they exist. */
export function useTrickplay(fileId: Id | null | undefined) {
  return useQuery({
    queryKey: ["trickplay", fileId ?? 0],
    queryFn: ({ signal }) => api.get<Trickplay>(`/api/files/${fileId}/trickplay`, { signal }),
    enabled: fileId != null && fileId > 0,
    retry: false,
    staleTime: Infinity,
    refetchInterval: (query) => (query.state.data ? false : 20_000),
  });
}

export function useChildren(id: Id | null | undefined, enabled = true) {
  return useQuery({
    queryKey: keys.children(id ?? 0),
    queryFn: ({ signal }) => api.get<{ items: ChildItem[] }>(`/api/items/${id}/children`, { signal }),
    enabled: enabled && id != null && id > 0,
    select: (d) => d.items,
  });
}

export function fetchChildren(qc: QueryClient, id: Id): Promise<ChildItem[]> {
  return qc
    .fetchQuery({
      queryKey: keys.children(id),
      queryFn: () => api.get<{ items: ChildItem[] }>(`/api/items/${id}/children`),
      staleTime: 30_000,
    })
    .then((d) => d.items);
}

export function useOnDeck(id: Id | null | undefined, enabled = true) {
  return useQuery({
    queryKey: keys.onDeck(id ?? 0),
    queryFn: () => api.get<{ item: ItemSummary | null }>(`/api/items/${id}/on-deck`),
    enabled: enabled && id != null && id > 0,
    select: (d) => d.item,
  });
}

export function fetchOnDeck(qc: QueryClient, id: Id) {
  return qc
    .fetchQuery({
      queryKey: keys.onDeck(id),
      queryFn: () => api.get<{ item: ItemSummary | null }>(`/api/items/${id}/on-deck`),
      staleTime: 0,
    })
    .then((d) => d.item);
}

export function fetchNext(qc: QueryClient, id: Id) {
  return qc
    .fetchQuery({
      queryKey: keys.next(id),
      queryFn: () => api.get<{ item: ItemSummary | null }>(`/api/items/${id}/next`),
      staleTime: 0,
    })
    .then((d) => d.item);
}

/** Invalidate everything that shows watch state. */
export function invalidateWatchState(qc: QueryClient) {
  void qc.invalidateQueries({ queryKey: ["item"] });
  void qc.invalidateQueries({ queryKey: ["library"] });
  void qc.invalidateQueries({ queryKey: ["hubs"] });
  void qc.invalidateQueries({ queryKey: ["search"] });
}

export function useSetWatched() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, watched }: { id: Id; watched: boolean }) =>
      api.post<void>(`/api/items/${id}/${watched ? "watched" : "unwatched"}`),
    onSuccess: () => invalidateWatchState(qc),
  });
}

export function useSetRating() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, rating }: { id: Id; rating: number | null }) => api.put<void>(`/api/items/${id}/rating`, { rating }),
    onSuccess: () => invalidateWatchState(qc),
  });
}

export function useRefreshItem() {
  return useMutation({
    mutationFn: (id: Id) => api.post<void>(`/api/items/${id}/refresh`),
  });
}

// ---------------------------------------------------------------------------
// Home / search
// ---------------------------------------------------------------------------

export function useHomeHubs() {
  return useQuery({
    queryKey: keys.homeHubs,
    queryFn: () => api.get<{ hubs: Hub[] }>("/api/hubs/home"),
    select: (d) => d.hubs,
  });
}

/** Every result the server gives (up to 200 of each kind); the search page shows a few of each until asked. */
export function useSearch(q: string, limit = 200) {
  const trimmed = q.trim();
  return useQuery({
    queryKey: keys.search(trimmed, limit),
    queryFn: ({ signal }) => api.get<SearchResults>(`/api/search${qs({ q: trimmed, limit })}`, { signal }),
    enabled: trimmed.length > 0,
    placeholderData: (prev) => prev,
  });
}

// ---------------------------------------------------------------------------
// Continue Watching and My List
// ---------------------------------------------------------------------------

type HubsData = { hubs: Hub[] };

/** Home's hubs and every library's. */
const hubQueries = {
  predicate: (q: { queryKey: readonly unknown[] }) =>
    q.queryKey[0] === "hubs" || (q.queryKey[0] === "library" && q.queryKey[2] === "hubs"),
};

/** The hubs without `item` in their Continue Watching (an episode takes its show's other episodes along). */
export function withoutContinuing(data: HubsData, item: Pick<ItemSummary, "id" | "kind" | "grandparent_id">): HubsData {
  const show = item.kind === "episode" ? item.grandparent_id : null;
  const gone = (i: ItemSummary) => i.id === item.id || (show != null && i.grandparent_id === show);
  return {
    ...data,
    hubs: data.hubs.map((h) => (h.key.startsWith("continue") && h.items.some(gone) ? { ...h, items: h.items.filter((i) => !gone(i)) } : h)),
  };
}

/**
 * Hides a movie or show from Continue Watching until it is played again.
 * The card leaves every Continue Watching at once, and comes back if the
 * server says no.
 */
export function useRemoveFromContinueWatching() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (item: ItemSummary) => api.del<void>(`/api/continue-watching/${item.id}`),
    onMutate: async (item) => {
      await qc.cancelQueries(hubQueries);
      const before = qc.getQueriesData<HubsData>(hubQueries);
      qc.setQueriesData<HubsData>(hubQueries, (old) => (old ? withoutContinuing(old, item) : old));
      return { before };
    },
    onError: (_err, _item, saved) => {
      for (const [key, data] of saved?.before ?? []) qc.setQueryData(key, data);
    },
    onSettled: () => qc.invalidateQueries(hubQueries),
  });
}

/** What can go on My List: movies and shows. */
export function listable(item: Pick<ItemSummary, "kind">): boolean {
  return item.kind === "movie" || item.kind === "show";
}

/** My List, newest added first. */
export function useWatchlist(enabled = true) {
  return useQuery({
    queryKey: keys.watchlist,
    queryFn: ({ signal }) => api.get<{ items: ItemSummary[] }>("/api/watchlist", { signal }),
    select: (d) => d.items,
    enabled,
    staleTime: 30_000,
  });
}

/** Puts a movie or show on My List or takes it off: shown at once, undone if the server says no. */
export function useSetInWatchlist() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ item, on }: { item: ItemSummary; on: boolean }) =>
      on ? api.put<void>(`/api/watchlist/${item.id}`) : api.del<void>(`/api/watchlist/${item.id}`),
    onMutate: async ({ item, on }) => {
      await Promise.all([
        qc.cancelQueries({ queryKey: keys.watchlist }),
        qc.cancelQueries({ queryKey: keys.item(item.id), exact: true }),
      ]);
      const list = qc.getQueryData<{ items: ItemSummary[] }>(keys.watchlist);
      const detail = qc.getQueryData<ItemDetail>(keys.item(item.id));
      if (list) {
        const rest = list.items.filter((i) => i.id !== item.id);
        qc.setQueryData(keys.watchlist, { items: on ? [item, ...rest] : rest });
      }
      if (detail) qc.setQueryData(keys.item(item.id), { ...detail, in_watchlist: on });
      return { list, detail };
    },
    onError: (_err, { item }, saved) => {
      if (saved?.list) qc.setQueryData(keys.watchlist, saved.list);
      if (saved?.detail) qc.setQueryData(keys.item(item.id), saved.detail);
    },
    onSettled: (_data, _err, { item }) => {
      void qc.invalidateQueries({ queryKey: keys.watchlist });
      void qc.invalidateQueries({ queryKey: keys.item(item.id), exact: true });
      // Home's My List hub.
      void qc.invalidateQueries({ queryKey: keys.homeHubs });
    },
  });
}

/** Whose plays: your own (default), one user's or everyone's (administrators). */
export interface HistoryScope {
  all?: boolean;
  userId?: Id;
}

export function useHistory(scope: HistoryScope, pageSize = 20) {
  return useInfiniteQuery({
    queryKey: keys.history(scope, pageSize),
    queryFn: ({ pageParam, signal }) =>
      api.get<HistoryPage>(
        `/api/history${qs({ all: scope.all ? 1 : undefined, user_id: scope.userId, offset: pageParam, limit: pageSize })}`,
        { signal },
      ),
    initialPageParam: 0,
    getNextPageParam: (last, pages) => {
      const loaded = pages.reduce((n, p) => n + p.entries.length, 0);
      return loaded < last.total ? loaded : undefined;
    },
  });
}

export function useDeleteHistory() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: Id) => api.del<void>(`/api/history/${id}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["history"] }),
  });
}

/** "More Like This" for a movie or show. */
export function useRelated(id: Id, enabled = true) {
  return useQuery({
    queryKey: keys.related(id),
    queryFn: ({ signal }) => api.get<{ items: ItemSummary[] }>(`/api/items/${id}/related`, { signal }),
    select: (d) => d.items,
    enabled,
    staleTime: 5 * 60_000,
  });
}

export function usePerson(id: Id) {
  return useQuery({
    queryKey: keys.person(id),
    queryFn: ({ signal }) => api.get<PersonDetail>(`/api/people/${id}`, { signal }),
    enabled: Number.isFinite(id) && id > 0,
  });
}

// ---------------------------------------------------------------------------
// Activity
// ---------------------------------------------------------------------------

export function useActivity(opts: { poll?: number | false } = {}) {
  return useQuery({
    ...queries.activity(),
    refetchInterval: (query) => {
      if (opts.poll) return opts.poll;
      const scans = query.state.data?.scans ?? [];
      return scans.some((s) => s.phase !== "done") ? 4_000 : false;
    },
    refetchIntervalInBackground: false,
  });
}

/**
 * Ends someone's playback (administrators; anyone their own): the player
 * stops and shows `message`.
 */
export function useStopSession() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ sessionId, message }: { sessionId: string; message?: string }) =>
      request<void>(`/api/sessions/${encodeURIComponent(sessionId)}`, {
        method: "DELETE",
        body: message ? { message } : undefined,
      }),
    onSuccess: (_data, { sessionId }) => {
      qc.setQueryData<Activity>(keys.activity, (old) =>
        old ? { ...old, sessions: old.sessions.filter((s) => s.session_id !== sessionId) } : old,
      );
      dropSession(qc, sessionId);
      void qc.invalidateQueries({ queryKey: keys.activity });
    },
  });
}

/** A session gone from the user's own list until it is fetched again. */
function dropSession(qc: QueryClient, sessionId: string) {
  qc.setQueryData<RemoteSession[]>(keys.sessions, (old) => old?.filter((s) => s.session_id !== sessionId));
  void qc.invalidateQueries({ queryKey: keys.sessions });
}

/**
 * The user's own sessions on their devices, this tab's included (see
 * `lib/playingHere`). Fetched again as sessions change (the event stream).
 */
export function usePlayingSessions() {
  return useQuery(queries.sessions());
}

/**
 * The devices that take casts (apps with the event stream open as a
 * receiver): the user's own, and those their household's other profiles are
 * on. Fetched again as they come and go, which the event stream says with
 * `sessions_changed`.
 */
export function useRemoteDevices() {
  return useQuery(queries.remoteDevices());
}

export interface SessionCommand {
  sessionId: string;
  command: Control;
  /** `seek`: where to (ms). */
  positionMs?: number;
  /** `skip`: how far (ms), back when negative. */
  offsetMs?: number;
  /** `audio` and `subtitles`: the track's stream index (subtitles -1: off). */
  streamIndex?: number;
  /** `quality`: the id of one of the session's `qualities`. */
  quality?: string;
}

/**
 * Sends one of the user's own players a command. The player acts on it and
 * reports, which brings `sessions_changed`: its new state shows from there.
 */
export function useSessionCommand() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ sessionId, command, positionMs, offsetMs, streamIndex, quality }: SessionCommand) =>
      request<void>(`/api/sessions/${encodeURIComponent(sessionId)}/command`, {
        method: "POST",
        body: { command, position_ms: positionMs, offset_ms: offsetMs, stream_index: streamIndex, quality },
      }),
    onError: (err, { sessionId }) => {
      // Its player is gone, or no longer obeys that: the list says what is left.
      if (isApiError(err, 404)) dropSession(qc, sessionId);
      else if (isApiError(err, 409)) void qc.invalidateQueries({ queryKey: keys.sessions });
    },
  });
}

/** What a remote says when `device` didn't take a command. */
export function sessionCommandError(err: unknown, device: string | null): string {
  const it = device ?? t("remote.thisDevice");
  if (isApiError(err, 404)) return t("remote.notPlaying", { device: it });
  if (isApiError(err, 409)) return t("remote.cantDoThat", { device: it });
  if (isApiError(err, 429)) return t("remote.tooMany");
  return errorMessage(err);
}

/**
 * Sends a device that takes casts something to play (`receiverId`: its
 * `receiver_id`). It plays it as its own Play would, and its session shows
 * in the list as it starts. One on another profile switches to the user's
 * first: the cast names that profile, and the user's PIN when they have one.
 * A cast isn't kept once it is done with (`gcTime: 0`, and the caller's
 * `reset`): the PIN would stay in the mutation cache with it.
 */
export function useCast() {
  const qc = useQueryClient();
  return useMutation({
    gcTime: 0,
    mutationFn: ({ receiverId, cast }: { receiverId: string; cast: CastRequest }) =>
      api.post<void>(`/api/remote/devices/${encodeURIComponent(receiverId)}/cast`, cast),
    onError: (err) => {
      // It went, or takes other things now: the list says what is left.
      if (isApiError(err, 404) || isApiError(err, 409)) void qc.invalidateQueries({ queryKey: keys.remoteDevices });
    },
  });
}

/** What "Play on…" says when `device` didn't take a cast. */
export function castError(err: unknown, device: string): string {
  if (isApiError(err, 404)) return t("remote.notOn", { device });
  if (isApiError(err, 409)) return t("remote.cantPlay", { device });
  if (isApiError(err, 429)) return t("remote.tooMany");
  return errorMessage(err);
}

// ---------------------------------------------------------------------------
// Users (admin)
// ---------------------------------------------------------------------------

export function useUsers() {
  return useQuery(queries.users());
}

export interface NewUserInput {
  username: string;
  password?: string;
  display_name?: string;
  is_admin?: boolean;
  managed?: boolean;
  home?: boolean;
  all_libraries?: boolean;
  library_ids?: Id[];
  max_age?: number | null;
  max_age_system?: string | null;
  pin?: string;
  request_access?: RequestAccess;
}

export function useCreateUser() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: NewUserInput) => api.post<User>("/api/users", body),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.users });
      void qc.invalidateQueries({ queryKey: keys.profiles });
    },
  });
}

/** Only the fields present are changed; `null` clears nullable ones. */
export interface UserPatch {
  password?: string;
  display_name?: string | null;
  is_admin?: boolean;
  home?: boolean;
  disabled?: boolean;
  all_libraries?: boolean;
  library_ids?: Id[];
  max_age?: number | null;
  max_age_system?: string | null;
  pin?: string | null;
  request_access?: RequestAccess;
}

export function useUpdateUser() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, ...body }: UserPatch & { id: Id }) => api.patch<User>(`/api/users/${id}`, body),
    onSuccess: (user) => {
      void qc.invalidateQueries({ queryKey: keys.users });
      void qc.invalidateQueries({ queryKey: keys.profiles });
      qc.setQueryData<User | undefined>(keys.me, (me) => (me && me.id === user.id ? user : me));
    },
  });
}

export function useDeleteUser() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: Id) => api.del<void>(`/api/users/${id}`),
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: keys.users });
      void qc.invalidateQueries({ queryKey: keys.profiles });
    },
  });
}

export function useUserDevices(id: Id | null) {
  return useQuery({
    queryKey: keys.userDevices(id ?? 0),
    queryFn: () => api.get<Device[]>(`/api/users/${id}/devices`),
    enabled: id != null,
  });
}

export function useSignOutEverywhere() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (id: Id) => api.del<void>(`/api/users/${id}/devices`),
    onSuccess: (_data, id) => qc.invalidateQueries({ queryKey: keys.userDevices(id) }),
  });
}

// ---------------------------------------------------------------------------
// Invitations
// ---------------------------------------------------------------------------

/** The rating systems age limits are picked and shown in. */
export function useRatingSystems() {
  return useQuery({
    queryKey: keys.ratingSystems,
    queryFn: () => api.get<RatingSystems>("/api/ratings/systems"),
    staleTime: 5 * 60_000,
  });
}

export function useInvites() {
  return useQuery(queries.invites());
}

export interface NewInviteInput {
  expires_in_days?: number;
  uses?: number;
  all_libraries?: boolean;
  library_ids?: Id[];
  max_age?: number | null;
  max_age_system?: string | null;
  note?: string;
}

export function useCreateInvite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: NewInviteInput) => api.post<Invite>("/api/invites", body),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.invites }),
  });
}

export function useDeleteInvite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (code: string) => api.del<void>(`/api/invites/${encodeURIComponent(code)}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.invites }),
  });
}

/** Public: is this invitation still valid? */
export function useInviteCheck(code: string) {
  return useQuery({
    queryKey: keys.invite(code),
    queryFn: () => api.get<InviteCheck>(`/api/invites/${encodeURIComponent(code)}`, { noAuthRedirect: true }),
    retry: false,
  });
}

export function useAcceptInvite(code: string) {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: { username: string; password: string; display_name?: string }) =>
      api.post<LoginResponse>(`/api/invites/${encodeURIComponent(code)}/accept`, body, { noAuthRedirect: true }),
    onSuccess: (res) => {
      const server = qc.getQueryData<ServerStatus>(keys.server);
      qc.clear();
      if (server) qc.setQueryData(keys.server, server);
      qc.setQueryData(keys.me, res.user);
    },
  });
}

// ---------------------------------------------------------------------------
// System (admin)
// ---------------------------------------------------------------------------

/** Server settings (administrators only). */
export function useSettings(enabled = true) {
  return useQuery({ ...queries.settings(), enabled });
}

export function useSaveSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: Partial<Settings>) => api.put<Settings>("/api/settings", body),
    onSuccess: (data) => {
      qc.setQueryData(keys.settings, data);
      void qc.invalidateQueries({ queryKey: keys.system });
      void qc.invalidateQueries({ queryKey: keys.server });
    },
  });
}

/** Where pley listens and could; again every 5 s while an address waits to be listened on. */
export function useNetwork() {
  return useQuery({
    ...queries.network(),
    refetchInterval: (query) => (query.state.data?.listeners.some((l) => l.error) ? 5_000 : false),
  });
}

/** Listen elsewhere: answers once the new addresses listen, or have tried for a moment. */
export function useSaveNetwork() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (body: ListenChoice) => api.put<NetworkSettings>("/api/network", body),
    onSuccess: (data) => qc.setQueryData(keys.network, data),
  });
}

/** Chooses the transcoder's work folder from pley's next start (`null`: the default one). */
export function useSaveTranscodeDir() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (path: string | null) => api.put<TranscodeDir>("/api/system/transcode-dir", { path }),
    onSuccess: (data) => {
      qc.setQueryData<SystemInfo>(keys.system, (system) => (system ? { ...system, transcode_dir: data } : system));
    },
  });
}

/** Whether a metadata service accepts a key and answers, or the transcoder does. */
export function useTestService() {
  return useMutation({
    mutationFn: (body: ServiceTest) => api.post<ServiceTestResult>("/api/settings/test", body),
  });
}

export function useStorageUsage() {
  return useQuery({ queryKey: keys.storage, queryFn: () => api.get<StorageUsage>("/api/system/storage"), staleTime: 30_000 });
}

export function useCleanStorage() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post<CleanReport>("/api/system/storage/clean"),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.storage }),
  });
}

export function useBackups() {
  return useQuery({ queryKey: keys.backups, queryFn: () => api.get<Backups>("/api/system/backups") });
}

export function useCreateBackup() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post<Backup>("/api/system/backups"),
    onSuccess: () => qc.invalidateQueries({ queryKey: keys.backups }),
  });
}

export function useSystemInfo() {
  return useQuery(queries.system());
}

/** How a playing session plays, with its transcode's live numbers: every 2 s while `enabled`. */
export function usePlaybackStatus(sessionId: string | null, enabled: boolean) {
  return useQuery({
    queryKey: ["playback", sessionId],
    queryFn: () => api.get<SessionStatus>(`/api/playback/${encodeURIComponent(sessionId ?? "")}`),
    enabled: enabled && !!sessionId,
    refetchInterval: 2_000,
    refetchIntervalInBackground: false,
    retry: false,
  });
}

export function useTranscoderStatus(pollMs: number | false = 2_500) {
  return useQuery({
    queryKey: keys.transcoder,
    queryFn: () => api.get<TranscoderStatus>("/api/transcoder"),
    refetchInterval: pollMs,
    refetchIntervalInBackground: false,
    staleTime: 1_000,
    retry: 1,
  });
}

/** When library work runs: the week as the server learned it, and the hours it may begin in (admins only). */
export function useWatchingHours(enabled = true) {
  return useQuery({
    queryKey: ["library-work"],
    queryFn: () => api.get<WatchingHours>("/api/system/library-work"),
    enabled,
    refetchInterval: 60_000,
    refetchIntervalInBackground: false,
  });
}

/** Samples kept, as the server keeps them: five minutes of them. */
const RESOURCE_SAMPLES = 150;

/**
 * The processors' and memory's use: the last five minutes at first, then
 * every 2 s only the samples taken since the newest one here.
 */
export function useResources(pollMs = 2_000) {
  const qc = useQueryClient();
  return useQuery({
    queryKey: keys.resources,
    queryFn: async () => {
      const held = qc.getQueryData<Resources>(keys.resources);
      const last = held?.samples.at(-1)?.at;
      const fresh = await api.get<Resources>(`/api/system/resources${qs({ since: last })}`);
      if (!held || last == null) return fresh;
      return { ...fresh, samples: [...held.samples, ...fresh.samples].slice(-RESOURCE_SAMPLES) };
    },
    refetchInterval: pollMs,
    refetchIntervalInBackground: false,
    retry: 1,
  });
}

export function useFs(path: string | null, enabled = true) {
  return useQuery({
    queryKey: keys.fs(path),
    // A POST: over plain HTTP the server can't tell another site's GET from its own page's.
    queryFn: () => api.post<FsListing>("/api/fs", { path }),
    enabled,
    staleTime: 5_000,
    retry: false,
  });
}

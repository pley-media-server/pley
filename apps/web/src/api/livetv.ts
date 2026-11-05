// Live TV (docs/API.md, "Live TV"): types and React Query hooks.

import { queryOptions, useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { deviceFields } from "@/lib/castReceiver";
import { api, qs } from "./client";
import type { Control, Id, Timestamp } from "./types";

export interface LiveSummary {
  /** Live TV is open to this account and has channels. */
  available: boolean;
  /** Open to this account (it may have no channels yet). */
  allowed: boolean;
  channels: number;
  /** Sources set up (administrators only; 0 for others). */
  sources: number;
  /** The channels watched last, newest first. */
  recent: Id[];
}

export interface Program {
  channel_id: Id;
  start: Timestamp;
  stop: Timestamp;
  title: string;
  subtitle: string | null;
  description: string | null;
  categories: string[];
  /** Same-origin picture URL, when the guide has one. */
  image: string | null;
  season: number | null;
  episode: number | null;
  is_new: boolean;
  is_live: boolean;
  rating: string | null;
}

export interface Channel {
  id: Id;
  source_id: Id;
  /** Its source's short id, which its address carries: `/live/{source_uid}/{id}`. */
  source_uid: string;
  name: string;
  number: string | null;
  group: string;
  /** Same-origin logo URL, when the channel has one. */
  logo: string | null;
  favorite: boolean;
  /** The guide has programmes for it. */
  has_guide: boolean;
  now: Program | null;
  next: Program | null;
}

export interface Guide {
  from: Timestamp;
  to: Timestamp;
  /** Programmes by channel id. */
  programs: Record<string, Program[]>;
}

/** A group of channels as the provider names it. */
export interface LiveGroup {
  name: string;
  /** The region its name begins with ("UK" of "UK| Sport"). */
  region: string | null;
  /** The name without its region ("Sport"). */
  label: string;
  count: number;
}

export interface LiveGroups {
  /** Channels shown in all. */
  total: number;
  favorites: number;
  recent: number;
  groups: LiveGroup[];
}

/** Which channels a list has. */
export type LiveCategory = { kind: "all" } | { kind: "favorites" } | { kind: "recent" } | { kind: "group"; group: string };

export interface ChannelPage {
  /** Channels in the whole list. */
  total: number;
  offset: number;
  channels: Channel[];
}

/** Channels asked for at once. */
export const CHANNEL_PAGE = 100;

/** "all", "favorites", "recent" or "group:<name>": in URLs and storage. */
export function categoryKey(c: LiveCategory): string {
  return c.kind === "group" ? `group:${c.group}` : c.kind;
}

export function parseCategory(key: string | null | undefined): LiveCategory {
  if (key === "favorites" || key === "recent") return { kind: key };
  if (key?.startsWith("group:")) return { kind: "group", group: key.slice(6) };
  return { kind: "all" };
}

function browseParams(category: LiveCategory, search: string) {
  return {
    category: category.kind,
    group: category.kind === "group" ? category.group : undefined,
    q: search.trim() || undefined,
  };
}

export interface Watching {
  session_id: string;
  /** The live HLS playlist. */
  url: string;
  kind: "hls" | "stream";
  channel: Channel;
}

// ---------------------------------------------------------------- admin

export type LiveAccess = "everyone" | "home" | "admins";

export interface LiveSettings {
  access: LiveAccess;
  /** Profiles with an age limit see Live TV too. */
  age_limited: boolean;
  deinterlace: boolean;
  /** The most lines a transcoded channel has; null: as many as it has. */
  max_height: number | null;
  timeshift_minutes: number;
  refresh_hours: number;
  guide_days: number;
  /** Channels that didn't play, and a provider's divider rows, are left out. */
  hide_offline: boolean;
  /** People who watch Live TV whatever `access` says, and people who don't (administrators always do). */
  allowed_users: Id[];
  denied_users: Id[];
}

export type SourceKind = "m3u" | "xtream";

export interface XtreamAccount {
  status: string;
  expires_at: Timestamp | null;
  max_connections: number | null;
  active_connections: number | null;
  trial: boolean;
}

export interface LiveSource {
  id: Id;
  /** Its short id, which its channels' addresses carry. */
  uid: string;
  name: string;
  kind: SourceKind;
  url: string;
  username: string;
  password_set: boolean;
  guide_url: string;
  user_agent: string;
  /** 0: no limit. */
  max_connections: number;
  enabled: boolean;
  created_at: Timestamp;
  channels_at: Timestamp | null;
  channels_error: string | null;
  guide_at: Timestamp | null;
  guide_error: string | null;
  account: XtreamAccount | null;
  channel_count: number;
  hidden_count: number;
  guided_count: number;
  connections_in_use: number;
  refreshing: boolean;
}

export interface SourceInput {
  name: string;
  kind: SourceKind;
  url: string;
  username: string;
  /** Omitted: the saved one is kept. */
  password?: string;
  guide_url: string;
  user_agent: string;
  max_connections: number;
  enabled: boolean;
}

export interface SourceTest {
  channels: number;
  groups: number;
  guide_url: string | null;
  account: XtreamAccount | null;
}

export interface LineupChannel {
  id: Id;
  source_id: Id;
  name: string;
  number: string | null;
  group: string;
  logo: string | null;
  guide_id: string;
  hidden: boolean;
  source_name: string;
  source_number: string | null;
  source_guide_id: string;
  matched_guide_id: string | null;
  custom_name: string | null;
  custom_number: string | null;
  custom_guide_id: string | null;
  has_guide: boolean;
  /** It didn't play when last tried, or it is a provider's divider row. */
  offline: boolean;
  offline_since: Timestamp | null;
}

export interface LineupGroup extends LiveGroup {
  /** Its channels hidden. */
  hidden: number;
  /** Its channels that don't play (or are divider rows). */
  offline: number;
}

/** Which of the lineup's channels administrators list. */
export type LineupState = "all" | "shown" | "hidden" | "no-guide" | "offline";

/** The lineup's channels to list or change. */
export interface LineupFilter {
  source_id?: Id;
  group?: string;
  state?: LineupState;
  /** A name, or a number's beginning. */
  q?: string;
}

export interface LineupPage {
  total: number;
  /** Of `total`, how many are hidden. */
  hidden: number;
  offset: number;
  channels: LineupChannel[];
}

export interface LineupPatch {
  ids: Id[];
  /** Every channel this filter keeps too. */
  filter?: LineupFilter;
  /** Every channel of these groups too. */
  groups?: string[];
  hidden?: boolean;
  /** "" goes back to the source's. */
  name?: string;
  number?: string;
  guide_id?: string;
}

export interface LivePlan {
  video_codec: string | null;
  width: number | null;
  height: number | null;
  frame_rate: number | null;
  interlaced: boolean;
  audio_codec: string | null;
  audio_channels: number | null;
  /** "copy", or the encoder transcoding it. */
  video: string;
  audio: string;
}

export interface TunerStatus {
  channel_id: Id;
  channel_name: string;
  source_id: Id;
  source_name: string;
  started_at: Timestamp;
  connected: boolean;
  error: string | null;
  bytes_in: number;
  kbps: number;
  viewers: { session_id: string; user_id: Id; username: string; kind: "hls" | "stream"; started_at: Timestamp }[];
  idle: boolean;
  plan: LivePlan | null;
}

// ---------------------------------------------------------------- queries

export const liveKeys = {
  all: ["live"] as const,
  summary: ["live", "summary"] as const,
  channels: ["live", "channels"] as const,
  channel: (id: Id) => ["live", "channels", id] as const,
  groups: ["live", "groups"] as const,
  browse: ["live", "browse"] as const,
  page: (category: LiveCategory, search: string, page: number) =>
    ["live", "browse", categoryKey(category), search.trim(), page] as const,
  around: (category: LiveCategory, id: Id) => ["live", "browse", categoryKey(category), "", "around", id] as const,
  guide: (from: Timestamp, to: Timestamp, ids?: string) => ["live", "guide", from, to, ids ?? "all"] as const,
  // Administrators'.
  settings: ["live", "admin", "settings"] as const,
  sources: ["live", "admin", "sources"] as const,
  lineup: ["live", "admin", "lineup"] as const,
  lineupGroups: ["live", "admin", "lineup", "groups"] as const,
  lineupPage: (filter: LineupFilter, page: number) => ["live", "admin", "lineup", "page", JSON.stringify(filter), page] as const,
  tuners: ["live", "admin", "tuners"] as const,
};

export const liveQueries = {
  summary: () => queryOptions({ queryKey: liveKeys.summary, queryFn: () => api.get<LiveSummary>("/api/live") }),
  channel: (id: Id) =>
    queryOptions({
      queryKey: liveKeys.channel(id),
      queryFn: ({ signal }) => api.get<Channel>(`/api/live/channels/${id}`, { signal }),
    }),
  groups: () =>
    queryOptions({
      queryKey: liveKeys.groups,
      queryFn: ({ signal }) => api.get<LiveGroups>("/api/live/groups", { signal }),
      staleTime: 60_000,
    }),
  /** Page `page` (CHANNEL_PAGE channels) of a category's channels matching `search`. */
  page: (category: LiveCategory, search: string, page: number) =>
    queryOptions({
      queryKey: liveKeys.page(category, search, page),
      queryFn: ({ signal }) =>
        api.get<ChannelPage>(
          `/api/live/browse${qs({ ...browseParams(category, search), offset: page * CHANNEL_PAGE, limit: CHANNEL_PAGE })}`,
          { signal },
        ),
      staleTime: 30_000,
      // What's on now changes as time passes.
      refetchInterval: 60_000,
      refetchIntervalInBackground: false,
    }),
  /** The page of a category with channel `id` in it. */
  around: (category: LiveCategory, id: Id) =>
    queryOptions({
      queryKey: liveKeys.around(category, id),
      queryFn: ({ signal }) =>
        api.get<ChannelPage>(`/api/live/browse${qs({ ...browseParams(category, ""), around: id, limit: CHANNEL_PAGE })}`, {
          signal,
        }),
      staleTime: 30_000,
      refetchInterval: 60_000,
      refetchIntervalInBackground: false,
    }),
  /** The guide of channels `ids` (comma-separated; every channel without). */
  guide: (from: Timestamp, to: Timestamp, ids?: string) =>
    queryOptions({
      queryKey: liveKeys.guide(from, to, ids),
      queryFn: ({ signal }) => api.get<Guide>(`/api/live/guide${qs({ from, to, ids })}`, { signal }),
      staleTime: 5 * 60_000,
    }),
  settings: () => queryOptions({ queryKey: liveKeys.settings, queryFn: () => api.get<LiveSettings>("/api/live/settings") }),
  sources: () => queryOptions({ queryKey: liveKeys.sources, queryFn: () => api.get<LiveSource[]>("/api/live/sources") }),
  lineupGroups: () =>
    queryOptions({ queryKey: liveKeys.lineupGroups, queryFn: () => api.get<LineupGroup[]>("/api/live/lineup/groups") }),
  /** Page `page` (CHANNEL_PAGE channels) of the lineup `filter` keeps. */
  lineupPage: (filter: LineupFilter, page: number) =>
    queryOptions({
      queryKey: liveKeys.lineupPage(filter, page),
      queryFn: ({ signal }) =>
        api.get<LineupPage>(
          `/api/live/lineup/page${qs({ ...filter, q: filter.q?.trim() || undefined, offset: page * CHANNEL_PAGE, limit: CHANNEL_PAGE })}`,
          { signal },
        ),
    }),
  tuners: () => queryOptions({ queryKey: liveKeys.tuners, queryFn: () => api.get<TunerStatus[]>("/api/live/tuners") }),
};

export function useLiveSummary(enabled = true) {
  return useQuery({ ...liveQueries.summary(), enabled });
}

export function useGuide(from: Timestamp, to: Timestamp) {
  return useQuery(liveQueries.guide(from, to));
}

export function useLiveGroups(enabled = true) {
  return useQuery({ ...liveQueries.groups(), enabled });
}

export function useSetFavorite() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: ({ id, favorite }: { id: Id; favorite: boolean }) =>
      favorite ? api.put<void>(`/api/live/channels/${id}/favorite`) : api.del<void>(`/api/live/channels/${id}/favorite`),
    onMutate: async ({ id, favorite }) => {
      await qc.cancelQueries({ queryKey: liveKeys.channels });
      await qc.cancelQueries({ queryKey: liveKeys.browse });
      const mark = (c: Channel) => (c.id === id ? { ...c, favorite } : c);
      qc.setQueriesData<ChannelPage>({ queryKey: liveKeys.browse }, (page) => page && { ...page, channels: page.channels.map(mark) });
      qc.setQueryData<Channel>(liveKeys.channel(id), (c) => c && { ...c, favorite });
      qc.setQueryData<LiveGroups>(liveKeys.groups, (g) => g && { ...g, favorites: Math.max(0, g.favorites + (favorite ? 1 : -1)) });
    },
    onSettled: () =>
      Promise.all([
        qc.invalidateQueries({ queryKey: liveKeys.channels }),
        qc.invalidateQueries({ queryKey: liveKeys.browse }),
        qc.invalidateQueries({ queryKey: liveKeys.groups }),
      ]),
  });
}

// ---------------------------------------------------------------- admin hooks

export function useLiveSettings() {
  return useQuery(liveQueries.settings());
}

export function useSaveLiveSettings() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (settings: LiveSettings) => api.put<LiveSettings>("/api/live/settings", settings),
    onSuccess: (data) => {
      qc.setQueryData(liveKeys.settings, data);
      void qc.invalidateQueries({ queryKey: liveKeys.summary });
    },
  });
}

export function useLiveSources() {
  return useQuery({
    ...liveQueries.sources(),
    // A source being read says so until it is done.
    refetchInterval: (q) => (q.state.data?.some((s) => s.refreshing) ? 2000 : false),
  });
}

function useInvalidateLive() {
  const qc = useQueryClient();
  return () => qc.invalidateQueries({ queryKey: liveKeys.all });
}

export function useAddSource() {
  const invalidate = useInvalidateLive();
  return useMutation({
    mutationFn: (input: SourceInput) => api.post<LiveSource>("/api/live/sources", input),
    onSuccess: invalidate,
  });
}

export function useUpdateSource() {
  const invalidate = useInvalidateLive();
  return useMutation({
    mutationFn: ({ id, input }: { id: Id; input: SourceInput }) => api.put<LiveSource>(`/api/live/sources/${id}`, input),
    onSuccess: invalidate,
  });
}

export function useDeleteSource() {
  const invalidate = useInvalidateLive();
  return useMutation({ mutationFn: (id: Id) => api.del<void>(`/api/live/sources/${id}`), onSuccess: invalidate });
}

export function useRefreshSource() {
  const invalidate = useInvalidateLive();
  return useMutation({ mutationFn: (id: Id) => api.post<void>(`/api/live/sources/${id}/refresh`), onSuccess: invalidate });
}

export function useTestSource() {
  return useMutation({
    mutationFn: (body: SourceInput & { source_id?: Id }) => api.post<SourceTest>("/api/live/sources/test", body),
  });
}

export function useLineupGroups(enabled = true) {
  return useQuery({ ...liveQueries.lineupGroups(), enabled });
}

export function usePatchLineup() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (patch: LineupPatch) => api.patch<void>("/api/live/lineup", patch),
    onMutate: async (patch) => {
      if (patch.hidden === undefined) return;
      await qc.cancelQueries({ queryKey: liveKeys.lineup });
      const ids = new Set(patch.ids);
      const mark = (c: LineupChannel) => (ids.has(c.id) ? { ...c, hidden: patch.hidden! } : c);
      qc.setQueriesData<LineupPage>({ queryKey: [...liveKeys.lineup, "page"] }, (page) => page && { ...page, channels: page.channels.map(mark) });
    },
    onSettled: () => qc.invalidateQueries({ queryKey: liveKeys.all }),
  });
}

export function useTuners(enabled = true) {
  return useQuery({ ...liveQueries.tuners(), enabled, refetchInterval: 5000, refetchIntervalInBackground: false });
}

export function useStopTuner() {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: (channelId: Id) => api.del<void>(`/api/live/tuners/${channelId}`),
    onSuccess: () => qc.invalidateQueries({ queryKey: liveKeys.tuners }),
  });
}

/** A connection of a source whose connections are all in use (a `409`'s `connections`). */
export interface BusyConnection {
  channel_id: Id;
  channel_name: string;
  /** Who watches it: named to administrators only. */
  watchers: string[];
  people: number;
  /** Only you watch it (on another device). */
  yours: boolean;
  /** You may take it over: it's yours, or you're an administrator and nobody watching it is. */
  can_take_over: boolean;
}

/** The connections a `409` from watching says are in use. */
export function busyConnections(err: unknown): BusyConnection[] {
  const body = (err as { body?: { connections?: BusyConnection[] } } | null)?.body;
  return Array.isArray(body?.connections) ? body.connections : [];
}

/**
 * Starts watching a channel on this device; `takeOver`: the channel whose
 * connection to take; `controls`: the remote's commands its player obeys.
 */
export function watchChannel(id: Id, clientId: string, takeOver?: Id, controls?: Control[]): Promise<Watching> {
  return api.post<Watching>(`/api/live/channels/${id}/watch`, {
    client_id: clientId,
    take_over: takeOver,
    controls,
    ...deviceFields(),
  });
}

/**
 * What the channel a session watches is made into, once the transcoder has
 * looked at it (null when that takes too long).
 */
export function livePlan(sessionId: string): Promise<LivePlan | null> {
  return api.get<LivePlan | null>(`/api/live/sessions/${encodeURIComponent(sessionId)}/plan`);
}

/** A channel sent as it is (H.264, AAC, or either alone): a player that reads MPEG-TS plays it from `stream.ts`. */
export function playsAsSent(plan: LivePlan): boolean {
  const video = plan.video === "copy" || plan.video === "none";
  const audio = plan.audio === "copy" || plan.audio === "none";
  return video && audio && !(plan.video === "none" && plan.audio === "none");
}

/** The channel's MPEG-TS as the provider sends it, for this device. */
export function streamUrl(channelId: Id, clientId: string): string {
  return `/api/live/channels/${channelId}/stream.ts${qs({ client_id: clientId })}`;
}

/** Stops watching (keepalive: also as the page goes away). */
export function leaveChannel(sessionId: string, keepalive = false): Promise<void> {
  return api.del<void>(`/api/live/sessions/${encodeURIComponent(sessionId)}`, { keepalive }).catch(() => undefined);
}

// ---------------------------------------------------------------- helpers

/** The programme's place between its start and stop at `now` (0–1). */
export function programProgress(p: Pick<Program, "start" | "stop">, now: number): number {
  return Math.max(0, Math.min(1, (now - p.start) / Math.max(1, p.stop - p.start)));
}

/** "S2 E14", when the guide numbers the episode. */
export function episodeLabel(p: Pick<Program, "season" | "episode">): string | null {
  if (p.season != null && p.episode != null) return `S${p.season} E${p.episode}`;
  if (p.episode != null) return `E${p.episode}`;
  return null;
}

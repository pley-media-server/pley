/**
 * The parts of pley's API the TV apps use (docs/API.md). Their own copy
 * rather than the web app's: the TV reads fields the web leaves out (a
 * stream's level, Dolby Vision's level and HDR10+), and sends version 2
 * capabilities.
 */

export type Id = number;
/** Unix seconds. */
export type Timestamp = number;

export interface User {
  id: Id;
  username: string;
  name: string;
  display_name: string | null;
  is_admin?: boolean;
  /** A profile without a password, reached by switching. */
  managed?: boolean;
  /** It has a PIN, which "Who's watching?" asks at a start. */
  has_pin?: boolean;
  avatar_url: string | null;
  avatar_color: string | null;
  audio_language: string | null;
  subtitle_language: string | null;
  subtitle_mode: SubtitleMode;
  /** An app's token: what it was allowed ("library play"). */
  scope?: string;
}

export type SubtitleMode = "off" | "forced" | "foreign" | "always";

export type LibraryKind = "movies" | "shows" | "music";

export interface Library {
  id: Id;
  name: string;
  kind: LibraryKind;
  item_count: number;
  scanning: boolean;
}

export type ItemKind = "movie" | "show" | "season" | "episode" | "artist" | "album" | "track" | "extra";

export interface UserState {
  position_ms: number;
  watched: boolean;
  play_count: number;
  last_played_at?: Timestamp | null;
  rating?: number | null;
}

export type DynamicRange = "sdr" | "hdr10" | "hdr10plus" | "hlg" | "dolby_vision";

export interface MediaSummary {
  bitrate: number | null;
  size: number | null;
  width: number | null;
  height: number | null;
  video_codec: string | null;
  dynamic_range: DynamicRange | null;
}

/** Fields without a value are left out of the JSON: treat missing as null. */
export interface ItemSummary {
  id: Id;
  library_id: Id;
  kind: ItemKind;
  title: string;
  sort_title: string;
  year?: number | null;
  index?: number | null;
  parent_id?: Id | null;
  parent_title?: string | null;
  parent_index?: number | null;
  grandparent_id?: Id | null;
  grandparent_title?: string | null;
  duration_ms?: number | null;
  rating?: number | null;
  added_at: Timestamp;
  poster_url?: string | null;
  backdrop_url?: string | null;
  thumb_url?: string | null;
  user_state: UserState;
  leaf_count?: number | null;
  viewed_leaf_count?: number | null;
  child_count?: number | null;
  summary?: string | null;
  release_date?: string | null;
  extra_type?: string | null;
  media?: MediaSummary | null;
}

export interface ItemDetail extends ItemSummary {
  original_title: string | null;
  tagline: string | null;
  genres: string[];
  studio: string | null;
  content_rating: string | null;
  files: MediaFile[];
  markers?: Marker[];
  extras?: ItemSummary[];
  cast?: Credit[];
  crew?: Credit[];
  in_watchlist?: boolean;
}

export type CreditRole = "actor" | "creator" | "director" | "writer" | "producer";

export interface Credit {
  id: Id;
  name: string;
  role: CreditRole;
  character?: string;
  photo_url?: string;
}

export interface PersonSummary {
  id: Id;
  name: string;
  photo_url?: string;
}

export interface PersonDetail extends PersonSummary {
  credits: { item: ItemSummary; roles: { role: CreditRole; character?: string; episodes?: number }[] }[];
}

export type MarkerKind = "intro" | "credits";

export interface Marker {
  file_id: Id;
  kind: MarkerKind;
  start_ms: number;
  end_ms: number;
  source: "chapter" | "fingerprint" | "frames";
}

export interface Chapter {
  start_ms: number;
  end_ms: number;
  title: string | null;
  thumb_url?: string;
}

export interface MediaFile {
  id: Id;
  path: string;
  size: number;
  container: string | null;
  duration_ms: number | null;
  bitrate: number | null;
  width: number | null;
  height: number | null;
  video_codec: string | null;
  audio_codec: string | null;
  streams: MediaStream[];
  edition: string | null;
  selected_audio: number | null;
  selected_subtitle: number | null;
  /**
   * `selected_audio` is a track the profile picked. `false`: nothing is
   * picked, and each player gets its best track of that language at play
   * time, so a title's page names the language. Absent from older servers.
   */
  audio_picked?: boolean;
  chapters: Chapter[];
}

export type StreamKind = "video" | "audio" | "subtitle";

export interface MediaStream {
  index: number;
  kind: StreamKind;
  codec: string;
  language: string | null;
  title: string | null;
  default: boolean;
  forced: boolean;
  bitrate?: number;
  // video
  width?: number;
  height?: number;
  frame_rate?: number;
  bit_depth?: number;
  hdr?: boolean;
  dynamic_range?: DynamicRange;
  dv_profile?: number;
  dv_bl_compat?: number;
  dv_level?: number;
  /** Profile 7's enhancement layer: full or minimal (absent: not known). */
  dv_el?: "fel" | "mel";
  /** HDR10+ in the first frames (Dolby Vision and PQ streams; absent: not looked for). */
  hdr10plus?: boolean;
  /** Numbered as ffprobe numbers it (HEVC 5.1 = 153, H.264 5.1 = 51). */
  level?: number;
  profile?: string;
  pix_fmt?: string;
  decode_order_times?: boolean;
  // audio
  channels?: number;
  channel_layout?: string;
  sample_rate?: number;
  // subtitle
  external?: boolean;
  text_based?: boolean;
  /** Added on the server (an uploaded file, or one found on OpenSubtitles). */
  added?: boolean;
  deletable?: boolean;
}

export interface Hub {
  key: string;
  title: string;
  library_id: Id | null;
  items: ItemSummary[];
}

export interface Collection {
  id: Id;
  library_id: Id;
  title: string;
  summary: string | null;
  item_count: number;
  poster_url: string;
  backdrop_url: string;
}

export interface Playlist {
  id: Id;
  title: string;
  kind: "video" | "audio";
  item_count: number;
  duration_ms: number;
  artwork: string[];
  smart_filter: string | null;
}

export interface PlaylistEntry {
  entry_id: Id;
  item: ItemSummary;
}

export interface SubtitleTrack {
  index: number;
  codec: string;
  language: string | null;
  title: string | null;
  default: boolean;
  forced: boolean;
  external: boolean;
  text_based: boolean;
  url: string | null;
  /** Added on the server (a file uploaded, or found on OpenSubtitles), not in or beside the media file. */
  added?: boolean;
  /** This profile may delete it (an added track, and an administrator or whoever added it). */
  deletable?: boolean;
}

/** What the server knows of the network a client plays on (GET/PUT /api/playback/network). */
export interface NetworkAnswer {
  /** The server's own network: never a speed test. */
  local: boolean;
  /** The most Automatic starts at there, kbps; `null`: no limit known. */
  ceiling_kbps: number | null;
  source: "playback" | "probe" | null;
  /** When what set it happened, ms since 1970. */
  since: number | null;
  /** Run a speed test before starting Automatic. */
  probe: boolean;
}

/** What a client tells the server of its network: a speed test's result, or Automatic's ceiling (`null`: forgotten). */
export type NetworkReport = { probe_kbps: number; probe_complete: boolean } | { ceiling_kbps: number | null };

/** A subtitle file found for an item (GET /api/items/{id}/subtitles/search). */
export interface SubtitleResult {
  /** Opaque: passed back to add it. */
  id: string;
  provider: string;
  language: string;
  name: string;
  format: string;
  hearing_impaired: boolean;
  forced: boolean;
  downloads: number;
  /** "hash": made for this very file; "title": for the title. */
  matched: "hash" | "title";
}

export type PlaybackMethod = "direct" | "remux" | "transcode";

export interface PlaybackInfo {
  session_id: string;
  method: PlaybackMethod;
  url: string;
  hls: boolean;
  file_id: Id;
  duration_ms: number;
  audio_index: number | null;
  subtitle_index: number | null;
  burn_subtitles: boolean;
  audio_converted?: boolean;
  subtitles: SubtitleTrack[];
  reasons?: string[];
  reason: string | null;
  auto_quality?: string | null;
  /** What the screen gets of HDR video, by name: "Dolby Vision P8.1", "HDR10 (Dolby Vision base layer)", "SDR (tone-mapped)"… */
  video_range?: string | null;
  /** The stream comes in several sizes the TV's player moves between by itself: Automatic leaves the session be. */
  adaptive?: boolean;
  /**
   * The video is converted, and its bitrate changes as it is sent (`PUT
   * /api/playback/{id}/bitrate`): Automatic's steps need no new stream.
   */
  live_bitrate?: boolean;
}

export interface TranscodeInfo {
  state: "starting" | "running" | "idle" | "finished" | "failed";
  video: "copy" | "transcode" | "none";
  audio: "copy" | "transcode" | "none";
  video_in: string | null;
  video_out: string | null;
  audio_in: string | null;
  audio_out: string | null;
  encoder: string | null;
  decoder: string | null;
  pipeline: string | null;
  tonemap: string | null;
  dolby_vision: string | null;
  hdr10plus_stripped: boolean;
  subtitle_burn: boolean;
  speed: number | null;
  fps: number | null;
  transcoded_ms: number;
  segments_ready: number;
  segments_total: number;
  last_error: string | null;
}

export interface SessionStatus {
  session_id: string;
  method: PlaybackMethod;
  transcode: TranscodeInfo | null;
}

export interface Trickplay {
  id: string;
  interval_ms: number;
  width: number;
  height: number;
  columns: number;
  rows: number;
  count: number;
  sheets: number;
}

export type PlaybackMode = "auto" | "direct" | "remux" | "transcode";

/** Capabilities, version 1 fields and version 2 (docs/API.md, "Version 2"). */
export interface ClientCaps {
  containers: string[];
  video_codecs: string[];
  audio_codecs: string[];
  hdr: boolean;
  dolby_vision: boolean;
  caps_version: 2;
  video: VideoDecoderCaps[];
  hdr_types: Exclude<DynamicRange, "sdr">[];
  dolby_vision_profiles: { profile: number; max_level?: number }[];
  audio_decode: string[];
  audio_passthrough: string[];
  audio_channels: number;
  /** The highest sample rate the TV decodes audio at, in Hz; left out, not checked. */
  audio_max_sample_rate?: number;
  subtitle_codecs: string[];
  audio_track_switching: boolean;
  picture_subtitles_sideload: boolean;
  /** `variants`: moves between a multivariant playlist's sizes by itself (adaptive bitrate). */
  hls: { master_playlist: boolean; segments: string[]; variants?: boolean };
  display?: { width: number; height: number; refresh_rates: number[] };
  device: { form: "tv"; tier: "low" | "mid" | "high"; fire_tv: false; model: string };
}

export interface VideoDecoderCaps {
  codec: string;
  hardware: boolean;
  profiles?: string[];
  max_width?: number;
  max_height?: number;
  max_fps?: number;
  max_bit_depth?: number;
  max_level?: number;
}

export interface PlaybackRequest {
  item_id: Id;
  file_id?: Id;
  audio_index?: number | null;
  subtitle_index?: number | null;
  max_bitrate?: number | null;
  max_height?: number | null;
  start_ms?: number;
  mode?: PlaybackMode;
  client_id?: string;
  /** Started while the last session plays on, to move to it once it is ready: that one is left to the player to end. */
  keep_previous?: boolean;
  /** The TV's own player can't send the Authorization header: its URLs carry a key (docs/API.md, "Keyed URLs"). */
  keyed_urls?: boolean;
  caps: ClientCaps;
}

export type PlaybackState = "playing" | "paused" | "stopped";

/**
 * The server's events. Request events reach a sign-in with `requests`
 * (the managers' ones with `approvals` too); an older server sends
 * `requests_changed` and `request_available` with fewer fields, so those
 * past `type` may be missing.
 */
export type ServerEvent =
  | { type: "library_changed"; library_id: Id }
  | { type: "libraries_changed" }
  | { type: "sessions_changed" }
  | { type: "watchlist_changed" }
  /** Watch state changed here or on another device: the item, then its parent and grandparent; `children`: everything under the first too. */
  | { type: "user_state_changed"; item_ids: Id[]; children?: boolean }
  | { type: "session_stopped"; session_id: string; message: string | null; by_admin: boolean }
  | { type: "scan_started" | "scan_progress" | "scan_finished"; library_id: Id }
  /** Live TV's sources, channels or guide changed: ask again. */
  | { type: "live_channels_changed" }
  /** A live channel was tuned or let go, or its viewers changed. */
  | { type: "live_tuners_changed" }
  /** Someone took over the connection of live channel `channel_id`: every viewer of it on this account was ended, and `message` says why (docs/API.md, "Live TV"). */
  | { type: "live_taken_over"; session_id: string; channel_id: Id; message: string | null }
  /** Any change of a request (its stage included); all three null when many changed (after Your requests was seen elsewhere). */
  | { type: "requests_changed"; request_id?: Id | null; kind?: TitleKind | null; tmdb_id?: number | null }
  /** Ready to watch, or some seasons of a show are (`complete` false, `seasons` the ones newly ready); `item_id` as this profile can open it. */
  | {
      type: "request_available";
      request_id: Id;
      title: string;
      kind?: TitleKind;
      tmdb_id?: number;
      item_id: Id | null;
      poster_url?: string | null;
      seasons?: number[] | null;
      complete?: boolean;
    }
  | { type: "request_decided"; request_id: Id; title: string; kind: TitleKind; tmdb_id: number; status: "approved" | "declined"; reason: string | null }
  /** To managers: someone asked (`pending` waits for one of them). */
  | { type: "request_created"; request_id: Id; title: string; kind: TitleKind; tmdb_id: number; seasons: number[] | null; requested_by: RequestUser; status: "pending" | "approved" }
  /** To managers: a request couldn't be sent and the server won't try again by itself. */
  | { type: "request_failed"; request_id: Id; title: string; kind: TitleKind; reason: string | null; retry_at: Timestamp | null }
  /** Requests turned on or off, the limits or the TMDB key changed, or this profile's access did: the summary is asked again. */
  | { type: "requests_settings_changed" };

// ---- Live TV (docs/API.md, "Live TV")

/** `GET /api/live`: whether to show Live TV at all. */
export interface LiveSummary {
  /** Open to this profile, and it has channels: the rail shows Live TV. */
  available: boolean;
  /** Open to this profile (it may have no channels yet). */
  allowed: boolean;
  channels: number;
  /** Sources set up (administrators; 0 for others). */
  sources: number;
  /** The channels watched last, newest first. */
  recent: Id[];
}

export interface LiveProgram {
  channel_id: Id;
  start: Timestamp;
  stop: Timestamp;
  title: string;
  subtitle: string | null;
  description: string | null;
  categories: string[];
  /** The picture's address on this server, when the guide has one. */
  image: string | null;
  season: number | null;
  episode: number | null;
  is_new: boolean;
  is_live: boolean;
  rating: string | null;
}

export interface LiveChannel {
  id: Id;
  source_id: Id;
  name: string;
  number: string | null;
  group: string;
  /** The logo's address on this server, when the channel has one. */
  logo: string | null;
  favorite: boolean;
  /** The guide has programmes for it. */
  has_guide: boolean;
  now: LiveProgram | null;
  next: LiveProgram | null;
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

/** `GET /api/live/groups`: the groups in the lineup's order, and how many channels in all, marked and watched lately. */
export interface LiveGroups {
  total: number;
  favorites: number;
  recent: number;
  groups: LiveGroup[];
}

/** `GET /api/live/browse`: a page of a category's channels. */
export interface LiveChannelPage {
  /** Channels in the whole list. */
  total: number;
  offset: number;
  channels: LiveChannel[];
}

/** `GET /api/live/guide`: programmes by channel id, between `from` and `to`. */
export interface LiveGuide {
  from: Timestamp;
  to: Timestamp;
  programs: Record<string, LiveProgram[]>;
}

/** `POST /api/live/channels/{id}/watch`: the session, and the playlist's address (keyed when asked). */
export interface LiveWatching {
  session_id: string;
  url: string;
  kind: "hls";
  channel: LiveChannel;
}

/** A connection of a source whose connections are all in use (a `409` from watching, its `connections`). */
export interface LiveBusyConnection {
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

// ---- requests and Discover (docs/API.md, "Requests")

export type TitleKind = "movie" | "show";
export type RequestAccess = "off" | "ask" | "auto";
/** The decision. */
export type RequestStatus = "pending" | "approved" | "declined" | "failed" | "available";
/** Where it has got to, from the decision to the library. */
export type RequestStage = "pending" | "declined" | "failed" | "approved" | "waiting" | "searching" | "downloading" | "importing" | "partial" | "available" | "gone";
/** What this sign-in may do to a request now, in the order to list them. */
export type RequestAction = "withdraw" | "unwatch" | "approve" | "retry" | "decline" | "handled" | "delete" | "ask_again";
export type DiscoverList = "trending" | "movies" | "shows" | "upcoming" | "anime";
/** A title for this profile: the first of these that holds, in this order. */
export type TitleStatus = "library" | "restricted" | "requested" | "declined" | "partial" | "none";
/** A season for this profile, by the server's own rule for a whole season. */
export type SeasonState = "library" | "partial" | "requested" | "missing" | "unaired";

/** What requests are for this sign-in (`GET /api/requests/summary`, any app may ask). */
export interface RequestSummary {
  /** Requests are on, and TMDB has a key. */
  enabled: boolean;
  /** What asking from this sign-in does. */
  access: RequestAccess;
  /** What may still be asked for in the last 7 days; `null`: no limit. */
  movies_left: number | null;
  seasons_left: number | null;
  /** The limits per 7 days; `null`: none. */
  movies_limit: number | null;
  seasons_limit: number | null;
  /** None left: when one more may be asked for. */
  movies_next_at: Timestamp | null;
  seasons_next_at: Timestamp | null;
  /** This sign-in approves and declines (an administrator's, with `approvals`). */
  manage: boolean;
  /** For managers: requests waiting for a decision, and failed ones the server won't try again by itself. */
  pending: number;
  failed: number;
  /** This profile's requests (and those it follows) with news since Your requests was last seen. */
  unseen: number;
  /** This profile's requests and those it follows, any status. */
  own: number;
  /** The newest change among the requests this sign-in sees: without events, a change here means asking again. */
  changed_at: Timestamp | null;
  /** The sign-in predates what it needs: "requests" to ask at all, "approvals" to decide. */
  sign_in_again: "requests" | "approvals" | null;
}

export interface DiscoverTitle {
  kind: TitleKind;
  tmdb_id: number;
  title: string;
  year: number | null;
  overview: string;
  /** Server paths (`/api/discover/images/…`), sized with `w` as library artwork is. */
  poster_url: string | null;
  backdrop_url: string | null;
  /** TMDB users', 0–10; `null` with too few votes. */
  rating: number | null;
  status: TitleStatus;
  /** In a library this profile can open. */
  item_id: Id | null;
  /** An active request for it this profile made or follows (any, for managers), whatever `status` says. */
  request_id: Id | null;
}

export interface DiscoverPage {
  page: number;
  /** 0 to 500; a page may hold fewer than 20 (even none) before the last. */
  total_pages: number;
  results: DiscoverTitle[];
}

export interface CastMember {
  name: string;
  character: string | null;
  photo_url: string | null;
}

export interface DiscoverSeason {
  /** 0: specials. */
  number: number;
  name: string;
  episodes: number;
  aired: number;
  air_date: string | null;
  poster_url: string | null;
  in_library: number;
  requested: boolean;
  /** What the season is for this profile: drawn from this, never from the counts. */
  state: SeasonState;
  /** A request for it this profile made or followed was declined in the last 30 days. */
  declined: boolean;
  /** The active request asking for it, where this profile made or follows it (any, for managers). */
  request_id: Id | null;
}

export interface DiscoverDetail extends DiscoverTitle {
  original_title: string | null;
  tagline: string | null;
  genres: string[];
  release_date: string | null;
  runtime_minutes: number | null;
  certification: string | null;
  network: string | null;
  show_status: string | null;
  anime: boolean;
  cast: CastMember[];
  /** Shows, in TMDB's order; `null` for movies. */
  seasons: DiscoverSeason[] | null;
  imdb_id: string | null;
  /** This profile's (or one it followed), within its 30 days; `reason` is null for a follower. */
  declined: { reason: string | null; at: Timestamp; again_at: Timestamp } | null;
  /** Why it can't be asked for ("Rated R: above this account's age limit"). */
  restriction: string | null;
  /** This profile follows an active request for it. */
  watching: boolean;
  /** This profile's and those it follows (everyone's, for managers), newest first. */
  requests: MediaRequest[];
  movies_left: number | null;
  seasons_left: number | null;
  movies_next_at: Timestamp | null;
  seasons_next_at: Timestamp | null;
}

export interface RequestUser {
  id: Id;
  name: string;
  avatar_url: string | null;
  avatar_color: string | null;
}

export interface RequestDownload {
  /** 0–1, over everything queued for it. */
  progress: number;
  state: "queued" | "downloading" | "paused" | "importing" | "warning";
  eta: Timestamp | null;
  episodes: number;
}

export interface MediaRequest {
  id: Id;
  kind: TitleKind;
  tmdb_id: number;
  title: string;
  year: number | null;
  poster_url: string | null;
  backdrop_url: string | null;
  /** Shows: the seasons asked for; `null` for movies. */
  seasons: number[] | null;
  /** Shows: those ready. */
  available_seasons: number[];
  anime: boolean;
  status: RequestStatus;
  stage: RequestStage;
  /** Waiting: when it comes out (an episode's airing exactly; a release known by its day at 12:00 UTC). */
  stage_at: Timestamp | null;
  /** Declined: the administrator's (null for followers); failed: what went wrong, for managers. */
  reason: string | null;
  /** For managers: what to know about it. */
  note: string | null;
  /** This profile made it. */
  mine: boolean;
  /** This profile follows it. */
  watching: boolean;
  /** For managers: how many follow it. */
  watchers: number;
  /** It became available, partly available or declined since this profile last looked. */
  unseen: boolean;
  actions: RequestAction[];
  /** `null` for someone else's request seen by someone who doesn't manage. */
  requested_by: RequestUser | null;
  decided_by: RequestUser | null;
  decided_at: Timestamp | null;
  created_at: Timestamp;
  updated_at: Timestamp;
  available_at: Timestamp | null;
  /** Failed: when the server tries again by itself; `null`: it won't. */
  retry_at: Timestamp | null;
  sent_to: { service: "radarr" | "sonarr"; quality_profile: string | null; root_folder: string | null } | null;
  /** The library's title, where this profile can open it. */
  item_id: Id | null;
  download: RequestDownload | null;
}

export interface RequestList {
  total: number;
  /** Every filter but `status` applied. */
  counts: Record<RequestStatus, number>;
  requests: MediaRequest[];
  /** For managers: everyone with a request matching every filter but the person, by name. */
  people: RequestUser[];
}

/** A library title's place in requests (`GET /api/items/{id}/request`). */
export interface ItemRequest {
  kind: TitleKind;
  tmdb_id: number;
  status: TitleStatus;
  /** The newest active request for it this profile made or follows (any, for managers). */
  request: MediaRequest | null;
  /** The seasons the picker would tick with no limit (`[]` for movies); `null` while the server has no TMDB details for it. */
  askable: number[] | null;
}

export interface NewRequest {
  kind: TitleKind;
  tmdb_id: number;
  seasons?: number[];
}

export interface HistoryEntry {
  id: Id;
  viewed_at: Timestamp;
  device?: string;
  item: ItemSummary;
}

export interface HouseholdProfile {
  id: Id;
  name: string;
  avatar_url: string | null;
  avatar_color: string | null;
  current: boolean;
  needs: "none" | "pin" | "password" | "locked";
}

export type LibrarySort =
  | "title"
  | "year"
  | "added"
  | "rating"
  | "user_rating"
  | "release"
  | "duration"
  | "last_played"
  | "bitrate";

/** What a library's listing can be filtered by: `resolutions` and `dynamic_ranges` hold the server's tokens, best first, from a server that has them. */
export interface LibraryFilters {
  genres: string[];
  years: number[];
  resolutions?: string[];
  dynamic_ranges?: string[];
}

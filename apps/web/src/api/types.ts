// Mirrors docs/API.md. Keep in sync with the server.

export type Id = number;
/** Unix seconds. */
export type Timestamp = number;

export interface User {
  id: Id;
  username: string;
  /** Display name, or the username when there is none. */
  name: string;
  display_name: string | null;
  is_admin: boolean;
  /** A profile without a password, reached by switching from a home member. */
  managed: boolean;
  /** Listed in the household's profile switcher. */
  home: boolean;
  has_pin: boolean;
  avatar_url: string | null;
  /** "#38bdf8" */
  avatar_color: string | null;
  /** `false`: only the libraries in `library_ids`. */
  all_libraries: boolean;
  library_ids: Id[];
  /** Age limit for movies and shows; `null` = none. */
  max_age: number | null;
  /** The rating system the limit is shown in (`GB`, or `age`); `null`: the metadata region's. Only in administrators' lists. */
  max_age_system?: string | null;
  /** "2015-03": the limit follows this birthday (only in administrators' lists). */
  birth_month?: string | null;
  /** Labels whose titles it sees even above its age limit (only in administrators' lists). */
  show_labels?: string[];
  /** Labels whose titles it never sees (only in administrators' lists). */
  hide_labels?: string[];
  disabled: boolean;
  created_at: Timestamp;
  /** Default tracks for titles without a remembered choice. ISO 639 code; `null`: the file's default track. */
  audio_language: string | null;
  /** `null`: the audio language. */
  subtitle_language: string | null;
  subtitle_mode: SubtitleMode;
  /** The language Pley is in for this account (`en`, `fr`, `es`); `null`: automatic (the device's, else the server language). Absent from servers before it. */
  language?: string | null;
  /** `false`: a managed profile, or an account without a password. */
  has_password: boolean;
  /** Signs in with a code from an authenticator app after the password. */
  two_step: boolean;
  /** Requests: may not ask, asks an administrator, or approved as made ("auto" for administrators). Absent from servers before requests. */
  request_access?: RequestAccess;
}

/** May not ask for titles; asks an administrator; approved as made. */
export type RequestAccess = "off" | "ask" | "auto";

/**
 * When subtitles come on by themselves: never, only forced ones (foreign
 * dialogue), full ones when the audio is in another language, or always.
 */
export type SubtitleMode = "off" | "forced" | "foreign" | "always";

/** The account settings `PATCH /api/me` takes. */
export interface ProfilePatch {
  display_name?: string | null;
  avatar_color?: string | null;
  audio_language?: string | null;
  subtitle_language?: string | null;
  subtitle_mode?: SubtitleMode;
  /** `null`: automatic. */
  language?: string | null;
}

/** A signed-in session (browser, app). */
export interface Device {
  id: string;
  name: string;
  user_agent: string | null;
  ip: string | null;
  created_at: Timestamp;
  last_seen_at: Timestamp;
  /** The session making this request. */
  current: boolean;
}

export interface Invite {
  code: string;
  created_at: Timestamp;
  expires_at: Timestamp | null;
  uses_left: number;
  all_libraries: boolean;
  library_ids: Id[];
  max_age: number | null;
  max_age_system: string | null;
  show_labels?: string[];
  hide_labels?: string[];
  note: string | null;
}

/** `GET /api/labels`: a label on titles, and how many carry it. */
export interface LabelUse {
  label: string;
  /** Titles that carry it. */
  items: number;
  /** Profiles' and invitations' rules that name it. */
  rules: number;
}

/** `GET /api/ratings/systems`: what an age limit is picked and shown in. */
export interface RatingSystems {
  /** The metadata region (`GB`): its system is offered first. */
  region: string;
  systems: RatingSystem[];
}

export interface RatingSystem {
  /** ISO 3166-1 alpha-2. */
  country: string;
  /** In English; shown in the language shown through the country code instead. */
  name: string;
  /** Youngest first. */
  ratings: { name: string; age: number; kind: "film" | "tv" | null }[];
}

export interface InviteCheck {
  server_name: string;
  expires_at: Timestamp | null;
}

export interface ServerStatus {
  name: string;
  /** The server language, for devices in a language Pley doesn't have. Absent from servers before it. */
  language?: string | null;
  version: string;
  /** The server build's short commit hash; "unknown" without the repository. */
  commit?: string;
  /** Where the server's source code is offered. */
  source?: string;
  setup_required: boolean;
  /** Setting up takes the code in the server's log. */
  setup_code_required?: boolean;
  /**
   * Every account with a password signs in with two-step sign-in: one
   * without it sets it up as it signs in. Absent from servers before it.
   */
  two_step_required?: boolean;
}

/** `GET /api/me/two-step`: two-step sign-in for the signed-in user. */
export interface TwoStepStatus {
  enabled: boolean;
  /** When it was turned on. */
  since: Timestamp | null;
  recovery_codes_left: number;
}

/** `POST /api/me/two-step/setup`: a new secret for an authenticator app. */
export interface TwoStepSetup {
  /** Base32, for typing in. */
  secret: string;
  /** `otpauth://totp/…`: what the QR code says, and a link that opens an authenticator app on this device. */
  uri: string;
  /** The QR code: its width in modules, and an SVG path of its dark modules. */
  qr: { size: number; path: string };
  /** The secret must be confirmed with a code before then. */
  expires_at: Timestamp;
  /**
   * Sent back as `setup_id` with the first code when signing in sets it up:
   * only with it is this secret shown again, or turned on.
   */
  id: string;
}

/** Shown once: each signs in once instead of a code from the app. */
export interface RecoveryCodes {
  recovery_codes: string[];
}

export interface LoginResponse {
  user: User;
  /** Only for clients that aren't browsers: browsers keep the session in a cookie. */
  token?: string;
  /** Two-step sign-in was set up on the way in (the server requires it): its recovery codes, shown once. */
  recovery_codes?: string[];
}

export type LibraryKind = "movies" | "shows" | "music";

export interface Library {
  id: Id;
  name: string;
  kind: LibraryKind;
  paths: string[];
  item_count: number;
  created_at: Timestamp;
  scanned_at: Timestamp | null;
  scanning: boolean;
}

export type ItemKind = "movie" | "show" | "season" | "episode" | "artist" | "album" | "track" | "extra";

export interface UserState {
  position_ms: number;
  watched: boolean;
  play_count: number;
  last_played_at?: Timestamp | null;
  /** The user's own rating, 0-10 (two points per star). */
  rating?: number | null;
}

/** Fields without a value are left out of the JSON (smaller lists). */
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
  /** Bonus material: "trailer", "featurette", "behind_the_scenes", … */
  extra_type?: string | null;
  /** What the title's files are like: in listings sorted by bitrate. */
  media?: MediaSummary | null;
}

/** A title's best file (a show's files on average). */
export interface MediaSummary {
  /** Bits per second. */
  bitrate: number | null;
  /** Bytes (a show's: all its episodes'). */
  size: number | null;
  width: number | null;
  height: number | null;
  video_codec: string | null;
  dynamic_range: DynamicRange | null;
}

export interface ItemDetail extends ItemSummary {
  original_title: string | null;
  tagline: string | null;
  genres: string[];
  studio: string | null;
  content_rating: string | null;
  /** The country `content_rating` is from ("GB"), when known: read with its ratings, else the metadata region's. */
  content_rating_country?: string;
  tmdb_id: number | null;
  imdb_id: string | null;
  files: MediaFile[];
  /** Fields edited by hand; metadata refreshes keep them. "match" after Fix Match. */
  locked_fields?: string[];
  /** Fields the item's NFO file sets, named as `locked_fields` are, and "rating". Absent from servers before it. */
  nfo_fields?: string[];
  /** The item's labels: for administrators only, absent for others. */
  labels?: string[];
  /** Intro and credits of the files (absent on servers without detection). */
  markers?: Marker[];
  /** Trailers and other bonus material (movies and shows). */
  extras?: ItemSummary[];
  /** Actors in billing order; for episodes, the guest stars. */
  cast?: Credit[];
  /** Creators, directors, writers and producers. */
  crew?: Credit[];
  /** On the user's My List (movies and shows). */
  in_watchlist?: boolean;
}

export type CreditRole = "actor" | "creator" | "director" | "writer" | "producer";

/** A person credited in an item. */
export interface Credit {
  /** The person (`/person/{id}`). */
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

export interface PersonRole {
  role: CreditRole;
  character?: string;
  /** Credits from single episodes (guest parts): how many. */
  episodes?: number;
}

/** A movie or show a person is credited in (episode credits count towards the show). */
export interface PersonCredit {
  item: ItemSummary;
  roles: PersonRole[];
}

export interface PersonDetail extends PersonSummary {
  /** Newest first. */
  credits: PersonCredit[];
}

export interface HistoryUser {
  id: Id;
  name: string;
  avatar_url?: string | null;
  avatar_color?: string | null;
}

/** A play that reached the end (`GET /api/history`). */
export interface HistoryEntry {
  id: Id;
  viewed_at: Timestamp;
  /** "Chrome on Windows", a Jellyfin app's device. */
  device?: string;
  user: HistoryUser;
  item: ItemSummary;
}

export interface HistoryPage {
  total: number;
  entries: HistoryEntry[];
}

/** Space used by what pley stores (`GET /api/system/storage`). */
export interface StorageUsage {
  blobs: number;
  blob_bytes: number;
  media_folders: number;
  media_bytes: number;
  cache_bytes: number;
}

export interface CleanReport {
  blobs_removed: number;
  media_folders_removed: number;
  bytes_freed: number;
}

export interface DatabaseHealth {
  ok: boolean | null;
  problem?: string;
  checked_at: Timestamp | null;
}

export interface Backup {
  name: string;
  path: string;
  bytes: number;
  created_at: Timestamp;
}

export interface Backups {
  health: DatabaseHealth;
  backups: Backup[];
}

export interface SearchResults {
  items: ItemSummary[];
  people?: PersonSummary[];
  /** Live TV channels by name, number or what's on, for those it's open to. */
  channels?: import("./livetv").Channel[];
}

export type MarkerKind = "intro" | "credits";

export interface Marker {
  file_id: Id;
  kind: MarkerKind;
  start_ms: number;
  end_ms: number;
  source: "chapter" | "fingerprint" | "frames";
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
  /** The edition the file is tagged with (`{edition-Final Cut}`). */
  edition: string | null;
  /** Tracks to start with: the user's remembered choice for this show or movie. */
  selected_audio: number | null;
  selected_subtitle: number | null;
  /**
   * `selected_audio` is a track the user picked. `false`: nothing is picked, and each
   * player gets its best track of that language at play time, so a title's page names
   * the language, not a track. Absent from older servers.
   */
  audio_picked?: boolean;
  chapters: Chapter[];
}

export interface Chapter {
  start_ms: number;
  end_ms: number;
  title: string | null;
  /** Picture of the chapter, once the file has seek previews. */
  thumb_url?: string;
}

/** Seek-bar preview sprite sheets of a file (`GET /api/files/{id}/trickplay`). */
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

export type StreamKind = "video" | "audio" | "subtitle";

export type DynamicRange = "sdr" | "hdr10" | "hdr10plus" | "hlg" | "dolby_vision";

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
  /** Dolby Vision profile (5, 7, 8, …). */
  dv_profile?: number;
  /** DV base-layer compatibility id (1 = HDR10, 2 = SDR, 4 = HLG, 6 = Blu-ray HDR10 (profile 7), 0 = none). */
  dv_bl_compat?: number;
  /** Dolby Vision level. */
  dv_level?: number;
  /** Profile 7's enhancement layer: full or minimal (absent: not known). */
  dv_el?: "fel" | "mel";
  /** HDR10+ in the first frames (Dolby Vision and PQ streams; absent: not looked for). */
  hdr10plus?: boolean;
  profile?: string;
  // audio
  channels?: number;
  channel_layout?: string;
  sample_rate?: number;
  // subtitle
  external?: boolean;
  text_based?: boolean;
  /** Added on the server (a file uploaded, or one found on OpenSubtitles), not in or beside the media file. */
  added?: boolean;
  /** This profile may delete it (an added track, and an administrator or whoever added it). */
  deletable?: boolean;
}

export interface Hub {
  key: string;
  title: string;
  library_id: Id | null;
  items: ItemSummary[];
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
  /** Added on the server (a file uploaded, or one found on OpenSubtitles). */
  added?: boolean;
  /** This profile may delete it (an added track, and an administrator or whoever added it). */
  deletable?: boolean;
}

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
  /** The audio is converted, also when the video is copied (a direct stream). */
  audio_converted?: boolean;
  subtitles: SubtitleTrack[];
  /** Why the file isn't played as it is, one short sentence each. */
  reasons?: string[];
  /** The same, in one line. */
  reason: string | null;
  /**
   * The quality a conversion of the original aims for (the closest one the
   * player offers, by id: "30m"…): absent when a quality was picked or the
   * video isn't converted.
   */
  auto_quality?: string | null;
  /**
   * What the screen gets of HDR video, by name: "Dolby Vision P8.1", "HDR10
   * (Dolby Vision base layer)", "SDR (tone-mapped)"… Absent for SDR video
   * and from older servers.
   */
  video_range?: string | null;
  /**
   * The stream comes in several sizes the player moves between by itself
   * (`master.m3u8`): Automatic leaves the session be. Absent from older servers.
   */
  adaptive?: boolean;
  /**
   * How much later than the file's times (as picture subtitles' `.mks` is
   * timed) an HLS stream's timestamps are, in ms; null for a direct play.
   * Absent from older servers.
   */
  pts_offset_ms?: number | null;
}

/** `GET /api/playback/{session}`: how a session plays, with its transcode's live numbers. */
export interface SessionStatus {
  session_id: string;
  method: PlaybackMethod;
  transcode: TranscodeInfo | null;
}

export interface ClientCaps {
  containers: string[];
  video_codecs: string[];
  audio_codecs: string[];
  /** Shows HDR10 / HLG without tone mapping. */
  hdr?: boolean;
  /** Decodes Dolby Vision (needed for profile 5). */
  dolby_vision?: boolean;
  /** HLS: `variants` moves between a multivariant playlist's sizes by itself (adaptive bitrate). */
  hls?: { variants?: boolean };
  /** Subtitles the player shows itself, pictures included (ffprobe's names: "hdmv_pgs_subtitle"). */
  subtitle_codecs?: string[];
  /** Shows picture subtitles from their `.mks` next to any stream: asks for no `subtitle_index` for them. */
  picture_subtitles_sideload?: boolean;
}

/** `auto` lets the server pick the cheapest method the client supports. */
export type PlaybackMode = "auto" | "direct" | "remux" | "transcode";

export interface PlaybackRequest {
  item_id: Id;
  file_id?: Id;
  audio_index?: number | null;
  subtitle_index?: number | null;
  max_bitrate?: number | null;
  max_height?: number | null;
  start_ms?: number;
  mode?: PlaybackMode;
  /** Identifies the player; its previous sessions end when it starts a new one. */
  client_id?: string;
  /** Started while the last session plays on, to move to it once it is ready: that one is left to the player to end. */
  keep_previous?: boolean;
  /** The remote's commands this player obeys; left out: none (Stop always works). */
  controls?: Control[];
  /** What remotes call this device; left out: the server names it from the User-Agent ("Firefox on macOS"). */
  device_name?: string;
  /** What kind of device it is; left out: the server tells from the app (a browser here). */
  device_kind?: DeviceKind;
  caps: ClientCaps;
}

export type PlaybackState = "playing" | "paused" | "stopped";

export interface ProgressRequest {
  position_ms: number;
  duration_ms?: number;
  state: PlaybackState;
  session_id?: string;
  /** The audio track it plays, by stream index, for the user's remotes; left out: as known before. */
  audio_index?: number;
  /** The subtitles it shows, by stream index (-1: none). */
  subtitle_index?: number;
  /** Its Quality menu's pick, by id ("auto", "original", "8m"…). */
  quality?: string;
}

export type ScanPhase = "queued" | "walking" | "probing" | "metadata" | "done";

export interface ScanStatus {
  library_id: Id;
  library_name: string;
  phase: ScanPhase;
  processed: number;
  total: number;
  started_at: Timestamp;
}

export type TranscodeJobState = "starting" | "running" | "idle" | "finished" | "failed";

export type TranscodePipeline = "gpu" | "hybrid" | "cpu" | "copy";

export interface TranscodeInfo {
  job_id: string;
  state: TranscodeJobState;
  video: "copy" | "transcode" | "none";
  audio: "copy" | "transcode" | "none";
  video_in: string | null;
  video_out: string | null;
  audio_in: string | null;
  audio_out: string | null;
  encoder: string | null;
  /** The software encoder's preset now. */
  preset?: string | null;
  decoder: string | null;
  /** Where frames are processed: zero-copy on one GPU, mixed, CPU only, or stream copy. */
  pipeline?: TranscodePipeline | null;
  tonemap: string | null;
  subtitle_burn: boolean;
  speed: number | null;
  fps: number | null;
  transcoded_ms: number;
  segments_ready: number;
  segments_total: number;
  /** What the transcode aims for, video and audio together (bits per second). */
  bitrate_out?: number | null;
  last_error: string | null;
  /** Of a transcode in several sizes: the one the player takes now (0 the largest; the rest is its)… */
  rendition?: number;
  /** …and how many smaller ones there are (0: one size). */
  variants?: number;
}

export interface SessionInfo {
  session_id: string;
  kind: "item";
  user_id: Id;
  username: string;
  /** The device it plays on ("Living room TV", "Firefox on macOS"); null when it isn't known. */
  device: string | null;
  item: ItemSummary;
  method: PlaybackMethod;
  position_ms: number;
  state: PlaybackState;
  started_at: Timestamp;
  updated_at: Timestamp;
  transcode: TranscodeInfo | null;
}

/** A command a remote sends one of the user's players (`POST /api/sessions/{id}/command`). */
export type Control = "play" | "pause" | "seek" | "skip" | "next" | "previous" | "audio" | "subtitles" | "quality";

/** What kind of device plays a session, or takes casts: remotes show each with its own icon. */
export type DeviceKind = "tv" | "phone" | "tablet" | "computer" | "browser" | "other";

/** One of a title's audio or subtitle tracks, which a remote picks from (`PlayingSession.tracks`). */
export interface TrackOption {
  /** The stream index commands and progress reports name. */
  index: number;
  language: string | null;
  title: string | null;
  codec: string | null;
  /** Audio only. */
  channels: number | null;
  default: boolean;
  forced: boolean;
}

/** One of the user's own sessions whose player is there (`GET /api/sessions`): what remotes control. */
export interface PlayingSession {
  session_id: string;
  kind: "item" | "live";
  device: string | null;
  /** Which device plays it: everything one sign-in plays has the same, which receivers have too. */
  device_id: string;
  device_kind: DeviceKind;
  /** What plays (kind "item"). */
  item: ItemSummary | null;
  /**
   * The channel watched (kind "live"): `now`, the programme on, from
   * `now_start` to `now_stop` (unix seconds), and `next`, the one after it.
   * The times and `next` are absent from servers before them.
   */
  channel: {
    id: Id;
    name: string;
    number: string | null;
    logo: string | null;
    now: string | null;
    now_start?: Timestamp | null;
    now_stop?: Timestamp | null;
    next?: string | null;
  } | null;
  /** Live: always "playing". */
  state: "playing" | "paused" | "buffering";
  /** Live: 0. */
  position_ms: number;
  /** Null when the length isn't known, and on live. */
  duration_ms: number | null;
  /** Its player's last report, by the server's clock. */
  updated_at: Timestamp;
  /** How long ago that report was, by the server's clock: a remote moves the place on from it while it plays. */
  age_ms: number;
  /** What its player obeys; Stop always works. */
  controls: Control[];
  /** The audio track it plays, by stream index; null when not known, and on live. */
  audio_index: number | null;
  /** The subtitles it shows, by stream index (-1: none); null when not known, and on live. */
  subtitle_index: number | null;
  /** Its file's tracks, which `audio` and `subtitles` pick from; empty on live. */
  tracks: { audio: TrackOption[]; subtitles: TrackOption[] };
  /** Its Quality menu's pick, by id; null until its player says (absent from servers before it). */
  quality?: string | null;
  /** What its Quality menu offers for the file, which `quality` picks from; empty for music and on live. */
  qualities?: { id: string; label: string }[];
  /** Its file's intro and credits, which a remote offers to skip; empty on live (absent from servers before them). */
  markers?: Marker[];
}

/** What a device plays when cast to: titles, music and playlists (`item`), Live TV channels (`live`). */
export type CastKind = "item" | "live";

/**
 * A device that takes casts (`GET /api/remote/devices`): one of the user's,
 * or one of their household's other profiles is on.
 */
export interface RemoteDevice {
  /** The app's own id for the device: where casts go. */
  receiver_id: string;
  /** As its sessions say it: what it plays has the same. */
  device_id: string;
  name: string | null;
  kind: DeviceKind;
  casts: CastKind[];
  /**
   * The household profile it is on, when it isn't the user's own: a cast
   * switches it to the user's first. What it plays isn't listed.
   */
  profile?: { id: Id; name: string };
}

/**
 * What a remote sends a device to play (`POST /api/remote/devices/{id}/cast`):
 * exactly one of a title, a playlist and a channel.
 */
export interface CastRequest {
  item_id?: Id;
  playlist_id?: Id;
  channel_id?: Id;
  /** Where a title starts (ms); left out, where the device's own Play would. */
  start_ms?: number;
  /** A playlist, an album or an artist, shuffled. */
  shuffle?: boolean;
  /** A movie or an episode: the audio track and subtitles (-1: off) picked on the page, by stream index. */
  audio_index?: number;
  subtitle_index?: number;
  /** A device on another profile: that profile (its `profile.id`), which the device switches from. */
  profile_id?: Id;
  /** With `profile_id`: the device listed (its `device_id`), which the cast goes to or doesn't go at all. */
  device_id?: string;
  /** The user's PIN, when their profile has one and the device is on another profile. */
  pin?: string;
}

export interface Activity {
  scans: ScanStatus[];
  sessions: SessionInfo[];
  /** Intro/credits detection in progress (admins only). */
  analysis?: AnalysisStatus | null;
  /** Seasons waiting for intro/credits detection (admins only). Absent from servers before it. */
  seasons_pending?: number;
  /** Files waiting for seek previews, those being made included (admins only). */
  previews_pending?: number;
  /** Files whose seek previews are being made now (admins only). Absent from servers before it. */
  previews_running?: number;
  /** When what waits gets done, while some does (admins only). Absent from servers before it. */
  library_work?: LibraryWorkStatus | null;
}

/** Why library work waits: something plays (or did a minute ago), other programs keep the processors busy, or these hours aren't its own. */
export type LibraryWorkWait = "playing" | "busy" | "hours";

export interface LibraryWorkStatus {
  mode: LibraryWork;
  /** Why the rest of the libraries waits now; null: it doesn't. What's likely to be watched soon goes on but for `playing` and `busy`. */
  waiting: LibraryWorkWait | null;
  /** When the hours it waits for begin. */
  resumes_at: Timestamp | null;
  /** The hours people watch are known (about a week was seen). */
  learned: boolean;
}

/** `GET /api/system/library-work`: the week as learned. */
export interface WatchingHours extends LibraryWorkStatus {
  /** The hour of the week now: 0 is Sunday from midnight, in the server's time. */
  now: number;
  /** The 168 hours of the week from Sunday midnight. */
  hours: WatchingHour[];
}

export interface WatchingHour {
  /** The share of its minutes something played in, 0–1; null where it wasn't seen enough to tell. */
  playing: number | null;
  /** Work on the rest of the libraries may begin then. */
  open: boolean;
}

export interface AnalysisStatus {
  item_id: Id;
  title: string;
  done: number;
  total: number;
}

export type ServerEvent =
  | { type: "scan_started"; library_id: Id }
  | { type: "scan_progress"; library_id: Id; phase: ScanPhase; processed: number; total: number }
  | { type: "scan_finished"; library_id: Id; added: number; updated: number; removed: number }
  | { type: "library_changed"; library_id: Id }
  | { type: "libraries_changed" }
  | { type: "sessions_changed" }
  /** My List changed on another of the user's devices. */
  | { type: "watchlist_changed" }
  /**
   * The user's watch state of an item changed, here or on another device:
   * `item_ids` is the item, then its season and show (album and artist);
   * `children`: everything under the first one changed too.
   */
  | { type: "user_state_changed"; item_ids: Id[]; children: boolean }
  /** A playback session was ended on the server (`DELETE /api/sessions/{id}`): the device playing it stops. */
  | { type: "session_stopped"; session_id: string; message: string | null; by_admin: boolean }
  /**
   * The user's remote sent their player of `session_id` a command: `seek`
   * has `position_ms`, `skip` has `offset_ms` (back when negative), `audio`
   * and `subtitles` the track's `stream_index` (subtitles -1: off),
   * `quality` the Quality menu's entry by id (absent from servers before it).
   */
  | {
      type: "player_command";
      session_id: string;
      command: Control;
      position_ms: number | null;
      offset_ms: number | null;
      stream_index: number | null;
      quality?: string | null;
    }
  /**
   * The user's remote sent their device of `receiver_id` something to play
   * (`from`: the remote's device name). The desktop app takes casts
   * (components/CastReceiver.tsx); a browser tab is a remote only. Sent
   * from another profile of the household, it names that profile
   * (`profile_id`) and the ticket this device switches to it with first
   * (`switch_ticket`).
   */
  | {
      type: "cast";
      receiver_id: string;
      item_id: Id | null;
      playlist_id: Id | null;
      channel_id: Id | null;
      start_ms: number | null;
      shuffle: boolean;
      audio_index?: number | null;
      subtitle_index?: number | null;
      from: string | null;
      profile_id?: Id;
      switch_ticket?: string;
    }
  /** Live TV's sources, channels or guide changed. */
  | { type: "live_channels_changed" }
  /** A live channel started or stopped being watched: the connections in use changed. */
  | { type: "live_tuners_changed" }
  /**
   * Someone took the connection of a channel this account watched, to watch
   * another: every player of `channel_id` stops and shows `message`.
   */
  | { type: "live_taken_over"; session_id: string; channel_id: Id; message: string }
  /**
   * A request changed (made, decided, followed, its stage, withdrawn,
   * deleted), to its maker, its followers and managers; all three `null`
   * when many did (or this account's other streams marked them seen).
   */
  | { type: "requests_changed"; request_id: Id | null; kind: TitleKindName | null; tmdb_id: number | null }
  /**
   * Something this account asked for or follows is ready (`item_id`: where
   * it may open it), or some of a show's seasons are (`complete: false`,
   * `seasons`: those newly ready).
   */
  | {
      type: "request_available";
      request_id: Id;
      title: string;
      kind: TitleKindName;
      tmdb_id: number;
      item_id: Id | null;
      poster_url: string | null;
      seasons: number[] | null;
      complete: boolean;
    }
  /** A request this account made was approved or declined (declined: its followers too, without the reason). */
  | { type: "request_decided"; request_id: Id; title: string; kind: TitleKindName; tmdb_id: number; status: "approved" | "declined"; reason: string | null }
  /** Managers: someone asked for something (they're told of `pending` ones). */
  | {
      type: "request_created";
      request_id: Id;
      title: string;
      kind: TitleKindName;
      tmdb_id: number;
      seasons: number[] | null;
      requested_by: { id: Id; name: string; avatar_url: string | null; avatar_color: string | null };
      status: "pending" | "approved";
    }
  /** Managers: a request couldn't be sent to Radarr or Sonarr, and waits for one of them. */
  | { type: "request_failed"; request_id: Id; title: string; kind: TitleKindName; reason: string | null; retry_at: Timestamp | null }
  /** Requests were turned on or off, their limits or the TMDB key changed, or this account's access did. */
  | { type: "requests_settings_changed" };

/** A request's kind of title. */
type TitleKindName = "movie" | "show";

/** `vulkan` (h264_vulkan) isn't offered: it can only be set through the API. */
export type HwAccel = "auto" | "none" | "nvenc" | "qsv" | "amf" | "vaapi" | "videotoolbox" | "vulkan";
/** `auto`: the transcoder picks one from its load and how far ahead of the player it is, and changes it as they change. */
export type TranscodePreset = "auto" | "ultrafast" | "superfast" | "veryfast" | "faster" | "fast" | "medium";
export type ToneMapping = "auto" | "libplacebo" | "opencl" | "zscale" | "none";
/** Where TV show metadata comes from; "auto" is TMDB, else TheTVDB (when they have a key), else TVmaze. */
export type TvMetadata = "auto" | "tmdb" | "tvdb" | "tvmaze";
/**
 * When library work runs: `auto`, what's likely to be watched soon at any
 * hour and the rest in the hours people rarely watch, as learned; `hours`,
 * all of it between set hours; `idle`, whenever nothing plays.
 */
export type LibraryWork = "auto" | "hours" | "idle";

export interface Settings {
  server_name: string;
  /** The server language; `""`: not chosen, which reads as English. */
  language: string;
  tmdb_api_key: string;
  metadata_language: string;
  tv_metadata: TvMetadata;
  tvdb_api_key: string;
  /** Subscriber PIN, for TheTVDB keys that need one. */
  tvdb_pin: string;
  /** Artists and albums from MusicBrainz, covers from the Cover Art Archive. */
  music_metadata: boolean;
  /**
   * Ids and details from the NFO files next to movies, shows, seasons and
   * episodes, over the online source's. Absent from servers before it.
   */
  read_nfo?: boolean;
  hwaccel: HwAccel;
  hardware_decoding: boolean;
  tonemapping: ToneMapping;
  max_transcodes: number;
  transcode_preset: TranscodePreset;
  throttle_transcodes: boolean;
  /** Keep transcodes' segments in RAM where the transcoder can, else on disk. Throttled transcodes only. */
  transcode_in_memory: boolean;
  scan_interval_minutes: number;
  watch_libraries: boolean;
  detect_markers: boolean;
  seek_previews: boolean;
  previews_in_background: boolean;
  /**
   * When intro and credits detection and previews of whole libraries run,
   * besides waiting while anything plays (see `LibraryWork`). Absent from
   * servers before it, as are the three after it.
   */
  library_work?: LibraryWork;
  /** The hours of the day (0–23, server time) of `hours`: past midnight when `to` comes first, all day when they're the same. */
  library_work_from?: number;
  library_work_to?: number;
  /** Library work waits while other programs keep more than this share of the processors busy (%, 10–100; 100: never). */
  library_work_max_cpu?: number;
  /** Apps by other publishers may sign in (pley's own apps always can); turning it off signs them out. */
  third_party_apps: boolean;
  /**
   * Every account with a password signs in with two-step sign-in. Turning it
   * on (only an administrator who uses it can) signs out everyone without it,
   * who sets it up at their next sign-in. Absent from servers before it.
   */
  require_two_step?: boolean;
  /**
   * Addresses with a port pley is reached at through a reverse proxy that
   * leaves the port out of `Host` (`https://media.example.com:9999`).
   * Absent from servers before it.
   */
  proxy_addresses?: string[];
  /** OpenSubtitles.com API key: subtitle search is off without it. Absent from servers before it. */
  opensubtitles_api_key?: string;
  /** An opensubtitles.com account, for its daily download quota. */
  opensubtitles_username?: string;
  /** Sent only to change it (never sent back): "" removes it. */
  opensubtitles_password?: string;
  /** A password is saved. */
  opensubtitles_password_set?: boolean;
  /** People may ask for movies and shows (needs a TMDB key). Absent from servers before requests, as are those after it. */
  requests?: boolean;
  /** Requests an account may make in 7 days, 0–1000 (administrators: no limit); 0 = no limit. */
  request_limit_movies?: number;
  /** As many seasons (a show counts its seasons). */
  request_limit_seasons?: number;
  /** "http://localhost:7878"; "" = no Radarr: movies approved are added by hand. */
  radarr_url?: string;
  /** Sent only to change it (never sent back): left out, the saved one stays; "" = none. A new `radarr_url` needs it too. */
  radarr_api_key?: string;
  radarr_api_key_set?: boolean;
  /** Radarr's id; 0 = not chosen (approving fails until one is). */
  radarr_quality_profile?: number;
  /** "" = not chosen (approving fails until one is). */
  radarr_root_folder?: string;
  /** For anime; 0 = `radarr_quality_profile`. */
  radarr_anime_quality_profile?: number;
  /** For anime; "" = `radarr_root_folder`. */
  radarr_anime_root_folder?: string;
  /** When Radarr may take a release. */
  radarr_minimum_availability?: MinimumAvailability;
  /** Search as soon as a movie is added. */
  radarr_search?: boolean;
  /** "http://localhost:8989"; "" = no Sonarr. */
  sonarr_url?: string;
  sonarr_api_key?: string;
  sonarr_api_key_set?: boolean;
  sonarr_quality_profile?: number;
  sonarr_root_folder?: string;
  /** Sonarr 3's language profile; 0 = its first (Sonarr 4 has none). */
  sonarr_language_profile?: number;
  sonarr_anime_quality_profile?: number;
  sonarr_anime_root_folder?: string;
  /** A folder for each season. */
  sonarr_season_folders?: boolean;
  /** Search for the seasons asked for as soon as they're added. */
  sonarr_search?: boolean;
}

export type MinimumAvailability = "announced" | "in_cinemas" | "released";

/** A service the settings can test (`POST /api/settings/test`). */
export type TestedService = "tmdb" | "tvdb" | "tvmaze" | "musicbrainz" | "transcoder" | "opensubtitles";

export interface ServiceTest {
  service: TestedService;
  /** A key typed in the form (not saved yet); the saved one when left out. */
  api_key?: string;
  pin?: string;
  /** An OpenSubtitles account typed in; the saved password when left out. */
  username?: string;
  password?: string;
}

export interface ServiceTestResult {
  ok: boolean;
  message: string;
  /** How long the answer took. */
  ms: number;
}

export interface TranscoderInfo {
  available: boolean;
  managed: boolean;
  url: string | null;
  version: string | null;
  commit?: string | null;
  license: string | null;
  ffmpeg_version: string | null;
  encoders: string[];
  active_encoder: string | null;
  hwaccels: string[];
  /** Hardware accelerations with a working H.264 encoder, best first: the first is the one Automatic uses. */
  accels: HwAccel[];
  /** Some video is decoded on a GPU here: the hardware decoding setting changes something. */
  hardware_decoding: boolean;
  /** What each hardware acceleration choice does here (empty while unavailable). Absent from servers before it. */
  with_accel?: Partial<Record<HwAccel, AccelUse>>;
  tonemapping: string[];
  dolby_vision: boolean;
  /** Audio fingerprints, which find intros and credits that chapters don't name. */
  chromaprint: boolean;
  /** FFmpeg can hold transcodes back (6.1 and newer). */
  throttling: boolean;
  /** Whether segments can be kept in memory, and how much; `null` while the transcoder is unavailable. */
  memory?: TranscoderMemory | null;
  max_jobs: number;
  active_jobs: number;
  error: string | null;
}

/** What choosing a hardware acceleration does on this server, as its transcoder found. */
export interface AccelUse {
  /** Some video is decoded on a GPU with it: the hardware decoding setting changes something. */
  hardware_decoding: boolean;
  /** What it leaves to software encoders (`hdr`: HEVC in 10 bits, for transcodes that keep HDR), where one works. */
  software: ("h264" | "hevc" | "hdr")[];
  /** The tone mappers that work with it, where the transcoder tried them (VAAPI); `null`: every one it has. */
  tonemappers?: string[] | null;
}

export interface TranscoderMemory {
  available: boolean;
  /** Why not ("works on Linux only"). */
  reason?: string | null;
  /** The folder in RAM (`/dev/shm`). */
  path?: string | null;
  /** The most segments take there. */
  budget_bytes: number;
}

export interface TranscoderCounters {
  jobs_created: number;
  jobs_failed: number;
  processes_started: number;
  process_failures: number;
  segments_produced: number;
  segments_served: number;
  bytes_served: number;
  probes: number;
  probe_failures: number;
  probe_ms_total: number;
  thumbnails: number;
  subtitle_extractions: number;
  keyframe_scans: number;
}

export interface TranscoderJob {
  id: string;
  label: string | null;
  owner: string | null;
  state: TranscodeJobState;
  created_at_ms: number;
  last_access_ms: number;
  info: TranscodeInfo;
  restarts: number;
  served_bytes: number;
  warnings: string[];
}

export interface TranscoderStatus {
  available: boolean;
  version: string | null;
  uptime_secs: number;
  max_jobs: number;
  running_processes: number;
  hw_processes: number;
  max_hw_jobs: number;
  work_dir_bytes: number;
  counters: TranscoderCounters;
  jobs: TranscoderJob[];
}

/** Where pley listens: addresses such as `127.0.0.1:7539` (Settings → Network). */
export interface ListenChoice {
  /** The web app and its API. */
  web: string[];
  /** The Jellyfin-compatible API on addresses of its own (port 8096): none when off or on pley's port. */
  jellyfin: string[];
  /** The Jellyfin-compatible API answers on pley's own port, to its apps. */
  jellyfin_on_web: boolean;
  /** The Plex-compatible API on addresses of its own (port 32400): none when off or on pley's port. */
  plex: string[];
  /** The Plex-compatible API answers on pley's own port, to its apps and tools (the default). */
  plex_on_web: boolean;
  /** Whether the pley apps on the networks find the server (UDP). */
  discovery: boolean;
  /** Whether pley announces itself by Bonjour, for Apple's apps. */
  bonjour: boolean;
  /** The adapters Bonjour announces from, by name: every one the web app listens on when empty. */
  bonjour_adapters: string[];
  /** Bonjour's host name, the label before `.local`: the generated one when empty. */
  bonjour_host_name: string;
}

export type ListenerKind = "web" | "jellyfin" | "plex" | "discovery";

export interface NetworkListener {
  kind: ListenerKind;
  address: string;
  /** Why pley doesn't listen there (yet). */
  error: string | null;
}

export interface NetworkAdapter {
  /** "Wi-Fi", "Ethernet", "eth0". */
  name: string;
  /** The hardware, on Windows. */
  description: string | null;
  /** Connected. */
  up: boolean;
  /** Its IPv4 addresses. */
  addresses: string[];
  /** The network it is on, as Windows knows it. */
  network: { name: string; kind: "public" | "private" | "domain" } | null;
}

export interface NetworkSettings extends ListenChoice {
  /** What the command line or the environment sets: it can't change here. */
  fixed: { web: boolean; jellyfin: boolean; plex: boolean; discovery: boolean; bonjour: boolean };
  listeners: NetworkListener[];
  adapters: NetworkAdapter[];
  /** Bonjour's host name when none is chosen (`pley-` and the server id's start). */
  bonjour_host_default: string;
  /** Where a change is saved. */
  config_file: string;
  /** Windows Firewall decides which kinds of network reach pley. */
  windows: boolean;
}

/** The managed transcoder's work folder (Settings → Transcoder). */
export interface TranscodeDir {
  /** The folder it works in now. */
  in_use: string;
  /** The one pley's next start uses, when that is another. */
  next: string | null;
  /** `transcode` in the data folder: used when none is chosen, or the one chosen can't be. */
  default: string;
  /** The folder chosen, when it couldn't be used at start, and why. */
  problem: { folder: string; reason: string } | null;
  /** The command line or the environment chooses it. */
  fixed: boolean;
}

export interface SystemInfo {
  name: string;
  version: string;
  commit?: string;
  os: string;
  arch: string;
  uptime_secs: number;
  data_dir: string;
  /**
   * The server can use its secret.key: without it, authenticator apps can't
   * be set up or checked. Absent from servers before it.
   */
  secret_key_ok?: boolean;
  transcode_dir: TranscodeDir;
  transcoder: TranscoderInfo;
  counts: {
    libraries: number;
    movies: number;
    shows: number;
    episodes: number;
    artists: number;
    albums: number;
    tracks: number;
    files: number;
    users: number;
  };
}

/** The machine and pley (the server, its transcoder and their FFmpegs) at one moment. */
export interface ResourceSample {
  /** Unix milliseconds. */
  at: number;
  /** Share of all the processors' time, 0–100. */
  cpu: number;
  pley_cpu: number;
  /** Bytes in use. */
  memory: number;
  pley_memory: number;
}

export interface Resources {
  available: boolean;
  interval_ms: number;
  cpus: number;
  memory_total: number;
  /** Oldest first, the last five minutes. */
  samples: ResourceSample[];
}

export interface FsEntry {
  name: string;
  path: string;
}

export interface FsListing {
  path: string | null;
  parent: string | null;
  entries: FsEntry[];
}

export type LibrarySort =
  | "title"
  | "year"
  | "added"
  | "rating"
  | "release"
  | "duration"
  | "last_played"
  | "user_rating"
  | "bitrate";
export type SortOrder = "asc" | "desc";

export interface LibraryItemsQuery {
  sort?: LibrarySort;
  order?: SortOrder;
  watched?: boolean;
  genre?: string;
  year?: number;
  resolution?: Resolution;
  dynamic_range?: RangeFilter;
  offset?: number;
  limit?: number;
}

export interface LibraryItemsResponse {
  total: number;
  items: ItemSummary[];
}

/** A title's best file's resolution, as libraries filter by it. */
export type Resolution = "4k" | "1080" | "720" | "sd";

/** A title's best dynamic range, as libraries filter by it; `hdr` is any but SDR. */
export type RangeFilter = "hdr" | DynamicRange;

export interface LibraryFilters {
  genres: string[];
  years: number[];
  /** Best first; only those the titles have, and only two or more (older servers: none). */
  resolutions?: Resolution[];
  dynamic_ranges?: RangeFilter[];
}

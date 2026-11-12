//! The playback of one video, then of what plays after it, on one engine
//! (packages/tv/src/player/controller.ts, the Android app's
//! `SessionPlaybackController`):
//!
//! - **Start.** The item's details and the TV's capabilities come together,
//!   then `POST /api/playback`, the Dolby Vision guards, and the engine loads
//!   the answer, at once at the resume point.
//! - **Reports.** Progress every 10 s while playing and every 30 s paused, on
//!   pause, and 1 s after the last seek; `stopped` then DELETE at the end, on
//!   closing and before any new session.
//! - **Recovery.** The ladder for errors, 12 s stalls and dropped frames; a
//!   server out of reach is waited for 90 s.
//! - **Tracks.** Audio switched by the engine in direct play, a new session
//!   for streams; subtitles drawn by the TV (the server's WebVTT, or its
//!   `.mks` of a picture track), burned in only when the server can't make
//!   the `.mks`. Choices are saved with `PUT /api/items/{id}/streams`. The
//!   user's remote picks them the same way, and every report says which play.
//! - **Quality.** The menu, Original, and Automatic with LG's signals (waits
//!   and buffer growth) and in-place bitrate changes.
//! - **Markers, Up Next, the sleep timer, speed, Playback details**, and a
//!   session the server stopped: it says so and plays no more.
//! - **A video on a drive** ([`PlayerServer::local`](crate::server::PlayerServer::local)):
//!   played as it is with no server behind it, so nothing is converted or
//!   asked of a server, a 12 s stall fails at once, audio always switches in
//!   place, and picture subtitles come as they are read out of the file.
//!
//! Nothing here blocks: server calls run on the task pool and come back
//! through promises that [`PlaybackController::tick`] collects; time comes
//! in with each tick (ms of the app's clock).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use pley_base::Rect;
use pley_core::caps::decide;
use pley_core::format::{episode_label, stream_label, two_letter_code};
use pley_core::http::sentence;
use pley_core::media::{
    AudioFacts, AudioTrack, EngineEvent, EngineStats, LoadRequest, MediaKind, PlayerEngine, VideoFacts,
};
use pley_core::model::{
    AddSubtitle, AudioChoice, BitrateAnswer, ClientCaps, Control, DynamicRange, ItemDetail, ItemKind, ItemSummary,
    Marker, MediaFile, MediaStream, PlaybackInfo, PlaybackMethod, PlaybackMode, ProgressReport, ProgressState,
    ServerEvent, SessionStatus, StreamChoice, StreamKind, SubtitleResult, SubtitleTrack, TranscodeInfo, TrickplayInfo,
};
use pley_core::task::{Cancel, Promise};
use pley_core::{ApiError, CapsReport};
use pley_i18n::t;
use pley_subs::{Fetch, PictureAnswer, PictureTrack, Presenter, Shown, TrackKind, Trouble, clamp_offset};

use crate::autoquality::{AutoOptions, AutoQuality, AutoStep, FIRST_MINUTE_MS, LiveStream, Remembered};
use crate::details::{self, DetailsInput, PlaybackDetails};
use crate::env::{PlayerEnv, ThroughShared};
use crate::markers::{CreditsGate, active_marker, markers_of, skip_label, skip_target};
use crate::nextup::{
    TRICKPLAY_GIVE_UP_MS, TRICKPLAY_RETRY_MS, has_next, label_of, next_after, preview_bucket, sheet_path, start_of,
    thumbnail_at,
};
use crate::plan::{
    self, Choices, PlayRequest, audio_plays_here, audio_streams, choose_file, dv_container_mode, dv_retry_mode,
    initial_audio, initial_subtitle, is_keyed, is_picture, mime_type_of, needs_indexes, picture_path, playback_request,
    playing_audio, subtitle_streams, text_path, video_stream_of, will_stream, with_width,
};
use crate::prepare::{PREPARE_LEAD_MS, PREPARE_MAX_MS, warm_stream};
use crate::quality::{
    AUTO_ID, ORIGINAL_ID, Quality, auto_ladder, choose_quality, file_hint, live_floor, live_label, named_for,
    offered_qualities, preset, presets, qualities_for, stream_lines,
};
use crate::recovery::{EngineError, Failure, FallbackReason, Recovery, RecoveryAction, StruggleMeter, failure_text};
use crate::remote::{self, Command, RemoteAction, RemoteKind, RemotePlayer, remote_action, video_controls_with_tracks};
use crate::sleep::{SleepChoice, SleepItem, SleepState, SleepTimer};

/// The clock of the controller's own checks.
pub const TICK_MS: i64 = 500;
/// Buffering this soon after a seek is the seek's, not a stall.
const SEEK_SETTLE_MS: i64 = 2_000;
/// Progress while it plays.
pub const PROGRESS_MS: i64 = 10_000;
/// While paused: the server forgets players it hasn't heard from for 90 s.
pub const PAUSED_HEARTBEAT_MS: i64 = 30_000;
/// A report this long after the last seek.
pub const SEEK_REPORT_MS: i64 = 1_000;
/// Not moving this long while meant to play is a stall.
pub const STALL_MS: i64 = 12_000;
/// "Ended" less than 5 s after its start and more than a minute before its end is no end.
pub const SHORT_END_MS: i64 = 60_000;
/// See [`SHORT_END_MS`].
pub const SHORT_PLAY_MS: i64 = 5_000;
/// Less than this buffered ahead during a stall is starving.
pub const STARVING_MS: i64 = 1_000;
/// Dropped frames are sampled this often while it plays.
pub const STRUGGLE_SAMPLE_MS: i64 = 2_000;
/// Frames dropped while the TV's player switches a file's audio track aren't
/// the video struggling: for this long after a switch they aren't counted.
pub const TRACK_SWITCH_QUIET_MS: i64 = 5_000;
/// Playback details refresh this often while open.
pub const DETAILS_MS: i64 = 2_000;
/// Up Next's countdown.
pub const COUNTDOWN_SECONDS: i64 = 10;
/// The longest a new session waits for the one before to be ended.
pub const END_WAIT_MS: u64 = 2_000;
/// Seeks stop this short of the end, so the end isn't reached by a seek.
pub const SEEK_END_MARGIN_MS: i64 = 250;
/// Up Next's picture is asked for this wide (`?w=480`).
pub const UP_NEXT_IMAGE_WIDTH: u32 = 480;
/// The server out of reach mid-playback is asked again this long, as long as it waits for a player it doesn't hear from.
pub const OUTAGE_MS: i64 = 90_000;
/// Between its questions: 0.5 s, 1 s, then every 2 s.
pub const OUTAGE_STEPS_MS: [i64; 3] = [500, 1_000, 2_000];
/// How long each question waits for an answer.
pub const OUTAGE_ASK_MS: u64 = 5_000;
/// The engine moving this far past where the server went means it came back by itself.
pub const OUTAGE_MOVED_MS: i64 = 1_000;
/// A chapter picture stands in for a seek preview at this width (240 dp × 2).
pub const CHAPTER_PICTURE_PX: u32 = 480;
/// A video on a drive's picture subtitles still being read out of it are
/// asked for again after 1 s, then twice as long each time, 4 s at most.
pub const LOCAL_PICTURES_FIRST_MS: u64 = 1_000;
/// See [`LOCAL_PICTURES_FIRST_MS`].
pub const LOCAL_PICTURES_MOST_MS: u64 = 4_000;

/// Where the player is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Preparing,
    Buffering,
    Playing,
    Paused,
    /// The end, with nothing next: the player leaves.
    Ended,
    /// `title` replaces "Can't play this item" ("Can't reach your server").
    Error {
        message: String,
        can_retry: bool,
        title: Option<String>,
    },
}

impl Phase {
    fn same_kind(&self, other: &Phase) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

/// An audio or subtitle track as the menus show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackChoice {
    /// The server's stream index; none for "Off".
    pub index: Option<i64>,
    pub label: String,
    pub selected: bool,
    /// Pictures a stream draws onto the video ("burn-in").
    pub burn_in: bool,
    /// Subtitles added on the server (found, or a file uploaded on the web).
    pub added: bool,
}

/// A row of the Quality menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QualityChoice {
    pub id: &'static str,
    pub label: String,
    pub hint: Option<String>,
    pub selected: bool,
}

/// A chapter of the file playing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChapterMark {
    pub title: Option<String>,
    pub start_ms: i64,
    pub end_ms: i64,
    /// Absolute.
    pub image_url: Option<String>,
}

/// The Up Next card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpNextCard {
    pub item_id: i64,
    pub title: String,
    /// "Up next · S1 · E2".
    pub heading: String,
    /// Absolute, 480 px wide.
    pub image_url: Option<String>,
    /// Seconds before it plays by itself; none when it won't.
    pub seconds_left: Option<i64>,
}

/// A file's seek previews.
#[derive(Clone, Debug, PartialEq)]
pub struct Trickplay {
    pub file_id: i64,
    pub info: TrickplayInfo,
}

/// What a seek preview shows for a target.
#[derive(Clone, Debug, PartialEq)]
pub enum SeekPreview {
    /// A tile of a sheet: the sheet's absolute address and the tile's rectangle in its pixels.
    Sheet { url: String, x: i64, y: i64, width: i64, height: i64 },
    /// No previews: the chapter's own picture (absolute, 480 px wide).
    Chapter { url: String },
}

/// Find subtitles: where the search for a file's subtitles stands.
#[derive(Clone, Debug, PartialEq)]
pub enum SubtitleSearch {
    Idle,
    Searching,
    Found(Vec<SubtitleResult>),
    /// The server's answer: 404 is an older server, 503 one not set up for it.
    Failed(ApiError),
    /// A result is being added.
    Adding(String),
}

/// What the player screen shows, but the clock and the subtitles.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerState {
    pub phase: Phase,
    pub item_id: i64,
    /// What plays: a movie, an episode, an extra; none until its details come.
    pub kind: Option<ItemKind>,
    /// A movie's "Title (Year)", an episode's show.
    pub title: String,
    /// An episode's "S1 · E2 · Title".
    pub subtitle: Option<String>,
    pub audio: Vec<TrackChoice>,
    pub subtitles: Vec<TrackChoice>,
    /// The Quality row's value: "Automatic · 8 Mbps 1080p", "Original".
    pub quality: String,
    pub qualities: Vec<QualityChoice>,
    /// "Direct play", "Transcoding"…
    pub method: Option<String>,
    /// Why it doesn't play the original.
    pub note: Option<String>,
    pub can_play_original: bool,
    pub suggest_lower_quality: bool,
    pub skip_label: Option<String>,
    pub markers: Vec<Marker>,
    pub chapters: Vec<ChapterMark>,
    pub trickplay: Option<Trickplay>,
    pub up_next: Option<UpNextCard>,
    pub details: Option<PlaybackDetails>,
    /// A one-shot toast; `notice_id` changes with each one.
    pub notice: Option<String>,
    pub notice_id: u64,
    /// What the user's remote on another device just did here ("Paused", "+30 s"), for the
    /// screen to say briefly in place of its controls ([`crate::remote`]); `remote_notice_id`
    /// changes with each one.
    pub remote_notice: Option<String>,
    pub remote_notice_id: u64,
    /// The remote paused it, and nothing has played it since.
    pub paused_by_remote: bool,
    pub subtitle_offset_ms: i32,
    /// The Sync row shows.
    pub subtitle_sync: bool,
    /// How fast it plays (1: normal), for this title.
    pub speed: f32,
    /// The speeds offered; empty when only normal.
    pub speeds: Vec<f32>,
    pub sleep: SleepState,
    pub stopped_by_server: bool,
    /// The server stopped answering mid-playback: "Reconnecting…".
    pub reconnecting: bool,
    /// Time to the first picture of the session playing, in ms.
    pub first_frame_ms: Option<i64>,
    /// Times the video stopped to wait for more once it played, seeks aside (Info's Stalls).
    pub stalls: u32,
    /// It moves to a session made ready beside the last one: the ring waits longer (2.5 s).
    pub switching: bool,
    pub search: SubtitleSearch,
    /// Find subtitles is offered: a server searches for them and adds them
    /// (none does for a video on a drive).
    pub can_find_subtitles: bool,
}

impl Default for PlayerState {
    fn default() -> PlayerState {
        PlayerState {
            phase: Phase::Preparing,
            item_id: 0,
            kind: None,
            title: String::new(),
            subtitle: None,
            audio: Vec::new(),
            subtitles: Vec::new(),
            quality: t!("player.quality.original").into(),
            qualities: Vec::new(),
            method: None,
            note: None,
            can_play_original: false,
            suggest_lower_quality: false,
            skip_label: None,
            markers: Vec::new(),
            chapters: Vec::new(),
            trickplay: None,
            up_next: None,
            details: None,
            notice: None,
            notice_id: 0,
            remote_notice: None,
            remote_notice_id: 0,
            paused_by_remote: false,
            subtitle_offset_ms: 0,
            subtitle_sync: false,
            speed: 1.0,
            speeds: Vec::new(),
            sleep: SleepState::default(),
            stopped_by_server: false,
            reconnecting: false,
            first_frame_ms: None,
            stalls: 0,
            switching: false,
            search: SubtitleSearch::Idle,
            can_find_subtitles: true,
        }
    }
}

/// The player's clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerClock {
    pub position_ms: i64,
    pub duration_ms: i64,
    /// Where what is loaded ends (absolute).
    pub buffered_ms: i64,
}

/// Where a closed or hidden player was, to start again from there.
#[derive(Clone, Debug, PartialEq)]
pub struct SavedPlayback {
    pub request: PlayRequest,
    pub quality: String,
    pub playing: bool,
}

/// What the engine said of itself, as the controller sees it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EngineStatus {
    #[default]
    Idle,
    Buffering,
    Ready,
    Ended,
}

#[derive(Default)]
struct EngineView {
    status: EngineStatus,
    playing: bool,
    duration_ms: Option<u64>,
    /// Where what is loaded ends, when the engine says.
    buffered_end: Option<i64>,
    tracks: Option<Vec<AudioTrack>>,
}

/// An engine error in the terms recovery decides on. Codes follow
/// `MediaError` (2 network, 3 decode, 4 source not supported) as LG's web
/// engine gave them; an HTTP status (100–599) says the server refused.
pub fn engine_error(code: i32, message: &str, hls: bool) -> EngineError {
    let lower = message.to_ascii_lowercase();
    let audio = lower.contains("audio");
    let detail = |default: &str| if message.trim().is_empty() { default.to_string() } else { message.to_string() };
    match code {
        2 => EngineError::Network { detail: detail("the network failed while loading") },
        3 => EngineError::Unsupported { audio, container: false, detail: detail("the TV couldn't decode it") },
        4 if hls => EngineError::Playlist { detail: detail("the stream couldn't be opened") },
        4 => EngineError::Unsupported {
            audio,
            container: ["demux", "container", "format"].iter().any(|w| lower.contains(w)),
            detail: detail("the TV can't play this file"),
        },
        100..=599 => EngineError::Http { status: code as u16, detail: detail("the server refused") },
        _ => EngineError::Other { detail: detail("playback failed") },
    }
}

/// The server stopped answering: since when, how often it was asked, and whether the engine gave up too.
struct Outage {
    id: u64,
    since: i64,
    attempts: usize,
    due: Option<i64>,
    engine_failed: bool,
    at_ms: i64,
}

/// What the item's details, the caps and the network's memory are gathered for.
enum Then {
    /// The first start.
    Start,
    /// Up Next or Play next: `req`, after the session before ended.
    Switch { req: PlayRequest, flush: bool },
}

/// The parts a start waits for, as they come.
struct Gather {
    g: u64,
    then: Then,
    item: Option<Result<ItemDetail, ApiError>>,
    caps: Option<Option<CapsReport>>,
    known: Option<Option<Option<Remembered>>>,
}

/// What a session was asked with, to take its answer.
#[derive(Clone)]
struct Asked {
    at_ms: i64,
    alongside: bool,
    choices: Choices,
    original: bool,
    /// The audio picked, sent with the request; none: the server's choice.
    audio: Option<MediaStream>,
    /// A restart (mid-playback): a server out of reach is waited for.
    restart: bool,
}

/// The wait for the server that a new session ended, which goes on if that session fails the
/// same way (a transcoder still restarting): `g`, the start it gave way to.
#[derive(Clone, Copy)]
struct Away {
    g: u64,
    since: i64,
    attempts: usize,
}

/// A session opened by a task.
struct Opened {
    info: PlaybackInfo,
    /// The Dolby Vision guard it took, kept for the item.
    guard: Option<PlaybackMode>,
}

enum Opening {
    /// 503 to a session asked for beside another: no transcode free for two.
    Busy,
    /// A newer start took over: the session was let go.
    Stale,
    Failed(ApiError),
}

/// Why the session's status was asked.
enum Check {
    Error(EngineError),
    Stall { starving: bool },
    Server { starving: bool },
}

/// What a lookup of the next item is for.
enum NextFor {
    Credits,
    Ended {
        sleeping: bool,
    },
    PlayNext,
    /// The remote's next, pressed on another device: nothing next says nothing here.
    Remote,
}

/// What a task hands back.
enum Done {
    Item {
        g: u64,
        result: Result<ItemDetail, ApiError>,
    },
    Caps {
        g: u64,
        report: Option<CapsReport>,
    },
    Known {
        g: u64,
        known: Option<Option<Remembered>>,
    },
    Session {
        g: u64,
        asked: Box<Asked>,
        result: Result<Opened, Opening>,
    },
    Check {
        session: String,
        check: Check,
        result: Result<SessionStatus, ApiError>,
    },
    Reach {
        outage: u64,
        result: Result<(), ApiError>,
    },
    Bitrate {
        session: String,
        auto: u64,
        asked: f64,
        result: Result<BitrateAnswer, ApiError>,
    },
    Next {
        lookup: u64,
        purpose: NextFor,
        result: Result<Option<ItemSummary>, ApiError>,
    },
    NextDetail {
        saved: u64,
        result: Result<ItemDetail, ApiError>,
    },
    Trickplay {
        file_id: i64,
        result: Result<TrickplayInfo, ApiError>,
    },
    Details {
        result: Result<SessionStatus, ApiError>,
    },
    /// A subtitle file's answer, and when it was asked for (the server's pictures are asked for
    /// again counting from the ask).
    Subtitle {
        fetch: Fetch,
        asked: i64,
        result: Result<crate::server::SubtitleAnswer, ApiError>,
    },
    Pictures {
        fetch: Fetch,
        result: Result<(PictureTrack, bool), ApiError>,
    },
    Found {
        search: u64,
        result: Result<Vec<SubtitleResult>, ApiError>,
    },
    Added {
        result: Result<SubtitleTrack, ApiError>,
    },
}

/// A session made ready beside the one playing, waiting for its stream.
struct Warming {
    g: u64,
    info: PlaybackInfo,
    asked: Asked,
    deadline: i64,
    promise: Promise<()>,
}

/// The tracks the session plays, as its reports tell the user's remotes (docs/API.md, "Remote
/// control"). A pick that needs a new session changes them once that session plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlayingTracks {
    /// The audio's stream index; none while not known.
    audio: Option<i64>,
    /// The subtitles shown or burned in, by stream index; -1 for none.
    subtitle: i64,
}

impl PlayingTracks {
    const NONE: PlayingTracks = PlayingTracks { audio: None, subtitle: -1 };
}

/// One video's playback, then what plays after it.
pub struct PlaybackController {
    env: PlayerEnv,
    engine: Box<dyn PlayerEngine>,
    view: EngineView,
    state: PlayerState,
    clock: PlayerClock,
    /// Bumped by every change of what the screen shows.
    version: u64,

    current: PlayRequest,
    item: Option<ItemDetail>,
    caps: Option<ClientCaps>,
    dv_containers: Option<Vec<String>>,
    file: Option<MediaFile>,
    audio: Option<MediaStream>,
    /// The audio was picked (asked for, or in the menu), not chosen by the server: it is saved for next time.
    audio_picked: bool,
    /// The file's audio track to switch to once the engine lists its tracks.
    audio_pending: Option<(usize, Option<String>)>,
    subtitle: Option<MediaStream>,
    /// What the session plays of them, for its reports.
    tracks_now: PlayingTracks,
    /// The quality chosen in the menu: Automatic, the original, or a conversion.
    quality: Quality,
    /// The server converts no video, as its last play answer had it
    /// (`original_only`): only the original is offered, and plays.
    original_only: bool,
    /// Automatic, when chosen.
    auto: Option<AutoQuality>,
    /// Bumped for each Automatic made: answers for an older one are dropped.
    auto_id: u64,
    markers: Vec<Marker>,
    /// The mode a Dolby Vision guard asked for, for this item's later sessions too.
    guard_mode: Option<PlaybackMode>,

    info: Option<PlaybackInfo>,
    session_ended: bool,
    /// The remote picked a quality that needs a new session: that session reports as it begins, so
    /// the remote sees the pick (its play request doesn't say it).
    report_begun: bool,
    /// Bumped by every new start; shared with the tasks opening sessions.
    generation: Arc<AtomicU64>,
    /// The generation of a session being made ready beside the one playing.
    preparing: Option<u64>,
    warming: Option<Warming>,

    last_position_ms: i64,
    started_at_ms: i64,
    media_shown: bool,
    /// When a seek of the player's own was asked (its buffering is no stall).
    seeked_at: i64,
    want_play: bool,
    ended: bool,
    closed: bool,
    /// The server stopped the playback: nothing plays again.
    stopped: bool,
    /// The server has no `.mks` of picture subtitles: they are burned in.
    pictures_unavailable: bool,
    offset: i32,
    /// Something has played: a lost server is waited for from now on, rather than failing at once.
    shown_once: bool,
    outage: Option<Outage>,
    outages: u64,
    away: Option<Away>,

    recovery: Recovery,
    struggle: StruggleMeter,
    credits: CreditsGate,
    presenter: Presenter,
    sleep: SleepTimer,

    last_advance_at: i64,
    last_advance_pos: i64,
    last_struggle_sample: i64,
    frames_quiet_until: i64,
    session_started_at: i64,
    /// How long the title has played (its picture moving), for Automatic's first minute.
    title_played_ms: i64,
    moving_at: Option<i64>,
    live: Option<TranscodeInfo>,

    now: i64,
    next_tick: i64,
    progress_at: Option<i64>,
    seek_report_at: Option<i64>,
    countdown: Option<(i64, i64, i64)>,
    details_at: Option<i64>,
    details_open: bool,
    details_asking: bool,
    trickplay_at: Option<(i64, i64)>,
    trickplay_started: i64,
    subtitle_fetch: Option<(Fetch, i64)>,
    pressed_at: Option<i64>,

    next: Option<ItemSummary>,
    next_detail: Option<ItemDetail>,
    choices_saved: u64,
    next_lookup: u64,
    searches: u64,

    gather: Option<Gather>,
    jobs: Vec<Promise<Done>>,
}

impl PlaybackController {
    /// A player for `request` on `engine`, asked for at `now` (ms of the app's
    /// clock: the first picture is timed from it). Nothing happens before [`PlaybackController::start`].
    pub fn new(request: PlayRequest, env: PlayerEnv, engine: Box<dyn PlayerEngine>, now: i64) -> PlaybackController {
        let speeds = env.speeds.clone();
        let local = env.server.local();
        let mut presenter = Presenter::new();
        if local {
            presenter.set_drive_retry(LOCAL_PICTURES_FIRST_MS, LOCAL_PICTURES_MOST_MS);
        }
        PlaybackController {
            env,
            engine,
            view: EngineView::default(),
            state: PlayerState {
                item_id: request.item_id,
                speeds,
                can_find_subtitles: !local,
                ..PlayerState::default()
            },
            clock: PlayerClock::default(),
            version: 0,
            current: request,
            item: None,
            caps: None,
            dv_containers: None,
            file: None,
            audio: None,
            audio_picked: false,
            audio_pending: None,
            subtitle: None,
            tracks_now: PlayingTracks::NONE,
            quality: Quality::original(),
            original_only: false,
            auto: None,
            auto_id: 0,
            markers: Vec::new(),
            guard_mode: None,
            info: None,
            session_ended: true,
            report_begun: false,
            generation: Arc::new(AtomicU64::new(0)),
            preparing: None,
            warming: None,
            last_position_ms: 0,
            started_at_ms: 0,
            media_shown: false,
            seeked_at: i64::MIN / 2,
            want_play: true,
            ended: false,
            closed: false,
            stopped: false,
            pictures_unavailable: false,
            offset: 0,
            shown_once: false,
            outage: None,
            outages: 0,
            away: None,
            recovery: Recovery::default(),
            struggle: StruggleMeter::default(),
            credits: CreditsGate::default(),
            presenter,
            sleep: SleepTimer::default(),
            last_advance_at: now,
            last_advance_pos: -1,
            last_struggle_sample: 0,
            frames_quiet_until: 0,
            session_started_at: now,
            title_played_ms: 0,
            moving_at: None,
            live: None,
            now,
            next_tick: now + TICK_MS,
            progress_at: None,
            seek_report_at: None,
            countdown: None,
            details_at: None,
            details_open: false,
            details_asking: false,
            trickplay_at: None,
            trickplay_started: now,
            subtitle_fetch: None,
            pressed_at: Some(now),
            next: None,
            next_detail: None,
            choices_saved: 0,
            next_lookup: 0,
            searches: 0,
            gather: None,
            jobs: Vec::new(),
        }
    }

    // ---------------------------------------------------------------- what the screen reads

    /// What the screen shows.
    pub fn state(&self) -> &PlayerState {
        &self.state
    }

    /// The clock, as of the last tick.
    pub fn clock(&self) -> PlayerClock {
        self.clock
    }

    /// Changes with every change of [`PlaybackController::state`] or the clock: the screen redraws when it moves.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The subtitles to draw now.
    pub fn subtitles(&self) -> &Shown {
        self.presenter.shown()
    }

    /// What the engine says of itself, and the TV's network speed, for Info.
    pub fn engine_stats(&self) -> EngineStats {
        EngineStats { network_bps: self.engine.network_bps(), ..self.engine.stats() }
    }

    /// What the engine says of itself, for Playback details' readout: it shows no network speed,
    /// which on LG means reading a file.
    pub fn readout_stats(&self) -> EngineStats {
        self.engine.stats()
    }

    /// Whether the player means to play (play/pause shows the other).
    pub fn wants_to_play(&self) -> bool {
        self.want_play
    }

    /// The item playing, once its details came.
    pub fn item(&self) -> Option<&ItemDetail> {
        self.item.as_ref()
    }

    /// The file playing.
    pub fn file(&self) -> Option<&MediaFile> {
        self.file.as_ref()
    }

    /// The session playing.
    pub fn session(&self) -> Option<&PlaybackInfo> {
        self.info.as_ref()
    }

    /// Whether a server call is still out or an answer waits to be taken (tests
    /// wait for them). A stream being made ready doesn't count until it is.
    pub fn busy(&self) -> bool {
        !self.jobs.is_empty()
            || self.warming.as_ref().is_some_and(|w| w.promise.ready() || w.promise.failed())
            || !self.env.reporter.idle()
    }

    /// Whether a session is being made ready beside the one playing (its stream warming).
    pub fn warming(&self) -> bool {
        self.warming.is_some()
    }

    /// Where the video is now, asked of the engine between ticks; the clock's position while it has nothing to say.
    pub fn live_position_ms(&self) -> i64 {
        if !self.media_shown || self.ended || self.closed {
            return self.clock.position_ms;
        }
        self.engine.position_ms() as i64
    }

    fn live_position(&self) -> i64 {
        if self.media_shown && self.info.is_some() && !self.session_ended && !self.ended {
            self.engine.position_ms() as i64
        } else {
            self.last_position_ms
        }
    }

    /// When [`PlaybackController::tick`] has something to do next (ms of the app's clock).
    pub fn next_wake(&self) -> Option<i64> {
        if self.closed {
            return None;
        }
        let mut due = vec![Some(self.next_tick)];
        due.extend([self.progress_at, self.seek_report_at, self.details_at, self.sleep.next_due()]);
        due.push(self.countdown.map(|c| c.2));
        due.push(self.trickplay_at.map(|t| t.0));
        due.push(self.outage.as_ref().and_then(|o| o.due));
        due.push(self.warming.as_ref().map(|w| w.deadline));
        due.push(self.subtitle_fetch.as_ref().map(|f| f.1));
        if self.presenter.showing().is_some() {
            let speed = if self.want_play { self.state.speed } else { 1.0 };
            due.push(Some(self.now + self.presenter.wait_ms(speed) as i64));
        }
        due.into_iter().flatten().min()
    }

    // ---------------------------------------------------------------- the clock

    /// Collects answers and engine events and runs what is due at `now`; true when what the screen shows changed.
    pub fn tick(&mut self, now: i64) -> bool {
        let before = self.version;
        self.now = now.max(self.now);
        if self.closed {
            return false;
        }
        self.collect();
        for event in self.engine.events() {
            self.on_engine(event);
        }
        self.timers();
        if self.now >= self.next_tick {
            self.next_tick = self.now + TICK_MS;
            self.check();
        }
        self.subtitles_now();
        self.collect();
        self.version != before
    }

    fn changed(&mut self) {
        self.version += 1;
    }

    fn timers(&mut self) {
        let now = self.now;
        if self.progress_at.is_some_and(|t| now >= t) {
            self.progress_at = None;
            // Not while reconnecting: unanswered reports would queue up behind each other.
            if !self.ended && self.outage.is_none() && !self.in_error() {
                self.report(if self.want_play { ProgressState::Playing } else { ProgressState::Paused });
            }
            self.arm_progress();
        }
        if self.seek_report_at.is_some_and(|t| now >= t) {
            self.seek_report_at = None;
            self.report(if self.want_play { ProgressState::Playing } else { ProgressState::Paused });
        }
        if self.sleep.next_due().is_some_and(|t| now >= t) {
            let item = self.sleep_item();
            let fired = self.sleep.tick(now, item);
            self.sleep_changed();
            if fired {
                self.on_sleep();
            }
        }
        if let Some((item_id, left, at)) = self.countdown
            && now >= at
        {
            self.countdown_step(item_id, left);
        }
        if self.details_at.is_some_and(|t| now >= t) && !self.details_asking {
            self.details_at = None;
            self.details_loop();
        }
        if let Some((at, file_id)) = self.trickplay_at
            && now >= at
        {
            self.trickplay_at = None;
            self.ask_trickplay(file_id);
        }
        if let Some(o) = &mut self.outage
            && o.due.is_some_and(|t| now >= t)
        {
            o.due = None;
            let id = o.id;
            self.ask_again(id);
        }
        if self.warming.as_ref().is_some_and(|w| now >= w.deadline || w.promise.ready() || w.promise.failed()) {
            let w = self.warming.take().expect("checked");
            self.warmed(w);
        }
    }

    fn collect(&mut self) {
        loop {
            let mut done = Vec::new();
            let mut i = 0;
            while i < self.jobs.len() {
                if self.jobs[i].ready() {
                    let mut p = self.jobs.swap_remove(i);
                    if let Some(d) = p.take() {
                        done.push(d);
                    }
                } else if self.jobs[i].failed() {
                    self.jobs.swap_remove(i);
                } else {
                    i += 1;
                }
            }
            if done.is_empty() {
                return;
            }
            for d in done {
                self.on_done(d);
            }
        }
    }

    fn spawn(&mut self, f: impl FnOnce() -> Done + Send + 'static) {
        let p = self.env.tasks.spawn(f);
        self.jobs.push(p);
    }

    // ---------------------------------------------------------------- start

    /// Starts: the item's details, the TV's capabilities and the network's
    /// memory are asked for together, then the first session.
    pub fn start(&mut self) {
        self.prepare_first();
    }

    fn prepare_first(&mut self) {
        let g = self.next_generation();
        let id = self.current.item_id;
        self.gather = Some(Gather { g, then: Then::Start, item: None, caps: None, known: None });
        let server = self.env.server.clone();
        self.spawn(move || Done::Item { g, result: server.item(id) });
        let caps = self.env.caps.clone();
        self.spawn(move || Done::Caps { g, report: caps() });
        self.network_start(g);
    }

    /// Asks the server's memory of networks for Automatic's start; none for a quality picked.
    fn network_start(&mut self, g: u64) {
        let remembered = self.env.quality.remembered();
        let auto =
            choose_quality(remembered.as_deref(), &presets()).id == AUTO_ID && !self.local() && !self.original_only;
        match self.env.network.clone().filter(|_| auto) {
            Some(network) => self.spawn(move || Done::Known { g, known: network.start() }),
            None => {
                if let Some(gather) = &mut self.gather {
                    gather.known = Some(None);
                }
            }
        }
    }

    fn gathered(&mut self) {
        let Some(gather) = &self.gather else { return };
        if gather.item.is_none() || gather.caps.is_none() || gather.known.is_none() {
            return;
        }
        let gather = self.gather.take().expect("checked");
        if gather.g != self.current_generation() || self.closed {
            return;
        }
        let detail = match gather.item.expect("checked") {
            Ok(d) => d,
            Err(e) => return self.fail_with(&e),
        };
        if let Some(Some(report)) = gather.caps {
            self.caps = Some(report.caps);
            // An empty list says nothing (a report without the pipeline's facts): no container guard then.
            self.dv_containers = Some(report.dv_containers).filter(|c| !c.is_empty());
        }
        let known = gather.known.expect("checked");
        let (req, flush) = match gather.then {
            Then::Start => (self.current.clone(), false),
            Then::Switch { req, flush } => (req, flush),
        };
        let at = plan::start_ms(&detail, &req);
        self.load(detail, req, known);
        self.begin(at, gather.g, false, false, flush);
    }

    /// Takes `detail` as the item playing, with `req`'s choices; `known`: where the server says Automatic may start.
    fn load(&mut self, detail: ItemDetail, req: PlayRequest, known: Option<Option<Remembered>>) {
        let chosen = choose_file(&detail, req.file_id, self.caps.as_ref()).cloned();
        self.audio = initial_audio(chosen.as_ref(), req.audio_index);
        self.audio_picked = self.audio.is_some();
        self.subtitle = initial_subtitle(chosen.as_ref(), req.subtitle_index);
        // Nothing converts a file on a drive, nor a server that converts no
        // video: it plays as it is, without Automatic.
        self.quality = if self.local() || self.original_only {
            Quality::original()
        } else {
            choose_quality(self.env.quality.remembered().as_deref(), &qualities_for(chosen.as_ref()))
        };
        self.title_played_ms = 0;
        self.moving_at = None;
        self.current = req;
        self.file = chosen;
        self.start_auto(known);
        self.offset = match &self.file {
            Some(f) => self.env.offsets.get(self.env.server.server(), f.id, self.env.profile),
            None => 0,
        };
        self.markers = self.file.as_ref().map(|f| markers_of(&detail, f.id)).unwrap_or_default();
        let episode = detail.kind == ItemKind::Episode;
        let s = &mut self.state;
        s.item_id = detail.id;
        s.kind = Some(detail.kind);
        s.title = if episode {
            detail.grandparent_title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| detail.title.clone())
        } else {
            match detail.year {
                Some(y) => format!("{} ({y})", detail.title),
                None => detail.title.clone(),
            }
        };
        s.subtitle = episode.then(|| {
            [episode_label(detail.parent_index, detail.index), detail.title.clone()]
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(" · ")
        });
        s.markers = self.markers.clone();
        s.chapters = self
            .file
            .as_ref()
            .map(|f| {
                f.chapters
                    .iter()
                    .map(|c| ChapterMark {
                        title: c.title.clone(),
                        start_ms: c.start_ms,
                        end_ms: c.end_ms,
                        image_url: c.thumb_url.as_deref().map(|u| self.env.server.url(u)),
                    })
                    .collect()
            })
            .unwrap_or_default();
        s.trickplay = None;
        s.subtitle_offset_ms = self.offset;
        let duration = self.file.as_ref().and_then(|f| f.duration_ms).or(detail.duration_ms).unwrap_or(0);
        self.item = Some(detail);
        self.clock.duration_ms = duration;
        self.changed();
        self.publish_choices();
        // Only a server makes seek previews.
        let file_id = self.file.as_ref().map(|f| f.id).filter(|_| !self.local());
        self.trickplay_at = None;
        if let Some(id) = file_id {
            self.trickplay_started = self.now;
            self.ask_trickplay(id);
        }
    }

    // ---------------------------------------------------------------- sessions

    fn current_generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// A new start: what an older one finds when it comes back is no longer its own,
    /// and a session it was making ready is let go.
    fn next_generation(&mut self) -> u64 {
        self.preparing = None;
        if let Some(w) = self.warming.take() {
            w.promise.cancel();
            self.discard(&w.info);
        }
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Starts a server session at `at_ms`, ending the one before. `prepared`,
    /// for a change asked for while it plays (a quality, a track, an Automatic
    /// step): the new session is made ready while this one plays on. A
    /// recovery starts again at once.
    fn restart(&mut self, at_ms: i64, prepared: bool) {
        self.restart_after(at_ms, prepared, None);
    }

    /// [`Self::restart`], after a wait for the server that this start ends (`away`: its start
    /// and attempts), which goes on if the start fails the same way.
    fn restart_after(&mut self, at_ms: i64, prepared: bool, away: Option<(i64, usize)>) {
        if self.stopped || self.closed {
            return;
        }
        let alongside = prepared && self.can_prepare();
        let g = self.next_generation();
        self.away = away.map(|(since, attempts)| Away { g, since, attempts });
        self.begin(at_ms, g, alongside, true, false);
    }

    /// A session plays, has shown its picture, and nothing is wrong: the next can be made ready beside it.
    fn can_prepare(&self) -> bool {
        self.info.is_some()
            && !self.session_ended
            && self.media_shown
            && !self.ended
            && self.outage.is_none()
            && !self.in_error()
    }

    fn in_error(&self) -> bool {
        matches!(self.state.phase, Phase::Error { .. })
    }

    /// A video on a drive (the Samsung app's `IPlayerSource.Local`): it plays
    /// as it is, and what only a server does (converting, seek previews, the
    /// next episode, finding subtitles) isn't there.
    fn local(&self) -> bool {
        self.env.server.local()
    }

    /// The quality that plays: Automatic's of the moment, or the one chosen.
    fn playing_preset(&self) -> Quality {
        match &self.auto {
            Some(a) => named_for(self.file.as_ref(), &preset(&a.playing).unwrap_or_else(Quality::original)),
            None => self.quality.clone(),
        }
    }

    /// Picture subtitles are drawn here unless the server can't make them.
    fn sideload(&self) -> bool {
        !self.pictures_unavailable && self.caps.as_ref().is_some_and(|c| c.picture_subtitles_sideload)
    }

    /// Asks for a session at `at_ms` and has the engine play it. `alongside`:
    /// the session playing plays on meanwhile (`keep_previous`), and the
    /// engine moves once the new stream's first segment is made (20 s at most).
    fn begin(&mut self, at_ms: i64, g: u64, alongside: bool, restart: bool, flush: bool) {
        let Some(detail_id) = self.item.as_ref().map(|d| d.id) else { return };
        let Some(chosen) = self.file.clone() else {
            self.fail(details::no_file(), false, None);
            return;
        };
        let mut flush = flush;
        if alongside {
            self.preparing = Some(g);
        } else {
            // The session before ends first; the new one waits up to 2 s for that to be said.
            flush |= self.end_session(at_ms);
            self.reset(at_ms, false);
        }
        let playing = self.playing_preset();
        let original = playing.id == ORIGINAL_ID;
        // A file on a drive has no server to remux it: it plays as it is, and
        // the screen gets what Playback details says.
        if original && self.guard_mode.is_none() && !self.local() {
            self.guard_mode = dv_container_mode(Some(&chosen), self.caps.as_ref(), self.dv_containers.as_deref());
        }
        let mode = if original {
            self.recovery.fallback_mode.or(self.guard_mode).unwrap_or(playing.mode())
        } else {
            playing.mode()
        };
        let decision = self.caps.as_ref().map(|c| decide(&chosen, c));
        // Automatic audio is chosen again for each session: a stream (a lower
        // quality, say) has the track it copies rather than converting the one
        // the file played as it is.
        let audio = if self.audio_picked { self.audio.clone() } else { None };
        let streaming = will_stream(mode, decision.as_ref(), Some(&chosen), audio.as_ref(), self.caps.as_ref());
        let choices = Choices {
            file: chosen,
            start_ms: at_ms,
            audio: audio.clone(),
            subtitle: self.subtitle.clone(),
            quality: playing,
            mode,
            streaming,
            sideload: self.sideload(),
        };
        let asked = Asked { at_ms, alongside, choices, original, audio, restart };
        self.open(detail_id, g, asked, flush);
    }

    /// The task that opens a session: the request, then the Dolby Vision guard
    /// and the burn-in indexes, asking again where they say. Its requests
    /// aren't cancelled with the player: a session the server made is always
    /// let go when nobody takes it.
    fn open(&mut self, item_id: i64, g: u64, asked: Asked, flush: bool) {
        let server = self.env.server.clone();
        let reporter = self.env.reporter.clone();
        let generation = self.generation.clone();
        let (client_id, keyed) = (self.env.client_id.clone(), self.env.keyed_urls);
        let caps = self.caps.clone();
        let local = self.local();
        let has_next = self.item.as_ref().is_some_and(|d| has_next(d.kind, self.current.queue));
        let file = Some(&asked.choices.file);
        let (audio, subtitles) = (!audio_streams(file).is_empty(), !subtitle_streams(file).is_empty());
        let controls = video_controls_with_tracks(has_next, audio, subtitles, !local);
        let mut asked = asked;
        self.spawn(move || {
            Cancel::new().scope(|| {
                if flush {
                    reporter.flush(Duration::from_millis(END_WAIT_MS));
                }
                let ask = |c: &Choices| {
                    let mut req = playback_request(item_id, c, &client_id, caps.as_ref(), keyed);
                    req.keep_previous = asked.alongside;
                    req.controls = controls.clone();
                    req
                };
                let stale = || generation.load(Ordering::SeqCst) != g;
                let discard = |info: &PlaybackInfo| {
                    let (server, session) = (server.clone(), info.session_id.clone());
                    reporter.send(move || {
                        let _ = server.stop_session(&session);
                    });
                };
                let result = (|| {
                    let mut sent = ask(&asked.choices);
                    let mut info = match server.play(&sent) {
                        Ok(info) => info,
                        Err(e) if asked.alongside && e.is(503) => return Err(Opening::Busy),
                        Err(e) => return Err(Opening::Failed(e)),
                    };
                    if stale() {
                        discard(&info);
                        return Err(Opening::Stale);
                    }
                    // Played as it is, a drive's profile 7 shows its HDR10 base layer, where
                    // asking again would only be refused (Samsung's bc8dbab1).
                    let guard =
                        if local { None } else { dv_retry_mode(&info, Some(&asked.choices.file), caps.as_ref()) };
                    if let Some(mode) = guard {
                        discard(&info);
                        asked.choices.mode = mode;
                        asked.choices.streaming = true;
                        sent = ask(&asked.choices);
                        info = server.play(&sent).map_err(Opening::Failed)?;
                    }
                    if needs_indexes(&info, &sent, &asked.choices) {
                        discard(&info);
                        asked.choices.streaming = true;
                        sent = ask(&asked.choices);
                        info = server.play(&sent).map_err(Opening::Failed)?;
                    }
                    if stale() {
                        discard(&info);
                        return Err(Opening::Stale);
                    }
                    Ok(Opened { info, guard })
                })();
                Done::Session { g, asked: Box::new(asked), result }
            })
        });
    }

    fn on_session(&mut self, g: u64, asked: Asked, result: Result<Opened, Opening>) {
        let current = g == self.current_generation() && !self.closed;
        let opened = match result {
            Ok(o) => o,
            Err(Opening::Stale) => return,
            Err(Opening::Busy) if current => {
                // No transcode free for two at once: the one playing ends first, as before.
                let at = self.live_position();
                let flush = self.end_session(at);
                self.reset(at, false);
                self.preparing = None;
                let item_id = self.item.as_ref().map_or(self.current.item_id, |d| d.id);
                let mut asked = asked;
                asked.alongside = false;
                asked.at_ms = at;
                asked.choices.start_ms = at;
                self.open(item_id, g, asked, flush);
                return;
            }
            Err(Opening::Busy) => return,
            Err(Opening::Failed(e)) => {
                if !current {
                    return;
                }
                if self.preparing == Some(g) {
                    self.preparing = None;
                }
                // Mid-playback, a server or transcoder that doesn't answer is waited for: 90 s in
                // all, however often the server answers in between (`away`, the wait this start
                // ended).
                let away = self.away.take().filter(|a| a.g == g);
                let spent = away.is_some_and(|a| self.now - a.since >= OUTAGE_MS);
                if asked.restart && self.shown_once && e.out_of_reach() && !spent {
                    self.last_position_ms = if asked.alongside { self.live_position() } else { asked.at_ms };
                    self.reach_lost_after(true, away.map(|a| (a.since, a.attempts)));
                } else {
                    self.fail_with(&e);
                }
                return;
            }
        };
        if !current {
            // Closed or replaced while the server made it: it never played.
            self.discard(&opened.info);
            return;
        }
        if let Some(mode) = opened.guard {
            self.guard_mode = Some(mode);
        }
        let info = opened.info;
        // A file on a drive plays at its own address.
        if self.env.keyed_urls && !self.local() && !is_keyed(&info.url) {
            self.discard(&info);
            self.preparing = None;
            self.fail(details::too_old(), false, None);
            return;
        }
        if asked.alongside {
            let deadline = self.now + PREPARE_MAX_MS;
            let promise = if info.hls {
                let server = self.env.server.clone();
                let url = self.env.server.url(&info.url);
                let start = asked.at_ms + PREPARE_LEAD_MS;
                self.env.tasks.spawn(move || warm_stream(server.as_ref(), &url, start))
            } else {
                Promise::resolved(())
            };
            self.warming = Some(Warming { g, info, asked, deadline, promise });
            if self.warming.as_ref().is_some_and(|w| w.promise.ready()) {
                let w = self.warming.take().expect("checked");
                self.warmed(w);
            }
            return;
        }
        self.take_session(g, info, asked);
    }

    /// The stream made ready beside the one playing is there (or 20 s went by).
    fn warmed(&mut self, w: Warming) {
        w.promise.cancel();
        // Replaced or closed meanwhile, or the last one ended or failed by itself: this one never plays.
        if w.g != self.current_generation() || self.closed || self.ended || self.in_error() {
            self.discard(&w.info);
            if self.preparing == Some(w.g) {
                self.preparing = None;
            }
            return;
        }
        // From where the last one got to while this one got ready. The server answered, whatever the last one found.
        let at = self.live_position();
        self.end_outage();
        self.end_session(at);
        self.reset(at, true);
        let mut asked = w.asked;
        asked.at_ms = at;
        self.take_session(w.g, w.info, asked);
    }

    /// Plays the session `info`: the engine opens it at `asked.at_ms`.
    fn take_session(&mut self, g: u64, info: PlaybackInfo, asked: Asked) {
        if self.preparing == Some(g) {
            self.preparing = None;
        }
        let Some(chosen) = self.file.clone() else { return };
        // No track picked: the one the server chose for this TV plays, and the menu says so.
        self.audio = asked.audio.clone().or_else(|| playing_audio(Some(&chosen), &info));
        self.tracks_now.audio = self.audio.as_ref().map(|a| a.index);
        self.session_ended = false;
        self.live = None;
        self.info = Some(info.clone());
        self.original_only = info.original_only;
        if self.original_only {
            // Whatever was asked for, the original plays: the menu says so,
            // and the quality kept stays for servers that convert.
            self.quality = Quality::original();
            self.auto = None;
        }
        if self.auto.is_some() {
            let live = self.auto_began(&info, asked.original);
            // A new stream of the same quality (a recovery, a track) goes on at the bitrate the last one had come to.
            if live.is_some()
                && let Some(kbps) = self.auto.as_ref().filter(|a| a.live_lowered()).and_then(|a| a.live_kbps)
            {
                self.send_bitrate(kbps);
            }
        }
        self.state.method = Some(details::method_text(&info).into());
        self.state.note = self.note_text();
        self.state.can_play_original = self.recovery.struggle_switched && self.recovery.fallback_mode.is_some();
        if info.duration_ms > 0 {
            self.clock.duration_ms = info.duration_ms;
        }
        self.changed();
        self.publish_choices();
        self.arm_progress();
        if std::mem::take(&mut self.report_begun) {
            let state = if self.want_play { ProgressState::Playing } else { ProgressState::Paused };
            self.report_at(state, asked.at_ms);
        }
        self.refresh_details(false);
        let video = video_stream_of(Some(&chosen));
        let transcode = info.method == PlaybackMethod::Transcode;
        let detail_duration = self.item.as_ref().and_then(|d| d.duration_ms);
        let req = LoadRequest {
            url: self.env.server.url(&info.url),
            kind: if info.hls { MediaKind::Hls } else { MediaKind::File },
            start_ms: asked.at_ms.max(0) as u64,
            container: if info.hls {
                None
            } else {
                Some(mime_type_of(chosen.container.as_deref()).unwrap_or("video/mp4").to_string())
            },
            video: video.map(|v| VideoFacts {
                codec: if transcode { "unknown".into() } else { v.codec.clone() },
                width: v.width.unwrap_or(0).max(0) as u32,
                height: v.height.unwrap_or(0).max(0) as u32,
                frame_rate: v.frame_rate,
                // A conversion's dynamic range is the server's to choose.
                dynamic_range: if transcode { DynamicRange::Sdr } else { v.dynamic_range.unwrap_or(DynamicRange::Sdr) },
                dv_profile: if transcode { None } else { v.dv_profile },
                bit_depth: Some(v.bit_depth.unwrap_or(8).max(0) as u32),
            }),
            audio: if info.audio_converted {
                None
            } else {
                self.audio
                    .as_ref()
                    .map(|a| AudioFacts { codec: a.codec.clone(), channels: a.channels.map(|c| c.max(0) as u32) })
            },
            duration_hint_ms: if info.duration_ms > 0 {
                Some(info.duration_ms as u64)
            } else {
                chosen.duration_ms.or(detail_duration).map(|d| d.max(0) as u64)
            },
            autoplay: self.want_play,
        };
        self.view = EngineView { status: EngineStatus::Buffering, ..EngineView::default() };
        self.engine.load(req);
        let speed = self.state.speed;
        if speed != 1.0 {
            self.engine.set_rate(speed);
        }
        self.apply_tracks();
    }

    /// Reports `stopped` at `position_ms` and deletes the session playing; false when none plays.
    fn end_session(&mut self, position_ms: i64) -> bool {
        let Some(info) = self.info.as_ref().filter(|_| !self.session_ended) else { return false };
        self.session_ended = true;
        self.progress_at = None;
        let item_id = self.item.as_ref().map_or(self.current.item_id, |d| d.id);
        let report = progress(position_ms, info, ProgressState::Stopped, self.tracks_now, self.quality.id);
        let (server, session) = (self.env.server.clone(), info.session_id.clone());
        self.env.reporter.send(move || {
            let _ = server.progress(item_id, &report);
            let _ = server.stop_session(&session);
        });
        true
    }

    /// What is watched of a session starts over, for one at `at_ms`; `switching` to one made ready beside the last.
    fn reset(&mut self, at_ms: i64, switching: bool) {
        self.last_position_ms = at_ms;
        self.started_at_ms = at_ms;
        self.media_shown = false;
        self.ended = false;
        self.struggle.reset();
        self.recovery.on_jump();
        self.last_advance_at = self.now;
        self.last_advance_pos = -1;
        self.session_started_at = self.now;
        self.phase_force(Phase::Preparing);
        self.state.skip_label = None;
        self.state.first_frame_ms = None;
        self.state.switching = switching;
        self.clock.position_ms = at_ms;
        self.changed();
    }

    /// Deletes a session that never played: no `stopped`, which would move the resume point.
    fn discard(&self, info: &PlaybackInfo) {
        let (server, session) = (self.env.server.clone(), info.session_id.clone());
        self.env.reporter.send(move || {
            let _ = server.stop_session(&session);
        });
    }

    /// Audio in place for direct play; subtitles where they're drawn: the
    /// server's WebVTT for text, its `.mks` for pictures, nothing when burned in.
    fn apply_tracks(&mut self) {
        let Some(info) = self.info.clone() else { return };
        self.audio_pending = None;
        if !info.hls
            && let Some(audio) = self.audio.clone()
        {
            let ordinal = audio_streams(self.file.as_ref()).iter().position(|s| s.index == audio.index);
            if let Some(ordinal) = ordinal {
                // LG's pipeline lists its tracks once the file is open: the
                // track is switched to then.
                self.audio_pending = Some((ordinal, audio.language.clone()));
                self.apply_pending_audio();
            }
        }
        let drawn = match self.subtitle.clone() {
            None => {
                self.presenter.hide();
                None
            }
            Some(sub) if !is_picture(Some(&sub)) => {
                let path = info
                    .subtitles
                    .iter()
                    .find(|t| t.index == sub.index)
                    .and_then(|t| t.url.clone())
                    .unwrap_or_else(|| text_path(info.file_id, sub.index));
                self.presenter.show_text(&path);
                Some(sub.index)
            }
            Some(sub) if !info.burn_subtitles && self.sideload() => {
                self.presenter.show_pictures(&picture_path(info.file_id, sub.index));
                Some(sub.index)
            }
            Some(_) => {
                self.presenter.hide();
                None
            }
        };
        // What shows: drawn here, else burned into the picture by the stream, else none.
        let burned = info.subtitle_index.filter(|_| info.burn_subtitles);
        self.tracks_now.subtitle = drawn.or(burned).unwrap_or(-1);
        self.changed();
    }

    /// Switches to the pending audio track once the engine has listed its tracks.
    fn apply_pending_audio(&mut self) {
        let Some((ordinal, language)) = self.audio_pending.clone() else { return };
        let Some(tracks) = self.view.tracks.clone() else { return };
        self.audio_pending = None;
        let default = self.audio.as_ref().is_none_or(|a| a.default);
        if let Some(target) = self.track_for(&tracks, ordinal, language.as_deref())
            && self.engine.select_audio(target)
            && !default
        {
            // Another track than the one the file starts with: the switch may drop frames.
            self.quiet_frames();
        }
    }

    /// Switches the file's `ordinal`-th audio track in place; false when the engine can't.
    fn switch_audio(&mut self, ordinal: usize, language: Option<&str>) -> bool {
        match self.view.tracks.clone() {
            Some(tracks) => match self.track_for(&tracks, ordinal, language) {
                Some(target) => self.engine.select_audio(target),
                None => false,
            },
            // The engine hasn't listed its tracks: by the file's order.
            None => self.engine.select_audio(ordinal),
        }
    }

    /// The engine's track for the file's `ordinal`-th audio track ([`match_track`]). A file on a
    /// drive is read in the player's own order and its languages may be written another way
    /// ("jpn" and "ja"): where they don't agree, its track goes by the file's order.
    fn track_for(&self, tracks: &[AudioTrack], ordinal: usize, language: Option<&str>) -> Option<usize> {
        let matched = match_track(tracks, ordinal, language);
        if self.local() { matched.or_else(|| tracks.get(ordinal).map(|t| t.ordinal)) } else { matched }
    }

    /// An audio track switched in place: the frames it drops aren't counted, and counting starts afresh after.
    fn quiet_frames(&mut self) {
        self.frames_quiet_until = self.now + TRACK_SWITCH_QUIET_MS;
        self.struggle.reset();
    }

    /// The server can't make the `.mks` (an older server or transcoder): picture subtitles are burned in from now on.
    fn pictures_failed(&mut self) {
        if self.closed || self.stopped {
            return;
        }
        // A file on a drive has no server to burn them in.
        if self.local() {
            self.notice(details::subtitles_failed());
            return;
        }
        self.pictures_unavailable = true;
        self.publish_choices();
        if is_picture(self.subtitle.as_ref()) && self.info.as_ref().is_some_and(|i| !i.burn_subtitles) {
            let at = self.live_position();
            self.restart(at, true);
        }
    }

    // ---------------------------------------------------------------- engine events

    fn on_engine(&mut self, event: EngineEvent) {
        match event {
            EngineEvent::Loaded { duration_ms } => {
                self.view.duration_ms = duration_ms.filter(|d| *d > 0);
            }
            EngineEvent::AudioTracks(tracks) => {
                self.view.tracks = Some(tracks);
                self.apply_pending_audio();
            }
            EngineEvent::VideoInfo { .. } => {}
            EngineEvent::FirstPicture => self.on_first_frame(),
            EngineEvent::Playing => {
                self.view.playing = true;
                self.set_status(EngineStatus::Ready);
            }
            EngineEvent::Paused => {
                self.view.playing = false;
                if self.view.status != EngineStatus::Ended {
                    self.set_status(EngineStatus::Ready);
                }
            }
            EngineEvent::Waiting(true) => self.set_status(EngineStatus::Buffering),
            EngineEvent::Waiting(false) => {
                if self.view.status == EngineStatus::Buffering {
                    self.set_status(EngineStatus::Ready);
                }
            }
            EngineEvent::Buffered { ahead_ms } => {
                self.view.buffered_end = Some(self.engine.position_ms() as i64 + ahead_ms as i64);
            }
            EngineEvent::Ended => {
                self.view.playing = false;
                self.set_status(EngineStatus::Ended);
            }
            EngineEvent::Error { code, message, .. } => {
                let hls = self.info.as_ref().is_some_and(|i| i.hls);
                self.on_error(engine_error(code, &message, hls));
            }
        }
    }

    fn set_status(&mut self, status: EngineStatus) {
        let repeat = self.view.status == status;
        self.view.status = status;
        if self.info.is_none() || self.closed {
            return;
        }
        match status {
            EngineStatus::Buffering => {
                if self.ended || repeat {
                    return;
                }
                // A stall: it played, and no seek of its own asked it to wait.
                if self.state.phase == Phase::Playing && self.now - self.seeked_at > SEEK_SETTLE_MS {
                    self.state.stalls += 1;
                    self.changed();
                    if let Some(a) = &mut self.auto {
                        a.waiting(self.now);
                    }
                }
                self.phase(Phase::Buffering);
            }
            EngineStatus::Ready => {
                self.media_shown = true;
                self.shown_once = true;
                if self.state.switching {
                    self.state.switching = false;
                    self.changed();
                }
                if let Some(a) = &mut self.auto {
                    a.resumed(self.now);
                }
                if self.outage.is_none() {
                    self.phase(if self.view.playing { Phase::Playing } else { Phase::Paused });
                }
            }
            EngineStatus::Ended => {
                if repeat {
                    return;
                }
                if self.ended_short() {
                    self.on_ended_short();
                } else {
                    self.on_ended();
                }
            }
            EngineStatus::Idle => {}
        }
    }

    fn on_first_frame(&mut self) {
        if self.closed || self.info.is_none() {
            return;
        }
        self.media_shown = true;
        self.shown_once = true;
        self.state.switching = false;
        if self.state.first_frame_ms.is_none() {
            // From the press for the first session; from the session's start after that.
            let from = self.pressed_at.take().unwrap_or(self.session_started_at);
            self.state.first_frame_ms = Some(self.now - from);
        }
        self.changed();
    }

    fn on_error(&mut self, error: EngineError) {
        let Some(info) = self.info.clone() else { return };
        if self.closed || self.stopped {
            return;
        }
        if let EngineError::Stopped { message } = error {
            self.stop_by_server(message.as_deref(), true);
            return;
        }
        // The TV's player can't say why a request failed: the session's own answer tells a stop
        // from the rest. A file on a drive has no session to ask about.
        if !self.local()
            && matches!(error, EngineError::Http { .. } | EngineError::Network { .. } | EngineError::Playlist { .. })
        {
            if let Some(o) = &mut self.outage {
                // Reconnecting already: the engine gave up too, so a new session starts once the server answers.
                o.engine_failed = true;
                return;
            }
            self.ask_session(&info.session_id, Check::Error(error));
            return;
        }
        self.recover(&error);
    }

    fn ask_session(&mut self, session: &str, check: Check) {
        let server = self.env.server.clone();
        let session = session.to_string();
        self.spawn(move || {
            let result = server.session_status(&session, Duration::from_millis(OUTAGE_ASK_MS));
            Done::Check { session, check, result }
        });
    }

    fn on_check(&mut self, session: String, check: Check, result: Result<SessionStatus, ApiError>) {
        if self.info.as_ref().is_none_or(|i| i.session_id != session) || self.closed || self.stopped {
            return;
        }
        match check {
            Check::Error(error) => match result {
                Ok(_) => self.recover(&error),
                Err(ApiError::SessionStopped { message }) => self.stop_by_server(message.as_deref(), true),
                Err(e) if e.out_of_reach() => self.reach_lost(true),
                Err(e) if e.is(404) => self.recover(&EngineError::Http { status: 404, detail: e.to_string() }),
                Err(_) => self.recover(&error),
            },
            Check::Stall { starving } => {
                if self.outage.is_some() {
                    return;
                }
                let method = self.info.as_ref().map_or(PlaybackMethod::Direct, |i| i.method);
                match result {
                    Err(e) if e.out_of_reach() => self.reach_lost(false),
                    Err(ApiError::SessionStopped { message }) => self.stop_by_server(message.as_deref(), true),
                    _ => {
                        let action = self.recovery.on_stall(method, starving);
                        self.act(action, starving);
                    }
                }
            }
            Check::Server { starving } => match result {
                Ok(status) => {
                    let speed = status.transcode.and_then(|t| t.speed);
                    let action = self.recovery.after_server_check(speed, starving);
                    self.act(action, starving);
                }
                // A stall with the server gone isn't the video's fault: wait for the server rather than convert.
                Err(e) if e.out_of_reach() => self.reach_lost(false),
                Err(ApiError::SessionStopped { message }) => self.stop_by_server(message.as_deref(), true),
                Err(_) => {
                    let action = self.recovery.after_server_check(None, starving);
                    self.act(action, starving);
                }
            },
        }
    }

    fn recover(&mut self, error: &EngineError) {
        let Some(info) = &self.info else { return };
        if self.closed || self.stopped {
            return;
        }
        let codec = video_stream_of(self.file.as_ref()).map(|v| v.codec.clone());
        let action = self.recovery.decide(error, info.method, codec.as_deref());
        self.act(action, false);
    }

    fn act(&mut self, action: RecoveryAction, starving: bool) {
        // A file on a drive that still can't be read (the drive pulled out) says so, not that a
        // server is away (`SessionPlaybackController.kt`'s `act`).
        if self.local() && matches!(action, RecoveryAction::Fail { failure: Failure::Network | Failure::Stopped, .. }) {
            self.fail(details::drive_unreadable(), true, None);
            return;
        }
        match action {
            RecoveryAction::NewSession { reason, .. } => {
                // The player's own fallback is a change too: Automatic waits before it changes again.
                if reason.is_some()
                    && let Some(a) = &mut self.auto
                {
                    a.fell_back(self.now);
                }
                if reason == Some(FallbackReason::NotSmooth) && self.recovery.struggle_switched {
                    self.state.can_play_original = true;
                    self.changed();
                }
                let at = self.live_position();
                self.restart(at, false);
            }
            RecoveryAction::CheckServer if self.local() => {
                let action = self.recovery.after_server_check(None, starving);
                self.act(action, starving);
            }
            RecoveryAction::CheckServer => {
                if let Some(session) = self.info.as_ref().map(|i| i.session_id.clone()) {
                    self.ask_session(&session, Check::Server { starving });
                }
            }
            // Not where there is none to choose.
            RecoveryAction::SuggestLowerQuality if self.original_only => {}
            RecoveryAction::SuggestLowerQuality => {
                self.state.suggest_lower_quality = true;
                self.state.note = Some(details::slow_server().into());
                self.changed();
            }
            RecoveryAction::Fail { failure, detail } => {
                let title = (failure == Failure::Network).then(|| details::offline_title().to_string());
                self.fail(&failure_text(failure, detail.as_deref()), failure != Failure::Unplayable, title);
            }
        }
    }

    /// The media ended moments after where it started, a minute or more before its end: not the end.
    fn ended_short(&self) -> bool {
        if self.ended {
            return false;
        }
        let expected = match self.info.as_ref().map(|i| i.duration_ms).filter(|d| *d > 0) {
            Some(d) => d,
            None => self.file.as_ref().and_then(|f| f.duration_ms).unwrap_or(0),
        };
        if expected <= 0 {
            return false;
        }
        let reached = self.engine.position_ms() as i64;
        reached + SHORT_END_MS < expected && reached < self.started_at_ms + SHORT_PLAY_MS
    }

    fn on_ended_short(&mut self) {
        let Some(method) = self.info.as_ref().map(|i| i.method) else { return };
        self.media_shown = false;
        self.last_position_ms = self.started_at_ms;
        self.clock.position_ms = self.started_at_ms;
        self.state.skip_label = None;
        self.changed();
        let action = self.recovery.on_ended_short(method);
        self.act(action, false);
    }

    fn on_ended(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        let key = self.item.as_ref().map_or(-1, |d| d.id);
        let sleeping = self.sleep.item_ended(key);
        self.sleep_changed();
        if sleeping {
            self.on_sleep();
        }
        let engine_duration = self.view.duration_ms.map_or(0, |d| d as i64);
        let duration = match self.info.as_ref().map(|i| i.duration_ms).filter(|d| *d > 0) {
            Some(d) => d,
            None if engine_duration > 0 => engine_duration,
            None => self.last_position_ms,
        };
        self.last_position_ms = duration;
        self.end_session(duration);
        self.clock.position_ms = duration;
        self.state.skip_label = None;
        self.changed();
        let has = self.item.as_ref().is_some_and(|d| has_next(d.kind, self.current.queue));
        if !has {
            self.phase(Phase::Ended);
            return;
        }
        self.phase(Phase::Paused);
        if let Some(known) = self.next.clone() {
            let restart = !sleeping && self.countdown.is_none();
            self.show_up_next(known, restart);
            return;
        }
        self.look_up_next(NextFor::Ended { sleeping });
    }

    // ---------------------------------------------------------------- the 500 ms check

    fn check(&mut self) {
        if self.info.is_none() || self.closed || self.session_ended {
            return;
        }
        let now = self.now;
        let status = self.view.status;
        let moving = self.media_shown && status == EngineStatus::Ready && self.view.playing && !self.ended;
        if moving && let Some(at) = self.moving_at {
            self.title_played_ms += (now - at).min(2 * TICK_MS);
        }
        self.moving_at = moving.then_some(now);
        if self.media_shown && !self.ended {
            let pos = self.engine.position_ms() as i64;
            self.last_position_ms = pos;
            if moving {
                self.recovery.on_position(pos);
            }
            let marker = if self.in_error() { None } else { active_marker(&self.markers, pos).cloned() };
            let info_duration = self.info.as_ref().map_or(0, |i| i.duration_ms);
            let engine_duration = self.view.duration_ms.map_or(0, |d| d as i64);
            let clock = PlayerClock {
                position_ms: pos,
                buffered_ms: self.view.buffered_end.unwrap_or(pos),
                duration_ms: if info_duration > 0 {
                    info_duration
                } else if engine_duration > 0 {
                    engine_duration
                } else {
                    self.clock.duration_ms
                },
            };
            if clock != self.clock {
                self.clock = clock;
                self.changed();
            }
            let label = marker.as_ref().map(|m| skip_label(m).to_string());
            if label != self.state.skip_label {
                self.state.skip_label = label;
                self.changed();
            }
            if let Some(m) = marker
                && self.credits.enter(&m)
            {
                self.on_credits();
            }
        }
        // Reconnecting: the stall ladder and the frame counts wait; the engine moving on by itself ends it.
        if let Some(o) = &self.outage {
            if moving && self.engine.position_ms() as i64 > o.at_ms + OUTAGE_MOVED_MS {
                self.end_outage();
                self.phase(Phase::Playing);
            }
            return;
        }
        // A session made ready beside this one: nothing more is decided about the stream it leaves.
        if self.preparing.is_some() {
            self.last_advance_at = now;
            return;
        }
        if self.watch_quality(moving) {
            return;
        }
        // A stall: meant to play, not moving.
        let stalling = self.want_play && !self.ended && !self.in_error() && status != EngineStatus::Idle;
        let pos = if self.media_shown { self.engine.position_ms() as i64 } else { -1 };
        if !stalling || pos != self.last_advance_pos {
            self.last_advance_at = now;
            self.last_advance_pos = pos;
        } else if now - self.last_advance_at >= STALL_MS {
            self.last_advance_at = now;
            // A file on a drive waits for no server, and the same file again would stand still
            // again: one whose picture never came isn't one this TV plays; one that played, here
            // or before the file was started again, can't be read any more (the drive pulled out).
            if self.local() {
                let why = if self.shown_once { details::drive_unreadable() } else { details::local_stuck() };
                self.fail(why, true, None);
                return;
            }
            let ahead = self.view.buffered_end.map_or(0, |e| e - self.engine.position_ms() as i64);
            let starving = status == EngineStatus::Buffering && ahead < STARVING_MS;
            if let Some(session) = self.info.as_ref().map(|i| i.session_id.clone()) {
                self.ask_session(&session, Check::Stall { starving });
            }
            return;
        }
        // Dropped frames, sampled every 2 s of playback.
        if moving && now >= self.frames_quiet_until && now - self.last_struggle_sample >= STRUGGLE_SAMPLE_MS {
            self.last_struggle_sample = now;
            let stats = self.engine.stats();
            if let (Some(dropped), Some(total)) = (stats.dropped_frames, stats.total_frames)
                && self.struggle.sample(now, dropped, total)
            {
                self.struggled();
            }
        }
    }

    /// The decoder drops frames: Automatic's to lower, or the original's one fallback.
    fn struggled(&mut self) {
        self.struggle.reset();
        let method = self.info.as_ref().map_or(PlaybackMethod::Direct, |i| i.method);
        let patient = self.auto.as_ref().is_some_and(|a| a.patient());
        if patient && self.title_played_ms >= FIRST_MINUTE_MS {
            // Automatic on a stream that can't change as it plays: dropped frames move it in the
            // title's first minute only, a rough guess that this TV can't keep up with the file;
            // later the picture plays on, and the next title starts lower (docs/QUALITY.md).
            if let Some(a) = &mut self.auto {
                a.struggled();
            }
            return;
        }
        // The original falls back to a conversion once, as ever; a conversion that drops frames is Automatic's to lower.
        if let Some(action) = self.recovery.on_struggling(method) {
            self.act(action, false);
        } else if let Some(a) = &mut self.auto {
            a.decoding();
            let step = a.check(self.now);
            self.apply_auto(step);
        }
    }

    // ---------------------------------------------------------------- reports

    /// Progress every 10 s while playing and every 30 s paused, so the server keeps the session and its transcode.
    fn arm_progress(&mut self) {
        self.progress_at = None;
        if self.info.is_none() || self.session_ended {
            return;
        }
        self.progress_at = Some(self.now + if self.want_play { PROGRESS_MS } else { PAUSED_HEARTBEAT_MS });
    }

    fn report(&mut self, state: ProgressState) {
        self.report_at(state, self.live_position());
    }

    /// `position`: where the player went, when the TV's player may not say so yet (just after a seek).
    fn report_at(&mut self, state: ProgressState, position: i64) {
        let Some(info) = self.info.as_ref().filter(|_| !self.session_ended) else { return };
        let item_id = self.item.as_ref().map_or(self.current.item_id, |d| d.id);
        let report = progress(position, info, state, self.tracks_now, self.quality.id);
        let server = self.env.server.clone();
        self.env.reporter.send(move || {
            let _ = server.progress(item_id, &report);
        });
    }

    fn sleep_item(&self) -> SleepItem {
        let left = if self.info.is_none() || !self.media_shown || self.ended || self.clock.duration_ms <= 0 {
            None
        } else {
            Some(self.clock.duration_ms - self.live_position())
        };
        SleepItem { key: self.item.as_ref().map_or(-1, |d| d.id), left_ms: left }
    }

    fn sleep_changed(&mut self) {
        if self.state.sleep != self.sleep.state {
            self.state.sleep = self.sleep.state;
            self.changed();
        }
    }

    /// The sleep timer ended: it pauses, and what plays next doesn't start by itself.
    fn on_sleep(&mut self) {
        self.countdown = None;
        if let Some(card) = &mut self.state.up_next {
            card.seconds_left = None;
        }
        self.engine.pause();
        self.set_want_play(false);
        self.changed();
    }

    /// The server ended the session: the engine stops, nothing more is asked of
    /// the server (only `stopped`, for the resume point), and the screen says why.
    fn stop_by_server(&mut self, message: Option<&str>, by_admin: bool) {
        if self.closed || self.stopped {
            return;
        }
        self.stopped = true;
        self.next_generation();
        let position = self.live_position();
        self.last_position_ms = position;
        if let Some(info) = self.info.as_ref().filter(|_| !self.session_ended) {
            self.session_ended = true;
            self.progress_at = None;
            let item_id = self.item.as_ref().map_or(self.current.item_id, |d| d.id);
            let report = progress(position, info, ProgressState::Stopped, self.tracks_now, self.quality.id);
            let server = self.env.server.clone();
            self.env.reporter.send(move || {
                let _ = server.progress(item_id, &report);
            });
        }
        self.seek_report_at = None;
        self.next_lookup += 1;
        self.hide_up_next();
        self.presenter.hide();
        self.sleep.cancel();
        self.sleep_changed();
        self.engine.unload();
        self.state.phase =
            Phase::Error { message: details::stopped_text(message, by_admin), can_retry: false, title: None };
        self.state.skip_label = None;
        self.state.stopped_by_server = true;
        self.changed();
    }

    // ---------------------------------------------------------------- commands

    /// What the server says happened elsewhere: a stop of this session, or a command of the
    /// user's remote on another device.
    pub fn server_event(&mut self, event: &ServerEvent) {
        if let ServerEvent::SessionStopped { session_id, message, by_admin } = event
            && self.info.as_ref().is_some_and(|i| i.session_id == *session_id)
        {
            self.stop_by_server(message.as_deref(), *by_admin);
        } else if let Some(command) = Command::of(event) {
            self.obey(&command);
        }
    }

    /// A command of the user's remote, on another device, for the session playing
    /// ([`crate::remote`] has the rule): what changed is reported at once, so the remote sees it.
    /// A pause reports as any pause does, the next item with its own session; a play or pause
    /// that finds it so already says it again. A remote's next that finds nothing says nothing
    /// here, whoever pressed it being elsewhere. What changed is said in a remote notice; what
    /// changed nothing says nothing, and the next title shows as it begins, as it does by itself.
    fn obey(&mut self, command: &Command) {
        if self.closed || self.stopped || self.session_ended || self.in_error() {
            return;
        }
        let Some(info) = self.info.as_ref() else { return };
        let duration = self.clock.duration_ms;
        let player = RemotePlayer {
            kind: RemoteKind::Video,
            session: Some(&info.session_id),
            playing: self.want_play && !self.ended,
            position_ms: self.live_position(),
            duration_ms: (duration > 0).then_some(duration),
            has_next: self.item.as_ref().is_some_and(|d| has_next(d.kind, self.current.queue)),
        };
        match remote_action(&player, command) {
            RemoteAction::Play => {
                self.play();
                self.report(ProgressState::Playing);
                self.remote_notice(remote::playing().into());
            }
            RemoteAction::Pause => {
                self.pause();
                // Nobody here paused it: the screen doesn't keep its controls for this pause.
                self.state.paused_by_remote = true;
                self.remote_notice(remote::paused().into());
            }
            // Playing or paused already: said again, so a remote a little behind settles.
            RemoteAction::Report => {
                self.report(if self.want_play { ProgressState::Playing } else { ProgressState::Paused })
            }
            RemoteAction::Seek(at) => {
                self.seek_to(at);
                // Not a second later, as after the screen's seeks; a seek that started a new
                // session has that report itself.
                if self.seek_report_at.take().is_some() {
                    let state = if self.want_play { ProgressState::Playing } else { ProgressState::Paused };
                    self.report_at(state, self.last_position_ms);
                }
                // A skip says how far it was asked to go, a seek where to.
                let said = match command.offset_ms.filter(|_| command.command == Control::Skip) {
                    Some(by) => remote::skipped(by),
                    None => remote::jumped(at),
                };
                self.remote_notice(said);
            }
            RemoteAction::Next => match self.next.clone() {
                Some(known) => self.switch_to(known),
                None => self.look_up_next(NextFor::Remote),
            },
            RemoteAction::Audio(index) => {
                let before = self.audio.as_ref().map(|a| a.index);
                self.pick_track(|c| c.select_audio(index));
                if self.audio.as_ref().map(|a| a.index) != before
                    && let Some(label) = menu_label(&self.state.audio, Some(index))
                {
                    self.remote_notice(remote::audio_picked(&label));
                }
            }
            RemoteAction::Subtitles(index) => {
                let before = self.subtitle.as_ref().map(|s| s.index);
                self.pick_track(|c| c.select_subtitle(index));
                if self.subtitle.as_ref().map(|s| s.index) != before
                    && let Some(label) = menu_label(&self.state.subtitles, index)
                {
                    // The menu's Off row says "Subtitles off".
                    let shown = index.map(|_| label.as_str());
                    self.remote_notice(remote::subtitles_picked(shown));
                }
            }
            RemoteAction::Quality(id) => {
                let before = self.quality.id;
                self.pick_quality(id);
                if self.quality.id != before
                    && let Some(label) = self.state.qualities.iter().find(|q| q.id == id).map(|q| q.label.clone())
                {
                    self.remote_notice(remote::quality_picked(&label));
                }
            }
            RemoteAction::None | RemoteAction::Previous | RemoteAction::Channel(_) => {}
        }
    }

    /// What the remote did, for the screen to say briefly.
    fn remote_notice(&mut self, text: String) {
        self.state.remote_notice = Some(text);
        self.state.remote_notice_id += 1;
        self.changed();
    }

    /// A track the remote picked, through the menu's own path. What this session plays then (a
    /// track switched in place, or the one playing already, which changes nothing) is reported at
    /// once with the tracks it plays, so the remote settles; a pick that needs a new session tells
    /// the server with that session's start.
    fn pick_track(&mut self, pick: impl FnOnce(&mut PlaybackController)) {
        let g = self.current_generation();
        pick(self);
        if self.current_generation() == g {
            self.report(if self.want_play { ProgressState::Playing } else { ProgressState::Paused });
        }
    }

    /// A quality the remote picked, through the Quality menu's own path, kept for the next titles
    /// as a pick there is. One the menu doesn't offer for the file does nothing. The one picked
    /// already, or one the same stream plays on with, is reported at once; one that needs a new
    /// session is reported as that session begins.
    fn pick_quality(&mut self, id: &'static str) {
        if self.local() || !offered_qualities(self.file.as_ref(), self.original_only).iter().any(|q| q.id == id) {
            return;
        }
        let g = self.current_generation();
        // Set first: the new session may begin before the pick returns.
        self.report_begun = true;
        self.select_quality(id);
        if self.current_generation() == g {
            self.report_begun = false;
            self.report(if self.want_play { ProgressState::Playing } else { ProgressState::Paused });
        }
    }

    /// Plays (after the end: again from the start).
    pub fn play(&mut self) {
        if self.closed || self.stopped {
            return;
        }
        if self.ended {
            self.restart(0, false);
            return;
        }
        self.set_want_play(true);
        self.engine.play();
    }

    /// Pauses.
    pub fn pause(&mut self) {
        if self.closed {
            return;
        }
        self.engine.pause();
        self.set_want_play(false);
    }

    /// Plays or pauses, from what the player means to do now.
    pub fn toggle(&mut self) {
        if self.want_play { self.pause() } else { self.play() }
    }

    fn set_want_play(&mut self, play: bool) {
        let was = self.want_play;
        self.want_play = play;
        if play {
            self.sleep.resumed();
            self.sleep_changed();
            // Played again, by anyone: the remote's pause is over.
            if std::mem::take(&mut self.state.paused_by_remote) {
                self.changed();
            }
        }
        if self.info.is_none() || was == play {
            return;
        }
        if !play {
            self.struggle.reset();
            if let Some(a) = &mut self.auto {
                a.interrupted();
            }
            if !self.ended {
                self.report(ProgressState::Paused);
            }
            if self.view.status == EngineStatus::Ready {
                self.phase(Phase::Paused);
            }
        } else if self.view.status == EngineStatus::Ready {
            self.phase(Phase::Playing);
        }
        self.arm_progress();
    }

    /// Seeks to `position_ms`, a quarter second short of the end at most.
    pub fn seek_to(&mut self, position_ms: i64) {
        if self.closed || self.stopped {
            return;
        }
        let duration = self.clock.duration_ms;
        let target = if duration > 0 {
            position_ms.min((duration - SEEK_END_MARGIN_MS).max(0)).max(0)
        } else {
            position_ms.max(0)
        };
        self.hide_up_next();
        // The server's picture subtitles still being read: asked for at once from the target. A
        // drive's still being read out of the file: what is read so far, at once.
        self.presenter.sought(target as u64);
        if self.local()
            && let Some((_, at)) = &mut self.subtitle_fetch
        {
            *at = self.now;
            self.presenter.drive_ask_went_at_once();
        }
        if self.ended || self.info.is_none() || self.session_ended {
            self.last_position_ms = target;
            self.clock.position_ms = target;
            self.changed();
            self.restart(target, false);
            return;
        }
        self.engine.seek(target as u64);
        self.seeked_at = self.now;
        self.last_position_ms = target;
        self.started_at_ms = target;
        self.recovery.on_jump();
        self.struggle.reset();
        if let Some(a) = &mut self.auto {
            a.interrupted();
        }
        self.last_advance_at = self.now;
        self.clock.position_ms = target;
        self.changed();
        self.seek_report_at = Some(self.now + SEEK_REPORT_MS);
    }

    /// Plays the audio track `index`: in place when the TV's player switches it, else in a new session.
    pub fn select_audio(&mut self, index: i64) {
        let Some(chosen) = audio_streams(self.file.as_ref()).into_iter().find(|s| s.index == index).cloned() else {
            return;
        };
        if self.audio.as_ref().is_some_and(|a| a.index == index) {
            return;
        }
        self.audio = Some(chosen.clone());
        self.audio_picked = true;
        self.save_streams();
        self.publish_choices();
        let Some(info) = &self.info else { return };
        let ordinal = audio_streams(self.file.as_ref()).iter().position(|s| s.index == index).unwrap_or(0);
        if !info.hls && audio_plays_here(Some(&chosen), self.caps.as_ref()) {
            if self.switch_audio(ordinal, chosen.language.as_deref()) {
                self.quiet_frames();
                self.tracks_now.audio = Some(index);
                return;
            }
            // A new session would load the same file on a drive again and switch nothing.
            if self.local() {
                return;
            }
        }
        let at = self.live_position();
        self.restart(at, true);
    }

    /// Shows the subtitle track `index`, or none.
    pub fn select_subtitle(&mut self, index: Option<i64>) {
        let chosen = match index {
            None => None,
            Some(i) => match subtitle_streams(self.file.as_ref()).into_iter().find(|s| s.index == i) {
                Some(s) => Some(s.clone()),
                None => return,
            },
        };
        if chosen.as_ref().map(|s| s.index) == self.subtitle.as_ref().map(|s| s.index) {
            return;
        }
        self.subtitle = chosen.clone();
        self.save_streams();
        self.publish_choices();
        let Some(info) = &self.info else { return };
        // Pictures the TV can't draw are burned in: that is another session.
        let burn = is_picture(chosen.as_ref()) && !self.sideload();
        if burn != info.burn_subtitles || (burn && info.subtitle_index != chosen.as_ref().map(|s| s.index)) {
            let at = self.live_position();
            self.restart(at, true);
        } else {
            self.apply_tracks();
        }
    }

    /// Searches the subtitle files the server finds for what plays, in
    /// `language` (ISO 639-2; the profile's own when none): [`PlayerState::search`] says how it went.
    pub fn find_subtitles(&mut self, language: Option<&str>) {
        let (Some(item), Some(file)) = (&self.item, &self.file) else { return };
        let (item_id, file_id) = (item.id, file.id);
        let languages: Vec<String> = language.map(|l| vec![two_letter_code(l)]).unwrap_or_default();
        self.searches += 1;
        let search = self.searches;
        self.state.search = SubtitleSearch::Searching;
        self.changed();
        let server = self.env.server.clone();
        self.spawn(move || Done::Found { search, result: server.search_subtitles(item_id, Some(file_id), &languages) });
    }

    /// Adds `result` on the server, for everyone; then it is listed with the file's and shows.
    pub fn add_found_subtitle(&mut self, result: &SubtitleResult) {
        let (Some(item), Some(file)) = (&self.item, &self.file) else { return };
        let add = AddSubtitle { result: result.id.clone(), file_id: Some(file.id) };
        let item_id = item.id;
        self.state.search = SubtitleSearch::Adding(result.id.clone());
        self.changed();
        let server = self.env.server.clone();
        self.spawn(move || Done::Added { result: server.add_subtitle(item_id, &add) });
    }

    /// A track added on the server: listed with the file's own (and the session's, for its address), and shown.
    fn subtitle_added(&mut self, track: SubtitleTrack) {
        if self.closed || self.stopped || self.file.is_none() || self.item.is_none() {
            return;
        }
        let stream = MediaStream {
            index: track.index,
            kind: StreamKind::Subtitle,
            codec: track.codec.clone(),
            language: track.language.clone(),
            title: track.title.clone(),
            default: false,
            forced: track.forced,
            external: true,
            text_based: Some(track.text_based),
            added: true,
            deletable: track.deletable,
            ..MediaStream::default()
        };
        if let Some(file) = &mut self.file {
            file.streams.retain(|s| !(s.kind == StreamKind::Subtitle && s.index == track.index));
            file.streams.push(stream.clone());
            let updated = file.clone();
            if let Some(item) = &mut self.item {
                for f in item.files.iter_mut().filter(|f| f.id == updated.id) {
                    *f = updated.clone();
                }
            }
        }
        if let Some(info) = &mut self.info {
            info.subtitles.retain(|t| t.index != track.index);
            info.subtitles.push(track.clone());
        }
        self.state.search = SubtitleSearch::Idle;
        self.notice(&details::subtitles_added(&stream_label(&stream)));
        if self.subtitle.as_ref().is_some_and(|s| s.index == track.index) {
            self.publish_choices();
        } else {
            self.select_subtitle(Some(track.index));
        }
    }

    /// Moves the subtitles by `ms` (positive: later), kept for this file and profile.
    pub fn set_subtitle_offset(&mut self, ms: i64) {
        if self.closed {
            return;
        }
        let moved = clamp_offset(ms);
        if moved == self.offset {
            return;
        }
        self.offset = moved;
        self.state.subtitle_offset_ms = moved;
        self.changed();
        if let Some(f) = &self.file {
            self.env.offsets.set(self.env.server.server(), f.id, self.env.profile, i64::from(moved), &self.env.tasks);
        }
    }

    /// Plays at `rate` times normal speed, until the next title; only at the speeds offered.
    pub fn set_speed(&mut self, rate: f32) {
        if self.closed || !self.state.speeds.contains(&rate) {
            return;
        }
        if self.engine.set_rate(rate) {
            self.state.speed = rate;
            self.changed();
        }
    }

    /// Sets the sleep timer.
    pub fn set_sleep(&mut self, choice: SleepChoice) {
        if self.closed || self.stopped {
            return;
        }
        let item = self.sleep_item();
        self.sleep.set(self.now, choice, item);
        self.sleep_changed();
    }

    /// "Keep watching": the sleep timer adds the same time again.
    pub fn keep_watching(&mut self) {
        if self.closed || self.stopped {
            return;
        }
        let item = self.sleep_item();
        self.sleep.extend(self.now, item);
        self.sleep_changed();
        // What plays next was held for the timer: its countdown goes on.
        if let Some(known) = self.next.clone()
            && self.state.up_next.is_some()
            && self.countdown.is_none()
            && !self.ended
        {
            self.show_up_next(known, true);
        }
    }

    /// A quality picked in the menu, kept for the next titles. Automatic picked
    /// again while it plays lower starts afresh at the original. The session
    /// starts again only when it would ask for something else.
    pub fn select_quality(&mut self, id: &str) {
        let offered = offered_qualities(self.file.as_ref(), self.original_only);
        let Some(chosen) = offered.into_iter().find(|p| p.id == id) else { return };
        let lowered = self.auto.as_ref().is_some_and(|a| a.playing != ORIGINAL_ID || a.live_lowered());
        if chosen.id == self.quality.id && !lowered {
            return;
        }
        let before = self.playing_preset().id;
        let fell_back = self.recovery.fallback_mode.is_some();
        // A stream Automatic had sent on at less than its own bitrate.
        let eased = self.auto.as_ref().filter(|a| a.live_lowered()).and_then(|a| a.live_top());
        self.quality = chosen.clone();
        self.recovery.quality_changed();
        if chosen.id == AUTO_ID {
            if let Some(m) = &self.env.auto_memory {
                m.lock().unwrap_or_else(|e| e.into_inner()).set(None);
            }
            if let Some(network) = self.env.network.clone() {
                self.env.reporter.send(move || network.report(None));
            }
        }
        // Automatic picked again starts afresh at the original, nothing asked.
        self.start_auto(Some(None));
        self.state.suggest_lower_quality = false;
        self.state.note = self.note_text();
        self.state.can_play_original = false;
        self.changed();
        self.env.quality.remember(chosen.id);
        if self.item.is_some() && (self.playing_preset().id != before || fell_back) {
            self.publish_choices();
            let at = self.live_position();
            self.restart(at, true);
            return;
        }
        // The same stream plays on: Automatic watches it afresh, and it goes back to its own bitrate.
        if self.auto.is_some()
            && !self.session_ended
            && let Some(info) = self.info.clone()
        {
            let original = self.playing_preset().id == ORIGINAL_ID;
            self.auto_began(&info, original);
        }
        self.publish_choices();
        if let Some(top) = eased {
            self.send_bitrate(top);
        }
    }

    /// Skip Intro or Skip Credits.
    pub fn skip(&mut self) {
        let Some(marker) = active_marker(&self.markers, self.live_position()).cloned() else { return };
        let duration = self.clock.duration_ms;
        let target = skip_target(&marker);
        self.seek_to(if duration > 0 { target.min(duration) } else { target });
    }

    /// Plays what comes next now; says so when nothing does.
    pub fn play_next(&mut self) {
        if self.closed || self.stopped {
            return;
        }
        match self.next.clone() {
            Some(known) => self.switch_to(known),
            None => self.look_up_next(NextFor::PlayNext),
        }
    }

    /// Hides Up Next; after the end, the player leaves.
    pub fn cancel_up_next(&mut self) {
        self.hide_up_next();
        if self.ended {
            self.phase(Phase::Ended);
        }
    }

    /// After a fallback: tries the original again.
    pub fn play_original(&mut self) {
        if self.recovery.fallback_mode.is_none() {
            return;
        }
        self.recovery.play_original();
        self.state.can_play_original = false;
        self.state.note = self.note_text();
        self.changed();
        let at = self.live_position();
        self.restart(at, true);
    }

    /// Info or the compact readout opens (refreshed every 2 s) or closes.
    pub fn set_details_open(&mut self, open: bool) {
        self.details_at = None;
        self.details_open = open;
        if !open {
            self.state.details = None;
            self.changed();
            return;
        }
        self.details_at = Some(self.now);
    }

    fn details_loop(&mut self) {
        match self.info.as_ref().filter(|i| i.hls).map(|i| i.session_id.clone()) {
            Some(session) => {
                self.details_asking = true;
                let server = self.env.server.clone();
                self.spawn(move || Done::Details { result: server.session_status(&session, Duration::from_secs(20)) });
            }
            None => {
                self.refresh_details(true);
                self.details_at = Some(self.now + DETAILS_MS);
            }
        }
    }

    fn refresh_details(&mut self, force: bool) {
        if !force && self.state.details.is_none() {
            return;
        }
        let Some(info) = &self.info else { return };
        let note = self.note_text();
        let details = details::build_details(DetailsInput {
            info,
            file: self.file.as_ref(),
            audio_index: self.audio.as_ref().map(|a| a.index),
            note: note.as_deref(),
            position_ms: self.live_position(),
            live: self.live.as_ref(),
            subtitle: self.subtitle.as_ref(),
            sideload: self.sideload(),
        });
        self.state.details = Some(details);
        self.changed();
    }

    /// The toast was shown.
    pub fn dismiss_notice(&mut self) {
        if self.state.notice.take().is_some() {
            self.changed();
        }
    }

    /// Try again after an error.
    pub fn retry(&mut self) {
        if self.closed || self.stopped {
            return;
        }
        self.end_outage();
        self.recovery.retry();
        self.phase_force(Phase::Preparing);
        if self.item.is_none() {
            self.prepare_first();
        } else {
            let at = self.last_position_ms;
            self.restart(at, false);
        }
    }

    /// Where the player is, to start again there after the app comes back.
    pub fn save_state(&self) -> SavedPlayback {
        SavedPlayback {
            request: PlayRequest {
                item_id: self.item.as_ref().map_or(self.current.item_id, |d| d.id),
                file_id: self.file.as_ref().map(|f| f.id).or(self.current.file_id),
                start_ms: Some(self.live_position()),
                from_start: false,
                audio_index: if self.audio_picked { self.audio.as_ref().map(|a| a.index) } else { None },
                subtitle_index: self.subtitle.as_ref().map(|s| s.index),
                queue: self.current.queue,
            },
            quality: self.quality.id.to_string(),
            playing: self.want_play,
        }
    }

    /// The picture's place: none for the full screen, or a rectangle of the 960×540 canvas.
    pub fn set_video_rect(&mut self, rect: Option<Rect>) {
        self.engine.set_video_rect(rect);
    }

    /// Ends the playback: `stopped` and DELETE for the session, the engine unloaded.
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        let position = self.live_position();
        self.next_generation();
        self.end_outage();
        self.end_session(position);
        self.closed = true;
        self.sleep.cancel();
        self.presenter.hide();
        self.countdown = None;
        self.seek_report_at = None;
        self.details_at = None;
        self.trickplay_at = None;
        self.subtitle_fetch = None;
        self.jobs.clear();
        self.engine.unload();
    }

    // ---------------------------------------------------------------- seek previews and chapters

    /// The seek preview for `target_ms`: a tile of the file's sheets, else the chapter's own picture.
    pub fn seek_preview(&self, target_ms: i64) -> Option<SeekPreview> {
        if let Some(t) = &self.state.trickplay {
            let at = thumbnail_at(&t.info, preview_bucket(target_ms));
            let url = self.env.server.url(&sheet_path(t.file_id, &t.info, at.sheet));
            return Some(SeekPreview::Sheet { url, x: at.x, y: at.y, width: t.info.width, height: t.info.height });
        }
        let chapter = self.state.chapters.iter().rev().find(|c| c.start_ms <= target_ms)?;
        let url = chapter.image_url.as_deref()?;
        Some(SeekPreview::Chapter { url: with_width(url, CHAPTER_PICTURE_PX) })
    }

    // ---------------------------------------------------------------- Up Next

    fn on_credits(&mut self) {
        let has = self.item.as_ref().is_some_and(|d| has_next(d.kind, self.current.queue));
        if !has || self.state.up_next.is_some() {
            return;
        }
        self.look_up_next(NextFor::Credits);
    }

    /// Finds what plays next (one lookup at a time).
    fn look_up_next(&mut self, purpose: NextFor) {
        let Some((id, kind)) = self.item.as_ref().map(|d| (d.id, d.kind)) else {
            return self.next_found(purpose, None);
        };
        self.next_lookup += 1;
        let lookup = self.next_lookup;
        let queue = self.current.queue;
        let server = self.env.server.clone();
        self.spawn(move || Done::Next { lookup, purpose, result: next_after(server.as_ref(), id, kind, queue) });
    }

    fn next_found(&mut self, purpose: NextFor, found: Option<ItemSummary>) {
        if let Some(f) = &found {
            self.next = Some(f.clone());
        }
        match (purpose, found) {
            (NextFor::Credits, Some(f)) => self.show_up_next(f, true),
            (NextFor::Credits, None) => {}
            (NextFor::Ended { .. }, None) => self.phase(Phase::Ended),
            (NextFor::Ended { sleeping }, Some(f)) => self.show_up_next(f, !sleeping),
            (NextFor::PlayNext, Some(f)) => self.switch_to(f),
            (NextFor::PlayNext, None) => {
                let text =
                    if self.current.queue.is_some() { details::last_in_queue() } else { details::last_episode() };
                self.notice(text);
            }
            (NextFor::Remote, Some(f)) => self.switch_to(f),
            (NextFor::Remote, None) => {}
        }
    }

    fn show_up_next(&mut self, found: ItemSummary, restart_countdown: bool) {
        self.next = Some(found.clone());
        if self.next_detail.as_ref().is_none_or(|d| d.id != found.id) {
            let saved = self.choices_saved;
            let server = self.env.server.clone();
            let id = found.id;
            self.spawn(move || Done::NextDetail { saved, result: server.item(id) });
        }
        let image = found.thumb_url.as_deref().or(found.backdrop_url.as_deref());
        self.state.up_next = Some(UpNextCard {
            item_id: found.id,
            title: found.title.clone(),
            heading: details::up_next(label_of(&found).as_deref()),
            image_url: image.map(|i| self.env.server.url(&with_width(i, UP_NEXT_IMAGE_WIDTH))),
            seconds_left: None,
        });
        self.changed();
        if !restart_countdown {
            return;
        }
        self.countdown = None;
        let key = self.item.as_ref().map_or(-1, |d| d.id);
        if !self.env.autoplay_next || self.sleep.ends_with(key) {
            return;
        }
        self.countdown_step(found.id, COUNTDOWN_SECONDS);
    }

    /// One second of Up Next's countdown: `left` seconds before `item_id` plays.
    fn countdown_step(&mut self, item_id: i64, left: i64) {
        self.countdown = None;
        let Some(card) = self.state.up_next.as_mut().filter(|c| c.item_id == item_id) else { return };
        if left <= 0 {
            if let Some(next) = self.next.clone().filter(|n| n.id == item_id) {
                self.switch_to(next);
            }
            return;
        }
        card.seconds_left = Some(left);
        self.countdown = Some((item_id, left - 1, self.now + 1000));
        self.changed();
    }

    fn hide_up_next(&mut self) {
        self.countdown = None;
        if self.state.up_next.take().is_some() {
            self.changed();
        }
    }

    /// Plays `target` on the same engine: the session before ends, the next one starts where `target` resumes.
    fn switch_to(&mut self, target: ItemSummary) {
        self.hide_up_next();
        self.next_lookup += 1;
        let position = self.live_position();
        let ended_before = self.end_session(position);
        self.next = None;
        self.recovery.new_item();
        self.credits.reset();
        self.guard_mode = None;
        self.presenter.hide();
        self.ended = false;
        self.media_shown = false;
        let req = PlayRequest {
            item_id: target.id,
            file_id: None,
            start_ms: Some(start_of(&target)),
            from_start: false,
            audio_index: None,
            subtitle_index: None,
            queue: self.current.queue,
        };
        // The next title plays at normal speed.
        if self.state.speed != 1.0 {
            self.engine.set_rate(1.0);
        }
        let s = &self.state;
        self.state = PlayerState {
            item_id: target.id,
            title: s.title.clone(),
            sleep: s.sleep,
            speeds: s.speeds.clone(),
            notice_id: s.notice_id,
            remote_notice_id: s.remote_notice_id,
            // A pause goes on into the next title: the remote's is still the remote's.
            paused_by_remote: s.paused_by_remote,
            ..PlayerState::default()
        };
        self.clock.position_ms = req.start_ms.unwrap_or(0);
        self.clock.buffered_ms = 0;
        self.changed();
        let g = self.next_generation();
        let prefetched = self.next_detail.take().filter(|d| d.id == target.id);
        self.gather = Some(Gather {
            g,
            then: Then::Switch { req, flush: ended_before },
            item: None,
            caps: Some(None),
            known: None,
        });
        match prefetched {
            Some(d) => {
                if let Some(gather) = &mut self.gather {
                    gather.item = Some(Ok(d));
                }
            }
            None => {
                let server = self.env.server.clone();
                let id = target.id;
                self.spawn(move || Done::Item { g, result: server.item(id) });
            }
        }
        self.network_start(g);
        self.gathered();
    }

    // ---------------------------------------------------------------- seek previews

    fn ask_trickplay(&mut self, file_id: i64) {
        let server = self.env.server.clone();
        self.spawn(move || Done::Trickplay { file_id, result: server.trickplay(file_id) });
    }

    // ---------------------------------------------------------------- Automatic

    /// Automatic, when chosen: a new one for the title, starting where the
    /// server says this network allows (`known`), else where the TV last found
    /// it did. Its ceilings are told to the server.
    fn start_auto(&mut self, known: Option<Option<Remembered>>) {
        self.auto = None;
        if self.quality.id != AUTO_ID {
            return;
        }
        let file = self.file.as_ref();
        let network = self.env.network.clone();
        let wall = self.env.wall.clone();
        let mut options = AutoOptions::new(
            auto_ladder(file),
            file.and_then(|f| f.bitrate).filter(|b| *b > 0).map(|b| b as f64 / 1000.0),
        );
        options.memory = self.env.auto_memory.clone().map(|m| Box::new(ThroughShared(m)) as Box<_>);
        options.wall = Box::new(move || wall());
        // Told in order with the other reports, after whatever was said before.
        let reporter = self.env.reporter.clone();
        options.on_ceiling = network.map(|n| {
            Box::new(move |kbps: Option<f64>| {
                let n = n.clone();
                reporter.send(move || n.report(kbps));
            }) as Box<dyn FnMut(Option<f64>) + Send>
        });
        // LG's pipeline goes black between streams (docs/QUALITY.md).
        options.blanks = true;
        let mut auto = AutoQuality::new(options);
        auto.start(self.now, known);
        self.auto = Some(auto);
        self.auto_id += 1;
    }

    /// The quality a converted stream was asked at: the one Automatic plays, or the server's own for the original.
    fn stream_preset(&self, info: &PlaybackInfo) -> Option<Quality> {
        let playing = self.playing_preset();
        let preset =
            if playing.id == ORIGINAL_ID { info.auto_quality.as_deref().and_then(preset) } else { Some(playing) };
        preset.filter(|p| p.max_bitrate_kbps.is_some()).map(|p| named_for(self.file.as_ref(), &p))
    }

    /// Automatic watches the session `info` afresh: a stream whose bitrate
    /// changes as it is sent (returned), or the `original` converted, which it
    /// steps down from what it plays at.
    fn auto_began(&mut self, info: &PlaybackInfo, original: bool) -> Option<LiveStream> {
        let live = self.live_stream(info);
        let converted = if original { info.auto_quality.as_deref().and_then(preset) } else { None };
        let auto = self.auto.as_mut()?;
        auto.began(live);
        if let Some(c) = converted {
            auto.set_original_kbps(c.max_bitrate_kbps.map(f64::from));
        }
        live
    }

    /// A converted stream whose bitrate changes as it is sent (`live_bitrate`):
    /// its own bitrate, and the least its size keeps; none for any other.
    fn live_stream(&self, info: &PlaybackInfo) -> Option<LiveStream> {
        if !info.live_bitrate || info.method != PlaybackMethod::Transcode {
            return None;
        }
        let preset = self.stream_preset(info)?;
        let top = f64::from(preset.max_bitrate_kbps?);
        Some(LiveStream { top, floor: top.min(live_floor(stream_lines(self.file.as_ref(), &preset))) })
    }

    /// The stream playing goes on at `kbps` from the segments the TV's player
    /// hasn't fetched yet (`PUT /api/playback/{id}/bitrate`).
    fn send_bitrate(&mut self, kbps: f64) {
        let Some(session) = self.info.as_ref().filter(|_| !self.session_ended).map(|i| i.session_id.clone()) else {
            return;
        };
        let server = self.env.server.clone();
        let auto = self.auto_id;
        self.spawn(move || {
            let result = server.set_bitrate(&session, kbps.round() as u32);
            Done::Bitrate { session, auto, asked: kbps, result }
        });
    }

    /// Tells Automatic what the engine shows while it is meant to play, and acts on what it decides; true when a new session starts.
    fn watch_quality(&mut self, moving: bool) -> bool {
        let now = self.now;
        if self.auto.is_none() || !self.want_play || !self.media_shown || self.ended || self.in_error() {
            return false;
        }
        if now - self.seeked_at <= SEEK_SETTLE_MS {
            return false;
        }
        // A stream in several sizes: the TV's player moves between them itself.
        if self.info.as_ref().is_some_and(|i| i.adaptive) {
            return false;
        }
        let position = self.engine.position_ms() as f64 / 1000.0;
        let buffered = self.view.buffered_end.map(|e| e as f64 / 1000.0);
        let auto = self.auto.as_mut().expect("checked");
        if moving {
            auto.sample(now, position, buffered);
        }
        let step = auto.check(now);
        self.apply_auto(step)
    }

    /// Acts on Automatic's step: the stream goes on at its new bitrate, or a new one starts. A step down says so, once.
    fn apply_auto(&mut self, step: Option<AutoStep>) -> bool {
        let Some(step) = step else { return false };
        if self.closed || self.stopped {
            return false;
        }
        let stream = match (step.bitrate, &self.info) {
            (Some(_), Some(info)) => self.stream_preset(info),
            _ => None,
        };
        let to = match (&stream, step.bitrate) {
            (Some(s), Some(kbps)) => live_label(self.file.as_ref(), s, kbps),
            _ => named_for(self.file.as_ref(), &preset(&step.to).unwrap_or_else(Quality::original)).label,
        };
        self.state.suggest_lower_quality = false;
        self.state.note = self.note_text();
        self.changed();
        self.publish_choices();
        if step.why.is_some() {
            self.notice(&details::smoother(&to));
        }
        self.struggle.reset();
        match step.bitrate {
            Some(kbps) => self.send_bitrate(kbps),
            None => {
                let at = self.live_position();
                self.restart(at, true);
            }
        }
        true
    }

    /// Why it doesn't play the original: the player's fallback, or Automatic's step down.
    fn note_text(&self) -> Option<String> {
        if let Some(reason) = self.recovery.fallback_reason {
            return Some(details::fallback_text(reason).into());
        }
        let a = self.auto.as_ref()?;
        let why = a.lowered?;
        (a.playing != ORIGINAL_ID || a.live_lowered()).then(|| details::lowered_text(why).into())
    }

    // ---------------------------------------------------------------- subtitles

    fn subtitles_now(&mut self) {
        if self.presenter.showing().is_none() {
            self.subtitle_fetch = None;
            return;
        }
        if let Some(fetch) = self.presenter.next_fetch() {
            self.subtitle_fetch = Some((fetch.clone(), self.now + fetch.after_ms as i64));
        }
        if let Some((fetch, at)) = self.subtitle_fetch.take() {
            if self.now >= at {
                let server = self.env.server.clone();
                let mut url = fetch.url.to_string();
                // The server's picture track from where the player is (a seek's target), at every
                // ask: while it isn't whole, the answer is the pictures around there.
                if fetch.kind == TrackKind::Pictures && !self.local() {
                    let at = fetch.at_ms.unwrap_or(self.live_position().max(0) as u64);
                    url = plan::picture_url_at(&url, at);
                }
                let timeout = Duration::from_millis(fetch.kind.timeout_ms());
                let asked = self.now;
                self.spawn(move || {
                    // A file on a drive hands its picture tracks over as they are.
                    if fetch.kind == TrackKind::Pictures
                        && let Some(result) = server.picture_track(&url, timeout)
                    {
                        return Done::Pictures { fetch, result };
                    }
                    let result = server.subtitles(&url, fetch.kind.accept(), timeout);
                    Done::Subtitle { fetch, asked, result }
                });
            } else {
                self.subtitle_fetch = Some((fetch, at));
            }
        }
        let position = self.live_position().max(0) as u64;
        if self.presenter.update(position, i64::from(self.offset)) {
            self.changed();
        }
    }

    fn on_subtitle(&mut self, fetch: Fetch, asked: i64, result: Result<crate::server::SubtitleAnswer, ApiError>) {
        let waited = (self.now - asked).max(0) as u64;
        let trouble = match result {
            Err(e) => self.presenter.fetch_failed(fetch.id, e.status()),
            Ok(answer) => match fetch.kind {
                TrackKind::Text => {
                    self.presenter.text_arrived(fetch.id, &String::from_utf8_lossy(&answer.body), answer.complete);
                    None
                }
                TrackKind::Pictures => {
                    let picture = PictureAnswer::Whole(answer.body);
                    self.presenter.pictures_arrived(fetch.id, picture, answer.complete, waited).err()
                }
            },
        };
        self.subtitle_trouble(trouble);
    }

    /// A picture track handed over as it is, and whether it is whole.
    fn on_pictures(&mut self, fetch: Fetch, result: Result<(PictureTrack, bool), ApiError>) {
        let trouble = match result {
            Err(e) => self.presenter.fetch_failed(fetch.id, e.status()),
            Ok((track, complete)) => {
                self.presenter.pictures_arrived(fetch.id, PictureAnswer::Track(track), complete, 0).err()
            }
        };
        self.subtitle_trouble(trouble);
    }

    fn subtitle_trouble(&mut self, trouble: Option<Trouble>) {
        match trouble {
            Some(Trouble::Failed) => self.notice(details::subtitles_failed()),
            Some(Trouble::PicturesUnavailable) => self.pictures_failed(),
            None => {}
        }
        self.changed();
    }

    // ---------------------------------------------------------------- answers

    fn on_done(&mut self, done: Done) {
        if self.closed {
            return;
        }
        match done {
            Done::Item { g, result } => {
                if let Some(gather) = self.gather.as_mut().filter(|x| x.g == g) {
                    gather.item = Some(result);
                    self.gathered();
                }
            }
            Done::Caps { g, report } => {
                if let Some(gather) = self.gather.as_mut().filter(|x| x.g == g) {
                    gather.caps = Some(report);
                    self.gathered();
                }
            }
            Done::Known { g, known } => {
                if let Some(gather) = self.gather.as_mut().filter(|x| x.g == g) {
                    gather.known = Some(known);
                    self.gathered();
                }
            }
            Done::Session { g, asked, result } => self.on_session(g, *asked, result),
            Done::Check { session, check, result } => self.on_check(session, check, result),
            Done::Reach { outage, result } => self.on_reach(outage, result),
            Done::Bitrate { session, auto, asked, result } => {
                if self.info.as_ref().is_none_or(|i| i.session_id != session) || auto != self.auto_id {
                    return;
                }
                if let Some(a) = &mut self.auto {
                    match result {
                        Ok(answer) => a.live_at(f64::from(answer.bitrate), asked),
                        // A stream that can't change (an older transcoder, a stream no longer converted) isn't asked again.
                        Err(e) => a.live_refused(e.is(400)),
                    }
                }
                self.state.note = self.note_text();
                self.changed();
                self.publish_choices();
            }
            Done::Next { lookup, purpose, result } => {
                if lookup != self.next_lookup {
                    return;
                }
                self.next_found(purpose, result.ok().flatten());
            }
            Done::NextDetail { saved, result } => {
                // Tracks picked meanwhile change what it starts with: asked again then.
                if let Ok(d) = result
                    && saved == self.choices_saved
                {
                    self.next_detail = Some(d);
                }
            }
            Done::Trickplay { file_id, result } => {
                if self.file.as_ref().is_none_or(|f| f.id != file_id) {
                    return;
                }
                match result {
                    Ok(info) if info.count > 0 => {
                        self.state.trickplay = Some(Trickplay { file_id, info });
                        self.changed();
                    }
                    Ok(_) => {}
                    Err(_) => {
                        if self.now - self.trickplay_started < TRICKPLAY_GIVE_UP_MS {
                            self.trickplay_at = Some((self.now + TRICKPLAY_RETRY_MS, file_id));
                        }
                    }
                }
            }
            Done::Details { result } => {
                self.details_asking = false;
                if let Ok(status) = result {
                    self.live = status.transcode;
                }
                if !self.details_open {
                    return;
                }
                self.refresh_details(true);
                self.details_at = Some(self.now + DETAILS_MS);
            }
            Done::Subtitle { fetch, asked, result } => self.on_subtitle(fetch, asked, result),
            Done::Pictures { fetch, result } => self.on_pictures(fetch, result),
            Done::Found { search, result } => {
                if search != self.searches {
                    return;
                }
                self.state.search = match result {
                    Ok(found) => SubtitleSearch::Found(found),
                    Err(e) => SubtitleSearch::Failed(e),
                };
                self.changed();
            }
            Done::Added { result } => match result {
                Ok(track) => self.subtitle_added(track),
                Err(e) => {
                    self.state.search = SubtitleSearch::Failed(e);
                    self.changed();
                }
            },
        }
    }

    // ---------------------------------------------------------------- state

    fn publish_choices(&mut self) {
        let file = self.file.as_ref();
        let info = self.info.as_ref();
        let sideload = self.sideload();
        let audio: Vec<TrackChoice> = audio_streams(file)
            .into_iter()
            .map(|s| TrackChoice {
                index: Some(s.index),
                label: stream_label(s),
                selected: self.audio.as_ref().is_some_and(|a| a.index == s.index),
                burn_in: false,
                added: false,
            })
            .collect();
        let subs = subtitle_streams(file);
        let subtitles = if subs.is_empty() {
            Vec::new()
        } else {
            let off = TrackChoice {
                index: None,
                label: details::subtitles_off().into(),
                selected: self.subtitle.is_none(),
                burn_in: false,
                added: false,
            };
            std::iter::once(off)
                .chain(subs.into_iter().map(|s| TrackChoice {
                    index: Some(s.index),
                    label: stream_label(s),
                    selected: self.subtitle.as_ref().is_some_and(|x| x.index == s.index),
                    burn_in: is_picture(Some(s)) && !sideload,
                    added: s.added,
                }))
                .collect()
        };
        // Text moves always; pictures when drawn here from the server's .mks.
        let syncable = self
            .subtitle
            .as_ref()
            .is_some_and(|s| !is_picture(Some(s)) || (sideload && info.is_none_or(|i| !i.burn_subtitles)));
        let playing = self.playing_preset();
        // The original, converted here: at the closest quality, which Automatic says it plays.
        let converted = info
            .and_then(|i| i.auto_quality.as_deref())
            .filter(|_| playing.id == ORIGINAL_ID)
            .and_then(preset)
            .map(|p| named_for(file, &p))
            .filter(|p| p.max_bitrate_kbps.is_some())
            .map(|p| p.label);
        let hint = Some(file_hint(file)).filter(|h| !h.is_empty());
        let auto = self.auto.as_ref();
        // A stream Automatic sends on below its own bitrate: "4 Mbps 1080p".
        let eased = match (auto, info) {
            (Some(a), Some(i)) if a.live_lowered() => {
                self.stream_preset(i).and_then(|s| a.live_kbps.map(|k| live_label(file, &s, k)))
            }
            _ => None,
        };
        let auto_now = eased.clone().unwrap_or_else(|| {
            if playing.id == ORIGINAL_ID {
                converted.unwrap_or_else(|| t!("player.quality.original").into())
            } else {
                playing.label.clone()
            }
        });
        // Nothing converts a file on a drive: no Quality to choose.
        let qualities = offered_qualities(file, self.original_only)
            .into_iter()
            .filter(|_| !self.local())
            .map(|p| QualityChoice {
                id: p.id,
                hint: if p.id == AUTO_ID {
                    Some(match auto {
                        Some(a) => {
                            let why = a
                                .lowered
                                .filter(|_| playing.id != ORIGINAL_ID || eased.is_some())
                                .map(details::lowered_text);
                            details::auto_now(&auto_now, why)
                        }
                        None => details::auto_hint().into(),
                    })
                } else if p.id == ORIGINAL_ID {
                    hint.clone()
                } else {
                    None
                },
                selected: p.id == self.quality.id,
                label: p.label,
            })
            .collect();
        let s = &mut self.state;
        s.audio = audio;
        s.subtitles = subtitles;
        s.qualities = qualities;
        s.quality = if auto.is_some() { details::automatic(&auto_now) } else { self.quality.label.clone() };
        s.subtitle_sync = syncable;
        s.subtitle_offset_ms = self.offset;
        self.changed();
    }

    /// `PUT /api/items/{id}/streams`: the next episode, or the next viewing,
    /// starts with the same tracks. Audio the server chose for this TV isn't
    /// saved: another device may play another track of that language better.
    fn save_streams(&mut self) {
        let Some(item_id) = self.item.as_ref().map(|d| d.id) else { return };
        let choice = StreamChoice {
            file_id: self.file.as_ref().map(|f| f.id),
            audio: if self.audio_picked {
                self.audio.as_ref().map_or(AudioChoice::Automatic, |a| AudioChoice::Track(a.index))
            } else {
                AudioChoice::Keep
            },
            subtitle_index: self.subtitle.as_ref().map(|s| s.index),
        };
        let server = self.env.server.clone();
        self.env.reporter.send(move || {
            let _ = server.set_streams(item_id, &choice);
        });
        // The next episode's details, asked for before, name the tracks it
        // started with then: they are asked for again, after this choice is saved.
        self.choices_saved += 1;
        self.next_detail = None;
    }

    fn phase(&mut self, phase: Phase) {
        if self.in_error() || self.state.phase.same_kind(&phase) {
            return;
        }
        self.state.phase = phase;
        self.changed();
    }

    /// Sets the phase even over an error (a new session, Try again).
    fn phase_force(&mut self, phase: Phase) {
        if self.state.phase != phase {
            self.state.phase = phase;
            self.changed();
        }
    }

    fn notice(&mut self, text: &str) {
        self.state.notice = Some(text.to_string());
        self.state.notice_id += 1;
        self.changed();
    }

    fn fail(&mut self, message: &str, can_retry: bool, title: Option<String>) {
        self.state.phase = Phase::Error { message: message.to_string(), can_retry, title };
        self.state.skip_label = None;
        self.changed();
    }

    fn fail_with(&mut self, e: &ApiError) {
        // A file on a drive asks nothing of a server: what fails is reading it. What nothing
        // converts says so itself.
        if self.local() && !matches!(e, ApiError::Status { code, .. } if code == "local_file") {
            self.fail(details::drive_unreadable(), true, None);
            return;
        }
        if let ApiError::SessionStopped { message } = e {
            self.stop_by_server(message.as_deref(), true);
            return;
        }
        if e.unreachable() {
            self.fail(details::offline_body(), true, Some(details::offline_title().into()));
            return;
        }
        let can_retry = !matches!(e, ApiError::SignedOut) && !matches!(e.status(), Some(401 | 403 | 404));
        self.fail(&sentence(&e.to_string()), can_retry, None);
    }

    // ---------------------------------------------------------------- the server out of reach

    /// The server stopped answering mid-playback, or its transcoder did (the
    /// Android app's `MediaLoading` and `ReconnectWatcher`): the player keeps
    /// its place, says "Reconnecting…" and asks again after 0.5 s, 1 s, then
    /// every 2 s, for 90 s. No recovery is spent on it, and no stall makes it
    /// convert the video.
    fn reach_lost(&mut self, engine_failed: bool) {
        self.reach_lost_after(engine_failed, None);
    }

    /// [`Self::reach_lost`]; `away`: the wait before (its start and attempts), which a new
    /// session ended too soon, goes on.
    fn reach_lost_after(&mut self, engine_failed: bool, away: Option<(i64, usize)>) {
        if self.closed || self.stopped {
            return;
        }
        if let Some(o) = &mut self.outage {
            o.engine_failed |= engine_failed;
            return;
        }
        self.outages += 1;
        let (since, attempts) = away.unwrap_or((self.now, 0));
        self.outage =
            Some(Outage { id: self.outages, since, attempts, due: None, engine_failed, at_ms: self.last_position_ms });
        self.state.reconnecting = true;
        self.state.skip_label = None;
        self.changed();
        if !self.in_error() {
            self.phase(Phase::Buffering);
        }
        self.ask_again_later();
    }

    fn ask_again_later(&mut self) {
        let now = self.now;
        if let Some(o) = &mut self.outage {
            o.due = Some(now + OUTAGE_STEPS_MS[o.attempts.min(OUTAGE_STEPS_MS.len() - 1)]);
            o.attempts += 1;
        }
    }

    fn ask_again(&mut self, outage: u64) {
        let server = self.env.server.clone();
        let session = self.info.as_ref().filter(|_| !self.session_ended).map(|i| i.session_id.clone());
        self.spawn(move || {
            let timeout = Duration::from_millis(OUTAGE_ASK_MS);
            let result = match session {
                Some(s) => server.session_status(&s, timeout).map(drop),
                None => server.reachable(timeout).map(drop),
            };
            Done::Reach { outage, result }
        });
    }

    fn on_reach(&mut self, id: u64, result: Result<(), ApiError>) {
        let Some(o) = self.outage.as_ref().filter(|o| o.id == id) else { return };
        let since = o.since;
        match result {
            Ok(()) => self.reach_back(false),
            Err(ApiError::SessionStopped { message }) => {
                self.end_outage();
                self.stop_by_server(message.as_deref(), true);
            }
            // It answers again, but the session went (404) or failed: a new one.
            Err(e) if !e.out_of_reach() => self.reach_back(true),
            Err(_) if self.now - since >= OUTAGE_MS => {
                self.end_outage();
                self.fail(details::offline_body(), true, Some(details::offline_title().into()));
            }
            Err(_) => self.ask_again_later(),
        }
    }

    /// The server answers again: the engine carries on if it came back by itself, else a new session starts at the place.
    fn reach_back(&mut self, session_gone: bool) {
        if self.closed || self.stopped {
            return;
        }
        let Some(o) = self.outage.as_ref() else { return };
        let (engine_failed, away) = (o.engine_failed, (o.since, o.attempts));
        let at = self.last_position_ms;
        self.end_outage();
        if !session_gone && !engine_failed && self.view.playing && self.view.status == EngineStatus::Ready {
            self.phase(Phase::Playing);
            return;
        }
        self.restart_after(at, false, Some(away));
    }

    fn end_outage(&mut self) {
        if self.outage.take().is_none() {
            return;
        }
        // The stall clock starts over.
        self.last_advance_at = self.now;
        self.state.reconnecting = false;
        self.changed();
    }
}

impl Drop for PlaybackController {
    fn drop(&mut self) {
        self.close();
    }
}

/// A progress report of `info` at `position_ms`, with the tracks it plays and the quality picked.
fn progress(
    position_ms: i64,
    info: &PlaybackInfo,
    state: ProgressState,
    tracks: PlayingTracks,
    quality: &str,
) -> ProgressReport {
    ProgressReport {
        position_ms: position_ms.max(0),
        duration_ms: (info.duration_ms > 0).then_some(info.duration_ms),
        state,
        session_id: Some(info.session_id.clone()),
        age_ms: None,
        audio_index: tracks.audio,
        subtitle_index: Some(tracks.subtitle),
        quality: Some(quality.to_owned()),
    }
}

/// What the menu of `tracks` calls the track of `index` (none: its Off row).
fn menu_label(tracks: &[TrackChoice], index: Option<i64>) -> Option<String> {
    tracks.iter().find(|t| t.index == index).map(|t| t.label.clone())
}

/// A language code as tracks are compared: its two-letter form where it has one, without a region
/// ("eng", "en-GB" and "en" are "en"; "vie" is "vi").
fn norm(language: Option<&str>) -> String {
    let code = language.unwrap_or("").trim().split(['-', '_']).next().unwrap_or("");
    two_letter_code(code)
}

/// Whether two codes (as [`norm`] writes them) name the same language: the same, or one beginning
/// the other where a code isn't in the table (`TizenMediaEngine.Same`). Compared exactly, a file's
/// "vie" never met the TV's "vi", so the track was asked for through a new stream at every try.
fn same_language(a: &str, b: &str) -> bool {
    a == b || (a.len() >= 2 && b.len() >= 2 && (a.starts_with(b) || b.starts_with(a)))
}

/// The engine's track for the file's `ordinal`-th audio track: the one in
/// that place of the engine's list when its language agrees (or it has
/// none), else the first in that language. LG's pipeline lists the default
/// first, not always in the file's order. The answer is what `select_audio` takes.
pub fn match_track(tracks: &[AudioTrack], ordinal: usize, language: Option<&str>) -> Option<usize> {
    let want = norm(language);
    if let Some(t) = tracks.get(ordinal) {
        let theirs = norm(t.language.as_deref());
        if want.is_empty() || theirs.is_empty() || same_language(&theirs, &want) {
            return Some(t.ordinal);
        }
    }
    if want.is_empty() {
        return None;
    }
    tracks.iter().find(|t| same_language(&norm(t.language.as_deref()), &want)).map(|t| t.ordinal)
}

/// The chapter start the next-chapter key goes to: the next one starting more than a second ahead.
pub fn next_chapter_start(chapters: &[ChapterMark], position_ms: i64) -> Option<i64> {
    chapters.iter().map(|c| c.start_ms).find(|s| *s > position_ms + 1_000)
}

/// Where the previous-chapter key goes: this chapter's start when more than
/// 3 s into it (or in the first), else the chapter before; 0 without chapters.
pub fn previous_chapter_start(chapters: &[ChapterMark], position_ms: i64) -> i64 {
    let Some(at) = chapters.iter().rposition(|c| c.start_ms <= position_ms) else { return 0 };
    let start = chapters[at].start_ms;
    if position_ms - start > 3_000 || at == 0 { start } else { chapters[at - 1].start_ms }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(ordinal: usize, language: &str) -> AudioTrack {
        AudioTrack { ordinal, language: Some(language.into()), ..AudioTrack::default() }
    }

    #[test]
    fn tracks_match_by_place_then_language() {
        let tracks = [track(0, "fre"), track(1, "eng"), track(2, "ger")];
        assert_eq!(match_track(&tracks, 1, Some("eng")), Some(1));
        // LG moved the default first: the English track is found by its language.
        assert_eq!(match_track(&tracks, 0, Some("en")), Some(1));
        assert_eq!(match_track(&tracks, 2, Some("deu")), Some(2));
        assert_eq!(match_track(&tracks, 2, Some("jpn")), None);
        assert_eq!(match_track(&[], 0, Some("eng")), None);
        assert_eq!(match_track(&tracks, 0, None), Some(0));
        assert_eq!(match_track(&tracks, 5, None), None);
    }

    #[test]
    fn languages_written_two_ways_are_the_same() {
        let named = |codes: &[&str]| -> Vec<AudioTrack> {
            codes
                .iter()
                .enumerate()
                .map(|(i, c)| AudioTrack { ordinal: i, language: Some(c.to_string()), ..AudioTrack::default() })
                .collect()
        };
        for (file, tv) in [("vie", "vi"), ("ind", "id"), ("ukr", "uk"), ("hrv", "hr"), ("fre", "fr"), ("por", "pt-BR")]
        {
            let tracks = named(&["en", tv]);
            assert_eq!(match_track(&tracks, 1, Some(file)), Some(1), "{file} and {tv}");
            assert_eq!(match_track(&tracks, 0, Some(file)), Some(1), "{file} found by language");
        }
        // Different languages stay apart, whatever their letters.
        assert_eq!(match_track(&named(&["en", "ar"]), 1, Some("arm")), None);
    }

    #[test]
    fn engine_errors_in_recoverys_terms() {
        assert!(matches!(engine_error(2, "", false), EngineError::Network { .. }));
        assert!(matches!(
            engine_error(3, "audio decoder", false),
            EngineError::Unsupported { audio: true, container: false, .. }
        ));
        assert!(matches!(engine_error(4, "", true), EngineError::Playlist { .. }));
        assert!(matches!(engine_error(4, "demuxer failed", false), EngineError::Unsupported { container: true, .. }));
        assert!(matches!(engine_error(404, "", true), EngineError::Http { status: 404, .. }));
        assert!(matches!(engine_error(-7, "PIPELINE_ERROR", false), EngineError::Other { .. }));
    }

    #[test]
    fn chapter_keys() {
        let c = |s| ChapterMark { title: None, start_ms: s, end_ms: s + 60_000, image_url: None };
        let chapters = [c(0), c(60_000), c(120_000)];
        assert_eq!(next_chapter_start(&chapters, 59_500), Some(120_000));
        assert_eq!(next_chapter_start(&chapters, 58_000), Some(60_000));
        assert_eq!(next_chapter_start(&chapters, 130_000), None);
        assert_eq!(previous_chapter_start(&chapters, 65_000), 60_000);
        assert_eq!(previous_chapter_start(&chapters, 61_000), 0);
        assert_eq!(previous_chapter_start(&chapters, 1_000), 0);
        assert_eq!(previous_chapter_start(&[], 10_000), 0);
    }
}

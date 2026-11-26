//! A live channel playing on the TV's own player (packages/tv/src/live/session.ts,
//! the Android app's `LivePlayback`, the Samsung app's `Live/LivePlayback.cs`):
//! watching it on the server with this TV's id and a keyed playlist address,
//! changing channels, and the server told at once when watching ends. Live is
//! live: no pause, rewind or skip. The user's remote, on another of their
//! devices, changes channel through the owner's channel up and down
//! ([`LiveSession::server_event`]), and stops it (`session_stopped`).
//!
//! - As the Samsung app's, the channel's `stream.ts` (MPEG-TS as the provider
//!   sends it) is read at once, under an id of its own, while the server is
//!   asked what the channel is made into ([`plays_as_sent`]): sent as it is,
//!   or with sound the TV decodes itself, it plays from its first keyframe
//!   rather than after the first HLS segments. So does one the server would
//!   convert whose video and sound this TV plays as the provider sends them,
//!   while the server's Direct Play is on: no transcode is made for it. Its
//!   address takes the token, which the TV's player can't send, so the app
//!   reads it and hands it on from the TV's loopback address ([`StreamRelay`]).
//! - Otherwise, or when it fails, the server's HLS from its live edge, once
//!   its playlist has come: before the stream.ts played at once; while it
//!   played, once the server has said watching didn't end there.
//! - A stream.ts that stands still while it goes on coming (its provider's
//!   stream started its clock over, and the TV's player says nothing) is read
//!   again on the same session, from the server's last keyframe
//!   ([`crate::live::stuck`], the web app's rules).
//! - Playback details ([`crate::live::details`]) asks the plan again while
//!   it shows ([`LiveSession::ask_plan`]): the server answers what it knows
//!   at once, with what the provider sends now.
//!
//! The owner calls [`LiveSession::tick`] each frame with the app's clock in ms.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use pley_base::Rect;
use pley_core::ApiError;
use pley_core::caps::takes;
use pley_core::http::{Method, sentence};
use pley_core::media::{EngineEvent, EngineStats, LoadRequest, MediaKind, PlayerEngine};
use pley_core::model::{
    BusyConnection, ClientCaps, DeviceKind, LiveChannel, LivePlan, MediaStream, ServerEvent, VideoDecoder, WatchRequest,
};
use pley_core::task::{Cancel, Promise, Tasks};
use pley_i18n::t;

use crate::controller::engine_error;
use crate::env::{CapsProbe, PlayerEnv};
use crate::live::relay::{Opener, StreamRelay};
use crate::live::stuck::{Coming, StuckWatch, Unstick, unstick};
use crate::recovery::EngineError;
use crate::remote::{Command, RemoteAction, RemotePlayer, live_controls, remote_action};
use crate::reporter::Reporter;
use crate::server::PlayerServer;

/// The server waits for the channel's first segments before it answers the playlist.
pub const PLAYLIST_WAIT_MS: u64 = 45_000;
/// No picture this long after the playlist came: the channel didn't start.
pub const START_WAIT_MS: i64 = 30_000;
/// Trying again after the engine fails, this many times: a second, then three later.
pub const RETRIES: u32 = 2;
/// The sound is checked for a picture this long after it starts.
pub const PICTURE_CHECK_MS: i64 = 1_500;
/// A stall this long and the server is asked whether watching ended there: a
/// TV's player says nothing of a playlist that has gone (`410`), it only
/// stops once what it holds runs out.
pub const STALL_CHECK_MS: i64 = 5_000;
/// A stall this long: watched anew, as a failure.
pub const STALL_GIVE_UP_MS: i64 = 20_000;
/// The longest a watch waits for the last session's leaving to be said.
const LEAVE_WAIT_MS: u64 = 2_000;
/// The stream.ts not playing this long after it was given: watched as HLS. LG's pipeline may
/// wait on a stream it can't play rather than say so.
pub const DIRECT_WAIT_MS: i64 = 10_000;
/// `stream.ts` is read under this TV's id with this after it (Android's, Samsung's): the HLS
/// session the plan is asked of stays.
pub const STREAM_CLIENT: &str = "-stream";
/// Sound every LG TV decodes itself from an MPEG-TS, which the plan converts for players that
/// can't: AC-3, E-AC-3 and MP2 (docs/LG.md, "What an LG TV plays").
pub const TV_DECODED_AUDIO: [&str; 3] = ["ac3", "eac3", "mp2"];

/// Video LG's pipeline reads from an MPEG-TS, by ffprobe's names: H.264, HEVC and MPEG-2, on
/// every webOS from 4.0 (LG's format pages, docs/LG.md's references). They list VP9 and AV1 only
/// from Matroska and MP4, so a channel in those stays with the server.
pub const TS_VIDEO: [&str; 3] = ["h264", "hevc", "mpeg2video"];
/// Sound they list for one that every TV's report decodes: AAC, AC-3, E-AC-3, MP2 and MP3.
pub const TS_AUDIO: [&str; 5] = ["aac", "ac3", "eac3", "mp2", "mp3"];

/// Sent as it is, or left out.
pub(crate) fn as_is(action: &str) -> bool {
    action == "copy" || action == "none"
}

/// The channel as the server sends it plays here whatever the TV reports (the Samsung app's
/// `LiveRules.PlaysAsSent`): its picture sent as it is (or none), its sound sent as it is (or
/// none) or in a format every LG TV decodes itself.
pub(crate) fn sent_as_it_is(plan: &LivePlan) -> bool {
    as_is(&plan.video)
        && (as_is(&plan.audio)
            || plan.audio_codec.as_deref().is_some_and(|c| TV_DECODED_AUDIO.contains(&c.to_ascii_lowercase().as_str())))
}

/// Whether this TV decodes the provider's video from the stream.ts: progressive (interlaced
/// video is the server's to make progressive), in a codec the pipeline reads from MPEG-TS, and
/// within a decoder of the TV's report at its bit depth (8 when the plan doesn't say), size and
/// frame rate, by the rule a file's direct play goes by ([`takes`]). A report that only names
/// codecs (version 1) is taken at 8 bits.
pub(crate) fn decodes_video(plan: &LivePlan, caps: &ClientCaps) -> bool {
    let Some(codec) = plan.video_codec.as_deref().map(str::to_ascii_lowercase) else { return false };
    if plan.interlaced || !TS_VIDEO.contains(&codec.as_str()) {
        return false;
    }
    let depth = plan.bit_depth.filter(|d| *d > 0).unwrap_or(8);
    let decoders: Vec<&VideoDecoder> = caps.video.iter().filter(|e| e.codec.eq_ignore_ascii_case(&codec)).collect();
    if decoders.is_empty() {
        return !caps.is_v2() && depth <= 8 && caps.video_codecs.iter().any(|c| c.eq_ignore_ascii_case(&codec));
    }
    let video = MediaStream {
        codec,
        width: plan.width,
        height: plan.height,
        frame_rate: plan.frame_rate,
        ..MediaStream::default()
    };
    decoders.iter().any(|e| takes(e, &video, depth))
}

/// Whether this TV plays the provider's sound from the stream.ts: in a format the pipeline
/// reads from MPEG-TS, which the TV's report decodes or sends on to a receiver.
pub(crate) fn plays_audio(plan: &LivePlan, caps: &ClientCaps) -> bool {
    plan.audio_codec
        .as_deref()
        .is_some_and(|c| TS_AUDIO.iter().any(|t| t.eq_ignore_ascii_case(c)) && caps.plays_audio(c))
}

/// Whether this TV plays a channel's stream.ts as the provider sends it. A channel the server
/// sends as it is always does ([`sent_as_it_is`]). While the server's Direct Play is on, so
/// does one it would convert whose video and sound this TV's report (`caps`, the one a file's
/// direct play goes by) plays; with no report, or from an older server (no `direct_play`), only
/// the first holds.
pub fn plays_as_sent(plan: &LivePlan, caps: Option<&ClientCaps>) -> bool {
    if sent_as_it_is(plan) {
        return true;
    }
    let Some(caps) = caps.filter(|_| plan.direct_play == Some(true)) else { return false };
    let video = as_is(&plan.video) || decodes_video(plan, caps);
    let audio = as_is(&plan.audio) || plays_audio(plan, caps);
    video && audio && !(plan.video == "none" && plan.audio == "none")
}

/// Why a channel isn't playing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TroubleKind {
    Busy,
    Unavailable,
    Stopped,
    Missing,
    Older,
    Away,
    Other,
}

/// Why a channel isn't playing, in words for the person; `can_retry` offers Try again (Watch again
/// when [`TroubleKind::Stopped`]: watching ended on the server, its connection taken over or stopped
/// there).
#[derive(Clone, Debug, PartialEq)]
pub struct Trouble {
    pub kind: TroubleKind,
    pub title: String,
    pub message: String,
    pub can_retry: bool,
    /// All connections in use: what holds each, and whether it may be taken over.
    pub connections: Vec<BusyConnection>,
}

impl Trouble {
    fn new(kind: TroubleKind, title: impl Into<String>, message: impl Into<String>, can_retry: bool) -> Trouble {
        Trouble { kind, title: title.into(), message: message.into(), can_retry, connections: Vec::new() }
    }
}

/// Who holds a connection: "You, on another device", their names, or how many.
pub fn holders(c: &BusyConnection) -> String {
    if c.yours {
        return t!("livetv.player.heldByYou").into();
    }
    if !c.watchers.is_empty() {
        return c.watchers.join(", ");
    }
    if c.people == 1 { t!("livetv.player.someoneElse").into() } else { t!("livetv.player.people", count = c.people) }
}

/// Watching ended on the server: its connection taken over, stopped there, or let go after the TV
/// asked for nothing for a while. The server's own words (`why`), as every app says them; watching
/// again may well work.
pub fn stopped_trouble(why: Option<&str>) -> Trouble {
    let why = why.map(str::trim).filter(|w| !w.is_empty());
    Trouble::new(
        TroubleKind::Stopped,
        t!("livetv.trouble.stopped"),
        why.map_or_else(|| t!("livetv.trouble.stoppedBody").to_string(), sentence),
        true,
    )
}

/// The session stopped from elsewhere (`session_stopped`): who stopped it, and their message on a
/// line of its own when given, as a video's and Android's.
pub fn stopped_sentence(message: Option<&str>, by_admin: bool) -> String {
    let who = if by_admin { t!("livetv.trouble.stoppedByAdmin") } else { t!("livetv.trouble.stoppedElsewhere") };
    match message.map(str::trim).filter(|m| !m.is_empty()) {
        Some(m) => format!("{who}\n{m}"),
        None => who.to_string(),
    }
}

/// What a `410` says of why watching ended: the server's words, if it gave any.
fn stopped_note(e: &ApiError) -> Option<String> {
    match e {
        ApiError::SessionStopped { message } => message.clone(),
        _ => None,
    }
}

/// The server's answer to watching `channel` as a trouble. 502–504 read as
/// the channel not playing, as on Android; out of reach, the TV app's words.
pub fn trouble_of(e: &ApiError, channel: &str) -> Trouble {
    let message = sentence(&e.to_string());
    if e.is(409) {
        let mut t = Trouble::new(TroubleKind::Busy, t!("livetv.trouble.busy"), message, true);
        t.connections = e.busy().map(|b| b.connections).unwrap_or_default();
        return t;
    }
    if e.is(410) {
        return stopped_trouble(stopped_note(e).as_deref());
    }
    if e.is(404) {
        return Trouble::new(TroubleKind::Missing, t!("livetv.trouble.missing"), message, false);
    }
    if e.is(403) {
        return Trouble::new(TroubleKind::Other, t!("livetv.trouble.notAllowed"), message, false);
    }
    if matches!(e.status(), Some(502..=504)) {
        return Trouble::new(
            TroubleKind::Unavailable,
            t!("livetv.trouble.unavailable", channel = channel),
            message,
            true,
        );
    }
    if e.unreachable() {
        return Trouble::new(TroubleKind::Away, t!("livetv.trouble.offline"), e.problem(), true);
    }
    Trouble::new(TroubleKind::Other, t!("livetv.trouble.didNotStart", channel = channel), message, true)
}

/// Where watching is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LivePhase {
    Tuning,
    Playing,
    Buffering,
    Error,
}

/// What the live player shows.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveState {
    pub channel: LiveChannel,
    pub phase: LivePhase,
    pub trouble: Option<Trouble>,
    /// The channel's picture (or sound) has started.
    pub shown: bool,
    /// It has sound and no picture (a radio channel).
    pub audio_only: bool,
    /// Counts each channel tuned by the person (not a recovery), so the banner shows anew.
    pub tunes: u32,
}

/// What a failure's question to the server was for.
enum Asked {
    Stall,
    Failure(EngineError, Box<LiveChannel>),
    /// The stream.ts broke off after it played: watched as HLS unless watching ended.
    Direct(Box<LiveChannel>),
    /// The stream.ts stands still while it comes: read again unless watching ended.
    Again,
}

/// What the TV's player opens.
enum Source {
    /// The server's live HLS playlist, its first segments in.
    Hls(String),
    /// The channel's stream.ts, handed on from the TV's loopback address.
    Ts(String),
}

/// A tune's session, from the moment its watch answered. The player takes it then, as Android's
/// does, so the remote's commands and a stop for it are heard while the channel's first segments
/// come; whichever takes it out (the player, or the tune itself once the player has moved on) lets
/// it go.
type Answered = Arc<Mutex<Option<String>>>;

fn take(answered: &Answered) -> Option<String> {
    answered.lock().unwrap_or_else(|e| e.into_inner()).take()
}

/// The TV's report, asked once, on a task: what [`plays_as_sent`] and Playback details go by.
type Caps = Arc<OnceLock<Option<ClientCaps>>>;

/// What a tune found to play, with what the server said the channel is made into when the tune
/// asked.
struct Found {
    source: Source,
    plan: Option<Box<LivePlan>>,
}

enum Done {
    Tuned { g: u64, session: Answered, channel: Option<Box<LiveChannel>>, result: Result<Found, Trouble> },
    Why { g: u64, asked: Box<Asked>, why: Option<String> },
}

/// One live player.
pub struct LiveSession {
    server: Arc<dyn PlayerServer>,
    tasks: Tasks,
    reporter: Reporter,
    engine: Box<dyn PlayerEngine>,
    client_id: String,
    state: LiveState,
    version: u64,
    session: Option<String>,
    /// The session watched last, kept once it ends: the server's word of who stopped it may come
    /// after its `410`.
    watched: Option<String>,
    /// The session of the tune of this generation, until the player takes it.
    answered: (u64, Answered),
    /// The owner goes through channels with channel up and down: the remote's next and previous
    /// do the same.
    steps: bool,
    generation: Arc<AtomicU64>,
    retries: u32,
    now: i64,
    start_deadline: Option<i64>,
    picture_at: Option<i64>,
    retry_at: Option<(i64, LiveChannel)>,
    stall_check_at: Option<i64>,
    stall_give_up_at: Option<i64>,
    closed: bool,
    jobs: Vec<Promise<Done>>,
    /// Hands the channel's stream.ts on to the TV's player; made at the first tune, gone with
    /// the session.
    relay: Option<Arc<StreamRelay>>,
    /// The app can't listen on the loopback address: every channel plays its HLS.
    no_relay: bool,
    /// What plays is the channel's stream.ts: failing, it is watched again as HLS.
    direct: bool,
    /// Reads the stream.ts of the channel tuned, under this TV's stream id: again when it
    /// stands still.
    opener: Option<Opener>,
    /// Watches the stream.ts for standing still, from its first picture until it says so.
    stuck: Option<StuckWatch>,
    /// Whether the stream.ts comes, from what the relay counts of it.
    coming: Coming,
    /// When the stream.ts was last read again, since the channel was tuned.
    read_again_at: Option<i64>,
    /// Asks the TV's report.
    probe: CapsProbe,
    caps: Caps,
    /// What the server last said the channel tuned is made into, for Playback details.
    plan: Option<LivePlan>,
    /// The plan asked again for Playback details, and the tune it was asked in.
    plan_job: Option<(u64, Promise<Option<LivePlan>>)>,
}

/// The TV's report, asked the first time: on a task, as the TV may ask its services.
fn caps_of<'a>(caps: &'a Caps, probe: &CapsProbe) -> Option<&'a ClientCaps> {
    caps.get_or_init(|| probe().map(|report| report.caps)).as_ref()
}

/// Why a session ended, from its plan: the server's words for a `410` (empty when it gave none, not
/// a video's "stopped by the administrator"); none while it goes on (or can't say).
fn why_ended(server: &dyn PlayerServer, session: &str) -> Option<String> {
    match server.live_plan(session) {
        Ok(_) => None,
        Err(ApiError::SessionStopped { message }) => Some(message.unwrap_or_default()),
        Err(e) if e.is(410) => Some(e.to_string()),
        Err(_) => None,
    }
}

impl LiveSession {
    /// A live player for `channel`, on `engine`; nothing plays before [`LiveSession::tune`].
    pub fn new(channel: LiveChannel, env: &PlayerEnv, engine: Box<dyn PlayerEngine>, now: i64) -> LiveSession {
        LiveSession {
            server: env.server.clone(),
            tasks: env.tasks.clone(),
            reporter: env.reporter.clone(),
            engine,
            client_id: env.client_id.clone(),
            state: LiveState {
                channel,
                phase: LivePhase::Tuning,
                trouble: None,
                shown: false,
                audio_only: false,
                tunes: 0,
            },
            version: 0,
            session: None,
            watched: None,
            answered: (0, Answered::default()),
            steps: false,
            generation: Arc::new(AtomicU64::new(0)),
            retries: 0,
            now,
            start_deadline: None,
            picture_at: None,
            retry_at: None,
            stall_check_at: None,
            stall_give_up_at: None,
            closed: false,
            jobs: Vec::new(),
            relay: None,
            no_relay: false,
            direct: false,
            opener: None,
            stuck: None,
            coming: Coming::from(0),
            read_again_at: None,
            probe: env.caps.clone(),
            caps: Caps::default(),
            plan: None,
            plan_job: None,
        }
    }

    /// The owner goes through channels with channel up and down, which the remote's next and
    /// previous ask of it ([`LiveSession::server_event`]); without, the remote can only stop it.
    /// Said in each watch from then on.
    pub fn set_steps(&mut self, steps: bool) {
        self.steps = steps;
    }

    /// Whether what plays is the channel's stream.ts, handed on from the loopback address.
    pub fn direct(&self) -> bool {
        self.direct
    }

    /// What the player shows.
    pub fn state(&self) -> &LiveState {
        &self.state
    }

    /// Changes with every change of [`LiveSession::state`].
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The watching session on the server.
    pub fn session(&self) -> Option<&str> {
        self.session.as_deref()
    }

    /// What the server last said the channel tuned is made into: from the tune when it asked,
    /// then from each [`LiveSession::ask_plan`]; none until it is known.
    pub fn plan(&self) -> Option<&LivePlan> {
        self.plan.as_ref()
    }

    /// This TV's report, once a tune or [`LiveSession::ask_plan`] has asked it; none before, and
    /// where the TV gave none.
    pub fn caps(&self) -> Option<&ClientCaps> {
        self.caps.get().and_then(Option::as_ref)
    }

    /// Asks the plan of the session watched again, for Playback details: what the provider sends
    /// now, and the plan itself where the tune didn't wait for it. One ask at a time. A failure
    /// leaves what is known: a session that ended says so where its stream stops.
    pub fn ask_plan(&mut self) {
        let g = self.current();
        if self.closed || self.plan_job.as_ref().is_some_and(|(asked, _)| *asked == g) {
            return;
        }
        let Some(session) = self.session.clone() else { return };
        let (server, caps, probe) = (self.server.clone(), self.caps.clone(), self.probe.clone());
        let job = self.tasks.spawn(move || {
            caps_of(&caps, &probe);
            server.live_plan(&session).ok().flatten()
        });
        self.plan_job = Some((g, job));
    }

    /// Whether an answer is still to come (tests wait for them).
    pub fn busy(&self) -> bool {
        !self.jobs.is_empty() || self.plan_job.is_some() || !self.reporter.idle()
    }

    /// Bytes of stream.ts that have come for the TV's player since the first channel was tuned
    /// (tests wait for them).
    pub fn stream_came(&self) -> u64 {
        self.relay.as_ref().map_or(0, |r| r.came())
    }

    /// What the engine says of itself, and the TV's network speed, for Playback details.
    pub fn engine_stats(&self) -> EngineStats {
        EngineStats { network_bps: self.engine.network_bps(), ..self.engine.stats() }
    }

    /// The picture's place: none for the full screen, or the channel list's 520×540 at the left.
    pub fn set_video_rect(&mut self, rect: Option<Rect>) {
        self.engine.set_video_rect(rect);
    }

    /// When [`LiveSession::tick`] has something to do next.
    pub fn next_wake(&self) -> Option<i64> {
        [
            self.start_deadline,
            self.picture_at,
            self.retry_at.as_ref().map(|r| r.0),
            self.stall_check_at,
            self.stall_give_up_at,
            self.stuck.as_ref().map(StuckWatch::next),
        ]
        .into_iter()
        .flatten()
        .min()
    }

    fn changed(&mut self) {
        self.version += 1;
    }

    fn next_generation(&mut self) -> u64 {
        self.generation.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn current(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    /// Watches `channel` from its live edge; what was watched before ends.
    pub fn tune(&mut self, channel: LiveChannel) {
        self.retries = 0;
        self.state.tunes += 1;
        self.start(channel, None);
    }

    /// The same channel again (Try again).
    pub fn retry(&mut self) {
        let channel = self.state.channel.clone();
        self.tune(channel);
    }

    /// Every connection in use: the same channel, on the connection channel `channel_id` holds (Watch here instead).
    pub fn take_over(&mut self, channel_id: i64) {
        self.retries = 0;
        let channel = self.state.channel.clone();
        self.start(channel, Some(channel_id));
    }

    /// Someone took over the connection of channel `channel_id` (the server's `live_taken_over`):
    /// the channel playing stops at once, with the server's words, and isn't watched again by
    /// itself. The event names the channel, not the session (a stream's reader never learns its
    /// own), and the server ends every viewer of it, each with an event of its own: the first says
    /// so, the others change nothing, unless a `410` without words said so first.
    pub fn taken_over(&mut self, channel_id: i64, message: Option<&str>) {
        if self.closed || channel_id != self.state.channel.id {
            return;
        }
        self.take_answered();
        let trouble = stopped_trouble(message);
        if self.state.phase == LivePhase::Error {
            let said = message.is_some_and(|m| !m.trim().is_empty());
            // Only a card without the server's words takes these: another stop's words stay.
            if let Some(shown) = self.state.trouble.as_mut()
                && shown.kind == TroubleKind::Stopped
                && said
                && shown.message == stopped_trouble(None).message
            {
                *shown = trouble;
                self.changed();
            }
            return;
        }
        // Its session is gone on the server: nothing to leave.
        self.session = None;
        self.fail(trouble);
    }

    /// What the server says happened elsewhere. The session stopped from another device or by the
    /// server's administrator (`session_stopped`) stops as a connection taken over does, saying
    /// who and why; the card of one whose `410` came first, without words, takes them. A command
    /// of the user's remote for the session watched gives the channel step it asks for (1 up, -1
    /// down), for the owner to take as channel up and down: the new channel's watch tells the
    /// server, and so the remote.
    pub fn server_event(&mut self, event: &ServerEvent) -> Option<i64> {
        if self.closed {
            return None;
        }
        self.take_answered();
        if let Some(command) = Command::of(event) {
            if self.state.phase == LivePhase::Error {
                return None;
            }
            let player = RemotePlayer::live(self.session.as_deref(), self.steps);
            return match remote_action(&player, &command) {
                RemoteAction::Channel(by) => Some(by),
                _ => None,
            };
        }
        let ServerEvent::SessionStopped { session_id, message, by_admin } = event else { return None };
        let error = self.state.phase == LivePhase::Error;
        let own = if error { &self.watched } else { &self.session };
        if own.as_deref() != Some(session_id.as_str()) {
            return None;
        }
        // The message as the server gives it, as a video's and Android's: no full stop added.
        let said = stopped_sentence(message.as_deref(), *by_admin);
        let trouble = Trouble::new(TroubleKind::Stopped, t!("livetv.trouble.stopped"), said, true);
        if error {
            // Only a card without the server's words takes these: another stop's words stay.
            if let Some(shown) = self.state.trouble.as_mut()
                && shown.kind == TroubleKind::Stopped
                && shown.message == stopped_trouble(None).message
            {
                *shown = trouble;
                self.changed();
            }
            return None;
        }
        // Its session is gone on the server: nothing to leave.
        self.session = None;
        self.fail(trouble);
        None
    }

    fn start(&mut self, channel: LiveChannel, take_over: Option<i64>) {
        self.start_with(channel, take_over, true);
    }

    /// The relay, made the first time; none where the app can't listen on the loopback address.
    fn relay(&mut self) -> Option<Arc<StreamRelay>> {
        if self.relay.is_none() && !self.no_relay {
            match StreamRelay::new() {
                Ok(r) => self.relay = Some(Arc::new(r)),
                Err(e) => {
                    // LG documents nothing on a native app listening on 127.0.0.1 (its Developer
                    // Mode jail included): where it may not, every channel plays its HLS.
                    log::info!("live: no loopback address to hand stream.ts on from ({e}): channels play their HLS");
                    self.no_relay = true;
                }
            }
        }
        self.relay.clone()
    }

    fn stop_stream(&mut self) {
        self.direct = false;
        self.stuck = None;
        if let Some(r) = &self.relay {
            r.stop();
        }
    }

    /// `allow_direct`: the channel's stream.ts may play (not after it failed).
    fn start_with(&mut self, channel: LiveChannel, take_over: Option<i64>, allow_direct: bool) {
        if self.closed {
            return;
        }
        let g = self.next_generation();
        self.clear_timers();
        self.jobs.clear();
        self.engine.unload();
        self.stop_stream();
        self.leave();
        // As Android's: stream.ts is read under an id of its own (so it doesn't end the HLS
        // session the plan is asked of), and played once the plan says this TV plays the channel
        // as it is. Asked for with the channel, as the server tunes for whichever comes first;
        // after it when a connection is taken over, as only the watch takes it.
        let relay = if allow_direct { self.relay() } else { None };
        let opener: Opener = {
            let (server, id, client) = (self.server.clone(), channel.id, format!("{}{STREAM_CLIENT}", self.client_id));
            Arc::new(move |cancel: &Cancel| server.live_stream(id, &client, cancel.clone()))
        };
        self.opener = Some(opener.clone());
        self.read_again_at = None;
        // Of the channel before, or of this one before it was watched anew.
        self.plan = None;
        let mut stream = match (&relay, take_over) {
            (Some(r), None) => r.serve(opener.clone()),
            _ => None,
        };
        self.state.channel = channel.clone();
        self.state.phase = LivePhase::Tuning;
        self.state.trouble = None;
        self.state.shown = false;
        self.state.audio_only = false;
        self.changed();
        let (server, reporter, generation) = (self.server.clone(), self.reporter.clone(), self.generation.clone());
        let (caps, probe) = (self.caps.clone(), self.probe.clone());
        let answered = Answered::default();
        self.answered = (g, answered.clone());
        let req = WatchRequest {
            client_id: Some(self.client_id.clone()),
            keyed_urls: true,
            take_over,
            controls: live_controls(self.steps),
            device_kind: Some(DeviceKind::Tv),
        };
        let job = self.tasks.spawn(move || {
            // The last session goes first: this TV holds one connection.
            reporter.flush(Duration::from_millis(LEAVE_WAIT_MS));
            // Not canceled with the player: a session the server made is always let go.
            let watching = Cancel::new().scope(|| server.watch_channel(channel.id, &req));
            let session = answered.clone();
            let watching = match watching {
                Ok(w) => w,
                Err(e) => return Done::Tuned { g, session, channel: None, result: Err(trouble_of(&e, &channel.name)) },
            };
            if generation.load(Ordering::SeqCst) != g {
                let (server, id) = (server.clone(), watching.session_id.clone());
                reporter.send(move || {
                    let _ = server.leave_channel(&id);
                });
                return Done::Tuned { g, session, channel: None, result: Err(trouble_of(&cancelled(), &channel.name)) };
            }
            // The player's from now on: the server lists it, and the remote may already act on it.
            *answered.lock().unwrap_or_else(|e| e.into_inner()) = Some(watching.session_id.clone());
            let named = watching.channel.clone().map(Box::new);
            let current = || generation.load(Ordering::SeqCst) == g;
            // A player closed, hidden or tuned elsewhere since the watch drops this job unanswered,
            // so the job lets its session go itself, unless the player took it (and so let it go).
            let handed_on = |done: Done| {
                if !current()
                    && let Some(session) = take(&answered)
                {
                    let server = server.clone();
                    reporter.send(move || {
                        let _ = server.leave_channel(&session);
                    });
                }
                done
            };
            // An older server's address needs the token, which the TV's player can't send.
            if !watching.url.contains("/api/live/media/") {
                let older = Trouble::new(
                    TroubleKind::Older,
                    t!("livetv.trouble.olderTitle"),
                    t!("livetv.trouble.olderBody"),
                    false,
                );
                return handed_on(Done::Tuned { g, session, channel: named, result: Err(older) });
            }
            let playlist = server.url(&watching.url);
            if take_over.is_some() {
                // Only while this is still the tune: a tune or a stop since has the relay to itself.
                stream = relay.as_ref().and_then(|r| r.serve_while(opener, current));
                if !current() {
                    if let Some(s) = &stream {
                        s.end();
                    }
                    let result = Err(trouble_of(&cancelled(), &channel.name));
                    return handed_on(Done::Tuned { g, session, channel: None, result });
                }
            }
            let mut plan = None;
            if let Some(stream) = stream {
                match server.live_plan(&watching.session_id) {
                    // A channel sent as it is plays whatever the TV reports: its report isn't
                    // waited for.
                    Ok(Some(p)) if sent_as_it_is(&p) || plays_as_sent(&p, caps_of(&caps, &probe)) => {
                        log::info!(
                            "live: the channel plays from its stream.ts (video {} {}, audio {} {})",
                            p.video,
                            p.video_codec.as_deref().unwrap_or("?"),
                            p.audio,
                            p.audio_codec.as_deref().unwrap_or("?")
                        );
                        let found = Found { source: Source::Ts(stream.url().to_string()), plan: Some(Box::new(p)) };
                        return handed_on(Done::Tuned { g, session, channel: named, result: Ok(found) });
                    }
                    Ok(Some(p)) => {
                        log::info!(
                            "live: the channel plays its HLS: the server makes it into video {}, audio {}",
                            p.video,
                            p.audio
                        );
                        plan = Some(Box::new(p));
                    }
                    Ok(None) => log::info!("live: the channel plays its HLS: its plan isn't known in time"),
                    Err(e) => log::info!("live: the channel plays its HLS: no plan ({})", e.problem()),
                }
                // As the Samsung app's: the stream.ts read beside it lets the provider's
                // connection go now, not once the HLS has come.
                stream.end();
            }
            // Left since the watch answered: no playlist to wait on.
            if !current() {
                let result = Err(trouble_of(&cancelled(), &channel.name));
                return handed_on(Done::Tuned { g, session, channel: None, result });
            }
            // The channel's first segments, before the player asks: a TV's player gives up sooner than the server takes to tune.
            let result = match server.media(Method::Get, &playlist, Duration::from_millis(PLAYLIST_WAIT_MS)) {
                Ok(_) => Ok(Found { source: Source::Hls(playlist), plan }),
                // Ended before it played (taken over, say): a keyed address only says the session
                // has gone, its own address says why.
                Err(e) if e.is(410) => {
                    let why = why_ended(server.as_ref(), &watching.session_id).or_else(|| stopped_note(&e));
                    Err(stopped_trouble(why.as_deref()))
                }
                Err(e) => Err(trouble_of(&e, &channel.name)),
            };
            handed_on(Done::Tuned { g, session, channel: named, result })
        });
        self.jobs.push(job);
    }

    /// Collects answers and engine events and runs what is due at `now`; true when what shows changed.
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
        if self.direct
            && let Some(r) = &self.relay
        {
            self.coming.saw(self.now, r.came(), r.offering());
        }
        self.timers();
        self.collect();
        self.version != before
    }

    /// The plan asked again for Playback details: kept when it came for the channel still tuned.
    fn collect_plan(&mut self) {
        let Some((g, job)) = self.plan_job.as_mut() else { return };
        if !job.ready() && !job.failed() {
            return;
        }
        let (g, plan) = (*g, job.take().flatten());
        self.plan_job = None;
        if g == self.current()
            && let Some(plan) = plan
            && self.plan.as_ref() != Some(&plan)
        {
            self.plan = Some(plan);
            self.changed();
        }
    }

    fn collect(&mut self) {
        self.take_answered();
        self.collect_plan();
        loop {
            let mut done = Vec::new();
            let mut i = 0;
            while i < self.jobs.len() {
                if self.jobs[i].ready() {
                    if let Some(d) = self.jobs.swap_remove(i).take() {
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

    fn timers(&mut self) {
        let now = self.now;
        if self.start_deadline.is_some_and(|t| now >= t) {
            self.start_deadline = None;
            if !self.state.shown {
                self.on_engine_error(EngineError::Other { detail: t!("livetv.engine.late").into() });
            }
        }
        if self.picture_at.is_some_and(|t| now >= t) {
            self.picture_at = None;
            let stats = self.engine.stats();
            self.state.audio_only = stats.width.is_none_or(|w| w == 0);
            self.changed();
        }
        if let Some((at, _)) = &self.retry_at
            && now >= *at
        {
            let (_, channel) = self.retry_at.take().expect("checked");
            self.start(channel, None);
        }
        if self.stall_check_at.is_some_and(|t| now >= t) {
            self.stall_check_at = None;
            // Not of a stream.ts, as Android asks nothing of a stall: the server is asked once
            // it breaks off or stands still.
            if self.state.phase == LivePhase::Buffering
                && !self.direct
                && let Some(session) = self.session.clone()
            {
                let (server, g) = (self.server.clone(), self.current());
                self.jobs.push(self.tasks.spawn(move || Done::Why {
                    g,
                    asked: Box::new(Asked::Stall),
                    why: why_ended(server.as_ref(), &session),
                }));
            }
        }
        if self.stall_give_up_at.is_some_and(|t| now >= t) {
            self.stall_give_up_at = None;
            if self.state.phase == LivePhase::Buffering {
                self.on_engine_error(EngineError::Other { detail: t!("livetv.engine.stalled").into() });
            }
        }
        if self.stuck.as_ref().is_some_and(|w| now >= w.next()) {
            // Where the pipeline last said it is, not the position run on past it: one that
            // stops saying, or says the same again, stands still here, and one that never says
            // isn't judged.
            let position = self.engine.reported_ms();
            let coming = self.coming.now(now);
            // A stall the pipeline says has the 20 s above; the watch holds through it.
            let buffering = self.state.phase == LivePhase::Buffering;
            let stuck = self.stuck.as_mut().and_then(|w| w.look(now, position, coming, buffering));
            if let Some(played) = stuck {
                self.stuck = None;
                self.stands_still(played);
            }
        }
    }

    /// The stream.ts stands still while it goes on coming, with no word from the TV's player
    /// (the web app's `whenStuck`): read again where that may help, its HLS otherwise.
    fn stands_still(&mut self, played: bool) {
        let session =
            self.session.clone().filter(|_| unstick(played, self.read_again_at, self.now) == Unstick::ReadAgain);
        let Some(session) = session else {
            let why = if played { "stands still again, soon after it was read again" } else { "never played" };
            return self.direct_failed(why);
        };
        log::info!("live: the channel's stream.ts stands still while it comes: read again");
        // Waiting, for the person, from now until what is read again plays.
        self.clear_stall();
        if self.state.phase != LivePhase::Buffering {
            self.state.phase = LivePhase::Buffering;
            self.changed();
        }
        // The plan first: asking marks the watch session seen, so it stays while no stream is
        // read for it, and one that ended (stopped from elsewhere) says why instead.
        let (server, g) = (self.server.clone(), self.current());
        self.jobs.push(self.tasks.spawn(move || Done::Why {
            g,
            asked: Box::new(Asked::Again),
            why: why_ended(server.as_ref(), &session),
        }));
    }

    /// Opens the stream.ts again, on the same session and stream id: the server starts a new
    /// reader at its last keyframe, and the TV's player starts over on it. Waiting shows until
    /// it plays.
    fn read_again(&mut self) {
        self.engine.unload();
        // Served anew: the reading that stood still closes, and no player of it is left.
        let served = match (&self.relay, &self.opener) {
            (Some(relay), Some(opener)) if self.direct => relay.serve(opener.clone()),
            _ => None,
        };
        let Some(stream) = served else { return self.direct_failed("can't be read again") };
        self.clear_timers();
        self.read_again_at = Some(self.now);
        self.stuck = None;
        self.coming = Coming::from(self.stream_came());
        self.state.shown = false;
        self.state.phase = LivePhase::Buffering;
        self.changed();
        let mut req = LoadRequest::new(stream.url().to_string(), MediaKind::LiveTs);
        req.autoplay = true;
        self.engine.load(req);
        self.start_deadline = Some(self.now + DIRECT_WAIT_MS);
    }

    fn on_done(&mut self, done: Done) {
        match done {
            Done::Tuned { g, session, channel, result } => {
                let session = take(&session);
                if g != self.current() || self.closed {
                    if let Some(s) = session {
                        self.leave_session(s);
                    }
                    return;
                }
                if let Some(s) = session {
                    self.watched = Some(s.clone());
                    self.session = Some(s);
                }
                if let Some(c) = channel {
                    self.state.channel = *c;
                    self.changed();
                }
                let result = result.map(|found| {
                    self.plan = found.plan.map(|plan| *plan);
                    found.source
                });
                match result {
                    Ok(Source::Hls(url)) => {
                        // Made into something else, or not known in time: the stream.ts read
                        // beside it goes.
                        self.stop_stream();
                        let mut req = LoadRequest::new(url, MediaKind::LiveHls);
                        req.autoplay = true;
                        self.engine.load(req);
                        self.start_deadline = Some(self.now + START_WAIT_MS);
                    }
                    Ok(Source::Ts(url)) => {
                        // LG's pipeline takes no buffering from an app (neither its mediaOption
                        // reference nor uMediaServer's lists one), so the stream starts as the
                        // TV's own player buffers it, not at the Samsung app's 0.5 s.
                        self.direct = true;
                        self.coming = Coming::from(self.stream_came());
                        let mut req = LoadRequest::new(url, MediaKind::LiveTs);
                        req.autoplay = true;
                        self.engine.load(req);
                        self.start_deadline = Some(self.now + DIRECT_WAIT_MS);
                    }
                    Err(trouble) => {
                        // Ended on the server already: nothing to leave.
                        if trouble.kind == TroubleKind::Stopped {
                            self.session = None;
                        }
                        self.fail(trouble);
                    }
                }
            }
            Done::Why { g, asked, why } => {
                if g != self.current() || self.closed {
                    return;
                }
                match *asked {
                    Asked::Stall => {
                        if let Some(why) = why.filter(|_| self.state.phase == LivePhase::Buffering) {
                            self.session = None;
                            self.fail(stopped_trouble(Some(&why)));
                        }
                    }
                    Asked::Failure(e, channel) => {
                        let stopped = matches!(e, EngineError::Stopped { .. } | EngineError::Http { status: 410, .. });
                        if why.is_some() || stopped {
                            // Its session is gone on the server: nothing to leave.
                            if why.is_some() {
                                self.session = None;
                            }
                            let said = match &e {
                                EngineError::Stopped { message: Some(m) } => Some(m.clone()),
                                _ => None,
                            };
                            let why = why.filter(|w| !w.is_empty()).or(said);
                            self.fail(stopped_trouble(why.as_deref()));
                            return;
                        }
                        self.after_failure(e, *channel);
                    }
                    Asked::Direct(channel) => match why {
                        Some(why) => {
                            // Already gone at the server: nothing to leave.
                            self.session = None;
                            self.fail(stopped_trouble(Some(&why)));
                        }
                        None => self.start_with(*channel, None, false),
                    },
                    Asked::Again => match why {
                        Some(why) => {
                            // Already gone at the server: nothing to leave.
                            self.session = None;
                            self.fail(stopped_trouble(Some(&why)));
                        }
                        None => self.read_again(),
                    },
                }
            }
        }
    }

    fn on_engine(&mut self, event: EngineEvent) {
        match event {
            EngineEvent::FirstPicture => self.on_first_frame(),
            EngineEvent::Playing | EngineEvent::Waiting(false) => {
                if matches!(self.state.phase, LivePhase::Error | LivePhase::Tuning) {
                    return;
                }
                self.clear_stall();
                if self.state.phase != LivePhase::Playing {
                    self.state.phase = LivePhase::Playing;
                    self.changed();
                }
            }
            EngineEvent::Waiting(true) => {
                if matches!(self.state.phase, LivePhase::Error | LivePhase::Tuning) {
                    return;
                }
                if self.state.phase != LivePhase::Buffering {
                    self.state.phase = LivePhase::Buffering;
                    self.changed();
                }
                self.watch_stall();
            }
            // A stream.ts that ends before it plays: its HLS now, not after the wait.
            EngineEvent::Ended if self.direct && !self.state.shown => {
                self.on_engine_error(EngineError::Other { detail: t!("livetv.engine.directEnded").into() });
            }
            EngineEvent::Ended => {
                if !matches!(self.state.phase, LivePhase::Error | LivePhase::Tuning) {
                    self.on_engine_error(EngineError::Other { detail: t!("livetv.engine.ended").into() });
                }
            }
            EngineEvent::Error { code, message, .. } => self.on_engine_error(engine_error(code, &message, true)),
            EngineEvent::Paused
            | EngineEvent::Loaded { .. }
            | EngineEvent::AudioTracks(_)
            | EngineEvent::VideoInfo { .. }
            | EngineEvent::Buffered { .. } => {}
        }
    }

    /// A stall that lasts: the server says whether watching ended (and why); one that goes on is a failure.
    fn watch_stall(&mut self) {
        if self.stall_check_at.is_some() || self.stall_give_up_at.is_some() {
            return;
        }
        self.stall_check_at = Some(self.now + STALL_CHECK_MS);
        self.stall_give_up_at = Some(self.now + STALL_GIVE_UP_MS);
    }

    fn clear_stall(&mut self) {
        self.stall_check_at = None;
        self.stall_give_up_at = None;
    }

    fn on_first_frame(&mut self) {
        if self.state.phase == LivePhase::Error {
            return;
        }
        self.start_deadline = None;
        self.state.shown = true;
        self.state.phase = LivePhase::Playing;
        self.picture_at = Some(self.now + PICTURE_CHECK_MS);
        // From its first picture: until then the stream has its 10 s to start.
        self.stuck = self.direct.then(|| StuckWatch::new(self.now, self.engine.reported_ms()));
        self.changed();
    }

    /// The TV's player failed: the server is asked first whether watching
    /// ended there (someone took the connection over, or it was stopped),
    /// which the player can't say, then it is watched anew, twice, the first
    /// time after a second: a take-over's event, on its way as the stream
    /// breaks off, stops it before it asks for a connection that is gone.
    fn on_engine_error(&mut self, e: EngineError) {
        // Stopped already (watching ended on the server, or it failed): the player's last words
        // change nothing.
        if self.closed || self.state.phase == LivePhase::Error {
            return;
        }
        if self.direct {
            return self.direct_failed(if self.state.shown { "broke off" } else { "didn't play" });
        }
        let channel = self.state.channel.clone();
        let g = self.next_generation();
        self.clear_timers();
        self.jobs.clear();
        self.engine.unload();
        self.state.phase = LivePhase::Tuning;
        self.changed();
        let server = self.server.clone();
        let session = self.session.clone();
        self.jobs.push(self.tasks.spawn(move || {
            let why = session.and_then(|s| why_ended(server.as_ref(), &s));
            Done::Why { g, asked: Box::new(Asked::Failure(e, Box::new(channel))), why }
        }));
    }

    /// The stream.ts failed or ended (Android's `onPlayerError` and `brokeOff`). Before it played,
    /// the channel is watched again as HLS at once. While it played, the server is asked first
    /// whether watching ended there (its connection taken over, the channel stopped); if not, it
    /// broke off, and is watched again as HLS. So is one that stands still where reading it
    /// again wouldn't help. `what`: what it did, for the log.
    fn direct_failed(&mut self, what: &str) {
        let played = self.state.shown;
        self.stop_stream();
        log::info!("live: the channel's stream.ts {what}: its HLS now");
        let channel = self.state.channel.clone();
        if !played {
            return self.start_with(channel, None, false);
        }
        let g = self.next_generation();
        self.clear_timers();
        self.jobs.clear();
        self.engine.unload();
        self.state.phase = LivePhase::Tuning;
        self.changed();
        let server = self.server.clone();
        let session = self.session.clone();
        self.jobs.push(self.tasks.spawn(move || {
            let why = session.and_then(|s| why_ended(server.as_ref(), &s));
            Done::Why { g, asked: Box::new(Asked::Direct(Box::new(channel))), why }
        }));
    }

    fn after_failure(&mut self, e: EngineError, channel: LiveChannel) {
        if self.retries < RETRIES {
            // Watched anew: the server may have let the channel go, or the network blinked.
            self.retries += 1;
            self.next_generation();
            self.clear_timers();
            self.engine.unload();
            self.state.phase = LivePhase::Tuning;
            self.changed();
            let wait = if self.retries == 1 { 1_000 } else { 3_000 };
            self.retry_at = Some((self.now + wait, channel));
            return;
        }
        let detail = match &e {
            EngineError::Unsupported { detail, .. }
            | EngineError::Http { detail, .. }
            | EngineError::Playlist { detail }
            | EngineError::Network { detail }
            | EngineError::Other { detail } => detail.clone(),
            EngineError::Stopped { message } => message.clone().unwrap_or_default(),
        };
        let message = if detail.is_empty() { String::new() } else { t!("livetv.engine.said", detail = detail) };
        let title = t!("livetv.trouble.channelStopped", channel = channel.name);
        self.fail(Trouble::new(TroubleKind::Other, title, message, true));
    }

    fn fail(&mut self, trouble: Trouble) {
        self.next_generation();
        self.clear_timers();
        self.engine.unload();
        self.stop_stream();
        self.leave();
        self.state.phase = LivePhase::Error;
        self.state.trouble = Some(trouble);
        self.changed();
    }

    /// The TV hid the app: watching ends on the server, to start again when it shows, unless it had
    /// stopped there (taken over, say).
    pub fn suspend(&mut self) {
        self.next_generation();
        self.jobs.clear();
        self.clear_timers();
        self.engine.unload();
        self.stop_stream();
        self.leave();
    }

    /// The TV shows the app again: the channel from its live edge. One that stopped on the server
    /// keeps its card, and is watched again only with Watch again (as Android's).
    pub fn resume(&mut self) {
        if self.closed {
            return;
        }
        if self.state.phase == LivePhase::Error
            && self.state.trouble.as_ref().is_some_and(|t| t.kind == TroubleKind::Stopped)
        {
            return;
        }
        self.retries = 0;
        let channel = self.state.channel.clone();
        self.start(channel, None);
    }

    /// The player goes: watching ends on the server at once.
    pub fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        self.next_generation();
        self.jobs.clear();
        self.clear_timers();
        self.leave();
        self.engine.unload();
        // Its address and listener go with it.
        self.stop_stream();
        self.relay = None;
    }

    fn leave(&mut self) {
        // The plan asked of the session that goes isn't waited for.
        self.plan_job = None;
        if let Some(s) = self.session.take() {
            self.leave_session(s);
        }
        // A tune's session the player hadn't taken yet: its job may be done, its answer dropped
        // with the jobs, so it goes now, before the next watch, and the job finds nothing to leave.
        if let Some(s) = take(&self.answered.1) {
            self.leave_session(s);
        }
    }

    fn leave_session(&self, session: String) {
        let server = self.server.clone();
        self.reporter.send(move || {
            let _ = server.leave_channel(&session);
        });
    }

    /// The session of the tune under way, as soon as its watch answered.
    fn take_answered(&mut self) {
        if self.answered.0 == self.current()
            && let Some(s) = take(&self.answered.1)
        {
            self.watched = Some(s.clone());
            self.session = Some(s);
        }
    }

    fn clear_timers(&mut self) {
        self.start_deadline = None;
        self.picture_at = None;
        self.retry_at = None;
        self.clear_stall();
    }
}

impl Drop for LiveSession {
    fn drop(&mut self) {
        self.close();
    }
}

/// What a watch let go for a newer one stands for: never shown.
fn cancelled() -> ApiError {
    ApiError::Net(pley_core::NetError::Cancelled)
}

#[cfg(test)]
mod tests {
    use pley_i18n::{Language, with_language};

    use super::*;

    fn refused(status: u16) -> ApiError {
        ApiError::Status { status, code: "x".into(), message: "Upstream said no.".into(), body: None }
    }

    #[test]
    fn troubles_name_the_channel_in_french_and_spanish() {
        with_language(Language::Fr, || {
            assert_eq!(trouble_of(&refused(502), "Voyage News").title, "Voyage News est indisponible");
            let stopped = trouble_of(&ApiError::SessionStopped { message: None }, "Voyage News");
            assert_eq!(stopped.title, "La TV en direct s’est arrêtée");
            assert_eq!(stopped.message, "Cette chaîne n’est plus regardée.");
            assert_eq!(
                stopped_sentence(Some("Bonne nuit"), true),
                "La TV en direct a été arrêtée par l’administrateur du serveur.\nBonne nuit"
            );
        });
        with_language(Language::Es, || {
            assert_eq!(trouble_of(&refused(500), "Voyage News").title, "Voyage News no se inició");
        });
    }

    #[test]
    fn who_holds_a_connection_is_counted_in_the_language_shown() {
        let held = |people: i64| BusyConnection { people, ..BusyConnection::default() };
        with_language(Language::Fr, || {
            assert_eq!(holders(&held(1)), "Quelqu’un d’autre");
            assert_eq!(holders(&held(3)), "3 personnes");
            assert_eq!(holders(&BusyConnection { yours: true, ..held(1) }), "Vous, sur un autre appareil");
        });
        with_language(Language::Es, || assert_eq!(holders(&held(2)), "2 personas"));
    }
}

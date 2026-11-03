//! A live channel playing on the TV's own player (packages/tv/src/live/session.ts,
//! the Android app's `LivePlayback`, the Samsung app's `Live/LivePlayback.cs`):
//! watching it on the server with this TV's id and a keyed playlist address,
//! changing channels, and the server told at once when watching ends. Live is
//! live: no pause, rewind or skip.
//!
//! - As the Samsung app's, the channel's `stream.ts` (MPEG-TS as the provider
//!   sends it) is read at once, under an id of its own, while the server is
//!   asked what the channel is made into ([`plays_as_sent`]): sent as it is,
//!   or with sound the TV decodes itself, it plays from its first keyframe
//!   rather than after the first HLS segments. Its address takes the token,
//!   which the TV's player can't send, so the app reads it and hands it on
//!   from the TV's loopback address ([`StreamRelay`]).
//! - Otherwise, or when it fails, the server's HLS from its live edge, once
//!   its playlist has come: before the stream.ts played at once; while it
//!   played, once the server has said watching didn't end there.
//!
//! The owner calls [`LiveSession::tick`] each frame with the app's clock in ms.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use pley_base::Rect;
use pley_core::ApiError;
use pley_core::http::{Method, sentence};
use pley_core::media::{EngineEvent, EngineStats, LoadRequest, MediaKind, PlayerEngine};
use pley_core::model::{BusyConnection, LiveChannel, LivePlan, WatchRequest};
use pley_core::task::{Cancel, Promise, Tasks};

use crate::controller::engine_error;
use crate::env::PlayerEnv;
use crate::live::relay::{Opener, StreamRelay};
use crate::recovery::EngineError;
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

/// Whether this TV plays a channel's stream.ts as the provider sends it (the Samsung app's
/// `LiveRules.PlaysAsSent`): its picture sent as it is (or none), its sound sent as it is (or
/// none) or in a format the TV decodes itself.
pub fn plays_as_sent(plan: &LivePlan) -> bool {
    let as_is = |s: &str| s == "copy" || s == "none";
    as_is(&plan.video)
        && (as_is(&plan.audio)
            || plan.audio_codec.as_deref().is_some_and(|c| TV_DECODED_AUDIO.contains(&c.to_ascii_lowercase().as_str())))
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

/// Why a channel isn't playing, in words for the person; `can_retry` offers Try again.
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
        return "You, on another device".into();
    }
    if !c.watchers.is_empty() {
        return c.watchers.join(", ");
    }
    if c.people == 1 { "Someone else".into() } else { format!("{} people", c.people) }
}

/// The server's answer to watching `channel` as a trouble. 502–504 read as
/// the channel not playing, as on Android; out of reach, the TV app's words.
pub fn trouble_of(e: &ApiError, channel: &str) -> Trouble {
    let message = sentence(&e.to_string());
    if e.is(409) {
        let mut t = Trouble::new(TroubleKind::Busy, "All connections are in use", message, true);
        t.connections = e.busy().map(|b| b.connections).unwrap_or_default();
        return t;
    }
    // Stopped on the server, or let go after the TV asked for nothing for a while: watching again may well work.
    if e.is(410) {
        return Trouble::new(TroubleKind::Stopped, "Watching stopped", message, true);
    }
    if e.is(404) {
        return Trouble::new(TroubleKind::Missing, "This channel isn't here any more", message, false);
    }
    if e.is(403) {
        return Trouble::new(TroubleKind::Other, "Live TV isn't open to this profile", message, false);
    }
    if matches!(e.status(), Some(502..=504)) {
        return Trouble::new(TroubleKind::Unavailable, format!("{channel} isn't playing"), message, true);
    }
    if e.unreachable() {
        return Trouble::new(TroubleKind::Away, "Can't reach your server", e.problem(), true);
    }
    Trouble::new(TroubleKind::Other, format!("{channel} didn't start"), message, true)
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
}

/// What the TV's player opens.
enum Source {
    /// The server's live HLS playlist, its first segments in.
    Hls(String),
    /// The channel's stream.ts, handed on from the TV's loopback address.
    Ts(String),
}

enum Done {
    Tuned { g: u64, session: Option<String>, channel: Option<Box<LiveChannel>>, result: Result<Source, Trouble> },
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
}

/// Why a session ended, from its plan: the server's words for a `410`; none while it goes on (or can't say).
fn why_ended(server: &dyn PlayerServer, session: &str) -> Option<String> {
    match server.live_plan(session) {
        Ok(_) => None,
        Err(e @ ApiError::SessionStopped { .. }) => Some(e.to_string()),
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
        }
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

    /// Whether an answer is still to come (tests wait for them).
    pub fn busy(&self) -> bool {
        !self.jobs.is_empty() || !self.reporter.idle()
    }

    /// What the engine says of itself, for Info.
    pub fn engine_stats(&self) -> EngineStats {
        self.engine.stats()
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
        let req = WatchRequest { client_id: Some(self.client_id.clone()), keyed_urls: true, take_over };
        let job = self.tasks.spawn(move || {
            // The last session goes first: this TV holds one connection.
            reporter.flush(Duration::from_millis(LEAVE_WAIT_MS));
            // Not cancelled with the player: a session the server made is always let go.
            let watching = Cancel::new().scope(|| server.watch_channel(channel.id, &req));
            let watching = match watching {
                Ok(w) => w,
                Err(e) => return Done::Tuned { g, session: None, channel: None, result: Err(trouble_of(&e, &channel.name)) },
            };
            if generation.load(Ordering::SeqCst) != g {
                let (server, session) = (server.clone(), watching.session_id.clone());
                reporter.send(move || {
                    let _ = server.leave_channel(&session);
                });
                return Done::Tuned { g, session: None, channel: None, result: Err(trouble_of(&cancelled(), &channel.name)) };
            }
            let session = Some(watching.session_id.clone());
            let named = watching.channel.clone().map(Box::new);
            // An older server's address needs the token, which the TV's player can't send.
            if !watching.url.contains("/api/live/media/") {
                let older = Trouble::new(
                    TroubleKind::Older,
                    "Live TV needs a newer Pley on the server",
                    "This server can't send live channels to the TV's own player. Update Pley on the server, then try again.",
                    false,
                );
                return Done::Tuned { g, session, channel: named, result: Err(older) };
            }
            let playlist = server.url(&watching.url);
            if take_over.is_some() {
                // Only while this is still the tune: a tune or a stop since has the relay to itself.
                let current = || generation.load(Ordering::SeqCst) == g;
                stream = relay.as_ref().and_then(|r| r.serve_while(opener, current));
                if !current() {
                    if let Some(s) = &stream {
                        s.end();
                    }
                    // Its session is left as the answer comes.
                    let result = Err(trouble_of(&cancelled(), &channel.name));
                    return Done::Tuned { g, session, channel: None, result };
                }
            }
            if let Some(stream) = stream {
                match server.live_plan(&watching.session_id) {
                    Ok(Some(plan)) if plays_as_sent(&plan) => {
                        log::info!(
                            "live: the channel plays from its stream.ts (video {} {}, audio {} {})",
                            plan.video,
                            plan.video_codec.as_deref().unwrap_or("?"),
                            plan.audio,
                            plan.audio_codec.as_deref().unwrap_or("?")
                        );
                        let url = stream.url().to_string();
                        return Done::Tuned { g, session, channel: named, result: Ok(Source::Ts(url)) };
                    }
                    Ok(Some(plan)) => log::info!(
                        "live: the channel plays its HLS: the server makes it into video {}, audio {}",
                        plan.video,
                        plan.audio
                    ),
                    Ok(None) => log::info!("live: the channel plays its HLS: its plan isn't known in time"),
                    Err(e) => log::info!("live: the channel plays its HLS: no plan ({})", e.problem()),
                }
                // As the Samsung app's: the stream.ts read beside it lets the provider's
                // connection go now, not once the HLS has come.
                stream.end();
            }
            // The channel's first segments, before the player asks: a TV's player gives up sooner than the server takes to tune.
            let result = match server.media(Method::Get, &playlist, Duration::from_millis(PLAYLIST_WAIT_MS)) {
                Ok(_) => Ok(Source::Hls(playlist)),
                Err(e) => Err(trouble_of(&e, &channel.name)),
            };
            Done::Tuned { g, session, channel: named, result }
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
        self.timers();
        self.collect();
        self.version != before
    }

    fn collect(&mut self) {
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
                self.on_engine_error(EngineError::Other { detail: "it didn't start in time".into() });
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
            // Not of a stream.ts, as Android asks nothing of a stall: its HLS session, unread,
            // goes after 45 s, and would say watching ended when it hadn't.
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
                self.on_engine_error(EngineError::Other { detail: "it stalled".into() });
            }
        }
    }

    fn on_done(&mut self, done: Done) {
        match done {
            Done::Tuned { g, session, channel, result } => {
                if g != self.current() || self.closed {
                    if let Some(s) = session {
                        self.leave_session(s);
                    }
                    return;
                }
                self.session = session;
                if let Some(c) = channel {
                    self.state.channel = *c;
                    self.changed();
                }
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
                        let mut req = LoadRequest::new(url, MediaKind::LiveTs);
                        req.autoplay = true;
                        self.engine.load(req);
                        self.start_deadline = Some(self.now + DIRECT_WAIT_MS);
                    }
                    Err(trouble) => self.fail(trouble),
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
                            self.fail(Trouble::new(TroubleKind::Stopped, "Watching stopped", why, true));
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
                                EngineError::Stopped { message: Some(m) } if !m.is_empty() => Some(m.clone()),
                                _ => None,
                            };
                            let message =
                                why.or(said).unwrap_or_else(|| "The channel was stopped on the server.".into());
                            self.fail(Trouble::new(TroubleKind::Stopped, "Watching stopped", message, true));
                            return;
                        }
                        self.after_failure(e, *channel);
                    }
                    Asked::Direct(channel) => match why {
                        Some(why) => {
                            // Already gone at the server: nothing to leave.
                            self.session = None;
                            self.fail(Trouble::new(TroubleKind::Stopped, "Watching stopped", why, true));
                        }
                        None => self.start_with(*channel, None, false),
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
                self.on_engine_error(EngineError::Other { detail: "its stream ended".into() });
            }
            EngineEvent::Ended => {
                if !matches!(self.state.phase, LivePhase::Error | LivePhase::Tuning) {
                    self.on_engine_error(EngineError::Other { detail: "the stream ended".into() });
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
        self.changed();
    }

    /// The TV's player failed: the server is asked first whether watching
    /// ended there (someone took the connection over, or it was stopped),
    /// which the player can't say, then it is watched anew, twice.
    fn on_engine_error(&mut self, e: EngineError) {
        if self.closed {
            return;
        }
        if self.direct {
            return self.direct_failed();
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
    /// broke off, and is watched again as HLS.
    fn direct_failed(&mut self) {
        let played = self.state.shown;
        self.stop_stream();
        log::info!("live: the channel's stream.ts {}: its HLS now", if played { "broke off" } else { "didn't play" });
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
        let message = if detail.is_empty() { String::new() } else { format!("The TV's player said: {detail}") };
        self.fail(Trouble::new(TroubleKind::Other, format!("{} stopped", channel.name), message, true));
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

    /// The TV hid the app: watching ends on the server, to start again when it shows.
    pub fn suspend(&mut self) {
        self.next_generation();
        self.jobs.clear();
        self.clear_timers();
        self.engine.unload();
        self.stop_stream();
        self.leave();
    }

    /// The TV shows the app again: the channel from its live edge.
    pub fn resume(&mut self) {
        if self.closed {
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
        if let Some(s) = self.session.take() {
            self.leave_session(s);
        }
    }

    fn leave_session(&self, session: String) {
        let server = self.server.clone();
        self.reporter.send(move || {
            let _ = server.leave_channel(&session);
        });
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

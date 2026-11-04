//! The music player (packages/tv/src/player/music.ts, the Android app's
//! `MusicPlayback`): one queue of tracks, played from keyed addresses, one
//! server session per track with its own client id (`<install id>-music`, so
//! a video doesn't end the music's session nor the other way round), its
//! progress reported. It plays on behind every page; a video or a live
//! channel starting makes it yield ([`MusicSession::yield_to_video`]).
//!
//! As on Android: a decode error in direct play asks again as a conversion,
//! once; a session gone (404) or a network error starts a new one where it
//! was, up to 3 times; Previous restarts a track past 3 s. Kept from the TV
//! app: a report every 30 s while paused, since LG's keyed addresses die with
//! a session the server stops hearing from. The user's remote, on another of
//! their devices, does what the buttons do ([`MusicSession::obey`]).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use pley_core::ApiError;
use pley_core::media::{EngineEvent, LoadRequest, MediaKind, PlayerEngine};
use pley_core::model::{
    ClientCaps, DeviceKind, ItemSummary, PlaybackInfo, PlaybackMethod, PlaybackMode, PlaybackRequest, ProgressReport,
    ProgressState, ServerEvent,
};
use pley_core::task::{Cancel, Promise};

use crate::env::PlayerEnv;
use crate::remote::{Command, MUSIC_CONTROLS, RemoteAction, RemoteKind, RemotePlayer, remote_action, restarts_track};
use crate::sleep::{SLEEP_CHOICES, SleepChoice, SleepItem, SleepState, SleepTimer};

/// Progress reports while music plays.
pub const REPORT_MS: i64 = 10_000;
/// While paused: the server forgets players it hasn't heard from for 90 s, and their addresses with them.
pub const PAUSED_REPORT_MS: i64 = 30_000;
/// New sessions for a track that keeps failing.
pub const MAX_RETRIES: u32 = 3;
/// A failing track is asked for again this long after.
const RETRY_AFTER_MS: i64 = 1_000;
/// Earlier reports go first, this long at most, so the server ends the last session before this one starts.
const FLUSH_MS: u64 = 3_000;
/// How long a session's state is waited for after the engine fails.
const STATUS_MS: u64 = 20_000;

/// Why the music stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MusicError {
    Offline,
    SignedOut,
    Stopped,
    Unplayable,
}

impl MusicError {
    /// What Now Playing says (the Android app's words).
    pub fn text(self) -> &'static str {
        match self {
            MusicError::Offline => "Can't reach the server.",
            MusicError::SignedOut => "You were signed out: sign in again.",
            MusicError::Stopped => "Playback was stopped by the server.",
            MusicError::Unplayable => "This track can't be played.",
        }
    }

    fn of(e: &ApiError) -> MusicError {
        match e {
            ApiError::SessionStopped { .. } => MusicError::Stopped,
            ApiError::SignedOut => MusicError::SignedOut,
            _ if e.is(401) => MusicError::SignedOut,
            _ if e.unreachable() => MusicError::Offline,
            _ => MusicError::Unplayable,
        }
    }
}

/// What the music player is doing, for the pages that show it.
#[derive(Clone, Debug, PartialEq)]
pub struct NowPlaying {
    pub track: ItemSummary,
    /// The place in play order, and how many there are.
    pub index: usize,
    pub count: usize,
    pub playing: bool,
    pub loading: bool,
    pub ended: bool,
    pub shuffle: bool,
    pub error: Option<MusicError>,
}

/// Fisher–Yates, `first` kept first; `random` gives numbers in [0, 1).
pub fn shuffled<T: Clone>(list: &[T], first: usize, mut random: impl FnMut() -> f64) -> Vec<T> {
    let mut rest: Vec<T> = list.iter().enumerate().filter(|(i, _)| *i != first).map(|(_, t)| t.clone()).collect();
    for i in (1..rest.len()).rev() {
        let j = ((random() * (i + 1) as f64).floor() as usize).min(i);
        rest.swap(i, j);
    }
    if first < list.len() {
        rest.insert(0, list[first].clone());
    }
    rest
}

/// A small random source for shuffling (xorshift, seeded from the clock).
fn clock_random() -> impl FnMut() -> f64 {
    let mut x =
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0x2545_f491, |d| d.as_nanos() as u64)
            | 1;
    move || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
}

enum Done {
    Opened { g: u64, from_ms: i64, result: Result<PlaybackInfo, ApiError> },
    Status { g: u64, session: PlaybackInfo, result: Result<(), ApiError> },
}

/// The app's one music player.
pub struct MusicSession {
    env: PlayerEnv,
    engine: Box<dyn PlayerEngine>,
    queue: Vec<ItemSummary>,
    order: Vec<ItemSummary>,
    index: usize,
    now_playing: Option<NowPlaying>,
    session: Option<PlaybackInfo>,
    generation: Arc<AtomicU64>,
    retries: u32,
    retry_at: Option<(i64, i64, Option<PlaybackMode>)>,
    report_at: Option<i64>,
    last_report_at: i64,
    sleep: SleepTimer,
    /// Where it was when a video took over: the next session starts there.
    yielded_at: Option<i64>,
    duration_ms: Option<u64>,
    buffered_end: Option<i64>,
    /// The track opens to play: the engine hasn't played or paused it yet. The remote counts it
    /// as playing, as Android's `playWhenReady` does.
    opening: bool,
    /// The remote paused the track while it opened: it opens paused.
    opens_paused: bool,
    now: i64,
    version: u64,
    jobs: Vec<Promise<Done>>,
    random: Box<dyn FnMut() -> f64 + Send>,
}

impl MusicSession {
    /// The music player on `engine`.
    pub fn new(env: PlayerEnv, engine: Box<dyn PlayerEngine>) -> MusicSession {
        MusicSession {
            env,
            engine,
            queue: Vec::new(),
            order: Vec::new(),
            index: 0,
            now_playing: None,
            session: None,
            generation: Arc::new(AtomicU64::new(0)),
            retries: 0,
            retry_at: None,
            report_at: None,
            last_report_at: 0,
            sleep: SleepTimer::default(),
            yielded_at: None,
            duration_ms: None,
            buffered_end: None,
            opening: false,
            opens_paused: false,
            now: 0,
            version: 0,
            jobs: Vec::new(),
            random: Box::new(clock_random()),
        }
    }

    /// Shuffles with `random` (numbers in [0, 1)) instead of the clock's (tests).
    pub fn set_random(&mut self, random: impl FnMut() -> f64 + Send + 'static) {
        self.random = Box::new(random);
    }

    /// What plays; none once nothing does (Now Playing leaves the rail).
    pub fn now_playing(&self) -> Option<&NowPlaying> {
        self.now_playing.as_ref()
    }

    /// The sleep timer.
    pub fn sleep(&self) -> SleepState {
        self.sleep.state
    }

    /// The queue in play order.
    pub fn list(&self) -> &[ItemSummary] {
        &self.order
    }

    /// Changes with every change of what shows.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether an answer is still to come (tests wait for them).
    pub fn busy(&self) -> bool {
        !self.jobs.is_empty() || !self.env.reporter.idle()
    }

    /// Where the track is.
    pub fn position_ms(&self) -> i64 {
        if self.now_playing.as_ref().is_some_and(|n| n.ended) {
            return self.duration_ms();
        }
        self.engine.position_ms() as i64
    }

    /// The track's length: the engine's, else the item's.
    pub fn duration_ms(&self) -> i64 {
        match self.duration_ms.filter(|d| *d > 0) {
            Some(d) => d as i64,
            None => self.order.get(self.index).and_then(|t| t.duration_ms).unwrap_or(0),
        }
    }

    /// Where what is loaded ends.
    pub fn buffered_ms(&self) -> i64 {
        self.buffered_end.unwrap_or(0)
    }

    /// When [`MusicSession::tick`] has something to do next.
    pub fn next_wake(&self) -> Option<i64> {
        [self.report_at, self.retry_at.map(|r| r.0), self.sleep.next_due()].into_iter().flatten().min()
    }

    fn changed(&mut self) {
        self.version += 1;
    }

    fn update(&mut self, f: impl FnOnce(&mut NowPlaying)) {
        if let Some(n) = &mut self.now_playing {
            f(n);
            self.version += 1;
        }
    }

    /// Plays `tracks` from `at`; shuffled, `at` first and the rest in a random order.
    pub fn play(&mut self, tracks: Vec<ItemSummary>, at: usize, shuffle: bool) {
        if tracks.is_empty() {
            return;
        }
        self.end_session();
        let start = at.min(tracks.len() - 1);
        self.queue = tracks;
        self.order = if shuffle { shuffled(&self.queue, start, &mut self.random) } else { self.queue.clone() };
        self.index = if shuffle { 0 } else { start };
        self.retries = 0;
        if let Some(n) = &mut self.now_playing {
            n.shuffle = shuffle;
        }
        self.start(0, None, shuffle);
    }

    /// Play or pause; after the end, the last track again; after an error or a video, a new session where it was.
    pub fn toggle(&mut self) {
        let Some(now) = self.now_playing.clone() else { return };
        if now.ended {
            self.start(0, None, now.shuffle);
            return;
        }
        if now.error.is_some() || self.session.is_none() {
            let at = self.yielded_at.unwrap_or(self.engine.position_ms() as i64);
            self.start(at, None, now.shuffle);
            return;
        }
        if now.playing {
            self.engine.pause();
        } else {
            self.engine.play();
        }
    }

    /// Pauses.
    pub fn pause(&mut self) {
        if self.now_playing.as_ref().is_some_and(|n| n.playing) {
            self.engine.pause();
        }
    }

    /// Nothing plays any more: the session ends and the queue goes.
    pub fn stop(&mut self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.engine.pause();
        self.end_session();
        self.engine.unload();
        self.sleep.cancel();
        self.queue.clear();
        self.order.clear();
        self.index = 0;
        self.retries = 0;
        self.retry_at = None;
        self.jobs.clear();
        self.opening = false;
        self.now_playing = None;
        self.changed();
    }

    /// The next track.
    pub fn next(&mut self) {
        if self.index + 1 >= self.order.len() {
            return;
        }
        self.end_session();
        self.index += 1;
        self.retries = 0;
        let shuffle = self.shuffle();
        self.start(0, None, shuffle);
    }

    /// The track before; this one from its start when past 3 s, or the first.
    pub fn previous(&mut self) {
        if restarts_track(self.engine.position_ms() as i64, self.index) {
            self.seek_to(0);
            return;
        }
        self.end_session();
        self.index -= 1;
        self.retries = 0;
        let shuffle = self.shuffle();
        self.start(0, None, shuffle);
    }

    /// The track at `position` of the queue.
    pub fn jump_to(&mut self, position: usize) {
        if position >= self.order.len() {
            return;
        }
        self.end_session();
        self.index = position;
        self.retries = 0;
        let shuffle = self.shuffle();
        self.start(0, None, shuffle);
    }

    /// Seeks: in place in direct play, else a new session there. Whether it moved within the
    /// session playing.
    pub fn seek_to(&mut self, ms: i64) -> bool {
        if self.session.as_ref().is_some_and(|s| s.method == PlaybackMethod::Direct || !s.hls) {
            self.engine.seek(ms.max(0) as u64);
            return true;
        }
        if !self.order.is_empty() {
            self.end_session();
            let shuffle = self.shuffle();
            self.start(ms.max(0), None, shuffle);
        }
        false
    }

    fn shuffle(&self) -> bool {
        self.now_playing.as_ref().is_some_and(|n| n.shuffle)
    }

    /// Shuffle on (the track playing first) or off (the given order, at the same track).
    pub fn set_shuffle(&mut self, on: bool) {
        let Some(current) = self.order.get(self.index).cloned() else { return };
        let at = self.queue.iter().position(|t| t.id == current.id).unwrap_or(0);
        if on {
            self.order = shuffled(&self.queue, at, &mut self.random);
            self.index = 0;
        } else {
            self.order = self.queue.clone();
            self.index = at;
        }
        let index = self.index;
        self.update(|n| {
            n.shuffle = on;
            n.index = index;
        });
    }

    /// The sleep timer steps through its choices: Off, 15 … 90 min, end of this track; during its notice, Keep listening.
    pub fn step_sleep(&mut self) {
        let item = self.sleep_item();
        if self.sleep.state.seconds_left.is_some() {
            self.sleep.extend(self.now, item);
            self.changed();
            return;
        }
        let at = SLEEP_CHOICES.iter().position(|c| *c == self.sleep.state.choice).unwrap_or(0);
        let next = SLEEP_CHOICES[(at + 1) % SLEEP_CHOICES.len()];
        if self.order.is_empty() && next != SleepChoice::Off {
            return;
        }
        self.sleep.set(self.now, next, item);
        self.changed();
    }

    /// A video takes the screen and the TV's media pipeline: the music pauses
    /// and lets go of its session; the queue and the place stay, and Play
    /// starts a new session there.
    pub fn yield_to_video(&mut self) {
        if self.now_playing.is_none() {
            return;
        }
        self.engine.pause();
        let at = self.engine.position_ms();
        self.end_session();
        self.engine.unload();
        self.opening = false;
        self.update(|n| n.playing = false);
        self.yielded_at = Some(at as i64);
    }

    /// What the server says happened elsewhere: a stop of the music's session, or a command of
    /// the user's remote on another device.
    pub fn server_event(&mut self, event: &ServerEvent) {
        if let ServerEvent::SessionStopped { session_id, .. } = event
            && self.session.as_ref().is_some_and(|s| s.session_id == *session_id)
        {
            self.engine.pause();
            self.session = None;
            self.report_at = None;
            self.update(|n| {
                n.playing = false;
                n.error = Some(MusicError::Stopped);
            });
        } else if let Some(command) = Command::of(event) {
            self.obey(&command);
        }
    }

    /// The session the track plays in; none between tracks, after an error or a video took over.
    pub fn session_id(&self) -> Option<&str> {
        self.session.as_ref().map(|s| s.session_id.as_str())
    }

    /// A command of the user's remote, on another device, for the track playing
    /// ([`crate::remote`] has the rule), as the buttons here do it: what changed is reported at
    /// once, so the remote sees it. A pause reports as any pause does, another track with its own
    /// session; a play or pause that finds it so already says it again. What it did.
    pub fn obey(&mut self, command: &Command) -> RemoteAction {
        let Some(now) = self.now_playing.as_ref().filter(|n| n.error.is_none() && !n.ended) else {
            return RemoteAction::None;
        };
        let Some(session) = self.session.as_ref() else { return RemoteAction::None };
        let player = RemotePlayer {
            kind: RemoteKind::Music { index: self.index },
            session: Some(&session.session_id),
            playing: now.playing || (self.opening && !self.opens_paused),
            position_ms: self.engine.position_ms() as i64,
            duration_ms: Some(self.duration_ms()).filter(|d| *d > 0),
            has_next: self.index + 1 < self.order.len(),
        };
        let playing = player.playing;
        let action = remote_action(&player, command);
        let state = if playing { ProgressState::Playing } else { ProgressState::Paused };
        match action {
            RemoteAction::Play => {
                self.opens_paused = false;
                self.engine.play();
                self.report(ProgressState::Playing);
            }
            RemoteAction::Pause if self.opening => {
                // The engine opens it paused, and only then says so: the remote hears it now.
                self.opens_paused = true;
                self.engine.pause();
                self.report(ProgressState::Paused);
            }
            RemoteAction::Pause => self.engine.pause(),
            // Playing or paused already: said again, so a remote a little behind settles.
            RemoteAction::Report => self.report(state),
            RemoteAction::Seek(at) => {
                if self.seek_to(at) {
                    self.report_from(state, at);
                }
            }
            RemoteAction::Next => self.next(),
            RemoteAction::Previous => self.previous(),
            RemoteAction::None | RemoteAction::Channel(_) | RemoteAction::Audio(_) | RemoteAction::Subtitles(_) => {}
        }
        action
    }

    fn sleep_item(&self) -> SleepItem {
        let left = (self.duration_ms() > 0).then(|| (self.duration_ms() - self.engine.position_ms() as i64).max(0));
        SleepItem { key: self.order.get(self.index).map_or(-1, |t| t.id), left_ms: left }
    }

    fn start(&mut self, from_ms: i64, mode: Option<PlaybackMode>, shuffle: bool) {
        let Some(track) = self.order.get(self.index).cloned() else { return };
        let g = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        self.retry_at = None;
        self.yielded_at = None;
        self.duration_ms = None;
        self.buffered_end = None;
        self.opening = true;
        self.opens_paused = false;
        self.now_playing = Some(NowPlaying {
            track: track.clone(),
            index: self.index,
            count: self.order.len(),
            playing: false,
            loading: true,
            ended: false,
            shuffle,
            error: None,
        });
        self.changed();
        let server = self.env.server.clone();
        let reporter = self.env.reporter.clone();
        let caps = self.env.caps.clone();
        let generation = self.generation.clone();
        let req = move |caps: Option<ClientCaps>| PlaybackRequest {
            item_id: track.id,
            start_ms: (from_ms > 0).then_some(from_ms),
            mode: mode.unwrap_or(PlaybackMode::Auto),
            client_id: None,
            keyed_urls: true,
            controls: MUSIC_CONTROLS.to_vec(),
            device_kind: Some(DeviceKind::Tv),
            caps: caps.unwrap_or_else(ClientCaps::empty),
            ..PlaybackRequest::default()
        };
        let client_id = format!("{}-music", self.env.client_id);
        let keyed = self.env.keyed_urls;
        self.jobs.push(self.env.tasks.spawn(move || {
            // Earlier reports first, so the server ends the last session before this one starts.
            reporter.flush(Duration::from_millis(FLUSH_MS));
            let mut body = req(caps().map(|r| r.caps));
            body.client_id = Some(client_id);
            body.keyed_urls = keyed;
            // Not cancelled with the player: a session the server made is always let go.
            let result = Cancel::new().scope(|| server.play(&body));
            if let Ok(info) = &result
                && generation.load(Ordering::SeqCst) != g
            {
                let (server, session) = (server.clone(), info.session_id.clone());
                reporter.send(move || {
                    let _ = server.stop_session(&session);
                });
            }
            Done::Opened { g, from_ms, result }
        }));
    }

    /// Collects answers and engine events and runs what is due at `now`; true when what shows changed.
    pub fn tick(&mut self, now: i64) -> bool {
        let before = self.version;
        self.now = now.max(self.now);
        self.collect();
        for event in self.engine.events() {
            self.on_engine(event);
        }
        if let Some((at, from, mode)) = self.retry_at
            && self.now >= at
        {
            self.retry_at = None;
            let shuffle = self.shuffle();
            self.start(from, mode, shuffle);
        }
        if self.report_at.is_some_and(|t| self.now >= t) {
            self.report_at = Some(self.now + REPORT_MS);
            let playing = self.now_playing.as_ref().is_some_and(|n| n.playing);
            if playing {
                self.report(ProgressState::Playing);
            } else if self.now - self.last_report_at >= PAUSED_REPORT_MS {
                self.report(ProgressState::Paused);
            }
        }
        if self.sleep.next_due().is_some_and(|t| self.now >= t) {
            let item = self.sleep_item();
            if self.sleep.tick(self.now, item) {
                self.engine.pause();
            }
            self.changed();
        }
        self.collect();
        self.version != before
    }

    fn collect(&mut self) {
        let mut i = 0;
        while i < self.jobs.len() {
            if self.jobs[i].ready() {
                if let Some(d) = self.jobs.swap_remove(i).take() {
                    self.on_done(d);
                }
            } else if self.jobs[i].failed() {
                self.jobs.swap_remove(i);
            } else {
                i += 1;
            }
        }
    }

    fn on_done(&mut self, done: Done) {
        let current = |g: u64| g == self.generation.load(Ordering::SeqCst);
        match done {
            Done::Opened { g, from_ms, result } => {
                if !current(g) {
                    return;
                }
                match result {
                    Ok(info) => {
                        let mut req = LoadRequest::new(
                            self.env.server.url(&info.url),
                            if info.hls { MediaKind::Hls } else { MediaKind::File },
                        );
                        // A direct play resumed starts where it was; a stream starts there on the server.
                        req.start_ms = if info.hls { 0 } else { from_ms.max(0) as u64 };
                        req.duration_hint_ms = (info.duration_ms > 0).then_some(info.duration_ms as u64);
                        req.autoplay = true;
                        self.session = Some(info);
                        self.engine.load(req);
                        self.report_at = Some(self.now + REPORT_MS);
                    }
                    Err(e) => self.update(|n| {
                        n.loading = false;
                        n.playing = false;
                        n.error = Some(MusicError::of(&e));
                    }),
                }
            }
            Done::Status { g, session, result } => {
                if !current(g) {
                    return;
                }
                match result {
                    Ok(()) => self.track_failed(session),
                    Err(ApiError::SessionStopped { .. }) => self.stopped_by_server(&session.session_id),
                    Err(e) if e.is(404) => {
                        self.session = None;
                        let at = self.engine.position_ms() as i64;
                        let mode = (session.method != PlaybackMethod::Direct).then_some(PlaybackMode::Transcode);
                        let shuffle = self.shuffle();
                        self.start(at, mode, shuffle);
                    }
                    Err(e) if e.unreachable() => self.update(|n| {
                        n.playing = false;
                        n.loading = false;
                        n.error = Some(MusicError::Offline);
                    }),
                    Err(_) => self.track_failed(session),
                }
            }
        }
    }

    fn on_engine(&mut self, event: EngineEvent) {
        match event {
            EngineEvent::Playing => {
                self.opening = false;
                self.sleep.resumed();
                self.update(|n| {
                    n.playing = true;
                    n.loading = false;
                });
            }
            EngineEvent::Paused => {
                // A track that opens paused has loaded.
                let opened = std::mem::take(&mut self.opening);
                self.update(|n| {
                    n.playing = false;
                    n.loading &= !opened;
                });
                self.report(ProgressState::Paused);
            }
            EngineEvent::Waiting(waiting) => self.update(|n| n.loading = waiting),
            EngineEvent::Loaded { duration_ms } => self.duration_ms = duration_ms,
            EngineEvent::Buffered { ahead_ms } => {
                self.buffered_end = Some(self.engine.position_ms() as i64 + ahead_ms as i64);
            }
            EngineEvent::Ended => self.track_ended(),
            EngineEvent::Error { .. } => self.failed(),
            EngineEvent::FirstPicture | EngineEvent::AudioTracks(_) | EngineEvent::VideoInfo { .. } => {}
        }
    }

    /// The engine gave up, which it says no more about. The session tells:
    /// gone means a new one where it was; stopped means the server stopped it;
    /// no answer, the server is out of reach; alive, the track is at fault.
    fn failed(&mut self) {
        let Some(session) = self.session.clone() else { return };
        if self.now_playing.is_none() {
            return;
        }
        let g = self.generation.load(Ordering::SeqCst);
        let server = self.env.server.clone();
        self.jobs.push(self.env.tasks.spawn(move || {
            let result = server.session_status(&session.session_id, Duration::from_millis(STATUS_MS)).map(drop);
            Done::Status { g, session, result }
        }));
    }

    fn track_failed(&mut self, session: PlaybackInfo) {
        // A file the TV can't decode: once more as a conversion; other trouble: a few new sessions.
        let at = self.engine.position_ms() as i64;
        if session.method == PlaybackMethod::Direct && self.retries == 0 {
            self.retries += 1;
            self.end_session();
            let shuffle = self.shuffle();
            self.start(at, Some(PlaybackMode::Transcode), shuffle);
            return;
        }
        if self.retries < MAX_RETRIES {
            self.retries += 1;
            self.end_session();
            let mode = (session.method != PlaybackMethod::Direct).then_some(PlaybackMode::Transcode);
            self.retry_at = Some((self.now + RETRY_AFTER_MS, at, mode));
            return;
        }
        self.update(|n| {
            n.playing = false;
            n.loading = false;
            n.error = Some(MusicError::Unplayable);
        });
    }

    fn track_ended(&mut self) {
        let Some(track) = self.order.get(self.index).cloned() else { return };
        // `stopped` at its length, then the session's end (once, as on Android).
        self.end_session();
        self.retries = 0;
        if self.sleep.item_ended(track.id) {
            // The timer ends with this track: the next one waits, paused.
            if self.index + 1 < self.order.len() {
                self.index += 1;
                let (next, index) = (self.order[self.index].clone(), self.index);
                self.update(|n| {
                    n.track = next;
                    n.index = index;
                    n.playing = false;
                    n.loading = false;
                });
                self.session = None;
            }
            return;
        }
        if self.index + 1 < self.order.len() {
            self.index += 1;
            let shuffle = self.shuffle();
            self.start(0, None, shuffle);
        } else {
            self.update(|n| {
                n.playing = false;
                n.ended = true;
            });
        }
    }

    fn report(&mut self, state: ProgressState) {
        self.report_from(state, self.engine.position_ms() as i64);
    }

    /// `position_ms`: where the track went, when the TV's player may not say so yet (just after a seek).
    fn report_from(&mut self, state: ProgressState, position_ms: i64) {
        let (Some(s), Some(track)) = (self.session.as_ref(), self.order.get(self.index)) else { return };
        // A track has no tracks for the remote to pick: its reports name none.
        let report = ProgressReport {
            position_ms,
            duration_ms: Some(self.duration_ms()).filter(|d| *d > 0),
            state,
            session_id: Some(s.session_id.clone()),
            ..ProgressReport::default()
        };
        let (server, item) = (self.env.server.clone(), track.id);
        self.last_report_at = self.now;
        self.env.reporter.send(move || {
            let _ = server.progress(item, &report);
        });
    }

    /// The track's session ends: its last position, then the session itself.
    fn end_session(&mut self) {
        let Some(s) = self.session.clone() else { return };
        self.report(ProgressState::Stopped);
        let server = self.env.server.clone();
        self.env.reporter.send(move || {
            let _ = server.stop_session(&s.session_id);
        });
        self.session = None;
        self.report_at = None;
    }

    fn stopped_by_server(&mut self, session: &str) {
        if self.session.as_ref().is_none_or(|s| s.session_id != session) {
            return;
        }
        self.engine.pause();
        self.session = None;
        self.report_at = None;
        self.update(|n| {
            n.playing = false;
            n.error = Some(MusicError::Stopped);
        });
    }
}

impl Drop for MusicSession {
    fn drop(&mut self) {
        self.end_session();
    }
}

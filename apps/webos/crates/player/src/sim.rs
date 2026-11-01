//! `SimEngine`: the player engine of the simulator and the tests. It shows
//! no picture (the screen draws a dark frame); its clock runs
//! while it plays, and it honours seeks, the rate, buffering and ends, so
//! everything above the engine behaves as on the TV. A [`SimHandle`] lets a
//! test (or the simulator's key script) make it wait, fail, end, drop frames
//! or report its buffer.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use pley_base::Rect;
use pley_core::media::{AudioTrack, EngineEvent, EngineStats, LoadRequest, PlayerEngine};
use pley_core::model::DynamicRange;

/// How long a source takes to open, by default.
pub const SIM_LOAD_MS: i64 = 300;

/// The time a [`SimEngine`] runs on, in ms: the app's clock, or one a test moves.
#[derive(Clone)]
pub struct SimClock(Option<Arc<AtomicI64>>);

impl SimClock {
    /// The app's clock ([`crate::now_ms`]).
    pub fn system() -> SimClock {
        SimClock(None)
    }

    /// A clock at 0 that moves only when told.
    pub fn manual() -> SimClock {
        SimClock(Some(Arc::new(AtomicI64::new(0))))
    }

    /// The time now.
    pub fn now(&self) -> i64 {
        match &self.0 {
            Some(t) => t.load(Ordering::SeqCst),
            None => crate::now_ms(),
        }
    }

    /// Sets a manual clock (no effect on the system's).
    pub fn set(&self, ms: i64) {
        if let Some(t) = &self.0 {
            t.store(ms, Ordering::SeqCst);
        }
    }

    /// Moves a manual clock on.
    pub fn advance(&self, ms: i64) {
        if let Some(t) = &self.0 {
            t.fetch_add(ms, Ordering::SeqCst);
        }
    }
}

struct Sim {
    commands: Vec<String>,
    loads: Vec<LoadRequest>,
    source: Option<LoadRequest>,
    /// When the source opens; none once it has.
    opens_at: Option<i64>,
    open: bool,
    load_ms: i64,
    duration_ms: Option<u64>,
    /// The position at `base_at`, from which it moves at `rate` while playing.
    base_ms: f64,
    base_at: i64,
    rate: f32,
    can_rate: bool,
    playing: bool,
    want_play: bool,
    waiting: bool,
    wait_until: Option<i64>,
    ended: bool,
    events: VecDeque<EngineEvent>,
    tracks: Vec<AudioTrack>,
    switched: Vec<usize>,
    frames: Option<(u64, u64)>,
    rect: Option<Rect>,
}

impl Sim {
    fn position(&self, now: i64) -> f64 {
        if !self.open {
            return self.base_ms;
        }
        let moving = self.playing && !self.waiting && !self.ended;
        let at = if moving { self.base_ms + (now - self.base_at) as f64 * self.rate as f64 } else { self.base_ms };
        match self.duration_ms {
            Some(d) => at.min(d as f64),
            None => at,
        }
    }

    fn rebase(&mut self, now: i64) {
        self.base_ms = self.position(now);
        self.base_at = now;
    }

    fn advance(&mut self, now: i64) {
        if let Some(at) = self.opens_at.filter(|at| now >= *at) {
            self.opens_at = None;
            self.open = true;
            self.base_at = at;
            let duration = self.duration_ms;
            self.events.push_back(EngineEvent::Loaded { duration_ms: duration });
            if !self.tracks.is_empty() {
                self.events.push_back(EngineEvent::AudioTracks(self.tracks.clone()));
            }
            if let Some(v) = self.source.as_ref().and_then(|s| s.video.clone()) {
                self.events.push_back(EngineEvent::VideoInfo {
                    width: v.width,
                    height: v.height,
                    fps: v.frame_rate.map(|f| f as f32),
                    dynamic_range: v.dynamic_range,
                });
            }
            self.events.push_back(EngineEvent::FirstPicture);
            self.playing = self.want_play;
            self.events.push_back(if self.playing { EngineEvent::Playing } else { EngineEvent::Paused });
        }
        if let Some(until) = self.wait_until.filter(|u| now >= *u) {
            self.wait_until = None;
            self.base_at = until;
            self.waiting = false;
            self.events.push_back(EngineEvent::Waiting(false));
        }
        if self.open
            && !self.ended
            && let Some(d) = self.duration_ms
            && self.position(now) >= d as f64
        {
            self.rebase(now);
            self.ended = true;
            self.playing = false;
            self.events.push_back(EngineEvent::Ended);
        }
    }
}

/// A handle on a [`SimEngine`] for whoever drives it from outside.
#[derive(Clone)]
pub struct SimHandle {
    sim: Arc<Mutex<Sim>>,
    clock: SimClock,
}

/// The engine of the simulator and the tests.
pub struct SimEngine {
    sim: Arc<Mutex<Sim>>,
    clock: SimClock,
}

impl SimEngine {
    /// An engine on `clock`, with its handle.
    pub fn new(clock: SimClock) -> (SimEngine, SimHandle) {
        let sim = Arc::new(Mutex::new(Sim {
            commands: Vec::new(),
            loads: Vec::new(),
            source: None,
            opens_at: None,
            open: false,
            load_ms: SIM_LOAD_MS,
            duration_ms: None,
            base_ms: 0.0,
            base_at: 0,
            rate: 1.0,
            can_rate: true,
            playing: false,
            want_play: false,
            waiting: false,
            wait_until: None,
            ended: false,
            events: VecDeque::new(),
            tracks: Vec::new(),
            switched: Vec::new(),
            frames: None,
            rect: None,
        }));
        (SimEngine { sim: sim.clone(), clock: clock.clone() }, SimHandle { sim, clock })
    }

    fn sim(&self) -> MutexGuard<'_, Sim> {
        self.sim.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl PlayerEngine for SimEngine {
    fn load(&mut self, req: LoadRequest) {
        let now = self.clock.now();
        let mut s = self.sim();
        s.commands.push(format!("load {:?} {}", req.kind, req.start_ms));
        s.loads.push(req.clone());
        s.opens_at = Some(now + s.load_ms);
        s.open = false;
        s.base_ms = req.start_ms as f64;
        s.base_at = now;
        s.duration_ms = if req.kind.is_live() { None } else { req.duration_hint_ms };
        s.want_play = req.autoplay;
        s.playing = false;
        s.waiting = false;
        s.wait_until = None;
        s.ended = false;
        s.events.clear();
        s.source = Some(req);
    }

    fn play(&mut self) {
        let now = self.clock.now();
        let mut s = self.sim();
        s.commands.push("play".into());
        s.want_play = true;
        if s.open && !s.playing && !s.ended {
            s.rebase(now);
            s.playing = true;
            s.events.push_back(EngineEvent::Playing);
        }
    }

    fn pause(&mut self) {
        let now = self.clock.now();
        let mut s = self.sim();
        s.commands.push("pause".into());
        s.want_play = false;
        if s.open && s.playing {
            s.rebase(now);
            s.playing = false;
            s.events.push_back(EngineEvent::Paused);
        }
    }

    fn seek(&mut self, position_ms: u64) {
        let now = self.clock.now();
        let mut s = self.sim();
        s.commands.push(format!("seek {position_ms}"));
        s.base_ms = position_ms as f64;
        s.base_at = now;
        if s.ended {
            s.ended = false;
            s.playing = s.want_play;
        }
    }

    fn set_rate(&mut self, rate: f32) -> bool {
        let now = self.clock.now();
        let mut s = self.sim();
        s.commands.push(format!("rate {rate}"));
        if !s.can_rate {
            return false;
        }
        s.rebase(now);
        s.rate = rate;
        true
    }

    fn select_audio(&mut self, ordinal: usize) -> bool {
        let mut s = self.sim();
        s.commands.push(format!("audio {ordinal}"));
        s.switched.push(ordinal);
        s.tracks.is_empty() || s.tracks.iter().any(|t| t.ordinal == ordinal)
    }

    fn set_video_rect(&mut self, rect: Option<Rect>) {
        let mut s = self.sim();
        s.commands.push(format!("rect {rect:?}"));
        s.rect = rect;
    }

    fn unload(&mut self) {
        let now = self.clock.now();
        let mut s = self.sim();
        s.commands.push("unload".into());
        s.rebase(now);
        s.source = None;
        s.opens_at = None;
        s.open = false;
        s.playing = false;
        s.events.clear();
    }

    fn events(&mut self) -> Vec<EngineEvent> {
        let now = self.clock.now();
        let mut s = self.sim();
        s.advance(now);
        s.events.drain(..).collect()
    }

    fn position_ms(&self) -> u64 {
        let now = self.clock.now();
        self.sim().position(now).max(0.0) as u64
    }

    fn stats(&self) -> EngineStats {
        let s = self.sim();
        let video = s.source.as_ref().and_then(|r| r.video.clone());
        EngineStats {
            player: "Simulator".into(),
            width: video.as_ref().map(|v| v.width).filter(|w| *w > 0),
            height: video.as_ref().map(|v| v.height).filter(|h| *h > 0),
            video_codec: video.as_ref().map(|v| v.codec.clone()),
            fps: video.as_ref().and_then(|v| v.frame_rate).map(|f| f as f32),
            dynamic_range: video
                .as_ref()
                .map(|v| v.dynamic_range)
                .filter(|r| *r != DynamicRange::Sdr)
                .map(|r| r.as_str().to_string()),
            dropped_frames: s.frames.map(|f| f.0),
            total_frames: s.frames.map(|f| f.1),
            ..EngineStats::default()
        }
    }
}

impl SimHandle {
    fn sim(&self) -> MutexGuard<'_, Sim> {
        self.sim.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Every command, in words (`load Hls 600000`, `seek 1000`, `audio 1`…).
    pub fn commands(&self) -> Vec<String> {
        self.sim().commands.clone()
    }

    /// Every source loaded, in order.
    pub fn loads(&self) -> Vec<LoadRequest> {
        self.sim().loads.clone()
    }

    /// The audio tracks switched to.
    pub fn switched(&self) -> Vec<usize> {
        self.sim().switched.clone()
    }

    /// Where it plays.
    pub fn position(&self) -> u64 {
        let now = self.clock.now();
        self.sim().position(now).max(0.0) as u64
    }

    /// Whether it plays (not paused, waiting or ended).
    pub fn playing(&self) -> bool {
        let s = self.sim();
        s.open && s.playing && !s.waiting && !s.ended
    }

    /// The picture's place.
    pub fn rect(&self) -> Option<Rect> {
        self.sim().rect
    }

    /// How long a load takes from now on.
    pub fn set_load_ms(&self, ms: i64) {
        self.sim().load_ms = ms.max(0);
    }

    /// The audio tracks it lists when a source opens.
    pub fn set_tracks(&self, tracks: Vec<AudioTrack>) {
        self.sim().tracks = tracks;
    }

    /// Whether it plays at other speeds.
    pub fn set_can_rate(&self, can: bool) {
        self.sim().can_rate = can;
    }

    /// Its dropped and total frames (none: it can't tell, as LG's pipeline).
    pub fn set_frames(&self, frames: Option<(u64, u64)>) {
        self.sim().frames = frames;
    }

    /// Waits for data (`true`) until told otherwise, or plays on (`false`).
    pub fn wait(&self, waiting: bool) {
        let now = self.clock.now();
        let mut s = self.sim();
        if s.waiting == waiting {
            return;
        }
        s.rebase(now);
        s.waiting = waiting;
        s.wait_until = None;
        s.events.push_back(EngineEvent::Waiting(waiting));
    }

    /// Waits for data for `ms`, then plays on.
    pub fn stall(&self, ms: i64) {
        self.wait(true);
        let now = self.clock.now();
        self.sim().wait_until = Some(now + ms);
    }

    /// Says how far ahead of the position it has loaded.
    pub fn buffered(&self, ahead_ms: u64) {
        self.sim().events.push_back(EngineEvent::Buffered { ahead_ms });
    }

    /// Fails with `code` (as `MediaError`: 2 network, 3 decode, 4 not supported;
    /// or an HTTP status) and stands still, as a pipeline that gave up.
    pub fn fail(&self, code: i32, message: &str) {
        let now = self.clock.now();
        let mut s = self.sim();
        let before = !s.open;
        s.rebase(now);
        s.playing = false;
        s.events.push_back(EngineEvent::Error { code, message: message.into(), before_first_picture: before });
    }

    /// Reaches the end now.
    pub fn end(&self) {
        let now = self.clock.now();
        let mut s = self.sim();
        s.rebase(now);
        if let Some(d) = s.duration_ms {
            s.base_ms = d as f64;
        }
        s.ended = true;
        s.playing = false;
        s.events.push_back(EngineEvent::Ended);
    }

    /// Jumps to `ms` as if it had played there (tests).
    pub fn set_position(&self, ms: u64) {
        let now = self.clock.now();
        let mut s = self.sim();
        s.base_ms = ms as f64;
        s.base_at = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_core::media::MediaKind;

    #[test]
    fn opens_plays_seeks_waits_and_ends() {
        let clock = SimClock::manual();
        let (mut e, h) = SimEngine::new(clock.clone());
        h.set_tracks(vec![AudioTrack { ordinal: 0, language: Some("eng".into()), ..AudioTrack::default() }]);
        let mut req = LoadRequest::new("http://x/a.mkv", MediaKind::File);
        req.start_ms = 10_000;
        req.duration_hint_ms = Some(20_000);
        e.load(req);
        assert!(e.events().is_empty());
        clock.advance(SIM_LOAD_MS);
        let first = e.events();
        assert!(matches!(first[0], EngineEvent::Loaded { duration_ms: Some(20_000) }));
        assert!(first.contains(&EngineEvent::FirstPicture));
        assert_eq!(first.last(), Some(&EngineEvent::Playing));
        clock.advance(1_000);
        assert_eq!(e.position_ms(), 11_000);
        e.set_rate(2.0);
        clock.advance(1_000);
        assert_eq!(e.position_ms(), 13_000);
        h.stall(500);
        clock.advance(400);
        assert_eq!(e.position_ms(), 13_000);
        assert_eq!(e.events(), [EngineEvent::Waiting(true)]);
        clock.advance(100);
        assert_eq!(e.events(), [EngineEvent::Waiting(false)]);
        e.seek(19_000);
        clock.advance(1_000);
        assert_eq!(e.events(), [EngineEvent::Ended]);
        assert_eq!(e.position_ms(), 20_000);
    }
}

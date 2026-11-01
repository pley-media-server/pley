//! The engine's state machine, with no input or output of its own: commands
//! and uMediaServer's messages go in, calls to make and the controller's
//! events come out. [`super::UmsEngine`] runs the calls; tests script the
//! messages.
//!
//! The rules (DESIGN.md §5.3, RES/plx-media.md §11 and §18.4):
//! - the start position goes in the load, and commands given while loading
//!   wait for `loadCompleted`;
//! - "has this session shown a picture" is its own flag; until it is set any
//!   error is the load's verdict, and no picture within 20 s fails the load;
//! - after the picture, a decoder taken back by the TV (`600`, "VDEC resource
//!   is released by policy action") parks the engine: the pipeline is
//!   released and the next command rebuilds it where it was;
//! - buffering is reported only while playing and not seeking, since the
//!   controller counts each wait as a playback issue;
//! - the position comes from `currentTime` and runs on between reports, at
//!   most a second ahead of the last one.

use std::time::{Duration, Instant};

use pley_base::Rect;
use pley_core::media::{AudioTrack, EngineEvent, EngineStats, LoadRequest};
use pley_core::model::DynamicRange;
use serde_json::Value;

use super::payload;
use super::plane::{Candidate, Plane, PlaneMemory};
use crate::facts::CodecTable;

/// How long a load may take to show a picture.
pub const LOAD_BUDGET: Duration = Duration::from_secs(20);
/// How long after the first picture a 4.x binding is judged.
pub const PLANE_CHECK_DELAY: Duration = Duration::from_secs(2);
/// How far the position runs on past the pipeline's last report.
pub const MAX_EXTRAPOLATION_MS: u64 = 1000;
/// The pipeline's code for a decoder taken back by the TV.
pub const POLICY_ACTION: i32 = 600;
/// The code of an error the engine finds itself (no picture in time, a lost bus).
pub const ENGINE_ERROR: i32 = -1;
/// The player's name in Playback details.
pub const PLAYER: &str = "webOS media pipeline";

/// The interface plane, in device pixels.
const PLANE_RECT: Rect = Rect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 };

/// uMediaServer's methods the engine calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Method {
    Load,
    Play,
    Pause,
    Seek,
    SetPlayRate,
    SelectTrack,
    SetDisplayWindow,
    Unload,
    GetForegroundAppInfo,
}

impl Method {
    /// Its name on the bus.
    pub fn name(self) -> &'static str {
        match self {
            Method::Load => "load",
            Method::Play => "play",
            Method::Pause => "pause",
            Method::Seek => "seek",
            Method::SetPlayRate => "setPlayRate",
            Method::SelectTrack => "selectTrack",
            Method::SetDisplayWindow => "setDisplayWindow",
            Method::Unload => "unload",
            Method::GetForegroundAppInfo => "getForegroundAppInfo",
        }
    }
}

/// ACB's play states that follow the pipeline's (webOS 4.x).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcbState {
    Playing,
    Paused,
    Unloaded,
}

/// Something for the engine's shell to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Call `luna://com.webos.media/<method>`; the answer comes back as
    /// [`Input::Loaded`] for `load`, [`Input::Answered`] otherwise.
    Call {
        serial: u64,
        method: Method,
        payload: Value,
    },
    /// Subscribe to the pipeline's messages: each comes back as [`Input::Message`].
    Subscribe {
        serial: u64,
        media_id: String,
    },
    Unsubscribe {
        serial: u64,
    },
    /// Place session `serial`'s exported window (device pixels).
    PlaceWindow {
        serial: u64,
        source: Rect,
        dest: Rect,
    },
    DestroyWindow {
        serial: u64,
    },
    /// Bind the picture through ACB (webOS 4.x); the outcome comes back as [`Input::AcbBound`].
    AcbBind {
        serial: u64,
        media_id: String,
        dest: Rect,
        playing: bool,
    },
    AcbPlace {
        dest: Rect,
        full: bool,
    },
    AcbState(AcbState),
    /// Ask whether the picture is on screen; the verdict comes back as [`Input::PlaneShown`].
    CheckPlane {
        serial: u64,
        media_id: String,
    },
    /// Keep what was learned about the 4.x bindings.
    Remember(PlaneMemory),
    /// Run the machine again after this long (its bounded waits).
    WakeAfter(Duration),
}

/// Why a call failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: i32,
    pub message: String,
}

/// What comes back from the bus.
#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    /// `load`'s answer: the pipeline's id.
    Loaded {
        serial: u64,
        answer: Result<String, Failure>,
    },
    /// One message of the subscription.
    Message {
        serial: u64,
        message: Value,
    },
    /// A call other than `load` answered (`error` none) or failed.
    Answered {
        serial: u64,
        method: Method,
        error: Option<Failure>,
    },
    /// The subscription failed.
    Lost {
        serial: u64,
        error: Failure,
    },
    /// Whether the picture is on screen (`None`: the TV wouldn't say).
    PlaneShown {
        serial: u64,
        shown: Option<bool>,
    },
    AcbBound {
        serial: u64,
        ok: bool,
    },
}

/// What the machine is set up with.
#[derive(Clone, Debug)]
pub struct Config {
    /// The app id the pipeline is tied to (`option.appId`).
    pub app_id: String,
    pub plane: PlaneKind,
    /// Whether ACB's library opened (webOS 4.x).
    pub acb_available: bool,
    pub table: Option<CodecTable>,
    /// Device pixels per dp.
    pub scale: f32,
}

/// How this TV binds the picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaneKind {
    /// webOS 5 and later.
    Exported,
    /// webOS 4.x: the candidates.
    Legacy,
}

#[derive(Clone, Copy, Debug)]
struct Clock {
    ms: u64,
    at: Instant,
    running: bool,
}

#[derive(Clone, Debug, Default)]
struct Video {
    width: u32,
    height: u32,
    fps: Option<f32>,
    hdr: Option<String>,
    /// Pixel aspect ratio (width over height).
    par: f32,
    bitrate: Option<u32>,
}

#[derive(Debug)]
struct Session {
    serial: u64,
    req: LoadRequest,
    media_id: Option<String>,
    started: Instant,
    completed: bool,
    shown: bool,
    playing: bool,
    want_play: bool,
    waiting: bool,
    seeking: Option<u64>,
    pending_seek: Option<u64>,
    pending_audio: Option<usize>,
    clock: Clock,
    last_report: Option<u64>,
    duration: Option<u64>,
    sent_duration: Option<Option<u64>>,
    tracks: Vec<AudioTrack>,
    video: Video,
    video_codec: Option<String>,
    container: Option<String>,
    audio_channels: Option<u32>,
    sample_rate: Option<u32>,
    ahead_ms: Option<u64>,
    selected_audio: usize,
    candidate: Option<Candidate>,
    check_at: Option<Instant>,
    acb_bound: bool,
}

/// A session the TV took the decoder from, to rebuild where it was.
#[derive(Clone, Debug)]
struct Parked {
    req: LoadRequest,
    position_ms: u64,
    audio: Option<usize>,
}

/// The state machine.
#[derive(Debug)]
pub struct Machine {
    cfg: Config,
    memory: PlaneMemory,
    serial: u64,
    session: Option<Session>,
    parked: Option<Parked>,
    /// Sessions closed while their `load` was unanswered.
    orphans: Vec<u64>,
    rate: f32,
    rect: Option<Rect>,
    carry_audio: Option<usize>,
    events: Vec<EngineEvent>,
    actions: Vec<Action>,
}

impl Machine {
    /// A machine with what was learned about this firmware's 4.x bindings.
    pub fn new(cfg: Config, memory: PlaneMemory) -> Machine {
        Machine {
            cfg,
            memory,
            serial: 0,
            session: None,
            parked: None,
            orphans: Vec::new(),
            rate: 1.0,
            rect: None,
            carry_audio: None,
            events: Vec::new(),
            actions: Vec::new(),
        }
    }

    /// How the next playback's picture is bound.
    pub fn plane(&self) -> Plane {
        match self.cfg.plane {
            PlaneKind::Exported => Plane::Exported,
            PlaneKind::Legacy => Plane::Legacy(self.memory.choose(self.cfg.acb_available)),
        }
    }

    /// Whether a load wants an exported window made first.
    pub fn wants_window(&self) -> bool {
        self.cfg.plane == PlaneKind::Exported
    }

    /// Whether the TV took the decoder back and the next command rebuilds.
    pub fn is_parked(&self) -> bool {
        self.parked.is_some()
    }

    /// The calls to make, in order.
    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// The controller's events, in order.
    pub fn take_events(&mut self) -> Vec<EngineEvent> {
        std::mem::take(&mut self.events)
    }

    /// Opens `req` (with session `serial`'s exported window, if one was made),
    /// closing what played. Gives the session's number.
    pub fn load(&mut self, req: LoadRequest, window: Option<&str>, now: Instant) -> u64 {
        self.close();
        self.parked = None;
        self.serial += 1;
        let serial = self.serial;
        let candidate = match self.plane() {
            Plane::Legacy(c) => Some(c),
            Plane::Exported => None,
        };
        let payload = payload::load(&req, &self.cfg.app_id, window, self.cfg.table.as_ref());
        self.actions.push(Action::Call { serial, method: Method::Load, payload });
        self.actions.push(Action::WakeAfter(LOAD_BUDGET));
        if window.is_some() {
            let source = req
                .video
                .as_ref()
                .filter(|v| v.width > 0 && v.height > 0)
                .map_or(PLANE_RECT, |v| Rect::new(0.0, 0.0, v.width as f32, v.height as f32));
            let dest = self.dest(None);
            self.actions.push(Action::PlaceWindow { serial, source, dest });
        }
        self.session = Some(Session {
            serial,
            media_id: None,
            started: now,
            completed: false,
            shown: false,
            playing: false,
            want_play: req.autoplay,
            waiting: false,
            seeking: None,
            pending_seek: None,
            pending_audio: self.carry_audio.take(),
            clock: Clock { ms: if req.kind.is_live() { 0 } else { req.start_ms }, at: now, running: false },
            last_report: None,
            duration: if req.kind.is_live() { None } else { req.duration_hint_ms },
            sent_duration: None,
            tracks: Vec::new(),
            video: Video { par: 1.0, ..Video::default() },
            video_codec: None,
            container: req.container.clone(),
            audio_channels: None,
            sample_rate: None,
            ahead_ms: None,
            selected_audio: 0,
            candidate,
            check_at: None,
            acb_bound: false,
            req,
        });
        serial
    }

    /// The request to load again after the TV took the decoder back, from
    /// where it was; `None` when the engine isn't parked.
    pub fn take_parked(&mut self, at_ms: Option<u64>, autoplay: bool) -> Option<LoadRequest> {
        let parked = self.parked.take()?;
        let mut req = parked.req;
        req.start_ms = at_ms.unwrap_or(parked.position_ms);
        req.autoplay = autoplay;
        self.carry_audio = parked.audio;
        Some(req)
    }

    /// Plays, once the pipeline has loaded.
    pub fn play(&mut self) {
        let acb = self.acb_bound();
        let Some(s) = &mut self.session else { return };
        s.want_play = true;
        if let (true, Some(id)) = (s.completed, s.media_id.clone()) {
            let serial = s.serial;
            self.actions.push(Action::Call { serial, method: Method::Play, payload: payload::media(&id) });
            if acb {
                self.actions.push(Action::AcbState(AcbState::Playing));
            }
        }
    }

    /// Pauses where it is, once the pipeline has loaded.
    pub fn pause(&mut self, now: Instant) {
        let acb = self.acb_bound();
        let rate = self.rate;
        let Some(s) = &mut self.session else { return };
        s.want_play = false;
        s.clock = Clock { ms: s.clock.now(now, rate), at: now, running: false };
        if let (true, Some(id)) = (s.completed, s.media_id.clone()) {
            let serial = s.serial;
            self.actions.push(Action::Call { serial, method: Method::Pause, payload: payload::media(&id) });
            if acb {
                self.actions.push(Action::AcbState(AcbState::Paused));
            }
        }
    }

    /// Moves to `position_ms` (within the duration), once the pipeline has loaded.
    pub fn seek(&mut self, position_ms: u64, now: Instant) {
        let Some(s) = &mut self.session else { return };
        let target = s.duration.map_or(position_ms, |d| position_ms.min(d));
        s.clock = Clock { ms: target, at: now, running: false };
        match (s.completed, s.media_id.clone()) {
            (true, Some(id)) => {
                s.seeking = Some(target);
                let serial = s.serial;
                self.actions.push(Action::Call { serial, method: Method::Seek, payload: payload::seek(&id, target) });
            }
            _ => s.pending_seek = Some(target),
        }
    }

    /// Plays at `rate` with sound, for later sources too; false outside LG's
    /// 0.1–2× (sound is kept only up to 2×).
    pub fn set_rate(&mut self, rate: f32) -> bool {
        if !(0.1..=2.0).contains(&rate) {
            return false;
        }
        self.rate = rate;
        if let Some(s) = &self.session
            && let (true, Some(id)) = (s.completed, &s.media_id)
        {
            self.actions.push(Action::Call {
                serial: s.serial,
                method: Method::SetPlayRate,
                payload: payload::rate(id, rate),
            });
        }
        true
    }

    /// Switches to the pipeline's `ordinal`-th audio track (on webOS 4 and
    /// later the pipeline lists the default track first).
    pub fn select_audio(&mut self, ordinal: usize) -> bool {
        if let Some(p) = &mut self.parked {
            p.audio = Some(ordinal);
            return true;
        }
        let Some(s) = &mut self.session else { return false };
        if !s.tracks.is_empty() && ordinal >= s.tracks.len() {
            return false;
        }
        match (s.completed, s.media_id.clone()) {
            (true, Some(id)) => {
                s.selected_audio = ordinal;
                let serial = s.serial;
                let payload = payload::audio_track(&id, ordinal);
                self.actions.push(Action::Call { serial, method: Method::SelectTrack, payload });
            }
            _ => s.pending_audio = Some(ordinal),
        }
        true
    }

    /// Where the picture goes: the whole screen, or a rectangle in dp.
    pub fn set_video_rect(&mut self, rect: Option<Rect>) {
        self.rect = rect;
        self.place();
    }

    /// Closes the source; the machine may load again.
    pub fn unload(&mut self) {
        self.parked = None;
        self.close();
    }

    /// Where playing is, in ms of the file's time.
    pub fn position_ms(&self, now: Instant) -> u64 {
        if let Some(p) = &self.parked {
            return p.position_ms;
        }
        let Some(s) = &self.session else { return 0 };
        let ms = s.clock.now(now, self.rate);
        s.duration.map_or(ms, |d| ms.min(d))
    }

    /// Runs the bounded waits.
    pub fn tick(&mut self, now: Instant) {
        let Some(s) = &mut self.session else { return };
        if !s.shown && now.duration_since(s.started) >= LOAD_BUDGET {
            self.fail(ENGINE_ERROR, "no picture within 20 seconds");
            return;
        }
        if let (Some(due), Some(id)) = (s.check_at, s.media_id.clone())
            && now >= due
        {
            s.check_at = None;
            let serial = s.serial;
            self.actions.push(Action::CheckPlane { serial, media_id: id });
        }
    }

    /// Takes in what came back from the bus.
    pub fn input(&mut self, input: Input, now: Instant) {
        match input {
            Input::Loaded { serial, answer } => self.on_loaded(serial, answer),
            Input::Message { serial, message } if self.current(serial) => self.on_message(&message, now),
            Input::Message { .. } => {}
            Input::Answered { serial, method: Method::Unload, .. } => {
                self.actions.push(Action::DestroyWindow { serial });
            }
            Input::Answered { serial, method, error: Some(e) } if self.current(serial) => {
                self.on_refused(method, e, now)
            }
            Input::Answered { .. } => {}
            Input::Lost { serial, error } if self.current(serial) => {
                let shown = self.session.as_ref().is_some_and(|s| s.shown);
                if !shown {
                    self.fail(error.code, &error.message);
                }
            }
            Input::Lost { .. } => {}
            Input::PlaneShown { serial, shown } if self.current(serial) => self.on_plane_verdict(shown, now),
            Input::PlaneShown { .. } => {}
            Input::AcbBound { serial, ok } if self.current(serial) => {
                if ok {
                    if let Some(s) = &mut self.session {
                        s.acb_bound = true;
                    }
                } else {
                    self.binding_failed(now);
                }
            }
            Input::AcbBound { .. } => {}
        }
    }

    /// What Playback details shows.
    pub fn stats(&self) -> EngineStats {
        let mut stats = EngineStats { player: PLAYER.into(), ..EngineStats::default() };
        let Some(s) = &self.session else { return stats };
        if s.video.width > 0 && s.video.height > 0 {
            stats.width = Some(s.video.width);
            stats.height = Some(s.video.height);
        }
        stats.video_codec = s.video_codec.clone();
        stats.fps = s.video.fps;
        stats.dynamic_range = s.video.hdr.as_deref().map(range_label);
        let track = s.tracks.get(s.selected_audio);
        stats.audio_codec = track.and_then(|t| t.codec.clone());
        stats.audio_channels = s.audio_channels.or_else(|| track.and_then(|t| t.channels));
        stats.bitrate_kbps = s.video.bitrate.map(|b| b / 1000).filter(|k| *k > 0);
        stats.buffered_ahead_ms = s.ahead_ms;
        let plane = match (self.cfg.plane, s.candidate) {
            (PlaneKind::Exported, _) => "exported window",
            (_, Some(Candidate::Acb)) => "ACB",
            (_, Some(Candidate::DisplayWindow)) => "display window",
            _ => "app id",
        };
        stats.extra.push(("Picture".into(), plane.into()));
        if let Some(c) = &s.container {
            stats.extra.push(("Container".into(), c.clone()));
        }
        if let Some(rate) = s.sample_rate {
            stats.extra.push(("Sample rate".into(), format!("{} kHz", rate as f32 / 1000.0)));
        }
        stats
    }

    fn current(&self, serial: u64) -> bool {
        self.session.as_ref().is_some_and(|s| s.serial == serial)
    }

    fn acb_bound(&self) -> bool {
        self.session.as_ref().is_some_and(|s| s.acb_bound)
    }

    /// Where the picture goes on the plane, in device pixels: the rectangle
    /// asked for, narrowed to the picture's shape when it is known.
    fn dest(&self, video: Option<&Video>) -> Rect {
        let area = self.rect.map_or(PLANE_RECT, |r| {
            let k = self.cfg.scale;
            Rect::new(r.x * k, r.y * k, r.w * k, r.h * k)
        });
        match video.filter(|v| v.width > 0 && v.height > 0) {
            Some(v) => fit(area, v.width as f32 * v.par / v.height as f32),
            None => area,
        }
    }

    /// Places the picture where it should be now.
    fn place(&mut self) {
        let Some(s) = &self.session else { return };
        let dest = self.dest(Some(&s.video));
        let serial = s.serial;
        match (self.cfg.plane, s.candidate) {
            (PlaneKind::Exported, _) => {
                let source = if s.video.width > 0 && s.video.height > 0 {
                    Rect::new(0.0, 0.0, s.video.width as f32, s.video.height as f32)
                } else {
                    PLANE_RECT
                };
                self.actions.push(Action::PlaceWindow { serial, source, dest });
            }
            (PlaneKind::Legacy, Some(Candidate::Acb)) if s.acb_bound => {
                self.actions.push(Action::AcbPlace { dest, full: self.rect.is_none() });
            }
            (PlaneKind::Legacy, Some(Candidate::DisplayWindow)) => {
                if let (true, Some(id)) = (s.completed, &s.media_id) {
                    let payload = payload::display_window(id, None, ints(dest));
                    self.actions.push(Action::Call { serial, method: Method::SetDisplayWindow, payload });
                }
            }
            _ => {}
        }
    }

    /// Ends the session: unsubscribes, unloads, and destroys its window once
    /// the pipeline is released.
    fn close(&mut self) {
        let acb = self.acb_bound();
        let Some(s) = self.session.take() else { return };
        self.actions.push(Action::Unsubscribe { serial: s.serial });
        if acb {
            self.actions.push(Action::AcbState(AcbState::Unloaded));
        }
        match s.media_id {
            Some(id) => self.actions.push(Action::Call {
                serial: s.serial,
                method: Method::Unload,
                payload: payload::media(&id),
            }),
            // Its `load` is unanswered: the answer is unloaded when it comes.
            None => self.orphans.push(s.serial),
        }
    }

    fn fail(&mut self, code: i32, message: &str) {
        let before = self.session.as_ref().is_none_or(|s| !s.shown);
        crate::log::line(&format!("ums: playback failed ({code}): {message}"));
        self.events.push(EngineEvent::Error { code, message: message.into(), before_first_picture: before });
        self.close();
    }

    fn on_loaded(&mut self, serial: u64, answer: Result<String, Failure>) {
        if !self.current(serial) {
            if let Some(i) = self.orphans.iter().position(|g| *g == serial) {
                self.orphans.remove(i);
                match answer {
                    Ok(id) => {
                        self.actions.push(Action::Call { serial, method: Method::Unload, payload: payload::media(&id) })
                    }
                    Err(_) => self.actions.push(Action::DestroyWindow { serial }),
                }
            }
            return;
        }
        match answer {
            Ok(id) => {
                let s = self.session.as_mut().expect("the current session");
                s.media_id = Some(id.clone());
                self.actions.push(Action::Subscribe { serial, media_id: id });
            }
            Err(e) => {
                // Nothing was loaded: only the window is left to go.
                self.session = None;
                self.events.push(EngineEvent::Error { code: e.code, message: e.message, before_first_picture: true });
                self.actions.push(Action::DestroyWindow { serial });
            }
        }
    }

    fn on_refused(&mut self, method: Method, e: Failure, now: Instant) {
        crate::log::line(&format!("ums: {} refused ({}): {}", method.name(), e.code, e.message));
        match method {
            Method::Play | Method::Load => {
                let shown = self.session.as_ref().is_some_and(|s| s.shown);
                if !shown {
                    self.fail(e.code, &e.message);
                }
            }
            Method::SetDisplayWindow
                if self.session.as_ref().and_then(|s| s.candidate) == Some(Candidate::DisplayWindow) =>
            {
                self.binding_failed(now);
            }
            _ => {}
        }
    }

    fn on_message(&mut self, message: &Value, now: Instant) {
        let Some((name, body)) = event_of(message) else { return };
        match name {
            "loadCompleted" => self.on_load_completed(now),
            "playing" => self.on_playing(now),
            "paused" => self.on_paused(now),
            "seekDone" => {
                if let Some(s) = &mut self.session {
                    s.seeking = None;
                }
            }
            "endOfStream" => {
                if let Some(s) = &mut self.session {
                    s.playing = false;
                    s.clock = Clock { ms: s.duration.unwrap_or(s.clock.ms), at: now, running: false };
                }
                self.events.push(EngineEvent::Ended);
            }
            "currentTime" => {
                let ms = body.get("currentTime").unwrap_or(body).as_f64().map(|v| v.max(0.0) as u64);
                if let Some(ms) = ms {
                    self.on_time(ms, now);
                }
            }
            "bufferingStart" => {
                if let Some(s) = &mut self.session
                    && s.shown
                    && s.want_play
                    && s.seeking.is_none()
                    && !s.waiting
                {
                    s.waiting = true;
                    s.clock = Clock { ms: s.clock.now(now, self.rate), at: now, running: false };
                    self.events.push(EngineEvent::Waiting(true));
                }
            }
            "bufferingEnd" => {
                if let Some(s) = &mut self.session
                    && s.waiting
                {
                    s.waiting = false;
                    s.clock.at = now;
                    s.clock.running = s.playing;
                    self.events.push(EngineEvent::Waiting(false));
                }
            }
            "bufferRange" => self.on_buffer(body, now),
            "sourceInfo" => self.on_source_info(body),
            "videoInfo" => self.on_video_info(body),
            "audioInfo" => {
                if let Some(s) = &mut self.session {
                    s.audio_channels =
                        body.get("channels").and_then(Value::as_u64).map(|c| c as u32).or(s.audio_channels);
                    s.sample_rate = body.get("sampleRate").and_then(Value::as_u64).map(|r| r as u32).or(s.sample_rate);
                }
            }
            "videoFrame" => {
                if body.get("valid") == Some(&Value::Bool(true)) {
                    self.picture(now);
                }
            }
            "error" => {
                let code = body.get("errorCode").and_then(Value::as_i64).unwrap_or(ENGINE_ERROR as i64) as i32;
                let text = body.get("errorText").and_then(Value::as_str).unwrap_or("the TV's player failed");
                self.on_error(code, text, now);
            }
            _ => {}
        }
    }

    fn on_load_completed(&mut self, now: Instant) {
        let Some(s) = &mut self.session else { return };
        if s.completed {
            return;
        }
        s.completed = true;
        let (serial, id) = (s.serial, s.media_id.clone().unwrap_or_default());
        let duration = if s.req.kind.is_live() { None } else { s.duration };
        s.sent_duration = Some(duration);
        self.events.push(EngineEvent::Loaded { duration_ms: duration });
        let (seek, audio, want_play) = (s.pending_seek.take(), s.pending_audio.take(), s.want_play);
        let candidate = s.candidate;
        if let Some(ms) = seek {
            s.seeking = Some(ms);
            self.actions.push(Action::Call { serial, method: Method::Seek, payload: payload::seek(&id, ms) });
        }
        if let Some(index) = audio {
            s.selected_audio = index;
            self.actions.push(Action::Call {
                serial,
                method: Method::SelectTrack,
                payload: payload::audio_track(&id, index),
            });
        }
        if self.rate != 1.0 {
            self.actions.push(Action::Call {
                serial,
                method: Method::SetPlayRate,
                payload: payload::rate(&id, self.rate),
            });
        }
        if want_play {
            self.actions.push(Action::Call { serial, method: Method::Play, payload: payload::media(&id) });
        }
        if let Some(c) = candidate.filter(|c| c.binds_after_load()) {
            self.bind(c);
        }
        if !want_play {
            // Prepared paused: the pipeline holds its first picture.
            self.picture(now);
            self.events.push(EngineEvent::Paused);
        }
    }

    /// Binds a pipeline already loaded through a 4.x candidate.
    fn bind(&mut self, candidate: Candidate) {
        let dest = self.dest(self.session.as_ref().map(|s| &s.video));
        let Some(s) = &mut self.session else { return };
        let (Some(id), serial) = (s.media_id.clone(), s.serial) else { return };
        s.candidate = Some(candidate);
        match candidate {
            Candidate::Acb => {
                self.actions.push(Action::AcbBind { serial, media_id: id, dest, playing: s.want_play });
            }
            Candidate::DisplayWindow => {
                let payload = payload::display_window(&id, None, ints(dest));
                self.actions.push(Action::Call { serial, method: Method::SetDisplayWindow, payload });
            }
            Candidate::AppIdOnly => {}
        }
    }

    /// The candidate in use didn't bind the picture: remember it and try the
    /// next one that can bind a loaded pipeline.
    fn binding_failed(&mut self, now: Instant) {
        let Some(current) = self.session.as_ref().and_then(|s| s.candidate) else { return };
        if !self.memory.failed.contains(&current) {
            self.memory.failed.push(current);
        }
        if self.memory.winner == Some(current) {
            self.memory.winner = None;
        }
        self.actions.push(Action::Remember(self.memory.clone()));
        let next = self.memory.next_after(Some(current), self.cfg.acb_available).filter(|c| c.binds_after_load());
        if let Some(next) = next {
            crate::log::line(&format!("ums: the picture isn't bound by {}; trying {}", current.name(), next.name()));
            self.bind(next);
            if let Some(s) = &mut self.session
                && s.shown
            {
                s.check_at = Some(now + PLANE_CHECK_DELAY);
                self.actions.push(Action::WakeAfter(PLANE_CHECK_DELAY));
            }
        }
    }

    fn on_plane_verdict(&mut self, shown: Option<bool>, now: Instant) {
        let Some(current) = self.session.as_ref().and_then(|s| s.candidate) else { return };
        match shown {
            Some(true) => {
                if self.memory.winner != Some(current) {
                    crate::log::line(&format!("ums: the picture is bound by {}", current.name()));
                    self.memory.winner = Some(current);
                    self.actions.push(Action::Remember(self.memory.clone()));
                }
            }
            Some(false) => self.binding_failed(now),
            None => {}
        }
    }

    fn on_playing(&mut self, now: Instant) {
        let rate = self.rate;
        let Some(s) = &mut self.session else { return };
        if !s.playing {
            s.playing = true;
            self.events.push(EngineEvent::Playing);
        }
        if s.waiting {
            s.waiting = false;
            self.events.push(EngineEvent::Waiting(false));
        }
        s.clock = Clock { ms: s.clock.now(now, rate), at: now, running: true };
        self.picture(now);
    }

    fn on_paused(&mut self, now: Instant) {
        let rate = self.rate;
        let Some(s) = &mut self.session else { return };
        s.clock = Clock { ms: s.clock.now(now, rate), at: now, running: false };
        if s.playing {
            s.playing = false;
            self.events.push(EngineEvent::Paused);
        }
    }

    /// The first picture is on screen.
    fn picture(&mut self, now: Instant) {
        let undecided = self.cfg.plane == PlaneKind::Legacy && self.memory.undecided(self.cfg.acb_available);
        let Some(s) = &mut self.session else { return };
        if s.shown {
            return;
        }
        s.shown = true;
        self.events.push(EngineEvent::FirstPicture);
        if undecided {
            s.check_at = Some(now + PLANE_CHECK_DELAY);
            self.actions.push(Action::WakeAfter(PLANE_CHECK_DELAY));
        }
    }

    fn on_time(&mut self, ms: u64, now: Instant) {
        let Some(s) = &mut self.session else { return };
        let advanced = s.last_report.is_some_and(|last| ms > last);
        s.last_report = Some(ms);
        // A report after a seek's target means the seek is done.
        if s.seeking.is_some() {
            s.seeking = None;
        }
        s.clock = Clock { ms, at: now, running: s.playing && !s.waiting };
        // A TV that leaves `playing` out still moves its clock.
        if advanced && s.want_play && !s.playing {
            self.on_playing(now);
        }
    }

    fn on_buffer(&mut self, body: &Value, now: Instant) {
        let rate = self.rate;
        let Some(s) = &mut self.session else { return };
        let position = s.clock.now(now, rate);
        let end = body.get("endTime").and_then(Value::as_f64).filter(|e| *e >= 0.0).map(|e| e as u64);
        let ahead = match end {
            Some(end) if end >= position => Some(end - position),
            _ => body.get("remainingTime").and_then(Value::as_f64).filter(|r| *r >= 0.0).map(|r| r as u64),
        };
        if let Some(ahead) = ahead
            && s.ahead_ms != Some(ahead)
        {
            s.ahead_ms = Some(ahead);
            self.events.push(EngineEvent::Buffered { ahead_ms: ahead });
        }
    }

    fn on_source_info(&mut self, body: &Value) {
        let Some(s) = &mut self.session else { return };
        let program = body.pointer("/programInfo/0").unwrap_or(body);
        if let Some(c) = body.get("container").and_then(Value::as_str) {
            s.container = Some(c.to_string());
        }
        let duration = program.get("duration").and_then(Value::as_f64).filter(|d| *d > 0.0).map(|d| d as u64);
        if let Some(tracks) = program.get("audioTrackInfo").and_then(Value::as_array) {
            let list: Vec<AudioTrack> = tracks
                .iter()
                .enumerate()
                .map(|(ordinal, t)| AudioTrack {
                    ordinal,
                    language: t.get("language").and_then(Value::as_str).filter(|l| !l.is_empty()).map(str::to_string),
                    codec: t.get("codec").and_then(Value::as_str).filter(|c| !c.is_empty()).map(str::to_string),
                    channels: t.get("channels").and_then(Value::as_u64).map(|c| c as u32),
                })
                .collect();
            if list != s.tracks {
                s.tracks = list.clone();
                self.events.push(EngineEvent::AudioTracks(list));
            }
        }
        if let Some(v) = program.pointer("/videoTrackInfo/0") {
            s.video_codec = v.get("codec").and_then(Value::as_str).map(str::to_string).or(s.video_codec.take());
        }
        if s.req.kind.is_live() {
            return;
        }
        if let Some(d) = duration {
            s.duration = Some(d);
            // Once `loadCompleted` was told without it, the duration is told again.
            if s.sent_duration.is_some_and(|sent| sent != Some(d)) {
                s.sent_duration = Some(Some(d));
                self.events.push(EngineEvent::Loaded { duration_ms: Some(d) });
            }
        }
    }

    fn on_video_info(&mut self, body: &Value) {
        let Some(s) = &mut self.session else { return };
        let num = |k: &str| body.get(k).and_then(Value::as_f64).filter(|v| *v > 0.0);
        let (Some(width), Some(height)) = (num("width"), num("height")) else { return };
        let fps = match body.get("frameRate") {
            Some(Value::Object(r)) => {
                let (n, d) = (r.get("num").and_then(Value::as_f64), r.get("den").and_then(Value::as_f64));
                n.zip(d).filter(|(_, d)| *d > 0.0).map(|(n, d)| (n / d) as f32)
            }
            Some(v) => v.as_f64().map(|f| f as f32),
            None => None,
        }
        .filter(|f| *f > 0.0);
        let hdr = body.get("hdrType").and_then(Value::as_str).map(str::to_string);
        let par = pixel_aspect(body.get("pixelAspectRatio")).unwrap_or(s.video.par);
        let next = Video {
            width: width as u32,
            height: height as u32,
            fps: fps.or(s.video.fps),
            hdr: hdr.or_else(|| s.video.hdr.clone()),
            par,
            bitrate: num("bitRate").map(|b| b as u32).or(s.video.bitrate),
        };
        let old = &s.video;
        let changed = (next.width, next.height, next.fps, &next.hdr, next.par)
            != (old.width, old.height, old.fps, &old.hdr, old.par);
        let raster = (next.width, next.height, next.par) != (old.width, old.height, old.par);
        s.video = next;
        if changed {
            self.events.push(EngineEvent::VideoInfo {
                width: s.video.width,
                height: s.video.height,
                fps: s.video.fps,
                dynamic_range: s.video.hdr.as_deref().map_or(DynamicRange::Unknown, range_of),
            });
        }
        // A new raster (an HLS rung, the first picture) places the picture again.
        if raster {
            self.place();
        }
    }

    fn on_error(&mut self, code: i32, text: &str, now: Instant) {
        let Some(s) = &self.session else { return };
        if !s.shown {
            self.fail(code, text);
        } else if code == POLICY_ACTION {
            // The TV took the decoder (the screen saver over a paused picture):
            // whatever took it may still hold it, so the rebuild waits for the
            // next command rather than happening now.
            let rate = self.rate;
            let s = self.session.as_ref().expect("a session");
            let parked =
                Parked { req: s.req.clone(), position_ms: s.clock.now(now, rate), audio: Some(s.selected_audio) };
            let was_playing = s.playing;
            crate::log::line(&format!(
                "ums: the TV took the decoder back ({code}): parked at {} ms",
                parked.position_ms
            ));
            self.close();
            self.parked = Some(parked);
            if was_playing {
                self.events.push(EngineEvent::Paused);
            }
        } else {
            crate::log::line(&format!("ums: an error while playing ({code}): {text}"));
            self.events.push(EngineEvent::Error { code, message: text.into(), before_first_picture: false });
        }
    }
}

impl Clock {
    fn now(&self, now: Instant, rate: f32) -> u64 {
        if !self.running {
            return self.ms;
        }
        let elapsed = now.saturating_duration_since(self.at).as_millis() as f64 * f64::from(rate);
        self.ms + (elapsed as u64).min(MAX_EXTRAPOLATION_MS)
    }
}

/// The messages the engine reads; each names its event by its one key.
const EVENTS: [&str; 15] = [
    "loadCompleted",
    "playing",
    "paused",
    "seekDone",
    "endOfStream",
    "currentTime",
    "bufferingStart",
    "bufferingEnd",
    "bufferRange",
    "sourceInfo",
    "videoInfo",
    "audioInfo",
    "videoFrame",
    "error",
    "unloadCompleted",
];

/// The event a message names, and its body.
pub fn event_of(message: &Value) -> Option<(&str, &Value)> {
    message.as_object()?.iter().find(|(k, _)| EVENTS.contains(&k.as_str())).map(|(k, v)| (k.as_str(), v))
}

/// The dynamic range the pipeline names (`HDR10`, `DolbyVision`, `HLG`, `none`…).
pub fn range_of(hdr_type: &str) -> DynamicRange {
    let key: String = hdr_type.chars().filter(char::is_ascii_alphanumeric).collect::<String>().to_lowercase();
    match key.as_str() {
        "hdr10" if hdr_type.contains('+') => DynamicRange::Hdr10Plus,
        "hdr10" => DynamicRange::Hdr10,
        "hdr10plus" => DynamicRange::Hdr10Plus,
        "hlg" => DynamicRange::Hlg,
        "dolbyvision" | "dolbyhdr" | "dv" => DynamicRange::DolbyVision,
        "none" | "sdr" => DynamicRange::Sdr,
        _ => DynamicRange::Unknown,
    }
}

/// Playback details' words for the range the pipeline names.
fn range_label(hdr_type: &str) -> String {
    match range_of(hdr_type) {
        DynamicRange::Hdr10 => "HDR10".into(),
        DynamicRange::Hdr10Plus => "HDR10+".into(),
        DynamicRange::Hlg => "HLG".into(),
        DynamicRange::DolbyVision => "Dolby Vision".into(),
        DynamicRange::Sdr => "SDR".into(),
        _ => hdr_type.into(),
    }
}

/// A pixel aspect ratio given as `{"width", "height"}` or `"w:h"`.
fn pixel_aspect(v: Option<&Value>) -> Option<f32> {
    let (w, h) = match v? {
        Value::Object(o) => (o.get("width")?.as_f64()?, o.get("height")?.as_f64()?),
        Value::String(s) => {
            let (w, h) = s.split_once(':')?;
            (w.trim().parse().ok()?, h.trim().parse().ok()?)
        }
        _ => return None,
    };
    (w > 0.0 && h > 0.0).then_some((w / h) as f32)
}

/// The largest rectangle of `aspect` (width over height) centred in `area`.
pub fn fit(area: Rect, aspect: f32) -> Rect {
    if !(aspect.is_finite() && aspect > 0.0) || area.w <= 0.0 || area.h <= 0.0 {
        return area;
    }
    if area.w / area.h > aspect {
        let w = area.h * aspect;
        Rect::new(area.x + (area.w - w) / 2.0, area.y, w, area.h)
    } else {
        let h = area.w / aspect;
        Rect::new(area.x, area.y + (area.h - h) / 2.0, area.w, h)
    }
}

fn ints(r: Rect) -> [i32; 4] {
    [r.x.round() as i32, r.y.round() as i32, r.w.round() as i32, r.h.round() as i32]
}

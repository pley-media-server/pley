//! The player engine: what plays a video or a song for the playback
//! controller. On the TV it is LG's media pipeline (uMediaServer, in
//! `pley-webos`); in the simulator and tests, `pley-player`'s `SimEngine`.
//!
//! The controller owns one engine, sends it commands and drains its events
//! every frame; the engine never calls back.

use pley_base::Rect;

use crate::model::DynamicRange;

/// What kind of address an engine opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MediaKind {
    /// A video or song stream from `master.m3u8` or `index.m3u8`.
    Hls,
    /// The file as it is (direct play), with `Range` seeks.
    File,
    /// A Live TV channel's sliding HLS window: no end, no seeking back past it.
    LiveHls,
    /// A Live TV channel's MPEG-TS as the provider sends it.
    LiveTs,
}

impl MediaKind {
    /// A Live TV channel.
    pub fn is_live(self) -> bool {
        matches!(self, MediaKind::LiveHls | MediaKind::LiveTs)
    }

    /// An HLS playlist rather than a file.
    pub fn is_hls(self) -> bool {
        matches!(self, MediaKind::Hls | MediaKind::LiveHls)
    }
}

/// The video stream's facts, from the item's details: what LG's pipeline
/// declares as its ceilings for HLS (`adaptiveStreaming`) so it sets the
/// decoder up for the largest picture at once.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VideoFacts {
    /// ffprobe's name (`hevc`); `unknown` for a transcode's output.
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub frame_rate: Option<f64>,
    pub dynamic_range: DynamicRange,
    pub dv_profile: Option<i64>,
    pub bit_depth: Option<u32>,
}

/// The audio stream's facts: none when the server converts the sound.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioFacts {
    pub codec: String,
    pub channels: Option<u32>,
}

/// What to open.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadRequest {
    /// Absolute and keyed (`/api/media/pley_mk_…/master.m3u8`): LG's pipeline
    /// sends no headers, so the address carries its own key. Never logged.
    pub url: String,
    pub kind: MediaKind,
    /// Where to start, in ms of the file's time (0 for live).
    pub start_ms: u64,
    /// A file's container (`mkv`, `mp4`) or MIME type; none for HLS.
    pub container: Option<String>,
    pub video: Option<VideoFacts>,
    pub audio: Option<AudioFacts>,
    /// The item's duration, before the engine knows it.
    pub duration_hint_ms: Option<u64>,
    /// Play as soon as it can; false opens it paused (a seek preview kept, a stream prepared).
    pub autoplay: bool,
}

impl LoadRequest {
    /// A request for `url` of `kind`, from the start, playing at once.
    pub fn new(url: impl Into<String>, kind: MediaKind) -> LoadRequest {
        LoadRequest {
            url: url.into(),
            kind,
            start_ms: 0,
            container: None,
            video: None,
            audio: None,
            duration_hint_ms: None,
            autoplay: true,
        }
    }
}

/// One audio track of what plays, in the file's order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AudioTrack {
    /// Its place among the file's audio tracks, from 0: what `select_audio` takes.
    pub ordinal: usize,
    /// ISO 639 as the pipeline says it.
    pub language: Option<String>,
    pub codec: Option<String>,
    pub channels: Option<u32>,
}

/// What an engine tells the controller.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineEvent {
    /// The source is open; its duration once known (none for live).
    Loaded {
        duration_ms: Option<u64>,
    },
    AudioTracks(Vec<AudioTrack>),
    /// The picture's size, rate and range as decoded.
    VideoInfo {
        width: u32,
        height: u32,
        fps: Option<f32>,
        dynamic_range: DynamicRange,
    },
    /// The first picture is on screen: the curtain can go.
    FirstPicture,
    Playing,
    Paused,
    /// Buffering while playing (not a seek), and its end.
    Waiting(bool),
    /// How much is loaded ahead of the position.
    Buffered {
        ahead_ms: u64,
    },
    Ended,
    /// The engine failed. `before_first_picture`: the load itself failed.
    Error {
        code: i32,
        message: String,
        before_first_picture: bool,
    },
}

/// What Playback details shows of the engine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EngineStats {
    /// "webOS media pipeline".
    pub player: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub video_codec: Option<String>,
    pub fps: Option<f32>,
    /// "HDR10", "Dolby Vision"… as the pipeline reports it.
    pub dynamic_range: Option<String>,
    pub audio_codec: Option<String>,
    pub audio_channels: Option<u32>,
    pub bitrate_kbps: Option<u32>,
    pub buffered_ahead_ms: Option<u64>,
    /// Frame counts, when the engine can tell (LG's pipeline can't).
    pub dropped_frames: Option<u64>,
    pub total_frames: Option<u64>,
    /// Anything else worth a line: label, value.
    pub extra: Vec<(String, String)>,
}

/// Plays one source at a time.
pub trait PlayerEngine: Send {
    /// Opens `req`, closing what played before.
    fn load(&mut self, req: LoadRequest);
    /// Plays (or goes on playing).
    fn play(&mut self);
    /// Pauses where it is.
    fn pause(&mut self);
    /// Moves to `position_ms` of the file's time.
    fn seek(&mut self, position_ms: u64);
    /// Plays at `rate` with sound; false when the engine can't.
    fn set_rate(&mut self, rate: f32) -> bool;
    /// Switches to the file's `ordinal`-th audio track; false when it can't.
    fn select_audio(&mut self, ordinal: usize) -> bool;
    /// Where the picture goes: none for the full screen, or a rectangle in dp
    /// on the 960×540 canvas (the picture moved aside for a panel).
    fn set_video_rect(&mut self, rect: Option<Rect>);
    /// Closes the source; the engine may load again.
    fn unload(&mut self);
    /// The events since the last call, in order.
    fn events(&mut self) -> Vec<EngineEvent>;
    /// Where playing is, in ms of the file's time.
    fn position_ms(&self) -> u64;
    /// What Playback details shows.
    fn stats(&self) -> EngineStats;
}

/// An engine that plays nothing and says nothing, recording what it was
/// told: for tests and screens that need an engine but no playback.
#[derive(Debug, Default)]
pub struct NullEngine {
    /// Every command, in words (`load http://…`, `seek 1000`…).
    pub commands: Vec<String>,
    position_ms: u64,
}

impl PlayerEngine for NullEngine {
    fn load(&mut self, req: LoadRequest) {
        self.position_ms = req.start_ms;
        self.commands.push(format!("load {:?} {}", req.kind, req.start_ms));
    }

    fn play(&mut self) {
        self.commands.push("play".into());
    }

    fn pause(&mut self) {
        self.commands.push("pause".into());
    }

    fn seek(&mut self, position_ms: u64) {
        self.position_ms = position_ms;
        self.commands.push(format!("seek {position_ms}"));
    }

    fn set_rate(&mut self, rate: f32) -> bool {
        self.commands.push(format!("rate {rate}"));
        true
    }

    fn select_audio(&mut self, ordinal: usize) -> bool {
        self.commands.push(format!("audio {ordinal}"));
        true
    }

    fn set_video_rect(&mut self, rect: Option<Rect>) {
        self.commands.push(format!("rect {rect:?}"));
    }

    fn unload(&mut self) {
        self.commands.push("unload".into());
    }

    fn events(&mut self) -> Vec<EngineEvent> {
        Vec::new()
    }

    fn position_ms(&self) -> u64 {
        self.position_ms
    }

    fn stats(&self) -> EngineStats {
        EngineStats { player: "No player".into(), ..EngineStats::default() }
    }
}

//! Playing: the play request and its answer, sessions, progress, tracks,
//! qualities, the network memory and speed test, subtitles and seek previews.

use serde::de::Error as _;
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{ClientCaps, is_false, nd, wire_enum};

wire_enum! {
    /// How the app asks for an item to be delivered (apps/server/src/playback.rs, `PlaybackMode`).
    pub enum PlaybackMode or Unknown {
        /// The cheapest the caps allow: direct play, then remux, then transcode.
        #[default]
        Auto = "auto",
        /// The original file, even when the caps don't claim it plays.
        Direct = "direct",
        /// HLS, the video copied when it can be.
        Remux = "remux",
        /// HLS, the video always converted.
        Transcode = "transcode",
        Unknown = "unknown",
    }
}

wire_enum! {
    /// How the server delivers it (apps/server/src/playback.rs, `Method`).
    pub enum PlaybackMethod or Unknown {
        /// The file as it is.
        Direct = "direct",
        /// HLS with the video copied.
        Remux = "remux",
        /// HLS with the video converted.
        Transcode = "transcode",
        #[default]
        Unknown = "unknown",
    }
}

wire_enum! {
    /// A command of the user's remote, on another of their devices (docs/API.md, "Remote control").
    pub enum Control or Unknown {
        Play = "play",
        Pause = "pause",
        Seek = "seek",
        Skip = "skip",
        Next = "next",
        Previous = "previous",
        /// Another audio track, by its stream index.
        Audio = "audio",
        /// Other subtitles, by their stream index; -1 turns them off.
        Subtitles = "subtitles",
        /// A command a newer server knows: nothing obeys it.
        #[default]
        Unknown = "unknown",
    }
}

wire_enum! {
    /// What kind of device a player plays on, as remotes show it (docs/API.md, "Remote control").
    pub enum DeviceKind or Other {
        Tv = "tv",
        Phone = "phone",
        Tablet = "tablet",
        Computer = "computer",
        Browser = "browser",
        #[default]
        Other = "other",
    }
}

/// The body of `POST /api/playback` (apps/server/src/playback.rs,
/// `PlaybackRequest`). What is left out the server picks: the item's first
/// file, the audio this player plays best, nothing burned in, the original
/// quality, the start.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaybackRequest {
    pub item_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<i64>,
    /// Only for a track the person picked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_index: Option<i64>,
    /// Only for picture subtitles burned in by a stream (a player that draws
    /// them from their `.mks` leaves it out).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_index: Option<i64>,
    /// kbps; none: the original quality.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_bitrate: Option<u32>,
    /// Lines: 2160, 1080, 720, 480.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_height: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<i64>,
    #[serde(default, deserialize_with = "nd")]
    pub mode: PlaybackMode,
    /// This TV's install id: a new session ends this client's earlier ones.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    /// Prepared while the last session plays on; the player ends that one itself.
    #[serde(default, deserialize_with = "nd", skip_serializing_if = "is_false")]
    pub keep_previous: bool,
    /// The TV's player sends no headers: addresses carry a key of their own.
    #[serde(default, deserialize_with = "nd", skip_serializing_if = "is_false")]
    pub keyed_urls: bool,
    /// The remote's commands this player obeys; left out: none, only Stop.
    #[serde(default, deserialize_with = "nd", skip_serializing_if = "Vec::is_empty")]
    pub controls: Vec<Control>,
    /// Always a TV from this app: remotes tell the TVs among the user's devices by it. The server
    /// names the TV from the `User-Agent`, webOS giving the app no name of the set's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_kind: Option<DeviceKind>,
    #[serde(default, deserialize_with = "nd")]
    pub caps: ClientCaps,
}

/// A subtitle track of the file being played.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SubtitleTrack {
    pub index: i64,
    #[serde(default, deserialize_with = "nd")]
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub default: bool,
    #[serde(default, deserialize_with = "nd")]
    pub forced: bool,
    #[serde(default, deserialize_with = "nd")]
    pub external: bool,
    #[serde(default, deserialize_with = "nd")]
    pub text_based: bool,
    /// WebVTT: `/api/files/7/subtitles/3.vtt`; none for picture subtitles.
    pub url: Option<String>,
    /// Added on the server (found online, or uploaded).
    #[serde(default, deserialize_with = "nd")]
    pub added: bool,
    /// This profile may delete it.
    #[serde(default, deserialize_with = "nd")]
    pub deletable: bool,
}

/// The answer of `POST /api/playback`: how the item plays and from where.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaybackInfo {
    #[serde(default, deserialize_with = "nd")]
    pub session_id: String,
    #[serde(default, deserialize_with = "nd")]
    pub method: PlaybackMethod,
    /// Relative: `/api/files/{id}/stream`, `/api/playback/{session}/master.m3u8`,
    /// or keyed `/api/media/pley_mk_…/…`.
    #[serde(default, deserialize_with = "nd")]
    pub url: String,
    #[serde(default, deserialize_with = "nd")]
    pub hls: bool,
    #[serde(default, deserialize_with = "nd")]
    pub file_id: i64,
    #[serde(default, deserialize_with = "nd")]
    pub duration_ms: i64,
    /// The audio track that plays.
    pub audio_index: Option<i64>,
    /// The subtitle burned in.
    pub subtitle_index: Option<i64>,
    #[serde(default, deserialize_with = "nd")]
    pub burn_subtitles: bool,
    #[serde(default, deserialize_with = "nd")]
    pub audio_converted: bool,
    /// Every subtitle track of the file.
    #[serde(default, deserialize_with = "nd")]
    pub subtitles: Vec<SubtitleTrack>,
    /// Why it isn't played as it is, one sentence each.
    #[serde(default, deserialize_with = "nd")]
    pub reasons: Vec<String>,
    pub reason: Option<String>,
    /// The quality a conversion of the original aims for (`30m` … `2m`).
    pub auto_quality: Option<String>,
    /// What the screen gets of HDR: `Dolby Vision P8.1`, `SDR (tone-mapped)`…
    pub video_range: Option<String>,
    /// Several sizes (never for LG, which doesn't ask for variants).
    #[serde(default, deserialize_with = "nd")]
    pub adaptive: bool,
    /// Converted video whose bitrate changes in place (`PUT …/bitrate`).
    #[serde(default, deserialize_with = "nd")]
    pub live_bitrate: bool,
    /// 1400 for HLS sessions: segment timestamps are file time plus this.
    pub pts_offset_ms: Option<i64>,
}

wire_enum! {
    /// A transcode job's state (packages/transcoder-api, `JobState`).
    pub enum JobState or Unknown {
        Starting = "starting",
        Running = "running",
        /// Nothing running: paused, or everything asked for is ready.
        Idle = "idle",
        Finished = "finished",
        Failed = "failed",
        #[default]
        Unknown = "unknown",
    }
}

wire_enum! {
    /// What a transcode does with a stream.
    pub enum StreamAction or Unknown {
        Copy = "copy",
        Transcode = "transcode",
        None = "none",
        #[default]
        Unknown = "unknown",
    }
}

/// The live numbers of a session's transcode, for Playback details.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TranscodeInfo {
    #[serde(default, deserialize_with = "nd")]
    pub job_id: String,
    #[serde(default, deserialize_with = "nd")]
    pub state: JobState,
    #[serde(default, deserialize_with = "nd")]
    pub video: StreamAction,
    #[serde(default, deserialize_with = "nd")]
    pub audio: StreamAction,
    /// `HEVC 3840×2160 Dolby Vision P8.1`.
    pub video_in: Option<String>,
    /// `H.264 1920×1080`.
    pub video_out: Option<String>,
    /// `EAC3 5.1`.
    pub audio_in: Option<String>,
    /// `AAC 2.0`.
    pub audio_out: Option<String>,
    pub encoder: Option<String>,
    pub preset: Option<String>,
    pub decoder: Option<String>,
    /// `gpu`, `hybrid`, `cpu` or `copy`.
    pub pipeline: Option<String>,
    /// The tone mapper when HDR is made SDR.
    pub tonemap: Option<String>,
    /// `kept`, `converted to 8.1`.
    pub dolby_vision: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub hdr10plus_stripped: bool,
    #[serde(default, deserialize_with = "nd")]
    pub subtitle_burn: bool,
    /// 2.5: two and a half times real time.
    pub speed: Option<f64>,
    pub fps: Option<f64>,
    #[serde(default, deserialize_with = "nd")]
    pub transcoded_ms: i64,
    #[serde(default, deserialize_with = "nd")]
    pub segments_ready: i64,
    #[serde(default, deserialize_with = "nd")]
    pub segments_total: i64,
    /// Bits per second, video and audio together.
    pub bitrate_out: Option<i64>,
    pub last_error: Option<String>,
    pub rendition: Option<String>,
    pub variants: Option<i64>,
}

/// `GET /api/playback/{session}`: how the session plays now.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SessionStatus {
    #[serde(default, deserialize_with = "nd")]
    pub session_id: String,
    #[serde(default, deserialize_with = "nd")]
    pub method: PlaybackMethod,
    /// None for direct play, or when the transcoder can't say.
    pub transcode: Option<TranscodeInfo>,
}

wire_enum! {
    /// The state a progress report gives.
    pub enum ProgressState or Unknown {
        #[default]
        Playing = "playing",
        Paused = "paused",
        /// Waiting for data.
        Buffering = "buffering",
        /// The player closed.
        Stopped = "stopped",
        Unknown = "unknown",
    }
}

/// The body of `POST /api/items/{id}/progress`: every ~10 s while playing,
/// on pause, every ~30 s paused, `stopped` on closing. At 90 % the item is
/// watched; under 15 s the resume point is cleared.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProgressReport {
    #[serde(default, deserialize_with = "nd")]
    pub position_ms: i64,
    /// The player's own duration; none: the item's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(default, deserialize_with = "nd")]
    pub state: ProgressState,
    /// Keeps the session and its transcode alive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// How long ago a late report was made.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub age_ms: Option<i64>,
    /// The audio track a video plays, by stream index, for the user's remotes; left out while it
    /// isn't known, and by music.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_index: Option<i64>,
    /// The subtitles a video shows, by stream index, -1 for none; left out by music.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtitle_index: Option<i64>,
}

/// What a track choice says of the audio.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AudioChoice {
    /// Left out: the audio chosen before stays (only subtitles were picked).
    #[default]
    Keep,
    /// `null`: the audio picked before is forgotten, each device plays its best track of the language.
    Automatic,
    /// The track the person picked.
    Track(i64),
}

/// The body of `PUT /api/items/{id}/streams`: the tracks picked, remembered
/// for the whole show or the movie. Its audio is the one field whose `null`
/// differs from its absence.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StreamChoice {
    /// The file the indexes are of; none: the item's first.
    pub file_id: Option<i64>,
    pub audio: AudioChoice,
    /// None: subtitles off.
    pub subtitle_index: Option<i64>,
}

impl Serialize for StreamChoice {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        if let Some(id) = self.file_id {
            map.serialize_entry("file_id", &id)?;
        }
        match self.audio {
            AudioChoice::Keep => {}
            AudioChoice::Automatic => map.serialize_entry("audio_index", &None::<i64>)?,
            AudioChoice::Track(i) => map.serialize_entry("audio_index", &i)?,
        }
        if let Some(i) = self.subtitle_index {
            map.serialize_entry("subtitle_index", &i)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for StreamChoice {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<StreamChoice, D::Error> {
        let fields = serde_json::Map::<String, serde_json::Value>::deserialize(d)?;
        let int = |key: &str| -> Result<Option<i64>, D::Error> {
            match fields.get(key) {
                None | Some(serde_json::Value::Null) => Ok(None),
                Some(v) => v.as_i64().map(Some).ok_or_else(|| D::Error::custom(format!("{key} isn't a number"))),
            }
        };
        let audio = match fields.get("audio_index") {
            None => AudioChoice::Keep,
            Some(serde_json::Value::Null) => AudioChoice::Automatic,
            Some(_) => AudioChoice::Track(int("audio_index")?.unwrap_or_default()),
        };
        Ok(StreamChoice { file_id: int("file_id")?, audio, subtitle_index: int("subtitle_index")? })
    }
}

/// The body of `PUT /api/items/{id}/rating`: 0 to 10 (two points a star); none clears it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RatingRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rating: Option<f64>,
}

/// The body of `PUT /api/playback/{session}/bitrate`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BitrateRequest {
    /// kbps.
    #[serde(default, deserialize_with = "nd")]
    pub max_bitrate: u32,
}

/// Its answer: what the stream goes on at, kbps.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BitrateAnswer {
    #[serde(default, deserialize_with = "nd")]
    pub bitrate: u32,
}

/// What the server remembers of this TV's network for Automatic's start
/// (`GET`/`PUT /api/playback/network`, docs/QUALITY.md).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NetworkAnswer {
    /// The server's own network: never a speed test.
    #[serde(default, deserialize_with = "nd")]
    pub local: bool,
    /// The most Automatic starts at here, kbps; none: no limit known.
    pub ceiling_kbps: Option<i64>,
    /// `playback` or `probe`.
    pub source: Option<String>,
    /// Unix milliseconds of what set it.
    pub since: Option<i64>,
    /// Run a speed test before Automatic starts.
    #[serde(default, deserialize_with = "nd")]
    pub probe: bool,
}

/// A body of `PUT /api/playback/network`: a speed test's result, or Automatic's ceiling.
#[derive(Clone, Debug, PartialEq)]
pub enum NetworkReport {
    /// kbps measured; `complete`: all of it arrived within the time it had.
    Probe { kbps: f64, complete: bool },
    /// Automatic's ceiling, kbps; none: the original (forgotten).
    Ceiling(Option<f64>),
}

impl NetworkReport {
    /// The body for `client_id`.
    pub fn body(&self, client_id: &str) -> serde_json::Value {
        match self {
            NetworkReport::Probe { kbps, complete } => {
                serde_json::json!({ "client_id": client_id, "probe_kbps": kbps.round(), "probe_complete": complete })
            }
            NetworkReport::Ceiling(kbps) => {
                serde_json::json!({ "client_id": client_id, "ceiling_kbps": kbps.map(f64::round) })
            }
        }
    }
}

/// What a speed test (`GET /api/playback/speedtest`) got: `bytes` in `ms`
/// from the first byte; `complete` when all of it came in its window.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SpeedSample {
    pub bytes: u64,
    pub ms: u64,
    pub complete: bool,
}

impl SpeedSample {
    /// Less than this says nothing of the network (packages/tv/src/player/network.ts).
    pub const MIN_BYTES: u64 = 256 * 1024;

    /// kbps (bytes × 8 / seconds / 1000), 1 to 10,000,000; none when too little arrived.
    pub fn kbps(&self) -> Option<f64> {
        if self.bytes < Self::MIN_BYTES {
            return None;
        }
        Some(((self.bytes as f64 * 8.0) / self.ms.max(1) as f64).round().clamp(1.0, 10_000_000.0))
    }
}

/// A subtitle found online for a file (`GET /api/items/{id}/subtitles/search`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SubtitleResult {
    /// Opaque: what `POST /api/items/{id}/subtitles` takes back.
    #[serde(default, deserialize_with = "nd")]
    pub id: String,
    #[serde(default, deserialize_with = "nd")]
    pub provider: String,
    /// `en`, `pt-BR`.
    pub language: Option<String>,
    /// The release it was made for.
    #[serde(default, deserialize_with = "nd")]
    pub name: String,
    /// `srt`, `vtt`, `ass`, `ssa`.
    pub format: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub hearing_impaired: bool,
    #[serde(default, deserialize_with = "nd")]
    pub forced: bool,
    #[serde(default, deserialize_with = "nd")]
    pub downloads: i64,
    /// `hash`: made for this very file; `title`: for the title.
    pub matched: Option<String>,
}

/// `{ results }`: the answer of the subtitle search.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SubtitleResults {
    #[serde(default, deserialize_with = "nd")]
    pub results: Vec<SubtitleResult>,
}

/// The body of `POST /api/items/{id}/subtitles` that adds a subtitle found by search.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AddSubtitle {
    #[serde(default, deserialize_with = "nd")]
    pub result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<i64>,
}

/// `GET /api/files/{id}/trickplay`: a file's seek previews, tiled into JPEG
/// sheets. Thumbnail `k` shows the video at `k × interval_ms`, at column
/// `k % columns`, row `(k / columns) % rows` of sheet `k / (columns × rows)`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrickplayInfo {
    /// The `v` of the sheets' addresses.
    #[serde(default, deserialize_with = "nd")]
    pub id: String,
    #[serde(default, deserialize_with = "nd")]
    pub interval_ms: i64,
    /// One thumbnail, in pixels.
    #[serde(default, deserialize_with = "nd")]
    pub width: i64,
    #[serde(default, deserialize_with = "nd")]
    pub height: i64,
    #[serde(default, deserialize_with = "nd")]
    pub columns: i64,
    #[serde(default, deserialize_with = "nd")]
    pub rows: i64,
    /// Thumbnails in all.
    #[serde(default, deserialize_with = "nd")]
    pub count: i64,
    /// Sheets: `0.jpg` to `{sheets - 1}.jpg`.
    #[serde(default, deserialize_with = "nd")]
    pub sheets: i64,
}

impl TrickplayInfo {
    /// The thumbnail for `position_ms`.
    pub fn thumbnail_at(&self, position_ms: i64) -> i64 {
        if self.interval_ms <= 0 || self.count <= 0 {
            return 0;
        }
        (position_ms / self.interval_ms).clamp(0, self.count - 1)
    }

    /// The sheet thumbnail `k` is on.
    pub fn sheet_of(&self, k: i64) -> i64 {
        k / (self.columns * self.rows).max(1)
    }

    /// Its column and row on that sheet.
    pub fn cell_of(&self, k: i64) -> (i64, i64) {
        let columns = self.columns.max(1);
        (k % columns, (k / columns) % self.rows.max(1))
    }
}

wire_enum! {
    /// Where a download stands on the server. TVs stream everything; the
    /// model reads it for completeness.
    pub enum ConversionState or Unknown {
        /// The original plays as it is.
        Direct = "direct",
        Queued = "queued",
        Running = "running",
        Done = "done",
        Failed = "failed",
        #[default]
        Unknown = "unknown",
    }
}

/// `POST /api/conversions` and `GET /api/conversions/{id}`: a download's file.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ConversionInfo {
    pub id: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub state: ConversionState,
    /// 0 to 1.
    #[serde(default, deserialize_with = "nd")]
    pub progress: f64,
    pub size: Option<i64>,
    pub error: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub url: String,
}

/// A quality the player offers (docs/QUALITY.md, the same on every app):
/// the original, Automatic, or a conversion capped at a bitrate and height.
/// Ids are what the server's `auto_quality` names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QualityPreset {
    pub id: &'static str,
    pub label: &'static str,
    /// What Settings says under it, where it says anything.
    pub hint: Option<&'static str>,
    pub max_bitrate_kbps: Option<u32>,
    pub max_height: Option<u32>,
}

impl QualityPreset {
    /// The mode it asks for: the original and Automatic let the server pick; the others convert.
    pub fn mode(&self) -> PlaybackMode {
        if self.max_bitrate_kbps.is_some() { PlaybackMode::Transcode } else { PlaybackMode::Auto }
    }
}

/// Every quality, in the menu's order (apps/server/src/playback.rs `QUALITIES`;
/// hints from the Android app's Settings).
pub const QUALITY_PRESETS: &[QualityPreset] = &[
    QualityPreset {
        id: "original",
        label: "Original",
        hint: Some("Full quality. May buffer on a slow connection."),
        max_bitrate_kbps: None,
        max_height: None,
    },
    QualityPreset {
        id: "auto",
        label: "Automatic",
        hint: Some("Dynamically adjust to conditions."),
        max_bitrate_kbps: None,
        max_height: None,
    },
    QualityPreset {
        id: "30m",
        label: "30 Mbps 4K",
        hint: None,
        max_bitrate_kbps: Some(30_000),
        max_height: Some(2160),
    },
    QualityPreset {
        id: "15m",
        label: "15 Mbps 4K",
        hint: None,
        max_bitrate_kbps: Some(15_000),
        max_height: Some(2160),
    },
    QualityPreset {
        id: "20m",
        label: "20 Mbps 1080p",
        hint: None,
        max_bitrate_kbps: Some(20_000),
        max_height: Some(1080),
    },
    QualityPreset {
        id: "12m",
        label: "12 Mbps 1080p",
        hint: None,
        max_bitrate_kbps: Some(12_000),
        max_height: Some(1080),
    },
    QualityPreset {
        id: "8m",
        label: "8 Mbps 1080p",
        hint: None,
        max_bitrate_kbps: Some(8_000),
        max_height: Some(1080),
    },
    QualityPreset { id: "4m", label: "4 Mbps 720p", hint: None, max_bitrate_kbps: Some(4_000), max_height: Some(720) },
    QualityPreset { id: "2m", label: "2 Mbps 480p", hint: None, max_bitrate_kbps: Some(2_000), max_height: Some(480) },
];

/// The quality with `id`.
pub fn quality_preset(id: &str) -> Option<&'static QualityPreset> {
    QUALITY_PRESETS.iter().find(|q| q.id == id)
}

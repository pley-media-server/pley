//! The capability report a play request carries (apps/server/src/playback.rs,
//! `Caps`): what this TV plays as it is. Version 1 fields always; version 2
//! (`caps_version: 2`) adds the decoders, HDR, sound, subtitles and HLS facts
//! the server decides by (docs/API.md, "Version 2").
//!
//! It writes its keys in the web app's order (`apps/webos/src/caps.ts`) and
//! whole numbers without a decimal point, so the LG port's snapshots compare
//! byte for byte.

use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize, Serializer};

use super::{DynamicRange, js_number, nd};

/// One video decoder (`video[]`): a stream plays as it is when one entry takes
/// its codec, profile, size (either way round), frame rate, depth and level.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct VideoDecoder {
    /// ffprobe's name: `h264`, `hevc`, `vp9`, `av1`, `mpeg2video`, `mpeg4`.
    #[serde(default, deserialize_with = "nd")]
    pub codec: String,
    /// Needed for HDR transcodes (HEVC Main 10 kept HDR10 or HLG).
    #[serde(default, deserialize_with = "nd")]
    pub hardware: bool,
    /// ffprobe's profile names (`Main 10`); none: any.
    pub profiles: Option<Vec<String>>,
    pub max_width: Option<i64>,
    pub max_height: Option<i64>,
    pub max_fps: Option<f64>,
    pub max_bit_depth: Option<i64>,
    /// ffprobe's numbering: HEVC 5.1 is 153, H.264 5.1 is 51.
    pub max_level: Option<i64>,
}

impl Serialize for VideoDecoder {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        map.serialize_entry("codec", &self.codec)?;
        map.serialize_entry("hardware", &self.hardware)?;
        if let Some(p) = &self.profiles {
            map.serialize_entry("profiles", p)?;
        }
        if let Some(w) = self.max_width {
            map.serialize_entry("max_width", &w)?;
        }
        if let Some(h) = self.max_height {
            map.serialize_entry("max_height", &h)?;
        }
        if let Some(fps) = self.max_fps {
            map.serialize_entry("max_fps", &JsNumber(fps))?;
        }
        if let Some(d) = self.max_bit_depth {
            map.serialize_entry("max_bit_depth", &d)?;
        }
        if let Some(l) = self.max_level {
            map.serialize_entry("max_level", &l)?;
        }
        map.end()
    }
}

/// A Dolby Vision profile decoded, up to a level when it says.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct DolbyVisionDecoder {
    #[serde(default, deserialize_with = "nd")]
    pub profile: i64,
    pub max_level: Option<i64>,
}

impl Serialize for DolbyVisionDecoder {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        map.serialize_entry("profile", &self.profile)?;
        if let Some(l) = self.max_level {
            map.serialize_entry("max_level", &l)?;
        }
        map.end()
    }
}

/// How the TV plays HLS. LG never sets `variants`: its pipeline may go black
/// moving between sizes, so Automatic changes the bitrate in place
/// (docs/QUALITY.md).
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct HlsCaps {
    /// Sessions start at `master.m3u8`, which names Dolby Vision.
    #[serde(default, deserialize_with = "nd")]
    pub master_playlist: bool,
    /// Ignored by the server (segments are always MPEG-TS).
    #[serde(default, deserialize_with = "nd")]
    pub segments: Vec<String>,
    pub variants: Option<bool>,
}

impl Serialize for HlsCaps {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        map.serialize_entry("master_playlist", &self.master_playlist)?;
        map.serialize_entry("segments", &self.segments)?;
        if let Some(v) = self.variants {
            map.serialize_entry("variants", &v)?;
        }
        map.end()
    }
}

/// The screen: its size decides download sizes; the refresh rates are ignored.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct DisplayCaps {
    #[serde(default, deserialize_with = "nd")]
    pub width: i64,
    #[serde(default, deserialize_with = "nd")]
    pub height: i64,
    #[serde(default, deserialize_with = "nd")]
    pub refresh_rates: Vec<f64>,
}

impl Serialize for DisplayCaps {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        map.serialize_entry("width", &self.width)?;
        map.serialize_entry("height", &self.height)?;
        map.serialize_entry("refresh_rates", &JsNumbers(&self.refresh_rates))?;
        map.end()
    }
}

/// What the device is; the server accepts it and doesn't use it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DeviceCaps {
    /// `tv`.
    #[serde(default, deserialize_with = "nd")]
    pub form: String,
    /// `low`, `mid` or `high`.
    pub tier: Option<String>,
    #[serde(default, deserialize_with = "nd")]
    pub fire_tv: bool,
    pub model: Option<String>,
}

/// The capability report. `caps_version` none is a version 1 report (only
/// the first five fields are written): what a TV sends when its probe failed.
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ClientCaps {
    /// Containers played as they are (`mkv`, `mp4`…), matched case-insensitively.
    #[serde(default, deserialize_with = "nd")]
    pub containers: Vec<String>,
    /// Version 1's codecs: stand-ins only when `video` is absent.
    #[serde(default, deserialize_with = "nd")]
    pub video_codecs: Vec<String>,
    /// Version 1's: stand-ins only when `audio_decode` and `audio_passthrough` are absent.
    #[serde(default, deserialize_with = "nd")]
    pub audio_codecs: Vec<String>,
    /// Version 1: shows HDR10 and HLG.
    #[serde(default, deserialize_with = "nd")]
    pub hdr: bool,
    /// Version 1: decodes Dolby Vision.
    #[serde(default, deserialize_with = "nd")]
    pub dolby_vision: bool,
    /// `2` turns the fields below on.
    pub caps_version: Option<u32>,
    #[serde(default, deserialize_with = "nd")]
    pub video: Vec<VideoDecoder>,
    /// Shown by the screen and fed by a 10-bit hardware decoder.
    #[serde(default, deserialize_with = "nd")]
    pub hdr_types: Vec<DynamicRange>,
    #[serde(default, deserialize_with = "nd")]
    pub dolby_vision_profiles: Vec<DolbyVisionDecoder>,
    #[serde(default, deserialize_with = "nd")]
    pub audio_decode: Vec<String>,
    /// Sent as they are to a receiver: `ac3`, `eac3`, `truehd`, `dts`.
    #[serde(default, deserialize_with = "nd")]
    pub audio_passthrough: Vec<String>,
    /// The output's channels: 2, 6 or 8.
    #[serde(default, deserialize_with = "nd")]
    pub audio_channels: u32,
    /// Shown by the player itself, pictures included.
    #[serde(default, deserialize_with = "nd")]
    pub subtitle_codecs: Vec<String>,
    /// Switches a file's audio tracks itself: a non-default track needs no stream.
    #[serde(default, deserialize_with = "nd")]
    pub audio_track_switching: bool,
    /// Draws picture subtitles from their `.mks` beside HLS too.
    #[serde(default, deserialize_with = "nd")]
    pub picture_subtitles_sideload: bool,
    #[serde(default, deserialize_with = "nd")]
    pub hls: HlsCaps,
    pub display: Option<DisplayCaps>,
    pub device: Option<DeviceCaps>,
}

impl ClientCaps {
    /// What a TV sends when it couldn't check what it plays: version 1, all empty.
    pub fn empty() -> ClientCaps {
        ClientCaps::default()
    }

    /// A version 2 report.
    pub fn is_v2(&self) -> bool {
        self.caps_version.is_some_and(|v| v >= 2)
    }

    /// Whether `container` (as `MediaFile.container` names it) plays as it is;
    /// `matroska` and `mkv` are the same.
    pub fn plays_container(&self, container: &str) -> bool {
        let first = container.split(',').next().unwrap_or("").trim().to_ascii_lowercase();
        let name = if first == "matroska" { "mkv" } else { first.as_str() };
        self.containers.iter().any(|c| c.eq_ignore_ascii_case(name))
    }

    /// Whether an audio codec plays here, decoded or passed through.
    pub fn plays_audio(&self, codec: &str) -> bool {
        [&self.audio_codecs, &self.audio_decode, &self.audio_passthrough]
            .into_iter()
            .flatten()
            .any(|c| c.eq_ignore_ascii_case(codec))
    }

    /// Whether the screen shows `range` (version 2's `hdr_types`).
    pub fn shows(&self, range: DynamicRange) -> bool {
        self.hdr_types.contains(&range)
    }
}

impl Serialize for ClientCaps {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        map.serialize_entry("containers", &self.containers)?;
        map.serialize_entry("video_codecs", &self.video_codecs)?;
        map.serialize_entry("audio_codecs", &self.audio_codecs)?;
        map.serialize_entry("hdr", &self.hdr)?;
        map.serialize_entry("dolby_vision", &self.dolby_vision)?;
        if let Some(version) = self.caps_version {
            map.serialize_entry("caps_version", &version)?;
            map.serialize_entry("video", &self.video)?;
            map.serialize_entry("hdr_types", &self.hdr_types)?;
            map.serialize_entry("dolby_vision_profiles", &self.dolby_vision_profiles)?;
            map.serialize_entry("audio_decode", &self.audio_decode)?;
            map.serialize_entry("audio_passthrough", &self.audio_passthrough)?;
            map.serialize_entry("audio_channels", &self.audio_channels)?;
            map.serialize_entry("subtitle_codecs", &self.subtitle_codecs)?;
            map.serialize_entry("audio_track_switching", &self.audio_track_switching)?;
            map.serialize_entry("picture_subtitles_sideload", &self.picture_subtitles_sideload)?;
            map.serialize_entry("hls", &self.hls)?;
            if let Some(d) = &self.display {
                map.serialize_entry("display", d)?;
            }
            if let Some(d) = &self.device {
                map.serialize_entry("device", d)?;
            }
        }
        map.end()
    }
}

/// A number written as JavaScript writes it.
struct JsNumber(f64);

impl Serialize for JsNumber {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        js_number::serialize(&self.0, s)
    }
}

struct JsNumbers<'a>(&'a [f64]);

impl Serialize for JsNumbers<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        js_number::list::serialize(self.0, s)
    }
}

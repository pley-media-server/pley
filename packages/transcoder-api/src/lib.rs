//! Wire protocol between the pley media server and a pley transcoder node.
//!
//! The transcoder is a separate program that owns every interaction with
//! FFmpeg. The server talks to it over plain HTTP + JSON using the types in
//! this crate. See `docs/TRANSCODER.md` for the endpoint list.
//!
//! Conventions: all timestamps are unix **milliseconds** (`*_ms`), durations are
//! milliseconds, stream indexes are FFmpeg input stream indexes.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Protocol version spoken by this crate. Bumped on breaking changes.
pub const API_VERSION: u32 = 1;

/// Environment variable carrying the shared secret: the server's managed
/// transcoder gets it this way, and one started by hand should too (a
/// command line is in the list of processes, for every account to read).
pub const TOKEN_ENV: &str = "PLEY_TRANSCODER_TOKEN";

/// Line printed on stdout by `pley-transcoder --announce` once it is listening.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Announce {
    pub event: String,
    pub addr: String,
}

// ---------------------------------------------------------------------------
// Media information
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MediaInfo {
    /// Normalised container name: `mkv`, `mp4`, `webm`, `avi`, `ts`, `mp3`, `flac`, …
    pub container: Option<String>,
    /// Raw FFmpeg demuxer name(s), e.g. `matroska,webm`.
    pub format_name: Option<String>,
    pub duration_ms: Option<i64>,
    /// Container start time (ms, may be negative). FFmpeg's `-ss` and the
    /// output timeline are relative to it, packet timestamps are not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_ms: Option<i64>,
    /// Overall bitrate in bits per second.
    pub bitrate: Option<i64>,
    pub streams: Vec<MediaStream>,
    /// Container level tags with lowercase keys (`title`, `artist`, `album`, …).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub chapters: Vec<Chapter>,
    /// An embedded cover picture exists (MP3/M4A/FLAC artwork, MP4 cover).
    #[serde(default)]
    pub has_cover_art: bool,
}

impl MediaInfo {
    pub fn video(&self) -> Option<&MediaStream> {
        self.streams.iter().find(|s| s.kind == StreamKind::Video)
    }

    pub fn default_audio(&self) -> Option<&MediaStream> {
        let mut audio = self.streams.iter().filter(|s| s.kind == StreamKind::Audio);
        let first = audio.clone().next();
        audio.find(|s| s.default).or(first)
    }

    pub fn stream(&self, index: i64) -> Option<&MediaStream> {
        self.streams.iter().find(|s| s.index == index)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Chapter {
    pub start_ms: i64,
    pub end_ms: i64,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum StreamKind {
    #[default]
    Video,
    Audio,
    Subtitle,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DynamicRange {
    #[default]
    Sdr,
    Hdr10,
    Hdr10plus,
    Hlg,
    DolbyVision,
}

impl DynamicRange {
    pub fn is_hdr(self) -> bool {
        !matches!(self, DynamicRange::Sdr)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            DynamicRange::Sdr => "sdr",
            DynamicRange::Hdr10 => "hdr10",
            DynamicRange::Hdr10plus => "hdr10plus",
            DynamicRange::Hlg => "hlg",
            DynamicRange::DolbyVision => "dolby_vision",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DynamicRange::Sdr => "SDR",
            DynamicRange::Hdr10 => "HDR10",
            DynamicRange::Hdr10plus => "HDR10+",
            DynamicRange::Hlg => "HLG",
            DynamicRange::DolbyVision => "Dolby Vision",
        }
    }
}

/// What a Dolby Vision profile 7 stream's enhancement layer carries, as
/// the RPUs of its first frames say.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum DolbyVisionLayer {
    /// A minimal enhancement layer: no picture of its own (its residual is
    /// zero), so the base layer with the RPU shows all of it.
    Mel,
    /// A full enhancement layer: picture data (12-bit detail) that only
    /// players decoding profile 7 add to the base layer.
    Fel,
}

impl DolbyVisionLayer {
    pub fn label(self) -> &'static str {
        match self {
            DolbyVisionLayer::Mel => "MEL",
            DolbyVisionLayer::Fel => "FEL",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MediaStream {
    /// FFmpeg stream index inside the file. External subtitle files get 1000+.
    pub index: i64,
    pub kind: StreamKind,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    #[serde(default)]
    pub default: bool,
    #[serde(default)]
    pub forced: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitrate: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,

    // ---- video ----
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bit_depth: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pix_fmt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interlaced: Option<bool>,
    /// How far the picture is turned to be shown, in degrees, as ffprobe
    /// reads the stream's display matrix (`rotation`, counterclockwise):
    /// phones record portrait video as landscape frames turned by 90 or
    /// -90. Absent when not turned (and in older probes).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<i64>,
    /// Frames are stored out of order (B-frames). FFmpeg then seeks a little
    /// before the point asked for in containers that don't seek by
    /// presentation time (Matroska, MPEG-TS).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_b_frames: Option<bool>,
    /// The video carries CEA-608/708 closed captions in its frames, as
    /// ffprobe finds them (`closed_captions`). Absent in older probes,
    /// which say nothing either way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed_captions: Option<bool>,
    /// The frames are stored out of display order (B-frames), but the file
    /// gives each one the time it is decoded at as the time it is shown (an
    /// MP4 muxed from a raw stream, without composition offsets). Players
    /// that trust those times show frames out of order or drop them, and
    /// copying the stream keeps the wrong times: only decoding it puts them
    /// right.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub decode_order_times: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hdr: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic_range: Option<DynamicRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dv_profile: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dv_level: Option<i64>,
    /// Dolby Vision base-layer signal compatibility id (0 none, 1 HDR10, 2 SDR, 4 HLG).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dv_bl_compat: Option<i64>,
    /// Dolby Vision profile 7's enhancement layer: full (`fel`) or minimal
    /// (`mel`). `None`: another profile, or not known (probed by an older
    /// transcoder, or frames that couldn't be read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dv_el: Option<DolbyVisionLayer>,
    /// Dolby Vision found only in the frames (their RPUs): the file doesn't
    /// name it, having no configuration record (some remuxes and older
    /// muxers leave it out). A player given the file as it is decodes its
    /// base layer; a remux names it. The profile is the one the RPUs
    /// point to, the base layer's compatibility the one its transfer
    /// says, and `dv_level` is unknown. Older probes took such a stream
    /// for its base layer.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dv_unnamed: bool,
    /// HDR10+ dynamic metadata (SMPTE ST 2094-40) in the video's first
    /// frames, looked for in Dolby Vision and PQ (HDR10) streams. `true` on
    /// a Dolby Vision stream is Dolby Vision + HDR10+, which some Dolby
    /// Vision decoders fail on; on a PQ stream it comes with the dynamic
    /// range `hdr10plus`. `None`: not looked for (other streams, or probed
    /// by an older transcoder).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hdr10plus: Option<bool>,
    /// HDR10's static metadata (the mastering display's colours and
    /// brightness, SMPTE ST 2086) in the video's first frames, looked for
    /// in Dolby Vision and PQ streams. Quick Sync's scaler and VAAPI tone
    /// map by it: without it the first leaves the picture as it is and the
    /// second refuses the frames. `None`: not looked for (other streams, or
    /// probed by an older transcoder).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hdr10_metadata: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_transfer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_primaries: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_space: Option<String>,

    // ---- audio ----
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_layout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<i64>,

    // ---- subtitle ----
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_based: Option<bool>,
    /// Set by the server for sidecar subtitle files; never by the transcoder.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub external: bool,
    /// Set by the server for subtitles people added (uploaded, or found on
    /// OpenSubtitles), which are external too; never by the transcoder.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub added: bool,
}

/// "Dolby Vision P8.1", "HDR10", … for display: the profile with its base
/// layer's compatibility (profile 8 and up; profile 7's is always Blu-ray
/// HDR10, and profile 5 has none).
pub fn dynamic_range_label(range: DynamicRange, dv_profile: Option<i64>, dv_bl_compat: Option<i64>) -> String {
    match (range, dv_profile) {
        (DynamicRange::DolbyVision, Some(p)) => match dv_bl_compat {
            Some(c @ (1 | 2 | 4)) if p >= 8 => format!("Dolby Vision P{p}.{c}"),
            _ => format!("Dolby Vision P{p}"),
        },
        _ => range.label().to_owned(),
    }
}

/// The whole name of a video's format, as every client shows it: "Dolby
/// Vision P7 FEL", "Dolby Vision P8.1 + HDR10+", "HDR10+", "HDR10"…
pub fn video_range_label(
    range: DynamicRange,
    dv_profile: Option<i64>,
    dv_bl_compat: Option<i64>,
    dv_el: Option<DolbyVisionLayer>,
    hdr10plus: bool,
) -> String {
    let mut label = dynamic_range_label(range, dv_profile, dv_bl_compat);
    if range == DynamicRange::DolbyVision {
        if let Some(el) = dv_el.filter(|_| dv_profile == Some(7)) {
            label.push(' ');
            label.push_str(el.label());
        }
        if hdr10plus {
            label.push_str(" + HDR10+");
        }
    }
    label
}

impl MediaStream {
    /// [`MediaStream::rotation`] from 0 to 359 degrees; 0 when not turned.
    pub fn rotation_degrees(&self) -> i64 {
        self.rotation.unwrap_or(0).rem_euclid(360)
    }

    /// Whether the picture is shown on its side: its width and height swap.
    pub fn shown_sideways(&self) -> bool {
        matches!(self.rotation_degrees(), 90 | 270)
    }

    pub fn dynamic_range(&self) -> DynamicRange {
        self.dynamic_range.unwrap_or_default()
    }

    /// The format's whole name ([`video_range_label`]).
    pub fn dynamic_range_label(&self) -> String {
        video_range_label(
            self.dynamic_range(),
            self.dv_profile,
            self.dv_bl_compat,
            self.dv_el,
            self.hdr10plus == Some(true),
        )
    }

    /// The Dolby Vision profile alone: "Dolby Vision P7", "Dolby Vision
    /// P8.1" (no layer, no HDR10+).
    pub fn dolby_vision_label(&self) -> String {
        dynamic_range_label(self.dynamic_range(), self.dv_profile, self.dv_bl_compat)
    }

    /// Dolby Vision without a backward-compatible base layer (profile 5, or
    /// 7/8 with compatibility id 0): only DV-capable decoders show it correctly.
    pub fn is_dolby_vision_only(&self) -> bool {
        self.dynamic_range() == DynamicRange::DolbyVision
            && (self.dv_profile == Some(5) || self.dv_bl_compat == Some(0))
    }

    /// Whether the picture is HDR10 once its scene-by-scene metadata is left
    /// unread: HDR10 and HDR10+, and Dolby Vision over an HDR10 base layer
    /// (compatibility 1, and 6, profile 7's Blu-ray HDR10).
    pub fn has_hdr10_base(&self) -> bool {
        match self.dynamic_range() {
            DynamicRange::Hdr10 | DynamicRange::Hdr10plus => true,
            DynamicRange::DolbyVision => matches!(self.dv_bl_compat, Some(1 | 6)),
            DynamicRange::Hlg | DynamicRange::Sdr => false,
        }
    }

    /// The Dolby Vision level the stream's size and frame rate need: the
    /// lowest whose pixels a second take them, in Dolby's table (1 for 720p
    /// at 24 fps up to 13 for 8K at 120). `None` without a size or a rate.
    pub fn dv_level_needed(&self) -> Option<i64> {
        const PIXEL_RATES: [(i64, f64); 13] = [
            (1, 1280.0 * 720.0 * 24.0),
            (2, 1280.0 * 720.0 * 30.0),
            (3, 1920.0 * 1080.0 * 24.0),
            (4, 1920.0 * 1080.0 * 30.0),
            (5, 1920.0 * 1080.0 * 60.0),
            (6, 3840.0 * 2160.0 * 24.0),
            (7, 3840.0 * 2160.0 * 30.0),
            (8, 3840.0 * 2160.0 * 48.0),
            (9, 3840.0 * 2160.0 * 60.0),
            (10, 3840.0 * 2160.0 * 120.0),
            (11, 7680.0 * 4320.0 * 30.0),
            (12, 7680.0 * 4320.0 * 60.0),
            (13, 7680.0 * 4320.0 * 120.0),
        ];
        let (w, h, fps) = (self.width?, self.height?, self.frame_rate.filter(|f| *f > 0.0)?);
        // A hair over, for rates rounded up (24.0001).
        let rate = (w * h) as f64 * fps / 1.001;
        PIXEL_RATES.iter().find(|(_, most)| rate <= *most).map(|(level, _)| *level)
    }

    /// The level Dolby Vision is named with: the file's, unless its size
    /// and frame rate need less (a re-encode that says 10 for 4K at 24
    /// fps): a decoder that refuses the level the file says takes the one
    /// the stream needs.
    pub fn dv_level_named(&self) -> Option<i64> {
        match (self.dv_level.filter(|l| *l > 0), self.dv_level_needed()) {
            (Some(said), Some(needed)) => Some(said.min(needed)),
            (said, needed) => said.or(needed),
        }
    }

    pub fn is_text_subtitle(&self) -> bool {
        self.kind == StreamKind::Subtitle && self.text_based.unwrap_or(false)
    }
}

/// Subtitle codecs that are text (can be converted to WebVTT / rendered with libass).
pub fn is_text_subtitle_codec(codec: &str) -> bool {
    matches!(
        codec,
        "subrip"
            | "srt"
            | "ass"
            | "ssa"
            | "webvtt"
            | "mov_text"
            | "text"
            | "microdvd"
            | "subviewer"
            | "subviewer1"
            | "sami"
            | "jacosub"
            | "realtext"
            | "stl"
            | "mpl2"
            | "pjs"
            | "vplayer"
    )
}

// ---------------------------------------------------------------------------
// Capabilities, limits, status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum HwAccel {
    #[default]
    Auto,
    None,
    Nvenc,
    Qsv,
    Amf,
    Vaapi,
    Videotoolbox,
    /// Vulkan Video (`h264_vulkan`), used for fully-GPU tone mapping pipelines.
    Vulkan,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Tonemapper {
    /// GPU (Vulkan) via libplacebo. Handles Dolby Vision profile 5 reshaping.
    Libplacebo,
    /// GPU via OpenCL (`tonemap_opencl`).
    Opencl,
    /// CPU via zimg (`zscale` + `tonemap`).
    Zscale,
    /// Intel's GPU via Quick Sync's scaler (`vpp_qsv`), in place of
    /// libplacebo where the GPU falls behind: the transcoder's choice, never
    /// one to ask for. Never sent: a job's status says it with
    /// [`VideoPlan::tonemap_quick_sync`], which older servers ignore, where
    /// this value would make its status unreadable to them.
    #[serde(rename = "vpp_qsv")]
    QuickSync,
}

impl Tonemapper {
    pub fn as_str(self) -> &'static str {
        match self {
            Tonemapper::Libplacebo => "libplacebo",
            Tonemapper::Opencl => "opencl",
            Tonemapper::Zscale => "zscale",
            Tonemapper::QuickSync => "vpp_qsv",
        }
    }
}

/// How much of libplacebo's work a tone mapped transcode does: all of it
/// where the GPU has the speed, less where it would fall behind the player
/// (see [`Capabilities::tonemap_speed`]). Ordered from the fastest.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum TonemapQuality {
    /// Balanced, with colours outside BT.709 clipped rather than mapped,
    /// no dithering and a bilinear downscaler, which samples the source
    /// directly where the output is at least half its size.
    Fast,
    /// libplacebo's defaults but its two costliest steps: no contrast
    /// recovery, and the chroma upscaled bilinearly.
    Balanced,
    /// libplacebo's defaults.
    #[default]
    High,
}

impl TonemapQuality {
    pub fn as_str(self) -> &'static str {
        match self {
            TonemapQuality::Fast => "fast",
            TonemapQuality::Balanced => "balanced",
            TonemapQuality::High => "high",
        }
    }
}

/// Frames a second one FFmpeg process tone maps from 4K HDR10 to 4K SDR
/// with libplacebo at each [`TonemapQuality`], measured on synthetic
/// frames when the transcoder detects FFmpeg.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct TonemapSpeed {
    pub high: f64,
    pub balanced: f64,
    pub fast: f64,
}

impl TonemapSpeed {
    pub fn at(&self, quality: TonemapQuality) -> f64 {
        match quality {
            TonemapQuality::Fast => self.fast,
            TonemapQuality::Balanced => self.balanced,
            TonemapQuality::High => self.high,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum TonemapMode {
    #[default]
    Auto,
    Libplacebo,
    Opencl,
    Zscale,
    None,
}

/// What a transcode makes of Dolby Vision profile 7 whose enhancement layer
/// is full (FEL, [`DolbyVisionLayer::Fel`]). No FFmpeg reads that layer, so
/// the picture is always its HDR10 base layer's; what differs is whether
/// the stream's Dolby Vision metadata is used with it. Its mapping never is
/// (libplacebo applies none that needs the layer), but the brightness it
/// gives each scene is, where libplacebo is built with libdovi. On most
/// titles that brightness matches the base layer; on those whose layer
/// carries part of it, it describes a picture the base layer isn't, and
/// the tone mapped picture comes out too dark or too bright.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DolbyVisionFel {
    /// As HDR10: the base layer, by its own static metadata, the Dolby
    /// Vision metadata unread. The picture its HDR10 grade gives, whatever
    /// the title; and any decoder can decode it, Quick Sync's included.
    #[default]
    BaseLayer,
    /// The base layer, tone mapped by the Dolby Vision metadata's
    /// brightness for each scene, as profile 7 with a minimal layer and
    /// profile 8.1 are.
    Metadata,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum VideoCodec {
    #[default]
    H264,
    Hevc,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EncoderInfo {
    /// FFmpeg encoder name, e.g. `h264_nvenc`.
    pub name: String,
    pub codec: VideoCodec,
    pub hardware: bool,
    pub accel: HwAccel,
    /// An HEVC encoder that passed a 10-bit (Main 10) test encode too: what
    /// transcodes that keep HDR need.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub main10: bool,
    /// Quick Sync's encoder passed its test encodes in low-power mode
    /// (Intel's fixed-function encoder, VDEnc), Main 10 too for HEVC: jobs
    /// then encode that way, which leaves the GPU's shader cores to tone
    /// mapping and runs faster at the same picture quality.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub low_power: bool,
}

/// A hardware pipeline that keeps frames in GPU memory from decode to encode.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GpuPipeline {
    pub accel: HwAccel,
    /// Input codec keys ([`decode_key`]): `h264`, `hevc`, `hevc10`,
    /// `hevc10-yuv422p10le`, `av1`, `vp9`, `mpeg2video`.
    pub codecs: Vec<String>,
    /// Those whose frames stay on the GPU in 10 bits too, into its HEVC
    /// Main 10 encoder: transcodes that keep HDR. Quick Sync's only; older
    /// transcoders tried none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub main10: Vec<String>,
}

/// Codec key used for hardware decode capability checks: `hevc10` for
/// 10-bit HEVC; the depth for video deeper than 10 bits (`hevc-12`); and the
/// pixel format after the key for video that isn't 4:2:0 (`h264-yuv422p`).
/// A pipeline tested on 8- and 10-bit 4:2:0 clips isn't used for the others
/// (Intel's decoders can't decode 4:2:2 H.264): the start-up tests try one
/// of them, 10-bit 4:2:2 HEVC (`hevc10-yuv422p10le`), and the rest match no
/// pipeline.
pub fn decode_key(codec: &str, bit_depth: Option<i64>, pix_fmt: Option<&str>) -> String {
    let key = match (codec, bit_depth.unwrap_or(8)) {
        (c, d) if d > 10 => format!("{c}-{d}"),
        ("hevc", d) if d > 8 => "hevc10".into(),
        ("h264", d) if d > 8 => "h264-10".into(),
        ("vp9", d) if d > 8 => "vp9-10".into(),
        (c, _) => c.to_owned(),
    };
    match pix_fmt {
        Some(format) if !is_420(format) => format!("{key}-{format}"),
        _ => key,
    }
}

/// Whether FFmpeg's pixel format halves chroma both ways (4:2:0), as all
/// but one of the start-up tests' clips do.
fn is_420(pix_fmt: &str) -> bool {
    ["yuv420", "yuvj420", "yuva420", "nv12", "nv21", "p010", "p012", "p016"].iter().any(|f| pix_fmt.starts_with(f))
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Capabilities {
    pub ffmpeg_available: bool,
    pub ffprobe_available: bool,
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub ffmpeg_version: Option<String>,
    /// Video encoders that passed a test encode, best first.
    pub encoders: Vec<EncoderInfo>,
    /// Decoding hwaccels reported by `ffmpeg -hwaccels`.
    pub hwaccels: Vec<String>,
    /// Working tone mappers, best first.
    pub tonemappers: Vec<Tonemapper>,
    /// Dolby Vision profile 5 can be rendered correctly (requires libplacebo).
    pub dolby_vision: bool,
    /// Codecs the GPU decodes straight into Vulkan memory, enabling a
    /// zero-copy decode → libplacebo tone mapping pipeline.
    #[serde(default)]
    pub vulkan_decode: Vec<String>,
    /// `h264_vulkan` encodes libplacebo output without leaving the GPU.
    #[serde(default)]
    pub vulkan_encode: bool,
    /// Zero-copy decode → scale → encode pipelines that passed a self-test,
    /// with the input codecs each one handles.
    #[serde(default)]
    pub gpu_pipelines: Vec<GpuPipeline>,
    /// Quick Sync's scaler tone maps HDR10 that its decoder decoded to SDR
    /// ([`Tonemapper::QuickSync`]). A start-up test.
    #[serde(default)]
    pub qsv_tonemap: bool,
    /// Quick Sync lays picture subtitles on frames its decoder decoded, on
    /// Windows. A start-up test.
    #[serde(default)]
    pub qsv_overlay: bool,
    /// OpenCL tone maps Dolby Vision by its own metadata between FFmpeg's
    /// decoder on Direct3D 11 and Quick Sync's encoder, the frames never
    /// copied (Windows, with Pley's FFmpeg or jellyfin-ffmpeg): where Dolby
    /// Vision goes when libplacebo can't keep up. A start-up test, on a clip
    /// that carries the metadata.
    #[serde(default)]
    pub qsv_opencl_dovi: bool,
    /// Tone mappers whose frames reach a VAAPI encoder (Linux): libplacebo
    /// on a Vulkan device and OpenCL on a device made from the VAAPI one,
    /// the frames uploaded back to it. A start-up test; zimg always does.
    #[serde(default)]
    pub vaapi_tonemappers: Vec<Tonemapper>,
    /// [`Capabilities::vulkan_decode`] on the Vulkan device made from the
    /// VAAPI one, through libplacebo to a VAAPI encoder.
    #[serde(default)]
    pub vaapi_vulkan_decode: Vec<String>,
    pub subtitle_burn_image: bool,
    pub subtitle_burn_text: bool,
    /// `-readrate`/`-readrate_initial_burst` are supported (transcode throttling).
    pub readrate: bool,
    /// `-readrate_catchup` is supported: a throttled read that fell behind
    /// (a slow disk or share) catches up faster than real time.
    #[serde(default)]
    pub readrate_catchup: bool,
    /// FFmpeg's `chromaprint` muxer is available: audio fingerprints for
    /// intro and credits detection (`POST /v1/fingerprint`).
    #[serde(default)]
    pub chromaprint: bool,
    /// Jobs have a multivariant playlist (`master.m3u8`) naming their
    /// codecs, and remuxes do what [`VideoRequest::dolby_vision`] and
    /// [`VideoRequest::strip_hdr10plus`] ask. Older transcoders lack both.
    #[serde(default)]
    pub hls_master_playlist: bool,
    /// Transcodes of HDR video can keep it, as HEVC Main 10 in HDR10 or HLG
    /// ([`VideoRequest::dynamic_range`]): a 10-bit HEVC encoder works. Older
    /// transcoders lack it and always tone-map to SDR.
    #[serde(default)]
    pub hdr_transcode: bool,
    /// `POST /v1/subtitles/mks` copies a subtitle track into a Matroska file
    /// of its own (picture subtitles players show next to an HLS stream).
    /// Older transcoders lack it.
    #[serde(default)]
    pub subtitle_matroska: bool,
    /// `POST /v1/subtitles/mks` takes [`SubtitleRequest::partial`]: it
    /// answers within seconds with the Matroska copied so far, and
    /// `X-Pley-Complete: 0` until it is whole. Older transcoders answer only
    /// once the whole track is copied.
    #[serde(default)]
    pub subtitle_matroska_partial: bool,
    /// `POST /v1/conversions` makes a [`JobRequest`] into one Matroska file
    /// (a download that plays where the original doesn't). Older
    /// transcoders lack it.
    #[serde(default)]
    pub conversions: bool,
    /// Conversions make MP4 files too ([`JobRequest::container`]). Older
    /// transcoders make Matroska only.
    #[serde(default)]
    pub conversions_mp4: bool,
    /// Jobs take lower renditions for adaptive bitrate
    /// ([`JobRequest::variants`]). Older transcoders make one.
    #[serde(default)]
    pub hls_variants: bool,
    /// A job's video bitrate changes as it is sent (`POST
    /// /v1/jobs/{id}/bitrate`). Older transcoders keep the one asked for.
    #[serde(default)]
    pub live_bitrate: bool,
    /// libplacebo's speed on this computer's GPU, which sets each tone
    /// mapped transcode's [`TonemapQuality`]. Absent without libplacebo on
    /// a GPU, and from older transcoders, which tone map at `High`.
    #[serde(default)]
    pub tonemap_speed: Option<TonemapSpeed>,
    /// libplacebo takes `shader_cache` (FFmpeg 8 and later): compiled
    /// shaders are kept on disk, so tone mapping starts sooner.
    #[serde(default)]
    pub placebo_shader_cache: bool,
    /// libplacebo takes `fallback_peak` (Pley's FFmpeg): HDR10 without its
    /// metadata is tone mapped from that peak rather than PQ's 10000 cd/m².
    #[serde(default)]
    pub placebo_fallback_peak: bool,
    /// `tonemap_opencl` has BT.2390's curve (jellyfin-ffmpeg's and Pley's
    /// FFmpeg): it then tone maps by the stream's own metadata and applies
    /// Dolby Vision's reshaping, and is given that curve.
    #[serde(default)]
    pub opencl_bt2390: bool,
    /// AMF encoders get frames FFmpeg uploaded to the AMD GPU (Direct3D 11)
    /// rather than frames in system memory, which AMF copies over itself
    /// several times slower (Windows).
    #[serde(default)]
    pub amf_upload: bool,
    /// FFmpeg has the `h264_metadata` bitstream filter, which writes the
    /// colour primaries `h264_amf` leaves unsaid.
    #[serde(default)]
    pub h264_metadata: bool,
    /// Live channels: an MPEG-TS stream sent in as it comes made into a
    /// live HLS stream (`/v1/live`). Older transcoders lack it.
    #[serde(default)]
    pub live: bool,
    /// Jobs make fragmented MP4 segments (`init.mp4` and `{n}.m4s`) when
    /// [`JobRequest::segment_format`] asks for them, except jobs without
    /// video, which stay MPEG-TS ([`JobCreated::segment_format`] says which).
    /// Older transcoders make MPEG-TS only.
    #[serde(default)]
    pub fmp4_segments: bool,
    /// Transcodes do what [`TranscodeOptions::dolby_vision_fel`] asks with
    /// Dolby Vision profile 7's full enhancement layer. Older transcoders
    /// tone map it by its Dolby Vision metadata, whatever is asked.
    #[serde(default)]
    pub dolby_vision_fel: bool,
    /// FFmpeg can rebuild Dolby Vision profile 7's full picture from its
    /// base layer, its full enhancement layer and their metadata, as a
    /// Dolby Vision player does (Pley's FFmpeg). Experimental: what
    /// [`TranscodeOptions::dolby_vision_fel_full`] turns on.
    #[serde(default)]
    pub dolby_vision_fel_full: bool,
    /// The version of the seek previews this transcoder makes
    /// ([`TrickplayInfo::version`]). Older transcoders lack it (0).
    #[serde(default)]
    pub trickplay_version: u32,
    /// Seek-preview sets it makes at once (`POST /v1/trickplay`): a client
    /// sends no more, and further requests wait their turn. With it, a set's
    /// time limit grows with its file, and a set paused through a long
    /// transcode keeps its work. Older transcoders lack it (0): they make one
    /// at a time, and give a set up after an hour.
    #[serde(default)]
    pub trickplay_runs: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Limits {
    pub max_jobs: u32,
    pub max_hw_jobs: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TranscoderInfo {
    pub name: String,
    pub version: String,
    /// The commit the program was built from, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    pub api_version: u32,
    /// SPDX license of the transcoder program (Pley's: `AGPL-3.0-only`).
    pub license: String,
    /// Where its source code can be obtained, if it is offered (Pley's
    /// transcoder: its repository).
    pub source: Option<String>,
    pub started_at_ms: i64,
    pub capabilities: Capabilities,
    pub limits: Limits,
    /// Whether jobs' segments can be kept in memory
    /// ([`TranscodeOptions::in_memory`]). `None` from older transcoders,
    /// which keep them on disk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemoryInfo>,
}

/// Where a transcoder can keep jobs' segments in memory: a folder in RAM
/// (`/dev/shm` on Linux), with room to spare.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct MemoryInfo {
    pub available: bool,
    /// Why not, when not ("works on Linux only").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The folder in RAM.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The most segments take there, in bytes: past it, jobs' segments go
    /// to disk.
    #[serde(default)]
    pub budget_bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Counters {
    pub jobs_created: u64,
    pub jobs_failed: u64,
    pub processes_started: u64,
    pub process_failures: u64,
    pub segments_produced: u64,
    pub segments_served: u64,
    pub bytes_served: u64,
    pub probes: u64,
    pub probe_failures: u64,
    pub probe_ms_total: u64,
    pub thumbnails: u64,
    pub subtitle_extractions: u64,
    pub keyframe_scans: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TranscoderStatus {
    pub version: String,
    pub uptime_secs: u64,
    pub ffmpeg_version: Option<String>,
    pub max_jobs: u32,
    pub running_processes: u32,
    pub hw_processes: u32,
    pub max_hw_jobs: u32,
    pub work_dir_bytes: u64,
    /// Segments kept in memory now ([`TranscodeOptions::in_memory`]).
    #[serde(default)]
    pub memory_bytes: u64,
    /// Background FFmpeg runs (seek previews, fingerprints, keyframe scans)
    /// paused while a playback starts.
    #[serde(default)]
    pub background_paused: u32,
    /// Seek-preview sets being made now, paused ones included.
    #[serde(default)]
    pub trickplay_running: u32,
    /// How busy its computer's processors are and the memory it has, where
    /// it can tell. Absent from older transcoders.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub load: Option<Load>,
    pub counters: Counters,
    pub jobs: Vec<JobStatus>,
    /// Live channels being made ([`Capabilities::live`]).
    #[serde(default)]
    pub live: Vec<LiveStatus>,
}

/// What a transcoder's computer has to spare ([`TranscoderStatus::load`]).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Load {
    /// The cores it may use: the processors it may run on, or its control
    /// group's CPU limit (a container's `--cpus`) where that is lower.
    pub cores: f64,
    /// The share of those cores' time that went to any work lately (about
    /// 10 s), 0 to 1. Absent before it has looked twice, and where it can't
    /// tell (macOS).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub busy: Option<f64>,
    /// The cores other programs' work took lately (not its own, nor its
    /// FFmpeg processes').
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreign: Option<f64>,
    /// Memory it can still take without swapping, in bytes: what the
    /// computer has available, or what its control group's memory limit
    /// leaves, whichever is less.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_available: Option<u64>,
}

// ---------------------------------------------------------------------------
// Live channels (IPTV)
// ---------------------------------------------------------------------------

/// A live channel to make (`POST /v1/live`). Its MPEG-TS comes as the body
/// of `PUT /v1/live/{id}/input`, as the channel sends it; players read
/// `index.m3u8`, a sliding window of segments, and its segments. Every player
/// of a channel reads the same stream, so it is made for all of them: H.264
/// and AAC stereo, the video copied when it is H.264 already (and not
/// interlaced, when `deinterlace`), the audio when it is AAC.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveRequest {
    /// The channel's name, for status pages.
    #[serde(default)]
    pub label: Option<String>,
    /// Who watches it, for status pages.
    #[serde(default)]
    pub owner: Option<String>,
    /// Transcode interlaced video into progressive (double rate where a
    /// hardware encoder makes it), rather than copy it as it is.
    #[serde(default = "default_true")]
    pub deinterlace: bool,
    /// Transcode the video even when it could be copied.
    #[serde(default)]
    pub force_transcode: bool,
    /// The most lines a transcode makes (default: as many as the channel's).
    #[serde(default)]
    pub max_height: Option<u32>,
    /// A transcode's video bitrate cap, kbit/s (default chosen from its size).
    #[serde(default)]
    pub max_bitrate_kbps: Option<u32>,
    #[serde(default)]
    pub hwaccel: HwAccel,
    /// How much is kept behind the live edge for pausing and going back,
    /// in seconds (default 300; at least 30).
    #[serde(default)]
    pub window_seconds: Option<u32>,
}

impl Default for LiveRequest {
    fn default() -> Self {
        Self {
            label: None,
            owner: None,
            deinterlace: true,
            force_transcode: false,
            max_height: None,
            max_bitrate_kbps: None,
            hwaccel: HwAccel::Auto,
            window_seconds: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LiveCreated {
    pub id: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LiveState {
    /// No input yet, or not enough of it to tell what it is.
    #[default]
    Waiting,
    /// FFmpeg is making segments.
    Running,
    /// The input ended or the job was stopped.
    Ended,
    /// FFmpeg failed, or the input isn't a stream it reads ([`LiveStatus::error`]).
    Failed,
}

/// What a live channel's input carries and what is made of it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LivePlan {
    /// The input's video codec (`h264`, `mpeg2video`, `hevc`…).
    #[serde(default)]
    pub video_codec: Option<String>,
    #[serde(default)]
    pub width: Option<i64>,
    #[serde(default)]
    pub height: Option<i64>,
    #[serde(default)]
    pub frame_rate: Option<f64>,
    #[serde(default)]
    pub interlaced: bool,
    /// The input's audio codec (`aac`, `mp2`, `ac3`…).
    #[serde(default)]
    pub audio_codec: Option<String>,
    #[serde(default)]
    pub audio_channels: Option<i64>,
    /// `copy`, or the encoder transcoding it (`h264_nvenc`, `libx264`…).
    pub video: String,
    /// `copy` or `aac`.
    pub audio: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LiveStatus {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub owner: Option<String>,
    pub state: LiveState,
    #[serde(default)]
    pub plan: Option<LivePlan>,
    #[serde(default)]
    pub error: Option<String>,
    pub created_at_ms: i64,
    /// Bytes of input received so far.
    pub bytes_in: u64,
    /// Segments made so far.
    pub segments: u64,
    /// How far behind real time the newest segment came out, ms: what the
    /// input or the transcode lags (0 while it keeps up).
    #[serde(default)]
    pub lag_ms: i64,
}

// ---------------------------------------------------------------------------
// Jobs (HLS sessions)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum VideoMode {
    /// Stream copy (remux). Falls back to `Transcode` when the stream cannot be
    /// segmented on keyframes yet; the effective mode is reported in the plan.
    Copy,
    #[default]
    Transcode,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum AudioMode {
    Copy,
    #[default]
    Transcode,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VideoRequest {
    /// Video stream to use (default: first video stream).
    #[serde(default)]
    pub stream_index: Option<i64>,
    #[serde(default)]
    pub mode: VideoMode,
    #[serde(default)]
    pub codec: VideoCodec,
    #[serde(default)]
    pub max_width: Option<u32>,
    #[serde(default)]
    pub max_height: Option<u32>,
    /// The video bitrate a transcode aims for and keeps under, in kbit/s
    /// (default: its size's, [`default_video_kbps`]).
    #[serde(default)]
    pub max_bitrate_kbps: Option<u32>,
    /// What a remux does with Dolby Vision (ignored when transcoding, and
    /// for other streams).
    #[serde(default)]
    pub dolby_vision: DolbyVisionMode,
    /// A remux leaves out the HDR10+ metadata (SMPTE ST 2094-40 SEI
    /// messages) of an HEVC stream, for Dolby Vision decoders that fail on
    /// streams carrying both (androidx/media#1895).
    #[serde(default)]
    pub strip_hdr10plus: bool,
    /// The dynamic range a transcode of HDR video is made in (ignored for
    /// SDR video and remuxes). HDR10 and HLG need `codec` HEVC: the
    /// transcode is then HEVC Main 10 with the HDR10 metadata of the source
    /// kept (Dolby Vision's left out); video in the other HDR format is
    /// converted with libplacebo (HLG or Dolby Vision profile 5 to HDR10, say).
    /// Where that can't be done (no 10-bit HEVC encoder, no libplacebo for a
    /// conversion, subtitles drawn onto the video) it is tone-mapped to SDR,
    /// with a warning.
    #[serde(default)]
    pub dynamic_range: OutputRange,
}

/// A lower rendition of a job's video ([`JobRequest::variants`]): the video
/// as [`VideoRequest`] asks, made no bigger and with no more bits than this.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct VariantRequest {
    #[serde(default)]
    pub max_width: Option<u32>,
    #[serde(default)]
    pub max_height: Option<u32>,
    /// The video bitrate it aims for and keeps under, in kbit/s (default:
    /// its size's, [`default_video_kbps`]).
    #[serde(default)]
    pub max_bitrate_kbps: Option<u32>,
}

/// The video bitrate a transcode aims for at a size when none is asked
/// for, in kbit/s: by its lines counted as a 16:9 picture's, so a 1920×800
/// film gets 1080p's.
pub fn default_video_kbps(width: i64, height: i64) -> u32 {
    match height.max(width * 9 / 16) {
        h if h >= 2000 => 20_000,
        h if h >= 1400 => 12_000,
        h if h >= 1000 => 8_000,
        h if h >= 700 => 4_000,
        h if h >= 500 => 2_000,
        h if h >= 400 => 1_200,
        _ => 700,
    }
}

/// The dynamic range of a transcode of HDR video.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum OutputRange {
    /// Tone-mapped to SDR: what every player shows.
    #[default]
    Sdr,
    /// PQ, BT.2020 (HDR10 static metadata kept when the source has it).
    Hdr10,
    /// Hybrid log-gamma, BT.2020.
    Hlg,
}

impl OutputRange {
    /// The range as [`DynamicRange`] names it.
    pub fn dynamic_range(self) -> DynamicRange {
        match self {
            OutputRange::Sdr => DynamicRange::Sdr,
            OutputRange::Hdr10 => DynamicRange::Hdr10,
            OutputRange::Hlg => DynamicRange::Hlg,
        }
    }
}

/// What a remux does with a Dolby Vision stream. Whatever it is, the
/// stream's Dolby Vision metadata (RPU) stays in the MPEG-TS segments,
/// which can't say what they carry: the master playlist's `CODECS` does.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DolbyVisionMode {
    /// Copied as it is and not named: the base layer's codec only
    /// (`hvc1…`), what a player without Dolby Vision decodes.
    #[default]
    Copy,
    /// Copied as it is and named (`dvh1.05.06`; `hvc1…` with
    /// `SUPPLEMENTAL-CODECS="dvh1.08.06/db1p"` for profile 8): for players
    /// that decode the profile.
    Signal,
    /// Profile 7 made profile 8.1 and named so: the enhancement layer left
    /// out and the RPU rewritten (`dovi_tool`'s mode 2). Other profiles are
    /// signalled as they are.
    To81,
}

/// The codec converted audio is encoded to.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum AudioCodec {
    /// AAC-LC: every player decodes it.
    #[default]
    Aac,
    /// Dolby Digital, up to 5.1 at 640 kbit/s.
    Ac3,
    /// Dolby Digital Plus (E-AC-3), for players that decode it or pass it
    /// through to a receiver.
    Eac3,
}

impl AudioCodec {
    /// FFmpeg's name for the codec (and its encoder).
    pub fn as_str(self) -> &'static str {
        match self {
            AudioCodec::Aac => "aac",
            AudioCodec::Ac3 => "ac3",
            AudioCodec::Eac3 => "eac3",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AudioRequest {
    /// Audio stream to use (default: the default audio stream).
    #[serde(default)]
    pub stream_index: Option<i64>,
    #[serde(default)]
    pub mode: AudioMode,
    /// Codec when transcoding (default AAC).
    #[serde(default)]
    pub codec: AudioCodec,
    /// Output channels when transcoding (default 2), never more than the
    /// source has.
    #[serde(default)]
    pub channels: Option<u32>,
    /// Default: 192 kbit/s for stereo AAC, 64 per channel for more; 640 for
    /// AC-3 and E-AC-3 above stereo.
    #[serde(default)]
    pub bitrate_kbps: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SubtitleBurnRequest {
    /// Embedded subtitle stream to burn in.
    #[serde(default)]
    pub stream_index: Option<i64>,
    /// Sidecar subtitle file (`.srt`, `.ass`, `.sup`, …) to burn in instead.
    #[serde(default)]
    pub external_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranscodeOptions {
    #[serde(default)]
    pub hwaccel: HwAccel,
    #[serde(default = "default_true")]
    pub hardware_decoding: bool,
    #[serde(default)]
    pub tonemap: TonemapMode,
    /// x264 preset for software encodes (`veryfast` by default), or `auto`:
    /// chosen by the transcoder from its load and how far ahead of the player
    /// it is, and changed as they change.
    #[serde(default)]
    pub preset: Option<String>,
    /// Hold ffmpeg to real time once a buffer has been built.
    #[serde(default = "default_true")]
    pub throttle: bool,
    /// Keep the segments in memory when the transcoder can
    /// ([`TranscoderInfo::memory`]) and has room, else on disk. Throttled
    /// jobs only: their lead on the player is bounded.
    #[serde(default = "default_true")]
    pub in_memory: bool,
    /// What transcodes make of Dolby Vision profile 7 with a full
    /// enhancement layer ([`Capabilities::dolby_vision_fel`]).
    #[serde(default)]
    pub dolby_vision_fel: DolbyVisionFel,
    /// Experimental: Dolby Vision profile 7 with a full enhancement layer
    /// is transcoded from the full picture both layers make, where the
    /// transcoder can ([`Capabilities::dolby_vision_fel_full`]), rather
    /// than as [`TranscodeOptions::dolby_vision_fel`] says.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dolby_vision_fel_full: bool,
}

impl Default for TranscodeOptions {
    fn default() -> Self {
        Self {
            hwaccel: HwAccel::Auto,
            hardware_decoding: true,
            tonemap: TonemapMode::Auto,
            preset: None,
            throttle: true,
            in_memory: true,
            dolby_vision_fel: DolbyVisionFel::BaseLayer,
            dolby_vision_fel_full: false,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JobRequest {
    /// Absolute path of the input file, as seen by the transcoder.
    pub input: String,
    /// Media information the caller already has (skips a probe). Ignored when it
    /// does not contain a duration.
    #[serde(default)]
    pub media: Option<MediaInfo>,
    /// Human readable label for status pages, e.g. `"The Matrix (1999)"`.
    #[serde(default)]
    pub label: Option<String>,
    /// Who the job is for (username), for status pages.
    #[serde(default)]
    pub owner: Option<String>,
    /// `None` = drop video (audio-only output).
    #[serde(default)]
    pub video: Option<VideoRequest>,
    /// `None` = drop audio.
    #[serde(default)]
    pub audio: Option<AudioRequest>,
    #[serde(default)]
    pub subtitle: Option<SubtitleBurnRequest>,
    /// Target segment length (default 4 s).
    #[serde(default)]
    pub segment_seconds: Option<f64>,
    /// Start transcoding at this position right away (playback start / resume).
    #[serde(default)]
    pub start_ms: Option<i64>,
    #[serde(default)]
    pub options: TranscodeOptions,
    /// What a conversion's file is (`POST /v1/conversions` only): Matroska
    /// when `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<FileContainer>,
    /// Lower renditions of the video, for a player that moves between them
    /// by itself as its connection and the transcoder allow (adaptive
    /// bitrate), highest first. Each is `video` made no bigger and with no
    /// more bits than it says, and everything else the same; the job's
    /// `master.m3u8` lists them after `video`'s `index.m3u8`, as
    /// `v1.m3u8`… with segments `v1-{n}.ts`…, laid out as `index.m3u8`'s,
    /// so the player can move at any segment. Only one makes segments at a
    /// time: the one the player asked last. Ignored unless the video is
    /// transcoded into segments (a copy, a conversion), and a rendition no
    /// smaller than the one above it is left out ([`JobCreated::variants`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<VariantRequest>,
    /// How the segments are packaged: MPEG-TS (`{n}.ts`), or fragmented MP4
    /// (`init.mp4` and `{n}.m4s`) where the transcoder has
    /// [`Capabilities::fmp4_segments`]. Older transcoders ignore it and make
    /// MPEG-TS; [`JobCreated::segment_format`] says what the job makes.
    #[serde(default, skip_serializing_if = "SegmentFormat::is_ts")]
    pub segment_format: SegmentFormat,
}

/// How a job's HLS segments are packaged.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum SegmentFormat {
    /// MPEG-TS: `{n}.ts`, each segment playable by itself.
    #[default]
    Ts,
    /// Fragmented MP4: `init.mp4`, then `{n}.m4s`, which play after it.
    Fmp4,
}

impl SegmentFormat {
    pub fn is_ts(&self) -> bool {
        *self == Self::Ts
    }

    /// The segments' extension.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Ts => "ts",
            Self::Fmp4 => "m4s",
        }
    }

    /// The segments' media type (`init.mp4`'s is `video/mp4` too).
    pub fn content_type(self) -> &'static str {
        match self {
            Self::Ts => "video/mp2t",
            Self::Fmp4 => "video/mp4",
        }
    }
}

/// What a conversion's file is: Matroska, which keeps every track, or MP4
/// for players that open no Matroska (QuickTime), its text subtitles made
/// MP4's own and its picture ones left out.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum FileContainer {
    #[default]
    Matroska,
    Mp4,
}

impl FileContainer {
    /// The file's extension.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Matroska => "mkv",
            Self::Mp4 => "mp4",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            Self::Matroska => "video/x-matroska",
            Self::Mp4 => "video/mp4",
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VideoPlan {
    /// Effective mode (a requested copy may fall back to transcode).
    pub mode: VideoMode,
    pub codec_in: String,
    pub codec_out: String,
    pub width_in: Option<i64>,
    pub height_in: Option<i64>,
    pub width_out: Option<i64>,
    pub height_out: Option<i64>,
    pub dynamic_range_in: DynamicRange,
    /// What a transcode comes out as: SDR (tone-mapped, or SDR already),
    /// HDR10 or HLG. `None` for remuxes (and from older transcoders).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic_range_out: Option<DynamicRange>,
    pub dv_profile: Option<i64>,
    /// Dolby Vision base-layer compatibility id (1 HDR10, 2 SDR, 4 HLG; 0 none).
    #[serde(default)]
    pub dv_bl_compat: Option<i64>,
    pub tonemap: Option<Tonemapper>,
    /// Quick Sync's scaler tone maps in libplacebo's place
    /// ([`Tonemapper::QuickSync`]); `tonemap` is then `None`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub tonemap_quick_sync: bool,
    /// What libplacebo does of its work (`tonemap` libplacebo only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tonemap_quality: Option<TonemapQuality>,
    pub encoder: Option<String>,
    /// `cpu`, `cuda`, `qsv`, `vaapi`, `videotoolbox`, `d3d11va`, `vulkan`, …
    pub decoder: Option<String>,
    /// `gpu` (frames never leave the GPU), `hybrid` (GPU decode/filter with a
    /// copy to system memory), `cpu`, or `copy`.
    #[serde(default)]
    pub pipeline: Option<String>,
    pub bitrate_kbps: Option<u32>,
    pub subtitle_burn: Option<String>,
    /// What the remux does with Dolby Vision; `None` when the output carries
    /// none (a transcode, or another stream).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dolby_vision: Option<DolbyVisionMode>,
    /// The remux leaves out the stream's HDR10+ metadata.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hdr10plus_stripped: bool,
    /// Profile 7's enhancement layer, full or minimal, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dv_el: Option<DolbyVisionLayer>,
    /// The stream carries HDR10+ (with Dolby Vision, when
    /// `dynamic_range_in` says Dolby Vision).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hdr10plus: bool,
}

impl VideoPlan {
    /// The input's format by name, as [`MediaStream::dynamic_range_label`].
    pub fn range_in_label(&self) -> String {
        video_range_label(self.dynamic_range_in, self.dv_profile, self.dv_bl_compat, self.dv_el, self.hdr10plus)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct AudioPlan {
    pub mode: AudioMode,
    pub codec_in: String,
    pub codec_out: String,
    pub channels_in: Option<i64>,
    pub channels_out: Option<i64>,
    pub bitrate_kbps: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JobCreated {
    pub id: String,
    pub duration_ms: i64,
    pub segment_seconds: f64,
    pub segment_count: u32,
    pub video: Option<VideoPlan>,
    pub audio: Option<AudioPlan>,
    pub warnings: Vec<String>,
    /// The lower renditions made of [`JobRequest::variants`]: with any, the
    /// job is played from its `master.m3u8`.
    #[serde(default)]
    pub variants: u32,
    /// How its segments are packaged: fragmented MP4 only when asked for
    /// and the job has video. Older transcoders make MPEG-TS.
    #[serde(default)]
    pub segment_format: SegmentFormat,
}

/// Where a conversion is.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ConversionState {
    /// Waiting its turn (one at a time) or for a free transcode slot.
    #[default]
    Queued,
    Running,
    /// The file is whole: `GET /v1/conversions/{id}/file`.
    Done,
    /// FFmpeg failed: see `error`.
    Failed,
}

/// `POST /v1/conversions` and `GET /v1/conversions/{id}`: a [`JobRequest`]
/// made into one Matroska file, at full speed. Its id comes from the input
/// (path, size and time) and what the request asks of it, so asking again
/// finds the same conversion (or the file it made).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ConversionStatus {
    pub id: String,
    pub state: ConversionState,
    /// How far it has come, 0 to 1.
    pub progress: f64,
    pub duration_ms: i64,
    /// The file's size, once done.
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub video: Option<VideoPlan>,
    #[serde(default)]
    pub audio: Option<AudioPlan>,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum JobState {
    #[default]
    Starting,
    /// An ffmpeg process is producing segments.
    Running,
    /// No process running (paused client, or everything requested is ready).
    Idle,
    /// The whole file has been produced.
    Finished,
    /// The last process failed; see `last_error`.
    Failed,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ProcessStatus {
    pub pid: Option<u32>,
    pub start_segment: u32,
    pub started_at_ms: i64,
    pub frame: Option<u64>,
    pub fps: Option<f64>,
    pub speed: Option<f64>,
    pub bitrate_kbps: Option<f64>,
    /// Absolute media position (ms) that ffmpeg has produced up to.
    pub out_time_ms: i64,
    pub total_size: Option<u64>,
    pub dup_frames: Option<u64>,
    pub drop_frames: Option<u64>,
    pub hardware: bool,
    /// The software encoder's preset (`veryfast`…), when the video is
    /// encoded in software.
    #[serde(default)]
    pub preset: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct JobStatus {
    pub id: String,
    pub label: Option<String>,
    pub owner: Option<String>,
    pub input: String,
    pub state: JobState,
    pub created_at_ms: i64,
    pub last_access_ms: i64,
    pub duration_ms: i64,
    pub segment_seconds: f64,
    pub segment_count: u32,
    pub segments_ready: u32,
    pub video: Option<VideoPlan>,
    pub audio: Option<AudioPlan>,
    pub process: Option<ProcessStatus>,
    pub restarts: u32,
    pub served_segments: u64,
    pub served_bytes: u64,
    pub last_error: Option<String>,
    pub warnings: Vec<String>,
    /// The rendition the player takes now, of a job with lower ones
    /// ([`JobRequest::variants`]): 0 for `video`, 1 for the first variant…
    /// The rest of the status is that rendition's.
    #[serde(default)]
    pub rendition: u32,
    /// How many lower renditions the job has.
    #[serde(default)]
    pub variants: u32,
    /// Its segments go to memory now ([`TranscodeOptions::in_memory`]);
    /// those made before may be on disk, or the other way round.
    #[serde(default)]
    pub in_memory: bool,
    /// Bytes of its segments in memory now.
    #[serde(default)]
    pub memory_bytes: u64,
}

// ---------------------------------------------------------------------------
// One-shot extraction requests
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ProbeRequest {
    pub path: String,
}

/// `POST /v1/thumbnail` → `image/jpeg`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ThumbnailRequest {
    pub path: String,
    /// Position of the frame (default: 20 % into the file).
    #[serde(default)]
    pub time_ms: Option<i64>,
    /// Output width in pixels (default 640).
    #[serde(default)]
    pub width: Option<u32>,
    /// Duration hint used to compute the default position.
    #[serde(default)]
    pub duration_ms: Option<i64>,
    /// Tone-map HDR sources so frame grabs are not washed out.
    #[serde(default)]
    pub hdr: bool,
}

/// `POST /v1/cover` → embedded cover art bytes (`image/jpeg` or `image/png`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct CoverRequest {
    pub path: String,
}

/// `POST /v1/subtitles` → `text/vtt`. `POST /v1/subtitles/mks` takes it too
/// (`partial` aside): the track (or the subtitle file, `stream_index` left
/// out) copied into Matroska.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SubtitleRequest {
    pub path: String,
    /// Embedded stream index; `None` means `path` is itself a subtitle file.
    #[serde(default)]
    pub stream_index: Option<i64>,
    /// For an embedded track: answer within a few seconds, with the cues
    /// found so far when the track isn't all out by then, rather than wait
    /// for the whole file; `X-Pley-Complete: 0` says more are coming. For a
    /// picture track's Matroska (any track, a subtitle file's too), the
    /// Matroska copied so far, up to its last whole cluster: always a file
    /// players read, its track's header at least (see
    /// [`Capabilities::subtitle_matroska_partial`]).
    #[serde(default)]
    pub partial: bool,
    /// For a picture track's Matroska: where the player is (ms, the track's
    /// time). A track read through the Cues comes from around there first.
    /// Taken by the request that starts the copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_ms: Option<i64>,
    /// For a picture track's Matroska: media information the caller already
    /// has (skips a probe while a playback starts). Ignored when it does not
    /// contain a duration and a start time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaInfo>,
    /// For a picture track's Matroska, with `partial`, for a player that
    /// reads the track once (Kodi's `.sup`): what a read through the Cues
    /// has read so far, from around `from_ms` on, rather than only the
    /// blocks around `from_ms`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rest: bool,
}

/// Header of a `partial` subtitles answer: `1` when the track is complete.
pub const SUBTITLES_COMPLETE_HEADER: &str = "x-pley-complete";

/// `POST /v1/keyframes` → `202` (index is built in the background so later
/// copy-mode jobs can be segmented exactly on keyframes).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct KeyframesRequest {
    pub path: String,
}

/// `POST /v1/hold` → `204`: a playback the transcoder doesn't run itself (a
/// direct play) is starting, so its background work (seek previews,
/// fingerprints, key frames, keyframe scans, probes) waits, and its FFmpeg
/// runs under way pause, for `ms` (a minute at most). Its own jobs' starts
/// hold it without asking. Transcoders before this answer `404`.
/// `POST /v1/jobs/{id}/bitrate` → [`BitrateChanged`]: the job's video, a
/// conversion, goes on at `max_bitrate_kbps` from the segments its player
/// hasn't fetched yet, the same size and layout.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BitrateRequest {
    pub max_bitrate_kbps: u32,
}

/// The bitrate the job goes on at (kbps): the one asked for, kept within
/// 1 Mbps and the job's own.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct BitrateChanged {
    pub bitrate_kbps: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct HoldRequest {
    pub ms: u64,
}

/// `POST /v1/prefetch` → [`Prefetched`]: read the parts of a file a playback
/// starting at `start_ms` reads first (its head, its index, the keyframe
/// before the start and a few seconds on), so that they come from a cache
/// when it does: worth seconds on a network share.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PrefetchRequest {
    pub path: String,
    /// Where the playback would start (ms); 0 or none: the beginning.
    #[serde(default)]
    pub start_ms: Option<i64>,
}

/// What a prefetch read.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Prefetched {
    /// The byte ranges read, in the order they were read.
    pub ranges: Vec<PrefetchedRange>,
    /// Bytes read in all.
    pub bytes: u64,
    pub elapsed_ms: u64,
    /// Why nothing was read: a playback is starting, or the file is being
    /// read already.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
    /// Stopped before the end: a playback started, or time ran out.
    #[serde(default)]
    pub stopped: bool,
}

/// A byte range a prefetch read: `part` is `head`, `index` (Matroska Cues or
/// MP4 movie box) or `start` (where the playback starts).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct PrefetchedRange {
    pub part: String,
    pub start: u64,
    /// Exclusive.
    pub end: u64,
}

/// `POST /v1/fingerprint` → [`Fingerprint`]: a Chromaprint fingerprint of the
/// audio in part of a file. The server compares the fingerprints of episodes
/// to find the intro and credits they share.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FingerprintRequest {
    pub path: String,
    /// Audio stream to analyse (default: the first audio stream).
    #[serde(default)]
    pub stream_index: Option<i64>,
    /// Where to start, ms from the beginning of the file.
    #[serde(default)]
    pub start_ms: i64,
    /// How much audio to analyse; the end of the file may stop it earlier.
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Fingerprint {
    /// Position of the first value, ms from the beginning of the file.
    pub start_ms: i64,
    /// Seconds between consecutive values (1365 samples at 11025 Hz).
    pub item_seconds: f64,
    /// Raw Chromaprint values of the default algorithm, one per `item_seconds`.
    pub data: Vec<u32>,
}

/// `POST /v1/frames` → [`Frames`]: how dark the key frames of part of a video
/// are and how much contrast they have. The server looks for credits in
/// them: text on black.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FramesRequest {
    pub path: String,
    /// Where to start, ms from the beginning of the file.
    #[serde(default)]
    pub start_ms: i64,
    /// How much video to look at; the end of the file may stop it earlier.
    pub duration_ms: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Frames {
    /// Key frames in order of time.
    pub frames: Vec<FrameStats>,
}

/// One key frame, its luma in 8-bit studio range (black is 16).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct FrameStats {
    /// Ms from the beginning of the file.
    pub ms: i64,
    /// Share of pixels at most 32 (near black), in percent.
    pub black: f32,
    /// Luma of the darkest and brightest 10 % of pixels, and the extremes.
    pub ylow: f32,
    pub yhigh: f32,
    pub ymax: f32,
    /// Average saturation.
    pub sat: f32,
}

/// `POST /v1/trickplay`: seek-bar preview thumbnails of a video, tiled into
/// JPEG sprite sheets fetched with `GET /v1/trickplay/{id}/{n}`. Cached until
/// every sheet has been fetched (callers keep their own copy), or for a day:
/// asking again for the same file version meanwhile returns at once.
/// Transcoders with [`Capabilities::trickplay_runs`] make that many at once;
/// further requests wait their turn.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TrickplayRequest {
    pub path: String,
    /// Time between thumbnails (default: 2–10 s depending on the length).
    #[serde(default)]
    pub interval_ms: Option<i64>,
    /// Thumbnail width in pixels (default 320).
    #[serde(default)]
    pub width: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TrickplayInfo {
    /// Identifies the sheets on the transcoder.
    pub id: String,
    pub interval_ms: i64,
    /// Size of one thumbnail.
    pub width: u32,
    pub height: u32,
    /// Thumbnails per sheet: `columns × rows`, left to right, top to bottom.
    pub columns: u32,
    pub rows: u32,
    /// Number of thumbnails; thumbnail `k` shows the video at `k × interval_ms`.
    pub count: u32,
    /// Number of sheets, `0.jpg` to `{sheets - 1}.jpg`.
    pub sheets: u32,
    /// How the sheets were made: a transcoder making them otherwise (their
    /// quality, size or spacing) has a higher one
    /// ([`Capabilities::trickplay_version`]). Sets made by older transcoders
    /// lack it (0).
    #[serde(default)]
    pub version: u32,
}

/// Error body returned by the transcoder for non-2xx responses.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_request_defaults_are_lenient() {
        let req: JobRequest =
            serde_json::from_str(r#"{"input":"/m/a.mkv","video":{},"audio":{"mode":"copy"}}"#).unwrap();
        assert_eq!(req.video.as_ref().unwrap().mode, VideoMode::Transcode);
        let audio = req.audio.unwrap();
        assert_eq!((audio.mode, audio.codec), (AudioMode::Copy, AudioCodec::Aac), "AAC unless asked");
        let audio: AudioRequest = serde_json::from_str(r#"{"codec":"eac3","channels":6}"#).unwrap();
        assert_eq!((audio.codec, audio.codec.as_str()), (AudioCodec::Eac3, "eac3"));
        assert!(req.options.hardware_decoding);
        assert!(req.options.throttle);
        assert_eq!(req.options.hwaccel, HwAccel::Auto);
        // Dolby Vision copied as it is unless asked.
        let video = req.video.unwrap();
        assert_eq!((video.dolby_vision, video.strip_hdr10plus), (DolbyVisionMode::Copy, false));
        let video: VideoRequest =
            serde_json::from_str(r#"{"mode":"copy","dolby_vision":"to81","strip_hdr10plus":true}"#).unwrap();
        assert_eq!((video.dolby_vision, video.strip_hdr10plus), (DolbyVisionMode::To81, true));
        assert_eq!(serde_json::to_value(DolbyVisionMode::Signal).unwrap(), "signal");
        // Transcodes are SDR unless asked.
        assert_eq!(video.dynamic_range, OutputRange::Sdr);
        let video: VideoRequest = serde_json::from_str(r#"{"codec":"hevc","dynamic_range":"hdr10"}"#).unwrap();
        assert_eq!((video.codec, video.dynamic_range), (VideoCodec::Hevc, OutputRange::Hdr10));
        assert_eq!(OutputRange::Hlg.dynamic_range(), DynamicRange::Hlg);
        // Encoders and capabilities from older transcoders.
        let enc: EncoderInfo =
            serde_json::from_str(r#"{"name":"hevc_nvenc","codec":"hevc","hardware":true,"accel":"nvenc"}"#).unwrap();
        assert!(!enc.main10);
        let caps: Capabilities = serde_json::from_str(
            r#"{"ffmpeg_available":true,"ffprobe_available":true,"ffmpeg_path":"","ffprobe_path":"",
                "ffmpeg_version":null,"encoders":[],"hwaccels":[],"tonemappers":[],"dolby_vision":false,
                "subtitle_burn_image":false,"subtitle_burn_text":false,"readrate":false}"#,
        )
        .unwrap();
        assert!(!caps.hdr_transcode && !caps.subtitle_matroska);
    }

    #[test]
    fn video_that_isnt_4_2_0_has_a_decode_key_of_its_own() {
        assert_eq!(decode_key("h264", Some(8), Some("yuv420p")), "h264");
        assert_eq!(decode_key("hevc", Some(10), Some("yuv420p10le")), "hevc10");
        assert_eq!(decode_key("mjpeg", None, Some("yuvj420p")), "mjpeg");
        assert_eq!(decode_key("hevc", Some(10), None), "hevc10", "unknown: as before");
        // 10-bit 4:2:2 HEVC has a start-up test of its own.
        assert_eq!(decode_key("hevc", Some(10), Some("yuv422p10le")), "hevc10-yuv422p10le");
        // What the start-up tests never decode: no GPU pipeline matches it.
        assert_eq!(decode_key("h264", Some(8), Some("yuv422p")), "h264-yuv422p");
        assert_eq!(decode_key("h264", Some(8), Some("yuv444p")), "h264-yuv444p");
        assert_eq!(decode_key("h264", Some(8), Some("gray")), "h264-gray");
        assert_eq!(decode_key("hevc", Some(12), Some("yuv420p12le")), "hevc-12");
        assert_eq!(decode_key("av1", Some(12), Some("yuv444p12le")), "av1-12-yuv444p12le");
        // Pipelines from older transcoders tried nothing in 10 bits.
        let p: GpuPipeline = serde_json::from_str(r#"{"accel":"qsv","codecs":["hevc10"]}"#).unwrap();
        assert!(p.main10.is_empty());
        assert!(!serde_json::to_string(&p).unwrap().contains("main10"));
    }

    /// Dolby Vision profile 7's full enhancement layer: its base layer
    /// alone unless a request asks for its metadata, a server from before
    /// the choice included, and a transcoder from before it doesn't say it
    /// follows the choice.
    #[test]
    fn a_full_enhancement_layer_is_made_from_the_base_layer_unless_asked() {
        let options: TranscodeOptions = serde_json::from_str("{}").unwrap();
        assert_eq!(options.dolby_vision_fel, DolbyVisionFel::BaseLayer);
        assert_eq!(options, TranscodeOptions::default());
        let options: TranscodeOptions = serde_json::from_str(r#"{"dolby_vision_fel":"metadata"}"#).unwrap();
        assert_eq!(options.dolby_vision_fel, DolbyVisionFel::Metadata);
        let json = serde_json::to_string(&TranscodeOptions::default()).unwrap();
        assert!(json.contains(r#""dolby_vision_fel":"base_layer""#), "{json}");
        assert!(!Capabilities::default().dolby_vision_fel);
    }

    #[test]
    fn segment_formats_are_ts_unless_asked_and_known() {
        // Requests from older servers, and answers from older transcoders.
        let req: JobRequest = serde_json::from_str(r#"{"input":"/m/a.mkv"}"#).unwrap();
        assert_eq!(req.segment_format, SegmentFormat::Ts);
        let created: JobCreated = serde_json::from_str(
            r#"{"id":"j","duration_ms":1,"segment_seconds":4.0,"segment_count":1,"video":null,"audio":null,
                "warnings":[]}"#,
        )
        .unwrap();
        assert_eq!(created.segment_format, SegmentFormat::Ts);
        let caps: Capabilities = serde_json::from_str(
            r#"{"ffmpeg_available":true,"ffprobe_available":true,"ffmpeg_path":"","ffprobe_path":"",
                "ffmpeg_version":null,"encoders":[],"hwaccels":[],"tonemappers":[],"dolby_vision":false,
                "subtitle_burn_image":false,"subtitle_burn_text":false,"readrate":false}"#,
        )
        .unwrap();
        assert!(!caps.fmp4_segments);
        // A request for MPEG-TS reads to an older transcoder as it always did.
        let ts = serde_json::to_value(JobRequest { input: "/m/a.mkv".into(), ..Default::default() }).unwrap();
        assert!(ts.get("segment_format").is_none(), "{ts}");
        let fmp4 = JobRequest { segment_format: SegmentFormat::Fmp4, ..Default::default() };
        assert_eq!(serde_json::to_value(&fmp4).unwrap()["segment_format"], "fmp4");
        assert_eq!((SegmentFormat::Ts.extension(), SegmentFormat::Fmp4.extension()), ("ts", "m4s"));
        assert_eq!((SegmentFormat::Ts.content_type(), SegmentFormat::Fmp4.content_type()), ("video/mp2t", "video/mp4"));
    }

    #[test]
    fn media_stream_round_trip_skips_empty_fields() {
        let s = MediaStream {
            index: 1,
            kind: StreamKind::Audio,
            codec: "aac".into(),
            channels: Some(2),
            ..Default::default()
        };
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("width"));
        assert!(!json.contains("external"));
        let back: MediaStream = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn dolby_vision_labels() {
        assert_eq!(dynamic_range_label(DynamicRange::DolbyVision, Some(8), Some(1)), "Dolby Vision P8.1");
        assert_eq!(dynamic_range_label(DynamicRange::DolbyVision, Some(8), Some(4)), "Dolby Vision P8.4");
        assert_eq!(dynamic_range_label(DynamicRange::DolbyVision, Some(5), Some(0)), "Dolby Vision P5");
        assert_eq!(dynamic_range_label(DynamicRange::Hdr10, None, None), "HDR10");
        // Profile 7's base layer is Blu-ray HDR10 (compatibility 6): no ".6".
        assert_eq!(dynamic_range_label(DynamicRange::DolbyVision, Some(7), Some(6)), "Dolby Vision P7");
        let dv = |profile, compat, el, hdr10plus| MediaStream {
            dynamic_range: Some(DynamicRange::DolbyVision),
            dv_profile: Some(profile),
            dv_bl_compat: Some(compat),
            dv_el: el,
            hdr10plus,
            ..Default::default()
        };
        assert_eq!(dv(7, 6, Some(DolbyVisionLayer::Fel), None).dynamic_range_label(), "Dolby Vision P7 FEL");
        assert_eq!(dv(7, 6, Some(DolbyVisionLayer::Mel), Some(false)).dynamic_range_label(), "Dolby Vision P7 MEL");
        assert_eq!(dv(7, 6, None, None).dynamic_range_label(), "Dolby Vision P7");
        assert_eq!(dv(8, 1, None, Some(true)).dynamic_range_label(), "Dolby Vision P8.1 + HDR10+");
        assert_eq!(dv(8, 1, None, Some(true)).dolby_vision_label(), "Dolby Vision P8.1");
        let hdr10plus =
            MediaStream { dynamic_range: Some(DynamicRange::Hdr10plus), hdr10plus: Some(true), ..Default::default() };
        assert_eq!(hdr10plus.dynamic_range_label(), "HDR10+");
        // What is HDR10 under its scene-by-scene metadata: HDR10+, and
        // Dolby Vision over an HDR10 base layer, profile 7's included.
        assert!(hdr10plus.has_hdr10_base());
        assert!(dv(8, 1, None, None).has_hdr10_base() && dv(7, 6, Some(DolbyVisionLayer::Fel), None).has_hdr10_base());
        assert!(!dv(5, 0, None, None).has_hdr10_base(), "profile 5 has no picture without its metadata");
        assert!(!dv(8, 4, None, None).has_hdr10_base() && !dv(8, 2, None, None).has_hdr10_base(), "HLG and SDR bases");
        assert!(!MediaStream::default().has_hdr10_base());
        let p5 =
            MediaStream { dynamic_range: Some(DynamicRange::DolbyVision), dv_profile: Some(5), ..Default::default() };
        assert!(p5.is_dolby_vision_only());
        let p81 = MediaStream {
            dynamic_range: Some(DynamicRange::DolbyVision),
            dv_profile: Some(8),
            dv_bl_compat: Some(1),
            ..Default::default()
        };
        assert!(!p81.is_dolby_vision_only());
    }

    #[test]
    fn default_audio_prefers_flagged_stream() {
        let info = MediaInfo {
            streams: vec![
                MediaStream { index: 0, kind: StreamKind::Video, codec: "h264".into(), ..Default::default() },
                MediaStream { index: 1, kind: StreamKind::Audio, codec: "aac".into(), ..Default::default() },
                MediaStream {
                    index: 2,
                    kind: StreamKind::Audio,
                    codec: "ac3".into(),
                    default: true,
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        assert_eq!(info.default_audio().unwrap().index, 2);
    }

    #[test]
    fn dolby_vision_is_named_at_the_level_its_stream_needs() {
        let dv = |w, h, fps, level| MediaStream {
            width: Some(w),
            height: Some(h),
            frame_rate: Some(fps),
            dv_level: level,
            ..Default::default()
        };
        // 4K at 24 fps (scope or not) needs 6, at 30 fps 7, at 60 fps 9.
        assert_eq!(dv(3840, 2160, 24_000.0 / 1001.0, None).dv_level_needed(), Some(6));
        assert_eq!(dv(3840, 2160, 24.0, None).dv_level_needed(), Some(6));
        assert_eq!(dv(3840, 2072, 30.0, None).dv_level_needed(), Some(7));
        assert_eq!(dv(3840, 2160, 59.94, None).dv_level_needed(), Some(9));
        assert_eq!(dv(1920, 1080, 60.0, None).dv_level_needed(), Some(5));
        assert_eq!(dv(1920, 804, 24.0, None).dv_level_needed(), Some(3));
        // A re-encode that says 10 for 4K at 24 fps is named 6; a level said
        // lower than the size needs stays as said.
        assert_eq!(dv(3840, 1632, 24_000.0 / 1001.0, Some(10)).dv_level_named(), Some(6));
        assert_eq!(dv(3840, 2160, 24.0, Some(6)).dv_level_named(), Some(6));
        assert_eq!(dv(1920, 1080, 60.0, Some(4)).dv_level_named(), Some(4));
        // Nothing to go by: the file's.
        let unknown = MediaStream { dv_level: Some(10), ..Default::default() };
        assert_eq!((unknown.dv_level_needed(), unknown.dv_level_named()), (None, Some(10)));
    }

    #[test]
    fn older_transcoders_and_servers_read_each_others_previews_and_load() {
        // An older transcoder: one set at a time, nothing said of its load.
        let caps: Capabilities = serde_json::from_str(
            r#"{"ffmpeg_available":true,"ffprobe_available":true,"ffmpeg_path":"","ffprobe_path":"",
                "ffmpeg_version":null,"encoders":[],"hwaccels":[],"tonemappers":[],"dolby_vision":false,
                "subtitle_burn_image":false,"subtitle_burn_text":false,"readrate":false,"trickplay_version":1}"#,
        )
        .unwrap();
        assert_eq!(caps.trickplay_runs, 0);
        let old = r#"{"version":"0.1.0","uptime_secs":5,"ffmpeg_version":null,"max_jobs":8,"running_processes":0,
            "hw_processes":0,"max_hw_jobs":2,"work_dir_bytes":0,"counters":{"jobs_created":0,"jobs_failed":0,
            "processes_started":0,"process_failures":0,"segments_produced":0,"segments_served":0,"bytes_served":0,
            "probes":0,"probe_failures":0,"probe_ms_total":0,"thumbnails":0,"subtitle_extractions":0,
            "keyframe_scans":0},"jobs":[]}"#;
        let status: TranscoderStatus = serde_json::from_str(old).unwrap();
        assert_eq!((status.trickplay_running, status.load), (0, None));
        // What it can't measure is left out, and read back as unknown.
        let load = Load { cores: 2.0, ..Default::default() };
        assert_eq!(serde_json::to_string(&load).unwrap(), r#"{"cores":2.0}"#);
        let status = TranscoderStatus { trickplay_running: 3, load: Some(load), ..Default::default() };
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(serde_json::from_str::<TranscoderStatus>(&json).unwrap(), status);
        // An older server, which knows neither, reads a new status.
        #[derive(Deserialize)]
        struct Before {
            running_processes: u32,
            background_paused: u32,
        }
        let before: Before = serde_json::from_str(&json).unwrap();
        assert_eq!((before.running_processes, before.background_paused), (0, 0));
    }
}

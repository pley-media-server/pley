//! Playback decisions and sessions.
//!
//! For every play request the server compares the file with what the client
//! can decode and picks **direct play** (the raw file over HTTP range requests),
//! **remux** (HLS with the video stream copied) or **transcode** (HLS produced by
//! the transcoder). Sessions are tracked for "now playing" and resume.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use pley_transcoder_api::{
    AudioCodec, AudioMode, AudioRequest, DolbyVisionLayer, DolbyVisionMode, DynamicRange, FileContainer, JobRequest,
    JobStatus, MediaInfo, MediaStream, OutputRange, StreamKind, SubtitleBurnRequest, VariantRequest, VideoCodec,
    VideoMode, VideoPlan, VideoRequest, default_video_kbps, video_range_label,
};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

use crate::catalog::FileRow;
use crate::events::{EventBus, ServerEvent};
use crate::transcoder::Transcoder;
use crate::util::now;

/// What a player plays as it is, sent with every play request.
///
/// Version 1 is the first five fields: codec lists and two booleans, what
/// the web app and the Android app's web view send. Version 2
/// (`caps_version: 2`, the native Android app's `ClientCaps`) adds every
/// decoder with its limits, the screen's dynamic ranges, the Dolby Vision
/// profiles decoded, and the audio decoded, passed through to a receiver
/// and the output's channels. Players without `caps_version` play exactly
/// as before version 2 existed. Version 2 players still send the version 1
/// fields: `containers` always counts, the codec lists only where the
/// version 2 fields that replace them are missing.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Caps {
    #[serde(default)]
    pub containers: Vec<String>,
    #[serde(default)]
    pub video_codecs: Vec<String>,
    #[serde(default)]
    pub audio_codecs: Vec<String>,
    /// Client shows HDR10 / HLG as it is: on an HDR screen, or tone-mapping
    /// it itself (desktop Chromium does on SDR screens).
    #[serde(default)]
    pub hdr: bool,
    /// Client decodes Dolby Vision (including profile 5, which has no HDR10
    /// or SDR base layer).
    #[serde(default)]
    pub dolby_vision: bool,

    // ---- version 2 ----
    /// 2 for the fields below; absent: version 1.
    #[serde(default)]
    pub caps_version: Option<u32>,
    /// Every video decoder with its limits. A codec can have several (4K at
    /// 30 fps and 1080p at 60, say): a stream plays when one takes it.
    #[serde(default)]
    pub video: Option<Vec<VideoDecoder>>,
    /// The dynamic ranges the screen shows as they are (a decoder that feeds
    /// it included): `"hdr10"`, `"hdr10plus"`, `"hlg"`, `"dolby_vision"`.
    #[serde(default)]
    pub hdr_types: Option<Vec<String>>,
    /// The Dolby Vision profiles decoded, each with its highest level; used
    /// only when `hdr_types` has `"dolby_vision"`. Without it, they say the
    /// player decodes Dolby Vision and its screen is what doesn't show it.
    #[serde(default)]
    pub dolby_vision_profiles: Option<Vec<DolbyVisionDecoder>>,
    /// Its Dolby Vision decoding shows a stream that also carries HDR10+ (a
    /// hybrid) as Dolby Vision. Many Android TV devices' decoders show black
    /// or freeze on one (androidx/media#1895): without this, hybrids come
    /// without their HDR10+.
    #[serde(default)]
    pub dolby_vision_hdr10plus: bool,
    /// Audio codecs decoded on the device, by FFmpeg's names.
    #[serde(default)]
    pub audio_decode: Option<Vec<String>>,
    /// Audio codecs sent as they are to a receiver or a TV.
    #[serde(default)]
    pub audio_passthrough: Option<Vec<String>>,
    /// The most channels the audio output takes: 2, 6, 8.
    #[serde(default)]
    pub audio_channels: Option<u32>,
    /// Subtitle codecs the player shows itself, pictures (PGS…) included.
    #[serde(default)]
    pub subtitle_codecs: Option<Vec<String>>,
    /// The player shows picture subtitles (PGS, VobSub, DVB) itself also
    /// when the file is repackaged or converted, from their own file
    /// (`/api/files/{id}/subtitles/{index}.mks`) next to the HLS stream: it
    /// asks for no `subtitle_index` then, and nothing is drawn onto the
    /// video.
    #[serde(default)]
    pub picture_subtitles_sideload: Option<bool>,
    /// The player switches a file's audio tracks itself.
    #[serde(default)]
    pub audio_track_switching: Option<bool>,
    #[serde(default)]
    pub hls: Option<HlsCaps>,
    #[serde(default)]
    pub display: Option<DisplayCaps>,
    #[serde(default)]
    pub device: Option<DeviceCaps>,
}

/// One video decoder of a version 2 player. Limits left out aren't known
/// and aren't checked.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct VideoDecoder {
    pub codec: String,
    pub hardware: bool,
    /// The profiles it decodes, as ffprobe names them: "Main", "Main 10"…
    pub profiles: Vec<String>,
    pub max_width: Option<i64>,
    pub max_height: Option<i64>,
    pub max_fps: Option<f64>,
    pub max_bit_depth: Option<i64>,
    /// Numbered as ffprobe numbers a stream's `level`.
    pub max_level: Option<i64>,
}

/// A Dolby Vision profile a version 2 player decodes.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DolbyVisionDecoder {
    pub profile: i64,
    pub max_level: Option<i64>,
}

/// What a version 2 player takes of HLS.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct HlsCaps {
    /// Reads multivariant playlists.
    pub master_playlist: bool,
    /// Segment formats: "ts", "fmp4".
    pub segments: Vec<String>,
    /// Moves between a multivariant playlist's variants by itself, as its
    /// connection allows (adaptive bitrate): its transcodes come in several
    /// sizes ([`ladder`]), from `master.m3u8`. Taken from players of either
    /// version.
    pub variants: bool,
}

/// A version 2 player's screen mode.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DisplayCaps {
    pub width: i64,
    pub height: i64,
    pub refresh_rates: Vec<f64>,
}

/// A version 2 player's device.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct DeviceCaps {
    /// "phone", "tablet", "tv" or "unknown".
    pub form: String,
    /// "low", "mid" or "high".
    pub tier: Option<String>,
    pub fire_tv: bool,
    pub model: Option<String>,
}

fn listed(list: &[String], c: &str) -> bool {
    list.iter().any(|x| x.eq_ignore_ascii_case(c))
}

impl Caps {
    fn container(&self, c: &str) -> bool {
        listed(&self.containers, c)
    }
    fn video(&self, c: &str) -> bool {
        listed(&self.video_codecs, c)
    }
    fn audio(&self, c: &str) -> bool {
        listed(&self.audio_codecs, c)
    }

    /// What a converted download for this player is: MP4 for one that opens
    /// MP4 but no Matroska (QuickTime), else Matroska, which keeps more.
    pub fn download_container(&self) -> FileContainer {
        if !self.container("mkv") && self.container("mp4") { FileContainer::Mp4 } else { FileContainer::Matroska }
    }

    /// A version 2 report (`caps_version` 2 or more).
    pub fn is_v2(&self) -> bool {
        self.caps_version.is_some_and(|v| v >= 2)
    }

    /// The player takes this audio codec as it is: decoded, or (version 2)
    /// passed through to a receiver.
    fn plays_audio(&self, c: &str) -> bool {
        if !self.is_v2() || (self.audio_decode.is_none() && self.audio_passthrough.is_none()) {
            return self.audio(c);
        }
        [&self.audio_decode, &self.audio_passthrough].into_iter().flatten().any(|list| listed(list, c))
    }

    /// The screen shows this dynamic range (version 2 names).
    fn shows(&self, range: &str) -> bool {
        self.hdr_types.as_deref().is_some_and(|types| listed(types, range))
    }

    /// The version 2 player shows these subtitles itself.
    fn shows_subtitles(&self, codec: &str) -> bool {
        self.is_v2() && self.subtitle_codecs.as_deref().is_some_and(|list| listed(list, codec))
    }

    /// The player draws these picture subtitles itself from their own file
    /// (`/api/files/{id}/subtitles/{index}.mks`) next to whatever plays
    /// ([`Caps::picture_subtitles_sideload`]). Read from players of either
    /// version: the web app says so in a version 1 report, which these two
    /// fields change nothing else of.
    pub fn sideloads_pictures(&self, codec: &str) -> bool {
        self.picture_subtitles_sideload == Some(true)
            && self.subtitle_codecs.as_deref().is_some_and(|list| listed(list, codec))
    }

    /// A version 2 player that reads multivariant playlists: its HLS
    /// sessions start at `master.m3u8`, whose `CODECS` can name Dolby
    /// Vision, so its remuxes keep it (docs/API.md, "Dolby Vision
    /// remuxes").
    pub fn master_playlist(&self) -> bool {
        self.is_v2() && self.hls.as_ref().is_some_and(|hls| hls.master_playlist)
    }

    /// The player moves between variants by itself ([`HlsCaps::variants`]).
    pub fn adaptive(&self) -> bool {
        self.hls.as_ref().is_some_and(|hls| hls.variants)
    }
}

/// What the player does with audio, for choosing among a language's tracks.
impl crate::streams::AudioOut for Caps {
    fn plays(&self, codec: &str) -> bool {
        self.plays_audio(codec)
    }
    fn passes_through(&self, codec: &str) -> bool {
        self.is_v2() && self.audio_passthrough.as_deref().is_some_and(|list| listed(list, codec))
    }
    fn switches_tracks(&self) -> bool {
        self.is_v2() && self.audio_track_switching == Some(true)
    }
    fn channels(&self) -> Option<i64> {
        self.audio_channels.filter(|_| self.is_v2()).map(i64::from)
    }
    /// Version 1 players copy what browsers play from MPEG-TS; version 2
    /// players everything MPEG-TS carries that they play.
    fn stream_copies(&self, codec: &str) -> bool {
        if self.is_v2() {
            matches!(codec, "aac" | "mp3" | "ac3" | "eac3") && self.plays_audio(codec)
        } else {
            matches!(codec, "aac" | "mp3") && self.audio(codec)
        }
    }
}

/// How the client wants the media delivered.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackMode {
    /// The cheapest method the client supports: direct play, then remux, then transcode.
    #[default]
    Auto,
    /// The original file, even when the client did not claim support for it.
    Direct,
    /// HLS, copying the video stream when possible.
    Remux,
    /// HLS with the video always re-encoded.
    Transcode,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PlaybackRequest {
    pub item_id: i64,
    #[serde(default)]
    pub file_id: Option<i64>,
    #[serde(default)]
    pub audio_index: Option<i64>,
    #[serde(default)]
    pub subtitle_index: Option<i64>,
    #[serde(default)]
    pub max_bitrate: Option<u32>,
    #[serde(default)]
    pub max_height: Option<u32>,
    #[serde(default)]
    pub start_ms: Option<i64>,
    #[serde(default)]
    pub caps: Caps,
    #[serde(default)]
    pub mode: PlaybackMode,
    /// Identifies the player (a browser tab, a device). A client plays one
    /// thing at a time, so its previous sessions end when it starts a new one.
    #[serde(default)]
    pub client_id: Option<String>,
    /// The player starts this session while the one it plays goes on, to
    /// move to it once its first segments are made, with no wait: the
    /// client's other sessions are left to it to end.
    #[serde(default)]
    pub keep_previous: bool,
    /// The player can't send the Authorization header with its media
    /// requests (a TV's own player): the session's addresses carry a key
    /// of their own instead ([`Session::media_key`]).
    #[serde(default)]
    pub keyed_urls: bool,
    /// The transcoder keeps HDR in transcodes (its `hdr_transcode`
    /// capability): set by the server, never read from the player.
    #[serde(skip)]
    pub hdr_transcodes: bool,
    /// A file made for downloading (`POST /api/conversions`), kept on the
    /// player: set by the server, never read from the player.
    #[serde(skip)]
    pub download: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Method {
    Direct,
    Remux,
    Transcode,
}

#[derive(Debug, Clone, Serialize)]
pub struct SubtitleTrack {
    pub index: i64,
    pub codec: String,
    pub language: Option<String>,
    pub title: Option<String>,
    pub default: bool,
    pub forced: bool,
    pub external: bool,
    pub text_based: bool,
    pub url: Option<String>,
    /// Added by someone (uploaded, or from OpenSubtitles; see
    /// [`crate::subtitles`]), external too.
    pub added: bool,
    /// An added track this user may delete (an administrator, or whoever
    /// added it); left out otherwise.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub deletable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlaybackInfo {
    pub session_id: String,
    pub method: Method,
    pub url: String,
    pub hls: bool,
    pub file_id: i64,
    pub duration_ms: i64,
    pub audio_index: Option<i64>,
    pub subtitle_index: Option<i64>,
    pub burn_subtitles: bool,
    /// The audio is converted, also when the video is copied (a direct stream).
    pub audio_converted: bool,
    pub subtitles: Vec<SubtitleTrack>,
    /// Why the file isn't played as it is, one short sentence each.
    pub reasons: Vec<String>,
    /// The same, in one line.
    pub reason: Option<String>,
    /// The quality a transcode of the original aims for, by the players' id
    /// ("30m"…, see [`QUALITIES`]): `None` when a quality was asked for or the
    /// video isn't converted.
    pub auto_quality: Option<String>,
    /// What the screen gets of HDR video, by name ([`video_range_shown`]):
    /// "Dolby Vision P8.1", "HDR10 (Dolby Vision base layer)", "SDR
    /// (tone-mapped)"… `None` for SDR video.
    pub video_range: Option<String>,
    /// The stream comes in several sizes the player moves between by itself
    /// ([`HlsCaps::variants`]), from `master.m3u8`: its own Automatic
    /// quality leaves the session be.
    pub adaptive: bool,
    /// The video is converted, and its bitrate can change as it is sent
    /// (`PUT /api/playback/{id}/bitrate`): Automatic's steps need no new
    /// stream (docs/QUALITY.md, "Players that go black between streams").
    pub live_bitrate: bool,
    /// How much later than the file's times (its start taken off, as
    /// subtitles are timed) the stream's timestamps are: [`HLS_PTS_OFFSET_MS`]
    /// for an HLS stream, whatever it was started at; `None` when the file
    /// plays as it is. A player that knows the timestamp of the frame it
    /// shows (hls.js: media time + `initPTS`) draws subtitles at that less
    /// this; its media time alone may be off by the stream's B-frame delay
    /// and more (docs/API.md, "Types": `pts_offset_ms`).
    pub pts_offset_ms: Option<i64>,
}

/// What FFmpeg's MPEG-TS muxer adds to every timestamp of the transcoder's
/// HLS segments, which otherwise keep the file's times from whatever point
/// a process starts at (`-output_ts_offset`).
pub const HLS_PTS_OFFSET_MS: i64 = 1_400;

/// The qualities players offer (apps/web/src/player/quality.ts), highest
/// bitrate first: id, kbps, lines.
pub const QUALITIES: [(&str, u32, u32); 7] = [
    ("30m", 30_000, 2160),
    ("20m", 20_000, 1080),
    ("15m", 15_000, 2160),
    ("12m", 12_000, 1080),
    ("8m", 8_000, 1080),
    ("4m", 4_000, 720),
    ("2m", 2_000, 480),
];

/// What a transcode of the original aims for, instead of the file's own
/// size and bitrate (heavy to make and to send, for no better picture): the
/// closest quality players offer. It keeps the file's resolution and never
/// goes above it (4K stays 4K, a 1920×800 film stays 1080p), with the
/// highest bitrate up to 1.5 times the file's (H.264 needs more than HEVC
/// for the same picture), or that resolution's lowest for a file with fewer
/// bits than any.
pub fn closest_quality(width: Option<i64>, height: Option<i64>, bitrate: Option<i64>) -> (&'static str, u32, u32) {
    closest_quality_within(width, height, bitrate, None)
}

/// [`closest_quality`] for a player whose decoders stop at `most_lines`
/// (a 1080p TV stick gets 1080p for a 4K file).
pub fn closest_quality_within(
    width: Option<i64>,
    height: Option<i64>,
    bitrate: Option<i64>,
    most_lines: Option<u32>,
) -> (&'static str, u32, u32) {
    let lines = quality_lines(width, height);
    let lines = match most_lines {
        Some(most) => [2160, 1080, 720, 480].into_iter().find(|l| *l <= lines && *l <= most).unwrap_or(480),
        None => lines,
    };
    let cap = bitrate.filter(|b| *b > 0).map(|b| b as f64 * 1.5 / 1000.0);
    let mut tier = QUALITIES.iter().filter(|q| q.2 == lines);
    let lowest = tier.clone().next_back();
    tier.find(|q| cap.is_none_or(|cap| f64::from(q.1) <= cap)).or(lowest).copied().unwrap_or(QUALITIES[4])
}

/// The lines qualities are named by for a `width`×`height` video: 2160,
/// 1080, 720 or 480, by its width too (a 1920×800 film is 1080p); 1080 when
/// neither is known.
fn quality_lines(width: Option<i64>, height: Option<i64>) -> u32 {
    let (w, h) = (width.unwrap_or(0), height.unwrap_or(0));
    if w >= 3800 || h >= 2100 {
        2160
    } else if w >= 1800 || h >= 1000 || (w <= 0 && h <= 0) {
        1080
    } else if w >= 1200 || h >= 700 {
        720
    } else {
        480
    }
}

/// The most a conversion of a file with `bitrate` (bits per second) to
/// `lines` aims for, whatever is asked (kbps): half again the file's, as
/// [`closest_quality`] has it, within the bitrates players offer at those
/// lines (the tallest of 2160, 1080, 720 and 480 at or below them). A
/// player asking for far more (an app of the compatibility API asking for
/// its most) gets what the file is worth.
fn most_kbps(lines: u32, bitrate: Option<i64>) -> u32 {
    let tier = [2160, 1080, 720, 480].into_iter().find(|l| *l <= lines).unwrap_or(480);
    let offered = QUALITIES.iter().filter(|q| q.2 == tier).map(|q| q.1);
    let (low, high) = (offered.clone().min().unwrap_or(0), offered.max().unwrap_or(u32::MAX));
    bitrate.filter(|b| *b > 0).map_or(high, |b| u32::try_from(b * 3 / 2000).unwrap_or(u32::MAX).clamp(low, high))
}

/// `GET /api/playback/{session}`: how a session plays, with its transcode's
/// live numbers.
#[derive(Debug, Clone, Serialize)]
pub struct SessionStatus {
    pub session_id: String,
    pub method: Method,
    pub transcode: Option<TranscodeInfo>,
}

#[derive(Debug, Clone)]
pub struct Decision {
    pub method: Method,
    pub video: Option<VideoRequest>,
    pub audio: Option<AudioRequest>,
    pub subtitle: Option<SubtitleBurnRequest>,
    pub audio_index: Option<i64>,
    pub burn_index: Option<i64>,
    pub reasons: Vec<String>,
    /// The quality picked for the original ([`closest_quality`]), by id.
    pub quality: Option<&'static str>,
    /// The transcode's lower renditions, for a player that moves between
    /// them ([`ladder`]).
    pub variants: Vec<VariantRequest>,
}

/// The lines of adaptive bitrate's renditions below a transcode's own size,
/// each a 16:9 picture's (a film wider than 16:9 is made narrower, so as
/// tall as it fits).
const LADDER_LINES: [i64; 4] = [1080, 720, 540, 360];

/// The lower renditions of a transcode of a `width`×`height` video as
/// `top` asks, for a player that moves between them by itself as its
/// connection allows: its own size at the bitrate that size gets
/// ([`default_video_kbps`]) when `top` asks for more (a picked 20 Mbps
/// 1080p also comes at 1080p's 8), then 1080p, 720p, 540p and 360p below
/// its own size, each at the bitrate its size gets and never more than the
/// one above. Below its own size, steps of about half the bits each: one
/// with more than two thirds of the one above is left out.
pub fn ladder(width: Option<i64>, height: Option<i64>, top: &VideoRequest) -> Vec<VariantRequest> {
    let (w, h) = (width.unwrap_or(1920).max(2), height.unwrap_or(1080).max(2));
    // The transcode's own size, as the transcoder fits it.
    let scale = [top.max_width.map(|m| f64::from(m) / w as f64), top.max_height.map(|m| f64::from(m) / h as f64)]
        .into_iter()
        .flatten()
        .fold(1.0f64, f64::min);
    let (w, h) = ((w as f64 * scale) as i64, (h as f64 * scale) as i64);
    let lines = h.max(w * 9 / 16);
    let cap = top.max_bitrate_kbps.unwrap_or(u32::MAX);
    // The top's bitrate, as the transcoder makes it.
    let own = default_video_kbps(w, h);
    let mut above = top.max_bitrate_kbps.map_or(own, |kbps| kbps.max(300));
    let mut rungs = Vec::new();
    if own < above {
        rungs.push(VariantRequest {
            max_width: top.max_width,
            max_height: top.max_height,
            max_bitrate_kbps: Some(own),
        });
        above = own;
    }
    for rung in LADDER_LINES.into_iter().filter(|l| *l < lines * 9 / 10) {
        let width = (rung * 16 / 9 + 1) / 2 * 2;
        let kbps = default_video_kbps(width, rung).min(cap);
        if u64::from(kbps) * 3 > u64::from(above) * 2 {
            continue;
        }
        rungs.push(VariantRequest {
            max_width: u32::try_from(width).ok(),
            max_height: u32::try_from(rung).ok(),
            max_bitrate_kbps: Some(kbps),
        });
        above = kbps;
    }
    rungs
}

/// Why the client cannot show this video's dynamic range as is (the video is
/// then tone-mapped to SDR), or `None` when it can.
fn range_problem(v: &MediaStream, caps: &Caps) -> Option<String> {
    let range = v.dynamic_range();
    if !range.is_hdr() {
        return None;
    }
    let label = v.dynamic_range_label();
    if range == DynamicRange::DolbyVision {
        if caps.dolby_vision {
            return None;
        }
        if v.is_dolby_vision_only() {
            // No HDR10/SDR base layer: colours are wrong without DV processing.
            return Some(format!("{label} converted to SDR."));
        }
        // Profile 7/8: an HDR10 or HLG base layer plays as plain HDR.
        if caps.hdr {
            return None;
        }
    } else if caps.hdr {
        return None;
    }
    Some(format!("{label} converted to SDR."))
}

/// What a version 2 player does with a video stream.
#[derive(Debug, Default, PartialEq)]
struct VideoVerdict {
    /// The file can play as it is, as far as the video goes.
    direct: bool,
    /// The stream can be copied into HLS and play right (Dolby Vision
    /// profile 7 as its HDR10 base layer, say).
    copy: bool,
    /// What a copy does with Dolby Vision.
    dolby_vision: DolbyVisionMode,
    /// A copy leaves HDR10+ out.
    strip_hdr10plus: bool,
    reasons: Vec<String>,
}

impl VideoVerdict {
    fn plays() -> Self {
        VideoVerdict { direct: true, copy: true, ..Default::default() }
    }
    fn no(reason: String) -> Self {
        VideoVerdict { direct: false, copy: false, reasons: vec![reason], ..Default::default() }
    }
}

/// Dolby Vision base-layer compatibility ids (ffprobe's
/// `dv_bl_signal_compatibility_id`).
const DV_BL_HDR10: i64 = 1;
const DV_BL_SDR: i64 = 2;
const DV_BL_HLG: i64 = 4;
/// The Blu-ray's HDR10, profile 7's: HDR10 too in a profile 8 stream (a
/// profile 7 converted elsewhere, its id kept).
const DV_BL_BLU_RAY: i64 = 6;

/// The version 2 rules for a video stream (docs/API.md, "Version 2"):
/// - SDR video needs a decoder entry that takes it: codec, profile (Main 10
///   for 10-bit), size, frame rate and level;
/// - HDR10 needs `hdr10` on the screen, HDR10+ `hdr10plus` or `hdr10`, HLG
///   `hlg`; otherwise it is converted: to the HDR format the screen shows
///   when the player takes that ([`hdr_transcode`], `converted`), else
///   tone-mapped to SDR;
/// - Dolby Vision plays as it is when the screen shows it and the profile
///   is listed. Otherwise profiles 8.1 and 8.4 (and 9, 10) play their
///   HDR10, HLG or SDR base layer by those rules; profile 7, whose
///   enhancement layer Media3 can't decode, is repackaged so its HDR10 base
///   layer plays; profile 5, with no base layer, is converted like the
///   above, never repackaged (an MPEG-TS remux loses the Dolby Vision
///   signalling, which leaves its colours wrong).
///
/// Players that read multivariant playlists ([`Caps::master_playlist`])
/// get Dolby Vision kept in remuxes, named in the playlist's `CODECS`:
/// profile 5 too when the player lists it; profile 7 converted to 8.1 for
/// a player listing 8 but not 7; and Dolby Vision + HDR10+ streams
/// repackaged without their HDR10+ for a player that would decode them as
/// Dolby Vision, never played as they are, unless it says it shows them
/// ([`Caps::dolby_vision_hdr10plus`]; androidx/media#1895). Without a
/// master playlist, those are repackaged as their base layer. A stream
/// whose file says a higher level than its decoder takes, though its size
/// and frame rate need one it takes (a re-encode's 10 for 4K at 24 fps), is
/// repackaged to keep Dolby Vision, named at the level it needs; a
/// download stays the file as it is.
fn video_v2(v: &MediaStream, caps: &Caps, converted: OutputRange, download: bool) -> VideoVerdict {
    let label = v.dynamic_range_label();
    // What the video is converted to when the screen doesn't show it.
    let out = converted.dynamic_range().label();
    let elsewhere = || format!("{label} converted to {out}.");
    match v.dynamic_range() {
        DynamicRange::Sdr => fits(v, caps),
        DynamicRange::Hdr10 if caps.shows("hdr10") => fits(v, caps),
        DynamicRange::Hdr10plus if caps.shows("hdr10plus") || caps.shows("hdr10") => fits(v, caps),
        DynamicRange::Hlg if caps.shows("hlg") => fits(v, caps),
        DynamicRange::Hdr10 | DynamicRange::Hdr10plus | DynamicRange::Hlg => VideoVerdict::no(elsewhere()),
        DynamicRange::DolbyVision => {
            let profile = v.dv_profile.unwrap_or(0);
            let decoder_of = |profile: i64| {
                caps.dolby_vision_profiles
                    .iter()
                    .flatten()
                    .find(|d| d.profile == profile)
                    .filter(|_| caps.shows("dolby_vision"))
            };
            let takes_level = |d: &DolbyVisionDecoder| match (v.dv_level, d.max_level) {
                (Some(level), Some(max)) => level <= max,
                _ => true,
            };
            // The level a repackaged stream is named with (the transcoder's
            // master playlist): the one its size and frame rate need, where
            // the file says more.
            let takes_named = |d: &DolbyVisionDecoder| match (v.dv_level_named(), d.max_level) {
                (Some(level), Some(max)) => level <= max,
                _ => true,
            };
            let decoder = decoder_of(profile);
            let level_ok = decoder.is_none_or(takes_level);
            // Remuxes keep Dolby Vision only where a master playlist names it.
            let signal = caps.master_playlist() && v.codec == "hevc";
            // HDR10+ goes from Dolby Vision the player doesn't show with it.
            let strip = v.hdr10plus == Some(true) && !caps.dolby_vision_hdr10plus;
            // What plays when Dolby Vision itself doesn't: the base layer.
            let compat = match v.dv_bl_compat {
                Some(DV_BL_BLU_RAY) if profile == 8 => Some(DV_BL_HDR10),
                compat => compat.or((profile == 8).then_some(DV_BL_HDR10)),
            };
            let base_layer = base_layer_name(v, caps);
            // No base layer to fall back on: a Dolby Vision screen whose
            // player doesn't decode the profile is told apart from the rest.
            let needs_dolby_vision = || {
                if caps.shows("dolby_vision") {
                    format!("{} converted to {out}.", v.dolby_vision_label())
                } else {
                    format!("{label} converted to {out}.")
                }
            };
            let base = match (profile, compat) {
                (5, _) => VideoVerdict::no(needs_dolby_vision()),
                (7, _) if caps.shows("hdr10") => {
                    let mut base = fits(v, caps);
                    if base.copy {
                        base.direct = false;
                        base.reasons.push(format!("{label} plays as {base_layer}."));
                    }
                    base
                }
                (7, _) => VideoVerdict::no(elsewhere()),
                (_, Some(DV_BL_HDR10)) if caps.shows("hdr10") => fits(v, caps),
                (_, Some(DV_BL_HLG)) if caps.shows("hlg") => fits(v, caps),
                (_, Some(DV_BL_SDR)) => fits(v, caps),
                (_, Some(DV_BL_HDR10 | DV_BL_HLG)) => VideoVerdict::no(elsewhere()),
                _ => VideoVerdict::no(needs_dolby_vision()),
            };
            match decoder {
                // Many Dolby Vision decoders show black, or freeze, when
                // HDR10+ comes with it (androidx/media#1895): never as it is.
                Some(_) if level_ok && strip => {
                    if signal {
                        return VideoVerdict {
                            direct: false,
                            copy: true,
                            dolby_vision: DolbyVisionMode::Signal,
                            strip_hdr10plus: true,
                            reasons: vec![format!("{label} plays without HDR10+.")],
                        };
                    }
                    let mut base = base;
                    if base.direct {
                        base.direct = false;
                        base.reasons.push(format!("{label} plays as {base_layer}."));
                    }
                    base
                }
                Some(_) if level_ok => VideoVerdict {
                    direct: true,
                    copy: base.copy || signal,
                    dolby_vision: if signal { DolbyVisionMode::Signal } else { DolbyVisionMode::Copy },
                    ..Default::default()
                },
                // As it is, the decoder refuses the level the file says
                // and its base layer plays: repackaged, it's named at the
                // level the stream needs, which the decoder takes.
                Some(d) if signal && !download && takes_named(d) => VideoVerdict {
                    direct: false,
                    copy: true,
                    dolby_vision: DolbyVisionMode::Signal,
                    strip_hdr10plus: strip,
                    reasons: vec![format!(
                        "Dolby Vision level {} relabelled {}.",
                        v.dv_level.unwrap_or_default(),
                        v.dv_level_named().unwrap_or_default(),
                    )],
                },
                Some(_) if !base.direct => {
                    let mut base = base;
                    base.reasons
                        .insert(0, format!("Dolby Vision level {} not supported.", v.dv_level.unwrap_or_default()));
                    base
                }
                // Profile 7 for a player that decodes 8.1: converted while
                // repackaged (rather than its base layer alone).
                _ if profile == 7 && signal && decoder_of(8).is_some_and(takes_named) => {
                    // What profile 8.1 can't carry, and the HDR10+ the player doesn't take with it.
                    let left_out = match (v.dv_el == Some(DolbyVisionLayer::Fel), strip) {
                        (true, true) => ", enhancement layer and HDR10+ dropped",
                        (true, false) => ", enhancement layer dropped",
                        (false, true) => ", HDR10+ dropped",
                        (false, false) => "",
                    };
                    VideoVerdict {
                        direct: false,
                        copy: true,
                        dolby_vision: DolbyVisionMode::To81,
                        strip_hdr10plus: strip,
                        reasons: vec![format!("{label} converted to P8.1{left_out}.")],
                    }
                }
                _ => base,
            }
        }
    }
}

/// What a Dolby Vision stream's base layer shows as on its own: HDR10 (with
/// its HDR10+, on a screen that shows HDR10+), HLG or SDR.
fn base_layer_name(v: &MediaStream, caps: &Caps) -> &'static str {
    match v.dv_bl_compat {
        Some(DV_BL_HLG) => "HLG",
        Some(DV_BL_SDR) => "SDR",
        _ if v.hdr10plus == Some(true) && (caps.shows("hdr10plus") || !caps.is_v2()) => "HDR10+",
        _ => "HDR10",
    }
}

/// How a video reaches the player: as it is, copied into HLS (with what
/// the remux does with Dolby Vision, and whether it leaves HDR10+ out), or
/// converted to a dynamic range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    Direct,
    Copy { dolby_vision: DolbyVisionMode, hdr10plus_stripped: bool },
    Converted(DynamicRange),
}

impl Delivery {
    /// From what the transcoder made of the job's video.
    pub fn of_plan(plan: &VideoPlan) -> Delivery {
        if plan.mode == VideoMode::Copy {
            Delivery::Copy {
                dolby_vision: plan.dolby_vision.unwrap_or_default(),
                hdr10plus_stripped: plan.hdr10plus_stripped,
            }
        } else {
            // Transcoders that don't say tone-map HDR to SDR.
            Delivery::Converted(plan.dynamic_range_out.unwrap_or(DynamicRange::Sdr))
        }
    }
}

/// The player decodes this Dolby Vision stream as Dolby Vision when it
/// gets it as it is: a version 2 player that lists the profile, at the
/// stream's level, with a Dolby Vision screen; a version 1 player that
/// decodes Dolby Vision (single-layer profiles only: none decodes 7's two).
fn decodes_dolby_vision(v: &MediaStream, caps: &Caps) -> bool {
    if !caps.is_v2() {
        return caps.dolby_vision && v.dv_profile != Some(7);
    }
    let profile = v.dv_profile.unwrap_or(0);
    caps.shows("dolby_vision")
        && caps
            .dolby_vision_profiles
            .iter()
            .flatten()
            .any(|d| d.profile == profile && v.dv_level.zip(d.max_level).is_none_or(|(level, max)| level <= max))
}

/// What the screen gets of an HDR video, by name, for the player's details
/// (`PlaybackInfo::video_range`): "Dolby Vision P8.1", "HDR10 (Dolby Vision
/// base layer)", "Dolby Vision P8.1 (converted from P7 FEL)", "HDR10 (this
/// screen doesn't show HDR10+)", "SDR (tone-mapped)"… `None` for SDR video.
/// Version 1 players don't say which HDR formats their screen shows: their
/// HDR10+ is named as it is.
pub fn video_range_shown(v: &MediaStream, caps: &Caps, delivery: Delivery) -> Option<String> {
    let range = v.dynamic_range();
    if !range.is_hdr() {
        return None;
    }
    let shows_hdr10plus = caps.shows("hdr10plus") || !caps.is_v2();
    let dolby_vision = v.dolby_vision_label();
    let shown = match delivery {
        Delivery::Converted(DynamicRange::Sdr) => "SDR (tone-mapped)".to_owned(),
        Delivery::Converted(out) => {
            let base = range == DynamicRange::DolbyVision
                && match out {
                    DynamicRange::Hlg => v.dv_bl_compat == Some(DV_BL_HLG),
                    _ => !v.is_dolby_vision_only() && !matches!(v.dv_bl_compat, Some(DV_BL_HLG | DV_BL_SDR)),
                };
            match (range, out) {
                (DynamicRange::Hdr10plus, DynamicRange::Hdr10) => "HDR10 (HDR10+ left out)".to_owned(),
                _ if base => format!("{} (Dolby Vision base layer)", out.label()),
                (DynamicRange::Hdr10, DynamicRange::Hdr10) | (DynamicRange::Hlg, DynamicRange::Hlg) => {
                    out.label().to_owned()
                }
                _ => format!("{} (converted from {})", out.label(), v.dynamic_range_label()),
            }
        }
        Delivery::Direct | Delivery::Copy { .. } if range != DynamicRange::DolbyVision => match range {
            DynamicRange::Hdr10plus if !shows_hdr10plus => "HDR10 (this screen doesn't show HDR10+)".to_owned(),
            other => other.label().to_owned(),
        },
        Delivery::Direct if decodes_dolby_vision(v, caps) => {
            video_range_label(range, v.dv_profile, v.dv_bl_compat, v.dv_el, false)
        }
        Delivery::Copy { dolby_vision: DolbyVisionMode::To81, hdr10plus_stripped } => {
            let from = video_range_label(range, v.dv_profile, v.dv_bl_compat, v.dv_el, false);
            let from = from.trim_start_matches("Dolby Vision ");
            let stripped = if hdr10plus_stripped { ", HDR10+ left out" } else { "" };
            format!("Dolby Vision P8.1 (converted from {from}{stripped})")
        }
        Delivery::Copy { dolby_vision: DolbyVisionMode::Signal, hdr10plus_stripped } => {
            let kept = video_range_label(range, v.dv_profile, v.dv_bl_compat, v.dv_el, false);
            if hdr10plus_stripped { format!("{kept} (HDR10+ left out)") } else { kept }
        }
        // Dolby Vision unnamed, or not decoded: the base layer.
        Delivery::Direct | Delivery::Copy { .. } => {
            if v.is_dolby_vision_only() {
                // No base layer of its own: only a direct play the player
                // insisted on gets here.
                format!("{dolby_vision} not decoded (wrong colours)")
            } else {
                format!("{} (Dolby Vision base layer, as {})", base_layer_name(v, caps), base_layer_why(v, caps))
            }
        }
    };
    Some(shown)
}

/// Why a Dolby Vision stream the player gets as it is, or copied, shows as
/// its base layer: the screen, the player, or a remux that doesn't name its
/// Dolby Vision (the player reads no master playlist, the transcoder writes
/// none, or the stream isn't HEVC). A player that lists profiles but no
/// Dolby Vision screen decodes it: its screen is what's missing. One that
/// lists none is told neither: Kodi and Roku can't tell their screen from
/// their decoders.
fn base_layer_why(v: &MediaStream, caps: &Caps) -> String {
    let profile = v.dv_profile.unwrap_or(0);
    if decodes_dolby_vision(v, caps) {
        return if caps.is_v2() && v.hdr10plus == Some(true) && !caps.dolby_vision_hdr10plus {
            "Dolby Vision with HDR10+ doesn't play here".to_owned()
        } else {
            "its remux doesn't name Dolby Vision".to_owned()
        };
    }
    if !caps.is_v2() {
        return if caps.dolby_vision {
            format!("this player doesn't decode profile {profile}")
        } else {
            "Dolby Vision isn't shown here".to_owned()
        };
    }
    let decoders = caps.dolby_vision_profiles.as_deref().unwrap_or_default();
    if !caps.shows("dolby_vision") {
        return if decoders.is_empty() {
            "Dolby Vision isn't shown here".to_owned()
        } else {
            "this screen doesn't show Dolby Vision".to_owned()
        };
    }
    match decoders.iter().find(|d| d.profile == profile).and_then(|d| v.dv_level.zip(d.max_level)) {
        Some((level, max)) if level > max => format!("level {level} is above this player's {max}"),
        _ => format!("this player doesn't decode profile {profile}"),
    }
}

/// What a version 2 player gets when its HDR video is converted (docs/API.md,
/// "HDR transcodes"): HEVC Main 10 in the HDR format its screen shows, when
/// a hardware decoder takes HEVC Main 10 at the video's frame rate. HLG
/// stays HLG on a screen that shows HLG, everything else goes to HDR10 (PQ)
/// on a screen that shows HDR10 or HDR10+, and to HLG on one that shows
/// only HLG. Otherwise, and for SDR video (Dolby Vision on an SDR base
/// layer too), SDR.
fn hdr_transcode(v: &MediaStream, caps: &Caps) -> OutputRange {
    let range = v.dynamic_range();
    let dv_base = |compat: i64| range == DynamicRange::DolbyVision && v.dv_bl_compat == Some(compat);
    if !range.is_hdr() || dv_base(DV_BL_SDR) || hevc10_lines(caps, v.frame_rate).is_none() {
        return OutputRange::Sdr;
    }
    let hlg_source = range == DynamicRange::Hlg || dv_base(DV_BL_HLG);
    let (pq, hlg) = (caps.shows("hdr10") || caps.shows("hdr10plus"), caps.shows("hlg"));
    match (hlg_source, pq, hlg) {
        (true, _, true) => OutputRange::Hlg,
        (_, true, _) => OutputRange::Hdr10,
        (false, false, true) => OutputRange::Hlg,
        _ => OutputRange::Sdr,
    }
}

/// The most lines a version 2 player's hardware HEVC Main 10 decoders take
/// at `fps` (what HDR transcodes are made in): `None` without one, `Some(None)`
/// when one doesn't say.
fn hevc10_lines(caps: &Caps, fps: Option<f64>) -> Option<Option<u32>> {
    let decoders: Vec<&VideoDecoder> = caps
        .video
        .iter()
        .flatten()
        .filter(|d| d.codec.eq_ignore_ascii_case("hevc") && d.hardware)
        .filter(|d| d.max_bit_depth.is_none_or(|b| b >= 10))
        .filter(|d| d.profiles.is_empty() || listed(&d.profiles, "Main 10"))
        .filter(|d| fps.zip(d.max_fps).is_none_or(|(fps, max)| fps <= max + 0.01))
        .collect();
    if decoders.is_empty() {
        return None;
    }
    let lines =
        decoders.iter().map(|d| d.max_height.zip(d.max_width).map(|(h, w)| h.min(w))).collect::<Option<Vec<_>>>();
    Some(lines.and_then(|lines| lines.into_iter().max()).and_then(|l| u32::try_from(l).ok()))
}

/// Whether one of a version 2 player's decoders takes the stream: any one
/// entry of its codec, each limit it gives checked.
fn fits(v: &MediaStream, caps: &Caps) -> VideoVerdict {
    let Some(decoders) = caps.video.as_deref() else {
        // A version 2 report without decoders: its codec list.
        return if caps.video(&v.codec) {
            VideoVerdict::plays()
        } else {
            VideoVerdict::no(format!("{} video not supported.", codec_label(&v.codec)))
        };
    };
    let entries: Vec<&VideoDecoder> = decoders.iter().filter(|d| d.codec.eq_ignore_ascii_case(&v.codec)).collect();
    if entries.is_empty() {
        return VideoVerdict::no(format!("{} video not supported.", codec_label(&v.codec)));
    }
    let depth = bit_depth(v);
    if entries.iter().any(|d| takes(d, v, depth)) {
        return VideoVerdict::plays();
    }
    let mut what = vec![codec_label(&v.codec).to_owned()];
    if let (Some(w), Some(h)) = (v.width, v.height) {
        what.push(format!("{w}×{h}"));
    }
    if let Some(fps) = v.frame_rate {
        what.push(format!("{} fps", trim_fps(fps)));
    }
    what.push(format!("{depth}-bit"));
    VideoVerdict::no(format!("{} not supported.", what.join(" ")))
}

/// A frame rate as people write it: 23.976, 25, 59.94.
fn trim_fps(fps: f64) -> String {
    let s = format!("{fps:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_owned()
}

/// Profiles both ffprobe and Android name the same way: a stream's profile
/// missing from a decoder's list refuses it only when it is one of these.
const KNOWN_PROFILES: &[&str] = &[
    "main",
    "main 10",
    "main still picture",
    "rext",
    "baseline",
    "constrained baseline",
    "high",
    "high 10",
    "high 4:2:2",
    "high 4:4:4 predictive",
    "profile 0",
    "profile 1",
    "profile 2",
    "profile 3",
    "professional",
];

fn takes(d: &VideoDecoder, v: &MediaStream, depth: i64) -> bool {
    if let (Some(w), Some(h), Some(max_w), Some(max_h)) = (v.width, v.height, d.max_width, d.max_height) {
        // A decoder that takes 1920×1080 takes 1080×1920 too.
        if !(w <= max_w && h <= max_h) && !(h <= max_w && w <= max_h) {
            return false;
        }
    }
    if let (Some(fps), Some(max)) = (v.frame_rate, d.max_fps)
        && fps > max + 0.01
    {
        return false;
    }
    if d.max_bit_depth.is_some_and(|max| depth > max) {
        return false;
    }
    if let (Some(level), Some(max)) = (v.level, d.max_level)
        && level > 0
        && level > max
    {
        return false;
    }
    if !d.profiles.is_empty() {
        let has = |p: &str| d.profiles.iter().any(|x| x.eq_ignore_ascii_case(p));
        if let Some(profile) = v.profile.as_deref()
            && !has(profile)
            && KNOWN_PROFILES.contains(&profile.to_ascii_lowercase().as_str())
        {
            return false;
        }
        // 10-bit HEVC needs Main 10 even when ffprobe gave no profile.
        if v.codec == "hevc" && depth >= 10 && !has("Main 10") && !has("Rext") {
            return false;
        }
    }
    true
}

/// The stream's bit depth, else from its pixel format ("yuv420p10le") or
/// profile ("Main 10").
fn bit_depth(v: &MediaStream) -> i64 {
    if let Some(depth) = v.bit_depth {
        return depth;
    }
    let from_pix_fmt = v.pix_fmt.as_deref().and_then(|f| {
        let f = f.trim_end_matches("le").trim_end_matches("be");
        let digits = &f[f.trim_end_matches(|c: char| c.is_ascii_digit()).len()..];
        digits.parse::<i64>().ok().filter(|d| matches!(d, 10 | 12))
    });
    from_pix_fmt.unwrap_or(if v.profile.as_deref().is_some_and(|p| p.contains("10")) { 10 } else { 8 })
}

/// The codec and channels audio is converted to: E-AC-3 5.1 for a
/// multichannel output that takes E-AC-3 (decoded or passed through), else
/// AC-3 5.1, else AAC with as many channels as the output takes (up to
/// 5.1). Stereo sources, stereo outputs and players that don't say how many
/// channels they have (the web) get stereo AAC.
fn converted_audio(caps: &Caps, source: &MediaStream) -> (AudioCodec, u32) {
    let output = caps.audio_channels.unwrap_or(2).max(1);
    let surround = output >= 6 && source.channels.unwrap_or(2) > 2;
    if surround && caps.plays_audio("eac3") {
        (AudioCodec::Eac3, 6)
    } else if surround && caps.plays_audio("ac3") {
        (AudioCodec::Ac3, 6)
    } else {
        // The transcoder never makes more channels than the source has.
        (AudioCodec::Aac, output.clamp(2, 6))
    }
}

/// The most lines a version 2 player's H.264 decoders take (what
/// transcodes are made in), when it says.
fn h264_lines(caps: &Caps) -> Option<u32> {
    caps.video
        .iter()
        .flatten()
        .filter(|d| d.codec.eq_ignore_ascii_case("h264"))
        .map(|d| d.max_height.zip(d.max_width).map(|(h, w)| h.min(w)))
        .collect::<Option<Vec<i64>>>()
        .and_then(|lines| lines.into_iter().max())
        .and_then(|l| u32::try_from(l).ok())
}

/// The most lines of a quality the version 2 player's screen shows: 1080p
/// on a 1080×2316 phone and on a 1440×3088 one, 2160p on a 4K TV.
fn screen_lines(caps: &Caps) -> Option<u32> {
    let display = caps.display.as_ref().filter(|_| caps.is_v2())?;
    let lines = display.width.min(display.height);
    (lines > 0).then(|| [2160, 1080, 720, 480].into_iter().find(|l| i64::from(*l) <= lines).unwrap_or(480))
}

/// Video codecs HLS can carry in MPEG-TS segments, which players unpack
/// (hls.js turns them into fragmented MP4 for the browser).
fn hls_copyable(codec: &str) -> bool {
    matches!(codec, "h264" | "hevc")
}

/// Containers the browser can play natively (used when a file was never probed).
fn web_container(container: &str) -> bool {
    matches!(container, "mp4" | "m4v" | "webm" | "mp3" | "m4a" | "flac" | "ogg" | "wav" | "aac")
}

pub fn decide(file: &FileRow, info: Option<&MediaInfo>, streams: &[MediaStream], req: &PlaybackRequest) -> Decision {
    let caps = &req.caps;
    let container = file.container.as_deref().unwrap_or_default();
    let mut reasons = Vec::new();

    let Some(info) = info else {
        // Never probed (transcoder was unavailable during the scan).
        let direct = match req.mode {
            PlaybackMode::Direct => true,
            PlaybackMode::Auto => caps.container(container) || web_container(container),
            PlaybackMode::Remux | PlaybackMode::Transcode => false,
        };
        return Decision {
            method: if direct { Method::Direct } else { Method::Transcode },
            video: (!direct).then(VideoRequest::default),
            audio: (!direct).then(AudioRequest::default),
            subtitle: None,
            audio_index: None,
            burn_index: None,
            reasons: if direct { vec![] } else { vec!["File not analyzed yet.".into()] },
            quality: None,
            variants: Vec::new(),
        };
    };

    let video = info.video();
    let default_audio = info.default_audio();
    let audio = req.audio_index.and_then(|i| info.stream(i).filter(|s| s.kind == StreamKind::Audio)).or(default_audio);

    let v2 = caps.is_v2();
    let burn_stream =
        req.subtitle_index.and_then(|i| streams.iter().find(|s| s.index == i && s.kind == StreamKind::Subtitle));
    let pictures = burn_stream.filter(|s| !s.text_based.unwrap_or(true) && video.is_some());
    // Picture subtitles a version 2 player shows itself need drawing only
    // when the file doesn't play as it is (HLS segments carry no subtitles),
    // whether that is the file's or the player's choice (Remux or Transcode
    // asked for); see below.
    let player_draws = pictures.is_some_and(|s| !s.external && caps.shows_subtitles(&s.codec));
    let mut burn = pictures.filter(|_| !player_draws);
    let burn_reason = |s: &MediaStream| format!("{} subtitles burned in.", codec_label(&s.codec));
    if let Some(s) = burn {
        reasons.push(burn_reason(s));
    }

    let container_ok = caps.container(container);
    if !container_ok {
        reasons.push(format!("{} not supported.", container.to_uppercase()));
    }
    // Version 2: whether the video plays as it is, and whether it plays
    // copied into HLS (a profile 7 base layer, say), which version 1 takes
    // to be the same.
    let mut video_copy_ok = true;
    // What a copy does with Dolby Vision and HDR10+ (version 2).
    let mut copy_dolby_vision = DolbyVisionMode::Copy;
    let mut copy_strip_hdr10plus = false;
    // What converted HDR video becomes (version 2, with a transcoder that
    // keeps HDR): HEVC Main 10 in HDR10 or HLG, else SDR. Picture subtitles
    // drawn onto it (converted video always has them drawn) make it SDR.
    let hdr_out = match video {
        Some(v) if v2 && req.hdr_transcodes && pictures.is_none() => hdr_transcode(v, caps),
        _ => OutputRange::Sdr,
    };
    let video_ok = match video {
        None => true,
        Some(v) if v2 => {
            let verdict = video_v2(v, caps, hdr_out, req.download);
            reasons.extend(verdict.reasons);
            // A download's file carries what its stream would only hint at
            // (a Dolby Vision profile the device doesn't decode, kept in
            // Matroska): its video is copied only when it plays as it is.
            video_copy_ok = if req.download { verdict.direct } else { verdict.copy };
            copy_dolby_vision = verdict.dolby_vision;
            copy_strip_hdr10plus = verdict.strip_hdr10plus;
            verdict.direct
        }
        Some(v) => {
            let mut ok = true;
            if !caps.video(&v.codec) {
                reasons.push(format!("{} video not supported.", codec_label(&v.codec)));
                ok = false;
            }
            if let Some(problem) = range_problem(v, caps) {
                reasons.push(problem);
                ok = false;
            }
            if v.codec == "h264" && v.bit_depth.unwrap_or(8) > 8 {
                reasons.push("10-bit H.264 not supported.".into());
                ok = false;
            }
            video_copy_ok = ok;
            // Version 1 remuxes never name Dolby Vision, and a stream with
            // no base layer of its own shows the wrong colours unnamed: it
            // plays only as it is, else converted.
            if ok && v.is_dolby_vision_only() {
                video_copy_ok = false;
                reasons.push(format!("{} converted to SDR.", v.dynamic_range_label()));
            }
            ok
        }
    };
    // Played or copied as they are, its frames stutter: about one in five is
    // shown out of turn or dropped.
    let video_ok = if video.is_some_and(|v| v.decode_order_times) {
        reasons.push("Frame timing fixed.".into());
        video_copy_ok = false;
        false
    } else {
        video_ok
    };
    let audio_ok = match audio {
        None => true,
        Some(a) if caps.plays_audio(&a.codec) => true,
        Some(a) => {
            reasons.push(format!("{} audio not supported.", codec_label(&a.codec)));
            false
        }
    };
    let audio_is_default = match (audio, default_audio) {
        (Some(a), Some(d)) => a.index == d.index,
        _ => true,
    } || (v2 && caps.audio_track_switching == Some(true));
    if !audio_is_default {
        reasons.push("Audio track switching not supported.".into());
    }
    let bitrate_ok = req.max_bitrate.is_none_or(|max| file.bitrate.unwrap_or(0) <= max as i64 * 1000);
    let size_ok = req.max_height.is_none_or(|max| video.and_then(|v| v.height).unwrap_or(0) <= max as i64);
    if !bitrate_ok || !size_ok {
        reasons.push("Lower quality picked.".into());
    }

    let audio_index = audio.map(|a| a.index);
    let direct_ok = container_ok && video_ok && audio_ok && audio_is_default && bitrate_ok && size_ok && burn.is_none();
    let direct = match req.mode {
        PlaybackMode::Auto => direct_ok,
        // The user knows better (e.g. a browser that plays more than it
        // admits); image subtitles and alternate audio are lost.
        PlaybackMode::Direct => true,
        PlaybackMode::Remux | PlaybackMode::Transcode => false,
    };
    if player_draws && !direct {
        // Not as it is after all (the file doesn't play so, or a stream was
        // asked for): the subtitles are drawn onto the video.
        burn = pictures;
        if let Some(s) = burn {
            reasons.insert(0, burn_reason(s));
        }
    }
    if direct {
        return Decision {
            method: Method::Direct,
            video: None,
            audio: None,
            subtitle: None,
            audio_index,
            burn_index: None,
            reasons: Vec::new(),
            quality: None,
            variants: Vec::new(),
        };
    }

    match req.mode {
        PlaybackMode::Remux if direct_ok => reasons.push("Remux requested.".into()),
        PlaybackMode::Transcode => reasons.push("Transcode requested.".into()),
        _ => {}
    }
    let mut quality = None;
    let video_req = video.map(|v| {
        // Whatever the client plays as it is goes through untouched, like
        // Plex's Direct Stream: only the audio or the container changes.
        let copy = req.mode != PlaybackMode::Transcode
            && video_copy_ok
            && hls_copyable(&v.codec)
            && v.interlaced != Some(true)
            && bitrate_ok
            && size_ok
            && burn.is_none();
        // HDR kept, in HEVC Main 10, unless subtitles are drawn on it.
        let hdr = if copy || burn.is_some() { OutputRange::Sdr } else { hdr_out };
        // The original, converted: at the closest quality players offer.
        // A version 2 player's H.264 decoders bound what it is converted to
        // (its HEVC Main 10 ones when HDR is kept).
        let most_lines = match (v2 && !copy, hdr) {
            (false, _) => None,
            (true, OutputRange::Sdr) => h264_lines(caps),
            (true, _) => hevc10_lines(caps, v.frame_rate).flatten(),
        };
        // A download is watched on the player's screen: made no bigger.
        let most_lines = match (most_lines, screen_lines(caps).filter(|_| req.download && !copy)) {
            (Some(most), Some(screen)) => Some(most.min(screen)),
            (most, screen) => most.or(screen),
        };
        let (max_height, max_bitrate) = if copy || req.max_bitrate.is_some() || req.max_height.is_some() {
            let max_height = match (req.max_height, most_lines) {
                (Some(asked), Some(most)) => Some(asked.min(most)),
                (None, Some(most)) if v.height.is_some_and(|h| h > i64::from(most)) => Some(most),
                (asked, _) => asked,
            };
            (max_height, req.max_bitrate)
        } else {
            let (id, kbps, lines) = closest_quality_within(v.width, v.height, file.bitrate, most_lines);
            quality = Some(id);
            (Some(lines), Some(kbps))
        };
        // A quality asked for is what the conversion aims for, up to what
        // the file is worth at its lines.
        let max_bitrate = match max_bitrate.filter(|_| !copy) {
            Some(kbps) => {
                let lines = max_height.unwrap_or_else(|| quality_lines(v.width, v.height));
                Some(kbps.min(most_kbps(lines, file.bitrate)))
            }
            None => max_bitrate,
        };
        // A download gets no more than half again the original's, not even
        // its size's lowest quality: a file with fewer bits would come out
        // several times as big.
        let max_bitrate = match file.bitrate.filter(|b| *b > 0 && req.download && !copy) {
            Some(bits) => {
                let most = u32::try_from(bits * 3 / 2000).unwrap_or(u32::MAX);
                Some(max_bitrate.map_or(most, |max| max.min(most)))
            }
            None => max_bitrate,
        };
        VideoRequest {
            stream_index: Some(v.index),
            mode: if copy { VideoMode::Copy } else { VideoMode::Transcode },
            codec: if hdr == OutputRange::Sdr { VideoCodec::H264 } else { VideoCodec::Hevc },
            max_width: None,
            max_height,
            max_bitrate_kbps: max_bitrate.map(|b| b.saturating_sub(192).max(500)),
            dolby_vision: if copy { copy_dolby_vision } else { DolbyVisionMode::Copy },
            strip_hdr10plus: copy && copy_strip_hdr10plus,
            dynamic_range: hdr,
        }
    });
    let audio_req = audio.map(|a| {
        // As the choice of the track counted on (streams::fit).
        let copy = crate::streams::AudioOut::stream_copies(caps, &a.codec);
        let (codec, channels) = converted_audio(caps, a);
        AudioRequest {
            stream_index: Some(a.index),
            mode: if copy { AudioMode::Copy } else { AudioMode::Transcode },
            codec,
            channels: Some(channels),
            bitrate_kbps: None,
        }
    });
    let remux = match &video_req {
        Some(v) => v.mode == VideoMode::Copy,
        None => audio_req.as_ref().is_some_and(|a| a.mode == AudioMode::Copy),
    };
    // Smaller sizes too, for a player that moves between them as its
    // connection allows; a download is one file.
    let variants = match (&video_req, video) {
        (Some(v), Some(src)) if v.mode == VideoMode::Transcode && caps.adaptive() && !req.download => {
            ladder(src.width, src.height, v)
        }
        _ => Vec::new(),
    };
    let subtitle = burn.map(|s| {
        if s.external {
            // An added track's file is in the data folder: the caller, which
            // knows where that is, names it ([`crate::subtitles::AddedSub::path`]).
            let path = file.external_subs().into_iter().find(|e| e.index == s.index).map(|e| e.path);
            SubtitleBurnRequest { stream_index: None, external_path: path }
        } else {
            SubtitleBurnRequest { stream_index: Some(s.index), external_path: None }
        }
    });
    Decision {
        method: if remux { Method::Remux } else { Method::Transcode },
        video: video_req,
        audio: audio_req,
        burn_index: burn.map(|s| s.index),
        subtitle,
        audio_index,
        reasons,
        quality,
        variants,
    }
}

/// [`decide`] for a play that names no audio track, with the track `pick`
/// chooses for the player (`streams::resolve`): with the file as it is if it
/// plays so, and else again for a stream, whose copy of a track beats a
/// conversion of the one the file would have played (a DTS default next to
/// AC-3, once Automatic quality lowers the video): the transcoder converts
/// no audio it needn't.
pub fn decide_with_audio(
    file: &FileRow,
    info: Option<&MediaInfo>,
    streams: &[MediaStream],
    req: &mut PlaybackRequest,
    pick: impl Fn(&dyn crate::streams::AudioOut) -> Option<i64>,
) -> Decision {
    req.audio_index = pick(&req.caps);
    let decision = decide(file, info, streams, req);
    if decision.method == Method::Direct {
        return decision;
    }
    let streamed = pick(&crate::streams::Streamed(&req.caps));
    if streamed == req.audio_index {
        return decision;
    }
    req.audio_index = streamed;
    decide(file, info, streams, req)
}

/// The picture subtitle track whose `.mks` the player of `req` will ask for
/// (it draws them itself, [`Caps::sideloads_pictures`]), so the server has
/// it copied out of its file at once: the track named in the request, else
/// the one the user's choices pick (`chosen`, `streams::resolve`), when it
/// is pictures the player draws and nothing burns them in. Not for a track
/// in a file played as it is: a player that reads it from the file (the
/// Android app, Kodi) needs no `.mks`, and one that doesn't asks for it at
/// once anyway. Players change the track as they play: the `.mks` request
/// starts the copy then.
pub fn picture_to_prepare(
    streams: &[MediaStream],
    req: &PlaybackRequest,
    decision: &Decision,
    chosen: Option<i64>,
) -> Option<i64> {
    let index = req.subtitle_index.or(chosen)?;
    let track = streams.iter().find(|s| s.index == index && s.kind == StreamKind::Subtitle)?;
    let drawn = !track.text_based.unwrap_or(true) && req.caps.sideloads_pictures(&track.codec);
    let streamed = decision.method != Method::Direct || track.external;
    (drawn && streamed && decision.burn_index != Some(index)).then_some(index)
}

/// Whether a player that draws picture subtitles itself may want one of
/// these streams' (see [`picture_to_prepare`]).
pub fn may_draw_pictures(streams: &[MediaStream], caps: &Caps) -> bool {
    streams
        .iter()
        .any(|s| s.kind == StreamKind::Subtitle && !s.text_based.unwrap_or(true) && caps.sideloads_pictures(&s.codec))
}

pub fn subtitle_tracks(file_id: i64, streams: &[MediaStream]) -> Vec<SubtitleTrack> {
    streams
        .iter()
        .filter(|s| s.kind == StreamKind::Subtitle)
        .map(|s| {
            let text = s.text_based.unwrap_or(true);
            SubtitleTrack {
                index: s.index,
                codec: s.codec.clone(),
                language: s.language.clone(),
                title: s.title.clone(),
                default: s.default,
                forced: s.forced,
                external: s.external,
                text_based: text,
                url: text.then(|| format!("/api/files/{file_id}/subtitles/{}.vtt", s.index)),
                added: s.added,
                deletable: false,
            }
        })
        .collect()
}

// ------------------------------------------------------------------ sessions

#[derive(Debug, Clone, Serialize)]
pub struct PlayState {
    pub position_ms: i64,
    pub state: String,
    pub updated_at: i64,
}

/// What a transcode session asked the transcoder for, kept to re-create the
/// job with the exact same segment layout if the transcoder loses it.
#[derive(Debug, Clone)]
pub struct TranscodeSpec {
    /// The request with the *effective* video mode (a remux request the
    /// transcoder answered with a transcode stays a transcode).
    pub request: JobRequest,
    pub segment_count: u32,
    pub duration_ms: i64,
}

pub struct Session {
    pub id: String,
    pub user_id: i64,
    pub username: String,
    pub client_id: Option<String>,
    pub item_id: i64,
    pub file_id: i64,
    pub method: Method,
    pub duration_ms: i64,
    pub transcode: Option<TranscodeSpec>,
    pub job_id: Mutex<Option<String>>,
    pub started_at: i64,
    pub play: Mutex<PlayState>,
    /// The app grant whose token started it (`None`: a session token). The
    /// Plex API's token-less session files refuse these.
    pub app_grant: Option<i64>,
    /// What the session's keyed addresses carry (`/api/media/{key}/…`) when
    /// its player asked for them (`keyed_urls`): 256 random bits that open
    /// this session's media and nothing else, for as long as it lasts.
    pub media_key: Option<String>,
    /// Serialises job re-creation when the transcoder lost the job.
    pub recreate: tokio::sync::Mutex<()>,
    /// The video bitrate its player asked for since it started (kbps),
    /// which a recreated job goes on at too.
    pub bitrate: Mutex<Option<u32>>,
    last_transcoder_touch: AtomicI64,
    /// Not heard from for [`ACTIVE_WINDOW_SECS`] (no report, heartbeat or
    /// request for media): out of "Now playing", its transcode stopped. The
    /// next sign of life wakes it, and a request for media makes a new
    /// transcode (`api::playback::replacement_job`).
    quiet: AtomicBool,
}

impl Session {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        user_id: i64,
        username: String,
        client_id: Option<String>,
        item_id: i64,
        file_id: i64,
        method: Method,
        duration_ms: i64,
        transcode: Option<TranscodeSpec>,
        job_id: Option<String>,
        start_ms: i64,
    ) -> Self {
        let now = now();
        Self {
            id: uuid::Uuid::new_v4().simple().to_string(),
            user_id,
            username,
            client_id,
            item_id,
            file_id,
            method,
            duration_ms,
            transcode,
            job_id: Mutex::new(job_id),
            started_at: now,
            play: Mutex::new(PlayState { position_ms: start_ms, state: "playing".into(), updated_at: now }),
            app_grant: None,
            media_key: None,
            recreate: tokio::sync::Mutex::new(()),
            bitrate: Mutex::new(None),
            last_transcoder_touch: AtomicI64::new(now),
            quiet: AtomicBool::new(false),
        }
    }

    pub fn job_id(&self) -> Option<String> {
        self.job_id.lock().clone()
    }

    pub fn position_ms(&self) -> i64 {
        self.play.lock().position_ms
    }

    /// A request for its media; whether that woke it (it shows again in
    /// "Now playing").
    pub fn touch(&self) -> bool {
        self.play.lock().updated_at = now();
        self.quiet.swap(false, Ordering::SeqCst)
    }

    /// Records a progress report; whether the play state (playing, paused,
    /// stopped) changed or the session woke, which the dashboard and Plex
    /// tools hear about at once.
    pub fn update(&self, position_ms: i64, state: &str) -> bool {
        let mut play = self.play.lock();
        let changed = play.state != state;
        play.position_ms = position_ms;
        if changed {
            play.state = state.to_owned();
        }
        play.updated_at = now();
        drop(play);
        let woke = self.quiet.swap(false, Ordering::SeqCst);
        changed || woke
    }

    /// Playing or paused, with a player that was heard from lately: what
    /// "Now playing" lists.
    pub fn is_active(&self, now: i64) -> bool {
        let play = self.play.lock();
        play.state != "stopped" && now - play.updated_at <= ACTIVE_WINDOW_SECS
    }

    /// Keep the transcoder job alive while the player reports progress (paused players).
    pub fn should_touch_transcoder(&self) -> bool {
        let now = now();
        let last = self.last_transcoder_touch.load(Ordering::Relaxed);
        if now - last >= 30 {
            self.last_transcoder_touch.store(now, Ordering::Relaxed);
            true
        } else {
            false
        }
    }
}

#[derive(Default)]
pub struct Sessions {
    map: Mutex<HashMap<String, Arc<Session>>>,
    /// Transcode slots of play requests still creating their job (see
    /// [`Sessions::reserve_transcode`]). Only raised while `map` is locked.
    reserved: AtomicUsize,
    /// Sessions started so far (see [`Sessions::started_since`]), and play
    /// requests begun.
    starts: AtomicU64,
    /// Play requests on their way to a session (see [`Sessions::begin_start`]).
    beginning: AtomicUsize,
    /// A session started or ended.
    changed: Notify,
    /// Sessions stopped from elsewhere, by id (see [`Sessions::stop`]).
    stopped: Mutex<HashMap<String, Stopped>>,
    /// Conversions asked for (converted downloads), by id: the file each is
    /// made of, and who asked. Whoever may see that file may fetch it.
    conversions: Mutex<HashMap<String, Conversion>>,
}

/// A conversion asked for: the same file and plan make the same one, so
/// several people may wait for it.
#[derive(Debug, Default)]
struct Conversion {
    file_id: i64,
    asked_by: std::collections::HashSet<i64>,
}

/// A session stopped from elsewhere (an administrator, or its owner on
/// another device), remembered for [`STOPPED_KEPT_SECS`] so that its player's
/// next requests learn it (`410 Gone`) and its reports don't start it again.
#[derive(Debug, Clone)]
pub struct Stopped {
    pub user_id: i64,
    pub client_id: Option<String>,
    pub item_id: i64,
    /// What the one who stopped it said, if anything.
    pub message: Option<String>,
    /// Its keyed addresses' key, so they answer `410` too.
    pub media_key: Option<String>,
    at: i64,
}

/// A key for a session's keyed addresses (`/api/media/{key}/…`): 256
/// random bits, prefixed so a leaked one is easy to recognise.
pub fn new_media_key() -> String {
    format!("pley_mk_{}", crate::auth::new_token())
}

/// Whether `a` and `b` are the same key, in time that doesn't depend on
/// where they differ.
pub(crate) fn same_key(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// How long a stopped session is remembered: longer than a player keeps
/// going without asking for anything (a direct play's buffer, a pause).
const STOPPED_KEPT_SECS: i64 = 30 * 60;
/// How many stopped sessions are remembered at most (the oldest go first).
const STOPPED_KEPT: usize = 1_000;

/// A transcode slot, held while the job is created. It counts against the
/// limit until the session takes it over ([`Sessions::insert_transcode`]) or
/// it is dropped: the request failed, or the client went away.
pub struct TranscodeSlot<'a> {
    sessions: &'a Sessions,
}

impl Drop for TranscodeSlot<'_> {
    fn drop(&mut self) {
        self.sessions.reserved.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A play request on its way to a session (see [`Sessions::begin_start`]).
pub struct Beginning<'a> {
    sessions: &'a Sessions,
}

impl Drop for Beginning<'_> {
    fn drop(&mut self) {
        self.sessions.beginning.fetch_sub(1, Ordering::SeqCst);
        self.sessions.changed.notify_waiters();
    }
}

/// Quiet sessions are forgotten after this (a player that comes back later
/// starts a new one).
const SESSION_IDLE_SECS: i64 = 15 * 60;
const STOPPED_LINGER_SECS: i64 = 60;
/// Players report every 10 seconds while playing and every 30 while paused:
/// a session not heard from for this long has lost its player.
const ACTIVE_WINDOW_SECS: i64 = 90;
const REAP_EVERY: Duration = Duration::from_secs(15);
/// Background work that reads whole files (seek previews for a library,
/// intro detection) starts only once nothing has played for this long, so it
/// never takes the disk, the network or the GPU from someone watching (or
/// about to start the next episode).
pub const QUIET_BEFORE_BACKGROUND_SECS: i64 = 60;
/// A playback is starting from its play request ([`Sessions::begin_start`])
/// until this long after its session began: work that can wait (seek
/// previews, intro detection) keeps off the disk and the network meanwhile,
/// so the player's first reads get them (see [`Sessions::starting_for`]).
/// The transcoder holds its own background work (scan probes too) for as
/// long as the start really takes.
pub const STARTING_SECS: i64 = 30;
/// Direct plays a user has at most: the oldest go first. They cost only
/// memory (transcodes have their own limit), but a client starting plays in
/// a loop mustn't be able to grow it without end.
const MAX_DIRECT_PER_USER: usize = 20;

impl Sessions {
    /// Conversion `id` is made of file `file_id`, and `user_id` asked for it.
    pub fn note_conversion(&self, id: &str, file_id: i64, user_id: i64) {
        let mut conversions = self.conversions.lock();
        let conversion = conversions.entry(id.to_owned()).or_default();
        conversion.file_id = file_id;
        conversion.asked_by.insert(user_id);
    }

    /// The file conversion `id` is made of; `None` for one not asked for
    /// since the server started (asked again, it is known again).
    pub fn conversion_file(&self, id: &str) -> Option<i64> {
        self.conversions.lock().get(id).map(|c| c.file_id)
    }

    /// `user_id` doesn't want conversion `id` any more: whether nobody else
    /// does either (it may be stopped then).
    pub fn give_up_conversion(&self, id: &str, user_id: i64) -> bool {
        let mut conversions = self.conversions.lock();
        let Some(conversion) = conversions.get_mut(id) else { return true };
        conversion.asked_by.remove(&user_id);
        conversion.asked_by.is_empty()
    }

    /// Conversion `id` was stopped: unknown until asked for again.
    pub fn forget_conversion(&self, id: &str) {
        self.conversions.lock().remove(id);
    }

    pub fn insert(&self, session: Session) -> Arc<Session> {
        let session = Arc::new(session);
        let mut map = self.map.lock();
        let mut direct: Vec<(i64, String)> = map
            .values()
            .filter(|s| s.user_id == session.user_id && s.transcode.is_none())
            .map(|s| (s.started_at, s.id.clone()))
            .collect();
        if direct.len() >= MAX_DIRECT_PER_USER {
            direct.sort_unstable();
            for (_, id) in &direct[..=direct.len() - MAX_DIRECT_PER_USER] {
                map.remove(id);
            }
        }
        map.insert(session.id.clone(), session.clone());
        drop(map);
        self.started();
        session
    }

    /// Take one of `max` transcode slots, or `None` when live transcodes and
    /// the play requests still creating their job use them all. Creating a
    /// job takes seconds: checking and taking happen under one lock, so play
    /// requests arriving together can't all pass the check before any of
    /// them has a session.
    pub fn reserve_transcode(&self, max: usize) -> Option<TranscodeSlot<'_>> {
        let map = self.map.lock();
        if live_transcodes(&map) + self.reserved.load(Ordering::SeqCst) >= max {
            return None;
        }
        self.reserved.fetch_add(1, Ordering::SeqCst);
        Some(TranscodeSlot { sessions: self })
    }

    /// Add a transcode session, which takes over the slot reserved for it.
    pub fn insert_transcode(&self, session: Session, slot: TranscodeSlot<'_>) -> Arc<Session> {
        let session = Arc::new(session);
        let mut map = self.map.lock();
        map.insert(session.id.clone(), session.clone());
        // Under the same lock: the session counts as the slot stops counting.
        drop(slot);
        drop(map);
        self.started();
        session
    }

    fn started(&self) {
        self.starts.fetch_add(1, Ordering::SeqCst);
        self.changed.notify_waiters();
    }

    /// A play request begins: a playback is starting from now on, before it
    /// has a session (creating its transcode takes seconds, and reads the
    /// file). Background work stops and waits ([`Sessions::started_since`]
    /// resolves, [`Sessions::starting_for`] and [`Sessions::quiet_for`] count
    /// it) until the guard is dropped; the session, once there, takes over.
    pub fn begin_start(&self) -> Beginning<'_> {
        self.beginning.fetch_add(1, Ordering::SeqCst);
        self.started();
        Beginning { sessions: self }
    }

    /// How many sessions have started so far, for [`Sessions::started_since`].
    pub fn starts(&self) -> u64 {
        self.starts.load(Ordering::SeqCst)
    }

    /// Resolves once a session starts after [`Sessions::starts`] said `seen`.
    pub async fn started_since(&self, seen: u64) {
        loop {
            // Registered before the look, so no start slips in between.
            let changed = self.changed.notified();
            if self.starts() != seen {
                return;
            }
            changed.await;
        }
    }

    /// Resolves at the next session that starts or ends.
    pub async fn changed(&self) {
        self.changed.notified().await;
    }

    /// How long the newest session that hasn't stopped is still starting
    /// (it began less than [`STARTING_SECS`] ago), or `None` when none is.
    pub fn starting_for(&self) -> Option<Duration> {
        if self.beginning.load(Ordering::SeqCst) > 0 {
            return Some(Duration::from_secs(STARTING_SECS as u64));
        }
        let now = now();
        let newest =
            self.map.lock().values().filter(|s| s.play.lock().state != "stopped").map(|s| s.started_at).max()?;
        let left = STARTING_SECS - (now - newest);
        (left > 0).then(|| Duration::from_secs(left as u64))
    }

    pub fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.map.lock().get(id).cloned()
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Session>> {
        let removed = self.map.lock().remove(id);
        if removed.is_some() {
            self.changed.notify_waiters();
        }
        removed
    }

    /// Ends a session from elsewhere than its player: it is removed (its
    /// transcode is the caller's to stop) and remembered as stopped, with
    /// `message`. `None` when there is no such session.
    pub fn stop(&self, id: &str, message: Option<String>) -> Option<Arc<Session>> {
        // Remembered before it is removed, so no request in between finds it
        // merely gone (404 makes a player start over; 410 makes it stop).
        let session = self.get(id)?;
        let now = now();
        let mut stopped = self.stopped.lock();
        stopped.retain(|_, s| now - s.at <= STOPPED_KEPT_SECS);
        if stopped.len() >= STOPPED_KEPT
            && let Some(oldest) = stopped.iter().min_by_key(|(_, s)| s.at).map(|(id, _)| id.clone())
        {
            stopped.remove(&oldest);
        }
        stopped.insert(
            session.id.clone(),
            Stopped {
                user_id: session.user_id,
                client_id: session.client_id.clone(),
                item_id: session.item_id,
                message,
                media_key: session.media_key.clone(),
                at: now,
            },
        );
        drop(stopped);
        // Two stops at once: only the one that removes it ends it.
        self.remove(&session.id)
    }

    /// The session `id` if it was stopped from elsewhere lately.
    pub fn stopped(&self, id: &str) -> Option<Stopped> {
        let now = now();
        self.stopped.lock().get(id).filter(|s| now - s.at <= STOPPED_KEPT_SECS).cloned()
    }

    /// The session whose keyed addresses carry `key`.
    pub fn by_media_key(&self, key: &str) -> Option<Arc<Session>> {
        self.map.lock().values().find(|s| s.media_key.as_deref().is_some_and(|k| same_key(k, key))).cloned()
    }

    /// The session stopped lately whose keyed addresses carry `key`.
    pub fn stopped_by_media_key(&self, key: &str) -> Option<Stopped> {
        let now = now();
        self.stopped
            .lock()
            .values()
            .find(|s| now - s.at <= STOPPED_KEPT_SECS && s.media_key.as_deref().is_some_and(|k| same_key(k, key)))
            .cloned()
    }

    /// Whether a session of `user_id` playing `item_id` on `client_id` was
    /// stopped from elsewhere lately (its player may not know yet).
    pub fn was_stopped(&self, user_id: i64, client_id: Option<&str>, item_id: i64) -> bool {
        let now = now();
        self.stopped.lock().values().any(|s| {
            s.user_id == user_id
                && s.item_id == item_id
                && s.client_id.as_deref() == client_id
                && now - s.at <= STOPPED_KEPT_SECS
        })
    }

    /// Forget that `user_id`'s session of `item_id` on `client_id` was
    /// stopped: its player has stopped too, so its next play is a new one.
    pub fn forget_stopped(&self, user_id: i64, client_id: Option<&str>, item_id: i64) {
        self.stopped
            .lock()
            .retain(|_, s| !(s.user_id == user_id && s.item_id == item_id && s.client_id.as_deref() == client_id));
    }

    /// Remove the sessions of `user_id` playing on `client_id` (the client
    /// moved on to something else, or switched quality / audio track).
    pub fn take_client_sessions(&self, user_id: i64, client_id: &str) -> Vec<Arc<Session>> {
        let mut map = self.map.lock();
        let ids: Vec<String> = map
            .values()
            .filter(|s| s.user_id == user_id && s.client_id.as_deref() == Some(client_id))
            .map(|s| s.id.clone())
            .collect();
        let taken: Vec<_> = ids.iter().filter_map(|id| map.remove(id)).collect();
        drop(map);
        if !taken.is_empty() {
            self.changed.notify_waiters();
        }
        taken
    }

    pub fn list(&self) -> Vec<Arc<Session>> {
        let mut list: Vec<_> = self.map.lock().values().cloned().collect();
        list.sort_by_key(|s| s.started_at);
        list
    }

    /// The sessions a player is still there for (see [`Session::is_active`]),
    /// oldest first: "Now playing".
    pub fn active(&self) -> Vec<Arc<Session>> {
        let now = now();
        self.list().into_iter().filter(|s| s.is_active(now)).collect()
    }

    /// Seconds since anything played: 0 while a player is there, counted
    /// from the last sign of the sessions still kept otherwise, and "long
    /// ago" without any.
    pub fn quiet_for(&self) -> i64 {
        if self.beginning.load(Ordering::SeqCst) > 0 {
            return 0;
        }
        let now = now();
        let list = self.list();
        if list.iter().any(|s| s.is_active(now)) {
            return 0;
        }
        list.iter().map(|s| now - s.play.lock().updated_at).min().unwrap_or(i64::MAX)
    }

    /// One look at every session at `now`: those to forget (stopped a
    /// while ago, or quiet for long) and those that just went quiet. Both
    /// leave "Now playing"; their transcodes are the caller's to stop.
    pub fn sweep(&self, now: i64) -> Sweep {
        let mut sweep = Sweep::default();
        for session in self.list() {
            let (stopped, idle) = {
                let play = session.play.lock();
                (play.state == "stopped", now - play.updated_at)
            };
            if idle > SESSION_IDLE_SECS || (stopped && idle > STOPPED_LINGER_SECS) {
                self.remove(&session.id);
                sweep.forgotten.push(session);
            } else if idle > ACTIVE_WINDOW_SECS && !session.quiet.swap(true, Ordering::SeqCst) {
                sweep.quieted.push(session);
            }
        }
        sweep
    }

    /// Transcode sessions with recent activity (segment requests or progress
    /// reports). Abandoned sessions (closed tabs) stop counting quickly even
    /// though they are only reaped later.
    pub fn active_transcodes(&self) -> usize {
        live_transcodes(&self.map.lock())
    }

    /// Every [`REAP_EVERY`]: sessions whose player went quiet leave "Now
    /// playing" and their transcodes stop (a closed tab, a phone gone
    /// to sleep, a crashed app); sessions quiet for long are forgotten.
    pub fn spawn_reaper(self: &Arc<Self>, transcoder: Arc<Transcoder>, events: EventBus) {
        let sessions = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(REAP_EVERY);
            loop {
                tick.tick().await;
                let Some(sessions) = sessions.upgrade() else { break };
                let sweep = sessions.sweep(now());
                // A quieted session's job is gone already when it is forgotten.
                let stop =
                    sweep.quieted.iter().chain(sweep.forgotten.iter().filter(|s| !s.quiet.load(Ordering::SeqCst)));
                for job in stop.filter_map(|s| s.job_id()).collect::<Vec<_>>() {
                    let _ = transcoder.delete_job(&job).await;
                }
                if !sweep.quieted.is_empty() || !sweep.forgotten.is_empty() {
                    events.send(ServerEvent::SessionsChanged);
                }
            }
        });
    }
}

/// What [`Sessions::sweep`] found.
#[derive(Default)]
pub struct Sweep {
    pub forgotten: Vec<Arc<Session>>,
    pub quieted: Vec<Arc<Session>>,
}

/// See [`Sessions::active_transcodes`].
fn live_transcodes(map: &HashMap<String, Arc<Session>>) -> usize {
    let now = now();
    map.values().filter(|s| s.transcode.is_some() && s.is_active(now)).count()
}

// ------------------------------------------------------------------ status mapping

#[derive(Debug, Clone, Serialize)]
pub struct TranscodeInfo {
    pub job_id: String,
    pub state: pley_transcoder_api::JobState,
    pub video: &'static str,
    pub audio: &'static str,
    pub video_in: Option<String>,
    pub video_out: Option<String>,
    pub audio_in: Option<String>,
    pub audio_out: Option<String>,
    pub encoder: Option<String>,
    /// The software encoder's preset now (the automatic one changes).
    pub preset: Option<String>,
    pub decoder: Option<String>,
    /// `gpu` (decode → filters → encode on one GPU), `hybrid`, `cpu` or `copy`.
    pub pipeline: Option<String>,
    pub tonemap: Option<String>,
    /// What a remux does with Dolby Vision: `kept` (named in the master
    /// playlist), `converted to 8.1` (profile 7); `None` otherwise.
    pub dolby_vision: Option<&'static str>,
    /// A remux leaves out the stream's HDR10+ metadata.
    pub hdr10plus_stripped: bool,
    pub subtitle_burn: bool,
    pub speed: Option<f64>,
    pub fps: Option<f64>,
    pub transcoded_ms: i64,
    pub segments_ready: u32,
    pub segments_total: u32,
    /// What the transcode aims for, video and audio together (bits per second).
    pub bitrate_out: Option<i64>,
    pub last_error: Option<String>,
    /// Of a transcode in several sizes ([`HlsCaps::variants`]): the one the
    /// player takes now (0 the largest; the rest of this is its)…
    pub rendition: u32,
    /// …and how many smaller ones there are (0: one size).
    pub variants: u32,
}

/// A codec as people know it: `"hevc"` → `"HEVC"`, `"hdmv_pgs_subtitle"` → `"PGS"`.
pub fn codec_label(codec: &str) -> String {
    match codec {
        "h264" => "H.264".into(),
        "hevc" => "HEVC".into(),
        "av1" => "AV1".into(),
        "vp9" => "VP9".into(),
        "vc1" => "VC-1".into(),
        "mpeg2video" => "MPEG-2".into(),
        "mpeg4" => "MPEG-4".into(),
        "truehd" => "TrueHD".into(),
        "eac3" => "E-AC3".into(),
        "opus" => "Opus".into(),
        "hdmv_pgs_subtitle" | "pgssub" => "PGS".into(),
        "dvd_subtitle" | "dvdsub" => "VobSub".into(),
        "dvb_subtitle" => "DVB".into(),
        other => other.to_uppercase(),
    }
}

/// A note from the transcoder as a sentence: `"no tone mapper available"` →
/// `"No tone mapper available."`.
pub fn sentence(note: &str) -> String {
    let note = note.trim();
    let mut chars = note.chars();
    let mut out: String = chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default();
    if !out.is_empty() && !out.ends_with(['.', '!', '?']) {
        out.push('.');
    }
    out
}

pub fn channel_label(channels: i64) -> String {
    match channels {
        1 => "1.0".into(),
        2 => "2.0".into(),
        3 => "2.1".into(),
        6 => "5.1".into(),
        7 => "6.1".into(),
        8 => "7.1".into(),
        n => format!("{n}ch"),
    }
}

pub fn transcode_info(job: &JobStatus) -> TranscodeInfo {
    let video = job.video.as_ref();
    let audio = job.audio.as_ref();
    let video_in = video.map(|v| {
        let mut s = format!("{} {}×{}", codec_label(&v.codec_in), v.width_in.unwrap_or(0), v.height_in.unwrap_or(0));
        if v.dynamic_range_in.is_hdr() {
            s.push(' ');
            s.push_str(&v.range_in_label());
        }
        s
    });
    let video_out = video.map(|v| {
        let mut s = format!("{} {}×{}", codec_label(&v.codec_out), v.width_out.unwrap_or(0), v.height_out.unwrap_or(0));
        // A transcode that keeps HDR says which.
        if let Some(range) = v.dynamic_range_out.filter(|r| r.is_hdr()) {
            s.push(' ');
            s.push_str(range.label());
        }
        s
    });
    let audio_in = audio.map(|a| {
        format!("{} {}", codec_label(&a.codec_in), a.channels_in.map(channel_label).unwrap_or_default())
            .trim()
            .to_owned()
    });
    let audio_out = audio.map(|a| {
        format!("{} {}", codec_label(&a.codec_out), a.channels_out.map(channel_label).unwrap_or_default())
            .trim()
            .to_owned()
    });
    let process = job.process.as_ref();
    let transcoded_ms = process.map(|p| p.out_time_ms).unwrap_or_else(|| {
        (job.segments_ready as f64 * job.segment_seconds * 1000.0).min(job.duration_ms as f64) as i64
    });
    TranscodeInfo {
        job_id: job.id.clone(),
        state: job.state,
        video: match video {
            None => "none",
            Some(v) if v.mode == VideoMode::Copy => "copy",
            Some(_) => "transcode",
        },
        audio: match audio {
            None => "none",
            Some(a) if a.mode == AudioMode::Copy => "copy",
            Some(_) => "transcode",
        },
        video_in,
        video_out,
        audio_in,
        audio_out,
        encoder: video.and_then(|v| v.encoder.clone()),
        preset: process.and_then(|p| p.preset.clone()),
        decoder: video.and_then(|v| v.decoder.clone()),
        pipeline: video.and_then(|v| v.pipeline.clone()),
        // Only for SDR out: libplacebo also converts between HDR formats
        // (HLG to HDR10), which keeps the video HDR.
        tonemap: video
            .filter(|v| !v.dynamic_range_out.is_some_and(|r| r.is_hdr()))
            .and_then(|v| v.tonemap)
            .map(|t| t.as_str().to_owned()),
        dolby_vision: video.and_then(|v| v.dolby_vision).and_then(|m| match m {
            DolbyVisionMode::Copy => None,
            DolbyVisionMode::Signal => Some("kept"),
            DolbyVisionMode::To81 => Some("converted to 8.1"),
        }),
        hdr10plus_stripped: video.is_some_and(|v| v.hdr10plus_stripped),
        subtitle_burn: video.is_some_and(|v| v.subtitle_burn.is_some()),
        speed: process.and_then(|p| p.speed),
        fps: process.and_then(|p| p.fps),
        transcoded_ms,
        segments_ready: job.segments_ready,
        segments_total: job.segment_count,
        bitrate_out: video
            .filter(|v| v.mode != VideoMode::Copy)
            .and_then(|v| v.bitrate_kbps)
            .map(|kbps| (i64::from(kbps) + audio.and_then(|a| a.bitrate_kbps).map_or(0, i64::from)) * 1000),
        last_error: job.last_error.clone(),
        rendition: job.rendition,
        variants: job.variants,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_transcoder_api::DynamicRange;

    #[test]
    fn a_user_keeps_a_bounded_number_of_direct_plays() {
        let sessions = Sessions::default();
        let play = |user: i64| Session::new(user, "u".into(), None, 1, 1, Method::Direct, 0, None, None, 0);
        let other = sessions.insert(play(2));
        let mut last = None;
        for _ in 0..MAX_DIRECT_PER_USER + 5 {
            last = Some(sessions.insert(play(1)));
        }
        let mine = sessions.list().into_iter().filter(|s| s.user_id == 1).count();
        assert_eq!(mine, MAX_DIRECT_PER_USER);
        assert!(sessions.get(&last.unwrap().id).is_some(), "the newest stays");
        assert!(sessions.get(&other.id).is_some(), "other users' plays are theirs");
    }

    #[tokio::test]
    async fn new_sessions_are_starting_for_a_while() {
        let sessions = Sessions::default();
        assert_eq!(sessions.starting_for(), None, "nothing plays");
        let play = |age: i64| {
            let mut session = Session::new(1, "u".into(), None, 1, 1, Method::Direct, 0, None, None, 0);
            session.started_at -= age;
            session
        };
        let seen = sessions.starts();
        let old = sessions.insert(play(STARTING_SECS + 5));
        assert_eq!(sessions.starting_for(), None, "playing for a while");
        assert_ne!(sessions.starts(), seen);
        // Noticed even when it started before the wait began.
        tokio::time::timeout(Duration::from_secs(1), sessions.started_since(seen)).await.expect("a start");

        let seen = sessions.starts();
        let waiting = sessions.started_since(seen);
        tokio::pin!(waiting);
        assert!(futures_now(waiting.as_mut()).is_none(), "nothing new yet");
        let new = sessions.insert(play(10));
        assert!(futures_now(waiting.as_mut()).is_some(), "a session started");
        let left = sessions.starting_for().expect("starting");
        assert!(
            left <= Duration::from_secs((STARTING_SECS - 10) as u64) && left >= Duration::from_secs(15),
            "{left:?}"
        );

        // A stopped one is no longer starting; nor one that ended.
        new.update(0, "stopped");
        assert_eq!(sessions.starting_for(), None);
        new.update(0, "playing");
        assert!(sessions.starting_for().is_some());
        let ended = sessions.changed();
        tokio::pin!(ended);
        assert!(futures_now(ended.as_mut()).is_none());
        sessions.remove(&new.id);
        assert!(futures_now(ended.as_mut()).is_some(), "the end is heard");
        assert_eq!(sessions.starting_for(), None);
        assert!(sessions.get(&old.id).is_some());
    }

    #[tokio::test]
    async fn play_requests_are_starting_before_their_session_exists() {
        let sessions = Sessions::default();
        assert_eq!(sessions.quiet_for(), i64::MAX, "nothing ever played");
        let seen = sessions.starts();
        let stop = sessions.started_since(seen);
        tokio::pin!(stop);
        assert!(futures_now(stop.as_mut()).is_none());

        // Creating a transcode takes seconds: background work stops at once.
        let beginning = sessions.begin_start();
        assert!(futures_now(stop.as_mut()).is_some(), "work under way stops");
        assert_eq!(sessions.starting_for(), Some(Duration::from_secs(STARTING_SECS as u64)));
        assert_eq!(sessions.quiet_for(), 0, "nothing new starts");
        let over = sessions.changed();
        tokio::pin!(over);
        assert!(futures_now(over.as_mut()).is_none());

        // The request failed (or its session took over): it is over.
        drop(beginning);
        assert!(futures_now(over.as_mut()).is_some(), "waiting work looks again");
        assert_eq!(sessions.starting_for(), None);
        assert_eq!(sessions.quiet_for(), i64::MAX);
    }

    /// Polls a future once.
    fn futures_now<F: std::future::Future>(future: std::pin::Pin<&mut F>) -> Option<F::Output> {
        let waker = std::task::Waker::noop();
        match future.poll(&mut std::task::Context::from_waker(waker)) {
            std::task::Poll::Ready(out) => Some(out),
            std::task::Poll::Pending => None,
        }
    }

    fn file(container: &str, bitrate: i64) -> FileRow {
        FileRow {
            id: 7,
            item_id: 1,
            library_id: 1,
            path: "/m/a".into(),
            size: 1,
            mtime: 0,
            container: Some(container.into()),
            duration_ms: Some(60_000),
            bitrate: Some(bitrate),
            width: Some(1920),
            height: Some(1080),
            video_codec: None,
            audio_codec: None,
            media_info: None,
            external_subs: "[]".into(),
            fingerprint: None,
            added_subs: "[]".into(),
        }
    }

    fn info(video: &str, audio: &[&str], range: DynamicRange) -> MediaInfo {
        let mut streams = vec![MediaStream {
            index: 0,
            kind: StreamKind::Video,
            codec: video.into(),
            width: Some(1920),
            height: Some(1080),
            bit_depth: Some(8),
            dynamic_range: Some(range),
            ..Default::default()
        }];
        for (i, a) in audio.iter().enumerate() {
            streams.push(MediaStream {
                index: i as i64 + 1,
                kind: StreamKind::Audio,
                codec: (*a).into(),
                default: i == 0,
                channels: Some(6),
                ..Default::default()
            });
        }
        streams.push(MediaStream {
            index: 9,
            kind: StreamKind::Subtitle,
            codec: "hdmv_pgs_subtitle".into(),
            text_based: Some(false),
            ..Default::default()
        });
        MediaInfo { container: Some("mkv".into()), duration_ms: Some(60_000), streams, ..Default::default() }
    }

    fn browser() -> Caps {
        Caps {
            containers: vec!["mp4".into(), "webm".into()],
            video_codecs: vec!["h264".into(), "vp9".into()],
            audio_codecs: vec!["aac".into(), "mp3".into(), "opus".into()],
            hdr: false,
            dolby_vision: false,
            ..Default::default()
        }
    }

    fn request() -> PlaybackRequest {
        PlaybackRequest {
            item_id: 1,
            file_id: None,
            audio_index: None,
            subtitle_index: None,
            max_bitrate: None,
            max_height: None,
            start_ms: None,
            caps: browser(),
            mode: PlaybackMode::Auto,
            client_id: None,
            keep_previous: false,
            keyed_urls: false,
            hdr_transcodes: false,
            download: false,
        }
    }

    #[test]
    fn mp4_h264_aac_direct_plays() {
        let i = info("h264", &["aac"], DynamicRange::Sdr);
        let d = decide(&file("mp4", 5_000_000), Some(&i), &i.streams, &request());
        assert_eq!(d.method, Method::Direct);
        assert!(d.reasons.is_empty());
    }

    #[test]
    fn frames_with_the_wrong_times_are_converted() {
        // A 1080p Blu-ray rip in MP4: HEVC with B-frames, every frame timed
        // as it is decoded.
        let mut i = info("hevc", &["aac"], DynamicRange::Sdr);
        i.streams[0].height = Some(804);
        i.streams[0].decode_order_times = true;
        let mut f = file("mp4", 2_231_717);
        f.height = Some(804);
        let mut req = request();
        req.caps.video_codecs.push("hevc".into());
        let d = decide(&f, Some(&i), &i.streams, &req);
        assert_eq!(d.method, Method::Transcode, "neither played as it is nor copied");
        assert_eq!(d.video.as_ref().unwrap().mode, VideoMode::Transcode);
        assert_eq!(d.quality, Some("8m"), "at its own size");
        assert!(d.reasons.iter().any(|r| r.contains("Frame timing")), "{:?}", d.reasons);
        req.mode = PlaybackMode::Remux;
        assert_eq!(decide(&f, Some(&i), &i.streams, &req).method, Method::Transcode, "a copy keeps the times");
        req.mode = PlaybackMode::Direct;
        assert_eq!(decide(&f, Some(&i), &i.streams, &req).method, Method::Direct, "the player asked for it");
        // The apps' players trust the times too.
        req.mode = PlaybackMode::Auto;
        req.caps.caps_version = Some(2);
        let d = decide(&f, Some(&i), &i.streams, &req);
        assert_eq!((d.method, d.video.unwrap().mode), (Method::Transcode, VideoMode::Transcode));
    }

    #[test]
    fn mkv_h264_remuxes_and_transcodes_audio() {
        let i = info("h264", &["ac3"], DynamicRange::Sdr);
        let d = decide(&file("mkv", 5_000_000), Some(&i), &i.streams, &request());
        assert_eq!(d.method, Method::Remux);
        assert_eq!(d.video.unwrap().mode, VideoMode::Copy);
        assert_eq!(d.audio.unwrap().mode, AudioMode::Transcode);
        assert!(d.reasons.iter().any(|r| r.contains("MKV")), "{:?}", d.reasons);
    }

    #[test]
    fn hdr_hevc_transcodes() {
        let i = info("hevc", &["eac3"], DynamicRange::DolbyVision);
        let d = decide(&file("mp4", 20_000_000), Some(&i), &i.streams, &request());
        assert_eq!(d.method, Method::Transcode);
        assert!(d.reasons.iter().any(|r| r.contains("Dolby Vision")));
    }

    #[test]
    fn quality_limit_and_burn_in_force_transcode() {
        let i = info("h264", &["aac"], DynamicRange::Sdr);
        let mut req = request();
        req.max_bitrate = Some(2_000);
        let d = decide(&file("mp4", 8_000_000), Some(&i), &i.streams, &req);
        assert_eq!(d.method, Method::Transcode);
        assert_eq!(d.video.as_ref().unwrap().max_bitrate_kbps, Some(1_808));

        let mut req = request();
        req.subtitle_index = Some(9);
        let d = decide(&file("mp4", 5_000_000), Some(&i), &i.streams, &req);
        assert_eq!(d.method, Method::Transcode);
        assert_eq!(d.subtitle.unwrap().stream_index, Some(9));
    }

    #[test]
    fn adaptive_renditions_step_down_by_about_half() {
        let rungs = |w, h, max_height, kbps: Option<u32>| {
            let top = VideoRequest { max_height, max_bitrate_kbps: kbps, ..Default::default() };
            ladder(Some(w), Some(h), &top)
                .iter()
                .map(|v| (v.max_height.unwrap(), v.max_bitrate_kbps.unwrap()))
                .collect::<Vec<_>>()
        };
        // More bits than 4K gets: 4K again at its own, then the smaller sizes.
        assert_eq!(
            rungs(3840, 2160, Some(2160), Some(29_808)),
            [(2160, 20_000), (1080, 8_000), (720, 4_000), (540, 2_000), (360, 700)]
        );
        assert_eq!(
            rungs(3840, 2160, Some(2160), Some(14_808)),
            [(1080, 8_000), (720, 4_000), (540, 2_000), (360, 700)]
        );
        // A scope film is 1080p's for its width: 1080p's 8 Mbps, then 720p's
        // and below.
        assert_eq!(rungs(1920, 800, Some(1080), Some(19_808)), [(1080, 8_000), (720, 4_000), (540, 2_000), (360, 700)]);
        // Below a quality picked, at most its bitrate.
        assert_eq!(rungs(1920, 1080, Some(1080), Some(7_808)), [(720, 4_000), (540, 2_000), (360, 700)]);
        assert_eq!(rungs(1920, 1080, Some(720), Some(3_808)), [(540, 2_000), (360, 700)]);
        assert_eq!(rungs(1920, 1080, Some(480), Some(1_808)), [(480, 1_200), (360, 700)]);
        // Too close to the one above to make a difference: left out.
        assert!(rungs(1920, 1080, Some(1080), Some(1_000)).is_empty());
        assert!(rungs(640, 360, None, None).is_empty(), "nothing below 360p");
    }

    #[test]
    fn a_quality_asked_for_is_aimed_for_up_to_what_the_file_is_worth() {
        let mut i = info("hevc", &["aac"], DynamicRange::Sdr);
        let mut kbps = |(w, h): (i64, i64), mbps: i64, max_bitrate: u32, max_height: Option<u32>| {
            (i.streams[0].width, i.streams[0].height) = (Some(w), Some(h));
            let mut req = request();
            (req.max_bitrate, req.max_height) = (Some(max_bitrate), max_height);
            let d = decide(&file("mkv", mbps * 1_000_000), Some(&i), &i.streams, &req);
            d.video.unwrap().max_bitrate_kbps.unwrap()
        };
        // What was asked, less the audio.
        assert_eq!(kbps((3840, 2160), 57, 30_000, Some(2160)), 29_808);
        assert_eq!(kbps((3840, 2160), 57, 20_000, Some(1080)), 19_808);
        assert_eq!(kbps((3840, 2160), 57, 2_000, Some(480)), 1_808);
        // A taller quality named by the file's lines ("8 Mbps 720p").
        assert_eq!(kbps((1280, 720), 12, 8_000, Some(1080)), 7_808);
        // Half again the file's: H.264 needs more than HEVC, but no more.
        assert_eq!(kbps((3840, 2160), 12, 20_000, Some(1080)), 17_808);
        // Far more asked (the compatibility API's apps): what the file is worth
        // at those lines, as if nothing had been asked.
        assert_eq!(kbps((3840, 2160), 57, 200_000, Some(2160)), 29_808);
        assert_eq!(kbps((3840, 2160), 57, 200_000, None), 29_808);
        assert_eq!(kbps((3840, 2160), 57, 200_000, Some(720)), 3_808);
        // Few bits: that size's lowest quality, as a conversion of the
        // original gets.
        assert_eq!(kbps((1920, 1080), 2, 20_000, Some(1080)), 7_808);
    }

    #[test]
    fn the_original_converted_aims_for_the_closest_quality() {
        let pick = |w, h, mbps: i64| closest_quality(Some(w), Some(h), Some(mbps * 1_000_000)).0;
        assert_eq!(pick(3840, 2160, 57), "30m", "a 4K remux");
        assert_eq!(pick(3840, 2160, 18), "15m");
        assert_eq!(pick(3840, 2160, 6), "15m", "4K stays 4K, even with few bits");
        assert_eq!(pick(3840, 1600, 40), "30m", "scope 4K");
        assert_eq!(pick(1920, 1080, 25), "20m");
        assert_eq!(pick(1920, 1080, 10), "12m", "HEVC 10 Mbps: H.264 needs more");
        assert_eq!(pick(1920, 1080, 4), "8m");
        assert_eq!(pick(1920, 800, 9), "12m", "scope 1080p stays 1080p");
        assert_eq!(pick(1280, 720, 3), "4m");
        assert_eq!(pick(720, 480, 2), "2m");
        assert_eq!(closest_quality(None, None, None).0, "20m", "nothing known: 1080p's best");

        // HEVC can't play here: converted at that quality, which the menu shows.
        let i = info("hevc", &["aac"], DynamicRange::Sdr);
        let d = decide(&file("mkv", 25_000_000), Some(&i), &i.streams, &request());
        let v = d.video.as_ref().unwrap();
        assert_eq!((d.method, d.quality), (Method::Transcode, Some("20m")));
        assert_eq!((v.max_height, v.max_bitrate_kbps), (Some(1080), Some(19_808)));

        // A quality asked for wins, and copied video has none.
        let mut req = request();
        req.max_bitrate = Some(4_000);
        req.max_height = Some(720);
        let d = decide(&file("mkv", 25_000_000), Some(&i), &i.streams, &req);
        assert_eq!((d.quality, d.video.unwrap().max_height), (None, Some(720)));
        let h264 = info("h264", &["ac3"], DynamicRange::Sdr);
        let d = decide(&file("mkv", 5_000_000), Some(&h264), &h264.streams, &request());
        assert_eq!((d.method, d.quality), (Method::Remux, None));
    }

    #[test]
    fn alternate_audio_track_needs_hls() {
        let i = info("h264", &["aac", "aac"], DynamicRange::Sdr);
        let mut req = request();
        req.audio_index = Some(2);
        let d = decide(&file("mp4", 5_000_000), Some(&i), &i.streams, &req);
        assert_eq!(d.method, Method::Remux);
        assert_eq!(d.audio.unwrap().stream_index, Some(2));
    }

    #[test]
    fn a_file_that_streams_anyway_has_the_track_its_stream_copies() {
        // FLAC (the default) and AAC in English: a browser plays the FLAC
        // with the file as it is, but its stream copies only AAC.
        let mut i = info("h264", &["flac", "aac"], DynamicRange::Sdr);
        for s in i.streams.iter_mut().filter(|s| s.kind == StreamKind::Audio) {
            s.language = Some("eng".into());
        }
        let mut req = request();
        req.caps.audio_codecs.push("flac".into());
        let defaults = crate::streams::Defaults::default();
        let pick =
            |out: &dyn crate::streams::AudioOut| crate::streams::resolve(None, &defaults, &i.streams, Some(out)).audio;
        let d = decide_with_audio(&file("mp4", 8_000_000), Some(&i), &i.streams, &mut req, pick);
        assert_eq!((d.method, req.audio_index), (Method::Direct, None), "the FLAC, as it is");
        // Automatic lowers the quality: the video is converted, and the AAC
        // copied, where the FLAC would have been converted too.
        req.max_bitrate = Some(2_000);
        let d = decide_with_audio(&file("mp4", 8_000_000), Some(&i), &i.streams, &mut req, pick);
        let audio = d.audio.unwrap();
        assert_eq!((d.method, req.audio_index), (Method::Transcode, Some(2)));
        assert_eq!((audio.stream_index, audio.mode), (Some(2), AudioMode::Copy));
    }

    fn dolby_vision(profile: i64, compat: i64) -> MediaInfo {
        let mut i = info("hevc", &["aac"], DynamicRange::DolbyVision);
        i.streams[0].dv_profile = Some(profile);
        i.streams[0].dv_bl_compat = Some(compat);
        i.streams[0].bit_depth = Some(10);
        i
    }

    #[test]
    fn dolby_vision_needs_matching_client() {
        let mut req = request();
        req.caps.containers.push("mkv".into());
        req.caps.video_codecs.push("hevc".into());
        req.caps.hdr = true;

        // Profile 8.1 has an HDR10 base layer: an HDR client plays it directly.
        let p81 = dolby_vision(8, 1);
        assert_eq!(decide(&file("mkv", 20_000_000), Some(&p81), &p81.streams, &req).method, Method::Direct);

        // Profile 5 needs Dolby Vision decoding, otherwise it is tone-mapped.
        let p5 = dolby_vision(5, 0);
        let d = decide(&file("mkv", 20_000_000), Some(&p5), &p5.streams, &req);
        assert_eq!(d.method, Method::Transcode);
        assert!(d.reasons.iter().any(|r| r.contains("Dolby Vision P5")), "{:?}", d.reasons);
        req.caps.dolby_vision = true;
        assert_eq!(decide(&file("mkv", 20_000_000), Some(&p5), &p5.streams, &req).method, Method::Direct);

        // Without HDR support, 8.1 is tone-mapped too.
        req.caps.dolby_vision = false;
        req.caps.hdr = false;
        let d = decide(&file("mkv", 20_000_000), Some(&p81), &p81.streams, &req);
        assert_eq!(d.method, Method::Transcode);
        assert!(d.reasons.iter().any(|r| r.contains("Dolby Vision P8.1")), "{:?}", d.reasons);
    }

    /// Chrome on Windows: HEVC through the graphics card, Matroska, no Dolby
    /// Vision, no AC-3 family, an SDR screen it tone-maps HDR for.
    fn chrome() -> Caps {
        let mut caps = browser();
        caps.containers.push("mkv".into());
        caps.video_codecs.extend(["hevc".into(), "av1".into()]);
        caps.hdr = true;
        caps
    }

    #[test]
    fn video_the_player_decodes_is_copied_when_only_the_audio_needs_work() {
        let mut req = request();
        req.caps = chrome();

        // A 4K remux: Dolby Vision 8.1 with TrueHD Atmos, as
        // Plex plays it in a browser: the video copied, the audio converted.
        let mut p81 = dolby_vision(8, 1);
        p81.streams[1].codec = "truehd".into();
        let d = decide(&file("mkv", 60_000_000), Some(&p81), &p81.streams, &req);
        assert_eq!(d.method, Method::Remux);
        assert_eq!(d.video.unwrap().mode, VideoMode::Copy);
        assert_eq!(d.audio.unwrap().mode, AudioMode::Transcode);
        assert_eq!(d.reasons, ["TrueHD audio not supported."]);

        // HDR10 and SDR HEVC alike.
        for range in [DynamicRange::Hdr10, DynamicRange::Sdr] {
            let i = info("hevc", &["eac3"], range);
            let d = decide(&file("mkv", 20_000_000), Some(&i), &i.streams, &req);
            assert_eq!((d.method, d.video.unwrap().mode), (Method::Remux, VideoMode::Copy), "{range:?}");
        }

        // VP9 and AV1 can't travel in MPEG-TS segments: they play directly
        // when they can and are converted otherwise.
        for codec in ["vp9", "av1"] {
            let i = info(codec, &["aac"], DynamicRange::Sdr);
            assert_eq!(decide(&file("mkv", 5_000_000), Some(&i), &i.streams, &req).method, Method::Direct);
            let i = info(codec, &["ac3"], DynamicRange::Sdr);
            let d = decide(&file("mkv", 5_000_000), Some(&i), &i.streams, &req);
            assert_eq!((d.method, d.video.unwrap().mode), (Method::Transcode, VideoMode::Transcode), "{codec}");
        }

        // A screen without HDR, a lower quality or burned-in subtitles still
        // convert the video.
        let hdr10 = info("hevc", &["eac3"], DynamicRange::Hdr10);
        req.caps.hdr = false;
        assert_eq!(decide(&file("mkv", 20_000_000), Some(&hdr10), &hdr10.streams, &req).method, Method::Transcode);
        req.caps.hdr = true;
        req.max_bitrate = Some(8_000);
        assert_eq!(decide(&file("mkv", 20_000_000), Some(&hdr10), &hdr10.streams, &req).method, Method::Transcode);
    }

    #[test]
    fn playback_modes_override_the_decision() {
        let i = info("h264", &["aac"], DynamicRange::Sdr);
        let mut req = request();
        req.mode = PlaybackMode::Transcode;
        let d = decide(&file("mp4", 5_000_000), Some(&i), &i.streams, &req);
        assert_eq!(d.method, Method::Transcode);
        assert_eq!(d.video.unwrap().mode, VideoMode::Transcode);

        req.mode = PlaybackMode::Remux;
        let d = decide(&file("mp4", 5_000_000), Some(&i), &i.streams, &req);
        assert_eq!(d.method, Method::Remux);

        let hevc = info("hevc", &["eac3"], DynamicRange::Sdr);
        req.mode = PlaybackMode::Direct;
        let d = decide(&file("mkv", 5_000_000), Some(&hevc), &hevc.streams, &req);
        assert_eq!(d.method, Method::Direct);
    }

    #[test]
    fn unprobed_files_play_directly_only_in_web_containers() {
        let req = request();
        let d = decide(&file("mp4", 0), None, &[], &req);
        assert_eq!((d.method, d.reasons.len()), (Method::Direct, 0));
        let d = decide(&file("mkv", 0), None, &[], &req);
        assert_eq!(d.method, Method::Transcode);
        assert_eq!(d.reasons, ["File not analyzed yet."]);
        assert!(d.video.is_some() && d.audio.is_some());
        // The player claims the container: trusted.
        let mut req = request();
        req.caps.containers.push("mkv".into());
        assert_eq!(decide(&file("mkv", 0), None, &[], &req).method, Method::Direct);
        req.mode = PlaybackMode::Remux;
        assert_eq!(decide(&file("mp4", 0), None, &[], &req).method, Method::Transcode, "nothing is known to copy");
    }

    #[test]
    fn ten_bit_and_interlaced_h264_are_re_encoded() {
        let mut i = info("h264", &["aac"], DynamicRange::Sdr);
        i.streams[0].bit_depth = Some(10);
        let d = decide(&file("mp4", 5_000_000), Some(&i), &i.streams, &request());
        assert_eq!(d.method, Method::Transcode);
        assert!(d.reasons.iter().any(|r| r.contains("10-bit")), "{:?}", d.reasons);
        assert_eq!(d.video.unwrap().mode, VideoMode::Transcode);

        let mut i = info("h264", &["aac"], DynamicRange::Sdr);
        i.streams[0].interlaced = Some(true);
        let d = decide(&file("mkv", 5_000_000), Some(&i), &i.streams, &request());
        assert_eq!(d.video.unwrap().mode, VideoMode::Transcode, "browsers can't deinterlace");
    }

    #[test]
    fn resolution_and_bitrate_limits() {
        let i = info("h264", &["aac"], DynamicRange::Sdr);
        let mut req = request();
        req.max_height = Some(1080);
        assert_eq!(decide(&file("mp4", 5_000_000), Some(&i), &i.streams, &req).method, Method::Direct);
        req.max_height = Some(720);
        let d = decide(&file("mp4", 5_000_000), Some(&i), &i.streams, &req);
        assert_eq!(d.method, Method::Transcode);
        assert_eq!(d.video.as_ref().unwrap().max_height, Some(720));
        // Very low limits keep a watchable floor; the file's bitrate exactly at the limit plays.
        let mut req = request();
        req.max_bitrate = Some(300);
        let d = decide(&file("mp4", 5_000_000), Some(&i), &i.streams, &req);
        assert_eq!(d.video.unwrap().max_bitrate_kbps, Some(500));
        req.max_bitrate = Some(5_000);
        assert_eq!(decide(&file("mp4", 5_000_000), Some(&i), &i.streams, &req).method, Method::Direct);
    }

    #[test]
    fn hdr10_plays_directly_on_hdr_displays() {
        let i = info("hevc", &["aac"], DynamicRange::Hdr10);
        let mut req = request();
        req.caps.video_codecs.push("hevc".into());
        let d = decide(&file("mp4", 20_000_000), Some(&i), &i.streams, &req);
        assert_eq!(d.method, Method::Transcode);
        assert!(d.reasons.iter().any(|r| r.contains("converted to SDR")), "{:?}", d.reasons);
        req.caps.hdr = true;
        assert_eq!(decide(&file("mp4", 20_000_000), Some(&i), &i.streams, &req).method, Method::Direct);
    }

    fn track(codec: &str) -> MediaInfo {
        MediaInfo {
            duration_ms: Some(200_000),
            streams: vec![MediaStream {
                index: 0,
                kind: StreamKind::Audio,
                codec: codec.into(),
                default: true,
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn audio_only_files_remux_or_transcode_the_audio() {
        let mut req = request();
        req.caps.containers.push("flac".into());
        let flac = track("flac");
        let d = decide(&file("flac", 900_000), Some(&flac), &flac.streams, &req);
        assert_eq!(d.method, Method::Transcode, "flac is not in the player's codecs");
        assert!(d.video.is_none());
        assert_eq!(d.audio.unwrap().mode, AudioMode::Transcode);

        let mp3 = track("mp3");
        let d = decide(&file("mka", 320_000), Some(&mp3), &mp3.streams, &request());
        assert_eq!(d.method, Method::Remux, "the audio is copied into HLS");
        assert_eq!(d.audio.unwrap().mode, AudioMode::Copy);
    }

    #[test]
    fn external_image_subtitles_are_burned_from_their_file() {
        let mut i = info("h264", &["aac"], DynamicRange::Sdr);
        i.streams.push(MediaStream {
            index: 5,
            kind: StreamKind::Subtitle,
            codec: "subrip".into(),
            text_based: Some(true),
            ..Default::default()
        });
        let mut f = file("mp4", 5_000_000);
        f.media_info = Some(serde_json::to_string(&i).unwrap());
        f.external_subs =
            r#"[{"index":1000,"path":"/m/a.en.sup","language":"eng","codec":"hdmv_pgs_subtitle"}]"#.into();
        let streams = f.streams();
        let ext = streams.iter().find(|s| s.index == 1000).expect("the sidecar is a stream");
        assert!(ext.external && ext.text_based == Some(false));
        let mut req = request();
        req.subtitle_index = Some(1000);
        let d = decide(&f, Some(&i), &streams, &req);
        assert_eq!(d.method, Method::Transcode);
        assert_eq!(d.burn_index, Some(1000));
        let burn = d.subtitle.unwrap();
        assert_eq!((burn.stream_index, burn.external_path.as_deref()), (None, Some("/m/a.en.sup")));
        // Text subtitles are shown by the player: nothing to burn.
        let mut req = request();
        req.subtitle_index = Some(5);
        assert_eq!(decide(&f, Some(&i), &streams, &req).method, Method::Direct);
    }

    #[test]
    fn side_loaded_picture_subtitles_are_never_drawn() {
        // DTS the player doesn't take: repackaged, with the PGS track (9)
        // shown by the player from its own file.
        let i = info("h264", &["dts"], DynamicRange::Sdr);
        let f = file("mkv", 5_000_000);
        let mut req = request();
        req.caps = serde_json::from_value(serde_json::json!({
            "containers": ["mkv", "mp4"], "video_codecs": ["h264"], "audio_codecs": ["aac"],
            "caps_version": 2, "subtitle_codecs": ["hdmv_pgs_subtitle"], "picture_subtitles_sideload": true,
        }))
        .unwrap();
        assert_eq!(req.caps.picture_subtitles_sideload, Some(true));
        let d = decide(&f, Some(&i), &i.streams, &req);
        assert_eq!(d.method, Method::Remux);
        assert_eq!((d.burn_index, d.subtitle.is_none()), (None, true));
        assert!(!d.reasons.iter().any(|r| r.contains("ubtitles")), "{:?}", d.reasons);
        // Asked for by index, they are drawn as for any player.
        req.subtitle_index = Some(9);
        assert_eq!(decide(&f, Some(&i), &i.streams, &req).burn_index, Some(9));
    }

    #[test]
    fn picture_subtitles_the_player_shows_are_drawn_on_any_stream_it_asks_for() {
        // Kodi: Matroska, H.264 and AC-3 play as they are, PGS shown by the
        // player from the file.
        let i = info("h264", &["ac3"], DynamicRange::Sdr);
        let f = file("mkv", 5_000_000);
        let mut req = request();
        req.caps = serde_json::from_value(serde_json::json!({
            "containers": ["mkv"], "video_codecs": ["h264"], "audio_codecs": ["ac3"],
            "caps_version": 2, "subtitle_codecs": ["hdmv_pgs_subtitle"],
        }))
        .unwrap();
        req.subtitle_index = Some(9);
        let d = decide(&f, Some(&i), &i.streams, &req);
        assert_eq!((d.method, d.burn_index), (Method::Direct, None));
        // A stream asked for (Remux, Transcode) carries no subtitles: they
        // are drawn, as when the file doesn't play as it is. They were left
        // out (the player shows none over HLS).
        for mode in [PlaybackMode::Remux, PlaybackMode::Transcode] {
            req.mode = mode;
            let d = decide(&f, Some(&i), &i.streams, &req);
            assert_eq!((d.method, d.burn_index), (Method::Transcode, Some(9)), "{mode:?}");
            assert_eq!(d.reasons[0], "PGS subtitles burned in.");
        }
        // Direct asked for: as it is, the player shows them.
        req.mode = PlaybackMode::Direct;
        assert_eq!(decide(&f, Some(&i), &i.streams, &req).burn_index, None);
    }

    /// A version 2 web app's report (the TVs' too): Matroska, H.264 and
    /// HEVC Main 10, HDR10 and HLG shown, master playlists, and every
    /// picture subtitle drawn by the player from its own file.
    fn sideloading_v2() -> Caps {
        serde_json::from_value(serde_json::json!({
            "containers": ["mkv", "mp4"], "video_codecs": ["h264", "hevc"], "audio_codecs": ["aac", "eac3"],
            "hdr": true, "dolby_vision": false, "caps_version": 2,
            "video": [
                {"codec": "h264", "hardware": true, "profiles": ["Main", "High"], "max_width": 3840, "max_height": 2160,
                 "max_bit_depth": 8},
                {"codec": "hevc", "hardware": true, "profiles": ["Main", "Main 10"], "max_width": 3840, "max_height": 2160,
                 "max_bit_depth": 10}
            ],
            "hdr_types": ["hdr10", "hlg"],
            "audio_decode": ["aac", "eac3"],
            "subtitle_codecs": ["subrip", "ass", "webvtt", "hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle"],
            "picture_subtitles_sideload": true,
            "hls": {"master_playlist": true, "segments": ["ts"]}
        }))
        .unwrap()
    }

    #[test]
    fn picture_subtitles_a_player_draws_from_their_own_file_keep_the_picture() {
        let mut req = request();
        req.caps = sideloading_v2();
        req.hdr_transcodes = true;
        let mut hdr10 = info("hevc", &["eac3"], DynamicRange::Hdr10);
        hdr10.streams[0].bit_depth = Some(10);
        hdr10.streams[0].profile = Some("Main 10".into());
        // HDR10 HEVC: plays as it is, PGS drawn by the player.
        let d = decide(&file("mkv", 20_000_000), Some(&hdr10), &hdr10.streams, &req);
        assert_eq!((d.method, d.burn_index), (Method::Direct, None));
        // DTS the player doesn't take: repackaged, the video copied, HDR kept.
        let mut dts = hdr10.clone();
        dts.streams[1].codec = "dts".into();
        let d = decide(&file("mkv", 20_000_000), Some(&dts), &dts.streams, &req);
        assert_eq!((d.method, d.burn_index, d.subtitle.is_none()), (Method::Remux, None, true));
        assert_eq!(d.video.as_ref().unwrap().mode, VideoMode::Copy);
        // Dolby Vision 8.1 on a Dolby Vision screen, in a remux named by the
        // master playlist: kept.
        let mut p81 = dolby_vision(8, 1);
        p81.streams[1].codec = "dts".into();
        let mut dv = req.clone();
        dv.caps.hdr_types.get_or_insert_with(Vec::new).push("dolby_vision".into());
        dv.caps.dolby_vision_profiles = Some(vec![DolbyVisionDecoder { profile: 8, max_level: None }]);
        let d = decide(&file("mkv", 20_000_000), Some(&p81), &p81.streams, &dv);
        assert_eq!((d.method, d.burn_index), (Method::Remux, None));
        assert_eq!(d.video.as_ref().unwrap().dolby_vision, DolbyVisionMode::Signal, "named, so shown");
        // Converted (a lower quality): HDR10 kept, nothing drawn on it.
        req.max_bitrate = Some(8_000);
        let d = decide(&file("mkv", 20_000_000), Some(&hdr10), &hdr10.streams, &req);
        assert_eq!((d.method, d.burn_index), (Method::Transcode, None));
        assert_eq!(d.video.as_ref().unwrap().dynamic_range, OutputRange::Hdr10, "no SDR for the subtitles");
        assert!(!d.reasons.iter().any(|r| r.contains("ubtitles")), "{:?}", d.reasons);
    }

    #[test]
    fn picture_tracks_are_prepared_for_players_that_draw_them_beside_a_stream() {
        let mut i = info("h264", &["dts"], DynamicRange::Sdr);
        i.streams.push(MediaStream {
            index: 10,
            kind: StreamKind::Subtitle,
            codec: "subrip".into(),
            text_based: Some(true),
            ..Default::default()
        });
        let mut f = file("mkv", 5_000_000);
        f.media_info = Some(serde_json::to_string(&i).unwrap());
        f.external_subs =
            r#"[{"index":1000,"path":"/m/a.en.sup","language":"eng","codec":"hdmv_pgs_subtitle"}]"#.into();
        let streams = f.streams();
        let mut req = request();
        req.caps = sideloading_v2();
        assert!(may_draw_pictures(&streams, &req.caps));
        let prepared = |req: &PlaybackRequest, chosen: Option<i64>| {
            let d = decide(&f, Some(&i), &streams, req);
            (d.method, picture_to_prepare(&streams, req, &d, chosen))
        };
        // DTS: repackaged, with the PGS track the user's choices pick.
        assert_eq!(prepared(&req, Some(9)), (Method::Remux, Some(9)));
        // Text, or nothing chosen: nothing.
        assert_eq!(prepared(&req, Some(10)).1, None);
        assert_eq!(prepared(&req, None).1, None);
        // As it is, a track in the file: the player reads it there or asks
        // at once. One next to it is prepared.
        let mut direct = req.clone();
        direct.caps.audio_decode = Some(vec!["dts".into()]);
        assert_eq!(prepared(&direct, Some(9)), (Method::Direct, None));
        assert_eq!(prepared(&direct, Some(1000)), (Method::Direct, Some(1000)));
        // Named by the player: burned in, nothing to prepare.
        let mut burn = req.clone();
        burn.subtitle_index = Some(9);
        assert_eq!(prepared(&burn, None).1, None);
        // A player that doesn't draw them, or not these.
        let mut android = req.clone();
        android.caps.picture_subtitles_sideload = None;
        assert!(!may_draw_pictures(&streams, &android.caps));
        assert_eq!(prepared(&android, Some(9)).1, None);
        let mut pgs_only = req.clone();
        pgs_only.caps.subtitle_codecs = Some(vec!["dvd_subtitle".into()]);
        assert_eq!(prepared(&pgs_only, Some(9)).1, None);
        // The web app's version 1 report with the two fields: the same.
        let mut web = request();
        web.caps = serde_json::from_value(serde_json::json!({
            "containers": ["mp4", "webm", "mkv"], "video_codecs": ["h264"], "audio_codecs": ["aac"],
            "subtitle_codecs": ["subrip", "hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle"],
            "picture_subtitles_sideload": true,
        }))
        .unwrap();
        assert!(!web.caps.is_v2());
        let d = decide(&f, Some(&i), &streams, &web);
        assert_eq!((d.method, d.burn_index), (Method::Remux, None), "DTS: repackaged, PGS not drawn on");
        assert_eq!(picture_to_prepare(&streams, &web, &d, Some(9)), Some(9));
    }

    #[test]
    fn the_web_apps_picture_subtitles_change_no_decision() {
        // The web app says in its version 1 report that it draws picture
        // subtitles from their `.mks` (apps/web/src/lib/caps.ts): whatever
        // it plays, and with a picture track burned in or not, the server
        // decides as without it.
        let web = |draws: bool| {
            let mut caps = browser();
            caps.containers.push("mkv".into());
            if draws {
                let codecs =
                    ["subrip", "ass", "ssa", "webvtt", "mov_text", "hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle"];
                caps.subtitle_codecs = Some(codecs.iter().map(|c| (*c).to_owned()).collect());
                caps.picture_subtitles_sideload = Some(true);
            }
            caps
        };
        let files = [
            ("mp4", "h264", "aac", DynamicRange::Sdr),
            ("mkv", "h264", "ac3", DynamicRange::Sdr),
            ("mkv", "hevc", "aac", DynamicRange::Hdr10),
            ("mkv", "h264", "dts", DynamicRange::Sdr),
        ];
        for (container, video, audio, range) in files {
            let i = info(video, &[audio], range);
            let f = file(container, 5_000_000);
            for subtitle_index in [None, Some(9)] {
                for mode in [PlaybackMode::Auto, PlaybackMode::Remux, PlaybackMode::Transcode] {
                    let mut req = request();
                    (req.subtitle_index, req.mode, req.caps) = (subtitle_index, mode, web(false));
                    let before = format!("{:?}", decide(&f, Some(&i), &i.streams, &req));
                    req.caps = web(true);
                    let after = format!("{:?}", decide(&f, Some(&i), &i.streams, &req));
                    assert_eq!(before, after, "{container} {video} {audio} {subtitle_index:?} {mode:?}");
                }
            }
        }
    }

    #[test]
    fn transcode_status_labels() {
        use pley_transcoder_api::{AudioPlan, JobState, ProcessStatus, VideoPlan};
        let job = JobStatus {
            id: "j".into(),
            state: JobState::Running,
            duration_ms: 60_000,
            segment_seconds: 4.0,
            segment_count: 15,
            segments_ready: 3,
            video: Some(VideoPlan {
                mode: VideoMode::Transcode,
                codec_in: "hevc".into(),
                codec_out: "h264".into(),
                width_in: Some(3840),
                height_in: Some(2160),
                width_out: Some(1920),
                height_out: Some(1080),
                dynamic_range_in: DynamicRange::DolbyVision,
                dv_profile: Some(8),
                dv_bl_compat: Some(1),
                ..Default::default()
            }),
            audio: Some(AudioPlan {
                mode: AudioMode::Transcode,
                codec_in: "truehd".into(),
                codec_out: "aac".into(),
                channels_in: Some(8),
                channels_out: Some(2),
                ..Default::default()
            }),
            ..Default::default()
        };
        let info = transcode_info(&job);
        assert_eq!(info.video_in.as_deref(), Some("HEVC 3840×2160 Dolby Vision P8.1"));
        assert_eq!(info.video_out.as_deref(), Some("H.264 1920×1080"));
        assert_eq!((info.audio_in.as_deref(), info.audio_out.as_deref()), (Some("TrueHD 7.1"), Some("AAC 2.0")));
        assert_eq!((info.video, info.audio), ("transcode", "transcode"));
        assert_eq!(info.transcoded_ms, 12_000, "ready segments while no process reports a time");
        let running =
            JobStatus { process: Some(ProcessStatus { out_time_ms: 30_500, ..Default::default() }), ..job.clone() };
        assert_eq!(transcode_info(&running).transcoded_ms, 30_500);
        // Profile 7's layer and HDR10+ are named; the tone mapper only when
        // the video comes out SDR (libplacebo also turns HLG into HDR10).
        let mut plan = job.video.clone().unwrap();
        (plan.dv_profile, plan.dv_bl_compat, plan.dv_el, plan.hdr10plus) =
            (Some(7), Some(6), Some(pley_transcoder_api::DolbyVisionLayer::Fel), true);
        plan.tonemap = Some(pley_transcoder_api::Tonemapper::Libplacebo);
        let with = |plan: VideoPlan| transcode_info(&JobStatus { video: Some(plan), ..job.clone() });
        let p7 = with(plan.clone());
        assert_eq!(p7.video_in.as_deref(), Some("HEVC 3840×2160 Dolby Vision P7 FEL + HDR10+"));
        assert_eq!(p7.tonemap.as_deref(), Some("libplacebo"));
        plan.dynamic_range_out = Some(DynamicRange::Hdr10);
        assert_eq!(with(plan).tonemap, None);
        assert_eq!(transcode_info(&JobStatus { video: None, audio: None, ..job }).video, "none");
        assert_eq!(channel_label(6), "5.1");
        assert_eq!(channel_label(4), "4ch");
    }

    fn session(user_id: i64, client: Option<&str>, transcode: bool) -> Session {
        let spec =
            transcode.then(|| TranscodeSpec { request: JobRequest::default(), segment_count: 1, duration_ms: 1 });
        Session::new(user_id, "u".into(), client.map(str::to_owned), 1, 1, Method::Transcode, 1, spec, None, 0)
    }

    #[test]
    fn a_client_plays_one_thing_at_a_time() {
        let sessions = Sessions::default();
        let tab = sessions.insert(session(1, Some("tab"), true)).id.clone();
        sessions.insert(session(1, Some("tv"), true));
        sessions.insert(session(2, Some("tab"), true));
        sessions.insert(session(1, None, false));
        let taken = sessions.take_client_sessions(1, "tab");
        assert_eq!(taken.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), [tab.as_str()]);
        assert!(sessions.get(&tab).is_none());
        assert_eq!(sessions.list().len(), 3, "other clients and users keep playing");
        assert!(sessions.take_client_sessions(1, "tab").is_empty());
    }

    #[test]
    fn only_live_transcodes_count_against_the_limit() {
        let sessions = Sessions::default();
        sessions.insert(session(1, None, false));
        let live = sessions.insert(session(1, None, true));
        let stopped = sessions.insert(session(1, None, true));
        stopped.update(0, "stopped");
        let abandoned = sessions.insert(session(1, None, true));
        abandoned.play.lock().updated_at = now() - ACTIVE_WINDOW_SECS - 1;
        assert_eq!(sessions.active_transcodes(), 1);
        live.update(1_000, "paused");
        assert_eq!(sessions.active_transcodes(), 1, "paused players keep their slot");
    }

    #[test]
    fn sessions_whose_player_went_quiet_leave_now_playing_and_wake_again() {
        let sessions = Sessions::default();
        let playing = sessions.insert(session(1, None, true));
        let paused = sessions.insert(session(1, None, false));
        paused.update(5_000, "paused");
        let stopped = sessions.insert(session(1, None, false));
        stopped.update(9_000, "stopped");
        let gone = sessions.insert(session(1, None, true));
        gone.play.lock().updated_at = now() - ACTIVE_WINDOW_SECS - 1;
        // Sorted: sessions started in the same second come in any order.
        let ids = |list: Vec<Arc<Session>>| {
            let mut ids = list.iter().map(|s| s.id.clone()).collect::<Vec<_>>();
            ids.sort();
            ids
        };
        assert_eq!(
            ids(sessions.active()),
            ids(vec![playing.clone(), paused.clone()]),
            "stopped and quiet ones aren't playing"
        );

        let sweep = sessions.sweep(now());
        assert_eq!(ids(sweep.quieted), std::slice::from_ref(&gone.id));
        assert!(sweep.forgotten.is_empty(), "stopped a moment ago: kept a little");
        assert!(sessions.sweep(now()).quieted.is_empty(), "quieted once");
        assert!(sessions.get(&gone.id).is_some(), "kept, to wake");

        // A sign of life wakes it.
        assert!(gone.touch(), "a request for media");
        assert!(!gone.touch());
        assert!(ids(sessions.active()).contains(&gone.id));
        gone.play.lock().updated_at = now() - ACTIVE_WINDOW_SECS - 1;
        assert_eq!(sessions.sweep(now()).quieted.len(), 1);
        assert!(gone.update(10_000, "playing"), "a progress report wakes it too");

        // Later: stopped ones and long-quiet ones are forgotten.
        let later = now() + SESSION_IDLE_SECS + 1;
        let forgotten = ids(sessions.sweep(later).forgotten);
        assert_eq!(forgotten.len(), 4);
        assert!(sessions.list().is_empty());
    }

    #[test]
    fn background_work_waits_for_quiet() {
        let sessions = Sessions::default();
        assert_eq!(sessions.quiet_for(), i64::MAX, "nothing played since the start");
        let playing = sessions.insert(session(1, None, false));
        assert_eq!(sessions.quiet_for(), 0);
        playing.update(5_000, "paused");
        assert_eq!(sessions.quiet_for(), 0, "a paused player is still there");
        playing.update(5_000, "stopped");
        playing.play.lock().updated_at = now() - 30;
        assert_eq!(sessions.quiet_for(), 30, "stopped half a minute ago");
        assert!(sessions.quiet_for() < QUIET_BEFORE_BACKGROUND_SECS);
    }

    #[test]
    fn transcode_slots_are_taken_before_the_job_exists() {
        let sessions = Sessions::default();
        let first = sessions.reserve_transcode(2).expect("a free slot");
        let second = sessions.reserve_transcode(2).expect("another one");
        assert!(sessions.reserve_transcode(2).is_none(), "both are taken while their jobs are created");
        drop(second);
        let third = sessions.reserve_transcode(2).expect("a failed request gives its slot back");
        sessions.insert_transcode(session(1, None, true), first);
        assert_eq!(sessions.active_transcodes(), 1);
        assert!(sessions.reserve_transcode(2).is_none(), "the session keeps the slot it took over");
        drop(third);
        assert!(sessions.reserve_transcode(2).is_some());
        assert!(sessions.reserve_transcode(1).is_none(), "a lower limit counts the live session");
    }

    #[test]
    fn bit_depths_and_frame_rates_for_version_2_decisions() {
        let stream = |bit_depth, pix_fmt: Option<&str>, profile: Option<&str>| MediaStream {
            bit_depth,
            pix_fmt: pix_fmt.map(str::to_owned),
            profile: profile.map(str::to_owned),
            ..Default::default()
        };
        assert_eq!(bit_depth(&stream(Some(12), Some("yuv420p10le"), None)), 12, "the stream says");
        assert_eq!(bit_depth(&stream(None, Some("yuv420p10le"), None)), 10);
        assert_eq!(bit_depth(&stream(None, Some("p010le"), None)), 10);
        assert_eq!(bit_depth(&stream(None, Some("yuv444p12be"), None)), 12);
        assert_eq!(bit_depth(&stream(None, Some("yuv420p"), Some("Main 10"))), 10);
        assert_eq!(bit_depth(&stream(None, Some("yuv420p"), Some("High"))), 8);
        assert_eq!(bit_depth(&stream(None, None, None)), 8);
        assert_eq!(trim_fps(24_000.0 / 1001.0), "23.976");
        assert_eq!(trim_fps(25.0), "25");
        assert_eq!(trim_fps(59.94), "59.94");
        assert_eq!(closest_quality_within(Some(3840), Some(2160), Some(57_000_000), Some(1080)), ("20m", 20_000, 1080));
        assert_eq!(closest_quality_within(Some(1280), Some(720), None, Some(1080)).2, 720, "never above the file");
        assert_eq!(closest_quality_within(Some(3840), Some(2160), None, Some(600)).2, 480);
    }

    #[test]
    fn subtitle_tracks_have_urls_for_text() {
        let streams = vec![
            MediaStream {
                index: 3,
                kind: StreamKind::Subtitle,
                codec: "subrip".into(),
                text_based: Some(true),
                ..Default::default()
            },
            MediaStream {
                index: 4,
                kind: StreamKind::Subtitle,
                codec: "hdmv_pgs_subtitle".into(),
                text_based: Some(false),
                ..Default::default()
            },
        ];
        let tracks = subtitle_tracks(7, &streams);
        assert_eq!(tracks[0].url.as_deref(), Some("/api/files/7/subtitles/3.vtt"));
        assert_eq!(tracks[1].url, None);
    }
}

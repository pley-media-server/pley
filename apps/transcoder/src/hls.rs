// SPDX-License-Identifier: AGPL-3.0-only
//! A job's multivariant playlist, `master.m3u8`: its variants (the job's
//! `index.m3u8`, then its lower renditions' `v1.m3u8`… for adaptive
//! bitrate) and what a player needs to pick its decoders before it
//! reads a segment, since MPEG-TS segments can't say that a stream is Dolby
//! Vision: `CODECS` (RFC 6381), `SUPPLEMENTAL-CODECS` for Dolby Vision
//! profile 8 (RFC 8216bis), `RESOLUTION`, `FRAME-RATE`, `VIDEO-RANGE`,
//! `CLOSED-CAPTIONS=NONE` for video without captions in its frames, and
//! the bandwidth. Media3 then gives a stream named `dvh1…` (or `hvc1…` with
//! a `dvh1…` supplemental codec and the matching range) to a Dolby Vision
//! decoder, and with every codec named it prepares without downloading a
//! segment first. The names come from the plan and the media, not from an
//! fMP4 job's init: the master playlist is asked for before any process
//! has made one.

use pley_transcoder_api::{
    DolbyVisionMode, DynamicRange, MediaInfo, MediaStream, SegmentFormat, VideoCodec, default_video_kbps,
};

use crate::plan::{AudioOut, Plan, VideoOut};

/// The variant of a job's master playlist.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub codecs: Vec<String>,
    pub supplemental_codecs: Option<String>,
    pub resolution: Option<(i64, i64)>,
    pub frame_rate: Option<f64>,
    /// `SDR`, `PQ` or `HLG`.
    pub video_range: Option<&'static str>,
    /// The probe found no CEA-608/708 captions in the video:
    /// `CLOSED-CAPTIONS=NONE`, as Apple's HLS authoring rules ask, so AVKit
    /// offers no captions menu of its own beside the player's subtitles.
    /// Captions in the frames (a TV recording's) survive copying and
    /// encoding, so a video with them, or probed before probes said, says
    /// nothing, and players look for them.
    pub no_closed_captions: bool,
    /// Bits per second.
    pub average_bandwidth: u64,
    pub bandwidth: u64,
}

/// MPEG-TS packet and PES headers on top of the streams' own bitrates.
const TS_OVERHEAD: f64 = 1.05;
/// fMP4's fragment headers and sample tables, lighter than MPEG-TS's.
const FMP4_OVERHEAD: f64 = 1.02;

/// What a job's segments add to its streams' bitrates.
fn overhead(format: SegmentFormat) -> f64 {
    match format {
        SegmentFormat::Ts => TS_OVERHEAD,
        SegmentFormat::Fmp4 => FMP4_OVERHEAD,
    }
}

impl Variant {
    pub fn of(plan: &Plan, media: &MediaInfo) -> Variant {
        let video = plan.video.as_ref().map(|v| (v, media.stream(v.stream_index)));
        let audio = plan.audio.as_ref().map(|a| (a, media.stream(a.stream_index)));
        let mut codecs = Vec::new();
        let mut supplemental_codecs = None;
        let mut video_range = None;
        if let Some((v, src)) = video {
            let (codec, supplemental, range) = video_codecs(v, src);
            codecs.push(codec);
            supplemental_codecs = supplemental;
            video_range = Some(range);
        }
        if let Some((a, src)) = audio {
            codecs.push(audio_codec(a, src));
        }
        let video_bps = video.map_or(0, |(v, src)| video_bitrate(v, src, media));
        let audio_bps = audio.map_or(0, |(a, src)| audio_bitrate(a, src));
        let overhead = overhead(plan.segment_format);
        let average = (video_bps + audio_bps) as f64 * overhead;
        // Copied video keeps its peaks (twice the average is common on
        // discs); encodes stay near their rate.
        let peak = match video {
            Some((v, _)) if v.copy => (2 * video_bps + audio_bps) as f64 * overhead,
            _ => average * 1.25,
        };
        Variant {
            codecs,
            supplemental_codecs,
            resolution: video.map(|(v, _)| (v.width, v.height)),
            frame_rate: video.and_then(|(_, src)| src?.frame_rate).filter(|f| *f > 0.0),
            video_range,
            no_closed_captions: video.is_some_and(|(_, src)| src.and_then(|s| s.closed_captions) == Some(false)),
            average_bandwidth: average.round() as u64,
            bandwidth: peak.round() as u64,
        }
    }

    /// The `#EXT-X-STREAM-INF` attributes.
    pub fn attributes(&self) -> String {
        let mut a = vec![
            format!("BANDWIDTH={}", self.bandwidth.max(1)),
            format!("AVERAGE-BANDWIDTH={}", self.average_bandwidth.max(1)),
            format!("CODECS=\"{}\"", self.codecs.join(",")),
        ];
        if let Some(s) = &self.supplemental_codecs {
            a.push(format!("SUPPLEMENTAL-CODECS=\"{s}\""));
        }
        if let Some((w, h)) = self.resolution {
            a.push(format!("RESOLUTION={w}x{h}"));
        }
        if let Some(fps) = self.frame_rate {
            a.push(format!("FRAME-RATE={fps:.3}"));
        }
        if self.no_closed_captions {
            a.push("CLOSED-CAPTIONS=NONE".into());
        }
        if let Some(range) = self.video_range {
            a.push(format!("VIDEO-RANGE={range}"));
        }
        a.join(",")
    }
}

/// The master playlist of a job whose media playlist is `index.m3u8` next
/// to it.
#[cfg(test)]
pub fn master_playlist(plan: &Plan, media: &MediaInfo) -> String {
    renditions_playlist(&[(plan, "index.m3u8".into())], media)
}

/// The master playlist of a job with lower renditions too
/// ([`pley_transcoder_api::JobRequest::variants`]), each with its media
/// playlist, highest first: players that pick by the list's order start
/// with the best. fMP4 segments (`EXT-X-MAP`) need version 7; a job's
/// renditions are all of one format.
pub fn renditions_playlist(renditions: &[(&Plan, String)], media: &MediaInfo) -> String {
    let fmp4 = renditions.iter().any(|(plan, _)| plan.segment_format == SegmentFormat::Fmp4);
    let mut out = format!("#EXTM3U\n#EXT-X-VERSION:{}\n", if fmp4 { 7 } else { 3 });
    if renditions.iter().any(|(plan, _)| plan.video.is_some()) {
        out.push_str("#EXT-X-INDEPENDENT-SEGMENTS\n");
    }
    for (plan, uri) in renditions {
        out.push_str(&format!("#EXT-X-STREAM-INF:{}\n{uri}\n", Variant::of(plan, media).attributes()));
    }
    out
}

/// The video's codec, its supplemental codec, and its range.
fn video_codecs(v: &VideoOut, src: Option<&MediaStream>) -> (String, Option<String>, &'static str) {
    let Some(src) = src.filter(|_| v.copy) else {
        // Encoded here: HEVC Main 10 keeping HDR, else tone-mapped (or SDR
        // already), 8-bit.
        if let Some(hdr) = v.hdr {
            let range = if hdr.range == DynamicRange::Hlg { "HLG" } else { "PQ" };
            return (hevc_codec(Some("Main 10"), 10, None, v.height), None, range);
        }
        let level = Some(h264_level(v.width, v.height, src.and_then(|s| s.frame_rate)));
        let codec = match v.codec {
            VideoCodec::H264 => avc_codec(Some("High"), level),
            VideoCodec::Hevc => hevc_codec(Some("Main"), 8, None, v.height),
        };
        return (codec, None, "SDR");
    };
    let base = match src.codec.as_str() {
        "h264" => avc_codec(src.profile.as_deref(), src.level),
        "hevc" => hevc_codec(src.profile.as_deref(), src.bit_depth.unwrap_or(8), src.level, v.height),
        // fMP4 only.
        "vp9" => vp9_codec(src, v.width, v.height),
        "av1" => av1_codec(src, v.width, v.height),
        other => other.to_owned(),
    };
    let range = base_layer_range(src);
    let signalled = matches!(v.dolby_vision, Some(DolbyVisionMode::Signal | DolbyVisionMode::To81));
    if !signalled || src.dynamic_range() != DynamicRange::DolbyVision {
        return (base, None, range);
    }
    // The level the stream needs, where the file says more (the server
    // repackages such a stream for a decoder that refuses what it says).
    let level = src.dv_level_named().unwrap_or(0).clamp(0, 99);
    let to_81 = v.dolby_vision == Some(DolbyVisionMode::To81);
    match (if to_81 { 8 } else { src.dv_profile.unwrap_or(0) }, src.dv_bl_compat) {
        // No base layer anything else decodes: Dolby Vision alone.
        (5, _) => (format!("dvh1.05.{level:02}"), None, "PQ"),
        // The base layer decodes on its own: the HEVC codec, with Dolby
        // Vision as its supplement, the brand saying what the base layer is.
        (8, compat) => {
            let compat = if to_81 { Some(1) } else { compat };
            // 6, the Blu-ray's HDR10, in a profile 7 made 8.1 elsewhere: HDR10.
            let (brand, range) = match compat {
                Some(1 | 6) | None => ("db1p", "PQ"),
                Some(4) => ("db4h", "HLG"),
                Some(2) => ("db2g", "SDR"),
                _ => return (base, None, range),
            };
            (base, Some(format!("dvh1.08.{level:02}/{brand}")), range)
        }
        // Profile 7, both layers, for players that decode it (the Shield).
        (7, _) => (format!("dvh1.07.{level:02}"), None, "PQ"),
        _ => (base, None, range),
    }
}

/// The range the stream shows as without Dolby Vision.
fn base_layer_range(src: &MediaStream) -> &'static str {
    match src.dynamic_range() {
        DynamicRange::Sdr => "SDR",
        DynamicRange::Hdr10 | DynamicRange::Hdr10plus => "PQ",
        DynamicRange::Hlg => "HLG",
        DynamicRange::DolbyVision => match src.dv_bl_compat {
            Some(2) => "SDR",
            Some(4) => "HLG",
            _ => "PQ",
        },
    }
}

/// `avc1.PPCCLL`: profile, constraint flags and level, in hex.
fn avc_codec(profile: Option<&str>, level: Option<i64>) -> String {
    let (idc, constraints) = match profile.map(str::to_ascii_lowercase).as_deref() {
        Some("constrained baseline") => (66, 0x40),
        Some("baseline") => (66, 0),
        Some("main") => (77, 0),
        Some("extended") => (88, 0),
        Some("high 10" | "high 10 intra") => (110, 0),
        Some("high 4:2:2" | "high 4:2:2 intra") => (122, 0),
        Some("high 4:4:4 predictive" | "high 4:4:4 intra") => (244, 0),
        _ => (100, 0),
    };
    let level = level.filter(|l| (9..=62).contains(l)).unwrap_or(40);
    format!("avc1.{idc:02X}{constraints:02X}{level:02X}")
}

/// `hvc1.P.C.LLL.B0`: profile, its compatibility flags (bit-reversed, as
/// RFC 6381 wants), Main tier and level (ffprobe's `level` is the general
/// level, 30 × the level number), progressive source.
fn hevc_codec(profile: Option<&str>, bit_depth: i64, level: Option<i64>, height: i64) -> String {
    let (idc, compat) = match profile {
        Some("Main") => (1, 6),
        Some("Main 10") => (2, 4),
        Some(p) if p.starts_with("Rext") || p.contains("4:2:2") || p.contains("4:4:4") => (4, 10),
        _ if bit_depth > 8 => (2, 4),
        _ => (1, 6),
    };
    let level = level.filter(|l| *l > 0).unwrap_or(match height {
        h if h > 1080 => 153,
        h if h > 720 => 123,
        _ => 93,
    });
    format!("hvc1.{idc}.{compat}.L{level}.B0")
}

/// `vp09.PP.LL.DD`: profile, level (10 for 1 … 62 for 6.2) and bit depth.
/// ffprobe seldom knows a VP9 stream's level: the lowest that takes its
/// size and frame rate, then.
fn vp9_codec(src: &MediaStream, width: i64, height: i64) -> String {
    const LEVELS: [(i64, f64, f64); 14] = [
        (10, 36_864.0, 829_440.0),
        (11, 73_728.0, 2_764_800.0),
        (20, 122_880.0, 4_608_000.0),
        (21, 245_760.0, 9_216_000.0),
        (30, 552_960.0, 20_736_000.0),
        (31, 983_040.0, 36_864_000.0),
        (40, 2_228_224.0, 83_558_400.0),
        (41, 2_228_224.0, 160_432_128.0),
        (50, 8_912_896.0, 311_951_360.0),
        (51, 8_912_896.0, 588_251_136.0),
        (52, 8_912_896.0, 1_176_502_272.0),
        (60, 35_651_584.0, 1_176_502_272.0),
        (61, 35_651_584.0, 2_353_004_544.0),
        (62, 35_651_584.0, 4_706_009_088.0),
    ];
    let profile = numbered_profile(src.profile.as_deref()).filter(|p| (0..=3).contains(p)).unwrap_or(0);
    let level = src
        .level
        .filter(|l| LEVELS.iter().any(|(known, _, _)| known == l))
        .unwrap_or_else(|| lowest_level(&LEVELS, width, height, src.frame_rate));
    format!("vp09.{profile:02}.{level:02}.{:02}", bit_depth(src))
}

/// `av01.P.LLT.DD`: profile (Main 0, High 1, Professional 2), level
/// (`seq_level_idx`, which ffprobe gives: 8 for 4.0), Main tier, bit depth.
fn av1_codec(src: &MediaStream, width: i64, height: i64) -> String {
    // seq_level_idx (4 per major level from 2.0), the picture size and the
    // samples per second it takes.
    const LEVELS: [(i64, f64, f64); 14] = [
        (0, 147_456.0, 4_423_680.0),
        (1, 278_784.0, 8_363_520.0),
        (4, 665_856.0, 19_975_680.0),
        (5, 1_065_024.0, 31_950_720.0),
        (8, 2_359_296.0, 70_778_880.0),
        (9, 2_359_296.0, 141_557_760.0),
        (12, 8_912_896.0, 267_386_880.0),
        (13, 8_912_896.0, 534_773_760.0),
        (14, 8_912_896.0, 1_069_547_520.0),
        (15, 8_912_896.0, 1_069_547_520.0),
        (16, 35_651_584.0, 1_069_547_520.0),
        (17, 35_651_584.0, 2_139_095_040.0),
        (18, 35_651_584.0, 4_278_190_080.0),
        (19, 35_651_584.0, 4_278_190_080.0),
    ];
    let profile = match src.profile.as_deref().map(str::to_ascii_lowercase).as_deref() {
        Some("high") => 1,
        Some("professional") => 2,
        _ => 0,
    };
    let level = src
        .level
        .filter(|l| (0..=23).contains(l))
        .unwrap_or_else(|| lowest_level(&LEVELS, width, height, src.frame_rate));
    format!("av01.{profile}.{level:02}M.{:02}", bit_depth(src))
}

/// The number in ffprobe's "Profile 2".
fn numbered_profile(profile: Option<&str>) -> Option<i64> {
    profile?.strip_prefix("Profile ")?.trim().parse().ok()
}

fn bit_depth(src: &MediaStream) -> i64 {
    src.bit_depth.filter(|b| (8..=16).contains(b)).unwrap_or(8)
}

/// The first of `levels` (each with its largest picture and samples per
/// second) that takes this size and frame rate, else the last.
fn lowest_level(levels: &[(i64, f64, f64)], width: i64, height: i64, fps: Option<f64>) -> i64 {
    let picture = (width.max(1) * height.max(1)) as f64;
    let rate = picture * fps.filter(|f| *f > 0.0).unwrap_or(30.0);
    levels.iter().find(|(_, size, speed)| picture <= *size && rate <= *speed).unwrap_or(&levels[levels.len() - 1]).0
}

/// The lowest H.264 level for this size and frame rate (Table A-1), as
/// encoders pick it: 30 for 3.0 … 52 for 5.2.
fn h264_level(width: i64, height: i64, fps: Option<f64>) -> i64 {
    let mbs = ((width.max(16) + 15) / 16) * ((height.max(16) + 15) / 16);
    let rate = mbs as f64 * fps.unwrap_or(30.0);
    const LEVELS: [(i64, i64, f64); 9] = [
        (30, 1_620, 40_500.0),
        (31, 3_600, 108_000.0),
        (32, 5_120, 216_000.0),
        (40, 8_192, 245_760.0),
        (42, 8_704, 522_240.0),
        (50, 22_080, 589_824.0),
        (51, 36_864, 983_040.0),
        (52, 36_864, 2_073_600.0),
        (62, 139_264, 16_711_680.0),
    ];
    LEVELS.iter().find(|(_, frame, speed)| mbs <= *frame && rate <= *speed).map_or(62, |l| l.0)
}

fn audio_codec(a: &AudioOut, src: Option<&MediaStream>) -> String {
    let codec = if a.copy { src.map_or("aac", |s| s.codec.as_str()) } else { a.codec.as_str() };
    let profile = src.filter(|_| a.copy).and_then(|s| s.profile.as_deref()).unwrap_or_default();
    match codec {
        "aac" if profile == "HE-AAC" => "mp4a.40.5".into(),
        "aac" if profile == "HE-AACv2" => "mp4a.40.29".into(),
        "aac" => "mp4a.40.2".into(),
        "mp3" => "mp4a.40.34".into(),
        "ac3" => "ac-3".into(),
        "eac3" => "ec-3".into(),
        // Copied Opus and FLAC are named by their sample entries, as RFC
        // 6381 asks: AVPlayer refuses a master playlist that says `opus` or
        // `flac` ("Cannot Open"); hls.js and Media3 read either spelling.
        "opus" => "Opus".into(),
        "flac" => "fLaC".into(),
        other => other.to_owned(),
    }
}

fn video_bitrate(v: &VideoOut, src: Option<&MediaStream>, media: &MediaInfo) -> u64 {
    if !v.copy {
        return u64::from(v.bitrate_kbps) * 1000;
    }
    let audio: i64 = media
        .streams
        .iter()
        .filter(|s| s.kind != pley_transcoder_api::StreamKind::Video)
        .filter_map(|s| s.bitrate)
        .sum();
    src.and_then(|s| s.bitrate)
        .or_else(|| media.bitrate.map(|b| b - audio))
        .filter(|b| *b > 0)
        .map_or(u64::from(default_video_kbps(v.width, v.height)) * 1000, |b| b as u64)
}

fn audio_bitrate(a: &AudioOut, src: Option<&MediaStream>) -> u64 {
    if !a.copy {
        return u64::from(a.bitrate_kbps) * 1000;
    }
    src.and_then(|s| s.bitrate).filter(|b| *b > 0).map_or(640_000, |b| b as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Pipeline, uniform_layout};
    use pley_transcoder_api::{AudioCodec, StreamKind};

    fn video_out(copy: bool, dolby_vision: Option<DolbyVisionMode>) -> VideoOut {
        VideoOut {
            stream_index: 0,
            copy,
            encoder: None,
            codec: VideoCodec::H264,
            src_width: 3840,
            src_height: 2160,
            width: if copy { 3840 } else { 1920 },
            height: if copy { 2160 } else { 1080 },
            bitrate_kbps: if copy { 0 } else { 8_000 },
            tonemap: None,
            tonemap_quality: Default::default(),
            tonemap_input: None,
            libplacebo_asked: false,
            qsv_base_layer: false,
            base_layer_only: false,
            fel_compose: false,
            opencl_bt2390: false,
            amf_upload: false,
            vui_primaries: None,
            hw_decode: None,
            gpu_scale: None,
            deinterlace: false,
            preset: String::new(),
            pipeline: Pipeline::Software,
            dolby_vision,
            strip_hdr10plus: false,
            hevc: copy,
            dvh1: false,
            hdr: None,
        }
    }

    fn audio_out(copy: bool, codec: AudioCodec) -> AudioOut {
        AudioOut {
            stream_index: 1,
            copy,
            codec,
            channels: 6,
            bitrate_kbps: 640,
            resample: false,
            partial_decode: None,
            downmix: None,
        }
    }

    fn plan(video: Option<VideoOut>, audio: Option<AudioOut>) -> Plan {
        let (boundaries, split_at) = uniform_layout(60_000, 2_000);
        Plan {
            duration_ms: 60_000,
            seek_at: boundaries.clone(),
            boundaries,
            split_at,
            segment_seconds: 2.0,
            video,
            audio,
            burn: None,
            throttle: false,
            catchup: false,
            warnings: vec![],
            segment_format: Default::default(),
        }
    }

    /// 4K HEVC Main 10 at 23.976 fps, level 5.1, 40 Mbit/s, of `range`;
    /// E-AC-3 5.1 at 768 kbit/s.
    fn media(range: DynamicRange, dv: Option<(i64, i64, i64)>) -> MediaInfo {
        MediaInfo {
            bitrate: Some(41_000_000),
            streams: vec![
                MediaStream {
                    index: 0,
                    kind: StreamKind::Video,
                    codec: "hevc".into(),
                    profile: Some("Main 10".into()),
                    width: Some(3840),
                    height: Some(2160),
                    frame_rate: Some(23.976),
                    bit_depth: Some(10),
                    level: Some(153),
                    dynamic_range: Some(range),
                    dv_profile: dv.map(|d| d.0),
                    dv_level: dv.map(|d| d.1),
                    dv_bl_compat: dv.map(|d| d.2),
                    closed_captions: Some(false),
                    ..Default::default()
                },
                MediaStream {
                    index: 1,
                    kind: StreamKind::Audio,
                    codec: "eac3".into(),
                    channels: Some(6),
                    bitrate: Some(768_000),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    fn stream_inf(video: VideoOut, m: &MediaInfo) -> String {
        let text = master_playlist(&plan(Some(video), Some(audio_out(true, AudioCodec::Aac))), m);
        let line = text.lines().find(|l| l.starts_with("#EXT-X-STREAM-INF:")).unwrap();
        line.trim_start_matches("#EXT-X-STREAM-INF:").to_owned()
    }

    #[test]
    fn master_playlists_name_one_variant() {
        let m = media(DynamicRange::Hdr10, None);
        let text = master_playlist(&plan(Some(video_out(true, None)), Some(audio_out(true, AudioCodec::Aac))), &m);
        assert_eq!(
            text,
            "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-INDEPENDENT-SEGMENTS\n\
             #EXT-X-STREAM-INF:BANDWIDTH=85293600,AVERAGE-BANDWIDTH=43050000,CODECS=\"hvc1.2.4.L153.B0,ec-3\",\
             RESOLUTION=3840x2160,FRAME-RATE=23.976,CLOSED-CAPTIONS=NONE,VIDEO-RANGE=PQ\nindex.m3u8\n"
        );
    }

    #[test]
    fn video_with_captions_in_its_frames_leaves_them_to_be_found() {
        let mut m = media(DynamicRange::Sdr, None);
        // A probe from before probes said.
        m.streams[0].closed_captions = None;
        assert!(!stream_inf(video_out(true, None), &m).contains("CLOSED-CAPTIONS"));
        m.streams[0].closed_captions = Some(false);
        assert!(stream_inf(video_out(true, None), &m).contains(",CLOSED-CAPTIONS=NONE,"));
        m.streams[0].closed_captions = Some(true);
        assert!(!stream_inf(video_out(true, None), &m).contains("CLOSED-CAPTIONS"));
        assert!(!stream_inf(video_out(false, None), &m).contains("CLOSED-CAPTIONS"));
        // Audio alone has no video to carry them.
        let text = master_playlist(&plan(None, Some(audio_out(false, AudioCodec::Aac))), &m);
        assert!(!text.contains("CLOSED-CAPTIONS"), "{text}");
    }

    #[test]
    fn dolby_vision_is_named_as_asked() {
        let p81 = media(DynamicRange::DolbyVision, Some((8, 6, 1)));
        // Not asked: the HDR10 base layer.
        assert!(
            stream_inf(video_out(true, Some(DolbyVisionMode::Copy)), &p81)
                .contains("CODECS=\"hvc1.2.4.L153.B0,ec-3\",RESOLUTION")
        );
        // Profile 8.1: HEVC with Dolby Vision as its supplement.
        let inf = stream_inf(video_out(true, Some(DolbyVisionMode::Signal)), &p81);
        assert!(inf.contains("CODECS=\"hvc1.2.4.L153.B0,ec-3\",SUPPLEMENTAL-CODECS=\"dvh1.08.06/db1p\""), "{inf}");
        assert!(inf.ends_with("VIDEO-RANGE=PQ"), "{inf}");
        // 8.4 is HLG underneath.
        let p84 = media(DynamicRange::DolbyVision, Some((8, 6, 4)));
        let inf = stream_inf(video_out(true, Some(DolbyVisionMode::Signal)), &p84);
        assert!(inf.contains("SUPPLEMENTAL-CODECS=\"dvh1.08.06/db4h\"") && inf.ends_with("VIDEO-RANGE=HLG"), "{inf}");
        // Profile 5: Dolby Vision alone, at the level its 4K at 23.976 fps
        // needs, though the file says 9.
        let p5 = media(DynamicRange::DolbyVision, Some((5, 9, 0)));
        let inf = stream_inf(video_out(true, Some(DolbyVisionMode::Signal)), &p5);
        assert!(inf.contains("CODECS=\"dvh1.05.06,ec-3\",RESOLUTION") && inf.ends_with("VIDEO-RANGE=PQ"), "{inf}");
        // A re-encode's 8.1 that says level 10: 6, as it needs.
        let p81_10 = media(DynamicRange::DolbyVision, Some((8, 10, 1)));
        let inf = stream_inf(video_out(true, Some(DolbyVisionMode::Signal)), &p81_10);
        assert!(inf.contains("SUPPLEMENTAL-CODECS=\"dvh1.08.06/db1p\""), "{inf}");
        // Profile 8 with the Blu-ray's HDR10 id (a profile 7 made 8.1 elsewhere): HDR10 underneath.
        let p86 = media(DynamicRange::DolbyVision, Some((8, 6, 6)));
        let inf = stream_inf(video_out(true, Some(DolbyVisionMode::Signal)), &p86);
        assert!(inf.contains("SUPPLEMENTAL-CODECS=\"dvh1.08.06/db1p\"") && inf.ends_with("VIDEO-RANGE=PQ"), "{inf}");
        // Profile 7 made 8.1, or named as it is for a player that decodes it.
        let p7 = media(DynamicRange::DolbyVision, Some((7, 6, 6)));
        let inf = stream_inf(video_out(true, Some(DolbyVisionMode::To81)), &p7);
        assert!(inf.contains("SUPPLEMENTAL-CODECS=\"dvh1.08.06/db1p\""), "{inf}");
        let inf = stream_inf(video_out(true, Some(DolbyVisionMode::Signal)), &p7);
        assert!(inf.contains("CODECS=\"dvh1.07.06,ec-3\""), "{inf}");
        let inf = stream_inf(video_out(true, Some(DolbyVisionMode::Copy)), &p7);
        assert!(inf.contains("CODECS=\"hvc1.2.4.L153.B0,ec-3\",RESOLUTION") && inf.ends_with("VIDEO-RANGE=PQ"));
    }

    #[test]
    fn encodes_are_named_by_what_comes_out() {
        let m = media(DynamicRange::DolbyVision, Some((5, 9, 0)));
        let p = plan(Some(video_out(false, None)), Some(audio_out(false, AudioCodec::Eac3)));
        let v = Variant::of(&p, &m);
        // 1080p at 23.976 fps: level 4.0; tone-mapped to SDR.
        assert_eq!(v.codecs, ["avc1.640028", "ec-3"]);
        assert_eq!((v.supplemental_codecs, v.video_range, v.resolution), (None, Some("SDR"), Some((1920, 1080))));
        assert_eq!(v.average_bandwidth, ((8_000_000.0 + 640_000.0) * TS_OVERHEAD).round() as u64);
        // Audio alone.
        let p = plan(None, Some(audio_out(false, AudioCodec::Aac)));
        let text = master_playlist(&p, &m);
        assert!(
            text.ends_with(
                "#EXT-X-STREAM-INF:BANDWIDTH=840000,AVERAGE-BANDWIDTH=672000,CODECS=\"mp4a.40.2\"\nindex.m3u8\n"
            ),
            "{text}"
        );
        assert!(!text.contains("INDEPENDENT"));
    }

    #[test]
    fn hdr_encodes_are_main_10_in_their_range() {
        use crate::plan::HdrOut;
        let m = media(DynamicRange::DolbyVision, Some((5, 9, 0)));
        let mut video = video_out(false, None);
        video.codec = VideoCodec::Hevc;
        video.hdr = Some(HdrOut { range: DynamicRange::Hdr10, convert: true });
        let v = Variant::of(&plan(Some(video.clone()), None), &m);
        assert_eq!((v.codecs, v.video_range), (vec!["hvc1.2.4.L123.B0".to_owned()], Some("PQ")));
        video.hdr = Some(HdrOut { range: DynamicRange::Hlg, convert: false });
        (video.width, video.height) = (3840, 2160);
        let v = Variant::of(&plan(Some(video), None), &media(DynamicRange::Hlg, None));
        assert_eq!((v.codecs, v.video_range), (vec!["hvc1.2.4.L153.B0".to_owned()], Some("HLG")));
    }

    #[test]
    fn fmp4_master_playlists_are_version_7_with_less_overhead() {
        let m = media(DynamicRange::Hdr10, None);
        let mut p = plan(Some(video_out(true, None)), Some(audio_out(true, AudioCodec::Aac)));
        let ts = master_playlist(&p, &m);
        p.segment_format = SegmentFormat::Fmp4;
        let fmp4 = master_playlist(&p, &m);
        assert_eq!(
            fmp4,
            "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-INDEPENDENT-SEGMENTS\n\
             #EXT-X-STREAM-INF:BANDWIDTH=82856640,AVERAGE-BANDWIDTH=41820000,CODECS=\"hvc1.2.4.L153.B0,ec-3\",\
             RESOLUTION=3840x2160,FRAME-RATE=23.976,CLOSED-CAPTIONS=NONE,VIDEO-RANGE=PQ\nindex.m3u8\n"
        );
        assert!(ts.starts_with("#EXTM3U\n#EXT-X-VERSION:3\n"), "{ts}");
        // The same streams.
        let codecs = |text: &str| text.split("CODECS=").nth(1).unwrap().to_owned();
        assert_eq!(codecs(&ts), codecs(&fmp4));
        // HEVC encodes are hvc1 (which the transcoder tags them), H.264 avc1.
        let mut video = video_out(false, None);
        video.codec = VideoCodec::Hevc;
        let mut p = plan(Some(video), None);
        p.segment_format = SegmentFormat::Fmp4;
        let v = Variant::of(&p, &m);
        assert_eq!(v.codecs, ["hvc1.1.6.L123.B0"]);
        assert_eq!(v.average_bandwidth, (8_000_000.0 * FMP4_OVERHEAD).round() as u64);
    }

    #[test]
    fn vp9_and_av1_copies_are_named_as_rfc_6381_says() {
        let stream = |codec: &str, profile: &str, level: Option<i64>, bit_depth: i64| MediaStream {
            kind: StreamKind::Video,
            codec: codec.into(),
            profile: Some(profile.into()),
            level,
            bit_depth: Some(bit_depth),
            frame_rate: Some(23.976),
            ..Default::default()
        };
        // ffprobe seldom knows VP9's level: from the size and frame rate.
        assert_eq!(vp9_codec(&stream("vp9", "Profile 0", None, 8), 1920, 1080), "vp09.00.40.08");
        assert_eq!(vp9_codec(&stream("vp9", "Profile 0", Some(-99), 8), 1280, 720), "vp09.00.31.08");
        assert_eq!(vp9_codec(&stream("vp9", "Profile 2", None, 10), 3840, 2160), "vp09.02.50.10");
        assert_eq!(vp9_codec(&stream("vp9", "Profile 0", Some(41), 8), 1920, 1080), "vp09.00.41.08");
        // AV1's is its seq_level_idx, which ffprobe gives.
        assert_eq!(av1_codec(&stream("av1", "Main", Some(8), 8), 1920, 1080), "av01.0.08M.08");
        assert_eq!(av1_codec(&stream("av1", "Main", Some(13), 10), 3840, 2160), "av01.0.13M.10");
        assert_eq!(av1_codec(&stream("av1", "High", None, 8), 3840, 2160), "av01.1.12M.08");
        assert_eq!(av1_codec(&stream("av1", "Professional", Some(-99), 12), 1280, 720), "av01.2.05M.12");

        // In a master playlist, copied.
        let mut m = media(DynamicRange::Sdr, None);
        m.streams[0] =
            MediaStream { index: 0, width: Some(3840), height: Some(2160), ..stream("av1", "Main", Some(12), 10) };
        let mut video = video_out(true, None);
        video.hevc = false;
        let mut p = plan(Some(video), Some(audio_out(false, AudioCodec::Aac)));
        p.segment_format = SegmentFormat::Fmp4;
        let v = Variant::of(&p, &m);
        assert_eq!((v.codecs, v.video_range), (vec!["av01.0.12M.10".to_owned(), "mp4a.40.2".to_owned()], Some("SDR")));
    }

    #[test]
    fn rfc_6381_names() {
        assert_eq!(avc_codec(Some("High"), Some(41)), "avc1.640029");
        assert_eq!(avc_codec(Some("Constrained Baseline"), Some(30)), "avc1.42401E");
        assert_eq!(avc_codec(Some("Main"), None), "avc1.4D0028");
        assert_eq!(hevc_codec(Some("Main"), 8, Some(120), 1080), "hvc1.1.6.L120.B0");
        assert_eq!(hevc_codec(None, 10, None, 2160), "hvc1.2.4.L153.B0");
        assert_eq!(h264_level(1920, 1080, Some(23.976)), 40);
        assert_eq!(h264_level(1920, 1080, Some(59.94)), 42);
        assert_eq!(h264_level(3840, 2160, Some(23.976)), 51);
        assert_eq!(h264_level(3840, 2160, Some(60.0)), 52);
        assert_eq!(h264_level(1280, 720, Some(30.0)), 31);
        let copied = |codec: &str, profile: Option<&str>| {
            let src = MediaStream { codec: codec.into(), profile: profile.map(Into::into), ..Default::default() };
            audio_codec(&audio_out(true, AudioCodec::Aac), Some(&src))
        };
        assert_eq!(copied("aac", Some("LC")), "mp4a.40.2");
        assert_eq!(copied("aac", Some("HE-AAC")), "mp4a.40.5");
        assert_eq!(copied("mp3", None), "mp4a.40.34");
        assert_eq!(copied("ac3", None), "ac-3");
        assert_eq!(copied("eac3", Some("Dolby Digital Plus + Dolby Atmos")), "ec-3");
        assert_eq!(copied("opus", None), "Opus");
        assert_eq!(copied("flac", None), "fLaC");
        assert_eq!(copied("alac", None), "alac");
    }
}

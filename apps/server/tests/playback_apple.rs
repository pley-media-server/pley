//! Playback decisions for the Apple apps' capability reports: each
//! recorded device class (`fixtures/caps/apple_*_v2.json`, written by
//! apps/apple/PleyKit's `RecordedDevicesTests` from its recorded probes)
//! against the files of the Apple test matrix, as the server decides them
//! with a transcoder that makes fMP4 segments and keeps HDR.

use pley::catalog::FileRow;
use pley::playback::{Decision, Delivery, Method, PlaybackRequest, decide, video_range_shown};
use pley_transcoder_api::{
    AudioCodec, AudioMode, DolbyVisionLayer, DolbyVisionMode, DynamicRange, MediaInfo, MediaStream, SegmentFormat,
    StreamKind, VideoMode,
};
use serde_json::{Value, json};

// ------------------------------------------------------------------ devices

const DEVICES: [&str; 7] =
    ["iphone_a16_hdr", "ipad_5th_gen", "mac_m3_xdr", "mac_intel_sdr", "tv_4k_hdr10", "tv_4k_dv", "tv_hd"];

fn fixture(device: &str) -> Value {
    let path = format!("{}/tests/fixtures/caps/apple_{device}_v2.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The report as the server takes it, with what it fills in itself: its
/// transcoder makes fMP4 segments and keeps HDR in transcodes.
fn request(device: &str) -> PlaybackRequest {
    let mut req: PlaybackRequest =
        serde_json::from_value(json!({"item_id": 1, "caps": fixture(device), "keyed_urls": true})).unwrap();
    req.fmp4_segments = true;
    req.hdr_transcodes = true;
    req
}

// ------------------------------------------------------------------ files

fn file(container: &str) -> FileRow {
    FileRow {
        id: 7,
        item_id: 1,
        library_id: 1,
        path: "/m/a".into(),
        size: 1,
        mtime: 0,
        container: Some(container.into()),
        duration_ms: Some(60_000),
        bitrate: Some(12_000_000),
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

/// A 1080p, 23.976 fps video stream.
fn video(codec: &str, profile: &str, bit_depth: i64, level: i64, range: DynamicRange) -> MediaStream {
    MediaStream {
        index: 0,
        kind: StreamKind::Video,
        codec: codec.into(),
        profile: Some(profile.into()),
        width: Some(1920),
        height: Some(1080),
        frame_rate: Some(24_000.0 / 1001.0),
        bit_depth: Some(bit_depth),
        level: Some(level),
        dynamic_range: Some(range),
        ..Default::default()
    }
}

/// HEVC Main 10 Dolby Vision at level 6.
fn dolby_vision(profile: i64, compat: i64, el: Option<DolbyVisionLayer>) -> MediaStream {
    let mut v = video("hevc", "Main 10", 10, 120, DynamicRange::DolbyVision);
    (v.dv_profile, v.dv_level, v.dv_bl_compat, v.dv_el) = (Some(profile), Some(6), Some(compat), el);
    v
}

fn audio(codec: &str, channels: i64) -> MediaStream {
    MediaStream {
        index: 1,
        kind: StreamKind::Audio,
        codec: codec.into(),
        channels: Some(channels),
        sample_rate: Some(48_000),
        default: true,
        ..Default::default()
    }
}

fn pgs() -> MediaStream {
    MediaStream {
        index: 2,
        kind: StreamKind::Subtitle,
        codec: "hdmv_pgs_subtitle".into(),
        language: Some("eng".into()),
        text_based: Some(false),
        ..Default::default()
    }
}

/// A test file: its container, streams, and the subtitle picked.
struct Source {
    container: &'static str,
    streams: Vec<MediaStream>,
    subtitle: Option<i64>,
}

const FILES: [&str; 12] = [
    "MP4 H.264 AAC",
    "MKV H.264 AC-3",
    "MKV HEVC HDR10 E-AC-3",
    "MKV DV 8.1 TrueHD",
    "MKV DV 5",
    "MKV DV 7 FEL",
    "MKV AV1 10-bit",
    "MKV MPEG-2 MP2",
    "MKV H.264 DTS 5.1",
    "MKV H.264 PGS picked",
    "MKV H.264 Opus",
    "MKV H.264 FLAC",
];

fn source(name: &str) -> Source {
    let h264 = || video("h264", "High", 8, 41, DynamicRange::Sdr);
    let (container, streams, subtitle) = match name {
        "MP4 H.264 AAC" => ("mp4", vec![h264(), audio("aac", 2)], None),
        "MKV H.264 AC-3" => ("mkv", vec![h264(), audio("ac3", 6)], None),
        "MKV HEVC HDR10 E-AC-3" => {
            ("mkv", vec![video("hevc", "Main 10", 10, 120, DynamicRange::Hdr10), audio("eac3", 6)], None)
        }
        "MKV DV 8.1 TrueHD" => ("mkv", vec![dolby_vision(8, 1, None), audio("truehd", 8)], None),
        "MKV DV 5" => ("mkv", vec![dolby_vision(5, 0, None), audio("eac3", 6)], None),
        "MKV DV 7 FEL" => ("mkv", vec![dolby_vision(7, 6, Some(DolbyVisionLayer::Fel)), audio("truehd", 8)], None),
        "MKV AV1 10-bit" => ("mkv", vec![video("av1", "Main", 10, 8, DynamicRange::Sdr), audio("aac", 2)], None),
        "MKV MPEG-2 MP2" => ("mkv", vec![video("mpeg2video", "Main", 8, 8, DynamicRange::Sdr), audio("mp2", 2)], None),
        "MKV H.264 DTS 5.1" => ("mkv", vec![h264(), audio("dts", 6)], None),
        "MKV H.264 PGS picked" => ("mkv", vec![h264(), audio("aac", 2), pgs()], Some(2)),
        "MKV H.264 Opus" => ("mkv", vec![h264(), audio("opus", 2)], None),
        "MKV H.264 FLAC" => ("mkv", vec![h264(), audio("flac", 2)], None),
        other => panic!("{other}"),
    };
    Source { container, streams, subtitle }
}

// ------------------------------------------------------------------ deciding

/// What the server answers, as `POST /api/playback` and the session's
/// details say it.
#[derive(Debug, PartialEq)]
struct Answer {
    /// `D` as it is; `R` the video copied into HLS, `R+DV` with Dolby
    /// Vision kept and named, `R+8.1` made profile 8.1; `T` converted.
    method: &'static str,
    /// The codec the audio is converted to (`audio_converted`); empty when
    /// it plays or is copied as it is.
    audio: &'static str,
    segments: SegmentFormat,
    /// `video_range`: what the screen gets of HDR video.
    video_range: Option<String>,
    reasons: Vec<String>,
}

fn answer(req: &PlaybackRequest, name: &str) -> Answer {
    let src = source(name);
    let mut req = req.clone();
    // As the apps ask: picture subtitles they draw from the `.mks` aren't sent, so nothing is burned in.
    if !req.caps.sideloads_pictures("hdmv_pgs_subtitle") {
        req.subtitle_index = src.subtitle;
    }
    let info = MediaInfo {
        container: Some(src.container.into()),
        duration_ms: Some(60_000),
        streams: src.streams,
        ..Default::default()
    };
    let d: Decision = decide(&file(src.container), Some(&info), &info.streams, &req);
    let video = d.video.as_ref();
    let method = match (d.method, video) {
        (Method::Direct, _) => "D",
        (_, Some(v)) if v.mode == VideoMode::Copy => match v.dolby_vision {
            DolbyVisionMode::Copy => "R",
            DolbyVisionMode::Signal => "R+DV",
            DolbyVisionMode::To81 => "R+8.1",
        },
        _ => "T",
    };
    let delivery = match (d.method, video) {
        (Method::Direct, _) => Delivery::Direct,
        (_, Some(v)) if v.mode == VideoMode::Copy => {
            Delivery::Copy { dolby_vision: v.dolby_vision, hdr10plus_stripped: v.strip_hdr10plus }
        }
        (_, Some(v)) => Delivery::Converted(v.dynamic_range.dynamic_range()),
        (_, None) => unreachable!("{name}: no video"),
    };
    let audio = match d.audio.as_ref().filter(|a| a.mode != AudioMode::Copy).map(|a| a.codec) {
        None => "",
        Some(AudioCodec::Aac) => "aac",
        Some(AudioCodec::Ac3) => "ac3",
        Some(AudioCodec::Eac3) => "eac3",
    };
    Answer {
        method,
        audio,
        segments: d.segment_format,
        video_range: video_range_shown(&info.streams[0], &req.caps, delivery),
        reasons: d.reasons,
    }
}

// ------------------------------------------------------------------ the table

const MKV: &str = "MKV not supported.";
const TRUEHD: &str = "TrueHD audio not supported.";
const DTS: &str = "DTS audio not supported.";
const AV1: &str = "AV1 video not supported.";
const MPEG2: &str = "MPEG-2 video not supported.";
const MP2: &str = "MP2 audio not supported.";
const OPUS: &str = "Opus audio not supported.";
const SDR: Option<&str> = Some("SDR (tone-mapped)");

/// One cell: the method, the audio converted to, the video range and the
/// reasons.
type Cell = (&'static str, &'static str, Option<&'static str>, &'static [&'static str]);

/// The rows every Apple device decides alike.
const MP4_H264: Cell = ("D", "", None, &[]);
const MKV_H264_AC3: Cell = ("R", "", None, &[MKV]);
const MPEG2_MP2: Cell = ("T", "aac", None, &[MKV, MPEG2, MP2]);
/// The app draws the PGS from the server's `.mks`: the video is copied.
const PGS_PICKED: Cell = ("R", "", None, &[MKV]);
const AV1_CONVERTED: Cell = ("T", "", None, &[MKV, AV1]);
/// A screen without HDR (or, on the iPad 5th generation, without a 10-bit
/// HEVC decoder): HDR and Dolby Vision become SDR.
const SDR_ROWS: [Cell; 4] = [
    ("T", "", SDR, &[MKV, "HDR10 converted to SDR."]),
    ("T", "aac", SDR, &[MKV, "Dolby Vision P8.1 converted to SDR.", TRUEHD]),
    ("T", "", SDR, &[MKV, "Dolby Vision P5 converted to SDR."]),
    ("T", "aac", SDR, &[MKV, "Dolby Vision P7 FEL converted to SDR.", TRUEHD]),
];

/// A Dolby Vision screen: profiles 5 and 8 kept, 7 made 8.1; TrueHD becomes
/// E-AC-3 for a surround output (`eac3`), AAC for stereo (`aac`).
fn dolby_vision_rows(surround: &'static str) -> [Cell; 4] {
    [
        ("R", "", Some("HDR10"), &[MKV]),
        ("R+DV", surround, Some("Dolby Vision P8.1"), &[MKV, TRUEHD]),
        ("R+DV", "", Some("Dolby Vision P5"), &[MKV]),
        (
            "R+8.1",
            surround,
            Some("Dolby Vision P8.1 (converted from P7 FEL)"),
            &[MKV, "Dolby Vision P7 FEL converted to P8.1, enhancement layer dropped.", TRUEHD],
        ),
    ]
}

/// An HDR10 TV: Dolby Vision plays as its HDR10 base layer; profile 5,
/// which has none, is converted to HDR10.
const HDR10_TV_ROWS: [Cell; 4] = [
    ("R", "", Some("HDR10"), &[MKV]),
    ("R", "eac3", Some("HDR10 (Dolby Vision base layer, as Dolby Vision isn't shown here)"), &[MKV, TRUEHD]),
    ("T", "", Some("HDR10 (converted from Dolby Vision P5)"), &[MKV, "Dolby Vision P5 converted to HDR10."]),
    (
        "R",
        "eac3",
        Some("HDR10 (Dolby Vision base layer, as Dolby Vision isn't shown here)"),
        &[MKV, "Dolby Vision P7 FEL plays as HDR10.", TRUEHD],
    ),
];

/// Opus and FLAC in Matroska: the video is copied, and the audio converted
/// to AAC, since streams copy only AAC, MP3, AC-3 and E-AC-3. iPadOS 16
/// doesn't report Opus at all, so the iPad 5th generation is told why.
const OPUS_FLAC_CONVERTED: Cell = ("R", "aac", None, &[MKV]);

/// The device's row of the table, the files in `FILES`' order.
fn expected(device: &str) -> [Cell; 12] {
    let [hdr10, dv81, dv5, dv7] = match device {
        "iphone_a16_hdr" | "tv_4k_dv" => dolby_vision_rows("eac3"),
        "mac_m3_xdr" => dolby_vision_rows("aac"),
        "tv_4k_hdr10" => HDR10_TV_ROWS,
        _ => SDR_ROWS,
    };
    let av1 = if device == "mac_m3_xdr" { ("R", "", None, &[MKV] as &[&str]) } else { AV1_CONVERTED };
    let dts = if matches!(device, "iphone_a16_hdr" | "tv_4k_hdr10" | "tv_4k_dv") { "eac3" } else { "aac" };
    let opus = if device == "ipad_5th_gen" { ("R", "aac", None, &[MKV, OPUS] as &[&str]) } else { OPUS_FLAC_CONVERTED };
    [
        MP4_H264,
        MKV_H264_AC3,
        hdr10,
        dv81,
        dv5,
        dv7,
        av1,
        MPEG2_MP2,
        ("R", dts, None, &[MKV, DTS]),
        PGS_PICKED,
        opus,
        OPUS_FLAC_CONVERTED,
    ]
}

/// Every recorded Apple device against every file of the test matrix.
#[test]
fn what_each_apple_device_gets_of_each_file() {
    for device in DEVICES {
        let req = request(device);
        for (name, (method, audio, range, reasons)) in FILES.iter().zip(expected(device)) {
            let got = answer(&req, name);
            let segments = if method == "D" { got.segments } else { SegmentFormat::Fmp4 };
            let want = Answer {
                method,
                audio,
                segments,
                video_range: range.map(str::to_owned),
                reasons: reasons.iter().map(|r| (*r).to_owned()).collect(),
            };
            assert_eq!(got, want, "{device}, {name}");
        }
    }
}

#[test]
fn the_reports_are_read_as_version_2_with_fmp4_first() {
    for device in DEVICES {
        let caps = &request(device).caps;
        assert!(caps.is_v2(), "{device}");
        assert!(caps.master_playlist() && caps.adaptive(), "{device}");
        assert_eq!(caps.segment_format(true), SegmentFormat::Fmp4, "{device}");
        assert_eq!(caps.segment_format(false), SegmentFormat::Ts, "{device}");
        assert!(!caps.dolby_vision_profiles.iter().flatten().any(|p| p.profile == 7), "{device}");
    }
}

/// With a transcoder that makes no fMP4, the reports' `["fmp4"]` take
/// MPEG-TS with H.264 only, the one video codec Apple's players take from
/// it: HEVC, HDR and Dolby Vision are converted rather than copied into
/// segments AVPlayer would play without a picture, and H.264 is still
/// copied.
#[test]
fn without_fmp4_from_the_transcoder_hevc_is_converted_to_h264() {
    let mut req = request("tv_4k_dv");
    req.fmp4_segments = false;
    for name in ["MKV HEVC HDR10 E-AC-3", "MKV DV 8.1 TrueHD", "MKV DV 5"] {
        let d = answer(&req, name);
        assert_eq!((d.method, d.segments), ("T", SegmentFormat::Ts), "{name}");
        assert!(d.reasons.iter().any(|r| r == "HEVC not supported in MPEG-TS."), "{name}: {:?}", d.reasons);
    }
    // AV1 can't travel in MPEG-TS at all.
    let mut mac = request("mac_m3_xdr");
    mac.fmp4_segments = false;
    let av1 = answer(&mac, "MKV AV1 10-bit");
    assert_eq!((av1.method, av1.segments), ("T", SegmentFormat::Ts));
}

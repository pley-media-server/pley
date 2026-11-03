//! Dolby Vision in remuxes (docs/API.md, "Dolby Vision remuxes"): version 2
//! players that read multivariant playlists get Dolby Vision kept and named
//! in `master.m3u8`, profile 7 made 8.1 when they decode 8 but not 7, and
//! Dolby Vision + HDR10+ streams without their HDR10+ instead of as they
//! are. Other players decide as before (playback_caps.rs).
//!
//! The decisions come first, then the HTTP side: a stand-in transcoder
//! that speaks the transcoder API (docs/TRANSCODER.md), with and without
//! master playlists.

mod common;

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use parking_lot::Mutex;
use pley::catalog::FileRow;
use pley::playback::{Decision, Delivery, Method, PlaybackRequest, decide, video_range_shown};
use pley_transcoder_api::{
    API_VERSION, Capabilities, DolbyVisionLayer, DolbyVisionMode, DynamicRange, ErrorBody, JobCreated, JobRequest,
    JobStatus, Limits, MediaInfo, MediaStream, OutputRange, StreamKind, TranscoderInfo, VideoCodec, VideoMode,
    VideoPlan,
};
use serde_json::{Value, json};

use common::TestApp;

// ------------------------------------------------------------------ players

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/caps/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Fire TV Stick 4K Max (2023): Dolby Vision 5, 8, 9 and 10 up to level 9,
/// not 7; reads master playlists.
fn fire_tv_4k_max() -> Value {
    fixture("fire_tv_stick_4k_max_2023_v2")
}

/// The same stick as an app without master playlists would report it.
fn without_master(mut caps: Value) -> Value {
    caps["hls"]["master_playlist"] = json!(false);
    caps
}

/// NVIDIA Shield TV: Dolby Vision 4, 5, 7 (the app's quirk) and 8.
fn shield() -> Value {
    let mut caps = fire_tv_4k_max();
    caps["dolby_vision_profiles"] = json!([
        {"profile": 4, "max_level": 9}, {"profile": 5, "max_level": 9},
        {"profile": 7, "max_level": 9}, {"profile": 8, "max_level": 9}
    ]);
    caps["hdr_types"] = json!(["hdr10", "hlg", "dolby_vision"]);
    caps["device"]["model"] = json!("SHIELD Android TV");
    caps
}

/// Google TV Streamer on an HDR10 monitor: no Dolby Vision shown.
fn google_tv_streamer() -> Value {
    fixture("google_tv_streamer_v2")
}

/// Fire TV Stick Lite: HDR10 and HLG, no Dolby Vision.
fn fire_tv_lite() -> Value {
    fixture("fire_tv_stick_lite_v2")
}

fn chrome() -> Value {
    json!({
        "containers": ["mp4", "m4v", "mov", "webm", "mkv"],
        "video_codecs": ["h264", "hevc", "vp9", "av1"],
        "audio_codecs": ["aac", "mp3", "opus", "flac"],
        "hdr": true,
        "dolby_vision": false
    })
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
        bitrate: Some(20_000_000),
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

/// A 1080p HEVC Main 10 Dolby Vision stream at level 6 with E-AC-3 5.1:
/// `P5`, `P7`, `P8.1`, `P8.4`, and `P8.1+`, `P7+` with HDR10+ too.
fn source(row: &str) -> MediaInfo {
    let (profile, compat) = match row.trim_end_matches('+') {
        "P5" => (5, 0),
        "P7" => (7, 6),
        "P8.1" => (8, 1),
        "P8.4" => (8, 4),
        other => panic!("{other}"),
    };
    let video = MediaStream {
        index: 0,
        kind: StreamKind::Video,
        codec: "hevc".into(),
        profile: Some("Main 10".into()),
        width: Some(1920),
        height: Some(1080),
        frame_rate: Some(24_000.0 / 1001.0),
        bit_depth: Some(10),
        level: Some(120),
        dynamic_range: Some(DynamicRange::DolbyVision),
        dv_profile: Some(profile),
        dv_level: Some(6),
        dv_bl_compat: Some(compat),
        hdr10plus: Some(row.ends_with('+')),
        ..Default::default()
    };
    let audio = MediaStream {
        index: 1,
        kind: StreamKind::Audio,
        codec: "eac3".into(),
        channels: Some(6),
        sample_rate: Some(48_000),
        default: true,
        ..Default::default()
    };
    MediaInfo {
        container: Some("mkv".into()),
        duration_ms: Some(60_000),
        streams: vec![video, audio],
        ..Default::default()
    }
}

fn request(caps: Value) -> PlaybackRequest {
    serde_json::from_value(json!({"item_id": 1, "caps": caps})).expect("the report is read")
}

fn play_in(caps: Value, container: &str, info: &MediaInfo) -> Decision {
    decide(&file(container), Some(info), &info.streams, &request(caps))
}

/// "D": as it is; "R" the video copied, with what it does with Dolby
/// Vision ("", "+DV" kept and named, "+8.1" made 8.1) and "-HDR10+" when
/// HDR10+ goes; "T": converted.
fn outcome(d: &Decision) -> String {
    match (d.method, d.video.as_ref()) {
        (Method::Direct, _) => "D".into(),
        (_, Some(v)) if v.mode == VideoMode::Copy => {
            let dv = match v.dolby_vision {
                DolbyVisionMode::Copy => "",
                DolbyVisionMode::Signal => "+DV",
                DolbyVisionMode::To81 => "+8.1",
            };
            format!("R{dv}{}", if v.strip_hdr10plus { "-HDR10+" } else { "" })
        }
        _ => "T".into(),
    }
}

// ------------------------------------------------------------------ decisions

#[test]
fn the_dolby_vision_table_with_master_playlists() {
    let rows = ["P5", "P7", "P8.1", "P8.4", "P8.1+", "P7+"];
    #[rustfmt::skip]
    let table: [(&str, Value, [&str; 6]); 5] = [
        //                                   P5   P7      8.1  8.4  8.1+ HDR10+       P7 + HDR10+
        ("Fire TV Stick 4K Max", fire_tv_4k_max(), ["D", "R+8.1", "D", "D", "R+DV-HDR10+", "R+8.1-HDR10+"]),
        ("Shield",               shield(),         ["D", "D",     "D", "D", "R+DV-HDR10+", "R+DV-HDR10+"]),
        ("Google TV Streamer",   google_tv_streamer(), ["T", "R", "D", "T", "D",         "R"]),
        ("Fire TV Stick Lite",   fire_tv_lite(),   ["T", "R",     "D", "D", "D",         "R"]),
        ("4K Max, no master playlist", without_master(fire_tv_4k_max()), ["D", "R", "D", "D", "R", "R"]),
    ];
    for (device, caps, expected) in table {
        for (row, want) in rows.iter().zip(expected) {
            let d = play_in(caps.clone(), "mkv", &source(row));
            assert_eq!(outcome(&d), want, "{device}, {row}: {:?}", d.reasons);
        }
    }
}

#[test]
fn why_dolby_vision_is_repackaged() {
    let reasons = |caps: Value, row: &str| play_in(caps, "mkv", &source(row)).reasons;
    assert_eq!(reasons(fire_tv_4k_max(), "P7"), ["Dolby Vision P7 converted to P8.1."]);
    assert_eq!(reasons(fire_tv_4k_max(), "P8.1+"), ["Dolby Vision P8.1 + HDR10+ plays without HDR10+."]);
    // The 4K Max shows HDR10+: the base layer keeps it.
    assert_eq!(reasons(without_master(fire_tv_4k_max()), "P8.1+"), ["Dolby Vision P8.1 + HDR10+ plays as HDR10+."]);
    assert_eq!(reasons(without_master(fire_tv_4k_max()), "P7"), ["Dolby Vision P7 plays as HDR10."]);
    assert_eq!(reasons(fire_tv_4k_max(), "P7+"), ["Dolby Vision P7 + HDR10+ converted to P8.1, HDR10+ dropped."]);
}

/// A player whose Dolby Vision decoding shows Dolby Vision + HDR10+
/// (`dolby_vision_hdr10plus`, Kodi on CoreELEC) gets those as it gets the
/// rest: as they are, and HDR10+ kept where they are repackaged.
#[test]
fn a_player_that_shows_dolby_vision_with_hdr10plus_keeps_it() {
    let shows = |mut caps: Value| {
        caps["dolby_vision_hdr10plus"] = json!(true);
        caps
    };
    assert_eq!(outcome(&play_in(shows(fire_tv_4k_max()), "mkv", &source("P8.1+"))), "D");
    assert_eq!(outcome(&play_in(shows(without_master(fire_tv_4k_max())), "mkv", &source("P8.1+"))), "D");
    assert_eq!(outcome(&play_in(shows(fire_tv_4k_max()), "mkv", &source("P7+"))), "R+8.1");
    assert_eq!(outcome(&play_in(shows(fire_tv_4k_max()), "m2ts", &source("P8.1+"))), "R+DV");
    let shown = screen_gets(shows(without_master(fire_tv_4k_max())), "m2ts", &source("P8.1+"));
    assert_eq!(shown, "HDR10+ (Dolby Vision base layer, as its remux doesn't name Dolby Vision)");
}

/// What profile 7's enhancement layer holds is said where it matters: a
/// full one is left out when the stream is made profile 8.1.
#[test]
fn profile_7_says_which_layer() {
    let p7 = |el| {
        let mut info = source("P7");
        info.streams[0].dv_el = Some(el);
        info
    };
    let reasons = |caps: Value, info: &MediaInfo| play_in(caps, "mkv", info).reasons;
    assert_eq!(
        reasons(fire_tv_4k_max(), &p7(DolbyVisionLayer::Fel)),
        ["Dolby Vision P7 FEL converted to P8.1, enhancement layer dropped."]
    );
    assert_eq!(reasons(fire_tv_4k_max(), &p7(DolbyVisionLayer::Mel)), ["Dolby Vision P7 MEL converted to P8.1."]);
    assert_eq!(reasons(google_tv_streamer(), &p7(DolbyVisionLayer::Fel)), ["Dolby Vision P7 FEL plays as HDR10."]);
}

/// Profile 5 on a Dolby Vision screen whose player doesn't list it: the
/// screen isn't what's missing.
#[test]
fn profile_5_on_a_dolby_vision_screen_without_its_decoder() {
    let mut caps = fire_tv_4k_max();
    caps["dolby_vision_profiles"] = json!([{"profile": 8, "max_level": 9}]);
    let d = play_in(caps, "mkv", &source("P5"));
    assert_eq!(outcome(&d), "T");
    // (A transcoder that doesn't keep HDR: SDR.)
    assert_eq!(d.reasons, ["Dolby Vision P5 converted to SDR."]);
    let d = play_in(google_tv_streamer(), "mkv", &source("P5"));
    assert_eq!(d.reasons, ["Dolby Vision P5 converted to SDR."]);
}

/// Safari decodes Dolby Vision but opens no Matroska: profile 5 repackaged
/// would be named plain HEVC and show the wrong colours, so it's converted;
/// profiles 7 and 8 are repackaged as their base layer.
#[test]
fn version_1_profile_5_is_never_repackaged() {
    let safari = json!({
        "containers": ["mp4", "m4v", "mov"],
        "video_codecs": ["h264", "hevc"],
        "audio_codecs": ["aac", "mp3", "ac3", "eac3"],
        "hdr": true,
        "dolby_vision": true
    });
    let d = play_in(safari.clone(), "mkv", &source("P5"));
    assert_eq!(outcome(&d), "T");
    assert_eq!(d.reasons, ["MKV not supported.", "Dolby Vision P5 converted to SDR."]);
    assert_eq!(outcome(&play_in(safari.clone(), "mp4", &source("P5"))), "D");
    for row in ["P7", "P8.1", "P8.4"] {
        assert_eq!(outcome(&play_in(safari.clone(), "mkv", &source(row))), "R", "{row}");
    }
}

/// What the screen gets of `info` in `container`, as the player's details
/// say it, from a transcoder that keeps HDR.
fn screen_gets(caps: Value, container: &str, info: &MediaInfo) -> String {
    let mut req = request(caps.clone());
    req.hdr_transcodes = true;
    let d = decide(&file(container), Some(info), &info.streams, &req);
    let delivery = match (d.method, d.video.as_ref()) {
        (Method::Direct, _) => Delivery::Direct,
        (_, Some(v)) if v.mode == VideoMode::Copy => {
            Delivery::Copy { dolby_vision: v.dolby_vision, hdr10plus_stripped: v.strip_hdr10plus }
        }
        (_, Some(v)) => Delivery::Converted(v.dynamic_range.dynamic_range()),
        _ => unreachable!(),
    };
    video_range_shown(&info.streams[0], &request(caps).caps, delivery).unwrap()
}

/// What each player's screen gets, as the player's details say it.
#[test]
fn what_the_screen_gets() {
    let shown = |caps: Value, row: &str| screen_gets(caps, "mkv", &source(row));
    #[rustfmt::skip]
    let hdr10plus_screen = |mut caps: Value| {
        caps["hdr_types"] = json!(["hdr10", "hdr10plus", "hlg"]);
        caps
    };
    let table: [(&str, Value, [&str; 6]); 5] = [
        (
            "Fire TV Stick 4K Max",
            fire_tv_4k_max(),
            [
                "Dolby Vision P5",
                "Dolby Vision P8.1 (converted from P7)",
                "Dolby Vision P8.1",
                "Dolby Vision P8.4",
                "Dolby Vision P8.1 (HDR10+ left out)",
                "Dolby Vision P8.1 (converted from P7, HDR10+ left out)",
            ],
        ),
        (
            "Google TV Streamer",
            google_tv_streamer(),
            [
                "HDR10 (converted from Dolby Vision P5)",
                "HDR10 (Dolby Vision base layer, as this screen doesn't show Dolby Vision)",
                "HDR10 (Dolby Vision base layer, as this screen doesn't show Dolby Vision)",
                "HDR10 (converted from Dolby Vision P8.4)",
                "HDR10 (Dolby Vision base layer, as this screen doesn't show Dolby Vision)",
                "HDR10 (Dolby Vision base layer, as this screen doesn't show Dolby Vision)",
            ],
        ),
        // HDR10 and HLG, no HDR10+.
        (
            "Fire TV Stick Lite",
            fire_tv_lite(),
            [
                "HDR10 (converted from Dolby Vision P5)",
                "HDR10 (Dolby Vision base layer, as Dolby Vision isn't shown here)",
                "HDR10 (Dolby Vision base layer, as Dolby Vision isn't shown here)",
                "HLG (Dolby Vision base layer, as Dolby Vision isn't shown here)",
                "HDR10 (Dolby Vision base layer, as Dolby Vision isn't shown here)",
                "HDR10 (Dolby Vision base layer, as Dolby Vision isn't shown here)",
            ],
        ),
        // Its screen shows HDR10+.
        (
            "Fire TV Stick Lite, HDR10+",
            hdr10plus_screen(fire_tv_lite()),
            [
                "HDR10 (converted from Dolby Vision P5)",
                "HDR10 (Dolby Vision base layer, as Dolby Vision isn't shown here)",
                "HDR10 (Dolby Vision base layer, as Dolby Vision isn't shown here)",
                "HLG (Dolby Vision base layer, as Dolby Vision isn't shown here)",
                "HDR10+ (Dolby Vision base layer, as Dolby Vision isn't shown here)",
                "HDR10+ (Dolby Vision base layer, as Dolby Vision isn't shown here)",
            ],
        ),
        (
            "Chrome",
            chrome(),
            [
                "SDR (tone-mapped)",
                "HDR10 (Dolby Vision base layer, as Dolby Vision isn't shown here)",
                "HDR10 (Dolby Vision base layer, as Dolby Vision isn't shown here)",
                "HLG (Dolby Vision base layer, as Dolby Vision isn't shown here)",
                "HDR10+ (Dolby Vision base layer, as Dolby Vision isn't shown here)",
                "HDR10+ (Dolby Vision base layer, as Dolby Vision isn't shown here)",
            ],
        ),
    ];
    let rows = ["P5", "P7", "P8.1", "P8.4", "P8.1+", "P7+"];
    for (device, caps, expected) in table {
        for (row, want) in rows.iter().zip(expected) {
            assert_eq!(shown(caps.clone(), row), want, "{device}, {row}");
        }
    }
    // HDR10+ and the rest, without Dolby Vision.
    let plain = |range: DynamicRange| {
        let mut info = source("P8.1");
        let v = &mut info.streams[0];
        (v.dynamic_range, v.dv_profile, v.dv_bl_compat, v.dv_level) = (Some(range), None, None, None);
        v.hdr10plus = (range == DynamicRange::Hdr10plus).then_some(true);
        info.streams.remove(0)
    };
    let hdr10plus = plain(DynamicRange::Hdr10plus);
    let caps = |c: Value| request(c).caps;
    assert_eq!(video_range_shown(&hdr10plus, &caps(fire_tv_4k_max()), Delivery::Direct).unwrap(), "HDR10+");
    assert_eq!(
        video_range_shown(&hdr10plus, &caps(google_tv_streamer()), Delivery::Direct).unwrap(),
        "HDR10 (this screen doesn't show HDR10+)"
    );
    // A browser doesn't say: HDR10+ as it is.
    assert_eq!(video_range_shown(&hdr10plus, &caps(chrome()), Delivery::Direct).unwrap(), "HDR10+");
    assert_eq!(
        video_range_shown(&hdr10plus, &caps(fire_tv_lite()), Delivery::Converted(DynamicRange::Hdr10)).unwrap(),
        "HDR10 (HDR10+ left out)"
    );
    assert_eq!(
        video_range_shown(
            &plain(DynamicRange::Hlg),
            &caps(google_tv_streamer()),
            Delivery::Converted(DynamicRange::Hdr10)
        )
        .unwrap(),
        "HDR10 (converted from HLG)"
    );
    assert_eq!(video_range_shown(&plain(DynamicRange::Sdr), &caps(chrome()), Delivery::Direct), None);
    // Profile 7's layer, where it's kept or converted.
    let mut fel = source("P7").streams.remove(0);
    fel.dv_el = Some(DolbyVisionLayer::Fel);
    assert_eq!(video_range_shown(&fel, &caps(shield()), Delivery::Direct).unwrap(), "Dolby Vision P7 FEL");
    let to81 = Delivery::Copy { dolby_vision: DolbyVisionMode::To81, hdr10plus_stripped: false };
    assert_eq!(
        video_range_shown(&fel, &caps(fire_tv_4k_max()), to81).unwrap(),
        "Dolby Vision P8.1 (converted from P7 FEL)"
    );
}

/// A Dolby Vision stream that plays as its base layer says why: the screen,
/// the player, the stream's level, or a remux that can't name it.
#[test]
fn why_the_base_layer_plays() {
    let why = |caps: Value, container: &str, info: &MediaInfo| {
        let shown = screen_gets(caps, container, info);
        shown.split_once(" (Dolby Vision base layer, as ").map(|(_, why)| why.trim_end_matches(')').to_owned())
    };
    let row = |name: &str| source(name);
    // The streamer decodes Dolby Vision; its HDR10 monitor doesn't show it.
    assert_eq!(why(google_tv_streamer(), "mkv", &row("P8.1")).unwrap(), "this screen doesn't show Dolby Vision");
    assert_eq!(why(fire_tv_lite(), "mkv", &row("P8.1")).unwrap(), "Dolby Vision isn't shown here");
    // Without a master playlist profile 7 isn't converted to 8.1.
    assert_eq!(
        why(without_master(fire_tv_4k_max()), "mkv", &row("P7")).unwrap(),
        "this player doesn't decode profile 7"
    );
    assert_eq!(
        why(without_master(fire_tv_4k_max()), "mkv", &row("P8.1+")).unwrap(),
        "Dolby Vision with HDR10+ doesn't play here"
    );
    assert_eq!(
        why(without_master(fire_tv_4k_max()), "m2ts", &row("P8.1")).unwrap(),
        "its remux doesn't name Dolby Vision"
    );
    // 4K at 24 fps needs level 6, above the decoder's.
    let mut level_5 = fire_tv_4k_max();
    level_5["dolby_vision_profiles"] = json!([{"profile": 8, "max_level": 5}]);
    let mut uhd = row("P8.1");
    (uhd.streams[0].width, uhd.streams[0].height) = (Some(3840), Some(2160));
    assert_eq!(why(level_5, "mkv", &uhd).unwrap(), "level 6 is above this player's 5");
    // Version 1: a player that decodes Dolby Vision decodes no profile 7.
    let mut safari = chrome();
    safari["dolby_vision"] = json!(true);
    assert_eq!(why(safari.clone(), "mkv", &row("P7")).unwrap(), "this player doesn't decode profile 7");
    assert_eq!(why(safari, "mkv", &row("P8.1")).unwrap(), "its remux doesn't name Dolby Vision");
    assert_eq!(why(chrome(), "mkv", &row("P8.1")).unwrap(), "Dolby Vision isn't shown here");
}

#[test]
fn repackaged_dolby_vision_stays_dolby_vision() {
    // An M2TS file the 4K Max can't open: copied, with Dolby Vision named,
    // profile 5 included (no longer converted to SDR).
    for row in ["P5", "P8.1", "P8.4"] {
        let d = play_in(fire_tv_4k_max(), "m2ts", &source(row));
        assert_eq!(outcome(&d), "R+DV", "{row}: {:?}", d.reasons);
        assert_eq!(d.reasons, ["M2TS not supported."]);
    }
    // Without a master playlist the base layer (none for profile 5).
    let no_master = without_master(fire_tv_4k_max());
    assert_eq!(outcome(&play_in(no_master.clone(), "m2ts", &source("P8.1"))), "R");
    assert_eq!(outcome(&play_in(no_master, "m2ts", &source("P5"))), "T");
    // Converted video never carries Dolby Vision.
    let mut req = request(fire_tv_4k_max());
    req.max_bitrate = Some(4_000);
    let info = source("P7");
    let d = decide(&file("mkv"), Some(&info), &info.streams, &req);
    let v = d.video.unwrap();
    assert_eq!((v.mode, v.dolby_vision, v.strip_hdr10plus), (VideoMode::Transcode, DolbyVisionMode::Copy, false));
}

#[test]
fn profile_7_needs_the_8_1_level_to_be_converted() {
    // A profile 8 decoder up to level 2 takes no 1080p at 23.976 fps (3).
    let mut low = fire_tv_4k_max();
    low["dolby_vision_profiles"] = json!([{"profile": 5, "max_level": 2}, {"profile": 8, "max_level": 2}]);
    let mut info = source("P7");
    info.streams[0].dv_level = Some(10);
    let d = play_in(low, "mkv", &info);
    assert_eq!(outcome(&d), "R");
    assert_eq!(d.reasons, ["Dolby Vision P7 plays as HDR10."]);
    // 1080p at 23.976 fps that says 10 needs 3: converted, named at 3.
    let mut info = source("P7");
    info.streams[0].dv_level = Some(10);
    assert_eq!(outcome(&play_in(fire_tv_4k_max(), "mkv", &info)), "R+8.1");
    // No Dolby Vision screen: the base layer, as before.
    let mut caps = fire_tv_4k_max();
    caps["hdr_types"] = json!(["hdr10", "hlg"]);
    assert_eq!(outcome(&play_in(caps, "mkv", &source("P7"))), "R");
}

#[test]
fn a_level_the_file_overstates_is_repackaged_to_stay_dolby_vision() {
    // A re-encode's 8.1 at 4K, 23.976 fps, that says level 10: the stick
    // takes 9, and Media3 would play it as its HDR10 base layer as it is.
    let mut info = source("P8.1");
    (info.streams[0].width, info.streams[0].height) = (Some(3840), Some(1632));
    info.streams[0].dv_level = Some(10);
    let d = play_in(fire_tv_4k_max(), "mkv", &info);
    assert_eq!(outcome(&d), "R+DV");
    assert_eq!(d.reasons, ["Dolby Vision level 10 relabelled 6."]);
    // Profile 5 the same, rather than converted.
    let mut p5 = source("P5");
    p5.streams[0].dv_level = Some(10);
    assert_eq!(outcome(&play_in(fire_tv_4k_max(), "mkv", &p5)), "R+DV");
    // With HDR10+ too, that goes as for any Dolby Vision decoder.
    let mut hybrid = source("P8.1+");
    hybrid.streams[0].dv_level = Some(10);
    assert_eq!(outcome(&play_in(fire_tv_4k_max(), "mkv", &hybrid)), "R+DV-HDR10+");
    // Without a master playlist to name it: the base layer, as it is.
    assert_eq!(outcome(&play_in(without_master(fire_tv_4k_max()), "mkv", &info)), "D");
    // A download is the file as it is.
    let mut req = request(fire_tv_4k_max());
    req.download = true;
    assert_eq!(decide(&file("mkv"), Some(&info), &info.streams, &req).method, Method::Direct);
    // Really 4K at 120 fps: level 10 it is, past the stick's Dolby Vision.
    info.streams[0].frame_rate = Some(120.0);
    assert!(!outcome(&play_in(fire_tv_4k_max(), "mkv", &info)).contains("DV"));
}

#[test]
fn profile_8_with_the_blu_ray_id_has_an_hdr10_base_layer() {
    // A profile 7 converted to 8 elsewhere, its compatibility id (6) kept.
    let mut info = source("P8.1");
    info.streams[0].dv_bl_compat = Some(6);
    // As it is where Dolby Vision is shown; its HDR10 where it isn't, never converted.
    assert_eq!(outcome(&play_in(fire_tv_4k_max(), "mkv", &info)), "D");
    let d = play_in(google_tv_streamer(), "mkv", &info);
    assert_eq!(outcome(&d), "D", "{:?}", d.reasons);
}

#[test]
fn unknown_hdr10_plus_plays_as_before() {
    // Probed before HDR10+ was looked for: as it is.
    let mut info = source("P8.1");
    info.streams[0].hdr10plus = None;
    assert_eq!(outcome(&play_in(fire_tv_4k_max(), "mkv", &info)), "D");
}

#[test]
fn version_1_players_are_untouched() {
    // Chrome: profiles 7 and 8 copied as their base layer, Dolby Vision
    // never named, HDR10+ never taken out.
    for row in ["P7", "P8.1", "P8.1+"] {
        let d = play_in(chrome(), "mkv", &source(row));
        assert_eq!(outcome(&d), "R", "{row}");
    }
    let hls = json!({"containers": [], "video_codecs": ["h264"], "audio_codecs": ["aac", "mp3"]});
    assert_eq!(outcome(&play_in(hls, "mkv", &source("P8.1+"))), "T");
}

// ------------------------------------------------------------------ the stand-in

const TOKEN: &str = "shared-secret";
const MASTER: &str = "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-INDEPENDENT-SEGMENTS\n#EXT-X-STREAM-INF:BANDWIDTH=1,\
                      CODECS=\"hvc1.2.4.L120.B0,ec-3\",SUPPLEMENTAL-CODECS=\"dvh1.08.06/db1p\",VIDEO-RANGE=PQ\n\
                      index.m3u8\n";

#[derive(Clone, Default)]
struct Fake {
    /// Answers `hls_master_playlist` in its capabilities.
    masters: Arc<AtomicBool>,
    /// Answers `hdr_transcode` in its capabilities.
    hdr: Arc<AtomicBool>,
    created: Arc<Mutex<Vec<JobRequest>>>,
}

fn error(status: StatusCode, code: &str) -> Response {
    (status, Json(ErrorBody { code: code.into(), message: format!("{code} (stand-in)") })).into_response()
}

fn job_status(req: &JobRequest) -> JobStatus {
    JobStatus {
        id: "job1".into(),
        input: req.input.clone(),
        duration_ms: 60_000,
        segment_seconds: 2.0,
        segment_count: 30,
        video: req.video.as_ref().map(|v| VideoPlan {
            mode: v.mode,
            codec_in: "hevc".into(),
            codec_out: if v.mode == VideoMode::Copy || v.codec == VideoCodec::Hevc { "hevc" } else { "h264" }.into(),
            dynamic_range_in: DynamicRange::DolbyVision,
            dynamic_range_out: (v.mode == VideoMode::Transcode).then(|| v.dynamic_range.dynamic_range()),
            dv_profile: Some(7),
            dolby_vision: Some(v.dolby_vision),
            hdr10plus_stripped: v.strip_hdr10plus,
            ..Default::default()
        }),
        ..Default::default()
    }
}

async fn start_fake(masters: bool, hdr: bool) -> (Fake, String) {
    let fake = Fake::default();
    fake.masters.store(masters, Ordering::SeqCst);
    fake.hdr.store(hdr, Ordering::SeqCst);
    let app = Router::new()
        .route(
            "/v1/info",
            get(|State(fake): State<Fake>, headers: HeaderMap| async move {
                let bearer = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok());
                if bearer != Some(&format!("Bearer {TOKEN}")) {
                    return error(StatusCode::UNAUTHORIZED, "unauthorized");
                }
                Json(TranscoderInfo {
                    name: "pley-transcoder".into(),
                    version: "9.9.9".into(),
                    api_version: API_VERSION,
                    capabilities: Capabilities {
                        ffmpeg_available: true,
                        ffprobe_available: true,
                        hls_master_playlist: fake.masters.load(Ordering::SeqCst),
                        hdr_transcode: fake.hdr.load(Ordering::SeqCst),
                        hls_variants: true,
                        ..Default::default()
                    },
                    limits: Limits { max_jobs: 8, max_hw_jobs: 0 },
                    ..Default::default()
                })
                .into_response()
            }),
        )
        // Every file is a Dolby Vision profile 7 film with E-AC-3.
        .route(
            "/v1/probe",
            post(|| async {
                let mut info = source("P7");
                info.start_ms = Some(0);
                Json(info)
            }),
        )
        .route(
            "/v1/jobs",
            post(|State(fake): State<Fake>, Json(req): Json<JobRequest>| async move {
                let status = job_status(&req);
                let variants = req.variants.len() as u32;
                fake.created.lock().push(req);
                Json(JobCreated {
                    id: "job1".into(),
                    duration_ms: 60_000,
                    segment_seconds: 2.0,
                    segment_count: 30,
                    video: status.video,
                    audio: None,
                    warnings: vec![],
                    variants,
                })
            }),
        )
        .route(
            "/v1/jobs/{id}",
            get(|State(fake): State<Fake>| async move {
                let req = fake.created.lock().last().cloned().unwrap();
                Json(job_status(&req))
            })
            .delete(|| async { StatusCode::NO_CONTENT }),
        )
        .route(
            "/v1/jobs/{id}/{file}",
            get(|State(fake): State<Fake>, Path((_, file)): Path<(String, String)>| async move {
                let playlist = match file.as_str() {
                    "master.m3u8" if fake.masters.load(Ordering::SeqCst) => MASTER.to_owned(),
                    "index.m3u8" => {
                        "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXTINF:2.000,\n0.ts\n#EXT-X-ENDLIST\n".to_owned()
                    }
                    "v1.m3u8" => {
                        "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXTINF:2.000,\nv1-0.ts\n#EXT-X-ENDLIST\n".to_owned()
                    }
                    // A lower rendition's segment, as the job's own would come.
                    "v1-0.ts" => return ([(header::CONTENT_TYPE, "video/mp2t")], vec![0x47u8; 188]).into_response(),
                    _ => return error(StatusCode::NOT_FOUND, "not_found"),
                };
                ([(header::CONTENT_TYPE, "application/vnd.apple.mpegurl")], playlist).into_response()
            }),
        )
        .route("/v1/keyframes", post(|| async { StatusCode::ACCEPTED }))
        .route("/v1/prefetch", post(|| async { Json(pley_transcoder_api::Prefetched::default()) }))
        .with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (fake, format!("http://{addr}"))
}

/// pley with the stand-in as its transcoder and a movie: the admin's token
/// and the movie's id.
async fn world(masters: bool) -> (TestApp, Fake, String, i64) {
    world_with(masters, false).await
}

/// [`world`] with a stand-in that keeps HDR in transcodes or not.
async fn world_with(masters: bool, hdr: bool) -> (TestApp, Fake, String, i64) {
    let (fake, url) = start_fake(masters, hdr).await;
    let app = TestApp::with_config(|c| {
        c.transcoder_url = Some(url);
        c.transcoder_token = Some(TOKEN.into());
    })
    .await;
    app.state.transcoder.start();
    assert!(app.state.transcoder.wait_ready(Duration::from_secs(10)).await);
    let admin = app.setup_admin().await;
    app.put("/api/settings", &admin, json!({"seek_previews": false})).await;
    app.media_file("movies/Remux (2020)/Remux (2020).mkv", 4096);
    let library = app.library(&admin, "Movies", "movies", "movies").await;
    let movie = app.ids(library, &admin).await["Remux"];
    (app, fake, admin, movie)
}

#[tokio::test]
async fn master_playlists_reach_players_that_read_them() {
    let (app, fake, admin, movie) = world(true).await;
    let started = app.post("/api/playback", &admin, json!({"item_id": movie, "caps": fire_tv_4k_max()})).await;
    assert_eq!(started.status, StatusCode::OK, "{:?}", started.body);
    let info = &started.body;
    let session = info["session_id"].as_str().unwrap().to_owned();
    assert_eq!(info["method"], json!("remux"));
    assert_eq!(info["url"], json!(format!("/api/playback/{session}/master.m3u8")));
    assert_eq!(info["reasons"], json!(["Dolby Vision P7 converted to P8.1."]));
    let job = fake.created.lock().last().cloned().unwrap();
    let video = job.video.unwrap();
    assert_eq!(
        (video.mode, video.dolby_vision, video.strip_hdr10plus),
        (VideoMode::Copy, DolbyVisionMode::To81, false)
    );

    // The master playlist as the transcoder wrote it; the media playlist
    // it points to beside it.
    let master = app.get(&format!("/api/playback/{session}/master.m3u8"), &admin).await;
    assert_eq!(master.status, StatusCode::OK);
    assert_eq!(master.headers[header::CONTENT_TYPE], "application/vnd.apple.mpegurl");
    assert_eq!(master.text(), MASTER);
    assert_eq!(app.get(&format!("/api/playback/{session}/index.m3u8"), &admin).await.status, StatusCode::OK);
    // The player's details say what became of Dolby Vision.
    let status = app.get(&format!("/api/playback/{session}"), &admin).await;
    assert_eq!(status.body["transcode"]["dolby_vision"], json!("converted to 8.1"), "{:?}", status.body);
    assert_eq!(status.body["transcode"]["hdr10plus_stripped"], json!(false));

    // The web player keeps its media playlist.
    let web = app.post("/api/playback", &admin, json!({"item_id": movie, "caps": chrome()})).await;
    let session = web.body["session_id"].as_str().unwrap();
    assert_eq!(web.body["url"], json!(format!("/api/playback/{session}/index.m3u8")));
    assert_eq!(fake.created.lock().last().unwrap().video.as_ref().unwrap().dolby_vision, DolbyVisionMode::Copy);
}

#[tokio::test]
async fn players_that_adapt_get_transcodes_in_several_sizes() {
    let (app, fake, admin, movie) = world(true).await;
    let mut caps = chrome();
    caps["hls"] = json!({"variants": true});
    let started = app.post("/api/playback", &admin, json!({"item_id": movie, "caps": caps, "mode": "transcode"})).await;
    assert_eq!(started.status, StatusCode::OK, "{:?}", started.body);
    let session = started.body["session_id"].as_str().unwrap().to_owned();
    assert_eq!(started.body["adaptive"], json!(true));
    assert_eq!(started.body["url"], json!(format!("/api/playback/{session}/master.m3u8")));
    // The 1080p film's own size at its quality (20 Mbps), again at 1080p's
    // own 8, then 720p, 540p and 360p.
    let job = fake.created.lock().last().cloned().unwrap();
    assert_eq!(job.video.as_ref().unwrap().max_bitrate_kbps, Some(19_808));
    let rungs: Vec<_> = job.variants.iter().map(|v| (v.max_width, v.max_height, v.max_bitrate_kbps)).collect();
    assert_eq!(
        rungs,
        [
            (None, Some(1080), Some(8_000)),
            (Some(1280), Some(720), Some(4_000)),
            (Some(960), Some(540), Some(2_000)),
            (Some(640), Some(360), Some(700)),
        ]
    );

    // A lower rendition's playlist and segments come through the session,
    // and nothing else by such names.
    assert_eq!(app.get(&format!("/api/playback/{session}/v1.m3u8"), &admin).await.status, StatusCode::OK);
    let segment = app.get(&format!("/api/playback/{session}/v1-0.ts"), &admin).await;
    assert_eq!(
        (segment.status, segment.headers[header::CONTENT_TYPE].to_str().unwrap()),
        (StatusCode::OK, "video/mp2t")
    );
    for name in ["v0.m3u8", "v1-x.ts", "v1.ts", "status.m3u8", "..%2Fstatus"] {
        let answer = app.get(&format!("/api/playback/{session}/{name}"), &admin).await;
        assert_eq!(answer.status, StatusCode::NOT_FOUND, "{name}");
    }

    // A player that doesn't say so gets one size, as before.
    let plain =
        app.post("/api/playback", &admin, json!({"item_id": movie, "caps": chrome(), "mode": "transcode"})).await;
    assert_eq!(plain.body["adaptive"], json!(false));
    assert!(fake.created.lock().last().unwrap().variants.is_empty());
}

#[tokio::test]
async fn older_transcoders_get_requests_as_before() {
    let (app, fake, admin, movie) = world(false).await;
    let started = app.post("/api/playback", &admin, json!({"item_id": movie, "caps": fire_tv_4k_max()})).await;
    assert_eq!(started.status, StatusCode::OK, "{:?}", started.body);
    let session = started.body["session_id"].as_str().unwrap();
    assert_eq!(started.body["url"], json!(format!("/api/playback/{session}/index.m3u8")));
    assert_eq!(started.body["reasons"], json!(["Dolby Vision P7 plays as HDR10."]));
    let video = fake.created.lock().last().cloned().unwrap().video.unwrap();
    assert_eq!((video.mode, video.dolby_vision), (VideoMode::Copy, DolbyVisionMode::Copy));
}

/// The profile 7 film converted for the Google TV Streamer: HEVC Main 10 in
/// HDR10 from a transcoder that keeps HDR, H.264 in SDR from one that
/// doesn't.
#[tokio::test]
async fn transcodes_keep_hdr_where_the_transcoder_can() {
    for (hdr, codec, range, video_out) in [
        (true, VideoCodec::Hevc, OutputRange::Hdr10, "HEVC 0×0 HDR10"),
        (false, VideoCodec::H264, OutputRange::Sdr, "H.264 0×0"),
    ] {
        let (app, fake, admin, movie) = world_with(true, hdr).await;
        let body = json!({"item_id": movie, "caps": google_tv_streamer(), "mode": "transcode"});
        let started = app.post("/api/playback", &admin, body).await;
        assert_eq!(started.body["method"], json!("transcode"), "{:?}", started.body);
        let video = fake.created.lock().last().cloned().unwrap().video.unwrap();
        assert_eq!((video.mode, video.codec, video.dynamic_range), (VideoMode::Transcode, codec, range));
        // The player's details say what it comes out as.
        let session = started.body["session_id"].as_str().unwrap();
        let status = app.get(&format!("/api/playback/{session}"), &admin).await;
        assert_eq!(status.body["transcode"]["video_out"], json!(video_out), "{:?}", status.body);
    }
}

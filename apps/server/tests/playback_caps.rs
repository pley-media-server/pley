//! Playback decisions for version 2 capability reports (docs/API.md,
//! "Version 2"), as the native Android app sends them: the Dolby Vision and
//! HDR table per device, decoder limits, audio passed through to receivers,
//! surround audio converted to E-AC-3 5.1, and version 1 players (the web
//! app, Jellyfin's apps' HLS streams) deciding as before.
//!
//! The reports in `fixtures/caps` are the Android app's own snapshots
//! (apps/android/core/caps/src/test/resources/snapshots on the
//! android-native branch), read here as the server reads them.

use pley::catalog::FileRow;
use pley::playback::{Decision, Method, PlaybackRequest, decide};
use pley_transcoder_api::{
    AudioCodec, AudioMode, DynamicRange, MediaInfo, MediaStream, OutputRange, StreamKind, VideoCodec, VideoMode,
};
use serde_json::{Value, json};

// ------------------------------------------------------------------ devices

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/caps/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Fire TV Stick Lite (T0): 1080p decoders, HEVC Main 10 up to level 4.1,
/// no AV1, no Dolby Vision; HDR10 and HLG; AC-3, E-AC-3 and DTS passed
/// through to a 7.1 receiver.
fn fire_tv_lite() -> Value {
    fixture("fire_tv_stick_lite_v2")
}

/// Fire TV Stick 4K Max (2023): 4K60 HEVC and AV1, Dolby Vision 5, 8, 9 and
/// 10 but not 7, every HDR type; TrueHD and DTS passed through.
fn fire_tv_4k_max() -> Value {
    fixture("fire_tv_stick_4k_max_2023_v2")
}

/// Galaxy Note20 Ultra: an HDR10 phone, software AV1 only (8-bit, 1080p30),
/// stereo output.
fn hdr10_phone() -> Value {
    fixture("galaxy_note20_ultra_v2")
}

/// NVIDIA Shield TV: 4K HEVC, no AV1; Dolby Vision 4, 5, 7 (the quirk) and
/// 8; HDR10 and HLG but no HDR10+; lossless audio passed through.
fn shield() -> Value {
    json!({
        "containers": ["mkv", "mp4", "ts"],
        "video_codecs": ["h264", "hevc", "vp9"],
        "audio_codecs": ["aac", "ac3", "eac3", "truehd", "dts"],
        "hdr": true,
        "dolby_vision": true,
        "caps_version": 2,
        "video": [
            {"codec": "h264", "hardware": true, "profiles": ["Main", "High"], "max_width": 3840, "max_height": 2160,
             "max_fps": 60.0, "max_bit_depth": 8, "max_level": 52},
            {"codec": "hevc", "hardware": true, "profiles": ["Main", "Main 10"], "max_width": 3840, "max_height": 2160,
             "max_fps": 60.0, "max_bit_depth": 10, "max_level": 153},
            {"codec": "vp9", "hardware": true, "profiles": ["Profile 0", "Profile 2"], "max_width": 3840,
             "max_height": 2160, "max_fps": 60.0, "max_bit_depth": 10}
        ],
        "hdr_types": ["hdr10", "hlg", "dolby_vision"],
        "dolby_vision_profiles": [
            {"profile": 4, "max_level": 9}, {"profile": 5, "max_level": 9},
            {"profile": 7, "max_level": 9}, {"profile": 8, "max_level": 9}
        ],
        "audio_decode": ["aac", "mp3", "ac3", "eac3", "flac"],
        "audio_passthrough": ["ac3", "eac3", "truehd", "dts"],
        "audio_channels": 8,
        "subtitle_codecs": ["subrip", "ass", "hdmv_pgs_subtitle"],
        "audio_track_switching": true,
        "hls": {"master_playlist": true, "segments": ["ts", "fmp4"]},
        "display": {"width": 3840, "height": 2160, "refresh_rates": [23.976, 24.0, 50.0, 59.94, 60.0]},
        "device": {"form": "tv", "tier": "high", "fire_tv": false, "model": "SHIELD Android TV"}
    })
}

/// Chromecast with Google TV HD: 1080p60 decoders, AV1 included; every HDR
/// type but Dolby Vision; AC-3 and E-AC-3 passed through to a 5.1 system.
/// Two entries for HEVC (4K at 30 fps would be a third).
fn chromecast_hd() -> Value {
    json!({
        "containers": ["mkv", "mp4"],
        "video_codecs": ["h264", "hevc", "vp9", "av1"],
        "audio_codecs": ["aac", "ac3", "eac3"],
        "hdr": true,
        "caps_version": 2,
        "video": [
            {"codec": "h264", "hardware": true, "profiles": ["Main", "High"], "max_width": 1920, "max_height": 1080,
             "max_fps": 60.0, "max_bit_depth": 8, "max_level": 42},
            {"codec": "hevc", "hardware": true, "profiles": ["Main", "Main 10"], "max_width": 1920, "max_height": 1080,
             "max_fps": 60.0, "max_bit_depth": 10, "max_level": 123},
            {"codec": "hevc", "hardware": true, "profiles": ["Main"], "max_width": 1280, "max_height": 720,
             "max_fps": 120.0, "max_bit_depth": 8, "max_level": 123},
            {"codec": "av1", "hardware": true, "profiles": ["Main"], "max_width": 1920, "max_height": 1080,
             "max_fps": 60.0, "max_bit_depth": 10, "max_level": 9}
        ],
        "hdr_types": ["hdr10", "hdr10plus", "hlg"],
        "dolby_vision_profiles": [],
        "audio_decode": ["aac", "mp3", "opus", "flac"],
        "audio_passthrough": ["ac3", "eac3"],
        "audio_channels": 6,
        "audio_track_switching": true,
        "device": {"form": "tv", "tier": "low", "fire_tv": false, "model": "Chromecast HD"}
    })
}

/// A cheap phone with an SDR screen: 1080p, 8-bit HEVC only.
fn sdr_phone() -> Value {
    json!({
        "containers": ["mkv", "mp4"],
        "video_codecs": ["h264", "hevc"],
        "audio_codecs": ["aac", "ac3", "eac3"],
        "caps_version": 2,
        "video": [
            {"codec": "h264", "hardware": true, "profiles": ["Main", "High"], "max_width": 1920, "max_height": 1080,
             "max_fps": 60.0, "max_bit_depth": 8, "max_level": 42},
            {"codec": "hevc", "hardware": true, "profiles": ["Main"], "max_width": 1920, "max_height": 1080,
             "max_fps": 60.0, "max_bit_depth": 8, "max_level": 123}
        ],
        "hdr_types": [],
        "audio_decode": ["aac", "mp3", "ac3", "eac3"],
        "audio_passthrough": [],
        "audio_channels": 2,
        "audio_track_switching": true,
        "device": {"form": "phone", "tier": "mid", "fire_tv": false, "model": "Moto G"}
    })
}

/// Chrome on Windows, as apps/web/src/lib/caps.ts reports it: version 1.
fn chrome() -> Value {
    json!({
        "containers": ["mp4", "m4v", "mov", "webm", "mkv"],
        "video_codecs": ["h264", "hevc", "vp9", "av1"],
        "audio_codecs": ["aac", "mp3", "opus", "flac"],
        "hdr": true,
        "dolby_vision": false
    })
}

/// What the Jellyfin-compatible API's HLS streams ask for
/// (apps/server/src/jellyfin/playback.rs, `master`).
fn jellyfin_hls() -> Value {
    json!({"containers": [], "video_codecs": ["h264"], "audio_codecs": ["aac", "mp3"]})
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

/// A 1080p, 23.976 fps, 10-bit video stream of `range`.
fn video(codec: &str, range: DynamicRange) -> MediaStream {
    MediaStream {
        index: 0,
        kind: StreamKind::Video,
        codec: codec.into(),
        profile: Some(if codec == "av1" { "Main" } else { "Main 10" }.into()),
        width: Some(1920),
        height: Some(1080),
        frame_rate: Some(24_000.0 / 1001.0),
        bit_depth: Some(10),
        level: Some(if codec == "av1" { 8 } else { 120 }),
        dynamic_range: Some(range),
        ..Default::default()
    }
}

fn audio(index: i64, codec: &str, channels: i64, default: bool) -> MediaStream {
    MediaStream {
        index,
        kind: StreamKind::Audio,
        codec: codec.into(),
        channels: Some(channels),
        sample_rate: Some(48_000),
        default,
        ..Default::default()
    }
}

fn media(video: MediaStream, audio: Vec<MediaStream>) -> MediaInfo {
    let mut streams = vec![video];
    streams.extend(audio);
    streams.push(MediaStream {
        index: 9,
        kind: StreamKind::Subtitle,
        codec: "hdmv_pgs_subtitle".into(),
        text_based: Some(false),
        ..Default::default()
    });
    MediaInfo { container: Some("mkv".into()), duration_ms: Some(60_000), streams, ..Default::default() }
}

/// The rows of the Dolby Vision and HDR table, 1080p HEVC (AV1 for profile
/// 10) with E-AC-3 5.1.
fn source(row: &str) -> MediaInfo {
    let dv = |profile: i64, compat: i64, codec: &str| {
        let mut v = video(codec, DynamicRange::DolbyVision);
        (v.dv_profile, v.dv_level, v.dv_bl_compat) = (Some(profile), Some(6), Some(compat));
        v
    };
    let v = match row {
        "P5" => dv(5, 0, "hevc"),
        "P7" => dv(7, 6, "hevc"),
        "P8.1" => dv(8, 1, "hevc"),
        "P8.4" => dv(8, 4, "hevc"),
        "P10" => dv(10, 1, "av1"),
        "HDR10" => video("hevc", DynamicRange::Hdr10),
        "HDR10+" => video("hevc", DynamicRange::Hdr10plus),
        "HLG" => video("hevc", DynamicRange::Hlg),
        "SDR" => {
            let mut v = video("hevc", DynamicRange::Sdr);
            (v.profile, v.bit_depth) = (Some("Main".into()), Some(8));
            v
        }
        other => panic!("{other}"),
    };
    media(v, vec![audio(1, "eac3", 6, true)])
}

fn request(caps: Value) -> PlaybackRequest {
    serde_json::from_value(json!({"item_id": 1, "caps": caps})).expect("the report is read")
}

fn play(caps: Value, info: &MediaInfo) -> Decision {
    play_with(request(caps), "mkv", info)
}

fn play_with(req: PlaybackRequest, container: &str, info: &MediaInfo) -> Decision {
    decide(&file(container), Some(info), &info.streams, &req)
}

/// "D": plays as it is; "R": the video copied into HLS; "T": the video
/// converted.
fn outcome(d: &Decision) -> &'static str {
    match (d.method, d.video.as_ref().map(|v| v.mode)) {
        (Method::Direct, _) => "D",
        (_, Some(VideoMode::Copy)) => "R",
        _ => "T",
    }
}

// ------------------------------------------------------------------ the table

#[test]
fn the_dolby_vision_and_hdr_table_per_device() {
    let rows = ["P5", "P7", "P8.1", "P8.4", "P10", "HDR10", "HDR10+", "HLG", "SDR"];
    #[rustfmt::skip]
    let table: [(&str, Value, [&str; 9]); 6] = [
        //                               P5   P7   8.1  8.4  P10  HDR10 HDR10+ HLG SDR
        ("Fire TV Stick Lite",  fire_tv_lite(),   ["T", "R", "D", "D", "T", "D", "D", "D", "D"]),
        ("Fire TV Stick 4K Max", fire_tv_4k_max(), ["D", "R", "D", "D", "D", "D", "D", "D", "D"]),
        ("Shield",              shield(),          ["D", "D", "D", "D", "T", "D", "D", "D", "D"]),
        ("Chromecast HD",       chromecast_hd(),   ["T", "R", "D", "D", "D", "D", "D", "D", "D"]),
        ("HDR10 phone",         hdr10_phone(),     ["T", "R", "D", "D", "T", "D", "D", "D", "D"]),
        ("SDR phone",           sdr_phone(),       ["T", "T", "T", "T", "T", "T", "T", "T", "D"]),
    ];
    for (device, caps, expected) in table {
        for (row, want) in rows.iter().zip(expected) {
            let d = play(caps.clone(), &source(row));
            assert_eq!(outcome(&d), want, "{device}, {row}: {:?}", d.reasons);
            if want == "D" {
                assert!(d.reasons.is_empty(), "{device}, {row}: {:?}", d.reasons);
            }
        }
    }
}

#[test]
fn why_each_row_is_not_played_as_it_is() {
    let reasons = |caps: Value, row: &str| play(caps, &source(row)).reasons;
    // Profile 5 has no base layer: tone-mapped, never repackaged.
    assert_eq!(reasons(fire_tv_lite(), "P5"), ["Dolby Vision P5 converted to SDR."]);
    // Profile 7: its HDR10 base layer, repackaged; tone-mapped on an SDR
    // screen. (Made 8.1 for Dolby Vision players that read master
    // playlists: playback_dv.rs.)
    let mut no_master = fire_tv_4k_max();
    no_master["hls"]["master_playlist"] = json!(false);
    assert_eq!(reasons(no_master, "P7"), ["Dolby Vision P7 plays as HDR10."]);
    assert_eq!(reasons(fire_tv_4k_max(), "P7"), ["Dolby Vision P7 converted to P8.1."]);
    assert_eq!(reasons(sdr_phone(), "P7"), ["Dolby Vision P7 converted to SDR."]);
    assert_eq!(reasons(sdr_phone(), "HDR10+"), ["HDR10+ converted to SDR."]);
    // Profile 10 is AV1 underneath: no 10-bit AV1 decoder, no play.
    assert_eq!(reasons(hdr10_phone(), "P10"), ["AV1 1920×1080 23.976 fps 10-bit not supported."]);
    assert_eq!(reasons(shield(), "P10"), ["AV1 video not supported."]);
    // 10-bit HEVC needs Main 10.
    let mut sdr10 = source("SDR");
    (sdr10.streams[0].profile, sdr10.streams[0].bit_depth) = (Some("Main 10".into()), Some(10));
    assert_eq!(play(sdr_phone(), &sdr10).reasons, ["HEVC 1920×1080 23.976 fps 10-bit not supported."]);
}

#[test]
fn hdr10_plus_plays_on_hdr10_screens_and_hlg_needs_hlg() {
    let mut caps = shield();
    caps["hdr_types"] = json!(["hdr10"]);
    assert_eq!(outcome(&play(caps.clone(), &source("HDR10+"))), "D", "HDR10+ shows as HDR10");
    let d = play(caps, &source("HLG"));
    assert_eq!(outcome(&d), "T");
    assert_eq!(d.reasons, ["HLG converted to SDR."]);
    let mut caps = fire_tv_4k_max();
    caps["hdr_types"] = json!(["hdr10plus"]);
    assert_eq!(outcome(&play(caps, &source("HDR10+"))), "D");
}

#[test]
fn dolby_vision_needs_the_screen_the_profile_and_its_level() {
    // The profile listed but no Dolby Vision screen: the base layer plays.
    let mut caps = fire_tv_4k_max();
    caps["hdr_types"] = json!(["hdr10", "hlg"]);
    assert_eq!(outcome(&play(caps.clone(), &source("P8.1"))), "D");
    assert_eq!(outcome(&play(caps, &source("P5"))), "T", "no base layer to fall back to");

    // A level above the decoder's that the picture (1080p at 23.976 fps)
    // doesn't need: repackaged, named at the one it needs.
    for row in ["P8.1", "P5"] {
        let mut said = source(row);
        said.streams[0].dv_level = Some(10);
        let d = play(fire_tv_4k_max(), &said);
        assert_eq!(outcome(&d), "R", "{row}: {:?}", d.reasons);
        assert!(d.reasons[0].contains("relabelled"), "{:?}", d.reasons);
    }
    // One it needs (4K at 120 fps): no Dolby Vision here.
    let mut high = source("P5");
    (high.streams[0].width, high.streams[0].height, high.streams[0].frame_rate) = (Some(3840), Some(2160), Some(120.0));
    high.streams[0].dv_level = Some(10);
    let d = play(fire_tv_4k_max(), &high);
    assert_eq!(outcome(&d), "T");
    assert_eq!(d.reasons[0], "Dolby Vision level 10 not supported.");

    // A profile 8 file that doesn't say its base layer is HDR10's.
    let mut p8 = source("P8.1");
    p8.streams[0].dv_bl_compat = None;
    assert_eq!(outcome(&play(fire_tv_lite(), &p8)), "D");
}

#[test]
fn a_4k_file_converted_for_a_1080p_player_is_made_in_1080p() {
    let mut info = source("HDR10");
    (info.streams[0].width, info.streams[0].height, info.streams[0].level) = (Some(3840), Some(2160), Some(153));
    let mut big = file("mkv");
    big.bitrate = Some(57_000_000);
    let req = request(fire_tv_lite());
    let d = decide(&big, Some(&info), &info.streams, &req);
    let v = d.video.as_ref().unwrap();
    assert_eq!((d.method, v.mode), (Method::Transcode, VideoMode::Transcode));
    assert_eq!((d.quality, v.max_height, v.max_bitrate_kbps), (Some("20m"), Some(1080), Some(19_808)));
    assert_eq!(d.reasons, ["HEVC 3840×2160 23.976 fps 10-bit not supported."]);

    // A quality picked above what the player decodes is made at its most.
    let mut req = request(fire_tv_lite());
    (req.max_height, req.max_bitrate) = (Some(2160), Some(30_000));
    let d = decide(&big, Some(&info), &info.streams, &req);
    assert_eq!(d.video.as_ref().unwrap().max_height, Some(1080));

    // The 4K stick plays it, and copies it when only the audio needs work.
    let d = decide(&big, Some(&info), &info.streams, &request(fire_tv_4k_max()));
    assert_eq!(d.method, Method::Direct);
    // A 1080p60 entry next to a 4K30 one: a 4K60 file fits neither.
    let mut caps = fire_tv_4k_max();
    caps["video"][1]["max_fps"] = json!(30.0);
    caps["video"].as_array_mut().unwrap().push(json!({"codec": "hevc", "hardware": true,
        "profiles": ["Main", "Main 10"], "max_width": 1920, "max_height": 1080, "max_fps": 60.0,
        "max_bit_depth": 10, "max_level": 153}));
    assert_eq!(outcome(&play(caps.clone(), &info)), "D", "4K at 23.976 fits the first");
    info.streams[0].frame_rate = Some(60.0);
    assert_eq!(outcome(&play(caps.clone(), &info)), "T");
    (info.streams[0].width, info.streams[0].height) = (Some(1920), Some(1080));
    assert_eq!(outcome(&play(caps, &info)), "D", "1080p60 fits the second");
}

#[test]
fn a_download_is_made_for_the_players_screen() {
    let mut info = source("HDR10");
    (info.streams[0].width, info.streams[0].height, info.streams[0].level) = (Some(3840), Some(2160), Some(153));
    let mut big = file("mkv");
    big.bitrate = Some(57_000_000);
    let download = |caps: Value, file: &FileRow, info: &MediaInfo| {
        let mut req = request(caps);
        req.download = true;
        decide(file, Some(info), &info.streams, &req)
    };
    let made = |d: &Decision| {
        let v = d.video.as_ref().unwrap();
        (v.mode, v.max_height, v.max_bitrate_kbps)
    };

    // A 4K film the phone can't play: 1080p, what its screen shows (its
    // decoders go to 4K), at the quality a playback would aim for there.
    let mut phone = hdr10_phone();
    phone["video"].as_array_mut().unwrap().retain(|d| d["codec"] != "hevc");
    let d = download(phone.clone(), &big, &info);
    assert_eq!(made(&d), (VideoMode::Transcode, Some(1080), Some(19_808)));
    // Streamed, the same film is made in 4K.
    let streamed = play_with(request(phone.clone()), "mkv", &info);
    assert_eq!(streamed.video.unwrap().max_height, Some(2160));

    // With fewer bits than any 1080p quality: half again the original's.
    let mut small = file("mkv");
    small.bitrate = Some(2_000_000);
    let d = download(phone.clone(), &small, &info);
    assert_eq!(made(&d), (VideoMode::Transcode, Some(1080), Some(2_808)));
    // A quality asked for, too.
    let mut req = request(phone.clone());
    (req.download, req.max_height, req.max_bitrate) = (true, Some(720), Some(4_000));
    let d = decide(&small, Some(&info), &info.streams, &req);
    assert_eq!(made(&d), (VideoMode::Transcode, Some(720), Some(2_808)));

    // A 4K TV's screen takes 4K; copied video stays as it is.
    let mut tv = shield();
    tv["video"].as_array_mut().unwrap().retain(|d| d["codec"] != "hevc");
    let d = download(tv, &big, &info);
    assert_eq!(made(&d), (VideoMode::Transcode, Some(2160), Some(29_808)));
    // Copied only when it plays as it is: the 4K stick decodes profile 8,
    // not 7, whose base layer a stream would show but a file keeps.
    let d = download(fire_tv_4k_max(), &small, &source("P8.1"));
    assert_eq!(outcome(&d), "D");
    let d = download(fire_tv_4k_max(), &small, &source("P7"));
    assert_eq!(made(&d).0, VideoMode::Transcode);
    assert_eq!(outcome(&play(fire_tv_4k_max(), &source("P7"))), "R", "streamed, it is copied");
}

#[test]
fn the_picked_quality_still_limits_version_2_players() {
    let mut req = request(fire_tv_4k_max());
    req.max_bitrate = Some(8_000);
    let d = play_with(req, "mkv", &source("HDR10"));
    assert_eq!(outcome(&d), "T");
    assert_eq!(d.reasons, ["Lower quality picked."]);
}

// ------------------------------------------------------------------ HDR transcodes

/// Google TV Streamer on an HDR10 monitor: HEVC Main 10 to 4K60 in
/// hardware, HDR10 only.
fn google_tv_streamer() -> Value {
    fixture("google_tv_streamer_v2")
}

/// A request through a transcoder that keeps HDR.
fn with_hdr(caps: Value) -> PlaybackRequest {
    let mut req = request(caps);
    req.hdr_transcodes = true;
    req
}

/// What the video is converted to: "HDR10" or "HLG" (HEVC), "SDR" (H.264);
/// "D" and "R" when it isn't.
fn converted_to(d: &Decision) -> &'static str {
    match d.video.as_ref() {
        Some(v) if v.mode == VideoMode::Transcode => match (v.codec, v.dynamic_range) {
            (VideoCodec::Hevc, OutputRange::Hdr10) => "HDR10",
            (VideoCodec::Hevc, OutputRange::Hlg) => "HLG",
            (VideoCodec::H264, OutputRange::Sdr) => "SDR",
            _ => "?",
        },
        _ => outcome(d),
    }
}

#[test]
fn transcodes_keep_hdr_for_screens_that_show_it() {
    let rows = ["P5", "P7", "P8.1", "P8.4", "HDR10", "HDR10+", "HLG", "SDR"];
    let mut hlg_only = fire_tv_lite();
    hlg_only["hdr_types"] = json!(["hlg"]);
    #[rustfmt::skip]
    let table: [(&str, Value, [&str; 8]); 5] = [
        //                                         P5       P7       8.1      8.4      HDR10    HDR10+   HLG      SDR
        ("Google TV Streamer", google_tv_streamer(), ["HDR10", "HDR10", "HDR10", "HDR10", "HDR10", "HDR10", "HDR10", "SDR"]),
        ("Fire TV Stick Lite", fire_tv_lite(),       ["HDR10", "HDR10", "HDR10", "HLG",   "HDR10", "HDR10", "HLG",   "SDR"]),
        ("An HLG screen",      hlg_only,             ["HLG",   "HLG",   "HLG",   "HLG",   "HLG",   "HLG",   "HLG",   "SDR"]),
        // No HEVC Main 10 decoder, no HDR screen; the web app.
        ("SDR phone",          sdr_phone(),          ["SDR",   "SDR",   "SDR",   "SDR",   "SDR",   "SDR",   "SDR",   "SDR"]),
        ("Chrome",             chrome(),             ["SDR",   "SDR",   "SDR",   "SDR",   "SDR",   "SDR",   "SDR",   "SDR"]),
    ];
    for (device, caps, expected) in table {
        for (row, want) in rows.iter().zip(expected) {
            // A lower quality than the file's: everything is converted.
            let mut req = with_hdr(caps.clone());
            req.max_bitrate = Some(8_000);
            let d = play_with(req, "mkv", &source(row));
            assert_eq!(converted_to(&d), want, "{device}, {row}: {:?}", d.reasons);
        }
    }
    // A transcoder that can't: SDR, as before.
    let mut req = request(google_tv_streamer());
    req.max_bitrate = Some(8_000);
    assert_eq!(converted_to(&play_with(req, "mkv", &source("HDR10"))), "SDR");
}

#[test]
fn what_hdr_becomes_is_said() {
    let reasons = |req: PlaybackRequest, row: &str| play_with(req, "mkv", &source(row)).reasons;
    assert_eq!(reasons(with_hdr(google_tv_streamer()), "P5"), ["Dolby Vision P5 converted to HDR10."]);
    assert_eq!(reasons(with_hdr(google_tv_streamer()), "HLG"), ["HLG converted to HDR10."]);
    assert_eq!(reasons(with_hdr(google_tv_streamer()), "P8.4"), ["Dolby Vision P8.4 converted to HDR10."]);
    assert_eq!(reasons(request(google_tv_streamer()), "P5"), ["Dolby Vision P5 converted to SDR."]);
    // Picture subtitles drawn onto the video: SDR (the Streamer shows PGS
    // itself, but not on converted video).
    let mut req = with_hdr(google_tv_streamer());
    req.subtitle_index = Some(9);
    let d = play_with(req, "mkv", &source("P5"));
    assert_eq!((converted_to(&d), d.burn_index), ("SDR", Some(9)));
    assert_eq!(d.reasons, ["PGS subtitles burned in.", "Dolby Vision P5 converted to SDR."]);
}

#[test]
fn hdr_transcodes_are_bound_by_the_hevc_main_10_decoders() {
    // A 4K film at "4K · 30 Mbps" on the Streamer: Dolby Vision 8.1
    // over the cap, made HEVC Main 10 HDR10 in 4K.
    let mut info = source("P8.1");
    (info.streams[0].width, info.streams[0].height, info.streams[0].level) = (Some(3840), Some(2160), Some(153));
    let mut remux = file("mkv");
    remux.bitrate = Some(63_000_000);
    let mut req = with_hdr(google_tv_streamer());
    (req.max_height, req.max_bitrate) = (Some(2160), Some(30_000));
    let d = decide(&remux, Some(&info), &info.streams, &req);
    let v = d.video.as_ref().unwrap();
    assert_eq!((converted_to(&d), v.max_height, v.max_bitrate_kbps), ("HDR10", Some(2160), Some(29_808)));
    assert_eq!(d.reasons, ["Lower quality picked."]);

    // A 1080p HEVC Main 10 decoder: 1080p, also for the original.
    let mut hdr10 = source("HDR10");
    (hdr10.streams[0].width, hdr10.streams[0].height) = (Some(3840), Some(2160));
    let d = decide(&remux, Some(&hdr10), &hdr10.streams, &with_hdr(fire_tv_lite()));
    let v = d.video.as_ref().unwrap();
    assert_eq!((converted_to(&d), d.quality, v.max_height), ("HDR10", Some("20m"), Some(1080)));

    // Main 10 only in software, or not at the video's frame rate: SDR.
    let mut caps = google_tv_streamer();
    caps["video"][1]["hardware"] = json!(false);
    let d = decide(&remux, Some(&info), &info.streams, &{
        let mut req = with_hdr(caps);
        req.max_bitrate = Some(30_000);
        req
    });
    assert_eq!(converted_to(&d), "SDR");
    let mut caps = google_tv_streamer();
    caps["video"][1]["max_fps"] = json!(30.0);
    info.streams[0].frame_rate = Some(60.0);
    let mut req = with_hdr(caps);
    req.max_bitrate = Some(30_000);
    assert_eq!(converted_to(&decide(&remux, Some(&info), &info.streams, &req)), "SDR");
}

// ------------------------------------------------------------------ audio

#[test]
fn audio_passed_through_plays_as_it_is() {
    let truehd = media(source("HDR10").streams[0].clone(), vec![audio(1, "truehd", 8, true)]);
    // The 4K Max passes TrueHD to the receiver; the Lite doesn't.
    assert_eq!(play(fire_tv_4k_max(), &truehd).method, Method::Direct);
    let d = play(fire_tv_lite(), &truehd);
    assert_eq!(outcome(&d), "R");
    assert_eq!(d.reasons, ["TrueHD audio not supported."]);
    // DTS goes to the Lite's receiver.
    let dts = media(source("HDR10").streams[0].clone(), vec![audio(1, "dts", 6, true)]);
    assert_eq!(play(fire_tv_lite(), &dts).method, Method::Direct);
}

#[test]
fn surround_audio_becomes_e_ac_3_5_1_for_surround_outputs() {
    let truehd = |channels| media(source("HDR10").streams[0].clone(), vec![audio(1, "truehd", channels, true)]);
    let converted = |caps: Value, info: &MediaInfo| {
        let d = play(caps, info);
        let a = d.audio.expect("audio");
        assert_eq!((d.method, a.mode), (Method::Remux, AudioMode::Transcode));
        (a.codec, a.channels)
    };
    // A TV passing E-AC-3 to a 7.1 receiver, and one with a 5.1 system.
    assert_eq!(converted(fire_tv_lite(), &truehd(8)), (AudioCodec::Eac3, Some(6)));
    assert_eq!(converted(chromecast_hd(), &truehd(8)), (AudioCodec::Eac3, Some(6)));
    // AC-3 only, then neither.
    let mut caps = chromecast_hd();
    caps["audio_passthrough"] = json!(["ac3"]);
    assert_eq!(converted(caps.clone(), &truehd(8)), (AudioCodec::Ac3, Some(6)));
    caps["audio_passthrough"] = json!([]);
    assert_eq!(converted(caps, &truehd(8)), (AudioCodec::Aac, Some(6)));
    // A stereo output, a stereo source: stereo AAC.
    assert_eq!(converted(hdr10_phone(), &truehd(8)), (AudioCodec::Aac, Some(2)));
    let mut flac = truehd(2);
    flac.streams[1].codec = "flac".into();
    let mut caps = fire_tv_lite();
    caps["audio_decode"] = json!(["aac"]);
    assert_eq!(converted(caps, &flac).0, AudioCodec::Aac);
}

#[test]
fn ac_3_family_audio_is_copied_into_hls() {
    // Profile 7 is repackaged; its AC-3 track goes along as it is.
    let mut p7 = source("P7");
    p7.streams[1].codec = "ac3".into();
    let d = play(fire_tv_lite(), &p7);
    assert_eq!((outcome(&d), d.audio.unwrap().mode), ("R", AudioMode::Copy));
    let d = play(fire_tv_lite(), &source("P7"));
    assert_eq!(d.audio.unwrap().mode, AudioMode::Copy, "E-AC-3 too");
    // Only what the player takes: the phone decodes AC-3 but not DTS.
    let mut dts = source("P7");
    dts.streams[1].codec = "dts".into();
    let a = play(hdr10_phone(), &dts).audio.unwrap();
    assert_eq!((a.mode, a.codec, a.channels), (AudioMode::Transcode, AudioCodec::Aac, Some(2)));
}

#[test]
fn players_that_switch_audio_tracks_play_others_as_they_are() {
    let info = media(source("HDR10").streams[0].clone(), vec![audio(1, "eac3", 6, true), audio(2, "ac3", 6, false)]);
    let mut req = request(fire_tv_lite());
    req.audio_index = Some(2);
    let d = play_with(req, "mkv", &info);
    assert_eq!((d.method, d.audio_index), (Method::Direct, Some(2)));
    // Without the switch, an HLS stream with that track.
    let mut caps = fire_tv_lite();
    caps["audio_track_switching"] = json!(false);
    let mut req = request(caps);
    req.audio_index = Some(2);
    let d = play_with(req, "mkv", &info);
    assert_eq!((outcome(&d), d.audio.unwrap().stream_index), ("R", Some(2)));
    assert_eq!(d.reasons, ["Audio track switching not supported."]);
}

// ------------------------------------------------------------------ subtitles

#[test]
fn picture_subtitles_the_player_shows_are_drawn_only_when_converting() {
    let mut req = request(shield());
    req.subtitle_index = Some(9);
    let d = play_with(req, "mkv", &source("HDR10"));
    assert_eq!((d.method, d.burn_index), (Method::Direct, None), "the Shield shows PGS itself");

    // Repackaged for the Lite, which also lists PGS: HLS carries no
    // subtitles, so they are drawn, which converts the video.
    let mut caps = fire_tv_lite();
    caps["containers"] = json!(["mp4"]);
    let mut req = request(caps);
    req.subtitle_index = Some(9);
    let d = play_with(req, "mkv", &source("HDR10"));
    assert_eq!((outcome(&d), d.burn_index), ("T", Some(9)));
    assert_eq!(d.reasons, ["PGS subtitles burned in.", "MKV not supported."]);
    // A player that doesn't list them: drawn, as for version 1.
    let mut caps = fire_tv_lite();
    caps["subtitle_codecs"] = json!(["subrip"]);
    let mut req = request(caps);
    req.subtitle_index = Some(9);
    assert_eq!(play_with(req, "mkv", &source("HDR10")).burn_index, Some(9));
}

// ------------------------------------------------------------------ the wire

#[test]
fn the_android_apps_reports_are_read() {
    for name in
        ["fire_tv_stick_4k_max_2023_v2", "fire_tv_stick_lite_v2", "google_tv_streamer_v2", "galaxy_note20_ultra_v2"]
    {
        let caps = request(fixture(name)).caps;
        assert!(caps.is_v2(), "{name}");
        assert!(caps.video.as_ref().is_some_and(|v| !v.is_empty()), "{name}");
        assert!(caps.audio_channels.is_some() && caps.hls.is_some() && caps.display.is_some(), "{name}");
        assert!(caps.device.as_ref().is_some_and(|d| ["low", "mid", "high"].contains(&d.tier.as_deref().unwrap())));
    }
    let max = request(fire_tv_4k_max()).caps;
    let profiles: Vec<i64> = max.dolby_vision_profiles.unwrap().iter().map(|p| p.profile).collect();
    assert_eq!(profiles, [5, 8, 9, 10]);
    assert_eq!(max.device.unwrap().model.as_deref(), Some("AFTKRT"));
    assert!(!request(fixture("fire_tv_stick_lite_v1")).caps.is_v2(), "version 1 only");

    // Values a later app may send are taken, not refused.
    let mut caps = fire_tv_lite();
    caps["hdr_types"] = json!(["hdr10", "unknown", "hdr_vivid"]);
    caps["device"]["form"] = json!("car");
    caps["future_field"] = json!({"a": 1});
    assert_eq!(outcome(&play(caps, &source("HDR10"))), "D");
}

// ------------------------------------------------------------------ version 1

#[test]
fn version_1_players_decide_as_before() {
    // Chrome: HDR shown, HEVC, Matroska, no Dolby Vision, no AC-3 family.
    let chrome_outcomes: Vec<&str> = ["P5", "P7", "P8.1", "P8.4", "P10", "HDR10", "HDR10+", "HLG", "SDR"]
        .iter()
        .map(|row| {
            let d = play(chrome(), &source(row));
            // Converted audio stays stereo AAC for browsers.
            let a = d.audio.as_ref().unwrap();
            assert_eq!((a.codec, a.channels), (AudioCodec::Aac, Some(2)), "{row}");
            outcome(&d)
        })
        .collect();
    // Profiles 7, 8 and 10 play their base layer, profile 5 is converted,
    // VP9 and AV1 can't be copied into MPEG-TS.
    assert_eq!(chrome_outcomes, ["T", "R", "R", "R", "T", "R", "R", "R", "R"]);
    let d = play(chrome(), &source("P5"));
    assert_eq!(d.reasons, ["Dolby Vision P5 converted to SDR.", "E-AC3 audio not supported."]);

    // The Android app's version 1 report: profile 7 plays directly with
    // `hdr`, as before version 2.
    assert_eq!(play(fixture("fire_tv_stick_lite_v1"), &source("P7")).method, Method::Direct);

    // Jellyfin's apps' HLS streams: H.264 and AAC, everything else converted
    // to stereo AAC.
    let mut req = request(jellyfin_hls());
    req.mode = pley::playback::PlaybackMode::Remux;
    let mut h264 = source("SDR");
    (h264.streams[0].codec, h264.streams[0].profile) = ("h264".into(), Some("High".into()));
    let d = play_with(req.clone(), "mkv", &h264);
    let a = d.audio.as_ref().unwrap();
    assert_eq!((outcome(&d), a.codec, a.channels), ("R", AudioCodec::Aac, Some(2)));
    let d = play_with(req, "mkv", &source("HDR10"));
    assert_eq!(outcome(&d), "T");

    // A version 1 report never reads version 2 fields.
    let mut caps = chrome();
    caps["audio_track_switching"] = json!(true);
    caps["audio_decode"] = json!(["eac3"]);
    let d = play(caps, &source("HDR10"));
    assert_eq!(d.reasons, ["E-AC3 audio not supported."]);
}

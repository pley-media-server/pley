//! What an LG TV plays, reported to the server (capabilities version 2) so
//! it sends files as they are whenever the TV's pipeline takes them: the port
//! of the LG web app's capability report. From LG's format
//! pages per webOS version, the TV's own HDR and sound flags, and what other
//! players found on real sets. Pure: the same facts give the same report, byte
//! for byte the web app's (its snapshots are this crate's golden tests).

use pley_base::Tier;
use pley_core::device::{CapsOverrides, CapsReport};
use pley_core::model::{ClientCaps, DeviceCaps, DisplayCaps, DolbyVisionDecoder, DynamicRange, HlsCaps, VideoDecoder};

use crate::facts::WebosFacts;

/// Containers the pipeline opens from an address, by the server's names.
const CONTAINERS: [&str; 18] = [
    "mkv", "mp4", "m4v", "mov", "ts", "m2ts", "mpegts", "avi", "mpg", "mpeg", "mp3", "m4a", "aac", "flac", "ogg",
    "oga", "wav", "mka",
];

/// Subtitles the app draws itself: text as WebVTT from the server, pictures from their `.mks`.
const SUBTITLES: [&str; 8] =
    ["subrip", "ass", "ssa", "webvtt", "mov_text", "hdmv_pgs_subtitle", "dvd_subtitle", "dvb_subtitle"];

// ffprobe's levels: H.264 5.1 = 51; HEVC 4.1 = 123, 5.1 = 153, 6.1 = 183.
const H264_FHD: i64 = 42;
const H264_UHD: i64 = 51;
const HEVC_FHD: i64 = 123;
const HEVC_UHD: i64 = 153;
const HEVC_8K: i64 = 183;

/// The report for these facts and settings, with Diagnostics' lines and the
/// containers the TV shows Dolby Vision from (others play their base layer,
/// so a remux keeps it).
pub fn webos_caps(f: &WebosFacts, o: &CapsOverrides) -> CapsReport {
    let v = f.webos;
    let uhd = f.uhd;
    let h264 = ["Constrained Baseline", "Baseline", "Main", "High"];
    let mut video = Vec::new();
    if uhd {
        // 4K H.264 at 60 only on webOS 4.0 sets; later ones promise 30 (some 60).
        video.push(decoder("h264", Some(&h264), (3840, 2160), if v == 4 { 60.0 } else { 30.0 }, 8, Some(H264_UHD)));
    }
    video.push(decoder("h264", Some(&h264), (1920, 1080), 60.0, 8, Some(H264_FHD)));
    let hevc = ["Main", "Main 10"];
    video.push(if f.uhd8k {
        decoder("hevc", Some(&hevc), (7680, 4320), 60.0, 10, Some(HEVC_8K))
    } else if uhd {
        decoder("hevc", Some(&hevc), (4096, 2160), 60.0, 10, Some(HEVC_UHD))
    } else {
        decoder("hevc", Some(&hevc), (1920, 1080), 60.0, 10, Some(HEVC_FHD))
    });
    if v >= 5 {
        let big = if f.uhd8k {
            (7680, 4320)
        } else if uhd {
            (3840, 2160)
        } else {
            (1920, 1080)
        };
        video.push(decoder("vp9", Some(&["Profile 0", "Profile 2"]), big, 60.0, 10, None));
        video.push(decoder("av1", Some(&["Main"]), big, 60.0, 10, None));
    } else {
        // webOS 4: VP9 from Matroska only, with no size promised.
        video.push(decoder("vp9", Some(&["Profile 0"]), (1920, 1080), 60.0, 8, None));
    }
    video.push(decoder("mpeg2video", None, (1920, 1080), 60.0, 8, None));
    video.push(decoder("mpeg4", None, (1920, 1080), 30.0, 8, None));

    let mut hdr_types = Vec::new();
    if f.hdr10 {
        hdr_types.extend([DynamicRange::Hdr10, DynamicRange::Hlg]);
    }
    let dolby_vision = f.dolby_vision && v >= 4;
    if dolby_vision {
        hdr_types.push(DynamicRange::DolbyVision);
    }
    let dv_profiles = if dolby_vision {
        vec![DolbyVisionDecoder { profile: 5, max_level: None }, DolbyVisionDecoder { profile: 8, max_level: None }]
    } else {
        Vec::new()
    };

    // Sound: what the TV decodes, and what goes to a receiver as it is.
    let mut decode = strings(&["aac", "ac3", "eac3", "mp3", "mp2", "flac", "vorbis", "pcm_s16le", "pcm_s24le"]);
    if v >= 4 {
        decode.push("opus".into());
    }
    // DTS: decoded by 2018–2019 sets; dropped in 2020, back on some 2023–2024 sets, dropped again in 2025.
    let dts_decoded = v == 4;
    if dts_decoded {
        decode.push("dts".into());
    }
    let output = f.sound_output.as_deref().unwrap_or("");
    let to_receiver = (output.starts_with("external_arc") || output.starts_with("external_optical"))
        && f.digital_output.as_deref() != Some("pcm");
    let mut passthrough: Vec<String> = Vec::new();
    if to_receiver {
        passthrough.extend(strings(&["ac3", "eac3"]));
        let edid = f.edid.as_deref().unwrap_or("").to_lowercase();
        if o.dts() || (!dts_decoded && edid.contains("dts") && (v == 23 || v == 24)) {
            passthrough.push("dts".into());
        }
        // TrueHD from an app isn't known to reach a receiver on any LG: only when asked for.
        if o.truehd() {
            passthrough.push("truehd".into());
        }
    } else if o.dts() && !dts_decoded {
        passthrough.push("dts".into());
    }
    let channels = if to_receiver || f.atmos {
        8
    } else if output.starts_with("tv_") {
        2
    } else {
        6
    };

    let caps = ClientCaps {
        containers: strings(&CONTAINERS),
        video_codecs: unique(video.iter().map(|d| d.codec.clone())),
        audio_codecs: unique(decode.iter().chain(&passthrough).cloned()),
        hdr: f.hdr10,
        dolby_vision: !dv_profiles.is_empty(),
        caps_version: Some(2),
        video,
        hdr_types,
        dolby_vision_profiles: dv_profiles,
        audio_decode: decode,
        audio_passthrough: unique(passthrough.into_iter()),
        audio_channels: channels,
        subtitle_codecs: strings(&SUBTITLES),
        audio_track_switching: v >= 4,
        picture_subtitles_sideload: true,
        // One size per stream (no `variants`): LG's pipeline may go black moving
        // between sizes, as between streams (docs/QUALITY.md).
        hls: HlsCaps {
            master_playlist: true,
            segments: if v >= 5 { strings(&["ts", "fmp4"]) } else { strings(&["ts"]) },
            variants: None,
        },
        display: f.screen.map(|(width, height)| DisplayCaps {
            width: width as i64,
            height: height as i64,
            refresh_rates: vec![50.0, 60.0],
        }),
        device: Some(DeviceCaps {
            form: "tv".into(),
            tier: Some(tier_of(f).as_str().into()),
            fire_tv: false,
            model: Some(f.model.clone()),
        }),
    };
    let dv_containers = if v >= 25 {
        strings(&["mp4", "m4v", "mov", "ts", "m2ts", "mpegts", "mkv"])
    } else {
        strings(&["mp4", "m4v", "mov", "ts", "m2ts", "mpegts"])
    };
    let lines = lines(f, &caps, &dv_containers);
    CapsReport { caps, lines, dv_containers }
}

/// Low for 2018–2019 sets and small-memory ones; high from webOS 22 on.
pub fn tier_of(f: &WebosFacts) -> Tier {
    let memory: String =
        f.memory.as_deref().unwrap_or("").chars().filter(|c| c.is_ascii_digit() || *c == '.').collect();
    let gb = parse_float(&memory);
    if f.webos <= 4 || gb.is_some_and(|gb| gb > 0.0 && gb < 1.5) {
        Tier::Low
    } else if f.webos >= 22 {
        Tier::High
    } else {
        Tier::Mid
    }
}

/// JavaScript's `parseFloat` on digits and dots: the longest number at the start.
fn parse_float(s: &str) -> Option<f64> {
    let mut end = 0;
    let mut dot = false;
    for (i, c) in s.char_indices() {
        match c {
            '0'..='9' => end = i + 1,
            '.' if !dot => dot = true,
            _ => break,
        }
    }
    s[..end].parse().ok()
}

fn decoder(
    codec: &str,
    profiles: Option<&[&str]>,
    (width, height): (i64, i64),
    fps: f64,
    depth: i64,
    level: Option<i64>,
) -> VideoDecoder {
    VideoDecoder {
        codec: codec.into(),
        hardware: true,
        profiles: profiles.map(strings),
        max_width: Some(width),
        max_height: Some(height),
        max_fps: Some(fps),
        max_bit_depth: Some(depth),
        max_level: level,
    }
}

fn strings(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn unique(list: impl Iterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for s in list {
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

fn lines(f: &WebosFacts, caps: &ClientCaps, dv_containers: &[String]) -> Vec<String> {
    let mut out = vec![
        format!("TV: {}{}", f.model, f.firmware.as_ref().map_or(String::new(), |fw| format!(" (firmware {fw})"))),
        format!("webOS {}{}", f.webos, f.release.as_ref().map_or(String::new(), |r| format!(" · release {r}"))),
        format!(
            "Panel: {}{}",
            if f.uhd8k {
                "8K"
            } else if f.uhd {
                "4K"
            } else {
                "HD"
            },
            if f.oled { " OLED" } else { "" }
        ),
        format!(
            "HDR: {}",
            if caps.hdr_types.is_empty() {
                "none".to_string()
            } else {
                caps.hdr_types.iter().map(|t| hdr_name(*t)).collect::<Vec<_>>().join(", ")
            }
        ),
    ];
    if !caps.dolby_vision_profiles.is_empty() {
        let profiles: Vec<String> = caps.dolby_vision_profiles.iter().map(|p| p.profile.to_string()).collect();
        out.push(format!(
            "Dolby Vision: profiles {}, from {}",
            profiles.join(", "),
            dv_containers.join(", ").to_uppercase()
        ));
    }
    let video: Vec<String> = caps
        .video
        .iter()
        .map(|d| {
            format!(
                "{} {}×{}@{}",
                d.codec,
                d.max_width.unwrap_or(0),
                d.max_height.unwrap_or(0),
                d.max_fps.unwrap_or(0.0)
            )
        })
        .collect();
    out.push(format!("Video: {}", video.join(", ")));
    out.push(format!("Audio decoded: {}", caps.audio_decode.join(", ")));
    out.push(format!(
        "Sent to the receiver: {}",
        if caps.audio_passthrough.is_empty() { "nothing".to_string() } else { caps.audio_passthrough.join(", ") }
    ));
    out.push(format!(
        "Sound: {}{}{}",
        f.sound_output.as_deref().unwrap_or("unknown"),
        f.digital_output.as_ref().map_or(String::new(), |d| format!(" (digital {d})")),
        if f.atmos { " · Atmos" } else { "" }
    ));
    out.push(format!("Streams: HLS {}", caps.hls.segments.join(", ")));
    if let Some(table) = &f.codecs {
        out.push(table.line());
    }
    out
}

fn hdr_name(t: DynamicRange) -> &'static str {
    match t {
        DynamicRange::Hdr10 => "HDR10",
        DynamicRange::Hdr10Plus => "HDR10+",
        DynamicRange::Hlg => "HLG",
        _ => "Dolby Vision",
    }
}

#[cfg(test)]
mod tests {
    use pley_core::store::{PassthroughChoice, PassthroughFormat};

    use super::*;

    /// The web app's test set: a 2025 OLED with its speakers, changed by `over`.
    pub fn tv(over: impl FnOnce(&mut WebosFacts)) -> WebosFacts {
        let mut f = WebosFacts {
            webos: 25,
            release: None,
            build: None,
            model: "OLED65C5PUA".into(),
            firmware: Some("33.20.65".into()),
            screen: Some((3840, 2160)),
            uhd: true,
            uhd8k: false,
            oled: true,
            hdr10: true,
            dolby_vision: true,
            sound_output: Some("tv_speaker".into()),
            digital_output: Some("auto".into()),
            atmos: false,
            edid: None,
            memory: Some("3G".into()),
            codecs: None,
            clock_24h: true,
        };
        over(&mut f);
        f
    }

    fn caps(f: &WebosFacts) -> ClientCaps {
        webos_caps(f, &CapsOverrides::default()).caps
    }

    fn overrides(truehd: bool, dts: bool) -> CapsOverrides {
        let mut o = CapsOverrides::default();
        let on = |b| if b { PassthroughChoice::On } else { PassthroughChoice::Auto };
        o.passthrough.set(PassthroughFormat::TrueHd, on(truehd));
        o.passthrough.set(PassthroughFormat::Dts, on(dts));
        o
    }

    #[test]
    fn shows_dolby_vision_only_from_mp4_and_ts_before_webos_25() {
        let dv = |webos| webos_caps(&tv(|f| f.webos = webos), &CapsOverrides::default()).dv_containers;
        assert!(!dv(24).contains(&"mkv".to_string()));
        assert!(dv(25).contains(&"mkv".to_string()));
    }

    #[test]
    fn claims_dolby_vision_only_on_sets_that_show_it() {
        let no = caps(&tv(|f| f.dolby_vision = false));
        assert!(!no.hdr_types.contains(&DynamicRange::DolbyVision));
        assert!(no.dolby_vision_profiles.is_empty());
        let yes = caps(&tv(|_| {}));
        assert_eq!(yes.dolby_vision_profiles.iter().map(|p| p.profile).collect::<Vec<_>>(), [5, 8]);
    }

    #[test]
    fn sends_dolby_to_a_receiver_only_when_the_digital_output_lets_it() {
        let pass = |out: &str, digital: &str| {
            caps(&tv(|f| {
                f.sound_output = Some(out.into());
                f.digital_output = Some(digital.into());
            }))
            .audio_passthrough
        };
        assert_eq!(pass("external_arc", "auto"), ["ac3", "eac3"]);
        assert!(pass("external_arc", "pcm").is_empty());
        assert!(pass("tv_speaker", "auto").is_empty());
    }

    #[test]
    fn sends_truehd_and_dts_only_when_settings_asks() {
        let arc = tv(|f| f.sound_output = Some("external_arc".into()));
        assert!(!caps(&arc).audio_passthrough.contains(&"truehd".to_string()));
        let on = webos_caps(&arc, &overrides(true, true)).caps.audio_passthrough;
        assert!(on.contains(&"truehd".to_string()) && on.contains(&"dts".to_string()));
    }

    #[test]
    fn decodes_dts_only_on_webos_4() {
        let decodes = |webos| caps(&tv(|f| f.webos = webos)).audio_decode.contains(&"dts".to_string());
        assert!(decodes(4));
        assert!(!decodes(5));
        assert!(!decodes(25));
    }

    #[test]
    fn offers_av1_and_vp9_profile_2_from_webos_5_and_fmp4_hls() {
        let four = caps(&tv(|f| f.webos = 4));
        assert!(!four.video_codecs.contains(&"av1".to_string()));
        assert_eq!(four.hls.segments, ["ts"]);
        let five = caps(&tv(|f| f.webos = 5));
        assert!(five.video_codecs.contains(&"av1".to_string()));
        assert_eq!(caps(&tv(|f| f.webos = 6)).hls.segments, ["ts", "fmp4"]);
        assert!(five.hls.variants.is_none());
    }

    #[test]
    fn rates_2018_2019_and_small_sets_low() {
        assert_eq!(tier_of(&tv(|f| f.webos = 4)), Tier::Low);
        assert_eq!(tier_of(&tv(|f| (f.webos, f.memory) = (6, Some("1G".into())))), Tier::Low);
        assert_eq!(tier_of(&tv(|f| (f.webos, f.memory) = (5, Some("2G".into())))), Tier::Mid);
        assert_eq!(tier_of(&tv(|f| f.webos = 23)), Tier::High);
        assert_eq!(tier_of(&tv(|f| (f.webos, f.memory) = (23, Some("1.25G".into())))), Tier::Low);
        assert_eq!(tier_of(&tv(|f| (f.webos, f.memory) = (6, None))), Tier::Mid);
        assert_eq!(parse_float("1.5.2"), Some(1.5));
        assert_eq!(parse_float("."), None);
    }

    #[test]
    fn diagnostics_lines_say_what_the_report_says() {
        let report = webos_caps(
            &tv(|f| {
                f.release = Some("10.2.0".into());
                f.sound_output = Some("external_arc".into());
                f.atmos = true;
            }),
            &CapsOverrides::default(),
        );
        assert_eq!(report.lines[0], "TV: OLED65C5PUA (firmware 33.20.65)");
        assert_eq!(report.lines[1], "webOS 25 · release 10.2.0");
        assert_eq!(report.lines[2], "Panel: 4K OLED");
        assert_eq!(report.lines[3], "HDR: HDR10, HLG, Dolby Vision");
        assert_eq!(report.lines[4], "Dolby Vision: profiles 5, 8, from MP4, M4V, MOV, TS, M2TS, MPEGTS, MKV");
        assert!(report.lines[5].starts_with("Video: h264 3840×2160@30, h264 1920×1080@60, hevc 4096×2160@60"));
        assert_eq!(report.lines[7], "Sent to the receiver: ac3, eac3");
        assert_eq!(report.lines[8], "Sound: external_arc (digital auto) · Atmos");
        assert_eq!(report.lines[9], "Streams: HLS ts, fmp4");
        assert_eq!(report.lines.len(), 10);
    }
}

//! The server's direct-play rules for version 2 reports, on the TV's side
//! (packages/tv/src/caps/policy.ts, the Android app's
//! `DefaultPlaybackCapsPolicy`): what the server will do with a file for this
//! TV's report. The player uses it to pick the version of a title that plays
//! best; Playback details uses it to say why. The server decides; this only
//! predicts.

use crate::model::{ClientCaps, DynamicRange, ItemDetail, MediaFile, MediaStream, StreamKind, VideoDecoder};

/// Whether a video plays as it is here, and if not, what the server does about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CapsDecision {
    Direct,
    /// Dolby Vision this TV can't decode, on an HDR10 screen: the server sends the HDR10 base layer.
    RemuxBaseLayer,
    /// The screen can't show its HDR: the server converts it.
    ServerToneMaps,
    /// Beyond this TV's decoders, and why.
    Unsupported(String),
}

/// Base-layer compatibility ids of Dolby Vision (`dv_bl_compat`).
const BL_HDR10: i64 = 1;
const BL_SDR: i64 = 2;
const BL_HLG: i64 = 4;
const BL_BLU_RAY: i64 = 6;
/// The frame rate's slack: 0.01 fps.
const RATE_SLACK: f64 = 0.01;

/// Profiles ffprobe and the TVs name the same way: missing from a decoder's
/// list, they refuse a stream (`KNOWN_PROFILES`, apps/server/src/playback.rs).
const KNOWN_PROFILES: [&str; 15] = [
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

/// The main video stream of a file: the default one, else the first.
pub fn video_of(file: &MediaFile) -> Option<&MediaStream> {
    file.streams_of(StreamKind::Video).find(|s| s.default).or_else(|| file.streams_of(StreamKind::Video).next())
}

/// The video's dynamic range, as the server reads it.
pub fn range_of(video: Option<&MediaStream>) -> DynamicRange {
    match video {
        None => DynamicRange::Sdr,
        Some(v) => match v.dynamic_range {
            Some(r) => r,
            None if v.dv_profile.is_some() => DynamicRange::DolbyVision,
            None => DynamicRange::Sdr,
        },
    }
}

/// The stream's bit depth, else from its pixel format (`yuv420p10le`) or profile (`Main 10`).
pub fn bit_depth(video: &MediaStream) -> i64 {
    if let Some(d) = video.bit_depth.filter(|d| *d > 0) {
        return d;
    }
    let f = video.pix_fmt.as_deref().unwrap_or("");
    let core = f.strip_suffix("le").or_else(|| f.strip_suffix("be"));
    if let Some(core) = core {
        if core.ends_with("10") {
            return 10;
        }
        if core.ends_with("12") {
            return 12;
        }
    }
    if video.profile.as_deref().unwrap_or("").contains("10") { 10 } else { 8 }
}

/// What the server will do with `file` for this report. Pure.
pub fn decide(file: &MediaFile, caps: &ClientCaps) -> CapsDecision {
    let Some(video) = video_of(file) else { return CapsDecision::Direct };
    let range = range_of(Some(video));
    match range {
        DynamicRange::DolbyVision => dolby_vision(video, caps),
        DynamicRange::Hdr10 | DynamicRange::Hdr10Plus | DynamicRange::Hlg => {
            let shown = match range {
                DynamicRange::Hlg => caps.shows(DynamicRange::Hlg),
                DynamicRange::Hdr10Plus => caps.shows(DynamicRange::Hdr10Plus) || caps.shows(DynamicRange::Hdr10),
                _ => caps.shows(DynamicRange::Hdr10),
            };
            if shown { fits(video, &video.codec, caps, true) } else { CapsDecision::ServerToneMaps }
        }
        _ => fits(video, &video.codec, caps, false),
    }
}

fn dolby_vision(video: &MediaStream, caps: &ClientCaps) -> CapsDecision {
    let profile = video.dv_profile;
    let decoded = caps.dolby_vision_profiles.iter().find(|p| Some(p.profile) == profile);
    if caps.shows(DynamicRange::DolbyVision)
        && let Some(decoded) = decoded
    {
        if let (Some(level), Some(max)) = (video.dv_level, decoded.max_level)
            && level > max
        {
            return CapsDecision::Unsupported(format!("Dolby Vision level {level} is above this TV's {max}"));
        }
        return CapsDecision::Direct;
    }
    let base = match profile {
        Some(9) => "h264",
        Some(10) => "av1",
        _ if video.codec == "dvhe" || video.codec == "dvh1" => "hevc",
        _ => video.codec.as_str(),
    };
    match profile {
        Some(5) => return CapsDecision::ServerToneMaps,
        Some(7) => {
            return if caps.shows(DynamicRange::Hdr10) {
                CapsDecision::RemuxBaseLayer
            } else {
                CapsDecision::ServerToneMaps
            };
        }
        _ => {}
    }
    // The Blu-ray's HDR10 id in profile 8 (a profile 7 converted elsewhere) is HDR10.
    let said =
        if profile == Some(8) && video.dv_bl_compat == Some(BL_BLU_RAY) { Some(BL_HDR10) } else { video.dv_bl_compat };
    let compat = said.or(if profile == Some(8) { Some(BL_HDR10) } else { None });
    match compat {
        Some(BL_HDR10) if caps.shows(DynamicRange::Hdr10) => fits(video, base, caps, true),
        Some(BL_HLG) if caps.shows(DynamicRange::Hlg) => fits(video, base, caps, true),
        Some(BL_SDR) => fits(video, base, caps, false),
        _ => CapsDecision::ServerToneMaps,
    }
}

fn fits(video: &MediaStream, codec: &str, caps: &ClientCaps, hdr: bool) -> CapsDecision {
    let entries: Vec<&VideoDecoder> = caps.video.iter().filter(|e| e.codec.eq_ignore_ascii_case(codec)).collect();
    if entries.is_empty() {
        return CapsDecision::Unsupported(format!("No decoder for {codec} here"));
    }
    let depth = bit_depth(video);
    if hdr && !entries.iter().any(|e| e.hardware && e.max_bit_depth.unwrap_or(8) >= 10) {
        return CapsDecision::Unsupported(format!("No 10-bit hardware decoder for {codec} here"));
    }
    if entries.iter().any(|e| takes(e, video, depth)) {
        return CapsDecision::Direct;
    }
    let size = [video.width, video.height].iter().flatten().map(i64::to_string).collect::<Vec<_>>().join("×");
    let fps = video.frame_rate.map_or(String::new(), |f| format!(" at {} fps", crate::format::fixed(f, 3)));
    let profile = video.profile.as_deref().map_or(String::new(), |p| format!(", {p}"));
    CapsDecision::Unsupported(format!("{codec} {size}{fps}, {depth}-bit{profile}, is beyond this TV's decoders"))
}

/// Whether decoder `e` takes `video` at bit depth `depth`: size either way
/// round, frame rate (0.01 slack), depth, level and listed profiles.
pub fn takes(e: &VideoDecoder, video: &MediaStream, depth: i64) -> bool {
    if let (Some(w), Some(h), Some(mw), Some(mh)) = (video.width, video.height, e.max_width, e.max_height) {
        let as_is = w <= mw && h <= mh;
        let turned = h <= mw && w <= mh;
        if !as_is && !turned {
            return false;
        }
    }
    if let (Some(fps), Some(max)) = (video.frame_rate, e.max_fps)
        && fps > max + RATE_SLACK
    {
        return false;
    }
    if e.max_bit_depth.is_some_and(|max| depth > max) {
        return false;
    }
    if let (Some(level), Some(max)) = (video.level.filter(|l| *l > 0), e.max_level)
        && level > max
    {
        return false;
    }
    let listed = e.profiles.as_deref().unwrap_or(&[]);
    if let Some(profile) = video.profile.as_deref().filter(|p| !p.is_empty())
        && !listed.is_empty()
        && !listed.iter().any(|p| p.eq_ignore_ascii_case(profile))
        && KNOWN_PROFILES.contains(&profile.to_lowercase().as_str())
    {
        return false;
    }
    let ten_bit_hevc = video.codec == "hevc" && depth >= 10;
    !(!listed.is_empty()
        && ten_bit_hevc
        && !listed.iter().any(|p| p.eq_ignore_ascii_case("main 10") || p.eq_ignore_ascii_case("rext")))
}

/// The container a file's extension names, as the TV's rules name it:
/// lowercase, `mts` as `m2ts` and `mpg` as `mpeg`.
pub fn container_of(extension: &str) -> String {
    match extension.to_ascii_lowercase().as_str() {
        "mts" => "m2ts".into(),
        "mpg" => "mpeg".into(),
        other => other.into(),
    }
}

/// What the screen gets of `video` played as it is from a `container`
/// file, by name, as the server words it for direct play
/// (`video_range_shown` with `Delivery::Direct`, apps/server/src/playback.rs):
/// "HDR10", "Dolby Vision P8.1", "HDR10 (this screen doesn't show HDR10+)",
/// "HDR10 (Dolby Vision base layer, as this screen doesn't show Dolby
/// Vision)", "Dolby Vision P5 not decoded (wrong colours)". With LG's rule
/// beside the server's: Dolby Vision shows only from the containers in
/// `dv_containers` (none: any), and from the others the TV plays its base
/// layer. `None` for SDR video.
pub fn direct_range_shown(
    video: &MediaStream,
    caps: &ClientCaps,
    container: &str,
    dv_containers: Option<&[String]>,
) -> Option<String> {
    let range = range_of(Some(video));
    if !matches!(range, DynamicRange::Hdr10 | DynamicRange::Hdr10Plus | DynamicRange::Hlg | DynamicRange::DolbyVision) {
        return None;
    }
    let shows_hdr10plus = caps.shows(DynamicRange::Hdr10Plus) || !caps.is_v2();
    if range != DynamicRange::DolbyVision {
        return crate::format::dynamic_range_label(Some(range), None, None, None, None, None).map(|label| {
            if range == DynamicRange::Hdr10Plus && !shows_hdr10plus {
                "HDR10 (this screen doesn't show HDR10+)".to_string()
            } else {
                label
            }
        });
    }
    let (profile, compat) = (video.dv_profile, video.dv_bl_compat);
    let container = container.to_ascii_lowercase().replacen("matroska", "mkv", 1);
    let container = container.split(',').next().unwrap_or("");
    let shown_from = dv_containers.is_none_or(|list| list.iter().any(|c| c == container));
    let decodes = decodes_dolby_vision(video, caps);
    if decodes && shown_from {
        return crate::format::dynamic_range_label(Some(range), profile, compat, None, video.dv_el.as_deref(), None);
    }
    // No base layer of its own: the TV shows it in the wrong colours.
    if profile == Some(5) || compat == Some(0) {
        let named = crate::format::dynamic_range_label(Some(range), profile, compat, None, None, None)?;
        return Some(format!("{named} not decoded (wrong colours)"));
    }
    let base = match compat {
        Some(BL_HLG) => "HLG",
        Some(BL_SDR) => "SDR",
        _ if video.hdr10plus == Some(true) && shows_hdr10plus => "HDR10+",
        _ => "HDR10",
    };
    let why = if decodes {
        format!("this TV doesn't show Dolby Vision from {}", container.to_uppercase())
    } else {
        base_layer_why(video, caps)
    };
    Some(format!("{base} (Dolby Vision base layer, as {why})"))
}

/// The TV decodes this Dolby Vision stream as Dolby Vision when it gets it
/// as it is: a version 2 report that lists the profile, at the stream's
/// level, with a Dolby Vision screen; a version 1 report that decodes Dolby
/// Vision (single-layer profiles only).
fn decodes_dolby_vision(video: &MediaStream, caps: &ClientCaps) -> bool {
    if !caps.is_v2() {
        return caps.dolby_vision && video.dv_profile != Some(7);
    }
    let profile = video.dv_profile.unwrap_or(0);
    caps.shows(DynamicRange::DolbyVision)
        && caps
            .dolby_vision_profiles
            .iter()
            .any(|d| d.profile == profile && video.dv_level.zip(d.max_level).is_none_or(|(level, max)| level <= max))
}

/// Why a Dolby Vision stream the TV doesn't decode shows as its base layer,
/// in the server's words: the screen, or the decoders.
fn base_layer_why(video: &MediaStream, caps: &ClientCaps) -> String {
    let profile = video.dv_profile.unwrap_or(0);
    if !caps.is_v2() {
        return if caps.dolby_vision {
            format!("this player doesn't decode profile {profile}")
        } else {
            "Dolby Vision isn't shown here".to_string()
        };
    }
    if !caps.shows(DynamicRange::DolbyVision) {
        return if caps.dolby_vision_profiles.is_empty() {
            "Dolby Vision isn't shown here".to_string()
        } else {
            "this screen doesn't show Dolby Vision".to_string()
        };
    }
    let decoder = caps.dolby_vision_profiles.iter().find(|d| d.profile == profile);
    match decoder.and_then(|d| video.dv_level.zip(d.max_level)) {
        Some((level, max)) if level > max => format!("level {level} is above this player's {max}"),
        _ => format!("this player doesn't decode profile {profile}"),
    }
}

fn rank(d: &CapsDecision) -> u8 {
    match d {
        CapsDecision::Direct => 0,
        CapsDecision::RemuxBaseLayer => 1,
        CapsDecision::ServerToneMaps => 2,
        CapsDecision::Unsupported(_) => 3,
    }
}

fn range_rank(r: DynamicRange) -> u8 {
    match r {
        DynamicRange::DolbyVision => 4,
        DynamicRange::Hdr10Plus => 3,
        DynamicRange::Hdr10 => 2,
        DynamicRange::Hlg => 1,
        _ => 0,
    }
}

/// The version of `item` that plays best here: one that plays as it is,
/// then one no bigger than the screen, then the tallest, then (among those
/// playing as they are) the richest dynamic range, then the highest bitrate.
pub fn best_file<'a>(item: &'a ItemDetail, caps: &ClientCaps) -> Option<&'a MediaFile> {
    let screen = caps.display.as_ref().map(|d| d.width.min(d.height));
    let height = |f: &MediaFile| video_of(f).and_then(|v| v.height).or(f.height).unwrap_or(0);
    let mut scored: Vec<(&MediaFile, CapsDecision)> = item.files.iter().map(|f| (f, decide(f, caps))).collect();
    scored.sort_by(|(a, da), (b, db)| {
        let over = |f: &MediaFile| screen.is_some_and(|s| height(f) > s) as u8;
        let rich = |f: &MediaFile, d: &CapsDecision| {
            if *d == CapsDecision::Direct { range_rank(range_of(video_of(f))) } else { 0 }
        };
        rank(da)
            .cmp(&rank(db))
            .then(over(a).cmp(&over(b)))
            .then(height(b).cmp(&height(a)))
            .then(rich(b, db).cmp(&rich(a, da)))
            .then(b.bitrate.unwrap_or(0).cmp(&a.bitrate.unwrap_or(0)))
    });
    scored.first().map(|(f, _)| *f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{CapsOverrides, lg_2022_caps};
    use crate::model::DolbyVisionDecoder;

    fn caps() -> ClientCaps {
        lg_2022_caps(&CapsOverrides::default()).caps
    }

    fn sdr_caps() -> ClientCaps {
        ClientCaps { hdr_types: vec![], dolby_vision_profiles: vec![], ..caps() }
    }

    fn file(f: impl FnOnce(&mut MediaStream)) -> MediaFile {
        let mut v = MediaStream {
            kind: StreamKind::Video,
            codec: "hevc".into(),
            width: Some(3840),
            height: Some(2160),
            frame_rate: Some(23.976),
            profile: Some("Main 10".into()),
            bit_depth: Some(10),
            level: Some(150),
            default: true,
            ..MediaStream::default()
        };
        f(&mut v);
        MediaFile { id: 1, height: v.height, streams: vec![v], ..MediaFile::default() }
    }

    fn dv(profile: i64, compat: i64) -> MediaFile {
        file(|v| {
            v.dynamic_range = Some(DynamicRange::DolbyVision);
            v.dv_profile = Some(profile);
            v.dv_bl_compat = Some(compat);
            v.dv_level = Some(6);
        })
    }

    #[test]
    fn hdr_plays_when_the_screen_shows_it_and_is_converted_when_not() {
        let hdr10 = file(|v| v.dynamic_range = Some(DynamicRange::Hdr10));
        assert_eq!(decide(&hdr10, &caps()), CapsDecision::Direct);
        assert_eq!(decide(&hdr10, &sdr_caps()), CapsDecision::ServerToneMaps);
        let plus = file(|v| v.dynamic_range = Some(DynamicRange::Hdr10Plus));
        assert_eq!(decide(&plus, &caps()), CapsDecision::Direct, "HDR10+ shows as HDR10");
        assert_eq!(decide(&file(|_| {}), &sdr_caps()), CapsDecision::Direct);
    }

    #[test]
    fn dolby_vision_by_profile() {
        assert_eq!(decide(&dv(8, 1), &caps()), CapsDecision::Direct);
        let low =
            ClientCaps { dolby_vision_profiles: vec![DolbyVisionDecoder { profile: 8, max_level: Some(5) }], ..caps() };
        assert_eq!(
            decide(&dv(8, 1), &low),
            CapsDecision::Unsupported("Dolby Vision level 6 is above this TV's 5".into())
        );
        let hdr10_only = ClientCaps { hdr_types: vec![DynamicRange::Hdr10], dolby_vision_profiles: vec![], ..caps() };
        assert_eq!(decide(&dv(7, 6), &hdr10_only), CapsDecision::RemuxBaseLayer);
        assert_eq!(decide(&dv(7, 6), &sdr_caps()), CapsDecision::ServerToneMaps);
        assert_eq!(decide(&dv(5, 0), &hdr10_only), CapsDecision::ServerToneMaps);
        // Profile 8 by its base layer: HDR10 (the Blu-ray id too), SDR, HLG.
        assert_eq!(decide(&dv(8, 6), &hdr10_only), CapsDecision::Direct);
        assert_eq!(decide(&dv(8, 2), &sdr_caps()), CapsDecision::Direct);
        assert_eq!(decide(&dv(8, 4), &hdr10_only), CapsDecision::ServerToneMaps);
        assert_eq!(decide(&dv(8, 1), &sdr_caps()), CapsDecision::ServerToneMaps);
    }

    #[test]
    fn what_the_screen_gets_of_a_file_played_as_it_is() {
        let shown = |f: &MediaFile, caps: &ClientCaps, container: &str, before_25: bool| {
            let list: Vec<String> = ["mp4", "m4v", "mov", "ts", "m2ts", "mpegts"].map(String::from).to_vec();
            direct_range_shown(video_of(f).unwrap(), caps, container, before_25.then_some(&list[..]))
        };
        let range = |r| file(move |v| v.dynamic_range = Some(r));
        assert_eq!(shown(&file(|_| {}), &caps(), "mkv", true), None);
        assert_eq!(shown(&range(DynamicRange::Hdr10), &caps(), "mkv", true).as_deref(), Some("HDR10"));
        assert_eq!(shown(&range(DynamicRange::Hlg), &caps(), "mkv", true).as_deref(), Some("HLG"));
        // LG shows no HDR10+.
        assert_eq!(
            shown(&range(DynamicRange::Hdr10Plus), &caps(), "mkv", true).as_deref(),
            Some("HDR10 (this screen doesn't show HDR10+)")
        );
        // Dolby Vision decoded from MP4 any time, from MKV from webOS 25.
        assert_eq!(shown(&dv(8, 1), &caps(), "mp4", true).as_deref(), Some("Dolby Vision P8.1"));
        assert_eq!(shown(&dv(8, 1), &caps(), "matroska,webm", false).as_deref(), Some("Dolby Vision P8.1"));
        assert_eq!(
            shown(&dv(8, 1), &caps(), "mkv", true).as_deref(),
            Some("HDR10 (Dolby Vision base layer, as this TV doesn't show Dolby Vision from MKV)")
        );
        assert_eq!(
            shown(&dv(5, 0), &caps(), "mkv", true).as_deref(),
            Some("Dolby Vision P5 not decoded (wrong colours)")
        );
        // Profile 7, which no LG decodes: its base layer.
        assert_eq!(
            shown(&dv(7, 6), &caps(), "mkv", false).as_deref(),
            Some("HDR10 (Dolby Vision base layer, as this player doesn't decode profile 7)")
        );
        assert_eq!(
            shown(&dv(8, 4), &sdr_caps(), "mp4", false).as_deref(),
            Some("HLG (Dolby Vision base layer, as Dolby Vision isn't shown here)")
        );
        assert_eq!(container_of("MTS"), "m2ts");
        assert_eq!(container_of("mpg"), "mpeg");
        assert_eq!(container_of("Mkv"), "mkv");
    }

    #[test]
    fn decoders_take_size_either_way_rate_with_slack_depth_and_level() {
        let e = &caps().video[2];
        assert_eq!(e.codec, "hevc");
        let v = |f: fn(&mut MediaStream)| video_of(&file(f)).cloned().unwrap();
        assert!(takes(e, &v(|_| {}), 10));
        assert!(takes(e, &v(|s| (s.width, s.height) = (Some(2160), Some(3840))), 10), "a turned picture fits");
        assert!(!takes(e, &v(|s| (s.width, s.height) = (Some(7680), Some(4320))), 10));
        assert!(takes(e, &v(|s| s.frame_rate = Some(60.005)), 10));
        assert!(!takes(e, &v(|s| s.frame_rate = Some(60.02)), 10));
        assert!(!takes(e, &v(|s| s.level = Some(183)), 10));
        assert!(!takes(e, &v(|_| {}), 12));
    }

    #[test]
    fn listed_profiles_refuse_only_names_they_know() {
        let h264 = &caps().video[1];
        let stream = |profile: &str| MediaStream {
            kind: StreamKind::Video,
            codec: "h264".into(),
            width: Some(1920),
            height: Some(1080),
            profile: Some(profile.into()),
            ..MediaStream::default()
        };
        assert!(takes(h264, &stream("High"), 8));
        assert!(!takes(h264, &stream("High 10"), 8));
        assert!(takes(h264, &stream("Something New"), 8));
        let main_only = VideoDecoder { profiles: Some(vec!["Main".into()]), ..caps().video[2].clone() };
        assert!(
            !takes(&main_only, &video_of(&file(|v| v.profile = None)).cloned().unwrap(), 10),
            "10-bit HEVC needs Main 10"
        );
    }

    #[test]
    fn beyond_the_decoders_says_why() {
        let big = file(|v| {
            (v.width, v.height) = (Some(7680), Some(4320));
            v.frame_rate = Some(60.0);
        });
        assert_eq!(
            decide(&big, &caps()),
            CapsDecision::Unsupported(
                "hevc 7680×4320 at 60.000 fps, 10-bit, Main 10, is beyond this TV's decoders".into()
            )
        );
        let vc1 = file(|v| v.codec = "vc1".into());
        assert_eq!(decide(&vc1, &caps()), CapsDecision::Unsupported("No decoder for vc1 here".into()));
    }

    #[test]
    fn the_best_version_plays_as_it_is_fits_the_screen_and_is_richest() {
        let mut uhd_dv = dv(8, 1);
        uhd_dv.id = 1;
        let mut uhd_sdr = file(|_| {});
        uhd_sdr.id = 2;
        let mut eight_k = file(|v| (v.width, v.height) = (Some(7680), Some(4320)));
        eight_k.id = 3;
        let mut hd = file(|v| (v.width, v.height) = (Some(1920), Some(1080)));
        hd.id = 4;
        let item = ItemDetail { files: vec![hd, eight_k, uhd_sdr, uhd_dv], ..ItemDetail::default() };
        assert_eq!(best_file(&item, &caps()).map(|f| f.id), Some(1));
        assert_eq!(best_file(&item, &sdr_caps()).map(|f| f.id), Some(2));
        assert_eq!(best_file(&ItemDetail::default(), &caps()), None);
    }

    #[test]
    fn depth_comes_from_the_pixel_format_or_profile_when_not_said() {
        let v = |pix: Option<&str>, profile: Option<&str>| MediaStream {
            pix_fmt: pix.map(Into::into),
            profile: profile.map(Into::into),
            ..MediaStream::default()
        };
        assert_eq!(bit_depth(&v(Some("yuv420p10le"), None)), 10);
        assert_eq!(bit_depth(&v(Some("yuv420p12be"), None)), 12);
        assert_eq!(bit_depth(&v(Some("yuv420p"), Some("Main 10"))), 10);
        assert_eq!(bit_depth(&v(None, None)), 8);
    }
}

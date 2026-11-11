//! Jellyfin's device profiles, read as Pley's players' capabilities
//! (docs/API.md, "Play request"), so that Pley's own rules decide for
//! Jellyfin's apps as they do for its own (docs/QUALITY.md).
//!
//! A profile's direct-play profiles say what a device plays as it is
//! ([`caps_of`]). Its first HLS transcoding profile and its codec profiles
//! say what its streams may carry: `PlaybackInfo` writes that into the
//! stream's address, in the names Jellyfin's server writes them
//! ([`stream_settings`]), and `master.m3u8` reads the address back as a
//! version 2 player's capabilities ([`stream_caps`]): the segments, the
//! codecs and their limits, the dynamic ranges shown and the audio. An app
//! that writes its own address (JellyCon) is read the same way.

use std::collections::BTreeSet;

use pley_transcoder_api::{DynamicRange, MediaStream};
use serde_json::Value;

use super::{Q, field};
use crate::playback::{Caps, DolbyVisionDecoder, HlsCaps, VideoDecoder};

/// Jellyfin's codec names as FFmpeg's, which Pley's capabilities use.
pub fn codec(name: &str) -> String {
    match name.trim().to_ascii_lowercase().as_str() {
        "h265" | "hevc" => "hevc".into(),
        "dca" | "dts" => "dts".into(),
        "mpeg2" => "mpeg2video".into(),
        other => other.to_owned(),
    }
}

/// Jellyfin's container names as Pley's (`mkv`, `mp4`, `webm`, …).
fn container(name: &str) -> Vec<String> {
    match name.trim().to_ascii_lowercase().as_str() {
        "matroska" | "mkv" => vec!["mkv".into()],
        "mp4" | "m4v" | "mov" => vec!["mp4".into(), "mov".into()],
        "mpegts" | "ts" | "m2ts" => vec!["ts".into(), "mpegts".into()],
        other => vec![other.to_owned()],
    }
}

/// What a device plays as it is, from the direct-play profiles of its
/// device profile. A device that sends none plays anything it is given, as
/// players built on mpv or ExoPlayer do (and say so by sending no limits).
pub fn caps_of(profile: Option<&Value>) -> Caps {
    let any = Caps {
        containers: ["mkv", "mp4", "mov", "webm", "ts", "mpegts", "avi", "m4a", "flac", "mp3", "ogg", "wav"]
            .map(String::from)
            .to_vec(),
        video_codecs: ["h264", "hevc", "av1", "vp9", "vp8", "mpeg2video", "mpeg4", "vc1"].map(String::from).to_vec(),
        audio_codecs: [
            "aac",
            "mp3",
            "ac3",
            "eac3",
            "dts",
            "truehd",
            "flac",
            "opus",
            "vorbis",
            "alac",
            "mp2",
            "pcm_s16le",
            "pcm_s24le",
        ]
        .map(String::from)
        .to_vec(),
        hdr: true,
        dolby_vision: true,
        ..Default::default()
    };
    let Some(profile) = profile else { return any };
    let direct: Vec<&Value> =
        field(profile, "DirectPlayProfiles").and_then(|v| v.as_array()).map(|a| a.iter().collect()).unwrap_or_default();
    if direct.is_empty() {
        return any;
    }
    let mut caps = Caps { hdr: true, ..Default::default() };
    for p in direct {
        let kind = text(field(p, "Type")).unwrap_or_else(|| "Video".into());
        if !kind.eq_ignore_ascii_case("video") && !kind.eq_ignore_ascii_case("audio") {
            continue;
        }
        let containers = names(field(p, "Container"));
        let videos = names(field(p, "VideoCodec"));
        let audios = names(field(p, "AudioCodec"));
        if containers.is_empty() {
            caps.containers.extend(any.containers.iter().cloned());
        }
        for c in containers {
            caps.containers.extend(container(&c));
        }
        if videos.is_empty() && kind.eq_ignore_ascii_case("video") {
            caps.video_codecs.extend(any.video_codecs.iter().cloned());
        }
        caps.video_codecs.extend(videos.iter().map(|c| codec(c)));
        if audios.is_empty() {
            caps.audio_codecs.extend(any.audio_codecs.iter().cloned());
        }
        caps.audio_codecs.extend(audios.iter().map(|c| codec(c)));
    }
    // Dolby Vision only where a profile names it (`DOVI…` video ranges).
    caps.dolby_vision = profile.to_string().contains("DOVI");
    for list in [&mut caps.containers, &mut caps.video_codecs, &mut caps.audio_codecs] {
        list.sort();
        list.dedup();
    }
    caps
}

/// A profile's list: `"a,b"`.
fn names(value: Option<&Value>) -> Vec<String> {
    text(value)
        .map(|s| s.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned).collect())
        .unwrap_or_default()
}

/// A profile's value, written as a string or as a number.
fn text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// `list` in its order, each once.
fn each_once(list: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in list {
        if !out.contains(&item) {
            out.push(item);
        }
    }
    out
}

// ------------------------------------------------------------------ streams

/// The video codecs of streams: H.264 and HEVC, copied or made, in either
/// kind of segment; VP9 and AV1 copied, into fMP4 segments.
const STREAM_VIDEO: &[&str] = &["h264", "hevc", "av1", "vp9"];

/// Jellyfin's dynamic ranges (`VideoRangeType`).
const RANGE_TYPES: &[&str] = &[
    "SDR",
    "HDR10",
    "HDR10Plus",
    "HLG",
    "DOVI",
    "DOVIWithHDR10",
    "DOVIWithHLG",
    "DOVIWithSDR",
    "DOVIWithEL",
    "DOVIWithHDR10Plus",
    "DOVIWithELHDR10Plus",
    "DOVIInvalid",
];

/// What a device that says nothing of dynamic ranges is shown: HDR as it
/// is, as Jellyfin's server copies it for such a device, and Dolby Vision's
/// base layers, but no Dolby Vision without one (`DOVI`), whose colors
/// only a Dolby Vision decoder gets right.
const RANGES_UNSAID: &[&str] = &["SDR", "HDR10", "HDR10Plus", "HLG"];

/// Jellyfin's name for a video's dynamic range (`VideoRangeType`).
pub fn range_type(s: &MediaStream) -> &'static str {
    let plus = s.hdr10plus == Some(true);
    match s.dynamic_range() {
        DynamicRange::Sdr => "SDR",
        DynamicRange::Hdr10 => "HDR10",
        DynamicRange::Hdr10plus => "HDR10Plus",
        DynamicRange::Hlg => "HLG",
        DynamicRange::DolbyVision => match (s.dv_profile, s.dv_bl_compat) {
            (Some(7), _) if plus => "DOVIWithELHDR10Plus",
            (Some(7), _) => "DOVIWithEL",
            (_, Some(1 | 6)) if plus => "DOVIWithHDR10Plus",
            (_, Some(1 | 6)) => "DOVIWithHDR10",
            (_, Some(2)) => "DOVIWithSDR",
            (_, Some(4)) => "DOVIWithHLG",
            _ => "DOVI",
        },
    }
}

/// A dynamic range by Jellyfin's name, in any case.
fn range_named(name: &str) -> Option<&'static str> {
    RANGE_TYPES.iter().copied().find(|r| r.eq_ignore_ascii_case(name.trim()))
}

/// Profiles as ffprobe names them ("Main 10"), which Pley's decoders are
/// matched by: Jellyfin's apps write them in lowercase, with or without
/// their spaces ("main 10", "main10").
const PROFILES: &[&str] = &[
    "Main",
    "Main 10",
    "Main Still Picture",
    "Rext",
    "Baseline",
    "Constrained Baseline",
    "High",
    "High 10",
    "High 4:2:2",
    "High 4:4:4 Predictive",
    "Profile 0",
    "Profile 1",
    "Profile 2",
    "Profile 3",
    "Professional",
];

fn ffprobe_profile(name: &str) -> String {
    let squeezed = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_ascii_lowercase();
    let wanted = squeezed(name);
    PROFILES.iter().find(|p| squeezed(p) == wanted).map_or_else(|| name.trim().to_owned(), |p| (*p).to_owned())
}

/// A condition of a codec profile.
struct Condition {
    /// `equals`, `notequals`, `equalsany`, `lessthanequal`, `greaterthanequal`.
    kind: String,
    /// `videoprofile`, `videolevel`, `width`…
    property: String,
    value: String,
    /// `IsRequired`, true unless it says otherwise: whether a video that
    /// doesn't say the property's value fails the condition.
    required: bool,
}

fn conditions(value: Option<&Value>) -> Vec<Condition> {
    let Some(list) = value.and_then(Value::as_array) else { return Vec::new() };
    list.iter()
        .filter_map(|c| {
            let required = match field(c, "IsRequired") {
                Some(Value::Bool(b)) => *b,
                Some(Value::String(s)) => !s.trim().eq_ignore_ascii_case("false"),
                _ => true,
            };
            Some(Condition {
                kind: text(field(c, "Condition"))?.to_ascii_lowercase(),
                property: text(field(c, "Property"))?.to_ascii_lowercase(),
                value: text(field(c, "Value")).unwrap_or_default(),
                required,
            })
        })
        .collect()
}

/// The bit depth of a video, as ffprobe gives it or as its pixel format
/// says ("yuv420p10le").
fn bit_depth(s: &MediaStream) -> i64 {
    s.bit_depth.unwrap_or_else(|| if s.pix_fmt.as_deref().is_some_and(|f| f.contains("10")) { 10 } else { 8 })
}

/// Whether the video `source` meets condition `c`, as Jellyfin's server
/// reads a codec profile's `ApplyConditions`: a property it doesn't read
/// (`IsInterlaced`, `RefFrames`…) is met, and so is one whose value the
/// video doesn't say, unless the condition is required. Otherwise a
/// profile's limits would be dropped for a condition Pley can't check.
fn holds(c: &Condition, source: Option<&MediaStream>) -> bool {
    let Some(s) = source else { return !c.required };
    let same = |a: &str, b: &str| a.replace(' ', "").eq_ignore_ascii_case(&b.replace(' ', ""));
    let word = |actual: Option<&str>| {
        let Some(actual) = actual else { return !c.required };
        let any = c.value.split('|').any(|v| same(v, actual));
        match c.kind.as_str() {
            "equals" | "equalsany" => any,
            "notequals" => !any,
            _ => false,
        }
    };
    let number = |actual: Option<f64>| {
        let Some(actual) = actual else { return !c.required };
        let Ok(value) = c.value.trim().parse::<f64>() else { return false };
        match c.kind.as_str() {
            "equals" => actual == value,
            "notequals" => actual != value,
            "lessthanequal" => actual <= value,
            "greaterthanequal" => actual >= value,
            _ => false,
        }
    };
    match c.property.as_str() {
        "videoprofile" => word(s.profile.as_deref()),
        "videorangetype" => word(Some(range_type(s))),
        "width" => number(s.width.map(|n| n as f64)),
        "height" => number(s.height.map(|n| n as f64)),
        "videolevel" => number(s.level.map(|n| n as f64)),
        "videobitdepth" => number(Some(bit_depth(s) as f64)),
        "videoframerate" => number(s.frame_rate),
        _ => true,
    }
}

/// What a device's codec profiles say of a codec in its streams.
#[derive(Debug, Default, Clone, PartialEq)]
struct Limits {
    /// A profile names none of its profiles ("none"): it doesn't decode it.
    refused: bool,
    profiles: Option<Vec<String>>,
    level: Option<i64>,
    bit_depth: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    ranges: Option<BTreeSet<&'static str>>,
}

impl Limits {
    fn apply(&mut self, c: &Condition) {
        let values = || c.value.split('|').map(str::trim).filter(|v| !v.is_empty());
        let at_most = matches!(c.kind.as_str(), "lessthanequal" | "equals");
        let number = || c.value.trim().parse::<f64>().ok().map(|n| n as i64).filter(|n| *n > 0);
        let lower = |was: Option<i64>| match (was, number()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        match c.property.as_str() {
            "videoprofile" if matches!(c.kind.as_str(), "equals" | "equalsany") => {
                let named: Vec<String> = values().map(ffprobe_profile).collect();
                if named.iter().any(|p| p.eq_ignore_ascii_case("none")) {
                    self.refused = true;
                } else {
                    self.profiles = Some(match self.profiles.take() {
                        Some(before) => before.into_iter().filter(|p| named.contains(p)).collect(),
                        None => named,
                    });
                }
            }
            "videolevel" if at_most => self.level = lower(self.level),
            "videobitdepth" if at_most => self.bit_depth = lower(self.bit_depth),
            "width" if at_most => self.width = lower(self.width),
            "height" if at_most => self.height = lower(self.height),
            "videorangetype" => {
                let named: BTreeSet<&'static str> = values().filter_map(range_named).collect();
                let allowed: BTreeSet<&'static str> = match c.kind.as_str() {
                    "equals" | "equalsany" => named,
                    "notequals" => RANGE_TYPES.iter().copied().filter(|r| !named.contains(r)).collect(),
                    _ => return,
                };
                self.ranges = Some(match self.ranges.take() {
                    Some(before) => before.intersection(&allowed).copied().collect(),
                    None => allowed,
                });
            }
            _ => {}
        }
    }
}

/// What the video codec profiles of `profile` say of `codec`: those for
/// it (or every codec) whose `ApplyConditions` the source meets.
fn limits_of(profile: Option<&Value>, codec_name: &str, source: Option<&MediaStream>) -> Limits {
    let mut limits = Limits::default();
    let profiles = profile.and_then(|p| field(p, "CodecProfiles")).and_then(Value::as_array);
    for p in profiles.into_iter().flatten() {
        if !text(field(p, "Type")).is_some_and(|t| t.eq_ignore_ascii_case("video")) {
            continue;
        }
        let codecs: Vec<String> = names(field(p, "Codec")).iter().map(|c| codec(c)).collect();
        if !codecs.is_empty() && !codecs.iter().any(|c| c == codec_name) {
            continue;
        }
        if !conditions(field(p, "ApplyConditions")).iter().all(|c| holds(c, source)) {
            continue;
        }
        for c in conditions(field(p, "Conditions")) {
            limits.apply(&c);
        }
    }
    limits
}

/// The most audio channels of the video codec profiles' `AudioChannels`.
fn video_audio_channels(profile: Option<&Value>) -> Option<i64> {
    let profiles = profile.and_then(|p| field(p, "CodecProfiles")).and_then(Value::as_array);
    profiles
        .into_iter()
        .flatten()
        .filter(|p| text(field(p, "Type")).is_some_and(|t| t.eq_ignore_ascii_case("videoaudio")))
        .flat_map(|p| conditions(field(p, "Conditions")))
        .filter(|c| c.property == "audiochannels" && matches!(c.kind.as_str(), "lessthanequal" | "equals"))
        .filter_map(|c| c.value.trim().parse::<i64>().ok())
        .filter(|n| *n > 0)
        .min()
}

/// The settings of a stream for a device, as `PlaybackInfo` writes them
/// into its address: from the device profile's first HLS transcoding
/// profile for video (its first for video of any protocol, Pley streaming
/// HLS alone), its codecs, its segments (`SegmentContainer`: `mp4` for
/// fMP4) and its audio channels, and per video codec what the codec
/// profiles say of it (`{codec}-profile`, `-level`, `-videobitdepth`,
/// `-maxwidth`, `-maxheight`, `-rangetype`). `source` is the file's video,
/// against which those profiles' `ApplyConditions` are read, as Jellyfin's
/// server reads them. A device without such a profile gets H.264 and AAC in
/// MPEG-TS.
pub fn stream_settings(profile: Option<&Value>, source: Option<&MediaStream>) -> Vec<(String, String)> {
    let transcoding = profile.and_then(|p| field(p, "TranscodingProfiles")).and_then(Value::as_array);
    let videos: Vec<&Value> = transcoding
        .into_iter()
        .flatten()
        .filter(|t| text(field(t, "Type")).is_some_and(|k| k.eq_ignore_ascii_case("video")))
        .collect();
    let hls = |t: &&&Value| text(field(t, "Protocol")).is_some_and(|p| p.eq_ignore_ascii_case("hls"));
    let Some(chosen) = videos.iter().find(hls).or(videos.first()) else {
        return [("VideoCodec", "h264"), ("AudioCodec", "aac"), ("SegmentContainer", "ts")]
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .to_vec();
    };
    let fmp4 = text(field(chosen, "Container"))
        .is_some_and(|c| matches!(c.trim().to_ascii_lowercase().as_str(), "mp4" | "fmp4" | "m4s"));
    let mut limits: Vec<(String, Limits)> = each_once(names(field(chosen, "VideoCodec")).iter().map(|c| codec(c)))
        .into_iter()
        .filter(|c| STREAM_VIDEO.contains(&c.as_str()))
        .map(|c| {
            let limits = limits_of(profile, &c, source);
            (c, limits)
        })
        .filter(|(_, l)| !l.refused)
        .collect();
    if limits.is_empty() {
        limits.push(("h264".into(), Limits::default()));
    }
    let audio = each_once(names(field(chosen, "AudioCodec")).iter().map(|c| codec(c)));
    let channels =
        [text(field(chosen, "MaxAudioChannels")).and_then(|n| n.trim().parse().ok()), video_audio_channels(profile)]
            .into_iter()
            .flatten()
            .filter(|n: &i64| *n > 0)
            .min();

    let mut out = vec![
        ("VideoCodec".to_owned(), limits.iter().map(|(c, _)| c.as_str()).collect::<Vec<_>>().join(",")),
        ("AudioCodec".to_owned(), if audio.is_empty() { "aac".to_owned() } else { audio.join(",") }),
        ("SegmentContainer".to_owned(), if fmp4 { "mp4" } else { "ts" }.to_owned()),
    ];
    if let Some(n) = channels {
        out.push(("TranscodingMaxAudioChannels".into(), n.to_string()));
    }
    for (c, l) in &limits {
        let mut put = |name: &str, value: String| out.push((format!("{c}-{name}"), value));
        if let Some(profiles) = l.profiles.as_ref().filter(|p| !p.is_empty()) {
            put("profile", profiles.iter().map(|p| p.to_ascii_lowercase()).collect::<Vec<_>>().join(","));
        }
        for (name, value) in
            [("level", l.level), ("videobitdepth", l.bit_depth), ("maxwidth", l.width), ("maxheight", l.height)]
        {
            if let Some(value) = value {
                put(name, value.to_string());
            }
        }
        if let Some(ranges) = &l.ranges {
            // Not even SDR: SDR all the same, which every device shows.
            let ranges =
                if ranges.is_empty() { "SDR".to_owned() } else { ranges.iter().copied().collect::<Vec<_>>().join(",") };
            put("rangetype", ranges);
        }
    }
    out
}

/// What a stream's address asks of it (written by [`stream_settings`], or
/// by an app itself), as a version 2 player's capabilities: a decoder for
/// each of `VideoCodec`'s (H.264 when it names none) with its limits, the
/// dynamic ranges shown (all but Dolby Vision without a base layer when
/// none are named), `AudioCodec`'s codecs decoded (AAC and MP3 when it
/// names none) on as many channels as `MaxAudioChannels`,
/// `TranscodingMaxAudioChannels` or `AudioChannels` say, and the segments
/// of `SegmentContainer` (the last one written). It reads master playlists,
/// which name what Pley's streams carry, and plays one size.
pub fn stream_caps(q: &Q) -> Caps {
    let mut videos: Vec<String> = each_once(q.list("VideoCodec").iter().map(|c| codec(c)))
        .into_iter()
        .filter(|c| STREAM_VIDEO.contains(&c.as_str()))
        .collect();
    if videos.is_empty() {
        videos.push("h264".into());
    }
    let depth = q.number("MaxVideoBitDepth").filter(|d| *d > 0);
    let positive = |name: String| q.number(&name).filter(|n| *n > 0);
    let decoders: Vec<VideoDecoder> = videos
        .iter()
        .map(|c| VideoDecoder {
            codec: c.clone(),
            // Jellyfin's apps list the codecs their devices decode, which
            // they mostly do in hardware: HDR may stay HDR in transcodes.
            hardware: true,
            profiles: q.list(&format!("{c}-profile")).iter().map(|p| ffprobe_profile(p)).collect(),
            max_width: positive(format!("{c}-maxwidth")),
            max_height: positive(format!("{c}-maxheight")),
            max_fps: None,
            max_bit_depth: match (positive(format!("{c}-videobitdepth")), depth) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            },
            max_level: positive(format!("{c}-level")),
        })
        .collect();

    let named: Vec<String> = videos.iter().flat_map(|c| q.list(&format!("{c}-rangetype"))).collect();
    let ranges: BTreeSet<&'static str> = if named.is_empty() {
        RANGES_UNSAID.iter().copied().collect()
    } else {
        named.iter().filter_map(|r| range_named(r)).collect()
    };
    let mut hdr_types: Vec<String> = [("HDR10", "hdr10"), ("HDR10Plus", "hdr10plus"), ("HLG", "hlg")]
        .into_iter()
        .filter(|(range, _)| ranges.contains(range))
        .map(|(_, name)| name.to_owned())
        .collect();
    // Dolby Vision without a base layer is shown only by a Dolby Vision
    // decoder: one that shows it shows the profiles with one too.
    let dolby_vision = ranges.contains("DOVI");
    let mut dv_profiles = Vec::new();
    if dolby_vision {
        hdr_types.push("dolby_vision".into());
        if videos.iter().any(|c| c == "hevc") {
            dv_profiles.extend([5, 8]);
            if ranges.contains("DOVIWithEL") || ranges.contains("DOVIWithELHDR10Plus") {
                dv_profiles.push(7);
            }
        }
        if videos.iter().any(|c| c == "av1") {
            dv_profiles.push(10);
        }
    }
    let dolby_vision_hdr10plus =
        dolby_vision && (ranges.contains("DOVIWithHDR10Plus") || ranges.contains("DOVIWithELHDR10Plus"));

    let mut audio = each_once(q.list("AudioCodec").iter().map(|c| codec(c)));
    if audio.is_empty() {
        audio = vec!["aac".into(), "mp3".into()];
    }
    let channels = ["MaxAudioChannels", "TranscodingMaxAudioChannels", "AudioChannels"]
        .into_iter()
        .filter_map(|name| q.number(name))
        .filter(|n| *n > 0)
        .min()
        .map(|n| n.clamp(1, 8) as u32);
    let fmp4 = q
        .list("SegmentContainer")
        .last()
        .is_some_and(|c| matches!(c.to_ascii_lowercase().as_str(), "mp4" | "fmp4" | "m4s"));

    Caps {
        containers: Vec::new(),
        video_codecs: videos,
        audio_codecs: audio.clone(),
        hdr: !hdr_types.is_empty(),
        dolby_vision,
        caps_version: Some(2),
        video: Some(decoders),
        hdr_types: Some(hdr_types),
        dolby_vision_profiles: (!dv_profiles.is_empty())
            .then(|| dv_profiles.into_iter().map(|profile| DolbyVisionDecoder { profile, max_level: None }).collect()),
        dolby_vision_hdr10plus,
        audio_decode: Some(audio),
        audio_channels: channels,
        hls: Some(HlsCaps {
            master_playlist: true,
            segments: vec![if fmp4 { "fmp4" } else { "ts" }.to_owned()],
            variants: false,
        }),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use pley_transcoder_api::{
        AudioCodec, AudioMode, DolbyVisionMode, MediaInfo, OutputRange, SegmentFormat, StreamKind, VideoCodec,
        VideoMode,
    };
    use serde_json::json;

    use super::*;
    use crate::catalog::FileRow;
    use crate::playback::{Controls, Decision, PlaybackMode, PlaybackRequest, decide};

    #[test]
    fn device_profiles_become_capabilities() {
        let profile = json!({
            "DirectPlayProfiles": [
                {"Container": "mkv,mp4", "VideoCodec": "h264,h265", "AudioCodec": "aac,ac3", "Type": "Video"},
                {"Container": "flac", "Type": "Audio"}
            ]
        });
        let caps = caps_of(Some(&profile));
        assert!(caps.containers.contains(&"mkv".to_owned()) && caps.containers.contains(&"mov".to_owned()));
        assert_eq!(caps.video_codecs, ["h264", "hevc"]);
        assert!(caps.audio_codecs.contains(&"ac3".to_owned()) && caps.audio_codecs.contains(&"flac".to_owned()));
        assert!(!caps.dolby_vision);

        let none = caps_of(None);
        assert!(none.video_codecs.contains(&"hevc".to_owned()) && none.dolby_vision, "no profile: plays anything");
        let empty = caps_of(Some(&json!({"DirectPlayProfiles": []})));
        assert!(empty.containers.contains(&"mkv".to_owned()), "no profiles: plays anything");
    }

    fn hevc(range: DynamicRange) -> MediaStream {
        MediaStream {
            index: 0,
            kind: StreamKind::Video,
            codec: "hevc".into(),
            profile: Some("Main 10".into()),
            level: Some(153),
            width: Some(3840),
            height: Some(2160),
            frame_rate: Some(23.976),
            bit_depth: Some(10),
            dynamic_range: Some(range),
            ..Default::default()
        }
    }

    #[test]
    fn conditions_pley_cant_check_are_met_unless_required() {
        let one = |c: Value| conditions(Some(&json!([c]))).pop().unwrap();
        let source = hevc(DynamicRange::Sdr);
        let interlaced = one(json!({"Condition": "Equals", "Property": "IsInterlaced", "Value": "false"}));
        assert!(holds(&interlaced, Some(&source)), "a property Pley doesn't read");
        let unsaid = MediaStream { frame_rate: None, ..source.clone() };
        let rate = json!({"Condition": "LessThanEqual", "Property": "VideoFramerate", "Value": "30"});
        assert!(!holds(&one(rate.clone()), Some(&unsaid)), "required unless it says otherwise");
        let mut optional = rate;
        optional["IsRequired"] = json!(false);
        assert!(holds(&one(optional), Some(&unsaid)));
        let level = one(json!({"Condition": "LessThanEqual", "Property": "VideoLevel", "Value": "120"}));
        assert!(!holds(&level, Some(&source)), "one Pley reads is checked");
    }

    /// A TV app's profile, as Jellyfin for Android TV writes one: MPEG-TS
    /// first, HEVC up to Main 10 at level 5.1 (5.0 for Main), no Dolby
    /// Vision without a base layer, 4K at most, sound on up to six
    /// channels; no VP8 at all.
    fn tv_profile() -> Value {
        json!({
            "TranscodingProfiles": [
                {"Type": "Audio", "Container": "ts", "Protocol": "hls", "AudioCodec": "aac"},
                {"Type": "Video", "Container": "ts", "Protocol": "hls", "VideoCodec": "hevc,h264,vp8",
                 "AudioCodec": "aac,ac3,eac3", "MaxAudioChannels": "8"},
                {"Type": "Video", "Container": "mp4", "Protocol": "hls", "VideoCodec": "hevc,h264", "AudioCodec": "aac"}
            ],
            "CodecProfiles": [
                {"Type": "Video", "Codec": "hevc", "Conditions": [
                    {"Condition": "EqualsAny", "Property": "VideoProfile", "Value": "main|main 10"}]},
                {"Type": "Video", "Codec": "hevc",
                 "Conditions": [{"Condition": "LessThanEqual", "Property": "VideoLevel", "Value": "150"}],
                 "ApplyConditions": [{"Condition": "Equals", "Property": "VideoProfile", "Value": "main"}]},
                {"Type": "Video", "Codec": "hevc",
                 "Conditions": [{"Condition": "LessThanEqual", "Property": "VideoLevel", "Value": "153"}],
                 "ApplyConditions": [{"Condition": "Equals", "Property": "VideoProfile", "Value": "main 10"}]},
                {"Type": "Video", "Codec": "hevc", "Conditions": [
                    {"Condition": "LessThanEqual", "Property": "Width", "Value": "3840"},
                    {"Condition": "LessThanEqual", "Property": "Height", "Value": "2160"}]},
                {"Type": "Video", "Codec": "hevc",
                 "Conditions": [
                    {"Condition": "NotEquals", "Property": "VideoRangeType", "Value": "DOVI|DOVIWithEL|DOVIInvalid"}],
                 "ApplyConditions": [
                    {"Condition": "EqualsAny", "Property": "VideoRangeType", "Value": "DOVI|DOVIWithEL|DOVIInvalid"}]},
                {"Type": "Video", "Codec": "vp8", "Conditions": [
                    {"Condition": "Equals", "Property": "VideoProfile", "Value": "none"}]},
                {"Type": "VideoAudio", "Conditions": [
                    {"Condition": "LessThanEqual", "Property": "AudioChannels", "Value": "6"}]}
            ]
        })
    }

    fn setting<'a>(settings: &'a [(String, String)], name: &str) -> Option<&'a str> {
        settings.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    #[test]
    fn a_profile_says_what_its_streams_carry() {
        let profile = tv_profile();
        let settings = stream_settings(Some(&profile), Some(&hevc(DynamicRange::Hdr10)));
        let get = |name| setting(&settings, name);
        assert_eq!(get("VideoCodec"), Some("hevc,h264"), "the first HLS video profile's; VP8 refused");
        assert_eq!((get("AudioCodec"), get("SegmentContainer")), (Some("aac,ac3,eac3"), Some("ts")));
        assert_eq!(get("TranscodingMaxAudioChannels"), Some("6"), "the fewest it says");
        assert_eq!(get("hevc-profile"), Some("main,main 10"));
        assert_eq!(get("hevc-level"), Some("153"), "the Main 10 source's limit");
        assert_eq!((get("hevc-maxwidth"), get("hevc-maxheight")), (Some("3840"), Some("2160")));
        assert_eq!(get("hevc-rangetype"), None, "HDR10 is no range the profile turns down");
        assert_eq!(get("h264-profile"), None);

        // Dolby Vision without a base layer: the profile turns it down.
        let vision = MediaStream { dv_profile: Some(5), ..hevc(DynamicRange::DolbyVision) };
        let settings = stream_settings(Some(&profile), Some(&vision));
        let ranges: Vec<&str> = setting(&settings, "hevc-rangetype").unwrap().split(',').collect();
        assert!(ranges.contains(&"HDR10") && ranges.contains(&"DOVIWithHDR10"), "{ranges:?}");
        assert!(!ranges.contains(&"DOVI") && !ranges.contains(&"DOVIWithEL"), "{ranges:?}");

        // fMP4 where the profile's first asks for it; no profile, H.264 and AAC.
        let fmp4 = json!({"TranscodingProfiles": [{"Type": "Video", "Container": "mp4", "Protocol": "hls",
            "VideoCodec": "av1,hevc,h264", "AudioCodec": "aac,opus"}]});
        let settings = stream_settings(Some(&fmp4), None);
        assert_eq!(setting(&settings, "SegmentContainer"), Some("mp4"));
        assert_eq!(setting(&settings, "VideoCodec"), Some("av1,hevc,h264"));
        let plain = stream_settings(None, None);
        assert_eq!(
            plain.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>(),
            ["VideoCodec=h264", "AudioCodec=aac", "SegmentContainer=ts"]
        );
    }

    /// The capabilities of a stream's address, as its query reads.
    fn caps(query: &str) -> Caps {
        let pairs = query.split('&').filter_map(|p| p.split_once('=')).map(|(k, v)| (k.to_owned(), v.to_owned()));
        stream_caps(&Q(pairs.collect()))
    }

    #[test]
    fn an_address_is_read_as_a_players_capabilities() {
        let c = caps(
            "VideoCodec=hevc,h264&AudioCodec=aac,ac3,eac3&SegmentContainer=mp4&TranscodingMaxAudioChannels=6\
             &hevc-profile=main,main10&hevc-level=153&hevc-videobitdepth=10&hevc-maxwidth=3840&hevc-maxheight=2160\
             &hevc-rangetype=SDR,HDR10,HLG,DOVI,DOVIWithHDR10",
        );
        assert!(c.is_v2() && c.master_playlist() && !c.adaptive());
        assert_eq!(c.segment_format(true), SegmentFormat::Fmp4);
        assert_eq!(c.segment_format(false), SegmentFormat::Ts, "a transcoder without fMP4");
        let decoders = c.video.as_ref().unwrap();
        assert_eq!(decoders.iter().map(|d| d.codec.as_str()).collect::<Vec<_>>(), ["hevc", "h264"]);
        let hevc = &decoders[0];
        assert_eq!(hevc.profiles, ["Main", "Main 10"]);
        assert_eq!((hevc.max_level, hevc.max_bit_depth, hevc.max_height), (Some(153), Some(10), Some(2160)));
        assert_eq!(c.hdr_types.as_deref().unwrap(), ["hdr10", "hlg", "dolby_vision"]);
        let profiles: Vec<i64> = c.dolby_vision_profiles.iter().flatten().map(|d| d.profile).collect();
        assert_eq!(profiles, [5, 8], "no profile 7 without DOVIWithEL");
        assert_eq!((c.audio_decode.as_deref().unwrap().len(), c.audio_channels), (3, Some(6)));

        // Nothing said: H.264 alone, HDR shown but no Dolby Vision, the
        // channels unsaid, MPEG-TS. JellyCon's own address, and the last
        // `SegmentContainer` an app appends.
        let plain = caps("MediaSourceId=x");
        assert_eq!(plain.video_codecs, ["h264"]);
        assert_eq!(plain.hdr_types.as_deref().unwrap(), ["hdr10", "hdr10plus", "hlg"]);
        assert!(plain.dolby_vision_profiles.is_none() && plain.audio_channels.is_none());
        assert_eq!(plain.segment_format(true), SegmentFormat::Ts);
        let kodi = caps("VideoCodec=h264&AudioCodec=ac3&TranscodingMaxAudioChannels=6&MaxVideoBitDepth=8");
        assert_eq!((kodi.video.as_ref().unwrap()[0].max_bit_depth, kodi.audio_channels), (Some(8), Some(6)));
        assert_eq!(caps("SegmentContainer=ts&SegmentContainer=mp4").segment_format(true), SegmentFormat::Fmp4);
    }

    fn file() -> FileRow {
        FileRow {
            id: 7,
            item_id: 1,
            library_id: 1,
            path: "/m/a.mkv".into(),
            size: 1,
            mtime: 0,
            container: Some("matroska,webm".into()),
            duration_ms: Some(60_000),
            bitrate: Some(40_000_000),
            width: Some(3840),
            height: Some(2160),
            video_codec: Some("hevc".into()),
            audio_codec: Some("truehd".into()),
            media_info: None,
            external_subs: "[]".into(),
            fingerprint: None,
            added_subs: "[]".into(),
        }
    }

    /// What Pley decides for a stream of `video` with TrueHD 7.1, for the
    /// address `query`, with a transcoder that makes fMP4 and HDR.
    fn decision(video: MediaStream, query: &str, max_bitrate: Option<u32>) -> Decision {
        let sound = MediaStream {
            index: 1,
            kind: StreamKind::Audio,
            codec: "truehd".into(),
            channels: Some(8),
            default: true,
            ..Default::default()
        };
        let info =
            MediaInfo { container: Some("matroska,webm".into()), streams: vec![video, sound], ..Default::default() };
        let req = PlaybackRequest {
            item_id: 1,
            file_id: Some(7),
            audio_index: None,
            subtitle_index: None,
            max_bitrate,
            max_height: None,
            start_ms: None,
            caps: caps(query),
            mode: PlaybackMode::Remux,
            client_id: None,
            keep_previous: false,
            keyed_urls: false,
            controls: Controls::new(),
            device_name: None,
            device_kind: None,
            hdr_transcodes: true,
            fmp4_segments: true,
            download: false,
            copy_video_only: false,
        };
        decide(&file(), Some(&info), &info.streams, &req)
    }

    #[test]
    fn streams_go_as_far_as_pleys_own_players() {
        let tv = "VideoCodec=hevc,h264&AudioCodec=aac,ac3,eac3&TranscodingMaxAudioChannels=6";
        // HEVC HDR10 copied, in MPEG-TS or fMP4 as asked, the sound made
        // E-AC-3 5.1.
        for (segments, format) in [("ts", SegmentFormat::Ts), ("mp4", SegmentFormat::Fmp4)] {
            let d = decision(hevc(DynamicRange::Hdr10), &format!("{tv}&SegmentContainer={segments}"), None);
            let (video, audio) = (d.video.unwrap(), d.audio.unwrap());
            assert_eq!((video.mode, d.segment_format), (VideoMode::Copy, format), "{segments}");
            assert_eq!((audio.mode, audio.codec, audio.channels), (AudioMode::Transcode, AudioCodec::Eac3, Some(6)));
        }
        // Stereo AAC for an app that takes no more.
        let audio = decision(hevc(DynamicRange::Hdr10), "VideoCodec=hevc,h264&AudioCodec=aac", None).audio.unwrap();
        assert_eq!((audio.codec, audio.channels), (AudioCodec::Aac, Some(2)));

        // A lower quality: converted, staying HDR10 in HEVC Main 10.
        let lower = decision(hevc(DynamicRange::Hdr10), &format!("{tv}&SegmentContainer=mp4"), Some(8_000));
        let lower = lower.video.unwrap();
        assert_eq!(
            (lower.mode, lower.codec, lower.dynamic_range),
            (VideoMode::Transcode, VideoCodec::Hevc, OutputRange::Hdr10)
        );
        // A screen that shows SDR alone: tone-mapped to H.264.
        let sdr = decision(hevc(DynamicRange::Hdr10), &format!("{tv}&hevc-rangetype=SDR"), None).video.unwrap();
        assert_eq!(
            (sdr.mode, sdr.codec, sdr.dynamic_range),
            (VideoMode::Transcode, VideoCodec::H264, OutputRange::Sdr)
        );
        // A level above the decoder's: converted.
        let level = decision(hevc(DynamicRange::Hdr10), &format!("{tv}&hevc-level=120"), None).video.unwrap();
        assert_eq!(level.mode, VideoMode::Transcode);

        // AV1 copied into fMP4 only.
        let av1 = MediaStream { codec: "av1".into(), profile: Some("Main".into()), ..hevc(DynamicRange::Hdr10) };
        let asked = "VideoCodec=av1,hevc,h264&AudioCodec=aac";
        let fmp4 = decision(av1.clone(), &format!("{asked}&SegmentContainer=mp4"), None).video.unwrap();
        assert_eq!(fmp4.mode, VideoMode::Copy);
        let ts = decision(av1, &format!("{asked}&SegmentContainer=ts"), None).video.unwrap();
        assert_eq!(ts.mode, VideoMode::Transcode);

        // Dolby Vision 8.1 kept and named for a device that shows it;
        // played as its HDR10 base layer by one that doesn't.
        let vision = MediaStream {
            dv_profile: Some(8),
            dv_bl_compat: Some(1),
            dv_level: Some(6),
            ..hevc(DynamicRange::DolbyVision)
        };
        let ranges = "hevc-rangetype=SDR,HDR10,DOVI,DOVIWithHDR10";
        let shown = decision(vision.clone(), &format!("{tv}&SegmentContainer=mp4&{ranges}"), None).video.unwrap();
        assert_eq!((shown.mode, shown.dolby_vision), (VideoMode::Copy, DolbyVisionMode::Signal));
        let base = decision(vision.clone(), &format!("{tv}&SegmentContainer=mp4"), None).video.unwrap();
        assert_eq!((base.mode, base.dolby_vision), (VideoMode::Copy, DolbyVisionMode::Copy));
        // Profile 5 for a device without Dolby Vision: converted, to HDR10.
        let five = MediaStream { dv_profile: Some(5), dv_bl_compat: Some(0), ..vision };
        let five = decision(five, &format!("{tv}&SegmentContainer=mp4"), None).video.unwrap();
        assert_eq!((five.mode, five.dynamic_range), (VideoMode::Transcode, OutputRange::Hdr10));
    }

    #[test]
    fn ranges_and_profiles_are_named_as_each_side_names_them() {
        let vision = |profile, compat, plus| MediaStream {
            dv_profile: Some(profile),
            dv_bl_compat: Some(compat),
            hdr10plus: Some(plus),
            ..hevc(DynamicRange::DolbyVision)
        };
        assert_eq!(range_type(&hevc(DynamicRange::Hdr10plus)), "HDR10Plus");
        assert_eq!(range_type(&vision(8, 1, false)), "DOVIWithHDR10");
        assert_eq!(range_type(&vision(8, 1, true)), "DOVIWithHDR10Plus");
        assert_eq!(range_type(&vision(7, 6, false)), "DOVIWithEL");
        assert_eq!(range_type(&vision(8, 4, false)), "DOVIWithHLG");
        assert_eq!(range_type(&vision(5, 0, false)), "DOVI");
        assert_eq!(ffprobe_profile("main10"), "Main 10");
        assert_eq!(ffprobe_profile("constrained baseline"), "Constrained Baseline");
    }
}

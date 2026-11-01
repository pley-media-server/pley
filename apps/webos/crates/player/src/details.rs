//! What the player says (packages/tv/src/player/details.ts, the Android
//! app's strings_session.xml) and Playback details: how the video plays,
//! why, and what the server's conversion does.

use pley_core::format::{codec_label, format_bitrate, resolution_label, stream_range};
use pley_core::model::{MediaFile, MediaStream, PlaybackInfo, PlaybackMethod, StreamAction, StreamKind, TranscodeInfo};

use crate::autoquality::Lowered;
use crate::plan::is_picture;
use crate::recovery::FallbackReason;

/// The menu's value while Automatic plays: "Automatic · 8 Mbps 1080p".
pub fn automatic(now: &str) -> String {
    format!("Automatic · {now}")
}

/// Automatic's hint when it isn't chosen.
pub const AUTO_HINT: &str = "Dynamically adjust to conditions";

/// Automatic's hint when chosen: "Now 8 Mbps 1080p · Lowered for your connection".
pub fn auto_now(now: &str, why: Option<&str>) -> String {
    match why {
        Some(why) => format!("Now {now} · {why}"),
        None => format!("Now {now}"),
    }
}

/// Why Automatic plays below the original.
pub fn lowered_text(why: Lowered) -> &'static str {
    match why {
        Lowered::Network => "Lowered for your connection",
        Lowered::Device => "Lowered for this device",
    }
}

/// The notice of a step down: "Quality lowered to 4 Mbps 1080p".
pub fn smoother(label: &str) -> String {
    format!("Quality lowered to {label}")
}

/// Up Next's heading: "Up next · S1 · E2", or "Up next".
pub fn up_next(label: Option<&str>) -> String {
    match label {
        Some(l) => format!("Up next · {l}"),
        None => "Up next".into(),
    }
}

/// Play next with no episode after this one.
pub const LAST_EPISODE: &str = "This is the last episode";
/// Play next at the end of a playlist or collection.
pub const LAST_IN_QUEUE: &str = "This is the last one";
/// The subtitles menu's first row.
pub const SUBTITLES_OFF: &str = "Off";
/// A subtitle track that couldn't be loaded.
pub const SUBTITLES_FAILED: &str = "These subtitles couldn't be loaded.";
/// The note when the server's conversion runs slower than real time.
pub const SLOW_SERVER: &str = "The server can't keep up with this video. Try a lower quality.";
/// An item with no file.
pub const NO_FILE: &str = "This item has no file to play.";
/// Subtitles the server draws onto the video.
pub const BURNED_IN: &str = "Burned in";
/// Under the ring while the server is out of reach.
pub const RECONNECTING: &str = "Reconnecting…";
/// The title of an error when the server is out of reach.
pub const OFFLINE_TITLE: &str = "Can't reach your server";
/// What to check when the server is out of reach.
pub const OFFLINE_BODY: &str = "Check that the server is on and that this device is on its network.";
/// A server that can't give keyed addresses.
pub const TOO_OLD: &str = "This server is too old to play on this TV: update it.";

/// "Subtitles added: French (SRT)".
pub fn subtitles_added(label: &str) -> String {
    format!("Subtitles added: {label}")
}

/// How the session plays, in words.
pub fn method_text(info: &PlaybackInfo) -> &'static str {
    match info.method {
        PlaybackMethod::Remux if info.audio_converted => "Direct stream · Audio transcoding",
        PlaybackMethod::Remux => "Direct stream",
        PlaybackMethod::Transcode => "Transcoding",
        _ => "Direct play",
    }
}

/// Why a fallback plays something other than the original.
pub fn fallback_text(reason: FallbackReason) -> &'static str {
    match reason {
        FallbackReason::Unplayable => "This device couldn't play the original file.",
        FallbackReason::NotSmooth => "The original didn't play smoothly on this device.",
    }
}

/// The server stopped the playback: who did, and their message when given.
pub fn stopped_text(message: Option<&str>, by_admin: bool) -> String {
    let who = if by_admin {
        "Playback was stopped by the server's administrator."
    } else {
        "Playback was stopped from another device."
    };
    match message.map(str::trim).filter(|m| !m.is_empty()) {
        Some(m) => format!("{who}\n{m}"),
        None => who.to_string(),
    }
}

/// How the subtitles chosen reach the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubtitleDelivery {
    ServerText,
    ServerPictures,
    BurnedIn,
}

impl SubtitleDelivery {
    /// What Info's Subtitles section says.
    pub fn label(self) -> &'static str {
        match self {
            SubtitleDelivery::BurnedIn => "Burned in by the server",
            _ => "Drawn by this TV",
        }
    }
}

/// How `s` reaches the screen in the session `info`.
pub fn delivery_of(info: &PlaybackInfo, s: &MediaStream, sideload: bool) -> SubtitleDelivery {
    if info.burn_subtitles && info.subtitle_index == Some(s.index) {
        return SubtitleDelivery::BurnedIn;
    }
    match (is_picture(Some(s)), sideload) {
        (true, true) => SubtitleDelivery::ServerPictures,
        (true, false) => SubtitleDelivery::BurnedIn,
        _ => SubtitleDelivery::ServerText,
    }
}

/// A row of Playback details.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetailRow {
    pub label: String,
    pub value: String,
}

/// How the video plays, while the details are open.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackDetails {
    pub method: String,
    pub why: Option<String>,
    pub pipeline: Option<String>,
    pub rows: Vec<DetailRow>,
    pub playback_method: PlaybackMethod,
    pub file: Option<MediaFile>,
    pub video: Option<MediaStream>,
    pub audio: Option<MediaStream>,
    pub subtitle: Option<MediaStream>,
    pub subtitle_delivery: Option<SubtitleDelivery>,
    pub transcode: Option<TranscodeInfo>,
}

fn encoder_name(encoder: &str) -> &str {
    match encoder {
        "libx264" => "x264",
        "libx265" => "x265",
        "h264_nvenc" | "hevc_nvenc" => "NVIDIA NVENC",
        "h264_qsv" | "hevc_qsv" => "Intel Quick Sync",
        "h264_amf" | "hevc_amf" => "AMD AMF",
        "h264_vaapi" | "hevc_vaapi" => "VA-API",
        "h264_videotoolbox" | "hevc_videotoolbox" => "VideoToolbox",
        "h264_vulkan" | "hevc_vulkan" => "Vulkan",
        "h264_v4l2m2m" => "V4L2",
        other => other,
    }
}

fn pipeline_name(pipeline: &str) -> &str {
    match pipeline {
        "gpu" => "GPU",
        "hybrid" => "GPU and CPU",
        "cpu" => "CPU",
        "copy" => "copy",
        other => other,
    }
}

fn channels_name(channels: i64) -> String {
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

/// A video stream as "HEVC 4K Dolby Vision P8.1".
pub fn video_label(v: Option<&MediaStream>) -> String {
    let Some(v) = v else { return String::new() };
    [
        codec_label(&v.codec),
        resolution_label(v.width, v.height).unwrap_or("").to_string(),
        stream_range(v).unwrap_or_default(),
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join(" ")
}

/// What the screen gets of HDR video: the live transcode's tone mapping when
/// there is one, else what the server said ("HDR10 (Dolby Vision base
/// layer)"), else none (SDR video, or an older server).
pub fn shown_as(info: &PlaybackInfo, live: Option<&TranscodeInfo>) -> Option<String> {
    if let Some(t) = live.and_then(|l| l.tonemap.as_deref()) {
        return Some(format!("SDR (tone-mapped with {t})"));
    }
    info.video_range.clone()
}

/// An audio stream as "TrueHD 7.1".
pub fn audio_label(a: Option<&MediaStream>) -> String {
    let Some(a) = a else { return String::new() };
    let channels = a.channels.filter(|c| *c != 0).map(channels_name);
    [Some(codec_label(&a.codec)), channels]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The transcoder's "H.264 1920×1080" as "H.264 1080p".
pub fn simplified(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_digit() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_digit() {
                i += 1;
            }
            if i < chars.len() && chars[i] == '×' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                let w: String = chars[start..i].iter().collect();
                let h: String = chars[i + 1..j].iter().collect();
                match resolution_label(w.parse().ok(), h.parse().ok()) {
                    Some(label) => out.push_str(label),
                    None => out.extend(&chars[start..j]),
                }
                i = j;
            } else {
                out.extend(&chars[start..i]);
            }
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// "NVIDIA NVENC (GPU)".
pub fn encoder_label(t: &TranscodeInfo) -> String {
    let name = t.encoder.as_deref().map(encoder_name);
    let pipeline = t.pipeline.as_deref().filter(|p| *p != "copy").map(|p| format!("({})", pipeline_name(p)));
    [name.map(str::to_string), pipeline].into_iter().flatten().collect::<Vec<_>>().join(" ")
}

/// What Playback details is built from.
pub struct DetailsInput<'a> {
    pub info: &'a PlaybackInfo,
    pub file: Option<&'a MediaFile>,
    pub audio_index: Option<i64>,
    pub note: Option<&'a str>,
    pub position_ms: i64,
    pub live: Option<&'a TranscodeInfo>,
    pub subtitle: Option<&'a MediaStream>,
    pub sideload: bool,
}

/// The details of a session (apps/web/src/player/PlaybackDetails.tsx, the Android app's PlaybackDetails).
pub fn build_details(o: DetailsInput<'_>) -> PlaybackDetails {
    let info = o.info;
    let transcode = if info.method == PlaybackMethod::Direct { None } else { o.live };
    let streams: &[MediaStream] = o.file.map_or(&[], |f| &f.streams);
    let video = streams.iter().find(|s| s.kind == StreamKind::Video);
    let audios: Vec<&MediaStream> = streams.iter().filter(|s| s.kind == StreamKind::Audio).collect();
    let want = o.audio_index.or(info.audio_index);
    let audio = audios
        .iter()
        .find(|s| Some(s.index) == want)
        .or_else(|| audios.iter().find(|s| s.default))
        .or(audios.first())
        .copied();
    // Nothing more specific known: the method says it.
    let why = if info.method == PlaybackMethod::Direct {
        o.note.map(str::to_string)
    } else {
        o.note
            .map(str::to_string)
            .or_else(|| Some(info.reasons.join(" ")).filter(|r| !r.is_empty()))
            .or_else(|| info.reason.clone())
    };
    let pending = "…";
    let video_in = match transcode.and_then(|t| t.video_in.as_deref()) {
        Some(v) => simplified(v),
        None => video_label(video),
    };
    let audio_in = transcode.and_then(|t| t.audio_in.clone()).unwrap_or_else(|| audio_label(audio));
    let converting = info.method != PlaybackMethod::Direct;
    let video_row = if converting && transcode.is_none_or(|t| t.video != StreamAction::Copy) {
        let out = match transcode.and_then(|t| t.video_out.as_deref()) {
            Some(v) => {
                format!("{}{}", simplified(v), if transcode.is_some_and(|t| t.tonemap.is_some()) { " SDR" } else { "" })
            }
            None => pending.to_string(),
        };
        format!("{video_in} → {out}")
    } else {
        video_in
    };
    let audio_row = if converting && transcode.is_none_or(|t| t.audio != StreamAction::Copy) {
        format!("{audio_in} → {}", transcode.and_then(|t| t.audio_out.as_deref()).unwrap_or(pending))
    } else {
        audio_in
    };
    let bitrate = format_bitrate(o.file.and_then(|f| f.bitrate));
    let ahead = transcode.map_or(0, |t| ((t.transcoded_ms - o.position_ms) / 1000).max(0));
    let mut rows = Vec::new();
    let mut row = |label: &str, value: String| rows.push(DetailRow { label: label.into(), value });
    if video.is_some() {
        row("Video", video_row);
    }
    if audio.is_some() {
        row("Audio", audio_row);
    }
    if !bitrate.is_empty() {
        row("Bitrate", bitrate);
    }
    if let Some(t) = transcode.filter(|t| t.video == StreamAction::Transcode && t.encoder.is_some()) {
        row("Encoder", encoder_label(t));
    }
    if let Some(shown) = video.and_then(|_| shown_as(info, transcode)) {
        row("Shown as", shown);
    }
    if transcode.is_some_and(|t| t.subtitle_burn) {
        row("Subtitles", BURNED_IN.into());
    }
    if let Some(speed) = transcode.and_then(|t| t.speed).filter(|s| *s > 0.0) {
        let speed = format!("{speed:.1}");
        row(
            "Speed",
            if ahead > 0 { format!("{speed}× real time · {ahead} s ready") } else { format!("{speed}× real time") },
        );
    }
    PlaybackDetails {
        method: method_text(info).into(),
        why,
        pipeline: transcode
            .filter(|t| t.video == StreamAction::Transcode)
            .and_then(|t| t.pipeline.as_deref())
            .map(|p| pipeline_name(p).to_string()),
        rows,
        playback_method: info.method,
        file: o.file.cloned(),
        video: video.cloned(),
        audio: audio.cloned(),
        subtitle: o.subtitle.cloned(),
        subtitle_delivery: o.subtitle.map(|s| delivery_of(info, s, o.sideload)),
        transcode: transcode.cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simplifies_the_transcoders_sizes() {
        assert_eq!(simplified("H.264 1920×1080"), "H.264 1080p");
        assert_eq!(simplified("HEVC 3840×2160 Dolby Vision P8.1"), "HEVC 4K Dolby Vision P8.1");
        assert_eq!(simplified("AAC 2.0"), "AAC 2.0");
    }

    #[test]
    fn stopped_texts() {
        assert_eq!(stopped_text(None, true), "Playback was stopped by the server's administrator.");
        assert_eq!(stopped_text(Some(" Bedtime "), false), "Playback was stopped from another device.\nBedtime");
    }

    #[test]
    fn a_transcode_reads_from_the_source_to_what_it_makes() {
        let info = PlaybackInfo { method: PlaybackMethod::Transcode, hls: true, ..PlaybackInfo::default() };
        let file = MediaFile {
            bitrate: Some(8_200_000),
            streams: vec![
                MediaStream {
                    index: 0,
                    kind: StreamKind::Video,
                    codec: "hevc".into(),
                    width: Some(3840),
                    height: Some(2160),
                    ..MediaStream::default()
                },
                MediaStream {
                    index: 1,
                    kind: StreamKind::Audio,
                    codec: "eac3".into(),
                    channels: Some(6),
                    default: true,
                    ..MediaStream::default()
                },
            ],
            ..MediaFile::default()
        };
        let live = TranscodeInfo {
            video: StreamAction::Transcode,
            audio: StreamAction::Copy,
            video_out: Some("H.264 1920×1080".into()),
            encoder: Some("h264_nvenc".into()),
            pipeline: Some("gpu".into()),
            speed: Some(2.46),
            transcoded_ms: 70_000,
            ..TranscodeInfo::default()
        };
        let d = build_details(DetailsInput {
            info: &info,
            file: Some(&file),
            audio_index: None,
            note: None,
            position_ms: 34_000,
            live: Some(&live),
            subtitle: None,
            sideload: true,
        });
        assert_eq!(d.method, "Transcoding");
        assert_eq!(d.pipeline.as_deref(), Some("GPU"));
        let rows: Vec<(&str, &str)> = d.rows.iter().map(|r| (r.label.as_str(), r.value.as_str())).collect();
        assert_eq!(rows[0].1, "HEVC 4K → H.264 1080p");
        assert_eq!(rows[1].1, "EAC3 5.1");
        assert!(rows.contains(&("Encoder", "NVIDIA NVENC (GPU)")));
        assert!(rows.contains(&("Speed", "2.5× real time · 36 s ready")));
    }
}

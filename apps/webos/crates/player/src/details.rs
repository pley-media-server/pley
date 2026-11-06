//! What the player says (packages/tv/src/player/details.ts, the Android
//! app's strings_session.xml) and Playback details: how the video plays,
//! why, and what the server's conversion does.

use pley_core::format::{codec_label, format_bitrate, resolution_label, stream_range};
use pley_core::model::{MediaFile, MediaStream, PlaybackInfo, PlaybackMethod, StreamAction, StreamKind, TranscodeInfo};
use pley_i18n::t;

use crate::autoquality::Lowered;
use crate::plan::is_picture;
use crate::recovery::FallbackReason;

/// The menu's value while Automatic plays: "Automatic · 8 Mbps 1080p".
pub fn automatic(now: &str) -> String {
    t!("player.quality.automaticAt", quality = now)
}

/// Automatic's hint when it isn't chosen.
pub fn auto_hint() -> &'static str {
    t!("player.quality.automaticHint")
}

/// Automatic's hint when chosen: "Now 8 Mbps 1080p · Lowered for your connection".
pub fn auto_now(now: &str, why: Option<&str>) -> String {
    match why {
        Some(why) => t!("player.quality.nowBecause", quality = now, why = why),
        None => t!("player.quality.now", quality = now),
    }
}

/// Why Automatic plays below the original.
pub fn lowered_text(why: Lowered) -> &'static str {
    match why {
        Lowered::Network => t!("player.quality.lowered.network"),
        Lowered::Device => t!("player.quality.lowered.device"),
    }
}

/// The notice of a step down: "Quality lowered to 4 Mbps 1080p".
pub fn smoother(label: &str) -> String {
    t!("player.toasts.qualityLowered", quality = label)
}

/// Up Next's heading: "Up next · S1 · E2", or "Up next".
pub fn up_next(label: Option<&str>) -> String {
    match label {
        Some(l) => t!("player.upNext.episode", episode = l),
        None => t!("player.upNext.title").into(),
    }
}

/// Play next with no episode after this one.
pub fn last_episode() -> &'static str {
    t!("player.toasts.lastEpisode")
}

/// Play next at the end of a playlist or collection.
pub fn last_in_queue() -> &'static str {
    t!("player.toasts.lastInQueue")
}

/// The subtitles menu's first row.
pub fn subtitles_off() -> &'static str {
    t!("player.subtitles.off")
}

/// A subtitle track that couldn't be loaded.
pub fn subtitles_failed() -> &'static str {
    t!("player.toasts.subtitlesNotLoaded")
}

/// The note when the server's conversion runs slower than real time.
pub fn slow_server() -> &'static str {
    t!("player.errors.slowServer")
}

/// An item with no file.
pub fn no_file() -> &'static str {
    t!("player.errors.noFile")
}

/// Subtitles the server draws onto the video.
pub fn burned_in() -> &'static str {
    t!("player.subtitles.burnedIn")
}

/// Under the ring while the server is out of reach.
pub fn reconnecting() -> &'static str {
    t!("player.errors.reconnecting")
}

/// The title of an error when the server is out of reach.
pub fn offline_title() -> &'static str {
    t!("player.errors.offline")
}

/// What to check when the server is out of reach.
pub fn offline_body() -> &'static str {
    t!("player.errors.reachHint")
}

/// A server that can't give keyed addresses.
pub fn too_old() -> &'static str {
    t!("player.errors.serverTooOld")
}

/// A video on a drive that can't be read (Android's `driveUnreadable`): what a server out of reach
/// would say of a stream.
pub fn drive_unreadable() -> &'static str {
    t!("player.errors.driveUnreadable")
}

/// A video on a drive whose picture never started in 12 s: the same file again wouldn't either.
pub fn local_stuck() -> &'static str {
    t!("player.errors.driveStuck")
}

/// "Subtitles added: French (SRT)".
pub fn subtitles_added(label: &str) -> String {
    t!("player.toasts.subtitlesAdded", track = label)
}

/// How the session plays, in words.
pub fn method_text(info: &PlaybackInfo) -> &'static str {
    match info.method {
        PlaybackMethod::Remux if info.audio_converted => t!("player.method.remuxAudio"),
        PlaybackMethod::Remux => t!("player.method.remux"),
        PlaybackMethod::Transcode => t!("player.method.transcode"),
        _ => t!("player.method.direct"),
    }
}

/// Why a fallback plays something other than the original.
pub fn fallback_text(reason: FallbackReason) -> &'static str {
    match reason {
        FallbackReason::Unplayable => t!("player.errors.originalFailed"),
        FallbackReason::NotSmooth => t!("player.errors.originalNotSmooth"),
    }
}

/// The server stopped the playback: who did, and their message when given.
pub fn stopped_text(message: Option<&str>, by_admin: bool) -> String {
    let who = if by_admin { t!("player.errors.stoppedByAdmin") } else { t!("player.errors.stoppedElsewhere") };
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
            SubtitleDelivery::BurnedIn => t!("player.info.burnedIn"),
            _ => t!("player.info.drawnHere"),
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
        "hybrid" => t!("player.details.pipeline.hybrid"),
        "cpu" => "CPU",
        "copy" => t!("player.details.pipeline.copy"),
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
        n => t!("player.details.channels", n = n),
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
        return Some(t!("player.details.toneMapped", method = t));
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
        row(t!("player.details.video"), video_row);
    }
    if audio.is_some() {
        row(t!("player.details.audio"), audio_row);
    }
    if !bitrate.is_empty() {
        row(t!("player.details.bitrate"), bitrate);
    }
    if let Some(t) = transcode.filter(|t| t.video == StreamAction::Transcode && t.encoder.is_some()) {
        row(t!("player.details.encoder"), encoder_label(t));
    }
    if let Some(shown) = video.and_then(|_| shown_as(info, transcode)) {
        row(t!("player.details.shownAs"), shown);
    }
    if transcode.is_some_and(|t| t.subtitle_burn) {
        row(t!("player.details.subtitles"), burned_in().into());
    }
    if let Some(speed) = transcode.and_then(|t| t.speed).filter(|s| *s > 0.0) {
        let speed = pley_i18n::decimal(speed, 1);
        row(
            t!("player.details.speed"),
            if ahead > 0 {
                t!("player.details.realTimeReady", speed = speed, seconds = ahead)
            } else {
                t!("player.details.realTime", speed = speed)
            },
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

        // In French: the method, the rows and the server's speed with a decimal comma.
        pley_i18n::with_language(pley_i18n::Language::Fr, || {
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
            assert_eq!(d.method, "Transcodage");
            let rows: Vec<(&str, &str)> = d.rows.iter().map(|r| (r.label.as_str(), r.value.as_str())).collect();
            assert!(rows.contains(&("Encodeur", "NVIDIA NVENC (GPU)")));
            assert!(rows.contains(&("Vitesse", "2,5× temps réel · 36 s d’avance")));
        });
    }

    #[test]
    fn automatic_says_what_it_plays_in_the_language_shown() {
        pley_i18n::with_language(pley_i18n::Language::Es, || {
            assert_eq!(automatic("8 Mbps 1080p"), "Automática · 8 Mbps 1080p");
            assert_eq!(
                auto_now("4 Mbps 720p", Some(lowered_text(Lowered::Network))),
                "Ahora 4 Mbps 720p · Reducida para tu conexión"
            );
            assert_eq!(up_next(Some("S1 · E2")), "A continuación · S1 · E2");
        });
    }
}

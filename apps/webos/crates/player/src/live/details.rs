//! Playback details for a live channel (a video's are [`crate::details`]): how it plays (the
//! provider's stream, the server's copy of it, or its transcode), why it isn't the provider's
//! stream, and what the channel is and is made into, from its session's plan.

use pley_core::format::{channels_label, codec_label, format_bitrate, frame_rate_label};
use pley_core::model::{ClientCaps, LivePlan};
use pley_i18n::t;

use crate::details::{DetailRow, encoder_name};
use crate::live::session::{as_is, decodes_video, plays_audio, sent_as_it_is};

/// Why a channel plays the server's HLS when the server doesn't send it as it is: what
/// [`crate::live::plays_as_sent`] refused it for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotAsSent {
    /// Direct Play is off in the server's Live TV settings (or the server is older than it).
    Off,
    /// Interlaced video, which the server makes progressive.
    Interlaced,
    /// Video this TV doesn't play from an MPEG-TS, by ffprobe's name.
    Video(String),
    /// Sound it doesn't.
    Audio(String),
}

impl NotAsSent {
    /// In words, for the details' Why.
    pub fn text(&self) -> String {
        match self {
            NotAsSent::Off => t!("livetv.details.directPlayOff").into(),
            NotAsSent::Interlaced => t!("livetv.details.interlaced").into(),
            NotAsSent::Video(codec) => t!("livetv.details.videoNotPlayed", codec = codec_label(codec)),
            NotAsSent::Audio(codec) => t!("livetv.details.audioNotPlayed", codec = codec_label(codec)),
        }
    }
}

/// Why the channel of `plan` doesn't play from its stream.ts on a TV reporting `caps`, in the
/// order the rule refuses it. None where the rule takes it (its stream.ts failed, or the app
/// can't hand it on), and where nothing is known to say: no report, or a codec the plan doesn't
/// name.
pub fn why_not_as_sent(plan: &LivePlan, caps: Option<&ClientCaps>) -> Option<NotAsSent> {
    if sent_as_it_is(plan) {
        return None;
    }
    match plan.direct_play {
        Some(true) => {}
        Some(false) => return Some(NotAsSent::Off),
        // An older server has no such setting: nothing to say of it.
        None => return None,
    }
    let caps = caps?;
    if !as_is(&plan.video) {
        if plan.made_progressive() {
            return Some(NotAsSent::Interlaced);
        }
        if !decodes_video(plan, caps) {
            return plan.video_codec.clone().map(NotAsSent::Video);
        }
    }
    if !as_is(&plan.audio) && !plays_audio(plan, caps) {
        return plan.audio_codec.clone().map(NotAsSent::Audio);
    }
    None
}

/// How a live channel plays, while its details are open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveDetails {
    /// "Direct play" from the provider's stream; from the server's HLS, "Direct stream" when
    /// the video is copied, else "Transcoding": a video's words.
    pub method: String,
    pub why: Option<String>,
    /// Video, Audio, Bitrate and Encoder; a row whose value isn't known is left out.
    pub rows: Vec<DetailRow>,
}

/// The codec an encoder makes, by ffprobe's name: `h264_nvenc` and `libx264` make `h264`.
fn encoder_codec(encoder: &str) -> &str {
    match encoder {
        "libx264" => "h264",
        "libx265" => "hevc",
        other => other.split('_').next().unwrap_or(other),
    }
}

/// "HEVC 10-bit · 3840×2160 · 50 fps", and what a transcode makes of it: "… → H.264 1080p".
fn video_row(plan: &LivePlan, converted: bool) -> Option<String> {
    let codec = plan.video_codec.as_deref().filter(|c| !c.is_empty())?;
    let mut name = codec_label(codec);
    if let Some(depth) = plan.bit_depth.filter(|d| *d > 8) {
        name = format!("{name} {}", t!("format.track.bitDepth", n = depth));
    }
    let size = plan.width.zip(plan.height).filter(|(w, h)| *w > 0 && *h > 0).map(|(w, h)| format!("{w}×{h}"));
    let rate = frame_rate_label(plan.frame_rate, plan.interlaced);
    let sent = [Some(name), size, rate].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    if !converted {
        return Some(sent);
    }
    // A transcode that keeps the size doesn't say its lines: the provider's.
    let lines = plan.out_height.or(plan.height).filter(|h| *h > 0).map(|h| format!("{h}p"));
    let made = [Some(codec_label(encoder_codec(&plan.video))), lines].into_iter().flatten().collect::<Vec<_>>();
    Some(format!("{sent} → {}", made.join(" ")))
}

/// "EAC3 5.1", and what the server makes of it: "… → AAC Stereo".
fn audio_row(plan: &LivePlan, converted: bool) -> Option<String> {
    let codec = plan.audio_codec.as_deref().filter(|c| !c.is_empty())?;
    let sent = [codec_label(codec), channels_label(plan.audio_channels, None)]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if !converted {
        return Some(sent);
    }
    // The server's AAC is stereo.
    let made = match plan.audio.as_str() {
        "aac" => format!("{} {}", codec_label("aac"), t!("format.channels.stereo")),
        other => codec_label(other),
    };
    Some(format!("{sent} → {made}"))
}

/// What the provider sends now, and a transcode's video bitrate after it: "12 Mbps → 8.0 Mbps".
fn bitrate_row(plan: &LivePlan, converted: bool) -> Option<String> {
    let rate = |kbps: i64| Some(format_bitrate(Some(kbps.max(0).saturating_mul(1000)))).filter(|r| !r.is_empty());
    let sent = rate(plan.kbps)?;
    Some(match plan.video_kbps.filter(|_| converted).and_then(rate) {
        Some(made) => format!("{sent} → {made}"),
        None => sent,
    })
}

/// The details of the channel playing; `direct`: what plays is its stream.ts, as the provider
/// sends it, whatever the server would make of it. `plan`: what the server last said of it.
/// None for a channel on the server's HLS whose plan isn't known: nothing says how it plays.
pub fn live_details(plan: Option<&LivePlan>, direct: bool, caps: Option<&ClientCaps>) -> Option<LiveDetails> {
    let Some(plan) = plan else {
        return direct.then(|| LiveDetails { method: t!("player.method.direct").into(), why: None, rows: Vec::new() });
    };
    let video_converted = !direct && !as_is(&plan.video);
    let audio_converted = !direct && !as_is(&plan.audio);
    let method = if direct {
        t!("player.method.direct")
    } else if video_converted {
        t!("player.method.transcode")
    } else if audio_converted {
        t!("player.method.remuxAudio")
    } else {
        t!("player.method.remux")
    };
    let why = if direct { None } else { why_not_as_sent(plan, caps).map(|w| w.text()) };
    let mut rows = Vec::new();
    let mut row = |label: &str, value: Option<String>| {
        if let Some(value) = value {
            rows.push(DetailRow { label: label.into(), value });
        }
    };
    row(t!("player.details.video"), video_row(plan, video_converted));
    row(t!("player.details.audio"), audio_row(plan, audio_converted));
    row(t!("player.details.bitrate"), bitrate_row(plan, video_converted));
    row(t!("player.details.encoder"), video_converted.then(|| encoder_name(&plan.video).to_string()));
    Some(LiveDetails { method: method.into(), why, rows })
}

#[cfg(test)]
mod tests {
    use pley_core::device::{CapsOverrides, lg_2022_caps};
    use pley_i18n::{Language, with_language};

    use super::*;

    fn caps() -> ClientCaps {
        lg_2022_caps(&CapsOverrides::default()).caps
    }

    /// An HEVC channel, 10-bit 4K at 50 with E-AC-3 5.1, as the server would transcode it.
    fn hevc() -> LivePlan {
        LivePlan {
            video_codec: Some("hevc".into()),
            width: Some(3840),
            height: Some(2160),
            frame_rate: Some(50.0),
            bit_depth: Some(10),
            audio_codec: Some("eac3".into()),
            audio_channels: Some(6),
            video: "h264_nvenc".into(),
            audio: "aac".into(),
            out_height: Some(1080),
            video_kbps: Some(8_000),
            direct_play: Some(true),
            kbps: 12_300,
            ..LivePlan::default()
        }
    }

    fn rows(d: &LiveDetails) -> Vec<(&str, &str)> {
        d.rows.iter().map(|r| (r.label.as_str(), r.value.as_str())).collect()
    }

    #[test]
    fn the_providers_stream_is_direct_play_with_what_the_channel_is() {
        let d = live_details(Some(&hevc()), true, Some(&caps())).unwrap();
        assert_eq!((d.method.as_str(), d.why.as_deref()), ("Direct play", None));
        // Nothing is made of it: no transcode's size, sound, bitrate or encoder.
        assert_eq!(
            rows(&d),
            [("Video", "HEVC 10-bit · 3840×2160 · 50 fps"), ("Audio", "EAC3 5.1"), ("Bitrate", "12 Mbps")]
        );
    }

    #[test]
    fn a_transcode_reads_from_the_channel_to_what_the_server_makes() {
        // Its stream.ts failed, say: the TV plays HEVC, so nothing more is known to say why.
        let d = live_details(Some(&hevc()), false, Some(&caps())).unwrap();
        assert_eq!((d.method.as_str(), d.why.as_deref()), ("Transcoding", None));
        assert_eq!(
            rows(&d),
            [
                ("Video", "HEVC 10-bit · 3840×2160 · 50 fps → H.264 1080p"),
                ("Audio", "EAC3 5.1 → AAC Stereo"),
                ("Bitrate", "12 Mbps → 8.0 Mbps"),
                ("Encoder", "NVIDIA NVENC"),
            ]
        );
        // A transcode that keeps the size has the provider's lines.
        let same = LivePlan { out_height: None, height: Some(720), width: Some(1280), ..hevc() };
        let d = live_details(Some(&same), false, Some(&caps())).unwrap();
        assert_eq!(rows(&d)[0].1, "HEVC 10-bit · 1280×720 · 50 fps → H.264 720p");
    }

    #[test]
    fn a_copied_channel_on_hls_is_direct_stream() {
        let copied = LivePlan {
            video_codec: Some("h264".into()),
            width: Some(1920),
            height: Some(1080),
            frame_rate: Some(25.0),
            bit_depth: Some(8),
            audio_codec: Some("aac".into()),
            audio_channels: Some(2),
            video: "copy".into(),
            audio: "copy".into(),
            direct_play: Some(true),
            kbps: 850,
            ..LivePlan::default()
        };
        let d = live_details(Some(&copied), false, Some(&caps())).unwrap();
        assert_eq!((d.method.as_str(), d.why.as_deref()), ("Direct stream", None));
        assert_eq!(
            rows(&d),
            [("Video", "H.264 · 1920×1080 · 25 fps"), ("Audio", "AAC Stereo"), ("Bitrate", "850 kbps")]
        );
        // Sound alone converted, as a video's: DTS, which no LG TV reads from an MPEG-TS.
        let dts = LivePlan { audio_codec: Some("dts".into()), audio_channels: Some(6), audio: "aac".into(), ..copied };
        let d = live_details(Some(&dts), false, Some(&caps())).unwrap();
        assert_eq!(d.method, "Direct stream · Audio transcoding");
        assert_eq!(d.why.as_deref(), Some("This TV doesn't play DTS sound"));
        assert_eq!(rows(&d)[1], ("Audio", "DTS 5.1 → AAC Stereo"));
    }

    #[test]
    fn why_follows_the_rule() {
        let caps = caps();
        let why = |plan: &LivePlan| why_not_as_sent(plan, Some(&caps));
        assert_eq!(why(&LivePlan { direct_play: Some(false), ..hevc() }), Some(NotAsSent::Off));
        assert_eq!(why(&LivePlan { interlaced: true, ..hevc() }), Some(NotAsSent::Interlaced));
        // The server's setting comes first.
        assert_eq!(why(&LivePlan { interlaced: true, direct_play: Some(false), ..hevc() }), Some(NotAsSent::Off));
        // AV1, which the pipeline doesn't read from an MPEG-TS; 12-bit HEVC, beyond its decoder.
        assert_eq!(why(&LivePlan { video_codec: Some("av1".into()), ..hevc() }), Some(NotAsSent::Video("av1".into())));
        assert_eq!(why(&LivePlan { bit_depth: Some(12), ..hevc() }), Some(NotAsSent::Video("hevc".into())));
        assert_eq!(
            why(&LivePlan { audio_codec: Some("truehd".into()), ..hevc() }),
            Some(NotAsSent::Audio("truehd".into()))
        );
        // Played as it is, with no report to go by, or with nothing named: nothing to say.
        assert_eq!(why(&hevc()), None);
        assert_eq!(why_not_as_sent(&LivePlan { bit_depth: Some(12), ..hevc() }, None), None);
        assert_eq!(why(&LivePlan { video_codec: None, ..hevc() }), None);
        // Interlaced video the server copies stays as it is: only its sound can be why.
        let copied = LivePlan { interlaced: true, video: "copy".into(), ..hevc() };
        assert_eq!(why(&copied), None);
    }

    #[test]
    fn rows_without_a_value_are_left_out() {
        // From an older server: no bit depth, no bitrate, and no Direct Play setting to speak of.
        let old = LivePlan {
            video_codec: Some("mpeg2video".into()),
            audio_codec: Some("mp2".into()),
            interlaced: true,
            frame_rate: Some(25.0),
            video: "libx264".into(),
            audio: "aac".into(),
            ..LivePlan::default()
        };
        let d = live_details(Some(&old), false, Some(&caps())).unwrap();
        assert_eq!(d.why, None);
        let off = LivePlan { direct_play: Some(false), ..old.clone() };
        let said = live_details(Some(&off), false, Some(&caps())).unwrap().why;
        assert_eq!(said.as_deref(), Some("Direct play is off on the server"));
        assert_eq!(
            rows(&d),
            [("Video", "MPEG-2 · 25 fps, interlaced → H.264"), ("Audio", "MP2 → AAC Stereo"), ("Encoder", "x264")]
        );
        // A radio channel: no video row.
        let radio = LivePlan {
            audio_codec: Some("mp3".into()),
            audio_channels: Some(2),
            video: "none".into(),
            audio: "aac".into(),
            direct_play: Some(true),
            ..LivePlan::default()
        };
        let d = live_details(Some(&radio), true, Some(&caps())).unwrap();
        assert_eq!(rows(&d), [("Audio", "MP3 Stereo")]);
        // No plan: the provider's stream says how it plays, the server's HLS nothing yet.
        let d = live_details(None, true, None).unwrap();
        assert_eq!((d.method.as_str(), d.rows.len()), ("Direct play", 0));
        assert_eq!(live_details(None, false, None), None);
    }

    #[test]
    fn details_in_the_language_shown() {
        let caps = caps();
        with_language(Language::Fr, || {
            let d = live_details(Some(&LivePlan { interlaced: true, ..hevc() }), false, Some(&caps)).unwrap();
            assert_eq!(d.method, "Transcodage");
            assert_eq!(d.why.as_deref(), Some("La vidéo entrelacée est désentrelacée"));
            assert_eq!(rows(&d)[2], ("Débit", "12 Mbit/s → 8,0 Mbit/s"));
        });
        with_language(Language::Es, || {
            let plan = LivePlan { video_codec: Some("av1".into()), ..hevc() };
            let d = live_details(Some(&plan), false, Some(&caps)).unwrap();
            assert_eq!(d.why.as_deref(), Some("Esta TV no reproduce AV1"));
        });
    }
}

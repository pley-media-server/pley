//! Playback details for a live channel (a video's are [`crate::details`]): how it plays (the
//! provider's stream, the server's copy of it, or its transcode), why it isn't the provider's
//! stream, and what the channel is and is made into, from its session's plan. They read as a
//! video's do: the same rows in the same words, and the provider's video and sound in sections
//! of their own, as a file's streams are.

use pley_core::format::{
    Fact, FactBlock, bit_depth_label, channels_label, codec_label, dimensions_label, format_bitrate, frame_rate_label,
};
use pley_core::model::{ClientCaps, LivePlan, MediaStream};
use pley_i18n::t;

use crate::details::{DetailRow, audio_label, encoder_name, video_label};
use crate::live::session::{as_is, decodes_video, reports_video, sent_as_it_is};

/// Why a channel plays the server's HLS when the server doesn't send it as it is: what
/// [`crate::live::plays_as_sent`] refused it for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotAsSent {
    /// Interlaced video, which the server makes progressive.
    Interlaced,
    /// Video this TV's report doesn't decode, by ffprobe's name, at the bit depth the plan says.
    Video(String, Option<i64>),
    /// Sound it neither decodes nor sends to a receiver.
    Audio(String),
}

impl NotAsSent {
    /// In a sentence, for the details' Why.
    pub fn text(&self) -> String {
        match self {
            NotAsSent::Interlaced => t!("livetv.details.interlaced").into(),
            NotAsSent::Video(codec, depth) => {
                // The depth beside the codec above 8 bits ("HEVC 10-bit"): a TV that plays 8-bit
                // HEVC isn't told it doesn't play HEVC.
                let codec = match depth.filter(|d| *d > 8) {
                    Some(d) => format!("{} {}", codec_label(codec), t!("format.track.bitDepth", n = d)),
                    None => codec_label(codec),
                };
                t!("livetv.details.videoNotPlayed", codec = codec)
            }
            NotAsSent::Audio(codec) => t!("livetv.details.audioNotPlayed", codec = codec_label(codec)),
        }
    }
}

/// Why the channel of `plan` doesn't play from its stream.ts on a TV reporting `caps`, in the
/// order the rule refuses it. None where the rule takes it (its stream.ts failed, or the app
/// can't hand it on), and where nothing is known to say: no report, a codec the plan doesn't
/// name, or one the report plays that the pipeline doesn't read from an MPEG-TS (VP9, AV1, Opus),
/// of which Android's `notAsSent` and the Samsung app's `WhyNotAsSent` say nothing either.
pub fn why_not_as_sent(plan: &LivePlan, caps: Option<&ClientCaps>) -> Option<NotAsSent> {
    if sent_as_it_is(plan) {
        return None;
    }
    let caps = caps?;
    if !as_is(&plan.video) {
        if plan.made_progressive() {
            return Some(NotAsSent::Interlaced);
        }
        if !reports_video(plan, caps) {
            return plan.video_codec.clone().map(|codec| NotAsSent::Video(codec, plan.bit_depth));
        }
        // Video the TV plays from a file but its pipeline doesn't read from an MPEG-TS (VP9, AV1):
        // no reason to give, as Android and the Samsung app give none.
        if !decodes_video(plan, caps) {
            return None;
        }
    }
    // Sound the report plays, read from an MPEG-TS or not (Opus, DTS sent to a receiver), likewise.
    if !as_is(&plan.audio) && !plan.audio_codec.as_deref().is_some_and(|c| caps.plays_audio(c)) {
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
    /// Video and Audio: what the provider sends, as the plan has it. A video's Info has these
    /// sections for its file's streams.
    pub streams: Vec<FactBlock>,
}

/// The codec an encoder makes, by ffprobe's name: `h264_nvenc` and `libx264` make `h264`.
fn encoder_codec(encoder: &str) -> &str {
    match encoder {
        "libx264" => "h264",
        "libx265" => "hevc",
        other => other.split('_').next().unwrap_or(other),
    }
}

/// "HEVC 4K", and what a transcode makes of it: "… → H.264 1080p", as a video's row says them.
fn video_row(plan: &LivePlan, converted: bool) -> Option<String> {
    let codec = plan.video_codec.as_deref().filter(|c| !c.is_empty())?;
    let label = |codec: &str, width, height| {
        video_label(Some(&MediaStream { codec: codec.into(), width, height, ..MediaStream::default() }))
    };
    let sent = label(codec, plan.width, plan.height);
    if !converted {
        return Some(sent);
    }
    // A transcode says its lines only when it changes them: its width follows the provider's shape.
    let size = plan.width.zip(plan.height).filter(|(w, h)| *w > 0 && *h > 0);
    let (width, height) = match (plan.out_height.filter(|h| *h > 0), size) {
        (Some(lines), Some((w, h))) => (Some(w.saturating_mul(lines) / h), Some(lines)),
        (Some(lines), None) => (None, Some(lines)),
        (None, _) => (plan.width, plan.height),
    };
    Some(format!("{sent} → {}", label(encoder_codec(&plan.video), width, height)))
}

/// "EAC3 5.1", and what the server makes of it: "… → AAC 2.0", as a video's row says them.
fn audio_row(plan: &LivePlan, converted: bool) -> Option<String> {
    let codec = plan.audio_codec.as_deref().filter(|c| !c.is_empty())?;
    let label = |codec: &str, channels| {
        audio_label(Some(&MediaStream { codec: codec.into(), channels, ..MediaStream::default() }))
    };
    let sent = label(codec, plan.audio_channels);
    if !converted {
        return Some(sent);
    }
    // The server's AAC is stereo.
    let made = match plan.audio.as_str() {
        "aac" => label("aac", Some(2)),
        other => label(other, None),
    };
    Some(format!("{sent} → {made}"))
}

/// The provider's video and sound under Video and Audio, with a file's labels for what the plan
/// says of them: the codec, the size, the frame rate and the bit depth; the codec and the
/// channels. A fact it doesn't say is left out, and a section with none.
fn stream_blocks(plan: &LivePlan) -> Vec<FactBlock> {
    let named = |codec: &Option<String>| codec.as_deref().filter(|c| !c.is_empty()).map(codec_label);
    let block = |title: &str, facts: Vec<(&str, Option<String>)>| {
        let facts: Vec<Fact> = facts
            .into_iter()
            .filter_map(|(label, value)| Some(Fact { label: label.into(), value: value.filter(|v| !v.is_empty())? }))
            .collect();
        (!facts.is_empty()).then(|| FactBlock { title: title.into(), facts })
    };
    let video = block(
        t!("player.details.video"),
        vec![
            (t!("format.media.codec"), named(&plan.video_codec)),
            (t!("format.media.resolution"), dimensions_label(plan.width, plan.height)),
            (t!("format.media.frameRate"), frame_rate_label(plan.frame_rate, plan.interlaced)),
            (t!("format.media.bitDepth"), bit_depth_label(plan.bit_depth, None)),
        ],
    );
    let audio = block(
        t!("player.details.audio"),
        vec![
            (t!("format.media.codec"), named(&plan.audio_codec)),
            (t!("format.media.channels"), Some(channels_label(plan.audio_channels, None))),
        ],
    );
    video.into_iter().chain(audio).collect()
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
        return direct.then(|| LiveDetails {
            method: t!("player.method.direct").into(),
            why: None,
            rows: Vec::new(),
            streams: Vec::new(),
        });
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
    Some(LiveDetails { method: method.into(), why, rows, streams: stream_blocks(plan) })
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
            kbps: 12_300,
            ..LivePlan::default()
        }
    }

    fn rows(d: &LiveDetails) -> Vec<(&str, &str)> {
        d.rows.iter().map(|r| (r.label.as_str(), r.value.as_str())).collect()
    }

    /// The sections after How it plays: each one's title and facts.
    fn streams(d: &LiveDetails) -> Vec<(&str, Vec<(&str, &str)>)> {
        d.streams
            .iter()
            .map(|b| (b.title.as_str(), b.facts.iter().map(|f| (f.label.as_str(), f.value.as_str())).collect()))
            .collect()
    }

    #[test]
    fn the_providers_stream_is_direct_play_with_what_the_channel_is() {
        let d = live_details(Some(&hevc()), true, Some(&caps())).unwrap();
        assert_eq!((d.method.as_str(), d.why.as_deref()), ("Direct play", None));
        // Nothing is made of it: no transcode's size, sound, bitrate or encoder.
        assert_eq!(rows(&d), [("Video", "HEVC 4K"), ("Audio", "EAC3 5.1"), ("Bitrate", "12 Mbps")]);
        // What the provider sends, in a video's Video and Audio sections.
        assert_eq!(
            streams(&d),
            [
                (
                    "Video",
                    vec![
                        ("Codec", "HEVC"),
                        ("Resolution", "3840×2160 (4K)"),
                        ("Frame rate", "50 fps"),
                        ("Bit depth", "10-bit"),
                    ]
                ),
                ("Audio", vec![("Codec", "EAC3"), ("Channels", "5.1")]),
            ]
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
                ("Video", "HEVC 4K → H.264 1080p"),
                ("Audio", "EAC3 5.1 → AAC 2.0"),
                ("Bitrate", "12 Mbps → 8.0 Mbps"),
                ("Encoder", "NVIDIA NVENC"),
            ]
        );
        // The sections say what the provider sends, whatever is made of it.
        assert_eq!(streams(&d), streams(&live_details(Some(&hevc()), true, Some(&caps())).unwrap()));
        // A transcode that keeps the size has the provider's.
        let same = LivePlan { out_height: None, height: Some(720), width: Some(1280), ..hevc() };
        let d = live_details(Some(&same), false, Some(&caps())).unwrap();
        assert_eq!(rows(&d)[0].1, "HEVC 720p → H.264 720p");
        // One it makes smaller keeps the provider's shape: 1920×800 is 1080p, 1280×534 is 720p.
        let wide = LivePlan { width: Some(1920), height: Some(800), out_height: Some(534), ..hevc() };
        let d = live_details(Some(&wide), false, Some(&caps())).unwrap();
        assert_eq!(rows(&d)[0].1, "HEVC 1080p → H.264 720p");
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
            kbps: 850,
            ..LivePlan::default()
        };
        let d = live_details(Some(&copied), false, Some(&caps())).unwrap();
        assert_eq!((d.method.as_str(), d.why.as_deref()), ("Direct stream", None));
        assert_eq!(rows(&d), [("Video", "H.264 1080p"), ("Audio", "AAC 2.0"), ("Bitrate", "850 kbps")]);
        // Sound alone converted, as a video's: DTS, which this TV neither decodes nor sends on.
        let dts = LivePlan { audio_codec: Some("dts".into()), audio_channels: Some(6), audio: "aac".into(), ..copied };
        let d = live_details(Some(&dts), false, Some(&caps())).unwrap();
        assert_eq!(d.method, "Direct stream · Audio transcoding");
        assert_eq!(d.why.as_deref(), Some("This TV doesn't play DTS audio."));
        assert_eq!(rows(&d)[1], ("Audio", "DTS 5.1 → AAC 2.0"));
    }

    #[test]
    fn why_follows_the_rule() {
        let caps = caps();
        let why = |plan: &LivePlan| why_not_as_sent(plan, Some(&caps));
        assert_eq!(why(&LivePlan { interlaced: true, ..hevc() }), Some(NotAsSent::Interlaced));
        // VC-1, which the TV's report doesn't decode; 12-bit HEVC, beyond its decoder.
        let vc1 = LivePlan { video_codec: Some("vc1".into()), ..hevc() };
        assert_eq!(why(&vc1), Some(NotAsSent::Video("vc1".into(), Some(10))));
        assert_eq!(why(&LivePlan { bit_depth: Some(12), ..hevc() }), Some(NotAsSent::Video("hevc".into(), Some(12))));
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
        // From an older server: no bit depth and no bitrate.
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
        assert_eq!(d.why.as_deref(), Some("Interlaced video is made progressive."));
        assert_eq!(rows(&d), [("Video", "MPEG-2 → H.264"), ("Audio", "MP2 → AAC 2.0"), ("Encoder", "x264")]);
        assert_eq!(
            streams(&d),
            [
                ("Video", vec![("Codec", "MPEG-2"), ("Frame rate", "25 fps, interlaced")]),
                ("Audio", vec![("Codec", "MP2")]),
            ]
        );
        // A radio channel: no video row.
        let radio = LivePlan {
            audio_codec: Some("mp3".into()),
            audio_channels: Some(2),
            video: "none".into(),
            audio: "aac".into(),
            ..LivePlan::default()
        };
        let d = live_details(Some(&radio), true, Some(&caps())).unwrap();
        assert_eq!(rows(&d), [("Audio", "MP3 2.0")]);
        assert_eq!(streams(&d), [("Audio", vec![("Codec", "MP3"), ("Channels", "Stereo")])]);
        // No plan: the provider's stream says how it plays, the server's HLS nothing yet.
        let d = live_details(None, true, None).unwrap();
        assert_eq!((d.method.as_str(), d.rows.len(), d.streams.len()), ("Direct play", 0, 0));
        assert_eq!(live_details(None, false, None), None);
    }

    #[test]
    fn details_in_the_language_shown() {
        let caps = caps();
        with_language(Language::Fr, || {
            let d = live_details(Some(&LivePlan { interlaced: true, ..hevc() }), false, Some(&caps)).unwrap();
            assert_eq!(d.method, "Transcodage");
            assert_eq!(d.why.as_deref(), Some("La vidéo entrelacée est convertie en progressif."));
            assert_eq!(rows(&d)[2], ("Débit", "12 Mbit/s → 8,0 Mbit/s"));
            assert_eq!(streams(&d)[0].1[2], ("Débit d’images", "50 i/s, entrelacé"));
        });
        with_language(Language::Es, || {
            let plan = LivePlan { video_codec: Some("vc1".into()), ..hevc() };
            let d = live_details(Some(&plan), false, Some(&caps)).unwrap();
            assert_eq!(d.why.as_deref(), Some("Este televisor no reproduce el video VC-1 10 bits."));
        });
    }

    #[test]
    fn what_the_tv_plays_but_not_from_an_mpeg_ts_is_no_reason() {
        // As Android's and the Samsung app's: the report decodes AV1, VP9 and Opus, and sends DTS to a
        // receiver when Settings says so, which the pipeline doesn't read from an MPEG-TS. The TV plays
        // them from a file, so it isn't said not to.
        let mut caps = caps();
        caps.audio_passthrough.push("dts".into());
        let why = |plan: &LivePlan| why_not_as_sent(plan, Some(&caps));
        for codec in ["av1", "vp9"] {
            let plan = LivePlan { video_codec: Some(codec.into()), ..hevc() };
            assert_eq!(why(&plan), None, "{codec}");
            assert_eq!(live_details(Some(&plan), false, Some(&caps)).unwrap().why, None, "{codec}");
        }
        for codec in ["opus", "dts"] {
            assert_eq!(why(&LivePlan { audio_codec: Some(codec.into()), ..hevc() }), None, "{codec}");
        }
        // What the report doesn't decode is still said, whether the pipeline reads it from an MPEG-TS or not.
        let av1 = LivePlan { video_codec: Some("av1".into()), bit_depth: Some(12), ..hevc() };
        assert_eq!(why(&av1), Some(NotAsSent::Video("av1".into(), Some(12))));
    }

    #[test]
    fn why_names_the_codec_with_its_bit_depth_above_8() {
        // 12-bit HEVC on a TV that plays HEVC to 10 bits: the depth says what it doesn't play.
        assert_eq!(NotAsSent::Video("hevc".into(), Some(12)).text(), "This TV doesn't play HEVC 12-bit video.");
        // At 8 bits, or with no depth in the plan, the codec alone.
        assert_eq!(NotAsSent::Video("av1".into(), Some(8)).text(), "This TV doesn't play AV1 video.");
        assert_eq!(NotAsSent::Video("vp9".into(), None).text(), "This TV doesn't play VP9 video.");
        // Sound has no depth to say.
        assert_eq!(NotAsSent::Audio("truehd".into()).text(), "This TV doesn't play TrueHD audio.");
        assert_eq!(NotAsSent::Interlaced.text(), "Interlaced video is made progressive.");
        // Every language says them the same way, the TV its subject.
        for (language, video, audio, interlaced) in [
            (
                Language::Es,
                "Este televisor no reproduce el video HEVC 12 bits.",
                "Este televisor no reproduce el audio TrueHD.",
                "El video entrelazado se convierte a progresivo.",
            ),
            (
                Language::Fr,
                "Ce téléviseur ne lit pas la vidéo HEVC 12 bits.",
                "Ce téléviseur ne lit pas l’audio TrueHD.",
                "La vidéo entrelacée est convertie en progressif.",
            ),
            (
                Language::ZhCn,
                "此电视无法播放 HEVC 12 位 视频。",
                "此电视无法播放 TrueHD 音频。",
                "隔行扫描视频会转换为逐行扫描。",
            ),
            (
                Language::ZhTw,
                "此電視無法播放 HEVC 12 位 影片。",
                "此電視無法播放 TrueHD 音訊。",
                "隔行掃描影片會轉換為逐行掃描。",
            ),
        ] {
            with_language(language, || {
                assert_eq!(NotAsSent::Video("hevc".into(), Some(12)).text(), video);
                assert_eq!(NotAsSent::Audio("truehd".into()).text(), audio);
                assert_eq!(NotAsSent::Interlaced.text(), interlaced);
            });
        }
    }
}

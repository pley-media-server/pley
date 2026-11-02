//! A file's facts in blocks of "label: value" lines, for the title page's
//! media info and Playback details, as the Android app's `MediaFacts` words
//! them (packages/tv/src/format/media-info.ts).

use super::{
    channels_label, codec_label, fixed, format_bitrate, format_bytes, format_duration, is_atmos, language_name,
    resolution_label, stream_range,
};
use crate::model::{MediaFile, MediaStream, StreamKind};

/// One line: a label and its value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fact {
    pub label: String,
    pub value: String,
}

/// A titled group of facts ("File", "Video", "Audio 2"…).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FactBlock {
    pub title: String,
    pub facts: Vec<Fact>,
}

impl FactBlock {
    /// The value of the fact labelled `label`.
    pub fn value(&self, label: &str) -> Option<&str> {
        self.facts.iter().find(|f| f.label == label).map(|f| f.value.as_str())
    }
}

/// A codec level as people write it: HEVC 153 → "5.1", H.264 51 → "5.1", AV1 13 → "5.1".
pub fn level_label(codec: &str, level: Option<i64>) -> Option<String> {
    let level = level.filter(|l| *l > 0)?;
    let c = codec.to_ascii_lowercase();
    if c == "av1" {
        return Some(format!("{}.{}", 2 + (level >> 2), level & 3));
    }
    let n = if c == "hevc" || c == "h265" { level as f64 / 30.0 } else { level as f64 / 10.0 };
    let text = fixed(n, 1);
    Some(text.strip_suffix(".0").map_or(text.clone(), str::to_string))
}

/// "Main 10 · Level 5.1".
pub fn profile_level_label(s: &MediaStream) -> Option<String> {
    let parts: Vec<String> = [
        s.profile.clone().unwrap_or_default(),
        level_label(&s.codec, s.level).map_or(String::new(), |l| format!("Level {l}")),
    ]
    .into_iter()
    .filter(|p| !p.is_empty())
    .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// "23.976 fps": up to three decimals, trailing zeros left out.
pub fn frame_rate_label(fps: Option<f64>, interlaced: bool) -> Option<String> {
    let fps = fps.filter(|f| *f > 0.0)?;
    let text = fixed(fps, 3);
    let text = text.trim_end_matches('0').trim_end_matches('.');
    Some(format!("{text} fps{}", if interlaced { ", interlaced" } else { "" }))
}

/// "48 kHz", "44.1 kHz".
pub fn sample_rate_label(hz: Option<i64>) -> Option<String> {
    let hz = hz.filter(|h| *h > 0)?;
    let text = fixed(hz as f64 / 1000.0, 1);
    Some(format!("{} kHz", text.strip_suffix(".0").unwrap_or(&text)))
}

/// "10-bit · 4:2:0".
pub fn bit_depth_label(depth: Option<i64>, pix_fmt: Option<&str>) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(d) = depth.filter(|d| *d > 0) {
        parts.push(format!("{d}-bit"));
    }
    let f = pix_fmt.unwrap_or("").to_ascii_lowercase();
    let chroma = if ["yuv420", "nv12", "p010"].iter().any(|p| f.starts_with(p)) {
        "4:2:0"
    } else if f.starts_with("yuv422") {
        "4:2:2"
    } else if f.starts_with("yuv444") {
        "4:4:4"
    } else if f.starts_with("gray") {
        "4:0:0"
    } else {
        ""
    };
    if !chroma.is_empty() {
        parts.push(chroma.into());
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn primaries(name: &str) -> Option<&'static str> {
    Some(match name {
        "bt709" => "BT.709",
        "bt2020" => "BT.2020",
        "smpte170m" => "BT.601 NTSC",
        "bt470bg" => "BT.601 PAL",
        "bt470m" => "BT.470 M",
        "smpte240m" => "SMPTE 240M",
        "smpte431" => "DCI-P3",
        "smpte432" => "Display P3",
        "film" => "Film",
        _ => return None,
    })
}

fn transfer(name: &str) -> Option<&'static str> {
    Some(match name {
        "smpte2084" => "PQ (SMPTE ST 2084)",
        "arib-std-b67" => "HLG (ARIB STD-B67)",
        "bt709" => "BT.709",
        "smpte170m" => "BT.601",
        "bt470bg" => "BT.470 BG",
        "bt2020-10" => "BT.2020 (10-bit)",
        "bt2020-12" => "BT.2020 (12-bit)",
        "iec61966-2-1" => "sRGB",
        "linear" => "Linear",
        _ => return None,
    })
}

fn matrix(name: &str) -> Option<&'static str> {
    Some(match name {
        "bt709" => "BT.709",
        "bt2020nc" => "BT.2020 non-constant",
        "bt2020c" => "BT.2020 constant",
        "smpte170m" | "bt470bg" => "BT.601",
        "ictcp" => "ICtCp",
        "gbr" => "RGB",
        "ycgco" => "YCgCo",
        "fcc" => "FCC",
        _ => return None,
    })
}

fn colour(table: fn(&str) -> Option<&'static str>, name: Option<&str>) -> Option<String> {
    let name = name.filter(|n| !n.is_empty() && *n != "unknown" && *n != "reserved")?;
    Some(table(name).map_or_else(|| name.to_string(), str::to_string))
}

/// "Profile 8.1 · Level 6 · HDR10-compatible base layer". Only 8.1, 8.2 and 8.4 carry their
/// compatibility in the name, as the labels do: compatibility 6 reads "Profile 8 · … · Blu-ray
/// HDR10-compatible base layer".
pub fn dolby_vision_details(s: &MediaStream) -> Option<String> {
    let profile = s.dv_profile?;
    let compat = s.dv_bl_compat.unwrap_or(0);
    let mut parts = vec![format!(
        "Profile {profile}{}",
        if profile >= 8 && matches!(compat, 1 | 2 | 4) { format!(".{compat}") } else { String::new() }
    )];
    if let Some(level) = s.dv_level {
        parts.push(format!("Level {level}"));
    }
    let base = match compat {
        0 => Some("no compatible base layer".to_string()),
        1 => Some("HDR10-compatible base layer".into()),
        2 => Some("SDR-compatible base layer".into()),
        4 => Some("HLG-compatible base layer".into()),
        6 => Some("Blu-ray HDR10-compatible base layer".into()),
        _ => None,
    };
    parts.extend(base);
    if profile == 7 {
        match s.dv_el.as_deref() {
            Some("fel") => parts.push("full enhancement layer".into()),
            Some("mel") => parts.push("minimal enhancement layer".into()),
            _ => {}
        }
    }
    Some(parts.join(" · "))
}

/// "3840×2160 (4K)".
pub fn dimensions_label(w: Option<i64>, h: Option<i64>) -> Option<String> {
    let (w, h) = (w.filter(|w| *w > 0)?, h.filter(|h| *h > 0)?);
    Some(match resolution_label(Some(w), Some(h)) {
        Some(r) => format!("{w}×{h} ({r})"),
        None => format!("{w}×{h}"),
    })
}

fn flags(s: &MediaStream) -> Option<String> {
    let f: Vec<&str> = [(s.default, "Default"), (s.forced, "Forced"), (s.external, "Beside the media")]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
    (!f.is_empty()).then(|| f.join(", "))
}

fn push(out: &mut Vec<Fact>, label: &str, value: Option<String>) {
    if let Some(v) = value.filter(|v| !v.trim().is_empty()) {
        out.push(Fact { label: label.into(), value: v });
    }
}

/// The file name only, after the last `/` or `\`.
pub fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// A file's blocks: the file, then each stream, video first.
pub fn media_facts(file: &MediaFile) -> Vec<FactBlock> {
    let mut f = Vec::new();
    push(&mut f, "Name", Some(file_name(&file.path).to_string()));
    push(&mut f, "Container", file.container.as_ref().map(|c| c.to_uppercase()));
    push(&mut f, "Size", (file.size > 0).then(|| format_bytes(Some(file.size as u64))));
    push(&mut f, "Duration", file.duration_ms.filter(|d| *d > 0).map(|d| format_duration(Some(d))));
    push(&mut f, "Overall bitrate", file.bitrate.filter(|b| *b > 0).map(|b| format_bitrate(Some(b))));
    let mut blocks = vec![FactBlock { title: "File".into(), facts: f }];
    let order = |k: StreamKind| match k {
        StreamKind::Video => 0,
        StreamKind::Audio => 1,
        _ => 2,
    };
    let mut streams: Vec<&MediaStream> = file.streams.iter().collect();
    streams.sort_by_key(|s| (order(s.kind), s.index));
    let count = |k: StreamKind| streams.iter().filter(|s| order(s.kind) == order(k)).count();
    let mut seen = [0usize; 3];
    for s in &streams {
        let o = order(s.kind);
        seen[o] += 1;
        let name = ["Video", "Audio", "Subtitles"][o];
        let title = if count(s.kind) > 1 { format!("{name} {}", seen[o]) } else { name.to_string() };
        blocks.push(stream_block(s, &title));
    }
    blocks
}

/// One stream's facts, under `title`.
pub fn stream_block(s: &MediaStream, title: &str) -> FactBlock {
    let mut facts = Vec::new();
    match s.kind {
        StreamKind::Video => {
            push(&mut facts, "Codec", Some(codec_label(&s.codec)));
            push(&mut facts, "Profile", profile_level_label(s));
            push(&mut facts, "Resolution", dimensions_label(s.width, s.height));
            let interlaced = s.field_order.as_deref().is_some_and(|f| !f.is_empty() && f != "progressive");
            push(&mut facts, "Frame rate", frame_rate_label(s.frame_rate, interlaced));
            push(&mut facts, "Bit depth", bit_depth_label(s.bit_depth, s.pix_fmt.as_deref()));
            push(&mut facts, "HDR", Some(stream_range(s).unwrap_or_else(|| "SDR".into())));
            push(&mut facts, "Dolby Vision", dolby_vision_details(s));
            push(&mut facts, "Primaries", colour(primaries, s.color_primaries.as_deref()));
            push(&mut facts, "Transfer", colour(transfer, s.color_transfer.as_deref()));
            push(&mut facts, "Matrix", colour(matrix, s.color_space.as_deref()));
            push(&mut facts, "Bitrate", s.bitrate.filter(|b| *b > 0).map(|b| format_bitrate(Some(b))));
            push(&mut facts, "Title", s.title.clone());
        }
        StreamKind::Audio => {
            push(
                &mut facts,
                "Codec",
                Some(format!("{}{}", codec_label(&s.codec), if is_atmos(s) { " Atmos" } else { "" })),
            );
            push(&mut facts, "Profile", s.profile.clone());
            let ch = channels_label(s.channels, s.channel_layout.as_deref());
            let channels = match s.channel_layout.as_deref() {
                Some(layout) if !ch.is_empty() && layout != ch => format!("{ch} ({layout})"),
                _ => ch,
            };
            push(&mut facts, "Channels", Some(channels));
            push(&mut facts, "Sample rate", sample_rate_label(s.sample_rate));
            push(&mut facts, "Language", language_name(s.language.as_deref()).or(s.language.clone()));
            push(&mut facts, "Title", s.title.clone());
            push(&mut facts, "Bitrate", s.bitrate.filter(|b| *b > 0).map(|b| format_bitrate(Some(b))));
            push(&mut facts, "Flags", flags(s));
        }
        _ => {
            let kind = if s.text_based == Some(false) { "picture" } else { "text" };
            push(&mut facts, "Format", Some(format!("{}, {kind}", codec_label(&s.codec))));
            push(&mut facts, "Language", language_name(s.language.as_deref()).or(s.language.clone()));
            push(&mut facts, "Title", s.title.clone());
            push(&mut facts, "Flags", flags(s));
        }
    }
    FactBlock { title: title.into(), facts }
}

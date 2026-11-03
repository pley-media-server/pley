//! Files, streams, times and sizes in words: the Android app's labels
//! (`core/model/format`), which follow the web's, as the TV app ports them
//! (packages/tv/src/format/format.ts).

mod media_info;

pub use media_info::{
    Fact, FactBlock, bit_depth_label, dimensions_label, dolby_vision_details, file_name, frame_rate_label, level_label,
    media_facts, profile_level_label, sample_rate_label, stream_block,
};

use crate::model::{DynamicRange, ItemKind, ItemSummary, MediaFile, MediaStream, MediaSummary, StreamKind};

/// `v` with `digits` decimals as JavaScript's `toFixed` writes it (halves round up).
pub(crate) fn fixed(v: f64, digits: i32) -> String {
    let scale = 10f64.powi(digits);
    let rounded = (v * scale).round() / scale;
    format!("{rounded:.prec$}", prec = digits as usize)
}

/// ffprobe's codec name as people know it: `h264` → "H.264", `hdmv_pgs_subtitle`
/// → "PGS"; others upper-cased.
pub fn codec_label(codec: &str) -> String {
    let known = match codec.to_ascii_lowercase().as_str() {
        "h264" => "H.264",
        "hevc" | "h265" => "HEVC",
        "av1" => "AV1",
        "vp9" => "VP9",
        "vp8" => "VP8",
        "mpeg4" => "MPEG-4",
        "mpeg2video" => "MPEG-2",
        "mpeg1video" => "MPEG-1",
        "vc1" => "VC-1",
        "wmv3" => "WMV",
        "msmpeg4v3" => "DivX",
        "theora" => "Theora",
        "prores" => "ProRes",
        "aac" => "AAC",
        "ac3" => "AC3",
        "eac3" => "EAC3",
        "dts" => "DTS",
        "truehd" => "TrueHD",
        "mlp" => "MLP",
        "flac" => "FLAC",
        "alac" => "ALAC",
        "mp3" => "MP3",
        "mp2" => "MP2",
        "opus" => "Opus",
        "vorbis" => "Vorbis",
        "wmav2" => "WMA",
        "pcm_s16le" | "pcm_s24le" | "pcm_s32le" | "pcm_f32le" => "PCM",
        "subrip" | "srt" => "SRT",
        "ass" => "ASS",
        "ssa" => "SSA",
        "webvtt" => "WebVTT",
        "mov_text" => "MOV Text",
        "hdmv_pgs_subtitle" | "pgssub" => "PGS",
        "dvd_subtitle" | "dvdsub" => "VobSub",
        "dvb_subtitle" => "DVB",
        "dvb_teletext" => "Teletext",
        "eia_608" => "CEA-608",
        _ => return codec.to_uppercase(),
    };
    known.to_string()
}

/// 2 → "Stereo", 6 → "5.1", 8 → "7.1", 5 → "5ch"; the layout's name without a count.
pub fn channels_label(channels: Option<i64>, layout: Option<&str>) -> String {
    match channels.unwrap_or(0) {
        0 => layout.unwrap_or("").to_string(),
        1 => "Mono".into(),
        2 => "Stereo".into(),
        3 => "2.1".into(),
        6 => "5.1".into(),
        7 => "6.1".into(),
        8 => "7.1".into(),
        n => format!("{n}ch"),
    }
}

/// "4K", "1440p", "1080p", "720p", "576p", "480p" or "SD"; none without a size.
pub fn resolution_label(width: Option<i64>, height: Option<i64>) -> Option<&'static str> {
    let (w, h) = (width.unwrap_or(0), height.unwrap_or(0));
    if w == 0 && h == 0 {
        return None;
    }
    Some(if w >= 3800 || h >= 2100 {
        "4K"
    } else if w >= 2500 || h >= 1400 {
        "1440p"
    } else if w >= 1800 || h >= 1000 {
        "1080p"
    } else if w >= 1200 || h >= 700 {
        "720p"
    } else if h >= 540 {
        "576p"
    } else if h >= 470 {
        "480p"
    } else {
        "SD"
    })
}

/// The four steps badges use: "4K", "1080p", "720p", "SD"; none without a size.
pub fn resolution_badge(width: Option<i64>, height: Option<i64>) -> Option<&'static str> {
    let (w, h) = (width.unwrap_or(0), height.unwrap_or(0));
    if w >= 3800 || h >= 2100 {
        Some("4K")
    } else if w >= 1800 || h >= 1000 {
        Some("1080p")
    } else if w >= 1200 || h >= 700 {
        Some("720p")
    } else if h > 0 {
        Some("SD")
    } else {
        None
    }
}

/// "Dolby Vision P8.1" (the base layer's compatibility from profile 8 on:
/// profile 7's is always Blu-ray HDR10, never "P7.6"), "Dolby Vision P7 FEL"
/// (or MEL), "+ HDR10+" with HDR10+ metadata, "HDR10+", "HDR10", "HLG", "HDR"
/// (an older server's flag), or none for SDR: the server's names.
pub fn dynamic_range_label(
    range: Option<DynamicRange>,
    dv_profile: Option<i64>,
    dv_bl_compat: Option<i64>,
    hdr: Option<bool>,
    dv_el: Option<&str>,
    hdr10plus: Option<bool>,
) -> Option<String> {
    match range {
        Some(DynamicRange::DolbyVision) => {
            let mut label = "Dolby Vision".to_string();
            if let Some(p) = dv_profile {
                let compat = match dv_bl_compat {
                    Some(c @ (1 | 2 | 4)) if p >= 8 => format!(".{c}"),
                    _ => String::new(),
                };
                let layer = match dv_el {
                    Some(el) if p == 7 && !el.is_empty() => format!(" {}", el.to_uppercase()),
                    _ => String::new(),
                };
                label.push_str(&format!(" P{p}{compat}{layer}"));
            }
            Some(if hdr10plus == Some(true) { format!("{label} + HDR10+") } else { label })
        }
        Some(DynamicRange::Hdr10Plus) => Some("HDR10+".into()),
        Some(DynamicRange::Hdr10) => Some("HDR10".into()),
        Some(DynamicRange::Hlg) => Some("HLG".into()),
        Some(DynamicRange::Sdr) => None,
        _ => (hdr == Some(true)).then(|| "HDR".to_string()),
    }
}

/// A video stream's whole format: "Dolby Vision P8.1 + HDR10+", "HDR10+"…
pub fn stream_range(s: &MediaStream) -> Option<String> {
    dynamic_range_label(s.dynamic_range, s.dv_profile, s.dv_bl_compat, s.hdr, s.dv_el.as_deref(), s.hdr10plus)
}

/// "DV P8.1", "DV P7 FEL", "DV", "HDR10+", "HDR10", "HLG", "HDR", or none.
pub fn dynamic_range_short(
    range: Option<DynamicRange>,
    dv_profile: Option<i64>,
    dv_bl_compat: Option<i64>,
    hdr: Option<bool>,
    dv_el: Option<&str>,
) -> Option<String> {
    dynamic_range_label(range, dv_profile, dv_bl_compat, hdr, dv_el, None).map(|l| l.replace("Dolby Vision", "DV"))
}

/// A video stream's range badges: "DV P8.1", and "HDR10+" too for Dolby Vision with HDR10+.
pub fn dynamic_range_badges(s: &MediaStream) -> Vec<String> {
    let Some(badge) = dynamic_range_short(s.dynamic_range, s.dv_profile, s.dv_bl_compat, s.hdr, s.dv_el.as_deref())
    else {
        return Vec::new();
    };
    if s.dynamic_range == Some(DynamicRange::DolbyVision) && s.hdr10plus == Some(true) {
        vec![badge, "HDR10+".into()]
    } else {
        vec![badge]
    }
}

/// Dolby Atmos, as ffprobe names it in the stream's profile or a title says.
pub fn is_atmos(s: &MediaStream) -> bool {
    s.kind == StreamKind::Audio
        && [s.profile.as_deref(), s.title.as_deref()].into_iter().flatten().any(|t| t.to_lowercase().contains("atmos"))
}

/// "TrueHD Atmos 7.1", "EAC3 5.1", "AAC Stereo".
pub fn audio_format_label(s: &MediaStream) -> String {
    let parts = [
        codec_label(&s.codec),
        if is_atmos(s) { "Atmos".into() } else { String::new() },
        channels_label(s.channels, s.channel_layout.as_deref()),
    ];
    parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ")
}

/// Whether a character counts as part of a word in subtitle titles (letters,
/// digits, and everything from U+00C0 on, as the server's `is_forced` reads them).
fn wordy(c: char) -> bool {
    c.is_ascii_digit() || c.is_ascii_lowercase() || c.is_ascii_uppercase() || ('\u{C0}'..='\u{FFFF}').contains(&c)
}

/// Subtitles the server counts as forced: flagged so, or a title saying so
/// ("English (Forced)", "Signs & Songs").
pub fn is_forced_subtitle(s: &MediaStream) -> bool {
    if s.forced {
        return true;
    }
    let title = s.title.as_deref().unwrap_or("").to_lowercase();
    title.split(|c: char| !wordy(c)).any(|w| w == "forced" || w == "signs")
}

/// A word character as a regular expression's `\b` sees it.
fn word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `text` with every whole word `word` (ASCII, any case) replaced by a space.
fn without_word(text: &str, word: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut prev: Option<char> = None;
    while let Some(c) = rest.chars().next() {
        let starts = rest.len() >= word.len() && rest.as_bytes()[..word.len()].eq_ignore_ascii_case(word.as_bytes());
        // The bytes matched are ASCII, so the word's end is a character boundary.
        if starts && !prev.is_some_and(word_char) && !rest[word.len()..].chars().next().is_some_and(word_char) {
            out.push(' ');
            prev = Some(' ');
            rest = &rest[word.len()..];
            continue;
        }
        out.push(c);
        prev = Some(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Where `needle` is in `hay`, ignoring case: byte offsets of its start and end.
fn find_ignoring_case(hay: &str, needle: &str) -> Option<(usize, usize)> {
    let wanted: Vec<char> = needle.chars().flat_map(char::to_lowercase).collect();
    if wanted.is_empty() {
        return None;
    }
    for (start, _) in hay.char_indices() {
        let mut have = Vec::new();
        for (i, c) in hay[start..].char_indices() {
            have.extend(c.to_lowercase());
            if have.len() >= wanted.len() {
                if have == wanted {
                    return Some((start, start + i + c.len_utf8()));
                }
                break;
            }
        }
    }
    None
}

/// What a subtitle title says beyond its language and "forced" (shown as a
/// flag): "SDH" of "English SDH", nothing of "English (Forced)", the whole
/// title when it doesn't name the language ("Signs & Songs").
fn subtitle_title_rest(title: &str, lang: &str) -> String {
    let Some((at, end)) = find_ignoring_case(title, lang) else {
        let rest = without_word(title, "forced");
        return if rest.chars().any(wordy) { title.to_string() } else { String::new() };
    };
    let joined = format!("{} {}", &title[..at], &title[end..]);
    let no_forced = without_word(&joined, "forced");
    let punct: &[char] = &['(', ')', '[', ']', '{', '}', '-', '–', '—', '_', '.', ',', ':', ';', '|', '/'];
    let spaced: String = no_forced.chars().map(|c| if punct.contains(&c) { ' ' } else { c }).collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A track as the pickers list it. Audio: "English (EAC3 5.1)", "English –
/// Commentary (AAC Stereo)". Subtitles: "French – SDH (SRT, Forced)". Video:
/// "4K HEVC Dolby Vision P8.1 10-bit".
pub fn stream_label(s: &MediaStream) -> String {
    let lang = language_name(s.language.as_deref());
    let title = s.title.as_deref().filter(|t| !t.is_empty());
    match s.kind {
        StreamKind::Audio => {
            let extra = match (title, &lang) {
                (Some(t), Some(l)) if !t.to_lowercase().contains(&l.to_lowercase()) => format!(" – {t}"),
                _ => String::new(),
            };
            let details = audio_format_label(s);
            let base = match (title, &lang) {
                (Some(t), None) => t.to_string(),
                (_, Some(l)) => l.clone(),
                (None, None) => "Unknown".into(),
            };
            format!("{base}{extra}{}", if details.is_empty() { String::new() } else { format!(" ({details})") })
        }
        StreamKind::Subtitle => {
            let base = lang.clone().or(title.map(str::to_string)).unwrap_or_else(|| "Unknown".into());
            let rest = match (title, &lang) {
                (Some(t), Some(l)) => subtitle_title_rest(t, l),
                _ => String::new(),
            };
            let flags: Vec<String> = [
                codec_label(&s.codec),
                if is_forced_subtitle(s) { "Forced".into() } else { String::new() },
                if s.external { "External".into() } else { String::new() },
            ]
            .into_iter()
            .filter(|f| !f.is_empty())
            .collect();
            let rest = if rest.is_empty() { String::new() } else { format!(" – {rest}") };
            let flags = if flags.is_empty() { String::new() } else { format!(" ({})", flags.join(", ")) };
            format!("{base}{rest}{flags}")
        }
        _ => {
            let depth = s.bit_depth.filter(|d| *d > 8).map(|d| format!("{d}-bit"));
            [
                resolution_label(s.width, s.height).map(str::to_string),
                Some(codec_label(&s.codec)),
                stream_range(s),
                depth,
            ]
            .into_iter()
            .flatten()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
        }
    }
}

/// The short quality badges of a file: resolution, dynamic range, then
/// "Atmos", else "7.1" or "5.1".
pub fn quality_badges(file: &MediaFile) -> Vec<String> {
    let video = file.streams_of(StreamKind::Video).next();
    let audio =
        file.streams_of(StreamKind::Audio).find(|s| s.default).or_else(|| file.streams_of(StreamKind::Audio).next());
    let mut out = Vec::new();
    if let Some(b) =
        resolution_badge(file.width.or(video.and_then(|v| v.width)), file.height.or(video.and_then(|v| v.height)))
    {
        out.push(b.to_string());
    }
    if let Some(v) = video {
        out.extend(dynamic_range_badges(v));
    }
    let channels = audio.and_then(|a| a.channels).unwrap_or(0);
    if audio.is_some_and(is_atmos) {
        out.push("Atmos".into());
    } else if channels >= 8 {
        out.push("7.1".into());
    } else if channels >= 6 {
        out.push("5.1".into());
    }
    out
}

/// The badges a listing can give: 4K (no other resolution, as on the web) and the dynamic range,
/// HDR10 and HDR10+ as plain "HDR" and "HDR+" (a title's page names them exactly).
pub fn summary_badges(media: &MediaSummary) -> Vec<String> {
    let range = match media.dynamic_range {
        Some(DynamicRange::DolbyVision) => Some("DV"),
        Some(DynamicRange::Hdr10Plus) => Some("HDR+"),
        Some(DynamicRange::Hdr10) => Some("HDR"),
        Some(DynamicRange::Hlg) => Some("HLG"),
        _ => None,
    };
    [resolution_badge(media.width, media.height).filter(|r| *r == "4K"), range]
        .into_iter()
        .flatten()
        .map(str::to_string)
        .collect()
}

/// Whether the profile picked the file's audio track; older servers don't
/// say, and their choice reads as picked.
pub fn audio_picked(file: &MediaFile) -> bool {
    file.audio_picked != Some(false)
}

/// The audio track a file is described by: the profile's pick, else the
/// default, else the first.
pub fn starting_audio(file: &MediaFile) -> Option<&MediaStream> {
    let mut audio = file.streams_of(StreamKind::Audio);
    let picked = if audio_picked(file) {
        file.streams_of(StreamKind::Audio).find(|s| Some(s.index) == file.selected_audio)
    } else {
        None
    };
    picked.or_else(|| file.streams_of(StreamKind::Audio).find(|s| s.default)).or_else(|| audio.next())
}

/// The audio a title's page offers while no track is picked: "Automatic
/// (English)", the language of the details' choice (else of the default track).
pub fn automatic_audio_label(file: &MediaFile) -> String {
    let track = file
        .streams_of(StreamKind::Audio)
        .find(|s| Some(s.index) == file.selected_audio)
        .or_else(|| file.streams_of(StreamKind::Audio).find(|s| s.default))
        .or_else(|| file.streams_of(StreamKind::Audio).next());
    let code = track.and_then(|t| t.language.as_deref()).unwrap_or("");
    let lang = if code.is_empty() || ["und", "zxx", "mul"].contains(&code) { None } else { language_name(Some(code)) };
    match lang {
        Some(l) => format!("Automatic ({l})"),
        None => "Automatic".into(),
    }
}

/// "Final Cut · 4K · HEVC · MKV · 58.2 GB", else "Version 2".
pub fn version_label(file: &MediaFile, position: usize) -> String {
    let parts: Vec<String> = [
        file.edition.clone().unwrap_or_default(),
        resolution_label(file.width, file.height).unwrap_or("").to_string(),
        file.video_codec.as_deref().map(codec_label).unwrap_or_default(),
        file.container.as_deref().unwrap_or("").to_uppercase(),
        if file.size > 0 { format_bytes(Some(file.size as u64)) } else { String::new() },
    ]
    .into_iter()
    .filter(|p| !p.is_empty())
    .collect();
    if parts.is_empty() { format!("Version {}", position + 1) } else { parts.join(" · ") }
}

/// ISO 639 codes (both kinds of three letters, and two) and their English names.
const LANGUAGES: &[(&str, &str)] = &[
    ("afr", "Afrikaans"),
    ("alb", "Albanian"),
    ("sqi", "Albanian"),
    ("amh", "Amharic"),
    ("ara", "Arabic"),
    ("arm", "Armenian"),
    ("hye", "Armenian"),
    ("aze", "Azerbaijani"),
    ("baq", "Basque"),
    ("eus", "Basque"),
    ("bel", "Belarusian"),
    ("ben", "Bengali"),
    ("bos", "Bosnian"),
    ("bre", "Breton"),
    ("bul", "Bulgarian"),
    ("bur", "Burmese"),
    ("mya", "Burmese"),
    ("cat", "Catalan"),
    ("chi", "Chinese"),
    ("zho", "Chinese"),
    ("cze", "Czech"),
    ("ces", "Czech"),
    ("dan", "Danish"),
    ("dut", "Dutch"),
    ("nld", "Dutch"),
    ("eng", "English"),
    ("est", "Estonian"),
    ("fao", "Faroese"),
    ("fil", "Filipino"),
    ("fin", "Finnish"),
    ("fre", "French"),
    ("fra", "French"),
    ("geo", "Georgian"),
    ("kat", "Georgian"),
    ("ger", "German"),
    ("deu", "German"),
    ("gle", "Irish"),
    ("glg", "Galician"),
    ("gre", "Greek"),
    ("ell", "Greek"),
    ("guj", "Gujarati"),
    ("heb", "Hebrew"),
    ("hin", "Hindi"),
    ("hrv", "Croatian"),
    ("hun", "Hungarian"),
    ("ice", "Icelandic"),
    ("isl", "Icelandic"),
    ("ind", "Indonesian"),
    ("ita", "Italian"),
    ("jpn", "Japanese"),
    ("kan", "Kannada"),
    ("kaz", "Kazakh"),
    ("khm", "Khmer"),
    ("kor", "Korean"),
    ("kur", "Kurdish"),
    ("lao", "Lao"),
    ("lat", "Latin"),
    ("lav", "Latvian"),
    ("lit", "Lithuanian"),
    ("ltz", "Luxembourgish"),
    ("mac", "Macedonian"),
    ("mkd", "Macedonian"),
    ("mal", "Malayalam"),
    ("mao", "Maori"),
    ("mri", "Maori"),
    ("mar", "Marathi"),
    ("may", "Malay"),
    ("msa", "Malay"),
    ("mlt", "Maltese"),
    ("mon", "Mongolian"),
    ("nep", "Nepali"),
    ("nob", "Norwegian Bokmål"),
    ("nno", "Norwegian Nynorsk"),
    ("nor", "Norwegian"),
    ("per", "Persian"),
    ("fas", "Persian"),
    ("pol", "Polish"),
    ("por", "Portuguese"),
    ("pan", "Punjabi"),
    ("rum", "Romanian"),
    ("ron", "Romanian"),
    ("rus", "Russian"),
    ("slo", "Slovak"),
    ("slk", "Slovak"),
    ("slv", "Slovenian"),
    ("som", "Somali"),
    ("spa", "Spanish"),
    ("srp", "Serbian"),
    ("swa", "Swahili"),
    ("swe", "Swedish"),
    ("tam", "Tamil"),
    ("tel", "Telugu"),
    ("tgl", "Tagalog"),
    ("tha", "Thai"),
    ("tib", "Tibetan"),
    ("bod", "Tibetan"),
    ("tur", "Turkish"),
    ("ukr", "Ukrainian"),
    ("urd", "Urdu"),
    ("uzb", "Uzbek"),
    ("vie", "Vietnamese"),
    ("wel", "Welsh"),
    ("cym", "Welsh"),
    ("yid", "Yiddish"),
    ("zul", "Zulu"),
    ("yue", "Cantonese"),
    ("cmn", "Mandarin"),
    ("nan", "Min Nan"),
    ("hat", "Haitian Creole"),
    ("haw", "Hawaiian"),
    ("ibo", "Igbo"),
    ("yor", "Yoruba"),
    ("hau", "Hausa"),
    ("sin", "Sinhala"),
    ("pus", "Pashto"),
    ("tat", "Tatar"),
    ("kir", "Kyrgyz"),
    ("tuk", "Turkmen"),
    ("tgk", "Tajik"),
    ("und", "Unknown"),
    ("mul", "Multiple"),
    ("zxx", "No linguistic content"),
    ("qaa", "Original"),
    ("af", "Afrikaans"),
    ("sq", "Albanian"),
    ("am", "Amharic"),
    ("ar", "Arabic"),
    ("hy", "Armenian"),
    ("az", "Azerbaijani"),
    ("eu", "Basque"),
    ("be", "Belarusian"),
    ("bn", "Bengali"),
    ("bs", "Bosnian"),
    ("bg", "Bulgarian"),
    ("my", "Burmese"),
    ("ca", "Catalan"),
    ("zh", "Chinese"),
    ("cs", "Czech"),
    ("da", "Danish"),
    ("nl", "Dutch"),
    ("en", "English"),
    ("et", "Estonian"),
    ("fi", "Finnish"),
    ("fr", "French"),
    ("ka", "Georgian"),
    ("de", "German"),
    ("ga", "Irish"),
    ("gl", "Galician"),
    ("el", "Greek"),
    ("gu", "Gujarati"),
    ("he", "Hebrew"),
    ("hi", "Hindi"),
    ("hr", "Croatian"),
    ("hu", "Hungarian"),
    ("is", "Icelandic"),
    ("id", "Indonesian"),
    ("it", "Italian"),
    ("ja", "Japanese"),
    ("kn", "Kannada"),
    ("kk", "Kazakh"),
    ("km", "Khmer"),
    ("ko", "Korean"),
    ("ku", "Kurdish"),
    ("lo", "Lao"),
    ("la", "Latin"),
    ("lv", "Latvian"),
    ("lt", "Lithuanian"),
    ("lb", "Luxembourgish"),
    ("mk", "Macedonian"),
    ("ml", "Malayalam"),
    ("mi", "Maori"),
    ("mr", "Marathi"),
    ("ms", "Malay"),
    ("mt", "Maltese"),
    ("mn", "Mongolian"),
    ("ne", "Nepali"),
    ("nb", "Norwegian Bokmål"),
    ("nn", "Norwegian Nynorsk"),
    ("no", "Norwegian"),
    ("fa", "Persian"),
    ("pl", "Polish"),
    ("pt", "Portuguese"),
    ("pa", "Punjabi"),
    ("ro", "Romanian"),
    ("ru", "Russian"),
    ("sk", "Slovak"),
    ("sl", "Slovenian"),
    ("so", "Somali"),
    ("es", "Spanish"),
    ("sr", "Serbian"),
    ("sw", "Swahili"),
    ("sv", "Swedish"),
    ("ta", "Tamil"),
    ("te", "Telugu"),
    ("tl", "Tagalog"),
    ("th", "Thai"),
    ("bo", "Tibetan"),
    ("tr", "Turkish"),
    ("uk", "Ukrainian"),
    ("ur", "Urdu"),
    ("uz", "Uzbek"),
    ("vi", "Vietnamese"),
    ("cy", "Welsh"),
    ("yi", "Yiddish"),
    ("zu", "Zulu"),
];

fn language(code: &str) -> Option<&'static str> {
    LANGUAGES.iter().find(|(c, _)| *c == code).map(|(_, n)| *n)
}

/// A language's two-letter code, as subtitle sites name them: "eng" → "en",
/// "chi" → "zh"; a code without one comes back as it is.
pub fn two_letter_code(code: &str) -> String {
    let base = code.trim().to_lowercase();
    if base.len() == 2 {
        return base;
    }
    let Some(name) = language(&base) else { return base };
    LANGUAGES.iter().find(|(c, n)| c.len() == 2 && *n == name).map_or(base, |(c, _)| c.to_string())
}

/// "eng" → "English", "pt-BR" → "Portuguese (BR)", unknown codes upper-cased; none without one.
pub fn language_name(code: Option<&str>) -> Option<String> {
    let raw = code.unwrap_or("").trim();
    if raw.is_empty() {
        return None;
    }
    let lower = raw.to_lowercase();
    let mut parts = lower.split(['-', '_']);
    let first = parts.next().unwrap_or("");
    let Some(name) = language(first) else {
        return Some(if raw.chars().count() <= 3 { raw.to_uppercase() } else { raw.to_string() });
    };
    Some(match parts.next() {
        Some(region) if !region.is_empty() => format!("{name} ({})", region.to_uppercase()),
        _ => name.to_string(),
    })
}

/// "S1 · E3", "E3", or "" with neither number.
pub fn episode_label(season: Option<i64>, episode: Option<i64>) -> String {
    [season.map(|s| format!("S{s}")), episode.map(|e| format!("E{e}"))]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A season's own title, else "Specials" for season 0, else "Season 2".
pub fn season_title(title: Option<&str>, index: Option<i64>) -> String {
    match (title.filter(|t| !t.is_empty()), index) {
        (Some(t), _) => t.to_string(),
        (None, Some(0)) => "Specials".into(),
        (None, Some(i)) => format!("Season {i}"),
        (None, None) => "Season".into(),
    }
}

/// 12345 → "12,345".
pub fn grouped(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// "1 movie", "12,345 movies" (`many` defaults to `one` + "s").
pub fn pluralize(n: i64, one: &str, many: Option<&str>) -> String {
    let word = if n == 1 { one.to_string() } else { many.map_or_else(|| format!("{one}s"), str::to_string) };
    format!("{} {word}", grouped(n))
}

/// The short line under a card, by kind: a year, "3 seasons", "S2 · E5",
/// "Daft Punk · 2001"…
pub fn card_subtitle(item: &ItemSummary) -> String {
    let year = item.year.map(|y| y.to_string());
    match item.kind {
        ItemKind::Movie => year.unwrap_or_default(),
        ItemKind::Show => match (item.child_count.filter(|c| *c > 0), item.leaf_count) {
            (Some(c), _) => pluralize(c, "season", None),
            (None, Some(l)) => pluralize(l, "episode", None),
            _ => year.unwrap_or_default(),
        },
        ItemKind::Season => item.leaf_count.map(|l| pluralize(l, "episode", None)).unwrap_or_default(),
        ItemKind::Episode => episode_label(item.parent_index, item.index),
        ItemKind::Artist => item.child_count.map(|c| pluralize(c, "album", None)).unwrap_or_default(),
        ItemKind::Album => [item.parent_title.clone().filter(|t| !t.is_empty()), year]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
        ItemKind::Track => [item.grandparent_title.clone(), item.parent_title.clone()]
            .into_iter()
            .flatten()
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join(" · "),
        _ => String::new(),
    }
}

/// How much of an item has been played, 0 to 1.
pub fn progress_fraction(position_ms: i64, duration_ms: Option<i64>) -> f32 {
    match duration_ms {
        Some(d) if d > 0 && position_ms > 0 => (position_ms as f64 / d as f64).clamp(0.0, 1.0) as f32,
        _ => 0.0,
    }
}

/// An item's resume point is worth showing: started, not finished, not marked watched.
pub fn is_in_progress(item: &ItemSummary) -> bool {
    let p = item.user_state.position_ms;
    item.duration_ms.is_some_and(|d| d > 0 && !item.user_state.watched && p > 0 && p < d)
}

/// Episodes of a show or season not watched yet; 0 for other kinds.
pub fn unwatched_count(item: &ItemSummary) -> i64 {
    item.leaf_count.map_or(0, |l| (l - item.viewed_leaf_count.unwrap_or(0)).max(0))
}

fn whole_minutes(ms: i64) -> i64 {
    (ms as f64 / 60_000.0).round() as i64
}

/// 6720000 → "1h 52m", 2700000 → "45m", 29000 → "29s".
pub fn format_duration(ms: Option<i64>) -> String {
    let ms = ms.unwrap_or(0);
    if ms <= 0 {
        return String::new();
    }
    let total = whole_minutes(ms);
    if total < 1 {
        return format!("{}s", ((ms as f64 / 1000.0).round() as i64).max(1));
    }
    let (h, m) = (total / 60, total % 60);
    match (h, m) {
        (0, m) => format!("{m}m"),
        (h, 0) => format!("{h}h"),
        (h, m) => format!("{h}h {m}m"),
    }
}

/// "58 min" under an hour, "1h 52m" from an hour, "29s" below half a minute.
pub fn format_runtime(ms: Option<i64>) -> String {
    let Some(ms) = ms.filter(|m| *m > 0) else { return String::new() };
    let total = whole_minutes(ms);
    if (1..=59).contains(&total) { format!("{total} min") } else { format_duration(Some(ms)) }
}

/// "26 min left", "1h 12m left"; empty without a duration or once nothing is left.
pub fn format_time_left(position_ms: i64, duration_ms: Option<i64>) -> String {
    let Some(d) = duration_ms.filter(|d| *d > 0) else { return String::new() };
    let left = format_runtime(Some((d - position_ms).max(0)));
    if left.is_empty() { left } else { format!("{left} left") }
}

/// Milliseconds as "1:02:03" or "2:03" ("0:00" for nothing).
pub fn format_clock(ms: Option<i64>) -> String {
    let Some(ms) = ms.filter(|m| *m >= 0) else { return "0:00".into() };
    let s = ms / 1000;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{sec:02}") } else { format!("{m}:{sec:02}") }
}

/// A time of day as the TV is set to show it: "21:05" or "9:05 PM".
pub fn format_time_of_day(hours: u32, minutes: u32, hours24: bool) -> String {
    if hours24 {
        return format!("{hours:02}:{minutes:02}");
    }
    let h12 = if hours.is_multiple_of(12) { 12 } else { hours % 12 };
    format!("{h12}:{minutes:02} {}", if hours < 12 { "AM" } else { "PM" })
}

/// 1536 → "1.5 KB", 150 MiB → "150 MB": three significant digits, in units of 1024.
pub fn format_bytes(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else { return String::new() };
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{} {}", fixed(v, if v >= 100.0 || i == 0 { 0 } else { 1 }), UNITS[i])
}

/// Bits per second as "8.2 Mbps", "12 Mbps", "320 kbps"; empty without a rate.
pub fn format_bitrate(bps: Option<i64>) -> String {
    let Some(bps) = bps.filter(|b| *b != 0) else { return String::new() };
    if bps >= 1_000_000 {
        let mbps = bps as f64 / 1_000_000.0;
        return format!("{} Mbps", fixed(mbps, if bps >= 10_000_000 { 0 } else { 1 }));
    }
    format!("{} kbps", (bps as f64 / 1000.0).round() as i64)
}

#[cfg(test)]
mod tests;

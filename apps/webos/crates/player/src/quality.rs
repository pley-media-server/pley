//! The qualities the player offers (docs/QUALITY.md, the same on every app):
//! the original, Automatic, or a conversion capped at a bitrate and a height.
//! Ids are what the server's `auto_quality` names
//! (packages/tv/src/player/quality.ts).

use pley_core::format::{format_bitrate, resolution_label};
use pley_core::model::{MediaFile, PlaybackMode, QUALITY_PRESETS, StreamKind};

use crate::autoquality::Rung;

/// A quality as the menu shows it: a preset of [`QUALITY_PRESETS`], perhaps
/// renamed by the file's own lines ([`named_for`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quality {
    pub id: &'static str,
    pub label: String,
    pub max_bitrate_kbps: Option<u32>,
    pub max_height: Option<u32>,
}

/// The original's id.
pub const ORIGINAL_ID: &str = "original";
/// Automatic's id: the original, lower only when it doesn't play well.
pub const AUTO_ID: &str = "auto";

impl Quality {
    /// The original file's quality.
    pub fn original() -> Quality {
        preset(ORIGINAL_ID).expect("the original is a preset")
    }

    /// Automatic.
    pub fn auto() -> Quality {
        preset(AUTO_ID).expect("Automatic is a preset")
    }

    /// The mode it asks for: the original lets the server pick, the others convert.
    pub fn mode(&self) -> PlaybackMode {
        if self.max_bitrate_kbps.is_none() { PlaybackMode::Auto } else { PlaybackMode::Transcode }
    }

    /// Whether this is a conversion at a bitrate (not the original or Automatic).
    pub fn is_copy(&self) -> bool {
        self.max_bitrate_kbps.is_some() && self.max_height.is_some()
    }
}

/// Every quality in the menu's order: the 4K ones first, then 1080p and below.
pub fn presets() -> Vec<Quality> {
    QUALITY_PRESETS
        .iter()
        .map(|p| Quality {
            id: p.id,
            label: p.label.to_string(),
            max_bitrate_kbps: p.max_bitrate_kbps,
            max_height: p.max_height,
        })
        .collect()
}

/// The quality with `id`.
pub fn preset(id: &str) -> Option<Quality> {
    presets().into_iter().find(|p| p.id == id)
}

fn first_video(file: &MediaFile) -> Option<&pley_core::model::MediaStream> {
    file.streams_of(StreamKind::Video).next()
}

fn height_of(file: Option<&MediaFile>) -> Option<i64> {
    let file = file?;
    file.height.or_else(|| first_video(file).and_then(|v| v.height))
}

fn width_of(file: Option<&MediaFile>) -> Option<i64> {
    let file = file?;
    file.width.or_else(|| first_video(file).and_then(|v| v.width))
}

/// A file's lines, as qualities are named: 2160, 1080, 720 or 480, by its
/// width too (a 1920×800 film is 1080p); none when neither is known.
pub fn lines_of(file: Option<&MediaFile>) -> Option<u32> {
    let w = width_of(file).unwrap_or(0);
    let h = height_of(file).unwrap_or(0);
    if w <= 0 && h <= 0 {
        return None;
    }
    Some(if w >= 3800 || h >= 2100 {
        2160
    } else if w >= 1800 || h >= 1000 {
        1080
    } else if w >= 1200 || h >= 700 {
        720
    } else {
        480
    })
}

/// A copy at more than this share of a file's bitrate, at its lines, plays no better than the file.
const LIGHTER: f64 = 0.8;

fn lines_name(lines: u32) -> String {
    if lines >= 2160 { "4K".to_string() } else { format!("{lines}p") }
}

/// A number as JavaScript prints it: `8`, `4.6`.
pub(crate) fn js_number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v}") }
}

/// `preset` as it is named for `file`: by the file's own lines when the
/// preset is taller than the file ("8 Mbps 720p" for a 720p file, whose copy
/// is no taller), else as the table names it.
pub fn named_for(file: Option<&MediaFile>, preset: &Quality) -> Quality {
    let (Some(kbps), Some(height), Some(lines)) = (preset.max_bitrate_kbps, preset.max_height, lines_of(file)) else {
        return preset.clone();
    };
    if height <= lines {
        return preset.clone();
    }
    Quality { label: format!("{} Mbps {}", js_number(kbps as f64 / 1000.0), lines_name(lines)), ..preset.clone() }
}

/// The qualities worth offering for `file` (docs/QUALITY.md): the original
/// and Automatic, and each preset with fewer lines than the file or at most
/// 80 % of its bitrate (every one when neither is known), named by the
/// file's own lines when taller ([`named_for`]), once for each bitrate and
/// lines, tallest first, then by bitrate.
pub fn qualities_for(file: Option<&MediaFile>) -> Vec<Quality> {
    qualities_in(file, &presets())
}

/// [`qualities_for`] from another table than the presets.
pub fn qualities_in(file: Option<&MediaFile>, table: &[Quality]) -> Vec<Quality> {
    let kbps = file.and_then(|f| f.bitrate).filter(|b| *b != 0).map(|b| b as f64 / 1000.0);
    let height = height_of(file);
    if kbps.is_none() && height.is_none() {
        return table.to_vec();
    }
    let lines = lines_of(file);
    let mut out = Vec::new();
    let mut copies: Vec<(Quality, u32)> = Vec::new();
    let mut seen: Vec<(u32, u32)> = Vec::new();
    for p in table {
        let (Some(p_kbps), Some(p_height)) = (p.max_bitrate_kbps, p.max_height) else {
            out.push(p.clone());
            continue;
        };
        let fewer_lines = height.is_some_and(|h| (p_height as i64) < h);
        let lighter = kbps.is_some_and(|k| p_kbps as f64 <= k * LIGHTER);
        if !fewer_lines && !lighter {
            continue;
        }
        let shown = lines.map_or(p_height, |l| p_height.min(l));
        if seen.contains(&(p_kbps, shown)) {
            continue;
        }
        seen.push((p_kbps, shown));
        copies.push((named_for(file, p), shown));
    }
    copies.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.max_bitrate_kbps.cmp(&a.0.max_bitrate_kbps)));
    out.extend(copies.into_iter().map(|(q, _)| q));
    out
}

/// The quality to start with: the remembered one when this file offers it;
/// the original when it doesn't need that one (an 8 Mbps cap on a 5 Mbps
/// file); Automatic when none was ever chosen.
pub fn choose_quality(remembered: Option<&str>, offered: &[Quality]) -> Quality {
    match remembered.filter(|r| preset(r).is_some()) {
        None => Quality::auto(),
        Some(r) => offered.iter().find(|p| p.id == r).cloned().unwrap_or_else(Quality::original),
    }
}

/// What the server converts the original at (its `closest_quality`): the
/// file's lines, the highest bitrate up to 1.5 times the file's, else that
/// resolution's lowest. Automatic never goes above it.
pub fn server_quality(file: Option<&MediaFile>) -> Quality {
    let lines = lines_of(file).unwrap_or(1080);
    let cap = file.and_then(|f| f.bitrate).filter(|b| *b > 0).map(|b| b as f64 * 1.5 / 1000.0);
    let tier: Vec<Quality> = presets().into_iter().filter(|p| p.max_height == Some(lines)).collect();
    tier.iter()
        .find(|p| cap.is_none_or(|c| p.max_bitrate_kbps.unwrap_or(0) as f64 <= c))
        .or(tier.last())
        .cloned()
        .expect("every line count has presets")
}

/// The qualities Automatic steps through below the original, highest first:
/// those offered for the file, at no more lines and bitrate than the
/// server's own quality for it, each lower in bitrate than the one above
/// (a 4K file goes 30 Mbps 4K, 15 Mbps 4K, then 12 Mbps 1080p).
pub fn auto_ladder(file: Option<&MediaFile>) -> Vec<Rung> {
    let top = server_quality(file);
    let (top_height, top_kbps) = (top.max_height.unwrap_or(0), top.max_bitrate_kbps.unwrap_or(0));
    let mut out = Vec::new();
    let mut below = u32::MAX;
    for p in qualities_for(file) {
        let (Some(kbps), Some(height)) = (p.max_bitrate_kbps, p.max_height) else { continue };
        if height > top_height || kbps > top_kbps || kbps >= below {
            continue;
        }
        out.push(Rung { id: p.id.to_string(), label: p.label.clone(), kbps: kbps as f64, height });
        below = kbps;
    }
    out
}

/// The lines of `file` converted at `preset`: the preset's, or the file's own when fewer.
pub fn stream_lines(file: Option<&MediaFile>, preset: &Quality) -> u32 {
    let lines = lines_of(file);
    let most = preset.max_height.or(lines).unwrap_or(1080);
    lines.map_or(most, |l| most.min(l))
}

/// The least bitrate a converted stream of `lines` keeps when its bitrate
/// changes as it is sent (docs/QUALITY.md, "Players that go black between
/// streams"), kbps: 8 Mbps for 4K, 3 for 1080p, 1.5 for 720p, 1 below.
pub fn live_floor(lines: u32) -> f64 {
    if lines >= 2160 {
        8000.0
    } else if lines >= 1080 {
        3000.0
    } else if lines >= 720 {
        1500.0
    } else {
        1000.0
    }
}

/// "4 Mbps 1080p": the stream of `file` asked for at `preset`, going on at `kbps`.
pub fn live_label(file: Option<&MediaFile>, preset: &Quality, kbps: f64) -> String {
    let lines = stream_lines(file, preset);
    format!("{} Mbps {}", js_number((kbps / 100.0).round() / 10.0), lines_name(lines))
}

/// "1080p · 8.2 Mbps": what a file is, for the Original row's hint.
pub fn file_hint(file: Option<&MediaFile>) -> String {
    let Some(file) = file else { return String::new() };
    let video = first_video(file);
    let resolution =
        resolution_label(file.width.or(video.and_then(|v| v.width)), file.height.or(video.and_then(|v| v.height)));
    let bitrate = format_bitrate(file.bitrate);
    [resolution.map(str::to_string).unwrap_or_default(), bitrate]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(width: i64, height: i64, bitrate: i64) -> MediaFile {
        MediaFile { width: Some(width), height: Some(height), bitrate: Some(bitrate), ..MediaFile::default() }
    }

    fn ids(q: &[Quality]) -> Vec<&str> {
        q.iter().map(|q| q.id).collect()
    }

    #[test]
    fn a_4k_file_offers_every_lighter_copy() {
        let f = file(3840, 2160, 40_000_000);
        assert_eq!(ids(&qualities_for(Some(&f))), ["original", "auto", "30m", "15m", "20m", "12m", "8m", "4m", "2m"]);
        let ladder: Vec<String> = auto_ladder(Some(&f)).into_iter().map(|r| r.id).collect();
        assert_eq!(ladder, ["30m", "15m", "12m", "8m", "4m", "2m"]);
    }

    #[test]
    fn unknown_files_offer_everything() {
        assert_eq!(qualities_for(None).len(), QUALITY_PRESETS.len());
    }

    #[test]
    fn choosing_falls_back_sensibly() {
        let f = file(1920, 1080, 5_000_000);
        let offered = qualities_for(Some(&f));
        assert_eq!(choose_quality(None, &offered).id, "auto");
        assert_eq!(choose_quality(Some("nonsense"), &offered).id, "auto");
        assert_eq!(choose_quality(Some("8m"), &offered).id, "original");
        assert_eq!(choose_quality(Some("2m"), &offered).id, "2m");
    }

    #[test]
    fn floors_and_labels() {
        assert_eq!(live_floor(2160), 8000.0);
        assert_eq!(live_floor(1080), 3000.0);
        assert_eq!(live_floor(720), 1500.0);
        assert_eq!(live_floor(480), 1000.0);
        let f = file(1920, 1080, 10_000_000);
        assert_eq!(live_label(Some(&f), &preset("8m").unwrap(), 3000.0), "3 Mbps 1080p");
        assert_eq!(live_label(Some(&f), &preset("8m").unwrap(), 4560.0), "4.6 Mbps 1080p");
        assert_eq!(server_quality(Some(&f)).id, "12m");
    }
}

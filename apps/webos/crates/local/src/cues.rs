//! A subtitle track read out of a Matroska file on a drive, as the server's
//! transcoder reads one (docs/TRANSCODER.md, "Subtitle extraction"): the
//! Cues list each subtitle block with its cluster and its place there, so
//! the track is read block by block, a seek each, not the whole film. Text
//! becomes WebVTT; pictures go to the app's decoders as they are.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::ebml::{El, children, element, uint, vint};
use crate::matroska::{CLUSTER, CUES};
use crate::{LocalFacts, MatroskaTrack, ReadAt};

const TIMECODE: u64 = 0xE7;
const SIMPLE_BLOCK: u64 = 0xA3;
const BLOCK_GROUP: u64 = 0xA0;
const BLOCK: u64 = 0xA1;
const BLOCK_DURATION: u64 = 0x9B;
const CUE_POINT: u64 = 0xBB;
const CUE_TRACK_POSITIONS: u64 = 0xB7;
const CUE_TRACK: u64 = 0xF7;
const CUE_CLUSTER_POSITION: u64 = 0xF1;
const CUE_RELATIVE_POSITION: u64 = 0xF0;
const CUE_DURATION: u64 = 0xB2;

/// The most of the Cues read: a film's are a few MB at most.
pub const CUES_MOST: usize = 32 * 1024 * 1024;
/// A block read where the Cues put it: a subtitle's is small, a picture's
/// under this.
pub const BLOCK_MOST: u64 = 1024 * 1024;
/// A cluster read whole when the Cues don't say where in it the block is.
pub const CLUSTER_MOST: u64 = 16 * 1024 * 1024;
/// The most a compressed block inflates to.
pub const INFLATED_MOST: usize = 4 * 1024 * 1024;
/// The most cue points kept for a track: a film's subtitles are a few thousand.
const CUE_POINTS_MOST: usize = 100_000;
/// The most of a track's blocks held: a UHD film's PGS is a few tens of MB.
pub const TRACK_MOST: usize = 64 * 1024 * 1024;

/// A subtitle's block: when it shows, for how long when the file says, and
/// its bytes, uncompressed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubtitleBlock {
    pub start_ms: i64,
    pub duration_ms: Option<i64>,
    pub data: Vec<u8>,
}

/// A track's place in the Cues.
struct Cue {
    cluster: u64,
    relative: Option<u64>,
    duration_ms: Option<i64>,
}

/// A block as the cluster holds it: its time from the cluster's, its
/// duration in the file's unit, and its bytes.
struct RawBlock {
    relative: i16,
    duration: Option<u64>,
    data: Vec<u8>,
}

/// `track`'s blocks in time order. Each is passed to `found` as it is read,
/// so what is read shows while the rest is; `cancel` stops the reading with
/// what was read. Empty when the file has no Cues (FFmpeg writes them for
/// video alone) or none for the track.
pub fn read<R: ReadAt + ?Sized>(
    file: &R,
    facts: &LocalFacts,
    track: &MatroskaTrack,
    mut found: impl FnMut(&SubtitleBlock),
    cancel: &AtomicBool,
) -> Vec<SubtitleBlock> {
    let Some(cues_at) = facts.cues_at else { return Vec::new() };
    let mut cues = cues_of(file, cues_at, track.number, facts.scale_ns);
    cues.sort_by_key(|c| (c.cluster, c.relative.unwrap_or(0)));
    let mut blocks: Vec<SubtitleBlock> = Vec::new();
    let mut starts = HashSet::new();
    let mut held = 0usize;
    let mut cluster_times = HashMap::new();
    for cue in cues {
        if cancel.load(Ordering::Relaxed) || held > TRACK_MOST {
            break;
        }
        let Some(cluster_at) = facts.segment_start.checked_add(cue.cluster) else { continue };
        let time = *cluster_times.entry(cluster_at).or_insert_with(|| cluster_time(file, cluster_at));
        let Some(time) = time else { continue };
        let raws = match cue.relative {
            Some(relative) => block_at(file, cluster_at, relative, track.number).into_iter().collect(),
            None => cluster_blocks(file, cluster_at, track.number),
        };
        for raw in raws {
            let Some(start_ms) = ms(i128::from(time) + i128::from(raw.relative), facts.scale_ns) else { continue };
            let duration_ms = match raw.duration {
                Some(d) => ms(i128::from(d), facts.scale_ns),
                None => cue.duration_ms,
            };
            // Cues may name a block twice (one per track position): keep it once.
            if !starts.insert(start_ms) {
                continue;
            }
            let Some(data) = uncompressed(raw.data, track) else { continue };
            held = held.saturating_add(data.len());
            let block = SubtitleBlock { start_ms, duration_ms, data };
            found(&block);
            blocks.push(block);
        }
    }
    blocks.sort_by_key(|b| b.start_ms);
    blocks
}

/// `ticks` of `scale_ns` each, in ms; none past what an i64 holds.
fn ms(ticks: i128, scale_ns: u64) -> Option<i64> {
    i64::try_from(ticks.checked_mul(i128::from(scale_ns))? / 1_000_000).ok()
}

fn cues_of<R: ReadAt + ?Sized>(file: &R, at: u64, track: u64, scale_ns: u64) -> Vec<Cue> {
    let head = file.read_at(at, 16);
    let Some(e) = element(&head, 0, head.len()) else { return Vec::new() };
    let Some(size) = e.size.filter(|_| e.id == CUES) else { return Vec::new() };
    let whole = (e.data_start as u64).saturating_add(size);
    let bytes = file.read_at(at, whole.min(CUES_MOST as u64) as usize);
    let end = bytes.len().min(e.end());
    let mut cues = Vec::new();
    for point in children(&bytes, e.data_start, end).filter(|p| p.id == CUE_POINT) {
        for p in children(&bytes, point.data_start, point.end()).filter(|p| p.id == CUE_TRACK_POSITIONS) {
            if cues.len() >= CUE_POINTS_MOST {
                return cues;
            }
            let (mut number, mut cluster, mut relative, mut duration_ms) = (0, None, None, None);
            for c in children(&bytes, p.data_start, p.end()) {
                match c.id {
                    CUE_TRACK => number = uint(&bytes, &c),
                    CUE_CLUSTER_POSITION => cluster = Some(uint(&bytes, &c)),
                    CUE_RELATIVE_POSITION => relative = Some(uint(&bytes, &c)),
                    CUE_DURATION => duration_ms = ms(i128::from(uint(&bytes, &c)), scale_ns),
                    _ => {}
                }
            }
            if number == track
                && let Some(cluster) = cluster
            {
                cues.push(Cue { cluster, relative, duration_ms });
            }
        }
    }
    cues
}

/// A cluster's timestamp: its first child.
fn cluster_time<R: ReadAt + ?Sized>(file: &R, at: u64) -> Option<u64> {
    let bytes = file.read_at(at, 64);
    let cluster = element(&bytes, 0, bytes.len()).filter(|c| c.id == CLUSTER)?;
    children(&bytes, cluster.data_start, bytes.len()).find(|c| c.id == TIMECODE).map(|c| uint(&bytes, &c))
}

/// The block at `relative` in the cluster's data, when it is the track's.
fn block_at<R: ReadAt + ?Sized>(file: &R, cluster_at: u64, relative: u64, track: u64) -> Option<RawBlock> {
    let head = file.read_at(cluster_at, 16);
    let cluster = element(&head, 0, head.len())?;
    let at = cluster_at.checked_add(cluster.data_start as u64)?.checked_add(relative)?;
    let top = file.read_at(at, 16);
    let e = element(&top, 0, top.len())?;
    let size = e.size.filter(|s| *s <= BLOCK_MOST)?;
    let bytes = file.read_at(at, (e.data_start as u64 + size) as usize);
    parse(&bytes, &element(&bytes, 0, bytes.len())?, track)
}

/// Every block of the track in the cluster, read whole: Cues that don't say
/// where in it.
fn cluster_blocks<R: ReadAt + ?Sized>(file: &R, cluster_at: u64, track: u64) -> Vec<RawBlock> {
    let head = file.read_at(cluster_at, 16);
    let Some(cluster) = element(&head, 0, head.len()) else { return Vec::new() };
    let Some(size) = cluster.size else { return Vec::new() };
    let whole = (cluster.data_start as u64).saturating_add(size).min(CLUSTER_MOST);
    let bytes = file.read_at(cluster_at, whole as usize);
    children(&bytes, cluster.data_start, bytes.len()).filter_map(|c| parse(&bytes, &c, track)).collect()
}

fn parse(b: &[u8], e: &El, track: u64) -> Option<RawBlock> {
    let mut duration = None;
    let block = match e.id {
        SIMPLE_BLOCK => *e,
        BLOCK_GROUP => {
            let mut block = None;
            for c in children(b, e.data_start, e.end()) {
                match c.id {
                    BLOCK => block = Some(c),
                    BLOCK_DURATION => duration = Some(uint(b, &c)),
                    _ => {}
                }
            }
            block?
        }
        _ => return None,
    };
    let end = block.end();
    if end > b.len() {
        return None;
    }
    let (number, length, _) = vint(b, block.data_start, end, false)?;
    if number != track {
        return None;
    }
    // Its time from the cluster's (signed), its flags, then its bytes.
    let at = block.data_start + length;
    let &[high, low, _flags] = b.get(at..at + 3)? else { return None };
    let data = b.get(at + 3..end)?.to_vec();
    Some(RawBlock { relative: i16::from_be_bytes([high, low]), duration, data })
}

/// A block's bytes as the codec has them: zlib undone, a stripped header put
/// back. None when it inflates past [`INFLATED_MOST`].
fn uncompressed(data: Vec<u8>, track: &MatroskaTrack) -> Option<Vec<u8>> {
    match (track.comp_algo, &track.comp_settings) {
        (Some(3), Some(header)) => Some([header.as_slice(), &data].concat()),
        (Some(0), _) if data.len() >= 2 => {
            // zlib: its two-byte header, then deflate.
            let mut out = Vec::new();
            let limit = INFLATED_MOST as u64 + 1;
            match flate2::read::DeflateDecoder::new(&data[2..]).take(limit).read_to_end(&mut out) {
                Ok(_) if out.len() > INFLATED_MOST => None,
                Ok(_) => Some(out),
                // Not zlib after all: as it is, as the server's reader keeps it.
                Err(_) => Some(data),
            }
        }
        _ => Some(data),
    }
}

/// A text track's blocks as WebVTT. ASS and SSA lines lose their fields and
/// styling; a block without a duration ends at the next one, the last after
/// 4 s; empty ones are left out.
pub fn webvtt(blocks: &[SubtitleBlock], codec_id: &str) -> String {
    let mut vtt = String::from("WEBVTT\n\n");
    let ass = codec_id.starts_with("S_TEXT/ASS") || codec_id.starts_with("S_TEXT/SSA");
    for (i, b) in blocks.iter().enumerate() {
        let end = match (b.duration_ms.filter(|d| *d > 0), blocks.get(i + 1)) {
            (Some(d), _) => b.start_ms.saturating_add(d),
            (None, Some(next)) => next.start_ms,
            (None, None) => b.start_ms.saturating_add(4_000),
        };
        let text = String::from_utf8_lossy(&b.data);
        let text = text.trim_end_matches('\0');
        let text = if ass { ass_text(text) } else { text.to_string() };
        let text = text.replace("\r\n", "\n");
        let text = text.trim();
        if text.is_empty() {
            continue;
        }
        let _ = write!(vtt, "{} --> {}\n{text}\n\n", time(b.start_ms), time(end));
    }
    vtt
}

/// An ASS event's text (the ninth field), without its override tags.
fn ass_text(line: &str) -> String {
    let text = line.splitn(9, ',').nth(8).unwrap_or(line);
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    // Each `{…}` goes; an unclosed `{` stays, with what follows it.
    while let Some(open) = rest.find('{') {
        let Some(close) = rest[open..].find('}') else { break };
        out.push_str(&rest[..open]);
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    out.replace("\\N", "\n").replace("\\n", "\n").replace("\\h", " ")
}

fn time(ms: i64) -> String {
    let ms = ms.max(0);
    format!("{:02}:{:02}:{:02}.{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

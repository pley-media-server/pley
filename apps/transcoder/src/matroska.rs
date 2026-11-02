// SPDX-License-Identifier: AGPL-3.0-only
//! Matroska/WebM files read through their index, the Cues element, never
//! cluster by cluster:
//!
//! - the keyframe times of the video (written by mkvmerge and FFmpeg), for
//!   segmenting remuxed video;
//! - the subtitle tracks, text and pictures. mkvmerge and FFmpeg list every
//!   subtitle block in the Cues, with its place in its cluster: only those
//!   blocks are read, a few hundred kilobytes (tens of megabytes for a
//!   Blu-ray's pictures), where extracting the tracks with FFmpeg reads the
//!   whole file (minutes for a big film on a network share). Pictures come
//!   from around where the player is first.
//!
//! Only the segment's metadata elements are read otherwise (seek head, info,
//! tracks, cues, tags), so this takes milliseconds to a second.

use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use pley_transcoder_api::{MediaInfo, StreamKind};

use crate::prefetch::ContainerIndex;
use crate::webvtt::TextCodec;

const EBML: u32 = 0x1A45_DFA3;
const SEGMENT: u32 = 0x1853_8067;
const SEEK_HEAD: u32 = 0x114D_9B74;
const SEEK: u32 = 0x4DBB;
const SEEK_ID: u32 = 0x53AB;
const SEEK_POSITION: u32 = 0x53AC;
const INFO: u32 = 0x1549_A966;
const TIMESTAMP_SCALE: u32 = 0x2A_D7B1;
const WRITING_APP: u32 = 0x5741;
const TRACKS: u32 = 0x1654_AE6B;
const TRACK_ENTRY: u32 = 0xAE;
const TRACK_NUMBER: u32 = 0xD7;
const TRACK_UID: u32 = 0x73C5;
const TRACK_TYPE: u32 = 0x83;
const CODEC_ID: u32 = 0x86;
const CODEC_PRIVATE: u32 = 0x63A2;
const CODEC_DELAY: u32 = 0x56AA;
const DEFAULT_DURATION: u32 = 0x23_E383;
const TRACK_TIMESTAMP_SCALE: u32 = 0x23_314F;
const CONTENT_ENCODINGS: u32 = 0x6D80;
const CONTENT_ENCODING: u32 = 0x6240;
const CONTENT_ENCODING_SCOPE: u32 = 0x5032;
const CONTENT_ENCODING_TYPE: u32 = 0x5033;
const CONTENT_COMPRESSION: u32 = 0x5034;
const CONTENT_COMP_ALGO: u32 = 0x4254;
const CONTENT_COMP_SETTINGS: u32 = 0x4255;
const CUES: u32 = 0x1C53_BB6B;
const CUE_POINT: u32 = 0xBB;
const CUE_TIME: u32 = 0xB3;
const CUE_TRACK_POSITIONS: u32 = 0xB7;
const CUE_TRACK: u32 = 0xF7;
const CUE_CLUSTER_POSITION: u32 = 0xF1;
const CUE_RELATIVE_POSITION: u32 = 0xF0;
const CUE_DURATION: u32 = 0xB2;
const CLUSTER: u32 = 0x1F43_B675;
const CLUSTER_TIMESTAMP: u32 = 0xE7;
const SIMPLE_BLOCK: u32 = 0xA3;
const BLOCK_GROUP: u32 = 0xA0;
const BLOCK: u32 = 0xA1;
const BLOCK_DURATION: u32 = 0x9B;
const TAGS: u32 = 0x1254_C367;
const TAG: u32 = 0x7373;
const TARGETS: u32 = 0x63C0;
const TAG_TRACK_UID: u32 = 0x63C5;
const SIMPLE_TAG: u32 = 0x67C8;
const TAG_NAME: u32 = 0x45A3;
const TAG_STRING: u32 = 0x4487;
const ATTACHMENTS: u32 = 0x1941_A469;
const ATTACHED_FILE: u32 = 0x61A7;
const FILE_NAME: u32 = 0x466E;
const FILE_MIME_TYPE: u32 = 0x4660;
const FILE_DATA: u32 = 0x465C;

const UNKNOWN_SIZE: u64 = u64::MAX;
const MAX_ELEMENT_READ: u64 = 64 * 1024 * 1024;
const MAX_TOP_LEVEL_ELEMENTS: usize = 50_000;
/// The most seek heads read: the first, and those it points to (a muxer may
/// write a second one at the end of the file, for the Cues).
const MAX_SEEK_HEADS: usize = 4;
/// What a block may inflate to here. FFmpeg gives up on a block a little past
/// 10 MB (how far past depends on the block's size) and drops it, so a file
/// with bigger ones is left to it.
const MAX_INFLATED: u64 = 10_000_000;
/// The largest block read for a subtitle: a line takes bytes, an ASS drawing
/// kilobytes (and FFmpeg can't write a cue past 1 MiB).
const MAX_BLOCK: u64 = 1024 * 1024;
/// The most streams FFmpeg makes of a file (its `max_streams`): it can't open
/// a file with more tracks, so neither is one read here.
const MAX_STREAMS: usize = 1_000;
/// What each block the Cues list for a track costs a read through them,
/// besides its data: the records kept of it along the way, from its cue to
/// its WebVTT cue (some 200 bytes at most at any time). It is taken out of
/// the budget as the Cues are read, so a file listing millions of tiny blocks
/// runs out of it before any block is read.
const ENTRY_COST: u64 = 256;
/// How many cue points and positions are read between checks of the time a
/// read has left.
const CUES_BETWEEN_CHECKS: usize = 4096;

/// Keyframe times (ms) of the first video track, or `None` when the file is not
/// Matroska or has no usable Cues.
pub fn cue_keyframes(path: &Path) -> Option<Vec<i64>> {
    let mut file = BufReader::with_capacity(64 * 1024, File::open(path).ok()?);
    let layout = layout(&mut file, true, None).ok()?;
    let scale = layout.timestamp_scale(&mut file);
    let video_track =
        layout.tracks.and_then(|p| child_payload(&mut file, p, TRACKS)).and_then(|d| first_video_track(&d))?;
    let cues = child_payload(&mut file, layout.cues?, CUES)?;
    let mut times: Vec<i64> = cue_times(&cues, video_track).into_iter().map(|(t, _)| ticks_to_ms(t, scale)).collect();
    times.sort_unstable();
    times.dedup();
    (times.len() >= 2).then_some(times)
}

/// Where a file's Cues are (the bytes of the element) and where its video
/// keyframes are: `(time in ms, offset of the cluster that holds it)`, sorted
/// by time. For reading ahead of a playback, so only what the seek heads and
/// the elements before the first cluster point to is looked at: `None` when
/// that doesn't find the Cues (a search past the clusters would read the
/// whole file), or the file isn't Matroska.
pub fn cue_index(path: &Path) -> Option<ContainerIndex> {
    let mut file = BufReader::with_capacity(64 * 1024, File::open(path).ok()?);
    let layout = layout(&mut file, false, None).ok()?;
    let scale = layout.timestamp_scale(&mut file);
    let video_track =
        layout.tracks.and_then(|p| child_payload(&mut file, p, TRACKS)).and_then(|d| first_video_track(&d))?;
    let at = layout.cues?;
    let cues = child_payload(&mut file, at, CUES)?;
    let end = file.stream_position().ok()?;
    let mut points: Vec<(i64, u64)> = cue_times(&cues, video_track)
        .into_iter()
        .filter_map(|(time, cluster)| Some((ticks_to_ms(time, scale), layout.segment.checked_add(cluster?)?)))
        .collect();
    points.sort_unstable();
    points.dedup_by_key(|p| p.0);
    Some((at..end, points))
}

/// Milliseconds of `ticks` of `scale` nanoseconds.
fn ticks_to_ms(ticks: u64, scale: u64) -> i64 {
    (ticks as u128 * scale as u128 / 1_000_000) as i64
}

/// Where the segment's metadata elements are.
struct Layout {
    /// Where the segment's data starts: positions in the seek head and the
    /// Cues count from here.
    segment: u64,
    info: Option<u64>,
    tracks: Option<u64>,
    cues: Option<u64>,
    tags: Option<u64>,
    attachments: Option<u64>,
}

impl Layout {
    /// Nanoseconds per tick of the file's timestamps.
    fn timestamp_scale<R: Read + Seek>(&self, file: &mut R) -> u64 {
        self.info.and_then(|p| child_payload(file, p, INFO)).map_or(1_000_000, |info| timestamp_scale(&info))
    }

    /// Takes the places a seek head gives for what isn't found yet, then
    /// those the seek heads it points to give, reading each once
    /// ([`MAX_SEEK_HEADS`] in all, where they are kept in `read`), within
    /// `budget` when there's one.
    fn note_seek_head<R: Read + Seek>(
        &mut self,
        file: &mut R,
        seek_head: &[u8],
        read: &mut Vec<u64>,
        budget: Option<&Budget>,
    ) -> Result<(), String> {
        let mut pointed = Vec::new();
        self.note(seek_head, &mut pointed);
        while let Some(at) = pointed.pop() {
            if read.contains(&at) || read.len() >= MAX_SEEK_HEADS {
                continue;
            }
            read.push(at);
            let seek_head = match budget {
                Some(budget) => budgeted_payload(file, Some(at), SEEK_HEAD, budget)?,
                None => child_payload(file, at, SEEK_HEAD),
            };
            if let Some(seek_head) = seek_head {
                self.note(&seek_head, &mut pointed);
                if let Some(budget) = budget {
                    budget.refund(seek_head.len() as u64);
                }
            }
        }
        Ok(())
    }

    /// Takes the places a seek head gives for what isn't found yet, and
    /// those of the seek heads it points to in `more` (a few at most).
    fn note(&mut self, seek_head: &[u8], more: &mut Vec<u64>) {
        for (target, rel) in seek_entries(seek_head) {
            let Some(abs) = self.segment.checked_add(rel) else { continue };
            match target {
                INFO => self.info = self.info.or(Some(abs)),
                TRACKS => self.tracks = self.tracks.or(Some(abs)),
                CUES => self.cues = self.cues.or(Some(abs)),
                TAGS => self.tags = self.tags.or(Some(abs)),
                ATTACHMENTS => self.attachments = self.attachments.or(Some(abs)),
                SEEK_HEAD if more.len() < MAX_SEEK_HEADS => more.push(abs),
                _ => {}
            }
        }
    }
}

/// Nanoseconds per tick of the timestamps of a file with this info.
fn timestamp_scale(info: &[u8]) -> u64 {
    find_uint(info, TIMESTAMP_SCALE).filter(|&s| s > 0).unwrap_or(1_000_000)
}

/// The positions of the segment's info, tracks, cues, tags and attachments: from its seek
/// heads (the first, and those it points to) and the elements before its
/// first cluster. When `past_clusters`, from the elements after the clusters
/// too if the Cues aren't found before them, which means going through the
/// clusters one by one (as long as reading the file, on a network share).
/// With a `budget`, the seek heads read are taken out of it, and the search
/// stops with an error once it runs out of time. An error when the file isn't
/// Matroska.
fn layout<R: Read + Seek>(file: &mut R, past_clusters: bool, budget: Option<&Budget>) -> Result<Layout, String> {
    const NOT_MATROSKA: &str = "not a Matroska file";
    let io = |e: std::io::Error| e.to_string();
    file.seek(SeekFrom::Start(0)).map_err(io)?;
    let (id, size) = element_header(file).ok_or(NOT_MATROSKA)?;
    if id != EBML || size == UNKNOWN_SIZE {
        return Err(NOT_MATROSKA.into());
    }
    file.seek(SeekFrom::Current(i64::try_from(size).map_err(|_| NOT_MATROSKA)?)).map_err(io)?;
    let (id, segment_size) = element_header(file).ok_or(NOT_MATROSKA)?;
    if id != SEGMENT {
        return Err(NOT_MATROSKA.into());
    }
    let segment = file.stream_position().map_err(io)?;
    let segment_end = if segment_size == UNKNOWN_SIZE { u64::MAX } else { segment.saturating_add(segment_size) };

    let mut layout = Layout { segment, info: None, tracks: None, cues: None, tags: None, attachments: None };
    // Where the seek heads read are.
    let mut seek_heads = Vec::new();
    let mut pos = segment;
    for _ in 0..MAX_TOP_LEVEL_ELEMENTS {
        if let Some(budget) = budget {
            budget.check()?;
        }
        let found = layout.info.is_some() && layout.tracks.is_some() && layout.cues.is_some();
        if pos >= segment_end || (found && layout.tags.is_some() && layout.attachments.is_some()) {
            break;
        }
        file.seek(SeekFrom::Start(pos)).map_err(io)?;
        let Some((id, size)) = element_header(file) else { break };
        let data = file.stream_position().map_err(io)?;
        match id {
            SEEK_HEAD if seek_heads.len() < MAX_SEEK_HEADS && !seek_heads.contains(&pos) => {
                if let Some(budget) = budget.filter(|_| size <= MAX_ELEMENT_READ) {
                    budget.spend(size)?;
                }
                let seek_head = read_payload(file, size).ok_or(NOT_MATROSKA)?;
                seek_heads.push(pos);
                layout.note_seek_head(file, &seek_head, &mut seek_heads, budget)?;
                if let Some(budget) = budget {
                    budget.refund(size);
                }
            }
            INFO => layout.info = Some(pos),
            TRACKS => layout.tracks = Some(pos),
            CUES => layout.cues = Some(pos),
            TAGS => layout.tags = layout.tags.or(Some(pos)),
            ATTACHMENTS => layout.attachments = layout.attachments.or(Some(pos)),
            CLUSTER if !past_clusters || (layout.cues.is_some() && layout.tracks.is_some()) => break,
            _ => {}
        }
        match data.checked_add(size) {
            Some(next) if size != UNKNOWN_SIZE => pos = next,
            _ => break,
        }
    }
    Ok(layout)
}

// ------------------------------------------------------------------ attached fonts

/// A font attached to the file: ASS subtitles name the fonts they are drawn
/// with, and muxers attach those that aren't common.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Font {
    pub name: String,
    pub data: Vec<u8>,
}

/// The most fonts taken from one file: a film's subtitles use a few dozen,
/// and libass opens every file of the folder they are put in, so a file
/// with hundreds of thousands of tiny ones would be read at every burn-in.
pub const MAX_FONTS: usize = 1000;

/// The fonts attached to a Matroska file, `max_bytes` of them and
/// [`MAX_FONTS`] at most (in the order they come): only the Attachments element is read, which the
/// seek head points to, or which comes before the first cluster (mkvmerge
/// and FFmpeg write it there). An empty list when there are none; an error
/// when the file isn't Matroska or can't be read. Blocking.
pub fn attached_fonts(path: &Path, max_bytes: u64) -> Result<Vec<Font>, String> {
    let mut file = BufReader::with_capacity(64 * 1024, File::open(path).map_err(|e| e.to_string())?);
    let layout = layout(&mut file, false, None)?;
    let Some(at) = layout.attachments else { return Ok(Vec::new()) };
    let io = |e: std::io::Error| e.to_string();
    file.seek(SeekFrom::Start(at)).map_err(io)?;
    match element_header(&mut file) {
        Some((ATTACHMENTS, size)) if size != UNKNOWN_SIZE => {
            let start = file.stream_position().map_err(io)?;
            let end = start.saturating_add(size);
            let mut fonts = Vec::new();
            let mut total = 0u64;
            let mut pos = start;
            // Each attached file is read whole, one at a time.
            while pos < end && fonts.len() < MAX_FONTS {
                file.seek(SeekFrom::Start(pos)).map_err(io)?;
                let Some((id, size)) = element_header(&mut file) else { break };
                let data = file.stream_position().map_err(io)?;
                if size == UNKNOWN_SIZE {
                    break;
                }
                // One that can't fit isn't read (its data is a little smaller).
                if id == ATTACHED_FILE
                    && size <= max_bytes.saturating_sub(total).saturating_add(1024)
                    && let Some(font) = read_payload(&mut file, size).and_then(|entry| font(&entry))
                    && total + font.data.len() as u64 <= max_bytes
                {
                    total += font.data.len() as u64;
                    fonts.push(font);
                }
                pos = data.saturating_add(size);
            }
            Ok(fonts)
        }
        _ => Ok(Vec::new()),
    }
}

/// The font an AttachedFile element holds, if it is one: by its MIME type,
/// or its name's extension (some muxers say `application/octet-stream`).
fn font(entry: &[u8]) -> Option<Font> {
    let text = |id| {
        children(entry).find(|(child, _)| *child == id).map(|(_, p)| String::from_utf8_lossy(trim_nul(p)).into_owned())
    };
    let name = text(FILE_NAME).unwrap_or_default();
    let mime = text(FILE_MIME_TYPE).unwrap_or_default().to_ascii_lowercase();
    let extension = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let by_mime = mime.starts_with("font/")
        || matches!(
            mime.as_str(),
            "application/x-truetype-font"
                | "application/x-font-ttf"
                | "application/x-font-otf"
                | "application/x-font-opentype"
                | "application/x-font"
                | "application/font-sfnt"
                | "application/vnd.ms-opentype"
        );
    if !by_mime && !matches!(extension.as_str(), "ttf" | "otf" | "ttc" | "otc") {
        return None;
    }
    let data = children(entry).find(|(child, _)| *child == FILE_DATA)?.1.to_vec();
    (!data.is_empty()).then_some(Font { name, data })
}

// ------------------------------------------------------------------ text subtitles

/// What a read through the Cues may spend: time, and memory for what it
/// keeps: the file's metadata elements it reads (seek heads, info, tracks,
/// Cues, tags), the records of each block the Cues list for the tracks it
/// reads ([`ENTRY_COST`]), the blocks and what they inflate to, and the WebVTT
/// made of them. Past either, it stops with an error: FFmpeg reads the file
/// then, a packet at a time, in a process of its own.
pub struct Budget {
    bytes: AtomicU64,
    started: Instant,
    /// The time it may take, in milliseconds from `started`.
    time_ms: AtomicU64,
}

impl Budget {
    pub fn new(bytes: u64, time: Duration) -> Self {
        Self { bytes: AtomicU64::new(bytes), started: Instant::now(), time_ms: AtomicU64::new(time.as_millis() as u64) }
    }

    /// Whether the read has time left.
    pub fn check(&self) -> Result<(), String> {
        if self.started.elapsed().as_millis() as u64 > self.time_ms.load(Ordering::Relaxed) {
            Err("reading through the cues took too long".into())
        } else {
            Ok(())
        }
    }

    /// Gives the read `time` more, for time spent waiting rather than
    /// reading.
    pub fn extend(&self, time: Duration) {
        self.time_ms.fetch_add(time.as_millis() as u64, Ordering::Relaxed);
    }

    /// Takes `bytes` of memory out of the budget; once it runs out, nothing
    /// more can be taken.
    pub fn spend(&self, bytes: u64) -> Result<(), String> {
        let mut left = self.bytes.load(Ordering::Relaxed);
        while let Some(rest) = left.checked_sub(bytes) {
            match self.bytes.compare_exchange_weak(left, rest, Ordering::Relaxed, Ordering::Relaxed) {
                Ok(_) => return Ok(()),
                Err(now) => left = now,
            }
        }
        self.bytes.store(0, Ordering::Relaxed);
        Err("the subtitles take more memory than a read through the cues may".into())
    }

    /// Gives back memory no longer held.
    fn refund(&self, bytes: u64) {
        self.bytes.fetch_add(bytes, Ordering::Relaxed);
    }
}

/// Subtitle tracks of a file, block by block: text ([`TextTracks`]), or
/// pictures ([`PictureTracks`]), by their codec of kind `C`.
#[derive(Debug)]
pub struct SubtitleTracks<C> {
    /// Nanoseconds per tick of the block times.
    pub tick_ns: u64,
    /// Whether every stream's times are whole milliseconds (ticks of whole
    /// milliseconds, no track with a timestamp scale of its own): then so is
    /// the file's start time, as FFmpeg takes it.
    pub whole_ms: bool,
    pub tracks: Vec<SubtitleTrack<C>>,
    /// What reading the blocks took: reads, and bytes (the Cues aside).
    pub reads: usize,
    pub bytes_read: u64,
    /// Whether more stretches of blocks come after these (see
    /// [`picture_tracks`]).
    pub more: bool,
    /// The span of the file's time (ticks, from and until) whose blocks
    /// these are all of, when the Cues tell it: their times are the blocks'
    /// own.
    pub span: Option<(u64, u64)>,
}

#[derive(Debug)]
pub struct SubtitleTrack<C> {
    /// FFmpeg's index of the stream.
    pub stream_index: i64,
    pub codec: C,
    /// The track's codec private data (an ASS track's script header, a
    /// VobSub track's palette).
    pub header: Vec<u8>,
    /// In the order FFmpeg reads them: the order of the file.
    pub blocks: Vec<TextBlock>,
}

/// The text subtitle tracks of a file.
pub type TextTracks = SubtitleTracks<TextCodec>;
/// The picture subtitle tracks of a file, by their Matroska codec ID
/// (`S_HDMV/PGS`, `S_VOBSUB`, `S_DVBSUB`).
pub type PictureTracks = SubtitleTracks<&'static str>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextBlock {
    /// When it starts, in ticks.
    pub time: i64,
    /// How long it shows, in ticks (0 when the file doesn't say).
    pub duration: u64,
    pub data: Vec<u8>,
}

/// A track's entry in the Tracks element, its data where the element is.
#[derive(Debug, Default)]
struct Track<'a> {
    number: u64,
    uid: u64,
    kind: u64,
    /// As FFmpeg takes it: up to its first NUL.
    codec_id: &'a [u8],
    codec_private: &'a [u8],
    codec_delay: u64,
    default_duration: u64,
    /// The deprecated per-track timestamp scale, when it isn't 1.
    scaled: bool,
    /// Two at most: a track with more isn't read here, as with two.
    encodings: Vec<Encoding<'a>>,
}

#[derive(Debug, Clone)]
struct Encoding<'a> {
    /// 1: the blocks, 2: the codec private data.
    scope: u64,
    /// 0: compression, 1: encryption.
    kind: u64,
    /// 0: zlib, 1: bzlib, 2: lzo, 3: header stripping.
    algorithm: u64,
    settings: &'a [u8],
}

/// Why data couldn't be decoded.
#[derive(Debug, PartialEq)]
enum Undecoded {
    /// FFmpeg can't decode it either.
    Corrupt(String),
    /// More than is decoded here (FFmpeg may still take it).
    Refused(String),
}

impl From<Undecoded> for String {
    fn from(err: Undecoded) -> Self {
        match err {
            Undecoded::Corrupt(message) | Undecoded::Refused(message) => message,
        }
    }
}

impl Encoding<'_> {
    /// Undo the encoding of `data`, taking what that makes out of `budget`.
    fn decode(&self, data: &[u8], budget: &Budget) -> Result<Vec<u8>, Undecoded> {
        match self.algorithm {
            0 => {
                let out = inflate(data, MAX_INFLATED as usize)?;
                budget.spend(out.len() as u64).map_err(Undecoded::Refused)?;
                Ok(out)
            }
            3 => {
                budget.spend((self.settings.len() + data.len()) as u64).map_err(Undecoded::Refused)?;
                Ok([self.settings, data].concat())
            }
            other => Err(Undecoded::Refused(format!("compression {other} isn't supported"))),
        }
    }
}

/// zlib data inflated as FFmpeg does it: the stream must end, with a
/// checksum that agrees, and make at most `cap` bytes.
fn inflate(data: &[u8], cap: usize) -> Result<Vec<u8>, Undecoded> {
    let mut zlib = flate2::Decompress::new(true);
    let mut out: Vec<u8> = Vec::new();
    loop {
        if out.len() > cap {
            return Err(Undecoded::Refused(format!("a block inflates to more than {cap} bytes")));
        }
        if out.len() == out.capacity() {
            // As FFmpeg, three times the input to start with, then more.
            out.reserve_exact((data.len() * 3).max(out.len()).max(64).min(cap + 1 - out.len()));
        }
        let (read, written) = (zlib.total_in(), zlib.total_out());
        let input = data.get(read as usize..).unwrap_or_default();
        let status = zlib
            .decompress_vec(input, &mut out, flate2::FlushDecompress::None)
            .map_err(|e| Undecoded::Corrupt(format!("a block doesn't inflate: {e}")))?;
        if status == flate2::Status::StreamEnd {
            if out.len() > cap {
                return Err(Undecoded::Refused(format!("a block inflates to more than {cap} bytes")));
            }
            return Ok(out);
        }
        if (zlib.total_in(), zlib.total_out()) == (read, written) {
            return Err(Undecoded::Corrupt("a block's compressed data stops short".into()));
        }
    }
}

/// FFmpeg's name for the codec of a subtitle track (from `ff_mkv_codec_tags`).
fn subtitle_codec_name(codec_id: &[u8]) -> Option<&'static str> {
    const NAMES: [(&str, &str); 15] = [
        ("D_WEBVTT/SUBTITLES", "webvtt"),
        ("D_WEBVTT/CAPTIONS", "webvtt"),
        ("D_WEBVTT/DESCRIPTIONS", "webvtt"),
        ("D_WEBVTT/METADATA", "webvtt"),
        ("S_TEXT/UTF8", "subrip"),
        ("S_TEXT/ASCII", "text"),
        ("S_TEXT/ASS", "ass"),
        ("S_TEXT/SSA", "ass"),
        ("S_ASS", "ass"),
        ("S_SSA", "ass"),
        ("S_VOBSUB", "dvd_subtitle"),
        ("S_DVBSUB", "dvb_subtitle"),
        ("S_HDMV/PGS", "hdmv_pgs_subtitle"),
        ("S_HDMV/TEXTST", "hdmv_text_subtitle"),
        ("S_ARIBSUB", "arib_caption"),
    ];
    NAMES.iter().find(|(id, _)| codec_id.starts_with(id.as_bytes())).map(|(_, name)| *name)
}

fn text_codec(codec_id: &[u8]) -> Option<TextCodec> {
    match subtitle_codec_name(codec_id)? {
        "subrip" => Some(TextCodec::Subrip),
        "ass" => Some(TextCodec::Ass),
        "text" => Some(TextCodec::Text),
        "webvtt" => Some(TextCodec::Webvtt),
        _ => None,
    }
}

/// The Matroska codec ID of a picture subtitle track's codec, as FFmpeg
/// names it (what a Matroska file of the track says).
pub fn picture_codec_id(ffmpeg_name: &str) -> Option<&'static str> {
    match ffmpeg_name {
        "hdmv_pgs_subtitle" => Some("S_HDMV/PGS"),
        "dvd_subtitle" => Some("S_VOBSUB"),
        "dvb_subtitle" => Some("S_DVBSUB"),
        _ => None,
    }
}

fn picture_codec(codec_id: &[u8]) -> Option<&'static str> {
    picture_codec_id(subtitle_codec_name(codec_id)?)
}

impl<'a> Track<'a> {
    fn parse(entry: &'a [u8]) -> Self {
        let mut track = Track::default();
        for (id, value) in children(entry) {
            match id {
                TRACK_NUMBER => track.number = uint(value).unwrap_or(0),
                TRACK_UID => track.uid = uint(value).unwrap_or(0),
                TRACK_TYPE => track.kind = uint(value).unwrap_or(0),
                CODEC_ID => track.codec_id = value.split(|&b| b == 0).next().unwrap_or_default(),
                CODEC_PRIVATE => track.codec_private = value,
                CODEC_DELAY => track.codec_delay = uint(value).unwrap_or(0),
                DEFAULT_DURATION => track.default_duration = uint(value).unwrap_or(0),
                TRACK_TIMESTAMP_SCALE => track.scaled = float(value).is_some_and(|scale| scale != 1.0),
                CONTENT_ENCODINGS => {
                    for (id, encoding) in children(value) {
                        if id != CONTENT_ENCODING || track.encodings.len() == 2 {
                            continue;
                        }
                        let compression = children(encoding).find(|(id, _)| *id == CONTENT_COMPRESSION).map(|(_, c)| c);
                        track.encodings.push(Encoding {
                            scope: find_uint(encoding, CONTENT_ENCODING_SCOPE).unwrap_or(1),
                            kind: find_uint(encoding, CONTENT_ENCODING_TYPE).unwrap_or(0),
                            algorithm: compression.and_then(|c| find_uint(c, CONTENT_COMP_ALGO)).unwrap_or(0),
                            settings: compression
                                .and_then(|c| children(c).find(|(id, _)| *id == CONTENT_COMP_SETTINGS))
                                .map_or(&[], |(_, s)| s),
                        });
                    }
                }
                _ => {}
            }
        }
        track
    }

    /// Whether FFmpeg makes a stream of this track: tracks of other kinds,
    /// or whose codec doesn't fit their kind, are left out and don't count.
    fn has_stream(&self) -> bool {
        let first = self.codec_id.first().copied();
        match self.kind {
            1 => first == Some(b'V'),
            2 => first == Some(b'A'),
            17 | 33 => matches!(first, Some(b'D' | b'S')),
            _ => false,
        }
    }

    /// The kind of stream FFmpeg makes of it.
    fn stream_kind(&self) -> Option<StreamKind> {
        match self.kind {
            1 => Some(StreamKind::Video),
            2 => Some(StreamKind::Audio),
            17 => Some(StreamKind::Subtitle),
            33 if subtitle_codec_name(self.codec_id) == Some("webvtt") => Some(StreamKind::Subtitle),
            _ => None,
        }
    }
}

/// The text subtitle streams `wanted` (FFmpeg's stream indexes, as in
/// `media`) of a Matroska file, read through its Cues within `budget`; an
/// error says why the file doesn't allow it (FFmpeg has to read the file
/// then): it isn't Matroska, its Cues aren't found in its seek heads or
/// before its clusters, its tracks don't line up with `media`'s streams, its
/// Cues don't list every block of the tracks, its blocks aren't where the
/// Cues say, it uses something not supported here, or it needs more than the
/// budget.
pub fn text_tracks(path: &Path, media: &MediaInfo, wanted: &[i64], budget: &Budget) -> Result<TextTracks, String> {
    let kind = Kind { codec: text_codec, name: "text", webm: |c| c == TextCodec::Webvtt };
    let mut whole = None;
    subtitle_tracks(path, media, wanted, budget, kind, Order::WHOLE, |tracks| {
        whole = Some(tracks);
        Ok(())
    })?;
    whole.ok_or_else(|| "no track read".into())
}

/// The picture subtitle streams `wanted` of a Matroska file, read through
/// its Cues within `budget`, as [`text_tracks`] reads text (their blocks as
/// they are, compression undone), and handed to `got` in stretches: first
/// the blocks from `from_ms` to `until_ms` (the file's time, as FFmpeg takes
/// it), then those after, then those before. Each stretch's blocks are in
/// the order of the file; an error `got` returns ends the read.
pub fn picture_tracks(
    path: &Path,
    media: &MediaInfo,
    wanted: &[i64],
    budget: &Budget,
    (from_ms, until_ms): (i64, i64),
    got: impl FnMut(PictureTracks) -> Result<(), String>,
) -> Result<(), String> {
    let kind = Kind { codec: picture_codec, name: "pictures", webm: |_| false };
    subtitle_tracks(path, media, wanted, budget, kind, Order { from_ms, until_ms }, got)
}

/// The blocks a read through the Cues hands over first: those from
/// `from_ms` to `until_ms` of the file's time (as FFmpeg takes it).
#[derive(Clone, Copy)]
struct Order {
    from_ms: i64,
    until_ms: i64,
}

impl Order {
    /// Every block at once.
    const WHOLE: Order = Order { from_ms: 0, until_ms: i64::MAX };
}

/// The subtitle tracks a read through the Cues takes: their codec (`None`
/// for a track of another kind), the kind's name, and whether a codec's
/// blocks are laid out as WebM's WebVTT.
struct Kind<C> {
    codec: fn(&[u8]) -> Option<C>,
    name: &'static str,
    webm: fn(C) -> bool,
}

fn subtitle_tracks<C: Copy>(
    path: &Path,
    media: &MediaInfo,
    wanted: &[i64],
    budget: &Budget,
    kind: Kind<C>,
    order: Order,
    mut got: impl FnMut(SubtitleTracks<C>) -> Result<(), String>,
) -> Result<(), String> {
    let mut file = BufReader::with_capacity(64 * 1024, File::open(path).map_err(|e| e.to_string())?);
    // mkvmerge and FFmpeg list the Cues in the seek head, or put them before
    // the clusters: another file's aren't looked for cluster by cluster,
    // which would take as long as reading it.
    let layout = layout(&mut file, false, Some(budget))?;
    let info = budgeted_payload(&mut file, layout.info, INFO, budget)?;
    let tick_ns = info.as_deref().map_or(1_000_000, timestamp_scale);
    let writing_app =
        info.as_deref().and_then(|info| children(info).find(|(id, _)| *id == WRITING_APP)).map(|(_, app)| app);

    let tracks = budgeted_payload(&mut file, layout.tracks, TRACKS, budget)?.ok_or("no tracks")?;
    // FFmpeg numbers the streams in the order of the tracks it keeps.
    let streams: Vec<Track> = children(&tracks)
        .filter(|(id, _)| *id == TRACK_ENTRY)
        .map(|(_, entry)| Track::parse(entry))
        .filter(Track::has_stream)
        .take(MAX_STREAMS + 1)
        .collect();
    if streams.len() > MAX_STREAMS {
        return Err(format!("the file has more than {MAX_STREAMS} streams"));
    }
    for stream in &media.streams {
        let track = usize::try_from(stream.index).ok().and_then(|i| streams.get(i));
        let lines_up = track.is_some_and(|t| {
            t.stream_kind() == Some(stream.kind)
                && (stream.kind != StreamKind::Subtitle
                    || subtitle_codec_name(t.codec_id).is_none_or(|name| name == stream.codec))
        });
        if !lines_up {
            return Err(format!("stream {} doesn't line up with the file's tracks", stream.index));
        }
    }

    let mut text = Vec::new();
    for &index in wanted {
        let track = usize::try_from(index).ok().and_then(|i| streams.get(i)).ok_or("no such stream")?;
        let codec = (kind.codec)(track.codec_id).ok_or_else(|| format!("stream {index} isn't {}", kind.name))?;
        if track.scaled || track.codec_delay != 0 {
            return Err(format!("stream {index} has a timing of its own"));
        }
        match track.encodings.as_slice() {
            [] => {}
            [encoding] if encoding.kind == 0 && matches!(encoding.algorithm, 0 | 3) => {}
            _ => return Err(format!("stream {index} is encrypted or encoded in a way not supported here")),
        }
        text.push((index, codec, track));
    }
    let slot = |number: u64| text.iter().position(|(_, _, t)| t.number == number).unwrap_or_default();

    let cues = budgeted_payload(&mut file, layout.cues, CUES, budget)?.ok_or("no cues")?;
    let numbers: Vec<u64> = text.iter().map(|(_, _, t)| t.number).collect();
    let mut entries = cue_entries(&cues, &numbers, budget)?;
    if entries.iter().any(|e| e.relative.is_none()) {
        return Err("the cues don't say where blocks are in their clusters".into());
    }
    // A track without entries is empty, unless the file lists no subtitles
    // at all: its muxer doesn't put them in the Cues.
    if numbers.iter().any(|n| !entries.iter().any(|e| e.track == *n)) {
        let subtitles: Vec<u64> = streams.iter().filter(|t| t.kind == 17).map(|t| t.number).collect();
        if !lists_any(&cues, &subtitles) {
            return Err("the cues list no subtitles".into());
        }
    }
    budget.refund(cues.len() as u64);
    drop(cues);
    // mkvmerge counts each track's blocks in its statistics tags: when they
    // are its own (not copied over by a later muxer), the Cues must list
    // them all.
    let uids: Vec<u64> = text.iter().map(|(_, _, t)| t.uid).collect();
    let counts = match budgeted_payload(&mut file, layout.tags, TAGS, budget)? {
        Some(tags) => {
            let counts = block_counts(&tags, writing_app, &uids);
            budget.refund(tags.len() as u64);
            counts
        }
        None => vec![None; uids.len()],
    };
    for ((index, _, track), count) in text.iter().zip(counts) {
        let listed = entries.iter().filter(|e| e.track == track.number).count() as u64;
        if let Some(blocks) = count.filter(|&blocks| blocks != listed) {
            return Err(format!("the cues list {listed} of stream {index}'s {blocks} blocks"));
        }
    }

    let mut headers = Vec::with_capacity(text.len());
    for (_, _, track) in &text {
        budget.spend(track.codec_private.len() as u64)?;
        let mut header = track.codec_private.to_vec();
        if let Some(encoding) = track.encodings.first().filter(|e| e.scope & 2 != 0 && !header.is_empty()) {
            header = match encoding.decode(&header, budget) {
                Ok(header) => header,
                // FFmpeg drops a header it can't decode.
                Err(Undecoded::Corrupt(_)) => Vec::new(),
                Err(refused) => return Err(refused.into()),
            };
        }
        headers.push(header);
    }
    let whole_ms = tick_ns % 1_000_000 == 0 && !streams.iter().any(|t| t.scaled);

    // In the order of the file, as FFmpeg reads them.
    entries.sort_by_key(|e| (e.cluster, e.relative));
    entries.dedup_by_key(|e| (e.cluster, e.relative));
    entries.shrink_to_fit();
    let clusters = clusters_of(layout.segment, &entries)?;
    let window = (ticks_from(order.from_ms, media, tick_ns), ticks_from(order.until_ms, media, tick_ns));
    let mut guess = true;
    // Whether the cues' times have been the blocks' own so far: the spans
    // stretches hold all the blocks of are told by them.
    let mut timed = true;
    let stretches = stretches(&clusters, window);
    let last = stretches.len() - 1;
    for (n, stretch) in stretches.into_iter().enumerate() {
        budget.check()?;
        let read = read_blocks(path, &clusters, stretch.clone(), &mut guess, budget)?;
        let mut blocks: Vec<Vec<TextBlock>> = text.iter().map(|_| Vec::new()).collect();
        let cues = clusters[stretch.clone()].iter().flat_map(|(_, entries)| entries.iter());
        for (ReadBlock { cue_track, cluster_time, block, .. }, cue) in in_file_order(read.by_reader).zip(cues) {
            budget.check()?;
            if block.track != cue_track {
                return Err("a cue points at another track's block".into());
            }
            let slot = slot(cue_track);
            let (_, codec, track) = &text[slot];
            if block.flags & 0x06 != 0 {
                return Err("laced subtitle blocks aren't supported".into());
            }
            let cluster_time = i64::try_from(cluster_time).map_err(|_| "a cluster's timestamp is out of range")?;
            let time = cluster_time.checked_add(block.timecode as i64).ok_or("a block's timestamp is out of range")?;
            timed &= u64::try_from(time).ok() == Some(cue.time);
            // FFmpeg gives a block before the start of time no timestamp, and
            // drops it.
            if time < 0 {
                continue;
            }
            let mut data = block.data;
            if let Some(encoding) = track.encodings.first().filter(|e| e.scope & 1 != 0) {
                data = encoding.decode(&data, budget)?;
            }
            if (kind.webm)(*codec) {
                data = webm_cue_text(&data).ok_or("a WebVTT block isn't laid out as WebM's")?.to_vec();
            }
            if data.is_empty() {
                continue;
            }
            let duration = match block.duration {
                Some(d) if d > 0 => d,
                _ => track.default_duration / tick_ns,
            };
            blocks[slot].push(TextBlock { time, duration, data });
        }
        let tracks = text
            .iter()
            .zip(&headers)
            .zip(blocks)
            .map(|((&(stream_index, codec, _), header), blocks)| SubtitleTrack {
                stream_index,
                codec,
                header: header.clone(),
                blocks,
            })
            .collect();
        let span = timed.then(|| span(&clusters, &stretch));
        let (reads, bytes_read, more) = (read.reads, read.bytes, n < last);
        got(SubtitleTracks { tick_ns, whole_ms, tracks, reads, bytes_read, more, span })?;
    }
    Ok(())
}

/// The clusters the entries (sorted by position) are in: where each starts
/// in the file, and its entries.
fn clusters_of(segment: u64, entries: &[CueEntry]) -> Result<Vec<Cluster<'_>>, String> {
    let same_cluster = |a: &CueEntry, b: &CueEntry| a.cluster == b.cluster;
    let mut clusters = Vec::with_capacity(entries.chunk_by(same_cluster).count());
    for cluster in entries.chunk_by(same_cluster) {
        let at = segment.checked_add(cluster[0].cluster).ok_or("a cue points past the end of the file")?;
        clusters.push((at, cluster));
    }
    Ok(clusters)
}

/// `ms` of the file's time, as FFmpeg takes it (its start taken off), in
/// the file's ticks of `tick_ns`: 0 for its start or before.
fn ticks_from(ms: i64, media: &MediaInfo, tick_ns: u64) -> u64 {
    if ms <= 0 {
        return 0;
    }
    let ms = ms.saturating_add(media.start_ms.unwrap_or(0)).max(0) as u128;
    u64::try_from(ms * 1_000_000 / u128::from(tick_ns.max(1))).unwrap_or(u64::MAX)
}

/// The span of the file's time (ticks) whose blocks the clusters `stretch`
/// hold all of: from past the last block before them to the first after
/// them (blocks come in order of time).
fn span(clusters: &[Cluster], stretch: &Range<usize>) -> (u64, u64) {
    let last_before = stretch.start.checked_sub(1).and_then(|c| clusters[c].1.iter().map(|e| e.time).max());
    let first_after = clusters.get(stretch.end).and_then(|(_, entries)| entries.iter().map(|e| e.time).min());
    (last_before.map_or(0, |time| time.saturating_add(1)), first_after.unwrap_or(u64::MAX))
}

/// How many clusters a stretch after or before the blocks around the
/// player has at most: each is handed over once read, so a player that
/// reads a track once is given what is read by then.
const STRETCH_CLUSTERS: usize = 256;

/// The stretches of `clusters` (in the order of the file) a read hands over
/// one after the other: those with blocks from `from` to `until` (ticks),
/// even when there are none, then those after, then those before, a few
/// hundred clusters at a time.
fn stretches(clusters: &[Cluster], (from, until): (u64, u64)) -> Vec<Range<usize>> {
    let first_from = |time: u64| {
        clusters.iter().position(|(_, entries)| entries.iter().any(|e| e.time >= time)).unwrap_or(clusters.len())
    };
    let start = first_from(from);
    let end = first_from(until).max(start);
    let pieces = |stretch: Range<usize>| {
        let last = stretch.end;
        stretch.step_by(STRETCH_CLUSTERS).map(move |at| at..(at + STRETCH_CLUSTERS).min(last))
    };
    let after = [end..clusters.len(), 0..start].into_iter().flat_map(pieces);
    std::iter::once(start..end).chain(after).collect()
}

/// Blocks are read by this many threads at once, each with a handle of its
/// own: a network share or a disk array serves reads side by side, and a
/// disk sorts nearby ones into one sweep. Each read is a seek on a disk
/// (around 10 ms on a NAS), and they are what a track costs.
const READERS: usize = 16;
/// While cluster heads are skipped (see [`Window::guess_block`]), each
/// reader still reads one head in this many, to check the guesses.
const CHECK_EVERY: usize = 16;

/// A block read for a cue.
struct ReadBlock {
    /// Its cluster's place in the stretch of clusters read.
    cluster: usize,
    /// The track the cue is for.
    cue_track: u64,
    /// Its cluster's timestamp.
    cluster_time: u64,
    block: RawBlock,
}

struct ReadBlocks {
    /// What each reader read, in the order it read it: see [`in_file_order`].
    by_reader: Vec<Vec<ReadBlock>>,
    reads: usize,
    bytes: u64,
}

enum ReadError {
    /// A guessed block wasn't where its cluster's head puts it: this file's
    /// heads are read after all.
    Guessed,
    Failed(String),
    /// Another reader failed.
    Stopped,
}

impl ReadError {
    /// The one of two readers' errors that says what went wrong.
    fn or(self, other: ReadError) -> ReadError {
        match (self, other) {
            (failed @ ReadError::Failed(_), _) | (_, failed @ ReadError::Failed(_)) => failed,
            (ReadError::Guessed, _) | (_, ReadError::Guessed) => ReadError::Guessed,
            _ => ReadError::Stopped,
        }
    }
}

impl From<&str> for ReadError {
    fn from(message: &str) -> Self {
        ReadError::Failed(message.to_owned())
    }
}

/// The blocks the entries of the clusters `stretch` of `clusters` (sorted
/// by position) point at, their memory taken out of `budget`. `guess` says
/// whether cluster heads may be skipped: once a stretch finds this file's
/// are needed, it is unset for the next ones.
fn read_blocks(
    path: &Path,
    clusters: &[Cluster],
    stretch: Range<usize>,
    guess: &mut bool,
    budget: &Budget,
) -> Result<ReadBlocks, String> {
    match read_blocks_with(path, clusters, stretch.clone(), *guess, budget) {
        Err(ReadError::Guessed) => {
            *guess = false;
            read_blocks_with(path, clusters, stretch, false, budget)
        }
        other => other,
    }
    .map_err(|e| match e {
        ReadError::Failed(message) => message,
        ReadError::Guessed => "blocks aren't where their clusters put them".into(),
        ReadError::Stopped => "a reader stopped".into(),
    })
}

/// A cluster to read blocks of: where it starts, and the entries in it.
type Cluster<'a> = (u64, &'a [CueEntry]);
/// What a reader read: its blocks, and the reads and bytes that took.
type ReaderBlocks = (Vec<ReadBlock>, usize, u64);

/// [`read_blocks`], skipping the cluster heads where the blocks can be told
/// without them (`guess`). The clusters are dealt out in turn, so the
/// readers move through the file side by side.
fn read_blocks_with(
    path: &Path,
    clusters: &[Cluster],
    stretch: Range<usize>,
    guess: bool,
    budget: &Budget,
) -> Result<ReadBlocks, ReadError> {
    let readers = READERS.min(stretch.len());
    let (failed, kept) = (AtomicBool::new(false), AtomicU64::new(0));
    let shared = Readers { path, clusters, stretch, guess, budget, failed: &failed, kept: &kept };
    let results: Vec<Result<ReaderBlocks, ReadError>> = std::thread::scope(|scope| {
        let shared = &shared;
        let workers: Vec<_> = (0..readers)
            .map(|first| {
                scope.spawn(move || {
                    let read = shared.read(first, readers);
                    if read.is_err() {
                        shared.failed.store(true, Ordering::Relaxed);
                    }
                    read
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap_or_else(|_| Err("a reader failed".into()))).collect()
    });
    let mut by_reader = Vec::with_capacity(readers);
    let (mut reads, mut bytes, mut error) = (0, 0, None::<ReadError>);
    for result in results {
        match result {
            Ok((blocks, n, b)) => {
                reads += n;
                bytes += b;
                by_reader.push(blocks);
            }
            Err(err) => {
                error = Some(match error.take() {
                    Some(earlier) => earlier.or(err),
                    None => err,
                })
            }
        }
    }
    match error {
        // Read again: what this attempt kept is let go.
        Some(ReadError::Guessed) => {
            drop(by_reader);
            budget.refund(kept.load(Ordering::Relaxed));
            Err(ReadError::Guessed)
        }
        Some(err) => Err(err),
        None => Ok(ReadBlocks { by_reader, reads, bytes }),
    }
}

/// The blocks the readers of [`read_blocks_with`] read, in the order of the
/// file: cluster after cluster, each from the reader it was dealt to.
fn in_file_order(by_reader: Vec<Vec<ReadBlock>>) -> impl Iterator<Item = ReadBlock> {
    let mut left: usize = by_reader.iter().map(Vec::len).sum();
    let mut readers: Vec<_> = by_reader.into_iter().map(|blocks| blocks.into_iter().peekable()).collect();
    let count = readers.len();
    let mut cluster = 0;
    std::iter::from_fn(move || {
        while left > 0 {
            let reader = &mut readers[cluster % count];
            if reader.peek().is_some_and(|block| block.cluster == cluster) {
                left -= 1;
                return reader.next();
            }
            cluster += 1;
        }
        None
    })
}

/// What the readers of [`read_blocks_with`] share.
struct Readers<'a> {
    path: &'a Path,
    /// Every cluster of the read, and those of this stretch.
    clusters: &'a [Cluster<'a>],
    stretch: Range<usize>,
    guess: bool,
    budget: &'a Budget,
    /// Set by a reader that fails: the others stop.
    failed: &'a AtomicBool,
    /// The memory taken by the blocks read so far.
    kept: &'a AtomicU64,
}

impl Readers<'_> {
    /// The blocks of every `step`th cluster of the stretch from the `first`
    /// on, checked to be where blocks can be: each inside its cluster
    /// (before the next one starts) and after the one before it, as a
    /// file's blocks are.
    fn read(&self, first: usize, step: usize) -> Result<ReaderBlocks, ReadError> {
        let file = File::open(self.path).map_err(|e| ReadError::Failed(e.to_string()))?;
        let mut reader = Window::new(file);
        let mine = || self.stretch.clone().skip(first).step_by(step);
        // A block for each entry, or none at all: room for them all at once.
        let mut out = Vec::with_capacity(mine().map(|n| self.clusters[n].1.len()).sum());
        for (k, n) in mine().enumerate() {
            let (at, cluster) = self.clusters[n];
            let next = self.clusters.get(n + 1).map(|&(next, _)| next);
            let checked = !self.guess || k % CHECK_EVERY == 0;
            let mut head = None;
            let mut end = at;
            for entry in cluster {
                if self.failed.load(Ordering::Relaxed) {
                    return Err(ReadError::Stopped);
                }
                self.budget.check().map_err(ReadError::Failed)?;
                if checked && head.is_none() {
                    head = Some(reader.cluster_head(at).ok_or("a cue points at no cluster")?);
                }
                let guessed = if self.guess { reader.guess_block(at, entry) } else { None };
                let (pos, time, block, limit, unchecked) = match (head, guessed) {
                    // The cue's time is the block's: its cluster's timestamp
                    // is what's left.
                    (None, Some((pos, block))) => {
                        let time = cluster_time(entry.time, block.timecode).ok_or(ReadError::Guessed)?;
                        (pos, time, block, next, true)
                    }
                    (_, guessed) => {
                        let (data_start, time, data_end) = match head {
                            Some(head) => head,
                            None => *head.insert(reader.cluster_head(at).ok_or("a cue points at no cluster")?),
                        };
                        let pos = data_start
                            .checked_add(entry.relative.unwrap_or_default())
                            .ok_or("a cue points past the end of the file")?;
                        let block = reader.block(pos)?;
                        // A guess checked against the head must agree with it.
                        let wrong = |(at, block): (u64, RawBlock)| {
                            at != pos || cluster_time(entry.time, block.timecode) != Some(time)
                        };
                        if guessed.is_some_and(wrong) {
                            return Err(ReadError::Guessed);
                        }
                        let limit = match (data_end, next) {
                            (Some(end), Some(next)) => Some(end.min(next)),
                            (end, next) => end.or(next),
                        };
                        (pos, time, block, limit, false)
                    }
                };
                let block_end = pos.checked_add(block.len).ok_or("a block runs past the end of the file")?;
                let misplaced = if pos < end {
                    Some("a block overlaps the one before it")
                } else if limit.is_some_and(|limit| block_end > limit) {
                    Some("a block runs past its cluster")
                } else {
                    None
                };
                if let Some(misplaced) = misplaced {
                    // Guesses where no head was read may be wrong: read them.
                    return Err(if unchecked { ReadError::Guessed } else { misplaced.into() });
                }
                end = block_end;
                let size = block.data.len() as u64;
                self.budget.spend(size).map_err(ReadError::Failed)?;
                self.kept.fetch_add(size, Ordering::Relaxed);
                out.push(ReadBlock {
                    cluster: n - self.stretch.start,
                    cue_track: entry.track,
                    cluster_time: time,
                    block,
                });
            }
        }
        Ok((out, reader.reads, reader.bytes_read))
    }
}

/// The timestamp of the cluster of a block whose cue says it starts at
/// `time`, from the block's own timecode in it: never below zero.
fn cluster_time(time: u64, timecode: i16) -> Option<u64> {
    let cluster = i64::try_from(time).ok()?.checked_sub(timecode as i64)?;
    u64::try_from(cluster).ok()
}

/// The text of a WebM WebVTT block (`id\nsettings\ntext`), as FFmpeg's
/// demuxer takes it.
fn webm_cue_text(data: &[u8]) -> Option<&[u8]> {
    let mut p = 0;
    for _ in 0..2 {
        while p < data.len() && !matches!(data[p], b'\r' | b'\n') {
            p += 1;
        }
        if data.get(p) == Some(&b'\r') {
            p += 1;
        }
        if data.get(p) != Some(&b'\n') {
            return None;
        }
        p += 1;
    }
    let mut text = &data[p..];
    while let [rest @ .., b'\r' | b'\n'] = text {
        text = rest;
    }
    Some(text)
}

#[derive(Debug, Clone, Copy)]
struct CueEntry {
    /// The block's time, in ticks.
    time: u64,
    track: u64,
    /// The cluster's position from the segment's start.
    cluster: u64,
    /// The block's position in its cluster's data.
    relative: Option<u64>,
    /// How long the block lasts, in ticks (written for subtitles).
    duration: Option<u64>,
}

/// The Cues' entries for blocks of `tracks`, each taken out of `budget` as it
/// is found ([`ENTRY_COST`]), the time left checked as they are read.
fn cue_entries(cues: &[u8], tracks: &[u64], budget: &Budget) -> Result<Vec<CueEntry>, String> {
    let mut entries = Vec::new();
    let mut read = 0;
    let mut checked = || {
        read += 1;
        if read % CUES_BETWEEN_CHECKS == 0 { budget.check() } else { Ok(()) }
    };
    for (_, point) in children(cues).filter(|(id, _)| *id == CUE_POINT) {
        checked()?;
        let time = find_uint(point, CUE_TIME);
        for (_, positions) in children(point).filter(|(id, _)| *id == CUE_TRACK_POSITIONS) {
            checked()?;
            if let Some(entry) = cue_entry(time, positions, tracks) {
                budget.spend(ENTRY_COST)?;
                entries.push(entry);
            }
        }
    }
    Ok(entries)
}

/// The entry of a cue point at `time` with these track positions, when it is
/// for one of `tracks`.
fn cue_entry(time: Option<u64>, positions: &[u8], tracks: &[u64]) -> Option<CueEntry> {
    let track = find_uint(positions, CUE_TRACK).filter(|track| tracks.contains(track))?;
    Some(CueEntry {
        time: time?,
        track,
        cluster: find_uint(positions, CUE_CLUSTER_POSITION)?,
        relative: find_uint(positions, CUE_RELATIVE_POSITION),
        duration: find_uint(positions, CUE_DURATION),
    })
}

/// Whether the Cues have an entry for a block of any of `tracks`.
fn lists_any(cues: &[u8], tracks: &[u64]) -> bool {
    children(cues).filter(|(id, _)| *id == CUE_POINT).any(|(_, point)| {
        let time = find_uint(point, CUE_TIME);
        children(point)
            .filter(|(id, _)| *id == CUE_TRACK_POSITIONS)
            .any(|(_, positions)| cue_entry(time, positions, tracks).is_some())
    })
}

/// The number of blocks of each of the tracks `uids` in mkvmerge's
/// statistics tags (`None` where they don't say), when the tags were written
/// by the program that wrote the file: another muxer copies them over
/// unchanged, counts and all.
fn block_counts(tags: &[u8], writing_app: Option<&[u8]>, uids: &[u64]) -> Vec<Option<u64>> {
    let mut counts = vec![None; uids.len()];
    for (_, tag) in children(tags).filter(|(id, _)| *id == TAG) {
        let Some(slot) = children(tag)
            .find(|(id, _)| *id == TARGETS)
            .and_then(|(_, targets)| find_uint(targets, TAG_TRACK_UID))
            .and_then(|uid| uids.iter().position(|&u| u == uid))
        else {
            continue;
        };
        // The first count of a track is its own.
        if counts[slot].is_some() {
            continue;
        }
        let mut frames = None;
        let mut app = None;
        for (_, simple) in children(tag).filter(|(id, _)| *id == SIMPLE_TAG) {
            let name = children(simple).find(|(id, _)| *id == TAG_NAME).map(|(_, n)| n);
            let value = children(simple).find(|(id, _)| *id == TAG_STRING).map(|(_, v)| v);
            match (name, value) {
                (Some(b"NUMBER_OF_FRAMES"), Some(v)) => {
                    frames = std::str::from_utf8(v).ok().and_then(|v| v.trim().parse().ok())
                }
                (Some(b"_STATISTICS_WRITING_APP"), Some(v)) => app = Some(v),
                _ => {}
            }
        }
        if let (Some(frames), Some(app)) = (frames, app)
            && writing_app.is_some_and(|w| trim_nul(w) == trim_nul(app))
        {
            counts[slot] = Some(frames);
        }
    }
    counts
}

fn trim_nul(s: &[u8]) -> &[u8] {
    let end = s.iter().rposition(|&b| b != 0).map_or(0, |p| p + 1);
    &s[..end]
}

/// A block of a cluster: a SimpleBlock, or the Block of a BlockGroup.
struct RawBlock {
    track: u64,
    timecode: i16,
    flags: u8,
    duration: Option<u64>,
    data: Vec<u8>,
    /// The length of its element in the file.
    len: u64,
}

/// Reads small parts of a file, keeping the last read for the parts near it:
/// the subtitle blocks of every track for a given moment sit next to each
/// other.
struct Window<R> {
    inner: R,
    start: u64,
    data: Vec<u8>,
    reads: usize,
    bytes_read: u64,
}

/// How much to read for a cluster's head, and around a block: a Blu-ray's
/// PGS pictures take up to tens of kilobytes, and on a disk a read costs a
/// seek more than its bytes, so one read takes all of one.
const HEAD_READ: usize = 64;
const BLOCK_READ: usize = 64 * 1024;

impl<R: Read + Seek> Window<R> {
    fn new(inner: R) -> Self {
        Self { inner, start: 0, data: Vec::new(), reads: 0, bytes_read: 0 }
    }

    /// `len` bytes at `pos` (fewer at the end of the file), reading at least
    /// `ahead` when they aren't in memory.
    fn get(&mut self, pos: u64, len: usize, ahead: usize) -> Option<&[u8]> {
        let end = pos.checked_add(len as u64)?;
        if pos < self.start || end > self.start + self.data.len() as u64 {
            self.inner.seek(SeekFrom::Start(pos)).ok()?;
            // Room for this read alone: a big block's isn't kept for the
            // small ones after it.
            let size = len.max(ahead);
            (self.start, self.data) = (pos, Vec::with_capacity(size));
            (&mut self.inner).take(size as u64).read_to_end(&mut self.data).ok()?;
            self.reads += 1;
            self.bytes_read += self.data.len() as u64;
        }
        let from = usize::try_from(pos - self.start).ok()?;
        Some(&self.data[from..(from + len).min(self.data.len())])
    }

    /// Where the data of the cluster at `pos` starts, its timestamp, and
    /// where it ends (unless its size is unknown).
    fn cluster_head(&mut self, pos: u64) -> Option<(u64, u64, Option<u64>)> {
        let head = self.get(pos, HEAD_READ, HEAD_READ)?.to_vec();
        let mut cursor = std::io::Cursor::new(head.as_slice());
        let (id, cluster_size) = element_header(&mut cursor)?;
        if id != CLUSTER {
            return None;
        }
        let data_start = pos.checked_add(cursor.position())?;
        let data_end = if cluster_size == UNKNOWN_SIZE { None } else { Some(data_start.checked_add(cluster_size)?) };
        // The timestamp comes first, perhaps after a checksum or padding.
        let mut at = data_start;
        for _ in 0..4 {
            let bytes = self.get(at, 24, HEAD_READ)?.to_vec();
            let mut cursor = std::io::Cursor::new(bytes.as_slice());
            let (id, size) = element_header(&mut cursor)?;
            let value_at = cursor.position() as usize;
            match id {
                CLUSTER_TIMESTAMP => {
                    let value = bytes.get(value_at..value_at.checked_add(usize::try_from(size).ok()?)?)?;
                    return Some((data_start, uint(value)?, data_end));
                }
                SIMPLE_BLOCK | BLOCK_GROUP => return None,
                _ if size == UNKNOWN_SIZE => return None,
                _ => at = at.checked_add(value_at as u64)?.checked_add(size)?,
            }
        }
        None
    }

    /// The block `entry` points at, found without reading its cluster's
    /// head: a cluster's id and size take 5 to 12 bytes, so the block starts
    /// at one of 8 places, and only one of them can hold a BlockGroup of the
    /// entry's track that lasts as long as the cue says and is filled
    /// exactly by its elements, or, for a cue without a duration (mkvmerge
    /// writes none for PGS), a whole SimpleBlock of the entry's track. With
    /// the block's position; `None` when that isn't certain (two fits, a
    /// block longer than a read).
    fn guess_block(&mut self, cluster: u64, entry: &CueEntry) -> Option<(u64, RawBlock)> {
        let earliest = cluster.checked_add(5)?.checked_add(entry.relative?)?;
        let bytes = self.get(earliest, 7 + BLOCK_READ, BLOCK_READ)?.to_vec();
        let block_at = |bytes: &[u8]| match entry.duration {
            Some(duration) => block_group_at(bytes, entry.track, duration),
            None => simple_block_at(bytes, entry.track),
        };
        let mut found = None;
        for shift in 0..bytes.len().min(8) {
            if let Some(block) = block_at(&bytes[shift..]) {
                if found.is_some() {
                    return None;
                }
                found = Some((earliest + shift as u64, block));
            }
        }
        // A cluster's timestamp can't be below zero.
        found.filter(|(_, block)| cluster_time(entry.time, block.timecode).is_some())
    }

    /// The block element at `pos`, if it's no bigger than a subtitle can be.
    fn block(&mut self, pos: u64) -> Result<RawBlock, ReadError> {
        const NO_BLOCK: &str = "a cue points at no block";
        let head = self.get(pos, 12, BLOCK_READ).ok_or(NO_BLOCK)?.to_vec();
        let mut cursor = std::io::Cursor::new(head.as_slice());
        let (id, size) = element_header(&mut cursor).ok_or(NO_BLOCK)?;
        if size == UNKNOWN_SIZE {
            return Err(NO_BLOCK.into());
        }
        if size > MAX_BLOCK {
            return Err("a block is too big for a subtitle".into());
        }
        let len = cursor.position() + size;
        let payload = self.get(pos + cursor.position(), size as usize, BLOCK_READ).ok_or(NO_BLOCK)?;
        if payload.len() as u64 != size {
            return Err(NO_BLOCK.into());
        }
        let block = match id {
            SIMPLE_BLOCK => parse_block(payload, None, len),
            BLOCK_GROUP => children(payload).find(|(id, _)| *id == BLOCK).and_then(|(_, block)| {
                let duration = children(payload).find(|(id, _)| *id == BLOCK_DURATION).and_then(|(_, d)| uint(d));
                parse_block(block, Some(duration.unwrap_or(0)), len)
            }),
            _ => None,
        };
        block.ok_or_else(|| NO_BLOCK.into())
    }
}

/// A whole BlockGroup of `track` lasting `duration` at the start of `bytes`.
fn block_group_at(bytes: &[u8], track: u64, duration: u64) -> Option<RawBlock> {
    let mut cursor = std::io::Cursor::new(bytes);
    let (id, size) = element_header(&mut cursor)?;
    if id != BLOCK_GROUP || size == UNKNOWN_SIZE {
        return None;
    }
    let start = cursor.position() as usize;
    let payload = bytes.get(start..start.checked_add(usize::try_from(size).ok()?)?)?;
    let (mut block, mut length) = (None, None);
    let mut cursor = std::io::Cursor::new(payload);
    while (cursor.position() as usize) < payload.len() {
        let (id, size) = element_header(&mut cursor)?;
        let from = cursor.position() as usize;
        let value = payload.get(from..from.checked_add(usize::try_from(size).ok()?)?)?;
        cursor.set_position((from + value.len()) as u64);
        match id {
            BLOCK if block.is_none() => block = Some(value),
            BLOCK_DURATION if length.is_none() => length = Some(uint(value)?),
            // What else a BlockGroup may hold: references, additions,
            // padding, codec state, void, a checksum.
            0xFB | 0xFA | 0x75A1 | 0x75A2 | 0xA4 | 0xEC | 0xBF => {}
            _ => return None,
        }
    }
    if length != Some(duration) {
        return None;
    }
    let block = parse_block(block?, length, (start + payload.len()) as u64)?;
    (block.track == track && block.flags & 0x06 == 0).then_some(block)
}

/// A whole SimpleBlock of `track`, not laced, at the start of `bytes`.
fn simple_block_at(bytes: &[u8], track: u64) -> Option<RawBlock> {
    let mut cursor = std::io::Cursor::new(bytes);
    let (id, size) = element_header(&mut cursor)?;
    if id != SIMPLE_BLOCK || size == UNKNOWN_SIZE || size > MAX_BLOCK {
        return None;
    }
    let start = cursor.position() as usize;
    let payload = bytes.get(start..start.checked_add(usize::try_from(size).ok()?)?)?;
    let block = parse_block(payload, None, (start + payload.len()) as u64)?;
    (block.track == track && block.flags & 0x06 == 0).then_some(block)
}

/// The Block (or SimpleBlock) in `payload`, of an element `len` bytes long.
fn parse_block(payload: &[u8], duration: Option<u64>, len: u64) -> Option<RawBlock> {
    let mut cursor = std::io::Cursor::new(payload);
    let (track, _) = vint(&mut cursor, false)?;
    let at = cursor.position() as usize;
    let header = payload.get(at..at + 3)?;
    Some(RawBlock {
        track,
        timecode: i16::from_be_bytes([header[0], header[1]]),
        flags: header[2],
        duration,
        data: payload[at + 3..].to_vec(),
        len,
    })
}

// ------------------------------------------------------------------ EBML

/// Variable-length integer: (value, length). Element ids keep their marker bit.
fn vint(r: &mut impl Read, keep_marker: bool) -> Option<(u64, usize)> {
    let mut first = [0u8; 1];
    r.read_exact(&mut first).ok()?;
    let b = first[0];
    if b == 0 {
        return None;
    }
    let len = b.leading_zeros() as usize + 1;
    // For 8-byte vints the first byte holds only the marker (mask = 0).
    let mut value = if keep_marker { b as u64 } else { b as u64 & (0xFF_u64 >> len) };
    let mut rest = [0u8; 7];
    r.read_exact(&mut rest[..len - 1]).ok()?;
    for byte in &rest[..len - 1] {
        value = (value << 8) | *byte as u64;
    }
    Some((value, len))
}

fn element_header(r: &mut impl Read) -> Option<(u32, u64)> {
    let (id, id_len) = vint(r, true)?;
    if id_len > 4 {
        return None;
    }
    let (size, size_len) = vint(r, false)?;
    let unknown = size == (1u64 << (7 * size_len)) - 1;
    Some((id as u32, if unknown { UNKNOWN_SIZE } else { size }))
}

fn read_payload(r: &mut impl Read, size: u64) -> Option<Vec<u8>> {
    if size == UNKNOWN_SIZE || size > MAX_ELEMENT_READ {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    r.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Payload of the element at `pos`, checking it has the expected id.
fn child_payload<R: Read + Seek>(r: &mut R, pos: u64, expected: u32) -> Option<Vec<u8>> {
    r.seek(SeekFrom::Start(pos)).ok()?;
    let (id, size) = element_header(r)?;
    if id != expected {
        return None;
    }
    read_payload(r, size)
}

/// [`child_payload`] at `pos` if there's one, its memory taken out of
/// `budget` (the caller gives it back when it lets it go), and the time the
/// read has left checked first.
fn budgeted_payload<R: Read + Seek>(
    r: &mut R,
    pos: Option<u64>,
    expected: u32,
    budget: &Budget,
) -> Result<Option<Vec<u8>>, String> {
    let Some(pos) = pos else { return Ok(None) };
    budget.check()?;
    r.seek(SeekFrom::Start(pos)).map_err(|e| e.to_string())?;
    match element_header(r) {
        Some((id, size)) if id == expected && size <= MAX_ELEMENT_READ => {
            budget.spend(size)?;
            let payload = read_payload(r, size);
            if payload.is_none() {
                budget.refund(size);
            }
            Ok(payload)
        }
        _ => Ok(None),
    }
}

/// Iterate `(id, payload)` over the direct children in `data`.
fn children(data: &[u8]) -> impl Iterator<Item = (u32, &[u8])> {
    let mut cursor = std::io::Cursor::new(data);
    std::iter::from_fn(move || {
        let (id, size) = element_header(&mut cursor)?;
        let start = cursor.position() as usize;
        let end = start.checked_add(usize::try_from(size).ok()?)?;
        if end > data.len() {
            return None;
        }
        cursor.set_position(end as u64);
        Some((id, &data[start..end]))
    })
}

fn uint(bytes: &[u8]) -> Option<u64> {
    if bytes.is_empty() || bytes.len() > 8 {
        return None;
    }
    Some(bytes.iter().fold(0u64, |acc, b| (acc << 8) | *b as u64))
}

fn float(bytes: &[u8]) -> Option<f64> {
    match bytes.len() {
        4 => Some(f32::from_be_bytes(bytes.try_into().ok()?) as f64),
        8 => Some(f64::from_be_bytes(bytes.try_into().ok()?)),
        _ => None,
    }
}

fn find_uint(data: &[u8], id: u32) -> Option<u64> {
    children(data).find(|(child, _)| *child == id).and_then(|(_, payload)| uint(payload))
}

/// The `(element id, position)` entries of a seek head.
fn seek_entries(data: &[u8]) -> impl Iterator<Item = (u32, u64)> + '_ {
    children(data).filter(|(id, _)| *id == SEEK).filter_map(|(_, seek)| {
        let target = children(seek).find(|(id, _)| *id == SEEK_ID).and_then(|(_, p)| uint(p))? as u32;
        let position = find_uint(seek, SEEK_POSITION)?;
        Some((target, position))
    })
}

fn first_video_track(tracks: &[u8]) -> Option<u64> {
    children(tracks)
        .filter(|(id, _)| *id == TRACK_ENTRY)
        .find(|(_, entry)| find_uint(entry, TRACK_TYPE) == Some(1))
        .and_then(|(_, entry)| find_uint(entry, TRACK_NUMBER))
}

/// The `(CueTime, CueClusterPosition)` of the cue points of `track`: the
/// cluster's position counts from the start of the segment's data, and may be
/// missing.
fn cue_times(cues: &[u8], track: u64) -> Vec<(u64, Option<u64>)> {
    children(cues)
        .filter(|(id, _)| *id == CUE_POINT)
        .filter_map(|(_, point)| {
            let time = find_uint(point, CUE_TIME)?;
            let (_, positions) = children(point)
                .filter(|(id, _)| *id == CUE_TRACK_POSITIONS)
                .find(|(_, pos)| find_uint(pos, CUE_TRACK) == Some(track))?;
            Some((time, find_uint(positions, CUE_CLUSTER_POSITION)))
        })
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use pley_transcoder_api::MediaStream;

    /// Encode an element (id bytes as written, 8-byte size).
    fn el(id: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.push(0x01);
        out.extend_from_slice(&(payload.len() as u64).to_be_bytes()[1..]);
        out.extend_from_slice(payload);
        out
    }

    fn uint_el(id: &[u8], v: u64) -> Vec<u8> {
        el(id, &v.to_be_bytes())
    }

    /// Encode an element with as short a size as fits, as FFmpeg writes
    /// clusters (so their heads differ in length).
    fn el_short(id: &[u8], payload: &[u8]) -> Vec<u8> {
        let len = payload.len() as u64;
        let n = (1..=8).find(|n| len < (1u64 << (7 * n)) - 1).unwrap();
        let mut out = id.to_vec();
        out.extend_from_slice(&(len | 1 << (7 * n)).to_be_bytes()[8 - n..]);
        out.extend_from_slice(payload);
        out
    }

    /// More memory and time than any test file needs.
    fn plenty() -> Budget {
        Budget::new(64 << 20, Duration::from_secs(600))
    }

    /// `PLEY_TEST_MKV=<file> cargo test -p pley-transcoder real_file -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_file() {
        let path = std::env::var("PLEY_TEST_MKV").expect("set PLEY_TEST_MKV");
        let mut file = BufReader::new(File::open(&path).unwrap());
        let (id, size) = element_header(&mut file).unwrap();
        println!("first element {id:#x} size {size}");
        file.seek_relative(size as i64).unwrap();
        let (id, size) = element_header(&mut file).unwrap();
        let start = file.stream_position().unwrap();
        println!("segment {id:#x} size {size} data at {start}");
        let mut pos = start;
        for _ in 0..12 {
            file.seek(SeekFrom::Start(pos)).unwrap();
            let Some((id, size)) = element_header(&mut file) else { break };
            let data = file.stream_position().unwrap();
            println!("  top-level {id:#x} at {pos} size {size}");
            if id == SEEK_HEAD {
                let seek_head = read_payload(&mut file, size).unwrap();
                println!("    seek head: {:x?}", seek_entries(&seek_head).collect::<Vec<_>>());
            }
            if size == UNKNOWN_SIZE {
                break;
            }
            pos = data + size;
        }
        println!("result: {:?}", cue_keyframes(Path::new(&path)).map(|t| t.into_iter().take(5).collect::<Vec<_>>()));
    }

    #[test]
    fn reads_vints() {
        assert_eq!(vint(&mut &[0x81u8][..], false), Some((1, 1)));
        assert_eq!(vint(&mut &[0x40u8, 0x02][..], false), Some((2, 2)));
        assert_eq!(vint(&mut &[0x1A, 0x45, 0xDF, 0xA3][..], true), Some((0x1A45_DFA3, 4)));
        let unknown = element_header(&mut &[0xA3u8, 0xFF][..]).unwrap();
        assert_eq!(unknown, (0xA3, UNKNOWN_SIZE));
    }

    #[test]
    fn parses_a_synthetic_file() {
        let video_entry = el(&[0xAE], &[uint_el(&[0xD7], 1), uint_el(&[0x83], 1)].concat());
        let audio_entry = el(&[0xAE], &[uint_el(&[0xD7], 2), uint_el(&[0x83], 2)].concat());
        let tracks = el(&[0x16, 0x54, 0xAE, 0x6B], &[audio_entry, video_entry].concat());
        let info = el(&[0x15, 0x49, 0xA9, 0x66], &uint_el(&[0x2A, 0xD7, 0xB1], 1_000_000));
        let cue = |time: u64, track: u64| {
            let position = [uint_el(&[0xF7], track), uint_el(&[0xF1], 1000 + time)].concat();
            el(&[0xBB], &[uint_el(&[0xB3], time), el(&[0xB7], &position)].concat())
        };
        let cues = el(&[0x1C, 0x53, 0xBB, 0x6B], &[cue(0, 1), cue(2002, 1), cue(500, 2), cue(4004, 1)].concat());
        let cues_at = (info.len() + tracks.len()) as u64;
        let cues_len = cues.len() as u64;
        let segment = el(&[0x18, 0x53, 0x80, 0x67], &[info, tracks, cues].concat());
        let ebml = el(&[0x1A, 0x45, 0xDF, 0xA3], &[0x42, 0x86, 0x81, 0x01]);
        // Where the segment's data starts: positions count from there.
        let data = (ebml.len() + 4 + 8) as u64;
        let file = [ebml, segment].concat();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.mkv");
        std::fs::write(&path, file).unwrap();
        assert_eq!(cue_keyframes(&path), Some(vec![0, 2002, 4004]));
        // The Cues' bytes, and the cluster of each video keyframe.
        let (cues, points) = cue_index(&path).unwrap();
        assert_eq!(cues, data + cues_at..data + cues_at + cues_len);
        assert_eq!(points, vec![(0, data + 1000), (2002, data + 3002), (4004, data + 5004)]);

        std::fs::write(&path, b"not a matroska file").unwrap();
        assert_eq!(cue_keyframes(&path), None);
    }

    /// An AttachedFile element.
    fn attached(name: &str, mime: &str, data: &[u8]) -> Vec<u8> {
        let children =
            [el(&[0x46, 0x6E], name.as_bytes()), el(&[0x46, 0x60], mime.as_bytes()), el(&[0x46, 0x5C], data)];
        el(&[0x61, 0xA7], &children.concat())
    }

    /// A Matroska file with these files attached (name, MIME type, data)
    /// and nothing else.
    pub(crate) fn with_attachments(files: &[(&str, &str, &[u8])]) -> Vec<u8> {
        let files: Vec<u8> = files.iter().flat_map(|(name, mime, data)| attached(name, mime, data)).collect();
        let segment = el(&[0x18, 0x53, 0x80, 0x67], &el(&[0x19, 0x41, 0xA4, 0x69], &files));
        [el(&[0x1A, 0x45, 0xDF, 0xA3], &[0x42, 0x86, 0x81, 0x01]), segment].concat()
    }

    #[test]
    fn reads_the_attached_fonts() {
        let info = el(&[0x15, 0x49, 0xA9, 0x66], &uint_el(&[0x2A, 0xD7, 0xB1], 1_000_000));
        let files = [
            attached("Font One.ttf", "application/x-truetype-font", b"font one"),
            attached("cover.jpg", "image/jpeg", b"a picture"),
            attached("two.OTF", "application/octet-stream", b"font two"),
            attached("three", "font/sfnt", b"font three!"),
        ];
        let attachments = el(&[0x19, 0x41, 0xA4, 0x69], &files.concat());
        let cluster = el(&[0x1F, 0x43, 0xB6, 0x75], &uint_el(&[0xE7], 0));
        let ebml = el(&[0x1A, 0x45, 0xDF, 0xA3], &[0x42, 0x86, 0x81, 0x01]);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.mkv");
        let names = |fonts: Vec<Font>| fonts.into_iter().map(|f| f.name).collect::<Vec<_>>();

        // Before the first cluster, found by going through the elements.
        let segment = el(&[0x18, 0x53, 0x80, 0x67], &[info.clone(), attachments.clone(), cluster.clone()].concat());
        std::fs::write(&path, [ebml.clone(), segment].concat()).unwrap();
        let fonts = attached_fonts(&path, u64::MAX).unwrap();
        assert_eq!(names(fonts.clone()), ["Font One.ttf", "two.OTF", "three"]);
        assert_eq!(fonts[0].data, b"font one");
        // No more than the budget: the third doesn't fit after the first two.
        assert_eq!(names(attached_fonts(&path, 20).unwrap()), ["Font One.ttf", "two.OTF"]);

        // After the clusters, where the seek head says.
        let seek =
            |id: &[u8], pos: u64| el(&[0x4D, 0xBB], &[el(&[0x53, 0xAB], id), uint_el(&[0x53, 0xAC], pos)].concat());
        let seek_head_len = el(&[0x11, 0x4D, 0x9B, 0x74], &seek(&[0x19, 0x41, 0xA4, 0x69], 0)).len();
        let at = (seek_head_len + info.len() + cluster.len()) as u64;
        let seek_head = el(&[0x11, 0x4D, 0x9B, 0x74], &seek(&[0x19, 0x41, 0xA4, 0x69], at));
        let segment = el(&[0x18, 0x53, 0x80, 0x67], &[seek_head, info.clone(), cluster.clone(), attachments].concat());
        std::fs::write(&path, [ebml.clone(), segment].concat()).unwrap();
        assert_eq!(names(attached_fonts(&path, u64::MAX).unwrap()), ["Font One.ttf", "two.OTF", "three"]);

        // None attached; not Matroska.
        let segment = el(&[0x18, 0x53, 0x80, 0x67], &[info, cluster].concat());
        std::fs::write(&path, [ebml, segment].concat()).unwrap();
        assert_eq!(attached_fonts(&path, u64::MAX).unwrap(), Vec::new());
        std::fs::write(&path, b"not a matroska file").unwrap();
        assert!(attached_fonts(&path, u64::MAX).is_err());
    }

    #[test]
    fn no_more_than_so_many_fonts_are_taken() {
        let names: Vec<String> = (0..MAX_FONTS + 50).map(|n| format!("{n}.ttf")).collect();
        let files: Vec<(&str, &str, &[u8])> = names.iter().map(|n| (n.as_str(), "font/ttf", b"f".as_slice())).collect();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("many.mkv");
        std::fs::write(&path, with_attachments(&files)).unwrap();
        let fonts = attached_fonts(&path, u64::MAX).unwrap();
        assert_eq!(fonts.len(), MAX_FONTS);
        assert_eq!(fonts[0].name, "0.ttf");
    }

    // ------------------------------------------------------------ text tracks

    /// A track of the synthetic file.
    pub(crate) struct TestTrack {
        pub(crate) number: u64,
        pub(crate) kind: u64,
        pub(crate) codec: &'static str,
        pub(crate) private: Vec<u8>,
        /// Extra children of the entry (content encodings).
        pub(crate) extra: Vec<u8>,
    }

    impl TestTrack {
        pub(crate) fn new(number: u64, kind: u64, codec: &'static str) -> Self {
            Self { number, kind, codec, private: Vec::new(), extra: Vec::new() }
        }

        fn entry(&self) -> Vec<u8> {
            let mut children = [
                uint_el(&[0xD7], self.number),
                uint_el(&[0x73, 0xC5], 100 + self.number),
                uint_el(&[0x83], self.kind),
                el(&[0x86], self.codec.as_bytes()),
            ]
            .concat();
            if !self.private.is_empty() {
                children.extend(el(&[0x63, 0xA2], &self.private));
            }
            children.extend(&self.extra);
            el(&[0xAE], &children)
        }
    }

    /// Where a synthetic file says its Cues are.
    #[derive(Debug, Clone, Copy, PartialEq)]
    enum Listed {
        /// In the seek head at its start, as mkvmerge and FFmpeg do.
        InSeekHead,
        /// In a second seek head at its end, which the first points to.
        InSecondSeekHead,
        /// Nowhere: only going through its clusters finds them.
        Nowhere,
    }

    /// A block of the synthetic file: (track, timecode in its cluster,
    /// duration, data), written as a BlockGroup when it has a duration.
    pub(crate) type TestBlock = (u64, i16, Option<u64>, Vec<u8>);

    /// A block as an element: a BlockGroup when it has a duration, else a
    /// SimpleBlock; `flags` as the block's flags byte.
    fn block_element((track, timecode, duration, data): &TestBlock, flags: u8) -> Vec<u8> {
        let mut block = vec![0x80 | *track as u8];
        block.extend_from_slice(&timecode.to_be_bytes());
        block.push(flags);
        block.extend_from_slice(data);
        match duration {
            Some(d) => el(&[0xA0], &[el(&[0xA1], &block), uint_el(&[0x9B], *d)].concat()),
            None => el(&[0xA3], &block),
        }
    }

    /// A Matroska file with `tracks` and clusters of `(timestamp, blocks)`,
    /// with a cue for every block of a subtitle track, placed in the file
    /// after the clusters as mkvmerge does.
    pub(crate) struct TestFile {
        pub(crate) tracks: Vec<TestTrack>,
        pub(crate) clusters: Vec<(u64, Vec<TestBlock>)>,
        /// Leave out the blocks' places in their clusters.
        pub(crate) no_relative: bool,
        /// Where the file says its Cues are.
        cues_listed: Listed,
        /// Clusters with sizes as short as fit (else 8 bytes, as mkvmerge's
        /// can be).
        short_clusters: bool,
        /// Cue times this far off the blocks' (a muxer writing others)...
        cue_time_offset: u64,
        /// ...in this cluster only (the cluster's place in the list).
        offset_cluster: Option<usize>,
        /// Subtitle tracks (by number) left out of the Cues.
        uncued: Vec<u64>,
        /// More cues: `(track, cluster, block, offset)` point `offset` bytes
        /// into the element of block `block` of cluster `cluster` (places in
        /// the lists), claiming a block of `track` is there.
        pub(crate) extra_cues: Vec<(u64, usize, usize, u64)>,
        /// The flags byte of every block (`0x80`: a keyframe).
        block_flags: u8,
        tags: Vec<u8>,
        writing_app: &'static str,
    }

    impl TestFile {
        pub(crate) fn new(tracks: Vec<TestTrack>, clusters: Vec<(u64, Vec<TestBlock>)>) -> Self {
            Self {
                tracks,
                clusters,
                no_relative: false,
                cues_listed: Listed::InSeekHead,
                short_clusters: false,
                cue_time_offset: 0,
                offset_cluster: None,
                uncued: Vec::new(),
                extra_cues: Vec::new(),
                block_flags: 0x80,
                tags: Vec::new(),
                writing_app: "mkvmerge v90",
            }
        }

        fn bytes(&self) -> Vec<u8> {
            let info = el(
                &[0x15, 0x49, 0xA9, 0x66],
                &[uint_el(&[0x2A, 0xD7, 0xB1], 1_000_000), el(&[0x57, 0x41], self.writing_app.as_bytes())].concat(),
            );
            let tracks =
                el(&[0x16, 0x54, 0xAE, 0x6B], &self.tracks.iter().flat_map(TestTrack::entry).collect::<Vec<_>>());
            let tags = if self.tags.is_empty() { Vec::new() } else { el(&[0x12, 0x54, 0xC3, 0x67], &self.tags) };
            // A seek head first, as mkvmerge writes it: its positions take 8
            // bytes, whatever they are, so its size is known before them.
            let (info_id, tracks_id, cues_id, tags_id, seek_head_id) = (
                [0x15, 0x49, 0xA9, 0x66],
                [0x16, 0x54, 0xAE, 0x6B],
                [0x1C, 0x53, 0xBB, 0x6B],
                [0x12, 0x54, 0xC3, 0x67],
                [0x11, 0x4D, 0x9B, 0x74],
            );
            let seek_head = |entries: &[([u8; 4], u64)]| {
                let seeks = entries.iter().map(|(id, pos)| {
                    el(&[0x4D, 0xBB], &[el(&[0x53, 0xAB], id), uint_el(&[0x53, 0xAC], *pos)].concat())
                });
                el(&seek_head_id, &seeks.collect::<Vec<_>>().concat())
            };
            let mut listed = vec![(info_id, 0), (tracks_id, 0)];
            match self.cues_listed {
                Listed::InSeekHead => listed.push((cues_id, 0)),
                Listed::InSecondSeekHead => listed.push((seek_head_id, 0)),
                Listed::Nowhere => {}
            }
            if !tags.is_empty() {
                listed.push((tags_id, 0));
            }
            let head_len = seek_head(&listed).len() + info.len() + tracks.len();
            let cued = |number: u64| {
                self.tracks.iter().any(|t| t.number == number && t.kind == 17) && !self.uncued.contains(&number)
            };
            let cue = |time: u64, track: u64, cluster: u64, relative: u64, duration: Option<u64>| {
                let mut positions = vec![uint_el(&[0xF7], track), uint_el(&[0xF1], cluster)];
                if !self.no_relative {
                    positions.push(uint_el(&[0xF0], relative));
                }
                if let Some(duration) = duration {
                    positions.push(uint_el(&[0xB2], duration));
                }
                el(&[0xBB], &[uint_el(&[0xB3], time), el(&[0xB7], &positions.concat())].concat())
            };
            let mut clusters = Vec::new();
            let mut cue_points = Vec::new();
            for (n, (timestamp, blocks)) in self.clusters.iter().enumerate() {
                let cluster_pos = (head_len + clusters.len()) as u64;
                let mut data = uint_el(&[0xE7], *timestamp);
                let offset = if self.offset_cluster.is_none_or(|only| only == n) { self.cue_time_offset } else { 0 };
                for (k, block) in blocks.iter().enumerate() {
                    let relative = data.len() as u64;
                    let time = (*timestamp as i64 + block.1 as i64) as u64;
                    if cued(block.0) {
                        cue_points.extend(cue(time + offset, block.0, cluster_pos, relative, block.2));
                    }
                    for &(track, _, _, into) in self.extra_cues.iter().filter(|c| (c.1, c.2) == (n, k)) {
                        cue_points.extend(cue(time, track, cluster_pos, relative + into, None));
                    }
                    data.extend(block_element(block, self.block_flags));
                }
                let id = [0x1F, 0x43, 0xB6, 0x75];
                clusters.extend(if self.short_clusters { el_short(&id, &data) } else { el(&id, &data) });
            }
            // After the clusters, as mkvmerge puts them.
            let cues = el(&cues_id, &cue_points);
            let info_pos = seek_head(&listed).len() as u64;
            let tracks_pos = info_pos + info.len() as u64;
            let cues_pos = (head_len + clusters.len()) as u64;
            let tags_pos = cues_pos + cues.len() as u64;
            let second_pos = tags_pos + tags.len() as u64;
            for (id, pos) in &mut listed {
                *pos = match *id {
                    id if id == info_id => info_pos,
                    id if id == tracks_id => tracks_pos,
                    id if id == cues_id => cues_pos,
                    id if id == tags_id => tags_pos,
                    _ => second_pos,
                };
            }
            let second = if self.cues_listed == Listed::InSecondSeekHead {
                seek_head(&[(cues_id, cues_pos)])
            } else {
                Vec::new()
            };
            let segment = el(
                &[0x18, 0x53, 0x80, 0x67],
                &[seek_head(&listed), info, tracks, clusters, cues, tags, second].concat(),
            );
            [el(&[0x1A, 0x45, 0xDF, 0xA3], &[0x42, 0x86, 0x81, 0x01]), segment].concat()
        }

        pub(crate) fn write(&self, dir: &Path) -> std::path::PathBuf {
            let path = dir.join("subs.mkv");
            std::fs::write(&path, self.bytes()).unwrap();
            path
        }
    }

    /// A stream as ffprobe describes it.
    pub(crate) fn stream(index: i64, kind: StreamKind, codec: &str) -> MediaStream {
        let text_based = (kind == StreamKind::Subtitle).then(|| pley_transcoder_api::is_text_subtitle_codec(codec));
        MediaStream { index, kind, codec: codec.into(), text_based, ..Default::default() }
    }

    /// What ffprobe says of [`movie`].
    pub(crate) fn movie_media() -> MediaInfo {
        MediaInfo {
            format_name: Some("matroska,webm".into()),
            streams: vec![
                stream(0, StreamKind::Video, "h264"),
                stream(1, StreamKind::Audio, "aac"),
                stream(2, StreamKind::Subtitle, "subrip"),
                stream(3, StreamKind::Subtitle, "hdmv_pgs_subtitle"),
                stream(4, StreamKind::Subtitle, "ass"),
            ],
            ..Default::default()
        }
    }

    /// Video, audio, a button track FFmpeg leaves out, SubRip, PGS and ASS.
    pub(crate) fn movie() -> TestFile {
        let mut ass = TestTrack::new(6, 17, "S_TEXT/ASS");
        ass.private = b"[Script Info]\n".to_vec();
        TestFile::new(
            vec![
                TestTrack::new(1, 1, "V_MPEG4/ISO/AVC"),
                TestTrack::new(2, 2, "A_AAC"),
                TestTrack::new(3, 0x12, "B_VOBBTN"),
                TestTrack::new(4, 17, "S_TEXT/UTF8"),
                TestTrack::new(5, 17, "S_HDMV/PGS"),
                ass,
            ],
            vec![
                (
                    0,
                    vec![
                        (1, 0, None, vec![0; 300]),
                        (2, 0, None, vec![1; 40]),
                        (4, 1_000, Some(1_500), b"Hello".to_vec()),
                        (5, 1_000, None, vec![0x16, 0, 0]),
                        (6, 1_000, Some(800), b"0,0,Default,,0,0,0,,Hi".to_vec()),
                    ],
                ),
                (5_000, vec![(1, 0, None, vec![0; 300]), (4, -200, Some(1_000), b"Before the cluster".to_vec())]),
                (9_000, vec![(4, 2, None, b"No duration".to_vec()), (2, 10, None, vec![1; 40])]),
            ],
        )
    }

    #[test]
    fn text_tracks_are_read_through_the_cues() {
        let dir = tempfile::tempdir().unwrap();
        let path = movie().write(dir.path());
        let found = text_tracks(&path, &movie_media(), &[2, 4], &plenty()).unwrap();
        assert_eq!(found.tick_ns, 1_000_000);
        let [subrip, ass] = found.tracks.as_slice() else { panic!("two tracks") };
        assert_eq!((subrip.stream_index, subrip.codec), (2, TextCodec::Subrip), "the button track has no stream");
        let block = |time: i64, duration: u64, data: &str| TextBlock { time, duration, data: data.as_bytes().to_vec() };
        assert_eq!(
            subrip.blocks,
            [block(1_000, 1_500, "Hello"), block(4_800, 1_000, "Before the cluster"), block(9_002, 0, "No duration")]
        );
        assert_eq!((ass.codec, ass.header.as_slice()), (TextCodec::Ass, b"[Script Info]\n".as_slice()));
        assert_eq!(ass.blocks, [block(1_000, 800, "0,0,Default,,0,0,0,,Hi")]);
    }

    #[test]
    fn picture_tracks_are_read_through_the_cues() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = movie();
        // A VobSub track too, its palette in its private data, compressed
        // as mkvmerge does it.
        let mut vobsub = TestTrack::new(7, 17, "S_VOBSUB");
        vobsub.private = b"palette: 000000".to_vec();
        vobsub.extra = el(&[0x6D, 0x80], &el(&[0x62, 0x40], &el(&[0x50, 0x34], &uint_el(&[0x42, 0x54], 0))));
        file.tracks.push(vobsub);
        let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut zlib, b"vobsub picture").unwrap();
        file.clusters[1].1.push((7, 300, Some(2_000), zlib.finish().unwrap()));
        let path = file.write(dir.path());
        let mut media = movie_media();
        media.streams.push(stream(5, StreamKind::Subtitle, "dvd_subtitle"));

        let [found] =
            picture_stretches(&path, &media, &[3, 5], (0, i64::MAX)).unwrap().try_into().expect("one stretch");
        let [pgs, vobsub] = found.tracks.as_slice() else { panic!("two tracks") };
        assert_eq!((pgs.stream_index, pgs.codec), (3, "S_HDMV/PGS"));
        assert_eq!(pgs.blocks, [TextBlock { time: 1_000, duration: 0, data: vec![0x16, 0, 0] }]);
        assert_eq!((vobsub.codec, vobsub.header.as_slice()), ("S_VOBSUB", b"palette: 000000".as_slice()));
        assert_eq!(vobsub.blocks, [TextBlock { time: 5_300, duration: 2_000, data: b"vobsub picture".to_vec() }]);
        // Text isn't pictures, nor pictures text.
        assert!(picture_stretches(&path, &media, &[2], (0, i64::MAX)).unwrap_err().contains("isn't pictures"));
        assert!(text_tracks(&path, &media, &[3], &plenty()).unwrap_err().contains("isn't text"));
    }

    /// Every stretch [`picture_tracks`] hands over, read from `window` on.
    fn picture_stretches(
        path: &Path,
        media: &MediaInfo,
        wanted: &[i64],
        window: (i64, i64),
    ) -> Result<Vec<PictureTracks>, String> {
        let mut stretches = Vec::new();
        picture_tracks(path, media, wanted, &plenty(), window, |found| {
            stretches.push(found);
            Ok(())
        })?;
        Ok(stretches)
    }

    #[test]
    fn stretches_tell_what_they_hold_all_of_when_the_cues_tell_the_blocks_times() {
        let dir = tempfile::tempdir().unwrap();
        let spans = |file: &TestFile| -> Vec<Option<(u64, u64)>> {
            let path = file.write(dir.path());
            picture_stretches(&path, &movie_media(), &[3], (40_000, 50_000)).unwrap().iter().map(|s| s.span).collect()
        };
        let mut file = pictures_every_second(80);
        assert_eq!(spans(&file), [Some((39_001, 50_000)), Some((49_001, u64::MAX)), Some((0, 40_000))]);
        // Cues whose times aren't the blocks' own (read at their cluster's
        // head) tell none.
        file.cue_time_offset = 3;
        assert_eq!(spans(&file), [None, None, None]);
    }

    /// The synthetic film with a PGS picture every second for `seconds`,
    /// each a cluster of its own with some video.
    pub(crate) fn pictures_every_second(seconds: u64) -> TestFile {
        let mut file = movie();
        file.clusters = (0..seconds)
            .map(|n| (n * 1_000, vec![(1, 0, None, vec![0; 40]), (5, 0, None, vec![0x16, 0, n as u8])]))
            .collect();
        file
    }

    #[test]
    fn picture_tracks_come_from_where_the_player_is_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = pictures_every_second(80).write(dir.path());
        let seconds = |window: (i64, i64)| -> Vec<(Vec<i64>, bool)> {
            let stretches = picture_stretches(&path, &movie_media(), &[3], window).unwrap();
            let seconds = |s: &PictureTracks| s.tracks[0].blocks.iter().map(|b| b.time / 1_000).collect();
            stretches.iter().map(|s| (seconds(s), s.more)).collect()
        };
        let span = |seconds: Range<i64>, more: bool| (seconds.collect::<Vec<_>>(), more);
        // Those around where the player is, then those after, then those
        // before.
        assert_eq!(seconds((40_000, 50_000)), [span(40..50, true), span(50..80, true), span(0..40, false)]);
        // A window from the start, or one that takes it all: in one go.
        assert_eq!(seconds((0, 90_000)), [span(0..80, false)]);
        // None there: an empty window first all the same.
        assert_eq!(seconds((90_000, 95_000)), [span(0..0, true), span(0..80, false)]);
        // What takes them can stop the read.
        let stopped = picture_tracks(&path, &movie_media(), &[3], &plenty(), (0, 1), |_| Err("enough".into()));
        assert_eq!(stopped.unwrap_err(), "enough");
    }

    #[test]
    fn stretches_start_with_those_around_the_player() {
        let entries: Vec<CueEntry> = (0..100)
            .map(|n| CueEntry { time: n * 1_000, track: 5, cluster: n * 10, relative: Some(0), duration: None })
            .collect();
        let clusters = clusters_of(0, &entries).unwrap();
        assert_eq!(stretches(&clusters, (40_000, 45_500)), [40..46, 46..100, 0..40]);
        assert_eq!(stretches(&clusters, (90_000, 200_000)), [90..100, 0..90]);
        // All at once, as text is read; and an empty track is handed over too.
        let (all, none) = (stretches(&clusters, (0, u64::MAX)), stretches(&[], (0, 1)));
        assert_eq!((all.len(), all[0].clone(), none.len(), none[0].clone()), (1, 0..100, 1, 0..0));
        // A long track's after and before, a few hundred clusters at a time.
        let entries: Vec<CueEntry> = (0..1_000)
            .map(|n| CueEntry { time: n * 1_000, track: 5, cluster: n * 10, relative: Some(0), duration: None })
            .collect();
        let clusters = clusters_of(0, &entries).unwrap();
        assert_eq!(
            stretches(&clusters, (300_000, 310_000)),
            [300..310, 310..566, 566..822, 822..1000, 0..256, 256..300]
        );
        assert_eq!(stretches(&clusters, (0, u64::MAX)), vec![0..1000]);
        // What each holds all of: an empty one too.
        assert_eq!(span(&clusters, &(300..310)), (299_001, 310_000));
        assert_eq!((span(&clusters, &(0..256)), span(&clusters, &(822..1000))), ((0, 256_000), (821_001, u64::MAX)));
        assert_eq!(span(&clusters, &(5..5)), (4_001, 5_000));
        // The file's start time is the track's 0.
        let media = MediaInfo { start_ms: Some(1_000), ..Default::default() };
        assert_eq!((ticks_from(500, &media, 1_000_000), ticks_from(0, &media, 1_000_000)), (1_500, 0));
        assert_eq!(ticks_from(500, &MediaInfo::default(), 100_000), 5_000);
    }

    #[test]
    fn files_that_dont_line_up_are_left_to_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let path = movie().write(dir.path());
        // ffprobe saw other streams than the tracks say.
        let mut media = movie_media();
        media.streams[2].codec = "ass".into();
        assert!(text_tracks(&path, &media, &[2], &plenty()).unwrap_err().contains("line up"));
        let mut media = movie_media();
        media.streams.push(stream(5, StreamKind::Subtitle, "subrip"));
        assert!(text_tracks(&path, &media, &[2], &plenty()).is_err());
        // Not text, not there.
        assert!(text_tracks(&path, &movie_media(), &[3], &plenty()).unwrap_err().contains("isn't text"));
        assert!(text_tracks(&path, &movie_media(), &[9], &plenty()).is_err());
        // Cues without the blocks' places.
        let mut file = movie();
        file.no_relative = true;
        let path = file.write(dir.path());
        assert!(text_tracks(&path, &movie_media(), &[2], &plenty()).unwrap_err().contains("clusters"));
        // Not Matroska at all.
        std::fs::write(&path, b"RIFF....AVI LIST").unwrap();
        assert_eq!(text_tracks(&path, &movie_media(), &[2], &plenty()).unwrap_err(), "not a Matroska file");
        // More tracks than FFmpeg opens.
        let many = TestFile::new((1..=1_001).map(|n| TestTrack::new(n, 17, "S_TEXT/UTF8")).collect(), Vec::new());
        let err = text_tracks(&many.write(dir.path()), &MediaInfo::default(), &[0], &plenty()).unwrap_err();
        assert_eq!(err, "the file has more than 1000 streams");
    }

    fn statistics(uid: u64, frames: u64, app: &str) -> Vec<u8> {
        let simple = |name: &str, value: &str| {
            el(&[0x67, 0xC8], &[el(&[0x45, 0xA3], name.as_bytes()), el(&[0x44, 0x87], value.as_bytes())].concat())
        };
        el(
            &[0x73, 0x73],
            &[
                el(&[0x63, 0xC0], &uint_el(&[0x63, 0xC5], uid)),
                simple("NUMBER_OF_FRAMES", &frames.to_string()),
                simple("_STATISTICS_WRITING_APP", app),
            ]
            .concat(),
        )
    }

    #[test]
    fn statistics_tags_check_the_cues_list_every_block() {
        let dir = tempfile::tempdir().unwrap();
        // mkvmerge's own count agrees: read.
        let mut file = movie();
        file.tags = statistics(104, 3, "mkvmerge v90");
        assert!(text_tracks(&file.write(dir.path()), &movie_media(), &[2], &plenty()).is_ok());
        // It counted more blocks than the Cues list: FFmpeg reads the file.
        file.tags = statistics(104, 4, "mkvmerge v90");
        let err = text_tracks(&file.write(dir.path()), &movie_media(), &[2], &plenty()).unwrap_err();
        assert!(err.contains("3 of stream 2's 4 blocks"), "{err}");
        // Counts copied over from another file by FFmpeg are stale: ignored.
        file.writing_app = "Lavf62.3.100";
        assert!(text_tracks(&file.write(dir.path()), &movie_media(), &[2], &plenty()).is_ok());
    }

    /// `data` compressed with zlib.
    fn zlib(data: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        zlib.write_all(data).unwrap();
        zlib.finish().unwrap()
    }

    /// A ContentEncoding: compression `algo` (with `settings`) of what
    /// `scope` says (1: the blocks, 2: the codec private data).
    fn encoding(scope: u64, algo: u64, settings: &[u8]) -> Vec<u8> {
        let compression = [uint_el(&[0x42, 0x54], algo), el(&[0x42, 0x55], settings)].concat();
        el(&[0x62, 0x40], &[uint_el(&[0x50, 0x32], scope), el(&[0x50, 0x34], &compression)].concat())
    }

    /// A track's ContentEncodings with that one encoding.
    fn encoded(scope: u64, algo: u64, settings: &[u8]) -> Vec<u8> {
        el(&[0x6D, 0x80], &encoding(scope, algo, settings))
    }

    #[test]
    fn compressed_and_webm_tracks() {
        let squeezed = zlib(b"Squeezed");
        let encoding = |algo: u64, settings: &[u8]| encoded(1, algo, settings);
        let mut zipped = TestTrack::new(1, 17, "S_TEXT/UTF8");
        zipped.extra = encoding(0, b"");
        let mut stripped = TestTrack::new(2, 17, "S_TEXT/UTF8");
        stripped.extra = encoding(3, b"Head ");
        let webvtt = TestTrack::new(3, 17, "D_WEBVTT/SUBTITLES");
        let file = TestFile::new(
            vec![zipped, stripped, webvtt],
            vec![(
                10,
                vec![
                    (1, 0, Some(5), squeezed),
                    (2, 0, Some(5), b"less".to_vec()),
                    (3, 0, Some(5), b"cue-1\r\nline:0\r\n<i>Text</i>\r\n".to_vec()),
                    (3, 1, Some(5), b"\n\nNo id\n\n".to_vec()),
                ],
            )],
        );
        let dir = tempfile::tempdir().unwrap();
        let media = MediaInfo {
            streams: vec![
                stream(0, StreamKind::Subtitle, "subrip"),
                stream(1, StreamKind::Subtitle, "subrip"),
                stream(2, StreamKind::Subtitle, "webvtt"),
            ],
            ..Default::default()
        };
        let found = text_tracks(&file.write(dir.path()), &media, &[0, 1, 2], &plenty()).unwrap();
        let data: Vec<Vec<&[u8]>> =
            found.tracks.iter().map(|t| t.blocks.iter().map(|b| b.data.as_slice()).collect()).collect();
        assert_eq!(data, [vec![b"Squeezed".as_slice()], vec![b"Head less"], vec![b"<i>Text</i>", b"No id"]]);

        // bzip2 and LZO aren't supported here.
        let mut file = file;
        file.tracks[0].extra = encoding(1, b"");
        assert!(text_tracks(&file.write(dir.path()), &media, &[0], &plenty()).unwrap_err().contains("encoded"));

        // A block that inflates past what FFmpeg takes is refused, not cut.
        let mut zipped = TestTrack::new(1, 17, "S_TEXT/UTF8");
        zipped.extra = encoding(0, b"");
        let huge = zlib(&vec![b'a'; MAX_INFLATED as usize + 1]);
        let file = TestFile::new(vec![zipped], vec![(0, vec![(1, 0, Some(5), huge)])]);
        let media = MediaInfo { streams: media.streams[..1].to_vec(), ..Default::default() };
        let err = text_tracks(&file.write(dir.path()), &media, &[0], &plenty()).unwrap_err();
        assert!(err.contains("more than 10000000 bytes"), "{err}");
    }

    #[test]
    fn blocks_inflate_whole_or_not_at_all() {
        assert_eq!(inflate(&zlib(b"Squeezed"), 100), Ok(b"Squeezed".to_vec()));
        let squeezed = zlib(&[0; 1000]);
        assert_eq!(inflate(&squeezed, 1000).map(|out| out.len()), Ok(1000));
        // Past the cap: FFmpeg drops such a block, so it isn't cut short here.
        assert!(matches!(inflate(&squeezed, 999), Err(Undecoded::Refused(_))));
        // Cut short, damaged, or not zlib at all: FFmpeg can't read it either.
        assert!(matches!(inflate(&squeezed[..squeezed.len() - 1], 1000), Err(Undecoded::Corrupt(_))));
        let mut damaged = squeezed.clone();
        *damaged.last_mut().unwrap() ^= 1;
        assert!(matches!(inflate(&damaged, 1000), Err(Undecoded::Corrupt(_))), "the checksum");
        assert!(matches!(inflate(b"not zlib", 1000), Err(Undecoded::Corrupt(_))));
        assert!(matches!(inflate(b"", 1000), Err(Undecoded::Corrupt(_))));
    }

    #[test]
    fn compressed_headers_are_read_as_ffmpeg_reads_them() {
        let dir = tempfile::tempdir().unwrap();
        let media = MediaInfo { streams: vec![stream(0, StreamKind::Subtitle, "ass")], ..Default::default() };
        let header = b"[Script Info]\nScriptType: v4.00+\n";
        let file = |private: Vec<u8>, extra: Vec<u8>, data: Vec<u8>| {
            let mut ass = TestTrack::new(1, 17, "S_TEXT/ASS");
            (ass.private, ass.extra) = (private, extra);
            TestFile::new(vec![ass], vec![(0, vec![(1, 0, Some(5), data)])]).write(dir.path())
        };
        let read = |path: std::path::PathBuf| {
            let found = text_tracks(&path, &media, &[0], &plenty()).unwrap();
            let [track] = found.tracks.as_slice() else { panic!("one track") };
            (track.header.clone(), track.blocks[0].data.clone())
        };
        let event = b"0,0,Default,,0,0,0,,Hi".to_vec();
        // The header compressed (scope 2), the blocks as they are...
        let (got_header, data) = read(file(zlib(header), encoded(2, 0, b""), event.clone()));
        assert_eq!((got_header.as_slice(), data.as_slice()), (header.as_slice(), event.as_slice()));
        // ...or both (scope 3).
        let (got_header, data) = read(file(zlib(header), encoded(3, 0, b""), zlib(&event)));
        assert_eq!((got_header.as_slice(), data.as_slice()), (header.as_slice(), event.as_slice()));
        // Header stripping puts the bytes back in front of the header too.
        let (got_header, _) = read(file(b"Info]\n".to_vec(), encoded(3, 3, b"[Script "), b"Hi".to_vec()));
        assert_eq!(got_header, b"[Script Info]\n");
        // A header that doesn't inflate is dropped, as FFmpeg drops it.
        let (got_header, data) = read(file(b"not zlib".to_vec(), encoded(2, 0, b""), event.clone()));
        assert_eq!((got_header.as_slice(), data.as_slice()), (b"".as_slice(), event.as_slice()));
    }

    #[test]
    fn tracks_with_what_isnt_read_here_are_left_to_ffmpeg() {
        let dir = tempfile::tempdir().unwrap();
        let media = MediaInfo { streams: vec![stream(0, StreamKind::Subtitle, "subrip")], ..Default::default() };
        let read = |extra: Vec<u8>, flags: u8| {
            let mut track = TestTrack::new(1, 17, "S_TEXT/UTF8");
            track.extra = extra;
            let blocks = vec![(1, 0, Some(5), b"Hi".to_vec()), (1, 10, Some(5), Vec::new())];
            let mut file = TestFile::new(vec![track], vec![(0, blocks)]);
            file.block_flags = flags;
            text_tracks(&file.write(dir.path()), &media, &[0], &plenty())
        };
        // As it is: read, the empty block dropped, as FFmpeg drops it.
        let found = read(Vec::new(), 0x80).unwrap();
        assert_eq!(found.tracks[0].blocks, [TextBlock { time: 0, duration: 5, data: b"Hi".to_vec() }]);
        // A timestamp scale or a delay of its own.
        assert!(read(el(&[0x23, 0x31, 0x4F], &2f64.to_be_bytes()), 0x80).unwrap_err().contains("timing of its own"));
        assert!(read(uint_el(&[0x56, 0xAA], 1_000), 0x80).unwrap_err().contains("timing of its own"));
        // Encrypted, or encoded twice.
        let encrypted = el(&[0x6D, 0x80], &el(&[0x62, 0x40], &uint_el(&[0x50, 0x33], 1)));
        assert!(read(encrypted, 0x80).unwrap_err().contains("encrypted"));
        let twice = el(&[0x6D, 0x80], &[encoding(1, 3, b"a"), encoding(1, 3, b"b")].concat());
        assert!(read(twice, 0x80).unwrap_err().contains("encrypted or encoded"));
        // Laced blocks.
        assert!(read(Vec::new(), 0x82).unwrap_err().contains("laced"));
    }

    #[test]
    fn subtitles_left_out_of_the_cues() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = movie();
        // Not one listed: the muxer doesn't put them in the Cues.
        file.uncued = vec![4, 5, 6];
        let err = text_tracks(&file.write(dir.path()), &movie_media(), &[2], &plenty()).unwrap_err();
        assert_eq!(err, "the cues list no subtitles");
        // Others are listed (here the PGS track's blocks): this one has none.
        file.uncued = vec![4, 6];
        let found = text_tracks(&file.write(dir.path()), &movie_media(), &[2, 4], &plenty()).unwrap();
        assert!(found.tracks.iter().all(|t| t.blocks.is_empty()));
        assert_eq!(found.tracks[1].header, b"[Script Info]\n");
    }

    /// The least budget a read of `wanted` succeeds within.
    fn least_budget(path: &Path, media: &MediaInfo, wanted: &[i64]) -> u64 {
        let reads = |bytes: u64| text_tracks(path, media, wanted, &Budget::new(bytes, Duration::from_secs(60))).is_ok();
        let (mut low, mut high) = (0, 64 << 20);
        assert!(reads(high));
        while high - low > 1 {
            let middle = (low + high) / 2;
            if reads(middle) {
                high = middle;
            } else {
                low = middle;
            }
        }
        high
    }

    #[test]
    fn reads_stay_within_their_budget() {
        let dir = tempfile::tempdir().unwrap();
        // 400 lines of 300 bytes.
        let film = || {
            let mut file = long_film();
            for (_, blocks) in &mut file.clusters {
                blocks[1].3 = vec![b'x'; 300];
            }
            file
        };
        let path = film().write(dir.path());
        let media = long_film_media();
        let budget = |bytes: u64| Budget::new(bytes, Duration::from_secs(60));
        // A read holds the lines, a record of each and the file's info and
        // tracks (a few hundred bytes): within that, and not in one less.
        let least = least_budget(&path, &media, &[1]);
        let held = 400 * 300 + 400 * ENTRY_COST;
        assert!((held..held + 1_000).contains(&least), "{least}");
        let err = text_tracks(&path, &media, &[1], &budget(least - 1)).unwrap_err();
        assert!(err.contains("memory"), "{err}");
        // Read again with every cluster head (one block wasn't where its guess
        // put it), the blocks count once.
        let mut file = film();
        (file.cue_time_offset, file.offset_cluster) = (1, Some(256));
        let path = file.write(dir.path());
        let found = text_tracks(&path, &media, &[1], &budget(least)).unwrap();
        assert!(found.reads >= 800, "every head read: {}", found.reads);
        // Out of time.
        let late = Budget::new(u64::MAX, Duration::ZERO);
        assert!(text_tracks(&path, &media, &[1], &late).unwrap_err().contains("too long"));
    }

    #[test]
    fn every_block_listed_counts_against_the_budget() {
        let dir = tempfile::tempdir().unwrap();
        let media = long_film_media();
        // 30,000 subtitles in a cluster, every one listed in the Cues: next to
        // nothing to read, but a record to keep of each.
        for (data, read) in [(Vec::new(), 0), (b"a".to_vec(), 30_000)] {
            let blocks = (0..30_000).map(|_| (2, 0, None, data.clone())).collect();
            let file = TestFile::new(
                vec![TestTrack::new(1, 1, "V_MPEG4/ISO/AVC"), TestTrack::new(2, 17, "S_TEXT/UTF8")],
                vec![(0, blocks)],
            );
            let path = file.write(dir.path());
            // Their records alone take more than 7 MiB...
            let err = text_tracks(&path, &media, &[1], &Budget::new(7 << 20, Duration::from_secs(60))).unwrap_err();
            assert!(err.contains("memory"), "{err}");
            // ...which a read may hold.
            let found = text_tracks(&path, &media, &[1], &plenty()).unwrap();
            assert_eq!(found.tracks[0].blocks.len(), read, "the empty ones dropped");
        }
    }

    #[test]
    fn the_cues_are_found_through_the_seek_heads() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = movie();
        // A second seek head at the end lists them, the first points to it.
        file.cues_listed = Listed::InSecondSeekHead;
        assert_eq!(text_tracks(&file.write(dir.path()), &movie_media(), &[2], &plenty()).unwrap().tracks.len(), 1);
        // Listed nowhere, they are only found by going through every cluster:
        // not for subtitles (that takes as long as reading the file)...
        file.cues_listed = Listed::Nowhere;
        let path = file.write(dir.path());
        assert_eq!(text_tracks(&path, &movie_media(), &[2], &plenty()).unwrap_err(), "no cues");
        // ...only for keyframes, which come from nowhere else.
        let mut reader = BufReader::new(File::open(&path).unwrap());
        assert!(layout(&mut reader, true, None).unwrap().cues.is_some());
        assert!(layout(&mut reader, false, None).unwrap().cues.is_none());
    }

    /// Video and SubRip: in the first cluster a line whose text holds what
    /// looks like a SimpleBlock, and a video block ending with what looks
    /// like the head of one running on into the next cluster.
    fn tricky() -> TestFile {
        let hidden = el(&[0xA3], &[0x82, 0, 0, 0x80, b'x']);
        let video = [vec![7; 10], vec![0xA3, 0x40, 0x40, 0x82, 0, 0, 0x80]].concat();
        TestFile::new(
            vec![TestTrack::new(1, 1, "V_MPEG4/ISO/AVC"), TestTrack::new(2, 17, "S_TEXT/UTF8")],
            vec![
                (0, vec![(2, 100, None, [b"ab".as_slice(), &hidden].concat()), (1, 0, None, video.clone())]),
                (5_000, vec![(2, 0, None, b"Next".to_vec()), (1, 0, None, video)]),
            ],
        )
    }

    #[test]
    fn blocks_are_only_taken_where_blocks_can_be() {
        let dir = tempfile::tempdir().unwrap();
        let read = |file: &TestFile| text_tracks(&file.write(dir.path()), &long_film_media(), &[1], &plenty());
        assert_eq!(read(&tricky()).unwrap().tracks[0].blocks.len(), 2);
        // A cue into a line's text (the line's element is 29 bytes long)...
        let mut file = tricky();
        file.extra_cues = vec![(2, 0, 0, 15)];
        assert_eq!(read(&file).unwrap_err(), "a block overlaps the one before it");
        // ...into a video block, at something running past the cluster...
        file.extra_cues = vec![(2, 0, 1, 23)];
        assert_eq!(read(&file).unwrap_err(), "a block runs past its cluster");
        // ...or at the video block itself.
        file.extra_cues = vec![(2, 0, 1, 0)];
        assert_eq!(read(&file).unwrap_err(), "a cue points at another track's block");
        // A block bigger than any subtitle isn't read.
        let file = TestFile::new(
            vec![TestTrack::new(1, 1, "V_MPEG4/ISO/AVC"), TestTrack::new(2, 17, "S_TEXT/UTF8")],
            vec![(0, vec![(2, 0, None, vec![b'a'; MAX_BLOCK as usize])])],
        );
        assert_eq!(read(&file).unwrap_err(), "a block is too big for a subtitle");
    }

    /// 400 clusters of video of three sizes (heads 5 to 7 bytes long), each
    /// with a line of SubRip.
    fn long_film() -> TestFile {
        let clusters = (0..400u64)
            .map(|i| {
                let video = vec![7; [10, 1_000, 20_000][i as usize % 3]];
                (i * 1_000, vec![(1, 0, None, video), (2, 100, Some(500), format!("Line {i}").into_bytes())])
            })
            .collect();
        let mut file = TestFile::new(
            vec![TestTrack::new(1, 1, "V_MPEG4/ISO/AVC"), TestTrack::new(2, 17, "S_TEXT/UTF8")],
            clusters,
        );
        file.short_clusters = true;
        file
    }

    fn long_film_media() -> MediaInfo {
        MediaInfo {
            streams: vec![stream(0, StreamKind::Video, "h264"), stream(1, StreamKind::Subtitle, "subrip")],
            ..Default::default()
        }
    }

    fn lines(found: &TextTracks) -> Vec<(i64, u64, String)> {
        found.tracks[0]
            .blocks
            .iter()
            .map(|b| (b.time, b.duration, String::from_utf8_lossy(&b.data).into_owned()))
            .collect()
    }

    #[test]
    fn blocks_are_found_without_their_cluster_heads() {
        let dir = tempfile::tempdir().unwrap();
        let path = long_film().write(dir.path());
        let found = text_tracks(&path, &long_film_media(), &[1], &plenty()).unwrap();
        let expected: Vec<(i64, u64, String)> = (0..400).map(|i| (i * 1_000 + 100, 500, format!("Line {i}"))).collect();
        assert_eq!(lines(&found), expected);
        assert!(found.reads < 500, "one read a block, and a head now and then: {}", found.reads);

        // Cue times that aren't the blocks' are caught: every head is read.
        let mut file = long_film();
        file.cue_time_offset = 1;
        let path = file.write(dir.path());
        let found = text_tracks(&path, &long_film_media(), &[1], &plenty()).unwrap();
        assert_eq!(lines(&found), expected, "the blocks' own times");
        assert!(found.reads >= 800, "{}", found.reads);
    }

    #[test]
    fn pictures_are_found_without_their_cluster_heads() {
        let dir = tempfile::tempdir().unwrap();
        // PGS in SimpleBlocks, cued without durations (as mkvmerge writes
        // them), one in three bigger than a small read, in clusters of video
        // of three sizes.
        let film = || {
            let mut file = long_film();
            file.tracks[1] = TestTrack::new(2, 17, "S_HDMV/PGS");
            for (i, (_, blocks)) in file.clusters.iter_mut().enumerate() {
                let size = [3, 2_000, 40_000][i % 3];
                blocks[1] = (2, 100, None, [vec![0x16, 0, 0], vec![i as u8; size]].concat());
            }
            file
        };
        let media = MediaInfo {
            streams: vec![stream(0, StreamKind::Video, "h264"), stream(1, StreamKind::Subtitle, "hdmv_pgs_subtitle")],
            ..Default::default()
        };
        let read = |file: &TestFile| {
            let path = file.write(dir.path());
            let mut found = Vec::new();
            picture_tracks(&path, &media, &[1], &plenty(), (0, i64::MAX), |stretch| {
                found.push(stretch);
                Ok(())
            })
            .unwrap();
            let [found]: [PictureTracks; 1] = found.try_into().expect("one stretch");
            found
        };
        let found = read(&film());
        let times: Vec<i64> = found.tracks[0].blocks.iter().map(|b| b.time).collect();
        assert_eq!(times, (0..400).map(|i| i * 1_000 + 100).collect::<Vec<_>>());
        assert_eq!(found.tracks[0].blocks[2].data.len(), 3 + 40_000);
        assert!(found.reads < 500, "one read a picture, and a head now and then: {}", found.reads);

        // Cue times that aren't the blocks' are caught: every head is read.
        let mut file = film();
        file.cue_time_offset = 1;
        let found = read(&file);
        assert_eq!(found.tracks[0].blocks.iter().map(|b| b.time).collect::<Vec<_>>(), times, "the blocks' own times");
        // A head and its picture, but for the smallest clusters, whose head
        // read holds their picture.
        assert!(found.reads > 600, "{}", found.reads);
    }

    #[test]
    fn simple_blocks_are_told_from_chance() {
        let simple =
            |track: u8, flags: u8| el_short(&[0xA3], &[vec![0x80 | track, 0x00, 0x64, flags], vec![0x16; 4]].concat());
        let block = simple_block_at(&simple(2, 0x80), 2).unwrap();
        assert_eq!((block.track, block.timecode, block.data.as_slice()), (2, 100, [0x16; 4].as_slice()));
        assert!(simple_block_at(&simple(3, 0x80), 2).is_none(), "another track");
        assert!(simple_block_at(&simple(2, 0x82), 2).is_none(), "laced");
        assert!(simple_block_at(&simple(2, 0x80)[1..], 2).is_none(), "off by one");
        let mut cut = simple(2, 0x80);
        cut.pop();
        assert!(simple_block_at(&cut, 2).is_none(), "cut short");
    }

    #[test]
    fn block_groups_are_told_from_chance() {
        let group = |track: u8, duration: u8, extra: &[u8]| {
            let block = [vec![0x80 | track, 0x00, 0x64, 0x80], b"Hi".to_vec()].concat();
            el_short(&[0xA0], &[el_short(&[0xA1], &block), el_short(&[0x9B], &[duration]), extra.to_vec()].concat())
        };
        let block = block_group_at(&group(2, 50, b""), 2, 50).unwrap();
        assert_eq!((block.track, block.timecode, block.data.as_slice()), (2, 100, b"Hi".as_slice()));
        assert!(block_group_at(&group(3, 50, b""), 2, 50).is_none(), "another track");
        assert!(block_group_at(&group(2, 60, b""), 2, 50).is_none(), "another duration");
        assert!(block_group_at(&group(2, 50, &el_short(&[0xFB], &[1])), 2, 50).is_some(), "with a reference");
        assert!(block_group_at(&group(2, 50, &el_short(&[0x86], b"x")), 2, 50).is_none(), "something else inside");
        assert!(block_group_at(&group(2, 50, b"")[1..], 2, 50).is_none(), "off by one");
        let mut cut = group(2, 50, b"");
        cut.pop();
        assert!(block_group_at(&cut, 2, 50).is_none(), "cut short");
    }

    #[test]
    fn webm_cue_text_is_taken_as_ffmpeg_does() {
        assert_eq!(webm_cue_text(b"id\nsettings\ntext\n"), Some(b"text".as_slice()));
        assert_eq!(webm_cue_text(b"\r\n\r\nline\r\nline 2\r\n"), Some(b"line\r\nline 2".as_slice()));
        assert_eq!(webm_cue_text(b"only text"), None);
    }
}

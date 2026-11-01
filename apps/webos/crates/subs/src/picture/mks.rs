//! The Matroska files the server makes of a picture subtitle track: one
//! track of PGS, VobSub or DVB copied as it is, with the original's times
//! from the start of the file. Only what that needs is read: the track's
//! codec and private data, and every block with its time and duration.
//!
//! A track still being copied out of its video (`X-Pley-Complete: 0`) comes
//! as far as it is written, up to its last whole cluster; the server serves
//! the file with `Range`, so [`MksReader`] reads it as it grows from the new
//! bytes alone (`RES/tv-player-logic.md §18.2`: the TV app fetched the whole
//! file every time). Block data isn't copied: samples share the answers'
//! buffers.

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

const EBML: u64 = 0x1A45DFA3;
const SEGMENT: u64 = 0x18538067;
const INFO: u64 = 0x1549A966;
const TIMECODE_SCALE: u64 = 0x2AD7B1;
const TRACKS: u64 = 0x1654AE6B;
const TRACK_ENTRY: u64 = 0xAE;
const TRACK_NUMBER: u64 = 0xD7;
const TRACK_TYPE: u64 = 0x83;
const CODEC_ID: u64 = 0x86;
const CODEC_PRIVATE: u64 = 0x63A2;
const CONTENT_ENCODINGS: u64 = 0x6D80;
const CONTENT_ENCODING: u64 = 0x6240;
const CONTENT_COMPRESSION: u64 = 0x5034;
const CONTENT_COMP_ALGO: u64 = 0x4254;
const CONTENT_COMP_SETTINGS: u64 = 0x4255;
const CLUSTER: u64 = 0x1F43B675;
const TIMECODE: u64 = 0xE7;
const SIMPLE_BLOCK: u64 = 0xA3;
const BLOCK_GROUP: u64 = 0xA0;
const BLOCK: u64 = 0xA1;
const BLOCK_DURATION: u64 = 0x9B;

/// Elements whose children are read; everything else is skipped by its size.
const MASTERS: [u64; 8] =
    [INFO, TRACKS, TRACK_ENTRY, CONTENT_ENCODINGS, CONTENT_ENCODING, CONTENT_COMPRESSION, CLUSTER, BLOCK_GROUP];

/// The Segment's children (SeekHead, Info, Tracks, Cluster, Cues, Chapters,
/// Tags, Attachments): where an element of unknown size inside one ends.
const LEVEL_ONE: [u64; 8] = [0x114D9B74, INFO, TRACKS, CLUSTER, 0x1C53BB6B, 0x1043A770, 0x1254C367, 0x1941A469];

/// Matroska's track type for subtitles.
const SUBTITLE_TRACK: u64 = 0x11;

/// Bytes a request for the rest of a file asks for again, so the answer
/// shows it continues the file already held.
const OVERLAP: u64 = 16;

/// Why a file can't be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MksError {
    NotMatroska,
    NoTrack,
    /// The frames are compressed (zlib), which the server never does.
    Compressed,
}

impl fmt::Display for MksError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            MksError::NotMatroska => "not a Matroska file",
            MksError::NoTrack => "no track",
            MksError::Compressed => "compressed subtitle frames",
        })
    }
}

impl std::error::Error for MksError {}

/// One sample of the track: a PGS display set, a VobSub subpicture or a DVB
/// display set.
#[derive(Clone)]
pub struct PictureSample {
    /// When it shows, in ms from the start of the file.
    pub time_ms: u64,
    /// From the block group; `None` when the sample shows until the next one.
    pub duration_ms: Option<u64>,
    buf: Arc<Vec<u8>>,
    range: Range<usize>,
}

impl PictureSample {
    /// A sample of its own bytes.
    pub fn new(time_ms: u64, duration_ms: Option<u64>, data: Vec<u8>) -> PictureSample {
        let range = 0..data.len();
        PictureSample { time_ms, duration_ms, buf: Arc::new(data), range }
    }

    /// The sample's bytes.
    pub fn data(&self) -> &[u8] {
        &self.buf[self.range.clone()]
    }

    /// The same sample (the same bytes of the same answer), not just equal ones.
    pub(crate) fn same(&self, o: &PictureSample) -> bool {
        self.time_ms == o.time_ms
            && self.duration_ms == o.duration_ms
            && Arc::ptr_eq(&self.buf, &o.buf)
            && self.range == o.range
    }
}

impl fmt::Debug for PictureSample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PictureSample({} ms, {:?}, {} bytes)", self.time_ms, self.duration_ms, self.range.len())
    }
}

impl PartialEq for PictureSample {
    fn eq(&self, o: &PictureSample) -> bool {
        self.time_ms == o.time_ms && self.duration_ms == o.duration_ms && self.data() == o.data()
    }
}

/// The subtitle track of a `.mks`: its codec, private data and samples
/// sorted by time.
#[derive(Clone, Debug, PartialEq)]
pub struct PictureTrack {
    /// `S_HDMV/PGS`, `S_VOBSUB` or `S_DVBSUB`.
    pub codec_id: String,
    /// VobSub's `.idx` header; nothing for the others.
    pub codec_private: Option<Vec<u8>>,
    pub samples: Vec<PictureSample>,
}

/// Reads a whole `.mks`, or as much of one as there is: a file cut short
/// gives the samples before the cut.
pub fn read_picture_track(bytes: Vec<u8>) -> Result<PictureTrack, MksError> {
    let mut reader = MksReader::new();
    reader.feed_whole(bytes)?;
    reader.finish()
}

#[derive(Clone, Debug, Default)]
struct TrackEntry {
    number: u64,
    kind: u64,
    codec_id: String,
    codec_private: Option<Vec<u8>>,
    /// Bytes stripped from the start of every frame (ContentCompAlgo 3).
    stripped: Option<Vec<u8>>,
    /// Compressed with something this reader can't undo (zlib).
    compressed: bool,
}

#[derive(Clone)]
struct RawBlock {
    track: u64,
    /// In the file's ticks.
    time: i64,
    duration: Option<u64>,
    buf: Arc<Vec<u8>>,
    range: Range<usize>,
}

/// A `.mks` read as its bytes arrive: the start of the file, then the rest
/// as it is written.
///
/// Elements the file holds whole are read for good; the cluster the bytes
/// end in is read as far as its blocks are whole, and again once more of it
/// arrives. Track header, clusters and samples are what the server's files
/// hold (`apps/server/src/pgs.rs`); everything else is skipped by its size.
#[derive(Clone)]
pub struct MksReader {
    /// The last answer's bytes, or what remained unread of the one before and
    /// this answer's new bytes: file offsets `base..base + len`.
    buf: Arc<Vec<u8>>,
    base: u64,
    /// Where reading resumes: the first element not read for good.
    pos: u64,
    in_segment: bool,
    /// The Segment's end; `None` while its size is unknown (being written).
    segment_end: Option<u64>,
    scale_ns: u64,
    tracks: Vec<TrackEntry>,
    blocks: Vec<RawBlock>,
    /// The blocks of the cluster the bytes end in.
    provisional: Vec<RawBlock>,
    seen_cluster: bool,
}

impl Default for MksReader {
    fn default() -> MksReader {
        MksReader::new()
    }
}

impl MksReader {
    /// A reader that has read nothing.
    pub fn new() -> MksReader {
        MksReader {
            buf: Arc::new(Vec::new()),
            base: 0,
            pos: 0,
            in_segment: false,
            segment_end: None,
            scale_ns: 1_000_000,
            tracks: Vec::new(),
            blocks: Vec::new(),
            provisional: Vec::new(),
            seen_cluster: false,
        }
    }

    /// Reads a whole answer: the file from its start, as far as it goes.
    /// Whatever was read before is forgotten.
    pub fn feed_whole(&mut self, bytes: Vec<u8>) -> Result<(), MksError> {
        *self = MksReader::new();
        self.buf = Arc::new(bytes);
        self.parse()
    }

    /// Reads an answer to a request for the rest ([`MksReader::resume_from`]):
    /// the file's bytes from `start`. `Ok(false)` when they don't continue
    /// the file held (it changed on the server): ask for the whole file.
    pub fn feed_range(&mut self, start: u64, bytes: Vec<u8>) -> Result<bool, MksError> {
        let held_end = self.held_end();
        if start < self.base || start > held_end {
            return Ok(false);
        }
        let overlap = (held_end - start) as usize;
        let held = &self.buf[(start - self.base) as usize..];
        let shared = overlap.min(bytes.len());
        if bytes[..shared] != held[..shared] {
            return Ok(false);
        }
        if bytes.len() <= overlap {
            return Ok(true);
        }
        if self.pos < held_end {
            // The cluster the last answer ended in is read again with its rest.
            let mut joined = self.buf[(self.pos - self.base) as usize..].to_vec();
            joined.extend_from_slice(&bytes[overlap..]);
            self.base = self.pos;
            self.buf = Arc::new(joined);
        } else {
            self.base = start;
            self.buf = Arc::new(bytes);
        }
        self.parse()?;
        Ok(true)
    }

    /// Where a request for the rest of the file starts (`Range: bytes=<n>-`):
    /// a few bytes before the end of what is held, which the answer must
    /// repeat. `None` before any cluster: ask for the whole file again, as
    /// the server's first answers may be a stand-in header.
    pub fn resume_from(&self) -> Option<u64> {
        self.seen_cluster.then(|| self.held_end().saturating_sub(OVERLAP).max(self.base))
    }

    /// The file's bytes held so far.
    pub fn held(&self) -> u64 {
        self.held_end()
    }

    /// The track as read so far; `None` before its header.
    pub fn track(&self) -> Result<Option<PictureTrack>, MksError> {
        let Some(t) = self.tracks.iter().find(|t| t.kind == SUBTITLE_TRACK).or(self.tracks.first()) else {
            return Ok(None);
        };
        if t.compressed {
            return Err(MksError::Compressed);
        }
        let scale = i128::from(self.scale_ns);
        let ms = |ticks: i128| (ticks * scale + 500_000).div_euclid(1_000_000);
        let mut samples: Vec<PictureSample> = self
            .blocks
            .iter()
            .chain(&self.provisional)
            .filter(|b| b.track == t.number)
            .map(|b| {
                let time_ms = ms(i128::from(b.time)).clamp(0, i128::from(u64::MAX)) as u64;
                let duration_ms = b.duration.map(|d| ms(i128::from(d)).clamp(0, i128::from(u64::MAX)) as u64);
                match &t.stripped {
                    Some(prefix) => {
                        let mut data = prefix.clone();
                        data.extend_from_slice(&b.buf[b.range.clone()]);
                        PictureSample::new(time_ms, duration_ms, data)
                    }
                    None => PictureSample { time_ms, duration_ms, buf: b.buf.clone(), range: b.range.clone() },
                }
            })
            .collect();
        samples.sort_by_key(|s| s.time_ms);
        Ok(Some(PictureTrack { codec_id: t.codec_id.clone(), codec_private: t.codec_private.clone(), samples }))
    }

    /// The track, taking what is held as the whole file.
    pub fn finish(&self) -> Result<PictureTrack, MksError> {
        if !self.in_segment {
            return Err(MksError::NotMatroska);
        }
        self.track()?.ok_or(MksError::NoTrack)
    }

    fn held_end(&self) -> u64 {
        self.base + self.buf.len() as u64
    }

    fn parse(&mut self) -> Result<(), MksError> {
        self.provisional.clear();
        let view = View { buf: self.buf.clone(), base: self.base };
        let held_end = view.end();
        if !self.in_segment {
            if self.pos == 0 && view.byte(0) == Some(0) {
                return Err(MksError::NotMatroska);
            }
            loop {
                let Some(e) = view.element(self.pos, held_end) else { return Ok(()) };
                if self.pos == 0 && e.id != EBML {
                    return Err(MksError::NotMatroska);
                }
                if e.id == SEGMENT {
                    self.in_segment = true;
                    self.segment_end = e.size.map(|s| e.data_start.saturating_add(s));
                    self.pos = e.data_start;
                    break;
                }
                let Some(size) = e.size else { return Err(MksError::NotMatroska) };
                self.pos = e.data_start.saturating_add(size);
                if self.pos > held_end {
                    return Ok(());
                }
            }
        }
        loop {
            let end = self.segment_end.map_or(held_end, |s| s.min(held_end));
            if self.pos >= end {
                return Ok(());
            }
            let Some(e) = view.element(self.pos, end) else { return Ok(()) };
            let el_end = e.size.map(|s| e.data_start.saturating_add(s));
            if !matches!(e.id, INFO | TRACKS | CLUSTER) {
                // Skipped by its size; one of unknown size can't be, and ends what is read.
                self.pos = el_end.unwrap_or(end);
                if self.pos > held_end || el_end.is_none() {
                    return Ok(());
                }
                continue;
            }
            if e.id == CLUSTER {
                self.seen_cluster = true;
            }
            let mut w = Walker::default();
            w.open(e.id);
            let stop = w.walk(&view, e.data_start, el_end.map_or(end, |x| x.min(end)), e.size.is_none());
            let next = match (stop, el_end) {
                (Stop::LevelOne(q), _) => Some(q),
                (Stop::End, Some(x)) if x <= held_end => Some(x),
                (Stop::End, None) if self.segment_end.is_some_and(|s| s <= held_end) => Some(end),
                _ => None,
            };
            let Some(next) = next else {
                // Cut short: the rest comes later. A cluster's whole blocks show meanwhile.
                if e.id == CLUSTER {
                    self.provisional = w.blocks;
                }
                return Ok(());
            };
            match e.id {
                INFO => self.scale_ns = w.scale_ns.unwrap_or(self.scale_ns),
                TRACKS => self.tracks = w.tracks,
                _ => self.blocks.append(&mut w.blocks),
            }
            self.pos = next;
        }
    }
}

/// The bytes held, at file offsets.
struct View {
    buf: Arc<Vec<u8>>,
    base: u64,
}

struct Element {
    id: u64,
    /// `None`: unknown size (being written).
    size: Option<u64>,
    data_start: u64,
}

impl View {
    fn end(&self) -> u64 {
        self.base + self.buf.len() as u64
    }

    fn byte(&self, at: u64) -> Option<u8> {
        at.checked_sub(self.base).and_then(|i| self.buf.get(i as usize)).copied()
    }

    fn slice(&self, from: u64, to: u64) -> &[u8] {
        let to = to.min(self.end());
        let from = from.min(to);
        &self.buf[(from - self.base) as usize..(to - self.base) as usize]
    }

    /// The element whose header starts at `at` and ends by `end`.
    fn element(&self, at: u64, end: u64) -> Option<Element> {
        let (id, id_len, _) = self.vint(at, false)?;
        if at + id_len > end {
            return None;
        }
        let (size, size_len, unknown) = self.vint(at + id_len, true)?;
        let data_start = at + id_len + size_len;
        (data_start <= end).then_some(Element { id, size: (!unknown).then_some(size), data_start })
    }

    /// An EBML variable-length integer at `at`: an element id keeps its
    /// length marker (`strip` false), a size or track number loses it. The
    /// value, its length, and whether it is all ones (an unknown size).
    fn vint(&self, at: u64, strip: bool) -> Option<(u64, u64, bool)> {
        let first = self.byte(at)?;
        if first == 0 {
            return None;
        }
        let length = u64::from(first.leading_zeros()) + 1;
        if at + length > self.end() {
            return None;
        }
        let mask = 0x80u8 >> (length - 1);
        let bits = first & mask.wrapping_sub(1);
        let mut value = u64::from(if strip { bits } else { first });
        let mut all_ones = bits == mask.wrapping_sub(1);
        for i in 1..length {
            let b = self.byte(at + i)?;
            all_ones &= b == 0xFF;
            value = (value << 8) | u64::from(b);
        }
        Some((value, length, strip && all_ones))
    }
}

enum Stop {
    /// Read to the end given.
    End,
    /// An element of unknown size ended where a Segment child starts.
    LevelOne(u64),
    /// The bytes ran out first.
    Cut,
}

/// The state of reading one of the Segment's children, as the TypeScript
/// reader's walk kept it.
#[derive(Default)]
struct Walker {
    scale_ns: Option<u64>,
    tracks: Vec<TrackEntry>,
    track: Option<TrackEntry>,
    comp_algo: u64,
    comp_settings: Option<Vec<u8>>,
    cluster_time: u64,
    group_block: Option<RawBlock>,
    group_duration: Option<u64>,
    blocks: Vec<RawBlock>,
}

impl Walker {
    /// Reads the elements from `at` to `end` (within the bytes held); with
    /// `scan`, an element of unknown size, up to the next Segment child.
    fn walk(&mut self, view: &View, mut at: u64, end: u64, scan: bool) -> Stop {
        while at < end {
            let Some(e) = view.element(at, end) else { return Stop::Cut };
            if scan && LEVEL_ONE.contains(&e.id) {
                return Stop::LevelOne(at);
            }
            let master = MASTERS.contains(&e.id);
            let el_end = e.size.map_or(end, |s| e.data_start.saturating_add(s));
            // A file still being written ends inside an element: a block cut
            // short isn't read as a sample.
            if e.size.is_some() && el_end > view.end() && !master {
                return Stop::Cut;
            }
            self.open(e.id);
            if master {
                match self.walk(view, e.data_start, el_end.min(end), e.size.is_none()) {
                    Stop::Cut => return Stop::Cut,
                    Stop::LevelOne(q) => {
                        self.close(e.id);
                        at = q;
                        continue;
                    }
                    Stop::End if el_end > view.end() => return Stop::Cut,
                    Stop::End => {}
                }
            } else {
                self.leaf(e.id, view, e.data_start, el_end.min(end));
            }
            self.close(e.id);
            at = el_end;
        }
        Stop::End
    }

    fn open(&mut self, id: u64) {
        match id {
            CLUSTER => self.cluster_time = 0,
            TRACK_ENTRY => self.track = Some(TrackEntry::default()),
            CONTENT_COMPRESSION => {
                self.comp_algo = 0;
                self.comp_settings = None;
            }
            _ => {}
        }
    }

    fn close(&mut self, id: u64) {
        match id {
            CONTENT_COMPRESSION => {
                if let Some(t) = &mut self.track {
                    if self.comp_algo == 3 {
                        t.stripped = self.comp_settings.clone();
                    } else {
                        t.compressed = true;
                    }
                }
            }
            TRACK_ENTRY => self.tracks.extend(self.track.take()),
            BLOCK_GROUP => {
                if let Some(mut b) = self.group_block.take() {
                    b.duration = self.group_duration;
                    self.blocks.push(b);
                }
                self.group_duration = None;
            }
            _ => {}
        }
    }

    fn leaf(&mut self, id: u64, view: &View, start: u64, end: u64) {
        let body = view.slice(start, end);
        match id {
            TIMECODE_SCALE => self.scale_ns = Some(Some(uint(body)).filter(|s| *s != 0).unwrap_or(1_000_000)),
            TRACK_NUMBER | TRACK_TYPE | CODEC_ID | CODEC_PRIVATE => {
                let Some(t) = &mut self.track else { return };
                match id {
                    TRACK_NUMBER => t.number = uint(body),
                    TRACK_TYPE => t.kind = uint(body),
                    CODEC_ID => t.codec_id = body.iter().take_while(|b| **b != 0).map(|b| *b as char).collect(),
                    _ => t.codec_private = Some(body.to_vec()),
                }
            }
            CONTENT_COMP_ALGO => self.comp_algo = uint(body),
            CONTENT_COMP_SETTINGS => self.comp_settings = Some(body.to_vec()),
            TIMECODE => self.cluster_time = uint(body),
            SIMPLE_BLOCK => self.blocks.extend(self.block(view, start, end)),
            BLOCK => self.group_block = self.block(view, start, end),
            BLOCK_DURATION => self.group_duration = Some(uint(body)),
            _ => {}
        }
    }

    /// A block's track, time and its one frame (subtitles are never laced).
    fn block(&self, view: &View, start: u64, end: u64) -> Option<RawBlock> {
        let (track, len, _) = view.vint(start, true)?;
        let header = start + len + 3;
        if header > end {
            return None;
        }
        let relative = i16::from_be_bytes([view.byte(start + len)?, view.byte(start + len + 1)?]);
        let flags = view.byte(start + len + 2)?;
        let mut from = header;
        if (flags >> 1) & 3 != 0 && from < end {
            // Laced subtitle frames don't occur in practice: the lace count
            // goes, the frames' bytes stay as they are.
            from += 1;
        }
        Some(RawBlock {
            track,
            time: self.cluster_time as i64 + i64::from(relative),
            duration: None,
            buf: view.buf.clone(),
            range: (from - view.base) as usize..(end - view.base) as usize,
        })
    }
}

/// A big-endian unsigned integer.
fn uint(body: &[u8]) -> u64 {
    body.iter().fold(0u64, |v, b| v.wrapping_shl(8) | u64::from(*b))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::PictureSubtitles;
    use super::super::tests::{empty_set, u16b, white_set};
    use super::*;

    /// An EBML element with an 8-byte size, which keeps the builder simple.
    pub fn el(id: u64, body: &[u8]) -> Vec<u8> {
        let mut out: Vec<u8> = id.to_be_bytes().into_iter().skip_while(|b| *b == 0).collect();
        out.push(0x01);
        out.extend(&(body.len() as u64).to_be_bytes()[1..]);
        out.extend(body);
        out
    }

    /// An element of unknown size, as one being written.
    fn el_unknown(id: u64, body: &[u8]) -> Vec<u8> {
        let mut out: Vec<u8> = id.to_be_bytes().into_iter().skip_while(|b| *b == 0).collect();
        out.extend([0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        out.extend(body);
        out
    }

    pub struct Block {
        pub time: u32,
        pub data: Vec<u8>,
        pub duration: Option<u32>,
    }

    fn ebml() -> Vec<u8> {
        el(EBML, &el(0x4282, b"matroska"))
    }

    /// The Segment's Info and Tracks for one subtitle track, its entry ending with `extra`.
    fn info_and_tracks(codec_id: &str, codec_private: Option<&[u8]>, extra: &[u8]) -> Vec<u8> {
        let mut entry = el(TRACK_NUMBER, &[1]);
        entry.extend(el(TRACK_TYPE, &[0x11]));
        entry.extend(el(CODEC_ID, codec_id.as_bytes()));
        if let Some(p) = codec_private {
            entry.extend(el(CODEC_PRIVATE, p));
        }
        entry.extend(extra);
        let mut out = el(INFO, &el(TIMECODE_SCALE, &[0x0F, 0x42, 0x40]));
        out.extend(el(TRACKS, &el(TRACK_ENTRY, &entry)));
        out
    }

    /// The start of a `.mks` being written: its Segment's size unknown, no cluster yet.
    pub fn header(codec_id: &str, codec_private: Option<&[u8]>) -> Vec<u8> {
        let mut out = ebml();
        out.extend(el_unknown(SEGMENT, &[]));
        out.extend(info_and_tracks(codec_id, codec_private, &[]));
        out
    }

    pub fn cluster(time: u32, blocks: &[Block]) -> Vec<u8> {
        let mut body = el(TIMECODE, &u16b(time));
        for b in blocks {
            let mut block = vec![0x81];
            block.extend(u16b(b.time - time));
            match b.duration {
                Some(d) => {
                    block.push(0x00);
                    block.extend(&b.data);
                    let mut group = el(BLOCK, &block);
                    group.extend(el(BLOCK_DURATION, &u16b(d)));
                    body.extend(el(BLOCK_GROUP, &group));
                }
                None => {
                    block.push(0x80);
                    block.extend(&b.data);
                    body.extend(el(SIMPLE_BLOCK, &block));
                }
            }
        }
        el(CLUSTER, &body)
    }

    /// A `.mks` with its Segment's size known, as the TypeScript tests build it.
    pub fn mks(codec_id: &str, blocks: &[Block], codec_private: Option<&[u8]>) -> Vec<u8> {
        mks_with(codec_id, blocks, codec_private, &[])
    }

    fn mks_with(codec_id: &str, blocks: &[Block], codec_private: Option<&[u8]>, extra: &[u8]) -> Vec<u8> {
        let mut body = info_and_tracks(codec_id, codec_private, extra);
        body.extend(cluster(0, blocks));
        let mut out = ebml();
        out.extend(el(SEGMENT, &body));
        out
    }

    fn three() -> Vec<Block> {
        vec![
            Block { time: 1000, data: white_set(), duration: None },
            Block { time: 3000, data: empty_set(), duration: None },
            Block { time: 5000, data: white_set(), duration: Some(1500) },
        ]
    }

    fn times(track: &PictureTrack) -> Vec<(u64, Option<u64>)> {
        track.samples.iter().map(|s| (s.time_ms, s.duration_ms)).collect()
    }

    #[test]
    fn reads_the_track_its_samples_times_and_durations() {
        let track = read_picture_track(mks("S_HDMV/PGS", &three(), None)).unwrap();
        assert_eq!(track.codec_id, "S_HDMV/PGS");
        assert_eq!(times(&track), [(1000, None), (3000, None), (5000, Some(1500))]);
        let mut subs = PictureSubtitles::of(track).unwrap();
        assert!(subs.frame_at(500).is_none());
        assert_eq!(subs.frame_at(1500).unwrap().0, 0);
        assert!(subs.frame_at(3200).is_none());
        assert_eq!(subs.frame_at(6000).unwrap().0, 2);
        assert!(subs.frame_at(6600).is_none());
        assert_eq!(subs.next_change(1500), Some(3000));
        assert_eq!(subs.next_change(4000), Some(5000));
        assert_eq!(subs.next_change(5100), Some(6500));
    }

    #[test]
    fn reads_a_file_cut_short_up_to_the_cut_without_the_sample_cut_in_two() {
        let set = white_set();
        let blocks = [
            Block { time: 1000, data: set.clone(), duration: None },
            Block { time: 3000, data: empty_set(), duration: None },
            Block { time: 5000, data: set.clone(), duration: None },
        ];
        let whole = mks("S_HDMV/PGS", &blocks, None);
        let cut = whole[..whole.len() - set.len() / 2].to_vec();
        let track = read_picture_track(cut).unwrap();
        assert_eq!(times(&track), [(1000, None), (3000, None)]);
        assert_eq!(track.samples[0].data(), set.as_slice());
        // Cut inside the track list: no track yet.
        assert_eq!(read_picture_track(whole[..60].to_vec()), Err(MksError::NoTrack));
    }

    #[test]
    fn refuses_what_isnt_matroska() {
        assert_eq!(read_picture_track(vec![1, 2, 3, 4]), Err(MksError::NotMatroska));
        assert_eq!(read_picture_track(vec![0, 0, 0, 0]), Err(MksError::NotMatroska));
        assert_eq!(read_picture_track(el(0x1A45DFA4, b"x")), Err(MksError::NotMatroska));
        assert_eq!(MksError::Compressed.to_string(), "compressed subtitle frames");
    }

    #[test]
    fn strips_headers_and_refuses_compressed_frames() {
        let with_encoding = |algo: u8| {
            let mut compression = el(CONTENT_COMP_ALGO, &[algo]);
            compression.extend(el(CONTENT_COMP_SETTINGS, &[0x16, 0x00]));
            let encodings = el(CONTENT_ENCODINGS, &el(CONTENT_ENCODING, &el(CONTENT_COMPRESSION, &compression)));
            let blocks = [Block { time: 10, data: vec![0x05, 0x01], duration: None }];
            read_picture_track(mks_with("S_HDMV/PGS", &blocks, None, &encodings))
        };
        let track = with_encoding(3).unwrap();
        assert_eq!(track.samples[0].data(), [0x16, 0x00, 0x05, 0x01]);
        assert_eq!(with_encoding(0), Err(MksError::Compressed));
    }

    #[test]
    fn reads_clusters_of_unknown_size_and_a_file_as_it_grows() {
        let first = cluster(0, &[Block { time: 1000, data: white_set(), duration: None }]);
        let second = cluster(2000, &[Block { time: 3000, data: empty_set(), duration: None }]);
        // An unknown-size cluster ends where the next starts.
        let mut file = header("S_HDMV/PGS", None);
        file.extend(el_unknown(CLUSTER, &first[12..]));
        file.extend(&second);
        assert_eq!(times(&read_picture_track(file.clone()).unwrap()), [(1000, None), (3000, None)]);

        // The same file arriving in three answers: the header, then each cluster.
        let head = header("S_HDMV/PGS", None);
        let mut reader = MksReader::new();
        reader.feed_whole(head.clone()).unwrap();
        assert_eq!(reader.track().unwrap().unwrap().samples.len(), 0);
        assert_eq!(reader.resume_from(), None, "before any cluster, the whole file again");
        let mut so_far = head.clone();
        so_far.extend(&first);
        reader.feed_whole(so_far.clone()).unwrap();
        let from = reader.resume_from().unwrap();
        assert_eq!(from, so_far.len() as u64 - 16);
        let mut whole = so_far.clone();
        whole.extend(&second);
        assert!(reader.feed_range(from, whole[from as usize..].to_vec()).unwrap());
        assert_eq!(times(&reader.track().unwrap().unwrap()), [(1000, None), (3000, None)]);
        // Nothing new yet: the answer is the overlap alone.
        let from = reader.resume_from().unwrap();
        assert!(reader.feed_range(from, whole[from as usize..].to_vec()).unwrap());
        assert_eq!(reader.track().unwrap().unwrap().samples.len(), 2);
        // A file that changed on the server doesn't continue what is held.
        let mut other = whole.clone();
        let at = from as usize + 3;
        other[at] ^= 0xFF;
        assert!(!reader.feed_range(from, other[from as usize..].to_vec()).unwrap());
        assert!(!reader.feed_range(whole.len() as u64 + 10, vec![1]).unwrap());
    }

    #[test]
    fn reads_a_cluster_cut_in_an_answer_again_with_its_rest() {
        let blocks = [
            Block { time: 1000, data: white_set(), duration: None },
            Block { time: 2000, data: empty_set(), duration: Some(500) },
            Block { time: 3000, data: white_set(), duration: None },
        ];
        let mut whole = header("S_DVBSUB", None);
        let head = whole.len();
        whole.extend(cluster(0, &blocks));
        let all = [(1000, None), (2000, Some(500)), (3000, None)];
        // Every cut gives the samples whole before it, and the rest gives them all.
        for cut in head + 1..whole.len() {
            let mut reader = MksReader::new();
            reader.feed_whole(whole[..cut].to_vec()).unwrap();
            let before = times(&reader.track().unwrap().unwrap());
            assert!(all.starts_with(&before), "cut at {cut}: {before:?}");
            match reader.resume_from() {
                Some(from) => assert!(reader.feed_range(from, whole[from as usize..].to_vec()).unwrap()),
                None => reader.feed_whole(whole.clone()).unwrap(),
            }
            assert_eq!(times(&reader.track().unwrap().unwrap()), all, "cut at {cut}");
        }
    }
}

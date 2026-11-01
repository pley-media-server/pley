// SPDX-License-Identifier: AGPL-3.0-only
//! Instant keyframe index for MP4, MOV and other ISO base media files, read
//! from the video track's sample tables (`stss` sync samples, `stts` decode
//! durations, `ctts` composition offsets, the `elst` edit list) and, in
//! fragmented files, from the sample flags of every movie fragment (`moof`,
//! `traf`, `tfhd`, `tfdt`, `trun`). Only box headers and these tables are
//! read, never the media data, so this takes milliseconds where a packet scan
//! reads the whole file.
//!
//! The times are the packet timestamps FFmpeg's demuxer reports, which is what
//! remuxing seeks with: the edit list is applied the way libavformat applies
//! it. Files this does not model exactly (several edits, sample groups that
//! add sync points, inconsistent tables) are refused, and the caller falls
//! back to scanning packets. Every size and count comes from an untrusted file
//! and is bounded.

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::prefetch::ContainerIndex;

/// Why a file's own index was not used.
pub type Result<T> = std::result::Result<T, &'static str>;

/// Largest header box read whole (`elst`, `trex`, …).
const MAX_SMALL_BOX: u64 = 1 << 20;
/// What the video track's sample tables (`stts`, `ctts`, `stss`, `stps`,
/// `stsc`) may take together: per-sample `stts` and `ctts` tables for
/// `MAX_SAMPLES` samples would be 64 MB, real ones are a few.
const TABLE_BUDGET: u64 = 48 << 20;
/// Largest `trun` read whole (a million samples with every field).
const MAX_TRUN: u64 = 16 << 20;
/// Most samples of the video track, movie box and fragments together (46
/// hours at 24 frames per second).
const MAX_SAMPLES: u64 = 4_000_000;
/// Most top-level boxes walked (a fragmented file has two per fragment).
const MAX_TOP_LEVEL_BOXES: u64 = 4_000_000;
/// Most children of one box.
const MAX_CHILDREN: usize = 100_000;
const MAX_EDITS: usize = 1_000;
/// FFmpeg drops a `ctts` table with offsets this large as invalid.
const MAX_CTS_OFFSET: i64 = 1 << 28;

/// Box types an ISO base media file can start with.
const FIRST_BOXES: [&[u8; 4]; 9] = [b"ftyp", b"moov", b"mdat", b"free", b"skip", b"wide", b"pnot", b"styp", b"uuid"];
/// Video sample formats FFmpeg decodes as still images (cover art, chapter
/// pictures): not the track that is played.
const IMAGE_FORMATS: [&[u8; 4]; 8] = [b"jpeg", b"mjpa", b"mjpb", b"AVDJ", b"dmb1", b"png ", b"MPNG", b"gif "];
/// `sample_is_non_sync_sample` and `sample_depends_on = 1` (FFmpeg treats
/// either as "not a keyframe").
const NON_SYNC: u32 = 0x0101_0000;

/// Whether `head`, the first bytes of a file, starts like an ISO base media file.
pub fn sniff(head: &[u8]) -> bool {
    head.get(4..8).is_some_and(|kind| FIRST_BOXES.iter().any(|b| b.as_slice() == kind))
}

/// Keyframe times (ms, sorted, at or after zero) of the first video track, as
/// FFmpeg's packet timestamps, or why the file's index can't give them. Gives
/// up once `deadline` has passed (walking many movie fragments can take a while
/// on slow storage).
pub fn keyframes(path: &Path, deadline: Option<Instant>) -> Result<Vec<i64>> {
    let file = File::open(path).map_err(|_| "cannot open the file")?;
    let len = file.metadata().map_err(|_| "cannot read the file's size")?.len();
    index(BufReader::with_capacity(16 * 1024, file), len, deadline)
}

fn index<R: Read + Seek>(inner: R, len: u64, deadline: Option<Instant>) -> Result<Vec<i64>> {
    let mut r = Reader { inner, pos: 0 };
    let mut top = TopLevel { pos: 0, len, walked: 0, deadline };
    let moov = loop {
        let h = top.next(&mut r)?.ok_or("no movie box (moov)")?;
        if top.walked == 1 && !FIRST_BOXES.contains(&&h.kind) {
            return Err("not an ISO base media file");
        }
        if h.kind == *b"moov" {
            break h;
        }
    };
    let movie = read_movie(&mut r, &moov)?;
    let track = movie.video.ok_or("no video track")?;
    let edit = edit_plan(&track.edits, movie.timescale, track.timescale)?;
    let tables = &track.tables;

    // Presentation times in media ticks.
    let mut pts: Vec<i64> = Vec::new();
    if movie.fragmented {
        let trex = movie.trex.iter().find(|t| t.track == track.id).copied().ok_or("fragment defaults missing")?;
        if edit.is_some() && tables.count > 0 {
            return Err("edit list on a fragmented file with samples in its movie box");
        }
        // Fragments are not edited: FFmpeg only offsets their timestamps.
        let time_offset = edit.map_or(0, |e| e.media_time - e.empty);
        let mut frags = Fragments {
            track: track.id,
            trex,
            time_offset,
            end: (tables.count == 0 && time_offset == 0).then_some(0),
            sidx: false,
            samples: tables.count,
            min_offset: 0,
        };
        let mut fragment_pts = Vec::new();
        while let Some(h) = top.next(&mut r)? {
            match &h.kind {
                b"moof" => frags.read_moof(&mut r, &h, &mut fragment_pts)?,
                b"sidx" => frags.sidx = true,
                _ => {}
            }
        }
        // FFmpeg reads every fragment before the first packet and delays
        // presentation by the most negative composition offset of the track
        // (decode times stay as they are).
        let shift = tables.dts_shift.max(-i64::from(frags.min_offset));
        if shift > 0 && tables.count > 0 {
            return Err("negative composition offsets in a fragmented file with samples in its movie box");
        }
        for s in Samples::new(tables).filter(|s| s.key) {
            pts.push(narrow(s.cts)?);
        }
        for t in fragment_pts {
            pts.push(t.checked_add(shift).ok_or("time out of range")?);
        }
    } else {
        match edit {
            None => {
                for s in Samples::new(tables).filter(|s| s.key) {
                    pts.push(narrow(s.cts)?);
                }
            }
            Some(edit) => pts = edited_keyframes(tables, &edit)?,
        }
    }

    let scale = i128::from(track.timescale);
    let mut times: Vec<i64> = pts
        .into_iter()
        .filter(|&t| t >= 0)
        .filter_map(|t| i64::try_from((i128::from(t) * 1000 + scale / 2) / scale).ok())
        .collect();
    times.sort_unstable();
    times.dedup();
    if times.is_empty() {
        return Err("no keyframes");
    }
    Ok(times)
}

/// Where a file's movie box is (its bytes) and where the video track's
/// keyframes are: `(time in ms, offset)`, sorted by time. For reading ahead of
/// a playback: the times are the samples' presentation times moved by the
/// usual edit list (a decoding delay hidden), not applied in full as FFmpeg
/// does (a few frames off at most for a usual file, close enough to read
/// around), the offsets from the movie box's sample tables (`stco`/`co64`
/// chunk offsets, `stsc` samples per chunk, `stsz`/`stz2` sample sizes; the
/// chunk's own offset when the sizes can't be read). A fragmented file lists
/// no samples there: no keyframes then.
pub fn chunk_index(path: &Path) -> Result<ContainerIndex> {
    let file = File::open(path).map_err(|_| "cannot open the file")?;
    let len = file.metadata().map_err(|_| "cannot read the file's size")?.len();
    let mut r = Reader { inner: BufReader::with_capacity(16 * 1024, file), pos: 0 };
    let mut top = TopLevel { pos: 0, len, walked: 0, deadline: Some(Instant::now() + Duration::from_secs(2)) };
    let (start, moov) = loop {
        let start = top.pos;
        let h = top.next(&mut r)?.ok_or("no movie box (moov)")?;
        if top.walked == 1 && !FIRST_BOXES.contains(&&h.kind) {
            return Err("not an ISO base media file");
        }
        if h.kind == *b"moov" {
            break (start, h);
        }
    };
    let movie = read_movie(&mut r, &moov)?;
    let mut points = Vec::new();
    if let Some(track) = movie.video.filter(|t| t.tables.count > 0) {
        let tables = &track.tables;
        let mut budget = TABLE_BUDGET;
        let offsets = chunk_offsets(&mut r, tables.chunk_table.ok_or("no chunk offsets")?, &mut budget)?;
        let sizes = match tables.size_table {
            Some(h) => sample_sizes(&mut r, h, &mut budget).unwrap_or_default(),
            None => SampleSizes::default(),
        };
        let scale = i128::from(track.timescale);
        // The usual edit list (a decoding delay hidden) moves every time alike.
        let shift = edit_plan(&track.edits, movie.timescale, track.timescale)
            .ok()
            .flatten()
            .map_or(0, |edit| edit.media_time - edit.empty);
        let mut samples = Samples::new(tables);
        let mut number = 0usize;
        let mut run = 0;
        'chunks: for (i, &offset) in offsets.iter().enumerate() {
            let chunk = i as u64 + 1;
            // The first run starts at chunk 1 whatever it says (as FFmpeg reads it).
            while tables.stsc.get(run + 1).is_some_and(|next| u64::from(next.0) <= chunk) {
                run += 1;
            }
            let per_chunk = tables.stsc.get(run).map_or(0, |r| r.1);
            let mut at = offset;
            for _ in 0..per_chunk {
                let Some(sample) = samples.next() else { break 'chunks };
                if sample.key {
                    let ticks = (sample.cts - shift).max(0);
                    let ms = i64::try_from((ticks * 1000 + scale / 2) / scale).unwrap_or(i64::MAX);
                    points.push((ms, at));
                }
                at = at.saturating_add(sizes.get(number));
                number += 1;
            }
        }
    }
    points.sort_unstable();
    points.dedup_by_key(|p| p.0);
    Ok((start..moov.end, points))
}

/// The chunk offsets of an `stco` or `co64` box.
fn chunk_offsets<R: Read + Seek>(r: &mut Reader<R>, h: BoxHead, budget: &mut u64) -> Result<Vec<u64>> {
    let data = table(r, &h, budget)?;
    let mut f = Fields::new(&data);
    f.skip(4)?;
    if h.kind == *b"co64" {
        let n = f.count(8)?;
        (0..n).map(|_| f.u64()).collect()
    } else {
        let n = f.count(4)?;
        (0..n).map(|_| f.u32().map(u64::from)).collect()
    }
}

/// Sample sizes (`stsz` or `stz2`); 0 for those not known.
#[derive(Default)]
struct SampleSizes {
    all: u32,
    each: Vec<u32>,
}

impl SampleSizes {
    fn get(&self, n: usize) -> u64 {
        u64::from(if self.all > 0 { self.all } else { self.each.get(n).copied().unwrap_or(0) })
    }
}

fn sample_sizes<R: Read + Seek>(r: &mut Reader<R>, h: BoxHead, budget: &mut u64) -> Result<SampleSizes> {
    let data = table(r, &h, budget)?;
    let mut f = Fields::new(&data);
    f.skip(4)?;
    if h.kind == *b"stsz" {
        let all = f.u32()?;
        if all > 0 {
            return Ok(SampleSizes { all, each: Vec::new() });
        }
        let n = f.count(4)?;
        return Ok(SampleSizes { all, each: (0..n).map(|_| f.u32()).collect::<Result<_>>()? });
    }
    // `stz2`: 4, 8 or 16 bits per sample.
    let bits = f.take::<4>()?[3];
    let n = f.u32()? as usize;
    // 4 bits each would hold 8 times their size in memory: as many as a
    // track may have, no more.
    if n as u64 > MAX_SAMPLES {
        return Err("too many samples");
    }
    let bytes = match bits {
        4 => n.div_ceil(2),
        8 => n,
        16 => n.checked_mul(2).ok_or("too many samples")?,
        _ => return Err("unknown sample size field"),
    };
    let raw = data.get(12..12 + bytes).ok_or("table lists more entries than it holds")?;
    let each = (0..n)
        .map(|i| match bits {
            4 => u32::from(if i % 2 == 0 { raw[i / 2] >> 4 } else { raw[i / 2] & 0x0F }),
            8 => u32::from(raw[i]),
            _ => u32::from(u16::from_be_bytes([raw[2 * i], raw[2 * i + 1]])),
        })
        .collect();
    Ok(SampleSizes { all: 0, each })
}

/// A time in media ticks, kept in 64 bits.
fn narrow(ticks: i128) -> Result<i64> {
    i64::try_from(ticks).map_err(|_| "time out of range")
}

// ---------------------------------------------------------------- reading

struct Reader<R> {
    inner: R,
    /// Where `inner` is (tracked here so nearby reads stay in the buffer).
    pos: u64,
}

#[derive(Debug, Clone, Copy)]
struct BoxHead {
    kind: [u8; 4],
    /// Offset of the payload.
    body: u64,
    /// Offset just past the box.
    end: u64,
}

impl BoxHead {
    fn body_len(&self) -> u64 {
        self.end - self.body
    }
}

impl<R: Read + Seek> Reader<R> {
    fn read_at(&mut self, pos: u64, buf: &mut [u8]) -> Result<()> {
        if pos != self.pos {
            let delta = i64::try_from(i128::from(pos) - i128::from(self.pos)).map_err(|_| "seek out of range")?;
            self.inner.seek_relative(delta).map_err(|_| "seek failed")?;
            self.pos = pos;
        }
        self.inner.read_exact(buf).map_err(|_| "unexpected end of file")?;
        self.pos += buf.len() as u64;
        Ok(())
    }

    /// The box at `pos` in a parent ending at `end`, or `None` when no box
    /// fits there. `clamp` cuts a box running past `end` (the last top-level
    /// box of a truncated file) instead of refusing it.
    fn head(&mut self, pos: u64, end: u64, clamp: bool) -> Result<Option<BoxHead>> {
        if end < pos.saturating_add(8) {
            return Ok(None);
        }
        let mut h = [0u8; 8];
        self.read_at(pos, &mut h)?;
        let kind = [h[4], h[5], h[6], h[7]];
        let mut header = 8u64;
        let size = match u32::from_be_bytes([h[0], h[1], h[2], h[3]]) {
            0 => end - pos,
            1 => {
                let mut large = [0u8; 8];
                if end < pos + 16 {
                    return Err("truncated box header");
                }
                self.read_at(pos + 8, &mut large)?;
                header = 16;
                u64::from_be_bytes(large)
            }
            n => u64::from(n),
        };
        if kind == *b"uuid" {
            header += 16;
        }
        if size < header {
            return Err("box smaller than its header");
        }
        let mut box_end = pos.checked_add(size).ok_or("box size out of range")?;
        if box_end > end {
            if !clamp {
                return Err("box extends past its parent");
            }
            box_end = end;
        }
        if box_end < pos + header {
            return Err("truncated box");
        }
        Ok(Some(BoxHead { kind, body: pos + header, end: box_end }))
    }

    fn children(&mut self, parent: &BoxHead) -> Result<Vec<BoxHead>> {
        let mut out = Vec::new();
        let mut pos = parent.body;
        while let Some(h) = self.head(pos, parent.end, false)? {
            if out.len() == MAX_CHILDREN {
                return Err("too many boxes");
            }
            pos = h.end;
            out.push(h);
        }
        Ok(out)
    }

    /// The whole payload of a box no larger than `max`.
    fn body(&mut self, h: &BoxHead, max: u64) -> Result<Vec<u8>> {
        if h.body_len() > max {
            return Err("box too large");
        }
        let mut buf = vec![0u8; h.body_len() as usize];
        self.read_at(h.body, &mut buf)?;
        Ok(buf)
    }

    /// The first `n` bytes of a box's payload.
    fn prefix(&mut self, h: &BoxHead, n: usize) -> Result<Vec<u8>> {
        if h.body_len() < n as u64 {
            return Err("box too short");
        }
        let mut buf = vec![0u8; n];
        self.read_at(h.body, &mut buf)?;
        Ok(buf)
    }

    /// Up to `n` bytes of a box's payload.
    fn head_bytes(&mut self, h: &BoxHead, n: u64) -> Result<Vec<u8>> {
        self.prefix(h, h.body_len().min(n) as usize)
    }
}

/// Walks the top-level boxes, within limits.
struct TopLevel {
    pos: u64,
    len: u64,
    walked: u64,
    deadline: Option<Instant>,
}

impl TopLevel {
    fn next<R: Read + Seek>(&mut self, r: &mut Reader<R>) -> Result<Option<BoxHead>> {
        let Some(h) = r.head(self.pos, self.len, true)? else { return Ok(None) };
        self.walked += 1;
        if self.walked > MAX_TOP_LEVEL_BOXES {
            return Err("too many boxes");
        }
        if self.walked.is_multiple_of(64) && self.deadline.is_some_and(|d| Instant::now() >= d) {
            return Err("too many fragments to read in time");
        }
        self.pos = h.end;
        Ok(Some(h))
    }
}

/// Big-endian fields of a payload.
struct Fields<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Fields<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        let bytes = self.data.get(self.pos..self.pos + N).ok_or("box too short")?;
        self.pos += N;
        Ok(bytes.try_into().unwrap_or([0; N]))
    }

    fn u32(&mut self) -> Result<u32> {
        self.take::<4>().map(u32::from_be_bytes)
    }

    fn i32(&mut self) -> Result<i32> {
        self.take::<4>().map(i32::from_be_bytes)
    }

    fn u64(&mut self) -> Result<u64> {
        self.take::<8>().map(u64::from_be_bytes)
    }

    fn i64(&mut self) -> Result<i64> {
        self.take::<8>().map(i64::from_be_bytes)
    }

    fn skip(&mut self, n: usize) -> Result<()> {
        if self.remaining() < n {
            return Err("box too short");
        }
        self.pos += n;
        Ok(())
    }

    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    /// The entry count of a table whose entries are `entry_size` bytes,
    /// checked against what the payload holds.
    fn count(&mut self, entry_size: usize) -> Result<usize> {
        let n = self.u32()? as usize;
        if n.checked_mul(entry_size).is_none_or(|bytes| bytes > self.remaining()) {
            return Err("table lists more entries than it holds");
        }
        Ok(n)
    }
}

// ---------------------------------------------------------------- movie box

#[derive(Default)]
struct Movie {
    timescale: u32,
    video: Option<Track>,
    trex: Vec<Trex>,
    fragmented: bool,
}

struct Track {
    id: u32,
    timescale: u32,
    edits: Vec<Edit>,
    tables: Tables,
}

#[derive(Debug, Clone, Copy)]
struct Edit {
    /// In the movie timescale.
    duration: u64,
    /// In the media timescale; -1 for an empty edit.
    media_time: i64,
    /// 16.16 fixed point.
    rate: i32,
}

#[derive(Debug, Clone, Copy)]
struct Trex {
    track: u32,
    description: u32,
    duration: u32,
    flags: u32,
}

fn read_movie<R: Read + Seek>(r: &mut Reader<R>, moov: &BoxHead) -> Result<Movie> {
    let mut movie = Movie::default();
    for h in r.children(moov)? {
        match &h.kind {
            b"mvhd" => movie.timescale = timescale(&r.head_bytes(&h, 24)?)?,
            b"trak" if movie.video.is_none() => movie.video = read_track(r, &h)?,
            b"mvex" => {
                movie.fragmented = true;
                for t in r.children(&h)?.iter().filter(|t| t.kind == *b"trex") {
                    let data = r.prefix(t, 24)?;
                    let mut f = Fields::new(&data);
                    f.skip(4)?;
                    let (track, description, duration) = (f.u32()?, f.u32()?, f.u32()?);
                    f.skip(4)?; // default sample size
                    movie.trex.push(Trex { track, description, duration, flags: f.u32()? });
                }
            }
            b"cmov" => return Err("compressed movie box"),
            _ => {}
        }
    }
    Ok(movie)
}

/// The timescale of an `mvhd` or `mdhd` payload.
fn timescale(payload: &[u8]) -> Result<u32> {
    let mut f = Fields::new(payload);
    let version = f.take::<4>()?[0];
    f.skip(if version == 1 { 16 } else { 8 })?;
    f.u32()
}

fn find(boxes: &[BoxHead], kind: &[u8; 4]) -> Option<BoxHead> {
    boxes.iter().find(|h| h.kind == *kind).copied()
}

/// The track, when it is video FFmpeg would play (not a still picture).
fn read_track<R: Read + Seek>(r: &mut Reader<R>, trak: &BoxHead) -> Result<Option<Track>> {
    let kids = r.children(trak)?;
    let Some(mdia) = find(&kids, b"mdia") else { return Ok(None) };
    let media = r.children(&mdia)?;
    let Some(hdlr) = find(&media, b"hdlr") else { return Ok(None) };
    if r.prefix(&hdlr, 12)?[8..12] != *b"vide" {
        return Ok(None);
    }
    let minf = find(&media, b"minf").ok_or("video track without media information")?;
    let stbl = find(&r.children(&minf)?, b"stbl").ok_or("video track without sample tables")?;
    let tables = r.children(&stbl)?;
    let stsd = find(&tables, b"stsd").ok_or("video track without sample descriptions")?;
    let format = r.prefix(&stsd, 16)?;
    if IMAGE_FORMATS.iter().any(|f| f.as_slice() == &format[12..16]) {
        return Ok(None);
    }

    let tkhd = find(&kids, b"tkhd").ok_or("track without a header")?;
    let head = r.head_bytes(&tkhd, 24)?;
    let mut f = Fields::new(&head);
    let version = f.take::<4>()?[0];
    f.skip(if version == 1 { 16 } else { 8 })?;
    let id = f.u32()?;

    let mdhd = find(&media, b"mdhd").ok_or("track without a media header")?;
    let scale = timescale(&r.head_bytes(&mdhd, 24)?)?;
    if scale == 0 {
        return Err("media timescale of zero");
    }

    let mut edits = Vec::new();
    if let Some(edts) = find(&kids, b"edts")
        && let Some(elst) = find(&r.children(&edts)?, b"elst")
    {
        edits = parse_elst(&r.body(&elst, MAX_SMALL_BOX)?)?;
    }
    Ok(Some(Track { id, timescale: scale, edits, tables: read_tables(r, &tables)? }))
}

fn parse_elst(data: &[u8]) -> Result<Vec<Edit>> {
    let mut f = Fields::new(data);
    let version = f.take::<4>()?[0];
    let n = f.count(if version == 1 { 20 } else { 12 })?;
    if n > MAX_EDITS {
        return Err("too many edits");
    }
    (0..n)
        .map(|_| {
            let (duration, media_time) =
                if version == 1 { (f.u64()?, f.i64()?) } else { (u64::from(f.u32()?), i64::from(f.i32()?)) };
            Ok(Edit { duration, media_time, rate: f.i32()? })
        })
        .collect()
}

/// The video track's sample tables, as FFmpeg's demuxer indexes them.
#[derive(Debug, Default)]
struct Tables {
    /// Samples in the movie box.
    count: u64,
    /// (sample count, decode duration).
    stts: Vec<(u32, u32)>,
    /// (sample count, composition offset).
    ctts: Vec<(u32, i32)>,
    /// Sync sample numbers (from 1); `None`: every sample is one.
    sync: Option<Vec<u32>>,
    /// Partial sync samples (QuickTime `stps`), also keyframes to FFmpeg.
    partial_sync: Vec<u32>,
    /// How much FFmpeg shifts decode times for negative composition offsets.
    dts_shift: i64,
    /// (first chunk, samples per chunk, description) runs (`stsc`).
    stsc: Vec<(u32, u32, u32)>,
    /// The chunk offsets (`stco` or `co64`) and sample sizes (`stsz` or
    /// `stz2`), not read here.
    chunk_table: Option<BoxHead>,
    size_table: Option<BoxHead>,
}

/// A sample table's payload, charged to what the track's tables may take.
fn table<R: Read + Seek>(r: &mut Reader<R>, h: &BoxHead, budget: &mut u64) -> Result<Vec<u8>> {
    if h.body_len() > *budget {
        return Err("sample tables too large");
    }
    *budget -= h.body_len();
    r.body(h, u64::MAX)
}

fn read_tables<R: Read + Seek>(r: &mut Reader<R>, boxes: &[BoxHead]) -> Result<Tables> {
    let mut t = Tables::default();
    let mut budget = TABLE_BUDGET;
    let mut sizes = None;
    let mut chunks = None;
    for h in boxes {
        match &h.kind {
            b"stsz" | b"stz2" => {
                sizes = Some(Fields::new(&r.prefix(h, 12)?[8..]).u32()?);
                t.size_table = Some(*h);
            }
            b"stco" | b"co64" => {
                chunks = Some(Fields::new(&r.prefix(h, 8)?[4..]).u32()?);
                t.chunk_table = Some(*h);
            }
            b"stsc" => {
                let data = table(r, h, &mut budget)?;
                let mut f = Fields::new(&data);
                f.skip(4)?;
                let n = f.count(12)?;
                let entries = (0..n)
                    .map(|_| {
                        let (first, per_chunk, description) = (f.u32()?, f.u32()?, f.u32()?);
                        Ok((first, per_chunk, description))
                    })
                    .collect::<Result<Vec<_>>>()?;
                t.stsc = entries;
            }
            b"stts" => {
                let data = table(r, h, &mut budget)?;
                let mut f = Fields::new(&data);
                f.skip(4)?;
                let n = f.count(8)?;
                t.stts = (0..n).map(|_| Ok((f.u32()?, f.u32()?))).collect::<Result<_>>()?;
            }
            b"ctts" => {
                let data = table(r, h, &mut budget)?;
                let mut f = Fields::new(&data);
                f.skip(4)?;
                let n = f.count(8)?;
                for i in 0..n {
                    let (count, offset) = (f.u32()?, f.i32()?);
                    // FFmpeg reads counts as signed and skips the ones that
                    // aren't positive; it ignores the last two entries when
                    // judging the offsets.
                    if count == 0 || count > i32::MAX as u32 {
                        continue;
                    }
                    if i + 2 < n {
                        if i64::from(offset).abs() > MAX_CTS_OFFSET {
                            return Err("composition offsets out of range");
                        }
                        t.dts_shift = t.dts_shift.max(-i64::from(offset));
                    }
                    t.ctts.push((count, offset));
                }
            }
            b"stss" => {
                let sync = sample_numbers(&table(r, h, &mut budget)?)?;
                if sync.is_empty() {
                    return Err("empty sync sample table");
                }
                t.sync = Some(sync);
            }
            b"stps" => t.partial_sync = sample_numbers(&table(r, h, &mut budget)?)?,
            b"sbgp" => {
                let head = r.prefix(h, 8)?;
                if matches!(&head[4..8], b"rap " | b"sync") {
                    return Err("sync points in sample groups");
                }
            }
            _ => {}
        }
    }

    // FFmpeg walks the chunks (`stsc` runs over `stco` entries) and stops at
    // the sample count of `stsz`.
    let covered = match chunks {
        Some(chunks) if !t.stsc.is_empty() => samples_in_chunks(&t.stsc, u64::from(chunks))?,
        _ => 0,
    };
    t.count = covered.min(u64::from(sizes.unwrap_or(0)));
    if t.count > MAX_SAMPLES {
        return Err("too many samples");
    }
    if t.count > 0 {
        let timed: u64 = t.stts.iter().map(|&(n, _)| u64::from(n)).sum();
        if timed < t.count {
            return Err("decode times missing for some samples");
        }
        if t.stts.iter().any(|&(n, d)| n > 0 && d > i32::MAX as u32) {
            return Err("decode duration out of range");
        }
        let offsets: u64 = t.ctts.iter().map(|&(n, _)| u64::from(n)).sum();
        if !t.ctts.is_empty() && offsets < t.count {
            return Err("composition offsets missing for some samples");
        }
    }
    Ok(t)
}

/// A strictly increasing list of sample numbers (`stss`, `stps`).
fn sample_numbers(data: &[u8]) -> Result<Vec<u32>> {
    let mut f = Fields::new(data);
    f.skip(4)?;
    let n = f.count(4)?;
    let numbers = (0..n).map(|_| f.u32()).collect::<Result<Vec<_>>>()?;
    if numbers.first() == Some(&0) || numbers.windows(2).any(|w| w[0] >= w[1]) {
        return Err("unordered sample numbers");
    }
    Ok(numbers)
}

/// Samples in `chunks` chunks laid out by `stsc` runs, the way FFmpeg counts
/// them: the first run starts at chunk 1 whatever it says.
fn samples_in_chunks(stsc: &[(u32, u32, u32)], chunks: u64) -> Result<u64> {
    if stsc.iter().any(|&(first, per_chunk, description)| first == 0 || per_chunk == 0 || description == 0)
        || stsc.windows(2).any(|w| w[0].0 >= w[1].0)
    {
        return Err("invalid sample-to-chunk table");
    }
    let mut total = 0u64;
    for (i, &(first, per_chunk, _)) in stsc.iter().enumerate() {
        let start = if i == 0 { 1 } else { u64::from(first) }.min(chunks + 1);
        let end = stsc.get(i + 1).map_or(chunks + 1, |next| u64::from(next.0)).min(chunks + 1);
        total = total.saturating_add(end.saturating_sub(start).saturating_mul(u64::from(per_chunk)));
    }
    Ok(total)
}

// ---------------------------------------------------------------- samples

#[derive(Debug, Clone, Copy)]
struct Sample {
    dts: i128,
    cts: i128,
    key: bool,
}

/// Run-length table cursor.
struct Runs<'a, T> {
    runs: &'a [(u32, T)],
    index: usize,
    used: u32,
}

impl<'a, T: Copy> Runs<'a, T> {
    fn new(runs: &'a [(u32, T)]) -> Self {
        Self { runs, index: 0, used: 0 }
    }

    fn next(&mut self) -> Option<T> {
        while let Some(&(count, value)) = self.runs.get(self.index) {
            if self.used < count {
                self.used += 1;
                return Some(value);
            }
            self.index += 1;
            self.used = 0;
        }
        None
    }
}

/// The movie box's samples in decode order (tables already checked to cover
/// every sample).
struct Samples<'a> {
    t: &'a Tables,
    /// Next sample number, from 1.
    number: u64,
    dts: i128,
    stts: Runs<'a, u32>,
    ctts: Runs<'a, i32>,
    sync: usize,
    partial: usize,
}

impl<'a> Samples<'a> {
    fn new(t: &'a Tables) -> Self {
        Self { t, number: 1, dts: 0, stts: Runs::new(&t.stts), ctts: Runs::new(&t.ctts), sync: 0, partial: 0 }
    }
}

impl Iterator for Samples<'_> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if self.number > self.t.count {
            return None;
        }
        let number = self.number as u32;
        self.number += 1;
        // Like FFmpeg: a sync sample, else a partial sync sample.
        let key = match &self.t.sync {
            None => true,
            Some(sync) if sync.get(self.sync) == Some(&number) => {
                self.sync += 1;
                true
            }
            Some(_) if self.t.partial_sync.get(self.partial) == Some(&number) => {
                self.partial += 1;
                true
            }
            Some(_) => false,
        };
        let sample = Sample { dts: self.dts, cts: self.dts + i128::from(self.ctts.next().unwrap_or(0)), key };
        self.dts += i128::from(self.stts.next().unwrap_or(0));
        Some(sample)
    }
}

// ---------------------------------------------------------------- edit list

/// The edit list FFmpeg applies exactly: an optional empty edit (a delay),
/// then one edit playing the media at normal speed from `media_time`. All in
/// media ticks.
#[derive(Debug, Clone, Copy)]
struct EditPlan {
    empty: i128,
    media_time: i128,
    /// `None`: until the end (a zero duration, used by fragmented files).
    duration: Option<i128>,
}

fn edit_plan(edits: &[Edit], movie_scale: u32, media_scale: u32) -> Result<Option<EditPlan>> {
    let (empty, rest) = match edits {
        [] => return Ok(None),
        [first, rest @ ..] if first.media_time == -1 => (first.duration, rest),
        _ => (0, edits),
    };
    let edit = match rest {
        [edit] => edit,
        [] => return Err("edit list without media"),
        _ => return Err("edit list with several edits"),
    };
    if edit.media_time < 0 {
        return Err("edit list without media");
    }
    if edit.rate != 0x1_0000 {
        return Err("edit list changing the speed");
    }
    if movie_scale == 0 {
        return Err("edit list without a movie timescale");
    }
    // FFmpeg converts movie ticks with av_rescale (rounding to nearest).
    let to_media = |movie_ticks: u64| {
        (i128::from(movie_ticks) * i128::from(media_scale) + i128::from(movie_scale) / 2) / i128::from(movie_scale)
    };
    Ok(Some(EditPlan {
        empty: to_media(empty),
        media_time: i128::from(edit.media_time),
        duration: (edit.duration > 0).then(|| to_media(edit.duration)),
    }))
}

/// Keyframe presentation times of a movie box edited by `edit`, mirroring
/// libavformat (`mov_fix_index`): decoding starts at the last keyframe at or
/// before the edit; the first sample presented in the edit, or the earliest
/// one if that comes later, starts at the end of the empty edit.
fn edited_keyframes(t: &Tables, edit: &EditPlan) -> Result<Vec<i64>> {
    let m = edit.media_time;
    let duration = edit.duration.ok_or("edit list without a duration")?;
    let in_edit = |s: &Sample| s.cts >= m && s.cts < m + duration;
    let start = Samples::new(t)
        .take_while(|s| s.dts <= m)
        .enumerate()
        .filter(|(_, s)| s.key && s.cts <= m)
        .last()
        .map_or(0, |(i, _)| i);
    let mut first_dts = None;
    let mut min_cts: Option<i128> = None;
    let mut keys = Vec::new();
    for s in Samples::new(t).skip(start).filter(in_edit) {
        first_dts.get_or_insert(s.dts);
        min_cts = Some(min_cts.map_or(s.cts, |c| c.min(s.cts)));
        if s.key {
            keys.push(narrow(s.cts)?);
        }
    }
    let (Some(first_dts), Some(min_cts)) = (first_dts, min_cts) else { return Err("no samples in the edit") };
    let shift = first_dts.max(min_cts);
    keys.into_iter().map(|cts| narrow(edit.empty + i128::from(cts) - shift)).collect()
}

// ---------------------------------------------------------------- fragments

struct Fragments {
    track: u32,
    trex: Trex,
    /// Subtracted from fragment decode times (FFmpeg's edit list handling).
    time_offset: i128,
    /// Where the previous run of samples ended (media ticks), for fragments
    /// without a decode time.
    end: Option<i128>,
    /// A segment index exists (FFmpeg would take times from it).
    sidx: bool,
    samples: u64,
    /// Most negative composition offset of any sample.
    min_offset: i32,
}

impl Fragments {
    fn read_moof<R: Read + Seek>(&mut self, r: &mut Reader<R>, moof: &BoxHead, pts: &mut Vec<i64>) -> Result<()> {
        for traf in r.children(moof)?.iter().filter(|h| h.kind == *b"traf") {
            self.read_traf(r, traf, pts)?;
        }
        Ok(())
    }

    fn read_traf<R: Read + Seek>(&mut self, r: &mut Reader<R>, traf: &BoxHead, pts: &mut Vec<i64>) -> Result<()> {
        let kids = r.children(traf)?;
        let Some(tfhd) = find(&kids, b"tfhd") else { return Ok(()) };
        let head = r.head_bytes(&tfhd, 40)?;
        let mut f = Fields::new(&head);
        let flags = f.u32()? & 0x00FF_FFFF;
        if f.u32()? != self.track {
            return Ok(());
        }
        if flags & 0x01 != 0 {
            f.skip(8)?; // base data offset
        }
        let description = if flags & 0x02 != 0 { f.u32()? } else { self.trex.description };
        let duration = if flags & 0x08 != 0 { f.u32()? } else { self.trex.duration };
        if flags & 0x10 != 0 {
            f.skip(4)?; // default sample size
        }
        let sample_flags = if flags & 0x20 != 0 { f.u32()? } else { self.trex.flags };
        if description != 1 {
            return Err("fragment with another sample description");
        }

        let mut base = None;
        let mut next: Option<i128> = None;
        for h in &kids {
            match &h.kind {
                b"tfdt" => {
                    let data = r.head_bytes(h, 12)?;
                    let mut f = Fields::new(&data);
                    let version = f.take::<4>()?[0];
                    base = Some(if version == 1 { i128::from(f.u64()?) } else { i128::from(f.u32()?) });
                }
                b"sbgp" => {
                    if matches!(&r.prefix(h, 8)?[4..8], b"rap " | b"sync") {
                        return Err("sync points in sample groups");
                    }
                }
                b"trun" => {
                    let start = match (next, base) {
                        (Some(dts), _) => dts,
                        (None, Some(base)) => base - self.time_offset,
                        (None, None) if self.sidx => return Err("fragment timed by a segment index"),
                        (None, None) => self.end.ok_or("fragment without a decode time")? - self.time_offset,
                    };
                    let data = r.body(h, MAX_TRUN)?;
                    next = Some(self.read_trun(&data, start, duration, sample_flags, pts)?);
                }
                _ => {}
            }
        }
        if let Some(dts) = next {
            self.end = Some(dts + self.time_offset);
        }
        Ok(())
    }

    /// Keyframes of a track run starting at decode time `dts` (presentation
    /// times before the track's final shift); returns where it ends.
    fn read_trun(
        &mut self,
        data: &[u8],
        mut dts: i128,
        default_duration: u32,
        default_flags: u32,
        pts: &mut Vec<i64>,
    ) -> Result<i128> {
        let mut f = Fields::new(data);
        let flags = f.u32()? & 0x00FF_FFFF;
        let count = f.u32()?;
        if flags & 0x01 != 0 {
            f.skip(4)?; // data offset
        }
        let first_flags = if flags & 0x04 != 0 { f.u32()? } else { default_flags };
        let per_sample = 4 * (flags & 0xF00).count_ones() as usize;
        if per_sample > 0 && (count as usize).checked_mul(per_sample).is_none_or(|n| n > f.remaining()) {
            return Err("track run lists more samples than it holds");
        }
        self.samples += u64::from(count);
        if self.samples > MAX_SAMPLES {
            return Err("too many samples");
        }
        for i in 0..count {
            let duration = if flags & 0x100 != 0 { f.u32()? } else { default_duration };
            if flags & 0x200 != 0 {
                f.skip(4)?; // size
            }
            let mut sample_flags = if i == 0 { first_flags } else { default_flags };
            if flags & 0x400 != 0 {
                sample_flags = f.u32()?;
            }
            let offset = if flags & 0x800 != 0 { f.i32()? } else { 0 };
            self.min_offset = self.min_offset.min(offset);
            if sample_flags & NON_SYNC == 0 {
                pts.push(narrow(dts + i128::from(offset))?);
            }
            dts += i128::from(duration);
        }
        Ok(dts)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    // ---- building files in memory

    fn bx(kind: &[u8; 4], parts: &[&[u8]]) -> Vec<u8> {
        let payload = parts.concat();
        let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(&payload);
        out
    }

    /// A full box: version, flags, payload.
    fn fbx(kind: &[u8; 4], version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
        let mut head = flags.to_be_bytes();
        head[0] = version;
        bx(kind, &[&head, payload])
    }

    fn u32s(values: &[u32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_be_bytes()).collect()
    }

    fn table(kind: &[u8; 4], entries: &[u32]) -> Vec<u8> {
        fbx(kind, 0, 0, &u32s(entries))
    }

    /// `[entry count, entries…]` of (count, value) pairs.
    fn pairs(kind: &[u8; 4], entries: &[(u32, i64)]) -> Vec<u8> {
        let mut v = vec![entries.len() as u32];
        for &(n, x) in entries {
            v.extend([n, x as i32 as u32]);
        }
        table(kind, &v)
    }

    fn header(kind: &[u8; 4], timescale: u32) -> Vec<u8> {
        // creation, modification, timescale, duration (+ language for mdhd)
        fbx(kind, 0, 0, &u32s(&[0, 0, timescale, 0, 0]))
    }

    struct Video {
        timescale: u32,
        /// (count, duration)
        stts: Vec<(u32, i64)>,
        ctts: Vec<(u32, i64)>,
        stss: Option<Vec<u32>>,
        samples: u32,
        /// (duration in movie ticks, media time)
        edits: Vec<(u32, i32)>,
        format: [u8; 4],
        extra: Vec<Vec<u8>>,
    }

    impl Video {
        fn new(timescale: u32, samples: u32, duration: u32) -> Self {
            Self {
                timescale,
                stts: vec![(samples, duration as i64)],
                ctts: vec![],
                stss: None,
                samples,
                edits: vec![],
                format: *b"avc1",
                extra: vec![],
            }
        }

        fn trak(&self, id: u32) -> Vec<u8> {
            let mut stbl = vec![
                fbx(b"stsd", 0, 0, &[u32s(&[1, 16]), self.format.to_vec(), vec![0; 4]].concat()),
                pairs(b"stts", &self.stts),
            ];
            if !self.ctts.is_empty() {
                stbl.push(pairs(b"ctts", &self.ctts));
            }
            if let Some(stss) = &self.stss {
                stbl.push(table(b"stss", &[&[stss.len() as u32][..], stss].concat()));
            }
            // One sample per chunk.
            stbl.push(table(b"stsc", &[1, 1, 1, 1]));
            stbl.push(table(b"stsz", &[0, self.samples]));
            stbl.push(table(b"stco", &[self.samples]));
            stbl.extend(self.extra.iter().cloned());
            let stbl = bx(b"stbl", &stbl.iter().map(Vec::as_slice).collect::<Vec<_>>());
            let mdia = bx(
                b"mdia",
                &[
                    &header(b"mdhd", self.timescale),
                    &fbx(b"hdlr", 0, 0, &[&[0u8; 4][..], b"vide", &[0; 13]].concat()),
                    &bx(b"minf", &[&stbl]),
                ],
            );
            let mut parts = vec![fbx(b"tkhd", 0, 3, &u32s(&[0, 0, id, 0, 0]))];
            if !self.edits.is_empty() {
                let mut elst = vec![self.edits.len() as u32];
                for &(duration, media_time) in &self.edits {
                    elst.extend([duration, media_time as u32, 0x1_0000]);
                }
                parts.push(bx(b"edts", &[&table(b"elst", &elst)]));
            }
            parts.push(mdia);
            bx(b"trak", &parts.iter().map(Vec::as_slice).collect::<Vec<_>>())
        }
    }

    fn audio_trak() -> Vec<u8> {
        let mdia =
            bx(b"mdia", &[&header(b"mdhd", 48_000), &fbx(b"hdlr", 0, 0, &[&[0u8; 4][..], b"soun", &[0; 13]].concat())]);
        bx(b"trak", &[&fbx(b"tkhd", 0, 3, &u32s(&[0, 0, 2, 0, 0])), &mdia])
    }

    fn moov(movie_scale: u32, traks: &[Vec<u8>], mvex: Option<Vec<u8>>) -> Vec<u8> {
        let mut parts = vec![header(b"mvhd", movie_scale)];
        parts.extend(traks.iter().cloned());
        parts.extend(mvex);
        bx(b"moov", &parts.iter().map(Vec::as_slice).collect::<Vec<_>>())
    }

    fn ftyp() -> Vec<u8> {
        bx(b"ftyp", &[b"isom", &[0, 0, 2, 0], b"isomavc1"])
    }

    fn parse(file: &[u8]) -> Result<Vec<i64>> {
        index(Cursor::new(file), file.len() as u64, None)
    }

    /// Ten half-second samples, keyframes at 0, 2 and 4 s, in four chunks of
    /// 2, 2, 3 and 3 samples, the movie box after the media; with an edit
    /// list starting the media at `edit` ms.
    fn chunked(co64: bool, edit: Option<u32>) -> Vec<u8> {
        let offsets = [1_000u64, 2_000, 3_000, 4_000];
        let chunks = if co64 {
            let mut entries = vec![offsets.len() as u32];
            entries.extend(offsets.iter().flat_map(|o| [(o >> 32) as u32, *o as u32]));
            table(b"co64", &entries)
        } else {
            table(b"stco", &[&[offsets.len() as u32][..], &offsets.map(|o| o as u32)].concat())
        };
        let stbl = bx(
            b"stbl",
            &[
                &fbx(b"stsd", 0, 0, &[u32s(&[1, 16]), b"avc1".to_vec(), vec![0; 4]].concat()),
                &pairs(b"stts", &[(10, 500)]),
                &table(b"stss", &[3, 1, 5, 9]),
                &table(b"stsc", &[2, 1, 2, 1, 3, 3, 1]),
                &table(b"stsz", &[0, 10, 100, 101, 102, 103, 104, 105, 106, 107, 108, 109]),
                &chunks,
            ],
        );
        let mdia = bx(
            b"mdia",
            &[
                &header(b"mdhd", 1000),
                &fbx(b"hdlr", 0, 0, &[&[0u8; 4][..], b"vide", &[0; 13]].concat()),
                &bx(b"minf", &[&stbl]),
            ],
        );
        let edts = edit.map(|at| bx(b"edts", &[&table(b"elst", &[1, 5_000 - at, at, 0x1_0000])])).unwrap_or_default();
        let trak = bx(b"trak", &[&fbx(b"tkhd", 0, 3, &u32s(&[0, 0, 1, 0, 0])), &edts, &mdia]);
        [ftyp(), bx(b"mdat", &[&[0u8; 5000]]), moov(1000, &[audio_trak(), trak], None)].concat()
    }

    #[test]
    fn chunk_index_finds_the_chunks_of_keyframes() {
        let dir = tempfile::tempdir().unwrap();
        for co64 in [false, true] {
            let file = chunked(co64, None);
            let path = dir.path().join("movie.mp4");
            std::fs::write(&path, &file).unwrap();
            let moov = (ftyp().len() + 5008) as u64;
            let (range, points) = chunk_index(&path).unwrap();
            assert_eq!(range, moov..file.len() as u64, "co64: {co64}");
            // Keyframes start chunks 1 and 3, and come second in chunk 4.
            assert_eq!(points, vec![(0, 1_000), (2_000, 3_000), (4_000, 4_107)], "co64: {co64}");
        }
        // An edit list hiding a decoding delay moves the times as FFmpeg does.
        let path = dir.path().join("delayed.mp4");
        std::fs::write(&path, chunked(false, Some(250))).unwrap();
        let (_, points) = chunk_index(&path).unwrap();
        assert_eq!(points, vec![(0, 1_000), (1_750, 3_000), (3_750, 4_107)]);
        // Not an MP4 file.
        let other = dir.path().join("other.mp4");
        std::fs::write(&other, b"not a movie at all").unwrap();
        assert!(chunk_index(&other).is_err());
    }

    /// 25 fps at 12800 ticks per second with two B-frames, in closed groups of
    /// 49 frames (a keyframe every 1.96 s), and an edit list hiding the
    /// two-frame decoding delay: what FFmpeg writes for x264.
    fn bframes(frames: u32) -> Video {
        let mut v = Video::new(12_800, frames, 512);
        // Decode order I0 P3 B1 B2 P6 B4 B5 …: composition offset =
        // (display - decode + 2 frames) × 512.
        v.ctts = (0..frames)
            .map(|d| match d % 49 {
                0 => (1, 1024),
                g if (g - 1) % 3 == 0 => (1, 2048),
                _ => (1, 512),
            })
            .collect();
        v.stss = Some((0..frames.div_ceil(49)).map(|k| k * 49 + 1).collect());
        v.edits = vec![(frames * 512, 1024)];
        v
    }

    #[test]
    fn sniffs_iso_base_media() {
        assert!(sniff(&ftyp()));
        assert!(sniff(&bx(b"moov", &[])));
        assert!(sniff(b"\0\0\0\x08wide\0\0\0\0mdat"));
        assert!(!sniff(&[0x1A, 0x45, 0xDF, 0xA3, 0, 0, 0, 0]));
        assert!(!sniff(b"RIFF\0\0\0\0AVI "));
        assert!(!sniff(b"\0\0"));
    }

    #[test]
    fn reads_an_edited_movie_with_b_frames() {
        let file = [ftyp(), moov(1000, &[audio_trak(), bframes(750).trak(1)], None), bx(b"mdat", &[&[0; 64]])].concat();
        let expected: Vec<i64> = (0..16).map(|k| k * 1_960).collect();
        assert_eq!(parse(&file), Ok(expected.clone()));

        // The movie box after a large (64-bit) media data box.
        let mut mdat = vec![0, 0, 0, 1];
        mdat.extend_from_slice(b"mdat");
        mdat.extend_from_slice(&(16u64 + 100).to_be_bytes());
        mdat.extend_from_slice(&[0; 100]);
        let file = [ftyp(), mdat, moov(1000, &[bframes(750).trak(1)], None)].concat();
        assert_eq!(parse(&file), Ok(expected));
    }

    #[test]
    fn without_an_edit_list_times_include_the_decoding_delay() {
        let mut v = bframes(100);
        v.edits.clear();
        let file = [ftyp(), moov(1000, &[v.trak(1)], None)].concat();
        assert_eq!(parse(&file), Ok(vec![80, 2_040, 4_000]));
    }

    #[test]
    fn without_a_sync_table_every_sample_is_a_keyframe() {
        let v = Video::new(30_000, 4, 1001);
        let file = [ftyp(), moov(600, &[v.trak(1)], None)].concat();
        assert_eq!(parse(&file), Ok(vec![0, 33, 67, 100]));
    }

    #[test]
    fn a_trimmed_edit_starts_at_its_first_frame() {
        // Cut between two frames (media time 1.38 s): FFmpeg starts the
        // timeline at the first frame shown after the cut (1.4 s), not at the
        // cut, so the keyframe at 2.04 s is at 0.64 s. The one at 0.08 s,
        // before the cut, is only decoded.
        let mut v = bframes(750);
        v.edits = vec![(26_000, 17_664)];
        let file = [ftyp(), moov(1000, &[v.trak(1)], None)].concat();
        let times = parse(&file).unwrap();
        assert_eq!(&times[..3], &[640, 2_600, 4_560]);
    }

    #[test]
    fn an_empty_edit_delays_the_track() {
        let mut v = bframes(100);
        v.edits = vec![(1_500, -1), (4_000, 1024)];
        let file = [ftyp(), moov(1000, &[v.trak(1)], None)].concat();
        assert_eq!(parse(&file), Ok(vec![1_500, 3_460, 5_420]));
    }

    #[test]
    fn still_picture_tracks_are_skipped() {
        let mut cover = Video::new(1000, 1, 1000);
        cover.format = *b"jpeg";
        let file = [ftyp(), moov(1000, &[cover.trak(1), bframes(100).trak(2)], None)].concat();
        assert_eq!(parse(&file), Ok(vec![0, 1_960, 3_920]));
        let file = [ftyp(), moov(1000, &[cover.trak(1)], None)].concat();
        assert_eq!(parse(&file), Err("no video track"));
    }

    #[test]
    fn unusual_files_are_left_to_the_packet_scan() {
        let refuse = |v: Video| parse(&[ftyp(), moov(1000, &[v.trak(1)], None)].concat()).unwrap_err();
        let mut v = bframes(100);
        v.edits = vec![(1_000, 1024), (1_000, 30_000)];
        assert_eq!(refuse(v), "edit list with several edits");
        let mut v = bframes(100);
        v.stss = Some(vec![]);
        assert_eq!(refuse(v), "empty sync sample table");
        let mut v = bframes(100);
        v.stss = Some(vec![51, 1]);
        assert_eq!(refuse(v), "unordered sample numbers");
        let mut v = bframes(100);
        v.extra = vec![fbx(b"sbgp", 0, 0, &[b"rap ".as_slice(), &u32s(&[0])].concat())];
        assert_eq!(refuse(v), "sync points in sample groups");
        let mut v = bframes(100);
        v.stts = vec![(50, 512)];
        assert_eq!(refuse(v), "decode times missing for some samples");

        assert_eq!(parse(b"\0\0\0\x0cRIFF\0\0\0\0"), Err("not an ISO base media file"));
        assert_eq!(parse(&ftyp()), Err("no movie box (moov)"));
        assert_eq!(parse(&[ftyp(), moov(1000, &[audio_trak()], None)].concat()), Err("no video track"));
    }

    fn mvex(flags: u32) -> Vec<u8> {
        bx(b"mvex", &[&fbx(b"trex", 0, 0, &u32s(&[1, 1, 512, 0, flags]))])
    }

    /// A fragment of `samples` frames of 512 ticks from `base`, with a sync
    /// sample every `gop` frames (first-sample flags plus per-sample flags),
    /// shown `delay` ticks after they are decoded.
    fn moof(base: u64, samples: u32, gop: u32, delay: u32) -> Vec<u8> {
        let tfhd = fbx(b"tfhd", 0, 0x02_0020, &u32s(&[1, 0x0101_0000]));
        let tfdt = fbx(b"tfdt", 1, 0, &base.to_be_bytes());
        let mut entries = vec![samples, 0x0200_0000];
        for i in 0..samples {
            entries.push(if i % gop == 0 { 0x0200_0000 } else { 0x0101_0000 });
            entries.push(delay);
        }
        let trun = fbx(b"trun", 1, 0x0004 | 0x0400 | 0x0800, &u32s(&entries));
        let traf = bx(b"traf", &[&tfhd, &tfdt, &trun]);
        let other = bx(b"traf", &[&fbx(b"tfhd", 0, 0, &u32s(&[2])), &fbx(b"trun", 0, 0, &u32s(&[3]))]);
        bx(b"moof", &[&fbx(b"mfhd", 0, 0, &u32s(&[1])), &traf, &other])
    }

    #[test]
    fn reads_movie_fragments() {
        let mut empty = Video::new(12_800, 0, 512);
        empty.stts.clear();
        let head = [ftyp(), moov(1000, &[empty.trak(1)], Some(mvex(0x0101_0000)))].concat();
        let file = [
            head.clone(),
            moof(0, 50, 25, 1024),
            bx(b"mdat", &[&[0; 32]]),
            moof(25_600, 50, 50, 1024),
            bx(b"mdat", &[&[0; 32]]),
            bx(b"mfra", &[]),
        ]
        .concat();
        // Keyframes at 0, 1 s (mid-fragment) and 2 s, plus the 80 ms delay.
        assert_eq!(parse(&file), Ok(vec![80, 1_080, 2_080]));

        // An edit list on fragments only shifts them (FFmpeg's time offset).
        let mut edited = empty;
        edited.edits = vec![(0, 1024)];
        let file = [
            ftyp(),
            moov(1000, &[edited.trak(1)], Some(mvex(0x0101_0000))),
            moof(0, 50, 50, 1024),
            moof(25_600, 50, 50, 1024),
        ]
        .concat();
        assert_eq!(parse(&file), Ok(vec![0, 2_000]));

        // Fragments too slow to walk are given up on.
        let late = Some(Instant::now());
        let many: Vec<u8> = (0..200).flat_map(|k| moof(k * 25_600, 50, 50, 0)).collect();
        let file = [head, many].concat();
        assert_eq!(index(Cursor::new(&file), file.len() as u64, late), Err("too many fragments to read in time"));
        assert_eq!(parse(&file).map(|t| t.len()), Ok(200));
    }

    /// A version 1 fragment (CMAF) whose first sample only is a keyframe.
    fn cmaf_moof(base: u64, offsets: &[i32]) -> Vec<u8> {
        let tfhd = fbx(b"tfhd", 0, 0x02_0000, &u32s(&[1]));
        let tfdt = fbx(b"tfdt", 1, 0, &base.to_be_bytes());
        let mut entries = vec![offsets.len() as u32];
        for (i, &offset) in offsets.iter().enumerate() {
            entries.extend([if i == 0 { 0x0200_0000 } else { 0x0101_0000 }, offset as u32]);
        }
        let trun = fbx(b"trun", 1, 0x0400 | 0x0800, &u32s(&entries));
        bx(b"moof", &[&bx(b"traf", &[&tfhd, &tfdt, &trun])])
    }

    #[test]
    fn negative_offsets_in_fragments_delay_the_whole_track() {
        // No edit list: FFmpeg delays every fragment by the most negative
        // composition offset of the track, wherever it is.
        let mut empty = Video::new(12_800, 0, 512);
        empty.stts.clear();
        let head = [ftyp(), moov(1000, &[empty.trak(1)], Some(mvex(0x0101_0000)))].concat();
        let file = [head.clone(), cmaf_moof(0, &[0, 512, -512]), cmaf_moof(1536, &[0, -1024, 512])].concat();
        assert_eq!(parse(&file), Ok(vec![80, 200]));
        let file = [head, cmaf_moof(0, &[0, 512, 512])].concat();
        assert_eq!(parse(&file), Ok(vec![0]));
    }

    #[test]
    fn fragment_tables_are_bounded() {
        let mut empty = Video::new(12_800, 0, 512);
        empty.stts.clear();
        let head = [ftyp(), moov(1000, &[empty.trak(1)], Some(mvex(0x0101_0000)))].concat();
        // A run claiming four billion samples in a few bytes.
        let tfhd = fbx(b"tfhd", 0, 0, &u32s(&[1]));
        let trun = fbx(b"trun", 0, 0x0400, &u32s(&[u32::MAX, 0]));
        let file = [head.clone(), bx(b"moof", &[&bx(b"traf", &[&tfhd, &trun])])].concat();
        assert_eq!(parse(&file), Err("track run lists more samples than it holds"));
        // Without per-sample fields, the sample budget stops it.
        let trun = fbx(b"trun", 0, 0, &u32s(&[u32::MAX]));
        let file = [head, bx(b"moof", &[&bx(b"traf", &[&tfhd, &trun])])].concat();
        assert_eq!(parse(&file), Err("too many samples"));
    }

    #[test]
    fn hostile_sizes_never_panic() {
        let file = [ftyp(), moov(1000, &[audio_trak(), bframes(120).trak(1)], None), bx(b"mdat", &[&[0; 16]])].concat();
        assert!(parse(&file).is_ok());
        // Every byte set to a few nasty values: errors or other answers, never a panic.
        for i in 0..file.len() {
            for value in [0x00, 0x01, 0x7F, 0xFF] {
                let mut damaged = file.clone();
                damaged[i] = value;
                let _ = parse(&damaged);
            }
        }
        // Truncated anywhere.
        for len in 0..file.len() {
            let _ = parse(&file[..len]);
        }
        // A box claiming more bytes than its parent holds.
        let mut v = bframes(10);
        v.extra = vec![[&u32::MAX.to_be_bytes()[..], b"free"].concat()];
        assert_eq!(parse(&[ftyp(), moov(1000, &[v.trak(1)], None)].concat()), Err("box extends past its parent"));
        // A table claiming more entries than it holds.
        let mut v = bframes(10);
        v.stts.clear();
        v.extra = vec![table(b"stts", &[u32::MAX, 10, 512])];
        assert_eq!(
            parse(&[ftyp(), moov(1000, &[v.trak(1)], None)].concat()),
            Err("table lists more entries than it holds")
        );
    }

    /// Compares every MP4/MOV/M4V file in a folder with FFmpeg's packet scan:
    /// `PLEY_TEST_MP4_DIR=<dir> cargo test -p pley-transcoder matches_ffprobe -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn matches_ffprobe() {
        let dir = std::env::var("PLEY_TEST_MP4_DIR").expect("set PLEY_TEST_MP4_DIR");
        let mut failures = Vec::new();
        let mut entries: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
        entries.sort();
        for path in entries {
            let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            if !matches!(ext.as_str(), "mp4" | "m4v" | "mov") {
                continue;
            }
            let started = Instant::now();
            let ours = keyframes(&path, None);
            let took = started.elapsed();
            let output = std::process::Command::new("ffprobe")
                .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts_time,flags"])
                .args(["-of", "csv=p=0"])
                .arg(&path)
                .output()
                .unwrap();
            let mut theirs: Vec<i64> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter_map(crate::keyframes::parse_packet_line)
                .collect();
            theirs.sort_unstable();
            theirs.dedup();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            match &ours {
                Ok(ours) if *ours == theirs => {
                    println!("ok    {name}: {} keyframes in {took:?}", ours.len())
                }
                Ok(ours) => {
                    let diff = ours.iter().zip(&theirs).position(|(a, b)| a != b);
                    println!(
                        "DIFF  {name}: ours {} vs ffprobe {}, first difference at {diff:?}: {:?} vs {:?}",
                        ours.len(),
                        theirs.len(),
                        diff.map(|i| &ours[i..(i + 3).min(ours.len())]),
                        diff.map(|i| &theirs[i..(i + 3).min(theirs.len())])
                    );
                    failures.push(name);
                }
                Err(why) => println!("skip  {name}: {why} (ffprobe: {} keyframes)", theirs.len()),
            }
        }
        assert!(failures.is_empty(), "different from ffprobe: {failures:?}");
    }
}

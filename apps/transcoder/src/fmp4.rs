// SPDX-License-Identifier: LicenseRef-Restricted
//! FFmpeg's fragmented MP4 output (`-f mp4 -movflags +frag_keyframe+…` on
//! a pipe), read box by box and taken apart: the initialization (`ftyp`
//! and `moov`) and each movie fragment (`moof` and its `mdat`), with the
//! times needed to put fragments in segments and a writer that puts an
//! edited fragment back together. Dolby Vision remuxes have their samples'
//! NAL units edited as MPEG-TS segments are ([`crate::tsedit`]) and their
//! init given the configuration record the stream now has ([`set_dovi`]).
//!
//! Everything read here comes from FFmpeg, which reads files from anywhere,
//! so every size and count is bounded; what the bytes are used for is the
//! caller's business.

use std::io;
use std::ops::Range;

use tokio::io::{AsyncRead, AsyncReadExt};

use crate::dovi::NalEdit;
use crate::mp4::Fields;
use crate::tsedit::{self, Edited, Edits};

/// Why a box could not be used.
pub type Result<T> = std::result::Result<T, &'static str>;

/// Largest top-level boxes read whole, by type.
const MAX_FTYP: u64 = 64 << 10;
const MAX_MOOV: u64 = 16 << 20;
const MAX_MOOF: u64 = 16 << 20;
const MAX_MDAT: u64 = 1 << 30;
/// Largest other top-level box (`free`, `sidx`, `styp`, `mfra`, …), skipped
/// without being kept.
const MAX_SKIPPED: u64 = 16 << 20;
/// Most samples in one track run, and in one fragment.
const MAX_RUN_SAMPLES: u32 = 1_000_000;
const MAX_FRAGMENT_SAMPLES: usize = 1_000_000;

/// `tfhd` flags.
const TFHD_BASE_DATA_OFFSET: u32 = 0x01;
const TFHD_DESCRIPTION: u32 = 0x02;
const TFHD_DURATION: u32 = 0x08;
const TFHD_SIZE: u32 = 0x10;
const TFHD_FLAGS: u32 = 0x20;
const TFHD_DEFAULT_BASE_IS_MOOF: u32 = 0x02_0000;
/// `trun` flags.
const TRUN_DATA_OFFSET: u32 = 0x01;
const TRUN_FIRST_FLAGS: u32 = 0x04;
const TRUN_DURATION: u32 = 0x100;
const TRUN_SIZE: u32 = 0x200;
const TRUN_FLAGS: u32 = 0x400;
const TRUN_CTO: u32 = 0x800;

// ---------------------------------------------------------------- top-level boxes

/// A whole top-level box, header included.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopBox {
    pub kind: [u8; 4],
    pub bytes: Vec<u8>,
}

/// Reads top-level boxes from a stream as they arrive.
pub struct BoxReader<R: AsyncRead + Unpin> {
    inner: R,
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn truncated() -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, "the stream ends inside a box")
}

impl<R: AsyncRead + Unpin> BoxReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

    /// The next `ftyp`, `moov`, `moof` or `mdat`, skipping any other box;
    /// `None` at the end of the stream between boxes.
    pub async fn next(&mut self) -> io::Result<Option<TopBox>> {
        loop {
            let mut head = [0u8; 16];
            let mut got = 0;
            while got < 8 {
                let n = self.inner.read(&mut head[got..8]).await?;
                if n == 0 {
                    return if got == 0 { Ok(None) } else { Err(truncated()) };
                }
                got += n;
            }
            let kind: [u8; 4] = [head[4], head[5], head[6], head[7]];
            let (size, header) = match u32::from_be_bytes([head[0], head[1], head[2], head[3]]) {
                0 => return Err(invalid("a box that runs to the end of the stream")),
                1 => {
                    self.inner.read_exact(&mut head[8..16]).await.map_err(|_| truncated())?;
                    (u64::from_be_bytes(head[8..16].try_into().unwrap_or([0; 8])), 16)
                }
                n => (u64::from(n), 8),
            };
            if size < header as u64 {
                return Err(invalid("a box smaller than its header"));
            }
            let limit = match &kind {
                b"ftyp" => MAX_FTYP,
                b"moov" => MAX_MOOV,
                b"moof" => MAX_MOOF,
                b"mdat" => MAX_MDAT,
                _ => {
                    if size > MAX_SKIPPED {
                        return Err(invalid("a box too large to skip"));
                    }
                    let rest = size - header as u64;
                    let skipped = tokio::io::copy(&mut (&mut self.inner).take(rest), &mut tokio::io::sink()).await?;
                    if skipped < rest {
                        return Err(truncated());
                    }
                    continue;
                }
            };
            if size > limit {
                return Err(invalid("a box larger than allowed"));
            }
            // Grows as the data arrives rather than trusting the size up front.
            let mut bytes = Vec::with_capacity(size.min(1 << 20) as usize);
            bytes.extend_from_slice(&head[..header]);
            let rest = size - header as u64;
            let read = (&mut self.inner).take(rest).read_to_end(&mut bytes).await?;
            if (read as u64) < rest {
                return Err(truncated());
            }
            return Ok(Some(TopBox { kind, bytes }));
        }
    }
}

// ---------------------------------------------------------------- boxes in memory

struct Child<'a> {
    kind: [u8; 4],
    /// The whole box.
    whole: &'a [u8],
    payload: &'a [u8],
}

/// The boxes one after another in `data` (a box's payload).
fn children(data: &[u8]) -> Result<Vec<Child<'_>>> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < data.len() {
        let mut f = Fields::new(&data[pos..]);
        let size = f.u32()?;
        let kind = f.take::<4>()?;
        let (size, header) = match size {
            0 => ((data.len() - pos) as u64, 8),
            1 => (f.u64()?, 16),
            n => (u64::from(n), 8),
        };
        if size < header as u64 || size > (data.len() - pos) as u64 {
            return Err("a box's size doesn't fit");
        }
        let whole = &data[pos..pos + size as usize];
        out.push(Child { kind, whole, payload: &whole[header..] });
        pos += size as usize;
    }
    Ok(out)
}

fn child<'a>(boxes: &'a [Child<'a>], kind: &[u8; 4]) -> Option<&'a Child<'a>> {
    boxes.iter().find(|c| c.kind == *kind)
}

/// The payload of a whole box (`bytes` as [`BoxReader`] gives it).
fn payload(bytes: &[u8]) -> Result<&[u8]> {
    let header = if bytes.get(..4) == Some(&[0, 0, 0, 1]) { 16 } else { 8 };
    bytes.get(header..).ok_or("box too short")
}

// ---------------------------------------------------------------- initialization

/// A track of the initialization, as fragments need it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackInfo {
    pub id: u32,
    /// `vide`, `soun`, …
    pub handler: [u8; 4],
    pub timescale: u32,
    /// How far the edit list moves presentation back (media ticks): FFmpeg
    /// writes the B-frame delay here.
    pub shift: i64,
    /// The sample entry's type (`avc1`, `hvc1`, `mp4a`, `ac-3`, …).
    pub format: [u8; 4],
    /// Bytes of each NAL unit's length in a sample (`avcC`, `hvcC`).
    pub nal_length_size: Option<u8>,
    /// The Dolby Vision configuration record of the sample entry.
    pub dovi: Option<DoviConfig>,
    defaults: Option<Defaults>,
}

/// A track's sample defaults (`trex`, or `tfhd` over it).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Defaults {
    duration: Option<u32>,
    size: Option<u32>,
    flags: Option<u32>,
}

/// The initialization segment: `ftyp` and `moov`.
#[derive(Debug, Clone)]
pub struct Init {
    pub bytes: Vec<u8>,
    pub tracks: Vec<TrackInfo>,
    /// The first video track's id.
    pub video: Option<u32>,
}

impl Init {
    /// `ftyp` and `moov` are whole boxes, header included.
    pub fn parse(ftyp: &[u8], moov: &[u8]) -> Result<Init> {
        let boxes = children(moov)?;
        let [moov] = boxes.as_slice() else { return Err("not one movie box") };
        if moov.kind != *b"moov" || children(ftyp)?.first().is_none_or(|b| b.kind != *b"ftyp") {
            return Err("not a file type and movie box");
        }
        let kids = children(moov.payload)?;
        let mvhd = child(&kids, b"mvhd").ok_or("no movie header")?;
        let movie_scale = timescale(mvhd.payload)?;
        let mut tracks = Vec::new();
        for trak in kids.iter().filter(|c| c.kind == *b"trak") {
            tracks.push(track(trak.payload, movie_scale)?);
        }
        if let Some(mvex) = child(&kids, b"mvex") {
            for trex in children(mvex.payload)?.iter().filter(|c| c.kind == *b"trex") {
                let mut f = Fields::new(trex.payload);
                f.skip(4)?;
                let id = f.u32()?;
                f.skip(4)?; // sample description index
                let defaults = Defaults { duration: Some(f.u32()?), size: Some(f.u32()?), flags: Some(f.u32()?) };
                if let Some(t) = tracks.iter_mut().find(|t| t.id == id) {
                    t.defaults = Some(defaults);
                }
            }
        }
        let video = tracks.iter().find(|t| t.handler == *b"vide").map(|t| t.id);
        Ok(Init { bytes: [ftyp, moov.whole].concat(), tracks, video })
    }

    /// An init as [`set_dovi`] gives it: `ftyp` and `moov` one after the
    /// other.
    pub fn from_bytes(bytes: &[u8]) -> Result<Init> {
        match children(bytes)?.as_slice() {
            [ftyp, moov] => Init::parse(ftyp.whole, moov.whole),
            _ => Err("not a file type and movie box"),
        }
    }

    pub fn track(&self, id: u32) -> Option<&TrackInfo> {
        self.tracks.iter().find(|t| t.id == id)
    }
}

/// The timescale of an `mvhd` or `mdhd` payload.
fn timescale(payload: &[u8]) -> Result<u32> {
    let mut f = Fields::new(payload);
    let version = f.take::<4>()?[0];
    f.skip(if version == 1 { 16 } else { 8 })?;
    match f.u32()? {
        0 => Err("timescale of zero"),
        n => Ok(n),
    }
}

fn track(trak: &[u8], movie_scale: u32) -> Result<TrackInfo> {
    let kids = children(trak)?;
    let tkhd = child(&kids, b"tkhd").ok_or("track without a header")?;
    let mut f = Fields::new(tkhd.payload);
    let version = f.take::<4>()?[0];
    f.skip(if version == 1 { 16 } else { 8 })?;
    let id = f.u32()?;

    let mdia = children(child(&kids, b"mdia").ok_or("track without media")?.payload)?;
    let timescale = timescale(child(&mdia, b"mdhd").ok_or("track without a media header")?.payload)?;
    let hdlr = child(&mdia, b"hdlr").ok_or("track without a handler")?.payload;
    let handler: [u8; 4] = hdlr.get(8..12).and_then(|h| h.try_into().ok()).ok_or("handler too short")?;

    let minf = children(child(&mdia, b"minf").ok_or("track without media information")?.payload)?;
    let stbl = children(child(&minf, b"stbl").ok_or("track without sample tables")?.payload)?;
    let stsd = child(&stbl, b"stsd").ok_or("track without sample descriptions")?.payload;
    let entries = children(stsd.get(8..).ok_or("sample descriptions too short")?)?;
    let entry = entries.first().ok_or("no sample description")?;
    let mut nal_length_size = None;
    let mut dovi = None;
    if handler == *b"vide" {
        // A visual sample entry's own fields take 78 bytes before its boxes.
        for c in children(entry.payload.get(78..).unwrap_or_default()).unwrap_or_default() {
            let at = match &c.kind {
                b"avcC" => 4,
                b"hvcC" => 21,
                kind if DOVI_BOXES.contains(kind) => {
                    dovi = DoviConfig::parse(c.payload);
                    continue;
                }
                _ => continue,
            };
            let byte = *c.payload.get(at).ok_or("decoder configuration too short")?;
            nal_length_size = Some((byte & 3) + 1);
        }
    }

    let mut shift = 0;
    if let Some(edts) = child(&kids, b"edts")
        && let Some(elst) = child(&children(edts.payload)?, b"elst")
    {
        shift = edit_shift(elst.payload, movie_scale, timescale)?;
    }
    Ok(TrackInfo { id, handler, timescale, shift, format: entry.kind, nal_length_size, dovi, defaults: None })
}

/// The media time of the first edit that shows media, less the empty edits
/// before it (which delay presentation), in media ticks.
fn edit_shift(elst: &[u8], movie_scale: u32, media_scale: u32) -> Result<i64> {
    let mut f = Fields::new(elst);
    let version = f.take::<4>()?[0];
    let n = f.count(if version == 1 { 20 } else { 12 })?;
    let mut empty: i128 = 0;
    for _ in 0..n {
        let (duration, media_time) =
            if version == 1 { (f.u64()?, f.i64()?) } else { (u64::from(f.u32()?), i64::from(f.i32()?)) };
        f.skip(4)?; // rate
        if media_time >= 0 {
            let empty = empty * i128::from(media_scale) / i128::from(movie_scale);
            return i64::try_from(i128::from(media_time) - empty).map_err(|_| "edit list out of range");
        }
        empty += i128::from(duration);
    }
    Ok(0)
}

// ---------------------------------------------------------------- fragments

/// A movie fragment: its `moof` and `mdat`.
#[derive(Debug, Clone)]
pub struct Fragment {
    /// The `moof` and `mdat` as read.
    pub bytes: Vec<u8>,
    pub sequence: u32,
    pub trafs: Vec<Traf>,
    /// Where the `mdat` starts in `bytes`, and its header's length.
    mdat_at: usize,
    mdat_header: usize,
    /// Boxes of the `moof` other than `mfhd` and `traf`, kept as they were.
    other: Vec<Vec<u8>>,
}

/// One track's part of a fragment.
#[derive(Debug, Clone)]
pub struct Traf {
    pub track: u32,
    pub flags: u32,
    pub base_data_offset: Option<u64>,
    pub description: Option<u32>,
    pub default_duration: Option<u32>,
    pub default_size: Option<u32>,
    pub default_flags: Option<u32>,
    /// Decode time of the first sample (media ticks).
    pub tfdt: i64,
    tfdt_version: u8,
    pub runs: Vec<Run>,
    /// Boxes other than `tfhd`, `tfdt` and `trun`, kept as they were.
    pub other: Vec<Vec<u8>>,
    /// `tfhd` over `trex`.
    defaults: Defaults,
}

/// A track run (`trun`).
#[derive(Debug, Clone)]
pub struct Run {
    pub version: u8,
    pub flags: u32,
    pub first_sample_flags: Option<u32>,
    pub samples: Vec<Sample>,
    /// The run's bytes in the `mdat`'s payload, when they can be found
    /// there.
    pub data: Option<Range<usize>>,
    /// The run's bytes as [`Fragment::edit_video`] made them, in place of
    /// `data`'s.
    edited: Option<Vec<u8>>,
}

/// A sample, with the defaults filled in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sample {
    pub duration: u32,
    pub size: u32,
    pub flags: u32,
    /// Composition time offset.
    pub cto: i64,
}

impl Fragment {
    /// `moof` and `mdat` are whole boxes, header included.
    pub fn parse(moof: Vec<u8>, mdat: Vec<u8>, init: &Init) -> Result<Fragment> {
        let moof_len = moof.len();
        let mdat_header = if mdat.get(..4) == Some(&[0, 0, 0, 1]) { 16 } else { 8 };
        if mdat.len() < mdat_header || mdat.get(4..8) != Some(b"mdat") || moof.get(4..8) != Some(b"moof") {
            return Err("not a movie fragment and its media data");
        }
        let mdat_len = mdat.len() - mdat_header;
        let mut sequence = None;
        let mut trafs = Vec::new();
        let mut other = Vec::new();
        let mut samples = 0;
        for c in children(payload(&moof)?)? {
            match &c.kind {
                b"mfhd" => sequence = Some(Fields::new(c.payload.get(4..).unwrap_or_default()).u32()?),
                b"traf" => trafs.push(parse_traf(c.payload, init, moof_len, mdat_header, mdat_len, &mut samples)?),
                _ => other.push(c.whole.to_vec()),
            }
        }
        let sequence = sequence.ok_or("fragment without a header")?;
        let mut bytes = moof;
        bytes.extend_from_slice(&mdat);
        Ok(Fragment { bytes, sequence, trafs, mdat_at: moof_len, mdat_header, other })
    }

    /// Whether the fragment can be written again by [`to_bytes`](Self::to_bytes):
    /// every run's data is addressed from the start of the `moof` and lies
    /// in the `mdat`.
    pub fn rewritable(&self) -> bool {
        self.trafs.iter().all(|t| {
            t.flags & TFHD_DEFAULT_BASE_IS_MOOF != 0
                && t.base_data_offset.is_none()
                && t.runs.iter().all(|r| r.flags & TRUN_DATA_OFFSET != 0 && r.data.is_some())
        })
    }

    /// When the fragment's first video sample is presented (ms), as FFmpeg
    /// reads it: its decode time plus its composition offset, less the edit
    /// list's shift. With B-frames a keyframe is decoded before it is shown,
    /// and FFmpeg's seeking and segment times count in presentation time.
    pub fn presentation_start_ms(&self, init: &Init) -> Option<i64> {
        let video = init.track(init.video?)?;
        let traf = self.trafs.iter().find(|t| t.track == video.id)?;
        let first = traf.runs.iter().find_map(|r| r.samples.first())?;
        let ticks = i128::from(traf.tfdt) + i128::from(first.cto) - i128::from(video.shift);
        let scale = i128::from(video.timescale);
        i64::try_from((2 * ticks * 1000 + scale).div_euclid(2 * scale)).ok()
    }

    /// Whether an audio track starts before 0 (encoder priming), which
    /// [`trim_before_zero`](Self::trim_before_zero) takes out.
    pub fn needs_trim(&self, init: &Init) -> bool {
        self.trafs.iter().any(|t| t.tfdt < 0 && init.track(t.track).is_some_and(|i| i.handler == *b"soun"))
    }

    /// Takes out the audio samples decoded before 0 and their data, so the
    /// stream starts at 0 (or at the first whole frame after it) rather
    /// than giving players a negative start. Video is never trimmed. Gives
    /// whether anything changed; a fragment that isn't
    /// [`rewritable`](Self::rewritable) is left as it is.
    pub fn trim_before_zero(&mut self, init: &Init) -> bool {
        if !self.rewritable() {
            return false;
        }
        let mut changed = false;
        for traf in &mut self.trafs {
            if traf.tfdt >= 0 {
                continue;
            }
            match init.track(traf.track).map(|t| t.handler) {
                Some(h) if h == *b"soun" => {}
                Some(h) if h == *b"vide" => {
                    tracing::debug!(
                        track = traf.track,
                        tfdt = traf.tfdt,
                        "video fragment decoded before 0 left as it is"
                    );
                    continue;
                }
                _ => continue,
            }
            let mut dts = traf.tfdt;
            for run in &mut traf.runs {
                let mut dropped = 0;
                let mut bytes = 0;
                for s in &run.samples {
                    if dts >= 0 {
                        break;
                    }
                    dts += i64::from(s.duration);
                    bytes += s.size as usize;
                    dropped += 1;
                }
                if dropped > 0 {
                    run.samples.drain(..dropped);
                    match (&mut run.edited, &mut run.data) {
                        (Some(edited), _) => drop(edited.drain(..bytes)),
                        (None, Some(data)) => data.start += bytes,
                        (None, None) => {}
                    }
                    changed = true;
                }
            }
            traf.runs.retain(|r| !r.samples.is_empty());
            traf.tfdt = dts;
        }
        self.trafs.retain(|t| !t.runs.is_empty());
        changed
    }

    /// Makes `edits` to the HEVC NAL units of every video sample (each unit
    /// after its length, in `nal_length_size` bytes), as [`crate::tsedit`]
    /// makes them to MPEG-TS segments: the samples' sizes follow, and
    /// [`to_bytes`](Self::to_bytes) works the data offsets and the `mdat`
    /// out anew. Gives what was done, or `None` when nothing changed; an
    /// error, changing nothing, when the fragment isn't
    /// [`rewritable`](Self::rewritable) or a sample's units don't read.
    pub fn edit_video(&mut self, init: &Init, edits: Edits) -> Result<Option<Edited>> {
        let Some(video) = init.video.and_then(|id| init.track(id)) else { return Ok(None) };
        if !edits.any() || !self.trafs.iter().any(|t| t.track == video.id) {
            return Ok(None);
        }
        if !self.rewritable() {
            return Err("the fragment's data isn't addressed from its movie fragment box");
        }
        let length_size = usize::from(video.nal_length_size.ok_or("no NAL unit length size")?);
        let mdat = &self.bytes[self.mdat_at + self.mdat_header..];
        // Every run worked out before any changes, so that a sample that
        // doesn't read leaves the fragment as it was.
        let mut edited = Edited::default();
        let mut new_runs = Vec::new();
        for (t, traf) in self.trafs.iter().enumerate().filter(|(_, t)| t.track == video.id) {
            for (r, run) in traf.runs.iter().enumerate() {
                let data = match (&run.edited, &run.data) {
                    (Some(edited), _) => edited.as_slice(),
                    (None, Some(data)) => &mdat[data.clone()],
                    (None, None) => continue,
                };
                let mut out = Vec::with_capacity(data.len());
                let mut sizes = Vec::with_capacity(run.samples.len());
                let mut changed = false;
                let mut at = 0;
                for s in &run.samples {
                    let sample = data.get(at..at + s.size as usize).ok_or("a sample runs past its run's data")?;
                    at += sample.len();
                    let before = out.len();
                    changed |= edit_sample(sample, length_size, edits, &mut edited, &mut out)?;
                    sizes.push(u32::try_from(out.len() - before).map_err(|_| "an edited sample too large")?);
                }
                if changed {
                    new_runs.push((t, r, out, sizes));
                }
            }
        }
        if new_runs.is_empty() {
            return Ok(None);
        }
        for (t, r, out, sizes) in new_runs {
            let run = &mut self.trafs[t].runs[r];
            for (sample, size) in run.samples.iter_mut().zip(sizes) {
                sample.size = size;
            }
            run.edited = Some(out);
        }
        Ok(Some(edited))
    }

    /// Each sample's data of track `id`, as the fragment has it now.
    #[cfg(test)]
    pub(crate) fn samples(&self, id: u32) -> Vec<&[u8]> {
        let mdat = &self.bytes[self.mdat_at + self.mdat_header..];
        let mut out = Vec::new();
        for run in self.trafs.iter().filter(|t| t.track == id).flat_map(|t| &t.runs) {
            let data = match (&run.edited, &run.data) {
                (Some(edited), _) => edited.as_slice(),
                (None, Some(data)) => &mdat[data.clone()],
                (None, None) => continue,
            };
            let mut at = 0;
            for s in &run.samples {
                out.push(&data[at..at + s.size as usize]);
                at += s.size as usize;
            }
        }
        out
    }

    /// The fragment written out again: `moof` (with every run's data offset
    /// and every size worked out anew) and `mdat`, the runs' data in the
    /// order it was in. Unchanged, this is the bytes FFmpeg wrote. A
    /// fragment that isn't [`rewritable`](Self::rewritable) is given as it
    /// was read.
    pub fn to_bytes(&self) -> Vec<u8> {
        if !self.rewritable() {
            return self.bytes.clone();
        }
        let mdat = &self.bytes[self.mdat_at + self.mdat_header..];

        // Each run's place in the new mdat, in the original order.
        let mut order: Vec<(usize, usize, &Range<usize>)> = Vec::new();
        for (t, traf) in self.trafs.iter().enumerate() {
            for (r, run) in traf.runs.iter().enumerate() {
                if let Some(data) = &run.data {
                    order.push((t, r, data));
                }
            }
        }
        order.sort_by_key(|(_, _, data)| data.start);
        let mut payload = Vec::new();
        let mut places: Vec<Vec<usize>> = self.trafs.iter().map(|t| vec![0; t.runs.len()]).collect();
        for (t, r, data) in order {
            places[t][r] = payload.len();
            match &self.trafs[t].runs[r].edited {
                Some(edited) => payload.extend_from_slice(edited),
                None => payload.extend_from_slice(&mdat[data.clone()]),
            }
        }
        let mdat_header = if payload.len() + 8 > u32::MAX as usize { 16 } else { self.mdat_header };

        let mut out = Vec::with_capacity(self.mdat_at + mdat_header + payload.len());
        let mut patches = Vec::new();
        let moof = open_box(&mut out, b"moof");
        let mfhd = open_box(&mut out, b"mfhd");
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&self.sequence.to_be_bytes());
        close_box(&mut out, mfhd);
        for (traf, places) in self.trafs.iter().zip(&places) {
            traf.write(&mut out, places, &mut patches);
        }
        for other in &self.other {
            out.extend_from_slice(other);
        }
        close_box(&mut out, moof);
        let base = out.len() + mdat_header;
        for (at, place) in patches {
            let offset = (base + place) as u32;
            out[at..at + 4].copy_from_slice(&offset.to_be_bytes());
        }
        if mdat_header == 16 {
            out.extend_from_slice(&1u32.to_be_bytes());
            out.extend_from_slice(b"mdat");
            out.extend_from_slice(&((payload.len() + 16) as u64).to_be_bytes());
        } else {
            out.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
            out.extend_from_slice(b"mdat");
        }
        out.extend_from_slice(&payload);
        out
    }
}

/// Writes `sample`'s NAL units (each after its length, in `length_size`
/// bytes) to `out` as `edits` leave them; whether any changed.
fn edit_sample(
    sample: &[u8],
    length_size: usize,
    edits: Edits,
    edited: &mut Edited,
    out: &mut Vec<u8>,
) -> Result<bool> {
    let mut changed = false;
    let mut at = 0;
    while at < sample.len() {
        let length = sample.get(at..at + length_size).ok_or("a NAL unit's length is cut off")?;
        let length = length.iter().fold(0usize, |n, &b| (n << 8) | usize::from(b));
        let nal = sample.get(at + length_size..at + length_size + length).ok_or("a NAL unit runs past its sample")?;
        match tsedit::nal_decision(nal, edits, edited) {
            NalEdit::Keep => out.extend_from_slice(&sample[at..at + length_size + length]),
            NalEdit::Drop => changed = true,
            NalEdit::Replace(new) => {
                if (new.len() as u64) >> (8 * length_size) != 0 {
                    return Err("an edited NAL unit too long for its length field");
                }
                out.extend_from_slice(&(new.len() as u64).to_be_bytes()[8 - length_size..]);
                out.extend_from_slice(&new);
                changed = true;
            }
        }
        at += length_size + length;
    }
    Ok(changed)
}

// ---------------------------------------------------------------- Dolby Vision in the init

/// The boxes a Dolby Vision configuration record comes in: `dvcC` up to
/// profile 7, `dvvC` for 8 to 10, `dvwC` above.
const DOVI_BOXES: [[u8; 4]; 3] = [*b"dvcC", *b"dvvC", *b"dvwC"];

/// A Dolby Vision decoder configuration record (Dolby's
/// `DOVIDecoderConfigurationRecord`, version 1.0), which a video sample
/// entry carries after its `hvcC`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoviConfig {
    pub profile: u8,
    pub level: u8,
    pub rpu: bool,
    /// An enhancement layer in the stream.
    pub el: bool,
    pub bl: bool,
    /// The base layer's signal compatibility id (0 none, 1 HDR10, 2 SDR,
    /// 4 HLG, 6 Blu-ray HDR10).
    pub compat: u8,
}

impl DoviConfig {
    fn kind(&self) -> [u8; 4] {
        DOVI_BOXES[match self.profile {
            0..=7 => 0,
            8..=10 => 1,
            _ => 2,
        }]
    }

    /// The box: 24 bytes of record after its header, the reserved bits 0.
    fn to_box(self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32);
        let at = open_box(&mut out, &self.kind());
        let bits = (u16::from(self.profile & 0x7F) << 9)
            | (u16::from(self.level & 0x3F) << 3)
            | (u16::from(self.rpu) << 2)
            | (u16::from(self.el) << 1)
            | u16::from(self.bl);
        out.extend_from_slice(&[1, 0]);
        out.extend_from_slice(&bits.to_be_bytes());
        out.push(self.compat << 4);
        out.extend_from_slice(&[0; 19]);
        close_box(&mut out, at);
        out
    }

    fn parse(payload: &[u8]) -> Option<DoviConfig> {
        let bits = u16::from_be_bytes([*payload.get(2)?, *payload.get(3)?]);
        Some(DoviConfig {
            profile: (bits >> 9) as u8,
            level: ((bits >> 3) & 0x3F) as u8,
            rpu: bits & 4 != 0,
            el: bits & 2 != 0,
            bl: bits & 1 != 0,
            compat: payload.get(4)? >> 4,
        })
    }
}

/// The init (`ftyp` and `moov`, as [`Init::bytes`]) with its video sample
/// entry's Dolby Vision configuration record replaced by `config` (put
/// right after the `hvcC`), or taken out when `None`; the file type's
/// compatible brands then have `dby1` (Dolby Vision) or don't. Every box
/// around the record gets its new size.
pub fn set_dovi(init: &[u8], config: Option<DoviConfig>) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(init.len() + 32);
    let mut video = false;
    for top in children(init)? {
        match &top.kind {
            b"ftyp" => {
                // Major brand and version, then the compatible brands.
                let (head, brands) = top.payload.split_at_checked(8).ok_or("file type box too short")?;
                let at = open_box(&mut out, b"ftyp");
                out.extend_from_slice(head);
                brands.chunks(4).filter(|b| *b != b"dby1").for_each(|b| out.extend_from_slice(b));
                if config.is_some() {
                    out.extend_from_slice(b"dby1");
                }
                close_box(&mut out, at);
            }
            b"moov" => {
                // moov/trak (video)/mdia/minf/stbl/stsd (after its count)/
                // the first entry (after its 78 bytes of fields).
                let path: [(Pick, usize); 6] = [
                    (|_, c| c.kind == *b"trak" && is_video_track(c), 0),
                    (|_, c| c.kind == *b"mdia", 0),
                    (|_, c| c.kind == *b"minf", 0),
                    (|_, c| c.kind == *b"stbl", 0),
                    (|_, c| c.kind == *b"stsd", 8),
                    (|i, _| i == 0, 78),
                ];
                out.extend(rebuild(&top, 0, &path, &mut |boxes| Ok(entry_boxes(boxes, config)))?);
                video = true;
            }
            _ => out.extend_from_slice(top.whole),
        }
    }
    if !video {
        return Err("no movie box");
    }
    Ok(out)
}

/// Which child of a box a path through the boxes goes on to, by its place
/// and the box.
type Pick = fn(usize, &Child) -> bool;

/// `whole` written again (its first `skip` payload bytes as they were):
/// the child `path` picks made anew in the same way down the path, and the
/// children at its end made by `edit`.
fn rebuild(
    whole: &Child,
    skip: usize,
    path: &[(Pick, usize)],
    edit: &mut dyn FnMut(&[Child]) -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    let fields = whole.payload.get(..skip).ok_or("a box too short")?;
    let kids = children(&whole.payload[skip..])?;
    let body = match path.split_first() {
        None => edit(&kids)?,
        Some(((pick, child_skip), rest)) => {
            let mut body = Vec::new();
            let mut found = false;
            for (i, c) in kids.iter().enumerate() {
                if !found && pick(i, c) {
                    body.extend(rebuild(c, *child_skip, rest, edit)?);
                    found = true;
                } else {
                    body.extend_from_slice(c.whole);
                }
            }
            if !found {
                return Err("no video sample entry");
            }
            body
        }
    };
    let mut out = Vec::with_capacity(8 + fields.len() + body.len());
    let at = open_box(&mut out, &whole.kind);
    out.extend_from_slice(fields);
    out.extend(body);
    close_box(&mut out, at);
    Ok(out)
}

fn is_video_track(trak: &Child) -> bool {
    let Ok(kids) = children(trak.payload) else { return false };
    let Some(Ok(mdia)) = child(&kids, b"mdia").map(|m| children(m.payload)) else { return false };
    child(&mdia, b"hdlr").and_then(|h| h.payload.get(8..12)) == Some(&b"vide"[..])
}

/// A sample entry's boxes without any Dolby Vision record, `config`'s
/// after the decoder configuration (or last, without one).
fn entry_boxes(boxes: &[Child], config: Option<DoviConfig>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut record = config.map(DoviConfig::to_box);
    for b in boxes.iter().filter(|b| !DOVI_BOXES.contains(&b.kind)) {
        out.extend_from_slice(b.whole);
        if b.kind == *b"hvcC" || b.kind == *b"avcC" {
            out.extend(record.take().unwrap_or_default());
        }
    }
    out.extend(record.unwrap_or_default());
    out
}

fn parse_traf(
    data: &[u8],
    init: &Init,
    moof_len: usize,
    mdat_header: usize,
    mdat_len: usize,
    samples: &mut usize,
) -> Result<Traf> {
    let kids = children(data)?;
    let tfhd = child(&kids, b"tfhd").ok_or("track fragment without a header")?;
    let mut f = Fields::new(tfhd.payload);
    let flags = f.u32()? & 0x00FF_FFFF;
    let track = f.u32()?;
    let info = init.track(track).ok_or("fragment of a track the movie box doesn't have")?;
    let base_data_offset = if flags & TFHD_BASE_DATA_OFFSET != 0 { Some(f.u64()?) } else { None };
    let description = if flags & TFHD_DESCRIPTION != 0 { Some(f.u32()?) } else { None };
    let default_duration = if flags & TFHD_DURATION != 0 { Some(f.u32()?) } else { None };
    let default_size = if flags & TFHD_SIZE != 0 { Some(f.u32()?) } else { None };
    let default_flags = if flags & TFHD_FLAGS != 0 { Some(f.u32()?) } else { None };
    let trex = info.defaults.unwrap_or_default();
    let defaults = Defaults {
        duration: default_duration.or(trex.duration),
        size: default_size.or(trex.size),
        flags: default_flags.or(trex.flags),
    };
    // Run data is found only when it is addressed from the moof.
    let addressed = flags & TFHD_DEFAULT_BASE_IS_MOOF != 0 && base_data_offset.is_none();

    let mut tfdt = None;
    let mut tfdt_version = 1;
    let mut runs = Vec::new();
    let mut other = Vec::new();
    let mut next_data: Option<usize> = None;
    for c in &kids {
        match &c.kind {
            b"tfhd" => {}
            b"tfdt" => {
                let mut f = Fields::new(c.payload);
                tfdt_version = f.take::<4>()?[0];
                // FFmpeg writes a negative decode time as its 64-bit
                // two's complement.
                tfdt = Some(if tfdt_version == 1 { f.u64()? as i64 } else { i64::from(f.u32()?) });
            }
            b"trun" => {
                let run = parse_trun(c.payload, defaults, addressed, moof_len, mdat_header, mdat_len, next_data)?;
                *samples += run.samples.len();
                if *samples > MAX_FRAGMENT_SAMPLES {
                    return Err("too many samples in a fragment");
                }
                next_data = run.data.as_ref().map(|d| d.end);
                runs.push(run);
            }
            _ => other.push(c.whole.to_vec()),
        }
    }
    Ok(Traf {
        track,
        flags,
        base_data_offset,
        description,
        default_duration,
        default_size,
        default_flags,
        tfdt: tfdt.ok_or("track fragment without a decode time")?,
        tfdt_version,
        runs,
        other,
        defaults,
    })
}

fn parse_trun(
    data: &[u8],
    defaults: Defaults,
    addressed: bool,
    moof_len: usize,
    mdat_header: usize,
    mdat_len: usize,
    previous_end: Option<usize>,
) -> Result<Run> {
    let mut f = Fields::new(data);
    let head = f.u32()?;
    let version = (head >> 24) as u8;
    let flags = head & 0x00FF_FFFF;
    let count = f.u32()?;
    if count > MAX_RUN_SAMPLES {
        return Err("too many samples in a track run");
    }
    let data_offset = if flags & TRUN_DATA_OFFSET != 0 { Some(f.i32()?) } else { None };
    let first_sample_flags = if flags & TRUN_FIRST_FLAGS != 0 { Some(f.u32()?) } else { None };
    let per_sample = 4 * (flags & (TRUN_DURATION | TRUN_SIZE | TRUN_FLAGS | TRUN_CTO)).count_ones() as usize;
    if per_sample > 0 && (count as usize).checked_mul(per_sample).is_none_or(|n| n > f.remaining()) {
        return Err("track run lists more samples than it holds");
    }
    let mut samples = Vec::with_capacity(count as usize);
    let mut total: u64 = 0;
    for i in 0..count {
        let duration =
            if flags & TRUN_DURATION != 0 { f.u32()? } else { defaults.duration.ok_or("no sample duration")? };
        let size = if flags & TRUN_SIZE != 0 { f.u32()? } else { defaults.size.ok_or("no sample size")? };
        let mut sample_flags = match first_sample_flags {
            Some(first) if i == 0 => first,
            _ => defaults.flags.unwrap_or(0),
        };
        if flags & TRUN_FLAGS != 0 {
            sample_flags = f.u32()?;
        }
        let cto = match (flags & TRUN_CTO != 0, version) {
            (false, _) => 0,
            (true, 0) => i64::from(f.u32()?),
            (true, _) => i64::from(f.i32()?),
        };
        total += u64::from(size);
        samples.push(Sample { duration, size, flags: sample_flags, cto });
    }

    // Where the run's data is in the mdat payload.
    let start = match data_offset {
        Some(offset) if addressed => {
            usize::try_from(i64::from(offset) - (moof_len + mdat_header) as i64).ok().filter(|_| offset >= 0)
        }
        None if addressed => previous_end,
        _ => None,
    };
    let data = start.and_then(|s| {
        let end = s as u64 + total;
        (end <= mdat_len as u64).then_some(s..end as usize)
    });
    Ok(Run { version, flags, first_sample_flags, samples, data, edited: None })
}

impl Traf {
    /// Writes the `traf`, noting where each run's data offset goes and the
    /// run's place in the new mdat.
    fn write(&self, out: &mut Vec<u8>, places: &[usize], patches: &mut Vec<(usize, usize)>) {
        let traf = open_box(out, b"traf");
        let tfhd = open_box(out, b"tfhd");
        out.extend_from_slice(&self.flags.to_be_bytes());
        out.extend_from_slice(&self.track.to_be_bytes());
        for value in
            [self.description, self.default_duration, self.default_size, self.default_flags].into_iter().flatten()
        {
            out.extend_from_slice(&value.to_be_bytes());
        }
        close_box(out, tfhd);

        let tfdt = open_box(out, b"tfdt");
        if self.tfdt_version == 0 && (0..=i64::from(u32::MAX)).contains(&self.tfdt) {
            out.extend_from_slice(&[0; 4]);
            out.extend_from_slice(&(self.tfdt as u32).to_be_bytes());
        } else {
            out.extend_from_slice(&0x0100_0000u32.to_be_bytes());
            out.extend_from_slice(&(self.tfdt as u64).to_be_bytes());
        }
        close_box(out, tfdt);

        for (run, &place) in self.runs.iter().zip(places) {
            run.write(out, self.defaults, place, patches);
        }
        for other in &self.other {
            out.extend_from_slice(other);
        }
        close_box(out, traf);
    }
}

impl Run {
    fn write(&self, out: &mut Vec<u8>, defaults: Defaults, place: usize, patches: &mut Vec<(usize, usize)>) {
        // A field left out must read back as the sample's value: when it
        // wouldn't (a sample taken out, say), the run carries it per sample.
        let mut flags = self.flags | TRUN_DATA_OFFSET;
        let rest_flags = defaults.flags.unwrap_or(0);
        if flags & TRUN_DURATION == 0 && self.samples.iter().any(|s| Some(s.duration) != defaults.duration) {
            flags |= TRUN_DURATION;
        }
        if flags & TRUN_SIZE == 0 && self.samples.iter().any(|s| Some(s.size) != defaults.size) {
            flags |= TRUN_SIZE;
        }
        if flags & TRUN_FLAGS == 0 {
            if self.samples.iter().skip(1).any(|s| s.flags != rest_flags) {
                flags = (flags | TRUN_FLAGS) & !TRUN_FIRST_FLAGS;
            } else if self.samples.first().is_some_and(|s| s.flags != rest_flags) {
                flags |= TRUN_FIRST_FLAGS;
            }
        }
        if flags & TRUN_CTO == 0 && self.samples.iter().any(|s| s.cto != 0) {
            flags |= TRUN_CTO;
        }
        let version = if self.samples.iter().any(|s| s.cto < 0) { self.version.max(1) } else { self.version };

        let trun = open_box(out, b"trun");
        out.extend_from_slice(&((u32::from(version) << 24) | flags).to_be_bytes());
        out.extend_from_slice(&(self.samples.len() as u32).to_be_bytes());
        patches.push((out.len(), place));
        out.extend_from_slice(&[0; 4]);
        if flags & TRUN_FIRST_FLAGS != 0 {
            let first = self.samples.first().map_or(self.first_sample_flags.unwrap_or(rest_flags), |s| s.flags);
            out.extend_from_slice(&first.to_be_bytes());
        }
        for s in &self.samples {
            if flags & TRUN_DURATION != 0 {
                out.extend_from_slice(&s.duration.to_be_bytes());
            }
            if flags & TRUN_SIZE != 0 {
                out.extend_from_slice(&s.size.to_be_bytes());
            }
            if flags & TRUN_FLAGS != 0 {
                out.extend_from_slice(&s.flags.to_be_bytes());
            }
            if flags & TRUN_CTO != 0 {
                let cto = if version == 0 { s.cto as u32 } else { s.cto as i32 as u32 };
                out.extend_from_slice(&cto.to_be_bytes());
            }
        }
        close_box(out, trun);
    }
}

/// Starts a box, its size filled in by [`close_box`].
fn open_box(out: &mut Vec<u8>, kind: &[u8; 4]) -> usize {
    let at = out.len();
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(kind);
    at
}

fn close_box(out: &mut [u8], at: usize) {
    let size = (out.len() - at) as u32;
    out[at..at + 4].copy_from_slice(&size.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use tokio::io::AsyncWriteExt;

    use super::*;

    /// The movie flags fMP4 jobs give FFmpeg.
    const MOVFLAGS: &str = "+frag_keyframe+empty_moov+default_base_moof+delay_moov+frag_discont+skip_trailer";

    fn has_ffmpeg() -> bool {
        let ok = |tool: &str| Command::new(tool).arg("-version").output().is_ok_and(|o| o.status.success());
        if !(ok("ffmpeg") && ok("ffprobe")) {
            eprintln!("ffmpeg not found: skipped");
            return false;
        }
        let encoders = Command::new("ffmpeg").args(["-hide_banner", "-encoders"]).output().unwrap();
        if !String::from_utf8_lossy(&encoders.stdout).contains("libx264") {
            eprintln!("FFmpeg without libx264: skipped");
            return false;
        }
        true
    }

    /// 6 s of x264 with B-frames, keyframes every 2 s as jobs force them,
    /// and a tone in `audio`, written by FFmpeg as jobs have it write fMP4.
    fn stream(audio: &str) -> Vec<u8> {
        let out = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25"])
            .args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000", "-t", "6"])
            .args(["-c:v", "libx264", "-preset", "veryfast", "-bf", "2", "-sc_threshold", "0"])
            .args(["-force_key_frames", "expr:gte(t,n_forced*2)", "-c:a", audio, "-avoid_negative_ts", "disabled"])
            .args(["-f", "mp4", "-movflags", MOVFLAGS, "-write_tmcd", "0", "-flush_packets", "1", "pipe:1"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        out.stdout
    }

    async fn read_all(mut reader: BoxReader<impl AsyncRead + Unpin>) -> io::Result<Vec<TopBox>> {
        let mut boxes = Vec::new();
        while let Some(b) = reader.next().await? {
            boxes.push(b);
        }
        Ok(boxes)
    }

    async fn split(data: &[u8]) -> (Init, Vec<Fragment>) {
        let boxes = read_all(BoxReader::new(data)).await.unwrap();
        let kinds: Vec<_> = boxes.iter().map(|b| b.kind).collect();
        assert_eq!(kinds[..2], [*b"ftyp", *b"moov"]);
        let init = Init::parse(&boxes[0].bytes, &boxes[1].bytes).unwrap();
        let fragments = boxes[2..]
            .chunks(2)
            .map(|pair| {
                assert_eq!((pair[0].kind, pair[1].kind), (*b"moof", *b"mdat"));
                Fragment::parse(pair[0].bytes.clone(), pair[1].bytes.clone(), &init).unwrap()
            })
            .collect();
        (init, fragments)
    }

    fn traf<'a>(fragment: &'a Fragment, init: &Init, handler: &[u8; 4]) -> &'a Traf {
        let id = init.tracks.iter().find(|t| t.handler == *handler).unwrap().id;
        fragment.trafs.iter().find(|t| t.track == id).unwrap()
    }

    #[tokio::test]
    async fn fragments_are_placed_at_their_keyframes() {
        if !has_ffmpeg() {
            return;
        }
        for (audio, format) in [("aac", *b"mp4a"), ("ac3", *b"ac-3")] {
            let (init, fragments) = split(&stream(audio)).await;
            let formats: Vec<_> = init.tracks.iter().map(|t| (t.handler, t.format)).collect();
            assert_eq!(formats, [(*b"vide", *b"avc1"), (*b"soun", format)]);
            assert_eq!(init.tracks[1].timescale, 48000);
            let video = init.track(init.video.unwrap()).unwrap();
            assert_eq!(video.nal_length_size, Some(4));
            // The B-frame delay, which a keyframe's composition offset
            // carries and the edit list takes back.
            assert!(video.shift > 0, "{audio}: no B-frame delay");

            let starts: Vec<_> = fragments.iter().filter_map(|f| f.presentation_start_ms(&init)).collect();
            assert_eq!(starts, [0, 2000, 4000], "{audio}");
            for f in &fragments {
                let first = traf(f, &init, b"vide").runs[0].samples[0];
                assert_eq!(first.cto, video.shift, "{audio}: a keyframe shown later than decoded");
                assert_eq!(first.flags & 0x0101_0000, 0, "{audio}: a fragment starting on a non-sync sample");
            }
        }
    }

    #[tokio::test]
    async fn boxes_read_the_same_however_the_stream_arrives() {
        if !has_ffmpeg() {
            return;
        }
        let data = stream("aac");
        let len = data.len();
        let whole = read_all(BoxReader::new(data.as_slice())).await.unwrap();
        assert_eq!(whole.iter().map(|b| b.bytes.len()).sum::<usize>(), len);
        let (mut tx, rx) = tokio::io::duplex(64);
        let writer = tokio::spawn(async move {
            for chunk in data.chunks(7) {
                tx.write_all(chunk).await.unwrap();
            }
        });
        let chunked = read_all(BoxReader::new(rx)).await.unwrap();
        writer.await.unwrap();
        assert_eq!(chunked, whole);
    }

    fn top(kind: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = ((payload.len() + 8) as u32).to_be_bytes().to_vec();
        out.extend_from_slice(kind);
        out.extend_from_slice(payload);
        out
    }

    #[tokio::test]
    async fn the_reader_bounds_and_skips_boxes() {
        // An mdat with a 64-bit size, after boxes that are skipped.
        let mut data = top(b"free", &[7; 100]);
        data.extend(top(b"sidx", &[1; 20]));
        let mut mdat = 1u32.to_be_bytes().to_vec();
        mdat.extend_from_slice(b"mdat");
        mdat.extend_from_slice(&(16u64 + 5).to_be_bytes());
        mdat.extend_from_slice(b"hello");
        data.extend_from_slice(&mdat);
        data.extend(top(b"moof", &[2; 8]));
        let boxes = read_all(BoxReader::new(data.as_slice())).await.unwrap();
        assert_eq!(
            boxes,
            [TopBox { kind: *b"mdat", bytes: mdat }, TopBox { kind: *b"moof", bytes: top(b"moof", &[2; 8]) }]
        );

        // Larger than a file type box may be; a box too large to skip.
        let mut big = (MAX_FTYP as u32 + 1).to_be_bytes().to_vec();
        big.extend_from_slice(b"ftyp");
        let err = read_all(BoxReader::new(big.as_slice())).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        let mut big = (MAX_SKIPPED as u32 + 1).to_be_bytes().to_vec();
        big.extend_from_slice(b"free");
        assert!(read_all(BoxReader::new(big.as_slice())).await.is_err());

        // Cut off in a box, in a header, and in a skipped box.
        let moof = top(b"moof", &[3; 50]);
        let free = top(b"free", &[0; 40]);
        for cut in [&moof[..30], &moof[..5], &free[..20]] {
            let err = read_all(BoxReader::new(cut)).await.unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::UnexpectedEof);
        }
        // A box running to the end, and one smaller than its header.
        assert!(read_all(BoxReader::new(&[0, 0, 0, 0, b'm', b'd', b'a', b't'][..])).await.is_err());
        assert!(read_all(BoxReader::new(&[0, 0, 0, 4, b'm', b'd', b'a', b't'][..])).await.is_err());
    }

    #[tokio::test]
    async fn unchanged_fragments_are_written_back_as_they_were() {
        if !has_ffmpeg() {
            return;
        }
        for audio in ["aac", "ac3"] {
            let (_, fragments) = split(&stream(audio)).await;
            assert!(fragments.len() >= 3);
            for f in &fragments {
                assert!(f.rewritable());
                assert_eq!(f.to_bytes(), f.bytes, "{audio}: fragment {}", f.sequence);
            }
        }
    }

    #[tokio::test]
    async fn audio_before_zero_is_trimmed() {
        if !has_ffmpeg() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        // AAC primes with a whole frame (1024); AC-3's first frame (1536)
        // starts 256 before 0, so the first one kept starts at 1280.
        for (audio, before, after) in [("aac", -1024, 0), ("ac3", -256, 1280)] {
            let (init, fragments) = split(&stream(audio)).await;
            let mut file = init.bytes.clone();
            for (i, f) in fragments.iter().enumerate() {
                let mut trimmed = f.clone();
                if i > 0 {
                    assert!(!f.needs_trim(&init));
                    assert!(!trimmed.trim_before_zero(&init));
                    file.extend(trimmed.to_bytes());
                    continue;
                }
                assert!(f.needs_trim(&init));
                assert_eq!(traf(f, &init, b"soun").tfdt, before, "{audio}");
                assert!(trimmed.trim_before_zero(&init));
                let bytes = trimmed.to_bytes();
                file.extend_from_slice(&bytes);

                // Read back: one audio sample fewer, the mdat smaller by
                // its size, the video untouched.
                let boxes = read_all(BoxReader::new(bytes.as_slice())).await.unwrap();
                let again = Fragment::parse(boxes[0].bytes.clone(), boxes[1].bytes.clone(), &init).unwrap();
                let (old, new) = (traf(f, &init, b"soun"), traf(&again, &init, b"soun"));
                assert_eq!(new.tfdt, after, "{audio}");
                let old_samples: Vec<_> = old.runs.iter().flat_map(|r| &r.samples).collect();
                let new_samples: Vec<_> = new.runs.iter().flat_map(|r| &r.samples).collect();
                assert_eq!(new_samples[..], old_samples[1..]);
                let mdat_len = |f: &Fragment| f.bytes.len() - f.mdat_at;
                assert_eq!(mdat_len(&again), mdat_len(f) - old_samples[0].size as usize);

                let video = |f: &Fragment| {
                    let t = traf(f, &init, b"vide");
                    let data = t.runs[0].data.clone().unwrap();
                    (t.tfdt, t.runs[0].samples.clone(), f.bytes[f.mdat_at + f.mdat_header..][data].to_vec())
                };
                assert_eq!(video(&again), video(f), "{audio}");
            }

            let path = dir.path().join(format!("{audio}.mp4"));
            std::fs::write(&path, &file).unwrap();
            let decode = Command::new("ffmpeg")
                .args(["-v", "error", "-i"])
                .arg(&path)
                .args(["-f", "null", "-"])
                .output()
                .unwrap();
            assert!(decode.status.success() && decode.stderr.is_empty(), "{}", String::from_utf8_lossy(&decode.stderr));
            let packets = Command::new("ffprobe")
                .args(["-v", "error", "-select_streams", "a:0", "-show_entries", "packet=pts", "-of", "csv=p=0"])
                .arg(&path)
                .output()
                .unwrap();
            assert!(packets.stderr.is_empty(), "{}", String::from_utf8_lossy(&packets.stderr));
            let first = String::from_utf8_lossy(&packets.stdout).lines().next().unwrap().trim().to_owned();
            assert_eq!(first, after.to_string(), "{audio}");
        }
    }

    // ---- a fragment made by hand

    fn full(kind: &[u8; 4], flags: u32, payload: &[u8]) -> Vec<u8> {
        top(kind, &[&flags.to_be_bytes()[..], payload].concat())
    }

    fn u32s(values: &[u32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_be_bytes()).collect()
    }

    fn handmade_init() -> Init {
        let entry = top(b"mp4a", &[0; 28]);
        let stsd = full(b"stsd", 0, &[&u32s(&[1])[..], &entry].concat());
        let minf = top(b"minf", &top(b"stbl", &stsd));
        let hdlr = full(b"hdlr", 0, &[&u32s(&[0])[..], b"soun", &[0; 13]].concat());
        let mdhd = full(b"mdhd", 0, &u32s(&[0, 0, 48000, 0, 0]));
        let mdia = top(b"mdia", &[mdhd, hdlr, minf].concat());
        let tkhd = full(b"tkhd", 7, &u32s(&[0, 0, 1, 0, 0]));
        let trak = top(b"trak", &[tkhd, mdia].concat());
        let mvhd = full(b"mvhd", 0, &u32s(&[0, 0, 1000, 0]));
        // Samples of 1024 ticks and 10 bytes by default.
        let trex = full(b"trex", 0, &u32s(&[1, 1, 1024, 10, 0]));
        let moov = top(b"moov", &[mvhd, trak, top(b"mvex", &trex)].concat());
        Init::parse(&top(b"ftyp", b"isom\0\0\0\0"), &moov).unwrap()
    }

    /// Two samples of 10 bytes, at `data_offset`.
    fn handmade_fragment(tfhd_flags: u32, data_offset: u32) -> (Vec<u8>, Vec<u8>) {
        let mfhd = full(b"mfhd", 0, &u32s(&[1]));
        let tfhd = full(b"tfhd", tfhd_flags, &u32s(&[1]));
        let tfdt = full(b"tfdt", 0x0100_0000, &u32s(&[0, 0]));
        let trun = full(b"trun", TRUN_DATA_OFFSET, &u32s(&[2, data_offset]));
        let moof = top(b"moof", &[mfhd, top(b"traf", &[tfhd, tfdt, trun].concat())].concat());
        (moof, top(b"mdat", &[1; 20]))
    }

    #[test]
    fn fragments_not_addressed_from_the_moof_are_not_rewritten() {
        let init = handmade_init();
        assert_eq!(init.tracks[0].handler, *b"soun");
        assert_eq!(init.video, None);
        let offset = handmade_fragment(0, 0).0.len() as u32 + 8;
        let (moof, mdat) = handmade_fragment(TFHD_DEFAULT_BASE_IS_MOOF, offset);
        let f = Fragment::parse(moof, mdat, &init).unwrap();
        assert!(f.rewritable());
        assert_eq!(f.trafs[0].runs[0].data, Some(0..20));
        assert_eq!(f.to_bytes(), f.bytes);
        assert_eq!(f.presentation_start_ms(&init), None);

        // Addressed from the start of the file, as without default-base-is-moof.
        let (moof, mdat) = handmade_fragment(0, offset);
        let f = Fragment::parse(moof, mdat, &init).unwrap();
        assert!(!f.rewritable());
        assert_eq!(f.to_bytes(), f.bytes);
        // Data past the end of the mdat.
        let (moof, mdat) = handmade_fragment(TFHD_DEFAULT_BASE_IS_MOOF, offset + 1);
        assert!(!Fragment::parse(moof, mdat, &init).unwrap().rewritable());
        // A fragment of a track the movie box doesn't have.
        let (moof, mdat) = handmade_fragment(TFHD_DEFAULT_BASE_IS_MOOF, offset);
        let other = Init { tracks: Vec::new(), ..init };
        assert!(Fragment::parse(moof, mdat, &other).is_err());
    }

    // ---- Dolby Vision, by hand

    /// A track: `entry` is the whole sample entry, `trex` its defaults.
    fn handmade_trak(id: u32, handler: &[u8; 4], timescale: u32, entry: &[u8]) -> Vec<u8> {
        let stsd = full(b"stsd", 0, &[&u32s(&[1])[..], entry].concat());
        let minf = top(b"minf", &top(b"stbl", &stsd));
        let hdlr = full(b"hdlr", 0, &[&u32s(&[0])[..], handler, &[0; 13]].concat());
        let mdhd = full(b"mdhd", 0, &u32s(&[0, 0, timescale, 0, 0]));
        let mdia = top(b"mdia", &[mdhd, hdlr, minf].concat());
        let tkhd = full(b"tkhd", 7, &u32s(&[0, 0, id, 0, 0]));
        top(b"trak", &[tkhd, mdia].concat())
    }

    /// HEVC (track 1, NAL unit lengths of 4 bytes) with `dovi` boxes after
    /// its `hvcC` and a `pasp`, and audio (track 2) of 10-byte samples.
    fn handmade_av_init(dovi: &[Vec<u8>]) -> Vec<u8> {
        let mut hvcc = vec![1; 23];
        hvcc[21] = 0xF3;
        let mut boxes = top(b"hvcC", &hvcc);
        dovi.iter().for_each(|b| boxes.extend(b));
        boxes.extend(top(b"pasp", &u32s(&[1, 1])));
        let video = top(b"hvc1", &[&[0; 78][..], &boxes].concat());
        let audio = top(b"mp4a", &[0; 28]);
        let mvhd = full(b"mvhd", 0, &u32s(&[0, 0, 1000, 0]));
        let trex = [full(b"trex", 0, &u32s(&[1, 1, 3750, 0, 0])), full(b"trex", 0, &u32s(&[2, 1, 1024, 10, 0]))];
        let traks = [handmade_trak(1, b"vide", 90000, &video), handmade_trak(2, b"soun", 48000, &audio)];
        let moov = top(b"moov", &[mvhd, traks.concat(), top(b"mvex", &trex.concat())].concat());
        [top(b"ftyp", b"isom\0\0\x02\0isomiso6"), moov].concat()
    }

    /// Whether every box down to the sample entries' own fills its parent
    /// exactly.
    fn sizes_add_up(data: &[u8]) -> bool {
        let Ok(boxes) = children(data) else { return false };
        boxes.iter().all(|b| match &b.kind {
            b"moov" | b"trak" | b"mdia" | b"minf" | b"stbl" | b"mvex" => sizes_add_up(b.payload),
            b"stsd" => sizes_add_up(&b.payload[8..]),
            b"hvc1" | b"dvh1" => sizes_add_up(&b.payload[78..]),
            _ => true,
        })
    }

    /// The boxes of the video sample entry, by type.
    fn entry_kinds(init: &[u8]) -> Vec<[u8; 4]> {
        let mut data = init;
        let mut skip = 0;
        for (kind, then) in
            [(b"moov", 0), (b"trak", 0), (b"mdia", 0), (b"minf", 0), (b"stbl", 0), (b"stsd", 8), (b"hvc1", 78)]
        {
            let boxes = children(&data[skip..]).unwrap();
            data = boxes.iter().find(|b| b.kind == *kind).unwrap().payload;
            skip = then;
        }
        children(&data[skip..]).unwrap().iter().map(|b| b.kind).collect()
    }

    #[test]
    fn the_init_gets_the_dolby_vision_record_asked_for() {
        let p81 = DoviConfig { profile: 8, level: 6, rpu: true, el: false, bl: true, compat: 1 };
        let p7 = DoviConfig { profile: 7, level: 6, rpu: true, el: true, bl: true, compat: 6 };
        let plain = handmade_av_init(&[]);
        let audio_trak = |init: &[u8]| {
            let boxes = children(init).unwrap();
            let moov = children(boxes[1].payload).unwrap();
            moov.iter().filter(|b| b.kind == *b"trak").nth(1).unwrap().whole.to_vec()
        };
        assert_eq!(&plain[24 + 4..24 + 8], b"moov");
        let parsed = Init::from_bytes(&plain).unwrap();
        assert_eq!((parsed.tracks[0].nal_length_size, parsed.tracks[0].dovi), (Some(4), None));

        // Put in, after the hvcC, with the brand.
        let inserted = set_dovi(&plain, Some(p81)).unwrap();
        assert_eq!(inserted.len(), plain.len() + 32 + 4);
        assert!(sizes_add_up(&inserted));
        assert_eq!(entry_kinds(&inserted), [*b"hvcC", *b"dvvC", *b"pasp"]);
        let parsed = Init::from_bytes(&inserted).unwrap();
        assert_eq!(parsed.tracks[0].dovi, Some(p81));
        assert_eq!(parsed.tracks[1], Init::from_bytes(&plain).unwrap().tracks[1]);
        assert_eq!(audio_trak(&inserted), audio_trak(&plain), "the sound's track untouched");
        let ftyp = |init: &[u8]| children(init).unwrap()[0].payload.to_vec();
        assert_eq!(ftyp(&inserted), b"isom\0\0\x02\0isomiso6dby1");
        // The record: version 1.0, profile 8 level 6, RPU and base layer,
        // compatibility 1, the rest reserved.
        let record = inserted.windows(4).position(|w| w == b"dvvC").unwrap();
        assert_eq!(
            inserted[record - 4..record + 12],
            [0, 0, 0, 32, b'd', b'v', b'v', b'C', 1, 0, 0x10, 0x35, 0x10, 0, 0, 0]
        );
        // The same again: the same bytes, the brand not twice.
        assert_eq!(set_dovi(&inserted, Some(p81)).unwrap(), inserted);

        // Replaced: FFmpeg's profile 7 record becomes 8.1's.
        let with_p7 = handmade_av_init(&[p7.to_box()]);
        assert_eq!(Init::from_bytes(&with_p7).unwrap().tracks[0].dovi, Some(p7));
        let replaced = set_dovi(&with_p7, Some(p81)).unwrap();
        assert_eq!(replaced, inserted);
        assert_eq!(entry_kinds(&replaced), [*b"hvcC", *b"dvvC", *b"pasp"]);

        // Taken out, with any other record, and the brand (which FFmpeg
        // gives a stream with a record).
        let with_both = handmade_av_init(&[p7.to_box(), p81.to_box()]);
        let removed = set_dovi(&with_both, None).unwrap();
        assert_eq!(removed, plain);
        assert_eq!(set_dovi(&inserted, None).unwrap(), plain);
        assert_eq!(ftyp(&removed), b"isom\0\0\x02\0isomiso6");
        assert!(sizes_add_up(&removed));
        assert_eq!(Init::from_bytes(&removed).unwrap().tracks[0].dovi, None);

        // Profile 5's record is a dvcC.
        let p5 = DoviConfig { profile: 5, level: 9, rpu: true, el: false, bl: true, compat: 0 };
        let made = set_dovi(&plain, Some(p5)).unwrap();
        assert_eq!(entry_kinds(&made), [*b"hvcC", *b"dvcC", *b"pasp"]);
        assert_eq!(Init::from_bytes(&made).unwrap().tracks[0].dovi, Some(p5));
        assert!(set_dovi(&plain[..24], Some(p5)).is_err(), "no movie box");
    }

    /// Two video samples (AUD, slice, enhancement layer, profile 7 RPU)
    /// and two 10-byte audio samples after them, at offsets from the moof.
    fn handmade_av_fragment() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let unit = |nal: &[u8]| [&(nal.len() as u32).to_be_bytes()[..], nal].concat();
        let rpu = crate::dovi::tests::p7_rpu(false);
        let sample = |slice: u8| {
            [unit(&[0x46, 0x01, 0x50]), unit(&[0x26, 0x01, slice, 0x40]), unit(&[0x7E, 0x01, 0x22]), unit(&rpu)]
                .concat()
        };
        let video: Vec<u8> = [sample(0xA1), sample(0xA2)].concat();
        let audio = [3u8; 20];
        let moof = |offsets: [u32; 2]| {
            let mfhd = full(b"mfhd", 0, &u32s(&[5]));
            let size = sample(0).len() as u32;
            let vtraf = [
                full(b"tfhd", TFHD_DEFAULT_BASE_IS_MOOF, &u32s(&[1])),
                full(b"tfdt", 0x0100_0000, &u32s(&[0, 90000])),
                full(b"trun", TRUN_DATA_OFFSET | TRUN_SIZE, &u32s(&[2, offsets[0], size, size])),
            ];
            let atraf = [
                full(b"tfhd", TFHD_DEFAULT_BASE_IS_MOOF, &u32s(&[2])),
                full(b"tfdt", 0x0100_0000, &u32s(&[0, 48000])),
                full(b"trun", TRUN_DATA_OFFSET, &u32s(&[2, offsets[1]])),
            ];
            top(b"moof", &[mfhd, top(b"traf", &vtraf.concat()), top(b"traf", &atraf.concat())].concat())
        };
        let base = moof([0, 0]).len() as u32 + 8;
        let mdat = top(b"mdat", &[&video[..], &audio].concat());
        (moof([base, base + video.len() as u32]), mdat, rpu)
    }

    #[test]
    fn video_samples_are_edited_and_the_fragment_written_anew() {
        let init = Init::from_bytes(&handmade_av_init(&[])).unwrap();
        let (moof, mdat, rpu) = handmade_av_fragment();
        let mut f = Fragment::parse(moof, mdat, &init).unwrap();
        assert!(f.rewritable());
        let old_sizes: Vec<usize> = f.samples(1).iter().map(|s| s.len()).collect();
        assert_eq!(f.edit_video(&init, Edits::default()).unwrap(), None, "nothing asked");
        let edits = Edits { to_81: true, strip_hdr10plus: true };
        let edited = f.edit_video(&init, edits).unwrap().expect("changed");
        assert_eq!((edited.rpus_converted, edited.el_dropped, edited.rpus_dropped), (2, 2, 0));

        let bytes = f.to_bytes();
        let moof_len = u32::from_be_bytes(bytes[..4].try_into().unwrap()) as usize;
        let again = Fragment::parse(bytes[..moof_len].to_vec(), bytes[moof_len..].to_vec(), &init).unwrap();
        assert!(again.rewritable());
        let crate::dovi::NalEdit::Replace(p81) = crate::dovi::rpu_to_p81(&rpu).unwrap() else { panic!() };
        let unit = |nal: &[u8]| [&(nal.len() as u32).to_be_bytes()[..], nal].concat();
        let samples = again.samples(1);
        for (sample, slice) in samples.iter().zip([0xA1, 0xA2]) {
            let expected = [unit(&[0x46, 0x01, 0x50]), unit(&[0x26, 0x01, slice, 0x40]), unit(&p81)].concat();
            assert_eq!(*sample, expected.as_slice(), "the enhancement layer out, the RPU 8.1");
        }
        // The trun says the new sizes; the audio after the video, moved up.
        let video = &again.trafs[0].runs[0];
        let new_sizes: Vec<usize> = video.samples.iter().map(|s| s.size as usize).collect();
        assert_eq!(new_sizes, samples.iter().map(|s| s.len()).collect::<Vec<_>>());
        assert!(new_sizes.iter().zip(&old_sizes).all(|(new, old)| new < old));
        let video_len: usize = new_sizes.iter().sum();
        assert_eq!(video.data, Some(0..video_len));
        assert_eq!(again.trafs[1].runs[0].data, Some(video_len..video_len + 20));
        assert_eq!(again.samples(2), [[3u8; 10], [3u8; 10]]);
        let mdat_size = u32::from_be_bytes(bytes[moof_len..moof_len + 4].try_into().unwrap()) as usize;
        assert_eq!((mdat_size, bytes.len()), (8 + video_len + 20, moof_len + 8 + video_len + 20));
        assert_eq!((again.trafs[0].tfdt, again.trafs[1].tfdt), (90000, 48000));

        // Done once, nothing more to do.
        let mut twice = again.clone();
        assert_eq!(twice.edit_video(&init, edits).unwrap(), None);
        assert_eq!(twice.to_bytes(), again.bytes);

        // A unit running past its sample: an error, the fragment as it was.
        let (moof, mut mdat, _) = handmade_av_fragment();
        mdat[8 + 3] = 0xFF;
        let mut broken = Fragment::parse(moof, mdat, &init).unwrap();
        assert!(broken.edit_video(&init, edits).is_err());
        assert_eq!(broken.to_bytes(), broken.bytes);
    }
}

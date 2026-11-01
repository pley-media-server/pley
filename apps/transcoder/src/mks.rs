// SPDX-License-Identifier: AGPL-3.0-only
//! The Matroska files of one picture subtitle track (`.mks`) that players
//! draw next to a stream (`POST /v1/subtitles/mks`): written here from the
//! blocks read through a file's Cues, and, while FFmpeg writes one, how much
//! of it can be read already.
//!
//! What is written is what `ffmpeg -c copy -f matroska` writes of the track,
//! as far as players read it: the track's codec and private data, every
//! block with its time (in milliseconds, the file's start taken off) and its
//! duration when it has one, Cues and a seek head.

use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

const EBML: u32 = 0x1A45_DFA3;
const SEGMENT: u32 = 0x1853_8067;
const SEEK_HEAD: u32 = 0x114D_9B74;
const INFO: u32 = 0x1549_A966;
const TRACKS: u32 = 0x1654_AE6B;
const CLUSTER: u32 = 0x1F43_B675;
const CUES: u32 = 0x1C53_BB6B;

/// A cluster's blocks stay within this of its timestamp (block times are
/// 16 bits from it).
const CLUSTER_SPAN_MS: i64 = 30_000;

/// A block of the track: when it shows (ms), how long (0: until the next
/// one), and its data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub time_ms: i64,
    pub duration_ms: u64,
    pub data: Vec<u8>,
}

/// Writes the track (`codec_id`, its `private` data, its `blocks` in order
/// of time) as a Matroska file at `path`. Blocking.
pub fn write(path: &Path, codec_id: &str, private: &[u8], blocks: &[Block]) -> std::io::Result<()> {
    let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
    out.write_all(&element(EBML, &ebml_header()))?;
    out.write_all(&id_bytes(SEGMENT))?;
    // The segment's size, once known.
    let segment_size_at = out.stream_position()?;
    out.write_all(&[0x01, 0, 0, 0, 0, 0, 0, 0])?;
    let segment = out.stream_position()?;

    // A seek head of fixed size first: the Cues' place is told once known.
    let seek = |id: u32, position: u64| {
        element(0x4DBB, &[element(0x53AB, &id_bytes(id)), uint8_element(0x53AC, position)].concat())
    };
    let end_ms = blocks.iter().map(|b| b.time_ms.saturating_add(b.duration_ms as i64)).max().unwrap_or(0);
    let info = element(
        INFO,
        &[
            uint_element(0x2A_D7B1, 1_000_000),
            element(0x4D80, b"Pley"),
            element(0x5741, b"Pley"),
            element(0x4489, &(end_ms.max(0) as f64).to_be_bytes()),
        ]
        .concat(),
    );
    let mut entry = vec![
        uint_element(0xD7, 1),
        uint_element(0x73C5, 1),
        uint_element(0x83, 0x11),
        uint_element(0x9C, 0),
        element(0x86, codec_id.as_bytes()),
    ];
    if !private.is_empty() {
        entry.push(element(0x63A2, private));
    }
    let tracks = element(TRACKS, &element(0xAE, &entry.concat()));
    let head_len = element(SEEK_HEAD, &[seek(INFO, 0), seek(TRACKS, 0), seek(CUES, 0)].concat()).len() as u64;
    let info_at = head_len;
    let tracks_at = info_at + info.len() as u64;
    let clusters_at = tracks_at + tracks.len() as u64;
    let seek_head_at = out.stream_position()?;
    out.write_all(&element(SEEK_HEAD, &[seek(INFO, info_at), seek(TRACKS, tracks_at), seek(CUES, 0)].concat()))?;
    out.write_all(&info)?;
    out.write_all(&tracks)?;

    let mut cues = Vec::new();
    let mut at = clusters_at;
    let mut start = 0;
    while start < blocks.len() {
        let timestamp = blocks[start].time_ms.max(0);
        let end = blocks[start..]
            .iter()
            .position(|b| b.time_ms - timestamp > CLUSTER_SPAN_MS || b.time_ms < timestamp)
            .map_or(blocks.len(), |n| start + n);
        let mut body = uint_element(0xE7, timestamp as u64);
        for block in &blocks[start..end] {
            body.extend(block_element(block, (block.time_ms - timestamp) as i16));
        }
        let cluster = element(CLUSTER, &body);
        cues.extend(element(
            0xBB,
            &[
                uint_element(0xB3, timestamp as u64),
                element(0xB7, &[uint_element(0xF7, 1), uint_element(0xF1, at)].concat()),
            ]
            .concat(),
        ));
        out.write_all(&cluster)?;
        at += cluster.len() as u64;
        start = end;
    }
    let cues_at = at;
    out.write_all(&element(CUES, &cues))?;
    let end = out.stream_position()?;

    out.seek(SeekFrom::Start(segment_size_at))?;
    out.write_all(&size8(end - segment))?;
    out.seek(SeekFrom::Start(seek_head_at))?;
    out.write_all(&element(SEEK_HEAD, &[seek(INFO, info_at), seek(TRACKS, tracks_at), seek(CUES, cues_at)].concat()))?;
    out.into_inner().map_err(|e| e.into_error())?.sync_all()
}

/// A Matroska file of the track with no block yet: what players are given
/// before the first of a file FFmpeg writes is there.
pub fn header_only(codec_id: &str) -> Vec<u8> {
    let entry = [
        uint_element(0xD7, 1),
        uint_element(0x73C5, 1),
        uint_element(0x83, 0x11),
        uint_element(0x9C, 0),
        element(0x86, codec_id.as_bytes()),
    ]
    .concat();
    let segment =
        [element(INFO, &uint_element(0x2A_D7B1, 1_000_000)), element(TRACKS, &element(0xAE, &entry))].concat();
    [element(EBML, &ebml_header()), element(SEGMENT, &segment)].concat()
}

/// How much of a Matroska file being written (FFmpeg's, its segment's size
/// not known yet) can be read already: up to the end of its last whole
/// element after the segment's start (a cluster, as a rule), once its
/// tracks are there. `None` before that.
pub fn readable(bytes: &[u8]) -> Option<usize> {
    let (id, size, at) = header(bytes, 0)?;
    if id != EBML {
        return None;
    }
    let mut at = at.checked_add(usize::try_from(size?).ok()?)?;
    let (id, _, children) = header(bytes, at)?;
    if id != SEGMENT {
        return None;
    }
    at = children;
    let mut tracks = false;
    let mut end = None;
    while let Some((id, Some(size), data)) = header(bytes, at) {
        let next = data.checked_add(usize::try_from(size).ok()?)?;
        if next > bytes.len() {
            break;
        }
        tracks |= id == TRACKS;
        at = next;
        end = Some(next);
    }
    end.filter(|_| tracks)
}

/// The element at `at`: its id, its size (`None`: unknown), and where its
/// data starts; `None` when it isn't all there.
fn header(bytes: &[u8], at: usize) -> Option<(u32, Option<u64>, usize)> {
    let first = *bytes.get(at)?;
    let id_len = first.leading_zeros() as usize + 1;
    if id_len > 4 {
        return None;
    }
    let id = bytes.get(at..at + id_len)?.iter().fold(0u32, |v, b| (v << 8) | u32::from(*b));
    let at = at + id_len;
    let first = *bytes.get(at)?;
    let len = first.leading_zeros() as usize + 1;
    if len > 8 {
        return None;
    }
    let raw = bytes.get(at..at + len)?;
    let marker = if len == 8 { 0 } else { 0xFFu8 >> len };
    let mut size = u64::from(raw[0] & marker);
    let mut unknown = raw[0] & marker == marker;
    for b in &raw[1..] {
        size = (size << 8) | u64::from(*b);
        unknown &= *b == 0xFF;
    }
    Some((id, (!unknown).then_some(size), at + len))
}

fn ebml_header() -> Vec<u8> {
    [
        uint_element(0x4286, 1),
        uint_element(0x42F7, 1),
        uint_element(0x42F2, 4),
        uint_element(0x42F3, 8),
        element(0x4282, b"matroska"),
        uint_element(0x4287, 4),
        uint_element(0x4285, 2),
    ]
    .concat()
}

/// A BlockGroup with its duration, or a SimpleBlock without one.
fn block_element(block: &Block, relative: i16) -> Vec<u8> {
    let mut data = vec![0x81];
    data.extend_from_slice(&relative.to_be_bytes());
    if block.duration_ms > 0 {
        data.push(0);
        data.extend_from_slice(&block.data);
        element(0xA0, &[element(0xA1, &data), uint_element(0x9B, block.duration_ms)].concat())
    } else {
        data.push(0x80);
        data.extend_from_slice(&block.data);
        element(0xA3, &data)
    }
}

fn id_bytes(id: u32) -> Vec<u8> {
    let bytes = id.to_be_bytes();
    let skip = bytes.iter().take_while(|b| **b == 0).count();
    bytes[skip..].to_vec()
}

fn element(id: u32, data: &[u8]) -> Vec<u8> {
    [id_bytes(id), size(data.len() as u64), data.to_vec()].concat()
}

fn uint_element(id: u32, value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let skip = bytes.iter().take_while(|b| **b == 0).count().min(7);
    element(id, &bytes[skip..])
}

/// An unsigned integer in 8 bytes, whatever it is (a seek position told
/// once known).
fn uint8_element(id: u32, value: u64) -> Vec<u8> {
    element(id, &value.to_be_bytes())
}

/// An element size as short as it fits (a size of all ones would mean
/// unknown).
fn size(n: u64) -> Vec<u8> {
    let len = (1..=8).find(|len| n < (1u64 << (7 * len)) - 1).unwrap_or(8);
    if len == 8 {
        return size8(n);
    }
    let marked = n | (1u64 << (7 * len));
    marked.to_be_bytes()[8 - len..].to_vec()
}

fn size8(n: u64) -> Vec<u8> {
    let mut bytes = n.to_be_bytes();
    bytes[0] = 0x01;
    bytes.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blocks() -> Vec<Block> {
        vec![
            Block { time_ms: 1_000, duration_ms: 0, data: vec![0x16, 0, 1] },
            Block { time_ms: 2_500, duration_ms: 1_200, data: vec![0x80, 0, 0] },
            // Past a cluster's span: in a cluster of its own.
            Block { time_ms: 95_000, duration_ms: 0, data: vec![0x16, 0, 2] },
        ]
    }

    #[test]
    fn sizes_are_as_short_as_they_fit() {
        assert_eq!(size(0), [0x80]);
        assert_eq!(size(126), [0xFE]);
        assert_eq!(size(127), [0x40, 0x7F], "0xFF would be unknown");
        assert_eq!(size(300), [0x41, 0x2C]);
        assert_eq!(size8(5), [0x01, 0, 0, 0, 0, 0, 0, 5]);
        assert_eq!(header(&[0x1F, 0x43, 0xB6, 0x75, 0x41, 0x2C], 0), Some((CLUSTER, Some(300), 6)));
        assert_eq!(
            header(&[0x1F, 0x43, 0xB6, 0x75, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], 0).unwrap().1,
            None
        );
    }

    #[test]
    fn whole_files_are_read_as_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.mks");
        write(&path, "S_VOBSUB", b"palette: 000000", &blocks()).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(readable(&bytes), Some(bytes.len()), "every element whole, the segment's size told");
        let (_, segment_size, data) =
            header(&bytes, header(&bytes, 0).map(|(_, s, at)| at + s.unwrap() as usize).unwrap()).unwrap();
        assert_eq!(data + segment_size.unwrap() as usize, bytes.len());
        // What players look for is there, in order.
        let find = |needle: &[u8]| bytes.windows(needle.len()).position(|w| w == needle);
        let (tracks, codec) = (find(&[0x16, 0x54, 0xAE, 0x6B]).unwrap(), find(b"S_VOBSUB").unwrap());
        let palette = find(b"palette: 000000").unwrap();
        let clusters: Vec<usize> =
            bytes.windows(4).enumerate().filter(|(_, w)| *w == [0x1F, 0x43, 0xB6, 0x75]).map(|(i, _)| i).collect();
        assert!(tracks < codec && codec < palette && palette < clusters[0]);
        assert_eq!(clusters.len(), 2);
        // The first block 0 ms into the first cluster (at 1 s), the second a
        // BlockGroup 1.5 s in, lasting 1.2 s, the third 0 ms into the second.
        assert_eq!(&bytes[clusters[0] + 5..clusters[0] + 9], &[0xE7, 0x82, 0x03, 0xE8]);
        let simple = find(&[0xA3, 0x87, 0x81, 0, 0, 0x80, 0x16, 0, 1]).unwrap();
        let group = find(&[0xA1, 0x87, 0x81, 0x05, 0xDC, 0, 0x80, 0, 0, 0x9B, 0x82, 0x04, 0xB0]).unwrap();
        assert!(clusters[0] < simple && simple < group && group < clusters[1]);
        assert!(find(&[0xE7, 0x83, 0x01, 0x73, 0x18, 0xA3, 0x87, 0x81, 0, 0, 0x80, 0x16, 0, 2]).is_some());
        // The seek head says where the Cues are, from the segment's data.
        // (The last of its ids: the seek head names it too.)
        let cues = bytes.windows(4).rposition(|w| w == [0x1C, 0x53, 0xBB, 0x6B]).unwrap();
        let seeks: Vec<usize> =
            bytes.windows(3).enumerate().filter(|(_, w)| *w == [0x53, 0xAC, 0x88]).map(|(i, _)| i).collect();
        let told = u64::from_be_bytes(bytes[seeks[2] + 3..seeks[2] + 11].try_into().unwrap());
        assert_eq!(data + told as usize, cues);
    }

    #[test]
    fn files_being_written_are_read_to_their_last_whole_element() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.mks");
        write(&path, "S_HDMV/PGS", &[], &blocks()).unwrap();
        let mut bytes = std::fs::read(&path).unwrap();
        // As FFmpeg writes it: the segment's size unknown until the end.
        let segment = header(&bytes, 0).map(|(_, s, at)| at + s.unwrap() as usize).unwrap();
        bytes[segment + 4..segment + 12].copy_from_slice(&[0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);
        let clusters: Vec<usize> =
            bytes.windows(4).enumerate().filter(|(_, w)| *w == [0x1F, 0x43, 0xB6, 0x75]).map(|(i, _)| i).collect();
        // Before the tracks are whole: nothing to read yet.
        let tracks = bytes.windows(4).position(|w| w == [0x16, 0x54, 0xAE, 0x6B]).unwrap();
        assert_eq!(readable(&bytes[..tracks + 6]), None);
        assert_eq!(readable(&bytes[..10]), None);
        // The tracks, then each cluster once it is whole.
        assert_eq!(readable(&bytes[..clusters[0] + 3]), Some(clusters[0]));
        assert_eq!(readable(&bytes[..clusters[1] - 1]), Some(clusters[0]));
        assert_eq!(readable(&bytes[..clusters[1]]), Some(clusters[1]));
        assert_eq!(readable(&bytes[..clusters[1] + 3]), Some(clusters[1]));
        assert_eq!(readable(b"not matroska"), None);
    }

    #[test]
    fn headers_alone_are_whole_files() {
        let bytes = header_only("S_DVBSUB");
        assert_eq!(readable(&bytes), Some(bytes.len()));
        assert!(bytes.windows(8).any(|w| w == b"S_DVBSUB"));
    }
}

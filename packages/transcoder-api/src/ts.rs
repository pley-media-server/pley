//! The MPEG-TS a live channel comes as, read as far as a reader joining it
//! late needs: the tables a demuxer wants before anything else (PAT and PMT)
//! and where video keyframes start. The server hands a new viewer the tables
//! and what came from the last keyframe; the transcoder starts FFmpeg on a
//! channel it held the same way.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use bytes::Bytes;

/// An MPEG-TS packet's size, and the byte it starts with.
pub const PACKET: usize = 188;
pub const SYNC: u8 = 0x47;

/// What came of a stream lately, at most `limit` bytes of it, with where its
/// video keyframes start.
pub struct Backlog {
    /// Each with when it came.
    chunks: VecDeque<(Instant, Bytes)>,
    bytes: usize,
    limit: usize,
    /// Chunks dropped from the front: the number of the first.
    dropped: u64,
    /// Where the keyframes held start, oldest first: their chunk's number
    /// and the offset in it.
    keyframes: VecDeque<(u64, usize)>,
    scan: Scan,
}

impl Backlog {
    pub fn new(limit: usize) -> Self {
        Self { chunks: VecDeque::new(), bytes: 0, limit, dropped: 0, keyframes: VecDeque::new(), scan: Scan::default() }
    }

    pub fn push(&mut self, chunk: &Bytes) {
        if let Some(offset) = self.scan.scan(chunk) {
            self.keyframes.push_back((self.dropped + self.chunks.len() as u64, offset));
        }
        self.bytes += chunk.len();
        self.chunks.push_back((Instant::now(), chunk.clone()));
        while self.bytes > self.limit {
            self.drop_front();
        }
    }

    fn drop_front(&mut self) {
        let Some((_, old)) = self.chunks.pop_front() else { return };
        self.bytes -= old.len();
        self.dropped += 1;
        while self.keyframes.front().is_some_and(|&(n, _)| n < self.dropped) {
            self.keyframes.pop_front();
        }
    }

    /// Lets go of what came before the newest keyframe at least `age` old,
    /// so a reader started at the first keyframe held ([`Backlog::start_held`])
    /// is given that much at once. Nothing goes until a keyframe is that old.
    pub fn trim_to_keyframe_older_than(&mut self, age: Duration) {
        let old_enough = |&(n, _): &(u64, usize)| {
            self.chunks.get((n - self.dropped) as usize).is_some_and(|(at, _)| at.elapsed() >= age)
        };
        let Some(&(n, _)) = self.keyframes.iter().rev().find(|k| old_enough(k)) else { return };
        while self.dropped < n && !self.chunks.is_empty() {
            self.drop_front();
        }
    }

    /// What a reader is given first: the tables and what came from the last
    /// video keyframe, so the picture starts whole and at once instead of
    /// seconds behind. A stream without marked keyframes is given all of it.
    pub fn start(&self) -> Vec<Bytes> {
        self.start_at(self.keyframes.back().copied())
    }

    /// The tables and all that is held from its first keyframe: a reader
    /// started on what was kept for it ([`Backlog::trim_to_keyframe_older_than`]).
    pub fn start_held(&self) -> Vec<Bytes> {
        self.start_at(self.keyframes.front().copied())
    }

    fn start_at(&self, keyframe: Option<(u64, usize)>) -> Vec<Bytes> {
        let (Some((n, offset)), Some(pat), Some(pmt)) = (keyframe, &self.scan.pat, &self.scan.pmt) else {
            return self.chunks.iter().map(|(_, chunk)| chunk.clone()).collect();
        };
        let from = (n - self.dropped) as usize;
        let mut out = vec![pat.clone(), pmt.clone()];
        for (i, (_, chunk)) in self.chunks.iter().skip(from).enumerate() {
            out.push(if i == 0 { chunk.slice(offset..) } else { chunk.clone() });
        }
        out
    }

    /// The bytes it holds.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn has_keyframe(&self) -> bool {
        !self.keyframes.is_empty()
    }
}

/// Reads the packets of a stream chunk by chunk (packets run across chunks).
#[derive(Default)]
struct Scan {
    /// Where the next packet starts in the next chunk; `None` until the
    /// packets are found.
    next: Option<usize>,
    pat: Option<Bytes>,
    pmt_pid: Option<u16>,
    pmt: Option<Bytes>,
}

impl Scan {
    /// Where the last video keyframe's first packet starts in `chunk`.
    fn scan(&mut self, chunk: &Bytes) -> Option<usize> {
        let data = &chunk[..];
        let mut at = match self.next {
            Some(n) if n >= data.len() => {
                self.next = Some(n - data.len());
                return None;
            }
            Some(n) if data[n] == SYNC => n,
            _ => sync(data)?,
        };
        let mut keyframe = None;
        while at + PACKET <= data.len() {
            if data[at] != SYNC {
                // Lost: found again further on, or in the next chunk.
                match sync(&data[at..]) {
                    Some(skip) => at += skip,
                    None => {
                        self.next = None;
                        return keyframe;
                    }
                }
                continue;
            }
            let packet = &data[at..at + PACKET];
            let pid = u16::from(packet[1] & 0x1f) << 8 | u16::from(packet[2]);
            if packet[1] & 0x40 != 0 {
                if pid == 0 {
                    self.pat = Some(Bytes::copy_from_slice(packet));
                    self.pmt_pid = pmt_pid(packet).or(self.pmt_pid);
                } else if Some(pid) == self.pmt_pid {
                    self.pmt = Some(Bytes::copy_from_slice(packet));
                } else if is_video_keyframe(packet) {
                    keyframe = Some(at);
                }
            }
            at += PACKET;
        }
        self.next = Some(if at < data.len() { PACKET - (data.len() - at) } else { 0 });
        keyframe
    }
}

/// Where packets start in `data`: a sync byte with another a packet on, when
/// there's that much.
pub fn sync(data: &[u8]) -> Option<usize> {
    (0..data.len()).find(|&i| data[i] == SYNC && data.get(i + PACKET).is_none_or(|&b| b == SYNC))
}

/// A packet's payload, after its adaptation field.
pub fn payload(packet: &[u8]) -> Option<&[u8]> {
    let control = (packet[3] >> 4) & 3;
    if control & 1 == 0 {
        return None;
    }
    let start = if control & 2 != 0 { 5 + usize::from(packet[4]) } else { 4 };
    packet.get(start..)
}

/// The PMT's PID, from a PAT packet: its first program's.
pub fn pmt_pid(packet: &[u8]) -> Option<u16> {
    let payload = payload(packet)?;
    let table = payload.get(1 + usize::from(*payload.first()?)..)?;
    // table_id 0, then the section's length and five bytes of header.
    if table.first() != Some(&0) {
        return None;
    }
    let length = usize::from(u16::from(*table.get(1)? & 0x0f) << 8 | u16::from(*table.get(2)?));
    let programs = table.get(8..(3 + length).saturating_sub(4))?;
    programs
        .as_chunks::<4>()
        .0
        .iter()
        .find(|p| p[0] != 0 || p[1] != 0)
        .map(|p| u16::from(p[2] & 0x1f) << 8 | u16::from(p[3]))
}

/// A packet that starts a video PES at a random access point: a keyframe.
fn is_video_keyframe(packet: &[u8]) -> bool {
    let control = (packet[3] >> 4) & 3;
    let random_access = control & 2 != 0 && packet[4] > 0 && packet[5] & 0x40 != 0;
    random_access
        && payload(packet).is_some_and(|p| p.len() >= 4 && p[..3] == [0, 0, 1] && (0xe0..=0xef).contains(&p[3]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: usize = 2 << 20;

    fn packet(pid: u16, start: bool, flags: Option<u8>, payload: &[u8]) -> Vec<u8> {
        let mut p = vec![SYNC, if start { 0x40 } else { 0 } | (pid >> 8) as u8, pid as u8, 0x10];
        if let Some(flags) = flags {
            p[3] = 0x30;
            p.extend([1, flags]);
        }
        p.extend_from_slice(payload);
        p.resize(PACKET, 0xff);
        p
    }

    fn pat(pmt: u16) -> Vec<u8> {
        packet(0, true, None, &[0, 0, 0xb0, 13, 0, 1, 0xc1, 0, 0, 0, 1, 0xe0 | (pmt >> 8) as u8, pmt as u8, 0, 0, 0, 0])
    }

    fn backlog(stream: &[u8], cuts: &[usize]) -> Backlog {
        let mut backlog = Backlog::new(LIMIT);
        let mut from = 0;
        for &to in cuts.iter().chain([stream.len()].iter()) {
            backlog.push(&Bytes::copy_from_slice(&stream[from..to]));
            from = to;
        }
        backlog
    }

    #[test]
    fn new_readers_start_at_the_last_keyframe_with_the_tables() {
        let (pat, pmt) = (pat(0x100), packet(0x100, true, None, &[0, 2]));
        let key = |n: u8| packet(0x101, true, Some(0x40), &[0, 0, 1, 0xe0, n]);
        let frame = packet(0x101, true, Some(0), &[0, 0, 1, 0xe0]);
        // Audio is marked at every frame: not where the picture starts.
        let audio = packet(0x102, true, Some(0x40), &[0, 0, 1, 0xc0]);
        let stream = [pat.clone(), pmt.clone(), key(1), frame.clone(), audio, key(2), frame.clone()].concat();
        // Cut across packets: the scan carries on in the next chunk.
        let start = backlog(&stream, &[400, 900]).start().concat();
        assert_eq!(start, [pat, pmt, key(2), frame].concat());
    }

    #[test]
    fn a_stream_without_marked_keyframes_is_given_whole() {
        let audio = packet(0x102, true, Some(0x40), &[0, 0, 1, 0xc0]);
        let stream = [pat(0x100), packet(0x100, true, None, &[0, 2]), audio.clone(), audio].concat();
        assert_eq!(backlog(&stream, &[300]).start().concat(), stream);
        // Not MPEG-TS at all: as it came.
        assert_eq!(backlog(b"not a transport stream", &[5]).start().concat(), b"not a transport stream");
    }

    #[test]
    fn a_keyframe_dropped_from_the_backlog_is_forgotten() {
        let mut b = backlog(
            &[pat(0x100), packet(0x100, true, None, &[0, 2]), packet(0x101, true, Some(0x40), &[0, 0, 1, 0xe0])]
                .concat(),
            &[],
        );
        assert!(b.has_keyframe());
        let filler = Bytes::from(packet(0x101, false, None, &[]).repeat(LIMIT / PACKET));
        b.push(&filler);
        assert!(!b.has_keyframe());
        assert_eq!(b.start(), vec![filler]);
    }

    #[test]
    fn trimmed_to_a_keyframe_old_enough_it_holds_from_there() {
        let (pat, pmt) = (pat(0x100), packet(0x100, true, None, &[0, 2]));
        let key = |n: u8| packet(0x101, true, Some(0x40), &[0, 0, 1, 0xe0, n]);
        let frame = packet(0x101, true, Some(0), &[0, 0, 1, 0xe0]);
        let chunks = [[pat.clone(), pmt.clone(), key(1)].concat(), frame.repeat(20), key(2), frame.clone()];
        let mut b = Backlog::new(LIMIT);
        for chunk in &chunks {
            b.push(&Bytes::copy_from_slice(chunk));
            // No keyframe is an hour old: nothing goes, and a reader starts at the first.
            b.trim_to_keyframe_older_than(Duration::from_secs(3600));
        }
        assert_eq!(b.bytes(), chunks.iter().map(Vec::len).sum::<usize>());
        assert_eq!(
            b.start_held().concat(),
            [pat.clone(), pmt.clone(), key(1), frame.repeat(20), key(2), frame.clone()].concat()
        );
        // Any age: from the newest; new readers start there either way.
        b.trim_to_keyframe_older_than(Duration::ZERO);
        assert_eq!(b.bytes(), 2 * PACKET);
        assert_eq!(b.start_held().concat(), [pat.clone(), pmt.clone(), key(2), frame.clone()].concat());
        assert_eq!(b.start().concat(), [pat, pmt, key(2), frame].concat());
    }

    #[test]
    fn a_pat_cut_short_is_no_pat() {
        // A pointer field leaving one byte of table.
        let mut p = packet(0, true, None, &[]);
        p[4] = (PACKET - 6) as u8;
        p[PACKET - 1] = 0;
        assert_eq!(pmt_pid(&p), None);
    }
}

//! The sound of an HLS channel kept apart from its picture: a variant that
//! names an audio group (`AUDIO="…"`) whose rendition has a playlist of its
//! own (`#EXT-X-MEDIA:TYPE=AUDIO,…,URI="…"`). Many broadcasters serve their
//! channels this way; followed alone, the variant plays without sound.
//!
//! The rendition's segments are put into the variant's MPEG-TS as one more
//! stream of its program, so what the tuner hands on (to the transcoder and
//! to players reading `stream.ts`) is one stream with sound, as a provider
//! sending MPEG-TS gives it.
//!
//! - Packed audio (ADTS AAC, its first timestamp in an ID3 tag before it) is
//!   made into PES packets; audio in MPEG-TS has its stream moved to a PID
//!   of its own.
//! - The picture's PMT names the new stream. A picture that has audio of its
//!   own already keeps it, and the rendition is no longer read.
//! - Each round's audio is spread among the picture's packets, so neither
//!   runs ahead of the other by more than a segment. The timestamps keep them
//!   together; the spreading only keeps a demuxer from waiting for one.
//! - The picture leads. Audio is read alongside it, given about a segment's
//!   time, and let be for a while after failing a few times in a row: a
//!   provider whose audio doesn't answer leaves gaps in the sound, not in the
//!   picture. What a round reads and makes is bounded.

use std::time::{Duration, Instant};

use super::{MediaPlaylist, NO_SEGMENTS, Segment, TS_PACKET, TS_SYNC, Tuner};
use super::{get_segment, get_text, media_playlist, pmt_pid, read_segment, ts_payload, ts_sync};

type Packet = [u8; TS_PACKET];

/// The most of an audio segment read: a few seconds of AAC are tens of
/// kilobytes.
const SEGMENT_BYTES: usize = 2 << 20;
/// The most audio segments, and bytes of them, read for one segment of the
/// picture: enough to catch up after a gap.
const ROUND_SEGMENTS: usize = 4;
const ROUND_BYTES: usize = 8 << 20;
/// The most audio packets put into one segment of the picture (packing
/// makes tiny frames a packet each).
const ROUND_PACKETS: usize = (16 << 20) / TS_PACKET;
/// How long the audio playlist may take.
pub(super) const PLAYLIST_WAIT: Duration = Duration::from_secs(5);
/// Less time than this left for a round starts no request.
const MIN_REQUEST: Duration = Duration::from_millis(250);
/// Failures in a row after which the audio is let be, for so long.
const FAILURES: u32 = 3;
const PAUSE: Duration = Duration::from_secs(30);
/// How long the first segment of the picture waits for its sound: the
/// transcoder decides from it whether the channel has any.
pub(super) const FIRST_SOUND_WAIT: Duration = Duration::from_secs(5);
/// Packed audio segments without a timestamp, before any had one, after
/// which the rendition is given up on.
const UNTIMED: u32 = 3;

/// The PES stream id of the first MPEG audio stream.
const AUDIO_STREAM_ID: u8 = 0xc0;
/// The stream type of ADTS AAC.
const ADTS_AAC: u8 = 0x0f;
/// Samples in an AAC frame's raw data block.
const AAC_SAMPLES: u64 = 1024;
/// The owner of the ID3 frame that gives packed audio's first timestamp.
const TIMESTAMP_OWNER: &[u8] = b"com.apple.streaming.transportStreamTimestamp\0";
/// MPEG timestamps wrap at 33 bits.
const PTS_MASK: u64 = (1 << 33) - 1;

/// The URI of an audio group's rendition: the default one, else one chosen
/// automatically, else the first. `None` when the group has none with a URI
/// (its audio is in the variant's segments).
pub(super) fn rendition(master: &str, group: &str) -> Option<String> {
    let mut best: Option<(u8, String)> = None;
    for line in master.lines().map(str::trim) {
        let Some(attrs) = line.strip_prefix("#EXT-X-MEDIA:") else { continue };
        if attribute(attrs, "TYPE") != Some("AUDIO") || attribute(attrs, "GROUP-ID") != Some(group) {
            continue;
        }
        let Some(uri) = attribute(attrs, "URI").filter(|u| !u.is_empty()) else { continue };
        let rank = if attribute(attrs, "DEFAULT") == Some("YES") {
            2
        } else {
            u8::from(attribute(attrs, "AUTOSELECT") == Some("YES"))
        };
        if best.as_ref().is_none_or(|(r, _)| rank > *r) {
            best = Some((rank, uri.to_owned()));
        }
    }
    best.map(|(_, uri)| uri)
}

/// An attribute of an HLS tag's attribute list, without its quotes. Quoted
/// values may hold commas (`CODECS="avc1.64002a,mp4a.40.2"`).
pub(super) fn attribute<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = attrs;
    while !rest.is_empty() {
        let (key, after) = rest.split_once('=')?;
        let (value, next) = match after.strip_prefix('"') {
            Some(quoted) => {
                let end = quoted.find('"')?;
                let next = quoted[end + 1..].split_once(',').map_or("", |(_, n)| n);
                (&quoted[..end], next)
            }
            None => after.split_once(',').unwrap_or((after, "")),
        };
        if key.trim() == name {
            return Some(value.trim());
        }
        rest = next;
    }
    None
}

/// An audio rendition's playlist, followed beside the picture's.
pub(super) struct Rendition {
    url: url::Url,
    playlist: Option<MediaPlaylist>,
    /// The next segment to read; set by the picture's first segment.
    next: Option<u64>,
    /// The last segment read, to tell whether the next follows it.
    last: Option<u64>,
    /// Seconds of audio read less those of the picture.
    ahead: f64,
    join: Join,
    /// It can't be put in (and why was said): no longer read.
    given_up: bool,
    /// Playlists and segments that failed in a row.
    failures: u32,
    /// Let be until then after failing.
    paused_until: Option<Instant>,
    /// A pause was said in the log (once is enough).
    said_paused: bool,
}

impl Rendition {
    pub(super) fn new(url: url::Url) -> Self {
        Self {
            url,
            playlist: None,
            next: None,
            last: None,
            ahead: 0.0,
            join: Join::default(),
            given_up: false,
            failures: 0,
            paused_until: None,
            said_paused: false,
        }
    }

    /// Whether it is still read.
    pub(super) fn wanted(&self) -> bool {
        !self.given_up && self.join.wanted()
    }

    /// Whether it is read now: not let be after failing.
    pub(super) fn active(&self) -> bool {
        self.wanted() && self.paused_until.is_none_or(|until| Instant::now() >= until)
    }

    /// Reads the picture's segment's tables before its sound is waited for:
    /// a picture with sound of its own doesn't wait.
    pub(super) fn look(&mut self, video: &[u8]) {
        if let Some(start) = ts_sync(video) {
            // A PMT it can't use is said when the segment is joined.
            let _ = self.join.read_tables(video[start..].as_chunks::<TS_PACKET>().0);
        }
    }

    /// Reads its playlist again, by `by`. Only one it can never read
    /// (fragmented MP4, encrypted) gives it up; one empty for a moment keeps
    /// the last.
    pub(super) async fn refresh(&mut self, tuner: &Tuner, http: &reqwest::Client, by: Instant) {
        if !self.active() {
            return;
        }
        let Some(wait) = within(PLAYLIST_WAIT, by, Instant::now()) else { return };
        let text = match get_text(tuner, http, &self.url, wait).await {
            Ok(text) => text,
            Err(why) => return self.failed(tuner.channel_id, &why),
        };
        match media_playlist(&text) {
            Ok(playlist) => self.playlist = Some(playlist),
            Err(why) if why == NO_SEGMENTS => {
                tracing::debug!(channel = tuner.channel_id, "a live channel's audio playlist is empty for now");
            }
            Err(why) => self.give_up(tuner.channel_id, &why),
        }
    }

    /// The audio segments that cover `seconds` more of the picture, whose
    /// segment `sequence` is read alongside, as many as come by `by`. Each
    /// may take about its own length; one that fails is read again next
    /// time.
    pub(super) async fn take(
        &mut self,
        tuner: &Tuner,
        http: &reqwest::Client,
        sequence: u64,
        seconds: f64,
        by: Instant,
    ) -> Vec<Sound> {
        if !self.active() {
            return Vec::new();
        }
        self.ahead -= seconds;
        let mut out = Vec::new();
        let mut bytes = 0;
        for segment in self.due(sequence) {
            let Ok(at) = self.url.join(&segment.uri) else {
                self.read(&segment);
                continue;
            };
            let Some(wait) = within(Duration::from_secs_f64(segment.duration.clamp(2.0, 10.0)), by, Instant::now())
            else {
                break;
            };
            let read = match get_segment(tuner, http, at, wait).await {
                Ok(resp) => read_segment(resp, SEGMENT_BYTES).await,
                Err(why) => Err(why),
            };
            match read {
                Ok(data) => {
                    let follows = self.last.is_some_and(|last| last + 1 == segment.sequence);
                    self.last = Some(segment.sequence);
                    self.read(&segment);
                    self.failures = 0;
                    bytes += data.len();
                    out.push(Sound { data, follows });
                    if bytes >= ROUND_BYTES {
                        break;
                    }
                }
                Err(why) => {
                    self.failed(tuner.channel_id, &why);
                    break;
                }
            }
        }
        out
    }

    /// The segments to read for the audio to catch up with the picture:
    /// from the next one, [`ROUND_SEGMENTS`] at most. At first it starts at
    /// the picture's segment `sequence` when the two number their segments
    /// alike, as packagers do; else as far behind its live edge as the
    /// picture.
    fn due(&mut self, sequence: u64) -> Vec<Segment> {
        let Some(playlist) = &self.playlist else { return Vec::new() };
        let next = *self.next.get_or_insert_with(|| {
            if playlist.segments.iter().any(|s| s.sequence == sequence) {
                sequence
            } else {
                playlist.last_sequence().saturating_sub(2)
            }
        });
        let mut ahead = self.ahead;
        playlist
            .segments
            .iter()
            .filter(|s| s.sequence >= next)
            .take_while(|s| {
                let behind = ahead < 0.0;
                ahead += s.duration;
                behind
            })
            .take(ROUND_SEGMENTS)
            .cloned()
            .collect()
    }

    fn read(&mut self, segment: &Segment) {
        self.next = Some(segment.sequence + 1);
        self.ahead += segment.duration;
    }

    /// A playlist or segment didn't come: after a few in a row, the audio
    /// is let be for a while, then picked up at its live edge, from a
    /// playlist read after the pause.
    fn failed(&mut self, channel: i64, why: &str) {
        tracing::debug!(channel, "a live channel's audio didn't come: {why}");
        self.failures += 1;
        if self.failures >= FAILURES {
            if !self.said_paused {
                tracing::warn!(channel, "a live channel's separate audio isn't answering ({why}): tried again later");
                self.said_paused = true;
            }
            self.failures = 0;
            self.paused_until = Some(Instant::now() + PAUSE);
            (self.playlist, self.next, self.last, self.ahead) = (None, None, None, 0.0);
        }
    }

    /// The picture's segment with `audio` put in; as it is when the audio
    /// can't be.
    pub(super) fn join(&mut self, channel: i64, video: &[u8], audio: &[Sound]) -> Vec<u8> {
        match self.join.join(video, audio) {
            Ok(joined) => joined,
            Err(Unsupported(why)) => {
                self.give_up(channel, why);
                video.to_vec()
            }
        }
    }

    fn give_up(&mut self, channel: i64, why: &str) {
        tracing::warn!(channel, "a live channel's separate audio can't be played: {why}");
        self.given_up = true;
    }
}

/// An audio segment read, and whether it follows the last one read: one
/// that doesn't (after a gap or a pause) isn't timed from it.
pub(super) struct Sound {
    data: Vec<u8>,
    follows: bool,
}

/// How long a request may take to be done by `by`: `wait`, or what is left
/// if that is less; `None` when too little is left to start one.
fn within(wait: Duration, by: Instant, now: Instant) -> Option<Duration> {
    let left = by.checked_duration_since(now)?;
    (left >= MIN_REQUEST).then(|| wait.min(left))
}

/// Why a rendition's audio can't be put in: it is no longer read.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Unsupported(pub &'static str);

/// Puts a rendition's audio segments into the picture's, segment after
/// segment, keeping what it learned of the streams between them.
#[derive(Default)]
pub(super) struct Join {
    /// The picture's PMT PID, from its PAT.
    pmt_pid: Option<u16>,
    /// The PID the audio goes out on, chosen free in the picture's PMT.
    pid: Option<u16>,
    /// The audio's stream type and descriptors, for the PMT: known from its
    /// first segment.
    stream: Option<(u8, Vec<u8>)>,
    /// The audio packets' continuity counter.
    continuity: u8,
    /// The timestamp after the last packed audio frame, for a segment
    /// without one of its own.
    next_pts: Option<u64>,
    /// Packed audio segments without a timestamp, before any had one.
    untimed: u32,
    /// The picture carries audio of its own: nothing is put in.
    own_audio: bool,
}

impl Join {
    /// Whether the rendition is still wanted.
    pub(super) fn wanted(&self) -> bool {
        !self.own_audio
    }

    /// The picture's segment `video` with the audio segments `audio` put in.
    pub(super) fn join(&mut self, video: &[u8], audio: &[Sound]) -> Result<Vec<u8>, Unsupported> {
        let Some(start) = ts_sync(video) else { return Ok(video.to_vec()) };
        let packets = video[start..].as_chunks::<TS_PACKET>().0;
        self.read_tables(packets)?;
        if self.own_audio {
            return Ok(video.to_vec());
        }
        let Some(pid) = self.pid else { return Ok(video.to_vec()) };
        let mut sound = Vec::new();
        for segment in audio {
            if !segment.follows {
                self.next_pts = None;
            }
            self.audio_packets(&segment.data, pid, &mut sound)?;
        }
        let mut out = Vec::with_capacity(video.len() + sound.len() * TS_PACKET);
        out.extend_from_slice(&video[..start]);
        let (n, m) = (packets.len(), sound.len());
        let mut given = 0;
        for (i, packet) in packets.iter().enumerate() {
            match self.with_audio(packet, pid)? {
                Some(pmt) => out.extend_from_slice(&pmt),
                None => out.extend_from_slice(packet),
            }
            // A packet behind: none before the picture's first tables.
            while given * n < i * m {
                out.extend_from_slice(&sound[given]);
                given += 1;
            }
        }
        for packet in &sound[given..] {
            out.extend_from_slice(packet);
        }
        out.extend_from_slice(&video[start + n * TS_PACKET..]);
        Ok(out)
    }

    /// Reads the picture's PAT and PMT: the PID to put the audio on, and
    /// whether it has audio of its own.
    fn read_tables(&mut self, packets: &[Packet]) -> Result<(), Unsupported> {
        for packet in packets {
            if packet[0] != TS_SYNC || packet[1] & 0x40 == 0 {
                continue;
            }
            let pid = packet_pid(packet);
            if pid == 0 {
                self.pmt_pid = pmt_pid(packet).or(self.pmt_pid);
            } else if Some(pid) == self.pmt_pid {
                let Some(pmt) = Pmt::read(packet) else {
                    if pmt_spans(packet) {
                        return Err(Unsupported("its picture's PMT spans several packets"));
                    }
                    continue;
                };
                if pmt.streams.iter().any(Stream::is_audio) {
                    self.own_audio = true;
                    return Ok(());
                }
                if self.pid.is_none_or(|p| pmt.streams.iter().any(|s| s.pid == p)) {
                    let taken = |p: u16| p == pid || p == pmt.pcr_pid || pmt.streams.iter().any(|s| s.pid == p);
                    self.pid = (0x100..0x1fff).find(|&p| !taken(p));
                }
            }
        }
        Ok(())
    }

    /// A PMT packet of the picture with the audio stream named in it; `None`
    /// for any other packet, or before the audio's stream is known.
    fn with_audio(&self, packet: &Packet, pid: u16) -> Result<Option<Packet>, Unsupported> {
        if packet[1] & 0x40 == 0 || Some(packet_pid(packet)) != self.pmt_pid {
            return Ok(None);
        }
        let Some((stream_type, descriptors)) = &self.stream else { return Ok(None) };
        let Some(at) = section_at(packet) else { return Ok(None) };
        let section = &packet[at..];
        if section.len() < 12 || section[0] != 0x02 {
            return Ok(None);
        }
        if section[6] != 0 || section[7] != 0 {
            return Err(Unsupported("its picture's PMT has several sections"));
        }
        let length = usize::from(u16::from(section[1] & 0x0f) << 8 | u16::from(section[2]));
        if length < 13 || 3 + length > section.len() {
            return Ok(None);
        }
        // Where the CRC is: the new stream goes there, the CRC after it.
        let crc_at = at + 3 + length - 4;
        let mut entry = vec![*stream_type, 0xe0 | (pid >> 8) as u8, pid as u8];
        entry.extend_from_slice(&[0xf0 | (descriptors.len() >> 8) as u8, descriptors.len() as u8]);
        entry.extend_from_slice(descriptors);
        if crc_at + entry.len() + 4 > TS_PACKET {
            return Err(Unsupported("its picture's PMT has no room for another stream"));
        }
        let mut out = *packet;
        out[crc_at..crc_at + entry.len()].copy_from_slice(&entry);
        let length = length + entry.len();
        out[at + 1] = (section[1] & 0xf0) | (length >> 8) as u8;
        out[at + 2] = length as u8;
        let end = crc_at + entry.len();
        let crc = crc32(&out[at..end]);
        out[end..end + 4].copy_from_slice(&crc.to_be_bytes());
        Ok(Some(out))
    }

    /// One audio segment's packets, on `pid`.
    fn audio_packets(&mut self, segment: &[u8], pid: u16, out: &mut Vec<Packet>) -> Result<(), Unsupported> {
        let (timestamp, data) = id3_timestamp(segment);
        if data.len() >= TS_PACKET && data[0] == TS_SYNC && ts_sync(data) == Some(0) {
            self.ts_audio(data, pid, out)
        } else if is_adts(data) {
            self.adts_audio(data, timestamp, pid, out)
        } else if data.is_empty() {
            Ok(())
        } else {
            Err(Unsupported("its audio is neither AAC nor MPEG-TS"))
        }
    }

    /// Audio in MPEG-TS: its first audio stream's packets, moved to `pid`.
    fn ts_audio(&mut self, data: &[u8], pid: u16, out: &mut Vec<Packet>) -> Result<(), Unsupported> {
        let packets = data.as_chunks::<TS_PACKET>().0;
        let mut pmt_at = None;
        let mut source = None;
        for packet in packets.iter().filter(|p| p[0] == TS_SYNC && p[1] & 0x40 != 0) {
            let at = packet_pid(packet);
            if at == 0 {
                pmt_at = pmt_pid(packet).or(pmt_at);
            } else if Some(at) == pmt_at
                && let Some(pmt) = Pmt::read(packet)
            {
                source = pmt.streams.into_iter().find(Stream::is_audio);
                break;
            }
        }
        let Some(source) = source else {
            return Err(Unsupported("its audio segments name no audio stream"));
        };
        self.stream = Some((source.kind, source.descriptors));
        for packet in packets.iter().filter(|p| p[0] == TS_SYNC && packet_pid(*p) == source.pid) {
            let mut p = *packet;
            p[1] = (p[1] & 0xe0) | (pid >> 8) as u8;
            p[2] = pid as u8;
            if p[3] & 0x10 != 0 {
                p[3] = (p[3] & 0xf0) | self.continuity;
                self.continuity = (self.continuity + 1) & 0x0f;
            } else {
                p[3] = (p[3] & 0xf0) | (self.continuity.wrapping_sub(1) & 0x0f);
            }
            push(out, p)?;
        }
        Ok(())
    }

    /// Packed ADTS AAC: each frame a PES packet of its own, timed from the
    /// segment's timestamp (or the last segment's end) by the frames before.
    fn adts_audio(
        &mut self,
        mut data: &[u8],
        timestamp: Option<u64>,
        pid: u16,
        out: &mut Vec<Packet>,
    ) -> Result<(), Unsupported> {
        let Some(mut pts) = timestamp.or(self.next_pts) else {
            // Nothing to time it by: left out, unless none ever is.
            self.untimed += 1;
            if self.untimed >= UNTIMED {
                return Err(Unsupported("its audio segments carry no timestamp"));
            }
            return Ok(());
        };
        self.stream = Some((ADTS_AAC, Vec::new()));
        // Samples since `pts`, at `rate`: exact however many frames.
        let (mut samples, mut rate) = (0u64, 0u64);
        while is_adts(data) {
            let length = usize::from(data[3] & 0x03) << 11 | usize::from(data[4]) << 3 | usize::from(data[5] >> 5);
            let Some(frame_rate) = sample_rate(data[2]) else { break };
            if length < 7 || length > data.len() {
                break;
            }
            if frame_rate != rate {
                // A new rate counts from where the old one got to.
                pts += (samples * 90_000).checked_div(rate).unwrap_or(0);
                (samples, rate) = (0, frame_rate);
            }
            let at = (pts + samples * 90_000 / rate) & PTS_MASK;
            self.packetize(&pes(&data[..length], at), pid, out)?;
            samples += AAC_SAMPLES * (u64::from(data[6] & 0x03) + 1);
            data = &data[length..];
        }
        if let Some(elapsed) = (samples * 90_000).checked_div(rate) {
            self.next_pts = Some((pts + elapsed) & PTS_MASK);
        }
        Ok(())
    }

    /// A PES packet cut into TS packets on `pid`, the last one filled out by
    /// its adaptation field.
    fn packetize(&mut self, pes: &[u8], pid: u16, out: &mut Vec<Packet>) -> Result<(), Unsupported> {
        let mut rest = pes;
        let mut first = true;
        while !rest.is_empty() {
            let mut p = [0xff; TS_PACKET];
            p[0] = TS_SYNC;
            p[1] = if first { 0x40 } else { 0 } | (pid >> 8) as u8;
            p[2] = pid as u8;
            let room = TS_PACKET - 4;
            let take = rest.len().min(room);
            if take == room {
                p[3] = 0x10 | self.continuity;
            } else {
                let stuffing = room - take;
                p[3] = 0x30 | self.continuity;
                p[4] = (stuffing - 1) as u8;
                if stuffing > 1 {
                    p[5] = 0;
                }
            }
            p[TS_PACKET - take..].copy_from_slice(&rest[..take]);
            push(out, p)?;
            self.continuity = (self.continuity + 1) & 0x0f;
            rest = &rest[take..];
            first = false;
        }
        Ok(())
    }
}

/// One more audio packet for a segment of the picture, [`ROUND_PACKETS`] at
/// most.
fn push(out: &mut Vec<Packet>, packet: Packet) -> Result<(), Unsupported> {
    if out.len() >= ROUND_PACKETS {
        return Err(Unsupported("its audio is far larger than sound can be"));
    }
    out.push(packet);
    Ok(())
}

/// What a PMT section says: the PCR's PID and its streams.
struct Pmt {
    pcr_pid: u16,
    streams: Vec<Stream>,
}

struct Stream {
    kind: u8,
    pid: u16,
    descriptors: Vec<u8>,
}

impl Stream {
    /// MPEG-1 and -2 audio, AAC (ADTS and LATM), AC-3 and E-AC-3; or private
    /// data that DVB's descriptors say is AC-3, E-AC-3, DTS or AAC.
    fn is_audio(&self) -> bool {
        match self.kind {
            0x03 | 0x04 | 0x0f | 0x11 | 0x81 | 0x87 => true,
            0x06 => descriptor_tags(&self.descriptors).any(|tag| matches!(tag, 0x6a | 0x7a | 0x7b | 0x7c)),
            _ => false,
        }
    }
}

impl Pmt {
    /// From a packet holding the start of a PMT; `None` for one that isn't
    /// whole in it.
    fn read(packet: &[u8]) -> Option<Self> {
        let section = packet.get(section_at(packet)?..)?;
        if *section.first()? != 0x02 {
            return None;
        }
        let length = usize::from(u16::from(*section.get(1)? & 0x0f) << 8 | u16::from(*section.get(2)?));
        let section = section.get(..3 + length)?;
        let pcr_pid = u16::from(*section.get(8)? & 0x1f) << 8 | u16::from(*section.get(9)?);
        let info = usize::from(u16::from(*section.get(10)? & 0x0f) << 8 | u16::from(*section.get(11)?));
        let mut rest = section.get(12 + info..section.len().checked_sub(4)?)?;
        let mut streams = Vec::new();
        while rest.len() >= 5 {
            let pid = u16::from(rest[1] & 0x1f) << 8 | u16::from(rest[2]);
            let info = usize::from(u16::from(rest[3] & 0x0f) << 8 | u16::from(rest[4]));
            let descriptors = rest.get(5..5 + info)?.to_vec();
            streams.push(Stream { kind: rest[0], pid, descriptors });
            rest = &rest[5 + info..];
        }
        Some(Self { pcr_pid, streams })
    }
}

/// Where the table section a packet starts begins in it, after its pointer
/// field; `None` for a packet with no room for one.
fn section_at(packet: &[u8]) -> Option<usize> {
    let payload = ts_payload(packet)?;
    let pointer = usize::from(*payload.first()?);
    let at = packet.len() - payload.len() + 1 + pointer;
    (at < packet.len()).then_some(at)
}

/// A PMT that goes on in the packets after this one.
fn pmt_spans(packet: &[u8]) -> bool {
    let Some(section) = section_at(packet).map(|at| &packet[at..]) else { return false };
    section.len() >= 3
        && section[0] == 0x02
        && 3 + usize::from(u16::from(section[1] & 0x0f) << 8 | u16::from(section[2])) > section.len()
}

/// The tags of a run of descriptors.
fn descriptor_tags(mut descriptors: &[u8]) -> impl Iterator<Item = u8> + '_ {
    std::iter::from_fn(move || {
        let (&tag, rest) = descriptors.split_first()?;
        let (&length, rest) = rest.split_first()?;
        descriptors = rest.get(usize::from(length)..)?;
        Some(tag)
    })
}

fn packet_pid(packet: &[u8]) -> u16 {
    u16::from(packet[1] & 0x1f) << 8 | u16::from(packet[2])
}

/// An ADTS header starts `data`: its sync word, MPEG layer 0.
fn is_adts(data: &[u8]) -> bool {
    data.len() >= 7 && data[0] == 0xff && data[1] & 0xf6 == 0xf0
}

/// The sample rate an ADTS header's third byte gives.
fn sample_rate(byte: u8) -> Option<u64> {
    const RATES: [u64; 13] =
        [96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 8_000, 7_350];
    RATES.get(usize::from((byte >> 2) & 0x0f)).copied()
}

/// The ID3 tags before packed audio: the timestamp of its first frame, if
/// one gives it, and what follows them.
fn id3_timestamp(mut data: &[u8]) -> (Option<u64>, &[u8]) {
    let mut timestamp = None;
    while data.len() >= 10 && data.starts_with(b"ID3") {
        // A synchsafe size: seven bits a byte. A footer adds ten bytes.
        let size = data[6..10].iter().fold(0usize, |size, &b| size << 7 | usize::from(b & 0x7f));
        let end = (10 + size + if data[5] & 0x10 != 0 { 10 } else { 0 }).min(data.len());
        let tag = &data[10.min(end)..end];
        if let Some(at) = tag.windows(TIMESTAMP_OWNER.len()).position(|w| w == TIMESTAMP_OWNER)
            && let Some(bytes) = tag.get(at + TIMESTAMP_OWNER.len()..at + TIMESTAMP_OWNER.len() + 8)
        {
            timestamp = Some(u64::from_be_bytes(bytes.try_into().unwrap_or_default()) & PTS_MASK);
        }
        data = &data[end..];
    }
    (timestamp, data)
}

/// An audio frame as a PES packet with its presentation timestamp.
fn pes(frame: &[u8], pts: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(14 + frame.len());
    out.extend_from_slice(&[0, 0, 1, AUDIO_STREAM_ID]);
    out.extend_from_slice(&((frame.len() + 8) as u16).to_be_bytes());
    // Data aligned; a PTS, five bytes of it.
    out.extend_from_slice(&[0x84, 0x80, 5]);
    out.extend_from_slice(&[
        0x21 | ((pts >> 29) & 0x0e) as u8,
        (pts >> 22) as u8,
        0x01 | ((pts >> 14) & 0xfe) as u8,
        (pts >> 7) as u8,
        0x01 | ((pts << 1) & 0xfe) as u8,
    ]);
    out.extend_from_slice(frame);
    out
}

/// The CRC that ends an MPEG-TS table section.
fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= u32::from(byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 { crc << 1 ^ 0x04c1_1db7 } else { crc << 1 };
        }
    }
    crc
}

#[cfg(test)]
mod tests {
    use super::*;

    const MASTER: &str = "#EXTM3U\n\
        #EXT-X-STREAM-INF:BANDWIDTH=8500800,CODECS=\"avc1.64002a,mp4a.40.2\",RESOLUTION=1920x1080,AUDIO=\"program_audio\",SUBTITLES=\"subs\"\n\
        master1080p.m3u8\n\
        #EXT-X-MEDIA:TYPE=AUDIO,LANGUAGE=\"de\",NAME=\"Clear speech\",AUTOSELECT=YES,DEFAULT=NO,GROUP-ID=\"program_audio\",URI=\"masteraudio2.m3u8\"\n\
        #EXT-X-MEDIA:TYPE=AUDIO,LANGUAGE=\"de\",NAME=\"German\",AUTOSELECT=YES,DEFAULT=YES,CHANNELS=\"2\",GROUP-ID=\"program_audio\",URI=\"masteraudio1.m3u8\"\n\
        #EXT-X-MEDIA:TYPE=SUBTITLES,NAME=\"German\",DEFAULT=YES,GROUP-ID=\"subs\",URI=\"mastersubs.m3u8\"\n";

    #[test]
    fn attributes_are_read_past_quoted_commas() {
        let attrs = "BANDWIDTH=1,CODECS=\"avc1.64002a,mp4a.40.2\",AUDIO=\"aac\",FRAME-RATE=50.000";
        assert_eq!(attribute(attrs, "AUDIO"), Some("aac"));
        assert_eq!(attribute(attrs, "CODECS"), Some("avc1.64002a,mp4a.40.2"));
        assert_eq!(attribute(attrs, "FRAME-RATE"), Some("50.000"));
        assert_eq!(attribute(attrs, "RESOLUTION"), None);
    }

    #[test]
    fn the_default_rendition_of_the_group_is_read() {
        assert_eq!(rendition(MASTER, "program_audio").as_deref(), Some("masteraudio1.m3u8"));
        assert_eq!(rendition(MASTER, "subs"), None);
        assert_eq!(rendition(MASTER, "other"), None);
        // Without a default: one chosen automatically; a rendition without a
        // URI is in the variant.
        let master = "#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",NAME=\"x\",URI=\"x.m3u8\"\n\
                      #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"a\",NAME=\"y\",AUTOSELECT=YES,URI=\"y.m3u8\"\n\
                      #EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID=\"b\",NAME=\"z\",DEFAULT=YES\n";
        assert_eq!(rendition(master, "a").as_deref(), Some("y.m3u8"));
        assert_eq!(rendition(master, "b"), None);
    }

    fn segments(text: &str) -> MediaPlaylist {
        media_playlist(text).unwrap()
    }

    #[test]
    fn audio_is_read_as_far_as_the_picture_goes() {
        let mut r = Rendition::new("http://provider.invalid/a.m3u8".parse().unwrap());
        assert!(r.due(10).is_empty());
        let text =
            "#EXTM3U\n#EXT-X-TARGETDURATION:2\n#EXT-X-MEDIA-SEQUENCE:8\n".to_owned() + &"#EXTINF:2,\na.aac\n".repeat(8);
        r.playlist = Some(segments(&text));
        // It starts where the picture does, when the numbers match.
        r.ahead = -2.0;
        let due = r.due(10);
        assert_eq!(due.iter().map(|s| s.sequence).collect::<Vec<_>>(), [10]);
        r.read(&due[0]);
        // Caught up: nothing until the picture goes on. Behind: no more than
        // a round's worth.
        assert!(r.due(11).is_empty());
        r.ahead = -60.0;
        assert_eq!(r.due(11).iter().map(|s| s.sequence).collect::<Vec<_>>(), [11, 12, 13, 14]);
        // Numbered otherwise: as far behind the live edge as the picture.
        let mut r = Rendition::new("http://provider.invalid/a.m3u8".parse().unwrap());
        r.playlist = Some(segments(&text));
        r.ahead = -2.0;
        assert_eq!(r.due(500)[0].sequence, 13);
    }

    #[test]
    fn audio_that_keeps_failing_is_let_be_and_picked_up_again() {
        let mut r = Rendition::new("http://provider.invalid/a.m3u8".parse().unwrap());
        (r.next, r.ahead) = (Some(5), -4.0);
        r.failed(1, "timed out");
        r.failed(1, "timed out");
        assert!(r.active());
        r.playlist = Some(segments("#EXTM3U\n#EXTINF:2,\na.aac\n"));
        r.failed(1, "timed out");
        assert!(!r.active() && r.wanted());
        // Picked up at its live edge, not where it stopped: from a playlist
        // read after the pause, and not timed from before it.
        assert_eq!((r.next, r.last, r.ahead), (None, None, 0.0));
        assert!(r.playlist.is_none() && r.due(5).is_empty());
        r.paused_until = Some(Instant::now() - Duration::from_secs(1));
        assert!(r.active());
    }

    fn packet(pid: u16, start: bool, payload: &[u8]) -> Packet {
        let mut p = [0xff; TS_PACKET];
        p[..4].copy_from_slice(&[TS_SYNC, if start { 0x40 } else { 0 } | (pid >> 8) as u8, pid as u8, 0x10]);
        p[4..4 + payload.len()].copy_from_slice(payload);
        p
    }

    fn section(table: u8, body: &[u8]) -> Vec<u8> {
        let length = body.len() + 4;
        let mut s = vec![table, 0xb0 | (length >> 8) as u8, length as u8];
        s.extend_from_slice(body);
        let crc = crc32(&s);
        s.extend_from_slice(&crc.to_be_bytes());
        [vec![0], s].concat()
    }

    fn pat(pmt: u16) -> Packet {
        packet(0, true, &section(0, &[0, 1, 0xc1, 0, 0, 0, 1, 0xe0 | (pmt >> 8) as u8, pmt as u8]))
    }

    fn pmt_body(streams: &[(u8, u16, &[u8])]) -> Vec<u8> {
        let mut body = vec![0, 1, 0xc1, 0, 0, 0xe0 | (streams[0].1 >> 8) as u8, streams[0].1 as u8, 0xf0, 0];
        for &(kind, pid, descriptors) in streams {
            body.extend_from_slice(&[kind, 0xe0 | (pid >> 8) as u8, pid as u8, 0xf0, descriptors.len() as u8]);
            body.extend_from_slice(descriptors);
        }
        body
    }

    /// A PMT with PCR on the first stream.
    fn pmt(pid: u16, streams: &[(u8, u16)]) -> Packet {
        let streams: Vec<_> = streams.iter().map(|&(kind, pid)| (kind, pid, &[][..])).collect();
        packet(pid, true, &section(2, &pmt_body(&streams)))
    }

    fn streams(packet: &[u8]) -> Vec<(u8, u16)> {
        Pmt::read(packet).unwrap().streams.iter().map(|s| (s.kind, s.pid)).collect()
    }

    fn adts(rate_index: u8, payload: &[u8]) -> Vec<u8> {
        let length = 7 + payload.len();
        let mut frame = vec![
            0xff,
            0xf1,
            0x40 | (rate_index << 2),
            0x80 | (length >> 11) as u8,
            (length >> 3) as u8,
            ((length & 7) << 5) as u8 | 0x1f,
            0xfc,
        ];
        frame.extend_from_slice(payload);
        frame
    }

    fn id3(pts: u64) -> Vec<u8> {
        let mut frame = TIMESTAMP_OWNER.to_vec();
        frame.extend_from_slice(&pts.to_be_bytes());
        let mut tag = b"ID3\x04\0\0".to_vec();
        let size = 10 + frame.len();
        tag.extend_from_slice(&[0, 0, 0, size as u8]);
        tag.extend_from_slice(b"PRIV");
        tag.extend_from_slice(&[0, 0, 0, frame.len() as u8, 0, 0]);
        tag.extend_from_slice(&frame);
        tag
    }

    fn read_pts(p: &[u8]) -> u64 {
        u64::from(p[0] & 0x0e) << 29
            | u64::from(p[1]) << 22
            | u64::from(p[2] & 0xfe) << 14
            | u64::from(p[3]) << 7
            | u64::from(p[4]) >> 1
    }

    fn heard(data: Vec<u8>) -> Sound {
        Sound { data, follows: true }
    }

    fn packets(data: &[u8]) -> Vec<Packet> {
        data.as_chunks::<TS_PACKET>().0.to_vec()
    }

    /// The timestamps of the PES packets on `pid`.
    fn timestamps(out: &[Packet], pid: u16) -> Vec<u64> {
        out.iter()
            .filter(|p| packet_pid(*p) == pid && p[1] & 0x40 != 0)
            .map(|p| {
                let pes = ts_payload(p).unwrap();
                read_pts(&pes[9..14])
            })
            .collect()
    }

    #[test]
    fn packed_aac_goes_into_the_picture_named_in_its_pmt() {
        let video = [pat(0x1000), pmt(0x1000, &[(0x1b, 0x100)]), packet(0x100, true, &[0, 0, 1, 0xe0])].concat();
        let video = [video, packet(0x100, false, &[]).repeat(3)].concat();
        let audio = [id3(90_000), adts(3, &[1; 300]), adts(3, &[2; 10])].concat();
        let mut join = Join::default();
        let out = packets(&join.join(&video, &[heard(audio)]).unwrap());

        // The PMT names an audio stream on a free PID, with a good CRC.
        assert_eq!(streams(&out[1]), [(0x1b, 0x100), (ADTS_AAC, 0x101)]);
        let section = &out[1][5..5 + 3 + 23];
        assert_eq!(crc32(section), 0);

        // Each frame is a PES packet with its timestamp: 1024 samples at
        // 48 kHz apart. The first spans two packets, the last filled out.
        let sound: Vec<_> = out.iter().filter(|p| packet_pid(*p) == 0x101).collect();
        assert_eq!(sound.len(), 3);
        assert_eq!(sound.iter().map(|p| p[3] & 0x0f).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(&sound[0][4..8], &[0, 0, 1, AUDIO_STREAM_ID]);
        assert_eq!(timestamps(&out, 0x101), [90_000, 90_000 + 1920]);
        assert_eq!(sound[1][1] & 0x40, 0);
        let last = sound[2];
        assert_eq!(last[3] & 0x30, 0x30);
        let pes = &last[5 + usize::from(last[4])..];
        assert_eq!(&pes[14..21], &adts(3, &[2; 10])[..7]);

        // Spread among the picture's packets after its tables, the
        // picture's order kept.
        let at: Vec<_> = out.iter().map(|p| packet_pid(p)).collect();
        assert_eq!(at, [0, 0x1000, 0x101, 0x100, 0x100, 0x101, 0x100, 0x100, 0x101]);
        assert_eq!(out[3], packets(&video)[2]);

        // A segment without a timestamp follows on from the last.
        let out = packets(&join.join(&video, &[heard(adts(3, &[3; 10]))]).unwrap());
        let sound: Vec<_> = out.iter().filter(|p| packet_pid(*p) == 0x101).collect();
        assert_eq!(sound[0][3] & 0x0f, 3);
        assert_eq!(timestamps(&out, 0x101), [90_000 + 2 * 1920]);
    }

    #[test]
    fn timestamps_wrap_at_33_bits() {
        let video = [pat(0x1000), pmt(0x1000, &[(0x1b, 0x100)])].concat();
        let audio = [id3(PTS_MASK - 959), adts(3, &[0; 4]), adts(3, &[0; 4])].concat();
        let mut join = Join::default();
        let out = packets(&join.join(&video, &[heard(audio)]).unwrap());
        assert_eq!(timestamps(&out, 0x101), [PTS_MASK - 959, 960]);
        assert_eq!(join.next_pts, Some(960 + 1920));
    }

    #[test]
    fn a_frame_cut_short_ends_the_segment() {
        let video = [pat(0x1000), pmt(0x1000, &[(0x1b, 0x100)])].concat();
        let mut cut = adts(3, &[0; 100]);
        cut.truncate(50);
        let audio = [id3(0), adts(3, &[0; 4]), cut].concat();
        let mut join = Join::default();
        let out = packets(&join.join(&video, &[heard(audio)]).unwrap());
        assert_eq!(timestamps(&out, 0x101), [0]);
        // The next segment goes on from the whole frames.
        assert_eq!(join.next_pts, Some(1920));
    }

    #[test]
    fn audio_in_mpeg_ts_moves_to_a_pid_of_its_own() {
        let video = [pat(0x1000), pmt(0x1000, &[(0x1b, 0x100), (0x15, 0x101)])].concat();
        // The audio's stream uses the picture's PID, AC-3 marked the DVB way.
        let audio_pmt = packet(0x1000, true, &section(2, &pmt_body(&[(0x06, 0x100, &[0x6a, 1, 0])])));
        let audio = [pat(0x1000), audio_pmt, packet(0x100, true, &[0, 0, 1, 0xbd])].concat();
        let mut join = Join::default();
        let out = packets(&join.join(&video, &[heard(audio)]).unwrap());
        assert_eq!(streams(&out[1]), [(0x1b, 0x100), (0x15, 0x101), (0x06, 0x102)]);
        // Its descriptor goes with it.
        assert_eq!(Pmt::read(&out[1]).unwrap().streams[2].descriptors, [0x6a, 1, 0]);
        assert_eq!(out.len(), 3);
        assert_eq!(packet_pid(&out[2]), 0x102);
        assert_eq!(&out[2][4..8], &[0, 0, 1, 0xbd]);
    }

    #[test]
    fn a_picture_with_sound_of_its_own_keeps_it() {
        let video = [pat(0x1000), pmt(0x1000, &[(0x1b, 0x100), (0x0f, 0x101)])].concat();
        let mut join = Join::default();
        assert_eq!(join.join(&video, &[heard(adts(3, &[0; 4]))]).unwrap(), video);
        assert!(!join.wanted());
        // AC-3 marked the DVB way is sound too; teletext isn't.
        let teletext = packet(0x1000, true, &section(2, &pmt_body(&[(0x1b, 0x100, &[]), (0x06, 0x101, &[0x56, 0])])));
        let mut join = Join::default();
        join.join(&[pat(0x1000), teletext].concat(), &[heard([id3(0), adts(3, &[0; 4])].concat())]).unwrap();
        assert!(join.wanted());
        let ac3 = packet(0x1000, true, &section(2, &pmt_body(&[(0x1b, 0x100, &[]), (0x06, 0x101, &[0x6a, 1, 0])])));
        let video = [pat(0x1000), ac3].concat();
        let mut join = Join::default();
        assert_eq!(join.join(&video, &[heard(adts(3, &[0; 4]))]).unwrap(), video);
        assert!(!join.wanted());
    }

    #[test]
    fn audio_that_cant_be_put_in_is_said() {
        let video = [pat(0x1000), pmt(0x1000, &[(0x1b, 0x100)])].concat();
        let mut join = Join::default();
        assert!(join.join(&video, &[heard(b"not audio".to_vec())]).is_err());
        // Packed AAC needs a timestamp to start from: a segment or two
        // without is left out, then the rendition is given up.
        let mut join = Join::default();
        for _ in 1..UNTIMED {
            assert_eq!(join.join(&video, &[heard(adts(3, &[0; 4]))]).unwrap(), video);
        }
        assert!(join.join(&video, &[heard(adts(3, &[0; 4]))]).is_err());
        // Tiny frames, each a packet, are bounded.
        let flood = [id3(0), adts(3, &[]).repeat(ROUND_PACKETS + 1)].concat();
        assert!(Join::default().join(&video, &[heard(flood)]).is_err());
    }

    #[test]
    fn a_pmt_with_no_room_is_said() {
        // One spanning packets.
        let mut long = pmt(0x1000, &[(0x1b, 0x100)]);
        long[6] = 0xb0 | 1;
        long[7] = 0;
        let video = [pat(0x1000), long].concat();
        let audio = [id3(0), adts(3, &[0; 4])].concat();
        assert!(Join::default().join(&video, &[heard(audio.clone())]).is_err());
        // One filling its packet.
        let full = packet(0x1000, true, &section(2, &pmt_body(&[(0x1b, 0x100, &[0x52; 160])])));
        let video = [pat(0x1000), full].concat();
        assert!(Join::default().join(&video, &[heard(audio)]).is_err());
    }

    #[test]
    fn a_pmt_packet_with_no_payload_left_is_passed_on() {
        // An adaptation field filling the packet: no pointer field to read.
        let mut empty = [0xff; TS_PACKET];
        empty[..6].copy_from_slice(&[TS_SYNC, 0x40 | 0x10, 0x00, 0x30, 183, 0]);
        let video = [pat(0x1000), pmt(0x1000, &[(0x1b, 0x100)]), empty].concat();
        let mut join = Join::default();
        let out = join.join(&video, &[heard([id3(0), adts(3, &[0; 4])].concat())]).unwrap();
        assert!(packets(&out).contains(&empty));
        assert_eq!(section_at(&empty), None);
        assert!(Pmt::read(&empty).is_none());
    }

    #[test]
    fn requests_end_by_the_rounds_deadline() {
        let now = Instant::now();
        let wait = Duration::from_secs(4);
        assert_eq!(within(wait, now + Duration::from_secs(10), now), Some(wait));
        assert_eq!(within(wait, now + Duration::from_secs(1), now), Some(Duration::from_secs(1)));
        // Too little left, or none: no request.
        assert_eq!(within(wait, now + Duration::from_millis(100), now), None);
        assert_eq!(within(wait, now, now + Duration::from_secs(1)), None);
    }

    #[test]
    fn a_picture_with_sound_of_its_own_doesnt_wait_for_the_rendition() {
        let mut r = Rendition::new("http://provider.invalid/a.m3u8".parse().unwrap());
        assert!(r.active());
        r.look(&[pat(0x1000), pmt(0x1000, &[(0x1b, 0x100), (0x0f, 0x101)])].concat());
        assert!(!r.active() && !r.wanted());
    }

    #[test]
    fn audio_after_a_gap_isnt_timed_from_before_it() {
        let video = [pat(0x1000), pmt(0x1000, &[(0x1b, 0x100)])].concat();
        let mut join = Join::default();
        join.join(&video, &[heard([id3(0), adts(3, &[0; 4])].concat())]).unwrap();
        assert_eq!(join.next_pts, Some(1920));
        // A segment without a timestamp after a gap is left out.
        let gap = Sound { data: adts(3, &[0; 4]), follows: false };
        assert_eq!(join.join(&video, &[gap]).unwrap().len(), video.len());
        assert_eq!(join.next_pts, None);
    }
}

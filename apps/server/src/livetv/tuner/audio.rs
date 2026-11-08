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

use super::{MediaPlaylist, TS_PACKET, TS_SYNC, Tuner, get_segment, get_text, media_playlist, pmt_pid, read_segment};
use super::{ts_payload, ts_sync};

type Packet = [u8; TS_PACKET];

/// The most of an audio segment read.
const AUDIO_SEGMENT_BYTES: usize = 8 << 20;

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

/// An audio rendition's playlist, followed beside the picture's. The
/// picture's playlist leads: a rendition that fails to answer leaves a gap
/// in the sound, not a broken connection.
pub(super) struct Rendition {
    url: url::Url,
    playlist: Option<MediaPlaylist>,
    /// The next segment to read; set by the first playlist.
    next: Option<u64>,
    /// Seconds of audio read less those of the picture.
    ahead: f64,
    join: Join,
    /// It can't be put in (and why was said): no longer read.
    given_up: bool,
}

impl Rendition {
    pub(super) fn new(url: url::Url) -> Self {
        Self { url, playlist: None, next: None, ahead: 0.0, join: Join::default(), given_up: false }
    }

    /// Whether it is still read.
    pub(super) fn wanted(&self) -> bool {
        !self.given_up && self.join.wanted()
    }

    /// Reads its playlist again. At first it starts where the picture does,
    /// `first`, when the two number their segments alike, as packagers do;
    /// else as far behind its live edge as the picture.
    pub(super) async fn refresh(&mut self, tuner: &Tuner, http: &reqwest::Client, first: u64) {
        let text = match get_text(tuner, http, &self.url).await {
            Ok(text) => text,
            Err(why) => {
                tracing::debug!(channel = tuner.channel_id, "a live channel's audio playlist didn't come: {why}");
                return;
            }
        };
        match media_playlist(&text) {
            Ok(playlist) => {
                if self.next.is_none() {
                    let aligned = playlist.segments.iter().any(|s| s.sequence == first);
                    self.next = Some(if aligned { first } else { playlist.last_sequence().saturating_sub(2) });
                }
                self.playlist = Some(playlist);
            }
            Err(why) => self.give_up(tuner.channel_id, &why),
        }
    }

    /// The audio segments that cover `seconds` more of the picture, as far
    /// as its playlist goes.
    pub(super) async fn take(&mut self, tuner: &Tuner, http: &reqwest::Client, seconds: f64) -> Vec<Vec<u8>> {
        let Self { url, playlist, next, ahead, .. } = self;
        *ahead -= seconds;
        let (Some(playlist), Some(next)) = (playlist.as_ref(), next.as_mut()) else { return Vec::new() };
        let mut out = Vec::new();
        let from = *next;
        for segment in playlist.segments.iter().filter(|s| s.sequence >= from) {
            if *ahead >= 0.0 {
                break;
            }
            *next = segment.sequence + 1;
            *ahead += segment.duration;
            let Ok(at) = url.join(&segment.uri) else { continue };
            let read = match get_segment(tuner, http, at).await {
                Ok(resp) => read_segment(resp, AUDIO_SEGMENT_BYTES).await,
                Err(why) => Err(why),
            };
            match read {
                Ok(bytes) => out.push(bytes),
                Err(why) => {
                    // The rest waits for the next segment: a provider that
                    // doesn't answer holds the picture up once, not for each.
                    tracing::debug!(channel = tuner.channel_id, "a live channel's audio segment failed: {why}");
                    break;
                }
            }
        }
        out
    }

    /// The picture's segment with `audio` put in; as it is when the audio
    /// can't be.
    pub(super) fn join(&mut self, channel: i64, video: &[u8], audio: &[Vec<u8>]) -> Vec<u8> {
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
    /// The audio's stream type, for the PMT: known from its first segment.
    stream_type: Option<u8>,
    /// The audio packets' continuity counter.
    continuity: u8,
    /// The timestamp after the last packed audio frame, for a segment
    /// without one of its own.
    next_pts: Option<u64>,
    /// The picture carries audio of its own: nothing is put in.
    own_audio: bool,
}

impl Join {
    /// Whether the rendition is still wanted.
    pub(super) fn wanted(&self) -> bool {
        !self.own_audio
    }

    /// The picture's segment `video` with the audio segments `audio` put in.
    pub(super) fn join(&mut self, video: &[u8], audio: &[impl AsRef<[u8]>]) -> Result<Vec<u8>, Unsupported> {
        let Some(start) = ts_sync(video) else { return Ok(video.to_vec()) };
        let packets = video[start..].as_chunks::<TS_PACKET>().0;
        self.read_tables(packets);
        if self.own_audio {
            return Ok(video.to_vec());
        }
        let Some(pid) = self.pid else { return Ok(video.to_vec()) };
        let mut sound = Vec::new();
        for segment in audio {
            self.audio_packets(segment.as_ref(), pid, &mut sound)?;
        }
        let mut out = Vec::with_capacity(video.len() + sound.len() * TS_PACKET);
        out.extend_from_slice(&video[..start]);
        let (n, m) = (packets.len(), sound.len());
        let mut given = 0;
        for (i, packet) in packets.iter().enumerate() {
            match self.with_audio(packet, pid) {
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
    fn read_tables(&mut self, packets: &[Packet]) {
        for packet in packets {
            if packet[0] != TS_SYNC || packet[1] & 0x40 == 0 {
                continue;
            }
            let pid = packet_pid(packet);
            if pid == 0 {
                self.pmt_pid = pmt_pid(packet).or(self.pmt_pid);
            } else if Some(pid) == self.pmt_pid
                && let Some(pmt) = Pmt::read(packet)
            {
                if pmt.streams.iter().any(|&(kind, _)| is_audio(kind)) {
                    self.own_audio = true;
                    return;
                }
                if self.pid.is_none_or(|p| pmt.streams.iter().any(|&(_, s)| s == p)) {
                    let taken = |p: u16| p == pid || p == pmt.pcr_pid || pmt.streams.iter().any(|&(_, s)| s == p);
                    self.pid = (0x100..0x1fff).find(|&p| !taken(p));
                }
            }
        }
    }

    /// A PMT packet of the picture with the audio stream named in it; `None`
    /// for any other packet, or before the audio's stream type is known.
    fn with_audio(&self, packet: &Packet, pid: u16) -> Option<Packet> {
        if packet[1] & 0x40 == 0 || Some(packet_pid(packet)) != self.pmt_pid {
            return None;
        }
        let stream_type = self.stream_type?;
        let payload_at = TS_PACKET - ts_payload(packet)?.len();
        let section_at = payload_at + 1 + usize::from(packet[payload_at]);
        let section = packet.get(section_at..)?;
        if section.len() < 12 || section[0] != 0x02 || section[6] != 0 || section[7] != 0 {
            return None;
        }
        let length = usize::from(u16::from(section[1] & 0x0f) << 8 | u16::from(section[2]));
        let body = 3 + length.checked_sub(4)?;
        // The section, the new stream and the CRC in what the packet holds.
        if length < 13 || section_at + body + 5 + 4 > TS_PACKET {
            return None;
        }
        let mut out = *packet;
        let entry = [stream_type, 0xe0 | (pid >> 8) as u8, pid as u8, 0xf0, 0];
        out[section_at + body..section_at + body + 5].copy_from_slice(&entry);
        let length = length + 5;
        out[section_at + 1] = (section[1] & 0xf0) | (length >> 8) as u8;
        out[section_at + 2] = length as u8;
        let crc = crc32(&out[section_at..section_at + body + 5]);
        out[section_at + body + 5..section_at + body + 9].copy_from_slice(&crc.to_be_bytes());
        Some(out)
    }

    /// One audio segment's packets, on `pid`.
    fn audio_packets(&mut self, segment: &[u8], pid: u16, out: &mut Vec<Packet>) -> Result<(), Unsupported> {
        let (timestamp, data) = id3_timestamp(segment);
        if data.len() >= TS_PACKET && data[0] == TS_SYNC && ts_sync(data) == Some(0) {
            self.ts_audio(data, pid, out)
        } else if data.len() >= 7 && data[0] == 0xff && data[1] & 0xf6 == 0xf0 {
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
                source = pmt.streams.into_iter().find(|&(kind, _)| is_audio(kind));
                break;
            }
        }
        let Some((kind, source)) = source else {
            return Err(Unsupported("its audio segments name no audio stream"));
        };
        self.stream_type = Some(kind);
        for packet in packets.iter().filter(|p| p[0] == TS_SYNC && packet_pid(*p) == source) {
            let mut p = *packet;
            p[1] = (p[1] & 0xe0) | (pid >> 8) as u8;
            p[2] = pid as u8;
            if p[3] & 0x10 != 0 {
                p[3] = (p[3] & 0xf0) | self.continuity;
                self.continuity = (self.continuity + 1) & 0x0f;
            } else {
                p[3] = (p[3] & 0xf0) | (self.continuity.wrapping_sub(1) & 0x0f);
            }
            out.push(p);
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
            return Err(Unsupported("its audio segments carry no timestamp"));
        };
        self.stream_type = Some(ADTS_AAC);
        // Samples since `pts`, at `rate`: exact however many frames.
        let (mut samples, mut rate) = (0u64, 0u64);
        while data.len() >= 7 && data[0] == 0xff && data[1] & 0xf6 == 0xf0 {
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
            self.packetize(&pes(&data[..length], at), pid, out);
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
    fn packetize(&mut self, pes: &[u8], pid: u16, out: &mut Vec<Packet>) {
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
            out.push(p);
            self.continuity = (self.continuity + 1) & 0x0f;
            rest = &rest[take..];
            first = false;
        }
    }
}

/// What a PMT section says: the PCR's PID and each stream's type and PID.
struct Pmt {
    pcr_pid: u16,
    streams: Vec<(u8, u16)>,
}

impl Pmt {
    /// From a packet holding the start of a PMT; `None` for one that isn't
    /// whole in it.
    fn read(packet: &[u8]) -> Option<Self> {
        let payload = ts_payload(packet)?;
        let section = payload.get(1 + usize::from(*payload.first()?)..)?;
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
            streams.push((rest[0], pid));
            rest = rest.get(5 + info..)?;
        }
        Some(Self { pcr_pid, streams })
    }
}

fn packet_pid(packet: &[u8]) -> u16 {
    u16::from(packet[1] & 0x1f) << 8 | u16::from(packet[2])
}

/// MPEG-1 and -2 audio, AAC (ADTS and LATM), AC-3 and E-AC-3.
fn is_audio(stream_type: u8) -> bool {
    matches!(stream_type, 0x03 | 0x04 | 0x0f | 0x11 | 0x81 | 0x87)
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

    /// A PMT with PCR on the first stream.
    fn pmt(pid: u16, streams: &[(u8, u16)]) -> Packet {
        let mut body = vec![0, 1, 0xc1, 0, 0, 0xe0 | (streams[0].1 >> 8) as u8, streams[0].1 as u8, 0xf0, 0];
        for &(kind, pid) in streams {
            body.extend_from_slice(&[kind, 0xe0 | (pid >> 8) as u8, pid as u8, 0xf0, 0]);
        }
        packet(pid, true, &section(2, &body))
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

    fn packets(data: &[u8]) -> Vec<Packet> {
        data.as_chunks::<TS_PACKET>().0.to_vec()
    }

    #[test]
    fn packed_aac_goes_into_the_picture_named_in_its_pmt() {
        let video = [pat(0x1000), pmt(0x1000, &[(0x1b, 0x100)]), packet(0x100, true, &[0, 0, 1, 0xe0])].concat();
        let video = [video, packet(0x100, false, &[]).repeat(3)].concat();
        let audio = [id3(90_000), adts(3, &[1; 300]), adts(3, &[2; 10])].concat();
        let mut join = Join::default();
        let out = packets(&join.join(&video, &[audio]).unwrap());

        // The PMT names an audio stream on a free PID, with a good CRC.
        let pmt = Pmt::read(&out[1]).unwrap();
        assert_eq!(pmt.streams, [(0x1b, 0x100), (ADTS_AAC, 0x101)]);
        let section = &out[1][5..5 + 3 + 23];
        assert_eq!(crc32(section), 0);

        // Each frame is a PES packet with its timestamp: 1024 samples at
        // 48 kHz apart. The first spans two packets, the last filled out.
        let sound: Vec<_> = out.iter().filter(|p| packet_pid(*p) == 0x101).collect();
        assert_eq!(sound.len(), 3);
        assert_eq!(sound.iter().map(|p| p[3] & 0x0f).collect::<Vec<_>>(), [0, 1, 2]);
        assert_eq!(&sound[0][4..8], &[0, 0, 1, AUDIO_STREAM_ID]);
        assert_eq!(read_pts(&sound[0][13..18]), 90_000);
        assert_eq!(sound[1][1] & 0x40, 0);
        let last = sound[2];
        assert_eq!(last[3] & 0x30, 0x30);
        let pes = &last[5 + usize::from(last[4])..];
        assert_eq!(read_pts(&pes[9..14]), 90_000 + 1920);
        assert_eq!(&pes[14..21], &adts(3, &[2; 10])[..7]);

        // Spread among the picture's packets after its tables, the
        // picture's order kept.
        let at: Vec<_> = out.iter().map(|p| packet_pid(p)).collect();
        assert_eq!(at, [0, 0x1000, 0x101, 0x100, 0x100, 0x101, 0x100, 0x100, 0x101]);
        assert_eq!(out[3], packets(&video)[2]);

        // A segment without a timestamp follows on from the last.
        let out = packets(&join.join(&video, &[adts(3, &[3; 10])]).unwrap());
        let sound: Vec<_> = out.iter().filter(|p| packet_pid(*p) == 0x101).collect();
        assert_eq!(sound[0][3] & 0x0f, 3);
        let pes = &sound[0][5 + usize::from(sound[0][4])..];
        assert_eq!(read_pts(&pes[9..14]), 90_000 + 2 * 1920);
    }

    #[test]
    fn audio_in_mpeg_ts_moves_to_a_pid_of_its_own() {
        let video = [pat(0x1000), pmt(0x1000, &[(0x1b, 0x100), (0x15, 0x101)])].concat();
        // The audio's stream uses the picture's PID.
        let audio = [pat(0x1000), pmt(0x1000, &[(0x0f, 0x100)]), packet(0x100, true, &[0, 0, 1, 0xc0])].concat();
        let mut join = Join::default();
        let out = packets(&join.join(&video, &[audio]).unwrap());
        assert_eq!(Pmt::read(&out[1]).unwrap().streams, [(0x1b, 0x100), (0x15, 0x101), (0x0f, 0x102)]);
        assert_eq!(out.len(), 3);
        assert_eq!(packet_pid(&out[2]), 0x102);
        assert_eq!(&out[2][4..8], &[0, 0, 1, 0xc0]);
    }

    #[test]
    fn a_picture_with_sound_of_its_own_keeps_it() {
        let video = [pat(0x1000), pmt(0x1000, &[(0x1b, 0x100), (0x0f, 0x101)])].concat();
        let mut join = Join::default();
        assert_eq!(join.join(&video, &[adts(3, &[0; 4])]).unwrap(), video);
        assert!(!join.wanted());
    }

    #[test]
    fn audio_that_cant_be_put_in_is_said() {
        let video = [pat(0x1000), pmt(0x1000, &[(0x1b, 0x100)])].concat();
        let mut join = Join::default();
        assert!(join.join(&video, &[b"not audio".to_vec()]).is_err());
        // Packed AAC needs a timestamp to start from.
        assert!(join.join(&video, &[adts(3, &[0; 4])]).is_err());
    }
}

//! A Matroska file's head (EBML, the Segment's Info, Tracks and Chapters),
//! read for a file played from a drive, which no server has probed: the
//! player's audio and subtitle choices, its HDR and its chapters. Elements
//! the head doesn't hold are found through the SeekHead and read where it
//! says (FFmpeg writes Chapters after the clusters).

use std::collections::HashMap;

use pley_core::model::{Chapter, DynamicRange, MediaStream, StreamKind};

use crate::ebml::{self, El, children, element, float, text, uint};
use crate::hevc;
use crate::{LocalFacts, ReadAt};

const EBML: u64 = 0x1A45DFA3;
const SEGMENT: u64 = 0x18538067;
const SEEK_HEAD: u64 = 0x114D9B74;
const SEEK: u64 = 0x4DBB;
const SEEK_ID: u64 = 0x53AB;
const SEEK_POSITION: u64 = 0x53AC;
const INFO: u64 = 0x1549A966;
const TIMECODE_SCALE: u64 = 0x2AD7B1;
const DURATION: u64 = 0x4489;
const TRACKS: u64 = 0x1654AE6B;
const TRACK_ENTRY: u64 = 0xAE;
const TRACK_NUMBER: u64 = 0xD7;
const TRACK_TYPE: u64 = 0x83;
const FLAG_DEFAULT: u64 = 0x88;
const FLAG_FORCED: u64 = 0x55AA;
const CODEC_ID: u64 = 0x86;
const CODEC_PRIVATE: u64 = 0x63A2;
const LANGUAGE: u64 = 0x22B59C;
const LANGUAGE_IETF: u64 = 0x22B59D;
const NAME: u64 = 0x536E;
const VIDEO: u64 = 0xE0;
const PIXEL_WIDTH: u64 = 0xB0;
const PIXEL_HEIGHT: u64 = 0xBA;
const COLOUR: u64 = 0x55B0;
const MATRIX_COEFFICIENTS: u64 = 0x55B1;
const BITS_PER_CHANNEL: u64 = 0x55B2;
const TRANSFER_CHARACTERISTICS: u64 = 0x55BA;
const PRIMARIES: u64 = 0x55BB;
const BLOCK_ADDITION_MAPPING: u64 = 0x41E4;
const BLOCK_ADD_ID_TYPE: u64 = 0x41E7;
const BLOCK_ADD_ID_EXTRA_DATA: u64 = 0x41ED;
const CONTENT_ENCODINGS: u64 = 0x6D80;
const CONTENT_ENCODING: u64 = 0x6240;
const CONTENT_COMPRESSION: u64 = 0x5034;
const CONTENT_COMP_ALGO: u64 = 0x4254;
const CONTENT_COMP_SETTINGS: u64 = 0x4255;
const AUDIO: u64 = 0xE1;
const CHANNELS: u64 = 0x9F;
const SAMPLING_FREQUENCY: u64 = 0xB5;
pub(crate) const CUES: u64 = 0x1C53BB6B;
const CHAPTERS: u64 = 0x1043A770;
const EDITION_ENTRY: u64 = 0x45B9;
const CHAPTER_ATOM: u64 = 0xB6;
const CHAPTER_TIME_START: u64 = 0x91;
const CHAPTER_TIME_END: u64 = 0x92;
const CHAPTER_DISPLAY: u64 = 0x80;
const CHAP_STRING: u64 = 0x85;
pub(crate) const CLUSTER: u64 = 0x1F43B675;

/// Dolby Vision's configuration records, as BlockAddIdType names them.
const DVCC: u64 = 0x64766343;
const DVVC: u64 = 0x64767643;
/// ITU-T T.35 in BlockAddIdType: HDR10+'s dynamic metadata, as mkvmerge marks it.
const ITU_T35: u64 = 4;

/// The first frames read for what they say of the picture: a few, whatever
/// their size.
pub const FRAMES_MOST: usize = 4 * 1024 * 1024;
/// The most of an element read from where the SeekHead points: Tracks and
/// Chapters are a few KB.
pub const ELEMENT_MOST: usize = 1024 * 1024;
/// The most tracks and chapters kept: a film has a few dozen of each.
const TRACKS_MOST: usize = 256;
const CHAPTERS_MOST: usize = 1000;

/// A track as the file numbers and codes it: its blocks name its number;
/// their bytes may be compressed (`comp_algo` 0: zlib, 3: a stripped header).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MatroskaTrack {
    /// The stream's index in [`LocalFacts::streams`].
    pub index: i64,
    pub number: u64,
    /// `S_TEXT/UTF8`, `S_HDMV/PGS`…
    pub codec_id: String,
    pub private: Option<Vec<u8>>,
    pub comp_algo: Option<u64>,
    pub comp_settings: Option<Vec<u8>>,
}

/// An element found, copied out of the buffer it was read in.
struct Found {
    bytes: Vec<u8>,
    start: usize,
    end: usize,
}

/// The facts of a Matroska file: `head` is its first bytes, `file` the whole
/// file for what lies further. None when it isn't Matroska.
pub fn read<R: ReadAt + ?Sized>(head: &[u8], file: &R) -> Option<LocalFacts> {
    let ebml = element(head, 0, head.len())?;
    if ebml.id != EBML || ebml.size.is_none() {
        return None;
    }
    let segment = element(head, ebml.end(), head.len())?;
    if segment.id != SEGMENT {
        return None;
    }
    let segment_data = segment.data_start;
    let mut found: HashMap<u64, Found> = HashMap::new();
    let mut seeks: HashMap<u64, u64> = HashMap::new();
    let mut first_cluster = None;
    let mut at = segment_data;
    while at < head.len() {
        let Some(e) = element(head, at, head.len()) else { break };
        if e.id == CLUSTER {
            first_cluster = Some(at as u64);
            break;
        }
        if e.size.is_none() {
            break;
        }
        let end = e.end().min(head.len());
        if e.id == SEEK_HEAD {
            read_seeks(head, e.data_start, end, &mut seeks);
        } else if matches!(e.id, INFO | TRACKS | CHAPTERS) && e.end() <= head.len() {
            found.insert(e.id, Found { bytes: ebml::data(head, &e).to_vec(), start: 0, end: e.end() - e.data_start });
        }
        at = e.end();
    }
    // Past the head: where the SeekHead says.
    for id in [INFO, TRACKS, CHAPTERS] {
        if found.contains_key(&id) {
            continue;
        }
        let Some(position) = seeks.get(&id).and_then(|p| p.checked_add(segment_data as u64)) else { continue };
        let bytes = file.read_at(position, ELEMENT_MOST);
        let Some(e) = element(&bytes, 0, bytes.len()) else { continue };
        if e.id != id || e.size.is_none() || e.end() > bytes.len() {
            continue;
        }
        let (start, end) = (e.data_start, e.end());
        found.insert(id, Found { bytes, start, end });
    }

    let mut scale_ns: u64 = 1_000_000;
    let mut duration_ms = None;
    if let Some(info) = found.get(&INFO) {
        let mut duration = None;
        for c in children(&info.bytes, info.start, info.end) {
            match c.id {
                TIMECODE_SCALE => scale_ns = uint(&info.bytes, &c).max(1),
                DURATION => duration = float(&info.bytes, &c),
                _ => {}
            }
        }
        if let Some(d) = duration.filter(|d| d.is_finite() && *d > 0.0) {
            // A float to an integer saturates in Rust.
            duration_ms = Some((d * scale_ns as f64 / 1_000_000.0) as i64);
        }
    }
    let mut tracks = Vec::new();
    let mut streams = match found.get(&TRACKS) {
        Some(t) => read_tracks(&t.bytes, t.start, t.end, &mut tracks),
        None => Vec::new(),
    };
    // What the frames say of the picture (HDR10's metadata, Dolby Vision's
    // RPU, HDR10+), which many files' heads leave out: read from the first
    // cluster, wherever it starts (after a few MB of fonts, in some files).
    if let Some(video) = streams.iter_mut().find(|s| s.kind == StreamKind::Video && s.codec == "hevc") {
        let cluster = first_cluster.or_else(|| seeks.get(&CLUSTER).and_then(|c| c.checked_add(segment_data as u64)));
        if let Some(first) = cluster {
            let frames = file.read_at(first, FRAMES_MOST);
            *video = hevc::apply(video, &hevc::scan(&frames));
        }
    }
    let chapters = match found.get(&CHAPTERS) {
        Some(c) => read_chapters(&c.bytes, c.start, c.end, duration_ms),
        None => Vec::new(),
    };
    Some(LocalFacts {
        duration_ms,
        streams,
        chapters,
        tracks,
        segment_start: segment_data as u64,
        cues_at: seeks.get(&CUES).and_then(|c| c.checked_add(segment_data as u64)),
        scale_ns,
    })
}

fn read_seeks(b: &[u8], from: usize, to: usize, seeks: &mut HashMap<u64, u64>) {
    for seek in children(b, from, to).filter(|s| s.id == SEEK) {
        let (mut id, mut position) = (None, None);
        for c in children(b, seek.data_start, seek.end()) {
            match c.id {
                SEEK_ID => id = Some(uint(b, &c)),
                SEEK_POSITION => position = Some(uint(b, &c)),
                _ => {}
            }
        }
        if let (Some(id), Some(position)) = (id, position) {
            seeks.entry(id).or_insert(position);
        }
    }
}

/// What a TrackEntry says, before it becomes a stream.
struct Entry {
    kind: u64,
    number: u64,
    default: bool,
    forced: bool,
    codec: String,
    language: String,
    ietf: Option<String>,
    name: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    channels: Option<i64>,
    rate: Option<i64>,
    depth: Option<i64>,
    transfer: u64,
    primaries: u64,
    matrix: u64,
    dv: Option<(i64, i64, i64)>,
    hdr10plus: bool,
    private: Option<Vec<u8>>,
    comp_algo: Option<u64>,
    comp_settings: Option<Vec<u8>>,
}

impl Default for Entry {
    fn default() -> Entry {
        Entry {
            kind: 0,
            number: 0,
            // Matroska's defaults: a track is a default one, in English.
            default: true,
            forced: false,
            codec: String::new(),
            language: "eng".into(),
            ietf: None,
            name: None,
            width: None,
            height: None,
            channels: None,
            rate: None,
            depth: None,
            // 2: unspecified.
            transfer: 2,
            primaries: 2,
            matrix: 2,
            dv: None,
            hdr10plus: false,
            private: None,
            comp_algo: None,
            comp_settings: None,
        }
    }
}

fn int(b: &[u8], e: &El) -> Option<i64> {
    i64::try_from(uint(b, e)).ok()
}

fn read_tracks(b: &[u8], from: usize, to: usize, numbered: &mut Vec<MatroskaTrack>) -> Vec<MediaStream> {
    let mut streams = Vec::new();
    for entry in children(b, from, to).filter(|e| e.id == TRACK_ENTRY) {
        if streams.len() >= TRACKS_MOST {
            break;
        }
        let mut t = Entry::default();
        for c in children(b, entry.data_start, entry.end()) {
            match c.id {
                TRACK_NUMBER => t.number = uint(b, &c),
                TRACK_TYPE => t.kind = uint(b, &c),
                FLAG_DEFAULT => t.default = uint(b, &c) != 0,
                FLAG_FORCED => t.forced = uint(b, &c) != 0,
                CODEC_ID => t.codec = text(b, &c),
                CODEC_PRIVATE => t.private = Some(ebml::data(b, &c).to_vec()),
                LANGUAGE => t.language = text(b, &c),
                LANGUAGE_IETF => t.ietf = Some(text(b, &c)),
                NAME => t.name = Some(text(b, &c)),
                CONTENT_ENCODINGS => read_encodings(b, &c, &mut t),
                VIDEO => {
                    for v in children(b, c.data_start, c.end()) {
                        match v.id {
                            PIXEL_WIDTH => t.width = int(b, &v),
                            PIXEL_HEIGHT => t.height = int(b, &v),
                            COLOUR => {
                                for k in children(b, v.data_start, v.end()) {
                                    match k.id {
                                        TRANSFER_CHARACTERISTICS => t.transfer = uint(b, &k),
                                        PRIMARIES => t.primaries = uint(b, &k),
                                        MATRIX_COEFFICIENTS => t.matrix = uint(b, &k),
                                        BITS_PER_CHANNEL => t.depth = int(b, &k),
                                        _ => {}
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                BLOCK_ADDITION_MAPPING => {
                    let mut add_type = 0;
                    let mut extra = None;
                    for m in children(b, c.data_start, c.end()) {
                        match m.id {
                            BLOCK_ADD_ID_TYPE => add_type = uint(b, &m),
                            BLOCK_ADD_ID_EXTRA_DATA => extra = Some(ebml::data(b, &m)),
                            _ => {}
                        }
                    }
                    if add_type == ITU_T35 {
                        t.hdr10plus = true;
                    } else if (add_type == DVCC || add_type == DVVC)
                        && let Some(record) = extra
                    {
                        t.dv = dolby_vision_record(record).or(t.dv);
                    }
                }
                AUDIO => {
                    for a in children(b, c.data_start, c.end()) {
                        match a.id {
                            CHANNELS => t.channels = int(b, &a),
                            SAMPLING_FREQUENCY => t.rate = float(b, &a).map(|f| f as i64),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        let kind = match t.kind {
            1 => StreamKind::Video,
            2 => StreamKind::Audio,
            0x11 => StreamKind::Subtitle,
            _ => continue,
        };
        let index = streams.len() as i64;
        numbered.push(MatroskaTrack {
            index,
            number: t.number,
            codec_id: t.codec.clone(),
            private: t.private.take(),
            comp_algo: t.comp_algo,
            comp_settings: t.comp_settings.take(),
        });
        streams.push(stream(index, kind, t));
    }
    streams
}

fn read_encodings(b: &[u8], encodings: &El, t: &mut Entry) {
    for enc in children(b, encodings.data_start, encodings.end()).filter(|e| e.id == CONTENT_ENCODING) {
        for part in children(b, enc.data_start, enc.end()).filter(|e| e.id == CONTENT_COMPRESSION) {
            // ContentCompAlgo's default is zlib.
            t.comp_algo = Some(0);
            for x in children(b, part.data_start, part.end()) {
                match x.id {
                    CONTENT_COMP_ALGO => t.comp_algo = Some(uint(b, &x)),
                    CONTENT_COMP_SETTINGS => t.comp_settings = Some(ebml::data(b, &x).to_vec()),
                    _ => {}
                }
            }
        }
    }
}

/// Dolby Vision's configuration record (dvcC, dvvC): its profile, level and
/// base layer's compatibility.
pub(crate) fn dolby_vision_record(r: &[u8]) -> Option<(i64, i64, i64)> {
    let &[_, _, a, b, c, ..] = r else { return None };
    Some((i64::from((a >> 1) & 0x7F), i64::from(((a & 1) << 5) | (b >> 3)), i64::from(c >> 4)))
}

/// A colour's codes as ffprobe names them.
pub(crate) fn transfer_name(transfer: u64) -> Option<&'static str> {
    match transfer {
        16 => Some("smpte2084"),
        18 => Some("arib-std-b67"),
        1 => Some("bt709"),
        _ => None,
    }
}

pub(crate) fn primaries_name(primaries: u64) -> Option<&'static str> {
    match primaries {
        9 => Some("bt2020"),
        1 => Some("bt709"),
        _ => None,
    }
}

pub(crate) fn matrix_name(matrix: u64) -> Option<&'static str> {
    match matrix {
        9 => Some("bt2020nc"),
        1 => Some("bt709"),
        _ => None,
    }
}

fn stream(index: i64, kind: StreamKind, t: Entry) -> MediaStream {
    let codec = codec_name(&t.codec);
    let video = kind == StreamKind::Video;
    let range = if !video {
        None
    } else if t.dv.is_some() {
        Some(DynamicRange::DolbyVision)
    } else if t.transfer == 16 {
        Some(if t.hdr10plus { DynamicRange::Hdr10Plus } else { DynamicRange::Hdr10 })
    } else if t.transfer == 18 {
        Some(DynamicRange::Hlg)
    } else {
        Some(DynamicRange::Sdr)
    };
    let (dv_profile, dv_level, dv_bl_compat) = match t.dv {
        Some((p, l, c)) => (Some(p), Some(l), Some(c)),
        None => (None, None, None),
    };
    // Unsaid, Dolby Vision's base layer says it (8.1 and 7: HDR10, 8.4: HLG).
    let transfer = transfer_name(t.transfer).or(match dv_bl_compat {
        Some(1 | 6) => Some("smpte2084"),
        Some(4) => Some("arib-std-b67"),
        _ => None,
    });
    let text_based = matches!(codec.as_str(), "subrip" | "ass" | "ssa" | "webvtt" | "mov_text");
    // "und" says nothing; the IETF tag is the newer, finer name.
    let language =
        ietf_language(t.ietf.as_deref()).or((t.language != "und" && !t.language.is_empty()).then_some(t.language));
    MediaStream {
        index,
        kind,
        language,
        title: t.name.filter(|n| !n.trim().is_empty()),
        default: t.default,
        forced: t.forced,
        width: t.width,
        height: t.height,
        channels: t.channels,
        sample_rate: t.rate.filter(|r| *r > 0),
        bit_depth: if video { t.depth } else { None },
        hdr: video.then(|| range != Some(DynamicRange::Sdr)),
        dynamic_range: range,
        hdr10plus: (video && t.hdr10plus).then_some(true),
        dv_profile,
        dv_level,
        dv_bl_compat,
        color_transfer: if video { transfer.map(Into::into) } else { None },
        color_primaries: if video { primaries_name(t.primaries).map(Into::into) } else { None },
        color_space: if video { matrix_name(t.matrix).map(Into::into) } else { None },
        text_based: (kind == StreamKind::Subtitle).then_some(text_based),
        codec,
        ..MediaStream::default()
    }
}

/// The first edition's chapters, in order; one that doesn't say where it
/// ends ends where the next starts, the last at the film's end.
fn read_chapters(b: &[u8], from: usize, to: usize, duration_ms: Option<i64>) -> Vec<Chapter> {
    let mut chapters = Vec::new();
    if let Some(edition) = children(b, from, to).find(|e| e.id == EDITION_ENTRY) {
        for atom in children(b, edition.data_start, edition.end()).filter(|e| e.id == CHAPTER_ATOM) {
            if chapters.len() >= CHAPTERS_MOST {
                break;
            }
            let (mut start, mut end, mut title) = (0, None, None);
            for c in children(b, atom.data_start, atom.end()) {
                match c.id {
                    // Nanoseconds: anything a u64 holds fits as ms.
                    CHAPTER_TIME_START => start = (uint(b, &c) / 1_000_000) as i64,
                    CHAPTER_TIME_END => end = Some((uint(b, &c) / 1_000_000) as i64),
                    CHAPTER_DISPLAY if title.is_none() => {
                        title = children(b, c.data_start, c.end())
                            .filter(|d| d.id == CHAP_STRING)
                            .map(|d| text(b, &d))
                            .last();
                    }
                    _ => {}
                }
            }
            chapters.push(Chapter {
                start_ms: start,
                end_ms: end.unwrap_or(0),
                title: title.filter(|t| !t.trim().is_empty()),
                thumb_url: None,
            });
        }
    }
    chapters.sort_by_key(|c| c.start_ms);
    for i in 0..chapters.len() {
        let next = chapters.get(i + 1).map(|c| c.start_ms);
        let c = &mut chapters[i];
        if c.end_ms <= c.start_ms {
            c.end_ms = next.or(duration_ms).unwrap_or(c.start_ms);
        }
    }
    chapters
}

/// Matroska's codec id as ffprobe names the codec, which the player's rules
/// know.
pub fn codec_name(id: &str) -> String {
    if id.starts_with("A_AAC") {
        return "aac".into();
    }
    if id.starts_with("A_PCM") {
        return "pcm_s16le".into();
    }
    if id.starts_with("A_DTS") {
        return "dts".into();
    }
    match id {
        "V_MPEGH/ISO/HEVC" => "hevc",
        "V_MPEG4/ISO/AVC" => "h264",
        "V_AV1" => "av1",
        "V_VP9" => "vp9",
        "V_VP8" => "vp8",
        "V_MPEG2" => "mpeg2video",
        "V_MPEG4/ISO/ASP" | "V_MPEG4/ISO/SP" => "mpeg4",
        "V_MS/VFW/FOURCC" => "unknown",
        "A_AC3" => "ac3",
        "A_EAC3" => "eac3",
        "A_TRUEHD" => "truehd",
        "A_FLAC" => "flac",
        "A_OPUS" => "opus",
        "A_VORBIS" => "vorbis",
        "A_MPEG/L3" => "mp3",
        "A_MPEG/L2" => "mp2",
        "S_TEXT/UTF8" | "S_TEXT/ASCII" => "subrip",
        "S_TEXT/ASS" => "ass",
        "S_TEXT/SSA" => "ssa",
        "S_TEXT/WEBVTT" => "webvtt",
        "S_HDMV/PGS" => "hdmv_pgs_subtitle",
        "S_VOBSUB" => "dvd_subtitle",
        "S_DVBSUB" => "dvb_subtitle",
        _ => return id.to_lowercase(),
    }
    .into()
}

/// An IETF tag's language (`en-US`: `en`); none without one.
fn ietf_language(ietf: Option<&str>) -> Option<String> {
    let ietf = ietf.map(str::trim).filter(|t| !t.is_empty() && *t != "und")?;
    Some(match ietf.find('-') {
        Some(dash) if dash > 0 => ietf[..dash].to_string(),
        _ => ietf.to_string(),
    })
}

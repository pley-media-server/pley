//! An MP4 or QuickTime file's movie box (moov), read for a file played from
//! a drive as [`crate::matroska`] reads a Matroska head: its length, its
//! tracks as ffprobe names them (codec, size, colour, Dolby Vision,
//! language, channels, Atmos) and its chapters (Nero's chpl). The moov is
//! found box by box, at the start or the end of the file.

use pley_core::model::{Chapter, DynamicRange, MediaStream, StreamKind};

use crate::hevc;
use crate::matroska::{dolby_vision_record, matrix_name, primaries_name, transfer_name};
use crate::{LocalFacts, ReadAt};

/// The most of a moov read: a film's sample tables are a few MB.
pub const MOOV_MOST: u64 = 32 * 1024 * 1024;
/// The most top-level boxes walked looking for the moov.
const BOXES_MOST: usize = 1000;
/// The first frames read for what they say of the picture.
const FRAMES_MOST: usize = 4 * 1024 * 1024;
/// How deep boxes inside a track are walked (mdia, minf, stbl).
const DEPTH_MOST: usize = 4;
const TRACKS_MOST: usize = 256;
const CHAPTERS_MOST: usize = 1000;

/// The facts of the file; none when it isn't MP4.
pub fn read<R: ReadAt + ?Sized>(file: &R) -> Option<LocalFacts> {
    let length = file.size();
    let mut at: u64 = 0;
    let mut moov = None;
    let mut boxes = 0;
    while at.checked_add(8).is_some_and(|e| e <= length) && boxes < BOXES_MOST {
        boxes += 1;
        let head = file.read_at(at, 16);
        let Some(mut size) = u32_at(&head, 0).map(u64::from) else { break };
        let kind: [u8; 4] = head.get(4..8)?.try_into().ok()?;
        let mut header = 8;
        if size == 1 && head.len() >= 16 {
            size = u64_at(&head, 8)?;
            header = 16;
        } else if size == 0 {
            size = length - at;
        }
        if size < header {
            break;
        }
        if boxes == 1 && !matches!(&kind, b"ftyp" | b"moov" | b"free" | b"wide" | b"mdat") {
            return None;
        }
        if &kind == b"moov" {
            if size > MOOV_MOST {
                return None;
            }
            moov = Some(file.read_at(at + header, (size - header) as usize));
            break;
        }
        let Some(next) = at.checked_add(size) else { break };
        at = next;
    }
    let moov = moov?;

    let (mut timescale, mut duration) = (1000u64, 0u64);
    let mut streams: Vec<MediaStream> = Vec::new();
    let mut chapters = Vec::new();
    let mut first_video_chunk = None;
    for (kind, from, to) in boxes_in(&moov, 0, moov.len()) {
        match &kind {
            b"mvhd" => {
                let v = moov.get(from).copied().unwrap_or(0);
                timescale = u32_at(&moov, from + if v == 1 { 20 } else { 12 }).map_or(0, u64::from);
                duration = if v == 1 { u64_at(&moov, from + 24) } else { u32_at(&moov, from + 16).map(u64::from) }
                    .unwrap_or(0);
            }
            b"trak" if streams.len() < TRACKS_MOST => {
                if let Some((stream, chunk)) = track(&moov, from, to, streams.len() as i64) {
                    if stream.kind == StreamKind::Video && first_video_chunk.is_none() {
                        first_video_chunk = chunk;
                    }
                    streams.push(stream);
                }
            }
            b"udta" => chapters = read_chapters(&moov, from, to),
            _ => {}
        }
    }
    let duration_ms = (timescale > 0 && duration > 0)
        .then(|| u128::from(duration) * 1000 / u128::from(timescale))
        .and_then(|ms| i64::try_from(ms).ok());
    // What the frames say of the picture, which the movie box may leave out:
    // from the first chunk.
    if let Some(video) = streams.iter_mut().find(|s| s.kind == StreamKind::Video && s.codec == "hevc")
        && let Some(chunk) = first_video_chunk
    {
        *video = hevc::apply(video, &hevc::scan(&file.read_at(chunk, FRAMES_MOST)));
    }
    for i in 0..chapters.len() {
        let next = chapters.get(i + 1).map(|c: &Chapter| c.start_ms);
        let c = &mut chapters[i];
        c.end_ms = next.or(duration_ms).unwrap_or(c.start_ms);
    }
    // The first of each kind is the one a file starts with.
    for kind in [StreamKind::Video, StreamKind::Audio] {
        if let Some(first) = streams.iter_mut().find(|s| s.kind == kind) {
            first.default = true;
        }
    }
    Some(LocalFacts { duration_ms, streams, chapters, ..LocalFacts::default() })
}

/// What a track's boxes say.
#[derive(Default)]
struct Track {
    handler: Option<[u8; 4]>,
    codec: Option<String>,
    language: Option<String>,
    name: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    channels: Option<i64>,
    rate: Option<i64>,
    colour: Option<(u64, u64, u64)>,
    dv: Option<(i64, i64, i64)>,
    atmos: bool,
    chunk: Option<u64>,
}

/// A trak box's stream, and where its first chunk is.
fn track(b: &[u8], from: usize, to: usize, index: i64) -> Option<(MediaStream, Option<u64>)> {
    let mut t = Track::default();
    walk(b, from, to, 0, &mut t);
    let kind = match t.handler.as_ref() {
        Some(b"vide") => StreamKind::Video,
        Some(b"soun") => StreamKind::Audio,
        Some(b"sbtl" | b"text" | b"subt") => StreamKind::Subtitle,
        _ => return None,
    };
    let codec = codec_name(t.codec.as_deref()?);
    let video = kind == StreamKind::Video;
    let (primaries, mut transfer, matrix) = t.colour.unwrap_or((2, 2, 2));
    if video && t.dv.is_some() && transfer == 2 {
        transfer = match t.dv.map(|d| d.2) {
            Some(1 | 6) => 16,
            Some(4) => 18,
            _ => 2,
        };
    }
    let range = if !video {
        None
    } else if t.dv.is_some() {
        Some(DynamicRange::DolbyVision)
    } else if transfer == 16 {
        Some(DynamicRange::Hdr10)
    } else if transfer == 18 {
        Some(DynamicRange::Hlg)
    } else {
        Some(DynamicRange::Sdr)
    };
    let stream = MediaStream {
        index,
        kind,
        codec,
        language: t.language,
        title: t.name,
        width: t.width,
        height: t.height,
        channels: t.channels,
        sample_rate: t.rate.filter(|r| *r > 0),
        profile: t.atmos.then(|| "Dolby Digital Plus + Dolby Atmos".into()),
        hdr: video.then(|| range != Some(DynamicRange::Sdr)),
        dynamic_range: range,
        dv_profile: t.dv.map(|d| d.0),
        dv_level: t.dv.map(|d| d.1),
        dv_bl_compat: t.dv.map(|d| d.2),
        color_transfer: if video { transfer_name(transfer).map(Into::into) } else { None },
        color_primaries: if video { primaries_name(primaries).map(Into::into) } else { None },
        color_space: if video { matrix_name(matrix).map(Into::into) } else { None },
        text_based: (kind == StreamKind::Subtitle).then_some(true),
        ..MediaStream::default()
    };
    Some((stream, t.chunk))
}

fn walk(b: &[u8], from: usize, to: usize, depth: usize, t: &mut Track) {
    if depth > DEPTH_MOST {
        return;
    }
    for (kind, s, e) in boxes_in(b, from, to) {
        match &kind {
            b"mdia" | b"minf" | b"stbl" => walk(b, s, e, depth + 1, t),
            b"mdhd" => {
                // Its language: three letters, five bits each, after the times.
                let at = s + if b.get(s) == Some(&1) { 32 } else { 20 };
                if at + 2 <= e
                    && let Some(packed) = u16_at(b, at)
                {
                    let code: String =
                        [10, 5, 0].iter().map(|shift| char::from(((packed >> shift) & 31) as u8 + 0x60)).collect();
                    if code != "und" && code.chars().all(|c| c.is_ascii_lowercase()) {
                        t.language = Some(code);
                    }
                }
            }
            b"hdlr" => {
                if s + 12 <= e {
                    t.handler = b.get(s + 8..s + 12).and_then(|h| h.try_into().ok());
                }
                if e > s + 24 {
                    let label = String::from_utf8_lossy(b.get(s + 24..e).unwrap_or_default());
                    let label = label.trim_matches(|c| c == '\0' || c == ' ');
                    // Muxers' own labels ("SoundHandler", "VideoHandler") aren't titles.
                    if !label.is_empty() && !label.ends_with("Handler") && !label.contains("Media Handler") {
                        t.name = Some(label.to_string());
                    }
                }
            }
            b"stsd" => sample_entry(b, s, e, t),
            b"stco" if t.chunk.is_none() && s + 12 <= e && u32_at(b, s + 4).unwrap_or(0) > 0 => {
                t.chunk = u32_at(b, s + 8).map(u64::from);
            }
            b"co64" if t.chunk.is_none() && s + 16 <= e && u32_at(b, s + 4).unwrap_or(0) > 0 => {
                t.chunk = u64_at(b, s + 8);
            }
            _ => {}
        }
    }
}

/// The stsd's first entry: its codec, and for video its size, colour and
/// Dolby Vision, for sound its channels, rate and Atmos.
fn sample_entry(b: &[u8], s: usize, e: usize, t: &mut Track) {
    if s + 16 > e {
        return;
    }
    let entry = s + 8;
    let Some(entry_size) = u32_at(b, entry) else { return };
    t.codec = b.get(entry + 4..entry + 8).map(|c| String::from_utf8_lossy(c).into_owned());
    let entry_end = e.min(entry.saturating_add(entry_size as usize));
    let u16_or_0 = |at: usize| u16_at(b, at).map_or(0, i64::from);
    match t.handler.as_ref() {
        Some(b"vide") if entry + 8 + 78 <= entry_end => {
            t.width = Some(u16_or_0(entry + 32));
            t.height = Some(u16_or_0(entry + 34));
            for (kind, cs, ce) in boxes_in(b, entry + 86, entry_end) {
                match &kind {
                    b"colr" if cs + 10 <= ce && b.get(cs..cs + 4) == Some(b"nclx") => {
                        let code = |at: usize| u16_at(b, at).map_or(2, u64::from);
                        t.colour = Some((code(cs + 4), code(cs + 6), code(cs + 8)));
                    }
                    b"dvcC" | b"dvvC" => {
                        if let Some(record) = b.get(cs..ce) {
                            t.dv = dolby_vision_record(record).or(t.dv);
                        }
                    }
                    _ => {}
                }
            }
        }
        Some(b"soun") if entry + 8 + 28 <= entry_end => {
            t.channels = Some(u16_or_0(entry + 24));
            t.rate = Some(u16_or_0(entry + 32));
            for (kind, cs, ce) in boxes_in(b, entry + 36, entry_end) {
                match &kind {
                    b"dec3" => t.atmos = joc(b.get(cs..ce).unwrap_or_default()),
                    b"dac3" if cs + 3 <= ce => {
                        if let Some(&x) = b.get(cs + 1) {
                            t.channels = Some(ac3_channels((x >> 3) & 7, (x >> 2) & 1));
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

/// E-AC-3's dec3 box: whether it carries Atmos (JOC), the flag after its
/// substreams.
fn joc(b: &[u8]) -> bool {
    let Some(&count) = b.get(1) else { return false };
    let substreams = usize::from(count & 7) + 1;
    let mut at = 2;
    for _ in 0..substreams {
        let Some(&x) = b.get(at + 2) else { break };
        let dependents = (x >> 1) & 15;
        at += if dependents > 0 { 4 } else { 3 };
    }
    at + 2 <= b.len() && b.get(at).is_some_and(|x| x & 1 == 1)
}

fn ac3_channels(acmod: u8, lfe: u8) -> i64 {
    const FRONT_AND_BACK: [i64; 8] = [2, 1, 2, 3, 3, 4, 4, 5];
    FRONT_AND_BACK.get(usize::from(acmod)).copied().unwrap_or(2) + i64::from(lfe)
}

/// Nero's chapters (udta/chpl): each start, in 100 ns, and its title.
fn read_chapters(b: &[u8], from: usize, to: usize) -> Vec<Chapter> {
    let mut chapters = Vec::new();
    for (kind, s, e) in boxes_in(b, from, to) {
        if &kind != b"chpl" || s + 9 > e {
            continue;
        }
        let mut at = s + 4 + if b.get(s) == Some(&1) { 4 } else { 0 };
        let Some(&count) = b.get(at) else { continue };
        at += 1;
        for _ in 0..count {
            if at + 9 > e || chapters.len() >= CHAPTERS_MOST {
                break;
            }
            let start = u64_at(b, at).unwrap_or(0) / 10_000;
            let length = usize::from(b.get(at + 8).copied().unwrap_or(0));
            at += 9;
            let Some(title) = b.get(at..at + length).filter(|_| at + length <= e) else { break };
            let title = String::from_utf8_lossy(title).into_owned();
            at += length;
            chapters.push(Chapter {
                start_ms: start as i64,
                end_ms: 0,
                title: (!title.trim().is_empty()).then_some(title),
                thumb_url: None,
            });
        }
    }
    chapters.sort_by_key(|c| c.start_ms);
    chapters
}

/// An MP4 sample entry's code as ffprobe names the codec.
pub fn codec_name(fourcc: &str) -> String {
    match fourcc {
        "hvc1" | "hev1" | "dvh1" | "dvhe" => "hevc",
        "avc1" | "avc3" | "dva1" | "dvav" => "h264",
        "av01" | "dav1" => "av1",
        "vp09" => "vp9",
        "mp4v" => "mpeg4",
        "mp4a" => "aac",
        "ac-3" => "ac3",
        "ec-3" => "eac3",
        "ac-4" => "ac4",
        "Opus" => "opus",
        "fLaC" => "flac",
        ".mp3" => "mp3",
        "dtsc" | "dtsh" | "dtsl" | "dtse" => "dts",
        "mlpa" => "truehd",
        "tx3g" => "mov_text",
        "wvtt" => "webvtt",
        _ => return fourcc.trim().to_lowercase(),
    }
    .into()
}

/// The boxes between `from` and `to`: each one's type and its content's
/// bounds.
fn boxes_in(b: &[u8], from: usize, to: usize) -> impl Iterator<Item = ([u8; 4], usize, usize)> + '_ {
    let to = to.min(b.len());
    let mut at = from;
    std::iter::from_fn(move || {
        if at.checked_add(8)? > to {
            return None;
        }
        let mut size = u64::from(u32_at(b, at)?);
        let mut header = 8;
        if size == 1 && at + 16 <= to {
            size = u64_at(b, at + 8)?;
            header = 16;
        } else if size == 0 {
            size = (to - at) as u64;
        }
        let end = usize::try_from(size).ok().and_then(|s| at.checked_add(s)).filter(|e| *e <= to);
        let Some(end) = end.filter(|_| size >= header as u64) else {
            at = to;
            return None;
        };
        let kind = b.get(at + 4..at + 8)?.try_into().ok()?;
        let start = at + header;
        at = end;
        Some((kind, start, end))
    })
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_be_bytes(b.get(at..at.checked_add(8)?)?.try_into().ok()?))
}

//! Videos on a drive plugged into the TV: their folders, heads, subtitle
//! files and resume points, as the Samsung app's tests have them
//! (apps/tizen-native/test/Pley.Tv.Core.Tests/LocalTests.cs).

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use pley_core::device::scratch_dir;
use pley_core::format::{is_atmos, stream_range};
use pley_core::model::{
    DynamicRange, PlaybackInfo, PlaybackMethod, PlaybackMode, PlaybackRequest, ProgressReport, StreamKind,
};
use pley_core::{ApiError, Store};
use pley_player::server::PlayerServer;
use pley_subs::parse_vtt;

use super::*;

// ---- a Matroska file, built element by element

/// An element with an 8-byte size: any length fits.
fn el(id: u64, parts: &[Vec<u8>]) -> Vec<u8> {
    let data = parts.concat();
    let mut out: Vec<u8> = id.to_be_bytes().into_iter().skip_while(|b| *b == 0).collect();
    out.push(0x01);
    out.extend(&(data.len() as u64).to_be_bytes()[1..]);
    out.extend(data);
    out
}

fn u(v: u64) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

fn f(v: f64) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

fn s(text: &str) -> Vec<u8> {
    text.as_bytes().to_vec()
}

fn track(kind: u64, codec: &str, language: Option<&str>, name: Option<&str>, default: bool, extra: Vec<u8>) -> Vec<u8> {
    let mut parts =
        vec![el(0xD7, &[u(1)]), el(0x83, &[u(kind)]), el(0x86, &[s(codec)]), el(0x88, &[u(default as u64)])];
    if let Some(language) = language {
        parts.push(el(0x22B59C, &[s(language)]));
    }
    if let Some(name) = name {
        parts.push(el(0x536E, &[s(name)]));
    }
    parts.push(extra);
    el(0xAE, &parts)
}

fn ebml_header() -> Vec<u8> {
    el(0x1A45DFA3, &[el(0x4282, &[s("matroska")])])
}

fn film(chapters_at_end: bool, hdr: bool) -> Vec<u8> {
    let info = el(0x1549A966, &[el(0x2AD7B1, &[u(1_000_000)]), el(0x4489, &[f(7_380_000.0)])]);
    let colour =
        if hdr { el(0x55B0, &[el(0x55BA, &[u(16)]), el(0x55BB, &[u(9)]), el(0x55B2, &[u(10)])]) } else { vec![] };
    let tracks = el(
        0x1654AE6B,
        &[
            track(
                1,
                "V_MPEGH/ISO/HEVC",
                None,
                None,
                true,
                el(0xE0, &[el(0xB0, &[u(1920)]), el(0xBA, &[u(800)]), colour]),
            ),
            track(2, "A_TRUEHD", Some("eng"), Some("Atmos"), true, el(0xE1, &[el(0x9F, &[u(8)])])),
            track(2, "A_AC3", Some("eng"), Some("Compatibility"), false, el(0xE1, &[el(0x9F, &[u(6)])])),
            track(2, "A_AAC", Some("rus"), None, false, vec![]),
            track(0x11, "S_TEXT/UTF8", Some("eng"), None, true, vec![]),
        ],
    );
    let chapters = el(
        0x1043A770,
        &[el(
            0x45B9,
            &[
                el(0xB6, &[el(0x91, &[u(0)]), el(0x80, &[el(0x85, &[s("Opening")])])]),
                el(0xB6, &[el(0x91, &[u(600_000_000_000)]), el(0x80, &[el(0x85, &[s("The Road")])])]),
            ],
        )],
    );
    // A frame with HDR10+'s metadata mark in it.
    let frame = if hdr { el(0xA3, &[hdr10plus_frame()]) } else { vec![] };
    let cluster = el(0x1F43B675, &[el(0xE7, &[u(0)]), frame]);
    if !chapters_at_end {
        return [ebml_header(), el(0x18538067, &[info, tracks, chapters, cluster])].concat();
    }
    // FFmpeg's way: Chapters after the clusters, found through the SeekHead,
    // whose own size is fixed (8-byte sizes), so the chapters' offset can be
    // worked out first.
    let seek_head = |at: u64| el(0x114D9B74, &[el(0x4DBB, &[el(0x53AB, &[u(0x1043A770)]), el(0x53AC, &[u(at)])])]);
    let at = (seek_head(0).len() + info.len() + tracks.len() + cluster.len()) as u64;
    [ebml_header(), el(0x18538067, &[seek_head(at), info, tracks, cluster, chapters])].concat()
}

/// A SimpleBlock's data for track 1 holding one HEVC unit: a prefix SEI with
/// HDR10+'s T.35 message.
fn hdr10plus_frame() -> Vec<u8> {
    let sei = [0x4E, 0x01, 0x04, 0x08, 0xB5, 0x00, 0x3C, 0x00, 0x01, 0x04, 0x01, 0x40, 0x80];
    [&[0x81, 0, 0, 0x80][..], &(sei.len() as u32).to_be_bytes(), &sei].concat()
}

fn read_head(file: &[u8], head_bytes: usize) -> LocalFacts {
    matroska::read(&file[..head_bytes.min(file.len())], file).expect("a Matroska head")
}

#[test]
fn a_matroska_head_gives_its_length_tracks_and_chapters() {
    let facts = read_head(&film(false, false), usize::MAX);
    assert_eq!(facts.duration_ms, Some(7_380_000));
    let kinds: Vec<_> = facts.streams.iter().map(|s| s.kind).collect();
    use StreamKind::*;
    assert_eq!(kinds, [Video, Audio, Audio, Audio, Subtitle]);
    let codecs: Vec<_> = facts.streams.iter().map(|s| s.codec.as_str()).collect();
    assert_eq!(codecs, ["hevc", "truehd", "ac3", "aac", "subrip"]);
    assert_eq!(facts.streams.iter().map(|s| s.index).collect::<Vec<_>>(), [0, 1, 2, 3, 4]);
    assert_eq!((facts.streams[0].width, facts.streams[0].height), (Some(1920), Some(800)));
    assert_eq!(facts.streams[1].channels, Some(8));
    assert_eq!(facts.streams[1].title.as_deref(), Some("Atmos"));
    let defaults: Vec<_> = facts.streams.iter().filter(|s| s.kind == Audio).map(|s| s.default).collect();
    assert_eq!(defaults, [true, false, false]);
    assert_eq!(facts.streams[3].language.as_deref(), Some("rus"));
    let titles: Vec<_> = facts.chapters.iter().map(|c| c.title.as_deref().unwrap_or("")).collect();
    assert_eq!(titles, ["Opening", "The Road"]);
    // A chapter without its end ends at the next one's start, the last at the film's end.
    let times: Vec<_> = facts.chapters.iter().map(|c| (c.start_ms, c.end_ms)).collect();
    assert_eq!(times, [(0, 600_000), (600_000, 7_380_000)]);
    assert_eq!(facts.streams[0].dynamic_range, Some(DynamicRange::Sdr));
    assert_eq!(facts.streams[4].text_based, Some(true));
}

#[test]
fn a_matroska_head_says_hdr10_and_hdr10plus() {
    let facts = read_head(&film(false, true), usize::MAX);
    let video = &facts.streams[0];
    assert_eq!(video.dynamic_range, Some(DynamicRange::Hdr10Plus));
    assert_eq!(video.color_transfer.as_deref(), Some("smpte2084"));
    assert_eq!(video.color_primaries.as_deref(), Some("bt2020"));
    assert_eq!(video.bit_depth, Some(10));
    assert_eq!(video.hdr10plus, Some(true));
    assert_eq!(stream_range(video).as_deref(), Some("HDR10+"));
}

#[test]
fn chapters_after_the_clusters_are_found_through_the_seekhead() {
    let file = film(true, false);
    // The head stops before the chapters.
    let facts = read_head(&file, file.len() - 60);
    let titles: Vec<_> = facts.chapters.iter().map(|c| c.title.as_deref().unwrap_or("")).collect();
    assert_eq!(titles, ["Opening", "The Road"]);
    assert_eq!(facts.streams.len(), 5);
}

#[test]
fn a_file_that_isnt_matroska_reads_as_nothing() {
    let mp4 = b"\0\0\0\x20ftypisom";
    assert_eq!(matroska::read(mp4, &[][..]), None);
}

/// A film with a text track (number 5) of two subtitles, in two clusters the
/// Cues list.
fn subtitled(codec: &str, first: Option<Vec<u8>>, extra: Vec<u8>) -> Vec<u8> {
    let info = el(0x1549A966, &[el(0x2AD7B1, &[u(1_000_000)]), el(0x4489, &[f(60_000.0)])]);
    let entry =
        el(0xAE, &[el(0xD7, &[u(5)]), el(0x83, &[u(0x11)]), el(0x86, &[s(codec)]), el(0x22B59C, &[s("eng")]), extra]);
    let tracks = el(0x1654AE6B, &[entry]);
    // Block: track 5 (0x85), a relative time, flags, the data.
    let block = |rel: i16, data: Vec<u8>| [&[0x85][..], &rel.to_be_bytes(), &[0], &data].concat();
    let timecode1 = el(0xE7, &[u(1_000)]);
    let group = el(0xA0, &[el(0xA1, &[block(500, first.unwrap_or_else(|| s("Hello")))]), el(0x9B, &[u(2_000)])]);
    let cluster1 = el(0x1F43B675, &[timecode1.clone(), group]);
    let cluster2 = el(0x1F43B675, &[el(0xE7, &[u(10_000)]), el(0xA3, &[block(0, s("There"))])]);
    let seek_head = |cues: u64| el(0x114D9B74, &[el(0x4DBB, &[el(0x53AB, &[u(0x1C53BB6B)]), el(0x53AC, &[u(cues)])])]);
    let c1 = (seek_head(0).len() + info.len() + tracks.len()) as u64;
    let c2 = c1 + cluster1.len() as u64;
    let cues_at = c2 + cluster2.len() as u64;
    // Each block sits after its cluster's timecode.
    let rel = timecode1.len() as u64;
    let point = |time: u64, cluster: u64| {
        el(0xBB, &[el(0xB3, &[u(time)]), el(0xB7, &[el(0xF7, &[u(5)]), el(0xF1, &[u(cluster)]), el(0xF0, &[u(rel)])])])
    };
    let cues = el(0x1C53BB6B, &[point(1_500, c1), point(10_000, c2)]);
    [ebml_header(), el(0x18538067, &[seek_head(cues_at), info, tracks, cluster1, cluster2, cues])].concat()
}

fn read_track(file: &[u8], facts: &LocalFacts) -> Vec<SubtitleBlock> {
    let mut seen = Vec::new();
    let count = cues::read(file, facts, &facts.tracks[0], |b| seen.push(b), &AtomicBool::new(false));
    assert_eq!(count, seen.len(), "each block is passed on as it is read");
    assert_eq!(seen, cues::read_all(file, facts, &facts.tracks[0], &AtomicBool::new(false)));
    seen
}

#[test]
fn a_text_track_is_read_out_of_the_file_through_its_cues() {
    let file = subtitled("S_TEXT/UTF8", None, vec![]);
    let facts = read_head(&file, usize::MAX);
    assert!(facts.cues_at.is_some());
    assert_eq!(facts.tracks.len(), 1);
    assert_eq!((facts.tracks[0].number, facts.tracks[0].codec_id.as_str()), (5, "S_TEXT/UTF8"));
    let blocks = read_track(&file, &facts);
    let times: Vec<_> = blocks.iter().map(|b| (b.start_ms, b.duration_ms)).collect();
    assert_eq!(times, [(1_500, Some(2_000)), (10_000, None)]);
    let cues: Vec<_> = parse_vtt(&cues::webvtt(&blocks, "S_TEXT/UTF8"))
        .into_iter()
        .map(|c| (c.plain_text(), c.start_ms, c.end_ms))
        .collect();
    assert_eq!(cues, [("Hello".to_string(), 1_500, 3_500), ("There".to_string(), 10_000, 14_000)]);
}

#[test]
fn an_ass_track_loses_its_fields_and_styling() {
    let file = subtitled("S_TEXT/ASS", Some(s(r"1,0,Default,,0,0,0,,{\i1}Hello{\i0}\Nthere")), vec![]);
    let facts = read_head(&file, usize::MAX);
    let blocks = read_track(&file, &facts);
    let cues = parse_vtt(&cues::webvtt(&blocks, "S_TEXT/ASS"));
    assert_eq!(cues[0].plain_text(), "Hello\nthere");
}

#[test]
fn a_picture_tracks_blocks_are_read_out_as_they_are() {
    let file = subtitled("S_HDMV/PGS", Some(vec![0x16, 0x00, 0x00]), vec![]);
    let facts = read_head(&file, usize::MAX);
    assert_eq!(facts.streams[0].codec, "hdmv_pgs_subtitle");
    assert_eq!(facts.streams[0].text_based, Some(false));
    let blocks = read_track(&file, &facts);
    assert_eq!(blocks[0].data, [0x16, 0x00, 0x00]);
    assert_eq!(blocks.iter().map(|b| b.start_ms).collect::<Vec<_>>(), [1_500, 10_000]);
}

#[test]
fn a_zlib_vobsub_track_is_inflated_and_a_stripped_header_put_back() {
    use std::io::Write;
    let picture = vec![0x00, 0x2A, 0x10, 0x20, 0x30, 0x40, 0x00, 0x00, 0x00, 0x00];
    let mut zlib = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    zlib.write_all(&picture).unwrap();
    let compressed = zlib.finish().unwrap();
    // ContentEncodings › ContentEncoding › ContentCompression, its algorithm left at zlib.
    let zlib_encoding = el(0x6D80, &[el(0x6240, &[el(0x5034, &[])])]);
    let file = subtitled("S_VOBSUB", Some(compressed), [zlib_encoding, el(0x63A2, &[s("size: 720x576\n")])].concat());
    let facts = read_head(&file, usize::MAX);
    assert_eq!(facts.tracks[0].comp_algo, Some(0));
    assert_eq!(facts.tracks[0].private.as_deref(), Some(&b"size: 720x576\n"[..]));
    assert_eq!(read_track(&file, &facts)[0].data, picture);

    let stripped = el(0x6D80, &[el(0x6240, &[el(0x5034, &[el(0x4254, &[u(3)]), el(0x4255, &[vec![0x16]])])])]);
    let file = subtitled("S_HDMV/PGS", Some(vec![0x00, 0x00]), stripped);
    let facts = read_head(&file, usize::MAX);
    assert_eq!(read_track(&file, &facts)[0].data, [0x16, 0x00, 0x00]);
}

#[test]
fn reading_a_track_stops_when_asked() {
    let file = subtitled("S_TEXT/UTF8", None, vec![]);
    let facts = read_head(&file, usize::MAX);
    assert_eq!(cues::read(&file, &facts, &facts.tracks[0], |_| {}, &AtomicBool::new(true)), 0);
}

#[test]
fn a_dolby_vision_file_without_its_colour_says_hdr10plus_when_its_frames_carry_it() {
    // Profile 8.1 (dvcC: version 1.0, profile 8 level 6, base layer 1), no
    // Colour element, HDR10+ in the first frame.
    let dvcc = vec![1, 0, 8 << 1, 6 << 3, 1 << 4, 0, 0, 0];
    let mapping = el(0x41E4, &[el(0x41E7, &[u(0x64766343)]), el(0x41ED, &[dvcc])]);
    let entry = el(0xAE, &[el(0xD7, &[u(1)]), el(0x83, &[u(1)]), el(0x86, &[s("V_MPEGH/ISO/HEVC")]), mapping]);
    let cluster = el(0x1F43B675, &[el(0xE7, &[u(0)]), el(0xA3, &[hdr10plus_frame()])]);
    let file = [ebml_header(), el(0x18538067, &[el(0x1654AE6B, &[entry]), cluster])].concat();
    let facts = read_head(&file, usize::MAX);
    let video = &facts.streams[0];
    assert_eq!(video.dynamic_range, Some(DynamicRange::DolbyVision));
    assert_eq!((video.dv_profile, video.dv_level, video.dv_bl_compat), (Some(8), Some(6), Some(1)));
    assert_eq!(video.color_transfer.as_deref(), Some("smpte2084"));
    assert_eq!(video.hdr10plus, Some(true));
}

#[test]
fn frames_without_a_record_give_the_dolby_vision_profile() {
    // A Dolby Vision RPU (NAL 62, 0x19) and an enhancement layer (NAL 63): profile 7.
    let unit =
        |header: u8, body: &[u8]| [&(body.len() as u32 + 2).to_be_bytes()[..], &[header << 1, 0x01], body].concat();
    let frames = [unit(62, &[0x19, 0x08, 0x09]), unit(63, &[0x01, 0x02, 0x03])].concat();
    let marks = hevc::scan(&frames);
    assert!(marks.dolby_vision && marks.enhancement_layer && marks.any());
    let video = pley_core::model::MediaStream { kind: StreamKind::Video, codec: "hevc".into(), ..Default::default() };
    let seen = hevc::apply(&video, &marks);
    assert_eq!(
        (seen.dynamic_range, seen.dv_profile, seen.dv_bl_compat),
        (Some(DynamicRange::DolbyVision), Some(7), Some(6))
    );
    // An RPU alone, with no HDR10 metadata in the base layer: profile 5.
    let seen = hevc::apply(&video, &hevc::scan(&unit(62, &[0x19, 0x08, 0x09])));
    assert_eq!((seen.dv_profile, seen.dv_bl_compat), (Some(5), Some(0)));
}

// ---- an MP4, box by box

fn mp4_box(kind: &str, parts: &[Vec<u8>]) -> Vec<u8> {
    let body = parts.concat();
    [&((body.len() + 8) as u32).to_be_bytes()[..], kind.as_bytes(), &body].concat()
}

fn be(v: u64, bytes: usize) -> Vec<u8> {
    v.to_be_bytes()[8 - bytes..].to_vec()
}

fn mp4() -> Vec<u8> {
    let ftyp = mp4_box("ftyp", &[s("isom"), be(0, 4)]);
    let mvhd = mp4_box("mvhd", &[be(0, 4), be(0, 4), be(0, 4), be(1000, 4), be(6_000_000, 4), vec![0; 80]]);
    let mdhd = |lang: &str| {
        let l = lang.as_bytes();
        let packed = (u64::from(l[0] - 0x60) << 10) | (u64::from(l[1] - 0x60) << 5) | u64::from(l[2] - 0x60);
        mp4_box("mdhd", &[be(0, 4), be(0, 4), be(0, 4), be(1000, 4), be(6_000_000, 4), be(packed, 2), be(0, 2)])
    };
    let hdlr = |kind: &str| mp4_box("hdlr", &[be(0, 4), be(0, 4), s(kind), vec![0; 12], vec![0]]);
    let stsd = |entry: Vec<u8>| mp4_box("stsd", &[be(0, 4), be(1, 4), entry]);
    let trak = |lang: &str, kind: &str, entry: Vec<u8>| {
        mp4_box(
            "trak",
            &[mp4_box("mdia", &[mdhd(lang), hdlr(kind), mp4_box("minf", &[mp4_box("stbl", &[stsd(entry)])])])],
        )
    };
    // A video entry: 8 reserved and reference, 16 more, its size, 50 more, then its boxes.
    let colr = mp4_box("colr", &[s("nclx"), be(9, 2), be(16, 2), be(9, 2), vec![0]]);
    let hvc1 = mp4_box("hvc1", &[vec![0; 8], vec![0; 16], be(3840, 2), be(1600, 2), vec![0; 50], colr]);
    // An audio entry: E-AC-3 5.1 with Atmos (dec3: one substream, no dependents, then the JOC flag).
    let dec3 = mp4_box("dec3", &[vec![0x0C, 0x00, 0x20, 0x0F, 0x00, 0x01, 0x10]]);
    let ec3 = mp4_box("ec-3", &[vec![0; 8], vec![0; 8], be(6, 2), be(16, 2), vec![0; 4], be(48000, 2), be(0, 2), dec3]);
    // Nero's chapters: two, at 0 and 10 minutes, in 100 ns.
    let chapter = |start_ms: u64, title: &str| [be(start_ms * 10_000, 8), vec![title.len() as u8], s(title)].concat();
    let chpl = mp4_box(
        "chpl",
        &[vec![1, 0, 0, 0], vec![0; 4], vec![2], chapter(0, "Andy's Room"), chapter(600_000, "Pizza Planet")],
    );
    let udta = mp4_box("udta", &[chpl]);
    let mdat = mp4_box("mdat", &[vec![0; 64]]);
    // Not fast-started: the movie box after the media.
    let moov = mp4_box("moov", &[mvhd, trak("und", "vide", hvc1), trak("eng", "soun", ec3), udta]);
    [ftyp, mdat, moov].concat()
}

#[test]
fn an_mp4_movie_box_gives_its_length_tracks_hdr_atmos_and_chapters() {
    let file = mp4();
    let facts = mp4::read(&file).expect("an MP4 movie box");
    assert_eq!(facts.duration_ms, Some(6_000_000));
    assert_eq!(facts.streams.iter().map(|s| s.kind).collect::<Vec<_>>(), [StreamKind::Video, StreamKind::Audio]);
    let video = &facts.streams[0];
    assert_eq!((video.codec.as_str(), video.width, video.height), ("hevc", Some(3840), Some(1600)));
    assert_eq!(video.dynamic_range, Some(DynamicRange::Hdr10));
    assert_eq!(
        (video.color_transfer.as_deref(), video.color_primaries.as_deref()),
        (Some("smpte2084"), Some("bt2020"))
    );
    assert_eq!(video.language, None);
    let sound = &facts.streams[1];
    assert_eq!((sound.codec.as_str(), sound.language.as_deref(), sound.channels), ("eac3", Some("eng"), Some(6)));
    assert!(is_atmos(sound));
    assert!(video.default && sound.default);
    let chapters: Vec<_> =
        facts.chapters.iter().map(|c| (c.title.as_deref().unwrap_or(""), c.start_ms, c.end_ms)).collect();
    assert_eq!(chapters, [("Andy's Room", 0, 600_000), ("Pizza Planet", 600_000, 6_000_000)]);
    // Whatever the file is called, its head says what it is.
    assert_eq!(crate::facts(&file), Some(facts.clone()));
}

// ---- untrusted bytes

/// Bytes that look random, the same every run.
fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed;
    (0..len)
        .map(|_| {
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (x >> 33) as u8
        })
        .collect()
}

#[test]
fn a_file_cut_anywhere_or_full_of_noise_reads_as_less_never_a_panic() {
    let never = AtomicBool::new(false);
    for file in [film(true, true), subtitled("S_TEXT/ASS", None, vec![]), mp4()] {
        for cut in 0..file.len() {
            let part = &file[..cut];
            if let Some(facts) = facts(part) {
                for track in &facts.tracks {
                    let blocks = cues::read_all(part, &facts, track, &never);
                    cues::webvtt(&blocks, &track.codec_id);
                }
            }
            hevc::scan(part);
        }
        // A byte changed anywhere.
        for at in 0..file.len() {
            let mut bent = file.clone();
            bent[at] ^= 0xA5;
            if let Some(facts) = facts(&bent) {
                for track in &facts.tracks {
                    cues::read(&bent, &facts, track, |_| {}, &never);
                }
            }
        }
    }
    for seed in 0..64 {
        let bytes = noise(16 * 1024, seed);
        assert_eq!(matroska::read(&bytes, &bytes), None);
        mp4::read(&bytes);
        hevc::scan(&bytes);
        let facts = LocalFacts { cues_at: Some(0), ..LocalFacts::default() };
        let track = MatroskaTrack { comp_algo: Some(0), ..MatroskaTrack::default() };
        cues::read(&bytes, &facts, &track, |_| {}, &never);
        folder::decode(&bytes);
        folder::to_webvtt(&String::from_utf8_lossy(&bytes));
    }
}

#[test]
fn boxes_nested_deep_or_sizes_too_large_are_left_alone() {
    // A track nested a hundred thousand boxes deep reads as no track.
    let hdlr = mp4_box("hdlr", &[be(0, 4), be(0, 4), s("vide")]);
    let depth = 100_000;
    let mut inner = Vec::with_capacity(depth * 8 + hdlr.len());
    for level in (1..=depth).rev() {
        inner.extend(((level * 8 + hdlr.len()) as u32).to_be_bytes());
        inner.extend(b"mdia");
    }
    inner.extend(hdlr);
    let file = [mp4_box("ftyp", &[s("isom")]), mp4_box("moov", &[mp4_box("trak", &[inner])])].concat();
    assert_eq!(mp4::read(&file).map(|f| f.streams.len()), Some(0));
    // A moov that says it is 4 GB is refused rather than read.
    let huge = [mp4_box("ftyp", &[s("isom")]), (u32::MAX).to_be_bytes().to_vec(), s("moov")].concat();
    assert_eq!(mp4::read(&huge), None);
    // An element that says it is longer than the file isn't read past it.
    let long = [ebml_header(), vec![0x18, 0x53, 0x80, 0x67, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE]].concat();
    let facts = matroska::read(&long, &long).expect("a Segment, if an empty one");
    assert!(facts.streams.is_empty());
}

#[test]
fn bytes_that_only_look_like_hevc_units_are_scanned_in_bounded_time() {
    // Every sixth byte starts what looks like a 1 MB SEI.
    let bytes: Vec<u8> = [0x00, 0x0F, 0xFF, 0xFF, 0x4E, 0x01].iter().copied().cycle().take(4 * 1024 * 1024).collect();
    let started = std::time::Instant::now();
    assert!(!hevc::scan(&bytes).any());
    assert!(started.elapsed() < std::time::Duration::from_secs(20));
}

// ---- folders

#[test]
fn names_sort_as_a_person_sorts_them() {
    let mut names = vec!["Part 10.mkv", "part 2.mkv", "Part 1.mkv", "Alpha.mkv", "Part 007.mkv"];
    names.sort_by(|a, b| folder::natural(a, b));
    assert_eq!(names, ["Alpha.mkv", "Part 1.mkv", "part 2.mkv", "Part 007.mkv", "Part 10.mkv"]);
}

#[test]
fn hidden_and_system_entries_are_left_out() {
    for (name, shown) in [
        ("$RECYCLE.BIN", false),
        (".device_info_8cdc30de", false),
        ("System Volume Information", false),
        ("thumbs.db", false),
        ("Ad Astra", true),
    ] {
        assert_eq!(folder::shown(name), shown, "{name}");
    }
}

#[test]
fn a_folder_lists_its_folders_then_its_videos() {
    let dir = scratch_dir("local-folder");
    for d in ["Season 10", "Season 2", "$RECYCLE.BIN"] {
        std::fs::create_dir(dir.join(d)).unwrap();
    }
    for f in ["Film.mkv", "Film.en.srt", "notes.txt", ".hidden.mkv", "Clip.MP4", "Trailer.webm"] {
        std::fs::write(dir.join(f), "x").unwrap();
    }
    let listed = folder::list(&dir).unwrap();
    // Only what plays: subtitle files, notes, hidden files and WebM, which
    // LG's player doesn't list, aren't shown.
    assert_eq!(
        listed.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
        ["Season 2", "Season 10", "Clip.MP4", "Film.mkv"]
    );
    assert_eq!(listed.iter().map(|e| e.folder).collect::<Vec<_>>(), [true, true, false, false]);
    assert_eq!(listed[3].size, 1);
    assert_eq!(listed[3].path, dir.join("Film.mkv"));
    assert_eq!(folder::title(&listed[3].path), "Film");
    assert!(folder::list(&dir.join("gone")).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn subtitle_files_beside_a_video_say_their_language_and_kind() {
    let names = [
        "Ad Astra.mkv",
        "Ad Astra.srt",
        "Ad Astra.en.forced.srt",
        "Ad Astra.English.SDH.srt",
        "Ad Astra.fr.vtt",
        "Ad Astra 2.srt",
        "Other.srt",
        "Ad Astra.ass",
    ];
    let found = folder::sidecars(Path::new("/media/Ad Astra.mkv"), names);
    let files: Vec<_> = found.iter().map(|s| s.path.file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(files, ["Ad Astra.en.forced.srt", "Ad Astra.English.SDH.srt", "Ad Astra.fr.vtt", "Ad Astra.srt"]);
    assert_eq!(
        found.iter().map(|s| s.language.as_deref()).collect::<Vec<_>>(),
        [Some("en"), Some("eng"), Some("fr"), None]
    );
    assert_eq!(found.iter().map(|s| s.forced).collect::<Vec<_>>(), [true, false, false, false]);
    assert_eq!(found[1].title.as_deref(), Some("SDH"));
    assert_eq!(found.iter().map(|s| s.codec).collect::<Vec<_>>(), ["subrip", "subrip", "webvtt", "subrip"]);
    // After the file's own streams, numbered as the server numbers files beside the media.
    let streams = folder::sidecar_streams(&found);
    assert_eq!(streams.iter().map(|s| s.index).collect::<Vec<_>>(), [1000, 1001, 1002, 1003]);
    assert!(streams.iter().all(|s| s.external && s.text_based == Some(true) && s.kind == StreamKind::Subtitle));
}

#[test]
fn subrip_becomes_webvtt_the_app_draws() {
    let vtt = folder::to_webvtt(
        "\u{feff}1\r\n00:00:01,500 --> 00:00:03,000\r\nHello\r\n\r\n2\r\n00:01:02,250 --> 00:01:04,000\r\nThere\r\n",
    );
    assert!(vtt.starts_with("WEBVTT\n\n"));
    let cues = parse_vtt(&vtt);
    assert_eq!(cues.iter().map(|c| c.start_ms).collect::<Vec<_>>(), [1_500, 62_250]);
    assert_eq!(cues[0].plain_text(), "Hello");
    // A WebVTT file stays as it is.
    assert_eq!(
        folder::to_webvtt("WEBVTT\n\n00:01.000 --> 00:02.000\nHi,1\n"),
        "WEBVTT\n\n00:01.000 --> 00:02.000\nHi,1\n"
    );
}

#[test]
fn a_subtitle_file_not_in_utf8_is_read_as_latin1() {
    assert_eq!(folder::decode(&[0x63, 0x61, 0x66, 0xE9]), "café");
    assert_eq!(folder::decode("café".as_bytes()), "café");
    let dir = scratch_dir("local-sidecar");
    let path = dir.join("Film.fr.srt");
    std::fs::write(&path, b"1\n00:00:01,000 --> 00:00:02,000\nCaf\xE9\n").unwrap();
    let cues = parse_vtt(&folder::read_sidecar(&path).unwrap());
    assert_eq!(cues[0].plain_text(), "Café");
    let beside = folder::sidecars_beside(&dir.join("Film.mkv"));
    assert_eq!(beside.iter().map(|s| s.path.clone()).collect::<Vec<_>>(), [path]);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- reading a file

#[test]
fn a_file_is_read_a_stretch_at_a_time() {
    let dir = scratch_dir("local-reader");
    let path = dir.join("Film.mkv");
    let file = film(false, true);
    std::fs::write(&path, &file).unwrap();
    let reader = FileReader::open(&path).unwrap();
    assert_eq!(reader.size(), file.len() as u64);
    assert_eq!(reader.read_at(4, 8), file[4..12]);
    assert_eq!(reader.read_at(file.len() as u64 - 3, 100), file[file.len() - 3..]);
    assert!(reader.read_at(file.len() as u64, 10).is_empty());
    assert!(reader.read_at(u64::MAX, 10).is_empty());
    assert_eq!(facts(&reader).map(|f| f.streams[0].dynamic_range), Some(Some(DynamicRange::Hdr10Plus)));
    drop(reader);
    let _ = std::fs::remove_dir_all(&dir);
}

// ---- where it stopped

#[test]
fn where_a_video_stopped_is_kept_on_the_tv_until_it_was_watched() {
    let store = Store::open(scratch_dir("local-resume")).unwrap();
    let film = resume::file_id(Path::new("Films/Film.mkv"), 4_000_000_000);
    let other = resume::file_id(Path::new("Films/Other.mkv"), 4_000_000_000);
    resume::keep(&store, film, 600_000, Some(7_200_000)).unwrap();
    assert_eq!(resume::kept(&store, film), Some(Kept { position_ms: 600_000, duration_ms: 7_200_000 }));
    assert_eq!(resume::kept(&store, other), None);
    let all = resume::all_kept(&store);
    assert_eq!((all.len(), all.get(&film).map(|k| k.position_ms)), (1, Some(600_000)));
    // A position of 0 changes nothing.
    resume::keep(&store, film, 0, Some(7_200_000)).unwrap();
    assert_eq!(resume::kept(&store, film).map(|k| k.position_ms), Some(600_000));
    // At 95 % it was watched: it starts over next time.
    resume::keep(&store, film, 7_000_000, Some(7_200_000)).unwrap();
    assert_eq!(resume::kept(&store, film), None);
}

#[test]
fn the_newest_500_resume_points_are_kept() {
    let store = Store::open(scratch_dir("local-resume-most")).unwrap();
    for id in 0..=500 {
        resume::keep(&store, id, 1_000, None).unwrap();
    }
    assert_eq!(resume::kept(&store, 0), None, "the oldest went first");
    assert_eq!(resume::kept(&store, 1), Some(Kept { position_ms: 1_000, duration_ms: 0 }));
    // Kept again, it is the newest.
    resume::keep(&store, 1, 2_000, None).unwrap();
    resume::keep(&store, 501, 1_000, None).unwrap();
    assert_eq!(resume::kept(&store, 1).map(|k| k.position_ms), Some(2_000));
    assert_eq!(resume::kept(&store, 2), None);
    let raw: Vec<(i64, i64, i64)> = store.get(pley_core::store::keys::LOCAL_RESUME).unwrap();
    assert_eq!((raw.len(), raw[0]), (500, (501, 1_000, 0)));
}

#[test]
fn the_resume_id_survives_a_new_mount_letter() {
    let one = Path::new("/tmp/usb/sda/sda1/Films/Mad Max.mkv");
    let two = Path::new("/tmp/usb/sdb/sdb1/Films/Mad Max.mkv");
    let id = |path: &Path, root: &str, size| resume::file_id(path.strip_prefix(root).unwrap(), size);
    assert_eq!(id(one, "/tmp/usb/sda/sda1", 42), id(two, "/tmp/usb/sdb/sdb1", 42));
    // Another file of the same name, or another name, is another film.
    assert_ne!(id(one, "/tmp/usb/sda/sda1", 42), id(one, "/tmp/usb/sda/sda1", 43));
    assert_ne!(resume::file_id(Path::new("Films/Mad Max.mkv"), 42), resume::file_id(Path::new("Mad Max.mkv"), 42));
    assert!((0..=0x3fff_ffff_ffff_ffff).contains(&resume::file_id(Path::new("Films/Mad Max.mkv"), 42)));
}

// ---- the player's server

fn lg() -> pley_core::CapsReport {
    pley_core::device::lg_2022_caps(&pley_core::CapsOverrides::default())
}

fn server_for(dir: &Path, name: &str, bytes: Vec<u8>, store: &Store) -> LocalServer {
    let film = dir.join(name);
    std::fs::write(&film, bytes).unwrap();
    LocalServer::new(film, dir.to_path_buf(), store.clone(), std::sync::Arc::new(|| Some(lg())))
}

fn play(server: &LocalServer, mode: PlaybackMode) -> Result<PlaybackInfo, ApiError> {
    server.play(&PlaybackRequest { item_id: server::ITEM_ID, mode, caps: lg().caps, ..PlaybackRequest::default() })
}

const SECOND: Duration = Duration::from_secs(1);

#[test]
fn a_film_on_a_drive_plays_as_it_is_with_its_subtitle_files_and_a_track_the_tv_decodes() {
    let dir = scratch_dir("local-film");
    let store = Store::open(scratch_dir("local-film-store")).unwrap();
    std::fs::write(dir.join("Mad Max.en.srt"), "1\n00:00:01,000 --> 00:00:02,000\nWitness me\n").unwrap();
    let server = server_for(&dir, "Mad Max.mkv", film(false, false), &store);
    let detail = server.item(server::ITEM_ID).unwrap();
    assert_eq!((detail.id, detail.title.as_str()), (server::ITEM_ID, "Mad Max"));
    let file = &detail.files[0];
    // The subtitle in the file has no Cues to read it through: the file beside it shows, after
    // the file's own streams, numbered as the server numbers files beside the media.
    use StreamKind::*;
    assert_eq!(file.streams.iter().map(|s| s.kind).collect::<Vec<_>>(), [Video, Audio, Audio, Audio, Subtitle]);
    let srt = &file.streams[4];
    assert_eq!((srt.codec.as_str(), srt.language.as_deref(), srt.index), ("subrip", Some("en"), 1000));
    assert_eq!((file.chapters.len(), file.container.as_deref()), (2, Some("mkv")));

    let info = play(&server, PlaybackMode::Auto).unwrap();
    assert_eq!(
        (info.session_id.as_str(), info.method, info.url.as_str(), info.hls),
        ("local", PlaybackMethod::Direct, &*dir.join("Mad Max.mkv").to_string_lossy(), false)
    );
    // TrueHD is the default, but this TV doesn't decode it: the AC-3 beside it plays.
    assert_eq!(info.audio_index, Some(2));
    assert_eq!(info.video_range, None);
    let url = info.subtitles[0].url.clone().unwrap();
    assert_eq!(info.subtitles.len(), 1);
    assert_eq!(url, dir.join("Mad Max.en.srt").to_string_lossy());
    let text = server.subtitles(&url, "text/vtt", None, SECOND).unwrap();
    assert!(text.complete);
    assert_eq!(parse_vtt(&String::from_utf8_lossy(&text.body))[0].plain_text(), "Witness me");
    // The TV's player opens it at its address, each part encoded.
    let address = server.url(&info.url);
    assert!(address.starts_with("file:///") && address.ends_with("/Mad%20Max.mkv"), "{address}");

    // Nothing converts it.
    for mode in [PlaybackMode::Remux, PlaybackMode::Transcode] {
        let refused = play(&server, mode).unwrap_err();
        assert_eq!((refused.status(), refused.to_string()), (Some(409), server::NOTHING_CONVERTS.to_string()));
    }
    // Only what it gave is read.
    std::fs::write(dir.join("notes.srt"), "1\n00:00:01,000 --> 00:00:02,000\nNo\n").unwrap();
    assert!(server.subtitles(&dir.join("notes.srt").to_string_lossy(), "text/vtt", None, SECOND).is_err());
    assert!(server.subtitles("embedded:4", "text/vtt", None, SECOND).is_err());
    assert!(server.picture_track("/api/files/7/subtitles/4.mks", SECOND).unwrap().is_err());

    // Where it stopped is kept, for the next time it plays.
    let report =
        |position_ms| ProgressReport { position_ms, duration_ms: Some(7_380_000), ..ProgressReport::default() };
    server.progress(server::ITEM_ID, &report(600_000)).unwrap();
    let again = LocalServer::new(dir.join("Mad Max.mkv"), dir.clone(), store.clone(), std::sync::Arc::new(|| None));
    assert_eq!(again.item(server::ITEM_ID).unwrap().user_state.position_ms, 600_000);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_files_own_subtitle_tracks_are_read_out_of_it_for_the_player() {
    let dir = scratch_dir("local-tracks");
    let store = Store::open(scratch_dir("local-tracks-store")).unwrap();
    let text = server_for(&dir, "Text.mkv", subtitled("S_TEXT/UTF8", None, vec![]), &store);
    let info = play(&text, PlaybackMode::Auto).unwrap();
    assert_eq!(info.subtitles[0].url.as_deref(), Some("embedded:0"));
    let answer = text.subtitles("embedded:0", "text/vtt", None, SECOND).unwrap();
    assert!(answer.complete);
    let cues: Vec<_> = parse_vtt(&String::from_utf8_lossy(&answer.body)).iter().map(|c| c.plain_text()).collect();
    assert_eq!(cues, ["Hello", "There"]);
    // A text track is never handed over as pictures.
    assert!(text.picture_track("/api/files/7/subtitles/0.mks", SECOND).unwrap().is_err());

    let pictures = server_for(&dir, "Pictures.mkv", subtitled("S_HDMV/PGS", Some(vec![0x16, 0, 0]), vec![]), &store);
    let info = play(&pictures, PlaybackMode::Auto).unwrap();
    assert_eq!((info.subtitles[0].url.as_deref(), info.subtitles[0].text_based), (None, false));
    let (track, complete) = pictures.picture_track("/api/files/7/subtitles/0.mks", SECOND).unwrap().unwrap();
    assert!(complete);
    assert_eq!(track.codec_id, "S_HDMV/PGS");
    assert_eq!(track.samples.iter().map(|s| s.time_ms).collect::<Vec<_>>(), [1_500, 10_000]);
    assert_eq!(track.samples[0].data(), [0x16, 0, 0]);
    // Each picture is held once: every answer hands over the same bytes, not a copy.
    let (again, _) = pictures.picture_track("/api/files/7/subtitles/0.mks", SECOND).unwrap().unwrap();
    assert_eq!(again.samples[0].data().as_ptr(), track.samples[0].data().as_ptr());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_tracks_blocks_go_to_the_reader_without_a_copy_kept_while_reading() {
    let file = subtitled("S_HDMV/PGS", Some(vec![0x16, 0x00, 0x00]), vec![]);
    let facts = read_head(&file, usize::MAX);
    // The blocks are the reader's as they come: what it keeps are the very buffers read.
    let mut kept: Vec<(usize, SubtitleBlock)> = Vec::new();
    let count = cues::read(
        file.as_slice(),
        &facts,
        &facts.tracks[0],
        |b| kept.push((b.data.as_ptr() as usize, b)),
        &AtomicBool::new(false),
    );
    assert_eq!(count, 2);
    assert!(kept.iter().all(|(at, b)| *at == b.data.as_ptr() as usize));
    assert_eq!(kept.iter().map(|(_, b)| b.start_ms).collect::<Vec<_>>(), [1_500, 10_000]);
}

#[test]
fn a_files_address_names_each_part_encoded() {
    let path = Path::new("/tmp/usb/sda/sda1/Films #1/Amélie (2001).mkv");
    assert_eq!(LocalServer::file_url(path), "file:///tmp/usb/sda/sda1/Films%20%231/Am%C3%A9lie%20(2001).mkv");
}

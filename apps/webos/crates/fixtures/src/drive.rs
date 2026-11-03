//! A pretend USB drive for the simulator's `--drive fixtures`: a folder of videos that are heads
//! only (what the app reads of a file before playing it, a few KB each), made here rather than
//! stored, with [`crate::pgs`]'s Matroska writer.
//!
//! - `Mad Max.mkv`: HEVC in PQ with HDR10+ in its first frame, TrueHD (the default), AC-3 and AAC
//!   audio, an English text track its Cues lead to, two chapters, and `Mad Max.en.srt` beside it;
//! - `Alita HDR10+.mkv`: HDR10+ alone, in its first frame;
//! - `Dolby Vision P7.mkv`: a profile 7 record (its HDR10 base layer plays);
//! - `Toy Story.mp4`: an MP4 whose movie box comes last, with Nero's chapters;
//! - `Season 1/`, with two episodes; and what a drive holds that is never listed: `$RECYCLE.BIN/`,
//!   `.hidden.mkv` and `notes.txt`.

use std::fs;
use std::io;
use std::path::Path;

use crate::pgs::{element, float, string, uint};

const EBML: u32 = 0x1A45DFA3;
const SEGMENT: u32 = 0x18538067;
const SEEK_HEAD: u32 = 0x114D9B74;
const INFO: u32 = 0x1549A966;
const TRACKS: u32 = 0x1654AE6B;
const CHAPTERS: u32 = 0x1043A770;
const CLUSTER: u32 = 0x1F43B675;
const CUES: u32 = 0x1C53BB6B;

/// Writes the pretend drive's files into `dir` (made when missing).
pub fn write_sample_drive(dir: &Path) -> io::Result<()> {
    fs::create_dir_all(dir.join("Season 1"))?;
    fs::create_dir_all(dir.join("$RECYCLE.BIN"))?;
    fs::write(dir.join("Mad Max.mkv"), mad_max())?;
    fs::write(dir.join("Mad Max.en.srt"), "1\n00:00:01,500 --> 00:00:03,500\nWitness me!\n")?;
    fs::write(dir.join("Alita HDR10+.mkv"), simple("V_MPEGH/ISO/HEVC", hdr10(), Some(hdr10plus_frame()), 7_320_000))?;
    fs::write(dir.join("Dolby Vision P7.mkv"), simple("V_MPEGH/ISO/HEVC", dolby_vision_p7(), None, 8_280_000))?;
    fs::write(dir.join("Toy Story.mp4"), toy_story())?;
    for (name, length) in [("Episode 1.mkv", 2_640_000), ("Episode 2.mkv", 2_580_000), ("Episode 10.mkv", 2_700_000)] {
        fs::write(dir.join("Season 1").join(name), simple("V_MPEG4/ISO/AVC", Vec::new(), None, length))?;
    }
    fs::write(dir.join(".hidden.mkv"), simple("V_MPEG4/ISO/AVC", Vec::new(), None, 60_000))?;
    fs::write(dir.join("notes.txt"), "Not a video.\n")?;
    Ok(())
}

fn header() -> Vec<u8> {
    element(EBML, &string(0x4282, "matroska"))
}

fn info(duration_ms: i64) -> Vec<u8> {
    element(INFO, &[uint(0x2AD7B1, 1_000_000), float(0x4489, duration_ms as f64)].concat())
}

/// A track entry: its number, kind (1 video, 2 audio, 0x11 subtitles), codec, default flag,
/// language and what else it says.
fn track(number: u64, kind: u64, codec: &str, default: bool, language: &str, extra: Vec<u8>) -> Vec<u8> {
    let parts = [
        uint(0xD7, number),
        uint(0x73C5, number),
        uint(0x83, kind),
        string(0x86, codec),
        uint(0x88, u64::from(default)),
        string(0x22B59C, language),
        extra,
    ];
    element(0xAE, &parts.concat())
}

/// A video's size and, when given, its colour.
fn video(width: u64, height: u64, colour: Vec<u8>) -> Vec<u8> {
    element(0xE0, &[uint(0xB0, width), uint(0xBA, height), colour].concat())
}

/// PQ with BT.2020 primaries, 10 bits: HDR10.
fn hdr10() -> Vec<u8> {
    let colour = element(0x55B0, &[uint(0x55BA, 16), uint(0x55BB, 9), uint(0x55B2, 10)].concat());
    video(3840, 1608, colour)
}

/// A Dolby Vision profile 7 record (dvcC: version 1.0, profile 7 level 6, base layer 6) over an
/// HDR10 base layer.
fn dolby_vision_p7() -> Vec<u8> {
    let dvcc = [1, 0, 7 << 1, 6 << 3, 6 << 4, 0, 0, 0];
    let mapping = element(0x41E4, &[uint(0x41E7, 0x6476_6343), element(0x41ED, &dvcc)].concat());
    [hdr10(), mapping].concat()
}

/// A SimpleBlock's data for track 1 holding one HEVC unit: a prefix SEI with HDR10+'s T.35
/// message.
fn hdr10plus_frame() -> Vec<u8> {
    let sei = [0x4E, 0x01, 0x04, 0x08, 0xB5, 0x00, 0x3C, 0x00, 0x01, 0x04, 0x01, 0x40, 0x80];
    [&[0x81, 0, 0, 0x80][..], &(sei.len() as u32).to_be_bytes(), &sei].concat()
}

/// A film of one video track (`codec`, with `extra` in its entry), `duration_ms` long, its first
/// frame `frame` when given.
fn simple(codec: &str, extra: Vec<u8>, frame: Option<Vec<u8>>, duration_ms: i64) -> Vec<u8> {
    let extra = if extra.is_empty() { video(1920, 1080, Vec::new()) } else { extra };
    let tracks = element(TRACKS, &track(1, 1, codec, true, "und", extra));
    let cluster = frame.map(|f| element(CLUSTER, &[uint(0xE7, 0), element(0xA3, &f)].concat())).unwrap_or_default();
    [header(), element(SEGMENT, &[info(duration_ms), tracks, cluster].concat())].concat()
}

/// Mad Max, as the Samsung app's tests make it, with its text track's two subtitles in two
/// clusters the Cues list, the SeekHead leading to the Cues, as FFmpeg writes them.
fn mad_max() -> Vec<u8> {
    let tracks = element(
        TRACKS,
        &[
            track(1, 1, "V_MPEGH/ISO/HEVC", true, "und", hdr10()),
            track(2, 2, "A_TRUEHD", true, "eng", element(0xE1, &uint(0x9F, 8))),
            track(3, 2, "A_AC3", false, "eng", element(0xE1, &uint(0x9F, 6))),
            track(4, 2, "A_AAC", false, "rus", element(0xE1, &uint(0x9F, 2))),
            track(5, 0x11, "S_TEXT/UTF8", true, "eng", Vec::new()),
        ]
        .concat(),
    );
    let chapter = |at_ns: u64, name: &str| {
        element(
            0xB6,
            &[uint(0x73C4, at_ns / 1_000_000 + 1), uint(0x91, at_ns), element(0x80, &string(0x85, name))].concat(),
        )
    };
    let chapters =
        element(CHAPTERS, &element(0x45B9, &[chapter(0, "Opening"), chapter(600_000_000_000, "The Road")].concat()));
    // A text block of track 5 (0x85): its time from the cluster's, flags, the words.
    let block = |relative: i16, words: &str| [&[0x85][..], &relative.to_be_bytes(), &[0x80], words.as_bytes()].concat();
    let timecode = uint(0xE7, 1_000);
    let first =
        [&timecode[..], &element(0xA3, &hdr10plus_frame()), &block_group(block(500, "Witness me!"), 2_000)].concat();
    let cluster1 = element(CLUSTER, &first);
    let cluster2 = element(CLUSTER, &[uint(0xE7, 600_000), element(0xA3, &block(1_000, "Oh, what a day!"))].concat());
    // The SeekHead's size doesn't change with the Cues' place (8-byte positions), so the places
    // are worked out with it first.
    let seek_head = |cues: u64| {
        let seek = [element(0x53AB, &CUES.to_be_bytes()), element(0x53AC, &cues.to_be_bytes())].concat();
        element(SEEK_HEAD, &element(0x4DBB, &seek))
    };
    let info = info(7_380_000);
    let c1 = (seek_head(0).len() + info.len() + tracks.len() + chapters.len()) as u64;
    let c2 = c1 + cluster1.len() as u64;
    let cues_at = c2 + cluster2.len() as u64;
    // Where each subtitle's block is in its cluster's data: after the timecode (and the frame).
    let in_first = (timecode.len() + element(0xA3, &hdr10plus_frame()).len()) as u64;
    let in_second = uint(0xE7, 600_000).len() as u64;
    let point = |time: u64, cluster: u64, relative: u64| {
        let positions = [uint(0xF7, 5), uint(0xF1, cluster), uint(0xF0, relative)].concat();
        element(0xBB, &[uint(0xB3, time), element(0xB7, &positions)].concat())
    };
    let cues = element(CUES, &[point(1_500, c1, in_first), point(601_000, c2, in_second)].concat());
    let segment = [seek_head(cues_at), info, tracks, chapters, cluster1, cluster2, cues].concat();
    [header(), element(SEGMENT, &segment)].concat()
}

/// A BlockGroup of `block`, lasting `duration_ms`.
fn block_group(block: Vec<u8>, duration_ms: u64) -> Vec<u8> {
    element(0xA0, &[element(0xA1, &block), uint(0x9B, duration_ms)].concat())
}

fn mp4_box(kind: &str, parts: &[Vec<u8>]) -> Vec<u8> {
    let body = parts.concat();
    [&((body.len() + 8) as u32).to_be_bytes()[..], kind.as_bytes(), &body].concat()
}

fn be(v: u64, bytes: usize) -> Vec<u8> {
    v.to_be_bytes()[8 - bytes..].to_vec()
}

/// Toy Story as an MP4: H.264 at 1080p and AAC stereo, its movie box after the media, with
/// Nero's chapters (`udta/chpl`, in 100 ns).
fn toy_story() -> Vec<u8> {
    let ftyp = mp4_box("ftyp", &[b"isom".to_vec(), be(0, 4)]);
    let mvhd = mp4_box("mvhd", &[be(0, 4), be(0, 4), be(0, 4), be(1000, 4), be(4_860_000, 4), vec![0; 80]]);
    let mdhd = |lang: &str| {
        let l = lang.as_bytes();
        let packed = (u64::from(l[0] - 0x60) << 10) | (u64::from(l[1] - 0x60) << 5) | u64::from(l[2] - 0x60);
        mp4_box("mdhd", &[be(0, 4), be(0, 4), be(0, 4), be(1000, 4), be(4_860_000, 4), be(packed, 2), be(0, 2)])
    };
    let hdlr = |kind: &str| mp4_box("hdlr", &[be(0, 4), be(0, 4), kind.as_bytes().to_vec(), vec![0; 12], vec![0]]);
    let stsd = |entry: Vec<u8>| mp4_box("stsd", &[be(0, 4), be(1, 4), entry]);
    let trak = |lang: &str, kind: &str, entry: Vec<u8>| {
        let stbl = mp4_box("stbl", &[stsd(entry)]);
        mp4_box("trak", &[mp4_box("mdia", &[mdhd(lang), hdlr(kind), mp4_box("minf", &[stbl])])])
    };
    let avc1 = mp4_box("avc1", &[vec![0; 8], vec![0; 16], be(1920, 2), be(1080, 2), vec![0; 50]]);
    let mp4a = mp4_box("mp4a", &[vec![0; 8], vec![0; 8], be(2, 2), be(16, 2), vec![0; 4], be(48000, 2), be(0, 2)]);
    let chapter =
        |start_ms: u64, title: &str| [be(start_ms * 10_000, 8), vec![title.len() as u8], title.into()].concat();
    let chpl = mp4_box(
        "chpl",
        &[vec![1, 0, 0, 0], vec![0; 4], vec![2], chapter(0, "Andy's Room"), chapter(600_000, "Pizza Planet")],
    );
    let moov = mp4_box("moov", &[mvhd, trak("und", "vide", avc1), trak("eng", "soun", mp4a), mp4_box("udta", &[chpl])]);
    [ftyp, mp4_box("mdat", &[vec![0; 64]]), moov].concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sample_drive_holds_its_films_and_what_is_never_listed() {
        let dir = std::env::temp_dir().join(format!("pley-fixtures-drive-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        write_sample_drive(&dir).unwrap();
        for name in ["Mad Max.mkv", "Mad Max.en.srt", "Alita HDR10+.mkv", "Dolby Vision P7.mkv", "Toy Story.mp4"] {
            assert!(dir.join(name).is_file(), "{name}");
        }
        assert!(dir.join("Season 1/Episode 10.mkv").is_file());
        assert!(dir.join("$RECYCLE.BIN").is_dir() && dir.join(".hidden.mkv").is_file());
        // Heads only: a few KB each.
        assert!(fs::metadata(dir.join("Mad Max.mkv")).unwrap().len() < 4096);
        assert_eq!(&fs::read(dir.join("Toy Story.mp4")).unwrap()[4..8], b"ftyp");
        let _ = fs::remove_dir_all(&dir);
    }
}

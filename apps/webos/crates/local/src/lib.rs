//! Videos on a USB drive plugged into the TV, everything but the playing and
//! the drawing: what a drive's folders show and the subtitle files beside a
//! video ([`folder`]), what a file says of itself in its head, which no
//! server has probed (Matroska in [`matroska`], MP4 in [`mp4`], the first
//! HEVC frames in [`hevc`]), the file's own subtitle tracks read through its
//! Cues ([`cues`]), and where each video stopped, kept on the TV ([`resume`]).
//! The player plays such a video through [`LocalServer`], which answers it
//! as a server would for direct play ([`server`]).
//!
//! A file on a drive is untrusted input: every read and allocation here is
//! bounded, and a malformed file reads as less, never as a panic. Nothing
//! here calls the TV.

pub mod cues;
pub mod ebml;
pub mod folder;
pub mod hevc;
pub mod matroska;
pub mod mp4;
pub mod reader;
pub mod resume;
pub mod server;

pub use cues::SubtitleBlock;
pub use folder::{Entry, Sidecar};
pub use hevc::HevcMarks;
pub use matroska::MatroskaTrack;
pub use reader::{FileReader, ReadAt};
pub use resume::Kept;
pub use server::LocalServer;

use pley_core::model::{Chapter, MediaStream};

/// The head read: Matroska's Info and Tracks come first, within a few KB, and
/// the first frames (HDR10+'s mark) within a couple of MB, after attachments
/// such as fonts.
pub const HEAD_BYTES: usize = 2 * 1024 * 1024;

/// What a file says of itself: its length, its streams as ffprobe would list
/// them, and its chapters. For Matroska, also what reading its subtitle
/// tracks out of it needs.
#[derive(Clone, Debug, PartialEq)]
pub struct LocalFacts {
    pub duration_ms: Option<i64>,
    /// In the file's order, numbered from 0.
    pub streams: Vec<MediaStream>,
    pub chapters: Vec<Chapter>,
    /// Each stream's Matroska track, by the stream's index; none for MP4.
    pub tracks: Vec<MatroskaTrack>,
    /// Where the Segment's data starts: positions in it count from here.
    pub segment_start: u64,
    /// Where the Cues are, from the file's start; none when the file has none.
    pub cues_at: Option<u64>,
    /// A timestamp's unit, in nanoseconds.
    pub scale_ns: u64,
}

impl Default for LocalFacts {
    fn default() -> LocalFacts {
        LocalFacts {
            duration_ms: None,
            streams: Vec::new(),
            chapters: Vec::new(),
            tracks: Vec::new(),
            segment_start: 0,
            cues_at: None,
            scale_ns: 1_000_000,
        }
    }
}

/// The facts of a file: its Matroska head, else its MP4 movie box, whatever
/// the file is called; none when it is neither, or too damaged to read.
pub fn facts<R: ReadAt + ?Sized>(file: &R) -> Option<LocalFacts> {
    let head = file.read_at(0, HEAD_BYTES);
    matroska::read(&head, file).or_else(|| mp4::read(file))
}

#[cfg(test)]
mod tests;

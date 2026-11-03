//! What a folder of the drive shows, read off the main thread (the Samsung app's
//! `StorageScreen.Load` and `Lines`): its lines, the caption under the page's title, or why it
//! shows none.

use std::path::{Path, PathBuf};

use pley_base::Icon;
use pley_core::format::{format_bytes, format_clock};
use pley_core::{Device, Store};
use pley_kit::TrackLine;
use pley_local::{folder, resume};

/// The page's title, and the rail entry's label.
pub const TITLE: &str = "USB drive";
/// The line that goes up a folder.
pub const UP: &str = "..";
/// Between the parts of the folder's place on the drive: "USB drive › Films › 2019".
const BETWEEN: &str = " › ";
/// A video started before shows at least this much of its bar.
const LEAST_PROGRESS: f32 = 0.02;

/// Why a folder shows no lines of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    /// No drive is plugged in, or the folder isn't on the one that is.
    NoDrive,
    /// The folder couldn't be read.
    Unreadable,
    /// It holds no videos or folders.
    Empty,
}

impl Problem {
    /// The message's title and its sentence.
    pub fn words(self) -> (&'static str, &'static str) {
        match self {
            Problem::NoDrive => ("No USB drive", "Plug a USB drive into the TV to play its videos here."),
            Problem::Unreadable => ("This folder can't be read", "The drive may have been pulled out."),
            Problem::Empty => ("Nothing to play here", "This folder has no videos or folders in it."),
        }
    }
}

/// What OK on a line does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Goes up a folder: Back.
    Up,
    /// Opens the folder over this page.
    Folder(PathBuf),
    /// Plays the video from where it stopped.
    Video { path: PathBuf, start_ms: u64 },
}

/// A line of the page, its words made once.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub pick: Pick,
    pub title: String,
    pub subtitle: Option<String>,
    /// A video's size; empty for a folder.
    pub size: String,
    pub icon: Icon,
    /// How far through it is, 0 to 1.
    pub progress: f32,
}

impl Line {
    /// The kit's line for it.
    pub fn track(&self) -> TrackLine<'_> {
        TrackLine {
            title: &self.title,
            subtitle: self.subtitle.as_deref(),
            duration: &self.size,
            icon: Some(self.icon),
            progress: self.progress,
            ..TrackLine::default()
        }
    }
}

/// A folder as the page shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Listing {
    /// Under the title: how full the drive is at its top, else where the folder is on it.
    pub caption: String,
    pub lines: Vec<Line>,
    pub problem: Option<Problem>,
}

impl Listing {
    /// Whether it starts with the line that goes up.
    pub fn has_up(&self) -> bool {
        self.lines.first().is_some_and(|l| l.pick == Pick::Up)
    }
}

/// The folder `asked` (none: the drive's top) of the drive `device` has now: its folders, then
/// its videos, each saying where it stopped (kept in `store`). It reads the disk: run it off the
/// main thread.
pub fn list(device: &dyn Device, store: &Store, asked: Option<&Path>) -> Listing {
    let drive = device.drive().filter(|d| d.readable());
    let Some(root) = drive.map(|d| d.root) else { return problem(String::new(), Vec::new(), Problem::NoDrive) };
    let dir = asked.unwrap_or(&root);
    if !dir.starts_with(&root) {
        return problem(String::new(), Vec::new(), Problem::NoDrive);
    }
    let under = dir.strip_prefix(&root).unwrap_or(Path::new(""));
    let mut lines = Vec::new();
    let caption = if under.as_os_str().is_empty() {
        device
            .drive_space(&root)
            .map(|s| format!("{} free of {}", format_bytes(Some(s.free)), format_bytes(Some(s.total))))
            .unwrap_or_default()
    } else {
        lines.push(Line {
            pick: Pick::Up,
            title: UP.into(),
            subtitle: Some("Up a folder".into()),
            size: String::new(),
            icon: Icon::CornerLeftUp,
            progress: 0.0,
        });
        let parts = under.components().map(|c| c.as_os_str().to_string_lossy().into_owned());
        std::iter::once(TITLE.to_string()).chain(parts).collect::<Vec<_>>().join(BETWEEN)
    };
    let entries = match folder::list(dir) {
        Ok(entries) => entries,
        Err(_) => return problem(caption, lines, Problem::Unreadable),
    };
    if entries.is_empty() {
        return problem(caption, lines, Problem::Empty);
    }
    let kept = resume::all_kept(store);
    lines.extend(entries.into_iter().map(|e| {
        if e.folder {
            return Line {
                pick: Pick::Folder(e.path),
                title: e.name,
                subtitle: None,
                size: String::new(),
                icon: Icon::Folder,
                progress: 0.0,
            };
        }
        let relative = e.path.strip_prefix(&root).unwrap_or(&e.path);
        let at = kept.get(&resume::file_id(relative, e.size)).copied().unwrap_or_default();
        let kind = e.path.extension().map(|x| x.to_string_lossy().to_uppercase()).unwrap_or_default();
        let subtitle = if at.position_ms > 0 {
            format!("{kind} · Resumes at {}", format_clock(Some(at.position_ms)))
        } else {
            kind
        };
        let progress = if at.position_ms > 0 && at.duration_ms > 0 {
            (at.position_ms as f32 / at.duration_ms as f32).clamp(LEAST_PROGRESS, 1.0)
        } else {
            0.0
        };
        Line {
            title: folder::title(&e.path),
            pick: Pick::Video { path: e.path, start_ms: at.position_ms.max(0) as u64 },
            subtitle: Some(subtitle).filter(|s| !s.is_empty()),
            size: format_bytes(Some(e.size)),
            icon: Icon::FileVideo,
            progress,
        }
    }));
    Listing { caption, lines, problem: None }
}

fn problem(caption: String, lines: Vec<Line>, problem: Problem) -> Listing {
    Listing { caption, lines, problem: Some(problem) }
}

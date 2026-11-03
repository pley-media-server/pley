//! What a folder of the drive shows, read off the main thread (the Samsung app's
//! `StorageScreen.Load` and `Lines`): the page's title, its lines, the caption under the title,
//! or why it shows none.

use std::path::{Path, PathBuf};

use pley_base::Icon;
use pley_core::format::{format_bytes, format_clock};
use pley_core::{Device, Store};
use pley_kit::TrackLine;
use pley_local::{folder, resume};

/// The rail entry's label, and the page's title when the drive doesn't say its make and model.
pub const TITLE: &str = "USB drive";
/// The line that goes up a folder.
pub const UP: &str = "..";
/// Between the parts of the folder's place on the drive, and before them: "/Films/2019".
const SLASH: &str = "/";
/// Between the drive's top and how full it is: "/ · 437 GB free of 1.0 TB".
const DOT: &str = " · ";
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

    /// The picture over the message.
    pub fn icon(self) -> Icon {
        match self {
            Problem::NoDrive => Icon::Usb,
            Problem::Unreadable => Icon::CircleAlert,
            Problem::Empty => Icon::FolderOpen,
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
    /// The drive's make and model as it says them ("Kingston DataTraveler 3.0"), else [`TITLE`].
    pub title: String,
    /// Under the title, where the folder is on the drive: "/Films/2019"; at its top "/" and how
    /// full it is. Empty without a drive.
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
    let no_drive = || problem(TITLE.into(), String::new(), Vec::new(), Problem::NoDrive);
    let Some(root) = drive.map(|d| d.root) else { return no_drive() };
    let dir = asked.unwrap_or(&root);
    if !dir.starts_with(&root) {
        return no_drive();
    }
    let title = device.drive_name(&root).unwrap_or_else(|| TITLE.into());
    let under = dir.strip_prefix(&root).unwrap_or(Path::new(""));
    let mut lines = Vec::new();
    let caption = if under.as_os_str().is_empty() {
        let space = device
            .drive_space(&root)
            .map(|s| format!("{} free of {}", format_bytes(Some(s.free)), format_bytes(Some(s.total))));
        space.map_or_else(|| SLASH.to_string(), |s| format!("{SLASH}{DOT}{s}"))
    } else {
        lines.push(up_line());
        under.components().map(|c| format!("{SLASH}{}", c.as_os_str().to_string_lossy())).collect()
    };
    let entries = match folder::list(dir) {
        Ok(entries) => entries,
        Err(_) => return problem(title, caption, lines, Problem::Unreadable),
    };
    if entries.is_empty() {
        return problem(title, caption, lines, Problem::Empty);
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
    Listing { title, caption, lines, problem: None }
}

/// What the page shows when reading the folder `asked` failed outright (the reading itself
/// stopped): that it can't be read, with the line that goes up from a folder under the top.
pub fn unreadable(asked: Option<&Path>) -> Listing {
    let lines = if asked.is_some() { vec![up_line()] } else { Vec::new() };
    problem(TITLE.into(), String::new(), lines, Problem::Unreadable)
}

fn up_line() -> Line {
    Line {
        pick: Pick::Up,
        title: UP.into(),
        subtitle: Some("Up a folder".into()),
        size: String::new(),
        icon: Icon::CornerLeftUp,
        progress: 0.0,
    }
}

fn problem(title: String, caption: String, lines: Vec<Line>, problem: Problem) -> Listing {
    Listing { title, caption, lines, problem: Some(problem) }
}

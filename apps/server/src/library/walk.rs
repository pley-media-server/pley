//! Directory walking: finds media files plus the sidecar files (artwork,
//! subtitles) next to them, skipping system folders and samples. Bonus
//! material (trailers, featurettes, …) is found with Plex's conventions:
//! folders such as `Trailers` or `Behind The Scenes` next to the movie or in
//! the show folder, or a suffix such as `Movie (2020)-trailer.mkv`. Names
//! that are not valid UTF-8 are left out: paths are kept as text.
//!
//! Several folders are listed at once ([`WALK_THREADS`]): on a network share
//! each listing waits for the server, which answers many at a time. What is
//! found comes out in the same order whichever thread found it.

use std::collections::HashMap;
use std::fs::DirEntry;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use parking_lot::{Condvar, Mutex};
use same_file::Handle;

use super::LibraryKind;
use crate::util::system_time_secs;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mkv", "mp4", "m4v", "avi", "mov", "wmv", "ts", "m2ts", "mts", "webm", "flv", "mpg", "mpeg", "ogv", "3gp", "divx",
    "asf",
];
const AUDIO_EXTENSIONS: &[&str] =
    &["mp3", "flac", "m4a", "aac", "ogg", "oga", "opus", "wav", "wma", "aiff", "aif", "ape", "wv", "mka", "m4b", "dsf"];
pub const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "tbn"];
pub const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "sup", "smi"];

const SKIP_DIRS: &[&str] = &[
    "@eadir",
    "#recycle",
    "$recycle.bin",
    "system volume information",
    "lost+found",
    ".trickplay",
    "#snapshot",
    ".grab",
];
/// Release samples: never imported.
const SAMPLE_DIRS: &[&str] = &["sample", "samples"];

/// Folders listed at the same time. On a network share every listing is a
/// round trip to the server, which serves many at once: a NAS share of 1,444
/// folders took 14 to 20 s to walk one folder at a time and 2 to 3 s sixteen
/// at a time; a local disk was 2 to 5 times faster too.
const WALK_THREADS: usize = 16;

/// Kinds of bonus material (Plex's extra types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExtraKind {
    Trailer,
    BehindTheScenes,
    DeletedScene,
    Featurette,
    Interview,
    Scene,
    Short,
    Other,
}

impl ExtraKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ExtraKind::Trailer => "trailer",
            ExtraKind::BehindTheScenes => "behind_the_scenes",
            ExtraKind::DeletedScene => "deleted_scene",
            ExtraKind::Featurette => "featurette",
            ExtraKind::Interview => "interview",
            ExtraKind::Scene => "scene",
            ExtraKind::Short => "short",
            ExtraKind::Other => "other",
        }
    }

    /// A bonus-material folder name (lowercase).
    fn from_dir(name: &str) -> Option<Self> {
        Some(match name {
            "trailers" => ExtraKind::Trailer,
            "behind the scenes" => ExtraKind::BehindTheScenes,
            "deleted scenes" => ExtraKind::DeletedScene,
            "featurettes" => ExtraKind::Featurette,
            "interviews" => ExtraKind::Interview,
            "scenes" => ExtraKind::Scene,
            "shorts" => ExtraKind::Short,
            "extras" | "other" | "bonus" | "special features" | "specials features" => ExtraKind::Other,
            _ => return None,
        })
    }

    /// A file named with a bonus-material suffix (`…-trailer`), and its stem
    /// without the suffix. Only `trailer` is matched in any case and after
    /// `.` or `_` too; the others must be Plex's lowercase `-suffix`, so that
    /// release groups (`…x264-SCENE`) are not mistaken for extras.
    fn from_stem(stem: &str) -> Option<(Self, &str)> {
        const SUFFIXES: &[(&str, ExtraKind)] = &[
            ("-behindthescenes", ExtraKind::BehindTheScenes),
            ("-deleted", ExtraKind::DeletedScene),
            ("-featurette", ExtraKind::Featurette),
            ("-interview", ExtraKind::Interview),
            ("-scene", ExtraKind::Scene),
            ("-short", ExtraKind::Short),
            ("-other", ExtraKind::Other),
        ];
        let lower = stem.to_ascii_lowercase();
        for tail in ["-trailer", ".trailer", "_trailer"] {
            if lower.ends_with(tail) {
                return Some((ExtraKind::Trailer, &stem[..stem.len() - tail.len()]));
            }
        }
        SUFFIXES
            .iter()
            .find(|(tail, _)| stem.ends_with(tail))
            .map(|(tail, kind)| (*kind, &stem[..stem.len() - tail.len()]))
    }
}

/// Bonus material and what it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Extra {
    pub kind: ExtraKind,
    /// The movie's or show's folder (folder extras), or the file's own folder.
    pub owner_dir: PathBuf,
    /// For `Movie (2020)-trailer.mkv`: `Movie (2020)`, the owner's file stem.
    pub owner_stem: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FoundFile {
    pub path: PathBuf,
    pub root: PathBuf,
    pub size: i64,
    pub mtime: i64,
    /// Set for bonus material (trailers, featurettes, …).
    pub extra: Option<Extra>,
}

/// Whether `path` is bonus material: inside a bonus folder below `root`, or
/// named with a bonus suffix.
pub fn extra_of(path: &Path, root: &Path) -> Option<Extra> {
    let relative = path.strip_prefix(root).ok()?;
    let mut dir = root.to_path_buf();
    let components: Vec<_> = relative.components().collect();
    // Folders between the root and the file, nearest last.
    let mut found = None;
    for component in components.iter().take(components.len().saturating_sub(1)) {
        let name = component.as_os_str().to_string_lossy().to_ascii_lowercase();
        if let Some(kind) = ExtraKind::from_dir(&name) {
            found = Some(Extra { kind, owner_dir: dir.clone(), owner_stem: None });
            break;
        }
        dir.push(component);
    }
    if found.is_some() {
        return found;
    }
    let stem = path.file_stem()?.to_string_lossy();
    let (kind, owner) = ExtraKind::from_stem(&stem)?;
    Some(Extra { kind, owner_dir: path.parent()?.to_path_buf(), owner_stem: Some(owner.to_owned()) })
}

#[derive(Debug, Default)]
pub struct Walked {
    /// Media files by library folder (in the order given), then by path.
    pub media: Vec<FoundFile>,
    /// Sidecar file names (images, subtitles) per directory, sorted.
    pub sidecars: HashMap<PathBuf, Vec<String>>,
    /// Library roots that could not be read (unmounted drive?): files below
    /// them must not be treated as deleted.
    pub offline_roots: Vec<PathBuf>,
    /// Folders, links and files inside the roots that failed to read (an
    /// offline network share behind a symlink, a permission problem, an I/O
    /// error): files below them must not be treated as deleted either.
    pub unreadable: Vec<PathBuf>,
    /// Folders listed.
    pub folders: usize,
}

impl Walked {
    /// Whether a known file at `path` may be missing only because it could
    /// not be read this time.
    pub fn maybe_unavailable(&self, path: &Path) -> bool {
        self.offline_roots.iter().chain(&self.unreadable).any(|p| path.starts_with(p))
    }
}

fn extension(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

pub fn is_media_file(path: &Path, kind: LibraryKind) -> bool {
    let ext = extension(path);
    match kind {
        LibraryKind::Music => AUDIO_EXTENSIONS.contains(&ext.as_str()),
        _ => VIDEO_EXTENSIONS.contains(&ext.as_str()),
    }
}

/// What kind of file an extension names: video, audio, picture or
/// subtitles (`None`: something else).
fn file_kind(ext: &str) -> Option<u8> {
    [VIDEO_EXTENSIONS, AUDIO_EXTENSIONS, IMAGE_EXTENSIONS, SUBTITLE_EXTENSIONS]
        .iter()
        .position(|list| list.contains(&ext))
        .map(|kind| kind as u8)
}

/// Whether `path` is a file of the kind its name says, also when it is a
/// symbolic link: a link is followed only to a file of the same kind (a
/// video to a video…). Someone who can put a link named like a film in a
/// library folder (a shared folder, an unpacked download) can't make pley
/// hand out a password file or pley's own database to everyone who sees the
/// library, whether the link was there when the library was scanned or
/// swapped in later.
pub fn leads_to_its_kind(path: &Path) -> bool {
    let Ok(meta) = std::fs::symlink_metadata(path) else { return false };
    if !meta.file_type().is_symlink() {
        return true;
    }
    let kind = file_kind(&extension(path));
    kind.is_some() && std::fs::canonicalize(path).is_ok_and(|target| file_kind(&extension(&target)) == kind)
}

/// `path` opened, and its metadata, when it is a file of the kind its name
/// says (as [`leads_to_its_kind`] has it), told by the file opened: what is
/// read is what was checked. Checking the name and then opening it again
/// would let someone who can write in the library swap a link in between,
/// and hand out pley's database or `secret.key` named `poster.jpg`. The name
/// is checked first too, so that a link to a device or a pipe isn't opened.
pub fn open_of_its_kind(path: &Path) -> Option<(std::fs::File, std::fs::Metadata)> {
    if !leads_to_its_kind(path) {
        return None;
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    // One swapped in since: opened without waiting for a pipe's writer or
    // taking a terminal, then refused as no file.
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NONBLOCK | libc::O_NOCTTY);
    let file = options.open(path).ok()?;
    checked(file, path)
}

/// `file`, opened from `path`, and its metadata, when it is a file and the
/// one `path` leads to now: `path` itself when it isn't a link, else the
/// file of the same kind the link leads to. Whatever was swapped in before
/// the open leads elsewhere by now, or here to another file than the one
/// opened.
fn checked(file: std::fs::File, path: &Path) -> Option<(std::fs::File, std::fs::Metadata)> {
    let opened = file.metadata().ok().filter(std::fs::Metadata::is_file)?;
    let meta = std::fs::symlink_metadata(path).ok()?;
    let kind = file_kind(&extension(path));
    let named = if meta.file_type().is_symlink() {
        let target = std::fs::canonicalize(path).ok()?;
        if kind.is_none() || file_kind(&extension(&target)) != kind {
            return None;
        }
        target
    } else {
        path.to_path_buf()
    };
    is_the_file_at(&file, &opened, &named, kind).then_some((file, opened))
}

/// Whether `file` (whose metadata is `opened`) is the file at `named`, not a
/// link there: by device and inode on Unix, read without following a link.
#[cfg(unix)]
fn is_the_file_at(_file: &std::fs::File, opened: &std::fs::Metadata, named: &Path, _kind: Option<u8>) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(named)
        .is_ok_and(|now| !now.file_type().is_symlink() && now.dev() == opened.dev() && now.ino() == opened.ino())
}

/// Whether `file` is the file `named` names: by the name Windows gives the
/// file opened, wherever junctions and links led (object-manager links
/// included, which take no privilege and which the name's own checks don't
/// see). Its name is `named`'s, or one of the library name's `kind`. Only
/// the name: a library's folders may be junctions or links themselves.
#[cfg(windows)]
fn is_the_file_at(file: &std::fs::File, _opened: &std::fs::Metadata, named: &Path, kind: Option<u8>) -> bool {
    let Some(opened) = final_path(file) else { return false };
    let (Some(opened_name), Some(name)) = (opened.file_name(), named.file_name()) else { return false };
    opened_name.to_string_lossy().eq_ignore_ascii_case(&name.to_string_lossy())
        || (kind.is_some() && file_kind(&extension(&opened)) == kind)
}

/// The path Windows gives `file`, an open file, as it is now.
#[cfg(windows)]
fn final_path(file: &std::fs::File) -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_NAME_NORMALIZED, FILE_NAME_OPENED, GetFinalPathNameByHandleW, VOLUME_NAME_DOS, VOLUME_NAME_NT,
    };
    // By its drive's letter, or else (a volume mounted with none) by the
    // device's name; normalized, or else as opened, on a file system that
    // can't normalize names. Either is the name junctions and links led to,
    // and only the file's own name is looked at.
    let flags = [
        FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
        FILE_NAME_NORMALIZED | VOLUME_NAME_NT,
        FILE_NAME_OPENED | VOLUME_NAME_DOS,
        FILE_NAME_OPENED | VOLUME_NAME_NT,
    ];
    for flags in flags {
        let mut buf = vec![0u16; 1024];
        // Once more when the buffer was too small: `len` is then the length it takes.
        for _ in 0..2 {
            // SAFETY: an open file's handle, and a buffer of the length given.
            let len =
                unsafe { GetFinalPathNameByHandleW(file.as_raw_handle(), buf.as_mut_ptr(), buf.len() as u32, flags) }
                    as usize;
            if len == 0 {
                break;
            }
            if len < buf.len() {
                buf.truncate(len);
                return Some(PathBuf::from(std::ffi::OsString::from_wide(&buf)));
            }
            buf.resize(len + 1, 0);
        }
    }
    // The file isn't sent: said once, for the drive it's on.
    static TOLD: std::sync::Once = std::sync::Once::new();
    let err = std::io::Error::last_os_error();
    TOLD.call_once(|| tracing::warn!("Windows gives no name for a library's file opened ({err}): it isn't sent"));
    None
}

/// Release samples: `sample.mkv`, `sample-movie.mkv`, `movie-sample.mkv`
/// (small files only).
fn is_sample(path: &Path, size: u64) -> bool {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let sample_name = stem == "sample"
        || ["sample-", "sample.", "sample_"].iter().any(|p| stem.starts_with(p))
        || ["-sample", ".sample", "_sample"].iter().any(|s| stem.ends_with(s));
    sample_name && size < 500 * 1024 * 1024
}

pub fn walk(roots: &[PathBuf], kind: LibraryKind) -> Walked {
    walk_with(roots, kind, WALK_THREADS)
}

/// [`walk`] with `threads` threads listing folders, the calling one included.
fn walk_with(roots: &[PathBuf], kind: LibraryKind, threads: usize) -> Walked {
    let walker = Walker::new(roots, kind);
    let found = std::thread::scope(|scope| {
        let helpers: Vec<_> = (1..threads)
            .map_while(|_| {
                std::thread::Builder::new().name("pley-walk".into()).spawn_scoped(scope, || walker.work()).ok()
            })
            .collect();
        let mut found = vec![walker.work()];
        for helper in helpers {
            // A panic fails the scan, as it did with one thread.
            found.push(helper.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic)));
        }
        found
    });
    walker.finish(found)
}

/// A walk in progress: what it looks for, and the folders still to list.
struct Walker<'a> {
    roots: &'a [PathBuf],
    kind: LibraryKind,
    queue: Mutex<Queue>,
    /// Wakes the threads waiting for folders to list.
    ready: Condvar,
}

struct Queue {
    folders: Vec<Folder>,
    /// Folders being listed, which may add more.
    listing: usize,
}

/// A folder to list.
struct Folder {
    trail: Arc<Trail>,
    /// Which of the library folders (roots) it is in.
    root: usize,
}

/// A folder and the folders it is in, up to its library folder. A link to a
/// folder is followed only if it doesn't lead back to one of them: the walk
/// would go round in circles.
struct Trail {
    path: PathBuf,
    /// Which folder it is on disk, looked up when a link first needs it (the
    /// handle stays open: that is what keeps the answer right).
    id: OnceLock<Option<Handle>>,
    up: Option<Arc<Trail>>,
}

impl Trail {
    fn new(path: PathBuf, up: Option<Arc<Trail>>) -> Arc<Self> {
        Arc::new(Self { path, id: OnceLock::new(), up })
    }

    /// The folder on this trail that the link `link` (to a folder) leads
    /// back to, if any.
    fn leads_back(&self, link: &Path) -> std::io::Result<Option<&Path>> {
        let target = Handle::from_path(link)?;
        let mut folder = Some(self);
        while let Some(trail) = folder {
            match trail.id.get_or_init(|| Handle::from_path(&trail.path).ok()) {
                Some(id) if *id == target => return Ok(Some(&trail.path)),
                Some(_) => {}
                None => {
                    let unknown = format!("cannot tell which folder {} is", trail.path.display());
                    return Err(std::io::Error::other(unknown));
                }
            }
            folder = trail.up.as_deref();
        }
        Ok(None)
    }
}

/// What one thread found.
#[derive(Default)]
struct Found {
    /// Media files, with the position of their library folder.
    media: Vec<(usize, FoundFile)>,
    sidecars: Vec<(PathBuf, Vec<String>)>,
    offline: Vec<usize>,
    unreadable: Vec<PathBuf>,
    not_utf8_folders: Vec<PathBuf>,
    not_utf8_files: Vec<PathBuf>,
    folders: usize,
}

impl Found {
    /// Something in a library folder that could not be read: files below it
    /// must not be taken for deleted.
    fn cannot_read(&mut self, path: PathBuf, err: &std::io::Error) {
        tracing::warn!("cannot read {}: {err}", path.display());
        self.unreadable.push(path);
    }
}

/// A folder being listed. The folders found in it are queued once it is
/// done, also when listing it panicked: the other threads would otherwise
/// wait for them forever.
struct Listing<'w, 'a> {
    walker: &'w Walker<'a>,
    subfolders: Vec<Folder>,
}

impl Drop for Listing<'_, '_> {
    fn drop(&mut self) {
        let mut queue = self.walker.queue.lock();
        queue.listing -= 1;
        let wake = !self.subfolders.is_empty() || queue.listing == 0;
        queue.folders.append(&mut self.subfolders);
        drop(queue);
        if wake {
            self.walker.ready.notify_all();
        }
    }
}

impl<'a> Walker<'a> {
    fn new(roots: &'a [PathBuf], kind: LibraryKind) -> Self {
        // Taken from the end: the first library folder is listed first.
        let folders =
            roots.iter().enumerate().rev().map(|(root, path)| Folder { trail: Trail::new(path.clone(), None), root });
        Self { roots, kind, queue: Mutex::new(Queue { folders: folders.collect(), listing: 0 }), ready: Condvar::new() }
    }

    /// Lists folders until there are none left.
    fn work(&self) -> Found {
        let mut found = Found::default();
        while let Some(folder) = self.next() {
            let mut listing = Listing { walker: self, subfolders: Vec::new() };
            self.list(&folder, &mut found, &mut listing.subfolders);
        }
        found
    }

    /// The next folder to list, once there is one; `None` when all are listed.
    fn next(&self) -> Option<Folder> {
        let mut queue = self.queue.lock();
        loop {
            if let Some(folder) = queue.folders.pop() {
                queue.listing += 1;
                return Some(folder);
            }
            if queue.listing == 0 {
                return None;
            }
            self.ready.wait(&mut queue);
        }
    }

    fn list(&self, folder: &Folder, found: &mut Found, subfolders: &mut Vec<Folder>) {
        let path = &folder.trail.path;
        let entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(_) if folder.trail.up.is_none() => {
                tracing::warn!("library folder {} is not accessible; skipping", path.display());
                found.offline.push(folder.root);
                return;
            }
            Err(err) => return found.cannot_read(path.clone(), &err),
        };
        found.folders += 1;
        let mut sidecars = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => self.entry(folder, &entry, found, subfolders, &mut sidecars),
                // The listing broke off: what it didn't get to is unknown.
                Err(err) => {
                    found.cannot_read(path.clone(), &err);
                    break;
                }
            }
        }
        if !sidecars.is_empty() {
            found.sidecars.push((path.clone(), sidecars));
        }
    }

    /// One entry of a folder: a folder to list, a media or sidecar file, or
    /// something to leave out.
    fn entry(
        &self,
        folder: &Folder,
        entry: &DirEntry,
        found: &mut Found,
        subfolders: &mut Vec<Folder>,
        sidecars: &mut Vec<String>,
    ) {
        let name = entry.file_name();
        let lower = name.to_string_lossy().to_ascii_lowercase();
        if lower.starts_with('.') {
            return;
        }
        let path = entry.path();
        // A link counts as what it leads to.
        let (file_type, target) = match entry.file_type() {
            Ok(link) if link.is_symlink() => match std::fs::metadata(&path) {
                Ok(target) => (target.file_type(), Some(target)),
                Err(err) => return found.cannot_read(path, &err),
            },
            Ok(file_type) => (file_type, None),
            Err(err) => return found.cannot_read(path, &err),
        };
        if file_type.is_dir() {
            if SKIP_DIRS.contains(&lower.as_str())
                || (self.kind != LibraryKind::Music && SAMPLE_DIRS.contains(&lower.as_str()))
            {
                return;
            }
            if name.to_str().is_none() {
                found.not_utf8_folders.push(path);
                return;
            }
            if target.is_some() {
                match folder.trail.leads_back(&path) {
                    Ok(None) => {}
                    Ok(Some(ancestor)) => {
                        tracing::warn!("{} leads back to {}: not followed", path.display(), ancestor.display());
                        found.unreadable.push(path);
                        return;
                    }
                    Err(err) => return found.cannot_read(path, &err),
                }
            }
            let trail = Trail::new(path, Some(folder.trail.clone()));
            subfolders.push(Folder { trail, root: folder.root });
            return;
        }
        if !file_type.is_file() {
            return;
        }
        let ext = extension(&path);
        let sidecar = IMAGE_EXTENSIONS.contains(&ext.as_str()) || SUBTITLE_EXTENSIONS.contains(&ext.as_str());
        if !sidecar && !is_media_file(&path, self.kind) {
            return;
        }
        if target.is_some() && !leads_to_its_kind(&path) {
            tracing::warn!("{} links to a file of another kind: left out", path.display());
            return;
        }
        let Some(name) = name.to_str() else {
            found.not_utf8_files.push(path);
            return;
        };
        if sidecar {
            sidecars.push(name.to_owned());
            return;
        }
        // Size and time come with the listing on Windows; a link's are those
        // of what it leads to.
        let meta = match target.map_or_else(|| entry.metadata(), Ok) {
            Ok(meta) => meta,
            Err(err) => return found.cannot_read(path, &err),
        };
        if self.kind != LibraryKind::Music && is_sample(&path, meta.len()) {
            return;
        }
        let root = &self.roots[folder.root];
        let file = FoundFile {
            root: root.clone(),
            size: meta.len() as i64,
            mtime: meta.modified().map(system_time_secs).unwrap_or(0),
            extra: if self.kind == LibraryKind::Music { None } else { extra_of(&path, root) },
            path,
        };
        found.media.push((folder.root, file));
    }

    /// What the threads found, in the same order whichever thread found it.
    fn finish(&self, found: Vec<Found>) -> Walked {
        let mut out = Walked::default();
        let mut media = Vec::new();
        let mut offline = Vec::new();
        let (mut not_utf8_folders, mut not_utf8_files) = (Vec::new(), Vec::new());
        for part in found {
            media.extend(part.media);
            for (dir, names) in part.sidecars {
                out.sidecars.entry(dir).or_default().extend(names);
            }
            offline.extend(part.offline);
            out.unreadable.extend(part.unreadable);
            not_utf8_folders.extend(part.not_utf8_folders);
            not_utf8_files.extend(part.not_utf8_files);
            out.folders += part.folders;
        }
        // By library folder, then by path: the order of a walk of one folder
        // at a time in name order.
        media.sort_unstable_by(|(a_root, a), (b_root, b)| a_root.cmp(b_root).then_with(|| a.path.cmp(&b.path)));
        out.media = media.into_iter().map(|(_, file)| file).collect();
        for names in out.sidecars.values_mut() {
            names.sort_unstable();
        }
        offline.sort_unstable();
        out.offline_roots = offline.into_iter().map(|root| self.roots[root].clone()).collect();
        out.unreadable.sort_unstable();
        not_utf8_folders.sort_unstable();
        not_utf8_files.sort_unstable();
        // Once per scan rather than once per file.
        let skipped = not_utf8_folders.len() + not_utf8_files.len();
        if let Some(example) = not_utf8_folders.first().or(not_utf8_files.first()) {
            tracing::warn!(
                "left out {skipped} folders and files whose names are not valid UTF-8, such as {}: rename them to add them",
                example.display()
            );
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::time::Instant;

    use walkdir::WalkDir;

    use super::*;

    /// A name with a byte (Unix) or a code unit (Windows) that is not valid
    /// UTF-8, as Latin-1 names from old Samba shares are.
    fn not_utf8(start: &str, end: &str) -> Option<OsString> {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let mut bytes = start.as_bytes().to_vec();
            bytes.push(0xe9);
            bytes.extend_from_slice(end.as_bytes());
            Some(OsString::from_vec(bytes))
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStringExt;
            let mut wide: Vec<u16> = start.encode_utf16().collect();
            wide.push(0xd800);
            wide.extend(end.encode_utf16());
            Some(OsString::from_wide(&wide))
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (start, end);
            None
        }
    }

    /// The walk as it was before folders were listed in parallel (walkdir,
    /// one folder at a time, in the order the file system lists them): the
    /// parallel walk must find the same.
    fn reference(roots: &[PathBuf], kind: LibraryKind) -> Walked {
        let mut out = Walked::default();
        for root in roots {
            if std::fs::read_dir(root).is_err() {
                out.offline_roots.push(root.clone());
                continue;
            }
            let walker = WalkDir::new(root).follow_links(true).into_iter().filter_entry(|entry| {
                if entry.depth() == 0 {
                    return true;
                }
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if name.starts_with('.') {
                    return false;
                }
                if entry.file_type().is_dir() {
                    if SKIP_DIRS.contains(&name.as_str()) {
                        return false;
                    }
                    if kind != LibraryKind::Music && SAMPLE_DIRS.contains(&name.as_str()) {
                        return false;
                    }
                    if entry.file_name().to_str().is_none() {
                        return false;
                    }
                }
                true
            });
            for entry in walker {
                let entry = match entry {
                    Ok(e) => e,
                    Err(err) => {
                        out.unreadable.push(err.path().unwrap_or(root).to_path_buf());
                        continue;
                    }
                };
                if entry.file_type().is_dir() {
                    out.folders += 1;
                    continue;
                }
                if !entry.file_type().is_file() {
                    continue;
                }
                let path = entry.path();
                let ext = extension(path);
                let sidecar = IMAGE_EXTENSIONS.contains(&ext.as_str()) || SUBTITLE_EXTENSIONS.contains(&ext.as_str());
                if !sidecar && !is_media_file(path, kind) {
                    continue;
                }
                if entry.path_is_symlink() && !leads_to_its_kind(path) {
                    continue;
                }
                let Some(name) = entry.file_name().to_str() else { continue };
                if sidecar {
                    if let Some(dir) = path.parent() {
                        out.sidecars.entry(dir.to_path_buf()).or_default().push(name.to_owned());
                    }
                    continue;
                }
                let Ok(meta) = entry.metadata() else {
                    out.unreadable.push(path.to_path_buf());
                    continue;
                };
                if kind != LibraryKind::Music && is_sample(path, meta.len()) {
                    continue;
                }
                out.media.push(FoundFile {
                    path: path.to_path_buf(),
                    root: root.clone(),
                    size: meta.len() as i64,
                    mtime: meta.modified().map(system_time_secs).unwrap_or(0),
                    extra: if kind == LibraryKind::Music { None } else { extra_of(path, root) },
                });
            }
        }
        out
    }

    /// What a walk found, in a form two walks can be compared by.
    #[derive(Debug, PartialEq)]
    struct Summary {
        /// Position of the library folder, path, size, time, bonus material.
        media: Vec<(usize, PathBuf, i64, i64, Option<Extra>)>,
        sidecars: BTreeMap<PathBuf, Vec<String>>,
        offline_roots: Vec<PathBuf>,
        unreadable: Vec<PathBuf>,
    }

    fn summary(walked: &Walked, roots: &[PathBuf]) -> Summary {
        let position = |root: &Path| roots.iter().position(|r| r == root).unwrap();
        let mut media: Vec<_> = walked
            .media
            .iter()
            .map(|f| (position(&f.root), f.path.clone(), f.size, f.mtime, f.extra.clone()))
            .collect();
        media.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        let sidecars = walked
            .sidecars
            .iter()
            .map(|(dir, names)| {
                let mut names = names.clone();
                names.sort();
                (dir.clone(), names)
            })
            .collect();
        let mut unreadable = walked.unreadable.clone();
        unreadable.sort();
        Summary { media, sidecars, offline_roots: walked.offline_roots.clone(), unreadable }
    }

    fn file(root: &Path, rel: &str, bytes: usize) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![7u8; bytes]).unwrap();
    }

    /// A library folder with a bit of everything a walk looks at.
    fn library(root: &Path) {
        for n in 0..40 {
            let movie = format!("Film {n:02} ({})", 1980 + n);
            file(root, &format!("{movie}/{movie}.mkv"), n + 1);
            if n % 2 == 0 {
                file(root, &format!("{movie}/{movie}.en.srt"), 1);
                file(root, &format!("{movie}/poster.jpg"), 1);
            }
            if n % 3 == 0 {
                file(root, &format!("{movie}/Featurettes/Making of {n}.mp4"), 2);
                file(root, &format!("{movie}/Behind The Scenes/Set/Day {n}.mkv"), 2);
            }
            if n % 5 == 0 {
                file(root, &format!("{movie}/Subs/2_English.srt"), 1);
                file(root, &format!("{movie}/{movie}-trailer.mkv"), 3);
                file(root, &format!("{movie}/{movie} - Extended.mkv"), 3);
            }
            if n % 7 == 0 {
                file(root, &format!("{movie}/Sample/sample.mkv"), 1);
                file(root, &format!("{movie}/{movie}-sample.mkv"), 1);
                file(root, &format!("{movie}/Samples/{movie}.mp3"), 1);
            }
            if n % 4 == 0 {
                file(root, &format!("{movie}/@eaDir/{movie}.mkv"), 1);
                file(root, &format!("{movie}/.grab/frame.jpg"), 1);
            }
        }
        for (season, episode) in [(1, 1), (1, 2), (1, 3), (2, 1), (0, 1)] {
            file(root, &format!("Show (2010)/Season {season:02}/Show - S{season:02}E{episode:02}.mkv"), 4);
        }
        file(root, "Show (2010)/Season 01/Subs/Show - S01E01/3_French.srt", 1);
        file(root, "Show (2010)/Season 01/Show - S01E02.forced.ass", 1);
        file(root, "Show (2010)/season01-poster.jpg", 1);
        file(root, "Show (2010)/fanart.PNG", 1);
        file(root, "Show (2010)/Featurettes/Cast.mkv", 1);
        file(root, "Band/Record (2001)/01 - Song.flac", 5);
        file(root, "Band/Record (2001)/02 - Other Song.MP3", 5);
        file(root, "Band/Record (2001)/cover.jpg", 1);
        file(root, "Band/Record (2001)/Samples/03 - Loop.mp3", 5);
        file(root, ".hidden/Secret (2001).mkv", 1);
        file(root, "#recycle/Old (1999).mkv", 1);
        file(root, "$RECYCLE.BIN/Old (1998).mkv", 1);
        file(root, "lost+found/Lost (1997).mkv", 1);
        file(root, "Deep/1/2/3/4/5/6/7/8/9/10/Nested (2002).mkv", 1);
        file(root, "Ünïcode Fïlm (2003)/Ünïcode Fïlm (2003).mkv", 1);
        file(root, "Ünïcode Fïlm (2003)/Ünïcode Fïlm (2003).ja.srt", 1);
        file(root, "Loose (2004).MKV", 1);
        file(root, "Loose (2004).nfo", 1);
        file(root, "Cover.JPG", 1);
        file(root, "notes.txt", 1);
        std::fs::create_dir_all(root.join("Empty (2005)")).unwrap();
    }

    #[test]
    fn the_parallel_walk_finds_what_the_old_walk_found() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        library(&a);
        library(&b);
        let roots = [b.clone(), a.clone(), dir.path().join("missing")];
        for kind in [LibraryKind::Movies, LibraryKind::Shows, LibraryKind::Music] {
            let old = reference(&roots, kind);
            let expected = summary(&old, &roots);
            assert!(expected.media.len() >= 8 && expected.sidecars.len() >= 10, "{expected:?}");
            for threads in [1, 2, 16] {
                let walked = walk_with(&roots, kind, threads);
                assert_eq!(summary(&walked, &roots), expected, "{kind:?}, {threads} threads");
                assert_eq!(walked.folders, old.folders, "{kind:?}, {threads} threads");
            }
        }
        let movies = walk(&roots, LibraryKind::Movies);
        let names: Vec<_> =
            movies.media.iter().filter(|f| f.root == a).map(|f| f.path.strip_prefix(&a).unwrap()).collect();
        assert!(names.contains(&Path::new("Deep/1/2/3/4/5/6/7/8/9/10/Nested (2002).mkv")));
        assert!(names.contains(&Path::new("Film 03 (1983)/Behind The Scenes/Set/Day 3.mkv")));
        assert!(
            !names.iter().any(|n| n.starts_with(".hidden") || n.starts_with("#recycle") || n.ends_with("sample.mkv"))
        );
        assert_eq!(movies.offline_roots, [dir.path().join("missing")]);
    }

    #[test]
    fn what_is_found_comes_in_the_same_order_every_time() {
        let dir = tempfile::tempdir().unwrap();
        let (first, second) = (dir.path().join("z-first"), dir.path().join("a-second"));
        for root in [&first, &second] {
            // Created out of order; listed in whatever order the disk keeps.
            for name in ["é", "b2", "a-b", "B", "a b", "a"] {
                file(root, &format!("{name}/x/{name}.mkv"), 1);
                file(root, &format!("{name}/{name}.mkv"), 1);
                file(root, &format!("{name}/Poster.png"), 1);
                file(root, &format!("{name}/{name}.srt"), 1);
                file(root, &format!("{name}/fanart.jpg"), 1);
            }
        }
        let roots = [first.clone(), second.clone()];
        let walked = walk_with(&roots, LibraryKind::Movies, 8);
        let names = |walked: &Walked| -> Vec<String> {
            walked
                .media
                .iter()
                .map(|f| f.path.strip_prefix(dir.path()).unwrap().to_string_lossy().replace('\\', "/"))
                .collect()
        };
        let mut expected = Vec::new();
        for root in ["z-first", "a-second"] {
            // Files and folders by name, as a walk of one folder at a time in
            // name order would find them: `x` comes before `é.mkv`.
            for name in ["B", "a", "a b", "a-b", "b2", "é"] {
                let mut found = [format!("{root}/{name}/{name}.mkv"), format!("{root}/{name}/x/{name}.mkv")];
                if name == "é" {
                    found.reverse();
                }
                expected.extend(found);
            }
        }
        assert_eq!(names(&walked), expected);
        assert_eq!(walked.sidecars[&first.join("a")], ["Poster.png", "a.srt", "fanart.jpg"]);
        for threads in [1, 3, 16, 64] {
            assert_eq!(names(&walk_with(&roots, LibraryKind::Movies, threads)), expected, "{threads} threads");
        }
    }

    #[test]
    fn deep_and_wide_folders_are_all_listed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let mut deep = root.join("Deep");
        for level in 0..30 {
            deep.push(level.to_string());
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("Bottom (2001).mkv"), b"x").unwrap();
        for n in 0..300 {
            file(root, &format!("Wide {n:03} ({})/Wide {n:03}.mkv", 1900 + n), 1);
        }
        let walked = walk(&[root.to_path_buf()], LibraryKind::Movies);
        assert_eq!(walked.media.len(), 301);
        assert_eq!(walked.folders, 1 + 31 + 300);
        assert!(walked.media.windows(2).all(|pair| pair[0].path < pair[1].path));
        assert!(walked.media[0].path.ends_with("Bottom (2001).mkv"));
    }

    #[test]
    fn skipped_folders_are_not_even_listed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        file(root, "Film (2001)/Film (2001).mkv", 1);
        for skipped in ["@eaDir", "#recycle", ".hidden", "Sample", "System Volume Information", ".trickplay"] {
            file(root, &format!("{skipped}/Inside/Other (2002).mkv"), 1);
        }
        let movies = walk(&[root.to_path_buf()], LibraryKind::Movies);
        assert_eq!(movies.media.len(), 1);
        assert_eq!(movies.folders, 2, "the library folder and Film (2001)");
        // Music keeps sample folders (a track may be called that).
        assert_eq!(walk(&[root.to_path_buf()], LibraryKind::Music).folders, 4);
    }

    #[test]
    fn names_that_are_not_utf8_are_left_out() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("Heat (1995).mkv"), b"x").unwrap();
        let (Some(file), Some(folder), Some(poster)) =
            (not_utf8("Caf", " (2001).mkv"), not_utf8("Am", "lie (2001)"), not_utf8("Caf", ".jpg"))
        else {
            return;
        };
        // Some file systems (APFS) refuse such names: nothing to test there.
        if std::fs::write(root.join(&file), b"x").is_err() {
            return;
        }
        std::fs::create_dir(root.join(&folder)).unwrap();
        std::fs::write(root.join(&folder).join("Amelie (2001).mkv"), b"x").unwrap();
        std::fs::write(root.join(&poster), b"x").unwrap();
        std::fs::write(root.join("poster.jpg"), b"x").unwrap();

        let walked = walk(&[root.to_path_buf()], LibraryKind::Movies);
        let media: Vec<_> = walked.media.iter().map(|f| f.path.clone()).collect();
        assert_eq!(media, [root.join("Heat (1995).mkv")]);
        assert_eq!(walked.sidecars[root], ["poster.jpg"]);
        assert!(walked.unreadable.is_empty() && walked.offline_roots.is_empty());
    }

    #[test]
    fn samples_and_trailers() {
        let small = 10 * 1024 * 1024;
        assert!(is_sample(Path::new("/m/x/sample.mkv"), small));
        assert!(is_sample(Path::new("/m/x/movie.2019.1080p-sample.mkv"), small));
        assert!(is_sample(Path::new("/m/x/sample-movie.mkv"), small));
        assert!(!is_sample(Path::new("/m/x/Movie-trailer.mp4"), 4_000_000_000), "trailers are extras now");
        assert!(!is_sample(Path::new("/m/x/HDR Sample (2023).mkv"), small));
        assert!(!is_sample(Path::new("/m/x/Free Samples (2012).mkv"), small));
        assert!(!is_sample(Path::new("/m/x/movie-sample.mkv"), 2_000_000_000));
    }

    #[test]
    fn walks_a_tree() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("Movie (2020)/Extras")).unwrap();
        std::fs::create_dir_all(root.join(".hidden")).unwrap();
        std::fs::write(root.join("Movie (2020)/Movie (2020).mkv"), b"x").unwrap();
        std::fs::write(root.join("Movie (2020)/poster.jpg"), b"x").unwrap();
        std::fs::write(root.join("Movie (2020)/Movie (2020).en.srt"), b"x").unwrap();
        std::fs::write(root.join("Movie (2020)/Extras/Behind.mkv"), b"x").unwrap();
        std::fs::write(root.join(".hidden/secret.mkv"), b"x").unwrap();
        std::fs::write(root.join("Movie (2020)/Movie-sample.mkv"), b"x").unwrap();
        std::fs::write(root.join("notes.txt"), b"x").unwrap();

        let walked = walk(&[root.to_path_buf(), root.join("missing")], LibraryKind::Movies);
        let mut media: Vec<_> = walked.media.iter().collect();
        media.sort_by_key(|f| f.extra.is_some());
        assert_eq!(media.len(), 2);
        assert!(media[0].path.ends_with("Movie (2020).mkv"));
        assert!(media[0].extra.is_none());
        let extra = media[1].extra.as_ref().unwrap();
        assert_eq!((extra.kind, extra.owner_dir.clone()), (ExtraKind::Other, root.join("Movie (2020)")));
        let sidecars = &walked.sidecars[&root.join("Movie (2020)")];
        assert_eq!(sidecars.len(), 2);
        assert_eq!(walked.offline_roots, vec![root.join("missing")]);
        assert!(walked.maybe_unavailable(&root.join("missing/Film (1999)/Film.mkv")));
        assert!(!walked.maybe_unavailable(&root.join("Gone (2001)/Gone.mkv")));
    }

    /// A link to the folder `target` at `link`: a symbolic link, or on
    /// Windows a junction when symbolic links take rights the tests lack.
    fn link_folder(target: &Path, link: &Path) -> bool {
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link).is_ok()
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(target, link).is_ok()
                || std::process::Command::new("cmd")
                    .args(["/C", "mklink", "/J"])
                    .arg(link)
                    .arg(target)
                    .output()
                    .is_ok_and(|out| out.status.success())
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = (target, link);
            false
        }
    }

    #[test]
    fn links_that_lead_back_up_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("Library");
        file(&root, "Film (2001)/Film (2001).mkv", 1);
        file(&root, "Show (2010)/Season 01/Show - S01E01.mkv", 1);
        let links = [
            ("Show (2010)", "Show (2010)/Season 01/Back to the show"),
            ("", "Film (2001)/Back to the library"),
            ("Film (2001)", "Film (2001)/Itself"),
            // Not a loop: what it leads to is walked again, under its name.
            ("Film (2001)", "Shortcut (2001)"),
        ];
        for (target, link) in links {
            if !link_folder(&root.join(target), &root.join(link)) {
                return;
            }
        }
        let roots = [root.clone()];
        let walked = walk(&roots, LibraryKind::Movies);
        let found: Vec<_> = walked.media.iter().map(|f| f.path.strip_prefix(&root).unwrap()).collect();
        assert_eq!(
            found,
            [
                Path::new("Film (2001)/Film (2001).mkv"),
                Path::new("Shortcut (2001)/Film (2001).mkv"),
                Path::new("Show (2010)/Season 01/Show - S01E01.mkv"),
            ]
        );
        let unreadable: Vec<_> = walked.unreadable.iter().map(|p| p.strip_prefix(&root).unwrap()).collect();
        assert_eq!(
            unreadable,
            [
                Path::new("Film (2001)/Back to the library"),
                Path::new("Film (2001)/Itself"),
                Path::new("Shortcut (2001)/Back to the library"),
                Path::new("Shortcut (2001)/Itself"),
                Path::new("Show (2010)/Season 01/Back to the show"),
            ]
        );
        assert_eq!(summary(&walked, &roots), summary(&reference(&roots, LibraryKind::Movies), &roots));

        // A link to a folder that is gone (an offline share): left out, and
        // what was below it is not taken for deleted.
        let elsewhere = dir.path().join("Elsewhere");
        file(&elsewhere, "Far (1999)/Far (1999).mkv", 1);
        if !link_folder(&elsewhere, &root.join("Far away")) {
            return;
        }
        assert_eq!(walk(&roots, LibraryKind::Movies).media.len(), 4);
        std::fs::remove_dir_all(&elsewhere).unwrap();
        let walked = walk(&roots, LibraryKind::Movies);
        assert_eq!(walked.media.len(), 3);
        assert!(walked.maybe_unavailable(&root.join("Far away/Far (1999)/Far (1999).mkv")));
        assert!(!walked.maybe_unavailable(&root.join("Film (2001)/Gone (2001).mkv")));
    }

    /// Keeps everyone from listing a folder until dropped: no permissions on
    /// Unix, an entry that denies it on Windows. `None` where that can't be
    /// done, or doesn't stop the tests (running as root).
    struct Locked(PathBuf);

    impl Locked {
        fn new(dir: &Path) -> Option<Self> {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o000)).ok()?;
            }
            #[cfg(windows)]
            {
                let denied = std::process::Command::new("icacls")
                    .arg(dir)
                    .args(["/deny", "*S-1-1-0:(RD)"])
                    .output()
                    .is_ok_and(|out| out.status.success());
                if !denied {
                    return None;
                }
            }
            let locked = Self(dir.to_path_buf());
            std::fs::read_dir(dir).is_err().then_some(locked)
        }
    }

    impl Drop for Locked {
        fn drop(&mut self) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
            }
            #[cfg(windows)]
            {
                let _ = std::process::Command::new("icacls").arg(&self.0).args(["/remove:d", "*S-1-1-0"]).output();
            }
        }
    }

    #[test]
    fn a_folder_that_cannot_be_read_loses_nothing_else() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for n in 0..20 {
            file(root, &format!("Film {n} ({})/Film {n}.mkv", 2000 + n), 1);
        }
        file(root, "Locked (1999)/Locked (1999).mkv", 1);
        file(root, "Locked (1999)/Inside/Inside (1998).mkv", 1);
        let Some(_locked) = Locked::new(&root.join("Locked (1999)")) else { return };
        for threads in [1, 16] {
            let walked = walk_with(&[root.to_path_buf()], LibraryKind::Movies, threads);
            assert_eq!(walked.media.len(), 20, "{threads} threads");
            assert_eq!(walked.unreadable, [root.join("Locked (1999)")]);
            assert!(walked.maybe_unavailable(&root.join("Locked (1999)/Inside/Inside (1998).mkv")));
            assert!(!walked.maybe_unavailable(&root.join("Film 1 (2001)/Film 1.mkv")));
            assert!(walked.offline_roots.is_empty());
        }
        // A library folder that can't be read at all is offline.
        let _root_locked = Locked::new(root);
        let walked = walk(&[root.to_path_buf()], LibraryKind::Movies);
        assert!(walked.media.is_empty() && walked.offline_roots == [root.to_path_buf()]);
    }

    #[cfg(unix)]
    #[test]
    fn links_only_lead_to_files_of_their_kind() {
        let dir = tempfile::tempdir().unwrap();
        let library = dir.path().join("Movies");
        std::fs::create_dir_all(library.join("Alien (1979)")).unwrap();
        std::fs::write(dir.path().join("secrets.db"), b"not a film").unwrap();
        std::fs::write(dir.path().join("elsewhere.mkv"), vec![0u8; 64]).unwrap();
        std::fs::write(dir.path().join("passwd"), b"root:x:0:0").unwrap();
        let link = |target: &str, name: &str| {
            std::os::unix::fs::symlink(dir.path().join(target), library.join("Alien (1979)").join(name)).unwrap()
        };
        link("secrets.db", "Alien (1979).mkv");
        link("elsewhere.mkv", "Alien (1979) - Extended.mkv");
        link("passwd", "Alien (1979).en.srt");
        let walked = walk(std::slice::from_ref(&library), LibraryKind::Movies);
        let names: Vec<_> = walked.media.iter().map(|f| f.path.file_name().unwrap().to_owned()).collect();
        assert_eq!(names, ["Alien (1979) - Extended.mkv"]);
        assert!(walked.sidecars.values().all(|names| names.is_empty()), "{:?}", walked.sidecars);
        assert_eq!(walked.media[0].size, 64, "the size of what it leads to");

        // A link swapped after the scan is caught when the file is served.
        let film = library.join("Alien (1979)").join("Alien (1979) - Extended.mkv");
        assert!(leads_to_its_kind(&film));
        std::fs::remove_file(&film).unwrap();
        std::os::unix::fs::symlink(dir.path().join("secrets.db"), &film).unwrap();
        assert!(!leads_to_its_kind(&film));
        assert!(leads_to_its_kind(&dir.path().join("passwd")), "not a link: what the scan found");
        assert!(open_of_its_kind(&film).is_none());

        // What is read is what was checked: a file opened through a link
        // swapped in for a moment is no file of the link's kind, though the
        // link leads to one again by the time it's looked at.
        std::fs::write(dir.path().join("real.jpg"), b"\xff\xd8\xff").unwrap();
        let poster = library.join("Alien (1979)").join("poster.jpg");
        let swap = |target: &str| {
            let _ = std::fs::remove_file(&poster);
            std::os::unix::fs::symlink(dir.path().join(target), &poster).unwrap();
        };
        swap("real.jpg");
        assert!(open_of_its_kind(&poster).is_some(), "a link to a picture");
        swap("secrets.db");
        let opened = std::fs::File::open(&poster).unwrap();
        swap("real.jpg");
        assert!(checked(opened, &poster).is_none(), "the database, opened before the link went back");
        swap("secrets.db");
        let opened = std::fs::File::open(&poster).unwrap();
        std::fs::remove_file(&poster).unwrap();
        std::fs::write(&poster, b"\xff\xd8\xff").unwrap();
        assert!(checked(opened, &poster).is_none(), "the database, though a picture is there now");
        // And a picture opened as itself, now a link to the database: no.
        let opened = std::fs::File::open(&poster).unwrap();
        swap("secrets.db");
        assert!(checked(opened, &poster).is_none());
    }

    /// A library's folder may be a junction (no privilege makes one): what
    /// it holds is told by the name Windows gives the file opened.
    #[cfg(windows)]
    #[test]
    fn a_file_through_a_junction_is_opened() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        std::fs::write(real.join("Film (2020).mkv"), b"a film").unwrap();
        let films = dir.path().join("Films");
        let made = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(&films)
            .arg(&real)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(made.success());
        assert!(open_of_its_kind(&films.join("Film (2020).mkv")).is_some());
    }

    /// What was opened goes out only as the file the name names, Windows
    /// telling by the name it gives the file opened.
    #[test]
    fn a_file_other_than_the_one_named_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let poster = dir.path().join("poster.jpg");
        std::fs::write(&poster, b"\xff\xd8\xff").unwrap();
        std::fs::write(dir.path().join("secrets.db"), b"secret").unwrap();
        let database = std::fs::File::open(dir.path().join("secrets.db")).unwrap();
        assert!(checked(database, &poster).is_none());
        assert!(checked(std::fs::File::open(&poster).unwrap(), &poster).is_some());
    }

    #[test]
    fn a_file_is_opened_when_it_is_one() {
        let dir = tempfile::tempdir().unwrap();
        let film = dir.path().join("Film (2020).mkv");
        std::fs::write(&film, b"a film").unwrap();
        let (mut opened, meta) = open_of_its_kind(&film).expect("a film");
        assert_eq!(meta.len(), 6);
        let mut read = String::new();
        std::io::Read::read_to_string(&mut opened, &mut read).unwrap();
        assert_eq!(read, "a film");
        assert!(open_of_its_kind(&dir.path().join("passwd.txt")).is_none(), "gone");
        assert!(open_of_its_kind(dir.path()).is_none(), "a folder");
        // Not a link: whatever its name says, what the scan found.
        std::fs::write(dir.path().join("notes"), b"x").unwrap();
        assert!(open_of_its_kind(&dir.path().join("notes")).is_some());
    }

    #[test]
    fn unreadable_folders_protect_their_files() {
        let walked = Walked { unreadable: vec![PathBuf::from("/m/nas-link")], ..Default::default() };
        assert!(walked.maybe_unavailable(Path::new("/m/nas-link/Movie (2020)/Movie.mkv")));
        assert!(walked.maybe_unavailable(Path::new("/m/nas-link")));
        assert!(!walked.maybe_unavailable(Path::new("/m/other/Movie.mkv")));
    }

    #[test]
    fn finds_extras() {
        let root = Path::new("/m");
        let e = extra_of(Path::new("/m/Movie (2020)/Trailers/Teaser.mkv"), root).unwrap();
        assert_eq!((e.kind, e.owner_dir, e.owner_stem), (ExtraKind::Trailer, PathBuf::from("/m/Movie (2020)"), None));
        let e = extra_of(Path::new("/m/Movie (2020)/Movie (2020)-behindthescenes.mkv"), root).unwrap();
        assert_eq!((e.kind, e.owner_stem.as_deref()), (ExtraKind::BehindTheScenes, Some("Movie (2020)")));
        let e = extra_of(Path::new("/m/Movie (2020)/Movie (2020).Trailer.mp4"), root).unwrap();
        assert_eq!(e.kind, ExtraKind::Trailer);
        let e = extra_of(Path::new("/m/Show (2010)/Featurettes/Making Of.mkv"), root).unwrap();
        assert_eq!((e.kind, e.owner_dir), (ExtraKind::Featurette, PathBuf::from("/m/Show (2010)")));
        assert_eq!(extra_of(Path::new("/m/Show (2010)/Season 1/Show - S01E01.mkv"), root), None);
        assert_eq!(extra_of(Path::new("/m/Movie.2019.1080p.x264-SCENE.mkv"), root), None, "a release group");
        assert_eq!(extra_of(Path::new("/m/Crime Scene (2001)/Crime Scene (2001).mkv"), root), None);
    }

    /// Times the walk of real library folders against the old walk, and
    /// checks that both find the same. For a network share:
    /// `PLEY_WALK_ROOTS='\\nas\Media\TV' PLEY_WALK_KIND=shows cargo test -p pley-server --lib walks_real_folders -- --ignored --nocapture`
    /// (several folders separated as in PATH).
    #[test]
    #[ignore = "walks the folders named in PLEY_WALK_ROOTS"]
    fn walks_real_folders() {
        let Some(roots) = std::env::var_os("PLEY_WALK_ROOTS") else { return };
        let roots: Vec<PathBuf> = std::env::split_paths(&roots).collect();
        let kind =
            std::env::var("PLEY_WALK_KIND").ok().and_then(|k| LibraryKind::parse(&k)).unwrap_or(LibraryKind::Movies);
        let rounds = std::env::var("PLEY_WALK_ROUNDS").ok().and_then(|n| n.parse().ok()).unwrap_or(3);
        for round in 1..=rounds {
            let started = Instant::now();
            let old = reference(&roots, kind);
            let mut line = format!("round {round}: one folder at a time {:.2}s", started.elapsed().as_secs_f64());
            for threads in [1, 4, 8, 16, 32] {
                let started = Instant::now();
                let walked = walk_with(&roots, kind, threads);
                line += &format!(", {threads} threads {:.2}s", started.elapsed().as_secs_f64());
                assert_eq!(summary(&walked, &roots), summary(&old, &roots), "{threads} threads");
            }
            println!("{line} ({} folders, {} media files)", old.folders, old.media.len());
        }
    }
}

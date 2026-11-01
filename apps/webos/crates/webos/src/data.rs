//! Where the app keeps its data on the TV (DESIGN.md §5.7, RES/plx-net-data.md §6).
//!
//! No folder is writable under every jail: Developer Mode mounts
//! `/media/developer` read-write, the Homebrew Channel's jail
//! `/media/internal`, some older sets only `/media/developer/temp`, and some
//! none at all. So the data folder is probed, best first, and the last resort
//! is under `/tmp`, which a restart empties: the app then says it can't keep
//! the sign-in. Never inside the app's own folder, which every install
//! replaces. Folders are 0700, files 0600.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

/// The data folder chosen, and whether it survives a restart.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataDir {
    pub path: PathBuf,
    pub durable: bool,
}

/// The folders tried, best first, under `root` (`/` on the TV), with whether
/// each survives a restart.
pub fn candidates(root: &Path, app_id: &str) -> [(PathBuf, bool); 4] {
    [
        (root.join("media/developer").join(format!("{app_id}-data")), true),
        (root.join("media/internal").join(format!(".{app_id}-data")), true),
        (root.join("media/developer/temp").join(format!("{app_id}-data")), true),
        (root.join("tmp").join(app_id).join("data"), false),
    ]
}

/// The first candidate the app can make (or has) and write in; `None` when
/// even `/tmp` refuses.
pub fn probe(root: &Path, app_id: &str) -> Option<DataDir> {
    candidates(root, app_id).into_iter().find_map(|(path, durable)| {
        // Only the last folder is made for the durable places: their parents
        // are the jail's mounts, which exist or don't.
        usable(&path, !durable).ok()?;
        Some(DataDir { path, durable })
    })
}

/// Makes `dir` (0700) if it isn't there, refuses it if it is a link or
/// someone else's, and checks a file can be written in it.
fn usable(dir: &Path, make_parents: bool) -> io::Result<()> {
    if make_parents && let Some(parent) = dir.parent() {
        fs::create_dir_all(parent)?;
    }
    match private_dir_builder().create(dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    check_own_dir(dir)?;
    let probe = dir.join(format!(".probe-{}", std::process::id()));
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&probe)?;
    let written = file.write_all(b"1").and_then(|_| file.sync_all());
    drop(file);
    fs::remove_file(&probe)?;
    written
}

#[cfg(unix)]
fn private_dir_builder() -> fs::DirBuilder {
    use std::os::unix::fs::DirBuilderExt;
    let mut b = fs::DirBuilder::new();
    b.mode(0o700);
    b
}

#[cfg(not(unix))]
fn private_dir_builder() -> fs::DirBuilder {
    fs::DirBuilder::new()
}

#[cfg(unix)]
fn check_own_dir(dir: &Path) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let meta = fs::symlink_metadata(dir)?;
    // SAFETY: geteuid has no preconditions.
    if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } {
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, "not the app's folder"));
    }
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
fn check_own_dir(dir: &Path) -> io::Result<()> {
    if fs::symlink_metadata(dir)?.is_dir() {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::PermissionDenied, "not a folder"))
    }
}

/// Writes `bytes` to `path` so that a power cut leaves the old file or the new
/// one, never half of either: a temporary file (0600), flushed, then renamed.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    let temp = path.with_file_name(format!(".{name}.new-{}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let result = options.open(&temp).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    match result.and_then(|_| fs::rename(&temp, path)) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&temp);
            Err(e)
        }
    }
}

/// The app's install folder: the folder of `/proc/self/exe` on the TV
/// (`HOME` differs between the two jails), the executable's folder elsewhere.
pub fn app_dir() -> Option<PathBuf> {
    let exe = fs::read_link("/proc/self/exe").or_else(|_| std::env::current_exe()).ok()?;
    exe.parent().map(Path::to_path_buf)
}

/// The app id SAM launched, from the install folder's name
/// (`…/applications/<id>`); `fallback` when the folder isn't an install.
pub fn app_id_of(app_dir: &Path, fallback: &str) -> String {
    let installed = app_dir.parent().and_then(Path::file_name).is_some_and(|n| n == "applications");
    match app_dir.file_name().and_then(|n| n.to_str()) {
        Some(name) if installed && !name.is_empty() => name.to_string(),
        _ => fallback.to_string(),
    }
}

/// The byte budget of the picture cache in `dir`: half the free space, at
/// most 128 MiB, and none below 8 MiB (the app partition is shared by every
/// installed app).
pub fn cache_budget(dir: &Path) -> u64 {
    budget_for(free_bytes(dir))
}

fn budget_for(free: Option<u64>) -> u64 {
    const MAX: u64 = 128 * 1024 * 1024;
    const MIN: u64 = 8 * 1024 * 1024;
    let budget = free.map_or(MAX, |f| (f / 2).min(MAX));
    if budget < MIN { 0 } else { budget }
}

#[cfg(unix)]
fn free_bytes(dir: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    // SAFETY: a zeroed statvfs is a valid output buffer for statvfs(3).
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: a NUL-terminated path and a valid buffer.
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    Some((stat.f_bavail as u64).saturating_mul(stat.f_frsize as u64))
}

#[cfg(not(unix))]
fn free_bytes(_dir: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        pley_core::device::scratch_dir("data")
    }

    #[test]
    fn developer_mode_keeps_data_in_its_own_folder() {
        let root = root();
        fs::create_dir_all(root.join("media/developer")).unwrap();
        fs::create_dir_all(root.join("media/internal")).unwrap();
        let dir = probe(&root, "app.pley.tv").unwrap();
        assert_eq!(dir, DataDir { path: root.join("media/developer/app.pley.tv-data"), durable: true });
        assert!(dir.path.is_dir());
        assert_eq!(fs::read_dir(&dir.path).unwrap().count(), 0, "the probe leaves nothing behind");
    }

    #[test]
    fn the_homebrew_jail_uses_media_internal() {
        let root = root();
        fs::create_dir_all(root.join("media/internal")).unwrap();
        let dir = probe(&root, "app.pley.tv").unwrap();
        assert_eq!(dir, DataDir { path: root.join("media/internal/.app.pley.tv-data"), durable: true });
    }

    #[test]
    fn an_older_set_uses_the_developer_temp_folder() {
        let root = root();
        // `/media/developer` itself is the app's to read only: here, a file stands in.
        fs::create_dir_all(root.join("media/developer/temp")).unwrap();
        fs::write(root.join("media/developer/app.pley.tv-data"), b"not a folder").unwrap();
        let dir = probe(&root, "app.pley.tv").unwrap();
        assert_eq!(dir, DataDir { path: root.join("media/developer/temp/app.pley.tv-data"), durable: true });
    }

    #[test]
    fn with_nothing_durable_the_data_goes_to_tmp_and_says_so() {
        let root = root();
        let dir = probe(&root, "app.pley.tv").unwrap();
        assert_eq!(dir, DataDir { path: root.join("tmp/app.pley.tv/data"), durable: false });
        // A second start finds the same folder.
        assert_eq!(probe(&root, "app.pley.tv").unwrap(), dir);
    }

    #[cfg(unix)]
    #[test]
    fn folders_are_the_apps_alone() {
        use std::os::unix::fs::PermissionsExt;
        let root = root();
        fs::create_dir_all(root.join("media/developer")).unwrap();
        let dir = probe(&root, "app.pley.tv").unwrap();
        assert_eq!(fs::metadata(&dir.path).unwrap().permissions().mode() & 0o777, 0o700);
        write_atomic(&dir.path.join("a.json"), b"{}").unwrap();
        assert_eq!(fs::metadata(dir.path.join("a.json")).unwrap().permissions().mode() & 0o777, 0o600);
    }

    #[test]
    fn writes_replace_the_whole_file() {
        let dir = root();
        let file = dir.join("settings.json");
        write_atomic(&file, b"{\"a\":1}").unwrap();
        write_atomic(&file, b"{}").unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"{}");
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "no temporary file left");
    }

    #[test]
    fn the_app_id_comes_from_the_install_folder() {
        let installed = Path::new("/media/developer/apps/usr/palm/applications/app.pley.tv.beta");
        assert_eq!(app_id_of(installed, "app.pley.tv"), "app.pley.tv.beta");
        assert_eq!(app_id_of(Path::new("/home/me/pley/target/debug"), "app.pley.tv"), "app.pley.tv");
        assert!(app_dir().is_some());
    }

    #[test]
    fn the_picture_cache_takes_half_the_free_space_up_to_128_mib() {
        const MIB: u64 = 1024 * 1024;
        assert_eq!(budget_for(Some(1000 * MIB)), 128 * MIB);
        assert_eq!(budget_for(Some(100 * MIB)), 50 * MIB);
        assert_eq!(budget_for(Some(15 * MIB)), 0);
        assert_eq!(budget_for(None), 128 * MIB);
    }
}

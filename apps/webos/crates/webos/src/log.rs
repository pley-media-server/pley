//! The TV's log file (DESIGN.md §5.7, RES/plx-net-data.md §10).
//!
//! Everything the app prints to stderr (its own lines, panics, other crates'
//! `eprintln!`, the TV libraries' own complaints) goes through a pipe to one
//! writer thread, which stamps each line with the milliseconds since start,
//! scrubs tokens and keyed media addresses out of it, and appends it to
//! `/tmp/app.pley.tv/pley.log`, rotated at 1 MiB into `pley.log.1`. The files
//! are 0640 so webOS Dev Manager, which reads as another user in the app's
//! group, can fetch them. `/tmp` is shared by every app, so the folder and the
//! files are opened without following links and refused unless they are the
//! app's own.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Where the TV's log goes.
pub const LOG_DIR: &str = "/tmp/app.pley.tv";

/// A log file's size before it is rotated.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Sends stderr to the log in `dir` from now on; gives the log file's path.
/// Call it first in `main`, before other threads print. Off the TV (no
/// `dup2`) it does nothing and says so.
pub fn start(dir: &Path) -> io::Result<PathBuf> {
    imp::start(dir)
}

/// Writes one line to the log (stderr, which [`start`] captures).
pub fn line(text: &str) {
    eprintln!("{text}");
}

/// `text` with credentials and keyed addresses taken out: bearer tokens,
/// token and key parameters in queries, forms and JSON, and the key of a
/// `/api/media/<key>/…` address. The call sites keep such values out of their
/// lines; this is the backstop.
pub fn scrub(text: &str) -> String {
    let mut out = text.to_string();
    out = redact_after(&out, "Bearer ", |c| c.is_whitespace() || c == '"' || c == '\'' || c == ',');
    for name in ["access_token", "refresh_token", "id_token", "device_code", "token", "key", "password"] {
        out = redact_after(&out, &format!("{name}="), |c| c == '&' || c == '"' || c == '\'' || c.is_whitespace());
        out = redact_after(&out, &format!("\"{name}\":\""), |c| c == '"');
        out = redact_after(&out, &format!("\"{name}\": \""), |c| c == '"');
    }
    out = redact_after(&out, "/api/media/", |c| c == '/' || c == '?' || c == '"' || c.is_whitespace());
    redact_after(&out, "pley_mk_", |c| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
}

/// Replaces what follows each `marker` up to the first character `stop`
/// takes (or the end) with `…`.
fn redact_after(text: &str, marker: &str, stop: impl Fn(char) -> bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(marker) {
        let (before, after) = rest.split_at(at + marker.len());
        // A `key=` inside a longer name (`apikey=` is still a key, `monkey=` isn't
        // worth the trouble) is redacted all the same: a log loses little.
        out.push_str(before);
        let end = after.find(&stop).unwrap_or(after.len());
        if end > 0 {
            out.push('…');
        }
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

/// `secs` since 1970 as `2026-10-31 21:05:09 UTC`: the one wall-clock line
/// at start, since every other line counts milliseconds from start.
pub fn utc(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC", rem / 3600, rem / 60 % 60, rem % 60)
}

fn now_line() -> String {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    format!("Pley log started {}", utc(secs))
}

#[cfg(unix)]
mod imp {
    use std::fs::{self, File, OpenOptions};
    use std::io::{self, BufRead, BufReader, Write};
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::time::Instant;

    use super::{MAX_FILE_BYTES, now_line, scrub};

    pub fn start(dir: &Path) -> io::Result<PathBuf> {
        own_dir(dir)?;
        let path = dir.join("pley.log");
        let mut file = open_own(&path)?;
        file.set_len(0)?;
        writeln!(file, "{}", now_line())?;

        let mut fds = [0; 2];
        // SAFETY: `fds` has room for the two descriptors pipe(2) writes.
        if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: pipe(2) just made both descriptors and nothing else owns them.
        let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
        let started = Instant::now();
        let target = path.clone();
        std::thread::Builder::new().name("pley-log".into()).spawn(move || {
            let mut writer = Writer { path: target, file, written: 0 };
            let reader = BufReader::new(File::from(read));
            for line in reader.split(b'\n') {
                let Ok(line) = line else { break };
                let text = String::from_utf8_lossy(&line);
                let ms = started.elapsed().as_millis();
                writer.write(&format!("[{:>6}.{:03}] {}\n", ms / 1000, ms % 1000, scrub(&text)));
            }
        })?;
        // SAFETY: both are valid descriptors; fd 2 now refers to the pipe and the
        // original pipe descriptor is closed when `write` drops.
        if unsafe { libc::dup2(std::os::fd::AsRawFd::as_raw_fd(&write), libc::STDERR_FILENO) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(path)
    }

    struct Writer {
        path: PathBuf,
        file: File,
        written: u64,
    }

    impl Writer {
        fn write(&mut self, line: &str) {
            if self.written + line.len() as u64 > MAX_FILE_BYTES {
                let _ = fs::rename(&self.path, self.path.with_extension("log.1"));
                if let Ok(file) = open_own(&self.path) {
                    let _ = file.set_len(0);
                    self.file = file;
                }
                self.written = 0;
            }
            // One write per line: a failed write (a full /tmp) loses that line only.
            if self.file.write_all(line.as_bytes()).is_ok() {
                self.written += line.len() as u64;
            }
        }
    }

    /// The log folder, made 0750 if it isn't there; refused when it is a link
    /// or someone else's (another app could have made it in the shared `/tmp`).
    fn own_dir(dir: &Path) -> io::Result<()> {
        match fs::DirBuilder::new().mode(0o750).create(dir) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        let meta = fs::symlink_metadata(dir)?;
        // SAFETY: geteuid has no preconditions.
        if !meta.is_dir() || meta.uid() != unsafe { libc::geteuid() } {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the log folder isn't the app's"));
        }
        fs::set_permissions(dir, fs::Permissions::from_mode(0o750))
    }

    /// A log file opened for appending without following a link, refused
    /// unless it is a regular file of the app's with one name (a hard link to
    /// another file would otherwise be widened to 0640), then made 0640.
    fn open_own(path: &Path) -> io::Result<File> {
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o640)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)?;
        let meta = file.metadata()?;
        // SAFETY: geteuid has no preconditions.
        if !meta.is_file() || meta.uid() != unsafe { libc::geteuid() } || meta.nlink() != 1 {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the log file isn't the app's"));
        }
        file.set_permissions(fs::Permissions::from_mode(0o640))?;
        Ok(file)
    }
}

#[cfg(not(unix))]
mod imp {
    use std::io;
    use std::path::{Path, PathBuf};

    pub fn start(_dir: &Path) -> io::Result<PathBuf> {
        // The simulator's stderr is the terminal; there is no file to keep.
        let _ = super::now_line;
        Err(io::Error::new(io::ErrorKind::Unsupported, "the TV's log runs only on the TV"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_and_keyed_addresses_never_reach_the_file() {
        assert_eq!(scrub("Authorization: Bearer abc.def-ghi"), "Authorization: Bearer …");
        assert_eq!(scrub("GET /api/media/pley_mk_9f8e7d/master.m3u8?x=1"), "GET /api/media/…/master.m3u8?x=1");
        assert_eq!(scrub("load http://h/x?access_token=secret&a=1"), "load http://h/x?access_token=…&a=1");
        assert_eq!(scrub(r#"{"refresh_token":"r1","id":3}"#), r#"{"refresh_token":"…","id":3}"#);
        assert_eq!(scrub(r#"{"token": "t"}"#), r#"{"token": "…"}"#);
        assert_eq!(scrub("uri pley_mk_ABC-12_x/file.mkv"), "uri pley_mk_…/file.mkv");
        assert_eq!(scrub("nothing to hide"), "nothing to hide");
        assert_eq!(scrub("ends with Bearer "), "ends with Bearer ");
    }

    #[test]
    fn the_start_line_names_the_time_in_utc() {
        assert_eq!(utc(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(utc(951_782_400), "2000-02-29 00:00:00 UTC");
        assert_eq!(utc(1_793_478_309), "2026-10-31 20:25:09 UTC");
        assert!(now_line().starts_with("Pley log started 20"));
    }
}

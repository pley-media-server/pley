//! The app's log: lines to stderr and to a small file that rotates, written by one thread so the
//! main loop never waits on the disk. Tokens, codes and keyed addresses never reach either: every
//! line is redacted before it leaves.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, Sender};

use log::{LevelFilter, Log, Metadata, Record};

/// The log file rotates past this size, keeping one older file.
pub const ROTATE_AT: u64 = 512 * 1024;

struct Logger {
    level: LevelFilter,
    file: Option<Mutex<Sender<String>>>,
}

impl Log for Logger {
    fn enabled(&self, meta: &Metadata) -> bool {
        meta.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let line = redact(&format!(
            "{:9.3} {:5} {}: {}",
            pley_base::time::now(),
            record.level(),
            record.target(),
            record.args()
        ));
        eprintln!("{line}");
        if let Some(tx) = &self.file {
            let _ = tx.lock().unwrap_or_else(|e| e.into_inner()).send(line);
        }
    }

    fn flush(&self) {}
}

/// Starts the log: stderr, and `dir/pley.log` when a folder is given (made when missing).
pub fn init(dir: Option<&Path>, verbose: bool) {
    let level = if verbose { LevelFilter::Debug } else { LevelFilter::Info };
    let file = dir.and_then(|d| fs::create_dir_all(d).ok().map(|_| d.join("pley.log"))).map(|path| {
        let (tx, rx) = mpsc::channel::<String>();
        let _ = std::thread::Builder::new().name("pley-log".into()).spawn(move || {
            let mut out = Writer::open(path);
            for line in rx {
                out.write(&line);
            }
        });
        Mutex::new(tx)
    });
    // Our own crates at the level asked; others' (the windowing ones are chatty) at warnings.
    let logger = Logger { level, file };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(level);
    }
}

/// The log file, rotated past [`ROTATE_AT`].
struct Writer {
    path: PathBuf,
    file: Option<File>,
    size: u64,
}

impl Writer {
    fn open(path: PathBuf) -> Writer {
        let file = open_append(&path);
        let size = file.as_ref().and_then(|f| f.metadata().ok()).map_or(0, |m| m.len());
        Writer { path, file, size }
    }

    fn write(&mut self, line: &str) {
        if self.size > ROTATE_AT {
            self.file = None;
            let mut old = self.path.clone().into_os_string();
            old.push(".1");
            let _ = fs::rename(&self.path, old);
            self.file = open_append(&self.path);
            self.size = 0;
        }
        if let Some(f) = self.file.as_mut()
            && writeln!(f, "{line}").is_ok()
        {
            self.size += line.len() as u64 + 1;
        }
    }
}

fn open_append(path: &Path) -> Option<File> {
    let mut o = OpenOptions::new();
    o.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o640);
    }
    o.open(path).ok()
}

/// What a secret's value is replaced with.
const HIDDEN: &str = "…";

/// `line` with secrets taken out: bearer tokens, token and code parameters (in queries, forms and
/// JSON), the key of keyed media addresses (`/api/media/pley_mk_…`), and the folders and names of
/// a file on a USB drive (`/tmp/usb/sda/sda1/….mkv`).
pub fn redact(line: &str) -> String {
    let mut out = line.to_string();
    for marker in ["Bearer ", "bearer "] {
        out = mask_after(&out, marker, |c| c.is_whitespace() || c == '"' || c == ',');
    }
    for name in ["access_token", "refresh_token", "device_code", "user_code", "token", "code", "pin", "password"] {
        out = mask_after(&out, &format!("{name}="), |c| c == '&' || c == '"' || c.is_whitespace());
        out = mask_after(&out, &format!("\"{name}\":\""), |c| c == '"');
        out = mask_after(&out, &format!("\"{name}\": \""), |c| c == '"');
    }
    out = mask_after(&out, "pley_mk_", |c| c == '/' || c == '?' || c == '"' || c.is_whitespace());
    pley_core::device::hide_drive_paths(&out)
}

/// Replaces what follows each `marker` up to a character `end` accepts with [`HIDDEN`].
fn mask_after(s: &str, marker: &str, end: impl Fn(char) -> bool) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find(marker) {
        let (before, after) = rest.split_at(at + marker.len());
        out.push_str(before);
        let stop = after.find(&end).unwrap_or(after.len());
        if stop > 0 {
            out.push_str(HIDDEN);
        }
        rest = &after[stop..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_never_reach_the_log() {
        assert_eq!(redact("Authorization: Bearer abc.def-ghi"), "Authorization: Bearer …");
        assert_eq!(
            redact("POST grant_type=x&refresh_token=r123&client_id=c"),
            "POST grant_type=x&refresh_token=…&client_id=c"
        );
        assert_eq!(redact(r#"{"access_token":"a1","expires_in":3600}"#), r#"{"access_token":"…","expires_in":3600}"#);
        assert_eq!(
            redact("load http://10.0.0.2:7539/api/media/pley_mk_9f8e7d/master.m3u8"),
            "load http://10.0.0.2:7539/api/media/pley_mk_…/master.m3u8"
        );
        assert_eq!(redact("device_code=dc&user_code=ABCD-EFGH"), "device_code=…&user_code=…");
        assert_eq!(redact("page home opens"), "page home opens");
        assert_eq!(redact("load file:///tmp/usb/sdb/sdb1/Kids/Frozen.mkv"), "load file:///tmp/usb/sdb/sdb1/….mkv");
    }

    #[test]
    fn the_file_rotates() {
        let dir = pley_core::device::scratch_dir("log-test");
        let path = dir.join("pley.log");
        let mut w = Writer::open(path.clone());
        let line = "x".repeat(1000);
        for _ in 0..(ROTATE_AT / 1000 + 5) {
            w.write(&line);
        }
        assert!(dir.join("pley.log.1").exists());
        assert!(fs::metadata(&path).unwrap().len() < ROTATE_AT);
        let _ = fs::remove_dir_all(dir);
    }
}

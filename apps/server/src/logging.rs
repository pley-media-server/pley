use std::fs::{File, OpenOptions};
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, Once, PoisonError};

use tracing::Metadata;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::fmt::writer::EitherWriter;
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

/// Start a new log file once the current one is larger than this.
const ROTATE_BYTES: u64 = 10 * 1024 * 1024;
/// Earlier log files kept: `pley.log.1` (the latest) to `pley.log.3`.
const KEEP: usize = 3;

/// Target of the events that report panics (see [`install_panic_hook`]).
const PANIC_TARGET: &str = "pley::panic";

/// Cleared when the process leaves its console window (the Windows tray
/// does); log lines then only go to the log file.
static CONSOLE: AtomicBool = AtomicBool::new(true);

/// Stop writing log lines to the console. Call before detaching from it.
pub fn console_detached() {
    CONSOLE.store(false, Ordering::SeqCst);
}

/// Standard error while there is a console, nothing after.
struct Console;

impl<'a> MakeWriter<'a> for Console {
    type Writer = EitherWriter<std::io::Stderr, std::io::Sink>;

    fn make_writer(&'a self) -> Self::Writer {
        if CONSOLE.load(Ordering::Relaxed) {
            EitherWriter::A(std::io::stderr())
        } else {
            EitherWriter::B(std::io::sink())
        }
    }

    /// Panics are left out: Rust's own message is printed there already.
    fn make_writer_for(&'a self, meta: &Metadata<'_>) -> Self::Writer {
        if meta.target() == PANIC_TARGET { EitherWriter::B(std::io::sink()) } else { self.make_writer() }
    }
}

/// Initialize tracing and log panics (see [`install_panic_hook`]). `filter`
/// uses `EnvFilter` syntax; a bare level such as `info` keeps noisy
/// dependencies at `warn`. With `file`, logs also go to that file (the
/// server's, so they survive without a console window), rotated by size.
pub fn init(filter: &str, file: Option<&Path>) {
    let filter = if filter.contains('=') || filter.contains(',') {
        filter.to_owned()
    } else {
        format!("{filter},sqlx=warn,hyper=warn,tower_http=warn,reqwest=warn,zbus=warn")
    };
    let env_filter = EnvFilter::try_new(&filter).unwrap_or_else(|_| EnvFilter::new("info"));
    let stderr = fmt::layer()
        .with_target(false)
        .with_ansi(std::io::stderr().is_terminal())
        .log_internal_errors(false)
        .with_writer(Console)
        .compact();
    let file = file.map(|path| {
        let file = LogFile::open(path, ROTATE_BYTES, KEEP);
        fmt::layer().with_target(false).with_ansi(false).log_internal_errors(false).with_writer(file).compact()
    });
    let _ = tracing_subscriber::registry().with(env_filter).with(stderr).with(file).try_init();
    install_panic_hook();
}

/// Log panics through tracing: the log file gets the message, where it
/// happened and a backtrace, then the previous hook prints Rust's usual
/// message to the console. Once the tray has detached from the console,
/// standard error leads nowhere and the log file is the only trace.
fn install_panic_hook() {
    static INSTALLED: Once = Once::new();
    INSTALLED.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let payload = info.payload();
            let message = payload
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                .unwrap_or("Box<dyn Any>");
            let location = info.location().map_or_else(|| "an unknown place".to_owned(), ToString::to_string);
            let thread = std::thread::current();
            let backtrace = std::backtrace::Backtrace::force_capture();
            tracing::error!(
                target: PANIC_TARGET,
                "thread '{}' panicked at {location}: {message}\nbacktrace:\n{backtrace}",
                thread.name().unwrap_or("<unnamed>")
            );
            previous(info);
        }));
    });
}

/// The log file, rotated by size while pley runs: once `pley.log` reached
/// its limit, it becomes `pley.log.1` (`.1` becomes `.2` and so on, the
/// oldest is deleted) and a new `pley.log` is started. The size is checked
/// before each event, so an event is never split over two files.
struct LogFile {
    path: PathBuf,
    limit: u64,
    /// Earlier files kept (at least 1).
    keep: usize,
    current: Mutex<Current>,
}

struct Current {
    /// `None` while the file can't be opened; tried again at the next event.
    file: Option<File>,
    /// Size of the file when it was opened plus what was written since.
    size: u64,
}

impl LogFile {
    fn open(path: &Path, limit: u64, keep: usize) -> Self {
        let log = Self {
            path: path.to_owned(),
            limit,
            keep: keep.max(1),
            current: Mutex::new(Current { file: None, size: 0 }),
        };
        log.reopen(&mut log.lock());
        log
    }

    fn lock(&self) -> MutexGuard<'_, Current> {
        self.current.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `pley.log` with `suffix` added, e.g. `pley.log.1`.
    fn sibling(&self, suffix: impl std::fmt::Display) -> PathBuf {
        let mut name = self.path.as_os_str().to_owned();
        name.push(format!(".{suffix}"));
        name.into()
    }

    /// Open the file (again), appending to it.
    fn reopen(&self, current: &mut Current) {
        let file = match self.path.parent() {
            Some(dir) => std::fs::create_dir_all(dir),
            None => Ok(()),
        }
        .and_then(|()| OpenOptions::new().create(true).append(true).open(&self.path));
        current.size = file.as_ref().ok().and_then(|f| f.metadata().ok()).map_or(0, |m| m.len());
        current.file = file.ok();
    }

    fn rotate(&self, current: &mut Current) {
        // Closed first: Windows renames open files only when every program
        // that has them open allows it.
        current.file = None;
        // Moved aside before anything else, so that when another program
        // holds it, the earlier files are left as they are.
        let staged = self.sibling("rotating");
        let moved = std::fs::rename(&self.path, &staged).is_ok();
        if moved {
            let _ = std::fs::remove_file(self.sibling(self.keep));
            for n in (1..self.keep).rev() {
                let _ = std::fs::rename(self.sibling(n), self.sibling(n + 1));
            }
            let _ = std::fs::rename(&staged, self.sibling(1));
        }
        self.reopen(current);
        if !moved {
            // Keep appending to it, and try again after another `limit` bytes.
            current.size = 0;
        }
    }
}

impl<'a> MakeWriter<'a> for LogFile {
    type Writer = LogWriter<'a>;

    fn make_writer(&'a self) -> Self::Writer {
        let mut current = self.lock();
        if current.file.is_none() {
            self.reopen(&mut current);
        } else if current.size >= self.limit {
            self.rotate(&mut current);
        }
        LogWriter(current)
    }
}

/// Writes one event to the log file, which it holds meanwhile.
struct LogWriter<'a>(MutexGuard<'a, Current>);

impl Write for LogWriter<'_> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let current = &mut *self.0;
        let Some(file) = current.file.as_mut() else { return Ok(buf.len()) };
        let written = file.write(buf)?;
        current.size += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.file.as_mut().map_or(Ok(()), Write::flush)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(log: &LogFile, line: &str) {
        log.make_writer().write_all(line.as_bytes()).unwrap();
    }

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap_or_default()
    }

    /// Lines of 40 bytes: "07 xxx…\n".
    fn line(n: usize) -> String {
        format!("{n:02} {}\n", "x".repeat(36))
    }

    #[test]
    fn the_log_is_rotated_by_size_while_it_is_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("logs").join("pley.log");
        let log = LogFile::open(&path, 100, 2);
        for n in 0..12 {
            event(&log, &line(n));
        }
        // Three whole lines per file (the size is checked before each one);
        // the two files before the current one are kept.
        assert_eq!(read(&path), [9, 10, 11].map(line).concat());
        assert_eq!(read(&log.sibling(1)), [6, 7, 8].map(line).concat());
        assert_eq!(read(&log.sibling(2)), [3, 4, 5].map(line).concat());
        assert!(!log.sibling(3).exists() && !log.sibling("rotating").exists());
    }

    #[test]
    fn a_large_log_of_an_earlier_run_is_rotated_first() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pley.log");
        std::fs::write(&path, "x".repeat(150)).unwrap();
        std::fs::write(dir.path().join("pley.log.1"), "older").unwrap();
        let log = LogFile::open(&path, 100, 3);
        event(&log, "new\n");
        assert_eq!(read(&path), "new\n");
        assert_eq!(read(&log.sibling(1)).len(), 150);
        assert_eq!(read(&log.sibling(2)), "older");
    }

    #[test]
    fn a_log_that_cannot_be_opened_is_tried_again() {
        let dir = tempfile::tempdir().unwrap();
        let in_the_way = dir.path().join("logs");
        std::fs::write(&in_the_way, "a file where the folder goes").unwrap();
        let path = in_the_way.join("pley.log");
        let log = LogFile::open(&path, 100, 3);
        event(&log, "lost\n");
        std::fs::remove_file(&in_the_way).unwrap();
        event(&log, "back\n");
        assert_eq!(read(&path), "back\n");
    }

    /// A program that reads the log without letting it be renamed (some log
    /// viewers on Windows): pley goes on appending, then tries again later.
    #[cfg(windows)]
    #[test]
    fn a_log_held_open_elsewhere_is_rotated_once_it_is_let_go() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pley.log");
        let log = LogFile::open(&path, 100, 3);
        std::fs::write(log.sibling(1), "older").unwrap();
        for n in 0..3 {
            event(&log, &line(n));
        }
        let viewer = OpenOptions::new().read(true).share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE).open(&path).unwrap();
        event(&log, &line(3));
        assert_eq!(read(&path), [0, 1, 2, 3].map(line).concat(), "appended");
        assert_eq!(read(&log.sibling(1)), "older", "earlier files untouched");

        drop(viewer);
        for n in 4..7 {
            event(&log, &line(n));
        }
        assert_eq!(read(&path), line(6), "rotated after another 100 bytes");
        assert_eq!(read(&log.sibling(1)), [0, 1, 2, 3, 4, 5].map(line).concat());
        assert_eq!(read(&log.sibling(2)), "older");
    }
}

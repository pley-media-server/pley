//! Temporary folders for tests whose files stay open after the test: a
//! database, a server, a transcoder.
//!
//! A [`tempfile::TempDir`] removes its folder when dropped, but Windows
//! doesn't remove a file that's open, and these close theirs only after the
//! test's own values are gone: the database's connections close on threads
//! of their own, and whatever the test's runtime spawned holds on to them
//! until that runtime stops. The removal fails, silently, and the folder
//! stays in the temporary folder for good. [`tempdir`]'s folders are removed
//! once the thread that made them ends instead (a test's, after its
//! runtime), trying again while the last files close.
//!
//! The integration tests use this file too (`tests/common`).

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// How long a thread's folders may take to be let go of once it ends.
const GRACE: Duration = Duration::from_secs(10);

/// A temporary folder, removed when this thread ends.
pub fn tempdir() -> tempfile::TempDir {
    let mut dir = tempfile::tempdir().unwrap();
    dir.disable_cleanup(true);
    LEFT.with(|left| left.0.borrow_mut().push(dir.path().to_owned()));
    dir
}

/// The folders this thread made, removed when it ends.
struct Left(RefCell<Vec<PathBuf>>);

impl Drop for Left {
    fn drop(&mut self) {
        let deadline = Instant::now() + GRACE;
        for dir in self.0.take() {
            loop {
                match std::fs::remove_dir_all(&dir) {
                    Ok(()) => break,
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => break,
                    Err(err) if Instant::now() >= deadline => {
                        eprintln!("could not remove {}: {err}", dir.display());
                        break;
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(20)),
                }
            }
        }
    }
}

thread_local! {
    static LEFT: Left = const { Left(RefCell::new(Vec::new())) };
}

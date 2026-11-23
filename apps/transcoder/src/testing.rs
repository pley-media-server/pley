// SPDX-License-Identifier: AGPL-3.0-only
//! What the tests share: FFmpeg, which the tests that run it look for first,
//! and temporary folders for tests whose files stay open after the test (a
//! transcoder's, its jobs' FFmpeg).
//!
//! A test that runs FFmpeg is skipped where FFmpeg, ffprobe, or an encoder or
//! filter it needs isn't there, unless `PLEY_TEST_FFMPEG=1` (which CI sets):
//! then it fails. What only some builds or versions of FFmpeg have (libx265
//! with HDR10+, FFmpeg 7.1's `dovi_rpu` filter) is skipped either way, saying
//! so.
//!
//! A [`tempfile::TempDir`] removes its folder when dropped, but Windows
//! doesn't remove a file that's open, and these close theirs only after the
//! test's own values are gone: whatever the test's runtime spawned holds on
//! to them until that runtime stops, and a killed FFmpeg lets go of them
//! once it has exited. The removal fails, silently, and the folder stays in
//! the temporary folder for good. [`tempdir`]'s folders are removed once the
//! thread that made them ends instead (a test's, after its runtime), trying
//! again while the last files close.

use std::cell::RefCell;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};

/// Whether the tests that run FFmpeg must run: `PLEY_TEST_FFMPEG=1`.
pub fn ffmpeg_required() -> bool {
    std::env::var("PLEY_TEST_FFMPEG").is_ok_and(|v| v.trim() == "1")
}

/// Skips a test that needs what `missing` says isn't here, saying so; or,
/// where FFmpeg's tests must run ([`ffmpeg_required`]), fails it.
#[track_caller]
pub fn skip(missing: &str) {
    assert!(!ffmpeg_required(), "{missing}, and PLEY_TEST_FFMPEG=1 needs it: install it, or unset PLEY_TEST_FFMPEG");
    eprintln!("{missing}: skipped");
}

/// Whether FFmpeg and ffprobe run from `PATH`; a test is [skipped](skip)
/// without them.
#[track_caller]
pub fn has_ffmpeg() -> bool {
    let runs = |tool: &str| Command::new(tool).arg("-version").output().is_ok_and(|o| o.status.success());
    let found = runs("ffmpeg") && runs("ffprobe");
    if !found {
        skip("ffmpeg not found");
    }
    found
}

/// What FFmpeg lists with `-encoders`, `-filters` or `-bsfs`.
fn listed(what: &str) -> String {
    let out = Command::new("ffmpeg").args(["-hide_banner", what]).output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
}

/// Whether FFmpeg and ffprobe are here ([`has_ffmpeg`]) and FFmpeg has the
/// encoder `name` (`libx264`); a test is [skipped](skip) otherwise.
#[track_caller]
pub fn has_encoder(name: &str) -> bool {
    has_ffmpeg() && has_named(&listed("-encoders"), name, "encoder")
}

/// Whether FFmpeg and ffprobe are here and FFmpeg has the filter `name`
/// (`subtitles`, which libass brings); a test is [skipped](skip) otherwise.
#[track_caller]
pub fn has_filter(name: &str) -> bool {
    has_ffmpeg() && has_named(&listed("-filters"), name, "filter")
}

#[track_caller]
fn has_named(list: &str, name: &str, kind: &str) -> bool {
    let found = names(list).any(|listed| listed == name);
    if !found {
        skip(&format!("FFmpeg without the {name} {kind}"));
    }
    found
}

/// The names `-encoders` or `-filters` lists, each after its flags.
fn names(list: &str) -> impl Iterator<Item = &str> {
    list.lines().filter_map(|line| line.split_whitespace().nth(1))
}

/// Whether FFmpeg has the bitstream filter `name`, which FFmpeg has had
/// since `version` (major, minor): skipped, saying so, where it is older;
/// [skipped](skip) where it is as new and hasn't it.
#[track_caller]
pub fn has_bitstream_filter(name: &str, version: (u32, u32)) -> bool {
    if listed("-bsfs").lines().any(|line| line.trim() == name) {
        return true;
    }
    let out = Command::new("ffmpeg").arg("-version").output().map(|o| o.stdout).unwrap_or_default();
    match version_of(&String::from_utf8_lossy(&out)) {
        Some(older) if older < version => {
            eprintln!(
                "FFmpeg {}.{} without the {name} filter ({}.{} and later): skipped",
                older.0, older.1, version.0, version.1
            );
        }
        _ => skip(&format!("FFmpeg without the {name} filter")),
    }
    false
}

/// The version (major, minor) `ffmpeg -version` says, when it says one:
/// `ffmpeg version 6.1.1-3ubuntu5`, `n7.1`; a build of FFmpeg's latest code
/// says none.
fn version_of(text: &str) -> Option<(u32, u32)> {
    let version = text.split_whitespace().nth(2)?.trim_start_matches('n');
    let mut numbers = version.split(|c: char| !c.is_ascii_digit()).map(str::parse::<u32>);
    Some((numbers.next()?.ok()?, numbers.next()?.ok()?))
}

#[test]
fn ffmpeg_says_its_version_and_what_it_has() {
    let version = |line: &str| version_of(&format!("ffmpeg version {line} Copyright (c) 2000-2026"));
    assert_eq!(version("6.1.1-3ubuntu5"), Some((6, 1)));
    assert_eq!(version("n7.1"), Some((7, 1)));
    assert_eq!(version("9.0.2-full_build-www.gyan.dev"), Some((9, 0)));
    assert_eq!(version("N-121212-g0123456789"), None);
    let encoders = "Encoders:\n V..... = Video\n ------\n V....D libx264              libx264 H.264\n A....D aac                  AAC";
    assert_eq!(names(encoders).collect::<Vec<_>>(), ["=", "libx264", "aac"]);
    let filters = " TSC subtitles         V->V       Render text subtitles onto input video using the libass library.";
    assert_eq!(names(filters).collect::<Vec<_>>(), ["subtitles"]);
}

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

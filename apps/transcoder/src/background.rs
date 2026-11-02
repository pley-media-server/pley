// SPDX-License-Identifier: AGPL-3.0-only
//! Background work keeps out of the way of playbacks and transcodes.
//!
//! Seek previews, audio fingerprints, the key frames of credits, keyframe
//! scans and library probes read whole files, or long stretches of them, from
//! the disk or network share the playbacks read too: on a network share a 4K
//! remux needs all of it to start quickly. While a playback starts, none of
//! them starts, and the FFmpeg runs under way are paused (their process
//! suspended), then resumed once the start is over. A playback starts while a
//! job is being created, while one of its FFmpeg processes (Play, a seek, a
//! restart) hasn't made its first segments yet, and while the server says a
//! direct play, which it serves itself, is starting ([`Background::hold`]).
//!
//! Transcodes come first too: while a job's FFmpeg or a conversion runs (it
//! holds a transcode slot, see [`Background::transcode`]), background FFmpeg
//! runs wait and are paused the same way, until a few seconds after the last
//! one ends ([`AFTER_TRANSCODE`]). So do picture subtitles a player waits
//! for, while they are read through a file's Cues ([`Background::pictures`]):
//! on a share's disks each of their blocks is a seek, and a seek preview's
//! FFmpeg reading a remux beside them makes each one wait its turn. Probes
//! and reading ahead (`prefetch.rs`) wait only for starts: they read little,
//! and a new file or the next playback shouldn't wait for a film to end.
//!
//! A paused run only waits: nothing is lost, and its time limit counts the
//! time it ran.

use std::collections::HashMap;
use std::process::{ExitStatus, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::time::Duration;

use parking_lot::Mutex;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};
use tokio::sync::watch;
use tokio::time::Instant;

use crate::error::{ApiError, ApiResult};

/// The longest a playback start holds background work (a process whose
/// first segments are slow to come, a job creation probing a sleeping disk).
pub const MAX_START: Duration = Duration::from_secs(20);
/// The longest a hold asked for from outside lasts.
pub const MAX_HOLD: Duration = Duration::from_secs(60);
/// Background work waits this long after the last transcode ends: a job's
/// FFmpeg restarting (a seek, another preset) ends one before the next.
pub const AFTER_TRANSCODE: Duration = Duration::from_secs(5);
/// Background work waits this long after the last read of picture subtitles
/// ends: a player's next ask, or the next stretch of a track, comes soon.
pub const AFTER_PICTURES: Duration = Duration::from_secs(2);

#[derive(Default)]
struct State {
    /// Playbacks starting, by id: until when each holds background work at most.
    starts: HashMap<u64, Instant>,
    next_id: u64,
    /// Until when the server asked for background work to wait.
    held_until: Option<Instant>,
    /// Transcodes running (see [`Background::transcode`]).
    transcodes: usize,
    /// Until when the last transcode to end holds background work.
    transcoded_until: Option<Instant>,
    /// Reads of picture subtitles under way (see [`Background::pictures`]).
    picture_reads: usize,
    /// Until when the last of them to end holds background work.
    pictures_read_until: Option<Instant>,
}

impl State {
    /// When what waits only for starts may go on again, if it must wait now.
    fn starting_until(&self, now: Instant) -> Option<Instant> {
        self.starts.values().copied().chain(self.held_until).filter(|until| *until > now).max()
    }

    /// When background work may go on again, if it must wait now. While a
    /// transcode or a read of picture subtitles runs that is unknown: a while
    /// from now, and its end says so.
    fn held_until(&self, now: Instant) -> Option<Instant> {
        if self.transcodes > 0 || self.picture_reads > 0 {
            return Some(now + MAX_HOLD);
        }
        let after = [self.transcoded_until, self.pictures_read_until].into_iter().flatten();
        self.starting_until(now).into_iter().chain(after.filter(|until| *until > now)).max()
    }
}

pub struct Background {
    state: Mutex<State>,
    /// Sent on every change: waiting and paused runs look again.
    changed: watch::Sender<()>,
    /// Background processes paused now.
    paused: AtomicUsize,
}

impl Background {
    pub fn new() -> Arc<Self> {
        Arc::new(Self { state: Mutex::default(), changed: watch::Sender::new(()), paused: AtomicUsize::new(0) })
    }

    /// A playback starts: background work waits until the guard is dropped,
    /// or [`MAX_START`] at most.
    pub fn start(self: &Arc<Self>) -> Starting {
        let mut state = self.state.lock();
        let id = state.next_id;
        state.next_id += 1;
        state.starts.insert(id, Instant::now() + MAX_START);
        drop(state);
        self.changed.send_replace(());
        Starting { background: self.clone(), id }
    }

    /// Background work waits for `span` ([`MAX_HOLD`] at most) from now, or
    /// longer when held already.
    pub fn hold(&self, span: Duration) {
        let until = Instant::now() + span.min(MAX_HOLD);
        let mut state = self.state.lock();
        if state.held_until.is_none_or(|held| held < until) {
            state.held_until = Some(until);
        }
        drop(state);
        self.changed.send_replace(());
    }

    /// A transcode runs (a job's FFmpeg, a conversion): background work
    /// waits until the guard is dropped, and [`AFTER_TRANSCODE`] longer.
    pub fn transcode(self: &Arc<Self>) -> Transcoding {
        self.state.lock().transcodes += 1;
        self.changed.send_replace(());
        Transcoding { background: self.clone() }
    }

    /// Picture subtitles a player waits for are read through a file's Cues
    /// (the blocks around it, or the rest of its track): background work
    /// waits until the guard is dropped, and [`AFTER_PICTURES`] longer.
    pub fn pictures(self: &Arc<Self>) -> ReadingPictures {
        self.state.lock().picture_reads += 1;
        self.changed.send_replace(());
        ReadingPictures { background: self.clone() }
    }

    /// Whether background work must wait now.
    #[cfg(test)]
    pub fn held(&self) -> bool {
        self.held_until().is_some()
    }

    /// Whether a playback is starting (or the server asked for a hold): what
    /// reads little (probes, reading ahead) waits only for that.
    pub fn starting(&self) -> bool {
        self.state.lock().starting_until(Instant::now()).is_some()
    }

    /// Background processes paused now.
    pub fn paused(&self) -> usize {
        self.paused.load(Relaxed)
    }

    fn held_until(&self) -> Option<Instant> {
        self.state.lock().held_until(Instant::now())
    }

    /// Resolves once background work may go on.
    pub async fn wait(&self) {
        self.wait_while(State::held_until).await;
    }

    /// Resolves once no playback is starting (see [`Background::starting`]).
    pub async fn wait_for_starts(&self) {
        self.wait_while(State::starting_until).await;
    }

    async fn wait_while(&self, held: fn(&State, Instant) -> Option<Instant>) {
        let mut changed = self.changed.subscribe();
        // Seen before the look: no change slips in between.
        changed.mark_unchanged();
        loop {
            let Some(until) = held(&self.state.lock(), Instant::now()) else { break };
            tokio::select! {
                _ = changed.changed() => {}
                () = tokio::time::sleep_until(until) => {}
            }
        }
    }

    /// Run FFmpeg (or FFprobe) as background work: once nothing starts or
    /// transcodes, and paused while something does. Its output and log; an
    /// error when it fails or runs longer than `timeout`.
    pub async fn run(&self, mut cmd: Command, timeout: Duration) -> ApiResult<(Vec<u8>, String)> {
        self.wait().await;
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| ApiError::Internal(anyhow::anyhow!("failed to start ffmpeg: {e}")))?;
        let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
            return Err(ApiError::Internal(anyhow::anyhow!("ffmpeg pipes unavailable")));
        };
        let (status, stdout, stderr) =
            tokio::join!(self.supervise(&mut child, timeout), read_all(stdout), read_all(stderr));
        let stderr = String::from_utf8_lossy(&stderr).trim().to_owned();
        if !status?.success() {
            let message = stderr.lines().last().unwrap_or("ffmpeg failed").to_owned();
            return Err(ApiError::Failed(message));
        }
        Ok((stdout, stderr))
    }

    /// Wait for a background process to end, pausing it while a playback
    /// starts or a transcode runs; it is stopped once it has run for
    /// `timeout`, paused time not counted.
    pub async fn supervise(&self, child: &mut Child, timeout: Duration) -> ApiResult<ExitStatus> {
        let mut changed = self.changed.subscribe();
        changed.mark_unchanged();
        let mut left = timeout;
        let mut paused = Paused { background: self, on: false };
        loop {
            let until = self.held_until();
            if until.is_some() != paused.on && pause(child, until.is_some()) {
                paused.set(until.is_some());
                let what = if paused.on { "paused: playback comes first" } else { "resumed" };
                tracing::debug!("background process {} {what}", child.id().unwrap_or_default());
            }
            let since = Instant::now();
            tokio::select! {
                status = child.wait() => {
                    // Not resumed: it is gone.
                    paused.set(false);
                    return status.map_err(|e| ApiError::Internal(e.into()));
                }
                _ = changed.changed() => {}
                () = tokio::time::sleep_until(until.unwrap_or(since)), if until.is_some() => {}
                () = tokio::time::sleep(left), if !paused.on => {
                    let _ = child.start_kill();
                    let _ = child.wait().await;
                    paused.set(false);
                    return Err(ApiError::Timeout("ffmpeg took too long".into()));
                }
            }
            if !paused.on {
                left = left.saturating_sub(since.elapsed());
            }
        }
    }
}

/// A playback starting (see [`Background::start`]); over when dropped.
pub struct Starting {
    background: Arc<Background>,
    id: u64,
}

impl Drop for Starting {
    fn drop(&mut self) {
        self.background.state.lock().starts.remove(&self.id);
        self.background.changed.send_replace(());
    }
}

/// A transcode running (see [`Background::transcode`]); over when dropped.
pub struct Transcoding {
    background: Arc<Background>,
}

impl Drop for Transcoding {
    fn drop(&mut self) {
        let mut state = self.background.state.lock();
        state.transcodes = state.transcodes.saturating_sub(1);
        state.transcoded_until = Some(Instant::now() + AFTER_TRANSCODE);
        drop(state);
        self.background.changed.send_replace(());
    }
}

/// A read of picture subtitles under way (see [`Background::pictures`]);
/// over when dropped.
pub struct ReadingPictures {
    background: Arc<Background>,
}

impl Drop for ReadingPictures {
    fn drop(&mut self) {
        let mut state = self.background.state.lock();
        state.picture_reads = state.picture_reads.saturating_sub(1);
        state.pictures_read_until = Some(Instant::now() + AFTER_PICTURES);
        drop(state);
        self.background.changed.send_replace(());
    }
}

/// Counts a supervised process in [`Background::paused`] while it is paused.
struct Paused<'a> {
    background: &'a Background,
    on: bool,
}

impl Paused<'_> {
    fn set(&mut self, on: bool) {
        if on != self.on {
            self.on = on;
            if on {
                self.background.paused.fetch_add(1, Relaxed);
            } else {
                self.background.paused.fetch_sub(1, Relaxed);
            }
        }
    }
}

impl Drop for Paused<'_> {
    fn drop(&mut self) {
        // A supervision dropped midway (its request went away) kills the
        // process (`kill_on_drop`): no resuming needed.
        self.set(false);
    }
}

async fn read_all(mut pipe: impl AsyncRead + Unpin) -> Vec<u8> {
    let mut out = Vec::new();
    let _ = pipe.read_to_end(&mut out).await;
    out
}

#[cfg(windows)]
#[link(name = "ntdll.dll", kind = "raw-dylib", modifiers = "+verbatim")]
unsafe extern "system" {
    fn NtSuspendProcess(process: windows_sys::Win32::Foundation::HANDLE) -> i32;
    fn NtResumeProcess(process: windows_sys::Win32::Foundation::HANDLE) -> i32;
}

/// Suspend (`true`) or resume a child process that hasn't been waited for;
/// whether that was done. Only between polls of its `wait`, so that its pid
/// can't have been reaped and given to another process meanwhile.
pub(crate) fn pause(child: &Child, suspend: bool) -> bool {
    #[cfg(windows)]
    {
        let Some(handle) = child.raw_handle() else { return false };
        // SAFETY: the handle is the child's own, open for as long as `child` lives.
        let status = unsafe { if suspend { NtSuspendProcess(handle) } else { NtResumeProcess(handle) } };
        status >= 0
    }
    #[cfg(target_os = "linux")]
    {
        let Some(pid) = child.id() else { return false };
        let signal = if suspend { libc::SIGSTOP } else { libc::SIGCONT };
        // SAFETY: plain syscall; the pid is the unreaped child's (see above).
        unsafe { libc::kill(pid as libc::pid_t, signal) == 0 }
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = (child, suspend);
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(windows, target_os = "linux"))]
    use tokio::io::{AsyncBufReadExt, BufReader};

    #[tokio::test(start_paused = true)]
    async fn starts_hold_background_work_until_they_are_over() {
        let bg = Background::new();
        assert!(!bg.held());
        let one = bg.start();
        let two = bg.start();
        assert!(bg.held());
        drop(one);
        assert!(bg.held(), "the other start goes on");
        let waiting = tokio::spawn({
            let bg = bg.clone();
            async move { bg.wait().await }
        });
        tokio::time::sleep(Duration::from_secs(1)).await;
        assert!(!waiting.is_finished());
        drop(two);
        tokio::time::timeout(Duration::from_millis(10), waiting).await.expect("released").unwrap();
        assert!(!bg.held());
    }

    #[tokio::test(start_paused = true)]
    async fn a_start_holds_for_a_while_at_most() {
        let bg = Background::new();
        let _forgotten = bg.start();
        let started = Instant::now();
        bg.wait().await;
        assert_eq!(started.elapsed().as_secs(), MAX_START.as_secs());
        assert!(!bg.held());
    }

    #[tokio::test(start_paused = true)]
    async fn holds_from_outside_last_what_they_ask_within_a_limit() {
        let bg = Background::new();
        bg.hold(Duration::from_secs(10));
        bg.hold(Duration::from_secs(3));
        let started = Instant::now();
        bg.wait().await;
        assert_eq!(started.elapsed().as_secs(), 10, "a shorter hold doesn't cut a longer one");
        bg.hold(Duration::from_secs(3600));
        let started = Instant::now();
        bg.wait().await;
        assert_eq!(started.elapsed().as_secs(), MAX_HOLD.as_secs());
    }

    /// A process that prints numbers as fast as its pipe takes them, for long.
    #[cfg(any(windows, target_os = "linux"))]
    fn chatty() -> Command {
        let mut cmd = if cfg!(windows) {
            let mut cmd = Command::new("cmd");
            cmd.args(["/c", "for /l %i in (1,1,10000000) do @echo %i"]);
            cmd
        } else {
            let mut cmd = Command::new("sh");
            cmd.args(["-c", "i=0; while [ $i -lt 10000000 ]; do echo $i; i=$((i+1)); done"]);
            cmd
        };
        cmd.kill_on_drop(true).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
        cmd
    }

    /// A process that takes about `secs` seconds and then succeeds.
    #[cfg(any(windows, target_os = "linux"))]
    fn slow(secs: u32) -> Command {
        let mut cmd = if cfg!(windows) {
            let mut cmd = Command::new("ping");
            cmd.args(["-n", &(secs + 1).to_string(), "127.0.0.1"]);
            cmd
        } else {
            let mut cmd = Command::new("sleep");
            cmd.arg(secs.to_string());
            cmd
        };
        cmd.kill_on_drop(true);
        cmd
    }

    /// Whether a line comes from `lines` within `wait`.
    #[cfg(any(windows, target_os = "linux"))]
    async fn line_within(lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>, wait: Duration) -> bool {
        matches!(tokio::time::timeout(wait, lines.next_line()).await, Ok(Ok(Some(_))))
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[tokio::test]
    async fn a_paused_process_stops_until_it_is_resumed() {
        let mut child = chatty().spawn().unwrap();
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        assert!(line_within(&mut lines, Duration::from_secs(10)).await, "it runs");
        assert!(pause(&child, true));
        // What it wrote before is still in the pipe.
        while line_within(&mut lines, Duration::from_millis(300)).await {}
        assert!(!line_within(&mut lines, Duration::from_millis(700)).await, "nothing while paused");
        assert!(pause(&child, false));
        assert!(line_within(&mut lines, Duration::from_secs(5)).await, "it goes on");
        child.kill().await.unwrap();
    }

    #[tokio::test(start_paused = true)]
    async fn transcodes_hold_background_work_until_a_while_after_they_end() {
        let bg = Background::new();
        let one = bg.transcode();
        let two = bg.transcode();
        assert!(bg.held());
        assert!(!bg.starting(), "probes and reading ahead go on");
        tokio::time::timeout(Duration::from_millis(1), bg.wait_for_starts()).await.expect("nothing starts");
        let waiting = tokio::spawn({
            let bg = bg.clone();
            async move { bg.wait().await }
        });
        tokio::time::sleep(Duration::from_secs(3 * 3600)).await;
        assert!(!waiting.is_finished(), "for as long as they run");
        drop(one);
        tokio::time::sleep(Duration::from_secs(60)).await;
        assert!(!waiting.is_finished(), "the other one runs");
        drop(two);
        let ended = Instant::now();
        waiting.await.unwrap();
        assert_eq!(ended.elapsed().as_secs(), AFTER_TRANSCODE.as_secs(), "a restart's gap doesn't free it");
        assert!(!bg.held());
    }

    #[tokio::test(start_paused = true)]
    async fn picture_reads_hold_background_work_until_a_moment_after_they_end() {
        let bg = Background::new();
        let around_one_player = bg.pictures();
        let a_track = bg.pictures();
        assert!(bg.held());
        assert!(!bg.starting(), "probes and reading ahead go on");
        let waiting = tokio::spawn({
            let bg = bg.clone();
            async move { bg.wait().await }
        });
        tokio::time::sleep(Duration::from_secs(600)).await;
        assert!(!waiting.is_finished(), "for as long as they read");
        drop(around_one_player);
        tokio::time::sleep(Duration::from_secs(60)).await;
        assert!(!waiting.is_finished(), "the track's read goes on");
        drop(a_track);
        let ended = Instant::now();
        waiting.await.unwrap();
        assert_eq!(ended.elapsed().as_secs(), AFTER_PICTURES.as_secs(), "the next read comes soon");
        assert!(!bg.held());
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[tokio::test]
    async fn runs_are_paused_while_a_playback_starts_a_transcode_runs_or_pictures_are_read() {
        for kind in ["start", "transcode", "pictures"] {
            let bg = Background::new();
            let mut child = chatty().spawn().unwrap();
            let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
            let supervised = tokio::spawn({
                let bg = bg.clone();
                async move { bg.supervise(&mut child, Duration::from_secs(3600)).await }
            });
            assert!(line_within(&mut lines, Duration::from_secs(10)).await);
            let hold: Box<dyn std::any::Any> = match kind {
                "start" => Box::new(bg.start()),
                "transcode" => Box::new(bg.transcode()),
                _ => Box::new(bg.pictures()),
            };
            let deadline = Instant::now() + Duration::from_secs(5);
            while bg.paused() == 0 && Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            assert_eq!(bg.paused(), 1, "{kind}");
            while line_within(&mut lines, Duration::from_millis(300)).await {}
            assert!(!line_within(&mut lines, Duration::from_millis(500)).await, "paused");
            drop(hold);
            assert!(line_within(&mut lines, AFTER_TRANSCODE + Duration::from_secs(5)).await, "resumed");
            assert_eq!(bg.paused(), 0);
            supervised.abort();
        }
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[tokio::test]
    async fn paused_time_does_not_count_against_the_time_limit() {
        let bg = Background::new();
        let run = tokio::spawn({
            let bg = bg.clone();
            async move { bg.run(slow(1), Duration::from_secs(3)).await.map(|_| ()) }
        });
        tokio::time::sleep(Duration::from_millis(200)).await;
        bg.hold(Duration::from_secs(4));
        let done = tokio::time::timeout(Duration::from_secs(20), run).await.expect("it ends").unwrap();
        assert!(done.is_ok(), "{done:?}");
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[tokio::test]
    async fn running_too_long_is_an_error() {
        let bg = Background::new();
        let started = Instant::now();
        let done = bg.run(slow(30), Duration::from_millis(500)).await;
        assert!(matches!(done, Err(ApiError::Timeout(_))), "{done:?}");
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[tokio::test]
    async fn nothing_starts_while_held() {
        let bg = Background::new();
        let start = bg.start();
        let run = tokio::spawn({
            let bg = bg.clone();
            async move { bg.run(slow(0), Duration::from_secs(30)).await.map(|_| ()) }
        });
        tokio::time::sleep(Duration::from_millis(1500)).await;
        assert!(!run.is_finished(), "waits for the start");
        drop(start);
        let done = tokio::time::timeout(Duration::from_secs(20), run).await.expect("it ends").unwrap();
        assert!(done.is_ok(), "{done:?}");
    }
}

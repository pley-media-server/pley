// SPDX-License-Identifier: AGPL-3.0-only
//! Reading a file ahead of a playback ("prefetch"), when its page is opened or
//! its Play button pointed at: the parts the player or FFmpeg read first come
//! from a cache when Play is pressed (the share's and this host's), which is
//! worth up to seconds on a network share.
//!
//! What is read:
//!
//! - the head (a few MB: headers, the first clusters or chunks);
//! - the file's index, the Matroska Cues or the MP4 movie box, often at the
//!   end of the file;
//! - for a resume, the media from the keyframe a transcode at that point
//!   decodes from (at or before its segment's start) to a few seconds past
//!   the position, found through the index: the keyframe's cluster
//!   (`CueClusterPosition`) or chunk (`stco`/`co64`).
//!
//! It is background work: one file at a time, never twice at once for a
//! file, ten seconds at most, and none while a playback starts (a job being
//! created, a job's FFmpeg starting, a direct play: see
//! [`crate::background`]); one that starts meanwhile stops the reading.

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use pley_transcoder_api::{PrefetchRequest, Prefetched, PrefetchedRange};
use tokio::sync::Semaphore;

use crate::error::{ApiError, ApiResult};
use crate::job::JobManager;
use crate::keyframes::KeyframeStore;
use crate::plan::DEFAULT_SEGMENT_SECONDS;

/// A file's own index: where its bytes are, and `(time in ms, offset)` of
/// the keyframes it lists, sorted by time.
pub type ContainerIndex = (Range<u64>, Vec<(i64, u64)>);

/// The head of the file read.
const HEAD: u64 = 4 << 20;
/// Reads are this large, [`READERS`] at a time.
const PIECE: u64 = 2 << 20;
const READERS: usize = 4;
/// The most a prefetch takes, waiting for its turn included.
const TIMEOUT: Duration = Duration::from_secs(10);
/// No prefetch starts this soon after a job creation began.
const YIELD: Duration = Duration::from_secs(5);
/// How far past the position the start is read.
const AHEAD_MS: i64 = 4_000;
/// What is read at the start: at least…
const MIN_START: u64 = 2 << 20;
/// …and at most (6 s of a 4K remux at 50 Mb/s).
const MAX_START: u64 = 40 << 20;
/// The most of an index read.
const MAX_INDEX: u64 = 64 << 20;

pub struct Prefetcher {
    jobs: Arc<JobManager>,
    keyframes: Arc<KeyframeStore>,
    /// Files being read.
    reading: Mutex<HashSet<PathBuf>>,
    /// One file at a time.
    limiter: Semaphore,
}

impl Prefetcher {
    pub fn new(jobs: Arc<JobManager>, keyframes: Arc<KeyframeStore>) -> Self {
        Self { jobs, keyframes, reading: Mutex::default(), limiter: Semaphore::new(1) }
    }

    pub async fn prefetch(&self, req: &PrefetchRequest) -> ApiResult<Prefetched> {
        let path = PathBuf::from(&req.path);
        if !path.is_file() {
            return Err(ApiError::NotFound(format!("file not found: {}", req.path)));
        }
        let started = Instant::now();
        let skipped = |why: &str| Prefetched {
            skipped: Some(why.into()),
            elapsed_ms: started.elapsed().as_millis() as u64,
            ..Default::default()
        };
        if self.starting() {
            return Ok(skipped("a playback is starting"));
        }
        let Some(_flight) = Flight::take(&self.reading, &path) else {
            return Ok(skipped("the file is being read already"));
        };
        let deadline = started + TIMEOUT;
        let Ok(permit) = tokio::time::timeout_at(deadline.into(), self.limiter.acquire()).await else {
            return Ok(skipped("other files took the time"));
        };
        let _permit = permit.map_err(|e| ApiError::Internal(e.into()))?;
        if self.starting() {
            return Ok(skipped("a playback is starting"));
        }
        // Reading stops at the deadline, when a job creation or another
        // playback start begins, or when this request is dropped (the reads
        // run on the blocking pool).
        let stop = Arc::new(Stop {
            jobs: self.jobs.clone(),
            creations: self.jobs.creations(),
            deadline,
            dropped: AtomicBool::new(false),
        });
        let _dropped = DropStops(stop.clone());
        let start_ms = req.start_ms.unwrap_or(0).max(0);
        let (mut done, indexed) = {
            let (path, stop) = (path.clone(), stop.clone());
            tokio::task::spawn_blocking(move || warm(&path, start_ms, &|| stop.now()))
                .await
                .map_err(|e| ApiError::Internal(e.into()))?
        };
        // The index is in the cache now: keep the keyframes it lists, for
        // remuxing (they are read again only if the file changes).
        if indexed && !done.stopped {
            let _ = self.keyframes.get_or_read(&path).await;
        }
        done.elapsed_ms = started.elapsed().as_millis() as u64;
        let ranges: Vec<String> = done.ranges.iter().map(|r| format!("{} {}-{}", r.part, r.start, r.end)).collect();
        tracing::debug!(
            "read ahead {} at {} ms: {} ({:.1} MB in {} ms{})",
            path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
            start_ms,
            ranges.join(", "),
            done.bytes as f64 / 1e6,
            done.elapsed_ms,
            if done.stopped { ", stopped" } else { "" }
        );
        Ok(done)
    }

    fn starting(&self) -> bool {
        self.jobs.created_within(YIELD) || self.jobs.background.starting()
    }
}

/// A file being read; it is let go when dropped.
struct Flight<'a> {
    reading: &'a Mutex<HashSet<PathBuf>>,
    path: PathBuf,
}

impl<'a> Flight<'a> {
    fn take(reading: &'a Mutex<HashSet<PathBuf>>, path: &Path) -> Option<Self> {
        reading.lock().insert(path.to_path_buf()).then(|| Self { reading, path: path.to_path_buf() })
    }
}

impl Drop for Flight<'_> {
    fn drop(&mut self) {
        self.reading.lock().remove(&self.path);
    }
}

struct Stop {
    jobs: Arc<JobManager>,
    creations: u64,
    deadline: Instant,
    dropped: AtomicBool,
}

impl Stop {
    fn now(&self) -> bool {
        self.dropped.load(Ordering::Relaxed)
            || Instant::now() >= self.deadline
            || self.jobs.creations() != self.creations
            || self.jobs.background.starting()
    }
}

struct DropStops(Arc<Stop>);

impl Drop for DropStops {
    fn drop(&mut self) {
        self.0.dropped.store(true, Ordering::Relaxed);
    }
}

/// Read the head of the file, its index and its start at `start_ms`, until
/// `stop` says so; and whether it has an index that lists keyframes. Blocking.
fn warm(path: &Path, start_ms: i64, stop: &(dyn Fn() -> bool + Sync)) -> (Prefetched, bool) {
    let mut done = Prefetched::default();
    let Ok(len) = std::fs::metadata(path).map(|m| m.len()) else { return (done, false) };
    let mut magic = Vec::with_capacity(12);
    if File::open(path).and_then(|f| f.take(12).read_to_end(&mut magic)).is_err() {
        return (done, false);
    }
    let index = if magic.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        crate::matroska::cue_index(path)
    } else if crate::mp4::sniff(&magic) {
        crate::mp4::chunk_index(path).ok()
    } else {
        None
    };
    let mut parts = vec![("head", 0..HEAD.min(len))];
    if let Some((range, points)) = &index {
        let end = range.end.min(len).min(range.start.saturating_add(MAX_INDEX));
        parts.push(("index", range.start.min(end)..end));
        if start_ms > 0 {
            parts.extend(start_region(points, start_ms, len).map(|r| ("start", r)));
        }
    }
    let mut covered: Vec<Range<u64>> = Vec::new();
    for (part, range) in parts {
        for todo in uncovered(&range, &covered) {
            if stop() {
                done.stopped = true;
                break;
            }
            for read in read_range(path, todo, stop) {
                done.bytes += read.end - read.start;
                done.ranges.push(PrefetchedRange { part: part.into(), start: read.start, end: read.end });
            }
        }
        covered.push(range);
    }
    done.stopped |= stop();
    (done, index.is_some_and(|(_, points)| !points.is_empty()))
}

/// The bytes a playback starting at `start_ms` reads first, from the
/// keyframes `(time in ms, offset)` of a file of `len` bytes: from the
/// keyframe at or before the start of the segment holding `start_ms` (where
/// a transcode decodes from; a remux or a direct play starts there or later)
/// to the first keyframe [`AHEAD_MS`] past it, within [`MIN_START`] and
/// [`MAX_START`].
fn start_region(points: &[(i64, u64)], start_ms: i64, len: u64) -> Option<Range<u64>> {
    let segment = (DEFAULT_SEGMENT_SECONDS * 1000.0) as i64;
    let boundary = start_ms / segment * segment;
    let from = points.iter().rev().find(|(t, _)| *t <= boundary).or(points.first())?.1;
    let until = start_ms + AHEAD_MS;
    let end = points.iter().find(|(t, at)| *t >= until && *at > from).map_or(len, |p| p.1);
    let end = end.clamp(from.saturating_add(MIN_START), from.saturating_add(MAX_START)).min(len);
    (end > from).then_some(from..end)
}

/// The parts of `range` outside every range of `covered`.
fn uncovered(range: &Range<u64>, covered: &[Range<u64>]) -> Vec<Range<u64>> {
    let mut parts = vec![range.clone()];
    for c in covered {
        parts = parts
            .into_iter()
            .flat_map(|p| [p.start..c.start.min(p.end), c.end.max(p.start)..p.end])
            .filter(|p| !p.is_empty())
            .collect();
    }
    parts
}

/// Read `range` of the file in [`PIECE`]s, [`READERS`] at a time (each with a
/// handle of its own), until `stop` says so; what was read, merged. Blocking.
fn read_range(path: &Path, range: Range<u64>, stop: &(dyn Fn() -> bool + Sync)) -> Vec<Range<u64>> {
    let pieces: Vec<Range<u64>> =
        (range.start..range.end).step_by(PIECE as usize).map(|at| at..(at + PIECE).min(range.end)).collect();
    let next = AtomicUsize::new(0);
    let read = Mutex::new(Vec::with_capacity(pieces.len()));
    std::thread::scope(|scope| {
        for _ in 0..READERS.min(pieces.len()) {
            scope.spawn(|| {
                let Ok(file) = File::open(path) else { return };
                let mut buf = vec![0u8; PIECE as usize];
                while let Some(piece) = pieces.get(next.fetch_add(1, Ordering::Relaxed)) {
                    if stop() {
                        break;
                    }
                    let want = (piece.end - piece.start) as usize;
                    let got = read_fully(&file, &mut buf[..want], piece.start);
                    if got > 0 {
                        read.lock().push(piece.start..piece.start + got as u64);
                    }
                    if got < want {
                        break;
                    }
                }
            });
        }
    });
    let mut read = read.into_inner();
    read.sort_unstable_by_key(|r| r.start);
    let mut merged: Vec<Range<u64>> = Vec::with_capacity(read.len());
    for r in read {
        match merged.last_mut() {
            Some(last) if last.end == r.start => last.end = r.end,
            _ => merged.push(r),
        }
    }
    merged
}

/// Fill `buf` from `offset` (less at the end of the file, or on an error).
fn read_fully(file: &File, buf: &mut [u8], offset: u64) -> usize {
    let mut filled = 0;
    while filled < buf.len() {
        match read_at(file, &mut buf[filled..], offset + filled as u64) {
            Ok(0) | Err(_) => break,
            Ok(n) => filled += n,
        }
    }
    filled
}

#[cfg(windows)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    std::os::windows::fs::FileExt::seek_read(file, buf, offset)
}

#[cfg(unix)]
fn read_at(file: &File, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    std::os::unix::fs::FileExt::read_at(file, buf, offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extract::Extractor;
    use crate::ffmpeg::Ffmpeg;
    use crate::metrics::Metrics;
    use crate::probe::Prober;
    use pley_transcoder_api::Limits;

    const MB: u64 = 1 << 20;

    #[test]
    fn the_start_runs_from_the_segments_keyframe_to_past_the_position() {
        // A keyframe every 2 s, 1 MB apart.
        let points: Vec<(i64, u64)> = (0..30).map(|k| (k * 2_000, 1_000 + k as u64 * MB)).collect();
        let len = 1_000 + 30 * MB;
        // 15 s is in the segment from 12 s: from the keyframe at 12 s to the
        // first at 19 s or later (20 s).
        assert_eq!(start_region(&points, 15_000, len), Some(1_000 + 6 * MB..1_000 + 10 * MB));
        // A keyframe every 10 s: the one before the segment.
        let sparse: Vec<(i64, u64)> = (0..10).map(|k| (k * 10_000, k as u64 * 10 * MB)).collect();
        assert_eq!(start_region(&sparse, 25_000, 100 * MB), Some(20 * MB..30 * MB));
        // Bounded: 40 MB at most, 2 MB at least, never past the end.
        let huge: Vec<(i64, u64)> = (0..10).map(|k| (k * 10_000, k as u64 * 100 * MB)).collect();
        assert_eq!(start_region(&huge, 25_000, 1000 * MB), Some(200 * MB..240 * MB));
        let dense: Vec<(i64, u64)> = (0..100).map(|k| (k * 1_000, k as u64 * 1_000)).collect();
        assert_eq!(start_region(&dense, 25_000, 10 * MB), Some(24_000..24_000 + 2 * MB));
        assert_eq!(start_region(&points, 57_000, len), Some(1_000 + 28 * MB..len), "to the end of the file");
        assert_eq!(start_region(&[], 25_000, len), None);
    }

    #[test]
    fn ranges_read_already_are_left_out() {
        let left = |range: Range<u64>, covered: &[(u64, u64)]| -> Vec<(u64, u64)> {
            let covered: Vec<Range<u64>> = covered.iter().map(|&(start, end)| start..end).collect();
            uncovered(&range, &covered).into_iter().map(|r| (r.start, r.end)).collect()
        };
        assert_eq!(left(0..10, &[]), [(0, 10)]);
        assert_eq!(left(0..10, &[(3, 5)]), [(0, 3), (5, 10)]);
        assert_eq!(left(0..10, &[(0, 4), (8, 20)]), [(4, 8)]);
        assert_eq!(left(5..10, &[(0, 20)]), []);
        assert_eq!(left(5..10, &[(20, 30)]), [(5, 10)]);
    }

    /// Encode a Matroska element with an 8-byte size.
    fn el(id: &[u8], payload: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.push(0x01);
        out.extend_from_slice(&(payload.len() as u64).to_be_bytes()[1..]);
        out.extend_from_slice(payload);
        out
    }

    fn uint(id: &[u8], v: u64) -> Vec<u8> {
        el(id, &v.to_be_bytes())
    }

    /// A Matroska file: a seek head, info, a video track, 12 clusters of 1 MB
    /// two seconds apart, then the Cues; and where its clusters and Cues are.
    fn matroska(dir: &Path) -> (PathBuf, Vec<u64>, Range<u64>) {
        let ebml = el(&[0x1A, 0x45, 0xDF, 0xA3], &[0x42, 0x86, 0x81, 0x01]);
        let info = el(&[0x15, 0x49, 0xA9, 0x66], &uint(&[0x2A, 0xD7, 0xB1], 1_000_000));
        let tracks = el(&[0x16, 0x54, 0xAE, 0x6B], &el(&[0xAE], &[uint(&[0xD7], 1), uint(&[0x83], 1)].concat()));
        let seek_head = |cues: u64| {
            let seek =
                |id: &[u8], pos: u64| el(&[0x4D, 0xBB], &[el(&[0x53, 0xAB], id), uint(&[0x53, 0xAC], pos)].concat());
            el(&[0x11, 0x4D, 0x9B, 0x74], &seek(&[0x1C, 0x53, 0xBB, 0x6B], cues))
        };
        let head = seek_head(0).len() + info.len() + tracks.len();
        let mut clusters = Vec::new();
        let mut at = Vec::new();
        for k in 0..12u64 {
            at.push((head + clusters.len()) as u64);
            let data = [uint(&[0xE7], k * 2_000), el(&[0xEC], &vec![0u8; MB as usize])].concat();
            clusters.extend(el(&[0x1F, 0x43, 0xB6, 0x75], &data));
        }
        let points: Vec<u8> = at
            .iter()
            .enumerate()
            .flat_map(|(k, pos)| {
                let position = el(&[0xB7], &[uint(&[0xF7], 1), uint(&[0xF1], *pos)].concat());
                el(&[0xBB], &[uint(&[0xB3], k as u64 * 2_000), position].concat())
            })
            .collect();
        let cues = el(&[0x1C, 0x53, 0xBB, 0x6B], &points);
        let cues_at = (head + clusters.len()) as u64;
        let segment = [seek_head(cues_at), info, tracks, clusters, cues.clone()].concat();
        // Positions count from the segment's data, after its 12-byte header.
        let data = (ebml.len() + 12) as u64;
        let file = [ebml, el(&[0x18, 0x53, 0x80, 0x67], &segment)].concat();
        let path = dir.join("movie.mkv");
        std::fs::write(&path, file).unwrap();
        (path, at.iter().map(|p| p + data).collect(), data + cues_at..data + cues_at + cues.len() as u64)
    }

    fn prefetcher(dir: &Path) -> (Prefetcher, Arc<JobManager>, Arc<KeyframeStore>) {
        let ff = Arc::new(Ffmpeg {
            ffmpeg: dir.join("no-ffmpeg"),
            ffprobe: dir.join("no-ffprobe"),
            caps: Default::default(),
            segment_times_relative: true,
        });
        let metrics = Arc::new(Metrics::default());
        let prober = Arc::new(Prober::new(ff.clone(), metrics.clone(), 1));
        let background = crate::background::Background::new();
        let keyframes = KeyframeStore::new(ff.clone(), metrics.clone(), background.clone(), dir.join("keyframes"));
        let extractor = Arc::new(Extractor::new(ff.clone(), prober.clone(), metrics.clone(), dir.join("subs"), 1));
        let jobs = JobManager::new(
            ff,
            prober,
            keyframes.clone(),
            extractor,
            metrics,
            dir.join("jobs"),
            None,
            None,
            Limits { max_jobs: 1, max_hw_jobs: 0 },
            background,
        );
        (Prefetcher::new(jobs.clone(), keyframes.clone()), jobs, keyframes)
    }

    fn parts(done: &Prefetched) -> Vec<(&str, Range<u64>)> {
        done.ranges.iter().map(|r| (r.part.as_str(), r.start..r.end)).collect()
    }

    #[tokio::test]
    async fn the_head_the_cues_and_the_start_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let (movie, clusters, cues) = matroska(dir.path());
        let len = std::fs::metadata(&movie).unwrap().len();
        let (prefetcher, _, keyframes) = prefetcher(dir.path());
        let req = |start_ms| PrefetchRequest { path: movie.to_string_lossy().into_owned(), start_ms };

        // From the beginning: the head and the Cues at the end.
        let done = prefetcher.prefetch(&req(None)).await.unwrap();
        assert_eq!(parts(&done), vec![("head", 0..4 * MB), ("index", cues.clone())]);
        assert_eq!(done.bytes, 4 * MB + (cues.end - cues.start));
        assert!(!done.stopped && done.skipped.is_none());
        // The keyframes are kept for remuxing.
        let times: Vec<i64> = (0..12).map(|k| k * 2_000).collect();
        assert_eq!(keyframes.get(&movie).as_deref(), Some(&times));

        // A resume at 15 s: from the cluster of the keyframe at 12 s (the
        // segment's start) to the cluster at 20 s.
        let done = prefetcher.prefetch(&req(Some(15_000))).await.unwrap();
        assert_eq!(
            parts(&done),
            vec![("head", 0..4 * MB), ("index", cues.clone()), ("start", clusters[6]..clusters[10])]
        );
        // Near the end: up to the Cues, read already.
        let done = prefetcher.prefetch(&req(Some(21_000))).await.unwrap();
        assert_eq!(parts(&done).last(), Some(&("start", clusters[10]..cues.start)));
        assert!(cues.end == len);

        // Not a container with an index: its head only.
        let other = dir.path().join("movie.ts");
        std::fs::write(&other, vec![0x47u8; 5 * MB as usize]).unwrap();
        let done = prefetcher
            .prefetch(&PrefetchRequest { path: other.to_string_lossy().into_owned(), start_ms: Some(60_000) })
            .await
            .unwrap();
        assert_eq!(parts(&done), vec![("head", 0..4 * MB)]);
        // A missing file is not found.
        let missing =
            PrefetchRequest { path: dir.path().join("gone.mkv").to_string_lossy().into_owned(), start_ms: None };
        assert!(matches!(prefetcher.prefetch(&missing).await, Err(ApiError::NotFound(_))));
    }

    #[tokio::test]
    async fn a_starting_playback_comes_first() {
        let dir = tempfile::tempdir().unwrap();
        let (movie, _, _) = matroska(dir.path());
        let (prefetcher, jobs, _) = prefetcher(dir.path());
        let req = PrefetchRequest { path: movie.to_string_lossy().into_owned(), start_ms: Some(15_000) };

        // Once at a time per file.
        let flight = Flight::take(&prefetcher.reading, &movie).unwrap();
        let done = prefetcher.prefetch(&req).await.unwrap();
        assert_eq!((done.bytes, done.skipped.as_deref()), (0, Some("the file is being read already")));
        drop(flight);

        // Not while a job is being created…
        jobs.note_creation();
        let done = prefetcher.prefetch(&req).await.unwrap();
        assert_eq!((done.bytes, done.skipped.as_deref()), (0, Some("a playback is starting")));

        // …and one starting meanwhile stops the reading.
        let creations = jobs.creations();
        let stop = Stop { jobs: jobs.clone(), creations, deadline: Instant::now() + TIMEOUT, dropped: false.into() };
        assert!(!stop.now());
        let calls = AtomicUsize::new(0);
        let (done, _) = warm(&movie, 15_000, &|| {
            // A job is created as the second piece is about to be read.
            if calls.fetch_add(1, Ordering::SeqCst) == 1 {
                jobs.note_creation();
            }
            stop.now()
        });
        assert!(done.stopped);
        assert!(done.bytes < 4 * MB, "{}", done.bytes);
    }

    #[tokio::test]
    async fn other_playback_starts_come_first_too() {
        let dir = tempfile::tempdir().unwrap();
        let (movie, _, _) = matroska(dir.path());
        let (prefetcher, jobs, _) = prefetcher(dir.path());
        let req = PrefetchRequest { path: movie.to_string_lossy().into_owned(), start_ms: Some(15_000) };

        // A job's FFmpeg starting at a seek, or a direct play the server said is starting.
        let start = jobs.background.start();
        let done = prefetcher.prefetch(&req).await.unwrap();
        assert_eq!((done.bytes, done.skipped.as_deref()), (0, Some("a playback is starting")));
        let stop = Stop {
            jobs: jobs.clone(),
            creations: jobs.creations(),
            deadline: Instant::now() + TIMEOUT,
            dropped: false.into(),
        };
        assert!(stop.now(), "it stops the reading");
        drop(start);
        assert!(!stop.now());
        let done = prefetcher.prefetch(&req).await.unwrap();
        assert_eq!(done.skipped, None);
        assert!(done.bytes > 0);
    }
}

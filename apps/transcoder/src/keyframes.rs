// SPDX-License-Identifier: AGPL-3.0-only
//! Keyframe index used to segment stream-copied (remuxed) video exactly on
//! keyframes, cached on disk per file version.
//!
//! Matroska and MP4/MOV files carry an index of their own (Cues; sample
//! tables and fragment headers), read in milliseconds when a job is created.
//! Other files, or ones whose index can't be trusted, need every packet
//! header read: that runs in the background one file at a time, out of the
//! way of playbacks and transcodes (see [`crate::background`]), and jobs
//! transcode until it is done.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use parking_lot::Mutex;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

use crate::background::Background;
use crate::ffmpeg::Ffmpeg;
use crate::metrics::Metrics;
use crate::util::{MEDIA_INPUT_OPTIONS, background_priority, file_key, input_arg, touch, write_atomically};

const MEMORY_ENTRIES: usize = 256;
/// How long creating a job may spend reading a file's own index (only walking
/// thousands of movie fragments on slow storage comes near it); beyond that
/// the job transcodes and the index is finished in the background.
const READ_BUDGET: Duration = Duration::from_secs(2);
/// The longest a scan of every packet header takes, paused time not counted
/// (a whole file read over a slow share; it never stopped before).
const SCAN_TIMEOUT: Duration = Duration::from_secs(12 * 3600);

pub struct KeyframeStore {
    dir: PathBuf,
    memory: Mutex<HashMap<String, Arc<Vec<i64>>>>,
    pending: Mutex<HashSet<String>>,
    queue: mpsc::UnboundedSender<(String, PathBuf)>,
}

impl KeyframeStore {
    pub fn new(ff: Arc<Ffmpeg>, metrics: Arc<Metrics>, background: Arc<Background>, dir: PathBuf) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&dir);
        let (tx, mut rx) = mpsc::unbounded_channel::<(String, PathBuf)>();
        let store = Arc::new(Self { dir, memory: Mutex::default(), pending: Mutex::default(), queue: tx });
        let worker = Arc::downgrade(&store);
        tokio::spawn(async move {
            while let Some((key, path)) = rx.recv().await {
                let Some(store) = worker.upgrade() else { break };
                background.wait().await;
                Metrics::inc(&metrics.keyframe_scans);
                let started = Instant::now();
                let own = {
                    let path = path.clone();
                    tokio::task::spawn_blocking(move || container_index(&path, None)).await.ok().flatten()
                };
                let result = match own {
                    Some((times, _)) => Ok(times),
                    None => scan(&ff, &background, &path).await,
                };
                match result {
                    Ok(times) => {
                        tracing::debug!(
                            "keyframe index for {} ({} keyframes) built in {:.1}s",
                            path.display(),
                            times.len(),
                            started.elapsed().as_secs_f32()
                        );
                        store.save(&key, times).await;
                    }
                    Err(err) => tracing::warn!("keyframe scan failed for {}: {err:#}", path.display()),
                }
                store.pending.lock().remove(&key);
            }
        });
        store
    }

    /// Cached keyframe times (ms) for `path`, if an index exists.
    pub fn get(&self, path: &Path) -> Option<Arc<Vec<i64>>> {
        let key = file_key(path).ok()?;
        if let Some(hit) = self.memory.lock().get(&key) {
            return Some(hit.clone());
        }
        let file = self.dir.join(format!("{key}.txt"));
        let text = std::fs::read_to_string(&file).ok()?;
        let times: Vec<i64> = text.lines().filter_map(|l| l.trim().parse().ok()).collect();
        if times.is_empty() {
            return None;
        }
        // In use: cache pruning keeps it.
        touch(&file);
        let times = Arc::new(times);
        self.remember(key, times.clone());
        Some(times)
    }

    /// Like [`get`](Self::get), but without a cached index first reads the
    /// file's own (Matroska cues, MP4 sample tables), which takes
    /// milliseconds.
    pub async fn get_or_read(&self, path: &Path) -> Option<Arc<Vec<i64>>> {
        if let Some(hit) = self.get(path) {
            return Some(hit);
        }
        let key = file_key(path).ok()?;
        let file = path.to_path_buf();
        let deadline = Instant::now() + READ_BUDGET;
        let started = Instant::now();
        let (times, source) =
            tokio::task::spawn_blocking(move || container_index(&file, Some(deadline))).await.ok().flatten()?;
        tracing::debug!(
            "keyframe index for {} read from {source} ({} keyframes, {:.0} ms)",
            path.display(),
            times.len(),
            started.elapsed().as_secs_f64() * 1000.0
        );
        self.save(&key, times.clone()).await;
        Some(Arc::new(times))
    }

    /// Queue an index build unless one exists or is already queued.
    pub fn request(&self, path: &Path) {
        let Ok(key) = file_key(path) else { return };
        if self.memory.lock().contains_key(&key) || self.dir.join(format!("{key}.txt")).exists() {
            return;
        }
        if self.pending.lock().insert(key.clone()) {
            let _ = self.queue.send((key, path.to_path_buf()));
        }
    }

    async fn save(&self, key: &str, times: Vec<i64>) {
        if times.is_empty() {
            return;
        }
        let text: String = times.iter().map(|t| format!("{t}\n")).collect();
        // Whole or not at all (a truncated index would misplace segment
        // boundaries), even when a job and the background worker save the
        // same one at once.
        let path = self.dir.join(format!("{key}.txt"));
        let _ = tokio::task::spawn_blocking(move || write_atomically(&path, text.as_bytes())).await;
        self.remember(key.to_owned(), Arc::new(times));
    }

    fn remember(&self, key: String, times: Arc<Vec<i64>>) {
        let mut memory = self.memory.lock();
        if memory.len() >= MEMORY_ENTRIES {
            memory.clear();
        }
        memory.insert(key, times);
    }
}

/// Keyframe times from the file's own index, recognised by content rather
/// than by name: Matroska cues or MP4/MOV sample tables. `None` when it has
/// none that can be trusted (the caller scans packets instead). Blocking.
fn container_index(path: &Path, deadline: Option<Instant>) -> Option<(Vec<i64>, &'static str)> {
    let mut head = Vec::with_capacity(12);
    std::fs::File::open(path).ok()?.take(12).read_to_end(&mut head).ok()?;
    if head.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return crate::matroska::cue_keyframes(path).map(|times| (times, "Matroska cues"));
    }
    if crate::mp4::sniff(&head) {
        return match crate::mp4::keyframes(path, deadline) {
            Ok(times) => Some((times, "MP4 sample tables")),
            Err(why) => {
                tracing::debug!("no keyframe index in {}: {why}", path.display());
                None
            }
        };
    }
    None
}

/// Read every video packet header and collect keyframe timestamps.
async fn scan(ff: &Ffmpeg, background: &Background, path: &Path) -> anyhow::Result<Vec<i64>> {
    let mut cmd = ff.probe_command();
    cmd.args(["-v", "error", "-select_streams", "v:0", "-show_entries", "packet=pts_time,flags", "-of", "csv=p=0"])
        .args(MEDIA_INPUT_OPTIONS)
        .arg(input_arg(path))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // Background work: playback comes first.
    background_priority(&mut cmd);
    let mut child = cmd.spawn().context("spawning ffprobe")?;
    let stdout = child.stdout.take().context("no stdout")?;
    let read = async {
        let mut lines = BufReader::with_capacity(1 << 16, stdout).lines();
        let mut times = Vec::new();
        while let Some(line) = lines.next_line().await? {
            if let Some(t) = parse_packet_line(&line) {
                times.push(t);
            }
        }
        anyhow::Ok(times)
    };
    // Paused while a playback starts or a transcode runs.
    let (status, times) = tokio::join!(background.supervise(&mut child, SCAN_TIMEOUT), read);
    let (status, mut times) = (status.map_err(|e| anyhow::anyhow!("{e:?}"))?, times?);
    if !status.success() && times.is_empty() {
        bail!("ffprobe exited with {status}");
    }
    times.sort_unstable();
    times.dedup();
    Ok(times)
}

/// `"12.345000,K__"` → `Some(12345)` for keyframes.
pub(crate) fn parse_packet_line(line: &str) -> Option<i64> {
    let (pts, flags) = line.split_once(',')?;
    if !flags.starts_with('K') {
        return None;
    }
    let secs: f64 = pts.trim().parse().ok()?;
    (secs.is_finite() && secs >= 0.0).then(|| (secs * 1000.0).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_packet_lines() {
        assert_eq!(parse_packet_line("12.345000,K__"), Some(12_345));
        assert_eq!(parse_packet_line("12.345000,___"), None);
        assert_eq!(parse_packet_line("N/A,K__"), None);
        assert_eq!(parse_packet_line("garbage"), None);
        assert_eq!(parse_packet_line("-0.042000,K__"), None, "before the start");
        assert_eq!(parse_packet_line(" 0.5 ,KD_"), Some(500));
    }

    fn store(dir: &Path) -> Arc<KeyframeStore> {
        let ff = Arc::new(Ffmpeg {
            ffmpeg: dir.join("no-ffmpeg"),
            ffprobe: dir.join("no-ffprobe"),
            caps: Default::default(),
            segment_times_relative: true,
        });
        KeyframeStore::new(ff, Arc::new(Metrics::default()), Background::new(), dir.join("keyframes"))
    }

    #[tokio::test]
    async fn indexes_are_kept_per_file_version() {
        let dir = tempfile::tempdir().unwrap();
        let movie = dir.path().join("movie.mp4");
        std::fs::write(&movie, b"frames").unwrap();
        let keyframes = store(dir.path());
        assert!(keyframes.get(&movie).is_none());
        keyframes.save(&file_key(&movie).unwrap(), vec![0, 2_002, 4_004]).await;
        assert_eq!(keyframes.get(&movie).as_deref(), Some(&vec![0, 2_002, 4_004]));
        // On disk too: a new store (a restarted transcoder) finds it.
        let restarted = store(dir.path());
        assert_eq!(restarted.get(&movie).as_deref(), Some(&vec![0, 2_002, 4_004]));
        // An index exists: no scan is queued.
        restarted.request(&movie);
        assert!(restarted.pending.lock().is_empty());
        // A replaced file needs a new index; empty ones are never stored.
        std::fs::write(&movie, b"other frames").unwrap();
        assert!(restarted.get(&movie).is_none());
        restarted.save(&file_key(&movie).unwrap(), vec![]).await;
        assert!(restarted.get(&movie).is_none());
        // Neither Matroska nor MP4 inside: it waits for a scan.
        assert!(restarted.get_or_read(&movie).await.is_none());
        // Nothing half-written is left behind.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("keyframes"))
            .unwrap()
            .flatten()
            .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
            .collect();
        assert!(leftovers.is_empty());
    }

    /// The smallest MP4 with a video track of three keyframes, 1 s apart.
    fn tiny_mp4() -> Vec<u8> {
        fn bx(kind: &[u8; 4], parts: &[&[u8]]) -> Vec<u8> {
            let payload = parts.concat();
            [&((payload.len() + 8) as u32).to_be_bytes()[..], kind, &payload].concat()
        }
        fn full(kind: &[u8; 4], words: &[u32]) -> Vec<u8> {
            let payload: Vec<u8> = [0u32].iter().chain(words).flat_map(|w| w.to_be_bytes()).collect();
            bx(kind, &[&payload])
        }
        let stbl = bx(
            b"stbl",
            &[
                &full(b"stsd", &[1, 16, u32::from_be_bytes(*b"avc1"), 0]),
                &full(b"stts", &[1, 3, 1000]),
                &full(b"stsc", &[1, 1, 1, 1]),
                &full(b"stsz", &[0, 3]),
                &full(b"stco", &[3]),
            ],
        );
        let hdlr = full(b"hdlr", &[0, u32::from_be_bytes(*b"vide"), 0, 0, 0]);
        let mdia = bx(b"mdia", &[&full(b"mdhd", &[0, 0, 1000, 3000, 0]), &hdlr, &bx(b"minf", &[&stbl])]);
        let trak = bx(b"trak", &[&full(b"tkhd", &[0, 0, 1, 0, 0]), &mdia]);
        let moov = bx(b"moov", &[&full(b"mvhd", &[0, 0, 1000, 3000]), &trak]);
        [bx(b"ftyp", &[b"isom"]), moov, bx(b"mdat", &[&[0; 8]])].concat()
    }

    #[tokio::test]
    async fn mp4_indexes_are_read_on_the_spot() {
        let dir = tempfile::tempdir().unwrap();
        // Recognised by its content, whatever its name.
        let movie = dir.path().join("movie.m4v.part");
        std::fs::write(&movie, tiny_mp4()).unwrap();
        let keyframes = store(dir.path());
        assert_eq!(keyframes.get_or_read(&movie).await.as_deref(), Some(&vec![0, 1_000, 2_000]));
        // Saved: a restarted transcoder has it without reading the file.
        assert_eq!(store(dir.path()).get(&movie).as_deref(), Some(&vec![0, 1_000, 2_000]));
        // A damaged one is left to the background scan.
        let broken = dir.path().join("broken.mp4");
        std::fs::write(&broken, &tiny_mp4()[..60]).unwrap();
        assert!(keyframes.get_or_read(&broken).await.is_none());
    }
}

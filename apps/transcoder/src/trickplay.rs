// SPDX-License-Identifier: AGPL-3.0-only
//! Seek-bar preview thumbnails ("trickplay"): one small frame every few
//! seconds, tiled into JPEG sprite sheets. Only keyframes are decoded, but
//! the whole file is read, so a set takes as long as its disk or share takes
//! to give it (paused while a playback starts, a transcode runs or a
//! player's subtitles are read, see [`crate::background`]); HDR and Dolby
//! Vision are tone-mapped so previews
//! look like the transcoded picture. Results are cached per file version
//! until every sheet has been fetched (the server keeps its own copy), or for
//! a day (see `cache.rs`).
//!
//! Up to `--max-previews` sets are made at once ([`default_runs`]): a title
//! someone opened beside the libraries' backlog, which the Pley server
//! spreads over disks. Where runs can't be paused (macOS), one at a time: a
//! second would keep reading through every playback start and transcode.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use pley_transcoder_api::{Tonemapper, TrickplayInfo, TrickplayRequest};
use tokio::process::Command;
use tokio::sync::Semaphore;
use tokio::task::JoinError;

use crate::background::Background;
use crate::cpuload::CpuMeter;
use crate::error::{ApiError, ApiResult};
use crate::extract::{KEEP_FRESH, Lock, Locks, kept_fresh, pass_timeout};
use crate::ffmpeg::Ffmpeg;
use crate::plan::{PLACEBO_SDR_LIFT, PLACEBO_STATIC_PEAK, ZSCALE_TONEMAP, untagged_hdr_fix};
use crate::probe::{Prober, Rejected};
use crate::util::{MEDIA_INPUT_OPTIONS, background_priority, file_key, input_arg, touch};

const COLUMNS: u32 = 10;
const ROWS: u32 = 10;
/// JPEG quality of the sheets (FFmpeg's `-q:v`, 2 to 31, lower is better).
/// Thumbnails are small and seen in passing: at their size 7 looks no
/// different from 5, and takes a fifth less space.
const QUALITY: &str = "7";
/// The version of the sets made here ([`TrickplayInfo::version`]), raised
/// whenever they are made otherwise, so that servers make theirs again. 1:
/// sheets at [`QUALITY`] 7 rather than 5.
pub const VERSION: u32 = 1;
/// The shortest time limit a set gets ([`run_limit`]).
const TIMEOUT: Duration = Duration::from_secs(60 * 60);
/// How long the file is waited for at first: a share whose disks spin up
/// answers within it.
const FILE_LOOK: Duration = Duration::from_secs(60);
/// Files looked at at once at most: a share that doesn't answer holds a
/// blocking thread for each, and no more than these, however often its
/// files are asked for again.
const LOOKS_AT_ONCE: usize = 4;
/// Sets whose fetched sheets are tracked at once (sets fetched only in part
/// are forgotten beyond that, and left to cache pruning).
const MAX_TRACKED: usize = 1000;

pub struct Trickplay {
    ff: Arc<Ffmpeg>,
    prober: Arc<Prober>,
    background: Arc<Background>,
    dir: PathBuf,
    /// `runs` generations at once: the Pley server sends no more, and asks
    /// for what it wants first. Further requests wait their turn.
    limiter: Semaphore,
    runs: usize,
    /// First looks at files ([`LOOKS_AT_ONCE`]).
    looks: Arc<Semaphore>,
    /// One generation per set, the lock forgotten however it ends.
    locks: Locks,
    /// Sheets fetched so far, per set.
    served: Mutex<HashMap<String, HashSet<u32>>>,
    /// How busy the processors are: a CPU limit it finds sizes FFmpeg's
    /// threads.
    cpu: Arc<CpuMeter>,
}

/// Thumbnails every 2–10 s: about 360 for a film, more density for short clips.
pub fn default_interval_ms(duration_ms: i64) -> i64 {
    ((duration_ms / 360).clamp(2_000, 10_000) / 1_000) * 1_000
}

/// Sets made at once unless `--max-previews` says otherwise: one for a title
/// someone opened and four for the libraries' backlog (the Pley server sends
/// no more). Where runs can't be paused (macOS), one: a second would keep
/// reading through every playback start and transcode.
pub fn default_runs() -> u32 {
    if cfg!(any(windows, target_os = "linux")) { 5 } else { 1 }
}

/// How long a set may take to make, paused time not counted: an hour, or
/// longer for a file too big to read in that time at 10 MB/s (a slow disk or
/// share, read by others meanwhile), as passes over whole files get
/// ([`pass_timeout`]). A 70 GB file gets about 2 hours.
fn run_limit(bytes: u64) -> Duration {
    pass_timeout(bytes).max(TIMEOUT)
}

/// FFmpeg's thread counts under a CPU limit (a container's `--cpus`), which
/// it doesn't see: it counts the processors it may run on, and threads past
/// the limit only get the whole control group throttled, the Pley server in
/// it too. None without one: FFmpeg's own counts serve best (4 threads of 16
/// made a run slower, and four runs no faster).
fn threads(quota: Option<f64>) -> Option<String> {
    quota.map(|cores| (cores.ceil().max(1.0) as u32).to_string())
}

/// What a probe that failed answers. Only a file ffprobe couldn't read is
/// one previews can't be made of; a timeout (a share whose disks sleep) or
/// ffprobe failing to start may pass, and isn't the file's failure.
fn probe_error(err: anyhow::Error) -> ApiError {
    if err.downcast_ref::<Rejected>().is_some() {
        ApiError::Failed(format!("probe failed: {err:#}"))
    } else {
        ApiError::Internal(err.context("probe failed"))
    }
}

/// `look`, which blocks, run off the async workers with one of `permits`,
/// and its answer if that comes within `wait`, the wait for a permit
/// included (`None` if not). It keeps its permit until it ends, answered
/// in time or not, so that no more than the permits ever block at once.
async fn bounded<T: Send + 'static>(
    permits: Arc<Semaphore>,
    wait: Duration,
    look: impl FnOnce() -> Option<T> + Send + 'static,
) -> Result<Option<T>, JoinError> {
    let run = async move {
        let Ok(permit) = permits.acquire_owned().await else { return Ok(None) };
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            look()
        })
        .await
    };
    tokio::time::timeout(wait, run).await.unwrap_or(Ok(None))
}

/// Marks a set's folder as being written, for the cache's pruning: a marker
/// made and removed in it changes the folder's time on every system.
fn freshen(part: &Path) {
    let marker = part.join(".fresh");
    if std::fs::write(&marker, b"").is_ok() {
        let _ = std::fs::remove_file(&marker);
    }
}

impl Trickplay {
    pub fn new(
        ff: Arc<Ffmpeg>,
        prober: Arc<Prober>,
        background: Arc<Background>,
        dir: PathBuf,
        runs: u32,
        cpu: Arc<CpuMeter>,
    ) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        let runs = runs.max(1) as usize;
        Self {
            ff,
            prober,
            background,
            dir,
            limiter: Semaphore::new(runs),
            runs,
            looks: Arc::new(Semaphore::new(LOOKS_AT_ONCE)),
            locks: Mutex::default(),
            served: Mutex::default(),
            cpu,
        }
    }

    /// Sets being made now, those paused or waiting for a playback's start
    /// or a transcode included.
    pub fn running(&self) -> usize {
        self.runs.saturating_sub(self.limiter.available_permits())
    }

    pub fn sheet_path(&self, id: &str, n: u32) -> Option<PathBuf> {
        valid_id(id).then(|| self.dir.join(id).join(format!("{n}.jpg"))).filter(|p| p.is_file())
    }

    /// Note that sheet `n` of set `id` has been read for a client. Once every
    /// sheet of the set has been, the set is deleted: the server stores the
    /// sheets it fetches, so this copy would only take space. Blocking.
    pub fn sheet_served(&self, id: &str, n: u32) {
        if !valid_id(id) {
            return;
        }
        let dir = self.dir.join(id);
        let Some(info) = read_index(&dir) else { return };
        let complete = {
            let mut served = self.served.lock();
            if served.len() >= MAX_TRACKED && !served.contains_key(id) {
                served.clear();
            }
            let sheets = served.entry(id.to_owned()).or_default();
            sheets.insert(n);
            let complete = (0..info.sheets).all(|k| sheets.contains(&k));
            if complete {
                served.remove(id);
            }
            complete
        };
        if complete {
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => tracing::debug!("seek previews {id} fetched in full, removed"),
                Err(err) => tracing::debug!("could not remove seek previews {id}: {err}"),
            }
        }
    }

    pub async fn generate(&self, req: &TrickplayRequest) -> ApiResult<TrickplayInfo> {
        let path = Path::new(&req.path);
        // One look at the file, off the async workers: a share whose disks
        // sleep takes seconds to answer, and one that doesn't answer in a
        // minute, its turn included, is taken for one that isn't there (it
        // may be later).
        let file = path.to_owned();
        let looked = bounded(self.looks.clone(), FILE_LOOK, move || {
            let meta = std::fs::metadata(&file).ok().filter(|meta| meta.is_file())?;
            Some(file_key(&file).map(|key| (key, meta.len())))
        })
        .await;
        let (key, size) = match looked.map_err(|e| ApiError::Internal(e.into()))? {
            Some(found) => found?,
            None => return Err(ApiError::NotFound(format!("file not found: {}", req.path))),
        };
        let media = self.prober.probe(path).await.map_err(probe_error)?;
        let video = media.video().ok_or_else(|| ApiError::BadRequest("the file has no video".into()))?;
        let duration =
            media.duration_ms.filter(|d| *d > 0).ok_or_else(|| ApiError::BadRequest("unknown duration".into()))?;
        let interval = req.interval_ms.unwrap_or_else(|| default_interval_ms(duration)).clamp(1_000, 60_000);
        let width = req.width.unwrap_or(320).clamp(96, 640);
        let (src_w, src_h) = (video.width.unwrap_or(1920).max(2), video.height.unwrap_or(1080).max(2));
        let height = ((width as f64 * src_h as f64 / src_w as f64 / 2.0).round() as u32 * 2).max(2);

        // A set made by another version is another set.
        let id = format!("{key}-{interval}-{width}-v{VERSION}");
        let dir = self.dir.join(&id);
        if let Some(info) = read_index(&dir) {
            touch(&dir.join("index.json"));
            return Ok(info);
        }
        let entry = Lock::acquire(&self.locks, &id);
        let _guard = entry.lock.lock().await;
        if let Some(info) = read_index(&dir) {
            touch(&dir.join("index.json"));
            return Ok(info);
        }
        let _permit = self.limiter.acquire().await.map_err(|e| ApiError::Internal(e.into()))?;

        let range = video.dynamic_range();
        let caps = &self.ff.caps;
        let placebo = range.is_hdr() && caps.tonemappers.contains(&Tonemapper::Libplacebo);
        let picture = if placebo {
            // Handles Dolby Vision profile 5 too (its RPU is on every keyframe).
            // Tone mapped as the video is, by the stream's metadata.
            format!(
                "libplacebo=w={width}:h={height}:format=yuv420p:colorspace=bt709:color_primaries=bt709:color_trc=bt709:range=tv:tonemapping=auto:{PLACEBO_STATIC_PEAK}:{PLACEBO_SDR_LIFT}"
            )
        } else if range.is_hdr() && caps.tonemappers.contains(&Tonemapper::Zscale) {
            // zimg needs colours: Dolby Vision profile 5 frames have none.
            let tags = untagged_hdr_fix(video).map(|f| format!("{f},")).unwrap_or_default();
            format!("{tags}scale={width}:{height}:flags=bilinear,{ZSCALE_TONEMAP}")
        } else {
            format!("scale={width}:{height}:flags=bilinear")
        };
        let filter = format!("fps=1000/{interval},{picture},tile={COLUMNS}x{ROWS}");

        // The set's folder is made once the run may start: made before
        // waiting out a transcode, which may last hours, it would look
        // abandoned.
        self.background.wait().await;
        let part = self.dir.join(format!("{id}.part"));
        let _ = tokio::fs::remove_dir_all(&part).await;
        tokio::fs::create_dir_all(&part).await?;
        let threads = threads(self.cpu.quota());
        let cmd = self.command(path, video.index, &filter, placebo, threads.as_deref(), &part);
        let started = std::time::Instant::now();
        // The run writes nothing while it is paused (a transcode may last
        // hours): its folder is marked as being written meanwhile.
        let result = kept_fresh(self.background.run(cmd, run_limit(size)), KEEP_FRESH, || freshen(&part)).await;
        let sheets = count_sheets(&part);
        if let Err(err) = result {
            let _ = tokio::fs::remove_dir_all(&part).await;
            return Err(err);
        }
        if sheets == 0 {
            let _ = tokio::fs::remove_dir_all(&part).await;
            return Err(ApiError::Failed("no thumbnails could be made".into()));
        }
        let per_sheet = COLUMNS * ROWS;
        let count = ((duration + interval - 1) / interval) as u32;
        let info = TrickplayInfo {
            id: id.clone(),
            interval_ms: interval,
            width,
            height,
            columns: COLUMNS,
            rows: ROWS,
            count: count.min(sheets * per_sheet),
            sheets,
            version: VERSION,
        };
        tokio::fs::write(part.join("index.json"), serde_json::to_vec(&info).map_err(anyhow::Error::from)?).await?;
        let _ = tokio::fs::remove_dir_all(&dir).await;
        tokio::fs::rename(&part, &dir).await?;
        tracing::info!(
            "seek previews for {} ({} thumbnails) made in {:.1}s",
            path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
            info.count,
            started.elapsed().as_secs_f32()
        );
        Ok(info)
    }

    /// FFmpeg's command for a set: `filter` over the video stream `stream` of
    /// `path`, its sheets written into `part`. Under a CPU limit, its decoder
    /// and filters take `threads`.
    fn command(
        &self,
        path: &Path,
        stream: i64,
        filter: &str,
        placebo: bool,
        threads: Option<&str>,
        part: &Path,
    ) -> Command {
        let mut cmd = self.ff.command();
        cmd.args(["-loglevel", "error"]);
        if let Some(threads) = threads {
            cmd.args(["-filter_threads", threads]);
        }
        if placebo {
            cmd.args(["-init_hw_device", "vulkan=vk", "-filter_hw_device", "vk"]);
        }
        if let Some(threads) = threads {
            // Before the input: the decoder's.
            cmd.args(["-threads", threads]);
        }
        // Decoding keyframes only is what makes this cheap.
        cmd.args(["-skip_frame", "nokey"]).args(MEDIA_INPUT_OPTIONS).arg("-i").arg(input_arg(path));
        cmd.args(["-map", &format!("0:{stream}"), "-an", "-sn", "-dn", "-vf", filter]);
        cmd.args(["-fps_mode", "passthrough", "-c:v", "mjpeg", "-q:v", QUALITY, "-start_number", "0"]);
        cmd.arg(part.join("%d.jpg"));
        background_priority(&mut cmd);
        cmd
    }
}

/// Set ids are file keys plus numbers: nothing that could leave the folder.
fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn read_index(dir: &Path) -> Option<TrickplayInfo> {
    serde_json::from_slice(&std::fs::read(dir.join("index.json")).ok()?).ok()
}

fn count_sheets(dir: &Path) -> u32 {
    (0..).take_while(|n| dir.join(format!("{n}.jpg")).is_file()).count() as u32
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use pley_transcoder_api::{MediaInfo, StreamKind};

    use super::*;

    #[test]
    fn intervals_scale_with_length() {
        assert_eq!(default_interval_ms(2 * 3_600_000), 10_000);
        assert_eq!(default_interval_ms(45 * 60_000), 7_000);
        assert_eq!(default_interval_ms(20 * 60_000), 3_000);
        assert_eq!(default_interval_ms(60_000), 2_000);
    }

    /// Several sets at once only where a run can be paused: elsewhere a
    /// second would keep reading through every playback start.
    #[tokio::test]
    async fn runs_default_to_five_where_they_can_be_paused() {
        let mut cmd = if cfg!(windows) {
            let mut cmd = Command::new("ping");
            cmd.args(["-n", "30", "127.0.0.1"]);
            cmd
        } else {
            let mut cmd = Command::new("sleep");
            cmd.arg("30");
            cmd
        };
        let mut child = cmd.kill_on_drop(true).stdout(std::process::Stdio::null()).spawn().unwrap();
        let pauses = crate::background::pause(&child, true);
        if pauses {
            assert!(crate::background::pause(&child, false));
        }
        child.kill().await.unwrap();
        assert_eq!(default_runs(), if pauses { 5 } else { 1 });
    }

    #[test]
    fn the_time_limit_grows_with_the_file() {
        assert_eq!(run_limit(0), Duration::from_secs(3600));
        assert_eq!(run_limit(1_000_000_000), Duration::from_secs(3600), "1 GB: an hour");
        assert_eq!(run_limit(70_000_000_000), Duration::from_secs(7000), "a 70 GB remux: nearly 2 hours");
    }

    #[test]
    fn threads_follow_a_cpu_limit() {
        assert_eq!(threads(Some(2.0)).as_deref(), Some("2"));
        assert_eq!(threads(Some(2.5)).as_deref(), Some("3"));
        assert_eq!(threads(Some(0.5)).as_deref(), Some("1"));
        assert_eq!(threads(None), None, "FFmpeg's own counts");
        // Before the input, they are the decoder's; the filters' are global.
        let dir = tempfile::tempdir().unwrap();
        let (trickplay, _, _) = without_ffmpeg(dir.path());
        let args = |threads| {
            let cmd = trickplay.command(Path::new("movie.mkv"), 0, "fps=1", false, threads, Path::new("set.part"));
            cmd.as_std().get_args().map(|arg| arg.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ")
        };
        let limited = args(Some("2"));
        assert!(
            limited.starts_with("-hide_banner -nostdin -loglevel error -filter_threads 2 -threads 2 "),
            "{limited}"
        );
        assert!(limited.contains(" -threads 2 -skip_frame nokey -format_whitelist "), "{limited}");
        let free = args(None);
        assert!(!free.contains("threads"), "{free}");
    }

    #[tokio::test]
    async fn probe_failures_that_may_pass_are_not_failures() {
        let rejected = anyhow::Error::from(Rejected("ffprobe failed: Invalid data found".into()));
        assert!(matches!(probe_error(rejected), ApiError::Failed(_)));
        let elapsed = tokio::time::timeout(Duration::ZERO, std::future::pending::<()>()).await.unwrap_err();
        let timed_out = anyhow::Error::from(elapsed).context("ffprobe timed out");
        assert!(matches!(probe_error(timed_out), ApiError::Internal(_)), "a share whose disks sleep");
        // Nor is ffprobe that can't start the file's failure.
        let dir = tempfile::tempdir().unwrap();
        let (trickplay, _, _) = without_ffmpeg(dir.path());
        let unprobed = dir.path().join("unprobed.mkv");
        std::fs::write(&unprobed, b"not probed yet").unwrap();
        let req = TrickplayRequest { path: unprobed.to_string_lossy().into_owned(), ..Default::default() };
        assert!(matches!(trickplay.generate(&req).await, Err(ApiError::Internal(_))));
    }

    #[tokio::test(start_paused = true)]
    async fn kept_fresh_marks_its_folder_while_it_runs() {
        let dir = tempfile::tempdir().unwrap();
        let part = dir.path().join("set.part");
        std::fs::create_dir(&part).unwrap();
        let made = std::fs::metadata(&part).unwrap().modified().unwrap();
        std::thread::sleep(Duration::from_millis(50));
        // A run paused through a long transcode writes nothing...
        let mut marked = 0;
        let paused = kept_fresh(std::future::pending::<()>(), KEEP_FRESH, || {
            marked += 1;
            freshen(&part);
        });
        assert!(tokio::time::timeout(Duration::from_secs(25 * 60), paused).await.is_err());
        // ...but its folder is marked as being written as it starts and every
        // 10 minutes: its time changes, and nothing is left in it.
        assert_eq!(marked, 3);
        assert!(std::fs::metadata(&part).unwrap().modified().unwrap() > made);
        assert_eq!(std::fs::read_dir(&part).unwrap().count(), 0);
    }

    /// Previews without FFmpeg (it isn't there), with one at a time, and a
    /// film ffprobe would have read: the request for it, and the background
    /// work it waits for.
    fn without_ffmpeg(dir: &Path) -> (Arc<Trickplay>, Arc<Background>, TrickplayRequest) {
        let ff = Arc::new(Ffmpeg {
            ffmpeg: dir.join("no-ffmpeg"),
            ffprobe: dir.join("no-ffprobe"),
            caps: Default::default(),
            segment_times_relative: true,
        });
        let prober = Arc::new(Prober::new(ff.clone(), Arc::new(crate::metrics::Metrics::default()), 1));
        let background = Background::new();
        let trickplay =
            Trickplay::new(ff, prober.clone(), background.clone(), dir.join("trickplay"), 1, Arc::default());
        let movie = dir.join("movie.mkv");
        std::fs::write(&movie, b"not really a movie").unwrap();
        let video = crate::matroska::tests::stream(0, StreamKind::Video, "h264");
        prober.remember(&movie, MediaInfo { duration_ms: Some(60_000), streams: vec![video], ..Default::default() });
        let req = TrickplayRequest { path: movie.to_string_lossy().into_owned(), ..Default::default() };
        (Arc::new(trickplay), background, req)
    }

    #[tokio::test]
    async fn files_that_do_not_answer_hold_a_few_threads_at_most() {
        let permits = Arc::new(Semaphore::new(2));
        let wait = Duration::from_millis(100);
        // Files on a share that doesn't answer are taken for files that
        // aren't there, but keep their threads and permits...
        let mut releases = Vec::new();
        for _ in 0..2 {
            let (release, held) = std::sync::mpsc::channel::<()>();
            assert_eq!(bounded(permits.clone(), wait, move || held.recv().ok()).await.unwrap(), None);
            releases.push(release);
        }
        // ...so that no other file is looked at, however often it is asked for.
        let looked = Arc::new(AtomicBool::new(false));
        for _ in 0..3 {
            let looked = looked.clone();
            let look = move || {
                looked.store(true, Ordering::SeqCst);
                Some(())
            };
            assert_eq!(bounded(permits.clone(), wait, look).await.unwrap(), None);
        }
        assert!(!looked.load(Ordering::SeqCst));
        // Once the share answers, files are looked at again.
        for release in releases {
            release.send(()).unwrap();
        }
        assert_eq!(bounded(permits, Duration::from_secs(10), || Some(3)).await.unwrap(), Some(3));
    }

    #[tokio::test(start_paused = true)]
    async fn a_file_is_waited_for_a_minute_at_most_its_turn_included() {
        let dir = tempfile::tempdir().unwrap();
        let (trickplay, _, req) = without_ffmpeg(dir.path());
        // Every look is taken by files that don't answer.
        let _taken = trickplay.looks.clone().acquire_many_owned(LOOKS_AT_ONCE as u32).await.unwrap();
        let asked = tokio::time::Instant::now();
        assert!(matches!(trickplay.generate(&req).await, Err(ApiError::NotFound(_))));
        assert!(asked.elapsed() >= FILE_LOOK);
    }

    #[tokio::test]
    async fn failed_generations_leave_no_lock_behind() {
        let dir = tempfile::tempdir().unwrap();
        let (trickplay, _, req) = without_ffmpeg(dir.path());
        // FFmpeg isn't there: the set isn't made, and its lock goes.
        assert!(trickplay.generate(&req).await.is_err());
        assert!(trickplay.locks.lock().is_empty());
    }

    #[tokio::test]
    async fn a_sets_folder_is_made_once_it_may_run() {
        let dir = tempfile::tempdir().unwrap();
        let (trickplay, background, req) = without_ffmpeg(dir.path());
        let folders = || std::fs::read_dir(dir.path().join("trickplay")).unwrap().count();
        // A playback starts: the set has its turn, but waits, and has no
        // folder yet that would look abandoned were the wait long.
        let starting = background.start();
        let run = tokio::spawn({
            let trickplay = trickplay.clone();
            async move { trickplay.generate(&req).await }
        });
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!((trickplay.running(), folders()), (1, 0));
        // Then it runs: FFmpeg isn't there, so the set isn't made, and
        // neither its folder nor its lock stays.
        drop(starting);
        assert!(run.await.unwrap().is_err());
        assert_eq!((trickplay.running(), folders()), (0, 0));
        assert!(trickplay.locks.lock().is_empty());
    }
}

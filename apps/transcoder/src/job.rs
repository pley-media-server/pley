// SPDX-License-Identifier: AGPL-3.0-only
//! HLS transcode jobs.
//!
//! A job owns a fixed segment layout (its VOD playlist never changes) and at
//! most one ffmpeg process producing a window of segments. Segment requests
//! are served from disk (or memory, see [`crate::memory`]) when ready;
//! otherwise the running process is awaited when it is about to produce the
//! segment, or a new process is started right at the requested segment
//! (seeking). Idle processes are stopped, idle jobs expire, and segments far
//! behind the playhead are pruned.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use pley_transcoder_api::{
    EncoderInfo, JobCreated, JobRequest, JobState, JobStatus, Limits, Load, MediaInfo, ProcessStatus, SegmentFormat,
    TonemapQuality, VideoCodec, VideoMode,
};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};
use tokio::process::ChildStdout;
use tokio::sync::{Notify, oneshot, watch};

use crate::adaptive;
use crate::background::{self, Background, Starting, Transcoding};
use crate::cpuload::CpuMeter;
use crate::error::{ApiError, ApiResult};
use crate::extract::Extractor;
use crate::ffmpeg::Ffmpeg;
use crate::fmp4::{BoxReader, Fragment, Init};
use crate::hls;
use crate::keyframes::KeyframeStore;
use crate::memory::{self, Lease, MemoryStore};
use crate::metrics::Metrics;
use crate::plan::{ArgsContext, Burn, MAX_DURATION_MS, PROCESS_WINDOW, Pipeline, Plan, READRATE, READRATE_BURST_MS};
use crate::probe::Prober;
use crate::share::{self, Lessons, Share};
use crate::tsedit::{self, Edits};
use crate::util::now_ms;

/// Wait for the running process instead of restarting when the requested
/// segment is at most this far ahead of it (3 segments of 4 s, 6 of 2 s).
const LOOKAHEAD: Duration = Duration::from_secs(12);
const SEGMENT_WAIT: Duration = Duration::from_secs(45);
/// Stop ffmpeg when nobody requested anything for this long (paused player).
const PROCESS_IDLE_MS: i64 = 60_000;
/// Forget jobs nobody touched for this long.
const JOB_EXPIRE_MS: i64 = 20 * 60_000;
/// Segments on disk kept behind the most recently requested one: this long
/// of them (90 segments of 4 s, 180 of 2 s). In memory, less
/// ([`memory::KEEP_BEHIND`]).
const KEEP_BEHIND: Duration = Duration::from_secs(6 * 60);
/// Give up on a segment after this many failed process starts at it.
const MAX_FAILURES: u32 = 2;
/// A process younger than this is not killed for a far-away request while
/// another request is waiting for it (prevents two requests from restarting
/// each other forever).
const PREEMPT_GRACE: Duration = Duration::from_secs(3);
/// What stopping a process costs, until one was stopped (FFmpeg takes
/// 0.1-0.3 s to exit, tone mapping on Vulkan the longest).
const STOP_COST: Duration = Duration::from_millis(300);
/// Segments deleted per lock acquisition when pruning.
const PRUNE_BATCH: usize = 32;
/// Jobs whose hardware encoder failed before the capability cache is
/// deleted, so that the next start tests the encoders again: a driver
/// update or a GPU gone leaves the cache saying they work.
const HARDWARE_FAILURES: u32 = 3;
/// A process starts (and background work waits, see [`crate::background`])
/// until it has made this many segments: what players buffer before they
/// play.
const STARTED_AFTER: u32 = 3;
/// Throttling: a process this far ahead of the last segment the player
/// asked for is paused (suspended), until it is less than [`RESUME_AHEAD`]
/// ahead. Players buffer ahead of what they show, so that is a few minutes
/// ahead of the picture. What is made beyond goes to waste when the player
/// stops or seeks, and takes the processor from other playbacks meanwhile.
const PAUSE_AHEAD: Duration = Duration::from_secs(90);
/// The lowest video bitrate a player may ask a job for as it plays (kbps).
pub const MIN_BITRATE_KBPS: u32 = 1_000;
/// Segments past the one its player asked for last that a new bitrate
/// leaves as they are: the next is likely being fetched already.
const REBITRATE_KEEP: u32 = 2;
const RESUME_AHEAD: Duration = Duration::from_secs(60);

/// Concurrency budget shared by all jobs.
pub struct Slots {
    used: Mutex<(u32, u32)>,
    max: u32,
    max_hw: u32,
    /// Background work, held while a slot is taken: transcodes come first.
    background: Arc<Background>,
}

impl Slots {
    pub fn new(limits: &Limits, background: Arc<Background>) -> Self {
        Self { used: Mutex::new((0, 0)), max: limits.max_jobs.max(1), max_hw: limits.max_hw_jobs, background }
    }

    /// A process slot, with a hardware session when `want_hw` and one is
    /// free; `None` when every slot is taken.
    pub(crate) fn take(self: &Arc<Self>, want_hw: bool) -> Option<Slot> {
        let hw = self.acquire(want_hw)?;
        Some(Slot { slots: self.clone(), hw, _transcoding: self.background.transcode() })
    }

    /// Reserve a process slot. Returns whether a hardware session was granted,
    /// or `None` when every slot is taken.
    fn acquire(&self, want_hw: bool) -> Option<bool> {
        let mut used = self.used.lock();
        if used.0 >= self.max {
            return None;
        }
        used.0 += 1;
        let hw = want_hw && used.1 < self.max_hw;
        if hw {
            used.1 += 1;
        }
        Some(hw)
    }

    fn release(&self, hw: bool) {
        let mut used = self.used.lock();
        used.0 = used.0.saturating_sub(1);
        if hw {
            used.1 = used.1.saturating_sub(1);
        }
    }

    pub fn counts(&self) -> (u32, u32) {
        *self.used.lock()
    }
}

/// A transcode slot, given back however the process (or conversion)
/// holding it ends.
pub(crate) struct Slot {
    slots: Arc<Slots>,
    /// Whether it holds a hardware encoder session.
    pub hw: bool,
    _transcoding: Transcoding,
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.slots.release(self.hw);
    }
}

pub struct JobManager {
    ff: Arc<Ffmpeg>,
    prober: Arc<Prober>,
    keyframes: Arc<KeyframeStore>,
    extractor: Arc<Extractor>,
    metrics: Arc<Metrics>,
    jobs_dir: PathBuf,
    /// libplacebo's shader cache (see [`crate::plan::shader_cache_option`]).
    shader_cache: Option<PathBuf>,
    /// The capability cache, deleted after [`HARDWARE_FAILURES`] jobs whose
    /// hardware encoder failed.
    capabilities: Option<PathBuf>,
    hardware_failures: AtomicU32,
    pub limits: Limits,
    pub slots: Arc<Slots>,
    /// Background work, held while jobs start.
    pub background: Arc<Background>,
    jobs: Mutex<HashMap<String, Arc<Job>>>,
    /// The folder in RAM that jobs asking for it keep their segments in
    /// ([`crate::memory`]), when this system has one.
    memory: OnceLock<Arc<MemoryStore>>,
    /// Set on shutdown: no new jobs are accepted.
    closed: AtomicBool,
    /// Job creations begun so far, and when the last began: reading ahead
    /// (`prefetch.rs`) keeps out of a starting playback's way.
    creations: AtomicU64,
    last_creation: Mutex<Option<Instant>>,
    /// How busy the CPU is, and with whose work.
    cpu: Arc<CpuMeter>,
    /// Memory the transcoder can still take, in bytes, looked at with the
    /// CPU ([`memory::available`]); `u64::MAX` where unknown.
    memory_available: AtomicU64,
    /// What jobs falling behind their players taught (see [`crate::share`]).
    lessons: Mutex<Lessons>,
    /// Whether the CPU may decode for a GPU that falls behind (see
    /// [`crate::share`]).
    cpu_decode: AtomicBool,
}

pub struct Job {
    pub id: String,
    pub request: JobRequest,
    pub media: Arc<MediaInfo>,
    pub plan: Plan,
    pub input: PathBuf,
    pub dir: PathBuf,
    pub created_at_ms: i64,
    last_access_ms: AtomicI64,
    last_requested: AtomicU32,
    /// Where each segment made is: `None` until it is.
    ready: Mutex<Vec<Option<Place>>>,
    ready_count: AtomicU32,
    /// Segments that requests are currently waiting for (one entry per request).
    waiting: Mutex<Vec<u32>>,
    /// Set when the job is removed; no process may be started afterwards.
    closed: AtomicBool,
    notify: Notify,
    process: tokio::sync::Mutex<Option<Proc>>,
    runtime: Mutex<Runtime>,
    served_segments: AtomicU64,
    served_bytes: AtomicU64,
    /// Its lower renditions ([`JobRequest::variants`]), each a job of its
    /// own in a folder of this one's (`v1`…). The job manager knows this one
    /// only: they come and go with it.
    variants: Vec<Arc<Job>>,
    /// What the segments are named with: "" for a job's own, "v1-"… for its
    /// variants'.
    prefix: String,
    /// The rendition its player asked for last: 0 for its own, 1 for its
    /// first variant…
    active: AtomicU32,
    /// Its folder in RAM, when it asked for memory and may have it.
    memory: Option<JobMemory>,
    /// The video bitrate its player asked for since it started (kbps), in
    /// place of the plan's: see [`JobManager::set_bitrate`].
    bitrate: Mutex<Option<u32>>,
    /// An fMP4 job's initialization segment (`init.mp4` in its folder on
    /// disk), once its first process wrote it: every later process must
    /// write the same (see [`Job::store_init`]).
    init: Mutex<Option<Arc<[u8]>>>,
    /// What made its init: every later process makes its video the same
    /// way (see [`restart_encoder`]).
    init_encoder: Mutex<Option<InitEncoder>>,
}

/// The encoder an fMP4 job's init was made with. The init holds the
/// encoder's parameter sets and B-frame delay, which MPEG-TS carried in
/// every segment: a process on another encoder, or at another x264 preset,
/// would make segments that don't play after the init players have.
#[derive(Debug, Clone, PartialEq, Eq)]
struct InitEncoder {
    /// Made on the GPU, with the planned hardware encoder.
    hardware: bool,
    /// The software encoder that stood in for the planned hardware one,
    /// when one did.
    stand_in: Option<String>,
    /// The automatic preset it was made at (index in [`adaptive::LADDER`]).
    preset: Option<usize>,
}

/// What a process encodes its video with (see [`restart_encoder`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EncoderChoice {
    /// The planned encoder: on the GPU when it is a hardware one.
    Planned,
    /// The software stand-in: the hardware encoder failed the job.
    Retreat,
    /// The software stand-in: every hardware session is in use.
    NoSession,
    /// The software stand-in, which made the fMP4 job's init.
    Kept,
}

impl EncoderChoice {
    fn stands_in(self) -> bool {
        self != EncoderChoice::Planned
    }
}

/// What the next process of a job encodes with, and at which automatic
/// preset (`preset` asked for, `None` to choose one): `planned_hw` when the
/// planned encoder is a hardware one, `stand_in` when a software encoder
/// can stand in for it, `hardware_failed` once the hardware encoder failed
/// the job, and `hw_slot` when the process has a hardware session. Before
/// an fMP4 job has an init (`pinned`), the stand-in takes over from a
/// failed or busy hardware encoder. Once it has one, every process makes
/// the video as the one that made it: the same encoder at the same preset,
/// or none.
fn restart_encoder(
    pinned: Option<&InitEncoder>,
    planned_hw: bool,
    stand_in: bool,
    hardware_failed: bool,
    preset: Option<usize>,
    hw_slot: bool,
) -> ApiResult<(EncoderChoice, Option<usize>)> {
    let Some(pinned) = pinned else {
        let choice = if stand_in && hardware_failed {
            EncoderChoice::Retreat
        } else if planned_hw && !hw_slot {
            if !stand_in {
                return Err(ApiError::Busy(
                    "all hardware encoder sessions are in use and no software encoder is available".into(),
                ));
            }
            EncoderChoice::NoSession
        } else {
            EncoderChoice::Planned
        };
        return Ok((choice, preset));
    };
    let choice = if pinned.hardware {
        if hardware_failed {
            return Err(ApiError::Failed(
                "the hardware encoder failed after the stream began; fMP4 segments can't change encoder".into(),
            ));
        }
        if !hw_slot {
            return Err(ApiError::Busy(
                "all hardware encoder sessions are in use, and this stream can't change encoder".into(),
            ));
        }
        EncoderChoice::Planned
    } else if pinned.stand_in.is_some() {
        if !stand_in {
            return Err(ApiError::Failed("the encoder this stream began with is gone".into()));
        }
        EncoderChoice::Kept
    } else {
        EncoderChoice::Planned
    };
    Ok((choice, pinned.preset))
}

/// Where a segment is made: in the job's folder on disk, or in RAM
/// ([`crate::memory`]). Each process chooses for the segments it makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Disk,
    Memory,
}

/// A job's (a rendition's) folder in RAM.
struct JobMemory {
    store: Arc<MemoryStore>,
    dir: PathBuf,
    /// What its segments are reckoned to take ([`memory::reckoned_bytes`]).
    want: u64,
    /// Its reservation, while its segments go to memory.
    lease: Mutex<Option<Lease>>,
    /// Bytes of its segments in memory now.
    bytes: AtomicU64,
}

impl JobMemory {
    /// A job's folder in RAM, `folder` in `store`, with what FFmpeg reads
    /// from its folder on disk, `disk` (burned-in text subtitles): each
    /// process runs in the folder it writes its segments to. `None` when
    /// it can't be made: its segments go to disk.
    async fn make(
        store: Option<&Arc<MemoryStore>>,
        folder: Option<PathBuf>,
        disk: &Path,
        plan: &Plan,
        media: &MediaInfo,
    ) -> Option<JobMemory> {
        let (store, folder) = (store?, folder?);
        let made = async {
            tokio::fs::create_dir_all(&folder).await?;
            if let Some(Burn::Text { file_name, .. }) = &plan.burn {
                tokio::fs::copy(disk.join(file_name), folder.join(file_name)).await?;
            }
            std::io::Result::Ok(())
        };
        if let Err(err) = made.await {
            tracing::warn!("segments go to disk: {} can't be made: {err}", folder.display());
            return None;
        }
        Some(JobMemory {
            store: store.clone(),
            dir: folder,
            want: memory::reckoned_bytes(plan, media),
            lease: Mutex::new(None),
            bytes: AtomicU64::new(0),
        })
    }

    /// Its segments of `bytes` left memory.
    fn removed(&self, bytes: u64) {
        // No more than it counted (a prune finishing after the job went).
        if let Ok(before) = self.bytes.try_update(Relaxed, Relaxed, |b| Some(b.saturating_sub(bytes))) {
            self.store.removed(before.min(bytes));
        }
    }

    /// The job is gone: so are its segments in memory, and its reservation.
    fn release(&self) {
        self.store.removed(self.bytes.swap(0, Relaxed));
        self.lease.lock().take();
    }
}

/// A job's file by name ([`JobRequest::variants`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobFile {
    /// `master.m3u8`.
    Master,
    /// A rendition's media playlist: `index.m3u8` (0), `v1.m3u8`…
    Playlist(usize),
    /// An fMP4 rendition's initialization segment: `init.mp4` (0),
    /// `v1-init.mp4`…
    Init(usize),
    /// A rendition's segment: `12.ts` or `12.m4s` (0), `v1-12.ts` or
    /// `v1-12.m4s`…
    Segment(usize, u32, SegmentFormat),
}

impl JobFile {
    pub fn parse(name: &str) -> Option<JobFile> {
        let rendition = |k: &str| k.parse().ok().filter(|k| *k > 0);
        match name {
            "master.m3u8" => Some(JobFile::Master),
            "index.m3u8" => Some(JobFile::Playlist(0)),
            "init.mp4" => Some(JobFile::Init(0)),
            _ => {
                if let Some(playlist) = name.strip_suffix(".m3u8") {
                    return playlist.strip_prefix('v').and_then(rendition).map(JobFile::Playlist);
                }
                if let Some(init) = name.strip_suffix("-init.mp4") {
                    return init.strip_prefix('v').and_then(rendition).map(JobFile::Init);
                }
                let (segment, format) = match name.strip_suffix(".ts") {
                    Some(segment) => (segment, SegmentFormat::Ts),
                    None => (name.strip_suffix(".m4s")?, SegmentFormat::Fmp4),
                };
                match segment.strip_prefix('v').and_then(|s| s.split_once('-')) {
                    Some((k, n)) => Some(JobFile::Segment(rendition(k)?, n.parse().ok()?, format)),
                    None => Some(JobFile::Segment(0, segment.parse().ok()?, format)),
                }
            }
        }
    }
}

#[derive(Default)]
struct Runtime {
    state: JobState,
    /// Counts the processes started: the latest one's is the only exit that
    /// says how the job stands (one stopped long before may go after
    /// another started, see [`Proc::stop`]).
    generation: u64,
    process: Option<ProcessStatus>,
    last_error: Option<String>,
    restarts: u32,
    started_any: bool,
    warnings: Vec<String>,
    encoder_override: Option<EncoderInfo>,
    /// The hardware encoder failed before its first segment: the software
    /// one takes over (see [`JobManager::spawn`]).
    hardware_failed: bool,
    /// FFmpeg said the GPU couldn't decode the video, and decodes it on the
    /// CPU instead.
    decoded_in_software: bool,
    failures: HashMap<u32, u32>,
    /// The automatic preset of the running process (index in
    /// [`adaptive::LADDER`]), and when it started or moved.
    preset: Option<(usize, Instant)>,
    /// What the latest process does on the GPU, when libplacebo tone maps
    /// (see [`crate::share`]).
    share: Option<Share>,
    /// What a restart costs this job: from starting FFmpeg to its first
    /// segment (the latest process's), and stopping the one before.
    start_cost: Option<Duration>,
    stop_cost: Option<Duration>,
}

struct Proc {
    /// The segment it started at.
    start: u32,
    end: u32,
    /// Where it writes its segments.
    place: Place,
    started: Instant,
    next: Arc<AtomicU32>,
    /// [`LOOKAHEAD`] in the job's segments.
    lookahead: u32,
    pace: Arc<Mutex<Pace>>,
    exited: watch::Receiver<bool>,
    kill: Option<oneshot::Sender<()>>,
    /// Whether it is paused (see [`PAUSE_AHEAD`]): its task suspends or
    /// resumes it as this changes. Its segment reader pauses it too.
    pause: Arc<watch::Sender<bool>>,
}

/// How fast a process has been going over the last few seconds. FFmpeg's
/// own `speed` is the average since the process started, which the burst
/// before the read rate applies keeps high long after it ended.
#[derive(Default)]
struct Pace {
    /// When the process was where (ms of media), oldest first.
    samples: VecDeque<(Instant, i64)>,
}

impl Pace {
    /// How long the speed is measured over.
    const WINDOW: Duration = Duration::from_secs(4);
    /// The least of them that gives a speed.
    const MIN_SPAN: Duration = Duration::from_secs(2);
    /// How long its steady speed is measured over, which judges its share
    /// of the GPU (see [`share::verdict`]).
    const STEADY: Duration = Duration::from_secs(20);

    fn record(&mut self, at: Instant, out_ms: i64) {
        self.samples.push_back((at, out_ms));
        while self.samples.get(1).is_some_and(|(t, _)| at.saturating_duration_since(*t) >= Self::STEADY) {
            self.samples.pop_front();
        }
    }

    /// Media seconds made per second lately.
    fn speed(&self) -> Option<f64> {
        self.speed_over(Self::WINDOW, Self::MIN_SPAN)
    }

    /// The same over the last 20 s, once there are 15.
    fn steady_speed(&self) -> Option<f64> {
        self.speed_over(Self::STEADY, Self::STEADY * 3 / 4)
    }

    /// The speed from the latest sample `window` old (or the oldest) to the
    /// last, when they are `least` apart.
    fn speed_over(&self, window: Duration, least: Duration) -> Option<f64> {
        let &(t1, m1) = self.samples.back()?;
        let old = self.samples.iter().rev().find(|(t, _)| t1.saturating_duration_since(*t) >= window);
        let &(t0, m0) = old.or(self.samples.front())?;
        let span = t1.saturating_duration_since(t0);
        (span >= least).then(|| (m1 - m0) as f64 / 1000.0 / span.as_secs_f64())
    }
}

impl Proc {
    fn running(&self) -> bool {
        !*self.exited.borrow()
    }

    /// Whether this process will produce segment `n` soon, so a request for it
    /// should wait rather than restart ffmpeg. Segments the process already
    /// passed (pruned or skipped) are not coming back.
    fn will_produce(&self, n: u32) -> bool {
        let next = self.next.load(Relaxed);
        n >= next && n < self.end && n <= next + self.lookahead
    }

    async fn stop(mut self) {
        if let Some(kill) = self.kill.take() {
            let _ = kill.send(());
        }
        let _ = tokio::time::timeout(Duration::from_secs(10), self.exited.wait_for(|exited| *exited)).await;
    }

    fn paused(&self) -> bool {
        *self.pause.borrow()
    }

    /// Pauses or resumes the process; whether that changed anything. Its
    /// speed is measured anew after a pause.
    fn set_paused(&self, on: bool) -> bool {
        let changed = self.pause.send_if_modified(|paused| std::mem::replace(paused, on) != on);
        if changed && !on {
            self.pace.lock().samples.clear();
        }
        changed
    }
}

impl JobManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        ff: Arc<Ffmpeg>,
        prober: Arc<Prober>,
        keyframes: Arc<KeyframeStore>,
        extractor: Arc<Extractor>,
        metrics: Arc<Metrics>,
        jobs_dir: PathBuf,
        shader_cache: Option<PathBuf>,
        capabilities: Option<PathBuf>,
        limits: Limits,
        background: Arc<Background>,
    ) -> Arc<Self> {
        Arc::new(Self {
            slots: Arc::new(Slots::new(&limits, background.clone())),
            background,
            ff,
            prober,
            keyframes,
            extractor,
            metrics,
            jobs_dir,
            shader_cache,
            capabilities,
            hardware_failures: AtomicU32::new(0),
            limits,
            jobs: Mutex::default(),
            memory: OnceLock::new(),
            closed: AtomicBool::new(false),
            creations: AtomicU64::new(0),
            last_creation: Mutex::new(None),
            cpu: Arc::default(),
            memory_available: AtomicU64::new(u64::MAX),
            lessons: Mutex::default(),
            cpu_decode: AtomicBool::new(true),
        })
    }

    /// The GPU decodes what it planned to, however far behind it falls (see
    /// [`crate::share`]).
    pub fn never_decode_on_cpu(&self) {
        self.cpu_decode.store(false, Relaxed);
    }

    /// How many job creations have begun so far.
    pub fn creations(&self) -> u64 {
        self.creations.load(Relaxed)
    }

    /// A job creation begins.
    pub(crate) fn note_creation(&self) {
        *self.last_creation.lock() = Some(Instant::now());
        self.creations.fetch_add(1, Relaxed);
    }

    /// Whether a job creation began within `window`.
    pub fn created_within(&self, window: Duration) -> bool {
        self.last_creation.lock().is_some_and(|at| at.elapsed() < window)
    }

    /// Jobs asking for memory keep their segments in `store` from now on.
    pub fn keep_in_memory(&self, store: Arc<MemoryStore>) {
        let _ = self.memory.set(store);
    }

    /// Bytes of segments in memory now.
    pub fn memory_bytes(&self) -> u64 {
        self.memory.get().map_or(0, |store| store.used())
    }

    /// How busy the processors are, looked at every few seconds (the
    /// reaper's turn).
    pub fn cpu(&self) -> Arc<CpuMeter> {
        self.cpu.clone()
    }

    /// What the computer has to spare, as of the last look (`/v1/status`).
    pub fn load(&self) -> Load {
        let memory = self.memory_available.load(Relaxed);
        Load {
            cores: self.cpu.room().unwrap_or_else(|| f64::from(crate::util::processors())),
            busy: self.cpu.busy(),
            foreign: self.cpu.foreign(),
            memory_available: (memory != u64::MAX).then_some(memory),
        }
    }

    pub fn get(&self, id: &str) -> Option<Arc<Job>> {
        self.jobs.lock().get(id).cloned()
    }

    pub fn list(&self) -> Vec<Arc<Job>> {
        let mut jobs: Vec<_> = self.jobs.lock().values().cloned().collect();
        jobs.sort_by_key(|j| j.created_at_ms);
        jobs
    }

    /// Every job's renditions ([`Job::renditions`]), whose processes run.
    fn renditions(&self) -> Vec<Arc<Job>> {
        self.list().iter().flat_map(Job::renditions).collect()
    }

    pub async fn create(self: &Arc<Self>, req: JobRequest) -> ApiResult<JobCreated> {
        if self.closed.load(Relaxed) {
            return Err(ApiError::Busy("the transcoder is shutting down".into()));
        }
        if !self.ff.caps.ffmpeg_available {
            return Err(ApiError::Failed("ffmpeg is not available on the transcoder host".into()));
        }
        self.note_creation();
        // Until its process takes over (see `spawn`).
        let _starting = self.background.start();
        let input = PathBuf::from(&req.input);
        match tokio::fs::metadata(&input).await {
            Ok(m) if m.is_file() => {}
            Ok(_) => return Err(ApiError::BadRequest(format!("{} is not a file", req.input))),
            Err(_) => return Err(ApiError::NotFound(format!("input not found: {}", req.input))),
        }
        let wants_copy = req.video.as_ref().is_some_and(|v| v.mode == VideoMode::Copy);
        // Remuxing needs the container start time (keyframe times are absolute,
        // seeking is relative to it); media info from older servers lacks it.
        // An implausible duration (stored from an older transcoder's probe) is
        // probed again: the probe now looks past it.
        let media = match req
            .media
            .clone()
            .filter(|m| m.duration_ms.is_some_and(|d| d > 0 && d <= MAX_DURATION_MS) && !m.streams.is_empty())
            .filter(|m| !wants_copy || m.start_ms.is_some())
        {
            Some(media) => Arc::new(media),
            None => self.prober.probe(&input).await.map_err(|e| ApiError::Failed(format!("probe failed: {e:#}")))?,
        };
        let keyframes = if wants_copy {
            let index = self.keyframes.get_or_read(&input).await;
            if index.is_none() {
                self.keyframes.request(&input);
            }
            index
        } else {
            None
        };
        let mut plan = Plan::build(&req, &media, &self.ff, keyframes.as_deref().map(Vec::as_slice))
            .map_err(ApiError::BadRequest)?;
        // libplacebo shares the GPU with the jobs tone mapping on it now.
        plan.share_tonemapping(self.ff.caps.tonemap_speed.as_ref(), &media, self.tonemapping_load());

        let id = uuid::Uuid::new_v4().simple().to_string();
        let dir = self.jobs_dir.join(&id);
        // `jobs` as it was made at start-up, even if something took it away
        // since: never made again with the default permissions, nor a link.
        crate::own_folder(&self.jobs_dir)?;
        tokio::fs::create_dir(&dir).await?;
        // Its folder in RAM, when it asks for memory and is throttled: its
        // lead on the player is bounded then.
        let store = self.memory.get().filter(|_| req.options.in_memory && plan.throttle);
        let memory_dir = store.map(|store| store.root().join(&id));
        let unmade = Unmade(std::iter::once(dir.clone()).chain(memory_dir.clone()).collect());
        let mut fonts = None;
        if let Some(Burn::Text { source, file_name, .. }) = &plan.burn {
            // The file's fonts are read while the track is extracted, which
            // takes longer: they cost the start nothing.
            let ass = dir.join(file_name);
            let extracted = self.extractor.extract_ass(&input, source, &ass);
            let (extracted, found) = tokio::join!(extracted, self.extractor.attached_fonts(&input));
            extracted?;
            fonts = found;
        }
        plan.use_fonts(fonts.clone());

        let start_segment = plan.segment_for(req.start_ms.unwrap_or(0));
        let mut variants = Vec::new();
        for (k, (variant, mut plan)) in self.variant_plans(&req, &media, &plan).into_iter().enumerate() {
            let k = k + 1;
            plan.use_fonts(fonts.clone());
            let variant_dir = dir.join(format!("v{k}"));
            tokio::fs::create_dir_all(&variant_dir).await?;
            // Its subtitles are the job's: extracted once.
            if let Some(Burn::Text { file_name, .. }) = &plan.burn {
                tokio::fs::copy(dir.join(file_name), variant_dir.join(file_name)).await?;
            }
            let folder = memory_dir.as_ref().map(|d| d.join(format!("v{k}")));
            let memory = JobMemory::make(store, folder, &variant_dir, &plan, &media).await;
            let job =
                Job::new(format!("{id}-v{k}"), variant, media.clone(), plan, input.clone(), variant_dir, start_segment);
            variants.push(Arc::new(Job { prefix: format!("v{k}-"), memory, ..job }));
        }
        let memory = JobMemory::make(store, memory_dir, &dir, &plan, &media).await;
        let created = JobCreated {
            id: id.clone(),
            duration_ms: plan.duration_ms,
            segment_seconds: plan.segment_seconds,
            segment_count: plan.segment_count(),
            video: plan.video_plan(&media, None),
            audio: plan.audio_plan(&media),
            warnings: plan.warnings.clone(),
            variants: variants.len() as u32,
            segment_format: plan.segment_format,
        };
        let job =
            Arc::new(Job { variants, memory, ..Job::new(id.clone(), req, media, plan, input, dir, start_segment) });
        self.jobs.lock().insert(id.clone(), job.clone());
        unmade.made();
        Metrics::inc(&self.metrics.jobs_created);
        tracing::info!(
            job = %id,
            "new job '{}' for {}: {}",
            job.request.label.as_deref().unwrap_or("-"),
            job.request.owner.as_deref().unwrap_or("-"),
            describe(&created)
        );

        // Start producing right away so the first segment request is fast.
        if let Err(err) = self.ensure_producing(&job, start_segment).await {
            if matches!(err, ApiError::Busy(_)) {
                self.remove(&id).await;
                return Err(err);
            }
            tracing::warn!(job = %id, "could not pre-start transcode: {err:?}");
        }
        // The jobs already on the GPU make room for it now.
        self.share_gpu().await;
        Ok(created)
    }

    /// The lower renditions of `req` ([`JobRequest::variants`]) that make a
    /// difference: each no bigger and with fewer bits than the one above it,
    /// and laid out as `top`, so a player moves between them at any segment.
    fn variant_plans(&self, req: &JobRequest, media: &MediaInfo, top: &Plan) -> Vec<(JobRequest, Plan)> {
        let mut made: Vec<(JobRequest, Plan)> = Vec::new();
        if top.video.as_ref().is_none_or(|v| v.copy) {
            return made;
        }
        for want in &req.variants {
            let mut variant = JobRequest { variants: Vec::new(), ..req.clone() };
            if let Some(v) = variant.video.as_mut() {
                (v.max_width, v.max_height, v.max_bitrate_kbps) =
                    (want.max_width, want.max_height, want.max_bitrate_kbps);
            }
            let Ok(mut plan) = Plan::build(&variant, media, &self.ff, None) else { continue };
            plan.tone_map_like(top);
            let above = made.last().map_or(top, |(_, plan)| plan);
            let (Some(v), Some(a)) = (plan.video.as_ref(), above.video.as_ref()) else { continue };
            let lower = !v.copy && v.width * v.height <= a.width * a.height && v.bitrate_kbps < a.bitrate_kbps;
            if lower && plan.boundaries == top.boundaries {
                made.push((variant, plan));
            }
        }
        made
    }

    /// Segment `n` of rendition `k` of `job` ([`JobFile`]), the rendition
    /// its player takes from now on: the others stop making segments (those
    /// made stay, for when it comes back).
    pub async fn rendition_segment(
        self: &Arc<Self>,
        job: &Arc<Job>,
        k: usize,
        n: u32,
    ) -> ApiResult<(Arc<Job>, PathBuf)> {
        let (rendition, _) = job.activate(k)?;
        let path = self.segment(&rendition, n).await?;
        // Asked for before its player moved on: made, and stopped again.
        if job.active.load(Relaxed) as usize != k {
            park(rendition.clone());
        }
        Ok((rendition, path))
    }

    /// The initialization segment of fMP4 rendition `k` of `job`
    /// ([`JobFile::Init`]). Once made, it is there at once; before, the
    /// rendition is taken as by [`JobManager::rendition_segment`] for the
    /// segment its player asked for last, and the answer waits for the
    /// process that makes it.
    pub async fn rendition_init(self: &Arc<Self>, job: &Arc<Job>, k: usize) -> ApiResult<(Arc<Job>, PathBuf)> {
        let rendition = job.rendition(k)?;
        if !rendition.fmp4() {
            return Err(ApiError::NotFound("its segments are MPEG-TS: it has no init.mp4".into()));
        }
        if rendition.has_init() {
            job.touch();
            return Ok((rendition.clone(), rendition.init_path()));
        }
        let (rendition, before) = job.activate(k)?;
        // Where the player is, in whichever rendition it took last.
        let n = job.rendition(before)?.last_requested.load(Relaxed).min(rendition.plan.segment_count() - 1);
        rendition.touch();
        let waiting = Waiting::register(&rendition, n);
        let deadline = Instant::now() + SEGMENT_WAIT;
        let made = loop {
            let notified = rendition.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if rendition.has_init() {
                break Ok(rendition.init_path());
            }
            self.ensure_producing(&rendition, n).await?;
            let now = Instant::now();
            if now >= deadline {
                break Err(ApiError::Timeout("the init segment was not ready in time".into()));
            }
            let wait = (deadline - now).min(Duration::from_secs(1));
            tokio::select! {
                _ = &mut notified => {}
                _ = tokio::time::sleep(wait) => {}
            }
        };
        drop(waiting);
        if job.active.load(Relaxed) as usize != k {
            park(rendition.clone());
        }
        Ok((rendition, made?))
    }

    /// Changes the video bitrate of `job` as it is sent (docs/QUALITY.md,
    /// "Players that go black between streams"): the segments its player
    /// hasn't fetched yet are made again at `kbps`, the same size and
    /// layout, so the player plays on from the same playlist without a new
    /// stream. Between [`MIN_BITRATE_KBPS`] and the job's own bitrate. Not
    /// for fMP4 jobs: another rate can make x264 choose another level, which
    /// the init players have says (see [`InitEncoder`]).
    pub async fn set_bitrate(self: &Arc<Self>, job: &Arc<Job>, kbps: u32) -> ApiResult<u32> {
        if job.fmp4() {
            return Err(ApiError::BadRequest("an fMP4 stream's bitrate can't change as it is made".into()));
        }
        let planned = match job.plan.video.as_ref() {
            Some(v) if !v.copy => v.bitrate_kbps,
            _ => return Err(ApiError::BadRequest("the video isn't converted: its bitrate is the file's".into())),
        };
        let kbps = kbps.clamp(MIN_BITRATE_KBPS.min(planned), planned);
        job.touch();
        let mut guard = job.process.lock().await;
        if job.closed.load(Relaxed) {
            return Err(ApiError::JobNotFound(format!("job {} was removed", job.id)));
        }
        let before = job.bitrate.lock().replace(kbps).unwrap_or(planned);
        if before == kbps {
            return Ok(kbps);
        }
        // What the player may be fetching now (the one it asked for last and
        // the next) stays; the rest is made again.
        let from = job.last_requested.load(Relaxed).saturating_add(REBITRATE_KEEP).min(job.plan.segment_count());
        let running = guard.as_ref().is_some_and(Proc::running);
        if let Some(old) = guard.take() {
            old.stop().await;
        }
        let forgotten = job.forget_from(from);
        tracing::info!(
            job = %job.id,
            "bitrate {before} → {kbps} kbps from segment {from} ({forgotten} made again)"
        );
        if running && from < job.plan.segment_count() {
            *guard = Some(self.spawn(job, from, None)?);
        }
        drop(guard);
        job.notify.notify_waiters();
        Ok(kbps)
    }

    /// Path of segment `n`, waiting for or (re)starting ffmpeg as needed.
    pub async fn segment(self: &Arc<Self>, job: &Arc<Job>, n: u32) -> ApiResult<PathBuf> {
        if n >= job.plan.segment_count() {
            return Err(ApiError::NotFound(format!("segment {n} out of range")));
        }
        job.touch();
        job.last_requested.store(n, Relaxed);
        let _waiting = Waiting::register(job, n);
        let deadline = Instant::now() + SEGMENT_WAIT;
        loop {
            let notified = job.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if job.is_ready(n) {
                return Ok(job.segment_path(n));
            }
            self.ensure_producing(job, n).await?;
            if job.is_ready(n) {
                return Ok(job.segment_path(n));
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(ApiError::Timeout(format!("segment {n} was not ready in time")));
            }
            let wait = (deadline - now).min(Duration::from_secs(1));
            tokio::select! {
                _ = &mut notified => {}
                _ = tokio::time::sleep(wait) => {}
            }
        }
    }

    async fn ensure_producing(self: &Arc<Self>, job: &Arc<Job>, n: u32) -> ApiResult<()> {
        let mut guard = job.process.lock().await;
        // Checked under the process lock: `remove` sets the flag before taking
        // the lock, so no process can be started for a removed job.
        if job.closed.load(Relaxed) {
            return Err(ApiError::JobNotFound(format!("job {} was removed", job.id)));
        }
        if job.is_ready(n) {
            return Ok(());
        }
        if let Some(p) = guard.as_ref().filter(|p| p.running()) {
            // A segment the player waits for: a paused process goes on at once.
            p.set_paused(false);
            if p.will_produce(n) {
                match job.throttled_wait(p, n) {
                    None => return Ok(()),
                    Some(wait) => tracing::debug!(
                        job = %job.id,
                        "segment {n}: restarting rather than waiting {:.1} s for the throttled process",
                        wait.as_secs_f64()
                    ),
                }
            } else if p.started.elapsed() < PREEMPT_GRACE && job.is_awaited(|m| m != n && p.will_produce(m)) {
                // Another request is waiting for this young process: let it be
                // served first. This request retries after the next segment or second.
                return Ok(());
            }
        }
        {
            let rt = job.runtime.lock();
            if rt.failures.get(&n).copied().unwrap_or(0) >= MAX_FAILURES {
                return Err(ApiError::Failed(rt.last_error.clone().unwrap_or_else(|| "transcode failed".into())));
            }
        }
        if let Some(old) = guard.take() {
            let running = old.running();
            let stopping = Instant::now();
            old.stop().await;
            if running {
                job.runtime.lock().stop_cost = Some(stopping.elapsed());
            }
        }
        *guard = Some(self.spawn(job, n, None)?);
        Ok(())
    }

    /// A job's hardware encoder failed: after [`HARDWARE_FAILURES`], the
    /// next start tests the encoders again.
    fn hardware_failed(&self) {
        let failures = self.hardware_failures.fetch_add(1, Relaxed) + 1;
        let Some(cache) = self.capabilities.as_ref().filter(|_| failures == HARDWARE_FAILURES) else { return };
        match std::fs::remove_file(cache) {
            Ok(()) => tracing::warn!(
                "the hardware encoder failed {failures} jobs: the encoders are tested again at the next start"
            ),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => tracing::warn!("could not delete {}: {err}", cache.display()),
        }
    }

    /// Software encodes running now.
    fn software_encodes(&self) -> u32 {
        self.renditions().iter().filter(|job| job.encodes_in_software()).count() as u32
    }

    /// What the automatic preset depends on for `job` now, counting it.
    fn load_for(&self, job: &Job) -> adaptive::Load {
        let (pixels, hevc) = job
            .plan
            .video
            .as_ref()
            .map_or((0, false), |v| (v.width.max(0) as u64 * v.height.max(0) as u64, v.codec == VideoCodec::Hevc));
        let others = self.renditions().iter().filter(|j| j.id != job.id && j.encodes_in_software()).count() as u32;
        adaptive::Load { software_jobs: others + 1, cores: crate::util::processors(), pixels, hevc }
    }

    /// Moves jobs on the automatic preset a step faster or slower when their
    /// lead over the player and their speed call for it (see
    /// [`crate::adaptive`]), restarting ffmpeg where it was.
    async fn adapt(self: &Arc<Self>) {
        if self.software_encodes() == 0 {
            return;
        }
        for job in self.renditions() {
            // An fMP4 job's preset is that of its init (see [`InitEncoder`]).
            if job.init_encoder.lock().is_some() {
                continue;
            }
            let Some((current, moved_at)) = job.runtime.lock().preset else { continue };
            let Ok(mut guard) = job.process.try_lock() else { continue };
            // A paused process is far ahead already: it moves once it goes on.
            let Some(proc) = guard.as_ref().filter(|p| p.running() && !p.paused()) else { continue };
            let next = proc.next.load(Relaxed);
            if next >= proc.end {
                continue;
            }
            // How fast it goes lately (FFmpeg's own speed averages in its
            // burst, and the time it was paused).
            let speed = proc.pace.lock().speed().or_else(|| job.runtime.lock().process.as_ref().and_then(|p| p.speed));
            let lead = job.lead_secs(next);
            let target = adaptive::target(self.load_for(&job));
            let to = match adaptive::decide(current, target, lead, speed, moved_at.elapsed()) {
                adaptive::Move::Faster => current - 1,
                adaptive::Move::Slower => current + 1,
                adaptive::Move::Stay => continue,
            };
            tracing::info!(
                job = %job.id,
                "preset {} → {} ({lead:.0} s ahead of the player at {}× real time)",
                adaptive::LADDER[current],
                adaptive::LADDER[to],
                speed.map_or("?".into(), |s| format!("{s:.1}"))
            );
            if let Some(old) = guard.take() {
                old.stop().await;
            }
            match self.spawn(&job, next, Some(to)) {
                Ok(process) => *guard = Some(process),
                Err(err) => tracing::warn!(job = %job.id, "could not restart at a new preset: {err:?}"),
            }
            drop(guard);
            job.notify.notify_waiters();
        }
    }

    /// What the next process of `job` does on the GPU with the jobs running
    /// now (see [`crate::share`]), `hardware` when it encodes on the GPU;
    /// `None` when libplacebo doesn't tone map its video.
    fn gpu_share(&self, job: &Job, hardware: bool) -> Option<Share> {
        let need = job.plan.tonemap_need(&job.media)?;
        let others: Vec<Arc<Job>> = self.renditions().into_iter().filter(|j| j.id != job.id).collect();
        let around: f64 =
            others.iter().filter(|j| j.tone_maps_on_gpu()).filter_map(|j| j.plan.tonemap_need(&j.media)).sum();
        let speed = self.ff.caps.tonemap_speed.as_ref();
        let held = self.lessons.lock().holding(need + around, Instant::now());
        let quality = held.cap(job.plan.shared_quality(speed, &job.media, around)?);
        let gpu = share::Gpu {
            engine_shared: hardware && job.plan.decodes_on_shared_engine(),
            behind: held.gpu_behind() || job.plan.tonemapping_falls_behind(speed, &job.media, around),
        };
        // Other programs' work keeps a job from starting to decode on the
        // CPU, not one decoding there from going on: that one stops when
        // the CPU is found with no time to spare.
        let cores = crate::util::processors();
        let decoding = job.runtime.lock().share.is_some_and(|s| s.cpu_decode);
        let cpu = share::Cpu {
            cores,
            decoding: others.iter().map(|j| j.cpu_decode_cores()).sum(),
            software_encodes: others.iter().filter(|j| j.encodes_in_software()).count() as u32,
            foreign: if decoding { 0.0 } else { self.cpu.foreign().unwrap_or(0.0) },
        };
        let takes = job.plan.source_rate(&job.media).map_or(0.0, |(pixels, fps)| share::decode_cores(pixels, fps));
        let cpu_decode = self.cpu_decode.load(Relaxed) && !held.cpu_busy && share::cpu_decodes(gpu, cpu, takes);
        Some(Share { quality, cpu_decode })
    }

    /// What the jobs tone mapping on the GPU now need together (see
    /// [`Plan::tonemap_need`]).
    fn tonemapping_load(&self) -> f64 {
        self.renditions().iter().filter(|j| j.tone_maps_on_gpu()).filter_map(|j| j.plan.tonemap_need(&j.media)).sum()
    }

    /// Learns from the jobs tone mapping on the GPU whether they keep ahead
    /// of their players, once their processes have run a while, each beside
    /// what the others get done (see [`share::verdict`]).
    fn judge(&self) {
        let (load, cpu_busy, now) = (self.tonemapping_load(), self.cpu.busy(), Instant::now());
        // What each process making on the GPU gets done (its speed, and the
        // work that is), and those that have run long enough to be judged.
        let mut going = Vec::new();
        for job in self.renditions() {
            let Some(share) = job.runtime.lock().share else { continue };
            let Ok(guard) = job.process.try_lock() else { continue };
            let Some(proc) = guard.as_ref().filter(|p| p.running() && !p.paused()) else { continue };
            let Some(speed) = proc.pace.lock().steady_speed() else { continue };
            let next = proc.next.load(Relaxed);
            let judged = next < proc.end && proc.started.elapsed() >= share::JUDGE_AFTER;
            let work = speed * job.plan.tonemap_need(&job.media).unwrap_or(1.0);
            going.push((job.clone(), share, speed, work, judged.then(|| job.lead_secs(next))));
        }
        let work: Vec<(Share, f64)> = going.iter().map(|g| (g.1, g.3)).collect();
        for (i, (job, share, speed, _, lead_secs)) in going.iter().enumerate() {
            let (share, speed) = (*share, *speed);
            let Some(lead_secs) = *lead_secs else { continue };
            let others = share::others(&work, i);
            let verdict = share::verdict(share::Measured { speed, lead_secs, cpu_busy, others });
            if verdict == share::Verdict::Starved {
                tracing::trace!(
                    job = %job.id,
                    "{speed:.2}× real time, held back by its input: another job tone mapping alike gets {others:.1} times as much done"
                );
            }
            if !self.lessons.lock().learn(verdict, share, load, now) {
                continue;
            }
            let learned = match verdict {
                share::Verdict::CpuBound => "the CPU has no time to spare: the GPU decodes".to_owned(),
                _ if share.quality == TonemapQuality::Fast => {
                    "the GPU falls behind even at fast: the CPU decodes where it has the cores".to_owned()
                }
                _ => format!("the GPU falls behind at {}: a cheaper quality", share.quality.as_str()),
            };
            tracing::info!(
                job = %job.id,
                "{learned} ({speed:.2}× real time, {lead_secs:.0} s ahead of the player, the CPU {} busy)",
                cpu_busy.map_or("?".into(), |busy| format!("{:.0} %", busy * 100.0))
            );
        }
    }

    /// Moves the jobs tone mapping on the GPU to what its load calls for
    /// now (see [`crate::share`]): at once to less work for the GPU when
    /// another job starts using it, or for the CPU found with no time to
    /// spare, back to more once a job is left alone, has run a while and is
    /// well ahead of its player. Each move restarts ffmpeg where it was,
    /// between two segments.
    async fn share_gpu(self: &Arc<Self>) {
        self.judge();
        let cpu_busy = self.lessons.lock().holding(self.tonemapping_load(), Instant::now()).cpu_busy;
        for job in self.renditions() {
            let (have, hardware) = {
                let rt = job.runtime.lock();
                (rt.share, rt.process.as_ref().is_some_and(|p| p.hardware))
            };
            let Some(have) = have else { continue };
            let Some(want) = self.gpu_share(&job, hardware).filter(|want| *want != have) else { continue };
            let Ok(mut guard) = job.process.try_lock() else { continue };
            // A paused process is far ahead already: it moves once it goes on.
            let Some(proc) = guard.as_ref().filter(|p| p.running() && !p.paused()) else { continue };
            let next = proc.next.load(Relaxed);
            if next >= proc.end {
                continue;
            }
            if !have.moves_at_once(want, cpu_busy)
                && (proc.started.elapsed() < share::MORE_AFTER || job.lead_secs(next) < share::MORE_LEAD_SECS)
            {
                continue;
            }
            let decoder = |s: Share| if s.cpu_decode { "CPU" } else { "GPU" };
            tracing::info!(
                job = %job.id,
                "the GPU's load changed: tone mapping {} → {}, decoding on the {} → {}",
                have.quality.as_str(),
                want.quality.as_str(),
                decoder(have),
                decoder(want)
            );
            let preset = job.runtime.lock().preset.map(|(i, _)| i);
            if let Some(old) = guard.take() {
                old.stop().await;
            }
            match self.spawn(&job, next, preset) {
                Ok(process) => *guard = Some(process),
                Err(err) => tracing::warn!(job = %job.id, "could not restart for the GPU's load: {err:?}"),
            }
            drop(guard);
            job.notify.notify_waiters();
        }
    }

    fn spawn(self: &Arc<Self>, job: &Arc<Job>, start: u32, preset: Option<usize>) -> ApiResult<Proc> {
        let plan = &job.plan;
        // Stop before the first segment that is already on disk: ffmpeg would
        // truncate and rewrite it while it may be being served.
        let limit = (start + PROCESS_WINDOW).min(plan.segment_count());
        let end = job.first_ready(start + 1, limit).unwrap_or(limit);
        let planned = plan.encoder();
        let stand_in = plan.software_stand_in(&self.ff);
        // Once the hardware encoder failed the job, the software one takes
        // over; once an fMP4 job has its init, the encoder that made it
        // stays.
        let pinned = job.init_encoder.lock().clone();
        let planned_hw = planned.is_some_and(|e| e.hardware);
        let hardware_failed = job.runtime.lock().hardware_failed;
        let encoder = |hw_slot| {
            restart_encoder(pinned.as_ref(), planned_hw, stand_in.is_some(), hardware_failed, preset, hw_slot)
        };
        let want_hw = planned_hw && encoder(true)?.0 == EncoderChoice::Planned;
        let slot = self
            .slots
            .take(want_hw)
            .ok_or_else(|| ApiError::Busy(format!("all {} transcode slots are in use", self.limits.max_jobs)))?;
        let hw = slot.hw;
        let (choice, preset) = encoder(hw).map_err(|err| match err {
            ApiError::Busy(_) if pinned.is_none() => ApiError::Busy(format!(
                "all {} hardware encoder sessions are in use and no software encoder is available",
                self.limits.max_hw_jobs
            )),
            err => err,
        })?;
        let fallback = choice.stands_in().then_some(stand_in).flatten();
        match (choice, fallback) {
            (EncoderChoice::Retreat, Some(software)) => {
                if job.warn("hardware encoder failed, using software") {
                    let failed = planned.map_or("the hardware encoder", |e| e.name.as_str());
                    tracing::warn!(
                        job = %job.id,
                        "{failed} failed before its first segment: encoding with {} instead",
                        software.name
                    );
                    self.hardware_failed();
                }
            }
            (EncoderChoice::NoSession, _) => {
                job.warn("hardware encoder busy, using software");
            }
            _ => {}
        }

        // The software encoder's preset: the planned one, or the automatic
        // one, which starts a step faster when nothing is ready ahead.
        let software = planned.is_some() && (!want_hw || fallback.is_some());
        let auto = job.plan.video.as_ref().is_some_and(|v| v.preset == adaptive::AUTO);
        let chosen = (software && auto).then(|| {
            preset.unwrap_or_else(|| adaptive::start(adaptive::target(self.load_for(job)), job.lead_secs(start)))
        });
        let preset_name = match chosen {
            Some(i) => Some(adaptive::LADDER[i].to_owned()),
            None if software => job.plan.video.as_ref().map(|v| v.preset.clone()),
            None => None,
        };
        let ctx = ArgsContext {
            input: &job.input,
            encoder: fallback,
            relative_split_times: self.ff.segment_times_relative,
            preset: preset_name.as_deref(),
            shader_cache: self.shader_cache.as_deref(),
        };
        // What the GPU's load leaves it, and a bitrate the player asked for
        // since: the same plan with them.
        let share = self.gpu_share(job, hw && fallback.is_none());
        let bitrate = *job.bitrate.lock();
        let adjusted = (share.is_some() || bitrate.is_some()).then(|| {
            let mut plan = share.map_or_else(|| plan.clone(), |s| plan.with_share(s));
            if let (Some(kbps), Some(v)) = (bitrate, plan.video.as_mut()) {
                v.bitrate_kbps = kbps;
            }
            plan
        });
        let args = adjusted.as_ref().unwrap_or(plan).ffmpeg_args(&ctx, start, end);
        tracing::debug!(job = %job.id, "ffmpeg {}", args.iter().map(|a| a.to_string_lossy()).collect::<Vec<_>>().join(" "));
        // It writes its segments where it runs.
        let place = job.choose_place();
        let mut cmd = self.ff.command();
        cmd.args(&args)
            .current_dir(job.folder(place))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let starting = self.background.start();
        let spawned = Instant::now();
        let mut child =
            cmd.spawn().map_err(|err| ApiError::Internal(anyhow::anyhow!("failed to start ffmpeg: {err}")))?;
        Metrics::inc(&self.metrics.processes_started);
        let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
            return Err(ApiError::Internal(anyhow::anyhow!("ffmpeg pipes unavailable")));
        };

        let start_ms = plan.boundaries[start as usize];
        let generation = {
            let mut rt = job.runtime.lock();
            rt.generation += 1;
            if rt.started_any {
                rt.restarts += 1;
            }
            rt.started_any = true;
            rt.state = JobState::Running;
            rt.encoder_override = fallback.cloned();
            rt.preset = chosen.map(|i| (i, Instant::now()));
            rt.share = share;
            rt.process = Some(ProcessStatus {
                pid: child.id(),
                start_segment: start,
                started_at_ms: now_ms(),
                out_time_ms: start_ms,
                hardware: hw,
                preset: preset_name.clone(),
                ..Default::default()
            });
            rt.generation
        };

        let next = Arc::new(AtomicU32::new(start));
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        let pace = Arc::new(Mutex::new(Pace::default()));
        let (pause_tx, mut pause_rx) = watch::channel(false);
        let pause_tx = Arc::new(pause_tx);
        let made = SegmentsMade {
            job: job.clone(),
            place,
            next: next.clone(),
            spawned,
            starting: Some(starting),
            metrics: self.metrics.clone(),
            pause: pause_tx.clone(),
            count: 0,
        };
        // Why its output couldn't be used, when that stopped it.
        let reason = Arc::new(Mutex::new(None));
        let lister = if job.fmp4() {
            let encoder = InitEncoder {
                hardware: hw && fallback.is_none(),
                stand_in: fallback.map(|e| e.name.clone()),
                preset: chosen,
            };
            tokio::spawn(read_fragments(stdout, start, end, generation, encoder, made, reason.clone()))
        } else {
            tokio::spawn(async move {
                read_segment_list(stdout, made).await;
                None
            })
        };
        tokio::spawn(read_progress(job.clone(), stderr, start_ms, tail.clone(), pace.clone()));

        let (exit_tx, exit_rx) = watch::channel(false);
        let (kill_tx, mut kill_rx) = oneshot::channel::<()>();
        let metrics = self.metrics.clone();
        let job_ref = job.clone();
        let next_ref = next.clone();
        tokio::spawn(async move {
            // What was done to it: Windows counts suspensions.
            let mut suspended = false;
            let waited = loop {
                tokio::select! {
                    status = child.wait() => break Some(status),
                    _ = &mut kill_rx => break None,
                    // Between polls of `wait`: the pid is still the child's.
                    Ok(()) = pause_rx.changed() => {
                        let on = *pause_rx.borrow_and_update();
                        if on != suspended && background::pause(&child, on) {
                            suspended = on;
                            tracing::debug!(
                                job = %job_ref.id,
                                "ffmpeg {}",
                                if on { "paused: far enough ahead of the player" } else { "resumed" }
                            );
                        }
                    }
                }
            };
            let (status, killed) = match waited {
                Some(status) => (status, false),
                None => {
                    let _ = child.start_kill();
                    (child.wait().await, true)
                }
            };
            // Every segment it listed is ready (edited, when it needs it)
            // before the process counts as gone: a request coming then
            // must not start another one that writes them again. An fMP4
            // process's last segment ends with its output: made when it
            // ended well, else gone.
            if let Some(pending) = lister.await.ok().flatten() {
                let ended_well = !killed && status.as_ref().is_ok_and(ExitStatus::success) && reason.lock().is_none();
                if !ended_well {
                    pending.discard().await;
                } else if let Err(err) = pending.commit().await {
                    *reason.lock() = Some(format!("a segment couldn't be written: {err}"));
                }
            }
            drop(slot);
            let at = next_ref.load(Relaxed);
            let reason = reason.lock().take();
            if job_ref.on_exit(generation, end, at, status, killed, reason, &tail, &metrics) && place == Place::Memory {
                // The segment it was making, half made, takes no memory.
                job_ref.drop_unmade(at, generation);
            }
            let _ = exit_tx.send(true);
            job_ref.notify.notify_waiters();
        });

        Ok(Proc {
            start,
            end,
            place,
            started: Instant::now(),
            next,
            lookahead: job.segments_in(LOOKAHEAD),
            pace,
            exited: exit_rx,
            kill: Some(kill_tx),
            pause: pause_tx,
        })
    }

    pub async fn remove(&self, id: &str) -> bool {
        let Some(job) = self.jobs.lock().remove(id) else { return false };
        for rendition in job.renditions() {
            rendition.closed.store(true, Relaxed);
            if let Some(p) = rendition.process.lock().await.take() {
                p.stop().await;
            }
            // Wake waiting requests so they fail now instead of at their timeout.
            rendition.notify.notify_waiters();
            if let Some(memory) = &rendition.memory {
                memory.release();
            }
        }
        // The variants' folders with it.
        remove_dir_with_retry(&job.dir).await;
        if let Some(store) = self.memory.get() {
            remove_dir_with_retry(&store.root().join(id)).await;
        }
        tracing::info!(job = %id, "job removed");
        true
    }

    pub fn spawn_reaper(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(Duration::from_secs(5));
            loop {
                tick.tick().await;
                let Some(mgr) = weak.upgrade() else { break };
                mgr.reap().await;
            }
        });
    }

    /// The rendition to move to disk, when memory runs short
    /// ([`memory::short`]): of those whose segments go to memory, the one
    /// with most there.
    fn memory_victim(&self) -> Option<Arc<Job>> {
        if !self.memory.get()?.short() {
            return None;
        }
        self.renditions()
            .into_iter()
            .filter_map(|job| {
                let memory = job.memory.as_ref()?;
                let bytes = memory.lease.lock().is_some().then(|| memory.bytes.load(Relaxed))?;
                Some((bytes, job))
            })
            .max_by_key(|(bytes, _)| *bytes)
            .map(|(_, job)| job)
    }

    /// When memory runs short, moves a rendition to disk (one a look, see
    /// [`JobManager::memory_victim`]): its reservation goes, and a process
    /// writing to memory starts again on disk where it was (a paused one,
    /// far ahead, at the next request). What it made stays where it is,
    /// served from there until pruned.
    async fn relieve_memory(self: &Arc<Self>) {
        let Some(job) = self.memory_victim() else { return };
        let Ok(mut guard) = job.process.try_lock() else { return };
        if let Some(memory) = &job.memory {
            memory.lease.lock().take();
        }
        tracing::info!(job = %job.id, "memory runs short: its segments go to disk from now on");
        let Some(old) = guard.take_if(|p| p.running() && p.place == Place::Memory) else { return };
        let (next, end, paused) = (old.next.load(Relaxed), old.end, old.paused());
        old.stop().await;
        if !paused && next < end {
            let preset = job.runtime.lock().preset.map(|(i, _)| i);
            match self.spawn(&job, next, preset) {
                Ok(process) => *guard = Some(process),
                Err(err) => tracing::warn!(job = %job.id, "could not go on on disk: {err:?}"),
            }
        }
        drop(guard);
        job.notify.notify_waiters();
    }

    async fn reap(self: &Arc<Self>) {
        // Reads /proc on Linux: a few hundred small files on a busy computer.
        let mgr = self.clone();
        let _ = tokio::task::spawn_blocking(move || {
            mgr.cpu.sample();
            mgr.memory_available.store(memory::available().unwrap_or(u64::MAX), Relaxed);
        })
        .await;
        self.adapt().await;
        self.share_gpu().await;
        self.relieve_memory().await;
        let now = now_ms();
        for job in self.list() {
            // A job is asked for whichever rendition its player takes.
            let idle = now - job.last_access_ms.load(Relaxed);
            if idle > JOB_EXPIRE_MS {
                tracing::info!(job = %job.id, "job expired after {}s idle", idle / 1000);
                self.remove(&job.id).await;
                continue;
            }
            for job in job.renditions() {
                let idle = now - job.last_access_ms.load(Relaxed);
                if idle > PROCESS_IDLE_MS
                    && let Ok(mut guard) = job.process.try_lock()
                    && guard.as_ref().is_some_and(Proc::running)
                {
                    tracing::debug!(job = %job.id, "stopping idle ffmpeg process");
                    if let Some(p) = guard.take() {
                        p.stop().await;
                    }
                }
                if let Ok(guard) = job.process.try_lock()
                    && let Some(p) = guard.as_ref().filter(|p| p.running())
                {
                    job.pace_lead(p);
                }
                let pruned = job.clone();
                let _ = tokio::task::spawn_blocking(move || pruned.prune()).await;
            }
        }
    }

    /// FFmpeg as detected, for live channels ([`crate::live`]).
    pub fn ffmpeg(&self) -> Arc<Ffmpeg> {
        self.ff.clone()
    }

    /// Stop every job (concurrently) and refuse new ones.
    pub async fn shutdown(&self) {
        self.closed.store(true, Relaxed);
        let ids: Vec<String> = self.jobs.lock().keys().cloned().collect();
        futures::future::join_all(ids.iter().map(|id| self.remove(id))).await;
    }
}

/// Stops the job's process, if it runs: what it made stays.
fn park(job: Arc<Job>) {
    tokio::spawn(async move {
        if let Some(p) = job.process.lock().await.take() {
            p.stop().await;
            tracing::debug!(job = %job.id, "stopped: its player took another rendition");
        }
    });
}

/// Registers a request as waiting for a segment for as long as it lives.
struct Waiting<'a> {
    job: &'a Job,
    n: u32,
}

impl<'a> Waiting<'a> {
    fn register(job: &'a Job, n: u32) -> Self {
        job.waiting.lock().push(n);
        Self { job, n }
    }
}

impl Drop for Waiting<'_> {
    fn drop(&mut self) {
        let mut waiting = self.job.waiting.lock();
        if let Some(i) = waiting.iter().position(|&m| m == self.n) {
            waiting.swap_remove(i);
        }
        drop(waiting);
        // A request held back for this one (see `ensure_producing`), now
        // gone (served, or aborted by its player), looks again right away
        // rather than at its next timer tick.
        self.job.notify.notify_waiters();
    }
}

impl Job {
    fn new(
        id: String,
        request: JobRequest,
        media: Arc<MediaInfo>,
        plan: Plan,
        input: PathBuf,
        dir: PathBuf,
        start_segment: u32,
    ) -> Job {
        let count = plan.segment_count() as usize;
        Job {
            id,
            request,
            media,
            plan,
            input,
            dir,
            created_at_ms: now_ms(),
            last_access_ms: AtomicI64::new(now_ms()),
            last_requested: AtomicU32::new(start_segment),
            ready: Mutex::new(vec![None; count]),
            ready_count: AtomicU32::new(0),
            waiting: Mutex::default(),
            closed: AtomicBool::new(false),
            notify: Notify::new(),
            process: tokio::sync::Mutex::new(None),
            runtime: Mutex::new(Runtime::default()),
            served_segments: AtomicU64::new(0),
            served_bytes: AtomicU64::new(0),
            variants: Vec::new(),
            prefix: String::new(),
            active: AtomicU32::new(0),
            bitrate: Mutex::new(None),
            memory: None,
            init: Mutex::new(None),
            init_encoder: Mutex::new(None),
        }
    }

    /// The job and its lower renditions, highest first: rendition `k` is
    /// `[k]`.
    pub fn renditions(self: &Arc<Self>) -> Vec<Arc<Job>> {
        std::iter::once(self.clone()).chain(self.variants.iter().cloned()).collect()
    }

    /// Rendition `k`: the job itself (0), or one of its variants.
    fn rendition(self: &Arc<Self>, k: usize) -> ApiResult<Arc<Job>> {
        match k.checked_sub(1) {
            None => Ok(self.clone()),
            Some(i) => self.variants.get(i).cloned().ok_or_else(|| ApiError::NotFound(format!("no rendition {k}"))),
        }
    }

    /// Rendition `k`, the one its player takes from now on: the others stop
    /// making segments (those made stay, for when it comes back). With the
    /// one it took before.
    fn activate(self: &Arc<Self>, k: usize) -> ApiResult<(Arc<Job>, usize)> {
        let rendition = self.rendition(k)?;
        self.touch();
        let before = self.active.swap(k as u32, Relaxed) as usize;
        if before != k {
            tracing::info!(job = %self.id, "the player moved from rendition {before} to {k}");
            for (i, other) in self.renditions().into_iter().enumerate() {
                if i != k {
                    park(other);
                }
            }
        }
        Ok((rendition, before))
    }

    /// Whether its segments are fragmented MP4 (see [`crate::fmp4`]).
    pub fn fmp4(&self) -> bool {
        self.plan.segment_format == SegmentFormat::Fmp4
    }

    fn has_init(&self) -> bool {
        self.init.lock().is_some()
    }

    /// Where an fMP4 job's `init.mp4` is: always on disk, a few kilobytes
    /// that players fetch once and must find again for as long as it plays.
    fn init_path(&self) -> PathBuf {
        self.dir.join("init.mp4")
    }

    /// The initialization segment a process of this fMP4 job wrote
    /// (`generation`): the first one's is kept as `init.mp4`, and every
    /// later one must be the same, byte for byte. Players fetch it once and
    /// decode every segment with it: segments made with another (another
    /// encoder or its settings) wouldn't play. The first one's `encoder`
    /// makes every later process's video ([`restart_encoder`]). Why not,
    /// when it isn't the same.
    fn store_init(&self, bytes: &[u8], generation: u64, encoder: &InitEncoder) -> Result<(), String> {
        let mut init = self.init.lock();
        if let Some(kept) = init.as_deref() {
            if kept == bytes {
                return Ok(());
            }
            tracing::warn!(
                job = %self.id,
                "process {generation} wrote another init segment ({} bytes, {} kept): it stops",
                bytes.len(),
                kept.len()
            );
            return Err("the init segment changed after a restart (encoder settings differ)".into());
        }
        // Small: written under the lock, so no two processes write it at once.
        let part = self.dir.join(format!("init.mp4.p{generation}"));
        let written = std::fs::write(&part, bytes).and_then(|()| std::fs::rename(&part, self.init_path()));
        if let Err(err) = written {
            let _ = std::fs::remove_file(&part);
            return Err(format!("the init segment couldn't be written: {err}"));
        }
        *init = Some(Arc::from(bytes));
        *self.init_encoder.lock() = Some(encoder.clone());
        drop(init);
        self.notify.notify_waiters();
        Ok(())
    }

    pub fn touch(&self) {
        self.last_access_ms.store(now_ms(), Relaxed);
    }

    /// Whether a process of this job is tone mapping with libplacebo now.
    fn tone_maps_on_gpu(&self) -> bool {
        let rt = self.runtime.lock();
        rt.state == JobState::Running && rt.process.is_some() && self.plan.tone_maps_with_libplacebo()
    }

    /// Whether a process of this job is encoding video in software now.
    fn encodes_in_software(&self) -> bool {
        let rt = self.runtime.lock();
        rt.state == JobState::Running
            && rt.process.as_ref().is_some_and(|p| !p.hardware)
            && self.plan.video.as_ref().is_some_and(|v| !v.copy)
    }

    /// The cores a process of this job takes decoding its video on the CPU
    /// now (see [`share::decode_cores`]): none when the GPU decodes it.
    fn cpu_decode_cores(&self) -> f64 {
        let rt = self.runtime.lock();
        let planned =
            self.plan.video.as_ref().is_some_and(|v| v.pipeline == Pipeline::Software && v.hw_decode.is_none());
        let on_cpu = planned || rt.decoded_in_software || rt.share.is_some_and(|s| s.cpu_decode);
        match self.plan.source_rate(&self.media) {
            Some((pixels, fps)) if on_cpu && rt.state == JobState::Running && rt.process.is_some() => {
                share::decode_cores(pixels, fps)
            }
            _ => 0.0,
        }
    }

    /// The wait for segment `n` from process `p`, which will produce it, when
    /// restarting at `n` is quicker, so the request doesn't wait. That is when
    /// the process is held back by the read rate: past its burst, going at
    /// that rate (the encoder isn't what limits it), so slower than a new
    /// process with a burst of its own. And when the wait is longer than a
    /// restart costs this job, and no other request waits for a segment the
    /// process is about to make (players that fetch several at once).
    fn throttled_wait(&self, p: &Proc, n: u32) -> Option<Duration> {
        let bounds = &self.plan.boundaries;
        let next = p.next.load(Relaxed) as usize;
        let n = n as usize;
        if !self.plan.throttle || n < next || n + 1 >= bounds.len() || next + 1 >= bounds.len() {
            return None;
        }
        if bounds[next] - bounds[p.start as usize] < READRATE_BURST_MS {
            return None;
        }
        let speed = p.pace.lock().speed()?;
        if (speed - READRATE).abs() > READRATE * 0.1 {
            return None;
        }
        // Up to the end of `n`, from halfway through the one being made.
        let media_ms = (bounds[n + 1] - bounds[next]) - (bounds[next + 1] - bounds[next]) / 2;
        let wait = Duration::from_secs_f64(media_ms.max(0) as f64 / 1000.0 / speed);
        let restart = {
            let rt = self.runtime.lock();
            rt.start_cost? + rt.stop_cost.unwrap_or(STOP_COST)
        };
        let others = self.is_awaited(|m| m as usize != n && p.will_produce(m));
        (wait > restart && !others).then_some(wait)
    }

    /// Throttling (see [`PAUSE_AHEAD`]): pauses process `p` once it is far
    /// enough ahead of the player, and resumes it when the player comes
    /// closer. Never while a request waits for a segment it is to make.
    fn pace_lead(&self, p: &Proc) {
        let next = p.next.load(Relaxed);
        if !self.plan.throttle || next >= p.end {
            return;
        }
        let lead = self.lead_secs(next);
        let awaited = self.is_awaited(|m| p.will_produce(m));
        let ahead = if p.paused() { RESUME_AHEAD } else { PAUSE_AHEAD };
        let on = !awaited && lead >= ahead.as_secs_f64();
        if p.set_paused(on) {
            tracing::debug!(job = %self.id, "{} {lead:.0} s ahead of the player", if on { "pausing" } else { "resuming" });
            // Paused, it makes nothing: no speed, rather than the last one.
            if on && let Some(process) = self.runtime.lock().process.as_mut() {
                process.speed = None;
            }
        }
    }

    /// Whether a process at segment `next` is far enough ahead of the
    /// player to pause (see [`PAUSE_AHEAD`]), and no request waits for a
    /// segment it has yet to make.
    fn far_ahead(&self, next: u32) -> bool {
        self.plan.throttle && self.lead_secs(next) >= PAUSE_AHEAD.as_secs_f64() && !self.is_awaited(|m| m >= next)
    }

    /// How many of this job's segments last `span` (at least one).
    fn segments_in(&self, span: Duration) -> u32 {
        (span.as_secs_f64() / self.plan.segment_seconds).round().max(1.0) as u32
    }

    /// Seconds of video ready past the segment the player asked for last,
    /// with `next` the first segment not ready (negative: it is waiting).
    fn lead_secs(&self, next: u32) -> f64 {
        let asked = self.last_requested.load(Relaxed);
        (f64::from(next) - f64::from(asked) - 1.0) * self.plan.segment_seconds
    }

    /// Where segment `n` is, when made; else where it would be on disk.
    pub fn segment_path(&self, n: u32) -> PathBuf {
        let place = self.ready.lock().get(n as usize).copied().flatten();
        self.path_in(place.unwrap_or(Place::Disk), n)
    }

    fn path_in(&self, place: Place, n: u32) -> PathBuf {
        self.folder(place).join(format!("{n}.{}", self.plan.segment_format.extension()))
    }

    /// Where process `generation` writes fMP4 segment `n` in `place` until
    /// it is whole: a process stopped long ago, still going, writes its own.
    fn part_path(&self, place: Place, n: u32, generation: u64) -> PathBuf {
        self.folder(place).join(format!("{n}.m4s.p{generation}"))
    }

    /// The folder segments made in `place` are in.
    fn folder(&self, place: Place) -> &Path {
        match (place, &self.memory) {
            (Place::Memory, Some(memory)) => &memory.dir,
            _ => &self.dir,
        }
    }

    /// Where a process starting now makes its segments: in memory when the
    /// job asked for it and has its reservation, or gets one now; else on
    /// disk.
    fn choose_place(&self) -> Place {
        let Some(memory) = &self.memory else { return Place::Disk };
        let mut lease = memory.lease.lock();
        if lease.is_none() {
            *lease = memory.store.lease(memory.want);
            if lease.is_none() {
                tracing::debug!(job = %self.id, "no room in memory: segments go to disk");
            }
        }
        if lease.is_some() { Place::Memory } else { Place::Disk }
    }

    fn is_ready(&self, n: u32) -> bool {
        self.ready.lock().get(n as usize).is_some_and(Option::is_some)
    }

    /// First ready segment in `[from, to)`.
    fn first_ready(&self, from: u32, to: u32) -> Option<u32> {
        let ready = self.ready.lock();
        (from..to).find(|&n| ready.get(n as usize).is_some_and(Option::is_some))
    }

    /// Whether any request is waiting for a segment matching `pred`.
    fn is_awaited(&self, pred: impl Fn(u32) -> bool) -> bool {
        self.waiting.lock().iter().any(|&m| pred(m))
    }

    /// Marks segment `n` ready in `place`, without waking anyone: see
    /// [`segment_written`].
    fn mark_ready(&self, n: u32, place: Place) {
        // What it takes in memory (a tmpfs answers at once).
        let bytes = match (place, &self.memory) {
            (Place::Memory, Some(_)) => std::fs::metadata(self.path_in(place, n)).map_or(0, |m| m.len()),
            _ => 0,
        };
        let mut ready = self.ready.lock();
        if let Some(slot) = ready.get_mut(n as usize)
            && slot.is_none()
        {
            *slot = Some(place);
            self.ready_count.fetch_add(1, Relaxed);
            if let (Some(memory), true) = (&self.memory, bytes > 0) {
                memory.bytes.fetch_add(bytes, Relaxed);
                memory.store.added(bytes);
            }
        }
    }

    /// Adds `message` to the job's warnings: whether it is a new one.
    fn warn(&self, message: &str) -> bool {
        let mut rt = self.runtime.lock();
        let new = !rt.warnings.iter().any(|w| w == message);
        if new {
            rt.warnings.push(message.to_owned());
        }
        new
    }

    /// The GPU couldn't decode the video: the CPU does, which the job says.
    fn decoded_in_software(&self) {
        self.runtime.lock().decoded_in_software = true;
        if self.warn("GPU decoding failed, using the CPU") {
            tracing::warn!(job = %self.id, "hardware decoding failed: decoding on the CPU");
        }
    }

    pub fn record_served(&self, bytes: u64) {
        self.served_segments.fetch_add(1, Relaxed);
        self.served_bytes.fetch_add(bytes, Relaxed);
    }

    /// Process `generation` exited, at segment `next` of those it was to
    /// make up to `end`: whether it is the latest one (none started since).
    /// With a `reason` its output couldn't be used for (an fMP4 process's),
    /// it failed, for that reason rather than what FFmpeg said last.
    #[allow(clippy::too_many_arguments)]
    fn on_exit(
        &self,
        generation: u64,
        end: u32,
        next: u32,
        status: std::io::Result<ExitStatus>,
        killed: bool,
        reason: Option<String>,
        tail: &Mutex<VecDeque<String>>,
        metrics: &Metrics,
    ) -> bool {
        let success = status.as_ref().is_ok_and(ExitStatus::success) && reason.is_none();
        tracing::debug!(job = %self.id, "ffmpeg exited at segment {next} (killed: {killed}, success: {success})");
        let mut rt = self.runtime.lock();
        if rt.generation != generation {
            // Stopped long ago, gone at last: the job is the running one's.
            return false;
        }
        let process = rt.process.take();
        if killed {
            rt.state = JobState::Idle;
        } else if success {
            rt.state = if end >= self.plan.segment_count() { JobState::Finished } else { JobState::Idle };
        } else {
            Metrics::inc(&metrics.process_failures);
            if rt.state != JobState::Failed {
                Metrics::inc(&metrics.jobs_failed);
            }
            let lines: Vec<String> = tail.lock().iter().cloned().collect();
            let message = if let Some(reason) = reason {
                reason
            } else if lines.is_empty() {
                match &status {
                    Ok(s) => format!("ffmpeg exited with {s}"),
                    Err(e) => format!("ffmpeg could not be awaited: {e}"),
                }
            } else {
                lines[lines.len().saturating_sub(4)..].join(" | ")
            };
            tracing::warn!(job = %self.id, "ffmpeg failed at segment {next}: {message}");
            rt.last_error = Some(message);
            rt.state = JobState::Failed;
            *rt.failures.entry(next).or_insert(0) += 1;
            // A hardware encoder failing before it made anything (a driver
            // update, a GPU gone, a session refused) won't do better at the
            // next try.
            if process.is_some_and(|p| p.hardware && next == p.start_segment) {
                rt.hardware_failed = true;
            }
        }
        true
    }

    /// Delete segments far behind the playhead: on disk [`KEEP_BEHIND`],
    /// in memory [`memory::KEEP_BEHIND`], and in memory those far ahead of
    /// it too ([`memory::KEEP_AHEAD`]: a seek back leaves them). Blocking:
    /// run it on the blocking pool. The lock is held per batch so segment
    /// lookups never wait long, and a segment is only marked missing once
    /// its file is gone.
    fn prune(&self) {
        let last = self.last_requested.load(Relaxed) as usize;
        let behind = |keep: Duration| last.saturating_sub(self.segments_in(keep) as usize);
        let disk_from = behind(KEEP_BEHIND);
        let (memory_from, memory_to) =
            (behind(memory::KEEP_BEHIND), last + self.segments_in(memory::KEEP_AHEAD) as usize);
        let stale = |n: usize, place: Place| match place {
            Place::Disk => n < disk_from,
            Place::Memory => n < memory_from || n > memory_to,
        };
        let mut n = 0;
        loop {
            let mut ready = self.ready.lock();
            // Without memory, nothing past the disk's cutoff goes.
            let end = if self.memory.is_some() { ready.len() } else { disk_from.min(ready.len()) };
            if n >= end {
                return;
            }
            let mut deleted = 0;
            while n < end && deleted < PRUNE_BATCH {
                if let Some(place) = ready[n].filter(|&place| stale(n, place)) {
                    let path = self.path_in(place, n as u32);
                    let bytes = (place == Place::Memory).then(|| std::fs::metadata(&path).map_or(0, |m| m.len()));
                    if std::fs::remove_file(&path).is_ok() {
                        ready[n] = None;
                        self.ready_count.fetch_sub(1, Relaxed);
                        if let (Some(bytes), Some(memory)) = (bytes, &self.memory) {
                            memory.removed(bytes);
                        }
                        deleted += 1;
                    }
                }
                n += 1;
            }
        }
    }

    /// Deletes the segments made from `from` on, to be made again (a new
    /// bitrate): how many went. One that can't be deleted (being served)
    /// stays as it is.
    fn forget_from(&self, from: u32) -> usize {
        let mut ready = self.ready.lock();
        let mut gone = 0;
        for n in from as usize..ready.len() {
            let Some(place) = ready[n] else { continue };
            let path = self.path_in(place, n as u32);
            let bytes = (place == Place::Memory).then(|| std::fs::metadata(&path).map_or(0, |m| m.len()));
            if std::fs::remove_file(&path).is_ok() {
                ready[n] = None;
                self.ready_count.fetch_sub(1, Relaxed);
                if let (Some(bytes), Some(memory)) = (bytes, &self.memory) {
                    memory.removed(bytes);
                }
                gone += 1;
            }
        }
        gone
    }

    /// Process `generation` writing to memory stopped at segment `n`, not
    /// made: what it wrote of it goes.
    fn drop_unmade(&self, n: u32, generation: u64) {
        if self.ready.lock().get(n as usize).copied().flatten() != Some(Place::Memory) {
            let _ = std::fs::remove_file(self.path_in(Place::Memory, n));
        }
        if self.fmp4() {
            let _ = std::fs::remove_file(self.part_path(Place::Memory, n, generation));
        }
    }

    /// The multivariant playlist, naming the codecs (see [`crate::hls`]),
    /// and the lower renditions.
    pub fn master_playlist(&self) -> String {
        let renditions: Vec<(&Plan, String)> = std::iter::once((&self.plan, "index.m3u8".to_owned()))
            .chain(self.variants.iter().enumerate().map(|(i, v)| (&v.plan, format!("v{}.m3u8", i + 1))))
            .collect();
        hls::renditions_playlist(&renditions, &self.media)
    }

    /// Does to segment `n`'s video what FFmpeg couldn't (see
    /// [`crate::tsedit`]), before anyone is told it is ready.
    async fn edit_segment(&self, n: u32, place: Place, edits: Edits) {
        let path = self.path_in(place, n);
        let edited = tokio::task::spawn_blocking(move || tsedit::edit_segment(&path, edits)).await;
        match edited {
            Ok(Ok(edited)) => {
                if let Some(err) = edited.first_error.as_deref().filter(|_| edited.rpus_dropped > 0) {
                    tracing::debug!(job = %self.id, "segment {n}: {} RPUs left out: {err}", edited.rpus_dropped);
                    self.warn("some Dolby Vision metadata dropped");
                }
            }
            Ok(Err(err)) => {
                tracing::warn!(job = %self.id, "segment {n} could not be edited: {err}");
                self.warn("a segment's Dolby Vision metadata dropped");
            }
            Err(err) => tracing::warn!(job = %self.id, "segment {n} could not be edited: {err}"),
        }
    }

    pub fn playlist(&self) -> String {
        let bounds = &self.plan.boundaries;
        let longest = bounds.windows(2).map(|w| w[1] - w[0]).max().unwrap_or(0);
        let target = ((longest as f64) / 1000.0).ceil().max(1.0) as i64;
        let mut out = String::with_capacity(bounds.len() * 24 + 256);
        // fMP4 segments (EXT-X-MAP) take version 7, as Apple's authoring
        // specification asks.
        out.push_str(if self.fmp4() { "#EXTM3U\n#EXT-X-VERSION:7\n" } else { "#EXTM3U\n#EXT-X-VERSION:3\n" });
        out.push_str(&format!("#EXT-X-TARGETDURATION:{target}\n#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n"));
        if self.plan.video.is_some() {
            out.push_str("#EXT-X-INDEPENDENT-SEGMENTS\n");
        }
        if self.fmp4() {
            out.push_str(&format!("#EXT-X-MAP:URI=\"{}init.mp4\"\n", self.prefix));
        }
        let extension = self.plan.segment_format.extension();
        for (i, w) in bounds.windows(2).enumerate() {
            out.push_str(&format!("#EXTINF:{:.3},\n{}{i}.{extension}\n", (w[1] - w[0]) as f64 / 1000.0, self.prefix));
        }
        out.push_str("#EXT-X-ENDLIST\n");
        out
    }

    /// How the job goes: that of the rendition its player takes, with what
    /// all of them served.
    pub fn status(&self) -> JobStatus {
        if self.variants.is_empty() {
            return self.own_status();
        }
        let renditions: Vec<&Job> = std::iter::once(self).chain(self.variants.iter().map(Arc::as_ref)).collect();
        let k = (self.active.load(Relaxed) as usize).min(renditions.len() - 1);
        JobStatus {
            id: self.id.clone(),
            last_access_ms: self.last_access_ms.load(Relaxed),
            served_segments: renditions.iter().map(|r| r.served_segments.load(Relaxed)).sum(),
            served_bytes: renditions.iter().map(|r| r.served_bytes.load(Relaxed)).sum(),
            rendition: k as u32,
            variants: self.variants.len() as u32,
            memory_bytes: renditions.iter().filter_map(|r| r.memory.as_ref()).map(|m| m.bytes.load(Relaxed)).sum(),
            ..renditions[k].own_status()
        }
    }

    fn own_status(&self) -> JobStatus {
        let rt = self.runtime.lock();
        let mut video = self.plan.video_plan(&self.media, rt.encoder_override.as_ref());
        if let Some(v) = video.as_mut().filter(|_| rt.decoded_in_software) {
            v.decoder = Some("cpu".into());
            if v.pipeline.as_deref() == Some("gpu") {
                v.pipeline = Some("hybrid".into());
            }
        }
        // What its latest process does on the GPU.
        if let (Some(v), Some(share)) = (video.as_mut(), rt.share) {
            v.tonemap_quality = Some(share.quality);
            if share.cpu_decode {
                v.decoder = Some("cpu".into());
            }
        }
        let mut warnings = self.plan.warnings.clone();
        warnings.extend(rt.warnings.iter().cloned());
        JobStatus {
            id: self.id.clone(),
            label: self.request.label.clone(),
            owner: self.request.owner.clone(),
            input: self.request.input.clone(),
            state: rt.state,
            created_at_ms: self.created_at_ms,
            last_access_ms: self.last_access_ms.load(Relaxed),
            duration_ms: self.plan.duration_ms,
            segment_seconds: self.plan.segment_seconds,
            segment_count: self.plan.segment_count(),
            segments_ready: self.ready_count.load(Relaxed),
            video,
            audio: self.plan.audio_plan(&self.media),
            process: rt.process.clone(),
            restarts: rt.restarts,
            served_segments: self.served_segments.load(Relaxed),
            served_bytes: self.served_bytes.load(Relaxed),
            last_error: rt.last_error.clone(),
            warnings,
            rendition: 0,
            variants: 0,
            in_memory: self.memory.as_ref().is_some_and(|m| m.lease.lock().is_some()),
            memory_bytes: self.memory.as_ref().map_or(0, |m| m.bytes.load(Relaxed)),
        }
    }

    /// A process's progress, with its speed over the last seconds (`recent`)
    /// rather than FFmpeg's own, an average since it started.
    fn update_progress(&self, progress: &HashMap<String, String>, start_ms: i64, recent: Option<f64>) {
        let mut rt = self.runtime.lock();
        let Some(p) = rt.process.as_mut() else { return };
        let num = |k: &str| progress.get(k).and_then(|v| v.trim().parse::<f64>().ok());
        p.frame = num("frame").map(|v| v as u64);
        p.fps = num("fps").filter(|v| *v > 0.0);
        let average =
            progress.get("speed").and_then(|v| v.trim().trim_end_matches('x').parse().ok()).filter(|v: &f64| *v > 0.0);
        p.speed = recent.filter(|v| *v > 0.0).or(average);
        p.bitrate_kbps = progress.get("bitrate").and_then(|v| v.trim().trim_end_matches("kbits/s").parse().ok());
        p.total_size = num("total_size").map(|v| v as u64);
        p.dup_frames = num("dup_frames").map(|v| v as u64);
        p.drop_frames = num("drop_frames").map(|v| v as u64);
        if let Some(us) = num("out_time_us").filter(|v| *v >= 0.0) {
            p.out_time_ms = start_ms + (us / 1000.0) as i64;
        }
    }
}

/// The segments a process has made so far, and what each one made does:
/// for either reader ([`read_segment_list`], [`read_fragments`]).
struct SegmentsMade {
    job: Arc<Job>,
    place: Place,
    next: Arc<AtomicU32>,
    spawned: Instant,
    /// Held until it made [`STARTED_AFTER`] segments.
    starting: Option<Starting>,
    metrics: Arc<Metrics>,
    pause: Arc<watch::Sender<bool>>,
    count: u32,
}

impl SegmentsMade {
    /// Segment `n` is whole in its place: it is ready.
    fn segment(&mut self, n: u32) {
        let job = &self.job;
        tracing::trace!(job = %job.id, "segment {n} ready");
        if self.count == 0 {
            job.runtime.lock().start_cost = Some(self.spawned.elapsed());
        }
        segment_written(job, &self.next, n, self.place);
        Metrics::inc(&self.metrics.segments_produced);
        // Throttling: far enough ahead, it pauses (see `PAUSE_AHEAD`)
        // right away rather than at the reaper's next look.
        if job.far_ahead(n + 1) {
            self.pause.send_if_modified(|paused| !std::mem::replace(paused, true));
        }
        self.count += 1;
        if self.count == STARTED_AFTER {
            // Started: background work goes on.
            drop(self.starting.take());
        }
    }
}

/// Reads the segment list an MPEG-TS process writes, a line for each
/// segment it finished.
async fn read_segment_list(stdout: ChildStdout, mut made: SegmentsMade) {
    let mut lines = BufReader::new(stdout).lines();
    let edits = made.job.plan.segment_edits();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some(n) = parse_list_line(&line) {
            if let Some(edits) = edits {
                made.job.edit_segment(n, made.place, edits).await;
            }
            made.segment(n);
        }
    }
}

/// The fMP4 segment a process was writing when its output ended: whole if
/// the process ended well (`-t`, or the end of the media), else not.
struct Pending {
    n: u32,
    part: PathBuf,
    file: tokio::fs::File,
    made: SegmentsMade,
}

impl Pending {
    async fn commit(mut self) -> std::io::Result<()> {
        let path = self.made.job.path_in(self.made.place, self.n);
        commit_part(&mut self.file, &self.part, &path).await?;
        self.made.segment(self.n);
        Ok(())
    }

    async fn discard(self) {
        drop(self.file);
        let _ = tokio::fs::remove_file(&self.part).await;
    }
}

/// Writes out what `file` holds and names it `path` from `part`.
async fn commit_part(file: &mut tokio::fs::File, part: &Path, path: &Path) -> std::io::Result<()> {
    // A tokio file writes in the background: done once flushed.
    file.flush().await?;
    tokio::fs::rename(part, path).await
}

/// Reads the fragmented MP4 an fMP4 process (`generation`, making segments
/// `[start, end)`) writes on stdout and cuts it into segments: its `ftyp`
/// and `moov` are the job's `init.mp4` ([`Job::store_init`]), and each
/// `moof` and `mdat` goes in the segment its first video sample is shown
/// in, as the segment muxer splits MPEG-TS ([`Plan::split_at`]). Fragments
/// before `start` (a remux's seek landing a keyframe early) and from `end`
/// on are left out: other processes make those. A segment is written to a
/// part file of the process's own and named `{n}.m4s` once the next one
/// begins; the last is given back, made when the process ended well.
/// Output it can't use sets `reason` and ends the reading, which stops
/// FFmpeg (its pipe is closed).
async fn read_fragments(
    stdout: impl AsyncRead + Unpin,
    start: u32,
    end: u32,
    generation: u64,
    encoder: InitEncoder,
    mut made: SegmentsMade,
    reason: Arc<Mutex<Option<String>>>,
) -> Option<Pending> {
    let job = made.job.clone();
    let place = made.place;
    let count = job.plan.segment_count() as usize;
    let mut reader = BoxReader::new(stdout);
    let mut ftyp: Option<Vec<u8>> = None;
    let mut init: Option<Init> = None;
    // The segment being written: its number, part file and file.
    let mut open: Option<(u32, PathBuf, tokio::fs::File)> = None;
    let failed = loop {
        let top = match reader.next().await {
            Ok(Some(top)) => top,
            Ok(None) => break None,
            Err(err) => break Some(format!("its fMP4 output couldn't be read: {err}")),
        };
        match &top.kind {
            b"ftyp" => ftyp = Some(top.bytes),
            b"moov" => {
                let Some(ftyp) = ftyp.take() else { break Some("its fMP4 output has no file type box".into()) };
                let parsed = match Init::parse(&ftyp, &top.bytes) {
                    Ok(parsed) => parsed,
                    Err(why) => break Some(format!("its init segment couldn't be read: {why}")),
                };
                if let Err(why) = job.store_init(&parsed.bytes, generation, &encoder) {
                    break Some(why);
                }
                init = Some(parsed);
            }
            b"moof" => {
                let Some(init) = init.as_ref() else { break Some("its fMP4 output has no movie box".into()) };
                let mdat = match reader.next().await {
                    Ok(Some(mdat)) if mdat.kind == *b"mdat" => mdat,
                    Ok(_) => break Some("a fragment of its fMP4 output has no media data".into()),
                    Err(err) => break Some(format!("its fMP4 output couldn't be read: {err}")),
                };
                let mut fragment = match Fragment::parse(top.bytes, mdat.bytes, init) {
                    Ok(fragment) => fragment,
                    Err(why) => break Some(format!("a fragment of its fMP4 output couldn't be read: {why}")),
                };
                // Audio from before 0 (encoder priming, in a process that
                // started at 0) is left out: players take a negative start
                // badly.
                let mut rewritten = None;
                if fragment.needs_trim(init) {
                    if fragment.trim_before_zero(init) {
                        rewritten = Some(fragment.to_bytes());
                    } else if job.warn("a segment's sound starts before the stream does") {
                        tracing::warn!(job = %job.id, "a fragment's audio before 0 couldn't be left out");
                    }
                }
                // Its segment: where its first video sample is shown, as the
                // segment muxer splits (a segment begins at the first
                // keyframe at or after its split time). Without video (the
                // last flush), it goes with the one before.
                let k = match fragment.presentation_start_ms(init) {
                    Some(t) => job.plan.split_at[..count].partition_point(|&s| s <= t).max(1) as u32 - 1,
                    None => match &open {
                        Some((n, ..)) => *n,
                        None => continue,
                    },
                };
                if k < start || k >= end {
                    tracing::trace!(job = %job.id, "a fragment of segment {k} left out: made by another process");
                    continue;
                }
                if open.as_ref().is_none_or(|(n, ..)| k > *n) {
                    let first = match open.take() {
                        Some((n, part, mut file)) => {
                            if let Err(err) = commit_part(&mut file, &part, &job.path_in(place, n)).await {
                                break Some(format!("segment {n} couldn't be written: {err}"));
                            }
                            made.segment(n);
                            n + 1
                        }
                        None => start,
                    };
                    if k > first {
                        tracing::warn!(job = %job.id, "segments {first} to {} have no keyframe: never made", k - 1);
                    }
                    let part = job.part_path(place, k, generation);
                    match tokio::fs::File::create(&part).await {
                        Ok(file) => open = Some((k, part, file)),
                        Err(err) => break Some(format!("segment {k} couldn't be written: {err}")),
                    }
                } else if open.as_ref().is_some_and(|(n, ..)| k < *n) {
                    tracing::debug!(job = %job.id, "a fragment of segment {k} after a later one: left out");
                    continue;
                }
                let bytes = rewritten.as_deref().unwrap_or(&fragment.bytes);
                if let Some((n, _, file)) = open.as_mut()
                    && let Err(err) = file.write_all(bytes).await
                {
                    break Some(format!("segment {n} couldn't be written: {err}"));
                }
            }
            _ => break Some("its fMP4 output has media data without a fragment".into()),
        }
    };
    match failed {
        None => open.map(|(n, part, file)| Pending { n, part, file, made }),
        Some(why) => {
            tracing::warn!(job = %job.id, "process {generation} stopped: {why}");
            *reason.lock() = Some(why);
            if let Some((_, part, file)) = open {
                drop(file);
                let _ = tokio::fs::remove_file(&part).await;
            }
            None
        }
    }
}

/// FFmpeg finished segment `n`: it is ready, and the process is past it,
/// before anyone wakes. A woken request then never sees the process still
/// owing the segment just finished (and wait on it for nothing).
fn segment_written(job: &Job, next: &AtomicU32, n: u32, place: Place) {
    job.mark_ready(n, place);
    next.store(n + 1, Relaxed);
    job.notify.notify_waiters();
}

async fn read_progress(
    job: Arc<Job>,
    stderr: impl AsyncRead + Unpin,
    start_ms: i64,
    tail: Arc<Mutex<VecDeque<String>>>,
    pace: Arc<Mutex<Pace>>,
) {
    const PROGRESS_KEYS: [&str; 12] = [
        "frame",
        "fps",
        "bitrate",
        "total_size",
        "out_time_us",
        "out_time_ms",
        "out_time",
        "dup_frames",
        "drop_frames",
        "speed",
        "progress",
        "stream_0_0_q",
    ];
    let mut lines = BufReader::new(stderr).lines();
    let mut block: HashMap<String, String> = HashMap::new();
    while let Ok(Some(line)) = lines.next_line().await {
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            if PROGRESS_KEYS.contains(&key) || key.starts_with("stream_") {
                if key == "progress" {
                    let out_us = block.get("out_time_us").and_then(|v| v.trim().parse::<f64>().ok());
                    let mut recent = None;
                    if let Some(us) = out_us.filter(|us| *us >= 0.0) {
                        let out_ms = start_ms + (us / 1000.0) as i64;
                        let mut pace = pace.lock();
                        pace.record(Instant::now(), out_ms);
                        recent = pace.speed().map(|speed| {
                            // Held to the read rate past its burst: real time, as meant
                            // (players take less than 1× for a transcoder falling behind).
                            let paced = job.plan.throttle
                                && out_ms - start_ms >= READRATE_BURST_MS
                                && (speed - READRATE).abs() <= READRATE * 0.1;
                            if paced { READRATE } else { speed }
                        });
                    }
                    job.update_progress(&block, start_ms, recent);
                    block.clear();
                } else {
                    block.insert(key.to_owned(), value.to_owned());
                }
                continue;
            }
        }
        let line = line.trim();
        if crate::ffmpeg::hardware_decoder_failed(line) {
            job.decoded_in_software();
        }
        if !line.is_empty() {
            let mut tail = tail.lock();
            tail.push_back(line.to_owned());
            if tail.len() > 20 {
                tail.pop_front();
            }
        }
    }
}

/// The segment a line of FFmpeg's segment list names:
/// `"12.ts,48.000000,52.000000"` → `Some(12)`. Most builds list the file
/// (some with its path); SynoCommunity's for Synology DSM lists the number
/// alone, zero-padded (`"012,48.000000,52.000000"`).
pub(crate) fn parse_list_line(line: &str) -> Option<u32> {
    let name = line.split(',').next()?.trim();
    let name = name.rsplit(['/', '\\']).next()?;
    let number = name.strip_suffix(".ts").unwrap_or(name);
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    number.parse().ok()
}

fn describe(created: &JobCreated) -> String {
    let video = created.video.as_ref().map(|v| {
        format!(
            "video {} {}→{} {}x{}→{}x{}{}{}{}{}",
            if v.mode == VideoMode::Copy { "copy" } else { "transcode" },
            v.codec_in,
            v.codec_out,
            v.width_in.unwrap_or(0),
            v.height_in.unwrap_or(0),
            v.width_out.unwrap_or(0),
            v.height_out.unwrap_or(0),
            v.encoder.as_deref().map(|e| format!(" via {e}")).unwrap_or_default(),
            match (v.dynamic_range_out.filter(|r| r.is_hdr()), v.tonemap) {
                (Some(out), Some(t)) => format!(" {} to {}", t.as_str(), out.label()),
                (Some(out), None) => format!(" keeping {}", out.label()),
                (None, t) => t
                    .map(|t| {
                        let quality = v.tonemap_quality.map(|q| format!(" ({})", q.as_str())).unwrap_or_default();
                        format!(" tonemap {}{quality}", t.as_str())
                    })
                    .unwrap_or_default(),
            },
            v.dolby_vision.map(|m| format!(" dolby vision {m:?}").to_lowercase()).unwrap_or_default(),
            if v.hdr10plus_stripped { " without hdr10+" } else { "" },
        )
    });
    let audio = created
        .audio
        .as_ref()
        .map(|a| format!("audio {}→{} {}ch", a.codec_in, a.codec_out, a.channels_out.unwrap_or(0)));
    let format = if created.segment_format == SegmentFormat::Fmp4 { " fMP4" } else { "" };
    [video, audio, Some(format!("{}{format} segments", created.segment_count))]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ")
}

/// The folder of a job being made, removed with what's in it (a burn-in
/// track half written) when the job isn't made after all: an error, or its
/// request gone (the server gave up waiting, the player left).
struct Unmade(Vec<PathBuf>);

impl Unmade {
    /// The job is made: its folders stay.
    fn made(mut self) {
        self.0.clear();
    }
}

impl Drop for Unmade {
    fn drop(&mut self) {
        for dir in std::mem::take(&mut self.0) {
            // FFmpeg, killed with the request, may hold a file there a moment longer.
            match tokio::runtime::Handle::try_current() {
                Ok(runtime) => {
                    runtime.spawn(async move { remove_dir_with_retry(&dir).await });
                }
                Err(_) => {
                    let _ = std::fs::remove_dir_all(&dir);
                }
            }
        }
    }
}

async fn remove_dir_with_retry(dir: &Path) {
    for attempt in 0..5 {
        match tokio::fs::remove_dir_all(dir).await {
            Ok(()) => return,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
            Err(e) => {
                if attempt == 4 {
                    tracing::warn!("could not remove {}: {e}", dir.display());
                }
                tokio::time::sleep(Duration::from_millis(200 * (attempt + 1))).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn folders_of_jobs_never_made_go() {
        let dir = tempfile::tempdir().unwrap();
        let (made, unmade) = (dir.path().join("made"), dir.path().join("unmade"));
        for job in [&made, &unmade] {
            std::fs::create_dir(job).unwrap();
            std::fs::write(job.join("burn.ass"), "[Script Info]\n").unwrap();
        }
        Unmade(vec![made.clone()]).made();
        // Dropped midway (an error, or the request gone).
        drop(Unmade(vec![unmade.clone()]));
        for _ in 0..50 {
            if !unmade.exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!unmade.exists() && made.join("burn.ass").exists());
    }

    #[test]
    fn parses_segment_list_lines() {
        assert_eq!(parse_list_line("12.ts,48.000000,52.000000"), Some(12));
        assert_eq!(parse_list_line("C:\\tmp\\job\\3.ts,1,2"), Some(3));
        // SynoCommunity's FFmpeg for DSM: the number alone, zero-padded.
        assert_eq!(parse_list_line("000,0.167000,2.919000"), Some(0));
        assert_eq!(parse_list_line("1284,2862.541000,2864.043000"), Some(1284));
        assert_eq!(parse_list_line("index.m3u8"), None);
        assert_eq!(parse_list_line("frame=12,1,2"), None);
        assert_eq!(parse_list_line(""), None);
    }

    #[test]
    fn waits_only_for_segments_ahead_of_the_process() {
        let (_exit_tx, exited) = watch::channel(false);
        let p = Proc {
            start: 0,
            end: 20,
            place: Place::Disk,
            started: Instant::now(),
            next: Arc::new(AtomicU32::new(10)),
            lookahead: 3,
            pace: Arc::default(),
            exited,
            kill: None,
            pause: Arc::new(watch::channel(false).0),
        };
        assert!(!p.will_produce(9), "already passed (pruned or skipped): must restart");
        assert!(p.will_produce(10));
        assert!(p.will_produce(13));
        assert!(!p.will_produce(14), "too far ahead: restart there");
        p.next.store(18, Relaxed);
        assert!(p.will_produce(19));
        assert!(!p.will_produce(20), "outside the process window");
    }

    #[test]
    fn pace_is_measured_over_the_last_seconds() {
        let t = Instant::now();
        let at = |ms: u64| t + Duration::from_millis(ms);
        let mut pace = Pace::default();
        pace.record(at(0), 0);
        pace.record(at(1_000), 30_000);
        assert_eq!(pace.speed(), None, "too short to tell");
        // After a burst of 30 s in a second, twice real time.
        for s in 2..=10 {
            pace.record(at(s * 1_000), 30_000 + (s as i64 - 1) * 2_000);
        }
        let speed = pace.speed().unwrap();
        assert!((speed - 2.0).abs() < 0.001, "the burst is forgotten: {speed}");
        assert_eq!(pace.steady_speed(), None, "too short to tell steadily");
        for s in 11..=30 {
            pace.record(at(s * 1_000), 30_000 + (s as i64 - 1) * 2_000);
        }
        let steady = pace.steady_speed().unwrap();
        assert!((steady - 2.0).abs() < 0.001, "over the last 20 s: {steady}");
    }

    #[test]
    fn a_throttled_process_pauses_far_ahead_of_the_player() {
        let dir = tempfile::tempdir().unwrap();
        let mut job = job(dir.path(), (0..=500).map(|i| i * 2_000).collect());
        job.plan.segment_seconds = 2.0;
        let (_exit_tx, exited) = watch::channel(false);
        let p = Proc {
            start: 0,
            end: 500,
            place: Place::Disk,
            started: Instant::now(),
            next: Arc::new(AtomicU32::new(10)),
            lookahead: 6,
            pace: Arc::default(),
            exited,
            kill: None,
            pause: Arc::new(watch::channel(false).0),
        };
        let at = |next: u32, asked: u32| {
            p.next.store(next, Relaxed);
            job.last_requested.store(asked, Relaxed);
        };

        // Without throttling it never pauses.
        at(100, 0);
        job.pace_lead(&p);
        assert!(!p.paused());
        assert!(!job.far_ahead(100));

        job.plan.throttle = true;
        job.runtime.lock().process = Some(ProcessStatus { speed: Some(1.0), ..Default::default() });
        at(40, 0);
        job.pace_lead(&p);
        assert!(!p.paused(), "78 s ahead: on it goes");
        at(47, 0);
        assert!(job.far_ahead(47));
        job.pace_lead(&p);
        assert!(p.paused(), "92 s ahead: paused");
        assert_eq!(job.status().process.unwrap().speed, None, "paused, it has no speed");

        // It stays paused until the player is within a minute of it.
        at(47, 5);
        job.pace_lead(&p);
        assert!(p.paused(), "82 s ahead");
        at(47, 17);
        job.pace_lead(&p);
        assert!(!p.paused(), "58 s ahead: it goes on");
        p.pace.lock().record(Instant::now(), 0);
        assert!(p.set_paused(true));
        assert!(!p.set_paused(true), "once");
        assert!(p.set_paused(false));
        assert!(p.pace.lock().samples.is_empty(), "its speed is measured anew");

        // A request waiting for a segment it is to make keeps it going.
        at(60, 0);
        let _waiting = Waiting::register(&job, 61);
        assert!(!job.far_ahead(60));
        job.pace_lead(&p);
        assert!(!p.paused());
    }

    #[test]
    fn seeks_just_past_a_throttled_process_restart_it() {
        let dir = tempfile::tempdir().unwrap();
        let mut job = job(dir.path(), (0..=100).map(|i| i * 2_000).collect());
        job.plan.segment_seconds = 2.0;
        job.plan.throttle = true;
        let (_exit_tx, exited) = watch::channel(false);
        let p = Proc {
            start: 0,
            end: 100,
            place: Place::Disk,
            started: Instant::now(),
            next: Arc::new(AtomicU32::new(70)),
            lookahead: 6,
            pace: Arc::default(),
            exited,
            kill: None,
            pause: Arc::new(watch::channel(false).0),
        };
        let going = |speed: f64| {
            let at = Instant::now();
            let mut pace = p.pace.lock();
            *pace = Pace::default();
            pace.record(at, 140_000);
            pace.record(at + Duration::from_secs(3), 140_000 + (3_000.0 * speed) as i64);
        };
        going(1.0);
        assert_eq!(job.throttled_wait(&p, 72), None, "no restart measured yet");
        job.runtime.lock().start_cost = Some(Duration::from_millis(1_100));
        job.runtime.lock().stop_cost = Some(Duration::from_millis(100));

        // Two segments past the one being made: 2.5 segments of 2 s at real
        // time, against 1.2 s to restart.
        let wait = job.throttled_wait(&p, 72).unwrap();
        assert!((wait.as_secs_f64() - 5.0).abs() < 0.001, "{wait:?}");
        assert_eq!(job.throttled_wait(&p, 70), None, "half a segment: no quicker to restart");
        // (a) Still in its burst (100 s made of 120).
        p.next.store(50, Relaxed);
        assert_eq!(job.throttled_wait(&p, 52), None);
        p.next.store(70, Relaxed);
        // (b) Slower than the read rate: the encoder is what limits it.
        going(0.7);
        assert_eq!(job.throttled_wait(&p, 72), None);
        going(1.05);
        assert!(job.throttled_wait(&p, 72).is_some());
        // (c) A restart costing more than the wait.
        job.runtime.lock().start_cost = Some(Duration::from_secs(6));
        assert_eq!(job.throttled_wait(&p, 72), None);
        job.runtime.lock().start_cost = Some(Duration::from_millis(1_100));
        // (d) Another request waits for a segment the process is about to make.
        let other = Waiting::register(&job, 71);
        assert_eq!(job.throttled_wait(&p, 72), None);
        drop(other);
        let this = Waiting::register(&job, 72);
        assert!(job.throttled_wait(&p, 72).is_some(), "the request itself doesn't count");
        drop(this);
        // Not throttled at all.
        job.plan.throttle = false;
        assert_eq!(job.throttled_wait(&p, 72), None);
    }

    #[test]
    fn slots_go_back_however_their_holder_ends() {
        let background = Background::new();
        let slots = Arc::new(Slots::new(&Limits { max_jobs: 2, max_hw_jobs: 1 }, background.clone()));
        assert!(!background.held());
        let (hardware, software) = (slots.take(true).unwrap(), slots.take(true).unwrap());
        assert!(background.held(), "background work waits for transcodes");
        assert_eq!((hardware.hw, software.hw), (true, false));
        assert!(slots.take(false).is_none());
        drop(hardware);
        assert_eq!(slots.counts(), (1, 0));
        drop(software);
        assert_eq!(slots.counts(), (0, 0));
    }

    #[test]
    fn slots_fall_back_to_software() {
        let slots = Slots::new(&Limits { max_jobs: 2, max_hw_jobs: 1 }, Background::new());
        assert_eq!(slots.acquire(true), Some(true));
        assert_eq!(slots.acquire(true), Some(false));
        assert_eq!(slots.acquire(false), None);
        slots.release(true);
        assert_eq!(slots.counts(), (1, 0));
        assert_eq!(slots.acquire(true), Some(true));
        // Releasing more than was taken never underflows.
        let slots = Slots::new(&Limits { max_jobs: 0, max_hw_jobs: 0 }, Background::new());
        slots.release(true);
        assert_eq!(slots.counts(), (0, 0));
        assert_eq!(slots.acquire(true), Some(false), "at least one job, in software");
    }

    /// A job with the given segment boundaries whose segments live in `dir`.
    fn job(dir: &Path, boundaries: Vec<i64>) -> Job {
        let plan = Plan {
            duration_ms: *boundaries.last().unwrap(),
            split_at: boundaries.clone(),
            seek_at: boundaries.clone(),
            boundaries,
            segment_seconds: 4.0,
            video: None,
            audio: None,
            burn: None,
            throttle: false,
            catchup: false,
            warnings: vec![],
            segment_format: Default::default(),
        };
        let job = Job::new(
            "test".into(),
            JobRequest::default(),
            Arc::new(MediaInfo::default()),
            plan,
            dir.join("input.mkv"),
            dir.to_path_buf(),
            0,
        );
        Job { created_at_ms: 0, last_access_ms: AtomicI64::new(0), ..job }
    }

    /// A manager whose FFmpeg "works" but whose binaries are missing.
    fn manager(dir: &Path) -> Arc<JobManager> {
        let ff = Ffmpeg {
            ffmpeg: dir.join("no-ffmpeg"),
            ffprobe: dir.join("no-ffprobe"),
            caps: pley_transcoder_api::Capabilities { ffmpeg_available: true, ..Default::default() },
            segment_times_relative: true,
        };
        manager_with(dir, ff)
    }

    fn manager_with(dir: &Path, ff: Ffmpeg) -> Arc<JobManager> {
        let ff = Arc::new(ff);
        let metrics = Arc::new(Metrics::default());
        let prober = Arc::new(Prober::new(ff.clone(), metrics.clone(), 1));
        let background = Background::new();
        let keyframes = KeyframeStore::new(ff.clone(), metrics.clone(), background.clone(), dir.join("keyframes"));
        let extractor = Arc::new(Extractor::new(ff.clone(), prober.clone(), metrics.clone(), dir.join("subtitles"), 1));
        let limits = Limits { max_jobs: 2, max_hw_jobs: 1 };
        let capabilities = Some(dir.join("capabilities.json"));
        JobManager::new(
            ff,
            prober,
            keyframes,
            extractor,
            metrics,
            dir.join("jobs"),
            None,
            capabilities,
            limits,
            background,
        )
    }

    #[tokio::test]
    async fn implausible_durations_from_the_server_are_probed_again() {
        use pley_transcoder_api::{AudioRequest, MediaStream, StreamKind};
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("song.flac");
        std::fs::write(&input, b"not really audio").unwrap();
        let jobs = manager(dir.path());
        let request = |duration_ms: i64| JobRequest {
            input: input.to_string_lossy().into_owned(),
            media: Some(MediaInfo {
                duration_ms: Some(duration_ms),
                streams: vec![MediaStream {
                    index: 0,
                    kind: StreamKind::Audio,
                    codec: "flac".into(),
                    sample_rate: Some(44_100),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            audio: Some(AudioRequest::default()),
            ..Default::default()
        };
        // A believable duration is used as it is (nothing is produced without
        // ffmpeg, but the job exists).
        let created = jobs.create(request(60_000)).await.unwrap();
        assert_eq!(created.segment_count, 15);
        // 1e12 s would be 250 billion segments: never trusted, the file is
        // probed again (and ffprobe is missing too).
        let err = jobs.create(request(1_000_000_000_000_000)).await.unwrap_err();
        assert!(matches!(&err, ApiError::Failed(m) if m.contains("probe failed")), "{err:?}");
        assert_eq!(jobs.list().len(), 1);
    }

    /// A job's start holds even what waits only for starts, while it is
    /// created and until its FFmpeg has made its first segments; background
    /// work waits on while the FFmpeg runs. Needs FFmpeg on the PATH (skipped
    /// otherwise).
    #[tokio::test(flavor = "multi_thread")]
    async fn starting_jobs_hold_background_work() {
        use pley_transcoder_api::{VideoMode, VideoRequest};
        let ok =
            |tool: &str| std::process::Command::new(tool).arg("-version").output().is_ok_and(|o| o.status.success());
        if !(ok("ffmpeg") && ok("ffprobe")) {
            eprintln!("ffmpeg not found: skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("clip.mkv");
        let made = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
            .arg("testsrc2=size=320x180:rate=25:duration=30")
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-g", "25"])
            .arg(&input)
            .status()
            .unwrap();
        assert!(made.success());
        let ff = Ffmpeg {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            caps: pley_transcoder_api::Capabilities {
                ffmpeg_available: true,
                ffprobe_available: true,
                ..Default::default()
            },
            segment_times_relative: true,
        };
        let jobs = manager_with(dir.path(), ff);
        let background = jobs.background.clone();
        let seen = Arc::new(AtomicBool::new(false));
        let watch = tokio::spawn({
            let (background, seen) = (background.clone(), seen.clone());
            async move {
                loop {
                    if background.starting() {
                        seen.store(true, Relaxed);
                    }
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }
        });
        let created = jobs
            .create(JobRequest {
                input: input.to_string_lossy().into_owned(),
                video: Some(VideoRequest { mode: VideoMode::Copy, ..Default::default() }),
                segment_seconds: Some(2.0),
                ..Default::default()
            })
            .await
            .unwrap();
        let job = jobs.get(&created.id).unwrap();
        jobs.segment(&job, STARTED_AFTER - 1).await.unwrap();
        watch.abort();
        assert!(seen.load(Relaxed), "held while the job started");
        let deadline = Instant::now() + Duration::from_secs(5);
        while background.starting() && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(!background.starting(), "started once its first segments are out");
        jobs.remove(&created.id).await;
        tokio::time::sleep(background::AFTER_TRANSCODE + Duration::from_millis(100)).await;
        assert!(!background.held(), "free once nothing transcodes");
    }

    /// A hardware encoder that fails before its first segment hands the job
    /// to the software one. Needs FFmpeg on the PATH (skipped otherwise).
    #[tokio::test(flavor = "multi_thread")]
    async fn a_hardware_encoder_that_fails_hands_the_job_to_software() {
        use pley_transcoder_api::{EncoderInfo, HwAccel, VideoRequest};
        let ok =
            |tool: &str| std::process::Command::new(tool).arg("-version").output().is_ok_and(|o| o.status.success());
        if !(ok("ffmpeg") && ok("ffprobe")) {
            eprintln!("ffmpeg not found: skipped");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("clip.mkv");
        let made = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
            .arg("testsrc2=size=320x180:rate=25:duration=10")
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-g", "25"])
            .arg(&input)
            .status()
            .unwrap();
        assert!(made.success());
        let encoder = |name: &str, hardware: bool, accel: HwAccel| EncoderInfo {
            name: name.into(),
            codec: VideoCodec::H264,
            hardware,
            accel,
            main10: false,
        };
        let ff = Ffmpeg {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            caps: pley_transcoder_api::Capabilities {
                ffmpeg_available: true,
                ffprobe_available: true,
                // One FFmpeg doesn't know: it fails at once, as a broken
                // driver's would.
                encoders: vec![
                    encoder("h264_pley_broken", true, HwAccel::Amf),
                    encoder("libx264", false, HwAccel::None),
                ],
                ..Default::default()
            },
            segment_times_relative: true,
        };
        let jobs = manager_with(dir.path(), ff);
        let created = jobs
            .create(JobRequest {
                input: input.to_string_lossy().into_owned(),
                video: Some(VideoRequest::default()),
                segment_seconds: Some(2.0),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(created.video.as_ref().and_then(|v| v.encoder.as_deref()), Some("h264_pley_broken"));
        let job = jobs.get(&created.id).unwrap();
        jobs.segment(&job, 0).await.expect("made in software");
        let status = job.status();
        assert_eq!(status.video.as_ref().and_then(|v| v.encoder.as_deref()), Some("libx264"));
        assert!(status.warnings.iter().any(|w| w.contains("hardware encoder failed")), "{:?}", status.warnings);
        assert_eq!(status.process.map(|p| p.hardware), Some(false));
        assert_eq!(jobs.hardware_failures.load(Relaxed), 1);
        jobs.remove(&created.id).await;
    }

    #[test]
    fn job_files_are_named_by_rendition() {
        for (name, file) in [
            ("master.m3u8", Some(JobFile::Master)),
            ("index.m3u8", Some(JobFile::Playlist(0))),
            ("v2.m3u8", Some(JobFile::Playlist(2))),
            ("12.ts", Some(JobFile::Segment(0, 12, SegmentFormat::Ts))),
            ("v1-12.ts", Some(JobFile::Segment(1, 12, SegmentFormat::Ts))),
            ("12.m4s", Some(JobFile::Segment(0, 12, SegmentFormat::Fmp4))),
            ("v1-12.m4s", Some(JobFile::Segment(1, 12, SegmentFormat::Fmp4))),
            ("init.mp4", Some(JobFile::Init(0))),
            ("v2-init.mp4", Some(JobFile::Init(2))),
            ("v0.m3u8", None),
            ("v0-3.ts", None),
            ("v0-3.m4s", None),
            ("v0-init.mp4", None),
            ("-init.mp4", None),
            ("v-init.mp4", None),
            ("init.ts", None),
            ("init.m4s", None),
            ("12.m4s.p3", None),
            ("12.mp4", None),
            ("v1.ts", None),
            ("v1.m4s", None),
            ("x.m3u8", None),
            ("12.jpg", None),
        ] {
            assert_eq!(JobFile::parse(name), file, "{name}");
        }
    }

    /// A clip FFmpeg makes, and a manager whose only encoder is libx264
    /// (`None` without FFmpeg).
    fn with_ffmpeg(dir: &Path, size: &str) -> Option<(PathBuf, Arc<JobManager>)> {
        use pley_transcoder_api::{EncoderInfo, HwAccel};
        let ok =
            |tool: &str| std::process::Command::new(tool).arg("-version").output().is_ok_and(|o| o.status.success());
        if !(ok("ffmpeg") && ok("ffprobe")) {
            eprintln!("ffmpeg not found: skipped");
            return None;
        }
        let input = dir.join("clip.mkv");
        let made = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!("testsrc2=size={size}:rate=25:duration=10"))
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-g", "25"])
            .arg(&input)
            .status()
            .unwrap();
        assert!(made.success());
        let x264 = EncoderInfo {
            name: "libx264".into(),
            codec: VideoCodec::H264,
            hardware: false,
            accel: HwAccel::None,
            main10: false,
        };
        let caps = pley_transcoder_api::Capabilities {
            ffmpeg_available: true,
            ffprobe_available: true,
            encoders: vec![x264],
            ..Default::default()
        };
        let ff = Ffmpeg { ffmpeg: "ffmpeg".into(), ffprobe: "ffprobe".into(), caps, segment_times_relative: true };
        Some((input, manager_with(dir, ff)))
    }

    /// When the first video packet of a segment is shown, in seconds.
    fn first_video_pts(segment: &Path) -> f64 {
        let out = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-select_streams", "v:0", "-read_intervals", "%+#1"])
            .args(["-show_entries", "packet=pts_time", "-of", "csv=p=0"])
            .arg(segment)
            .output()
            .unwrap();
        // "5.400000," (with the packet's side data after it).
        let text = String::from_utf8_lossy(&out.stdout);
        text.lines().next().and_then(|l| l.split(',').next()).and_then(|t| t.trim().parse().ok()).unwrap()
    }

    #[tokio::test]
    async fn a_new_bitrate_remakes_what_the_player_has_not_fetched() {
        use pley_transcoder_api::VideoRequest;
        let dir = tempfile::tempdir().unwrap();
        let Some((_, jobs)) = with_ffmpeg(dir.path(), "320x240") else { return };
        // Noise, so that the bitrate asked for is what limits the encoder.
        let input = dir.path().join("noisy.mkv");
        let made = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
            .arg("testsrc2=size=1280x720:rate=25:duration=10,noise=alls=60:allf=t+u")
            .args(["-c:v", "libx264", "-preset", "ultrafast", "-g", "25"])
            .arg(&input)
            .status()
            .unwrap();
        assert!(made.success());
        let created = jobs
            .create(JobRequest {
                input: input.to_string_lossy().into_owned(),
                video: Some(VideoRequest { max_bitrate_kbps: Some(6_000), ..Default::default() }),
                segment_seconds: Some(2.0),
                ..Default::default()
            })
            .await
            .unwrap();
        let job = jobs.get(&created.id).unwrap();
        let size = |path: &Path| std::fs::metadata(path).unwrap().len();
        let before: Vec<(u64, f64)> = {
            let mut made = Vec::new();
            for n in 0..5 {
                let path = jobs.segment(&job, n).await.unwrap();
                made.push((size(&path), first_video_pts(&path)));
            }
            made
        };
        // The player has asked for segment 1: from segment 3 on, the stream goes on at 1.5 Mbps.
        job.last_requested.store(1, Relaxed);
        assert_eq!(jobs.set_bitrate(&job, 1_500).await.unwrap(), 1_500);
        // (A process still running restarts at once: segment 3 may be made again already.)
        assert!(job.is_ready(1) && job.is_ready(2), "what the player may be fetching stays");
        assert_eq!(size(&job.segment_path(2)), before[2].0, "and is the same");
        for (n, &(old_size, old_pts)) in before.iter().enumerate().skip(3) {
            let path = jobs.segment(&job, n as u32).await.unwrap();
            assert!(size(&path) * 2 < old_size, "segment {n}: {} bytes, {old_size} before", size(&path));
            // Laid out as before: the player plays on from the same playlist.
            assert!((first_video_pts(&path) - old_pts).abs() < 0.001, "segment {n} starts where it did");
        }
        // Asked again, nothing changes; never above the job's own, nor below 1 Mbps.
        assert_eq!(jobs.set_bitrate(&job, 1_500).await.unwrap(), 1_500);
        let own = job.plan.video.as_ref().unwrap().bitrate_kbps;
        assert_eq!(jobs.set_bitrate(&job, 50_000).await.unwrap(), own);
        assert_eq!(jobs.set_bitrate(&job, 10).await.unwrap(), MIN_BITRATE_KBPS);
        jobs.remove(&created.id).await;
    }

    #[tokio::test]
    async fn variants_are_laid_out_alike_and_made_one_at_a_time() {
        use pley_transcoder_api::{VariantRequest, VideoRequest};
        let dir = tempfile::tempdir().unwrap();
        let Some((input, jobs)) = with_ffmpeg(dir.path(), "1280x720") else { return };
        let variant =
            |w, h, kbps| VariantRequest { max_width: Some(w), max_height: Some(h), max_bitrate_kbps: Some(kbps) };
        let created = jobs
            .create(JobRequest {
                input: input.to_string_lossy().into_owned(),
                video: Some(VideoRequest::default()),
                segment_seconds: Some(2.0),
                variants: vec![
                    variant(854, 480, 1_200),
                    // No smaller than the one above: left out.
                    variant(1280, 720, 1_000),
                    variant(640, 360, 600),
                ],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(created.variants, 2);
        let job = jobs.get(&created.id).unwrap();

        // Listed highest first, each with its playlist and its segments' names.
        let master = job.master_playlist();
        let listed: Vec<(&str, &str)> = master
            .lines()
            .zip(master.lines().skip(1))
            .filter(|(info, _)| info.starts_with("#EXT-X-STREAM-INF"))
            .map(|(info, uri)| (info.split("RESOLUTION=").nth(1).unwrap().split(',').next().unwrap(), uri))
            .collect();
        assert_eq!(listed, [("1280x720", "index.m3u8"), ("854x480", "v1.m3u8"), ("640x360", "v2.m3u8")], "{master}");
        let renditions = job.renditions();
        assert!(renditions[1].playlist().contains("\nv1-0.ts\n"));
        assert_eq!(renditions[1].playlist().replace("v1-", ""), job.playlist(), "the same segments");

        // The player starts on the job's own, then moves down a rendition at
        // segment 2: the first stops, and the job tells of the second.
        let (_, top) = jobs.rendition_segment(&job, 0, 1).await.unwrap();
        let (_, lower) = jobs.rendition_segment(&job, 1, 2).await.unwrap();
        assert!(lower.starts_with(dir.path().join("jobs").join(&created.id).join("v1")), "{}", lower.display());
        let status = job.status();
        assert_eq!((status.id.as_str(), status.rendition, status.variants), (created.id.as_str(), 1, 2));
        assert_eq!(status.video.and_then(|v| v.width_out), Some(854));
        let stopped = async {
            while job.process.lock().await.as_ref().is_some_and(Proc::running) {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(10), stopped).await.expect("the first rendition stops");

        // Seamless: the second's segment 2 starts when the first's would.
        let (_, top_2) = jobs.rendition_segment(&job, 0, 2).await.unwrap();
        assert!((first_video_pts(&lower) - first_video_pts(&top_2)).abs() < 0.001);
        assert!(first_video_pts(&top) < first_video_pts(&top_2));
        assert_eq!(job.status().rendition, 0);
        jobs.remove(&created.id).await;
        assert!(!dir.path().join("jobs").join(&created.id).exists(), "the variants' folders with it");
    }

    #[tokio::test]
    async fn hardware_failing_jobs_have_the_encoders_tested_again() {
        let dir = tempfile::tempdir().unwrap();
        let jobs = manager(dir.path());
        let cache = dir.path().join("capabilities.json");
        std::fs::write(&cache, "{}").unwrap();
        for _ in 1..HARDWARE_FAILURES {
            jobs.hardware_failed();
        }
        assert!(cache.exists(), "a job or two: the hardware may be busy");
        jobs.hardware_failed();
        assert!(!cache.exists(), "tested again at the next start");
    }

    #[test]
    fn only_a_hardware_process_failing_before_its_first_segment_hands_over() {
        let dir = tempfile::tempdir().unwrap();
        let job = job(dir.path(), vec![0, 4_000, 8_000, 12_000]);
        let (metrics, tail) = (Metrics::default(), Mutex::new(VecDeque::new()));
        let exits = |hardware: bool, start: u32, next: u32, killed: bool| {
            job.runtime.lock().process = Some(ProcessStatus { hardware, start_segment: start, ..Default::default() });
            job.on_exit(0, 3, next, Ok(exit(1)), killed, None, &tail, &metrics);
            job.runtime.lock().hardware_failed
        };
        assert!(!exits(true, 1, 1, true), "stopped on purpose");
        assert!(!exits(true, 1, 2, false), "it made segment 1: the encoder works");
        assert!(!exits(false, 1, 1, false), "software");
        assert!(exits(true, 1, 1, false));
    }

    #[test]
    fn playlists_list_every_segment_with_its_length() {
        let dir = tempfile::tempdir().unwrap();
        let job = job(dir.path(), vec![0, 4_000, 8_400, 10_500]);
        assert_eq!(
            job.playlist(),
            "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:5\n#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n\
             #EXTINF:4.000,\n0.ts\n#EXTINF:4.400,\n1.ts\n#EXTINF:2.100,\n2.ts\n#EXT-X-ENDLIST\n",
            "the target duration covers the longest segment"
        );
    }

    #[test]
    fn segments_become_ready_once() {
        let dir = tempfile::tempdir().unwrap();
        let job = job(dir.path(), vec![0, 4_000, 8_000, 12_000]);
        job.mark_ready(1, Place::Disk);
        job.mark_ready(1, Place::Disk);
        job.mark_ready(7, Place::Disk);
        assert_eq!(job.ready_count.load(Relaxed), 1, "counted once; out of range ignored");
        assert!(job.is_ready(1) && !job.is_ready(0) && !job.is_ready(7));
        assert_eq!(job.first_ready(0, 3), Some(1));
        assert_eq!(job.first_ready(2, 3), None);
        assert_eq!(job.status().segments_ready, 1);
        assert_eq!(job.segment_path(2), dir.path().join("2.ts"));
        job.record_served(1_000);
        job.record_served(500);
        let status = job.status();
        assert_eq!((status.served_segments, status.served_bytes), (2, 1_500));
    }

    #[tokio::test]
    async fn a_request_going_away_wakes_the_others() {
        let dir = tempfile::tempdir().unwrap();
        let job = job(dir.path(), vec![0, 4_000, 8_000]);
        let first = Waiting::register(&job, 0);
        // Another request, held back while the first waits.
        let notified = job.notify.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        drop(first);
        let woken = tokio::time::timeout(Duration::from_millis(100), notified).await;
        assert!(woken.is_ok(), "it looks again now, not at its next tick");
    }

    #[test]
    fn woken_requests_see_the_process_past_the_finished_segment() {
        use std::task::{Context, Wake, Waker};
        /// Where the process was when the request was woken.
        struct Seen {
            next: Arc<AtomicU32>,
            at_wake: Mutex<Option<u32>>,
        }
        impl Wake for Seen {
            fn wake(self: Arc<Self>) {
                *self.at_wake.lock() = Some(self.next.load(Relaxed));
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let job = job(dir.path(), vec![0, 4_000, 8_000]);
        let next = Arc::new(AtomicU32::new(0));
        let seen = Arc::new(Seen { next: next.clone(), at_wake: Mutex::new(None) });
        let waker = Waker::from(seen.clone());
        let mut notified = std::pin::pin!(job.notify.notified());
        assert!(notified.as_mut().poll(&mut Context::from_waker(&waker)).is_pending());
        segment_written(&job, &next, 0, Place::Disk);
        assert_eq!(*seen.at_wake.lock(), Some(1), "the process had moved on when the request woke");
        assert!(job.is_ready(0));
    }

    #[test]
    fn requests_count_as_waiting_while_they_wait() {
        let dir = tempfile::tempdir().unwrap();
        let job = job(dir.path(), vec![0, 4_000, 8_000]);
        let first = Waiting::register(&job, 1);
        let second = Waiting::register(&job, 1);
        assert!(job.is_awaited(|n| n == 1));
        drop(first);
        assert!(job.is_awaited(|n| n == 1), "the other request still waits");
        drop(second);
        assert!(!job.is_awaited(|n| n == 1));
    }

    #[cfg(unix)]
    fn exit(code: i32) -> ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        ExitStatus::from_raw(code << 8)
    }

    #[cfg(windows)]
    fn exit(code: u32) -> ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        ExitStatus::from_raw(code)
    }

    #[test]
    fn process_exits_decide_the_job_state() {
        let dir = tempfile::tempdir().unwrap();
        let job = job(dir.path(), vec![0, 4_000, 8_000]);
        let metrics = Metrics::default();
        let tail = Mutex::new(VecDeque::new());
        job.on_exit(0, 2, 2, Ok(exit(0)), false, None, &tail, &metrics);
        assert_eq!(job.status().state, JobState::Finished, "produced up to the end");
        job.on_exit(0, 1, 1, Ok(exit(0)), false, None, &tail, &metrics);
        assert_eq!(job.status().state, JobState::Idle, "the window ended before the file did");
        job.on_exit(0, 2, 1, Ok(exit(1)), true, None, &tail, &metrics);
        assert_eq!(job.status().state, JobState::Idle, "stopped on purpose is no failure");
        assert_eq!(metrics.process_failures.load(Relaxed), 0);

        // A failure keeps the end of ffmpeg's output.
        tail.lock().extend(["one", "two", "three", "four", "five"].map(String::from));
        job.on_exit(0, 2, 1, Ok(exit(1)), false, None, &tail, &metrics);
        let status = job.status();
        assert_eq!(status.state, JobState::Failed);
        assert_eq!(status.last_error.as_deref(), Some("two | three | four | five"));
        job.on_exit(0, 2, 1, Err(std::io::Error::other("gone")), false, None, &Mutex::new(VecDeque::new()), &metrics);
        assert!(job.status().last_error.unwrap().contains("could not be awaited: gone"));
        assert_eq!(metrics.process_failures.load(Relaxed), 2);
        assert_eq!(metrics.jobs_failed.load(Relaxed), 1, "a failing job counts once");
        assert_eq!(job.runtime.lock().failures.get(&1), Some(&2), "failures per start segment");
    }

    #[test]
    fn a_process_stopped_long_ago_leaves_the_running_one_be() {
        let dir = tempfile::tempdir().unwrap();
        let job = job(dir.path(), vec![0, 4_000, 8_000]);
        let (metrics, tail) = (Metrics::default(), Mutex::new(VecDeque::new()));
        {
            let mut rt = job.runtime.lock();
            rt.generation = 2;
            rt.state = JobState::Running;
            rt.process = Some(ProcessStatus { pid: Some(7), ..Default::default() });
        }
        // The first process, which outlived its stop by seconds...
        job.on_exit(1, 2, 1, Ok(exit(1)), true, None, &tail, &metrics);
        let status = job.status();
        assert_eq!((status.state, status.process.and_then(|p| p.pid)), (JobState::Running, Some(7)));
        // ...then the second, the running one.
        job.on_exit(2, 2, 1, Ok(exit(1)), true, None, &tail, &metrics);
        let status = job.status();
        assert_eq!((status.state, status.process), (JobState::Idle, None));
    }

    #[test]
    fn progress_lines_update_the_running_process() {
        let dir = tempfile::tempdir().unwrap();
        let job = job(dir.path(), vec![0, 4_000]);
        let block = |pairs: &[(&str, &str)]| -> HashMap<String, String> {
            pairs.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect()
        };
        job.update_progress(&block(&[("fps", "50")]), 0, None);
        assert!(job.status().process.is_none(), "no process, nothing to update");

        job.runtime.lock().process = Some(ProcessStatus::default());
        job.update_progress(
            &block(&[
                ("frame", "120"),
                ("fps", "48.5"),
                ("speed", " 2.1x"),
                ("bitrate", "3000.5kbits/s"),
                ("out_time_us", "5000000"),
                ("drop_frames", "2"),
            ]),
            12_000,
            None,
        );
        let p = job.status().process.unwrap();
        assert_eq!((p.frame, p.fps, p.speed, p.bitrate_kbps), (Some(120), Some(48.5), Some(2.1), Some(3000.5)));
        // How fast it went lately wins over FFmpeg's average since it started.
        job.update_progress(&block(&[("speed", "6.2x")]), 12_000, Some(1.0));
        assert_eq!(job.status().process.unwrap().speed, Some(1.0));
        assert_eq!((p.out_time_ms, p.drop_frames), (17_000, Some(2)), "absolute media time");

        // Values ffmpeg doesn't know yet.
        job.update_progress(
            &block(&[("fps", "0.00"), ("speed", "N/A"), ("bitrate", "N/A"), ("out_time_us", "-9223372036854775807")]),
            12_000,
            None,
        );
        let p = job.status().process.unwrap();
        assert_eq!((p.fps, p.speed, p.bitrate_kbps), (None, None, None));
        assert_eq!(p.out_time_ms, 17_000, "a negative time keeps the last one");
    }

    #[tokio::test]
    async fn a_gpu_that_cant_decode_is_said_so() {
        let dir = tempfile::tempdir().unwrap();
        let job = Arc::new(job(dir.path(), vec![0, 4_000]));
        let tail = Arc::new(Mutex::new(VecDeque::new()));
        let pace = Arc::new(Mutex::new(Pace::default()));
        let stderr: &[u8] = b"frame=1\nprogress=continue\n\
            [hevc @ 000001c1] Failed setup for format vulkan: hwaccel initialisation returned error.\n\
            frame=2\nprogress=continue\n";
        read_progress(job.clone(), stderr, 0, tail.clone(), pace).await;
        assert!(job.runtime.lock().decoded_in_software);
        assert_eq!(job.status().warnings, ["GPU decoding failed, using the CPU"]);
        assert_eq!(tail.lock().len(), 1, "the line is kept for a failure's message too");

        let fine = Arc::new(self::job(dir.path(), vec![0, 4_000]));
        read_progress(fine.clone(), &b"frame=1\nprogress=continue\n"[..], 0, tail, Arc::default()).await;
        assert!(!fine.runtime.lock().decoded_in_software && fine.status().warnings.is_empty());
    }

    #[test]
    fn a_gpu_that_cant_decode_shows_the_cpu_decoding() {
        use pley_transcoder_api::{EncoderInfo, GpuPipeline, HwAccel, MediaStream, StreamKind, VideoRequest};
        let dir = tempfile::tempdir().unwrap();
        let nvenc = EncoderInfo {
            name: "h264_nvenc".into(),
            codec: VideoCodec::H264,
            hardware: true,
            accel: HwAccel::Nvenc,
            main10: false,
        };
        let ff = Ffmpeg {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            caps: pley_transcoder_api::Capabilities {
                encoders: vec![nvenc],
                hwaccels: vec!["cuda".into()],
                gpu_pipelines: vec![GpuPipeline { accel: HwAccel::Nvenc, codecs: vec!["h264".into()] }],
                ..Default::default()
            },
            segment_times_relative: true,
        };
        let video = MediaStream {
            kind: StreamKind::Video,
            codec: "h264".into(),
            width: Some(1920),
            height: Some(1080),
            ..Default::default()
        };
        let media = MediaInfo { duration_ms: Some(8_000), streams: vec![video], ..Default::default() };
        let req =
            JobRequest { input: "/media/movie.mkv".into(), video: Some(VideoRequest::default()), ..Default::default() };
        let mut job = job(dir.path(), vec![0, 4_000, 8_000]);
        job.plan = Plan::build(&req, &media, &ff, None).unwrap();
        job.media = Arc::new(media);
        let shown = |job: &Job| job.status().video.map(|v| (v.decoder, v.pipeline)).unwrap();
        assert_eq!(shown(&job), (Some("cuda".into()), Some("gpu".into())));

        job.decoded_in_software();
        assert_eq!(shown(&job), (Some("cpu".into()), Some("hybrid".into())));
    }

    #[test]
    fn segment_counted_limits_keep_their_length_in_time() {
        let dir = tempfile::tempdir().unwrap();
        let mut job = job(dir.path(), vec![0, 4_000, 8_000]);
        assert_eq!((job.segments_in(LOOKAHEAD), job.segments_in(KEEP_BEHIND)), (3, 90));
        job.plan.segment_seconds = 2.0;
        assert_eq!((job.segments_in(LOOKAHEAD), job.segments_in(KEEP_BEHIND)), (6, 180));
        job.plan.segment_seconds = 10.0;
        assert_eq!((job.segments_in(LOOKAHEAD), job.segments_in(KEEP_BEHIND)), (1, 36));
        job.plan.segment_seconds = 30.0;
        assert_eq!(job.segments_in(LOOKAHEAD), 1, "at least the next one");
    }

    #[test]
    fn segments_far_behind_the_playhead_are_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let job = job(dir.path(), (0..=200).map(|i| i * 4_000).collect());
        for n in [0, 10, 59, 60, 100, 150] {
            std::fs::write(job.segment_path(n), b"ts").unwrap();
            job.mark_ready(n, Place::Disk);
        }
        job.last_requested.store(90, Relaxed);
        job.prune();
        assert_eq!(job.ready_count.load(Relaxed), 6, "nothing is that far behind yet");

        job.last_requested.store(150, Relaxed);
        job.prune();
        for n in [0, 10, 59] {
            assert!(!job.is_ready(n) && !job.segment_path(n).exists(), "segment {n}");
        }
        for n in [60, 100, 150] {
            assert!(job.is_ready(n) && job.segment_path(n).exists(), "segment {n}");
        }
        assert_eq!(job.ready_count.load(Relaxed), 3);
    }

    const GIB: u64 = 1024 * 1024 * 1024;

    /// A computer with 16 GB, half of it its folder in RAM: 4 GB for segments.
    fn roomy() -> Arc<Mutex<memory::Room>> {
        Arc::new(Mutex::new(memory::Room { size: 8 * GIB, free: 8 * GIB, memory: 16 * GIB, available: 12 * GIB }))
    }

    /// A store in `dir` ("shm", made as `/dev/shm` is there) whose room is
    /// `room`'s.
    fn store(dir: &Path, room: Arc<Mutex<memory::Room>>) -> Arc<MemoryStore> {
        let shm = dir.join("shm");
        std::fs::create_dir_all(&shm).unwrap();
        MemoryStore::new(&shm, Box::new(move || Some(*room.lock()))).unwrap()
    }

    /// Job `id` (folder `dir/id` on disk), 4 s segments for 400 of them,
    /// asking for memory in `store`, reckoned to take `want` there.
    fn job_in_memory(dir: &Path, id: &str, store: &Arc<MemoryStore>, want: u64) -> Job {
        let disk = dir.join(id);
        let memory_dir = store.root().join(id);
        std::fs::create_dir_all(&disk).unwrap();
        std::fs::create_dir_all(&memory_dir).unwrap();
        let memory = JobMemory {
            store: store.clone(),
            dir: memory_dir,
            want,
            lease: Mutex::new(None),
            bytes: AtomicU64::new(0),
        };
        Job { id: id.into(), memory: Some(memory), ..job(&disk, (0..=400).map(|i| i * 4_000).collect()) }
    }

    /// Segment `n` of `bytes` made in `place`.
    fn make(job: &Job, n: u32, place: Place, bytes: usize) {
        std::fs::write(job.path_in(place, n), vec![0u8; bytes]).unwrap();
        job.mark_ready(n, place);
    }

    #[test]
    fn segments_go_to_memory_while_it_has_room() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path(), roomy());
        let big = job_in_memory(dir.path(), "big", &store, 3 * GIB);
        let other = job_in_memory(dir.path(), "other", &store, 2 * GIB);
        assert_eq!(big.choose_place(), Place::Memory);
        assert_eq!(big.choose_place(), Place::Memory, "it keeps its reservation");
        assert_eq!(store.reserved(), 3 * GIB);
        assert_eq!(other.choose_place(), Place::Disk, "past the budget: to disk");
        // A job that didn't ask for memory (or isn't throttled) has none.
        assert_eq!(job(dir.path(), vec![0, 4_000]).choose_place(), Place::Disk);

        // Each segment is where its process made it, and is found there.
        make(&big, 0, Place::Memory, 1_000);
        make(&big, 1, Place::Disk, 10);
        assert!(big.segment_path(0).starts_with(store.root()), "{:?}", big.segment_path(0));
        assert_eq!(big.segment_path(1), dir.path().join("big").join("1.ts"));
        let status = big.status();
        assert!(status.in_memory);
        assert_eq!((status.memory_bytes, store.used()), (1_000, 1_000), "segments on disk don't count");

        // The job gone: its reservation and its memory with it.
        big.memory.as_ref().unwrap().release();
        assert_eq!((store.used(), store.reserved()), (0, 0));
        assert_eq!(other.choose_place(), Place::Memory);
    }

    #[test]
    fn segments_in_memory_keep_a_minute_behind_and_three_ahead() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path(), roomy());
        let job = job_in_memory(dir.path(), "job", &store, GIB);
        // 4 s segments: 15 in a minute, 45 in three, 90 in six (on disk).
        let in_memory = [0, 84, 85, 100, 145, 146, 300];
        for n in in_memory {
            make(&job, n, Place::Memory, 100);
        }
        for n in [5, 10, 30] {
            make(&job, n, Place::Disk, 100);
        }
        assert_eq!(store.used(), 700);
        job.last_requested.store(100, Relaxed);
        job.prune();
        for n in [0, 84, 146, 300, 5] {
            assert!(!job.is_ready(n) && !job.path_in(Place::Memory, n).exists(), "segment {n} went");
        }
        assert!(!job.path_in(Place::Disk, 5).exists());
        for n in [85, 100, 145, 10, 30] {
            assert!(job.is_ready(n) && job.segment_path(n).exists(), "segment {n} stayed");
        }
        assert_eq!((store.used(), job.memory.as_ref().unwrap().bytes.load(Relaxed)), (300, 300));
    }

    #[test]
    fn a_segment_half_made_in_memory_goes() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path(), roomy());
        let job = job_in_memory(dir.path(), "job", &store, GIB);
        make(&job, 7, Place::Memory, 100);
        std::fs::write(job.path_in(Place::Memory, 8), b"half").unwrap();
        job.drop_unmade(8, 0);
        job.drop_unmade(7, 0);
        assert!(!job.path_in(Place::Memory, 8).exists());
        assert!(job.path_in(Place::Memory, 7).exists(), "made, it stays");
    }

    #[tokio::test]
    async fn throttled_jobs_asking_for_memory_get_a_folder_in_ram() {
        use pley_transcoder_api::{AudioRequest, MediaStream, StreamKind, TranscodeOptions};
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("song.flac");
        std::fs::write(&input, b"not really audio").unwrap();
        let ff = Ffmpeg {
            ffmpeg: dir.path().join("no-ffmpeg"),
            ffprobe: dir.path().join("no-ffprobe"),
            caps: pley_transcoder_api::Capabilities { ffmpeg_available: true, readrate: true, ..Default::default() },
            segment_times_relative: true,
        };
        let jobs = manager_with(dir.path(), ff);
        let store = store(dir.path(), roomy());
        jobs.keep_in_memory(store.clone());
        let request = |options: TranscodeOptions| JobRequest {
            input: input.to_string_lossy().into_owned(),
            media: Some(MediaInfo {
                duration_ms: Some(60_000),
                streams: vec![MediaStream { kind: StreamKind::Audio, codec: "flac".into(), ..Default::default() }],
                ..Default::default()
            }),
            audio: Some(AudioRequest::default()),
            options,
            ..Default::default()
        };
        let made = jobs.create(request(TranscodeOptions::default())).await.unwrap();
        let job = jobs.get(&made.id).unwrap();
        let folder = &job.memory.as_ref().expect("in memory by default").dir;
        assert!(folder.is_dir() && folder.starts_with(store.root()));
        for options in [
            TranscodeOptions { in_memory: false, ..Default::default() },
            // Its lead on the player isn't bounded.
            TranscodeOptions { throttle: false, ..Default::default() },
        ] {
            let other = jobs.create(request(options)).await.unwrap();
            assert!(jobs.get(&other.id).unwrap().memory.is_none());
        }
        let folder = folder.clone();
        jobs.remove(&made.id).await;
        assert!(!folder.exists(), "the folder in RAM goes with the job");
    }

    #[tokio::test]
    async fn memory_running_short_moves_the_job_taking_most_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        let room = roomy();
        let store = store(dir.path(), room.clone());
        let mgr = manager(dir.path());
        mgr.keep_in_memory(store.clone());
        let jobs: Vec<Arc<Job>> =
            ["small", "large"].map(|id| Arc::new(job_in_memory(dir.path(), id, &store, GIB))).into();
        for (job, bytes) in jobs.iter().zip([1_000, 5_000]) {
            assert_eq!(job.choose_place(), Place::Memory);
            make(job, 0, Place::Memory, bytes);
            mgr.jobs.lock().insert(job.id.clone(), job.clone());
        }
        assert!(mgr.memory_victim().is_none(), "plenty of room");

        // Something else takes the computer's memory.
        room.lock().available = GIB;
        assert_eq!(mgr.memory_victim().unwrap().id, "large");
        mgr.relieve_memory().await;
        let (small, large) = (&jobs[0], &jobs[1]);
        assert!(!large.status().in_memory && small.status().in_memory);
        assert_eq!(large.choose_place(), Place::Disk, "no room while short");
        assert!(large.segment_path(0).starts_with(store.root()), "what it made stays where it is");
        assert_eq!(mgr.memory_victim().unwrap().id, "small", "one a look");

        room.lock().available = 12 * GIB;
        assert!(mgr.memory_victim().is_none());
        assert_eq!(large.choose_place(), Place::Memory, "room again: the next process goes to memory");
        assert_eq!(mgr.memory_bytes(), 6_000);
    }

    #[test]
    fn fmp4_playlists_name_their_init_and_segments() {
        let dir = tempfile::tempdir().unwrap();
        let mut job = job(dir.path(), vec![0, 4_000, 8_400, 10_500]);
        job.plan.segment_format = SegmentFormat::Fmp4;
        assert_eq!(
            job.playlist(),
            "#EXTM3U\n#EXT-X-VERSION:7\n#EXT-X-TARGETDURATION:5\n#EXT-X-MEDIA-SEQUENCE:0\n#EXT-X-PLAYLIST-TYPE:VOD\n\
             #EXT-X-MAP:URI=\"init.mp4\"\n\
             #EXTINF:4.000,\n0.m4s\n#EXTINF:4.400,\n1.m4s\n#EXTINF:2.100,\n2.m4s\n#EXT-X-ENDLIST\n"
        );
        // A variant's: its own init.
        let variant = Job { prefix: "v1-".into(), ..job };
        let playlist = variant.playlist();
        assert!(playlist.contains("#EXT-X-MAP:URI=\"v1-init.mp4\"\n#EXTINF:4.000,\nv1-0.m4s\n"), "{playlist}");
        assert!(playlist.contains("\nv1-2.m4s\n") && !playlist.contains(".ts"), "{playlist}");
    }

    #[test]
    fn an_fmp4_part_half_made_in_memory_goes() {
        let dir = tempfile::tempdir().unwrap();
        let store = store(dir.path(), roomy());
        let mut job = job_in_memory(dir.path(), "job", &store, GIB);
        job.plan.segment_format = SegmentFormat::Fmp4;
        assert!(job.path_in(Place::Memory, 8).ends_with("8.m4s"));
        let part = job.part_path(Place::Memory, 8, 3);
        assert!(part.starts_with(store.root()) && part.ends_with("8.m4s.p3"), "{}", part.display());
        std::fs::write(&part, b"half").unwrap();
        // Another process's part is its own.
        let other = job.part_path(Place::Memory, 8, 4);
        std::fs::write(&other, b"half").unwrap();
        job.drop_unmade(8, 3);
        assert!(!part.exists() && other.exists());
    }

    /// 30 s of 320×180 H.264 with B-frames and a keyframe every 2 s, and a
    /// tone in AAC (`sound.mkv`: Matroska, whose Cues are the keyframe index
    /// at once), and a manager whose only encoder is libx264 (`None` without
    /// FFmpeg).
    fn with_sound(dir: &Path) -> Option<(PathBuf, Arc<JobManager>)> {
        let (_, jobs) = with_ffmpeg(dir, "320x180")?;
        let input = dir.join("sound.mkv");
        let made = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
            .arg("testsrc2=size=320x180:rate=25:duration=30")
            .args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=30"])
            .args(["-c:v", "libx264", "-preset", "veryfast", "-bf", "2", "-g", "50", "-sc_threshold", "0"])
            .args(["-c:a", "aac"])
            .arg(&input)
            .status()
            .unwrap();
        assert!(made.success());
        Some((input, jobs))
    }

    /// fMP4 segments of 2 s of `input`, its video made as `mode` says and
    /// its sound AAC, made at full speed with x264's `veryfast`.
    fn fmp4_request(input: &Path, mode: VideoMode) -> JobRequest {
        use pley_transcoder_api::{AudioRequest, TranscodeOptions, VideoRequest};
        JobRequest {
            input: input.to_string_lossy().into_owned(),
            video: Some(VideoRequest { mode, ..Default::default() }),
            audio: Some(AudioRequest::default()),
            segment_seconds: Some(2.0),
            options: TranscodeOptions { throttle: false, preset: Some("veryfast".into()), ..Default::default() },
            segment_format: SegmentFormat::Fmp4,
            ..Default::default()
        }
    }

    /// The `ftyp` and `moov` of `init`, read.
    async fn read_init(init: &[u8]) -> Init {
        let mut reader = BoxReader::new(init);
        let (ftyp, moov) = (reader.next().await.unwrap().unwrap(), reader.next().await.unwrap().unwrap());
        assert!(reader.next().await.unwrap().is_none(), "nothing but the init");
        Init::parse(&ftyp.bytes, &moov.bytes).unwrap()
    }

    /// The fragments of fMP4 segment `segment`.
    async fn read_fragments_of(init: &Init, segment: &Path) -> Vec<Fragment> {
        let bytes = std::fs::read(segment).unwrap();
        let mut reader = BoxReader::new(bytes.as_slice());
        let mut fragments = Vec::new();
        while let Some(moof) = reader.next().await.unwrap() {
            let mdat = reader.next().await.unwrap().unwrap();
            assert_eq!((moof.kind, mdat.kind), (*b"moof", *b"mdat"), "{}", segment.display());
            fragments.push(Fragment::parse(moof.bytes, mdat.bytes, init).unwrap());
        }
        assert!(!fragments.is_empty(), "{} is empty", segment.display());
        fragments
    }

    /// Each fragment's tracks and their decode times.
    fn decode_times(fragments: &[Fragment]) -> Vec<Vec<(u32, i64)>> {
        fragments.iter().map(|f| f.trafs.iter().map(|t| (t.track, t.tfdt)).collect()).collect()
    }

    /// The video's decode time at the start of a segment and after it.
    fn video_span(init: &Init, fragments: &[Fragment]) -> (i64, i64) {
        let video = init.video.unwrap();
        let trafs: Vec<&crate::fmp4::Traf> =
            fragments.iter().flat_map(|f| f.trafs.iter()).filter(|t| t.track == video).collect();
        let last = trafs.last().unwrap();
        let length: i64 = last.runs.iter().flat_map(|r| r.samples.iter()).map(|s| i64::from(s.duration)).sum();
        (trafs[0].tfdt, last.tfdt + length)
    }

    /// No part files left in `dir`.
    fn no_parts(dir: &Path) {
        let names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".p"))
            .collect();
        assert!(names.is_empty(), "{names:?}");
    }

    async fn stopped(job: &Job) {
        let stopped = async {
            while job.process.lock().await.as_ref().is_some_and(Proc::running) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        };
        tokio::time::timeout(Duration::from_secs(30), stopped).await.expect("its process stops");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fmp4_jobs_serve_an_init_and_segments_that_play_after_it() {
        let dir = tempfile::tempdir().unwrap();
        let Some((input, jobs)) = with_sound(dir.path()) else { return };
        let created = jobs.create(fmp4_request(&input, VideoMode::Transcode)).await.unwrap();
        assert_eq!((created.segment_format, created.segment_count), (SegmentFormat::Fmp4, 15));
        let job = jobs.get(&created.id).unwrap();
        // Asked for before its process wrote it: it waits for it.
        let (_, init_path) = jobs.rendition_init(&job, 0).await.unwrap();
        assert_eq!(init_path, job.dir.join("init.mp4"));
        let init_bytes = std::fs::read(&init_path).unwrap();
        let init = read_init(&init_bytes).await;
        let video = init.track(init.video.unwrap()).unwrap().clone();
        let audio = init.tracks.iter().find(|t| t.handler == *b"soun").unwrap().clone();
        assert_eq!((video.format, audio.format), (*b"avc1", *b"mp4a"));

        let mut whole = init_bytes.clone();
        for n in 0..created.segment_count {
            let path = jobs.segment(&job, n).await.unwrap();
            assert_eq!(path, job.dir.join(format!("{n}.m4s")));
            let fragments = read_fragments_of(&init, &path).await;
            // It starts on a keyframe, at its boundary (within a frame).
            let first = &fragments[0];
            let traf = first.trafs.iter().find(|t| t.track == video.id).expect("video first");
            assert_eq!(traf.runs[0].samples[0].flags & 0x0001_0000, 0, "segment {n} starts on a keyframe");
            let shown = first.presentation_start_ms(&init).unwrap();
            assert!((shown - job.plan.boundaries[n as usize]).abs() <= 40, "segment {n} starts at {shown} ms");
            if n == 0 {
                let sound = first.trafs.iter().find(|t| t.track == audio.id).expect("sound from the start");
                assert!(sound.tfdt >= 0, "the encoder's priming is left out: {}", sound.tfdt);
            }
            whole.extend(std::fs::read(&path).unwrap());
        }
        no_parts(&job.dir);

        // The init and the segments one after another play, every frame.
        let joined = dir.path().join("joined.mp4");
        std::fs::write(&joined, &whole).unwrap();
        let decoded = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-v", "error", "-i"])
            .arg(&joined)
            .args(["-f", "null", "-"])
            .output()
            .unwrap();
        assert!(decoded.status.success() && decoded.stderr.is_empty(), "{}", String::from_utf8_lossy(&decoded.stderr));
        let counted = std::process::Command::new("ffprobe")
            .args(["-v", "error", "-count_frames", "-select_streams", "v:0"])
            .args(["-show_entries", "stream=nb_read_frames", "-of", "csv=p=0"])
            .arg(&joined)
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&counted.stdout).trim(), "750");
        jobs.remove(&created.id).await;
    }

    /// A process started again within an fMP4 job (here at segment 12, as
    /// a new bitrate or a seek would) writes the same init as the first,
    /// and its video at the same times: segment 12 follows segment 11. Its
    /// sound, converted from a little before the seek point (and primed
    /// anew), starts within a frame or two of the first process's.
    async fn fmp4_restarts_go_on_where_the_first_process_was(mode: VideoMode) {
        let dir = tempfile::tempdir().unwrap();
        let Some((input, jobs)) = with_sound(dir.path()) else { return };
        let created = jobs.create(fmp4_request(&input, mode)).await.unwrap();
        assert_eq!(created.video.as_ref().map(|v| v.mode), Some(mode));
        assert_eq!(created.segment_format, SegmentFormat::Fmp4);
        let job = jobs.get(&created.id).unwrap();
        let count = created.segment_count;
        assert_eq!(count, 15);
        let mut first = Vec::new();
        for n in 0..count {
            first.push(jobs.segment(&job, n).await.unwrap());
        }
        let init_bytes = std::fs::read(job.init_path()).unwrap();
        let init = read_init(&init_bytes).await;
        let mut times = Vec::new();
        for path in &first {
            times.push(decode_times(&read_fragments_of(&init, path).await));
        }
        let (_, eleven_ends) = video_span(&init, &read_fragments_of(&init, &first[11]).await);
        let video = init.video.unwrap();
        let audio = init.tracks.iter().find(|t| t.handler == *b"soun").unwrap();
        let close = |a: &[Vec<(u32, i64)>], b: &[Vec<(u32, i64)>]| {
            a.len() == b.len()
                && a.iter().zip(b).all(|(a, b)| {
                    a.len() == b.len()
                        && a.iter().zip(b).all(|(&(ta, a), &(tb, b))| {
                            ta == tb
                                && match ta {
                                    t if t == video => a == b,
                                    // 50 ms.
                                    _ => (a - b).abs() <= i64::from(audio.timescale) / 20,
                                }
                        })
                })
        };
        stopped(&job).await;
        let restarts = job.status().restarts;

        assert_eq!(job.forget_from(12), 3);
        for n in 12..count {
            let path = jobs.segment(&job, n).await.unwrap();
            let fragments = read_fragments_of(&init, &path).await;
            let again = decode_times(&fragments);
            assert!(close(&again, &times[n as usize]), "segment {n}: {again:?}, first {:?}", times[n as usize]);
            if n == 12 {
                assert_eq!(video_span(&init, &fragments).0, eleven_ends, "segment 12 follows segment 11");
            }
        }
        assert!(job.status().restarts > restarts, "made again by another process");
        assert_eq!(std::fs::read(job.init_path()).unwrap(), init_bytes, "the same init");
        assert!(job.status().warnings.is_empty(), "{:?}", job.status().warnings);
        no_parts(&job.dir);
        jobs.remove(&created.id).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fmp4_transcodes_restarted_go_on_where_they_were() {
        fmp4_restarts_go_on_where_the_first_process_was(VideoMode::Transcode).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fmp4_remuxes_restarted_go_on_where_they_were() {
        fmp4_restarts_go_on_where_the_first_process_was(VideoMode::Copy).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_process_writing_another_init_fails_and_the_first_stays() {
        let dir = tempfile::tempdir().unwrap();
        let Some((input, jobs)) = with_sound(dir.path()) else { return };
        let created = jobs.create(fmp4_request(&input, VideoMode::Transcode)).await.unwrap();
        let job = jobs.get(&created.id).unwrap();
        jobs.segment(&job, 0).await.unwrap();
        let kept = std::fs::read(job.init_path()).unwrap();
        let encoder = job.init_encoder.lock().clone().expect("recorded with the init");
        assert_eq!(encoder, InitEncoder { hardware: false, stand_in: None, preset: None }, "veryfast, as asked");
        assert!(job.store_init(&kept, 7, &encoder).is_ok(), "the same again: nothing to do");
        let err = job.store_init(b"another encoder's", 7, &encoder).unwrap_err();
        assert!(err.contains("init segment changed"), "{err}");
        assert_eq!(std::fs::read(job.init_path()).unwrap(), kept, "players have it: it stays");

        // Processes whose init isn't the one players have fail at once,
        // twice, and the job says why.
        stopped(&job).await;
        *job.init.lock() = Some(Arc::from(&b"another encoder's"[..]));
        job.forget_from(10);
        let err = jobs.segment(&job, 10).await.unwrap_err();
        assert!(matches!(&err, ApiError::Failed(m) if m.contains("init segment changed")), "{err:?}");
        assert_eq!(
            job.status().last_error.as_deref(),
            Some("the init segment changed after a restart (encoder settings differ)")
        );
        assert!(!job.is_ready(10));
        assert_eq!(std::fs::read(job.init_path()).unwrap(), kept);
        no_parts(&job.dir);
        jobs.remove(&created.id).await;
    }

    #[test]
    fn fmp4_restarts_keep_the_encoder_their_init_was_made_with() {
        let gpu = InitEncoder { hardware: true, stand_in: None, preset: None };
        let x264 = InitEncoder { hardware: false, stand_in: Some("libx264".into()), preset: Some(2) };
        let software = InitEncoder { hardware: false, stand_in: None, preset: Some(4) };
        // (pinned, planned on the GPU, a stand-in, the hardware failed, preset asked, a hardware session).
        let choose = |pinned: Option<&InitEncoder>, failed: bool, preset: Option<usize>, hw_slot: bool| {
            restart_encoder(pinned, true, true, failed, preset, hw_slot)
        };

        // Before an init: the stand-in takes over from a busy or failed GPU.
        assert_eq!(choose(None, false, None, true).unwrap(), (EncoderChoice::Planned, None));
        assert_eq!(choose(None, false, Some(1), false).unwrap(), (EncoderChoice::NoSession, Some(1)));
        assert_eq!(choose(None, true, None, true).unwrap(), (EncoderChoice::Retreat, None));
        let err = restart_encoder(None, true, false, false, None, false).unwrap_err();
        assert!(matches!(err, ApiError::Busy(_)), "{err:?}");

        // Made on the GPU: on the GPU it stays, or nothing.
        assert_eq!(choose(Some(&gpu), false, None, true).unwrap(), (EncoderChoice::Planned, None));
        let err = choose(Some(&gpu), false, None, false).unwrap_err();
        assert!(matches!(&err, ApiError::Busy(m) if m.contains("can't change encoder")), "{err:?}");
        let err = choose(Some(&gpu), true, None, true).unwrap_err();
        assert!(
            matches!(&err, ApiError::Failed(m) if m == "the hardware encoder failed after the stream began; fMP4 segments can't change encoder"),
            "{err:?}"
        );

        // Made by the stand-in: it stays, with a hardware session free,
        // at its preset whatever is asked.
        assert_eq!(choose(Some(&x264), false, Some(5), true).unwrap(), (EncoderChoice::Kept, Some(2)));
        assert_eq!(choose(Some(&x264), true, None, false).unwrap(), (EncoderChoice::Kept, Some(2)));
        let err = restart_encoder(Some(&x264), true, false, false, None, true).unwrap_err();
        assert!(matches!(err, ApiError::Failed(_)), "{err:?}");

        // A software encode planned: the planned encoder, at the init's preset.
        assert_eq!(
            restart_encoder(Some(&software), false, false, false, Some(0), false).unwrap(),
            (EncoderChoice::Planned, Some(4))
        );
        assert_eq!(
            restart_encoder(None, false, false, false, Some(0), false).unwrap(),
            (EncoderChoice::Planned, Some(0))
        );
    }

    #[tokio::test]
    async fn fmp4_bitrates_stay_as_they_began() {
        let dir = tempfile::tempdir().unwrap();
        let jobs = manager(dir.path());
        let mut fmp4 = job(dir.path(), vec![0, 4_000, 8_000]);
        fmp4.plan.segment_format = SegmentFormat::Fmp4;
        let err = jobs.set_bitrate(&Arc::new(fmp4), 2_000).await.unwrap_err();
        assert!(matches!(&err, ApiError::BadRequest(m) if m.contains("fMP4")), "{err:?}");
        // MPEG-TS as before: refused only for video it doesn't convert.
        let err = jobs.set_bitrate(&Arc::new(job(dir.path(), vec![0, 4_000, 8_000])), 2_000).await.unwrap_err();
        assert!(matches!(&err, ApiError::BadRequest(m) if m.contains("isn't converted")), "{err:?}");
    }

    /// A job on the automatic preset, made to look behind its player with
    /// a process that is slow (a stand-in for the real one, stopped): what
    /// `adapt` does with it.
    async fn adapted(format: SegmentFormat) -> (Option<usize>, Option<usize>) {
        let dir = tempfile::tempdir().unwrap();
        let Some((input, jobs)) = with_sound(dir.path()) else { return (None, None) };
        let mut req = fmp4_request(&input, VideoMode::Transcode);
        req.segment_format = format;
        req.options.preset = Some(adaptive::AUTO.into());
        let created = jobs.create(req).await.unwrap();
        let job = jobs.get(&created.id).unwrap();
        jobs.segment(&job, 0).await.unwrap();
        if let Some(p) = job.process.lock().await.take() {
            p.stop().await;
        }
        let pinned = job.init_encoder.lock().as_ref().and_then(|e| e.preset);
        assert_eq!(pinned.is_some(), format == SegmentFormat::Fmp4, "an fMP4 job's preset is its init's");
        // Its exit sender gone: stopping it doesn't wait.
        let (_, exited) = watch::channel(false);
        *job.process.lock().await = Some(Proc {
            start: 5,
            end: 15,
            place: Place::Disk,
            started: Instant::now(),
            next: Arc::new(AtomicU32::new(5)),
            lookahead: 6,
            pace: Arc::default(),
            exited,
            kill: None,
            pause: Arc::new(watch::channel(false).0),
        });
        job.last_requested.store(5, Relaxed);
        {
            let mut rt = job.runtime.lock();
            rt.state = JobState::Running;
            rt.process = Some(ProcessStatus { speed: Some(0.5), ..Default::default() });
            rt.preset = Some((3, Instant::now().checked_sub(Duration::from_secs(60)).unwrap()));
        }
        jobs.adapt().await;
        let after = job.runtime.lock().preset.map(|(i, _)| i);
        if format == SegmentFormat::Fmp4 {
            assert_eq!(job.process.lock().await.as_ref().map(|p| p.start), Some(5), "left alone");
            // Made again later, it is at the init's preset.
            job.process.lock().await.take();
            job.runtime.lock().state = JobState::Idle;
            job.forget_from(6);
            jobs.segment(&job, 6).await.unwrap();
            assert_eq!(job.runtime.lock().preset.map(|(i, _)| i), pinned, "the init's preset");
        }
        jobs.remove(&created.id).await;
        (after, pinned)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_automatic_preset_moves_ts_jobs_only() {
        let (ts, _) = adapted(SegmentFormat::Ts).await;
        let (fmp4, _) = adapted(SegmentFormat::Fmp4).await;
        if ts.is_none() {
            return;
        }
        assert_eq!(ts, Some(2), "behind its player and slow: a step faster");
        assert_eq!(fmp4, Some(3), "pinned: where it was");
    }

    /// Each rendition of an fMP4 job has its own init, its segments laid
    /// out as the others', and keeps it as the player moves between them.
    #[tokio::test(flavor = "multi_thread")]
    async fn fmp4_renditions_each_have_their_init() {
        use pley_transcoder_api::VariantRequest;
        let dir = tempfile::tempdir().unwrap();
        let Some((input, jobs)) = with_sound(dir.path()) else { return };
        let variant =
            |w, h, kbps| VariantRequest { max_width: Some(w), max_height: Some(h), max_bitrate_kbps: Some(kbps) };
        let mut req = fmp4_request(&input, VideoMode::Transcode);
        req.variants = vec![variant(256, 144, 400), variant(192, 108, 200)];
        let created = jobs.create(req).await.unwrap();
        assert_eq!((created.variants, created.segment_format), (2, SegmentFormat::Fmp4));
        let job = jobs.get(&created.id).unwrap();
        let master = job.master_playlist();
        assert!(master.starts_with("#EXTM3U\n#EXT-X-VERSION:7\n"), "{master}");
        assert!(job.renditions()[2].playlist().contains("#EXT-X-MAP:URI=\"v2-init.mp4\"\n"));

        let mut inits = Vec::new();
        for k in 0..3 {
            let (rendition, path) = jobs.rendition_init(&job, k).await.unwrap();
            let expected =
                if k == 0 { job.dir.join("init.mp4") } else { job.dir.join(format!("v{k}")).join("init.mp4") };
            assert_eq!(path, expected);
            let bytes = std::fs::read(&path).unwrap();
            let init = read_init(&bytes).await;
            for n in 0..created.segment_count {
                let (_, segment) = jobs.rendition_segment(&job, k, n).await.unwrap();
                let first = &read_fragments_of(&init, &segment).await[0];
                let traf = first.trafs.iter().find(|t| Some(t.track) == init.video).expect("video first");
                assert_eq!(traf.runs[0].samples[0].flags & 0x0001_0000, 0, "v{k} {n} starts on a keyframe");
                let shown = first.presentation_start_ms(&init).unwrap();
                let at = rendition.plan.boundaries[n as usize];
                assert_eq!(at, job.plan.boundaries[n as usize], "laid out alike");
                assert!((shown - at).abs() <= 40, "v{k} segment {n} starts at {shown} ms, not {at}");
            }
            inits.push((path, bytes));
        }
        assert!(inits[0].1 != inits[1].1 && inits[1].1 != inits[2].1 && inits[0].1 != inits[2].1, "each its own");

        // Back to the first and down again, from segment 6: each keeps its init.
        for (k, n) in [(0, 6), (2, 7)] {
            job.renditions()[k].forget_from(n);
            jobs.rendition_segment(&job, k, n).await.unwrap();
            assert_eq!(jobs.rendition_init(&job, k).await.unwrap().1, inits[k].0);
        }
        for (path, bytes) in &inits {
            assert_eq!(&std::fs::read(path).unwrap(), bytes, "{}", path.display());
        }
        assert!(job.renditions().iter().all(|r| r.status().warnings.is_empty()));
        jobs.remove(&created.id).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fmp4_segments_in_memory_are_made_there() {
        use pley_transcoder_api::{EncoderInfo, HwAccel};
        let dir = tempfile::tempdir().unwrap();
        let Some((input, _)) = with_sound(dir.path()) else { return };
        // Throttled, as jobs keeping their segments in memory are.
        let x264 = EncoderInfo {
            name: "libx264".into(),
            codec: VideoCodec::H264,
            hardware: false,
            accel: HwAccel::None,
            main10: false,
        };
        let caps = pley_transcoder_api::Capabilities {
            ffmpeg_available: true,
            ffprobe_available: true,
            readrate: true,
            encoders: vec![x264],
            ..Default::default()
        };
        let ff = Ffmpeg { ffmpeg: "ffmpeg".into(), ffprobe: "ffprobe".into(), caps, segment_times_relative: true };
        let jobs = manager_with(dir.path(), ff);
        let store = store(dir.path(), roomy());
        jobs.keep_in_memory(store.clone());
        let mut req = fmp4_request(&input, VideoMode::Transcode);
        req.options.throttle = true;
        let created = jobs.create(req).await.unwrap();
        let job = jobs.get(&created.id).unwrap();
        let memory_dir = job.memory.as_ref().expect("in memory").dir.clone();
        let path = jobs.segment(&job, 0).await.unwrap();
        assert_eq!(path, memory_dir.join("0.m4s"));
        assert!(job.status().memory_bytes > 0);
        assert!(job.init_path().exists() && !memory_dir.join("init.mp4").exists(), "the init is on disk");
        // Stopped as it makes the next ones: what it was making goes.
        if let Some(p) = job.process.lock().await.take() {
            p.stop().await;
        }
        no_parts(&memory_dir);
        no_parts(&job.dir);
        jobs.remove(&created.id).await;
        assert!(!memory_dir.exists());
    }

    /// The cutter on its own, on what FFmpeg writes for 6 s with keyframes
    /// at 0, 2 and 4 s, for a job of three 2 s segments.
    #[tokio::test]
    async fn fragments_go_in_their_segments_and_outside_the_window_are_left_out() {
        let dir = tempfile::tempdir().unwrap();
        if with_ffmpeg(dir.path(), "320x180").is_none() {
            return;
        }
        let out = std::process::Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "testsrc2=size=320x180:rate=25"])
            .args(["-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000", "-t", "6"])
            .args(["-c:v", "libx264", "-preset", "veryfast", "-bf", "2", "-sc_threshold", "0"])
            .args(["-force_key_frames", "expr:gte(t,n_forced*2)", "-c:a", "aac", "-avoid_negative_ts", "disabled"])
            .args(["-f", "mp4", "-movflags", crate::plan::FMP4_MOVFLAGS, "-write_tmcd", "0", "pipe:1"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let stream = out.stdout;

        let background = Background::new();
        let cut = |job: &Arc<Job>, start: u32, end: u32, generation: u64, stream: &[u8]| {
            let made = SegmentsMade {
                job: job.clone(),
                place: Place::Disk,
                next: Arc::new(AtomicU32::new(start)),
                spawned: Instant::now(),
                starting: Some(background.start()),
                metrics: Arc::default(),
                pause: Arc::new(watch::channel(false).0),
                count: 0,
            };
            let reason = Arc::new(Mutex::new(None));
            let stream = stream.to_vec();
            async move {
                let encoder = InitEncoder { hardware: false, stand_in: None, preset: None };
                let pending =
                    read_fragments(stream.as_slice(), start, end, generation, encoder, made, reason.clone()).await;
                (pending, reason.lock().take())
            }
        };
        let mut fmp4 = job(dir.path(), vec![0, 2_000, 4_000, 6_000]);
        fmp4.plan.segment_format = SegmentFormat::Fmp4;
        let job = Arc::new(fmp4);

        // A process making segment 1 only (started a keyframe early, and
        // let a keyframe past its end through): the rest is left out.
        let (pending, reason) = cut(&job, 1, 2, 1, &stream).await;
        assert_eq!(reason, None);
        let pending = pending.expect("segment 1, whole once the process ended well");
        assert_eq!((pending.n, pending.part.clone()), (1, dir.path().join("1.m4s.p1")));
        assert!(!job.is_ready(1));
        pending.commit().await.unwrap();
        assert!(job.is_ready(1) && !job.is_ready(0) && !job.is_ready(2));
        let init = read_init(&std::fs::read(job.init_path()).unwrap()).await;
        let fragments = read_fragments_of(&init, &dir.path().join("1.m4s")).await;
        let starts: Vec<_> = fragments.iter().filter_map(|f| f.presentation_start_ms(&init)).collect();
        assert_eq!(starts, [2_000]);
        assert!(!dir.path().join("0.m4s").exists() && !dir.path().join("2.m4s").exists());

        // The whole: segments are made as the next begins, the last at the
        // end, and a process that didn't end well leaves no part behind.
        let (pending, reason) = cut(&job, 0, 3, 2, &stream).await;
        assert_eq!(reason, None);
        assert!(job.is_ready(0), "made once segment 1 began");
        let pending = pending.unwrap();
        assert_eq!(pending.n, 2);
        let part = pending.part.clone();
        pending.discard().await;
        assert!(!part.exists() && !job.is_ready(2));
        no_parts(dir.path());
        let first = read_fragments_of(&init, &dir.path().join("0.m4s")).await;
        let audio = init.tracks.iter().find(|t| t.handler == *b"soun").unwrap().id;
        let sound = first[0].trafs.iter().find(|t| t.track == audio).unwrap();
        assert_eq!(sound.tfdt, 0, "the encoder's priming left out");

        // Output that isn't fMP4, or stops inside a box, stops the process.
        for (broken, why) in [(&b"\0\0\0\x10moofnot an init"[..], "no movie box"), (&stream[..stream.len() - 9], "")] {
            let (pending, reason) = cut(&job, 0, 3, 3, broken).await;
            assert!(pending.is_none());
            assert!(reason.as_deref().is_some_and(|r| r.contains(why)), "{reason:?}");
            no_parts(dir.path());
        }
    }
}

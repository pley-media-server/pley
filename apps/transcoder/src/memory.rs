// SPDX-License-Identifier: AGPL-3.0-only
//! Segments kept in memory ([`TranscodeOptions::in_memory`]).
//!
//! FFmpeg writes a job's segments as files, so memory is a folder in RAM:
//! `/dev/shm` on Linux, a tmpfs. Windows and macOS have none by default (see
//! [`MemoryStore::open`]), and their segments stay on disk.
//!
//! Memory is bounded twice. A job in memory keeps fewer segments behind
//! the playhead than on disk ([`KEEP_BEHIND`]), drops those far ahead of it
//! ([`KEEP_AHEAD`]: a seek back leaves them), and is throttled, so its
//! process pauses 90 s ahead of the player. And the store has a budget
//! ([`budget`]): each job reserves what it is reckoned to take
//! ([`reckoned_bytes`]) before its segments go to memory, and goes to disk
//! when that doesn't fit, or when the folder or the computer is short of
//! memory ([`admits`]). When memory runs short later ([`short`]), the job
//! taking most moves to disk (`JobManager::relieve_memory`): the segments it
//! made stay where they are, the next ones go to disk.
//!
//! What memory the transcoder can still take, a control group's limit
//! counted, is told too ([`available`]): `/v1/status` reports it.
//!
//! [`TranscodeOptions::in_memory`]: pley_transcoder_api::TranscodeOptions::in_memory

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::time::Duration;

use parking_lot::Mutex;
use pley_transcoder_api::{MediaInfo, MemoryInfo};

use crate::plan::Plan;

/// Segments in memory kept behind the playhead (on disk, 6 minutes).
pub const KEEP_BEHIND: Duration = Duration::from_secs(60);
/// Segments in memory kept ahead of it: further ones (a seek back leaves
/// them) go. A throttled process pauses 90 s ahead.
pub const KEEP_AHEAD: Duration = Duration::from_secs(180);
/// What a job in memory is reckoned to hold: [`KEEP_BEHIND`], and the 90 s
/// a throttled process makes ahead with half a minute to spare.
const RECKONED: Duration = Duration::from_secs(180);
/// A budget smaller than this isn't worth having: memory isn't offered.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
const MIN_BUDGET: u64 = 256 * MIB;
/// Room left in the folder for others.
const FOLDER_MARGIN: u64 = 64 * MIB;
const MIB: u64 = 1024 * 1024;
/// The prefix of the store's folder in the folder in RAM, before the
/// transcoder's process id.
const PREFIX: &str = "pley-transcoder-";

/// What the folder in RAM and the computer have, in bytes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Room {
    /// The folder's size, and what is free in it.
    pub size: u64,
    pub free: u64,
    /// The computer's memory, and what is available (Linux's
    /// `MemAvailable`: what can be had without swapping).
    pub memory: u64,
    pub available: u64,
}

/// The most segments take in memory: half the folder (a tmpfs is half the
/// memory by default), a quarter of the computer's memory.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn budget(room: &Room) -> u64 {
    (room.size / 2).min(room.memory / 4)
}

/// Memory left to the rest of the computer: a tenth, 512 MB at least.
fn reserve(memory: u64) -> u64 {
    (memory / 10).max(512 * MIB)
}

/// Whether a job reckoned to take `want` bytes may keep its segments in
/// memory, with `committed` bytes of `budget` reserved or used already.
pub fn admits(committed: u64, want: u64, budget: u64, room: &Room) -> bool {
    committed.saturating_add(want) <= budget
        && room.free >= want.saturating_add(FOLDER_MARGIN)
        && room.available >= want.saturating_add(reserve(room.memory))
}

/// Whether memory runs short with `used` bytes of segments in it: past
/// the budget, or the folder or the computer short of room.
pub fn short(used: u64, budget: u64, room: &Room) -> bool {
    used > budget || room.free < FOLDER_MARGIN || room.available < reserve(room.memory)
}

/// What a job's segments are reckoned to take in memory: [`RECKONED`] of
/// its bitrate, a transcode's with a quarter to spare (its encoder's peaks),
/// a remux's from the source (40 Mbps when nobody knows).
pub fn reckoned_bytes(plan: &Plan, media: &MediaInfo) -> u64 {
    let stream_kbps = |index: i64| media.stream(index).and_then(|s| s.bitrate).filter(|b| *b > 0).map(|b| b / 1000);
    let video = match &plan.video {
        Some(v) if v.copy => {
            stream_kbps(v.stream_index).or(media.bitrate.filter(|b| *b > 0).map(|b| b / 1000)).unwrap_or(40_000).max(0)
                as u64
        }
        Some(v) => u64::from(v.bitrate_kbps) * 5 / 4,
        None => 0,
    };
    let audio = match &plan.audio {
        Some(a) if a.copy => stream_kbps(a.stream_index).unwrap_or(1_536).max(0) as u64,
        Some(a) => u64::from(a.bitrate_kbps),
        None => 0,
    };
    (video + audio) * 1000 / 8 * RECKONED.as_secs()
}

/// The folder in RAM the transcoder keeps segments in, with its budget.
pub struct MemoryStore {
    /// The folder in RAM (`/dev/shm`).
    base: PathBuf,
    /// The transcoder's own folder in it.
    root: PathBuf,
    budget: u64,
    measure: Box<dyn Fn() -> Option<Room> + Send + Sync>,
    /// Bytes reserved by jobs holding a [`Lease`].
    reserved: Mutex<u64>,
    /// Bytes of segments in it now.
    used: AtomicU64,
}

/// A job's reservation: given back when dropped.
pub struct Lease {
    store: Arc<MemoryStore>,
    bytes: u64,
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut reserved = self.store.reserved.lock();
        *reserved = reserved.saturating_sub(self.bytes);
    }
}

impl MemoryStore {
    /// This system's folder in RAM, or why segments can't go to memory here.
    pub fn open() -> Result<Arc<Self>, String> {
        #[cfg(target_os = "linux")]
        {
            let base = Path::new("/dev/shm");
            if !base.is_dir() {
                return Err(format!("{} isn't there", base.display()));
            }
            if !linux::is_tmpfs(base) {
                return Err(format!("{} isn't in memory", base.display()));
            }
            linux::remove_stale(base);
            let folder = base.to_path_buf();
            Self::new(base, Box::new(move || linux::measure(&folder)))
        }
        #[cfg(not(target_os = "linux"))]
        {
            // Windows and macOS have no folder in RAM by default. FFmpeg
            // writes the segments, so they can't be told to stay in the
            // file cache (FILE_ATTRIBUTE_TEMPORARY), and a RAM disk needs a
            // driver (Windows) or stays behind when the transcoder doesn't
            // (macOS's hdiutil).
            Err("works on Linux only".into())
        }
    }

    /// A store in `base`, a folder in RAM, whose room `measure` tells:
    /// the transcoder's folder in it is made (private to its user).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub fn new(base: &Path, measure: Box<dyn Fn() -> Option<Room> + Send + Sync>) -> Result<Arc<Self>, String> {
        let room = measure().ok_or_else(|| format!("{}'s room can't be told", base.display()))?;
        let budget = budget(&room);
        if budget < MIN_BUDGET {
            let short = if room.size / 2 < MIN_BUDGET {
                format!("{} holds only {} MB", base.display(), room.size / MIB)
            } else {
                format!("only {} MB of memory", room.memory / MIB)
            };
            return Err(short);
        }
        let root = base.join(format!("{PREFIX}{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        make_private_dir(&root).map_err(|err| format!("{} can't be written: {err}", base.display()))?;
        Ok(Arc::new(Self {
            base: base.to_path_buf(),
            root,
            budget,
            measure,
            reserved: Mutex::new(0),
            used: AtomicU64::new(0),
        }))
    }

    /// What `/v1/info` says of memory, from what [`MemoryStore::open`] gave.
    pub fn info(opened: &Result<Arc<Self>, String>) -> MemoryInfo {
        match opened {
            Ok(store) => MemoryInfo {
                available: true,
                reason: None,
                path: Some(store.base.to_string_lossy().into_owned()),
                budget_bytes: store.budget,
            },
            Err(reason) => MemoryInfo { available: false, reason: Some(reason.clone()), ..Default::default() },
        }
    }

    /// The transcoder's folder in RAM, removed at shutdown.
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn budget(&self) -> u64 {
        self.budget
    }

    /// Bytes of segments in memory now.
    pub fn used(&self) -> u64 {
        self.used.load(Relaxed)
    }

    #[cfg(test)]
    pub fn reserved(&self) -> u64 {
        *self.reserved.lock()
    }

    /// A reservation of `want` bytes for a job's segments, when they fit
    /// ([`admits`]); `None`: its segments go to disk.
    pub fn lease(self: &Arc<Self>, want: u64) -> Option<Lease> {
        let room = (self.measure)()?;
        let mut reserved = self.reserved.lock();
        let committed = (*reserved).max(self.used());
        if !admits(committed, want, self.budget, &room) {
            return None;
        }
        *reserved += want;
        Some(Lease { store: self.clone(), bytes: want })
    }

    /// Whether memory runs short ([`short`]): a job moves to disk.
    pub fn short(&self) -> bool {
        (self.measure)().is_none_or(|room| short(self.used(), self.budget, &room))
    }

    /// A segment of `bytes` went to memory.
    pub fn added(&self, bytes: u64) {
        self.used.fetch_add(bytes, Relaxed);
    }

    /// Segments of `bytes` left memory.
    pub fn removed(&self, bytes: u64) {
        let _ = self.used.try_update(Relaxed, Relaxed, |used| Some(used.saturating_sub(bytes)));
    }
}

/// Makes `dir`, a folder only this account may use. In a folder every
/// account writes to (`/dev/shm`) the name may already be taken by another
/// account's folder, or a link to one, which the transcoder can't remove:
/// that is refused rather than used, or its owner could read the segments,
/// swap them, or have FFmpeg write through links planted in it.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn make_private_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, MetadataExt};
        std::fs::DirBuilder::new().mode(0o700).create(dir).map_err(|err| {
            if err.kind() == std::io::ErrorKind::AlreadyExists {
                std::io::Error::new(err.kind(), format!("{} is another account's", dir.display()))
            } else {
                err
            }
        })?;
        // Made by this call, so this account's (in a sticky folder nobody
        // else can rename it), but checked all the same: a folder, with
        // nobody else's rights.
        let made = std::fs::symlink_metadata(dir)?;
        if !made.file_type().is_dir() || made.mode() & 0o077 != 0 {
            return Err(std::io::Error::other(format!("{} isn't private to this account", dir.display())));
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// `MemTotal` and `MemAvailable` of `/proc/meminfo`, in bytes.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_meminfo(text: &str) -> Option<(u64, u64)> {
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix(':'))
            .and_then(|rest| rest.trim().strip_suffix("kB")?.trim().parse::<u64>().ok())
            .map(|kb| kb * 1024)
    };
    Some((field("MemTotal")?, field("MemAvailable")?))
}

/// Memory the transcoder can still take without swapping, in bytes: Linux's
/// `MemAvailable`, or what its control group's memory limit leaves (a
/// container's `--memory`, systemd's `MemoryMax=`), whichever is less;
/// Windows' available physical memory. `None` elsewhere, and where it can't
/// be read. Blocking.
pub fn available() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let read = |path: &str| std::fs::read_to_string(path).ok();
        available_on_linux(
            read("/proc/meminfo").as_deref(),
            read("/proc/self/cgroup").as_deref(),
            Path::new("/sys/fs/cgroup"),
        )
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
        let mut status = MEMORYSTATUSEX { dwLength: size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
        // SAFETY: plain Win32 call filling a struct of our own, told its size.
        (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status.ullAvailPhys)
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        None
    }
}

/// [`available`] on Linux, from `/proc/meminfo`, `/proc/self/cgroup` and the
/// control groups mounted at `root`: the process's group of cgroup v1's
/// `memory` controller where it has one, else its v2 group.
#[cfg(any(target_os = "linux", test))]
fn available_on_linux(meminfo: Option<&str>, groups: Option<&str>, root: &Path) -> Option<u64> {
    use crate::cpuload::group_path;
    let computer = meminfo.and_then(parse_meminfo).map(|(_, available)| available);
    let group = groups.and_then(|groups| match group_path(groups, Some("memory")) {
        Some(group) => group_room(&root.join("memory"), group, false),
        None => group_room(root, group_path(groups, None)?, true),
    });
    computer.into_iter().chain(group).min()
}

/// A control group's memory limit this high is none: cgroup v1 says "no
/// limit" with one just below 2^63.
#[cfg(any(target_os = "linux", test))]
const NO_LIMIT: u64 = 1 << 62;

/// What the tightest memory limit of `group` (a path below `mount`) and the
/// groups above it leaves, in bytes: the limit less what the group uses, the
/// page cache it can drop not counted. cgroup v2: `memory.max` (`max` for
/// none) less `memory.current` less `memory.stat`'s `inactive_file`; v1:
/// `memory.limit_in_bytes` less `memory.usage_in_bytes` less
/// `total_inactive_file`. `None` without a limit.
#[cfg(any(target_os = "linux", test))]
fn group_room(mount: &Path, group: &str, v2: bool) -> Option<u64> {
    let (limit, usage, inactive) = if v2 {
        ("memory.max", "memory.current", "inactive_file")
    } else {
        ("memory.limit_in_bytes", "memory.usage_in_bytes", "total_inactive_file")
    };
    let read = |dir: &Path, name: &str| std::fs::read_to_string(dir.join(name)).ok();
    let room = crate::cpuload::tightest(mount, group, |dir| {
        let limit = read(dir, limit)?.trim().parse::<u64>().ok().filter(|&limit| limit < NO_LIMIT)?;
        let usage = read(dir, usage)?.trim().parse::<u64>().ok()?;
        // The page cache of files not read lately, which the kernel drops
        // before the group runs short (reading whole files fills it).
        let droppable = read(dir, "memory.stat")
            .and_then(|stat| {
                stat.lines().find_map(|line| line.strip_prefix(inactive)?.strip_prefix(' ')?.trim().parse::<u64>().ok())
            })
            .unwrap_or(0);
        Some(limit.saturating_sub(usage.saturating_sub(droppable)))
    });
    room.map(|(_, room)| room)
}

/// The process id in the name of a store's folder (`pley-transcoder-1234`).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn owner_pid(name: &str) -> Option<u32> {
    name.strip_prefix(PREFIX)?.parse().ok()
}

#[cfg(target_os = "linux")]
mod linux {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    use super::{Room, owner_pid, parse_meminfo};

    const TMPFS_MAGIC: i64 = 0x0102_1994;

    fn statfs(dir: &Path) -> Option<libc::statfs> {
        let path = CString::new(dir.as_os_str().as_bytes()).ok()?;
        let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
        // SAFETY: a NUL-terminated path and a statfs to fill.
        (unsafe { libc::statfs(path.as_ptr(), &mut stat) } == 0).then_some(stat)
    }

    /// Whether `dir` is on a tmpfs (ramfs has no size, so no room to tell).
    // The field types differ between C libraries and architectures.
    #[allow(clippy::unnecessary_cast)]
    pub fn is_tmpfs(dir: &Path) -> bool {
        statfs(dir).is_some_and(|s| s.f_type as i64 == TMPFS_MAGIC)
    }

    #[allow(clippy::unnecessary_cast)]
    pub fn measure(dir: &Path) -> Option<Room> {
        let s = statfs(dir)?;
        let block = s.f_bsize as u64;
        let (memory, available) = parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?)?;
        Some(Room { size: s.f_blocks as u64 * block, free: s.f_bavail as u64 * block, memory, available })
    }

    /// Removes what transcoders gone (a crash) left in `base`: a tmpfs
    /// keeps it in memory until then.
    pub fn remove_stale(base: &Path) {
        let Ok(entries) = std::fs::read_dir(base) else { return };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(pid) = owner_pid(&name.to_string_lossy()) else { continue };
            if pid != std::process::id() && !Path::new(&format!("/proc/{pid}")).exists() {
                match std::fs::remove_dir_all(entry.path()) {
                    Ok(()) => {
                        tracing::info!("removed segments left in {} by a transcoder gone", entry.path().display())
                    }
                    Err(err) => tracing::debug!("could not remove {}: {err}", entry.path().display()),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_transcoder_api::{MediaStream, StreamKind};

    /// A name another account took first in a folder everyone writes to
    /// (`/dev/shm`): neither its folder nor a link is used.
    #[cfg(unix)]
    #[test]
    fn a_folder_or_link_already_there_is_never_used() {
        use std::os::unix::fs::PermissionsExt;
        let base = tempfile::tempdir().unwrap();
        let taken = base.path().join("taken");
        std::fs::create_dir(&taken).unwrap();
        std::fs::set_permissions(&taken, std::fs::Permissions::from_mode(0o777)).unwrap();
        assert!(make_private_dir(&taken).is_err(), "a folder already there");
        assert_eq!(std::fs::metadata(&taken).unwrap().permissions().mode() & 0o777, 0o777);

        let elsewhere = base.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let link = base.path().join("link");
        std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
        assert!(make_private_dir(&link).is_err(), "a link to a folder");

        let fresh = base.path().join("fresh");
        make_private_dir(&fresh).unwrap();
        assert_eq!(std::fs::metadata(&fresh).unwrap().permissions().mode() & 0o777, 0o700);
    }

    const GIB: u64 = 1024 * MIB;

    /// A computer with 16 GB, half of it /dev/shm, `free` of that free and
    /// `available` of its memory.
    fn room(free: u64, available: u64) -> Room {
        Room { size: 8 * GIB, free, memory: 16 * GIB, available }
    }

    #[test]
    fn the_budget_is_a_quarter_of_memory_at_most() {
        assert_eq!(budget(&room(8 * GIB, 12 * GIB)), 4 * GIB);
        // Docker's 64 MB /dev/shm.
        let docker = Room { size: 64 * MIB, ..room(64 * MIB, 12 * GIB) };
        assert_eq!(budget(&docker), 32 * MIB);
    }

    #[test]
    fn a_job_goes_to_memory_while_it_fits() {
        let fine = room(8 * GIB, 12 * GIB);
        assert!(admits(0, GIB, 4 * GIB, &fine));
        assert!(admits(3 * GIB, GIB, 4 * GIB, &fine), "exactly the budget");
        assert!(!admits(3 * GIB + 1, GIB, 4 * GIB, &fine), "past the budget: to disk");
        // The folder full (other programs' files too).
        assert!(!admits(0, GIB, 4 * GIB, &room(GIB, 12 * GIB)));
        assert!(admits(0, GIB, 4 * GIB, &room(GIB + FOLDER_MARGIN, 12 * GIB)));
        // The computer short of memory: 1.6 GB is kept for the rest of it.
        assert!(!admits(0, GIB, 4 * GIB, &room(8 * GIB, 2 * GIB)));
        assert!(admits(0, GIB, 4 * GIB, &room(8 * GIB, 3 * GIB)));
    }

    #[test]
    fn memory_runs_short_past_the_budget_or_the_room() {
        let fine = room(8 * GIB, 12 * GIB);
        assert!(!short(4 * GIB, 4 * GIB, &fine));
        assert!(short(4 * GIB + 1, 4 * GIB, &fine), "segments past the budget");
        assert!(short(0, 4 * GIB, &room(MIB, 12 * GIB)), "the folder full");
        assert!(short(0, 4 * GIB, &room(8 * GIB, GIB)), "the computer short of memory");
        // A small computer keeps 512 MB.
        let small = Room { size: GIB, free: GIB, memory: 2 * GIB, available: 600 * MIB };
        assert!(!short(0, budget(&small), &small));
        assert!(short(0, budget(&small), &Room { available: 500 * MIB, ..small }));
    }

    #[test]
    fn reckons_with_the_bitrate() {
        use crate::plan::{AudioOut, Pipeline, VideoOut};
        let video = |copy: bool| VideoOut {
            stream_index: 0,
            copy,
            encoder: None,
            codec: pley_transcoder_api::VideoCodec::H264,
            src_width: 3840,
            src_height: 2160,
            width: 3840,
            height: 2160,
            bitrate_kbps: 20_000,
            tonemap: None,
            tonemap_quality: Default::default(),
            tonemap_input: None,
            libplacebo_asked: false,
            amf_upload: false,
            vui_primaries: None,
            hw_decode: None,
            gpu_scale: None,
            deinterlace: false,
            preset: String::new(),
            pipeline: Pipeline::Software,
            dolby_vision: None,
            strip_hdr10plus: false,
            hdr: None,
        };
        let audio = AudioOut {
            stream_index: 1,
            copy: false,
            codec: pley_transcoder_api::AudioCodec::Aac,
            channels: 2,
            bitrate_kbps: 192,
            resample: false,
            partial_decode: None,
            downmix: None,
        };
        let mut plan = Plan {
            duration_ms: 60_000,
            boundaries: vec![0, 60_000],
            split_at: vec![0, 60_000],
            seek_at: vec![0, 60_000],
            segment_seconds: 4.0,
            video: Some(video(false)),
            audio: Some(audio),
            burn: None,
            throttle: true,
            catchup: false,
            warnings: vec![],
        };
        let media = MediaInfo::default();
        // 4K at 20 Mbps with a quarter to spare, and AAC: 25.192 Mbps for 3 minutes.
        assert_eq!(reckoned_bytes(&plan, &media), 25_192 * 1000 / 8 * 180);
        // A remux: the source's bitrate, else 40 Mbps.
        plan.video = Some(video(true));
        assert_eq!(reckoned_bytes(&plan, &media), 40_192 * 1000 / 8 * 180);
        let stream = |kind, bitrate| MediaStream { kind, bitrate: Some(bitrate), ..Default::default() };
        let media = MediaInfo {
            streams: vec![stream(StreamKind::Video, 60_000_000), stream(StreamKind::Audio, 640_000)],
            ..Default::default()
        };
        let reckoned = reckoned_bytes(&plan, &media);
        assert_eq!(reckoned, 60_192 * 1000 / 8 * 180, "{} MB", reckoned / MIB);
    }

    fn store(dir: &Path, room: Arc<Mutex<Room>>) -> Result<Arc<MemoryStore>, String> {
        MemoryStore::new(dir, Box::new(move || Some(*room.lock())))
    }

    #[test]
    fn leases_reserve_until_they_go() {
        let dir = tempfile::tempdir().unwrap();
        let measured = Arc::new(Mutex::new(room(8 * GIB, 12 * GIB)));
        let store = store(dir.path(), measured.clone()).unwrap();
        assert!(store.root().is_dir() && store.root().starts_with(dir.path()));
        assert_eq!(store.budget(), 4 * GIB);
        let info = MemoryStore::info(&Ok(store.clone()));
        assert!(info.available && info.reason.is_none() && info.budget_bytes == 4 * GIB);

        let first = store.lease(3 * GIB).unwrap();
        assert!(store.lease(2 * GIB).is_none(), "past the budget");
        let second = store.lease(GIB).unwrap();
        assert_eq!(store.reserved(), 4 * GIB);
        drop(first);
        assert_eq!(store.reserved(), GIB);
        // Segments in memory count, when more than reserved.
        store.added(3 * GIB + GIB / 2);
        assert!(store.lease(GIB).is_none());
        store.removed(GIB);
        assert!(store.lease(GIB).is_some());
        store.removed(100 * GIB);
        assert_eq!(store.used(), 0, "never below nothing");
        drop(second);

        assert!(!store.short());
        measured.lock().available = GIB;
        assert!(store.short(), "the computer short of memory");
        assert!(store.lease(MIB).is_none());
    }

    #[test]
    fn too_little_room_is_no_memory() {
        let dir = tempfile::tempdir().unwrap();
        let docker = Room { size: 64 * MIB, free: 64 * MIB, memory: 16 * GIB, available: 12 * GIB };
        let err = store(dir.path(), Arc::new(Mutex::new(docker))).err().unwrap();
        assert!(err.ends_with("holds only 64 MB"), "{err}");
        let tiny = Room { size: 512 * MIB, free: 512 * MIB, memory: 512 * MIB, available: 256 * MIB };
        assert_eq!(store(dir.path(), Arc::new(Mutex::new(tiny))).err().unwrap(), "only 512 MB of memory");
        let info = MemoryStore::info(&Err("works on Linux only".into()));
        assert_eq!((info.available, info.reason.as_deref()), (false, Some("works on Linux only")));
    }

    #[test]
    fn reads_meminfo() {
        let text = "MemTotal:       16303148 kB\nMemFree:         1084128 kB\nMemAvailable:   11873912 kB\n";
        assert_eq!(parse_meminfo(text), Some((16_303_148 * 1024, 11_873_912 * 1024)));
        assert_eq!(parse_meminfo("MemTotal: 1 kB\n"), None, "kernels before 3.14 have no MemAvailable");
        assert_eq!(owner_pid("pley-transcoder-1234"), Some(1234));
        assert_eq!(owner_pid("pley-transcoder-x"), None);
        assert_eq!(owner_pid("pulse-shm-1"), None);
    }

    /// A control group's files in `dir`.
    fn group(dir: &Path, files: &[(&str, String)]) {
        std::fs::create_dir_all(dir).unwrap();
        for (name, text) in files {
            std::fs::write(dir.join(name), text).unwrap();
        }
    }

    /// A cgroup v2 group's limit, use, and page cache it can drop.
    fn v2(dir: &Path, max: &str, current: u64, inactive_file: u64) {
        let stat = format!("anon 4096\nfile 8192\ninactive_anon 4096\ninactive_file {inactive_file}\n");
        group(
            dir,
            &[("memory.max", format!("{max}\n")), ("memory.current", format!("{current}\n")), ("memory.stat", stat)],
        );
    }

    #[test]
    fn available_memory_counts_the_control_groups_limit() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path();
        // 4 GiB, of which 3.5 are used, 1 of that by files' pages it can drop.
        v2(&root.join("a/b"), &(4 * GIB).to_string(), 3 * GIB + GIB / 2, GIB);
        assert_eq!(group_room(root, "/a/b", true), Some(GIB + GIB / 2));
        v2(&root.join("a/b"), "max", 3 * GIB, 0);
        assert_eq!(group_room(root, "/a/b", true), None, "no limit");
        // A group above tighter than its own wins.
        v2(&root.join("a"), &(2 * GIB).to_string(), GIB + GIB / 2, 0);
        v2(&root.join("a/b"), &(8 * GIB).to_string(), GIB, 0);
        assert_eq!(group_room(root, "/a/b", true), Some(GIB / 2));
        // A container's own group is its root, whatever the path.
        v2(root, &(3 * GIB).to_string(), GIB, 0);
        assert_eq!(group_room(root, "/docker/abc", true), Some(2 * GIB));

        // cgroup v1: no limit is a number just below 2^63.
        let v1 = root.join("memory");
        let limits = |dir: &Path, limit: u64, usage: u64, inactive: u64| {
            // The group's own and, with those below it, its total.
            let stat = format!("cache 8192\ninactive_file 4096\ntotal_inactive_file {inactive}\n");
            group(
                dir,
                &[
                    ("memory.limit_in_bytes", format!("{limit}\n")),
                    ("memory.usage_in_bytes", format!("{usage}\n")),
                    ("memory.stat", stat),
                ],
            )
        };
        limits(&v1, 9_223_372_036_854_771_712, 10 * GIB, 0);
        assert_eq!(group_room(&v1, "/", false), None);
        limits(&v1.join("docker/abc"), 3 * GIB, 2 * GIB + GIB / 2, GIB);
        assert_eq!(group_room(&v1, "/docker/abc", false), Some(GIB + GIB / 2));

        // The computer's MemAvailable, or the group's room where that is less.
        let meminfo =
            |available: u64| format!("MemTotal: {} kB\nMemAvailable: {} kB\n", 64 * GIB / 1024, available / 1024);
        let (ten, half) = (meminfo(10 * GIB), meminfo(GIB / 2));
        v2(&root.join("c"), &(4 * GIB).to_string(), 3 * GIB, 0);
        let in_v2 = Some("0::/c\n");
        assert_eq!(available_on_linux(Some(&ten), in_v2, root), Some(GIB));
        assert_eq!(available_on_linux(Some(&half), in_v2, root), Some(GIB / 2));
        let in_v1 = Some("5:cpuacct,cpu:/docker/abc\n4:memory:/docker/abc\n0::/\n");
        assert_eq!(available_on_linux(Some(&ten), in_v1, root), Some(GIB + GIB / 2));
        assert_eq!(available_on_linux(Some(&ten), None, root), Some(10 * GIB), "no group known");
        assert_eq!(available_on_linux(None, in_v2, root), Some(GIB), "no MemAvailable (kernels before 3.14)");
        assert_eq!(available_on_linux(None, None, root), None);
    }

    #[test]
    fn this_computers_memory_is_told_where_it_can_be() {
        if cfg!(any(windows, target_os = "linux")) {
            assert!(available().is_some_and(|bytes| bytes > 0), "{:?}", available());
        } else {
            assert_eq!(available(), None);
        }
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn only_linux_has_a_folder_in_ram() {
        assert_eq!(MemoryStore::open().err().as_deref(), Some("works on Linux only"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn dev_shm_is_measured() {
        let base = Path::new("/dev/shm");
        if !base.is_dir() || !linux::is_tmpfs(base) {
            return;
        }
        let room = linux::measure(base).unwrap();
        assert!(room.size > 0 && room.free <= room.size && room.memory > 0 && room.available <= room.memory);
        // What a transcoder gone left goes; a live one's stays.
        let dead = base.join(format!("{PREFIX}{}", u32::MAX - 1));
        let live = base.join(format!("{PREFIX}1"));
        std::fs::create_dir_all(&dead).unwrap();
        let made_live = std::fs::create_dir(&live).is_ok();
        linux::remove_stale(base);
        assert!(!dead.exists());
        if made_live {
            assert!(live.exists(), "pid 1 is alive");
            std::fs::remove_dir(&live).unwrap();
        }
    }
}

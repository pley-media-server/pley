//! Seek-bar previews ("trickplay", Plex's video preview thumbnails) and the
//! chapter images cut from them. The transcoder makes the sprite sheets; they
//! are kept in the file's media folder ([`crate::store`]), so they follow the
//! file when it is moved or renamed. They are made in the background: the
//! titles people open (unless something is being transcoded, a playback is
//! starting, or the title itself is playing), and whole libraries after
//! scans when the setting allows, only while nothing plays: what is likely
//! to be watched soon first, the rest at the hours [`crate::schedule`] gives
//! library work. Where the transcoder makes several sets at once, a title
//! someone opens doesn't wait for the libraries' files, and those go one per
//! disk, more at once in the hours people rarely watch ([`lanes`] has the
//! rules). A playback that starts while previews are being made stops them;
//! they are made again later, in the order they were in. A download being
//! converted meanwhile pauses one of them (the transcoder holds its FFmpeg
//! until the conversion is done) and puts off the others.
//!
//! Chapter images and Plex's BIF index are cut from the sprite sheets, so
//! they cost no extra decoding of the video. The BIF index, larger than the
//! sheets it is cut from, is kept in the cache, for the titles Plex apps
//! played lately, rather than beside them.

mod lanes;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::{Duration, Instant};

use anyhow::Context;
use image::codecs::jpeg::JpegEncoder;
use parking_lot::Mutex;
use pley_transcoder_api::{Load, TrickplayInfo};
use tokio::task::{JoinError, JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

use crate::catalog::{self, FileRow};
use crate::db::Db;
use crate::disks::{Disk, Disks};
use crate::playback::Sessions;
use crate::schedule::{Class, Scheduler, Soon};
use crate::settings::SettingsStore;
use crate::store::Store;
use crate::transcoder::{TRANSCODING_LOOK, Transcoder};
use crate::util::now;
use lanes::{Narrow, Outcome, RunView, Why};

/// How often a waiting worker looks again.
const IDLE_LOOK: Duration = Duration::from_secs(15);
/// Quality of chapter images and BIF frames cut from the sheets.
const JPEG_QUALITY: u8 = 82;
/// A chapter's image is taken this far into it (or 20 % of it, if shorter),
/// past black frames and fades at the cut.
const CHAPTER_OFFSET_MS: i64 = 4_000;
/// Files whose chapter images are cut at once after their previews are made
/// (a decoded sheet each); those someone asks for don't wait.
const CUTS_AT_ONCE: usize = 2;
/// Files whose storage is looked up at once at most: a share that doesn't
/// answer holds a blocking thread for each, and no more than these.
const LOOKUPS_AT_ONCE: usize = 4;
/// How long the libraries' files wait for a lookup of their storage and for
/// its answer (a share's disks may take a while to spin up). One that
/// doesn't answer in time leaves the files after it alone, until the next
/// scan asks again: they are likely on the same share.
const LIBRARY_LOOKUP: Duration = Duration::from_secs(60);
/// How long a title someone opened waits for a lookup of its storage to
/// start, all lookups taken by shares that don't answer; then it goes
/// alone. One that has started is waited for: a title on a share that
/// doesn't answer couldn't be made anyway.
const OPENED_LOOKUP: Duration = Duration::from_secs(5);
/// How long scans go without reading the versions of the previews kept
/// while the transcoder's stays the same ([`versions_due`]).
const VERSIONS_AGAIN: Duration = Duration::from_secs(24 * 60 * 60);
/// The settings row saying the BIF indexes earlier versions kept beside the
/// previews are gone.
const OLD_BIFS_REMOVED: &str = "old_bifs_removed";

pub struct TrickplayService {
    db: Db,
    transcoder: Arc<Transcoder>,
    settings: Arc<SettingsStore>,
    sessions: Arc<Sessions>,
    schedule: Arc<Scheduler>,
    store: Arc<Store>,
    /// Where media files are read from: one disk isn't read for two files at once.
    disks: Arc<Disks>,
    /// Lookups of where files are read from ([`LOOKUPS_AT_ONCE`]).
    lookups: Arc<tokio::sync::Semaphore>,
    pace: Pace,
    /// Plex's BIF indexes (`<data>/cache/bif`).
    bif_cache: PathBuf,
    /// Files waiting, and those being made.
    queues: Mutex<Queues>,
    /// A file was queued.
    queued: tokio::sync::Notify,
    /// File versions that could not be done (by fingerprint); retried after a restart.
    failed: Mutex<HashMap<i64, String>>,
    /// The transcoder's version of previews when a scan last read those
    /// kept, and when ([`versions_due`]): in memory, so that a start reads
    /// them again.
    versions_read: Mutex<Option<(u32, Instant)>>,
    /// Fingerprints being made (see [`Claim`]).
    making: Mutex<HashSet<Arc<str>>>,
    /// One chapter-image or BIF job per media folder at a time, by fingerprint.
    cutting: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// Chapter images cut as previews are made ([`CUTS_AT_ONCE`]).
    cuts: tokio::sync::Semaphore,
    /// Cancelled when the server shuts down: the worker stops.
    stopping: CancellationToken,
}

/// How long the worker waits between its steps (tests shorten it).
#[derive(Debug, Clone, Copy)]
pub struct Pace {
    /// Between the starts of the libraries' files beyond the first.
    pub settle: Duration,
    /// How long the disk of a title someone opened takes no library file after.
    pub opened_grace: Duration,
    /// How often the transcoder is asked how it is doing while files are made.
    pub look: Duration,
}

impl Default for Pace {
    fn default() -> Self {
        Pace { settle: lanes::SETTLE, opened_grace: lanes::OPENED_GRACE, look: TRANSCODING_LOOK }
    }
}

/// A file waiting for previews.
#[derive(Debug, Clone)]
struct Entry {
    id: i64,
    /// Its contents' fingerprint, where known: files that share one share
    /// their previews, made once.
    fingerprint: Option<Arc<str>>,
}

/// A file being made.
struct Run {
    kind: Kind,
    disk: Disk,
    /// Its order among the files started.
    seq: u64,
    /// Puts it off.
    cancel: CancellationToken,
    /// Why it was put off, once it is.
    why: Option<Why>,
}

#[derive(Default)]
struct Queues {
    opened: VecDeque<Entry>,
    library: VecDeque<Entry>,
    /// Files whose previews an earlier version made: made again last, in
    /// the hours for the rest of the libraries (those there serve until then).
    refresh: VecDeque<Entry>,
    /// Files being made, by id. They keep their place in their queue
    /// meanwhile, so those put off are made again in the order they were in.
    running: HashMap<i64, Run>,
    /// Files queued or being made.
    pending: HashSet<i64>,
    /// Where the files queued are read from, as far as it is known yet: a
    /// file not in it goes alone.
    disks: HashMap<i64, Disk>,
    /// Files someone opened whose storage is being looked up: once at a
    /// time, however often their player asks.
    looking: HashSet<i64>,
}

/// Where a file whose storage isn't known is read from: it goes alone.
static UNKNOWN_DISK: LazyLock<Disk> = LazyLock::new(unknown_disk);

/// The queue a file was taken from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Opened,
    Library,
    Refresh,
}

impl Queues {
    /// Where file `id` is read from.
    fn disk(&self, id: i64) -> &Disk {
        self.disks.get(&id).unwrap_or(&UNKNOWN_DISK)
    }

    fn entry_mut(&mut self, id: i64) -> Option<&mut Entry> {
        self.opened.iter_mut().chain(self.library.iter_mut()).chain(self.refresh.iter_mut()).find(|e| e.id == id)
    }

    /// A file someone opened that waits for the libraries' turn goes with
    /// the titles opened, with what is known of it; one being made goes on,
    /// as theirs. Whether it moved.
    fn open(&mut self, id: i64) -> bool {
        if self.opened.iter().any(|e| e.id == id) {
            return false;
        }
        let taken = [&mut self.library, &mut self.refresh].into_iter().find_map(|queue| {
            let at = queue.iter().position(|e| e.id == id)?;
            queue.remove(at)
        });
        let Some(entry) = taken else { return false };
        self.opened.push_back(entry);
        if let Some(run) = self.running.get_mut(&id) {
            run.kind = Kind::Opened;
        }
        true
    }

    /// Done with a file, made or not: out of its queue, no longer pending.
    fn forget(&mut self, id: i64) {
        for queue in [&mut self.opened, &mut self.library, &mut self.refresh] {
            if let Some(at) = queue.iter().position(|e| e.id == id) {
                queue.remove(at);
            }
        }
        self.pending.remove(&id);
        self.disks.remove(&id);
    }

    /// Puts off file `id` if it is being made: its request is dropped, which
    /// stops the transcoder's FFmpeg.
    fn put_off(&mut self, id: i64, why: Why) {
        if let Some(run) = self.running.get_mut(&id).filter(|run| run.why.is_none()) {
            run.why = Some(why);
            run.cancel.cancel();
        }
    }

    /// The files being made and not put off.
    fn views(&self) -> Vec<RunView<'_>> {
        let going = self.running.iter().filter(|(_, run)| run.why.is_none());
        going.map(|(&id, run)| RunView { id, kind: run.kind, disk: &run.disk, seq: run.seq }).collect()
    }

    /// Whether any file may start, as far as the queues tell: before the
    /// transcoder is asked how it is doing.
    fn could_start(&self, playing: &HashSet<i64>, library_turn: bool, capacity: u32) -> bool {
        let views = self.views();
        let free = |e: &&Entry| !self.running.contains_key(&e.id);
        let opened = !views.iter().any(|r| r.kind == Kind::Opened)
            && (capacity >= 2 || views.is_empty())
            && self.opened.iter().filter(free).any(|e| !playing.contains(&e.id));
        let library = library_turn
            && (views.len() as u32) < capacity
            && self.library.iter().chain(&self.refresh).any(|e| free(&e));
        opened || library
    }
}

/// A fingerprint being made, given up when dropped: files with the same
/// contents (one file in two libraries) share their previews.
struct Claim<'a> {
    making: &'a Mutex<HashSet<Arc<str>>>,
    fingerprint: Arc<str>,
}

impl<'a> Claim<'a> {
    fn take(making: &'a Mutex<HashSet<Arc<str>>>, fingerprint: &str) -> Option<Self> {
        let fingerprint: Arc<str> = Arc::from(fingerprint);
        // Made only once claimed: dropping a claim gives it up.
        if !making.lock().insert(fingerprint.clone()) {
            return None;
        }
        Some(Claim { making, fingerprint })
    }
}

impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.making.lock().remove(&self.fingerprint);
    }
}

/// The transcoder's answer to how it is doing.
#[derive(Debug, Clone, Copy)]
struct Look {
    at: Instant,
    /// A playback's FFmpeg or a download's runs.
    transcoding: bool,
    load: Option<Load>,
}

/// What the worker keeps from one round to the next.
#[derive(Default)]
struct Rounds {
    look: Option<Look>,
    /// Files started so far.
    seq: u64,
    /// The file each task makes.
    tasks: HashMap<tokio::task::Id, i64>,
    /// When the last of the libraries' files started.
    library_start: Option<Instant>,
    /// The disk of the title someone opened last, and until when it rests.
    grace: Option<(Disk, Instant)>,
    /// Nothing starts until then: the transcoder said "not now".
    hold: Option<Instant>,
    /// The most of the libraries' files made at once since they last all ended.
    widest: usize,
    /// Why no more of them are made at once, as last said.
    narrow: Option<Narrow>,
}

/// Where a file whose disk couldn't be asked is read from: it goes alone.
fn unknown_disk() -> Disk {
    Disk { key: Arc::from("unknown"), width: 0 }
}

/// `work`, which blocks, started off the async workers with one of
/// `permits` if one is free within `wait` (`None` if not). It keeps its
/// permit until it ends, so that no more than the permits ever block at
/// once, however long they wait.
async fn bounded<T: Send + 'static>(
    permits: Arc<tokio::sync::Semaphore>,
    wait: Duration,
    work: impl FnOnce() -> T + Send + 'static,
) -> Option<JoinHandle<T>> {
    let permit = tokio::time::timeout(wait, permits.acquire_owned()).await.ok()?.ok()?;
    Some(tokio::task::spawn_blocking(move || {
        let _permit = permit;
        work()
    }))
}

/// What `TrickplayInfo` says about a tile: its sheet and position.
fn tile(info: &TrickplayInfo, n: u32) -> (u32, u32, u32) {
    let per_sheet = (info.columns * info.rows).max(1);
    let in_sheet = n % per_sheet;
    (n / per_sheet, (in_sheet % info.columns) * info.width, (in_sheet / info.columns) * info.height)
}

/// The tile shown at `ms` (the last one past the end).
fn tile_at(info: &TrickplayInfo, ms: i64) -> u32 {
    let n = (ms.max(0) / info.interval_ms.max(1)) as u32;
    n.min(info.count.saturating_sub(1))
}

fn encode_jpeg(img: &image::RgbImage) -> anyhow::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(img.as_raw().len() / 8);
    JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).encode_image(img)?;
    Ok(out)
}

impl TrickplayService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Db,
        transcoder: Arc<Transcoder>,
        settings: Arc<SettingsStore>,
        sessions: Arc<Sessions>,
        schedule: Arc<Scheduler>,
        store: Arc<Store>,
        disks: Arc<Disks>,
        pace: Pace,
        cache: PathBuf,
        stopping: CancellationToken,
    ) -> Arc<Self> {
        let service = Arc::new(Self {
            db,
            transcoder,
            settings,
            sessions,
            schedule,
            store,
            disks,
            lookups: Arc::new(tokio::sync::Semaphore::new(LOOKUPS_AT_ONCE)),
            pace,
            bif_cache: cache.join("bif"),
            queues: Mutex::default(),
            queued: tokio::sync::Notify::new(),
            failed: Mutex::default(),
            versions_read: Mutex::default(),
            making: Mutex::default(),
            cutting: Mutex::default(),
            cuts: tokio::sync::Semaphore::new(CUTS_AT_ONCE),
            stopping,
        });
        // Earlier versions kept previews in the cache, keyed by file id.
        let legacy_cache = cache.join("trickplay");
        tokio::spawn(async move {
            let _ = tokio::fs::remove_dir_all(legacy_cache).await;
        });
        let (db, store) = (service.db.clone(), service.store.clone());
        tokio::spawn(async move { remove_old_bifs(&db, store).await });
        tokio::spawn(service.clone().worker());
        service
    }

    fn dir(&self, file: &FileRow) -> Option<PathBuf> {
        Some(self.store.media_dir(file.fingerprint.as_deref()?)?.join("trickplay"))
    }

    /// The previews of this file, if they exist.
    pub fn index(&self, file: &FileRow) -> Option<TrickplayInfo> {
        let bytes = std::fs::read(self.dir(file)?.join("index.json")).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    pub fn sheet(&self, file: &FileRow, n: u32) -> Option<PathBuf> {
        Some(self.dir(file)?.join(format!("{n}.jpg"))).filter(|p| p.is_file())
    }

    /// Files waiting for previews, those being made included.
    pub fn pending(&self) -> usize {
        self.queues.lock().pending.len()
    }

    /// Files whose previews are being made now.
    pub fn running(&self) -> usize {
        self.queues.lock().views().len()
    }

    /// Whether the previews of this version of the file couldn't be made.
    fn failed(&self, file_id: i64, fingerprint: Option<&str>) -> bool {
        fingerprint.is_some() && self.failed.lock().get(&file_id).map(String::as_str) == fingerprint
    }

    /// Queue the file unless its previews exist, are queued or failed: one
    /// someone opened goes before the libraries' backlog, also when it was
    /// already in it.
    pub fn request(self: &Arc<Self>, file: &FileRow) {
        if !self.settings.get().seek_previews
            || file.video_codec.is_none()
            || self.index(file).is_some()
            || self.failed(file.id, file.fingerprint.as_deref())
        {
            return;
        }
        let mut queues = self.queues.lock();
        let new = queues.pending.insert(file.id);
        if queues.looking.contains(&file.id) {
            // Its lookup takes it to the titles opened.
            return;
        }
        if !new && (queues.disks.contains_key(&file.id) || queues.running.contains_key(&file.id)) {
            let moved = queues.open(file.id);
            drop(queues);
            if moved {
                self.queued.notify_one();
            }
            return;
        }
        queues.looking.insert(file.id);
        drop(queues);
        // Where it is read from is a question for its disk, which may be a
        // share asleep: the file counts as pending meanwhile, and goes with
        // the titles opened once that is known.
        let (this, id, path) = (self.clone(), file.id, PathBuf::from(&file.path));
        let fingerprint = file.fingerprint.as_deref().map(Arc::from);
        tokio::spawn(async move {
            let disk = match this.look_up(path, OPENED_LOOKUP).await {
                Some(lookup) => lookup.await.ok(),
                None => None,
            };
            let mut queues = this.queues.lock();
            queues.looking.remove(&id);
            if !queues.pending.contains(&id) {
                return;
            }
            if let Some(disk) = disk {
                queues.disks.entry(id).or_insert(disk);
            }
            if new {
                queues.opened.push_back(Entry { id, fingerprint });
            } else {
                queues.open(id);
            }
            drop(queues);
            this.queued.notify_one();
        });
    }

    /// The lookup of where `path` is read from ([`Disks::of`]), off the
    /// async workers and a few files at a time ([`LOOKUPS_AT_ONCE`]), so that
    /// a share that doesn't answer holds up nothing else; `None` when none
    /// could start within `wait`.
    async fn look_up(&self, path: PathBuf, wait: Duration) -> Option<JoinHandle<Disk>> {
        let disks = self.disks.clone();
        bounded(self.lookups.clone(), wait, move || disks.of(&path)).await
    }

    /// Queue every video file without previews, newest first, then those
    /// whose previews an earlier version made, when previews are made for
    /// whole libraries. Those are looked for only when that may find others
    /// ([`versions_due`]).
    pub async fn request_all(&self) {
        let settings = self.settings.get();
        if !settings.seek_previews || !settings.previews_in_background {
            return;
        }
        let files = match catalog::video_file_ids(&self.db).await {
            Ok(files) => files,
            Err(err) => {
                tracing::warn!("listing files for seek previews: {err}");
                return;
            }
        };
        // What the transcoder makes now tells which previews are older; at
        // start-up it may not have answered yet.
        if !self.transcoder.is_available() {
            self.transcoder.wait_ready(Duration::from_secs(60)).await;
        }
        let current = self.transcoder.trickplay_version();
        let versions = {
            let mut read = self.versions_read.lock();
            let due = versions_due(*read, current, Instant::now());
            if due {
                *read = Some((current, Instant::now()));
            }
            due
        };
        // Which ones have previews is a question for the disk (one look per
        // file in the data folder, and when versions are due a read of each
        // one's index): asked off the async workers. Those queued already
        // stay as they are, also if they are done meanwhile.
        let queued = self.queues.lock().pending.clone();
        let failed = self.failed.lock().clone();
        let store = self.store.clone();
        let (missing, older) = tokio::task::spawn_blocking(move || {
            let (mut missing, mut older) = (Vec::new(), Vec::new());
            for (id, fp, path) in files {
                if fp.as_ref().is_some_and(|fp| failed.get(&id) == Some(fp)) {
                    continue;
                }
                let index =
                    fp.as_deref().and_then(|fp| store.media_dir(fp)).map(|d| d.join("trickplay").join("index.json"));
                match (index.filter(|index| index.is_file()), fp) {
                    (Some(index), Some(fp)) => {
                        if versions && version_of(&index) < current {
                            older.push((id, Some(fp), path));
                        }
                    }
                    (_, fp) => missing.push((id, fp, path)),
                }
            }
            (missing, older)
        })
        .await
        .unwrap_or_default();
        // Where each is read from, those queued already whose storage is
        // known aside. A lookup that doesn't answer in time leaves the rest
        // unknown (they are likely on the same share): they go alone, until
        // the next scan asks again.
        let known: HashSet<i64> = {
            let queues = self.queues.lock();
            queues.disks.keys().chain(queues.running.keys()).chain(&queues.looking).copied().collect()
        };
        let mut disks = HashMap::new();
        for (id, _, path) in missing.iter().chain(&older) {
            if known.contains(id) {
                continue;
            }
            let lookup = self.look_up(PathBuf::from(path), LIBRARY_LOOKUP).await;
            let answer = match lookup {
                Some(mut lookup) => tokio::time::timeout(LIBRARY_LOOKUP, &mut lookup).await.ok(),
                None => None,
            };
            match answer {
                Some(Ok(disk)) => {
                    disks.insert(*id, disk);
                }
                Some(Err(_)) => {}
                None => {
                    tracing::info!(
                        "a library's storage didn't answer in time: the seek previews of the files left are made \
                         one at a time until the next scan"
                    );
                    break;
                }
            }
        }
        let mut queues = self.queues.lock();
        let mut queue = |queues: &mut Queues, files: Vec<(i64, Option<String>, String)>, kind: Kind| {
            let before = queues.pending.len();
            for (id, fingerprint, _) in files {
                if !queued.contains(&id) && queues.pending.insert(id) {
                    let entry = Entry { id, fingerprint: fingerprint.as_deref().map(Arc::from) };
                    let queue = if kind == Kind::Refresh { &mut queues.refresh } else { &mut queues.library };
                    queue.push_back(entry);
                }
                if queues.pending.contains(&id)
                    && let Some(disk) = disks.remove(&id)
                {
                    queues.disks.entry(id).or_insert(disk);
                }
            }
            queues.pending.len() - before
        };
        let made = queue(&mut queues, missing, Kind::Library);
        let again = queue(&mut queues, older, Kind::Refresh);
        drop(queues);
        if made + again > 0 {
            self.queued.notify_one();
        }
        if made > 0 {
            tracing::info!("making seek previews for {made} files in the background");
        }
        if again > 0 {
            tracing::info!("making the seek previews of {again} files again: an earlier version made them");
        }
    }

    /// Makes what is queued until the server shuts down, which drops the
    /// files in progress (their half-made previews are cleared next time):
    /// it would only fail without the database and the transcoder.
    async fn worker(self: Arc<Self>) {
        let stopping = self.stopping.clone();
        tokio::select! {
            () = stopping.cancelled() => {}
            () = self.work() => {}
        }
    }

    async fn work(self: &Arc<Self>) {
        let mut runs = JoinSet::new();
        let mut rounds = Rounds::default();
        loop {
            while let Some(done) = runs.try_join_next_with_id() {
                self.ended(done, &mut rounds);
            }
            if !self.queues.lock().running.is_empty() {
                // While files are made, a look at the transcoder every so
                // often: something that plays or is transcoded puts them off.
                if rounds.look.is_none_or(|look| look.at.elapsed() >= self.pace.look) {
                    rounds.look = Some(self.look().await);
                }
                let transcoding = rounds.look.is_some_and(|look| look.transcoding);
                let quiet_for = self.sessions.quiet_for();
                let mut queues = self.queues.lock();
                for (id, why) in lanes::put_offs(&queues.views(), quiet_for, transcoding) {
                    queues.put_off(id, why);
                }
            }
            // A playback that is starting gets the disk and the network to
            // itself: even the titles people opened wait (their player asks
            // again every 20 s, so they still show up during the play).
            let starting = self.sessions.starting_for();
            let mut waiting = rounds.hold.is_some_and(|until| Instant::now() < until);
            if !waiting && starting.is_none() && self.transcoder.is_available() {
                rounds.hold = None;
                waiting = self.dispatch(&mut runs, &mut rounds).await;
            }
            // Until something is queued, a file is done, a playback starts or
            // ends, or a look again (the start is over, playback may have
            // ended, the transcoder may have room).
            let busy = waiting || !self.queues.lock().running.is_empty();
            let wait = if busy { self.pace.look } else { starting.map_or(IDLE_LOOK, |left| left.min(IDLE_LOOK)) };
            tokio::select! {
                Some(done) = runs.join_next_with_id() => self.ended(done, &mut rounds),
                () = self.sessions.changed() => {}
                _ = tokio::time::timeout(wait, self.queued.notified()) => {}
            }
        }
    }

    /// Starts the files that may start now; whether they wait for a transcode.
    async fn dispatch(self: &Arc<Self>, runs: &mut JoinSet<Outcome>, rounds: &mut Rounds) -> bool {
        let playing = self.playing_files();
        let (soon, later) = self.library_turn().await;
        let capacity = self.transcoder.trickplay_runs();
        if !self.queues.lock().could_start(&playing, soon.is_some(), capacity) {
            self.narrowed(rounds, None);
            return false;
        }
        // Transcodes come first, a playback's or a download's. The last look
        // serves while it is fresh (`work` takes one every so often while
        // files are made); the transcoder is asked again otherwise.
        let look = match rounds.look.filter(|look| look.at.elapsed() < self.pace.look) {
            Some(look) => look,
            None => {
                let look = self.look().await;
                rounds.look = Some(look);
                look
            }
        };
        if look.transcoding {
            return true;
        }
        let wide = later && self.schedule.wide();
        // A playback that starts from here on puts off what starts now.
        let seen = self.sessions.starts();
        if self.sessions.starting_for().is_some() {
            return false;
        }
        let quiet_for = self.sessions.quiet_for();
        let now = Instant::now();
        let grace = rounds.grace.clone().filter(|(_, until)| now < *until).map(|(disk, _)| disk);
        let making = self.making.lock().clone();
        let twin = |e: &Entry| e.fingerprint.as_ref().is_some_and(|fp| making.contains(fp));
        let few_cores = look.load.is_none_or(|load| load.cores < lanes::FEW_CORES);
        let mut queues = self.queues.lock();

        // A title someone opened, unless one is being made: beside the
        // libraries' files where the transcoder makes several sets at once,
        // before them where it doesn't.
        let views = queues.views();
        let opened = views.iter().any(|r| r.kind == Kind::Opened);
        if !opened && (capacity >= 2 || views.is_empty()) {
            let startable = |e: &Entry| !playing.contains(&e.id) && !queues.running.contains_key(&e.id) && !twin(e);
            if let Some(id) = lanes::pick_opened(&queues.opened, startable) {
                if capacity >= 2 {
                    let disk = queues.disk(id).clone();
                    for off in lanes::opened_puts_off(&disk, &queues.views(), few_cores) {
                        queues.put_off(off, Why::Opened);
                    }
                }
                self.spawn(&mut queues, runs, rounds, id, Kind::Opened, seen);
            }
        }

        // The libraries' files, while library work may run: one at a time,
        // more where the rules let.
        let Some(soon) = soon else {
            drop(queues);
            self.narrowed(rounds, None);
            return false;
        };
        let narrow = loop {
            let views = queues.views();
            let all = views.len() as u32;
            let library = views.iter().filter(|r| r.kind != Kind::Opened).count() as u32;
            if all >= capacity {
                break (library > 0).then_some(Narrow::Transcoder);
            }
            if library > 0 {
                let facts = lanes::Facts {
                    capacity,
                    load: look.load,
                    later,
                    wide,
                    quiet_for,
                    since_library_start: rounds.library_start.map(|at| at.elapsed()),
                    settle: self.pace.settle,
                };
                if let Err(narrow) = lanes::widen(&facts, library, all) {
                    break Some(narrow);
                }
            }
            // Few cores make what someone opened alone.
            if few_cores && capacity >= 2 && views.iter().any(|r| r.kind == Kind::Opened) {
                break (library > 0).then_some(Narrow::Processors);
            }
            let startable = |e: &Entry| {
                !queues.running.contains_key(&e.id)
                    && !twin(e)
                    && lanes::room(queues.disk(e.id), &views, grace.as_ref())
            };
            let Some((kind, id)) = lanes::pick_library(&queues.library, &queues.refresh, &soon, later, startable)
            else {
                break (library > 0).then_some(Narrow::Storage);
            };
            self.spawn(&mut queues, runs, rounds, id, kind, seen);
            rounds.library_start = Some(Instant::now());
        };
        drop(queues);
        self.narrowed(rounds, narrow);
        false
    }

    /// Starts making file `id`'s previews, as a task of its own: a playback
    /// that starts after `seen` puts it off at once, whatever the worker is
    /// doing then.
    fn spawn(
        self: &Arc<Self>,
        queues: &mut Queues,
        runs: &mut JoinSet<Outcome>,
        rounds: &mut Rounds,
        id: i64,
        kind: Kind,
        seen: u64,
    ) {
        let disk = queues.disk(id).clone();
        let cancel = CancellationToken::new();
        rounds.seq += 1;
        queues.running.insert(id, Run { kind, disk, seq: rounds.seq, cancel: cancel.clone(), why: None });
        let this = self.clone();
        let task = runs.spawn(async move {
            tokio::select! {
                biased;
                () = this.sessions.started_since(seen) => Outcome::PutOff,
                () = cancel.cancelled() => Outcome::PutOff,
                outcome = this.make(id) => outcome,
            }
        });
        rounds.tasks.insert(task.id(), id);
    }

    /// A file's task ended: made, failed, put off (it keeps its place), or
    /// to be asked again.
    fn ended(&self, done: Result<(tokio::task::Id, Outcome), JoinError>, rounds: &mut Rounds) {
        let (task, outcome) = match done {
            Ok(done) => done,
            Err(err) => {
                tracing::warn!("seek previews: {err}");
                (err.id(), Outcome::Dropped)
            }
        };
        let Some(id) = rounds.tasks.remove(&task) else { return };
        let mut queues = self.queues.lock();
        let Some(run) = queues.running.remove(&id) else { return };
        match outcome {
            Outcome::Made | Outcome::Failed | Outcome::Dropped => queues.forget(id),
            Outcome::Again => rounds.hold = Some(Instant::now() + self.pace.look),
            Outcome::PutOff => {
                let why = run.why.map_or("a playback is starting", Why::says);
                tracing::debug!("seek previews for file {id} put off: {why}");
            }
            Outcome::Twin(fingerprint) => {
                if let Some(entry) = queues.entry_mut(id) {
                    entry.fingerprint = Some(fingerprint);
                }
            }
        }
        if run.kind == Kind::Opened {
            rounds.grace = Some((run.disk, Instant::now() + self.pace.opened_grace));
        }
        if !queues.running.values().any(|run| run.kind != Kind::Opened) {
            rounds.widest = 0;
        }
    }

    /// Says how many of the libraries' files are made at once when that is
    /// more than before, and why no more when that changes.
    fn narrowed(&self, rounds: &mut Rounds, narrow: Option<Narrow>) {
        let queues = self.queues.lock();
        let library: Vec<&str> =
            queues.views().into_iter().filter(|r| r.kind != Kind::Opened).map(|r| &*r.disk.key).collect();
        if library.len() > rounds.widest {
            rounds.widest = library.len();
            if library.len() > 1 {
                tracing::info!("making the seek previews of {} files at once", library.len());
                tracing::debug!("seek previews read from {}", library.join(", "));
            }
        }
        if narrow != rounds.narrow {
            rounds.narrow = narrow;
            if let Some(narrow) = narrow {
                tracing::debug!("seek previews: no more files at once ({narrow:?})");
            }
        }
    }

    /// How the transcoder is doing; not transcoding, its load unknown, when
    /// it can't say.
    async fn look(&self) -> Look {
        let status = if self.transcoder.is_available() { self.transcoder.status().await.ok() } else { None };
        Look {
            at: Instant::now(),
            transcoding: status.as_ref().is_some_and(|s| s.running_processes > 0),
            load: status.and_then(|s| s.load),
        }
    }

    /// Whether the libraries' previews may be made now (see
    /// [`Scheduler::turn`]): those of what is likely to be watched soon
    /// (given then), and the rest.
    async fn library_turn(&self) -> (Option<Arc<Soon>>, bool) {
        let empty = {
            let queues = self.queues.lock();
            queues.library.is_empty() && queues.refresh.is_empty()
        };
        if empty || self.schedule.turn(Class::Soon).is_err() {
            return (None, false);
        }
        (Some(self.schedule.soon().await), self.schedule.turn(Class::Later).is_ok())
    }

    /// The files sessions are playing now (paused ones included, until they go quiet).
    fn playing_files(&self) -> HashSet<i64> {
        let now = now();
        self.sessions.list().iter().filter(|s| s.is_active(now)).map(|s| s.file_id).collect()
    }

    async fn make(&self, file_id: i64) -> Outcome {
        let mut file = match catalog::file(&self.db, file_id).await {
            Ok(Some(file)) => file,
            Ok(None) => return Outcome::Dropped,
            Err(err) => {
                tracing::warn!("seek previews for file {file_id}: {err}");
                return Outcome::Dropped;
            }
        };
        let fingerprint = match self.store.fingerprint_of(file.id, &file.path, file.fingerprint.as_deref()).await {
            Ok(fingerprint) => fingerprint,
            Err(err) => {
                tracing::warn!("seek previews for file {file_id}: {err:#}");
                return Outcome::Dropped;
            }
        };
        file.fingerprint = Some(fingerprint.clone());
        let Some(_claim) = Claim::take(&self.making, &fingerprint) else {
            return Outcome::Twin(Arc::from(fingerprint));
        };
        // The version the transcoder makes, before previews there are taken
        // for current: while it isn't there, any would be.
        if !self.transcoder.is_available() && !self.transcoder.wait_ready(Duration::from_secs(60)).await {
            return Outcome::Again;
        }
        let Some(version) = self.transcoder.info().map(|info| info.capabilities.trickplay_version) else {
            return Outcome::Again;
        };
        if self.index(&file).is_some_and(|info| info.version >= version) {
            return Outcome::Made;
        }
        let info = match self.transcoder.trickplay(&file.path).await {
            Ok(info) => info,
            Err(err) => {
                let outcome = lanes::outcome(&err);
                match outcome {
                    Outcome::Failed => {
                        tracing::warn!("seek previews for file {file_id} failed: {err}");
                        self.failed.lock().insert(file.id, fingerprint);
                    }
                    Outcome::Again => tracing::debug!("seek previews for file {file_id} wait: {err}"),
                    _ => tracing::warn!("seek previews for file {file_id} not made now: {err}"),
                }
                return outcome;
            }
        };
        if let Err(err) = self.keep(&file, &info).await {
            tracing::warn!("seek previews for file {file_id}: {err:#}");
            return Outcome::Dropped;
        }
        if info.version < version {
            // Made by a transcoder older than the one asked: not again until a restart.
            self.failed.lock().insert(file.id, fingerprint);
        }
        // Chapter images come from the same sheets; make them while they are warm.
        if file.media_info().is_some_and(|m| m.chapters.len() > 1) {
            let _cut = self.cuts.acquire().await;
            if let Err(err) = self.chapter_images(&file).await {
                tracing::debug!("chapter images for file {file_id}: {err:#}");
            }
        }
        Outcome::Made
    }

    /// Fetches the sheets the transcoder made into the file's media folder,
    /// in place of those there.
    async fn keep(&self, file: &FileRow, info: &TrickplayInfo) -> anyhow::Result<()> {
        let target = self.dir(file).context("no media folder")?;
        let part = target.with_extension("part");
        let _ = tokio::fs::remove_dir_all(&part).await;
        tokio::fs::create_dir_all(&part).await.with_context(|| format!("creating {}", part.display()))?;
        for n in 0..info.sheets {
            let bytes = self.transcoder.trickplay_sheet(&info.id, n).await?;
            tokio::fs::write(part.join(format!("{n}.jpg")), bytes).await?;
        }
        tokio::fs::write(part.join("index.json"), serde_json::to_vec(info)?).await?;
        // Previews made before serve until the new ones take their place.
        let old = target.with_extension("old");
        let _ = tokio::fs::remove_dir_all(&old).await;
        let replaced = tokio::fs::rename(&target, &old).await.is_ok();
        if let Err(err) = tokio::fs::rename(&part, &target).await {
            if replaced {
                let _ = tokio::fs::rename(&old, &target).await;
            }
            return Err(err.into());
        }
        if replaced {
            let _ = tokio::fs::remove_dir_all(&old).await;
            // Their chapter images were cut from the sheets replaced.
            if let Some(chapters) = self.chapters_dir(file) {
                let lock = self.cut_lock(file);
                let _guard = lock.lock().await;
                let _ = tokio::fs::remove_dir_all(chapters).await;
            }
        }
        Ok(())
    }

    /// One chapter-image or BIF job per media folder at a time: a file in two
    /// libraries shares its folder.
    fn cut_lock(&self, file: &FileRow) -> Arc<tokio::sync::Mutex<()>> {
        let mut cutting = self.cutting.lock();
        // Those nobody holds or waits for go.
        cutting.retain(|_, lock| Arc::strong_count(lock) > 1);
        cutting.entry(file.fingerprint.clone().unwrap_or_default()).or_default().clone()
    }

    fn chapters_dir(&self, file: &FileRow) -> Option<PathBuf> {
        Some(self.store.media_dir(file.fingerprint.as_deref()?)?.join("chapters"))
    }

    /// Whether the file's chapters have images (or can get them right away).
    pub fn has_chapter_images(&self, file: &FileRow) -> bool {
        self.index(file).is_some()
    }

    /// The image of chapter `index`, cut from the previews when missing.
    pub async fn chapter_image(&self, file: &FileRow, index: usize) -> anyhow::Result<Option<PathBuf>> {
        let Some(dir) = self.chapters_dir(file) else { return Ok(None) };
        let path = dir.join(format!("{index}.jpg"));
        if !path.is_file() {
            self.chapter_images(file).await?;
        }
        Ok(Some(path).filter(|p| p.is_file()))
    }

    /// Cut an image for every chapter out of the sprite sheets.
    async fn chapter_images(&self, file: &FileRow) -> anyhow::Result<()> {
        let lock = self.cut_lock(file);
        let _guard = lock.lock().await;
        let (Some(info), Some(dir), Some(sheets)) = (self.index(file), self.chapters_dir(file), self.dir(file)) else {
            return Ok(());
        };
        let chapters = file.media_info().map(|m| m.chapters).unwrap_or_default();
        if chapters.is_empty() || dir.join(format!("{}.jpg", chapters.len() - 1)).is_file() {
            return Ok(());
        }
        let tiles: Vec<u32> = chapters
            .iter()
            .map(|c| {
                let length = (c.end_ms - c.start_ms).max(0);
                tile_at(&info, c.start_ms + CHAPTER_OFFSET_MS.min(length / 5))
            })
            .collect();
        crate::util::blocking(move || {
            std::fs::create_dir_all(&dir)?;
            // Chapters come in order, and so do their sheets: one decoded at a
            // time (a sheet is tens of megabytes so).
            let mut decoded: Option<(u32, image::RgbImage)> = None;
            for (index, n) in tiles.into_iter().enumerate() {
                let (sheet, x, y) = tile(&info, n);
                let img = match decoded.take() {
                    Some((at, img)) if at == sheet => img,
                    _ => image::open(sheets.join(format!("{sheet}.jpg")))?.to_rgb8(),
                };
                let crop = image::imageops::crop_imm(&img, x, y, info.width, info.height).to_image();
                decoded = Some((sheet, img));
                let tmp = dir.join(format!("{index}.jpg.tmp"));
                std::fs::write(&tmp, encode_jpeg(&crop)?)?;
                std::fs::rename(&tmp, dir.join(format!("{index}.jpg")))?;
            }
            Ok(())
        })
        .await
    }

    /// Plex's BIF index of the previews: the one in the cache, or else one
    /// cut from the sheets and kept there. It is named by the previews it is
    /// cut from, so previews made again otherwise get another one.
    pub async fn bif(&self, file: &FileRow) -> anyhow::Result<Option<Vec<u8>>> {
        let (Some(info), Some(sheets), Some(fingerprint)) =
            (self.index(file), self.dir(file), file.fingerprint.as_deref())
        else {
            return Ok(None);
        };
        let path = self.bif_cache.join(format!(
            "{fingerprint}-{}-{}-{}x{}-{}.bif",
            info.version, info.interval_ms, info.width, info.height, info.count
        ));
        // Read rather than looked for: the cache's clean-up may remove it in between.
        if let Ok(bytes) = tokio::fs::read(&path).await {
            return Ok(Some(bytes));
        }
        let lock = self.cut_lock(file);
        let _guard = lock.lock().await;
        if let Ok(bytes) = tokio::fs::read(&path).await {
            return Ok(Some(bytes));
        }
        let dir = self.bif_cache.clone();
        let bytes = crate::util::blocking(move || {
            let mut frames = Vec::with_capacity(info.count as usize);
            for sheet in 0..info.sheets {
                let img = image::open(sheets.join(format!("{sheet}.jpg")))?.to_rgb8();
                let per_sheet = info.columns * info.rows;
                for k in 0..per_sheet {
                    let n = sheet * per_sheet + k;
                    if n >= info.count {
                        break;
                    }
                    let (_, x, y) = tile(&info, n);
                    let crop = image::imageops::crop_imm(&img, x, y, info.width, info.height).to_image();
                    frames.push(encode_jpeg(&crop)?);
                }
            }
            let bytes = bif_bytes(&frames, info.interval_ms.clamp(1, i64::from(u32::MAX)) as u32);
            // Not kept (a full disk) costs only another cut next time.
            if let Err(err) = save(&dir, &path, &bytes) {
                tracing::debug!("could not keep {}: {err}", path.display());
            }
            Ok(bytes)
        })
        .await?;
        Ok(Some(bytes))
    }

    /// The preview frame at `ms`, out of the BIF index.
    pub async fn frame_at(&self, file: &FileRow, ms: i64) -> anyhow::Result<Option<Vec<u8>>> {
        let Some(bif) = self.bif(file).await? else { return Ok(None) };
        Ok(bif_frame(&bif, ms).map(<[u8]>::to_vec))
    }
}

/// Earlier versions kept the BIF index beside the previews, for good: those
/// go, once. A Plex app that asks again gets one cut into the cache.
async fn remove_old_bifs(db: &Db, store: Arc<Store>) {
    let done: sqlx::Result<Option<String>> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
        .bind(OLD_BIFS_REMOVED)
        .fetch_optional(&db.read)
        .await;
    if !matches!(done, Ok(None)) {
        return;
    }
    // Only video files have previews.
    let fingerprints: Vec<String> = match sqlx::query_scalar(
        "SELECT DISTINCT fingerprint FROM media_files
         WHERE video_codec IS NOT NULL AND fingerprint IS NOT NULL AND fingerprint != ''",
    )
    .fetch_all(&db.read)
    .await
    {
        Ok(fingerprints) => fingerprints,
        Err(err) => {
            tracing::warn!("listing the media folders that may hold old BIF indexes: {err}");
            return;
        }
    };
    let removed = crate::util::blocking(move || {
        let (mut count, mut bytes) = (0u64, 0u64);
        for fingerprint in &fingerprints {
            let Some(dir) = store.media_dir(fingerprint) else { continue };
            let path = dir.join("trickplay").join("index-sd.bif");
            if let Ok(meta) = std::fs::metadata(&path)
                && std::fs::remove_file(&path).is_ok()
            {
                count += 1;
                bytes += meta.len();
            }
        }
        Ok((count, bytes))
    })
    .await;
    let Ok((count, bytes)) = removed else { return };
    if let Err(err) = sqlx::query("INSERT OR IGNORE INTO settings (key, value) VALUES (?, '1')")
        .bind(OLD_BIFS_REMOVED)
        .execute(&db.write)
        .await
    {
        tracing::warn!("saving that the old BIF indexes are gone: {err}");
    }
    if count > 0 {
        tracing::info!(
            "removed {count} BIF indexes kept beside seek previews ({} MB freed): they are kept in the cache now",
            bytes / 1_000_000
        );
    }
}

/// Whether a scan reads the version of every file's previews at `now`, to
/// find those an earlier version made, `read` being the last such read (the
/// transcoder's version then, and when): only when that may find others
/// than it did, as the transcoder makes another version, or a day later, so
/// that those dropped from the queue meanwhile are found again. Never while
/// the transcoder doesn't say which it makes (0).
fn versions_due(read: Option<(u32, Instant)>, current: u32, now: Instant) -> bool {
    current > 0
        && read.is_none_or(|(version, at)| version != current || now.saturating_duration_since(at) >= VERSIONS_AGAIN)
}

/// The version of the previews whose index is at `index`; 0 when it
/// doesn't say (made before versions), or can't be read (made again).
fn version_of(index: &Path) -> u32 {
    std::fs::read(index)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<TrickplayInfo>(&bytes).ok())
        .map_or(0, |info| info.version)
}

/// Write `bytes` to `path` whole, through a temporary file of its own in
/// `dir`: a file played from two libraries may be cut by both at once.
fn save(dir: &Path, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4().simple()));
    let saved = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path));
    if saved.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    saved
}

const BIF_MAGIC: [u8; 8] = [0x89, b'B', b'I', b'F', 0x0d, 0x0a, 0x1a, 0x0a];

/// A BIF file (Roku's format, used by Plex): a 64-byte header, an index of
/// (frame number, offset) pairs ending with 0xffffffff, then the JPEGs. Frame
/// `n` shows the video at `n × interval_ms`.
fn bif_bytes(frames: &[Vec<u8>], interval_ms: u32) -> Vec<u8> {
    let index_len = (frames.len() + 1) * 8;
    let mut out = Vec::with_capacity(64 + index_len + frames.iter().map(Vec::len).sum::<usize>());
    out.extend_from_slice(&BIF_MAGIC);
    out.extend_from_slice(&0u32.to_le_bytes()); // version
    out.extend_from_slice(&(frames.len() as u32).to_le_bytes());
    out.extend_from_slice(&interval_ms.to_le_bytes());
    out.resize(64, 0);
    let mut offset = (64 + index_len) as u32;
    for (n, frame) in frames.iter().enumerate() {
        out.extend_from_slice(&(n as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += frame.len() as u32;
    }
    out.extend_from_slice(&u32::MAX.to_le_bytes());
    out.extend_from_slice(&offset.to_le_bytes());
    for frame in frames {
        out.extend_from_slice(frame);
    }
    out
}

/// The frame of a BIF file shown at `ms`.
fn bif_frame(bif: &[u8], ms: i64) -> Option<&[u8]> {
    if bif.len() < 64 || bif[..8] != BIF_MAGIC {
        return None;
    }
    let word = |at: usize| bif.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let count = word(12)? as usize;
    let interval = word(16)?.max(1) as i64;
    if count == 0 {
        return None;
    }
    let n = ((ms.max(0) / interval) as usize).min(count - 1);
    let start = word(64 + n * 8 + 4)? as usize;
    let end = word(64 + (n + 1) * 8 + 4)? as usize;
    bif.get(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info() -> TrickplayInfo {
        TrickplayInfo {
            id: "x".into(),
            interval_ms: 10_000,
            width: 320,
            height: 180,
            columns: 10,
            rows: 10,
            count: 150,
            sheets: 2,
            version: 1,
        }
    }

    #[test]
    fn files_being_made_keep_their_place() {
        let disk = Disk { key: Arc::from("disk:sda"), width: 1 };
        let mut queues = Queues::default();
        for id in 1..=5 {
            queues.pending.insert(id);
            queues.library.push_back(Entry { id, fingerprint: None });
            queues.disks.insert(id, disk.clone());
        }
        let start = |queues: &mut Queues, id: i64, seq: u64| {
            let run = Run { kind: Kind::Library, disk: disk.clone(), seq, cancel: CancellationToken::new(), why: None };
            queues.running.insert(id, run);
        };
        let ids = |queue: &VecDeque<Entry>| queue.iter().map(|e| e.id).collect::<Vec<_>>();
        for (seq, id) in [1, 3, 5].into_iter().enumerate() {
            start(&mut queues, id, seq as u64);
        }
        assert_eq!(queues.views().len(), 3);
        // Put off, whatever order their tasks end in: where they were.
        for id in [5, 1, 3] {
            queues.put_off(id, Why::Playing);
            assert!(queues.running[&id].cancel.is_cancelled());
            queues.running.remove(&id);
        }
        assert_eq!(ids(&queues.library), [1, 2, 3, 4, 5]);
        assert_eq!(queues.pending.len(), 5);

        // Opened while being made: it goes on, as an opened title's.
        start(&mut queues, 3, 4);
        assert!(queues.open(3));
        assert_eq!((ids(&queues.opened), ids(&queues.library)), (vec![3], vec![1, 2, 4, 5]));
        assert_eq!(queues.running[&3].kind, Kind::Opened);
        assert!(!queues.open(3), "once");
        assert!(!queues.open(9), "not queued");
        // Made: out of its queue, no longer pending.
        queues.running.remove(&3);
        queues.forget(3);
        assert!(queues.opened.is_empty());
        assert_eq!((queues.pending.len(), queues.disks.len()), (4, 4));
        assert_eq!(queues.disk(3).width, 0, "a file whose storage isn't known goes alone");
        assert!(queues.could_start(&HashSet::new(), true, 1));
        assert!(!queues.could_start(&HashSet::new(), false, 1), "nothing opened, not the libraries' turn");
    }

    #[tokio::test]
    async fn lookups_that_do_not_answer_hold_up_nothing_else() {
        let permits = Arc::new(tokio::sync::Semaphore::new(1));
        let wait = Duration::from_millis(100);
        let (release, held) = std::sync::mpsc::channel::<()>();
        // A share that doesn't answer keeps its permit, however long...
        let mut hung = bounded(permits.clone(), wait, move || held.recv().is_ok()).await.expect("a permit");
        assert!(tokio::time::timeout(wait, &mut hung).await.is_err());
        // ...so that the next waits for one only so long.
        assert!(bounded(permits.clone(), wait, || 1).await.is_none(), "no permit to spare");
        // Once it answers, others are asked again.
        release.send(()).unwrap();
        assert!(hung.await.unwrap());
        let next = bounded(permits.clone(), wait, || 2).await.expect("a permit again");
        assert_eq!(next.await.unwrap(), 2);
    }

    #[test]
    fn files_with_the_same_contents_are_made_once_at_a_time() {
        let making = Mutex::default();
        let claim = Claim::take(&making, "0123456789abcdef").expect("the first claims it");
        assert!(Claim::take(&making, "0123456789abcdef").is_none(), "its twin waits");
        assert!(making.lock().contains("0123456789abcdef"), "still claimed by the first");
        assert!(Claim::take(&making, "fedcba9876543210").is_some(), "other contents don't wait");
        drop(claim);
        assert!(Claim::take(&making, "0123456789abcdef").is_some(), "given up once dropped");
    }

    #[test]
    fn versions_of_previews_made_before_are_zero() {
        let dir = tempfile::tempdir().unwrap();
        let index = dir.path().join("index.json");
        std::fs::write(&index, serde_json::to_vec(&info()).unwrap()).unwrap();
        assert_eq!(version_of(&index), 1);
        // As earlier versions wrote it: no version.
        let mut before = serde_json::to_value(info()).unwrap();
        before.as_object_mut().unwrap().remove("version");
        std::fs::write(&index, before.to_string()).unwrap();
        assert_eq!(version_of(&index), 0);
        std::fs::write(&index, b"not json").unwrap();
        assert_eq!(version_of(&index), 0, "made again");
    }

    #[test]
    fn versions_are_read_again_for_another_transcoder_or_a_day_later() {
        let now = Instant::now();
        assert!(versions_due(None, 1, now), "the first scan");
        assert!(!versions_due(Some((1, now)), 1, now + Duration::from_secs(3600)), "the same transcoder, that day");
        assert!(versions_due(Some((1, now)), 2, now + Duration::from_secs(60)), "a transcoder that makes others");
        assert!(versions_due(Some((1, now)), 1, now + VERSIONS_AGAIN), "a day later");
        assert!(!versions_due(None, 0, now), "a transcoder that doesn't say");
    }

    #[test]
    fn tiles_map_to_sheets() {
        let info = info();
        assert_eq!(tile(&info, 0), (0, 0, 0));
        assert_eq!(tile(&info, 13), (0, 3 * 320, 180));
        assert_eq!(tile(&info, 104), (1, 4 * 320, 0));
        assert_eq!(tile_at(&info, 25_000), 2);
        assert_eq!(tile_at(&info, 99_999_999), 149, "past the end: the last tile");
    }

    #[test]
    fn bif_index_finds_frames() {
        let frames = vec![b"first".to_vec(), b"second".to_vec(), b"third".to_vec()];
        let bif = bif_bytes(&frames, 5_000);
        assert_eq!(&bif[..8], &BIF_MAGIC);
        assert_eq!(bif_frame(&bif, 0), Some(&b"first"[..]));
        assert_eq!(bif_frame(&bif, 7_400), Some(&b"second"[..]));
        assert_eq!(bif_frame(&bif, 60_000), Some(&b"third"[..]));
        assert_eq!(bif_frame(b"not a bif", 0), None);
    }

    #[tokio::test]
    async fn old_bif_indexes_go_once() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        for sql in [
            "INSERT INTO libraries (id, name, kind, paths, created_at) VALUES (1, 'Movies', 'movies', '[]', 0)",
            "INSERT INTO items (id, library_id, kind, key, title, sort_title, added_at, updated_at)
             VALUES (1, 1, 'movie', 'm', 'M', 'm', 0, 0)",
            "INSERT INTO media_files (item_id, library_id, path, size, mtime, fingerprint, video_codec, added_at, updated_at)
             VALUES (1, 1, '/m/a.mkv', 1, 0, '0123456789abcdef', 'h264', 0, 0)",
        ] {
            sqlx::query(sql).execute(&db.write).await.unwrap();
        }
        let store = Arc::new(Store::new(
            db.clone(),
            &dir.path().join("metadata"),
            &dir.path().join("media"),
            &dir.path().join("cache"),
        ));
        let previews = store.media_dir("0123456789abcdef").unwrap().join("trickplay");
        std::fs::create_dir_all(&previews).unwrap();
        for name in ["0.jpg", "index.json", "index-sd.bif"] {
            std::fs::write(previews.join(name), b"kept").unwrap();
        }

        remove_old_bifs(&db, store.clone()).await;
        assert!(!previews.join("index-sd.bif").exists(), "the BIF index goes");
        assert!(previews.join("0.jpg").is_file() && previews.join("index.json").is_file(), "the previews stay");

        // Done once: what is there afterwards isn't looked for again.
        std::fs::write(previews.join("index-sd.bif"), b"kept").unwrap();
        remove_old_bifs(&db, store).await;
        assert!(previews.join("index-sd.bif").is_file());
    }

    #[test]
    fn kept_files_are_whole() {
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("bif");
        let path = cache.join("0123456789abcdef-10000-320x180-150.bif");
        save(&cache, &path, b"first").unwrap();
        save(&cache, &path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        let names: Vec<_> = std::fs::read_dir(&cache).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(names.len(), 1, "no temporary file left: {names:?}");
    }
}

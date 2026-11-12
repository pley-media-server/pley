//! Library scanning.
//!
//! A single worker processes scan requests one library at a time. A scan walks
//! the library folders, diffs the files against the database (size + mtime),
//! probes new/changed files through the transcoder with bounded concurrency,
//! writes items in short transactions, removes vanished files (unless their
//! drive is offline), refreshes sidecar artwork, subtitles and NFO files (whose
//! ids decide a title's match, and whose fields win over the online source's),
//! recomputes counts and finally lets the metadata agent enrich new items.
//!
//! The database has a single writer, which progress reports, logins and
//! edits wait for: whatever takes time (matching sidecar files, deciding
//! what changed) is done before taking it, and only changes are written.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use parking_lot::Mutex;
use pley_transcoder_api::{MediaInfo, StreamKind};
use serde::Serialize;
use sqlx::SqliteConnection;
use tokio::sync::mpsc;

use super::nfo::{self, Nfo, NfoFields, NfoIds, NfoKind};
use super::parse::{EpisodeName, parse_episode, parse_movie, parse_track};
use super::sidecar::{ExternalSub, Sidecars};
use super::walk::{Extra, ExtraKind, FoundFile, Walked, walk};
use super::{LibraryKind, LibraryRow, get_library, list_libraries};
use crate::config::Config;
use crate::db::Db;
use crate::events::{EventBus, ScanPhase, ServerEvent};
use crate::metadata::MetadataAgent;
use crate::schedule::LibraryHold;
use crate::store::Store;
use crate::transcoder::{Transcoder, TranscoderError};
use crate::util::{Pace, normalize_key, now, path_str, sort_title};

const PROBE_CONCURRENCY: usize = 6;
const WRITE_BATCH: usize = 64;
/// Rows changed per transaction when a scan applies what changed.
const CHANGE_BATCH: usize = 500;

#[derive(Debug, Clone, Serialize)]
pub struct ScanStatus {
    pub library_id: i64,
    pub library_name: String,
    pub phase: ScanPhase,
    pub processed: u64,
    pub total: u64,
    pub started_at: i64,
    /// The library's first scan: Home says how far it is.
    pub first: bool,
    /// About how many seconds its files have left to be read, or its
    /// titles' details to be fetched, once that can be told from how fast
    /// they've gone.
    pub left_secs: Option<u64>,
    /// The phase `left_secs` is told for, and how fast it goes.
    #[serde(skip)]
    pace: Option<(ScanPhase, Pace)>,
}

impl ScanStatus {
    /// The scan is at `processed` of `total` in `phase`, with about how long
    /// that has left.
    fn update(&mut self, phase: ScanPhase, processed: u64, total: u64) {
        self.phase = phase;
        self.processed = processed;
        self.total = total;
        // Measured from where the phase began: reading files and fetching
        // details go at their own paces.
        if self.pace.is_none_or(|(measured, _)| measured != phase) {
            self.pace =
                matches!(phase, ScanPhase::Probing | ScanPhase::Metadata).then(|| (phase, Pace::new(processed)));
        }
        self.left_secs = self.pace.and_then(|(_, pace)| pace.left(processed, total));
    }
}

#[derive(Debug, Clone, Copy)]
struct ScanRequest {
    library_id: i64,
    force: bool,
    /// Walk the folders and write what changed.
    files: bool,
    /// Then fetch metadata for the titles that have none.
    metadata: bool,
}

impl ScanRequest {
    /// Two requests for one library as one: what either asked for.
    fn merge(&mut self, other: ScanRequest) {
        self.force |= other.force;
        self.files |= other.files;
        self.metadata |= other.metadata;
    }
}

pub struct Scanner {
    db: Db,
    transcoder: Arc<Transcoder>,
    events: EventBus,
    metadata: Arc<MetadataAgent>,
    store: Arc<Store>,
    /// Held through a library's first scan (see [`crate::schedule`]).
    hold: Arc<LibraryHold>,
    queue: mpsc::UnboundedSender<ScanRequest>,
    status: Mutex<HashMap<i64, ScanStatus>>,
    /// First scans that finished while other first scans are under way:
    /// Home ticks them off until the last is done.
    first_done: Mutex<Vec<ScanStatus>>,
    failures: Mutex<ScanFailures>,
}

#[derive(Debug, Default, Clone, Copy)]
struct Totals {
    added: u64,
    updated: u64,
    removed: u64,
}

/// What a scan went through and how long each of its phases took, for the
/// log: a slow scan says where its time went.
#[derive(Debug, Default)]
struct Phases {
    folders: usize,
    files: usize,
    /// Listing the folders, and indexing the sidecar files found.
    walk: Duration,
    /// Telling new, changed and vanished files apart.
    diff: Duration,
    /// Waiting for the transcoder's probes.
    probe: Duration,
    /// Saving files and items, removing vanished ones, counting children.
    write: Duration,
    /// Subtitles, local artwork and NFO files next to unchanged files.
    sidecars: Duration,
    metadata: Duration,
}

/// A duration for the log: `35ms`, `1.2s`.
fn took(duration: Duration) -> String {
    if duration < Duration::from_secs(1) {
        format!("{}ms", duration.as_millis())
    } else {
        format!("{:.1}s", duration.as_secs_f64())
    }
}

/// Libraries whose last scan failed. `scanned_at` only moves when a scan
/// succeeds, so without this a library that cannot be scanned (a full disk, a
/// read-only or damaged database) would be walked and probed again every
/// minute.
#[derive(Debug, Default)]
pub struct ScanFailures {
    /// Library → when its last scan failed, and how many failed in a row.
    failed: HashMap<i64, (i64, u32)>,
}

impl ScanFailures {
    /// How long to wait after `failures` failed scans in a row: 5 minutes,
    /// doubling up to 6 hours, never longer than the scan interval.
    pub fn retry_delay(failures: u32, interval: i64) -> i64 {
        let delay = (300i64 << failures.saturating_sub(1).min(7)).min(6 * 3600);
        if interval > 0 { delay.min(interval) } else { delay }
    }

    pub fn record(&mut self, library_id: i64, ok: bool, at: i64) {
        if ok {
            self.failed.remove(&library_id);
        } else {
            let failures = self.failed.get(&library_id).map_or(0, |&(_, n)| n);
            self.failed.insert(library_id, (at, failures + 1));
        }
    }

    /// Whether a library whose last scan failed should not be scanned yet.
    pub fn backing_off(&self, library_id: i64, interval: i64, now: i64) -> bool {
        self.failed.get(&library_id).is_some_and(|&(at, n)| now - at < Self::retry_delay(n, interval))
    }

    /// Whether the periodic scan of `lib`, every `interval` seconds, is due.
    pub fn due(&self, lib: &LibraryRow, interval: i64, now: i64) -> bool {
        match self.failed.get(&lib.id) {
            Some(&(at, n)) => now - at >= Self::retry_delay(n, interval),
            None => lib.scanned_at.is_none_or(|t| now - t >= interval),
        }
    }
}

impl Scanner {
    pub fn new(
        db: Db,
        transcoder: Arc<Transcoder>,
        events: EventBus,
        metadata: Arc<MetadataAgent>,
        store: Arc<Store>,
        hold: Arc<LibraryHold>,
        config: &Config,
    ) -> Arc<Self> {
        let _ = config;
        let (tx, rx) = mpsc::unbounded_channel();
        let scanner = Arc::new(Self {
            db,
            transcoder,
            events,
            metadata,
            store,
            hold,
            queue: tx,
            status: Mutex::default(),
            first_done: Mutex::default(),
            failures: Mutex::default(),
        });
        tokio::spawn(scanner.clone().worker(rx));
        scanner
    }

    /// Whether the periodic scan of `lib` is due ([`ScanFailures::due`]).
    pub fn due(&self, lib: &LibraryRow, interval: i64) -> bool {
        self.failures.lock().due(lib, interval, now())
    }

    /// Whether a library whose last scan failed should not be scanned yet.
    pub fn backing_off(&self, library_id: i64, interval: i64) -> bool {
        self.failures.lock().backing_off(library_id, interval, now())
    }

    /// Scans a library: its files, then metadata for what has none.
    pub fn enqueue(&self, library_id: i64, force: bool) {
        self.request(ScanRequest { library_id, force, files: true, metadata: true }, false);
    }

    /// Scans a library just added: a first scan from the moment it's
    /// queued, so Home lists it while it waits behind another.
    pub fn enqueue_new(&self, library_id: i64) {
        self.request(ScanRequest { library_id, force: false, files: true, metadata: true }, true);
    }

    /// Scans a library's files only, leaving metadata for later: the Plex
    /// import brings Plex's matches first, so the fetch finds their ids.
    pub fn enqueue_files(&self, library_id: i64) {
        self.request(ScanRequest { library_id, force: false, files: true, metadata: false }, false);
    }

    /// Fetches metadata for a library's titles that have none, without
    /// walking its folders again.
    pub fn enqueue_metadata(&self, library_id: i64) {
        self.request(ScanRequest { library_id, force: false, files: false, metadata: true }, false);
    }

    /// Queues `req`; `first` when it's the library's first scan.
    fn request(&self, req: ScanRequest, first: bool) {
        let library_id = req.library_id;
        let mut status = self.status.lock();
        let entry = status.entry(library_id).or_insert_with(|| ScanStatus {
            library_id,
            library_name: String::new(),
            phase: ScanPhase::Queued,
            processed: 0,
            total: 0,
            started_at: now(),
            first: false,
            left_secs: None,
            pace: None,
        });
        entry.first |= first;
        drop(status);
        let _ = self.queue.send(req);
        self.events.send(ServerEvent::ScanProgress { library_id, phase: ScanPhase::Queued, processed: 0, total: 0 });
    }

    pub async fn enqueue_all(&self, force: bool) -> sqlx::Result<()> {
        for lib in list_libraries(&self.db).await? {
            self.request(
                ScanRequest { library_id: lib.id, force, files: true, metadata: true },
                lib.scanned_at.is_none(),
            );
        }
        Ok(())
    }

    pub fn statuses(&self) -> Vec<ScanStatus> {
        let mut list: Vec<_> = self.status.lock().values().cloned().collect();
        list.sort_by_key(|s| s.started_at);
        list
    }

    /// The first scans done while others are queued or running; none once
    /// the last is done. A library scanned again since isn't among them.
    pub fn first_scans_done(&self) -> Vec<ScanStatus> {
        let status = self.status.lock();
        self.first_done.lock().iter().filter(|s| !status.contains_key(&s.library_id)).cloned().collect()
    }

    pub fn is_scanning(&self, library_id: i64) -> bool {
        self.status.lock().contains_key(&library_id)
    }

    /// Where a library's scan is, while it's queued or running.
    pub fn status_of(&self, library_id: i64) -> Option<ScanStatus> {
        self.status.lock().get(&library_id).cloned()
    }

    async fn worker(self: Arc<Self>, mut rx: mpsc::UnboundedReceiver<ScanRequest>) {
        let mut pending: Vec<ScanRequest> = Vec::new();
        // A new library's files and metadata go before other library work,
        // and so do the scans queued behind it: released between two, the
        // seek previews of the first would start beside the next.
        let mut hold = None;
        loop {
            // One queued while the last ran keeps the hold; only an empty
            // queue lets library work go.
            if pending.is_empty()
                && let Ok(req) = rx.try_recv()
            {
                pending.push(req);
            }
            if pending.is_empty() {
                hold = None;
                match rx.recv().await {
                    Some(req) => pending.push(req),
                    None => break,
                }
            }
            while let Ok(req) = rx.try_recv() {
                match pending.iter_mut().find(|p| p.library_id == req.library_id) {
                    Some(existing) => existing.merge(req),
                    None => pending.push(req),
                }
            }
            let req = pending.remove(0);
            if req.files && self.first_scan(req.library_id).await {
                hold.get_or_insert_with(|| self.hold.take());
            } else if hold.is_some() && !self.first_pending(&pending).await {
                // Only a library's first scan holds library work back: a
                // later one, with none to come, lets it go.
                hold = None;
            }
            let started = Instant::now();
            let result = self.scan(req).await;
            let scanned = matches!(result, Ok(Some(_)));
            self.failures.lock().record(req.library_id, result.is_ok(), now());
            match result {
                Ok(Some((t, p))) => tracing::info!(
                    "library {} scanned in {}: {} added, {} updated, {} removed ({} folders, {} media files; \
                     walk {}, diff {}, probe {}, write {}, sidecars {}, metadata {})",
                    req.library_id,
                    took(started.elapsed()),
                    t.added,
                    t.updated,
                    t.removed,
                    p.folders,
                    p.files,
                    took(p.walk),
                    took(p.diff),
                    took(p.probe),
                    took(p.write),
                    took(p.sidecars),
                    took(p.metadata)
                ),
                Ok(None) => {}
                Err(err) => tracing::error!("scan of library {} failed: {err:#}", req.library_id),
            }
            if !pending.iter().any(|p| p.library_id == req.library_id) {
                let mut status = self.status.lock();
                let mut first_done = self.first_done.lock();
                if let Some(done) = status.remove(&req.library_id)
                    && done.first
                    && scanned
                {
                    first_done.retain(|s| s.library_id != req.library_id);
                    first_done.push(ScanStatus { phase: ScanPhase::Done, left_secs: None, ..done });
                }
                if !status.values().any(|s| s.first) {
                    first_done.clear();
                }
            }
            self.events.send(ServerEvent::ScanProgress {
                library_id: req.library_id,
                phase: ScanPhase::Done,
                processed: 0,
                total: 0,
            });
        }
    }

    /// Whether one of `pending` is a library's first scan.
    async fn first_pending(&self, pending: &[ScanRequest]) -> bool {
        for req in pending {
            if req.files && self.first_scan(req.library_id).await {
                return true;
            }
        }
        false
    }

    /// Whether `library_id` was never scanned.
    async fn first_scan(&self, library_id: i64) -> bool {
        get_library(&self.db, library_id).await.ok().flatten().is_some_and(|lib| lib.scanned_at.is_none())
    }

    fn set_status(&self, lib: &LibraryRow, phase: ScanPhase, processed: u64, total: u64) {
        let mut status = self.status.lock();
        let entry = status.entry(lib.id).or_insert_with(|| ScanStatus {
            library_id: lib.id,
            library_name: lib.name.clone(),
            phase,
            processed,
            total,
            started_at: now(),
            first: false,
            left_secs: None,
            pace: None,
        });
        entry.library_name = lib.name.clone();
        entry.update(phase, processed, total);
        drop(status);
        self.events.send(ServerEvent::ScanProgress { library_id: lib.id, phase, processed, total });
    }

    async fn scan(&self, req: ScanRequest) -> anyhow::Result<Option<(Totals, Phases)>> {
        let ScanRequest { library_id, force, .. } = req;
        let Some(lib) = get_library(&self.db, library_id).await? else {
            return Ok(None);
        };
        if req.files
            && lib.scanned_at.is_none()
            && let Some(status) = self.status.lock().get_mut(&library_id)
        {
            status.first = true;
        }
        self.events.send(ServerEvent::ScanStarted { library_id });
        let (totals, mut phases, processed, total) =
            if req.files { self.scan_files(&lib, force).await? } else { (Totals::default(), Phases::default(), 0, 0) };

        let mut phase = Instant::now();
        if req.metadata && self.metadata.enabled_for(lib.kind()) {
            self.set_status(&lib, ScanPhase::Metadata, 0, 0);
            let progress = |done: u64, total: u64| self.set_status(&lib, ScanPhase::Metadata, done, total);
            if let Err(err) = self.metadata.refresh_library(&lib, force, &progress).await {
                tracing::warn!("metadata refresh for library {} failed: {err:#}", lib.id);
            }
            self.events.send(ServerEvent::LibraryChanged { library_id });
        }
        if totals.removed > 0 {
            self.metadata.prune_people().await;
        }
        phases.metadata = phase.elapsed();
        if req.files {
            // New and changed files get their fingerprint; then whatever
            // removed items left behind goes.
            let store = self.store.clone();
            let removed = totals.removed > 0;
            tokio::spawn(async move {
                store.fingerprint_pending().await;
                if removed && let Err(err) = store.clean().await {
                    tracing::warn!("storage clean-up failed: {err:#}");
                }
            });
        }

        self.set_status(&lib, ScanPhase::Done, processed, total);
        self.events.send(ServerEvent::ScanFinished {
            library_id,
            added: totals.added,
            updated: totals.updated,
            removed: totals.removed,
        });
        phase = Instant::now();
        self.db.optimize().await;
        phases.write += phase.elapsed();
        Ok(Some((totals, phases)))
    }

    /// The files part of a scan: walks the folders, probes and writes what
    /// changed, sidecars and NFO files, counts, and the overrides over it.
    /// Gives the totals, the phases' times, and the files probed of those
    /// to probe.
    async fn scan_files(&self, lib: &LibraryRow, force: bool) -> anyhow::Result<(Totals, Phases, u64, u64)> {
        let library_id = lib.id;
        let kind = lib.kind();
        let roots = lib.paths();
        self.set_status(lib, ScanPhase::Walking, 0, 0);

        // Sidecar files are indexed once: matching them file by file against
        // the whole library made every scan quadratic in its size.
        let mut phase = Instant::now();
        let (mut walked, sidecars) = {
            let roots = roots.clone();
            crate::util::blocking(move || {
                let mut walked = walk(&roots, kind);
                let sidecars = Sidecars::new(std::mem::take(&mut walked.sidecars));
                Ok((walked, Arc::new(sidecars)))
            })
            .await?
        };
        let mut phases =
            Phases { folders: walked.folders, files: walked.media.len(), walk: phase.elapsed(), ..Default::default() };

        phase = Instant::now();
        let existing: Vec<KnownFile> =
            sqlx::query_as("SELECT id, path, size, mtime, probed, external_subs FROM media_files WHERE library_id = ?")
                .bind(library_id)
                .fetch_all(&self.db.read)
                .await?;
        let existing_by_path: HashMap<&str, &KnownFile> = existing.iter().map(|f| (f.path.as_str(), f)).collect();

        let mut to_process = Vec::new();
        let mut unchanged = Vec::new();
        let mut seen = HashSet::new();
        for file in &walked.media {
            let key = path_str(&file.path);
            match existing_by_path.get(key.as_str()) {
                Some(known) if !force && known.size == file.size && known.mtime == file.mtime && known.probed => {
                    unchanged.push((known.id, file.path.clone(), known.external_subs.clone()));
                }
                _ => to_process.push(file.clone()),
            }
            seen.insert(key);
        }
        // A root that suddenly holds no media at all is far more likely an
        // unmounted drive (the mount point exists but is empty) than a library
        // whose every file was deleted: keep its items.
        for root in &roots {
            let empty = !walked.media.iter().any(|f| &f.root == root);
            if empty
                && !walked.offline_roots.contains(root)
                && existing.iter().any(|f| Path::new(&f.path).starts_with(root))
            {
                tracing::warn!("library folder {} is empty; assuming its drive is not mounted", root.display());
                walked.offline_roots.push(root.clone());
            }
        }
        let removed: Vec<i64> = existing
            .iter()
            .filter(|f| !seen.contains(&f.path) && !walked.maybe_unavailable(Path::new(&f.path)))
            .map(|f| f.id)
            .collect();

        phases.diff = phase.elapsed();

        let mut totals = Totals { removed: removed.len() as u64, ..Default::default() };
        let total = to_process.len() as u64;
        self.set_status(lib, ScanPhase::Probing, 0, total);
        // Probes and writes take turns: the time spent writing is taken out
        // of the probing time.
        phase = Instant::now();
        let mut writing = Duration::ZERO;
        if !to_process.is_empty() && !self.transcoder.is_available() {
            self.transcoder.wait_ready(Duration::from_secs(90)).await;
        }

        let mut writer = Writer::new(lib, &sidecars);
        let mut batch: Vec<(FoundFile, ProbeOutcome)> = Vec::with_capacity(WRITE_BATCH);
        let mut processed = 0u64;
        let mut last_emit = Instant::now();
        let mut last_flush = Instant::now();
        let transcoder = self.transcoder.clone();
        // Bonus material goes in after the movies and shows it belongs to.
        let (extras, regular): (Vec<FoundFile>, Vec<FoundFile>) =
            to_process.into_iter().partition(|f| f.extra.is_some());
        for files in [regular, extras] {
            let mut results = futures::stream::iter(files.into_iter().map(|file| {
                let transcoder = transcoder.clone();
                async move {
                    let outcome = match transcoder.probe(&path_str(&file.path)).await {
                        Ok(info) => ProbeOutcome::Ok(Box::new(info)),
                        // ffprobe ran and cannot read the file: don't retry every scan.
                        Err(TranscoderError::Api { code, message, .. }) if code == "ffmpeg_failed" => {
                            tracing::warn!("could not probe {}: {message}", file.path.display());
                            ProbeOutcome::Failed
                        }
                        // Unreachable transcoder, timeout (sleeping NAS), …: try again next scan.
                        Err(err) => {
                            tracing::debug!("probe of {} postponed: {err}", file.path.display());
                            ProbeOutcome::Unavailable
                        }
                    };
                    (file, outcome)
                }
            }))
            .buffer_unordered(PROBE_CONCURRENCY);

            while let Some(result) = results.next().await {
                batch.push(result);
                processed += 1;
                if batch.len() >= WRITE_BATCH || last_flush.elapsed() > Duration::from_millis(750) {
                    let flushing = Instant::now();
                    self.flush(&mut writer, &mut batch, &existing_by_path, &mut totals).await?;
                    writing += flushing.elapsed();
                    last_flush = Instant::now();
                }
                if last_emit.elapsed() > Duration::from_millis(400) {
                    self.set_status(lib, ScanPhase::Probing, processed, total);
                    last_emit = Instant::now();
                }
            }
            let flushing = Instant::now();
            self.flush(&mut writer, &mut batch, &existing_by_path, &mut totals).await?;
            writing += flushing.elapsed();
        }
        self.set_status(lib, ScanPhase::Probing, processed, total);
        phases.probe = phase.elapsed().saturating_sub(writing);

        // Vanished files go, with the items they leave empty.
        phase = Instant::now();
        for chunk in removed.chunks(CHANGE_BATCH) {
            let placeholders = vec!["?"; chunk.len()].join(",");
            let sql = format!("DELETE FROM media_files WHERE id IN ({placeholders})");
            let mut q = sqlx::query(sqlx::AssertSqlSafe(sql.as_str()));
            for id in chunk {
                q = q.bind(id);
            }
            let mut tx = self.db.write.begin().await?;
            if q.execute(&mut *tx).await?.rows_affected() > 0 {
                // Listings show an item's files (sizes, bitrates): one that
                // keeps its other files still lists differently.
                sqlx::query(
                    "INSERT INTO listing_versions (scope, id, version) VALUES ('library', ?, 1)
                     ON CONFLICT (scope, id) DO UPDATE SET version = version + 1",
                )
                .bind(library_id)
                .execute(&mut *tx)
                .await?;
            }
            tx.commit().await?;
        }
        let mut tx = self.db.write.begin().await?;
        cleanup_orphans(&mut tx, library_id).await?;
        tx.commit().await?;
        writing += phase.elapsed();

        // Sidecar subtitles and local artwork can change without their video
        // changing. What changed is worked out from the index, off the async
        // workers and without the writer; only that is written.
        phase = Instant::now();
        let rows: Vec<ArtworkRow> = sqlx::query_as(ARTWORK_ROWS).bind(library_id).fetch_all(&self.db.read).await?;
        let (subtitles, artwork) = {
            let sidecars = sidecars.clone();
            crate::util::blocking(move || {
                // Music has no subtitles (scans don't look for them).
                let subtitles =
                    if kind == LibraryKind::Music { Vec::new() } else { subtitle_changes(unchanged, &sidecars) };
                Ok((subtitles, artwork_changes(rows, &sidecars)))
            })
            .await?
        };
        for chunk in subtitles.chunks(CHANGE_BATCH) {
            let mut tx = self.db.write.begin().await?;
            for change in chunk {
                // Unless something else changed it meanwhile.
                sqlx::query("UPDATE media_files SET external_subs = ? WHERE id = ? AND external_subs = ?")
                    .bind(&change.subs)
                    .bind(change.file_id)
                    .bind(&change.before)
                    .execute(&mut *tx)
                    .await?;
            }
            tx.commit().await?;
        }
        let at = now();
        for chunk in artwork.chunks(CHANGE_BATCH) {
            let mut tx = self.db.write.begin().await?;
            for change in chunk {
                change.save(&mut tx, at).await?;
            }
            tx.commit().await?;
        }
        if subtitles.len() + artwork.len() > 0 {
            tracing::debug!(
                "library {library_id}: subtitles of {} files and artwork of {} items changed",
                subtitles.len(),
                artwork.len()
            );
        }
        // NFO files too, before what they set is applied below.
        if kind != LibraryKind::Music {
            self.read_nfos(lib, &sidecars, &mut walked).await?;
        }
        phases.sidecars = phase.elapsed();

        phase = Instant::now();
        let mut tx = self.db.write.begin().await?;
        update_counts(&mut tx, library_id).await?;
        sqlx::query("UPDATE libraries SET scanned_at = ? WHERE id = ?")
            .bind(now())
            .bind(library_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        // Scanning rewrites titles of unmatched items and local artwork; NFO
        // files win, and hand edits over them.
        crate::overrides::apply_library(&self.db, library_id).await?;
        writing += phase.elapsed();
        self.events.send(ServerEvent::LibraryChanged { library_id });
        phases.write = writing;
        Ok((totals, phases, processed, total))
    }

    /// Reads the NFO files of the library's movies, shows, seasons and
    /// episodes that appeared or changed, and lets go of those that went
    /// (all of them when NFO files aren't read). Like artwork, what changed
    /// is worked out off the async workers and without the writer, and only
    /// that is written.
    async fn read_nfos(&self, lib: &LibraryRow, sidecars: &Arc<Sidecars>, walked: &mut Walked) -> anyhow::Result<()> {
        let rows: Vec<NfoRow> = sqlx::query_as(NFO_ROWS).bind(lib.id).fetch_all(&self.db.read).await?;
        if rows.is_empty() {
            return Ok(());
        }
        let files: Vec<(i64, String)> = sqlx::query_as(NFO_FILES).bind(lib.id).fetch_all(&self.db.read).await?;
        let context = NfoContext {
            read: self.metadata.read_nfo(),
            roots: lib.paths(),
            unavailable: walked.offline_roots.iter().chain(&walked.unreadable).cloned().collect(),
            listed: std::mem::take(&mut walked.nfo_files),
            region: crate::ratings::region(&self.db).await?,
        };
        let changes = {
            let sidecars = sidecars.clone();
            crate::util::blocking(move || Ok(nfo_changes(rows, files, &sidecars, &context))).await?
        };
        for chunk in changes.chunks(CHANGE_BATCH) {
            let mut tx = self.db.write.begin().await?;
            for change in chunk {
                change.save(&mut tx).await?;
            }
            tx.commit().await?;
        }
        let read = changes.iter().filter(|c| c.after.is_some()).count();
        if !changes.is_empty() {
            tracing::info!("library {}: {read} NFO files read, {} gone", lib.id, changes.len() - read);
        }
        Ok(())
    }

    async fn flush(
        &self,
        writer: &mut Writer<'_>,
        batch: &mut Vec<(FoundFile, ProbeOutcome)>,
        existing: &HashMap<&str, &KnownFile>,
        totals: &mut Totals,
    ) -> anyhow::Result<()> {
        if batch.is_empty() {
            return Ok(());
        }
        let mut tx = self.db.write.begin().await?;
        for (file, outcome) in batch.drain(..) {
            let known = existing.get(path_str(&file.path).as_str());
            // A forced rescan while the transcoder is down must not replace
            // good probe data (and music tags) with nothing.
            if matches!(outcome, ProbeOutcome::Unavailable)
                && known.is_some_and(|k| k.size == file.size && k.mtime == file.mtime)
            {
                continue;
            }
            let is_new = known.is_none();
            match writer.write(&mut tx, &file, outcome).await {
                Ok(()) if is_new => totals.added += 1,
                Ok(()) => totals.updated += 1,
                Err(err) => tracing::warn!("could not save {}: {err:#}", file.path.display()),
            }
        }
        tx.commit().await?;
        Ok(())
    }
}

/// A media file of the library as the database knows it.
#[derive(sqlx::FromRow)]
struct KnownFile {
    id: i64,
    path: String,
    size: i64,
    mtime: i64,
    probed: bool,
    external_subs: String,
}

enum ProbeOutcome {
    Ok(Box<MediaInfo>),
    /// The transcoder could not be reached or timed out: store the file
    /// unprobed (unless it is already known), retry next scan.
    Unavailable,
    /// ffprobe rejected the file: store it as probed so we don't retry every scan.
    Failed,
}

/// Writes items for scanned files, caching container ids for the scan.
struct Writer<'a> {
    library_id: i64,
    kind: LibraryKind,
    sidecars: &'a Sidecars,
    containers: HashMap<String, i64>,
}

struct NewItem {
    parent_id: Option<i64>,
    grandparent_id: Option<i64>,
    kind: &'static str,
    key: String,
    title: String,
    original_title: Option<String>,
    year: Option<i32>,
    idx: Option<i32>,
    disc: Option<i32>,
    duration_ms: Option<i64>,
    path: Option<String>,
    release_date: Option<String>,
    genres: Option<String>,
    tmdb_id: Option<i64>,
    imdb_id: Option<String>,
}

impl NewItem {
    fn new(kind: &'static str, key: String, title: String) -> Self {
        Self {
            parent_id: None,
            grandparent_id: None,
            kind,
            key,
            title,
            original_title: None,
            year: None,
            idx: None,
            disc: None,
            duration_ms: None,
            path: None,
            release_date: None,
            genres: None,
            tmdb_id: None,
            imdb_id: None,
        }
    }
}

impl<'a> Writer<'a> {
    fn new(lib: &LibraryRow, sidecars: &'a Sidecars) -> Self {
        Self { library_id: lib.id, kind: lib.kind(), sidecars, containers: HashMap::new() }
    }

    async fn write(
        &mut self,
        conn: &mut SqliteConnection,
        file: &FoundFile,
        outcome: ProbeOutcome,
    ) -> anyhow::Result<()> {
        let (info, probed) = match outcome {
            ProbeOutcome::Ok(info) => (Some(*info), true),
            ProbeOutcome::Unavailable => (None, false),
            ProbeOutcome::Failed => (None, true),
        };
        let duration = info.as_ref().and_then(|i| i.duration_ms);
        let item_id = match (&file.extra, self.kind) {
            (Some(extra), _) => match self.write_extra(conn, file, extra, duration).await? {
                Some(id) => id,
                // Its movie or show is not in the library (yet): next scan.
                None => return Ok(()),
            },
            (None, LibraryKind::Movies) => self.write_movie(conn, file, duration).await?,
            (None, LibraryKind::Shows) => self.write_episode(conn, file, duration).await?,
            (None, LibraryKind::Music) => self.write_track(conn, file, info.as_ref(), duration).await?,
        };
        let subs = if self.kind == LibraryKind::Music { Vec::new() } else { self.sidecars.subtitles(&file.path) };
        upsert_file(conn, self.library_id, item_id, file, info.as_ref(), &subs, probed).await
    }

    async fn container(&mut self, conn: &mut SqliteConnection, item: NewItem) -> anyhow::Result<i64> {
        if let Some(id) = self.containers.get(&item.key) {
            return Ok(*id);
        }
        let key = item.key.clone();
        let id = upsert_item(conn, self.library_id, &item).await?;
        self.containers.insert(key, id);
        Ok(id)
    }

    /// Bonus material, as an item under its movie or show.
    async fn write_extra(
        &mut self,
        conn: &mut SqliteConnection,
        file: &FoundFile,
        extra: &Extra,
        duration: Option<i64>,
    ) -> anyhow::Result<Option<i64>> {
        let Some(owner) = self.extra_owner(conn, extra).await? else {
            tracing::debug!("no movie or show for the extra {}", file.path.display());
            return Ok(None);
        };
        let stem = file.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let mut item = NewItem::new("extra", format!("extra:{}", path_str(&file.path)), extra_title(&stem, extra));
        item.parent_id = Some(owner);
        item.duration_ms = duration;
        let id = upsert_item(conn, self.library_id, &item).await?;
        sqlx::query("UPDATE items SET extra_type = ? WHERE id = ?")
            .bind(extra.kind.as_str())
            .bind(id)
            .execute(&mut *conn)
            .await?;
        Ok(Some(id))
    }

    /// The movie or show bonus material belongs to: the movie whose file is
    /// in the owner folder (by name for `…-trailer` files, or the folder's
    /// only movie), or the show of that folder (or of that season folder).
    async fn extra_owner(&self, conn: &mut SqliteConnection, extra: &Extra) -> sqlx::Result<Option<i64>> {
        let dir = path_str(&extra.owner_dir);
        match self.kind {
            LibraryKind::Movies => {
                let candidates: Vec<(i64, String)> = sqlx::query_as(
                    "SELECT f.item_id, f.path FROM media_files f JOIN items i ON i.id = f.item_id
                     WHERE f.library_id = ? AND i.kind = 'movie' AND f.path LIKE ? ESCAPE '^'",
                )
                .bind(self.library_id)
                .bind(format!("{}%", crate::store::like_escape(&dir)))
                .fetch_all(&mut *conn)
                .await?;
                let direct: Vec<(i64, String)> = candidates
                    .into_iter()
                    .filter(|(_, path)| Path::new(path).parent() == Some(extra.owner_dir.as_path()))
                    .collect();
                if let Some(stem) = &extra.owner_stem {
                    let named = direct.iter().find(|(_, path)| {
                        Path::new(path).file_stem().is_some_and(|s| s.to_string_lossy().eq_ignore_ascii_case(stem))
                    });
                    if let Some((id, _)) = named {
                        return Ok(Some(*id));
                    }
                }
                let movies: HashSet<i64> = direct.iter().map(|(id, _)| *id).collect();
                Ok(if movies.len() == 1 { movies.into_iter().next() } else { None })
            }
            LibraryKind::Shows => {
                let row: Option<(i64, String, Option<i64>)> = sqlx::query_as(
                    "SELECT id, kind, parent_id FROM items
                     WHERE library_id = ? AND kind IN ('show', 'season') AND path = ? ORDER BY kind = 'season' LIMIT 1",
                )
                .bind(self.library_id)
                .bind(&dir)
                .fetch_optional(&mut *conn)
                .await?;
                Ok(row.map(|(id, kind, parent)| if kind == "season" { parent.unwrap_or(id) } else { id }))
            }
            LibraryKind::Music => Ok(None),
        }
    }

    async fn write_movie(
        &mut self,
        conn: &mut SqliteConnection,
        file: &FoundFile,
        duration: Option<i64>,
    ) -> anyhow::Result<i64> {
        let name = parse_movie(&file.path, &file.root);
        let key = match name.tmdb_id {
            Some(id) => format!("movie:tmdb:{id}"),
            None => format!("movie:{}:{}", normalize_key(&name.title), name.year.unwrap_or(0)),
        };
        let mut item = NewItem::new("movie", key, name.title);
        item.year = name.year;
        item.duration_ms = duration;
        item.tmdb_id = name.tmdb_id;
        item.imdb_id = name.imdb_id;
        item.path = file.path.parent().filter(|p| *p != file.root).map(path_str);
        Ok(upsert_item(conn, self.library_id, &item).await?)
    }

    async fn write_episode(
        &mut self,
        conn: &mut SqliteConnection,
        file: &FoundFile,
        duration: Option<i64>,
    ) -> anyhow::Result<i64> {
        let name = parse_episode(&file.path, &file.root);
        let show_key = match name.show_tmdb_id {
            Some(id) => format!("show:tmdb:{id}"),
            None => format!("show:{}:{}", normalize_key(&name.show), name.show_year.unwrap_or(0)),
        };
        let mut show = NewItem::new("show", show_key, name.show.clone());
        show.year = name.show_year;
        show.tmdb_id = name.show_tmdb_id;
        show.path = name.show_dir.as_deref().map(path_str);
        let show_id = self.container(conn, show).await?;

        let mut season =
            NewItem::new("season", format!("season:{show_id}:{}", name.season), season_title(name.season.into()));
        season.parent_id = Some(show_id);
        season.idx = Some(name.season);
        season.path = name.season_dir.as_deref().map(path_str);
        let season_id = self.container(conn, season).await?;

        let title = episode_title(&name, &file.path);
        let key = match name.episode {
            Some(number) => format!("episode:{season_id}:{number}"),
            // No number in the name: the title tells the episode apart, so
            // a season's unnumbered episodes never merge into one. Metadata
            // may number it by its title (see `metadata`).
            None => format!("episode:{season_id}:title:{}", normalize_key(&title)),
        };
        let mut episode = NewItem::new("episode", key, title);
        episode.parent_id = Some(season_id);
        episode.grandparent_id = Some(show_id);
        episode.idx = name.episode;
        episode.duration_ms = duration;
        episode.release_date = name.air_date.clone();
        episode.year = name.air_date.as_deref().and_then(|d| d.get(..4)).and_then(|y| y.parse().ok());
        Ok(upsert_item(conn, self.library_id, &episode).await?)
    }

    async fn write_track(
        &mut self,
        conn: &mut SqliteConnection,
        file: &FoundFile,
        info: Option<&MediaInfo>,
        duration: Option<i64>,
    ) -> anyhow::Result<i64> {
        let empty = Default::default();
        let tags = info.map(|i| &i.tags).unwrap_or(&empty);
        let name = parse_track(&file.path, &file.root, tags);

        let mut artist =
            NewItem::new("artist", format!("artist:{}", normalize_key(&name.album_artist)), name.album_artist.clone());
        artist.path = name.artist_dir.as_deref().map(path_str);
        let artist_id = self.container(conn, artist).await?;

        let genres = name.genre.as_ref().map(|g| serde_json::json!([g]).to_string());
        let mut album =
            NewItem::new("album", format!("album:{artist_id}:{}", normalize_key(&name.album)), name.album.clone());
        album.parent_id = Some(artist_id);
        album.year = name.year;
        album.path = Some(path_str(&name.album_dir));
        album.genres = genres.clone();
        let album_id = self.container(conn, album).await?;

        let mut track = NewItem::new("track", format!("track:{}", path_str(&file.path)), name.title.clone());
        track.parent_id = Some(album_id);
        track.grandparent_id = Some(artist_id);
        track.idx = name.track;
        track.disc = name.disc;
        track.year = name.year;
        track.duration_ms = duration;
        track.genres = genres;
        track.original_title = (name.artist != name.album_artist).then(|| name.artist.clone());
        Ok(upsert_item(conn, self.library_id, &track).await?)
    }
}

/// A season's title before metadata names it.
fn season_title(number: i64) -> String {
    if number == 0 { "Specials".to_owned() } else { format!("Season {number}") }
}

/// An episode's title from its file's name: the title in it, else its
/// number (`Episode 3`, `Episodes 1-2`), else the name itself.
fn episode_title(name: &EpisodeName, file: &Path) -> String {
    name.title.clone().unwrap_or_else(|| match (name.episode, name.episode_end) {
        (Some(number), Some(end)) => format!("Episodes {number}-{end}"),
        (Some(number), None) => format!("Episode {number}"),
        (None, _) => file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default(),
    })
}

async fn upsert_item(conn: &mut SqliteConnection, library_id: i64, item: &NewItem) -> sqlx::Result<i64> {
    let now = now();
    // Titles and years come from the files until a metadata agent matched the
    // item; music tags keep them (MusicBrainz only fills in what they lack).
    sqlx::query_scalar(
        "INSERT INTO items (id, library_id, parent_id, grandparent_id, kind, key, title, sort_title, original_title, year, idx,
                            disc, duration_ms, path, release_date, genres, tmdb_id, imdb_id, added_at, updated_at)
         VALUES ((SELECT last + 1 FROM item_ids), ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(library_id, key) DO UPDATE SET
            parent_id = excluded.parent_id,
            grandparent_id = excluded.grandparent_id,
            title = CASE WHEN (items.metadata_at IS NULL OR items.kind IN ('artist', 'album')) AND items.metadata_locked = 0 THEN excluded.title ELSE items.title END,
            sort_title = CASE WHEN (items.metadata_at IS NULL OR items.kind IN ('artist', 'album')) AND items.metadata_locked = 0 THEN excluded.sort_title ELSE items.sort_title END,
            original_title = CASE WHEN (items.metadata_at IS NULL OR items.kind IN ('artist', 'album')) AND items.metadata_locked = 0 THEN excluded.original_title ELSE items.original_title END,
            year = CASE WHEN (items.metadata_at IS NULL OR items.kind IN ('artist', 'album')) AND items.metadata_locked = 0 THEN COALESCE(excluded.year, items.year) ELSE items.year END,
            -- A name without a number keeps the one metadata gave it by title.
            idx = COALESCE(excluded.idx, items.idx),
            disc = excluded.disc,
            duration_ms = COALESCE(excluded.duration_ms, items.duration_ms),
            path = COALESCE(excluded.path, items.path),
            release_date = COALESCE(excluded.release_date, items.release_date),
            genres = COALESCE(excluded.genres, items.genres),
            tmdb_id = COALESCE(items.tmdb_id, excluded.tmdb_id),
            imdb_id = COALESCE(items.imdb_id, excluded.imdb_id),
            updated_at = excluded.updated_at
         RETURNING id",
    )
    .bind(library_id)
    .bind(item.parent_id)
    .bind(item.grandparent_id)
    .bind(item.kind)
    .bind(&item.key)
    .bind(&item.title)
    .bind(sort_title(&item.title))
    .bind(&item.original_title)
    .bind(item.year)
    .bind(item.idx)
    .bind(item.disc)
    .bind(item.duration_ms)
    .bind(&item.path)
    .bind(&item.release_date)
    .bind(&item.genres)
    .bind(item.tmdb_id)
    .bind(&item.imdb_id)
    .bind(now)
    .bind(now)
    .fetch_one(conn)
    .await
}

async fn upsert_file(
    conn: &mut SqliteConnection,
    library_id: i64,
    item_id: i64,
    file: &FoundFile,
    info: Option<&MediaInfo>,
    subs: &[ExternalSub],
    probed: bool,
) -> anyhow::Result<()> {
    let video = info.and_then(|i| i.video());
    let audio = info.and_then(|i| i.default_audio());
    let container = info
        .and_then(|i| i.container.clone())
        .or_else(|| file.path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()));
    let now = now();
    sqlx::query(
        "INSERT INTO media_files (id, item_id, library_id, path, size, mtime, container, duration_ms, bitrate, width,
                                  height, video_codec, dynamic_range, audio_codec, media_info, external_subs, probed,
                                  added_at, updated_at)
         VALUES ((SELECT last + 1 FROM file_ids), ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(path) DO UPDATE SET
            item_id = excluded.item_id, library_id = excluded.library_id, size = excluded.size, mtime = excluded.mtime,
            container = excluded.container, duration_ms = excluded.duration_ms, bitrate = excluded.bitrate,
            width = excluded.width, height = excluded.height, video_codec = excluded.video_codec,
            dynamic_range = excluded.dynamic_range,
            audio_codec = excluded.audio_codec, media_info = excluded.media_info, external_subs = excluded.external_subs,
            probed = excluded.probed, updated_at = excluded.updated_at,
            -- A changed file gets fingerprinted again.
            fingerprint = CASE WHEN media_files.size = excluded.size AND media_files.mtime = excluded.mtime
                               THEN media_files.fingerprint END",
    )
    .bind(item_id)
    .bind(library_id)
    .bind(path_str(&file.path))
    .bind(file.size)
    .bind(file.mtime)
    .bind(container)
    .bind(info.and_then(|i| i.duration_ms))
    .bind(info.and_then(|i| i.bitrate))
    .bind(video.and_then(|v| v.width))
    .bind(video.and_then(|v| v.height))
    .bind(video.map(|v| v.codec.clone()))
    .bind(video.map(|v| v.dynamic_range().as_str()))
    .bind(audio.map(|a| a.codec.clone()))
    .bind(info.map(serde_json::to_string).transpose()?)
    .bind(serde_json::to_string(subs)?)
    .bind(probed)
    .bind(now)
    .bind(now)
    .execute(conn)
    .await?;
    Ok(())
}

async fn cleanup_orphans(conn: &mut SqliteConnection, library_id: i64) -> sqlx::Result<()> {
    sqlx::query(
        "DELETE FROM items WHERE library_id = ? AND kind IN ('movie', 'episode', 'track', 'extra')
         AND NOT EXISTS (SELECT 1 FROM media_files f WHERE f.item_id = items.id)",
    )
    .bind(library_id)
    .execute(&mut *conn)
    .await?;
    for kinds in ["('season', 'album')", "('show', 'artist')"] {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DELETE FROM items WHERE library_id = ? AND kind IN {kinds}
             AND NOT EXISTS (SELECT 1 FROM items c WHERE c.parent_id = items.id AND c.kind != 'extra')"
        )))
        .bind(library_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn update_counts(conn: &mut SqliteConnection, library_id: i64) -> sqlx::Result<()> {
    // `+kind`: children are counted through the parent indexes. Before the
    // first `PRAGMA optimize` SQLite would otherwise go through every episode
    // of the library for each show (seconds of holding the writer).
    // Only counts that changed are written: every write to an item makes
    // apps fetch the library's listings again (see `listing_versions`).
    let statements = [
        "UPDATE items SET leaf_count = c.leaves, child_count = c.children
         FROM (SELECT p.id,
                 (SELECT COUNT(*) FROM items e WHERE e.grandparent_id = p.id AND +e.kind = 'episode') AS leaves,
                 (SELECT COUNT(*) FROM items s WHERE s.parent_id = p.id AND +s.kind = 'season') AS children
               FROM items p WHERE p.library_id = ? AND p.kind = 'show') AS c
         WHERE items.id = c.id AND (items.leaf_count IS NOT c.leaves OR items.child_count IS NOT c.children)",
        "UPDATE items SET leaf_count = c.leaves
         FROM (SELECT p.id, (SELECT COUNT(*) FROM items e WHERE e.parent_id = p.id) AS leaves
               FROM items p WHERE p.library_id = ? AND p.kind = 'season') AS c
         WHERE items.id = c.id AND items.leaf_count IS NOT c.leaves",
        "UPDATE items SET leaf_count = c.leaves, child_count = c.children
         FROM (SELECT p.id,
                 (SELECT COUNT(*) FROM items t WHERE t.grandparent_id = p.id AND +t.kind = 'track') AS leaves,
                 (SELECT COUNT(*) FROM items a WHERE a.parent_id = p.id) AS children
               FROM items p WHERE p.library_id = ? AND p.kind = 'artist') AS c
         WHERE items.id = c.id AND (items.leaf_count IS NOT c.leaves OR items.child_count IS NOT c.children)",
        "UPDATE items SET leaf_count = c.leaves, duration_ms = c.duration
         FROM (SELECT p.id,
                 (SELECT COUNT(*) FROM items t WHERE t.parent_id = p.id) AS leaves,
                 (SELECT SUM(t.duration_ms) FROM items t WHERE t.parent_id = p.id) AS duration
               FROM items p WHERE p.library_id = ? AND p.kind = 'album') AS c
         WHERE items.id = c.id AND (items.leaf_count IS NOT c.leaves OR items.duration_ms IS NOT c.duration)",
    ];
    for sql in statements {
        sqlx::query(sql).bind(library_id).execute(&mut *conn).await?;
    }
    Ok(())
}

/// The sidecar subtitles of a file whose video did not change.
struct SubtitleChange {
    file_id: i64,
    /// What the database has, and what it gets.
    before: String,
    subs: String,
}

/// Subtitles appear, change and go without their video changing: what the
/// index finds for `files` (id, path, stored `external_subs`) where it is
/// not what the database has.
fn subtitle_changes(files: Vec<(i64, PathBuf, String)>, sidecars: &Sidecars) -> Vec<SubtitleChange> {
    files
        .into_iter()
        .filter_map(|(file_id, path, before)| {
            let subs = serde_json::to_string(&sidecars.subtitles(&path)).ok()?;
            (subs != before).then_some(SubtitleChange { file_id, before, subs })
        })
        .collect()
}

/// Items and the files their local artwork is looked for next to.
const ARTWORK_ROWS: &str = "SELECT i.id, i.kind, i.path, i.idx, i.poster, i.backdrop, i.thumb, p.path AS parent_path,
        (SELECT f.path FROM media_files f WHERE f.item_id = i.id ORDER BY f.id LIMIT 1) AS file_path
     FROM items i LEFT JOIN items p ON p.id = i.parent_id
     WHERE i.library_id = ? AND i.kind IN ('movie', 'show', 'season', 'episode', 'artist', 'album')";

#[derive(sqlx::FromRow)]
struct ArtworkRow {
    id: i64,
    kind: String,
    path: Option<String>,
    idx: Option<i64>,
    poster: Option<String>,
    backdrop: Option<String>,
    thumb: Option<String>,
    parent_path: Option<String>,
    file_path: Option<String>,
}

/// A picture of an item that changes: what the database has, what it gets.
type Swap = Option<(Option<String>, Option<String>)>;

/// New local artwork of an item.
struct ArtworkChange {
    id: i64,
    poster: Swap,
    backdrop: Swap,
    thumb: Swap,
}

impl ArtworkChange {
    async fn save(&self, conn: &mut SqliteConnection, now: i64) -> sqlx::Result<()> {
        let mut query = sqlx::query(
            "UPDATE items SET
                poster = CASE WHEN ? AND poster IS ? THEN ? ELSE poster END,
                backdrop = CASE WHEN ? AND backdrop IS ? THEN ? ELSE backdrop END,
                thumb = CASE WHEN ? AND thumb IS ? THEN ? ELSE thumb END,
                updated_at = ?
             WHERE id = ?",
        );
        // Each picture only changes if it is still what the scan saw: an edit
        // or a metadata refresh meanwhile wins.
        for swap in [&self.poster, &self.backdrop, &self.thumb] {
            let (before, after) = swap.clone().unwrap_or_default();
            query = query.bind(swap.is_some()).bind(before).bind(after);
        }
        query.bind(now).bind(self.id).execute(conn).await?;
        Ok(())
    }
}

/// Local artwork (poster.jpg, fanart.jpg, season01-poster.jpg, …) that is
/// not what the items have. Local files win over downloaded artwork;
/// vanished local files are cleared.
fn artwork_changes(rows: Vec<ArtworkRow>, sidecars: &Sidecars) -> Vec<ArtworkChange> {
    let lookup = |dir: &str, bases: &[String]| -> Option<String> {
        sidecars.image(Path::new(dir), bases).map(|path| path_str(&path))
    };
    let strings = |names: &[&str]| names.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let mut changes = Vec::new();
    for row in rows {
        let file_stem_bases = |suffixes: &[&str]| -> Vec<String> {
            let stem = row
                .file_path
                .as_deref()
                .and_then(|p| Path::new(p).file_stem())
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            suffixes.iter().map(|s| format!("{stem}{s}")).collect()
        };
        let file_dir = row.file_path.as_deref().and_then(|p| Path::new(p).parent()).map(path_str);
        let (poster, backdrop, thumb) = match row.kind.as_str() {
            "movie" => {
                let dir = file_dir.clone().unwrap_or_default();
                let poster = lookup(&dir, &file_stem_bases(&["-poster", "-cover", ""])).or_else(|| {
                    row.path
                        .as_deref()
                        .and_then(|p| lookup(p, &strings(&["poster", "folder", "cover", "movie", "default"])))
                });
                let backdrop = lookup(&dir, &file_stem_bases(&["-fanart", "-backdrop"])).or_else(|| {
                    row.path.as_deref().and_then(|p| lookup(p, &strings(&["fanart", "backdrop", "background", "art"])))
                });
                (poster, backdrop, None)
            }
            "show" | "artist" => {
                let path = row.path.as_deref().unwrap_or_default();
                let names: &[&str] = if row.kind == "show" {
                    &["poster", "folder", "show", "cover"]
                } else {
                    &["artist", "folder", "poster", "cover"]
                };
                (
                    lookup(path, &strings(names)),
                    lookup(path, &strings(&["fanart", "backdrop", "background", "art"])),
                    None,
                )
            }
            "season" => {
                let n = row.idx.unwrap_or(1);
                let in_show = row.parent_path.as_deref().and_then(|show| {
                    let names = if n == 0 {
                        vec!["season-specials-poster".to_owned(), "season00-poster".to_owned()]
                    } else {
                        vec![format!("season{n:02}-poster"), format!("season{n}-poster")]
                    };
                    lookup(show, &names)
                });
                let in_dir = row.path.as_deref().and_then(|p| lookup(p, &strings(&["poster", "folder", "cover"])));
                (in_dir.or(in_show), None, None)
            }
            "album" => {
                let path = row.path.as_deref().unwrap_or_default();
                (lookup(path, &strings(&["cover", "folder", "front", "album", "poster"])), None, None)
            }
            "episode" => {
                let dir = file_dir.clone().unwrap_or_default();
                (None, None, lookup(&dir, &file_stem_bases(&["-thumb", ""])))
            }
            _ => (None, None, None),
        };

        let decide = |current: &Option<String>, local: Option<String>| -> Swap {
            let after = match (current, local) {
                (Some(cur), Some(new)) if *cur == new => return None,
                (_, Some(new)) => Some(new),
                // Clear local artwork that vanished, or became a link to
                // something else (downloaded art lives in the blob store).
                (Some(cur), None)
                    if !crate::store::is_blob(cur)
                        && !(Path::new(cur).exists() && super::walk::leads_to_its_kind(Path::new(cur))) =>
                {
                    None
                }
                _ => return None,
            };
            Some((current.clone(), after))
        };
        let change = ArtworkChange {
            id: row.id,
            poster: decide(&row.poster, poster),
            backdrop: decide(&row.backdrop, backdrop),
            thumb: decide(&row.thumb, thumb),
        };
        if change.poster.is_some() || change.backdrop.is_some() || change.thumb.is_some() {
            changes.push(change);
        }
    }
    changes
}

/// Movies, shows, seasons and episodes of a library, with the NFO file each
/// had at the last scan.
const NFO_ROWS: &str = "SELECT i.id, i.kind, i.path, i.idx, i.parent_id, i.grandparent_id, s.idx AS season, i.title,
        i.tmdb_id, i.imdb_id, i.tvdb_id, i.tvmaze_id, i.metadata_locked,
        EXISTS (SELECT 1 FROM item_overrides o WHERE o.item_id = i.id AND o.field = 'match') AS rematched,
        n.path AS nfo_path, n.size AS nfo_size, n.mtime AS nfo_mtime, n.ids AS nfo_ids, n.fields AS nfo_fields,
        n.locked AS nfo_locked
     FROM items i LEFT JOIN items s ON s.id = i.parent_id AND i.kind = 'episode'
     LEFT JOIN item_nfo n ON n.item_id = i.id
     WHERE i.library_id = ? AND i.kind IN ('movie', 'show', 'season', 'episode')";

/// The files of a library's movies and episodes, which NFO files are found
/// next to.
const NFO_FILES: &str = "SELECT f.item_id, f.path FROM media_files f JOIN items i ON i.id = f.item_id
     WHERE f.library_id = ? AND i.kind IN ('movie', 'episode') ORDER BY f.id";

#[derive(sqlx::FromRow)]
struct NfoRow {
    id: i64,
    kind: String,
    path: Option<String>,
    idx: Option<i64>,
    parent_id: Option<i64>,
    grandparent_id: Option<i64>,
    /// An episode's season number.
    season: Option<i64>,
    title: String,
    #[sqlx(flatten)]
    ids: ItemIds,
    metadata_locked: bool,
    /// Matched with Fix Match, which an NFO file's ids don't undo.
    rematched: bool,
    nfo_path: Option<String>,
    nfo_size: Option<i64>,
    nfo_mtime: Option<i64>,
    nfo_ids: Option<String>,
    nfo_fields: Option<String>,
    nfo_locked: Option<bool>,
}

impl NfoRow {
    /// The NFO file as the last scan read it.
    fn stored(&self) -> Option<ItemNfo> {
        Some(ItemNfo {
            path: self.nfo_path.clone()?,
            size: self.nfo_size?,
            mtime: self.nfo_mtime?,
            ids: self.nfo_ids.as_deref().and_then(|ids| serde_json::from_str(ids).ok()).unwrap_or_default(),
            fields: self.nfo_fields.as_deref().and_then(|f| serde_json::from_str(f).ok()).unwrap_or_default(),
            locked: self.nfo_locked.unwrap_or_default(),
        })
    }

    /// The item whose refresh fetches this one: a season's or an episode's
    /// show.
    fn refreshed_with(&self) -> Option<i64> {
        match self.kind.as_str() {
            "season" => self.parent_id,
            "episode" => self.grandparent_id,
            _ => Some(self.id),
        }
    }
}

/// An item's ids at the metadata sources.
#[derive(Debug, Clone, Default, PartialEq, sqlx::FromRow)]
pub(crate) struct ItemIds {
    pub tmdb_id: Option<i64>,
    pub imdb_id: Option<String>,
    pub tvdb_id: Option<i64>,
    pub tvmaze_id: Option<i64>,
}

/// Whether an NFO file describes the title its item is matched to, so that
/// its fields and lockdata are the item's: always, but for a movie or a
/// show matched with Fix Match to a title none of the file's ids name.
/// `overrides::RATING_LOCKED` says the same in SQL.
pub(crate) fn nfo_describes(kind: &str, item: &ItemIds, nfo: &NfoIds) -> bool {
    let named = ItemIds::of_nfo(kind, nfo);
    !matches!(kind, "movie" | "show") || named == ItemIds::default() || item.shares_with(&named)
}

/// Which ids an item of `kind` takes from its NFO file (TMDB, IMDb,
/// TheTVDB, TVmaze): a movie's TMDB and IMDb ids, a show's from every
/// source, an episode's own, none of a season.
fn nfo_ids_taken(kind: &str) -> [bool; 4] {
    match kind {
        "movie" => [true, true, false, false],
        "show" => [true, true, true, true],
        "episode" => [true, true, true, false],
        _ => [false; 4],
    }
}

impl ItemIds {
    /// The ids an item of `kind` takes from its NFO file.
    fn of_nfo(kind: &str, nfo: &NfoIds) -> Self {
        let [tmdb, imdb, tvdb, tvmaze] = nfo_ids_taken(kind);
        Self {
            tmdb_id: nfo.tmdb.filter(|_| tmdb),
            imdb_id: nfo.imdb.clone().filter(|_| imdb),
            tvdb_id: nfo.tvdb.filter(|_| tvdb),
            tvmaze_id: nfo.tvmaze.filter(|_| tvmaze),
        }
    }

    /// Whether the two have an id in common: they name the same title.
    fn shares_with(&self, other: &Self) -> bool {
        fn same<T: PartialEq>(a: &Option<T>, b: &Option<T>) -> bool {
            matches!((a, b), (Some(a), Some(b)) if a == b)
        }
        same(&self.tmdb_id, &other.tmdb_id)
            || same(&self.imdb_id, &other.imdb_id)
            || same(&self.tvdb_id, &other.tvdb_id)
            || same(&self.tvmaze_id, &other.tvmaze_id)
    }

    /// Whether the two have different ids of the same kind: they name
    /// different titles.
    fn conflicts_with(&self, other: &Self) -> bool {
        fn differs<T: PartialEq>(a: &Option<T>, b: &Option<T>) -> bool {
            matches!((a, b), (Some(a), Some(b)) if a != b)
        }
        differs(&self.tmdb_id, &other.tmdb_id)
            || differs(&self.imdb_id, &other.imdb_id)
            || differs(&self.tvdb_id, &other.tvdb_id)
            || differs(&self.tvmaze_id, &other.tvmaze_id)
    }

    /// What an item of `kind` holding these ids holds once it takes the ids
    /// of its NFO file, if that changes anything. A movie's or a show's ids
    /// decide its match: unless one of them is the NFO file's, they came
    /// from the names or a search, maybe for another title, and give way to
    /// the NFO file's, from which the source finds the rest. An episode's
    /// ids decide nothing: those it lacks are added, and only one it holds
    /// differently means another title, whose other ids go too.
    fn with_nfo(&self, kind: &str, nfo: &NfoIds) -> Option<Self> {
        let wanted = Self::of_nfo(kind, nfo);
        if wanted == Self::default() {
            return None;
        }
        let keep = match kind {
            "movie" | "show" => self.shares_with(&wanted),
            _ => !self.conflicts_with(&wanted),
        };
        let [tmdb, imdb, tvdb, tvmaze] = nfo_ids_taken(kind);
        fn pick<T>(keep: bool, taken: bool, want: Option<T>, have: Option<T>) -> Option<T> {
            if keep || !taken { want.or(have) } else { want }
        }
        let next = Self {
            tmdb_id: pick(keep, tmdb, wanted.tmdb_id, self.tmdb_id),
            imdb_id: pick(keep, imdb, wanted.imdb_id, self.imdb_id.clone()),
            tvdb_id: pick(keep, tvdb, wanted.tvdb_id, self.tvdb_id),
            tvmaze_id: pick(keep, tvmaze, wanted.tvmaze_id, self.tvmaze_id),
        };
        (next != *self).then_some(next)
    }

    /// What an item of `kind` holding these ids holds once its NFO file no
    /// longer gives the ids `gone`, if that changes anything. A movie or a
    /// show still matched by them goes back to the ids in its names
    /// (`named`), and the source finds it as it would without the file.
    fn without_nfo(&self, kind: &str, gone: &NfoIds, named: impl FnOnce() -> Self) -> Option<Self> {
        if !matches!(kind, "movie" | "show") || !self.shares_with(&Self::of_nfo(kind, gone)) {
            return None;
        }
        let next = named();
        (next != *self).then_some(next)
    }
}

/// An item's NFO file, and what the scan read in it.
#[derive(Debug, Clone, PartialEq)]
struct ItemNfo {
    path: String,
    size: i64,
    mtime: i64,
    ids: NfoIds,
    fields: NfoFields,
    locked: bool,
}

impl ItemNfo {
    /// Whether it sets anything at all (an NFO file of the wrong kind, or
    /// release notes, doesn't).
    fn sets_anything(&self) -> bool {
        self.locked || self.ids != NfoIds::default() || self.fields != NfoFields::default()
    }
}

/// What the NFO pass of a scan needs besides the items.
struct NfoContext {
    /// Whether NFO files are read (the `read_nfo` setting).
    read: bool,
    roots: Vec<PathBuf>,
    /// Folders the walk couldn't read: NFO files in them haven't gone.
    unavailable: Vec<PathBuf>,
    /// The size and modification time of the NFO files the walk listed.
    listed: HashMap<PathBuf, (i64, i64)>,
    /// The server's country, whose rating is taken of several countries'.
    region: String,
}

/// An item whose NFO file appeared, changed or went.
struct NfoChange {
    item_id: i64,
    /// The item whose online metadata has to be fetched again: the item, or
    /// a season's or an episode's show.
    refresh: Option<i64>,
    /// As the database has it, and as it is now.
    before: Option<ItemNfo>,
    after: Option<ItemNfo>,
    /// The item's ids as seen, and what they become.
    ids: Option<(ItemIds, ItemIds)>,
    /// Columns that the NFO file set and no longer does: the value it set,
    /// and what the item gets back (its names', or nothing until the next
    /// refresh fills it).
    reverts: Vec<(&'static str, String, Option<String>)>,
    /// Jellyfin's lockdata, as the item has it and as the NFO file has it.
    locked: Option<(bool, bool)>,
}

impl NfoChange {
    async fn save(&self, conn: &mut SqliteConnection) -> sqlx::Result<()> {
        if let Some((seen, next)) = &self.ids {
            // Only if the item's ids are still as seen, and it wasn't matched
            // by hand meanwhile. Otherwise none of the change is kept: the
            // next scan finds the NFO file new or changed again and works it
            // out from the ids the item has then.
            let set = sqlx::query(
                "UPDATE items SET tmdb_id = ?, imdb_id = ?, tvdb_id = ?, tvmaze_id = ?
                 WHERE id = ? AND tmdb_id IS ? AND imdb_id IS ? AND tvdb_id IS ? AND tvmaze_id IS ?
                   AND NOT EXISTS (SELECT 1 FROM item_overrides o WHERE o.item_id = items.id AND o.field = ?)",
            )
            .bind(next.tmdb_id)
            .bind(&next.imdb_id)
            .bind(next.tvdb_id)
            .bind(next.tvmaze_id)
            .bind(self.item_id)
            .bind(seen.tmdb_id)
            .bind(&seen.imdb_id)
            .bind(seen.tvdb_id)
            .bind(seen.tvmaze_id)
            .bind(crate::overrides::MATCH)
            .execute(&mut *conn)
            .await?;
            if set.rows_affected() == 0 {
                return Ok(());
            }
        }
        match (&self.before, &self.after) {
            (_, Some(nfo)) => {
                // Unless the item went meanwhile.
                sqlx::query(
                    "INSERT INTO item_nfo (item_id, path, size, mtime, ids, fields, locked)
                     SELECT ?, ?, ?, ?, ?, ?, ? WHERE EXISTS (SELECT 1 FROM items WHERE id = ?)
                     ON CONFLICT(item_id) DO UPDATE SET path = excluded.path, size = excluded.size,
                        mtime = excluded.mtime, ids = excluded.ids, fields = excluded.fields, locked = excluded.locked",
                )
                .bind(self.item_id)
                .bind(&nfo.path)
                .bind(nfo.size)
                .bind(nfo.mtime)
                .bind(serde_json::to_string(&nfo.ids).map_err(|e| sqlx::Error::Encode(e.into()))?)
                .bind(serde_json::to_string(&nfo.fields).map_err(|e| sqlx::Error::Encode(e.into()))?)
                .bind(nfo.locked)
                .bind(self.item_id)
                .execute(&mut *conn)
                .await?;
            }
            (Some(_), None) => {
                sqlx::query("DELETE FROM item_nfo WHERE item_id = ?").bind(self.item_id).execute(&mut *conn).await?;
            }
            (None, None) => {}
        }
        let now = now();
        for (column, set, back) in &self.reverts {
            // `column` comes from `overrides::nfo_values`, so formatting it
            // into SQL is safe. Only what still holds the NFO file's value
            // goes back: an edit or a refresh meanwhile wins.
            let age =
                if *column == "content_rating" { ", content_rating_country = NULL, rating_age = NULL" } else { "" };
            sqlx::query(sqlx::AssertSqlSafe(format!(
                "UPDATE items SET {column} = ?{age}, updated_at = ? WHERE id = ? AND {column} IS ?"
            )))
            .bind(back)
            .bind(now)
            .bind(self.item_id)
            .bind(set)
            .execute(&mut *conn)
            .await?;
        }
        if let Some((seen, locked)) = self.locked {
            sqlx::query("UPDATE items SET metadata_locked = ? WHERE id = ? AND metadata_locked = ?")
                .bind(locked)
                .bind(self.item_id)
                .bind(seen)
                .execute(&mut *conn)
                .await?;
        }
        if let Some(id) = self.refresh {
            sqlx::query("UPDATE items SET metadata_at = NULL WHERE id = ?").bind(id).execute(&mut *conn).await?;
        }
        Ok(())
    }
}

/// The NFO files that appeared, changed or went for `rows` (the movies,
/// shows, seasons and episodes of a library, `files` their files). Only
/// those are read; they are found with `sidecars`, the walk's index.
fn nfo_changes(rows: Vec<NfoRow>, files: Vec<(i64, String)>, sidecars: &Sidecars, cx: &NfoContext) -> Vec<NfoChange> {
    let mut files_of: HashMap<i64, Vec<String>> = HashMap::new();
    for (item, path) in files {
        files_of.entry(item).or_default().push(path);
    }
    let movie_ids: HashSet<i64> = rows.iter().filter(|r| r.kind == "movie").map(|r| r.id).collect();
    // `movie.nfo` is the movie's only in a folder that holds no other.
    let mut movies_in: HashMap<PathBuf, HashSet<i64>> = HashMap::new();
    for (item, paths) in &files_of {
        if movie_ids.contains(item) {
            for path in paths {
                if let Some(dir) = Path::new(path).parent() {
                    movies_in.entry(dir.to_path_buf()).or_default().insert(*item);
                }
            }
        }
    }
    // An episode of each show.
    let mut episode_of: HashMap<i64, i64> = HashMap::new();
    for row in rows.iter().filter(|r| r.kind == "episode") {
        if let Some(show) = row.grandparent_id {
            episode_of.entry(show).or_insert(row.id);
        }
    }
    let unavailable = |path: &Path| cx.unavailable.iter().any(|p| path.starts_with(p));
    let no_files = Vec::new();

    let mut changes = Vec::new();
    for row in rows {
        let before = row.stored();
        let files = files_of.get(&row.id).unwrap_or(&no_files);
        let found = if cx.read { find_nfo(&row, files, sidecars, &movies_in) } else { None };
        let found = found.and_then(|path| {
            let (size, mtime) = match cx.listed.get(&path) {
                Some(listed) => *listed,
                // The listing didn't say.
                None => {
                    let meta = std::fs::metadata(&path).ok()?;
                    (meta.len() as i64, meta.modified().map(crate::util::system_time_secs).unwrap_or(0))
                }
            };
            Some((path_str(&path), size, mtime))
        });
        let after = match (&found, &before) {
            (Some((path, size, mtime)), Some(seen))
                if *path == seen.path && *size == seen.size && *mtime == seen.mtime =>
            {
                continue;
            }
            (Some((path, size, mtime)), _) => {
                tracing::debug!("reading the NFO file {path} of item {}", row.id);
                // A file that can't be read (too big, a link elsewhere) is as
                // good as none.
                nfo::read(Path::new(path)).map(|entries| {
                    let entry = entry_for(&row, entries);
                    let (ids, mut fields, locked) = entry.map(|e| (e.ids, e.fields, e.locked)).unwrap_or_default();
                    if row.kind == "season" {
                        // A season's NFO file names it and says what it is about.
                        fields = NfoFields { title: fields.title, summary: fields.summary, ..Default::default() };
                    }
                    ItemNfo { path: path.clone(), size: *size, mtime: *mtime, ids, fields, locked }
                })
            }
            (None, _) => None,
        };
        match (&before, &after) {
            (None, None) => continue,
            // Not gone: its folder couldn't be read this time.
            (Some(seen), None) if cx.read && unavailable(Path::new(&seen.path)) => continue,
            _ => {}
        }

        // A show is named in its episodes' paths.
        let named_in = match row.kind.as_str() {
            "show" => episode_of.get(&row.id).and_then(|e| files_of.get(e)).unwrap_or(&no_files),
            _ => files,
        };
        let ids = match (&before, &after) {
            _ if row.rematched => None,
            (_, Some(nfo)) if ItemIds::of_nfo(&row.kind, &nfo.ids) != ItemIds::default() => {
                let same = before.as_ref().is_some_and(|b| b.ids == nfo.ids);
                if same { None } else { row.ids.with_nfo(&row.kind, &nfo.ids) }
            }
            // It gives no ids any more: those it gave make way for the
            // names', or a search.
            (Some(seen), _) => row.ids.without_nfo(&row.kind, &seen.ids, || {
                named(&row, named_in, &cx.roots).map(|names| names.ids).unwrap_or_default()
            }),
            (None, _) => None,
        }
        .map(|next| (row.ids.clone(), next));
        let reverts = match &before {
            Some(seen) => nfo_reverts(&row, &seen.fields, after.as_ref().map(|a| &a.fields), named_in, cx),
            None => Vec::new(),
        };
        // Lockdata goes with the title the file describes.
        let matched = ids.as_ref().map_or(&row.ids, |(_, next)| next);
        let locked = after.as_ref().is_some_and(|a| a.locked && nfo_describes(&row.kind, matched, &a.ids));
        let locked = (row.metadata_locked != locked).then_some((row.metadata_locked, locked));
        // Online metadata is fetched again when the match changed (an
        // episode's ids don't decide it), the NFO file went or stopped
        // setting fields, or it no longer locks the item.
        let refetch = (ids.is_some() && matches!(row.kind.as_str(), "movie" | "show"))
            || !reverts.is_empty()
            || locked == Some((true, false))
            || (after.is_none() && before.as_ref().is_some_and(ItemNfo::sets_anything));
        let refresh = if refetch { row.refreshed_with() } else { None };
        changes.push(NfoChange { item_id: row.id, refresh, before, after, ids, reverts, locked });
    }
    changes
}

/// Where an item's NFO file is: `<video>.nfo` next to a movie's or an
/// episode's file, or `movie.nfo` in a folder that holds one movie;
/// `tvshow.nfo` in a show's folder, `season.nfo` in a season's.
fn find_nfo(
    row: &NfoRow,
    files: &[String],
    sidecars: &Sidecars,
    movies_in: &HashMap<PathBuf, HashSet<i64>>,
) -> Option<PathBuf> {
    let next_to_files = || files.iter().find_map(|file| sidecars.nfo(Path::new(file)));
    match row.kind.as_str() {
        "movie" => next_to_files().or_else(|| {
            let dir = Path::new(files.first()?).parent()?;
            let alone = movies_in.get(dir).is_some_and(|movies| movies.len() == 1);
            if alone { sidecars.folder_nfo(dir, &["movie"]) } else { None }
        }),
        "episode" => next_to_files(),
        "show" => sidecars.folder_nfo(Path::new(row.path.as_deref()?), &["tvshow"]),
        "season" => sidecars.folder_nfo(Path::new(row.path.as_deref()?), &["season"]),
        _ => None,
    }
}

/// The entry of an NFO file that describes `row`: the movie, show or season
/// it holds, or the link to its page (a movie's mustn't name a show); of a
/// multi-episode file, the episode with the item's season and number.
fn entry_for(row: &NfoRow, entries: Vec<Nfo>) -> Option<Nfo> {
    let mut entries = entries.into_iter();
    match row.kind.as_str() {
        "movie" => entries
            .find(|e| e.kind == NfoKind::Movie || (e.kind == NfoKind::Link && e.links_to != Some(NfoKind::TvShow))),
        "show" => entries
            .find(|e| e.kind == NfoKind::TvShow || (e.kind == NfoKind::Link && e.links_to != Some(NfoKind::Movie))),
        "season" => entries.find(|e| e.kind == NfoKind::Season),
        "episode" => {
            let episodes: Vec<Nfo> = entries.filter(|e| e.kind == NfoKind::Episode).collect();
            if episodes.len() == 1 {
                return episodes.into_iter().next();
            }
            episodes.into_iter().find(|e| e.season == row.season && e.episode == row.idx && row.idx.is_some())
        }
        _ => None,
    }
}

/// What an item's file and folder names say, before any source described it.
struct Named {
    title: String,
    year: Option<i64>,
    release_date: Option<String>,
    /// A movie's or a show's ids (`{tmdb-603}`).
    ids: ItemIds,
}

/// What `row`'s names say: a movie's or an episode's file's, a show's
/// (`files` are then an episode's), a season's number.
fn named(row: &NfoRow, files: &[String], roots: &[PathBuf]) -> Option<Named> {
    if row.kind == "season" {
        return Some(Named { title: season_title(row.idx?), year: None, release_date: None, ids: ItemIds::default() });
    }
    let file = Path::new(files.first()?);
    let root = roots.iter().find(|root| file.starts_with(root))?;
    Some(match row.kind.as_str() {
        "movie" => {
            let name = parse_movie(file, root);
            let ids = ItemIds { tmdb_id: name.tmdb_id, imdb_id: name.imdb_id, ..Default::default() };
            Named { title: name.title, year: name.year.map(i64::from), release_date: None, ids }
        }
        "show" => {
            let name = parse_episode(file, root);
            let ids = ItemIds { tmdb_id: name.show_tmdb_id, ..Default::default() };
            Named { title: name.show, year: name.show_year.map(i64::from), release_date: None, ids }
        }
        "episode" => {
            let name = parse_episode(file, root);
            let year = name.air_date.as_deref().and_then(|d| d.get(..4)).and_then(|y| y.parse().ok());
            Named { title: episode_title(&name, file), year, release_date: name.air_date, ids: ItemIds::default() }
        }
        _ => return None,
    })
}

/// The columns an NFO file set that it no longer sets (`after` is `None`
/// when it went): what each holds from it, and what it gets back. Titles,
/// years and dates come from the names, as for an item no source described:
/// without an online source nothing else would put them back. The rest is
/// emptied for the next refresh to fill.
fn nfo_reverts(
    row: &NfoRow,
    before: &NfoFields,
    after: Option<&NfoFields>,
    files: &[String],
    cx: &NfoContext,
) -> Vec<(&'static str, String, Option<String>)> {
    let kept: HashSet<&str> = after
        .map(|a| crate::overrides::nfo_values(&row.kind, a, &cx.region).into_iter().map(|(c, _)| c).collect())
        .unwrap_or_default();
    let gone: Vec<(&'static str, String)> = crate::overrides::nfo_values(&row.kind, before, &cx.region)
        .into_iter()
        // A rating's country goes with the rating.
        .filter(|(column, _)| !kept.contains(column) && *column != "content_rating_country")
        .collect();
    if gone.is_empty() {
        return Vec::new();
    }
    let names = named(row, files, &cx.roots);
    let title_back = gone.iter().any(|(c, _)| *c == "title");
    let mut reverts = Vec::new();
    for (column, set) in gone {
        let back = match column {
            "title" => match &names {
                Some(names) => Some(names.title.clone()),
                // Nothing to name it after: the NFO file's title stays.
                None => continue,
            },
            "sort_title" => {
                let title = match (&names, title_back) {
                    (Some(names), true) => names.title.as_str(),
                    (None, true) => continue,
                    (_, false) => row.title.as_str(),
                };
                Some(sort_title(title))
            }
            "year" => names.as_ref().and_then(|n| n.year).map(|y| y.to_string()),
            "release_date" => names.as_ref().and_then(|n| n.release_date.clone()),
            _ => None,
        };
        reverts.push((column, set, back));
    }
    reverts
}

/// Fix Match was undone: the ids of the item's NFO file decide its match
/// again, as when the file was read, its fields and lockdata are the
/// item's again, and its metadata is fetched again.
pub(crate) async fn nfo_match_dropped(db: &Db, item_id: i64) -> sqlx::Result<()> {
    #[derive(sqlx::FromRow)]
    struct Row {
        kind: String,
        nfo_ids: String,
        nfo_locked: bool,
        metadata_locked: bool,
        #[sqlx(flatten)]
        ids: ItemIds,
    }
    let row: Option<Row> = sqlx::query_as(
        "SELECT i.kind, n.ids AS nfo_ids, n.locked AS nfo_locked, i.metadata_locked,
                i.tmdb_id, i.imdb_id, i.tvdb_id, i.tvmaze_id
         FROM items i JOIN item_nfo n ON n.item_id = i.id WHERE i.id = ?",
    )
    .bind(item_id)
    .fetch_optional(&db.read)
    .await?;
    let Some(row) = row else { return Ok(()) };
    let Ok(nfo) = serde_json::from_str::<NfoIds>(&row.nfo_ids) else { return Ok(()) };
    let next = row.ids.with_nfo(&row.kind, &nfo);
    // Its lockdata is the item's again, with its title.
    let locked = row.nfo_locked && nfo_describes(&row.kind, next.as_ref().unwrap_or(&row.ids), &nfo);
    let locked = (locked != row.metadata_locked).then_some((row.metadata_locked, locked));
    if next.is_none() && locked.is_none() {
        return Ok(());
    }
    let change = NfoChange {
        item_id,
        refresh: Some(item_id),
        before: None,
        after: None,
        ids: next.map(|next| (row.ids, next)),
        reverts: Vec::new(),
        locked,
    };
    let mut tx = db.write.begin().await?;
    change.save(&mut tx).await?;
    tx.commit().await?;
    // The file describes the item again: its fields are the item's.
    crate::overrides::apply(db, &[item_id]).await
}

/// Is this file a good candidate for remuxing (copying H.264 into HLS)?
pub fn remux_candidate(container: Option<&str>, streams: &[pley_transcoder_api::MediaStream]) -> bool {
    let video = streams.iter().find(|s| s.kind == StreamKind::Video);
    video.is_some_and(|v| v.codec == "h264" && v.bit_depth.unwrap_or(8) <= 8 && !v.dynamic_range().is_hdr())
        && !matches!(container, Some("mp4" | "m4v" | "mov" | "webm"))
}

/// The title of bonus material: `Teaser.Final.mkv` → "Teaser Final"; a
/// `Movie (2020)-trailer.mkv` file is just "Trailer".
fn extra_title(stem: &str, extra: &Extra) -> String {
    if extra.owner_stem.is_some() {
        return match extra.kind {
            ExtraKind::Trailer => "Trailer",
            ExtraKind::BehindTheScenes => "Behind the Scenes",
            ExtraKind::DeletedScene => "Deleted Scene",
            ExtraKind::Featurette => "Featurette",
            ExtraKind::Interview => "Interview",
            ExtraKind::Scene => "Scene",
            ExtraKind::Short => "Short",
            ExtraKind::Other => "Extra",
        }
        .to_owned();
    }
    let cleaned: String = stem.chars().map(|c| if c == '.' || c == '_' { ' ' } else { c }).collect();
    let cleaned = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() { stem.to_owned() } else { cleaned }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scan_says_how_long_its_phase_has_left_from_the_pace_since_it_began() {
        use std::time::Instant;
        let mut scan = ScanStatus {
            library_id: 1,
            library_name: "Movies".into(),
            phase: ScanPhase::Walking,
            processed: 0,
            total: 0,
            started_at: 0,
            first: true,
            left_secs: None,
            pace: None,
        };
        scan.update(ScanPhase::Walking, 0, 0);
        assert!(scan.pace.is_none(), "finding files has no count to go by");
        scan.update(ScanPhase::Probing, 100, 1000);
        assert_eq!(scan.left_secs, None, "too soon to tell");
        // 200 files read in 20 seconds: 700 more take 70.
        scan.pace = Some((ScanPhase::Probing, Pace::measured_since(Instant::now() - Duration::from_secs(20), 100)));
        scan.update(ScanPhase::Probing, 300, 1000);
        assert_eq!(scan.left_secs, Some(70));
        // Fetching details is measured afresh.
        scan.update(ScanPhase::Metadata, 0, 50);
        assert_eq!(scan.left_secs, None);
        assert!(matches!(scan.pace, Some((ScanPhase::Metadata, _))));
    }

    fn library(id: i64, scanned_at: Option<i64>) -> LibraryRow {
        LibraryRow {
            id,
            name: "Movies".into(),
            kind: "movies".into(),
            paths: "[]".into(),
            created_at: 0,
            scanned_at,
            labels_pending: false,
        }
    }

    fn ids(tmdb: Option<i64>, imdb: Option<&str>, tvdb: Option<i64>, tvmaze: Option<i64>) -> ItemIds {
        ItemIds { tmdb_id: tmdb, imdb_id: imdb.map(str::to_owned), tvdb_id: tvdb, tvmaze_id: tvmaze }
    }

    fn nfo_ids(tmdb: Option<i64>, imdb: Option<&str>, tvdb: Option<i64>) -> NfoIds {
        NfoIds { tmdb, imdb: imdb.map(str::to_owned), tvdb, ..Default::default() }
    }

    #[test]
    fn requests_for_one_library_merge_into_what_either_asked() {
        let files = ScanRequest { library_id: 1, force: false, files: true, metadata: false };
        let metadata = ScanRequest { library_id: 1, force: true, files: false, metadata: true };
        let mut merged = files;
        merged.merge(metadata);
        assert!(merged.files && merged.metadata && merged.force);
        let mut alone = metadata;
        alone.merge(metadata);
        assert!(!alone.files && alone.metadata);
    }

    #[test]
    fn an_nfo_files_ids_decide_a_match_unless_the_item_has_one_of_them() {
        let nfo = nfo_ids;
        let found = ids(None, None, None, None).with_nfo("movie", &nfo(None, Some("tt0133093"), None));
        assert_eq!(found, Some(ids(None, Some("tt0133093"), None, None)));
        // One of the item's ids is the NFO file's: the others stay.
        let matched = ids(Some(603), None, None, None);
        assert_eq!(
            matched.with_nfo("movie", &nfo(Some(603), Some("tt0133093"), None)),
            Some(ids(Some(603), Some("tt0133093"), None, None))
        );
        // The same ids change nothing; a movie takes no TheTVDB id.
        assert_eq!(matched.with_nfo("movie", &nfo(Some(603), None, Some(81189))), None);
        // None is: a search's or the names' ids give way, whether they
        // differ or the NFO file names another site.
        let searched = ids(Some(99), Some("tt0000099"), None, None);
        assert_eq!(
            searched.with_nfo("movie", &nfo(None, Some("tt0133093"), None)),
            Some(ids(None, Some("tt0133093"), None, None))
        );
        assert_eq!(matched.with_nfo("movie", &nfo(None, Some("tt0133093"), None)), found);
        // A show searched on TMDB, named by its TheTVDB id.
        let show = ids(Some(5), Some("tt0000005"), None, None);
        assert_eq!(show.with_nfo("show", &nfo(None, None, Some(81189))), Some(ids(None, None, Some(81189), None)));
        // One refreshed from TVmaze, which the NFO file's TheTVDB id confirms.
        let show = ids(None, Some("tt0000001"), Some(81189), Some(42));
        assert_eq!(
            show.with_nfo("show", &nfo(Some(1399), None, Some(81189))),
            Some(ids(Some(1399), Some("tt0000001"), Some(81189), Some(42)))
        );
        // An episode's ids decide nothing: a missing one is added, and only
        // another of the same kind means another episode.
        let episode = ids(Some(7), None, None, None);
        assert_eq!(episode.with_nfo("episode", &nfo(None, None, Some(9))), Some(ids(Some(7), None, Some(9), None)));
        let episode = ids(Some(7), None, Some(8), None);
        assert_eq!(episode.with_nfo("episode", &nfo(None, None, Some(9))), Some(ids(None, None, Some(9), None)));
        // A season takes none.
        assert_eq!(ids(None, None, None, None).with_nfo("season", &nfo(Some(1), None, None)), None);
    }

    #[test]
    fn the_ids_of_an_nfo_file_that_went_make_way_for_the_names() {
        let gone = nfo_ids(Some(8077), None, None);
        let matched = ids(Some(8077), Some("tt0103644"), None, None);
        assert_eq!(matched.without_nfo("movie", &gone, ItemIds::default), Some(ItemIds::default()));
        let named = || ids(Some(348), None, None, None);
        assert_eq!(matched.without_nfo("movie", &gone, named), Some(named()));
        // Matched to something else meanwhile, it stays.
        assert_eq!(ids(Some(679), None, None, None).without_nfo("movie", &gone, ItemIds::default), None);
        let show = ids(None, Some("tt4000001"), Some(290001), Some(4242));
        assert_eq!(
            show.without_nfo("show", &nfo_ids(None, None, Some(290001)), ItemIds::default),
            Some(ItemIds::default())
        );
        // An episode's ids decide nothing.
        assert_eq!(ids(Some(8077), None, None, None).without_nfo("episode", &gone, ItemIds::default), None);
    }

    /// An NFO file's ids are applied only to the ids the scan saw; when they
    /// changed meanwhile (a refresh), nothing of the file is kept, so that
    /// the next scan reads it again rather than taking it as known.
    #[tokio::test]
    async fn an_nfo_file_whose_item_changed_meanwhile_is_read_again() {
        let dir = crate::testing::tempdir();
        let db = Db::open(&dir.path().join("pley.db")).await.unwrap();
        for sql in [
            "INSERT INTO libraries (id, name, kind, paths, created_at) VALUES (1, 'Movies', 'movies', '[]', 0)",
            "INSERT INTO items (id, library_id, kind, key, title, sort_title, added_at, updated_at)
             VALUES (1, 1, 'movie', 'm', 'M', 'm', 0, 0)",
        ] {
            sqlx::query(sql).execute(&db.write).await.unwrap();
        }
        let nfo = ItemNfo {
            path: "/m/M.nfo".into(),
            size: 10,
            mtime: 1,
            ids: nfo_ids(Some(603), None, None),
            fields: NfoFields::default(),
            locked: false,
        };
        let change = NfoChange {
            item_id: 1,
            refresh: Some(1),
            before: None,
            after: Some(nfo),
            ids: Some((ItemIds::default(), ids(Some(603), None, None, None))),
            reverts: Vec::new(),
            locked: None,
        };
        async fn stored(db: &Db) -> (Option<i64>, Option<String>, i64) {
            let item: (Option<i64>, Option<String>) =
                sqlx::query_as("SELECT tmdb_id, imdb_id FROM items WHERE id = 1").fetch_one(&db.read).await.unwrap();
            let nfos: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM item_nfo").fetch_one(&db.read).await.unwrap();
            (item.0, item.1, nfos)
        }

        // A refresh set an IMDb id since the scan looked.
        sqlx::query("UPDATE items SET imdb_id = 'tt0000001' WHERE id = 1").execute(&db.write).await.unwrap();
        let mut conn = db.write.acquire().await.unwrap();
        change.save(&mut conn).await.unwrap();
        drop(conn);
        assert_eq!(stored(&db).await, (None, Some("tt0000001".into()), 0));

        sqlx::query("UPDATE items SET imdb_id = NULL WHERE id = 1").execute(&db.write).await.unwrap();
        let mut conn = db.write.acquire().await.unwrap();
        change.save(&mut conn).await.unwrap();
        drop(conn);
        assert_eq!(stored(&db).await, (Some(603), None, 1));
    }

    #[test]
    fn failed_scans_are_tried_again_less_and_less_often() {
        const HOUR: i64 = 3600;
        let interval = 6 * HOUR; // the default
        let delays: Vec<i64> = (1..=9).map(|n| ScanFailures::retry_delay(n, interval)).collect();
        assert_eq!(delays, [300, 600, 1200, 2400, 4800, 9600, 19200, 21600, 21600]);
        assert_eq!(ScanFailures::retry_delay(3, 15 * 60), 15 * 60, "never longer than the interval");
        assert_eq!(ScanFailures::retry_delay(30, 0), 6 * HOUR, "periodic scans off: changes still wait");

        let mut failures = ScanFailures::default();
        let t = 100 * HOUR;
        let lib = library(1, Some(t - 7 * HOUR));
        assert!(failures.due(&lib, interval, t));
        failures.record(1, false, t);
        // `scanned_at` is old: it used to be due again a minute later.
        assert!(!failures.due(&lib, interval, t + 60));
        assert!(failures.backing_off(1, interval, t + 60));
        assert!(failures.due(&lib, interval, t + 300));
        failures.record(1, false, t + 300);
        assert!(!failures.due(&lib, interval, t + 300 + 599));
        assert!(failures.due(&lib, interval, t + 300 + 600));
        assert!(!failures.backing_off(2, interval, t + 60), "other libraries are not held back");

        // A scan that works: the interval counts again.
        failures.record(1, true, t + 1000);
        let lib = library(1, Some(t + 1000));
        assert!(!failures.backing_off(1, interval, t + 1001));
        assert!(!failures.due(&lib, interval, t + 1000 + interval - 1));
        assert!(failures.due(&lib, interval, t + 1000 + interval));
    }
}

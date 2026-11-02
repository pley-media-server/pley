//! Library scanning.
//!
//! A single worker processes scan requests one library at a time. A scan walks
//! the library folders, diffs the files against the database (size + mtime),
//! probes new/changed files through the transcoder with bounded concurrency,
//! writes items in short transactions, removes vanished files (unless their
//! drive is offline), refreshes sidecar artwork/subtitles, recomputes counts and
//! finally lets the metadata agent enrich new items.
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

use super::parse::{parse_episode, parse_movie, parse_track};
use super::sidecar::{ExternalSub, Sidecars};
use super::walk::{Extra, ExtraKind, FoundFile, walk};
use super::{LibraryKind, LibraryRow, get_library, list_libraries};
use crate::config::Config;
use crate::db::Db;
use crate::events::{EventBus, ScanPhase, ServerEvent};
use crate::metadata::MetadataAgent;
use crate::store::Store;
use crate::transcoder::{Transcoder, TranscoderError};
use crate::util::{normalize_key, now, path_str, sort_title};

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
}

#[derive(Debug, Clone, Copy)]
struct ScanRequest {
    library_id: i64,
    force: bool,
}

pub struct Scanner {
    db: Db,
    transcoder: Arc<Transcoder>,
    events: EventBus,
    metadata: Arc<MetadataAgent>,
    store: Arc<Store>,
    queue: mpsc::UnboundedSender<ScanRequest>,
    status: Mutex<HashMap<i64, ScanStatus>>,
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
    /// Subtitles and local artwork next to unchanged files.
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
            queue: tx,
            status: Mutex::default(),
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

    pub fn enqueue(&self, library_id: i64, force: bool) {
        self.status.lock().entry(library_id).or_insert_with(|| ScanStatus {
            library_id,
            library_name: String::new(),
            phase: ScanPhase::Queued,
            processed: 0,
            total: 0,
            started_at: now(),
        });
        let _ = self.queue.send(ScanRequest { library_id, force });
        self.events.send(ServerEvent::ScanProgress { library_id, phase: ScanPhase::Queued, processed: 0, total: 0 });
    }

    pub async fn enqueue_all(&self, force: bool) -> sqlx::Result<()> {
        for lib in list_libraries(&self.db).await? {
            self.enqueue(lib.id, force);
        }
        Ok(())
    }

    pub fn statuses(&self) -> Vec<ScanStatus> {
        let mut list: Vec<_> = self.status.lock().values().cloned().collect();
        list.sort_by_key(|s| s.started_at);
        list
    }

    pub fn is_scanning(&self, library_id: i64) -> bool {
        self.status.lock().contains_key(&library_id)
    }

    async fn worker(self: Arc<Self>, mut rx: mpsc::UnboundedReceiver<ScanRequest>) {
        let mut pending: Vec<ScanRequest> = Vec::new();
        loop {
            if pending.is_empty() {
                match rx.recv().await {
                    Some(req) => pending.push(req),
                    None => break,
                }
            }
            while let Ok(req) = rx.try_recv() {
                match pending.iter_mut().find(|p| p.library_id == req.library_id) {
                    Some(existing) => existing.force |= req.force,
                    None => pending.push(req),
                }
            }
            let req = pending.remove(0);
            let started = Instant::now();
            let result = self.scan(req.library_id, req.force).await;
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
                self.status.lock().remove(&req.library_id);
            }
            self.events.send(ServerEvent::ScanProgress {
                library_id: req.library_id,
                phase: ScanPhase::Done,
                processed: 0,
                total: 0,
            });
        }
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
        });
        entry.library_name = lib.name.clone();
        entry.phase = phase;
        entry.processed = processed;
        entry.total = total;
        drop(status);
        self.events.send(ServerEvent::ScanProgress { library_id: lib.id, phase, processed, total });
    }

    async fn scan(&self, library_id: i64, force: bool) -> anyhow::Result<Option<(Totals, Phases)>> {
        let Some(lib) = get_library(&self.db, library_id).await? else {
            return Ok(None);
        };
        let kind = lib.kind();
        let roots = lib.paths();
        self.events.send(ServerEvent::ScanStarted { library_id });
        self.set_status(&lib, ScanPhase::Walking, 0, 0);

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
        self.set_status(&lib, ScanPhase::Probing, 0, total);
        // Probes and writes take turns: the time spent writing is taken out
        // of the probing time.
        phase = Instant::now();
        let mut writing = Duration::ZERO;
        if !to_process.is_empty() && !self.transcoder.is_available() {
            self.transcoder.wait_ready(Duration::from_secs(90)).await;
        }

        let mut writer = Writer::new(&lib, &sidecars);
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
                    self.set_status(&lib, ScanPhase::Probing, processed, total);
                    last_emit = Instant::now();
                }
            }
            let flushing = Instant::now();
            self.flush(&mut writer, &mut batch, &existing_by_path, &mut totals).await?;
            writing += flushing.elapsed();
        }
        self.set_status(&lib, ScanPhase::Probing, processed, total);
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
        // Scanning rewrites titles of unmatched items and local artwork;
        // hand edits win.
        crate::overrides::apply_library(&self.db, library_id).await?;
        writing += phase.elapsed();
        self.events.send(ServerEvent::LibraryChanged { library_id });

        phase = Instant::now();
        if self.metadata.enabled_for(kind) {
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
        // New and changed files get their fingerprint; then whatever removed
        // items left behind goes.
        let store = self.store.clone();
        let removed = totals.removed > 0;
        tokio::spawn(async move {
            store.fingerprint_pending().await;
            if removed && let Err(err) = store.clean().await {
                tracing::warn!("storage clean-up failed: {err:#}");
            }
        });

        self.set_status(&lib, ScanPhase::Done, processed, total);
        self.events.send(ServerEvent::ScanFinished {
            library_id,
            added: totals.added,
            updated: totals.updated,
            removed: totals.removed,
        });
        phase = Instant::now();
        self.db.optimize().await;
        phases.write = writing + phase.elapsed();
        Ok(Some((totals, phases)))
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

        let season_title = if name.season == 0 { "Specials".to_owned() } else { format!("Season {}", name.season) };
        let mut season = NewItem::new("season", format!("season:{show_id}:{}", name.season), season_title);
        season.parent_id = Some(show_id);
        season.idx = Some(name.season);
        season.path = name.season_dir.as_deref().map(path_str);
        let season_id = self.container(conn, season).await?;

        let (key, title) = match name.episode {
            Some(number) => (
                format!("episode:{season_id}:{number}"),
                name.title.clone().unwrap_or_else(|| match name.episode_end {
                    Some(end) => format!("Episodes {number}-{end}"),
                    None => format!("Episode {number}"),
                }),
            ),
            // No number in the name: the title tells the episode apart, so
            // a season's unnumbered episodes never merge into one. Metadata
            // may number it by its title (see `metadata`).
            None => {
                let title = name.title.clone().unwrap_or_else(|| {
                    file.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
                });
                (format!("episode:{season_id}:title:{}", normalize_key(&title)), title)
            }
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

    fn library(id: i64, scanned_at: Option<i64>) -> LibraryRow {
        LibraryRow { id, name: "Movies".into(), kind: "movies".into(), paths: "[]".into(), created_at: 0, scanned_at }
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

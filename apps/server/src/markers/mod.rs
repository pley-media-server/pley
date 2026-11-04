//! Intro and credits markers (Skip Intro / Skip Credits).
//!
//! Sources, best first:
//! 1. **Chapters** named like an intro or credits ("Opening", "OP", "Intro",
//!    "Ending", "ED", "End Credits", …) give exact times for any video.
//! 2. **Audio fingerprints** of episodes: an intro is audio of 12 s to 3 min
//!    near the start that other episodes of the season share, credits are
//!    shared audio near the end. The transcoder computes Chromaprint
//!    fingerprints of the first half (at most 10 min) and the last 35 % (at
//!    most 8 min) of each episode; they are cached per file version, so an
//!    episode added later is only compared against cached data. Each episode
//!    is compared with the 8 nearest of its season, then (a premiere with an
//!    intro of its own, a short season) with episodes of the show's other
//!    seasons already fingerprinted.
//! 3. **Key frames**, for credits no other episode shares (a different song
//!    every week): the last stretch of text on black near the end, as
//!    Plex and Jellyfin's Intro Skipper find them.
//!
//! A single background worker analyses one season at a time after library
//! scans (and on demand), at low priority and pausing while anything plays:
//! a playback that starts stops the request under way (it is made again at
//! the worker's next turn). A season being watched goes first, during
//! playback (the transcoder holds it back while a playback starts), one
//! request at a time: the episodes playing before the rest of it, and their
//! players are told when they have markers. Nothing is analysed while
//! something is being transcoded (a playback's or a download's): the
//! transcoder pauses what it was making until it is done. Seasons an
//! administrator asks for go next; then those of shows likely to be watched
//! soon, and the rest of the libraries at the hours [`crate::schedule`]
//! gives library work.

#[cfg(test)]
mod eval;
pub mod matcher;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use pley_transcoder_api::{Chapter, FingerprintRequest, FrameStats, FramesRequest, MediaInfo};
use serde::Serialize;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::db::Db;
use crate::events::{EventBus, ServerEvent};
use crate::library::{LibraryKind, get_library};
use crate::playback::{QUIET_BEFORE_BACKGROUND_SECS, Sessions};
use crate::schedule::{self, Class, Scheduler};
use crate::settings::SettingsStore;
use crate::transcoder::{TRANSCODING_LOOK, Transcoder, TranscoderError};
use crate::util::now;
use matcher::{MatchParams, consensus, shared_run};

/// Bump when detection improves so every file is analysed again.
pub const ALGORITHM_VERSION: i64 = 2;
/// Episodes each episode is compared with (the nearest ones in the season).
const NEIGHBORS: usize = 8;
/// Episodes of the show's other seasons compared with one its own season
/// shares nothing with (cached fingerprints only).
const OTHER_SEASONS: usize = 6;
const INTRO_MIN_SECONDS: f64 = 12.0;
const INTRO_MAX_MS: i64 = 3 * 60_000;
const CREDITS_MIN_SECONDS: f64 = 15.0;
/// The last matching fingerprint value is computed from audio that starts
/// about this long before the shared audio ends.
const END_PADDING_MS: i64 = 1_300;
/// The first match usually starts before the shared audio (its analysis
/// window still overlaps the audio before): about a second, measured
/// against chapter-marked intros.
const START_OFFSET_MS: i64 = 1_000;
/// A match ending this close to the end of the other fingerprint was cut
/// short by it rather than by the audio.
const TRUNCATION_MS: f64 = 3_000.0;
/// Credits ending this close to the end of the file run to the end.
const CREDITS_TAIL_MS: i64 = 5_000;
/// A key frame is a page of credits when this share (%) of its pixels is
/// near black…
const PAGE_BLACK: f32 = 85.0;
/// …or a smaller share is, around text on true black (studio-range luma 16,
/// no colour, dark overall): longer lines take more room.
const PAGE_BLACK_WITH_TEXT: f32 = 65.0;
/// Luma range of a page with text on it.
const PAGE_TEXT: f32 = 60.0;
/// Pages this close together belong to the same credits.
const PAGES_GAP_MS: i64 = 20_000;
/// On-screen credits end at most this long before the end of the file (an
/// after-credits scene); a dark stretch earlier is part of the episode.
const CREDITS_NEAR_END_MS: i64 = 120_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarkerKind {
    Intro,
    Credits,
}

impl MarkerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MarkerKind::Intro => "intro",
            MarkerKind::Credits => "credits",
        }
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Marker {
    pub file_id: i64,
    /// `intro` or `credits`.
    pub kind: String,
    pub start_ms: i64,
    pub end_ms: i64,
    /// `chapter`, `fingerprint` or `frames`.
    pub source: String,
}

/// Markers of every file of an item.
pub async fn for_item(db: &Db, item_id: i64) -> sqlx::Result<Vec<Marker>> {
    sqlx::query_as(
        "SELECT m.file_id, m.kind, m.start_ms, m.end_ms, m.source FROM markers m
         JOIN media_files f ON f.id = m.file_id WHERE f.item_id = ? ORDER BY m.file_id, m.start_ms",
    )
    .bind(item_id)
    .fetch_all(&db.read)
    .await
}

/// What the worker is doing, for `/api/activity`.
#[derive(Debug, Clone, Serialize)]
pub struct AnalysisStatus {
    pub item_id: i64,
    pub title: String,
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Job {
    /// Queue whatever in the library is not analysed yet.
    Library(i64),
    Season {
        id: i64,
        force: bool,
    },
    /// Chapters of a movie's files.
    Movie(i64),
}

impl Job {
    /// Whether it waits for the hours of library work: a season a scan
    /// found ([`Class`]). Seasons asked for go at once, and the rest reads
    /// only the database.
    fn scheduled(self) -> bool {
        matches!(self, Job::Season { force: false, .. })
    }
}

pub struct MarkerService {
    db: Db,
    transcoder: Arc<Transcoder>,
    settings: Arc<SettingsStore>,
    sessions: Arc<Sessions>,
    schedule: Arc<Scheduler>,
    events: EventBus,
    cache_dir: PathBuf,
    /// What waits, in the order it came (but see [`MarkerService::next_job`]).
    queue: Mutex<VecDeque<Job>>,
    /// Something was queued.
    more: Notify,
    queued: Mutex<HashSet<Job>>,
    /// Seasons being watched that aren't analysed yet: before any [`Job`].
    watched: Mutex<VecDeque<i64>>,
    /// Wakes the worker for a season in `watched`.
    wake: Notify,
    /// What library work the job in progress is, if it waits for its hours.
    class: Mutex<Option<Class>>,
    status: Mutex<Option<AnalysisStatus>>,
    /// Cancelled when the server shuts down: the worker stops.
    stopping: CancellationToken,
}

/// A media file of an episode, with what analysis needs.
#[derive(Debug, Clone)]
struct EpisodeFile {
    file_id: i64,
    path: String,
    size: i64,
    mtime: i64,
    duration_ms: i64,
    audio_index: Option<i64>,
    chapters: Vec<Chapter>,
    current: bool,
}

#[derive(sqlx::FromRow)]
struct EpisodeFileRow {
    file_id: i64,
    path: String,
    size: i64,
    mtime: i64,
    duration_ms: Option<i64>,
    media_info: Option<String>,
    version: Option<i64>,
    analyzed_size: Option<i64>,
    analyzed_mtime: Option<i64>,
    fingerprinted: Option<bool>,
}

impl EpisodeFileRow {
    fn into_file(self, chromaprint: bool) -> Option<EpisodeFile> {
        let duration_ms = self.duration_ms.filter(|d| *d > 0)?;
        let info: Option<MediaInfo> = self.media_info.as_deref().and_then(|j| serde_json::from_str(j).ok());
        let current = self.version.is_some_and(|v| v >= ALGORITHM_VERSION)
            && self.analyzed_size == Some(self.size)
            && self.analyzed_mtime == Some(self.mtime)
            && (self.fingerprinted == Some(true) || !chromaprint);
        Some(EpisodeFile {
            file_id: self.file_id,
            path: self.path,
            size: self.size,
            mtime: self.mtime,
            duration_ms,
            audio_index: info.as_ref().and_then(|i| i.default_audio()).map(|a| a.index),
            chapters: info.map(|i| i.chapters).unwrap_or_default(),
            current,
        })
    }
}

const EPISODE_FILES: &str = "SELECT f.id AS file_id, f.path, f.size, f.mtime, f.duration_ms,
        f.media_info, a.version, a.size AS analyzed_size, a.mtime AS analyzed_mtime, a.fingerprinted
     FROM items e JOIN media_files f ON f.item_id = e.id
     LEFT JOIN items s ON s.id = e.parent_id
     LEFT JOIN marker_analysis a ON a.file_id = f.id";

/// Fingerprint of one window of one file.
#[derive(Debug, Clone)]
struct Fp {
    start_ms: i64,
    /// End of the analysed audio (ms in the file).
    end_ms: i64,
    item_ms: f64,
    data: Vec<u32>,
}

/// Fingerprinting cannot proceed right now (transcoder gone or lacking
/// Chromaprint): keep what can be done without it.
#[derive(Debug)]
enum FpError {
    Retry(String),
    Unsupported,
    /// A season being watched goes first: this one waits its turn again.
    Preempted,
}

impl MarkerService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Db,
        transcoder: Arc<Transcoder>,
        settings: Arc<SettingsStore>,
        sessions: Arc<Sessions>,
        schedule: Arc<Scheduler>,
        events: EventBus,
        cache_dir: PathBuf,
        stopping: CancellationToken,
    ) -> Arc<Self> {
        Arc::new(Self {
            db,
            transcoder,
            settings,
            sessions,
            schedule,
            events,
            cache_dir,
            queue: Mutex::default(),
            more: Notify::new(),
            queued: Mutex::default(),
            watched: Mutex::default(),
            wake: Notify::new(),
            class: Mutex::default(),
            status: Mutex::default(),
            stopping,
        })
    }

    /// Start the worker: analyse what is missing shortly after start-up and
    /// after every library scan.
    pub fn start(self: &Arc<Self>) {
        tokio::spawn(self.clone().worker());
        let this = self.clone();
        tokio::spawn(async move {
            let mut events = this.events.subscribe();
            tokio::time::sleep(Duration::from_secs(30)).await;
            this.enqueue_all().await;
            loop {
                match events.recv().await {
                    Ok(ServerEvent::ScanFinished { library_id, .. }) => this.enqueue(Job::Library(library_id)),
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => this.enqueue_all().await,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        let this = self.clone();
        tokio::spawn(async move {
            let mut events = this.events.subscribe();
            loop {
                match events.recv().await {
                    Ok(ServerEvent::SessionsChanged { .. })
                    | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                        if let Err(err) = this.watch_playing().await {
                            tracing::warn!("intro/credits detection of what plays: {err:#}");
                        }
                    }
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    /// Put the seasons of the episodes playing that aren't analysed yet
    /// before everything else.
    async fn watch_playing(&self) -> sqlx::Result<()> {
        let files: Vec<i64> = self.sessions.active().iter().map(|s| s.file_id).collect();
        if files.is_empty() || !self.settings.get().detect_markers {
            return Ok(());
        }
        let sql = format!(
            "SELECT DISTINCT e.parent_id FROM items e
             JOIN media_files f ON f.item_id = e.id
             LEFT JOIN marker_analysis a ON a.file_id = f.id
             WHERE f.id IN ({}) AND e.kind = 'episode' AND e.parent_id IS NOT NULL AND f.duration_ms > 0
               AND (a.file_id IS NULL OR a.version < ? OR a.size != f.size OR a.mtime != f.mtime
                    OR (a.fingerprinted = 0 AND ?))",
            vec!["?"; files.len()].join(", ")
        );
        let mut query = sqlx::query_scalar(sqlx::AssertSqlSafe(sql.as_str()));
        for id in &files {
            query = query.bind(id);
        }
        let seasons: Vec<i64> = query.bind(ALGORITHM_VERSION).bind(self.chromaprint()).fetch_all(&self.db.read).await?;
        let mut watched = self.watched.lock();
        for id in seasons {
            if !watched.contains(&id) {
                watched.push_back(id);
                // The worker, idle or waiting to fingerprint something else.
                self.wake.notify_waiters();
                self.wake.notify_one();
            }
        }
        Ok(())
    }

    pub fn status(&self) -> Option<AnalysisStatus> {
        self.status.lock().clone()
    }

    /// Analyse an item again: a movie, or every episode of an episode's
    /// season, a season or a show.
    pub async fn analyze_item(&self, item_id: i64, kind: &str) -> sqlx::Result<bool> {
        match kind {
            "movie" => self.enqueue(Job::Movie(item_id)),
            "episode" => {
                let season: Option<i64> = sqlx::query_scalar("SELECT parent_id FROM items WHERE id = ?")
                    .bind(item_id)
                    .fetch_one(&self.db.read)
                    .await?;
                let Some(season) = season else { return Ok(false) };
                self.enqueue(Job::Season { id: season, force: true });
            }
            "season" => self.enqueue(Job::Season { id: item_id, force: true }),
            "show" => {
                let seasons: Vec<i64> =
                    sqlx::query_scalar("SELECT id FROM items WHERE parent_id = ? AND kind = 'season' ORDER BY idx")
                        .bind(item_id)
                        .fetch_all(&self.db.read)
                        .await?;
                for id in seasons {
                    self.enqueue(Job::Season { id, force: true });
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn enqueue(&self, job: Job) {
        if self.queued.lock().insert(job) {
            self.queue.lock().push_back(job);
            self.more.notify_one();
        }
    }

    /// Queue a season put off again, before the rest: its fingerprints made
    /// so far are cached, so it is soon done once its turn comes.
    fn put_off(&self, job: Job) {
        if self.queued.lock().insert(job) {
            self.queue.lock().push_front(job);
            self.more.notify_one();
        }
    }

    /// Seasons waiting for intro and credits detection.
    pub fn pending(&self) -> usize {
        self.queue.lock().iter().filter(|job| matches!(job, Job::Season { .. })).count()
    }

    async fn enqueue_all(&self) {
        let libraries: Vec<i64> = sqlx::query_scalar("SELECT id FROM libraries WHERE kind IN ('shows', 'movies')")
            .fetch_all(&self.db.read)
            .await
            .unwrap_or_default();
        for id in libraries {
            self.enqueue(Job::Library(id));
        }
    }

    fn chromaprint(&self) -> bool {
        self.transcoder.info().is_some_and(|i| i.capabilities.chromaprint)
    }

    /// Analyses what is queued until the server shuts down, which drops the
    /// job in progress (it would only fail without the database and the
    /// transcoder; its season is analysed again next time).
    async fn worker(self: Arc<Self>) {
        let stopping = self.stopping.clone();
        tokio::select! {
            () = stopping.cancelled() => {}
            () = self.work() => {}
        }
    }

    async fn work(&self) {
        loop {
            // A season being watched first.
            let watched = self.watched.lock().pop_front();
            let (job, class) = match watched {
                Some(id) => (Job::Season { id, force: false }, None),
                None => match self.next_job().await {
                    Some(next) => next,
                    None => {
                        // Until something is queued, a season is watched, a
                        // playback starts or ends, or a look again (the hours,
                        // the processors).
                        tokio::select! {
                            () = self.more.notified() => {}
                            () = self.wake.notified() => {}
                            () = self.sessions.changed() => {}
                            () = tokio::time::sleep(schedule::LOOK) => {}
                        }
                        continue;
                    }
                },
            };
            if watched.is_none() {
                self.queued.lock().remove(&job);
            }
            if !self.settings.get().detect_markers {
                continue;
            }
            *self.class.lock() = class;
            let result = match job {
                Job::Library(id) => self.library(id).await,
                Job::Season { id, force } => self.season(id, force, watched.is_some()).await,
                Job::Movie(id) => self.movie(id).await,
            };
            *self.status.lock() = None;
            if let Err(err) = result {
                tracing::warn!("intro/credits detection ({job:?}) failed: {err:#}");
            }
        }
    }

    /// The job to do next, if one may begin now: first one that doesn't
    /// wait for library work's hours ([`Job::scheduled`]), then a season of
    /// a show likely to be watched soon, then the rest (see
    /// [`Scheduler::turn`]); with the [`Class`] of a season that does.
    async fn next_job(&self) -> Option<(Job, Option<Class>)> {
        {
            let mut queue = self.queue.lock();
            if let Some(at) = queue.iter().position(|job| !job.scheduled()) {
                return queue.remove(at).map(|job| (job, None));
            }
            if queue.is_empty() {
                return None;
            }
        }
        self.schedule.turn(Class::Soon).ok()?;
        let soon = self.schedule.soon().await;
        let later = self.schedule.turn(Class::Later).is_ok();
        let mut queue = self.queue.lock();
        let likely = queue.iter().position(|job| matches!(job, Job::Season { id, .. } if soon.seasons.contains(id)));
        match likely {
            Some(at) => queue.remove(at).map(|job| (job, Some(Class::Soon))),
            None if later => queue.pop_front().map(|job| (job, Some(Class::Later))),
            None => None,
        }
    }

    async fn library(&self, library_id: i64) -> anyhow::Result<()> {
        let Some(lib) = get_library(&self.db, library_id).await? else { return Ok(()) };
        // Make sure probing data is there (the transcoder reports Chromaprint support).
        if !self.transcoder.is_available() {
            self.transcoder.wait_ready(Duration::from_secs(60)).await;
        }
        let chromaprint = self.chromaprint();
        match lib.kind() {
            LibraryKind::Shows => {
                let seasons: Vec<i64> = sqlx::query_scalar(
                    "SELECT DISTINCT e.parent_id FROM items e
                     JOIN media_files f ON f.item_id = e.id
                     LEFT JOIN marker_analysis a ON a.file_id = f.id
                     WHERE e.library_id = ? AND e.kind = 'episode' AND e.parent_id IS NOT NULL AND f.duration_ms > 0
                       AND (a.file_id IS NULL OR a.version < ? OR a.size != f.size OR a.mtime != f.mtime
                            OR (a.fingerprinted = 0 AND ?))",
                )
                .bind(library_id)
                .bind(ALGORITHM_VERSION)
                .bind(chromaprint)
                .fetch_all(&self.db.read)
                .await?;
                for id in seasons {
                    self.enqueue(Job::Season { id, force: false });
                }
            }
            LibraryKind::Movies => {
                let movies: Vec<i64> = sqlx::query_scalar(
                    "SELECT DISTINCT f.item_id FROM media_files f
                     JOIN items i ON i.id = f.item_id
                     LEFT JOIN marker_analysis a ON a.file_id = f.id
                     WHERE f.library_id = ? AND i.kind = 'movie'
                       AND (a.file_id IS NULL OR a.version < ? OR a.size != f.size OR a.mtime != f.mtime)",
                )
                .bind(library_id)
                .bind(ALGORITHM_VERSION)
                .fetch_all(&self.db.read)
                .await?;
                for id in movies {
                    self.movie(id).await?;
                }
            }
            LibraryKind::Music => {}
        }
        Ok(())
    }

    /// Movies: chapter markers only (there is nothing to compare with).
    async fn movie(&self, item_id: i64) -> anyhow::Result<()> {
        let rows: Vec<EpisodeFileRow> = sqlx::query_as(
            "SELECT f.id AS file_id, f.path, f.size, f.mtime, f.duration_ms, f.media_info,
                    a.version, a.size AS analyzed_size, a.mtime AS analyzed_mtime, a.fingerprinted
             FROM media_files f LEFT JOIN marker_analysis a ON a.file_id = f.id WHERE f.item_id = ?",
        )
        .bind(item_id)
        .fetch_all(&self.db.read)
        .await?;
        let found: Vec<(EpisodeFile, Vec<Found>)> = rows
            .into_iter()
            .filter_map(|r| r.into_file(false))
            .map(|f| {
                let markers = chapter_markers(&f.chapters, f.duration_ms);
                (f, markers)
            })
            .collect();
        self.save(&found, true).await
    }

    /// Analyse a season. `watched`: it plays, so it runs during playback, the
    /// episodes playing first, and players are told about new markers.
    async fn season(&self, season_id: i64, force: bool, watched: bool) -> anyhow::Result<()> {
        let chromaprint = self.chromaprint();
        let season = self.episode_files("e.parent_id = ?", season_id, chromaprint).await?;
        if season.is_empty() || (!force && season.iter().all(|f| f.current)) {
            return Ok(());
        }
        let title: Option<String> = sqlx::query_scalar(
            "SELECT COALESCE(sh.title || ' — ', '') || s.title FROM items s LEFT JOIN items sh ON sh.id = s.parent_id
             WHERE s.id = ?",
        )
        .bind(season_id)
        .fetch_optional(&self.db.read)
        .await?;
        let show: Option<i64> = sqlx::query_scalar("SELECT parent_id FROM items WHERE id = ?")
            .bind(season_id)
            .fetch_one(&self.db.read)
            .await?;
        let whole_show = match show {
            Some(show) => self.episode_files("e.grandparent_id = ?", show, chromaprint).await?,
            None => season.clone(),
        };
        // Single-episode seasons borrow neighbours from the rest of the show.
        let pool = if season.len() < 2 { whole_show.clone() } else { season.clone() };
        // Episodes of other seasons, for what the season shares nothing of.
        let elsewhere: Vec<EpisodeFile> =
            whole_show.into_iter().filter(|f| !season.iter().any(|s| s.file_id == f.file_id)).collect();

        // Chapters first; fingerprints only for what they don't cover.
        let mut found: Vec<(EpisodeFile, Vec<Found>)> =
            season.iter().map(|f| (f.clone(), chapter_markers(&f.chapters, f.duration_ms))).collect();
        // Recorded so files are fingerprinted once Chromaprint becomes available.
        let mut fingerprinted = chromaprint;
        if chromaprint && pool.len() >= 2 {
            *self.status.lock() = Some(AnalysisStatus {
                item_id: season_id,
                title: title.clone().unwrap_or_default(),
                done: 0,
                total: 0,
            });
            // The episodes playing first: their players get markers sooner.
            let playing: HashSet<i64> =
                if watched { self.sessions.active().iter().map(|s| s.file_id).collect() } else { HashSet::new() };
            let (first, rest): (Vec<usize>, Vec<usize>) =
                (0..found.len()).partition(|&i| playing.contains(&found[i].0.file_id));
            for targets in [first, rest] {
                if targets.is_empty() {
                    continue;
                }
                match self.detect(&pool, &elsewhere, &mut found, &targets, watched).await {
                    Ok(()) => {}
                    Err(FpError::Unsupported) => {
                        fingerprinted = false;
                        break;
                    }
                    Err(FpError::Retry(why)) => {
                        // Keep the season queued for the next scan instead of
                        // recording it as analysed without fingerprints.
                        tracing::info!("intro/credits detection postponed: {why}");
                        return Ok(());
                    }
                    Err(FpError::Preempted) => {
                        // Fingerprints made so far are cached: nothing is lost.
                        self.put_off(Job::Season { id: season_id, force });
                        return Ok(());
                    }
                }
                if watched && targets.len() < found.len() {
                    let done: Vec<_> = targets.iter().map(|&i| found[i].clone()).collect();
                    self.save(&done, fingerprinted).await?;
                    self.announce(season_id).await?;
                }
            }
        }
        self.save(&found, fingerprinted).await?;
        if watched {
            self.announce(season_id).await?;
        }
        let intros = found.iter().filter(|(_, m)| m.iter().any(|m| m.kind == MarkerKind::Intro)).count();
        let credits = found.iter().filter(|(_, m)| m.iter().any(|m| m.kind == MarkerKind::Credits)).count();
        tracing::info!(
            "{}: intro found in {intros}/{n} episodes, credits in {credits}/{n}",
            title.as_deref().unwrap_or("season"),
            n = found.len()
        );
        Ok(())
    }

    /// Fingerprint matches for what chapters left the episodes `targets`
    /// (indices in `found`) without; then the show's other seasons, and the
    /// key frames for credits.
    async fn detect(
        &self,
        pool: &[EpisodeFile],
        elsewhere: &[EpisodeFile],
        found: &mut [(EpisodeFile, Vec<Found>)],
        targets: &[usize],
        watched: bool,
    ) -> Result<(), FpError> {
        let wanted: Vec<(usize, MarkerKind)> = targets
            .iter()
            .flat_map(|&i| {
                let markers = &found[i].1;
                [MarkerKind::Intro, MarkerKind::Credits]
                    .into_iter()
                    .filter(|kind| !markers.iter().any(|m| m.kind == *kind))
                    .map(move |kind| (i, kind))
            })
            .collect();
        // Which fingerprints are needed: the targets and their neighbours.
        let mut needed: Vec<(usize, MarkerKind)> = Vec::new();
        let mut plans: Vec<(usize, MarkerKind, usize, Vec<usize>)> = Vec::new();
        for &(i, kind) in &wanted {
            let target = &found[i].0;
            let Some(pos) = pool.iter().position(|p| p.file_id == target.file_id) else { continue };
            let others = neighbours(pos, pool.len());
            for &p in std::iter::once(&pos).chain(&others) {
                if !needed.contains(&(p, kind)) {
                    needed.push((p, kind));
                }
            }
            plans.push((i, kind, pos, others));
        }

        if let Some(status) = self.status.lock().as_mut() {
            status.total += needed.len() as u32;
        }
        let fps = self.fingerprints(pool, &needed, watched).await?;
        for (i, kind, pos, others) in plans {
            let target = found[i].0.clone();
            let Some(own) = fps.get(&(pos, kind)) else { continue };
            let others: Vec<&Fp> = others.iter().filter_map(|o| fps.get(&(*o, kind))).collect();
            let mut marker = detect_one(own, &others, kind, target.duration_ms);
            if marker.is_none() {
                // Other seasons, as far as they are fingerprinted already.
                let cached: Vec<Fp> = elsewhere
                    .iter()
                    .filter_map(|f| read_cached(&self.cache_path(f, kind)))
                    .take(OTHER_SEASONS)
                    .collect();
                marker = detect_one(own, &cached.iter().collect::<Vec<_>>(), kind, target.duration_ms);
            }
            if marker.is_none() && kind == MarkerKind::Credits {
                marker = self.frames(&target, watched).await?.and_then(|f| credits_from_frames(&f, target.duration_ms));
            }
            found[i].1.extend(marker);
        }
        Ok(())
    }

    /// Players refetch the items of the season's library (new markers).
    async fn announce(&self, season_id: i64) -> sqlx::Result<()> {
        let library: Option<i64> = sqlx::query_scalar("SELECT library_id FROM items WHERE id = ?")
            .bind(season_id)
            .fetch_optional(&self.db.read)
            .await?;
        if let Some(library_id) = library {
            self.events.send(ServerEvent::LibraryChanged { library_id });
        }
        Ok(())
    }

    async fn episode_files(&self, filter: &str, id: i64, chromaprint: bool) -> sqlx::Result<Vec<EpisodeFile>> {
        let rows: Vec<EpisodeFileRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "{EPISODE_FILES} WHERE {filter} AND e.kind = 'episode' ORDER BY COALESCE(s.idx, 0), COALESCE(e.idx, 0), f.id"
        )))
        .bind(id)
        .fetch_all(&self.db.read)
        .await?;
        Ok(rows.into_iter().filter_map(|r| r.into_file(chromaprint)).collect())
    }

    /// Fingerprints of `needed` (pool index, window), from the cache or the
    /// transcoder. Files that cannot be fingerprinted are simply missing.
    async fn fingerprints(
        &self,
        pool: &[EpisodeFile],
        needed: &[(usize, MarkerKind)],
        watched: bool,
    ) -> Result<HashMap<(usize, MarkerKind), Fp>, FpError> {
        let mut out = HashMap::new();
        // Two at a time, like the transcoder's own limit; one while it plays,
        // which reads the disk too.
        for chunk in needed.chunks(if watched { 1 } else { 2 }) {
            let mut fetches = Vec::with_capacity(chunk.len());
            for &(i, kind) in chunk {
                fetches.push(self.fingerprint(&pool[i], kind, watched));
            }
            let results = futures::future::join_all(fetches).await;
            for (&(i, kind), result) in chunk.iter().zip(results) {
                if let Some(fp) = result? {
                    out.insert((i, kind), fp);
                }
            }
            if let Some(status) = self.status.lock().as_mut() {
                status.done += chunk.len() as u32;
            }
        }
        Ok(out)
    }

    /// Where the fingerprint of `file` for `kind` is cached.
    fn cache_path(&self, file: &EpisodeFile, kind: MarkerKind) -> PathBuf {
        let (start_ms, duration_ms) = window(kind, file.duration_ms);
        self.cache_dir.join(format!("{}_{}_{}_{}_{}.fp", file.file_id, file.size, file.mtime, start_ms, duration_ms))
    }

    /// Background work: playback has the machine to itself, except for the
    /// markers of what plays (which the transcoder holds back while a
    /// playback starts). Transcodes come first, even for those. A season
    /// whose hours end is put off until its next turn (other programs busy
    /// meanwhile only keep the next one from beginning: an external
    /// transcoder on this machine counts as one, fingerprinting this one).
    async fn wait_turn(&self, watched: bool) -> Result<(), FpError> {
        loop {
            let class = if watched { None } else { *self.class.lock() };
            if let Some(Err(schedule::Wait::Hours)) = class.map(|c| self.schedule.turn(c)) {
                return Err(FpError::Preempted);
            }
            let quiet = watched || self.sessions.quiet_for() >= QUIET_BEFORE_BACKGROUND_SECS;
            if quiet && !self.transcoder.transcoding().await {
                return Ok(());
            }
            if !watched && !self.watched.lock().is_empty() {
                return Err(FpError::Preempted);
            }
            let look = if quiet { TRANSCODING_LOOK } else { Duration::from_secs(15) };
            let _ = tokio::time::timeout(look, self.wake.notified()).await;
        }
    }

    /// What `request` answers once it is this work's turn (see
    /// [`MarkerService::wait_turn`]). Library work that a playback starting
    /// interrupts (dropping the request stops the transcoder's FFmpeg) is
    /// asked again at its next turn; the markers of what plays go on. A
    /// transcode beginning meanwhile pauses the transcoder's FFmpeg instead:
    /// held past the request's time, it is asked again too.
    async fn in_turn<T, F: Future<Output = Result<T, TranscoderError>>>(
        &self,
        watched: bool,
        request: impl Fn() -> F,
    ) -> Result<Result<T, TranscoderError>, FpError> {
        loop {
            self.wait_turn(watched).await?;
            let starts = self.sessions.starts();
            tokio::select! {
                result = request() => {
                    let timed_out = matches!(&result, Err(TranscoderError::Http(err)) if err.is_timeout());
                    if !timed_out || !self.transcoder.transcoding().await {
                        return Ok(result);
                    }
                    tracing::debug!("intro/credits detection put off: something is being transcoded");
                }
                () = self.sessions.started_since(starts), if !watched => {
                    tracing::debug!("intro/credits detection put off: a playback is starting");
                }
            }
        }
    }

    /// The key frames of the credits window of `file`, from the cache or the
    /// transcoder; `None` for files without video or a transcoder that can't.
    async fn frames(&self, file: &EpisodeFile, watched: bool) -> Result<Option<Vec<FrameStats>>, FpError> {
        let (start_ms, duration_ms) = window(MarkerKind::Credits, file.duration_ms);
        let cache = self
            .cache_dir
            .join(format!("{}_{}_{}_{}_{}.frames", file.file_id, file.size, file.mtime, start_ms, duration_ms));
        if let Some(frames) = std::fs::read(&cache).ok().and_then(|b| serde_json::from_slice(&b).ok()) {
            return Ok(Some(frames));
        }
        let req = FramesRequest { path: file.path.clone(), start_ms, duration_ms };
        match self.in_turn(watched, || self.transcoder.frames(&req)).await? {
            Ok(frames) => {
                if let Ok(bytes) = serde_json::to_vec(&frames.frames) {
                    write_cache_file(&self.cache_dir, &cache, file.file_id, &bytes);
                }
                Ok(Some(frames.frames))
            }
            Err(TranscoderError::Api { message, .. }) => {
                tracing::debug!("no key frames for {}: {message}", file.path);
                Ok(None)
            }
            Err(err) => Err(FpError::Retry(err.to_string())),
        }
    }

    async fn fingerprint(&self, file: &EpisodeFile, kind: MarkerKind, watched: bool) -> Result<Option<Fp>, FpError> {
        let (start_ms, duration_ms) = window(kind, file.duration_ms);
        let cache = self.cache_path(file, kind);
        if let Some(fp) = read_cached(&cache) {
            return Ok(Some(fp));
        }
        let req = FingerprintRequest { path: file.path.clone(), stream_index: file.audio_index, start_ms, duration_ms };
        match self.in_turn(watched, || self.transcoder.fingerprint(&req)).await? {
            Ok(fp) => {
                let fp = Fp {
                    start_ms: fp.start_ms,
                    end_ms: (start_ms + duration_ms).min(file.duration_ms),
                    item_ms: fp.item_seconds * 1000.0,
                    data: fp.data,
                };
                write_cached(&self.cache_dir, &cache, file.file_id, &fp);
                Ok(Some(fp))
            }
            Err(TranscoderError::Api { status: 400, .. }) => Err(FpError::Unsupported),
            Err(TranscoderError::Api { message, .. }) => {
                // No audio, unreadable file, …: nothing to compare for this one.
                tracing::debug!("no fingerprint for {}: {message}", file.path);
                Ok(None)
            }
            Err(err) => Err(FpError::Retry(err.to_string())),
        }
    }

    /// Replace the markers of every analysed file and record the analysis.
    async fn save(&self, found: &[(EpisodeFile, Vec<Found>)], fingerprinted: bool) -> anyhow::Result<()> {
        let now = now();
        let mut tx = self.db.write.begin().await?;
        for (file, markers) in found {
            sqlx::query("DELETE FROM markers WHERE file_id = ?").bind(file.file_id).execute(&mut *tx).await?;
            for m in markers {
                // INSERT … SELECT: the file may have been removed meanwhile.
                sqlx::query(
                    "INSERT INTO markers (file_id, kind, start_ms, end_ms, source, created_at)
                     SELECT id, ?, ?, ?, ?, ? FROM media_files WHERE id = ?",
                )
                .bind(m.kind.as_str())
                .bind(m.start_ms)
                .bind(m.end_ms)
                .bind(m.source)
                .bind(now)
                .bind(file.file_id)
                .execute(&mut *tx)
                .await?;
            }
            sqlx::query(
                "INSERT INTO marker_analysis (file_id, version, size, mtime, fingerprinted, analyzed_at)
                 SELECT id, ?, ?, ?, ?, ? FROM media_files WHERE id = ?
                 ON CONFLICT(file_id) DO UPDATE SET version = excluded.version, size = excluded.size,
                    mtime = excluded.mtime, fingerprinted = excluded.fingerprinted, analyzed_at = excluded.analyzed_at",
            )
            .bind(ALGORITHM_VERSION)
            .bind(file.size)
            .bind(file.mtime)
            .bind(fingerprinted)
            .bind(now)
            .bind(file.file_id)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

/// A marker found for a file, before it is stored.
#[derive(Debug, Clone, PartialEq)]
struct Found {
    kind: MarkerKind,
    start_ms: i64,
    end_ms: i64,
    source: &'static str,
}

/// The part of a file fingerprinted to look for `kind`: `(start, length)` in ms.
fn window(kind: MarkerKind, duration_ms: i64) -> (i64, i64) {
    match kind {
        MarkerKind::Intro => (0, (duration_ms / 2).min(10 * 60_000)),
        MarkerKind::Credits => {
            let len = (duration_ms * 35 / 100).min(8 * 60_000);
            (duration_ms - len, len)
        }
    }
}

/// Up to [`NEIGHBORS`] other positions in a list of `len`, nearest first.
fn neighbours(pos: usize, len: usize) -> Vec<usize> {
    nearest(pos, len, NEIGHBORS)
}

/// Up to `count` other positions in a list of `len`, nearest first.
fn nearest(pos: usize, len: usize, count: usize) -> Vec<usize> {
    let mut out = Vec::new();
    for d in 1..len {
        for p in [pos.checked_sub(d), pos.checked_add(d).filter(|p| *p < len)].into_iter().flatten() {
            if out.len() < count {
                out.push(p);
            }
        }
    }
    out
}

/// An episode's marker of `kind` from its fingerprint and its neighbours'.
fn detect_one(own: &Fp, others: &[&Fp], kind: MarkerKind, duration_ms: i64) -> Option<Found> {
    let candidates: Vec<(i64, i64)> = others.iter().filter_map(|other| shared_segment(own, other, kind)).collect();
    finalize(consensus(&candidates), kind, duration_ms)
}

/// The segment of `own` (in ms within its file) that `other` shares.
fn shared_segment(own: &Fp, other: &Fp, kind: MarkerKind) -> Option<(i64, i64)> {
    let min_seconds = match kind {
        MarkerKind::Intro => INTRO_MIN_SECONDS,
        MarkerKind::Credits => CREDITS_MIN_SECONDS,
    };
    let run = shared_run(&own.data, &other.data, &MatchParams::new(own.item_ms / 1000.0, min_seconds))?;
    let own_at = |i: usize| own.start_ms + (i as f64 * own.item_ms).round() as i64;
    let other_at = |j: usize| other.start_ms + (j as f64 * other.item_ms).round() as i64;
    let start = own_at(run.a.start) + START_OFFSET_MS;
    let mut end = own_at(run.a.end - 1) + END_PADDING_MS;
    // The other fingerprint ended while the audio still matched (its credits
    // run to the end of its file): the shared audio lasts at least until the
    // end of the other's audio, mapped onto this file.
    let tail = (TRUNCATION_MS / other.item_ms).ceil() as usize;
    if run.b.end + tail >= other.data.len() {
        let shift = own_at(run.a.start) - other_at(run.b.start);
        end = end.max(other.end_ms + shift);
    }
    Some((start, end))
}

/// Sanity checks and final boundaries of a fingerprint match.
fn finalize(range: Option<(i64, i64)>, kind: MarkerKind, duration_ms: i64) -> Option<Found> {
    let (start, end) = range?;
    let mut start = start.max(0);
    let mut end = end.min(duration_ms);
    match kind {
        MarkerKind::Intro => {
            if end - start > INTRO_MAX_MS {
                return None;
            }
            // An intro opening the file starts at 0:00 (as Intro Skipper
            // does for starts within 2 s).
            if start <= 2_000 {
                start = 0;
            }
        }
        MarkerKind::Credits => {
            if start < duration_ms / 2 {
                return None;
            }
            if duration_ms - end <= CREDITS_TAIL_MS {
                end = duration_ms;
            }
        }
    }
    (end - start >= 10_000).then_some(Found { kind, start_ms: start, end_ms: end, source: "fingerprint" })
}

/// Credits as text on black in key frames (sorted by time) of the end of a
/// file: the last stretch of pages less than [`PAGES_GAP_MS`] apart, at least
/// [`CREDITS_MIN_SECONDS`] long, mostly pages and the pages mostly with text,
/// ending near the end of the file. Black stretches without text (a fade
/// between scenes) and dark scenes (no true black) don't count.
fn credits_from_frames(frames: &[FrameStats], duration_ms: i64) -> Option<Found> {
    let page = |f: &FrameStats| {
        f.black >= PAGE_BLACK || (f.black >= PAGE_BLACK_WITH_TEXT && f.ylow <= 16.5 && f.sat <= 4.0 && f.yhigh <= 45.0)
    };
    let mut best: Option<(usize, usize)> = None;
    let mut i = 0;
    while i < frames.len() {
        if !page(&frames[i]) {
            i += 1;
            continue;
        }
        let (first, mut last) = (i, i);
        for (j, f) in frames.iter().enumerate().skip(i + 1) {
            if f.ms - frames[last].ms > PAGES_GAP_MS {
                break;
            }
            if page(f) {
                last = j;
            }
        }
        let pages: Vec<&FrameStats> = frames[first..=last].iter().filter(|f| page(f)).collect();
        let with_text = pages.iter().filter(|f| f.ymax - f.ylow >= PAGE_TEXT).count();
        let long = (frames[last].ms - frames[first].ms) as f64 >= CREDITS_MIN_SECONDS * 1000.0;
        // Mostly pages, and the pages mostly with text.
        if long && pages.len() * 2 > last - first && with_text * 2 >= pages.len() {
            best = Some((first, last));
        }
        i = last + 1;
    }
    let (first, last) = best?;
    if duration_ms - frames[last].ms > CREDITS_NEAR_END_MS {
        return None;
    }
    // Up to the next key frame (an after-credits scene), or the end.
    let end = frames.get(last + 1).map_or(duration_ms, |f| f.ms);
    let end = if duration_ms - end <= CREDITS_TAIL_MS { duration_ms } else { end };
    Some(Found { kind: MarkerKind::Credits, start_ms: frames[first].ms, end_ms: end, source: "frames" })
}

/// Markers from chapter titles ("Opening", "OP", "Intro", "Ending", "ED",
/// "End Credits", …).
fn chapter_markers(chapters: &[Chapter], duration_ms: i64) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    for chapter in chapters {
        let Some(kind) = chapter.title.as_deref().and_then(chapter_kind) else { continue };
        let (start, end) = (chapter.start_ms.max(0), chapter.end_ms.min(duration_ms.max(chapter.end_ms)));
        let plausible = match kind {
            MarkerKind::Intro => (5_000..=5 * 60_000).contains(&(end - start)),
            // Shorter "Credits" chapters mark the last seconds of a file (a
            // logo), not the credits: those are found as if there were none.
            MarkerKind::Credits => (end - start) as f64 >= CREDITS_MIN_SECONDS * 1000.0,
        };
        if plausible && !out.iter().any(|m| m.kind == kind) {
            out.push(Found { kind, start_ms: start, end_ms: end, source: "chapter" });
        }
    }
    out
}

fn chapter_kind(title: &str) -> Option<MarkerKind> {
    let lower = title.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    // A trailing number ("OP 2", "Ending 1") does not change the meaning.
    let words = match words.split_last() {
        Some((last, rest)) if !rest.is_empty() && last.chars().all(|c| c.is_ascii_digit()) => rest,
        _ => &words[..],
    };
    let joined = words.join(" ");
    let intro = [
        "intro",
        "opening",
        "op",
        "opening credits",
        "opening titles",
        "opening theme",
        "opening song",
        "title sequence",
        "main titles",
        "main title",
        "theme song",
    ];
    let credits = [
        "credits",
        "end credits",
        "ending credits",
        "closing credits",
        "ending",
        "ed",
        "outro",
        "end titles",
        "ending theme",
        "ending song",
        "credits roll",
    ];
    let numbered =
        |s: &str, prefix: &str| s.strip_prefix(prefix).is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()));
    if intro.contains(&joined.as_str()) || numbered(&joined, "op") {
        Some(MarkerKind::Intro)
    } else if credits.contains(&joined.as_str()) || numbered(&joined, "ed") {
        Some(MarkerKind::Credits)
    } else {
        None
    }
}

// ------------------------------------------------------------------ cache

const CACHE_MAGIC: &[u8; 4] = b"PFP2";
const CACHE_HEADER: usize = 32;

fn read_cached(path: &Path) -> Option<Fp> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < CACHE_HEADER || &bytes[..4] != CACHE_MAGIC {
        return None;
    }
    let start_ms = i64::from_le_bytes(bytes[4..12].try_into().ok()?);
    let end_ms = i64::from_le_bytes(bytes[12..20].try_into().ok()?);
    let item_ms = f64::from_le_bytes(bytes[20..28].try_into().ok()?);
    let count = u32::from_le_bytes(bytes[28..32].try_into().ok()?) as usize;
    let data = bytes.get(CACHE_HEADER..CACHE_HEADER + count * 4)?;
    Some(Fp {
        start_ms,
        end_ms,
        item_ms,
        data: data.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect(),
    })
}

/// Store a fingerprint, dropping cached ones of older versions of the file.
fn write_cached(dir: &Path, path: &Path, file_id: i64, fp: &Fp) {
    let mut bytes = Vec::with_capacity(CACHE_HEADER + fp.data.len() * 4);
    bytes.extend_from_slice(CACHE_MAGIC);
    bytes.extend_from_slice(&fp.start_ms.to_le_bytes());
    bytes.extend_from_slice(&fp.end_ms.to_le_bytes());
    bytes.extend_from_slice(&fp.item_ms.to_le_bytes());
    bytes.extend_from_slice(&(fp.data.len() as u32).to_le_bytes());
    for v in &fp.data {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    write_cache_file(dir, path, file_id, &bytes);
}

/// Store `bytes` at `path` (`{file}_{size}_{mtime}_…`), dropping what is
/// cached for older versions of the file.
fn write_cache_file(dir: &Path, path: &Path, file_id: i64, bytes: &[u8]) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let keep: &str = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    // "{file}_{size}_{mtime}_": the separator keeps mtime 55 from matching 555.
    let version_prefix = format!("{}_", keep.splitn(4, '_').take(3).collect::<Vec<_>>().join("_"));
    let file_prefix = format!("{file_id}_");
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&file_prefix) && !name.starts_with(&version_prefix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
    let tmp = path.with_extension("tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chapter_titles() {
        for t in ["Intro", "Opening", "OP", "OP2", "Opening 1", "Opening Credits", "Title Sequence", "OP - 1"] {
            assert_eq!(chapter_kind(t), Some(MarkerKind::Intro), "{t}");
        }
        for t in ["Credits", "End Credits", "Ending", "ED", "ED1", "Ending 2", "Outro", "Closing Credits"] {
            assert_eq!(chapter_kind(t), Some(MarkerKind::Credits), "{t}");
        }
        for t in ["Chapter 1", "Opening Scene", "Prologue", "Part A", "Preview", "The End of the World", "Edit"] {
            assert_eq!(chapter_kind(t), None, "{t}");
        }
    }

    #[test]
    fn chapters_become_markers() {
        let ch = |start_ms, end_ms, title: &str| Chapter { start_ms, end_ms, title: Some(title.into()) };
        let chapters = [
            ch(0, 90_000, "Prologue"),
            ch(90_000, 180_000, "Opening"),
            ch(180_000, 1_300_000, "Part A"),
            ch(1_300_000, 1_390_000, "Ending"),
            ch(1_390_000, 1_420_000, "Preview"),
        ];
        let found = chapter_markers(&chapters, 1_420_000);
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].kind, found[0].start_ms, found[0].end_ms), (MarkerKind::Intro, 90_000, 180_000));
        assert_eq!((found[1].kind, found[1].start_ms, found[1].end_ms), (MarkerKind::Credits, 1_300_000, 1_390_000));
    }

    #[test]
    fn windows_and_neighbours() {
        assert_eq!(window(MarkerKind::Intro, 300_000), (0, 150_000));
        assert_eq!(window(MarkerKind::Intro, 3_600_000), (0, 600_000));
        assert_eq!(window(MarkerKind::Credits, 300_000), (195_000, 105_000));
        assert_eq!(window(MarkerKind::Credits, 3_600_000), (3_120_000, 480_000));
        assert_eq!(nearest(0, 6, 4), vec![1, 2, 3, 4]);
        assert_eq!(nearest(3, 6, 4), vec![2, 4, 1, 5]);
        assert_eq!(neighbours(1, 2), vec![0]);
        assert_eq!(neighbours(3, 12), vec![2, 4, 1, 5, 0, 6, 7, 8]);
    }

    /// Key frames 4 s apart from `from_s` to `to_s`, each as `kind` says.
    fn key_frames(from_s: i64, to_s: i64, kind: impl Fn(i64) -> &'static str) -> Vec<FrameStats> {
        (from_s..to_s)
            .step_by(4)
            .map(|s| {
                let (black, ylow, yhigh, ymax, sat) = match kind(s) {
                    // White text on black.
                    "page" => (95.0, 16.0, 16.0, 200.0, 0.1),
                    // Plain black between scenes or pages.
                    "black" => (100.0, 16.0, 16.0, 17.0, 0.0),
                    // A dark scene: never true black, some colour.
                    "dark" => (70.0, 22.0, 45.0, 180.0, 2.5),
                    _ => (10.0, 30.0, 150.0, 235.0, 20.0),
                };
                FrameStats { ms: s * 1000, black, ylow, yhigh, ymax, sat }
            })
            .collect()
    }

    #[test]
    fn credits_are_found_as_text_on_black() {
        // A 30 min episode: credits from 28:00 to 29:20, an after-credits scene.
        let frames = key_frames(1_320, 1_800, |s| match s {
            1_680..1_760 => "page",
            1_500..1_600 => "dark",
            _ => "scene",
        });
        let found = credits_from_frames(&frames, 1_800_000).unwrap();
        assert_eq!((found.start_ms, found.end_ms, found.source), (1_680_000, 1_760_000, "frames"));

        // Running to the end of the file.
        let frames = key_frames(1_320, 1_800, |s| if s >= 1_740 { "page" } else { "scene" });
        assert_eq!(
            credits_from_frames(&frames, 1_800_000).map(|f| (f.start_ms, f.end_ms)),
            Some((1_740_000, 1_800_000))
        );

        // Black without text (a fade), a dark scene, credits too short or too
        // far from the end: none.
        for kind in ["black", "dark"] {
            let frames = key_frames(1_320, 1_800, |s| if s >= 1_700 { kind } else { "scene" });
            assert_eq!(credits_from_frames(&frames, 1_800_000), None, "{kind}");
        }
        let frames = key_frames(1_320, 1_800, |s| if (1_780..1_790).contains(&s) { "page" } else { "scene" });
        assert_eq!(credits_from_frames(&frames, 1_800_000), None, "10 s");
        let frames = key_frames(1_320, 1_800, |s| if (1_400..1_480).contains(&s) { "page" } else { "scene" });
        assert_eq!(credits_from_frames(&frames, 1_800_000), None, "5 min before the end");
    }

    #[test]
    fn short_credits_chapters_are_not_credits() {
        let chapter = |start_ms, end_ms, title: &str| Chapter { start_ms, end_ms, title: Some(title.into()) };
        let found =
            chapter_markers(&[chapter(0, 1_560_000, "Episode"), chapter(1_566_000, 1_571_000, "Credits")], 1_571_000);
        assert!(found.is_empty(), "{found:?}");
        let found = chapter_markers(&[chapter(1_500_000, 1_571_000, "Credits")], 1_571_000);
        assert_eq!(found.len(), 1);
    }

    #[test]
    fn credits_reaching_the_end_run_to_the_end() {
        let found = finalize(Some((1_250_000, 1_297_000)), MarkerKind::Credits, 1_300_000).unwrap();
        assert_eq!(found.end_ms, 1_300_000);
        let found = finalize(Some((1_200_000, 1_260_000)), MarkerKind::Credits, 1_300_000).unwrap();
        assert_eq!(found.end_ms, 1_260_000, "a post-credits scene follows");
        assert!(finalize(Some((100_000, 160_000)), MarkerKind::Credits, 1_300_000).is_none());
        assert!(finalize(Some((10_000, 300_000)), MarkerKind::Intro, 1_300_000).is_none());
    }

    #[test]
    fn matches_cut_short_by_the_other_file_reach_its_end() {
        let mut x = 99u32;
        let mut noise = |n: usize| -> Vec<u32> {
            (0..n)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    x
                })
                .collect()
        };
        let credits = noise(300);
        // This file: 10 s of show, 30 s of credits, 10 s of post-credits scene.
        let own = Fp {
            start_ms: 600_000,
            end_ms: 650_000,
            item_ms: 100.0,
            data: [noise(100), credits.clone(), noise(100)].concat(),
        };
        // The other file's credits run to its end (its audio ends 1.5 s
        // after its last fingerprint value starts).
        let other =
            Fp { start_ms: 500_000, end_ms: 535_000 + 1_500, item_ms: 100.0, data: [noise(50), credits].concat() };
        let (start, end) = shared_segment(&own, &other, MarkerKind::Credits).unwrap();
        // The first match starts up to a second early: START_OFFSET_MS.
        assert!((start - 610_700).abs() <= 400, "{start}");
        assert_eq!(end, 640_000 + 1_500, "extended to where the other's audio ends");
        assert_eq!(finalize(Some((300, 30_100)), MarkerKind::Intro, 1_300_000).map(|f| f.start_ms), Some(0));
    }

    #[test]
    fn cache_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("7_100_5_0_1000.fp");
        let fp = Fp { start_ms: 42, end_ms: 1_042, item_ms: 123.8, data: vec![1, 2, u32::MAX] };
        write_cached(dir.path(), &old, 7, &fp);
        let back = read_cached(&old).unwrap();
        assert_eq!((back.start_ms, back.end_ms, back.data.clone()), (42, 1_042, vec![1, 2, u32::MAX]));
        // A new version of file 7 replaces the old cache entries.
        let new = dir.path().join("7_200_6_0_1000.fp");
        write_cached(dir.path(), &new, 7, &fp);
        assert!(!old.exists() && new.exists());
    }
}

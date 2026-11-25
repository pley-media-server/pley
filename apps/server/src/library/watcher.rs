//! Automatic rescans: debounced filesystem notifications plus a periodic timer
//! (for network shares where notifications are unreliable). A library whose
//! last scan failed is tried again after a growing delay
//! ([`super::scanner::ScanFailures`]).
//!
//! A notification rescans only where it changed ([`super::scope`]); the
//! periodic scan, and a burst of more changes than notifications are sure to
//! carry, rescan the whole library.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use parking_lot::Mutex;
use tokio::sync::mpsc;

use super::list_libraries;
use super::scanner::Scanner;
use super::scope::MOST_PATHS;
use crate::db::Db;
use crate::settings::SettingsStore;

const QUIET_PERIOD: Duration = Duration::from_secs(10);

/// What the file system says.
#[derive(Debug)]
enum Notice {
    /// Something changed at this path.
    Changed(PathBuf),
    /// Notifications were lost (under these paths; anywhere when none).
    Missed(Vec<PathBuf>),
}

/// What changed in a library since it was last rescanned.
#[derive(Debug)]
struct Changes {
    /// The paths that changed; `None`: anything may have.
    paths: Option<BTreeSet<PathBuf>>,
}

impl Changes {
    fn add(&mut self, path: PathBuf) {
        if let Some(paths) = &mut self.paths {
            paths.insert(path);
            if paths.len() > MOST_PATHS {
                self.paths = None;
            }
        }
    }
}

pub struct LibraryWatcher {
    db: Db,
    scanner: Arc<Scanner>,
    settings: Arc<SettingsStore>,
    watcher: Mutex<Option<RecommendedWatcher>>,
    roots: Arc<Mutex<Vec<(PathBuf, i64)>>>,
    tx: mpsc::UnboundedSender<Notice>,
}

impl LibraryWatcher {
    pub fn new(db: Db, scanner: Arc<Scanner>, settings: Arc<SettingsStore>) -> Arc<Self> {
        let (tx, rx) = mpsc::unbounded_channel();
        let watcher = Arc::new(Self { db, scanner, settings, watcher: Mutex::new(None), roots: Arc::default(), tx });
        tokio::spawn(watcher.clone().debounce(rx));
        tokio::spawn(watcher.clone().periodic());
        watcher
    }

    /// Re-read the library list and (re)arm filesystem watches.
    pub async fn reload(&self) {
        let libraries = match list_libraries(&self.db).await {
            Ok(libs) => libs,
            Err(err) => {
                tracing::warn!("cannot load libraries for watching: {err}");
                return;
            }
        };
        let roots: Vec<(PathBuf, i64)> =
            libraries.iter().flat_map(|l| l.paths().into_iter().map(move |p| (p, l.id))).collect();
        *self.roots.lock() = roots.clone();

        // notify's watcher on Windows wakes ten times a second for as long
        // as it lives: none without folders to watch.
        if !self.settings.get().watch_libraries || roots.is_empty() {
            *self.watcher.lock() = None;
            return;
        }
        let tx = self.tx.clone();
        let created = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if let Ok(event) = res {
                if event.need_rescan() {
                    let _ = tx.send(Notice::Missed(event.paths));
                    return;
                }
                if matches!(event.kind, notify::EventKind::Access(_)) {
                    return;
                }
                for path in event.paths {
                    let _ = tx.send(Notice::Changed(path));
                }
            }
        });
        let mut watcher = match created {
            Ok(w) => w,
            Err(err) => {
                tracing::warn!("file watching unavailable: {err}");
                return;
            }
        };
        for (root, _) in &roots {
            if let Err(err) = watcher.watch(root, RecursiveMode::Recursive) {
                tracing::warn!("cannot watch {}: {err}", root.display());
            }
        }
        *self.watcher.lock() = Some(watcher);
    }

    async fn debounce(self: Arc<Self>, mut rx: mpsc::UnboundedReceiver<Notice>) {
        let mut pending: HashMap<i64, (Instant, Changes)> = HashMap::new();
        loop {
            tokio::select! {
                notice = rx.recv() => {
                    let Some(notice) = notice else { break };
                    note(&mut pending, &self.roots.lock(), notice, Instant::now());
                }
                // A look every second while a library waits for its quiet
                // period; with none waiting, nothing to look at until a
                // change comes.
                _ = tokio::time::sleep(Duration::from_secs(1)), if !pending.is_empty() => {}
            }
            let due: Vec<i64> =
                pending.iter().filter(|(_, (at, _))| at.elapsed() >= QUIET_PERIOD).map(|(id, _)| *id).collect();
            let interval = self.settings.get().scan_interval_minutes as i64 * 60;
            for id in due {
                // Try again once the running scan finished, or once a library
                // whose last scan failed has waited.
                if self.scanner.is_scanning(id) || self.scanner.backing_off(id, interval) {
                    if let Some((at, _)) = pending.get_mut(&id) {
                        *at = Instant::now();
                    }
                    continue;
                }
                let Some((_, changes)) = pending.remove(&id) else { continue };
                match changes.paths {
                    Some(paths) => {
                        tracing::info!("changes detected in library {id} at {} paths, rescanning there", paths.len());
                        self.scanner.enqueue_changes(id, paths.into_iter().collect());
                    }
                    None => {
                        tracing::info!("changes detected in library {id}, rescanning");
                        self.scanner.enqueue(id, false);
                    }
                }
            }
        }
    }

    async fn periodic(self: Arc<Self>) {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            let interval = self.settings.get().scan_interval_minutes as i64 * 60;
            if interval == 0 {
                continue;
            }
            let Ok(libraries) = list_libraries(&self.db).await else { continue };
            for lib in libraries {
                // A failed scan leaves `walked_at` as it was: the scanner
                // spaces out the next tries.
                if self.scanner.due(&lib, interval) && !self.scanner.is_scanning(lib.id) {
                    self.scanner.enqueue(lib.id, false);
                }
            }
        }
    }
}

/// Notes `notice`, come `at`, in the changes `pending` for the libraries of
/// `roots`: a change waits for its library to go quiet again.
fn note(pending: &mut HashMap<i64, (Instant, Changes)>, roots: &[(PathBuf, i64)], notice: Notice, at: Instant) {
    fn changes(pending: &mut HashMap<i64, (Instant, Changes)>, id: i64, at: Instant) -> &mut Changes {
        let (noticed, changes) = pending.entry(id).or_insert_with(|| (at, Changes { paths: Some(BTreeSet::new()) }));
        *noticed = at;
        changes
    }
    match notice {
        Notice::Changed(path) => {
            if let Some(id) = library_of(roots, &path) {
                changes(pending, id, at).add(path);
            }
        }
        Notice::Missed(paths) if paths.is_empty() => {
            let mut ids: Vec<i64> = roots.iter().map(|(_, id)| *id).collect();
            ids.sort_unstable();
            ids.dedup();
            for id in ids {
                changes(pending, id, at).paths = None;
            }
        }
        Notice::Missed(paths) => {
            for path in paths {
                if let Some(id) = library_of(roots, &path) {
                    changes(pending, id, at).paths = None;
                }
            }
        }
    }
}

/// The library a changed path belongs to. Library folders can be nested:
/// the deepest folder holding the path wins (then the oldest library),
/// whatever order the libraries are listed in.
fn library_of(roots: &[(PathBuf, i64)], path: &Path) -> Option<i64> {
    roots
        .iter()
        .filter(|(root, _)| path.starts_with(root))
        .max_by_key(|(root, id)| (root.components().count(), std::cmp::Reverse(*id)))
        .map(|(_, id)| *id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_go_to_the_deepest_library_folder_whatever_the_order() {
        let movies = (PathBuf::from("/media/movies"), 1);
        let kids = (PathBuf::from("/media/movies/kids"), 2);
        let also_kids = (PathBuf::from("/media/movies/kids"), 3);
        for roots in
            [[movies.clone(), kids.clone(), also_kids.clone()], [also_kids.clone(), kids.clone(), movies.clone()]]
        {
            assert_eq!(library_of(&roots, Path::new("/media/movies/kids/Up (2009)/Up.mkv")), Some(2));
            assert_eq!(library_of(&roots, Path::new("/media/movies/Heat (1995).mkv")), Some(1));
            assert_eq!(library_of(&roots, Path::new("/media/movies-4k/Heat (1995).mkv")), None);
            assert_eq!(library_of(&roots, Path::new("/media/music/Song.mp3")), None);
        }
    }

    #[test]
    fn changes_are_kept_by_library_until_there_are_too_many_to_trust() {
        let roots = [(PathBuf::from("/media/movies"), 1), (PathBuf::from("/media/tv"), 2)];
        let start = Instant::now();
        let mut pending = HashMap::new();
        let changed = |path: &str| Notice::Changed(PathBuf::from(path));
        note(&mut pending, &roots, changed("/media/movies/Film (2001)/Film (2001).mkv"), start);
        note(&mut pending, &roots, changed("/media/movies/Film (2001)/Film (2001).en.srt"), start);
        note(&mut pending, &roots, changed("/media/elsewhere/x.mkv"), start);
        let later = start + Duration::from_secs(3);
        note(&mut pending, &roots, changed("/media/tv/Show/Season 1/Show - S01E01.mkv"), later);
        let paths = |id: i64, pending: &HashMap<i64, (Instant, Changes)>| {
            pending[&id]
                .1
                .paths
                .as_ref()
                .map(|paths| paths.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>())
        };
        assert_eq!(pending.len(), 2);
        assert_eq!(
            paths(1, &pending),
            Some(vec![
                "/media/movies/Film (2001)/Film (2001).en.srt".into(),
                "/media/movies/Film (2001)/Film (2001).mkv".into()
            ])
        );
        assert_eq!(pending[&2].0, later, "each library goes quiet on its own");

        // A folder of titles at once: the whole library.
        for n in 0..=MOST_PATHS {
            note(&mut pending, &roots, changed(&format!("/media/tv/Show {n}/Show {n} - S01E01.mkv")), later);
        }
        assert_eq!(paths(2, &pending), None);
        assert!(paths(1, &pending).is_some());
        // Notifications lost somewhere: every library.
        note(&mut pending, &roots, Notice::Missed(Vec::new()), later);
        assert_eq!((paths(1, &pending), paths(2, &pending)), (None, None));
        let mut pending = HashMap::new();
        note(&mut pending, &roots, Notice::Missed(vec![PathBuf::from("/media/tv/Show")]), later);
        assert_eq!(pending.len(), 1);
        assert_eq!(paths(2, &pending), None);
    }
}

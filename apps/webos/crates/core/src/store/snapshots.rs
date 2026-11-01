//! Saved answers, per server and profile, so every screen draws what it
//! showed last before the server answers, and while it doesn't (the Android
//! app's `FileSnapshotStore`, `RES/android-settings-auth-live.md §9.2`).
//!
//! `snapshots/<server digest>/<profile id>/<key>`: written whole and
//! atomically, not rewritten when unchanged; 25 MB in all, the least recently
//! used dropped first; each server keeps its last 3 profiles' answers.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::{file_name, hex_digest, make_dir, write_atomic};

/// The most the snapshots take on disk.
pub const SNAPSHOT_BUDGET: u64 = 25 * 1024 * 1024;
/// How many profiles of a server keep their snapshots.
pub const SNAPSHOT_PROFILES: usize = 3;

/// The keys screens save their answers under (the Android app's `SnapshotKeys`).
pub mod snapshot_keys {
    /// Home's rows.
    pub const HOME: &str = "home";
    /// Who the app acts as.
    pub const ME: &str = "me";
    /// The libraries.
    pub const LIBRARIES: &str = "libraries";
    /// The playlists.
    pub const PLAYLISTS: &str = "playlists";
    /// My List.
    pub const WATCHLIST: &str = "watchlist";
    /// The first 15 entries of the watch history.
    pub const HISTORY_HEAD: &str = "history:head";
    /// Live TV's summary.
    pub const LIVE: &str = "live";
    /// Live TV's groups.
    pub const LIVE_GROUPS: &str = "live:groups";

    /// The first page of a Live TV category (`LiveCategory::key`).
    pub fn live_page(category: &str) -> String {
        format!("live:page:{category}:0:")
    }

    /// A library's front page rows.
    pub fn library_hubs(library: i64) -> String {
        format!("libhubs:{library}")
    }

    /// The first page of a library's listing for a query (`LibraryQuery::key`).
    pub fn library_head(library: i64, query: &str) -> String {
        format!("library:{library}:{query}:head")
    }

    /// A library's genres and years.
    pub fn filters(library: i64) -> String {
        format!("filters:{library}")
    }

    /// A library's collections.
    pub fn collections(library: i64) -> String {
        format!("collections:{library}")
    }

    /// A collection and its items.
    pub fn collection(id: i64) -> String {
        format!("collection:{id}")
    }

    /// An item's details.
    pub fn item(id: i64) -> String {
        format!("item:{id}")
    }

    /// An item's children.
    pub fn children(id: i64) -> String {
        format!("children:{id}")
    }

    /// A show's or season's on-deck episode.
    pub fn on_deck(id: i64) -> String {
        format!("ondeck:{id}")
    }

    /// An item's related titles.
    pub fn related(id: i64) -> String {
        format!("related:{id}")
    }

    /// A person's page.
    pub fn person(id: i64) -> String {
        format!("person:{id}")
    }

    /// A playlist and its entries.
    pub fn playlist(id: i64) -> String {
        format!("playlist:{id}")
    }
}

struct File {
    size: u64,
    /// When it was last read or written, in this index's order.
    used: u64,
}

#[derive(Default)]
struct Index {
    loaded: bool,
    files: HashMap<PathBuf, File>,
    total: u64,
    tick: u64,
}

/// Saved answers per server and profile. Safe from any thread; disk work
/// happens on the calling thread (a task's, or the main thread's at start).
pub struct Snapshots {
    root: PathBuf,
    budget: u64,
    index: Mutex<Index>,
}

impl std::fmt::Debug for Snapshots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Snapshots").field("root", &self.root).field("budget", &self.budget).finish()
    }
}

fn mtime(meta: &fs::Metadata) -> SystemTime {
    meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)
}

impl Snapshots {
    /// Snapshots in `root` (usually `<data dir>/snapshots`) within [`SNAPSHOT_BUDGET`].
    pub fn open(root: impl Into<PathBuf>) -> Snapshots {
        Snapshots::with_budget(root, SNAPSHOT_BUDGET)
    }

    /// Snapshots within `budget` bytes.
    pub fn with_budget(root: impl Into<PathBuf>, budget: u64) -> Snapshots {
        Snapshots { root: root.into(), budget, index: Mutex::new(Index::default()) }
    }

    /// The folder they are kept in.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn server_dir(&self, server: &str) -> PathBuf {
        self.root.join(hex_digest(server.trim_end_matches('/').as_bytes()))
    }

    fn profile_dir(&self, server: &str, profile: i64) -> PathBuf {
        self.server_dir(server).join(profile.to_string())
    }

    pub(crate) fn path(&self, server: &str, profile: i64, key: &str) -> PathBuf {
        self.profile_dir(server, profile).join(file_name(key))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Index> {
        let mut index = self.index.lock().unwrap_or_else(|e| e.into_inner());
        if !index.loaded {
            index.loaded = true;
            // Oldest first, so the order of use starts as the order of writing.
            let mut found: Vec<(SystemTime, PathBuf, u64)> = Vec::new();
            for server in read_dirs(&self.root) {
                for profile in read_dirs(&server) {
                    for f in fs::read_dir(&profile).into_iter().flatten().filter_map(|e| e.ok()) {
                        let Ok(meta) = f.metadata() else { continue };
                        if meta.is_file() && !f.file_name().to_string_lossy().ends_with(".tmp") {
                            found.push((mtime(&meta), f.path(), meta.len()));
                        }
                    }
                }
            }
            found.sort();
            for (_, path, size) in found {
                index.tick += 1;
                let used = index.tick;
                index.total += size;
                index.files.insert(path, File { size, used });
            }
        }
        index
    }

    /// A saved answer's bytes.
    pub fn read(&self, server: &str, profile: i64, key: &str) -> Option<Vec<u8>> {
        let path = self.path(server, profile, key);
        let bytes = fs::read(&path).ok()?;
        let mut index = self.lock();
        index.tick += 1;
        let tick = index.tick;
        if let Some(f) = index.files.get_mut(&path) {
            f.used = tick;
        }
        Some(bytes)
    }

    /// A saved answer read as `T`; none when absent or of another shape.
    pub fn get<T: DeserializeOwned>(&self, server: &str, profile: i64, key: &str) -> Option<T> {
        serde_json::from_slice(&self.read(server, profile, key)?).ok()
    }

    /// Saves an answer as JSON.
    pub fn put<T: Serialize + ?Sized>(&self, server: &str, profile: i64, key: &str, value: &T) -> io::Result<()> {
        let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
        self.write(server, profile, key, &bytes)
    }

    /// Saves an answer's bytes, unless they are what is saved already; then
    /// keeps the server's last 3 profiles and the budget.
    pub fn write(&self, server: &str, profile: i64, key: &str, bytes: &[u8]) -> io::Result<()> {
        let path = self.path(server, profile, key);
        let mut index = self.lock();
        index.tick += 1;
        let tick = index.tick;
        let unchanged = index.files.get(&path).is_some_and(|f| f.size == bytes.len() as u64)
            && fs::read(&path).is_ok_and(|old| old == bytes);
        if unchanged {
            if let Some(f) = index.files.get_mut(&path) {
                f.used = tick;
            }
            return Ok(());
        }
        let dir = self.profile_dir(server, profile);
        make_dir(&dir)?;
        write_atomic(&path, bytes)?;
        let old = index.files.insert(path.clone(), File { size: bytes.len() as u64, used: tick }).map_or(0, |f| f.size);
        index.total = index.total - old + bytes.len() as u64;
        self.keep_profiles(&mut index, server, profile);
        self.keep_budget(&mut index, &path);
        Ok(())
    }

    /// Forgets one saved answer.
    pub fn remove(&self, server: &str, profile: i64, key: &str) {
        let path = self.path(server, profile, key);
        let mut index = self.lock();
        if fs::remove_file(&path).is_ok()
            && let Some(f) = index.files.remove(&path)
        {
            index.total -= f.size;
        }
    }

    /// Forgets a profile's saved answers on `server`.
    pub fn forget_profile(&self, server: &str, profile: i64) {
        let dir = self.profile_dir(server, profile);
        let mut index = self.lock();
        drop_dir(&mut index, &dir);
    }

    /// Forgets everything saved for `server` (signing out, forgetting it).
    pub fn forget_server(&self, server: &str) {
        let dir = self.server_dir(server);
        let mut index = self.lock();
        drop_dir(&mut index, &dir);
    }

    /// Forgets every saved answer.
    pub fn clear(&self) {
        let mut index = self.lock();
        let _ = fs::remove_dir_all(&self.root);
        index.files.clear();
        index.total = 0;
    }

    /// The bytes saved in all.
    pub fn total_bytes(&self) -> u64 {
        self.lock().total
    }

    /// Drops the profile folders of `server` beyond the last 3 used, keeping `profile`'s.
    fn keep_profiles(&self, index: &mut Index, server: &str, profile: i64) {
        let server_dir = self.server_dir(server);
        let current = self.profile_dir(server, profile);
        let mut profiles: Vec<(u64, PathBuf)> = read_dirs(&server_dir)
            .into_iter()
            .map(|dir| {
                let used = index.files.iter().filter(|(p, _)| p.starts_with(&dir)).map(|(_, f)| f.used).max();
                (if dir == current { u64::MAX } else { used.unwrap_or(0) }, dir)
            })
            .collect();
        if profiles.len() <= SNAPSHOT_PROFILES {
            return;
        }
        profiles.sort_by_key(|p| std::cmp::Reverse(p.0));
        for (_, dir) in profiles.drain(SNAPSHOT_PROFILES..) {
            drop_dir(index, &dir);
        }
    }

    /// Drops the least recently used answers until the budget holds, never `keep`.
    fn keep_budget(&self, index: &mut Index, keep: &Path) {
        if index.total <= self.budget {
            return;
        }
        let mut order: Vec<(u64, PathBuf)> =
            index.files.iter().filter(|(p, _)| p.as_path() != keep).map(|(p, f)| (f.used, p.clone())).collect();
        order.sort();
        for (_, path) in order {
            if index.total <= self.budget {
                break;
            }
            let _ = fs::remove_file(&path);
            if let Some(f) = index.files.remove(&path) {
                index.total -= f.size;
            }
        }
    }
}

fn read_dirs(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect()
}

fn drop_dir(index: &mut Index, dir: &Path) {
    let _ = fs::remove_dir_all(dir);
    let gone: Vec<PathBuf> = index.files.keys().filter(|p| p.starts_with(dir)).cloned().collect();
    for p in gone {
        if let Some(f) = index.files.remove(&p) {
            index.total -= f.size;
        }
    }
}

//! The profile's recent searches (`CachedSearch`): kept in memory for the page, newest first, and
//! written to the store on the task pool, so the UI thread never waits on the disk. Writes land in
//! the order they were made: each one writes the newest list still waiting.

use std::sync::{Arc, Mutex, MutexGuard};

use pley_core::store::MAX_SEARCHES;
use pley_core::{Store, Tasks};

/// The recent searches of the server and profile in use.
pub struct Recents {
    /// Newest first.
    list: Vec<String>,
    /// The store's key for them (the server and profile); none without a profile.
    key: Option<String>,
    writer: Arc<Writer>,
}

/// Writes the lists in turn.
struct Writer {
    store: Store,
    /// The lists still to write, the newest for each key.
    waiting: Mutex<Vec<(String, Vec<String>)>>,
    /// Held while writing, so an older list never lands after a newer one.
    writing: Mutex<()>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Writer {
    /// Writes what waits; an empty list forgets the key.
    fn flush(&self) {
        let _in_turn = lock(&self.writing);
        let waiting = std::mem::take(&mut *lock(&self.waiting));
        for (key, list) in waiting {
            let done = if list.is_empty() { self.store.remove(&key) } else { self.store.set(&key, &list) };
            if let Err(e) = done {
                log::warn!("recent searches not kept: {e}");
            }
        }
    }
}

impl Recents {
    /// None yet: [`Recents::load`] reads them.
    pub fn new(store: Store) -> Recents {
        Recents {
            list: Vec::new(),
            key: None,
            writer: Arc::new(Writer { store, waiting: Mutex::new(Vec::new()), writing: Mutex::new(()) }),
        }
    }

    /// Reads the recent searches kept under `key` (`keys::searches`), or a list still waiting to
    /// be written; none for `None`. A small file, read when the page opens or the profile changes.
    pub fn load(&mut self, key: Option<String>) {
        let waiting = key
            .as_ref()
            .and_then(|k| lock(&self.writer.waiting).iter().find(|(w, _)| w == k).map(|(_, list)| list.clone()));
        self.list = match (&key, waiting) {
            (_, Some(list)) => list,
            (Some(k), None) => self.writer.store.get::<Vec<String>>(k).unwrap_or_default(),
            (None, None) => Vec::new(),
        };
        self.list.truncate(MAX_SEARCHES);
        self.key = key;
    }

    /// Newest first.
    pub fn list(&self) -> &[String] {
        &self.list
    }

    /// Puts `query` first (once, whatever its case), keeping 10, as `Store::remember_search` does.
    pub fn remember(&mut self, tasks: &Tasks, query: &str) {
        let q = query.trim();
        if q.is_empty() {
            return;
        }
        let lower = q.to_lowercase();
        self.list.retain(|s| s.to_lowercase() != lower);
        self.list.insert(0, q.to_string());
        self.list.truncate(MAX_SEARCHES);
        self.save(tasks);
    }

    /// Forgets them all (Clear).
    pub fn clear(&mut self, tasks: &Tasks) {
        self.list.clear();
        self.save(tasks);
    }

    fn save(&self, tasks: &Tasks) {
        let Some(key) = self.key.clone() else { return };
        {
            let mut waiting = lock(&self.writer.waiting);
            match waiting.iter_mut().find(|(k, _)| *k == key) {
                Some((_, list)) => list.clone_from(&self.list),
                None => waiting.push((key, self.list.clone())),
            }
        }
        let writer = self.writer.clone();
        tasks.detach(move || writer.flush());
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use pley_base::Waker;

    use super::*;

    fn store() -> (Store, std::path::PathBuf) {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("pley-search-recent-{}-{nanos}", std::process::id()));
        (Store::open(&dir).unwrap(), dir)
    }

    fn kept(store: &Store, want: &[&str]) -> bool {
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if store.searches("http://nas", 1) == want {
                return true;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn the_newest_list_is_kept_and_read_back() {
        let (store, dir) = store();
        let tasks = Tasks::new(Waker::noop());
        let key = pley_core::store::keys::searches("http://nas", 1);
        let mut r = Recents::new(store.clone());
        r.load(Some(key.clone()));
        assert!(r.list().is_empty());
        for q in ["north", " ma ", "North", "", "zz"] {
            r.remember(&tasks, q);
        }
        assert_eq!(r.list(), ["zz", "North", "ma"], "newest first, once whatever the case");
        assert!(kept(&store, &["zz", "North", "ma"]));
        for i in 0..12 {
            r.remember(&tasks, &format!("q{i}"));
        }
        assert_eq!(r.list().len(), MAX_SEARCHES);
        r.clear(&tasks);
        r.remember(&tasks, "last");
        assert!(kept(&store, &["last"]), "the writes land in order");

        let mut again = Recents::new(store.clone());
        again.load(Some(key));
        assert_eq!(again.list(), ["last"]);
        again.load(None);
        assert!(again.list().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}

//! What a player needs from the app around it: the server, the task pool,
//! the report queue, the TV's capabilities and the memories it keeps.

use std::sync::{Arc, Mutex};

use pley_core::store::keys;
use pley_core::task::Tasks;
use pley_core::{CapsReport, Store};
use pley_subs::OffsetBook;

use crate::autoquality::{AutoMemory, Remembered, wall_ms};
use crate::network::NetworkMemory;
use crate::reporter::Reporter;
use crate::server::PlayerServer;
use crate::speed::SPEEDS;

/// The quality chosen in Settings or the menu, kept for the next titles.
pub trait QualityPrefs: Send + Sync {
    /// The quality picked last (a preset's id); none when none was.
    fn remembered(&self) -> Option<String>;
    /// Keeps `id` as the quality to start with.
    fn remember(&self, id: &str);
}

/// A quality kept in memory only (tests; a player that saves nothing).
#[derive(Default)]
pub struct HeldQuality(pub Mutex<Option<String>>);

impl HeldQuality {
    /// Remembering `id` already.
    pub fn with(id: &str) -> HeldQuality {
        HeldQuality(Mutex::new(Some(id.to_string())))
    }
}

impl QualityPrefs for HeldQuality {
    fn remembered(&self) -> Option<String> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn remember(&self, id: &str) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = Some(id.to_string());
    }
}

/// Automatic's memory of a slow network, shared by the titles one after another.
pub type SharedMemory = Arc<Mutex<dyn AutoMemory>>;

/// An [`AutoMemory`] that goes through a [`SharedMemory`], as each title's Automatic is given.
pub(crate) struct ThroughShared(pub SharedMemory);

impl AutoMemory for ThroughShared {
    fn get(&self) -> Option<Remembered> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).get()
    }

    fn set(&mut self, value: Option<Remembered>) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).set(value);
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Kept {
    kbps: f64,
    at: i64,
}

/// Automatic's memory in the TV's store (`pley.autoQuality.<server>`,
/// `{kbps, at}`): read when made, written in the background.
pub struct StoreMemory {
    store: Store,
    key: String,
    value: Option<Remembered>,
    tasks: Option<Tasks>,
}

impl StoreMemory {
    /// The memory for `server`, read now; written through `tasks` (at once without).
    pub fn new(store: Store, server: &str, tasks: Option<Tasks>) -> StoreMemory {
        let key = keys::auto_quality(server);
        let value =
            store.get::<Kept>(&key).filter(|k| k.kbps.is_finite()).map(|k| Remembered { kbps: k.kbps, at: k.at });
        StoreMemory { store, key, value, tasks }
    }
}

impl AutoMemory for StoreMemory {
    fn get(&self) -> Option<Remembered> {
        self.value
    }

    fn set(&mut self, value: Option<Remembered>) {
        self.value = value;
        let (store, key) = (self.store.clone(), self.key.clone());
        let write = move || {
            // Storage full or blocked: the next title starts at the original.
            let _ = match value {
                Some(v) => store.set(&key, &Kept { kbps: v.kbps, at: v.at }),
                None => store.remove(&key),
            };
        };
        match &self.tasks {
            Some(t) => t.detach(write),
            None => write(),
        }
    }
}

/// Subtitle sync per file and profile (pley-subs' [`OffsetBook`]), saved in
/// the store under `pley.subtitleOffsets`. Cheap to clone.
#[derive(Clone)]
pub struct SubtitleOffsets {
    book: Arc<Mutex<OffsetBook>>,
    store: Option<Store>,
}

impl SubtitleOffsets {
    /// Offsets kept in this run only.
    pub fn held() -> SubtitleOffsets {
        SubtitleOffsets { book: Arc::new(Mutex::new(OffsetBook::new(Vec::new()))), store: None }
    }

    /// The offsets `store` keeps.
    pub fn load(store: Store) -> SubtitleOffsets {
        let entries: Vec<(String, i32)> = store.get(keys::SUBTITLE_OFFSETS).unwrap_or_default();
        SubtitleOffsets { book: Arc::new(Mutex::new(OffsetBook::new(entries))), store: Some(store) }
    }

    /// The offset of `file_id` for `profile` on `server`, in ms.
    pub fn get(&self, server: &str, file_id: i64, profile: Option<i64>) -> i32 {
        self.book.lock().unwrap_or_else(|e| e.into_inner()).get(server, file_id, profile)
    }

    /// Keeps an offset (0 forgets it); the store is written through `tasks`.
    pub fn set(&self, server: &str, file_id: i64, profile: Option<i64>, ms: i64, tasks: &Tasks) {
        let mut book = self.book.lock().unwrap_or_else(|e| e.into_inner());
        book.set(server, file_id, profile, ms);
        if !book.take_changed() {
            return;
        }
        if let Some(store) = self.store.clone() {
            let entries = book.entries().to_vec();
            tasks.detach(move || {
                let _ = store.set(keys::SUBTITLE_OFFSETS, &entries);
            });
        }
    }
}

/// The capability report, asked on a task (the TV may ask its services).
pub type CapsProbe = Arc<dyn Fn() -> Option<CapsReport> + Send + Sync>;

/// What a player needs from the app around it.
#[derive(Clone)]
pub struct PlayerEnv {
    pub server: Arc<dyn PlayerServer>,
    pub tasks: Tasks,
    pub reporter: Reporter,
    /// The TV's capabilities (none when the probe failed).
    pub caps: CapsProbe,
    /// The TV's player can't send headers: its addresses carry a key (docs/API.md, "Keyed URLs").
    pub keyed_urls: bool,
    /// This TV's install id: a new session ends this client's earlier ones.
    pub client_id: String,
    /// The profile playing, for its subtitle offsets.
    pub profile: Option<i64>,
    pub offsets: SubtitleOffsets,
    pub quality: Arc<dyn QualityPrefs>,
    /// Where Automatic keeps a slow network for the next title (per TV and server).
    pub auto_memory: Option<SharedMemory>,
    /// What the server knows of this TV's network.
    pub network: Option<Arc<dyn NetworkMemory>>,
    /// Up Next plays by itself after its countdown.
    pub autoplay_next: bool,
    /// The speeds the TV's player plays with their sound; empty when only normal.
    pub speeds: Vec<f32>,
    /// Milliseconds since 1970.
    pub wall: Arc<dyn Fn() -> i64 + Send + Sync>,
}

impl PlayerEnv {
    /// An environment with LG's defaults: keyed addresses, the speeds, no
    /// memories, offsets in this run only and Automatic until a quality is picked.
    pub fn new(
        server: Arc<dyn PlayerServer>,
        tasks: Tasks,
        reporter: Reporter,
        caps: CapsProbe,
        client_id: impl Into<String>,
    ) -> PlayerEnv {
        PlayerEnv {
            server,
            tasks,
            reporter,
            caps,
            keyed_urls: true,
            client_id: client_id.into(),
            profile: None,
            offsets: SubtitleOffsets::held(),
            quality: Arc::new(HeldQuality::default()),
            auto_memory: None,
            network: None,
            autoplay_next: true,
            speeds: SPEEDS.to_vec(),
            wall: Arc::new(wall_ms),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str) -> Store {
        let dir = std::env::temp_dir().join(format!("pley-player-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Store::open(dir).expect("a store")
    }

    #[test]
    fn automatics_memory_lives_in_the_tvs_store_per_server() {
        let s = store("memory");
        let mut kept = StoreMemory::new(s.clone(), "http://nas:7539", None);
        assert_eq!(kept.get(), None);
        kept.set(Some(Remembered { kbps: 4_000.0, at: 5 }));
        assert_eq!(
            StoreMemory::new(s.clone(), "http://nas:7539", None).get(),
            Some(Remembered { kbps: 4_000.0, at: 5 })
        );
        assert_eq!(StoreMemory::new(s.clone(), "http://other:7539", None).get(), None);
        kept.set(None);
        assert_eq!(StoreMemory::new(s.clone(), "http://nas:7539", None).get(), None);
        s.set_raw(&keys::auto_quality("http://x"), b"{broken").unwrap();
        assert_eq!(StoreMemory::new(s, "http://x", None).get(), None);
    }

    #[test]
    fn subtitle_offsets_are_kept_per_file_and_profile() {
        let s = store("offsets");
        let tasks = Tasks::with_threads(1, pley_base::Waker::noop());
        let offsets = SubtitleOffsets::load(s.clone());
        offsets.set("http://nas:7539", 7, Some(1), 500, &tasks);
        assert_eq!(offsets.get("http://nas:7539", 7, Some(1)), 500);
        assert_eq!(offsets.get("http://nas:7539", 7, Some(2)), 0);
        // Written in the background, read back by the next player.
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while SubtitleOffsets::load(s.clone()).get("http://nas:7539", 7, Some(1)) != 500 {
            assert!(std::time::Instant::now() < until);
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
}

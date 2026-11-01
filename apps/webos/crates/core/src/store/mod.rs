//! What the TV keeps between launches: small JSON values by key in a folder
//! of the data directory, each written atomically (a temporary file, flushed,
//! then renamed over the old one), read synchronously at start.
//!
//! The keys and shapes follow the TV web apps' (`packages/tv/src/state.ts`):
//! servers, sign-ins (tokens per server), the install id, settings, subtitle
//! offsets, Automatic's memory, Live TV's view and category, households,
//! library views and recent searches. Saved answers per server and profile
//! live in [`Snapshots`]. Every read trusts only the shape: a value that
//! isn't what it should be reads as absent.

mod settings;
mod snapshots;

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

pub use settings::{
    PassthroughChoice, PassthroughChoices, PassthroughFormat, Settings, SubtitleBackground, SubtitleFont,
    SubtitlePosition, SubtitleSize,
};
pub use snapshots::{SNAPSHOT_BUDGET, SNAPSHOT_PROFILES, Snapshots, snapshot_keys};

use crate::model::AppProfile;

/// The keys of what the TV keeps. `<server>` is always the server's origin.
pub mod keys {
    /// The server in use (its origin, as a JSON string).
    pub const SERVER: &str = "pley.server";
    /// The servers this TV chose: `[SavedServer]`, newest first, at most 20.
    pub const SERVERS: &str = "pley.servers";
    /// This TV's `client_id` for playbacks and Live TV: `webos-<32 hex>`, made once.
    pub const INSTALL_ID: &str = "pley.installId";
    /// The device settings that differ from their defaults.
    pub const SETTINGS: &str = "pley.settings";
    /// Live TV's view (`guide` or `channels`), for the whole TV.
    pub const LIVE_VIEW: &str = "pley.live.view";
    /// Live TV's category (`all`, `favorites`, `recent`, `group:<name>`), for the whole TV.
    pub const LIVE_CATEGORY: &str = "pley.live.category";
    /// Subtitle sync per file and profile: `[[key, ms], …]`, the 300 newest.
    pub const SUBTITLE_OFFSETS: &str = "pley.subtitleOffsets";

    /// A sign-in's tokens.
    pub fn tokens(server: &str) -> String {
        format!("pley.tokens.{server}")
    }

    /// The household's profiles as last seen (`[AppProfile]`), so "Who's
    /// watching?" draws before the server answers.
    pub fn household(server: &str) -> String {
        format!("pley.householdProfiles.{server}")
    }

    /// The profile last used on a server.
    pub fn last_profile(server: &str) -> String {
        format!("pley.profile.{server}")
    }

    /// How a library was left: its sort, order and filters (`LibraryQuery`).
    pub fn library_view(server: &str, profile: i64, library: i64) -> String {
        format!("pley.library.{server}.{profile}.{library}")
    }

    /// The tab a library was last on: `recommended`, `library` or `collections`.
    pub fn library_tab(server: &str, profile: i64, library: i64) -> String {
        format!("pley.libraryTab.{server}.{profile}.{library}")
    }

    /// Recent searches, newest first, at most 10.
    pub fn searches(server: &str, profile: i64) -> String {
        format!("pley.searches.{server}.{profile}")
    }

    /// Automatic's step-down memory: `{kbps, at}`, good for 30 minutes.
    pub fn auto_quality(server: &str) -> String {
        format!("pley.autoQuality.{server}")
    }

    /// Whether `key` belongs to `server` (forgotten with it).
    pub fn of_server(key: &str, server: &str) -> bool {
        [
            tokens(server),
            household(server),
            last_profile(server),
            auto_quality(server),
            format!("pley.library.{server}."),
            format!("pley.libraryTab.{server}."),
            format!("pley.searches.{server}."),
        ]
        .iter()
        .any(|k| if k.ends_with('.') { key.starts_with(k.as_str()) } else { key == k })
    }
}

/// A server this TV chose.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedServer {
    /// Its origin.
    pub url: String,
    pub name: String,
    /// The discovery id, when a broadcast found it.
    #[serde(default)]
    pub id: Option<String>,
    /// Unix milliseconds.
    #[serde(default)]
    pub last_used: u64,
}

/// The most servers kept.
pub const MAX_SERVERS: usize = 20;
/// The most recent searches kept.
pub const MAX_SEARCHES: usize = 10;
/// The most subtitle offsets kept.
pub const MAX_SUBTITLE_OFFSETS: usize = 300;
/// How long Automatic's memory counts (`player/autoquality.ts`).
pub const AUTO_QUALITY_MS: u64 = 30 * 60 * 1000;

#[derive(Serialize, Deserialize)]
struct AutoQualityMemory {
    kbps: u32,
    at: u64,
}

struct Inner {
    dir: PathBuf,
    durable: bool,
    writing: Mutex<()>,
}

/// The key-value store. Cheap to clone; safe from any thread.
#[derive(Clone)]
pub struct Store {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store").field("dir", &self.inner.dir).finish()
    }
}

/// A key as a file name: letters, digits, `-`, `_` and `.` kept, other bytes
/// as `~xx`; a name too long for a file system hashed.
pub(crate) fn file_name(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for &b in key.as_bytes() {
        if b.is_ascii_alphanumeric() || b"-_.".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("~{b:02x}"));
        }
    }
    if out.len() > 200 || out.starts_with('.') {
        return format!("~h{}", hex_digest(key.as_bytes()));
    }
    out
}

fn key_of(name: &str) -> Option<String> {
    if name.starts_with("~h") {
        return None;
    }
    let bytes = name.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'~' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The first 16 bytes of SHA-256, in hex.
pub(crate) fn hex_digest(bytes: &[u8]) -> String {
    let d = ring::digest::digest(&ring::digest::SHA256, bytes);
    d.as_ref()[..16].iter().map(|b| format!("{b:02x}")).collect()
}

/// Makes `dir` (and its parents), readable by the app alone on the TV.
pub(crate) fn make_dir(dir: &Path) -> io::Result<()> {
    let mut b = fs::DirBuilder::new();
    b.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(dir)
}

/// Writes `bytes` to `path` atomically: a temporary file beside it, flushed
/// to the disk, then renamed over it. A crash leaves the old value or the
/// new one, never half of one.
pub(crate) fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let mut options = fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut f = options.open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

impl Store {
    /// The store in `dir` (made when missing). Its values survive restarts.
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<Store> {
        Store::open_with(dir.into(), true)
    }

    /// A store in a folder that a restart may wipe (`/tmp` on a TV whose other
    /// folders refused it): the app then says it can't keep the sign-in.
    pub fn open_volatile(dir: impl Into<PathBuf>) -> io::Result<Store> {
        Store::open_with(dir.into(), false)
    }

    fn open_with(dir: PathBuf, durable: bool) -> io::Result<Store> {
        make_dir(&dir)?;
        Ok(Store { inner: Arc::new(Inner { dir, durable, writing: Mutex::new(()) }) })
    }

    /// The folder the values are in.
    pub fn dir(&self) -> &Path {
        &self.inner.dir
    }

    /// Whether values survive a restart.
    pub fn durable(&self) -> bool {
        self.inner.durable
    }

    fn path(&self, key: &str) -> PathBuf {
        self.inner.dir.join(format!("{}.json", file_name(key)))
    }

    /// A value's bytes, as written.
    pub fn get_raw(&self, key: &str) -> Option<Vec<u8>> {
        fs::read(self.path(key)).ok()
    }

    /// A value read as `T`; none when absent or of another shape.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        serde_json::from_slice(&self.get_raw(key)?).ok()
    }

    /// Keeps `value` under `key`, atomically.
    pub fn set<T: Serialize + ?Sized>(&self, key: &str, value: &T) -> io::Result<()> {
        let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
        self.set_raw(key, &bytes)
    }

    /// Keeps `bytes` under `key`, atomically.
    pub fn set_raw(&self, key: &str, bytes: &[u8]) -> io::Result<()> {
        let _one_at_a_time = self.inner.writing.lock().unwrap_or_else(|e| e.into_inner());
        write_atomic(&self.path(key), bytes)
    }

    /// Forgets `key`.
    pub fn remove(&self, key: &str) -> io::Result<()> {
        let _one_at_a_time = self.inner.writing.lock().unwrap_or_else(|e| e.into_inner());
        match fs::remove_file(self.path(key)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// Every key kept.
    pub fn keys(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.inner.dir) else { return Vec::new() };
        let mut keys: Vec<String> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| e.file_name().to_str().and_then(|n| n.strip_suffix(".json")).and_then(key_of))
            .collect();
        keys.sort();
        keys
    }

    /// Forgets every key `pick` chooses.
    pub fn remove_where(&self, pick: impl Fn(&str) -> bool) {
        for key in self.keys() {
            if pick(&key) {
                let _ = self.remove(&key);
            }
        }
    }

    /// Forgets everything ("Delete all local data").
    pub fn clear(&self) {
        let _one_at_a_time = self.inner.writing.lock().unwrap_or_else(|e| e.into_inner());
        if let Ok(entries) = fs::read_dir(&self.inner.dir) {
            for e in entries.filter_map(|e| e.ok()) {
                let _ = fs::remove_file(e.path());
            }
        }
    }

    /// This TV's id for playbacks and Live TV (`webos-` and 32 hex digits),
    /// made the first time it is asked for and never changed.
    pub fn install_id(&self) -> String {
        if let Some(id) = self.get::<String>(keys::INSTALL_ID).filter(|id| valid_install_id(id)) {
            return id;
        }
        let mut bytes = [0u8; 16];
        if ring::rand::SecureRandom::fill(&ring::rand::SystemRandom::new(), &mut bytes).is_err() {
            // No system randomness: the clock keeps two TVs apart well enough for a player id.
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
            bytes.copy_from_slice(&t.as_nanos().to_le_bytes());
        }
        let id = format!("webos-{}", bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
        let _ = self.set(keys::INSTALL_ID, &id);
        id
    }

    /// The server in use, if any.
    pub fn active_server(&self) -> Option<String> {
        self.get::<String>(keys::SERVER).filter(|s| crate::addresses::origin(s).as_deref() == Some(s.as_str()))
    }

    /// Makes `server` the one in use (or none).
    pub fn set_active_server(&self, server: Option<&str>) -> io::Result<()> {
        match server {
            Some(s) => self.set(keys::SERVER, s),
            None => self.remove(keys::SERVER),
        }
    }

    /// The servers this TV chose, most recently used first.
    pub fn servers(&self) -> Vec<SavedServer> {
        let mut list: Vec<SavedServer> = self.get(keys::SERVERS).unwrap_or_default();
        list.retain(|s| crate::addresses::origin(&s.url).as_deref() == Some(s.url.as_str()));
        list
    }

    /// Puts `url` first in the saved servers, replacing an entry with the same
    /// address or the same discovery id (a server found at a new address).
    pub fn remember_server(&self, url: &str, name: &str, id: Option<&str>, now_ms: u64) -> io::Result<()> {
        let url = crate::addresses::origin(url).unwrap_or_else(|| url.to_string());
        let mut list = self.servers();
        list.retain(|s| s.url != url && !(id.is_some() && s.id.as_deref() == id));
        list.insert(0, SavedServer { url, name: name.to_string(), id: id.map(str::to_string), last_used: now_ms });
        list.truncate(MAX_SERVERS);
        self.set(keys::SERVERS, &list)
    }

    /// Forgets everything kept for `server`: its entry, sign-in, household,
    /// last profile, library views, searches and Automatic's memory; it stops
    /// being the server in use. Its snapshots are [`Snapshots::forget_server`]'s.
    pub fn forget_server(&self, server: &str) -> io::Result<()> {
        let mut list = self.servers();
        list.retain(|s| s.url != server);
        self.set(keys::SERVERS, &list)?;
        self.remove_where(|k| keys::of_server(k, server));
        if self.active_server().as_deref() == Some(server) {
            self.set_active_server(None)?;
        }
        Ok(())
    }

    /// The household as last seen on `server`.
    pub fn household(&self, server: &str) -> Vec<AppProfile> {
        self.get(&keys::household(server)).unwrap_or_default()
    }

    /// Keeps the household as `server` lists it now.
    pub fn set_household(&self, server: &str, profiles: &[AppProfile]) -> io::Result<()> {
        self.set(&keys::household(server), profiles)
    }

    /// The profile last used on `server`.
    pub fn last_profile(&self, server: &str) -> Option<i64> {
        self.get(&keys::last_profile(server))
    }

    /// Remembers the profile last used on `server`.
    pub fn set_last_profile(&self, server: &str, profile: i64) -> io::Result<()> {
        self.set(&keys::last_profile(server), &profile)
    }

    /// Recent searches on `server` by `profile`, newest first.
    pub fn searches(&self, server: &str, profile: i64) -> Vec<String> {
        self.get(&keys::searches(server, profile)).unwrap_or_default()
    }

    /// Puts `query` first among the recent searches (once, whatever its case), keeping 10.
    pub fn remember_search(&self, server: &str, profile: i64, query: &str) -> io::Result<()> {
        let q = query.trim();
        if q.is_empty() {
            return Ok(());
        }
        let mut list = self.searches(server, profile);
        list.retain(|s| s.to_lowercase() != q.to_lowercase());
        list.insert(0, q.to_string());
        list.truncate(MAX_SEARCHES);
        self.set(&keys::searches(server, profile), &list)
    }

    /// Forgets the recent searches (Search's Clear).
    pub fn clear_searches(&self, server: &str, profile: i64) -> io::Result<()> {
        self.remove(&keys::searches(server, profile))
    }

    fn offset_key(server: &str, file_id: i64, profile: i64) -> String {
        format!("{server}/api/files/{file_id}#u{profile}")
    }

    /// The subtitle sync kept for a file and profile, in ms.
    pub fn subtitle_offset(&self, server: &str, file_id: i64, profile: i64) -> i64 {
        let key = Store::offset_key(server, file_id, profile);
        let list: Vec<(String, i64)> = self.get(keys::SUBTITLE_OFFSETS).unwrap_or_default();
        list.into_iter().find(|(k, _)| *k == key).map_or(0, |(_, ms)| ms)
    }

    /// Keeps a file's subtitle sync for a profile (0 forgets it), the newest 300.
    pub fn set_subtitle_offset(&self, server: &str, file_id: i64, profile: i64, ms: i64) -> io::Result<()> {
        let key = Store::offset_key(server, file_id, profile);
        let mut list: Vec<(String, i64)> = self.get(keys::SUBTITLE_OFFSETS).unwrap_or_default();
        list.retain(|(k, _)| *k != key);
        if ms != 0 {
            list.push((key, ms));
        }
        let extra = list.len().saturating_sub(MAX_SUBTITLE_OFFSETS);
        list.drain(..extra);
        self.set(keys::SUBTITLE_OFFSETS, &list)
    }

    /// Automatic's last step down on `server`, kbps, while it counts (30 minutes).
    pub fn auto_quality(&self, server: &str, now_ms: u64) -> Option<u32> {
        let m: AutoQualityMemory = self.get(&keys::auto_quality(server))?;
        (now_ms.saturating_sub(m.at) < AUTO_QUALITY_MS && m.kbps > 0).then_some(m.kbps)
    }

    /// Remembers Automatic's step down (none forgets it).
    pub fn set_auto_quality(&self, server: &str, kbps: Option<u32>, now_ms: u64) -> io::Result<()> {
        match kbps {
            Some(kbps) => self.set(&keys::auto_quality(server), &AutoQualityMemory { kbps, at: now_ms }),
            None => self.remove(&keys::auto_quality(server)),
        }
    }
}

fn valid_install_id(id: &str) -> bool {
    id.strip_prefix("webos-").is_some_and(|hex| hex.len() == 32 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
}

#[cfg(test)]
mod tests;

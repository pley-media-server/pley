//! The disk cache of picture bytes: one file per address, named by a hash of
//! it, under a byte budget with the least recently used files going first.
//!
//! Files are written to a temporary name and renamed, so a reader never sees
//! half a file; each starts with a small header (a check hash of the address
//! and the body's length), so a truncated or colliding file reads as a miss.
//! Addresses are only ever stored hashed (RES/plx-ui-gfx.md §6.5).

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

const MAGIC: &[u8; 4] = b"PLY1";
const HEADER: usize = 4 + 4 + 8 + 8;

/// A file is touched on disk at most this often when read, so recency
/// survives a restart without a write per read.
const TOUCH_EVERY: Duration = Duration::from_secs(3600);

/// The disk cache. Shared by the picture workers.
pub struct DiskCache {
    dir: PathBuf,
    budget: u64,
    index: Mutex<Option<Index>>,
    temp: AtomicU64,
}

#[derive(Default)]
struct Index {
    files: HashMap<String, Meta>,
    total: u64,
    clock: u64,
}

#[derive(Clone, Copy)]
struct Meta {
    size: u64,
    used: u64,
    touched: SystemTime,
}

impl DiskCache {
    /// A cache in `dir` (made on first write) holding at most `budget` bytes.
    pub fn new(dir: impl Into<PathBuf>, budget: u64) -> DiskCache {
        DiskCache { dir: dir.into(), budget, index: Mutex::new(None), temp: AtomicU64::new(0) }
    }

    /// The bytes stored for `url`, if any.
    pub fn get(&self, url: &str) -> Option<Vec<u8>> {
        let name = file_name(url);
        if !self.with_index(|i| i.files.contains_key(&name)) {
            return None;
        }
        let path = self.dir.join(&name);
        let body = fs::read(&path).ok().and_then(|data| parse(&data, check_hash(url)));
        let Some(body) = body else {
            self.forget(&name);
            let _ = fs::remove_file(&path);
            return None;
        };
        let now = SystemTime::now();
        let touch = self.with_index(|i| {
            i.clock += 1;
            let clock = i.clock;
            i.files.get_mut(&name).is_some_and(|m| {
                m.used = clock;
                let stale = now.duration_since(m.touched).is_ok_and(|age| age > TOUCH_EVERY);
                if stale {
                    m.touched = now;
                }
                stale
            })
        });
        if touch && let Ok(f) = fs::File::options().write(true).open(&path) {
            let _ = f.set_modified(now);
        }
        Some(body)
    }

    /// Stores `bytes` for `url`, then drops the least recently used files
    /// until the cache is within its budget. Failures leave the cache as it
    /// was: it is only a cache.
    pub fn put(&self, url: &str, bytes: &[u8]) {
        let size = (HEADER + bytes.len()) as u64;
        // One picture never takes more than a quarter of the cache.
        if size > self.budget / 4 {
            return;
        }
        let name = file_name(url);
        if self.write(&name, check_hash(url), bytes).is_err() {
            return;
        }
        let evict = self.with_index(|i| {
            i.clock += 1;
            let meta = Meta { size, used: i.clock, touched: SystemTime::now() };
            if let Some(old) = i.files.insert(name.clone(), meta) {
                i.total -= old.size;
            }
            i.total += size;
            if i.total <= self.budget {
                return Vec::new();
            }
            let mut by_age: Vec<(u64, String)> = i.files.iter().map(|(n, m)| (m.used, n.clone())).collect();
            by_age.sort_unstable();
            let mut gone = Vec::new();
            for (_, n) in by_age {
                if i.total <= self.budget {
                    break;
                }
                if let Some(m) = i.files.remove(&n) {
                    i.total -= m.size;
                    gone.push(n);
                }
            }
            gone
        });
        for n in evict {
            let _ = fs::remove_file(self.dir.join(n));
        }
    }

    /// Forgets `url` (its bytes didn't decode).
    pub fn remove(&self, url: &str) {
        let name = file_name(url);
        self.forget(&name);
        let _ = fs::remove_file(self.dir.join(name));
    }

    /// Deletes every file of the cache (signing out, "Delete all local data").
    pub fn clear(&self) {
        let names: Vec<String> = self.with_index(|i| {
            i.total = 0;
            i.files.drain().map(|(n, _)| n).collect()
        });
        for n in names {
            let _ = fs::remove_file(self.dir.join(n));
        }
    }

    /// Bytes the cache holds now.
    pub fn total(&self) -> u64 {
        self.with_index(|i| i.total)
    }

    fn forget(&self, name: &str) {
        self.with_index(|i| {
            if let Some(m) = i.files.remove(name) {
                i.total -= m.size;
            }
        });
    }

    fn write(&self, name: &str, check: u64, bytes: &[u8]) -> std::io::Result<()> {
        create_private_dir(&self.dir)?;
        let n = self.temp.fetch_add(1, Ordering::Relaxed);
        let temp = self.dir.join(format!("{name}.tmp{}-{n}", std::process::id()));
        let result = (|| {
            let mut f = private_file(&temp)?;
            f.write_all(MAGIC)?;
            f.write_all(&[0; 4])?;
            f.write_all(&check.to_le_bytes())?;
            f.write_all(&(bytes.len() as u64).to_le_bytes())?;
            f.write_all(bytes)?;
            drop(f);
            fs::rename(&temp, self.dir.join(name))
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    /// Runs `f` on the index, reading the folder the first time.
    fn with_index<R>(&self, f: impl FnOnce(&mut Index) -> R) -> R {
        let mut guard = self.index.lock().unwrap_or_else(|e| e.into_inner());
        let index = guard.get_or_insert_with(|| scan(&self.dir));
        f(index)
    }
}

/// Builds the index from the folder: recency from modification times; files
/// that aren't the cache's (temporary leftovers) are deleted.
fn scan(dir: &Path) -> Index {
    let mut index = Index::default();
    let Ok(entries) = fs::read_dir(dir) else { return index };
    let mut found: Vec<(SystemTime, String, u64)> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        if name.len() == 32 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
            found.push((meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), name, meta.len()));
        } else if name.contains(".tmp") {
            let _ = fs::remove_file(entry.path());
        }
    }
    found.sort();
    for (touched, name, size) in found {
        index.clock += 1;
        index.total += size;
        index.files.insert(name, Meta { size, used: index.clock, touched });
    }
    index
}

fn parse(data: &[u8], check: u64) -> Option<Vec<u8>> {
    if data.len() < HEADER || &data[..4] != MAGIC {
        return None;
    }
    let stored = u64::from_le_bytes(data[8..16].try_into().ok()?);
    let len = u64::from_le_bytes(data[16..24].try_into().ok()?);
    (stored == check && len == (data.len() - HEADER) as u64).then(|| data[HEADER..].to_vec())
}

#[cfg(unix)]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    if dir.is_dir() {
        return Ok(());
    }
    fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)
}

#[cfg(not(unix))]
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)
}

#[cfg(unix)]
fn private_file(path: &Path) -> std::io::Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    fs::File::options().write(true).create_new(true).mode(0o600).open(path)
}

#[cfg(not(unix))]
fn private_file(path: &Path) -> std::io::Result<fs::File> {
    fs::File::options().write(true).create_new(true).open(path)
}

/// 64-bit FNV-1a, its basis varied by `seed`, then mixed (SplitMix64's
/// finaliser) so every bit depends on every byte.
fn hash(seed: u64, bytes: &[u8]) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64 ^ seed.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h = (h ^ (h >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    h ^ (h >> 31)
}

fn file_name(url: &str) -> String {
    format!("{:016x}{:016x}", hash(1, url.as_bytes()), hash(2, url.as_bytes()))
}

fn check_hash(url: &str) -> u64 {
    hash(3, url.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pley-gfx-disk-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn stores_and_reads_back_by_address() {
        let dir = temp_dir("roundtrip");
        let cache = DiskCache::new(&dir, 1 << 20);
        assert_eq!(cache.get("http://a/1.jpg"), None);
        cache.put("http://a/1.jpg", b"one");
        cache.put("http://a/2.jpg", b"two");
        assert_eq!(cache.get("http://a/1.jpg").as_deref(), Some(&b"one"[..]));
        assert_eq!(cache.get("http://a/2.jpg").as_deref(), Some(&b"two"[..]));
        // Names are hashes: the address isn't on disk.
        for entry in fs::read_dir(&dir).unwrap() {
            let name = entry.unwrap().file_name().into_string().unwrap();
            assert_eq!(name.len(), 32);
            assert!(!name.contains("http"));
        }
        // A new cache over the same folder finds them.
        let again = DiskCache::new(&dir, 1 << 20);
        assert_eq!(again.get("http://a/2.jpg").as_deref(), Some(&b"two"[..]));
        assert_eq!(again.total(), 2 * HEADER as u64 + 6);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn evicts_the_least_recently_used_over_budget() {
        let dir = temp_dir("evict");
        let file = HEADER as u64 + 100;
        let cache = DiskCache::new(&dir, file * 4);
        let body = [7u8; 100];
        for n in 0..4 {
            cache.put(&format!("u{n}"), &body);
        }
        // Reading u0 makes u1 the oldest.
        assert!(cache.get("u0").is_some());
        cache.put("u4", &body);
        assert!(cache.get("u1").is_none());
        assert!(cache.get("u0").is_some());
        assert!(cache.get("u4").is_some());
        assert_eq!(cache.total(), file * 4);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 4);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_damaged_file_reads_as_a_miss_and_goes() {
        let dir = temp_dir("damaged");
        let cache = DiskCache::new(&dir, 1 << 20);
        cache.put("x", b"picture bytes");
        let path = dir.join(file_name("x"));
        let data = fs::read(&path).unwrap();
        fs::write(&path, &data[..data.len() - 3]).unwrap();
        assert_eq!(cache.get("x"), None);
        assert!(!path.exists());
        assert_eq!(cache.total(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn leftovers_are_swept_and_oversized_pictures_skipped() {
        let dir = temp_dir("sweep");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("0123.tmp9-1"), b"half").unwrap();
        let cache = DiskCache::new(&dir, 400);
        cache.put("big", &[0; 200]);
        assert_eq!(cache.get("big"), None);
        assert_eq!(cache.total(), 0);
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        cache.put("small", &[0; 10]);
        cache.clear();
        assert_eq!(fs::read_dir(&dir).unwrap().count(), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn names_and_checks_differ() {
        assert_ne!(file_name("a"), file_name("b"));
        assert_ne!(check_hash("a"), hash(1, b"a"));
        assert_eq!(file_name("a").len(), 32);
    }
}

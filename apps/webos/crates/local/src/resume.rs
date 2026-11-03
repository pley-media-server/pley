//! Where a video on a drive stopped, kept on the TV, since no server keeps
//! it. A drive's mount letters move (`sda1` one day, `sdb1` the next), so a
//! file is known by its path from the drive's top and its size, not by where
//! the drive is mounted. The store keeps one file per key, so every video's
//! point is in one bounded key, [`keys::LOCAL_RESUME`].

use std::io;
use std::path::{Component, Path};

use pley_core::Store;
use pley_core::store::keys;

/// Played this far through, it was watched: it starts at the beginning next
/// time.
pub const WATCHED: f64 = 0.95;
/// The most videos whose points are kept; the oldest go first.
pub const KEPT_MOST: usize = 500;

/// Where a video stopped and how long it is (0 when that wasn't known).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Kept {
    pub position_ms: i64,
    pub duration_ms: i64,
}

/// A stable id for a file, from its path from the drive's top and its size:
/// subtitle sync and where it stopped are kept by it. FNV-1a, 64 bits, kept
/// to 62 so it stays a positive JSON number.
pub fn file_id(relative: &Path, size: u64) -> i64 {
    let parts: Vec<String> = relative
        .components()
        .filter_map(|c| match c {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in parts.join("/").bytes().chain(size.to_le_bytes()) {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    (hash & 0x3fff_ffff_ffff_ffff) as i64
}

fn all(store: &Store) -> Vec<(i64, i64, i64)> {
    store.get(keys::LOCAL_RESUME).unwrap_or_default()
}

/// Where the file `id` stopped last; none when it wasn't kept, or was watched.
pub fn kept(store: &Store, id: i64) -> Option<Kept> {
    all(store)
        .into_iter()
        .find(|(i, position, _)| *i == id && *position > 0)
        .map(|(_, position_ms, duration_ms)| Kept { position_ms, duration_ms: duration_ms.max(0) })
}

/// Keeps where the file `id` stopped. At 95 % of its length or past it, it
/// was watched and is forgotten; a position of 0 changes nothing.
pub fn keep(store: &Store, id: i64, position_ms: i64, duration_ms: Option<i64>) -> io::Result<()> {
    let watched = duration_ms.is_some_and(|d| d > 0 && position_ms as f64 >= d as f64 * WATCHED);
    if !watched && position_ms <= 0 {
        return Ok(());
    }
    let mut list = all(store);
    let before = list.len();
    list.retain(|(i, _, _)| *i != id);
    if watched {
        if list.len() == before {
            return Ok(());
        }
    } else {
        list.insert(0, (id, position_ms, duration_ms.unwrap_or(0).max(0)));
        list.truncate(KEPT_MOST);
    }
    store.set(keys::LOCAL_RESUME, &list)
}

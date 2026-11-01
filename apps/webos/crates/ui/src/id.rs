//! Ids: what the interface calls each focusable element, group and animated value. They come
//! from the keys a screen draws (an item's id, a button's name) hashed within the scopes around
//! them, never from the order things are drawn in, so they stay the same from frame to frame and
//! when a page is drawn again after Back.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hash, Hasher};

/// A stable name for an element, a group or an animated value: a 64-bit hash of keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(u64);

impl Id {
    /// The scope every frame starts in.
    pub const ROOT: Id = Id(0x9e37_79b9_7f4a_7c15);

    /// The id of `key` at the top level: `Id::ROOT.with(key)`.
    pub fn new(key: impl Hash) -> Id {
        Id::ROOT.with(key)
    }

    /// The id of `key` inside this one, as a tile `key` is inside its row: what [`crate::Ui::id`]
    /// gives within this scope.
    pub fn with(self, key: impl Hash) -> Id {
        let mut h = Fnv(self.0);
        key.hash(&mut h);
        Id(mix(h.0))
    }

    /// The hash itself, for logs and tests.
    pub fn value(self) -> u64 {
        self.0
    }
}

/// FNV-1a: deterministic on every platform and run, unlike the standard library's hasher, and
/// fast on the short keys ids are made of.
struct Fnv(u64);

impl Hasher for Fnv {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// MurmurHash3's finaliser, so ids made from neighbouring keys spread over a map's buckets.
fn mix(mut h: u64) -> u64 {
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h = h.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    h ^ (h >> 33)
}

/// Ids are hashes already: maps keyed by them use the value as it is.
#[derive(Default)]
pub(crate) struct IdHasher(u64);

impl Hasher for IdHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 << 8) | b as u64;
        }
    }

    fn write_u64(&mut self, v: u64) {
        self.0 = v;
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

pub(crate) type IdMap<V> = HashMap<Id, V, BuildHasherDefault<IdHasher>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_keys_same_id() {
        assert_eq!(Id::new("play"), Id::new("play"));
        assert_eq!(Id::new("row").with(42u64), Id::new("row").with(42u64));
        assert_ne!(Id::new("row").with(42u64), Id::new("row").with(43u64));
        assert_ne!(Id::new("a").with("b"), Id::new("b").with("a"));
    }

    #[test]
    fn scopes_separate_equal_keys() {
        assert_ne!(Id::new("continue").with(7u64), Id::new("recent").with(7u64));
        assert_ne!(Id::new("x"), Id::ROOT);
    }
}

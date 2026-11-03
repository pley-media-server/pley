//! The layout cache: recent layouts by text, style and options, the least
//! recently used dropped first, bounded by count and by glyphs held.
//!
//! A hit costs one hash of the arguments, one map lookup, a comparison that
//! rules out a hash collision, and an `Arc` clone: the interface asks for every
//! string every frame it draws.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use pley_base::{TextLayout, TextOpts, TextStyle};

pub(crate) struct LayoutCache {
    entries: HashMap<u64, Entry, BuildHasherDefault<KeyHasher>>,
    clock: u64,
    glyphs: usize,
    max_entries: usize,
    max_glyphs: usize,
}

struct Entry {
    layout: Arc<TextLayout>,
    style: TextStyle,
    opts: TextOpts,
    used: u64,
}

impl LayoutCache {
    pub(crate) fn new(max_entries: usize, max_glyphs: usize) -> LayoutCache {
        LayoutCache {
            entries: HashMap::default(),
            clock: 0,
            glyphs: 0,
            max_entries: max_entries.max(1),
            max_glyphs: max_glyphs.max(1),
        }
    }

    pub(crate) fn key(text: &str, style: &TextStyle, opts: &TextOpts) -> u64 {
        let mut hasher = DefaultHasher::new();
        text.hash(&mut hasher);
        style.hash(&mut hasher);
        opts.hash(&mut hasher);
        hasher.finish()
    }

    pub(crate) fn get(&mut self, key: u64, text: &str, style: &TextStyle, opts: &TextOpts) -> Option<Arc<TextLayout>> {
        let entry = self.entries.get_mut(&key)?;
        if *entry.layout.text != *text || entry.style != *style || entry.opts != *opts {
            return None;
        }
        self.clock += 1;
        entry.used = self.clock;
        Some(entry.layout.clone())
    }

    pub(crate) fn insert(&mut self, key: u64, style: &TextStyle, opts: &TextOpts, layout: Arc<TextLayout>) {
        let glyphs = layout.glyphs.len();
        if self.entries.len() >= self.max_entries || self.glyphs + glyphs > self.max_glyphs {
            self.evict(glyphs);
        }
        self.clock += 1;
        let entry = Entry { layout, style: style.clone(), opts: *opts, used: self.clock };
        if let Some(old) = self.entries.insert(key, entry) {
            self.glyphs -= old.layout.glyphs.len();
        }
        self.glyphs += glyphs;
    }

    /// Drops the least recently used layouts until a quarter of the room is
    /// free again (so eviction runs rarely), or everything when one layout
    /// is that large (the newest is always kept).
    fn evict(&mut self, incoming: usize) {
        let mut by_age: Vec<(u64, u64)> = self.entries.iter().map(|(&key, e)| (e.used, key)).collect();
        by_age.sort_unstable();
        let (entries, glyphs) = (self.max_entries * 3 / 4, self.max_glyphs * 3 / 4);
        for (_, key) in by_age {
            if self.entries.len() <= entries && self.glyphs + incoming <= glyphs {
                break;
            }
            if let Some(old) = self.entries.remove(&key) {
                self.glyphs -= old.layout.glyphs.len();
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.glyphs = 0;
    }
}

/// Keys are already hashes; the map uses them as they are.
#[derive(Default)]
struct KeyHasher(u64);

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn write_u64(&mut self, n: u64) {
        self.0 = n;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pley_base::Weight;

    fn layout(text: &str, glyphs: usize) -> Arc<TextLayout> {
        let glyph = pley_base::Glyph { font: 0, id: 1, x: 0.0, y: 0.0, size: 14.0 };
        Arc::new(TextLayout { text: Arc::from(text), glyphs: vec![glyph; glyphs], ..TextLayout::default() })
    }

    #[test]
    fn least_recently_used_goes_first() {
        let style = TextStyle::new(14.0, 20.0, Weight::Regular);
        let opts = TextOpts::default();
        let mut cache = LayoutCache::new(4, 1000);
        let keys: Vec<u64> = ["a", "b", "c", "d"]
            .iter()
            .map(|t| {
                let key = LayoutCache::key(t, &style, &opts);
                cache.insert(key, &style, &opts, layout(t, 1));
                key
            })
            .collect();
        // "a" is used again, so "b" is the oldest when "e" comes.
        assert!(cache.get(keys[0], "a", &style, &opts).is_some());
        let e = LayoutCache::key("e", &style, &opts);
        cache.insert(e, &style, &opts, layout("e", 1));
        assert!(cache.get(keys[0], "a", &style, &opts).is_some());
        assert!(cache.get(keys[1], "b", &style, &opts).is_none());
        assert!(cache.get(e, "e", &style, &opts).is_some());
        assert!(cache.len() <= 4);
    }

    #[test]
    fn glyphs_are_bounded_and_a_large_layout_is_still_kept() {
        let style = TextStyle::new(14.0, 20.0, Weight::Regular);
        let opts = TextOpts::default();
        let mut cache = LayoutCache::new(100, 100);
        for t in ["a", "b", "c"] {
            cache.insert(LayoutCache::key(t, &style, &opts), &style, &opts, layout(t, 40));
        }
        assert!(cache.glyphs <= 100);
        let big = LayoutCache::key("big", &style, &opts);
        cache.insert(big, &style, &opts, layout("big", 500));
        assert!(cache.get(big, "big", &style, &opts).is_some());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn a_colliding_key_is_a_miss() {
        let style = TextStyle::new(14.0, 20.0, Weight::Regular);
        let opts = TextOpts::default();
        let mut cache = LayoutCache::new(10, 100);
        cache.insert(7, &style, &opts, layout("a", 1));
        assert!(cache.get(7, "b", &style, &opts).is_none());
        assert!(cache.get(7, "a", &style, &TextOpts::line(10.0)).is_none());
    }
}

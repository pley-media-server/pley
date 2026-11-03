//! The alpha atlas: one 8-bit page holding every glyph, icon mask and corner
//! mask the frame draws, packed on shelves and kept in a CPU copy whose
//! changed rows are uploaded once per draw.
//!
//! When the page is full it is emptied and refilled with what the frames go
//! on to draw (a page of 2048² is enough for a 1080p interface: 4 MiB).

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use pley_base::Icon;

/// The page's side in texels unless asked otherwise.
pub const DEFAULT_SIDE: u32 = 2048;

/// A solid block kept at the page's top-left corner: fills and gradients
/// sample its middle, so they share a batch with text and icons.
const WHITE_SIDE: u32 = 4;

/// What an atlas entry holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AtlasKey {
    /// A glyph at a size (sixteenths of a device pixel) and a horizontal
    /// phase (quarters of a pixel).
    Glyph { font: u16, glyph: u16, size: u32, phase: u8 },
    /// An icon `px` device pixels square with a stroke in hundredths of a grid
    /// unit.
    Icon { icon: Icon, px: u16, stroke: u16 },
    /// A corner piece (quarters of a pixel): its extent, the outer radius and
    /// an outline's width (0 when filled).
    Corner { extent: u16, radius: u16, width: u16 },
}

/// Hashes atlas keys, a few small numbers looked up for every glyph of every frame, with a
/// multiply and a rotate per number (FxHash's mixing) rather than the standard map's
/// SipHash, which guards against keys chosen by an attacker that these never are.
#[derive(Default)]
struct KeyHasher(u64);

impl KeyHasher {
    fn add(&mut self, n: u64) {
        self.0 = (self.0.rotate_left(5) ^ n).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
}

impl Hasher for KeyHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.add(u64::from(b));
        }
    }

    fn write_u8(&mut self, n: u8) {
        self.add(u64::from(n));
    }

    fn write_u16(&mut self, n: u16) {
        self.add(u64::from(n));
    }

    fn write_u32(&mut self, n: u32) {
        self.add(u64::from(n));
    }

    fn write_u64(&mut self, n: u64) {
        self.add(n);
    }

    fn write_usize(&mut self, n: usize) {
        self.add(n as u64);
    }
}

/// Where an entry's texels are, and for glyphs where they go relative to the
/// pen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AtlasEntry {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub left: i16,
    pub top: i16,
}

/// The page is full: the frame must draw what it has, then [`Atlas::reset`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasFull;

#[derive(Clone, Copy, Debug)]
struct Shelf {
    y: u32,
    height: u32,
    next_x: u32,
}

/// The atlas page and what is in it.
pub struct Atlas {
    side: u32,
    pixels: Vec<u8>,
    shelves: Vec<Shelf>,
    next_y: u32,
    entries: HashMap<AtlasKey, Option<AtlasEntry>, BuildHasherDefault<KeyHasher>>,
    /// Bands of rows changed since the last upload, apart from each other.
    dirty: Vec<(u32, u32)>,
    /// Bumped by every reset, so the GPU copy is known to be stale.
    generation: u64,
    /// Entries added since the last reset.
    added: usize,
}

impl Atlas {
    /// An empty page `side` texels square (at least 64).
    pub fn new(side: u32) -> Atlas {
        let side = side.max(64);
        let mut atlas = Atlas {
            side,
            pixels: vec![0; (side * side) as usize],
            shelves: Vec::new(),
            next_y: 0,
            entries: HashMap::default(),
            dirty: Vec::new(),
            generation: 0,
            added: 0,
        };
        atlas.reset();
        atlas
    }

    /// The page's side in texels.
    pub fn side(&self) -> u32 {
        self.side
    }

    /// Empties the page, keeping only the solid block.
    pub fn reset(&mut self) {
        self.pixels.fill(0);
        self.shelves.clear();
        self.next_y = 0;
        self.entries.clear();
        self.generation += 1;
        self.added = 0;
        // Only shelves in use are ever sampled (an entry and its clear border
        // lie within its shelf), so stale texels elsewhere on the GPU's copy
        // never show: only the shelves written from now on are uploaded.
        self.dirty.clear();
        let white = self.allocate(WHITE_SIDE, WHITE_SIDE).expect("an empty page holds the solid block");
        self.write(white, &[255; (WHITE_SIDE * WHITE_SIDE) as usize]);
    }

    /// How many resets the page has had.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether nothing but the solid block is on the page: an entry that
    /// doesn't fit now never will.
    pub fn is_fresh(&self) -> bool {
        self.added == 0
    }

    /// The texture coordinate of the solid block's middle.
    pub fn white_uv(&self) -> [f32; 2] {
        let c = (1 + WHITE_SIDE / 2) as f32 / self.side as f32;
        [c, c]
    }

    /// The entry for `key` if it is on the page (`Some(None)` for something
    /// known to draw nothing, like a space).
    pub fn get(&self, key: &AtlasKey) -> Option<Option<AtlasEntry>> {
        self.entries.get(key).copied()
    }

    /// Records that `key` draws nothing.
    pub fn insert_empty(&mut self, key: AtlasKey) {
        self.entries.insert(key, None);
    }

    /// Puts `w` × `h` coverage bytes on the page under `key`, with the glyph
    /// offsets `left` and `top`.
    pub fn insert(
        &mut self,
        key: AtlasKey,
        w: u32,
        h: u32,
        left: i32,
        top: i32,
        coverage: &[u8],
    ) -> Result<AtlasEntry, AtlasFull> {
        debug_assert_eq!(coverage.len(), (w * h) as usize);
        let mut entry = self.allocate(w, h).ok_or(AtlasFull)?;
        entry.left = left.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        entry.top = top.clamp(i16::MIN as i32, i16::MAX as i32) as i16;
        self.write(entry, coverage);
        self.entries.insert(key, Some(entry));
        self.added += 1;
        Ok(entry)
    }

    /// A free spot for `w` × `h` texels with an empty texel all around, on the
    /// first shelf that fits without wasting much height, else on a new one.
    fn allocate(&mut self, w: u32, h: u32) -> Option<AtlasEntry> {
        let (cw, ch) = (w + 2, h + 2);
        if cw > self.side || ch > self.side {
            return None;
        }
        let side = self.side;
        let fits = |s: &Shelf| s.height >= ch && s.height <= ch + ch / 4 + 4 && s.next_x + cw <= side;
        let shelf = match self.shelves.iter().position(fits) {
            Some(i) => i,
            None => {
                // Heights rounded up to 8 let later entries of about the same
                // size share the shelf.
                let height = (ch + 7) & !7;
                if self.next_y + height > side {
                    return None;
                }
                self.shelves.push(Shelf { y: self.next_y, height, next_x: 0 });
                self.next_y += height;
                self.shelves.len() - 1
            }
        };
        let s = &mut self.shelves[shelf];
        let entry =
            AtlasEntry { x: (s.next_x + 1) as u16, y: (s.y + 1) as u16, w: w as u16, h: h as u16, left: 0, top: 0 };
        s.next_x += cw;
        let rows = (s.y, s.y + s.height);
        self.mark_dirty(rows.0, rows.1);
        Some(entry)
    }

    fn write(&mut self, e: AtlasEntry, coverage: &[u8]) {
        let side = self.side as usize;
        for row in 0..e.h as usize {
            let start = (e.y as usize + row) * side + e.x as usize;
            self.pixels[start..start + e.w as usize].copy_from_slice(&coverage[row * e.w as usize..][..e.w as usize]);
        }
    }

    fn mark_dirty(&mut self, from: u32, to: u32) {
        let (mut from, mut to) = (from, to);
        // Bands that touch merge, so each upload is one contiguous run of rows.
        self.dirty.retain(|&(a, b)| {
            let touches = a <= to && from <= b;
            if touches {
                from = from.min(a);
                to = to.max(b);
            }
            !touches
        });
        self.dirty.push((from, to.min(self.side)));
    }

    /// The bands of rows changed since the last call (first row, end row), one
    /// texture upload each.
    pub fn take_dirty(&mut self) -> Vec<(u32, u32)> {
        let mut bands = std::mem::take(&mut self.dirty);
        bands.sort_unstable();
        bands
    }

    /// The bytes of rows `from` to `to`, whole rows.
    pub fn rows(&self, from: u32, to: u32) -> &[u8] {
        let side = self.side as usize;
        &self.pixels[from as usize * side..to as usize * side]
    }

    /// The texture coordinates of an entry's texel position (`tx`, `ty`), which
    /// may be fractional and run past its edges.
    pub fn uv(&self, e: &AtlasEntry, tx: f32, ty: f32) -> [f32; 2] {
        let s = self.side as f32;
        [(e.x as f32 + tx) / s, (e.y as f32 + ty) / s]
    }

    /// The coverage byte at a texel (for tests).
    #[cfg(test)]
    pub fn texel(&self, x: u32, y: u32) -> u8 {
        self.pixels[(y * self.side + x) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glyph(n: u16) -> AtlasKey {
        AtlasKey::Glyph { font: 0, glyph: n, size: 16 * 16, phase: 0 }
    }

    #[test]
    fn a_fresh_page_has_its_solid_block_and_uploads_its_shelf() {
        let mut atlas = Atlas::new(256);
        assert!(atlas.is_fresh());
        let [u, v] = atlas.white_uv();
        let (x, y) = ((u * 256.0) as u32, (v * 256.0) as u32);
        // The sample point sits between solid texels.
        assert_eq!(atlas.texel(x, y), 255);
        assert_eq!(atlas.texel(x - 1, y - 1), 255);
        assert_eq!(atlas.take_dirty(), vec![(0, 8)]);
        assert_eq!(atlas.rows(0, 8).len(), 8 * 256);
        assert!(atlas.take_dirty().is_empty());
    }

    #[test]
    fn entries_keep_a_clear_texel_around_them() {
        let mut atlas = Atlas::new(256);
        let a = atlas.insert(glyph(1), 3, 2, 1, -9, &[9; 6]).unwrap();
        let b = atlas.insert(glyph(2), 3, 2, 0, 0, &[7; 6]).unwrap();
        assert_eq!((a.left, a.top), (1, -9));
        assert_eq!(atlas.texel(a.x as u32, a.y as u32), 9);
        assert_eq!(atlas.texel(a.x as u32 + 3, a.y as u32), 0);
        assert_eq!(atlas.texel(a.x as u32, a.y as u32 + 2), 0);
        assert!(b.x >= a.x + a.w + 2 || b.y >= a.y + a.h + 2);
        assert_eq!(atlas.get(&glyph(1)), Some(Some(a)));
        assert_eq!(atlas.get(&glyph(3)), None);
        assert!(!atlas.is_fresh());
    }

    #[test]
    fn dirty_rows_cover_new_entries_only() {
        let mut atlas = Atlas::new(256);
        atlas.take_dirty();
        let e = atlas.insert(glyph(1), 10, 10, 0, 0, &[1; 100]).unwrap();
        let bands = atlas.take_dirty();
        assert_eq!(bands.len(), 1);
        let (from, to) = bands[0];
        // The band covers the entry and its clear texel above and below.
        assert!(from < e.y as u32 && to > (e.y + e.h) as u32);
        assert!(to - from < 64);
    }

    #[test]
    fn distant_shelves_upload_as_separate_bands() {
        let mut atlas = Atlas::new(256);
        atlas.insert(glyph(1), 10, 10, 0, 0, &[1; 100]).unwrap();
        atlas.insert(glyph(2), 10, 100, 0, 0, &[1; 1000]).unwrap();
        atlas.take_dirty();
        // A small entry back on the first shelf, a tall one on a new shelf below.
        atlas.insert(glyph(3), 10, 10, 0, 0, &[1; 100]).unwrap();
        atlas.insert(glyph(4), 10, 60, 0, 0, &[1; 600]).unwrap();
        let bands = atlas.take_dirty();
        assert_eq!(bands.len(), 2);
        assert!(bands[0].1 < bands[1].0);
    }

    #[test]
    fn a_full_page_refuses_then_starts_over() {
        let mut atlas = Atlas::new(64);
        let mut n = 0;
        while atlas.insert(glyph(n), 14, 14, 0, 0, &[1; 196]).is_ok() {
            n += 1;
        }
        // The solid block's shelf takes 8 rows; three shelves of four 16-texel
        // cells fit in the other 56.
        assert_eq!(n, 12);
        let generation = atlas.generation();
        atlas.reset();
        assert_eq!(atlas.generation(), generation + 1);
        assert!(atlas.is_fresh());
        assert_eq!(atlas.get(&glyph(0)), None);
        assert!(atlas.insert(glyph(0), 14, 14, 0, 0, &[1; 196]).is_ok());
    }

    #[test]
    fn similar_heights_share_a_shelf() {
        let mut atlas = Atlas::new(256);
        let a = atlas.insert(glyph(1), 5, 12, 0, 0, &[1; 60]).unwrap();
        let b = atlas.insert(glyph(2), 5, 10, 0, 0, &[1; 50]).unwrap();
        let c = atlas.insert(glyph(3), 5, 40, 0, 0, &[1; 200]).unwrap();
        assert_eq!(a.y, b.y);
        assert_ne!(a.y, c.y);
    }

    #[test]
    fn too_big_never_fits() {
        let mut atlas = Atlas::new(64);
        assert_eq!(atlas.insert(glyph(1), 64, 1, 0, 0, &[1; 64]), Err(AtlasFull));
    }
}

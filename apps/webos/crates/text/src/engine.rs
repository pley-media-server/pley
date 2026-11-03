//! The text engine: lays strings out for the interface, cached, and rasterises
//! the glyphs of those layouts for the renderer.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use pley_base::{Family, TextLayout, TextOpts, TextShaper, TextStyle, Weight};

use crate::cache::LayoutCache;
use crate::fonts::{Chain, FontSet, SYNTHETIC_ITALIC, lock};
use crate::layout;
use crate::raster::{self, GlyphBitmap};

/// Layouts kept: every string of the busiest screens several times over (the
/// item page draws about a hundred).
const CACHED_LAYOUTS: usize = 4096;
/// Glyphs kept across those layouts: 4 MiB of placed glyphs, so a long text
/// (a licence) can't hold memory without bound.
const CACHED_GLYPHS: usize = 256 * 1024;

/// Lays text out and rasterises its glyphs; shared by the interface (as its
/// [`TextShaper`]) and the renderer. It is `Send + Sync`: layouts and faces are
/// cached behind locks held only briefly.
pub struct TextEngine {
    fonts: FontSet,
    chains: Mutex<HashMap<(Family, Weight, bool), Arc<Chain>>>,
    cache: Mutex<LayoutCache>,
}

/// A style's vertical metrics in dp, from its first face.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineMetrics {
    /// Above the baseline and below it (both positive), as the face's line
    /// metrics give them.
    pub ascent: f32,
    pub descent: f32,
    /// How high a capital and a lower-case x stand above the baseline.
    pub cap_height: f32,
    pub x_height: f32,
    /// Where the baseline sits in a line box of the style's `line_height`.
    pub baseline: f32,
}

impl TextEngine {
    /// An engine over `fonts`, keeping the layouts of several screens.
    pub fn new(fonts: FontSet) -> TextEngine {
        TextEngine::with_cache(fonts, CACHED_LAYOUTS, CACHED_GLYPHS)
    }

    /// An engine keeping at most `layouts` layouts holding at most `glyphs`
    /// glyphs between them.
    pub fn with_cache(fonts: FontSet, layouts: usize, glyphs: usize) -> TextEngine {
        TextEngine { fonts, chains: Mutex::new(HashMap::new()), cache: Mutex::new(LayoutCache::new(layouts, glyphs)) }
    }

    /// The faces it draws with (what a glyph's face is, which families exist).
    pub fn fonts(&self) -> &FontSet {
        &self.fonts
    }

    /// The vertical metrics of a style, for lining up icons and text.
    pub fn metrics(&self, style: &TextStyle) -> LineMetrics {
        let line = layout::line_box(&self.chain(style), style);
        LineMetrics {
            ascent: line.ascent,
            descent: line.descent,
            cap_height: line.cap_height,
            x_height: line.x_height,
            baseline: line.baseline,
        }
    }

    /// The coverage of glyph `glyph` of `font` (a layout glyph's `font`, with
    /// its synthetic-italic flag) at `size_px` device pixels per em (the
    /// glyph's `size` times the screen's scale), its pen `subpixel_x` pixels
    /// right of a whole pixel (see [`crate::subpixel_position`]). `None` for a
    /// glyph without ink, such as a space.
    pub fn rasterize(&self, font: u16, glyph: u16, size_px: f32, subpixel_x: f32) -> Option<GlyphBitmap> {
        let face = self.fonts.face(font)?;
        raster::rasterize(&face, glyph, size_px, subpixel_x, font & SYNTHETIC_ITALIC != 0)
    }

    /// Forgets every cached layout (the TV asked the app to free memory).
    pub fn clear_cache(&self) {
        lock(&self.cache).clear();
    }

    fn chain(&self, style: &TextStyle) -> Arc<Chain> {
        let key = (style.family.clone(), style.weight, style.italic);
        if let Some(chain) = lock(&self.chains).get(&key) {
            return chain.clone();
        }
        // Built outside the lock: a family asked for by name may index the system's fonts.
        let chain = Arc::new(self.fonts.chain(&style.family, style.weight, style.italic));
        lock(&self.chains).entry(key).or_insert(chain).clone()
    }

    #[cfg(test)]
    pub(crate) fn cached_layouts(&self) -> usize {
        lock(&self.cache).len()
    }
}

impl TextShaper for TextEngine {
    fn layout(&self, text: &str, style: &TextStyle, opts: &TextOpts) -> Arc<TextLayout> {
        let key = LayoutCache::key(text, style, opts);
        if let Some(layout) = lock(&self.cache).get(key, text, style, opts) {
            return layout;
        }
        let layout = Arc::new(layout::lay_out(text, style, opts, &self.chain(style), &self.fonts));
        lock(&self.cache).insert(key, style, opts, layout.clone());
        layout
    }
}

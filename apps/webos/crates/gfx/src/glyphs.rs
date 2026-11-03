//! Where the renderer gets glyph pictures: `pley-text`'s rasteriser, behind a
//! trait so frames can be built and tested without fonts.

/// A glyph's coverage at device pixels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GlyphMask {
    pub width: u32,
    pub height: u32,
    /// From the pen position on the baseline to the bitmap's top-left corner,
    /// in device pixels, y down.
    pub left: i32,
    pub top: i32,
    /// One byte a pixel, rows top to bottom.
    pub coverage: Vec<u8>,
}

/// Rasterises glyphs for the atlas.
pub trait GlyphSource: Send + Sync {
    /// `glyph` of `font` at `size_px`, shifted right by `subpixel_x` (0, ¼, ½
    /// or ¾ of a pixel); `None` when it draws nothing.
    fn rasterize(&self, font: u16, glyph: u16, size_px: f32, subpixel_x: f32) -> Option<GlyphMask>;
}

impl GlyphSource for pley_text::TextEngine {
    fn rasterize(&self, font: u16, glyph: u16, size_px: f32, subpixel_x: f32) -> Option<GlyphMask> {
        let b = pley_text::TextEngine::rasterize(self, font, glyph, size_px, subpixel_x)?;
        Some(GlyphMask { width: b.width, height: b.height, left: b.left, top: b.top, coverage: b.coverage })
    }
}

/// No glyphs at all: text draws nothing (headless tools, tests).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoGlyphs;

impl GlyphSource for NoGlyphs {
    fn rasterize(&self, _font: u16, _glyph: u16, _size_px: f32, _subpixel_x: f32) -> Option<GlyphMask> {
        None
    }
}
